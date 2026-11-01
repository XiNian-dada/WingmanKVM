# Testing and regression gates

> This is a translation of [docs/TESTING.md](../TESTING.md); the Chinese version is authoritative.

This document describes local and CI gates for WingmanKVM. It does not claim
that any baseline, hardware, or CI run was executed while authoring these files.

## Prerequisites

- Rust toolchain pinned to `1.93.1` (`rustup`), `cargo`, and a C toolchain.
- Python 3.12+ with `venv`.
- Playwright pinned by CI; locally install the same version used by the workflow:
  `python -m pip install "playwright==<PLAYWRIGHT_VERSION>"`
  and `python -m playwright install --with-deps chromium`.
- `--browser-channel chrome` uses an installed Google Chrome. Use
  `--browser-channel chromium` when only Playwright's Chromium is available.
- Docker is only needed for the disposable permission-boundary gate.

The API scripts use loopback ports, test-owned state directories, and test-only
`sudo`/`ffmpeg` stubs. They do not create real accounts, change host policy, or
touch capture/HID/GPIO hardware.

## Native gates

```bash
cargo test --locked
cargo test --locked -p dtls
cargo build --locked
```

## HTTP/API gates (real binary, black-box)

```bash
BIN="$PWD/target/debug/wingmankvm"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/wkvm-regressions.XXXXXX")"

python3 scripts/verify_p1_api.py --binary "$BIN" --scratch-dir "$SCRATCH/p1-api" --report "$SCRATCH/p1-api.json"
python3 scripts/verify_p2_api.py --binary "$BIN" --scratch-dir "$SCRATCH/p2-api" --report "$SCRATCH/p2-api.json"
python3 scripts/verify_p2_login.py --binary "$BIN" --scratch-dir "$SCRATCH/p2-login" --report "$SCRATCH/p2-login.json"
python3 scripts/verify_p2b_probe.py --binary "$BIN" --scratch-dir "$SCRATCH/p2b-probe" --report "$SCRATCH/p2b-probe.json"
python3 scripts/verify_p2b_upload.py --binary "$BIN" --scratch-dir "$SCRATCH/p2b-upload" --report "$SCRATCH/p2b-upload.json"
```

## UI gates (real Chrome DOM/JS, mocked API)

These scripts serve the real repository UI and drive a real browser, but all
HTTP API responses and WebSocket objects are mocked. They are not a real-backend
or hardware test.

```bash
python3 scripts/verify_p1_ui.py --repo . --output-dir "$SCRATCH/p1-ui" --browser-channel chromium
python3 scripts/verify_p2_ui.py --repo . --output-dir "$SCRATCH/p2-ui" --browser-channel chromium
python3 scripts/verify_p2b_ui.py --repo . --output-dir "$SCRATCH/p2b-ui" --browser-channel chromium
```

Headless Chromium may refuse pointer lock/fullscreen. Those cases are recorded
as `"ok": null, "skipped": true`; the orchestrator counts them as explicit
non-failure skips, never as true passes. `--skip-ui` exists for local debugging
only and is recorded as skipped; CI always runs the UI gates.

## Sequential orchestrator

The full orchestrator is a Linux gate and requires kernel/Python pidfd support
for safe test-process cleanup. On macOS, run the individual HTTP/UI scripts;
the deployment transaction harness is Linux-specific.


```bash
python3 scripts/verify_regressions.py \
  --repo . \
  --binary "$BIN" \
  --scratch-dir "$SCRATCH/orchestrator" \
  --report "$SCRATCH/orchestrator-report.json" \
  --browser-channel chromium
```

It invokes every script exactly once in bounded child processes. Raw app scratch,
browser profiles, and raw stdout/stderr are removed after each case. Cleanup is
limited to the unique `run-*` directory created inside `--scratch-dir` and each
case's newly created private `/tmp/wkvm-reg-*` directory. The short per-case
`TMPDIR` avoids Chromium's Unix socket path limit on deeply nested CI paths;
the exact owned path is recorded and removed in `finally`. Cleanup never deletes
the parent scratch directory, shared `/tmp` entries, or repository files. Only sanitized
`stdout.log`, `stderr.log`, UI screenshots/report JSON, and case/summary JSON
remain. The summary records the exact sanitized report path and cleanup scope.

CI uploads only the exact sanitized summary files
`wingmankvm-reports/regressions.json` and
`wingmankvm-privilege-reports/privileges.json`, and only on failure. It never
uploads the whole scratch tree, raw app logs, browser profiles, or auth state.

The orchestrator supports the existing API `{checks, summary}` format, UI
`{status, checks}` format including `ok: null` skips, and the upgrade runner's
`{cases: [{name, result}], all_passed}` format. Malformed or unrecognized
upgrade cases are reported as failures rather than silently passing.

Issue #17 adds `scripts/verify_p2c_cookie.py` (`--binary --scratch-dir --report`)
and issue #19 adds `scripts/verify_p2c_upgrade.py` (`--repo --scratch-dir
--report`). Both are integrated in this PR and are mandatory in CI:

```bash
python3 scripts/verify_regressions.py ... \
  --require-gate p2c_cookie \
  --require-gate p2c_upgrade
```

## CI scope (`.github/workflows/regression.yml`)

| Gate | Covered scope | Not covered |
| --- | --- | --- |
| `cargo test` | Native unit/integration behavior | Real hardware |
| API scripts | Real binary over loopback, stubbed host tools | Real capture/HID/GPIO |
| UI scripts | Real Chrome DOM/JS with mocked API | Real backend, real devices |
| Cookie/upgrade gates | Session-cookie and upgrade boundaries | Host deployment |
| Container permission gate | Disposable-container privilege boundary | Host root policy mutations |
| Hardware smoke | Manual checklist below | CI |

CI uses `ubuntu-24.04`, read-only `contents` permission, no secrets, no
deployment, and no `pull_request_target`. Failure artifacts are limited to the
two sanitized summary reports.

## Security, cookie, upgrade, and permission gates

- Cookie/session security (`newCookie`, issue #17): `scripts/verify_p2c_cookie.py`
  owns the assertions; the orchestrator aggregates its report and CI makes it
  mandatory.
- Upgrade safety (issue #19): `scripts/verify_p2c_upgrade.py` runs with the
  repository and an isolated scratch directory. It is mandatory in CI and its
  `{cases, all_passed}` schema is parsed explicitly.
- Permission boundary: `scripts/verify_p2c_privileges.py` runs only inside the
  disposable image from `scripts/fixtures/privilege.Dockerfile`. The image is
  built from Dockerfile stdin with no repository build context:

```bash
docker build -t wingmankvm-privilege-test - < scripts/fixtures/privilege.Dockerfile
REPORTS="$(mktemp -d "${TMPDIR:-/tmp}/wkvm-privilege.XXXXXX")"
docker run --rm --network none \
  --mount type=bind,src="$PWD",dst=/repo,readonly \
  --mount type=bind,src="$REPORTS",dst=/reports \
  --workdir /repo \
  wingmankvm-privilege-test \
  python3 scripts/verify_p2c_privileges.py --repo /repo --disposable-root --report /reports/privileges.json
```

Never use `--privileged`, mount the Docker socket, or mount host `/etc` or
`/dev` into this container. Do not mutate host root accounts or policies.

## Process cleanup limits

- On Linux, owned descendants are enumerated through `/proc` before signalling.
  Pidfds are opened first, and only `pidfd_send_signal` is used after the direct
  child is reaped. Cached numeric PIDs are never signalled after reaping.
- On other platforms, cleanup is limited to the direct child with a bounded
  terminate/wait/kill fallback; no system-wide or user-wide process scan is used.
- Timeouts/interrupts reap the direct child and terminate fixture servers that
  created their own session, via pidfds on Linux.
- Cleanup only touches the unique `run-*` directory under `--scratch-dir` and
  the exact private per-case `TMPDIR` created by that run. Raw child scratch and
  these short temporary directories are removed.

## Manual hardware smoke checklist (not CI)

- HDMI capture: first valid JPEG, LIVE state, stale >5s handling, recovery.
- USB gadget: keyboard/mouse input, absolute and relative modes, lock LEDs.
- GPIO power: short/long press behavior against a disposable target.
- Virtual media: ISO/IMG attach, read-only defaults, detach cleanup.
- Record results manually; never claim CI covered hardware.

## What the latency tool measures (#94)

"Latency measurement" in the diagnostics panel measures the **input round trip**, not video latency.

- Each sample runs from the browser issuing a Caps Lock key request until the browser
  reads the flipped LED through `GET /api/hid/leds`. It covers browser → service → USB
  gadget → target OS → LED output report → service → browser, and never touches HDMI
  capture, encoding, decoding or display. Polling is an HTTP round trip each time, so a
  sample can read high by up to about one "LED poll time".
- Min, median and P95 (nearest rank) count only samples where the flip was seen; "valid
  samples" is `seen / sent`. A run has 10 samples, so P95 is close to the maximum.
- Bitrate is context only: H.264 shows measured receive bitrate (needs the quality overlay
  or adaptive bitrate on) and target bitrate; MJPEG exposes no byte counts, so it shows
  unavailable. This does not show a causal link between bitrate and latency.
- End-to-end video latency has no reliable method without software on the target, which
  would have to display a recognisable timestamp. The UI says "Not measured" and shows no
  estimate. On-screen Caps Lock hints vary by OS and are not a general signal. If you need
  this number, film the local action and the browser picture with an external fast camera.

Keys are sent only after the user clicks and confirms. Caps Lock is restored from the LED
state when a run ends or is stopped. A run aborts after 2 consecutive presses with no LED
report (some targets do not report, such as certain BIOSes; macOS may ignore very short
Caps Lock presses). "LED back to original state" then only describes the LED reading; the
target's real state cannot be confirmed. A run refuses to start during paste typing or
macro playback, and stops when the page goes to the background, since timer throttling
would skew samples.

Needs a real device: Caps Lock reporting, restore behaviour, values over WebSocket and HTTP.

## Baseline provenance

The scripts encode checks derived from earlier issue-specific fixes. This
document references those existing scripts and their assertions; it does not
claim that any old baseline commit or test suite was rerun here.
