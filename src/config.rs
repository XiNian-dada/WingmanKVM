use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
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
            video: VideoConfig::default(),
            hid: HidConfig::default(),
            power: PowerConfig::default(),
            media: MediaConfig::default(),
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
pub struct PowerConfig {
    pub enabled: bool,
    pub auto_detect: bool,
    pub gpio_chip: Option<String>,
    pub gpio_line: Option<u32>,
    pub active_high: bool,
    pub short_press_ms: u64,
    pub long_press_ms: u64,
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
            read_only_by_default: true,
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
        assert!(config.video.auto_detect);
        assert_eq!(config.video.device, None);
        assert_eq!(config.hid.keyboard_device, None);
        assert_eq!(config.hid.mouse_device, None);
        assert_eq!(config.hid.absolute_pointer_device, None);
        assert_eq!(config.hid.pointer_mode, PointerMode::Auto);
        assert_eq!(config.power.gpio_chip, None);
        assert_eq!(config.power.gpio_line, None);
        assert_eq!(config.media.lun_path, None);
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
