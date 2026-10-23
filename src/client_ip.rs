//! Resolve HTTP client addresses without trusting forwarding headers from arbitrary peers.

use std::{
    io,
    net::{IpAddr, SocketAddr},
};

use axum::{http::HeaderMap, serve::Listener};
use serde::Serialize;
use tokio::net::TcpStream;

use crate::proxy_protocol::{PrefixedStream, ProxyProtocolListener};

/// Keep the transport peer even when PPv2 replaces the logical client address.
#[derive(Clone, Copy, Debug)]
pub struct ClientConnection {
    pub transport_peer: Option<SocketAddr>,
    pub client_addr: SocketAddr,
}

pub struct ClientIpListener(pub ProxyProtocolListener);

impl Listener for ClientIpListener {
    type Io = PrefixedStream<TcpStream>;
    type Addr = ClientConnection;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (stream, client_addr) = self.0.accept().await;
        let transport_peer = stream.get_ref().peer_addr().ok();
        (
            stream,
            ClientConnection {
                transport_peer,
                client_addr,
            },
        )
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.0.local_addr().map(|addr| ClientConnection {
            transport_peer: Some(addr),
            client_addr: addr,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ClientAddress {
    pub ip: IpAddr,
    // A forwarded HTTP address does not provide the browser's TCP port.
    pub port: Option<u16>,
    pub source: &'static str,
}

fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    }
}

fn trusted(ip: IpAddr, proxies: &[IpAddr]) -> bool {
    proxies
        .iter()
        .any(|proxy| normalize(*proxy) == normalize(ip))
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ()> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    value.to_str().map(Some).map_err(|_| ())
}

pub fn resolve(
    connection: ClientConnection,
    headers: &HeaderMap,
    proxies: &[IpAddr],
) -> ClientAddress {
    let fallback = ClientAddress {
        ip: normalize(connection.client_addr.ip()),
        port: Some(connection.client_addr.port()),
        source: if connection
            .transport_peer
            .is_some_and(|peer| peer != connection.client_addr)
        {
            "proxy_protocol"
        } else {
            "socket"
        },
    };
    // Check both layers: a client must not gain HTTP-header trust by claiming a
    // trusted loopback address inside a PPv2 header sent over an untrusted socket.
    if !connection
        .transport_peer
        .is_some_and(|peer| trusted(peer.ip(), proxies))
        || !trusted(connection.client_addr.ip(), proxies)
    {
        return fallback;
    }

    match single_header(headers, "x-forwarded-for") {
        Ok(Some(value)) => {
            if value.len() > 4096 {
                return fallback;
            }
            let mut last = None;
            // Inspect the trusted suffix only; attacker-controlled entries on
            // the left cannot override the first untrusted hop on the right.
            for (index, hop) in value.rsplit(',').enumerate() {
                if index >= 32 {
                    return fallback;
                }
                let Ok(ip) = hop.trim().parse::<IpAddr>() else {
                    return fallback;
                };
                let ip = normalize(ip);
                last = Some(ip);
                if !trusted(ip, proxies) {
                    return ClientAddress {
                        ip,
                        port: None,
                        source: "x_forwarded_for",
                    };
                }
            }
            if let Some(ip) = last {
                return ClientAddress {
                    ip,
                    port: None,
                    source: "x_forwarded_for",
                };
            }
        }
        Err(()) => return fallback,
        Ok(None) => {
            if let Ok(Some(value)) = single_header(headers, "x-real-ip")
                && let Ok(ip) = value.trim().parse::<IpAddr>()
            {
                return ClientAddress {
                    ip: normalize(ip),
                    port: None,
                    source: "x_real_ip",
                };
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn connection(transport: &str, client: &str) -> ClientConnection {
        ClientConnection {
            transport_peer: Some(transport.parse().unwrap()),
            client_addr: client.parse().unwrap(),
        }
    }

    fn proxies() -> Vec<IpAddr> {
        ["192.168.1.2", "127.0.0.1", "::1"]
            .map(|ip| ip.parse().unwrap())
            .to_vec()
    }

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", value.parse().unwrap());
        headers
    }

    #[test]
    fn explicit_trust_is_required_for_both_transport_and_ppv2_peer() {
        let headers = headers("203.0.113.10");
        let tunneled = connection("192.168.1.2:1234", "127.0.0.1:5555");
        assert_eq!(
            resolve(tunneled, &headers, &[]).ip,
            tunneled.client_addr.ip()
        );
        let spoofed = connection("198.51.100.20:1234", "127.0.0.1:5555");
        assert_eq!(
            resolve(spoofed, &headers, &proxies()).source,
            "proxy_protocol"
        );
        let external = connection("192.168.1.2:1234", "198.51.100.20:5555");
        assert_eq!(
            resolve(external, &headers, &proxies()).ip,
            external.client_addr.ip()
        );
        let direct = connection("198.51.100.20:1234", "198.51.100.20:1234");
        assert_eq!(resolve(direct, &headers, &proxies()).source, "socket");
    }

    #[test]
    fn forwarded_chain_uses_first_untrusted_hop_from_the_right() {
        let conn = connection("192.168.1.2:1234", "127.0.0.1:5555");
        for value in [
            "203.0.113.10",
            "198.51.100.99, 203.0.113.10",
            "invalid, 203.0.113.10, 127.0.0.1",
        ] {
            let address = resolve(conn, &headers(value), &proxies());
            assert_eq!(address.ip, "203.0.113.10".parse::<IpAddr>().unwrap());
            assert_eq!(address.port, None);
            assert_eq!(address.source, "x_forwarded_for");
        }
    }

    #[test]
    fn direct_http_proxy_and_ipv6_addresses_are_supported() {
        let conn = connection("127.0.0.1:1234", "127.0.0.1:1234");
        assert_eq!(
            resolve(conn, &headers("2001:db8::10, ::1"), &proxies()).ip,
            "2001:db8::10".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            resolve(conn, &headers("::ffff:203.0.113.10"), &proxies()).ip,
            "203.0.113.10".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn invalid_ambiguous_and_oversized_forwarding_headers_fail_closed() {
        let conn = connection("192.168.1.2:1234", "127.0.0.1:5555");
        for value in [
            "",
            "unknown",
            "203.0.113.10:1234",
            "203.0.113.10,",
            "203.0.113.10, invalid",
        ] {
            let mut headers = headers(value);
            headers.insert("x-real-ip", HeaderValue::from_static("198.51.100.99"));
            assert_eq!(resolve(conn, &headers, &proxies()).source, "proxy_protocol");
        }
        let mut duplicate = headers("203.0.113.10");
        duplicate.append("x-forwarded-for", HeaderValue::from_static("198.51.100.99"));
        assert_eq!(
            resolve(conn, &duplicate, &proxies()).source,
            "proxy_protocol"
        );
        let long = "127.0.0.1,".repeat(500) + "127.0.0.1";
        assert_eq!(
            resolve(conn, &headers(&long), &proxies()).source,
            "proxy_protocol"
        );
        let many = "127.0.0.1,".repeat(33) + "127.0.0.1";
        assert_eq!(
            resolve(conn, &headers(&many), &proxies()).source,
            "proxy_protocol"
        );
    }

    #[test]
    fn real_ip_is_only_a_fallback_when_forwarded_for_is_absent() {
        let conn = connection("192.168.1.2:1234", "127.0.0.1:5555");
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.10"));
        assert_eq!(resolve(conn, &headers, &proxies()).source, "x_real_ip");
        headers.append("x-real-ip", HeaderValue::from_static("198.51.100.99"));
        assert_eq!(resolve(conn, &headers, &proxies()).source, "proxy_protocol");
    }

    #[test]
    fn missing_peer_information_never_enables_header_trust() {
        let conn = ClientConnection {
            transport_peer: None,
            client_addr: "127.0.0.1:5555".parse().unwrap(),
        };
        let address = resolve(conn, &headers("203.0.113.10"), &proxies());
        assert_eq!(address.source, "socket");
        assert_eq!(address.port, Some(5555));
    }

    #[tokio::test]
    async fn axum_preserves_transport_peer_through_ppv2_and_resolves_trusted_headers() {
        use crate::proxy_protocol::{PPV2_SIGNATURE, ProxyProtocolMode};
        use axum::{Json, Router, extract::ConnectInfo, routing::get, serve::ListenerExt};
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let listener = ClientIpListener(ProxyProtocolListener::new(
            listener,
            ProxyProtocolMode::Auto,
        ));
        let app = Router::new().route("/", get(
            |ConnectInfo(connection): ConnectInfo<ClientConnection>, headers: HeaderMap| async move {
                assert!(connection.transport_peer.unwrap().ip().is_loopback());
                Json(resolve(connection, &headers, &["127.0.0.1".parse().unwrap()]))
            }
        ));
        let server = tokio::spawn(async move {
            axum::serve(
                listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<ClientConnection>(),
            )
            .await
            .unwrap();
        });
        for (source, expected) in [
            ([127, 0, 0, 1], "203.0.113.10"),
            ([198, 51, 100, 20], "198.51.100.20"),
        ] {
            let mut stream = TcpStream::connect(address).await.unwrap();
            let mut packet = PPV2_SIGNATURE.to_vec();
            packet.extend_from_slice(&[0x21, 0x11, 0, 12]);
            packet.extend_from_slice(&source);
            packet.extend_from_slice(&[127, 0, 0, 1]);
            packet.extend_from_slice(&53000u16.to_be_bytes());
            packet.extend_from_slice(&address.port().to_be_bytes());
            packet.extend_from_slice(b"GET / HTTP/1.1\r\nHost: kvm.local\r\nX-Forwarded-For: 198.51.100.99, 203.0.113.10\r\nConnection: close\r\n\r\n");
            stream.write_all(&packet).await.unwrap();
            let mut response = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                stream.read_to_string(&mut response),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                response.contains(&format!("\"ip\":\"{expected}\"")),
                "{response}"
            );
        }
        server.abort();
    }
}
