#!/usr/bin/env python3
"""Black-box session-cookie policy runner for WingmanKVM issue #17.

Starts an isolated binary, performs real setup/login/logout requests and only
reports cookie attribute booleans. Tokens and passwords stay in memory and are
never printed or written to the report.
Exit code 0 means every check passed.
"""
import argparse
import http.client
import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_p1_api import PASSWORD, USERNAME, Runner  # noqa: E402

NAMES = ("http_debug_cookie_policy", "https_public_origin_cookie_policy")
SETUP_TIMEOUT = 60.0
LOGIN_TIMEOUT = 30.0


def cookie_parts(value):
    """Return only non-secret cookie attributes."""
    if not value:
        return None
    fields = [field.strip() for field in value.split(";") if field.strip()]
    if not fields:
        return None
    name, separator, token = fields[0].partition("=")
    attributes = {}
    for field in fields[1:]:
        key, has_value, attribute = field.partition("=")
        attributes[key.lower()] = attribute if has_value else True
    return {
        "name": name,
        "has_value": bool(separator and token),
        "http_only": "httponly" in attributes,
        "same_site_strict": str(attributes.get("samesite", "")).lower() == "strict",
        "path_root": attributes.get("path") == "/",
        "secure": "secure" in attributes,
        "max_age": attributes.get("max-age"),
    }


def cookie_matches(parts, expected_secure, deletion):
    return bool(
        parts
        and parts["name"] == "wingman_session"
        and parts["has_value"] is not deletion
        and parts["http_only"]
        and parts["same_site_strict"]
        and parts["path_root"]
        and parts["secure"] is expected_secure
        and parts["max_age"] == ("0" if deletion else "43200")
    )


def forged_headers(expected_secure):
    """Forge headers in the direction that would flip the cookie policy."""
    if expected_secure:
        return {
            "Origin": "http://evil.example",
            "X-Forwarded-Proto": "http",
            "X-Forwarded-Ssl": "off",
            "Forwarded": "proto=http;host=evil.example",
        }
    return {
        "Origin": "https://evil.example",
        "X-Forwarded-Proto": "https",
        "X-Forwarded-Ssl": "on",
        "Forwarded": "proto=https;host=evil.example",
    }


def session_set_cookie(headers):
    for key, value in headers:
        if key.lower() == "set-cookie" and value.startswith("wingman_session="):
            return value
    return None


def session_token(cookie):
    if not cookie:
        return None
    _, separator, token = cookie.split(";", 1)[0].partition("=")
    return token.strip().strip('"') if separator else None


class CookieRunner(Runner):
    def __init__(self, work, binary, https_origin):
        super().__init__(work, binary)
        self.https_origin = https_origin
        self.expected = dict(self.expected)
        if https_origin:
            self.expected["public_origin"] = f"https://127.0.0.1:{self.port}"
        else:
            self.expected.pop("public_origin", None)

    def request(self, method, path, body=None, cookie=None, headers=None, timeout=SETUP_TIMEOUT):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        request_headers = {"Accept": "application/json"}
        data = None
        if body is not None:
            request_headers["Content-Type"] = "application/json"
            data = json.dumps(body).encode("utf-8")
        if cookie:
            request_headers["Cookie"] = f"wingman_session={cookie}"
        if headers:
            request_headers.update(headers)
        try:
            connection.request(method, path, body=data, headers=request_headers)
            response = connection.getresponse()
            return response.status, response.getheaders(), response.read()
        finally:
            connection.close()

    def remember(self, cookie):
        token = session_token(cookie)
        if token and token not in self.secrets:
            self.secrets.append(token)

    def setup_body(self, token):
        return {
            "username": USERNAME,
            "password": PASSWORD,
            "setup_token": token,
            "video_device": None,
            "keyboard_device": None,
            "mouse_device": None,
            "absolute_pointer_device": None,
            "pointer_mode": "absolute",
            "power_enabled": False,
            "media_enabled": False,
            "image_directory": str(self.state / "images"),
        }

    def run_policy_checks(self):
        expected_secure = self.https_origin
        forged = forged_headers(expected_secure)
        token = self.token()
        phases = {}

        if not token:
            return {"status": "blocked", "reason": "setup token unavailable", "phases": phases}

        setup_status = None
        setup_cookie = None
        try:
            setup_status, headers, _ = self.request(
                "POST", "/api/setup", body=self.setup_body(token), headers=forged
            )
            setup_cookie = session_set_cookie(headers)
            self.remember(setup_cookie)
        except (OSError, ValueError, http.client.HTTPException) as error:
            setup_error = self.redact(f"{type(error).__name__}: {error}")
        else:
            setup_error = None
        setup_parts = cookie_parts(setup_cookie)
        phases["setup"] = {
            "status": setup_status,
            "error": setup_error,
            "cookie": setup_parts,
            "pass": setup_status == 200
            and cookie_matches(setup_parts, expected_secure, deletion=False),
        }

        login_status = None
        login_cookie = None
        login_error = None
        try:
            login_status, headers, _ = self.request(
                "POST",
                "/api/login",
                body={"username": USERNAME, "password": PASSWORD},
                headers=forged,
                timeout=LOGIN_TIMEOUT,
            )
            login_cookie = session_set_cookie(headers)
            self.remember(login_cookie)
        except (OSError, ValueError, http.client.HTTPException) as error:
            login_error = self.redact(f"{type(error).__name__}: {error}")
        login_parts = cookie_parts(login_cookie)
        phases["login"] = {
            "status": login_status,
            "error": login_error,
            "cookie": login_parts,
            "pass": login_status == 200
            and cookie_matches(login_parts, expected_secure, deletion=False),
        }

        logout_cookie = login_cookie or setup_cookie
        logout_status = None
        logout_cookie_value = None
        logout_error = None
        if logout_cookie:
            # POST logout is origin-checked by the auth middleware, so keep a
            # real same-host Origin while still forging forwarding headers.
            logout_headers = dict(forged)
            logout_headers["Origin"] = self.origin
            try:
                logout_status, headers, _ = self.request(
                    "POST",
                    "/api/logout",
                    cookie=session_token(logout_cookie),
                    headers=logout_headers,
                )
                logout_cookie_value = session_set_cookie(headers)
                self.remember(logout_cookie_value)
            except (OSError, ValueError, http.client.HTTPException) as error:
                logout_error = self.redact(f"{type(error).__name__}: {error}")
        logout_parts = cookie_parts(logout_cookie_value)
        phases["logout"] = {
            "status": logout_status,
            "error": logout_error,
            "cookie": logout_parts,
            "pass": logout_status == 200
            and cookie_matches(logout_parts, expected_secure, deletion=True),
        }

        return {
            "status": "pass" if all(phase["pass"] for phase in phases.values()) else "fail",
            "expected_secure": expected_secure,
            "forged_headers": sorted(forged),
            "phases": phases,
        }

    def run(self):
        self.prepare()
        self.start()
        if not self.ready():
            return {"status": "blocked", "reason": "not healthy with setup token"}
        try:
            return self.run_policy_checks()
        except Exception as error:  # noqa: BLE001 - converted to sanitized result
            return {
                "status": "blocked",
                "reason": self.redact(f"{type(error).__name__}: {error}"),
            }


def main(argv=None):
    parser = argparse.ArgumentParser(description="P2c black-box session-cookie policy runner")
    parser.add_argument("--binary", required=True, help="absolute path to wingmankvm")
    parser.add_argument("--scratch-dir", required=True, help="directory for temporary data")
    parser.add_argument("--report", help="optional JSON report path")
    args = parser.parse_args(argv)
    binary, scratch = os.path.abspath(args.binary), os.path.abspath(args.scratch_dir)
    if not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        parser.error("--binary must be an executable file")
    if scratch == os.path.sep:
        parser.error("--scratch-dir must not be the filesystem root")
    os.makedirs(scratch, exist_ok=True)

    checks = {}
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p2c-", dir=scratch) as work:
        for name, https_origin in zip(NAMES, (False, True)):
            runner = CookieRunner(Path(work) / name, Path(binary), https_origin)
            try:
                checks[name] = runner.run()
            except Exception as error:  # noqa: BLE001 - cleanup always runs
                checks[name] = {
                    "status": "blocked",
                    "reason": runner.redact(f"{type(error).__name__}: {error}"),
                }
            finally:
                runner.cleanup()

    states = [checks.get(name, {}).get("status") for name in NAMES]
    summary = {
        "pass": states.count("pass"),
        "fail": states.count("fail"),
        "blocked": states.count("blocked"),
    }
    summary["ok"] = summary["pass"] == len(NAMES)
    report = {"binary": str(binary), "checks": checks, "summary": summary}
    print(f"P2c session-cookie policy: {'PASS' if summary['ok'] else 'FAIL'}")
    for name in NAMES:
        print(f"  {name}: {checks.get(name, {}).get('status', 'missing')}")
    print(f"  summary: {summary['pass']} pass, {summary['fail']} fail, {summary['blocked']} blocked")
    if args.report:
        try:
            with open(args.report, "w", encoding="utf-8") as handle:
                json.dump(report, handle, indent=2, sort_keys=True)
                handle.write("\n")
            print(f"  report: {args.report}")
        except OSError as error:
            print(f"  report error: {type(error).__name__}", file=sys.stderr)
            return 1
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
