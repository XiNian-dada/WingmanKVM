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
    upgrade_input_ws(&sessions, &headers, ws, public_origin.as_deref(), move |text| {
        let state = state.clone();
        async move { input_reply(&state, &text).await }
    })
}

/// Shared by the real route and tests: authorizes the upgrade and runs the
/// socket loop with `execute` handling each text message.
pub(super) fn upgrade_input_ws<F, Fut>(
    sessions: &SessionStore,
    headers: &HeaderMap,
    ws: WebSocketUpgrade,
    public_origin: Option<&str>,
    execute: F,
) -> Response
where
    F: Fn(String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = serde_json::Value> + Send + 'static,
{
    if let Err(error) = validate_terminal_origin(headers, public_origin) {
        return error.into_response();
    }
    let Some(guard) = session_token(headers).and_then(|token| sessions.validate_guard(token)) else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    ws.max_message_size(INPUT_WS_MAX_MESSAGE)
        .max_frame_size(INPUT_WS_MAX_MESSAGE)
        .on_upgrade(move |socket| handle_input(socket, guard, execute))
}

async fn handle_input<F, Fut>(mut socket: WebSocket, guard: SessionGuard, execute: F)
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = serde_json::Value>,
{
    if !guard.is_active() {
        return;
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
        let reply = execute(text.to_string()).await;
        let send = socket.send(Message::Text(reply.to_string().into()));
        tokio::pin!(send);
        tokio::select! {
            biased;
            _ = guard.wait_for_end() => break,
            result = &mut send => if result.is_err() { break },
        }
    }
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
        ];
        for (index, (text, kind)) in cases.into_iter().enumerate() {
            let message: InputMessage = serde_json::from_str(text).expect(text);
            assert_eq!(message.id, index as u64 + 1);
            assert!(format!("{:?}", message.command).starts_with(kind), "{text}");
        }
    }

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
            upgrade_input_ws(&sessions, &headers, ws, None, |text| async move {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                json!({ "id": value["id"], "ok": true, "type": value["type"] })
            })
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
        assert!(sessions.revoke(&token));
        let closed = tokio::time::timeout(Duration::from_secs(2), socket.next()).await.expect("socket closes after logout");
        assert!(matches!(closed, None | Some(Ok(Client::Close(_))) | Some(Err(_))));
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

    #[test]
    fn unknown_input_messages_are_rejected() {
        assert!(serde_json::from_str::<InputMessage>(r#"{"id":1,"type":"teleport"}"#).is_err());
        assert!(serde_json::from_str::<InputMessage>(r#"{"type":"key","key":"KeyA"}"#).is_err());
    }
}
