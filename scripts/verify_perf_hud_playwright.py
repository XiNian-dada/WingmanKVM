import os
from ui_bundle import MODULE_TYPE, load_index_html, module_source
import json
import base64
from playwright.sync_api import sync_playwright

WORKSPACE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VENDOR_DIR = os.path.join(WORKSPACE, "web", "vendor", "xterm")
ARTIFACT_DIR = "/Users/bernard/.gemini/antigravity/brain/2392f8dc-b668-4683-9c0c-064ce20f152d"

os.makedirs(ARTIFACT_DIR, exist_ok=True)

raw_html = load_index_html(WORKSPACE)

with open(os.path.join(VENDOR_DIR, "xterm.css"), "r", encoding="utf-8") as f:
    xterm_css = f.read()
with open(os.path.join(VENDOR_DIR, "xterm.js"), "r", encoding="utf-8") as f:
    xterm_js = f.read()
with open(os.path.join(VENDOR_DIR, "addon-fit.js"), "r", encoding="utf-8") as f:
    xterm_fit_js = f.read()

mock_metrics = {
    "cpu": {
        "percent": 24.5,
        "cores": 4,
        "frequency_mhz": 1800
    },
    "load_avg": [0.62, 0.58, 0.45],
    "memory": {
        "total_bytes": 4096000000,
        "used_bytes": 1943040000,
        "free_bytes": 2152960000,
        "percent": 47.4
    },
    "disk": {
        "total_bytes": 30601641984,
        "used_bytes": 15243820992,
        "free_bytes": 15357820992,
        "percent": 49.8
    },
    "temperature": 51.5,
    "uptime_seconds": 273600
}

mock_bootstrap_main = {
    "setup_required": False,
    "token_required": False,
    "authenticated": True,
    "has_passkeys": True,
    "client_ip": "192.168.1.100",
    "client": {
        "ip": "192.168.1.100",
        "port": 54321,
        "source": "socket"
    },
    "config": {
        "version": 1,
        "server": {"listen_address": "0.0.0.0", "port": 8080, "proxy_protocol": "auto", "trusted_http_proxies": []},
        "video": {"device": "/dev/video0", "width": 1920, "height": 1080, "frames_per_second": 60, "jpeg_quality": 85},
        "hid": {"keyboard_device": "/dev/hidg0", "mouse_device": "/dev/hidg1", "absolute_pointer_device": "/dev/hidg2", "pointer_mode": "absolute"},
        "power": {"enabled": True, "gpio_chip": "/dev/gpiochip1", "gpio_line": 7, "active_high": True},
        "media": {"enabled": True, "lun_path": "/sys/kernel/config/usb_gadget/wingmankvm/functions/mass_storage.usb0/lun.0", "image_directory": "/var/lib/wingmankvm/images"}
    },
    "video": {
        "state": "ready",
        "message": "",
        "frames_per_second": 60.0,
        "width": 1920,
        "height": 1080
    },
    "webrtc": {"available": True, "reason": ""},
    "capabilities": {
        "video": True,
        "hid": True,
        "power": True,
        "mass_storage": True,
        "terminal": True
    }
}

mock_status = {
    "video": {
        "state": "ready",
        "message": "",
        "frames_per_second": 60.0,
        "width": 1920,
        "height": 1080
    },
    "power": {
        "power_led": {"configured": True, "state": "on", "sense_error": None}
    },
    "hid": {
        "keyboard_leds": {"num_lock": True, "caps_lock": False, "scroll_lock": False, "raw": 1}
    },
    "client": {
        "ip": "192.168.1.100",
        "port": 54321,
        "source": "socket",
        "proxy_protocol": "auto"
    },
    "system": mock_metrics
}

screen_svg = '''
<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">
  <rect width="1280" height="720" fill="#0d1117"/>
  <g font-family="Geist Mono, ui-monospace, SFMono-Regular, Menlo, monospace" font-size="16" fill="#c9d1d9">
    <text x="30" y="50" fill="#58a6ff" font-weight="bold">Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-40-generic x86_64)</text>
    <text x="30" y="85" fill="#8b949e"> * Documentation:  https://help.ubuntu.com</text>
    <text x="30" y="110" fill="#8b949e"> * Management:     https://landscape.canonical.com</text>
    <text x="30" y="135" fill="#8b949e"> * Support:        https://ubuntu.com/pro</text>
    
    <text x="30" y="180" fill="#3fb950">[  OK  ] Started OpenSSH server daemon.</text>
    <text x="30" y="210" fill="#3fb950">[  OK  ] Reached target Multi-User System.</text>
    <text x="30" y="240" fill="#3fb950">[  OK  ] Reached target Graphical Interface.</text>
    
    <text x="30" y="295" fill="#e6edf3">wingman-node login: </text>
    <text x="200" y="295" fill="#58a6ff">ubuntu</text>
    <text x="30" y="325" fill="#e6edf3">Password: </text>
    
    <text x="30" y="375" fill="#8b949e">Last login: Fri Oct 23 20:30:00 2026 from 192.168.1.100 on tty1</text>
    <text x="30" y="415" fill="#3fb950">ubuntu@wingman-node</text>
    <text x="210" y="415" fill="#c9d1d9">:</text>
    <text x="220" y="415" fill="#58a6ff">~</text>
    <text x="230" y="415" fill="#c9d1d9">$ </text>
    <text x="250" y="415" fill="#f0f6fc">systemctl status wingmankvm-node</text>
    
    <text x="30" y="450" fill="#3fb950">&#x25CF; wingmankvm-node.service - Target Node Agent</text>
    <text x="50" y="475" fill="#c9d1d9">   Loaded: loaded (/lib/systemd/system/wingmankvm-node.service; enabled)</text>
    <text x="50" y="500" fill="#3fb950">   Active: active (running) </text>
    <text x="250" y="500" fill="#8b949e">since Fri 2026-10-23 19:40:12 CST</text>
    
    <text x="30" y="580" fill="#3fb950">ubuntu@wingman-node</text>
    <text x="210" y="580" fill="#c9d1d9">:</text>
    <text x="220" y="580" fill="#58a6ff">~</text>
    <text x="230" y="580" fill="#c9d1d9">$ </text>
    <rect x="250" y="566" width="10" height="18" fill="#58a6ff"/>
  </g>
</svg>
'''
screen_data_url = "data:image/svg+xml;base64," + base64.b64encode(screen_svg.encode('utf-8')).decode('utf-8')

LAUNCH_ARGS = [
    "--single-process",
    "--no-sandbox",
    "--disable-gpu",
    "--force-device-scale-factor=2"
]
CHROME_PATH = "/Users/bernard/Library/Caches/ms-playwright/chromium_headless_shell-1247/chrome-headless-shell-mac-arm64/chrome-headless-shell"

def setup_routes(page):
    def route_main(route):
        url = route.request.url
        if module_source(url) is not None:
            route.fulfill(status=200, content_type=MODULE_TYPE, body=module_source(url))
        elif "/assets/xterm.css" in url:
            route.fulfill(status=200, content_type="text/css", body=xterm_css)
        elif "/assets/xterm-fit.js" in url:
            route.fulfill(status=200, content_type="application/javascript", body=xterm_fit_js)
        elif "/assets/xterm.js" in url:
            route.fulfill(status=200, content_type="application/javascript", body=xterm_js)
        elif "/assets/icon.svg" in url or "/favicon.ico" in url:
            route.fulfill(status=200, content_type="image/svg+xml", body='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 348 252"></svg>')
        elif "/api/bootstrap" in url:
            route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_bootstrap_main))
        elif "/api/status" in url:
            route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_status))
        elif "/api/system" in url:
            route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_metrics))
        elif "/api/passkeys" in url or "/api/media" in url:
            route.fulfill(status=200, content_type="application/json", body="[]")
        elif "/video_feed" in url:
            route.fulfill(status=200, content_type="image/svg+xml", body=screen_svg)
        elif url.endswith("/") or "index" in url:
            route.fulfill(status=200, content_type="text/html", body=raw_html)
        else:
            route.fulfill(status=200, body="")

    page.route("**/*", route_main)

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=CHROME_PATH, args=LAUNCH_ARGS)

    # 1. Wide Desktop Screen (1920x1080) - Dark Theme
    page = browser.new_page(viewport={"width": 1920, "height": 1080})
    setup_routes(page)
    page.goto("http://localhost/")
    page.wait_for_timeout(600)

    # Initialize video feed & metrics, set language to Traditional Chinese
    page.evaluate(f"""() => {{
        const feed = document.querySelector('#video-feed');
        if (feed) {{
            feed.src = '{screen_data_url}';
            feed.classList.remove('hidden');
        }}
        document.querySelector('#video-message')?.classList.add('hidden');
        const dot = document.querySelector('#status-dot');
        if (dot) dot.className = 'status-dot online';
        const statusLabel = document.querySelector('#status-label');
        if (statusLabel) statusLabel.textContent = 'MJPEG · 60 FPS';
        const transLabel = document.querySelector('#video-transport-label');
        if (transLabel) transLabel.textContent = 'LIVE';
        if (typeof setLanguage === 'function') setLanguage('zh-TW');
        if (typeof renderSystemMetrics === 'function') renderSystemMetrics({json.dumps(mock_metrics)});
    }}""")
    page.wait_for_timeout(400)

    # Capture 1: Wide Desktop showing the sleek HUD chip in the main topbar alongside MJPEG 60FPS
    wide_hud_path = os.path.join(ARTIFACT_DIR, "verified_perf_hud_wide.png")
    page.screenshot(path=wide_hud_path)
    print(f"Captured: {wide_hud_path}")

    # Capture 2: Click the topbar perf button to pop out the NAS Resource Monitor widget
    page.click("#topbar-perf-btn")
    page.wait_for_timeout(400)
    popover_open_path = os.path.join(ARTIFACT_DIR, "verified_perf_popover_open.png")
    page.screenshot(path=popover_open_path)
    print(f"Captured: {popover_open_path}")

    # Close popover
    page.click("#perf-pop-close")
    page.wait_for_timeout(300)

    # 2. Narrow Screen View (760x900) - Compact chip in topbar
    page.set_viewport_size({"width": 760, "height": 900})
    if page.query_selector('#app.inspector-open'):
        page.click('#inspector-close')
        page.wait_for_timeout(400)
    page.wait_for_timeout(200)

    # Capture 3: Narrow screen collapsed chip in topbar
    narrow_collapsed_path = os.path.join(ARTIFACT_DIR, "verified_perf_hud_narrow_collapsed.png")
    page.screenshot(path=narrow_collapsed_path)
    print(f"Captured: {narrow_collapsed_path}")

    # Capture 4: Click the collapsed button in narrow screen to pop out the widget
    page.click("#topbar-perf-btn")
    page.wait_for_timeout(400)
    narrow_open_path = os.path.join(ARTIFACT_DIR, "verified_perf_popover_narrow_open.png")
    page.screenshot(path=narrow_open_path)
    print(f"Captured: {narrow_open_path}")

    browser.close()
    print("All screenshots successfully captured!")
