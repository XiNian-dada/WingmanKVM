#!/usr/bin/env python3
"""Audit round 2 UI regressions (issues #29, #35-#38, #43).

Reuses the P2b mocked API harness: real DOM, real browser keyboard/pointer
events and the real UI state machine; no hardware is touched.
"""
import argparse, inspect, json, os, sys, time, urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from playwright.sync_api import sync_playwright
    from verify_p2_ui import extract_html, read_assets, serve_static, check
    from verify_p2b_ui import GIF, State, api_route, bootstrap_payload, new_page
except Exception as exc:  # pragma: no cover - environment guard
    print(json.dumps({"status": "FAIL", "error": f"Playwright/helper import failed: {exc}"}))
    sys.exit(2)



PIP_STUB = """
(() => {
  let current = null;
  window.__pipCalls = [];
  window.__pipReject = false;
  Object.defineProperty(Document.prototype, 'pictureInPictureEnabled', {configurable: true, get: () => true});
  Object.defineProperty(Document.prototype, 'pictureInPictureElement', {configurable: true, get: () => current});
  HTMLVideoElement.prototype.requestPictureInPicture = async function () {
    if (window.__pipReject) throw new DOMException('denied', 'NotAllowedError');
    window.__pipCalls.push(this.id || this.className);
    current = this;
    this.dispatchEvent(new Event('enterpictureinpicture'));
    return {};
  };
  Document.prototype.exitPictureInPicture = async function () {
    const element = current;
    current = null;
    element?.dispatchEvent(new Event('leavepictureinpicture'));
  };
})();
"""

FAKE_PEER = """
(() => {
  window.__statsCalls = 0;
  window.__statsSeq = [];
  window.__peers = [];
  window.RTCPeerConnection = class {
    constructor() {
      this.iceGatheringState = 'complete';
      this.connectionState = 'connected';
      this.iceConnectionState = 'connected';
      this.localDescription = {sdp: 'v=0'};
      window.__peers.push(this);
    }
    addTransceiver() { return {}; }
    async createOffer() { return {type: 'offer', sdp: 'v=0'}; }
    async setLocalDescription() {}
    async setRemoteDescription() {
      const canvas = document.createElement('canvas');
      canvas.width = 64; canvas.height = 36;
      const paint = canvas.getContext('2d');
      setInterval(() => { paint.fillStyle = `hsl(${Date.now() % 360},50%,50%)`; paint.fillRect(0, 0, 64, 36); }, 50);
      const stream = canvas.captureStream(20), track = stream.getVideoTracks()[0];
      setTimeout(() => this.ontrack?.({track, streams: [stream]}), 0);
    }
    getReceivers() { return []; }
    addEventListener() {}
    removeEventListener() {}
    close() { this.connectionState = 'closed'; }
    async getStats() {
      window.__statsCalls++;
      const seq = window.__statsSeq;
      return new Map((seq.length > 1 ? seq.shift() : seq[0] || []).map((entry) => [entry.id, entry]));
    }
  };
})();
"""


def active_id(page):
    return page.evaluate("() => document.activeElement && document.activeElement.id")


def keyboard_focus(browser, port, checks):
    """#29 / #36: forwarding focus and <summary> handling."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#input-state")
        check(checks, active_id(page) == "video-viewport", "input_toggle_click_focuses_viewport",
              active_id(page))
        page.keyboard.press("a")
        page.wait_for_timeout(150)
        check(checks, any(p.get("key") in ("a", "A", "KeyA") for p in state.key_posts),
              "typing_after_toggle_click_is_forwarded", state.key_posts)

        before = len(state.key_posts)
        summary = page.locator("#video-config details summary").first
        page.locator('[data-panel-target="video"]').click()
        summary.focus()
        page.keyboard.press("Enter")
        page.wait_for_timeout(150)
        opened = page.evaluate(
            "() => document.querySelector('#video-config details').open")
        check(checks, opened, "summary_enter_opens_details", opened)
        check(checks, len(state.key_posts) == before, "summary_enter_not_forwarded",
              state.key_posts[before:])

        page.click("#keyboard-toggle")
        page.locator('#virtual-keyboard button[data-key="Q"]').click()
        page.wait_for_timeout(100)
        focused_key = page.evaluate(
            "() => document.activeElement && document.activeElement.dataset.key || ''")
        check(checks, focused_key != "Q", "virtual_key_does_not_keep_focus", focused_key)
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    parser.add_argument("--browser-channel", default="chrome")
    args = parser.parse_args()
    checks = []
    summary = {"status": "FAIL", "repo": os.path.abspath(args.repo), "checks": checks}
    server = None
    try:
        server, port = serve_static(extract_html(args.repo), read_assets(args.repo))
        with sync_playwright() as p:
            browser = p.chromium.launch(headless=True, channel=args.browser_channel)
            try:
                for scenario in SCENARIOS:
                    scenario(browser, port, checks)
                summary["status"] = "PASS"
            finally:
                browser.close()
    except Exception as exc:
        summary["error"] = f"{type(exc).__name__}: {exc}"
    finally:
        if server:
            server.shutdown()
            server.server_close()
        print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
    return 0 if summary["status"] == "PASS" else 1


def video_settings(browser, port, checks):
    """#35: every video settings control is wired and persisted."""
    state = State()
    posts = []
    context, page = new_page(browser, state, port)
    try:
        page.route("**/api/config", lambda route: (
            posts.append(json.loads(route.request.post_data or "{}")),
            route.fulfill(status=200, content_type="application/json", body="{}"))[-1])
        page.locator('[data-panel-target="video"]').click()
        form = page.locator("#video-config")
        form.locator('select[name="h264_bitrate_preset"]').select_option("12000")
        hidden = form.locator('input[name="h264_bitrate_kbps"]').input_value()
        check(checks, hidden == "12000", "bitrate_preset_updates_value", hidden)
        form.locator('select[name="h264_bitrate_preset"]').select_option("custom")
        visible = form.locator(".h264-bitrate-custom").is_visible()
        check(checks, visible, "custom_bitrate_field_shown", visible)
        form.locator('input[name="h264_bitrate_kbps"]').fill("3000")

        form.locator('select[name="display_scale"]').select_option("native")
        form.locator('select[name="rendering"]').select_option("smooth")
        classes = page.evaluate("() => document.querySelector('#console').className")
        check(checks, "mode-native" in classes and "render-pixelated" not in classes,
              "display_controls_apply_immediately", classes)

        form.locator('input[name="jpeg_quality"]').fill("55")
        shown = page.text_content("#quality-value")
        check(checks, shown == "55", "jpeg_quality_output_tracks_slider", shown)

        form.locator('button[type="submit"]').click()
        page.wait_for_timeout(300)
        bitrate = posts and posts[-1].get("video", {}).get("h264", {}).get("bitrate_kbps")
        check(checks, bitrate == 3000, "submitted_bitrate_matches_selection", posts[-1:] if posts else posts)

        page.reload(wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        classes = page.evaluate("() => document.querySelector('#console').className")
        check(checks, "mode-native" in classes and "render-pixelated" not in classes,
              "display_preferences_survive_reload", classes)
    finally:
        context.close()


def logout_and_errors(browser, port, checks):
    """#37 / #38 / #43: failed logout, HID errors and release failures surface."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.route("**/api/logout", lambda route: route.fulfill(
            status=500, content_type="application/json", body='{"error":"boom"}'))
        page.locator('[data-panel-target="control"]').click()
        page.click("#logout")
        page.wait_for_timeout(300)
        in_app = page.evaluate("() => !document.querySelector('#app').classList.contains('hidden')")
        check(checks, in_app, "failed_logout_keeps_console", in_app)
        toast = page.text_content("#toast") or ""
        check(checks, "boom" in toast, "failed_logout_reports_error", toast)

        page.route("**/api/key", lambda route: route.fulfill(
            status=503, content_type="application/json", body='{"error":"键盘设备未就绪"}'))
        page.click("#input-state")
        page.keyboard.press("b")
        page.wait_for_timeout(300)
        toast = page.text_content("#toast") or ""
        check(checks, "未就绪" in toast, "hid_not_ready_is_reported", toast)

        page.route("**/api/input/release-all", lambda route: route.fulfill(
            status=429, content_type="application/json", body='{"error":"队列已满"}'))
        page.click("#input-release")
        page.wait_for_timeout(300)
        toast = page.text_content("#toast") or ""
        check(checks, "队列已满" in toast, "release_failure_is_reported", toast)
    finally:
        context.close()


def ppv2_confirmation(browser, port, checks):
    """#28: switching the server PPv2 mode requires explicit confirmation."""
    state = State()
    posts = []
    context, page = new_page(browser, state, port)
    try:
        page.route("**/api/config", lambda route: (
            posts.append(route.request.post_data),
            route.fulfill(status=200, content_type="application/json",
                          body='{"server":{"proxy_protocol":"required"}}'))[-1])
        dialogs = []
        page.on("dialog", lambda d: (dialogs.append(d.message), d.dismiss()))
        page.click("#settings-toggle")
        page.select_option("#setting-proxy-protocol", "required")
        page.wait_for_timeout(200)
        check(checks, dialogs and "PPv2" in dialogs[0], "required_mode_asks_for_confirmation", dialogs)
        check(checks, not posts, "dismissed_confirmation_sends_nothing", posts)
        value = page.eval_on_selector("#setting-proxy-protocol", "el => el.value")
        check(checks, value != "required", "dismissed_confirmation_restores_select", value)
    finally:
        context.close()


def low_risk_ui(browser, port, checks):
    """#43: status failure, inspector default, stale responses, ARIA state."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    context, page = new_page(browser, state, port)
    try:
        page.evaluate("() => document.querySelector('#video-feed').dispatchEvent(new Event('load'))")
        page.wait_for_timeout(3500)
        healthy = page.evaluate(
            "() => !document.querySelector('#video-viewport').classList.contains('video-unhealthy')")
        check(checks, healthy, "video_ready_before_status_failure", healthy)
        page.route("**/api/status", lambda route: route.fulfill(status=500, body="{}"))
        page.wait_for_timeout(3500)
        unhealthy = page.evaluate(
            "() => document.querySelector('#video-viewport').classList.contains('video-unhealthy')")
        check(checks, not unhealthy, "status_failure_keeps_video_visible", unhealthy)

        pressed = page.get_attribute("#input-state", "aria-pressed")
        page.click("#input-state")
        check(checks, pressed == "false" and page.get_attribute("#input-state", "aria-pressed") == "true",
              "input_toggle_exposes_aria_pressed", pressed)
        role = page.get_attribute("#input-state", "role")
        check(checks, role is None, "input_toggle_keeps_button_role", role)
        page.locator('[data-panel-target="media"]').click()
        selected = page.get_attribute('[data-panel-target="media"]', "aria-selected")
        check(checks, selected == "true", "inspector_tab_aria_selected", selected)
    finally:
        context.close()

    state = State()
    context = browser.new_context(viewport={"width": 1400, "height": 900})
    try:
        context.add_init_script(
            "localStorage.setItem('wingman_setting_auto_expand_inspector','false')")
        page = context.new_page()
        from verify_p2b_ui import api_route, GIF, bootstrap_payload
        page.route("**/api/**", lambda route: api_route(route, state))
        page.route("**/video_feed*", lambda route: route.fulfill(
            status=200, content_type="image/gif", body=GIF))
        page.goto(f"http://127.0.0.1:{port}/", wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        open_ = page.evaluate("() => document.querySelector('#app').classList.contains('inspector-open')")
        check(checks, not open_, "auto_expand_setting_respected", open_)

        # Two scans where the first answers last: the newer result must win.
        replies = []
        def scan(route):
            replies.append(route)
        page.route("**/api/devices/scan", scan)
        page.evaluate("() => document.querySelector('#scan-devices').click()")
        page.evaluate("() => document.querySelector('#scan-devices').click()")
        page.wait_for_timeout(200)
        newer = {"video": [{"path": "/dev/video-new", "label": "newer"}]}
        older = {"video": [{"path": "/dev/video-old", "label": "older"}]}
        replies[-1].fulfill(status=200, content_type="application/json", body=json.dumps(newer))
        page.wait_for_timeout(100)
        replies[0].fulfill(status=200, content_type="application/json", body=json.dumps(older))
        page.wait_for_timeout(200)
        diag = page.text_content("#device-diagnostics") or ""
        check(checks, "video-new" in diag and "video-old" not in diag,
              "stale_scan_response_is_ignored", diag[:120])
    finally:
        context.close()


def language_coverage(browser, port, checks):
    """#39: switching to English translates static labels and help text."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#lang-toggle")
        page.click('[data-lang="en"]')
        page.wait_for_timeout(200)
        leftover = page.evaluate("""() => {
          const cjk = /[\\u4e00-\\u9fff]/;
          const out = [];
          const walk = document.createTreeWalker(document.querySelector('#inspector'), NodeFilter.SHOW_TEXT);
          for (let n = walk.nextNode(); n; n = walk.nextNode()) {
            if (cjk.test(n.nodeValue) && n.parentElement.offsetParent !== null) out.push(n.nodeValue.trim());
          }
          return out;
        }""")
        check(checks, not leftover, "visible_inspector_text_translated", leftover[:8])
        help_text = page.get_attribute('#video-config [name="transport"]', "name") and page.evaluate(
            "() => document.querySelector('#video-config .field-help').dataset.help")
        check(checks, help_text and not any("\u4e00" <= c <= "\u9fff" for c in help_text),
              "help_text_translated", help_text)
        # User data that happens to equal a known phrase must stay verbatim.
        page.evaluate("""() => {
          const name = document.createElement('strong');
          name.className = 'media-item-name';
          name.textContent = '自动';
          document.querySelector('#media-list').append(name);
        }""")
        page.wait_for_timeout(100)
        kept = page.evaluate("() => document.querySelector('#media-list .media-item-name').textContent")
        check(checks, kept == "自动", "user_data_not_translated", kept)
        page.click("#lang-toggle")
        page.click('[data-lang="zh-CN"]')
        page.wait_for_timeout(200)
        label = page.text_content('[data-panel="video"] .field span') or ""
        check(checks, "播放方式" in label, "switching_back_restores_source_text", label)
    finally:
        context.close()



def power_confirmation(browser, port, checks):
    """#52: power actions use in-app confirmation; force-off needs a hold."""
    state = State()
    posts = []
    context, page = new_page(browser, state, port)
    try:
        from verify_p2b_ui import bootstrap_payload, api_route
        payload = bootstrap_payload(state)
        payload["capabilities"].update(gpio_power=True, gpio_reset=True)
        page.route("**/api/bootstrap", lambda route: route.fulfill(
            status=200, content_type="application/json", body=json.dumps(payload)))
        for path in ("/power", "/reset"):
            page.route(f"**{path}", lambda route, *_, endpoint=path: (
                posts.append((endpoint, route.request.post_data)),
                route.fulfill(status=200, content_type="application/json", body='{"ok":true}'))[-1])
        page.reload(wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        dialogs = []
        page.on("dialog", lambda d: (dialogs.append(d.message), d.dismiss()))

        page.locator('#panel-control [data-power-action="force-off"]').click()
        page.wait_for_selector("#power-confirm[open]", timeout=3000)
        focused = page.evaluate("() => document.activeElement.value")
        check(checks, focused == "cancel", "power_confirm_focuses_cancel", focused)
        page.click("#power-confirm-ok")
        page.wait_for_timeout(300)
        check(checks, not posts, "force_off_click_without_hold_does_nothing", posts)
        box = page.locator("#power-confirm-ok").bounding_box()
        page.mouse.move(box["x"] + 10, box["y"] + 10)
        page.mouse.down()
        page.wait_for_timeout(1200)
        page.mouse.up()
        page.wait_for_timeout(300)
        check(checks, posts and posts[-1][0] == "/power" and "duration=5" in (posts[-1][1] or ""),
              "force_off_hold_sends_long_press", posts)
        check(checks, not dialogs, "no_native_confirm_dialogs", dialogs)

        posts.clear()
        page.locator('#panel-control [data-power-action="reset"]').click()
        page.wait_for_selector("#power-confirm[open]", timeout=3000)
        page.click("#power-confirm-ok")
        page.wait_for_timeout(300)
        check(checks, posts and posts[-1][0] == "/reset", "reset_confirm_click_sends_reset", posts)
    finally:
        context.close()


def paste_as_typing(browser, port, checks):
    """#51 / UX-03: pasted text is typed as US-layout keystrokes."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#input-state")
        page.click("#keyboard-toggle")
        page.click("#paste-typing-btn")
        page.fill("#paste-key-text", "Ab1!\n")
        page.click("#paste-key-send")
        page.wait_for_timeout(700)
        sequence = [(post.get("key"), bool(post.get("shift"))) for post in state.key_posts]
        expected = [("KeyA", True), ("KeyB", False), ("Digit1", False),
                    ("Digit1", True), ("Enter", False)]
        check(checks, sequence == expected, "paste_typing_sends_us_sequence", sequence)

        state.key_posts.clear()
        page.fill("#paste-key-text", "中文")
        page.click("#paste-key-send")
        page.wait_for_timeout(300)
        check(checks, not state.key_posts, "paste_typing_blocks_unsupported_chars",
              state.key_posts)
        hint = page.text_content("#paste-key-unsupported") or ""
        check(checks, "中" in hint and "文" in hint,
              "paste_typing_lists_unsupported_chars", hint)

        # With Caps Lock on at the target, letter case must still come out right.
        state.key_posts.clear()
        from verify_p2b_ui import api_route, bootstrap_payload
        class CapsOnRoute:
            """Wraps a route so the mocked status reports Caps Lock on."""
            def __init__(self, route):
                self.route, self.request = route, route.request
            def fulfill(self, **kwargs):
                body = kwargs.get("body", "").replace('"caps_lock": false', '"caps_lock": true')
                return self.route.fulfill(**{**kwargs, "body": body})
        def caps_on(route):
            return api_route(CapsOnRoute(route), state)
        page.unroute("**/api/**")
        page.route("**/api/**", caps_on)
        page.wait_for_timeout(3500)
        page.fill("#paste-key-text", "Ab")
        page.click("#paste-key-send")
        page.wait_for_timeout(500)
        sequence = [(post.get("key"), bool(post.get("shift"))) for post in state.key_posts]
        check(checks, sequence == [("KeyA", False), ("KeyB", True)], "paste_typing_respects_caps_lock", sequence)
        page.unroute("**/api/**")
        page.route("**/api/**", lambda route: api_route(route, state))
        page.wait_for_timeout(3500)

        # Turning forwarding off mid-paste stops the run and reports it.
        state.key_posts.clear()
        page.fill("#paste-key-text", "a" * 200)
        page.click("#paste-key-send")
        page.wait_for_timeout(300)
        page.locator("#input-state").evaluate("b => b.click()")
        page.wait_for_timeout(600)
        sent = len(state.key_posts)
        status = page.text_content("#paste-key-status") or ""
        check(checks, 0 < sent < 200 and "已中断" in status, "paste_typing_stops_when_forwarding_off",
              (sent, status))
    finally:
        context.close()


def target_keyboard_layouts(browser, port, checks):
    """#63: paste-as-typing honors the selected target keyboard layout."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#input-state")

        def strokes(text, layout):
            return page.evaluate(
                """([text, layout]) => window.__wingmanTest
                    .textToKeystrokes(text, layout).strokes
                    .map(s => [s.key, !!s.shift, !!s.ctrl, !!s.alt])""",
                [text, layout])

        layouts = page.evaluate("() => window.__wingmanTest.targetLayouts")
        check(checks, layouts == ["us", "uk", "de", "fr", "jp"],
              "target_layouts_available", layouts)
        check(checks, page.evaluate("() => window.__wingmanTest.targetLayout()") == "us",
              "target_layout_defaults_to_us", page.evaluate(
                  "() => localStorage.getItem('wingman_setting_target_layout')"))

        de = strokes("zy@ä", "de")
        check(checks, de == [["KeyY", False, False, False],
                             ["KeyZ", False, False, False],
                             ["KeyQ", False, True, True],
                             ["Quote", False, False, False]],
              "target_layout_de_maps_qwertz_and_altgr", de)
        fr = strokes("a1", "fr")
        check(checks, fr == [["KeyQ", False, False, False],
                             ["Digit1", True, False, False]],
              "target_layout_fr_maps_azerty_and_shifted_digits", fr)
        uk = strokes('"', "uk")
        check(checks, uk == [["Digit2", True, False, False]],
              "target_layout_uk_maps_quote_to_shift_digit2", uk)
        jp = strokes("@[:;]", "jp")
        check(checks, jp == [["BracketLeft", False, False, False],
                             ["BracketRight", False, False, False],
                             ["Quote", False, False, False],
                             ["Semicolon", False, False, False],
                             ["Backslash", False, False, False]],
              "target_layout_jp_maps_jis_symbol_positions", jp)

        # The preference is persisted and the dialog names the active layout.
        page.click("#settings-toggle")
        options = page.eval_on_selector(
            "#setting-target-layout", "el => [...el.options].map(o => o.value)")
        check(checks, options == ["us", "uk", "de", "fr", "jp"],
              "target_layout_settings_options", options)
        page.select_option("#setting-target-layout", "de")
        page.wait_for_timeout(100)
        stored = page.evaluate(
            "() => localStorage.getItem('wingman_setting_target_layout')")
        check(checks, stored == "de", "target_layout_setting_persists", stored)
        page.click("[data-settings-close]")
        page.click("#keyboard-toggle")
        page.click("#paste-typing-btn")
        label = page.text_content("#paste-key-layout") or ""
        check(checks, "德语" in label, "paste_dialog_shows_target_layout", label)
        page.fill("#paste-key-text", "zy@ä")
        page.click("#paste-key-send")
        page.wait_for_timeout(700)
        sequence = [(post.get("key"), bool(post.get("shift")),
                     bool(post.get("ctrl")), bool(post.get("alt")))
                    for post in state.key_posts]
        expected = [("KeyY", False, False, False), ("KeyZ", False, False, False),
                    ("KeyQ", False, True, True), ("Quote", False, False, False)]
        check(checks, sequence == expected, "paste_typing_uses_selected_layout", sequence)
    finally:
        context.close()


def custom_shortcuts(browser, port, checks):
    """#50 / UX-02: custom combinations persist and send the right payload."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        # The combos menu stays reachable even when the console is narrow.
        page.click("#quick-more-toggle")
        common = page.locator(".quick-menu-common").is_visible()
        check(checks, common, "narrow_console_menu_lists_common_keys", common)
        page.set_viewport_size({"width": 900, "height": 900})
        page.wait_for_timeout(200)
        hidden = page.evaluate("""() => [...document.querySelectorAll('#quick-more-menu button')]
          .filter(b => getComputedStyle(b).display === 'none').map(b => b.textContent)""")
        check(checks, not hidden, "narrow_viewport_menu_items_all_visible", hidden)
        page.set_viewport_size({"width": 1280, "height": 900})
        page.click("#quick-combo-add")
        page.check('#combo-form input[name="ctrl"]')
        page.check('#combo-form input[name="alt"]')
        page.select_option("#combo-key", "KeyM")
        page.click('#combo-form button[type="submit"]')
        page.wait_for_timeout(200)
        stored = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_custom_combos') || '[]'"))
        saved = stored[0] if stored else {}
        check(checks, saved.get("key") == "KeyM" and saved.get("ctrl") and saved.get("alt"),
              "custom_shortcut_saved_to_local_storage", stored)

        page.reload(wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.click("#quick-more-toggle")
        item = page.locator("#quick-more-menu [data-custom-combo]").first
        check(checks, item.count() == 1, "custom_shortcut_survives_reload", item.count())

        state.key_posts.clear()
        item.click()
        page.wait_for_timeout(400)
        payload = state.key_posts[-1] if state.key_posts else {}
        check(checks, payload.get("key") == "KeyM" and payload.get("ctrl") is True
              and payload.get("alt") is True and payload.get("shift") is False
              and payload.get("meta") is False,
              "custom_shortcut_sends_correct_payload", payload)

        page.click("#quick-more-toggle")
        page.click("#quick-more-menu [data-remove-combo]")
        page.wait_for_timeout(200)
        remaining = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_custom_combos') || '[]'"))
        check(checks, remaining == [], "custom_shortcut_can_be_deleted", remaining)
    finally:
        context.close()


def media_drop_upload(browser, port, checks):
    """#54: dropping an image onto the media panel uploads it; others rejected."""
    state = State()
    uploads = []
    context, page = new_page(browser, state, port)
    try:
        page.route("**/api/media/upload", lambda route: (
            uploads.append(route.request.post_data_buffer or b""),
            route.fulfill(status=200, content_type="application/json", body='{"name":"x.iso","size":4}'))[-1])
        page.locator('[data-panel-target="media"]').click()
        # Playwright drops file parts from captured multipart bodies, so record
        # what the page actually hands to XHR.
        page.evaluate("""() => {
          const send = XMLHttpRequest.prototype.send;
          window.__sentFiles = [];
          XMLHttpRequest.prototype.send = function (body) {
            if (body instanceof FormData)
              for (const [, value] of body) if (value instanceof File) window.__sentFiles.push(value.name);
            return send.call(this, body);
          };
        }""")
        drop = """(name) => {
          const transfer = new DataTransfer();
          transfer.items.add(new File(['data'], name));
          const panel = document.querySelector('#panel-media');
          panel.dispatchEvent(new DragEvent('dragover', {dataTransfer: transfer, bubbles: true, cancelable: true}));
          panel.dispatchEvent(new DragEvent('drop', {dataTransfer: transfer, bubbles: true, cancelable: true}));
        }"""
        page.evaluate(drop, "notes.txt")
        page.wait_for_timeout(300)
        toast = page.text_content("#toast") or ""
        check(checks, not uploads and "ISO" in toast, "drop_rejects_non_image", (uploads, toast))
        page.evaluate(drop, "installer.iso")
        page.wait_for_timeout(800)
        sent = page.evaluate("() => window.__sentFiles")
        check(checks, uploads and sent == ["installer.iso"], "drop_uploads_image", sent)
        hidden = page.evaluate("() => document.querySelector('#upload-cancel').classList.contains('hidden')")
        check(checks, hidden, "cancel_button_hidden_after_upload", hidden)
    finally:
        context.close()


def inline_save_feedback(browser, port, checks):
    """#55: saving a settings form shows its result next to the button."""
    state = State()
    replies = {"status": 200}
    context, page = new_page(browser, state, port)
    try:
        page.route("**/api/config", lambda route: route.fulfill(
            status=replies["status"], content_type="application/json",
            body='{}' if replies["status"] == 200 else '{"error":"LUN 路径不存在"}'))
        page.locator('[data-panel-target="media"]').click()
        page.evaluate("() => document.querySelectorAll('#panel-media details').forEach(d => d.open = true)")
        page.click('#media-config button[type="submit"]')
        page.wait_for_timeout(300)
        status = page.text_content("#media-config .form-status") or ""
        check(checks, "已保存" in status, "save_success_shown_inline", status)
        replies["status"] = 500
        page.click('#media-config button[type="submit"]')
        page.wait_for_timeout(300)
        status = page.text_content("#media-config .form-status") or ""
        error = page.evaluate("() => document.querySelector('#media-config .form-status').classList.contains('error')")
        check(checks, "LUN 路径不存在" in status and error, "save_error_shown_inline", status)
    finally:
        context.close()


def shortcut_sheet(browser, port, checks):
    """#57: "?" opens the shortcut cheat sheet only while input is local."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.locator("#video-viewport").focus()
        page.keyboard.press("Shift+Slash")
        page.wait_for_timeout(200)
        opened = page.evaluate("() => document.querySelector('#shortcut-dialog').open")
        check(checks, opened, "question_mark_opens_sheet", opened)
        page.keyboard.press("Escape")
        page.wait_for_timeout(200)
        inspector = page.evaluate("() => document.querySelector('#app').classList.contains('inspector-open')")
        check(checks, inspector, "esc_closing_dialog_keeps_inspector_open", inspector)
        page.click("#input-state")
        before = len(state.key_posts)
        page.keyboard.press("Shift+Slash")
        page.wait_for_timeout(200)
        opened = page.evaluate("() => document.querySelector('#shortcut-dialog').open")
        check(checks, not opened and len(state.key_posts) > before,
              "question_mark_forwarded_while_controlling", (opened, state.key_posts[before:]))
    finally:
        context.close()


def touch_fullscreen_toolbar(browser, port, checks):
    """#58: touch devices get a floating toolbar in fullscreen."""
    from verify_p2b_ui import api_route, GIF, bootstrap_payload
    state = State()
    context = browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True, has_touch=True)
    try:
        page = context.new_page()
        page.route("**/api/**", lambda route: api_route(route, state))
        page.route("**/video_feed*", lambda route: route.fulfill(status=200, content_type="image/gif", body=GIF))
        page.goto(f"http://127.0.0.1:{port}/", wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        hint = page.locator("#local-exit-hint").is_visible()
        check(checks, not hint, "keyboard_hint_hidden_on_touch", hint)
        page.tap("#fullscreen")
        page.wait_for_timeout(500)
        if not page.evaluate("() => !!document.fullscreenElement"):
            check(checks, True, "touch_toolbar_fullscreen_unavailable", "headless fullscreen refused; not claimed")
            return
        visible = page.locator("#touch-toolbar").is_visible()
        check(checks, visible, "touch_toolbar_visible_in_fullscreen", visible)
        page.tap('[data-touch-action="input"]')
        page.wait_for_timeout(200)
        pressed = page.get_attribute("#input-state", "aria-pressed")
        label = page.text_content('[data-touch-action="input"]')
        check(checks, pressed == "true" and label == "释放", "touch_toolbar_toggles_control", (pressed, label))
        page.tap('[data-touch-action="exit"]')
        page.wait_for_timeout(400)
        still = page.evaluate("() => !!document.fullscreenElement")
        check(checks, not still, "touch_toolbar_exits_fullscreen", still)
    finally:
        context.close()

    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#fullscreen")
        page.wait_for_timeout(400)
        visible = page.locator("#touch-toolbar").is_visible()
        check(checks, not visible, "touch_toolbar_hidden_for_mouse", visible)
    finally:
        context.close()



def input_websocket(browser, port, checks):
    """#61: HID events use the input WebSocket and fall back to HTTP."""
    from verify_p2b_ui import api_route, GIF, bootstrap_payload
    state = State()
    frames, sockets = [], []
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    try:
        page = context.new_page()

        def on_socket(ws):
            sockets.append(ws)
            def reply(message):
                body = json.loads(message)
                frames.append(body)
                ws.send(json.dumps({"id": body["id"], "ok": True}))
            ws.on_message(reply)
        page.route_web_socket("**/api/input/ws", on_socket)
        page.route("**/api/**", lambda route: api_route(route, state))
        page.route("**/video_feed*", lambda route: route.fulfill(status=200, content_type="image/gif", body=GIF))
        page.goto(f"http://127.0.0.1:{port}/", wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.wait_for_timeout(300)
        page.click("#input-state")
        page.keyboard.press("a")
        page.wait_for_timeout(300)
        over_socket = [f for f in frames if f.get("type") == "key"]
        check(checks, over_socket and over_socket[-1].get("key") == "KeyA" and not state.key_posts,
              "keys_travel_over_input_websocket", (frames[-3:], state.key_posts))
        page.click("#diagnostics-toggle")
        mode = page.text_content("#diag-input-mode") or ""
        check(checks, "WebSocket" in mode, "diagnostics_shows_websocket_transport", mode)
        page.click("#diagnostics-toggle")

        # Unload-time release must use HTTP keepalive, not the socket.
        before = state.release_posts
        page.evaluate("() => window.dispatchEvent(new PageTransitionEvent('pagehide'))")
        page.wait_for_timeout(300)
        socket_releases = [f for f in frames if f.get("type") == "release_all"]
        check(checks, state.release_posts > before and not socket_releases,
              "unload_release_uses_http_keepalive", (state.release_posts - before, socket_releases))
        page.evaluate("() => window.dispatchEvent(new PageTransitionEvent('pageshow'))")
        page.wait_for_timeout(200)

        sockets[-1].close()
        page.wait_for_timeout(200)
        page.locator("#video-viewport").focus()
        page.keyboard.press("b")
        page.wait_for_timeout(300)
        check(checks, any(p.get("key") == "KeyB" for p in state.key_posts),
              "keys_fall_back_to_http_when_socket_closes", state.key_posts)
    finally:
        context.close()


def touch_gestures(browser, port, checks):
    """#62: tap, long press, one-finger drag and two-finger scroll."""
    from verify_p2b_ui import api_route, GIF, bootstrap_payload
    state = State()
    # A visible, playing feed is needed to map touch points to the target.
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    posts = []
    context = browser.new_context(viewport={"width": 1280, "height": 900}, has_touch=True)
    try:
        page = context.new_page()
        def record(route):
            path = route.request.url.split("/api/", 1)[1]
            if path.startswith("mouse/"):
                posts.append((path, json.loads(route.request.post_data or "{}")))
                return route.fulfill(status=204)
            return api_route(route, state)
        page.route("**/api/**", record)
        page.route("**/video_feed*", lambda route: route.fulfill(status=200, content_type="image/gif", body=GIF))
        page.goto(f"http://127.0.0.1:{port}/", wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        # Give the mocked 1x1 feed a real size so absolute mapping succeeds.
        page.evaluate("""() => {
          const feed = document.querySelector('#video-feed');
          Object.defineProperty(feed, 'naturalWidth', {value: 1280});
          Object.defineProperty(feed, 'naturalHeight', {value: 720});
          // The stub GIF is 1x1; give the element a realistic rendered size.
          feed.style.width = '640px';
          feed.style.height = '360px';
          feed.dispatchEvent(new Event('load'));
        }""")
        page.wait_for_timeout(300)
        touch = """([type, id, x, y]) => {
          const v = document.querySelector('#video-viewport');
          // Coordinates are relative to the rendered video, where touches map.
          const r = document.querySelector('#video-feed').getBoundingClientRect();
          v.dispatchEvent(new PointerEvent(type, {pointerId: id, pointerType: 'touch', isPrimary: id === 1,
            clientX: r.left + x, clientY: r.top + y, bubbles: true, cancelable: true, button: 0}));
        }"""
        page.click("#input-state")
        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 100, 100])
        page.evaluate(touch, ["pointerup", 1, 100, 100])
        page.wait_for_timeout(200)
        clicks = [b for p, b in posts if b.get("action") == "click" or p == "mouse/click"]
        check(checks, clicks and clicks[-1].get("button") == 1, "touch_tap_is_left_click", posts)

        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 120, 120])
        page.wait_for_timeout(700)
        page.evaluate(touch, ["pointerup", 1, 120, 120])
        page.wait_for_timeout(200)
        clicks = [b for p, b in posts if b.get("action") == "click" or p == "mouse/click"]
        check(checks, len(clicks) == 1 and clicks[0].get("button") == 2, "touch_long_press_is_right_click", posts)

        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 100, 100])
        for step in range(1, 6):
            page.evaluate(touch, ["pointermove", 1, 100 + step * 20, 100])
            page.wait_for_timeout(30)
        page.evaluate(touch, ["pointerup", 1, 200, 100])
        page.wait_for_timeout(200)
        moves = [b for p, b in posts if b.get("action") == "move"]
        clicks = [b for p, b in posts if b.get("action") == "click"]
        check(checks, moves and not clicks, "touch_drag_moves_without_clicking", posts[-4:])

        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 100, 300])
        page.evaluate(touch, ["pointerdown", 2, 160, 300])
        for step in range(1, 5):
            page.evaluate(touch, ["pointermove", 1, 100, 300 - step * 20])
            page.evaluate(touch, ["pointermove", 2, 160, 300 - step * 20])
        page.evaluate(touch, ["pointerup", 1, 100, 220])
        page.evaluate(touch, ["pointerup", 2, 160, 220])
        page.wait_for_timeout(200)
        scrolls = [b for p, b in posts if b.get("action") == "scroll"]
        clicks = [b for p, b in posts if b.get("action") == "click"]
        check(checks, scrolls and all(b["delta"] < 0 for b in scrolls) and not clicks,
              "two_finger_drag_scrolls", posts)

        # Lifting one finger mid-scroll must not burst wheel steps or drag.
        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 100, 300])
        page.evaluate(touch, ["pointerdown", 2, 300, 300])
        page.evaluate(touch, ["pointerup", 2, 300, 300])
        for step in range(1, 4):
            page.evaluate(touch, ["pointermove", 1, 100, 300 - step * 40])
        page.evaluate(touch, ["pointerup", 1, 100, 180])
        page.wait_for_timeout(200)
        check(checks, not posts, "lifting_a_finger_ends_scroll_quietly", posts)

        # The finger that takes control must not also drag the pointer.
        page.click("#input-state")
        posts.clear()
        page.evaluate(touch, ["pointerdown", 1, 100, 100])
        for step in range(1, 4):
            page.evaluate(touch, ["pointermove", 1, 100 + step * 30, 100])
        page.evaluate(touch, ["pointerup", 1, 190, 100])
        page.wait_for_timeout(200)
        enabled = page.get_attribute("#input-state", "aria-pressed")
        check(checks, enabled == "true" and not posts, "takeover_touch_sends_nothing", (enabled, posts))
    finally:
        context.close()


def runtime_metrics(browser, port, checks):
    """#67: diagnostics show version, connections and keyboard/mouse write health."""
    from verify_p2b_ui import api_route, bootstrap_payload
    state = State()
    context, page = new_page(browser, state, port)
    try:
        class WithRuntime:
            def __init__(self, route):
                self.route, self.request = route, route.request
            def fulfill(self, **kwargs):
                body = json.loads(kwargs.get("body") or "{}")
                body["runtime"] = {"version": "0.1.0", "uptime_seconds": 3700, "input_sockets": 1,
                                   "hid": {"ok": 42, "failed": 3, "last_error": "键盘写入超时",
                                           "last_error_age_seconds": 65}}
                body["webrtc"] = {"sessions": 0}
                return self.route.fulfill(**{**kwargs, "body": json.dumps(body)})
        page.unroute("**/api/**")
        page.route("**/api/**", lambda route: api_route(
            WithRuntime(route) if route.request.url.endswith("/api/status") else route, state))
        page.wait_for_timeout(3500)
        page.click("#diagnostics-toggle")
        hid = page.text_content("#diag-hid") or ""
        version = page.text_content("#diag-version") or ""
        check(checks, "42" in hid and "3" in hid and "键盘写入超时" in hid and "v0.1.0" in version,
              "diagnostics_show_runtime_metrics", (hid, version))
        page.click("#diagnostics-toggle")
        page.click("#lang-toggle")
        page.click('[data-lang="en"]')
        page.wait_for_timeout(300)
        page.click("#diagnostics-toggle")
        hid = page.text_content("#diag-hid") or ""
        check(checks, hid.startswith("OK 42 · failed 3 · last error:"), "runtime_metrics_translate", hid)
    finally:
        context.close()

def screenshot_and_record(browser, port, checks):
    """#70: screenshot downloads a PNG; recording downloads a WebM."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    context, page = new_page(browser, state, port)
    try:
        # The terminal tab pauses video: nothing live to capture.
        page.click('[data-workspace="terminal"]')
        page.wait_for_timeout(200)
        page.evaluate("() => document.querySelector('#screenshot').click()")
        page.wait_for_timeout(200)
        toast = page.text_content("#toast") or ""
        check(checks, "没有可截取" in toast, "screenshot_requires_live_picture", toast)
        page.click('[data-workspace="video"]')
        page.wait_for_timeout(300)
        page.evaluate("() => document.querySelector('#video-feed').dispatchEvent(new Event('load'))")
        page.wait_for_timeout(300)
        with page.expect_download(timeout=5000) as shot:
            page.click("#screenshot")
        name = shot.value.suggested_filename
        data = open(shot.value.path(), "rb").read(8)
        check(checks, name.endswith(".png") and data.startswith(b"\x89PNG"), "screenshot_downloads_png", name)

        page.click("#record")
        page.wait_for_timeout(1600)
        pressed = page.get_attribute("#record", "aria-pressed")
        with page.expect_download(timeout=8000) as clip:
            page.click("#record")
        size = len(open(clip.value.path(), "rb").read())
        check(checks, pressed == "true" and clip.value.suggested_filename.endswith(".webm") and size > 0,
              "recording_downloads_webm", (pressed, clip.value.suggested_filename, size))
        check(checks, page.get_attribute("#record", "aria-pressed") == "false", "recording_stops_on_second_click",
              page.get_attribute("#record", "aria-pressed"))
    finally:
        context.close()


def terminal_kept_in_background(browser, port, checks):
    """#47: switching to video keeps the terminal session and marks its tab."""
    from verify_p2b_ui import api_route, bootstrap_payload, GIF
    state = State()
    payload = bootstrap_payload(state)
    payload["terminal_ready"] = True
    sockets = []
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    try:
        page = context.new_page()
        page.route_web_socket("**/api/terminal/ws", lambda ws: sockets.append(ws))
        page.route("**/api/bootstrap", lambda route: route.fulfill(
            status=200, content_type="application/json", body=json.dumps(payload)))
        page.route("**/api/**", lambda route: api_route(route, state))
        page.route("**/video_feed*", lambda route: route.fulfill(status=200, content_type="image/gif", body=GIF))
        page.goto(f"http://127.0.0.1:{port}/", wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.click('[data-workspace="terminal"]')
        page.wait_for_timeout(500)
        page.click('[data-workspace="video"]')
        page.wait_for_timeout(300)
        live = page.evaluate("() => document.querySelector('[data-workspace=\"terminal\"]').classList.contains('session-live')")
        check(checks, len(sockets) == 1 and live, "terminal_session_kept_and_marked", (len(sockets), live))
        page.click('[data-workspace="terminal"]')
        page.wait_for_timeout(300)
        live = page.evaluate("() => document.querySelector('[data-workspace=\"terminal\"]').classList.contains('session-live')")
        check(checks, len(sockets) == 1 and not live, "returning_reuses_session_and_clears_mark", (len(sockets), live))
    finally:
        context.close()


def sessions_and_audit(browser, port, checks):
    """#68: session list, revocation controls and the audit log render."""
    state = State()
    context, page = new_page(browser, state, port)
    now_ms = int(time.time() * 1000)
    sessions = [
        {"id": "pub-current", "created_at_unix_ms": now_ms - 3_600_000,
         "last_seen_unix_ms": now_ms - 5_000, "client_ip": "203.0.113.5",
         "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                       "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
         "login_method": "password", "current": True},
        {"id": "pub-other", "created_at_unix_ms": now_ms - 7_200_000,
         "last_seen_unix_ms": now_ms - 120_000, "client_ip": "198.51.100.7",
         "user_agent": "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) "
                       "AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile Safari/604.1",
         "login_method": "passkey", "current": False},
    ]
    audit_entries = [
        {"ts": now_ms - 2_000, "event": "login_succeeded", "ip": "203.0.113.5",
         "session": "pub-current", "detail": "method=password"},
        {"ts": now_ms - 60_000, "event": "power_action", "ip": "198.51.100.7",
         "session": "pub-other", "detail": "press"},
    ]
    calls = {"sessions": 0, "audit": 0, "revoke": [], "others": 0}

    def sessions_route(route):
        calls["sessions"] += 1
        route.fulfill(status=200, content_type="application/json",
                      body=json.dumps({"sessions": sessions}))

    def audit_route(route):
        calls["audit"] += 1
        route.fulfill(status=200, content_type="application/json",
                      body=json.dumps({"entries": audit_entries}))

    def revoke_route(route):
        calls["revoke"].append(json.loads(route.request.post_data or "{}").get("id"))
        route.fulfill(status=200, content_type="application/json", body='{"ok":true}')

    def others_route(route):
        calls["others"] += 1
        route.fulfill(status=200, content_type="application/json",
                      body='{"ok":true,"revoked":1}')

    page.route("**/api/sessions", sessions_route)
    page.route("**/api/sessions/revoke", revoke_route)
    page.route("**/api/sessions/revoke-others", others_route)
    page.route("**/api/audit*", audit_route)
    try:
        page.on("dialog", lambda dialog: dialog.accept())
        page.click("#settings-toggle")
        page.wait_for_selector("#session-items [data-session-id]", timeout=5000)
        rows = page.evaluate("""() => [...document.querySelectorAll(
          '#session-items [data-session-id]')].map(el => ({
            id: el.dataset.sessionId,
            current: el.dataset.sessionCurrent,
            text: el.textContent || '',
            button: el.querySelector('button[data-session-revoke]')?.textContent || ''
          }))""")
        check(checks, len(rows) == 2, "session_list_renders_both_devices", rows)
        current = next((row for row in rows if row["id"] == "pub-current"), {})
        other = next((row for row in rows if row["id"] == "pub-other"), {})
        check(checks, current.get("current") == "true" and "当前会话" in current.get("text", ""),
              "current_session_is_marked", current)
        check(checks, current.get("button") == "退出登录", "current_session_button_is_logout",
              current.get("button"))
        check(checks, other.get("current") == "false" and other.get("button") == "下线",
              "other_session_button_is_revoke", other)
        check(checks, "203.0.113.5" in current.get("text", "")
              and "密码登录" in current.get("text", "")
              and "macOS" in current.get("text", ""),
              "session_row_shows_ip_method_and_device", current.get("text"))

        before = calls["sessions"]
        page.click('#session-items [data-session-id="pub-other"] button[data-session-revoke]')
        page.wait_for_timeout(400)
        check(checks, calls["revoke"] == ["pub-other"], "revoke_sends_the_selected_id", calls)
        check(checks, calls["sessions"] > before, "revoke_refreshes_the_session_list", calls)

        page.click("#sessions-revoke-others")
        page.wait_for_timeout(400)
        check(checks, calls["others"] == 1, "revoke_others_sends_the_request", calls)

        page.click("#audit-details summary")
        page.wait_for_selector("#audit-items .audit-item", timeout=5000)
        entries = page.evaluate("""() => [...document.querySelectorAll(
          '#audit-items .audit-item')].map(el => el.textContent || '')""")
        check(checks, len(entries) == 2, "audit_entries_render", entries)
        check(checks, "登录成功" in entries[0] and "203.0.113.5" in entries[0]
              and "method=password" in entries[0],
              "audit_entry_shows_event_ip_and_detail", entries[0])
        check(checks, "电源操作" in entries[1] and "press" in entries[1],
              "audit_second_entry_renders", entries[1])
    finally:
        context.close()


def media_url_download(browser, port, checks):
    """#69: URL download request, progress, list refresh and cancellation."""
    state = State()
    downloads, cancels, media_calls = [], [], []
    mode = {"id": "job-success", "phase": "running", "polls": 0}
    context, page = new_page(browser, state, port)
    try:
        def media_list(route):
            media_calls.append(route.request.url)
            route.fulfill(status=200, content_type="application/json", body=json.dumps({
                "images": [{"name": "downloaded.iso", "size": 4194304, "mounted": False}],
                "status": {"state": "detached"}}))

        def download(route):
            path = urllib.parse.urlsplit(route.request.url).path
            method = route.request.method
            if path == "/api/media/download" and method == "POST":
                downloads.append(json.loads(route.request.post_data or "{}"))
                mode.update(id="job-success" if len(downloads) == 1 else "job-cancel",
                            phase="running", polls=0)
                body = {"id": mode["id"], "state": "running", "received": 0,
                        "total": 4194304, "error": None, "name": "downloaded.iso"}
                return route.fulfill(status=202, content_type="application/json",
                                     body=json.dumps(body))
            if path == "/api/media/download/cancel":
                cancels.append(route.request.post_data)
                mode["phase"] = "cancelled"
                body = {"id": mode["id"], "state": "running", "received": 1048576,
                        "total": 4194304, "error": None, "name": "downloaded.iso"}
                return route.fulfill(status=200, content_type="application/json",
                                     body=json.dumps(body))
            if mode["phase"] == "cancelled":
                body = {"id": mode["id"], "state": "cancelled", "received": 1048576,
                        "total": 4194304, "error": None, "name": "downloaded.iso"}
            elif mode["id"] == "job-success":
                mode["polls"] += 1
                done = mode["polls"] >= 4
                body = {"id": mode["id"], "state": "done" if done else "running",
                        "received": 4194304 if done else 1048576,
                        "total": 4194304, "error": None, "name": "downloaded.iso"}
            else:
                body = {"id": mode["id"], "state": "running", "received": 524288,
                        "total": 4194304, "error": None, "name": "downloaded.iso"}
            return route.fulfill(status=200, content_type="application/json",
                                 body=json.dumps(body))

        page.route("**/api/media", media_list)
        page.route("**/api/media/download**", download)
        page.locator('[data-panel-target="media"]').click()
        page.locator("#media-download summary").click()
        form = page.locator("#media-download-form")
        digest = "a" * 64
        form.locator('input[name="url"]').fill("http://127.0.0.1:9/image.iso")
        form.locator('input[name="sha256"]').fill(digest)
        form.locator('button[type="submit"]').click()
        page.wait_for_timeout(900)

        check(checks, downloads and downloads[0].get("url") == "http://127.0.0.1:9/image.iso"
              and downloads[0].get("sha256") == digest,
              "url_download_submits_url_and_sha256", downloads)
        stats = page.text_content("#media-download-stats") or ""
        check(checks, "已下载" in stats and "MB" in stats and "速度" in stats and "/s" in stats,
              "url_download_renders_progress", stats)

        try:
            page.wait_for_function(
                "() => (document.querySelector('#media-list')?.textContent || '').includes('downloaded.iso')",
                timeout=5000)
        except Exception:
            pass
        listed = page.text_content("#media-list") or ""
        check(checks, "downloaded.iso" in listed and media_calls,
              "url_download_done_refreshes_list", (listed, len(media_calls)))

        form.locator('input[name="url"]').fill("http://127.0.0.1:9/other.iso")
        form.locator('button[type="submit"]').click()
        page.wait_for_timeout(400)
        started = len(downloads) == 2
        page.click("#media-download-cancel")
        page.wait_for_timeout(400)
        check(checks, started and len(cancels) == 1,
              "url_download_cancel_requested", (started, cancels))
        try:
            page.wait_for_function(
                "() => document.querySelector('#media-download-cancel').classList.contains('hidden')",
                timeout=5000)
        except Exception:
            pass
        hidden = page.evaluate(
            "() => document.querySelector('#media-download-cancel').classList.contains('hidden')")
        check(checks, hidden, "url_download_cancel_finishes_ui", hidden)
    finally:
        context.close()


def csp_policy(repo, host):
    """The production CSP, built from web/csp-policy.txt exactly like
    src/web/security_headers.rs does (hash of the inline theme script)."""
    import base64, hashlib
    html = open(os.path.join(repo, "web", "index.html"), encoding="utf-8").read()
    start = html.index("<script>") + len("<script>")
    script = html[start:html.index("</script>", start)]
    digest = base64.b64encode(hashlib.sha256(script.encode("utf-8")).digest()).decode()
    lines = open(os.path.join(repo, "web", "csp-policy.txt"), encoding="utf-8").read().splitlines()
    policy = "; ".join(l.strip() for l in lines if l.strip() and not l.strip().startswith("#"))
    return (policy.replace("{inline_script_hash}", f"'sha256-{digest}'")
            .replace("{websocket_sources}", f"ws://{host} wss://{host}"))


def serve_with_csp(repo):
    """Serve the unbundled UI (index.html + /assets/app.{js,css}) with the real CSP."""
    import threading, urllib.parse
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
    web = os.path.join(repo, "web")
    def concat(sub, ext):
        d = os.path.join(web, sub)
        return b"".join(open(os.path.join(d, n), "rb").read() for n in sorted(os.listdir(d)) if n.endswith(ext))
    xterm = read_assets(repo)
    files = {
        "/": (open(os.path.join(web, "index.html"), "rb").read(), "text/html; charset=utf-8"),
        "/assets/app.js": (concat("js/app", ".js"), "text/javascript"),
        "/assets/app.css": (concat("css/app", ".css"), "text/css"),
        "/assets/xterm.js": (xterm["xterm.js"], "text/javascript"),
        "/assets/xterm-fit.js": (xterm["addon-fit.js"], "text/javascript"),
        "/assets/xterm.css": (xterm["xterm.css"], "text/css"),
        "/assets/icon.svg": (open(os.path.join(web, "icon.svg"), "rb").read(), "image/svg+xml"),
    }
    files["/favicon.ico"] = files["/assets/icon.svg"]

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            body, ctype = files.get(urllib.parse.urlsplit(self.path).path, (b"not found", "text/plain"))
            self.send_response(200 if body != b"not found" else 404)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Content-Security-Policy", csp_policy(repo, self.headers.get("Host", "")))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


def csp_no_violations(browser, port, checks):
    """#92: the real CSP allows login -> console, video, terminal and settings."""
    import verify_p2_ui as p2
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    server, csp_port = serve_with_csp(repo)
    state = p2.State(authenticated=False, mass_storage=False)
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    context.add_init_script("""
      window.__csp = [];
      localStorage.setItem('wingman_theme', 'dark');
      document.addEventListener('securitypolicyviolation', e =>
        window.__csp.push(e.violatedDirective + ' ' + (e.blockedURI || 'inline')));
    """)
    page = context.new_page()
    console = []
    page.on("console", lambda m: m.type == "error" and console.append(m.text))
    page.on("pageerror", lambda e: console.append(str(e)))
    try:
        page.route("**/api/**", lambda route: p2.api_route(route, state))
        page.route("**/video_feed*", lambda route: route.fulfill(
            status=200, content_type="image/gif", body=p2.GIF))
        response = page.goto(f"http://localhost:{csp_port}/", wait_until="load", timeout=15000)
        header = response.headers.get("content-security-policy", "")
        check(checks, "script-src 'self' 'sha256-" in header, "csp_header_served", header)
        p2.wait_login(page)
        theme = page.evaluate("() => document.documentElement.getAttribute('data-theme')")
        check(checks, theme == "dark", "csp_allows_hashed_theme_script", theme)
        page.fill('#login-form input[name="username"]', p2.USER)
        page.fill('#login-form input[name="password"]', p2.PASSWORD)
        page.click('#login-form button[type="submit"]')
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.wait_for_timeout(500)
        page.click('[data-workspace="terminal"]')
        page.wait_for_selector("#terminal-host .xterm", timeout=5000)
        page.wait_for_timeout(300)
        page.click('[data-workspace="video"]')
        page.click("#settings-toggle")
        page.wait_for_timeout(300)
        page.click("[data-settings-close]")
        page.wait_for_timeout(300)
        violations = page.evaluate("() => window.__csp")
        check(checks, not violations, "csp_no_violation_events", violations)
        csp_console = [t for t in console if "Content Security Policy" in t or "Refused to" in t]
        check(checks, not csp_console, "csp_no_console_errors", csp_console)
    finally:
        context.close()
        server.shutdown()
        server.server_close()


def media_page(browser, state, port, h264=False, stubs=(PIP_STUB,)):
    """new_page with init scripts, plus an optional negotiable H.264 backend."""
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    page = context.new_page()
    for stub in stubs:
        page.add_init_script(stub)
    page.route("**/api/**", lambda route: api_route(route, state))
    page.route("**/video_feed*", lambda route: route.fulfill(
        status=200, content_type="image/gif", body=GIF))
    if h264:
        def bootstrap(route):
            body = bootstrap_payload(state)
            body["capabilities"]["video_webrtc_h264"] = True
            body["webrtc"] = {"available": True}
            route.fulfill(status=200, content_type="application/json", body=json.dumps(body))
        page.route("**/api/bootstrap", bootstrap)
        page.route("**/api/webrtc/offer", lambda route: route.fulfill(
            status=200, content_type="application/json",
            body=json.dumps({"type": "answer", "session_id": "s1", "sdp": "a=rtpmap:96 H264/90000"})))
        page.route("**/api/webrtc/close", lambda route: route.fulfill(
            status=200, content_type="application/json", body="{}"))
    page.goto(f"http://127.0.0.1:{port}/", wait_until="load", timeout=15000)
    page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
    page.bring_to_front()
    return context, page


def wait_mjpeg_playing(page):
    page.wait_for_function("() => document.querySelector('#video-feed').naturalWidth > 0", timeout=5000)
    page.wait_for_function("""() => {
      document.querySelector('#video-feed').dispatchEvent(new Event('load'));
      return /LIVE/.test(document.querySelector('#video-transport-label').textContent);
    }""", timeout=5000)


def pip_unsupported_hides_button(browser, port, checks):
    """#79: no PiP support, no button."""
    state = State()
    stub = """Object.defineProperty(Document.prototype, 'pictureInPictureEnabled',
      {configurable: true, get: () => false});"""
    context, page = media_page(browser, state, port, stubs=(stub,))
    try:
        check(checks, not page.locator("#pip").is_visible(), "pip_hidden_when_unsupported", "")
    finally:
        context.close()


def pip_mjpeg(browser, port, checks):
    """#79: MJPEG PiP plays a hidden canvas-fed video and cleans up."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10, frames_per_second=30)
    context, page = media_page(browser, state, port)
    source = "() => { const v = document.querySelector('video.pip-source'); return v ? !!v.srcObject : null; }"
    try:
        check(checks, page.locator("#pip").is_visible(), "pip_button_visible", "")
        page.click('[data-workspace="terminal"]')
        page.wait_for_timeout(200)
        page.evaluate("() => document.querySelector('#pip').click()")
        page.wait_for_timeout(100)
        toast = page.text_content("#toast") or ""
        check(checks, "没有可显示" in toast and not page.evaluate("() => window.__pipCalls.length"),
              "pip_requires_live_picture", toast)
        page.click('[data-workspace="video"]')
        wait_mjpeg_playing(page)

        page.click("#pip")
        page.wait_for_function("() => window.__pipCalls.length === 1", timeout=5000)
        check(checks, page.evaluate("() => window.__pipCalls[0]") == "pip-source" and page.evaluate(source) is True,
              "pip_mjpeg_uses_canvas_video", page.evaluate("() => window.__pipCalls"))
        check(checks, page.get_attribute("#pip", "aria-pressed") == "true", "pip_button_pressed", "")

        # Backgrounding the tab must keep the stream alive while in PiP...
        hide = """(hidden) => {
          Object.defineProperty(document, 'hidden', {configurable: true, get: () => hidden});
          document.dispatchEvent(new Event('visibilitychange'));
        }"""
        page.evaluate(hide, True)
        page.wait_for_timeout(100)
        check(checks, bool(page.get_attribute("#video-feed", "src")) and page.evaluate(source) is True,
              "pip_keeps_video_when_tab_hidden", "")
        # ...and closing the PiP window from the background stops it.
        page.evaluate("() => document.exitPictureInPicture()")
        page.wait_for_timeout(100)
        check(checks, not page.get_attribute("#video-feed", "src") and page.evaluate(source) is None
              and page.get_attribute("#pip", "aria-pressed") == "false",
              "pip_leave_in_background_stops_video", "")
        page.evaluate(hide, False)
        page.wait_for_timeout(200)
        wait_mjpeg_playing(page)

        # A workspace switch stops video, which must end PiP and its resources.
        page.click("#pip")
        page.wait_for_function("() => window.__pipCalls.length === 2", timeout=5000)
        page.click('[data-workspace="terminal"]')
        page.wait_for_timeout(200)
        check(checks, page.evaluate("() => document.pictureInPictureElement") is None and page.evaluate(source) is None
              and page.get_attribute("#pip", "aria-pressed") == "false", "pip_cleans_up_on_stop", "")
        page.click('[data-workspace="video"]')
        wait_mjpeg_playing(page)

        page.evaluate("() => { window.__pipReject = true; }")
        page.click("#pip")
        page.wait_for_timeout(300)
        check(checks, "无法进入" in (page.text_content("#toast") or "") and page.evaluate(source) is None
              and page.get_attribute("#pip", "aria-pressed") == "false", "pip_rejection_reports_and_cleans_up", "")
    finally:
        context.close()


def pip_webrtc(browser, port, checks):
    """#79: WebRTC PiP uses the live <video>; fallback to MJPEG ends it."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    context, page = media_page(browser, state, port, h264=True, stubs=(PIP_STUB, FAKE_PEER))
    try:
        page.wait_for_function("() => document.querySelector('#webrtc-feed').videoWidth > 0", timeout=8000)
        page.wait_for_function("() => /H.264/.test(document.querySelector('#video-transport-label').textContent)"
                               " && /LIVE/.test(document.querySelector('#video-transport-label').textContent)", timeout=8000)
        page.click("#pip")
        page.wait_for_function("() => window.__pipCalls.length === 1", timeout=5000)
        check(checks, page.evaluate("() => window.__pipCalls[0]") == "webrtc-feed"
              and page.get_attribute("#pip", "aria-pressed") == "true", "pip_webrtc_uses_video_element",
              page.evaluate("() => window.__pipCalls"))
        page.evaluate("""() => {
          const peer = window.__peers.at(-1);
          peer.connectionState = 'failed';
          peer.onconnectionstatechange();
        }""")
        page.wait_for_timeout(200)
        check(checks, page.evaluate("() => document.pictureInPictureElement") is None
              and page.get_attribute("#pip", "aria-pressed") == "false", "pip_ends_on_transport_fallback", "")
    finally:
        context.close()


def stats_report(at, scale=1, frames=None):
    """One getStats() result; counters grow linearly with `scale`."""
    return [
        {"id": "T", "type": "transport", "selectedCandidatePairId": "P"},
        {"id": "P", "type": "candidate-pair", "currentRoundTripTime": 0.025},
        {"id": "AUDIO", "type": "inbound-rtp", "kind": "audio", "timestamp": at, "bytesReceived": 9e9},
        {"id": "V", "type": "inbound-rtp", "kind": "video", "timestamp": at,
         "bytesReceived": 125000 * scale, "packetsReceived": 990 * scale, "packetsLost": 10 * scale,
         "framesDecoded": 30 * scale, "jitterBufferDelay": 1.5 * scale,
         "jitterBufferEmittedCount": 30 * scale, "frameWidth": 1920, "frameHeight": 1080},
    ]


def overlay_rows(page):
    return page.evaluate("""() => {
      const out = {};
      document.querySelectorAll('#quality-overlay dt:not(.hidden)').forEach((dt) => {
        out[dt.textContent] = dt.nextElementSibling.textContent;
      });
      return out;
    }""")


def quality_overlay_webrtc(browser, port, checks):
    """#81: stats deltas, persistence, and polling only while shown."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    context, page = media_page(browser, state, port, h264=True, stubs=(FAKE_PEER,))
    try:
        page.wait_for_function("() => /LIVE/.test(document.querySelector('#video-transport-label').textContent)", timeout=8000)
        page.wait_for_timeout(1200)
        check(checks, not page.locator("#quality-overlay").is_visible()
              and page.evaluate("() => window.__statsCalls") == 0, "quality_off_by_default_without_polling", "")

        # Two seconds apart: rates must divide by the real interval.
        reports = [stats_report(10000, 1), stats_report(12000, 3), stats_report(12000, 3), stats_report(13000, 1),
                   stats_report(14000, 2)]
        page.evaluate("(reports) => { window.__statsSeq = reports; }", reports)
        page.click("#quality-toggle")
        check(checks, page.locator("#quality-overlay").is_visible()
              and page.get_attribute("#quality-toggle", "aria-pressed") == "true", "quality_toggle_shows_overlay", "")
        rows = overlay_rows(page)
        check(checks, rows.get("接收码率") == "—", "quality_first_sample_has_no_rates", rows)
        page.wait_for_function("() => document.querySelector('#quality-overlay').textContent.includes('Mbps')", timeout=4000)
        rows = overlay_rows(page)
        check(checks, rows == {"往返延迟": "25 ms", "接收码率": "1.00 Mbps", "丢包率": "1.0%", "解码帧率": "30",
                               "抖动缓冲": "50 ms", "分辨率": "1920×1080"}, "quality_webrtc_stats_rendered", rows)
        page.wait_for_function("() => window.__statsCalls >= 3", timeout=4000)
        page.wait_for_timeout(50)
        rows = overlay_rows(page)
        check(checks, rows.get("接收码率") == "1.00 Mbps", "quality_cached_report_keeps_rates", rows)
        page.wait_for_function("() => window.__statsCalls >= 4", timeout=4000)
        page.wait_for_timeout(50)
        rows = overlay_rows(page)
        check(checks, rows.get("接收码率") == "—" and rows.get("解码帧率") == "—", "quality_counter_reset_not_negative", rows)
        page.wait_for_function("() => window.__statsCalls >= 5", timeout=4000)
        page.wait_for_timeout(50)
        rows = overlay_rows(page)
        check(checks, rows.get("接收码率") == "1.00 Mbps", "quality_recovers_after_reset", rows)

        page.evaluate("""() => {
          Object.defineProperty(document, 'hidden', {configurable: true, get: () => true});
          document.dispatchEvent(new Event('visibilitychange'));
        }""")
        before = page.evaluate("() => window.__statsCalls")
        page.wait_for_timeout(1300)
        after = page.evaluate("() => window.__statsCalls")
        check(checks, after == before, "quality_polling_stops_when_page_hidden", (before, after))
        page.evaluate("""() => {
          delete document.hidden;
          document.dispatchEvent(new Event('visibilitychange'));
        }""")
        page.wait_for_function("() => /LIVE/.test(document.querySelector('#video-transport-label').textContent)", timeout=8000)

        page.reload(wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.wait_for_function("() => /LIVE/.test(document.querySelector('#video-transport-label').textContent)", timeout=8000)
        check(checks, page.locator("#quality-overlay").is_visible()
              and page.get_attribute("#quality-toggle", "aria-pressed") == "true", "quality_setting_persists", "")
        page.click("#quality-toggle")
        before = page.evaluate("() => window.__statsCalls")
        page.wait_for_timeout(1300)
        check(checks, not page.locator("#quality-overlay").is_visible()
              and page.evaluate("() => window.__statsCalls") == before, "quality_toggle_off_stops_polling", "")
    finally:
        context.close()


def quality_overlay_mjpeg(browser, port, checks):
    """#81: MJPEG is honest about where FPS comes from and about the bitrate."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10, frames_per_second=24.0)
    context, page = media_page(browser, state, port, stubs=())
    try:
        wait_mjpeg_playing_once = """() => {
          const feed = document.querySelector('#video-feed');
          return feed.naturalWidth > 0 && /LIVE/.test(document.querySelector('#video-transport-label').textContent);
        }"""
        page.wait_for_function(wait_mjpeg_playing_once, timeout=5000)
        page.click("#quality-toggle")
        page.wait_for_timeout(100)
        rows = overlay_rows(page)
        check(checks, rows.get("帧率（服务端采集）") == "24" and rows.get("码率") == "不可用" and "丢包率" not in rows,
              "quality_mjpeg_single_load_uses_server_fps", rows)

        page.evaluate("""() => {
          window.__loads = setInterval(
            () => document.querySelector('#video-feed').dispatchEvent(new Event('load')), 100);
        }""")
        page.wait_for_function("""() => {
          const dt = [...document.querySelectorAll('#quality-overlay dt')].find((e) => e.textContent.includes('浏览器实测'));
          return dt && Number(dt.nextElementSibling.textContent) >= 6;
        }""", timeout=5000)
        rows = overlay_rows(page)
        fps = float(rows.get("帧率（浏览器实测）", "0"))
        check(checks, 6 <= fps <= 14 and "帧率（服务端采集）" not in rows, "quality_mjpeg_counts_load_events", rows)
        page.evaluate("() => clearInterval(window.__loads)")

        page.click("#lang-toggle")
        page.click('[data-lang="en"]')
        page.wait_for_timeout(1300)
        rows = overlay_rows(page)
        check(checks, rows.get("Bitrate") == "Unavailable" and "FPS (browser measured)" in rows,
              "quality_overlay_translates", rows)

        hit = page.evaluate("""() => {
          const box = document.querySelector('#quality-overlay').getBoundingClientRect();
          return document.elementFromPoint(box.left + 5, box.top + 5).closest('#quality-overlay') === null;
        }""")
        check(checks, hit, "quality_overlay_does_not_block_input", hit)
    finally:
        context.close()


def hotplug_recovery(browser, port, checks):
    """#93: capture-card unplug/replug and USB HID re-enumeration are announced."""
    from verify_p2b_ui import api_route, bootstrap_payload
    state = State()
    hid = {"keyboard_reenumerating": False, "mouse_reenumerating": False}
    context, page = new_page(browser, state, port)
    try:
        class WithHid:
            def __init__(self, route):
                self.route, self.request = route, route.request
            def fulfill(self, **kwargs):
                body = json.loads(kwargs.get("body") or "{}")
                body.setdefault("hid", {}).update(hid)
                return self.route.fulfill(**{**kwargs, "body": json.dumps(body)})
        page.unroute("**/api/**")
        page.route("**/api/**", lambda route: api_route(
            WithHid(route) if route.request.url.endswith("/api/status") else route, state))
        page.evaluate("""() => {
          window.__toasts = [];
          new MutationObserver((records) => records.forEach((r) => r.addedNodes.forEach((n) => {
            if (n.nodeValue) window.__toasts.push(n.nodeValue);
          }))).observe(document.querySelector('#toast'), {childList: true});
        }""")
        ready = dict(state.video)
        state.video = {**ready, "state": "offline", "reason": "device_missing",
                       "message": "采集卡已断开：未找到 /dev/video0"}
        hid["keyboard_reenumerating"] = True
        page.wait_for_timeout(3500)
        message = page.text_content("#video-message") or ""
        label = page.text_content("#status-label") or ""
        toasts = page.evaluate("() => window.__toasts")
        check(checks, "采集卡已断开" in message and label == "已断开", "capture_unplug_message", (message, label))
        check(checks, "采集卡已断开" in toasts and "USB 键鼠通道重新连接中" in toasts,
              "unplug_and_hid_toasts", toasts)
        check(checks, page.is_visible("#hid-link-state"), "hid_reenumerating_badge_shown", None)

        state.video = ready
        hid["keyboard_reenumerating"] = False
        page.wait_for_timeout(3500)
        toasts = page.evaluate("() => window.__toasts")
        check(checks, "采集卡已恢复" in toasts and "USB 键鼠通道已恢复" in toasts, "recovery_toasts", toasts)
        check(checks, not page.is_visible("#hid-link-state"), "hid_badge_cleared", None)
        message_hidden = page.evaluate("() => document.querySelector('#video-message').classList.contains('hidden')")
        check(checks, (page.text_content("#status-label") or "") != "已断开", "capture_recovered_label",
              page.text_content("#status-label"))

        page.click("#lang-toggle")
        page.click('[data-lang="en"]')
        state.video = {**ready, "state": "offline", "reason": "device_missing",
                       "message": "采集卡已断开：未找到 /dev/video0"}
        hid["mouse_reenumerating"] = True
        page.wait_for_timeout(3500)
        message = page.text_content("#video-message") or ""
        badge = page.text_content("#hid-link-state") or ""
        check(checks, message.startswith("Capture card disconnected") and badge == "HID reconnecting",
              "hotplug_messages_translate", (message, badge, message_hidden))
    finally:
        context.close()


def keyboard_macros(browser, port, checks):
    """#80: record forwarded keys, replay with timing, and manage JSON files."""
    state = State()
    context, page = new_page(browser, state, port)
    try:
        page.click("#input-state")
        page.click("#quick-more-toggle")
        page.click("#quick-macro-record")
        check(checks, page.locator("#macro-recording").is_visible(),
              "macro_recording_indicator_visible", page.locator("#macro-recording").is_visible())

        page.keyboard.press("Control+a")
        page.wait_for_timeout(30)
        page.keyboard.press("Shift+b")
        page.wait_for_timeout(600)
        page.keyboard.press("c")

        # Keys typed into local controls, especially password fields, must not
        # be recorded (or forwarded) even while a recording is running.
        page.evaluate("""() => {
          const input = document.createElement('input');
          input.type = 'password';
          input.id = 'macro-test-password';
          document.body.append(input);
          input.focus();
        }""")
        page.keyboard.press("d")
        page.wait_for_timeout(50)
        page.evaluate("""() => {
          document.querySelector('#macro-test-password')?.remove();
          document.querySelector('#video-viewport').focus();
        }""")

        # The local exit shortcut is handled before the forwarding path.
        page.keyboard.press("Control+Alt+`")
        page.wait_for_timeout(100)
        check(checks, not page.is_checked("#remote-input"),
              "macro_local_shortcut_exits_forwarding", page.is_checked("#remote-input"))

        page.click("#macro-record-stop")
        page.wait_for_selector("#macro-save-dialog[open]", timeout=3000)
        summary = page.text_content("#macro-save-summary") or ""
        check(checks, "3" in summary, "macro_save_dialog_reports_three_steps", summary)
        page.fill("#macro-save-name", "测试宏")
        page.click("#macro-save-confirm")
        page.wait_for_timeout(200)

        saved = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_keyboard_macros') || '[]'"))
        steps = saved[0].get("steps", []) if saved else []
        keys = [step.get("key") for step in steps]
        mods = [(bool(step.get("ctrl")), bool(step.get("shift")),
                 bool(step.get("alt")), bool(step.get("meta"))) for step in steps]
        check(checks, keys == ["KeyA", "KeyB", "KeyC"],
              "macro_recording_captures_forwarded_keys_only", keys)
        check(checks, mods == [(True, False, False, False),
                              (False, True, False, False),
                              (False, False, False, False)],
              "macro_recording_captures_modifiers", mods)
        check(checks, steps and steps[-1].get("delay_ms", 0) >= 500,
              "macro_recording_captures_delay", steps[-1:] if steps else steps)

        def play_and_time(speed):
            state.key_posts.clear()
            page.click("#quick-more-toggle")
            page.click(f'[data-macro-speed="{speed}"]')
            started = time.monotonic()
            page.click("#quick-more-menu [data-macro-play]")
            page.wait_for_function(
                """() => (document.querySelector('#macro-play-status')?.textContent || '')
                  .includes('已回放完成')""", timeout=6000)
            return time.monotonic() - started

        fast = play_and_time("2")
        fast_sequence = [(post.get("key"), bool(post.get("ctrl")), bool(post.get("shift")),
                          bool(post.get("alt")), bool(post.get("meta")))
                         for post in state.key_posts]
        check(checks, fast_sequence == [("KeyA", True, False, False, False),
                                       ("KeyB", False, True, False, False),
                                       ("KeyC", False, False, False, False)],
              "macro_replay_sends_order_and_modifiers", fast_sequence)
        page.click('#macro-play-dialog .dialog-head [data-macro-play-close]')
        page.wait_for_timeout(100)

        slow = play_and_time("0.5")
        check(checks, slow > fast + 0.5, "macro_replay_speed_changes_interval", (fast, slow))
        page.click('#macro-play-dialog .dialog-head [data-macro-play-close]')
        page.wait_for_timeout(100)

        # Cancel during the long recorded pause before the last key.
        state.key_posts.clear()
        page.click("#quick-more-toggle")
        page.click('[data-macro-speed="0.5"]')
        page.click("#quick-more-menu [data-macro-play]")
        page.wait_for_timeout(180)
        page.click("#macro-play-cancel")
        page.wait_for_timeout(500)
        canceled = len(state.key_posts)
        status = page.text_content("#macro-play-status") or ""
        check(checks, 0 < canceled < 3 and "已取消" in status,
              "macro_replay_can_be_canceled", (canceled, status))
        page.click('#macro-play-dialog .dialog-head [data-macro-play-close]')
        page.wait_for_timeout(100)

        # Turning forwarding off mid-replay stops the run and reports it.
        state.key_posts.clear()
        page.click("#quick-more-toggle")
        page.click('[data-macro-speed="0.5"]')
        page.click("#quick-more-menu [data-macro-play]")
        page.wait_for_timeout(180)
        page.locator("#input-state").evaluate("b => b.click()")
        page.wait_for_timeout(500)
        interrupted = len(state.key_posts)
        status = page.text_content("#macro-play-status") or ""
        check(checks, 0 < interrupted < 3 and "已中断" in status,
              "macro_replay_stops_when_forwarding_off", (interrupted, status))
        page.click('#macro-play-dialog .dialog-head [data-macro-play-close]')
        page.wait_for_timeout(100)

        page.click("#quick-more-toggle")
        page.click("#quick-macro-manage")
        page.wait_for_selector("#macro-manager-dialog[open]", timeout=3000)
        check(checks, page.locator("#macro-manager-list [data-macro-row]").count() == 1,
              "macro_manager_lists_saved_macro",
              page.locator("#macro-manager-list [data-macro-row]").count())

        page.fill("#macro-manager-list .macro-name-input", "重命名宏")
        page.click("#macro-manager-list [data-macro-rename]")
        page.wait_for_timeout(200)
        renamed = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_keyboard_macros') || '[]'"))
        check(checks, renamed and renamed[0].get("name") == "重命名宏",
              "macro_can_be_renamed", renamed[0].get("name") if renamed else None)

        # The export file carries only names, key names, modifiers and delays.
        with page.expect_download(timeout=5000) as download:
            page.click("#macro-export-all")
        exported = json.load(open(download.value.path(), encoding="utf-8"))
        exported_step = exported["macros"][0]["steps"][0]
        check(checks, set(exported) == {"version", "macros"}
              and set(exported["macros"][0]) == {"name", "steps"}
              and set(exported_step) == {"key", "ctrl", "shift", "alt", "meta", "delay_ms"},
              "macro_export_contains_only_key_data", exported_step)

        before = page.evaluate("() => localStorage.getItem('wingman_keyboard_macros')")
        page.set_input_files("#macro-import-file", files=[{
            "name": "bad.json", "mimeType": "application/json", "buffer": b"not json"}])
        page.wait_for_timeout(300)
        toast = page.text_content("#toast") or ""
        check(checks, page.evaluate("() => localStorage.getItem('wingman_keyboard_macros')") == before
              and "导入失败" in toast, "macro_import_rejects_invalid_json", toast)

        bad_key = json.dumps({"version": 1, "macros": [{"name": "bad", "steps": [{
            "key": "NotAKey", "ctrl": False, "shift": False, "alt": False,
            "meta": False, "delay_ms": 0}]}]}).encode()
        page.set_input_files("#macro-import-file", files=[{
            "name": "bad-key.json", "mimeType": "application/json", "buffer": bad_key}])
        page.wait_for_timeout(300)
        toast = page.text_content("#toast") or ""
        check(checks, page.evaluate("() => localStorage.getItem('wingman_keyboard_macros')") == before
              and "导入失败" in toast, "macro_import_rejects_unknown_keys", toast)

        valid = json.dumps({"version": 1, "macros": [{"name": "导入宏", "steps": [{
            "key": "KeyX", "ctrl": False, "shift": False, "alt": False,
            "meta": False, "delay_ms": 0}]}]}).encode()
        page.set_input_files("#macro-import-file", files=[{
            "name": "valid.json", "mimeType": "application/json", "buffer": valid}])
        page.wait_for_timeout(300)
        imported = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_keyboard_macros') || '[]'"))
        check(checks, len(imported) == 2
              and imported[-1].get("name") == "导入宏"
              and imported[-1]["steps"][0].get("key") == "KeyX",
              "macro_import_accepts_valid_file", imported)

        page.click("#macro-manager-list [data-macro-row]:last-child [data-macro-delete]")
        page.wait_for_timeout(200)
        remaining = json.loads(
            page.evaluate("() => localStorage.getItem('wingman_keyboard_macros') || '[]'"))
        check(checks, len(remaining) == 1 and remaining[0].get("name") == "重命名宏",
              "macro_can_be_deleted", remaining)

        page.click("#macro-manager-dialog .dialog-head [data-macro-manager-close]")
        page.reload(wait_until="load")
        page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
        page.click("#quick-more-toggle")
        names = page.eval_on_selector_all(
            "#quick-more-menu [data-macro-play]", "els => els.map(e => e.textContent)")
        check(checks, names == ["重命名宏"], "macro_survives_reload", names)
    finally:
        context.close()


def config_backup_restore(browser, port, checks):
    """#83: export downloads config; import previews, cancels and applies."""
    from verify_p2b_ui import bootstrap_payload
    state = State()
    context, page = new_page(browser, state, port)
    try:
        config = dict(bootstrap_payload(state)["config"])
        exported = {"format": "wingmankvm-config", "version": 1,
                    "exported_at": "2026-11-01T00:00:00Z", "config": config}
        export_body = json.dumps(exported)
        export_requests = []

        def export_route(route):
            export_requests.append(route.request.url)
            route.fulfill(status=200, headers={
                "content-type": "application/json",
                "content-disposition":
                    'attachment; filename="wingmankvm-config-2026-11-01.json"',
            }, body=export_body)

        changes = [
            {"path": "server.port", "old": 8080, "new": 9090},
            {"path": "video.width", "old": 1280, "new": 1920},
        ]
        preview_posts, import_posts = [], []
        preview_error = {"status": None}

        def preview_route(route):
            if preview_error["status"]:
                return route.fulfill(
                    status=preview_error["status"], content_type="application/json",
                    body='{"error":"配置版本 2 不受支持"}')
            preview_posts.append(json.loads(route.request.post_data or "{}"))
            return route.fulfill(
                status=200, content_type="application/json",
                body=json.dumps({"changes": changes, "restart_required": True}))

        def import_route(route):
            import_posts.append(json.loads(route.request.post_data or "{}"))
            return route.fulfill(
                status=200, content_type="application/json",
                body=json.dumps({"ok": True, "restart_required": True,
                                 "changes": changes}))

        page.route("**/api/config/export", export_route)
        page.route("**/api/config/import/preview", preview_route)
        page.route("**/api/config/import", import_route)
        page.click("#settings-toggle")
        page.wait_for_selector("#settings-dialog[open]", timeout=3000)

        with page.expect_download(timeout=5000) as download:
            page.click("#config-export-btn")
        filename = download.value.suggested_filename
        check(checks, filename == "wingmankvm-config-2026-11-01.json",
              "config_export_downloads_dated_json", filename)
        check(checks, export_requests, "config_export_requests_api", export_requests)

        def choose_file():
            with page.expect_file_chooser(timeout=3000) as chooser:
                page.click("#config-import-btn")
            chooser.value.set_files({
                "name": "wingmankvm-config-2026-11-01.json",
                "mimeType": "application/json",
                "buffer": export_body.encode(),
            })
            page.wait_for_selector("#config-import-dialog[open]", timeout=3000)

        choose_file()
        page.wait_for_timeout(200)
        diff = page.text_content("#config-import-diff") or ""
        check(checks, "server.port" in diff and "8080" in diff and "9090" in diff,
              "config_import_preview_lists_diff", diff[:160])
        check(checks, preview_posts and preview_posts[-1].get("format") == "wingmankvm-config",
              "config_import_preview_uploads_export", preview_posts[-1:])
        summary = page.text_content("#config-import-summary") or ""
        check(checks, "2" in summary, "config_import_preview_counts_changes", summary)

        page.click("#config-import-cancel")
        page.wait_for_timeout(200)
        opened = page.evaluate("() => document.querySelector('#config-import-dialog').open")
        check(checks, not import_posts and not opened,
              "config_import_cancel_does_not_apply", (import_posts, opened))

        choose_file()
        page.wait_for_timeout(200)
        page.click("#config-import-confirm")
        page.wait_for_timeout(300)
        check(checks, import_posts and import_posts[-1] == exported,
              "config_import_confirms_same_document", import_posts[-1:])
        status = page.text_content("#config-import-status") or ""
        check(checks, "导入成功" in status and "重启" in status,
              "config_import_reports_success_and_restart", status)

        page.click("#config-import-cancel")
        preview_error["status"] = 400
        before = len(import_posts)
        choose_file()
        page.wait_for_timeout(200)
        status = page.text_content("#config-import-status") or ""
        check(checks, "配置版本 2 不受支持" in status and "导入失败" in status,
              "config_import_preview_error_is_shown", status)
        check(checks, len(import_posts) == before,
              "config_import_error_does_not_apply", import_posts)
    finally:
        context.close()


# Every function taking (browser, port, checks) is a scenario, run in
# definition order; new scenarios need no list edit (and no merge conflict).
SCENARIOS = [
    function
    for function in list(globals().values())
    if inspect.isfunction(function)
    and list(inspect.signature(function).parameters) == ["browser", "port", "checks"]
]

if __name__ == "__main__":
    sys.exit(main())
