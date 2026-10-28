#!/usr/bin/env python3
"""Black-box P2 regression runner for WingmanKVM listener validation and
video-reconfigure side effects. Secrets stay in memory; only sanitized
results are printed or reported. Exit code 0 means every check passed.
"""
import argparse, copy, json, os, shlex, sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_p1_api import Runner  # noqa: E402

NAMES = (
    "invalid_server_listener_rejected",
    "nonvideo_changes_do_not_probe",
    "real_video_change_reprobes",
)


class P2Runner(Runner):
    def __init__(self, work, binary):
        super().__init__(work, binary)
        self.ffmpeg = self.bin / "ffmpeg"
        self.probe_log = self.bin / "ffmpeg-probes.log"

    def prepare(self):
        self.state.mkdir(parents=True)
        (self.state / "images").mkdir()
        self.bin.mkdir()
        stub = self.bin / "sudo"
        stub.write_text(
            "#!/bin/sh\n# Test stub: discard stdin, never execute arguments.\n"
            "/bin/cat >/dev/null 2>&1\nexit 0\n"
        )
        stub.chmod(0o755)
        # A fake encoder probe records every invocation so tests can observe
        # runtime side effects instead of mirroring the comparison expression.
        self.ffmpeg.write_text(
            "#!/bin/sh\n"
            f"printf '%s\\n' \"$*\" >> {shlex.quote(str(self.probe_log))}\n"
            "printf 'V....D libx264\\n'\n"
        )
        self.ffmpeg.chmod(0o755)
        config = {
            "version": 1,
            "server": dict(self.expected),
            "video": {
                "auto_detect": False,
                "h264": {
                    "ffmpeg_path": str(self.ffmpeg),
                    "allow_software": True,
                    "encoder": "software",
                },
            },
        }
        (self.state / "config.json").write_text(json.dumps(config) + "\n")

    def probe_count(self):
        try:
            text = self.probe_log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return 0
        return sum(1 for line in text.splitlines() if line.strip())

    def snapshot(self):
        status, live = self.get_config()
        try:
            disk = (self.state / "config.json").read_text(encoding="utf-8")
        except OSError:
            disk = None
        return status, live, disk

    def check_invalid_server_listener(self):
        if not self.cookie:
            self.check("invalid_server_listener_rejected", "blocked", reason="no session")
            return
        status, baseline, baseline_disk = self.snapshot()
        if status != 200 or not isinstance(baseline, dict):
            self.check(
                "invalid_server_listener_rejected",
                "fail",
                reason="could not read baseline configuration",
                live_status=status,
            )
            return

        cases = [
            ("hostname", "localhost", baseline["server"]["port"]),
            ("bracketed_ipv6", "[::1]", baseline["server"]["port"]),
            ("leading_space", " 127.0.0.1", baseline["server"]["port"]),
            ("empty", "", baseline["server"]["port"]),
            ("port_zero", baseline["server"]["listen_address"], 0),
        ]
        failures = []
        for name, address, port in cases:
            payload = copy.deepcopy(baseline)
            payload["server"]["listen_address"] = address
            payload["server"]["port"] = port
            try:
                status, _, raw = self.req(
                    "PUT", "/api/config", body=payload, cookie=self.cookie
                )
                try:
                    body = json.loads(raw)
                except ValueError:
                    body = None
                message = body.get("error") if isinstance(body, dict) else None
            except (OSError, ValueError) as error:
                status, message = None, f"{type(error).__name__}"
            live_status, live, disk = self.snapshot()
            unchanged = live == baseline and disk == baseline_disk
            if status != 400 or not unchanged:
                failures.append(
                    {
                        "case": name,
                        "status": status if status is not None else "error",
                        "message": message,
                        "live_unchanged": live == baseline,
                        "disk_unchanged": disk == baseline_disk,
                    }
                )
        self.check(
            "invalid_server_listener_rejected",
            "pass" if not failures else "fail",
            cases=len(cases),
            failures=failures,
        )

    def check_nonvideo_changes(self):
        if not self.cookie:
            self.check("nonvideo_changes_do_not_probe", "blocked", reason="no session")
            return
        status, live, _ = self.snapshot()
        if status != 200 or not isinstance(live, dict):
            self.check(
                "nonvideo_changes_do_not_probe",
                "fail",
                reason="could not read baseline configuration",
                live_status=status,
            )
            return

        baseline_probes = self.probe_count()
        attempts = []

        def record(name, method, path, payload):
            try:
                status, _, _ = self.req(method, path, body=payload, cookie=self.cookie)
            except (OSError, ValueError) as error:
                status = f"{type(error).__name__}"
            attempts.append({"name": name, "status": status, "probes": self.probe_count()})

        record("full_put_unchanged", "PUT", "/api/config", copy.deepcopy(live))
        record("empty_patch", "POST", "/api/config", {})
        record(
            "media_patch",
            "POST",
            "/api/config",
            {"media": {"read_only_by_default": True}},
        )
        record(
            "power_patch",
            "POST",
            "/api/config",
            {"power": {"short_press_ms": 600}},
        )
        record(
            "server_patch",
            "POST",
            "/api/config",
            {"server": {"proxy_protocol": "disabled"}},
        )
        record(
            "hid_patch",
            "POST",
            "/api/config",
            {"hid": {"pointer_mode": "auto"}},
        )
        current_status, current, _ = self.snapshot()
        if current_status == 200 and isinstance(current, dict):
            record("full_put_after_changes", "PUT", "/api/config", copy.deepcopy(current))

        failures = [
            attempt
            for attempt in attempts
            if attempt["status"] != 200 or attempt["probes"] != baseline_probes
        ]
        self.check(
            "nonvideo_changes_do_not_probe",
            "pass" if attempts and not failures else "fail",
            baseline_probes=baseline_probes,
            attempts=attempts,
            failures=failures,
        )

    def check_real_video_change(self):
        if not self.cookie:
            self.check("real_video_change_reprobes", "blocked", reason="no session")
            return
        status, live, _ = self.snapshot()
        if status != 200 or not isinstance(live, dict):
            self.check(
                "real_video_change_reprobes",
                "fail",
                reason="could not read baseline configuration",
                live_status=status,
            )
            return

        current_quality = live.get("video", {}).get("jpeg_quality", 80)
        next_quality = 70 if current_quality != 70 else 71
        baseline_probes = self.probe_count()
        try:
            status, _, _ = self.req(
                "POST",
                "/api/config",
                body={"video": {"jpeg_quality": next_quality}},
                cookie=self.cookie,
            )
        except (OSError, ValueError) as error:
            status = f"{type(error).__name__}"
        after_probes = self.probe_count()
        live_status, applied, _ = self.snapshot()
        applied_quality = (
            applied.get("video", {}).get("jpeg_quality")
            if isinstance(applied, dict)
            else None
        )
        passed = (
            status == 200
            and after_probes == baseline_probes + 1
            and live_status == 200
            and applied_quality == next_quality
        )
        self.check(
            "real_video_change_reprobes",
            "pass" if passed else "fail",
            http_status=status,
            baseline_probes=baseline_probes,
            probes_after=after_probes,
            expected_probes=baseline_probes + 1,
            live_status=live_status,
            applied_quality=applied_quality,
            expected_quality=next_quality,
        )

    def run(self):
        self.prepare()
        self.start()
        if not self.ready():
            for name in NAMES:
                self.check(name, "blocked", reason="not healthy with setup token")
            return self.report()
        token = self.token()
        if token:
            try:
                self.setup(token)
            except Exception as error:  # noqa: BLE001 - reported below
                self.check(
                    "invalid_server_listener_rejected",
                    "blocked",
                    reason=self.redact(f"{type(error).__name__}: {error}"),
                )
        if self.setup_status != 200:
            for name in NAMES:
                if name not in self.checks:
                    self.check(name, "blocked", reason=f"setup status {self.setup_status}")
            return self.report()
        self.run_check("invalid_server_listener_rejected", self.check_invalid_server_listener)
        self.run_check("nonvideo_changes_do_not_probe", self.check_nonvideo_changes)
        self.run_check("real_video_change_reprobes", self.check_real_video_change)
        return self.report()

    def report(self):
        states = [self.checks.get(name, {}).get("status") for name in NAMES]
        summary = {
            "pass": states.count("pass"),
            "fail": states.count("fail"),
            "blocked": states.count("blocked"),
        }
        summary["ok"] = summary["pass"] == len(NAMES)
        return {"binary": str(self.binary), "checks": self.checks, "summary": summary}


def main(argv=None):
    parser = argparse.ArgumentParser(description="P2 black-box API regression runner")
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
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p2-", dir=scratch) as work:
        runner = P2Runner(Path(work), Path(binary))
        try:
            report = runner.run()
        except Exception as error:  # noqa: BLE001 - cleanup always runs
            for name in NAMES:
                if name not in runner.checks:
                    runner.check(
                        name,
                        "blocked",
                        reason=runner.redact(f"{type(error).__name__}: {error}"),
                    )
            report = runner.report()
        finally:
            runner.cleanup()
    summary = report["summary"]
    print(f"P2 API regression: {'PASS' if summary['ok'] else 'FAIL'}")
    for name in NAMES:
        print(f"  {name}: {report['checks'].get(name, {}).get('status', 'missing')}")
    print(
        f"  summary: {summary['pass']} pass, {summary['fail']} fail, "
        f"{summary['blocked']} blocked"
    )
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
