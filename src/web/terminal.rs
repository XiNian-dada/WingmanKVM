use super::*;

/// Authenticated interactive shell on the RK3399. The PTY is intentionally
/// short-lived and is torn down as soon as the browser disconnects, the
/// session is revoked, or the session deadline passes.
pub(super) async fn terminal_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let public_origin = state.config.read().await.server.public_origin.clone();
    // Keep the same origin and authentication ordering as the upgrade path,
    // then gate PTY creation on the persisted migration marker. The marker is
    // read from disk rather than trusted from a login reply or session state.
    if let Err(error) = validate_terminal_origin(&headers, public_origin.as_deref()) {
        return error.into_response();
    }
    if !is_authenticated(&state, &headers) {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    }
    if !terminal_is_ready(&state) {
        return ApiError::new(StatusCode::SERVICE_UNAVAILABLE, TERMINAL_DEGRADED_MESSAGE)
            .into_response();
    }
    terminal_ws_with_command(
        state.sessions.clone(),
        headers,
        ws,
        public_origin,
        terminal_command(),
    )
    .await
}

pub(super) fn terminal_is_ready(state: &AppState) -> bool {
    state
        .auth
        .load()
        .ok()
        .flatten()
        .is_some_and(|record| record.system_password_synced)
}

pub(super) async fn terminal_ws_with_command(
    sessions: SessionStore,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    public_origin: Option<String>,
    command: CommandBuilder,
) -> Response {
    // Browser Origin is checked before authentication and before on_upgrade.
    // Forwarded scheme headers are deliberately ignored: only the exact
    // configured public origin or the direct HTTP Host can authorize a PTY.
    if let Err(error) = validate_terminal_origin(&headers, public_origin.as_deref()) {
        return error.into_response();
    }
    // Re-validate under the session lock so a logout that raced the auth
    // middleware cannot leave an upgrade with no cancellable session binding.
    let Some(guard) =
        session_token(&headers).and_then(|token| sessions.validate_guard(token))
    else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    ws.max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| handle_terminal(socket, guard, command))
}

pub(super) fn validate_terminal_origin(
    headers: &HeaderMap,
    public_origin: Option<&str>,
) -> Result<(), ApiError> {
    let forbidden = || ApiError::new(StatusCode::FORBIDDEN, "终端 WebSocket 来源校验失败");

    let mut origins = headers.get_all(ORIGIN).iter();
    let Some(origin_header) = origins.next() else {
        return Err(forbidden());
    };
    if origins.next().is_some() {
        return Err(forbidden());
    }
    let origin = origin_header
        .to_str()
        .map_err(|_| forbidden())
        .and_then(|value| config::NormalizedOrigin::parse(value).map_err(|_| forbidden()))?;

    let mut hosts = headers.get_all(HOST).iter();
    let Some(host_header) = hosts.next() else {
        return Err(forbidden());
    };
    if hosts.next().is_some() {
        return Err(forbidden());
    }
    let host = host_header.to_str().map_err(|_| forbidden())?;

    if let Some(configured) = public_origin {
        let expected =
            config::NormalizedOrigin::parse(configured).map_err(|_| forbidden())?;
        if origin != expected || !expected.host_matches(host) {
            return Err(forbidden());
        }
    } else if !origin.is_http() || !origin.host_matches(host) {
        return Err(forbidden());
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum TerminalClientMessage {
    Resize { cols: u16, rows: u16 },
}

pub(super) const TERMINAL_TASK_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct TerminalSession {
    pub(super) child: Box<dyn Child + Send + Sync>,
    pub(super) reader_task: tokio::task::JoinHandle<()>,
    pub(super) reader_rx: mpsc::Receiver<Vec<u8>>,
    pub(super) writer_tx: mpsc::Sender<Vec<u8>>,
    pub(super) writer_task: tokio::task::JoinHandle<()>,
    pub(super) master: Box<dyn MasterPty + Send>,
}

impl TerminalSession {
    /// Kill and reap the child, then close every PTY endpoint so the blocking
    /// reader thread can observe EOF/EIO and return.
    pub(super) async fn stop(self) -> bool {
        let Self {
            child,
            reader_task,
            reader_rx,
            writer_tx,
            writer_task,
            master,
        } = self;
        drop(writer_tx);
        drop(master);
        drop(reader_rx);
        let reaped = reap_terminal_child(child).await;
        // A detached descendant (e.g. `setsid cmd &`) can keep the PTY slave
        // open so the blocking reader never sees EOF. Bound the wait so the
        // connection task is released; the stray thread exits with the slave.
        for (name, task) in [("reader", reader_task), ("writer", writer_task)] {
            if tokio::time::timeout(TERMINAL_TASK_JOIN_TIMEOUT, task).await.is_err() {
                tracing::warn!(task = name, "terminal I/O thread did not exit; abandoning it");
            }
        }
        reaped
    }
}

pub(super) async fn reap_terminal_child(mut child: Box<dyn Child + Send + Sync>) -> bool {
    tokio::task::spawn_blocking(move || {
        let _ = child.kill();
        child.wait().is_ok()
    })
    .await
    .unwrap_or(false)
}

pub(super) fn terminal_command() -> CommandBuilder {
    let mut cmd = if std::path::Path::new("/usr/bin/sudo").exists() {
        let mut c = CommandBuilder::new("sudo");
        c.args(["-n", "-u", "wingman", "/bin/bash", "-l"]);
        c
    } else {
        CommandBuilder::new("/bin/bash")
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("HOME", "/home/wingman");
    cmd.cwd("/");
    cmd
}

pub(super) async fn spawn_terminal_session(command: CommandBuilder) -> Result<TerminalSession, String> {
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 32,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| format!("PTY error: {err}"))?;
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|err| format!("spawn error: {err}"))?;
    drop(pair.slave);
    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(err) => {
            reap_terminal_child(child).await;
            return Err(format!("PTY reader error: {err}"));
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(err) => {
            reap_terminal_child(child).await;
            return Err(format!("PTY writer error: {err}"));
        }
    };
    let master = pair.master;
    let (tx, reader_rx) = mpsc::channel::<Vec<u8>>(32);
    let reader_task = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(32);
    let writer_task = tokio::task::spawn_blocking(move || {
        while let Some(data) = writer_rx.blocking_recv() {
            if std::io::Write::write_all(&mut writer, &data).is_err() {
                break;
            }
        }
    });
    Ok(TerminalSession {
        child,
        reader_task,
        reader_rx,
        writer_tx,
        writer_task,
        master,
    })
}

pub(super) async fn handle_terminal(mut socket: WebSocket, guard: SessionGuard, command: CommandBuilder) {
    // Revocation or expiry may have won the race between the HTTP check and
    // this task being scheduled. Never spawn a PTY for a dead session.
    if !guard.is_active() {
        return;
    }
    let mut session = match spawn_terminal_session(command).await {
        Ok(session) => session,
        Err(message) => {
            let send = socket.send(Message::Text(format!("\r\n{message}\r\n").into()));
            tokio::pin!(send);
            tokio::select! {
                biased;
                _ = guard.wait_for_end() => {}
                _ = &mut send => {}
            }
            return;
        }
    };
    loop {
        tokio::select! {
            biased;
            _ = guard.wait_for_end() => break,
            _ = session.writer_tx.closed() => break,
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(TerminalClientMessage::Resize { cols, rows }) =
                        serde_json::from_str(text.as_str())
                        && (2..=1000).contains(&cols)
                        && (1..=500).contains(&rows)
                    {
                        let _ = session.master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                }
                Some(Ok(Message::Binary(data))) => {
                    tokio::select! {
                        biased;
                        _ = guard.wait_for_end() => break,
                        result = session.writer_tx.send(data.to_vec()) => {
                            if result.is_err() { break; }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            maybe = session.reader_rx.recv() => match maybe {
                Some(data) => {
                    // The send itself must be cancellable: a stalled browser must
                    // not keep a revoked terminal alive indefinitely.
                    let send = socket.send(Message::Binary(data.into()));
                    tokio::pin!(send);
                    tokio::select! {
                        biased;
                        _ = guard.wait_for_end() => break,
                        result = &mut send => {
                            if result.is_err() {
                                break;
                            }
                        }
                    }
                }
                None => break,
            },
        }
    }
    drop(socket);
    session.stop().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn terminal_stop_returns_when_a_detached_child_keeps_the_pty_open() {
        let mut command = CommandBuilder::new("/bin/sh");
        // The background subshell ignores SIGHUP and keeps the slave open.
        command.args(["-c", "(trap '' HUP; sleep 30) & exec sleep 30"]);
        let session = spawn_terminal_session(command).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        tokio::time::timeout(TERMINAL_TASK_JOIN_TIMEOUT * 3, session.stop())
            .await
            .expect("stop must not wait forever on the PTY reader");
    }

    use futures_util::{SinkExt, StreamExt};

    use tokio_tungstenite::tungstenite::{
        Error as WsError, Message as WsMessage, client::IntoClientRequest,
    };

    type TerminalTestSocket =
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    struct TerminalTestServer {
        address: SocketAddr,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TerminalTestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    #[derive(Clone)]
    struct TerminalTestState {
        sessions: SessionStore,
        public_origin: Option<String>,
        program: &'static str,
    }

    async fn terminal_test_ws(
        State(state): State<TerminalTestState>,
        headers: HeaderMap,
        ws: WebSocketUpgrade,
    ) -> Response {
        terminal_ws_with_command(
            state.sessions,
            headers,
            ws,
            state.public_origin,
            CommandBuilder::new(state.program),
        )
        .await
    }

    async fn spawn_terminal_test_server(
        sessions: SessionStore,
        public_origin: Option<String>,
    ) -> TerminalTestServer {
        spawn_terminal_test_server_with_program(sessions, public_origin, "/bin/cat").await
    }

    async fn spawn_terminal_test_server_with_program(
        sessions: SessionStore,
        public_origin: Option<String>,
        program: &'static str,
    ) -> TerminalTestServer {
        let app = Router::new()
            .route("/terminal", get(terminal_test_ws))
            .with_state(TerminalTestState {
                sessions,
                public_origin,
                program,
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local terminal test listener");
        let address = listener.local_addr().expect("local test address");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        TerminalTestServer { address, task }
    }

    fn terminal_request_with_origins(
        address: SocketAddr,
        token: &str,
        origins: &[&str],
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        let uri = format!("ws://{address}/terminal")
            .parse::<axum::http::Uri>()
            .expect("valid terminal test URI");
        let mut request: tokio_tungstenite::tungstenite::handshake::client::Request = uri
            .into_client_request()
            .expect("valid terminal upgrade request");
        request.headers_mut().insert(
            "cookie",
            format!("{SESSION_COOKIE}={token}")
                .parse::<HeaderValue>()
                .expect("valid session cookie"),
        );
        request.headers_mut().remove(ORIGIN);
        for origin in origins {
            request.headers_mut().append(
                ORIGIN,
                HeaderValue::from_str(origin).expect("valid test origin"),
            );
        }
        request
    }

    fn terminal_request(
        address: SocketAddr,
        token: &str,
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        let origin = format!("http://{address}");
        terminal_request_with_origins(address, token, &[origin.as_str()])
    }

    fn with_host(
        mut request: tokio_tungstenite::tungstenite::handshake::client::Request,
        host: &str,
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        request.headers_mut().insert(
            HOST,
            HeaderValue::from_str(host).expect("valid test host"),
        );
        request
    }

    fn with_header(
        mut request: tokio_tungstenite::tungstenite::handshake::client::Request,
        name: &'static str,
        value: &'static str,
    ) -> tokio_tungstenite::tungstenite::handshake::client::Request {
        request
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
        request
    }

    async fn terminal_upgrade_status(
        request: tokio_tungstenite::tungstenite::handshake::client::Request,
    ) -> StatusCode {
        let result =
            tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(request))
                .await
                .expect("terminal upgrade should not time out");
        match result {
            Ok((_socket, response)) => response.status(),
            Err(WsError::Http(response)) => response.status(),
            Err(error) => panic!("unexpected terminal upgrade error: {error}"),
        }
    }

    async fn connect_terminal(
        address: SocketAddr,
        token: &str,
    ) -> TerminalTestSocket {
        let request = terminal_request(address, token);
        let (socket, response) =
            tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(request))
                .await
                .expect("terminal upgrade should not time out")
                .expect("terminal upgrade should succeed");
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        socket
    }

    async fn send_terminal(socket: &mut TerminalTestSocket, data: &[u8]) {
        tokio::time::timeout(
            Duration::from_secs(5),
            socket.send(WsMessage::Binary(data.to_vec().into())),
        )
        .await
        .expect("terminal send should not time out")
        .expect("terminal send should succeed");
    }

    async fn read_terminal_until(socket: &mut TerminalTestSocket, needle: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        while !output.contains(needle) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let message = tokio::time::timeout(remaining, socket.next())
                .await
                .unwrap_or_else(|_| {
                    panic!("timed out waiting for {needle:?}; received {output:?}")
                })
                .unwrap_or_else(|| {
                    panic!("terminal closed before {needle:?}; received {output:?}")
                })
                .unwrap_or_else(|error| {
                    panic!("terminal error before {needle:?}: {error}; received {output:?}")
                });
            match message {
                WsMessage::Text(text) => output.push_str(text.as_str()),
                WsMessage::Binary(data) => {
                    output.push_str(&String::from_utf8_lossy(&data));
                }
                _ => {}
            }
        }
    }

    async fn expect_terminal_closed(socket: &mut TerminalTestSocket) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, socket.next()).await {
                Ok(Some(Ok(WsMessage::Close(_))) | None) | Ok(Some(Err(_))) => return true,
                Ok(Some(Ok(_))) => {}
                Err(_) => return false,
            }
        }
    }

    #[tokio::test]
    async fn terminal_input_interrupts_a_continuously_writing_child() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let server =
            spawn_terminal_test_server_with_program(sessions, None, "/usr/bin/yes").await;
        let mut socket = connect_terminal(server.address, &token).await;
        read_terminal_until(&mut socket, "y").await;
        send_terminal(&mut socket, &[3]).await;
        assert!(
            expect_terminal_closed(&mut socket).await,
            "Ctrl-C must reach the PTY even while terminal output stays ready"
        );
    }

    #[tokio::test]
    async fn terminal_websocket_closes_pre_existing_connections_on_revocation() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let revoked_token = sessions.create().unwrap();
        let live_token = sessions.create().unwrap();
        let server = spawn_terminal_test_server(sessions.clone(), None).await;

        let mut first = connect_terminal(server.address, &revoked_token).await;
        let mut second = connect_terminal(server.address, &revoked_token).await;
        let mut unrelated = connect_terminal(server.address, &live_token).await;

        send_terminal(&mut first, b"first\n").await;
        read_terminal_until(&mut first, "first").await;
        send_terminal(&mut second, b"second\n").await;
        read_terminal_until(&mut second, "second").await;
        send_terminal(&mut unrelated, b"unrelated\n").await;
        read_terminal_until(&mut unrelated, "unrelated").await;

        assert!(sessions.revoke(&revoked_token));
        assert!(
            expect_terminal_closed(&mut first).await,
            "first terminal for the revoked session must close"
        );
        assert!(
            expect_terminal_closed(&mut second).await,
            "second terminal for the revoked session must close"
        );

        // An unrelated authenticated session must keep working.
        send_terminal(&mut unrelated, b"still-live\n").await;
        read_terminal_until(&mut unrelated, "still-live").await;

        let _ = unrelated.send(WsMessage::Close(None)).await;
    }

    #[tokio::test]
    async fn terminal_websocket_closes_at_natural_session_expiry() {
        let sessions = SessionStore::new(Duration::from_secs(1));
        let token = sessions.create().unwrap();
        let server = spawn_terminal_test_server(sessions.clone(), None).await;
        let mut socket = connect_terminal(server.address, &token).await;

        assert!(
            expect_terminal_closed(&mut socket).await,
            "an existing terminal must close at the session deadline without pruning"
        );
        // The expired entry may remain until validation or pruning removes it.
        assert!(sessions.validate_guard(&token).is_none());
    }

    #[tokio::test]
    async fn terminal_websocket_rejects_revoked_before_upgrade() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        assert!(sessions.revoke(&token));
        let server = spawn_terminal_test_server(sessions.clone(), None).await;
        let request = terminal_request(server.address, &token);

        let result =
            tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(request))
                .await
                .expect("revoked upgrade should not time out");
        match result {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            Err(error) => panic!("unexpected revoked upgrade error: {error}"),
            Ok(_) => panic!("revoked session must not receive a terminal upgrade"),
        }
    }

    #[tokio::test]
    async fn terminal_websocket_requires_strict_same_origin_without_public_origin() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let server = spawn_terminal_test_server(sessions.clone(), None).await;
        let address = server.address;
        let legitimate = format!("http://{address}");
        let https_origin = format!("https://{address}");
        let different_port = format!("http://127.0.0.1:{}", address.port() + 1);
        let comma_separated = format!("{legitimate},{legitimate}");

        assert_eq!(
            terminal_upgrade_status(terminal_request(address, &token)).await,
            StatusCode::SWITCHING_PROTOCOLS
        );
        for origins in [
            Vec::<&str>::new(),
            vec!["null"],
            vec!["not-an-origin"],
            vec!["http://[2001:db8::1"],
            vec![different_port.as_str()],
            vec![https_origin.as_str()],
            vec![legitimate.as_str(), legitimate.as_str()],
            vec![comma_separated.as_str()],
        ] {
            assert_eq!(
                terminal_upgrade_status(terminal_request_with_origins(
                    address, &token, &origins
                ))
                .await,
                StatusCode::FORBIDDEN,
                "untrusted origin must be rejected: {origins:?}"
            );
        }

        let spoofed_scheme = with_header(
            terminal_request_with_origins(address, &token, &[https_origin.as_str()]),
            "x-forwarded-proto",
            "https",
        );
        assert_eq!(
            terminal_upgrade_status(spoofed_scheme).await,
            StatusCode::FORBIDDEN
        );

        let mismatched_host = with_host(terminal_request(address, &token), "evil.invalid");
        assert_eq!(
            terminal_upgrade_status(mismatched_host).await,
            StatusCode::FORBIDDEN
        );

        let invalid_host = with_host(terminal_request(address, &token), "bad_host");
        assert_eq!(
            terminal_upgrade_status(invalid_host).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn terminal_websocket_requires_exact_configured_public_origin() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let server = spawn_terminal_test_server(
            sessions.clone(),
            Some("HTTPS://KVM.Example.COM:443".to_owned()),
        )
        .await;
        let address = server.address;
        let direct_origin = format!("http://{address}");

        let exact = with_host(
            terminal_request_with_origins(
                address,
                &token,
                &["https://kvm.example.com"],
            ),
            "kvm.example.com",
        );
        assert_eq!(
            terminal_upgrade_status(exact).await,
            StatusCode::SWITCHING_PROTOCOLS
        );

        for (origin, host) in [
            ("http://kvm.example.com", "kvm.example.com"),
            ("https://other.example.com", "kvm.example.com"),
            ("https://kvm.example.com:8443", "kvm.example.com:8443"),
            ("https://kvm.example.com", "other.example.com"),
        ] {
            let request = with_host(
                terminal_request_with_origins(address, &token, &[origin]),
                host,
            );
            assert_eq!(
                terminal_upgrade_status(request).await,
                StatusCode::FORBIDDEN,
                "configured origin mismatch must be rejected: {origin} / {host}"
            );
        }

        let direct = with_host(
            terminal_request_with_origins(address, &token, &[direct_origin.as_str()]),
            &address.to_string(),
        );
        assert_eq!(
            terminal_upgrade_status(direct).await,
            StatusCode::FORBIDDEN,
            "a plain direct origin must not bypass a configured public origin"
        );

        let duplicate = with_host(
            terminal_request_with_origins(
                address,
                &token,
                &["https://kvm.example.com", "https://kvm.example.com"],
            ),
            "kvm.example.com",
        );
        assert_eq!(
            terminal_upgrade_status(duplicate).await,
            StatusCode::FORBIDDEN
        );
    }
}
