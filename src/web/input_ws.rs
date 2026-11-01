use super::*;

/// Upper bound on one input message; real messages are well under 200 bytes.
const INPUT_WS_MAX_MESSAGE: usize = 4 * 1024;

/// One keyboard/mouse operation sent over the input WebSocket. Every message
/// carries a client-chosen `id` that is echoed in its reply, so the browser can
/// resolve the matching request even though replies share one socket.
#[derive(Debug, Deserialize)]
pub(super) struct InputMessage {
    pub(super) id: u64,
    #[serde(flatten)]
    pub(super) command: InputCommand,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum InputCommand {
    Key(KeyRequest),
    Move(MouseMoveRequest),
    Absolute(AbsolutePointerRequest),
    Click(MouseClickRequest),
    Press(MouseClickRequest),
    Release(MouseClickRequest),
    Scroll(MouseScrollRequest),
    ReleaseAll,
}

/// Low-latency keyboard and mouse transport. It replaces one HTTP request per
/// event with a single socket, while keeping the same authorization model as
/// the terminal: exact Origin check before upgrade, and a session guard that
/// closes the socket on logout, revocation or expiry.
pub(super) async fn input_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let public_origin = state.config.read().await.server.public_origin.clone();
    let sessions = state.sessions.clone();
    let release_state = state.clone();
    let socket_guard = state.metrics.clone();
    upgrade_input_ws(
        &sessions,
        &headers,
        ws,
        public_origin.as_deref(),
        {
            // Counted as open for as long as the socket loop holds this closure.
            let _open = socket_guard.input_socket_opened();
            move |text| {
                let _ = &_open;
                let state = state.clone();
                async move { input_reply(&state, &text).await }
            }
        },
        // A socket can end mid-chord (tab closed, crash, network drop, logout);
        // never leave keys or buttons held on the target.
        move || async move {
            if let Err(error) = send_release_all(&release_state).await {
                tracing::warn!(error = %error.message, "release after input socket closed failed");
            }
        },
    )
}

/// Shared by the real route and tests: authorizes the upgrade and runs the
/// socket loop with `execute` handling each text message.
pub(super) fn upgrade_input_ws<F, Fut, R, RFut>(
    sessions: &SessionStore,
    headers: &HeaderMap,
    ws: WebSocketUpgrade,
    public_origin: Option<&str>,
    execute: F,
    on_close: R,
) -> Response
where
    F: Fn(String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = serde_json::Value> + Send + 'static,
    R: FnOnce() -> RFut + Send + 'static,
    RFut: std::future::Future<Output = ()> + Send + 'static,
{
    if let Err(error) = validate_terminal_origin(headers, public_origin) {
        return error.into_response();
    }
    let Some(guard) = session_token(headers).and_then(|token| sessions.validate_guard(token)) else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    ws.max_message_size(INPUT_WS_MAX_MESSAGE)
        .max_frame_size(INPUT_WS_MAX_MESSAGE)
        .on_upgrade(move |socket| async move {
            let used = handle_input(socket, guard, execute).await;
            if used {
                on_close().await;
            }
        })
}

/// Runs the socket until it closes or the session ends; returns whether any
/// message was processed (only then is a release needed).
async fn handle_input<F, Fut>(mut socket: WebSocket, guard: SessionGuard, execute: F) -> bool
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = serde_json::Value>,
{
    let mut used = false;
    if !guard.is_active() {
        return used;
    }
    loop {
        let incoming = tokio::select! {
            biased;
            _ = guard.wait_for_end() => break,
            incoming = socket.recv() => incoming,
        };
        let text = match incoming {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
            Some(Ok(_)) => continue,
        };
        // Operations run strictly in arrival order, matching what the user
        // typed; a key press can never overtake an earlier release.
        used = true;
        let reply = execute(text.to_string()).await;
        let send = socket.send(Message::Text(reply.to_string().into()));
        tokio::pin!(send);
        tokio::select! {
            biased;
            _ = guard.wait_for_end() => break,
            result = &mut send => if result.is_err() { break },
        }
    }
    used
}

/// Executes one message and builds its reply. Malformed input is answered
/// (with the id when it can be recovered) rather than closing the socket.
pub(super) async fn input_reply(state: &AppState, text: &str) -> serde_json::Value {
    let message = match serde_json::from_str::<InputMessage>(text) {
        Ok(message) => message,
        Err(error) => {
            let id = serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|value| value.get("id").and_then(serde_json::Value::as_u64));
            return json!({
                "id": id,
                "ok": false,
                "status": StatusCode::BAD_REQUEST.as_u16(),
                "error": format!("无法识别的输入消息：{error}"),
            });
        }
    };
    let result = match message.command {
        InputCommand::Key(request) => send_key(state, request).await,
        InputCommand::Move(request) => send_mouse_move(state, request).await.map(|()| json!({})),
        InputCommand::Absolute(request) => send_mouse_absolute(state, request).await.map(|()| json!({})),
        InputCommand::Click(request) => send_mouse_click(state, request).await.map(|()| json!({})),
        InputCommand::Press(request) => send_mouse_press(state, request).await.map(|()| json!({})),
        InputCommand::Release(request) => send_mouse_release(state, request).await.map(|()| json!({})),
        InputCommand::Scroll(request) => send_mouse_scroll(state, request).await.map(|()| json!({})),
        InputCommand::ReleaseAll => send_release_all(state).await.map(|()| json!({})),
    };
    match result {
        Ok(mut body) => {
            body["id"] = json!(message.id);
            body["ok"] = json!(true);
            body
        }
        Err(error) => json!({
            "id": message.id,
            "ok": false,
            "status": error.status.as_u16(),
            "error": error.message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_messages_parse_every_command_with_their_id() {
        let cases = [
            (r#"{"id":1,"type":"key","key":"KeyA","shift":true}"#, "Key"),
            (r#"{"id":2,"type":"move","dx":3,"dy":-4}"#, "Move"),
            (r#"{"id":3,"type":"absolute","action":"click","x":10,"y":20,"button":1}"#, "Absolute"),
            (r#"{"id":4,"type":"click","button":2}"#, "Click"),
            (r#"{"id":5,"type":"scroll","wheel":-1}"#, "Scroll"),
            (r#"{"id":6,"type":"release_all"}"#, "ReleaseAll"),
            (r#"{"id":7,"type":"press","button":1}"#, "Press"),
            (r#"{"id":8,"type":"release","button":1}"#, "Release"),
            (r#"{"id":9,"type":"absolute","action":"press","x":1,"y":2,"button":2}"#, "Absolute"),
            (r#"{"id":10,"type":"absolute","action":"release","x":1,"y":2,"button":2}"#, "Absolute"),
        ];
        for (index, (text, kind)) in cases.into_iter().enumerate() {
            let message: InputMessage = serde_json::from_str(text).expect(text);
            assert_eq!(message.id, index as u64 + 1);
            assert!(format!("{:?}", message.command).starts_with(kind), "{text}");
        }
    }

    /// Counts on_close releases across the test servers in this module.
    static RELEASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    struct InputTestServer {
        address: SocketAddr,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for InputTestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn spawn_input_test_server(sessions: SessionStore) -> Option<InputTestServer> {
        async fn route(
            State(sessions): State<SessionStore>,
            headers: HeaderMap,
            ws: WebSocketUpgrade,
        ) -> Response {
            // Echo executor: proves ordering and id round-trip without HID.
            upgrade_input_ws(
                &sessions,
                &headers,
                ws,
                None,
                |text| async move {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    json!({ "id": value["id"], "ok": true, "type": value["type"] })
                },
                || async {
                    RELEASES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
            )
        }
        let app = Router::new().route("/input", get(route)).with_state(sessions);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
        let address = listener.local_addr().ok()?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Some(InputTestServer { address, task })
    }

    fn input_request(
        address: SocketAddr,
        token: &str,
        origin: &str,
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://{address}/input").into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.insert(COOKIE, HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}")).unwrap());
        headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        request
    }

    #[tokio::test]
    async fn input_socket_replies_in_order_and_closes_on_logout() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Client;
        let sessions = SessionStore::default();
        let token = sessions.create().unwrap();
        let Some(server) = spawn_input_test_server(sessions.clone()).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let origin = format!("http://{}", server.address);
        let (mut socket, _) = tokio_tungstenite::connect_async(input_request(server.address, &token, &origin))
            .await
            .expect("authorized upgrade");
        for id in 1..=3 {
            socket
                .send(Client::Text(format!(r#"{{"id":{id},"type":"move","dx":1,"dy":0}}"#).into()))
                .await
                .unwrap();
        }
        for id in 1..=3 {
            let reply = socket.next().await.unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
            assert_eq!(value["id"], id, "replies arrive in send order");
        }
        let releases_before = RELEASES.load(std::sync::atomic::Ordering::SeqCst);
        assert!(sessions.revoke(&token));
        let closed = tokio::time::timeout(Duration::from_secs(2), socket.next()).await.expect("socket closes after logout");
        assert!(matches!(closed, None | Some(Ok(Client::Close(_))) | Some(Err(_))));
        // Held keys must be released once the socket ends.
        for _ in 0..50 {
            if RELEASES.load(std::sync::atomic::Ordering::SeqCst) > releases_before {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(RELEASES.load(std::sync::atomic::Ordering::SeqCst) > releases_before, "release on close");
    }

    #[tokio::test]
    async fn input_socket_rejects_foreign_origin_and_missing_session() {
        let sessions = SessionStore::default();
        let token = sessions.create().unwrap();
        let Some(server) = spawn_input_test_server(sessions).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let foreign = tokio_tungstenite::connect_async(input_request(server.address, &token, "http://evil.example")).await;
        assert!(foreign.is_err(), "cross-origin upgrade must be refused");
        let own = format!("http://{}", server.address);
        let anonymous = tokio_tungstenite::connect_async(input_request(server.address, "bogus", &own)).await;
        assert!(anonymous.is_err(), "upgrade without a live session must be refused");
    }

    /// The real route and a real mouse worker, with a FIFO as the pointer device.
    struct PointerDeviceServer {
        state: AppState,
        token: String,
        reader: std::fs::File,
        address: SocketAddr,
        task: tokio::task::JoinHandle<()>,
        dir: PathBuf,
    }

    impl Drop for PointerDeviceServer {
        fn drop(&mut self) {
            self.task.abort();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    type ClientSocket =
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    impl PointerDeviceServer {
        async fn start(name: &str, mode: PointerMode) -> Option<Self> {
            use std::os::unix::fs::OpenOptionsExt;
            let dir = std::env::temp_dir().join(format!("wingman-input-ws-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let device = dir.join("pointer");
            let c_path = std::ffi::CString::new(device.to_str().unwrap()).unwrap();
            // SAFETY: c_path is a valid NUL-terminated string for the call's duration.
            assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
            let reader = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&device)
                .unwrap();
            let mut config = Config::default();
            config.hid.pointer_mode = mode;
            if mode == PointerMode::Absolute {
                config.hid.absolute_pointer_device = Some(device);
            } else {
                config.hid.mouse_device = Some(device);
            }
            let state = crate::web::config_api::tests::test_state(config, dir.join("config.toml"));
            let token = state.sessions.create().unwrap();
            let app = Router::new().route("/input", get(input_ws)).with_state(state.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
            let address = listener.local_addr().ok()?;
            let task = tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Some(Self { state, token, reader, address, task, dir })
        }

        async fn connect(&self) -> ClientSocket {
            let origin = format!("http://{}", self.address);
            let request = input_request(self.address, &self.token, &origin);
            tokio_tungstenite::connect_async(request).await.expect("authorized upgrade").0
        }

        /// Reads exactly one report, waiting a bounded time for it.
        async fn report<const N: usize>(&mut self) -> [u8; N] {
            use std::io::Read;
            let mut report = [0; N];
            let mut got = 0;
            for _ in 0..500 {
                if let Ok(read) = self.reader.read(&mut report[got..]) {
                    got += read;
                }
                if got == N {
                    return report;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("no HID report after {got} bytes");
        }
    }

    async fn send_and_wait_reply(socket: &mut ClientSocket, text: &str) {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as Client;
        socket.send(Client::Text(text.to_owned().into())).await.unwrap();
        let reply = socket.next().await.unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
        assert_eq!(value["ok"], true, "{value}");
    }

    #[tokio::test]
    async fn absolute_drag_over_the_socket_is_released_when_the_socket_ends() {
        let Some(mut server) = PointerDeviceServer::start("absolute-end", PointerMode::Absolute).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let mut socket = server.connect().await;
        send_and_wait_reply(&mut socket, r#"{"id":1,"type":"absolute","action":"press","x":10,"y":20,"button":1}"#).await;
        assert_eq!(server.report::<6>().await, [1, 10, 0, 20, 0, 0]);
        send_and_wait_reply(&mut socket, r#"{"id":2,"type":"absolute","action":"move","x":30,"y":40}"#).await;
        assert_eq!(server.report::<6>().await, [1, 30, 0, 40, 0, 0], "the move holds the button");
        socket.close(None).await.unwrap();
        assert_eq!(server.report::<6>().await, [0, 30, 0, 40, 0, 0], "socket end sends a zero-button report");
        // The server's mask is clear too: a later HTTP move carries no button.
        send_mouse_absolute(&server.state, AbsolutePointerRequest::Move { x: 50, y: 60 }).await.unwrap();
        assert_eq!(server.report::<6>().await, [0, 50, 0, 60, 0, 0]);
    }

    #[tokio::test]
    async fn relative_drag_is_released_when_the_session_is_revoked() {
        let Some(mut server) = PointerDeviceServer::start("relative-revoke", PointerMode::Relative).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let mut socket = server.connect().await;
        send_and_wait_reply(&mut socket, r#"{"id":1,"type":"press","button":2}"#).await;
        assert_eq!(server.report::<4>().await, [2, 0, 0, 0]);
        send_and_wait_reply(&mut socket, r#"{"id":2,"type":"move","dx":3,"dy":4}"#).await;
        assert_eq!(server.report::<4>().await, [2, 3, 4, 0]);
        assert!(server.state.sessions.revoke(&server.token));
        assert_eq!(server.report::<4>().await, [0, 0, 0, 0], "revocation ends the socket, which releases");
        send_mouse_move(&server.state, MouseMoveRequest { dx: 1, dy: 1 }).await.unwrap();
        assert_eq!(server.report::<4>().await, [0, 1, 1, 0]);
    }

    #[tokio::test]
    async fn http_press_is_released_when_its_session_ends() {
        let Some(mut server) = PointerDeviceServer::start("http-revoke", PointerMode::Absolute).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        // No socket at all: what the logout and revoke handlers do.
        let press = AbsolutePointerRequest::Press { x: 1, y: 2, button: 4 };
        send_mouse_absolute(&server.state, press).await.unwrap();
        assert_eq!(server.report::<6>().await, [4, 1, 0, 2, 0, 0]);
        release_after_session_end(&server.state);
        assert_eq!(server.report::<6>().await, [0, 1, 0, 2, 0, 0]);
    }

    #[tokio::test]
    async fn press_in_the_wrong_pointer_mode_is_refused() {
        let Some(server) = PointerDeviceServer::start("wrong-mode", PointerMode::Absolute).await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let refused = send_mouse_press(&server.state, MouseClickRequest { button: 1 }).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::CONFLICT);
        let refused = send_mouse_release(&server.state, MouseClickRequest { button: 1 }).await.unwrap_err();
        assert_eq!(refused.status, StatusCode::CONFLICT);
    }

    #[test]
    fn unknown_input_messages_are_rejected() {
        assert!(serde_json::from_str::<InputMessage>(r#"{"id":1,"type":"teleport"}"#).is_err());
        assert!(serde_json::from_str::<InputMessage>(r#"{"type":"key","key":"KeyA"}"#).is_err());
    }
}
