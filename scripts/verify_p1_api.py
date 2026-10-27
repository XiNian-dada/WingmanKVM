#!/usr/bin/env python3
"""Black-box P1 regression runner for WingmanKVM issues #2/#3/#4/#5.
Secrets stay in memory; only sanitized results are printed or reported.
Exit code 0 means every check passed.
"""
import argparse, base64, http.client, json, os, re, socket, subprocess, sys, tempfile, threading, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

USERNAME = "p1admin"
PASSWORD = "P1-Verify-Str0ng!"
WRONG_PASSWORD = "P1-Wrong-Passw0rd!"
TIMEOUT, LOGIN_TIMEOUT, WS_TIMEOUT, READY_TIMEOUT = 20.0, 30.0, 5.0, 20.0
NAMES = ("setup_preserves_server_config", "terminal_ws_origin_policy", "concurrent_config_patch", "login_rate_limit_concurrency")
ANSI = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))")
TOKEN = re.compile(r"setup_token=([A-Za-z0-9_-]+)")

def free_port(exclude=None):
    for _ in range(50):
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            sock.bind(("127.0.0.1", 0)); port = sock.getsockname()[1]
        finally:
            sock.close()
        if port != exclude:
            return port
    raise RuntimeError("could not allocate a loopback port")

class Runner:
    def __init__(self, work, binary):
        self.work, self.binary = work, binary
        self.state, self.bin, self.log = work / "state", work / "bin", work / "wingmankvm.log"
        self.port = free_port(); self.origin = f"http://127.0.0.1:{self.port}"
        self.secrets = [PASSWORD, WRONG_PASSWORD]
        self.proc = self.logf = None; self.setup_status = self.cookie = None
        self.expected = {"listen_address": "127.0.0.1", "port": self.port,
                         "proxy_protocol": "disabled", "trusted_http_proxies": ["127.0.0.2"]}
        self.checks = {}
    def redact(self, text):
        for secret in self.secrets:
            if secret:
                text = text.replace(secret, "[redacted]")
        return text
    def check(self, name, status, **details):
        self.checks[name] = {"status": status, **details}
    def prepare(self):
        self.state.mkdir(parents=True); (self.state / "images").mkdir(); self.bin.mkdir()
        stub = self.bin / "sudo"
        stub.write_text("#!/bin/sh\n# Test stub: discard stdin, never execute arguments.\n"
                        "/bin/cat >/dev/null 2>&1\nexit 0\n")
        stub.chmod(0o755)
        config = {"version": 1, "server": dict(self.expected),
                  "video": {"auto_detect": False, "h264": {"ffmpeg_path": "/nonexistent-wingmankvm-p1-ffmpeg"}}}
        (self.state / "config.json").write_text(json.dumps(config) + "\n")
    def start(self):
        env = os.environ.copy()
        env["WINGMANKVM_STATE_DIR"] = str(self.state)
        env["PATH"] = str(self.bin) + os.pathsep + env.get("PATH", "/usr/bin:/bin")
        env["RUST_LOG"] = "wingmankvm=info"; env.pop("WINGMANKVM_PROXY_PROTOCOL", None)
        self.logf = open(self.log, "w", encoding="utf-8", errors="replace", buffering=1)
        self.proc = subprocess.Popen([str(self.binary)], cwd=str(self.work), env=env,
                                     stdout=self.logf, stderr=subprocess.STDOUT,
                                     close_fds=True, start_new_session=(os.name == "posix"))
    def cleanup(self):
        proc = self.proc
        if proc is not None:
            try:
                if proc.poll() is None: proc.terminate()
                try: proc.wait(timeout=5)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired): pass
            finally: self.proc = None
        if self.logf is not None: self.logf.close(); self.logf = None
    def req(self, method, path, body=None, cookie=None, origin=True, timeout=TIMEOUT):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        headers, data = {"Accept": "application/json"}, None
        if body is not None:
            headers["Content-Type"] = "application/json"; data = json.dumps(body).encode("utf-8")
        if cookie: headers["Cookie"] = f"wingman_session={cookie}"
        if origin: headers["Origin"] = self.origin
        try:
            conn.request(method, path, body=data, headers=headers)
            response = conn.getresponse()
            return response.status, response.getheaders(), response.read()
        finally:
            conn.close()
    def get_config(self):
        try:
            status, _, raw = self.req("GET", "/api/config", cookie=self.cookie)
            return (status, json.loads(raw)) if status == 200 else (status, None)
        except (OSError, ValueError, http.client.HTTPException):
            return None, None
    def health(self):
        try: return self.req("GET", "/healthz", origin=False, timeout=2.0)[0] == 200
        except (OSError, http.client.HTTPException): return False
    def token(self):
        try: match = TOKEN.search(ANSI.sub("", self.log.read_text(errors="ignore")))
        except OSError: return None
        if match:
            if match.group(1) not in self.secrets: self.secrets.append(match.group(1))
            return match.group(1)
        return None
    def ready(self):
        deadline = time.monotonic() + READY_TIMEOUT
        while time.monotonic() < deadline:
            if self.proc.poll() is not None: return False
            if self.health() and self.token(): return True
            time.sleep(0.1)
        return False
    def setup(self, token):
        body = {"username": USERNAME, "password": PASSWORD, "setup_token": token,
                "video_device": None, "keyboard_device": None, "mouse_device": None,
                "absolute_pointer_device": None, "pointer_mode": "absolute", "power_enabled": False,
                "media_enabled": False, "image_directory": str(self.state / "images")}
        status, headers, _ = self.req("POST", "/api/setup", body=body, timeout=60.0)
        self.setup_status = status
        for key, value in headers:
            if key.lower() == "set-cookie" and value.startswith("wingman_session="):
                self.cookie = value.split(";", 1)[0].split("=", 1)[1].strip().strip('"')
                self.secrets.append(self.cookie); break
    def check_setup(self):
        try: saved = json.loads((self.state / "config.json").read_text()).get("server")
        except (OSError, ValueError, AttributeError): saved = None
        live_status, live = self.get_config()
        live_server = live.get("server") if isinstance(live, dict) else None
        saved_ok, live_ok = saved == self.expected, live_status == 200 and live_server == self.expected
        self.check("setup_preserves_server_config",
                   "pass" if self.setup_status == 200 and saved_ok and live_ok else "fail",
                   setup_status=self.setup_status, saved_server_matches=saved_ok, live_status=live_status,
                   live_server_matches=live_ok, expected_server=self.expected,
                   saved_server=saved, live_server=live_server)
    def ws_status(self, origin, forwarded_proto, cookie):
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        lines = ["GET /api/terminal/ws HTTP/1.1", f"Host: 127.0.0.1:{self.port}", "Connection: Upgrade",
                 "Upgrade: websocket", f"Sec-WebSocket-Key: {key}", "Sec-WebSocket-Version: 13",
                 f"Cookie: wingman_session={cookie}"]
        if origin is not None: lines.append(f"Origin: {origin}")
        if forwarded_proto is not None: lines.append(f"X-Forwarded-Proto: {forwarded_proto}")
        sock = socket.create_connection(("127.0.0.1", self.port), timeout=WS_TIMEOUT)
        data = b""
        try:
            sock.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("ascii"))
            deadline = time.monotonic() + WS_TIMEOUT
            while b"\r\n\r\n" not in data and len(data) < 65536:
                remaining = deadline - time.monotonic()
                if remaining <= 0: break
                sock.settimeout(remaining); chunk = sock.recv(4096)
                if not chunk: break
                data += chunk
        finally:
            try: sock.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            sock.close()
        fields = data.split(b"\r\n", 1)[0].decode("latin-1", "ignore").split()
        return int(fields[1]) if len(fields) >= 2 and fields[0].startswith("HTTP/") and fields[1].isdigit() else None
    def check_ws(self):
        if not self.cookie:
            self.check("terminal_ws_origin_policy", "blocked", reason="no session"); return
        other = free_port(exclude=self.port)
        cases = {"missing_origin": (None, None, 403), "null_origin": ("null", None, 403),
                 "same_site_other_port": (f"http://127.0.0.1:{other}", None, 403),
                 "https_on_http": (f"https://127.0.0.1:{self.port}", None, 403),
                 "spoofed_forwarded_proto": (f"https://127.0.0.1:{self.port}", "https", 403),
                 "matching_origin": (self.origin, None, 101)}
        results = {}
        for name, (origin, proto, expected) in cases.items():
            try: actual = self.ws_status(origin, proto, self.cookie)
            except (OSError, http.client.HTTPException): actual = None
            results[name] = {"expected": expected, "actual": actual if actual is not None else "none",
                             "pass": actual == expected}
        self.check("terminal_ws_origin_policy",
                   "pass" if all(item["pass"] for item in results.values()) else "fail", cases=results)
    def check_patch(self):
        if not self.cookie:
            self.check("concurrent_config_patch", "blocked", reason="no session"); return
        failed, counts = [], {}
        for round_no in range(1, 9):
            quality, read_only = 60 + round_no, bool(round_no % 2)
            patches = [{"video": {"jpeg_quality": quality}}, {"media": {"read_only_by_default": read_only}}]
            barrier = threading.Barrier(2)
            def send(patch):
                try:
                    barrier.wait(timeout=15)
                    return self.req("POST", "/api/config", body=patch, cookie=self.cookie)[0]
                except (threading.BrokenBarrierError, OSError, http.client.HTTPException): return None
            with ThreadPoolExecutor(max_workers=2) as pool:
                statuses = list(pool.map(send, patches))
            for status in statuses:
                key = "error" if status is None else str(status); counts[key] = counts.get(key, 0) + 1
            live_status, live = self.get_config()
            try: saved = json.loads((self.state / "config.json").read_text())
            except (OSError, ValueError): saved = None
            def values(config):
                return (config.get("video", {}).get("jpeg_quality"),
                        config.get("media", {}).get("read_only_by_default")) if isinstance(config, dict) else None
            live_ok, saved_ok = values(live) == (quality, read_only), values(saved) == (quality, read_only)
            consistent = isinstance(live, dict) and isinstance(saved, dict) and live.get("server") == saved.get("server")
            if statuses != [200, 200] or not live_ok or not saved_ok or not consistent:
                failed.append({"round": round_no, "statuses": [s if s is not None else "error" for s in statuses],
                               "live_status": live_status, "live_values_match": live_ok,
                               "saved_values_match": saved_ok, "disk_live_server_consistent": consistent})
        self.check("concurrent_config_patch", "pass" if not failed else "fail",
                   rounds=8, status_counts=counts, failed_rounds=failed)
    def login_once(self):
        try:
            return self.req("POST", "/api/login", origin=True, timeout=LOGIN_TIMEOUT,
                            body={"username": USERNAME, "password": WRONG_PASSWORD})[0]
        except (OSError, http.client.HTTPException): return None
    def check_login(self):
        barrier = threading.Barrier(12)
        def worker(_):
            try: barrier.wait(timeout=20)
            except threading.BrokenBarrierError: return None
            return self.login_once()
        with ThreadPoolExecutor(max_workers=12) as pool:
            statuses = list(pool.map(worker, range(12)))
        counts = {}
        for status in statuses:
            key = "error" if status is None else str(status); counts[key] = counts.get(key, 0) + 1
        thirteenth = self.login_once()
        only = set(counts).issubset({"401", "429"})
        enough, limited = counts.get("429", 0) >= 7, 1 <= counts.get("401", 0) <= 5
        final = thirteenth == 429
        self.check("login_rate_limit_concurrency", "pass" if only and enough and limited and final else "fail",
                   status_counts=counts, thirteenth_status=thirteenth if thirteenth is not None else "error",
                   only_401_or_429=only, at_least_7_rejected=enough,
                   between_1_and_5_verified=limited, thirteenth_is_429=final)
    def run_check(self, name, func, *args):
        try: func(*args)
        except Exception as error:  # noqa: BLE001 - continue remaining checks
            self.check(name, "fail", error=self.redact(f"{type(error).__name__}: {error}"))
    def run(self):
        self.prepare(); self.start()
        if not self.ready():
            for name in NAMES: self.check(name, "blocked", reason="not healthy with setup token")
            return self.report()
        token = self.token()
        if token: self.run_check("setup_preserves_server_config", self.setup, token)
        self.run_check("setup_preserves_server_config", self.check_setup)
        self.run_check("terminal_ws_origin_policy", self.check_ws)
        self.run_check("concurrent_config_patch", self.check_patch)
        self.run_check("login_rate_limit_concurrency", self.check_login)
        return self.report()
    def report(self):
        states = [self.checks.get(name, {}).get("status") for name in NAMES]
        summary = {"pass": states.count("pass"), "fail": states.count("fail"), "blocked": states.count("blocked")}
        summary["ok"] = summary["pass"] == len(NAMES)
        return {"binary": str(self.binary), "checks": self.checks, "summary": summary}

def main(argv=None):
    parser = argparse.ArgumentParser(description="P1 black-box API regression runner")
    parser.add_argument("--binary", required=True, help="absolute path to wingmankvm")
    parser.add_argument("--scratch-dir", required=True, help="directory for temporary data")
    parser.add_argument("--report", help="optional JSON report path")
    args = parser.parse_args(argv)
    binary, scratch = os.path.abspath(args.binary), os.path.abspath(args.scratch_dir)
    if not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        parser.error("--binary must be an executable file")
    if scratch == os.path.sep: parser.error("--scratch-dir must not be the filesystem root")
    os.makedirs(scratch, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p1-", dir=scratch) as work:
        runner = Runner(Path(work), Path(binary))
        try: report = runner.run()
        except Exception as error:  # noqa: BLE001 - cleanup always runs
            for name in NAMES:
                if name not in runner.checks:
                    runner.check(name, "blocked", reason=runner.redact(f"{type(error).__name__}: {error}"))
            report = runner.report()
        finally: runner.cleanup()
    summary = report["summary"]
    print(f"P1 API regression: {'PASS' if summary['ok'] else 'FAIL'}")
    for name in NAMES: print(f"  {name}: {report['checks'].get(name, {}).get('status', 'missing')}")
    print(f"  summary: {summary['pass']} pass, {summary['fail']} fail, {summary['blocked']} blocked")
    if args.report:
        try:
            with open(args.report, "w", encoding="utf-8") as handle:
                json.dump(report, handle, indent=2, sort_keys=True); handle.write("\n")
            print(f"  report: {args.report}")
        except OSError as error:
            print(f"  report error: {type(error).__name__}", file=sys.stderr); return 1
    return 0 if summary["ok"] else 1

if __name__ == "__main__":
    sys.exit(main())
