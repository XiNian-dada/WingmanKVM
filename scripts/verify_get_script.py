#!/usr/bin/env python3
"""Exercise scripts/get.sh without touching GitHub or the host.

The test builds a fake release directory (aarch64 archive plus SHA256SUMS),
serves it from a loopback HTTP server and also uses a file:// base. It checks
latest-version discovery, explicit --version, sha256 verification, download-only
review mode, unsupported-architecture diagnostics and temporary cleanup.

Run:
  python3 scripts/verify_get_script.py
  python3 scripts/verify_get_script.py --repo . --report /tmp/get-report.json
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
from pathlib import Path
from typing import Any

TAG = "v9.9.9"
ARCHIVE = f"wingmankvm-{TAG}-aarch64-unknown-linux-gnu.tar.gz"
OTHER_ARCHIVE = f"wingmankvm-{TAG}-armv7-unknown-linux-gnueabihf.tar.gz"


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args: Any) -> None:
        return


class FixtureServer:
    def __init__(self, root: Path):
        self.root = root
        handler = lambda *args, **kwargs: QuietHandler(  # noqa: E731
            *args, directory=str(root), **kwargs
        )
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def api_url(self) -> str:
        return f"http://127.0.0.1:{self.port}/api/releases/latest"

    @property
    def download_base(self) -> str:
        return f"http://127.0.0.1:{self.port}/releases/download"

    def __enter__(self) -> "FixtureServer":
        self.thread.start()
        return self

    def __exit__(self, *_exc: Any) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_release(root: Path) -> Path:
    release = root / "releases" / "download" / TAG
    release.mkdir(parents=True)
    archive = release / ARCHIVE
    with tarfile.open(archive, "w:gz") as tar:
        payload = b"#!/usr/bin/env bash\necho fake installer \"$@\"\n"
        info = tarfile.TarInfo("deploy/install.sh")
        info.size = len(payload)
        info.mode = 0o755
        import io

        tar.addfile(info, io.BytesIO(payload))
    other = release / OTHER_ARCHIVE
    other.write_bytes(b"not the selected architecture\n")
    (release / "SHA256SUMS").write_text(
        f"{sha256(archive)}  {ARCHIVE}\n{sha256(other)}  {OTHER_ARCHIVE}\n",
        encoding="utf-8",
    )
    api = root / "api" / "releases" / "latest"
    api.parent.mkdir(parents=True)
    api.write_text(json.dumps({"tag_name": TAG}) + "\n", encoding="utf-8")
    return release


def fake_uname(directory: Path, machine: str) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    real = shutil.which("uname") or "/usr/bin/uname"
    script = directory / "uname"
    script.write_text(
        "#!/bin/sh\n"
        'if [ "${1:-}" = "-m" ]; then\n'
        f"  printf '%s\\n' '{machine}'\n"
        "  exit 0\n"
        "fi\n"
        f'exec "{real}" "$@"\n',
        encoding="utf-8",
    )
    script.chmod(0o755)
    return directory


class Harness:
    def __init__(self, repo: Path, scratch: Path, release: Path):
        self.repo = repo
        self.script = repo / "scripts" / "get.sh"
        self.scratch = scratch
        self.release = release
        self.checks: list[dict[str, Any]] = []
        self.tmp = scratch / "tmp"
        self.tmp.mkdir()

    def env(self, *, machine: str = "aarch64", api: str | None = None,
            base: str | None = None) -> dict[str, str]:
        bindir = fake_uname(self.scratch / "bin" / machine, machine)
        env = dict(os.environ)
        env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "/usr/bin:/bin")
        env["TMPDIR"] = str(self.tmp)
        if api:
            env["WINGMANKVM_API_URL"] = api
        else:
            env.pop("WINGMANKVM_API_URL", None)
        if base:
            env["WINGMANKVM_DOWNLOAD_BASE"] = base
        else:
            env.pop("WINGMANKVM_DOWNLOAD_BASE", None)
        return env

    def run(self, *args: str, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(self.script), *args],
            cwd=str(self.scratch), env=env, text=True,
            capture_output=True, timeout=60,
        )

    def record(self, name: str, ok: bool, detail: str) -> None:
        self.checks.append({"name": name, "status": "pass" if ok else "fail", "detail": detail})
        print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}", file=sys.stderr)

    def assert_clean_tmp(self) -> tuple[bool, str]:
        leftovers = sorted(self.tmp.glob("wingmankvm-get.*"))
        return not leftovers, ", ".join(str(path) for path in leftovers) or "clean"

    def check_http_latest(self, server: FixtureServer) -> None:
        out = self.scratch / "http-latest"
        result = self.run(
            "--download-only", str(out),
            env=self.env(api=server.api_url, base=server.download_base),
        )
        problems = []
        if result.returncode != 0:
            problems.append(f"exit {result.returncode}: {result.stderr.strip()}")
        if not (out / ARCHIVE).is_file():
            problems.append("archive not saved")
        if not (out / "SHA256SUMS").is_file():
            problems.append("SHA256SUMS not saved")
        if (out / ARCHIVE).is_file() and sha256(out / ARCHIVE) != sha256(self.release / ARCHIVE):
            problems.append("saved archive differs")
        if not list(out.glob("wingmankvm-review.*/deploy/install.sh")):
            problems.append("review extraction missing deploy/install.sh")
        clean, detail = self.assert_clean_tmp()
        if not clean:
            problems.append(f"temporary files left: {detail}")
        self.record("http_latest_download_and_checksum", not problems,
                    "; ".join(problems) or "verified")

    def check_file_version(self) -> None:
        out = self.scratch / "file-version"
        base = self.release.parent.as_uri()
        result = self.run(
            "--version", TAG, "--download-only", str(out),
            env=self.env(base=base),
        )
        problems = []
        if result.returncode != 0:
            problems.append(f"exit {result.returncode}: {result.stderr.strip()}")
        if not (out / ARCHIVE).is_file():
            problems.append("archive not saved")
        if not list(out.glob("wingmankvm-review.*/deploy/install.sh")):
            problems.append("review extraction missing deploy/install.sh")
        clean, detail = self.assert_clean_tmp()
        if not clean:
            problems.append(f"temporary files left: {detail}")
        self.record("file_url_explicit_version", not problems, "; ".join(problems) or "verified")

    def check_checksum_failure(self) -> None:
        bad_root = self.scratch / "bad-download"
        bad = bad_root / TAG
        shutil.copytree(self.release, bad)
        (bad / "SHA256SUMS").write_text(
            f"{'0' * 64}  {ARCHIVE}\n", encoding="utf-8"
        )
        out = self.scratch / "bad-output"
        result = self.run(
            "--version", TAG, "--download-only", str(out),
            env=self.env(base=bad_root.as_uri()),
        )
        problems = []
        if result.returncode == 0:
            problems.append("checksum mismatch accepted")
        elif "checksum mismatch" not in (result.stdout + result.stderr):
            problems.append("checksum diagnostic missing")
        if out.exists() and any(out.iterdir()):
            problems.append("partial output left in download directory")
        clean, detail = self.assert_clean_tmp()
        if not clean:
            problems.append(f"temporary files left: {detail}")
        self.record("checksum_mismatch_fails_and_cleans", not problems,
                    "; ".join(problems) or "rejected")

    def check_unsupported_arch(self) -> None:
        result = self.run(
            "--version", TAG, "--download-only", str(self.scratch / "x86"),
            env=self.env(machine="x86_64", base=self.release.parent.as_uri()),
        )
        problems = []
        if result.returncode == 0:
            problems.append("unsupported architecture accepted")
        if "unsupported architecture" not in (result.stdout + result.stderr):
            problems.append("architecture diagnostic missing")
        clean, detail = self.assert_clean_tmp()
        if not clean:
            problems.append(f"temporary files left: {detail}")
        self.record("unsupported_architecture_rejected", not problems,
                    "; ".join(problems) or "rejected")

    def check_workflow_archive_name(self) -> None:
        """The workflow must publish the file name the clients request."""
        workflow = (self.repo / ".github/workflows/release.yml").read_text(encoding="utf-8")
        clients = [(self.repo / path).read_text(encoding="utf-8")
                   for path in ("scripts/get.sh", "deploy/wingmankvm-online-upgrade")]
        ok = ('archive="wingmankvm-v${VERSION}-${TARGET}"' in workflow
              and all('"wingmankvm-${VERSION}-${TARGET}.tar.gz"' in text and "v" in text for text in clients))
        self.record("workflow_archive_name_matches_clients", ok, "release.yml vs get.sh and online upgrade")

    def check_online_upgrade_shares_logic(self) -> None:
        """deploy/wingmankvm-online-upgrade reuses get.sh's architecture
        detection, download and SHA256SUMS verification verbatim."""
        helper = self.repo / "deploy" / "wingmankvm-online-upgrade"
        problems = []

        def function(text: str, name: str) -> str:
            start = text.find(f"\n{name}() {{\n")
            if start < 0:
                return ""
            end = text.find("\n}\n", start)
            return text[start:end + 3]

        get_text = self.script.read_text(encoding="utf-8")
        helper_text = helper.read_text(encoding="utf-8")
        for name in ("detect_target", "download_file", "verify_archive"):
            body = function(get_text, name)
            if not body or body != function(helper_text, name):
                problems.append(f"{name} differs from scripts/get.sh")
        for forbidden in ("WINGMANKVM_API_URL:-", "WINGMANKVM_DOWNLOAD_BASE:-", "WINGMANKVM_REPO:-"):
            if forbidden in helper_text:
                problems.append(f"helper honours environment override {forbidden}")
        # Sourced functions: tag validation and maintenance-mode preservation.
        root = self.scratch / "online-root"
        (root / "etc" / "sudoers.d").mkdir(parents=True)
        probe = (
            f'source "{helper}"\nDEST_ROOT="{root}"\n'
            'for t in v1.2.3 v1.2.3-rc.1 1.2.3 "v1.2.3 x" "v1.2.3/../x"; do '
            'valid_tag "$t" && echo "ok:$t" || echo "no:$t"; done\n'
            'printf \'{"tag_name": "v4.5.6", "x": 1}\\n\' | parse_latest_tag\n'
            'echo "args:$(maintenance_args | tr \'\\n\' \' \')"\n'
            f': >"{root}/etc/sudoers.d/wingmankvm-maintenance-root"\n'
            'echo "args:$(maintenance_args | tr \'\\n\' \' \')"\n'
            f'echo "wingman ALL=(ALL:ALL) ALL" >"{root}/etc/sudoers.d/wingmankvm"\n'
            'maintenance_args >/dev/null && echo legacy:allowed || echo legacy:refused\n'
        )
        result = subprocess.run(["bash", "-c", probe], text=True, capture_output=True, timeout=30)
        expected = [
            "ok:v1.2.3", "no:v1.2.3-rc.1", "no:1.2.3", "no:v1.2.3 x", "no:v1.2.3/../x",
            "v4.5.6", "args:", "args:--maintenance-mode privileged ", "legacy:refused",
        ]
        if result.stdout.splitlines() != expected:
            problems.append(f"sourced helper output {result.stdout.splitlines()!r} {result.stderr.strip()}")
        bad = subprocess.run(["bash", str(helper), "--start", "--evil"], text=True,
                             capture_output=True, timeout=30)
        if bad.returncode == 0 or "takes no arguments" not in bad.stderr:
            problems.append("helper accepted extra arguments")
        self.record("online_upgrade_shares_get_logic", not problems,
                    "; ".join(problems) or "identical and strict")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    parser.add_argument("--report")
    args = parser.parse_args()
    repo = Path(args.repo).resolve()
    summary: dict[str, Any] = {"status": "FAIL", "checks": []}

    try:
        with tempfile.TemporaryDirectory(prefix="wingmankvm-get-test-") as raw:
            scratch = Path(raw)
            release = build_release(scratch / "fixture")
            harness = Harness(repo, scratch, release)
            with FixtureServer(scratch / "fixture") as server:
                harness.check_http_latest(server)
            harness.check_file_version()
            harness.check_checksum_failure()
            harness.check_workflow_archive_name()
            harness.check_unsupported_arch()
            harness.check_online_upgrade_shares_logic()
            summary["checks"] = harness.checks
            summary["status"] = "PASS" if all(
                item["status"] == "pass" for item in harness.checks
            ) else "FAIL"
    except Exception as exc:  # pragma: no cover - environment guard
        summary["error"] = f"{type(exc).__name__}: {exc}"

    if args.report:
        Path(args.report).write_text(
            json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
    return 0 if summary["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
