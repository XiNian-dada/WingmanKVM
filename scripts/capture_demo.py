#!/usr/bin/env python3
"""Record the README demo animations (docs/images/demo-<category>-<lang>.webp).

The flow runs against the same mocked API and synthetic screen as
capture_screenshots.py (see capture_fixtures.py); no device is contacted and
no power/reset control is shown or clicked. Features are split into dedicated,
focused animations per category:
  - setup:  Setup wizard, admin account, hardware detection & wiring guide
  - video:  Display modes (fit/native/fill), connection quality & stats
  - input:  Virtual keyboard, paste as typing, key combinations & macros
  - system: Host web terminal (persistent session), virtual media, diagnostics & dark theme
  - all:    Complete combined overview animation (demo-<lang>.webp)

Each step is captured as a still frame with smooth easing and high motion frame
rate (35-50 FPS cursor tracking, no screen recording), so timing never drifts,
and the frames are assembled into looping animated WebPs with img2webp (libwebp).

    python3 scripts/capture_demo.py [--demo video] [--lang zh-CN] [--output-dir docs/images]
"""
import argparse, hashlib, json, math, os, shutil, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from playwright.sync_api import sync_playwright  # noqa: E402

import capture_fixtures as F  # noqa: E402
import ui_harness  # noqa: E402

VIEWPORT = (1280, 800)
LANGS = ("zh-CN", "en")
CATEGORIES = ("setup", "video", "input", "system")
PASTE_TEXT = "sudo apt update && sudo apt full-upgrade -y"
MAX_BYTES = 2 * 1024 * 1024

def file_name(category, lang):
    if category == "all":
        return f"demo-{'zh' if lang == 'zh-CN' else 'en'}.webp"
    return f"demo-{category}-{'zh' if lang == 'zh-CN' else 'en'}.webp"


# A pointer drawn in the page, since headless screenshots have no cursor. It
# lives in a manual popover (top layer) so it can be raised above modal dialogs.
CURSOR_JS = """() => {
  if (window.__demoCursor) return;
  const host = document.createElement('div');
  host.setAttribute('popover', 'manual');
  host.dataset.noI18n = '';
  host.style.cssText = 'position:fixed;inset:auto;left:0;top:0;margin:0;padding:0;border:0;' +
    'background:transparent;overflow:visible;pointer-events:none;width:1px;height:1px;';
  host.innerHTML =
    '<span style="position:absolute;left:-16px;top:-16px;width:32px;height:32px;border-radius:50%;' +
    'background:rgba(0,112,243,.28);transform:scale(0);transition:none"></span>' +
    '<svg width="22" height="26" viewBox="0 0 22 26" style="position:absolute;left:-3px;top:-2px;overflow:visible">' +
    '<path d="M3 2 L3 21 L8 16.5 L11.5 24 L14.5 22.6 L11 15.4 L17.5 15.4 Z" fill="#111" stroke="#fff" ' +
    'stroke-width="1.6" stroke-linejoin="round"/></svg>';
  document.body.append(host);
  host.showPopover();
  const caption = document.createElement('div');
  caption.setAttribute('popover', 'manual');
  caption.dataset.noI18n = '';
  caption.style.cssText = 'position:fixed;inset:auto;left:20px;bottom:20px;margin:0;' +
    'padding:10px 16px;border:1px solid #ffffff26;border-radius:12px;' +
    'background:#172033;color:#fff;font:500 14px system-ui,sans-serif;pointer-events:none;';
  document.body.append(caption);
  caption.showPopover();
  let curX = 0, curY = 0;
  window.__demoCursor = {
    move(x, y) { curX = x; curY = y; host.style.transform = `translate(${x}px, ${y}px)`; },
    press(on) { host.firstChild.style.transform = on ? 'scale(1)' : 'scale(0)'; },
    caption(text) { caption.textContent = text; },
    raise() {
      try {
        caption.hidePopover(); caption.showPopover();
        host.hidePopover(); host.showPopover();
        host.style.transform = `translate(${curX}px, ${curY}px)`;
      } catch (e) {}
    },
  };
}"""


def ease_in_out(t: float) -> float:
    """Smooth cubic ease-in-out S-curve with zero initial & final velocity."""
    return 4 * t * t * t if t < 0.5 else 1 - math.pow(-2 * t + 2, 3) / 2


def motion_params(dx: float, dy: float):
    """Dynamic step count and frame duration (~35-45 FPS) based on distance."""
    dist = math.hypot(dx, dy)
    if dist < 6:
        return 0, 0
    if dist < 80:
        return 8, 28
    if dist < 250:
        return 12, 26
    if dist < 600:
        return 16, 24
    return 20, 22


class Recorder:
    def __init__(self, page, frame_dir, lang, initial_pos=None):
        self.page = page
        self.frame_dir = frame_dir
        self.lang = lang
        self.frames = []          # [path, duration_ms]
        self.last_digest = None
        self.pos = initial_pos or (VIEWPORT[0] * 0.5, VIEWPORT[1] * 0.5)
        page.evaluate(CURSOR_JS)
        self._cursor()

    def chapter(self, chinese, english):
        text = chinese if self.lang == "zh-CN" else english
        self.page.evaluate("text => window.__demoCursor.caption(text)", "DEMO · " + text)

    def raise_cursor(self):
        self.page.evaluate("() => window.__demoCursor.raise()")

    def _cursor(self):
        self.page.evaluate("([x, y]) => window.__demoCursor.move(x, y)", list(self.pos))

    def shot(self, duration):
        """Capture the current state and show it for `duration` ms."""
        data = self.page.screenshot(animations="disabled", caret="hide")
        digest = hashlib.sha256(data).hexdigest()
        if digest == self.last_digest:
            self.frames[-1][1] += duration
            return
        path = os.path.join(self.frame_dir, f"frame-{len(self.frames):04d}.png")
        with open(path, "wb") as fh:
            fh.write(data)
        self.frames.append([path, duration])
        self.last_digest = digest

    def hold(self, ms, settle=300):
        if settle > 0:
            self.page.wait_for_timeout(settle)
        self.shot(ms)

    def move_to(self, target, anchor=(0.5, 0.5)):
        """Move cursor smoothly using S-curve easing and distance-adaptive FPS."""
        if isinstance(target, (tuple, list)):
            end = (float(target[0]), float(target[1]))
        else:
            loc = self.page.locator(target).first
            box = loc.bounding_box()
            if not box:
                loc.scroll_into_view_if_needed()
                box = loc.bounding_box()
            if not box:
                raise RuntimeError(f"demo target not visible: {target}")
            end = (box["x"] + box["width"] * anchor[0], box["y"] + box["height"] * anchor[1])

        start = self.pos
        dx = end[0] - start[0]
        dy = end[1] - start[1]
        steps, frame_ms = motion_params(dx, dy)
        if steps == 0:
            self.pos = end
            self._cursor()
            return

        for i in range(1, steps + 1):
            t = i / steps
            ease = ease_in_out(t)
            self.pos = (start[0] + dx * ease, start[1] + dy * ease)
            self._cursor()
            self.shot(frame_ms)
        self.page.mouse.move(*self.pos)

    def click(self, target=None, anchor=(0.5, 0.5), settle=350):
        if target is not None:
            self.move_to(target, anchor=anchor)
        self.page.wait_for_timeout(60)
        self.page.evaluate("() => window.__demoCursor.press(true)")
        self.shot(90)
        if target is not None and not isinstance(target, (tuple, list)):
            self.page.locator(target).first.click()
        else:
            self.page.mouse.click(*self.pos)
        self.page.evaluate("() => window.__demoCursor.press(false)")
        self.shot(70)
        if settle > 0:
            self.page.wait_for_timeout(settle)

    def type_input(self, target, text, chunk_size=3, step_ms=45):
        """Click into input field and simulate natural typing."""
        self.click(target, settle=100)
        for end in range(chunk_size, len(text) + chunk_size, chunk_size):
            self.page.fill(target, text[:end])
            self.shot(step_ms)


# ==============================================================================
# Category 1: Complete Installation Flow (Terminal -> Setup -> Live Console)
# ==============================================================================
TERMINAL_HTML = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    background: #090d16; color: #c9d1d9;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace;
    width: 1280px; height: 800px;
    display: flex; align-items: center; justify-content: center;
  }
  .window {
    width: 1060px; height: 620px;
    background: #0d1117; border-radius: 12px;
    border: 1px solid #30363d;
    box-shadow: 0 20px 50px rgba(0,0,0,0.7);
    display: flex; flex-direction: column; overflow: hidden;
  }
  .titlebar {
    height: 38px; background: #161b22;
    border-bottom: 1px solid #30363d;
    display: flex; align-items: center; padding: 0 16px; gap: 8px;
  }
  .dot { width: 12px; height: 12px; border-radius: 50%; display: inline-block; }
  .dot-red { background: #ff5f56; }
  .dot-yellow { background: #ffbd2e; }
  .dot-green { background: #27c93f; }
  .title { flex: 1; text-align: center; font-size: 13px; color: #8b949e; margin-right: 48px; }
  .terminal {
    flex: 1; padding: 24px;
    font-family: Menlo, Monaco, "DejaVu Sans Mono", monospace;
    font-size: 14px; line-height: 1.6;
  }
  .prompt { color: #3fb950; font-weight: 600; }
  .path { color: #58a6ff; }
  .ok { color: #3fb950; font-weight: 600; }
  .box-wrap {
    margin-top: 16px; padding: 14px 18px;
    background: rgba(56, 139, 253, 0.08);
    border: 1px solid rgba(56, 139, 253, 0.35);
    border-radius: 8px;
  }
  .link { color: #58a6ff; text-decoration: underline; font-weight: 600; cursor: pointer; }
</style>
</head>
<body>
<div class="window">
  <div class="titlebar">
    <span class="dot dot-red"></span><span class="dot dot-yellow"></span><span class="dot dot-green"></span>
    <span class="title">ubuntu@firefly-rk3399: ~ (ssh)</span>
  </div>
  <div class="terminal">
    <p><span class="prompt">ubuntu@firefly-rk3399</span>:<span class="path">~</span>$ <span id="cmd"></span><span id="term-cursor" style="display:inline-block;width:8px;height:16px;background:#58a6ff;vertical-align:middle;"></span></p>
    <div id="output" style="display:none;">
      <p style="color:#8b949e; margin-top: 8px;">wingmankvm-get: downloading release v2.0.0-rc.1 for aarch64...</p>
      <p style="color:#8b949e;">wingmankvm-get: verified SHA256 checksum for release archive <span class="ok">[OK]</span></p>
      <p style="color:#8b949e;">wingmankvm: configuring UVC video & USB OTG gadgets... <span class="ok">[OK]</span></p>
      <p style="color:#8b949e;">wingmankvm: service installed and active (running) <span class="ok">[OK]</span></p>
      <div class="box-wrap">
        <p style="color:#fff; font-weight: 500;">{msg}</p>
        <p style="margin-top: 6px;"><span id="setup-link" class="link">http://192.168.1.100:8080/#setup=quick-start-token</span></p>
      </div>
    </div>
  </div>
</div>
</body>
</html>"""


def record_setup(browser, port, lang, frame_dir):
    current_bootstrap = dict(F.BOOTSTRAP_SETUP)

    def api_route_wrapper(route):
        if "/api/setup" in route.request.url and route.request.method == "POST":
            current_bootstrap.clear()
            current_bootstrap.update(F.BOOTSTRAP_CONSOLE)
            route.fulfill(status=200, content_type="application/json", body='{"ok": true}')
            return
        F.api_route(route, current_bootstrap)

    context = browser.new_context(
        viewport={"width": VIEWPORT[0], "height": VIEWPORT[1]},
        device_scale_factor=1,
        locale=F.LOCALES.get(lang, lang),
        timezone_id=F.TIMEZONE,
        color_scheme="light",
    )
    context.add_init_script(F.INIT_SCRIPT + "\n" + "".join(
        f"localStorage.setItem({json.dumps(k)}, {json.dumps(v)});\n" for k, v in {
            "wingman_lang": lang,
            "wingman_theme": "light",
            "wingman_setting_render_pixelated": "false",
        }.items()
    ))
    page = context.new_page()
    page.clock.set_fixed_time(F.FIXED_TIME)
    page.route("**/api/**", api_route_wrapper)
    target = F.Target(F.render_screens(browser))
    target.attach(page)

    try:
        rec = Recorder(page, frame_dir, lang, initial_pos=(640, 480))

        # ----------------------------------------------------------------------
        # Stage 1: Terminal Installation
        # ----------------------------------------------------------------------
        rec.chapter("一键命令行安装 · ARM 开发板", "One-line installer · ARM development board")
        box_msg = "🎉 WingmanKVM 安装成功！请在浏览器中打开初始化链接：" if lang == "zh-CN" else "🎉 WingmanKVM installed! Open setup link in browser:"
        page.set_content(TERMINAL_HTML.replace("{msg}", box_msg))
        page.evaluate(CURSOR_JS)
        rec.pos = (640, 480)
        rec._cursor()
        rec.hold(700)

        cmd_text = "curl -fsSL https://.../get.sh | sudo bash -s -- --install-deps"
        for end in range(4, len(cmd_text) + 4, 4):
            chunk = cmd_text[:end]
            page.evaluate("chunk => document.querySelector('#cmd').textContent = chunk", chunk)
            rec.shot(40)
        rec.hold(400)

        # Output results with OK indicators and setup link
        page.evaluate("() => { document.querySelector('#output').style.display = 'block'; document.querySelector('#term-cursor').style.display = 'none'; }")
        rec.hold(900)

        # Click the link
        rec.click("#setup-link", settle=300)

        # ----------------------------------------------------------------------
        # Stage 2: Browser Setup Page
        # ----------------------------------------------------------------------
        rec.chapter("初始化向导 · 管理员账号设置", "Setup wizard · Admin account configuration")
        page.goto(f"http://127.0.0.1:{port}/#setup=quick-start-token", wait_until="load")
        page.wait_for_selector("#setup-form:not(.hidden)")
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        page.evaluate(CURSOR_JS)
        rec.pos = (640, 360)
        rec._cursor()
        rec.hold(600)

        rec.type_input('#setup-form input[name="username"]', "admin", chunk_size=2, step_ms=45)
        rec.hold(250)
        rec.type_input('#setup-form input[name="password"]', "Admin123456!@#", chunk_size=3, step_ms=40)
        rec.hold(250)
        rec.type_input('#setup-form input[name="confirm_password"]', "Admin123456!@#", chunk_size=3, step_ms=40)
        rec.hold(500)

        rec.click('#setup-form [data-setup-next]', settle=400)
        page.wait_for_selector('[data-setup-step="1"].active')
        page.wait_for_selector('#setup-device-results .capability')

        # ----------------------------------------------------------------------
        # Stage 3: Automatic Hardware Detection
        # ----------------------------------------------------------------------
        rec.chapter("自动硬件检测 · 采集卡与 USB HID", "Automatic hardware detection · Video & USB HID")
        rec.move_to('#setup-device-results .capability')
        rec.hold(1300)

        # ----------------------------------------------------------------------
        # Stage 4: Enter Console
        # ----------------------------------------------------------------------
        rec.chapter("进入控制台 · 远程画面与键鼠控制", "Enter console · Remote display & input control")
        rec.click('#setup-submit', settle=300)
        page.wait_for_selector('#app:not(.hidden)', timeout=8000)
        page.wait_for_function("() => document.querySelector('#status-dot')?.classList.contains('online')", timeout=8000)

        # Focus remote console display
        rec.move_to('#video-feed', anchor=(0.5, 0.45))
        rec.hold(1600)

        # Smooth return to center for clean loop
        rec.move_to((640, 420))
        rec.hold(600)

        F.assert_clean(page, f"demo setup {lang}")
        return rec.frames
    finally:
        context.close()


# ==============================================================================
# Category 2: Display Modes & Remote Video
# ==============================================================================
def record_video(browser, port, lang, frame_dir):
    context, page, _ = F.new_page(
        browser, port, lang=lang, viewport=VIEWPORT, scale=1,
        storage={"wingman_setting_auto_expand_inspector": "false"}
    )
    try:
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        rec = Recorder(page, frame_dir, lang, initial_pos=(640, 380))
        rec.chapter("远程画面 · 适应 / 1:1 / 填满", "Remote display · Fit / 1:1 / Fill")
        rec.hold(900)

        # Cycle display modes: native -> fill -> fit
        for mode in ("native", "fill", "fit"):
            rec.click("#mode-button", settle=200)
            page.wait_for_function("mode => document.querySelector('#console').classList.contains('mode-' + mode)", arg=mode)
            rec.hold(750)

        # Connection quality overlay
        rec.chapter("连接质量 · 实时流与状态", "Connection quality · Stream stats & Status")
        rec.click("#quality-toggle", settle=200)
        page.wait_for_selector("#quality-overlay:not(.hidden)")
        rec.hold(1600)
        rec.click("#quality-toggle", settle=200)

        # Smoothly return to center for clean loop
        rec.move_to((640, 380))
        rec.hold(700)

        F.assert_clean(page, f"demo video {lang}")
        return rec.frames
    finally:
        context.close()


# ==============================================================================
# Category 3: Keyboard, Mouse, Paste as Typing & Macros
# ==============================================================================
def record_input(browser, port, lang, frame_dir):
    macro = {"id": "demo-tty", "name": "切换到 TTY2" if lang == "zh-CN" else "Switch to TTY2",
             "steps": [{"key": "F2", "ctrl": True, "alt": True, "delay_ms": 0},
                       {"key": "KeyL", "ctrl": True, "delay_ms": 800}]}
    context, page, target = F.new_page(
        browser, port, lang=lang, viewport=VIEWPORT, scale=1,
        storage={"wingman_setting_auto_expand_inspector": "false",
                 "wingman_keyboard_macros": json.dumps([macro])}, screens=("", PASTE_TEXT)
    )
    try:
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        rec = Recorder(page, frame_dir, lang, initial_pos=(640, 380))

        # Paste as typing
        rec.chapter("虚拟键盘 · 粘贴为键入", "Virtual keyboard · Paste as typing")
        rec.click("#keyboard-toggle", settle=300)
        rec.raise_cursor()
        rec.hold(700)

        rec.click("#paste-typing-btn", settle=300)
        rec.raise_cursor()
        rec.hold(500)

        rec.type_input("#paste-key-text", PASTE_TEXT, chunk_size=3, step_ms=50)
        rec.hold(500)

        target.hold_at = len(PASTE_TEXT) // 2
        rec.click("#paste-key-send", settle=0)
        page.wait_for_function("n => document.querySelector('#paste-key-progress')"
                               ".getAttribute('aria-valuenow') == n", arg=str(target.hold_at - 1))
        rec.hold(450, settle=50)
        target.typed = PASTE_TEXT
        target.release()
        page.wait_for_function("() => /\\d/.test(document.querySelector('#paste-key-status').textContent) &&"
                               " !document.querySelector('#paste-key-send').disabled")
        rec.hold(1000, settle=100)
        rec.click("#paste-key-cancel", settle=200)
        rec.click("#keyboard-dialog [data-close]", settle=200)

        # Show typed command on target console
        page.evaluate("() => { document.querySelector('#video-feed').src = '/video_feed?frame=typed'; }")
        page.wait_for_function("() => document.querySelector('#video-feed').complete")
        rec.hold(800)

        # Key combinations & Macro manager replay
        rec.chapter("组合键 · 键盘宏与回放", "Key combinations · Keyboard macros and replay")
        rec.click("#quick-more-toggle", settle=200)
        rec.hold(900)
        rec.click("#quick-macro-manage", settle=300)
        rec.raise_cursor()
        rec.hold(900)
        before_macro = target.keys()
        rec.click('#macro-manager-list [data-macro-play="demo-tty"]', settle=100)
        rec.hold(550, settle=100)
        page.wait_for_function("() => document.querySelector('#macro-play-progress').getAttribute('aria-valuenow') === '2'")
        rec.hold(700, settle=100)
        if target.keys() != before_macro + len(macro["steps"]):
            raise RuntimeError("demo macro did not reach the mocked target")
        rec.click("[data-macro-play-close]", settle=200)
        rec.click("[data-macro-manager-close]", settle=200)

        # Smoothly move back to center
        rec.move_to((640, 380))
        rec.hold(700)

        F.assert_clean(page, f"demo input {lang}")
        return rec.frames
    finally:
        context.close()


# ==============================================================================
# Category 4: Web Terminal, Virtual Media, Diagnostics & System
# ==============================================================================
def record_system(browser, port, lang, frame_dir):
    context, page, target = F.new_page(
        browser, port, lang=lang, viewport=VIEWPORT, scale=1,
        storage={"wingman_setting_auto_expand_inspector": "false"}
    )
    try:
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        page.evaluate("() => document.querySelector('[data-panel-target=\"media\"]').click()")
        rec = Recorder(page, frame_dir, lang, initial_pos=(640, 380))

        # Web Terminal
        rec.chapter("主控机网页终端 · 切换后保留会话", "Host web terminal · Session survives tab switching")
        rec.click('[data-workspace="terminal"]', settle=300)
        page.wait_for_function("() => !!document.querySelector('#terminal-host .xterm-screen')")
        page.locator("#terminal-host .xterm-helper-textarea").focus()
        page.keyboard.type("uptime", delay=55)
        rec.hold(600, settle=100)
        page.keyboard.press("Enter")
        rec.hold(1300)
        if target.terminal_commands != ["uptime"]:
            raise RuntimeError("demo terminal command did not reach the mocked socket")

        rec.click('[data-workspace="video"]', settle=200)
        page.wait_for_selector('[data-workspace="terminal"].session-live')
        rec.hold(600)

        # Virtual media
        rec.chapter("虚拟介质 · ISO 光驱与 IMG 镜像", "Virtual media · ISO optical drive and IMG images")
        page.evaluate("""() => {
          const body = document.querySelector('.inspector-body'), list = document.querySelector('#media-list');
          body.scrollTop += list.getBoundingClientRect().top - body.getBoundingClientRect().top - 16;
        }""")
        rec.click("#inspector-toggle", settle=300)
        rec.move_to("#media-list .media-item", anchor=(0.62, 0.3))
        rec.hold(1400)
        rec.click("#inspector-toggle", settle=300)

        # Device diagnostics
        rec.chapter("设备诊断 · 视频与键鼠通道", "Device diagnostics · Video and input channels")
        rec.click("#diagnostics-toggle", settle=300)
        rec.raise_cursor()
        rec.move_to(".diagnostics-card:has(#diag-input-mode)", anchor=(0.9, 0.85))
        rec.hold(1400)
        rec.click("#diagnostics-toggle", settle=250)

        # Host performance popover
        rec.chapter("主机性能 · CPU / 内存 / 温度", "Host performance · CPU / Memory / Temperature")
        rec.move_to("#topbar-perf-btn")
        page.wait_for_selector("#topbar-perf-popover:not([hidden])")
        rec.hold(1500)
        rec.click("#perf-pop-close", settle=200)

        # Dark theme
        rec.chapter("深色主题 · 远程控制台", "Dark theme · Remote console")
        rec.click("#theme-toggle", settle=200)
        rec.click("#theme-toggle", settle=200)
        page.wait_for_function("() => document.documentElement.dataset.theme === 'dark'")
        rec.hold(1400)

        F.assert_clean(page, f"demo system {lang}")
        return rec.frames
    finally:
        context.close()


# ==============================================================================
# Combined Overview: All chapters in one sequence (backward compatibility)
# ==============================================================================
def record_all(browser, port, lang, frame_dir):
    macro = {"id": "demo-tty", "name": "切换到 TTY2" if lang == "zh-CN" else "Switch to TTY2",
             "steps": [{"key": "F2", "ctrl": True, "alt": True, "delay_ms": 0},
                       {"key": "KeyL", "ctrl": True, "delay_ms": 800}]}
    context, page, target = F.new_page(
        browser, port, lang=lang, viewport=VIEWPORT, scale=1,
        storage={"wingman_setting_auto_expand_inspector": "false",
                 "wingman_keyboard_macros": json.dumps([macro])}, screens=("", PASTE_TEXT)
    )
    try:
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        page.evaluate("() => document.querySelector('[data-panel-target=\"media\"]').click()")
        rec = Recorder(page, frame_dir, lang, initial_pos=(640, 380))

        # 1. Display modes
        rec.chapter("远程画面 · 适应 / 1:1 / 填满", "Remote display · Fit / 1:1 / Fill")
        rec.hold(900)
        for mode in ("native", "fill", "fit"):
            rec.click("#mode-button", settle=200)
            page.wait_for_function("mode => document.querySelector('#console').classList.contains('mode-' + mode)", arg=mode)
            rec.hold(650)

        # 2. Paste as typing
        rec.chapter("虚拟键盘 · 粘贴为键入", "Virtual keyboard · Paste as typing")
        rec.click("#keyboard-toggle", settle=300)
        rec.raise_cursor()
        rec.hold(700)
        rec.click("#paste-typing-btn", settle=300)
        rec.raise_cursor()
        rec.hold(500)
        rec.type_input("#paste-key-text", PASTE_TEXT, chunk_size=3, step_ms=50)
        rec.hold(500)
        target.hold_at = len(PASTE_TEXT) // 2
        rec.click("#paste-key-send", settle=0)
        page.wait_for_function("n => document.querySelector('#paste-key-progress')"
                               ".getAttribute('aria-valuenow') == n", arg=str(target.hold_at - 1))
        rec.hold(450, settle=50)
        target.typed = PASTE_TEXT
        target.release()
        page.wait_for_function("() => /\\d/.test(document.querySelector('#paste-key-status').textContent) &&"
                               " !document.querySelector('#paste-key-send').disabled")
        rec.hold(1100, settle=150)
        rec.click("#paste-key-cancel", settle=200)
        rec.click("#keyboard-dialog [data-close]", settle=200)
        page.evaluate("() => { document.querySelector('#video-feed').src = '/video_feed?frame=typed'; }")
        page.wait_for_function("() => document.querySelector('#video-feed').complete")
        rec.hold(650)

        # 3. Macro replay
        rec.chapter("组合键 · 键盘宏与回放", "Key combinations · Keyboard macros and replay")
        rec.click("#quick-more-toggle", settle=200)
        rec.hold(950)
        rec.click("#quick-macro-manage", settle=300)
        rec.raise_cursor()
        rec.hold(950)
        before_macro = target.keys()
        rec.click('#macro-manager-list [data-macro-play="demo-tty"]', settle=100)
        rec.hold(550, settle=100)
        page.wait_for_function("() => document.querySelector('#macro-play-progress').getAttribute('aria-valuenow') === '2'")
        rec.hold(700, settle=100)
        if target.keys() != before_macro + len(macro["steps"]):
            raise RuntimeError("demo macro did not reach the mocked target")
        rec.click("[data-macro-play-close]", settle=200)
        rec.click("[data-macro-manager-close]", settle=200)

        # 4. Web Terminal
        rec.chapter("主控机网页终端 · 切换后保留会话", "Host web terminal · Session survives tab switching")
        rec.click('[data-workspace="terminal"]', settle=300)
        page.wait_for_function("() => !!document.querySelector('#terminal-host .xterm-screen')")
        page.locator("#terminal-host .xterm-helper-textarea").focus()
        page.keyboard.type("uptime", delay=55)
        rec.hold(650, settle=150)
        page.keyboard.press("Enter")
        rec.hold(1200)
        if target.terminal_commands != ["uptime"]:
            raise RuntimeError("demo terminal command did not reach the mocked socket")
        rec.click('[data-workspace="video"]', settle=200)
        page.wait_for_selector('[data-workspace="terminal"].session-live')
        rec.hold(450)

        # 5. Virtual Media
        rec.chapter("虚拟介质 · ISO 光驱与 IMG 镜像", "Virtual media · ISO optical drive and IMG images")
        page.evaluate("""() => {
          const body = document.querySelector('.inspector-body'), list = document.querySelector('#media-list');
          body.scrollTop += list.getBoundingClientRect().top - body.getBoundingClientRect().top - 16;
        }""")
        rec.click("#inspector-toggle", settle=300)
        rec.move_to("#media-list .media-item", anchor=(0.62, 0.3))
        rec.hold(1300)
        rec.click("#inspector-toggle", settle=300)

        # 6. Quality
        rec.chapter("连接质量 · 分辨率与帧率", "Connection quality · Resolution and frame rate")
        rec.click("#quality-toggle", settle=200)
        page.wait_for_selector("#quality-overlay:not(.hidden)")
        rec.hold(1200)
        rec.click("#quality-toggle", settle=200)

        # 7. Diagnostics
        rec.chapter("设备诊断 · 视频与键鼠通道", "Device diagnostics · Video and input channels")
        rec.click("#diagnostics-toggle", settle=300)
        rec.raise_cursor()
        rec.move_to(".diagnostics-card:has(#diag-input-mode)", anchor=(0.9, 0.85))
        rec.hold(1400)
        rec.click("#diagnostics-toggle", settle=250)

        # 8. Performance
        rec.chapter("主机性能 · CPU / 内存 / 温度", "Host performance · CPU / Memory / Temperature")
        rec.move_to("#topbar-perf-btn")
        page.wait_for_selector("#topbar-perf-popover:not([hidden])")
        rec.hold(1500)
        rec.click("#perf-pop-close", settle=200)

        # 9. Dark theme
        rec.chapter("深色主题 · 远程控制台", "Dark theme · Remote console")
        rec.click("#theme-toggle", settle=200)
        rec.click("#theme-toggle", settle=200)
        page.wait_for_function("() => document.documentElement.dataset.theme === 'dark'")
        rec.hold(1400)

        F.assert_clean(page, f"demo {lang}")
        return rec.frames
    finally:
        context.close()


RECORDERS = {
    "setup": record_setup,
    "video": record_video,
    "input": record_input,
    "system": record_system,
    "all": record_all,
}


def find_encoder():
    tool = shutil.which("img2webp")
    if not tool:
        raise SystemExit("img2webp not found: install libwebp (brew install webp / apt install webp)")
    return tool


def encode_webp(tool, frames, output):
    args = [tool, "-loop", "0", "-lossless", "-m", "6"]
    for path, duration in frames:
        args += ["-d", str(duration), path]
    args += ["-o", output]
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def build(browser, port, langs, output_dir, categories=None, keep_frames=None):
    tool = find_encoder()
    targets = categories or list(CATEGORIES)
    outputs = []
    for cat in targets:
        recorder_fn = RECORDERS[cat]
        for lang in langs:
            frame_dir = keep_frames and os.path.join(keep_frames, f"{cat}-{lang}")
            with tempfile.TemporaryDirectory(prefix=f"wkvm-demo-{cat}-") as tmp:
                frame_dir = frame_dir or tmp
                os.makedirs(frame_dir, exist_ok=True)
                frames = recorder_fn(browser, port, lang, frame_dir)
                out_name = file_name(cat, lang)
                output = os.path.join(output_dir, out_name)
                encode_webp(tool, frames, output)
                if keep_frames:
                    with open(os.path.join(frame_dir, "durations.txt"), "w", encoding="utf-8") as fh:
                        fh.writelines(f"{os.path.basename(p)} {d}\n" for p, d in frames)
            total = sum(d for _, d in frames) / 1000
            size = os.path.getsize(output)
            print(f"Generated {out_name}: {len(frames)} frames, {total:.1f} s, {size / 1024:.0f} KiB")
            if size > MAX_BYTES:
                raise SystemExit(f"{output} is {size} bytes, above the {MAX_BYTES} byte budget")
            outputs.append(output)
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--demo", choices=list(CATEGORIES) + ["all"], action="append",
                        help="which demo to generate (default: all categories)")
    parser.add_argument("--lang", action="append", choices=LANGS, help="default: all (zh-CN, en)")
    parser.add_argument("--output-dir", default=F.DOCS_IMG_DIR)
    parser.add_argument("--browser-channel", default="chrome")
    parser.add_argument("--keep-frames", default="", help="also write the PNG frames here")
    args = parser.parse_args()

    os.makedirs(args.output_dir, exist_ok=True)
    demos = args.demo or list(CATEGORIES)
    with ui_harness.static_server(F.REPO) as port, sync_playwright() as p:
        browser = F.launch(p, args.browser_channel)
        try:
            build(browser, port, args.lang or LANGS, args.output_dir,
                  categories=demos, keep_frames=args.keep_frames or None)
        finally:
            browser.close()


if __name__ == "__main__":
    main()
