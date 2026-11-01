use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::proxy_protocol::ProxyProtocolMode;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub const CONFIG_VERSION: u32 = 1;
pub const STATE_DIR_ENV: &str = "WINGMANKVM_STATE_DIR";
pub const DEFAULT_STATE_DIR: &str = "/var/lib/wingmankvm";
pub const CONFIG_FILE_NAME: &str = "config.json";
pub const PROXY_PROTOCOL_ENV: &str = "WINGMANKVM_PROXY_PROTOCOL";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub video: VideoConfig,
    #[serde(default)]
    pub hid: HidConfig,
    #[serde(default)]
    pub power: PowerConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub audio: AudioConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            server: ServerConfig::default(),
            video: VideoConfig::default(),
            hid: HidConfig::default(),
            power: PowerConfig::default(),
            media: MediaConfig::default(),
            notifications: NotificationsConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| ConfigError::Io {
            path: path.to_owned(),
            source,
        })?;
        let mut config: Self = serde_json::from_slice(&bytes).map_err(|source| ConfigError::Json {
            path: path.to_owned(),
            source,
        })?;
        config.validate_version()?;
        config.validate_public_origin()?;
        config.validate_server()?;
        if let Some(mode) = proxy_protocol_from_env() {
            config.server.proxy_protocol = mode;
        }
        Ok(config)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let mut config = match Self::load(path) {
            Ok(config) => config,
            Err(ConfigError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                Self::default()
            }
            Err(error) => return Err(error),
        };
        if let Some(mode) = proxy_protocol_from_env() {
            config.server.proxy_protocol = mode;
        }
        Ok(config)
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        self.validate_version()?;
        self.validate_public_origin()?;
        self.validate_server()?;
        let path = path.as_ref();
        let mut contents = serde_json::to_vec_pretty(self).map_err(ConfigError::Serialize)?;
        contents.push(b'\n');
        atomic_write(path, &contents)
    }

    fn validate_version(&self) -> Result<(), ConfigError> {
        if self.version == CONFIG_VERSION {
            Ok(())
        } else {
            Err(ConfigError::UnsupportedVersion {
                found: self.version,
                supported: CONFIG_VERSION,
            })
        }
    }

    fn validate_public_origin(&self) -> Result<(), ConfigError> {
        if let Some(origin) = &self.server.public_origin {
            NormalizedOrigin::parse(origin).map_err(|reason| ConfigError::InvalidPublicOrigin {
                value: origin.clone(),
                reason,
            })?;
        }
        Ok(())
    }

    fn validate_server(&self) -> Result<(), ConfigError> {
        self.server.socket_address().map(|_| ())
    }
}

pub fn proxy_protocol_from_env() -> Option<ProxyProtocolMode> {
    env::var(PROXY_PROTOCOL_ENV).ok().and_then(|val| {
        match val.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ProxyProtocolMode::Auto),
            "required" | "force" | "true" | "1" => Some(ProxyProtocolMode::Required),
            "disabled" | "off" | "false" | "0" => Some(ProxyProtocolMode::Disabled),
            _ => None,
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub listen_address: String,
    pub port: u16,
    pub proxy_protocol: ProxyProtocolMode,
    /// Exact proxy IPs allowed to supply HTTP client-address headers. Empty by default.
    pub trusted_http_proxies: Vec<IpAddr>,
    /// Exact browser-facing HTTP(S) origin for terminal WebSocket upgrades.
    /// Set this behind a reverse proxy and preserve the external Host header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_origin: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".to_owned(),
            port: 8080,
            proxy_protocol: proxy_protocol_from_env().unwrap_or_default(),
            trusted_http_proxies: Vec::new(),
            public_origin: None,
        }
    }
}

impl ServerConfig {
    /// Whether minted and cleared session cookies must carry the `Secure`
    /// attribute. Only the validated `public_origin` decides this; request
    /// headers such as `Host`, `Origin` or `X-Forwarded-Proto` never do.
    ///
    /// An unset or explicit `http://` origin keeps direct HTTP debug
    /// compatibility. A parsed `https://` origin enables `Secure`. A malformed
    /// `Some` value fails closed with `Secure` enabled; configuration loading
    /// and saving still reject invalid origins.
    pub fn session_cookie_secure(&self) -> bool {
        match self.public_origin.as_deref() {
            None => false,
            Some(origin) => NormalizedOrigin::parse(origin)
                .map_or(true, |origin| !origin.is_http()),
        }
    }

    /// Parse the configured listener into a concrete socket address.
    ///
    /// `listen_address` must be a bare IPv4 or IPv6 literal: hostnames,
    /// bracketed IPv6 and surrounding whitespace are rejected. Port 0 is
    /// rejected because the application requires a fixed, usable port.
    pub fn socket_address(&self) -> Result<SocketAddr, ConfigError> {
        let ip = self
            .listen_address
            .parse::<IpAddr>()
            .map_err(|_| ConfigError::InvalidListenAddress {
                value: self.listen_address.clone(),
            })?;
        if self.port == 0 {
            return Err(ConfigError::InvalidServerPort { port: self.port });
        }
        Ok(SocketAddr::new(ip, self.port))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginScheme {
    Http,
    Https,
}

impl OriginScheme {
    fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

/// A strict, normalized HTTP(S) origin. Browser Origin headers and the
/// configured public origin are compared as scheme, host and effective port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedOrigin {
    scheme: OriginScheme,
    host: String,
    port: u16,
}

impl NormalizedOrigin {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("origin 不能为空".to_owned());
        }
        if value.eq_ignore_ascii_case("null") {
            return Err("不接受 null origin".to_owned());
        }
        let (scheme, rest) = value
            .split_once("://")
            .ok_or_else(|| "origin 必须包含 http:// 或 https://".to_owned())?;
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "http" => OriginScheme::Http,
            "https" => OriginScheme::Https,
            _ => return Err("origin 只允许 http 或 https".to_owned()),
        };
        if rest
            .chars()
            .any(|character| matches!(character, '?' | '#' | '@') || character.is_whitespace())
        {
            return Err("origin 不能包含用户信息、查询或片段".to_owned());
        }
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        if !(path.is_empty() || path == "/") {
            return Err("origin 不能包含非根路径".to_owned());
        }
        let (host, port) = parse_authority(authority, scheme.default_port())?;
        Ok(Self { scheme, host, port })
    }

    pub fn is_http(&self) -> bool {
        self.scheme == OriginScheme::Http
    }

    pub fn host_matches(&self, value: &str) -> bool {
        parse_authority(value, self.scheme.default_port())
            .is_ok_and(|(host, port)| host == self.host && port == self.port)
    }
}

fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16), String> {
    if authority.is_empty()
        || authority.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(character, '@' | '?' | '#' | '/' | '\\' | ',')
        })
    {
        return Err("无效的 origin 主机".to_owned());
    }

    if let Some(rest) = authority.strip_prefix('[') {
        let (host, suffix) = rest
            .split_once(']')
            .ok_or_else(|| "IPv6 主机缺少右方括号".to_owned())?;
        let host = host
            .parse::<Ipv6Addr>()
            .map_err(|_| "无效的 IPv6 主机".to_owned())?;
        let port = if suffix.is_empty() {
            default_port
        } else if let Some(port) = suffix.strip_prefix(':') {
            parse_port(port)?
        } else {
            return Err("无效的 IPv6 主机端口".to_owned());
        };
        return Ok((format!("[{host}]"), port));
    }

    if authority.matches(':').count() > 1 {
        return Err("IPv6 主机必须使用方括号".to_owned());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (authority, None),
    };
    Ok((normalize_host(host)?, port.unwrap_or(default_port)))
}

fn parse_port(value: &str) -> Result<u16, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("origin 端口必须由十进制数字组成".to_owned());
    }
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| "origin 端口必须在 1 到 65535 之间".to_owned())
}

fn normalize_host(value: &str) -> Result<String, String> {
    let original = value;
    let value = value.strip_suffix('.').unwrap_or(value);
    if value.is_empty() {
        return Err("origin 主机不能为空".to_owned());
    }
    if let Ok(address) = value.parse::<Ipv4Addr>() {
        return Ok(if original.ends_with('.') {
            format!("{address}.")
        } else {
            address.to_string()
        });
    }
    if value
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err("无效的 IPv4 主机".to_owned());
    }
    if value.len() > 253
        || !value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err("无效的 origin 主机名".to_owned());
    }
    // A DNS-equivalent spelling is still a distinct browser origin.
    Ok(original.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoEncoding {
    MjpegPassthrough,
    TranscodeJpeg,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum H264Encoder {
    #[default]
    Auto,
    RockchipMpp,
    V4l2M2m,
    Software,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct H264Config {
    /// Target bitrate for the optional WebRTC transport.
    pub bitrate_kbps: u32,
    /// Encoder preference. `Auto` only selects a detected hardware encoder;
    /// software encoding remains opt-in through `allow_software`.
    pub encoder: H264Encoder,
    pub allow_software: bool,
    pub ffmpeg_path: Option<PathBuf>,
    pub max_sessions: u8,
    /// Optional shared ICE UDP socket; independent of the HTTP TCP listener.
    pub udp_bind_address: Option<SocketAddr>,
    /// Public host:port forwarded to udp_bind_address, including DNAT port changes.
    pub public_udp_endpoint: Option<String>,
}

impl Default for H264Config {
    fn default() -> Self {
        Self {
            bitrate_kbps: 4_000,
            encoder: H264Encoder::Auto,
            allow_software: false,
            ffmpeg_path: None,
            max_sessions: 1,
            udp_bind_address: None,
            public_udp_endpoint: None,
        }
    }
}

impl H264Config {
    pub fn validate_udp_mapping(&self) -> Result<(), &'static str> {
        if self
            .udp_bind_address
            .is_some_and(|address| address.port() == 0)
        {
            return Err("WebRTC 固定 UDP 监听端口不能为 0");
        }
        let Some(endpoint) = self.public_udp_endpoint.as_deref() else {
            return Ok(());
        };
        if self.udp_bind_address.is_none() {
            return Err("WebRTC 公网 UDP 地址需要同时配置固定 UDP 监听地址");
        }
        let (host, port) = endpoint
            .rsplit_once(':')
            .ok_or("WebRTC 公网 UDP 地址必须是 host:port")?;
        if port.parse::<u16>().ok().is_none_or(|port| port == 0) {
            return Err("WebRTC 公网 UDP 端口必须在 1 到 65535 之间");
        }
        if let Ok(address) = endpoint.parse::<SocketAddr>() {
            if address.ip().is_unspecified() || address.ip().is_multicast() {
                return Err("WebRTC 公网 UDP 地址不能是未指定或组播地址");
            }
            return Ok(());
        }
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty()
            || host.len() > 253
            || !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err("WebRTC 公网 UDP 地址必须使用有效的 IP 或域名");
        }
        Ok(())
    }
}

/// Lowest and highest Opus bitrate accepted for HDMI audio.
pub const AUDIO_BITRATE_RANGE: std::ops::RangeInclusive<u32> = 16..=256;
const MAX_AUDIO_DEVICE_LEN: usize = 96;

/// Optional HDMI audio forwarded over WebRTC (issue #71).
///
/// Audio is a separate top-level section so enabling, disabling or a failing
/// audio device never counts as a video change: saving it does not restart
/// the capture pipeline or any H.264 session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Off by default; MJPEG never carries audio.
    pub enabled: bool,
    /// ALSA capture PCM such as `plughw:CARD=MS2109,DEV=0`. `None` selects
    /// the sound card that belongs to the configured capture card.
    pub device: Option<String>,
    /// Opus target bitrate.
    pub bitrate_kbps: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device: None,
            bitrate_kbps: 96,
        }
    }
}

impl AudioConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !AUDIO_BITRATE_RANGE.contains(&self.bitrate_kbps) {
            return Err("音频码率必须在 16 到 256 Kbps 之间");
        }
        if let Some(device) = self.device.as_deref() {
            validate_alsa_device_name(device)?;
        }
        Ok(())
    }
}

/// ALSA PCM names are passed to FFmpeg as one argument. Restrict them to the
/// hardware capture plugins and to the characters real names use, so a value
/// can never look like an option, a path, a URL or a file-writing plugin.
pub fn validate_alsa_device_name(device: &str) -> Result<(), &'static str> {
    const PLUGINS: [&str; 5] = ["hw", "plughw", "dsnoop", "sysdefault", "default"];
    let plugin = device.split_once(':').map_or(device, |(plugin, _)| plugin);
    if device.is_empty()
        || device.len() > MAX_AUDIO_DEVICE_LEN
        || !PLUGINS.contains(&plugin)
        || !device
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_:,=.-".contains(&byte))
    {
        return Err("音频设备必须是 ALSA 设备名，例如 plughw:CARD=MS2109,DEV=0");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub auto_detect: bool,
    pub device: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frames_per_second: Option<u32>,
    pub encoding: VideoEncoding,
    pub jpeg_quality: u8,
    pub h264: H264Config,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            device: None,
            width: None,
            height: None,
            frames_per_second: None,
            encoding: VideoEncoding::MjpegPassthrough,
            jpeg_quality: 80,
            h264: H264Config::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HidConfig {
    pub auto_detect: bool,
    pub keyboard_device: Option<PathBuf>,
    pub mouse_device: Option<PathBuf>,
    pub absolute_pointer_device: Option<PathBuf>,
    pub pointer_mode: PointerMode,
}

impl Default for HidConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            keyboard_device: None,
            mouse_device: None,
            absolute_pointer_device: None,
            pointer_mode: PointerMode::Auto,
        }
    }
}

/// Compatibility view for configuration files written before issue #20.
/// The retired timing keys are consumed so their presence can be reported.
/// They are never retained in the active [`HidConfig`].
#[derive(Deserialize)]
#[serde(default)]
struct HidConfigFile {
    auto_detect: bool,
    keyboard_device: Option<PathBuf>,
    mouse_device: Option<PathBuf>,
    absolute_pointer_device: Option<PathBuf>,
    pointer_mode: PointerMode,
    #[serde(default, deserialize_with = "deserialize_deprecated_hid_timing")]
    write_timeout_ms: Option<serde::de::IgnoredAny>,
    #[serde(default, deserialize_with = "deserialize_deprecated_hid_timing")]
    retry_interval_ms: Option<serde::de::IgnoredAny>,
}

impl Default for HidConfigFile {
    fn default() -> Self {
        let config = HidConfig::default();
        Self {
            auto_detect: config.auto_detect,
            keyboard_device: config.keyboard_device,
            mouse_device: config.mouse_device,
            absolute_pointer_device: config.absolute_pointer_device,
            pointer_mode: config.pointer_mode,
            write_timeout_ms: None,
            retry_interval_ms: None,
        }
    }
}

fn deserialize_deprecated_hid_timing<'de, D>(
    deserializer: D,
) -> Result<Option<serde::de::IgnoredAny>, D::Error>
where
    D: Deserializer<'de>,
{
    // An absent key falls back to `None` from the default above; this runs
    // only when the key is present, including an explicit JSON `null`.
    serde::de::IgnoredAny::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for HidConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let file = HidConfigFile::deserialize(deserializer)?;
        for (field, present) in [
            ("hid.write_timeout_ms", file.write_timeout_ms.is_some()),
            ("hid.retry_interval_ms", file.retry_interval_ms.is_some()),
        ] {
            if present {
                tracing::warn!(
                    field,
                    "deprecated HID timing option ignored: HID reports use a fixed 500 ms per-report deadline with readiness-based retry, not polling"
                );
            }
        }
        Ok(Self {
            auto_detect: file.auto_detect,
            keyboard_device: file.keyboard_device,
            mouse_device: file.mouse_device,
            absolute_pointer_device: file.absolute_pointer_device,
            pointer_mode: file.pointer_mode,
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerMode {
    #[default]
    Auto,
    Absolute,
    Relative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioPulseConfig {
    pub gpio_chip: Option<String>,
    pub gpio_line: Option<u32>,
    pub active_high: bool,
    pub pulse_ms: u64,
}

impl Default for GpioPulseConfig {
    fn default() -> Self {
        Self {
            gpio_chip: None,
            gpio_line: None,
            active_high: true,
            pulse_ms: 500,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpioBias {
    AsIs,
    Disabled,
    PullDown,
    #[default]
    PullUp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioInputConfig {
    pub gpio_chip: Option<String>,
    pub gpio_line: Option<u32>,
    pub active_low: bool,
    pub bias: GpioBias,
    pub poll_interval_ms: u64,
    pub debounce_ms: u64,
}

impl Default for GpioInputConfig {
    fn default() -> Self {
        Self {
            gpio_chip: None,
            gpio_line: None,
            active_low: true,
            bias: GpioBias::PullUp,
            poll_interval_ms: 1_000,
            debounce_ms: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    pub enabled: bool,
    pub auto_detect: bool,
    pub gpio_chip: Option<String>,
    pub gpio_line: Option<u32>,
    pub active_high: bool,
    pub short_press_ms: u64,
    pub long_press_ms: u64,
    /// Optional reset-switch output. `None` leaves the reset switch unconfigured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_switch: Option<GpioPulseConfig>,
    /// Optional power-LED sense input. `None` leaves power-state sensing disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_led: Option<GpioInputConfig>,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_detect: true,
            gpio_chip: None,
            gpio_line: None,
            active_high: true,
            short_press_ms: 500,
            long_press_ms: 5_000,
            reset_switch: None,
            power_led: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub enabled: bool,
    pub auto_detect: bool,
    #[serde(alias = "lun_file")]
    pub lun_path: Option<PathBuf>,
    pub image_directory: Option<PathBuf>,
    pub max_upload_bytes: u64,
    pub read_only_by_default: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_detect: true,
            lun_path: None,
            image_directory: None,
            max_upload_bytes: 16 * 1024 * 1024 * 1024,
            read_only_by_default: false,
        }
    }
}

pub const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";

/// Outbound event notifications (issue #84).
///
/// Secret values (`webhook.url`, `ntfy.token`, `telegram.bot_token`) are
/// persisted in the private configuration file but never returned to the
/// browser; see [`NotificationsConfig::redact_json`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub webhook: WebhookChannelConfig,
    pub ntfy: NtfyChannelConfig,
    pub telegram: TelegramChannelConfig,
    pub events: NotificationEvents,
    /// Keyboard/mouse failures within the window, with no success, before alerting.
    pub hid_failure_threshold: u32,
    pub hid_failure_window_seconds: u64,
    /// Video must stay lost this long before a notification is sent.
    pub video_lost_seconds: u64,
    /// At most one message per event type per this many seconds; repeats are merged.
    pub rate_limit_seconds: u64,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            webhook: WebhookChannelConfig::default(),
            ntfy: NtfyChannelConfig::default(),
            telegram: TelegramChannelConfig::default(),
            events: NotificationEvents::default(),
            hid_failure_threshold: 5,
            hid_failure_window_seconds: 60,
            video_lost_seconds: 30,
            rate_limit_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookChannelConfig {
    pub enabled: bool,
    /// Treated as a secret: chat-service webhook URLs embed their credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NtfyChannelConfig {
    pub enabled: bool,
    pub server: String,
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Default for NtfyChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server: DEFAULT_NTFY_SERVER.to_owned(),
            topic: None,
            token: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramChannelConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationEvents {
    pub login_succeeded: bool,
    pub login_failed: bool,
    pub power_led_changed: bool,
    pub hid_failing: bool,
    pub video_lost: bool,
}

impl Default for NotificationEvents {
    fn default() -> Self {
        Self {
            login_succeeded: true,
            login_failed: true,
            power_led_changed: true,
            hid_failing: true,
            video_lost: true,
        }
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty())
}

/// Accept only absolute http(s) URLs with a host.
pub fn validate_http_url(value: &str) -> Result<reqwest::Url, &'static str> {
    if value.chars().any(|character| character.is_whitespace() || character.is_control()) {
        return Err("URL 不能包含空白或控制字符");
    }
    let url = reqwest::Url::parse(value).map_err(|_| "URL 格式无效")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("URL 只允许 http 或 https");
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("URL 缺少主机名");
    }
    Ok(url)
}

impl NotificationsConfig {
    pub fn webhook_url(&self) -> Option<&str> {
        non_empty(&self.webhook.url)
    }

    pub fn ntfy_topic(&self) -> Option<&str> {
        non_empty(&self.ntfy.topic)
    }

    pub fn ntfy_token(&self) -> Option<&str> {
        non_empty(&self.ntfy.token)
    }

    pub fn telegram_bot_token(&self) -> Option<&str> {
        non_empty(&self.telegram.bot_token)
    }

    pub fn telegram_chat_id(&self) -> Option<&str> {
        non_empty(&self.telegram.chat_id)
    }

    pub fn webhook_ready(&self) -> bool {
        self.webhook_url().is_some()
    }

    pub fn ntfy_ready(&self) -> bool {
        self.ntfy_topic().is_some() && !self.ntfy.server.trim().is_empty()
    }

    pub fn telegram_ready(&self) -> bool {
        self.telegram_bot_token().is_some() && self.telegram_chat_id().is_some()
    }

    /// Keep previously stored secrets when an update leaves them empty.
    /// Clearing a secret is an explicit, separate action.
    pub fn inherit_secrets(&mut self, previous: &Self) {
        for (current, previous) in [
            (&mut self.webhook.url, &previous.webhook.url),
            (&mut self.ntfy.token, &previous.ntfy.token),
            (&mut self.telegram.bot_token, &previous.telegram.bot_token),
        ] {
            if non_empty(current).is_none() {
                current.clone_from(previous);
            }
        }
    }

    /// Replace secret values in a serialized configuration with
    /// `*_configured` flags. `value` is the `notifications` object.
    pub fn redact_json(&self, value: &mut serde_json::Value) {
        let flags = [
            ("webhook", "url", self.webhook_url().is_some()),
            ("ntfy", "token", self.ntfy_token().is_some()),
            ("telegram", "bot_token", self.telegram_bot_token().is_some()),
        ];
        for (section, key, configured) in flags {
            if let Some(object) = value.get_mut(section).and_then(serde_json::Value::as_object_mut) {
                object.remove(key);
                object.insert(format!("{key}_configured"), serde_json::Value::Bool(configured));
            }
        }
        // Show only the webhook origin so the operator can recognise it.
        if let Some(webhook) = value.get_mut("webhook").and_then(serde_json::Value::as_object_mut) {
            let hint = self
                .webhook_url()
                .and_then(|url| reqwest::Url::parse(url).ok())
                .map(|url| url.origin().ascii_serialization());
            webhook.insert("url_hint".to_owned(), hint.into());
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(url) = self.webhook_url() {
            validate_http_url(url).map_err(|_| "Webhook URL 必须是有效的 http/https 地址")?;
        }
        if self.webhook.enabled && !self.webhook_ready() {
            return Err("启用 Webhook 前必须填写 URL");
        }
        if !self.ntfy.server.trim().is_empty() {
            let url = validate_http_url(self.ntfy.server.trim())
                .map_err(|_| "ntfy 服务器必须是有效的 http/https 地址")?;
            if url.query().is_some() || url.fragment().is_some() {
                return Err("ntfy 服务器地址不能包含查询或片段");
            }
        }
        if let Some(topic) = self.ntfy_topic() {
            if topic.len() > 64
                || !topic
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err("ntfy 主题只能包含 1–64 位字母、数字、短横线或下划线");
            }
        }
        if let Some(token) = self.ntfy_token() {
            if token.len() > 256 || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Err("ntfy 访问令牌只能包含可见 ASCII 字符");
            }
        }
        if self.ntfy.enabled && !self.ntfy_ready() {
            return Err("启用 ntfy 前必须填写服务器和主题");
        }
        if let Some(token) = self.telegram_bot_token() {
            let valid = token.split_once(':').is_some_and(|(id, secret)| {
                !id.is_empty()
                    && id.bytes().all(|byte| byte.is_ascii_digit())
                    && !secret.is_empty()
                    && secret.len() <= 128
                    && secret
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            });
            if !valid {
                return Err("Telegram Bot Token 格式无效（应为 数字:字符串）");
            }
        }
        if let Some(chat) = self.telegram_chat_id() {
            let digits = chat.strip_prefix('-').unwrap_or(chat);
            let numeric = !digits.is_empty() && digits.len() <= 20 && digits.bytes().all(|b| b.is_ascii_digit());
            let channel = chat.strip_prefix('@').is_some_and(|name| {
                (5..=32).contains(&name.len())
                    && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            });
            if !numeric && !channel {
                return Err("Telegram Chat ID 必须是数字 ID 或 @频道名");
            }
        }
        if self.telegram.enabled && !self.telegram_ready() {
            return Err("启用 Telegram 前必须填写 Bot Token 和 Chat ID");
        }
        if !(1..=1_000).contains(&self.hid_failure_threshold) {
            return Err("键鼠失败次数阈值必须在 1 到 1000 之间");
        }
        if !(5..=3_600).contains(&self.hid_failure_window_seconds) {
            return Err("键鼠失败统计窗口必须在 5 到 3600 秒之间");
        }
        if !(5..=3_600).contains(&self.video_lost_seconds) {
            return Err("视频丢失通知延迟必须在 5 到 3600 秒之间");
        }
        if !(10..=3_600).contains(&self.rate_limit_seconds) {
            return Err("通知限流间隔必须在 10 到 3600 秒之间");
        }
        Ok(())
    }
}

impl Config {
    /// The configuration as returned to the browser: identical to the stored
    /// form except that notification secrets become `*_configured` flags.
    pub fn redacted_json(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        if let Some(notifications) = value.get_mut("notifications") {
            self.notifications.redact_json(notifications);
        }
        value
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid JSON configuration in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize configuration: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("unsupported configuration version {found}; this build supports version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("invalid server.public_origin {value:?}: {reason}")]
    InvalidPublicOrigin { value: String, reason: String },
    #[error("invalid server.listen_address {value:?}: expected a bare IPv4 or IPv6 address without brackets or whitespace")]
    InvalidListenAddress { value: String },
    #[error("invalid server.port {port}: expected a fixed port between 1 and 65535")]
    InvalidServerPort { port: u16 },
    #[error("failed to obtain secure randomness for an atomic write: {0}")]
    Randomness(String),
}

pub fn state_dir() -> PathBuf {
    resolve_state_dir(env::var_os(STATE_DIR_ENV))
}

pub fn default_config_path() -> PathBuf {
    state_dir().join(CONFIG_FILE_NAME)
}

fn resolve_state_dir(value: Option<OsString>) -> PathBuf {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_DIR))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
        path: parent.to_owned(),
        source,
    })?;

    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).map_err(|error| ConfigError::Randomness(error.to_string()))?;
    let suffix = u64::from_ne_bytes(random);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CONFIG_FILE_NAME);
    let temporary_path = parent.join(format!(".{file_name}.{suffix:016x}.tmp"));

    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = options
            .open(&temporary_path)
            .map_err(|source| ConfigError::Io {
                path: temporary_path.clone(),
                source,
            })?;
        file.write_all(contents).map_err(|source| ConfigError::Io {
            path: temporary_path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| ConfigError::Io {
            path: temporary_path.clone(),
            source,
        })?;
        drop(file);

        #[cfg(unix)]
        fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o600)).map_err(
            |source| ConfigError::Io {
                path: temporary_path.clone(),
                source,
            },
        )?;

        fs::rename(&temporary_path, path).map_err(|source| ConfigError::Io {
            path: path.to_owned(),
            source,
        })?;
        sync_directory(parent)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn sync_directory(path: &Path) -> Result<(), ConfigError> {
    let directory = File::open(path).map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })?;
    directory.sync_all().map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_udp_mapping_validates_ports_and_public_endpoints() {
        let mut config = H264Config::default();
        assert!(config.validate_udp_mapping().is_ok());
        config.public_udp_endpoint = Some("kvm.example.com:38319".into());
        assert!(config.validate_udp_mapping().is_err());
        config.udp_bind_address = Some("0.0.0.0:8080".parse::<std::net::SocketAddr>().unwrap());
        for endpoint in [
            "kvm.example.com:38319",
            "203.0.113.10:38319",
            "[2001:db8::1]:38319",
        ] {
            config.public_udp_endpoint = Some(endpoint.into());
            assert!(config.validate_udp_mapping().is_ok(), "{endpoint}");
        }
        for endpoint in [
            "kvm.example.com:0",
            "0.0.0.0:38319",
            "[ff02::1]:38319",
            "kvm.example.com:65536",
            "http://kvm.example.com:38319",
            "user@kvm.example.com:38319",
            "bad host:38319",
            "2001:db8::1:38319",
        ] {
            config.public_udp_endpoint = Some(endpoint.into());
            assert!(config.validate_udp_mapping().is_err(), "{endpoint}");
        }
        config.public_udp_endpoint = None;
        config.udp_bind_address = Some("0.0.0.0:0".parse().unwrap());
        assert!(config.validate_udp_mapping().is_err());
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let mut random = [0_u8; 8];
            getrandom::fill(&mut random).unwrap();
            let path = env::temp_dir().join(format!(
                "wingmankvm-config-test-{}-{:016x}",
                std::process::id(),
                u64::from_ne_bytes(random)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_hardware_devices_are_not_hard_coded() {
        let config = Config::default();
        assert!(config.video.auto_detect);
        assert_eq!(config.video.device, None);
        assert_eq!(config.hid.keyboard_device, None);
        assert_eq!(config.hid.mouse_device, None);
        assert_eq!(config.hid.absolute_pointer_device, None);
        assert_eq!(config.hid.pointer_mode, PointerMode::Auto);
        assert_eq!(config.power.gpio_chip, None);
        assert_eq!(config.power.gpio_line, None);
        assert_eq!(config.power.reset_switch, None);
        assert_eq!(config.power.power_led, None);
        assert_eq!(config.media.lun_path, None);
        assert!(!config.media.read_only_by_default);
    }

    #[test]
    fn legacy_display_settings_are_discarded_without_changing_capture() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        for display in [
            serde_json::Value::Null,
            serde_json::json!({
                "virtual_monitor": "hd720p60",
                "control_device": "/dev/hidraw0"
            }),
        ] {
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({
                    "version": CONFIG_VERSION,
                    "display": display,
                    "video": {
                        "device": "/dev/video5",
                        "follow_display": true,
                        "width": 1920,
                        "height": 1080,
                        "frames_per_second": 30
                    }
                }))
                .unwrap(),
            )
            .unwrap();

            let config = Config::load(&path).unwrap();
            assert_eq!(config.video.device, Some(PathBuf::from("/dev/video5")));
            assert_eq!(config.video.width, Some(1920));
            assert_eq!(config.video.height, Some(1080));
            assert_eq!(config.video.frames_per_second, Some(30));
            config.save_atomic(&path).unwrap();
            let saved: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert!(saved.get("display").is_none());
            assert!(saved["video"].get("follow_display").is_none());
        }
    }

    #[test]
    fn saves_and_loads_a_versioned_configuration() {
        let directory = TestDirectory::new();
        let path = directory.0.join("nested/config.json");
        let mut expected = Config::default();
        expected.server.port = 9090;
        expected.video.width = Some(1920);
        expected.video.height = Some(1080);

        expected.save_atomic(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), expected);
        assert!(fs::read_to_string(path).unwrap().ends_with('\n'));
    }

    #[test]
    fn legacy_hid_configuration_defaults_to_automatic_pointer_mode() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "hid": {
                "auto_detect": false,
                "keyboard_device": "/dev/hidg0",
                "mouse_device": "/dev/hidg1"
            }
        }))
        .unwrap();

        assert_eq!(config.hid.pointer_mode, PointerMode::Auto);
        assert_eq!(config.hid.absolute_pointer_device, None);
        assert_eq!(config.hid.mouse_device, Some(PathBuf::from("/dev/hidg1")));
    }

    #[test]
    fn deprecated_hid_timing_fields_are_accepted_and_ignored() {
        for (write_timeout_ms, retry_interval_ms) in [
            (serde_json::json!(500), serde_json::json!(10)),
            (serde_json::json!(2_500), serde_json::json!(0)),
            (serde_json::Value::Null, serde_json::Value::Null),
        ] {
            let config: Config = serde_json::from_value(serde_json::json!({
                "version": CONFIG_VERSION,
                "hid": {
                    "auto_detect": false,
                    "keyboard_device": "/dev/hidg0",
                    "mouse_device": "/dev/hidg1",
                    "absolute_pointer_device": "/dev/hidg2",
                    "pointer_mode": "relative",
                    "write_timeout_ms": write_timeout_ms,
                    "retry_interval_ms": retry_interval_ms
                }
            }))
            .unwrap();

            assert_eq!(
                config.hid,
                HidConfig {
                    auto_detect: false,
                    keyboard_device: Some(PathBuf::from("/dev/hidg0")),
                    mouse_device: Some(PathBuf::from("/dev/hidg1")),
                    absolute_pointer_device: Some(PathBuf::from("/dev/hidg2")),
                    pointer_mode: PointerMode::Relative,
                }
            );
        }
    }

    #[test]
    fn deprecated_hid_timing_fields_are_dropped_when_configuration_is_saved() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": CONFIG_VERSION,
                "hid": {
                    "auto_detect": false,
                    "keyboard_device": "/dev/hidg0",
                    "write_timeout_ms": 900,
                    "retry_interval_ms": 1
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.hid.keyboard_device, Some(PathBuf::from("/dev/hidg0")));
        config.save_atomic(&path).unwrap();

        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let hid = saved.get("hid").expect("hid section");
        assert!(hid.get("write_timeout_ms").is_none());
        assert!(hid.get("retry_interval_ms").is_none());
        assert_eq!(hid.get("keyboard_device").unwrap(), "/dev/hidg0");
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn default_configuration_roundtrips_without_hid_timing_fields() {
        let config = Config::default();
        let value = serde_json::to_value(&config).unwrap();
        let hid = value.get("hid").expect("hid section");
        assert!(hid.get("write_timeout_ms").is_none());
        assert!(hid.get("retry_interval_ms").is_none());
        assert_eq!(serde_json::from_value::<Config>(value).unwrap(), config);
    }

    #[test]
    fn legacy_video_configuration_defaults_to_safe_h264_settings() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "video": {
                "device": "/dev/video5",
                "encoding": "mjpeg_passthrough"
            }
        }))
        .unwrap();

        assert_eq!(config.video.h264.bitrate_kbps, 4_000);
        assert_eq!(config.video.h264.encoder, H264Encoder::Auto);
        assert!(!config.video.h264.allow_software);
        assert_eq!(config.video.h264.max_sessions, 1);
    }

    #[test]
    fn audio_is_off_by_default_and_legacy_files_keep_it_off() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "video": { "device": "/dev/video0" }
        }))
        .unwrap();
        assert_eq!(config.audio, AudioConfig::default());
        assert!(!config.audio.enabled);
        assert!(config.audio.device.is_none());
        assert_eq!(config.audio.bitrate_kbps, 96);
        assert!(config.audio.validate().is_ok());

        let partial: AudioConfig =
            serde_json::from_value(serde_json::json!({ "enabled": true })).unwrap();
        assert!(partial.enabled);
        assert_eq!(partial.bitrate_kbps, 96);
    }

    #[test]
    fn audio_configuration_roundtrips_and_validates_device_names() {
        let config = Config {
            audio: AudioConfig {
                enabled: true,
                device: Some("plughw:CARD=MS2109,DEV=0".to_owned()),
                bitrate_kbps: 128,
            },
            ..Config::default()
        };
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.audio, config.audio);
        assert!(back.audio.validate().is_ok());

        for good in ["hw:1,0", "plughw:CARD=USB3.0,DEV=0", "default", "hw:CARD=Capture_Card"] {
            assert!(validate_alsa_device_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "-i",
            "hw:1,0 -f",
            "/dev/snd/pcmC1D0c",
            "file:secret",
            "hw:1;rm",
            &"a".repeat(97),
        ] {
            assert!(validate_alsa_device_name(bad).is_err(), "{bad:?}");
        }
        for bitrate in [0, 15, 257] {
            let audio = AudioConfig {
                bitrate_kbps: bitrate,
                ..AudioConfig::default()
            };
            assert!(audio.validate().is_err(), "{bitrate}");
        }
    }

    #[test]
    fn legacy_power_configuration_keeps_auxiliary_gpio_disabled() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "power": {
                "enabled": true,
                "gpio_chip": "gpiochip1",
                "gpio_line": 7,
                "active_high": true
            }
        }))
        .unwrap();

        assert!(config.power.enabled);
        assert_eq!(config.power.gpio_line, Some(7));
        assert_eq!(config.power.reset_switch, None);
        assert_eq!(config.power.power_led, None);
    }

    #[test]
    fn gpio_auxiliary_configuration_uses_stable_defaults() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "power": {
                "reset_switch": {
                    "gpio_chip": "gpiochip1",
                    "gpio_line": 10
                },
                "power_led": {
                    "gpio_chip": "gpiochip1",
                    "gpio_line": 12
                }
            }
        }))
        .unwrap();

        let reset = config.power.reset_switch.as_ref().unwrap();
        assert_eq!(reset.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(reset.gpio_line, Some(10));
        assert!(reset.active_high);
        assert_eq!(reset.pulse_ms, 500);

        let power_led = config.power.power_led.as_ref().unwrap();
        assert_eq!(power_led.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(power_led.gpio_line, Some(12));
        assert!(power_led.active_low);
        assert_eq!(power_led.bias, GpioBias::PullUp);
        assert_eq!(power_led.poll_interval_ms, 1_000);
        assert_eq!(power_led.debounce_ms, 50);
    }

    #[test]
    fn unset_auxiliary_gpio_is_omitted_when_serializing() {
        let value = serde_json::to_value(Config::default()).unwrap();
        let power = value.get("power").unwrap();
        assert!(power.get("reset_switch").is_none());
        assert!(power.get("power_led").is_none());
    }

    #[test]
    fn explicit_null_clears_auxiliary_gpio_configuration() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "power": {
                "reset_switch": null,
                "power_led": null
            }
        }))
        .unwrap();

        assert_eq!(config.power.reset_switch, None);
        assert_eq!(config.power.power_led, None);
    }

    #[test]
    fn pointer_mode_uses_stable_snake_case_values() {
        assert_eq!(
            serde_json::to_string(&PointerMode::Absolute).unwrap(),
            "\"absolute\""
        );
        assert_eq!(
            serde_json::from_str::<PointerMode>("\"relative\"").unwrap(),
            PointerMode::Relative
        );
    }

    #[test]
    fn load_or_default_only_ignores_a_missing_file() {
        let directory = TestDirectory::new();
        let missing = directory.0.join("missing.json");
        assert_eq!(Config::load_or_default(missing).unwrap(), Config::default());

        let invalid = directory.0.join("invalid.json");
        fs::write(&invalid, b"not json").unwrap();
        assert!(matches!(
            Config::load_or_default(invalid),
            Err(ConfigError::Json { .. })
        ));
    }

    #[test]
    fn rejects_an_unsupported_version() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "version": 99 })).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            Config::load(path),
            Err(ConfigError::UnsupportedVersion { found: 99, .. })
        ));
    }

    #[test]
    fn requires_an_explicit_version() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        fs::write(&path, b"{}").unwrap();
        assert!(matches!(Config::load(path), Err(ConfigError::Json { .. })));
    }

    #[test]
    fn resolves_state_directory_without_mutating_process_environment() {
        assert_eq!(resolve_state_dir(None), PathBuf::from(DEFAULT_STATE_DIR));
        assert_eq!(
            resolve_state_dir(Some(OsString::new())),
            PathBuf::from(DEFAULT_STATE_DIR)
        );
        assert_eq!(
            resolve_state_dir(Some(OsString::from("/tmp/wingmankvm-state"))),
            PathBuf::from("/tmp/wingmankvm-state")
        );
    }

    #[cfg(unix)]
    #[test]
    fn saved_configuration_is_private() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        Config::default().save_atomic(&path).unwrap();
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn server_config_defaults_and_proxy_protocol_deserialization() {
        let json = r#"{
            "version": 1,
            "server": {
                "listen_address": "127.0.0.1",
                "port": 9090,
                "proxy_protocol": "auto"
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.server.proxy_protocol, ProxyProtocolMode::Auto);
        assert_eq!(config.server.port, 9090);

        let json_omitted = r#"{ "version": 1 }"#;
        let config_omitted: Config = serde_json::from_str(json_omitted).unwrap();
        assert_eq!(config_omitted.server.proxy_protocol, ProxyProtocolMode::Disabled);
        assert!(config_omitted.server.trusted_http_proxies.is_empty());
    }

    #[test]
    fn trusted_http_proxies_require_explicit_ip_addresses() {
        let server: ServerConfig = serde_json::from_str(r#"{
            "trusted_http_proxies": ["192.168.1.2", "127.0.0.1", "::1"]
        }"#).unwrap();
        assert_eq!(server.trusted_http_proxies.len(), 3);
        for invalid in ["*", "0.0.0.0/0", "192.168.1.0/24", "proxy.local"] {
            let value = serde_json::json!({"trusted_http_proxies": [invalid]});
            assert!(serde_json::from_value::<ServerConfig>(value).is_err());
        }
    }

    #[test]
    fn public_origin_parser_normalizes_and_rejects_untrusted_values() {
        let origin = NormalizedOrigin::parse("HTTPS://KVM.Example.COM:443").unwrap();
        assert_eq!(origin.scheme, OriginScheme::Https);
        assert_eq!(origin.host, "kvm.example.com");
        assert_eq!(origin.port, 443);
        assert!(origin.host_matches("KVM.example.com"));
        assert!(origin.host_matches("kvm.example.com:443"));

        let origin = NormalizedOrigin::parse("http://[2001:DB8::1]:8080/").unwrap();
        assert_eq!(origin.scheme, OriginScheme::Http);
        assert_eq!(origin.host, "[2001:db8::1]");
        assert_eq!(origin.port, 8080);
        assert!(origin.host_matches("[2001:db8::1]:8080"));

        let origin = NormalizedOrigin::parse("http://127.0.0.1").unwrap();
        assert_eq!(origin.host, "127.0.0.1");
        assert_eq!(origin.port, 80);
        assert!(origin.host_matches("127.0.0.1:80"));

        let origin = NormalizedOrigin::parse("http://KVM.Example.COM:80").unwrap();
        assert_eq!(origin.host, "kvm.example.com");
        assert_eq!(origin.port, 80);
        assert!(origin.host_matches("kvm.example.com"));

        assert!(!origin.host_matches("kvm.example.com."));
        let dotted = NormalizedOrigin::parse("http://kvm.example.com.").unwrap();
        assert_ne!(dotted, origin);
        assert!(dotted.host_matches("KVM.EXAMPLE.COM.:80"));

        for invalid in [
            "",
            "null",
            "ftp://kvm.example.com",
            "kvm.example.com",
            "https://user@kvm.example.com",
            "https://kvm.example.com/path",
            "https://kvm.example.com/?query",
            "https://kvm.example.com/#fragment",
            "https://kvm.example.com:0",
            "https://kvm.example.com:65536",
            "https://kvm.example.com:not-a-port",
            "https://kvm.example.com:+443",
            "https://[2001:db8::1",
            "https://2001:db8::1",
            "https://kvm.example.com,https://other.example.com",
            "https://bad_host.example.com",
        ] {
            assert!(
                NormalizedOrigin::parse(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn public_origin_is_optional_and_roundtrips() {
        let config: Config =
            serde_json::from_value(serde_json::json!({"version": CONFIG_VERSION})).unwrap();
        assert_eq!(config.server.public_origin, None);

        let mut config = Config::default();
        config.server.public_origin = Some("https://kvm.example.com".to_owned());
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(
            value["server"]["public_origin"],
            serde_json::json!("https://kvm.example.com")
        );
        assert_eq!(serde_json::from_value::<Config>(value).unwrap(), config);

        let without: ServerConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(without.public_origin, None);
    }

    #[test]
    fn session_cookie_secure_uses_only_the_configured_public_origin() {
        let mut server = ServerConfig::default();
        assert!(!server.session_cookie_secure());

        for origin in ["http://kvm.example.com", "http://127.0.0.1:8080"] {
            server.public_origin = Some(origin.to_owned());
            assert!(!server.session_cookie_secure(), "{origin}");
        }

        for origin in [
            "https://kvm.example.com",
            "HTTPS://KVM.Example.COM:443",
            "https://127.0.0.1:8443",
            "https://[2001:db8::1]:443",
        ] {
            server.public_origin = Some(origin.to_owned());
            assert!(server.session_cookie_secure(), "{origin}");
        }

        // Persisted configuration rejects these values, but if one is present
        // in memory it must fail closed rather than silently dropping Secure.
        for origin in ["not-an-origin", "ftp://kvm.example.com", ""] {
            server.public_origin = Some(origin.to_owned());
            assert!(server.session_cookie_secure(), "{origin:?}");
        }
    }

    #[test]
    fn invalid_public_origin_fails_configuration_load_and_save() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": CONFIG_VERSION,
                "server": {"public_origin": "https://user@kvm.example.com"}
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            Config::load(&path),
            Err(ConfigError::InvalidPublicOrigin { .. })
        ));
        assert!(matches!(
            Config::load_or_default(&path),
            Err(ConfigError::InvalidPublicOrigin { .. })
        ));

        let mut config = Config::default();
        config.server.public_origin = Some("ftp://kvm.example.com".to_owned());
        assert!(matches!(
            config.save_atomic(&path),
            Err(ConfigError::InvalidPublicOrigin { .. })
        ));
    }

    #[test]
    fn server_socket_address_parses_ipv4_and_bare_ipv6() {
        let mut server = ServerConfig::default();
        assert_eq!(
            server.socket_address().unwrap(),
            "0.0.0.0:8080".parse::<std::net::SocketAddr>().unwrap()
        );

        server.listen_address = "127.0.0.1".to_owned();
        server.port = 9090;
        assert_eq!(
            server.socket_address().unwrap(),
            "127.0.0.1:9090".parse::<std::net::SocketAddr>().unwrap()
        );

        server.listen_address = "::".to_owned();
        server.port = 8080;
        assert_eq!(
            server.socket_address().unwrap(),
            "[::]:8080".parse::<std::net::SocketAddr>().unwrap()
        );

        server.listen_address = "2001:db8::1".to_owned();
        server.port = u16::MAX;
        assert_eq!(
            server.socket_address().unwrap(),
            "[2001:db8::1]:65535".parse::<std::net::SocketAddr>().unwrap()
        );
    }

    #[test]
    fn server_socket_address_rejects_invalid_addresses_and_port_zero() {
        let mut server = ServerConfig::default();
        for invalid in [
            "",
            "localhost",
            "kvm.example.com",
            "[::1]",
            "[127.0.0.1]",
            " 127.0.0.1",
            "127.0.0.1 ",
            "127.0.0.1:8080",
            "999.1.1.1",
            "::1%lo",
        ] {
            server.listen_address = invalid.to_owned();
            assert!(
                matches!(
                    server.socket_address(),
                    Err(ConfigError::InvalidListenAddress { .. })
                ),
                "unexpectedly accepted {invalid:?}"
            );
        }

        server.listen_address = "127.0.0.1".to_owned();
        server.port = 0;
        assert!(matches!(
            server.socket_address(),
            Err(ConfigError::InvalidServerPort { port: 0 })
        ));
        server.port = u16::MAX;
        assert!(server.socket_address().is_ok());
    }

    #[test]
    fn ipv4_and_ipv6_listeners_roundtrip_through_save_and_load() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        for listen in ["127.0.0.1", "::1"] {
            let mut config = Config::default();
            config.server.listen_address = listen.to_owned();
            config.server.port = 1;
            config.save_atomic(&path).unwrap();
            let loaded = Config::load(&path).unwrap();
            assert_eq!(loaded.server.listen_address, listen);
            assert_eq!(loaded.server.socket_address().unwrap().port(), 1);

            config.server.port = u16::MAX;
            config.save_atomic(&path).unwrap();
            assert_eq!(Config::load(&path).unwrap().server.port, u16::MAX);
        }
    }

    #[test]
    fn invalid_server_listener_fails_load_and_save_without_overwriting() {
        let directory = TestDirectory::new();
        let path = directory.0.join("config.json");
        for (listen, port) in [("localhost", 8080), ("127.0.0.1", 0), ("[::1]", 8080)] {
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({
                    "version": CONFIG_VERSION,
                    "server": {"listen_address": listen, "port": port}
                }))
                .unwrap(),
            )
            .unwrap();
            assert!(matches!(
                Config::load(&path),
                Err(ConfigError::InvalidListenAddress { .. })
                    | Err(ConfigError::InvalidServerPort { .. })
            ));
        }

        let mut valid = Config::default();
        valid.server.listen_address = "127.0.0.1".to_owned();
        valid.server.port = 9090;
        valid.save_atomic(&path).unwrap();
        let before = fs::read(&path).unwrap();

        let mut invalid = valid.clone();
        invalid.server.listen_address = "localhost".to_owned();
        assert!(matches!(
            invalid.save_atomic(&path),
            Err(ConfigError::InvalidListenAddress { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), before);

        let mut invalid = valid;
        invalid.server.port = 0;
        assert!(matches!(
            invalid.save_atomic(&path),
            Err(ConfigError::InvalidServerPort { .. })
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    fn configured_notifications() -> NotificationsConfig {
        let mut notifications = NotificationsConfig::default();
        notifications.webhook = WebhookChannelConfig {
            enabled: true,
            url: Some("https://hooks.example.com/services/T000/SECRET-PATH".to_owned()),
        };
        notifications.ntfy = NtfyChannelConfig {
            enabled: true,
            server: "https://ntfy.example.com".to_owned(),
            topic: Some("wingman-alerts".to_owned()),
            token: Some("tk_ntfysecret".to_owned()),
        };
        notifications.telegram = TelegramChannelConfig {
            enabled: true,
            bot_token: Some("123456:ABC-telegram_secret".to_owned()),
            chat_id: Some("-1001234".to_owned()),
        };
        notifications
    }

    #[test]
    fn notification_defaults_are_disabled_and_legacy_files_load() {
        let config: Config = serde_json::from_value(serde_json::json!({"version": 1})).unwrap();
        assert_eq!(config.notifications, NotificationsConfig::default());
        assert!(!config.notifications.webhook.enabled);
        assert!(config.notifications.validate().is_ok());
    }

    #[test]
    fn notification_validation_rejects_bad_urls_and_incomplete_channels() {
        let valid = configured_notifications();
        assert!(valid.validate().is_ok());
        for url in ["ftp://example.com/x", "javascript:alert(1)", "https://", "not a url", "file:///etc/passwd"] {
            let mut config = valid.clone();
            config.webhook.url = Some(url.to_owned());
            assert!(config.validate().is_err(), "{url}");
        }
        let mut config = valid.clone();
        config.ntfy.server = "gopher://ntfy.example.com".to_owned();
        assert!(config.validate().is_err());
        let mut config = valid.clone();
        config.ntfy.topic = Some("bad topic/../x".to_owned());
        assert!(config.validate().is_err());
        let mut config = valid.clone();
        config.ntfy.token = Some("line\nbreak".to_owned());
        assert!(config.validate().is_err());
        let mut config = valid.clone();
        config.telegram.bot_token = Some("not-a-token".to_owned());
        assert!(config.validate().is_err());
        let mut config = valid.clone();
        config.telegram.chat_id = Some("abc".to_owned());
        assert!(config.validate().is_err());
        let mut config = valid.clone();
        config.telegram.chat_id = Some("@my_channel".to_owned());
        assert!(config.validate().is_ok());
        let mut config = valid.clone();
        config.webhook.url = None;
        assert!(config.validate().is_err(), "enabled webhook needs a URL");
        config.webhook.enabled = false;
        assert!(config.validate().is_ok());
        let mut config = valid.clone();
        config.rate_limit_seconds = 1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn redacted_configuration_never_contains_notification_secrets() {
        let mut config = Config::default();
        config.notifications = configured_notifications();
        let value = config.redacted_json();
        let text = value.to_string();
        for secret in ["SECRET-PATH", "tk_ntfysecret", "telegram_secret"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        let notifications = &value["notifications"];
        assert_eq!(notifications["webhook"]["url_configured"], true);
        assert_eq!(notifications["webhook"]["url_hint"], "https://hooks.example.com");
        assert_eq!(notifications["ntfy"]["token_configured"], true);
        assert_eq!(notifications["ntfy"]["topic"], "wingman-alerts");
        assert_eq!(notifications["telegram"]["bot_token_configured"], true);
        assert_eq!(notifications["telegram"]["chat_id"], "-1001234");

        // A browser that saves the redacted form back keeps every secret.
        let mut roundtrip: Config = serde_json::from_value(value).unwrap();
        assert!(roundtrip.notifications.webhook.url.is_none());
        roundtrip.notifications.inherit_secrets(&config.notifications);
        assert_eq!(roundtrip.notifications, config.notifications);

        let empty = Config::default().redacted_json();
        assert_eq!(empty["notifications"]["telegram"]["bot_token_configured"], false);
        assert!(empty["notifications"]["webhook"]["url_hint"].is_null());
    }

    #[test]
    fn notification_secrets_persist_in_the_private_file() {
        let directory = TestDirectory::new();
        let path = directory.0.join(CONFIG_FILE_NAME);
        let mut config = Config::default();
        config.notifications = configured_notifications();
        config.save_atomic(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().notifications, config.notifications);
    }
}
