use std::{
    cmp,
    io::{self, Cursor},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::serve::Listener;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::watch,
};

/// 12-byte fixed signature required at the start of every PROXY protocol v2 header.
/// \x0D \x0A \x0D \x0A \x00 \x0D \x0A \x51 \x55 \x49 \x54 \x0A
pub const PPV2_SIGNATURE: [u8; 12] = [
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

/// Maximum allowed length of a PROXY protocol v2 header (header + addresses + TLVs).
/// This prevents memory exhaustion from malicious or malformed length fields.
pub const MAX_PPV2_HEADER_SIZE: usize = 4096;

/// Timeout waiting for the full PROXY protocol v2 header to arrive.
pub const PPV2_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum ProxyProtocolError {
    #[error("I/O error during proxy protocol handshake: {0}")]
    Io(#[from] io::Error),
    #[error("handshake timed out waiting for proxy protocol header")]
    Timeout,
    #[error("connection closed prematurely during proxy protocol handshake")]
    UnexpectedEof,
    #[error("invalid proxy protocol v2 signature")]
    InvalidSignature,
    #[error("unsupported proxy protocol version: expected 2, got {0}")]
    UnsupportedVersion(u8),
    #[error("unsupported proxy protocol command: {0}")]
    UnsupportedCommand(u8),
    #[error("proxy protocol header size exceeds maximum limit ({size} > {MAX_PPV2_HEADER_SIZE})")]
    HeaderTooLarge { size: usize },
    #[error("proxy protocol address block truncated: required {required}, available {available}")]
    TruncatedAddress { required: usize, available: usize },
    #[error("proxy protocol header required on this server, but none was provided")]
    RequiredHeaderMissing,
}

/// Operational mode for PROXY protocol v2 handling.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyProtocolMode {
    /// Normal direct connections; PROXY protocol headers are neither expected nor parsed.
    #[default]
    Disabled,
    /// Automatically detects whether the incoming stream starts with a PPv2 signature.
    /// If present, the real client IP is parsed and propagated; if not, direct connections
    /// are accepted seamlessly.
    Auto,
    /// Strictly requires a valid PPv2 header on every connection; any connection without
    /// a valid PPv2 header is rejected immediately.
    Required,
}

impl ProxyProtocolMode {
    #[allow(dead_code)]
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

/// Result of parsing a PROXY protocol v2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ppv2ParseResult {
    /// The real client socket address if PROXY command was used, or None if LOCAL / UNSPEC.
    pub client_addr: Option<SocketAddr>,
    /// Total bytes consumed by the entire PPv2 header (16 + length).
    pub header_length: usize,
}

/// Parses a PROXY protocol v2 header from an in-memory buffer.
/// Returns `Ok(Some(result))` if a complete valid PPv2 header is present,
/// `Ok(None)` if more bytes are needed, or `Err(ProxyProtocolError)` if malformed.
pub fn parse_ppv2_header(buf: &[u8]) -> Result<Option<Ppv2ParseResult>, ProxyProtocolError> {
    if buf.len() < 16 {
        return Ok(None);
    }

    if buf[..12] != PPV2_SIGNATURE {
        return Err(ProxyProtocolError::InvalidSignature);
    }

    let ver_cmd = buf[12];
    let version = (ver_cmd >> 4) & 0x0F;
    let command = ver_cmd & 0x0F;

    if version != 2 {
        return Err(ProxyProtocolError::UnsupportedVersion(version));
    }

    // Command: 0x0 = LOCAL, 0x1 = PROXY
    if command > 1 {
        return Err(ProxyProtocolError::UnsupportedCommand(command));
    }

    let fam_proto = buf[13];
    let family = (fam_proto >> 4) & 0x0F;
    let transport = fam_proto & 0x0F;

    let payload_len = u16::from_be_bytes([buf[14], buf[15]]) as usize;
    let total_header_size = 16 + payload_len;

    if total_header_size > MAX_PPV2_HEADER_SIZE {
        return Err(ProxyProtocolError::HeaderTooLarge {
            size: total_header_size,
        });
    }

    if buf.len() < total_header_size {
        // Wait for the full header (including any TLVs) to arrive
        return Ok(None);
    }

    let header_payload = &buf[16..total_header_size];

    let client_addr = if command == 0x01 {
        // PROXY command: parse real addresses
        match (family, transport) {
            (0x1, 0x1) => {
                // AF_INET, STREAM (IPv4 TCP)
                if header_payload.len() < 12 {
                    return Err(ProxyProtocolError::TruncatedAddress {
                        required: 12,
                        available: header_payload.len(),
                    });
                }
                let src_ip = Ipv4Addr::new(
                    header_payload[0],
                    header_payload[1],
                    header_payload[2],
                    header_payload[3],
                );
                let src_port = u16::from_be_bytes([header_payload[8], header_payload[9]]);
                Some(SocketAddr::V4(SocketAddrV4::new(src_ip, src_port)))
            }
            (0x2, 0x1) => {
                // AF_INET6, STREAM (IPv6 TCP)
                if header_payload.len() < 36 {
                    return Err(ProxyProtocolError::TruncatedAddress {
                        required: 36,
                        available: header_payload.len(),
                    });
                }
                let mut ip_bytes = [0u8; 16];
                ip_bytes.copy_from_slice(&header_payload[0..16]);
                let src_ip = Ipv6Addr::from(ip_bytes);
                let src_port = u16::from_be_bytes([header_payload[32], header_payload[33]]);
                Some(SocketAddr::V6(SocketAddrV6::new(src_ip, src_port, 0, 0)))
            }
            _ => {
                // UNSPEC or unsupported family; keep proxy/peer socket address
                None
            }
        }
    } else {
        // LOCAL command (health check, local probe); keep proxy/peer address
        None
    };

    Ok(Some(Ppv2ParseResult {
        client_addr,
        header_length: total_header_size,
    }))
}

/// A wrapper around an async stream (such as `TcpStream`) that yields a prefetched
/// buffer of bytes before reading directly from the underlying stream.
pub struct PrefixedStream<S> {
    prefix: Option<Cursor<Vec<u8>>>,
    inner: S,
}

impl<S> PrefixedStream<S> {
    pub fn new(inner: S) -> Self {
        Self { prefix: None, inner }
    }

    pub fn with_prefix(inner: S, prefix: Vec<u8>) -> Self {
        if prefix.is_empty() {
            Self::new(inner)
        } else {
            Self {
                prefix: Some(Cursor::new(prefix)),
                inner,
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    #[allow(dead_code)]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefixedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if let Some(prefix) = &mut self.prefix {
            let pos = prefix.position() as usize;
            let data_len = prefix.get_ref().len();
            if pos < data_len {
                let to_read = cmp::min(data_len - pos, buf.remaining());
                buf.put_slice(&prefix.get_ref()[pos..pos + to_read]);
                let new_pos = pos + to_read;
                prefix.set_position(new_pos as u64);
                if new_pos >= data_len {
                    self.prefix = None;
                }
                return Poll::Ready(Ok(()));
            } else {
                self.prefix = None;
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefixedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

/// An Axum-compatible `Listener` that intercepts incoming TCP connections,
/// optionally parses PROXY protocol v2 headers, and forwards the authentic client `SocketAddr`.
pub struct ProxyProtocolListener {
    listener: TcpListener,
    mode: watch::Receiver<ProxyProtocolMode>,
}

impl ProxyProtocolListener {
    #[cfg(test)]
    pub fn new(listener: TcpListener, mode: ProxyProtocolMode) -> Self {
        Self::with_mode_updates(listener, watch::channel(mode).1)
    }

    pub fn with_mode_updates(listener: TcpListener, mode: watch::Receiver<ProxyProtocolMode>) -> Self {
        Self { listener, mode }
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> ProxyProtocolMode {
        *self.mode.borrow()
    }

    /// Negotiates the incoming stream according to the configured `ProxyProtocolMode`.
    pub async fn process_stream<S>(
        mode: ProxyProtocolMode,
        mut stream: S,
        peer_addr: SocketAddr,
    ) -> Result<(PrefixedStream<S>, SocketAddr), ProxyProtocolError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(256);
        let mut temp_chunk = [0u8; 512];

        loop {
            // Check if we can determine connection type immediately
            if mode == ProxyProtocolMode::Auto {
                if !buf.is_empty() && buf[0] != PPV2_SIGNATURE[0] {
                    // Non-PPv2 direct traffic (e.g. HTTP 'GET', 'POST', TLS handshake, etc.)
                    return Ok((PrefixedStream::with_prefix(stream, buf), peer_addr));
                }
                if buf.len() >= 12 && buf[..12] != PPV2_SIGNATURE {
                    // 12 bytes received and does not match PPv2 magic
                    return Ok((PrefixedStream::with_prefix(stream, buf), peer_addr));
                }
            } else if mode == ProxyProtocolMode::Required {
                if !buf.is_empty() && buf[0] != PPV2_SIGNATURE[0] {
                    return Err(ProxyProtocolError::RequiredHeaderMissing);
                }
                if buf.len() >= 12 && buf[..12] != PPV2_SIGNATURE {
                    return Err(ProxyProtocolError::InvalidSignature);
                }
            }

            // Attempt to parse PPv2 header if we have at least 16 bytes
            if buf.len() >= 16 {
                match parse_ppv2_header(&buf)? {
                    Some(parsed) => {
                        let final_client_addr = parsed.client_addr.unwrap_or(peer_addr);
                        let remaining_bytes = buf[parsed.header_length..].to_vec();
                        return Ok((
                            PrefixedStream::with_prefix(stream, remaining_bytes),
                            final_client_addr,
                        ));
                    }
                    None => {
                        // Header length exceeds current buffer; read more bytes below
                    }
                }
            }

            // Read next chunk from the stream with timeout
            let n = tokio::time::timeout(PPV2_READ_TIMEOUT, stream.read(&mut temp_chunk))
                .await
                .map_err(|_| ProxyProtocolError::Timeout)?
                .map_err(ProxyProtocolError::Io)?;

            if n == 0 {
                return Err(ProxyProtocolError::UnexpectedEof);
            }

            buf.extend_from_slice(&temp_chunk[..n]);

            if buf.len() > MAX_PPV2_HEADER_SIZE + 16384 {
                return Err(ProxyProtocolError::HeaderTooLarge { size: buf.len() });
            }
        }
    }
}

impl Listener for ProxyProtocolListener {
    type Io = PrefixedStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok(conn) => conn,
                Err(err) => {
                    tracing::error!(%err, "TCP accept failed; sleeping 100ms");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };

            // Snapshot once per accepted connection; existing streams retain their protocol.
            let mode = *self.mode.borrow();
            if mode == ProxyProtocolMode::Disabled {
                return (PrefixedStream::new(stream), peer_addr);
            }

            match Self::process_stream(mode, stream, peer_addr).await {
                Ok(conn) => return conn,
                Err(err) => {
                    tracing::debug!(%peer_addr, ?err, "Rejected proxy protocol connection");
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn make_ppv2_ipv4_packet(
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        extra_payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&PPV2_SIGNATURE);
        packet.push(0x21); // version 2, command PROXY
        packet.push(0x11); // family AF_INET (1), transport STREAM (1)
        packet.extend_from_slice(&12u16.to_be_bytes()); // length 12
        packet.extend_from_slice(&src_ip.octets());
        packet.extend_from_slice(&dst_ip.octets());
        packet.extend_from_slice(&src_port.to_be_bytes());
        packet.extend_from_slice(&dst_port.to_be_bytes());
        packet.extend_from_slice(extra_payload);
        packet
    }

    fn make_ppv2_ipv6_packet(
        src_ip: Ipv6Addr,
        dst_ip: Ipv6Addr,
        src_port: u16,
        dst_port: u16,
        extra_payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&PPV2_SIGNATURE);
        packet.push(0x21); // version 2, command PROXY
        packet.push(0x21); // family AF_INET6 (2), transport STREAM (1)
        packet.extend_from_slice(&36u16.to_be_bytes()); // length 36
        packet.extend_from_slice(&src_ip.octets());
        packet.extend_from_slice(&dst_ip.octets());
        packet.extend_from_slice(&src_port.to_be_bytes());
        packet.extend_from_slice(&dst_port.to_be_bytes());
        packet.extend_from_slice(extra_payload);
        packet
    }

    #[test]
    fn parses_valid_ipv4_ppv2_header() {
        let src_ip = Ipv4Addr::new(203, 0, 113, 195);
        let dst_ip = Ipv4Addr::new(192, 168, 1, 10);
        let src_port = 54321;
        let dst_port = 8080;
        let payload = b"GET /api/status HTTP/1.1\r\n";
        let packet = make_ppv2_ipv4_packet(src_ip, dst_ip, src_port, dst_port, payload);

        let parsed = parse_ppv2_header(&packet).unwrap().expect("header present");
        assert_eq!(parsed.header_length, 28);
        assert_eq!(
            parsed.client_addr,
            Some(SocketAddr::V4(SocketAddrV4::new(src_ip, src_port)))
        );
        assert_eq!(&packet[parsed.header_length..], payload);
    }

    #[test]
    fn parses_valid_ipv6_ppv2_header() {
        let src_ip = Ipv6Addr::from_str("2001:db8::8a2e:370:7334").unwrap();
        let dst_ip = Ipv6Addr::from_str("2001:db8::1").unwrap();
        let src_port = 49152;
        let dst_port = 443;
        let payload = b"POST /api/login HTTP/1.1\r\n";
        let packet = make_ppv2_ipv6_packet(src_ip, dst_ip, src_port, dst_port, payload);

        let parsed = parse_ppv2_header(&packet).unwrap().expect("header present");
        assert_eq!(parsed.header_length, 52); // 16 + 36
        assert_eq!(
            parsed.client_addr,
            Some(SocketAddr::V6(SocketAddrV6::new(src_ip, src_port, 0, 0)))
        );
        assert_eq!(&packet[parsed.header_length..], payload);
    }

    #[test]
    fn parses_local_healthcheck_ppv2_header() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&PPV2_SIGNATURE);
        packet.push(0x20); // version 2, command LOCAL (0)
        packet.push(0x00); // UNSPEC
        packet.extend_from_slice(&0u16.to_be_bytes()); // length 0
        packet.extend_from_slice(b"OPTIONS * HTTP/1.1\r\n");

        let parsed = parse_ppv2_header(&packet).unwrap().expect("header present");
        assert_eq!(parsed.header_length, 16);
        assert_eq!(parsed.client_addr, None);
    }

    #[test]
    fn skips_tlv_extensions_properly() {
        let src_ip = Ipv4Addr::new(10, 0, 0, 1);
        let dst_ip = Ipv4Addr::new(10, 0, 0, 2);
        let mut packet = Vec::new();
        packet.extend_from_slice(&PPV2_SIGNATURE);
        packet.push(0x21); // version 2, command PROXY
        packet.push(0x11); // IPv4 TCP
        let total_len = 12u16 + 5u16; // 12 bytes address + 5 bytes fake TLV
        packet.extend_from_slice(&total_len.to_be_bytes());
        packet.extend_from_slice(&src_ip.octets());
        packet.extend_from_slice(&dst_ip.octets());
        packet.extend_from_slice(&12345u16.to_be_bytes());
        packet.extend_from_slice(&80u16.to_be_bytes());
        packet.extend_from_slice(&[0x01, 0x00, 0x02, 0xAA, 0xBB]); // fake TLV
        packet.extend_from_slice(b"HTTP/1.1 200 OK");

        let parsed = parse_ppv2_header(&packet).unwrap().expect("header present");
        assert_eq!(parsed.header_length, 16 + 17);
        assert_eq!(
            parsed.client_addr,
            Some(SocketAddr::V4(SocketAddrV4::new(src_ip, 12345)))
        );
        assert_eq!(&packet[parsed.header_length..], b"HTTP/1.1 200 OK");
    }

    #[test]
    fn rejects_invalid_signature() {
        let bad_packet = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let err = parse_ppv2_header(bad_packet).unwrap_err();
        assert!(matches!(err, ProxyProtocolError::InvalidSignature));
    }

    #[tokio::test]
    async fn prefixed_stream_reads_prefix_then_underlying() {
        let (mut client, server) = tokio::io::duplex(64);
        let mut prefixed = PrefixedStream::with_prefix(server, b"hello ".to_vec());

        tokio::spawn(async move {
            client.write_all(b"world").await.unwrap();
        });

        let mut buf = vec![0u8; 11];
        prefixed.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello world");
    }

    #[tokio::test]
    async fn process_stream_auto_mode_with_ppv2_packet() {
        let peer = "10.0.0.1:12345".parse().unwrap();
        let (client, server) = tokio::io::duplex(1024);
        let packet = make_ppv2_ipv4_packet(
            Ipv4Addr::new(198, 51, 100, 7),
            Ipv4Addr::new(10, 0, 0, 1),
            44332,
            8080,
            b"PING-PAYLOAD",
        );
        let writer = tokio::spawn(async move {
            let mut client = client;
            client.write_all(&packet).await.unwrap();
        });

        let (mut prefixed, remote_addr) =
            ProxyProtocolListener::process_stream(ProxyProtocolMode::Auto, server, peer)
                .await
                .unwrap();
        writer.await.unwrap();
        assert_eq!(remote_addr, "198.51.100.7:44332".parse::<SocketAddr>().unwrap());
        let mut payload = Vec::new();
        prefixed.read_to_end(&mut payload).await.unwrap();
        assert_eq!(&payload, b"PING-PAYLOAD");
    }

    #[tokio::test]
    async fn process_stream_auto_mode_with_direct_traffic() {
        let peer = "10.0.0.1:12345".parse().unwrap();
        let (client, server) = tokio::io::duplex(1024);
        let writer = tokio::spawn(async move {
            let mut client = client;
            client
                .write_all(b"GET /whoami HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .await
                .unwrap();
        });

        let (mut prefixed, remote_addr) =
            ProxyProtocolListener::process_stream(ProxyProtocolMode::Auto, server, peer)
                .await
                .unwrap();
        writer.await.unwrap();
        assert_eq!(remote_addr, peer);
        let mut payload = Vec::new();
        prefixed.read_to_end(&mut payload).await.unwrap();
        assert_eq!(&payload, b"GET /whoami HTTP/1.1\r\nHost: localhost\r\n\r\n");
    }

    #[tokio::test]
    async fn process_stream_required_mode_rejects_missing_header() {
        let peer = "10.0.0.1:12345".parse().unwrap();
        let (client, server) = tokio::io::duplex(1024);
        let writer = tokio::spawn(async move {
            let mut client = client;
            client
                .write_all(b"GET /whoami HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .await
                .unwrap();
        });

        let result =
            ProxyProtocolListener::process_stream(ProxyProtocolMode::Required, server, peer).await;
        writer.await.unwrap();
        assert!(matches!(result, Err(ProxyProtocolError::RequiredHeaderMissing)));
    }

    #[tokio::test]
    async fn listener_applies_mode_changes_to_new_connections() {
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = tcp.local_addr().unwrap();
        let (updates, modes) = watch::channel(ProxyProtocolMode::Disabled);
        let mut listener = ProxyProtocolListener::with_mode_updates(tcp, modes);
        let claimed_ip = Ipv4Addr::new(203, 0, 113, 195);
        let packet = make_ppv2_ipv4_packet(
            claimed_ip, Ipv4Addr::LOCALHOST, 54321, address.port(), b"hello",
        );

        // The same header is passed through while disabled, and parsed after an update.
        for mode in [ProxyProtocolMode::Disabled, ProxyProtocolMode::Auto, ProxyProtocolMode::Disabled] {
            updates.send_replace(mode);
            let mut client = match TcpStream::connect(address).await {
                Ok(stream) => stream,
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                    eprintln!("Skipping network test in sandboxed environment: {e}");
                    return;
                }
                Err(e) => panic!("Failed to connect to local test server: {e}"),
            };
            client.write_all(&packet).await.unwrap();
            let (mut stream, peer) = tokio::time::timeout(
                Duration::from_secs(2), listener.accept(),
            ).await.unwrap();
            if mode == ProxyProtocolMode::Auto {
                assert_eq!(peer, SocketAddr::new(claimed_ip.into(), 54321));
                let mut payload = [0; 5];
                stream.read_exact(&mut payload).await.unwrap();
                assert_eq!(&payload, b"hello");
            } else {
                assert_eq!(peer.ip(), Ipv4Addr::LOCALHOST);
                let mut header = [0; 12];
                stream.read_exact(&mut header).await.unwrap();
                assert_eq!(header, PPV2_SIGNATURE);
            }
        }
    }

    #[tokio::test]
    async fn axum_server_integrates_with_proxy_protocol_listener() {
        use axum::{extract::ConnectInfo, routing::get, serve::ListenerExt, Router};

        let app = Router::new().route(
            "/whoami",
            get(|ConnectInfo(addr): ConnectInfo<SocketAddr>| async move {
                addr.to_string()
            }),
        );

        let Ok(tcp_listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let local_addr = tcp_listener.local_addr().unwrap();
        let pp_listener = ProxyProtocolListener::new(tcp_listener, ProxyProtocolMode::Auto);

        let server_task = tokio::spawn(async move {
            let _ = axum::serve(
                pp_listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });

        // Test 1: Send with PPv2 header specifying remote client 203.0.113.195:54321
        let mut client1 = match tokio::net::TcpStream::connect(local_addr).await {
            Ok(stream) => stream,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping network integration test in sandboxed environment: {e}");
                server_task.abort();
                return;
            }
            Err(e) => panic!("Failed to connect to local test server: {e}"),
        };

        let packet = make_ppv2_ipv4_packet(
            Ipv4Addr::new(203, 0, 113, 195),
            Ipv4Addr::new(127, 0, 0, 1),
            54321,
            local_addr.port(),
            b"GET /whoami HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        client1.write_all(&packet).await.unwrap();
        let mut response = String::new();
        client1.read_to_string(&mut response).await.unwrap();
        assert!(
            response.contains("203.0.113.195:54321"),
            "Expected real client IP in response, got: {response}"
        );

        // Test 2: Send direct HTTP without PPv2 header (in Auto mode)
        let mut client2 = tokio::net::TcpStream::connect(local_addr).await.unwrap();
        client2
            .write_all(b"GET /whoami HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response2 = String::new();
        client2.read_to_string(&mut response2).await.unwrap();
        assert!(
            response2.contains("127.0.0.1"),
            "Expected 127.0.0.1 in direct response, got: {response2}"
        );

        server_task.abort();
    }
}
