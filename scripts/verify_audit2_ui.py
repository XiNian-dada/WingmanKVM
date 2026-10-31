#!/usr/bin/env python3
"""Audit round 2 UI regressions (issues #29, #35-#38, #43).

Reuses the P2b mocked API harness: real DOM, real browser keyboard/pointer
events and the real UI state machine; no hardware is touched.
"""
import argparse, inspect, json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from playwright.sync_api import sync_playwright
    from verify_p2_ui import extract_html, read_assets, serve_static, check
    from verify_p2b_ui import GIF, State, api_route, bootstrap_payload, new_page
except Exception as exc:  # pragma: no cover - environment guard
    print(json.dumps({"status": "FAIL", "error": f"Playwright/helper import failed: {exc}"}))
    sys.exit(2)


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
        from verify_p2b_ui import api_route, GIF
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
        from verify_p2b_ui import bootstrap_payload
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
        from verify_p2b_ui import api_route
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
    from verify_p2b_ui import api_route, GIF
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
    from verify_p2b_ui import api_route, GIF
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
    from verify_p2b_ui import api_route, GIF
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
    from verify_p2b_ui import api_route
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


ADAPTIVE_OFF = "localStorage.setItem('wingman_setting_adaptive_bitrate', 'false');"


def quality_overlay_webrtc(browser, port, checks):
    """#81: stats deltas, persistence, and polling only while shown."""
    state = State()
    state.video.update(state="ready", message="", last_frame_age_ms=10)
    # Adaptive bitrate (#82) also samples stats; this scenario covers the
    # overlay alone, so it starts with adaptation turned off.
    context, page = media_page(browser, state, port, h264=True, stubs=(FAKE_PEER, ADAPTIVE_OFF))
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
                               "抖动缓冲": "50 ms", "自适应码率": "已关闭", "分辨率": "1920×1080"},
              "quality_webrtc_stats_rendered", rows)
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
