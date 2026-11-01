#!/usr/bin/env python3
"""Regenerate every README/docs image: the screenshots and the demo animation.

    python3 scripts/capture_screenshots.py [--output-dir docs/images] [--skip-demo]

The real UI bundle is served by ui_harness.static_server() and every API call,
the input WebSocket and /video_feed are answered from capture_fixtures.py, so
no device is ever contacted. Viewport, scale, locale, time zone, the browser
clock and the synthetic target screen are fixed, so reruns on the same machine
only change images when the UI changes. Fonts come from the host: regenerate
the committed images on macOS so the Chinese UI uses PingFang SC.
"""
import argparse, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from playwright.sync_api import sync_playwright  # noqa: E402

import capture_demo  # noqa: E402
import capture_fixtures as F  # noqa: E402
import ui_harness  # noqa: E402


class Shooter:
    def __init__(self, output_dir):
        self.output_dir = output_dir

    def __call__(self, page, name):
        F.assert_clean(page, name)
        page.screenshot(path=os.path.join(self.output_dir, name), animations="disabled", caret="hide")
        print(f"Generated {name}")


def enable_forwarding(page):
    page.click("#input-state")
    page.wait_for_function("() => document.querySelector('#remote-input').checked")
    # Let the "forwarding enabled" toast fade out before the screenshot.
    page.wait_for_function("() => !document.querySelector('#toast').classList.contains('show')", timeout=6000)
    page.wait_for_timeout(400)


def capture_setup_flow(browser, port, shoot):
    context, page, _ = F.new_page(browser, port, viewport=(1280, 820), setup=True,
                                  path="/#setup=quick-start-token")
    try:
        page.fill('#setup-form input[name="username"]', "admin")
        page.fill('#setup-form input[name="password"]', "Admin123456!@#")
        page.fill('#setup-form input[name="confirm_password"]', "Admin123456!@#")
        page.wait_for_timeout(300)
        shoot(page, "setup-account.png")

        page.click("#setup-form [data-setup-next]")
        page.wait_for_timeout(800)
        shoot(page, "setup-detection.png")

        page.click('#setup-form [data-guide="video"]')
        page.wait_for_timeout(500)
        shoot(page, "setup-guide.png")
    finally:
        context.close()


def capture_console(browser, port, shoot):
    for lang, name in (("zh-CN", "console-desktop.png"), ("en", "console-desktop-en.png")):
        context, page, _ = F.new_page(browser, port, lang=lang)
        try:
            enable_forwarding(page)
            shoot(page, name)
        finally:
            context.close()

    context, page, _ = F.new_page(browser, port, theme="dark")
    try:
        enable_forwarding(page)
        shoot(page, "console-dark.png")
        page.click("#topbar-perf-btn")
        page.wait_for_timeout(400)
        shoot(page, "perf-popover.png")
    finally:
        context.close()

    context, page, _ = F.new_page(browser, port)
    try:
        page.click("#keyboard-toggle")
        page.wait_for_timeout(400)
        shoot(page, "virtual-keyboard.png")
        page.click("#keyboard-dialog [data-close]")
        page.wait_for_timeout(200)

        page.click('[data-panel-target="media"]')
        page.wait_for_timeout(400)
        shoot(page, "media-panel.png")

        page.click('[data-panel-target="video"]')
        page.wait_for_timeout(400)
        shoot(page, "video-panel.png")
    finally:
        context.close()

    # Phone portrait (iPhone 14/15 size), dark theme, inspector collapsed.
    context, page, _ = F.new_page(browser, port, theme="dark", viewport=(390, 844))
    try:
        if page.query_selector("#app.inspector-open"):
            page.click("#inspector-close")
        page.wait_for_timeout(400)
        shoot(page, "mobile-console.png")
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output-dir", default=F.DOCS_IMG_DIR)
    parser.add_argument("--browser-channel", default="chrome")
    parser.add_argument("--skip-demo", action="store_true", help="screenshots only")
    parser.add_argument("--demo-only", action="store_true", help="demo animation only")
    args = parser.parse_args()
    os.makedirs(args.output_dir, exist_ok=True)
    shoot = Shooter(args.output_dir)
    with ui_harness.static_server(F.REPO) as port, sync_playwright() as p:
        browser = F.launch(p, args.browser_channel)
        try:
            if not args.demo_only:
                capture_setup_flow(browser, port, shoot)
                capture_console(browser, port, shoot)
            if not args.skip_demo:
                capture_demo.build(browser, port, capture_demo.LANGS, args.output_dir)
        finally:
            browser.close()
    print("Done")


if __name__ == "__main__":
    main()
