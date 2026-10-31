#!/usr/bin/env python3
"""Fixture-driven regression checks for deploy/wingmankvm-doctor.

The script fabricates sysfs/dev trees in a temporary directory, puts tiny
command stubs on PATH, and runs the doctor with WINGMANKVM_* path overrides. It
never touches real /sys, /dev, systemd, network ports, or capture hardware.
Exit code 0 means every assertion passed.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_DOCTOR = REPO / "deploy" / "wingmankvm-doctor"


class Checks:
    def __init__(self):
        self.items = []

    def check(self, name, condition, detail=""):
        self.items.append((name, bool(condition), detail))

    def equal(self, name, actual, expected):
        self.check(name, actual == expected, f"expected {expected!r}, got {actual!r}")

    def report(self):
        failed = 0
        for name, ok, detail in self.items:
            print(f"{'PASS' if ok else 'FAIL'} {name}")
            if not ok and detail:
                print(f"     {detail}")
            if not ok:
                failed += 1
        print(f"\n{len(self.items) - failed} passed, {failed} failed")
        return failed == 0


def write(path, text=""):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def write_exe(path, body):
    write(path, "#!/bin/sh\n" + body)
    path.chmod(0o755)
    return path


def video_stub(modes):
    body = [
        "dev=\n",
        'while [ "$#" -gt 0 ]; do\n',
        "  case $1 in\n",
        "    -d|--device) dev=$2; shift 2 ;;\n",
        "    *) shift ;;\n",
        "  esac\n",
        "done\n",
        'case "$dev" in\n',
    ]
    for index, mode in enumerate(modes):
        if mode == "mjpg":
            body.append(
                f'  */video{index})\n'
                '    printf "Driver Info:\\n\\tVideo Capture\\n\\tStreaming\\n"\n'
                "    printf \"[0]: 'MJPG' (Motion-JPEG, compressed)\\n\"\n"
                "    ;;\n"
            )
        elif mode == "yuyv":
            body.append(
                f'  */video{index})\n'
                '    printf "Driver Info:\\n\\tVideo Capture\\n\\tStreaming\\n"\n'
                "    printf \"[0]: 'YUYV' (YUYV 4:2:2)\\n\"\n"
                "    ;;\n"
            )
        elif mode == "metadata":
            body.append(
                f'  */video{index})\n'
                '    printf "Driver Info:\\n\\tMetadata Capture\\n"\n'
                "    ;;\n"
            )
        else:
            raise ValueError(f"unknown video mode: {mode}")
    body.append("  *) exit 1 ;;\nesac\n")
    return "".join(body)


class Fixture:
    def __init__(self, root):
        self.root = Path(root)
        self.configfs = self.root / "sys" / "kernel" / "config"
        self.gadgets = self.configfs / "usb_gadget"
        self.udc = self.root / "sys" / "class" / "udc"
        self.modules = self.root / "sys" / "module"
        self.proc = self.root / "proc" / "filesystems"
        self.dev = self.root / "dev"
        self.roles = self.root / "sys" / "class" / "usb_role"
        self.bin = self.root / "bin"
        self.bin.mkdir(parents=True)
        self.udc.mkdir(parents=True)
        self.roles.mkdir(parents=True)
        self.dev.mkdir(parents=True)

    def python3(self):
        target = Path(sys.executable)
        link = self.bin / "python3"
        if not link.exists():
            link.symlink_to(target)
        return link

    def build(
        self,
        *,
        udcs=("fe800000.usb",),
        bindings=None,
        gadget_udc="",
        role="device",
        role_present=True,
        videos=("mjpg", "yuyv"),
        v4l2=True,
        gpioset="2.1.3",
        port_busy=False,
        app_state="active",
        gadget_state="active",
        http_code="200",
        configfs=True,
        libcomposite=True,
        systemctl=True,
        ss=True,
        curl=True,
    ):
        bindings = bindings or {}
        self.python3()

        if configfs:
            write(self.proc, "nodev\tconfigfs\nnodev\tsysfs\n")
            write(self.gadgets / "wingmankvm" / "UDC", gadget_udc)
            for name, udc in bindings.items():
                write(self.gadgets / name / "UDC", udc)
        else:
            write(self.proc, "nodev\tsysfs\n")

        if libcomposite:
            (self.modules / "libcomposite").mkdir(parents=True, exist_ok=True)

        for udc in udcs:
            (self.udc / udc).mkdir()

        if role is None:
            role_present = False
        if role_present:
            write(self.roles / "fe800000.usb-role-switch" / "role", role)

        for index, mode in enumerate(videos):
            write(self.dev / f"video{index}", "")
        if v4l2:
            write_exe(self.bin / "v4l2-ctl", video_stub(videos))

        if gpioset is not None:
            write_exe(
                self.bin / "gpioset",
                f"if [ \"$1\" = \"--version\" ]; then printf 'gpioset (libgpiod) v{gpioset}\\n'; fi\n",
            )

        if ss:
            line = "LISTEN 0 128 0.0.0.0:8080 0.0.0.0:*\\n" if port_busy else ""
            write_exe(self.bin / "ss", f"printf '{line}' ; exit 0\n")

        if systemctl:
            write_exe(
                self.bin / "systemctl",
                "case \"$1\" in\n"
                "  is-active)\n"
                "    case \"$2\" in\n"
                f"      wingmankvm.service) printf '%s\\n' \"${{FAKE_APP_STATE:-{app_state}}}\" ;;\n"
                f"      wingmankvm-gadget.service) printf '%s\\n' \"${{FAKE_GADGET_STATE:-{gadget_state}}}\" ;;\n"
                "      *) printf 'inactive\\n' ;;\n"
                "    esac\n"
                "    ;;\n"
                "esac\n",
            )

        if curl:
            if http_code is None:
                write_exe(self.bin / "curl", "printf '000' ; exit 7\n")
            else:
                write_exe(self.bin / "curl", f"printf '%s' '{http_code}' ; exit 0\n")

        return self

    def env(self, overrides=None):
        spacing = {
            "PATH": f"{self.bin}:/usr/bin:/bin:/usr/sbin:/sbin",
            "WINGMANKVM_CONFIGFS_ROOT": str(self.configfs),
            "WINGMANKVM_GADGETS_ROOT": str(self.gadgets),
            "WINGMANKVM_UDC_CLASS_ROOT": str(self.udc),
            "WINGMANKVM_MODULE_ROOT": str(self.modules),
            "WINGMANKVM_MODPROBE": str(self.bin / "modprobe"),
            "WINGMANKVM_PROC_FILESYSTEMS": str(self.proc),
            "WINGMANKVM_DEV_ROOT": str(self.dev),
            "WINGMANKVM_USB_ROLE_ROOT": str(self.roles),
            "WINGMANKVM_PORT": "8080",
            "WINGMANKVM_HEALTH_URL": "http://127.0.0.1:8080/healthz",
            "WINGMANKVM_HEALTH_TIMEOUT": "1",
            "WINGMANKVM_SYSTEMCTL": str(self.bin / "systemctl"),
            "WINGMANKVM_V4L2CTL": str(self.bin / "v4l2-ctl"),
            "WINGMANKVM_GPIOSET": str(self.bin / "gpioset"),
            "WINGMANKVM_SSCTL": str(self.bin / "ss"),
        }
        env = {"HOME": str(self.root), "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"}
        env.update(spacing)
        if overrides:
            env.update(overrides)
        return env


def run_doctor(doctor, fixture, *args, env_overrides=None):
    return subprocess.run(
        ["/bin/bash", str(doctor), *args],
        env=fixture.env(env_overrides),
        capture_output=True,
        text=True,
        timeout=30,
    )


def run_json(doctor, fixture, *args, env_overrides=None):
    result = run_doctor(doctor, fixture, "--json", *args, env_overrides=env_overrides)
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError(
            f"invalid JSON (rc={result.returncode}): {error}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        ) from error
    return result, payload


def status_map(payload):
    return {item["id"]: item for item in payload["checks"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doctor", default=str(DEFAULT_DOCTOR))
    options = parser.parse_args()
    doctor = Path(options.doctor).resolve()
    checks = Checks()

    if not doctor.is_file():
        print(f"doctor not found: {doctor}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="wingmankvm-doctor-") as tmp:
        tmp = Path(tmp)

        # Healthy system: every required and optional check should pass.
        fixture = Fixture(tmp / "healthy").build()
        result, payload = run_json(doctor, fixture)
        checks.equal("healthy exit code", result.returncode, 0)
        checks.check("healthy ok flag", payload.get("ok") is True, str(payload.get("ok")))
        for check_id in (
            "configfs",
            "libcomposite",
            "udc",
            "udc_busy",
            "usb_role",
            "video",
            "python3",
            "v4l2_ctl",
            "gpioset",
            "port",
            "service_app",
            "service_gadget",
            "health",
        ):
            item = status_map(payload).get(check_id)
            checks.equal(f"healthy {check_id}", item and item["status"], "pass")
        checks.equal("healthy summary", payload.get("summary"), {"pass": 13, "warn": 0, "fail": 0})

        # No UDC at all.
        fixture = Fixture(tmp / "no-udc").build(udcs=(), role=None)
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("no-udc udc", items["udc"]["status"], "fail")
        checks.equal("no-udc udc_busy", items["udc_busy"]["status"], "fail")
        checks.check("no-udc ok flag false", payload.get("ok") is False)
        checks.check("no-udc hint mentions role-switch", "role-switch" in items["udc"]["hint"])

        # The only UDC is owned by another gadget.
        fixture = Fixture(tmp / "udc-busy").build(bindings={"legacy-gadget": "fe800000.usb"})
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("udc-busy", items["udc_busy"]["status"], "fail")
        checks.check("udc-busy names owner", "legacy-gadget=fe800000.usb" in items["udc_busy"]["detail"])

        # One UDC is free even though another is occupied.
        fixture = Fixture(tmp / "udc-partial").build(
            udcs=("fe800000.usb", "fe900000.usb"),
            bindings={"legacy-gadget": "fe800000.usb"},
        )
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("udc-partial", items["udc_busy"]["status"], "warn")
        checks.check("udc-partial mentions free UDC", "fe900000.usb" in items["udc_busy"]["detail"])

        # Explicitly configured UDC is already owned by another gadget.
        fixture = Fixture(tmp / "udc-requested-busy").build(bindings={"legacy-gadget": "fe800000.usb"})
        _, payload = run_json(
            doctor, fixture, env_overrides={"WINGMANKVM_GADGET_UDC": "fe800000.usb"}
        )
        checks.equal("requested-udc-busy", status_map(payload)["udc_busy"]["status"], "fail")

        # No capture card at all.
        fixture = Fixture(tmp / "no-video").build(videos=())
        _, payload = run_json(doctor, fixture)
        checks.equal("no-video", status_map(payload)["video"]["status"], "warn")

        # Capture node exists but lacks MJPG.
        fixture = Fixture(tmp / "no-mjpg").build(videos=("yuyv",))
        _, payload = run_json(doctor, fixture)
        item = status_map(payload)["video"]
        checks.equal("no-mjpg", item["status"], "fail")
        checks.check("no-mjpg hint", "MJPG" in item["hint"])

        # v4l2-ctl is missing: video check degrades but does not crash.
        fixture = Fixture(tmp / "no-v4l2").build(v4l2=False)
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("no-v4l2 video", items["video"]["status"], "warn")
        checks.equal("no-v4l2 dependency", items["v4l2_ctl"]["status"], "warn")

        # gpioset 1.x must be rejected; absent gpioset is only a warning.
        fixture = Fixture(tmp / "gpioset-v1").build(gpioset="1.6.4")
        _, payload = run_json(doctor, fixture)
        checks.equal("gpioset-v1", status_map(payload)["gpioset"]["status"], "fail")
        fixture = Fixture(tmp / "gpioset-missing").build(gpioset=None)
        _, payload = run_json(doctor, fixture)
        checks.equal("gpioset-missing", status_map(payload)["gpioset"]["status"], "warn")

        # Port already bound by another process.
        fixture = Fixture(tmp / "port-busy").build(port_busy=True)
        _, payload = run_json(doctor, fixture)
        checks.equal("port-busy", status_map(payload)["port"]["status"], "warn")

        # Failed service and an active service with a broken /healthz.
        fixture = Fixture(tmp / "service-failed").build(app_state="failed")
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("service-failed", items["service_app"]["status"], "fail")
        checks.equal("service-failed health", items["health"]["status"], "warn")
        fixture = Fixture(tmp / "health-fail").build(http_code=None)
        _, payload = run_json(doctor, fixture)
        checks.equal("health-unreachable", status_map(payload)["health"]["status"], "fail")
        fixture = Fixture(tmp / "health-500").build(http_code="500")
        _, payload = run_json(doctor, fixture)
        checks.equal("health-500", status_map(payload)["health"]["status"], "fail")

        # Missing configfs and libcomposite.
        fixture = Fixture(tmp / "no-configfs").build(configfs=False, libcomposite=False)
        _, payload = run_json(doctor, fixture)
        items = status_map(payload)
        checks.equal("no-configfs", items["configfs"]["status"], "fail")
        checks.equal("no-libcomposite", items["libcomposite"]["status"], "fail")

        # USB role is host: a warning with an actionable hint.
        fixture = Fixture(tmp / "role-host").build(role="host")
        _, payload = run_json(doctor, fixture)
        item = status_map(payload)["usb_role"]
        checks.equal("role-host", item["status"], "warn")
        checks.check("role-host hint", "echo device" in item["hint"])

        # Human output uses the documented labels and keeps the exit status.
        fixture = Fixture(tmp / "human").build(udcs=(), videos=())
        result = run_doctor(doctor, fixture)
        checks.check("human failure label", "[失败]" in result.stdout)
        checks.check("human warning label", "[警告]" in result.stdout)
        checks.check("human repair hint", "修复:" in result.stdout)
        checks.equal("human exit code", result.returncode, 1)

        # --preflight is advisory: only key checks, always exit 0.
        fixture = Fixture(tmp / "preflight").build(
            udcs=(), videos=(), port_busy=True, http_code=None
        )
        result = run_doctor(doctor, fixture, "--preflight")
        checks.equal("preflight exit code", result.returncode, 0)
        checks.check("preflight shows failure", "[失败]" in result.stdout)
        checks.check("preflight skips video", "视频采集设备" not in result.stdout)

        # gadget.env can supply the explicitly configured role path.
        fixture = Fixture(tmp / "env-file").build(role=None)
        role_path = fixture.roles / "board-role" / "role"
        write(role_path, "device")
        env_file = write(fixture.root / "gadget.env", f'WINGMANKVM_USB_ROLE_PATH="{role_path}"\n')
        _, payload = run_json(doctor, fixture, "--env-file", str(env_file))
        checks.equal("env-file role", status_map(payload)["usb_role"]["status"], "pass")

    ok = checks.report()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
