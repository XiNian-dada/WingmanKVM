use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

#[cfg(target_os = "linux")]
use std::time::Duration;

use bytes::Bytes;
use serde::Serialize;
use tokio::sync::watch;

use crate::config::{VideoConfig, VideoEncoding};

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub jpeg: Bytes,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[serde(rename_all = "snake_case")]
pub enum VideoState {
    Unconfigured,
    Starting,
    Ready,
    Offline,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoStatus {
    pub state: VideoState,
    pub message: Option<String>,
    pub device: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frames_per_second: Option<f64>,
    pub encoding: VideoEncoding,
}

impl VideoStatus {
    fn from_config(config: &VideoConfig, state: VideoState, message: Option<String>) -> Self {
        Self {
            state,
            message,
            device: config.device.clone(),
            width: config.width,
            height: config.height,
            frames_per_second: config.frames_per_second.map(f64::from),
            encoding: config.encoding,
        }
    }
}

#[derive(Clone)]
pub struct VideoManager {
    config_tx: mpsc::Sender<VideoConfig>,
    frame_tx: watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: watch::Sender<VideoStatus>,
}

impl VideoManager {
    pub fn new(initial: VideoConfig) -> Self {
        let (config_tx, config_rx) = mpsc::channel();
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
            .spawn(move || video_supervisor(initial, config_rx, thread_frames, thread_status))
            .expect("failed to start video capture thread");

        Self {
            config_tx,
            frame_tx,
            status_tx,
        }
    }

    pub fn reconfigure(&self, config: VideoConfig) -> Result<(), String> {
        self.config_tx
            .send(config)
            .map_err(|_| "video capture thread stopped".to_string())
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<VideoFrame>>> {
        self.frame_tx.subscribe()
    }

    pub fn status(&self) -> VideoStatus {
        self.status_tx.borrow().clone()
    }
}

#[cfg(target_os = "linux")]
fn video_supervisor(
    mut config: VideoConfig,
    config_rx: mpsc::Receiver<VideoConfig>,
    frame_tx: watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: watch::Sender<VideoStatus>,
) {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    loop {
        config = drain_latest(&config_rx).unwrap_or(config);
        let Some(device) = config.device.clone() else {
            frame_tx.send_replace(None);
            status_tx.send_replace(VideoStatus::from_config(
                &config,
                VideoState::Unconfigured,
                Some("尚未选择视频采集设备".into()),
            ));
            match config_rx.recv() {
                Ok(next) => {
                    config = next;
                    continue;
                }
                Err(_) => return,
            }
        };

        status_tx.send_replace(VideoStatus::from_config(
            &config,
            VideoState::Starting,
            Some(format!("正在打开 {}", device.display())),
        ));
        let result = catch_unwind(AssertUnwindSafe(|| {
            capture_session(&config, &config_rx, &frame_tx, &status_tx)
        }));

        match result {
            Ok(Ok(Some(next))) => {
                config = next;
                continue;
            }
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
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

        match config_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(next) => config = next,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(target_os = "linux")]
fn capture_session(
    config: &VideoConfig,
    config_rx: &mpsc::Receiver<VideoConfig>,
    frame_tx: &watch::Sender<Option<Arc<VideoFrame>>>,
    status_tx: &watch::Sender<VideoStatus>,
) -> Result<Option<VideoConfig>, String> {
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

    let mut ready = VideoStatus::from_config(config, VideoState::Ready, None);
    ready.width = Some(actual.width);
    ready.height = Some(actual.height);
    ready.frames_per_second = actual_fps;
    status_tx.send_replace(ready);

    let mut sequence = 0_u64;
    loop {
        if let Some(next) = drain_latest(config_rx) {
            return Ok(Some(next));
        }
        match stream.next() {
            Ok((buffer, metadata)) => {
                if metadata.flags.contains(Flags::ERROR) {
                    continue;
                }
                let used = usize::try_from(metadata.bytesused)
                    .map_err(|_| "invalid V4L2 bytesused value".to_string())?;
                if used == 0 || used > buffer.len() {
                    continue;
                }
                let source = &buffer[..used];
                if !looks_like_jpeg(source) {
                    continue;
                }
                let jpeg = match config.encoding {
                    VideoEncoding::MjpegPassthrough => Bytes::copy_from_slice(source),
                    VideoEncoding::TranscodeJpeg => transcode_jpeg(source, config.jpeg_quality)?,
                };
                sequence = sequence.wrapping_add(1);
                frame_tx.send_replace(Some(Arc::new(VideoFrame { jpeg, sequence })));
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn video_supervisor(
    mut config: VideoConfig,
    config_rx: mpsc::Receiver<VideoConfig>,
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
        match config_rx.recv() {
            Ok(next) => config = next,
            Err(_) => return,
        }
    }
}

#[cfg(target_os = "linux")]
fn drain_latest(rx: &mpsc::Receiver<VideoConfig>) -> Option<VideoConfig> {
    let mut latest = None;
    while let Ok(config) = rx.try_recv() {
        latest = Some(config);
    }
    latest
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn looks_like_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9])
}

#[cfg(target_os = "linux")]
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

    #[test]
    fn rejects_truncated_jpeg_frames() {
        assert!(looks_like_jpeg(&[0xff, 0xd8, 0xff, 0xd9]));
        assert!(!looks_like_jpeg(&[0xff, 0xd8, 0x00]));
        assert!(!looks_like_jpeg(&[0x00, 0xd8, 0xff, 0xd9]));
    }
}
