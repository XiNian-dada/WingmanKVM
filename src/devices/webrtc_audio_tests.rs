//! Real loopback peers for the optional HDMI audio track (issue #71). Fake
//! FFmpeg scripts stand in for both the H.264 encoder and the ALSA capture;
//! no hardware is touched.
use super::tests::{ProbeFixture, serial_probe_fixture, shell_quote};
use super::*;
use crate::{
    config::{AudioConfig, VideoConfig},
    devices::audio::{
        AudioSettings, AudioState,
        tests::{TempTree, ogg_page, opus_stream, single_card_proc},
    },
};
use webrtc::rtp_transceiver::{
    RTCRtpTransceiverInit, rtp_codec::RTPCodecType,
    rtp_transceiver_direction::RTCRtpTransceiverDirection,
};

const VIDEO_SCRIPT: &str =
    r"while :; do printf '\000\000\000\001\011\360\000\000\000\001\145\210\204'; sleep 0.05; done";

struct Loopback {
    manager: WebRtcManager,
    // Keeps the synthetic frame source alive for the encoder writer.
    _frames: watch::Sender<Option<Arc<crate::devices::video::VideoFrame>>>,
    sessions: crate::auth::SessionStore,
    token: String,
}

fn loopback(fixture: &ProbeFixture, audio: AudioManager) -> Loopback {
    let script = fixture.script("loopback-video", VIDEO_SCRIPT);
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
            audio,
            config: Mutex::new(H264Config {
                udp_bind_address: Some(address),
                max_sessions: 2,
                ..H264Config::default()
            }),
            encoder: Mutex::new(Ok(EncoderSpec {
                executable: script,
                codec: "libx264",
                label: "test encoder",
                software: true,
            })),
            sessions: Mutex::new(HashMap::new()),
            offer_lock: AsyncMutex::new(()),
            udp_binding: AsyncMutex::new(None),
            probe_epoch: AtomicU64::new(0),
        }),
    };
    let sessions = crate::auth::SessionStore::new(Duration::from_secs(60));
    let token = sessions.create().unwrap();
    Loopback {
        manager,
        _frames: frames,
        sessions,
        token,
    }
}

fn enabled_audio(
    fixture: &ProbeFixture,
    body: &str,
    proc: &TempTree,
    sys: &TempTree,
) -> AudioManager {
    let script = fixture.script("loopback-audio", body);
    AudioManager::with_roots(
        AudioSettings::new(
            AudioConfig {
                enabled: true,
                ..AudioConfig::default()
            },
            Some(script),
            None,
        ),
        proc.0.clone(),
        sys.0.clone(),
    )
}

type Packets = mpsc::Receiver<(RTPCodecType, Bytes)>;

/// A browser-like client offering recvonly video and, optionally, audio.
async fn connect(
    loopback: &Loopback,
    with_audio: bool,
) -> (RTCPeerConnection, WebRtcOfferResponse, Packets) {
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
    let mut kinds = vec![RTPCodecType::Video];
    if with_audio {
        kinds.push(RTPCodecType::Audio);
    }
    for kind in kinds {
        client
            .add_transceiver_from_kind(
                kind,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: Vec::new(),
                }),
            )
            .await
            .unwrap();
    }
    let (packet_tx, packet_rx) = mpsc::channel(256);
    client.on_track(Box::new(move |track, _, _| {
        let tx = packet_tx.clone();
        Box::pin(async move {
            tokio::spawn(async move {
                let kind = track.kind();
                while let Ok((packet, _)) = track.read_rtp().await {
                    let _ = tx.try_send((kind, packet.payload));
                }
            });
        })
    }));
    let mut gathered = client.gathering_complete_promise().await;
    let offer = client.create_offer(None).await.unwrap();
    client.set_local_description(offer).await.unwrap();
    tokio::time::timeout(Duration::from_secs(6), gathered.recv())
        .await
        .unwrap();
    let offer = client.local_description().await.unwrap();
    assert_eq!(offer_requests_audio(&offer.sdp), with_audio);
    let answer = loopback
        .manager
        .offer(
            WebRtcOfferRequest {
                sdp_type: "offer".into(),
                sdp: offer.sdp,
            },
            loopback.sessions.validate_guard(&loopback.token).unwrap(),
        )
        .await
        .expect("negotiation must succeed with or without audio");
    client
        .set_remote_description(RTCSessionDescription::answer(answer.sdp.clone()).unwrap())
        .await
        .unwrap();
    (client, answer, packet_rx)
}

async fn next_packet(packets: &mut Packets, kind: RTPCodecType) -> Bytes {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (got, payload) = packets.recv().await.expect("client track open");
            if got == kind {
                return payload;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {kind} packet arrived"))
}

async fn wait_until(mut condition: impl FnMut() -> bool, what: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

#[test]
fn detects_audio_sections_in_an_offer() {
    assert!(offer_requests_audio(
        "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n"
    ));
    assert!(!offer_requests_audio(
        "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=x:m=audio\r\n"
    ));
}

#[tokio::test]
// The std guard only serialises fixture-spawning tests.
#[allow(clippy::await_holding_lock)]
async fn disabled_audio_still_negotiates_an_audio_offer_and_streams_video() {
    let _serial = serial_probe_fixture();
    let fixture = ProbeFixture::new();
    let loopback = loopback(&fixture, disabled_audio());
    let (client, answer, mut packets) = connect(&loopback, true).await;
    assert!(!answer.audio);
    assert!(answer.sdp.contains("H264/90000"));
    assert!(
        !next_packet(&mut packets, RTPCodecType::Video)
            .await
            .is_empty()
    );
    assert_eq!(
        loopback.manager.audio().status().state,
        AudioState::Disabled
    );
    client.close().await.unwrap();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn enabled_audio_without_a_device_adds_no_track() {
    let _serial = serial_probe_fixture();
    let fixture = ProbeFixture::new();
    let proc = TempTree::new("rtc-nodevice");
    let sys = TempTree::new("rtc-nodevice-sys");
    let audio = enabled_audio(&fixture, "exit 1", &proc, &sys);
    let loopback = loopback(&fixture, audio);
    let (client, answer, mut packets) = connect(&loopback, true).await;
    assert!(!answer.audio, "no device, no audio track");
    assert!(
        !next_packet(&mut packets, RTPCodecType::Video)
            .await
            .is_empty()
    );
    assert_eq!(loopback.manager.audio().status().viewers, 0);
    client.close().await.unwrap();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn a_failing_audio_capture_never_interrupts_video() {
    let _serial = serial_probe_fixture();
    let fixture = ProbeFixture::new();
    let proc = single_card_proc("rtc-failing");
    let sys = TempTree::new("rtc-failing-sys");
    let audio = enabled_audio(
        &fixture,
        "echo 'audio open error: Input/output error' >&2\nexit 1",
        &proc,
        &sys,
    );
    let loopback = loopback(&fixture, audio);
    let (client, answer, mut packets) = connect(&loopback, true).await;
    assert!(answer.audio, "device present: the track is negotiated");
    next_packet(&mut packets, RTPCodecType::Video).await;

    let audio = loopback.manager.audio().clone();
    wait_until(
        || {
            let status = audio.status();
            status.state == AudioState::Error && status.restarts >= 1
        },
        "the audio capture to fail",
    )
    .await;
    // Video keeps flowing in the same session after audio failed.
    while packets.try_recv().is_ok() {}
    assert!(
        !next_packet(&mut packets, RTPCodecType::Video)
            .await
            .is_empty()
    );
    assert_eq!(loopback.manager.status().sessions, 1);
    assert_eq!(audio.status().viewers, 1);

    assert!(loopback.sessions.revoke(&loopback.token));
    wait_until(
        || loopback.manager.status().sessions == 0,
        "session teardown",
    )
    .await;
    wait_until(
        || audio.status().viewers == 0,
        "the audio share to be released",
    )
    .await;
    client.close().await.unwrap();
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn the_audio_track_carries_opus_from_the_shared_capture() {
    let _serial = serial_probe_fixture();
    let fixture = ProbeFixture::new();
    let head = fixture.path("head.ogg");
    let frame = fixture.path("frame.ogg");
    let pids = fixture.path("audio.pids");
    std::fs::write(&head, opus_stream(0)).unwrap();
    std::fs::write(&frame, ogg_page(0, 5, &[&[0xfc, 0x01, 0x42]])).unwrap();
    let proc = single_card_proc("rtc-audio");
    let sys = TempTree::new("rtc-audio-sys");
    let body = format!(
        "echo $$ >> {}\ncat {}\nwhile :; do cat {}; sleep 0.02; done",
        shell_quote(&pids),
        shell_quote(&head),
        shell_quote(&frame)
    );
    let audio = enabled_audio(&fixture, &body, &proc, &sys);
    let loopback = loopback(&fixture, audio);
    let (client, answer, mut packets) = connect(&loopback, true).await;
    assert!(answer.audio);
    assert!(answer.sdp.to_ascii_lowercase().contains("opus/48000"));
    let payload = next_packet(&mut packets, RTPCodecType::Audio).await;
    assert_eq!(payload.as_ref(), &[0xfc, 0x01, 0x42]);
    next_packet(&mut packets, RTPCodecType::Video).await;
    let audio = loopback.manager.audio().clone();
    assert_eq!(audio.status().state, AudioState::Running);

    // A video-only viewer joins without an audio track or a second capture.
    let (second, second_answer, mut second_packets) = connect(&loopback, false).await;
    assert!(!second_answer.audio);
    next_packet(&mut second_packets, RTPCodecType::Video).await;
    assert_eq!(audio.status().viewers, 1);

    assert!(loopback.sessions.revoke(&loopback.token));
    wait_until(
        || loopback.manager.status().sessions == 0,
        "session teardown",
    )
    .await;
    wait_until(|| audio.status().state == AudioState::Idle, "capture stop").await;
    let pid: i32 = std::fs::read_to_string(&pids)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    wait_until(
        || unsafe { libc::kill(pid, 0) } != 0,
        "the capture process to exit with the last viewer",
    )
    .await;
    client.close().await.unwrap();
    second.close().await.unwrap();
}
