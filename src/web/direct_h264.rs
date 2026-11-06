use super::*;
use crate::devices::webrtc::{EncodedFrame, H264Subscription, MAX_ENCODED_FRAME};
use std::collections::VecDeque;

const WINDOW: usize = 8;
const ACK_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn direct_h264_ws(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let origin = state.config.read().await.server.public_origin.clone();
    if let Err(error) = validate_terminal_origin(&headers, origin.as_deref(), connection.tls) {
        return error.into_response();
    }
    let Some(guard) =
        session_token(&headers).and_then(|token| state.sessions.validate_guard(token))
    else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    let subscription = match state.webrtc.subscribe_h264(&guard).await {
        Ok(feed) => feed,
        Err(error) => return map_webrtc_error(error).into_response(),
    };
    let max_kbps = state.webrtc.status().bitrate_kbps;
    ws.max_message_size(1024)
        .max_frame_size(1024)
        .on_upgrade(move |socket| direct_stream(socket, subscription, guard, max_kbps))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    Ack { sequence: u64 },
    Bitrate { kbps: u32 },
}

// V1: "WK26", flags, 3 reserved bytes, epoch u32, timestamp_us u64,
// duration_us u32, sequence u64, then one complete Annex B access unit.
fn packet(frame: &EncodedFrame) -> Bytes {
    let mut bytes = Vec::with_capacity(32 + frame.data.len());
    bytes.extend_from_slice(b"WK26");
    bytes.extend_from_slice(&[u8::from(frame.key), 0, 0, 0]);
    bytes.extend_from_slice(&frame.epoch.to_be_bytes());
    bytes.extend_from_slice(&frame.timestamp_us.to_be_bytes());
    bytes.extend_from_slice(&frame.duration_us.to_be_bytes());
    bytes.extend_from_slice(&frame.sequence.to_be_bytes());
    bytes.extend_from_slice(&frame.data);
    Bytes::from(bytes)
}

async fn send(socket: &mut WebSocket, guard: &SessionGuard, message: Message) -> bool {
    tokio::select! {
        biased;
        _ = guard.wait_for_end() => false,
        result = tokio::time::timeout(ACK_TIMEOUT, socket.send(message)) => matches!(result, Ok(Ok(()))),
    }
}

async fn direct_stream(
    mut socket: WebSocket,
    mut feed: H264Subscription,
    guard: SessionGuard,
    max_kbps: u32,
) {
    let mut in_flight: VecDeque<(u64, Instant, usize)> = VecDeque::new();
    let mut pending_bytes = 0;
    let mut epoch = 0;
    let mut applied_bitrate = 0;
    let mut last_frame = Instant::now();
    loop {
        let deadline = in_flight
            .front()
            .map_or(last_frame + Duration::from_secs(10), |p| p.1 + ACK_TIMEOUT);
        tokio::select! {
            biased;
            _ = guard.wait_for_end() => break,
            _ = tokio::time::sleep_until(deadline.into()) => break,
            message = socket.recv() => {
                let Some(Ok(Message::Text(text))) = message else { break; };
                match serde_json::from_str::<Control>(&text) {
                    Ok(Control::Ack { sequence }) => {
                        // ACK only a frame actually sent on this socket. Cumulative
                        // acknowledgements also release decoder-discarded frames.
                        if let Some(index) = in_flight.iter().position(|p| p.0 == sequence) {
                            let rtt_ms = in_flight[index].1.elapsed().as_millis() as u64;
                            for _ in 0..=index { pending_bytes -= in_flight.pop_front().unwrap().2; }
                            let reply = json!({"type":"feedback", "rtt_ms":rtt_ms});
                            if !send(&mut socket, &guard, Message::Text(reply.to_string().into())).await { break; }
                        }
                    }
                    Ok(Control::Bitrate { kbps }) => {
                        let Ok(target) = feed.set_bitrate(kbps) else { break; };
                        let reply = json!({"type":"bitrate", "kbps":target, "max_kbps":max_kbps});
                        if !send(&mut socket, &guard, Message::Text(reply.to_string().into())).await { break; }
                    }
                    Err(_) => break,
                }
            }
            result = feed.next(), if in_flight.len() < WINDOW && pending_bytes < MAX_ENCODED_FRAME => {
                let Ok(frame) = result else { break; };
                if frame.epoch != epoch {
                    if !frame.key { continue; }
                    let Some(codec) = frame.codec.as_ref() else { continue; };
                    let config = json!({"type":"config", "version":1, "codec":codec, "epoch":frame.epoch,
                        "bitrate_kbps":frame.bitrate_kbps, "max_kbps":max_kbps, "min_kbps":256});
                    if !send(&mut socket, &guard, Message::Text(config.to_string().into())).await { break; }
                    epoch = frame.epoch;
                    applied_bitrate = frame.bitrate_kbps;
                } else if frame.bitrate_kbps != applied_bitrate {
                    let feedback = json!({"type":"bitrate_applied", "kbps":frame.bitrate_kbps});
                    if !send(&mut socket, &guard, Message::Text(feedback.to_string().into())).await { break; }
                    applied_bitrate = frame.bitrate_kbps;
                }
                let size = frame.data.len();
                if !send(&mut socket, &guard, Message::Binary(packet(&frame))).await { break; }
                last_frame = Instant::now();
                in_flight.push_back((frame.sequence, last_frame, size));
                pending_bytes += size;
            }
        }
    }
    // Dropping the feed detaches only this viewer. The encoder stops once the
    // last viewer has left; session revocation never leaves a live video socket.
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        MaybeTlsStream, WebSocketStream,
        tungstenite::{Message as Client, client::IntoClientRequest},
    };
    type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

    struct Server {
        state: AppState,
        address: SocketAddr,
        token: String,
        task: tokio::task::JoinHandle<()>,
        _source: watch::Sender<Option<Arc<crate::devices::video::VideoFrame>>>,
        _directory: crate::tls::tests::Directory,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.state.sessions.revoke(&self.token);
            self.task.abort();
        }
    }
    impl Server {
        async fn start() -> Self {
            use axum::serve::ListenerExt;
            use std::os::unix::fs::PermissionsExt;
            let directory = crate::tls::tests::Directory::new();
            let executable = directory.0.join("encoder");
            std::fs::write(&executable, "#!/bin/sh\nwhile :; do printf '\\000\\000\\000\\001\\011\\360\\000\\000\\000\\001\\147\\102\\300\\036\\000\\000\\000\\001\\150\\316\\000\\000\\000\\001\\145\\210'; sleep 0.02; done\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut config = Config::default();
            config.video.h264.max_sessions = 2;
            let (video, source) = VideoManager::test_source(config.video.clone());
            source.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
                jpeg: Bytes::from_static(&[255, 216, 255, 217]),
                sequence: 1,
            })));
            let mut state =
                config_api::tests::test_state(config.clone(), directory.0.join("config.toml"));
            state.webrtc =
                WebRtcManager::test_encoder(video.clone(), config.video.h264, executable);
            state.video = video;
            let token = state.sessions.create().unwrap();
            let app = Router::new()
                .route("/video", get(direct_h264_ws))
                .with_state(state.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let listener = crate::client_ip::ClientIpListener(
                crate::proxy_protocol::ProxyProtocolListener::new(
                    listener,
                    ProxyProtocolMode::Disabled,
                ),
            );
            let task = tokio::spawn(async move {
                axum::serve(
                    listener.tap_io(|_| {}),
                    app.into_make_service_with_connect_info::<ClientConnection>(),
                )
                .await
                .unwrap();
            });
            Self {
                state,
                address,
                token,
                task,
                _source: source,
                _directory: directory,
            }
        }
        fn request(
            &self,
            token: &str,
            origin: &str,
        ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
            let mut request = format!("ws://{}/video", self.address)
                .into_client_request()
                .unwrap();
            request.headers_mut().insert(
                COOKIE,
                HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}")).unwrap(),
            );
            request
                .headers_mut()
                .insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
            request
        }
        async fn connect(&self) -> Socket {
            tokio_tungstenite::connect_async(
                self.request(&self.token, &format!("http://{}", self.address)),
            )
            .await
            .unwrap()
            .0
        }
    }
    async fn next(socket: &mut Socket) -> Client {
        tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn frame(socket: &mut Socket) -> Bytes {
        loop {
            if let Client::Binary(data) = next(socket).await {
                return data;
            }
        }
    }
    fn sequence(data: &[u8]) -> u64 {
        u64::from_be_bytes(data[24..32].try_into().unwrap())
    }

    #[tokio::test]
    async fn direct_requires_origin_and_live_session_before_starting_encoder() {
        let server = Server::start().await;
        for (token, origin, status) in [
            (
                server.token.as_str(),
                "http://foreign.example".to_string(),
                StatusCode::FORBIDDEN,
            ),
            (
                "invalid",
                format!("http://{}", server.address),
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let error = tokio_tungstenite::connect_async(server.request(token, &origin))
                .await
                .unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("unexpected rejection: {error}");
            };
            assert_eq!(response.status(), status);
        }
        assert_eq!(server.state.webrtc.status().h264_viewers, 0);
    }

    #[tokio::test]
    async fn direct_window_is_bounded_ack_resumes_and_logout_closes() {
        let server = Server::start().await;
        let mut socket = server.connect().await;
        let config: serde_json::Value =
            serde_json::from_str(next(&mut socket).await.to_text().unwrap()).unwrap();
        assert_eq!(config["codec"], "avc1.42c01e");
        assert_eq!(config["version"], 1);
        let mut last = 0;
        let mut timestamp = 0;
        for _ in 0..WINDOW {
            let data = frame(&mut socket).await;
            assert_eq!(&data[..4], b"WK26");
            assert_eq!(data[4], 1);
            assert_eq!(u32::from_be_bytes(data[8..12].try_into().unwrap()), 1);
            let now = u64::from_be_bytes(data[12..20].try_into().unwrap());
            assert!(now > timestamp);
            timestamp = now;
            assert!(sequence(&data) > last);
            last = sequence(&data);
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(250), socket.next())
                .await
                .is_err()
        );
        // A forged future ACK cannot reopen the window.
        socket
            .send(Client::Text(
                json!({"type":"ack","sequence":last+1000})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), socket.next())
                .await
                .is_err()
        );
        socket
            .send(Client::Text(
                json!({"type":"ack","sequence":last}).to_string().into(),
            ))
            .await
            .unwrap();
        assert!(sequence(&frame(&mut socket).await) > last);
        server.state.sessions.revoke(&server.token);
        let closed = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await;
        assert!(closed.is_ok());
        for _ in 0..50 {
            if server.state.webrtc.status().h264_viewers == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(server.state.webrtc.status().h264_viewers, 0);
    }

    #[tokio::test]
    async fn direct_bitrate_acknowledges_clamping_and_restarts_with_new_epoch() {
        let server = Server::start().await;
        let mut socket = server.connect().await;
        let first = frame(&mut socket).await;
        socket
            .send(Client::Text(
                json!({"type":"ack","sequence":sequence(&first)})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Client::Text(r#"{"type":"bitrate","kbps":1}"#.into()))
            .await
            .unwrap();
        let mut reply = false;
        loop {
            match next(&mut socket).await {
                Client::Text(text) => {
                    let message: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if message["type"] == "bitrate" {
                        assert_eq!(message["kbps"], 256);
                        reply = true;
                    }
                    if message["type"] == "config" && message["epoch"] == 2 {
                        assert_eq!(message["bitrate_kbps"], 256);
                        assert!(reply);
                        break;
                    }
                }
                Client::Binary(data) => socket
                    .send(Client::Text(
                        json!({"type":"ack","sequence":sequence(&data)})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap(),
                message => panic!("unexpected message: {message:?}"),
            }
        }
        assert_eq!(frame(&mut socket).await[4], 1, "restart begins at an IDR");
    }

    #[tokio::test]
    async fn direct_unresponsive_viewer_is_detached() {
        let server = Server::start().await;
        let mut socket = server.connect().await;
        let ended = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "no ACK must terminate the stream");
        assert_eq!(server.state.webrtc.status().h264_viewers, 0);
    }
}
