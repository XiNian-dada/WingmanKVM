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


SCENARIOS = [keyboard_focus]

if __name__ == "__main__":
    sys.exit(main())
