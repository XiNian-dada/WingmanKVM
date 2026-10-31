mod media;
use self::media::*;
mod config_api;
use self::config_api::*;
mod config_validate;
use self::config_validate::*;
mod setup;
use self::setup::*;
mod terminal;
use self::terminal::*;
mod passkey;
use self::passkey::*;
mod password_sync;
use self::password_sync::*;
mod login;
use self::login::*;
mod error;
use self::error::*;

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
    web_ui::{APP_CSS, APP_JS, ICON_SVG, INDEX_HTML},
};

const SESSION_COOKIE: &str = "wingman_session";
const SESSION_COOKIE_MAX_AGE_SECONDS: u32 = 43_200;
const XTERM_JS: &str = include_str!("../../web/vendor/xterm/xterm.js");
const XTERM_FIT_JS: &str = include_str!("../../web/vendor/xterm/addon-fit.js");
const XTERM_CSS: &str = include_str!("../../web/vendor/xterm/xterm.css");
const GPIO_TEST_PULSE_MS: u64 = 150;
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
        .route("/assets/app.css", get(app_css))
        .route("/assets/app.js", get(app_js))
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

// App assets change with every release; never let a browser pair a fresh
// index with a stale script or stylesheet.
async fn app_css() -> impl IntoResponse {
    no_store(([(CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS))
}

async fn app_js() -> impl IntoResponse {
    no_store(([(CONTENT_TYPE, "text/javascript; charset=utf-8")], APP_JS.as_str()))
}

async fn xterm_css() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], XTERM_CSS)
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

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    #[cfg(unix)]
    use std::sync::atomic::AtomicU64;

    #[cfg(unix)]
    pub(crate) static TEST_PATH_ID: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn candidate(path: &str, kind: &str) -> discovery::DeviceCandidate {
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

    pub(crate) fn ready_gadget_candidate(path: &str, kind: &str) -> discovery::DeviceCandidate {
        let mut candidate = candidate(path, kind);
        candidate.compatible = Some(true);
        candidate.function_linked = Some(true);
        candidate.gadget_bound = Some(true);
        candidate
    }

    pub(crate) fn empty_discovery() -> discovery::DeviceDiscovery {
        discovery::DeviceDiscovery {
            video: Vec::new(),
            hid: Vec::new(),
            keyboard: Vec::new(),
            mouse: Vec::new(),
            absolute_pointer: Vec::new(),
            gpio: Vec::new(),
            mass_storage_luns: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::hid::KeyboardLeds;

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
}
