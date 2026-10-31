use super::*;

pub(super) async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.config.read().await.redacted_json()))
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
    let power_led = power_led_snapshot(&config.power);
    *state.config.write().await = config.clone();
    state.proxy_protocol_updates.send_replace(config.server.proxy_protocol);
    state.power.set_power_led_config(power_led).await;
    state.hid.set_keyboard_device(config.hid.keyboard_device.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

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
