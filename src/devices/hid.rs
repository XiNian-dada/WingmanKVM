use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, oneshot, watch},
    time::Instant,
};

const WRITE_TIMEOUT: Duration = Duration::from_millis(500);
const RELEASE_TIMEOUT: Duration = Duration::from_millis(500);
const QUEUE_CAPACITY: usize = 64;
/// While a channel is marked as re-enumerating, how often its nodes are
/// checked for having come back.
const LINK_PROBE_INTERVAL: Duration = Duration::from_millis(500);
const ABSOLUTE_COORDINATE_MAX: u16 = 32_767;
const ABSOLUTE_COORDINATE_CENTER: u16 = 16_384;
/// Shortest time a button stays down once pressed, however quickly its release
/// arrives. Network jitter can deliver a press and its release back to back, and
/// a firmware (BIOS/UEFI) driver polling a full-speed endpoint every 10 ms should
/// still see the button down for more than one poll. Same value as the default
/// key hold (`default_hold_ms`).
const MIN_PRESS_HOLD: Duration = Duration::from_millis(25);
/// A held button with no pointer traffic for this long is released: the bound for
/// a client that vanished without closing anything (an HTTP client, or a socket
/// whose network silently died). The console restates its position every 10 s
/// while a button is held, so it survives two lost heartbeats and any pause of the
/// user's; a scripted HTTP client only has to send something within this window.
const BUTTON_IDLE_RELEASE: Duration = Duration::from_secs(30);
/// A report that should have lifted a button but could not be written (host not
/// polling, gadget rebinding) is retried: the press may still reach the target.
const RELEASE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const RELEASE_RETRIES: u8 = 10;

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
    Press { x: u16, y: u16, button: u8 },
    Release { x: u16, y: u16, button: u8 },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyboardLeds {
    pub num_lock: bool,
    pub caps_lock: bool,
    pub scroll_lock: bool,
    pub raw: u8,
}

impl KeyboardLeds {
    pub fn from_byte(byte: u8) -> Self {
        Self {
            num_lock: (byte & 0x01) != 0,
            caps_lock: (byte & 0x02) != 0,
            scroll_lock: (byte & 0x04) != 0,
            raw: byte,
        }
    }

    #[allow(dead_code)]
    pub fn to_byte(&self) -> u8 {
        self.raw
    }
}

/// Per-channel USB link health exposed through `/api/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct HidLinkStatus {
    /// The keyboard gadget node vanished (ENODEV/ENOENT/ESHUTDOWN) and no
    /// write has succeeded since: the USB channel is re-enumerating.
    pub keyboard_reenumerating: bool,
    pub mouse_reenumerating: bool,
}

#[derive(Clone)]
pub struct HidManager {
    keyboard_tx: mpsc::Sender<KeyboardCommand>,
    mouse_tx: mpsc::Sender<MouseCommand>,
    keyboard_leds: Arc<AtomicU8>,
    keyboard_path_tx: watch::Sender<Option<PathBuf>>,
    keyboard_link_lost: Arc<AtomicBool>,
    mouse_link_lost: Arc<AtomicBool>,
    /// Set for good once the process starts shutting down: the workers then
    /// only execute releases, so nothing can be pressed after the last one.
    closing: Arc<AtomicBool>,
}

impl Default for HidManager {
    fn default() -> Self {
        Self::new()
    }
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
    Press {
        path: PathBuf,
        request: MouseClickRequest,
        reply: oneshot::Sender<Result<(), HidError>>,
    },
    ReleaseButton {
        path: PathBuf,
        request: MouseClickRequest,
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

/// Tells whether the USB host has the gadget behind a node configured,
/// without writing to it; `None` when that cannot be told.
type LinkReady = Box<dyn Fn(&Path) -> Option<bool> + Send>;

/// Worker-local handle cache. HID reports are tiny and frequent, so reopening
/// the gadget node for every report adds latency and widens the rebind race.
struct HidDeviceCache {
    devices: HashMap<PathBuf, AsyncFd<std::fs::File>>,
    /// Set while a gadget node is gone; cleared by the next successful write,
    /// or once every lost node opens again with its USB link up.
    link_lost: Arc<AtomicBool>,
    lost: HashSet<PathBuf>,
    link_ready: LinkReady,
    #[cfg(test)]
    opens: usize,
}

impl Default for HidDeviceCache {
    fn default() -> Self {
        let roots = super::discovery::GadgetRoots::default();
        Self {
            devices: HashMap::new(),
            link_lost: Arc::default(),
            lost: HashSet::new(),
            link_ready: Box::new(move |path| super::discovery::hid_link_ready(&roots, path)),
            #[cfg(test)]
            opens: 0,
        }
    }
}

/// Errors meaning the gadget node itself disappeared or was unbound (UDC
/// detach, target reboot re-enumeration, configfs rebind), as opposed to a
/// slow host or a malformed report.
fn is_device_gone(error: &HidError) -> bool {
    let HidError::Io { source, .. } = error else {
        return false;
    };
    matches!(
        source.raw_os_error(),
        Some(libc::ENODEV | libc::ENOENT | libc::ENXIO | libc::ESHUTDOWN)
    )
}

impl HidDeviceCache {
    fn with_link(link_lost: Arc<AtomicBool>) -> Self {
        Self {
            link_lost,
            ..Self::default()
        }
    }

    fn record(&mut self, path: &Path, result: &Result<(), HidError>) {
        match result {
            Ok(()) => {
                self.lost.clear();
                if self.link_lost.swap(false, Ordering::AcqRel) {
                    tracing::info!(path = %path.display(), "HID gadget channel recovered");
                }
            }
            Err(error) if is_device_gone(error) => {
                // Never keep a descriptor for a node that no longer exists:
                // the next write must reopen the re-enumerated device.
                self.invalidate(path);
                self.lost.insert(path.to_path_buf());
                if !self.link_lost.swap(true, Ordering::AcqRel) {
                    tracing::warn!(%error, path = %path.display(), "HID gadget node gone; waiting for re-enumeration");
                }
            }
            Err(_) => {}
        }
    }

    fn get_or_open(&mut self, path: &Path) -> Result<&mut AsyncFd<std::fs::File>, HidError> {
        if !self.devices.contains_key(path) {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(path)
                .map_err(|source| HidError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            // SAFETY: This owned nonblocking File is never closed or replaced
            // while registered; AsyncFd deregisters it before dropping the File.
            let fd = unsafe { AsyncFd::register(file) }.map_err(|source| HidError::Io {
                path: path.to_path_buf(),
                source: source.into(),
            })?;
            self.devices.insert(path.to_path_buf(), fd);
            #[cfg(test)]
            {
                self.opens += 1;
            }
        }
        Ok(self
            .devices
            .get_mut(path)
            .expect("cached HID device was just opened"))
    }

    fn invalidate(&mut self, path: &Path) {
        self.devices.remove(path);
    }

    fn link_lost(&self) -> bool {
        !self.lost.is_empty()
    }

    /// Clears the re-enumerating mark once every lost node opens again and
    /// its USB link is up, so the badge does not wait for the user to type.
    /// Nothing is written: a probe report would reach the target as input.
    fn probe_lost(&mut self) {
        let lost: Vec<PathBuf> = self.lost.iter().cloned().collect();
        for path in lost {
            // ENOENT/ENODEV while the gadget is still being rebuilt.
            if self.get_or_open(&path).is_err() {
                continue;
            }
            // The node of an unplugged cable or a powered-off target stays
            // and opens fine; only the UDC state says the host is back.
            if (self.link_ready)(&path) == Some(false) {
                continue;
            }
            self.lost.remove(&path);
        }
        if self.lost.is_empty() && self.link_lost.swap(false, Ordering::AcqRel) {
            tracing::info!("HID gadget channel recovered: device node is back");
        }
    }

    #[cfg(test)]
    fn open_count(&self) -> usize {
        self.opens
    }
}

impl HidManager {
    pub fn new() -> Self {
        Self::with_keyboard_device(None)
    }

    pub fn with_keyboard_device(keyboard_device: Option<PathBuf>) -> Self {
        Self::build(keyboard_device, true)
    }

    /// Tests that read a fake keyboard node must not race the LED reader for
    /// its bytes.
    #[cfg(test)]
    fn without_led_reader() -> Self {
        Self::build(None, false)
    }

    fn build(keyboard_device: Option<PathBuf>, led_reader: bool) -> Self {
        let (keyboard_tx, keyboard_rx) = mpsc::channel(QUEUE_CAPACITY);
        let (mouse_tx, mouse_rx) = mpsc::channel(QUEUE_CAPACITY);
        let (keyboard_path_tx, keyboard_path_rx) = watch::channel(keyboard_device);
        let keyboard_leds = Arc::new(AtomicU8::new(0));
        let keyboard_link_lost = Arc::new(AtomicBool::new(false));
        let mouse_link_lost = Arc::new(AtomicBool::new(false));
        let closing = Arc::new(AtomicBool::new(false));

        tokio::spawn(keyboard_worker(
            keyboard_rx,
            HidDeviceCache::with_link(Arc::clone(&keyboard_link_lost)),
            Arc::clone(&closing),
        ));
        tokio::spawn(mouse_worker(
            mouse_rx,
            HidDeviceCache::with_link(Arc::clone(&mouse_link_lost)),
            Arc::clone(&closing),
        ));
        if led_reader {
            tokio::spawn(keyboard_led_reader(
                keyboard_path_rx,
                Arc::clone(&keyboard_leds),
            ));
        }

        Self {
            keyboard_tx,
            mouse_tx,
            keyboard_leds,
            keyboard_path_tx,
            keyboard_link_lost,
            mouse_link_lost,
            closing,
        }
    }

    pub fn link_status(&self) -> HidLinkStatus {
        HidLinkStatus {
            keyboard_reenumerating: self.keyboard_link_lost.load(Ordering::Acquire),
            mouse_reenumerating: self.mouse_link_lost.load(Ordering::Acquire),
        }
    }

    pub fn leds(&self) -> KeyboardLeds {
        KeyboardLeds::from_byte(self.keyboard_leds.load(Ordering::Acquire))
    }

    #[allow(dead_code)]
    pub fn set_leds_raw(&self, raw: u8) {
        self.keyboard_leds.store(raw, Ordering::Release);
    }

    pub fn set_keyboard_device(&self, path: Option<PathBuf>) {
        if *self.keyboard_path_tx.borrow() != path {
            let _ = self.keyboard_path_tx.send(path);
        }
    }

    pub async fn key(&self, path: Option<PathBuf>, request: KeyRequest) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        self.set_keyboard_device(Some(path.clone()));
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

    pub async fn mouse_press(
        &self,
        path: Option<PathBuf>,
        request: MouseClickRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        self.mouse_tx
            .try_send(MouseCommand::Press {
                path,
                request,
                reply,
            })
            .map_err(map_send_error)?;
        response.await.map_err(|_| HidError::WorkerStopped)?
    }

    /// Like every release, waits briefly for queue room instead of being dropped.
    pub async fn mouse_release(
        &self,
        path: Option<PathBuf>,
        request: MouseClickRequest,
    ) -> Result<(), HidError> {
        let path = path.ok_or(HidError::NotConfigured)?;
        let (reply, response) = oneshot::channel();
        send_release_command(
            &self.mouse_tx,
            MouseCommand::ReleaseButton {
                path,
                request,
                reply,
            },
            response,
        )
        .await
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
        if matches!(request, AbsolutePointerRequest::Release { .. }) {
            let command = MouseCommand::Absolute {
                path,
                request,
                reply,
            };
            return send_release_command(&self.mouse_tx, command, response).await;
        }
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
            let result = send_release_command(
                &self.keyboard_tx,
                KeyboardCommand::Release { path, reply },
                response,
            )
            .await;
            remember_first_error(&mut first_error, result);
        }
        if let Some(path) = relative_mouse_path {
            let (reply, response) = oneshot::channel();
            let result = send_release_command(
                &self.mouse_tx,
                MouseCommand::Release { path, reply },
                response,
            )
            .await;
            remember_first_error(&mut first_error, result);
        }
        if let Some(path) = absolute_pointer_path {
            let (reply, response) = oneshot::channel();
            let result = send_release_command(
                &self.mouse_tx,
                MouseCommand::ReleaseAbsolute { path, reply },
                response,
            )
            .await;
            remember_first_error(&mut first_error, result);
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Final release before the process exits. Presses still queued are
    /// dropped and later ones refused, because a press that reached the target
    /// after this release would stay down with nobody left to release it. A
    /// key already being held finishes with its own release first.
    pub async fn release_for_shutdown(
        &self,
        keyboard_path: Option<PathBuf>,
        relative_mouse_path: Option<PathBuf>,
        absolute_pointer_path: Option<PathBuf>,
    ) -> Result<(), HidError> {
        self.closing.store(true, Ordering::Release);
        self.release_all(keyboard_path, relative_mouse_path, absolute_pointer_path)
            .await
    }
}

/// Emergency releases must not be dropped just because the queue is briefly
/// full. Wait for a short bounded window before reporting QueueFull.
async fn send_release_command<T>(
    tx: &mpsc::Sender<T>,
    command: T,
    response: oneshot::Receiver<Result<(), HidError>>,
) -> Result<(), HidError> {
    match tokio::time::timeout(RELEASE_TIMEOUT, tx.send(command)).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(HidError::WorkerStopped),
        Err(_) => return Err(HidError::QueueFull),
    }
    response.await.map_err(|_| HidError::WorkerStopped)?
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

fn link_probe() -> tokio::time::Interval {
    let mut probe = tokio::time::interval(LINK_PROBE_INTERVAL);
    probe.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    probe
}

async fn keyboard_worker(
    mut rx: mpsc::Receiver<KeyboardCommand>,
    mut devices: HidDeviceCache,
    closing: Arc<AtomicBool>,
) {
    let mut probe = link_probe();
    loop {
        let command = tokio::select! {
            command = rx.recv() => match command {
                Some(command) => command,
                None => break,
            },
            _ = probe.tick(), if devices.link_lost() => {
                devices.probe_lost();
                continue;
            }
        };
        // Dropping the command makes its caller see `WorkerStopped`.
        if closing.load(Ordering::Acquire) && !matches!(command, KeyboardCommand::Release { .. }) {
            continue;
        }
        match command {
            KeyboardCommand::Key {
                path,
                request,
                reply,
            } => {
                let result = send_key(&path, &request, &mut devices).await;
                let _ = reply.send(result);
            }
            KeyboardCommand::Release { path, reply } => {
                let result = send_report(&path, &[0; 8], &mut devices).await;
                let _ = reply.send(result);
            }
        }
    }
}

async fn keyboard_led_reader(mut path_rx: watch::Receiver<Option<PathBuf>>, leds: Arc<AtomicU8>) {
    loop {
        let current_path = path_rx.borrow_and_update().clone();
        let Some(path) = current_path else {
            if path_rx.changed().await.is_err() {
                break;
            }
            continue;
        };

        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
        {
            Ok(file) => file,
            Err(_) => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                    changed = path_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                    }
                }
                continue;
            }
        };

        // SAFETY: AsyncFd takes ownership of this nonblocking File. Its descriptor
        // stays open and unchanged until AsyncFd deregisters and drops it.
        let async_fd = match unsafe { AsyncFd::register(file) } {
            Ok(fd) => fd,
            Err(_) => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                    changed = path_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                    }
                }
                continue;
            }
        };

        let mut buf = [0u8; 8];
        loop {
            tokio::select! {
                changed = path_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
                readable = async_fd.readable() => {
                    let mut guard = match readable {
                        Ok(g) => g,
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            break;
                        }
                    };

                    match guard.try_io(|inner| {
                        let mut file = inner.get_ref();
                        file.read(&mut buf)
                    }) {
                        Ok(Ok(n)) if n > 0 => {
                            let byte = buf[0];
                            leds.store(byte, Ordering::Release);
                        }
                        Ok(Ok(_)) => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            break;
                        }
                        Ok(Err(source)) if source.kind() == io::ErrorKind::Interrupted => {
                            continue;
                        }
                        Ok(Err(source)) if source.kind() == io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        Ok(Err(_)) => {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            break;
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
            }
        }
    }
}

/// Server-side button bookkeeping. The worker owns one bitmask per pointer
/// device, so every report it writes (move, scroll, click) carries what is held.
#[derive(Default)]
struct PointerTracker {
    absolute: HashMap<PathBuf, AbsolutePointerState>,
    relative_buttons: HashMap<PathBuf, u8>,
    clocks: HashMap<PathBuf, ButtonClock>,
}

/// Timers of a device that has a button down or a release still to deliver.
#[derive(Debug, Clone, Copy)]
struct ButtonClock {
    last_input: Instant,
    last_press: Instant,
    retries_left: u8,
    next_retry: Instant,
}

impl ButtonClock {
    fn new(now: Instant) -> Self {
        Self {
            last_input: now,
            last_press: now,
            retries_left: 0,
            next_retry: now,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PointerTimings {
    min_press_hold: Duration,
    idle_release: Duration,
    release_retry: Duration,
}

impl Default for PointerTimings {
    fn default() -> Self {
        Self {
            min_press_hold: MIN_PRESS_HOLD,
            idle_release: BUTTON_IDLE_RELEASE,
            release_retry: RELEASE_RETRY_INTERVAL,
        }
    }
}

impl PointerTracker {
    fn buttons(&self, path: &Path) -> u8 {
        self.absolute.get(path).map_or(0, |state| state.buttons)
            | self.relative_buttons.get(path).copied().unwrap_or(0)
    }

    fn clear_buttons(&mut self, path: &Path) {
        if let Some(state) = self.absolute.get_mut(path) {
            state.buttons = 0;
        }
        self.relative_buttons.remove(path);
    }

    /// Any traffic from the client proves it is alive.
    fn note_input(&mut self, path: &Path) {
        if let Some(clock) = self.clocks.get_mut(path) {
            clock.last_input = Instant::now();
        }
    }

    fn note_press(&mut self, path: &Path) {
        let now = Instant::now();
        let clock = self
            .clocks
            .entry(path.to_path_buf())
            .or_insert_with(|| ButtonClock::new(now));
        clock.last_input = now;
        clock.last_press = now;
    }

    /// Records the outcome of a report that lifted a button: a failure is
    /// retried from the worker loop, a later success settles it.
    fn note_release_result(
        &mut self,
        path: &Path,
        result: &Result<(), HidError>,
        timings: PointerTimings,
    ) {
        if result.is_err() {
            let now = Instant::now();
            let clock = self
                .clocks
                .entry(path.to_path_buf())
                .or_insert_with(|| ButtonClock::new(now));
            clock.retries_left = RELEASE_RETRIES;
            clock.next_retry = now + timings.release_retry;
        } else if let Some(clock) = self.clocks.get_mut(path) {
            clock.retries_left = 0;
        }
        self.forget_settled(path);
    }

    fn forget_settled(&mut self, path: &Path) {
        if self.buttons(path) == 0
            && self
                .clocks
                .get(path)
                .is_some_and(|clock| clock.retries_left == 0)
        {
            self.clocks.remove(path);
        }
    }

    async fn wait_for_min_hold(&self, path: &Path, timings: PointerTimings) {
        if let Some(clock) = self.clocks.get(path) {
            tokio::time::sleep_until(clock.last_press + timings.min_press_hold).await;
        }
    }

    fn next_deadline(&self, timings: PointerTimings) -> Option<Instant> {
        self.clocks
            .iter()
            .flat_map(|(path, clock)| {
                let idle =
                    (self.buttons(path) != 0).then_some(clock.last_input + timings.idle_release);
                let retry = (clock.retries_left > 0).then_some(clock.next_retry);
                [idle, retry]
            })
            .flatten()
            .min()
    }

    /// Writes the current state of a device with no motion, so whatever the
    /// bitmask says is what the target ends up seeing.
    async fn write_current_state(
        &self,
        path: &Path,
        devices: &mut HidDeviceCache,
    ) -> Result<(), HidError> {
        if let Some(state) = self.absolute.get(path) {
            send_report(path, &state.report(0), devices).await
        } else {
            send_report(path, &relative_report(self.buttons(path), 0, 0, 0), devices).await
        }
    }

    async fn release_idle_and_retry(
        &mut self,
        timings: PointerTimings,
        devices: &mut HidDeviceCache,
    ) {
        let now = Instant::now();
        let due: Vec<(PathBuf, bool)> = self
            .clocks
            .iter()
            .filter_map(|(path, clock)| {
                let idle =
                    self.buttons(path) != 0 && clock.last_input + timings.idle_release <= now;
                let retry = clock.retries_left > 0 && clock.next_retry <= now;
                (idle || retry).then(|| (path.clone(), idle))
            })
            .collect();
        for (path, idle) in due {
            if idle {
                tracing::warn!(path = %path.display(), "mouse button held with no pointer traffic; releasing it");
                self.clear_buttons(&path);
            }
            let result = self.write_current_state(&path, devices).await;
            match result {
                Err(error) if !idle => {
                    let Some(clock) = self.clocks.get_mut(&path) else {
                        continue;
                    };
                    clock.retries_left -= 1;
                    clock.next_retry = Instant::now() + timings.release_retry;
                    if clock.retries_left == 0 {
                        tracing::error!(%error, path = %path.display(), "giving up on mouse button release");
                        self.forget_settled(&path);
                    }
                }
                result => self.note_release_result(&path, &result, timings),
            }
        }
    }
}

async fn sleep_until_some(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn mouse_worker(
    rx: mpsc::Receiver<MouseCommand>,
    devices: HidDeviceCache,
    closing: Arc<AtomicBool>,
) {
    run_mouse_worker(rx, devices, closing, PointerTimings::default()).await;
}

async fn run_mouse_worker(
    mut rx: mpsc::Receiver<MouseCommand>,
    mut devices: HidDeviceCache,
    closing: Arc<AtomicBool>,
    timings: PointerTimings,
) {
    let mut pointers = PointerTracker::default();
    let mut probe = link_probe();
    loop {
        let deadline = pointers.next_deadline(timings);
        let command = tokio::select! {
            biased;
            command = rx.recv() => match command {
                Some(command) => command,
                None => break,
            },
            () = sleep_until_some(deadline) => {
                pointers.release_idle_and_retry(timings, &mut devices).await;
                continue;
            }
            _ = probe.tick(), if devices.link_lost() => {
                devices.probe_lost();
                continue;
            }
        };
        // Dropping the command makes its caller see `WorkerStopped`. Anything
        // that only lifts a button still runs: it cannot press after the last
        // release.
        if closing.load(Ordering::Acquire)
            && !matches!(
                command,
                MouseCommand::Release { .. }
                    | MouseCommand::ReleaseAbsolute { .. }
                    | MouseCommand::ReleaseButton { .. }
                    | MouseCommand::Absolute {
                        request: AbsolutePointerRequest::Release { .. },
                        ..
                    }
            )
        {
            continue;
        }
        match command {
            MouseCommand::Move {
                path,
                request,
                reply,
            } => {
                pointers.note_input(&path);
                let buttons = pointers.buttons(&path);
                let result = send_mouse_move(&path, request, buttons, &mut devices).await;
                let _ = reply.send(result);
            }
            MouseCommand::Click {
                path,
                request,
                reply,
            } => {
                pointers.note_input(&path);
                let buttons = pointers.relative_buttons.entry(path.clone()).or_default();
                let result = send_mouse_click(&path, request, buttons, &mut devices).await;
                pointers.forget_settled(&path);
                let _ = reply.send(result);
            }
            MouseCommand::Press {
                path,
                request,
                reply,
            } => {
                let buttons = pointers.relative_buttons.entry(path.clone()).or_default();
                let result = send_relative_press(&path, request, buttons, &mut devices).await;
                if result.is_ok() {
                    pointers.note_press(&path);
                }
                let _ = reply.send(result);
            }
            MouseCommand::ReleaseButton {
                path,
                request,
                reply,
            } => {
                pointers.note_input(&path);
                let result = match validate_mouse_button(request.button) {
                    Ok(()) => {
                        pointers.wait_for_min_hold(&path, timings).await;
                        let buttons = pointers.relative_buttons.entry(path.clone()).or_default();
                        *buttons &= !request.button;
                        let report = relative_report(*buttons, 0, 0, 0);
                        let result = send_report(&path, &report, &mut devices).await;
                        pointers.note_release_result(&path, &result, timings);
                        result
                    }
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            MouseCommand::Scroll {
                path,
                request,
                reply,
            } => {
                pointers.note_input(&path);
                let buttons = pointers.buttons(&path);
                let result = send_mouse_scroll(&path, request, buttons, &mut devices).await;
                let _ = reply.send(result);
            }
            MouseCommand::Absolute {
                path,
                request,
                reply,
            } => {
                pointers.note_input(&path);
                if matches!(request, AbsolutePointerRequest::Release { .. }) {
                    pointers.wait_for_min_hold(&path, timings).await;
                }
                let state = pointers.absolute.entry(path.clone()).or_default();
                let result = send_absolute_pointer(&path, request, state, &mut devices).await;
                match request {
                    AbsolutePointerRequest::Press { .. } if result.is_ok() => {
                        pointers.note_press(&path);
                    }
                    AbsolutePointerRequest::Release { .. } => {
                        pointers.note_release_result(&path, &result, timings);
                    }
                    _ => pointers.forget_settled(&path),
                }
                let _ = reply.send(result);
            }
            MouseCommand::Release { path, reply } => {
                pointers.relative_buttons.remove(&path);
                let result = send_report(&path, &[0; 4], &mut devices).await;
                pointers.note_release_result(&path, &result, timings);
                let _ = reply.send(result);
            }
            MouseCommand::ReleaseAbsolute { path, reply } => {
                let state = pointers.absolute.entry(path.clone()).or_default();
                state.buttons = 0;
                let result = send_report(&path, &state.report(0), &mut devices).await;
                pointers.note_release_result(&path, &result, timings);
                let _ = reply.send(result);
            }
        }
    }
}

async fn send_key(
    path: &Path,
    request: &KeyRequest,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
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

    send_report(path, &report, devices).await?;
    tokio::time::sleep(Duration::from_millis(request.hold_ms.clamp(10, 500))).await;
    send_report(path, &[0; 8], devices).await
}

fn relative_report(buttons: u8, dx: i8, dy: i8, wheel: i8) -> [u8; 4] {
    [buttons & 0x07, dx as u8, dy as u8, wheel as u8]
}

async fn send_mouse_move(
    path: &Path,
    request: MouseMoveRequest,
    buttons: u8,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let mut dx = request.dx.clamp(-4096, 4096);
    let mut dy = request.dy.clamp(-4096, 4096);
    while dx != 0 || dy != 0 {
        let step_x = dx.clamp(-127, 127) as i8;
        let step_y = dy.clamp(-127, 127) as i8;
        send_report(path, &relative_report(buttons, step_x, step_y, 0), devices).await?;
        dx -= i16::from(step_x);
        dy -= i16::from(step_y);
    }
    Ok(())
}

async fn send_mouse_click(
    path: &Path,
    request: MouseClickRequest,
    buttons: &mut u8,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    validate_mouse_button(request.button)?;
    *buttons &= !request.button;
    let pressed = relative_report(*buttons | request.button, 0, 0, 0);
    send_report(path, &pressed, devices).await?;
    send_report(path, &relative_report(*buttons, 0, 0, 0), devices).await
}

/// The bit only stays set if the report went out: a failed press must not make
/// later moves claim a button the target never saw.
async fn send_relative_press(
    path: &Path,
    request: MouseClickRequest,
    buttons: &mut u8,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    validate_mouse_button(request.button)?;
    let was_held = *buttons & request.button != 0;
    *buttons |= request.button;
    let result = send_report(path, &relative_report(*buttons, 0, 0, 0), devices).await;
    if result.is_err() && !was_held {
        *buttons &= !request.button;
    }
    result
}

async fn send_mouse_scroll(
    path: &Path,
    request: MouseScrollRequest,
    buttons: u8,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let mut delta = request.delta.clamp(-4096, 4096);
    while delta != 0 {
        let step = delta.clamp(-127, 127) as i8;
        send_report(path, &relative_report(buttons, 0, 0, step), devices).await?;
        delta -= i16::from(step);
    }
    send_report(path, &relative_report(buttons, 0, 0, 0), devices).await
}

async fn send_absolute_pointer(
    path: &Path,
    request: AbsolutePointerRequest,
    state: &mut AbsolutePointerState,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    match request {
        AbsolutePointerRequest::Move { x, y } => {
            validate_absolute_position(x, y)?;
            state.set_position(x, y);
            send_report(path, &state.report(0), devices).await
        }
        AbsolutePointerRequest::Click { x, y, button } => {
            let reports = absolute_click_reports(state, x, y, button)?;
            send_all_reports(path, &reports, devices).await
        }
        AbsolutePointerRequest::Press { x, y, button } => {
            let was_held = state.buttons & button != 0;
            let report = absolute_press_report(state, x, y, button)?;
            let result = send_report(path, &report, devices).await;
            if result.is_err() && !was_held {
                state.buttons &= !button;
            }
            result
        }
        AbsolutePointerRequest::Release { x, y, button } => {
            let (report, position) = absolute_release_report(state, x, y, button)?;
            send_report(path, &report, devices).await.and(position)
        }
        AbsolutePointerRequest::Scroll { x, y, delta } => {
            validate_absolute_position(x, y)?;
            state.set_position(x, y);
            send_absolute_scroll(path, *state, delta, devices).await
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

fn absolute_press_report(
    state: &mut AbsolutePointerState,
    x: u16,
    y: u16,
    button: u8,
) -> Result<[u8; 6], HidError> {
    validate_mouse_button(button)?;
    validate_absolute_position(x, y)?;
    state.set_position(x, y);
    state.buttons |= button;
    Ok(state.report(0))
}

/// A bad position must not keep the button down: the button is lifted where the
/// pointer last was, and the position error is handed back to the caller.
fn absolute_release_report(
    state: &mut AbsolutePointerState,
    x: u16,
    y: u16,
    button: u8,
) -> Result<([u8; 6], Result<(), HidError>), HidError> {
    validate_mouse_button(button)?;
    let position = validate_absolute_position(x, y);
    if position.is_ok() {
        state.set_position(x, y);
    }
    state.buttons &= !button;
    Ok((state.report(0), position))
}

async fn send_absolute_scroll(
    path: &Path,
    state: AbsolutePointerState,
    delta: i16,
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let reports = absolute_scroll_reports(state, delta);
    let mut first_error = None;
    for report in &reports[..reports.len() - 1] {
        let result = send_report(path, report, devices).await;
        if let Err(error) = result {
            first_error = Some(error);
            break;
        }
    }
    remember_first_error(
        &mut first_error,
        send_report(
            path,
            reports.last().expect("scroll reports include reset"),
            devices,
        )
        .await,
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
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let mut first_error = None;
    for report in reports {
        remember_first_error(&mut first_error, send_report(path, report, devices).await);
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

async fn send_report(
    path: &Path,
    report: &[u8],
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let result = send_report_once(path, report, devices).await;
    devices.record(path, &result);
    result
}

async fn send_report_once(
    path: &Path,
    report: &[u8],
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    match write_cached_report(path, report, devices).await {
        Ok(()) => Ok(()),
        Err(first @ HidError::Io { .. }) => {
            // The gadget may have been rebound under us. Drop the stale handle
            // and retry once with a freshly opened device.
            devices.invalidate(path);
            tracing::warn!(%first, path = %path.display(), "HID write failed; reopening device");
            let retry = write_cached_report(path, report, devices).await;
            if retry.is_err() {
                devices.invalidate(path);
            }
            retry
        }
        Err(error) => {
            devices.invalidate(path);
            Err(error)
        }
    }
}

async fn write_cached_report(
    path: &Path,
    report: &[u8],
    devices: &mut HidDeviceCache,
) -> Result<(), HidError> {
    let fd = devices.get_or_open(path)?;
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
        // ISO/JIS extra keys used by non-US target layouts. HID usages come
        // from the Keyboard/Keypad page: 0x64 Non-US \ and | (the key left of
        // Z), 0x87 International1 (Ro), 0x89 International3 (Yen).
        "IntlBackslash" => 0x64,
        "Semicolon" | ";" => 0x33,
        "Quote" | "'" => 0x34,
        "Backquote" | "`" => 0x35,
        "Comma" | "," => 0x36,
        "Period" | "." => 0x37,
        "Slash" | "/" => 0x38,
        "IntlRo" => 0x87,
        "IntlYen" => 0x89,
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
pub(crate) mod tests {
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
    fn maps_iso_and_jis_extra_keys() {
        // ISO/JIS layouts need the extra physical keys beyond the US table.
        assert_eq!(keycode("IntlBackslash"), Some(0x64));
        assert_eq!(keycode("IntlRo"), Some(0x87));
        assert_eq!(keycode("IntlYen"), Some(0x89));
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

    #[test]
    fn absolute_press_move_release_keep_the_button_mask() {
        let mut state = AbsolutePointerState::default();
        assert_eq!(
            absolute_press_report(&mut state, 0x0102, 0x0304, 1).unwrap(),
            [1, 0x02, 0x01, 0x04, 0x03, 0],
            "press carries the button and its position"
        );
        state.set_position(0x0506, 0x0708);
        assert_eq!(
            state.report(0),
            [1, 0x06, 0x05, 0x08, 0x07, 0],
            "a move keeps the held button"
        );
        let (report, position) = absolute_release_report(&mut state, 0x0506, 0x0708, 1).unwrap();
        assert!(position.is_ok());
        assert_eq!(report, [0, 0x06, 0x05, 0x08, 0x07, 0], "release keeps the position");
    }

    #[test]
    fn absolute_multi_button_release_keeps_the_others() {
        let mut state = AbsolutePointerState::default();
        absolute_press_report(&mut state, 1, 1, 1).unwrap();
        assert_eq!(absolute_press_report(&mut state, 1, 1, 2).unwrap()[0], 3);
        assert_eq!(absolute_press_report(&mut state, 1, 1, 4).unwrap()[0], 7);
        let (report, _) = absolute_release_report(&mut state, 1, 1, 2).unwrap();
        assert_eq!(report[0], 5, "only the right button lifts");
        // A click of another button during a drag keeps the drag's button.
        let [pressed, released] = absolute_click_reports(&mut state, 1, 1, 2).unwrap();
        assert_eq!((pressed[0], released[0]), (7, 5));
    }

    #[test]
    fn absolute_press_and_release_reject_bad_input_without_changing_the_mask() {
        let mut state = AbsolutePointerState::default();
        assert!(matches!(
            absolute_press_report(&mut state, 1, 1, 3),
            Err(HidError::InvalidButton(3))
        ));
        assert!(matches!(
            absolute_press_report(&mut state, ABSOLUTE_COORDINATE_MAX + 1, 1, 1),
            Err(HidError::InvalidAbsoluteCoordinates { .. })
        ));
        assert_eq!(state.buttons, 0);
        state.buttons = 1;
        assert!(matches!(
            absolute_release_report(&mut state, 1, 1, 8),
            Err(HidError::InvalidButton(8))
        ));
        assert_eq!(state.buttons, 1);
    }

    #[test]
    fn absolute_release_with_a_bad_position_still_lifts_the_button() {
        let mut state = AbsolutePointerState::default();
        absolute_press_report(&mut state, 5, 6, 1).unwrap();
        let (report, position) =
            absolute_release_report(&mut state, ABSOLUTE_COORDINATE_MAX + 1, 6, 1).unwrap();
        assert_eq!(report, [0, 5, 0, 6, 0, 0], "lifted at the last good position");
        assert!(matches!(position, Err(HidError::InvalidAbsoluteCoordinates { .. })));
    }

    #[test]
    fn press_and_release_payloads_parse() {
        let press: AbsolutePointerRequest =
            serde_json::from_str(r#"{"action":"press","x":1,"y":2,"button":4}"#).unwrap();
        assert_eq!(press, AbsolutePointerRequest::Press { x: 1, y: 2, button: 4 });
        let release: AbsolutePointerRequest =
            serde_json::from_str(r#"{"action":"release","x":1,"y":2,"button":4}"#).unwrap();
        assert_eq!(release, AbsolutePointerRequest::Release { x: 1, y: 2, button: 4 });
    }

    /// A real mouse worker writing to a socket pair, driven through HidManager.
    #[cfg(unix)]
    struct WorkerFixture {
        manager: HidManager,
        path: PathBuf,
        peer: std::os::unix::net::UnixStream,
    }

    #[cfg(unix)]
    fn manager_for_mouse_queue(mouse_tx: mpsc::Sender<MouseCommand>) -> HidManager {
        HidManager {
            keyboard_tx: mpsc::channel(1).0,
            mouse_tx,
            keyboard_leds: Arc::new(AtomicU8::new(0)),
            keyboard_path_tx: watch::channel(None).0,
            keyboard_link_lost: Arc::default(),
            mouse_link_lost: Arc::default(),
            closing: Arc::default(),
        }
    }

    #[cfg(unix)]
    const NEVER: Duration = Duration::from_secs(600);

    #[cfg(unix)]
    impl WorkerFixture {
        fn start(timings: PointerTimings) -> Self {
            let path = PathBuf::from("/nonexistent/test-pointer");
            let (devices, peer) = cached_test_device(path.clone());
            peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let (mouse_tx, mouse_rx) = mpsc::channel(QUEUE_CAPACITY);
            let mut manager = manager_for_mouse_queue(mouse_tx);
            manager.closing = Arc::default();
            tokio::spawn(run_mouse_worker(
                mouse_rx,
                devices,
                Arc::clone(&manager.closing),
                timings,
            ));
            Self { manager, path, peer }
        }

        fn with_idle(idle_release: Duration) -> Self {
            Self::start(PointerTimings {
                min_press_hold: Duration::from_millis(1),
                idle_release,
                release_retry: NEVER,
            })
        }

        fn with_hold(min_press_hold: Duration) -> Self {
            Self::start(PointerTimings {
                min_press_hold,
                idle_release: NEVER,
                release_retry: NEVER,
            })
        }

        fn fast() -> Self {
            Self::with_idle(NEVER)
        }

        fn device(&self) -> Option<PathBuf> {
            Some(self.path.clone())
        }

        async fn read<const N: usize>(&self) -> [u8; N] {
            let mut peer = self.peer.try_clone().unwrap();
            tokio::task::spawn_blocking(move || {
                let mut report = [0; N];
                peer.read_exact(&mut report).expect("HID report");
                report
            })
            .await
            .unwrap()
        }

        fn assert_silent(&self) {
            self.peer.set_nonblocking(true).unwrap();
            let mut byte = [0; 1];
            let mut peer = &self.peer;
            let read = peer.read(&mut byte);
            self.peer.set_nonblocking(false).unwrap();
            assert!(read.is_err(), "unexpected report: {read:?} {byte:?}");
        }
    }

    #[cfg(unix)]
    const LEFT: MouseClickRequest = MouseClickRequest { button: 1 };
    #[cfg(unix)]
    const RIGHT: MouseClickRequest = MouseClickRequest { button: 2 };

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_press_move_scroll_release_report_bytes() {
        let device = WorkerFixture::fast();
        let path = device.device();
        let manager = &device.manager;
        manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        let drag = MouseMoveRequest { dx: 5, dy: -3 };
        manager.mouse_move(path.clone(), drag).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 5, (-3i8) as u8, 0], "move holds the button");
        manager.mouse_scroll(path.clone(), MouseScrollRequest { delta: 2 }).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 2], "scroll holds the button");
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        manager.mouse_press(path.clone(), RIGHT).await.unwrap();
        assert_eq!(device.read::<4>().await, [3, 0, 0, 0], "second button adds to the mask");
        manager.mouse_release(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [2, 0, 0, 0], "releasing one keeps the other");
        manager.mouse_release(path.clone(), RIGHT).await.unwrap();
        assert_eq!(device.read::<4>().await, [0, 0, 0, 0]);
        manager.mouse_move(path.clone(), MouseMoveRequest { dx: 1, dy: 1 }).await.unwrap();
        assert_eq!(device.read::<4>().await, [0, 1, 1, 0], "moves are button-free again");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_click_during_a_drag_keeps_the_held_button() {
        let device = WorkerFixture::fast();
        let path = device.device();
        device.manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        device.manager.mouse_click(path.clone(), RIGHT).await.unwrap();
        assert_eq!(device.read::<4>().await, [3, 0, 0, 0]);
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn invalid_buttons_are_rejected_without_a_report() {
        let device = WorkerFixture::fast();
        let path = device.device();
        let bad = MouseClickRequest { button: 3 };
        assert!(matches!(
            device.manager.mouse_press(path.clone(), bad).await,
            Err(HidError::InvalidButton(3))
        ));
        assert!(matches!(
            device.manager.mouse_release(path, bad).await,
            Err(HidError::InvalidButton(3))
        ));
        device.assert_silent();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn absolute_worker_holds_the_button_across_moves_until_release() {
        let device = WorkerFixture::fast();
        let path = device.device();
        let manager = &device.manager;
        let press = AbsolutePointerRequest::Press { x: 10, y: 20, button: 1 };
        manager.mouse_absolute(path.clone(), press).await.unwrap();
        assert_eq!(device.read::<6>().await, [1, 10, 0, 20, 0, 0]);
        let moved = AbsolutePointerRequest::Move { x: 30, y: 40 };
        manager.mouse_absolute(path.clone(), moved).await.unwrap();
        assert_eq!(device.read::<6>().await, [1, 30, 0, 40, 0, 0]);
        let release = AbsolutePointerRequest::Release { x: 30, y: 40, button: 1 };
        manager.mouse_absolute(path.clone(), release).await.unwrap();
        assert_eq!(device.read::<6>().await, [0, 30, 0, 40, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn release_all_clears_the_mask_and_sends_a_zero_button_report() {
        let device = WorkerFixture::fast();
        let path = device.device();
        let manager = &device.manager;
        // Relative: the release report is zero and later moves stay button-free.
        manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        manager.release_all(None, path.clone(), None).await.unwrap();
        assert_eq!(device.read::<4>().await, [0, 0, 0, 0]);
        manager.mouse_move(path.clone(), MouseMoveRequest { dx: 1, dy: 0 }).await.unwrap();
        assert_eq!(device.read::<4>().await, [0, 1, 0, 0]);
        // Absolute: same, staying at the last position.
        let press = AbsolutePointerRequest::Press { x: 7, y: 8, button: 2 };
        manager.mouse_absolute(path.clone(), press).await.unwrap();
        assert_eq!(device.read::<6>().await, [2, 7, 0, 8, 0, 0]);
        manager.release_all(None, None, path.clone()).await.unwrap();
        assert_eq!(device.read::<6>().await, [0, 7, 0, 8, 0, 0]);
        let moved = AbsolutePointerRequest::Move { x: 9, y: 9 };
        manager.mouse_absolute(path.clone(), moved).await.unwrap();
        assert_eq!(device.read::<6>().await, [0, 9, 0, 9, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_refuses_presses_but_still_lifts_buttons() {
        let device = WorkerFixture::fast();
        let path = device.device();
        let manager = &device.manager;
        manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        manager.closing.store(true, Ordering::Release);
        assert!(matches!(
            manager.mouse_press(path.clone(), RIGHT).await,
            Err(HidError::WorkerStopped)
        ));
        device.assert_silent();
        manager.mouse_release(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [0, 0, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_quick_release_waits_for_the_minimum_hold() {
        assert_eq!(MIN_PRESS_HOLD, Duration::from_millis(default_hold_ms()));
        let hold = Duration::from_millis(120);
        let device = WorkerFixture::with_hold(hold);
        let path = device.device();
        let manager = &device.manager;
        let press = AbsolutePointerRequest::Press { x: 1, y: 2, button: 1 };
        let release = AbsolutePointerRequest::Release { x: 1, y: 2, button: 1 };
        let before_press = std::time::Instant::now();
        manager.mouse_absolute(path.clone(), press).await.unwrap();
        assert_eq!(device.read::<6>().await[0], 1);
        manager.mouse_absolute(path.clone(), release).await.unwrap();
        assert_eq!(device.read::<6>().await[0], 0);
        assert!(before_press.elapsed() >= hold, "{:?}", before_press.elapsed());

        let before_press = std::time::Instant::now();
        manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await[0], 1);
        manager.mouse_release(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await[0], 0);
        assert!(before_press.elapsed() >= hold, "{:?}", before_press.elapsed());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn release_all_does_not_wait_for_the_minimum_hold() {
        let device = WorkerFixture::with_hold(Duration::from_secs(60));
        let path = device.device();
        device.manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        tokio::time::timeout(
            Duration::from_secs(5),
            device.manager.release_all(None, path, None),
        )
        .await
        .expect("an emergency release is immediate")
        .unwrap();
        assert_eq!(device.read::<4>().await, [0, 0, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_button_is_released_but_pointer_traffic_keeps_it_held() {
        let idle = Duration::from_millis(400);
        let device = WorkerFixture::with_idle(idle);
        let path = device.device();
        let manager = &device.manager;
        let press = AbsolutePointerRequest::Press { x: 3, y: 4, button: 1 };
        manager.mouse_absolute(path.clone(), press).await.unwrap();
        assert_eq!(device.read::<6>().await[0], 1);
        // A slow drag: each move arrives well inside the bound.
        for step in 0u16..4 {
            tokio::time::sleep(idle / 2).await;
            let moved = AbsolutePointerRequest::Move { x: 5 + step, y: 4 };
            manager.mouse_absolute(path.clone(), moved).await.unwrap();
            assert_eq!(device.read::<6>().await[0], 1, "still dragging at step {step}");
        }
        let last_input = std::time::Instant::now();
        assert_eq!(device.read::<6>().await, [0, 8, 0, 4, 0, 0], "released in place");
        assert!(last_input.elapsed() >= idle - Duration::from_millis(50), "{:?}", last_input.elapsed());
        // The mask really is clear: the next move carries no button.
        let moved = AbsolutePointerRequest::Move { x: 9, y: 9 };
        manager.mouse_absolute(path.clone(), moved).await.unwrap();
        assert_eq!(device.read::<6>().await, [0, 9, 0, 9, 0, 0]);
        // Nothing further fires once nothing is held.
        tokio::time::sleep(idle + idle / 2).await;
        device.assert_silent();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_idle_release_and_zero_move_as_heartbeat() {
        let idle = Duration::from_millis(400);
        let device = WorkerFixture::with_idle(idle);
        let path = device.device();
        let manager = &device.manager;
        manager.mouse_press(path.clone(), LEFT).await.unwrap();
        assert_eq!(device.read::<4>().await, [1, 0, 0, 0]);
        // The console's heartbeat in relative mode: no report, but it counts.
        for _ in 0..3 {
            tokio::time::sleep(idle / 2).await;
            manager.mouse_move(path.clone(), MouseMoveRequest { dx: 0, dy: 0 }).await.unwrap();
            device.assert_silent();
        }
        assert_eq!(device.read::<4>().await, [0, 0, 0, 0]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_release_is_retried_until_the_target_takes_it() {
        let dir = std::env::temp_dir().join(format!("wingman-hid-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pointer");
        let (mouse_tx, mouse_rx) = mpsc::channel(QUEUE_CAPACITY);
        let manager = manager_for_mouse_queue(mouse_tx);
        tokio::spawn(run_mouse_worker(
            mouse_rx,
            HidDeviceCache::default(),
            Arc::default(),
            PointerTimings {
                min_press_hold: Duration::from_millis(1),
                idle_release: NEVER,
                release_retry: Duration::from_millis(50),
            },
        ));
        // No node yet: the release fails, as it would during a gadget rebind.
        assert!(manager.release_all(None, Some(path.clone()), None).await.is_err());
        // The node comes back: the owed report lands without another request.
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: c_path is a valid NUL-terminated string for the call's duration.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let mut reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let mut report = [0xffu8; 4];
        let mut got = 0;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if let Ok(read) = reader.read(&mut report[got..]) {
                got += read;
            }
            if got == 4 {
                break;
            }
        }
        assert_eq!((got, report), (4, [0; 4]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn release_all_continues_after_an_interface_fails() {
        let (keyboard_tx, keyboard_rx) = mpsc::channel(1);
        drop(keyboard_rx);
        let (mouse_tx, mut mouse_rx) = mpsc::channel(2);
        let (keyboard_path_tx, _) = watch::channel(None);
        let manager = HidManager {
            keyboard_tx,
            mouse_tx,
            keyboard_leds: Arc::new(AtomicU8::new(0)),
            keyboard_path_tx,
            keyboard_link_lost: Arc::default(),
            mouse_link_lost: Arc::default(),
            closing: Arc::default(),
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

    #[tokio::test]
    async fn release_all_waits_for_queue_capacity_instead_of_dropping_the_release() {
        let (keyboard_tx, mut keyboard_rx) = mpsc::channel(1);
        let (mouse_tx, _mouse_rx) = mpsc::channel(1);
        let (keyboard_path_tx, _) = watch::channel(None);
        let manager = HidManager {
            keyboard_tx,
            mouse_tx,
            keyboard_leds: Arc::new(AtomicU8::new(0)),
            keyboard_path_tx,
            keyboard_link_lost: Arc::default(),
            mouse_link_lost: Arc::default(),
            closing: Arc::default(),
        };
        let (dummy_reply, _dummy_response) = oneshot::channel();
        manager
            .keyboard_tx
            .try_send(KeyboardCommand::Key {
                path: PathBuf::from("keyboard"),
                request: KeyRequest {
                    key: "KeyA".into(),
                    ctrl: false,
                    shift: false,
                    alt: false,
                    meta: false,
                    hold_ms: 25,
                },
                reply: dummy_reply,
            })
            .expect("queue has one slot");

        let release = tokio::spawn(async move {
            manager
                .release_all(Some(PathBuf::from("keyboard")), None, None)
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !release.is_finished(),
            "release_all must wait for capacity instead of returning QueueFull"
        );

        match keyboard_rx.recv().await.expect("queued key command") {
            KeyboardCommand::Key { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            _ => panic!("unexpected keyboard command"),
        }
        match keyboard_rx.recv().await.expect("release command") {
            KeyboardCommand::Release { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            _ => panic!("unexpected keyboard command"),
        }

        assert!(release.await.unwrap().is_ok());
    }

    #[cfg(unix)]
    fn cached_test_device(path: PathBuf) -> (HidDeviceCache, std::os::unix::net::UnixStream) {
        use std::os::fd::{FromRawFd, IntoRawFd};

        let (stream, peer) = std::os::unix::net::UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        // SAFETY: into_raw_fd transfers the only owner of this descriptor.
        let file = unsafe { std::fs::File::from_raw_fd(stream.into_raw_fd()) };
        // SAFETY: the File is owned by AsyncFd for its whole registration.
        let fd = unsafe { AsyncFd::register(file) }.unwrap();
        let mut devices = HidDeviceCache::default();
        devices.devices.insert(path, fd);
        (devices, peer)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn consecutive_reports_reuse_the_cached_device_handle() {
        let path = PathBuf::from("/virtual/wingmankvm-hidg0");
        let (mut devices, _peer) = cached_test_device(path.clone());

        send_report(&path, &[0; 4], &mut devices).await.unwrap();
        send_report(&path, &[0; 4], &mut devices).await.unwrap();
        assert_eq!(
            devices.open_count(),
            0,
            "two reports must reuse the already cached handle"
        );

        devices.invalidate(&path);
        assert!(!devices.devices.contains_key(&path));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_failure_discards_the_cached_device_handle() {
        let path = PathBuf::from("/virtual/wingmankvm-hidg0");
        let (mut devices, peer) = cached_test_device(path.clone());
        drop(peer);

        let result = send_report(&path, &[0; 4], &mut devices).await;
        assert!(result.is_err());
        assert!(!devices.devices.contains_key(&path));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn vanished_node_marks_channel_reenumerating_until_a_write_succeeds() {
        let root = crate::devices::hotplug::tests::temp_root("hidg");
        let path = root.join("hidg0");
        let link = Arc::new(AtomicBool::new(false));
        let mut devices = HidDeviceCache::with_link(Arc::clone(&link));

        // Node absent (gadget unbound / re-enumerating): ENOENT on open.
        let error = send_report(&path, &[0; 8], &mut devices).await.unwrap_err();
        assert!(is_device_gone(&error), "{error}");
        assert!(link.load(Ordering::Acquire));
        assert!(!devices.devices.contains_key(&path));

        // The node comes back: the next successful write clears the flag.
        let (mut fresh, _peer) = cached_test_device(path.clone());
        fresh.link_lost = Arc::clone(&link);
        send_report(&path, &[0; 8], &mut fresh).await.unwrap();
        assert!(!link.load(Ordering::Acquire));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reenumerating_mark_clears_when_the_node_returns_without_any_write() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-return");
        let path = root.join("hidg1");
        let manager = HidManager::without_led_reader();

        // The node is gone: the release fails and marks the channel.
        assert!(manager.release_all(None, Some(path.clone()), None).await.is_err());
        assert!(manager.link_status().mouse_reenumerating);
        assert!(!manager.link_status().keyboard_reenumerating);
        tokio::time::sleep(LINK_PROBE_INTERVAL * 2).await;
        assert!(manager.link_status().mouse_reenumerating, "still gone");

        // It comes back, and the user does not type anything.
        let reader = fake_hidg(&path);
        let deadline = Instant::now() + Duration::from_secs(5);
        while manager.link_status().mouse_reenumerating {
            assert!(Instant::now() < deadline, "mark never cleared");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(drain(&reader).is_empty(), "the probe must not send a report");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn present_node_with_the_usb_link_down_stays_reenumerating() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-link-down");
        let path = root.join("hidg0");
        let link = Arc::new(AtomicBool::new(false));
        let host_ready = Arc::new(AtomicU8::new(0));
        let mut devices = HidDeviceCache::with_link(Arc::clone(&link));
        let ready = Arc::clone(&host_ready);
        devices.link_ready = Box::new(move |_| match ready.load(Ordering::Acquire) {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        });

        let error = send_report(&path, &[0; 8], &mut devices).await.unwrap_err();
        assert!(is_device_gone(&error));
        let reader = fake_hidg(&path);

        // Cable out: the node is there and opens, but the host is gone.
        devices.probe_lost();
        assert!(link.load(Ordering::Acquire));
        host_ready.store(1, Ordering::Release);
        devices.probe_lost();
        assert!(!link.load(Ordering::Acquire));
        assert!(drain(&reader).is_empty());

        // Without configfs to ask, a node that opens is taken as back.
        devices.invalidate(&path);
        devices.record(&path, &Err(HidError::Io {
            path: path.clone(),
            source: io::Error::from_raw_os_error(libc::ESHUTDOWN),
        }));
        assert!(link.load(Ordering::Acquire));
        host_ready.store(2, Ordering::Release);
        devices.probe_lost();
        assert!(!link.load(Ordering::Acquire));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn every_lost_node_must_return_before_the_mark_clears() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-two-lost");
        let (relative, absolute) = (root.join("hidg1"), root.join("hidg2"));
        let link = Arc::new(AtomicBool::new(false));
        let mut devices = HidDeviceCache::with_link(Arc::clone(&link));
        devices.link_ready = Box::new(|_| None);
        assert!(send_report(&relative, &[0; 4], &mut devices).await.is_err());
        assert!(send_report(&absolute, &[0; 6], &mut devices).await.is_err());

        let _relative_reader = fake_hidg(&relative);
        devices.probe_lost();
        assert!(link.load(Ordering::Acquire), "the absolute node is still gone");
        let _absolute_reader = fake_hidg(&absolute);
        devices.probe_lost();
        assert!(!link.load(Ordering::Acquire));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enodev_on_cached_fd_invalidates_and_flags_link() {
        let link = Arc::new(AtomicBool::new(false));
        let mut devices = HidDeviceCache::with_link(Arc::clone(&link));
        let path = PathBuf::from("/virtual/hidg1");
        for errno in [libc::ENODEV, libc::ESHUTDOWN] {
            let error = HidError::Io {
                path: path.clone(),
                source: io::Error::from_raw_os_error(errno),
            };
            devices.record(&path, &Err(error));
            assert!(link.swap(false, Ordering::AcqRel), "errno {errno}");
        }
        // A plain timeout is a slow host, not a vanished node.
        devices.record(&path, &Err(HidError::Timeout { path: path.clone() }));
        assert!(!link.load(Ordering::Acquire));
        let eio = HidError::Io {
            path: path.clone(),
            source: io::Error::from_raw_os_error(libc::EIO),
        };
        assert!(!is_device_gone(&eio));
    }

    /// A fake hidg node: a FIFO whose read end the test holds, so a report
    /// written by the workers can be inspected byte for byte.
    #[cfg(unix)]
    pub(crate) fn fake_hidg(path: &Path) -> std::fs::File {
        use std::os::unix::ffi::OsStrExt;

        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `name` is a valid NUL-terminated path for the whole call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0, "{path:?}");
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    fn drain(mut reader: &std::fs::File) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 64];
        while let Ok(count) = reader.read(&mut chunk) {
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        bytes
    }

    fn key(name: &str, hold_ms: u64) -> KeyRequest {
        KeyRequest {
            key: name.to_owned(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            hold_ms,
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn release_writes_an_idle_report_to_every_fake_hidg_node() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-release");
        let (keyboard, mouse, absolute) =
            (root.join("hidg0"), root.join("hidg1"), root.join("hidg2"));
        let readers = [fake_hidg(&keyboard), fake_hidg(&mouse), fake_hidg(&absolute)];

        let manager = HidManager::without_led_reader();
        manager
            .release_all(Some(keyboard), Some(mouse), Some(absolute))
            .await
            .unwrap();

        assert_eq!(drain(&readers[0]), [0_u8; 8]);
        assert_eq!(drain(&readers[1]), [0_u8; 4]);
        // Not all zero on purpose: X/Y are absolute, and 0,0 would throw the
        // pointer into the top-left (hot) corner. The button byte is what
        // releases.
        let absolute_report = AbsolutePointerState::default().report(0);
        assert_eq!(absolute_report[0], 0);
        assert_eq!(drain(&readers[2]), absolute_report);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_release_lets_the_held_key_finish_and_drops_queued_presses() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-shutdown");
        let path = root.join("hidg0");
        let reader = fake_hidg(&path);
        let manager = HidManager::without_led_reader();

        // Being held when the shutdown starts.
        let held = {
            let (manager, path) = (manager.clone(), path.clone());
            tokio::spawn(async move { manager.key(Some(path), key("KeyA", 150)).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(drain(&reader), [0, 0, 0x04, 0, 0, 0, 0, 0], "KeyA is down");
        // Queued behind it, so it would press after the final release.
        let queued = {
            let (manager, path) = (manager.clone(), path.clone());
            tokio::spawn(async move { manager.key(Some(path), key("KeyB", 10)).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;

        manager
            .release_for_shutdown(Some(path.clone()), None, None)
            .await
            .unwrap();

        assert!(held.await.unwrap().is_ok());
        assert!(matches!(queued.await.unwrap(), Err(HidError::WorkerStopped)));
        let late = manager.key(Some(path.clone()), key("KeyC", 10)).await;
        assert!(matches!(late, Err(HidError::WorkerStopped)));
        // KeyA's own release and the final release; never a KeyB or KeyC press.
        assert_eq!(drain(&reader), [0_u8; 16]);

        // Anything racing the exit can still only release.
        manager.release_all(Some(path), None, None).await.unwrap();
        assert_eq!(drain(&reader), [0_u8; 8]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_release_refuses_queued_pointer_input() {
        let root = crate::devices::hotplug::tests::temp_root("hidg-shutdown-mouse");
        let path = root.join("hidg1");
        let reader = fake_hidg(&path);
        let manager = HidManager::new();

        manager
            .release_for_shutdown(None, Some(path.clone()), None)
            .await
            .unwrap();
        assert_eq!(drain(&reader), [0_u8; 4]);
        let click = manager
            .mouse_click(Some(path), MouseClickRequest { button: 1 })
            .await;
        assert!(matches!(click, Err(HidError::WorkerStopped)));
        assert!(drain(&reader).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keyboard_leds_parsing_and_serialization() {
        let leds_zero = KeyboardLeds::from_byte(0);
        assert!(!leds_zero.num_lock);
        assert!(!leds_zero.caps_lock);
        assert!(!leds_zero.scroll_lock);
        assert_eq!(leds_zero.to_byte(), 0);

        let leds_num = KeyboardLeds::from_byte(0x01);
        assert!(leds_num.num_lock);
        assert!(!leds_num.caps_lock);
        assert!(!leds_num.scroll_lock);
        assert_eq!(leds_num.to_byte(), 1);

        let leds_caps = KeyboardLeds::from_byte(0x02);
        assert!(!leds_caps.num_lock);
        assert!(leds_caps.caps_lock);
        assert!(!leds_caps.scroll_lock);
        assert_eq!(leds_caps.to_byte(), 2);

        let leds_scroll = KeyboardLeds::from_byte(0x04);
        assert!(!leds_scroll.num_lock);
        assert!(!leds_scroll.caps_lock);
        assert!(leds_scroll.scroll_lock);
        assert_eq!(leds_scroll.to_byte(), 4);

        let leds_all = KeyboardLeds::from_byte(0x07);
        assert!(leds_all.num_lock);
        assert!(leds_all.caps_lock);
        assert!(leds_all.scroll_lock);
        assert_eq!(leds_all.to_byte(), 7);
    }

    #[tokio::test]
    async fn hid_manager_leds_getter_and_setter() {
        let manager = HidManager::new();
        assert_eq!(manager.leds(), KeyboardLeds::default());
        manager.set_leds_raw(0x02);
        assert!(manager.leds().caps_lock);
        assert!(!manager.leds().num_lock);
        assert_eq!(manager.leds().to_byte(), 2);
    }
}
