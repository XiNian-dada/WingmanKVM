use std::{
    collections::HashMap,
    future::Future,
    io::{self, Read},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    process::{Child as StdChild, Command as StdCommand, ExitStatus, Stdio as StdStdio},
    sync::{Arc, Mutex, Weak, atomic::{AtomicU64, Ordering}},
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UdpSocket, lookup_host},
    process::{Child as TokioChild, Command as TokioCommand},
    sync::{Mutex as AsyncMutex, mpsc, watch},
    task::AbortHandle,
};
use webrtc::{
    api::{
        APIBuilder,
        interceptor_registry::register_default_interceptors,
        media_engine::{MIME_TYPE_H264, MediaEngine},
        setting_engine::SettingEngine,
    },
    ice::{
        network_type::NetworkType,
        udp_mux::{UDPMux, UDPMuxDefault, UDPMuxParams},
        udp_network::UDPNetwork,
    },
    ice_transport::ice_candidate_type::RTCIceCandidateType,
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

#[cfg(unix)]
use std::os::unix::{io::AsRawFd, process::CommandExt};

use crate::{
    auth::SessionGuard,
    config::{H264Config, H264Encoder},
    devices::video::VideoManager,
};

const MAX_OFFER_BYTES: usize = 64 * 1024;
const ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ACCESS_UNIT_BUFFER: usize = 16 * 1024 * 1024;

const DEFAULT_UNCONNECTED_DEADLINE: Duration = Duration::from_secs(30);
const DEFAULT_DISCONNECTED_GRACE: Duration = Duration::from_secs(15);
const DEFAULT_MEDIA_IDLE_DEADLINE: Duration = Duration::from_secs(60);
const SESSION_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const ENCODER_EXIT_GRACE: Duration = Duration::from_secs(2);
const ENCODER_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

/// Lowest bitrate a client may request for adaptive streaming.
pub const MIN_ADAPTIVE_BITRATE_KBPS: u32 = 256;
/// Minimum spacing between two encoder restarts of one session. Every change
/// restarts FFmpeg, so a misbehaving client must not be able to thrash it.
const BITRATE_CHANGE_INTERVAL: Duration = Duration::from_secs(10);

const PROBE_DEADLINE: Duration = Duration::from_secs(3);
const PROBE_MAX_OUTPUT: usize = 1024 * 1024;
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(5);
const PROBE_REAP_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_READ_CHUNK: usize = 16 * 1024;
const MAX_RETAINED_PROBES: usize = 4;
const PROBE_BUSY: &str = "H.264 编码器探测暂时不可用";
const PROBE_PENDING: &str = "H.264 编码器正在后台探测";
const PROBE_CLEANUP_FAILED: &str = "H.264 编码器探测清理失败";

#[cfg(unix)]
struct RetainedProbes {
    children: Vec<StdChild>,
    reserved: usize,
}

#[cfg(unix)]
static RETAINED_PROBES: Mutex<RetainedProbes> = Mutex::new(RetainedProbes {
    children: Vec::new(),
    reserved: 0,
});

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
    /// Configured (maximum) bitrate the session starts at.
    pub bitrate_kbps: u32,
    pub min_bitrate_kbps: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BitrateChange {
    pub session_id: String,
    /// Bitrate now in effect after clamping.
    pub kbps: u32,
    pub min_kbps: u32,
    pub max_kbps: u32,
    /// False when the clamped value equals the running bitrate.
    pub changed: bool,
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
    #[error("WebRTC 会话授权已失效")]
    Unauthorized,
    #[error("WebRTC 会话不存在")]
    SessionNotFound,
    #[error("码率调整过于频繁，请 {0} 秒后重试")]
    RateLimited(u64),
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
    shutdown: watch::Sender<bool>,
    /// Current encoder bitrate; changing it restarts only the FFmpeg process.
    bitrate: watch::Sender<u32>,
    /// Configured bitrate at offer time; clients can never exceed it.
    max_bitrate_kbps: u32,
    last_bitrate_change: Option<Instant>,
    /// Login session that created this peer; only it may tune the bitrate.
    owner: SessionGuard,
    state: watch::Sender<ConnectionPhase>,
    closed: watch::Receiver<bool>,
}

/// Keeps a negotiating peer from outliving a cancelled offer future.
///
/// Publication releases the guard. Every other path, including a future
/// dropped between awaits, closes the peer with the same bounded wait used by
/// established sessions.
struct NegotiationGuard {
    peer: Option<Arc<RTCPeerConnection>>,
}

impl NegotiationGuard {
    fn new(peer: Arc<RTCPeerConnection>) -> Self {
        Self { peer: Some(peer) }
    }

    async fn close(&mut self) {
        if let Some(peer) = self.peer.take() {
            let _ = tokio::time::timeout(SESSION_CLOSE_TIMEOUT, peer.close()).await;
        }
    }

    fn publish(&mut self) {
        self.peer = None;
    }
}

impl Drop for NegotiationGuard {
    fn drop(&mut self) {
        let Some(peer) = self.peer.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        // `close` is async, so a dropped offer must hand the peer to a small
        // owned task. The regular close path still awaits it explicitly; this
        // branch only covers cancellation and unexpected early returns.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(SESSION_CLOSE_TIMEOUT, peer.close()).await;
            });
        }));
    }
}

struct UdpBinding {
    address: SocketAddr,
    mux: Arc<UDPMuxDefault>,
}

struct Inner {
    video: VideoManager,
    config: Mutex<H264Config>,
    encoder: Mutex<Result<EncoderSpec, String>>,
    sessions: Mutex<HashMap<String, Session>>,
    offer_lock: AsyncMutex<()>,
    udp_binding: AsyncMutex<Option<UdpBinding>>,
    /// Guards startup probes from overwriting a newer `reconfigure` result.
    probe_epoch: AtomicU64,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for session in sessions.values() {
            let _ = session.shutdown.send_replace(true);
        }
    }
}

#[derive(Clone)]
pub struct WebRtcManager {
    inner: Arc<Inner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectionPhase {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
    Other,
}

impl ConnectionPhase {
    fn from_rtc(state: RTCPeerConnectionState) -> Self {
        match state {
            RTCPeerConnectionState::New => Self::New,
            RTCPeerConnectionState::Connecting => Self::Connecting,
            RTCPeerConnectionState::Connected => Self::Connected,
            RTCPeerConnectionState::Disconnected => Self::Disconnected,
            RTCPeerConnectionState::Failed => Self::Failed,
            RTCPeerConnectionState::Closed => Self::Closed,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LifecyclePolicy {
    unconnected: Duration,
    disconnected: Duration,
    media_idle: Duration,
    encoder_shutdown: Duration,
}

impl Default for LifecyclePolicy {
    fn default() -> Self {
        Self {
            unconnected: DEFAULT_UNCONNECTED_DEADLINE,
            disconnected: DEFAULT_DISCONNECTED_GRACE,
            media_idle: DEFAULT_MEDIA_IDLE_DEADLINE,
            encoder_shutdown: ENCODER_CLEANUP_TIMEOUT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifecycleExpiry {
    Unconnected,
    Disconnected,
    MediaIdle,
    ConnectionEnded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifecycleDecision {
    Wait(Instant),
    Expire(LifecycleExpiry),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndReason {
    Auth,
    Shutdown,
    Encoder,
    Watchdog(LifecycleExpiry),
}

struct LifecycleEnd {
    reason: EndReason,
    encoder: Option<Result<(), WebRtcError>>,
}

type EncoderFuture = Pin<Box<dyn Future<Output = Result<(), WebRtcError>> + Send>>;

/// Pure lifecycle policy used by the supervisor and by deterministic tests.
///
/// Connection state deadlines are independent of session wall-clock age. A
/// connected session is only reclaimed when it has not written an encoded
/// access unit for `media_idle`; healthy media keeps it alive indefinitely
/// until the authenticated browser session ends.
fn lifecycle_decision(
    now: Instant,
    started: Instant,
    phase: ConnectionPhase,
    phase_since: Instant,
    last_media: Option<Instant>,
    policy: &LifecyclePolicy,
) -> LifecycleDecision {
    let (base, limit, expiry) = match phase {
        ConnectionPhase::New | ConnectionPhase::Connecting | ConnectionPhase::Other => {
            (started, policy.unconnected, LifecycleExpiry::Unconnected)
        }
        ConnectionPhase::Disconnected => (
            phase_since,
            policy.disconnected,
            LifecycleExpiry::Disconnected,
        ),
        ConnectionPhase::Connected => {
            let base = last_media
                .filter(|progress| *progress >= phase_since)
                .unwrap_or(phase_since);
            (base, policy.media_idle, LifecycleExpiry::MediaIdle)
        }
        ConnectionPhase::Failed | ConnectionPhase::Closed => {
            return LifecycleDecision::Expire(LifecycleExpiry::ConnectionEnded);
        }
    };
    let due = base.checked_add(limit).unwrap_or(base);
    if now >= due {
        LifecycleDecision::Expire(expiry)
    } else {
        LifecycleDecision::Wait(due)
    }
}

/// Run the potentially slow FFmpeg probe without blocking the caller.
///
/// The epoch check prevents a startup probe that finishes after a
/// `reconfigure` from overwriting the newer encoder choice.
fn spawn_startup_probe(inner: Arc<Inner>, config: H264Config) {
    let epoch = inner.probe_epoch.load(Ordering::SeqCst);
    let probe = move || {
        let encoder = probe_encoder(&config);
        if let Err(message) = &encoder {
            tracing::info!(%message, "H.264 WebRTC transport is unavailable");
        }
        if inner.probe_epoch.load(Ordering::SeqCst) == epoch {
            *inner
                .encoder
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = encoder;
        }
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                if let Err(error) = tokio::task::spawn_blocking(probe).await {
                    tracing::warn!(%error, "H.264 编码器后台探测失败");
                }
            });
        }
        Err(_) => {
            // Construction outside a Tokio runtime still must not block; a
            // dedicated thread performs the one-off probe.
            std::thread::spawn(probe);
        }
    }
}

impl WebRtcManager {
    pub fn new(video: VideoManager, config: H264Config) -> Self {
        // Encoder discovery shells out to FFmpeg and may take seconds. Publish
        // a "probing" state immediately and finish the probe in the background
        // so startup can begin listening without waiting for the media stack.
        let inner = Arc::new(Inner {
            video,
            config: Mutex::new(config.clone()),
            encoder: Mutex::new(Err(PROBE_PENDING.to_owned())),
            sessions: Mutex::new(HashMap::new()),
            offer_lock: AsyncMutex::new(()),
            udp_binding: AsyncMutex::new(None),
            probe_epoch: AtomicU64::new(0),
        });
        spawn_startup_probe(Arc::clone(&inner), config);
        Self { inner }
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
        let _offer_guard = self.inner.offer_lock.lock().await;
        // Invalidate any startup probe that may still be running.
        self.inner.probe_epoch.fetch_add(1, Ordering::SeqCst);
        let udp_bind_address = config.udp_bind_address;
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
            close_session(&self.inner, &session_id).await;
        }
        let mut binding = self.inner.udp_binding.lock().await;
        if binding
            .as_ref()
            .is_some_and(|binding| Some(binding.address) != udp_bind_address)
            && let Some(binding) = binding.take()
        {
            let _ = binding.mux.close().await;
        }
    }

    async fn udp_mux(&self, address: SocketAddr) -> Result<Arc<UDPMuxDefault>, WebRtcError> {
        let mut binding = self.inner.udp_binding.lock().await;
        if let Some(binding) = binding.as_ref() {
            if binding.address != address {
                return Err(WebRtcError::Unavailable(
                    "WebRTC UDP 监听配置尚未切换".to_owned(),
                ));
            }
            return Ok(binding.mux.clone());
        }
        let socket = UdpSocket::bind(address).await.map_err(|error| {
            WebRtcError::Unavailable(format!("无法监听 WebRTC UDP {address}：{error}"))
        })?;
        let mux = UDPMuxDefault::new(UDPMuxParams::new(socket));
        *binding = Some(UdpBinding {
            address,
            mux: mux.clone(),
        });
        tracing::info!(%address, "WebRTC shared UDP listener is ready");
        Ok(mux)
    }

    pub async fn offer(
        &self,
        request: WebRtcOfferRequest,
        guard: SessionGuard,
    ) -> Result<WebRtcOfferResponse, WebRtcError> {
        if !guard.is_active() {
            return Err(WebRtcError::Unauthorized);
        }
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
        if !guard.is_active() {
            return Err(WebRtcError::Unauthorized);
        }
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

        let mut settings = SettingEngine::default();
        let mut public_endpoint = None;
        if let Some(bind_address) = config.udp_bind_address {
            settings.set_udp_network(UDPNetwork::Muxed(self.udp_mux(bind_address).await?));
            settings.set_network_types(vec![if bind_address.is_ipv4() {
                NetworkType::Udp4
            } else {
                NetworkType::Udp6
            }]);
            if let Some(endpoint) = config.public_udp_endpoint.as_deref() {
                let address = tokio::time::timeout(Duration::from_secs(3), lookup_host(endpoint))
                    .await
                    .map_err(|_| {
                        WebRtcError::Unavailable("解析 WebRTC 公网 UDP 地址超时".to_owned())
                    })?
                    .map_err(|error| {
                        WebRtcError::Unavailable(format!("无法解析 WebRTC 公网 UDP 地址：{error}"))
                    })?
                    .find(|address| {
                        address.is_ipv4() == bind_address.is_ipv4()
                            && !address.ip().is_unspecified()
                            && !address.ip().is_multicast()
                    })
                    .ok_or_else(|| {
                        WebRtcError::Unavailable(
                            "WebRTC 公网地址与 UDP 监听地址的 IP 协议不一致".to_owned(),
                        )
                    })?;
                settings
                    .set_nat_1to1_ips(vec![address.ip().to_string()], RTCIceCandidateType::Host);
                public_endpoint = Some((address, bind_address.port()));
            }
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
            .with_setting_engine(settings)
            .build();
        let peer = Arc::new(
            api.new_peer_connection(RTCConfiguration::default())
                .await
                .map_err(internal_error)?,
        );
        let mut negotiation = NegotiationGuard::new(peer.clone());
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
            negotiation.close().await;
            return Err(WebRtcError::InvalidOffer(error.to_string()));
        }
        let mut local = match async {
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
                negotiation.close().await;
                return Err(error);
            }
        };

        if let Some((endpoint, local_port)) = public_endpoint {
            // The library's 1:1 NAT option rewrites IPs, but does not support
            // a DNAT port change. Advertise that port only in the wire SDP;
            // the ICE agent continues to receive on its shared local socket.
            match advertise_udp_endpoint(&local.sdp, endpoint, local_port) {
                Ok(sdp) => local.sdp = sdp,
                Err(error) => {
                    negotiation.close().await;
                    return Err(error);
                }
            }
        }

        if !guard.is_active() {
            negotiation.close().await;
            return Err(WebRtcError::Unauthorized);
        }

        let session_id = generate_session_id()?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (state_tx, state_rx) = watch::channel(ConnectionPhase::New);
        let (closed_tx, closed_rx) = watch::channel(false);
        let (bitrate_tx, bitrate_rx) = watch::channel(config.bitrate_kbps);
        let publication = {
            let mut sessions = self
                .inner
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // Recheck authorization and capacity while the map is locked so a
            // revoke or another concurrent offer cannot publish a peer that
            // was never authorized or that exceeds the session limit. All
            // async closing happens after this lexical lock scope ends.
            if !guard.is_active() {
                Err(WebRtcError::Unauthorized)
            } else if sessions.len() >= usize::from(config.max_sessions) {
                Err(WebRtcError::Capacity)
            } else {
                sessions.insert(
                    session_id.clone(),
                    Session {
                        shutdown: shutdown_tx,
                        bitrate: bitrate_tx,
                        max_bitrate_kbps: config.bitrate_kbps,
                        last_bitrate_change: None,
                        owner: guard.clone(),
                        state: state_tx,
                        closed: closed_rx,
                    },
                );
                Ok(())
            }
        };
        if let Err(error) = publication {
            negotiation.close().await;
            return Err(error);
        }
        // Register only after the map entry exists, so every state transition
        // can update the supervisor's channel rather than being dropped.
        install_connection_cleanup(&self.inner, &peer, &session_id);
        negotiation.publish();

        let inner = Arc::downgrade(&self.inner);
        let task_session_id = session_id.clone();
        let video = self.inner.video.clone();
        tokio::spawn(run_session_supervisor(
            inner,
            task_session_id,
            peer,
            video,
            track,
            encoder,
            bitrate_rx,
            guard,
            shutdown_rx,
            state_rx,
            closed_tx,
            LifecyclePolicy::default(),
        ));

        Ok(WebRtcOfferResponse {
            sdp_type: "answer",
            sdp: local.sdp,
            session_id,
            bitrate_kbps: config.bitrate_kbps,
            min_bitrate_kbps: MIN_ADAPTIVE_BITRATE_KBPS.min(config.bitrate_kbps),
        })
    }

    /// Change the encoder bitrate of one session owned by `guard`.
    ///
    /// The request is clamped to `[256, configured bitrate]`, a session owned
    /// by another login is reported exactly like an unknown one, and real
    /// changes are limited to one per `BITRATE_CHANGE_INTERVAL`.
    pub fn set_bitrate(
        &self,
        session_id: &str,
        kbps: u32,
        guard: &SessionGuard,
    ) -> Result<BitrateChange, WebRtcError> {
        set_session_bitrate(&self.inner, session_id, kbps, guard, Instant::now())
    }

    pub async fn close(&self, session_id: &str) -> bool {
        close_session(&self.inner, session_id).await
    }
}

fn set_session_bitrate(
    inner: &Inner,
    session_id: &str,
    kbps: u32,
    guard: &SessionGuard,
    now: Instant,
) -> Result<BitrateChange, WebRtcError> {
    if !guard.is_active() {
        return Err(WebRtcError::Unauthorized);
    }
    let mut sessions = inner
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = sessions
        .get_mut(session_id)
        .filter(|session| session.owner.same_session(guard))
        .ok_or(WebRtcError::SessionNotFound)?;
    let max_kbps = session.max_bitrate_kbps;
    let min_kbps = MIN_ADAPTIVE_BITRATE_KBPS.min(max_kbps);
    let target = kbps.clamp(min_kbps, max_kbps);
    let current = *session.bitrate.borrow();
    let changed = target != current;
    if changed {
        if let Some(last) = session.last_bitrate_change {
            let elapsed = now.saturating_duration_since(last);
            if elapsed < BITRATE_CHANGE_INTERVAL {
                let wait = (BITRATE_CHANGE_INTERVAL - elapsed).as_secs_f64().ceil() as u64;
                return Err(WebRtcError::RateLimited(wait.max(1)));
            }
        }
        session.bitrate.send_replace(target);
        session.last_bitrate_change = Some(now);
        tracing::info!(%session_id, from = current, to = target, "WebRTC adaptive bitrate changed");
    }
    Ok(BitrateChange {
        session_id: session_id.to_owned(),
        kbps: target,
        min_kbps,
        max_kbps,
        changed,
    })
}

#[cfg(test)]
impl WebRtcManager {
    /// Manager without an encoder probe, for HTTP handler tests.
    pub(crate) fn for_tests() -> Self {
        Self {
            inner: Arc::new(Inner {
                video: VideoManager::new(crate::config::VideoConfig::default()),
                config: Mutex::new(H264Config::default()),
                encoder: Mutex::new(Err("test manager".to_owned())),
                sessions: Mutex::new(HashMap::new()),
                offer_lock: AsyncMutex::new(()),
                udp_binding: AsyncMutex::new(None),
                probe_epoch: AtomicU64::new(0),
            }),
        }
    }

    /// Register a session map entry without a peer or encoder.
    pub(crate) fn insert_test_session(
        &self,
        session_id: &str,
        owner: SessionGuard,
        max_bitrate_kbps: u32,
    ) -> watch::Receiver<u32> {
        let (bitrate_tx, bitrate_rx) = watch::channel(max_bitrate_kbps);
        let (closed_tx, closed_rx) = watch::channel(false);
        // Keep `closed` open so the entry looks live.
        std::mem::forget(closed_tx);
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                session_id.to_owned(),
                Session {
                    shutdown: watch::channel(false).0,
                    bitrate: bitrate_tx,
                    max_bitrate_kbps,
                    last_bitrate_change: None,
                    owner,
                    state: watch::channel(ConnectionPhase::New).0,
                    closed: closed_rx,
                },
            );
        bitrate_rx
    }
}

async fn close_session(inner: &Inner, session_id: &str) -> bool {
    let (shutdown, mut closed) = {
        let sessions = inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get(session_id) else {
            return false;
        };
        (session.shutdown.clone(), session.closed.clone())
    };
    let _ = shutdown.send_replace(true);
    // The supervisor removes the map entry before closing the peer and then
    // signals this channel. Bound the wait so an unhealthy peer cannot stall
    // HTTP configuration updates indefinitely.
    let _ = tokio::time::timeout(SESSION_CLOSE_TIMEOUT, wait_for_true(&mut closed)).await;
    true
}

async fn wait_for_true(receiver: &mut watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    let _ = receiver.changed().await;
}

fn advertise_udp_endpoint(
    sdp: &str,
    endpoint: SocketAddr,
    local_port: u16,
) -> Result<String, WebRtcError> {
    let mut mapped = 0;
    let mut lines = Vec::new();
    for line in sdp.lines() {
        if let Some(candidate) = line.strip_prefix("a=candidate:") {
            let mut fields = candidate
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if fields.len() >= 8
                && fields[2].eq_ignore_ascii_case("udp")
                && fields[4].parse::<IpAddr>().ok() == Some(endpoint.ip())
                && fields[5].parse::<u16>().ok() == Some(local_port)
                && fields[6] == "typ"
                && fields[7] == "host"
            {
                fields[5] = endpoint.port().to_string();
                lines.push(format!("a=candidate:{}", fields.join(" ")));
                mapped += 1;
                continue;
            }
        }
        lines.push(line.to_owned());
    }
    if mapped == 0 {
        return Err(WebRtcError::Internal(
            "WebRTC 未生成可映射的固定 UDP 候选".to_owned(),
        ));
    }
    Ok(format!("{}\r\n", lines.join("\r\n")))
}

fn install_connection_cleanup(inner: &Arc<Inner>, peer: &Arc<RTCPeerConnection>, session_id: &str) {
    let weak = Arc::downgrade(inner);
    let session_id = session_id.to_owned();
    peer.on_peer_connection_state_change(Box::new(move |state| {
        let weak = weak.clone();
        let session_id = session_id.clone();
        Box::pin(async move {
            tracing::info!(%session_id, ?state, "WebRTC connection state changed");
            let phase = ConnectionPhase::from_rtc(state);
            if let Some(inner) = weak.upgrade() {
                let sessions = inner
                    .sessions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(session) = sessions.get(&session_id) {
                    session.state.send_replace(phase);
                    if matches!(phase, ConnectionPhase::Failed | ConnectionPhase::Closed) {
                        let _ = session.shutdown.send_replace(true);
                    }
                }
            }
        })
    }));
}

/// Removes the session map entry, closes the peer and wakes `close` waiters on
/// every exit path, including a panic inside the supervisor.
struct SessionCleanup {
    inner: Weak<Inner>,
    session_id: String,
    peer: Option<Arc<RTCPeerConnection>>,
    closed: Option<watch::Sender<bool>>,
}

impl SessionCleanup {
    fn new(
        inner: Weak<Inner>,
        session_id: String,
        peer: Arc<RTCPeerConnection>,
        closed: watch::Sender<bool>,
    ) -> Self {
        Self {
            inner,
            session_id,
            peer: Some(peer),
            closed: Some(closed),
        }
    }

    fn remove_session(&self) {
        // Remove the map entry before closing the peer. The peer state callback
        // re-enters the map; if it ran during close while the entry still
        // existed it could signal a session that is already being torn down.
        if let Some(inner) = self.inner.upgrade() {
            inner
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&self.session_id);
        }
    }

    fn signal_closed(&mut self) {
        if let Some(closed) = self.closed.take() {
            let _ = closed.send_replace(true);
        }
    }

    async fn shutdown(&mut self) {
        self.remove_session();
        if let Some(peer) = self.peer.take() {
            let _ = tokio::time::timeout(SESSION_CLOSE_TIMEOUT, peer.close()).await;
        }
        self.signal_closed();
    }
}

impl Drop for SessionCleanup {
    fn drop(&mut self) {
        self.remove_session();
        if let Some(peer) = self.peer.take() {
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                self.signal_closed();
                return;
            };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(SESSION_CLOSE_TIMEOUT, peer.close()).await;
                });
            }));
        }
        self.signal_closed();
    }
}

/// The one lifecycle task for a session. It owns the peer, the encoder future,
/// the shutdown receiver and the authenticated session guard. The map only
/// keeps signalling channels, never a strong reference back to `Inner`.
#[allow(clippy::too_many_arguments)]
async fn run_session_supervisor(
    inner: Weak<Inner>,
    session_id: String,
    peer: Arc<RTCPeerConnection>,
    video: VideoManager,
    track: Arc<TrackLocalStaticSample>,
    encoder: EncoderSpec,
    bitrate: watch::Receiver<u32>,
    guard: SessionGuard,
    shutdown: watch::Receiver<bool>,
    state: watch::Receiver<ConnectionPhase>,
    closed: watch::Sender<bool>,
    policy: LifecyclePolicy,
) {
    // Construct the cleanup guard before any fallible/panicking work so the
    // session map entry cannot outlive this task.
    let mut cleanup = SessionCleanup::new(inner, session_id.clone(), peer, closed);
    let (progress_tx, progress_rx) = mpsc::channel(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let encoder_future: EncoderFuture = Box::pin(run_encoder(
        video,
        track,
        encoder,
        cancel_rx,
        bitrate,
        progress_tx,
    ));
    let end = supervise_lifecycle(
        &guard,
        shutdown,
        state,
        progress_rx,
        encoder_future,
        cancel_tx,
        policy,
    )
    .await;

    match &end.encoder {
        Some(Ok(())) => tracing::info!(%session_id, "H.264 encoder session ended"),
        Some(Err(error)) => {
            tracing::warn!(%session_id, %error, "H.264 encoder session stopped");
        }
        None => {
            tracing::info!(%session_id, reason = ?end.reason, "WebRTC session lifecycle ended");
        }
    }

    cleanup.shutdown().await;
}

async fn supervise_lifecycle(
    guard: &SessionGuard,
    mut shutdown: watch::Receiver<bool>,
    mut state: watch::Receiver<ConnectionPhase>,
    mut progress: mpsc::Receiver<()>,
    mut encoder: EncoderFuture,
    cancel: watch::Sender<bool>,
    policy: LifecyclePolicy,
) -> LifecycleEnd {
    let started = Instant::now();
    let mut phase = *state.borrow_and_update();
    let mut phase_since = started;
    let mut last_media = None;
    let mut progress_open = true;
    let mut encoder_done = false;
    let mut encoder_result = None;

    let reason = loop {
        if !guard.is_active() {
            break EndReason::Auth;
        }
        let now = Instant::now();
        let due = match lifecycle_decision(now, started, phase, phase_since, last_media, &policy) {
            LifecycleDecision::Expire(reason) => break EndReason::Watchdog(reason),
            LifecycleDecision::Wait(due) => due,
        };
        let sleep = tokio::time::sleep_until(tokio::time::Instant::from_std(due));
        tokio::pin!(sleep);
        tokio::select! {
            biased;
            _ = guard.wait_for_end() => break EndReason::Auth,
            _ = wait_for_true(&mut shutdown) => break EndReason::Shutdown,
            result = state.changed() => {
                if result.is_err() {
                    break EndReason::Shutdown;
                }
                let next = *state.borrow_and_update();
                if next != phase {
                    phase = next;
                    phase_since = Instant::now();
                }
            }
            maybe = progress.recv(), if progress_open => match maybe {
                Some(()) => last_media = Some(Instant::now()),
                None => progress_open = false,
            },
            result = &mut encoder, if !encoder_done => {
                encoder_done = true;
                encoder_result = Some(result);
                break EndReason::Encoder;
            }
            _ = &mut sleep => {}
        }
    };

    if !encoder_done {
        // Cooperative cancellation lets `run_encoder` kill and reap its child
        // and abort the stdin/stdout/stderr tasks. If that takes too long the
        // RAII guard cancels the child and tasks on drop.
        let _ = cancel.send_replace(true);
        let _ = tokio::time::timeout(policy.encoder_shutdown, &mut encoder).await;
    }

    LifecycleEnd {
        reason,
        encoder: encoder_result,
    }
}

fn generate_session_id() -> Result<String, WebRtcError> {
    let mut bytes = [0_u8; 18];
    getrandom::fill(&mut bytes)
        .map_err(|error| WebRtcError::Internal(format!("无法生成会话标识：{error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[derive(Clone, Copy)]
struct ProbeLimits {
    deadline: Duration,
    max_output: usize,
}

impl Default for ProbeLimits {
    fn default() -> Self {
        Self {
            deadline: PROBE_DEADLINE,
            max_output: PROBE_MAX_OUTPUT,
        }
    }
}

fn probe_encoder(config: &H264Config) -> Result<EncoderSpec, String> {
    probe_encoder_with(config, ProbeLimits::default())
}

fn probe_encoder_with(config: &H264Config, limits: ProbeLimits) -> Result<EncoderSpec, String> {
    config.validate_udp_mapping().map_err(str::to_owned)?;
    let executable = config
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let listing = run_probe_command(&executable, limits)?;
    let (codec, label, software) = select_encoder(config, &listing)?;
    Ok(EncoderSpec {
        executable,
        codec,
        label,
        software,
    })
}

#[cfg(unix)]
struct ProbeReservation;

#[cfg(unix)]
impl ProbeReservation {
    /// Poll retained children and reserve one of the bounded slots. The slot is
    /// held until this probe has either reaped its child or retained it.
    fn acquire() -> Result<Self, String> {
        let mut probes = RETAINED_PROBES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut index = 0;
        while index < probes.children.len() {
            match probes.children[index].try_wait() {
                Ok(Some(_)) => {
                    probes.children.swap_remove(index);
                }
                Ok(None) | Err(_) => index += 1,
            }
        }
        if probes.children.len() + probes.reserved >= MAX_RETAINED_PROBES {
            return Err(PROBE_BUSY.to_owned());
        }
        probes.reserved += 1;
        Ok(Self)
    }
}

#[cfg(unix)]
impl Drop for ProbeReservation {
    fn drop(&mut self) {
        let mut probes = RETAINED_PROBES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        probes.reserved = probes.reserved.saturating_sub(1);
    }
}

#[cfg(unix)]
struct ProbeProcess {
    child: Option<StdChild>,
    _reservation: ProbeReservation,
}

#[cfg(unix)]
impl ProbeProcess {
    fn new(child: StdChild, reservation: ProbeReservation) -> Self {
        Self {
            child: Some(child),
            _reservation: reservation,
        }
    }

    fn child_mut(&mut self) -> &mut StdChild {
        self.child.as_mut().expect("probe child is owned")
    }

    fn poll_exit(&mut self, deadline: Instant) -> Option<ExitStatus> {
        loop {
            let status = match self.child.as_mut()?.try_wait() {
                Ok(Some(status)) => status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return None;
                    }
                    std::thread::sleep(PROBE_POLL_INTERVAL);
                    continue;
                }
                Err(_) => return None,
            };
            self.child = None;
            return Some(status);
        }
    }

    /// Kill the private group before the leader is reaped. If SIGKILL cannot
    /// reap it promptly, hand ownership to the bounded retained-child registry
    /// instead of waiting without a bound or dropping the handle.
    fn terminate(&mut self) -> bool {
        let Some(mut child) = self.child.take() else {
            return true;
        };
        signal_probe_group(&child);
        let _ = child.kill();
        if wait_for_probe_reap(&mut child, PROBE_REAP_TIMEOUT) {
            true
        } else {
            retain_probe_child(child);
            false
        }
    }
}

#[cfg(unix)]
impl Drop for ProbeProcess {
    fn drop(&mut self) {
        // Covers panics and a future dropped between ownership transfer and the
        // explicit termination paths.
        let _ = self.terminate();
    }
}

#[cfg(unix)]
fn retain_probe_child(child: StdChild) {
    let mut probes = RETAINED_PROBES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // A reservation was held for this probe, so the registry cannot grow past
    // MAX_RETAINED_PROBES even when several probes finish concurrently.
    probes.children.push(child);
}

#[cfg(unix)]
fn signal_probe_group(child: &StdChild) {
    let pid = child.id();
    if let Ok(pid) = i32::try_from(pid)
        && pid > 1
    {
        // The leader has not been reaped yet, so its private group id cannot
        // have been reused. Signal the group before the child is reaped.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
fn wait_for_probe_reap(child: &mut StdChild, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(PROBE_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn probe_failure(message: &str, reaped: bool) -> String {
    if reaped {
        message.to_owned()
    } else {
        PROBE_CLEANUP_FAILED.to_owned()
    }
}

#[cfg(unix)]
fn run_probe_command(executable: &Path, limits: ProbeLimits) -> Result<String, String> {
    let reservation = ProbeReservation::acquire()?;
    let mut command = StdCommand::new(executable);
    command
        .args(["-hide_banner", "-encoders"])
        .stdin(StdStdio::null())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped());
    // A private process group lets timeout/limit paths terminate the leader and
    // every descendant that could still be holding the pipes.
    command.process_group(0);
    let child = command
        .spawn()
        .map_err(|_| "无法启动 FFmpeg 进行编码器探测".to_owned())?;
    let mut probe = ProbeProcess::new(child, reservation);
    let mut stdout = match probe.child_mut().stdout.take() {
        Some(stdout) => stdout,
        None => {
            let reaped = probe.terminate();
            return Err(probe_failure("H.264 编码器探测读取失败", reaped));
        }
    };
    let mut stderr = match probe.child_mut().stderr.take() {
        Some(stderr) => stderr,
        None => {
            let reaped = probe.terminate();
            return Err(probe_failure("H.264 编码器探测读取失败", reaped));
        }
    };
    if set_nonblocking(&stdout).is_err() || set_nonblocking(&stderr).is_err() {
        let reaped = probe.terminate();
        return Err(probe_failure("H.264 编码器探测读取失败", reaped));
    }

    let deadline = Instant::now() + limits.deadline;
    let mut remaining = limits.max_output;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut stdout_open = true;
    let mut stderr_open = true;

    loop {
        if stdout_open {
            match drain_pipe(&mut stdout, &mut stdout_bytes, &mut remaining) {
                Ok(open) => stdout_open = open,
                Err(ProbeReadError::Limit) => {
                    let reaped = probe.terminate();
                    return Err(probe_failure("H.264 编码器探测输出超过限制", reaped));
                }
                Err(ProbeReadError::Io) => {
                    let reaped = probe.terminate();
                    return Err(probe_failure("H.264 编码器探测读取失败", reaped));
                }
            }
        }
        if stderr_open {
            match drain_pipe(&mut stderr, &mut stderr_bytes, &mut remaining) {
                Ok(open) => stderr_open = open,
                Err(ProbeReadError::Limit) => {
                    let reaped = probe.terminate();
                    return Err(probe_failure("H.264 编码器探测输出超过限制", reaped));
                }
                Err(ProbeReadError::Io) => {
                    let reaped = probe.terminate();
                    return Err(probe_failure("H.264 编码器探测读取失败", reaped));
                }
            }
        }

        if !stdout_open && !stderr_open {
            match probe.poll_exit(deadline) {
                Some(status) if status.success() => break,
                Some(_) => return Err("FFmpeg 无法列出编码器".to_owned()),
                None => {
                    let reaped = probe.terminate();
                    return Err(probe_failure("H.264 编码器探测超时", reaped));
                }
            }
        }
        if Instant::now() >= deadline {
            // A descendant may still hold the pipes after the leader exits.
            // Kill the whole private group before reaping the leader.
            let reaped = probe.terminate();
            return Err(probe_failure("H.264 编码器探测超时", reaped));
        }
        std::thread::sleep(PROBE_POLL_INTERVAL);
    }

    let mut listing = String::from_utf8_lossy(&stdout_bytes).into_owned();
    listing.push('\n');
    listing.push_str(&String::from_utf8_lossy(&stderr_bytes));
    Ok(listing)
}

#[cfg(not(unix))]
fn run_probe_command(_executable: &Path, _limits: ProbeLimits) -> Result<String, String> {
    // The subprocess supervision below relies on Unix nonblocking pipes and
    // process groups. Prefer an explicit, safe unavailability over an
    // unbounded `Command::output` fallback.
    Err("当前平台不支持有界的 H.264 编码器探测".to_owned())
}

enum ProbeReadError {
    Limit,
    Io,
}

#[cfg(unix)]
fn drain_pipe<R: Read>(
    reader: &mut R,
    output: &mut Vec<u8>,
    remaining: &mut usize,
) -> Result<bool, ProbeReadError> {
    let mut chunk = [0_u8; PROBE_READ_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(false),
            Ok(read) => {
                if read > *remaining {
                    return Err(ProbeReadError::Limit);
                }
                *remaining -= read;
                output.extend_from_slice(&chunk[..read]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ProbeReadError::Io),
        }
    }
}

#[cfg(unix)]
fn set_nonblocking<T: AsRawFd>(pipe: &T) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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

struct EncoderGuard {
    child: Option<TokioChild>,
    tasks: Vec<AbortHandle>,
}

#[cfg(unix)]
fn signal_encoder_group(child: &TokioChild) {
    if let Some(pid) = child.id()
        && let Ok(pid) = i32::try_from(pid)
        && pid > 1
    {
        // The private group leader has not been reaped. Kill the group before
        // the leader is waited on so wrapper descendants cannot survive.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn signal_encoder_group(_child: &TokioChild) {}

impl EncoderGuard {
    fn new(child: TokioChild, tasks: Vec<AbortHandle>) -> Self {
        Self {
            child: Some(child),
            tasks,
        }
    }

    async fn wait_for_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let child = self.child.as_mut()?;
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => {
                self.child = None;
                Some(status)
            }
            Ok(Err(_)) => {
                self.child = None;
                None
            }
            Err(_) => None,
        }
    }

    async fn shutdown(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(mut child) = self.child.take() {
            signal_encoder_group(&child);
            let _ = child.start_kill();
            let _ = tokio::time::timeout(ENCODER_CLEANUP_TIMEOUT, child.wait()).await;
        }
    }
}

impl Drop for EncoderGuard {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(child) = self.child.as_mut() {
            signal_encoder_group(child);
            let _ = child.start_kill();
        }
    }
}

async fn wait_for_cancel(receiver: &mut watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    let _ = receiver.changed().await;
}

/// How one FFmpeg process run ended.
enum EncoderRun {
    /// Cancelled by the supervisor or the encoder finished cleanly.
    Finished,
    /// The session asked for a different bitrate; respawn with this value.
    Restart(u32),
}

/// Wait until the requested bitrate differs from the running one. A dropped
/// sender means the bitrate can no longer change, so this pends forever.
async fn wait_for_bitrate_change(receiver: &mut watch::Receiver<u32>, current: u32) -> u32 {
    loop {
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
        let next = *receiver.borrow_and_update();
        if next != current {
            return next;
        }
    }
}

/// Run the per-session encoder, restarting only the FFmpeg process when the
/// adaptive bitrate changes.
///
/// FFmpeg's stdin carries the MJPEG input, so its interactive commands cannot
/// be used, and none of the supported encoders (libx264, h264_rkmpp,
/// h264_v4l2m2m) accept a runtime `-b:v` change through FFmpeg anyway. The
/// peer connection and the `TrackLocalStaticSample` stay up, so no WebRTC
/// renegotiation happens: the new process starts with an IDR frame (with
/// SPS/PPS in band), the browser decoder resumes from it, and the visible gap
/// is the FFmpeg start-up time. Callers rate-limit changes.
async fn run_encoder(
    video: VideoManager,
    track: Arc<TrackLocalStaticSample>,
    encoder: EncoderSpec,
    mut cancel: watch::Receiver<bool>,
    mut bitrate: watch::Receiver<u32>,
    progress: mpsc::Sender<()>,
) -> Result<(), WebRtcError> {
    let mut kbps = *bitrate.borrow_and_update();
    loop {
        match run_encoder_process(
            &video,
            track.clone(),
            &encoder,
            kbps,
            &mut cancel,
            &mut bitrate,
            progress.clone(),
        )
        .await?
        {
            EncoderRun::Finished => return Ok(()),
            EncoderRun::Restart(next) => {
                tracing::info!(from = kbps, to = next, "restarting H.264 encoder for new bitrate");
                kbps = next;
            }
        }
    }
}

async fn run_encoder_process(
    video: &VideoManager,
    track: Arc<TrackLocalStaticSample>,
    encoder: &EncoderSpec,
    bitrate_kbps: u32,
    cancel: &mut watch::Receiver<bool>,
    bitrate_updates: &mut watch::Receiver<u32>,
    progress: mpsc::Sender<()>,
) -> Result<EncoderRun, WebRtcError> {
    let fps = video
        .status()
        .frames_per_second
        .unwrap_or(30.0)
        .round()
        .clamp(1.0, 120.0) as u32;
    let bitrate = format!("{bitrate_kbps}k");
    let buffer_size = format!("{}k", bitrate_kbps.saturating_mul(2));
    let fps_text = fps.to_string();
    let mut command = TokioCommand::new(&encoder.executable);
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
        .stdin(StdStdio::piped())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped());
    #[cfg(unix)]
    command.as_std_mut().process_group(0);

    let mut child = command
        .spawn()
        .map_err(|error| WebRtcError::Unavailable(format!("无法启动 H.264 编码器：{error}")))?;

    let Some(mut stdin) = child.stdin.take() else {
        signal_encoder_group(&child);
        let _ = child.start_kill();
        let _ = tokio::time::timeout(ENCODER_CLEANUP_TIMEOUT, child.wait()).await;
        return Err(WebRtcError::Internal("无法连接 FFmpeg 输入".to_owned()));
    };
    let Some(mut stdout) = child.stdout.take() else {
        signal_encoder_group(&child);
        let _ = child.start_kill();
        let _ = tokio::time::timeout(ENCODER_CLEANUP_TIMEOUT, child.wait()).await;
        return Err(WebRtcError::Internal("无法连接 FFmpeg 输出".to_owned()));
    };
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(message = %line, "FFmpeg H.264 encoder");
            }
        })
    });

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
                    let _ = progress.try_send(());
                }
                return Ok::<(), String>(());
            }
            let access_units = parser.push(&chunk[..read])?;
            for access_unit in access_units {
                write_access_unit(&track, access_unit, frame_duration).await?;
                // Progress is recorded only after an access unit was actually
                // accepted by the track, never from the configured FPS alone.
                let _ = progress.try_send(());
            }
        }
    });

    let mut abort_handles = vec![writer.abort_handle(), reader.abort_handle()];
    if let Some(task) = stderr_task {
        abort_handles.push(task.abort_handle());
    }
    let mut process = EncoderGuard::new(child, abort_handles);

    let result = tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => Ok(EncoderRun::Finished),
        next = wait_for_bitrate_change(bitrate_updates, bitrate_kbps) => Ok(EncoderRun::Restart(next)),
        result = &mut writer => match result {
            Ok(Ok(())) => Err(WebRtcError::Internal("FFmpeg 输入意外结束".to_owned())),
            Ok(Err(error)) => Err(WebRtcError::Internal(error)),
            Err(error) => Err(WebRtcError::Internal(error.to_string())),
        },
        result = &mut reader => match result {
            Ok(Ok(())) => match process.wait_for_exit(ENCODER_EXIT_GRACE).await {
                Some(status) if status.success() => Ok(EncoderRun::Finished),
                Some(_) => Err(WebRtcError::Unavailable(
                    "H.264 编码器异常退出".to_owned(),
                )),
                None => Err(WebRtcError::Internal(
                    "H.264 编码器未按预期退出".to_owned(),
                )),
            },
            Ok(Err(error)) => Err(WebRtcError::Internal(error)),
            Err(error) => Err(WebRtcError::Internal(error.to_string())),
        },
    };
    process.shutdown().await;
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
    use crate::config::VideoConfig;
    use std::{fs, future::pending};

    #[cfg(unix)]
    static PROBE_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    fn serial_probe_fixture() -> std::sync::MutexGuard<'static, ()> {
        PROBE_FIXTURE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn advertises_udp_dnat_port_without_changing_other_sdp_fields() {
        let sdp = concat!(
            "v=0\r\n",
            "a=ice-pwd:keep-this-value\r\n",
            "a=candidate:1 1 udp 100 203.0.113.10 8080 typ host generation 0\r\n",
            "a=candidate:2 1 tcp 90 203.0.113.10 8080 typ host tcptype passive\r\n",
            "a=candidate:3 1 udp 80 192.168.1.50 8080 typ host\r\n",
            "a=candidate:4 1 udp 70 203.0.113.10 9000 typ host\r\n",
        );
        let mapped =
            advertise_udp_endpoint(sdp, "203.0.113.10:38319".parse().unwrap(), 8080).unwrap();
        assert_eq!(
            mapped,
            sdp.replacen(
                "203.0.113.10 8080 typ host generation",
                "203.0.113.10 38319 typ host generation",
                1
            )
        );
    }

    #[test]
    fn refuses_to_advertise_an_endpoint_without_a_matching_udp_candidate() {
        let sdp = "v=0\r\na=candidate:1 1 udp 100 192.168.1.50 8080 typ host\r\n";
        assert!(advertise_udp_endpoint(sdp, "203.0.113.10:38319".parse().unwrap(), 8080).is_err());
    }

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

    #[test]
    fn lifecycle_policy_allows_healthy_media_past_ten_minutes() {
        let base = Instant::now();
        let policy = LifecyclePolicy::default();
        let eleven_minutes = Duration::from_secs(11 * 60);

        // Healthy connected media continues indefinitely.
        let decision = lifecycle_decision(
            base + eleven_minutes,
            base,
            ConnectionPhase::Connected,
            base,
            Some(base + eleven_minutes - Duration::from_secs(10)),
            &policy,
        );
        assert!(matches!(decision, LifecycleDecision::Wait(_)));

        // The same session with no encoded output for the idle window is
        // reclaimed, even though it is younger than the old 10 minute TTL.
        let decision = lifecycle_decision(
            base + Duration::from_secs(61),
            base,
            ConnectionPhase::Connected,
            base,
            None,
            &policy,
        );
        assert_eq!(
            decision,
            LifecycleDecision::Expire(LifecycleExpiry::MediaIdle)
        );
    }

    #[test]
    fn lifecycle_policy_reclaims_pending_disconnected_and_ended_sessions() {
        let base = Instant::now();
        let policy = LifecyclePolicy::default();
        assert_eq!(
            lifecycle_decision(
                base + Duration::from_secs(30),
                base,
                ConnectionPhase::Connecting,
                base,
                None,
                &policy,
            ),
            LifecycleDecision::Expire(LifecycleExpiry::Unconnected)
        );
        assert_eq!(
            lifecycle_decision(
                base + Duration::from_secs(16),
                base,
                ConnectionPhase::Disconnected,
                base + Duration::from_secs(1),
                None,
                &policy,
            ),
            LifecycleDecision::Expire(LifecycleExpiry::Disconnected)
        );
        assert_eq!(
            lifecycle_decision(base, base, ConnectionPhase::Failed, base, None, &policy,),
            LifecycleDecision::Expire(LifecycleExpiry::ConnectionEnded)
        );
        assert!(matches!(
            lifecycle_decision(
                base + Duration::from_secs(29),
                base,
                ConnectionPhase::New,
                base,
                None,
                &policy,
            ),
            LifecycleDecision::Wait(_)
        ));
    }

    fn test_policy() -> LifecyclePolicy {
        LifecyclePolicy {
            unconnected: Duration::from_secs(10),
            disconnected: Duration::from_secs(10),
            media_idle: Duration::from_secs(10),
            encoder_shutdown: Duration::ZERO,
        }
    }

    fn pending_encoder() -> EncoderFuture {
        Box::pin(pending())
    }

    #[tokio::test]
    async fn supervisor_stops_on_session_revocation() {
        let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::New);
        let (_progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                pending_encoder(),
                cancel_tx,
                test_policy(),
            )
            .await
        });

        assert!(sessions.revoke(&token));
        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("supervisor should stop")
            .unwrap();
        assert_eq!(end.reason, EndReason::Auth);
    }

    #[tokio::test]
    async fn supervisor_stops_when_session_expires() {
        let sessions = crate::auth::SessionStore::new(Duration::from_millis(50));
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::New);
        let (_progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                pending_encoder(),
                cancel_tx,
                test_policy(),
            )
            .await
        });

        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("supervisor should observe session expiry")
            .unwrap();
        assert_eq!(end.reason, EndReason::Auth);
    }

    #[tokio::test]
    async fn supervisor_stops_on_explicit_shutdown_and_reports_encoder_error() {
        let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::New);
        let (_progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                pending_encoder(),
                cancel_tx,
                test_policy(),
            )
            .await
        });
        let _ = shutdown_tx.send_replace(true);
        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("supervisor should stop on shutdown")
            .unwrap();
        assert_eq!(end.reason, EndReason::Shutdown);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::New);
        let (_progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let encoder: EncoderFuture =
            Box::pin(async { Err(WebRtcError::Internal("fixture encoder failure".to_owned())) });
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                encoder,
                cancel_tx,
                test_policy(),
            )
            .await
        });
        let _ = shutdown_tx;
        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("encoder error should end the supervisor")
            .unwrap();
        assert_eq!(end.reason, EndReason::Encoder);
        assert!(end.encoder.is_some_and(|result| result.is_err()));
    }

    #[tokio::test]
    async fn supervisor_reclaims_connected_session_without_media_progress() {
        let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::Connected);
        let (_progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let policy = LifecyclePolicy {
            media_idle: Duration::from_millis(50),
            ..test_policy()
        };
        let task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                pending_encoder(),
                cancel_tx,
                policy,
            )
            .await
        });
        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("media idle session should be reclaimed")
            .unwrap();
        assert_eq!(end.reason, EndReason::Watchdog(LifecycleExpiry::MediaIdle));
    }

    #[tokio::test]
    async fn supervisor_keeps_receiving_media_progress_alive() {
        let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_state_tx, state_rx) = watch::channel(ConnectionPhase::Connected);
        let (progress_tx, progress_rx) = mpsc::channel(1);
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let policy = LifecyclePolicy {
            media_idle: Duration::from_millis(100),
            ..test_policy()
        };
        let mut task = tokio::spawn(async move {
            supervise_lifecycle(
                &guard,
                shutdown_rx,
                state_rx,
                progress_rx,
                pending_encoder(),
                cancel_tx,
                policy,
            )
            .await
        });
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = progress_tx.try_send(());
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut task)
                .await
                .is_err(),
            "media progress must keep a connected session alive"
        );
        let _ = shutdown_tx.send_replace(true);
        let end = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("supervisor should stop once shutdown is signalled")
            .unwrap();
        assert_eq!(end.reason, EndReason::Shutdown);
    }

    fn test_inner() -> Arc<Inner> {
        let video = VideoManager::new(VideoConfig::default());
        Arc::new(Inner {
            video,
            config: Mutex::new(H264Config::default()),
            encoder: Mutex::new(Err("fixture unavailable".to_owned())),
            sessions: Mutex::new(HashMap::new()),
            offer_lock: AsyncMutex::new(()),
            udp_binding: AsyncMutex::new(None),
            probe_epoch: AtomicU64::new(0),
        })
    }

    fn insert_fake_session(inner: &Arc<Inner>, session_id: &str) {
        let store = crate::auth::SessionStore::default();
        let token = store.create().unwrap();
        let owner = store.validate_guard(&token).unwrap();
        insert_owned_session(inner, session_id, owner, 4_000);
        // Keep the store alive so the owner guard stays active.
        std::mem::forget(store);
    }

    fn insert_owned_session(
        inner: &Arc<Inner>,
        session_id: &str,
        owner: SessionGuard,
        max_bitrate_kbps: u32,
    ) -> watch::Receiver<u32> {
        let (bitrate_tx, bitrate_rx) = watch::channel(max_bitrate_kbps);
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let (state_tx, _state_rx) = watch::channel(ConnectionPhase::New);
        let (closed_tx, closed_rx) = watch::channel(false);
        inner.sessions.lock().unwrap().insert(
            session_id.to_owned(),
            Session {
                shutdown: shutdown_tx,
                bitrate: bitrate_tx,
                max_bitrate_kbps,
                last_bitrate_change: None,
                owner,
                state: state_tx,
                closed: closed_rx,
            },
        );
        let weak = Arc::downgrade(inner);
        let session_id = session_id.to_owned();
        tokio::spawn(async move {
            if !*shutdown_rx.borrow() {
                let _ = shutdown_rx.changed().await;
            }
            if let Some(inner) = weak.upgrade() {
                inner.sessions.lock().unwrap().remove(&session_id);
            }
            let _ = closed_tx.send_replace(true);
        });
        bitrate_rx
    }

    #[tokio::test]
    async fn set_bitrate_clamps_to_floor_and_configured_maximum() {
        let inner = test_inner();
        let store = crate::auth::SessionStore::default();
        let guard = store.validate_guard(&store.create().unwrap()).unwrap();
        let bitrate = insert_owned_session(&inner, "s", guard.clone(), 4_000);
        let start = Instant::now();

        let change = set_session_bitrate(&inner, "s", 10, &guard, start).unwrap();
        assert_eq!(
            change,
            BitrateChange {
                session_id: "s".to_owned(),
                kbps: MIN_ADAPTIVE_BITRATE_KBPS,
                min_kbps: MIN_ADAPTIVE_BITRATE_KBPS,
                max_kbps: 4_000,
                changed: true,
            }
        );
        assert_eq!(*bitrate.borrow(), 256);

        let later = start + BITRATE_CHANGE_INTERVAL;
        let change = set_session_bitrate(&inner, "s", 50_000, &guard, later).unwrap();
        assert_eq!((change.kbps, change.changed), (4_000, true));
        assert_eq!(*bitrate.borrow(), 4_000, "never above the configured bitrate");

        // Re-requesting the running value is a no-op and is not rate-limited.
        let change = set_session_bitrate(&inner, "s", 9_000, &guard, later).unwrap();
        assert_eq!((change.kbps, change.changed), (4_000, false));
    }

    #[tokio::test]
    async fn set_bitrate_is_rate_limited_per_session() {
        let inner = test_inner();
        let store = crate::auth::SessionStore::default();
        let guard = store.validate_guard(&store.create().unwrap()).unwrap();
        let bitrate = insert_owned_session(&inner, "s", guard.clone(), 4_000);
        let start = Instant::now();
        set_session_bitrate(&inner, "s", 2_000, &guard, start).unwrap();
        let error = set_session_bitrate(&inner, "s", 1_000, &guard, start + Duration::from_secs(3))
            .unwrap_err();
        assert!(matches!(error, WebRtcError::RateLimited(7)), "{error:?}");
        assert_eq!(*bitrate.borrow(), 2_000);
        let change =
            set_session_bitrate(&inner, "s", 1_000, &guard, start + BITRATE_CHANGE_INTERVAL)
                .unwrap();
        assert_eq!(change.kbps, 1_000);
        assert_eq!(*bitrate.borrow(), 1_000);
    }

    #[tokio::test]
    async fn set_bitrate_rejects_unknown_foreign_and_revoked_sessions() {
        let inner = test_inner();
        let store = crate::auth::SessionStore::default();
        let owner_token = store.create().unwrap();
        let owner = store.validate_guard(&owner_token).unwrap();
        let other = store.validate_guard(&store.create().unwrap()).unwrap();
        let bitrate = insert_owned_session(&inner, "s", owner.clone(), 4_000);
        let now = Instant::now();

        assert!(matches!(
            set_session_bitrate(&inner, "missing", 1_000, &owner, now),
            Err(WebRtcError::SessionNotFound)
        ));
        // Another login must not learn about, nor tune, this session.
        assert!(matches!(
            set_session_bitrate(&inner, "s", 1_000, &other, now),
            Err(WebRtcError::SessionNotFound)
        ));
        // A second guard of the same login is the same owner.
        let again = store.validate_guard(&owner_token).unwrap();
        assert!(set_session_bitrate(&inner, "s", 1_000, &again, now).unwrap().changed);
        assert!(store.revoke(&owner_token));
        assert!(matches!(
            set_session_bitrate(&inner, "s", 2_000, &owner, now + BITRATE_CHANGE_INTERVAL),
            Err(WebRtcError::Unauthorized)
        ));
        assert_eq!(*bitrate.borrow(), 1_000);
    }

    #[tokio::test]
    async fn set_bitrate_floor_never_exceeds_a_low_configured_bitrate() {
        let inner = test_inner();
        let store = crate::auth::SessionStore::default();
        let guard = store.validate_guard(&store.create().unwrap()).unwrap();
        insert_owned_session(&inner, "s", guard.clone(), 200);
        let change = set_session_bitrate(&inner, "s", 256, &guard, Instant::now()).unwrap();
        assert_eq!((change.kbps, change.min_kbps, change.max_kbps), (200, 200, 200));
    }

    #[tokio::test]
    async fn session_cleanup_removes_the_entry_and_signals_closed_on_panic() {
        let inner = test_inner();
        let (closed_tx, closed_rx) = watch::channel(false);
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);
        let (state_tx, _state_rx) = watch::channel(ConnectionPhase::New);
        let store = crate::auth::SessionStore::default();
        let owner = store.validate_guard(&store.create().unwrap()).unwrap();
        inner.sessions.lock().unwrap().insert(
            "session-panic".to_owned(),
            Session {
                shutdown: shutdown_tx,
                bitrate: watch::channel(4_000).0,
                max_bitrate_kbps: 4_000,
                last_bitrate_change: None,
                owner,
                state: state_tx,
                closed: closed_rx.clone(),
            },
        );
        let cleanup = SessionCleanup {
            inner: Arc::downgrade(&inner),
            session_id: "session-panic".to_owned(),
            peer: None,
            closed: Some(closed_tx),
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _cleanup = cleanup;
            panic!("simulated supervisor panic");
        }));

        assert!(result.is_err());
        assert!(inner.sessions.lock().unwrap().is_empty());
        assert!(*closed_rx.borrow());
    }

    #[tokio::test]
    async fn close_is_idempotent_and_removes_the_session_entry() {
        let inner = test_inner();
        insert_fake_session(&inner, "session-a");
        assert!(close_session(&inner, "session-a").await);
        assert!(inner.sessions.lock().unwrap().is_empty());
        assert!(!close_session(&inner, "session-a").await);
    }

    #[tokio::test]
    async fn reconfigure_closes_sessions_and_is_idempotent() {
        let inner = test_inner();
        insert_fake_session(&inner, "session-a");
        insert_fake_session(&inner, "session-b");
        let manager = WebRtcManager {
            inner: inner.clone(),
        };
        let config = H264Config {
            // A missing executable fails the bounded probe immediately without
            // depending on a real FFmpeg install.
            ffmpeg_path: Some(PathBuf::from("/nonexistent-wingmankvm-test-ffmpeg")),
            ..H264Config::default()
        };
        manager.reconfigure(config.clone()).await;
        assert!(inner.sessions.lock().unwrap().is_empty());
        manager.reconfigure(config).await;
        assert!(inner.sessions.lock().unwrap().is_empty());
    }

    #[test]
    fn probe_limits_and_policy_defaults_are_bounded() {
        let limits = ProbeLimits::default();
        assert_eq!(limits.deadline, Duration::from_secs(3));
        assert_eq!(limits.max_output, 1024 * 1024);
        let policy = LifecyclePolicy::default();
        assert_eq!(policy.unconnected, Duration::from_secs(30));
        assert_eq!(policy.disconnected, Duration::from_secs(15));
        assert_eq!(policy.media_idle, Duration::from_secs(60));
    }

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    struct ProbeFixture {
        directory: PathBuf,
    }

    #[cfg(unix)]
    impl ProbeFixture {
        fn new() -> Self {
            let mut random = [0_u8; 8];
            getrandom::fill(&mut random).unwrap();
            let directory = std::env::temp_dir().join(format!(
                "wingmankvm-probe-test-{}-{:016x}",
                std::process::id(),
                u64::from_ne_bytes(random)
            ));
            fs::create_dir(&directory).unwrap();
            Self { directory }
        }

        fn script(&self, name: &str, body: &str) -> PathBuf {
            let path = self.directory.join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        fn path(&self, name: &str) -> PathBuf {
            self.directory.join(name)
        }

        fn config(&self, script: &Path) -> H264Config {
            H264Config {
                ffmpeg_path: Some(script.to_path_buf()),
                encoder: H264Encoder::Software,
                allow_software: true,
                ..H264Config::default()
            }
        }
    }

    #[cfg(unix)]
    impl Drop for ProbeFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[cfg(unix)]
    fn shell_quote(value: &Path) -> String {
        format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
    }

    #[cfg(unix)]
    fn read_fixture_pid(path: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Ok(text) = fs::read_to_string(path)
                && let Some(pid) = text
                    .lines()
                    .find_map(|line| line.trim().parse::<u32>().ok())
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "fixture did not write its pid");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    fn process_exists(pid: u32) -> bool {
        let result = unsafe { libc::kill(pid as i32, 0) };
        result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(unix)]
    #[test]
    fn probe_accepts_a_normal_encoder_listing() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let script = fixture.script("ffmpeg-ok", "printf 'V....D libx264\\n'");
        let config = fixture.config(&script);
        let encoder = probe_encoder_with(&config, ProbeLimits::default()).unwrap();
        assert_eq!(encoder.codec, "libx264");
        assert_eq!(encoder.label, "libx264");
        assert!(encoder.software);
    }

    #[cfg(unix)]
    #[test]
    fn probe_nonzero_exit_is_generic_and_does_not_leak_output() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let script = fixture.script("ffmpeg-fail", "printf 'SECRET-PROBE-OUTPUT\\n' >&2\nexit 7");
        let config = fixture.config(&script);
        let error = probe_encoder_with(&config, ProbeLimits::default()).unwrap_err();
        assert!(error.contains("无法列出编码器"));
        assert!(!error.contains("SECRET-PROBE-OUTPUT"));
    }

    #[cfg(unix)]
    #[test]
    fn probe_hang_is_killed_and_reaped_within_the_deadline() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let pidfile = fixture.path("hang.pid");
        let script = fixture.script(
            "ffmpeg-hang",
            &format!(
                "echo $$ > {}\nsleep 3\nprintf 'V....D libx264\\n'",
                shell_quote(&pidfile)
            ),
        );
        let config = fixture.config(&script);
        let limits = ProbeLimits {
            deadline: Duration::from_secs(1),
            max_output: 1024,
        };
        let started = Instant::now();
        let error = probe_encoder_with(&config, limits).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(error.contains("超时"));
        let pid = read_fixture_pid(&pidfile);
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!process_exists(pid), "hung probe child must be reaped");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn new_returns_immediately_while_encoder_probe_is_suspended() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let script = fixture.script("ffmpeg-slow", "sleep 1\nprintf 'V....D libx264\\n'");
        let config = fixture.config(&script);

        let started = Instant::now();
        let manager = WebRtcManager::new(VideoManager::new(VideoConfig::default()), config);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "constructor must not wait for the encoder probe"
        );
        let probing = manager.status();
        assert!(!probing.available);
        assert!(
            probing
                .message
                .as_deref()
                .is_some_and(|message| message.contains("探测"))
        );

        let deadline = Instant::now() + Duration::from_secs(3);
        while !manager.status().available {
            assert!(
                Instant::now() < deadline,
                "background probe should finish and publish the encoder"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_stdout_flood_is_bounded() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let script = fixture.script(
            "ffmpeg-stdout-flood",
            "i=0\nwhile [ $i -lt 64 ]; do printf '%1024s' ''; i=$((i+1)); done",
        );
        let config = fixture.config(&script);
        let limits = ProbeLimits {
            deadline: Duration::from_secs(2),
            max_output: 4096,
        };
        let error = probe_encoder_with(&config, limits).unwrap_err();
        assert!(error.contains("输出超过限制"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn probe_stderr_flood_is_bounded() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let script = fixture.script(
            "ffmpeg-stderr-flood",
            "i=0\nwhile [ $i -lt 64 ]; do printf '%1024s' '' >&2; i=$((i+1)); done",
        );
        let config = fixture.config(&script);
        let limits = ProbeLimits {
            deadline: Duration::from_secs(2),
            max_output: 4096,
        };
        let error = probe_encoder_with(&config, limits).unwrap_err();
        assert!(error.contains("输出超过限制"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn probe_kills_a_descendant_that_holds_the_pipes() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let child_pidfile = fixture.path("descendant.pid");
        let script = fixture.script(
            "ffmpeg-pipe-holder",
            &format!(
                "sleep 3 &\necho $! > {}\nprintf 'V....D libx264\\n'\nexit 0",
                shell_quote(&child_pidfile)
            ),
        );
        let config = fixture.config(&script);
        let limits = ProbeLimits {
            deadline: Duration::from_secs(1),
            max_output: 1024 * 1024,
        };
        let error = probe_encoder_with(&config, limits).unwrap_err();
        assert!(error.contains("超时"), "{error}");
        let pid = read_fixture_pid(&child_pidfile);
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !process_exists(pid),
            "pipe-holding descendant must be killed with the group"
        );
    }

    #[cfg(unix)]
    fn encoder_group_script(fixture: &ProbeFixture, leader: &Path, child: &Path) -> PathBuf {
        fixture.script(
            "encoder-group",
            &format!(
                "echo $$ > {}\nsleep 30 &\necho $! > {}\nwait",
                shell_quote(leader),
                shell_quote(child)
            ),
        )
    }

    #[cfg(unix)]
    fn test_encoder_spec(script: &Path) -> EncoderSpec {
        EncoderSpec {
            executable: script.to_path_buf(),
            codec: "libx264",
            label: "test encoder",
            software: true,
        }
    }

    #[cfg(unix)]
    fn test_encoder_track() -> Arc<TrackLocalStaticSample> {
        Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90_000,
                ..Default::default()
            },
            "video".to_owned(),
            "test".to_owned(),
        ))
    }

    #[cfg(unix)]
    async fn wait_for_fixture_pid(path: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(text) = fs::read_to_string(path)
                && let Some(pid) = text
                    .lines()
                    .find_map(|line| line.trim().parse::<u32>().ok())
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "fixture did not write its pid");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(unix)]
    async fn wait_for_fixture_exit(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(pid) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !process_exists(pid),
            "encoder process group member survived"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn encoder_cancel_kills_private_group_and_stops_tasks() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let leader_pidfile = fixture.path("encoder-leader.pid");
        let child_pidfile = fixture.path("encoder-child.pid");
        let script = encoder_group_script(&fixture, &leader_pidfile, &child_pidfile);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (progress_tx, _progress_rx) = mpsc::channel(1);
        let task = tokio::spawn(run_encoder(
            VideoManager::new(VideoConfig::default()),
            test_encoder_track(),
            test_encoder_spec(&script),
            cancel_rx,
            watch::channel(H264Config::default().bitrate_kbps).1,
            progress_tx,
        ));
        let leader = wait_for_fixture_pid(&leader_pidfile).await;
        let child = wait_for_fixture_pid(&child_pidfile).await;
        assert_ne!(leader, child);
        let _ = cancel_tx.send_replace(true);
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("cancelled encoder should stop")
            .expect("encoder task should not panic");
        assert!(result.is_ok(), "{result:?}");
        wait_for_fixture_exit(leader).await;
        wait_for_fixture_exit(child).await;
    }

    #[cfg(unix)]
    async fn wait_for_fixture_lines(path: &Path, count: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let lines = fs::read_to_string(path)
                .map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
                .unwrap_or_default();
            if lines.len() >= count {
                return lines;
            }
            assert!(Instant::now() < deadline, "fixture wrote {} of {count} lines", lines.len());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    // The std guard only serialises fixture-spawning tests; it is never
    // contended inside this single-threaded runtime.
    #[allow(clippy::await_holding_lock)]
    async fn bitrate_change_restarts_only_the_encoder_process() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let runs = fixture.path("encoder-runs.log");
        let script = fixture.script(
            "encoder-bitrate",
            &format!(
                "echo \"$$ $*\" >> {}\nexec sleep 30",
                shell_quote(&runs)
            ),
        );
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (bitrate_tx, bitrate_rx) = watch::channel(4_000_u32);
        let (progress_tx, _progress_rx) = mpsc::channel(1);
        let task = tokio::spawn(run_encoder(
            VideoManager::new(VideoConfig::default()),
            test_encoder_track(),
            test_encoder_spec(&script),
            cancel_rx,
            bitrate_rx,
            progress_tx,
        ));
        let first = wait_for_fixture_lines(&runs, 1).await;
        assert!(first[0].contains("-b:v 4000k -maxrate 4000k -bufsize 8000k"), "{}", first[0]);
        let first_pid: u32 = first[0].split_whitespace().next().unwrap().parse().unwrap();

        // Re-sending the running value must not restart FFmpeg.
        bitrate_tx.send_replace(4_000);
        let _ = bitrate_tx.send_replace(1_500);
        let lines = wait_for_fixture_lines(&runs, 2).await;
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[1].contains("-b:v 1500k -maxrate 1500k -bufsize 3000k"), "{}", lines[1]);
        wait_for_fixture_exit(first_pid).await;
        assert!(!task.is_finished(), "the session encoder must keep running");

        let second_pid: u32 = lines[1].split_whitespace().next().unwrap().parse().unwrap();
        let _ = cancel_tx.send_replace(true);
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("cancelled encoder should stop")
            .expect("encoder task should not panic");
        assert!(result.is_ok(), "{result:?}");
        wait_for_fixture_exit(second_pid).await;
        assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn encoder_drop_kills_private_group_and_stops_tasks() {
        let _serial = serial_probe_fixture();
        let fixture = ProbeFixture::new();
        let leader_pidfile = fixture.path("encoder-drop-leader.pid");
        let child_pidfile = fixture.path("encoder-drop-child.pid");
        let script = encoder_group_script(&fixture, &leader_pidfile, &child_pidfile);
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let (progress_tx, _progress_rx) = mpsc::channel(1);
        let task = tokio::spawn(run_encoder(
            VideoManager::new(VideoConfig::default()),
            test_encoder_track(),
            test_encoder_spec(&script),
            cancel_rx,
            watch::channel(H264Config::default().bitrate_kbps).1,
            progress_tx,
        ));
        let leader = wait_for_fixture_pid(&leader_pidfile).await;
        let child = wait_for_fixture_pid(&child_pidfile).await;
        assert_ne!(leader, child);
        // Give the encoder future time to reach its select loop; the child is
        // already owned by the RAII guard at this point.
        tokio::time::sleep(Duration::from_millis(100)).await;
        task.abort();
        let _ = task.await;
        wait_for_fixture_exit(leader).await;
        wait_for_fixture_exit(child).await;
    }
    /// Real loopback ICE/DTLS/SRTP with two viewers sharing one server UDP
    /// port. The synthetic NAL bytes test transport, not H.264 decoding.
    #[cfg(unix)]
    #[tokio::test]
    async fn two_loopback_viewers_share_udp_and_end_on_logout() {
        let fixture = ProbeFixture::new();
        let script = fixture.script("loopback-encoder", r"while :; do printf '\000\000\000\001\011\360\000\000\000\001\145\210\204'; sleep 0.05; done");
        let (video, frames) = VideoManager::test_source(VideoConfig::default());
        frames.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
            jpeg: Bytes::from_static(&[0xff, 0xd8, 0xff, 0xd9]),
            sequence: 1,
        })));
        let reserved = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = reserved.local_addr().unwrap();
        drop(reserved);
        let manager = WebRtcManager {
            inner: Arc::new(Inner {
                video,
                config: Mutex::new(H264Config {
                    udp_bind_address: Some(address),
                    max_sessions: 2,
                    ..H264Config::default()
                }),
                encoder: Mutex::new(Ok(test_encoder_spec(&script))),
                sessions: Mutex::new(HashMap::new()),
                offer_lock: AsyncMutex::new(()),
                udp_binding: AsyncMutex::new(None),
                probe_epoch: AtomicU64::new(0),
            }),
        };
        let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
        let token = sessions.create().unwrap();
        let mut clients = Vec::new();
        for _ in 0..2 {
            let mut media = MediaEngine::default();
            media.register_default_codecs().unwrap();
            let mut settings = SettingEngine::default();
            settings.set_network_types(vec![NetworkType::Udp4]);
            let client = APIBuilder::new()
                .with_media_engine(media)
                .with_setting_engine(settings)
                .build()
                .new_peer_connection(RTCConfiguration::default())
                .await
                .unwrap();
            client
                .add_transceiver_from_kind(
                    webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video,
                    None,
                )
                .await
                .unwrap();
            let (packet_tx, mut packet_rx) = mpsc::channel(1);
            client.on_track(Box::new(move |track, _, _| {
                let tx = packet_tx.clone();
                Box::pin(async move {
                    if let Ok((packet, _)) = track.read_rtp().await {
                        let _ = tx.send(packet.payload).await;
                    }
                })
            }));
            let mut gathered = client.gathering_complete_promise().await;
            let offer = client.create_offer(None).await.unwrap();
            client.set_local_description(offer).await.unwrap();
            tokio::time::timeout(Duration::from_secs(6), gathered.recv())
                .await
                .unwrap();
            let offer = client.local_description().await.unwrap();
            let answer = manager
                .offer(
                    WebRtcOfferRequest {
                        sdp_type: "offer".into(),
                        sdp: offer.sdp,
                    },
                    sessions.validate_guard(&token).unwrap(),
                )
                .await
                .unwrap();
            assert!(
                answer
                    .sdp
                    .contains(&format!(" {} typ host", address.port()))
            );
            client
                .set_remote_description(RTCSessionDescription::answer(answer.sdp).unwrap())
                .await
                .unwrap();
            let packet = tokio::time::timeout(Duration::from_secs(10), packet_rx.recv())
                .await
                .expect("loopback SRTP packet")
                .unwrap();
            assert!(!packet.is_empty());
            clients.push(client);
        }
        assert_eq!(manager.status().sessions, 2);
        assert!(sessions.revoke(&token));
        tokio::time::timeout(Duration::from_secs(5), async {
            while manager.status().sessions != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        for client in clients {
            client.close().await.unwrap();
        }
    }
}
