//! Optional HDMI audio for the WebRTC transport (issue #71).
//!
//! UVC capture cards usually expose a USB Audio interface next to the video
//! node. This module finds those ALSA capture PCMs by reading `/proc/asound`
//! and `/sys/class/sound` (never by opening the device), runs one shared
//! FFmpeg process that encodes the card's audio to Opus, and fans the Opus
//! packets out to every WebRTC session.
//!
//! Audio is strictly best-effort: the capture process has its own lifecycle,
//! its failures only change the reported audio state, and nothing here can
//! end or restart a video encoder or a peer connection.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use bytes::Bytes;
use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch},
    task::JoinHandle,
};
use webrtc::{media::Sample, track::track_local::track_local_static_sample::TrackLocalStaticSample};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::config::AudioConfig;

use super::webrtc::{EncoderGuard, wait_for_cancel};

/// Packets buffered per viewer; a slow viewer drops old audio, never blocks.
const AUDIO_PACKET_BUFFER: usize = 64;
/// FFmpeg producing no Opus packet for this long is treated as a failure.
const AUDIO_STALL_TIMEOUT: Duration = Duration::from_secs(10);
const AUDIO_RESTART_MIN: Duration = Duration::from_secs(1);
const AUDIO_RESTART_MAX: Duration = Duration::from_secs(30);
/// A run that delivered audio for this long resets the restart backoff.
const AUDIO_HEALTHY_RUN: Duration = Duration::from_secs(10);
/// How often a missing audio device is looked up again while viewers exist.
const AUDIO_DEVICE_RETRY: Duration = Duration::from_secs(3);
/// The previous capture process must release the ALSA device first.
const AUDIO_PREVIOUS_WORKER_WAIT: Duration = Duration::from_secs(5);
const MAX_OGG_BUFFER: usize = 1024 * 1024;
const MAX_OPUS_PACKET: usize = 64 * 1024;
const DEFAULT_OPUS_FRAME: Duration = Duration::from_millis(20);
const NO_DEVICE_MESSAGE: &str = "未检测到音频采集设备";

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// One ALSA sound card from `/proc/asound/cards`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundCard {
    pub index: u32,
    pub id: String,
    pub driver: String,
    pub name: String,
}

/// One PCM from `/proc/asound/pcm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmEntry {
    pub card: u32,
    pub device: u32,
    pub name: String,
    pub capture: bool,
}

/// A capture PCM offered in the settings UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioCandidate {
    /// ALSA name passed to FFmpeg; stable across card re-numbering.
    pub device: String,
    pub label: String,
    pub card_index: u32,
    pub card_id: String,
    pub pcm_device: u32,
    pub driver: String,
    pub usb: bool,
    /// The V4L2 node of the capture card this sound card belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_card: Option<PathBuf>,
}

/// Parse `/proc/asound/cards`:
///
/// ```text
///  1 [MS2109         ]: USB-Audio - MS2109
///                       MACROSILICON MS2109 at usb-xhci-hcd.0.auto-1, high speed
/// ```
pub fn parse_asound_cards(text: &str) -> Vec<SoundCard> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (index, rest) = line.split_once(' ')?;
            let index = index.parse().ok()?;
            let rest = rest.trim_start().strip_prefix('[')?;
            let (id, rest) = rest.split_once(']')?;
            let rest = rest.strip_prefix(':')?.trim();
            let (driver, name) = rest.split_once(" - ").unwrap_or((rest, ""));
            let id = id.trim();
            (!id.is_empty()).then(|| SoundCard {
                index,
                id: id.to_owned(),
                driver: driver.trim().to_owned(),
                name: name.trim().to_owned(),
            })
        })
        .collect()
}

/// Parse `/proc/asound/pcm`:
///
/// ```text
/// 01-00: USB Audio : USB Audio : capture 1
/// 00-00: rt5651-aif1 : rt5651-aif1 : playback 1 : capture 1
/// ```
pub fn parse_asound_pcm(text: &str) -> Vec<PcmEntry> {
    text.lines()
        .filter_map(|line| {
            let (address, rest) = line.split_once(':')?;
            let (card, device) = address.trim().split_once('-')?;
            let mut fields = rest.split(" : ").map(str::trim);
            let name = fields.next().unwrap_or_default().to_owned();
            let capture = fields.any(|field| {
                field
                    .strip_prefix("capture")
                    .and_then(|count| count.trim().parse::<u32>().ok())
                    .is_some_and(|count| count > 0)
            });
            Some(PcmEntry {
                card: card.parse().ok()?,
                device: device.parse().ok()?,
                name,
                capture,
            })
        })
        .collect()
}

/// The USB device directory a sysfs class entry belongs to: `device` points
/// at the USB interface (`1-1:1.2`), its parent is the device (`1-1`).
fn usb_parent(class_entry: &Path) -> Option<PathBuf> {
    let interface = std::fs::canonicalize(class_entry.join("device")).ok()?;
    let parent = interface.parent()?;
    (parent != Path::new("/")).then(|| parent.to_path_buf())
}

fn video_usb_parent(sys_root: &Path, video: &Path) -> Option<PathBuf> {
    // `/dev/v4l/by-id/...` links resolve to `/dev/videoN`; test paths that do
    // not exist are used as given.
    let node = std::fs::canonicalize(video).unwrap_or_else(|_| video.to_path_buf());
    let name = node.file_name()?;
    usb_parent(&sys_root.join("class/video4linux").join(name))
}

fn read_cards(proc_root: &Path) -> Vec<SoundCard> {
    std::fs::read_to_string(proc_root.join("asound/cards"))
        .map(|text| parse_asound_cards(&text))
        .unwrap_or_default()
}

/// List capture PCMs and associate each with a capture card's video node.
///
/// Only procfs and sysfs are read; no ALSA device is opened, so scanning
/// never disturbs a running capture.
pub fn discover(proc_root: &Path, sys_root: &Path, video_devices: &[PathBuf]) -> Vec<AudioCandidate> {
    let cards = read_cards(proc_root);
    let pcms = std::fs::read_to_string(proc_root.join("asound/pcm"))
        .map(|text| parse_asound_pcm(&text))
        .unwrap_or_default();
    let videos: Vec<(PathBuf, PathBuf)> = video_devices
        .iter()
        .filter_map(|video| Some((video.clone(), video_usb_parent(sys_root, video)?)))
        .collect();
    let mut seen = HashSet::new();
    let mut candidates: Vec<AudioCandidate> = pcms
        .into_iter()
        .filter(|pcm| pcm.capture && seen.insert((pcm.card, pcm.device)))
        .filter_map(|pcm| {
            let card = cards.iter().find(|card| card.index == pcm.card)?;
            let parent = usb_parent(&sys_root.join("class/sound").join(format!("card{}", card.index)));
            let capture_card = parent.and_then(|parent| {
                videos
                    .iter()
                    .find(|(_, video_parent)| *video_parent == parent)
                    .map(|(video, _)| video.clone())
            });
            let usb = card.driver.eq_ignore_ascii_case("USB-Audio");
            let title = if card.name.is_empty() { &card.id } else { &card.name };
            Some(AudioCandidate {
                device: format!("plughw:CARD={},DEV={}", card.id, pcm.device),
                label: format!("{title} · {} (card {}, dev {})", pcm.name, card.index, pcm.device),
                card_index: card.index,
                card_id: card.id.clone(),
                pcm_device: pcm.device,
                driver: card.driver.clone(),
                usb,
                capture_card,
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| {
        (
            candidate.capture_card.is_none(),
            !candidate.usb,
            candidate.card_index,
            candidate.pcm_device,
        )
    });
    candidates
}

/// Whether an explicitly configured ALSA name refers to a present card.
pub fn explicit_device_present(device: &str, cards: &[SoundCard]) -> bool {
    let Some((_, address)) = device.split_once(':') else {
        // `default` / `sysdefault` without a card: any card will do.
        return !cards.is_empty();
    };
    let card = address
        .split(',')
        .find_map(|part| part.strip_prefix("CARD="))
        .or_else(|| address.split(',').next().filter(|part| !part.contains('=')));
    match card {
        Some(card) => cards
            .iter()
            .any(|entry| entry.id == card || entry.index.to_string() == card),
        None => !cards.is_empty(),
    }
}

/// The device to capture from: the explicit setting when its card is
/// present, otherwise the card that belongs to the capture card, otherwise
/// the only USB capture PCM. Ambiguity selects nothing rather than a
/// microphone that happens to be plugged in.
pub fn select_device(
    config: &AudioConfig,
    cards: &[SoundCard],
    candidates: &[AudioCandidate],
) -> Option<String> {
    if let Some(device) = config.device.as_deref() {
        return explicit_device_present(device, cards).then(|| device.to_owned());
    }
    if let Some(associated) = candidates.iter().find(|candidate| candidate.capture_card.is_some()) {
        return Some(associated.device.clone());
    }
    let mut usb = candidates.iter().filter(|candidate| candidate.usb);
    match (usb.next(), usb.next()) {
        (Some(only), None) => Some(only.device.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Ogg / Opus framing
// ---------------------------------------------------------------------------

/// Incremental Ogg page reader that yields complete Opus packets.
///
/// FFmpeg writes to a local pipe, so the CRC is not verified; the parser only
/// needs to survive joining mid-stream and resynchronise on `OggS`.
#[derive(Default)]
pub(crate) struct OggPacketParser {
    buffer: Vec<u8>,
    partial: Vec<u8>,
    partial_valid: bool,
}

impl OggPacketParser {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Bytes>, String> {
        self.buffer.extend_from_slice(bytes);
        let mut packets = Vec::new();
        loop {
            match self.buffer.windows(4).position(|window| window == b"OggS") {
                Some(0) => {}
                Some(start) => {
                    self.buffer.drain(..start);
                }
                None => {
                    // Keep a possible partial capture pattern at the end.
                    let keep = self.buffer.len().min(3);
                    self.buffer.drain(..self.buffer.len() - keep);
                    break;
                }
            }
            if self.buffer.len() < 27 {
                break;
            }
            let header_type = self.buffer[5];
            let segments = usize::from(self.buffer[26]);
            let header_len = 27 + segments;
            if self.buffer.len() < header_len {
                break;
            }
            let lacing = self.buffer[27..header_len].to_vec();
            let body_len: usize = lacing.iter().map(|&value| usize::from(value)).sum();
            if self.buffer.len() < header_len + body_len {
                break;
            }
            if header_type & 0x01 == 0 {
                // A fresh packet starts on this page; drop any orphaned tail.
                self.partial.clear();
                self.partial_valid = true;
            }
            let mut offset = header_len;
            for value in lacing {
                let value = usize::from(value);
                if self.partial_valid && self.partial.len() + value <= MAX_OPUS_PACKET {
                    self.partial
                        .extend_from_slice(&self.buffer[offset..offset + value]);
                } else {
                    self.partial_valid = false;
                }
                offset += value;
                if value < 255 {
                    if self.partial_valid && !self.partial.is_empty() {
                        packets.push(Bytes::from(std::mem::take(&mut self.partial)));
                    }
                    self.partial.clear();
                    self.partial_valid = true;
                }
            }
            self.buffer.drain(..header_len + body_len);
        }
        // One Ogg page is at most ~64 KiB, so a larger remainder means the
        // stream is not Ogg at all.
        if self.buffer.len() > MAX_OGG_BUFFER {
            return Err("音频输出缺少 Ogg 页边界".to_owned());
        }
        Ok(packets)
    }
}

/// Opus stream headers are not media.
pub(crate) fn is_opus_header(packet: &[u8]) -> bool {
    packet.starts_with(b"OpusHead") || packet.starts_with(b"OpusTags")
}

/// Packet duration from the Opus TOC byte (RFC 6716 section 3.1).
pub(crate) fn opus_packet_duration(packet: &[u8]) -> Duration {
    let Some(&toc) = packet.first() else {
        return DEFAULT_OPUS_FRAME;
    };
    let config = toc >> 3;
    let frame_us: u64 = match config {
        0..=11 => [10_000, 20_000, 40_000, 60_000][usize::from(config % 4)],
        12..=15 => [10_000, 20_000][usize::from(config % 2)],
        _ => [2_500, 5_000, 10_000, 20_000][usize::from(config % 4)],
    };
    let frames: u64 = match toc & 0x03 {
        0 => 1,
        1 | 2 => 2,
        _ => packet.get(1).map_or(0, |count| u64::from(count & 0x3f)),
    };
    if frames == 0 {
        return DEFAULT_OPUS_FRAME;
    }
    Duration::from_micros((frame_us * frames).min(120_000))
}

// ---------------------------------------------------------------------------
// Shared capture process
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AudioPacket {
    pub data: Bytes,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioState {
    /// Turned off in the configuration (the default).
    Disabled,
    /// Enabled, but no matching ALSA capture device is present.
    NoDevice,
    /// Enabled and a device is present; no WebRTC viewer needs it now.
    Idle,
    Starting,
    Running,
    /// The capture process failed; it is retried while viewers remain.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioStatus {
    pub enabled: bool,
    pub state: AudioState,
    pub device: Option<String>,
    pub message: Option<String>,
    pub viewers: usize,
    pub codec: &'static str,
    pub bitrate_kbps: u32,
    /// Capture process failures since the service started.
    pub restarts: u64,
}

/// Everything the capture process depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSettings {
    pub audio: AudioConfig,
    pub ffmpeg: PathBuf,
    /// The configured capture card, used to pick its sound card.
    pub video_device: Option<PathBuf>,
}

impl AudioSettings {
    pub fn new(audio: AudioConfig, ffmpeg: Option<PathBuf>, video_device: Option<PathBuf>) -> Self {
        Self {
            audio,
            ffmpeg: ffmpeg.unwrap_or_else(|| PathBuf::from("ffmpeg")),
            video_device,
        }
    }
}

struct Worker {
    cancel: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

struct Runtime {
    viewers: usize,
    epoch: u64,
    worker: Option<Worker>,
    /// A cancelled worker that may still hold the ALSA device.
    stopping: Option<JoinHandle<()>>,
    state: AudioState,
    device: Option<String>,
    message: Option<String>,
    restarts: u64,
}

struct Inner {
    settings: Mutex<AudioSettings>,
    runtime: Mutex<Runtime>,
    packets: broadcast::Sender<AudioPacket>,
    proc_root: PathBuf,
    sys_root: PathBuf,
}

impl Inner {
    fn settings(&self) -> AudioSettings {
        self.settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn runtime(&self) -> std::sync::MutexGuard<'_, Runtime> {
        self.runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn resolve(&self, settings: &AudioSettings) -> Option<String> {
        let cards = read_cards(&self.proc_root);
        if settings.audio.device.is_some() {
            return select_device(&settings.audio, &cards, &[]);
        }
        let videos: Vec<PathBuf> = settings.video_device.iter().cloned().collect();
        let candidates = discover(&self.proc_root, &self.sys_root, &videos);
        select_device(&settings.audio, &cards, &candidates)
    }

    /// Publish worker state unless a newer worker has replaced this one.
    fn report(&self, epoch: u64, state: AudioState, device: Option<String>, message: Option<String>) {
        let mut runtime = self.runtime();
        if runtime.epoch != epoch || runtime.worker.is_none() {
            return;
        }
        if state == AudioState::Error {
            runtime.restarts += 1;
        }
        runtime.state = state;
        runtime.device = device;
        runtime.message = message;
    }

    /// Start a worker for the current settings. Called with the lock held.
    fn start_worker(self: &Arc<Self>, runtime: &mut Runtime) {
        let settings = self.settings();
        if !settings.audio.enabled || runtime.worker.is_some() {
            return;
        }
        runtime.epoch += 1;
        runtime.state = AudioState::Starting;
        runtime.message = None;
        let (cancel, cancel_rx) = watch::channel(false);
        let previous = runtime.stopping.take();
        let handle = tokio::spawn(run_worker(
            Arc::downgrade(self),
            runtime.epoch,
            settings,
            cancel_rx,
            previous,
        ));
        runtime.worker = Some(Worker { cancel, handle });
    }

    /// Cancel the worker; the next start waits for it to release the device.
    fn stop_worker(runtime: &mut Runtime) {
        if let Some(worker) = runtime.worker.take() {
            let _ = worker.cancel.send_replace(true);
            // A worker still waiting on its own predecessor finishes only
            // after it, so keeping the newest handle orders the whole chain.
            runtime.stopping = Some(worker.handle);
        }
        runtime.epoch += 1;
        runtime.state = AudioState::Idle;
        runtime.message = None;
    }
}

/// One shared Opus capture for all WebRTC viewers.
#[derive(Clone)]
pub struct AudioManager {
    inner: Arc<Inner>,
}

/// A viewer's packet feed. Dropping the last one stops the capture process.
pub struct AudioSubscription {
    receiver: broadcast::Receiver<AudioPacket>,
    _lease: ViewerLease,
}

struct ViewerLease(Arc<Inner>);

impl Drop for ViewerLease {
    fn drop(&mut self) {
        let mut runtime = self.0.runtime();
        runtime.viewers = runtime.viewers.saturating_sub(1);
        if runtime.viewers == 0 {
            Inner::stop_worker(&mut runtime);
        }
    }
}

impl AudioManager {
    pub fn new(settings: AudioSettings) -> Self {
        Self::with_roots(settings, PathBuf::from("/proc"), PathBuf::from("/sys"))
    }

    pub(crate) fn with_roots(settings: AudioSettings, proc_root: PathBuf, sys_root: PathBuf) -> Self {
        let (packets, _) = broadcast::channel(AUDIO_PACKET_BUFFER);
        let state = if settings.audio.enabled {
            AudioState::Idle
        } else {
            AudioState::Disabled
        };
        Self {
            inner: Arc::new(Inner {
                settings: Mutex::new(settings),
                runtime: Mutex::new(Runtime {
                    viewers: 0,
                    epoch: 0,
                    worker: None,
                    stopping: None,
                    state,
                    device: None,
                    message: None,
                    restarts: 0,
                }),
                packets,
                proc_root,
                sys_root,
            }),
        }
    }

    /// Apply new settings. Viewers keep their subscription: a running capture
    /// is replaced (or stopped when audio was turned off) without touching
    /// any peer connection or video encoder.
    pub fn reconfigure(&self, settings: AudioSettings) {
        let changed = {
            let mut current = self
                .inner
                .settings
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let changed = *current != settings;
            *current = settings;
            changed
        };
        if !changed {
            return;
        }
        let mut runtime = self.inner.runtime();
        Inner::stop_worker(&mut runtime);
        if runtime.viewers > 0 {
            self.inner.start_worker(&mut runtime);
        }
    }

    pub fn enabled(&self) -> bool {
        self.inner
            .settings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .audio
            .enabled
    }

    /// The device a new session would capture from, if audio is enabled and
    /// one is present. Reads procfs/sysfs only.
    pub fn available_device(&self) -> Option<String> {
        let settings = self.inner.settings();
        if !settings.audio.enabled {
            return None;
        }
        self.inner.resolve(&settings)
    }

    /// Join the shared capture; the first viewer starts it.
    pub fn subscribe(&self) -> Option<AudioSubscription> {
        if !self.enabled() {
            return None;
        }
        let mut runtime = self.inner.runtime();
        runtime.viewers += 1;
        let receiver = self.inner.packets.subscribe();
        self.inner.start_worker(&mut runtime);
        Some(AudioSubscription {
            receiver,
            _lease: ViewerLease(Arc::clone(&self.inner)),
        })
    }

    pub fn status(&self) -> AudioStatus {
        let settings = self.inner.settings();
        let (viewers, state, device, message, restarts) = {
            let runtime = self.inner.runtime();
            (
                runtime.viewers,
                runtime.state,
                runtime.device.clone(),
                runtime.message.clone(),
                runtime.restarts,
            )
        };
        let (state, device, message) = if !settings.audio.enabled {
            (AudioState::Disabled, None, None)
        } else if viewers == 0 {
            match self.inner.resolve(&settings) {
                Some(device) => (AudioState::Idle, Some(device), None),
                None => (AudioState::NoDevice, None, Some(NO_DEVICE_MESSAGE.to_owned())),
            }
        } else {
            (state, device, message)
        };
        AudioStatus {
            enabled: settings.audio.enabled,
            state,
            device,
            message,
            viewers,
            codec: "opus",
            bitrate_kbps: settings.audio.bitrate_kbps,
            restarts,
        }
    }
}

async fn cancellable_sleep(cancel: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

async fn run_worker(
    inner: Weak<Inner>,
    epoch: u64,
    settings: AudioSettings,
    mut cancel: watch::Receiver<bool>,
    previous: Option<JoinHandle<()>>,
) {
    if let Some(previous) = previous {
        let _ = tokio::time::timeout(AUDIO_PREVIOUS_WORKER_WAIT, previous).await;
    }
    let mut backoff = AUDIO_RESTART_MIN;
    loop {
        if *cancel.borrow() {
            return;
        }
        let Some(manager) = inner.upgrade() else {
            return;
        };
        let lookup = Arc::clone(&manager);
        let lookup_settings = settings.clone();
        let device = tokio::task::spawn_blocking(move || lookup.resolve(&lookup_settings))
            .await
            .ok()
            .flatten();
        let Some(device) = device else {
            manager.report(epoch, AudioState::NoDevice, None, Some(NO_DEVICE_MESSAGE.to_owned()));
            drop(manager);
            if cancellable_sleep(&mut cancel, AUDIO_DEVICE_RETRY).await {
                return;
            }
            continue;
        };
        manager.report(epoch, AudioState::Starting, Some(device.clone()), None);
        let packets = manager.packets.clone();
        drop(manager);

        let started = Instant::now();
        let report_running = {
            let inner = inner.clone();
            let device = device.clone();
            move || {
                if let Some(manager) = inner.upgrade() {
                    manager.report(epoch, AudioState::Running, Some(device.clone()), None);
                }
            }
        };
        let result = run_capture(&settings, &device, &mut cancel, &packets, report_running).await;
        let message = match result {
            Ok(()) => return,
            Err(message) => message,
        };
        tracing::warn!(%device, %message, "HDMI audio capture stopped; video is unaffected");
        if let Some(manager) = inner.upgrade() {
            manager.report(epoch, AudioState::Error, Some(device), Some(message));
        }
        if started.elapsed() >= AUDIO_HEALTHY_RUN {
            backoff = AUDIO_RESTART_MIN;
        }
        if cancellable_sleep(&mut cancel, backoff).await {
            return;
        }
        backoff = (backoff * 2).min(AUDIO_RESTART_MAX);
    }
}

fn capture_args(settings: &AudioSettings, device: &str) -> Vec<String> {
    let bitrate = format!("{}k", settings.audio.bitrate_kbps);
    [
        "-hide_banner",
        "-loglevel",
        "warning",
        "-nostdin",
        "-f",
        "alsa",
        "-i",
        device,
        "-vn",
        "-ac",
        "2",
        "-ar",
        "48000",
        "-c:a",
        "libopus",
        "-b:a",
        &bitrate,
        "-application",
        "lowdelay",
        "-frame_duration",
        "20",
        "-f",
        "ogg",
        "-page_duration",
        "20000",
        "-flush_packets",
        "1",
        "pipe:1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Run one FFmpeg capture until it fails (`Err`) or is cancelled (`Ok`).
async fn run_capture(
    settings: &AudioSettings,
    device: &str,
    cancel: &mut watch::Receiver<bool>,
    packets: &broadcast::Sender<AudioPacket>,
    on_running: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let mut command = Command::new(&settings.ffmpeg);
    command
        .kill_on_drop(true)
        .args(capture_args(settings, device))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.as_std_mut().process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动音频采集：{error}"))?;
    let Some(mut stdout) = child.stdout.take() else {
        let mut guard = EncoderGuard::new(child, Vec::new());
        guard.shutdown().await;
        return Err("无法连接音频采集输出".to_owned());
    };
    let last_error = Arc::new(Mutex::new(None::<String>));
    let stderr_task = child.stderr.take().map(|stderr| {
        let last_error = Arc::clone(&last_error);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(message = %line, "FFmpeg HDMI audio");
                *last_error.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(line.chars().take(200).collect());
            }
        })
    });
    let packets = packets.clone();
    let mut reader = tokio::spawn(async move {
        let mut parser = OggPacketParser::default();
        let mut chunk = vec![0_u8; 16 * 1024];
        let mut on_running = Some(on_running);
        loop {
            let read = tokio::time::timeout(AUDIO_STALL_TIMEOUT, stdout.read(&mut chunk))
                .await
                .map_err(|_| {
                    format!("音频采集超过 {} 秒没有数据", AUDIO_STALL_TIMEOUT.as_secs())
                })?
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Ok::<(), String>(());
            }
            for packet in parser.push(&chunk[..read])? {
                if is_opus_header(&packet) {
                    continue;
                }
                if let Some(report) = on_running.take() {
                    report();
                }
                let duration = opus_packet_duration(&packet);
                // No receivers is not an error: viewers come and go.
                let _ = packets.send(AudioPacket {
                    data: packet,
                    duration,
                });
            }
        }
    });
    let mut tasks = vec![reader.abort_handle()];
    if let Some(task) = &stderr_task {
        tasks.push(task.abort_handle());
    }
    let mut process = EncoderGuard::new(child, tasks);
    let result = tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => Ok(()),
        result = &mut reader => {
            let exited = process.wait_for_exit(Duration::from_millis(500)).await;
            if let Some(task) = stderr_task {
                let _ = tokio::time::timeout(Duration::from_millis(200), task).await;
            }
            let detail = last_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            Err(match (result, detail) {
                (Ok(Err(error)), _) => error,
                (_, Some(detail)) => format!("音频采集已退出：{detail}"),
                (_, None) => match exited {
                    Some(status) => format!("音频采集已退出（{status}）"),
                    None => "音频采集已退出".to_owned(),
                },
            })
        }
    };
    process.shutdown().await;
    result
}

/// Copy the shared Opus feed into one session's audio track until the task
/// is aborted. Write errors are ignored: a stalled audio track must never be
/// able to end the session that also carries video.
pub async fn forward_to_track(mut subscription: AudioSubscription, track: Arc<TrackLocalStaticSample>) {
    loop {
        match subscription.receiver.recv().await {
            Ok(packet) => {
                let _ = track
                    .write_sample(&Sample {
                        data: packet.data,
                        duration: packet.duration,
                        ..Default::default()
                    })
                    .await;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const CARDS: &str = concat!(
        " 0 [rockchiprk3399 ]: rockchip_rk3399 - rockchip,rk3399\n",
        "                      rockchip,rk3399\n",
        " 1 [MS2109         ]: USB-Audio - MS2109\n",
        "                      MACROSILICON MS2109 at usb-xhci-hcd.0.auto-1, high speed\n",
        " 2 [Mic            ]: USB-Audio - USB PnP Sound Device\n",
        "                      C-Media USB PnP Sound Device at usb-ff5c0000.usb-1, full speed\n",
        " 3 [HDMI           ]: HDMI - HDMI playback\n",
    );
    const PCM: &str = concat!(
        "00-00: ff880000.i2s-rt5651-aif1 rt5651-aif1-0 : ff880000.i2s-rt5651-aif1 rt5651-aif1-0 : playback 1 : capture 1\n",
        "01-00: USB Audio : USB Audio : capture 1\n",
        "02-00: USB Audio : USB Audio : playback 1 : capture 1\n",
        "03-00: HDMI : HDMI : playback 1\n",
    );

    pub(crate) struct TempTree(pub(crate) PathBuf);

    impl TempTree {
        pub(crate) fn new(tag: &str) -> Self {
            let mut random = [0_u8; 8];
            getrandom::fill(&mut random).unwrap();
            let path = std::env::temp_dir().join(format!(
                "wingmankvm-audio-{tag}-{}-{:016x}",
                std::process::id(),
                u64::from_ne_bytes(random)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        pub(crate) fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// procfs listing a single USB capture card at index 1.
    pub(crate) fn single_card_proc(tag: &str) -> TempTree {
        let proc = TempTree::new(tag);
        proc.write(
            "asound/cards",
            " 1 [MS2109         ]: USB-Audio - MS2109\n                      MACROSILICON MS2109\n",
        );
        proc.write("asound/pcm", "01-00: USB Audio : USB Audio : capture 1\n");
        proc
    }

    #[test]
    fn parses_proc_asound_cards_and_pcm() {
        let cards = parse_asound_cards(CARDS);
        assert_eq!(cards.len(), 4);
        assert_eq!(
            cards[1],
            SoundCard {
                index: 1,
                id: "MS2109".into(),
                driver: "USB-Audio".into(),
                name: "MS2109".into(),
            }
        );
        assert_eq!(cards[3].id, "HDMI");
        assert!(parse_asound_cards("--- no soundcards ---\n").is_empty());

        let pcm = parse_asound_pcm(PCM);
        assert_eq!(pcm.len(), 4);
        assert!(pcm[0].capture && pcm[1].capture && pcm[2].capture);
        assert!(!pcm[3].capture, "playback-only HDMI output is not a capture source");
        assert_eq!((pcm[1].card, pcm[1].device, pcm[1].name.as_str()), (1, 0, "USB Audio"));
        assert!(parse_asound_pcm("garbage\n01-xx: a : capture 1\n").is_empty());
    }

    #[cfg(unix)]
    fn usb_tree(sys: &TempTree) {
        use std::os::unix::fs::symlink;
        let devices = sys.0.join("devices/usb1");
        for interface in ["1-1/1-1:1.0", "1-1/1-1:1.2", "1-2/1-2:1.0", "soc/i2s"] {
            std::fs::create_dir_all(devices.join(interface)).unwrap();
        }
        for (class, target) in [
            ("class/video4linux/video0", "1-1/1-1:1.0"),
            ("class/sound/card1", "1-1/1-1:1.2"),
            ("class/sound/card2", "1-2/1-2:1.0"),
            ("class/sound/card0", "soc/i2s"),
        ] {
            let entry = sys.0.join(class);
            std::fs::create_dir_all(&entry).unwrap();
            symlink(devices.join(target), entry.join("device")).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn discovery_associates_the_sound_card_of_the_capture_card() {
        let proc = TempTree::new("proc");
        proc.write("asound/cards", CARDS);
        proc.write("asound/pcm", PCM);
        let sys = TempTree::new("sys");
        usb_tree(&sys);

        let candidates = discover(&proc.0, &sys.0, &[PathBuf::from("/dev/video0")]);
        let devices: Vec<&str> = candidates.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(
            devices,
            [
                "plughw:CARD=MS2109,DEV=0",
                "plughw:CARD=Mic,DEV=0",
                "plughw:CARD=rockchiprk3399,DEV=0",
            ]
        );
        assert_eq!(candidates[0].capture_card.as_deref(), Some(Path::new("/dev/video0")));
        assert!(candidates[0].usb && candidates[1].usb && !candidates[2].usb);
        assert!(candidates[1].capture_card.is_none());
        assert!(candidates[0].label.contains("MS2109"));

        let auto = AudioConfig { enabled: true, ..AudioConfig::default() };
        let cards = parse_asound_cards(CARDS);
        assert_eq!(
            select_device(&auto, &cards, &candidates).as_deref(),
            Some("plughw:CARD=MS2109,DEV=0")
        );
        // Without the association two USB cards are ambiguous.
        let unassociated = discover(&proc.0, &sys.0, &[]);
        assert!(unassociated.iter().all(|candidate| candidate.capture_card.is_none()));
        assert_eq!(select_device(&auto, &cards, &unassociated), None);
    }

    #[test]
    fn discovery_without_alsa_reports_nothing() {
        let proc = TempTree::new("empty-proc");
        let sys = TempTree::new("empty-sys");
        assert!(discover(&proc.0, &sys.0, &[PathBuf::from("/dev/video0")]).is_empty());
        let auto = AudioConfig { enabled: true, ..AudioConfig::default() };
        assert_eq!(select_device(&auto, &[], &[]), None);
    }

    #[test]
    fn single_usb_card_is_selected_and_explicit_names_need_their_card() {
        let proc = single_card_proc("single");
        let sys = TempTree::new("single-sys");
        let candidates = discover(&proc.0, &sys.0, &[]);
        let cards = read_cards(&proc.0);
        let auto = AudioConfig { enabled: true, ..AudioConfig::default() };
        assert_eq!(
            select_device(&auto, &cards, &candidates).as_deref(),
            Some("plughw:CARD=MS2109,DEV=0")
        );

        for (device, present) in [
            ("hw:1,0", true),
            ("plughw:CARD=MS2109,DEV=0", true),
            ("hw:CARD=MS2109", true),
            ("hw:2,0", false),
            ("plughw:CARD=Other,DEV=0", false),
            ("default", true),
        ] {
            let explicit = AudioConfig {
                enabled: true,
                device: Some(device.to_owned()),
                ..AudioConfig::default()
            };
            assert_eq!(
                select_device(&explicit, &cards, &candidates).is_some(),
                present,
                "{device}"
            );
        }
        assert!(!explicit_device_present("default", &[]));
    }

    pub(crate) fn ogg_page(header_type: u8, sequence: u32, packets: &[&[u8]]) -> Vec<u8> {
        let mut lacing = Vec::new();
        let mut body = Vec::new();
        for packet in packets {
            let mut remaining = packet.len();
            loop {
                let value = remaining.min(255);
                lacing.push(value as u8);
                remaining -= value;
                if value < 255 {
                    break;
                }
            }
            body.extend_from_slice(packet);
        }
        let mut page = b"OggS".to_vec();
        page.push(0);
        page.push(header_type);
        page.extend_from_slice(&0_u64.to_le_bytes());
        page.extend_from_slice(&7_u32.to_le_bytes());
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.push(lacing.len() as u8);
        page.extend_from_slice(&lacing);
        page.extend_from_slice(&body);
        page
    }

    /// An Ogg Opus stream: headers, then `count` 20 ms CELT packets.
    pub(crate) fn opus_stream(count: u32) -> Vec<u8> {
        let mut stream = ogg_page(0x02, 0, &[b"OpusHead\x01\x02\x38\x01\x80\xbb\0\0\0\0\0"]);
        stream.extend(ogg_page(0, 1, &[b"OpusTags\0\0\0\0\0\0\0\0"]));
        for sequence in 0..count {
            // TOC 0xfc: config 31 (CELT 20 ms), one frame.
            stream.extend(ogg_page(0, sequence + 2, &[&[0xfc, 0x01, sequence as u8]]));
        }
        stream
    }

    #[test]
    fn ogg_parser_extracts_packets_across_chunks_and_pages() {
        let big = vec![7_u8; 600];
        let mut stream = b"garbage".to_vec();
        stream.extend(opus_stream(2));
        // A 600-byte packet split over two pages via the continuation flag.
        let first = ogg_page(0, 9, &[&big[..510]]);
        // `ogg_page` terminates the packet; rebuild the lacing to continue it.
        let mut first = first;
        let segments = usize::from(first[26]);
        assert_eq!(&first[27..27 + segments], &[255, 255, 0]);
        first[26] = 2;
        first.remove(29);
        stream.extend(first);
        stream.extend(ogg_page(0x01, 10, &[&big[510..]]));

        let mut parser = OggPacketParser::default();
        let mut packets = Vec::new();
        for chunk in stream.chunks(5) {
            packets.extend(parser.push(chunk).unwrap());
        }
        let media: Vec<_> = packets.iter().filter(|packet| !is_opus_header(packet)).collect();
        assert_eq!(packets.len(), 5, "two headers, two frames and one large packet");
        assert_eq!(media.len(), 3);
        assert_eq!(media[0].as_ref(), &[0xfc, 0x01, 0]);
        assert_eq!(media[2].len(), 600);
    }

    #[test]
    fn ogg_parser_drops_a_continuation_without_its_start() {
        let mut parser = OggPacketParser::default();
        let orphan = ogg_page(0x01, 3, &[&[1, 2, 3]]);
        assert!(parser.push(&orphan).unwrap().is_empty());
        let next = ogg_page(0, 4, &[&[0xfc, 9]]);
        assert_eq!(parser.push(&next).unwrap(), vec![Bytes::from_static(&[0xfc, 9])]);
        assert!(parser.push(&vec![0_u8; MAX_OGG_BUFFER + 1]).is_ok(), "noise is discarded");
    }

    #[test]
    fn opus_duration_follows_the_toc_byte() {
        assert_eq!(opus_packet_duration(&[0xfc]), Duration::from_millis(20));
        assert_eq!(opus_packet_duration(&[0xf8 | 0x01]), Duration::from_millis(40));
        assert_eq!(opus_packet_duration(&[0x08]), Duration::from_millis(20), "SILK 20 ms");
        assert_eq!(opus_packet_duration(&[0x03 | 0x18, 0x03]), Duration::from_millis(120), "capped");
        assert_eq!(opus_packet_duration(&[0x80]), Duration::from_micros(2_500));
        assert_eq!(opus_packet_duration(&[]), DEFAULT_OPUS_FRAME);
        assert_eq!(opus_packet_duration(&[0x03]), DEFAULT_OPUS_FRAME, "missing frame count");
    }

    #[test]
    fn capture_arguments_encode_opus_from_alsa() {
        let settings = AudioSettings::new(
            AudioConfig { enabled: true, bitrate_kbps: 128, ..AudioConfig::default() },
            None,
            None,
        );
        let args = capture_args(&settings, "plughw:CARD=MS2109,DEV=0").join(" ");
        assert!(args.contains("-f alsa -i plughw:CARD=MS2109,DEV=0"), "{args}");
        assert!(args.contains("-c:a libopus -b:a 128k"), "{args}");
        assert!(args.ends_with("-f ogg -page_duration 20000 -flush_packets 1 pipe:1"), "{args}");
        assert_eq!(settings.ffmpeg, PathBuf::from("ffmpeg"));
    }

    #[test]
    fn disabled_audio_never_subscribes_or_reports_a_device() {
        let proc = single_card_proc("disabled");
        let sys = TempTree::new("disabled-sys");
        let manager = AudioManager::with_roots(
            AudioSettings::new(AudioConfig::default(), None, None),
            proc.0.clone(),
            sys.0.clone(),
        );
        assert!(manager.subscribe().is_none());
        assert!(manager.available_device().is_none());
        let status = manager.status();
        assert_eq!(status.state, AudioState::Disabled);
        assert!(!status.enabled);
        assert_eq!(status.viewers, 0);
    }

    #[test]
    fn enabled_audio_reports_idle_or_no_device_without_viewers() {
        let proc = single_card_proc("idle");
        let sys = TempTree::new("idle-sys");
        let enabled = AudioConfig { enabled: true, ..AudioConfig::default() };
        let manager = AudioManager::with_roots(
            AudioSettings::new(enabled.clone(), None, None),
            proc.0.clone(),
            sys.0.clone(),
        );
        let status = manager.status();
        assert_eq!(status.state, AudioState::Idle);
        assert_eq!(status.device.as_deref(), Some("plughw:CARD=MS2109,DEV=0"));
        assert_eq!(manager.available_device().as_deref(), Some("plughw:CARD=MS2109,DEV=0"));

        let empty = TempTree::new("idle-empty");
        let missing = AudioManager::with_roots(
            AudioSettings::new(enabled, None, None),
            empty.0.clone(),
            sys.0.clone(),
        );
        let status = missing.status();
        assert_eq!(status.state, AudioState::NoDevice);
        assert_eq!(status.message.as_deref(), Some(NO_DEVICE_MESSAGE));
        assert!(missing.available_device().is_none());
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "no_device");
    }

    #[cfg(unix)]
    mod capture {
        use super::*;
        use crate::devices::webrtc::tests::{
            FIXTURE_POLL_TIMEOUT, ProbeFixture, process_exists, serial_probe_fixture,
            shell_quote, wait_for_fixture_exit, wait_for_fixture_pid,
        };

        fn enabled_settings(script: &Path) -> AudioSettings {
            AudioSettings::new(
                AudioConfig { enabled: true, ..AudioConfig::default() },
                Some(script.to_path_buf()),
                None,
            )
        }

        /// Fake FFmpeg that logs its pid, emits an Ogg Opus stream and stays up.
        fn streaming_script(fixture: &ProbeFixture, pids: &Path) -> PathBuf {
            let stream = fixture.path("stream.ogg");
            std::fs::write(&stream, opus_stream(40)).unwrap();
            fixture.script(
                "ffmpeg-audio",
                &format!(
                    "echo $$ >> {}\ncat {}\nexec sleep 30",
                    shell_quote(pids),
                    shell_quote(&stream)
                ),
            )
        }

        async fn wait_for_state(manager: &AudioManager, state: AudioState) -> AudioStatus {
            let deadline = Instant::now() + FIXTURE_POLL_TIMEOUT;
            loop {
                let status = manager.status();
                if status.state == state {
                    return status;
                }
                assert!(Instant::now() < deadline, "audio stayed {:?}", status);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        async fn next_media(subscription: &mut AudioSubscription) -> AudioPacket {
            tokio::time::timeout(FIXTURE_POLL_TIMEOUT, subscription.receiver.recv())
                .await
                .expect("audio packet")
                .expect("open audio feed")
        }

        #[tokio::test]
        // The std guard only serialises fixture-spawning tests.
        #[allow(clippy::await_holding_lock)]
        async fn one_capture_is_shared_and_stops_with_the_last_viewer() {
            let _serial = serial_probe_fixture();
            let fixture = ProbeFixture::new();
            let pids = fixture.path("audio.pids");
            let script = streaming_script(&fixture, &pids);
            let proc = single_card_proc("shared");
            let sys = TempTree::new("shared-sys");
            let manager =
                AudioManager::with_roots(enabled_settings(&script), proc.0.clone(), sys.0.clone());

            let mut first = manager.subscribe().expect("enabled audio");
            let mut second = manager.subscribe().expect("enabled audio");
            assert_eq!(manager.status().viewers, 2);
            let packet = next_media(&mut first).await;
            assert_eq!(packet.data[0], 0xfc);
            assert_eq!(packet.duration, Duration::from_millis(20));
            assert_eq!(next_media(&mut second).await.data, packet.data);
            let status = wait_for_state(&manager, AudioState::Running).await;
            assert_eq!(status.device.as_deref(), Some("plughw:CARD=MS2109,DEV=0"));

            let pid = wait_for_fixture_pid(&pids).await;
            drop(first);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(process_exists(pid), "a remaining viewer keeps the capture");
            drop(second);
            wait_for_fixture_exit(pid).await;
            assert_eq!(manager.status().state, AudioState::Idle);
            assert_eq!(
                std::fs::read_to_string(&pids).unwrap().lines().count(),
                1,
                "two viewers share one capture process"
            );
        }

        #[tokio::test]
        #[allow(clippy::await_holding_lock)]
        async fn a_failing_capture_reports_an_error_and_is_retried() {
            let _serial = serial_probe_fixture();
            let fixture = ProbeFixture::new();
            let runs = fixture.path("runs");
            let script = fixture.script(
                "ffmpeg-broken",
                &format!(
                    "echo run >> {}\necho 'cannot open audio device: Device or resource busy' >&2\nexit 1",
                    shell_quote(&runs)
                ),
            );
            let proc = single_card_proc("failing");
            let sys = TempTree::new("failing-sys");
            let manager =
                AudioManager::with_roots(enabled_settings(&script), proc.0.clone(), sys.0.clone());
            let subscription = manager.subscribe().unwrap();
            let status = wait_for_state(&manager, AudioState::Error).await;
            assert!(
                status.message.as_deref().unwrap_or_default().contains("Device or resource busy"),
                "{status:?}"
            );
            assert!(status.restarts >= 1);
            // The backoff retries the process while the viewer stays.
            let deadline = Instant::now() + FIXTURE_POLL_TIMEOUT;
            while std::fs::read_to_string(&runs).unwrap_or_default().lines().count() < 2 {
                assert!(Instant::now() < deadline, "capture was not retried");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            drop(subscription);
            assert_eq!(manager.status().state, AudioState::Idle);
        }

        #[tokio::test]
        #[allow(clippy::await_holding_lock)]
        async fn disabling_audio_stops_capture_but_keeps_viewers_subscribed() {
            let _serial = serial_probe_fixture();
            let fixture = ProbeFixture::new();
            let pids = fixture.path("audio.pids");
            let script = streaming_script(&fixture, &pids);
            let proc = single_card_proc("disable");
            let sys = TempTree::new("disable-sys");
            let settings = enabled_settings(&script);
            let manager =
                AudioManager::with_roots(settings.clone(), proc.0.clone(), sys.0.clone());
            let mut subscription = manager.subscribe().unwrap();
            next_media(&mut subscription).await;
            let pid = wait_for_fixture_pid(&pids).await;

            let mut disabled = settings.clone();
            disabled.audio.enabled = false;
            manager.reconfigure(disabled);
            wait_for_fixture_exit(pid).await;
            assert_eq!(manager.status().state, AudioState::Disabled);
            assert!(manager.subscribe().is_none());

            // Re-enabling restarts capture for the viewer that stayed.
            manager.reconfigure(settings);
            wait_for_state(&manager, AudioState::Running).await;
            assert_eq!(std::fs::read_to_string(&pids).unwrap().lines().count(), 2);
            drop(subscription);
        }

        #[tokio::test]
        #[allow(clippy::await_holding_lock)]
        async fn a_missing_device_never_spawns_ffmpeg() {
            let _serial = serial_probe_fixture();
            let fixture = ProbeFixture::new();
            let marker = fixture.path("spawned");
            let script =
                fixture.script("ffmpeg-marker", &format!("touch {}", shell_quote(&marker)));
            let proc = TempTree::new("nodevice");
            let sys = TempTree::new("nodevice-sys");
            let manager =
                AudioManager::with_roots(enabled_settings(&script), proc.0.clone(), sys.0.clone());
            let subscription = manager.subscribe().unwrap();
            let status = wait_for_state(&manager, AudioState::NoDevice).await;
            assert_eq!(status.viewers, 1);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(!marker.exists());
            drop(subscription);
        }
    }
}
