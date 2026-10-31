#!/usr/bin/env python3
"""Audit round 2 UI regressions (issues #29, #35-#38, #43).

Reuses the P2b mocked API harness: real DOM, real browser keyboard/pointer
events and the real UI state machine; no hardware is touched.
"""
import argparse, json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from playwright.sync_api import sync_playwright
    from verify_p2_ui import extract_html, read_assets, serve_static, check
    from verify_p2b_ui import State, new_page
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


SCENARIOS = [keyboard_focus, video_settings, logout_and_errors]

if __name__ == "__main__":
    sys.exit(main())
