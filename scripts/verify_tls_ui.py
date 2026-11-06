#!/usr/bin/env python3
"""Exercise HTTPS certificate settings using local TLS and the real browser UI."""
import json
import os
from pathlib import Path
import ssl
import subprocess
import tempfile

from playwright.sync_api import sync_playwright
import ui_harness
from ui_bundle import module_assets
from verify_p2b_ui import State, api_route, mock_video_feed


def main():
    args = ui_harness.parse_args()
    summary = ui_harness.new_summary(args.repo)
    checks = summary["checks"]
    check = lambda condition, name, evidence="": ui_harness.check(checks, condition, name, evidence)
    server = None
    try:
        with tempfile.TemporaryDirectory(prefix="wk-tls-ui-") as temp, sync_playwright() as playwright:
            root = Path(temp)
            subprocess.run([
                "openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                "-nodes", "-days", "2", "-subj", "/CN=localhost",
                "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1",
                "-keyout", str(root / "key.pem"), "-out", str(root / "cert.pem"),
            ], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            ssl_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ssl_context.load_cert_chain(root / "cert.pem", root / "key.pem")
            server, port = ui_harness.serve_static(
                ui_harness.extract_html(args.repo), ui_harness.read_assets(args.repo), module_assets(args.repo), ssl_context)
            browser = ui_harness.launch_browser(playwright, args.browser_channel)
            context = browser.new_context(ignore_https_errors=True, viewport={"width": 1280, "height": 1000})
            page = context.new_page()
            state = State()
            status = {
                "mode": "self_signed", "enabled": True, "public_origin": f"https://localhost:{port}",
                "custom_configured": False,
                "certificate": {"subject": "WingmanKVM", "not_after": "Nov 4 00:00:00 2036 GMT", "fingerprint_sha256": "AA:BB:CC"},
            }
            posts = []
            websocket_urls = []
            page.on("websocket", lambda socket: websocket_urls.append(socket.url))
            page.route("**/api/**", lambda route: api_route(route, state))
            mock_video_feed(page)

            def tls_api(route):
                if route.request.method == "POST":
                    body = json.loads(route.request.post_data)
                    posts.append(body)
                    if body.get("private_key_pem") == "INVALID-TEST-KEY":
                        return route.fulfill(status=400, content_type="application/json", body=json.dumps({"error": "证书与私钥不匹配"}))
                    status["mode"] = body["mode"]
                    status["public_origin"] = body["public_origin"]
                    if body.get("certificate_pem"):
                        status["custom_configured"] = True
                return route.fulfill(status=200, content_type="application/json", body=json.dumps(status))

            page.route("**/api/tls", tls_api)
            page.goto(f"https://localhost:{port}/", wait_until="load")
            page.wait_for_selector("#app:not(.hidden)", timeout=8000)
            page.locator("#settings-toggle").click()
            page.locator("#tls-settings").scroll_into_view_if_needed()
            page.wait_for_function("document.querySelector('#tls-status').textContent.includes('自动自签')")
            form = page.locator("#tls-config")
            check(form.locator('[name="mode"]').input_value() == "self_signed", "default_self_signed_mode")
            check(not page.locator("#tls-custom-fields").is_visible(), "custom_upload_hidden_for_self_signed")
            check(page.locator("#tls-download").is_visible(), "public_certificate_download_available")
            check(page.evaluate("window.isSecureContext"), "settings_served_in_secure_context")
            form.locator('[name="mode"]').select_option("custom")
            check(page.locator("#tls-custom-fields").is_visible(), "custom_mode_reveals_upload_controls")
            form.locator('button[type="submit"]').click()
            page.wait_for_function("document.querySelector('#tls-result').textContent.includes('请选择')")
            check(not posts, "custom_mode_requires_both_files")

            def upload(key):
                form.locator('[name="certificate"]').set_input_files({"name": "certificate.pem", "mimeType": "application/x-pem-file", "buffer": b"TEST-CERTIFICATE"})
                form.locator('[name="private_key"]').set_input_files({"name": "private.key", "mimeType": "application/x-pem-file", "buffer": key.encode()})
                form.locator('button[type="submit"]').click()

            upload("INVALID-TEST-KEY")
            page.wait_for_function("document.querySelector('#tls-result').textContent.includes('不匹配')")
            check(status["mode"] == "self_signed", "failed_import_keeps_active_certificate")
            check(form.locator('[name="private_key"]').input_value() == "", "failed_import_clears_file_selection")
            upload("LOCAL-FIXTURE-PRIVATE-KEY")
            page.wait_for_function("document.querySelector('#tls-result').textContent.includes('已应用')")
            check(posts[-1]["private_key_pem"] == "LOCAL-FIXTURE-PRIVATE-KEY", "custom_import_submits_selected_files")
            check("LOCAL-FIXTURE-PRIVATE-KEY" not in page.locator("body").inner_text(), "private_key_not_rendered")
            check(form.locator('[name="private_key"]').input_value() == "", "successful_import_clears_file_selection")
            check(page.locator("#tls-open").get_attribute("href") == f"https://localhost:{port}/", "https_link_preserves_custom_port")
            form.locator('[name="mode"]').select_option("self_signed")
            form.locator('button[type="submit"]').click()
            page.wait_for_function("document.querySelector('#tls-status').textContent.includes('自动自签')")
            check(status["custom_configured"], "switching_back_retains_custom_certificate")
            form.locator('[name="mode"]').select_option("custom")
            form.locator('button[type="submit"]').click()
            page.wait_for_function("document.querySelector('#tls-status').textContent.includes('自定义')")
            check("private_key_pem" not in posts[-1], "saved_custom_certificate_reused_without_key_upload")
            if args.output_dir:
                os.makedirs(args.output_dir, exist_ok=True)
                page.locator("#tls-settings").screenshot(path=str(Path(args.output_dir) / "tls-settings-desktop.png"))
                page.set_viewport_size({"width": 390, "height": 844})
                page.locator("#tls-settings").scroll_into_view_if_needed()
                page.locator("#tls-settings").screenshot(path=str(Path(args.output_dir) / "tls-settings-mobile.png"))
                check(page.locator("#tls-config").evaluate("e => e.scrollWidth <= e.clientWidth + 1"), "mobile_form_does_not_overflow")
            form.locator('[name="mode"]').select_option("self_signed")
            form.locator('[name="regenerate"]').check()
            form.locator('button[type="submit"]').click()
            page.wait_for_function("document.querySelector('#tls-status').textContent.includes('自动自签')")
            check(posts[-1].get("regenerate") is True, "explicit_self_signed_regeneration")
            check(not form.locator('[name="regenerate"]').is_checked(), "regeneration_not_repeated_on_next_save")
            if args.output_dir:
                page.locator("#tls-settings").screenshot(path=str(Path(args.output_dir) / "tls-settings-self-signed.png"))
            page.set_viewport_size({"width": 1280, "height": 1000})
            for language, heading in (("en", "HTTPS and certificates"), ("zh-TW", "HTTPS 與憑證")):
                page.locator("#settings-dialog").press("Escape")
                page.locator("#lang-toggle").click()
                page.locator(f'[data-lang="{language}"]').click()
                page.locator("#settings-toggle").click()
                check(page.locator("#tls-settings .settings-group-title").text_content().strip() == heading,
                      f"settings_translated_{language}")
            check(all(url.startswith("wss://") for url in websocket_urls), "websockets_follow_https_scheme", websocket_urls)
            context.close()
            browser.close()
            summary["status"] = "PASS"
    except Exception as error:
        summary["error"] = f"{type(error).__name__}: {error}"
    finally:
        ui_harness.stop_server(server)
    return ui_harness.finish(summary, args.output_dir, "tls_ui_report.json")


if __name__ == "__main__":
    raise SystemExit(main())
