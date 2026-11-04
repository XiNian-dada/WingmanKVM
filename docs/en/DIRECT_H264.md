# Direct H.264 and adaptive bitrate

Automatic playback tries **WebRTC H.264 → Direct H.264 → MJPEG**. Direct uses a same-origin WebSocket and WebCodecs without UDP or ICE. A working Direct connection is not interrupted to retry WebRTC. Select Direct H.264 in the video panel and apply to bypass WebRTC.

## Requirements

- A browser with `VideoDecoder`, an H.264 decoder and a secure context. Remote domain access requires HTTPS. Unsupported clients fall back to MJPEG.
- Enable self-signing or import a CA certificate in **Settings → HTTPS and certificates**. Establish browser trust for self-signed certificates; see [HTTPS](HTTPS.md). Existing HTTP configurations retain HTTP after an upgrade.
- Port 443 is unnecessary: TCP `38319 → device TCP 8080` supports `https://domain:38319` and `wss://domain:38319/api/video/h264/ws`. A reverse proxy must preserve the external Host and support WebSocket upgrades. Set `server.public_origin` when TLS terminates at the proxy, using the same Origin policy as the browser terminal.
- Selecting and allowing software encoding uses the bundled native x264 encoder by default, without system FFmpeg. Hardware encoding and an explicit `ffmpeg_path` retain FFmpeg 5.1 or newer. Direct cannot supply missing hardware drivers.
- Audio remains WebRTC-only. Direct and MJPEG do not carry audio. Screenshots, recording, picture-in-picture and pointer mapping use the active video surface.

## Shared encoder

One on-demand H.264 encoder serves both transports. `video.h264.max_sessions` limits their combined viewer count (default 1, configurable to 2–4); HID and MJPEG connections are excluded. Each viewer has an eight-frame queue. Lagging viewers discard delta frames until an IDR; the last viewer's departure stops encoding after a one-second grace period.

The encoder uses the **lowest requested bitrate among active viewers**. A slow viewer can lower other viewers' picture quality; separate per-viewer renditions are not produced. Disabling adaptation restores that viewer's request to the configured maximum, while another viewer can still request less. The Direct quality overlay distinguishes the viewer's request, shared encoder target and actual received bitrate. Static images may use far less bandwidth than the target.

## Encoding pipeline and latency limits

The native software path decodes JPEGs using libjpeg-turbo in a standalone process and calls x264 directly. Grayscale enters the luma plane; color YCbCr becomes I420 without an RGB intermediate. Every JPEG yields one length-delimited output frame without waiting for another AUD. Bitrate and VBV changes call `x264_encoder_reconfig` before the next frame, retaining the instance, reference pictures and timestamps.

In the FFmpeg path, JPEGs reach FFmpeg as multipart packets with `Content-Length`. A small probe window retains probe frames, avoiding startup frame loss and waiting for the next JPEG to complete the current packet. `-fps_mode passthrough` preserves received pictures instead of duplicating or dropping them to match a probed frame rate. Only `h264_metadata` inserts AUDs. Only H.264 units containing pictures enter viewer queues and advance media time; separate SPS/PPS units are still cached.

The FFmpeg Annex B output parser still uses the next AUD to delimit the current picture, usually adding one source frame interval during continuous streaming (about 67 ms at 15 FPS). Capture, JPEG processing, encoding load, network and display add further latency. These changes do not alter MJPEG playback.

RK3399 silicon supports H.264 encoding, but using it requires compatible kernel drivers, MPP and FFmpeg. An `h264_v4l2m2m` entry in `ffmpeg -encoders` only confirms the interface was compiled. A V4L2 encoder node whose capture formats list only JPEG cannot encode H.264 through that node. [ffmpeg-rockchip](https://github.com/nyanmisaka/ffmpeg-rockchip) requires a Rockchip BSP/vendor kernel and MPP device access; replacing FFmpeg or integrating [MPP](https://github.com/rockchip-linux/mpp) directly cannot supply a missing driver.

## Adaptation

Only samples with new media data count. Cached reports and empty reports during encoder gaps do not advance adaptation. Recovery requires decoded frames; new packets showing congestion can still lower bitrate while decoding is stalled. A five-second settling period after changes excludes encoder and decoder warm-up. Three consecutive congestion samples lower bitrate; loss, buffering and RTT growth relative to the connection's lowest observed RTT are considered. Stable high RTT alone neither lowers bitrate nor prevents recovery. WebRTC samples with fewer than 20 packets do not provide a loss ratio. Direct uses decode ACK RTT and relative buffering delay without inventing a TCP packet-loss metric. These are not HDMI-to-display latency measurements.

Decrease requests are at least 30 seconds apart. Recovery requires 30 consecutive healthy samples and at least 60 seconds since the last change and congestion. A decrease within 90 seconds of a raise counts as a failed probe, extending the recovery hold from 60 to 120 to 240 seconds. Ten congestion-free minutes clear the failure count.

The default native software encoder changes bitrate without restarting or resetting its epoch/browser decoder. Hardware encoding and an explicit FFmpeg executable retain the following restart safeguards.

Changing bitrate still briefly restarts FFmpeg. The shared encoder coalesces the latest requests across all viewers: the first decrease can be immediate, subsequent decreases are at least 30 seconds apart, and increases wait at least 60 seconds after startup or the last restart. The old bitrate keeps streaming while a change waits; reverting to the running target cancels the pending restart. Each new epoch starts with an IDR and SPS/PPS, and reconfigures the browser decoder.

## Wire protocol v1

`/api/video/h264/ws` requires a live login cookie and matching Origin before upgrade. Logout and session expiry end the stream.

A configuration JSON precedes frames: `{"type":"config","version":1,"codec":"avc1.42c01e","epoch":1,"bitrate_kbps":4000,"max_kbps":4000,"min_kbps":256}`.

Each binary message has a 32-byte big-endian header, then one complete Annex B access unit:

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | ASCII `WK26` |
| 4 | 1 | bit 0: IDR |
| 5 | 3 | reserved zero bytes |
| 8 | 4 | encoder epoch |
| 12 | 8 | strictly increasing microsecond timestamp |
| 20 | 4 | duration in microseconds |
| 24 | 8 | strictly increasing sequence |

Timestamps are relative to encoder startup, not UTC. Sequence and timestamp continue across encoder restarts within one shared run; epoch increments. IDRs carry the required SPS/PPS.

After decode/display the client sends `{"type":"ack","sequence":N}`. Only a frame outstanding on this connection can be cumulatively acknowledged. The window permits eight frames, each at most 4 MiB, with less than 8 MiB of unacknowledged payload. ACK/write timeout is three seconds; no-frame timeout is ten seconds. Decoder overload drops queued frames and waits for a key frame.

`{"type":"bitrate","kbps":2000}` changes the viewer's request; the reply reports the clamped request and maximum. The next config reports the actual shared target. Feedback RTT is measured from server send to decode ACK and includes network and decode time.

Rust tests exercise shared encoding, recovery, authentication/Origin, bounded ACK windows, timeouts and epoch changes. `scripts/verify_direct_h264_ui.py` generates 30 frames through multipart JPEG → FFmpeg → H.264, checks that no pictures are lost and no AUD-only units are emitted, then decodes them in Chrome over a locally mocked HTTPS/WSS service. These checks do not establish production-network or ARM64 hardware performance.
