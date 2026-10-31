#!/usr/bin/env python3
"""P2 UI regression: media read-only defaults (ISSUE-06) and auth secret clearing (ISSUE-09)."""
import json, os, sys, time, urllib.parse
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ui_harness
# Re-exported for older callers that imported the fixtures from this module.
from ui_harness import GIF, check, extract_html, mock_video_feed, read_assets, serve_static, wait_until
try:
    from playwright.sync_api import sync_playwright
except Exception as exc:  # pragma: no cover - environment guard
    print(json.dumps({"status": "FAIL", "error": f"Playwright import failed: {exc}"}))
    sys.exit(2)

USER, PASSWORD = "admin", "Synthetic-Passw0rd!9"
SETUP_TOKEN = "SYNTHETIC-SETUP-TOKEN-921"
STATUS = {
    "video": {"state": "ready", "message": "", "frames_per_second": 30.0, "width": 1280, "height": 720},
    "power": {"power_led": {"configured": False, "state": "unknown"}},
    "hid": {"keyboard_leds": {"num_lock": False, "caps_lock": False, "scroll_lock": False, "raw": 0}},
    "client": {"ip": "127.0.0.1", "port": 12345, "source": "socket"},
    "system": {"cpu": {"percent": 2.0, "cores": 2}, "load_avg": [0.01, 0.02, 0.03],
               "memory": {"total_bytes": 1048576, "used_bytes": 524288, "percent": 50},
               "disk": {"total_bytes": 4194304, "used_bytes": 1048576, "percent": 25},
               "uptime_seconds": 60},
}


class State:
    def __init__(self, default_ro=False, include_default=True, authenticated=True,
                 setup_required=False, token_required=False, has_passkeys=False, mass_storage=True):
        self.read_only_by_default = default_ro
        self.include_default = include_default
        self.authenticated = authenticated
        self.setup_required = setup_required
        self.token_required = token_required
        self.has_passkeys = has_passkeys
        self.mass_storage = mass_storage
        self.media_items = []
        self.media_status = {}
        self.bootstrap_fail = 0
        self.hang_bootstrap = False
        self.hung_bootstrap = []
        self.login_fail = False
        self.setup_fail = False
        self.status401 = False
        self.fail_passkey_start = False
        self.login_requests = 0
        self.setup_requests = 0
        self.media_fetches = 0
        self.attach_bodies = []


def bootstrap_payload(state):
    media = {"enabled": state.mass_storage, "lun_path": "/tmp/test-lun",
             "image_directory": "/tmp/test-images"}
    if state.include_default:
        media["read_only_by_default"] = state.read_only_by_default
    return {
        "setup_required": state.setup_required, "token_required": state.token_required,
        "authenticated": state.authenticated, "username": "admin",
        "has_passkeys": state.has_passkeys,
        "client": {"ip": "127.0.0.1", "port": 12345, "source": "socket"},
        "config": {
            "server": {"proxy_protocol": "auto", "trusted_http_proxies": []},
            "video": {"device": "/dev/video0", "width": 1280, "height": 720,
                      "frames_per_second": 30, "jpeg_quality": 80, "encoding": "mjpeg"},
            "hid": {"keyboard_device": "/dev/hidg0", "mouse_device": "/dev/hidg1",
                    "absolute_pointer_device": "/dev/hidg2", "pointer_mode": "absolute"},
            "power": {"enabled": False, "gpio_chip": None, "gpio_line": None, "active_high": True},
            "media": media,
        },
        "video": {"state": "ready", "message": "", "frames_per_second": 30.0,
                  "width": 1280, "height": 720},
        "webrtc": {"available": False, "reason": "disabled by test"},
        "capabilities": {"video": True, "hid": True, "power": False,
                         "mass_storage": state.mass_storage, "terminal": True,
                         "pointer_mode": "absolute", "video_webrtc_h264": False},
    }


def api_route(route, state):
    path = urllib.parse.urlsplit(route.request.url).path
    headers = {"content-type": "application/json"}
    if path == "/api/bootstrap":
        if state.hang_bootstrap:
            state.hung_bootstrap.append(route)
            return
        if state.bootstrap_fail > 0:
            state.bootstrap_fail -= 1
            return route.fulfill(status=500, headers=headers,
                                 body=json.dumps({"error": "bootstrap unavailable"}))
        return route.fulfill(status=200, headers=headers, body=json.dumps(bootstrap_payload(state)))
    if path == "/api/login":
        state.login_requests += 1
        if state.login_fail:
            return route.fulfill(status=401, headers=headers,
                                 body=json.dumps({"error": "invalid credentials"}))
        state.authenticated = True
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/setup":
        state.setup_requests += 1
        if state.setup_fail:
            return route.fulfill(status=400, headers=headers,
                                 body=json.dumps({"error": "setup rejected"}))
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/logout":
        state.authenticated = False
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/status":
        if state.status401:
            state.status401 = False
            return route.fulfill(status=401, headers=headers,
                                 body=json.dumps({"error": "session expired"}))
        return route.fulfill(status=200, headers=headers, body=json.dumps(STATUS))
    if path == "/api/media":
        state.media_fetches += 1
        return route.fulfill(status=200, headers=headers,
                             body=json.dumps({"items": state.media_items, "status": state.media_status}))
    if path == "/api/media/attach":
        try:
            body = json.loads(route.request.post_data or "{}")
        except Exception:
            body = {}
        state.attach_bodies.append(body)
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/passkey/list":
        return route.fulfill(status=200, headers=headers, body='{"passkeys":[]}')
    if path == "/api/passkey/login/start":
        if state.fail_passkey_start:
            return route.fulfill(status=401, headers=headers,
                                 body=json.dumps({"error": "passkey session expired"}))
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/devices/scan":
        return route.fulfill(status=200, headers=headers, body='{"devices":{}}')
    return route.fulfill(status=200, headers=headers, body="{}")


def new_page(browser, state, port, fragment=""):
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    page = context.new_page()
    page.route("**/api/**", lambda route: api_route(route, state))
    mock_video_feed(page)
    page.goto(f"http://localhost:{port}/{fragment}", wait_until="load", timeout=15000)
    return page


def wait_main(page):
    page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)


def wait_login(page, timeout=8000):
    page.wait_for_selector("#login-form:not(.hidden)", state="visible", timeout=timeout)


def wait_setup(page, timeout=8000):
    page.wait_for_selector("#setup-form:not(.hidden)", state="visible", timeout=timeout)


def media_item(page, name):
    return page.locator("article.media-item").filter(has_text=name).first


def media_readonly(page, name):
    return media_item(page, name).locator("input[type=checkbox]")


def wait_media(page, count):
    page.wait_for_function("(n) => document.querySelectorAll('article.media-item').length >= n",
                           arg=count, timeout=5000)


def rerender_media(page):
    page.click("#media-refresh")
    page.wait_for_timeout(350)


def attach_payload(page, state, name):
    before = len(state.attach_bodies)
    media_item(page, name).locator("button.media-attach").click()
    deadline = time.monotonic() + 4.0
    while len(state.attach_bodies) == before and time.monotonic() < deadline:
        page.wait_for_timeout(50)
    try:
        page.wait_for_function("() => !document.querySelector('#media-refresh').disabled", timeout=4000)
    except Exception:
        pass
    return state.attach_bodies[-1] if len(state.attach_bodies) > before else None


def value_of(page, selector):
    return page.eval_on_selector(selector, "el => el.value")


def empty_value(page, selector):
    return value_of(page, selector) == ""


def media_default_true(browser, port, checks):
    state = State(default_ro=True)
    names = ["default-on.img", "explicit-off.img", "disc.iso", "cdrom.img",
             "payload-on.img", "payload-off.img"]
    state.media_items = [{"name": n, "size": 1024} for n in names]
    state.media_status = {"state": "detached"}
    page = new_page(browser, state, port)
    wait_main(page)
    page.click('button[data-panel-target="media"]')
    wait_media(page, len(names))
    check(checks, media_readonly(page, "default-on.img").is_checked(),
          "media_default_true_unmounted_checked")
    check(checks, media_readonly(page, "disc.iso").is_checked(),
          "media_iso_forced_checked")
    check(checks, media_readonly(page, "disc.iso").is_disabled(),
          "media_iso_locked")
    media_item(page, "cdrom.img").locator("select.media-type-select").select_option("cdrom")
    check(checks, media_readonly(page, "cdrom.img").is_checked(),
          "media_cdrom_forced_checked")
    check(checks, media_readonly(page, "cdrom.img").is_disabled(),
          "media_cdrom_locked")
    media_readonly(page, "explicit-off.img").uncheck()
    rerender_media(page)
    check(checks, not media_readonly(page, "explicit-off.img").is_checked(),
          "media_explicit_false_survives_rerender")
    media_readonly(page, "payload-off.img").uncheck()
    payload = attach_payload(page, state, "payload-off.img")
    check(checks, payload is not None and payload.get("read_only") is False,
          "media_attach_payload_matches_false", payload)
    payload = attach_payload(page, state, "payload-on.img")
    check(checks, payload is not None and payload.get("read_only") is True,
          "media_attach_payload_matches_true", payload)
    return page


def media_mounted(browser, port, checks):
    state = State(default_ro=True)
    state.media_items = [{"name": "mounted-rw.img", "size": 1024, "mounted": True}]
    state.media_status = {"state": "attached", "mounted_name": "mounted-rw.img",
                          "media_type": "disk", "read_only": False}
    page = new_page(browser, state, port)
    wait_main(page)
    page.click('button[data-panel-target="media"]')
    wait_media(page, 1)
    check(checks, not media_readonly(page, "mounted-rw.img").is_checked(),
          "media_mounted_actual_readwrite")
    state.media_status = {"state": "attached", "mounted_name": "mounted-rw.img",
                          "media_type": "disk", "read_only": True}
    rerender_media(page)
    check(checks, media_readonly(page, "mounted-rw.img").is_checked(),
          "media_mounted_actual_readonly")
    return page


def media_default_false(browser, port, checks):
    state = State(default_ro=False)
    state.media_items = [{"name": "default-off.img", "size": 1024},
                         {"name": "explicit-on.img", "size": 1024}]
    state.media_status = {"state": "detached"}
    page = new_page(browser, state, port)
    wait_main(page)
    page.click('button[data-panel-target="media"]')
    wait_media(page, 2)
    check(checks, not media_readonly(page, "default-off.img").is_checked(),
          "media_default_false_unmounted_unchecked")
    media_readonly(page, "explicit-on.img").check()
    rerender_media(page)
    check(checks, media_readonly(page, "explicit-on.img").is_checked(),
          "media_explicit_true_survives_rerender")
    payload = attach_payload(page, state, "default-off.img")
    check(checks, payload is not None and payload.get("read_only") is False,
          "media_default_false_payload", payload)
    payload = attach_payload(page, state, "explicit-on.img")
    check(checks, payload is not None and payload.get("read_only") is True,
          "media_explicit_true_payload", payload)
    return page


def media_default_absent(browser, port, checks):
    state = State(default_ro=False, include_default=False)
    state.media_items = [{"name": "absent-field.img", "size": 1024}]
    state.media_status = {"state": "detached"}
    page = new_page(browser, state, port)
    wait_main(page)
    page.click('button[data-panel-target="media"]')
    wait_media(page, 1)
    check(checks, not media_readonly(page, "absent-field.img").is_checked(),
          "media_absent_default_falls_back_false")
    return page


def login_success_before_bootstrap_failure(browser, port, checks):
    state = State(authenticated=False, mass_storage=False)
    page = new_page(browser, state, port)
    wait_login(page)
    check(checks, page.get_attribute('#login-form input[name="password"]', "autocomplete") == "current-password",
          "login_password_autocomplete_preserved")
    page.fill('#login-form input[name="username"]', USER)
    page.fill('#login-form input[name="password"]', PASSWORD)
    state.hang_bootstrap = True
    page.click('#login-form button[type="submit"]')
    cleared = wait_until(page, lambda: empty_value(page, '#login-form input[name="password"]'))
    check(checks, cleared, "login_success_clears_password_before_bootstrap")
    check(checks, value_of(page, '#login-form input[name="username"]') == USER,
          "login_success_preserves_username")
    for route in state.hung_bootstrap:
        try:
            route.abort()
        except Exception:
            pass
    state.hung_bootstrap.clear()
    state.hang_bootstrap = False
    page.wait_for_timeout(300)
    check(checks, empty_value(page, '#login-form input[name="password"]'),
          "login_bootstrap_failure_keeps_password_clear")
    return page


def auth_transitions(browser, port, checks):
    state = State(authenticated=False, has_passkeys=True, mass_storage=False)
    page = new_page(browser, state, port)
    wait_login(page)
    page.fill('#login-form input[name="username"]', USER)
    page.fill('#login-form input[name="password"]', PASSWORD)
    page.click('#login-form button[type="submit"]')
    wait_main(page)
    check(checks, empty_value(page, '#login-form input[name="password"]'),
          "login_success_clears_password")
    check(checks, value_of(page, '#login-form input[name="username"]') == USER,
          "login_success_keeps_username_attribute_value")
    page.eval_on_selector('#login-form input[name="username"]', "el => el.value = 'background-user'")
    page.eval_on_selector('#login-form input[name="password"]', "el => el.value = 'background-secret'")
    state.status401 = True
    wait_login(page, timeout=10000)
    check(checks, empty_value(page, '#login-form input[name="password"]'),
          "status401_clears_password")
    check(checks, value_of(page, '#login-form input[name="username"]') == 'background-user',
          "status401_preserves_username")
    page.fill('#login-form input[name="password"]', 'reentry-secret')
    state.fail_passkey_start = True
    page.wait_for_selector('#passkey-login-btn:visible', timeout=4000)
    page.click('#passkey-login-btn')
    page.wait_for_timeout(400)
    check(checks, not empty_value(page, '#login-form input[name="password"]'),
          "background401_while_login_preserves_reentry")
    state.login_fail = True
    page.fill('#login-form input[name="username"]', USER)
    page.fill('#login-form input[name="password"]', PASSWORD)
    page.click('#login-form button[type="submit"]')
    cleared = wait_until(page, lambda: empty_value(page, '#login-form input[name="password"]'))
    check(checks, cleared, "failed_login_clears_password")
    check(checks, value_of(page, '#login-form input[name="username"]') == USER,
          "failed_login_preserves_username")
    state.login_fail = False
    page.fill('#login-form input[name="password"]', PASSWORD)
    page.click('#login-form button[type="submit"]')
    wait_main(page)
    page.click("#logout")
    wait_login(page)
    check(checks, empty_value(page, '#login-form input[name="password"]'),
          "logout_clears_password")
    count = state.login_requests
    page.click('#login-form button[type="submit"]')
    page.wait_for_timeout(300)
    check(checks, state.login_requests == count,
          "logout_empty_submit_sends_no_request")
    return page


def setup_success_before_bootstrap_failure(browser, port, checks):
    state = State(authenticated=False, setup_required=True, token_required=True, mass_storage=False)
    page = new_page(browser, state, port, "#setup=" + SETUP_TOKEN)
    wait_setup(page)
    check(checks, value_of(page, '#setup-form input[name="setup_token"]') == SETUP_TOKEN,
          "setup_url_token_survives_initial_show_setup")
    check(checks, page.locator('#setup-token-field').is_hidden(),
          "setup_url_token_field_stays_hidden")
    check(checks, page.get_attribute('#setup-form input[name="setup_token"]', "autocomplete") == "off" and
          page.get_attribute('#setup-form input[name="password"]', "autocomplete") == "new-password" and
          page.get_attribute('#setup-form input[name="confirm_password"]', "autocomplete") == "new-password",
          "setup_password_manager_autocomplete_preserved")
    page.fill('#setup-form input[name="username"]', USER)
    page.fill('#setup-form input[name="password"]', PASSWORD)
    page.fill('#setup-form input[name="confirm_password"]', PASSWORD)
    page.click('#setup-form [data-setup-next]')
    page.wait_for_selector('#setup-form [data-setup-step="1"].active', timeout=3000)
    state.hang_bootstrap = True
    page.click('#setup-submit')
    cleared = wait_until(page, lambda: empty_value(page, '#setup-form input[name="password"]'))
    check(checks, cleared, "setup_success_clears_password_before_bootstrap")
    check(checks, empty_value(page, '#setup-form input[name="confirm_password"]'),
          "setup_success_clears_confirm_password_before_bootstrap")
    check(checks, empty_value(page, '#setup-form input[name="setup_token"]'),
          "setup_success_clears_token_before_bootstrap")
    check(checks, not page.locator('#setup-token-field').evaluate("el => el.classList.contains('hidden')"),
          "setup_token_clear_unhides_required_field")
    check(checks, value_of(page, '#setup-form input[name="username"]') == USER,
          "setup_success_preserves_username")
    for route in state.hung_bootstrap:
        try:
            route.abort()
        except Exception:
            pass
    state.hung_bootstrap.clear()
    state.hang_bootstrap = False
    failed = wait_until(page, lambda: page.locator('#setup-error').inner_text().strip() != "")
    check(checks, failed, "setup_bootstrap_failure_reported")
    check(checks, empty_value(page, '#setup-form input[name="password"]') and
          empty_value(page, '#setup-form input[name="confirm_password"]'),
          "setup_bootstrap_failure_keeps_passwords_clear")
    page.click('#setup-form [data-setup-prev]')
    page.fill('#setup-form input[name="setup_token"]', SETUP_TOKEN)
    page.fill('#setup-form input[name="password"]', PASSWORD)
    page.fill('#setup-form input[name="confirm_password"]', PASSWORD)
    state.setup_fail = True
    page.click('#setup-form [data-setup-next]')
    page.click('#setup-submit')
    cleared = wait_until(page, lambda: empty_value(page, '#setup-form input[name="password"]') and
                         empty_value(page, '#setup-form input[name="confirm_password"]'))
    check(checks, cleared, "failed_setup_clears_passwords")
    check(checks, value_of(page, '#setup-form input[name="setup_token"]') == SETUP_TOKEN,
          "failed_setup_keeps_valid_token")
    return page


def main():
    args = ui_harness.parse_args()
    summary = ui_harness.new_summary(args.repo)
    checks = summary["checks"]
    current = media_page = None
    try:
        with ui_harness.static_server(args.repo) as port, sync_playwright() as p:
            browser = p.chromium.launch(headless=True, channel=args.browser_channel)
            try:
                current = media_page = media_default_true(browser, port, checks)
                current = media_mounted(browser, port, checks)
                current = media_default_false(browser, port, checks)
                current = media_default_absent(browser, port, checks)
                current = login_success_before_bootstrap_failure(browser, port, checks)
                current = auth_transitions(browser, port, checks)
                current = setup_success_before_bootstrap_failure(browser, port, checks)
                summary["status"] = "PASS"
                if ui_harness.screenshot(media_page, args.output_dir, "p2_ui_pass.png"):
                    summary["screenshots"] = ["p2_ui_pass.png"]
            except Exception:
                if ui_harness.screenshot(current, args.output_dir, "p2_ui_failure.png"):
                    summary["screenshots"] = ["p2_ui_failure.png"]
                raise
            finally:
                browser.close()
    except Exception as exc:
        summary["error"] = f"{type(exc).__name__}: {exc}"
    return ui_harness.finish(summary, args.output_dir, "p2_ui_report.json")


if __name__ == "__main__":
    sys.exit(main())
