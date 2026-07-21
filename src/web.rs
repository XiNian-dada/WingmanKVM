use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Body,
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
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{fs, io::AsyncWriteExt, sync::RwLock};

use crate::{
    auth::{AuthError, AuthStore, SessionStore},
    config::{self, CONFIG_VERSION, Config, VideoEncoding},
    devices::{
        discovery,
        hid::{
            HidError, HidManager, KeyRequest, MouseClickRequest, MouseMoveRequest,
            MouseScrollRequest,
        },
        media::{MediaConfigSnapshot, MediaError, MediaManager, sanitize_upload_name},
        power::{PowerConfigSnapshot, PowerError, PowerManager, PowerPress},
        video::VideoManager,
    },
    web_ui::INDEX_HTML,
};

const SESSION_COOKIE: &str = "wingman_session";

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<Config>>,
    config_path: Arc<PathBuf>,
    auth: AuthStore,
    sessions: SessionStore,
    setup_token: Arc<Mutex<Option<String>>>,
    login_limiter: LoginLimiter,
    hid: HidManager,
    power: PowerManager,
    media: Arc<MediaManager>,
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
            setup_token: Arc::new(Mutex::new(setup_token)),
            login_limiter: LoginLimiter::default(),
            hid: HidManager::new(),
            power: PowerManager::new(),
            media: Arc::new(MediaManager::default()),
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
        .route("/api/mouse/click", post(mouse_click))
        .route("/api/mouse/scroll", post(mouse_scroll))
        .route("/api/input/release-all", post(release_all))
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
    gpio_chip: Option<String>,
    #[serde(default)]
    gpio_line: Option<u32>,
    #[serde(default)]
    lun_path: Option<String>,
    #[serde(default)]
    image_directory: Option<String>,
}

async fn setup(
    State(state): State<AppState>,
    Json(request): Json<SetupRequest>,
) -> Result<Response, ApiError> {
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

    let mut new_config = Config::default();
    new_config.video.device = optional_path(request.video_device);
    new_config.video.auto_detect = new_config.video.device.is_none();
    new_config.hid.keyboard_device = optional_path(request.keyboard_device);
    new_config.hid.mouse_device = optional_path(request.mouse_device);
    new_config.hid.auto_detect =
        new_config.hid.keyboard_device.is_none() || new_config.hid.mouse_device.is_none();
    new_config.power.gpio_chip = optional_string(request.gpio_chip);
    new_config.power.gpio_line = request.gpio_line;
    new_config.power.enabled =
        new_config.power.gpio_chip.is_some() && new_config.power.gpio_line.is_some();
    new_config.media.lun_path = optional_path(request.lun_path);
    new_config.media.image_directory =
        optional_path(request.image_directory).or_else(|| Some(config::state_dir().join("images")));
    new_config.media.enabled = new_config.media.lun_path.is_some();
    validate_config(&new_config)?;

    persist_config(state.config_path.as_ref(), &new_config).await?;
    let auth = state.auth.clone();
    let username = request.username;
    let password = request.password;
    tokio::task::spawn_blocking(move || auth.initialize(username, &password))
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
    device: Option<PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    frames_per_second: Option<u32>,
    encoding: Option<VideoEncoding>,
    jpeg_quality: Option<u8>,
}

#[derive(Deserialize)]
struct HidPatch {
    keyboard_device: Option<PathBuf>,
    mouse_device: Option<PathBuf>,
}

#[derive(Deserialize)]
struct PowerPatch {
    enabled: Option<bool>,
    gpio_chip: Option<String>,
    gpio_line: Option<u32>,
    active_high: Option<bool>,
}

#[derive(Deserialize)]
struct MediaPatch {
    enabled: Option<bool>,
    lun_path: Option<PathBuf>,
    image_directory: Option<PathBuf>,
    read_only_by_default: Option<bool>,
}

async fn patch_config(
    State(state): State<AppState>,
    Json(patch): Json<ConfigPatch>,
) -> Result<impl IntoResponse, ApiError> {
    let mut config = state.config.read().await.clone();
    if let Some(video) = patch.video {
        config.video.device = video.device;
        config.video.auto_detect = config.video.device.is_none();
        config.video.width = video.width;
        config.video.height = video.height;
        config.video.frames_per_second = video.frames_per_second;
        if let Some(encoding) = video.encoding {
            config.video.encoding = encoding;
        }
        if let Some(quality) = video.jpeg_quality {
            config.video.jpeg_quality = quality;
        }
    }
    if let Some(hid) = patch.hid {
        config.hid.keyboard_device = hid.keyboard_device;
        config.hid.mouse_device = hid.mouse_device;
        config.hid.auto_detect =
            config.hid.keyboard_device.is_none() || config.hid.mouse_device.is_none();
    }
    if let Some(power) = patch.power {
        if let Some(enabled) = power.enabled {
            config.power.enabled = enabled;
        }
        config.power.gpio_chip = power.gpio_chip;
        config.power.gpio_line = power.gpio_line;
        if let Some(active_high) = power.active_high {
            config.power.active_high = active_high;
        }
    }
    if let Some(media) = patch.media {
        if let Some(enabled) = media.enabled {
            config.media.enabled = enabled;
        }
        config.media.lun_path = media.lun_path;
        config.media.image_directory = media.image_directory;
        if let Some(read_only) = media.read_only_by_default {
            config.media.read_only_by_default = read_only;
        }
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
    let path = state.config.read().await.hid.mouse_device.clone();
    state
        .hid
        .mouse_move(path, request)
        .await
        .map_err(map_hid_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mouse_click(
    State(state): State<AppState>,
    Json(request): Json<MouseClickRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let path = state.config.read().await.hid.mouse_device.clone();
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
    let path = state.config.read().await.hid.mouse_device.clone();
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
        .release_all(config.keyboard_device, config.mouse_device)
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
    #[serde(default = "default_true")]
    read_only: bool,
}

fn default_true() -> bool {
    true
}

async fn attach_media(
    State(state): State<AppState>,
    Json(request): Json<AttachMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let status = state
        .media
        .attach(
            media_snapshot(&state).await,
            &request.name,
            request.read_only,
        )
        .await
        .map_err(map_media_error)?;
    Ok(Json(status))
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
    Ok(Json(status))
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
    Capabilities {
        video: config.video.device.is_some(),
        keyboard: config.hid.keyboard_device.is_some(),
        mouse: config.hid.mouse_device.is_some(),
        gpio_power: config.power.enabled
            && config.power.gpio_chip.is_some()
            && config.power.gpio_line.is_some(),
        mass_storage: config.media.enabled && config.media.lun_path.is_some(),
        video_passthrough: true,
        video_transcode: true,
    }
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
        AuthError::WeakPassword(_) | AuthError::InvalidUsername => {
            ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
        }
        AuthError::AlreadyInitialized => ApiError::new(StatusCode::CONFLICT, error.to_string()),
        _ => ApiError::internal(error),
    }
}

fn map_hid_error(error: HidError) -> ApiError {
    let status = match error {
        HidError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
        HidError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        HidError::UnsupportedKey(_) | HidError::InvalidButton(_) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
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
        MediaError::InvalidName | MediaError::OutsideStorage | MediaError::UnsupportedType => {
            StatusCode::UNPROCESSABLE_ENTITY
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

    #[test]
    fn setup_token_comparison_checks_length_and_contents() {
        assert!(constant_time_eq("correct-token", "correct-token"));
        assert!(!constant_time_eq("correct-token", "wrong-token"));
        assert!(!constant_time_eq("correct-token", "correct-token-extra"));
    }

    #[test]
    fn configuration_validation_rejects_unsafe_values() {
        let mut config = Config::default();
        config.video.frames_per_second = Some(121);
        assert!(validate_config(&config).is_err());

        let mut config = Config::default();
        config.hid.keyboard_device = Some(PathBuf::from("dev/hidg0"));
        assert!(validate_config(&config).is_err());
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
