"""Shared fixtures for the Playwright UI regression suites.

Every suite (verify_p1_ui, verify_p2_ui, verify_p2b_ui, verify_audit2_ui)
serves the UI (index.html with the stylesheet inlined, plus the app ES modules
at /assets/js/) from a local static server, mocks /api/**
inside the browser, records checks in the same JSON shape and prints one JSON
summary line. This module holds those common pieces so a suite only defines
its own API mock and scenarios. verify_ui_suite.py runs all of them.
"""
import argparse, json, os, threading, time, urllib.parse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from ui_bundle import MODULE_TYPE, REPO, load_index_html, module_assets

GIF = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;"
ICON = b'<svg xmlns="http://www.w3.org/2000/svg"/>'
XTERM_ASSETS = ("xterm.js", "addon-fit.js", "xterm.css")
# Init script for scenarios that call window.__wingmanTest: the app modules
# only publish those hooks when this flag is set before they are evaluated, so
# add it with add_init_script before the first navigation (or reload after).
TEST_HOOKS = "window.__wingmanTestHooks = true;"


def extract_html(repo):
    return load_index_html(repo)


def read_assets(repo):
    root = os.path.join(repo, "web", "vendor", "xterm")
    assets = {}
    for name in XTERM_ASSETS:
        with open(os.path.join(root, name), "rb") as fh:
            assets[name] = fh.read()
    return assets


def serve_static(html, assets, modules=None):
    """Serve the page, the app ES modules and vendored xterm assets on 127.0.0.1:<random>."""
    html_bytes = html.encode("utf-8")
    files = {path: (body, MODULE_TYPE) for path, body in (modules or {}).items()}
    files.update({
        "/assets/xterm.js": (assets["xterm.js"], "application/javascript"),
        "/assets/xterm-fit.js": (assets["addon-fit.js"], "application/javascript"),
        "/assets/xterm.css": (assets["xterm.css"], "text/css"),
        "/assets/icon.svg": (ICON, "image/svg+xml"),
        "/favicon.ico": (ICON, "image/svg+xml"),
    })

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def _send(self, code, body, ctype):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)

        def _route(self):
            path = urllib.parse.urlsplit(self.path).path
            if path in ("/", "/index.html"):
                return self._send(200, html_bytes, "text/html; charset=utf-8")
            if path in files:
                return self._send(200, *files[path])
            return self._send(404, b"not found", "text/plain")

        do_GET = _route
        do_POST = _route

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


def stop_server(server):
    if server:
        server.shutdown()
        server.server_close()


@contextmanager
def static_server(repo, html=None):
    """Yield the port of a static server for `html` (default: the UI bundle)."""
    page = extract_html(repo) if html is None else html
    server, port = serve_static(page, read_assets(repo), module_assets(repo))
    try:
        yield port
    finally:
        stop_server(server)


def check(checks, condition, name, evidence="", limit=256):
    checks.append({"name": name, "ok": bool(condition), "evidence": str(evidence)[:limit]})
    if not condition:
        raise AssertionError(f"{name}: {evidence}")


# Uncaught errors (exceptions and unhandled rejections) from every page of a
# browser opened through launch_browser(). finish() fails the suite when any
# were seen and reports each message with its stack, so a crash at module
# evaluation shows up as the error itself instead of an 8-second selector
# timeout (AUD3-32).
PAGE_ERRORS = []
_tracking = {"enabled": False}


def _record_page_error(page, error):
    try:
        url = page.url if page else ""
    except Exception:
        url = ""
    PAGE_ERRORS.append({
        "message": getattr(error, "message", "") or str(error),
        "name": getattr(error, "name", "") or "",
        "stack": getattr(error, "stack", "") or "",
        "url": url,
    })


def track_page_errors(browser):
    """Record uncaught errors of every page `browser` opens from now on."""
    _tracking["enabled"] = True
    new_context, new_page = browser.new_context, browser.new_page

    def tracked_context(*args, **kwargs):
        context = new_context(*args, **kwargs)
        context.on("weberror", lambda web_error: _record_page_error(web_error.page, web_error.error))
        return context

    def tracked_page(*args, **kwargs):
        page = new_page(*args, **kwargs)
        page.on("pageerror", lambda error: _record_page_error(page, error))
        return page

    browser.new_context, browser.new_page = tracked_context, tracked_page
    return browser


def launch_browser(playwright, channel, **kwargs):
    """Headless Chromium whose pages are watched for uncaught errors."""
    kwargs.setdefault("headless", True)
    return track_page_errors(playwright.chromium.launch(channel=channel, **kwargs))


def page_errors_check(summary):
    """Fail `summary` when a tracked page threw; otherwise record a passing check."""
    if not _tracking["enabled"]:
        return
    if not PAGE_ERRORS:
        summary["checks"].append({"name": "no_uncaught_page_errors", "ok": True, "evidence": "none"})
        return
    summary["page_errors"] = list(PAGE_ERRORS)
    first = PAGE_ERRORS[0]
    detail = f"{len(PAGE_ERRORS)} uncaught page error(s); first: {first['message']} @ {first['url']}\n{first['stack']}"
    summary["checks"].append({"name": "no_uncaught_page_errors", "ok": False, "evidence": detail[:2000]})
    summary["status"] = "FAIL"
    summary["error"] = (summary.get("error", "") + "\n" if summary.get("error") else "") + detail[:4000]


# API paths the page asked for that no mock knows. They are answered like an
# unknown route, and because the app may swallow that 404, finish() also fails
# the suite: a renamed or new endpoint cannot pass unnoticed (AUD3-33).
UNMOCKED_REQUESTS = []


def answer_unmocked(route, path):
    entry = f"{route.request.method} {path}"
    if entry not in UNMOCKED_REQUESTS:
        UNMOCKED_REQUESTS.append(entry)
    return route.fulfill(status=404, headers={"content-type": "application/json"},
                         body=json.dumps({"error": f"unmocked endpoint {entry}"}))


def unmocked_requests_check(summary):
    """Fail `summary` when the page called an endpoint no mock answers."""
    if not _tracking["enabled"]:
        return
    if not UNMOCKED_REQUESTS:
        summary["checks"].append({"name": "no_unmocked_api_requests", "ok": True, "evidence": "none"})
        return
    detail = "page called endpoints no mock answers: " + ", ".join(UNMOCKED_REQUESTS)
    summary["checks"].append({"name": "no_unmocked_api_requests", "ok": False, "evidence": detail[:2000]})
    summary["status"] = "FAIL"
    summary["error"] = (summary.get("error", "") + "\n" if summary.get("error") else "") + detail[:2000]


def wait_until(page, predicate, timeout=3.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if predicate():
                return True
        except Exception:
            pass
        page.wait_for_timeout(50)
    try:
        return bool(predicate())
    except Exception:
        return False


def mock_video_feed(page):
    page.route("**/video_feed*", lambda route: route.fulfill(
        status=200, content_type="image/gif", body=GIF))


def parse_args(default_channel="chrome", description=None):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--output-dir", default="")
    parser.add_argument("--browser-channel", default=default_channel)
    return parser.parse_args()


def new_summary(repo, **extra):
    summary = {"status": "FAIL", "repo": os.path.abspath(repo), "checks": []}
    summary.update(extra)
    return summary


def finish(summary, output_dir, report_name):
    """Write the optional report file, print the JSON line, return the exit code."""
    page_errors_check(summary)
    unmocked_requests_check(summary)
    if output_dir:
        try:
            os.makedirs(output_dir, exist_ok=True)
            with open(os.path.join(output_dir, report_name), "w", encoding="utf-8") as fh:
                json.dump(summary, fh, ensure_ascii=False, indent=2)
        except Exception:
            pass
    print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
    return 0 if summary["status"] == "PASS" else 1


def screenshot(page, output_dir, name):
    """Best-effort screenshot; returns the file name or None."""
    if not (page and output_dir):
        return None
    try:
        os.makedirs(output_dir, exist_ok=True)
        page.screenshot(path=os.path.join(output_dir, name))
        return name
    except Exception:
        return None
