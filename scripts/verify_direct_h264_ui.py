#!/usr/bin/env python3
"""Real WebCodecs decoding over a mocked WSS transport; no capture hardware."""
import json
import re
import ssl
import struct
import subprocess
import tempfile
from pathlib import Path
from playwright.sync_api import sync_playwright
import ui_harness
from ui_bundle import module_assets
from verify_p2b_ui import State, api_route, bootstrap_payload
from verify_audit2_ui import FAKE_PEER, ADAPTIVE_DECISION_SCRIPT


def fixture(root):
    path = root / "fixture.h264"
    # Exercise the live pipeline's JPEG demuxer and bitstream filter too. A
    # direct lavfi -> x264 fixture missed duplicate AUDs and probe-frame loss.
    jpeg = subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
                           "testsrc2=size=128x72:rate=30", "-frames:v", "30", "-threads", "1",
                           "-c:v", "mjpeg", "-f", "image2pipe", "pipe:1"],
                          check=True, capture_output=True).stdout
    pictures = [part + b"\xff\xd9" for part in jpeg.split(b"\xff\xd9") if part]
    assert len(pictures) == 30, "JPEG fixture must contain thirty pictures"
    multipart = b"".join(
        f"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {len(picture)}\r\n\r\n".encode()
        + picture + b"\r\n" for picture in pictures)
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-probesize", "32",
                    "-analyzeduration", "0", "-f", "mpjpeg", "-r", "30", "-i", "pipe:0",
                    "-an", "-fps_mode", "passthrough", "-vf", "format=yuv420p", "-threads", "1",
                    "-c:v", "libx264", "-profile:v", "baseline", "-preset", "ultrafast",
                    "-tune", "zerolatency", "-b:v", "4000k", "-maxrate", "4000k", "-bufsize", "2000k",
                    "-g", "15", "-bf", "0", "-x264-params", "repeat-headers=1:scenecut=0",
                    "-bsf:v", "h264_metadata=aud=insert", "-f", "h264", str(path)],
                   input=multipart, check=True)
    data = path.read_bytes()
    starts = [match.start() for match in re.finditer(b"\x00\x00(?:\x00)?\x01\x09", data)]
    frames = [data[a:b] for a, b in zip(starts, starts[1:] + [len(data)])]
    assert len(frames) == len(pictures), "encoder must preserve every fixture picture"
    assert all(re.search(b"\x00\x00(?:\x00)?\x01[\x41\x65]", frame) for frame in frames), "no AUD-only frames"
    sps = re.search(b"\x00\x00(?:\x00)?\x01\x67(...)" , data, re.S).group(1)
    return frames, "avc1." + sps.hex()


class Stream:
    def __init__(self, ws, frames, codec, events):
        self.ws, self.frames, self.codec, self.events = ws, frames, codec, events
        self.seq, self.epoch, self.sent, self.acks = 0, 0, 0, []
        ws.on_message(self.message)

        events.append("direct")
        self.restart(4000)

    def restart(self, kbps):
        self.epoch += 1
        self.sent = 0
        self.ws.send(json.dumps({"type": "config", "version": 1, "codec": self.codec,
                                "epoch": self.epoch, "bitrate_kbps": kbps, "max_kbps": 4000, "min_kbps": 256}))
        for _ in range(4):
            self.send_frame()

    def send_frame(self):
        if self.sent >= len(self.frames):
            return
        data = self.frames[self.sent]
        key = bool(re.search(b"\x00\x00(?:\x00)?\x01[\x65]", data))
        self.seq += 1
        self.sent += 1
        header = struct.pack(">4sB3xIQIQ", b"WK26", key, self.epoch, self.seq * 33333, 33333, self.seq)
        self.ws.send(header + data)

    def message(self, text):
        message = json.loads(text)
        if message["type"] == "ack":
            self.acks.append(message["sequence"])
            self.ws.send(json.dumps({"type": "feedback", "rtt_ms": 420}))
            self.send_frame()
        elif message["type"] == "bitrate":
            target = max(256, min(4000, message["kbps"]))
            self.events.append(("bitrate", target))
            self.ws.send(json.dumps({"type": "bitrate", "kbps": target, "max_kbps": 4000}))
            self.restart(target)


def main():
    args = ui_harness.parse_args()
    summary = ui_harness.new_summary(args.repo)
    checks = summary["checks"]
    check = lambda condition, name, evidence="": ui_harness.check(checks, condition, name, evidence)
    server = None
    events, streams, messages = [], [], []
    try:
        with tempfile.TemporaryDirectory(prefix="wk-direct-ui-") as temp, sync_playwright() as playwright:
            root = Path(temp)
            frames, codec = fixture(root)
            subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                            "-nodes", "-days", "2", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
                            "-keyout", str(root / "key.pem"), "-out", str(root / "cert.pem")],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            tls.load_cert_chain(root / "cert.pem", root / "key.pem")
            server, port = ui_harness.serve_static(ui_harness.extract_html(args.repo), ui_harness.read_assets(args.repo), module_assets(args.repo), tls)
            browser = ui_harness.launch_browser(playwright, args.browser_channel)
            context = browser.new_context(ignore_https_errors=True, viewport={"width": 1280, "height": 900})
            page = context.new_page()
            events, streams = [], []
            messages = []
            page.on("console", lambda message: messages.append(message.text))
            state = State()
            state.video.update(state="ready", message="", last_frame_age_ms=10, frames_per_second=30)
            page.add_init_script(FAKE_PEER + "\nRTCPeerConnection.prototype.setRemoteDescription = async function() { throw new Error('fixture ICE failure'); };" + ui_harness.TEST_HOOKS)
            page.route("**/api/**", lambda route: api_route(route, state))
            def bootstrap(route):
                payload = bootstrap_payload(state)
                payload["capabilities"].update(video_webrtc_h264=True, video_direct_h264=True)
                payload["webrtc"].update(available=True, bitrate_kbps=4000)
                route.fulfill(status=200, content_type="application/json", body=json.dumps(payload))
            page.route("**/api/bootstrap", bootstrap)
            page.route("**/api/config", lambda route: route.fulfill(status=200, content_type="application/json", body="{}"))
            def offer(route):
                events.append("offer")
                route.fulfill(status=200, content_type="application/json", body=json.dumps({"type": "answer", "session_id": "s1", "sdp": "a=rtpmap:96 H264/90000"}))
            def close(route):
                events.append("close")
                route.fulfill(status=200, content_type="application/json", body="{}")
            page.route("**/api/webrtc/offer", offer)
            page.route("**/api/webrtc/close", close)
            def mjpeg(route):
                events.append("mjpeg")
                route.fulfill(status=200, content_type="image/gif", body=ui_harness.GIF)
            page.route("**/video_feed*", mjpeg)
            page.route_web_socket("**/api/video/h264/ws", lambda ws: streams.append(Stream(ws, frames, codec, events)))
            page.goto(f"https://localhost:{port}/", wait_until="load")
            page.wait_for_function("document.querySelector('#video-transport-label').textContent.includes('LIVE / Direct H.264')", timeout=12000)
            check(events[:3] == ["offer", "close", "direct"] and "mjpeg" not in events,
                  "failed_webrtc_closes_before_direct_and_keeps_h264", events)
            pixels = page.locator("#direct-feed").evaluate("e => ({width:e.width,height:e.height, colors:new Set(e.getContext('2d').getImageData(0,0,e.width,e.height).data).size})")
            check(pixels["width"] == 128 and pixels["height"] == 72 and pixels["colors"] > 32,
                  "real_annex_b_decoded_to_canvas", pixels)
            check(page.locator("#direct-feed").is_visible() and not page.locator("#video-feed").is_visible(), "only_direct_surface_visible")
            check(bool(streams[0].acks), "decoder_acknowledges_rendered_frames", streams[0].acks)
            page.wait_for_timeout(700)
            stats = page.evaluate("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()")
            check(stats["rttMs"] == 420 and stats["packetLossPct"] is None, "direct_reports_decode_rtt_without_fake_loss", stats)
            page.evaluate("""async () => {
                const q = await import('/assets/js/07b-quality.js'), a = await import('/assets/js/07c-adaptive-bitrate.js');
                Object.assign(q.connectionQuality,{rttMs:420,packetLossPct:null,jitterBufferMs:250});
                for(let i=0;i<3;i++) a.adaptiveBitrateTick(performance.now()+i*1000);
            }""")
            page.wait_for_function("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()?.effectiveKbps === 2800")
            check(("bitrate", 2800) in events, "direct_adaptation_sends_bitrate_over_wss", events)
            mapping = page.evaluate("""async () => {
                const input = await import('/assets/js/08-input.js'), e = input.activeVideoSurface(), b=e.getBoundingClientRect();
                return {id:e.id, point: input.mapAbsolute(b.x+b.width/2,b.y+b.height/2)};
            }""")
            check(mapping["id"] == "direct-feed" and abs(mapping["point"]["x"]-16384)<=1 and abs(mapping["point"]["y"]-16384)<=1,
                  "absolute_pointer_maps_direct_canvas", mapping)
            result = page.evaluate("async () => (await import('/assets/js/07e-direct-h264.js')).setDirectBitrate(2000)")
            check(result["kbps"] == 2000, "direct_bitrate_control_acknowledged", result)
            page.wait_for_function("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()?.effectiveKbps === 2000")
            check(streams[0].epoch == 3, "encoder_epoch_reconfigures_decoder")
            page.wait_for_timeout(3500)
            check(events.count("offer") == 1 and "mjpeg" not in events, "stable_direct_does_not_retry_webrtc", events)
            with page.expect_download() as download:
                page.locator("#screenshot").click()
            if args.output_dir:
                Path(args.output_dir).mkdir(parents=True, exist_ok=True)
                download.value.save_as(str(Path(args.output_dir) / "direct-decoded.png"))
                page.screenshot(path=str(Path(args.output_dir) / "direct-desktop.png"))
            check(download.value.suggested_filename.endswith(".png"), "direct_screenshot_works")
            decisions = page.evaluate(ADAPTIVE_DECISION_SCRIPT)
            check(decisions["rtt"] == [] and decisions["trend"] == [[2800, 7000]], "stable_high_rtt_holds_but_rising_rtt_lowers", decisions)
            high_latency = page.evaluate("""() => {
                const t = window.__wingmanTest; let s = t.newAdaptiveState(4000), targets=[];
                for(let i=0;i<70;i++) { const r=t.decideAdaptiveBitrate(s,{packetLossPct:i<3?8:0,rttMs:420,jitterBufferMs:20},i*1000); s=r.state; if(r.targetKbps) targets.push(r.targetKbps); }
                return targets;
            }""")
            check(high_latency == [2800, 3500, 4000], "high_latency_link_recovers_to_maximum", high_latency)
            # The persisted manual choice bypasses WebRTC on the next connection.
            page.locator('[data-panel-target="video"]').click()
            page.locator('#video-config select[name="transport"]').select_option("direct_h264")
            page.locator('#video-config button[type="submit"]').click()
            page.wait_for_function("document.querySelector('#video-transport-label').textContent.includes('LIVE / Direct H.264')")
            check(len(streams) == 2 and events.count("offer") == 1, "manual_direct_bypasses_webrtc", events)

            streams[-1].ws.send(b"broken-packet")
            page.wait_for_function("document.querySelector('#video-feed').naturalWidth > 0")
            check("mjpeg" in events and not page.locator("#direct-feed").is_visible(), "invalid_direct_stream_falls_back_to_mjpeg", events)
            # No WebCodecs: automatic mode still reaches MJPEG without a loop.
            page.add_init_script("Object.defineProperty(window, 'VideoDecoder', {value:undefined, configurable:true}); localStorage.setItem('wingmankvm.video.transport','auto');")
            before = len(streams)
            page.reload(wait_until="load")
            page.wait_for_function("document.querySelector('#video-feed').naturalWidth > 0")
            check(len(streams) == before, "unsupported_decoder_skips_direct")
            check(page.locator('#video-config option[value="direct_h264"]').evaluate("e => e.disabled"), "unsupported_direct_option_disabled", page.evaluate("typeof VideoDecoder"))
            context.close()
            browser.close()
            summary["status"] = "PASS"
    except Exception as error:
        summary["error"] = f"{type(error).__name__}: {error}"
        summary["debug"] = {"events": events, "console": messages[-20:]}
    finally:
        ui_harness.stop_server(server)
    return ui_harness.finish(summary, args.output_dir, "direct_h264_ui_report.json")

if __name__ == "__main__":
    raise SystemExit(main())
