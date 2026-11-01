#!/usr/bin/env python3
"""Exercise the installer mDNS helpers and wingmankvm-setup-url without root.

The test sources deploy/install.sh (definitions only) against a scratch
DEST_ROOT and runs deploy/wingmankvm-setup-url with fake id/systemctl/hostname
commands. It checks hostname validation, the avahi service file, the minimal
avahi-daemon.conf host-name edit (with backup, idempotence and untouched
sections), and that the .local URL is printed first only when avahi runs.

Run:
  python3 scripts/verify_mdns.py
  python3 scripts/verify_mdns.py --repo . --report /tmp/mdns-report.json
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

TOKEN = "abcdefghijklmnop0123"


def find_bash() -> str:
    # macOS ships bash 3.2; prefer a modern bash when one is installed.
    for candidate in ("/opt/homebrew/bin/bash", "/usr/local/bin/bash", "/bin/bash"):
        if os.access(candidate, os.X_OK):
            return candidate
    return shutil.which("bash") or "bash"


def write_exe(path: Path, body: str) -> None:
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)


class Harness:
    def __init__(self, repo: Path, scratch: Path):
        self.repo = repo
        self.scratch = scratch
        self.bash = find_bash()
        self.checks: list[dict[str, str]] = []

    def record(self, name: str, ok: bool, detail: str) -> None:
        print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}")
        self.checks.append({"name": name, "status": "pass" if ok else "fail", "detail": detail})

    def fake_bin(self, name: str, avahi_active: bool, hostname: str = "rockpi") -> Path:
        directory = self.scratch / f"bin-{name}"
        directory.mkdir()
        write_exe(directory / "id", 'if [ "$1" = -u ]; then echo 0; else exec /usr/bin/id "$@"; fi\n')
        status = 0 if avahi_active else 3
        write_exe(directory / "systemctl",
                  f'case "$*" in *is-active*avahi-daemon*) exit {status};; esac\nexit 0\n')
        write_exe(directory / "hostname",
                  f'case "$1" in -I) echo 192.0.2.10;; -f) echo {hostname}.lan;; *) echo {hostname};; esac\n')
        write_exe(directory / "ip", "exit 0\n")
        return directory

    def run_install_fn(self, name: str, body: str, path_prefix: Path | None = None
                       ) -> subprocess.CompletedProcess[str]:
        root = self.scratch / f"root-{name}"
        root.mkdir()
        script = self.scratch / f"{name}.sh"
        script.write_text(
            "set -Eeuo pipefail\n"
            f'source "{self.repo}/deploy/install.sh"\n'
            f'DEST_ROOT="{root}"\n' + body
        )
        env = dict(os.environ)
        if path_prefix is not None:
            env["PATH"] = f"{path_prefix}:{env['PATH']}"
        return subprocess.run([self.bash, str(script)], cwd=str(self.scratch), env=env,
                              text=True, capture_output=True, timeout=60)

    def check_hostname_validation(self) -> None:
        body = (
            'for n in wingmankvm kvm-1 a 0abc; do valid_mdns_hostname "$n" || echo "BAD-REJECT $n"; done\n'
            'for n in "" -kvm kvm- Kvm kvm.local kvm_1 "$(printf "a%.0s" $(seq 64))"; do\n'
            '  valid_mdns_hostname "$n" && echo "BAD-ACCEPT $n"; done\n'
            'usage | grep -q -- "--mdns-hostname NAME" || echo "BAD-USAGE"\n'
            'echo done\n'
        )
        r = self.run_install_fn("validate", body)
        ok = r.returncode == 0 and "BAD" not in r.stdout and "done" in r.stdout
        self.record("hostname_validation_and_usage", ok, (r.stdout + r.stderr).strip()[-300:] or "ok")

    def check_service_file(self) -> None:
        r = self.run_install_fn("service", 'write_mdns_service_file 8443\n')
        path = self.scratch / "root-service/etc/avahi/services/wingmankvm.service"
        text = path.read_text() if path.exists() else ""
        ok = (r.returncode == 0 and "<type>_http._tcp</type>" in text
              and "<port>8443</port>" in text and oct(path.stat().st_mode & 0o777) == "0o644")
        self.record("avahi_service_file", ok, r.stderr.strip() or "written")

    def check_conf_edit(self) -> None:
        original = (
            "[server]\n#host-name=foo\nuse-ipv4=yes\n\n"
            "[wide-area]\nhost-name=untouched\n"
        )
        body = (
            'conf=$(mdns_conf_path); mkdir -p "$(dirname "$conf")"\n'
            f"printf '%s' '{original}' > \"$conf\"\n"
            'set_mdns_conf_host_name "$conf" wingmankvm\n'
            'set_mdns_conf_host_name "$conf" wingmankvm\n'
            'echo "READ=$(mdns_conf_host_name "$conf")"\n'
            'printf "[publish]\\nx=1\\n" > "$conf.2"\n'
            'set_mdns_conf_host_name "$conf.2" kvm2\n'
            'echo "READ2=$(mdns_conf_host_name "$conf.2")"\n'
        )
        r = self.run_install_fn("conf", body)
        conf = self.scratch / "root-conf/etc/avahi/avahi-daemon.conf"
        text = conf.read_text() if conf.exists() else ""
        backup = conf.with_name("avahi-daemon.conf.wingmankvm.bak")
        problems = []
        if r.returncode != 0:
            problems.append(r.stderr.strip())
        if text != "[server]\nhost-name=wingmankvm\nuse-ipv4=yes\n\n[wide-area]\nhost-name=untouched\n":
            problems.append(f"unexpected conf: {text!r}")
        if not backup.exists() or backup.read_text() != original:
            problems.append("backup missing or not the original")
        if "READ=wingmankvm" not in r.stdout or "READ2=kvm2" not in r.stdout:
            problems.append(f"read-back failed: {r.stdout!r}")
        self.record("avahi_conf_host_name_edit", not problems, "; ".join(problems) or "minimal edit")

    def check_configure_mdns(self) -> None:
        fake = self.fake_bin("configure", avahi_active=True)
        body = (
            'conf=$(mdns_conf_path); mkdir -p "$(dirname "$conf")"\n'
            'printf "[server]\\nuse-ipv4=yes\\n" > "$conf"\n'
            'MDNS_ENABLED=true; MDNS_HOSTNAME=wingmankvm; NO_START=true\n'
            'configure_mdns 8080\n'
            'echo "HOST=$(mdns_access_host)"\n'
        )
        r = self.run_install_fn("configure", body, fake)
        conf = self.scratch / "root-configure/etc/avahi/avahi-daemon.conf"
        ok = (r.returncode == 0 and "HOST=wingmankvm.local" in r.stdout
              and "host-name=wingmankvm" in conf.read_text())
        self.record("configure_mdns_sets_name", ok, (r.stdout + r.stderr).strip()[-300:])

    def run_setup_url(self, name: str, avahi_active: bool) -> subprocess.CompletedProcess[str]:
        fake = self.fake_bin(name, avahi_active)
        state = self.scratch / f"state-{name}"
        state.mkdir()
        (state / "setup-token").write_text(TOKEN + "\n")
        conf = self.scratch / f"avahi-{name}.conf"
        conf.write_text("[server]\nhost-name=wingmankvm\n")
        env = dict(os.environ, PATH=f"{fake}:{os.environ['PATH']}",
                   WINGMANKVM_STATE_DIR=str(state), WINGMANKVM_AVAHI_CONF=str(conf))
        return subprocess.run([self.bash, str(self.repo / "deploy/wingmankvm-setup-url")],
                              env=env, text=True, capture_output=True, timeout=60)

    def bash_supports_empty_arrays(self) -> bool:
        # bash < 4.4 treats "${empty[@]}" as unbound under set -u; the helper
        # targets the Linux bash 5 that ships on the KVM hosts.
        r = subprocess.run([self.bash, "-c", "set -u; a=(); : \"${a[@]}\""],
                           capture_output=True, text=True)
        return r.returncode == 0

    def check_setup_url(self) -> None:
        if not self.bash_supports_empty_arrays():
            # Only stock macOS is excused: the KVM hosts and CI are Linux, where
            # a skip here would silently drop these two checks.
            status = "fail" if sys.platform.startswith("linux") else "skip"
            for name in ("setup_url_prefers_mdns", "setup_url_without_avahi"):
                print(f"{status.upper()} {name}: {self.bash} is older than 4.4")
                self.checks.append({"name": name, "status": status, "detail": "bash < 4.4"})
            return
        r = self.run_setup_url("setup-on", avahi_active=True)
        urls = [line.strip() for line in r.stdout.splitlines() if line.strip().startswith("http://")]
        ok = (r.returncode == 0 and bool(urls)
              and urls[0] == f"http://wingmankvm.local:8080/#setup={TOKEN}"
              and any("192.0.2.10" in u for u in urls))
        self.record("setup_url_prefers_mdns", ok, " | ".join(urls) or r.stderr.strip())

        r = self.run_setup_url("setup-off", avahi_active=False)
        ok = r.returncode == 0 and ".local" not in r.stdout and "192.0.2.10" in r.stdout
        self.record("setup_url_without_avahi", ok, r.stdout.strip().replace("\n", " | ")[-300:])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(Path(__file__).resolve().parent.parent))
    parser.add_argument("--report")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="wingmankvm-mdns.") as tmp:
        harness = Harness(Path(args.repo).resolve(), Path(tmp))
        for check in (harness.check_hostname_validation, harness.check_service_file,
                      harness.check_conf_edit, harness.check_configure_mdns,
                      harness.check_setup_url):
            try:
                check()
            except Exception as exc:  # noqa: BLE001
                harness.record(check.__name__, False, repr(exc))
    status = "PASS" if all(c["status"] in ("pass", "skip") for c in harness.checks) else "FAIL"
    report = {"status": status, "checks": harness.checks}
    if args.report:
        Path(args.report).write_text(json.dumps(report, indent=2))
    print(json.dumps(report))
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
