#!/usr/bin/env python3
"""P2b UI regression: video freshness (ISSUE-13) and local keyboard exit (ISSUE-15).

All API responses are mocked; the checks exercise the real DOM, real browser
keyboard events and the real UI state machine. No capture hardware is touched.
"""
import json, os, sys, time, urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:
    from playwright.sync_api import sync_playwright
    import ui_harness
    from ui_harness import GIF, check, extract_html, mock_video_feed, read_assets, serve_static
except Exception as exc:  # pragma: no cover - environment guard
    print(json.dumps({"status": "FAIL", "error": f"Playwright/helper import failed: {exc}"}))
    sys.exit(2)

BASE_VIDEO = {
    "state": "starting", "message": "正在等待首帧…", "frames_per_second": None,
    "width": 1280, "height": 720, "last_frame_unix_ms": None, "last_frame_age_ms": None,
}


class State:
    def __init__(self, pointer_mode="absolute"):
        self.pointer_mode = pointer_mode
        self.video = dict(BASE_VIDEO)
        self.key_posts = []
        self.mouse_posts = []
        self.unmocked = []
        self.release_posts = 0
        self.status_polls = 0


def bootstrap_payload(state):
    return {
        "setup_required": False, "token_required": False, "authenticated": True,
        "username": "admin", "client_ip": "127.0.0.1",
        "client": {"ip": "127.0.0.1", "port": 12345, "source": "socket"},
        "config": {
            "server": {"proxy_protocol": "auto", "trusted_http_proxies": []},
            "video": {"device": "/dev/video0", "width": 1280, "height": 720,
                      "frames_per_second": 30, "jpeg_quality": 80, "encoding": "mjpeg"},
            "hid": {"keyboard_device": "/dev/hidg0", "mouse_device": "/dev/hidg1",
                    "absolute_pointer_device": "/dev/hidg2",
                    "pointer_mode": state.pointer_mode},
            "power": {"enabled": False, "gpio_chip": None, "gpio_line": None,
                      "active_high": True},
            "media": {"enabled": False, "lun_path": None, "image_directory": None},
        },
        "video": state.video,
        "webrtc": {"available": False, "reason": "disabled by test"},
        "capabilities": {"video": True, "hid": True, "power": False,
                         "mass_storage": False, "terminal": True,
                         "pointer_mode": state.pointer_mode, "video_webrtc_h264": False},
    }


def api_route(route, state):
    path = urllib.parse.urlsplit(route.request.url).path
    headers = {"content-type": "application/json"}
    if path == "/api/bootstrap":
        return route.fulfill(status=200, headers=headers,
                             body=json.dumps(bootstrap_payload(state)))
    if path == "/api/status":
        state.status_polls += 1
        payload = {
            "video": state.video,
            "power": {"power_led": {"configured": False, "state": "unknown"}},
            "hid": {"keyboard_leds": {"num_lock": False, "caps_lock": False,
                                      "scroll_lock": False, "raw": 0}},
            "client": {"ip": "127.0.0.1", "port": 12345, "source": "socket"},
            "system": {"cpu": {"percent": 2.0, "cores": 2},
                       "load_avg": [0.01, 0.02, 0.03],
                       "memory": {"total_bytes": 1048576, "used_bytes": 524288, "percent": 50},
                       "disk": {"total_bytes": 4194304, "used_bytes": 1048576, "percent": 25},
                       "uptime_seconds": 60},
        }
        return route.fulfill(status=200, headers=headers, body=json.dumps(payload))
    if path == "/api/key":
        try:
            body = json.loads(route.request.post_data or "{}")
        except Exception:
            body = {}
        state.key_posts.append(body)
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/input/release-all":
        state.release_posts += 1
        return route.fulfill(status=200, headers=headers, body='{"ok":true}')
    if path == "/api/passkey/list":
        return route.fulfill(status=200, headers=headers, body='{"passkeys":[]}')
    if path == "/api/devices/scan":
        return route.fulfill(status=200, headers=headers, body='{"devices":{}}')
    if path == "/api/media":
        return route.fulfill(status=200, headers=headers, body='{"items":[]}')
    method = route.request.method
    if path.startswith("/api/mouse/") and method == "POST":
        try:
            body = json.loads(route.request.post_data or "{}")
        except Exception:
            body = {}
        state.mouse_posts.append((path.rsplit("/", 1)[1], body))
        return route.fulfill(status=204)
    # Real GET endpoints the console polls on its own, in the shapes the
    # server returns (src/web/session.rs, audit_api.rs, update.rs, config_api.rs).
    if method == "GET" and path == "/api/sessions":
        return route.fulfill(status=200, headers=headers, body='{"sessions":[]}')
    if method == "GET" and path == "/api/audit":
        return route.fulfill(status=200, headers=headers, body='{"entries":[]}')
    if method == "GET" and path == "/api/update/status":
        return route.fulfill(status=200, headers=headers, body=json.dumps(
            {"current_version": "0.0.0-test", "supported": False, "status": {"state": "idle"}}))
    if method == "GET" and path == "/api/config":
        return route.fulfill(status=200, headers=headers,
                             body=json.dumps(bootstrap_payload(state)["config"]))
    # Anything else is not mocked: answer like an unknown route instead of a
    # blanket 200 so a renamed or new endpoint cannot pass unnoticed (AUD3-33).
    state.unmocked.append(f"{method} {path}")
    return route.fulfill(status=404, headers=headers,
                         body=json.dumps({"error": f"unmocked endpoint {method} {path}"}))


def new_page(browser, state, port, init_scripts=()):
    context = browser.new_context(viewport={"width": 1280, "height": 900})
    page = context.new_page()
    for script in init_scripts:
        page.add_init_script(script)
    page.route("**/api/**", lambda route: api_route(route, state))
    mock_video_feed(page)
    page.goto(f"http://127.0.0.1:{port}/", wait_until="load", timeout=15000)
    page.wait_for_selector("#app:not(.hidden)", state="visible", timeout=8000)
    page.bring_to_front()
    return context, page


# Records every requestPointerLock/requestFullscreen call and how the browser
# answered it, without changing what the browser does. A check may only be
# skipped when the app asked and the browser refused (AUD3-11).
CAPTURE_SPY = """
(() => {
  const calls = (window.__captureCalls = []);
  const settle = (call, outcome) => { if (call.outcome === 'pending') call.outcome = outcome; };
  const wrap = (name, kind) => {
    const original = Element.prototype[name];
    if (typeof original !== 'function') return;
    Element.prototype[name] = function (...args) {
      const call = {kind, target: this.id || this.tagName, outcome: 'pending'};
      calls.push(call);
      let result;
      try {
        result = original.apply(this, args);
      } catch (error) {
        settle(call, 'refused: ' + error.name);
        throw error;
      }
      if (result && typeof result.then === 'function')
        result.then(() => settle(call, 'granted'), (error) => settle(call, 'refused: ' + (error && error.name)));
      return result;
    };
  };
  wrap('requestPointerLock', 'pointerlock');
  wrap('requestFullscreen', 'fullscreen');
  const refuse = (kind, why) => () => calls.forEach((call) => call.kind === kind && settle(call, why));
  document.addEventListener('pointerlockerror', refuse('pointerlock', 'refused: pointerlockerror'));
  document.addEventListener('fullscreenerror', refuse('fullscreen', 'refused: fullscreenerror'));
  document.addEventListener('pointerlockchange', () => {
    if (document.pointerLockElement) calls.forEach((call) => call.kind === 'pointerlock' && settle(call, 'granted'));
  });
})();
"""


def capture_calls(page, kind, engaged_js, timeout=2.0):
    """Spy records of `kind` and the id of the engaged element ('' if none),
    waiting until the last request is engaged, granted or refused."""
    deadline = time.monotonic() + timeout
    while True:
        calls = page.evaluate("(kind) => (window.__captureCalls || []).filter((c) => c.kind === kind)", kind)
        engaged = page.evaluate(engaged_js)
        if engaged or (calls and calls[-1]["outcome"] != "pending") or time.monotonic() > deadline:
            return calls, engaged
        page.wait_for_timeout(50)


def capture_outcome(checks, kind, engaged_name, calls, engaged, target):
    """Fail unless the app asked for `target`; skip only when the browser refused."""
    check(checks, bool(calls) and calls[-1]["target"] == target,
          f"{kind}_requested_by_app", calls or "the app never called the API")
    if engaged:
        check(checks, engaged == target, engaged_name, {"engaged": engaged, "calls": calls})
        return True
    outcome = calls[-1]["outcome"]
    if outcome.startswith("refused"):
        checks.append({"name": engaged_name, "ok": None, "skipped": True, "supported": False,
                       "evidence": f"the app asked; the browser refused ({outcome}); not claimed"})
        return False
    check(checks, False, engaged_name, f"requested but neither engaged nor refused: {calls}")
    return False
    check(checks, False, f"{name}_real_engaged",
          f"requested but neither engaged nor refused: {calls}")
    return False


def force_video_playing(page):
    """The mocked GIF is a real image load; dispatch load for deterministic timing."""
    page.evaluate("""() => {
      const feed = document.querySelector('#video-feed');
      if (feed && feed.getAttribute('src')) feed.dispatchEvent(new Event('load'));
    }""")


def dom_state(page):
    return page.evaluate("""() => {
      const message = document.querySelector('#video-message');
      const meta = document.querySelector('#video-transport-label');
      const label = document.querySelector('#status-label');
      const feed = document.querySelector('#video-feed');
      const active = document.activeElement;
      return {
        text: message ? message.textContent || '' : '',
        visible: message ? !message.classList.contains('hidden') &&
          getComputedStyle(message).display !== 'none' : false,
        meta: meta ? meta.textContent || '' : '',
        label: label ? label.textContent || '' : '',
        feed_hidden: feed ? feed.classList.contains('hidden') : true,
        remote: !!(document.querySelector('#remote-input') || {}).checked,
        active: active ? (active.id || active.tagName || '') : '',
        fullscreen: !!document.fullscreenElement,
        pointerlock: document.pointerLockElement === document.querySelector('#video-viewport'),
      };
    }""")


def wait_dom(page, predicate, timeout=9.0):
    deadline = time.monotonic() + timeout
    snapshot = dom_state(page)
    while time.monotonic() < deadline:
        snapshot = dom_state(page)
        if predicate(snapshot):
            return True, snapshot
        page.wait_for_timeout(100)
    return False, snapshot


def save(page, output_dir, name):
    if not output_dir:
        return
    try:
        os.makedirs(output_dir, exist_ok=True)
        page.screenshot(path=os.path.join(output_dir, name))
    except Exception:
        pass


def video_freshness(browser, port, checks, output_dir):
    state = State()
    context, page = new_page(browser, state, port)
    try:
        force_video_playing(page)
        page.wait_for_timeout(200)

        ok, snap = wait_dom(page, lambda s: s["visible"] and "等待首帧" in s["text"])
        check(checks, ok, "starting_overlay_explains_waiting", snap)
        check(checks, "LIVE" not in snap["meta"] and "LIVE" not in snap["label"],
              "starting_never_says_live", snap)
        check(checks, snap["feed_hidden"], "starting_hides_stale_frame", snap)
        save(page, output_dir, "p2b_ui_starting.png")

        now_ms = int(time.time() * 1000)
        state.video.update(state="ready", message=None, frames_per_second=30.0,
                           last_frame_unix_ms=now_ms, last_frame_age_ms=200)
        ok, snap = wait_dom(page, lambda s: "LIVE" in s["meta"] and not s["visible"])
        check(checks, ok, "ready_resumes_live_and_clears_overlay", snap)
        check(checks, not snap["feed_hidden"], "ready_restores_video_surface", snap)

        state.video.update(state="stalled", message="超过 5 秒未收到有效视频帧",
                           frames_per_second=None,
                           last_frame_unix_ms=now_ms - 6500, last_frame_age_ms=6500)
        ok, snap = wait_dom(page, lambda s: s["visible"]
                            and ("停止" in s["text"] or "未收到有效视频帧" in s["text"])
                            and ("秒前" in s["text"] or "最近有效帧" in s["text"]))
        check(checks, ok, "stalled_overlay_shows_age", snap)
        check(checks, "LIVE" not in snap["meta"] and "LIVE" not in snap["label"],
              "stalled_never_says_live", snap)
        check(checks, snap["feed_hidden"], "stalled_hides_stale_frame", snap)
        save(page, output_dir, "p2b_ui_stalled.png")

        now_ms = int(time.time() * 1000)
        state.video.update(state="ready", message=None, frames_per_second=30.0,
                           last_frame_unix_ms=now_ms, last_frame_age_ms=100)
        ok, snap = wait_dom(page, lambda s: "LIVE" in s["meta"] and not s["visible"])
        check(checks, ok, "recovery_resumes_live_without_reconfigure", snap)
    except Exception:
        save(page, output_dir, "p2b_ui_failure.png")
        raise
    finally:
        context.close()


def enable_remote_by_keyboard(page):
    page.focus("#remote-input")
    page.keyboard.press("Space")
    page.wait_for_function(
        "() => document.querySelector('#remote-input').checked && "
        "document.querySelector('#input-state').classList.contains('active')",
        timeout=3000)


def tab_to(page, target_id, max_tabs=200):
    for _ in range(max_tabs):
        if page.evaluate("() => (document.activeElement && document.activeElement.id) || ''") == target_id:
            return True
        page.keyboard.press("Tab")
    return page.evaluate("() => (document.activeElement && document.activeElement.id) || ''") == target_id


def wait_key(page, state, key, timeout=2.5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if any(post.get("key") == key for post in state.key_posts):
            return True
        page.wait_for_timeout(50)
    return any(post.get("key") == key for post in state.key_posts)


def keyboard_exit(browser, port, checks, output_dir):
    state = State()
    context, page = new_page(browser, state, port)
    try:
        force_video_playing(page)
        page.wait_for_timeout(200)
        check(checks, page.locator("#input-shortcut-hint").is_visible(),
              "shortcut_hint_visible_before_capture")
        check(checks, "input-shortcut-hint" in (page.get_attribute("#remote-input", "aria-describedby") or ""),
              "shortcut_hint_linked_to_toggle")
        check(checks, "local-exit-hint" in (page.get_attribute("#video-viewport", "aria-describedby") or ""),
              "shortcut_hint_linked_to_viewport")
        enable_remote_by_keyboard(page)
        check(checks, page.is_checked("#remote-input"), "keyboard_toggle_enables_remote")

        check(checks, tab_to(page, "video-viewport"), "keyboard_tab_enters_viewport",
              page.evaluate("() => document.activeElement && document.activeElement.id"))

        state.key_posts.clear()
        page.keyboard.press("Tab")
        check(checks, wait_key(page, state, "Tab"), "remote_tab_forwarded", state.key_posts)
        page.keyboard.press("Escape")
        check(checks, wait_key(page, state, "Escape"), "remote_escape_forwarded", state.key_posts)

        state.key_posts.clear()
        release_before = state.release_posts
        page.keyboard.press("Control+Alt+Backquote")
        page.wait_for_timeout(400)
        snap = dom_state(page)
        check(checks, not snap["remote"], "local_shortcut_disables_remote", snap)
        check(checks, state.release_posts > release_before, "local_shortcut_releases_input",
              state.release_posts)
        check(checks, not any(str(post.get("key", "")).lower() in ("backquote", "`", "~")
                              for post in state.key_posts),
              "local_shortcut_not_sent_remote", state.key_posts)
        check(checks, snap["active"] in ("remote-input", "input-state", "inspector-toggle"),
              "local_shortcut_restores_local_focus", snap)
        local_count = len(state.key_posts)
        page.keyboard.press("Tab")
        page.wait_for_timeout(250)
        check(checks, len(state.key_posts) == local_count,
              "tab_after_exit_navigates_locally", state.key_posts)

        page.click('button[data-panel-target="video"]')
        page.focus('#video-config input[name="width"]')
        page.keyboard.press("a")
        page.keyboard.press("Control+Alt+Backquote")
        page.wait_for_timeout(250)
        check(checks, len(state.key_posts) == local_count,
              "normal_form_typing_and_local_shortcut_preserved", state.key_posts)
        check(checks, not page.is_checked("#remote-input"),
              "local_shortcut_inactive_when_remote_off")
    except Exception:
        save(page, output_dir, "p2b_ui_failure_keyboard.png")
        raise
    finally:
        context.close()


def relative_pointerlock(browser, port, checks, summary, output_dir):
    state = State(pointer_mode="relative")
    context, page = new_page(browser, state, port, init_scripts=(CAPTURE_SPY,))
    try:
        force_video_playing(page)
        page.wait_for_timeout(200)
        enable_remote_by_keyboard(page)
        state.key_posts.clear()
        page.focus("#video-viewport")
        page.keyboard.press("Enter")
        calls, engaged = capture_calls(
            page, "pointerlock", "() => (document.pointerLockElement || {}).id || ''")
        check(checks, not any(str(post.get("key", "")).lower() == "enter"
                              for post in state.key_posts),
              "relative_capture_enter_not_forwarded", state.key_posts)
        locked = capture_outcome(checks, "pointerlock", "pointerlock_real_engaged",
                                 calls, engaged, "video-viewport")
        summary["browser_features"]["pointer_lock"] = (
            "real" if locked else "refused by the browser (not claimed)")
        summary["browser_features"]["pointer_lock_supported"] = locked
        if locked:
            state.key_posts.clear()
            page.keyboard.press("Enter")
            check(checks, wait_key(page, state, "Enter"), "locked_enter_forwarded",
                  state.key_posts)

        release_before = state.release_posts
        page.keyboard.press("Control+Alt+Backquote")
        page.wait_for_timeout(400)
        snap = dom_state(page)
        check(checks, not snap["remote"], "relative_mode_shortcut_exits_remote", snap)
        check(checks, state.release_posts > release_before,
              "relative_mode_shortcut_releases_input", state.release_posts)
        if locked:
            check(checks, not snap["pointerlock"], "pointerlock_released_by_shortcut", snap)
        else:
            checks.append({"name": "pointerlock_released_by_shortcut", "ok": None, "skipped": True,
                           "supported": False,
                           "evidence": "browser refused pointer lock; release not claimed"})
    except Exception:
        save(page, output_dir, "p2b_ui_failure_pointerlock.png")
        raise
    finally:
        context.close()


def fullscreen_exit(browser, port, checks, summary, output_dir):
    state = State()
    context, page = new_page(browser, state, port, init_scripts=(CAPTURE_SPY,))
    try:
        force_video_playing(page)
        page.wait_for_timeout(200)
        enable_remote_by_keyboard(page)
        page.focus("#fullscreen")
        page.keyboard.press("Space")
        calls, engaged = capture_calls(
            page, "fullscreen", "() => (document.fullscreenElement || {}).id || ''")
        fullscreen = capture_outcome(checks, "fullscreen", "fullscreen_real_entered",
                                     calls, engaged, "console")
        summary["browser_features"]["fullscreen"] = (
            "real" if fullscreen else "refused by the browser (not claimed)")
        summary["browser_features"]["fullscreen_supported"] = fullscreen

        release_before = state.release_posts
        page.keyboard.press("Control+Alt+Backquote")
        page.wait_for_timeout(600)
        snap = dom_state(page)
        check(checks, not snap["remote"], "fullscreen_shortcut_exits_remote", snap)
        check(checks, state.release_posts > release_before,
              "fullscreen_shortcut_releases_input", state.release_posts)
        check(checks, snap["active"] in ("remote-input", "input-state", "inspector-toggle"),
              "fullscreen_shortcut_restores_local_focus", snap)
        if fullscreen:
            check(checks, not snap["fullscreen"], "fullscreen_exited_before_focus", snap)
        else:
            checks.append({"name": "fullscreen_exited_before_focus", "ok": None, "skipped": True,
                           "supported": False,
                           "evidence": "browser refused fullscreen; exit not claimed"})
    except Exception:
        save(page, output_dir, "p2b_ui_failure_fullscreen.png")
        raise
    finally:
        context.close()


def main():
    args = ui_harness.parse_args()
    summary = ui_harness.new_summary(args.repo, browser_features={})
    checks = summary["checks"]

    def screenshots():
        if args.output_dir and os.path.isdir(args.output_dir):
            return [name for name in sorted(os.listdir(args.output_dir))
                    if name.startswith("p2b_ui_") and name.endswith(".png")]
        return []

    try:
        with ui_harness.static_server(args.repo) as port, sync_playwright() as p:
            browser = ui_harness.launch_browser(p, args.browser_channel)
            try:
                video_freshness(browser, port, checks, args.output_dir)
                keyboard_exit(browser, port, checks, args.output_dir)
                relative_pointerlock(browser, port, checks, summary, args.output_dir)
                fullscreen_exit(browser, port, checks, summary, args.output_dir)
                summary["status"] = "PASS"
                if args.output_dir:
                    os.makedirs(args.output_dir, exist_ok=True)
                    summary["screenshots"] = screenshots()
            finally:
                browser.close()
    except Exception as exc:
        summary["error"] = f"{type(exc).__name__}: {exc}"
        if screenshots():
            summary["screenshots"] = screenshots()
    return ui_harness.finish(summary, args.output_dir, "p2b_ui_report.json")


if __name__ == "__main__":
    sys.exit(main())
