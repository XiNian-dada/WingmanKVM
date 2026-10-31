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
