#!/usr/bin/env python3
"""Record the README demo animation (docs/images/demo-<lang>.webp).

The flow runs against the same mocked API and synthetic screen as
capture_screenshots.py (see capture_fixtures.py); no device is contacted and
no power/reset control is shown or clicked. The flow covers display modes,
paste as typing, keyboard macros, the host terminal, virtual media, connection
quality, host performance and themes. Each step is captured
as a still frame with a fixed display duration (no screen recording), so
timing never varies between runs, and the frames are assembled into a
looping animated WebP with img2webp (libwebp).

    python3 scripts/capture_demo.py [--lang zh-CN --lang en] [--output-dir docs/images]

capture_screenshots.py calls this too, so one command refreshes everything.
"""
import argparse, hashlib, json, os, shutil, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from playwright.sync_api import sync_playwright  # noqa: E402

import capture_fixtures as F  # noqa: E402
import ui_harness  # noqa: E402

VIEWPORT = (1280, 800)
LANGS = ("zh-CN", "en")
FILE_NAMES = {"zh-CN": "demo-zh.webp", "en": "demo-en.webp"}
PASTE_TEXT = "sudo apt update && sudo apt full-upgrade -y"
MAX_BYTES = 2 * 1024 * 1024

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
  window.__demoCursor = {
    move(x, y) { host.style.transform = `translate(${x}px, ${y}px)`; },
    press(on) { host.firstChild.style.transform = on ? 'scale(1)' : 'scale(0)'; },
    caption(text) { caption.textContent = text; },
    raise() { caption.hidePopover(); caption.showPopover(); host.hidePopover(); host.showPopover(); },
  };
}"""


class Recorder:
    def __init__(self, page, frame_dir, lang):
        self.page = page
        self.frame_dir = frame_dir
        self.lang = lang
        self.frames = []          # [path, duration_ms]
        self.last_digest = None
        self.pos = (VIEWPORT[0] * 0.55, VIEWPORT[1] * 0.62)
        page.evaluate(CURSOR_JS)
        self._cursor()

    def chapter(self, chinese, english):
        text = chinese if self.lang == "zh-CN" else english
        self.page.evaluate("text => window.__demoCursor.caption(text)", "DEMO · " + text)

    def _cursor(self):
        self.page.evaluate("([x, y]) => window.__demoCursor.move(x, y)", list(self.pos))

    def shot(self, duration):
        """Capture the current state and show it for `duration` ms."""
        self.page.evaluate("() => window.__demoCursor.raise()")
        data = self.page.screenshot(animations="disabled", caret="hide")
        digest = hashlib.sha256(data).hexdigest()
        if digest == self.last_digest:
            self.frames[-1][1] += duration
            return
        path = os.path.join(self.frame_dir, f"frame-{len(self.frames):03d}.png")
        with open(path, "wb") as fh:
            fh.write(data)
        self.frames.append([path, duration])
        self.last_digest = digest

    def hold(self, ms, settle=350):
        self.page.wait_for_timeout(settle)
        self.shot(ms)

    def move_to(self, selector, steps=6, frame_ms=45, anchor=(0.5, 0.5)):
        self.page.locator(selector).first.scroll_into_view_if_needed()
        box = self.page.locator(selector).first.bounding_box()
        if not box:
            raise RuntimeError(f"demo target not visible: {selector}")
        start = self.pos
        end = (box["x"] + box["width"] * anchor[0], box["y"] + box["height"] * anchor[1])
        for i in range(1, steps + 1):
            t = i / steps
            ease = 1 - (1 - t) ** 3
            self.pos = (start[0] + (end[0] - start[0]) * ease, start[1] + (end[1] - start[1]) * ease)
            self._cursor()
            self.shot(frame_ms)
        self.page.mouse.move(*self.pos)

    def click(self, selector, settle=400):
        self.move_to(selector)
        self.page.wait_for_timeout(120)
        self.page.evaluate("() => window.__demoCursor.press(true)")
        self.shot(110)
        self.page.mouse.click(*self.pos)
        self.page.evaluate("() => window.__demoCursor.press(false)")
        self.page.wait_for_timeout(settle)


def record(browser, port, lang, frame_dir):
    macro = {"id": "demo-tty", "name": "切换到 TTY2" if lang == "zh-CN" else "Switch to TTY2",
             "steps": [{"key": "F2", "ctrl": True, "alt": True, "delay_ms": 0},
                       {"key": "KeyL", "ctrl": True, "delay_ms": 800}]}
    context, page, target = F.new_page(
        browser, port, lang=lang, viewport=VIEWPORT, scale=1,
        storage={"wingman_setting_auto_expand_inspector": "false",
                 "wingman_keyboard_macros": json.dumps([macro])}, screens=("", PASTE_TEXT))
    page_errors = []
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    try:
        # Chapter captions explain each action; transient toasts distract from
        # the feature being shown. assert_clean still rejects any error toast.
        page.add_style_tag(content="#toast { visibility: hidden !important; }")
        # The inspector opens later on the media tab, so the power buttons on
        # the control tab never appear in the demo.
        page.evaluate("() => document.querySelector('[data-panel-target=\"media\"]').click()")
        rec = Recorder(page, frame_dir, lang)
        rec.chapter("远程画面 · 适应 / 1:1 / 填满", "Remote display · Fit / 1:1 / Fill")
        rec.hold(900)
        for mode in ("native", "fill", "fit"):
            rec.click("#mode-button")
            page.wait_for_function("mode => document.querySelector('#console').classList.contains('mode-' + mode)", arg=mode)
            rec.hold(650)

        # Paste as typing: virtual keyboard -> paste dialog -> type -> send.
        rec.chapter("虚拟键盘 · 粘贴为键入", "Virtual keyboard · Paste as typing")
        rec.click("#keyboard-toggle")
        rec.hold(700)
        rec.click("#paste-typing-btn")
        rec.hold(500)
        for end in range(3, len(PASTE_TEXT) + 3, 3):
            page.fill("#paste-key-text", PASTE_TEXT[:end])
            rec.shot(60)
        rec.hold(500, settle=100)
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
        rec.click("#paste-key-cancel")
        rec.click("#keyboard-dialog [data-close]")
        # The synthetic target now shows the pasted command at its prompt.
        page.evaluate("() => { document.querySelector('#video-feed').src = '/video_feed?frame=typed'; }")
        page.wait_for_function("() => document.querySelector('#video-feed').complete")
        rec.hold(650)

        # Use the real macro menu and replay path; HID messages reach the mock.
        rec.chapter("组合键 · 键盘宏与回放", "Key combinations · Keyboard macros and replay")
        rec.click("#quick-more-toggle")
        rec.hold(1000)
        rec.click("#quick-macro-manage")
        rec.hold(950)
        before_macro = target.keys()
        rec.click('#macro-manager-list [data-macro-play="demo-tty"]', settle=100)
        rec.hold(550, settle=100)
        page.wait_for_function("() => document.querySelector('#macro-play-progress').getAttribute('aria-valuenow') === '2'")
        rec.hold(700, settle=100)
        if target.keys() != before_macro + len(macro["steps"]):
            raise RuntimeError("demo macro did not reach the mocked target")
        rec.click("[data-macro-play-close]")
        rec.click("[data-macro-manager-close]")

        # xterm and its real WebSocket path, with a canned host response.
        rec.chapter("主控机网页终端 · 切换后保留会话", "Host web terminal · Session survives tab switching")
        rec.click('[data-workspace="terminal"]')
        page.wait_for_function("() => !!document.querySelector('#terminal-host .xterm-screen')")
        page.locator("#terminal-host .xterm-helper-textarea").focus()
        page.keyboard.type("uptime", delay=65)
        rec.hold(650, settle=150)
        page.keyboard.press("Enter")
        rec.hold(1200)
        if target.terminal_commands != ["uptime"]:
            raise RuntimeError("demo terminal command did not reach the mocked socket")
        rec.click('[data-workspace="video"]')
        page.wait_for_selector('[data-workspace="terminal"].session-live')
        rec.hold(450)

        # Virtual media panel, opened scrolled to the image list. (The native
        # file picker above it is labelled in the OS language, not the page's.)
        rec.chapter("虚拟介质 · ISO 光驱与 IMG 镜像", "Virtual media · ISO optical drive and IMG images")
        page.evaluate("""() => {
          const body = document.querySelector('.inspector-body'), list = document.querySelector('#media-list');
          body.scrollTop += list.getBoundingClientRect().top - body.getBoundingClientRect().top - 16;
        }""")
        rec.click("#inspector-toggle")
        rec.move_to("#media-list .media-item", anchor=(0.62, 0.3))
        rec.hold(1300)
        rec.click("#inspector-toggle")

        rec.chapter("连接质量 · 分辨率与帧率", "Connection quality · Resolution and frame rate")
        rec.click("#quality-toggle")
        page.wait_for_selector("#quality-overlay:not(.hidden)")
        rec.hold(1200)
        rec.click("#quality-toggle")

        # Diagnostics popover.
        rec.chapter("设备诊断 · 视频与键鼠通道", "Device diagnostics · Video and input channels")
        rec.click("#diagnostics-toggle")
        rec.move_to(".diagnostics-card:has(#diag-input-mode)", anchor=(0.9, 0.85))
        rec.hold(1400)
        rec.click("#diagnostics-toggle")

        rec.chapter("主机性能 · CPU / 内存 / 温度", "Host performance · CPU / Memory / Temperature")
        # Hover opens this popover; a delayed click would toggle it closed.
        rec.move_to("#topbar-perf-btn")
        page.wait_for_selector("#topbar-perf-popover:not([hidden])")
        rec.hold(1500)
        rec.click("#perf-pop-close")

        rec.chapter("深色主题 · 远程控制台", "Dark theme · Remote console")
        # The fixture starts light; the UI cycles light -> system -> dark.
        rec.click("#theme-toggle")
        rec.click("#theme-toggle")
        page.wait_for_function("() => document.documentElement.dataset.theme === 'dark'")
        rec.hold(1500)

        F.assert_clean(page, f"demo {lang}")
        if page_errors:
            raise RuntimeError(f"demo {lang}: page errors: {page_errors}")
        expected_keys = len(PASTE_TEXT) + len(macro["steps"])
        if target.keys() != expected_keys:
            raise RuntimeError(f"demo {lang}: {target.keys()} keys reached the mocked target; expected {expected_keys}")
        return rec.frames
    finally:
        context.close()


def find_encoder():
    # ffmpeg is often built without libwebp, so use the libwebp tool directly.
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


def build(browser, port, langs, output_dir, keep_frames=None):
    tool = find_encoder()
    outputs = []
    for lang in langs:
        frame_dir = keep_frames and os.path.join(keep_frames, lang)
        with tempfile.TemporaryDirectory(prefix="wkvm-demo-") as tmp:
            frame_dir = frame_dir or tmp
            os.makedirs(frame_dir, exist_ok=True)
            frames = record(browser, port, lang, frame_dir)
            output = os.path.join(output_dir, FILE_NAMES[lang])
            encode_webp(tool, frames, output)
            if keep_frames:
                with open(os.path.join(frame_dir, "durations.txt"), "w", encoding="utf-8") as fh:
                    fh.writelines(f"{os.path.basename(path)} {duration}\n" for path, duration in frames)
        total = sum(d for _, d in frames) / 1000
        size = os.path.getsize(output)
        print(f"Generated {os.path.basename(output)}: {len(frames)} frames, {total:.1f} s, {size / 1024:.0f} KiB")
        if size > MAX_BYTES:
            raise SystemExit(f"{output} is {size} bytes, above the {MAX_BYTES} byte README budget")
        outputs.append(output)
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lang", action="append", choices=LANGS, help="default: all")
    parser.add_argument("--output-dir", default=F.DOCS_IMG_DIR)
    parser.add_argument("--browser-channel", default="chrome")
    parser.add_argument("--keep-frames", default="", help="also write the PNG frames here")
    args = parser.parse_args()
    os.makedirs(args.output_dir, exist_ok=True)
    with ui_harness.static_server(F.REPO) as port, sync_playwright() as p:
        browser = F.launch(p, args.browser_channel)
        try:
            build(browser, port, args.lang or LANGS, args.output_dir, args.keep_frames or None)
        finally:
            browser.close()


if __name__ == "__main__":
    main()
