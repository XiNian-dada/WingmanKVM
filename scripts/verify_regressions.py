#!/usr/bin/env python3
"""Sequential regression orchestrator for WingmanKVM issue #18.

Runs existing black-box API/UI scripts as bounded children, keeps sanitized
stdout/stderr and JSON reports under one unique run directory, uses private
short per-case TMPDIRs, and aggregates
pass/fail/blocked/skipped. It never builds, installs, uses the network, or
changes host policy. Exit 0 means no failed/blocked case.
"""
import argparse
import datetime
import json
import os
import re
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

API_TIMEOUT = 180.0
UI_TIMEOUT = 180.0
KILL_GRACE = 5.0
ERROR_TAIL = 4000


@dataclass(frozen=True)
class CaseSpec:
    case_id: str
    script: str
    category: str  # api | ui | repo
    optional: bool = False
    timeout: float = API_TIMEOUT


CASES = (
    CaseSpec("p1_api", "scripts/verify_p1_api.py", "api"),
    CaseSpec("p2_api", "scripts/verify_p2_api.py", "api"),
    CaseSpec("p2_login", "scripts/verify_p2_login.py", "api"),
    CaseSpec("p2b_probe", "scripts/verify_p2b_probe.py", "api"),
    CaseSpec("p2b_upload", "scripts/verify_p2b_upload.py", "api"),
    CaseSpec("p1_ui", "scripts/verify_p1_ui.py", "ui", timeout=UI_TIMEOUT),
    CaseSpec("p2_ui", "scripts/verify_p2_ui.py", "ui", timeout=UI_TIMEOUT),
    CaseSpec("p2b_ui", "scripts/verify_p2b_ui.py", "ui", timeout=UI_TIMEOUT),
    CaseSpec("p2c_cookie", "scripts/verify_p2c_cookie.py", "api", optional=True),
    CaseSpec("p2c_upgrade", "scripts/verify_p2c_upgrade.py", "repo", optional=True),
)
OPTIONAL_IDS = tuple(c.case_id for c in CASES if c.optional)
COUNTERS = ("pass", "fail", "blocked", "skipped")
UPGRADE_RESULTS = {
    "pass": "pass", "passed": "pass",
    "fail": "fail", "failed": "fail",
    "blocked": "blocked",
    "skip": "skipped", "skipped": "skipped",
}

SECRET_PATTERNS = (
    (re.compile(r"(?i)\b(cookie|set-cookie|authorization)\s*:\s*[^\r\n]*"), r"\1: [redacted]"),
    (re.compile(r"(?i)\b(wingman_session|setup_token)\s*=\s*[^\s;\"']+"), r"\1=[redacted]"),
    (re.compile(r"(?i)((?:password|passwd|secret|access_token|refresh_token|setup_token|session|cookie)[A-Za-z0-9_-]*\s*[:=]\s*)(\"[^\"]*\"|'[^']*'|[^\s,;}\]]+)"), r"\1[redacted]"),
)
SECRET_KEY = re.compile(r"(?i)(?:password|passwd|secret)")
SESSION_KEY = re.compile(r"(?i)(?:token|session|cookie)")


def sanitize_text(text):
    for pattern, replacement in SECRET_PATTERNS:
        text = pattern.sub(replacement, text)
    return text


def sanitize_json(value):
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            if isinstance(key, str) and SECRET_KEY.search(key):
                out[key] = "[redacted]"
            elif isinstance(key, str) and SESSION_KEY.search(key) and isinstance(item, str):
                out[key] = "[redacted]"
            else:
                out[key] = sanitize_json(item)
        return out
    if isinstance(value, list):
        return [sanitize_json(item) for item in value]
    if isinstance(value, str):
        return sanitize_text(value)
    return value


def read_text(path):
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def write_json(path, value, secret=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with open(temporary, "w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True, ensure_ascii=False)
        handle.write("\n")
    os.replace(temporary, path)
    if secret:
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass


def sanitize_file(raw, destination):
    destination.write_text(sanitize_text(read_text(raw)), encoding="utf-8")
    try:
        os.chmod(destination, 0o600)
    except OSError:
        pass
    try:
        raw.unlink()
    except OSError:
        pass


def last_json_object(text):
    for line in reversed([part.strip() for part in text.splitlines() if part.strip()]):
        if line.startswith("{") and line.endswith("}"):
            try:
                value = json.loads(line)
            except ValueError:
                continue
            if isinstance(value, dict):
                return value
    return None


def load_json(path):
    try:
        value = json.loads(path.read_text(encoding="utf-8", errors="replace"))
    except (OSError, ValueError):
        return None
    return value if isinstance(value, dict) else None


def find_report(hint):
    if hint is None:
        return None
    if hint.is_file():
        return hint
    if hint.is_dir():
        candidates = sorted(p for p in hint.glob("*.json") if "report" in p.name.lower())
        return candidates[0] if candidates else None
    return None


def linux_children(pid):
    """Direct children from /proc only; no system-wide process scan."""
    try:
        text = Path(f"/proc/{pid}/task/{pid}/children").read_text(encoding="ascii", errors="ignore")
    except OSError:
        return []
    return [int(part) for part in text.split() if part.isdigit()]


def pidfd_available():
    return (
        os.name == "posix"
        and hasattr(os, "pidfd_open")
        and hasattr(signal, "pidfd_send_signal")
        and Path("/proc").is_dir()
    )


def wait_pidfds(fds, timeout):
    """Bounded wait for Linux pidfds to become readable (process exited)."""
    remaining = list(fds)
    if not remaining:
        return
    poller = select.poll()
    for fd in remaining:
        poller.register(fd, select.POLLIN)
    deadline = time.monotonic() + timeout
    while remaining:
        left = deadline - time.monotonic()
        if left <= 0:
            return
        try:
            events = poller.poll(max(1, int(left * 1000)))
        except OSError:
            return
        ready = {fd for fd, _ in events}
        remaining = [fd for fd in remaining if fd not in ready]


def terminate_tree(proc):
    """Terminate/reap our child and descendants proven to descend from it.

    Linux: enumerate descendants through /proc, open pidfds before signalling,
    and use only pidfd_send_signal after the direct child is reaped. Numeric
    PIDs are never signalled after reaping, so a reused PID cannot be hit.
    Other platforms: bounded direct-child fallback only; never scan and signal
    unrelated system/user processes.
    """
    if proc.poll() is not None:
        return

    if pidfd_available():
        owned, stack = set(), [proc.pid]
        while stack:
            pid = stack.pop()
            if pid in owned:
                continue
            owned.add(pid)
            stack.extend(linux_children(pid))

        fds = []
        for pid in sorted(owned):
            try:
                fds.append(os.pidfd_open(pid))
            except OSError:
                pass

        for fd in fds:
            try:
                signal.pidfd_send_signal(fd, signal.SIGTERM)
            except OSError:
                pass

        try:
            proc.wait(timeout=KILL_GRACE)
        except subprocess.TimeoutExpired:
            pass

        for fd in fds:
            try:
                signal.pidfd_send_signal(fd, signal.SIGKILL)
            except OSError:
                pass

        if proc.poll() is None:
            try:
                proc.wait(timeout=KILL_GRACE)
            except subprocess.TimeoutExpired:
                try:
                    proc.kill()
                except OSError:
                    pass
                try:
                    proc.wait(timeout=KILL_GRACE)
                except subprocess.TimeoutExpired:
                    pass

        wait_pidfds(fds, KILL_GRACE)
        for fd in fds:
            try:
                os.close(fd)
            except OSError:
                pass
        return

    try:
        proc.terminate()
    except OSError:
        pass
    try:
        proc.wait(timeout=KILL_GRACE)
    except subprocess.TimeoutExpired:
        try:
            proc.kill()
        except OSError:
            pass
        try:
            proc.wait(timeout=KILL_GRACE)
        except subprocess.TimeoutExpired:
            pass


def build_command(spec, python, repo, binary, case_dir, browser_channel):
    script = repo / spec.script
    if spec.category == "ui":
        output = case_dir / "output"
        command = [python, str(script), "--repo", str(repo), "--output-dir", str(output),
                   "--browser-channel", browser_channel]
        return command, output
    command = [python, str(script)]
    if spec.category == "api":
        command += ["--binary", str(binary)]
    elif spec.category == "repo":
        command += ["--repo", str(repo)]
    command += ["--scratch-dir", str(case_dir / "scratch"), "--report", str(case_dir / "report.json")]
    return command, case_dir / "report.json"


def base_result(spec, required_gates, command=None):
    return {
        "id": spec.case_id,
        "script": spec.script,
        "category": spec.category,
        "optional": spec.optional,
        "required": (not spec.optional) or (spec.case_id in required_gates),
        "command": command,
        "executed": False,
        "exit_code": None,
        "timed_out": False,
        "elapsed_seconds": 0.0,
        "status": "skipped",
        "reason": None,
        "counts": {key: 0 for key in COUNTERS},
        "artifacts": {"stdout": None, "stderr": None, "report": None, "output_dir": None},
        "error_output": None,
    }


def counts_from_upgrade_cases(cases):
    counts = {key: 0 for key in COUNTERS}
    if not isinstance(cases, list) or not cases:
        return counts, "empty-cases"
    for item in cases:
        if not isinstance(item, dict):
            return counts, "malformed-case"
        value = item.get("result")
        key = UPGRADE_RESULTS.get(value.lower()) if isinstance(value, str) else None
        if key is None:
            return counts, "unrecognized-case-result"
        counts[key] += 1
    return counts, None


def report_counts(report):
    """Return (counts, recognized, schema_error)."""
    counts = {key: 0 for key in COUNTERS}
    check_counts = {key: 0 for key in COUNTERS}
    if not isinstance(report, dict):
        return counts, False, None

    if isinstance(report.get("cases"), list):
        counts, error = counts_from_upgrade_cases(report["cases"])
        if error:
            return counts, True, error
        if "all_passed" in report and not isinstance(report["all_passed"], bool):
            return counts, True, "malformed-all-passed"
        return counts, True, None

    recognized = False
    checks = report.get("checks")
    if isinstance(checks, dict):
        for item in checks.values():
            if not isinstance(item, dict):
                continue
            status = item.get("status")
            if status in check_counts:
                check_counts[status] += 1
                recognized = True
            elif item.get("skipped") is True:
                check_counts["skipped"] += 1
                recognized = True
    elif isinstance(checks, list):
        for item in checks:
            if not isinstance(item, dict):
                continue
            if item.get("skipped") is True or ("ok" in item and item.get("ok") is None):
                check_counts["skipped"] += 1
                recognized = True
            elif item.get("ok") is True:
                check_counts["pass"] += 1
                recognized = True
            elif item.get("ok") is False:
                check_counts["fail"] += 1
                recognized = True
            elif item.get("status") in check_counts:
                check_counts[item["status"]] += 1
                recognized = True

    summary = report.get("summary")
    if isinstance(summary, dict) and any(key in summary for key in ("pass", "fail", "blocked")):
        for key in COUNTERS:
            value = summary.get(key, 0)
            counts[key] = value if isinstance(value, int) and value >= 0 else 0
        recognized = True
        for key in COUNTERS:
            counts[key] = max(counts[key], check_counts[key])
    elif recognized:
        counts = dict(check_counts)

    if isinstance(report.get("status"), str) and report["status"].strip():
        recognized = True
    if report.get("ok") is True:
        recognized = True
    return counts, recognized, None


def report_verdict(report, counts):
    summary = report.get("summary") if isinstance(report, dict) else None
    ok = summary.get("ok") if isinstance(summary, dict) else None
    status = str(report.get("status", "")).strip().upper() if isinstance(report, dict) else ""
    all_passed = report.get("all_passed") if isinstance(report, dict) else None
    if counts["fail"] > 0 or status == "FAIL":
        return "fail"
    if all_passed is False:
        return "fail"
    if counts["blocked"] > 0:
        return "blocked"
    if ok is False:
        return "fail"
    if ok is True or status == "PASS" or all_passed is True or counts["pass"] > 0 or counts["skipped"] > 0:
        return "pass"
    return "fail"


def run_case(spec, repo, binary, run_dir, browser_channel, required_gates, skip_ui):
    started = time.monotonic()
    case_dir = run_dir / "cases" / spec.case_id
    case_dir.mkdir(parents=True, exist_ok=True)
    command, report_hint = build_command(spec, sys.executable, repo, binary, case_dir, browser_channel)
    result = base_result(spec, required_gates, command)

    if spec.category == "ui" and skip_ui:
        result.update(status="skipped", reason="skip-ui",
                      counts={"pass": 0, "fail": 0, "blocked": 0, "skipped": 1})
        return result
    if not (repo / spec.script).is_file():
        required = result["required"]
        result.update(
            status="fail" if required else "skipped",
            reason="required-script-missing" if required else "optional-script-absent",
            counts={"pass": 0, "fail": 1 if required else 0, "blocked": 0,
                    "skipped": 0 if required else 1},
        )
        return result

    # Chromium creates Unix-domain sockets under TMPDIR. A nested CI scratch
    # path can exceed sockaddr_un's path limit and abort the browser at launch.
    # This directory is private, freshly created, and removed only by this case.
    tmp_dir = Path(tempfile.mkdtemp(prefix="wkvm-reg-", dir="/tmp"))
    result["temporary_directory"] = str(tmp_dir)
    stdout_raw, stderr_raw = case_dir / "stdout.raw", case_dir / "stderr.raw"
    stdout_log, stderr_log = case_dir / "stdout.log", case_dir / "stderr.log"
    env = os.environ.copy()
    env.update({"PYTHONDONTWRITEBYTECODE": "1", "PYTHONUNBUFFERED": "1",
                "TMPDIR": str(tmp_dir), "NO_COLOR": "1", "TERM": "dumb"})
    env.pop("PYTHONPATH", None)

    proc = None
    timed_out = False
    launch_error = None
    stdout_json = None
    try:
        with open(stdout_raw, "w", encoding="utf-8", errors="replace") as out, \
                open(stderr_raw, "w", encoding="utf-8", errors="replace") as err:
            try:
                proc = subprocess.Popen(
                    command, cwd=str(repo), env=env, stdin=subprocess.DEVNULL,
                    stdout=out, stderr=err, close_fds=True,
                    start_new_session=(os.name == "posix"),
                )
                result["executed"] = True
            except OSError as error:
                launch_error = f"{type(error).__name__}: {error}"
            if proc is not None:
                try:
                    proc.wait(timeout=spec.timeout)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    terminate_tree(proc)
    except KeyboardInterrupt:
        if proc is not None:
            terminate_tree(proc)
        raise
    finally:
        if proc is not None and proc.poll() is None:
            terminate_tree(proc)
        stdout_text = read_text(stdout_raw)
        stdout_json = last_json_object(stdout_text)
        sanitize_file(stdout_raw, stdout_log)
        sanitize_file(stderr_raw, stderr_log)
        result["elapsed_seconds"] = round(time.monotonic() - started, 3)
        result["exit_code"] = proc.returncode if proc is not None else None
        result["timed_out"] = timed_out
        result["artifacts"] = {
            "stdout": str(stdout_log),
            "stderr": str(stderr_log),
            "report": None,
            "output_dir": str(case_dir / "output") if spec.category == "ui" else None,
        }
        # Remove only our case scratch and the exact private TMPDIR created
        # above. Never clean their parent directories or shared /tmp entries.
        shutil.rmtree(case_dir / "scratch", ignore_errors=True)
        shutil.rmtree(tmp_dir, ignore_errors=True)

    report_obj = None
    report_path = find_report(report_hint)
    if report_path is not None:
        report_obj = load_json(report_path)
    if report_obj is None and stdout_json is not None:
        report_obj = stdout_json
        report_path = case_dir / "report.json"
    if report_obj is not None:
        report_obj = sanitize_json(report_obj)
        write_json(report_path, report_obj, secret=True)
        result["artifacts"]["report"] = str(report_path)

    if launch_error:
        status, reason = "fail", "launch-error"
        counts = {"pass": 0, "fail": 1, "blocked": 0, "skipped": 0}
        result["error_output"] = sanitize_text(launch_error)[-ERROR_TAIL:]
    else:
        counts, recognized, schema_error = report_counts(report_obj)
        if timed_out:
            status, reason = "fail", "timeout"
        elif report_obj is None:
            status, reason = "fail", "missing-report"
        elif schema_error:
            status, reason = "fail", "malformed-report"
        elif not recognized:
            status, reason = "fail", "unrecognized-report"
        else:
            verdict = report_verdict(report_obj, counts)
            if proc.returncode == 0:
                status = verdict
                reason = None if verdict == "pass" else f"child-report-{verdict}"
            elif verdict == "blocked":
                status, reason = "blocked", f"script-exit-{proc.returncode}"
            else:
                status, reason = "fail", f"script-exit-{proc.returncode}"
        if status == "fail" and counts["fail"] == 0:
            counts["fail"] = 1
        if status == "blocked" and counts["blocked"] == 0:
            counts["blocked"] = 1

    result.update(status=status, reason=reason, counts=counts)
    if status != "pass":
        chunks = []
        for label, path in (("stderr", stderr_log), ("stdout", stdout_log)):
            text = read_text(path).strip()
            if text:
                chunks.append(f"[{label}]\n{text}")
        if chunks:
            result["error_output"] = "\n".join(chunks)[-ERROR_TAIL:]
    return result


def skipped_result(spec, required_gates, reason, blocked=False):
    result = base_result(spec, required_gates)
    key = "blocked" if blocked else "skipped"
    result.update(status=key, reason=reason,
                  counts={"pass": 0, "fail": 0,
                          "blocked": 1 if blocked else 0,
                          "skipped": 0 if blocked else 1})
    return result


def summarize(results, interrupted):
    cases = {key: 0 for key in COUNTERS}
    checks = {key: 0 for key in COUNTERS}
    for result in results:
        cases[result["status"]] += 1
        for key in COUNTERS:
            checks[key] += result["counts"].get(key, 0)
    ok = cases["fail"] == 0 and cases["blocked"] == 0 and not interrupted
    if ok:
        status = "PASS"
    elif cases["fail"] == 0 and cases["blocked"] > 0:
        status = "BLOCKED"
    else:
        status = "FAIL"
    return {
        "status": status,
        "ok": ok,
        "interrupted": interrupted,
        "cases": cases,
        "checks": checks,
        "total_cases": len(results),
    }


def within(path, root):
    try:
        return os.path.commonpath([str(path), str(root)]) == str(root)
    except ValueError:
        return False


def main(argv=None):
    parser = argparse.ArgumentParser(description="WingmanKVM sequential regression orchestrator")
    parser.add_argument("--repo", default=".", help="repository root")
    parser.add_argument("--binary", required=True, help="existing executable wingmankvm")
    parser.add_argument("--scratch-dir", required=True, help="task-owned directory outside the repository")
    parser.add_argument("--report", help="JSON summary path outside the repository")
    parser.add_argument("--browser-channel", default="chrome", help="Playwright browser channel")
    parser.add_argument("--skip-ui", action="store_true", help="record UI gates as skipped (never used in CI)")
    parser.add_argument("--require-gate", action="append", choices=OPTIONAL_IDS, default=[],
                        help="make an optional integration gate mandatory; repeatable")
    args = parser.parse_args(argv)
    if sys.platform != "linux" or not pidfd_available():
        parser.error("the full regression gate requires Linux with pidfd support; use individual API/UI scripts on other platforms")
    try:
        descriptor = os.pidfd_open(os.getpid())
        os.close(descriptor)
    except OSError:
        parser.error("pidfd access is required to safely clean up isolated test processes")

    repo = Path(args.repo).resolve()
    binary = Path(args.binary).resolve()
    scratch = Path(args.scratch_dir).resolve()
    if not repo.is_dir():
        parser.error("--repo must be a directory")
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("--binary must be an existing executable file")
    if scratch == Path(os.sep) or within(scratch, repo):
        parser.error("--scratch-dir must be outside the repository and not the filesystem root")
    if args.report:
        report_path = Path(args.report).resolve()
        if report_path == Path(os.sep) or within(report_path, repo):
            parser.error("--report must be outside the repository and not the filesystem root")
    else:
        report_path = None
    os.makedirs(scratch, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="run-", dir=str(scratch)))
    if report_path is None:
        report_path = run_dir / "summary.json"

    required_gates = set(args.require_gate)
    results = []
    interrupted = False
    index = 0
    try:
        while index < len(CASES):
            results.append(run_case(CASES[index], repo, binary, run_dir, args.browser_channel,
                                    required_gates, args.skip_ui))
            index += 1
    except KeyboardInterrupt:
        interrupted = True
        if len(results) <= index:
            results.append(skipped_result(CASES[index], required_gates, "interrupted", blocked=True))
            index += 1
        for spec in CASES[index:]:
            results.append(skipped_result(spec, required_gates, "interrupted"))

    summary = summarize(results, interrupted)
    report = {
        "schema": 1,
        "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
        "repo": str(repo),
        "binary": str(binary),
        "scratch_dir": str(scratch),
        "run_dir": str(run_dir),
        "report_path": str(report_path),
        "browser_channel": args.browser_channel,
        "skip_ui": bool(args.skip_ui),
        "require_gate": sorted(required_gates),
        "sequential": True,
        "cleanup": {
            "scope": "unique_run_dir_and_exact_owned_case_tmpdirs",
            "removed": [
                "cases/<id>/scratch",
                "each case's temporary_directory (private /tmp/wkvm-reg-* directory)",
                "cases/<id>/stdout.raw",
                "cases/<id>/stderr.raw",
            ],
            "retained": [
                "cases/<id>/stdout.log",
                "cases/<id>/stderr.log",
                "cases/<id>/output",
                "case report JSON",
                "summary JSON",
            ],
        },
        "cases": results,
        "summary": summary,
    }
    try:
        write_json(report_path, sanitize_json(report), secret=True)
    except OSError as error:
        print(f"report error: {type(error).__name__}", file=sys.stderr)
        return 1

    print(f"Regression orchestration: {summary['status']}")
    for result in results:
        counts = result["counts"]
        timing = f", {result['elapsed_seconds']:.1f}s" if result["executed"] else ""
        suffix = f" [{result['reason']}]" if result["reason"] else ""
        print(f"  {result['id']}: {result['status']} "
              f"({counts['pass']} pass, {counts['fail']} fail, "
              f"{counts['blocked']} blocked, {counts['skipped']} skipped{timing}){suffix}")
    cases, checks = summary["cases"], summary["checks"]
    print(f"  cases: {cases['pass']} pass, {cases['fail']} fail, "
          f"{cases['blocked']} blocked, {cases['skipped']} skipped")
    print(f"  checks: {checks['pass']} pass, {checks['fail']} fail, "
          f"{checks['blocked']} blocked, {checks['skipped']} skipped")
    print(f"  report: {report_path}")
    if interrupted:
        return 130
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
