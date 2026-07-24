use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::Mutex as AsyncMutex,
    task::JoinHandle,
};
use webrtc::{
    api::{
        APIBuilder,
        interceptor_registry::register_default_interceptors,
        media_engine::{MIME_TYPE_H264, MediaEngine},
    },
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

use crate::{
    config::{H264Config, H264Encoder},
    devices::video::VideoManager,
};

const MAX_OFFER_BYTES: usize = 64 * 1024;
const ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ACCESS_UNIT_BUFFER: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct WebRtcOfferRequest {
    #[serde(rename = "type")]
    pub sdp_type: String,
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebRtcOfferResponse {
    #[serde(rename = "type")]
    pub sdp_type: &'static str,
    pub sdp: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebRtcStatus {
    pub available: bool,
    pub encoder: Option<String>,
    pub message: Option<String>,
    pub bitrate_kbps: u32,
    pub sessions: usize,
    pub max_sessions: u8,
    pub software_encoding: bool,
}

#[derive(Debug, Error)]
pub enum WebRtcError {
    #[error("{0}")]
    InvalidOffer(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("H.264 会话已达到上限")]
    Capacity,
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone)]
struct EncoderSpec {
    executable: PathBuf,
    codec: &'static str,
    label: &'static str,
    software: bool,
}

struct Session {
    peer: Arc<RTCPeerConnection>,
    encoder_task: JoinHandle<()>,
}

struct Inner {
    video: VideoManager,
    config: Mutex<H264Config>,
    encoder: Mutex<Result<EncoderSpec, String>>,
    sessions: Mutex<HashMap<String, Session>>,
    offer_lock: AsyncMutex<()>,
}

#[derive(Clone)]
pub struct WebRtcManager {
    inner: Arc<Inner>,
}

impl WebRtcManager {
    pub fn new(video: VideoManager, config: H264Config) -> Self {
        let encoder = probe_encoder(&config);
        if let Err(message) = &encoder {
            tracing::info!(%message, "H.264 WebRTC transport is unavailable");
        }
        Self {
            inner: Arc::new(Inner {
                video,
                config: Mutex::new(config),
                encoder: Mutex::new(encoder),
                sessions: Mutex::new(HashMap::new()),
                offer_lock: AsyncMutex::new(()),
            }),
        }
    }

    pub fn status(&self) -> WebRtcStatus {
        let config = self
            .inner
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let encoder = self
            .inner
            .encoder
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let sessions = self
            .inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len();
        match encoder {
            Ok(encoder) => WebRtcStatus {
                available: true,
                encoder: Some(encoder.label.to_owned()),
                message: None,
                bitrate_kbps: config.bitrate_kbps,
                sessions,
                max_sessions: config.max_sessions,
                software_encoding: encoder.software,
            },
            Err(message) => WebRtcStatus {
                available: false,
                encoder: None,
                message: Some(message),
                bitrate_kbps: config.bitrate_kbps,
                sessions,
                max_sessions: config.max_sessions,
                software_encoding: false,
            },
        }
    }

    pub async fn reconfigure(&self, config: H264Config) {
        let probe_config = config.clone();
        let encoder = tokio::task::spawn_blocking(move || probe_encoder(&probe_config))
            .await
            .unwrap_or_else(|error| Err(format!("H.264 编码器探测失败：{error}")));

        *self
            .inner
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = config;
        *self
            .inner
            .encoder
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = encoder;

        let session_ids = self
            .inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for session_id in session_ids {
            self.close(&session_id).await;
        }
    }

    pub async fn offer(
        &self,
        request: WebRtcOfferRequest,
    ) -> Result<WebRtcOfferResponse, WebRtcError> {
        if !request.sdp_type.eq_ignore_ascii_case("offer") {
            return Err(WebRtcError::InvalidOffer(
                "WebRTC 请求类型必须是 offer".to_owned(),
            ));
        }
        if request.sdp.is_empty() || request.sdp.len() > MAX_OFFER_BYTES {
            return Err(WebRtcError::InvalidOffer(
                "WebRTC offer 大小不正确".to_owned(),
            ));
        }

        let _offer_guard = self.inner.offer_lock.lock().await;
        let config = self
            .inner
            .config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let encoder = self
            .inner
            .encoder
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .map_err(WebRtcError::Unavailable)?;
        let session_count = self
            .inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len();
        if session_count >= usize::from(config.max_sessions) {
            return Err(WebRtcError::Capacity);
        }

        let frames = self.inner.video.subscribe();
        if frames.borrow().is_none() {
            return Err(WebRtcError::Unavailable(
                "当前还没有可用的视频帧".to_owned(),
            ));
        }

        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .map_err(internal_error)?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)
            .map_err(internal_error)?;
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();
        let peer = Arc::new(
            api.new_peer_connection(RTCConfiguration::default())
                .await
                .map_err(internal_error)?,
        );
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90_000,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                        .to_owned(),
                ..Default::default()
            },
            "video".to_owned(),
            "wingmankvm".to_owned(),
        ));
        let sender = peer
            .add_track(track.clone())
            .await
            .map_err(internal_error)?;
        tokio::spawn(async move { while sender.read_rtcp().await.is_ok() {} });

        let remote = RTCSessionDescription::offer(request.sdp)
            .map_err(|error| WebRtcError::InvalidOffer(error.to_string()))?;
        if let Err(error) = peer.set_remote_description(remote).await {
            let _ = peer.close().await;
            return Err(WebRtcError::InvalidOffer(error.to_string()));
        }
        let local = match async {
            let mut gathering_complete = peer.gathering_complete_promise().await;
            let answer = peer.create_answer(None).await.map_err(internal_error)?;
            peer.set_local_description(answer)
                .await
                .map_err(internal_error)?;
            tokio::time::timeout(ICE_GATHER_TIMEOUT, gathering_complete.recv())
                .await
                .map_err(|_| WebRtcError::Internal("WebRTC ICE 候选收集超时".to_owned()))?;
            peer.local_description()
                .await
                .ok_or_else(|| WebRtcError::Internal("WebRTC answer 未生成".to_owned()))
        }
        .await
        {
            Ok(local) => local,
            Err(error) => {
                let _ = peer.close().await;
                return Err(error);
            }
        };

        let session_id = generate_session_id()?;
        install_connection_cleanup(&self.inner, &peer, &session_id);
        let video = self.inner.video.clone();
        let encoder_id = session_id.clone();
        let encoder_task = tokio::spawn(async move {
            if let Err(error) = run_encoder(video, track, encoder, config).await {
                tracing::warn!(session_id = %encoder_id, %error, "H.264 encoder session stopped");
            }
        });
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                session_id.clone(),
                Session {
                    peer: peer.clone(),
                    encoder_task,
                },
            );
        install_session_ttl(&self.inner, session_id.clone());

        Ok(WebRtcOfferResponse {
            sdp_type: "answer",
            sdp: local.sdp,
            session_id,
        })
    }

    pub async fn close(&self, session_id: &str) -> bool {
        let session = self
            .inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
        let Some(session) = session else {
            return false;
        };
        session.encoder_task.abort();
        let _ = session.peer.close().await;
        true
    }
}

fn install_connection_cleanup(inner: &Arc<Inner>, peer: &Arc<RTCPeerConnection>, session_id: &str) {
    let weak = Arc::downgrade(inner);
    let session_id = session_id.to_owned();
    peer.on_peer_connection_state_change(Box::new(move |state| {
        let weak = weak.clone();
        let session_id = session_id.clone();
        Box::pin(async move {
            if matches!(
                state,
                RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
            ) && let Some(inner) = weak.upgrade()
            {
                close_inner(inner, &session_id).await;
            }
        })
    }));
}

fn install_session_ttl(inner: &Arc<Inner>, session_id: String) {
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        tokio::time::sleep(SESSION_TTL).await;
        if let Some(inner) = weak.upgrade() {
            close_inner(inner, &session_id).await;
        }
    });
}

async fn close_inner(inner: Arc<Inner>, session_id: &str) {
    let session = inner
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(session_id);
    if let Some(session) = session {
        session.encoder_task.abort();
        let _ = session.peer.close().await;
    }
}

fn generate_session_id() -> Result<String, WebRtcError> {
    let mut bytes = [0_u8; 18];
    getrandom::fill(&mut bytes)
        .map_err(|error| WebRtcError::Internal(format!("无法生成会话标识：{error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn probe_encoder(config: &H264Config) -> Result<EncoderSpec, String> {
    let executable = config
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let output = std::process::Command::new(&executable)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|error| format!("未找到 FFmpeg：{error}"))?;
    if !output.status.success() {
        return Err("FFmpeg 无法列出编码器".to_owned());
    }
    let listing = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let (codec, label, software) = select_encoder(config, &listing)?;
    Ok(EncoderSpec {
        executable,
        codec,
        label,
        software,
    })
}

fn select_encoder(
    config: &H264Config,
    listing: &str,
) -> Result<(&'static str, &'static str, bool), String> {
    let supports = |name: &str| listing.split_whitespace().any(|word| word == name);
    let requested = match config.encoder {
        H264Encoder::Auto if supports("h264_rkmpp") => Some(("h264_rkmpp", "Rockchip MPP", false)),
        H264Encoder::Auto if supports("h264_v4l2m2m") => Some(("h264_v4l2m2m", "V4L2 M2M", false)),
        H264Encoder::Auto if config.allow_software && supports("libx264") => {
            Some(("libx264", "libx264", true))
        }
        H264Encoder::RockchipMpp if supports("h264_rkmpp") => {
            Some(("h264_rkmpp", "Rockchip MPP", false))
        }
        H264Encoder::V4l2M2m if supports("h264_v4l2m2m") => {
            Some(("h264_v4l2m2m", "V4L2 M2M", false))
        }
        H264Encoder::Software if config.allow_software && supports("libx264") => {
            Some(("libx264", "libx264", true))
        }
        _ => None,
    };

    requested.ok_or_else(|| match config.encoder {
        H264Encoder::Software if !config.allow_software => "软件 H.264 编码未获允许".to_owned(),
        H264Encoder::Auto if !config.allow_software => {
            "未检测到 H.264 硬件编码器；MJPEG 仍可正常使用".to_owned()
        }
        _ => "未检测到所选 H.264 编码器".to_owned(),
    })
}

async fn run_encoder(
    video: VideoManager,
    track: Arc<TrackLocalStaticSample>,
    encoder: EncoderSpec,
    config: H264Config,
) -> Result<(), WebRtcError> {
    let fps = video
        .status()
        .frames_per_second
        .unwrap_or(30.0)
        .round()
        .clamp(1.0, 120.0) as u32;
    let bitrate = format!("{}k", config.bitrate_kbps);
    let buffer_size = format!("{}k", config.bitrate_kbps.saturating_mul(2));
    let fps_text = fps.to_string();
    let mut command = Command::new(&encoder.executable);
    command.kill_on_drop(true).args([
        "-hide_banner",
        "-loglevel",
        "warning",
        "-fflags",
        "nobuffer",
        "-f",
        "mjpeg",
        "-framerate",
        &fps_text,
        "-i",
        "pipe:0",
        "-an",
        "-vf",
        "format=yuv420p",
        "-c:v",
        encoder.codec,
        "-b:v",
        &bitrate,
        "-maxrate",
        &bitrate,
        "-bufsize",
        &buffer_size,
        "-g",
        &fps_text,
        "-bf",
        "0",
    ]);
    if encoder.software {
        command.args([
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-profile:v",
            "baseline",
            "-x264-params",
            "repeat-headers=1:aud=1:scenecut=0",
        ]);
    }
    command
        .args(["-bsf:v", "h264_metadata=aud=insert", "-f", "h264", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| WebRtcError::Unavailable(format!("无法启动 H.264 编码器：{error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| WebRtcError::Internal("无法连接 FFmpeg 输入".to_owned()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| WebRtcError::Internal("无法连接 FFmpeg 输出".to_owned()))?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(message = %line, "FFmpeg H.264 encoder");
            }
        });
    }

    let mut frames = video.subscribe();
    let mut writer = tokio::spawn(async move {
        loop {
            let frame = frames.borrow_and_update().clone();
            if let Some(frame) = frame {
                stdin
                    .write_all(&frame.jpeg)
                    .await
                    .map_err(|error| error.to_string())?;
                stdin.flush().await.map_err(|error| error.to_string())?;
            }
            frames
                .changed()
                .await
                .map_err(|_| "视频采集线程已停止".to_owned())?;
        }
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    });
    let frame_duration = Duration::from_secs_f64(1.0 / f64::from(fps));
    let mut reader = tokio::spawn(async move {
        let mut parser = AnnexBAccessUnitParser::default();
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let read = stdout
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                if let Some(access_unit) = parser.finish() {
                    write_access_unit(&track, access_unit, frame_duration).await?;
                }
                return Ok::<(), String>(());
            }
            let access_units = parser.push(&chunk[..read])?;
            for access_unit in access_units {
                write_access_unit(&track, access_unit, frame_duration).await?;
            }
        }
    });

    let result = tokio::select! {
        result = &mut writer => match result {
            Ok(Ok(())) => Err(WebRtcError::Internal("FFmpeg 输入意外结束".to_owned())),
            Ok(Err(error)) => Err(WebRtcError::Internal(error)),
            Err(error) => Err(WebRtcError::Internal(error.to_string())),
        },
        result = &mut reader => match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(WebRtcError::Internal(error)),
            Err(error) => Err(WebRtcError::Internal(error.to_string())),
        },
        status = child.wait() => match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(WebRtcError::Unavailable(format!("H.264 编码器退出：{status}"))),
            Err(error) => Err(WebRtcError::Internal(error.to_string())),
        },
    };
    writer.abort();
    reader.abort();
    let _ = child.kill().await;
    result
}

async fn write_access_unit(
    track: &TrackLocalStaticSample,
    access_unit: Bytes,
    duration: Duration,
) -> Result<(), String> {
    track
        .write_sample(&Sample {
            data: access_unit,
            duration,
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())
}

fn internal_error(error: impl std::fmt::Display) -> WebRtcError {
    WebRtcError::Internal(error.to_string())
}

#[derive(Default)]
struct AnnexBAccessUnitParser {
    buffer: Vec<u8>,
}

impl AnnexBAccessUnitParser {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Bytes>, String> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > MAX_ACCESS_UNIT_BUFFER {
            return Err("H.264 输出缺少访问单元边界".to_owned());
        }

        let mut output = Vec::new();
        loop {
            let aud_positions = annex_b_nal_units(&self.buffer)
                .into_iter()
                .filter_map(|(start, payload)| {
                    self.buffer
                        .get(payload)
                        .is_some_and(|byte| byte & 0x1f == 9)
                        .then_some(start)
                })
                .collect::<Vec<_>>();
            if aud_positions.len() < 2 {
                break;
            }
            let boundary = aud_positions[1];
            output.push(Bytes::copy_from_slice(&self.buffer[..boundary]));
            self.buffer.drain(..boundary);
        }
        Ok(output)
    }

    fn finish(self) -> Option<Bytes> {
        (!self.buffer.is_empty()).then(|| Bytes::from(self.buffer))
    }
}

fn annex_b_nal_units(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut units = Vec::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let prefix = if bytes[index..].starts_with(&[0, 0, 0, 1]) {
            Some(4)
        } else if bytes[index..].starts_with(&[0, 0, 1]) {
            Some(3)
        } else {
            None
        };
        if let Some(prefix) = prefix {
            units.push((index, index + prefix));
            index += prefix;
        } else {
            index += 1;
        }
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_annex_b_stream_at_aud_boundaries() {
        let mut parser = AnnexBAccessUnitParser::default();
        let first = [0, 0, 0, 1, 9, 0x10, 0, 0, 1, 7, 1, 2, 0, 0, 1, 5, 3, 4];
        let second = [0, 0, 1, 9, 0x10, 0, 0, 1, 1, 5, 6];
        assert!(parser.push(&first).unwrap().is_empty());
        let output = parser.push(&second).unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_ref(), first);
        assert_eq!(parser.finish().unwrap().as_ref(), second);
    }

    #[test]
    fn keeps_headers_before_the_first_aud() {
        let mut parser = AnnexBAccessUnitParser::default();
        let bytes = [
            0, 0, 1, 7, 1, 0, 0, 1, 8, 2, 0, 0, 1, 9, 0x10, 0, 0, 1, 5, 3, 0, 0, 1, 9, 0x10,
        ];
        let output = parser.push(&bytes).unwrap();
        assert_eq!(output.len(), 1);
        assert!(output[0].starts_with(&[0, 0, 1, 7]));
        assert!(parser.finish().unwrap().starts_with(&[0, 0, 1, 9]));
    }

    #[test]
    fn recognizes_three_and_four_byte_start_codes() {
        assert_eq!(
            annex_b_nal_units(&[0, 0, 1, 9, 0, 0, 0, 1, 5]),
            vec![(0, 3), (4, 8)]
        );
    }

    #[test]
    fn automatic_encoder_never_uses_software_without_opt_in() {
        let config = H264Config::default();
        let error = select_encoder(&config, "V....D libx264").unwrap_err();
        assert!(error.contains("硬件编码器"));

        let config = H264Config {
            allow_software: true,
            ..H264Config::default()
        };
        assert_eq!(
            select_encoder(&config, "V....D libx264").unwrap(),
            ("libx264", "libx264", true)
        );
    }

    #[test]
    fn automatic_encoder_prefers_rockchip_then_v4l2() {
        let config = H264Config {
            allow_software: true,
            ..H264Config::default()
        };
        let listing = "V....D libx264\nV....D h264_v4l2m2m\nV....D h264_rkmpp";
        assert_eq!(
            select_encoder(&config, listing).unwrap(),
            ("h264_rkmpp", "Rockchip MPP", false)
        );
    }
}
