#!/usr/bin/env python3
"""Real WebCodecs decoding over a mocked WSS transport; no capture hardware."""
import json
import re
import ssl
import struct
import subprocess
import tempfile
from itertools import permutations
from pathlib import Path
from playwright.sync_api import sync_playwright
import ui_harness
from ui_bundle import module_assets
from verify_p2b_ui import State, api_route, bootstrap_payload
from verify_audit2_ui import FAKE_PEER, ADAPTIVE_DECISION_SCRIPT


# Keep the real decoder, but allow one scenario to pause submission to it.
# This models a slow decoder without relying on machine speed or CPU load.
DELAYED_DECODER = """(() => {
    const RealDecoder = window.VideoDecoder;
    if (!RealDecoder) return;
    window.__holdDirectDecode = false;
    window.__directDecoders = [];
    window.VideoDecoder = class {
        static isConfigSupported(config) { return RealDecoder.isConfigSupported(config); }
        constructor(callbacks) {
            this.inner = new RealDecoder(callbacks); this.held = [];
            window.__directDecoders.push(this);
        }
        get state() { return this.inner.state; }
        get decodeQueueSize() { return this.held.length + this.inner.decodeQueueSize; }
        configure(config) { this.inner.configure(config); }
        decode(chunk) {
            if (window.__holdDirectDecode) this.held.push(chunk);
            else this.inner.decode(chunk);
        }
        close() { this.held = []; this.inner.close(); }
        release() {
            for (const chunk of this.held.splice(0)) this.inner.decode(chunk);
        }
    };
})();"""


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
        self.in_flight = []
        self.max_in_flight = 0
        self.live = False
        ws.on_message(self.message)

        events.append("direct")
        self.restart(4000)

    def restart(self, kbps):
        self.epoch += 1
        self.sent = 0
        self.in_flight.clear()
        self.ws.send(json.dumps({"type": "config", "version": 1, "codec": self.codec,
                                "epoch": self.epoch, "bitrate_kbps": kbps, "max_kbps": 4000, "min_kbps": 256}))
        self.send_frame()

    @property
    def drained(self):
        return self.sent == len(self.frames) and not self.in_flight

    def send_frame(self, window=1):
        # Normal scenarios have one frame in flight, so only a decoder ACK
        # advances them. The congestion scenario explicitly uses the real
        # server's eight-frame window to exercise bounded queue recovery.
        if self.sent >= len(self.frames) or len(self.in_flight) >= window:
            return
        data = self.frames[self.sent]
        key = bool(re.search(b"\x00\x00(?:\x00)?\x01[\x65]", data))
        self.seq += 1
        self.sent += 1
        self.in_flight.append(self.seq)
        self.max_in_flight = max(self.max_in_flight, len(self.in_flight))
        header = struct.pack(">4sB3xIQIQ", b"WK26", key, self.epoch, self.seq * 33333, 33333, self.seq)
        self.ws.send(header + data)

    def message(self, text):
        message = json.loads(text)
        if message["type"] == "ack":
            sequence = message["sequence"]
            # Config messages may acknowledge the previous epoch again.
            # Like the real server, ignore unknown/duplicate acknowledgements.
            if sequence not in self.in_flight:
                return
            del self.in_flight[:self.in_flight.index(sequence) + 1]
            self.acks.append(sequence)
            self.ws.send(json.dumps({"type": "feedback", "rtt_ms": 420}))
            self.send_frame()
        elif message["type"] == "bitrate":
            target = max(256, min(4000, message["kbps"]))
            self.events.append(("bitrate", target))
            self.ws.send(json.dumps({"type": "bitrate", "kbps": target, "max_kbps": 4000}))
            if self.live:
                self.ws.send(json.dumps({"type":"bitrate_applied", "kbps":target}))
                if self.drained:
                    self.sent = 0
                    self.send_frame()
            else:
                self.restart(target)


def automatic_transport_orders(browser, port, frames, codec, checks):
    """Exercise all six fallback orders and successful Direct/WebRTC recovery."""
    modes = ('webrtc_h264', 'direct_h264', 'mjpeg')
    cases = [(list(order), 'mjpeg', True) for order in permutations(modes)]
    cases += [(['direct_h264', 'webrtc_h264', 'mjpeg'], 'direct_h264', True),
              (['direct_h264', 'webrtc_h264', 'mjpeg'], 'webrtc_h264', True),
              (['mjpeg', 'direct_h264', 'webrtc_h264'], 'webrtc_h264', True),
              (['direct_h264', 'webrtc_h264', 'mjpeg'], 'mjpeg', False)]
    for index, (order, successful, decoder) in enumerate(cases):
        trace = []
        context = browser.new_context(ignore_https_errors=True)
        page = context.new_page()
        page.add_init_script(FAKE_PEER)
        page.add_init_script("localStorage.setItem('wingman_setting_video_auto_order', "
                             + json.dumps(','.join(order)) + ");")
        if not decoder:
            page.add_init_script("Object.defineProperty(window, 'VideoDecoder', {value:undefined, configurable:true});")
        state = State()
        state.video.update(state='ready', last_frame_age_ms=10, frames_per_second=30)
        page.route('**/api/**', lambda route: api_route(route, state))
        def bootstrap(route):
            payload = bootstrap_payload(state)
            payload['capabilities'].update(video_webrtc_h264=True, video_direct_h264=True)
            payload['webrtc'].update(available=True, bitrate_kbps=4000)
            route.fulfill(status=200, content_type='application/json', body=json.dumps(payload))
        def offer(route):
            trace.append('webrtc_h264')
            result = {'type':'answer', 'session_id':'order', 'sdp':'a=rtpmap:96 H264/90000'}
            route.fulfill(status=200 if successful == 'webrtc_h264' else 503,
                          content_type='application/json', body=json.dumps(result))
        def mjpeg(route):
            trace.append('mjpeg')
            if successful == 'mjpeg':
                route.fulfill(status=200, content_type='image/gif', body=ui_harness.GIF)
            else:
                route.abort('failed')
        def direct(ws):
            trace.append('direct_h264')
            if successful == 'direct_h264':
                Stream(ws, frames, codec, trace)
            else:
                ws.send(b'broken-packet')
        page.route('**/api/bootstrap', bootstrap)
        page.route('**/api/webrtc/offer', offer)
        page.route('**/api/webrtc/close', lambda route: route.fulfill(
            status=200, content_type='application/json', body='{}'))
        page.route('**/video_feed*', mjpeg)
        page.route_web_socket('**/api/video/h264/ws', direct)
        try:
            page.goto(f'https://localhost:{port}/', wait_until='load')
            page.wait_for_function('(mode) => document.querySelector("#video-transport-label").textContent.includes("LIVE / " + mode)',
                                   arg={'mjpeg':'MJPEG', 'direct_h264':'Direct H.264', 'webrtc_h264':'H.264'}[successful], timeout=12000)
            attempts = [mode for mode in trace if mode in modes]
            available = [mode for mode in order if decoder or mode != 'direct_h264']
            expected = available[:available.index(successful)+1]
            ui_harness.check(checks, attempts == expected, f'video_order_connection_and_fallback_{index}', attempts)
            if successful != 'mjpeg':
                page.wait_for_timeout(3500)
                ui_harness.check(checks, [mode for mode in trace if mode in modes] == expected,
                                 f'video_order_stable_success_is_not_interrupted_{index}', trace)
        finally:
            context.close()


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
            page.add_init_script(FAKE_PEER + "\nRTCPeerConnection.prototype.setRemoteDescription = async function() { throw new Error('fixture ICE failure'); };" + DELAYED_DECODER + ui_harness.TEST_HOOKS)
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
            check(ui_harness.wait_until(page, lambda: streams[0].drained, timeout=5),
                  "previous_epoch_drains_before_live_bitrate_test")
            before_acks = len(streams[0].acks)
            streams[0].message(json.dumps({"type":"ack", "sequence":streams[0].seq}))
            check(len(streams[0].acks) == before_acks and streams[0].drained,
                  "duplicate_ack_does_not_release_another_fixture_frame")
            streams[0].live = True
            page.evaluate("async () => (await import('/assets/js/07e-direct-h264.js')).setDirectBitrate(1200)")
            page.wait_for_function("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()?.effectiveKbps === 1200")
            check(ui_harness.wait_until(page, lambda: streams[0].drained and len(streams[0].acks) > before_acks, timeout=5),
                  "live_bitrate_change_delivers_new_decoded_frames")
            check(streams[0].epoch == 3, "native_bitrate_feedback_keeps_decoder_epoch")
            direct_stats = page.evaluate("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()")
            check(direct_stats['decoder'].startswith('WebCodecs') and direct_stats['framesDropped'] == 0,
                  "direct_reports_decoder_and_drop_diagnostics", direct_stats)
            page.wait_for_timeout(3500)
            check(events.count("offer") == 1 and "mjpeg" not in events, "stable_direct_does_not_retry_webrtc", events)
            page.evaluate("""async () => {
                const s = (await import('/assets/js/state.js')).shared;
                s.latestWebrtcStatus = {encoder:'x264 (native)', live_bitrate:true,
                    telemetry:{fps:15,processing_ms:32.1,last_frame_age_ms:8,restarts:0,
                        bitrate_changes:6,bitrate_kbps:1200,max_frame_gap_ms:79}};
                const q = await import('/assets/js/07b-quality.js');
                q.setQualityOverlayEnabled(true); q.renderQualityOverlay();
            }""")
            def overlay_bounds():
                return page.locator('#quality-overlay').evaluate("""e => {
                    const b=e.getBoundingClientRect(), p=e.parentElement.getBoundingClientRect();
                    return {inside:b.left>=p.left && b.right<=Math.min(p.right,innerWidth) && b.bottom<=p.bottom,
                        columns:getComputedStyle(e).gridTemplateColumns.split(' ').length,
                        pointer:getComputedStyle(e).pointerEvents};
                }""")
            wide = overlay_bounds()
            check(wide['inside'] and wide['columns'] == 4 and wide['pointer'] == 'none',
                  "direct_diagnostics_fit_wide_console_without_intercepting_input", wide)
            with page.expect_download() as download:
                page.locator("#screenshot").click()
            if args.output_dir:
                Path(args.output_dir).mkdir(parents=True, exist_ok=True)
                download.value.save_as(str(Path(args.output_dir) / "direct-decoded.png"))
                page.screenshot(path=str(Path(args.output_dir) / "direct-desktop.png"))
            page.set_viewport_size({"width": 640, "height": 900})
            page.wait_for_function("() => getComputedStyle(document.querySelector('#quality-overlay')).gridTemplateColumns.split(' ').length === 2")
            page.locator('#inspector-close').click()
            page.wait_for_function("() => !document.querySelector('#app').classList.contains('inspector-open')")
            page.wait_for_timeout(350)  # Let the mobile sheet finish closing before the screenshot.
            narrow = overlay_bounds()
            check(narrow['inside'] and narrow['columns'] == 2,
                  "direct_diagnostics_fit_narrow_console", narrow)
            if args.output_dir:
                page.screenshot(path=str(Path(args.output_dir) / "direct-narrow.png"))
            page.set_viewport_size({"width": 375, "height": 900})
            page.evaluate("async () => (await import('/assets/js/01-i18n.js')).setLanguage('en')")
            page.wait_for_function("() => document.querySelector('#quality-overlay').textContent.includes('Encoder backend')")
            check(overlay_bounds()['inside'], "direct_diagnostics_fit_phone_in_english", overlay_bounds())
            if args.output_dir:
                page.screenshot(path=str(Path(args.output_dir) / "direct-phone-en.png"))
            page.evaluate("async () => (await import('/assets/js/01-i18n.js')).setLanguage('zh-CN')")
            page.set_viewport_size({"width": 1280, "height": 900})
            page.wait_for_function("() => getComputedStyle(document.querySelector('#quality-overlay')).gridTemplateColumns.split(' ').length === 4")
            check(download.value.suggested_filename.endswith(".png"), "direct_screenshot_works")
            decisions = page.evaluate(ADAPTIVE_DECISION_SCRIPT)
            check(decisions["rtt"] == [] and decisions["trend"] == [[2800, 7000]], "stable_high_rtt_holds_but_rising_rtt_lowers", decisions)
            high_latency = page.evaluate("""() => {
                const t = window.__wingmanTest; let s = t.newAdaptiveState(4000), targets=[];
                for(let i=0;i<130;i++) { const r=t.decideAdaptiveBitrate(s,{packetLossPct:i<3?8:0,rttMs:420,jitterBufferMs:20},i*1000); s=r.state; if(r.targetKbps) targets.push(r.targetKbps); }
                return targets;
            }""")
            check(high_latency == [2800, 3500, 4000], "high_latency_link_recovers_to_maximum", high_latency)

            # Congestion is a separate scenario: hold real decode submission,
            # send within the server's bounded window, then resume at an IDR.
            check(streams[0].drained and streams[0].max_in_flight == 1,
                  "normal_fixture_preserves_single_frame_backpressure")
            page.evaluate("window.__holdDirectDecode = true")
            streams[0].sent = 0
            for _ in range(8):
                streams[0].send_frame(window=8)
            page.wait_for_function("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()?.framesDropped > 0")
            page.evaluate("""() => {
                window.__holdDirectDecode = false;
                for (const decoder of window.__directDecoders)
                    if (decoder.state === 'configured') decoder.release();
            }""")
            check(ui_harness.wait_until(page, lambda: streams[0].drained, timeout=5),
                  "decoder_backlog_recovers_and_acknowledges_new_frames")
            recovered = page.evaluate("async () => (await import('/assets/js/07e-direct-h264.js')).readDirectStats()")
            check(recovered['framesDropped'] > 0 and recovered['decodeQueue'] == 0
                  and 1 < streams[0].max_in_flight <= 8 and streams[0].epoch == 3,
                  "decoder_backlog_is_bounded_and_recovers_without_epoch_restart", recovered)
            # The persisted manual choice bypasses WebRTC on the next connection.
            page.locator('#inspector-toggle').click()
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
            automatic_transport_orders(browser, port, frames, codec, checks)
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
