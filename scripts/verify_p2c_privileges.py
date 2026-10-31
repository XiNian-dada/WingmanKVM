#!/usr/bin/env python3
"""Disposable-container verifier for WingmanKVM restricted/privileged modes."""
import argparse, json, os, pwd, re, shutil, subprocess, tempfile
from pathlib import Path

PW = "WingmanVerify-Only-7c1e"
D = Path("/etc/sudoers.d")
MANAGED, PRIV = D / "wingmankvm", D / "wingmankvm-maintenance-root"
FOREIGN = D / "zz-wingmankvm-verify-foreign"
HELPER = Path("/usr/local/sbin/wingmankvm-set-wingman-password")
UPGRADE = Path("/usr/local/sbin/wingmankvm-online-upgrade")
UPGRADE_MARK = Path("/run/wingmankvm-verify-upgrade-ran")
UPGRADE_STUB = """#!/bin/sh
# synthetic verifier stub: records its arguments, never upgrades anything
printf '%s\\n' "$*" > /run/wingmankvm-verify-upgrade-ran
exit 0
"""
MARK = Path("/run/wingmankvm-verify-helper-ran")
ENV = {"PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
       "LC_ALL": "C", "LANG": "C", "TERM": "dumb"}
NOLOGIN = "/usr/sbin/nologin" if Path("/usr/sbin/nologin").exists() else "/bin/false"
STUB = """#!/bin/sh
set -eu
IFS= read -r password || true
[ -n "$password" ] || exit 2
# synthetic verifier stub: fixed account, no output
: > /run/wingmankvm-verify-helper-ran
exit 0
"""

class Blocked(Exception): pass
class Failure(Exception): pass
class Stop(Exception): pass

def red(text):
    text = str(text or "").replace(PW, "[redacted]")
    return re.sub(r"(?i)(password\s*[:=]\s*)\S+", r"\1[redacted]", text)[-4000:]

def run(argv, input_text=None, user=None, timeout=60):
    cmd = [str(x) for x in argv]
    env, pre = dict(ENV), None
    if user:
        pw = pwd.getpwnam(user)
        def pre():
            os.setgroups([]); os.setgid(pw.pw_gid); os.setuid(pw.pw_uid)
        env.update(HOME=pw.pw_dir, USER=user, LOGNAME=user)
    try:
        return subprocess.run(cmd, input=input_text, text=True, capture_output=True,
                              timeout=timeout, env=env, cwd="/", preexec_fn=pre)
    except FileNotFoundError as exc:
        raise Blocked(f"required command not found: {cmd[0]}") from exc
    except subprocess.TimeoutExpired as exc:
        raise Failure(f"command timed out: {cmd[0]}") from exc

def ok(argv, **kw):
    p = run(argv, **kw)
    if p.returncode != 0:
        raise Failure(f"{argv[0]} failed ({p.returncode}): {red(p.stderr or p.stdout)}")
    return p

def expect(cond, message):
    if not cond:
        raise Failure(message)

def tool(name):
    if shutil.which(name) is None:
        raise Blocked(f"required tool not found: {name}")

def markers():
    out = []
    for path in ("/.dockerenv", "/run/.containerenv"):
        if Path(path).exists():
            out.append(path)
    if os.environ.get("container"):
        out.append("env:container")
    try:
        text = Path("/proc/1/cgroup").read_text(errors="replace")
        for token in ("docker", "kubepods", "containerd", "lxc", "podman"):
            if token in text:
                out.append("cgroup:" + token)
    except OSError:
        pass
    return out

def rm(path):
    try: Path(path).unlink()
    except FileNotFoundError: pass

def snapshot(path, tempdir):
    path = Path(path)
    if not path.exists() and not path.is_symlink():
        return None
    if path.is_symlink() or not path.is_file():
        raise Failure(f"refusing to snapshot non-regular file: {path}")
    backup = Path(tempdir) / (path.name + ".bak")
    shutil.copy2(path, backup)
    return backup

def restore(path, backup):
    path = Path(path)
    if backup is None:
        rm(path); return
    shutil.copy2(backup, path)
    os.chmod(path, 0o440 if "sudoers" in str(path) else 0o755)
    os.chown(path, 0, 0)

def visudo(path):
    p = run(["visudo", "-cf", str(path)])
    if p.returncode != 0:
        raise Failure(f"visudo rejected {path}: {red(p.stderr or p.stdout)}")

def put(source, dest):
    source, dest = Path(source), Path(dest)
    if not source.is_file():
        raise Failure(f"missing source file: {source}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_name(dest.name + ".verify.tmp")
    try:
        tmp.write_bytes(source.read_bytes())
        os.chmod(tmp, 0o440); os.chown(tmp, 0, 0)
        visudo(tmp)
        os.replace(tmp, dest)
        os.chmod(dest, 0o440); os.chown(dest, 0, 0)
        visudo(dest)
    finally:
        if tmp.exists(): rm(tmp)

def listing():
    p = run(["sudo", "-n", "-l", "-U", "wingman"])
    text = p.stdout + p.stderr
    if p.returncode != 0 and "not allowed to run sudo" not in text \
            and "may not run sudo" not in text:
        raise Failure(f"cannot read sudo policy: {red(text)}")
    return text

def has_allowed(text):
    return "may run the following commands" in text

def check(report, name, func):
    try:
        detail = func()
    except Blocked:
        raise
    except Failure as exc:
        report["tests"].append({"name": name, "status": "FAIL", "detail": red(exc)})
        raise Stop()
    except Exception as exc:
        report["tests"].append({"name": name, "status": "FAIL",
                                "detail": red(f"{type(exc).__name__}: {exc}")})
        raise Stop()
    report["tests"].append({"name": name, "status": "PASS", "detail": red(detail or "")})

def write_report(path, report):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--report", required=True)
    ap.add_argument("--disposable-root", action="store_true")
    args = ap.parse_args()
    report = {"status": "BLOCKED", "tests": [], "repo": str(Path(args.repo).resolve())}
    if not args.disposable_root:
        report["reason"] = "--disposable-root sentinel is required"
        write_report(args.report, report); print("BLOCKED: sentinel required"); return 2
    found = markers()
    if os.geteuid() != 0 or not found or not Path("/wingmankvm-disposable-test").is_file():
        report["reason"] = "requires uid 0 inside a recognized disposable container"
        write_report(args.report, report); print("BLOCKED: not disposable root"); return 2
    report["container_markers"] = found
    repo = Path(args.repo).resolve()
    files = (repo / "deploy/install.sh", repo / "deploy/wingmankvm-sudoers",
             repo / "deploy/wingmankvm-maintenance-root-sudoers",
             repo / "deploy/wingmankvm-set-wingman-password")
    tempdir = tempfile.mkdtemp(prefix="wingmankvm-p2c-")
    backups, status = {}, "PASS"
    try:
        for name in ("sudo", "visudo", "useradd", "chpasswd", "bash"):
            tool(name)

        def source_syntax():
            for path in files:
                expect(path.is_file(), f"missing source file: {path}")
            ok(["bash", "-n", files[0]])
            ok(["sh", "-n", files[3]])
            visudo(files[1]); visudo(files[2])
            return "bash/sh/visudo source checks passed"
        check(report, "source_syntax", source_syntax)

        def snapshot_state():
            for path in (MANAGED, PRIV, FOREIGN, HELPER, UPGRADE):
                backups[path] = snapshot(path, tempdir)
            return "container sudoers/helper state snapshotted"
        check(report, "snapshot_container_state", snapshot_state)

        def users():
            for name, system in (("wingmankvm", True), ("wingman", False)):
                try:
                    pwd.getpwnam(name); continue
                except KeyError:
                    pass
                cmd = ["useradd", "--user-group"]
                cmd += (["--system", "--no-create-home", "--shell", NOLOGIN]
                        if system else ["--create-home", "--shell", "/bin/bash"])
                ok(cmd + [name])
            p = run(["chpasswd"], input_text=f"wingman:{PW}\n")
            if p.returncode != 0:
                raise Failure(f"chpasswd failed: {red(p.stderr or p.stdout)}")
            return "fixture users ready; synthetic wingman password set"
        check(report, "fixture_users", users)

        def stub():
            HELPER.parent.mkdir(parents=True, exist_ok=True)
            tmp = HELPER.with_name(HELPER.name + ".verify.tmp")
            tmp.write_text(STUB)
            os.chmod(tmp, 0o755); os.chown(tmp, 0, 0)
            os.replace(tmp, HELPER)
            rm(MARK)
            return "synthetic fixed-account helper stub installed"
        check(report, "synthetic_helper_stub", stub)

        def restricted_install():
            rm(PRIV); put(files[1], MANAGED); ok(["visudo", "-c"])
            return "restricted managed sudoers installed"
        check(report, "restricted_policy_installed", restricted_install)

        check(report, "restricted_listing_clean",
              lambda: "no allowed sudo command" if not has_allowed(listing())
              else (_ for _ in ()).throw(Failure("sudo -l reports allowed commands")))

        def identity():
            p = ok(["sudo", "-n", "-u", "wingman", "/bin/bash", "-l"],
                   input_text="id -un\n", user="wingmankvm")
            expect("wingman" in p.stdout.split(), f"unexpected identity: {red(p.stdout)}")
            return "service user launched wingman identity"
        check(report, "service_launches_wingman_identity", identity)

        def no_root_sh():
            p = run(["sudo", "-n", "/bin/sh", "-c", "id -u"], user="wingmankvm")
            expect(p.returncode != 0 and p.stdout.strip() != "0",
                   "service user unexpectedly obtained root /bin/sh")
            return "service user denied root /bin/sh"
        check(report, "service_cannot_root_shell", no_root_sh)

        def helper():
            rm(MARK)
            p = run(["sudo", "-n", str(HELPER)], input_text=PW + "\n", user="wingmankvm")
            expect(p.returncode == 0, f"fixed helper failed: {red(p.stderr)}")
            expect(MARK.exists(), "fixed helper did not run")
            expect(PW not in p.stdout + p.stderr, "password was exposed")
            return "helper ran as root for fixed wingman target"
        check(report, "helper_fixed_target", helper)

        def helper_args():
            p = run(["sudo", "-n", str(HELPER), "extra"], input_text=PW + "\n",
                    user="wingmankvm")
            expect(p.returncode != 0, "helper accepted extra arguments")
            return "helper rejected extra arguments"
        check(report, "helper_rejects_arguments", helper_args)

        def upgrade_trigger():
            UPGRADE.parent.mkdir(parents=True, exist_ok=True)
            UPGRADE.write_text(UPGRADE_STUB); os.chmod(UPGRADE, 0o755); os.chown(UPGRADE, 0, 0)
            rm(UPGRADE_MARK)
            p = run(["sudo", "-n", str(UPGRADE), "--start"], user="wingmankvm")
            expect(p.returncode == 0 and UPGRADE_MARK.read_text().strip() == "--start",
                   f"fixed upgrade trigger failed: {red(p.stderr)}")
            for argv in (["--run"], ["--start", "x"], [], ["--start", "--run"]):
                rm(UPGRADE_MARK)
                p = run(["sudo", "-n", str(UPGRADE), *argv], user="wingmankvm")
                expect(p.returncode != 0 and not UPGRADE_MARK.exists(),
                       f"upgrade helper accepted arguments {argv}")
            p = run(["sudo", "-n", str(UPGRADE), "--start"], user="wingman")
            expect(p.returncode != 0, "wingman can trigger the upgrade helper")
            return "only the exact --start argument list is allowed, only for wingmankvm"
        check(report, "upgrade_trigger_fixed_arguments", upgrade_trigger)

        def wingman_no_root():
            p = run(["sudo", "-n", "-u", "root", "id", "-u"], user="wingman")
            expect(p.returncode != 0, "wingman unexpectedly obtained root in restricted mode")
            return "wingman denied root sudo"
        check(report, "wingman_cannot_sudo_root_restricted", wingman_no_root)

        def privileged_install():
            put(files[2], PRIV); ok(["visudo", "-c"])
            expect(has_allowed(listing()), "privileged rule is not visible")
            return "privileged optional rule installed"
        check(report, "privileged_policy_installed", privileged_install)

        def privileged_sudo():
            p = run(["sudo", "-S", "-k", "-p", "", "-u", "root", "id", "-u"],
                    input_text=PW + "\n", user="wingman")
            expect(p.returncode == 0 and p.stdout.strip() == "0",
                   f"privileged sudo failed: {red(p.stderr or p.stdout)}")
            return "wingman obtained uid 0 with synthetic password"
        check(report, "privileged_sudo_root", privileged_sudo)

        def foreign_grant():
            rm(PRIV); FOREIGN.write_text("wingman ALL=(ALL:ALL) ALL\n")
            os.chmod(FOREIGN, 0o440); os.chown(FOREIGN, 0, 0)
            visudo(FOREIGN); ok(["visudo", "-c"])
            expect(has_allowed(listing()), "foreign grant was not visible to sudo -l")
            result = run(["bash", "-c", 'source "$1"; MAINTENANCE_MODE=restricted; postflight_maintenance_mode', "policy-check", str(files[0])])
            expect(result.returncode != 0 and "unexpected host sudo grant" in result.stderr + result.stdout,
                   "actual installer postflight did not reject a foreign sudo grant")
            return "actual installer postflight rejected foreign sudo grant"
        check(report, "restricted_detects_foreign_grant", foreign_grant)
    except Blocked as exc:
        status, report["reason"] = "BLOCKED", red(exc)
    except Stop:
        status = "FAIL"
    except Exception as exc:
        status = "FAIL"; report["reason"] = red(f"{type(exc).__name__}: {exc}")
    finally:
        rm(MARK); rm(UPGRADE_MARK)
        for path, backup in backups.items():
            try: restore(path, backup)
            except Exception: pass
        rm(FOREIGN)
        shutil.rmtree(tempdir, ignore_errors=True)
    report["status"] = status
    write_report(args.report, report)
    print(json.dumps({"status": status, "report": args.report,
                      "tests": len(report["tests"])}))
    return 0 if status == "PASS" else (2 if status == "BLOCKED" else 1)

if __name__ == "__main__":
    raise SystemExit(main())
