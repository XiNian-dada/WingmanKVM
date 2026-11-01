#!/usr/bin/env python3
"""Issue #1 regression: terminal teardown on logout and HTTP 401."""
import json, os, sys, time, urllib.parse
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ui_harness
from ui_harness import mock_video_feed, static_server
try:
    from playwright.sync_api import sync_playwright
except Exception as exc:  # pragma: no cover - environment guard
    print(json.dumps({"status": "FAIL", "error": f"Playwright import failed: {exc}"}))
    sys.exit(2)
USER, PASSWORD = "admin", "Synthetic-Passw0rd!"
BOOTSTRAP = {
    "setup_required": False, "token_required": False, "authenticated": True,
    "username": "admin", "client_ip": "127.0.0.1",
    "client": {"ip": "127.0.0.1", "port": 12345, "source": "socket"},
    "config": {
        "server": {"proxy_protocol": "auto", "trusted_http_proxies": []},
        "video": {"device": "/dev/video0", "width": 1280, "height": 720,
                  "frames_per_second": 30, "jpeg_quality": 80, "encoding": "mjpeg"},
        "hid": {"keyboard_device": "/dev/hidg0", "mouse_device": "/dev/hidg1",
                "absolute_pointer_device": "/dev/hidg2", "pointer_mode": "absolute"},
        "power": {"enabled": False, "gpio_chip": None, "gpio_line": None, "active_high": True},
        "media": {"enabled": False, "lun_path": None, "image_directory": None},
    },
    "video": {"state": "ready", "message": "", "frames_per_second": 30.0,
              "width": 1280, "height": 720},
    "webrtc": {"available": False, "reason": "disabled by test"},
    "capabilities": {"video": True, "hid": True, "power": False, "mass_storage": False,
                     "terminal": True, "pointer_mode": "absolute",
                     "video_webrtc_h264": False},
}
STATUS = {"video": BOOTSTRAP["video"],
          "power": {"power_led": {"configured": False, "state": "unknown"}},
          "hid": {"keyboard_leds": {"num_lock": False, "caps_lock": False,
                                     "scroll_lock": False, "raw": 0}},
          "client": BOOTSTRAP["client"],
          "system": {"cpu": {"percent": 2.0, "cores": 2}, "load_avg": [0.01, 0.02, 0.03],
                     "memory": {"total_bytes": 1048576, "used_bytes": 524288, "percent": 50},
                     "disk": {"total_bytes": 4194304, "used_bytes": 1048576, "percent": 25},
                     "uptime_seconds": 60}}
WS_INIT = r"""
(() => {
  const P1 = window.__p1 = {sockets: [], closes: [], terminals: [], forceStatus401: false};
  class StubWebSocket extends EventTarget {
    constructor(url) {
      super(); this.url = String(url); this.readyState = 0; this.binaryType = "blob";
      this.bufferedAmount = 0; this.protocol = ""; this._handlers = {}; this._last = {};
      this.id = P1.sockets.length; P1.sockets.push(this);
      setTimeout(() => { if (this.readyState !== 0) return; this.readyState = 1;
        if (this._handlers.open) this._handlers.open.call(this, {type: "open", target: this}); }, 0);
    }
    _set(kind, fn) {
      this._handlers[kind] = typeof fn === "function" ? fn : null;
      if (typeof fn === "function") this._last[kind] = fn;
    }
    send(data) { this._sent = data; }
    close(code, reason) {
      if (this.readyState === 3) return; this.readyState = 3; P1.closes.push(this.id);
      const h = this._handlers.close; if (h) setTimeout(() => h.call(this,
        {type: "close", code: code || 1000, reason: reason || "", wasClean: true, target: this}), 0);
    }
  }
  for (const kind of ["open", "message", "error", "close"])
    Object.defineProperty(StubWebSocket.prototype, "on" + kind,
      {get() { return this._handlers[kind]; }, set(fn) { this._set(kind, fn); }});
  StubWebSocket.CONNECTING = 0; StubWebSocket.OPEN = 1;
  StubWebSocket.CLOSING = 2; StubWebSocket.CLOSED = 3;
  // Only the terminal socket is stubbed; other sockets (the keyboard/mouse
  // channel) use the real implementation and fall back to HTTP when the
  // static test server refuses them.
  const RealWebSocket = window.WebSocket;
  function RoutedWebSocket(url, protocols) {
    return String(url).includes("/api/terminal/ws")
      ? new StubWebSocket(url) : new RealWebSocket(url, protocols);
  }
  Object.assign(RoutedWebSocket, {CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3});
  window.WebSocket = RoutedWebSocket;
  const emit = (id, kind, data) => {
    const s = P1.sockets[id]; if (!s) throw new Error("socket " + id + " missing");
    const h = s._handlers[kind]; if (!h) return false;
    return h.call(s, {type: kind, data, target: s}) !== undefined;
  };
  const emitStale = (id, kind, data) => {
    const s = P1.sockets[id]; if (!s) throw new Error("socket " + id + " missing");
    const h = s._last[kind]; if (!h) return false; h.call(s, {type: kind, data, target: s}); return true;
  };
  P1.emitMessage = (id, text) => emit(id, "message", text);
  P1.emitError = (id) => emit(id, "error", null);
  P1.emitClose = (id) => emit(id, "close", null);
  P1.emitStaleMessage = (id, text) => emitStale(id, "message", text);
  P1.emitStaleError = (id) => emitStale(id, "error", null);
  const nativeFetch = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    if (P1.forceStatus401 && url.includes("/api/status")) return nativeFetch("/api/status401", init);
    return nativeFetch(input, init);
  };
})();
"""
TERMINAL_WRAP = r"""
<script>
(function () {
  var Native = window.Terminal;
  if (typeof Native !== "function") throw new Error("xterm Terminal missing");
  function RecordedTerminal(options) {
    var instance = new Native(options);
    (window.__p1.terminals = window.__p1.terminals || []).push(instance);
    return instance;
  }
  RecordedTerminal.prototype = Native.prototype;
  Object.setPrototypeOf(RecordedTerminal, Native);
  window.Terminal = RecordedTerminal;
})();
</script>
"""
class State:
    def __init__(self):
        self.authenticated, self.status401_seen = True, 0
def extract_html(repo):
    html = ui_harness.extract_html(repo)
    marker = '<script src="/assets/xterm-fit.js"></script>'
    if marker not in html:
        raise RuntimeError("xterm-fit script marker not found")
    return html.replace(marker, marker + TERMINAL_WRAP, 1)
def snapshot(page):
    return page.evaluate("""() => {
      const terms = window.__p1 && window.__p1.terminals || [];
      const t = terms[terms.length - 1];
      if (!t) return {text: "", host: "", error: "no terminal instance"};
      try {
        const b = t.buffer.active; let lines = [];
        for (let i = 0; i < b.length; i++) {
          const line = b.getLine(i); lines.push(line ? line.translateToString(true) : "");
        }
        return {text: lines.join("\\n"),
                host: (document.querySelector("#terminal-host .xterm-rows") || {}).innerText || ""};
      } catch (e) { return {text: "", host: "", error: String(e)}; }
    }""")
def buffer_has(page, text):
    snap = snapshot(page)
    return text in (snap.get("text", "") + "\n" + snap.get("host", ""))
def wait_buffer(page, text, present, timeout_ms=4000):
    deadline = time.monotonic() + timeout_ms / 1000.0
    while time.monotonic() < deadline:
        if buffer_has(page, text) == present:
            return True
        page.wait_for_timeout(50)
    return False
def socket_info(page):
    return page.evaluate("""() => ({
      count: window.__p1.sockets.length, closes: window.__p1.closes.slice(),
      states: window.__p1.sockets.map(s => s.readyState), terminals: window.__p1.terminals.length
    })""")
def check(checks, condition, name, evidence=""):
    ui_harness.check(checks, condition, name, evidence, limit=512)
def api_route(route, state):
    path, headers = urllib.parse.urlsplit(route.request.url).path, {"content-type": "application/json"}
    if path == "/api/bootstrap":
        payload = dict(BOOTSTRAP); payload["authenticated"] = state.authenticated
        route.fulfill(status=200, headers=headers, body=json.dumps(payload))
    elif path == "/api/logout":
        state.authenticated = False
        route.fulfill(status=200, headers=headers, body='{"ok":true}')
    elif path == "/api/login":
        state.authenticated = True
        route.fulfill(status=200, headers=headers, body='{"ok":true}')
    elif path == "/api/status401":
        state.status401_seen += 1
        route.fulfill(status=401, headers=headers, body='{"error":"session expired"}')
    elif path == "/api/status":
        route.fulfill(status=200, headers=headers, body=json.dumps(STATUS))
    elif path == "/api/passkey/list":
        route.fulfill(status=200, headers=headers, body='{"passkeys":[]}')
    elif path == "/api/devices/scan":
        route.fulfill(status=200, headers=headers, body='{"devices":{}}')
    elif path == "/api/input/release-all":
        route.fulfill(status=200, headers=headers, body='{"ok":true}')
    else:  # unmocked: an unknown route, and a suite failure (AUD3-33)
        ui_harness.answer_unmocked(route, path)
def wait_login(page, timeout=7000):
    page.wait_for_selector("#login-form:not(.hidden)", state="visible", timeout=timeout)
def main():
    args = ui_harness.parse_args(default_channel=None)
    summary = ui_harness.new_summary(args.repo)
    checks = summary["checks"]
    page = None
    try:
        state = State()
        with static_server(args.repo, extract_html(args.repo)) as port, sync_playwright() as p:
            browser = ui_harness.launch_browser(p, args.browser_channel)
            try:
                page = browser.new_context(viewport={"width": 1280, "height": 900}).new_page()
                page.add_init_script(WS_INIT)
                page.route("**/api/**", lambda route: api_route(route, state))
                mock_video_feed(page)
                page.goto(f"http://127.0.0.1:{port}/", wait_until="load", timeout=15000)
                page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=7000)
                check(checks, True, "authenticated_bootstrap")
                page.click('button[data-workspace="terminal"]')
                page.wait_for_function("() => window.__p1 && window.__p1.sockets.length === 1", timeout=5000)
                page.wait_for_function("() => window.__p1.sockets[0].readyState === 1", timeout=3000)
                check(checks, True, "terminal_tab_opens_socket")
                marker1 = "P1_INITIAL_TERMINAL_OUTPUT_1421"
                page.evaluate("(m) => window.__p1.emitMessage(0, m)", marker1)
                check(checks, wait_buffer(page, marker1, True), "synthetic_output_renders", snapshot(page))
                page.click("#logout"); wait_login(page); page.wait_for_timeout(200)
                info = socket_info(page)
                check(checks, 0 in info["closes"], "logout_closes_socket", info)
                check(checks, wait_buffer(page, marker1, False), "logout_clears_terminal", snapshot(page))
                page.evaluate("() => { window.__p1.emitStaleMessage(0,'P1_STALE_AFTER_LOGOUT_1421'); window.__p1.emitStaleError(0); }")
                page.wait_for_timeout(250)
                snap = snapshot(page)
                check(checks, "P1_STALE_AFTER_LOGOUT_1421" not in snap.get("text", "") + snap.get("host", "")
                      and "终端连接失败" not in snap.get("text", ""), "stale_callbacks_silent", snap)
                page.fill('#login-form input[name="username"]', USER)
                page.fill('#login-form input[name="password"]', PASSWORD)
                page.click('#login-form button[type="submit"]')
                page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=7000)
                page.click('button[data-workspace="terminal"]')
                page.wait_for_function("() => window.__p1.sockets.length >= 2", timeout=5000)
                page.wait_for_function("() => window.__p1.sockets[1].readyState === 1", timeout=3000)
                info = socket_info(page)
                check(checks, info["count"] >= 2 and info["states"][1] == 1,
                      "relogin_opens_new_socket", info)
                marker2 = "P1_RELOGIN_TERMINAL_OUTPUT_1421"
                page.evaluate("(m) => window.__p1.emitMessage(1, m)", marker2)
                check(checks, wait_buffer(page, marker2, True), "relogin_output_renders", snapshot(page))
                page.evaluate("() => { window.__p1.forceStatus401 = true; }")
                wait_login(page, timeout=8000)
                check(checks, state.status401_seen >= 1, "status401_requested", state.status401_seen)
                info = socket_info(page)
                check(checks, 1 in info["closes"], "status401_closes_socket", info)
                check(checks, wait_buffer(page, marker2, False), "status401_clears_terminal", snapshot(page))
                page.evaluate("() => { window.__p1.emitStaleMessage(1,'P1_STALE_AFTER_401_1421'); window.__p1.emitStaleError(1); }")
                page.wait_for_timeout(250)
                snap = snapshot(page)
                check(checks, "P1_STALE_AFTER_401_1421" not in snap.get("text", "") + snap.get("host", "")
                      and "终端连接失败" not in snap.get("text", ""), "status401_stale_callbacks_silent", snap)
                summary["status"] = "PASS"
                ui_harness.screenshot(page, args.output_dir, "p1_ui_pass.png")
            finally:
                browser.close()
    except Exception as exc:
        summary["error"] = f"{type(exc).__name__}: {exc}"
        ui_harness.screenshot(page, args.output_dir, "p1_ui_failure.png")
    return ui_harness.finish(summary, args.output_dir, "p1_ui_report.json")
if __name__ == "__main__":
    sys.exit(main())
