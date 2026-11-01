use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use serde::Serialize;
use tokio::sync::watch;

use crate::config::{VideoConfig, VideoEncoding};

/// A camera may legitimately deliver only one frame per second. Waiting five
/// seconds for a valid JPEG therefore still tolerates a 1 fps source while
/// surfacing a genuinely dead or malformed stream.
const FRAME_STALL_TIMEOUT: Duration = Duration::from_secs(5);

/// While the capture node is missing the supervisor blocks on the hot-plug
/// watcher in slices this long, so a reconfigure is still noticed promptly.
#[cfg(target_os = "linux")]
const DEVICE_WAIT_SLICE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub jpeg: Bytes,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[serde(rename_all = "snake_case")]
pub enum VideoState {
    Unconfigured,
    Starting,
    Ready,
    Stalled,
    Offline,
    #[cfg(not(target_os = "linux"))]
    Unsupported,
}

/// Machine-readable cause for a non-ready state, so the UI can show a
/// specific message instead of parsing `message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoReason {
    /// The configured device node (or its by-id symlink target) is absent:
    /// the capture card was unplugged or is re-enumerating.
    DeviceMissing,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoStatus {
    pub state: VideoState,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<VideoReason>,
    pub device: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frames_per_second: Option<f64>,
    pub encoding: VideoEncoding,
    /// Wall-clock time of the last valid JPEG frame, if one has arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_frame_unix_ms: Option<u64>,
    /// Age of the last valid frame at the time this status was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_frame_age_ms: Option<u64>,
}

impl VideoStatus {
    fn from_config(config: &VideoConfig, state: VideoState, message: Option<String>) -> Self {
        Self {
            state,
            message,
            reason: None,
            device: config.device.clone(),
            width: config.width,
            height: config.height,
            frames_per_second: config.frames_per_second.map(f64::from),
            encoding: config.encoding,
            last_frame_unix_ms: None,
            last_frame_age_ms: None,
        }
    }
}

/// What a settings change from one video configuration to the next requires
/// of the running capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureChange {
    /// Nothing the capture uses changed (for example only the H.264 section).
    Unchanged,
    /// Only per-frame processing changed (`encoding`, `jpeg_quality`): applied
    /// to the next frame without closing the device or interrupting viewers.
    /// With MJPEG passthrough the quality is not used at all.
    Live,
    /// The device or the format negotiated with it changed: reopen it.
    Restart,
}

pub fn capture_change(previous: &VideoConfig, next: &VideoConfig) -> CaptureChange {
    if previous.device != next.device
        || previous.width != next.width
        || previous.height != next.height
        || previous.frames_per_second != next.frames_per_second
    {
        CaptureChange::Restart
    } else if previous.encoding != next.encoding || previous.jpeg_quality != next.jpeg_quality {
        CaptureChange::Live
    } else {
        CaptureChange::Unchanged
    }
}

/// Applies `next` to a running session in place when it can; returns it when
/// the session has to be restarted instead.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn apply_to_session(current: &mut VideoConfig, next: VideoConfig) -> Option<VideoConfig> {
    if capture_change(current, &next) == CaptureChange::Restart {
        Some(next)
    } else {
        *current = next;
        None
    }
}

#[derive(Clone)]
pub struct VideoManager {
    command_tx: mpsc::Sender<VideoCommand>,
    frame_tx: watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: watch::Sender<VideoStatus>,
    /// The test source's command queue, so tests can see what was sent.
    #[cfg(test)]
    test_commands: Option<Arc<std::sync::Mutex<mpsc::Receiver<VideoCommand>>>>,
}

impl VideoManager {
    pub fn new(initial: VideoConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (frame_tx, _) = watch::channel(None);
        let (status_tx, _) = watch::channel(VideoStatus::from_config(
            &initial,
            VideoState::Starting,
            None,
        ));

        let thread_frames = frame_tx.clone();
        let thread_status = status_tx.clone();
        std::thread::Builder::new()
            .name("wingmankvm-video".into())
            .spawn(move || video_supervisor(initial, command_rx, thread_frames, thread_status))
            .expect("failed to start video capture thread");

        Self {
            command_tx,
            frame_tx,
            status_tx,
            #[cfg(test)]
            test_commands: None,
        }
    }

    /// Test-only frame source used by sibling lifecycle tests. It deliberately
    /// starts no capture thread and owns no hardware.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn test_source(
        initial: VideoConfig,
    ) -> (Self, watch::Sender<Option<Arc<VideoFrame>>>) {
        let (command_tx, command_rx) = mpsc::channel();
        let (frame_tx, _) = watch::channel(None);
        let (status_tx, _) = watch::channel(VideoStatus::from_config(
            &initial,
            VideoState::Starting,
            None,
        ));
        let manager = Self {
            command_tx,
            frame_tx: frame_tx.clone(),
            status_tx,
            test_commands: Some(Arc::new(std::sync::Mutex::new(command_rx))),
        };
        (manager, frame_tx)
    }

    /// Configurations sent to the test source since the last call.
    #[cfg(test)]
    pub(crate) fn take_test_reconfigures(&self) -> Vec<VideoConfig> {
        let commands = self.test_commands.as_ref().expect("a test source");
        let commands = commands.lock().unwrap();
        std::iter::from_fn(|| commands.try_recv().ok())
            .map(|VideoCommand::Reconfigure(config)| config)
            .collect()
    }

    /// Hands a new configuration to the capture thread. A running session
    /// applies a [`CaptureChange::Live`] change in place and only restarts
    /// for a [`CaptureChange::Restart`] one.
    pub fn reconfigure(&self, config: VideoConfig) -> Result<(), String> {
        self.command_tx
            .send(VideoCommand::Reconfigure(config))
            .map_err(|_| "video capture thread stopped".to_string())
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<VideoFrame>>> {
        self.frame_tx.subscribe()
    }

    pub fn status(&self) -> VideoStatus {
        let mut status = self.status_tx.borrow().clone();
        // Recompute from the wall clock on every read so polling observes a
        // growing age instead of a value frozen at the last publish.
        status.last_frame_age_ms = status.last_frame_unix_ms.and_then(frame_age_ms_now);
        status
    }
}

enum VideoCommand {
    Reconfigure(VideoConfig),
}

#[cfg(target_os = "linux")]
enum SessionExit {
    Reconfigure(VideoConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
enum CapturePhase {
    Starting,
    Ready,
    Stalled,
}

/// Monotonic capture-health tracker. Tests inject `Instant` values so the
/// timeout path is exercised without sleeping or touching a real device.
#[derive(Debug, Clone)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
struct CaptureHealth {
    phase: CapturePhase,
    started_at: Instant,
    last_frame_at: Option<Instant>,
    last_frame_unix_ms: Option<u64>,
}

impl CaptureHealth {
    fn new(now: Instant) -> Self {
        Self {
            phase: CapturePhase::Starting,
            started_at: now,
            last_frame_at: None,
            last_frame_unix_ms: None,
        }
    }

    fn on_valid_frame(&mut self, now: Instant) {
        self.last_frame_at = Some(now);
        self.last_frame_unix_ms = unix_ms_now();
        self.phase = CapturePhase::Ready;
    }

    /// Ticks the timeout tracker for timeouts and every invalid-frame path.
    /// Returns true exactly once, when the stream transitions to stalled.
    fn tick(&mut self, now: Instant) -> bool {
        if self.phase == CapturePhase::Stalled {
            return false;
        }
        let reference = self.last_frame_at.unwrap_or(self.started_at);
        if now.saturating_duration_since(reference) >= FRAME_STALL_TIMEOUT {
            self.phase = CapturePhase::Stalled;
            true
        } else {
            false
        }
    }

    fn last_frame_age_ms(&self, now: Instant) -> Option<u64> {
        self.last_frame_at.map(|at| {
            u64::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u64::MAX)
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
struct VideoGeometry {
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
}

/// Single publish path for capture health used by the Linux capture loop and
/// unit tests. Starting/Stalled states clear the cached frame so no consumer
/// can mistake an old image for a live one.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn publish_health(
    frame_tx: &watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: &watch::Sender<VideoStatus>,
    config: &VideoConfig,
    health: &CaptureHealth,
    now: Instant,
    geometry: VideoGeometry,
    starting_message: Option<&str>,
) {
    let (state, message) = match health.phase {
        CapturePhase::Starting => (
            VideoState::Starting,
            Some(starting_message.unwrap_or("正在等待首帧…").to_owned()),
        ),
        CapturePhase::Ready => (VideoState::Ready, None),
        CapturePhase::Stalled => (
            VideoState::Stalled,
            Some(format!(
                "超过 {} 秒未收到有效视频帧",
                FRAME_STALL_TIMEOUT.as_secs()
            )),
        ),
    };
    if matches!(health.phase, CapturePhase::Starting | CapturePhase::Stalled) {
        frame_tx.send_replace(None);
    }

    let mut status = VideoStatus::from_config(config, state, message);
    status.width = geometry.width;
    status.height = geometry.height;
    status.frames_per_second = if health.phase == CapturePhase::Ready {
        geometry.fps
    } else {
        None
    };
    status.last_frame_unix_ms = health.last_frame_unix_ms;
    status.last_frame_age_ms = health.last_frame_age_ms(now);
    status_tx.send_replace(status);
}

fn unix_ms_now() -> Option<u64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    u64::try_from(millis).ok()
}

fn frame_age_ms_now(last_frame_unix_ms: u64) -> Option<u64> {
    Some(unix_ms_now()?.saturating_sub(last_frame_unix_ms))
}

#[derive(Debug)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
enum DeviceWait {
    Present,
    Reconfigure(VideoConfig),
    Stopped,
}

/// Blocks while the configured capture node is absent, publishing a distinct
/// `device_missing` state, and returns as soon as the hot-plug watcher sees
/// it reappear (or a reconfigure / shutdown arrives).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn wait_for_device(
    config: &VideoConfig,
    device: &std::path::Path,
    command_rx: &mpsc::Receiver<VideoCommand>,
    frame_tx: &watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: &watch::Sender<VideoStatus>,
    slice: Duration,
) -> DeviceWait {
    use super::hotplug::{Presence, presence, wait_until_present};

    if presence(device) == Presence::Present {
        return DeviceWait::Present;
    }
    tracing::warn!(device = %device.display(), "capture device missing; waiting for it to reappear");
    frame_tx.send_replace(None);
    let mut status = VideoStatus::from_config(
        config,
        VideoState::Offline,
        Some(format!("采集卡已断开：未找到 {}", device.display())),
    );
    status.reason = Some(VideoReason::DeviceMissing);
    status_tx.send_replace(status);
    loop {
        if wait_until_present(device, slice) == Presence::Present {
            tracing::info!(device = %device.display(), "capture device reappeared");
            return DeviceWait::Present;
        }
        match command_rx.try_recv() {
            Ok(VideoCommand::Reconfigure(next)) => return DeviceWait::Reconfigure(next),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => return DeviceWait::Stopped,
        }
    }
}

#[cfg(target_os = "linux")]
fn video_supervisor(
    mut config: VideoConfig,
    command_rx: mpsc::Receiver<VideoCommand>,
    frame_tx: watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: watch::Sender<VideoStatus>,
) {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let mut last_logged_error: Option<String> = None;
    loop {
        // A new session must never inherit a frame from the previous device or
        // configuration.
        frame_tx.send_replace(None);
        let Some(device) = config.device.clone() else {
            status_tx.send_replace(VideoStatus::from_config(
                &config,
                VideoState::Unconfigured,
                Some("尚未选择视频采集设备".into()),
            ));
            match command_rx.recv() {
                Ok(VideoCommand::Reconfigure(next)) => config = next,
                Err(_) => return,
            }
            continue;
        };

        let was_missing = status_tx.borrow().reason == Some(VideoReason::DeviceMissing);
        match wait_for_device(
            &config,
            &device,
            &command_rx,
            &frame_tx,
            &status_tx,
            DEVICE_WAIT_SLICE,
        ) {
            DeviceWait::Present => {}
            DeviceWait::Reconfigure(next) => {
                config = next;
                continue;
            }
            DeviceWait::Stopped => return,
        }

        status_tx.send_replace(VideoStatus::from_config(
            &config,
            VideoState::Starting,
            Some(if was_missing {
                format!("采集卡已恢复，正在打开 {}", device.display())
            } else {
                format!("正在打开 {}", device.display())
            }),
        ));
        let result = catch_unwind(AssertUnwindSafe(|| {
            capture_session(&mut config, &command_rx, &frame_tx, &status_tx)
        }));

        match result {
            Ok(Ok(SessionExit::Reconfigure(next))) => {
                config = next;
                continue;
            }
            Ok(Err(error)) => {
                // The session retries every second; log each distinct failure once.
                if last_logged_error.as_deref() != Some(error.as_str()) {
                    tracing::warn!(device = %device.display(), %error, "video capture failed");
                    last_logged_error = Some(error.clone());
                }
                frame_tx.send_replace(None);
                status_tx.send_replace(VideoStatus::from_config(
                    &config,
                    VideoState::Offline,
                    Some(error),
                ));
            }
            Err(_) => {
                frame_tx.send_replace(None);
                status_tx.send_replace(VideoStatus::from_config(
                    &config,
                    VideoState::Offline,
                    Some("V4L2 驱动在停止视频流时异常，正在重试".into()),
                ));
            }
        }

        // An unplugged card fails the session with ENODEV; skip the blind
        // retry delay and go straight to the watcher, which wakes on replug.
        if super::hotplug::presence(&device) == super::hotplug::Presence::Missing {
            continue;
        }

        match command_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(VideoCommand::Reconfigure(next)) => config = next,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Live changes are written to `config`, so the supervisor restarts a failed
/// session with them rather than with the settings it was opened with.
#[cfg(target_os = "linux")]
fn capture_session(
    config: &mut VideoConfig,
    command_rx: &mpsc::Receiver<VideoCommand>,
    frame_tx: &watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: &watch::Sender<VideoStatus>,
) -> Result<SessionExit, String> {
    use v4l::{
        Device, Format, FourCC,
        buffer::{Flags, Type},
        io::{mmap::Stream as MmapStream, traits::CaptureStream},
        video::{Capture, capture::Parameters},
    };

    let path = config
        .device
        .as_ref()
        .ok_or_else(|| "video device is not configured".to_string())?;
    let device = Device::with_path(path).map_err(|error| error.to_string())?;
    let current = device.format().map_err(|error| error.to_string())?;
    let requested = Format::new(
        config.width.unwrap_or(current.width),
        config.height.unwrap_or(current.height),
        FourCC::new(b"MJPG"),
    );
    let actual = device
        .set_format(&requested)
        .map_err(|error| error.to_string())?;
    if actual.fourcc != FourCC::new(b"MJPG") {
        return Err(format!(
            "采集卡未接受 MJPEG 格式，实际返回 {}",
            actual.fourcc
        ));
    }

    let requested_fps = config.frames_per_second.unwrap_or(30).clamp(1, 120);
    let actual_params = device
        .set_params(&Parameters::with_fps(requested_fps))
        .map_err(|error| error.to_string())?;
    let actual_fps = if actual_params.interval.numerator == 0 {
        None
    } else {
        Some(
            f64::from(actual_params.interval.denominator)
                / f64::from(actual_params.interval.numerator),
        )
    };
    let mut stream =
        MmapStream::with_buffers(&device, Type::VideoCapture, 4).map_err(|e| e.to_string())?;
    stream.set_timeout(Duration::from_secs(1));

    let geometry = VideoGeometry {
        width: Some(actual.width),
        height: Some(actual.height),
        fps: actual_fps,
    };
    let mut health = CaptureHealth::new(Instant::now());
    publish_health(
        frame_tx,
        status_tx,
        config,
        &health,
        Instant::now(),
        geometry,
        Some("正在等待首帧…"),
    );

    let mut sequence = 0_u64;
    loop {
        match command_rx.try_recv() {
            Ok(VideoCommand::Reconfigure(next)) => {
                if let Some(next) = apply_to_session(config, next) {
                    return Ok(SessionExit::Reconfigure(next));
                }
                // Encoding or quality only: the next frame uses it.
                publish_health(
                    frame_tx,
                    status_tx,
                    config,
                    &health,
                    Instant::now(),
                    geometry,
                    None,
                );
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("video capture controller stopped".to_owned());
            }
        }
        match stream.next() {
            Ok((buffer, metadata)) => {
                let received_at = Instant::now();
                let valid = !metadata.flags.contains(Flags::ERROR)
                    && usize::try_from(metadata.bytesused)
                        .ok()
                        .filter(|used| *used > 0 && *used <= buffer.len())
                        .map(|used| &buffer[..used])
                        .is_some_and(looks_like_jpeg);
                if !valid {
                    if health.tick(received_at) {
                        publish_health(
                            frame_tx,
                            status_tx,
                            config,
                            &health,
                            received_at,
                            geometry,
                            None,
                        );
                    }
                    continue;
                }

                let used = usize::try_from(metadata.bytesused).unwrap_or(0);
                let source = &buffer[..used];
                let jpeg = match encode_frame(source, config) {
                    Ok(jpeg) => jpeg,
                    Err(error) => {
                        // A single corrupt or truncated frame must not tear down
                        // the whole capture session and force a device reopen.
                        tracing::warn!(%error, sequence, "failed to transcode JPEG frame; skipping");
                        if health.tick(received_at) {
                            publish_health(
                                frame_tx,
                                status_tx,
                                config,
                                &health,
                                received_at,
                                geometry,
                                None,
                            );
                        }
                        continue;
                    }
                };
                // Sample after the frame has been received, transcoded and
                // published, so freshness reflects publication rather than the
                // moment just before a potentially blocking read.
                sequence = sequence.wrapping_add(1);
                frame_tx.send_replace(Some(Arc::new(VideoFrame { jpeg, sequence })));
                let now = Instant::now();
                health.on_valid_frame(now);
                publish_health(frame_tx, status_tx, config, &health, now, geometry, None);
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                let now = Instant::now();
                if health.tick(now) {
                    publish_health(frame_tx, status_tx, config, &health, now, geometry, None);
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn video_supervisor(
    mut config: VideoConfig,
    command_rx: mpsc::Receiver<VideoCommand>,
    frame_tx: watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: watch::Sender<VideoStatus>,
) {
    frame_tx.send_replace(None);
    loop {
        status_tx.send_replace(VideoStatus::from_config(
            &config,
            VideoState::Unsupported,
            Some("V4L2 视频采集仅在 Linux 上可用".into()),
        ));
        match command_rx.recv() {
            Ok(VideoCommand::Reconfigure(next)) => config = next,
            Err(_) => return,
        }
    }
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn looks_like_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9])
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn encode_frame(source: &[u8], config: &VideoConfig) -> Result<Bytes, String> {
    match config.encoding {
        VideoEncoding::MjpegPassthrough => Ok(Bytes::copy_from_slice(source)),
        VideoEncoding::TranscodeJpeg => transcode_jpeg(source, config.jpeg_quality),
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn transcode_jpeg(source: &[u8], quality: u8) -> Result<Bytes, String> {
    let image = image::load_from_memory_with_format(source, image::ImageFormat::Jpeg)
        .map_err(|error| error.to_string())?;
    let mut output = Vec::with_capacity(source.len());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality.clamp(1, 100))
        .encode_image(&image)
        .map_err(|error| error.to_string())?;
    Ok(Bytes::from(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> VideoConfig {
        VideoConfig {
            device: Some(PathBuf::from("/dev/video-test")),
            ..VideoConfig::default()
        }
    }

    fn test_geometry() -> VideoGeometry {
        VideoGeometry {
            width: Some(1280),
            height: Some(720),
            fps: Some(30.0),
        }
    }

    fn test_frame(sequence: u64) -> Arc<VideoFrame> {
        // A synthetic black JPEG still has SOI/EOI markers; content does not
        // affect validity.
        Arc::new(VideoFrame {
            jpeg: Bytes::from_static(&[0xff, 0xd8, 0xff, 0xd9]),
            sequence,
        })
    }

    fn channels(
        config: &VideoConfig,
    ) -> (
        watch::Sender<Option<Arc<VideoFrame>>>,
        watch::Receiver<Option<Arc<VideoFrame>>>,
        watch::Sender<VideoStatus>,
        watch::Receiver<VideoStatus>,
    ) {
        let (frame_tx, frame_rx) = watch::channel(None);
        let (status_tx, status_rx) =
            watch::channel(VideoStatus::from_config(config, VideoState::Starting, None));
        (frame_tx, frame_rx, status_tx, status_rx)
    }

    #[test]
    fn starting_until_first_frame_then_stalls_after_five_seconds() {
        let config = test_config();
        let (frame_tx, frame_rx, status_tx, status_rx) = channels(&config);
        let start = Instant::now();
        let mut health = CaptureHealth::new(start);

        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &health,
            start,
            test_geometry(),
            Some("正在等待首帧…"),
        );
        assert_eq!(status_rx.borrow().state, VideoState::Starting);
        assert_eq!(status_rx.borrow().frames_per_second, None);

        assert!(!health.tick(start + Duration::from_secs(4)));
        assert_eq!(status_rx.borrow().state, VideoState::Starting);

        assert!(health.tick(start + Duration::from_secs(5)));
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &health,
            start + Duration::from_secs(5),
            test_geometry(),
            None,
        );
        let status = status_rx.borrow().clone();
        assert_eq!(status.state, VideoState::Stalled);
        assert_eq!(status.frames_per_second, None);
        assert!(status.message.as_deref().unwrap_or("").contains('5'));
        assert!(frame_rx.borrow().is_none());
    }

    #[test]
    fn valid_frame_then_stale_clears_cache_and_recovery_restores_ready() {
        let config = test_config();
        let (frame_tx, frame_rx, status_tx, status_rx) = channels(&config);
        let start = Instant::now();
        let mut health = CaptureHealth::new(start);

        frame_tx.send_replace(Some(test_frame(1)));
        health.on_valid_frame(start + Duration::from_secs(1));
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &health,
            start + Duration::from_secs(1),
            test_geometry(),
            None,
        );
        let ready = status_rx.borrow().clone();
        assert_eq!(ready.state, VideoState::Ready);
        assert_eq!(ready.frames_per_second, Some(30.0));
        assert!(ready.last_frame_unix_ms.is_some());
        assert_eq!(ready.last_frame_age_ms, Some(0));
        assert!(frame_rx.borrow().is_some());

        assert!(!health.tick(start + Duration::from_secs(5)));
        assert!(health.tick(start + Duration::from_secs(6)));
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &health,
            start + Duration::from_secs(6),
            test_geometry(),
            None,
        );
        let stalled = status_rx.borrow().clone();
        assert_eq!(stalled.state, VideoState::Stalled);
        assert_eq!(stalled.frames_per_second, None);
        assert_eq!(stalled.last_frame_age_ms, Some(5_000));
        assert!(frame_rx.borrow().is_none());

        frame_tx.send_replace(Some(test_frame(2)));
        health.on_valid_frame(start + Duration::from_secs(7));
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &health,
            start + Duration::from_secs(7),
            test_geometry(),
            None,
        );
        assert_eq!(status_rx.borrow().state, VideoState::Ready);
        assert_eq!(status_rx.borrow().frames_per_second, Some(30.0));
        assert!(frame_rx.borrow().is_some());
    }

    #[test]
    fn invalid_frame_progress_does_not_reset_stall_timer() {
        let start = Instant::now();
        let mut health = CaptureHealth::new(start);
        health.on_valid_frame(start);
        let last_frame = health.last_frame_at;

        for step in 1..=49 {
            assert!(!health.tick(start + Duration::from_millis(step * 100)));
        }
        assert_eq!(health.last_frame_at, last_frame);
        assert!(health.tick(start + Duration::from_secs(5)));
        assert_eq!(health.phase, CapturePhase::Stalled);
    }

    #[test]
    fn one_fps_source_stays_ready_inside_five_second_window() {
        let start = Instant::now();
        let mut health = CaptureHealth::new(start);
        health.on_valid_frame(start);

        for second in 1..=5 {
            assert!(!health.tick(start + Duration::from_millis(second * 1000 - 1)));
            health.on_valid_frame(start + Duration::from_secs(second));
            assert_eq!(health.phase, CapturePhase::Ready);
        }
    }

    #[test]
    fn reconfigure_reset_starts_fresh_and_clears_cached_frame() {
        let config = test_config();
        let (frame_tx, frame_rx, status_tx, status_rx) = channels(&config);
        let start = Instant::now();
        let mut old = CaptureHealth::new(start);
        old.on_valid_frame(start + Duration::from_secs(1));
        frame_tx.send_replace(Some(test_frame(1)));
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &old,
            start + Duration::from_secs(1),
            test_geometry(),
            None,
        );
        assert_eq!(status_rx.borrow().state, VideoState::Ready);
        assert!(frame_rx.borrow().is_some());

        let reset_at = start + Duration::from_secs(30);
        let fresh = CaptureHealth::new(reset_at);
        publish_health(
            &frame_tx,
            &status_tx,
            &config,
            &fresh,
            reset_at,
            test_geometry(),
            Some("正在等待首帧…"),
        );
        let status = status_rx.borrow().clone();
        assert_eq!(status.state, VideoState::Starting);
        assert_eq!(status.frames_per_second, None);
        assert_eq!(status.last_frame_unix_ms, None);
        assert_eq!(status.last_frame_age_ms, None);
        assert!(frame_rx.borrow().is_none());
    }

    #[test]
    fn status_read_recomputes_frame_age() {
        let (manager, _frame_tx) = VideoManager::test_source(test_config());
        let now = unix_ms_now().expect("system clock after unix epoch");
        let mut status = VideoStatus::from_config(&test_config(), VideoState::Ready, None);
        status.last_frame_unix_ms = Some(now.saturating_sub(2_500));
        manager.status_tx.send_replace(status);

        let age = manager.status().last_frame_age_ms.expect("frame age");
        assert!((2_400..=5_000).contains(&age), "unexpected age {age}");
    }

    #[test]
    fn only_device_and_format_changes_restart_the_capture() {
        let base = VideoConfig {
            width: Some(1920),
            height: Some(1080),
            frames_per_second: Some(30),
            ..test_config()
        };
        let changed = |edit: fn(&mut VideoConfig)| {
            let mut next = base.clone();
            edit(&mut next);
            capture_change(&base, &next)
        };
        assert_eq!(capture_change(&base, &base), CaptureChange::Unchanged);
        assert_eq!(changed(|c| c.device = Some("/dev/video9".into())), CaptureChange::Restart);
        assert_eq!(changed(|c| c.device = None), CaptureChange::Restart);
        assert_eq!(changed(|c| c.width = Some(1280)), CaptureChange::Restart);
        assert_eq!(changed(|c| c.height = None), CaptureChange::Restart);
        assert_eq!(changed(|c| c.frames_per_second = Some(60)), CaptureChange::Restart);
        assert_eq!(changed(|c| c.jpeg_quality = 55), CaptureChange::Live);
        assert_eq!(
            changed(|c| c.encoding = VideoEncoding::TranscodeJpeg),
            CaptureChange::Live
        );
        assert_eq!(changed(|c| c.auto_detect = !c.auto_detect), CaptureChange::Unchanged);
        assert_eq!(changed(|c| c.h264.bitrate_kbps = 1_000), CaptureChange::Unchanged);
        assert_eq!(changed(|c| c.h264.max_sessions = 4), CaptureChange::Unchanged);
    }

    #[test]
    fn live_changes_apply_in_place_and_reach_the_next_frame() {
        let source = {
            let image = image::RgbImage::from_pixel(16, 16, image::Rgb([200, 40, 40]));
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
                .encode_image(&image)
                .unwrap();
            jpeg
        };
        let mut current = test_config();
        assert_eq!(encode_frame(&source, &current).unwrap(), source, "passthrough");

        let mut next = current.clone();
        next.encoding = VideoEncoding::TranscodeJpeg;
        next.jpeg_quality = 10;
        assert!(apply_to_session(&mut current, next.clone()).is_none(), "no restart");
        assert_eq!(current, next);
        let low = encode_frame(&source, &current).unwrap();
        assert_ne!(low, source);

        next.jpeg_quality = 100;
        assert!(apply_to_session(&mut current, next.clone()).is_none());
        assert!(encode_frame(&source, &current).unwrap().len() > low.len());

        let mut resized = current.clone();
        resized.width = Some(640);
        assert_eq!(apply_to_session(&mut current, resized.clone()), Some(resized));
        assert_eq!(current, next, "a restart leaves the session's copy alone");
    }

    #[test]
    fn rejects_truncated_jpeg_frames() {
        assert!(looks_like_jpeg(&[0xff, 0xd8, 0xff, 0xd9]));
        assert!(!looks_like_jpeg(&[0xff, 0xd8, 0x00]));
        assert!(!looks_like_jpeg(&[0x00, 0xd8, 0xff, 0xd9]));
    }

    #[test]
    fn transcode_failure_is_a_per_frame_error_not_a_session_error() {
        // Valid SOI/EOI markers with corrupt payload pass the cheap frame check
        // but fail JPEG decoding.
        let source = [0xff, 0xd8, 0x00, 0x01, 0xff, 0xd9];
        assert!(looks_like_jpeg(&source));

        let mut config = test_config();
        config.encoding = VideoEncoding::TranscodeJpeg;
        assert!(encode_frame(&source, &config).is_err());

        config.encoding = VideoEncoding::MjpegPassthrough;
        assert_eq!(encode_frame(&source, &config).unwrap().as_ref(), source);
    }

    #[test]
    fn missing_device_reports_reason_and_wakes_on_replug() {
        let root = crate::devices::hotplug::tests::temp_root("video-replug");
        let node = root.join("video0");
        let config = VideoConfig {
            device: Some(node.clone()),
            ..VideoConfig::default()
        };
        let (frame_tx, frame_rx, status_tx, status_rx) = channels(&config);
        frame_tx.send_replace(Some(test_frame(1)));
        let (_command_tx, command_rx) = mpsc::channel();

        let worker = {
            let (config, node) = (config.clone(), node.clone());
            std::thread::spawn(move || {
                let started = Instant::now();
                let outcome = wait_for_device(
                    &config,
                    &node,
                    &command_rx,
                    &frame_tx,
                    &status_tx,
                    Duration::from_secs(10),
                );
                (outcome, started.elapsed())
            })
        };
        std::thread::sleep(Duration::from_millis(150));
        let missing = status_rx.borrow().clone();
        assert_eq!(missing.state, VideoState::Offline);
        assert_eq!(missing.reason, Some(VideoReason::DeviceMissing));
        assert!(missing.message.as_deref().unwrap().contains("采集卡已断开"));
        assert!(frame_rx.borrow().is_none(), "stale frame must be cleared");
        let json = serde_json::to_value(&missing).unwrap();
        assert_eq!(json["reason"], "device_missing");

        std::fs::write(&node, b"").unwrap();
        let (outcome, elapsed) = worker.join().unwrap();
        assert!(matches!(outcome, DeviceWait::Present));
        // Woken by the watcher, not by the 10 s slice timeout.
        assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn present_device_returns_immediately_without_publishing() {
        let root = crate::devices::hotplug::tests::temp_root("video-present");
        let node = root.join("video0");
        std::fs::write(&node, b"").unwrap();
        let config = test_config();
        let (frame_tx, _frame_rx, status_tx, status_rx) = channels(&config);
        let (_command_tx, command_rx) = mpsc::channel();
        let outcome = wait_for_device(
            &config,
            &node,
            &command_rx,
            &frame_tx,
            &status_tx,
            Duration::from_secs(10),
        );
        assert!(matches!(outcome, DeviceWait::Present));
        assert_eq!(status_rx.borrow().reason, None);
        assert!(serde_json::to_value(&*status_rx.borrow()).unwrap().get("reason").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconfigure_interrupts_missing_device_wait() {
        let root = crate::devices::hotplug::tests::temp_root("video-reconf");
        let config = test_config();
        let (frame_tx, _frame_rx, status_tx, _status_rx) = channels(&config);
        let (command_tx, command_rx) = mpsc::channel();
        let mut next = test_config();
        next.device = Some(root.join("video1"));
        command_tx.send(VideoCommand::Reconfigure(next)).unwrap();
        let outcome = wait_for_device(
            &config,
            &root.join("video0"),
            &command_rx,
            &frame_tx,
            &status_tx,
            Duration::from_millis(50),
        );
        match outcome {
            DeviceWait::Reconfigure(cfg) => assert_eq!(cfg.device, Some(root.join("video1"))),
            other => panic!("unexpected {other:?}"),
        }
        drop(command_tx);
        assert!(matches!(
            wait_for_device(
                &config,
                &root.join("video0"),
                &command_rx,
                &frame_tx,
                &status_tx,
                Duration::from_millis(50),
            ),
            DeviceWait::Stopped
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
