"""Shared fixtures for the Playwright UI regression suites.

Every suite (verify_p1_ui, verify_p2_ui, verify_p2b_ui, verify_audit2_ui)
serves the self-contained UI bundle from a local static server, mocks /api/**
inside the browser, records checks in the same JSON shape and prints one JSON
summary line. This module holds those common pieces so a suite only defines
its own API mock and scenarios. verify_ui_suite.py runs all of them.
"""
import argparse, json, os, threading, time, urllib.parse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from ui_bundle import REPO, load_index_html

GIF = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;"
ICON = b'<svg xmlns="http://www.w3.org/2000/svg"/>'
XTERM_ASSETS = ("xterm.js", "addon-fit.js", "xterm.css")


def extract_html(repo):
    return load_index_html(repo)


def read_assets(repo):
    root = os.path.join(repo, "web", "vendor", "xterm")
    assets = {}
    for name in XTERM_ASSETS:
        with open(os.path.join(root, name), "rb") as fh:
            assets[name] = fh.read()
    return assets


def serve_static(html, assets):
    """Serve the inlined page plus vendored xterm assets on 127.0.0.1:<random>."""
    html_bytes = html.encode("utf-8")
    files = {
        "/assets/xterm.js": (assets["xterm.js"], "application/javascript"),
        "/assets/xterm-fit.js": (assets["addon-fit.js"], "application/javascript"),
        "/assets/xterm.css": (assets["xterm.css"], "text/css"),
        "/assets/icon.svg": (ICON, "image/svg+xml"),
        "/favicon.ico": (ICON, "image/svg+xml"),
    }

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
    server, port = serve_static(extract_html(repo) if html is None else html, read_assets(repo))
    try:
        yield port
    finally:
        stop_server(server)


def check(checks, condition, name, evidence="", limit=256):
    checks.append({"name": name, "ok": bool(condition), "evidence": str(evidence)[:limit]})
    if not condition:
        raise AssertionError(f"{name}: {evidence}")


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
