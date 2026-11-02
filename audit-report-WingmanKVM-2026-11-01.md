# Fuck My Shit Mountain Audit Report

**Project:** WingmanKVM
**Audit mode:** full
**Date:** 2026-11-01
**Reviewer:** Claude Opus 5.5（主审）+ 3 路 Claude 只读子代理。稳定性与前端/测试/发布两路的高影响结论由主审对照源码复核；其余低危项为子代理追踪结果。

---

## 1. Executive Summary

本轮是 #97 约定的批次后全量复审，审查对象是合并 PR #105 后的 main（d7d77cd）。共 38 项：2 High / 11 Medium / 21 Low / 4 Info，没有 Critical。cargo test 362 通过，cargo audit 无漏洞，上次报告里的高危项没有回退。

最重要的问题在发布环节而不是代码本身：发布包文件名与 get.sh、在线升级请求的文件名不一致，加上仓库从未发布过版本，所以 README 的一行安装和在线升级目前都不可能成功。其次是测试缺口：桌面鼠标、快捷键这些 KVM 核心输入没有 UI 测试。运行时风险集中在输入 WebSocket 断开时可能重复执行按键、在线升级开始时显示旧状态、配置保存被中断后磁盘与内存不一致。

亮点：ES modules 改造经脚本化比对没有发现行为回退；WebRTC、音频的进程和会话生命周期没有发现泄漏；升级事务的路径防护严格。本报告的主要局限：所有结论来自代码阅读和模拟环境，新功能都没有在真实硬件上运行过；安全路子代理未能交付完整报告，安全维度由主审重新审查，覆盖度相应标低。

### Score Dashboard

```
Security        ████████░░  8.0  A   鉴权与同源边界完整；导入预览泄露密钥，ffmpeg_path 可执行任意文件
Stability       ████████░░  7.5  A   生命周期扎实；按键重放、配置提交可被取消
Performance     ████████░░  8.0  A   无明显热点问题；缺运行时实测，覆盖受限
Testing         ██████░░░░  6.0  B   套件大且 CI 严格，但核心鼠标输入无测试，跳过项不会失败
Maintainability ████████░░  7.5  A   模块化完成；webrtc.rs 3188 行、少量文档规则失真
Design          ████████░░  7.5  A   边界清晰、最小权限到位；部分校验只在写入时发生
Release         █████░░░░░  5.0  B   事务式升级很强，但发布包命名错误且从未发布过版本
─────────────────────────────────────
Overall         ███████░░░  7.1  B
```

每个维度 0.0–10.0，越高越好（10 = 干净，0 = 屎山），为判断分，不是公式计算。

### Finding Statistics

| Severity | Count | Confirmed | Suspected |
|----------|-------|-----------|-----------|
| Critical | 0 | 0 | 0 |
| High | 2 | 2 | 0 |
| Medium | 11 | 10 | 1 |
| Low | 21 | 20 | 1 |
| Info | 4 | 4 | 0 |
| **Total** | **38** | **36** | **2** |

## 2. Project Map

Rust/axum 单二进制运行在 ARM 板上。入口 src/main.rs 构建 AppState 与路由（src/web/mod.rs）。设备层 src/devices/ 负责视频采集、FFmpeg 编码子进程、WebRTC、音频、HID、虚拟介质、电源和热插拔。认证与会话在 src/auth.rs、src/web/session.rs、passkey.rs。持久化是状态目录下的 config.json、auth.json、审计日志，均原子写入。前端是 web/js/app/ 下由 main.js 引导的原生 ES modules，编译进二进制。特权边界：服务以 wingmankvm 运行，只有三条固定 sudo 规则；安装和升级由 deploy/ 脚本以 root 执行事务。外部接口：HTTP/WebSocket、WebRTC、GitHub Release、通知服务、用户提供的下载 URL。本项目没有 AI/LLM 功能。

### Coverage Matrix

| Dimension | Coverage | Evidence inspected | Exclusions / limits |
|-----------|----------|--------------------|---------------------|
| Architecture | Medium | 模块依赖、状态归属、模块求值顺序 | 未逐文件评估全部 Rust 模块 |
| Security | Medium | 路由与中间件、配置校验、URL 下载、通知、sudo 规则、安装器与升级事务 | 安全子代理报告未交付；proxy_protocol.rs、client_ip.rs 本轮未复读；未做渗透测试；doctor 只按关键词扫描 |
| Stability | High | WebRTC、音频、视频、HID、配置提交、审计、升级状态 | 竞态未在运行时复现；无真实硬件 |
| Performance | Low | 阻塞调用、锁、自适应码率逻辑 | 没有运行时 profiling |
| Testing | High | 全部 UI 套件、CI 编排、362 项 Rust 测试实跑 | 未读 p2c 脚本内部 |
| Testing Authenticity | High | mock 边界、跳过逻辑、默认响应 | — |
| Maintainability | Medium | 文件规模、clippy 31 条警告 | 未做函数级度量 |
| Design | Medium | 对照 rubrics/principles.md 抽查 | 未逐条穷举 |
| Release | High | release.yml、regression.yml、打包命名、版本检查、远端 Release 状态 | 未实际发布 |
| Documentation | Medium | README、DEPLOYMENT、TESTING、CHANGELOG 中英文对照 | DESIGN.md 未读 |
| Configuration | Medium | config.rs、config_validate.rs、启动校验 | 未穷举所有键 |
| Observability | Medium | /api/status、日志点、通知失败路径 | 未看真实日志 |
| Data Integrity | Medium | 原子写入、事务回滚、配置提交 | 未做断电实测 |
| Privacy | Medium | 审计内容、通知字段、配置导出 | — |
| Accessibility | Low | 键盘与焦点路径 | 未用屏幕阅读器 |
| Supply Chain | Medium | Action SHA 固定、锁文件、cargo audit、npm audit | GitHub 仓库设置未检查 |
| Cost | Low | 无界工作、存储保留 | 无外部付费 API；未实测资源 |
| AI Safety | Not assessed | 全仓无模型调用、提示词或工具边界 | 项目不含 AI/LLM 功能 |
| Fallback | Medium | HTTP 回退、MJPEG 回退、静默吞错 | — |
| Type Safety | Medium | unsafe 20 处均为 libc/fd 调用，边界解析 | 未逐处证明 |
| Frontend State | High | 脚本化比对 IIFE 与模块、shared 状态、init 顺序、监听器数量 | 无真实设备 |
| Backend API | Medium | 路由表、校验、错误响应 | 未覆盖全部接口的错误分支 |
| Dependency Weight | Low | Cargo.toml、package.json | 未量化二进制体积 |
| Code Consistency | Low | clippy 与 ESLint 结果 | 未做全仓人工比对 |
| Comment Coverage | Low | 抽查关键模块，发现模块求值规则注释失真 | 未穷举 |

覆盖说明：排除 target、node_modules、web/vendor、lockfile 内容与二进制图片。运行过的命令：cargo test、cargo clippy、cargo audit、npm audit、eslint、shellcheck、report_lint。

## 3. Top Risks

1. **AUD3-01 发布包文件名与 get.sh、在线升级请求的文件名不一致**（High）：工作流产出 `wingmankvm-2.0.0-<target>.tar.gz`，客户端请求 `wingmankvm-v2.0.0-<target>.tar.gz`。
2. **AUD3-02 桌面鼠标、快捷键和虚拟键盘修饰键没有任何 UI 测试**（High）：KVM 最核心的输入路径没有回归保护。
3. **AUD3-03 没有任何 Release，且预发布 tag 不会被标记为预发布**（Medium）：README 第一条命令目前必然失败；而 rc tag 发布时会成为 latest，web 更新检查随后报“不是可安装的正式版本”。
4. **AUD3-04 输入 WebSocket 断开时会重发服务端已执行的按键**（Medium）：服务端已执行但回复没送达时，客户端无法区分，会再发一次。
5. **AUD3-05 配置保存被中断后，磁盘与运行中配置不一致**（Medium）：处理函数随客户端断开被取消时，停在写盘与更新内存之间。
6. **AUD3-06 在线升级开始时界面显示上一轮的结果**（Medium）：新任务写入第一条状态前，轮询读到旧状态文件。
7. **AUD3-07 导入预览把存储的通知密钥明文返回给浏览器**（Medium）：/api/config 隐藏了这些值，预览和导入响应却把它们放在 old 字段；预览还显示成将被清空。
8. **AUD3-08 ffmpeg_path 可指向任意可执行文件并以服务用户运行**（Medium）：管理员会话或被盗会话可以把网页配置升级为服务用户下的任意程序执行。
9. **AUD3-09 未认证请求可以用失败登录冲刷审计日志**（Medium）：攻击者拥有大量来源 IP 时可在短时间内覆盖全部审计记录，每条 fsync 也会占用阻塞线程。
10. **AUD3-10 终端 WebSocket 没有心跳，代理空闲断开会杀掉 shell**（Medium）：代理默认 60 秒空闲超时。
11. **AUD3-11 p2b 的指针锁定和全屏检查只会跳过，不会失败**（Medium）：应用即使不再调用 requestPointerLock，CI 也保持绿色。
12. **AUD3-12 verify_get_script、verify_mdns、verify_doctor 不在 CI 中**（Medium）：这些脚本只有手动运行才有用。

## 4. Detailed Findings

### Finding: AUD3-01 发布包文件名与 get.sh、在线升级请求的文件名不一致

- Severity: High
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 发布流程 / 安装
- Evidence:
  - .github/workflows/release.yml:141 `archive="wingmankvm-${VERSION}-${TARGET}"`，VERSION 不带 v
  - scripts/get.sh:121-125,245 与 deploy/wingmankvm-online-upgrade:187 都请求 `wingmankvm-${VERSION}-${TARGET}.tar.gz`，VERSION 带 v
  - scripts/verify_get_script.py 的夹具使用带 v 的名字，只验证了客户端一侧
- Problem: 工作流产出 `wingmankvm-2.0.0-<target>.tar.gz`，客户端请求 `wingmankvm-v2.0.0-<target>.tar.gz`。
- Why it matters: 一行安装和在线升级这两条主路径在任何版本上都无法下载。失败是安全的，但功能完全不可用。
- Realistic failure scenario: 打出第一个 tag，Release 正常发布；用户运行 README 的命令，得到 download failed；在线升级报 network 错误。
- Minimal fix: 把 release.yml 的 archive 改为带 v 的名字，或让客户端去掉 v，两侧统一。
- Better long-term fix: CI 用真实打包产物跑一次 get.sh --download-only。
- Regression test suggestion: 打包后在 CI 中用 file:// 指向 dist/ 运行 get.sh 并校验通过。
- Estimated effort: 0.5 小时

### Finding: AUD3-02 桌面鼠标、快捷键和虚拟键盘修饰键没有任何 UI 测试

- Severity: High
- Confidence: High
- Category: Testing
- Status: Confirmed
- Affected area: 前端输入
- Evidence:
  - `grep page.mouse scripts/verify_*.py` 只命中 scripts/verify_audit2_ui.py:373-376 的电源长按
  - 没有测试点击 `[data-quick-key]`
  - web/js/app/08-input.js:1070-1096 mapAbsolute、1107-1141 相对移动、1363 按键映射、1392 滚轮
- Problem: KVM 最核心的输入路径没有回归保护。
- Why it matters: 按键映射、坐标偏移或滚轮方向出错时所有套件仍然全绿。
- Realistic failure scenario: 重构把左右键写反，或 Ctrl+Alt+Del 少发 Alt，CI 通过，用户在真机上才发现。
- Minimal fix: 新增场景：点击已知坐标断言 x/y、左右中键、滚轮符号、相对移动钳位、每个快捷键的载荷。
- Better long-term fix: 把输入映射抽成可单测的纯函数。
- Regression test suggestion: 上述场景本身。
- Estimated effort: 3-4 小时

### Finding: AUD3-03 没有任何 Release，且预发布 tag 不会被标记为预发布

- Severity: Medium
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 发布流程 / 文档
- Evidence:
  - `gh api repos/XiNian-dada/WingmanKVM/releases` 返回 0 个，`releases/latest` 返回 404，远端没有 tag
  - README.md 快速开始依赖 latest
  - .github/workflows/release.yml:201-205 的 gh release create 没有 --prerelease
  - src/web/update.rs:122 拒绝预发布和带后缀的 tag
- Problem: README 第一条命令目前必然失败；而 rc tag 发布时会成为 latest，web 更新检查随后报“不是可安装的正式版本”。
- Why it matters: 新用户的第一步就失败，rc 还会进入普通用户的安装路径。
- Realistic failure scenario: 用户照 README 安装得到 404；发布 rc 后，一行安装装上 rc，设置页的检查更新报错。
- Minimal fix: 带 `-` 的 tag 自动加 --prerelease；发布首个版本；README 注明 2.0 前需要 --version。
- Better long-term fix: 发布清单写入文档。
- Regression test suggestion: 工作流静态检查：tag 含 `-` 时命令带 --prerelease。
- Estimated effort: 0.5 小时

### Finding: AUD3-04 输入 WebSocket 断开时会重发服务端已执行的按键

- Severity: Medium
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 键鼠输入
- Evidence:
  - web/js/app/08a-input-socket.js:31-38,68-71 onclose 把所有未确认请求改走 HTTP
  - src/web/input_ws.rs:121-128 先 execute 再发送回复
- Problem: 服务端已执行但回复没送达时，客户端无法区分，会再发一次。
- Why it matters: Enter、点击等操作在被控机上执行两次，确认框场景后果明显。
- Realistic failure scenario: 用户在确认提示按 Enter，服务端已执行，Wi-Fi 切换导致连接断开，HTTP 重发，被控机收到两次 Enter。
- Minimal fix: 连接断开时只重放幂等操作（绝对移动、release_all），其余报“可能未送达”。
- Better long-term fix: 请求带序号，由服务端去重。
- Regression test suggestion: Playwright：服务端执行后关闭连接，断言没有 /api/key 重试且有提示。
- Estimated effort: 1-2 小时

### Finding: AUD3-05 配置保存被中断后，磁盘与运行中配置不一致

- Severity: Medium
- Confidence: Medium
- Category: Stability
- Status: Confirmed
- Affected area: 配置
- Evidence:
  - src/web/config_api.rs:867-894 commit_config 先写文件，等待 webrtc.reconfigure 后才更新内存
- Problem: 处理函数随客户端断开被取消时，停在写盘与更新内存之间。
- Why it matters: 之后的保存基于内存里的旧值，会悄悄写回旧配置。
- Realistic failure scenario: 保存码率后立刻关闭标签页，磁盘是新值，/api/config 仍是旧值；下一次无关保存把旧码率写回。
- Minimal fix: 把提交放进 tokio::spawn 并 await，断开连接不影响完成。
- Better long-term fix: —
- Regression test suggestion: 用永不返回的 reconfigure 桩中止处理函数，断言磁盘与内存一致。
- Estimated effort: 1-2 小时

### Finding: AUD3-06 在线升级开始时界面显示上一轮的结果

- Severity: Medium
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 在线升级
- Evidence:
  - deploy/wingmankvm-online-upgrade:222-229 --start 不清理旧状态
  - deploy/wingmankvm-online-upgrade.service RuntimeDirectoryPreserve=yes
  - src/web/update.rs:147 id 不进 JSON
  - web/js/app/12c-online-update.js:113-126 把 succeeded/failed 视为终态
- Problem: 新任务写入第一条状态前，轮询读到旧状态文件。
- Why it matters: 上一次失败时会提示“已回滚”而升级实际在进行；上一次成功时页面提前刷新。
- Realistic failure scenario: 上次升级失败，管理员重试，界面立刻显示失败并停止监视，服务随后意外重启。
- Minimal fix: --start 启动前删除状态文件。
- Better long-term fix: apply 返回上一轮 id，界面忽略旧 id。
- Regression test suggestion: 带旧状态文件的夹具，apply 后断言首次状态为 idle 或 running。
- Estimated effort: 1 小时

### Finding: AUD3-07 导入预览把存储的通知密钥明文返回给浏览器

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 配置备份 / 通知
- Evidence:
  - src/web/config_api.rs:136-142 config_changes 对完整 Config 做 diff
  - src/web/config_api.rs:175-186 预览未 inherit_secrets
  - web/js/app/13-settings-boot.js:153 显示 change.old
- Problem: /api/config 隐藏了这些值，预览和导入响应却把它们放在 old 字段；预览还显示成将被清空。
- Why it matters: 违反“密钥不回传浏览器”的设计，密钥会显示在屏幕上。
- Realistic failure scenario: 管理员导入一份普通备份，差异列表明文显示 Webhook 地址和令牌。
- Minimal fix: 预览也继承密钥，并对密钥路径打码，只显示已配置与否。
- Better long-term fix: ConfigChange 统一走 redacted_json。
- Regression test suggestion: 配置密钥后调用 preview，断言响应不含密钥。
- Estimated effort: 1 小时

### Finding: AUD3-08 ffmpeg_path 可指向任意可执行文件并以服务用户运行

- Severity: Medium
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 配置 / 编码器
- Evidence:
  - src/web/config_validate.rs:60-74 只要求绝对路径
  - src/devices/webrtc.rs:1303 直接作为 executable 运行
- Problem: 管理员会话或被盗会话可以把网页配置升级为服务用户下的任意程序执行。
- Why it matters: 服务用户可读 auth.json 和通知密钥，并可调用固定 sudo 规则，超出网页管理员的设计边界。不能直接拿到 root。
- Realistic failure scenario: 在受限终端里写一个脚本，把 ffmpeg_path 指向它，下次探测即以服务用户运行。
- Minimal fix: 网页接口拒绝修改 ffmpeg_path，只允许配置文件设置；或限制为 root 拥有且其他人不可写的文件。
- Better long-term fix: 启动时解析一次并固定。
- Regression test suggestion: PATCH 指向用户可写脚本，断言被拒绝。
- Estimated effort: 1-2 小时

### Finding: AUD3-09 未认证请求可以用失败登录冲刷审计日志

- Severity: Medium
- Confidence: Medium
- Category: Security
- Status: Confirmed
- Affected area: 审计
- Evidence:
  - src/web/passkey.rs:187-205 未知 challenge 记录 login_failed
  - src/web/login.rs:172,176 每 IP 每分钟 5 次，最多 4096 个 IP
  - src/audit.rs:24,163-190 总保留约 2 MiB，每条 sync_data
- Problem: 攻击者拥有大量来源 IP 时可在短时间内覆盖全部审计记录，每条 fsync 也会占用阻塞线程。
- Why it matters: 电源、配置、升级记录被冲掉后无法追查。I/O 占用部分尚未实测。
- Realistic failure scenario: 用 IPv6 /64 轮换地址发送失败登录，一分钟内旧的 power_action 记录被轮转掉。
- Minimal fix: 同一 IP 和时间窗的失败登录合并成一条。
- Better long-term fix: 高价值事件单独存文件。
- Regression test suggestion: 1 万次未知 challenge 后断言早先的 power_action 仍可读。
- Estimated effort: 半天

### Finding: AUD3-10 终端 WebSocket 没有心跳，代理空闲断开会杀掉 shell

- Severity: Medium
- Confidence: Medium
- Category: Stability
- Status: Suspected
- Affected area: 网页终端
- Evidence:
  - src/web/terminal.rs:257-310 不发 ping，socket 结束即 session.stop()
  - docs/DEPLOYMENT.md:554-559 的 nginx 示例未设 proxy_read_timeout
- Problem: 代理默认 60 秒空闲超时。
- Why it matters: 空闲的 nano 或静默的长任务被 SIGHUP 杀掉。
- Realistic failure scenario: 在 nginx 后打开终端运行静默任务，60 秒后连接断开，任务被杀。
- Minimal fix: 每 25 秒发送 Ping。
- Better long-term fix: —
- Regression test suggestion: 暂停时钟的 WebSocket 测试，断言 30 秒内收到 Ping。
- Estimated effort: 1 小时

### Finding: AUD3-11 p2b 的指针锁定和全屏检查只会跳过，不会失败

- Severity: Medium
- Confidence: High
- Category: Testing
- Status: Confirmed
- Affected area: UI 测试
- Evidence:
  - scripts/verify_p2b_ui.py:308-331,353-375 未进入状态就 skipped
  - scripts/verify_regressions.py:366-367,410 把跳过算作通过
- Problem: 应用即使不再调用 requestPointerLock，CI 也保持绿色。
- Why it matters: 相对鼠标路径很可能从未在 CI 里运行。
- Realistic failure scenario: 重构删掉 requestPointerLock 调用，所有检查仍显示通过。
- Minimal fix: 监视调用：未调用就失败，只有浏览器拒绝时才跳过。
- Better long-term fix: —
- Regression test suggestion: 移除调用后断言该检查失败。
- Estimated effort: 1 小时

### Finding: AUD3-12 verify_get_script、verify_mdns、verify_doctor 不在 CI 中

- Severity: Medium
- Confidence: High
- Category: Testing
- Status: Confirmed
- Affected area: CI
- Evidence:
  - .github/workflows/regression.yml 与 scripts/verify_regressions.py:43-53 均未引用这三个脚本
  - docs/TESTING.md:62-85 称它们为门禁
- Problem: 这些脚本只有手动运行才有用。
- Why it matters: 削弱安装器 SHA256 校验的改动不会被 CI 发现。
- Realistic failure scenario: 有人改坏 get.sh 校验逻辑，PR 全绿后合并。
- Minimal fix: 在 regression.yml 增加三个 run 步骤。
- Better long-term fix: —
- Regression test suggestion: CI 日志出现这些脚本的通过结果。
- Estimated effort: 0.5 小时

### Finding: AUD3-13 发布工作流不跑测试，也不依赖 CI 结果

- Severity: Medium
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 发布流程
- Evidence:
  - .github/workflows/release.yml 无 cargo test，构建任务没有依赖回归工作流
- Problem: tag 指向未通过测试的提交时也会发布。
- Why it matters: 坏版本会直接到达用户，在线升级还会推送给他们。
- Realistic failure scenario: 在未合并 CI 通过的提交上打 tag，Release 正常发布。
- Minimal fix: 发布前运行 cargo test --locked。
- Better long-term fix: 只允许 main 上已通过 CI 的提交发布。
- Regression test suggestion: 在故意失败的测试上试跑，断言不发布。
- Estimated effort: 1 小时

### Finding: AUD3-14 修改 ntfy 服务器后，原令牌会发给新服务器

- Severity: Low
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 通知
- Evidence:
  - src/web/config_api.rs:798-813 仅改 server 时保留 token
  - src/notify.rs:547-549 bearer_auth 发送
- Problem: 服务器与令牌没有绑定。
- Why it matters: 管理员会话可借此取出已隐藏的令牌。
- Realistic failure scenario: 把 server 改成自己的地址后点测试通知，令牌被发过去。
- Minimal fix: server 变化且未重新提供令牌时清空令牌。
- Better long-term fix: —
- Regression test suggestion: 改 server 后断言令牌已清空。
- Estimated effort: 0.5 小时

### Finding: AUD3-15 URL 下载不限制内网地址和跳转目标

- Severity: Low
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 虚拟介质
- Evidence:
  - src/web/media_download.rs:83-92 跳转策略只检查协议和次数
- Problem: 管理员可让设备请求局域网或本机地址。
- Why it matters: 结果只写入镜像目录，影响有限，但绕过了网络边界。
- Realistic failure scenario: 被盗会话请求内网管理地址并把响应存为镜像。
- Minimal fix: 默认拒绝环回和链路本地地址，内网可配置放行。
- Better long-term fix: —
- Regression test suggestion: 请求 127.0.0.1 断言被拒绝。
- Estimated effort: 1-2 小时

### Finding: AUD3-16 中断的上传和下载会留下隐藏文件

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 虚拟介质
- Evidence:
  - src/web/media.rs:88-95,117-129 只在 Drop 中清理
  - src/devices/media.rs:123 列表隐藏这些文件
- Problem: 崩溃或重启后，最大 16 GiB 的 .partial 文件无人清理。
- Why it matters: 与 config.json 同一文件系统，可能写满。
- Realistic failure scenario: 下载大镜像时服务重启，空间被悄悄占用。
- Minimal fix: 启动时和新上传前清理。
- Better long-term fix: —
- Regression test suggestion: 放置 partial 文件，启动后断言已删除。
- Estimated effort: 1 小时

### Finding: AUD3-17 进程退出时正在按下的键会一直保持按下

- Severity: Low
- Confidence: Medium
- Category: Stability
- Status: Confirmed
- Affected area: HID
- Evidence:
  - src/devices/hid.rs:720-722 按下、等待、释放
  - src/devices/hid.rs:320-353 启动不发零报告
  - 无 SIGTERM 处理
- Problem: 25 ms 窗口内重启会让按键卡住。
- Why it matters: 被控机持续自动重复该键。
- Realistic failure scenario: 升级重启恰好落在按键保持期间。
- Minimal fix: 启动时发送全零报告，并增加优雅退出。
- Better long-term fix: —
- Regression test suggestion: 假 hidg FIFO 断言构造时收到零报告。
- Estimated effort: 2 小时

### Finding: AUD3-18 USB 恢复后“键鼠重连中”提示不消失

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: HID 状态
- Evidence:
  - src/devices/hid.rs:262-279 只有成功写入才清除
  - src/web/status.rs:61-65
- Problem: 恢复判断依赖用户先输入。
- Why it matters: 用户等提示消失才敲键，会一直等下去。
- Realistic failure scenario: 拔插 USB 后界面持续显示重连中。
- Minimal fix: 设备节点可重新打开时清除。
- Better long-term fix: —
- Regression test suggestion: 恢复设备后不写入，断言标记清除。
- Estimated effort: 2 小时

### Finding: AUD3-19 视频采集失败不写日志，且每秒重试没有退避

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 视频
- Evidence:
  - src/devices/video.rs:398-426 错误只进 status.message
- Problem: journal 里看不到视频离线的原因。
- Why it matters: 排障困难，长期失败时持续重试。
- Realistic failure scenario: 采集卡接触不良，日志没有任何记录。
- Minimal fix: 错误变化时 warn，并加上限退避。
- Better long-term fix: —
- Regression test suggestion: 假设备失败，断言日志只出现一次且间隔增长。
- Estimated effort: 1 小时

### Finding: AUD3-20 通知和审计写入失败只进日志

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 可观测性
- Evidence:
  - src/notify.rs:469-476
  - src/audit.rs:138-142
  - src/web/status.rs:68 仅丢弃计数
- Problem: 界面看不到投递失败。
- Why it matters: 令牌失效后所有告警悄悄消失。
- Realistic failure scenario: Telegram 令牌被撤销，管理员以为告警正常。
- Minimal fix: /api/status 增加每通道 last_error 和失败计数。
- Better long-term fix: —
- Regression test suggestion: 假服务器返回 401，断言状态含错误。
- Estimated effort: 2 小时

### Finding: AUD3-21 保存视频设置后 H.264 会话永久退回 MJPEG

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 视频配置
- Evidence:
  - src/web/config_api.rs:870-883 任何 video 变化都重配
  - src/devices/webrtc.rs:509-519 关闭所有会话
  - web/js/app/07-video.js:261-268,343-346 不重试 H.264
- Problem: 改 JPEG 质量也会断开所有 H.264 观看者。
- Why it matters: 其他标签页画质下降且不恢复。
- Realistic failure scenario: 一个标签页改设置，另一个观看者一直停在 MJPEG。
- Minimal fix: 只在相关字段变化时重配；前端退避重试 H.264。
- Better long-term fix: —
- Regression test suggestion: 改 jpeg_quality 断言 H.264 会话保留。
- Estimated effort: 半天

### Finding: AUD3-22 升级事务不 sync，恢复时不校验快照

- Severity: Low
- Confidence: Medium
- Category: Release
- Status: Suspected
- Affected area: 升级事务
- Evidence:
  - deploy/upgrade-transaction.sh:245,252,322-329 未 sync
  - deploy/upgrade-transaction.sh:525-540 恢复不校验
- Problem: 断电后快照可能为空文件。
- Why it matters: 一键在线升级让这个窗口更容易被碰到，恢复可能写入空二进制。
- Realistic failure scenario: 升级中断电，recover.sh 恢复出空的 wingmankvm。
- Minimal fix: PENDING 发布前后 sync，清单记录 sha256 并在恢复时校验。
- Better long-term fix: —
- Regression test suggestion: 损坏快照后断言 recover.sh 拒绝。
- Estimated effort: 2 小时

### Finding: AUD3-23 terminal_is_ready 在异步运行时上同步读取 auth.json

- Severity: Low
- Confidence: High
- Category: Performance
- Status: Confirmed
- Affected area: 网页终端
- Evidence:
  - src/web/terminal.rs:21,35-42
- Problem: AUD2-18 的遗留点。
- Why it matters: 慢存储上短暂阻塞 worker。
- Realistic failure scenario: SD 卡繁忙时打开终端阻塞其他请求。
- Minimal fix: 改用 load_async。
- Better long-term fix: —
- Regression test suggestion: 代码检查即可。
- Estimated effort: 15 分钟

### Finding: AUD3-24 启动时没有完整校验 config.json

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 配置
- Evidence:
  - src/web/mod.rs:144
  - src/config.rs:59-76 只查版本、origin、server
- Problem: validate_config 只在写入时运行。
- Why it matters: 手改的非法值在启动时生效，之后还会阻塞无关保存。
- Realistic failure scenario: 手改 max_sessions 为 0 后 H.264 一直报满。
- Minimal fix: 启动时运行 validate_config 并明确报错。
- Better long-term fix: —
- Regression test suggestion: 非法配置启动，断言有明确错误。
- Estimated effort: 1 小时

### Finding: AUD3-25 登录失败审计记录原始用户名

- Severity: Low
- Confidence: High
- Category: Security
- Status: Confirmed
- Affected area: 审计 / 隐私
- Evidence:
  - src/web/login.rs:134-143
  - src/web/notifications.rs:17-19 注释指出用户可能把密码输入这一栏
- Problem: 审计日志可能含误输的密码，且可通过 /api/audit 读取。
- Why it matters: 敏感数据明文落盘。
- Realistic failure scenario: 用户把密码填进用户名栏，密码写入审计文件。
- Minimal fix: 只记录是否匹配或短哈希。
- Better long-term fix: —
- Regression test suggestion: 失败登录后断言审计不含原文。
- Estimated effort: 0.5 小时

### Finding: AUD3-26 延迟测量运行时可以启动粘贴和宏

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 延迟工具
- Evidence:
  - web/js/app/08c-latency.js:137 只有单向检查
  - web/js/app/08-input.js:612-650、web/js/app/07z-keyboard-macros.js:658-680 无反向检查
- Problem: Caps Lock 在键入过程中翻转。
- Why it matters: 粘贴的文本大小写错乱，包括密码。
- Realistic failure scenario: 启动测量后点开粘贴并发送密码。
- Minimal fix: 粘贴和宏在测量运行时拒绝启动。
- Better long-term fix: —
- Regression test suggestion: 测量运行中粘贴，断言被拒绝。
- Estimated effort: 0.5 小时

### Finding: AUD3-27 标签页关闭或会话过期时 Caps Lock 不恢复

- Severity: Low
- Confidence: High
- Category: Stability
- Status: Confirmed
- Affected area: 延迟工具
- Evidence:
  - web/js/app/08c-latency.js:178-189 finally 覆盖正常路径
  - web/js/app/08-input.js:1295 pagehide 只调用 releaseAll
- Problem: 奇数次切换后退出，状态保持反转。
- Why it matters: 被控机 Caps Lock 被留在相反状态。
- Realistic failure scenario: 测量中途关闭标签页。
- Minimal fix: pagehide 时按计数发送 keepalive 按键。
- Better long-term fix: —
- Regression test suggestion: 测量中触发 pagehide，断言发送恢复按键。
- Estimated effort: 0.5 小时

### Finding: AUD3-28 在线升级失败总是提示“已自动回滚”

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 在线升级界面
- Evidence:
  - web/js/app/12c-online-update.js:84
  - deploy/wingmankvm-online-upgrade:171-203 多数失败发生在安装之前
- Problem: 下载或校验失败时并没有回滚。
- Why it matters: 误导用户判断设备状态。
- Realistic failure scenario: 网络失败却显示已回滚。
- Minimal fix: 只有 install_failed 才显示回滚文字。
- Better long-term fix: —
- Regression test suggestion: 各错误码断言文案。
- Estimated effort: 15 分钟

### Finding: AUD3-29 “恢复默认”不重置自适应码率和质量浮层

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 设置
- Evidence:
  - web/js/app/13-settings-boot.js:353-361
  - web/js/app/00-settings.js:4-11
- Problem: 这两个键不在 SETTING_DEFAULTS 中。
- Why it matters: 提示已恢复默认，复选框却不变。
- Realistic failure scenario: 关闭自适应码率后恢复默认，仍然关闭。
- Minimal fix: 加入默认表并在重置时应用。
- Better long-term fix: —
- Regression test suggestion: 重置后断言复选框状态。
- Estimated effort: 15 分钟

### Finding: AUD3-30 Esc 不关闭下拉菜单，反而收起背后的面板

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 无障碍
- Evidence:
  - web/js/app/10-workspace-status.js:310-319
  - web/js/app/05-power.js:325-341
- Problem: 电源、语言、组合键菜单和诊断弹层只能点外部关闭。
- Why it matters: 键盘用户无法关闭菜单，焦点也不返回。
- Realistic failure scenario: 打开语言菜单按 Esc，右侧面板被收起。
- Minimal fix: 打开时 Esc 优先关闭并把焦点还给按钮。
- Better long-term fix: —
- Regression test suggestion: 每种菜单断言 Esc 关闭且焦点返回。
- Estimated effort: 0.5 小时

### Finding: AUD3-31 部分界面文字未翻译

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 国际化
- Evidence:
  - web/js/app/05-power.js:438,447,456 预设提示的 key 不存在
  - web/js/app/10-workspace-status.js:217
  - web/js/app/07z-keyboard-macros.js:681
  - web/js/app/09-terminal.js:102
  - web/index.html:272,585,608
- Problem: 英文界面仍出现中文。
- Why it matters: 影响英文用户。
- Realistic failure scenario: 英文界面点桌面优化预设，提示为中文。
- Minimal fix: 补齐 key 和短语。
- Better long-term fix: —
- Regression test suggestion: 检查每个 t() key 在三张表中都存在。
- Estimated effort: 1 小时

### Finding: AUD3-32 文档写的模块求值规则不成立，UI 测试也不监听页面错误

- Severity: Low
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 前端模块
- Evidence:
  - web/js/app/main.js:2-7、docs/TESTING.md:42-45 声称模块求值时不调用其他模块
  - web/js/app/06、11、12 顶层调用 latestOnly
  - scripts/ui_harness.py 无 pageerror 监听
- Problem: 目前靠函数提升和导入顺序才能工作。
- Why it matters: 改成 const 箭头函数就会启动崩溃，CI 只显示 8 秒超时。
- Realistic failure scenario: 重构 latestOnly 后白屏，测试日志没有堆栈。
- Minimal fix: 更正文档，harness 收集 pageerror 并断言为空。
- Better long-term fix: —
- Regression test suggestion: 注入抛错模块，断言测试失败且带错误信息。
- Estimated effort: 1 小时

### Finding: AUD3-33 测试基础设施的若干薄弱点

- Severity: Low
- Confidence: High
- Category: Testing
- Status: Confirmed
- Affected area: UI 测试
- Evidence:
  - scripts/verify_p2b_ui.py:94 未知接口默认返回 200
  - src/web_ui.rs:7-21 APP_CSS_PARTS 无完整性测试，而 scripts/ui_bundle.py:31-32 匹配整个目录
  - scripts/verify_perf_hud_playwright.py 调用的全局函数已不存在
  - 没有测试让真实二进制给未登录客户端提供 /assets/js/*
- Problem: 接口改名、CSS 漏注册都不会被发现。
- Why it matters: 测试里样式正常，发布的二进制却缺样式。
- Realistic failure scenario: 新增 CSS 文件忘记登记，CI 全绿，发布后样式缺失。
- Minimal fix: 未知接口返回 404；增加 CSS 完整性测试；修复或删除过时脚本。
- Better long-term fix: —
- Regression test suggestion: 各修复自身的断言。
- Estimated effort: 2 小时

### Finding: AUD3-34 文档与实际行为不符

- Severity: Low
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 文档
- Evidence:
  - docs/en/DEPLOYMENT.md:221-225 让用户从 journalctl 取令牌，但令牌不写日志
  - docs/DEPLOYMENT.md:646 链接指向 docs/docs/
  - CHANGELOG.md 称粘贴仅支持 US 布局，且缺多数已合并功能
  - UPGRADE_RECOVERY.md 被单元和脚本引用但不在发布包内
  - docs/en 缺 doctor 等章节
- Problem: 用户按文档操作会失败。
- Why it matters: 首次安装的英文用户拿不到令牌。
- Realistic failure scenario: 英文用户查 journalctl 找不到初始化链接。
- Minimal fix: 逐项修正，把恢复文档放进发布包。
- Better long-term fix: —
- Regression test suggestion: Markdown 链接检查进 CI。
- Estimated effort: 2 小时

### Finding: AUD3-35 发布包没有签名和来源证明

- Severity: Info
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 供应链
- Evidence:
  - release.yml 只发布 SHA256SUMS
- Problem: 校验和与文件来自同一 Release。
- Why it matters: 能发现损坏，不能防御账号被盗。已知并接受。
- Realistic failure scenario: GitHub 账号被盗后发布恶意版本。
- Minimal fix: 启用 artifact attestation。
- Better long-term fix: minisign 离线签名。
- Regression test suggestion: 验证证明。
- Estimated effort: 2 小时

### Finding: AUD3-36 未声明 rust-version

- Severity: Info
- Confidence: High
- Category: Release
- Status: Confirmed
- Affected area: 构建
- Evidence:
  - Cargo.toml 无 rust-version，代码使用 let-chains
- Problem: 低版本工具链报错不明确。
- Why it matters: 影响从源码构建的用户。
- Realistic failure scenario: 用旧 Rust 编译得到语法错误。
- Minimal fix: 声明 rust-version。
- Better long-term fix: —
- Regression test suggestion: CI 校验。
- Estimated effort: 15 分钟

### Finding: AUD3-37 生产环境暴露 window.__wingmanTest

- Severity: Info
- Confidence: High
- Category: Maintainability
- Status: Confirmed
- Affected area: 前端
- Evidence:
  - web/js/app/ 中设置 __wingmanTest 并暴露 ADAPTIVE_POLICY
- Problem: 测试钩子随产品发布。
- Why it matters: 风险低，页面脚本已同源可信。
- Realistic failure scenario: —
- Minimal fix: 改为只在测试注入时存在。
- Better long-term fix: —
- Regression test suggestion: 生产页面断言不存在。
- Estimated effort: 0.5 小时

### Finding: AUD3-38 自适应码率在容量边界上会周期性探测

- Severity: Info
- Confidence: Medium
- Category: Performance
- Status: Confirmed
- Affected area: 自适应码率
- Evidence:
  - src/devices/webrtc.rs:75,839-848
  - web/js/app/07c-adaptive-bitrate.js:29-37
- Problem: 控制器阻尼良好，但约每 40 秒一降一升。
- Why it matters: 每次变化重启 FFmpeg，有短暂卡顿。
- Realistic failure scenario: 带宽正好卡在档位之间。
- Minimal fix: 探测失败后指数退避。
- Better long-term fix: —
- Regression test suggestion: 模拟边界带宽，断言探测间隔增长。
- Estimated effort: 1 小时

## 5. Architecture Concerns

- Coverage: Medium
- Inspected evidence: 模块依赖、状态归属、模块求值顺序
- Exclusions / limits: 未逐文件评估全部 Rust 模块

相关问题：AUD3-32。依赖方向清晰，主要风险是模块求值顺序依赖函数提升。

已核实没有问题：设备层不依赖 web 层；路由集中在 mod.rs。

## 6. Security Concerns

- Coverage: Medium
- Inspected evidence: 路由与中间件、配置校验、URL 下载、通知、sudo 规则、安装器与升级事务
- Exclusions / limits: 安全子代理报告未交付；proxy_protocol.rs、client_ip.rs 本轮未复读；未做渗透测试；doctor 只按关键词扫描

相关问题：AUD3-07、AUD3-08、AUD3-09、AUD3-14、AUD3-15、AUD3-25。都需要管理员会话或大量来源 IP，没有未认证的代码执行路径。

已核实没有问题：所有受保护路由经 require_auth；修改类请求有同源检查；通知客户端不跟随跳转且错误不含 URL；改密码辅助脚本只读标准输入；wingman profile 只在账号新建时写入。

## 7. Stability Concerns

- Coverage: High
- Inspected evidence: WebRTC、音频、视频、HID、配置提交、审计、升级状态
- Exclusions / limits: 竞态未在运行时复现；无真实硬件

相关问题：AUD3-04、AUD3-05、AUD3-06、AUD3-10、AUD3-16、AUD3-17、AUD3-18、AUD3-19、AUD3-20、AUD3-21、AUD3-24、AUD3-26、AUD3-27。按键重放和配置取消影响最大。

已核实没有问题：WebRTC 所有退出路径都会清理；编码器进程组被回收；音频引用计数平衡；限流表有界；审计总量有界；AUD2-06、15、22、32、33 没有回退。

## 8. Performance Concerns

- Coverage: Low
- Inspected evidence: 阻塞调用、锁、自适应码率逻辑
- Exclusions / limits: 没有运行时 profiling

相关问题：AUD3-23、AUD3-38。没有实测数据，结论只来自代码阅读。

## 9. Testing Gaps

- Coverage: High
- Inspected evidence: 全部 UI 套件、CI 编排、362 项 Rust 测试实跑
- Exclusions / limits: 未读 p2c 脚本内部

相关问题：AUD3-02、AUD3-11、AUD3-12、AUD3-33。核心鼠标路径和相对移动是最大缺口。

## 10. Maintainability Concerns

- Coverage: Medium
- Inspected evidence: 文件规模、clippy 31 条警告
- Exclusions / limits: 未做函数级度量

相关问题：AUD3-28、AUD3-29、AUD3-30、AUD3-31、AUD3-32。src/devices/webrtc.rs 3188 行（含测试）、src/config.rs 1978 行仍偏大，但职责单一，本轮不建议拆分。

## 11. Design / Principles Concerns

- Coverage: Medium
- Inspected evidence: 对照 rubrics/principles.md 抽查
- Exclusions / limits: 未逐条穷举

相关问题：AUD3-05、AUD3-08、AUD3-24。校验只在写入时做，属于 fail-fast 缺口。

## 12. Type Safety Concerns

- Coverage: Medium
- Inspected evidence: unsafe 20 处均为 libc/fd 调用，边界解析
- Exclusions / limits: 未逐处证明

相关问题：无。unsafe 共 20 处，都是 libc 与文件描述符调用；未发现对不可信输入的 unwrap。

## 13. Release Concerns

- Coverage: High
- Inspected evidence: release.yml、regression.yml、打包命名、版本检查、远端 Release 状态
- Exclusions / limits: 未实际发布

相关问题：AUD3-01、AUD3-03、AUD3-13、AUD3-22、AUD3-34、AUD3-36。发布链路从未真实跑通，是本轮最需要先修的部分。

## 14. Documentation Analysis

- Coverage: Medium
- Inspected evidence: README、DEPLOYMENT、TESTING、CHANGELOG 中英文对照
- Exclusions / limits: DESIGN.md 未读

相关问题：AUD3-34。Subtype 分布：UserDocs 2、OperatorDocs 1、StaleDocs 2。音频文档与实现一致，无 /assets/app.js 等过期引用。

## 15. Observability / Operability Analysis

- Coverage: Medium
- Inspected evidence: /api/status、日志点、通知失败路径
- Exclusions / limits: 未看真实日志

相关问题：AUD3-19、AUD3-20。Subtype 分布：Logging 1、Alerting 1。

## 16. Configuration Safety Analysis

- Coverage: Medium
- Inspected evidence: config.rs、config_validate.rs、启动校验
- Exclusions / limits: 未穷举所有键

相关问题：AUD3-05、AUD3-08、AUD3-24。Subtype 分布：SchemaValidation 2、UnsafeDefault 0。默认安全：音频、mDNS 都是默认关闭。

## 17. Data Integrity Analysis

- Coverage: Medium
- Inspected evidence: 原子写入、事务回滚、配置提交
- Exclusions / limits: 未做断电实测

相关问题：AUD3-05、AUD3-22。配置与认证写入都有文件和目录 fsync。

## 18. Privacy / Data Governance Analysis

- Coverage: Medium
- Inspected evidence: 审计内容、通知字段、配置导出
- Exclusions / limits: —

相关问题：AUD3-07、AUD3-25。配置导出已排除通知密钥。

## 19. Accessibility / UX Correctness Analysis

- Coverage: Low
- Inspected evidence: 键盘与焦点路径
- Exclusions / limits: 未用屏幕阅读器

相关问题：AUD3-30。焦点与键盘路径抽查，未使用屏幕阅读器。

## 20. Supply Chain / Reproducibility Analysis

- Coverage: Medium
- Inspected evidence: Action SHA 固定、锁文件、cargo audit、npm audit
- Exclusions / limits: GitHub 仓库设置未检查

相关问题：AUD3-01、AUD3-13、AUD3-35、AUD3-36。Action 固定 SHA，cargo audit 无漏洞。npm audit 4 个中危都在 ESLint 依赖链，只用于开发，不进发布包，不单列。

已核实没有问题：权限最小化，无 pull_request_target，构建使用 --locked，npm ci --ignore-scripts。

## 21. Cost / Resource Economics Analysis

- Coverage: Low
- Inspected evidence: 无界工作、存储保留
- Exclusions / limits: 无外部付费 API；未实测资源

相关问题：AUD3-09、AUD3-16。无外部付费 API，主要资源风险是磁盘。

## 22. AI / LLM Safety Analysis

- Coverage: Not assessed
- Inspected evidence: 全仓无模型调用、提示词或工具边界
- Exclusions / limits: 项目不含 AI/LLM 功能

相关问题：无。Not assessed：全仓没有模型调用、提示词、检索或工具边界。

## 23. Fallback / Defensive Code Analysis

- Coverage: Medium
- Inspected evidence: HTTP 回退、MJPEG 回退、静默吞错
- Exclusions / limits: —

相关问题：AUD3-04、AUD3-28。HTTP 回退是唯一造成实际错误行为的回退；MJPEG 回退行为正确且有提示。

## 24. Testing Authenticity Analysis

- Coverage: High
- Inspected evidence: mock 边界、跳过逻辑、默认响应
- Exclusions / limits: —

相关问题：AUD3-02、AUD3-11、AUD3-33。Valuable：回环 WebRTC 测试、p2c_upgrade、audit2 的业务场景。Suspicious：UI 测试中 WebRTC 全部是 FAKE_PEER。Missing：真实二进制的未登录资源加载、桌面鼠标映射。

## 25. Frontend State Analysis

- Coverage: High
- Inspected evidence: 脚本化比对 IIFE 与模块、shared 状态、init 顺序、监听器数量
- Exclusions / limits: 无真实设备

相关问题：AUD3-26、AUD3-27、AUD3-29、AUD3-32。ES modules 改造没有发现行为回退。

已核实没有问题：无 TDZ 错误；原共享变量都在 shared 或有单一所有者；init 顺序与旧片段一致；监听器数量一致。

## 26. Backend API Analysis

- Coverage: Medium
- Inspected evidence: 路由表、校验、错误响应
- Exclusions / limits: 未覆盖全部接口的错误分支

相关问题：AUD3-05、AUD3-07、AUD3-24。错误响应结构统一。

## 27. Dependency Weight Analysis

- Coverage: Low
- Inspected evidence: Cargo.toml、package.json
- Exclusions / limits: 未量化二进制体积

相关问题：无。本轮没有发现明确多余的依赖；webrtc-rs 较重但为核心功能所需。未量化二进制体积。

## 28. Code Consistency Analysis

- Coverage: Low
- Inspected evidence: clippy 与 ESLint 结果
- Exclusions / limits: 未做全仓人工比对

相关问题：无。clippy 31 条警告以可折叠 if 为主，没有实际风险；其中两处 MutexGuard 跨 await 在测试代码。

## 29. Comment Coverage Analysis

- Coverage: Low
- Inspected evidence: 抽查关键模块，发现模块求值规则注释失真
- Exclusions / limits: 未穷举

相关问题：AUD3-32。模块求值规则的注释与实际不符，其余抽查准确。

## 30. Principles Compliance

| Principle | Violations | Severity | Affected Areas |
|-----------|------------|----------|----------------|
| Fail-Fast | 2 | Low | AUD3-24、AUD3-11 |
| Least Privilege | 1 | Medium | AUD3-08 |
| DRY / 单一事实来源 | 1 | High | AUD3-01（文件名在工作流和客户端各写一份）|

遵守良好：最小权限（固定 sudo 参数）、默认安全、原子写入、边界校验集中。

## 31. Recommended Fix Order

### Fix Immediately

发布 rc 前必须完成：AUD3-01、AUD3-03。

### Fix Before Stable Release

AUD3-04、AUD3-05、AUD3-06、AUD3-07、AUD3-08、AUD3-02、AUD3-11、AUD3-12、AUD3-13、AUD3-22、AUD3-34。

### Schedule Later

AUD3-09、AUD3-10、AUD3-14、AUD3-15、AUD3-16、AUD3-17、AUD3-18、AUD3-19、AUD3-20、AUD3-21、AUD3-24、AUD3-33。

### Ignore for Now

AUD3-35、AUD3-36、AUD3-37、AUD3-38。

## 32. Quick Wins

AUD3-23、AUD3-25、AUD3-26、AUD3-27、AUD3-28、AUD3-29、AUD3-30、AUD3-31、AUD3-12：每项不超过 1 小时，能直接消除真实问题。

## 33. Long-term Refactor Plan

证据不支持大型重构。可以考虑把输入映射抽成纯函数（动机：可单测；风险低；用现有 UI 套件兜底）。
