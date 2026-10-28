#!/usr/bin/env python3
"""P2b black-box upload runner for ISSUE-14. Output is sanitized."""
import argparse, hashlib, json, os, socket, sys, tempfile, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_p1_api import Runner  # noqa: E402

BOUNDARY = "wingmankvm-p2b-boundary"
SMALL_LIMIT = 4096
READ_TIMEOUT = 23.0
NAMES = ("busy_then_idle_timeout_recovery", "progress_and_content", "disconnect_cleanup", "oversized_413", "duplicate_409_preserves_hash")

class UploadRunner(Runner):
    def __init__(self, work, binary):
        super().__init__(work, binary); self.images = self.state / "images"; self.sockets = []; self.media_ready = False
    def report(self):
        states = [self.checks.get(n, {}).get("status") for n in NAMES]
        summary = {"pass": states.count("pass"), "fail": states.count("fail"), "blocked": states.count("blocked")}
        summary["ok"] = summary["pass"] == len(NAMES)
        return {"binary": str(self.binary), "checks": self.checks, "summary": summary}
    def close(self, sock):
        try: sock.shutdown(socket.SHUT_RDWR)
        except OSError: pass
        try: sock.close()
        except OSError: pass
        if sock in self.sockets: self.sockets.remove(sock)
    def cleanup(self):
        for sock in list(self.sockets): self.close(sock)
        super().cleanup()
    def block_all(self, reason):
        for name in NAMES: self.check(name, "blocked", reason=self.redact(reason))
    def partials(self):
        return sorted(p.name for p in self.images.iterdir() if p.name.endswith(".partial"))
    def wait_partial(self, timeout=5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.partials(): return True
            time.sleep(0.05)
        return False
    def wait_clean(self, timeout=5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not self.partials(): return True
            time.sleep(0.05)
        return False
    def body(self, name, payload):
        head = (f"--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\n"
                "Content-Type: application/octet-stream\r\n\r\n").encode()
        return head + payload + f"\r\n--{BOUNDARY}--\r\n".encode()
    def open_upload(self, name, payload, prefix=None):
        body = self.body(name, payload); sock = socket.create_connection(("127.0.0.1", self.port), timeout=READ_TIMEOUT)
        self.sockets.append(sock)
        head = (f"POST /api/media/upload HTTP/1.1\r\nHost: 127.0.0.1:{self.port}\r\nOrigin: {self.origin}\r\n"
                f"Cookie: wingman_session={self.cookie}\r\nContent-Type: multipart/form-data; boundary={BOUNDARY}\r\n"
                f"Content-Length: {len(body)}\r\nConnection: close\r\n\r\n").encode()
        sock.sendall(head); sock.sendall(body if prefix is None else body[:prefix]); return sock, body
    def status(self, sock, timeout):
        deadline = time.monotonic() + timeout; data = b""
        try:
            while b"\r\n\r\n" not in data and len(data) < 65536:
                remaining = deadline - time.monotonic()
                if remaining <= 0: break
                sock.settimeout(remaining); chunk = sock.recv(4096)
                if not chunk: break
                data += chunk
        except OSError: pass
        parts = data.split(b"\r\n", 1)[0].split()
        return int(parts[1]) if len(parts) >= 2 and parts[0].startswith(b"HTTP/") and parts[1].isdigit() else None
    def upload(self, name, payload):
        sock, _ = self.open_upload(name, payload)
        try: return self.status(sock, READ_TIMEOUT)
        finally: self.close(sock)
    def configure_media(self):
        status, _, raw = self.req("GET", "/api/config", cookie=self.cookie)
        if status != 200: raise RuntimeError(f"config GET returned {status}")
        config = json.loads(raw); media = config.setdefault("media", {})
        media.update({"enabled": True, "lun_path": str(self.state / "fake-lun.0"), "image_directory": str(self.images),
                      "max_upload_bytes": SMALL_LIMIT, "read_only_by_default": False})
        status, _, _ = self.req("PUT", "/api/config", body=config, cookie=self.cookie)
        if status != 200: raise RuntimeError(f"config PUT returned {status}")
        self.media_ready = True
    def check_busy(self):
        if not self.media_ready: return self.check(NAMES[0], "blocked", reason="media config")
        first = second = None
        try:
            payload = b"P" * 2048; full = self.body("pending.iso", payload)
            first, _ = self.open_upload("pending.iso", payload, prefix=len(full) - 16)
            if not self.wait_partial(8.0): raise RuntimeError("pending upload never created a partial")
            started = time.monotonic(); second, _ = self.open_upload("busy.iso", b"busy")
            busy = self.status(second, 2.0); elapsed = time.monotonic() - started
            self.close(second); second = None; first_status = self.status(first, READ_TIMEOUT)
            fresh = self.upload("fresh.iso", b"fresh") if first_status == 408 else None
            content = (self.images / "fresh.iso").read_bytes() if first_status == 408 else b""
            ok = busy == 409 and elapsed < 1.0 and first_status == 408 and fresh == 200 and content == b"fresh" and not self.partials()
            self.check(NAMES[0], "pass" if ok else "fail", second_status=busy, second_elapsed_ms=round(elapsed * 1000),
                       first_status=first_status, fresh_status=fresh, content_ok=content == b"fresh", partials=self.partials())
        except Exception as error:  # noqa: BLE001
            self.check(NAMES[0], "fail", error=self.redact(f"{type(error).__name__}: {error}"))
        finally:
            if first is not None: self.close(first)
            if second is not None: self.close(second)
    def check_progress(self):
        if not self.media_ready: return self.check(NAMES[1], "blocked", reason="media config")
        sock = None
        try:
            payload = b"progress-content-" * 64; body = self.body("progress.iso", payload)
            sock, _ = self.open_upload("progress.iso", payload, prefix=0)
            for offset in range(0, len(body), 256):
                sock.sendall(body[offset:offset + 256]); time.sleep(0.15)
            status = self.status(sock, READ_TIMEOUT)
            saved = (self.images / "progress.iso").read_bytes() if status == 200 else b""
            expected = hashlib.sha256(payload).hexdigest(); actual = hashlib.sha256(saved).hexdigest() if saved else None
            self.check(NAMES[1], "pass" if status == 200 and actual == expected else "fail", http_status=status, sha256=actual, expected_sha256=expected)
        except Exception as error:  # noqa: BLE001
            self.check(NAMES[1], "fail", error=self.redact(f"{type(error).__name__}: {error}"))
        finally:
            if sock is not None: self.close(sock)
    def check_disconnect(self):
        if not self.media_ready: return self.check(NAMES[2], "blocked", reason="media config")
        sock = None
        try:
            payload = b"D" * 8192; body = self.body("disconnect.iso", payload)
            sock, _ = self.open_upload("disconnect.iso", payload, prefix=len(body) // 2)
            if not self.wait_partial(5.0): raise RuntimeError("disconnect upload never created a partial")
            self.close(sock); sock = None; cleaned = self.wait_clean(5.0)
            final_exists = (self.images / "disconnect.iso").exists()
            self.check(NAMES[2], "pass" if cleaned and not final_exists else "fail", cleaned=cleaned, final_exists=final_exists, partials=self.partials())
        except Exception as error:  # noqa: BLE001
            self.check(NAMES[2], "fail", error=self.redact(f"{type(error).__name__}: {error}"))
        finally:
            if sock is not None: self.close(sock)
    def check_oversized(self):
        if not self.media_ready: return self.check(NAMES[3], "blocked", reason="media config")
        try:
            status = self.upload("large.iso", b"X" * (SMALL_LIMIT + 1)); final_exists = (self.images / "large.iso").exists()
            ok = status == 413 and not final_exists and not self.partials()
            self.check(NAMES[3], "pass" if ok else "fail", http_status=status, final_exists=final_exists, partials=self.partials())
        except Exception as error:  # noqa: BLE001
            self.check(NAMES[3], "fail", error=self.redact(f"{type(error).__name__}: {error}"))
    def check_duplicate(self):
        if not self.media_ready: return self.check(NAMES[4], "blocked", reason="media config")
        try:
            first = self.upload("duplicate.iso", b"original-content")
            prior = hashlib.sha256((self.images / "duplicate.iso").read_bytes()).hexdigest() if first == 200 else None
            second = self.upload("duplicate.iso", b"replacement-content")
            current = hashlib.sha256((self.images / "duplicate.iso").read_bytes()).hexdigest() if second else None
            ok = first == 200 and second == 409 and prior is not None and prior == current and not self.partials()
            self.check(NAMES[4], "pass" if ok else "fail", first_status=first, second_status=second, prior_sha256=prior, current_sha256=current, partials=self.partials())
        except Exception as error:  # noqa: BLE001
            self.check(NAMES[4], "fail", error=self.redact(f"{type(error).__name__}: {error}"))
    def run(self):
        self.prepare(); self.start()
        if not self.ready(): self.block_all("service did not become ready"); return self.report()
        try:
            token = self.token()
            if token: self.setup(token)
        except Exception as error:  # noqa: BLE001
            self.block_all(f"setup failed: {type(error).__name__}"); return self.report()
        if self.setup_status != 200 or not self.cookie: self.block_all("setup did not return a session"); return self.report()
        try: self.configure_media()
        except Exception as error:  # noqa: BLE001
            self.block_all(f"media config failed: {type(error).__name__}"); return self.report()
        for name, check in zip(NAMES, (self.check_busy, self.check_progress, self.check_disconnect, self.check_oversized, self.check_duplicate)):
            self.run_check(name, check)
        return self.report()

def main(argv=None):
    parser = argparse.ArgumentParser(description="P2b upload regression runner")
    parser.add_argument("--binary", required=True); parser.add_argument("--scratch-dir", required=True); parser.add_argument("--report")
    args = parser.parse_args(argv); binary, scratch = os.path.abspath(args.binary), os.path.abspath(args.scratch_dir)
    if not os.path.isfile(binary) or not os.access(binary, os.X_OK): parser.error("--binary must be executable")
    if scratch == os.path.sep: parser.error("--scratch-dir must not be root")
    os.makedirs(scratch, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="wingmankvm-p2b-", dir=scratch) as work:
        runner = UploadRunner(Path(work), Path(binary))
        try: report = runner.run()
        except Exception as error:  # noqa: BLE001
            runner.block_all(f"runner error: {type(error).__name__}"); report = runner.report()
        finally: runner.cleanup()
    summary = report["summary"]
    print(f"P2b upload regression: {'PASS' if summary['ok'] else 'FAIL'}")
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
