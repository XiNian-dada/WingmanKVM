import os
import glob
import json
import base64
from playwright.sync_api import sync_playwright

WORKSPACE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC_WEB_UI = os.path.join(WORKSPACE, "src", "web_ui.rs")
VENDOR_DIR = os.path.join(WORKSPACE, "web", "vendor", "xterm")
DOCS_IMG_DIR = os.path.join(WORKSPACE, "docs", "images")

os.makedirs(DOCS_IMG_DIR, exist_ok=True)

# 1. 提取 HTML 与静态文件
with open(SRC_WEB_UI, "r", encoding="utf-8") as f:
    rust_src = f.read()

start_marker = 'pub static INDEX_HTML: &str = r##"'
end_marker = '"##;'
s_idx = rust_src.index(start_marker) + len(start_marker)
e_idx = rust_src.index(end_marker, s_idx)
raw_html = rust_src[s_idx:e_idx]

with open(os.path.join(VENDOR_DIR, "xterm.css"), "r", encoding="utf-8") as f:
    xterm_css = f.read()
with open(os.path.join(VENDOR_DIR, "xterm.js"), "r", encoding="utf-8") as f:
    xterm_js = f.read()
with open(os.path.join(VENDOR_DIR, "addon-fit.js"), "r", encoding="utf-8") as f:
    xterm_fit_js = f.read()

# 准备 Mock 设备扫描数据 (Firefly RK3399 典型已配置状态)
mock_devices = {
    "video": [{
        "path": "/dev/video0",
        "label": "HDMI to USB Capture (1080p 60fps MJPEG)",
        "kind": "video",
        "warnings": [],
        "video_capture": True,
        "supports_mjpeg": True
    }],
    "hid": [
        {
            "path": "/dev/hidg0",
            "label": "USB Keyboard Gadget",
            "kind": "keyboard",
            "warnings": [],
            "gadget_bound": True,
            "function_linked": True,
            "compatible": True
        },
        {
            "path": "/dev/hidg1",
            "label": "USB Relative Mouse Gadget",
            "kind": "mouse",
            "warnings": [],
            "gadget_bound": True,
            "function_linked": True,
            "compatible": True
        },
        {
            "path": "/dev/hidg2",
            "label": "USB Absolute Pointer Gadget",
            "kind": "absolute_pointer",
            "warnings": [],
            "gadget_bound": True,
            "function_linked": True,
            "compatible": True
        }
    ],
    "keyboard": [{
        "path": "/dev/hidg0",
        "label": "USB Keyboard Gadget",
        "kind": "keyboard",
        "warnings": [],
        "gadget_bound": True,
        "function_linked": True,
        "compatible": True
    }],
    "mouse": [{
        "path": "/dev/hidg1",
        "label": "USB Relative Mouse Gadget",
        "kind": "mouse",
        "warnings": [],
        "gadget_bound": True,
        "function_linked": True,
        "compatible": True
    }],
    "absolute_pointer": [{
        "path": "/dev/hidg2",
        "label": "USB Absolute Pointer Gadget",
        "kind": "absolute_pointer",
        "warnings": [],
        "gadget_bound": True,
        "function_linked": True,
        "compatible": True
    }],
    "gpio": [{
        "path": "/dev/gpiochip1",
        "label": "RK3399 GPIO Chip 1 (Line 7: Power SW Relay)",
        "kind": "gpio",
        "warnings": []
    }],
    "mass_storage_luns": [{
        "path": "/sys/kernel/config/usb_gadget/wingmankvm/functions/mass_storage.usb0/lun.0",
        "label": "USB Mass Storage Gadget LUN 0",
        "kind": "mass_storage_lun",
        "warnings": []
    }]
}

mock_media_list = {
    "images": [
        {"name": "ubuntu-24.04-live-server-arm64.iso", "size": 2831155200, "kind": "iso"},
        {"name": "debian-12-netinst.iso", "size": 654311424, "kind": "iso"},
        {"name": "recovery-tools.img", "size": 104857600, "kind": "img"}
    ],
    "current": {
        "file": "ubuntu-24.04-live-server-arm64.iso",
        "read_only": True,
        "cdrom": True
    }
}

mock_system = {
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
    "system": mock_system
}

mock_passkeys = [
    {"id": "macbook-touch-id", "name": "MacBook Pro (Touch ID)", "created_at": "2026-10-25T14:20:00Z"}
]

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
    
    <text x="30" y="375" fill="#8b949e">Last login: Sun Oct 18 19:42:01 2026 from 192.168.1.20 on tty1</text>
    <text x="30" y="415" fill="#3fb950">ubuntu@wingman-node</text>
    <text x="210" y="415" fill="#c9d1d9">:</text>
    <text x="220" y="415" fill="#58a6ff">~</text>
    <text x="230" y="415" fill="#c9d1d9">$ </text>
    <text x="250" y="415" fill="#f0f6fc">systemctl status wingmankvm-node</text>
    
    <text x="30" y="450" fill="#3fb950">&#x25CF; wingmankvm-node.service - Target Node Agent</text>
    <text x="50" y="475" fill="#c9d1d9">   Loaded: loaded (/lib/systemd/system/wingmankvm-node.service; enabled)</text>
    <text x="50" y="500" fill="#3fb950">   Active: active (running) </text>
    <text x="250" y="500" fill="#8b949e">since Sun 2026-10-18 19:40:12 CST; 1h 05min ago</text>
    <text x="50" y="525" fill="#c9d1d9">   Main PID: 812 (agent)</text>
    <text x="50" y="550" fill="#c9d1d9">   Tasks: 4 (limit: 18889)</text>
    
    <text x="30" y="600" fill="#3fb950">ubuntu@wingman-node</text>
    <text x="210" y="600" fill="#c9d1d9">:</text>
    <text x="220" y="600" fill="#58a6ff">~</text>
    <text x="230" y="600" fill="#c9d1d9">$ </text>
    <rect x="250" y="586" width="10" height="18" fill="#58a6ff"/>
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
chrome_paths = sorted(glob.glob("/Users/bernard/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-arm64/chrome-headless-shell"))
CHROME_PATH = chrome_paths[-1] if chrome_paths else None

def capture_setup_flow():
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=CHROME_PATH, args=LAUNCH_ARGS)
        page = browser.new_page(viewport={"width": 1280, "height": 820})

        def route_setup(route):
            url = route.request.url
            if "/assets/xterm.css" in url:
                route.fulfill(status=200, content_type="text/css", body=xterm_css)
            elif "/assets/xterm-fit.js" in url:
                route.fulfill(status=200, content_type="application/javascript", body=xterm_fit_js)
            elif "/assets/xterm.js" in url:
                route.fulfill(status=200, content_type="application/javascript", body=xterm_js)
            elif "/assets/icon.svg" in url or "/favicon.ico" in url:
                route.fulfill(status=200, content_type="image/svg+xml", body='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 348 252"></svg>')
            elif "/api/bootstrap" in url:
                route.fulfill(status=200, content_type="application/json", body=json.dumps({
                    "setup_required": True,
                    "token_required": True,
                    "authenticated": False,
                    "has_passkeys": False,
                    "client_ip": "192.168.1.100"
                }))
            elif "/api/setup/devices" in url:
                route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_devices))
            elif url.endswith("/") or "index" in url:
                route.fulfill(status=200, content_type="text/html", body=raw_html)
            else:
                route.fulfill(status=200, body="")

        page.route("**/*", route_setup)
        page.goto("http://localhost/#setup=quick-start-token")
        page.wait_for_timeout(400)

        # 1. 填写第一步并截图
        page.wait_for_selector('#setup-form:not(.hidden)')
        page.fill('#setup-form input[name="username"]', 'admin')
        page.fill('#setup-form input[name="password"]', 'Admin123456!@#')
        page.fill('#setup-form input[name="confirm_password"]', 'Admin123456!@#')
        page.wait_for_timeout(300)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "setup-account.png"))
        print("Generated setup-account.png")

        # 2. 点击下一步：检查连接
        page.click('#setup-form [data-setup-next]')
        page.wait_for_timeout(800)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "setup-detection.png"))
        print("Generated setup-detection.png")

        # 3. 展开硬件向导
        page.click('#setup-form [data-guide="video"]')
        page.wait_for_timeout(500)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "setup-guide.png"))
        print("Generated setup-guide.png")

        browser.close()

def capture_console():
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=CHROME_PATH, args=LAUNCH_ARGS)
        page = browser.new_page(viewport={"width": 1440, "height": 900})

        def route_main(route):
            url = route.request.url
            if "/assets/xterm.css" in url:
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
                route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_system))
            elif "/api/setup/devices" in url or "/api/devices/scan" in url:
                route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_devices))
            elif "/api/media/images" in url or "/api/media" in url:
                route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_media_list))
            elif "/api/passkeys" in url:
                route.fulfill(status=200, content_type="application/json", body=json.dumps(mock_passkeys))
            elif "/video_feed" in url:
                route.fulfill(status=200, content_type="image/svg+xml", body=screen_svg)
            elif url.endswith("/") or "index" in url:
                route.fulfill(status=200, content_type="text/html", body=raw_html)
            else:
                route.fulfill(status=200, body="")

        page.route("**/*", route_main)
        page.goto("http://localhost/")
        page.wait_for_timeout(600)

        # 注入逼真画面和运行状态
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
            if (transLabel) transLabel.textContent = 'LIVE / MJPEG';
            const inState = document.querySelector('#input-state');
            if (inState) {{
                const textSpan = inState.querySelector('.input-text');
                if (textSpan) textSpan.textContent = '输入中 (绝对指针)';
                inState.classList.add('active');
            }}
            document.querySelector('#led-num')?.classList.add('active');
            document.querySelector('#inspector-led-num')?.classList.add('active');
            if (typeof renderSystemMetrics === 'function') renderSystemMetrics({json.dumps(mock_system)});
        }}""")
        page.wait_for_timeout(400)

        # 4. 浅色主控制台
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "console-desktop.png"))
        print("Generated console-desktop.png")

        # 5. 深色主控制台
        page.evaluate("document.documentElement.setAttribute('data-theme', 'dark')")
        page.wait_for_timeout(300)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "console-dark.png"))
        print("Generated console-dark.png")

        # 6. 顶栏呼出 NAS 主控机性能监控浮动面板
        page.click("#topbar-perf-btn")
        page.wait_for_timeout(400)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "perf-popover.png"))
        print("Generated perf-popover.png")
        page.click("#perf-pop-close")
        page.wait_for_timeout(300)

        # 恢复浅色
        page.evaluate("document.documentElement.setAttribute('data-theme', 'light')")

        # 7. 虚拟全键盘
        page.click('#keyboard-toggle')
        page.wait_for_timeout(400)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "virtual-keyboard.png"))
        print("Generated virtual-keyboard.png")

        # 收起虚拟键盘
        page.click('#keyboard-dialog [data-close]')
        page.wait_for_timeout(200)

        # 8. 介质控制面板
        page.click('[data-panel-target="media"]')
        page.wait_for_timeout(400)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "media-panel.png"))
        print("Generated media-panel.png")

        # 9. 视频调节面板
        page.click('[data-panel-target="video"]')
        page.wait_for_timeout(400)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "video-panel.png"))
        print("Generated video-panel.png")

        # 10. 移动端竖屏 (390 x 844 iPhone 尺寸)
        page.set_viewport_size({"width": 390, "height": 844})
        page.evaluate("document.documentElement.setAttribute('data-theme', 'dark')")
        if page.query_selector('#app.inspector-open'):
            page.click('#inspector-close')
        page.wait_for_timeout(400)
        page.screenshot(path=os.path.join(DOCS_IMG_DIR, "mobile-console.png"))
        print("Generated mobile-console.png")

        browser.close()

if __name__ == "__main__":
    print(f"Using Chrome path: {CHROME_PATH}")
    print("Capturing setup flow...")
    capture_setup_flow()
    print("Capturing console screens...")
    capture_console()
    print("Done!")
