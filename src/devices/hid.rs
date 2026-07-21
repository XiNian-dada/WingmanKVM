use std::{
    collections::HashMap,
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
const ABSOLUTE_COORDINATE_MAX: u16 = 32_767;
const ABSOLUTE_COORDINATE_CENTER: u16 = 16_384;

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
    #[error("absolute pointer coordinates must be between 0 and {max}: ({x}, {y})")]
    InvalidAbsoluteCoordinates { x: u16, y: u16, max: u16 },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AbsolutePointerRequest {
    Move { x: u16, y: u16 },
    Click { x: u16, y: u16, button: u8 },
    Scroll { x: u16, y: u16, delta: i16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AbsolutePointerState {
    buttons: u8,
    x: u16,
    y: u16,
}

impl Default for AbsolutePointerState {
    fn default() -> Self {
        Self {
            buttons: 0,
            x: ABSOLUTE_COORDINATE_CENTER,
            y: ABSOLUTE_COORDINATE_CENTER,
        }
    }
}

impl AbsolutePointerState {
    fn set_position(&mut self, x: u16, y: u16) {
        self.x = x;
        self.y = y;
    }

    fn report(self, wheel: i8) -> [u8; 6] {
        let [x_low, x_high] = self.x.to_le_bytes();
        let [y_low, y_high] = self.y.to_le_bytes();
        [
            self.buttons & 0x07,
            x_low,
            x_high,
            y_low,
            y_high,
            wheel as u8,
        ]
    }
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
    Absolute {
        path: PathBuf,
        request: AbsolutePointerRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    Release {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    ReleaseAbsolute {
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

    pub async fn mouse_absolute(
        &self,
        path: Option<PathBuf>,
        request: AbsolutePointerRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.mouse_tx
            .try_send(MouseCommand::Absolute {
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
        relative_mouse_path: Option<PathBuf>,
        absolute_pointer_path: Option<PathBuf>,
    ) -> Result<(), HidError> {
        let mut first_error = None;
        if let Some(path) = keyboard_path {
            let (reply, response) = oneshot::channel();
            let result = match self
                .keyboard_tx
                .try_send(KeyboardCommand::Release { path, reply })
            {
                Ok(()) => match response.await {
                    Ok(result) => result,
                    Err(_) => Err(HidError::WorkerStopped),
                },
                Err(error) => Err(map_send_error(error)),
            };
            remember_first_error(&mut first_error, result);
        }
        if let Some(path) = relative_mouse_path {
            let (reply, response) = oneshot::channel();
            let result = match self
                .mouse_tx
                .try_send(MouseCommand::Release { path, reply })
            {
                Ok(()) => match response.await {
                    Ok(result) => result,
                    Err(_) => Err(HidError::WorkerStopped),
                },
                Err(error) => Err(map_send_error(error)),
            };
            remember_first_error(&mut first_error, result);
        }
        if let Some(path) = absolute_pointer_path {
            let (reply, response) = oneshot::channel();
            let result = match self
                .mouse_tx
                .try_send(MouseCommand::ReleaseAbsolute { path, reply })
            {
                Ok(()) => match response.await {
                    Ok(result) => result,
                    Err(_) => Err(HidError::WorkerStopped),
                },
                Err(error) => Err(map_send_error(error)),
            };
            remember_first_error(&mut first_error, result);
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn remember_first_error(first_error: &mut Option<HidError>, result: Result<(), HidError>) {
    if first_error.is_none()
        && let Err(error) = result
    {
        *first_error = Some(error);
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
    let mut absolute_states = HashMap::<PathBuf, AbsolutePointerState>::new();
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
            MouseCommand::Absolute {
                path,
                request,
                reply,
            } => {
                let state = absolute_states.entry(path.clone()).or_default();
                let result = send_absolute_pointer(&path, request, state).await;
                let _ = reply.send(result);
            }
            MouseCommand::Release { path, reply } => {
                let result = send_report(&path, &[0; 4]).await;
                let _ = reply.send(result);
            }
            MouseCommand::ReleaseAbsolute { path, reply } => {
                let state = absolute_states.entry(path.clone()).or_default();
                state.buttons = 0;
                let result = send_report(&path, &state.report(0)).await;
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
    validate_mouse_button(request.button)?;
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

async fn send_absolute_pointer(
    path: &Path,
    request: AbsolutePointerRequest,
    state: &mut AbsolutePointerState,
) -> Result<(), HidError> {
    match request {
        AbsolutePointerRequest::Move { x, y } => {
            validate_absolute_position(x, y)?;
            state.set_position(x, y);
            send_report(path, &state.report(0)).await
        }
        AbsolutePointerRequest::Click { x, y, button } => {
            let reports = absolute_click_reports(state, x, y, button)?;
            send_all_reports(path, &reports).await
        }
        AbsolutePointerRequest::Scroll { x, y, delta } => {
            validate_absolute_position(x, y)?;
            state.set_position(x, y);
            send_absolute_scroll(path, *state, delta).await
        }
    }
}

fn absolute_click_reports(
    state: &mut AbsolutePointerState,
    x: u16,
    y: u16,
    button: u8,
) -> Result<[[u8; 6]; 2], HidError> {
    validate_mouse_button(button)?;
    validate_absolute_position(x, y)?;
    state.set_position(x, y);
    state.buttons |= button;
    let pressed = state.report(0);
    state.buttons &= !button;
    let released = state.report(0);
    Ok([pressed, released])
}

async fn send_absolute_scroll(
    path: &Path,
    state: AbsolutePointerState,
    delta: i16,
) -> Result<(), HidError> {
    let reports = absolute_scroll_reports(state, delta);
    let mut first_error = None;
    for report in &reports[..reports.len() - 1] {
        let result = send_report(path, report).await;
        if let Err(error) = result {
            first_error = Some(error);
            break;
        }
    }
    remember_first_error(
        &mut first_error,
        send_report(path, reports.last().expect("scroll reports include reset")).await,
    );
    first_error.map_or(Ok(()), Err)
}

fn absolute_scroll_reports(state: AbsolutePointerState, delta: i16) -> Vec<[u8; 6]> {
    let mut remaining = delta.clamp(-4096, 4096);
    let mut reports = Vec::with_capacity((remaining.unsigned_abs() as usize).div_ceil(127) + 1);
    while remaining != 0 {
        let step = remaining.clamp(-127, 127) as i8;
        reports.push(state.report(step));
        remaining -= i16::from(step);
    }
    reports.push(state.report(0));
    reports
}

async fn send_all_reports<const REPORT_LENGTH: usize>(
    path: &Path,
    reports: &[[u8; REPORT_LENGTH]],
) -> Result<(), HidError> {
    let mut first_error = None;
    for report in reports {
        remember_first_error(&mut first_error, send_report(path, report).await);
    }
    first_error.map_or(Ok(()), Err)
}

fn validate_mouse_button(button: u8) -> Result<(), HidError> {
    if matches!(button, 1 | 2 | 4) {
        Ok(())
    } else {
        Err(HidError::InvalidButton(button))
    }
}

fn validate_absolute_position(x: u16, y: u16) -> Result<(), HidError> {
    if x <= ABSOLUTE_COORDINATE_MAX && y <= ABSOLUTE_COORDINATE_MAX {
        Ok(())
    } else {
        Err(HidError::InvalidAbsoluteCoordinates {
            x,
            y,
            max: ABSOLUTE_COORDINATE_MAX,
        })
    }
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

    #[test]
    fn absolute_report_is_little_endian() {
        let mut state = AbsolutePointerState {
            buttons: 0xff,
            ..AbsolutePointerState::default()
        };
        state.set_position(0x1234, ABSOLUTE_COORDINATE_MAX);

        assert_eq!(state.report(-2), [0x07, 0x34, 0x12, 0xff, 0x7f, 0xfe]);
    }

    #[test]
    fn absolute_pointer_rejects_coordinates_outside_the_descriptor_range() {
        assert!(matches!(
            validate_absolute_position(ABSOLUTE_COORDINATE_MAX + 1, 0),
            Err(HidError::InvalidAbsoluteCoordinates { .. })
        ));
    }

    #[test]
    fn absolute_click_releases_at_the_same_position() {
        let mut state = AbsolutePointerState::default();
        let reports = absolute_click_reports(&mut state, 1_234, 23_456, 2).unwrap();

        assert_eq!(&reports[0][1..5], &reports[1][1..5]);
        assert_eq!(reports[0][0], 2);
        assert_eq!(reports[1][0], 0);
        assert_eq!(state.x, 1_234);
        assert_eq!(state.y, 23_456);
        assert_eq!(state.buttons, 0);
    }

    #[test]
    fn absolute_click_rejects_unknown_button_bits() {
        let mut state = AbsolutePointerState::default();
        assert!(matches!(
            absolute_click_reports(&mut state, 0, 0, 3),
            Err(HidError::InvalidButton(3))
        ));
    }

    #[test]
    fn absolute_scroll_splits_delta_and_resets_wheel_without_moving() {
        let state = AbsolutePointerState {
            buttons: 1,
            x: 0x1234,
            y: 0x5678,
        };
        let reports = absolute_scroll_reports(state, 200);

        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0], [1, 0x34, 0x12, 0x78, 0x56, 127]);
        assert_eq!(reports[1], [1, 0x34, 0x12, 0x78, 0x56, 73]);
        assert_eq!(reports[2], [1, 0x34, 0x12, 0x78, 0x56, 0]);
    }

    #[test]
    fn absolute_release_uses_last_position_instead_of_zero_report() {
        let mut state = AbsolutePointerState {
            buttons: 4,
            ..AbsolutePointerState::default()
        };
        state.set_position(999, 888);
        state.buttons = 0;

        assert_eq!(state.report(0), [0, 0xe7, 0x03, 0x78, 0x03, 0]);
        assert_ne!(state.report(0), [0; 6]);
    }

    #[test]
    fn release_error_collection_keeps_the_first_error() {
        let mut first_error = None;
        remember_first_error(&mut first_error, Err(HidError::InvalidButton(8)));
        remember_first_error(&mut first_error, Err(HidError::NotConfigured));

        assert!(matches!(first_error, Some(HidError::InvalidButton(8))));
    }

    #[tokio::test]
    async fn release_all_continues_after_an_interface_fails() {
        let (keyboard_tx, keyboard_rx) = mpsc::channel(1);
        drop(keyboard_rx);
        let (mouse_tx, mut mouse_rx) = mpsc::channel(2);
        let manager = HidManager {
            keyboard_tx,
            mouse_tx,
        };
        let mouse_releases = tokio::spawn(async move {
            let mut relative = false;
            let mut absolute = false;
            for _ in 0..2 {
                match mouse_rx.recv().await.expect("release command") {
                    MouseCommand::Release { reply, .. } => {
                        relative = true;
                        let _ = reply.send(Ok(()));
                    }
                    MouseCommand::ReleaseAbsolute { reply, .. } => {
                        absolute = true;
                        let _ = reply.send(Ok(()));
                    }
                    _ => panic!("unexpected mouse command"),
                }
            }
            (relative, absolute)
        });

        let result = manager
            .release_all(
                Some(PathBuf::from("keyboard")),
                Some(PathBuf::from("relative")),
                Some(PathBuf::from("absolute")),
            )
            .await;

        assert!(matches!(result, Err(HidError::WorkerStopped)));
        assert_eq!(mouse_releases.await.unwrap(), (true, true));
    }
}
