# WingmanKVM

轻量、现代的 Linux Web KVM：将 HDMI 采集卡、USB Gadget 与 GPIO 电源控制整合为一体，只需在浏览器中即可实现远程屏幕查看、BIOS 级键鼠操作、物理电源控制与 ISO/IMG 虚拟介质挂载。

![WingmanKVM 控制台主界面](docs/images/console-desktop.png)

---

## 硬件连接概览

```text
被控机 HDMI 输出  ──>  HDMI 采集卡  ──USB──>  KVM 主机 USB Host 口
被控机 USB Host   <──  USB OTG 线  ────────>  KVM 主机 Type-C / OTG 口
被控机 POWER SW   <──  继电器干接点 ────────>  KVM 主机 GPIO 针脚（可选）
浏览器终端        <──────  局域网网络  ──────>  http://<KVM主机IP>:8080
```

- **视频采集**：HDMI 采集卡插入 KVM 主机的 USB Host 接口。
- **键鼠与介质**：KVM 主机的 OTG/Device 接口（通常为 Type-C）连接被控机 USB 接口，模拟 USB 键盘、鼠标与虚拟 U 盘。
- **电源控制（可选）**：通过 GPIO 控制继电器模块，并联至被控机主板电源针脚。

---

## 极速开始（三步搞定）

### 1. 准备依赖并构建

在 KVM 主机（Debian / Ubuntu / Armbian）上执行：

```bash
sudo apt update
sudo apt install -y build-essential pkg-config clang libclang-dev linux-libc-dev libgpiod-tools v4l-utils

git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

### 2. 一键安装与启动

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

安装脚本会自动配置 systemd 服务、USB Gadget 复合设备（键盘、相对鼠标、绝对指针、虚拟介质）、权限规则，并启动监听在 `8080` 端口。

新装默认使用**受限网页终端**，`wingman` 不拥有 sudo 权限。需要主机 root 维护时，由主机管理员显式传入 `--maintenance-mode privileged`；旧版已有 root 授权的升级必须明确选择模式，详见[维护权限模式](docs/MAINTENANCE_MODES.md)。升级会保留整组托管文件的回滚快照，并检查 `/healthz`，详见[升级与恢复](docs/UPGRADE_RECOVERY.md)。

### 3. 打开网页完成初始化

安装完成后终端会输出一次性初始化地址：

```text
http://<你的主机IP>:8080/#setup=...
```

在同一局域网的电脑或平板浏览器中打开该地址，完成两步向导：

1. **设置管理员账号与强密码**（密码同时同步给主机内置终端用户 `wingman`）；
2. **确认自动检测到的硬件设备**（采集卡、HID 键鼠、虚拟介质 LUN）。

完成即可直接进入控制台接管远端机器！

#### 终端密码同步与降级说明

初始化或迁移旧版账号时，WingmanKVM 会尝试把管理员密码同步给内置终端用户 `wingman`。helper 的输入与等待期限为 5 秒，超时后最多再等待 2 秒回收进程。密码只通过标准输入传递，不会以明文写入 `auth.json` 或日志。

升级时须同时更新部署 helper：它使用 GNU coreutils 的 `/usr/bin/timeout`，在提权侧对 `chpasswd` 设置 3 秒期限，并在额外 1 秒后强制终止，避免依赖服务用户终止 root 进程。

- 同步成功时，`auth.json` 仅记录 `system_password_synced: true`，网页终端可用。
- 同步失败或超时时，初始化/KVM 登录仍会完成，画面、键鼠、电源与虚拟介质不受影响；但网页终端会保持不可用，并显示通用原因，终端 WebSocket 返回 `503`。首次设置不会因此失败，登录也不会被误报为完全就绪。
- 修复后使用同一正确密码**退出并重新登录**即可重试；成功后标记为已同步，后续登录直接跳过 helper。Passkey 登录不会在标记为 false 时伪造终端就绪状态。
- 并发登录只允许一个迁移尝试运行；其他登录立即以终端降级状态完成，不会排队等待。

#### 监听地址配置

`server.listen_address` 接受不带方括号的 IPv4 或 IPv6 字面地址，`server.port` 范围为 1–65535；加载与保存使用相同校验。通过完整配置 PUT 修改监听地址或端口后，需要重启服务才会重新绑定；这类成功响应带有 `X-WingmanKVM-Restart-Required: true`。响应头表示本次提交更改了监听设置，不是当前进程监听状态的查询。

---

## 核心功能与界面一览

### 1. 极简首次配置与硬件自检

初始化向导自动扫描可用的 V4L2 采集卡与 USB Gadget 节点，无需手动翻查设备树或配置文件。

| 步骤一：创建管理员 | 步骤二：连接自检 | 硬件指引 |
| :---: | :---: | :---: |
| [![创建管理员](docs/images/setup-account.png)](docs/images/setup-account.png) | [![连接自检](docs/images/setup-detection.png)](docs/images/setup-detection.png) | [![硬件指引](docs/images/setup-guide.png)](docs/images/setup-guide.png) |

---

### 2. 沉浸式深浅色控制台

支持日间明亮模式与极客质感的深色模式（Dark Mode），提供常用快捷键（`Ctrl+Alt+Del`、`Win`、`Alt+Tab`、`Esc`）与键盘状态锁指示灯（NumLock、CapsLock、ScrollLock 双向同步）。

- **视频健康状态**：只有收到首个有效 JPEG 帧后才显示 `LIVE`。超过 5 秒没有有效帧（兼容 1 FPS 采集）会标记为“已停止更新”，清除缓存画面与 FPS，并显示最近有效帧的时间；恢复首帧后自动回到 `LIVE`，无需重新配置。
- **本地退出快捷键**：开启键鼠转发后，按 `Ctrl + Alt + 反引号`（Ctrl+Alt+Backquote）会立即在本地退出转发并释放所有按键与鼠标捕获；该组合键不会发送到被控机。`Tab`、`Shift+Tab` 与 `Esc` 在转发状态下仍照常发送。相对鼠标模式下聚焦画面按 `Enter` 可捕获鼠标，锁定后 `Enter` 恢复为远程按键。
- **UI 回归验证**：运行 `python3 scripts/verify_p2b_ui.py --repo . --output-dir /tmp/wingmankvm-p2b` 可用 Playwright 驱动真实 Chrome、配合模拟 API 验证上述视频健康状态与本地退出快捷键；`--browser-channel` 可切换浏览器通道，JSON 报告与失败截图写入输出目录。

| 浅色模式 | 深色模式 |
| :---: | :---: |
| [![浅色控制台](docs/images/console-desktop.png)](docs/images/console-desktop.png) | [![深色控制台](docs/images/console-dark.png)](docs/images/console-dark.png) |

---

### 3. 虚拟软键盘与全键位支持

平板、手机或移动端无需外接键盘，点击控制台顶部的“键盘”即可展开标准全尺寸虚拟键盘，支持 8 字节 Boot HID 规范与全部功能键。

![虚拟全键盘](docs/images/virtual-keyboard.png)

---

### 4. 虚拟介质挂载（ISO / IMG）

无需频繁插拔物理 U 盘。支持在网页中直接上传 ISO/IMG 镜像，一键挂载为只读系统安装光驱或读写存储设备，被控机可直接开机引导或读写数据。

![虚拟介质控制面板](docs/images/media-panel.png)

---

### 5. 灵活画质调节与低延迟直通

默认采用超低延迟的 MJPEG 直通模式，局域网体验丝滑流畅；同时支持一键切换预设模式（桌面优化 1080p 60fps、BIOS / UEFI、低带宽省流）或按需启用 WebRTC H.264 编码。

视频设置中的尺寸与帧率只控制采集和网络传输；被控机的桌面分辨率需要在其操作系统中设置。当前版本不提供虚拟显示器或 EDID 写入功能。

![视频设置面板](docs/images/video-panel.png)

---

## 硬件实战与专题指南

- **Firefly RK3399 部署实战**：查看 [Firefly RK3399 从零部署指南](docs/RK3399_GUIDE.md)，包含详细接口对应、系统配置与避坑速查。
- **采集尺寸与显示器分辨率**：查看 [MS2130 EDID 历史调研与旧配置兼容说明](docs/MS2130_EDID.md)。
- **高级自定义配置**：查看 [系统部署与进阶排障说明](docs/DEPLOYMENT.md)。

---

## 许可证

基于 [MIT 许可证](LICENSE) 开源。
