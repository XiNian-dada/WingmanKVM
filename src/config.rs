use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub const CONFIG_VERSION: u32 = 1;
pub const STATE_DIR_ENV: &str = "WINGMANKVM_STATE_DIR";
pub const DEFAULT_STATE_DIR: &str = "/var/lib/wingmankvm";
pub const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default, deserialize_with = "deserialize_default_on_null")]
    pub display: DisplayConfig,
    #[serde(default)]
    pub video: VideoConfig,
    #[serde(default)]
    pub hid: HidConfig,
    #[serde(default)]
    pub power: PowerConfig,
    #[serde(default)]
    pub media: MediaConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            server: ServerConfig::default(),
            display: DisplayConfig::default(),
            video: VideoConfig::default(),
            hid: HidConfig::default(),
            power: PowerConfig::default(),
            media: MediaConfig::default(),
        }
    }
}

fn deserialize_default_on_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualMonitorMode {
    /// Leave the capture card's EDID and HPD state untouched.
    #[default]
    Unmanaged,
    Hd1080p60,
    Hd720p60,
}

impl VirtualMonitorMode {
    pub fn timing(self) -> Option<(u32, u32, u32)> {
        match self {
            Self::Unmanaged => None,
            Self::Hd1080p60 => Some((1920, 1080, 60)),
            Self::Hd720p60 => Some((1280, 720, 60)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// EDID profile applied to volatile MS2130 RAM. `Unmanaged` is deliberately
    /// the default so upgrading WingmanKVM never pulses HPD unexpectedly.
    pub virtual_monitor: VirtualMonitorMode,
    /// Optional explicit factory-HID node. When absent, the node must be
    /// discovered as a sibling of the selected V4L2 device.
    pub control_device: Option<PathBuf>,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            virtual_monitor: VirtualMonitorMode::Unmanaged,
            control_device: None,
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
        let config: Self = serde_json::from_slice(&bytes).map_err(|source| ConfigError::Json {
            path: path.to_owned(),
            source,
        })?;
        config.validate_version()?;
        Ok(config)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        match Self::load(path) {
            Ok(config) => Ok(config),
            Err(ConfigError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        self.validate_version()?;
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub listen_address: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".to_owned(),
            port: 8080,
        }
    }
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
}

impl Default for H264Config {
    fn default() -> Self {
        Self {
            bitrate_kbps: 4_000,
            encoder: H264Encoder::Auto,
            allow_software: false,
            ffmpeg_path: None,
            max_sessions: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub auto_detect: bool,
    pub device: Option<PathBuf>,
    /// Match the UVC capture format to the managed virtual-monitor mode.
    /// Existing configurations default to `false` and retain their old
    /// width/height semantics.
    pub follow_display: bool,
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
            follow_display: false,
            width: None,
            height: None,
            frames_per_second: None,
            encoding: VideoEncoding::MjpegPassthrough,
            jpeg_quality: 80,
            h264: H264Config::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HidConfig {
    pub auto_detect: bool,
    pub keyboard_device: Option<PathBuf>,
    pub mouse_device: Option<PathBuf>,
    pub absolute_pointer_device: Option<PathBuf>,
    pub pointer_mode: PointerMode,
    pub write_timeout_ms: u64,
    pub retry_interval_ms: u64,
}

impl Default for HidConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            keyboard_device: None,
            mouse_device: None,
            absolute_pointer_device: None,
            pointer_mode: PointerMode::Auto,
            write_timeout_ms: 500,
            retry_interval_ms: 10,
        }
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
        assert_eq!(
            config.display.virtual_monitor,
            VirtualMonitorMode::Unmanaged
        );
        assert_eq!(config.display.control_device, None);
        assert!(config.video.auto_detect);
        assert!(!config.video.follow_display);
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
    fn legacy_null_display_defaults_to_unmanaged() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "display": null,
            "video": {
                "device": "/dev/video5",
                "width": 1920,
                "height": 1080,
                "frames_per_second": 30
            }
        }))
        .unwrap();

        assert_eq!(
            config.display.virtual_monitor,
            VirtualMonitorMode::Unmanaged
        );
        assert_eq!(config.display.control_device, None);
        assert_eq!(config.video.device, Some(PathBuf::from("/dev/video5")));
        assert!(!config.video.follow_display);
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
        assert!(!config.video.follow_display);
        assert_eq!(
            config.display.virtual_monitor,
            VirtualMonitorMode::Unmanaged
        );
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
}
