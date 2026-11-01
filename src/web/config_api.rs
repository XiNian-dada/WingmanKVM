use super::*;

use axum::http::header::CONTENT_DISPOSITION;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const CONFIG_EXPORT_FORMAT: &str = "wingmankvm-config";
pub(super) const CONFIG_IMPORT_BODY_LIMIT: usize = 256 * 1024;

pub(super) async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.config.read().await.redacted_json()))
}

/// Download document for `GET /api/config/export`.
///
/// Only [`Config`] is included. Accounts, password hashes, Passkeys, sessions
/// and audit logs live in `auth.json` and in memory, never in this structure.
#[derive(Debug, Serialize)]
pub(super) struct ConfigExport {
    pub(super) format: &'static str,
    pub(super) version: u32,
    pub(super) exported_at: String,
    pub(super) config: Config,
}

impl ConfigExport {
    fn new(mut config: Config) -> Self {
        // Backups are often shared or stored loosely; notification
        // credentials stay on the device. The ntfy topic is included because
        // anyone who knows it can read the topic.
        let notifications = &mut config.notifications;
        notifications.webhook.url = None;
        notifications.ntfy.token = None;
        notifications.ntfy.topic = None;
        notifications.telegram.bot_token = None;
        let (_, exported_at) = utc_date_and_timestamp();
        Self {
            format: CONFIG_EXPORT_FORMAT,
            version: CONFIG_VERSION,
            exported_at,
            config,
        }
    }
}

/// Upload document accepted by the preview and import endpoints.
///
/// Fields are optional here so every rejection can be reported as a clear,
/// localized JSON error instead of a generic deserialization failure.
#[derive(Debug, Deserialize)]
pub(super) struct ConfigImportFile {
    format: Option<String>,
    version: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    exported_at: Option<String>,
    config: Option<serde_json::Value>,
}

impl ConfigImportFile {
    fn into_config(self) -> Result<Config, ApiError> {
        let format = self
            .format
            .ok_or_else(|| ApiError::bad_request("不是 WingmanKVM 配置导出文件：缺少 format"))?;
        if format != CONFIG_EXPORT_FORMAT {
            return Err(ApiError::bad_request(format!(
                "不是 WingmanKVM 配置导出文件：format 必须为 {CONFIG_EXPORT_FORMAT}"
            )));
        }
        let version = self
            .version
            .ok_or_else(|| ApiError::bad_request("配置导出文件缺少 version"))?;
        if version != CONFIG_VERSION {
            return Err(ApiError::bad_request(format!(
                "配置版本 {version} 不受支持，本机仅支持版本 {CONFIG_VERSION}"
            )));
        }
        let value = self
            .config
            .ok_or_else(|| ApiError::bad_request("配置导出文件缺少 config 内容"))?;
        if let Some(content_version) = value.get("version").and_then(serde_json::Value::as_u64) {
            if content_version != u64::from(CONFIG_VERSION) {
                return Err(ApiError::bad_request(format!(
                    "配置内容版本 {content_version} 不受支持，本机仅支持版本 {CONFIG_VERSION}"
                )));
            }
        }
        reject_retired_hid_timing_fields(&value).map_err(ApiError::bad_request)?;
        let config: Config = serde_json::from_value(value)
            .map_err(|error| ApiError::bad_request(format!("配置内容无效：{error}")))?;
        validate_config(&config)?;
        Ok(config)
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct ConfigChange {
    pub(super) path: String,
    pub(super) old: serde_json::Value,
    pub(super) new: serde_json::Value,
}

pub(super) fn collect_config_changes(
    old: &serde_json::Value,
    new: &serde_json::Value,
    path: &str,
    changes: &mut Vec<ConfigChange>,
) {
    match (old, new) {
        (serde_json::Value::Object(old), serde_json::Value::Object(new)) => {
            let mut keys: Vec<&String> = old.keys().chain(new.keys()).collect();
            keys.sort_unstable();
            keys.dedup();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_config_changes(
                    old.get(key).unwrap_or(&serde_json::Value::Null),
                    new.get(key).unwrap_or(&serde_json::Value::Null),
                    &child,
                    changes,
                );
            }
        }
        _ if old != new => changes.push(ConfigChange {
            path: path.to_owned(),
            old: old.clone(),
            new: new.clone(),
        }),
        _ => {}
    }
}

pub(super) fn config_changes(old: &Config, new: &Config) -> Vec<ConfigChange> {
    let old = serde_json::to_value(old).unwrap_or(serde_json::Value::Null);
    let new = serde_json::to_value(new).unwrap_or(serde_json::Value::Null);
    let mut changes = Vec::new();
    collect_config_changes(&old, &new, "", &mut changes);
    changes
}

pub(super) fn listen_address_changed(previous: &Config, next: &Config) -> bool {
    previous.server.listen_address != next.server.listen_address
        || previous.server.port != next.server.port
}

/// Exports the live configuration as a downloadable JSON document.
///
/// The response body is deliberately built from [`Config`] alone; `auth.json`,
/// Passkey credentials, sessions and audit logs are never serialized here.
pub(super) async fn export_config(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let config = state.config.read().await.clone();
    let (date, _) = utc_date_and_timestamp();
    let document = ConfigExport::new(config);
    let mut body = serde_json::to_vec_pretty(&document).map_err(ApiError::internal)?;
    body.push(b'\n');
    let disposition = format!("attachment; filename=\"wingmankvm-config-{date}.json\"");
    let disposition = HeaderValue::from_str(&disposition).map_err(ApiError::internal)?;
    let mut response = (
        [
            (CONTENT_TYPE, "application/json; charset=utf-8"),
            (CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response();
    response.headers_mut().insert(CONTENT_DISPOSITION, disposition);
    Ok(response)
}

pub(super) async fn preview_import_config(
    State(state): State<AppState>,
    ApiJson(file): ApiJson<ConfigImportFile>,
) -> Result<impl IntoResponse, ApiError> {
    let imported = file.into_config()?;
    let current = state.config.read().await.clone();
    let changes = config_changes(&current, &imported);
    Ok(no_store(Json(json!({
        "changes": changes,
        "restart_required": listen_address_changed(&current, &imported),
    }))))
}

pub(super) async fn import_config(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(file): ApiJson<ConfigImportFile>,
) -> Result<Response, ApiError> {
    let imported = file.into_config()?;
    let (response, previous, current) = import_config_with_state(state.clone(), imported).await?;
    audit_config_change(&state, connection, &headers, &previous, &current).await;
    Ok(response)
}

async fn import_config_with_state(
    state: AppState,
    mut imported: Config,
) -> Result<(Response, Config, Config), ApiError> {
    let _update_guard = state.config_update.lock().await;
    let previous = state.config.read().await.clone();
    // Exports omit notification credentials; keep the device's own.
    imported.notifications.inherit_secrets(&previous.notifications);
    if imported.notifications.ntfy_topic().is_none() {
        imported.notifications.ntfy.topic.clone_from(&previous.notifications.ntfy.topic);
    }
    let changes = config_changes(&previous, &imported);
    let restart_required = listen_address_changed(&previous, &imported);
    commit_config(&state, &imported).await?;
    let mut response = no_store(Json(json!({
        "ok": true,
        "restart_required": restart_required,
        "changes": changes,
    })))
    .into_response();
    if restart_required {
        response.headers_mut().insert(
            "x-wingmankvm-restart-required",
            HeaderValue::from_static("true"),
        );
    }
    Ok((response, previous, imported))
}

pub(super) struct FullConfigUpdate(Config);

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

pub(super) async fn update_config(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(FullConfigUpdate(mut config)): ApiJson<FullConfigUpdate>,
) -> Result<impl IntoResponse, ApiError> {
    let _update_guard = state.config_update.lock().await;
    let previous = state.config.read().await.clone();
    // The browser never sees notification secrets, so a full update that
    // leaves them empty keeps the stored values.
    config.notifications.inherit_secrets(&previous.notifications);
    validate_config(&config)?;
    let listener_changed = previous.server.listen_address != config.server.listen_address
        || previous.server.port != config.server.port;
    commit_config(&state, &config).await?;
    audit_config_change(&state, connection, &headers, &previous, &config).await;
    let mut response = no_store(Json(config.redacted_json())).into_response();
    if listener_changed {
        response.headers_mut().insert(
            "x-wingmankvm-restart-required",
            HeaderValue::from_static("true"),
        );
    }
    Ok(response)
}

#[derive(Default, Deserialize)]
pub(super) struct ConfigPatch {
    pub(super) server: Option<ServerPatch>,
    pub(super) video: Option<VideoPatch>,
    pub(super) hid: Option<HidPatch>,
    pub(super) power: Option<PowerPatch>,
    pub(super) media: Option<MediaPatch>,
    pub(super) notifications: Option<NotificationsPatch>,
    pub(super) audio: Option<AudioPatch>,
}

#[derive(Deserialize)]
pub(super) struct AudioPatch {
    pub(super) enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) device: Option<Option<String>>,
    pub(super) bitrate_kbps: Option<u32>,
}

#[derive(Deserialize)]
pub(super) struct ServerPatch {
    pub(super) proxy_protocol: Option<ProxyProtocolMode>,
    pub(super) trusted_http_proxies: Option<Vec<IpAddr>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) public_origin: Option<Option<String>>,
}

#[derive(Deserialize)]
pub(super) struct VideoPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) width: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) height: Option<Option<u32>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) frames_per_second: Option<Option<u32>>,
    pub(super) encoding: Option<VideoEncoding>,
    pub(super) jpeg_quality: Option<u8>,
    pub(super) h264: Option<H264Patch>,
}

#[derive(Deserialize)]
pub(super) struct H264Patch {
    pub(super) bitrate_kbps: Option<u32>,
    pub(super) encoder: Option<H264Encoder>,
    pub(super) allow_software: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) ffmpeg_path: Option<Option<PathBuf>>,
    pub(super) max_sessions: Option<u8>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) udp_bind_address: Option<Option<SocketAddr>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) public_udp_endpoint: Option<Option<String>>,
}

#[derive(Deserialize)]
pub(super) struct HidPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) keyboard_device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) mouse_device: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) absolute_pointer_device: Option<Option<PathBuf>>,
    pub(super) pointer_mode: Option<PointerMode>,
    #[serde(
        default,
        rename = "write_timeout_ms",
        deserialize_with = "reject_retired_write_timeout_ms"
    )]
    #[allow(dead_code)]
    pub(super) _write_timeout_ms: Option<serde_json::Value>,
    #[serde(
        default,
        rename = "retry_interval_ms",
        deserialize_with = "reject_retired_retry_interval_ms"
    )]
    #[allow(dead_code)]
    pub(super) _retry_interval_ms: Option<serde_json::Value>,
}

pub(super) fn retired_hid_timing_error(field: &'static str) -> String {
    format!(
        "{field} is deprecated and ignored: HID reports use a fixed 500 ms per-report deadline with readiness-based retry instead of polling; remove this field"
    )
}

pub(super) fn reject_retired_hid_timing<'de, D>(
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

pub(super) fn reject_retired_write_timeout_ms<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    reject_retired_hid_timing(deserializer, "hid.write_timeout_ms")
}

pub(super) fn reject_retired_retry_interval_ms<'de, D>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error>
where
    D: Deserializer<'de>,
{
    reject_retired_hid_timing(deserializer, "hid.retry_interval_ms")
}

pub(super) fn reject_retired_hid_timing_fields(value: &serde_json::Value) -> Result<(), String> {
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

pub(super) fn deserialize_optional_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
pub(super) struct PowerPatch {
    pub(super) enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_line: Option<Option<u32>>,
    pub(super) active_high: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) reset_switch: Option<Option<GpioPulsePatch>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) power_led: Option<Option<GpioInputPatch>>,
    pub(super) short_press_ms: Option<u64>,
    pub(super) long_press_ms: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct GpioPulsePatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_line: Option<Option<u32>>,
    pub(super) active_high: Option<bool>,
    pub(super) pulse_ms: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct GpioInputPatch {
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_chip: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) gpio_line: Option<Option<u32>>,
    pub(super) active_low: Option<bool>,
    pub(super) bias: Option<GpioBias>,
    pub(super) poll_interval_ms: Option<u64>,
    pub(super) debounce_ms: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct MediaPatch {
    pub(super) enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) lun_path: Option<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) image_directory: Option<Option<PathBuf>>,
    pub(super) read_only_by_default: Option<bool>,
}

pub(super) async fn patch_config(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(patch): ApiJson<ConfigPatch>,
) -> Result<impl IntoResponse, ApiError> {
    let _update_guard = state.config_update.lock().await;
    let previous = state.config.read().await.clone();
    let mut config = previous.clone();
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
    if let Some(notifications) = patch.notifications {
        apply_notifications_patch(&mut config.notifications, notifications);
    }
    if let Some(audio) = patch.audio {
        apply_audio_patch(&mut config.audio, audio);
    }
    validate_config(&config)?;
    commit_config(&state, &config).await?;
    audit_config_change(&state, connection, &headers, &previous, &config).await;
    Ok(no_store(Json(config.redacted_json())))
}

/// Audit only the names of changed top-level sections; never their values.
pub(super) fn changed_sections(previous: &Config, current: &Config) -> Vec<&'static str> {
    [
        ("server", previous.server != current.server),
        ("video", previous.video != current.video),
        ("hid", previous.hid != current.hid),
        ("power", previous.power != current.power),
        ("media", previous.media != current.media),
        ("notifications", previous.notifications != current.notifications),
        ("audio", previous.audio != current.audio),
    ]
    .into_iter()
    .filter_map(|(name, changed)| changed.then_some(name))
    .collect()
}

pub(super) async fn audit_config_change(
    state: &AppState,
    connection: ClientConnection,
    headers: &HeaderMap,
    previous: &Config,
    current: &Config,
) {
    let changed = changed_sections(previous, current);
    if changed.is_empty() {
        return;
    }
    let (ip, session) = audit_actor(state, connection, headers).await;
    state
        .audit
        .record(
            AuditEvent::ConfigChanged,
            ip,
            session,
            Some(changed.join(",")),
        )
        .await;
}

pub(super) fn apply_server_patch(config: &mut config::ServerConfig, patch: ServerPatch) {
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

pub(super) fn apply_audio_patch(config: &mut config::AudioConfig, patch: AudioPatch) {
    if let Some(enabled) = patch.enabled {
        config.enabled = enabled;
    }
    if let Some(device) = patch.device {
        // An empty selection means "the capture card's own sound card".
        config.device = optional_string(device);
    }
    if let Some(bitrate_kbps) = patch.bitrate_kbps {
        config.bitrate_kbps = bitrate_kbps;
    }
}

/// Inputs of the shared audio capture: its own section, the FFmpeg binary
/// shared with H.264 and the capture card used to find its sound card.
pub(super) fn audio_settings(config: &Config) -> AudioSettings {
    AudioSettings::new(
        config.audio.clone(),
        config.video.h264.ffmpeg_path.clone(),
        config.video.device.clone(),
    )
}

pub(super) fn apply_video_patch(config: &mut config::VideoConfig, patch: VideoPatch) {
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

pub(super) fn apply_hid_patch(config: &mut HidConfig, patch: HidPatch) {
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

pub(super) fn hid_needs_auto_detection(config: &HidConfig) -> bool {
    config.keyboard_device.is_none()
        || match config.pointer_mode {
            PointerMode::Absolute => config.absolute_pointer_device.is_none(),
            PointerMode::Relative => config.mouse_device.is_none(),
            PointerMode::Auto => {
                config.mouse_device.is_none() && config.absolute_pointer_device.is_none()
            }
        }
}

pub(super) fn apply_power_patch(config: &mut config::PowerConfig, patch: PowerPatch) {
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

pub(super) fn apply_media_patch(config: &mut config::MediaConfig, patch: MediaPatch) {
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

/// Notification settings. Secret fields follow "empty keeps, explicit
/// `clear_*` removes": the browser never receives stored secrets, so it can
/// only send a new value or ask for removal.
#[derive(Deserialize)]
pub(super) struct NotificationsPatch {
    pub(super) webhook: Option<WebhookPatch>,
    pub(super) ntfy: Option<NtfyPatch>,
    pub(super) telegram: Option<TelegramPatch>,
    pub(super) events: Option<NotificationEventsPatch>,
    pub(super) hid_failure_threshold: Option<u32>,
    pub(super) hid_failure_window_seconds: Option<u64>,
    pub(super) video_lost_seconds: Option<u64>,
    pub(super) rate_limit_seconds: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct WebhookPatch {
    pub(super) enabled: Option<bool>,
    pub(super) url: Option<String>,
    #[serde(default)]
    pub(super) clear_url: bool,
}

#[derive(Deserialize)]
pub(super) struct NtfyPatch {
    pub(super) enabled: Option<bool>,
    pub(super) server: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) topic: Option<Option<String>>,
    pub(super) token: Option<String>,
    #[serde(default)]
    pub(super) clear_token: bool,
}

#[derive(Deserialize)]
pub(super) struct TelegramPatch {
    pub(super) enabled: Option<bool>,
    pub(super) bot_token: Option<String>,
    #[serde(default)]
    pub(super) clear_bot_token: bool,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub(super) chat_id: Option<Option<String>>,
}

#[derive(Deserialize)]
pub(super) struct NotificationEventsPatch {
    pub(super) login_succeeded: Option<bool>,
    pub(super) login_failed: Option<bool>,
    pub(super) power_led_changed: Option<bool>,
    pub(super) hid_failing: Option<bool>,
    pub(super) video_lost: Option<bool>,
}

fn apply_secret(target: &mut Option<String>, value: Option<String>, clear: bool) {
    if clear {
        *target = None;
    }
    if let Some(value) = optional_string(value) {
        *target = Some(value);
    }
}

pub(super) fn apply_notifications_patch(
    config: &mut config::NotificationsConfig,
    patch: NotificationsPatch,
) {
    if let Some(webhook) = patch.webhook {
        if let Some(enabled) = webhook.enabled {
            config.webhook.enabled = enabled;
        }
        apply_secret(&mut config.webhook.url, webhook.url, webhook.clear_url);
        if webhook.clear_url {
            config.webhook.enabled = false;
        }
    }
    if let Some(ntfy) = patch.ntfy {
        if let Some(enabled) = ntfy.enabled {
            config.ntfy.enabled = enabled;
        }
        if let Some(server) = ntfy.server {
            let server = server.trim();
            config.ntfy.server = if server.is_empty() {
                config::DEFAULT_NTFY_SERVER.to_owned()
            } else {
                server.to_owned()
            };
        }
        if let Some(topic) = ntfy.topic {
            config.ntfy.topic = optional_string(topic);
        }
        apply_secret(&mut config.ntfy.token, ntfy.token, ntfy.clear_token);
    }
    if let Some(telegram) = patch.telegram {
        if let Some(enabled) = telegram.enabled {
            config.telegram.enabled = enabled;
        }
        apply_secret(
            &mut config.telegram.bot_token,
            telegram.bot_token,
            telegram.clear_bot_token,
        );
        if telegram.clear_bot_token {
            config.telegram.enabled = false;
        }
        if let Some(chat_id) = telegram.chat_id {
            config.telegram.chat_id = optional_string(chat_id);
        }
    }
    if let Some(events) = patch.events {
        for (target, value) in [
            (&mut config.events.login_succeeded, events.login_succeeded),
            (&mut config.events.login_failed, events.login_failed),
            (&mut config.events.power_led_changed, events.power_led_changed),
            (&mut config.events.hid_failing, events.hid_failing),
            (&mut config.events.video_lost, events.video_lost),
        ] {
            if let Some(value) = value {
                *target = value;
            }
        }
    }
    if let Some(value) = patch.hid_failure_threshold {
        config.hid_failure_threshold = value;
    }
    if let Some(value) = patch.hid_failure_window_seconds {
        config.hid_failure_window_seconds = value;
    }
    if let Some(value) = patch.video_lost_seconds {
        config.video_lost_seconds = value;
    }
    if let Some(value) = patch.rate_limit_seconds {
        config.rate_limit_seconds = value;
    }
}

pub(super) async fn persist_config(path: &Path, config: &Config) -> Result<(), ApiError> {
    let path = path.to_path_buf();
    let config = config.clone();
    tokio::task::spawn_blocking(move || config.save_atomic(path))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

pub(super) async fn commit_config(state: &AppState, config: &Config) -> Result<(), ApiError> {
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
    // Audio is applied on its own: an audio-only change never reaches the
    // video pipeline above, and an unchanged setting is a no-op here.
    state.webrtc.audio().reconfigure(audio_settings(config));
    let power_led = power_led_snapshot(&config.power);
    *state.config.write().await = config.clone();
    state.proxy_protocol_updates.send_replace(config.server.proxy_protocol);
    state.power.set_power_led_config(power_led).await;
    state.hid.set_keyboard_device(config.hid.keyboard_device.clone());
    state.notifier.update_config(config.notifications.clone());
    Ok(())
}

pub(super) fn optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

pub(super) fn optional_path(value: Option<String>) -> Option<PathBuf> {
    optional_string(value).map(PathBuf::from)
}

/// Current UTC date and RFC 3339 timestamp without pulling in a date crate.
fn utc_date_and_timestamp() -> (String, String) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let within_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_days(days);
    let hour = within_day / 3_600;
    let minute = within_day % 3_600 / 60;
    let second = within_day % 60;
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"),
    )
}

/// Converts days since 1970-01-01 to a proleptic Gregorian date.
fn civil_date_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096)
            / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let mut random = [0_u8; 8];
            getrandom::fill(&mut random).unwrap();
            let path = std::env::temp_dir().join(format!(
                "wingmankvm-config-backup-test-{}-{:016x}",
                std::process::id(),
                u64::from_ne_bytes(random)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(in crate::web) fn test_state(config: Config, path: PathBuf) -> AppState {
        let (video, _) = VideoManager::test_source(config.video.clone());
        let webrtc = WebRtcManager::test_stub(video.clone(), config.video.h264.clone());
        AppState {
            proxy_protocol_updates: watch::channel(config.server.proxy_protocol).0,
            config: Arc::new(RwLock::new(config)),
            config_path: Arc::new(path.clone()),
            auth: AuthStore::new(path.with_file_name("auth.json")),
            audit: AuditLog::new(path.parent().unwrap()),
            sessions: SessionStore::default(),
            setup: Arc::new(AsyncMutex::new(())),
            config_update: Arc::new(AsyncMutex::new(())),
            password_migration: Arc::new(AsyncMutex::new(())),
            setup_token: Arc::new(Mutex::new(None)),
            setup_scan: Arc::new(AsyncMutex::new(None)),
            login_limiter: LoginLimiter::default(),
            passkey_start_limiter: LoginLimiter::with_budget(MAX_PASSKEY_STARTS_PER_IP),
            passkey_challenges: Arc::new(Mutex::new(HashMap::new())),
            hid: HidManager::new(),
            power: PowerManager::new(),
            media: Arc::new(MediaManager::default()),
            media_upload: Arc::new(AsyncMutex::new(())),
            media_download: Arc::new(MediaDownloadManager::new().unwrap()),
            video,
            webrtc,
            system: Arc::new(crate::system::SystemMonitor::new()),
            metrics: Arc::new(RuntimeMetrics::new()),
            notifier: crate::notify::Notifier::new(Default::default()),
            update: OnlineUpdate::new(OnlineUpdateOptions::system(path.parent().unwrap().to_path_buf())),
        }
    }

    #[test]
    fn export_document_only_contains_configuration() {
        let mut config = Config::default();
        config.server.port = 9090;
        config.video.width = Some(1920);
        let document = ConfigExport::new(config.clone());
        let value = serde_json::to_value(&document).unwrap();
        let keys: std::collections::BTreeSet<&str> =
            value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["config", "exported_at", "format", "version"]
                .into_iter()
                .collect()
        );
        assert_eq!(value["format"], CONFIG_EXPORT_FORMAT);
        assert_eq!(value["version"], CONFIG_VERSION);
        assert_eq!(value["config"], serde_json::to_value(&config).unwrap());
        assert!(value["exported_at"].as_str().unwrap().ends_with('Z'));

        let text = serde_json::to_string(&document).unwrap().to_ascii_lowercase();
        for forbidden in [
            "password",
            "passkey",
            "session_id",
            "auth.json",
            "setup_token",
            "audit",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} leaked into export");
        }
    }

    #[test]
    fn export_import_roundtrip_is_lossless() {
        let mut config = Config::default();
        config.server.port = 9090;
        config.server.public_origin = Some("https://kvm.example.com".to_owned());
        config.video.width = Some(1920);
        config.video.h264.bitrate_kbps = 8_000;
        config.hid.pointer_mode = PointerMode::Relative;
        config.power.short_press_ms = 750;
        config.media.max_upload_bytes = 1_024;

        let document = serde_json::to_value(ConfigExport::new(config.clone())).unwrap();
        let file: ConfigImportFile = serde_json::from_value(document).unwrap();
        let imported = file.into_config().expect("valid export must import");
        assert_eq!(imported, config);
        assert!(config_changes(&config, &imported).is_empty());
    }

    #[test]
    fn import_rejects_wrong_format_version_and_invalid_configuration() {
        let exported = serde_json::to_value(ConfigExport::new(Config::default())).unwrap();

        let mut wrong_format = exported.clone();
        wrong_format["format"] = json!("other-tool");
        let error = serde_json::from_value::<ConfigImportFile>(wrong_format)
            .unwrap()
            .into_config()
            .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.message.contains("format"), "{}", error.message);

        let mut wrong_version = exported.clone();
        wrong_version["version"] = json!(CONFIG_VERSION + 1);
        let error = serde_json::from_value::<ConfigImportFile>(wrong_version)
            .unwrap()
            .into_config()
            .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            error.message.contains("不受支持") && error.message.contains("2"),
            "{}",
            error.message
        );

        let mut missing_version = exported.clone();
        missing_version.as_object_mut().unwrap().remove("version");
        let error = serde_json::from_value::<ConfigImportFile>(missing_version)
            .unwrap()
            .into_config()
            .unwrap_err();
        assert!(error.message.contains("version"), "{}", error.message);

        let mut content_version = exported.clone();
        content_version["config"]["version"] = json!(99);
        let error = serde_json::from_value::<ConfigImportFile>(content_version)
            .unwrap()
            .into_config()
            .unwrap_err();
        assert!(
            error.message.contains("配置内容版本 99"),
            "{}",
            error.message
        );

        let mut invalid = exported;
        invalid["config"]["server"]["port"] = json!(0);
        let error = serde_json::from_value::<ConfigImportFile>(invalid)
            .unwrap()
            .into_config()
            .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.message.contains("server.port"), "{}", error.message);
    }

    #[test]
    fn diff_lists_changed_field_paths_with_old_and_new_values() {
        let old = Config::default();
        let mut new = old.clone();
        new.server.port = 9090;
        new.server.public_origin = Some("https://kvm.example.com".to_owned());
        new.video.width = Some(1920);
        new.hid.mouse_device = Some(PathBuf::from("/dev/hidg1"));

        let changes = config_changes(&old, &new);
        let by_path: std::collections::HashMap<&str, &ConfigChange> =
            changes.iter().map(|change| (change.path.as_str(), change)).collect();
        assert_eq!(changes.len(), 4, "{changes:?}");
        assert_eq!(by_path["server.port"].old, json!(8080));
        assert_eq!(by_path["server.port"].new, json!(9090));
        assert_eq!(by_path["server.public_origin"].old, serde_json::Value::Null);
        assert_eq!(
            by_path["server.public_origin"].new,
            json!("https://kvm.example.com")
        );
        assert_eq!(by_path["video.width"].old, serde_json::Value::Null);
        assert_eq!(by_path["video.width"].new, json!(1920));
        assert_eq!(by_path["hid.mouse_device"].new, json!("/dev/hidg1"));
    }

    #[tokio::test]
    async fn failed_import_validation_leaves_live_and_persisted_config_unchanged() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        let config = Config::default();
        config.save_atomic(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let state = test_state(config, path.clone());

        let mut document = serde_json::to_value(ConfigExport::new(Config::default())).unwrap();
        document["config"]["server"]["port"] = json!(0);
        let file: ConfigImportFile = serde_json::from_value(document).unwrap();
        assert!(file.into_config().is_err());

        assert_eq!(state.config.read().await.server.port, 8080);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn import_waits_for_an_in_flight_config_update() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        let config = Config::default();
        config.save_atomic(&path).unwrap();
        let state = test_state(config, path.clone());

        let guard = state.config_update.lock().await;
        let mut imported = state.config.read().await.clone();
        imported.server.port = 9090;
        let task = tokio::spawn({
            let state = state.clone();
            async move { import_config_with_state(state, imported).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!task.is_finished(), "import must wait for the save lock");
        assert_eq!(state.config.read().await.server.port, 8080);
        assert_eq!(Config::load(&path).unwrap().server.port, 8080);

        drop(guard);
        task.await.unwrap().expect("import should complete");
        assert_eq!(state.config.read().await.server.port, 9090);
        assert_eq!(Config::load(&path).unwrap().server.port, 9090);
    }

    fn with_notification_secrets(mut config: Config) -> Config {
        let notifications = &mut config.notifications;
        notifications.webhook.url = Some("https://hooks.example/SECRET-WEBHOOK".into());
        notifications.ntfy.token = Some("SECRET-NTFY-TOKEN".into());
        notifications.ntfy.topic = Some("SECRET-NTFY-TOPIC".into());
        notifications.telegram.bot_token = Some("123456:SECRET-BOT-TOKEN".into());
        notifications.telegram.chat_id = Some("12345".into());
        config
    }

    #[test]
    fn export_omits_notification_secrets() {
        let config = with_notification_secrets(Config::default());
        let text = serde_json::to_string(&ConfigExport::new(config)).unwrap();
        assert!(!text.contains("SECRET-"), "notification secret leaked: {text}");
        assert!(text.contains("12345"), "non-secret settings stay in the export");
    }

    #[tokio::test]
    async fn import_keeps_existing_notification_secrets() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        let config = with_notification_secrets(Config::default());
        config.save_atomic(&path).unwrap();
        let state = test_state(config.clone(), path);

        let document = serde_json::to_value(ConfigExport::new(config.clone())).unwrap();
        let file: ConfigImportFile = serde_json::from_value(document).unwrap();
        import_config_with_state(state.clone(), file.into_config().unwrap())
            .await
            .unwrap();
        assert_eq!(state.config.read().await.notifications, config.notifications);
    }

    #[test]
    fn changed_sections_reports_only_top_level_names() {
        let previous = Config::default();
        let mut current = previous.clone();
        assert!(changed_sections(&previous, &current).is_empty());
        current.video.jpeg_quality = current.video.jpeg_quality.saturating_add(1);
        current.power.active_high = !current.power.active_high;
        assert_eq!(changed_sections(&previous, &current), vec!["video", "power"]);
        let detail = changed_sections(&previous, &current).join(",");
        assert_eq!(detail, "video,power");
        assert!(!detail.contains(&current.video.jpeg_quality.to_string()));
    }

    #[test]
    fn audio_patch_preserves_omitted_fields_and_empty_device_means_automatic() {
        let mut audio = config::AudioConfig::default();
        let patch: ConfigPatch = serde_json::from_value(json!({
            "audio": { "enabled": true, "device": " plughw:CARD=MS2109,DEV=0 " }
        }))
        .unwrap();
        apply_audio_patch(&mut audio, patch.audio.unwrap());
        assert!(audio.enabled);
        assert_eq!(audio.device.as_deref(), Some("plughw:CARD=MS2109,DEV=0"));
        assert_eq!(audio.bitrate_kbps, 96);

        let patch: AudioPatch = serde_json::from_value(json!({ "bitrate_kbps": 64 })).unwrap();
        apply_audio_patch(&mut audio, patch);
        assert_eq!(audio.device.as_deref(), Some("plughw:CARD=MS2109,DEV=0"));
        assert_eq!(audio.bitrate_kbps, 64);
        for clear in [json!({ "device": "" }), json!({ "device": null })] {
            let mut cleared = audio.clone();
            apply_audio_patch(&mut cleared, serde_json::from_value(clear).unwrap());
            assert!(cleared.device.is_none());
            assert!(cleared.enabled);
        }

        let previous = Config::default();
        let mut current = previous.clone();
        current.audio = audio;
        assert_eq!(changed_sections(&previous, &current), vec!["audio"]);
    }

    #[tokio::test]
    async fn audio_only_change_is_applied_without_touching_video() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        let config = Config::default();
        config.save_atomic(&path).unwrap();
        let state = test_state(config.clone(), path.clone());
        assert!(!state.webrtc.audio().enabled());

        let mut next = config.clone();
        next.audio.enabled = true;
        next.audio.device = Some("hw:1,0".to_owned());
        commit_config(&state, &next).await.unwrap();
        assert!(state.webrtc.audio().enabled());
        let live = state.config.read().await.clone();
        assert_eq!(live.video, config.video, "video stays as it was");
        assert_eq!(Config::load(&path).unwrap().audio, next.audio);

        let mut invalid = next.clone();
        invalid.audio.device = Some("file:/tmp/x".to_owned());
        assert!(commit_config(&state, &invalid).await.is_err());
        assert_eq!(Config::load(&path).unwrap().audio, next.audio);
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
    fn notification_patch_keeps_secrets_unless_replaced_or_cleared() {
        let mut config = config::NotificationsConfig::default();
        let patch = serde_json::from_value(json!({
            "webhook": {"enabled": true, "url": "https://hooks.example.com/abc"},
            "ntfy": {"enabled": true, "topic": "kvm", "token": "tk_one"},
            "telegram": {"enabled": true, "bot_token": "1:AAA", "chat_id": "42"},
            "events": {"login_succeeded": false},
            "video_lost_seconds": 45
        }))
        .unwrap();
        apply_notifications_patch(&mut config, patch);
        assert!(config.validate().is_ok());
        assert_eq!(config.webhook.url.as_deref(), Some("https://hooks.example.com/abc"));
        assert!(!config.events.login_succeeded);
        assert!(config.events.login_failed);
        assert_eq!(config.video_lost_seconds, 45);

        // Empty secrets (what the browser sends back) keep the stored ones.
        let patch = serde_json::from_value(json!({
            "webhook": {"url": ""},
            "ntfy": {"token": "  "},
            "telegram": {"bot_token": "", "chat_id": "43"}
        }))
        .unwrap();
        apply_notifications_patch(&mut config, patch);
        assert_eq!(config.webhook.url.as_deref(), Some("https://hooks.example.com/abc"));
        assert_eq!(config.ntfy.token.as_deref(), Some("tk_one"));
        assert_eq!(config.telegram.bot_token.as_deref(), Some("1:AAA"));
        assert_eq!(config.telegram.chat_id.as_deref(), Some("43"));

        // Explicit clears remove the secret and disable channels that need it.
        let patch = serde_json::from_value(json!({
            "webhook": {"clear_url": true},
            "ntfy": {"clear_token": true},
            "telegram": {"clear_bot_token": true}
        }))
        .unwrap();
        apply_notifications_patch(&mut config, patch);
        assert!(config.webhook.url.is_none() && !config.webhook.enabled);
        assert!(config.ntfy.token.is_none() && config.ntfy.enabled);
        assert!(config.telegram.bot_token.is_none() && !config.telegram.enabled);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn notification_patch_output_is_validated() {
        let mut config = Config::default();
        let patch = serde_json::from_value(json!({
            "webhook": {"enabled": true, "url": "ftp://hooks.example.com/abc"}
        }))
        .unwrap();
        apply_notifications_patch(&mut config.notifications, patch);
        assert!(validate_config(&config).is_err());
    }
}
