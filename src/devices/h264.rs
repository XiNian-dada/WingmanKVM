//! One on-demand encoder shared by WebRTC and Direct H.264 viewers.
//! Each viewer has a bounded queue and resumes at an IDR after falling behind.
use super::*;
use tokio::sync::broadcast;

const FRAME_QUEUE: usize = 8;
pub const MAX_ENCODED_FRAME: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize)]
pub struct EncoderTelemetry {
    pub frames: u64,
    pub epoch: u32,
    pub restarts: u32,
    pub bitrate_changes: u64,
    pub bitrate_kbps: u32,
    pub fps: Option<f64>,
    pub processing_ms: Option<f64>,
    pub last_frame_age_ms: Option<u64>,
    pub max_frame_gap_ms: Option<f64>,
}
pub(super) struct EncoderMetrics {
    value: EncoderTelemetry,
    window: Instant,
    window_frames: u64,
    last_frame: Option<Instant>,
}
impl Default for EncoderMetrics {
    fn default() -> Self { Self { value: EncoderTelemetry::default(), window: Instant::now(), window_frames: 0, last_frame: None } }
}
impl EncoderMetrics {
    fn record(&mut self, epoch: u32, kbps: u32, processing_ms: Option<f64>) {
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            let gap = now.duration_since(last).as_secs_f64() * 1000.0;
            self.value.max_frame_gap_ms = Some(self.value.max_frame_gap_ms.unwrap_or(0.0).max(gap));
        }
        if self.value.frames > 0 && self.value.bitrate_kbps != kbps { self.value.bitrate_changes += 1; }
        self.last_frame = Some(now);
        self.value.frames += 1;
        self.value.epoch = epoch;
        self.value.restarts = epoch.saturating_sub(1);
        self.value.bitrate_kbps = kbps;
        if let Some(ms) = processing_ms {
            self.value.processing_ms = Some(self.value.processing_ms.map_or(ms, |before| before * 0.9 + ms * 0.1));
        }
        self.window_frames += 1;
        let seconds = now.duration_since(self.window).as_secs_f64();
        if seconds >= 1.0 {
            self.value.fps = Some(self.window_frames as f64 / seconds);
            self.window = now; self.window_frames = 0;
        }
    }
    fn snapshot(&self) -> EncoderTelemetry {
        let mut value = self.value.clone();
        value.last_frame_age_ms = self.last_frame.map(|t| t.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        if self.last_frame.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) { value.fps = Some(0.0); }
        value
    }
}

#[derive(Clone, Debug)]
pub struct EncodedFrame {
    pub data: Bytes,
    pub sequence: u64,
    pub timestamp_us: u64,
    pub duration_us: u32,
    pub epoch: u32,
    pub key: bool,
    pub codec: Option<String>,
    pub bitrate_kbps: u32,
}

#[derive(Clone, Default)]
pub(super) struct H264Hub(Arc<Mutex<Option<HubRun>>>);
struct HubRun {
    id: u64,
    next_viewer: u64,
    viewers: watch::Sender<HashMap<u64, u32>>,
    frames: broadcast::Sender<Arc<EncodedFrame>>,
    cancel: watch::Sender<bool>,
    done: watch::Receiver<Option<String>>,
    metrics: Arc<Mutex<EncoderMetrics>>,
}
impl Drop for HubRun {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}

pub struct H264Subscription {
    hub: H264Hub,
    run: u64,
    viewer: u64,
    receiver: broadcast::Receiver<Arc<EncodedFrame>>,
    done: watch::Receiver<Option<String>>,
    needs_key: bool,
    max_kbps: u32,
}
impl H264Subscription {
    pub async fn next(&mut self) -> Result<Arc<EncodedFrame>, WebRtcError> {
        loop {
            if let Some(error) = self.done.borrow().clone() {
                return Err(WebRtcError::Unavailable(error));
            }
            tokio::select! {
                biased;
                _ = self.done.changed() => return Err(WebRtcError::Unavailable("H.264 编码已停止".into())),
                result = self.receiver.recv() => match result {
                    Ok(frame) => {
                        if self.needs_key && !frame.key { continue; }
                        self.needs_key = false;
                        return Ok(frame);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => self.needs_key = true,
                    Err(_) => return Err(WebRtcError::Unavailable("H.264 编码已停止".into())),
                }
            }
        }
    }
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<u32, WebRtcError> {
        let target = kbps.clamp(MIN_ADAPTIVE_BITRATE_KBPS.min(self.max_kbps), self.max_kbps);
        let mut lock = self.hub.0.lock().unwrap_or_else(|e| e.into_inner());
        let run = lock
            .as_mut()
            .filter(|run| run.id == self.run)
            .ok_or(WebRtcError::SessionNotFound)?;
        let current = run
            .viewers
            .borrow()
            .get(&self.viewer)
            .copied()
            .ok_or(WebRtcError::SessionNotFound)?;
        if current == target {
            return Ok(target);
        }
        run.viewers.send_modify(|viewers| {
            viewers.insert(self.viewer, target);
        });
        Ok(target)
    }
}
impl Drop for H264Subscription {
    fn drop(&mut self) {
        let mut lock = self.hub.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = lock.as_mut().filter(|run| run.id == self.run) {
            run.viewers.send_modify(|viewers| {
                viewers.remove(&self.viewer);
            });
        }
    }
}
impl H264Hub {
    pub fn telemetry(&self) -> Option<EncoderTelemetry> {
        let metrics = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.metrics.clone();
        let snapshot = metrics.lock().unwrap_or_else(|e| e.into_inner()).snapshot();
        Some(snapshot)
    }
    pub fn viewers(&self) -> usize {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(0, |r| r.viewers.borrow().len())
    }
    pub async fn stop(&self) {
        let run = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut run) = run {
            run.cancel.send_replace(true);
            if run.done.borrow().is_none() {
                let _ = tokio::time::timeout(
                    ENCODER_CLEANUP_TIMEOUT + Duration::from_secs(1),
                    run.done.changed(),
                )
                .await;
            }
        }
    }
    pub async fn subscribe(
        &self,
        video: VideoManager,
        encoder: EncoderSpec,
        config: &H264Config,
    ) -> Result<H264Subscription, WebRtcError> {
        loop {
            let mut waiting = {
                let mut lock = self.0.lock().unwrap_or_else(|e| e.into_inner());
                if lock.as_ref().is_some_and(|r| r.done.borrow().is_some()) {
                    *lock = None;
                }
                if lock.is_none() {
                    static NEXT_RUN: AtomicU64 = AtomicU64::new(1);
                    let id = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
                    let hub = Arc::downgrade(&self.0);
                    let (frames, _) = broadcast::channel(FRAME_QUEUE);
                    let (viewers, requests) = watch::channel(HashMap::new());
                    let (cancel, cancellation) = watch::channel(false);
                    let (finished, done) = watch::channel(None);
                    let metrics = Arc::new(Mutex::new(EncoderMetrics::default()));
                    let output = EncoderOutput::Shared {
                        frames: frames.clone(),
                        sequence: Arc::new(AtomicU64::new(0)),
                        timestamp: Arc::new(AtomicU64::new(0)),
                        epoch: Arc::new(AtomicU64::new(0)),
                        started: Instant::now(),
                        metrics: metrics.clone(),
                    };
                    let video = video.clone();
                    let encoder = encoder.clone();
                    tokio::spawn(async move {
                        let result =
                            drive_encoder(video, encoder, output, requests, cancellation, hub, id)
                                .await;
                        finished.send_replace(Some(
                            result
                                .err()
                                .map_or_else(|| "H.264 编码已停止".into(), |e| e.to_string()),
                        ));
                    });
                    *lock = Some(HubRun {
                        id,
                        next_viewer: 0,
                        viewers,
                        frames,
                        cancel,
                        done,
                        metrics,
                    });
                }
                let run = lock.as_mut().unwrap();
                if *run.cancel.borrow() {
                    run.done.clone()
                } else {
                    if run.viewers.borrow().len() >= usize::from(config.max_sessions) {
                        return Err(WebRtcError::Capacity);
                    }
                    run.next_viewer += 1;
                    let viewer = run.next_viewer;
                    let receiver = run.frames.subscribe();
                    run.viewers.send_modify(|viewers| {
                        viewers.insert(viewer, config.bitrate_kbps);
                    });
                    return Ok(H264Subscription {
                        hub: self.clone(),
                        run: run.id,
                        viewer,
                        receiver,
                        done: run.done.clone(),
                        needs_key: true,
                        max_kbps: config.bitrate_kbps,
                    });
                }
            };
            // Never attach a viewer to a run whose idle shutdown has started.
            tokio::time::timeout(
                ENCODER_CLEANUP_TIMEOUT + Duration::from_secs(1),
                waiting.changed(),
            )
            .await
            .map_err(|_| WebRtcError::Unavailable("H.264 编码器停止超时".into()))?
            .map_err(|_| WebRtcError::Unavailable("H.264 编码器已停止".into()))?;
        }
    }
}

async fn drive_encoder(
    video: VideoManager,
    encoder: EncoderSpec,
    output: EncoderOutput,
    mut viewers: watch::Receiver<HashMap<u64, u32>>,
    mut cancel: watch::Receiver<bool>,
    hub: Weak<Mutex<Option<HubRun>>>,
    id: u64,
) -> Result<(), WebRtcError> {
    while viewers.borrow().is_empty() {
        tokio::select! {
            _ = wait_for_cancel(&mut cancel) => return Ok(()),
            changed = viewers.changed() => if changed.is_err() { return Ok(()); },
        }
    }
    let initial = viewers
        .borrow_and_update()
        .values()
        .copied()
        .min()
        .unwrap_or(4000);
    let (bitrate, requests) = watch::channel(initial);
    let (stop, cancellation) = watch::channel(false);
    let (progress, mut pulses) = mpsc::channel(1);
    let future = run_encoder(video, output, encoder, cancellation, requests, progress);
    tokio::pin!(future);
    let mut idle_since = None;
    loop {
        let empty = viewers.borrow().is_empty();
        if empty {
            idle_since.get_or_insert(Instant::now());
        } else {
            idle_since = None;
        }
        let idle_remaining = idle_since.map_or(Duration::from_secs(1), |since| {
            Duration::from_secs(1).saturating_sub(since.elapsed())
        });
        tokio::select! {
            biased;
            _ = wait_for_cancel(&mut cancel) => break,
            result = &mut future => return result,
            _ = tokio::time::sleep(idle_remaining), if empty => {
                let Some(hub) = hub.upgrade() else { break; };
                let lock = hub.lock().unwrap_or_else(|e| e.into_inner());
                let Some(run) = lock.as_ref().filter(|run| run.id == id) else { break; };
                if run.viewers.borrow().is_empty() {
                    run.cancel.send_replace(true);
                    break;
                }
            },
            changed = viewers.changed() => {
                if changed.is_err() { break; }
                if let Some(target) = viewers.borrow_and_update().values().copied().min() { bitrate.send_replace(target); }
            }
            _ = pulses.recv() => {},
        }
    }
    stop.send_replace(true);
    future.await
}

#[derive(Clone)]
pub(super) enum EncoderOutput {
    Shared {
        frames: broadcast::Sender<Arc<EncodedFrame>>,
        sequence: Arc<AtomicU64>,
        timestamp: Arc<AtomicU64>,
        epoch: Arc<AtomicU64>,
        started: Instant,
        metrics: Arc<Mutex<EncoderMetrics>>,
    },
    #[cfg(test)]
    Track(Arc<TrackLocalStaticSample>),
}
#[cfg(test)]
impl From<Arc<TrackLocalStaticSample>> for EncoderOutput {
    fn from(track: Arc<TrackLocalStaticSample>) -> Self {
        Self::Track(track)
    }
}

pub(super) struct FrameWriter {
    output: EncoderOutput,
    sps: Option<Bytes>,
    pps: Option<Bytes>,
    codec: Option<String>,
    epoch: u32,
    kbps: u32,
    processing_ms: Option<f64>,
}
impl FrameWriter {
    pub fn set_bitrate(&mut self, kbps: u32) { self.kbps = kbps; }
    pub fn set_processing_time(&mut self, us: u32) { self.processing_ms = Some(f64::from(us) / 1000.0); }
    pub fn new(output: EncoderOutput, kbps: u32) -> Self {
        let epoch = match &output {
            EncoderOutput::Shared { epoch, .. } => epoch.fetch_add(1, Ordering::Relaxed) as u32 + 1,
            #[cfg(test)]
            EncoderOutput::Track(_) => 0,
        };
        Self {
            output,
            sps: None,
            pps: None,
            codec: None,
            epoch,
            kbps,
            processing_ms: None,
        }
    }
    /// Returns whether a picture was published. Header-only units may update
    /// parameter sets, but must not advance RTP time or occupy a viewer's queue.
    pub async fn write(&mut self, mut data: Bytes, duration: Duration) -> Result<bool, String> {
        if data.len() > MAX_ENCODED_FRAME {
            return Err("H.264 单帧超过缓冲上限".into());
        }
        let units = annex_b_nal_units(&data);
        let mut key = false;
        let mut has_picture = false;
        let (mut has_sps, mut has_pps) = (false, false);
        for (i, &(start, payload)) in units.iter().enumerate() {
            let end = units.get(i + 1).map_or(data.len(), |v| v.0);
            match data.get(payload).map(|b| b & 0x1f) {
                Some(kind @ 1..=5) => {
                    has_picture = true;
                    key |= kind == 5;
                }
                Some(7) if end >= payload + 4 => {
                    self.codec = Some(format!(
                        "avc1.{:02x}{:02x}{:02x}",
                        data[payload + 1],
                        data[payload + 2],
                        data[payload + 3]
                    ));
                    self.sps = Some(data.slice(start..end));
                    has_sps = true;
                }
                Some(8) => {
                    self.pps = Some(data.slice(start..end));
                    has_pps = true;
                }
                _ => {}
            }
        }
        if !has_picture {
            return Ok(false);
        }
        if key && (!has_sps || !has_pps) {
            let mut prefix = Vec::new();
            if !has_sps {
                if let Some(sps) = &self.sps {
                    prefix.extend_from_slice(sps);
                }
            }
            if !has_pps {
                if let Some(pps) = &self.pps {
                    prefix.extend_from_slice(pps);
                }
            }
            prefix.extend_from_slice(&data);
            data = Bytes::from(prefix);
        }
        #[cfg(test)]
        if let EncoderOutput::Track(track) = &self.output {
            return write_access_unit(track, data, duration).await.map(|()| true);
        }
        match &self.output {
            EncoderOutput::Shared {
                frames,
                sequence,
                timestamp,
                started,
                metrics,
                ..
            } => {
                if data.len() > MAX_ENCODED_FRAME {
                    return Err("H.264 单帧超过缓冲上限".into());
                }
                let now = started.elapsed().as_micros().min(u128::from(u64::MAX - 1)) as u64;
                // Encoder restarts finish the old writer before starting another.
                let timestamp_us = now.max(timestamp.load(Ordering::Relaxed).saturating_add(1));
                timestamp.store(timestamp_us, Ordering::Relaxed);
                metrics.lock().unwrap_or_else(|e| e.into_inner()).record(self.epoch, self.kbps, self.processing_ms);
                let _ = frames.send(Arc::new(EncodedFrame {
                    data,
                    sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
                    timestamp_us,
                    duration_us: duration.as_micros() as u32,
                    epoch: self.epoch,
                    key,
                    codec: self.pps.as_ref().and(self.codec.clone()),
                    bitrate_kbps: self.kbps,
                }));
            }
            #[cfg(test)]
            EncoderOutput::Track(_) => unreachable!(),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &[u8] = &[
        0, 0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1e, 0, 0, 0, 1, 0x68, 0xce, 0, 0, 0,
        1, 0x65, 1,
    ];
    const DELTA: &[u8] = &[0, 0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x41, 1];

    #[cfg(unix)]
    #[tokio::test]
    async fn native_bitrate_changes_keep_one_epoch_and_publish_every_input_frame() {
        let (video, source) = VideoManager::test_source(VideoConfig::default());
        let image = image::RgbImage::from_fn(128, 72, |x, y| image::Rgb([x as u8, y as u8, 100]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&image).unwrap();
        let hub = H264Hub::default();
        let encoder = native_h264::prepare(4000).unwrap();
        let config = H264Config { max_sessions: 2, ..H264Config::default() };
        let mut viewer = hub.subscribe(video, encoder, &config).await.unwrap();
        let mut codec = None;
        for (i, kbps) in [256, 4000, 1000, 3000, 700, 2000, 4000].into_iter().enumerate() {
            viewer.set_bitrate(kbps).unwrap();
            source.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
                jpeg: Bytes::copy_from_slice(&jpeg), sequence: i as u64 + 1,
            })));
            let frame = tokio::time::timeout(Duration::from_secs(3), viewer.next()).await.unwrap().unwrap();
            assert_eq!(frame.sequence, i as u64 + 1);
            assert_eq!(frame.epoch, 1, "live bitrate changes must not restart or reset the encoder");
            assert_eq!(frame.bitrate_kbps, kbps);
            assert!(!frame.data.is_empty());
            if i == 0 { codec = frame.codec.clone(); }
            assert_eq!(frame.codec, codec, "a low starting bitrate must not change SPS level on a raise");
            if i == 0 { assert!(frame.key && frame.codec.is_some()); }
        }
        let stats = hub.telemetry().unwrap();
        assert_eq!((stats.frames, stats.restarts, stats.bitrate_changes), (7, 0, 6));
        assert_eq!(stats.bitrate_kbps, 4000);
        assert!(stats.processing_ms.is_some_and(|ms| ms >= 0.0));
        assert!(stats.last_frame_age_ms.is_some());
        drop(viewer);
        hub.stop().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_encoder_rejects_corrupt_jpeg_without_hanging_the_hub() {
        let (video, source) = VideoManager::test_source(VideoConfig::default());
        source.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
            jpeg: Bytes::from_static(&[255, 216, 255, 217]), sequence: 1,
        })));
        let hub = H264Hub::default();
        let mut viewer = hub.subscribe(video, native_h264::prepare(4000).unwrap(), &H264Config::default()).await.unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(3), viewer.next()).await.unwrap().is_err());
        drop(viewer);
        hub.stop().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_raw_jpeg_decode_handles_gray_color_and_partial_mcu_edges() {
        for gray in [false, true] {
            let mut jpeg = Vec::new();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
            if gray {
                encoder.encode_image(&image::GrayImage::from_fn(130, 74, |x, y| image::Luma([(x+y) as u8]))).unwrap();
            } else {
                encoder.encode_image(&image::RgbImage::from_fn(130, 74, |x, y| image::Rgb([x as u8, y as u8, 100]))).unwrap();
            }
            let (video, source) = VideoManager::test_source(VideoConfig::default());
            source.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
                jpeg: Bytes::copy_from_slice(&jpeg), sequence: 1,
            })));
            let hub = H264Hub::default();
            let mut viewer = hub.subscribe(video, native_h264::prepare(4000).unwrap(), &H264Config::default()).await.unwrap();
            let frame = tokio::time::timeout(Duration::from_secs(3), viewer.next()).await.unwrap().unwrap();
            assert!(frame.key && frame.codec.is_some());
            drop(viewer);
            hub.stop().await;
        }
    }

    #[tokio::test]
    async fn consecutive_delimiters_publish_only_pictures_and_keep_parameter_sets() {
        let (frames, mut receiver) = broadcast::channel(FRAME_QUEUE);
        let sequence = Arc::new(AtomicU64::new(0));
        let timestamp = Arc::new(AtomicU64::new(0));
        let mut writer = FrameWriter::new(
            EncoderOutput::Shared {
                frames,
                sequence: sequence.clone(),
                timestamp: timestamp.clone(),
                epoch: Arc::new(AtomicU64::new(0)),
                started: Instant::now(),
            metrics: Arc::new(Mutex::new(EncoderMetrics::default())),
            },
            4000,
        );
        let duration = Duration::from_micros(33_333);
        let headers = &KEY[..KEY.len() - 6];
        assert!(!writer.write(Bytes::copy_from_slice(headers), duration).await.unwrap());
        assert_eq!(sequence.load(Ordering::Relaxed), 0);
        assert_eq!(timestamp.load(Ordering::Relaxed), 0);
        assert!(matches!(receiver.try_recv(), Err(broadcast::error::TryRecvError::Empty)));

        let aud = &[0, 0, 0, 1, 9, 0xf0];
        let key = &[0, 0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x65, 1];
        let stream = [aud.as_slice(), key.as_slice(), aud.as_slice(), DELTA, aud.as_slice()].concat();
        let mut parser = AnnexBAccessUnitParser::default();
        // Delimiters and NAL headers may be split across arbitrary pipe reads.
        let mut published = 0;
        for chunk in stream.chunks(3) {
            for unit in parser.push(chunk).unwrap() {
                published += usize::from(writer.write(unit, duration).await.unwrap());
            }
        }
        if let Some(unit) = parser.finish() {
            published += usize::from(writer.write(unit, duration).await.unwrap());
        }
        assert_eq!(published, 2);
        assert_eq!(sequence.load(Ordering::Relaxed), 2);
        let first = receiver.try_recv().unwrap();
        let second = receiver.try_recv().unwrap();
        assert_eq!((first.sequence, second.sequence), (1, 2));
        assert!(first.key);
        assert!(!second.key);
        assert_eq!(first.codec.as_deref(), Some("avc1.42c01e"));
        for kind in [7, 8] {
            assert!(annex_b_nal_units(&first.data).iter().any(|&(_, p)| first.data[p] & 31 == kind));
        }
        assert_eq!((first.duration_us, second.duration_us), (33_333, 33_333));
        assert!(matches!(receiver.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn shared_frames_keep_parameter_sets_and_recover_lag_at_an_idr() {
        let (frames, _) = broadcast::channel(FRAME_QUEUE);
        let mut fast = frames.subscribe();
        let (finished, done) = watch::channel(None);
        let mut slow = H264Subscription {
            hub: H264Hub::default(),
            run: 0,
            viewer: 0,
            receiver: frames.subscribe(),
            done,
            needs_key: true,
            max_kbps: 4000,
        };
        let output = EncoderOutput::Shared {
            frames,
            sequence: Arc::new(AtomicU64::new(0)),
            timestamp: Arc::new(AtomicU64::new(0)),
            epoch: Arc::new(AtomicU64::new(0)),
            started: Instant::now(),
        metrics: Arc::new(Mutex::new(EncoderMetrics::default())),
        };
        let mut writer = FrameWriter::new(output, 4000);
        let duration = Duration::from_millis(33);
        writer
            .write(Bytes::from_static(KEY), duration)
            .await
            .unwrap();
        let one = fast.recv().await.unwrap();
        let shared = slow.next().await.unwrap();
        assert!(
            Arc::ptr_eq(&one, &shared),
            "viewers reuse the encoded allocation"
        );
        assert_eq!(one.codec.as_deref(), Some("avc1.42c01e"));
        for _ in 0..12 {
            writer
                .write(Bytes::from_static(DELTA), duration)
                .await
                .unwrap();
        }
        writer
            .write(
                Bytes::from_static(&[0, 0, 0, 1, 9, 0xf0, 0, 0, 0, 1, 0x65, 2]),
                duration,
            )
            .await
            .unwrap();
        let recovered = slow.next().await.unwrap();
        assert_eq!(recovered.sequence, 14);
        assert!(recovered.key);
        assert!(
            annex_b_nal_units(&recovered.data)
                .iter()
                .any(|&(_, p)| recovered.data[p] & 31 == 7)
        );
        assert!(
            annex_b_nal_units(&recovered.data)
                .iter()
                .any(|&(_, p)| recovered.data[p] & 31 == 8)
        );
        drop(finished);
    }

    #[tokio::test]
    async fn two_viewers_start_one_process_and_last_departure_stops_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::tls::tests::Directory::new();
        let executable = dir.0.join("encoder");
        let count = dir.0.join("starts");
        let script = format!(
            "#!/bin/sh\nprintf 'start\\n' >> '{}'\nwhile :; do printf '\\000\\000\\000\\001\\011\\360\\000\\000\\000\\001\\145\\210'; sleep 0.03; done\n",
            count.display()
        );
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (video, source) = VideoManager::test_source(VideoConfig::default());
        source.send_replace(Some(Arc::new(crate::devices::video::VideoFrame {
            jpeg: Bytes::from_static(&[255, 216, 255, 217]),
            sequence: 1,
        })));
        let config = H264Config {
            max_sessions: 2,
            ..H264Config::default()
        };
        let encoder = EncoderSpec {
            executable,
            codec: "libx264",
            label: "test",
            software: true,
            native: false,
            #[cfg(unix)]
            _program: None,
        };
        let hub = H264Hub::default();
        let mut first = hub
            .subscribe(video.clone(), encoder.clone(), &config)
            .await
            .unwrap();
        let mut second = hub
            .subscribe(video.clone(), encoder.clone(), &config)
            .await
            .unwrap();
        assert!(matches!(
            hub.subscribe(video.clone(), encoder.clone(), &config).await,
            Err(WebRtcError::Capacity)
        ));
        tokio::time::timeout(Duration::from_secs(3), first.next())
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), second.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1);
        second.set_bitrate(1000).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(3), first.next())
                .await
                .unwrap()
                .unwrap();
            if frame.bitrate_kbps == 1000 {
                break;
            }
            assert!(Instant::now() < deadline);
        }
        // Requests from different viewers are coalesced during the restart cooldown.
        first.set_bitrate(800).unwrap();
        second.set_bitrate(700).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 2);
        let mut done = hub.0.lock().unwrap().as_ref().unwrap().done.clone();
        drop(first);
        drop(second);
        assert_eq!(hub.viewers(), 0);
        tokio::time::timeout(Duration::from_secs(4), done.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(done.borrow().is_some());
        // Reconnecting after the idle grace creates a fresh live run.
        let mut next = hub.subscribe(video, encoder, &config).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), next.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 3);
        drop(next);
        hub.stop().await;
    }
}
