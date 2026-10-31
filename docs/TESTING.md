# Testing and regression gates

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

## One-line installer gate (no network, no root)

`scripts/verify_get_script.py` builds a fake release, serves it from a loopback
HTTP server, and also exercises a `file://` base. It checks latest-version
discovery, explicit `--version`, SHA256 verification, `--download-only`,
unsupported-architecture handling, and temporary-directory cleanup. It never
contacts GitHub and never runs the real installer.

```bash
python3 scripts/verify_get_script.py
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

## 延迟测量工具的含义（#94）

诊断面板的“延迟测量”只测**键鼠回环**，不测画面延迟。

- 每个样本：浏览器发出 Caps Lock 按键请求起，到浏览器通过 `GET /api/hid/leds`
  读到 LED 翻转为止。覆盖浏览器 → 服务 → USB gadget → 被控机系统 → LED 输出报告 →
  服务 → 浏览器，不经过 HDMI 采集、编码、解码或显示。轮询每次都是一个 HTTP 往返，
  结果偏大最多约一次“LED 轮询耗时”。
- 统计：最小值、中位数、P95（最近秩法）只计入看到翻转的样本；“有效样本”为
  `看到翻转 / 已发送`。一次运行 10 个样本，样本太少时 P95 接近最大值。
- 码率只是上下文：H.264 显示实测接收码率（需已开启质量浮层或自适应码率）和目标码率；
  MJPEG 浏览器拿不到字节数，显示不可用。这些不代表延迟和码率有因果关系。
- 画面端到端延迟：不在被控机安装软件时没有可靠办法——需要被控机屏幕显示可识别的
  时间戳，所以界面明确写“未测量”，也不提供估算。Caps Lock 的屏幕提示因系统而异，
  不能作为通用信号。需要该数字时，可用外部高速相机同时拍下本地操作和浏览器画面。

仅在用户点击并确认后才发送按键。结束或停止后按 LED 状态恢复 Caps Lock；连续 2 次
没有 LED 回报时中止（有的系统不回报，例如部分 BIOS；macOS 可能忽略很短的 Caps Lock
按下）。此时“LED 已回到原状态”只说明 LED 读数，被控机实际状态无法确认。键入或宏回放
期间拒绝开始；页面转入后台时停止，避免定时器节流污染样本。

真机上需要验证：Caps Lock 回报、恢复行为、WebSocket/HTTP 通道下的数值。

## Baseline provenance

The scripts encode checks derived from earlier issue-specific fixes. This
document references those existing scripts and their assertions; it does not
claim that any old baseline commit or test suite was rerun here.
