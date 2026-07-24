use std::{
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Serialize;
use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::Child,
    sync::{Mutex, RwLock},
    time::timeout,
};

use crate::config::GpioBias;

const GPIO_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const GPIO_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// A fully resolved output pulse. Keeping this separate from the persisted
/// configuration means the worker never has to hold the configuration lock
/// while a GPIO process is running.
#[derive(Debug, Clone)]
pub struct GpioPulseConfigSnapshot {
    pub program: PathBuf,
    pub chip: String,
    pub line: u32,
    pub active_high: bool,
    pub pulse_ms: u64,
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PowerConfigSnapshot {
    pub program: PathBuf,
    pub chip: String,
    pub line: u32,
    pub active_high: bool,
    pub short_press_ms: u64,
    pub long_press_ms: u64,
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerLedConfigSnapshot {
    pub program: PathBuf,
    pub chip: String,
    pub line: u32,
    pub active_low: bool,
    pub bias: GpioBias,
    pub poll_interval_ms: u64,
    pub debounce_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum PowerPress {
    Short,
    Long,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PowerLedState {
    On,
    Off,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerLedStatus {
    pub configured: bool,
    pub state: PowerLedState,
    pub active: Option<bool>,
    pub sense_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerStatus {
    pub running: bool,
    pub last_pid: Option<u32>,
    pub last_error: Option<String>,
    pub power_led: PowerLedStatus,
}

#[derive(Debug, Error)]
pub enum PowerError {
    #[error("GPIO control is busy")]
    Busy,
    #[error("failed to start GPIO command: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("gpioset 2.x is required for timed power pulses: {0}")]
    UnsupportedVersion(String),
    #[error("GPIO command timed out")]
    Timeout,
    #[error("GPIO read failed: {0}")]
    Read(String),
}

#[derive(Clone)]
pub struct PowerManager {
    /// One gate covers power, reset, and output-test pulses. A rejected action
    /// is preferable to queuing a second pulse that could overlap the first.
    busy: Arc<Mutex<()>>,
    status: Arc<RwLock<PowerStatus>>,
    led_config: Arc<RwLock<Option<PowerLedConfigSnapshot>>>,
}

impl PowerManager {
    pub fn new() -> Self {
        let status = Arc::new(RwLock::new(PowerStatus::default()));
        let led_config = Arc::new(RwLock::new(None));
        tokio::spawn(power_led_worker(led_config.clone(), status.clone()));
        Self {
            busy: Arc::new(Mutex::new(())),
            status,
            led_config,
        }
    }

    pub async fn press(
        &self,
        config: PowerConfigSnapshot,
        press: PowerPress,
    ) -> Result<u32, PowerError> {
        let pulse_ms = match press {
            PowerPress::Short => config.short_press_ms,
            PowerPress::Long => config.long_press_ms,
        };
        self.pulse(GpioPulseConfigSnapshot {
            program: config.program,
            chip: config.chip,
            line: config.line,
            active_high: config.active_high,
            pulse_ms,
            cooldown_ms: config.cooldown_ms,
        })
        .await
    }

    pub async fn pulse(&self, config: GpioPulseConfigSnapshot) -> Result<u32, PowerError> {
        let gate = self
            .busy
            .clone()
            .try_lock_owned()
            .map_err(|_| PowerError::Busy)?;

        if let Err(error) = verify_gpioset_version(&config.program).await {
            self.status.write().await.last_error = Some(error.to_string());
            return Err(error);
        }

        let mut command = tokio::process::Command::new(&config.program);
        command
            .args(gpioset_args(&config))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let error = PowerError::Spawn(error);
                self.status.write().await.last_error = Some(error.to_string());
                return Err(error);
            }
        };
        let pid = child.id().unwrap_or_default();
        let stderr = child.stderr.take().map(read_stderr);
        {
            let mut current = self.status.write().await;
            current.running = true;
            current.last_pid = Some(pid);
            current.last_error = None;
        }

        let status = self.status.clone();
        tokio::spawn(async move {
            let _gate = gate;
            let result = wait_for_gpio_command(&mut child, stderr, config.pulse_ms).await;
            let mut current = status.write().await;
            current.running = false;
            if let Err(error) = result {
                current.last_error = Some(error.to_string());
            } else {
                current.last_error = None;
            }
            drop(current);
            if config.cooldown_ms > 0 {
                tokio::time::sleep(Duration::from_millis(config.cooldown_ms)).await;
            }
        });

        Ok(pid)
    }

    pub async fn set_power_led_config(&self, config: Option<PowerLedConfigSnapshot>) {
        *self.led_config.write().await = config;
        // Reset stale state immediately. The monitor will publish a fresh
        // reading on its next iteration.
        let configured = self.led_config.read().await.is_some();
        let mut status = self.status.write().await;
        status.power_led = if configured {
            PowerLedStatus {
                configured: true,
                ..PowerLedStatus::default()
            }
        } else {
            PowerLedStatus::default()
        };
    }

    pub async fn status(&self) -> PowerStatus {
        self.status.read().await.clone()
    }
}

impl Default for PowerManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn wait_for_gpio_command(
    child: &mut Child,
    stderr_task: Option<tokio::task::JoinHandle<Vec<u8>>>,
    pulse_ms: u64,
) -> Result<(), PowerError> {
    // A gpioset pulse should finish shortly after its requested duration. The
    // fixed upper bound protects us from a stuck helper even if configuration
    // is accidentally set to an unusually long value.
    let wait_for = Duration::from_millis(pulse_ms)
        .saturating_add(Duration::from_secs(5))
        .min(GPIO_COMMAND_TIMEOUT);
    let wait_result = timeout(wait_for, child.wait()).await;
    if wait_result.is_err() {
        // Kill and reap the child so a timed-out gpioset cannot retain the
        // GPIO line or linger as a zombie.
        let _ = child.kill().await;
        let _ = child.wait().await;
        if let Some(task) = stderr_task {
            let _ = task.await;
        }
        return Err(PowerError::Timeout);
    }

    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => Vec::new(),
    };
    match wait_result.expect("checked above") {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let message = String::from_utf8_lossy(&stderr).trim().to_owned();
            Err(PowerError::Read(if message.is_empty() {
                format!("gpioset exited with {status}")
            } else {
                message
            }))
        }
        Err(error) => Err(PowerError::Spawn(error)),
    }
}

fn read_stderr(mut stderr: tokio::process::ChildStderr) -> tokio::task::JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes).await;
        bytes
    })
}

async fn verify_gpioset_version(program: &std::path::Path) -> Result<(), PowerError> {
    let mut command = tokio::process::Command::new(program);
    command.arg("--version").kill_on_drop(true);
    let output = timeout(GPIO_READ_TIMEOUT, command.output())
        .await
        .map_err(|_| PowerError::Timeout)?
        .map_err(PowerError::Spawn)?;
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let is_v2 = output.status.success() && is_gpioset_v2(&version);
    if is_v2 {
        Ok(())
    } else {
        Err(PowerError::UnsupportedVersion(version.trim().to_string()))
    }
}

async fn power_led_worker(
    config: Arc<RwLock<Option<PowerLedConfigSnapshot>>>,
    status: Arc<RwLock<PowerStatus>>,
) {
    let mut previous_config: Option<PowerLedConfigSnapshot> = None;
    let mut candidate: Option<(bool, Instant)> = None;
    let mut stable: Option<bool> = None;

    loop {
        let snapshot = config.read().await.clone();
        let Some(snapshot) = snapshot else {
            previous_config = None;
            candidate = None;
            stable = None;
            status.write().await.power_led = PowerLedStatus::default();
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        };

        if previous_config.as_ref() != Some(&snapshot) {
            previous_config = Some(snapshot.clone());
            candidate = None;
            stable = None;
        }

        match read_power_led(&snapshot).await {
            Ok(active) => {
                let now = Instant::now();
                if stable == Some(active) {
                    candidate = None;
                } else if candidate.is_some_and(|(value, _)| value == active) {
                    let (_, since) = candidate.expect("candidate checked above");
                    if now.duration_since(since) >= Duration::from_millis(snapshot.debounce_ms) {
                        stable = Some(active);
                        candidate = None;
                    }
                } else if snapshot.debounce_ms == 0 {
                    stable = Some(active);
                    candidate = None;
                } else {
                    candidate = Some((active, now));
                }
                let mut current = status.write().await;
                current.power_led = PowerLedStatus {
                    configured: true,
                    state: stable.map_or(PowerLedState::Unknown, |value| {
                        if value {
                            PowerLedState::On
                        } else {
                            PowerLedState::Off
                        }
                    }),
                    active: stable,
                    sense_error: None,
                };
            }
            Err(error) => {
                candidate = None;
                stable = None;
                status.write().await.power_led = PowerLedStatus {
                    configured: true,
                    state: PowerLedState::Unknown,
                    active: None,
                    sense_error: Some(error.to_string()),
                };
            }
        }

        tokio::time::sleep(Duration::from_millis(
            snapshot.poll_interval_ms.clamp(100, 5_000),
        ))
        .await;
    }
}

async fn read_power_led(config: &PowerLedConfigSnapshot) -> Result<bool, PowerError> {
    let mut command = tokio::process::Command::new(&config.program);
    command.args(gpioget_args(config)).kill_on_drop(true);
    let output = timeout(GPIO_READ_TIMEOUT, command.output())
        .await
        .map_err(|_| PowerError::Timeout)?
        .map_err(PowerError::Spawn)?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(PowerError::Read(if error.is_empty() {
            format!("gpioget exited with {}", output.status)
        } else {
            error
        }));
    }
    parse_gpio_value(&output.stdout)
        .ok_or_else(|| PowerError::Read("gpioget returned a non-binary value".to_owned()))
}

fn gpioset_args(config: &GpioPulseConfigSnapshot) -> Vec<String> {
    let mut args = vec!["-c".to_owned(), config.chip.clone()];
    // The logical active value is always one. libgpiod performs the physical
    // inversion only when the circuit is configured as active-low.
    if !config.active_high {
        args.push("--active-low".to_owned());
    }
    args.extend([
        "-t".to_owned(),
        format!("{}ms,0", config.pulse_ms),
        format!("{}=1", config.line),
    ]);
    args
}

fn gpioget_args(config: &PowerLedConfigSnapshot) -> Vec<String> {
    let mut args = vec!["--numeric".to_owned(), "-c".to_owned(), config.chip.clone()];
    match config.bias {
        GpioBias::AsIs => {}
        GpioBias::Disabled => args.extend(["--bias".to_owned(), "disabled".to_owned()]),
        GpioBias::PullDown => args.extend(["--bias".to_owned(), "pull-down".to_owned()]),
        GpioBias::PullUp => args.extend(["--bias".to_owned(), "pull-up".to_owned()]),
    }
    if config.active_low {
        args.push("--active-low".to_owned());
    }
    args.push(config.line.to_string());
    args
}

fn parse_gpio_value(output: &[u8]) -> Option<bool> {
    let text = String::from_utf8_lossy(output);
    let value = text.split_whitespace().last()?.rsplit('=').next()?;
    match value {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn is_gpioset_v2(version: &str) -> bool {
    version
        .split_whitespace()
        .any(|part| part.trim_start_matches('v').starts_with("2."))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::GpioBias;

    use super::{
        GpioPulseConfigSnapshot, PowerLedConfigSnapshot, PowerStatus, gpioget_args, gpioset_args,
        is_gpioset_v2, parse_gpio_value,
    };

    #[test]
    fn recognizes_libgpiod_two_cli() {
        assert!(is_gpioset_v2("gpioset (libgpiod) v2.1.3"));
        assert!(!is_gpioset_v2("gpioset (libgpiod) v1.6.4"));
    }

    #[test]
    fn parses_numeric_gpio_values() {
        assert_eq!(parse_gpio_value(b"0\n"), Some(false));
        assert_eq!(parse_gpio_value(b"line=1\n"), Some(true));
        assert_eq!(parse_gpio_value(b"high\n"), None);
    }

    #[test]
    fn gpioset_uses_logical_one_and_only_adds_active_low_when_needed() {
        let mut config = GpioPulseConfigSnapshot {
            program: PathBuf::from("gpioset"),
            chip: "gpiochip1".to_owned(),
            line: 7,
            active_high: true,
            pulse_ms: 150,
            cooldown_ms: 0,
        };
        assert_eq!(
            gpioset_args(&config),
            ["-c", "gpiochip1", "-t", "150ms,0", "7=1"]
        );

        config.active_high = false;
        assert_eq!(
            gpioset_args(&config),
            ["-c", "gpiochip1", "--active-low", "-t", "150ms,0", "7=1"]
        );
    }

    #[test]
    fn gpioget_applies_input_bias_and_active_low() {
        let config = PowerLedConfigSnapshot {
            program: PathBuf::from("gpioget"),
            chip: "gpiochip1".to_owned(),
            line: 12,
            active_low: true,
            bias: GpioBias::PullUp,
            poll_interval_ms: 1_000,
            debounce_ms: 50,
        };
        assert_eq!(
            gpioget_args(&config),
            [
                "--numeric",
                "-c",
                "gpiochip1",
                "--bias",
                "pull-up",
                "--active-low",
                "12"
            ]
        );
    }

    #[test]
    fn unknown_power_led_status_is_explicit_in_json() {
        let status = serde_json::to_value(PowerStatus::default()).unwrap();
        assert_eq!(status["power_led"]["configured"], false);
        assert_eq!(status["power_led"]["state"], "unknown");
        assert!(status["power_led"]["active"].is_null());
        assert!(status["power_led"]["sense_error"].is_null());
    }
}
