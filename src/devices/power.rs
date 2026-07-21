use std::{path::PathBuf, process::Stdio, time::Duration};

use serde::Serialize;
use thiserror::Error;
use tokio::sync::{RwLock, mpsc, oneshot};

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

#[derive(Debug, Clone, Copy)]
pub enum PowerPress {
    Short,
    Long,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerStatus {
    pub running: bool,
    pub last_pid: Option<u32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Error)]
pub enum PowerError {
    #[error("power command queue is full")]
    QueueFull,
    #[error("power worker stopped")]
    WorkerStopped,
    #[error("failed to start gpioset: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("gpioset 2.x is required for timed power pulses: {0}")]
    UnsupportedVersion(String),
}

#[derive(Clone)]
pub struct PowerManager {
    tx: mpsc::Sender<PowerCommand>,
    status: std::sync::Arc<RwLock<PowerStatus>>,
}

struct PowerCommand {
    config: PowerConfigSnapshot,
    press: PowerPress,
    reply: oneshot::Sender<Result<u32, PowerError>>,
}

impl PowerManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(4);
        let status = std::sync::Arc::new(RwLock::new(PowerStatus::default()));
        tokio::spawn(power_worker(rx, status.clone()));
        Self { tx, status }
    }

    pub async fn press(
        &self,
        config: PowerConfigSnapshot,
        press: PowerPress,
    ) -> Result<u32, PowerError> {
        let (reply, response) = oneshot::channel();
        self.tx
            .try_send(PowerCommand {
                config,
                press,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => PowerError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => PowerError::WorkerStopped,
            })?;
        response.await.map_err(|_| PowerError::WorkerStopped)?
    }

    pub async fn status(&self) -> PowerStatus {
        self.status.read().await.clone()
    }
}

async fn power_worker(
    mut rx: mpsc::Receiver<PowerCommand>,
    status: std::sync::Arc<RwLock<PowerStatus>>,
) {
    while let Some(command) = rx.recv().await {
        if let Err(error) = verify_gpioset_version(&command.config.program).await {
            status.write().await.last_error = Some(error.to_string());
            let _ = command.reply.send(Err(error));
            continue;
        }
        let duration_ms = match command.press {
            PowerPress::Short => command.config.short_press_ms,
            PowerPress::Long => command.config.long_press_ms,
        };
        let active = u8::from(command.config.active_high);
        let assignment = format!("{}={active}", command.config.line);
        let toggle = format!("{duration_ms}ms,0");
        let child = tokio::process::Command::new(&command.config.program)
            .args(["-c", &command.config.chip, "-t", &toggle, &assignment])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();

        let child = match child {
            Ok(child) => child,
            Err(error) => {
                status.write().await.last_error = Some(error.to_string());
                let _ = command.reply.send(Err(PowerError::Spawn(error)));
                continue;
            }
        };
        let pid = child.id().unwrap_or_default();
        {
            let mut current = status.write().await;
            current.running = true;
            current.last_pid = Some(pid);
            current.last_error = None;
        }
        let _ = command.reply.send(Ok(pid));

        match child.wait_with_output().await {
            Ok(output) if output.status.success() => {
                status.write().await.last_error = None;
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                status.write().await.last_error = Some(if stderr.is_empty() {
                    format!("gpioset exited with {}", output.status)
                } else {
                    stderr
                });
            }
            Err(error) => status.write().await.last_error = Some(error.to_string()),
        }
        status.write().await.running = false;
        tokio::time::sleep(Duration::from_millis(command.config.cooldown_ms)).await;
    }
}

async fn verify_gpioset_version(program: &std::path::Path) -> Result<(), PowerError> {
    let output = tokio::process::Command::new(program)
        .arg("--version")
        .output()
        .await
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

fn is_gpioset_v2(version: &str) -> bool {
    version
        .split_whitespace()
        .any(|part| part.trim_start_matches('v').starts_with("2."))
}

#[cfg(test)]
mod tests {
    use super::is_gpioset_v2;

    #[test]
    fn recognizes_libgpiod_two_cli() {
        assert!(is_gpioset_v2("gpioset (libgpiod) v2.1.3"));
        assert!(!is_gpioset_v2("gpioset (libgpiod) v1.6.4"));
    }
}
