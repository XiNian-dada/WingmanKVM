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
    extract::{ConnectInfo, DefaultBodyLimit, Form, Multipart, Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, LOCATION, ORIGIN, SET_COOKIE},
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use tokio::{fs, io::AsyncWriteExt};

use crate::{
    auth::{AuthError, AuthRecord, AuthStore, PasswordPolicyError, SessionStore},
    config::{self, CONFIG_VERSION, Config, HidConfig, PointerMode, VideoEncoding},
    devices::{
        discovery,
        hid::{
            AbsolutePointerRequest, HidError, HidManager, KeyRequest, MouseClickRequest,
            MouseMoveRequest, MouseScrollRequest,
        },
        media::{MediaConfigSnapshot, MediaError, MediaManager, MediaType, sanitize_upload_name},
        power::{PowerConfigSnapshot, PowerError, PowerManager, PowerPress},
        video::VideoManager,
    },
    web_ui::INDEX_HTML,
};

const SESSION_COOKIE: &str = "wingman_session";
const XTERM_JS: &str = include_str!("../web/vendor/xterm/xterm.js");
const XTERM_FIT_JS: &str = include_str!("../web/vendor/xterm/addon-fit.js");
const XTERM_CSS: &str = include_str!("../web/vendor/xterm/xterm.css");

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<Config>>,
    config_path: Arc<PathBuf>,
    auth: AuthStore,
    sessions: SessionStore,
    setup: Arc<AsyncMutex<()>>,
    setup_token: Arc<Mutex<Option<String>>>,
    login_limiter: LoginLimiter,
    hid: HidManager,
    power: PowerManager,
    media: Arc<MediaManager>,
    media_upload: Arc<AsyncMutex<()>>,
    video: VideoManager,
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
        let sessions = SessionStore::default();
        let session_cleanup = sessions.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10 * 60));
            loop {
                interval.tick().await;
                session_cleanup.prune_expired();
            }
        });

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path: Arc::new(config_path),
            auth,
            sessions,
            setup: Arc::new(AsyncMutex::new(())),
            setup_token: Arc::new(Mutex::new(setup_token)),
            login_limiter: LoginLimiter::default(),
            hid: HidManager::new(),
            power: PowerManager::new(),
            media: Arc::new(MediaManager::default()),
            media_upload: Arc::new(AsyncMutex::new(())),
            video,
        })
    }

    pub async fn server_address(&self) -> anyhow::Result<SocketAddr> {
        let config = self.config.read().await;
        Ok(format!("{}:{}", config.server.listen_address, config.server.port).parse()?)
    }
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/video_feed", get(video_feed))
        .route("/api/status", get(status))
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
        .route("/api/terminal/ws", get(terminal_ws))
        .route("/power", post(power))
        .route("/api/media", get(list_media))
        .route(
            "/api/media/upload",
            post(upload_media).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/media/attach", post(attach_media))
        .route("/api/media/detach", post(detach_media))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/", get(index))
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
/// short-lived and is torn down as soon as the browser disconnects.
async fn terminal_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_terminal)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalClientMessage {
    Resize { cols: u16, rows: u16 },
}

async fn handle_terminal(mut socket: WebSocket) {
    let pty = native_pty_system();
    let pair = match pty.openpty(PtySize {
        rows: 32,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(err) => {
            let _ = socket
                .send(Message::Text(format!("\r\nPTY error: {err}\r\n").into()))
                .await;
            return;
        }
    };
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
    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(err) => {
            let _ = socket
                .send(Message::Text(format!("\r\nspawn error: {err}\r\n").into()))
                .await;
            return;
        }
    };
    drop(pair.slave);
    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(_) => return,
    };
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);
    tokio::task::spawn_blocking(move || {
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
    loop {
        tokio::select! {
            Some(data) = rx.recv() => { if socket.send(Message::Binary(data.into())).await.is_err() { break; } }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(TerminalClientMessage::Resize { cols, rows }) =
                        serde_json::from_str(text.as_str())
                        && (2..=1000).contains(&cols)
                        && (1..=500).contains(&rows)
                    {
                        let _ = pair.master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                }
                Some(Ok(Message::Binary(data))) => {
                    if std::io::Write::write_all(&mut writer, &data).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            }
        }
    }
    let _ = child.kill();
}

async fn index() -> impl IntoResponse {
    no_store(Html(INDEX_HTML))
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "wingmankvm"}))
}

#[derive(Serialize)]
struct BootstrapResponse {
    setup_required: bool,
    token_required: bool,
    authenticated: bool,
    config: Option<Config>,
    video: Option<crate::devices::video::VideoStatus>,
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
    mass_storage: bool,
    video_passthrough: bool,
    video_transcode: bool,
}

async fn bootstrap(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let setup_required = !state.auth.is_initialized().unwrap_or(false);
    let authenticated = !setup_required && is_authenticated(&state, &headers);
    let (config, video, capabilities) = if authenticated {
        let config = state.config.read().await.clone();
        let capabilities = capabilities(&config);
        (Some(config), Some(state.video.status()), Some(capabilities))
    } else {
        (None, None, None)
    };
    no_store(Json(BootstrapResponse {
        setup_required,
        token_required: setup_required,
        authenticated,
        config,
        video,
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
    let _setup_guard = state.setup.lock().await;
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
    let mut new_config = Config::default();
    new_config.video.device =
        resolve_setup_video(optional_path(request.video_device), &discovered.video)?;
    new_config.video.auto_detect = new_config.video.device.is_none();
    new_config.hid.keyboard_device = resolve_setup_hid(
        optional_path(request.keyboard_device),
        SetupHidRole::Keyboard,
        &discovered,
    )?;
    new_config.hid.mouse_device = resolve_setup_hid(
        optional_path(request.mouse_device),
        SetupHidRole::Mouse,
        &discovered,
    )?;
    new_config.hid.absolute_pointer_device = resolve_setup_hid(
        optional_path(request.absolute_pointer_device),
        SetupHidRole::AbsolutePointer,
        &discovered,
    )?;
    new_config.hid.pointer_mode = request.pointer_mode.unwrap_or_default();
    new_config.hid.auto_detect = new_config.hid.keyboard_device.is_none()
        || (new_config.hid.mouse_device.is_none()
            && new_config.hid.absolute_pointer_device.is_none());
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

    persist_config(state.config_path.as_ref(), &new_config).await?;

    // Keep the local maintenance account in sync with the web administrator.
    // The account is deliberately fixed to `wingman`; never interpolate a
    // user-supplied name into a shell command.
    sync_system_password(&password).await?;

    let auth = state.auth.clone();
    tokio::task::spawn_blocking(move || auth.initialize_record(auth_record))
        .await
        .map_err(ApiError::internal)?
        .map_err(map_auth_error)?;

    *state.config.write().await = new_config.clone();
    state
        .video
        .reconfigure(new_config.video)
        .map_err(ApiError::internal)?;
    state
        .setup_token
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let session = state.sessions.create().map_err(ApiError::internal)?;
    Ok(with_session_cookie(
        Json(json!({"ok": true})).into_response(),
        &session,
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

async fn sync_system_password(password: &str) -> Result<(), ApiError> {
    if password.is_empty() || password.bytes().any(|b| b == b'\n' || b == b'\r' || b == 0) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "密码格式不正确"));
    }
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut child = std::process::Command::new("sudo")
            .args(["-n", "/usr/local/sbin/wingmankvm-set-wingman-password"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        use std::io::Write;
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(format!("{password}\n").as_bytes())?;
        if !child.wait()?.success() {
            anyhow::bail!("无法更新 wingman 密码");
        }
        Ok(())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    if !state.login_limiter.allowed(peer.ip()) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "登录尝试过多，请稍后再试",
        ));
    }
    let auth = state.auth.clone();
    let username = request.username;
    let password = request.password;
    let valid = tokio::task::spawn_blocking(move || {
        let Some(record) = auth.load()? else {
            return Ok(false);
        };
        record.verify_credentials(&username, &password)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(map_auth_error)?;

    if !valid {
        state.login_limiter.record_failure(peer.ip());
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "用户名或密码不正确",
        ));
    }
    state.login_limiter.clear(peer.ip());
    let session = state.sessions.create().map_err(ApiError::internal)?;
    Ok(with_session_cookie(
        Json(json!({"ok": true})).into_response(),
        &session,
    ))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        state.sessions.revoke(token);
    }
    let mut response = Json(json!({"ok": true})).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_static("wingman_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
    );
    response
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await.clone();
    no_store(Json(json!({
        "video": state.video.status(),
        "power": state.power.status().await,
        "capabilities": capabilities(&config),
    })))
}

async fn scan_devices() -> impl IntoResponse {
    no_store(Json(discovery::scan().await))
}

async fn scan_setup_devices(State(state): State<AppState>) -> Result<Response, ApiError> {
    if state.auth.is_initialized().map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    }
    Ok(no_store(Json(discovery::scan().await)))
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.config.read().await.clone()))
}

async fn update_config(
    State(state): State<AppState>,
    Json(config): Json<Config>,
) -> Result<impl IntoResponse, ApiError> {
    validate_config(&config)?;
    persist_config(state.config_path.as_ref(), &config).await?;
    state
        .video
        .reconfigure(config.video.clone())
        .map_err(ApiError::internal)?;
    *state.config.write().await = config.clone();
    Ok(no_store(Json(config)))
}

#[derive(Default, Deserialize)]
struct ConfigPatch {
    video: Option<VideoPatch>,
    hid: Option<HidPatch>,
    power: Option<PowerPatch>,
    media: Option<MediaPatch>,
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
    let mut config = state.config.read().await.clone();
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
    persist_config(state.config_path.as_ref(), &config).await?;
    state
        .video
        .reconfigure(config.video.clone())
        .map_err(ApiError::internal)?;
    *state.config.write().await = config.clone();
    Ok(no_store(Json(config)))
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
    config.auto_detect = config.keyboard_device.is_none()
        || (config.mouse_device.is_none() && config.absolute_pointer_device.is_none());
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
    Ok(StatusCode::NO_CONTENT)
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
    let (chip, line) = match (config.enabled, config.gpio_chip, config.gpio_line) {
        (true, Some(chip), Some(line)) => (chip, line),
        _ => {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "GPIO 电源控制尚未配置",
            ));
        }
    };
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

async fn list_media(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let snapshot = media_snapshot(&state).await;
    let (images, status) = state.media.list(snapshot).await.map_err(map_media_error)?;
    Ok(no_store(Json(json!({"images": images, "status": status}))))
}

async fn upload_media(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let _upload_guard = state.media_upload.lock().await;
    let config = state.config.read().await.media.clone();
    if !config.enabled {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "虚拟介质尚未配置",
        ));
    }
    let image_dir = config
        .image_directory
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "镜像目录尚未配置"))?;
    fs::create_dir_all(&image_dir)
        .await
        .map_err(ApiError::internal)?;

    let mut uploaded = None;
    while let Some(mut field) = multipart.next_field().await.map_err(ApiError::internal)? {
        if !matches!(field.name(), Some("image" | "file")) {
            continue;
        }
        let original = field
            .file_name()
            .ok_or_else(|| ApiError::bad_request("上传内容缺少文件名"))?
            .to_string();
        let name = sanitize_upload_name(&original).map_err(map_media_error)?;
        let final_path = image_dir.join(&name);
        if fs::try_exists(&final_path)
            .await
            .map_err(ApiError::internal)?
        {
            return Err(ApiError::new(StatusCode::CONFLICT, "同名镜像已经存在"));
        }
        let partial = image_dir.join(format!(
            ".{name}.{}.partial",
            generate_token().map_err(ApiError::internal)?
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .await
            .map_err(ApiError::internal)?;
        let mut size = 0_u64;
        let result: Result<(), ApiError> = async {
            while let Some(chunk) = field.chunk().await.map_err(ApiError::internal)? {
                size = size.saturating_add(chunk.len() as u64);
                if size > config.max_upload_bytes {
                    return Err(ApiError::new(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "镜像超过配置的大小上限",
                    ));
                }
                file.write_all(&chunk).await.map_err(ApiError::internal)?;
            }
            file.sync_all().await.map_err(ApiError::internal)?;
            drop(file);
            fs::rename(&partial, &final_path)
                .await
                .map_err(ApiError::internal)?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = fs::remove_file(&partial).await;
        }
        result?;
        uploaded = Some(json!({"name": name, "size": size}));
        break;
    }
    uploaded
        .map(Json)
        .ok_or_else(|| ApiError::bad_request("表单中没有 image 文件字段"))
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

async fn video_feed(State(state): State<AppState>) -> Response {
    let mut frames = state.video.subscribe();
    let stream = async_stream::stream! {
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
            if frames.changed().await.is_err() {
                break;
            }
        }
    };
    let mut response = Body::from_stream(stream).into_response();
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

fn capabilities(config: &Config) -> Capabilities {
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
        mass_storage: config.media.enabled
            && config.media.lun_path.is_some()
            && config.media.image_directory.is_some(),
        video_passthrough: true,
        video_transcode: true,
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

async fn persist_config(path: &Path, config: &Config) -> Result<(), ApiError> {
    let path = path.to_path_buf();
    let config = config.clone();
    tokio::task::spawn_blocking(move || config.save_atomic(path))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

fn validate_config(config: &Config) -> Result<(), ApiError> {
    if config.version != CONFIG_VERSION {
        return Err(ApiError::bad_request("配置版本不受支持"));
    }
    if let (Some(width), Some(height)) = (config.video.width, config.video.height)
        && (!(160..=7680).contains(&width) || !(120..=4320).contains(&height))
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
    for path in [
        config.video.device.as_ref(),
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
    {
        return Err(ApiError::bad_request("绝对指针模式需要配置绝对指针设备"));
    }
    if config.hid.pointer_mode == PointerMode::Relative && config.hid.mouse_device.is_none() {
        return Err(ApiError::bad_request("相对指针模式需要配置相对鼠标设备"));
    }
    if config.power.enabled
        && (config.power.gpio_chip.is_none() || config.power.gpio_line.is_none())
    {
        return Err(ApiError::bad_request(
            "启用电源控制前必须配置 GPIO 芯片和线路",
        ));
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

fn with_session_cookie(mut response: Response, token: &str) -> Response {
    let cookie =
        format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
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
        PowerError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
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

#[derive(Clone, Default)]
struct LoginLimiter {
    failures: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
}

impl LoginLimiter {
    fn allowed(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let attempts = failures.entry(ip).or_default();
        attempts.retain(|attempt| now.duration_since(*attempt) < Duration::from_secs(60));
        attempts.len() < 5
    }

    fn record_failure(&self, ip: IpAddr) {
        self.failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(ip)
            .or_default()
            .push_back(Instant::now());
    }

    fn clear(&self, ip: IpAddr) {
        self.failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("dev/hidg0"));
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Absolute;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.pointer_mode = PointerMode::Relative;
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
    fn video_and_power_patches_preserve_omitted_fields_and_accept_null() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "video": { "jpeg_quality": 72 },
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
}
