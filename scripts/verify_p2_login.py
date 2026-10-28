#!/usr/bin/env python3
"""Black-box P2 login/migration regression runner for WingmanKVM issue #07.

The runner reuses the P1 HTTP helper, keeps credentials in memory and uses a
test-only PATH stub for `sudo`. It never touches the real maintenance account,
real authentication files or hardware. Exit code 0 means every check passed.
"""
import argparse, json, os, shlex, sys, tempfile, time
from pathlib import Path

from verify_p1_api import PASSWORD, WRONG_PASSWORD, Runner

CHECKS = (
    "setup_helper_failure_is_degraded",
    "setup_sync_success_repeat_login_skips_helper",
    "legacy_helper_failure_degrades_terminal",
    "legacy_helper_hang_is_bounded_and_reaped",
    "failed_migration_retry_becomes_ready",
    "invalid_credentials_remain_401",
)
TERMINAL_WS_TIMEOUT = 8.0


class P2Runner(Runner):
    def __init__(self, work, binary):
        super().__init__(work, binary)
        self.mode_file = work / "helper-mode.txt"
        self.invocations_file = work / "sudo-invocations.log"
        self.setup_body = None

    def prepare(self):
        super().prepare()
        self.mode_file.write_text("ok\n", encoding="utf-8")
        invocations = shlex.quote(str(self.invocations_file))
        mode_file = shlex.quote(str(self.mode_file))
        stub = self.bin / "sudo"
        stub.write_text(
            "#!/bin/sh\n"
            "# Test-only stub: keep stdin opaque, record only the process id.\n"
            f"printf '%s\\n' \"$$\" >> {invocations}\n"
            "/bin/cat >/dev/null 2>&1\n"
            f"mode=$(/bin/cat {mode_file} 2>/dev/null || true)\n"
            'case "$mode" in\n'
            "  fail) exit 1 ;;\n"
            "  hang) exec /bin/sleep 12 ;;\n"
            "  *) exit 0 ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        stub.chmod(0o755)

    def set_mode(self, mode):
        self.mode_file.write_text(mode + "\n", encoding="utf-8")

    def invocations(self):
        try:
            return [int(line) for line in self.invocations_file.read_text().splitlines() if line.strip()]
        except (OSError, ValueError):
            return []

    def remove_sync_marker(self):
        path = self.state / "auth.json"
        record = json.loads(path.read_text(encoding="utf-8"))
        record.pop("system_password_synced", None)
        path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def marker(self):
        return bool(json.loads((self.state / "auth.json").read_text(encoding="utf-8")).get("system_password_synced"))

    def setup_p2(self, token):
        body = {
            "username": "p1admin",
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
        status, headers, raw = self.req("POST", "/api/setup", body=body, timeout=60.0)
        self.setup_status = status
        self.setup_body = self.json_body(raw)
        self.cookie = self.cookie_from_headers(headers)
        if self.cookie and self.cookie not in self.secrets:
            self.secrets.append(self.cookie)
        return self.setup_body

    def login(self, password):
        started = time.monotonic()
        try:
            status, headers, raw = self.req(
                "POST",
                "/api/login",
                body={"username": "p1admin", "password": password},
                timeout=20.0,
            )
        except Exception as error:  # noqa: BLE001 - reported sanitized
            return {"status": None, "error": type(error).__name__, "elapsed": time.monotonic() - started}
        elapsed = time.monotonic() - started
        cookie = self.cookie_from_headers(headers)
        if cookie:
            self.cookie = cookie
            if cookie not in self.secrets:
                self.secrets.append(cookie)
        return {"status": status, "cookie": cookie, "body": self.json_body(raw), "elapsed": elapsed}

    def bootstrap(self):
        status, _, raw = self.req("GET", "/api/bootstrap", cookie=self.cookie)
        return status, self.json_body(raw)

    def protected_config_status(self):
        return self.req("GET", "/api/config", cookie=self.cookie)[0]

    @staticmethod
    def json_body(raw):
        try:
            return json.loads(raw)
        except (TypeError, ValueError):
            return None

    @staticmethod
    def cookie_from_headers(headers):
        for key, value in headers:
            if key.lower() == "set-cookie" and value.startswith("wingman_session="):
                return value.split(";", 1)[0].split("=", 1)[1].strip().strip('"')
        return None

    def wait_for_process_exit(self, pid, timeout=4.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return True
            except OSError:
                return True
            time.sleep(0.05)
        return False


def scenario_setup_helper_failure(runner):
    assert runner.setup_status == 200, "setup must succeed despite helper failure"
    assert isinstance(runner.setup_body, dict) and runner.setup_body.get("terminal_ready") is False, "failed setup sync must report terminal_ready=false"
    assert runner.marker() is False, "failed setup sync must persist marker=false"
    assert runner.cookie, "setup must still issue a KVM session"
    status, body = runner.bootstrap()
    assert status == 200 and body.get("authenticated") is True, "setup session must authenticate"
    assert body.get("terminal_ready") is False and body.get("terminal_message"), "bootstrap must expose the degraded terminal"
    ws_status = runner.ws_status(runner.origin, None, runner.cookie)
    assert ws_status == 503, f"terminal upgrade must be unavailable: {ws_status}"
    config_status = runner.protected_config_status()
    assert config_status == 200, f"KVM APIs must remain available: {config_status}"
    return {"setup_status": runner.setup_status, "terminal_ready": False, "marker": False, "bootstrap_terminal_ready": False, "terminal_ws_status": ws_status, "protected_config_status": config_status}


def scenario_setup_sync_skips_helper(runner):
    assert runner.setup_status == 200, "setup did not succeed"
    assert isinstance(runner.setup_body, dict) and runner.setup_body.get("terminal_ready") is True, "setup must report terminal_ready=true"
    before = len(runner.invocations())
    seen = []
    for _ in range(3):
        result = runner.login(PASSWORD)
        assert result["status"] == 200, f"repeat login failed: {result['status']}"
        assert result["body"] and result["body"].get("terminal_ready") is True, "repeat login must be terminal-ready"
        seen.append({"status": result["status"], "terminal_ready": True})
    after = len(runner.invocations())
    assert after == before, "repeat logins must not invoke the password helper"
    status, body = runner.bootstrap()
    assert status == 200 and body.get("authenticated") is True and body.get("terminal_ready") is True, "authenticated bootstrap must report terminal_ready=true"
    return {"setup_terminal_ready": True, "repeat_logins": seen, "helper_invocations_before": before, "helper_invocations_after": after}


def scenario_legacy_failure_degrades(runner):
    runner.remove_sync_marker()
    runner.set_mode("fail")
    before = len(runner.invocations())
    result = runner.login(PASSWORD)
    after = len(runner.invocations())
    assert result["status"] == 200, f"degraded login must still succeed: {result['status']}"
    assert result["body"] and result["body"].get("terminal_ready") is False, "failed migration must report terminal_ready=false"
    assert result["cookie"], "degraded login must issue a KVM session"
    assert after > before, "failed migration must attempt the helper once"
    assert runner.marker() is False, "failed migration must not persist a success marker"
    status, body = runner.bootstrap()
    assert status == 200 and body.get("authenticated") is True, "degraded session must authenticate"
    assert body.get("terminal_ready") is False and body.get("terminal_message"), "bootstrap must expose degraded terminal state"
    ws_status = runner.ws_status(runner.origin, None, runner.cookie)
    assert ws_status == 503, f"terminal upgrade must be unavailable: {ws_status}"
    config_status = runner.protected_config_status()
    assert config_status == 200, f"KVM APIs must remain available: {config_status}"
    return {"login_status": result["status"], "terminal_ready": False, "bootstrap_terminal_ready": False, "terminal_ws_status": ws_status, "protected_config_status": config_status, "helper_invocations_added": after - before}


def scenario_legacy_hang_is_bounded(runner):
    runner.remove_sync_marker()
    runner.set_mode("hang")
    result = runner.login(PASSWORD)
    assert result["status"] == 200, f"hang login must still succeed: {result['status']}"
    assert result["body"] and result["body"].get("terminal_ready") is False, "timed-out migration must stay degraded"
    assert result["elapsed"] < TERMINAL_WS_TIMEOUT, f"login exceeded the bounded deadline: {result['elapsed']:.2f}s"
    pids = runner.invocations()
    assert pids, "hang scenario did not invoke the helper"
    assert runner.wait_for_process_exit(pids[-1]), "helper child was left running after timeout"
    return {"login_status": result["status"], "elapsed_seconds": round(result["elapsed"], 3), "helper_child_reaped": True, "terminal_ready": False}


def scenario_retry_becomes_ready(runner):
    runner.remove_sync_marker()
    runner.set_mode("fail")
    first = runner.login(PASSWORD)
    assert first["status"] == 200 and first["body"].get("terminal_ready") is False, "first failed migration must degrade"
    after_failure = len(runner.invocations())
    runner.set_mode("ok")
    second = runner.login(PASSWORD)
    after_success = len(runner.invocations())
    assert second["status"] == 200 and second["body"].get("terminal_ready") is True, "successful retry must become terminal-ready"
    assert after_success > after_failure, "successful retry must invoke the helper again"
    assert runner.marker() is True, "successful retry must persist the marker"
    third = runner.login(PASSWORD)
    after_ready = len(runner.invocations())
    assert third["status"] == 200 and third["body"].get("terminal_ready") is True, "later login must remain terminal-ready"
    assert after_ready == after_success, "later login must skip the helper once the marker is true"
    status, body = runner.bootstrap()
    assert status == 200 and body.get("terminal_ready") is True, "bootstrap must become ready after migration"
    return {"failed_login_terminal_ready": False, "retry_terminal_ready": True, "helper_calls_after_failure": after_failure, "helper_calls_after_success": after_success, "later_login_helper_calls": after_ready, "bootstrap_terminal_ready": True}


def scenario_invalid_credentials(runner):
    before = len(runner.invocations())
    result = runner.login(WRONG_PASSWORD)
    after = len(runner.invocations())
    assert result["status"] == 401, f"invalid credentials must remain 401: {result['status']}"
    assert after == before, "invalid credentials must not invoke the password helper"
    return {"login_status": result["status"], "helper_invocations_before": before, "helper_invocations_after": after}


SCENARIOS = (
    (CHECKS[0], scenario_setup_helper_failure, lambda runner: runner.set_mode("fail")),
    (CHECKS[1], scenario_setup_sync_skips_helper, None),
    (CHECKS[2], scenario_legacy_failure_degrades, None),
    (CHECKS[3], scenario_legacy_hang_is_bounded, None),
    (CHECKS[4], scenario_retry_becomes_ready, None),
    (CHECKS[5], scenario_invalid_credentials, None),
)


def run_scenario(name, func, binary, scratch, before_setup=None):
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p2-", dir=scratch) as work:
        runner = P2Runner(Path(work), Path(binary))
        try:
            runner.prepare()
            runner.start()
            if not runner.ready():
                runner.check(name, "blocked", reason="not healthy with setup token")
                return runner.checks[name]
            token = runner.token()
            if not token:
                runner.check(name, "blocked", reason="setup token unavailable")
                return runner.checks[name]
            if before_setup is not None:
                before_setup(runner)
            runner.setup_p2(token)
            details = func(runner)
            runner.check(name, "pass", **details)
        except Exception as error:  # noqa: BLE001 - report sanitized and continue
            runner.check(name, "fail", error=runner.redact(f"{type(error).__name__}: {error}"))
        finally:
            runner.cleanup()
        return runner.checks.get(name, {"status": "missing"})


def main(argv=None):
    parser = argparse.ArgumentParser(description="P2 login and terminal-migration regression runner")
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
    for name, func, before_setup in SCENARIOS:
        checks[name] = run_scenario(name, func, binary, scratch, before_setup)

    states = [checks.get(name, {}).get("status") for name in CHECKS]
    summary = {"pass": states.count("pass"), "fail": states.count("fail"), "blocked": states.count("blocked")}
    summary["ok"] = summary["pass"] == len(CHECKS)
    report = {"binary": binary, "checks": checks, "summary": summary}

    print(f"P2 login regression: {'PASS' if summary['ok'] else 'FAIL'}")
    for name in CHECKS:
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
