#!/usr/bin/env python3
"""P2C upgrade transaction verification harness.

Real: deploy/upgrade-transaction.sh, installer integration functions, file
bytes/modes/absence, EXIT/TERM traps, SIGKILL recovery, Python HTTP probing and
GNU-timeout wall-clock enforcement.
Mocked: systemd/udev runtime commands and two injected failure points. Four
first-install cases use real systemctl --root unit-file enable/disable operations.
No root, sudo, network, hardware, user creation or host installation is used.

Usage:
  python3 scripts/verify_p2c_upgrade.py --repo . \
      --scratch-dir /tmp/p2c --report /tmp/p2c-report.json
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

MANAGED = {
    "usr/local/bin/wingmankvm": 0o755,
    "usr/local/sbin/wingmankvm-gadget": 0o755,
    "usr/local/sbin/wingmankvm-set-wingman-password": 0o755,
    "usr/local/sbin/wingmankvm-setup-url": 0o755,
    "usr/local/sbin/wingmankvm-doctor": 0o755,
    "etc/sudoers.d/wingmankvm": 0o440,
    "etc/udev/rules.d/99-wingmankvm.rules": 0o644,
    "etc/systemd/system/wingmankvm-gadget.service": 0o644,
    "etc/systemd/system/wingmankvm.service": 0o644,
}
USER_DATA = {
    "var/lib/wingmankvm/config.json": b'{"server":{"listen_address":"127.0.0.1","port":8080}}\n',
    "var/lib/wingmankvm/auth.json": b'{"admin":"fixture"}\n',
    "var/lib/wingmankvm/images/user.img": b"user-image\n",
    "etc/wingmankvm/gadget.env": b"WINGMANKVM_GADGET_UDC=\n",
    "home/wingman/.bash_profile": b"# existing profile\n",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path: Path, data: bytes, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)


class HealthFixture:
    def __init__(self, responses: list[tuple[int, bytes]]):
        self.responses = responses
        self.calls = 0
        outer = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                index = min(outer.calls, len(outer.responses) - 1)
                status, body = outer.responses[index]
                outer.calls += 1
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args: Any) -> None:
                return

        self.server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}/healthz"

    def __enter__(self) -> "HealthFixture":
        self.thread.start()
        return self

    def __exit__(self, *_exc: Any) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


class Harness:
    def __init__(self, repo: Path, scratch: Path):
        self.repo = repo.resolve()
        self.scratch = scratch.resolve()
        self.cases: list[dict[str, Any]] = []
        self.managed_before = {}

    def record(self, name: str, ok: bool, detail: str, mocked: list[str]) -> None:
        self.cases.append({
            "case": name,
            "result": "pass" if ok else "fail",
            "detail": detail,
            "mocked": mocked,
            "real": "file bytes/modes/absence, transaction/trap/rollback/recovery, Python HTTP probe",
        })

    def base(self, name: str) -> Path:
        path = self.scratch / name
        if path.exists():
            shutil.rmtree(path)
        path.mkdir(parents=True)
        return path

    def root(self, base: Path) -> Path:
        root = base / "root"
        for rel in list(MANAGED) + list(USER_DATA):
            (root / Path(rel).parent).mkdir(parents=True, exist_ok=True)
        for rel, data in USER_DATA.items():
            write(root / rel, data)
        write(base / "fixture-binary", b"\x7fELFfixture\n", 0o755)
        return root

    def stubs(self, base: Path) -> None:
        bindir = base / "bin"
        bindir.mkdir()
        systemctl = bindir / "systemctl"
        systemctl.write_text(
            """#!/bin/sh
printf '%s\\n' "$*" >>"$SYSTEMCTL_LOG"
cmd=$1
case $cmd in
  is-active)
    shift; [ "${1:-}" = --quiet ] && shift
    [ -f "$STUB_STATE/active/$1" ]; exit $?
    ;;
  is-enabled)
    shift; [ "${1:-}" = --quiet ] && shift
    [ -f "$STUB_STATE/enabled/$1" ]; exit $?
    ;;
  enable)
    shift; mkdir -p "$STUB_STATE/enabled"; : >"$STUB_STATE/enabled/$1"; exit 0
    ;;
  disable)
    shift; rm -f "$STUB_STATE/enabled/$1"; exit 0
    ;;
  start)
    shift
    [ "${FAIL_START_GADGET:-0}" = 1 ] && exit 1
    mkdir -p "$STUB_STATE/active"; : >"$STUB_STATE/active/$1"; exit 0
    ;;
  stop)
    shift; rm -f "$STUB_STATE/active/$1"; exit 0
    ;;
  restart)
    shift
    n=0; [ -f "$STUB_STATE/restart.count" ] && n=$(cat "$STUB_STATE/restart.count")
    n=$((n + 1)); printf '%s' "$n" >"$STUB_STATE/restart.count"
    if [ "$1" = wingmankvm.service ] && [ "${FAIL_FIRST_RESTART:-0}" = 1 ] && [ "$n" -eq 1 ]; then exit 1; fi
    mkdir -p "$STUB_STATE/active"; : >"$STUB_STATE/active/$1"; exit 0
    ;;
  daemon-reload|--no-pager) exit 0 ;;
esac
exit 0
""",
            encoding="utf-8",
        )
        udevadm = bindir / "udevadm"
        udevadm.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        for path in (systemctl, udevadm):
            path.chmod(0o755)

    def driver(self, base: Path, body: str) -> Path:
        script = base / "driver.sh"
        script.write_text(
            "#!/usr/bin/env bash\nset -Eeuo pipefail\n"
            f'source "{self.repo}/deploy/install.sh"\n'
            f'DEST_ROOT="{base}/root"\n'
            f'export PATH="{base}/bin:$PATH"\n'
            f'export STUB_STATE="{base}/stub-state"\n'
            f'export SYSTEMCTL_LOG="{base}/systemctl.log"\n'
            f'BINARY="{base}/fixture-binary"\n'
            'WINGMAN_GROUP=$(id -gn)\n'
            'WINGMAN_CREATED=false\n' + body,
            encoding="utf-8",
        )
        script.chmod(0o700)
        return script

    def run(self, script: Path, timeout: int = 90) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env.pop("DEST_ROOT", None)
        return subprocess.run(
            ["bash", str(script)], cwd=str(self.scratch), env=env,
            text=True, capture_output=True, timeout=timeout,
        )

    def hashes(self, root: Path) -> dict[str, str]:
        saved = {}
        for rel in list(MANAGED) + ["etc/sudoers.d/wingmankvm-maintenance-root"]:
            path = root / rel
            saved[rel] = (digest(path), stat.S_IMODE(path.stat().st_mode), path.stat().st_uid, path.stat().st_gid) if path.is_file() else None
        self.managed_before[str(root)] = saved
        return {rel: digest(root / rel) for rel in USER_DATA}

    def tree(self, root: Path, before: dict[str, str], expect_new: bool) -> list[str]:
        problems: list[str] = []
        if expect_new:
            for rel, mode in MANAGED.items():
                path = root / rel
                if not path.is_file() or path.is_symlink():
                    problems.append(f"missing managed file: {rel}")
                elif stat.S_IMODE(path.stat().st_mode) != mode:
                    problems.append(f"wrong mode: {rel}")
        if not expect_new:
            for rel, expected in self.managed_before.get(str(root), {}).items():
                path = root / rel
                if expected is None:
                    if path.exists() or path.is_symlink(): problems.append(f"new managed file left after rollback: {rel}")
                elif not path.is_file() or (digest(path), stat.S_IMODE(path.stat().st_mode), path.stat().st_uid, path.stat().st_gid) != expected:
                    problems.append(f"managed snapshot not fully restored: {rel}")
        for rel, expected in before.items():
            path = root / rel
            if not path.is_file() or digest(path) != expected:
                problems.append(f"preserved/user data changed: {rel}")
        return problems

    def prelude(self) -> str:
        return (
            'TX_STORE=$(dest_path /var/lib/wingmankvm-deploy)\n'
            'STATE_DIR=$(dest_path /var/lib/wingmankvm)\n'
            'load_deployment_sources\n'
            'ensure_install_directories\n'
        )

    def case_success(self) -> None:
        base = self.base("success")
        root = self.root(base)
        self.stubs(base)
        before = self.hashes(root)
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
install_missing_login_profile
install_managed_files
tx_commit
"""))
        problems = self.tree(root, before, True)
        store = root / "var/lib/wingmankvm-deploy"
        txns = sorted(store.glob("txn.*")) if store.is_dir() else []
        if result.returncode != 0:
            problems.append("driver failed: " + result.stderr.strip())
        if not txns or not (txns[-1] / "COMMITTED").is_file():
            problems.append("committed snapshot missing")
        elif not os.access(txns[-1] / "recover.sh", os.X_OK):
            problems.append("recover.sh not executable")
        if (store / "PENDING").exists():
            problems.append("PENDING left after commit")
        self.record("success", not problems, "; ".join(problems) or "ok", [])

    def case_missing_helper(self) -> None:
        base = self.base("missing-helper")
        root = self.root(base)
        deploy = base / "deploy"
        shutil.copytree(self.repo / "deploy", deploy)
        (deploy / "wingmankvm-set-wingman-password").unlink()
        before = self.hashes(root)
        script = base / "driver.sh"
        script.write_text(
            "#!/usr/bin/env bash\nset -Eeuo pipefail\n"
            f'source "{deploy}/install.sh"\n'
            f'DEST_ROOT="{root}"\n'
            'WINGMAN_GROUP=$(id -gn)\n'
            'load_deployment_sources\n'
            'tx_init "$(dest_path /var/lib/wingmankvm-deploy)" "$DEST_ROOT"\n',
            encoding="utf-8",
        )
        script.chmod(0o700)
        result = self.run(script)
        problems: list[str] = []
        if result.returncode == 0:
            problems.append("missing helper accepted")
        if "missing or unreadable" not in result.stderr + result.stdout:
            problems.append("missing-helper diagnostic absent")
        if (root / "var/lib/wingmankvm-deploy").exists():
            problems.append("store created before preflight passed")
        for rel in MANAGED:
            if (root / rel).exists():
                problems.append(f"managed file created before preflight: {rel}")
        problems += self.tree(root, before, False)
        self.record("preflight-missing-helper-zero-mutation", not problems,
                    "; ".join(problems) or "ok", [])

    def case_partial(self) -> None:
        base = self.base("partial")
        root = self.root(base)
        self.stubs(base)
        write(root / "usr/local/bin/wingmankvm", b"old-binary\n", 0o755)
        write(root / "etc/udev/rules.d/99-wingmankvm.rules", b"old-rules\n", 0o644)
        before = self.hashes(root)
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
eval "original_$(declare -f _tx_publish_one)"
count=0
_tx_publish_one() { count=$((count + 1)); [ "$count" -eq 2 ] && return 1; original__tx_publish_one "$@"; }
install_managed_files
"""))
        problems: list[str] = []
        if result.returncode == 0:
            problems.append("injected publish failure succeeded")
        if (root / "usr/local/bin/wingmankvm").read_bytes() != b"old-binary\n":
            problems.append("old binary not restored")
        if (root / "etc/udev/rules.d/99-wingmankvm.rules").read_bytes() != b"old-rules\n":
            problems.append("old rules not restored")
        problems += self.tree(root, before, False)
        store = root / "var/lib/wingmankvm-deploy"
        if (store / "PENDING").exists():
            problems.append("PENDING left after rollback")
        if not list(store.glob("txn.*/ROLLED_BACK")):
            problems.append("ROLLED_BACK missing")
        self.record("failure-after-partial-replacement", not problems,
                    "; ".join(problems) or "ok", ["_tx_publish_one fails entry 2"])

    def case_term(self) -> None:
        base = self.base("term")
        root = self.root(base)
        self.stubs(base)
        write(root / "usr/local/bin/wingmankvm", b"old-binary\n", 0o755)
        before = self.hashes(root)
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
eval "original_$(declare -f _tx_publish_one)"
count=0
_tx_publish_one() {
  original__tx_publish_one "$@"
  count=$((count + 1))
  [ "$count" -eq 1 ] && { kill -TERM $$; sleep 5; }
}
install_managed_files
"""))
        problems: list[str] = []
        if result.returncode != 143:
            problems.append(f"expected exit 143, got {result.returncode}")
        if (root / "usr/local/bin/wingmankvm").read_bytes() != b"old-binary\n":
            problems.append("binary not restored after TERM")
        problems += self.tree(root, before, False)
        store = root / "var/lib/wingmankvm-deploy"
        if (store / "PENDING").exists():
            problems.append("PENDING left after trapped TERM")
        if not list(store.glob("txn.*/ROLLED_BACK")):
            problems.append("ROLLED_BACK missing after TERM")
        self.record("trapped-term", not problems, "; ".join(problems) or "ok",
                    ["_tx_publish_one sends a real SIGTERM after entry 1"])

    def case_python_isolation(self) -> None:
        base = self.base("python-isolation")
        root = self.root(base)
        self.stubs(base)
        before = self.hashes(root)
        cwd = base / "cwd"
        cwd.mkdir()
        marker = base / "imported.marker"
        payload = f"open({str(marker)!r}, 'a').write('executed\\n')\n".encode()
        for name in ("json.py", "urllib.py", "http.py", "sitecustomize.py"):
            write(cwd / name, payload, 0o644)
        with HealthFixture([(200, b'{"status":"ok","service":"wingmankvm"}')]) as fixture:
            result = self.run(self.driver(base, self.prelude() + f"""
cd "{cwd}"
HEALTH_URL="{fixture.url}"
resolve_health_url
probe_health "$HEALTH_URL"
"""))
        problems: list[str] = []
        if result.returncode != 0:
            problems.append("isolated health probe failed: " + result.stderr.strip())
        if marker.exists():
            problems.append("current-directory module was imported by root Python")
        problems += self.tree(root, before, False)
        self.record("python-isolated-health-probe", not problems,
                    "; ".join(problems) or "ok",
                    ["local HTTP fixture; malicious json/urllib/http/sitecustomize in CWD"])

    def case_commit_signal(self) -> None:
        for name, signo in (("TERM", 15), ("INT", 2)):
            base = self.base("commit-signal-" + name.lower())
            root = self.root(base)
            self.stubs(base)
            old = b"old-binary\n"
            write(root / "usr/local/bin/wingmankvm", old, 0o755)
            before = self.hashes(root)
            result = self.run(self.driver(base, self.prelude() + f"""
tx_init "$TX_STORE" "$DEST_ROOT"
install_managed_files
tx_commit
tx_rollback "manual after commit" && exit 90
kill -{name} $$
sleep 5
"""))
            problems: list[str] = []
            if result.returncode not in (128 + signo, -signo):
                problems.append(f"unexpected exit after post-commit {name}: {result.returncode}")
            if (root / "usr/local/bin/wingmankvm").read_bytes() != (base / "fixture-binary").read_bytes():
                problems.append("committed binary was rolled back by a signal")
            store = root / "var/lib/wingmankvm-deploy"
            txns = sorted(store.glob("txn.*")) if store.is_dir() else []
            if not txns or not (txns[-1] / "COMMITTED").is_file():
                problems.append("COMMITTED marker missing")
            if list(store.glob("txn.*/ROLLED_BACK")):
                problems.append("ROLLED_BACK written after commit")
            if (store / "PENDING").exists():
                problems.append("PENDING reappeared after commit")
            if "refusing to roll back a committed transaction" not in result.stderr + result.stdout:
                problems.append("post-commit rollback refusal diagnostic missing")
            problems += self.tree(root, before, True)
            self.record(f"post-commit-{name.lower()}-does-not-rollback", not problems,
                        "; ".join(problems) or "ok",
                        [f"real SIG{name} after commit; direct tx_rollback refusal"])

    def case_recover_refuses_committed(self) -> None:
        base = self.base("recover-committed")
        root = self.root(base)
        self.stubs(base)
        old = b"old-binary\n"
        write(root / "usr/local/bin/wingmankvm", old, 0o755)
        before = self.hashes(root)
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
install_managed_files
tx_commit
"""))
        problems: list[str] = []
        if result.returncode != 0:
            problems.append("commit driver failed: " + result.stderr.strip())
        store = root / "var/lib/wingmankvm-deploy"
        txns = sorted(store.glob("txn.*")) if store.is_dir() else []
        if not txns:
            problems.append("transaction directory missing")
        else:
            recover = txns[-1] / "recover.sh"
            new_bytes = (base / "fixture-binary").read_bytes()
            refused = subprocess.run(
                ["bash", str(recover)], text=True, capture_output=True, timeout=60,
            )
            if refused.returncode == 0:
                problems.append("recover.sh accepted a committed transaction")
            if "committed" not in refused.stderr:
                problems.append("committed-refusal diagnostic missing")
            if (root / "usr/local/bin/wingmankvm").read_bytes() != new_bytes:
                problems.append("refused recovery still changed the binary")
            if (txns[-1] / "ROLLED_BACK").exists():
                problems.append("refused recovery wrote ROLLED_BACK")
            forced = subprocess.run(
                ["bash", str(recover), "--force"], text=True, capture_output=True, timeout=60,
            )
            if forced.returncode != 0:
                problems.append("--force recovery failed: " + forced.stderr.strip())
            if (root / "usr/local/bin/wingmankvm").read_bytes() != old:
                problems.append("--force recovery did not restore the old binary")
            if not (txns[-1] / "ROLLED_BACK").is_file():
                problems.append("--force recovery did not write ROLLED_BACK")
        if (store / "PENDING").exists():
            problems.append("PENDING left after recovery")
        problems += self.tree(root, before, False)
        self.record("recover-refuses-committed-until-force", not problems,
                    "; ".join(problems) or "ok", ["real recover.sh with and without --force"])

    def case_sigkill(self) -> None:
        base = self.base("sigkill")
        root = self.root(base)
        self.stubs(base)
        write(root / "usr/local/bin/wingmankvm", b"old-binary\n", 0o755)
        before = self.hashes(root)
        marker = base / "published"
        first = self.run(self.driver(base, self.prelude() + f"""
tx_init "$TX_STORE" "$DEST_ROOT"
eval "original_$(declare -f _tx_publish_one)"
count=0
_tx_publish_one() {{
  original__tx_publish_one "$@"
  count=$((count + 1))
  [ "$count" -eq 1 ] && {{ : >"{marker}"; kill -KILL $$; }}
}}
install_managed_files
"""))
        problems: list[str] = []
        if not marker.exists():
            problems.append("SIGKILL point not reached")
        store = root / "var/lib/wingmankvm-deploy"
        txns = sorted(store.glob("txn.*")) if store.is_dir() else []
        if not txns:
            problems.append("transaction directory missing after SIGKILL")
        if not (store / "PENDING").is_file():
            problems.append("PENDING missing after SIGKILL")
        second = self.run(self.driver(base, self.prelude() + 'tx_init "$TX_STORE" "$DEST_ROOT"\n'))
        if second.returncode == 0:
            problems.append("pending transaction was not rejected")
        if "did not complete" not in second.stderr + second.stdout:
            problems.append("pending rejection diagnostic absent")
        if txns:
            recover = subprocess.run(
                ["bash", str(txns[-1] / "recover.sh")], text=True,
                capture_output=True, timeout=60,
            )
            if recover.returncode != 0:
                problems.append("manual recovery failed: " + recover.stderr.strip())
        if (root / "usr/local/bin/wingmankvm").read_bytes() != b"old-binary\n":
            problems.append("manual recovery did not restore binary")
        if (store / "PENDING").exists():
            problems.append("manual recovery left PENDING")
        problems += self.tree(root, before, False)
        self.record("sigkill-pending-manual-recovery", not problems,
                    "; ".join(problems) or "ok", ["_tx_publish_one sends a real SIGKILL"])

    def case_health(self) -> None:
        base = self.base("health")
        root = self.root(base)
        self.stubs(base)
        write(root / "usr/local/bin/wingmankvm", b"old-binary\n", 0o755)
        before = self.hashes(root)
        (base / "stub-state/enabled").mkdir(parents=True)
        (base / "stub-state/active").mkdir(parents=True)
        for unit in ("wingmankvm.service", "wingmankvm-gadget.service"):
            (base / "stub-state/enabled" / unit).touch()
            (base / "stub-state/active" / unit).touch()
        with HealthFixture([(200, b'{"status":"ok","service":"wingmankvm"}')]) as fixture:
            result = self.run(self.driver(base, self.prelude() + f"""
tx_init "$TX_STORE" "$DEST_ROOT"
install_managed_files
HEALTH_URL="{fixture.url}"
FAIL_FIRST_RESTART=1
runtime_prepare
TX_PRE_ROLLBACK_HOOK=wingmankvm_runtime_pre_rollback
TX_ROLLBACK_HOOK=wingmankvm_runtime_rollback
if ! start_services; then handle_start_failure; fi
"""))
            probes = fixture.calls
        problems: list[str] = []
        if result.returncode == 0:
            problems.append("new-start failure succeeded")
        if (root / "usr/local/bin/wingmankvm").read_bytes() != b"old-binary\n":
            problems.append("files not restored after start failure")
        problems += self.tree(root, before, False)
        if probes < 1:
            problems.append("rollback did not verify restored app health")
        calls = (base / "systemctl.log").read_text() if (base / "systemctl.log").exists() else ""
        if "restart wingmankvm.service" not in calls:
            problems.append("prior active app was not restarted")
        if (root / "var/lib/wingmankvm-deploy/PENDING").exists():
            problems.append("PENDING left after successful recovery")
        self.record("new-start-failure-and-rollback-health", not problems,
                    "; ".join(problems) or "ok",
                    ["systemctl stub; real local HTTP health probe"])

        base = self.base("health-fail")
        root = self.root(base)
        self.stubs(base)
        before = self.hashes(root)
        with HealthFixture([(200, b'{"status":"ok","service":"other"}')]) as fixture:
            result = self.run(self.driver(base, self.prelude() + f"""
tx_init "$TX_STORE" "$DEST_ROOT"
install_managed_files
HEALTH_URL="{fixture.url}"
runtime_prepare
TX_PRE_ROLLBACK_HOOK=wingmankvm_runtime_pre_rollback
TX_ROLLBACK_HOOK=wingmankvm_runtime_rollback
if ! start_services; then handle_start_failure; fi
"""))
        problems = []
        if result.returncode == 0:
            problems.append("mismatched health JSON succeeded")
        problems += self.tree(root, before, False)
        if (root / "var/lib/wingmankvm-deploy/PENDING").exists():
            problems.append("PENDING left after health rollback")
        self.record("health-json-mismatch-rollback", not problems,
                    "; ".join(problems) or "ok",
                    ["systemctl stub; real local HTTP fixture"])

        base = self.base("health-poll")
        root = self.root(base)
        self.stubs(base)
        with HealthFixture([
            (503, b'{"status":"starting"}'),
            (503, b'{"status":"starting"}'),
            (200, b'{"status":"ok","service":"wingmankvm"}'),
        ]) as fixture:
            started = time.monotonic()
            result = self.run(self.driver(base, f'HEALTH_URL="{fixture.url}"\nprobe_health "$HEALTH_URL"\n'), 40)
            elapsed = time.monotonic() - started
            calls = fixture.calls
        problems = []
        if result.returncode != 0:
            problems.append("delayed healthy server rejected: " + result.stderr.strip())
        if calls < 3:
            problems.append(f"expected polling, saw {calls} request(s)")
        if not 0.2 <= elapsed <= 15:
            problems.append(f"unexpected polling duration {elapsed:.2f}s")
        self.record("health-polling", not problems, "; ".join(problems) or f"ok ({calls} requests)",
                    ["local HTTP fixture; real probe and GNU timeout supervisor"])

        base = self.base("health-invalid")
        root = self.root(base)
        self.stubs(base)
        with HealthFixture([(200, b'{"status":"ok"')]) as fixture:
            result = self.run(self.driver(base, f'HEALTH_URL="{fixture.url}"\nprobe_health "$HEALTH_URL"\n'), 40)
            calls = fixture.calls
        problems = []
        if result.returncode == 0:
            problems.append("invalid JSON accepted")
        if calls != 1:
            problems.append(f"invalid JSON should not retry, saw {calls} requests")
        self.record("health-invalid-json", not problems, "; ".join(problems) or "ok",
                    ["local HTTP fixture; real probe"])

    def case_first_install_units(self) -> None:
        # Real systemctl file operations in an isolated --root tree; service
        # start/stop and daemon operations still use the explicit runtime stub.
        real_systemctl = shutil.which("systemctl")
        if real_systemctl is None:
            raise RuntimeError("systemctl is required for isolated unit-file regression")
        for failure in ("before-start", "enable", "start", "health"):
            base = self.base("first-units-" + failure)
            root = self.root(base)
            self.stubs(base)
            before = self.hashes(root)
            wrapper = f'''
systemctl() {{
    case $1 in
        enable|disable)
            printf '%s\\n' "$*" >>"$SYSTEMCTL_LOG"
            {real_systemctl} --root "$DEST_ROOT" "$@" || return $?
            if [[ $1 == enable && {failure} == enable ]]; then return 1; fi
            ;;
        *) command systemctl "$@" ;;
    esac
}}
'''
            with HealthFixture([(200, b'{"status":"ok","service":"other"}')]) as fixture:
                body = self.prelude() + wrapper + f'''
tx_init "$TX_STORE" "$DEST_ROOT"
runtime_prepare
TX_PRE_ROLLBACK_HOOK=wingmankvm_runtime_pre_rollback
TX_ROLLBACK_HOOK=wingmankvm_runtime_rollback
install_managed_files
HEALTH_URL="{fixture.url}"
export FAIL_FIRST_RESTART={'1' if failure == 'start' else '0'}
'''
                body += ('exit 7\n' if failure == 'before-start' else
                         'if ! start_services; then handle_start_failure; fi\n')
                result = self.run(self.driver(base, body))
            problems = self.tree(root, before, False)
            if result.returncode == 0:
                problems.append("injected first-install failure succeeded")
            store = root / "var/lib/wingmankvm-deploy"
            if (store / "PENDING").exists() or not list(store.glob("txn.*/ROLLED_BACK")):
                problems.append("first-install rollback did not complete")
            links = list((root / "etc/systemd/system").rglob("*.wants/wingmankvm*.service"))
            if links:
                problems.append("new enable links left after rollback")
            active = base / "stub-state/active"
            if list(active.glob("wingmankvm*.service")):
                problems.append("new app/Gadget left active after rollback")
            self.record("first-install-real-unit-files-" + failure, not problems,
                        "; ".join(problems) or "ok",
                        ["runtime systemd/udev stubs; real systemctl --root enable/disable"])

    def case_rollback_failure(self) -> None:
        base = self.base("rollback-fail")
        root = self.root(base)
        self.stubs(base)
        write(root / "usr/local/bin/wingmankvm", b"old-binary\n", 0o755)
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
eval "original_$(declare -f _tx_publish_one)"
count=0
_tx_publish_one() { count=$((count + 1)); [ "$count" -eq 2 ] && return 1; original__tx_publish_one "$@"; }
_tx_restore_one() { return 1; }
install_managed_files
"""))
        problems: list[str] = []
        if result.returncode == 0:
            problems.append("rollback failure was ignored")
        store = root / "var/lib/wingmankvm-deploy"
        if not (store / "PENDING").is_file():
            problems.append("PENDING not retained")
        txns = sorted(store.glob("txn.*"))
        if not txns or not (txns[-1] / "recover.sh").is_file():
            problems.append("recovery command missing")
        if "manual recovery command" not in result.stderr + result.stdout:
            problems.append("recovery instructions absent")
        self.record("rollback-failure-retains-recovery", not problems,
                    "; ".join(problems) or "ok", ["_tx_restore_one forced to fail"])

    def case_no_start(self) -> None:
        base = self.base("no-start")
        root = self.root(base)
        (root / "etc/wingmankvm/gadget.env").unlink()
        self.stubs(base)
        before = {"home/wingman/.bash_profile": digest(root / "home/wingman/.bash_profile")}
        result = self.run(self.driver(base, self.prelude() + """
tx_init "$TX_STORE" "$DEST_ROOT"
install_missing_login_profile
install_managed_files
finish_no_start
"""))
        problems: list[str] = []
        if result.returncode != 0:
            problems.append("driver failed: " + result.stderr.strip())
        if "not runtime verified" not in (result.stdout + result.stderr).lower():
            problems.append("--no-start notice missing")
        if (base / "systemctl.log").exists() and (base / "systemctl.log").read_text().strip():
            problems.append("--no-start invoked systemctl")
        if not (root / "etc/wingmankvm/gadget.env").is_file():
            problems.append("missing gadget.env was not initialized")
        problems += self.tree(root, before, True)
        if not list((root / "var/lib/wingmankvm-deploy").glob("txn.*/COMMITTED")):
            problems.append("--no-start did not commit files")
        self.record("no-start-file-only", not problems, "; ".join(problems) or "ok", [])

    def case_maintenance_transaction(self) -> None:
        optional = "etc/sudoers.d/wingmankvm-maintenance-root"
        legacy = b"wingman ALL=(ALL:ALL) ALL\n"
        for name, body, expected_exit, remove in [
            ("legacy-choice-required", 'load_deployment_sources\nresolve_maintenance_mode\n', 1, False),
            ("restrict-rollback", self.prelude() + 'MAINTENANCE_MODE=restricted\nresolve_maintenance_mode\ngetent() { return 1; }\npreflight_maintenance_mode\ntx_init "$TX_STORE" "$DEST_ROOT"\ninstall_managed_files\n[[ ! -e "$(dest_path /etc/sudoers.d/wingmankvm-maintenance-root)" ]]\nexit 7\n', 7, False),
            ("restrict-commit", self.prelude() + 'MAINTENANCE_MODE=restricted\nresolve_maintenance_mode\ngetent() { return 1; }\npreflight_maintenance_mode\ntx_init "$TX_STORE" "$DEST_ROOT"\ninstall_managed_files\ntx_commit\n', 0, True),
        ]:
            base = self.base(name); root = self.root(base); self.stubs(base)
            write(root / "etc/sudoers.d/wingmankvm", legacy, 0o440)
            write(root / optional, (self.repo / "deploy/wingmankvm-maintenance-root-sudoers").read_bytes(), 0o440)
            before = self.hashes(root)
            result = self.run(self.driver(base, body))
            problems = []
            if result.returncode != expected_exit: problems.append(f"exit {result.returncode}, expected {expected_exit}: {result.stderr}")
            if remove:
                if (root / optional).exists(): problems.append("optional root grant left enabled")
                if (root / "etc/sudoers.d/wingmankvm").read_bytes() != (self.repo / "deploy/wingmankvm-sudoers").read_bytes(): problems.append("restricted default not installed")
                problems += self.tree(root, before, True)
            else:
                problems += self.tree(root, before, False)
            if name == "legacy-choice-required":
                if "explicitly select" not in result.stderr: problems.append("legacy mode-choice diagnostic missing")
                if (root / "var/lib/wingmankvm-deploy").exists(): problems.append("transaction started without explicit legacy mode choice")
            self.record(name, not problems, "; ".join(problems) or "ok", ["host account lookup skipped in temporary filesystem preflight"])

    def run_all(self) -> bool:
        for name in (
            "case_success", "case_missing_helper", "case_partial", "case_term",
            "case_python_isolation", "case_commit_signal",
            "case_recover_refuses_committed", "case_sigkill", "case_health",
            "case_first_install_units", "case_rollback_failure", "case_no_start",
            "case_maintenance_transaction",
        ):
            try:
                getattr(self, name)()
            except Exception as exc:
                self.record(name, False, f"harness exception: {exc!r}", [])
        return all(item["result"] == "pass" for item in self.cases)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=".")
    parser.add_argument("--scratch-dir", default="")
    parser.add_argument("--report", default="")
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args()
    os.umask(0o022)  # Fixture directories must model root-owned non-writable parents.
    repo = Path(args.repo).resolve()
    for required in ("deploy/install.sh", "deploy/upgrade-transaction.sh"):
        if not (repo / required).is_file():
            print(f"missing repository file: {required}", file=sys.stderr)
            return 2
    if args.scratch_dir:
        parent = Path(args.scratch_dir).resolve()
        parent.mkdir(parents=True, exist_ok=True)
        scratch = Path(tempfile.mkdtemp(prefix="verify-p2c-", dir=str(parent)))
    else:
        scratch = Path(tempfile.mkdtemp(prefix="verify-p2c-"))
    harness = Harness(repo, scratch)
    try:
        ok = harness.run_all()
        report = {
            "harness": "verify_p2c_upgrade",
            "root_used": os.geteuid() == 0,
            "note": "Real transaction and isolated systemctl --root unit-file operations; runtime systemd/udev commands are explicit stubs.",
            "scratch": str(scratch),
            "cases": harness.cases,
            "all_passed": ok,
        }
        text = json.dumps(report, indent=2)
        if args.report:
            Path(args.report).write_text(text + "\n", encoding="utf-8")
        print(text)
        return 0 if ok else 1
    finally:
        if not args.keep:
            shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
