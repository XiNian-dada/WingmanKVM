"""Mocked device state shared by the README screenshot and demo generators.

capture_screenshots.py and capture_demo.py serve the real UI bundle through
ui_harness.static_server() and answer every /api/** call (plus the input
WebSocket and /video_feed) from the fixtures below, so no device is ever
contacted. Everything that can drift between runs is pinned: viewport, device
scale factor, locale, time zone, the browser clock and the synthetic frame.
"""
import datetime, json, os, sys, urllib.parse
from xml.sax.saxutils import escape

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ui_harness  # noqa: E402

REPO = ui_harness.REPO
DOCS_IMG_DIR = os.path.join(REPO, "docs", "images")

# 2026-10-18 20:45:12 CST, the same moment the synthetic frame shows.
FIXED_TIME = datetime.datetime(2026, 10, 18, 12, 45, 12, tzinfo=datetime.timezone.utc)
TIMEZONE = "Asia/Shanghai"
LOCALES = {"zh-CN": "zh-CN", "zh-TW": "zh-TW", "en": "en-US"}

DEVICES = {
    "video": [{"path": "/dev/video0", "label": "HDMI to USB Capture (1080p 60fps MJPEG)", "kind": "video",
               "warnings": [], "video_capture": True, "supports_mjpeg": True}],
    "hid": [
        {"path": "/dev/hidg0", "label": "USB Keyboard Gadget", "kind": "keyboard", "warnings": [],
         "gadget_bound": True, "function_linked": True, "compatible": True},
        {"path": "/dev/hidg1", "label": "USB Relative Mouse Gadget", "kind": "mouse", "warnings": [],
         "gadget_bound": True, "function_linked": True, "compatible": True},
        {"path": "/dev/hidg2", "label": "USB Absolute Pointer Gadget", "kind": "absolute_pointer", "warnings": [],
         "gadget_bound": True, "function_linked": True, "compatible": True},
    ],
    "keyboard": [{"path": "/dev/hidg0", "label": "USB Keyboard Gadget", "kind": "keyboard", "warnings": [],
                  "gadget_bound": True, "function_linked": True, "compatible": True}],
    "mouse": [{"path": "/dev/hidg1", "label": "USB Relative Mouse Gadget", "kind": "mouse", "warnings": [],
               "gadget_bound": True, "function_linked": True, "compatible": True}],
    "absolute_pointer": [{"path": "/dev/hidg2", "label": "USB Absolute Pointer Gadget", "kind": "absolute_pointer",
                          "warnings": [], "gadget_bound": True, "function_linked": True, "compatible": True}],
    "gpio": [{"path": "/dev/gpiochip1", "label": "RK3399 GPIO Chip 1 (Line 7: Power SW Relay)", "kind": "gpio",
              "warnings": []}],
    "mass_storage_luns": [{"path": "/sys/kernel/config/usb_gadget/wingmankvm/functions/mass_storage.usb0/lun.0",
                           "label": "USB Mass Storage Gadget LUN 0", "kind": "mass_storage_lun", "warnings": []}],
}

MEDIA = {
    "images": [
        {"name": "ubuntu-24.04-live-server-arm64.iso", "size": 2831155200, "mounted": True},
        {"name": "debian-12-netinst.iso", "size": 654311424, "mounted": False},
        {"name": "recovery-tools.img", "size": 104857600, "mounted": False},
    ],
    "status": {
        "state": "attached", "mounted_path": "/var/lib/wingmankvm/images/ubuntu-24.04-live-server-arm64.iso",
        "mounted_name": "ubuntu-24.04-live-server-arm64.iso", "media_type": "cdrom", "read_only": True,
        "cdrom": True, "removable": True, "forced_eject_supported": False,
    },
}

SYSTEM = {
    "cpu": {"percent": 24.5, "cores": 6, "frequency_mhz": 1800},
    "load_avg": [0.62, 0.58, 0.45],
    "memory": {"total_bytes": 4096000000, "used_bytes": 1943040000, "free_bytes": 2152960000, "percent": 47.4},
    "disk": {"total_bytes": 30601641984, "used_bytes": 15243820992, "free_bytes": 15357820992, "percent": 49.8},
    "temperature": 51.5,
    "uptime_seconds": 273600,
}

VIDEO = {"state": "ready", "message": None, "device": "/dev/video0", "width": 1920, "height": 1080,
         "frames_per_second": 60.0, "encoding": "mjpeg_passthrough", "last_frame_unix_ms": None, "last_frame_age_ms": 16}

CLIENT = {"ip": "192.168.1.100", "port": 54321, "source": "socket"}

CAPABILITIES = {
    "video": True, "keyboard": True, "mouse": True, "mouse_relative": True, "mouse_absolute": True,
    "pointer_mode": "absolute", "gpio_power": True, "gpio_reset": True, "gpio_power_led": True,
    "mass_storage": True, "video_passthrough": True, "video_transcode": False, "video_webrtc_h264": False,
    "terminal": True,
}

CONFIG = {
    "version": 1,
    "server": {"listen_address": "0.0.0.0", "port": 8080, "proxy_protocol": "disabled", "trusted_http_proxies": []},
    "video": {"device": "/dev/video0", "width": 1920, "height": 1080, "frames_per_second": 60,
              "jpeg_quality": 85, "encoding": "mjpeg_passthrough"},
    "hid": {"keyboard_device": "/dev/hidg0", "mouse_device": "/dev/hidg1",
            "absolute_pointer_device": "/dev/hidg2", "pointer_mode": "absolute"},
    "power": {"enabled": True, "gpio_chip": "/dev/gpiochip1", "gpio_line": 7, "active_high": True},
    "media": {"enabled": True,
              "lun_path": "/sys/kernel/config/usb_gadget/wingmankvm/functions/mass_storage.usb0/lun.0",
              "image_directory": "/var/lib/wingmankvm/images"},
}

BOOTSTRAP_CONSOLE = {
    "setup_required": False, "token_required": False, "authenticated": True, "has_passkeys": True,
    "terminal_ready": True, "username": "admin", "client_ip": CLIENT["ip"], "client": CLIENT,
    "config": CONFIG, "video": VIDEO, "webrtc": {"available": False, "reason": "MJPEG 演示"},
    "capabilities": CAPABILITIES,
}

BOOTSTRAP_SETUP = {
    "setup_required": True, "token_required": True, "authenticated": False, "has_passkeys": False,
    "terminal_ready": False, "client_ip": CLIENT["ip"], "client": CLIENT,
}

STATUS = {
    "video": VIDEO,
    "power": {"power_led": {"configured": True, "state": "on", "sense_error": None}},
    "webrtc": {"available": False, "sessions": 0},
    "capabilities": CAPABILITIES,
    "hid": {"keyboard_leds": {"num_lock": True, "caps_lock": False, "scroll_lock": False, "raw": 1},
            "keyboard_reenumerating": False, "mouse_reenumerating": False},
    "system": SYSTEM,
    "runtime": {"version": "0.1.0", "uptime_seconds": 273600, "input_sockets": 1,
                "hid": {"ok": 1284, "failed": 0, "last_error": None, "last_error_age_seconds": None}},
    "notifications": {"dropped": 0},
    "client": dict(CLIENT, proxy_protocol="disabled"),
}

PASSKEYS = {"passkeys": [{"id": "macbook-touch-id", "name": "MacBook Pro (Touch ID)",
                          "created_at_unix_seconds": 1792909200}]}

GREEN, BLUE, GREY, TEXT, WHITE = "#3fb950", "#58a6ff", "#8b949e", "#c9d1d9", "#f0f6fc"
PROMPT = [(GREEN, "ubuntu@wingman-node"), (TEXT, ":"), (BLUE, "~"), (TEXT, "$ ")]
SCREEN_LINES = [
    (50, [(BLUE, "Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-40-generic x86_64)")]),
    (85, [(GREY, " * Documentation:  https://help.ubuntu.com")]),
    (110, [(GREY, " * Management:     https://landscape.canonical.com")]),
    (135, [(GREY, " * Support:        https://ubuntu.com/pro")]),
    (180, [(GREEN, "[  OK  ] Started OpenSSH server daemon.")]),
    (210, [(GREEN, "[  OK  ] Reached target Multi-User System.")]),
    (240, [(GREEN, "[  OK  ] Reached target Graphical Interface.")]),
    (295, [(TEXT, "wingman-node login: "), (BLUE, "ubuntu")]),
    (325, [(TEXT, "Password: ")]),
    (375, [(GREY, "Last login: Sun Oct 18 19:42:01 2026 from 192.168.1.20 on tty1")]),
    (415, PROMPT + [(WHITE, "systemctl status wingmankvm-node")]),
    (450, [(GREEN, "\u25cf wingmankvm-node.service - Target Node Agent")]),
    (475, [(TEXT, "     Loaded: loaded (/lib/systemd/system/wingmankvm-node.service; enabled)")]),
    (500, [(TEXT, "     Active: "), (GREEN, "active (running)"), (GREY, " since Sun 2026-10-18 19:40:12 CST; 1h 05min ago")]),
    (525, [(TEXT, "   Main PID: 812 (agent)")]),
    (550, [(TEXT, "      Tasks: 4 (limit: 18889)")]),
]


def screen_svg(typed=""):
    """Synthetic 1280x720 target screen; `typed` appears at the last prompt."""
    lines = SCREEN_LINES + [(600, PROMPT + [(WHITE, typed), (BLUE, "\u2588")])]
    rows = []
    for y, spans in lines:
        body = "".join(f'<tspan fill="{color}">{escape(text)}</tspan>' for color, text in spans)
        rows.append(f'    <text x="30" y="{y}" xml:space="preserve">{body}</text>')
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">\n'
            '  <rect width="1280" height="720" fill="#0d1117"/>\n'
            '  <g font-family="Menlo, \'DejaVu Sans Mono\', ui-monospace, monospace" font-size="16">\n'
            + "\n".join(rows) + "\n  </g>\n</svg>")


# Freeze the few nondeterministic inputs the UI reads besides the clock.
INIT_SCRIPT = """
(() => {
  let seed = 42;
  Math.random = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
  // Record every error toast so a capture can refuse to publish it.
  window.__errorToasts = [];
  document.addEventListener('DOMContentLoaded', () => {
    const toast = document.querySelector('#toast');
    if (!toast) return;
    new MutationObserver(() => {
      if (toast.classList.contains('show') && toast.style.color) window.__errorToasts.push(toast.textContent);
    }).observe(toast, {attributes: true, childList: true, characterData: true, subtree: true});
  });
})();
"""


class Target:
    """The mocked target machine: its screen and the /api/input/ws HID socket.

    Every HID message is acknowledged; `hold_at` holds back the reply to the
    n-th key so a capture can freeze a progress bar at an exact position.
    """

    def __init__(self, frames):
        self.frames = frames      # typed text -> PNG, see render_screens()
        self.typed = ""
        self.messages = []
        self.hold_at = None
        self.held = None

    def attach(self, page):
        page.route("**/video_feed*", lambda route: route.fulfill(
            status=200, content_type="image/png", body=self.frames[self.typed]))
        page.route_web_socket("**/api/input/ws", self._connect)

    def _connect(self, ws):
        def on_message(raw):
            try:
                msg = json.loads(raw)
            except Exception:
                return
            self.messages.append(msg)
            reply = json.dumps({"id": msg.get("id"), "ok": True})
            if msg.get("type") == "key" and self.hold_at == self.keys():
                self.held = (ws, reply)
                return
            ws.send(reply)
        ws.on_message(on_message)

    def keys(self):
        return sum(1 for m in self.messages if m.get("type") == "key")

    def release(self):
        self.hold_at = None
        if self.held:
            ws, reply = self.held
            self.held = None
            ws.send(reply)


def api_route(route, bootstrap):
    path = urllib.parse.urlsplit(route.request.url).path
    payload = {
        "/api/bootstrap": bootstrap,
        "/api/status": STATUS,
        "/api/system": SYSTEM,
        "/api/setup/devices": DEVICES,
        "/api/devices/scan": DEVICES,
        "/api/media": MEDIA,
        "/api/passkey/list": PASSKEYS,
        "/api/hid/leds": STATUS["hid"]["keyboard_leds"],
        "/api/sessions": {"sessions": []},
        "/api/audit": {"entries": []},
        "/api/update/status": {"state": "idle"},
    }.get(path, {"ok": True})
    route.fulfill(status=200, content_type="application/json", body=json.dumps(payload))


_SCREEN_CACHE = {}


def render_screens(browser, texts=("",)):
    """Rasterize the synthetic screen to 1920x1080 PNGs, like a capture card's frames.

    A raster frame scales the same way on every run; an SVG <img> may be
    re-rasterized mid-layout and differ by a few anti-aliased pixels.
    """
    missing = [text for text in texts if text not in _SCREEN_CACHE]
    if missing:
        context = browser.new_context(viewport={"width": 1280, "height": 720}, device_scale_factor=1.5)
        try:
            page = context.new_page()
            for text in missing:
                page.set_content('<body style="margin:0">' + screen_svg(text) + "</body>")
                page.evaluate("() => document.fonts.ready")
                _SCREEN_CACHE[text] = page.screenshot()
        finally:
            context.close()
    return {text: _SCREEN_CACHE[text] for text in texts}


def new_page(browser, port, *, lang="zh-CN", theme="light", viewport=(1440, 900), scale=1,
             setup=False, path="/", storage=None, screens=("",)):
    """Open the console (or the setup wizard) against the mocked API.

    `screens` lists every text the demo will show at the target's prompt, so
    each frame is rendered before the page starts requesting /video_feed.
    """
    frames = render_screens(browser, screens)
    context = browser.new_context(
        viewport={"width": viewport[0], "height": viewport[1]}, device_scale_factor=scale,
        locale=LOCALES.get(lang, lang), timezone_id=TIMEZONE, color_scheme=theme,
        reduced_motion="no-preference")
    # Smooth scaling: the 1080p frame is shown downscaled, where nearest-neighbour
    # (the UI default) would make the synthetic terminal text look broken.
    values = {"wingman_lang": lang, "wingman_theme": theme, "wingman_setting_render_pixelated": "false"}
    values.update(storage or {})
    context.add_init_script(INIT_SCRIPT + "\n" + "".join(
        f"localStorage.setItem({json.dumps(k)}, {json.dumps(v)});\n" for k, v in values.items()))
    page = context.new_page()
    page.clock.set_fixed_time(FIXED_TIME)
    bootstrap = BOOTSTRAP_SETUP if setup else BOOTSTRAP_CONSOLE
    page.route("**/api/**", lambda route: api_route(route, bootstrap))
    target = Target(frames)
    target.attach(page)
    page.goto(f"http://127.0.0.1:{port}{path}", wait_until="load", timeout=15000)
    if setup:
        page.wait_for_selector("#setup-form:not(.hidden)", timeout=8000)
    else:
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.wait_for_function(
            "() => document.querySelector('#status-dot')?.classList.contains('online')", timeout=8000)
    page.evaluate("() => document.fonts.ready")
    page.wait_for_timeout(300)
    return context, page, target


# Software raster with a fixed colour profile keeps anti-aliasing identical
# between runs, so regenerated images only change when the UI does.
LAUNCH_ARGS = ["--disable-gpu", "--disable-gpu-rasterization", "--disable-partial-raster",
               "--force-color-profile=srgb", "--disable-lcd-text", "--font-render-hinting=none"]


def launch(playwright, channel):
    return playwright.chromium.launch(headless=True, channel=channel or None, args=LAUNCH_ARGS)


def assert_clean(page, label):
    """Fail if an error toast was shown or the stylesheet did not apply."""
    state = page.evaluate("""() => ({
      errors: window.__errorToasts || [],
      styled: getComputedStyle(document.documentElement).getPropertyValue('--ink').trim() !== '',
    })""")
    if state["errors"]:
        raise RuntimeError(f"{label}: error toast shown: {state['errors']}")
    if not state["styled"]:
        raise RuntimeError(f"{label}: stylesheet not applied")
