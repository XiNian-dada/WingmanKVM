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
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use tokio::{fs, io::AsyncWriteExt};
use webauthn_rs::prelude::*;

use crate::{
    auth::{AuthError, AuthRecord, AuthStore, PasswordPolicyError, SessionStore},
    config::{
        self, CONFIG_VERSION, Config, GpioBias, GpioInputConfig, GpioPulseConfig, H264Encoder,
        HidConfig, PointerMode, VideoConfig, VideoEncoding, VirtualMonitorMode,
    },
    devices::{
        discovery,
        display::{DisplayError, DisplayManager, DisplayStatus},
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
    web_ui::INDEX_HTML,
};

const SESSION_COOKIE: &str = "wingman_session";
const XTERM_JS: &str = include_str!("../web/vendor/xterm/xterm.js");
const XTERM_FIT_JS: &str = include_str!("../web/vendor/xterm/addon-fit.js");
const XTERM_CSS: &str = include_str!("../web/vendor/xterm/xterm.css");
const GPIO_TEST_PULSE_MS: u64 = 150;
const VIDEO_PAUSE_TIMEOUT: Duration = Duration::from_secs(5);
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
    config_path: Arc<PathBuf>,
    auth: AuthStore,
    sessions: SessionStore,
    setup: Arc<AsyncMutex<()>>,
    config_update: Arc<AsyncMutex<()>>,
    setup_token: Arc<Mutex<Option<String>>>,
    login_limiter: LoginLimiter,
    passkey_challenges: Arc<Mutex<HashMap<String, PasskeyChallengeState>>>,
    hid: HidManager,
    power: PowerManager,
    media: Arc<MediaManager>,
    media_upload: Arc<AsyncMutex<()>>,
    display: DisplayManager,
    video: VideoManager,
    webrtc: WebRtcManager,
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
        let display = DisplayManager::new();
        if config.display.virtual_monitor != VirtualMonitorMode::Unmanaged
            && let Err(error) = display.apply(&config.display, config.video.device.as_deref())
        {
            tracing::error!(%error, "failed to restore configured virtual monitor at startup");
        }
        let video = VideoManager::new(effective_video_config(
            &config,
            display.status().applied_mode,
        ));
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

        let state = Self {
            config: Arc::new(RwLock::new(config)),
            config_path: Arc::new(config_path),
            auth,
            sessions,
            setup: Arc::new(AsyncMutex::new(())),
            config_update: Arc::new(AsyncMutex::new(())),
            setup_token: Arc::new(Mutex::new(setup_token)),
            login_limiter: LoginLimiter::default(),
            passkey_challenges,
            hid: HidManager::new(),
            power,
            media: Arc::new(MediaManager::default()),
            media_upload: Arc::new(AsyncMutex::new(())),
            display,
            video,
            webrtc,
        };
        state.start_display_monitor();
        Ok(state)
    }

    pub async fn server_address(&self) -> anyhow::Result<SocketAddr> {
        let config = self.config.read().await;
        Ok(format!("{}:{}", config.server.listen_address, config.server.port).parse()?)
    }

    fn start_display_monitor(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // `interval` ticks immediately once. Startup restoration already ran
            // before V4L2 was opened, so only monitor later USB generations.
            interval.tick().await;
            loop {
                interval.tick().await;
                let config = state.config.read().await.clone();
                if !state
                    .display
                    .needs_reapply(&config.display, config.video.device.as_deref())
                {
                    continue;
                }
                let _update_guard = state.config_update.lock().await;
                let config = state.config.read().await.clone();
                if !state
                    .display
                    .needs_reapply(&config.display, config.video.device.as_deref())
                {
                    continue;
                }
                tracing::warn!("MS2130 re-enumerated; restoring volatile EDID RAM");
                if let Err(error) = reapply_display_after_usb_reset(&state, &config).await {
                    tracing::error!(message = %error.message, "failed to restore EDID after USB reset");
                }
            }
        });
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
    has_passkeys: bool,
    config: Option<Config>,
    display: Option<DisplayStatus>,
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

async fn bootstrap(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let setup_required = !state.auth.is_initialized().unwrap_or(false);
    let authenticated = !setup_required && is_authenticated(&state, &headers);
    let has_passkeys = state
        .auth
        .load()
        .ok()
        .flatten()
        .is_some_and(|record| !record.passkeys.is_empty());
    let (config, display, video, webrtc, capabilities) = if authenticated {
        let config = state.config.read().await.clone();
        let webrtc = state.webrtc.status();
        let capabilities = capabilities_with_webrtc(&config, webrtc.available);
        (
            Some(config),
            Some(state.display.status()),
            Some(state.video.status()),
            Some(webrtc),
            Some(capabilities),
        )
    } else {
        (None, None, None, None, None)
    };
    no_store(Json(BootstrapResponse {
        setup_required,
        token_required: setup_required,
        authenticated,
        has_passkeys,
        config,
        display,
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
    let password_for_verification = password.clone();
    let valid = tokio::task::spawn_blocking(move || {
        let Some(record) = auth.load()? else {
            return Ok(false);
        };
        record.verify_credentials(&username, &password_for_verification)
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

    // Older deployments predate password synchronization during setup. A
    // successful administrator login is the only safe point at which the
    // plaintext password is available again, so repair those installations
    // before issuing a terminal-capable session.
    sync_system_password(&password).await?;
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !state.login_limiter.allowed(peer.ip()) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "登录尝试过多，请稍后再试",
        ));
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
    {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.retain(|_, s| match s {
            PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
            PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
        });
        challenges.insert(
            challenge_id.clone(),
            PasskeyChallengeState::Authentication {
                state: auth_state,
                rp_id,
                rp_origin,
                expires_at,
            },
        );
    }

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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<PasskeyLoginFinishRequest>,
) -> Result<Response, ApiError> {
    if !origin_matches_host(&headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }
    if !state.login_limiter.allowed(peer.ip()) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "登录尝试过多，请稍后再试",
        ));
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
        state.login_limiter.record_failure(peer.ip());
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无效或已过期的认证挑战"));
    };

    if Instant::now() > expires_at {
        state.login_limiter.record_failure(peer.ip());
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
            state.login_limiter.record_failure(peer.ip());
            return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Passkey 验证失败"));
        }
    };

    let cred_id = request.credential.raw_id;
    let username = state.auth.update(|record| {
        record.update_passkey_credential(cred_id.as_slice(), &auth_result);
        Ok(record.username.clone())
    }).map_err(map_auth_error)?;

    state.login_limiter.clear(peer.ip());
    let session = state.sessions.create().map_err(ApiError::internal)?;
    Ok(with_session_cookie(
        Json(json!({"ok": true, "username": username})).into_response(),
        &session,
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
    {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.retain(|_, s| match s {
            PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
            PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
        });
        challenges.insert(
            challenge_id.clone(),
            PasskeyChallengeState::Registration {
                state: reg_state,
                rp_id,
                rp_origin,
                expires_at,
            },
        );
    }

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

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await.clone();
    let webrtc = state.webrtc.status();
    no_store(Json(json!({
        "display": state.display.status(),
        "video": state.video.status(),
        "power": state.power.status().await,
        "webrtc": webrtc,
        "capabilities": capabilities_with_webrtc(&config, webrtc.available),
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
    let _update_guard = state.config_update.lock().await;
    validate_config(&config)?;
    commit_config(&state, &config).await?;
    Ok(no_store(Json(config)))
}

#[derive(Default, Deserialize)]
struct ConfigPatch {
    display: Option<DisplayPatch>,
    video: Option<VideoPatch>,
    hid: Option<HidPatch>,
    power: Option<PowerPatch>,
    media: Option<MediaPatch>,
}

#[derive(Deserialize)]
struct DisplayPatch {
    virtual_monitor: Option<VirtualMonitorMode>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    control_device: Option<Option<PathBuf>>,
}

#[derive(Deserialize)]
struct VideoPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    device: Option<Option<PathBuf>>,
    follow_display: Option<bool>,
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
    if let Some(display) = patch.display {
        apply_display_patch(&mut config.display, display);
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

fn apply_display_patch(config: &mut config::DisplayConfig, patch: DisplayPatch) {
    if let Some(virtual_monitor) = patch.virtual_monitor {
        config.virtual_monitor = virtual_monitor;
    }
    if let Some(control_device) = patch.control_device {
        config.control_device = control_device;
    }
}

fn apply_video_patch(config: &mut config::VideoConfig, patch: VideoPatch) {
    if let Some(device) = patch.device {
        config.device = device;
    }
    if let Some(follow_display) = patch.follow_display {
        config.follow_display = follow_display;
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

async fn webrtc_offer(
    State(state): State<AppState>,
    Json(request): Json<WebRtcOfferRequest>,
) -> Result<Response, ApiError> {
    let response = state
        .webrtc
        .offer(request)
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
    let previous = state.config.read().await.clone();
    let current_display = state.display.status();
    let apply_display = config.display.virtual_monitor != VirtualMonitorMode::Unmanaged
        && (config.display != previous.display
            || config.video.device != previous.video.device
            || current_display.applied_mode != Some(config.display.virtual_monitor));

    let rollback = if apply_display {
        pause_video(&state.video).await?;
        let display = state.display.clone();
        let display_config = config.display.clone();
        let video_device = config.video.device.clone();
        let apply_result = tokio::task::spawn_blocking(move || {
            display.apply(&display_config, video_device.as_deref())
        })
        .await;
        match apply_result {
            Ok(Ok(rollback)) => rollback,
            Ok(Err(error)) => {
                resume_video(
                    &state.video,
                    effective_video_config(&previous, current_display.applied_mode),
                )?;
                return Err(map_display_error(error));
            }
            Err(error) => {
                state
                    .display
                    .mark_unapplied("EDID 切换任务异常退出，已恢复旧采集配置");
                resume_video(
                    &state.video,
                    effective_video_config(&previous, current_display.applied_mode),
                )?;
                return Err(ApiError::internal(error));
            }
        }
    } else {
        None
    };

    if let Err(error) = persist_config(state.config_path.as_ref(), config).await {
        if let Some(rollback) = rollback {
            rollback_display(state, rollback, previous.display.virtual_monitor).await;
        }
        if apply_display {
            let _ = resume_video(
                &state.video,
                effective_video_config(&previous, current_display.applied_mode),
            );
        }
        return Err(error);
    }

    if config.display.virtual_monitor == VirtualMonitorMode::Unmanaged {
        state.display.set_unmanaged();
    }
    let video = effective_video_config(config, state.display.status().applied_mode);
    if let Err(error) = state.video.reconfigure(video) {
        tracing::error!(%error, "failed to resume video after saving configuration");
        if let Some(rollback) = rollback {
            rollback_display(state, rollback, previous.display.virtual_monitor).await;
        }
        if let Err(restore_error) = persist_config(state.config_path.as_ref(), &previous).await {
            tracing::error!(message = %restore_error.message, "failed to restore previous configuration");
        }
        return Err(ApiError::internal(error));
    }

    state.webrtc.reconfigure(config.video.h264.clone()).await;
    let power_led = power_led_snapshot(&config.power);
    *state.config.write().await = config.clone();
    state.power.set_power_led_config(power_led).await;
    Ok(())
}

async fn pause_video(video: &VideoManager) -> Result<(), ApiError> {
    let video = video.clone();
    tokio::task::spawn_blocking(move || video.pause(VIDEO_PAUSE_TIMEOUT))
        .await
        .map_err(ApiError::internal)?
        .map_err(|error| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, error))
}

fn resume_video(video: &VideoManager, config: VideoConfig) -> Result<(), ApiError> {
    video.reconfigure(config).map_err(ApiError::internal)
}

async fn rollback_display(
    state: &AppState,
    rollback: crate::devices::display::DisplayRollback,
    previous_mode: VirtualMonitorMode,
) {
    let display = state.display.clone();
    match tokio::task::spawn_blocking(move || display.rollback(rollback, previous_mode)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::error!(%error, "failed to roll back MS2130 EDID"),
        Err(error) => tracing::error!(%error, "MS2130 EDID rollback task failed"),
    }
}

async fn reapply_display_after_usb_reset(
    state: &AppState,
    config: &Config,
) -> Result<(), ApiError> {
    pause_video(&state.video).await?;
    let display = state.display.clone();
    let display_config = config.display.clone();
    let video_device = config.video.device.clone();
    let task_result = tokio::task::spawn_blocking(move || {
        display.apply(&display_config, video_device.as_deref())
    })
    .await;
    let result = match task_result {
        Ok(result) => result,
        Err(error) => {
            state
                .display
                .mark_unapplied("采集卡重新枚举后的 EDID 恢复任务异常退出");
            let _ = resume_video(&state.video, effective_video_config(config, None));
            return Err(ApiError::internal(error));
        }
    };
    if let Err(error) = result {
        let message = format!("采集卡重新枚举后 EDID 恢复失败: {error}");
        state.display.mark_unapplied(message.clone());
        let _ = resume_video(&state.video, effective_video_config(config, None));
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, message));
    }
    resume_video(
        &state.video,
        effective_video_config(config, state.display.status().applied_mode),
    )
}

fn effective_video_config(
    config: &Config,
    applied_mode: Option<VirtualMonitorMode>,
) -> VideoConfig {
    let mut video = config.video.clone();
    if video.follow_display {
        if applied_mode == Some(config.display.virtual_monitor)
            && let Some((width, height, frames_per_second)) =
                config.display.virtual_monitor.timing()
        {
            video.width = Some(width);
            video.height = Some(height);
            video.frames_per_second = Some(frames_per_second);
        } else {
            // Never pretend capture is native when the EDID was not confirmed.
            video.width = None;
            video.height = None;
            video.frames_per_second = None;
        }
    }
    video
}

fn map_display_error(error: DisplayError) -> ApiError {
    let status = match &error {
        DisplayError::VideoDeviceRequired
        | DisplayError::DeviceMismatch { .. }
        | DisplayError::DescriptorMismatch { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        #[cfg(not(target_os = "linux"))]
        DisplayError::Unsupported => StatusCode::SERVICE_UNAVAILABLE,
        DisplayError::ControlDeviceNotFound
        | DisplayError::AmbiguousControlDevice
        | DisplayError::Io { .. }
        | DisplayError::Protocol(_) => StatusCode::SERVICE_UNAVAILABLE,
    };
    ApiError::new(status, error.to_string())
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
    if !(256..=50_000).contains(&config.video.h264.bitrate_kbps) {
        return Err(ApiError::bad_request(
            "H.264 码率必须在 256 到 50000 Kbps 之间",
        ));
    }
    if !(1..=4).contains(&config.video.h264.max_sessions) {
        return Err(ApiError::bad_request("H.264 同时会话数必须在 1 到 4 之间"));
    }
    if config.video.follow_display
        && config.display.virtual_monitor == VirtualMonitorMode::Unmanaged
    {
        return Err(ApiError::bad_request(
            "原生采集需要先选择一个受管的虚拟显示器模式",
        ));
    }
    if config.video.follow_display
        && (config.video.width.is_some() || config.video.height.is_some())
    {
        return Err(ApiError::bad_request(
            "原生采集不能同时指定固定采集宽度或高度",
        ));
    }
    for path in [
        config.display.control_device.as_ref(),
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
        config.video.follow_display = true;
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.display.virtual_monitor = VirtualMonitorMode::Hd720p60;
        config.video.follow_display = true;
        config.video.width = Some(1280);
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn native_capture_uses_only_a_confirmed_display_mode() {
        let mut config = Config::default();
        config.display.virtual_monitor = VirtualMonitorMode::Hd720p60;
        config.video.follow_display = true;

        let pending = effective_video_config(&config, None);
        assert_eq!(
            (pending.width, pending.height, pending.frames_per_second),
            (None, None, None)
        );

        let applied = effective_video_config(&config, Some(VirtualMonitorMode::Hd720p60));
        assert_eq!(
            (applied.width, applied.height, applied.frames_per_second),
            (Some(1280), Some(720), Some(60))
        );
    }

    #[test]
    fn display_patch_preserves_omitted_fields_and_accepts_null() {
        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "display": {"virtual_monitor": "hd720p60"}
        }))
        .unwrap();
        let mut display = config::DisplayConfig {
            control_device: Some(PathBuf::from("/dev/hidraw0")),
            ..config::DisplayConfig::default()
        };
        apply_display_patch(&mut display, patch.display.unwrap());
        assert_eq!(display.virtual_monitor, VirtualMonitorMode::Hd720p60);
        assert_eq!(display.control_device, Some(PathBuf::from("/dev/hidraw0")));

        let patch: ConfigPatch = serde_json::from_value(serde_json::json!({
            "display": {"control_device": null}
        }))
        .unwrap();
        apply_display_patch(&mut display, patch.display.unwrap());
        assert_eq!(display.control_device, None);
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
}
