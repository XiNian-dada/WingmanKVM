use std::{
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use thiserror::Error;
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, oneshot},
    time::Instant,
};

const WRITE_TIMEOUT: Duration = Duration::from_millis(500);
const QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Error)]
pub enum HidError {
    #[error("HID command queue is full")]
    QueueFull,
    #[error("HID worker stopped")]
    WorkerStopped,
    #[error("HID device is not configured")]
    NotConfigured,
    #[error("unsupported key code: {0}")]
    UnsupportedKey(String),
    #[error("invalid mouse button: {0}")]
    InvalidButton(u8),
    #[error("timed out waiting for {path} to become writable")]
    Timeout { path: PathBuf },
    #[error("short HID write to {path}: wrote {actual} of {expected} bytes")]
    ShortWrite {
        path: PathBuf,
        expected: usize,
        actual: usize,
    },
    #[error("failed to access HID device {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyRequest {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
    #[serde(default = "default_hold_ms")]
    pub hold_ms: u64,
}

fn default_hold_ms() -> u64 {
    25
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MouseMoveRequest {
    pub dx: i16,
    pub dy: i16,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MouseClickRequest {
    pub button: u8,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MouseScrollRequest {
    #[serde(alias = "wheel")]
    pub delta: i16,
}

#[derive(Clone)]
pub struct HidManager {
    keyboard_tx: mpsc::Sender<KeyboardCommand>,
    mouse_tx: mpsc::Sender<MouseCommand>,
}

enum KeyboardCommand {
    Key {
        path: PathBuf,
        request: KeyRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    Release {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
}

enum MouseCommand {
    Move {
        path: PathBuf,
        request: MouseMoveRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    Click {
        path: PathBuf,
        request: MouseClickRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    Scroll {
        path: PathBuf,
        request: MouseScrollRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    Release {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
}

impl HidManager {
    pub fn new() -> Self {
        let (keyboard_tx, keyboard_rx) = mpsc::channel(QUEUE_CAPACITY);
        let (mouse_tx, mouse_rx) = mpsc::channel(QUEUE_CAPACITY);
        tokio::spawn(keyboard_worker(keyboard_rx));
        tokio::spawn(mouse_worker(mouse_rx));
        Self {
            keyboard_tx,
            mouse_tx,
        }
    }

    pub async fn key(&self, path: Option<PathBuf>, request: KeyRequest) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.keyboard_tx
            .try_send(KeyboardCommand::Key {
                path,
                request,
                reply,
            })
            .map_err(map_send_error)?;
        response.await.map_err(|_| HidError::WorkerStopped)?
    }

    pub async fn mouse_move(
        &self,
        path: Option<PathBuf>,
        request: MouseMoveRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.mouse_tx
            .try_send(MouseCommand::Move {
                path,
                request,
                reply,
            })
            .map_err(map_send_error)?;
        response.await.map_err(|_| HidError::WorkerStopped)?
    }

    pub async fn mouse_click(
        &self,
        path: Option<PathBuf>,
        request: MouseClickRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.mouse_tx
            .try_send(MouseCommand::Click {
                path,
                request,
                reply,
            })
            .map_err(map_send_error)?;
        response.await.map_err(|_| HidError::WorkerStopped)?
    }

    pub async fn mouse_scroll(
        &self,
        path: Option<PathBuf>,
        request: MouseScrollRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.mouse_tx
            .try_send(MouseCommand::Scroll {
                path,
                request,
                reply,
            })
            .map_err(map_send_error)?;
        response.await.map_err(|_| HidError::WorkerStopped)?
    }

    pub async fn release_all(
        &self,
        keyboard_path: Option<PathBuf>,
        mouse_path: Option<PathBuf>,
    ) -> Result<(), HidError> {
        if let Some(path) = keyboard_path {
            let (reply, response) = oneshot::channel();
            self.keyboard_tx
                .try_send(KeyboardCommand::Release { path, reply })
                .map_err(map_send_error)?;
            response.await.map_err(|_| HidError::WorkerStopped)??;
        }
        if let Some(path) = mouse_path {
            let (reply, response) = oneshot::channel();
            self.mouse_tx
                .try_send(MouseCommand::Release { path, reply })
                .map_err(map_send_error)?;
            response.await.map_err(|_| HidError::WorkerStopped)??;
        }
        Ok(())
    }
}

fn map_send_error<T>(error: mpsc::error::TrySendError<T>) -> HidError {
    match error {
        mpsc::error::TrySendError::Full(_) => HidError::QueueFull,
        mpsc::error::TrySendError::Closed(_) => HidError::WorkerStopped,
    }
}

async fn keyboard_worker(mut rx: mpsc::Receiver<KeyboardCommand>) {
    while let Some(command) = rx.recv().await {
        match command {
            KeyboardCommand::Key {
                path,
                request,
                reply,
            } => {
                let result = send_key(&path, &request).await;
                let _ = reply.send(result);
            }
            KeyboardCommand::Release { path, reply } => {
                let result = send_report(&path, &[0; 8]).await;
                let _ = reply.send(result);
            }
        }
    }
}

async fn mouse_worker(mut rx: mpsc::Receiver<MouseCommand>) {
    while let Some(command) = rx.recv().await {
        match command {
            MouseCommand::Move {
                path,
                request,
                reply,
            } => {
                let result = send_mouse_move(&path, request).await;
                let _ = reply.send(result);
            }
            MouseCommand::Click {
                path,
                request,
                reply,
            } => {
                let result = send_mouse_click(&path, request).await;
                let _ = reply.send(result);
            }
            MouseCommand::Scroll {
                path,
                request,
                reply,
            } => {
                let result = send_mouse_scroll(&path, request).await;
                let _ = reply.send(result);
            }
            MouseCommand::Release { path, reply } => {
                let result = send_report(&path, &[0; 4]).await;
                let _ = reply.send(result);
            }
        }
    }
}

async fn send_key(path: &Path, request: &KeyRequest) -> Result<(), HidError> {
    let key_modifier = match request.key.as_str() {
        "Control" | "ControlLeft" | "ControlRight" => 0x01,
        "Shift" | "ShiftLeft" | "ShiftRight" => 0x02,
        "Alt" | "AltLeft" | "AltRight" => 0x04,
        "Meta" | "MetaLeft" | "MetaRight" => 0x08,
        _ => 0,
    };
    let keycode = if key_modifier == 0 {
        keycode(&request.key).ok_or_else(|| HidError::UnsupportedKey(request.key.to_string()))?
    } else {
        0
    };
    let modifier = key_modifier
        | u8::from(request.ctrl)
        | (u8::from(request.shift) << 1)
        | (u8::from(request.alt) << 2)
        | (u8::from(request.meta) << 3);
    let report = [modifier, 0, keycode, 0, 0, 0, 0, 0];

    send_report(path, &report).await?;
    tokio::time::sleep(Duration::from_millis(request.hold_ms.clamp(10, 500))).await;
    send_report(path, &[0; 8]).await
}

async fn send_mouse_move(path: &Path, request: MouseMoveRequest) -> Result<(), HidError> {
    let mut dx = request.dx.clamp(-4096, 4096);
    let mut dy = request.dy.clamp(-4096, 4096);
    while dx != 0 || dy != 0 {
        let step_x = dx.clamp(-127, 127) as i8;
        let step_y = dy.clamp(-127, 127) as i8;
        send_report(path, &[0, step_x as u8, step_y as u8, 0]).await?;
        dx -= i16::from(step_x);
        dy -= i16::from(step_y);
    }
    Ok(())
}

async fn send_mouse_click(path: &Path, request: MouseClickRequest) -> Result<(), HidError> {
    if !matches!(request.button, 1 | 2 | 4) {
        return Err(HidError::InvalidButton(request.button));
    }
    send_report(path, &[request.button, 0, 0, 0]).await?;
    send_report(path, &[0; 4]).await
}

async fn send_mouse_scroll(path: &Path, request: MouseScrollRequest) -> Result<(), HidError> {
    let mut delta = request.delta.clamp(-4096, 4096);
    while delta != 0 {
        let step = delta.clamp(-127, 127) as i8;
        send_report(path, &[0, 0, 0, step as u8]).await?;
        delta -= i16::from(step);
    }
    send_report(path, &[0; 4]).await
}

async fn send_report(path: &Path, report: &[u8]) -> Result<(), HidError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|source| HidError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let fd = AsyncFd::new(file).map_err(|source| HidError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let deadline = Instant::now() + WRITE_TIMEOUT;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HidError::Timeout {
                path: path.to_path_buf(),
            });
        }
        let mut guard = tokio::time::timeout(remaining, fd.writable())
            .await
            .map_err(|_| HidError::Timeout {
                path: path.to_path_buf(),
            })?
            .map_err(|source| HidError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        match guard.try_io(|inner| {
            let mut file = inner.get_ref();
            file.write(report)
        }) {
            Ok(Ok(actual)) if actual == report.len() => return Ok(()),
            Ok(Ok(actual)) => {
                return Err(HidError::ShortWrite {
                    path: path.to_path_buf(),
                    expected: report.len(),
                    actual,
                });
            }
            Ok(Err(source)) if source.kind() == io::ErrorKind::Interrupted => continue,
            Ok(Err(source)) => {
                return Err(HidError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
            Err(_) => continue,
        }
    }
}

pub fn keycode(code: &str) -> Option<u8> {
    Some(match code {
        "KeyA" | "A" | "a" => 0x04,
        "KeyB" | "B" | "b" => 0x05,
        "KeyC" | "C" | "c" => 0x06,
        "KeyD" | "D" | "d" => 0x07,
        "KeyE" | "E" | "e" => 0x08,
        "KeyF" | "F" | "f" => 0x09,
        "KeyG" | "G" | "g" => 0x0a,
        "KeyH" | "H" | "h" => 0x0b,
        "KeyI" | "I" | "i" => 0x0c,
        "KeyJ" | "J" | "j" => 0x0d,
        "KeyK" | "K" | "k" => 0x0e,
        "KeyL" | "L" | "l" => 0x0f,
        "KeyM" | "M" | "m" => 0x10,
        "KeyN" | "N" | "n" => 0x11,
        "KeyO" | "O" | "o" => 0x12,
        "KeyP" | "P" | "p" => 0x13,
        "KeyQ" | "Q" | "q" => 0x14,
        "KeyR" | "R" | "r" => 0x15,
        "KeyS" | "S" | "s" => 0x16,
        "KeyT" | "T" | "t" => 0x17,
        "KeyU" | "U" | "u" => 0x18,
        "KeyV" | "V" | "v" => 0x19,
        "KeyW" | "W" | "w" => 0x1a,
        "KeyX" | "X" | "x" => 0x1b,
        "KeyY" | "Y" | "y" => 0x1c,
        "KeyZ" | "Z" | "z" => 0x1d,
        "Digit1" | "1" => 0x1e,
        "Digit2" | "2" => 0x1f,
        "Digit3" | "3" => 0x20,
        "Digit4" | "4" => 0x21,
        "Digit5" | "5" => 0x22,
        "Digit6" | "6" => 0x23,
        "Digit7" | "7" => 0x24,
        "Digit8" | "8" => 0x25,
        "Digit9" | "9" => 0x26,
        "Digit0" | "0" => 0x27,
        "Enter" => 0x28,
        "Escape" | "Esc" => 0x29,
        "Backspace" => 0x2a,
        "Tab" => 0x2b,
        "Space" | "Spacebar" | " " => 0x2c,
        "Minus" | "-" => 0x2d,
        "Equal" | "=" => 0x2e,
        "BracketLeft" | "[" => 0x2f,
        "BracketRight" | "]" => 0x30,
        "Backslash" | "\\" => 0x31,
        "Semicolon" | ";" => 0x33,
        "Quote" | "'" => 0x34,
        "Backquote" | "`" => 0x35,
        "Comma" | "," => 0x36,
        "Period" | "." => 0x37,
        "Slash" | "/" => 0x38,
        "CapsLock" => 0x39,
        "F1" => 0x3a,
        "F2" => 0x3b,
        "F3" => 0x3c,
        "F4" => 0x3d,
        "F5" => 0x3e,
        "F6" => 0x3f,
        "F7" => 0x40,
        "F8" => 0x41,
        "F9" => 0x42,
        "F10" => 0x43,
        "F11" => 0x44,
        "F12" => 0x45,
        "PrintScreen" => 0x46,
        "ScrollLock" => 0x47,
        "Pause" => 0x48,
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PageUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PageDown" => 0x4e,
        "ArrowRight" => 0x4f,
        "ArrowLeft" => 0x50,
        "ArrowDown" => 0x51,
        "ArrowUp" => 0x52,
        "NumLock" => 0x53,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_browser_codes_and_function_keys() {
        assert_eq!(keycode("KeyA"), Some(0x04));
        assert_eq!(keycode("F12"), Some(0x45));
        assert_eq!(keycode("Delete"), Some(0x4c));
        assert_eq!(keycode("Spacebar"), Some(0x2c));
        assert_eq!(keycode(";"), Some(0x33));
        assert_eq!(keycode("Unknown"), None);
    }
}
