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

## 前端模块与 ESLint

浏览器端脚本是 `web/js/app/` 下的原生 ES 模块，没有打包器，也没有构建步骤：

- `src/web_ui.rs` 的 `APP_JS_MODULES` 在编译时逐个内嵌这些文件，服务端在
  `/assets/js/<name>` 原样提供（`text/javascript; charset=utf-8`、
  `Cache-Control: no-store`，与 `/assets/app.css` 一致），发布二进制仍然自包含。
  `index.html` 只加载 `<script type="module" src="/assets/js/main.js">`，其余模块由
  浏览器沿 `import` 获取；CSP 的 `script-src 'self'` 无需改动。
- 模块只用静态 `import { … } from './<name>.js'` 引用同级模块。被两个以上模块读写的
  可变状态放在 `state.js` 的 `shared` 对象里（导入的绑定在导入方是只读的），只在一个
  模块内使用的状态留作该模块的局部变量；不使用 `window` 全局变量
  （测试钩子 `window.__wingmanTest` 除外：只有测试在页面加载前设置了
  `window.__wingmanTestHooks = true`（`ui_harness.TEST_HOOKS`）时才会出现，正常页面上不存在）。
- DOM 事件绑定写在各模块导出的 `init()` 中，由 `main.js` 按原片段顺序调用，最后执行
  启动序列；模块求值时基本只声明函数和模块内状态。存在循环依赖时，模块的求值顺序不是
  文件名顺序，所以求值期间跨模块只能做两类事：
  - 读取不导入任何模块的 `00-core.js`、`00-settings.js`、`state.js` 的绑定：它们不在
    任何循环里，总是先于导入方求值。
  - 调用其他模块用 `function` 声明的函数（链接阶段已初始化，与求值顺序无关），且该函数
    不读取尚未求值的模块状态。例如 `06`、`11`、`12*` 在顶层调用 `latestOnly()`。

  其余 `const`/`let`/`class` 绑定此时可能还在暂时性死区，读取会抛 `ReferenceError` 并
  白屏：把 `latestOnly` 改成 `const` 箭头函数就会启动崩溃，UI 套件会报出错误和堆栈。
- 新增模块：在 `web/js/app/` 建文件、从使用方 `import`，并加入 `APP_JS_MODULES`。
  Rust 测试会检查：目录下每个文件都已内嵌、每个 `import` 都指向已内嵌的同级模块、
  所有模块都能从 `main.js` 到达。
- 新增样式片段：在 `web/css/app/` 建文件，并按文件名顺序加入 `src/web_ui.rs` 的
  `APP_CSS_PARTS`。`scripts/ui_bundle.py` 的 `css_parts()` 会比对两者，不一致时所有 UI
  套件直接失败，否则测试里样式正常而发布的二进制缺样式。

ESLint（flat config：`eslint.config.js`）只检查非风格问题：`no-undef`、
`no-unused-vars`、`no-import-assign`、`no-const-assign`、`no-func-assign`、
`no-redeclare`、`no-dupe-keys`、`no-unreachable`，以及本地规则
`local/import-exports`（`eslint-rules/import-exports.js`，只用 ESLint 自带的 espree：
`import` 必须是同级 `./<name>.js`、文件存在且确实导出所引用的名字，禁止动态 `import()`）。
版本固定在 `package.json` 与 `package-lock.json` 中，需要 Node.js 20.19+ / 22.13+ / 24+。

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
feed mock, `check()`, uncaught-error tracking and the JSON report). 页面来自 `scripts/ui_bundle.py`：它把样式表
内联进 `index.html`，`web/js/app/` 下的模块则按生产路径 `/assets/js/<name>` 提供，
所以浏览器执行的就是线上那份模块图。 A new suite only defines its API
mock and scenarios; add it to `SUITES` in `verify_ui_suite.py` and to `CASES`
in `verify_regressions.py` so CI runs it.

Headless Chromium may refuse pointer lock/fullscreen. p2b spies on
`requestPointerLock`/`requestFullscreen`: if the app never called them the suite
fails (`pointerlock_requested_by_app`, `fullscreen_requested_by_app`). Only when
the app asked and the browser refused is the follow-up recorded as
`"ok": null, "skipped": true`; the orchestrator counts those as explicit
non-failure skips, never as true passes. `--skip-ui` exists for local debugging
only and is recorded as skipped; CI always runs the UI gates.

- 未捕获的页面错误：套件用 `ui_harness.launch_browser()` 启动浏览器，任何页面的未捕获
  异常或未处理的 Promise 拒绝都会让套件失败，报告的 `error`/`page_errors` 带消息和堆栈，
  并增加检查项 `no_uncaught_page_errors`。上下文级 init 脚本也会在初始 `about:blank` 上
  运行，在那里访问 `localStorage` 会抛错，需先判断 `location.protocol`。
- 接口模拟：各套件的默认 API 模拟对未知路径返回 404，不再是一律 200。应用可能吞掉这个
  404，所以 `ui_harness.answer_unmocked()` 还会记录路径并让套件失败（检查项
  `no_unmocked_api_requests`），接口改名或新增接口不会被悄悄放过。新增页面请求时要在模拟里
  按服务端的返回结构补上。
- 桌面输入（`verify_audit2_ui.py` 的 `desktop_*` 场景）：用真实的 `page.mouse` 和点击
  断言发往 HID 接口的载荷：fit 模式已知点的绝对坐标（±1，含黑边）、左右中键、滚轮符号与
  ±127 钳位、相对移动拆包、每个 `[data-quick-key]`、虚拟键盘粘滞修饰键。无头浏览器拒绝
  指针锁定时桩接 `requestPointerLock`，且只有应用自己发起请求才会“锁定”；真实指针锁定
  仍需有界面的浏览器或真机验证。场景不涉及电源和复位控件。

## README 截图与演示动图（#96）

界面改动后，一条命令重新生成 `docs/images/` 下全部截图和 README 顶部的中英文演示动图
（`demo-zh.webp`、`demo-en.webp`）：

```bash
python3 scripts/capture_screenshots.py --browser-channel chrome
# 只生成截图 / 只生成动图：加 --skip-demo / --demo-only
# 单独调试动图并保留逐帧 PNG：
python3 scripts/capture_demo.py --lang en --output-dir "$SCRATCH/demo" --keep-frames "$SCRATCH/demo/frames"
```

- 与 UI 门禁一样用 `scripts/ui_harness.py` 提供真实界面，所有 `/api/**`、键鼠 WebSocket 和
  `/video_feed` 都由 `scripts/capture_fixtures.py` 模拟，画面是合成的终端帧，不连接任何设备。
- 动图流程：控制台 → 粘贴为键入 → 虚拟介质面板 → 诊断弹层；不展示也不点击电源/复位按钮。
  每一步是固定时长的静态截图（不录屏），再用 `img2webp`（libwebp，`brew install webp` /
  `apt install webp`）合成循环 WebP；单个文件超过 2 MB 时脚本报错。不依赖 ffmpeg：未编入 libwebp 的 ffmpeg 无法输出动画 WebP。
- 视口、缩放、语言、时区、浏览器时钟和渲染参数都已固定，同一台机器重跑通常只在界面变化时
  改变图片（个别帧可能有少量抗锯齿差异）。字体取自本机：请在 macOS 上生成要提交的图片，
  中文界面才会使用 PingFang SC。
- 提交前请逐张打开生成的图片检查：界面语言正确、没有错误提示、样式完整。

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
| ESLint | 前端模块的未定义/未使用名字、import 解析 | 运行时行为（由 UI 套件覆盖） |
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
