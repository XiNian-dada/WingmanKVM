use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    io::Read,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Body,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, DefaultBodyLimit, Form, Multipart, Path as AxumPath, Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, LOCATION, ORIGIN, SET_COOKIE},
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use bytes::Bytes;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, RwLock, Semaphore, mpsc, watch};
use tokio::{fs, io::AsyncWriteExt};
use webauthn_rs::prelude::*;

use crate::proxy_protocol::ProxyProtocolMode;
use crate::client_ip::{self, ClientAddress, ClientConnection};

use crate::{
    auth::{AuthError, AuthRecord, AuthStore, PasswordPolicyError, SessionGuard, SessionStore},
    config::{
        self, CONFIG_VERSION, Config, GpioBias, GpioInputConfig, GpioPulseConfig, H264Encoder,
        HidConfig, PointerMode, VideoEncoding,
    },
    devices::{
        discovery,
        hid::{
            AbsolutePointerRequest, HidError, HidManager, KeyRequest, MouseClickRequest,
            MouseMoveRequest, MouseScrollRequest,
        },
        media::{MediaConfigSnapshot, MediaError, MediaManager, MediaType, sanitize_upload_name},
        power::{
            GpioPulseConfigSnapshot, PowerConfigSnapshot, PowerError, PowerLedConfigSnapshot,
            PowerManager, PowerPress,
        },
        video::VideoManager,
        webrtc::{WebRtcError, WebRtcManager, WebRtcOfferRequest, WebRtcStatus},
    },
    web_ui::{ICON_SVG, INDEX_HTML},
};

const SESSION_COOKIE: &str = "wingman_session";
const SESSION_COOKIE_MAX_AGE_SECONDS: u32 = 43_200;
const XTERM_JS: &str = include_str!("../web/vendor/xterm/xterm.js");
const XTERM_FIT_JS: &str = include_str!("../web/vendor/xterm/addon-fit.js");
const XTERM_CSS: &str = include_str!("../web/vendor/xterm/xterm.css");
const GPIO_TEST_PULSE_MS: u64 = 150;
const PASSKEY_CHALLENGE_TTL: Duration = Duration::from_secs(120);

#[derive(Clone)]
enum PasskeyChallengeState {
    Registration {
        state: PasskeyRegistration,
        rp_id: String,
        rp_origin: Url,
        expires_at: Instant,
    },
    Authentication {
        state: PasskeyAuthentication,
        rp_id: String,
        rp_origin: Url,
        expires_at: Instant,
    },
}

/// Bound on outstanding WebAuthn challenges; unauthenticated login starts
/// must not grow server memory without limit.
const MAX_PASSKEY_CHALLENGES: usize = 256;

fn insert_passkey_challenge(
    challenges: &Mutex<HashMap<String, PasskeyChallengeState>>,
    challenge_id: String,
    challenge: PasskeyChallengeState,
    now: Instant,
) -> Result<(), ApiError> {
    let mut map = challenges.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, state| match state {
        PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
        PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
    });
    if map.len() >= MAX_PASSKEY_CHALLENGES {
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Passkey 请求过多，请稍后重试"));
    }
    map.insert(challenge_id, challenge);
    Ok(())
}

fn prune_passkey_challenges(challenges: &Arc<Mutex<HashMap<String, PasskeyChallengeState>>>) {
    let now = Instant::now();
    let mut map = challenges.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, state| match state {
        PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
        PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
    });
}

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<Config>>,
    proxy_protocol_updates: watch::Sender<ProxyProtocolMode>,
    config_path: Arc<PathBuf>,
    auth: AuthStore,
    sessions: SessionStore,
    setup: Arc<AsyncMutex<()>>,
    config_update: Arc<AsyncMutex<()>>,
    password_migration: Arc<AsyncMutex<()>>,
    setup_token: Arc<Mutex<Option<String>>>,
    setup_scan: Arc<AsyncMutex<Option<(Instant, discovery::DeviceDiscovery)>>>,
    login_limiter: LoginLimiter,
    passkey_start_limiter: LoginLimiter,
    passkey_challenges: Arc<Mutex<HashMap<String, PasskeyChallengeState>>>,
    hid: HidManager,
    power: PowerManager,
    media: Arc<MediaManager>,
    media_upload: Arc<AsyncMutex<()>>,
    video: VideoManager,
    webrtc: WebRtcManager,
    system: Arc<crate::system::SystemMonitor>,
}

impl AppState {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = config::default_config_path();
        let config = Config::load_or_default(&config_path)?;
        let state_dir = config::state_dir();
        let auth = AuthStore::new(state_dir.join("auth.json"));
        let setup_token = if auth.is_initialized()? {
            None
        } else {
            let token = generate_token()
                .map_err(|error| anyhow::anyhow!("secure random generation failed: {error}"))?;
            tracing::warn!(
                setup_token = %token,
                "首次初始化需要此令牌；创建管理员后令牌立即失效"
            );
            Some(token)
        };
        let video = VideoManager::new(config.video.clone());
        let webrtc = WebRtcManager::new(video.clone(), config.video.h264.clone());
        let power = PowerManager::new();
        let power_led = power_led_snapshot(&config.power);
        let power_for_led = power.clone();
        tokio::spawn(async move {
            power_for_led.set_power_led_config(power_led).await;
        });
        let sessions = SessionStore::default();
        let session_cleanup = sessions.clone();
        let passkey_challenges = Arc::new(Mutex::new(HashMap::new()));
        let passkey_cleanup = passkey_challenges.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10 * 60));
            loop {
                interval.tick().await;
                session_cleanup.prune_expired();
                prune_passkey_challenges(&passkey_cleanup);
            }
        });

        let hid = HidManager::with_keyboard_device(config.hid.keyboard_device.clone());

        Ok(Self {
            proxy_protocol_updates: watch::channel(config.server.proxy_protocol).0,
            config: Arc::new(RwLock::new(config)),
            config_path: Arc::new(config_path),
            auth,
            sessions,
            setup: Arc::new(AsyncMutex::new(())),
            config_update: Arc::new(AsyncMutex::new(())),
            password_migration: Arc::new(AsyncMutex::new(())),
            setup_token: Arc::new(Mutex::new(setup_token)),
            setup_scan: Arc::new(AsyncMutex::new(None)),
            login_limiter: LoginLimiter::default(),
            passkey_start_limiter: LoginLimiter::with_budget(MAX_PASSKEY_STARTS_PER_IP),
            passkey_challenges,
            hid,
            power,
            media: Arc::new(MediaManager::default()),
            media_upload: Arc::new(AsyncMutex::new(())),
            video,
            webrtc,
            system: Arc::new(crate::system::SystemMonitor::new()),
        })
    }

    pub async fn server_address(&self) -> anyhow::Result<SocketAddr> {
        let config = self.config.read().await;
        Ok(config.server.socket_address()?)
    }

    pub async fn proxy_protocol_mode(&self) -> ProxyProtocolMode {
        let config = self.config.read().await;
        config.server.proxy_protocol
    }

    pub fn proxy_protocol_updates(&self) -> watch::Receiver<ProxyProtocolMode> {
        self.proxy_protocol_updates.subscribe()
    }

    async fn client_address(&self, connection: ClientConnection, headers: &HeaderMap) -> ClientAddress {
        let config = self.config.read().await;
        client_ip::resolve(connection, headers, &config.server.trusted_http_proxies)
    }
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/video_feed", get(video_feed))
        .route("/api/status", get(status))
        .route("/api/system", get(system_status))
        .route("/api/devices/scan", get(scan_devices).post(scan_devices))
        .route(
            "/api/config",
            get(get_config).put(update_config).post(patch_config),
        )
        .route("/api/logout", post(logout))
        .route("/api/key", post(key))
        .route("/api/mouse/move", post(mouse_move))
        .route("/api/mouse/absolute", post(mouse_absolute))
        .route("/api/mouse/click", post(mouse_click))
        .route("/api/mouse/scroll", post(mouse_scroll))
        .route("/api/input/release-all", post(release_all))
        .route("/api/hid/leds", get(get_keyboard_leds))
        .route("/api/gpio/test", post(gpio_test))
        .route("/api/power/test", post(gpio_test))
        .route(
            "/api/webrtc/offer",
            post(webrtc_offer).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/api/webrtc/close", post(webrtc_close))
        .route("/api/terminal/ws", get(terminal_ws))
        .route("/power", post(power))
        .route("/reset", post(reset))
        .route("/api/media", get(list_media))
        .route(
            "/api/media/upload",
            post(upload_media).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/media/attach", post(attach_media))
        .route("/api/media/detach", post(detach_media))
        .route("/api/passkey/register/start", post(passkey_register_start))
        .route("/api/passkey/register/finish", post(passkey_register_finish))
        .route("/api/passkey/list", get(passkey_list))
        .route("/api/passkey/delete", post(passkey_delete))
        .route("/api/passkey/{id}", delete(passkey_delete_path))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/assets/icon.svg", get(favicon))
        .route("/healthz", get(health))
        .route("/assets/xterm.js", get(xterm_js))
        .route("/assets/xterm-fit.js", get(xterm_fit_js))
        .route("/assets/xterm.css", get(xterm_css))
        .route("/api/bootstrap", get(bootstrap))
        .route(
            "/api/setup/devices",
            get(scan_setup_devices).post(scan_setup_devices),
        )
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/passkey/login/start", post(passkey_login_start))
        .route("/api/passkey/login/finish", post(passkey_login_finish))
        .merge(protected)
        .with_state(state)
}

async fn xterm_js() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/javascript; charset=utf-8")], XTERM_JS)
}

async fn xterm_fit_js() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/javascript; charset=utf-8")],
        XTERM_FIT_JS,
    )
}

async fn xterm_css() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], XTERM_CSS)
}

/// Authenticated interactive shell on the RK3399. The PTY is intentionally
/// short-lived and is torn down as soon as the browser disconnects, the
/// session is revoked, or the session deadline passes.
async fn terminal_ws(
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

fn terminal_is_ready(state: &AppState) -> bool {
    state
        .auth
        .load()
        .ok()
        .flatten()
        .is_some_and(|record| record.system_password_synced)
}

async fn terminal_ws_with_command(
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

fn validate_terminal_origin(
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
enum TerminalClientMessage {
    Resize { cols: u16, rows: u16 },
}

struct TerminalSession {
    child: Box<dyn Child + Send + Sync>,
    reader_task: tokio::task::JoinHandle<()>,
    reader_rx: mpsc::Receiver<Vec<u8>>,
    writer_tx: mpsc::Sender<Vec<u8>>,
    writer_task: tokio::task::JoinHandle<()>,
    master: Box<dyn MasterPty + Send>,
}

impl TerminalSession {
    /// Kill and reap the child, then close every PTY endpoint so the blocking
    /// reader thread can observe EOF/EIO and return.
    async fn stop(self) -> bool {
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
        let _ = reader_task.await;
        let _ = writer_task.await;
        reaped
    }
}

async fn reap_terminal_child(mut child: Box<dyn Child + Send + Sync>) -> bool {
    tokio::task::spawn_blocking(move || {
        let _ = child.kill();
        child.wait().is_ok()
    })
    .await
    .unwrap_or(false)
}

fn terminal_command() -> CommandBuilder {
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

async fn spawn_terminal_session(command: CommandBuilder) -> Result<TerminalSession, String> {
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

async fn handle_terminal(mut socket: WebSocket, guard: SessionGuard, command: CommandBuilder) {
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

async fn index() -> impl IntoResponse {
    no_store(Html(INDEX_HTML))
}

async fn favicon() -> impl IntoResponse {
    ([(CONTENT_TYPE, "image/svg+xml")], ICON_SVG)
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "wingmankvm"}))
}

#[derive(Serialize)]
struct BootstrapResponse {
    setup_required: bool,
    token_required: bool,
    authenticated: bool,
    has_passkeys: bool,
    terminal_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_ip: Option<String>,
    client: ClientAddress,
    config: Option<Config>,
    video: Option<crate::devices::video::VideoStatus>,
    webrtc: Option<WebRtcStatus>,
    capabilities: Option<Capabilities>,
}

#[derive(Serialize)]
struct Capabilities {
    video: bool,
    keyboard: bool,
    mouse: bool,
    mouse_relative: bool,
    mouse_absolute: bool,
    pointer_mode: PointerMode,
    gpio_power: bool,
    gpio_reset: bool,
    gpio_power_led: bool,
    mass_storage: bool,
    video_passthrough: bool,
    video_transcode: bool,
    video_webrtc_h264: bool,
}

async fn load_bootstrap_auth(auth: AuthStore) -> Result<(bool, Option<AuthRecord>), String> {
    tokio::task::spawn_blocking(move || Ok::<_, AuthError>((auth.is_initialized()?, auth.load()?)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

async fn bootstrap(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Response {
    let client = state.client_address(connection, &headers).await;
    // An unreadable auth store is not "first run": report it instead of
    // steering the operator into a setup flow whose token is already gone.
    let (initialized, record) = match load_bootstrap_auth(state.auth.clone()).await {
        Ok(stored) => stored,
        Err(error) => {
            tracing::error!(%error, "authentication store is unreadable");
            return ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "认证数据不可读，请检查 auth.json 的权限与服务日志",
            )
            .into_response();
        }
    };
    let setup_required = !initialized;
    let authenticated = !setup_required && is_authenticated(&state, &headers);
    let has_passkeys = record
        .as_ref()
        .is_some_and(|record| !record.passkeys.is_empty());
    let terminal_ready = authenticated
        && record
            .as_ref()
            .is_some_and(|record| record.system_password_synced);
    let terminal_message = (authenticated && !terminal_ready)
        .then(|| TERMINAL_DEGRADED_MESSAGE.to_owned());
    let (config, video, webrtc, capabilities) = if authenticated {
        let config = state.config.read().await.clone();
        let webrtc = state.webrtc.status();
        let capabilities = capabilities_with_webrtc(&config, webrtc.available);
        (
            Some(config),
            Some(state.video.status()),
            Some(webrtc),
            Some(capabilities),
        )
    } else {
        (None, None, None, None)
    };
    no_store(Json(BootstrapResponse {
        setup_required,
        token_required: setup_required,
        authenticated,
        has_passkeys,
        terminal_ready,
        terminal_message,
        client_ip: Some(client.ip.to_string()),
        client,
        config,
        video,
        webrtc,
        capabilities,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct SetupRequest {
    username: String,
    password: String,
    setup_token: String,
    #[serde(default)]
    video_device: Option<String>,
    #[serde(default)]
    keyboard_device: Option<String>,
    #[serde(default)]
    mouse_device: Option<String>,
    #[serde(default)]
    absolute_pointer_device: Option<String>,
    #[serde(default)]
    pointer_mode: Option<PointerMode>,
    #[serde(default)]
    power_enabled: Option<bool>,
    #[serde(default)]
    gpio_chip: Option<String>,
    #[serde(default)]
    gpio_line: Option<u32>,
    #[serde(default)]
    active_high: Option<bool>,
    #[serde(default)]
    media_enabled: Option<bool>,
    #[serde(default)]
    lun_path: Option<String>,
    #[serde(default)]
    image_directory: Option<String>,
}

async fn setup(
    State(state): State<AppState>,
    Json(request): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    let setup_guard = state.setup.clone().lock_owned().await;
    if state.auth.is_initialized().map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::CONFLICT, "管理员已经初始化"));
    }
    let valid_token = {
        let token = state
            .setup_token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        token
            .as_deref()
            .is_some_and(|expected| constant_time_eq(expected, &request.setup_token))
    };
    if !valid_token {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "初始化令牌不正确"));
    }

    let discovered = discovery::scan().await;
    let mut new_config = state.config.read().await.clone();
    new_config.video.device =
        resolve_setup_video(optional_path(request.video_device), &discovered.video)?;
    new_config.video.auto_detect = new_config.video.device.is_none();
    new_config.hid.keyboard_device = resolve_setup_hid(
        optional_path(request.keyboard_device),
        SetupHidRole::Keyboard,
        &discovered,
    )?;
    // New clients always send an explicit mode. If an older client omits it,
    // infer the legacy choice from the path it supplied, otherwise prefer the
    // new absolute-pointer default.
    let pointer_mode = request.pointer_mode.unwrap_or_else(|| {
        if request.absolute_pointer_device.is_some() {
            PointerMode::Absolute
        } else if request.mouse_device.is_some() {
            PointerMode::Relative
        } else {
            PointerMode::Absolute
        }
    });
    let requested_mouse = optional_path(request.mouse_device);
    let requested_absolute = optional_path(request.absolute_pointer_device);
    new_config.hid.mouse_device = match pointer_mode {
        PointerMode::Relative | PointerMode::Auto => {
            resolve_setup_hid(requested_mouse, SetupHidRole::Mouse, &discovered)?
        }
        PointerMode::Absolute => None,
    };
    new_config.hid.absolute_pointer_device = match pointer_mode {
        PointerMode::Absolute | PointerMode::Auto => resolve_setup_hid(
            requested_absolute,
            SetupHidRole::AbsolutePointer,
            &discovered,
        )?,
        PointerMode::Relative => None,
    };
    new_config.hid.pointer_mode = pointer_mode;
    new_config.hid.auto_detect = hid_needs_auto_detection(&new_config.hid);
    new_config.power.gpio_chip = optional_string(request.gpio_chip);
    new_config.power.gpio_line = request.gpio_line;
    new_config.power.active_high = request.active_high.unwrap_or(new_config.power.active_high);
    new_config.power.enabled = request
        .power_enabled
        .unwrap_or(new_config.power.gpio_chip.is_some() && new_config.power.gpio_line.is_some());
    let requested_lun = optional_path(request.lun_path);
    let media_enabled = request.media_enabled.unwrap_or(requested_lun.is_some());
    new_config.media.lun_path =
        resolve_setup_lun(requested_lun, media_enabled, &discovered.mass_storage_luns)?;
    new_config.media.image_directory =
        optional_path(request.image_directory).or_else(|| Some(config::state_dir().join("images")));
    new_config.media.enabled = media_enabled;
    validate_config(&new_config)?;
    let username = request.username;
    let password = request.password;

    // Validate the username, enforce the complete password policy and finish
    // the expensive hash before changing the local maintenance account.
    let auth_record = {
        let password = password.clone();
        tokio::task::spawn_blocking(move || AuthRecord::new(username, &password))
            .await
            .map_err(ApiError::internal)?
            .map_err(map_auth_error)?
    };

    // Keep the complete initialization transaction and its lock alive if the
    // HTTP client disconnects after privileged password synchronization starts.
    tokio::spawn(async move {
        let _setup_guard = setup_guard;
        finish_setup(state, new_config, auth_record, password).await
    })
    .await
    .map_err(ApiError::internal)?
}

async fn finish_setup(
    state: AppState,
    new_config: Config,
    auth_record: AuthRecord,
    password: String,
) -> Result<Response, ApiError> {
    persist_config(state.config_path.as_ref(), &new_config).await?;

    // Keep the local maintenance account in sync with the web administrator.
    // The account is deliberately fixed to `wingman`; never interpolate a
    // user-supplied name into a shell command. A helper failure must not abort
    // setup: KVM control is still usable, only the browser terminal is gated.
    let terminal_ready = match sync_system_password(&password).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "system password synchronization failed during setup");
            false
        }
    };
    let mut auth_record = auth_record;
    auth_record.system_password_synced = terminal_ready;

    let auth = state.auth.clone();
    tokio::task::spawn_blocking(move || auth.initialize_record(auth_record))
        .await
        .map_err(ApiError::internal)?
        .map_err(map_auth_error)?;

    *state.config.write().await = new_config.clone();
    state
        .video
        .reconfigure(new_config.video.clone())
        .map_err(ApiError::internal)?;
    state.webrtc.reconfigure(new_config.video.h264).await;
    state
        .power
        .set_power_led_config(power_led_snapshot(&new_config.power))
        .await;
    state
        .setup_token
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let session = state.sessions.create().map_err(ApiError::internal)?;
    let session_secure = new_config.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "terminal_ready": terminal_ready,
            "terminal_message": (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE),
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

fn candidate_is_ready(candidate: &discovery::DeviceCandidate) -> bool {
    candidate.compatible == Some(true)
        && candidate.gadget_bound == Some(true)
        && candidate.function_linked == Some(true)
}

fn resolve_setup_video(
    requested: Option<PathBuf>,
    candidates: &[discovery::DeviceCandidate],
) -> Result<Option<PathBuf>, ApiError> {
    if let Some(path) = requested {
        let candidate = find_candidate(&path, candidates).ok_or_else(|| {
            ApiError::bad_request("所选视频设备不在本次扫描结果中，请重新扫描后再试")
        })?;
        validate_video_candidate(candidate)?;
        return Ok(Some(path));
    }

    unique_usable_candidate(
        candidates,
        video_candidate_is_usable,
        "检测到多个可用的 MJPEG 视频设备，请在设备列表中选择一个",
    )
}

fn validate_video_candidate(candidate: &discovery::DeviceCandidate) -> Result<(), ApiError> {
    if candidate.video_capture == Some(false) {
        return Err(ApiError::bad_request("所选视频设备不支持 V4L2 视频采集"));
    }
    if candidate.supports_mjpeg == Some(false) {
        return Err(ApiError::bad_request(
            "所选视频设备不支持 MJPEG，无法用于当前视频流",
        ));
    }
    if !video_candidate_is_usable(candidate) {
        return Err(ApiError::bad_request(
            "无法确认所选视频设备的采集与 MJPEG 能力，请检查设备权限后重新扫描",
        ));
    }
    Ok(())
}

fn video_candidate_is_usable(candidate: &discovery::DeviceCandidate) -> bool {
    video_candidate_is_usable_with_probe_requirement(candidate, cfg!(target_os = "linux"))
}

fn video_candidate_is_usable_with_probe_requirement(
    candidate: &discovery::DeviceCandidate,
    require_verified_probe: bool,
) -> bool {
    if require_verified_probe {
        candidate.video_capture == Some(true) && candidate.supports_mjpeg == Some(true)
    } else {
        candidate.video_capture != Some(false) && candidate.supports_mjpeg != Some(false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SetupHidRole {
    Keyboard,
    Mouse,
    AbsolutePointer,
}

impl SetupHidRole {
    fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "键盘设备",
            Self::Mouse => "相对鼠标设备",
            Self::AbsolutePointer => "绝对指针设备",
        }
    }

    fn audit_name(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Mouse => "relative_mouse",
            Self::AbsolutePointer => "absolute_pointer",
        }
    }

    fn candidates(self, discovered: &discovery::DeviceDiscovery) -> &[discovery::DeviceCandidate] {
        match self {
            Self::Keyboard => &discovered.keyboard,
            Self::Mouse => &discovered.mouse,
            Self::AbsolutePointer => &discovered.absolute_pointer,
        }
    }
}

fn resolve_setup_hid(
    requested: Option<PathBuf>,
    role: SetupHidRole,
    discovered: &discovery::DeviceDiscovery,
) -> Result<Option<PathBuf>, ApiError> {
    let candidates = role.candidates(discovered);
    let Some(path) = requested else {
        return unique_usable_candidate(
            candidates,
            candidate_is_ready,
            format!("检测到多个可用的{}，请在设备列表中选择一个", role.label()),
        );
    };

    if let Some(candidate) = find_candidate(&path, candidates) {
        validate_gadget_candidate(role.label(), candidate)?;
        return Ok(Some(path));
    }

    if let Some(actual_role) = recognized_hid_role(&path, discovered) {
        return Err(ApiError::bad_request(format!(
            "所选{}实际被识别为{}，请重新选择",
            role.label(),
            actual_role.label()
        )));
    }

    // Some kernels do not expose the configfs `dev` attribute needed to map
    // hidg nodes back to functions. An explicit advanced choice may use a
    // node found by the generic HID scan, but it is never selected
    // automatically and the unverified role mapping is always audited.
    if find_candidate(&path, &discovered.hid).is_some() {
        tracing::warn!(
            path = %path.display(),
            requested_role = role.audit_name(),
            "首次设置采用无法从 configfs 验证角色的 HID 高级配置"
        );
        return Ok(Some(path));
    }

    Err(ApiError::bad_request(format!(
        "所选{}不在本次 HID 扫描结果中，请重新扫描后再试",
        role.label()
    )))
}

fn recognized_hid_role(
    path: &Path,
    discovered: &discovery::DeviceDiscovery,
) -> Option<SetupHidRole> {
    [
        SetupHidRole::Keyboard,
        SetupHidRole::Mouse,
        SetupHidRole::AbsolutePointer,
    ]
    .into_iter()
    .find(|role| find_candidate(path, role.candidates(discovered)).is_some())
}

fn resolve_setup_lun(
    requested: Option<PathBuf>,
    enabled: bool,
    candidates: &[discovery::DeviceCandidate],
) -> Result<Option<PathBuf>, ApiError> {
    if let Some(path) = requested {
        let candidate = find_candidate(&path, candidates).ok_or_else(|| {
            ApiError::bad_request("所选虚拟介质 LUN 不在本次扫描结果中，请重新扫描后再试")
        })?;
        validate_gadget_candidate("虚拟介质 LUN", candidate)?;
        return Ok(Some(path));
    }
    if !enabled {
        return Ok(None);
    }

    unique_usable_candidate(
        candidates,
        candidate_is_ready,
        "检测到多个可用的 USB 虚拟介质，请在设备列表中选择一个",
    )?
    .ok_or_else(|| ApiError::bad_request("未发现可用的 USB 虚拟介质，请先运行安装程序"))
    .map(Some)
}

fn validate_gadget_candidate(
    label: &str,
    candidate: &discovery::DeviceCandidate,
) -> Result<(), ApiError> {
    match candidate.compatible {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}与 WingmanKVM 所需的 USB 描述符不兼容"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}的 USB 描述符，请重新扫描"
            )));
        }
    }
    match candidate.function_linked {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}尚未链接到 USB Gadget 配置"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}是否已链接到 USB Gadget 配置，请重新扫描"
            )));
        }
    }
    match candidate.gadget_bound {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}所属 Gadget 尚未绑定 UDC"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}所属 Gadget 的 UDC 状态，请重新扫描"
            )));
        }
    }
    Ok(())
}

fn unique_usable_candidate(
    candidates: &[discovery::DeviceCandidate],
    usable: impl Fn(&discovery::DeviceCandidate) -> bool,
    multiple_message: impl Into<String>,
) -> Result<Option<PathBuf>, ApiError> {
    let mut usable = candidates.iter().filter(|candidate| usable(candidate));
    let Some(candidate) = usable.next() else {
        return Ok(None);
    };
    if usable.next().is_some() {
        return Err(ApiError::bad_request(multiple_message));
    }
    Ok(Some(candidate.path.clone()))
}

fn find_candidate<'a>(
    requested: &Path,
    candidates: &'a [discovery::DeviceCandidate],
) -> Option<&'a discovery::DeviceCandidate> {
    candidates
        .iter()
        .find(|candidate| paths_refer_to_same_node(requested, &candidate.path))
}

fn paths_refer_to_same_node(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

const SYSTEM_PASSWORD_SYNC_DEADLINE: Duration = Duration::from_secs(5);
const MAX_SYSTEM_PASSWORD_BYTES: usize = 1024;
const TERMINAL_DEGRADED_MESSAGE: &str =
    "终端凭证尚未同步，终端当前不可用；请退出后重新登录以重试。KVM 画面与输入不受影响。";

#[derive(Debug, thiserror::Error)]
enum PasswordSyncError {
    #[error("system password is not valid for helper synchronization")]
    InvalidPassword,
    #[error("failed to start system password helper")]
    Spawn,
    #[error("failed to write system password helper request")]
    Write,
    #[error("system password helper exited unsuccessfully")]
    Exit,
    #[error("system password helper timed out")]
    Timeout,
    #[error("system password helper supervisor stopped")]
    Supervisor,
}

#[derive(Clone, Debug)]
struct SystemPasswordHelper {
    program: PathBuf,
    args: Vec<String>,
    deadline: Duration,
}

impl Default for SystemPasswordHelper {
    fn default() -> Self {
        Self {
            program: PathBuf::from("sudo"),
            args: vec![
                "-n".to_owned(),
                "/usr/local/sbin/wingmankvm-set-wingman-password".to_owned(),
            ],
            deadline: SYSTEM_PASSWORD_SYNC_DEADLINE,
        }
    }
}

impl SystemPasswordHelper {
    fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        command
    }
}

async fn sync_system_password(password: &str) -> Result<(), PasswordSyncError> {
    sync_system_password_with(password, &SystemPasswordHelper::default()).await
}

/// Run the helper under a supervisor task that owns the child process. If the
/// HTTP request is cancelled, the supervisor still enforces the deadline, kills
/// the child and reaps it. The deadline covers both the stdin write and wait.
async fn sync_system_password_with(
    password: &str,
    helper: &SystemPasswordHelper,
) -> Result<(), PasswordSyncError> {
    if password.is_empty()
        || password.len() > MAX_SYSTEM_PASSWORD_BYTES
        || password
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r' | 0))
    {
        return Err(PasswordSyncError::InvalidPassword);
    }
    let password = password.to_owned();
    let helper = helper.clone();
    tokio::spawn(async move { supervise_system_password_sync(&helper, &password).await })
        .await
        .map_err(|_| PasswordSyncError::Supervisor)?
}

async fn supervise_system_password_sync(
    helper: &SystemPasswordHelper,
    password: &str,
) -> Result<(), PasswordSyncError> {
    let mut child = helper.command().spawn().map_err(|_| PasswordSyncError::Spawn)?;
    let Some(mut stdin) = child.stdin.take() else {
        kill_and_reap_system_password_child(&mut child).await;
        return Err(PasswordSyncError::Write);
    };
    let exchange = async {
        stdin.write_all(password.as_bytes()).await?;
        stdin.shutdown().await?;
        drop(stdin);
        child.wait().await
    };
    match tokio::time::timeout(helper.deadline, exchange).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => Err(PasswordSyncError::Exit),
        Ok(Err(_)) => {
            kill_and_reap_system_password_child(&mut child).await;
            Err(PasswordSyncError::Write)
        }
        Err(_) => {
            kill_and_reap_system_password_child(&mut child).await;
            Err(PasswordSyncError::Timeout)
        }
    }
}

async fn migrate_system_password_on_login(
    migration: Arc<AsyncMutex<()>>,
    auth: AuthStore,
    verified: AuthRecord,
    password: String,
    helper: SystemPasswordHelper,
) -> bool {
    let Ok(guard) = migration.try_lock_owned() else {
        // Do not queue login attempts behind a slow helper.
        return false;
    };
    tokio::spawn(supervise_system_password_migration(
        auth, verified, password, helper, guard,
    ))
    .await
    .unwrap_or(false)
}

async fn supervise_system_password_migration(
    auth: AuthStore,
    verified: AuthRecord,
    password: String,
    helper: SystemPasswordHelper,
    guard: tokio::sync::OwnedMutexGuard<()>,
) -> bool {
    let _migration_guard = guard;
    let Ok(Some(current)) = auth.load() else {
        return false;
    };
    if current.username != verified.username
        || current.password_hash != verified.password_hash
    {
        return false;
    }
    if current.system_password_synced {
        return true;
    }
    if let Err(error) = sync_system_password_with(&password, &helper).await {
        tracing::warn!(%error, "system password migration failed during login");
        return false;
    }
    match auth.update(|record| {
        if record.username == verified.username
            && record.password_hash == verified.password_hash
        {
            record.mark_system_password_synced()?;
            Ok(true)
        } else {
            // The password changed while the helper was running; do not
            // claim synchronization.
            Ok(false)
        }
    }) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to persist system password migration marker"
            );
            false
        }
    }
}

#[cfg(unix)]
fn signal_password_process_group(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        if pid > 1 {
            // The unreaped child is the leader of the private group created
            // by process_group(0). Signal it directly without spawning a tool.
            unsafe { libc::kill(-pid, libc::SIGKILL); }
        }
    }
}

async fn kill_and_reap_system_password_child(child: &mut tokio::process::Child) {
    // Signal the task-owned group before reaping the leader so its id cannot be
    // reused for a different group. process_group(0) makes the child the leader
    // of a fresh group, never the server's shared group.
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            signal_password_process_group(pid);
        }
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Reserve at admission so a burst cannot retry around the per-IP budget
    // merely because the global password workers are busy.
    if !state.login_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }
    let Some(password_slot) = state.login_limiter.try_acquire_password_slot() else {
        return Err(too_many_login_attempts());
    };
    let auth = state.auth.clone();
    let username = request.username;
    let password = request.password;
    let password_for_verification = password.clone();
    let verified = run_with_password_slot(password_slot, move || {
        let Some(record) = auth.load()? else {
            return Ok(None);
        };
        Ok(record
            .verify_credentials(&username, &password_for_verification)?
            .then_some(record))
    })
    .await?;

    let Some(verified) = verified else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "用户名或密码不正确",
        ));
    };

    // Older deployments predate password synchronization during setup. A
    // successful administrator login is the only safe point at which the
    // plaintext password is available again. The verified record binds the
    // migration to the exact password hash that just passed verification.
    let mut terminal_ready = verified.system_password_synced;
    if !terminal_ready {
        // The migration supervisor owns the lock guard, the current-state
        // recheck, the bounded helper and the conditional marker write. If this
        // HTTP task is cancelled, dropping the JoinHandle does not cancel that
        // task or release the guard while the helper is still running.
        terminal_ready = migrate_system_password_on_login(
            state.password_migration.clone(),
            state.auth.clone(),
            verified,
            password,
            SystemPasswordHelper::default(),
        )
        .await;
    }
    let terminal_message = (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE);

    let session = state.sessions.create().map_err(ApiError::internal)?;
    let session_secure = state.config.read().await.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "terminal_ready": terminal_ready,
            "terminal_message": terminal_message,
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

fn too_many_login_attempts() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "登录尝试过多，请稍后再试",
    )
}

/// Run password verification on the blocking pool while keeping the global
/// verification permit inside the blocking closure. Aborting or dropping the
/// HTTP task therefore cannot release the slot while Argon2 is still running.
async fn run_with_password_slot<T, F>(
    password_slot: OwnedSemaphorePermit,
    verify: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AuthError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let _password_slot = password_slot;
        verify()
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(map_auth_error)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        state.sessions.revoke(token);
    }
    let session_secure = state.config.read().await.server.session_cookie_secure();
    with_session_cookie(
        Json(json!({"ok": true})).into_response(),
        None,
        session_secure,
    )
}

fn build_webauthn(headers: &HeaderMap) -> Result<(Webauthn, String, Url), ApiError> {
    if !origin_matches_host(headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }
    let origin_url = if let Some(origin) = headers.get(ORIGIN) {
        let origin_str = origin.to_str().map_err(|_| {
            ApiError::new(StatusCode::BAD_REQUEST, "无效的 Origin 请求标头")
        })?;
        Url::parse(origin_str).map_err(|e| {
            ApiError::new(StatusCode::BAD_REQUEST, format!("无效的 Origin URL: {e}"))
        })?
    } else if let Some(host) = headers.get(HOST) {
        let host_str = host.to_str().map_err(|_| {
            ApiError::new(StatusCode::BAD_REQUEST, "无效的 Host 请求标头")
        })?;
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|p| p.to_str().ok())
            .unwrap_or("http");
        Url::parse(&format!("{proto}://{host_str}")).map_err(|e| {
            ApiError::new(StatusCode::BAD_REQUEST, format!("无效的主机 URL: {e}"))
        })?
    } else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "缺少 Origin 或 Host 标头"));
    };

    let Some(rp_id) = origin_url.host_str() else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无法解析 Relying Party ID"));
    };

    if rp_id.parse::<IpAddr>().is_ok() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Passkey (WebAuthn) 规范要求使用域名（如 localhost 或 wingman.local），不支持直接使用 IP 地址。请使用域名访问。",
        ));
    }

    let builder = WebauthnBuilder::new(rp_id, &origin_url).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    Ok((webauthn, rp_id.to_owned(), origin_url))
}

async fn passkey_login_start(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Starting authentication is not an authentication submission: check the
    // current budget without reserving an attempt. Finish reserves exactly one.
    if !state.login_limiter.has_capacity(client_ip) {
        return Err(too_many_login_attempts());
    }
    // Each start allocates a server-side challenge, so it has its own
    // recorded budget even though it is not an authentication submission.
    if !state.passkey_start_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }
    let Some(record) = state.auth.load().map_err(map_auth_error)? else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"));
    };
    if record.passkeys.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "尚未注册任何 Passkey"));
    }

    let (webauthn, rp_id, rp_origin) = build_webauthn(&headers)?;
    let passkeys: Vec<Passkey> = record.passkeys.iter().map(|c| c.passkey.clone()).collect();
    let (rcr, auth_state) = webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| ApiError::internal(format!("启动认证失败: {e}")))?;

    let challenge_id = generate_token()
        .map_err(|e| ApiError::internal(format!("生成 challenge 失败: {e}")))?;

    let now = Instant::now();
    let expires_at = now + PASSKEY_CHALLENGE_TTL;
    insert_passkey_challenge(
        &state.passkey_challenges,
        challenge_id.clone(),
        PasskeyChallengeState::Authentication {
            state: auth_state,
            rp_id,
            rp_origin,
            expires_at,
        },
        now,
    )?;

    Ok(no_store(Json(json!({
        "challenge_id": challenge_id,
        "options": rcr,
    }))).into_response())
}

#[derive(Deserialize)]
struct PasskeyLoginFinishRequest {
    challenge_id: String,
    credential: PublicKeyCredential,
}

async fn passkey_login_finish(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    Json(request): Json<PasskeyLoginFinishRequest>,
) -> Result<Response, ApiError> {
    if !origin_matches_host(&headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Completing authentication is the submitted attempt: reserve once before
    // validating the challenge. Success and every failure keep the reservation.
    if !state.login_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }

    let challenge_state = {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.remove(&request.challenge_id)
    };

    let Some(PasskeyChallengeState::Authentication {
        state: auth_state,
        rp_id,
        rp_origin,
        expires_at,
    }) = challenge_state else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无效或已过期的认证挑战"));
    };

    if Instant::now() > expires_at {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "认证挑战已过期"));
    }

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    let auth_result = match webauthn.finish_passkey_authentication(&request.credential, &auth_state) {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!(error = %e, "WebAuthn authentication failed");
            return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Passkey 验证失败"));
        }
    };

    let cred_id = request.credential.raw_id;
    // The challenge snapshots the credential set at start time; re-check that
    // the credential still exists so deleting a Passkey revokes it immediately.
    let login = state.auth.update(|record| {
        Ok(record
            .update_passkey_credential(cred_id.as_slice(), &auth_result)
            .then(|| (record.username.clone(), record.system_password_synced)))
    }).map_err(map_auth_error)?;
    let Some((username, terminal_ready)) = login else {
        tracing::warn!("Passkey login rejected: credential was removed after the challenge started");
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Passkey 验证失败"));
    };

    let session = state.sessions.create().map_err(ApiError::internal)?;
    let session_secure = state.config.read().await.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "username": username,
            "terminal_ready": terminal_ready,
            "terminal_message": (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE),
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

#[derive(Deserialize)]
struct PasskeyRegisterStartRequest {
    #[serde(default)]
    name: Option<String>,
}

async fn passkey_register_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PasskeyRegisterStartRequest>,
) -> Result<Response, ApiError> {
    let Some(record) = state.auth.load().map_err(map_auth_error)? else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"));
    };

    let (webauthn, rp_id, rp_origin) = build_webauthn(&headers)?;

    let exclude_credentials = if record.passkeys.is_empty() {
        None
    } else {
        Some(record.passkeys.iter().map(|p| p.passkey.cred_id().clone()).collect())
    };

    let user_unique_id = record.user_unique_id();
    let (ccr, reg_state) = webauthn
        .start_passkey_registration(
            user_unique_id,
            &record.username,
            &record.username,
            exclude_credentials,
        )
        .map_err(|e| ApiError::internal(format!("启动注册失败: {e}")))?;

    let challenge_id = generate_token()
        .map_err(|e| ApiError::internal(format!("生成 challenge 失败: {e}")))?;

    let now = Instant::now();
    let expires_at = now + PASSKEY_CHALLENGE_TTL;
    insert_passkey_challenge(
        &state.passkey_challenges,
        challenge_id.clone(),
        PasskeyChallengeState::Registration {
            state: reg_state,
            rp_id,
            rp_origin,
            expires_at,
        },
        now,
    )?;

    Ok(no_store(Json(json!({
        "challenge_id": challenge_id,
        "options": ccr,
        "suggested_name": request.name,
    }))).into_response())
}

#[derive(Deserialize)]
struct PasskeyRegisterFinishRequest {
    challenge_id: String,
    #[serde(default)]
    name: Option<String>,
    credential: RegisterPublicKeyCredential,
}

async fn passkey_register_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PasskeyRegisterFinishRequest>,
) -> Result<Response, ApiError> {
    if !origin_matches_host(&headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }

    let challenge_state = {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.remove(&request.challenge_id)
    };

    let Some(PasskeyChallengeState::Registration {
        state: reg_state,
        rp_id,
        rp_origin,
        expires_at,
    }) = challenge_state else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无效或已过期的注册挑战"));
    };

    if Instant::now() > expires_at {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "注册挑战已过期"));
    }

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    let passkey = webauthn
        .finish_passkey_registration(&request.credential, &reg_state)
        .map_err(|e| {
            tracing::warn!(error = %e, "WebAuthn finish registration failed");
            ApiError::new(StatusCode::BAD_REQUEST, format!("Passkey 注册验证失败: {e}"))
        })?;

    let passkey_name = request.name.unwrap_or_default();
    let (id, name) = state.auth.update(|record| {
        let cred = record.add_passkey(passkey_name, passkey)?;
        Ok((cred.id.clone(), cred.name.clone()))
    }).map_err(map_auth_error)?;

    Ok(Json(json!({
        "ok": true,
        "id": id,
        "name": name,
    })).into_response())
}

#[derive(Serialize)]
struct PasskeyInfo {
    id: String,
    name: String,
    created_at_unix_seconds: u64,
}

async fn passkey_list(State(state): State<AppState>) -> Result<Response, ApiError> {
    let record = state.auth.load().map_err(map_auth_error)?.ok_or_else(|| {
        ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化")
    })?;

    let list: Vec<PasskeyInfo> = record.passkeys.iter().map(|c| PasskeyInfo {
        id: c.id.clone(),
        name: c.name.clone(),
        created_at_unix_seconds: c.created_at_unix_seconds,
    }).collect();

    Ok(no_store(Json(json!({
        "ok": true,
        "passkeys": list,
    }))).into_response())
}

#[derive(Deserialize)]
struct PasskeyDeleteRequest {
    id: String,
}

async fn passkey_delete(
    State(state): State<AppState>,
    Json(request): Json<PasskeyDeleteRequest>,
) -> Result<Response, ApiError> {
    let removed = state.auth.update(|record| {
        record.remove_passkey(&request.id)
    }).map_err(map_auth_error)?;

    if !removed {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "找不到该 Passkey"));
    }

    Ok(Json(json!({ "ok": true })).into_response())
}

async fn passkey_delete_path(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    passkey_delete(State(state), Json(PasskeyDeleteRequest { id })).await
}

async fn system_status(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.system.snapshot()))
}

async fn status(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let client = state.client_address(connection, &headers).await;
    let config = state.config.read().await.clone();
    let webrtc = state.webrtc.status();
    let system = state.system.snapshot();
    no_store(Json(json!({
        "video": state.video.status(),
        "power": state.power.status().await,
        "webrtc": webrtc,
        "capabilities": capabilities_with_webrtc(&config, webrtc.available),
        "hid": {
            "keyboard_leds": state.hid.leds(),
        },
        "system": system,
        "client": {
            "ip": client.ip.to_string(),
            "port": client.port,
            "source": client.source,
            "proxy_protocol": config.server.proxy_protocol,
        },
    })))
}

async fn scan_devices() -> impl IntoResponse {
    no_store(Json(discovery::scan().await))
}

/// Results are reused briefly so unauthenticated callers during first setup
/// cannot queue unbounded device probes.
const SETUP_SCAN_CACHE_TTL: Duration = Duration::from_secs(3);

async fn scan_setup_devices(State(state): State<AppState>) -> Result<Response, ApiError> {
    if state.auth.is_initialized().map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    }
    Ok(no_store(Json(cached_setup_scan(&state.setup_scan, discovery::scan).await)))
}

/// Serializes setup scans and shares one result across callers within the TTL.
async fn cached_setup_scan<F, Fut>(
    cache: &AsyncMutex<Option<(Instant, discovery::DeviceDiscovery)>>,
    scan: F,
) -> discovery::DeviceDiscovery
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = discovery::DeviceDiscovery>,
{
    let mut cached = cache.lock().await;
    if let Some((scanned_at, result)) = cached.as_ref()
        && scanned_at.elapsed() < SETUP_SCAN_CACHE_TTL
    {
        return result.clone();
    }
    let result = scan().await;
    *cached = Some((Instant::now(), result.clone()));
    result
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.config.read().await.clone()))
}

struct FullConfigUpdate(Config);

impl<'de> Deserialize<'de> for FullConfigUpdate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        reject_retired_hid_timing_fields(&value).map_err(serde::de::Error::custom)?;
        serde_json::from_value(value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

async fn update_config(
    State(state): State<AppState>,
    Json(FullConfigUpdate(config)): Json<FullConfigUpdate>,
) -> Result<impl IntoResponse, ApiError> {
    let _update_guard = state.config_update.lock().await;
    validate_config(&config)?;
    let previous = state.config.read().await.server.clone();
    let listener_changed = previous.listen_address != config.server.listen_address
        || previous.port != config.server.port;
    commit_config(&state, &config).await?;
    let mut response = no_store(Json(config)).into_response();
    if listener_changed {
        response.headers_mut().insert(
            "x-wingmankvm-restart-required",
            HeaderValue::from_static("true"),
        );
    }
    Ok(response)
}

#[derive(Default, Deserialize)]
struct ConfigPatch {
    server: Option<ServerPatch>,
    video: Option<VideoPatch>,
    hid: Option<HidPatch>,
    power: Option<PowerPatch>,
    media: Option<MediaPatch>,
}

#[derive(Deserialize)]
struct ServerPatch {
    proxy_protocol: Option<ProxyProtocolMode>,
    trusted_http_proxies: Option<Vec<IpAddr>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    public_origin: Option<Option<String>>,
}

#[derive(Deserialize)]
struct VideoPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    width: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    height: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    frames_per_second: Option<Option<u32>>,
    encoding: Option<VideoEncoding>,
    jpeg_quality: Option<u8>,
    h264: Option<H264Patch>,
}

#[derive(Deserialize)]
struct H264Patch {
    bitrate_kbps: Option<u32>,
    encoder: Option<H264Encoder>,
    allow_software: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    ffmpeg_path: Option<Option<PathBuf>>,
    max_sessions: Option<u8>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    udp_bind_address: Option<Option<SocketAddr>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    public_udp_endpoint: Option<Option<String>>,
}

#[derive(Deserialize)]
struct HidPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    keyboard_device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    mouse_device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    absolute_pointer_device: Option<Option<PathBuf>>,
    pointer_mode: Option<PointerMode>,
    #[serde(
        default,
        rename = "write_timeout_ms",
        deserialize_with = "reject_retired_write_timeout_ms"
    )]
    #[allow(dead_code)]
    _write_timeout_ms: Option<serde_json::Value>,
    #[serde(
        default,
        rename = "retry_interval_ms",
        deserialize_with = "reject_retired_retry_interval_ms"
    )]
    #[allow(dead_code)]
    _retry_interval_ms: Option<serde_json::Value>,
}

fn retired_hid_timing_error(field: &'static str) -> String {
    format!(
        "{field} is deprecated and ignored: HID reports use a fixed 500 ms per-report deadline with readiness-based retry instead of polling; remove this field"
    )
}

fn reject_retired_hid_timing<'de, D>(
    deserializer: D,
    field: &'static str,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    // Consume any JSON value, including null, so the error is explicit
    // instead of an opaque type mismatch.
    let _ = serde_json::Value::deserialize(deserializer)?;
    Err(serde::de::Error::custom(retired_hid_timing_error(field)))
}

fn reject_retired_write_timeout_ms<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    reject_retired_hid_timing(deserializer, "hid.write_timeout_ms")
}

fn reject_retired_retry_interval_ms<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    reject_retired_hid_timing(deserializer, "hid.retry_interval_ms")
}

fn reject_retired_hid_timing_fields(value: &serde_json::Value) -> Result<(), String> {
    let Some(hid) = value.get("hid").and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    for (key, field) in [
        ("write_timeout_ms", "hid.write_timeout_ms"),
        ("retry_interval_ms", "hid.retry_interval_ms"),
    ] {
        if hid.contains_key(key) {
            return Err(retired_hid_timing_error(field));
        }
    }
    Ok(())
}

fn deserialize_optional_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct PowerPatch {
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_line: Option<Option<u32>>,
    active_high: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    reset_switch: Option<Option<GpioPulsePatch>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    power_led: Option<Option<GpioInputPatch>>,
    short_press_ms: Option<u64>,
    long_press_ms: Option<u64>,
}

#[derive(Deserialize)]
struct GpioPulsePatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_line: Option<Option<u32>>,
    active_high: Option<bool>,
    pulse_ms: Option<u64>,
}

#[derive(Deserialize)]
struct GpioInputPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    gpio_line: Option<Option<u32>>,
    active_low: Option<bool>,
    bias: Option<GpioBias>,
    poll_interval_ms: Option<u64>,
    debounce_ms: Option<u64>,
}

#[derive(Deserialize)]
struct MediaPatch {
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    lun_path: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    image_directory: Option<Option<PathBuf>>,
    read_only_by_default: Option<bool>,
}

async fn patch_config(
    State(state): State<AppState>,
    Json(patch): Json<ConfigPatch>,
) -> Result<impl IntoResponse, ApiError> {
    let _update_guard = state.config_update.lock().await;
    let mut config = state.config.read().await.clone();
    if let Some(server) = patch.server {
        apply_server_patch(&mut config.server, server);
    }
    if let Some(video) = patch.video {
        apply_video_patch(&mut config.video, video);
    }
    if let Some(hid) = patch.hid {
        apply_hid_patch(&mut config.hid, hid);
    }
    if let Some(power) = patch.power {
        apply_power_patch(&mut config.power, power);
    }
    if let Some(media) = patch.media {
        apply_media_patch(&mut config.media, media);
    }
    validate_config(&config)?;
    commit_config(&state, &config).await?;
    Ok(no_store(Json(config)))
}

fn apply_server_patch(config: &mut config::ServerConfig, patch: ServerPatch) {
    if let Some(proxy_protocol) = patch.proxy_protocol {
        config.proxy_protocol = proxy_protocol;
    }
    if let Some(proxies) = patch.trusted_http_proxies {
        config.trusted_http_proxies = proxies;
    }
    if let Some(public_origin) = patch.public_origin {
        config.public_origin = public_origin.map(|value| value.trim().to_owned());
    }
}

fn apply_video_patch(config: &mut config::VideoConfig, patch: VideoPatch) {
    if let Some(device) = patch.device {
        config.device = device;
    }
    if let Some(width) = patch.width {
        config.width = width;
    }
    if let Some(height) = patch.height {
        config.height = height;
    }
    if let Some(frames_per_second) = patch.frames_per_second {
        config.frames_per_second = frames_per_second;
    }
    if let Some(encoding) = patch.encoding {
        config.encoding = encoding;
    }
    if let Some(quality) = patch.jpeg_quality {
        config.jpeg_quality = quality;
    }
    if let Some(h264) = patch.h264 {
        if let Some(bitrate_kbps) = h264.bitrate_kbps {
            config.h264.bitrate_kbps = bitrate_kbps;
        }
        if let Some(encoder) = h264.encoder {
            config.h264.encoder = encoder;
        }
        if let Some(allow_software) = h264.allow_software {
            config.h264.allow_software = allow_software;
        }
        if let Some(ffmpeg_path) = h264.ffmpeg_path {
            config.h264.ffmpeg_path = ffmpeg_path;
        }
        if let Some(max_sessions) = h264.max_sessions {
            config.h264.max_sessions = max_sessions;
        }
        if let Some(address) = h264.udp_bind_address {
            config.h264.udp_bind_address = address;
        }
        if let Some(endpoint) = h264.public_udp_endpoint {
            config.h264.public_udp_endpoint = endpoint;
        }
    }
    config.auto_detect = config.device.is_none();
}

fn apply_hid_patch(config: &mut HidConfig, patch: HidPatch) {
    if let Some(path) = patch.keyboard_device {
        config.keyboard_device = path;
    }
    if let Some(path) = patch.mouse_device {
        config.mouse_device = path;
    }
    if let Some(path) = patch.absolute_pointer_device {
        config.absolute_pointer_device = path;
    }
    if let Some(mode) = patch.pointer_mode {
        config.pointer_mode = mode;
    }
    config.auto_detect = hid_needs_auto_detection(config);
}

fn hid_needs_auto_detection(config: &HidConfig) -> bool {
    config.keyboard_device.is_none()
        || match config.pointer_mode {
            PointerMode::Absolute => config.absolute_pointer_device.is_none(),
            PointerMode::Relative => config.mouse_device.is_none(),
            PointerMode::Auto => {
                config.mouse_device.is_none() && config.absolute_pointer_device.is_none()
            }
        }
}

fn apply_power_patch(config: &mut config::PowerConfig, patch: PowerPatch) {
    if let Some(enabled) = patch.enabled {
        config.enabled = enabled;
    }
    if let Some(gpio_chip) = patch.gpio_chip {
        config.gpio_chip = gpio_chip;
    }
    if let Some(gpio_line) = patch.gpio_line {
        config.gpio_line = gpio_line;
    }
    if let Some(active_high) = patch.active_high {
        config.active_high = active_high;
    }
    if let Some(short_press_ms) = patch.short_press_ms {
        config.short_press_ms = short_press_ms;
    }
    if let Some(long_press_ms) = patch.long_press_ms {
        config.long_press_ms = long_press_ms;
    }
    if let Some(reset_switch) = patch.reset_switch {
        if let Some(patch) = reset_switch {
            let mut value = config.reset_switch.clone().unwrap_or_default();
            if let Some(gpio_chip) = patch.gpio_chip {
                value.gpio_chip = optional_string(gpio_chip);
            }
            if let Some(gpio_line) = patch.gpio_line {
                value.gpio_line = gpio_line;
            }
            if let Some(active_high) = patch.active_high {
                value.active_high = active_high;
            }
            if let Some(pulse_ms) = patch.pulse_ms {
                value.pulse_ms = pulse_ms;
            }
            config.reset_switch = Some(value);
        } else {
            config.reset_switch = None;
        }
    }
    if let Some(power_led) = patch.power_led {
        if let Some(patch) = power_led {
            let mut value = config.power_led.clone().unwrap_or_default();
            if let Some(gpio_chip) = patch.gpio_chip {
                value.gpio_chip = optional_string(gpio_chip);
            }
            if let Some(gpio_line) = patch.gpio_line {
                value.gpio_line = gpio_line;
            }
            if let Some(active_low) = patch.active_low {
                value.active_low = active_low;
            }
            if let Some(bias) = patch.bias {
                value.bias = bias;
            }
            if let Some(poll_interval_ms) = patch.poll_interval_ms {
                value.poll_interval_ms = poll_interval_ms;
            }
            if let Some(debounce_ms) = patch.debounce_ms {
                value.debounce_ms = debounce_ms;
            }
            config.power_led = Some(value);
        } else {
            config.power_led = None;
        }
    }
}

fn apply_media_patch(config: &mut config::MediaConfig, patch: MediaPatch) {
    if let Some(enabled) = patch.enabled {
        config.enabled = enabled;
    }
    if let Some(path) = patch.lun_path {
        config.lun_path = path;
    }
    if let Some(path) = patch.image_directory {
        config.image_directory = path;
    }
    if let Some(read_only) = patch.read_only_by_default {
        config.read_only_by_default = read_only;
    }
}

async fn key(
    State(state): State<AppState>,
    Json(request): Json<KeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let path = state.config.read().await.hid.keyboard_device.clone();
    state.hid.key(path, request).await.map_err(map_hid_error)?;
    Ok(no_store(Json(json!({
        "keyboard_leds": state.hid.leds(),
    }))))
}

async fn get_keyboard_leds(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.hid.leds()))
}

async fn mouse_move(
    State(state): State<AppState>,
    Json(request): Json<MouseMoveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state
        .hid
        .mouse_move(path, request)
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mouse_absolute(
    State(state): State<AppState>,
    Json(request): Json<AbsolutePointerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.hid.clone();
    if resolved_pointer_mode(&config) != PointerMode::Absolute {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "当前配置未启用绝对指针模式",
        ));
    }
    state
        .hid
        .mouse_absolute(config.absolute_pointer_device, request)
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mouse_click(
    State(state): State<AppState>,
    Json(request): Json<MouseClickRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state
        .hid
        .mouse_click(path, request)
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mouse_scroll(
    State(state): State<AppState>,
    Json(request): Json<MouseScrollRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state
        .hid
        .mouse_scroll(path, request)
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn release_all(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let config = state.config.read().await.hid.clone();
    state
        .hid
        .release_all(
            config.keyboard_device,
            config.mouse_device,
            config.absolute_pointer_device,
        )
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PowerForm {
    duration: f64,
}

async fn power(
    State(state): State<AppState>,
    Form(form): Form<PowerForm>,
) -> Result<Response, ApiError> {
    let config = state.config.read().await.power.clone();
    let press = if (form.duration - 0.5).abs() < 0.01 {
        PowerPress::Short
    } else if (form.duration - 5.0).abs() < 0.01 {
        PowerPress::Long
    } else {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "只允许短按或长按电源键",
        ));
    };
    let (chip, line) = config
        .enabled
        .then(|| configured_gpio(config.gpio_chip.as_ref(), config.gpio_line))
        .flatten()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "GPIO 电源控制尚未配置"))?;
    let pid = state
        .power
        .press(
            PowerConfigSnapshot {
                program: PathBuf::from("gpioset"),
                chip,
                line,
                active_high: config.active_high,
                short_press_ms: config.short_press_ms,
                long_press_ms: config.long_press_ms,
                cooldown_ms: 1_000,
            },
            press,
        )
        .await
        .map_err(map_power_error)?;
    let location = format!("/?power_duration={:.2}&power_pid={pid}", form.duration);
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        LOCATION,
        HeaderValue::from_str(&location).map_err(ApiError::internal)?,
    );
    Ok(response)
}

#[derive(Deserialize, Default)]
struct ResetForm {
    duration: Option<f64>,
}

async fn reset(
    State(state): State<AppState>,
    Form(form): Form<ResetForm>,
) -> Result<Response, ApiError> {
    if form
        .duration
        .is_some_and(|duration| (duration - 0.5).abs() >= 0.01)
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "只允许短按重启键",
        ));
    }
    let config = state.config.read().await.power.clone();
    let snapshot = reset_pulse_snapshot(&config, None)
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "GPIO 重启控制尚未配置"))?;
    let duration = snapshot.pulse_ms as f64 / 1_000.0;
    let pid = state.power.pulse(snapshot).await.map_err(map_power_error)?;
    let location = format!("/?reset_duration={duration:.2}&reset_pid={pid}");
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        LOCATION,
        HeaderValue::from_str(&location).map_err(ApiError::internal)?,
    );
    Ok(response)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GpioTestTarget {
    Power,
    Reset,
}

#[derive(Debug, Deserialize)]
struct GpioTestRequest {
    target: GpioTestTarget,
    duration_ms: Option<u64>,
}

async fn gpio_test(
    State(state): State<AppState>,
    Json(request): Json<GpioTestRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let duration_ms = request.duration_ms.unwrap_or(GPIO_TEST_PULSE_MS);
    if !(50..=2_000).contains(&duration_ms) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "测试脉冲必须在 50 到 2000 毫秒之间",
        ));
    }
    let config = state.config.read().await.power.clone();
    let snapshot = match request.target {
        GpioTestTarget::Power => {
            power_pulse_snapshot(&config, Some(duration_ms)).ok_or_else(|| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "GPIO 电源控制尚未配置")
            })?
        }
        GpioTestTarget::Reset => {
            reset_pulse_snapshot(&config, Some(duration_ms)).ok_or_else(|| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "GPIO 重启控制尚未配置")
            })?
        }
    };
    let pid = state.power.pulse(snapshot).await.map_err(map_power_error)?;
    Ok(no_store(Json(json!({ "ok": true, "pid": pid }))))
}

async fn list_media(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let snapshot = media_snapshot(&state).await;
    let (images, status) = state.media.list(snapshot).await.map_err(map_media_error)?;
    Ok(no_store(Json(json!({"images": images, "status": status}))))
}

async fn upload_media(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    upload_media_with_limits(state.media_upload.clone(), state.config.clone(), multipart, UploadLimits::default()).await
}

const UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const UPLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const UPLOAD_BUSY_MESSAGE: &str = "另一个镜像上传正在进行，请稍后重试";

#[derive(Clone, Copy)]
struct UploadLimits { idle: Duration, total: Duration }
impl Default for UploadLimits {
    fn default() -> Self { Self { idle: UPLOAD_IDLE_TIMEOUT, total: UPLOAD_TOTAL_TIMEOUT } }
}

#[derive(Clone, Copy)]
enum UploadDeadline { Idle, Total }

fn upload_timeout_error(deadline: UploadDeadline) -> ApiError {
    let message = match deadline {
        UploadDeadline::Idle => "上传停滞超过 15 秒，已中止；请检查网络后重试",
        UploadDeadline::Total => "上传超过 2 小时总时限，已中止；请重新上传",
    };
    ApiError::new(StatusCode::REQUEST_TIMEOUT, message)
}

async fn with_total_deadline<T>(
    future: impl std::future::Future<Output = T>, total_deadline: Instant,
) -> Result<T, UploadDeadline> {
    let remaining = total_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() { return Err(UploadDeadline::Total); }
    tokio::time::timeout(remaining, future).await.map_err(|_| UploadDeadline::Total)
}

async fn with_upload_deadlines<T>(
    future: impl std::future::Future<Output = T>, idle_deadline: Instant, total_deadline: Instant,
) -> Result<T, UploadDeadline> {
    if total_deadline <= idle_deadline { return with_total_deadline(future, total_deadline).await; }
    let remaining = idle_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() { return Err(UploadDeadline::Idle); }
    tokio::time::timeout(remaining, future).await.map_err(|_| UploadDeadline::Idle)
}

async fn next_upload_chunk(
    field: &mut axum::extract::multipart::Field<'_>, last_progress: &mut Instant,
    limits: UploadLimits, total_deadline: Instant,
) -> Result<Option<Bytes>, ApiError> {
    let idle_deadline = *last_progress + limits.idle;
    let chunk = with_upload_deadlines(field.chunk(), idle_deadline, total_deadline)
        .await.map_err(upload_timeout_error)?.map_err(ApiError::internal)?;
    if let Some(chunk) = &chunk {
        if !chunk.is_empty() { *last_progress = Instant::now(); }
    }
    Ok(chunk)
}

async fn write_upload_chunk<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W, chunk: &[u8], uploaded: &mut u64,
    max_upload_bytes: u64, total_deadline: Instant,
) -> Result<(), ApiError> {
    let next_size = uploaded.saturating_add(chunk.len() as u64);
    if next_size > max_upload_bytes {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "镜像超过配置的大小上限"));
    }
    match with_total_deadline(writer.write_all(chunk), total_deadline).await {
        Ok(Ok(())) => { *uploaded = next_size; Ok(()) }
        Ok(Err(error)) => Err(ApiError::internal(error)),
        Err(_) => Err(upload_timeout_error(UploadDeadline::Total)),
    }
}

struct PartialUpload { path: Option<PathBuf>, file: Option<fs::File> }
impl PartialUpload {
    fn create(image_dir: &Path) -> Result<Self, ApiError> {
        let token = generate_token().map_err(ApiError::internal)?;
        let path = image_dir.join(format!(".wingmankvm-upload-{token}.partial"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt as _; options.mode(0o600); }
        let file = options.open(&path).map_err(ApiError::internal)?;
        Ok(Self { path: Some(path), file: Some(fs::File::from_std(file)) })
    }
    fn file_mut(&mut self) -> &mut fs::File {
        self.file.as_mut().expect("partial upload file is open")
    }
    fn publish(mut self, final_path: &Path) -> std::io::Result<()> {
        self.file.take();
        let partial_path = self.path.take().expect("partial upload path is set");
        match std::fs::hard_link(&partial_path, final_path) {
            Ok(()) => {
                if let Err(error) = std::fs::remove_file(&partial_path) {
                    tracing::warn!(error = %error, path = %partial_path.display(),
                        "published upload but could not remove its temporary file");
                    self.path = Some(partial_path);
                }
                Ok(())
            }
            Err(error) => { self.path = Some(partial_path); Err(error) }
        }
    }
}
impl Drop for PartialUpload {
    fn drop(&mut self) {
        self.file.take();
        if let Some(path) = self.path.take() {
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(error = %error, path = %path.display(),
                        "could not remove aborted upload temporary file");
                }
            }
        }
    }
}

async fn upload_media_with_limits(
    upload_slot: Arc<AsyncMutex<()>>, config_store: Arc<RwLock<Config>>,
    mut multipart: Multipart, limits: UploadLimits,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Ok(_upload_guard) = upload_slot.try_lock_owned() else {
        return Err(ApiError::new(StatusCode::CONFLICT, UPLOAD_BUSY_MESSAGE));
    };
    let started = Instant::now();
    let total_deadline = started + limits.total;
    let mut last_progress = started;
    let config = config_store.read().await.media.clone();
    if !config.enabled {
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "虚拟介质尚未配置"));
    }
    let image_dir = config.image_directory
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "镜像目录尚未配置"))?;
    match with_total_deadline(fs::create_dir_all(&image_dir), total_deadline).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(ApiError::internal(error)),
        Err(deadline) => return Err(upload_timeout_error(deadline)),
    }
    let mut uploaded = None;
    loop {
        let next = with_upload_deadlines(
            multipart.next_field(), last_progress + limits.idle, total_deadline,
        ).await.map_err(upload_timeout_error)?;
        let Some(mut field) = next.map_err(ApiError::internal)? else { break };
        last_progress = Instant::now();
        if !matches!(field.name(), Some("image" | "file")) {
            while next_upload_chunk(&mut field, &mut last_progress, limits, total_deadline).await?.is_some() {}
            continue;
        }
        let original = field.file_name()
            .ok_or_else(|| ApiError::bad_request("上传内容缺少文件名"))?.to_string();
        let name = sanitize_upload_name(&original).map_err(map_media_error)?;
        let final_path = image_dir.join(&name);
        let exists = with_total_deadline(fs::try_exists(&final_path), total_deadline)
            .await.map_err(upload_timeout_error)?.map_err(ApiError::internal)?;
        if exists { return Err(ApiError::new(StatusCode::CONFLICT, "同名镜像已经存在")); }
        let mut partial = PartialUpload::create(&image_dir)?;
        let mut size = 0_u64;
        while let Some(chunk) = next_upload_chunk(&mut field, &mut last_progress, limits, total_deadline).await? {
            if chunk.is_empty() { continue; }
            write_upload_chunk(partial.file_mut(), chunk.as_ref(), &mut size, config.max_upload_bytes, total_deadline).await?;
        }
        match with_total_deadline(partial.file_mut().sync_all(), total_deadline).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(ApiError::internal(error)),
            Err(deadline) => return Err(upload_timeout_error(deadline)),
        }
        match partial.publish(&final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists =>
                return Err(ApiError::new(StatusCode::CONFLICT, "同名镜像已经存在")),
            Err(error) => return Err(ApiError::internal(error)),
        }
        uploaded = Some(json!({"name": name, "size": size}));
        break;
    }
    uploaded.map(Json).ok_or_else(|| ApiError::bad_request("表单中没有 image 文件字段"))
}

#[derive(Deserialize)]
struct AttachMediaRequest {
    name: String,
    #[serde(default)]
    media_type: MediaType,
    read_only: Option<bool>,
}

async fn attach_media(
    State(state): State<AppState>,
    Json(request): Json<AttachMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let default_read_only = state.config.read().await.media.read_only_by_default;
    let status = state
        .media
        .attach(
            media_snapshot(&state).await,
            &request.name,
            request.media_type,
            request.read_only.unwrap_or(default_read_only),
        )
        .await
        .map_err(map_media_error)?;
    Ok(no_store(Json(status)))
}

#[derive(Deserialize)]
struct DetachMediaRequest {
    #[serde(default)]
    force: bool,
}

async fn detach_media(
    State(state): State<AppState>,
    Json(request): Json<DetachMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let status = state
        .media
        .detach(media_snapshot(&state).await, request.force)
        .await
        .map_err(map_media_error)?;
    Ok(no_store(Json(status)))
}

async fn video_feed(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // Bind the stream to the live session so logout, revocation, or expiry
    // ends it, matching the terminal and WebRTC lifecycles.
    let Some(guard) = session_token(&headers).and_then(|token| state.sessions.validate_guard(token))
    else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    let frames = state.video.subscribe();
    let mut response = mjpeg_body(frames, guard).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("multipart/x-mixed-replace; boundary=frame"),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    response
}

fn mjpeg_body(
    mut frames: watch::Receiver<Option<Arc<crate::devices::video::VideoFrame>>>,
    guard: SessionGuard,
) -> Body {
    Body::from_stream(async_stream::stream! {
        loop {
            let frame = frames.borrow_and_update().clone();
            if let Some(frame) = frame {
                let header = Bytes::from(format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nX-Wingman-Sequence: {}\r\n\r\n",
                    frame.jpeg.len(), frame.sequence
                ));
                yield Ok::<Bytes, Infallible>(header);
                yield Ok::<Bytes, Infallible>(frame.jpeg.clone());
                yield Ok::<Bytes, Infallible>(Bytes::from_static(b"\r\n"));
            }
            tokio::select! {
                biased;
                _ = guard.wait_for_end() => break,
                changed = frames.changed() => if changed.is_err() { break },
            }
        }
    })
}

async fn webrtc_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WebRtcOfferRequest>,
) -> Result<Response, ApiError> {
    // Acquire the live guard atomically with validation. A logout that races
    // this request cannot leave an authorized peer behind, because the guard
    // is passed into the manager and supervises revocation/expiry.
    let Some(guard) = session_token(&headers)
        .and_then(|token| state.sessions.validate_guard(token))
    else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    };
    let response = state
        .webrtc
        .offer(request, guard)
        .await
        .map_err(map_webrtc_error)?;
    Ok(no_store(Json(response)).into_response())
}

#[derive(Debug, Deserialize)]
struct WebRtcCloseRequest {
    session_id: String,
}

async fn webrtc_close(
    State(state): State<AppState>,
    Json(request): Json<WebRtcCloseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.session_id.len() > 128 || request.session_id.is_empty() {
        return Err(ApiError::bad_request("WebRTC 会话标识不正确"));
    }
    Ok(no_store(Json(json!({
        "closed": state.webrtc.close(&request.session_id).await,
    }))))
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_authenticated(&state, request.headers()) {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    }
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && !origin_matches_host(request.headers())
    {
        return ApiError::new(StatusCode::FORBIDDEN, "请求来源与当前主机不一致").into_response();
    }
    next.run(request).await
}

fn is_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    session_token(headers)
        .and_then(|token| state.sessions.validate(token))
        .is_some()
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix(&format!("{SESSION_COOKIE}=")))
}

fn origin_matches_host(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        return false;
    };
    let Some(origin_authority) = uri.authority() else {
        return false;
    };
    headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| origin_authority.as_str().eq_ignore_ascii_case(host))
}

#[allow(dead_code)]
fn capabilities(config: &Config) -> Capabilities {
    capabilities_with_webrtc(config, false)
}

fn capabilities_with_webrtc(config: &Config, video_webrtc_h264: bool) -> Capabilities {
    let pointer_mode = resolved_pointer_mode(&config.hid);
    let mouse_relative = config.hid.mouse_device.is_some();
    let mouse_absolute = config.hid.absolute_pointer_device.is_some();
    Capabilities {
        video: config.video.device.is_some(),
        keyboard: config.hid.keyboard_device.is_some(),
        mouse: match pointer_mode {
            PointerMode::Absolute => mouse_absolute,
            PointerMode::Auto | PointerMode::Relative => mouse_relative,
        },
        mouse_relative,
        mouse_absolute,
        pointer_mode,
        gpio_power: config.power.enabled
            && config.power.gpio_chip.is_some()
            && config.power.gpio_line.is_some(),
        gpio_reset: config
            .power
            .reset_switch
            .as_ref()
            .is_some_and(gpio_pulse_is_configured),
        gpio_power_led: config
            .power
            .power_led
            .as_ref()
            .is_some_and(gpio_input_is_configured),
        mass_storage: config.media.enabled
            && config.media.lun_path.is_some()
            && config.media.image_directory.is_some(),
        video_passthrough: true,
        video_transcode: true,
        video_webrtc_h264,
    }
}

fn resolved_pointer_mode(config: &HidConfig) -> PointerMode {
    match config.pointer_mode {
        PointerMode::Auto if config.absolute_pointer_device.is_some() => PointerMode::Absolute,
        PointerMode::Auto => PointerMode::Relative,
        mode => mode,
    }
}

fn relative_mouse_path(config: &HidConfig) -> Result<Option<PathBuf>, ApiError> {
    if resolved_pointer_mode(config) != PointerMode::Relative {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "当前配置未启用相对鼠标模式",
        ));
    }
    Ok(config.mouse_device.clone())
}

async fn media_snapshot(state: &AppState) -> Option<MediaConfigSnapshot> {
    let config = state.config.read().await.media.clone();
    if !config.enabled {
        return None;
    }
    Some(MediaConfigSnapshot {
        image_dir: config.image_directory?,
        lun_path: config.lun_path?,
    })
}

fn gpio_pulse_is_configured(config: &GpioPulseConfig) -> bool {
    configured_gpio(config.gpio_chip.as_ref(), config.gpio_line).is_some()
}

fn gpio_input_is_configured(config: &GpioInputConfig) -> bool {
    configured_gpio(config.gpio_chip.as_ref(), config.gpio_line).is_some()
}

fn configured_gpio(chip: Option<&String>, line: Option<u32>) -> Option<(String, u32)> {
    let chip = chip?.trim();
    let line = line?;
    (!chip.is_empty()).then(|| (chip.to_owned(), line))
}

fn power_pulse_snapshot(
    config: &config::PowerConfig,
    duration_override_ms: Option<u64>,
) -> Option<GpioPulseConfigSnapshot> {
    if !config.enabled {
        return None;
    }
    let (chip, line) = configured_gpio(config.gpio_chip.as_ref(), config.gpio_line)?;
    Some(GpioPulseConfigSnapshot {
        program: PathBuf::from("gpioset"),
        chip,
        line,
        active_high: config.active_high,
        pulse_ms: duration_override_ms.unwrap_or(config.short_press_ms),
        cooldown_ms: 1_000,
    })
}

fn reset_pulse_snapshot(
    config: &config::PowerConfig,
    duration_override_ms: Option<u64>,
) -> Option<GpioPulseConfigSnapshot> {
    let reset = config.reset_switch.as_ref()?;
    let (chip, line) = configured_gpio(reset.gpio_chip.as_ref(), reset.gpio_line)?;
    Some(GpioPulseConfigSnapshot {
        program: PathBuf::from("gpioset"),
        chip,
        line,
        active_high: reset.active_high,
        pulse_ms: duration_override_ms.unwrap_or(reset.pulse_ms),
        cooldown_ms: 250,
    })
}

fn power_led_snapshot(config: &config::PowerConfig) -> Option<PowerLedConfigSnapshot> {
    let led = config.power_led.as_ref()?;
    let (chip, line) = configured_gpio(led.gpio_chip.as_ref(), led.gpio_line)?;
    Some(PowerLedConfigSnapshot {
        program: PathBuf::from("gpioget"),
        chip,
        line,
        active_low: led.active_low,
        bias: led.bias,
        poll_interval_ms: led.poll_interval_ms,
        debounce_ms: led.debounce_ms,
    })
}

async fn persist_config(path: &Path, config: &Config) -> Result<(), ApiError> {
    let path = path.to_path_buf();
    let config = config.clone();
    tokio::task::spawn_blocking(move || config.save_atomic(path))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

async fn commit_config(state: &AppState, config: &Config) -> Result<(), ApiError> {
    validate_config(config)?;
    let previous = state.config.read().await.clone();
    let video_changed = previous.video != config.video;
    persist_config(state.config_path.as_ref(), config).await?;
    if video_changed {
        if let Err(error) = state.video.reconfigure(config.video.clone()) {
            tracing::error!(%error, "failed to reconfigure video after saving configuration");
            if let Err(restore_error) = persist_config(state.config_path.as_ref(), &previous).await {
                tracing::error!(message = %restore_error.message, "failed to restore previous configuration");
            }
            return Err(ApiError::internal(error));
        }

        // Capture changes also invalidate the H.264 source feeding WebRTC.
        state.webrtc.reconfigure(config.video.h264.clone()).await;
    }
    let power_led = power_led_snapshot(&config.power);
    *state.config.write().await = config.clone();
    state.proxy_protocol_updates.send_replace(config.server.proxy_protocol);
    state.power.set_power_led_config(power_led).await;
    state.hid.set_keyboard_device(config.hid.keyboard_device.clone());
    Ok(())
}

fn validate_config(config: &Config) -> Result<(), ApiError> {
    if config.version != CONFIG_VERSION {
        return Err(ApiError::bad_request("配置版本不受支持"));
    }
    config.server.socket_address().map_err(|error| match error {
        config::ConfigError::InvalidListenAddress { .. } => ApiError::bad_request(
            "server.listen_address 必须是不带方括号或空白的 IPv4/IPv6 地址",
        ),
        config::ConfigError::InvalidServerPort { .. } => {
            ApiError::bad_request("server.port 必须在 1 到 65535 之间")
        }
        _ => ApiError::bad_request("无效的服务器监听地址"),
    })?;
    if let Some(origin) = &config.server.public_origin {
        config::NormalizedOrigin::parse(origin)
            .map_err(|reason| ApiError::bad_request(format!("无效的 server.public_origin: {reason}")))?;
    }
    if config.server.trusted_http_proxies.len() > 64
        || config.server.trusted_http_proxies.iter().any(|ip| ip.is_unspecified() || ip.is_multicast())
    {
        return Err(ApiError::bad_request("可信 HTTP 代理最多配置 64 个明确的单播 IP 地址"));
    }
    // Width and height are optional independently; validate each on its own
    // so a partial override cannot persist an out-of-range capture size.
    if config.video.width.is_some_and(|width| !(160..=7680).contains(&width))
        || config.video.height.is_some_and(|height| !(120..=4320).contains(&height))
    {
        return Err(ApiError::bad_request("视频分辨率超出允许范围"));
    }
    if config
        .video
        .frames_per_second
        .is_some_and(|fps| !(1..=120).contains(&fps))
    {
        return Err(ApiError::bad_request("视频帧率必须在 1 到 120 之间"));
    }
    if !(1..=100).contains(&config.video.jpeg_quality) {
        return Err(ApiError::bad_request("JPEG 质量必须在 1 到 100 之间"));
    }
    if !(256..=50_000).contains(&config.video.h264.bitrate_kbps) {
        return Err(ApiError::bad_request(
            "H.264 码率必须在 256 到 50000 Kbps 之间",
        ));
    }
    if !(1..=4).contains(&config.video.h264.max_sessions) {
        return Err(ApiError::bad_request("H.264 同时会话数必须在 1 到 4 之间"));
    }
    config
        .video
        .h264
        .validate_udp_mapping()
        .map_err(ApiError::bad_request)?;
    for path in [
        config.video.device.as_ref(),
        config.video.h264.ffmpeg_path.as_ref(),
        config.hid.keyboard_device.as_ref(),
        config.hid.mouse_device.as_ref(),
        config.hid.absolute_pointer_device.as_ref(),
        config.media.image_directory.as_ref(),
        config.media.lun_path.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !path.is_absolute() {
            return Err(ApiError::bad_request("设备和存储路径必须是绝对路径"));
        }
    }
    if config.hid.pointer_mode == PointerMode::Absolute
        && config.hid.absolute_pointer_device.is_none()
        && !config.hid.auto_detect
    {
        return Err(ApiError::bad_request("绝对指针模式需要配置绝对指针设备"));
    }
    if config.hid.pointer_mode == PointerMode::Relative
        && config.hid.mouse_device.is_none()
        && !config.hid.auto_detect
    {
        return Err(ApiError::bad_request("相对指针模式需要配置相对鼠标设备"));
    }
    if config.power.enabled
        && configured_gpio(config.power.gpio_chip.as_ref(), config.power.gpio_line).is_none()
    {
        return Err(ApiError::bad_request(
            "启用电源控制前必须配置 GPIO 芯片和线路",
        ));
    }
    if config.power.gpio_chip.is_some() != config.power.gpio_line.is_some() {
        return Err(ApiError::bad_request("电源 GPIO 芯片和线路必须同时配置"));
    }
    if config
        .power
        .gpio_chip
        .as_deref()
        .is_some_and(|chip| chip.trim().is_empty())
    {
        return Err(ApiError::bad_request("电源 GPIO 芯片不能为空"));
    }
    if !(50..=2_000).contains(&config.power.short_press_ms) {
        return Err(ApiError::bad_request(
            "电源短按时长必须在 50 到 2000 毫秒之间",
        ));
    }
    if !(1_000..=15_000).contains(&config.power.long_press_ms)
        || config.power.long_press_ms < config.power.short_press_ms
    {
        return Err(ApiError::bad_request(
            "电源长按时长必须在 1000 到 15000 毫秒之间且不短于短按",
        ));
    }
    if let Some(reset) = &config.power.reset_switch {
        if reset.gpio_chip.is_some() != reset.gpio_line.is_some() {
            return Err(ApiError::bad_request("重启 GPIO 芯片和线路必须同时配置"));
        }
        if !gpio_pulse_is_configured(reset) {
            return Err(ApiError::bad_request(
                "重启 GPIO 配置必须同时包含芯片和线路",
            ));
        }
        if !(50..=2_000).contains(&reset.pulse_ms) {
            return Err(ApiError::bad_request(
                "重启脉冲时长必须在 50 到 2000 毫秒之间",
            ));
        }
    }
    if let Some(power_led) = &config.power.power_led {
        if power_led.gpio_chip.is_some() != power_led.gpio_line.is_some() {
            return Err(ApiError::bad_request("PWR LED GPIO 芯片和线路必须同时配置"));
        }
        if !gpio_input_is_configured(power_led) {
            return Err(ApiError::bad_request(
                "PWR LED GPIO 配置必须同时包含芯片和线路",
            ));
        }
        if !(100..=5_000).contains(&power_led.poll_interval_ms) {
            return Err(ApiError::bad_request(
                "PWR LED 轮询间隔必须在 100 到 5000 毫秒之间",
            ));
        }
        if power_led.debounce_ms > 5_000 {
            return Err(ApiError::bad_request(
                "PWR LED 去抖时长必须在 0 到 5000 毫秒之间",
            ));
        }
    }
    let mut gpio_lines: HashMap<(String, u32), &str> = HashMap::new();
    let mut check_gpio = |label: &'static str, chip: Option<&String>, line: Option<u32>| {
        if let (Some(chip), Some(line)) = (chip, line) {
            let key = (normalize_gpio_chip(chip), line);
            if gpio_lines.insert(key, label).is_some() {
                return Err(ApiError::bad_request(
                    "电源、重启和 PWR LED 不能共用同一个 GPIO 线路",
                ));
            }
        }
        Ok(())
    };
    check_gpio(
        "power",
        config.power.gpio_chip.as_ref(),
        config.power.gpio_line,
    )?;
    if let Some(reset) = &config.power.reset_switch {
        check_gpio("reset", reset.gpio_chip.as_ref(), reset.gpio_line)?;
    }
    if let Some(power_led) = &config.power.power_led {
        check_gpio(
            "power_led",
            power_led.gpio_chip.as_ref(),
            power_led.gpio_line,
        )?;
    }
    if config.media.enabled
        && (config.media.lun_path.is_none() || config.media.image_directory.is_none())
    {
        return Err(ApiError::bad_request(
            "启用虚拟介质前必须配置 LUN 和镜像目录",
        ));
    }
    if config.media.max_upload_bytes == 0 {
        return Err(ApiError::bad_request("镜像上传大小上限必须大于零"));
    }
    let hid_paths = [
        config.hid.keyboard_device.as_ref(),
        config.hid.mouse_device.as_ref(),
        config.hid.absolute_pointer_device.as_ref(),
    ];
    for (index, path) in hid_paths.iter().enumerate() {
        if path.is_some() && hid_paths[index + 1..].contains(path) {
            return Err(ApiError::bad_request("每个 HID 功能必须使用不同的设备路径"));
        }
    }
    Ok(())
}

fn normalize_gpio_chip(chip: &str) -> String {
    let chip = chip.trim();
    Path::new(chip)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(chip)
        .to_ascii_lowercase()
}

fn optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn optional_path(value: Option<String>) -> Option<PathBuf> {
    optional_string(value).map(PathBuf::from)
}

fn generate_token() -> Result<String, getrandom::Error> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn constant_time_eq(expected: &str, actual: &str) -> bool {
    let expected = expected.as_bytes();
    let actual = actual.as_bytes();
    let mut difference = expected.len() ^ actual.len();
    for index in 0..expected.len().max(actual.len()) {
        difference |= usize::from(
            expected.get(index).copied().unwrap_or_default()
                ^ actual.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn session_cookie_value(token: Option<&str>, secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}={}", token.unwrap_or_default());
    cookie.push_str("; HttpOnly; SameSite=Strict; Path=/");
    if secure {
        cookie.push_str("; Secure");
    }
    let max_age = if token.is_some() {
        SESSION_COOKIE_MAX_AGE_SECONDS
    } else {
        0
    };
    cookie.push_str(&format!("; Max-Age={max_age}"));
    cookie
}

fn with_session_cookie(
    mut response: Response,
    token: Option<&str>,
    secure: bool,
) -> Response {
    if let Ok(value) = HeaderValue::from_str(&session_cookie_value(token, secure)) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

fn no_store<T: IntoResponse>(value: T) -> Response {
    let mut response = value.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn map_auth_error(error: AuthError) -> ApiError {
    match error {
        AuthError::WeakPassword(policy) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            password_policy_message(policy),
        ),
        AuthError::InvalidUsername => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "账号需为 3–64 位，仅使用英文字母、数字、点、下划线或连字符",
        ),
        AuthError::AlreadyInitialized => ApiError::new(StatusCode::CONFLICT, error.to_string()),
        AuthError::NotInitialized => ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"),
        AuthError::PasskeyAlreadyExists => ApiError::new(StatusCode::CONFLICT, "此 Passkey 凭据已存在"),
        AuthError::InvalidPasskey(message) => ApiError::new(StatusCode::BAD_REQUEST, message),
        _ => ApiError::internal(error),
    }
}

fn password_policy_message(error: PasswordPolicyError) -> &'static str {
    match error {
        PasswordPolicyError::TooShort { .. }
        | PasswordPolicyError::MissingUppercase
        | PasswordPolicyError::MissingLowercase
        | PasswordPolicyError::MissingNumber
        | PasswordPolicyError::MissingSymbol => {
            "密码至少 12 位，并包含大写字母、小写字母、数字和符号"
        }
        PasswordPolicyError::TooLong { .. } => "密码过长",
        PasswordPolicyError::ControlCharacter => "密码不能包含控制字符",
    }
}

fn map_hid_error(error: HidError) -> ApiError {
    let status = match error {
        HidError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
        HidError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        HidError::UnsupportedKey(_)
        | HidError::InvalidButton(_)
        | HidError::InvalidAbsoluteCoordinates { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        HidError::NotConfigured | HidError::Io { .. } | HidError::ShortWrite { .. } => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        HidError::WorkerStopped => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ApiError::new(status, error.to_string())
}

fn map_power_error(error: PowerError) -> ApiError {
    let status = match error {
        PowerError::Busy => StatusCode::CONFLICT,
        PowerError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        PowerError::Read(_) => StatusCode::SERVICE_UNAVAILABLE,
        PowerError::Spawn(_) | PowerError::UnsupportedVersion(_) => StatusCode::SERVICE_UNAVAILABLE,
        PowerError::WorkerStopped => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ApiError::new(status, error.to_string())
}

fn map_media_error(error: MediaError) -> ApiError {
    let status = match error {
        MediaError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
        MediaError::InvalidName
        | MediaError::OutsideStorage
        | MediaError::UnsupportedType
        | MediaError::NotRegularFile
        | MediaError::EmptyImage => StatusCode::UNPROCESSABLE_ENTITY,
        MediaError::ImageNotFound => StatusCode::NOT_FOUND,
        MediaError::ForceEjectUnsupported
        | MediaError::AlreadyAttached(_)
        | MediaError::ReadOnlyFallback => StatusCode::CONFLICT,
        MediaError::IoTimedOut(_) | MediaError::StateTimedOut(_) => StatusCode::GATEWAY_TIMEOUT,
        MediaError::MissingLunAttribute(_) | MediaError::LunAttributeRejected(_) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        MediaError::Io(_) => StatusCode::SERVICE_UNAVAILABLE,
    };
    ApiError::new(status, error.to_string())
}

fn map_webrtc_error(error: WebRtcError) -> ApiError {
    let status = match &error {
        WebRtcError::InvalidOffer(_) => StatusCode::UNPROCESSABLE_ENTITY,
        WebRtcError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        WebRtcError::Capacity => StatusCode::TOO_MANY_REQUESTS,
        WebRtcError::Unauthorized => StatusCode::UNAUTHORIZED,
        WebRtcError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if matches!(&error, WebRtcError::Internal(_)) {
        tracing::error!(%error, "WebRTC request failed");
    }
    ApiError::new(status, error.to_string())
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

const LOGIN_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
const MAX_LOGIN_ATTEMPTS_PER_IP: usize = 5;
const MAX_PASSKEY_STARTS_PER_IP: usize = 10;
const MAX_LOGIN_LIMITER_RECORDS: usize = 4096;
const MAX_CONCURRENT_PASSWORD_VERIFICATIONS: usize = 2;

#[derive(Clone)]
struct LoginLimiter {
    attempts: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    password_slots: Arc<Semaphore>,
    per_ip: usize,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::with_budget(MAX_LOGIN_ATTEMPTS_PER_IP)
    }
}

impl LoginLimiter {
    fn with_budget(per_ip: usize) -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
            password_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_PASSWORD_VERIFICATIONS)),
            per_ip,
        }
    }

    /// Atomically admit one authentication submission for `ip`.
    ///
    /// At most five admissions are recorded per client IP in the rolling
    /// 60-second window. Every admitted password attempt and Passkey finish is
    /// counted exactly once, regardless of whether it succeeds; a successful
    /// login never clears earlier in-flight admissions. Expired timestamps are
    /// pruned during every admission, and the record map is bounded to 4096
    /// entries: an unseen IP at capacity fails closed.
    fn try_reserve(&self, ip: IpAddr) -> bool {
        self.try_reserve_at(ip, Instant::now())
    }

    fn try_reserve_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut attempts = self.attempts.lock().unwrap_or_else(|p| p.into_inner());
        prune_login_attempts(&mut attempts, now);
        if let Some(timestamps) = attempts.get_mut(&ip) {
            if timestamps.len() >= self.per_ip {
                return false;
            }
            timestamps.push_back(now);
            true
        } else if attempts.len() >= MAX_LOGIN_LIMITER_RECORDS {
            false
        } else {
            attempts.insert(ip, VecDeque::from([now]));
            true
        }
    }

    /// Check admission capacity without recording an attempt or allocating a
    /// new per-IP record. Also prunes expired records.
    fn has_capacity(&self, ip: IpAddr) -> bool {
        self.has_capacity_at(ip, Instant::now())
    }

    fn has_capacity_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut attempts = self.attempts.lock().unwrap_or_else(|p| p.into_inner());
        prune_login_attempts(&mut attempts, now);
        attempts.get(&ip).map_or(
            attempts.len() < MAX_LOGIN_LIMITER_RECORDS,
            |timestamps| timestamps.len() < self.per_ip,
        )
    }

    fn try_acquire_password_slot(&self) -> Option<OwnedSemaphorePermit> {
        self.password_slots.clone().try_acquire_owned().ok()
    }
}

fn prune_login_attempts(attempts: &mut HashMap<IpAddr, VecDeque<Instant>>, now: Instant) {
    attempts.retain(|_, timestamps| {
        // Concurrent callers can sample time before acquiring the mutex in a
        // different order, so expired entries need not be at the front.
        timestamps.retain(|at| now.saturating_duration_since(*at) < LOGIN_ATTEMPT_WINDOW);
        !timestamps.is_empty()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::hid::KeyboardLeds;

    #[test]
    fn twelve_concurrent_reservations_admit_exactly_five() {
        let limiter = LoginLimiter::default();
        let ip: IpAddr = "203.0.113.10".parse().unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(13));
        let admitted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..12 {
            let limiter = limiter.clone();
            let barrier = Arc::clone(&barrier);
            let admitted = Arc::clone(&admitted);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                if limiter.try_reserve_at(ip, Instant::now()) {
                    admitted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }));
        }
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(
            admitted.load(std::sync::atomic::Ordering::SeqCst),
            MAX_LOGIN_ATTEMPTS_PER_IP
        );
    }

    #[test]
    fn per_ip_budgets_are_independent_and_refill_after_the_window() {
        let limiter = LoginLimiter::default();
        let first: IpAddr = "198.51.100.1".parse().unwrap();
        let second: IpAddr = "198.51.100.2".parse().unwrap();
        let start = Instant::now();
        for _ in 0..MAX_LOGIN_ATTEMPTS_PER_IP {
            assert!(limiter.try_reserve_at(first, start));
        }
        assert!(!limiter.has_capacity_at(first, start));
        assert!(!limiter.try_reserve_at(first, start));
        assert!(limiter.try_reserve_at(second, start));

        let just_before = start + LOGIN_ATTEMPT_WINDOW - Duration::from_millis(1);
        assert!(!limiter.try_reserve_at(first, just_before));
        let refilled = start + LOGIN_ATTEMPT_WINDOW;
        assert!(limiter.try_reserve_at(first, refilled));
        assert!(limiter.has_capacity_at(first, refilled));

        let reordered = LoginLimiter::default();
        assert!(reordered.try_reserve_at(first, refilled));
        for _ in 1..MAX_LOGIN_ATTEMPTS_PER_IP {
            assert!(reordered.try_reserve_at(first, start));
        }
        assert!(reordered.try_reserve_at(first, refilled));
    }

    #[test]
    fn limiter_record_cap_fails_closed_and_prunes_expired_entries() {
        let limiter = LoginLimiter::default();
        let start = Instant::now();
        for index in 0..MAX_LOGIN_LIMITER_RECORDS {
            let ip = IpAddr::V4(std::net::Ipv4Addr::from(index as u32 + 1));
            assert!(limiter.try_reserve_at(ip, start));
        }
        let new_ip: IpAddr = "203.0.113.250".parse().unwrap();
        assert!(!limiter.has_capacity_at(new_ip, start));
        assert!(!limiter.try_reserve_at(new_ip, start));

        let after_window = start + LOGIN_ATTEMPT_WINDOW;
        assert!(limiter.try_reserve_at(new_ip, after_window));
        assert_eq!(limiter.attempts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn global_password_slots_cap_verification_across_ips_and_release() {
        let limiter = LoginLimiter::default();
        let now = Instant::now();
        assert!(limiter.try_reserve_at("198.51.100.1".parse().unwrap(), now));
        assert!(limiter.try_reserve_at("198.51.100.2".parse().unwrap(), now));

        let first = limiter.try_acquire_password_slot().expect("first slot");
        let second = limiter.try_acquire_password_slot().expect("second slot");
        assert!(limiter.try_acquire_password_slot().is_none());
        drop(first);
        let replacement = limiter
            .try_acquire_password_slot()
            .expect("released slot");
        assert!(limiter.try_acquire_password_slot().is_none());
        drop((second, replacement));
        assert_eq!(limiter.password_slots.available_permits(), 2);
    }

    #[tokio::test]
    async fn aborting_the_http_task_keeps_the_slot_until_blocking_verification_ends() {
        let limiter = LoginLimiter::default();
        let held = limiter.try_acquire_password_slot().expect("held slot");
        let moved = limiter
            .try_acquire_password_slot()
            .expect("verification slot");
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let task = tokio::spawn(run_with_password_slot(moved, move || {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            Ok::<bool, AuthError>(true)
        }));

        started_rx.await.expect("verification should start");
        task.abort();
        let _ = task.await;
        // The HTTP task is gone, but the blocking work still owns its permit.
        assert!(limiter.try_acquire_password_slot().is_none());

        release_tx.send(()).expect("release blocking verification");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if limiter.try_acquire_password_slot().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("slot should be released when blocking verification finishes");
        drop(held);
    }

    #[cfg(unix)]
    fn helper_fixture(name: &str) -> PathBuf {
        let id = TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wingmankvm-helper-{}-{id}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(unix)]
    fn script_helper(root: &Path, script: &str, deadline: Duration) -> SystemPasswordHelper {
        SystemPasswordHelper {
            program: PathBuf::from("/bin/sh"),
            args: vec![
                "-c".to_owned(),
                script.to_owned(),
                "wingman-helper-test".to_owned(),
                root.to_string_lossy().into_owned(),
            ],
            deadline,
        }
    }

    #[cfg(unix)]
    fn process_is_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(unix)]
    async fn wait_for_pid_file(path: &Path) -> String {
        for _ in 0..100 {
            if let Ok(pid) = fs::read_to_string(path) {
                if !pid.trim().is_empty() {
                    return pid.trim().to_owned();
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("helper fixture did not record its pid");
    }

    #[cfg(unix)]
    async fn wait_until_process_exits(pid: &str) {
        for _ in 0..200 {
            if !process_is_alive(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("helper child {pid} was not killed and reaped");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_pipes_exact_password_without_newline() {
        let root = helper_fixture("success");
        let helper = script_helper(&root, r#"cat > "$1/password""#, Duration::from_secs(2));
        sync_system_password_with("Correct-Horse7!", &helper)
            .await
            .unwrap();
        assert_eq!(
            fs::read(root.join("password")).unwrap(),
            b"Correct-Horse7!"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_reports_a_failed_exit() {
        let root = helper_fixture("failure");
        let helper = script_helper(
            &root,
            r#"cat >/dev/null; exit 7"#,
            Duration::from_secs(2),
        );
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Exit)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_times_out_and_reaps_the_child() {
        let root = helper_fixture("hang");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; exec sleep 30"#,
            Duration::from_secs(1),
        );
        let started = Instant::now();
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid = wait_for_pid_file(&root.join("pid")).await;
        wait_until_process_exits(&pid).await;
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_the_http_side_still_kills_and_reaps_the_helper() {
        let root = helper_fixture("cancel");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; /bin/cat >/dev/null; exec sleep 30"#,
            Duration::from_secs(1),
        );
        let pid_path = root.join("pid");
        let task = tokio::spawn(async move {
            sync_system_password_with("Correct-Horse7!", &helper).await
        });
        let pid = wait_for_pid_file(&pid_path).await;
        task.abort();
        let _ = task.await;
        wait_until_process_exits(&pid).await;
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_login_migration_keeps_the_guard_until_the_helper_ends() {
        let root = helper_fixture("migration-cancel");
        let store = AuthStore::new(root.join("auth.json"));
        store.initialize("admin", "Correct-Horse7!").unwrap();
        let verified = store.load().unwrap().unwrap();
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; exec sleep 2"#,
            Duration::from_secs(4),
        );
        let migration = Arc::new(AsyncMutex::new(()));
        let task = tokio::spawn(migrate_system_password_on_login(
            migration.clone(),
            store.clone(),
            verified,
            "Correct-Horse7!".to_owned(),
            helper,
        ));
        let pid = wait_for_pid_file(&root.join("pid")).await;
        task.abort();
        let _ = task.await;
        assert!(
            migration.clone().try_lock_owned().is_err(),
            "caller cancellation must not release the migration guard"
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(guard) = migration.clone().try_lock_owned() {
                drop(guard);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "migration guard was not released after the helper ended"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        wait_until_process_exits(&pid).await;
        assert!(store.load().unwrap().unwrap().system_password_synced);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn group_cleanup_terminates_owned_descendants() {
        let root = helper_fixture("descendants");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; /bin/sleep 30 & printf '%s' "$!" > "$1/descendant"; wait"#,
            Duration::from_secs(1),
        );
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Timeout)
        ));
        let leader = wait_for_pid_file(&root.join("pid")).await;
        let descendant = wait_for_pid_file(&root.join("descendant")).await;
        wait_until_process_exits(&leader).await;
        wait_until_process_exits(&descendant).await;
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn video_form_updates_preserve_udp_mapping_and_null_clears_it() {
        let mut video = config::VideoConfig::default();
        let patch = serde_json::from_value(json!({"h264": {
            "udp_bind_address": "0.0.0.0:8080",
            "public_udp_endpoint": "kvm.example.com:38319"
        }}))
        .unwrap();
        apply_video_patch(&mut video, patch);
        let patch = serde_json::from_value(json!({"h264": {"bitrate_kbps": 8000}})).unwrap();
        apply_video_patch(&mut video, patch);
        assert_eq!(
            video.h264.udp_bind_address,
            Some("0.0.0.0:8080".parse().unwrap())
        );
        assert_eq!(
            video.h264.public_udp_endpoint.as_deref(),
            Some("kvm.example.com:38319")
        );
        let patch = serde_json::from_value(
            json!({"h264": {"udp_bind_address": null, "public_udp_endpoint": null}}),
        )
        .unwrap();
        apply_video_patch(&mut video, patch);
        assert!(video.h264.udp_bind_address.is_none());
        assert!(video.h264.public_udp_endpoint.is_none());
    }

    #[cfg(unix)]
    use std::{
        fs,
        os::unix::fs::symlink,
        sync::atomic::{AtomicU64, Ordering},
    };

    #[cfg(unix)]
    static TEST_PATH_ID: AtomicU64 = AtomicU64::new(0);

    fn candidate(path: &str, kind: &str) -> discovery::DeviceCandidate {
        discovery::DeviceCandidate {
            path: PathBuf::from(path),
            label: path.to_owned(),
            kind: kind.to_owned(),
            warnings: Vec::new(),
            gadget: None,
            function: None,
            udc: None,
            gadget_bound: None,
            function_linked: None,
            compatible: None,
            device_major: None,
            device_minor: None,
            subclass: None,
            protocol: None,
            report_length: None,
            card: None,
            driver: None,
            video_capture: None,
            supports_mjpeg: None,
        }
    }

    fn ready_gadget_candidate(path: &str, kind: &str) -> discovery::DeviceCandidate {
        let mut candidate = candidate(path, kind);
        candidate.compatible = Some(true);
        candidate.function_linked = Some(true);
        candidate.gadget_bound = Some(true);
        candidate
    }

    fn empty_discovery() -> discovery::DeviceDiscovery {
        discovery::DeviceDiscovery {
            video: Vec::new(),
            hid: Vec::new(),
            keyboard: Vec::new(),
            mouse: Vec::new(),
            absolute_pointer: Vec::new(),
            gpio: Vec::new(),
            mass_storage_luns: Vec::new(),
        }
    }

    #[test]
    fn setup_token_comparison_checks_length_and_contents() {
        assert!(constant_time_eq("correct-token", "correct-token"));
        assert!(!constant_time_eq("correct-token", "wrong-token"));
        assert!(!constant_time_eq("correct-token", "correct-token-extra"));
    }

    #[test]
    fn session_cookie_issuance_and_deletion_share_attributes() {
        let issued = session_cookie_value(Some("token"), false);
        assert_eq!(
            issued,
            "wingman_session=token; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200"
        );
        assert!(!issued.contains("; Secure"));

        let issued_secure = session_cookie_value(Some("token"), true);
        assert_eq!(
            issued_secure,
            "wingman_session=token; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=43200"
        );

        let deleted = session_cookie_value(None, false);
        assert_eq!(
            deleted,
            "wingman_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
        );
        let deleted_secure = session_cookie_value(None, true);
        assert_eq!(
            deleted_secure,
            "wingman_session=; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=0"
        );

        let response = with_session_cookie(StatusCode::OK.into_response(), None, true);
        assert_eq!(
            response.headers().get(SET_COOKIE).unwrap(),
            deleted_secure.as_str()
        );
    }

    #[test]
    fn session_cookie_policy_ignores_host_origin_and_forwarding_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("kvm.example.com"));
        headers.insert("origin", HeaderValue::from_static("https://kvm.example.com"));
        headers.insert(
            "x-forwarded-proto",
            HeaderValue::from_static("https"),
        );
        headers.insert("x-forwarded-ssl", HeaderValue::from_static("on"));
        headers.insert("forwarded", HeaderValue::from_static("proto=https"));

        let mut server = config::ServerConfig::default();
        assert!(!server.session_cookie_secure());
        assert!(
            !session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );

        server.public_origin = Some("http://kvm.example.com".to_owned());
        assert!(!server.session_cookie_secure());
        assert!(
            !session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );

        // HTTPS stays HTTPS even when every request header tries to downgrade
        // it. The headers above are never an input to this decision.
        server.public_origin = Some("https://kvm.example.com".to_owned());
        headers.insert(
            "x-forwarded-proto",
            HeaderValue::from_static("http"),
        );
        assert!(server.session_cookie_secure());
        assert!(
            session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );
    }

    #[test]
    fn automatic_gadget_selection_requires_verified_ready_state() {
        let mut unknown = candidate("/dev/hidg0", "keyboard");
        assert!(!candidate_is_ready(&unknown));

        unknown.compatible = Some(true);
        unknown.function_linked = Some(true);
        unknown.gadget_bound = Some(false);
        assert!(!candidate_is_ready(&unknown));

        unknown.gadget_bound = Some(true);
        assert!(candidate_is_ready(&unknown));
    }

    #[test]
    fn automatic_video_selection_uses_only_unique_mjpeg_capture_device() {
        let mut raw = candidate("/dev/video0", "video");
        raw.video_capture = Some(true);
        raw.supports_mjpeg = Some(false);
        let mut mjpeg = candidate("/dev/video1", "video");
        mjpeg.video_capture = Some(true);
        mjpeg.supports_mjpeg = Some(true);

        let selected = unique_usable_candidate(
            &[raw, mjpeg.clone()],
            |candidate| video_candidate_is_usable_with_probe_requirement(candidate, true),
            "multiple",
        )
        .unwrap();
        assert_eq!(selected, Some(PathBuf::from("/dev/video1")));

        let error = unique_usable_candidate(
            &[mjpeg.clone(), {
                let mut second = mjpeg;
                second.path = PathBuf::from("/dev/video2");
                second
            }],
            |candidate| video_candidate_is_usable_with_probe_requirement(candidate, true),
            "检测到多个可用视频设备",
        )
        .unwrap_err();
        assert_eq!(error.message, "检测到多个可用视频设备");
    }

    #[test]
    fn linux_video_probe_must_confirm_capture_and_mjpeg() {
        let unprobed = candidate("/dev/video0", "video");
        assert!(!video_candidate_is_usable_with_probe_requirement(
            &unprobed, true
        ));
        assert!(video_candidate_is_usable_with_probe_requirement(
            &unprobed, false
        ));

        let mut capture = unprobed;
        capture.video_capture = Some(true);
        capture.supports_mjpeg = Some(true);
        assert!(video_candidate_is_usable_with_probe_requirement(
            &capture, true
        ));
    }

    #[test]
    fn explicit_video_must_come_from_the_latest_scan() {
        let error = resolve_setup_video(Some(PathBuf::from("/dev/video9")), &[]).unwrap_err();
        assert!(error.message.contains("不在本次扫描结果"));

        let mut raw = candidate("/dev/video0", "video");
        raw.video_capture = Some(true);
        raw.supports_mjpeg = Some(false);
        let error = resolve_setup_video(Some(raw.path.clone()), &[raw]).unwrap_err();
        assert!(error.message.contains("不支持 MJPEG"));
    }

    #[test]
    fn explicit_hid_rejects_a_known_different_role() {
        let mouse = ready_gadget_candidate("/dev/hidg1", "mouse");
        let mut discovered = empty_discovery();
        discovered.hid.push(candidate("/dev/hidg1", "hid_gadget"));
        discovered.mouse.push(mouse);

        let error = resolve_setup_hid(
            Some(PathBuf::from("/dev/hidg1")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap_err();
        assert!(error.message.contains("实际被识别为相对鼠标设备"));
    }

    #[cfg(unix)]
    #[test]
    fn explicit_stable_symlink_matches_the_scanned_device_node() {
        let id = TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wingmankvm-web-device-match-{}-{id}",
            std::process::id()
        ));
        let node = root.join("hidg0");
        let stable = root.join("wingmankvm-keyboard");
        fs::create_dir_all(&root).unwrap();
        fs::write(&node, []).unwrap();
        symlink(&node, &stable).unwrap();

        let scanned = ready_gadget_candidate(node.to_str().unwrap(), "keyboard");
        assert!(find_candidate(&stable, &[scanned]).is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_unclassified_hid_is_the_only_manual_role_escape_hatch() {
        let mut discovered = empty_discovery();
        discovered.hid.push(candidate("/dev/hidg0", "hid_gadget"));

        let selected = resolve_setup_hid(
            Some(PathBuf::from("/dev/hidg0")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap();
        assert_eq!(selected, Some(PathBuf::from("/dev/hidg0")));

        assert_eq!(
            resolve_setup_hid(None, SetupHidRole::Keyboard, &discovered).unwrap(),
            None
        );
        let error = resolve_setup_hid(
            Some(PathBuf::from("/dev/not-a-hid-node")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap_err();
        assert!(error.message.contains("不在本次 HID 扫描结果"));
    }

    #[test]
    fn explicit_and_automatic_luns_require_bound_linked_compatible_candidates() {
        let path = "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.0/lun.0";

        let mut incompatible = ready_gadget_candidate(path, "mass_storage_lun");
        incompatible.compatible = Some(false);
        let error =
            resolve_setup_lun(Some(incompatible.path.clone()), true, &[incompatible]).unwrap_err();
        assert!(error.message.contains("描述符不兼容"));

        let mut unlinked = ready_gadget_candidate(path, "mass_storage_lun");
        unlinked.function_linked = Some(false);
        let error = resolve_setup_lun(Some(unlinked.path.clone()), true, &[unlinked]).unwrap_err();
        assert!(error.message.contains("尚未链接"));

        let mut unbound = ready_gadget_candidate(path, "mass_storage_lun");
        unbound.gadget_bound = Some(false);
        let error = resolve_setup_lun(Some(unbound.path.clone()), true, &[unbound]).unwrap_err();
        assert!(error.message.contains("尚未绑定 UDC"));

        let first = ready_gadget_candidate(
            "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.0/lun.0",
            "mass_storage_lun",
        );
        let second = ready_gadget_candidate(
            "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.1/lun.0",
            "mass_storage_lun",
        );
        let error = resolve_setup_lun(None, true, &[first, second]).unwrap_err();
        assert!(error.message.contains("多个可用的 USB 虚拟介质"));
    }

    #[test]
    fn configuration_validation_rejects_unsafe_values() {
        let mut config = Config::default();
        config.video.frames_per_second = Some(121);
        assert!(validate_config(&config).is_err());

        for (width, height) in [(Some(0), None), (Some(99_999), None), (None, Some(0)), (None, Some(9_999))] {
            let mut config = Config::default();
            config.video.width = width;
            config.video.height = height;
            assert!(validate_config(&config).is_err(), "{width:?}x{height:?} must be rejected");
        }
        let mut config = Config::default();
        config.video.width = Some(1920);
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("dev/hidg0"));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Absolute;
        config.hid.auto_detect = false;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Relative;
        config.hid.auto_detect = false;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Absolute;
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Relative;
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.video.h264.bitrate_kbps = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.video.h264.max_sessions = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.enabled = true;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("/dev/hidg0"));
        config.hid.absolute_pointer_device = Some(PathBuf::from("/dev/hidg0"));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.media.enabled = true;
        config.media.lun_path = Some(PathBuf::from(
            "/sys/kernel/config/usb_gadget/wingman/functions/mass_storage.0/lun.0",
        ));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.media.max_upload_bytes = 0;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.server.public_origin = Some("https://kvm.example.com".to_owned());
        assert!(validate_config(&config).is_ok());
        config.server.public_origin = Some("https://user@kvm.example.com".to_owned());
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn configuration_validation_rejects_invalid_server_listener() {
        for address in ["localhost", "kvm.example.com", "[::1]", " 127.0.0.1"] {
            let mut config = Config::default();
            config.server.listen_address = address.to_owned();
            let error = validate_config(&config).unwrap_err();
            assert!(error.message.contains("listen_address"), "{address:?}");
        }

        let mut config = Config::default();
        config.server.port = 0;
        let error = validate_config(&config).unwrap_err();
        assert!(error.message.contains("server.port"));

        let mut config = Config::default();
        config.server.listen_address = "::1".to_owned();
        config.server.port = 9090;
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn legacy_display_patch_does_not_override_capture_settings() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "display": {"virtual_monitor": "hd720p60", "control_device": "/dev/hidraw0"},
            "video": {"follow_display": true, "jpeg_quality": 75}
        }))
        .unwrap();
        let mut config = Config::default();
        config.video = config::VideoConfig {
            device: Some(PathBuf::from("/dev/video5")),
            width: Some(1920),
            height: Some(1080),
            frames_per_second: Some(60),
            ..config::VideoConfig::default()
        };
        apply_video_patch(&mut config.video, patch.video.unwrap());
        assert!(validate_config(&config).is_ok());
        assert_eq!(config.video.width, Some(1920));
        assert_eq!(config.video.height, Some(1080));
        assert_eq!(config.video.frames_per_second, Some(60));
        assert_eq!(config.video.jpeg_quality, 75);
    }

    #[test]
    fn gpio_validation_rejects_duplicate_lines_and_unsafe_timing() {
        let mut config = Config::default();
        config.power.enabled = true;
        config.power.gpio_chip = Some("   ".to_owned());
        config.power.gpio_line = Some(7);
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.gpio_chip = Some("/dev/gpiochip1".to_owned());
        config.power.gpio_line = Some(7);
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(7),
            ..GpioPulseConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.short_press_ms = 49;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.long_press_ms = 999;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(10),
            pulse_ms: 2_001,
            ..GpioPulseConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            poll_interval_ms: 99,
            ..GpioInputConfig::default()
        });
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            debounce_ms: 5_001,
            ..GpioInputConfig::default()
        });
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn hid_patch_distinguishes_omitted_fields_from_null() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "hid": {
                "absolute_pointer_device": "/dev/hidg2",
                "pointer_mode": "absolute"
            }
        }))
        .unwrap();
        let mut config = HidConfig {
            keyboard_device: Some(PathBuf::from("/dev/hidg0")),
            mouse_device: Some(PathBuf::from("/dev/hidg1")),
            ..HidConfig::default()
        };
        apply_hid_patch(&mut config, patch.hid.unwrap());

        assert_eq!(config.keyboard_device, Some(PathBuf::from("/dev/hidg0")));
        assert_eq!(config.mouse_device, Some(PathBuf::from("/dev/hidg1")));
        assert_eq!(
            config.absolute_pointer_device,
            Some(PathBuf::from("/dev/hidg2"))
        );
        assert_eq!(config.pointer_mode, PointerMode::Absolute);

        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "hid": { "absolute_pointer_device": null }
        }))
        .unwrap();
        apply_hid_patch(&mut config, patch.hid.unwrap());
        assert_eq!(config.absolute_pointer_device, None);
        assert_eq!(config.keyboard_device, Some(PathBuf::from("/dev/hidg0")));
    }

    #[test]
    fn deprecated_hid_timing_patch_fields_are_rejected_even_when_null() {
        for (field, value) in [
            ("write_timeout_ms", serde_json::Value::Null),
            ("write_timeout_ms", serde_json::json!(500)),
            ("write_timeout_ms", serde_json::json!(2_500)),
            ("retry_interval_ms", serde_json::Value::Null),
            ("retry_interval_ms", serde_json::json!(10)),
            ("retry_interval_ms", serde_json::json!(0)),
        ] {
            let mut hid = serde_json::Map::new();
            hid.insert(field.to_owned(), value);
            let error = serde_json::from_value::<ConfigPatch>(serde_json::json!({ "hid": hid }))
                .err()
                .expect("deprecated HID timing field must be rejected")
                .to_string();
            assert!(error.contains(&format!("hid.{field}")), "{error}");
            assert!(error.contains("deprecated and ignored"), "{error}");
        }
    }

    #[test]
    fn hid_patch_without_retired_timing_fields_still_applies() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "hid": { "keyboard_device": "/dev/hidg0", "pointer_mode": "relative" }
        }))
        .expect("valid HID patch");
        let mut config = HidConfig::default();
        apply_hid_patch(&mut config, patch.hid.expect("hid patch"));
        assert_eq!(config.keyboard_device, Some(PathBuf::from("/dev/hidg0")));
        assert_eq!(config.pointer_mode, PointerMode::Relative);
    }

    #[test]
    fn full_config_update_rejects_retired_hid_timing_fields_and_accepts_valid_config() {
        for (field, value) in [
            ("write_timeout_ms", serde_json::Value::Null),
            ("write_timeout_ms", serde_json::json!(500)),
            ("retry_interval_ms", serde_json::Value::Null),
            ("retry_interval_ms", serde_json::json!(10)),
        ] {
            let mut hid = serde_json::Map::new();
            hid.insert(field.to_owned(), value);
            let payload = serde_json::json!({
                "version": CONFIG_VERSION,
                "hid": hid
            });
            let error = serde_json::from_value::<FullConfigUpdate>(payload)
                .err()
                .expect("retired HID timing field must fail PUT decoding")
                .to_string();
            assert!(error.contains(&format!("hid.{field}")), "{error}");
            assert!(error.contains("deprecated and ignored"), "{error}");
        }

        let valid: FullConfigUpdate = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "server": { "port": 9090 },
            "hid": { "keyboard_device": "/dev/hidg0", "pointer_mode": "relative" }
        }))
        .expect("valid full configuration");
        assert_eq!(valid.0.server.port, 9090);
        assert_eq!(valid.0.hid.keyboard_device, Some(PathBuf::from("/dev/hidg0")));
        assert_eq!(valid.0.hid.pointer_mode, PointerMode::Relative);
    }

    #[test]
    fn video_and_power_patches_preserve_omitted_fields_and_accept_null() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "video": { "jpeg_quality": 72, "h264": { "bitrate_kbps": 2000 } },
            "power": { "active_high": false }
        }))
        .unwrap();
        let mut video = config::VideoConfig {
            auto_detect: false,
            device: Some(PathBuf::from("/dev/video5")),
            width: Some(1920),
            height: Some(1080),
            frames_per_second: Some(30),
            ..config::VideoConfig::default()
        };
        let mut power = config::PowerConfig {
            enabled: true,
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(7),
            ..config::PowerConfig::default()
        };
        apply_video_patch(&mut video, patch.video.unwrap());
        apply_power_patch(&mut power, patch.power.unwrap());

        assert_eq!(video.device, Some(PathBuf::from("/dev/video5")));
        assert_eq!(video.width, Some(1920));
        assert_eq!(video.height, Some(1080));
        assert_eq!(video.frames_per_second, Some(30));
        assert_eq!(video.jpeg_quality, 72);
        assert_eq!(video.h264.bitrate_kbps, 2_000);
        assert_eq!(video.h264.encoder, H264Encoder::Auto);
        assert!(!video.h264.allow_software);
        assert_eq!(power.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(power.gpio_line, Some(7));
        assert!(!power.active_high);

        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "video": { "device": null, "width": null },
            "power": { "gpio_chip": null, "gpio_line": null }
        }))
        .unwrap();
        apply_video_patch(&mut video, patch.video.unwrap());
        apply_power_patch(&mut power, patch.power.unwrap());

        assert_eq!(video.device, None);
        assert_eq!(video.width, None);
        assert_eq!(video.height, Some(1080));
        assert!(video.auto_detect);
        assert_eq!(power.gpio_chip, None);
        assert_eq!(power.gpio_line, None);
        assert!(power.enabled);
    }

    #[test]
    fn auxiliary_gpio_patches_preserve_nested_fields_and_accept_null() {
        let mut power = config::PowerConfig {
            reset_switch: Some(GpioPulseConfig {
                gpio_chip: Some("gpiochip1".to_owned()),
                gpio_line: Some(10),
                active_high: true,
                pulse_ms: 500,
            }),
            power_led: Some(GpioInputConfig {
                gpio_chip: Some("gpiochip1".to_owned()),
                gpio_line: Some(12),
                active_low: true,
                bias: GpioBias::PullUp,
                poll_interval_ms: 1_000,
                debounce_ms: 50,
            }),
            ..config::PowerConfig::default()
        };
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "power": {
                "reset_switch": { "pulse_ms": 700 },
                "power_led": { "debounce_ms": 100 }
            }
        }))
        .unwrap();
        apply_power_patch(&mut power, patch.power.unwrap());

        let reset = power.reset_switch.as_ref().unwrap();
        assert_eq!(reset.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(reset.gpio_line, Some(10));
        assert_eq!(reset.pulse_ms, 700);
        let power_led = power.power_led.as_ref().unwrap();
        assert_eq!(power_led.gpio_line, Some(12));
        assert_eq!(power_led.poll_interval_ms, 1_000);
        assert_eq!(power_led.debounce_ms, 100);

        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "power": { "reset_switch": null, "power_led": null }
        }))
        .unwrap();
        apply_power_patch(&mut power, patch.power.unwrap());
        assert!(power.reset_switch.is_none());
        assert!(power.power_led.is_none());
    }

    #[test]
    fn gpio_test_target_uses_explicit_power_or_reset_values() {
        let power: GpioTestRequest = serde_json::from_value(serde_json::json!({
            "target": "power"
        }))
        .unwrap();
        assert!(matches!(power.target, GpioTestTarget::Power));
        assert_eq!(power.duration_ms, None);

        let reset: GpioTestRequest = serde_json::from_value(serde_json::json!({
            "target": "reset",
            "duration_ms": 150
        }))
        .unwrap();
        assert!(matches!(reset.target, GpioTestTarget::Reset));
        assert_eq!(reset.duration_ms, Some(150));
        assert!(
            serde_json::from_value::<GpioTestRequest>(serde_json::json!({
                "target": "power_led"
            }))
            .is_err()
        );
    }

    #[test]
    fn power_test_snapshot_requires_enabled_nonempty_configuration() {
        let mut config = config::PowerConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(7),
            ..config::PowerConfig::default()
        };
        assert!(power_pulse_snapshot(&config, Some(150)).is_none());

        config.enabled = true;
        assert_eq!(
            power_pulse_snapshot(&config, Some(150))
                .expect("enabled power GPIO")
                .pulse_ms,
            150
        );

        config.gpio_chip = Some(" ".to_owned());
        assert!(power_pulse_snapshot(&config, Some(150)).is_none());
    }

    #[test]
    fn media_patch_preserves_omitted_paths_and_accepts_explicit_null() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "media": { "read_only_by_default": false }
        }))
        .unwrap();
        let mut config = config::MediaConfig {
            enabled: true,
            lun_path: Some(PathBuf::from(
                "/sys/kernel/config/usb_gadget/wingman/functions/mass_storage.0/lun.0",
            )),
            image_directory: Some(PathBuf::from("/var/lib/wingmankvm/images")),
            ..config::MediaConfig::default()
        };
        apply_media_patch(&mut config, patch.media.unwrap());

        assert!(config.enabled);
        assert!(config.lun_path.is_some());
        assert!(config.image_directory.is_some());
        assert!(!config.read_only_by_default);

        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "media": { "lun_path": null }
        }))
        .unwrap();
        apply_media_patch(&mut config, patch.media.unwrap());
        assert_eq!(config.lun_path, None);
        assert!(config.image_directory.is_some());
    }

    #[test]
    fn setup_payload_keeps_explicit_capability_choices() {
        let request: SetupRequest = serde_json::from_value(serde_json::json!({
            "username": "admin",
            "password": "Strong-password-123",
            "setup_token": "token",
            "power_enabled": true,
            "gpio_chip": "gpiochip1",
            "gpio_line": 7,
            "active_high": false,
            "media_enabled": true
        }))
        .unwrap();

        assert_eq!(request.power_enabled, Some(true));
        assert_eq!(request.media_enabled, Some(true));
        assert_eq!(request.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(request.gpio_line, Some(7));
        assert_eq!(request.active_high, Some(false));
    }

    #[test]
    fn virtual_media_attach_payload_supports_writable_disks() {
        let request: AttachMediaRequest = serde_json::from_value(serde_json::json!({
            "name": "storage.img",
            "media_type": "disk",
            "read_only": false
        }))
        .unwrap();
        assert_eq!(request.name, "storage.img");
        assert_eq!(request.media_type, MediaType::Disk);
        assert_eq!(request.read_only, Some(false));

        let request: AttachMediaRequest = serde_json::from_value(serde_json::json!({
            "name": "installer.iso"
        }))
        .unwrap();
        assert_eq!(request.media_type, MediaType::Auto);
        assert_eq!(request.read_only, None);
    }

    #[test]
    fn virtual_media_timeouts_map_to_gateway_timeout() {
        assert_eq!(
            map_media_error(MediaError::StateTimedOut("detached")).status,
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            map_media_error(MediaError::ForceEjectUnsupported).status,
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn capabilities_resolve_automatic_pointer_mode() {
        let mut config = Config::default();
        config.hid.mouse_device = Some(PathBuf::from("/dev/hidg1"));
        let relative = capabilities(&config);
        assert!(relative.mouse);
        assert!(relative.mouse_relative);
        assert!(!relative.mouse_absolute);
        assert_eq!(relative.pointer_mode, PointerMode::Relative);

        config.hid.absolute_pointer_device = Some(PathBuf::from("/dev/hidg2"));
        let absolute = capabilities(&config);
        assert!(absolute.mouse);
        assert!(absolute.mouse_relative);
        assert!(absolute.mouse_absolute);
        assert_eq!(absolute.pointer_mode, PointerMode::Absolute);

        config.media.enabled = true;
        config.media.lun_path = Some(PathBuf::from(
            "/sys/kernel/config/usb_gadget/wingman/functions/mass_storage.0/lun.0",
        ));
        assert!(!capabilities(&config).mass_storage);
        config.media.image_directory = Some(PathBuf::from("/var/lib/wingmankvm/images"));
        assert!(capabilities(&config).mass_storage);

        assert!(!capabilities(&config).gpio_reset);
        assert!(!capabilities(&config).gpio_power_led);
        config.power.reset_switch = Some(GpioPulseConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(10),
            ..GpioPulseConfig::default()
        });
        config.power.power_led = Some(GpioInputConfig {
            gpio_chip: Some("gpiochip1".to_owned()),
            gpio_line: Some(12),
            ..GpioInputConfig::default()
        });
        let gpio = capabilities(&config);
        assert!(gpio.gpio_reset);
        assert!(gpio.gpio_power_led);
    }

    #[test]
    fn hid_auto_detection_follows_the_selected_pointer_mode() {
        let mut config = HidConfig {
            keyboard_device: Some(PathBuf::from("/dev/hidg0")),
            pointer_mode: PointerMode::Absolute,
            ..HidConfig::default()
        };
        assert!(hid_needs_auto_detection(&config));

        config.absolute_pointer_device = Some(PathBuf::from("/dev/hidg2"));
        assert!(!hid_needs_auto_detection(&config));

        config.pointer_mode = PointerMode::Relative;
        assert!(hid_needs_auto_detection(&config));
        config.mouse_device = Some(PathBuf::from("/dev/hidg1"));
        assert!(!hid_needs_auto_detection(&config));
    }

    #[test]
    fn absolute_pointer_api_payloads_use_an_explicit_action_tag() {
        let request: AbsolutePointerRequest = serde_json::from_value(serde_json::json!({
            "action": "click",
            "x": 32767,
            "y": 0,
            "button": 1
        }))
        .unwrap();
        assert_eq!(
            request,
            AbsolutePointerRequest::Click {
                x: 32767,
                y: 0,
                button: 1
            }
        );
        assert!(
            serde_json::from_value::<AbsolutePointerRequest>(serde_json::json!({
                "x": 1,
                "y": 2
            }))
            .is_err()
        );
    }

    #[test]
    fn origin_must_match_host_when_present() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("kvm.local:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://kvm.local:8080"));
        assert!(origin_matches_host(&headers));
        headers.insert(ORIGIN, HeaderValue::from_static("http://evil.invalid"));
        assert!(!origin_matches_host(&headers));
    }

    #[test]
    fn proxy_settings_patch_preserves_trust_unless_explicitly_changed() {
        let mut config = config::ServerConfig::default();
        config.trusted_http_proxies.push("127.0.0.1".parse().unwrap());
        let patch = serde_json::from_value(json!({"proxy_protocol": "auto"})).unwrap();
        apply_server_patch(&mut config, patch);
        assert_eq!(config.proxy_protocol, ProxyProtocolMode::Auto);
        assert_eq!(config.trusted_http_proxies.len(), 1);
        let patch = serde_json::from_value(json!({"trusted_http_proxies": []})).unwrap();
        apply_server_patch(&mut config, patch);
        assert!(config.trusted_http_proxies.is_empty());

        let patch =
            serde_json::from_value(json!({"public_origin": "https://kvm.example.com"})).unwrap();
        apply_server_patch(&mut config, patch);
        assert_eq!(
            config.public_origin.as_deref(),
            Some("https://kvm.example.com")
        );
        let patch = serde_json::from_value(json!({"proxy_protocol": "auto"})).unwrap();
        apply_server_patch(&mut config, patch);
        assert_eq!(
            config.public_origin.as_deref(),
            Some("https://kvm.example.com")
        );
        let patch = serde_json::from_value(json!({"public_origin": null})).unwrap();
        apply_server_patch(&mut config, patch);
        assert_eq!(config.public_origin, None);
    }

    #[test]
    fn proxy_trust_list_rejects_unspecified_multicast_and_unbounded_lists() {
        let mut config = Config::default();
        for invalid in ["0.0.0.0", "::", "224.0.0.1", "ff02::1"] {
            config.server.trusted_http_proxies = vec![invalid.parse().unwrap()];
            assert!(validate_config(&config).is_err());
        }
        config.server.trusted_http_proxies = vec!["127.0.0.1".parse().unwrap(); 65];
        assert!(validate_config(&config).is_err());
        config.server.trusted_http_proxies = vec!["127.0.0.1".parse().unwrap(), "192.168.1.2".parse().unwrap()];
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn build_webauthn_validates_domains_and_rejects_ip_addresses() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("localhost:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:8080"));
        let res = build_webauthn(&headers);
        assert!(res.is_ok());
        let (_, rp_id, _) = res.unwrap();
        assert_eq!(rp_id, "localhost");

        // Domain name
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("wingman.local:8443"));
        headers.insert(ORIGIN, HeaderValue::from_static("https://wingman.local:8443"));
        let res = build_webauthn(&headers);
        assert!(res.is_ok());
        let (_, rp_id, _) = res.unwrap();
        assert_eq!(rp_id, "wingman.local");

        // IP address should be rejected with helpful error
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("192.168.1.100:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://192.168.1.100:8080"));
        let res = build_webauthn(&headers);
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("域名"));

        // Host mismatch
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("wingman.local:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://attacker.com"));
        let res = build_webauthn(&headers);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn bootstrap_auth_distinguishes_missing_from_unreadable_store() {
        let root = std::env::temp_dir().join(format!("wingman-bootstrap-{}", generate_token().unwrap()));
        std::fs::create_dir_all(&root).unwrap();
        let missing = AuthStore::new(root.join("auth.json"));
        let (initialized, record) = load_bootstrap_auth(missing).await.unwrap();
        assert!(!initialized);
        assert!(record.is_none());

        // A regular file used as a parent directory makes metadata fail with ENOTDIR.
        std::fs::write(root.join("not-a-dir"), b"x").unwrap();
        let unreadable = AuthStore::new(root.join("not-a-dir").join("auth.json"));
        assert!(load_bootstrap_auth(unreadable).await.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_setup_scans_share_one_probe() {
        let cache = Arc::new(AsyncMutex::new(None));
        let probes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let probes = probes.clone();
            tasks.push(tokio::spawn(async move {
                cached_setup_scan(&cache, || async move {
                    probes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    empty_discovery()
                })
                .await
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(probes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn passkey_challenge_table_is_bounded_and_reclaims_expired_entries() {
        let challenges = Mutex::new(HashMap::new());
        let rp_origin = Url::parse("http://localhost:8080").unwrap();
        let webauthn = WebauthnBuilder::new("localhost", &rp_origin)
            .unwrap()
            .rp_name("WingmanKVM")
            .build()
            .unwrap();
        let (_ccr, reg_state) = webauthn
            .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
            .unwrap();
        let challenge = |expires_at| PasskeyChallengeState::Registration {
            state: reg_state.clone(),
            rp_id: "localhost".to_owned(),
            rp_origin: rp_origin.clone(),
            expires_at,
        };
        let now = Instant::now();
        for index in 0..MAX_PASSKEY_CHALLENGES {
            let expires_at = now + Duration::from_secs(60);
            insert_passkey_challenge(&challenges, index.to_string(), challenge(expires_at), now).unwrap();
        }
        let rejected = insert_passkey_challenge(
            &challenges,
            "overflow".to_owned(),
            challenge(now + Duration::from_secs(60)),
            now,
        )
        .unwrap_err();
        assert_eq!(rejected.status, StatusCode::SERVICE_UNAVAILABLE);

        // Once the outstanding challenges expire, capacity returns.
        let later = now + Duration::from_secs(61);
        insert_passkey_challenge(&challenges, "fresh".to_owned(), challenge(later + Duration::from_secs(60)), later)
            .unwrap();
        assert_eq!(challenges.lock().unwrap().len(), 1);
    }

    #[test]
    fn custom_limiter_budget_applies_per_ip() {
        let limiter = LoginLimiter::with_budget(MAX_PASSKEY_STARTS_PER_IP);
        let ip = IpAddr::from([192, 0, 2, 10]);
        let now = Instant::now();
        for _ in 0..MAX_PASSKEY_STARTS_PER_IP {
            assert!(limiter.try_reserve_at(ip, now));
        }
        assert!(!limiter.try_reserve_at(ip, now));
    }

    #[test]
    fn passkey_challenge_pruning_removes_expired_entries() {
        let challenges = Arc::new(Mutex::new(HashMap::new()));
        let now = Instant::now();
        {
            let mut map = challenges.lock().unwrap();
            let rp_origin = Url::parse("http://localhost:8080").unwrap();
            // Fresh registration
            let builder = WebauthnBuilder::new("localhost", &rp_origin).unwrap();
            let webauthn = builder.rp_name("WingmanKVM").build().unwrap();
            let (_ccr, reg_state) = webauthn
                .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
                .unwrap();
            map.insert(
                "fresh".to_owned(),
                PasskeyChallengeState::Registration {
                    state: reg_state.clone(),
                    rp_id: "localhost".to_owned(),
                    rp_origin: rp_origin.clone(),
                    expires_at: now + Duration::from_secs(60),
                },
            );
            // Expired registration
            map.insert(
                "expired".to_owned(),
                PasskeyChallengeState::Registration {
                    state: reg_state,
                    rp_id: "localhost".to_owned(),
                    rp_origin,
                    expires_at: now - Duration::from_secs(10),
                },
            );
        }

        prune_passkey_challenges(&challenges);
        let map = challenges.lock().unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("fresh"));
        assert!(!map.contains_key("expired"));
    }

    #[test]
    fn passkey_serialization_shape() {
        let rp_origin = Url::parse("http://localhost:8080").unwrap();
        let builder = WebauthnBuilder::new("localhost", &rp_origin).unwrap();
        let webauthn = builder.rp_name("WingmanKVM").build().unwrap();
        let (ccr, _) = webauthn
            .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
            .unwrap();
        let ccr_json = serde_json::to_value(&ccr).unwrap();
        assert!(ccr_json.get("publicKey").is_some());

        let (rcr, _) = webauthn
            .start_passkey_authentication(&[])
            .unwrap();
        let rcr_json = serde_json::to_value(&rcr).unwrap();
        assert!(rcr_json.get("publicKey").is_some());

        // Test RegisterPublicKeyCredential structure
        let reg_dummy = json!({
            "id": "AAECAw",
            "rawId": "AAECAw",
            "response": {
                "clientDataJSON": "AAECAw",
                "attestationObject": "AAECAw"
            },
            "type": "public-key",
            "extensions": {}
        });
        let reg_deser: Result<RegisterPublicKeyCredential, _> = serde_json::from_value(reg_dummy);
        assert!(reg_deser.is_ok());

        // Test PublicKeyCredential structure
        let auth_dummy = json!({
            "id": "AAECAw",
            "rawId": "AAECAw",
            "response": {
                "clientDataJSON": "AAECAw",
                "authenticatorData": "AAECAw",
                "signature": "AAECAw",
                "userHandle": null
            },
            "type": "public-key",
            "extensions": {}
        });
        let auth_deser: Result<PublicKeyCredential, _> = serde_json::from_value(auth_dummy);
        assert!(auth_deser.is_ok());
    }

    #[tokio::test]
    async fn keyboard_leds_serialization_and_endpoint_shape() {
        let hid = HidManager::new();
        hid.set_leds_raw(0x02);
        assert_eq!(hid.leds().caps_lock, true);
        assert_eq!(hid.leds().num_lock, false);
        assert_eq!(hid.leds().scroll_lock, false);

        let leds_json = serde_json::to_value(hid.leds()).unwrap();
        assert_eq!(leds_json["caps_lock"], true);
        assert_eq!(leds_json["num_lock"], false);
        assert_eq!(leds_json["scroll_lock"], false);
        assert_eq!(leds_json["raw"], 2);

        let parsed: KeyboardLeds = serde_json::from_value(leds_json).unwrap();
        assert_eq!(parsed, hid.leds());

        let status_json = json!({
            "hid": {
                "keyboard_leds": hid.leds(),
            }
        });
        assert_eq!(status_json["hid"]["keyboard_leds"]["caps_lock"], true);
    }

    #[tokio::test]
    async fn mjpeg_stream_ends_when_the_session_is_revoked() {
        let sessions = SessionStore::default();
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (_frames_tx, frames) = watch::channel(None);
        let body = mjpeg_body(frames, guard);
        let reader = tokio::spawn(axum::body::to_bytes(body, usize::MAX));
        tokio::task::yield_now().await;
        assert!(sessions.revoke(&token));
        let bytes = tokio::time::timeout(Duration::from_secs(1), reader)
            .await
            .expect("stream must end after revocation")
            .unwrap()
            .unwrap();
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn favicon_and_icon_endpoints_return_svg() {
        let resp = favicon().await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml"
        );
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

    use tokio::io::AsyncReadExt as _;

    struct UploadTestDir(PathBuf);
    impl UploadTestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("wingmankvm-upload-test-{}-{}", std::process::id(), generate_token().unwrap()));
            std::fs::create_dir_all(&path).unwrap(); Self(path)
        }
        fn path(&self) -> &Path { &self.0 }
    }
    impl Drop for UploadTestDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
    fn upload_test_config(dir: &Path, max: u64) -> Config {
        let mut config = Config::default();
        config.media.enabled = true; config.media.lun_path = Some(dir.join("fake-lun.0"));
        config.media.image_directory = Some(dir.to_path_buf()); config.media.max_upload_bytes = max; config
    }
    #[derive(Clone)]
    struct UploadTestState { slot: Arc<AsyncMutex<()>>, config: Arc<RwLock<Config>>, limits: UploadLimits }
    async fn upload_test_handler(State(state): State<UploadTestState>, multipart: Multipart) -> Response {
        match upload_media_with_limits(state.slot, state.config, multipart, state.limits).await {
            Ok(response) => response.into_response(), Err(error) => error.into_response(),
        }
    }
    struct UploadTestServer { address: SocketAddr, task: tokio::task::JoinHandle<()> }
    impl Drop for UploadTestServer { fn drop(&mut self) { self.task.abort(); } }
    async fn spawn_upload_test_server(config: Config, limits: UploadLimits) -> UploadTestServer {
        let app = Router::new().route("/upload", post(upload_test_handler)).with_state(UploadTestState {
            slot: Arc::new(AsyncMutex::new(())), config: Arc::new(RwLock::new(config)), limits,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
        UploadTestServer { address, task }
    }
    const UPLOAD_BOUNDARY: &str = "wingmankvm-upload-test";
    fn upload_body(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut body = format!("--{UPLOAD_BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
        body.extend_from_slice(contents);
        body.extend_from_slice(format!("\r\n--{UPLOAD_BOUNDARY}--\r\n").as_bytes()); body
    }
    async fn open_upload(address: SocketAddr, length: usize) -> tokio::net::TcpStream {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let header = format!("POST /upload HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: multipart/form-data; boundary={UPLOAD_BOUNDARY}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
        stream.write_all(header.as_bytes()).await.unwrap(); stream.flush().await.unwrap(); stream
    }
    async fn read_upload_status<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) -> StatusCode {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut data = Vec::new(); let mut buf = [0_u8; 1024];
        while !data.windows(4).any(|window| window == b"\r\n\r\n") {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            match tokio::time::timeout(remaining, reader.read(&mut buf)).await {
                Ok(Ok(0)) | Err(_) => break, Ok(Ok(n)) => data.extend_from_slice(&buf[..n]), Ok(Err(_)) => break,
            }
        }
        let code = String::from_utf8_lossy(&data).lines().next()
            .and_then(|line| line.split_whitespace().nth(1)).and_then(|code| code.parse().ok()).unwrap_or(500);
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
    fn partials(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir).unwrap().flatten().map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "partial")).collect()
    }
    async fn wait_partial(dir: &Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while partials(dir).is_empty() {
            assert!(Instant::now() < deadline, "upload did not create a partial file");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    async fn complete_upload(server: &UploadTestServer, name: &str, contents: &[u8]) -> StatusCode {
        let body = upload_body(name, contents);
        let mut stream = open_upload(server.address, body.len()).await;
        stream.write_all(&body).await.unwrap(); read_upload_status(&mut stream).await
    }
    fn limits(idle_ms: u64, total_s: u64) -> UploadLimits {
        UploadLimits { idle: Duration::from_millis(idle_ms), total: Duration::from_secs(total_s) }
    }
    #[tokio::test]
    async fn idle_stall_before_and_midfield_times_out_and_cleans() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 1024), limits(120, 5)).await;
        let mut stalled = open_upload(server.address, 512).await;
        assert_eq!(read_upload_status(&mut stalled).await, StatusCode::REQUEST_TIMEOUT);
        let body = upload_body("mid.iso", &[7_u8; 256]);
        let mut mid = open_upload(server.address, body.len()).await;
        mid.write_all(&body[..body.len() / 2]).await.unwrap();
        assert_eq!(read_upload_status(&mut mid).await, StatusCode::REQUEST_TIMEOUT);
        assert!(partials(dir.path()).is_empty());
        assert!(!dir.path().join("mid.iso").exists());
    }
    #[tokio::test]
    async fn progress_resets_idle_but_total_still_bounds() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 4096), limits(250, 5)).await;
        let contents = [3_u8; 128]; let body = upload_body("progress.iso", &contents);
        let mut stream = open_upload(server.address, body.len()).await;
        let prefix_end = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        stream.write_all(&body[..prefix_end]).await.unwrap();
        for chunk in body[prefix_end..].chunks(32) { stream.write_all(chunk).await.unwrap(); tokio::time::sleep(Duration::from_millis(70)).await; }
        assert_eq!(read_upload_status(&mut stream).await, StatusCode::OK);
        assert_eq!(std::fs::read(dir.path().join("progress.iso")).unwrap(), contents);
        let total = spawn_upload_test_server(upload_test_config(dir.path(), 4096), limits(5000, 1)).await;
        let body = upload_body("total.iso", &[5_u8; 256]);
        let stream = open_upload(total.address, body.len()).await;
        let (mut reader, mut writer) = stream.into_split();
        let sender = tokio::spawn(async move {
            for chunk in body.chunks(32) { if writer.write_all(chunk).await.is_err() { break; } tokio::time::sleep(Duration::from_millis(80)).await; }
        });
        assert_eq!(read_upload_status(&mut reader).await, StatusCode::REQUEST_TIMEOUT);
        let _ = sender.await; assert!(!dir.path().join("total.iso").exists());
    }
    #[tokio::test]
    async fn busy_disconnect_size_and_collision_are_safe() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 16), limits(2000, 10)).await;
        let body = upload_body("first.iso", b"first-content");
        let mut first = open_upload(server.address, body.len()).await;
        first.write_all(&body[..body.len() - 8]).await.unwrap(); wait_partial(dir.path()).await;
        let started = Instant::now();
        assert_eq!(complete_upload(&server, "second.iso", b"second").await, StatusCode::CONFLICT);
        assert!(started.elapsed() < Duration::from_secs(1));
        first.write_all(&body[body.len() - 8..]).await.unwrap();
        assert_eq!(read_upload_status(&mut first).await, StatusCode::OK);
        assert_eq!(complete_upload(&server, "large.iso", &[1_u8; 32]).await, StatusCode::PAYLOAD_TOO_LARGE);
        let final_path = dir.path().join("existing.iso"); std::fs::write(&final_path, b"existing").unwrap();
        assert_eq!(complete_upload(&server, "existing.iso", b"replacement").await, StatusCode::CONFLICT);
        assert_eq!(std::fs::read(&final_path).unwrap(), b"existing");
        assert!(partials(dir.path()).is_empty());
    }
    struct FailingUploadWriter;
    impl tokio::io::AsyncWrite for FailingUploadWriter {
        fn poll_write(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>, _buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Err(std::io::Error::other("injected upload write failure")))
        }
        fn poll_flush(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
        fn poll_shutdown(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
    }
    #[tokio::test]
    async fn injected_write_failure_is_reported() {
        let mut writer = FailingUploadWriter; let mut size = 0;
        let error = write_upload_chunk(&mut writer, b"abc", &mut size, 10, Instant::now() + Duration::from_secs(5)).await.unwrap_err();
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR); assert_eq!(size, 0);
    }
    #[tokio::test]
    async fn cancelled_upload_removes_only_its_owned_partial() {
        let dir = UploadTestDir::new();
        let destination = dir.path().join("preserved.iso");
        std::fs::write(&destination, b"valid image").unwrap();
        let partial = PartialUpload::create(dir.path()).unwrap();
        let partial_path = partial.path.clone().unwrap();
        let (ready, running) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _partial = partial;
            let _ = ready.send(());
            std::future::pending::<()>().await;
        });
        running.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!partial_path.exists());
        assert_eq!(std::fs::read(destination).unwrap(), b"valid image");
    }

    #[tokio::test]
    async fn raced_destination_is_never_replaced_during_publish() {
        let dir = UploadTestDir::new();
        let destination = dir.path().join("raced.iso");
        let mut partial = PartialUpload::create(dir.path()).unwrap();
        partial.file_mut().write_all(b"incoming").await.unwrap();
        partial.file_mut().sync_all().await.unwrap();
        std::fs::write(&destination, b"raced valid image").unwrap();
        assert_eq!(partial.publish(&destination).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(destination).unwrap(), b"raced valid image");
        assert!(partials(dir.path()).is_empty());
    }

}
