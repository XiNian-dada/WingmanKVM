#!/usr/bin/env python3
"""Black-box P2b regression runner for bounded H.264 encoder probing.

The runner owns fake FFmpeg fixtures (normal, hang and flood) and never uses a
real encoder or capture hardware. Only sanitized results are reported.
Exit code 0 means every check passed.
"""
import argparse, json, os, signal, sys, tempfile, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_p1_api import Runner  # noqa: E402

NAMES = (
    "startup_health_under_6s",
    "bad_probe_pid_terminated",
    "setup_and_video_post_bounded",
    "h264_unavailable_reported",
    "normal_encoder_identified",
    "flood_probe_bounded",
)
STARTUP_LIMIT = 6.0
SETUP_LIMIT = 8.0
POST_LIMIT = 6.5


class P2BProbeRunner(Runner):
    def __init__(self, work, binary):
        super().__init__(work, binary)
        self.ffmpeg = self.bin / "ffmpeg"
        self.pid_log = self.bin / "probe-pids.log"
        self.startup_elapsed = None
        self.startup_pids = []
        self.setup_status = None
        self.setup_elapsed = None
        self.post_status = None
        self.post_elapsed = None
        self.normal_status = None
        self.normal_elapsed = None
        self.flood_status = None
        self.flood_elapsed = None

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
        self.reset_pid_log()
        self.write_ffmpeg("hang")
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

    def reset_pid_log(self):
        self.pid_log.write_text("", encoding="utf-8")

    def write_ffmpeg(self, kind):
        if kind == "normal":
            body = "printf 'V....D libx264\\n'\n"
        elif kind == "hang":
            # Slow fixtures self-exit even if a regression forgets to kill them.
            body = f"echo $$ >> {self.pid_log}\nsleep 30\n"
        elif kind == "flood":
            body = (
                f"echo $$ >> {self.pid_log}\n"
                "i=0\n"
                "while [ $i -lt 2048 ]; do printf '%1024s' ''; i=$((i+1)); done\n"
            )
        else:
            raise ValueError("unknown fixture kind")
        self.ffmpeg.write_text("#!/bin/sh\n" + body)
        self.ffmpeg.chmod(0o755)

    def pid_count(self):
        try:
            return len(self.pid_log.read_text(encoding="utf-8").splitlines())
        except OSError:
            return 0

    def pids_since(self, count):
        try:
            lines = self.pid_log.read_text(encoding="utf-8").splitlines()
        except OSError:
            return []
        pids = []
        for line in lines[count:]:
            try:
                pid = int(line.strip())
            except ValueError:
                continue
            if pid > 1:
                pids.append(pid)
        return pids

    def fixture_pids(self):
        return set(self.pids_since(0))

    @staticmethod
    def pid_alive(pid):
        try:
            os.kill(pid, 0)
            return True
        except OSError as error:
            return getattr(error, "errno", None) == 1  # EPERM still means alive

    def wait_pid_gone(self, pid, timeout=3.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not self.pid_alive(pid):
                return True
            time.sleep(0.05)
        return not self.pid_alive(pid)

    def cleanup(self):
        super().cleanup()
        # The probe fixture owns a private process group. Kill it only while its
        # leader is still alive so a recycled pid is never signalled.
        for pid in self.fixture_pids():
            if not self.pid_alive(pid):
                continue
            try:
                if hasattr(os, "killpg"):
                    os.killpg(pid, signal.SIGKILL)
                else:
                    os.kill(pid, signal.SIGKILL)
            except OSError:
                try:
                    os.kill(pid, signal.SIGKILL)
                except OSError:
                    pass

    def bootstrap(self):
        try:
            status, _, raw = self.req("GET", "/api/bootstrap", cookie=self.cookie)
            return status, json.loads(raw)
        except (OSError, ValueError):
            return None, None

    def patch_video(self, payload):
        started = time.monotonic()
        try:
            status, _, _ = self.req(
                "POST", "/api/config", body=payload, cookie=self.cookie, timeout=20.0
            )
        except (OSError, ValueError):
            status = None
        return status, time.monotonic() - started

    def check_startup(self, ready, elapsed):
        self.startup_elapsed = elapsed
        passed = (
            ready
            and elapsed < STARTUP_LIMIT
            and bool(self.startup_pids)
        )
        self.check(
            "startup_health_under_6s",
            "pass" if passed else "fail",
            ready=ready,
            elapsed_seconds=round(elapsed, 3),
            limit_seconds=STARTUP_LIMIT,
            probe_attempts=len(self.startup_pids),
        )

    def check_startup_pids(self):
        # Re-check every fixture PID at the end so a regression that leaks a
        # hanging or flooding probe is caught, not merely cleaned up.
        pids = sorted(set(self.startup_pids) | self.fixture_pids())
        if not pids:
            self.check(
                "bad_probe_pid_terminated",
                "blocked",
                reason="probe fixture was never started",
            )
            return
        gone = {str(pid): self.wait_pid_gone(pid) for pid in pids}
        self.check(
            "bad_probe_pid_terminated",
            "pass" if all(gone.values()) else "fail",
            probe_attempts=len(pids),
            terminated_and_reaped=gone,
        )

    def check_bounded_setup_post(self):
        passed = (
            self.setup_status == 200
            and self.setup_elapsed is not None
            and self.setup_elapsed < SETUP_LIMIT
            and self.post_status == 200
            and self.post_elapsed is not None
            and self.post_elapsed < POST_LIMIT
        )
        self.check(
            "setup_and_video_post_bounded",
            "pass" if passed else "fail",
            setup_status=self.setup_status,
            setup_seconds=(
                round(self.setup_elapsed, 3) if self.setup_elapsed is not None else None
            ),
            setup_limit_seconds=SETUP_LIMIT,
            video_post_status=self.post_status,
            video_post_seconds=(
                round(self.post_elapsed, 3) if self.post_elapsed is not None else None
            ),
            video_post_limit_seconds=POST_LIMIT,
        )

    def check_unavailable(self, bootstrap):
        status, body = bootstrap
        webrtc = body.get("webrtc") if isinstance(body, dict) else None
        capabilities = body.get("capabilities") if isinstance(body, dict) else None
        available = webrtc.get("available") if isinstance(webrtc, dict) else None
        message = webrtc.get("message") if isinstance(webrtc, dict) else None
        h264_capability = (
            capabilities.get("video_webrtc_h264")
            if isinstance(capabilities, dict)
            else None
        )
        passed = (
            status == 200
            and available is False
            and isinstance(message, str)
            and bool(message)
            and h264_capability is False
        )
        self.check(
            "h264_unavailable_reported",
            "pass" if passed else "fail",
            bootstrap_status=status,
            available=available,
            message_present=isinstance(message, str) and bool(message),
            h264_capability=h264_capability,
        )

    def check_normal(self, bootstrap):
        status, body = bootstrap
        webrtc = body.get("webrtc") if isinstance(body, dict) else None
        capabilities = body.get("capabilities") if isinstance(body, dict) else None
        available = webrtc.get("available") if isinstance(webrtc, dict) else None
        encoder = webrtc.get("encoder") if isinstance(webrtc, dict) else None
        h264_capability = (
            capabilities.get("video_webrtc_h264")
            if isinstance(capabilities, dict)
            else None
        )
        passed = (
            status == 200
            and available is True
            and encoder == "libx264"
            and h264_capability is True
            and self.normal_status == 200
            and self.normal_elapsed is not None
            and self.normal_elapsed < POST_LIMIT
        )
        self.check(
            "normal_encoder_identified",
            "pass" if passed else "fail",
            bootstrap_status=status,
            available=available,
            encoder=encoder,
            h264_capability=h264_capability,
            reconfigure_status=self.normal_status,
            reconfigure_seconds=(
                round(self.normal_elapsed, 3)
                if self.normal_elapsed is not None
                else None
            ),
        )

    def check_flood(self, bootstrap):
        status, body = bootstrap
        webrtc = body.get("webrtc") if isinstance(body, dict) else None
        available = webrtc.get("available") if isinstance(webrtc, dict) else None
        passed = (
            self.flood_status == 200
            and self.flood_elapsed is not None
            and self.flood_elapsed < POST_LIMIT
            and status == 200
            and available is False
        )
        self.check(
            "flood_probe_bounded",
            "pass" if passed else "fail",
            reconfigure_status=self.flood_status,
            reconfigure_seconds=(
                round(self.flood_elapsed, 3)
                if self.flood_elapsed is not None
                else None
            ),
            limit_seconds=POST_LIMIT,
            available=available,
        )

    def run(self):
        self.prepare()
        count = self.pid_count()
        started = time.monotonic()
        self.start()
        ready = self.ready()
        self.startup_elapsed = time.monotonic() - started
        self.startup_pids = self.pids_since(count)
        self.run_check(
            "startup_health_under_6s", self.check_startup, ready, self.startup_elapsed
        )
        self.run_check("bad_probe_pid_terminated", self.check_startup_pids)

        token = self.token()
        if not ready or not token:
            for name in NAMES:
                if name not in self.checks:
                    self.check(name, "blocked", reason="not healthy with setup token")
            return self.report()

        setup_started = time.monotonic()
        try:
            self.setup(token)
        except Exception:  # noqa: BLE001 - reported by the bounded check
            self.setup_status = None
        self.setup_elapsed = time.monotonic() - setup_started

        self.post_status, self.post_elapsed = self.patch_video(
            {"video": {"jpeg_quality": 70}}
        )
        self.run_check("setup_and_video_post_bounded", self.check_bounded_setup_post)
        self.run_check(
            "h264_unavailable_reported", self.check_unavailable, self.bootstrap()
        )

        self.write_ffmpeg("normal")
        self.normal_status, self.normal_elapsed = self.patch_video(
            {"video": {"h264": {"bitrate_kbps": 4501}}}
        )
        self.run_check(
            "normal_encoder_identified", self.check_normal, self.bootstrap()
        )

        self.write_ffmpeg("flood")
        self.flood_status, self.flood_elapsed = self.patch_video(
            {"video": {"h264": {"bitrate_kbps": 4502}}}
        )
        self.run_check("flood_probe_bounded", self.check_flood, self.bootstrap())
        self.run_check("bad_probe_pid_terminated", self.check_startup_pids)
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
    parser = argparse.ArgumentParser(description="P2b bounded encoder probe regression runner")
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
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p2b-", dir=scratch) as work:
        runner = P2BProbeRunner(Path(work), Path(binary))
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
    print(f"P2b probe regression: {'PASS' if summary['ok'] else 'FAIL'}")
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
