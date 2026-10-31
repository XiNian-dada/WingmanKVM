# Fuck My Shit Mountain Audit Report

**Project:** WingmanKVM
**Audit mode:** full
**Date:** 2026-10-31
**Reviewer:** Claude Opus 5.5（主审）+ opencode-go/deepseek-v4.1-flash（4 路辅助审查，所有采纳结论均经主审对照源码复核）

---

## 1. Executive Summary

WingmanKVM 是一个运行在 RK3399 等 ARM 板上的 KVM-over-IP 服务：Rust/axum 后端负责认证、视频采集（MJPEG / WebRTC H.264）、USB HID 键鼠、虚拟介质、GPIO 电源和网页终端，前端是内嵌在 `src/web_ui.rs` 中的单页应用。上一轮审计的 21 个问题已全部关闭，核心安全基线（SameSite=Strict + HttpOnly 会话、非 GET 请求 Origin 校验、终端 WS 严格来源校验、登录限流与全局密码槽、原子配置写入、上传临时文件发布）都已落实，`cargo test` 222 项全部通过，CI 使用固定 SHA 的 action 与最小权限。

本轮共发现 36 个问题（4 High / 12 Medium / 17 Low / 3 Info），没有 Critical。最大的风险集中在三类：一是**可用性边界**——启用 PPv2 后单条慢连接即可串行阻塞整个 accept 循环，前端"偏好设置"里的 PPv2 下拉框改动会立即持久化并生效，误选"强制要求"会让所有直连客户端瞬间失联；二是**前端交互正确性**——键盘转发的焦点模型错误（点按钮后物理键被吞、Enter 重放上一个屏幕键）、视频设置面板多个控件未接线、HID 错误默认被静默吞掉、注销失败仍显示登录页；三是**会话生命周期缺口**——已删除的 Passkey 在挑战有效期内仍可登录、`/video_feed` 不随注销断流。

结构上，两个巨型文件是最突出的可维护性债务：`src/web.rs` 5460 行（约 3400 行生产代码，承担路由、认证、Passkey、配置、上传、终端、GPIO 等全部 HTTP 职责），`src/web_ui.rs` 2655 行（CSS/HTML/JS 全部塞在一个 Rust raw string 中，49 行 JS 超过 400 字符、最长 2667 字符，无法 lint/格式化/单测）。前端还存在同一信息多处重复渲染（主控机性能 3 处、键盘 LED 2 处、电源操作 2 处）、i18n 只覆盖约一半文案、信息架构混乱（Passkey 放在"控制"面板、主控机性能放在"视频"面板、服务端监听配置放在本地偏好弹窗）。总体评级 B（6.1），优先级建议：先修 4 个 High 和会话类 Medium，再按计划拆分前端与 `web.rs`。

### Score Dashboard

```
Security        ██████░░░░  6.5  B   核心认证扎实，但 PPv2 accept 阻塞、Passkey 撤销绕过、MJPEG 不随注销断流、安装器 root 导入劫持
Stability       ██████░░░░  6.0  B   单帧转码失败拆会话、WebRTC 槽位 panic 泄漏、终端 stop 无限等待、HID 错误静默吞掉
Performance     ████████░░  7.5  A   热路径基本合理；HID 每报告重开设备、auth.json 同步 IO 在 worker 上属局部问题
Testing         ██████░░░░  6.0  B   Rust 222 项真实通过，但前端 Rust 测试仅 contains 断言，Playwright 未覆盖接线/焦点缺陷
Maintainability █████░░░░░  4.5  C   web.rs 5460 行、web_ui.rs 2655 行单字符串、超长压缩行、UI 多处重复、i18n 半覆盖
Design          ██████░░░░  5.5  B   SRP 违背集中于两大文件；fail-fast 缺口（bootstrap/discovery 吞错）；IA 职责错位
Release         ███████░░░  7.0  A   事务式升级与回滚完善，但提交后 trap 仍可回滚、recover.sh 忽略 COMMITTED、dtls vendor 未校验
─────────────────────────────────────
Overall         ██████░░░░  6.1  B
```

Each dimension scored 0.0–10.0. **Higher = better (10 = clean, 0 = shit mountain).** Scores are judgment-based, not formula-based.

### Finding Statistics

| Severity | Count | Confirmed | Suspected |
|----------|-------|-----------|-----------|
| Critical | 0 | 0 | 0 |
| High | 4 | 4 | 0 |
| Medium | 12 | 11 | 1 |
| Low | 17 | 16 | 1 |
| Info | 3 | 2 | 1 |
| **Total** | **36** | **33** | **3** |

## 2. Project Map

- **入口与初始化**：`src/main.rs`（`#[tokio::main]`）→ `AppState::load()`（`src/web.rs:114`，加载配置、AuthStore、生成初始化令牌、创建 Video/WebRTC/Power/HID/Media 管理器）→ `ProxyProtocolListener`（`src/proxy_protocol.rs`，可选 PPv2）→ `router()`（`src/web.rs:195`）。
- **HTTP 边界**：公开路由（首页、静态资源、`/healthz`、`/api/bootstrap`、`/api/setup*`、`/api/login`、Passkey 登录）与受保护路由（`require_auth` 中间件：会话校验 + 非 GET 的 Origin/Host 一致性）。
- **状态归属**：配置 `Arc<RwLock<Config>>` + `config_update` 互斥串行化，持久化于 `config.json`（原子写）；认证记录 `auth.json`（`src/auth.rs`，原子写 + fsync）；会话为内存 `SessionStore`（12h TTL，`SessionGuard` 驱动 WS/WebRTC 撤销）。
- **设备层**：`src/devices/video.rs`（V4L2 采集 → watch 通道）、`webrtc.rs`（FFmpeg 子进程编码 + webrtc-rs）、`hid.rs`（键盘/鼠标 worker，64 深度队列）、`media.rs`（configfs LUN）、`power.rs`（`gpioset` 子进程）、`discovery.rs`（设备扫描）。
- **前端**：`src/web_ui.rs` 单个 `INDEX_HTML` raw string（CSS 22–389，HTML 390–770，JS 773–2424，Rust 测试 2426 起），内嵌 i18n 字典（zh-CN/zh-TW/en）、视频播放（MJPEG/WebRTC）、键鼠捕获、xterm 终端、设置与介质管理。
- **部署/发布**：`deploy/install.sh` + `deploy/upgrade-transaction.sh`（事务式安装与回滚）、`wingmankvm-gadget`（USB Gadget 配置）、systemd 单元（`User=wingmankvm`、`ProtectSystem=strict`）、sudoers 规则；CI `.github/workflows/regression.yml`。
- **AI/LLM 面**：无。**隐私数据**：管理员账号、Argon2 密码哈希、Passkey 公钥、客户端 IP（仅内存/日志）。
- **高风险区域**：PPv2 accept 路径、Passkey 流程、会话绑定的长连接、前端键盘捕获、安装器 root 执行段、两大单文件。

覆盖说明：已检查全部 first-party Rust 源码（`src/**`）、前端内嵌资源、`deploy/**`、`.github/workflows/**`、`scripts/*.py`、`docs/**`、`README.md`、`Cargo.toml`。排除 `target/`、`vendor/`（仅检查来源清单与 CI 校验）、`web/vendor/xterm`、`.playwright-cli/` 快照、二进制资产。执行：`cargo test --locked`（222 passed）、`cargo test -p dtls`（63 passed）、`cargo clippy --all-targets`（0 error / 22 warning）、`cargo audit --no-fetch`（通过）、`verify_xterm_vendor.py`（PASS）、前端脚本 `node --check` 与仓库外 Chromium 最小复现。未在真实 RK3399 硬件上运行（遵守测试机红线）。

### Coverage Matrix

| Dimension | Coverage | Evidence inspected | Exclusions / limits |
|-----------|----------|--------------------|---------------------|
| Architecture | High | 全部 src 模块依赖与状态归属、router 结构 | — |
| Security | High | 认证/会话/Origin/Passkey/PPv2/终端/上传/安装器 | 未做真实网络渗透 |
| Stability | High | 设备 worker、子进程生命周期、超时、错误传播 | 未在真实硬件压测 |
| Performance | Medium | 视频/HID 热路径、同步 IO、扫描 | 无运行时 profiling |
| Testing | High | 222 Rust 测试、Python/Playwright 脚本、CI | — |
| Maintainability | High | 文件规模、函数长度、重复、命名 | — |
| Design | High | 原则对照 rubrics/principles.md | — |
| Release | High | install/upgrade/recover 脚本、CI、版本 | 未在真实设备执行升级 |
| Documentation | Medium | README、DEPLOYMENT、RK3399_GUIDE、TESTING 抽样对照源码 | 未逐句核对全部文档 |
| Configuration | High | validate_config、patch 语义、PPv2/public_origin | — |
| Observability | Medium | tracing 使用、/healthz、错误映射 | 无生产日志样本 |
| Data Integrity | High | 原子写、配置并发、上传发布、事务标记 | — |
| Privacy | Medium | 存储字段、日志内容 | 无数据保留策略文档可对照 |
| Accessibility | Medium | 语义、焦点、键盘、ARIA（静态 + Chromium 复现） | 未用屏幕阅读器实测 |
| Supply Chain | High | Cargo.lock、vendor 清单、CI action 固定、cargo audit | advisory DB 为本地缓存 |
| Cost | Medium | 资源上限、扫描/挑战表/流并发 | 无云成本面 |
| AI-Safety | Not assessed | 全库无 LLM/模型调用 | 不适用 |
| Fallback | High | unwrap_or_default/ok()/catch 吞错全文检索 | — |
| Testing Authenticity | High | 测试是否触达真实行为 | — |
| Type Safety | Medium | serde 边界、字符串类型、unsafe 检索 | 未逐个审查 vendor |
| Frontend State | High | web_ui.rs 状态、请求代次、重复渲染 | — |
| Backend API | High | 全部路由、校验、错误结构 | — |
| Dependency Weight | Medium | Cargo.toml 依赖用途 | 未测量二进制体积 |
| Code Consistency | High | 两大文件风格、压缩行、命名 | — |
| Comment Coverage | Medium | 模块/函数注释抽样 | — |

## 3. Top Risks

1. **[High] PPv2 握手在 accept 内串行执行** — 启用 PPv2 时单条慢连接可让整个 HTTP 监听停摆。
2. **[High] 偏好设置中的 PPv2 下拉即时生效且无确认** — 误选"强制要求"立即锁死所有直连访问，只能 SSH 修复。
3. **[High] 键盘转发焦点模型错误** — 点击按钮后物理键被吞，Enter/Space 会重放上一个屏幕键或关闭转发。
4. **[High] 安装器以 root 执行 `python3 -`** — 当前目录下的 `json.py` 等会以 root 身份被导入。
5. **[Medium] 已删除 Passkey 仍可在 120 秒内登录** — 凭据撤销不生效。
6. **[Medium] `/video_feed` 不绑定会话** — 注销/过期后 MJPEG 继续推送屏幕。
7. **[Medium] Passkey 登录 start 不记账且挑战表无上限** — 未认证请求可放大内存与 CPU。
8. **[Medium] 事务提交后仍保留信号 trap** — Ctrl-C 会回滚已成功的升级。
9. **[Medium] 视频设置控件未接线** — 码率/缩放/插值/质量调整无效或提交旧值。
10. **[Medium] HID 错误默认静默吞掉** — 键鼠失效时用户没有任何反馈。
11. **[Medium] 前端单文件巨石与 UI 重复** — 2655 行字符串、同一信息 2–3 处渲染。
12. **[Medium] `web.rs` 5460 行多职责** — 任何 HTTP 改动波及整个文件。

## 4. Detailed Findings

### Finding: AUD2-01 PPv2 握手在 accept 路径串行执行且无总时限

- Severity: High
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: `src/proxy_protocol.rs` ProxyProtocolListener
- Evidence:
  - File: src/proxy_protocol.rs:299-394
  - Function / Module: `ProxyProtocolListener::process_stream` / `Listener::accept`
  - Relevant behavior: `accept()` 中直接 `await process_stream()`；读循环每次 `read` 单独 5s 超时，无总时限，只要声明的 payload ≤ 4096 就不会触发 HeaderTooLarge。
- Problem: 握手完成前 accept 不会返回下一条连接；攻击者发送 16 字节前缀后每 4.9 秒滴 1 字节即可让握手持续数小时，即使只建连不发数据也每连接阻塞 5 秒。
- Why it matters: axum serve 循环串行调用 accept，握手阻塞等于整个服务不可用。
- Realistic failure scenario: 管理网内一台被入侵设备对启用 PPv2 的端口发起慢速连接，内核 backlog 填满后 Web 控制台与 KVM 全部无法访问。
- Minimal fix: 把 `process_stream` 包进总时限 `timeout`，并将握手移出 accept：accept 立即返回，在独立任务中完成握手后通过 channel 交付（或用信号量限制并发握手）。
- Better long-term fix: 实现"握手中连接池"模式，accept 永不等待单条连接的应用层数据。
- Regression test suggestion: 连接 A 发送不完整 PPv2 头后停滞，断言连接 B 在 1 秒内被 accept；`tokio::time::pause` 下断言滴流连接在总时限内被拒。
- Estimated effort: 0.5–1 天

### Finding: AUD2-02 偏好设置弹窗中的 PPv2 下拉即时生效且无确认

- Severity: High
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端设置弹窗 / 配置提交
- Evidence:
  - File: src/web_ui.rs:741-751, src/web_ui.rs:2384-2400, src/web.rs:2935-2958
  - Function / Module: `#setting-proxy-protocol` change 处理器 / `commit_config`
  - Relevant behavior: `change` 事件立即 `POST /api/config {server:{proxy_protocol}}`，服务端持久化后 `proxy_protocol_updates.send_replace()` 让监听器立刻切换模式；`required` 模式拒绝所有无 PPv2 头的连接。
- Problem: 这是改变服务端监听行为的高危操作，却放在"全局偏好设置"（其他项均为本地浏览器偏好）里，用一个下拉框直接生效，没有二次确认、没有回滚窗口。
- Why it matters: 一次误点即可让所有直连客户端（包括当前操作者自己）立即失联，且配置已持久化，重启也无法恢复。
- Realistic failure scenario: 用户以为这是本地显示选项，选择"强制要求"；下一次请求起浏览器全部连接失败，只能 SSH 登录板子手改 `config.json`。
- Minimal fix: 把 PPv2 移到服务端"网络"配置区；切换到 `auto/required` 前弹确认框说明后果；服务端增加"未在 N 秒内收到有效 PPv2 连接则自动回退"或前端提交后用新连接探测、失败则提示恢复方式。
- Better long-term fix: 区分"本地偏好"与"服务端配置"两类设置 UI；对监听相关配置采用"试用—确认"两阶段提交。
- Regression test suggestion: Playwright：选择 required 时必须出现确认框，取消后不发请求；Rust：两阶段提交未确认时超时回退到原模式。
- Estimated effort: 0.5–1 天

### Finding: AUD2-03 键盘转发焦点模型错误，物理键被吞、Enter 重放屏幕键

- Severity: High
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端键鼠捕获
- Evidence:
  - File: src/web_ui.rs:1622, src/web_ui.rs:1646-1649, src/web_ui.rs:1715, src/web_ui.rs:1788
  - Function / Module: `interactiveTarget()`、document `keydown`、`#input-state` click、虚拟键盘按钮
  - Relevant behavior: `interactiveTarget` 把所有 `button` 视为本地交互，keydown 直接 return；点击 `#input-state` 开启转发后焦点仍在该按钮，Chromium 实测按字母键 0 次 `/api/key`，按 Enter 触发按钮再次切换把转发关闭；点击屏幕键 `A` 后按物理 Enter 会再次激活该按钮，发送的是 `A`。
- Problem: 转发状态显示"正在转发"，但实际键盘不工作或发送错误按键。
- Why it matters: KVM 最核心的输入链路在常见操作路径上失效，并可能向被控机发送错误字符。
- Realistic failure scenario: 用户在 BIOS/root shell 中点"开启转发"后输入命令无反应，按 Enter 反而关闭转发；用屏幕键盘输入后按回车，被控机收到的是上一个字符，可能在分区工具中确认错误选项。
- Minimal fix: 鼠标激活转发按钮/工具栏按钮/虚拟键后把焦点交回 `#video-viewport`；虚拟键盘按钮设为 `tabindex=-1` 并在 `pointerdown` 时 `preventDefault` 防止获得焦点。
- Better long-term fix: 只在 viewport（或 body）持有焦点时转发键盘，把"是否转发"做成明确的焦点状态机。
- Regression test suggestion: Playwright：点击 `#input-state` 后 `activeElement.id==='video-viewport'` 且按 `a` 产生 `KeyA` 请求；点屏幕键 A 后按 Enter，断言发送 Enter。
- Estimated effort: 1 天

### Finding: AUD2-04 安装器以 root 执行 `python3 -`，当前目录可劫持标准库导入

- Severity: High
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: `deploy/install.sh`
- Evidence:
  - File: deploy/install.sh:334, deploy/install.sh:380, deploy/install.sh:418
  - Function / Module: `resolve_health_url` / `probe_health`
  - Relevant behavior: 三处 `python3 - <<'PY'`；`-` 模式下 `sys.path[0]` 为当前目录，安装器不 `cd`，且已要求 root。
- Problem: 当前工作目录中的 `json.py`、`urllib.py`、`sitecustomize.py` 会先于标准库被 root 导入执行。
- Why it matters: 本地低权限用户可借此提权到 root。
- Realistic failure scenario: 管理员在 `/tmp` 解压发布包并执行 `sudo ./deploy/install.sh`；其他用户预先放置 `/tmp/json.py`，健康检查时以 root 执行任意代码。
- Minimal fix: 三处改为 `python3 -I -`（隔离模式，移除 CWD 与用户 site）。
- Better long-term fix: 安装器开头 `cd /` 并统一封装 Python 调用。
- Regression test suggestion: harness 在临时 CWD 放置写 marker 的 `json.py`，运行健康检查函数，断言 marker 不存在。
- Estimated effort: 0.5 小时

### Finding: AUD2-05 已删除的 Passkey 仍可在挑战有效期内完成登录

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: Passkey 登录
- Evidence:
  - File: src/web.rs:1585-1591, src/auth.rs:292-302
  - Function / Module: `passkey_login_finish` / `update_passkey_credential`
  - Relevant behavior: `update_passkey_credential` 找不到凭据时返回 false，但调用处丢弃返回值，随后无条件 `sessions.create()`。
- Problem: WebAuthn 校验使用 start 时快照的凭据集，删除凭据不会使已发出的挑战失效。
- Why it matters: 凭据撤销是丢失设备时的关键补救手段。
- Realistic failure scenario: 管理员发现设备丢失并删除对应 Passkey，持有设备者在删除前已调用 start，120 秒内提交断言仍获得会话。
- Minimal fix: 闭包在 `update_passkey_credential` 返回 false 时返回错误，handler 返回 401 且不创建会话。
- Better long-term fix: 删除 Passkey 时同时清理相关挑战并可选撤销全部会话。
- Regression test suggestion: 单测 `update_passkey_credential` 对不存在凭据返回 false；handler 层注入删除后的记录断言 401。
- Estimated effort: 1–2 小时

### Finding: AUD2-06 `/video_feed` MJPEG 流不绑定会话，注销后继续推送

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 视频流端点
- Evidence:
  - File: src/web.rs:2676-2705
  - Function / Module: `video_feed`
  - Relevant behavior: 会话只在中间件校验一次，流内仅 `frames.changed().await`，无 `SessionGuard`；对照 `webrtc_offer`（2707）与终端 WS 均持有 guard。
- Problem: 注销、过期、`revoke_all` 后已建立的 MJPEG 流持续输出屏幕帧。
- Why it matters: 与已修复的"注销断开终端/WebRTC"策略不一致，屏幕内容可能包含敏感信息。
- Realistic failure scenario: 共享电脑上管理员退出登录，但另一个标签页/被盗 cookie 打开的 `/video_feed` 仍实时显示被控机画面。
- Minimal fix: `validate_guard` 获取 guard，流中 `select!` 于 `frames.changed()` 与 `guard.wait_for_end()`。
- Better long-term fix: 对所有长连接统一一个"会话绑定流"辅助函数，并限制并发流数量。
- Regression test suggestion: HTTP 测试打开流后 `sessions.revoke(token)`，断言 1 秒内 body 结束。
- Estimated effort: 2–4 小时

### Finding: AUD2-07 Passkey 登录 start 不消耗限流额度，挑战表无上限

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: Passkey 登录
- Evidence:
  - File: src/web.rs:1477-1521, src/web.rs:3400-3413
  - Function / Module: `passkey_login_start` / `LoginLimiter::has_capacity`
  - Relevant behavior: start 只调用不记账的 `has_capacity`；每次请求插入挑战，插入前对整表 `retain`（O(n) 持锁），表无大小上限，TTL 120s。
- Problem: 未认证客户端可无限制创建挑战。
- Why it matters: 内存随请求速率线性增长，插入成本随表大小增长，RK3399 上易拖垮运行时。
- Realistic failure scenario: 注册过 Passkey 的实例被脚本以每秒数百次调用 start，数分钟后内存与 CPU 飙升、登录页卡顿。
- Minimal fix: start 增加独立记账限流（每 IP 每分钟 N 次）；挑战表设硬上限（满时拒绝）。
- Better long-term fix: 挑战表改为按到期时间排序的有界结构，后台统一清理。
- Regression test suggestion: 同一 IP 超预算调用 start 返回 429；连续插入后表长不超过上限。
- Estimated effort: 2–4 小时

### Finding: AUD2-08 事务提交后仍保留 INT/TERM/HUP trap，信号会回滚已提交安装

- Severity: Medium
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: `deploy/upgrade-transaction.sh`
- Evidence:
  - File: deploy/upgrade-transaction.sh:122-129, deploy/upgrade-transaction.sh:418-434
  - Function / Module: `_tx_on_signal` / `tx_commit`
  - Relevant behavior: `tx_commit` 只设 `TX_STATE=committed`，不移除 trap；`_tx_on_signal` 不检查状态，无条件 `_tx_rollback`。
- Problem: 提交后到脚本退出前收到信号会把托管文件恢复为旧版本，同时出现 `COMMITTED` 与 `ROLLED_BACK` 两个标记。
- Why it matters: 成功升级被部分回滚，下次重启运行旧版本，状态互相矛盾。
- Realistic failure scenario: 安装器打印首次访问地址/查询 journal 时管理员按 Ctrl-C 或 SSH 会话超时。
- Minimal fix: 提交后立即 `trap - INT TERM HUP`；`_tx_on_signal` 与 `_tx_rollback` 在 `committed` 状态下直接返回。
- Better long-term fix: 为事务状态机增加显式状态转移检查函数。
- Regression test suggestion: `verify_p2c_upgrade.py` 提交后发送 TERM/INT，断言文件仍为新字节且无 `ROLLED_BACK`。
- Estimated effort: 1 小时

### Finding: AUD2-09 视频设置面板多个控件未接线

- Severity: Medium
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端视频设置
- Evidence:
  - File: src/web_ui.rs:621-634, src/web_ui.rs:1226-1239
  - Function / Module: `syncH264Preset()` / `#video-config`
  - Relevant behavior: 只为 `resolution_preset`、`fps_preset` 注册 change；`h264_bitrate_preset` 无监听，选择 12 Mbps 后隐藏字段仍为旧值，选"自定义"不显示输入框；`display_scale`、`rendering` 无监听也不进入 payload；JPEG 质量 `<output>` 拖动时不更新；scale/rendering 偏好写入后从不读取。
- Problem: 用户的设置变更被静默忽略或以旧值提交。
- Why it matters: 用户以为码率已改为 12 Mbps，服务端仍是 4 Mbps；显示选项完全无效。
- Realistic failure scenario: 带宽不足时把码率降到 2 Mbps 并应用，实际仍按旧码率推流，画面持续卡顿。
- Minimal fix: 为上述控件注册 change/input 监听；提交前再同步一次；启动时读取显示偏好。
- Better long-term fix: 表单与状态采用单一数据模型（schema 驱动），自动绑定所有字段。
- Regression test suggestion: Playwright 选择 12000/自定义断言 payload `bitrate_kbps`；选择 native/smooth 断言 console class 变化并在重载后保留。
- Estimated effort: 0.5 天

### Finding: AUD2-10 `<summary>` 不在交互白名单，Enter/Space 被转发到被控机

- Severity: Medium
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端键鼠捕获 / 可访问性
- Evidence:
  - File: src/web_ui.rs:1622, src/web_ui.rs:1715-1716
  - Function / Module: `interactiveTarget()`
  - Relevant behavior: 白名单 `input,textarea,select,button,a,[contenteditable],dialog` 不含 `summary`；Chromium 实测焦点在 summary 上按 Enter，`details` 不展开且发出 `{key:"Enter"}`。
- Problem: 纯键盘用户无法展开 5 个折叠面板，且按键被误发到被控机。
- Why it matters: 误发 Enter 可能确认被控机上的危险对话框。
- Realistic failure scenario: 用户 Tab 到"手动配置"按 Enter，被控机上正弹出的"确认格式化"被确认。
- Minimal fix: 白名单加入 `summary,[role=button],[role=tab]`。
- Better long-term fix: 同 AUD2-03，仅在 viewport 聚焦时转发。
- Regression test suggestion: Playwright focus summary 按 Enter，断言 `details.open===true` 且无 `/api/key` 请求。
- Estimated effort: 1 小时

### Finding: AUD2-11 注销请求失败时仍切换到登录页

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 前端注销
- Evidence:
  - File: src/web_ui.rs:1170
  - Function / Module: `#logout` click
  - Relevant behavior: `try { await request('/api/logout') } finally { ...; showState('login') }`，即使重新获取的 bootstrap 显示仍已认证也展示登录页；Cookie 为 HttpOnly，前端无法清除。
- Problem: UI 伪造"已退出"状态，服务端会话仍有效 12 小时。
- Why it matters: 共享/机柜终端上下一个人刷新即可恢复控制台。
- Realistic failure scenario: 网络抖动时点击退出，页面显示登录框，管理员离开；下一个人刷新后直接进入 KVM。
- Minimal fix: 仅在注销成功或 401 后切换登录页；失败时保留控制台并提示"退出失败，请重试"。
- Better long-term fix: 注销后以 bootstrap 结果作为唯一事实来源决定视图。
- Regression test suggestion: mock `/api/logout` 返回 500，断言仍在控制台并出现错误提示。
- Estimated effort: 1 小时

### Finding: AUD2-12 HID 错误默认被静默吞掉

- Severity: Medium
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端错误处理
- Evidence:
  - File: src/web_ui.rs:1585, src/web_ui.rs:1606-1620, src/web_ui.rs:705
  - Function / Module: `isHidWritableTimeoutError` / `handleHidError`
  - Relevant behavior: `suppress_hid_timeout` 默认 true；消息包含 `timeout`、`timed out`、`超时`、`未就绪` 任一关键字即直接 return；同时 5 处 `.catch(()=>{})`。
- Problem: 字符串关键字匹配范围过宽，"设备未就绪"等真实故障也被吞掉，而且默认开启。
- Why it matters: 键鼠失效时用户完全没有反馈，排障方向被带偏（silent fallback）。
- Realistic failure scenario: Gadget 未绑定导致 HID 返回"未就绪"，用户反复打字无反应也无提示，以为是被控机死机。
- Minimal fix: 默认关闭屏蔽；仅按 HTTP 504 状态码识别写超时并节流提示；不屏蔽 503/未就绪；状态栏常驻显示"键鼠写入失败"计数。
- Better long-term fix: 服务端返回结构化错误码（`code: "hid_timeout"`），前端按码处理。
- Regression test suggestion: mock `/api/key` 返回 503"未就绪"，断言出现错误提示；返回 504 时节流提示。
- Estimated effort: 2 小时

### Finding: AUD2-13 前端单文件巨石与 UI 结构性重复

- Severity: Medium
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: `src/web_ui.rs`
- Evidence:
  - File: src/web_ui.rs:1-2655
  - Function / Module: `INDEX_HTML`
  - Relevant behavior: CSS/HTML/JS 共 2424 行存于一个 Rust raw string；JS 49 行超过 400 字符（1435/1967/1968 行均 >2000 字符）；主控机性能在顶栏弹层（489-504）、诊断弹层（516-521）、视频面板（587-612）三处渲染；键盘 LED 在顶栏（532）与控制面板（556-560）两处；电源操作在顶栏菜单（459-464）与控制面板（563-567）两处；设置入口两处；22 处内联 `style=`；`data-i18n` 仅 108 处，字段标签与全部帮助文案硬编码中文，切到 English 后面板大半仍为中文；标题混用 "Streaming"/"Viewer"。
- Problem: 无法 lint、格式化、类型检查或单元测试；同一状态多处渲染导致 AUD2-09 类接线遗漏；信息架构错位（Passkey 在"控制"面板，性能在"视频"面板，服务端监听配置在本地偏好弹窗）。
- Why it matters: 每次前端改动都需要在超长压缩行中定位，回归风险高，评审几乎不可能。
- Realistic failure scenario: 新增一个设置字段，只在一处渲染更新，另一处显示旧值；或 i18n 漏键导致英文界面混杂中文。
- Minimal fix: 把前端拆到 `web/` 目录（`index.html`、`css/*.css`、`js/*.js` 按功能分模块），Rust 侧以 `include_str!` 嵌入保持单二进制部署；合并重复 UI 区块为单一组件。
- Better long-term fix: 重新设计信息架构（控制 / 视频 / 设备 / 介质 / 账户与安全 / 系统网络），统一 i18n 覆盖，引入 `node --check` + Playwright 冒烟进入 CI。
- Regression test suggestion: CI 执行 JS 语法检查与 Playwright 冒烟；i18n 测试断言三语字典键集合一致且 HTML 中无未登记的 `data-i18n`。
- Estimated effort: 3–5 天

### Finding: AUD2-14 `src/web.rs` 5460 行承担全部 HTTP 职责

- Severity: Medium
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: `src/web.rs`
- Evidence:
  - File: src/web.rs:1-3428（生产代码），src/web.rs:3429-5460（测试）
  - Function / Module: 路由、终端 PTY、首次设置与设备选择、系统密码同步、登录限流、Passkey、配置 patch 与校验（`validate_config` 约 190 行）、HID/电源/介质/上传/WebRTC handler、错误映射
  - Relevant behavior: 一个模块内约 150 个函数，跨 10+ 个领域。
- Problem: SRP 违背（principle 1.1）与文件规模超限，任何领域改动都需理解整个文件。
- Why it matters: 冲突频繁、评审困难，测试也集中在同一个 2000 行 `mod tests`。
- Realistic failure scenario: 两个并行分支分别修改上传和 Passkey，在同一文件产生大量合并冲突，误合并引入回归。
- Minimal fix: 拆为 `src/web/{mod,router,error,session,terminal,setup,password_sync,passkey,config_api,input,power,media,video}.rs`，纯移动不改行为，每步 `cargo test` 通过。
- Better long-term fix: 抽出 `ConfigService` 等领域服务，handler 只做参数提取与错误映射。
- Regression test suggestion: 拆分前后 `cargo test` 测试数量与结果一致；新增各模块就近测试。
- Estimated effort: 1–2 天

### Finding: AUD2-15 WebRTC 会话槽位在 supervisor 异常终止时永久泄漏

- Severity: Medium
- Confidence: Medium
- Category: Stability
- Status: Suspected
- Affected area: `src/devices/webrtc.rs`
- Evidence:
  - File: src/devices/webrtc.rs:786-843
  - Function / Module: `run_session_supervisor`
  - Relevant behavior: 会话先插入 `sessions` 再 spawn supervisor；唯一清理点在函数尾部（remove、peer.close、closed.send_replace），无 RAII guard，JoinHandle 被丢弃。
- Problem: 若 encoder/webrtc-rs 调用 panic，清理全部跳过（触发条件未实测，故为 Suspected）。
- Why it matters: `max_sessions` 默认 1，一个幽灵条目即让所有后续 offer 返回 Capacity。
- Realistic failure scenario: 某次 webrtc-rs 内部 panic 后，WebRTC 永久不可用，只能重启服务。
- Minimal fix: 把尾部清理放入实现 `Drop` 的 guard 结构。
- Better long-term fix: 统一会话生命周期对象，所有退出路径（含 panic）走同一清理。
- Regression test suggestion: 注入 panic 的 encoder future，断言 `inner.sessions` 已清空且 `closed==true`。
- Estimated effort: 1–2 小时

### Finding: AUD2-16 未初始化窗口内 `/api/setup/devices` 可无限并发触发全量扫描

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 设备扫描
- Evidence:
  - File: src/web.rs:1817-1822, src/devices/discovery.rs:140-179
  - Function / Module: `scan_setup_devices` / `discovery::scan`
  - Relevant behavior: 未初始化时无需认证；每次扫描多次 `spawn_blocking` 并逐个打开 `/dev/video*` 做 ioctl；无并发上限、缓存或限流。
- Problem: 同网段任何人可在初始化完成前反复触发重型扫描。
- Why it matters: 可耗尽 blocking 池与 fd，影响整个服务。
- Realistic failure scenario: 设备上架后数天未完成向导，局域网扫描器循环请求该接口，服务卡死。
- Minimal fix: 扫描入口加 `Semaphore(1)` + 短期结果缓存（如 3 秒），忙时返回 503。
- Better long-term fix: 设置期接口统一要求初始化令牌。
- Regression test suggestion: 并发 8 次扫描，断言只执行 1 次真实探测。
- Estimated effort: 2–3 小时

### Finding: AUD2-17 只设置 width 或 height 之一时跳过分辨率范围校验

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 配置校验
- Evidence:
  - File: src/web.rs:2982-2986
  - Function / Module: `validate_config`
  - Relevant behavior: `if let (Some(width), Some(height))` 仅在两者都有值时检查范围。
- Problem: `{"video":{"width":0}}` 可被持久化。
- Why it matters: 采集请求非法格式失败，视频一直离线并跨重启保留。
- Realistic failure scenario: 自定义分辨率只填宽度并误输入 0，保存成功但视频永久离线。
- Minimal fix: 分别校验 width 与 height。
- Better long-term fix: 用带范围约束的 newtype 表示分辨率。
- Regression test suggestion: `width=Some(0), height=None` 必须返回错误。
- Estimated effort: 0.5 小时

### Finding: AUD2-18 auth.json 同步读写与 fsync 运行在 Tokio worker 上

- Severity: Low
- Confidence: High
- Category: Performance
- Status: Confirmed
- Affected area: 认证存储 / 系统监控
- Evidence:
  - File: src/web.rs:305-312, src/web.rs:628-630, src/web.rs:1585-1589, src/auth.rs:355-373, src/auth.rs:722-785, src/system.rs:62-77
  - Function / Module: `terminal_is_ready`、`bootstrap`、Passkey handler、`AuthStore::load/update`、`SystemMonitor::snapshot`
  - Relevant behavior: 同步 read/write/fsync/rename 直接在 async handler 中执行；`finish_setup` 已使用 `spawn_blocking`，其他路径未统一。
- Problem: 存储抖动时阻塞 worker 线程。
- Why it matters: RK3399 worker 数量少，并发时 API 整体假死。
- Realistic failure scenario: SD 卡写入抖动时多个 Passkey/bootstrap 请求同时卡在 fsync，WebSocket 心跳超时。
- Minimal fix: 为 AuthStore 提供 async 封装统一走 `spawn_blocking`。
- Better long-term fix: 认证记录内存缓存 + 写入串行化后台任务。
- Regression test suggestion: 注入慢存储，断言其他请求不被阻塞。
- Estimated effort: 0.5 天

### Finding: AUD2-19 终端退出时无限等待 PTY reader 线程

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 网页终端
- Evidence:
  - File: src/web.rs:396-412, src/web.rs:469-481
  - Function / Module: `TerminalSession::stop`
  - Relevant behavior: kill 只作用于 leader 进程，`reader_task.await` 无超时；`setsid` 后台进程持有 slave 时 master read 永不返回。
- Problem: 每次泄漏一个 blocking 线程与连接任务。
- Why it matters: 累积后耗尽 blocking 池，影响密码哈希、扫描等全部 blocking 工作。
- Realistic failure scenario: 管理员在终端运行 `setsid long-task &` 后关闭页面，重复多次后服务功能性瘫痪。
- Minimal fix: 对 reader/writer task 的 await 加超时并记录日志。
- Better long-term fix: 使用非阻塞 fd + `AsyncFd` 读取 PTY，或退出时杀掉整个会话的进程。
- Regression test suggestion: 保持 slave 打开，断言 `stop()` 在限定时间内返回。
- Estimated effort: 2–4 小时

### Finding: AUD2-20 bootstrap 把认证存储错误静默当作未初始化

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 启动引导接口
- Evidence:
  - File: src/web.rs:628-639
  - Function / Module: `bootstrap`
  - Relevant behavior: `!state.auth.is_initialized().unwrap_or(false)` 与 `load().ok().flatten()` 吞掉 IO/解析错误。
- Problem: auth.json 不可读时前端显示"首次初始化"，但初始化令牌已失效，操作永远失败。
- Why it matters: 误导运维删除认证数据重新初始化。
- Realistic failure scenario: 文件权限被误改后网页显示初始化向导，运维误判为数据丢失。
- Minimal fix: 读取失败时返回 503 与明确诊断信息。
- Better long-term fix: 统一"存储不可用"错误态并在 UI 中专门展示。
- Regression test suggestion: AuthStore 指向不可读文件，断言 bootstrap 返回 503。
- Estimated effort: 1 小时

### Finding: AUD2-21 TranscodeJpeg 模式单帧转码失败拆掉整个采集会话

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 视频采集
- Evidence:
  - File: src/devices/video.rs:435-440
  - Function / Module: 采集循环
  - Relevant behavior: `transcode_jpeg(source, quality)?` 把单帧错误上抛为会话失败，触发设备重开与状态 Offline。
- Problem: 偶发坏帧导致整条流重启。
- Why it matters: 画面周期性卡顿，WebRTC 可能被空闲看门狗断开。
- Realistic failure scenario: UVC 采集卡偶发截断帧，每几十秒断流一次。
- Minimal fix: 转码失败时记录警告并跳过该帧。
- Better long-term fix: 连续失败计数超过阈值才判定会话失败。
- Regression test suggestion: 输入首尾标记合法但内容非法的帧，断言被跳过而非会话失败。
- Estimated effort: 1–2 小时

### Finding: AUD2-22 release_all 队列满时丢弃释放命令且前端仍提示成功

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: HID / 前端释放按键
- Evidence:
  - File: src/devices/hid.rs:338-388, src/web_ui.rs:1718
  - Function / Module: `HidManager::release_all` / 前端 `releaseAll`
  - Relevant behavior: 使用 `try_send`，队列满返回 QueueFull（429）；前端 `.catch(()=>{})` 吞掉并照常提示"已释放"。
- Problem: 卡住的修饰键无法通过紧急释放清除。
- Why it matters: Ctrl/Alt 残留按下会误触被控机快捷键。
- Realistic failure scenario: 快速连打后点"释放按键"，请求 429 被吞，被控机 Ctrl 仍处于按下状态。
- Minimal fix: release_all 使用带短超时的 `send`；前端按结果显示成功或失败。
- Better long-term fix: worker 设置优先级释放通道。
- Regression test suggestion: 容量 1 的队列灌满后调用 release_all，断言最终收到 Release。
- Estimated effort: 2 小时

### Finding: AUD2-23 `/api/status` 单次失败即隐藏正在播放的视频

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端状态轮询
- Evidence:
  - File: src/web_ui.rs:1830, src/web_ui.rs:1445-1457, src/web_ui.rs:387
  - Function / Module: `refreshStatus` / `renderVideoUi`
  - Relevant behavior: catch 中把视频状态伪造为 offline，`.video-unhealthy` 隐藏所有 surface。
- Problem: 状态查询失败被等同为视频离线。
- Why it matters: 画面空白但流仍在消耗带宽，误导用户。
- Realistic failure scenario: 代理对 `/api/status` 偶发 502，画面每隔几秒消失。
- Minimal fix: 失败时保留上次已知状态，单独记录 statusError。
- Better long-term fix: 视频可见性只由视频元素自身事件与服务端明确状态决定。
- Regression test suggestion: 播放中让 `/api/status` 返回 500，断言 `#video-feed` 仍可见。
- Estimated effort: 1 小时

### Finding: AUD2-24 "宽屏默认展开面板"设置无效

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 前端偏好
- Evidence:
  - File: src/web_ui.rs:1136-1140, src/web_ui.rs:1589, src/web_ui.rs:2334
  - Function / Module: `showState('main')` / `setInspectorOpen`
  - Relevant behavior: 默认值只由媒体查询与 `wingman_inspector_open` 决定，`auto_expand_inspector` 从不参与。
- Problem: 设置项是纯装饰。
- Why it matters: 用户偏好被忽略，削弱对设置界面的信任。
- Realistic failure scenario: 取消勾选后重新登录面板依旧展开。
- Minimal fix: 用该设置作为默认值，仅在用户显式切换后才使用记忆值。
- Better long-term fix: 偏好统一由一个 settings 模块管理并有测试。
- Regression test suggestion: 设置为 false 并清除记忆键，断言宽屏登录后面板收起。
- Estimated effort: 1 小时

### Finding: AUD2-25 扫描/介质/Passkey 请求无代次保护，表单可重复提交

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 前端请求状态
- Evidence:
  - File: src/web_ui.rs:1400-1430, src/web_ui.rs:1966-1971, src/web_ui.rs:2183-2270
  - Function / Module: `scanDevices`、`refreshMedia`、`refreshPasskeys`、`#device-config`/`#media-config` submit
  - Relevant behavior: 无请求 id/AbortController，最后到达者覆盖；保存时不禁用按钮。
- Problem: 旧响应覆盖新状态；双击产生两次提交。
- Why it matters: 显示与实际设备状态不一致。
- Realistic failure scenario: 登录自动扫描与手动"重新检测"并发，慢的旧结果最后到达覆盖新结果。
- Minimal fix: 每类数据维护递增 requestId，丢弃过期响应；提交期间禁用按钮并设 `aria-busy`。
- Better long-term fix: 引入统一的请求状态封装（loading/error/data + 代次）。
- Regression test suggestion: mock 先发后到的两个扫描响应，断言最终显示后发结果。
- Estimated effort: 0.5–1 天

### Finding: AUD2-26 recover.sh 忽略 COMMITTED，对成功事务执行会回滚

- Severity: Low
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 升级恢复脚本
- Evidence:
  - File: deploy/upgrade-transaction.sh:438-561, docs/UPGRADE_RECOVERY.md:115-131
  - Function / Module: 内嵌 `recover.sh`
  - Relevant behavior: 只检查 PENDING，不检查 COMMITTED，直接按 manifest 恢复快照。
- Problem: 对已提交事务执行恢复会降级系统。
- Why it matters: 磁盘版本与运行版本不一致，标记互相矛盾。
- Realistic failure scenario: 管理员按文档在已成功的事务目录执行 recover.sh 做"清理"。
- Minimal fix: 存在 COMMITTED 且无 PENDING 时拒绝执行，除非 `--force`。
- Better long-term fix: 恢复脚本读取统一状态机文件。
- Regression test suggestion: 成功提交后执行 recover.sh，断言非零退出且文件不变。
- Estimated effort: 0.5 小时

### Finding: AUD2-27 指定 UDC 被占用时 wait_for_udc 吞掉 die 并重复重试

- Severity: Low
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: `deploy/wingmankvm-gadget`
- Evidence:
  - File: deploy/wingmankvm-gadget:106-134, deploy/wingmankvm-gadget:381
  - Function / Module: `select_udc` / `wait_for_udc`
  - Relevant behavior: `die` 在命令替换子 shell 中只退出子 shell，父 shell 视为"未就绪"继续重试约 76 次。
- Problem: 真实原因被掩盖，服务无谓等待 15 秒。
- Why it matters: 日志被重复错误淹没，难以定位配置冲突。
- Realistic failure scenario: 多 UDC 板卡把 UDC 指向厂商服务占用的控制器。
- Minimal fix: 进入等待前先做致命校验，失败立即 die。
- Better long-term fix: select_udc 返回专用状态码区分致命与暂时性失败。
- Regression test suggestion: 伪造被占用的 UDC，断言立即失败且只输出一次错误。
- Estimated effort: 0.5 小时

### Finding: AUD2-28 CI 未校验 patched dtls crate 的上游清单

- Severity: Low
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 供应链 / CI
- Evidence:
  - File: .github/workflows/regression.yml:35-36, vendor/dtls/upstream-manifest.json
  - Function / Module: CI regression job
  - Relevant behavior: 只执行 xterm 来源校验；dtls 有上游哈希清单但无脚本强制校验（当前 93/98 一致，5 个差异均在 PATCHES.md 记录）。
- Problem: 未来对 vendor 加密代码的意外修改不会被 CI 拦截。
- Why it matters: DTLS 是 WebRTC 加密链路的一部分。
- Realistic failure scenario: 依赖刷新 PR 意外修改 `vendor/dtls/src/crypto`，测试通过即合入。
- Minimal fix: 新增 `scripts/verify_dtls_vendor.py` 校验清单与已记录补丁哈希，并加入 CI。
- Better long-term fix: 推动上游合入补丁后移除 vendor。
- Regression test suggestion: 脚本自测：改动一个字节应失败。
- Estimated effort: 2–3 小时

### Finding: AUD2-29 discovery 用 unwrap_or_default 吞掉扫描任务失败

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: `src/devices/discovery.rs`
- Evidence:
  - File: src/devices/discovery.rs:171-174, src/devices/discovery.rs:385-388, src/devices/discovery.rs:492-497
  - Function / Module: `scan_video` / `scan_hid_roles` / `scan_luns`
  - Relevant behavior: `spawn_blocking(...).await.unwrap_or_default()`，panic/取消变为空列表。
- Problem: 无法区分"没有设备"与"探测失败"，且不记日志。
- Why it matters: 误导用户手工填写错误路径。
- Realistic failure scenario: 某驱动 ioctl 触发 panic，向导显示"未发现视频设备"。
- Minimal fix: 失败时记录 warn 并在结果中带 `degraded`/`warnings` 字段。
- Better long-term fix: scan 返回 `Result` 并由 UI 显示"探测失败，请重试"。
- Regression test suggestion: 注入 panic 的探测函数，断言结果标记 degraded。
- Estimated effort: 1–2 小时

### Finding: AUD2-30 快速开始文档缺少 libgpiod 2.x 与 python3 依赖说明

- Severity: Low
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 文档
- Evidence:
  - File: README.md:31-32, docs/RK3399_GUIDE.md:29-31, src/devices/power.rs:284-301, deploy/install.sh:764
  - Function / Module: `is_gpioset_v2` / `require_command python3`
  - Relevant behavior: 代码要求 gpioset 2.x 与 python3；README/RK3399 指南未写版本要求，也未列 python3（DEPLOYMENT.md 已正确）。
- Problem: 文档之间互相矛盾。
- Why it matters: 用户按快速开始操作会在安装或启用 GPIO 时失败。
- Realistic failure scenario: Debian bookworm 安装 libgpiod 1.x 后启用电源控制，得到"gpioset 2.x is required"。
- Minimal fix: README 与 RK3399 指南补充版本要求与 python3，或直接链接 DEPLOYMENT 依赖列表。
- Better long-term fix: install.sh 预检 gpioset 版本并警告。
- Regression test suggestion: 文档审阅；可选 install 预检 v1 stub 被警告。
- Estimated effort: 0.5 小时

### Finding: AUD2-31 可访问性语义缺失：转发按钮被 role=status 覆盖、修饰键与标签页无状态

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 前端可访问性
- Evidence:
  - File: src/web_ui.rs:532, src/web_ui.rs:543, src/web_ui.rs:1651-1665
  - Function / Module: `#input-state` / inspector tabs / `updateVirtualModifierButtons`
  - Relevant behavior: `<button role="status">` 覆盖按钮语义；修饰键只切 `.active` 无 `aria-pressed`；标签页无 `role=tab`/`aria-selected`。
- Problem: 辅助技术用户无法得知控件可操作或当前状态。
- Why it matters: 粘滞修饰键状态不可感知可能导致误操作。
- Realistic failure scenario: 屏幕阅读器用户不知道 Ctrl 处于粘滞按下状态。
- Minimal fix: 把 live region 移到内部文本；加 `aria-pressed`；标签页改为 tablist 语义。
- Better long-term fix: 前端重构时引入 axe-core 检查。
- Regression test suggestion: Playwright 可访问性快照断言 role 与 aria-pressed 变化。
- Estimated effort: 2 小时

### Finding: AUD2-32 HID 热路径每条报告都重新 open 并注册设备

- Severity: Low
- Confidence: High
- Category: Performance
- Status: Confirmed
- Affected area: `src/devices/hid.rs`
- Evidence:
  - File: src/devices/hid.rs:732-788
  - Function / Module: `send_report`
  - Relevant behavior: 每条 1–8 字节报告执行 open → AsyncFd::register → write → 注销 → close；大位移鼠标移动拆成数十条报告。
- Problem: 系统调用放大，并在 gadget rebind 时增加失败窗口。
- Why it matters: 增加输入延迟与 IO 驱动争用。
- Realistic failure scenario: 60Hz 指针输入下每秒数百次 open/close。
- Minimal fix: worker 内按路径缓存 `AsyncFd<File>`，写错误时重开。
- Better long-term fix: 封装 `HidWriter` 统一管理设备句柄生命周期。
- Regression test suggestion: 计数钩子断言连续两条报告只 open 一次。
- Estimated effort: 2–3 小时

### Finding: AUD2-33 GPIO 脉冲等待 stderr 读取无超时，可能永久占住 busy 锁

- Severity: Low
- Confidence: Medium
- Category: Stability
- Status: Suspected
- Affected area: `src/devices/power.rs`
- Evidence:
  - File: src/devices/power.rs:235-282
  - Function / Module: `wait_for_gpio_command` / `read_stderr`
  - Relevant behavior: 子进程退出后裸 `await` stderr 读取任务；若后代进程继承 stderr 管道则永不 EOF。当前硬编码 `gpioset` 不 fork，故为 Suspected。
- Problem: 一次外部命令异常可让电源/复位功能永久返回 Busy。
- Why it matters: 电源控制是远程恢复被控机的最后手段。
- Realistic failure scenario: 将来改用会 fork 的 wrapper 后，电源按钮永久 409。
- Minimal fix: stderr 任务 await 加剩余预算超时。
- Better long-term fix: 子进程统一封装（进程组 + 限长读取 + 超时）。
- Regression test suggestion: fixture 脚本 `sleep 30 & exec ...`，断言 pulse 在 7 秒内返回且锁释放。
- Estimated effort: 1 小时

### Finding: AUD2-34 提取器拒绝绕过统一 JSON 错误格式

- Severity: Info
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: Backend API 错误响应
- Evidence:
  - File: src/web.rs:3320-3348
  - Function / Module: `ApiError` / 各 handler 的 `Json`/`Form`/`Multipart` 提取器
  - Relevant behavior: axum 的 Rejection 以 text/plain 返回，不走 `{"error":...}`。
- Problem: 错误结构不一致。
- Why it matters: 前端只能显示通用错误。
- Realistic failure scenario: 代理截断请求体时登录接口返回纯文本 400。
- Minimal fix: 自定义提取器包装 Rejection 映射为 ApiError。
- Better long-term fix: 统一错误码枚举。
- Regression test suggestion: 畸形 JSON 请求断言 JSON 错误体。
- Estimated effort: 0.5 天

### Finding: AUD2-35 WebRtcManager::new 在 async 主线程同步探测 FFmpeg

- Severity: Info
- Confidence: High
- Category: Performance
- Status: Confirmed
- Affected area: 启动流程
- Evidence:
  - File: src/devices/webrtc.rs:337-353, src/web.rs:131
  - Function / Module: `WebRtcManager::new` / `probe_encoder`
  - Relevant behavior: 启动时阻塞探测最长约 5 秒；`reconfigure` 已正确使用 spawn_blocking。
- Problem: 启动期阻塞 worker。
- Why it matters: 仅延迟启动，无并发受害者。
- Realistic failure scenario: ffmpeg_path 指向卡住的脚本，服务晚 5 秒开始监听。
- Minimal fix: 启动后后台探测。
- Better long-term fix: 编码器能力作为异步可观察状态。
- Regression test suggestion: hang fixture 断言 AppState 构造快速返回。
- Estimated effort: 1 小时

### Finding: AUD2-36 切换到视频页后终端 WebSocket/PTY 仍在后台运行

- Severity: Info
- Confidence: Medium
- Category: Security
- Status: Suspected
- Affected area: 前端终端
- Evidence:
  - File: src/web_ui.rs:1789-1813
  - Function / Module: `setWorkspaceMode` / `terminalDisconnect`
  - Relevant behavior: 切到视频页只隐藏终端，不关闭 socket；可能是有意保留会话。
- Problem: 常驻可交互 shell 与资源占用，UI 未说明。
- Why it matters: 扩大暴露面。
- Realistic failure scenario: 长时间观看视频期间 root 维护 shell 一直存活。
- Minimal fix: 明确产品意图；若不需要保留则离开时断开，否则加空闲超时与 UI 提示。
- Better long-term fix: 终端会话状态显式化（后台运行指示）。
- Regression test suggestion: 切换后断言 socket 状态符合设计。
- Estimated effort: 1 小时

## 5. Architecture Concerns

- Coverage: High
- Inspected evidence: 全部 `src/` 模块、`router()`、`AppState`、设备管理器依赖方向
- Exclusions / limits: vendor 代码

依赖方向整体正确（web → devices/auth/config，devices 不反向依赖 web）。问题集中在 `web.rs` 的 ModuleBoundary（AUD2-14）与前端无模块边界（AUD2-13）；状态归属上前端同一数据多处渲染无单一来源（AUD2-13、AUD2-25）。

## 6. Security Concerns

- Coverage: High
- Inspected evidence: 会话、Origin、Passkey、PPv2、终端、上传、安装器
- Exclusions / limits: 未做真实网络渗透

AUD2-01、AUD2-04、AUD2-05、AUD2-06、AUD2-07、AUD2-11、AUD2-16、AUD2-36。已验证良好：路由鉴权无遗漏；SameSite=Strict/HttpOnly；非 GET Origin 校验；终端 WS 严格来源；登录限流 fail-closed；上传路径防穿越；常量时间令牌比较。

## 7. Stability Concerns

- Coverage: High
- Inspected evidence: 子进程生命周期、超时、worker 队列、错误传播
- Exclusions / limits: 无真实硬件压测

AUD2-02、AUD2-03、AUD2-09、AUD2-10、AUD2-12、AUD2-15、AUD2-17、AUD2-19、AUD2-20、AUD2-21、AUD2-22、AUD2-23、AUD2-25、AUD2-29、AUD2-33。

## 8. Performance Concerns

- Coverage: Medium
- Inspected evidence: 视频帧路径、HID 写入、同步 IO、扫描
- Exclusions / limits: 无运行时 profiling

AUD2-18、AUD2-32、AUD2-35。帧路径使用 `Bytes` 共享、watch 通道，无明显多余拷贝。

## 9. Testing Gaps

- Coverage: High
- Inspected evidence: 222 个 Rust 测试、`scripts/verify_*.py`、CI
- Exclusions / limits: —

前端 Rust 测试为 135 条 `INDEX_HTML.contains` 断言，AUD2-03/09/10/11/24 全部能通过现有测试；Playwright 脚本未覆盖视频表单、焦点与 details 键盘路径；各脚本重复实现 HTML 提取逻辑，依赖 raw string 边界。

## 10. Maintainability Concerns

- Coverage: High
- Inspected evidence: 文件规模、超长行、重复区块、i18n 覆盖
- Exclusions / limits: —

AUD2-13、AUD2-14、AUD2-24、AUD2-31、AUD2-34。

## 11. Design / Principles Concerns

- Coverage: High
- Inspected evidence: rubrics/principles.md 对照
- Exclusions / limits: —

SRP（两大文件）、DRY（UI 三处性能渲染、Playwright 提取重复）、Fail-Fast（AUD2-12、AUD2-20、AUD2-29）、最小惊讶（AUD2-02 本地偏好弹窗里放服务端配置）。

## 12. Type Safety Concerns

- Coverage: Medium
- Inspected evidence: serde 边界、patch 三态（`Option<Option<T>>`）、错误类型
- Exclusions / limits: vendor 未审

无 unsafe 滥用；patch 三态语义正确且有测试。StringlyTyped：前端以消息文本匹配识别 HID 超时（AUD2-12），应改为结构化错误码。

## 13. Release Concerns

- Coverage: High
- Inspected evidence: install/upgrade/recover/gadget 脚本、CI、Cargo 版本
- Exclusions / limits: 未在真实设备执行升级

AUD2-04、AUD2-08、AUD2-26、AUD2-27、AUD2-28、AUD2-30。版本号仍为 0.1.0，无发布产物签名（Info，未单列）。

## 14. Documentation Analysis

- Coverage: Medium
- Inspected evidence: README、DEPLOYMENT、RK3399_GUIDE、TESTING、UPGRADE_RECOVERY 抽样对照
- Exclusions / limits: 未逐句核对

| Subtype | Count | Affected Docs | Recommended Action |
|---------|-------|---------------|-------------------|
| UserDocs | 1 | README、RK3399_GUIDE | 补充依赖版本（AUD2-30） |
| OperatorDocs | 1 | UPGRADE_RECOVERY | 说明 COMMITTED 事务不可 recover（AUD2-26） |
| DeveloperDocs | 0 | — | TESTING.md 与 CI 一致 |
| ApiDocs | 0 | — | — |
| DecisionRecord | 0 | — | — |
| StaleDocs | 0 | — | — |

## 15. Observability / Operability Analysis

- Coverage: Medium
- Inspected evidence: tracing 调用、`/healthz`、错误映射日志
- Exclusions / limits: 无生产日志样本

| Subtype | Count | Critical Signals Missing | Recommended Action |
|---------|-------|--------------------------|-------------------|
| Logging | 1 | discovery 失败无日志（AUD2-29） | 加 warn |
| Debuggability | 1 | bootstrap 吞错（AUD2-20） | 返回诊断信息 |
| HealthCheck | 0 | — | `/healthz` 已存在 |

## 16. Configuration Safety Analysis

- Coverage: High
- Inspected evidence: `validate_config`、patch 函数、PPv2/public_origin
- Exclusions / limits: —

| Subtype | Count | Affected Keys / Files | Recommended Action |
|---------|-------|-----------------------|-------------------|
| SchemaValidation | 1 | video.width/height（AUD2-17） | 独立校验 |
| UnsafeDefault | 1 | 前端 suppress_hid_timeout=true（AUD2-12） | 默认关闭 |
| FeatureFlag | 1 | server.proxy_protocol 即时生效（AUD2-02） | 确认 + 两阶段提交 |

## 17. Data Integrity Analysis

- Coverage: High
- Inspected evidence: 原子写、配置串行化、上传 hard_link 发布、事务标记
- Exclusions / limits: —

| Subtype | Count | Invariants at Risk | Recommended Action |
|---------|-------|-------------------|-------------------|
| TransactionBoundary | 2 | 升级事务状态（AUD2-08、AUD2-26） | 提交后禁止回滚 |
| ConcurrencyConsistency | 0 | — | config_update 互斥已正确 |

## 18. Privacy / Data Governance Analysis

- Coverage: Medium
- Inspected evidence: auth.json 字段、日志字段、客户端 IP 处理
- Exclusions / limits: 无保留策略文档

| Subtype | Count | Affected Data | Recommended Action |
|---------|-------|---------------|-------------------|
| AccessBoundary | 2 | 屏幕画面（AUD2-06）、会话（AUD2-11） | 绑定会话生命周期 |
| TelemetryPrivacy | 0 | — | 初始化令牌仅在启动日志打印一次，属有意设计 |

## 19. Accessibility / UX Correctness Analysis

- Coverage: Medium
- Inspected evidence: 静态 ARIA/语义检查 + Chromium 键盘复现
- Exclusions / limits: 未用屏幕阅读器

| Subtype | Count | Affected Workflows | Recommended Action |
|---------|-------|-------------------|-------------------|
| SemanticStructure | 1 | 转发按钮、标签页（AUD2-31） | 修正 role/aria |
| KeyboardFocus | 2 | 键盘转发（AUD2-03）、details（AUD2-10） | 焦点状态机 |
| ErrorState | 2 | HID 错误（AUD2-12）、注销失败（AUD2-11） | 显式错误反馈 |
| LoadingState | 1 | 重复提交（AUD2-25） | 禁用按钮 + aria-busy |
| UXStateCorrectness | 3 | 视频设置（AUD2-09）、状态失败（AUD2-23）、面板偏好（AUD2-24） | 单一状态源 |

## 20. Supply Chain / Reproducibility Analysis

- Coverage: High
- Inspected evidence: Cargo.lock、`--locked` 构建、vendor 清单、CI action SHA、cargo audit
- Exclusions / limits: advisory DB 为本地缓存

| Subtype | Count | Affected Surface | Recommended Action |
|---------|-------|------------------|-------------------|
| DependencyProvenance | 1 | vendor/dtls（AUD2-28） | CI 校验清单 |
| CIIntegrity | 0 | — | 已固定 SHA、最小权限 |
| ArtifactProvenance | 0 | — | 建议后续增加发布校验和（Info） |

## 21. Cost / Resource Economics Analysis

- Coverage: Medium
- Inspected evidence: 挑战表、扫描、流并发、blocking 池使用
- Exclusions / limits: 无云/外部 API 成本面

| Subtype | Count | Cost Driver | Recommended Action |
|---------|-------|-------------|-------------------|
| UnboundedWork | 3 | 挑战表（AUD2-07）、设备扫描（AUD2-16）、终端线程（AUD2-19） | 上限 + 去重 |

## 22. AI / LLM Safety Analysis

- Coverage: Not assessed
- Inspected evidence: 全库检索无 LLM/模型 SDK 或 prompt 代码
- Exclusions / limits: 不适用

| Subtype | Count | Boundary Crossed | Recommended Action |
|---------|-------|------------------|-------------------|
| PromptInjection | 0 | 不适用 | — |

## 23. Fallback / Defensive Code Analysis

- Coverage: High
- Inspected evidence: `unwrap_or_default`、`.ok()`、`catch(()=>{})` 全文检索
- Exclusions / limits: —

| Subtype | Count | KeepWithAlert | FailFast | Remove |
|---------|-------|---------------|----------|--------|
| SilentFallback | 3 | 1（AUD2-29） | 1（AUD2-20） | 1（AUD2-12 默认屏蔽） |
| EmptyCatch | 2 | 1（AUD2-22 前端） | 0 | 1（AUD2-11） |
| DefensiveGuess | 1 | 0 | 1（AUD2-23 伪造 offline） | 0 |

## 24. Testing Authenticity Analysis

- Coverage: High
- Inspected evidence: Rust 测试、Python/Playwright 脚本
- Exclusions / limits: —

### Confidence Assessment

| Test Area | Real Confidence | Risk | Action |
|-----------|---------------|------|--------|
| 后端单元/集成（222 项） | High | 少量 handler 无 HTTP 级测试 | Keep |
| 升级事务 harness | High | 未覆盖提交后信号 | Keep + 补用例 |
| 前端 Rust contains 断言 | Low | 接线/焦点缺陷全部逃逸 | Rewrite 为 Playwright |
| Playwright UI 脚本 | Medium | 未覆盖视频表单/焦点 | Keep + 扩展 |

### Valuable Tests

登录限流并发测试、系统密码 helper 进程组回收、终端 WS 撤销测试、升级事务 SIGKILL/PENDING 测试。

### Suspicious Tests

`src/web_ui.rs` 测试模块中的字符串存在性断言，只证明代码文本存在。

### Missing Tests

键盘焦点转发、视频设置提交 payload、注销失败、PPv2 accept 并发、Passkey 删除后登录。

## 25. Frontend State Analysis

- Coverage: High
- Inspected evidence: `src/web_ui.rs` 全部 JS
- Exclusions / limits: —

| Subtype | Count | Affected Components |
|---------|-------|-------------------|
| ComponentSize | 1 | 整个前端单文件（AUD2-13） |
| StateDuplication | 1 | 性能/LED/电源多处渲染（AUD2-13） |
| RequestState | 2 | 扫描/介质/Passkey（AUD2-25）、状态轮询（AUD2-23） |
| UIBusinessCoupling | 1 | 偏好弹窗内服务端配置（AUD2-02） |

## 26. Backend API Analysis

- Coverage: High
- Inspected evidence: 全部路由与 handler
- Exclusions / limits: —

| Subtype | Count | Affected Endpoints |
|---------|-------|-------------------|
| Validation | 1 | POST /api/config（AUD2-17） |
| Auth | 3 | /video_feed、/api/passkey/login/*（AUD2-05/06/07） |
| ErrorResponse | 1 | 提取器拒绝（AUD2-34） |

## 27. Dependency Weight Analysis

- Coverage: Medium
- Inspected evidence: Cargo.toml 依赖用途
- Exclusions / limits: 未测二进制体积

| Dependency | Status | Weight | Transitives | Used For | Recommended Action |
|------------|--------|--------|-------------|----------|-------------------|
| webrtc@0.17 | Healthy | 大 | 多 | H.264 推流 | Keep |
| openssl (vendored) | Healthy | 大 | 少 | webauthn 证书校验 | Keep |
| image@0.25 (jpeg) | Healthy | 中 | 少 | JPEG 转码 | Keep |
| portable-pty@0.9 | Healthy | 小 | 少 | 网页终端 | Keep |

## 28. Code Consistency Analysis

- Coverage: High
- Inspected evidence: 两大文件风格
- Exclusions / limits: —

`web.rs` 上传段（2460-2630）为压缩单行风格，与文件其余 rustfmt 风格不一致；前端 JS 中长行压缩与多行格式混用。拆分时统一格式化。

## 29. Comment Coverage Analysis

- Coverage: Medium
- Inspected evidence: 关键函数注释抽样
- Exclusions / limits: —

安全关键路径（终端来源校验、会话 guard、限流）注释质量高；前端 JS 几乎无注释，结合超长行几乎不可读。

---

## 30. Principles Compliance

整体而言，后端安全与并发相关代码遵循 fail-closed 与明确所有权原则，问题集中在模块规模和前端结构。

### Principles Violated

| Principle | Violations | Severity | Affected Areas |
|-----------|------------|----------|----------------|
| Single Responsibility (SRP) | 2 | Medium | web.rs、web_ui.rs |
| File Size Limit | 2 | Medium | web.rs（5460）、web_ui.rs（2655） |
| DRY | 2 | Medium | 前端重复 UI 区块、Playwright 提取逻辑 |
| Fail-Fast | 3 | Low–Medium | bootstrap、discovery、前端 HID 错误 |
| Principle of Least Astonishment | 1 | High | PPv2 放在本地偏好弹窗 |

### Principles Respected

最小权限（systemd 用户、sudoers 精确命令）、原子写、显式会话生命周期（SessionGuard）、配置 patch 三态语义、子进程进程组回收、CI action 固定。

---

## 31. Recommended Fix Order

### Fix Immediately

AUD2-01、AUD2-02、AUD2-03、AUD2-04

### Fix Before Stable Release

AUD2-05、AUD2-06、AUD2-07、AUD2-08、AUD2-09、AUD2-10、AUD2-11、AUD2-12、AUD2-15、AUD2-16

### Schedule Later

AUD2-13、AUD2-14（结构重构，与前端重设计一起做）、AUD2-17 至 AUD2-29、AUD2-31、AUD2-32

### Ignore for Now

AUD2-30（文档，顺手修）、AUD2-33、AUD2-34、AUD2-35、AUD2-36

## 32. Quick Wins

- AUD2-04：三处加 `-I`（10 分钟）
- AUD2-17：分别校验宽高（20 分钟）
- AUD2-10：白名单加 `summary`（10 分钟）
- AUD2-08：提交后移除 trap（30 分钟）
- AUD2-26：recover.sh 检查 COMMITTED（20 分钟）
- AUD2-05：检查返回值（30 分钟）
- AUD2-11：注销失败不切页（30 分钟）

## 33. Long-term Refactor Plan

1. **前端拆分与重设计**：动机——AUD2-13 及 AUD2-03/09/12/24/25 均源于单文件无结构。方案——`web/index.html` + `web/css/*` + `web/js/*`（api、i18n、video、input、terminal、settings、media、devices、passkey、status 等模块），Rust 以 `include_str!` 嵌入；合并重复区块，重排信息架构，补全 i18n。风险——行为回归。测试——先扩展 Playwright 覆盖再迁移，CI 加 `node --check`。
2. **`web.rs` 模块化**：动机——AUD2-14。方案——按领域拆分为 `src/web/` 子模块，纯移动。风险——低。测试——`cargo test` 结果一致。
3. **统一长连接会话绑定与资源上限**：动机——AUD2-06/07/16/19。方案——公共的 guard 包装与有界资源池。测试——撤销/并发上限测试。
