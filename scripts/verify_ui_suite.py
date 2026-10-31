#!/usr/bin/env python3
"""Run every Playwright UI regression suite and aggregate their checks.

Each suite runs in its own interpreter (isolated browser, server and globals)
with the shared fixtures from ui_harness.py. The combined JSON line keeps the
per-suite check lists so counts can be compared with earlier runs. Works on
macOS and Linux; CI runs the same suites through verify_regressions.py.

    python3 scripts/verify_ui_suite.py --browser-channel chrome [--only p2_ui]
"""
import argparse, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
SUITES = (
    ("p1_ui", "verify_p1_ui.py"),
    ("p2_ui", "verify_p2_ui.py"),
    ("p2b_ui", "verify_p2b_ui.py"),
    ("audit2_ui", "verify_audit2_ui.py"),
)


def last_json(text):
    for line in reversed(text.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                continue
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=REPO)
    parser.add_argument("--output-dir", default="")
    parser.add_argument("--browser-channel", default="chrome")
    parser.add_argument("--only", action="append", choices=[name for name, _ in SUITES], default=[])
    parser.add_argument("--timeout", type=float, default=600.0, help="per-suite timeout in seconds")
    args = parser.parse_args()

    summary = {"status": "PASS", "repo": os.path.abspath(args.repo), "suites": {}, "checks": []}
    for name, script in SUITES:
        if args.only and name not in args.only:
            continue
        command = [sys.executable, os.path.join(HERE, script), "--repo", args.repo,
                   "--browser-channel", args.browser_channel]
        if args.output_dir:
            command += ["--output-dir", os.path.join(args.output_dir, name)]
        started = time.monotonic()
        try:
            proc = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout)
            report, code = last_json(proc.stdout), proc.returncode
        except subprocess.TimeoutExpired:
            report, code = None, "timeout"
        report = report or {"status": "FAIL", "checks": [], "error": f"no report (exit {code})"}
        checks = report.get("checks", [])
        skipped = sum(1 for c in checks if c.get("skipped"))
        summary["suites"][name] = {
            "status": report.get("status", "FAIL"), "exit_code": code,
            "passed": len(checks) - skipped, "skipped": skipped,
            "elapsed_seconds": round(time.monotonic() - started, 1),
            **({"error": report["error"]} if report.get("error") else {}),
        }
        summary["checks"] += [dict(c, name=f"{name}.{c.get('name')}") for c in checks]
        if report.get("status") != "PASS" or code != 0:
            summary["status"] = "FAIL"
    print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
    return 0 if summary["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
