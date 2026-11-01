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

## Frontend modules and ESLint

The browser script is a set of native ES modules in `web/js/app/`, with no
bundler and no build step:

- `APP_JS_MODULES` in `src/web_ui.rs` embeds each file at compile time and the
  server serves it unchanged at `/assets/js/<name>` (`text/javascript;
  charset=utf-8`, `Cache-Control: no-store`, like `/assets/app.css`), so the
  release binary stays self-contained. `index.html` only loads
  `<script type="module" src="/assets/js/main.js">`; the browser fetches the
  other modules through their imports. The CSP `script-src 'self'` is unchanged.
- Modules reference siblings only through static
  `import { … } from './<name>.js'`. Mutable state that two or more modules read
  or write lives on the `shared` object in `state.js` (an imported binding is
  read-only in the importer); state used by one module stays local to it. No
  `window` globals (apart from the `window.__wingmanTest` test hooks, which
  exist only when a test set `window.__wingmanTestHooks = true` before the page
  loaded, see `ui_harness.TEST_HOOKS`; a normal page has none).
- DOM wiring lives in each module's exported `init()`, which `main.js` calls in
  the old fragment order before running the boot sequence; evaluating a module
  mostly just declares its functions and module state. With import cycles, modules
  are not evaluated in file-name order, so during evaluation a module may only do
  two things across modules:
  - Read bindings of modules that import nothing (`00-core.js`, `00-settings.js`,
    `state.js`): they are in no cycle and always evaluate before their importers.
  - Call another module's `function` declarations (initialized at link time,
    whatever the evaluation order), as long as the function reads no module state
    that may not be evaluated yet. For example `06`, `11` and `12*` call
    `latestOnly()` at the top level.

  Any other `const`/`let`/`class` binding may still be in its temporal dead zone;
  reading it throws `ReferenceError` and leaves a blank page. Turning `latestOnly`
  into a `const` arrow function crashes startup, and the UI suites report that
  error with its stack.
- To add a module: create the file in `web/js/app/`, `import` it where used,
  and add it to `APP_JS_MODULES`. Rust tests check that every file there is
  embedded, that every `import` names an embedded sibling module, and that
  every module is reachable from `main.js`.
- To add a stylesheet fragment: create the file in `web/css/app/` and add it to
  `APP_CSS_PARTS` in `src/web_ui.rs` in file-name order. `css_parts()` in
  `scripts/ui_bundle.py` compares the two and fails every UI suite when they
  disagree; otherwise tests look right while the release binary lacks the styles.

ESLint (flat config, `eslint.config.js`) only checks non-stylistic problems:
`no-undef`, `no-unused-vars`, `no-import-assign`, `no-const-assign`,
`no-func-assign`, `no-redeclare`, `no-dupe-keys`, `no-unreachable`, and the
local rule `local/import-exports` (`eslint-rules/import-exports.js`, using only
ESLint's own espree parser: an import must be a sibling `./<name>.js` that
exists and really exports the imported names; dynamic `import()` is rejected).
Versions are pinned in `package.json` and `package-lock.json`; Node.js
20.19+ / 22.13+ / 24+ is required.

```bash
npm ci
npx eslint --max-warnings 0
```

## One-line installer gate (no network, no root)

`scripts/verify_get_script.py` builds a fake release, serves it from a loopback
HTTP server, and also exercises a `file://` base. It checks latest-version
discovery, explicit `--version`, SHA256 verification, `--download-only`,
unsupported-architecture handling, and temporary-directory cleanup. It never
contacts GitHub and never runs the real installer.

```bash
python3 scripts/verify_get_script.py
```

## mDNS installer helpers (no network, no root)

`scripts/verify_mdns.py` sources `deploy/install.sh` against a scratch root and
runs `deploy/wingmankvm-setup-url` with fake `id`/`systemctl`/`hostname`. It
checks `--mdns-hostname` validation, the avahi service file, the minimal
`avahi-daemon.conf` `host-name=` edit (backup, idempotence, other sections
untouched) and that the `.local` URL is listed first only while avahi runs.
The setup-url checks are skipped on bash older than 4.4 outside Linux (stock
macOS); on Linux an old bash fails them instead of skipping.

```bash
python3 scripts/verify_mdns.py
```

## Doctor gate (no root, no hardware)

`scripts/verify_doctor.py` fabricates sysfs/dev trees in a temporary directory,
puts command stubs on `PATH` and runs `deploy/wingmankvm-doctor` with
`WINGMANKVM_*` path overrides. It checks the JSON and human output, exit codes
and repair hints for healthy and broken systems, and `--preflight`. It never
touches real `/sys`, `/dev`, systemd, network ports or capture hardware.

```bash
python3 scripts/verify_doctor.py
```

CI runs `verify_get_script.py`, `verify_mdns.py` and `verify_doctor.py` as their
own steps in `regression.yml`, before any toolchain is installed.

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
python3 scripts/verify_audit2_ui.py --repo . --output-dir "$SCRATCH/audit2-ui" --browser-channel chromium
# or all suites at once (macOS or Linux), with per-suite check counts:
python3 scripts/verify_ui_suite.py --output-dir "$SCRATCH/ui" --browser-channel chromium
```

All suites share `scripts/ui_harness.py` (static server, xterm assets, video
feed mock, `check()`, uncaught-error tracking and the JSON report). The page comes from `scripts/ui_bundle.py`: it inlines
the stylesheet into `index.html` and serves the `web/js/app/` modules at their
production paths `/assets/js/<name>`, so the browser runs the same module
graph as production. A new suite only defines its API
mock and scenarios; add it to `SUITES` in `verify_ui_suite.py` and to `CASES`
in `verify_regressions.py` so CI runs it.

Headless Chromium may refuse pointer lock/fullscreen. p2b spies on
`requestPointerLock`/`requestFullscreen`: if the app never called them the suite
fails (`pointerlock_requested_by_app`, `fullscreen_requested_by_app`). Only when
the app asked and the browser refused is the follow-up recorded as
`"ok": null, "skipped": true`; the orchestrator counts those as explicit
non-failure skips, never as true passes. `--skip-ui` exists for local debugging
only and is recorded as skipped; CI always runs the UI gates.

- Uncaught page errors: suites start the browser with
  `ui_harness.launch_browser()`. An uncaught exception or unhandled promise
  rejection on any page fails the suite, with message and stack in the report's
  `error`/`page_errors` and a `no_uncaught_page_errors` check. Context-level init
  scripts also run on the initial `about:blank`, where touching `localStorage`
  throws, so check `location.protocol` first.
- API mocks: each suite's default API mock answers unknown paths with 404 rather
  than a blanket 200. The app may swallow that 404, so
  `ui_harness.answer_unmocked()` also records the path and fails the suite
  (`no_unmocked_api_requests`): a renamed or new endpoint cannot pass unnoticed. When
  the page starts a new request, add it to the mock in the shape the server returns.
- Desktop input (the `desktop_*` scenarios in `verify_audit2_ui.py`): real
  `page.mouse` input and clicks, asserted on the payloads sent to the HID
  endpoints: absolute coordinates at known points in fit mode (±1, with
  letterboxing), left/right/middle buttons, wheel sign and the ±127 clamp, relative
  movement splitting, every `[data-quick-key]`, and sticky virtual-keyboard
  modifiers. When headless refuses pointer lock, `requestPointerLock` is stubbed and
  only "locks" when the app itself asks; real pointer lock still needs a headed
  browser or real hardware. No power or reset control is involved.

## README screenshots and demo animation (#96)

After a UI change, one command regenerates every screenshot in `docs/images/` and the
Chinese/English demo animations at the top of the READMEs (`demo-zh.webp`, `demo-en.webp`):

```bash
python3 scripts/capture_screenshots.py --browser-channel chrome
# screenshots only / animation only: add --skip-demo / --demo-only
# debug the animation alone and keep the PNG frames:
python3 scripts/capture_demo.py --lang en --output-dir "$SCRATCH/demo" --keep-frames "$SCRATCH/demo/frames"
```

- Like the UI gates, it serves the real UI through `scripts/ui_harness.py`; every `/api/**`
  call, the input WebSocket and `/video_feed` are mocked by `scripts/capture_fixtures.py`, and
  the picture is a synthetic terminal frame. No device is contacted.
- Demo flow: console → paste as typing → virtual media panel → diagnostics popover. Power and
  reset controls are never shown or clicked. Each step is a still screenshot with a fixed
  duration (no screen recording), assembled into a looping WebP with `img2webp` (libwebp:
  `brew install webp` / `apt install webp`). The script fails if a file exceeds 2 MB. ffmpeg is not
  needed: builds without libwebp cannot write animated WebP.
- Viewport, scale, language, time zone, browser clock and raster flags are fixed, so reruns on
  the same machine normally change images only when the UI changes (a few frames may differ by
  some anti-aliased pixels). Fonts come from the host: generate committed images on macOS so
  the Chinese UI uses PingFang SC.
- Before committing, open every generated image and check the language, that no error toast
  is visible, and that styles are intact.

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
| ESLint | Undefined/unused names and import resolution in the frontend modules | Runtime behavior (UI suites) |
| `cargo test` | Native unit/integration behavior | Real hardware |
| Installer/doctor scripts | `get.sh`, mDNS helpers, setup URL and `wingmankvm-doctor` against fixtures and fake commands | Real install, real avahi, real hardware |
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
