# WingmanKVM

WingmanKVM 是一个运行在 ARM 开发板上的网页 KVM。把被控机的 HDMI 输出接到采集卡、USB 接到开发板的 OTG 口，就可以在浏览器里远程看画面、操作键鼠（包括 BIOS 阶段）、挂载 ISO 镜像，并可选地通过 GPIO 继电器控制电源。

单个二进制部署，不需要额外的数据库或前端构建工具。

![WingmanKVM 控制台](docs/images/console-desktop.png)

## 功能

**画面**
- 自动选择传输方式：优先 WebRTC H.264（支持 RK3399 硬件编码），不可用时回退 MJPEG
- 适应窗口 / 1:1 像素 / 填满三种显示方式，1:1 下可拖动平移
- 画面截图（PNG）与本地录屏（WebM）

**键盘与鼠标**
- 模拟 USB 键盘、相对鼠标和绝对指针，BIOS / UEFI 中可用
- 键鼠事件走 WebSocket 长连接，断开时自动回退 HTTP；连接中断时服务端自动释放所有按键
- 粘贴为键入：把本机剪贴板文字逐字输入被控机，支持 US / UK / DE / FR / JP 布局
- 组合键菜单（Ctrl+Alt+Del、Ctrl+Alt+F1–F7 等）与自定义组合
- 虚拟键盘、键盘指示灯同步、手机触屏手势（点按、长按右键、双指滚动）

**虚拟介质与电源**
- 网页上传 ISO / IMG，以光驱或 U 盘方式挂载给被控机
- 可选 GPIO 电源控制：短按、复位、强制关机（强制关机需按住 1 秒确认）

**账户与运维**
- 管理员密码 + Passkey（Touch ID / Face ID / Windows Hello / 安全密钥）登录
- 诊断面板：视频与编码状态、键鼠写入成功/失败、主控机 CPU / 内存 / 温度
- 内置网页终端（受限账户），切到视频页时会话保留
- 界面支持简体中文、繁體中文、English，浅色 / 深色主题

## 硬件要求

| 部件 | 要求 |
|---|---|
| KVM 主机 | Linux ARM 开发板（aarch64 或 armv7），需有支持 Device 模式的 USB OTG 口（内核提供 UDC 与 configfs）。已在 Firefly RK3399 上验证 |
| 采集卡 | UVC 免驱 HDMI 采集卡，需支持 MJPG 输出 |
| 电源控制（可选） | 继电器模块，干接点并联到被控机主板的电源 / 复位针脚 |
| 系统 | Debian 12 / Armbian bookworm 或更新（发布包要求 glibc ≥ 2.34） |

```text
被控机 HDMI 输出  ──>  HDMI 采集卡  ──USB──>  KVM 主机 USB Host 口
被控机 USB 口     <──  USB 线  ────────────>  KVM 主机 OTG 口（通常是 Type-C）
被控机电源针脚    <──  继电器干接点  ──────>  KVM 主机 GPIO（可选）
浏览器            <──  局域网  ────────────>  http://<KVM 主机 IP>:8080
```

## 快速开始

1. 从 [Releases](https://github.com/XiNian-dada/WingmanKVM/releases) 下载对应架构的发布包和 `SHA256SUMS`，放在同一目录：
   - ARM64：`wingmankvm-<版本>-aarch64-unknown-linux-gnu.tar.gz`
   - 32 位 ARM：`wingmankvm-<版本>-armv7-unknown-linux-gnueabihf.tar.gz`
2. 校验、解压并安装：

   ```bash
   sha256sum -c --ignore-missing SHA256SUMS
   mkdir wingmankvm && tar -xzf wingmankvm-<版本>-<架构>.tar.gz -C wingmankvm
   cd wingmankvm && sudo ./deploy/install.sh --release ../wingmankvm-<版本>-<架构>.tar.gz
   ```

3. 安装结束时终端会打印一次性初始化链接 `http://<IP>:8080/#setup=...`。在同一局域网的浏览器中打开，设置管理员账号并确认自动检测到的设备，即可进入控制台。

安装器会配置 systemd 服务、USB Gadget（键盘、鼠标、绝对指针、虚拟介质）和权限规则；升级时保留回滚快照，失败自动回滚。运行依赖、从源码构建和排障见 [部署说明](docs/DEPLOYMENT.md)。

## 安全建议

WingmanKVM 能直接操作被控机，请把它当作服务器的物理控制台对待：

- 不要把 8080 端口直接暴露到公网。远程访问请使用 VPN（如 WireGuard、Tailscale），或在 HTTPS 反向代理之后部署并配置 `server.public_origin`（见 [会话 Cookie 策略](docs/SESSION_COOKIES.md)）。
- 网页终端默认使用无 sudo 权限的受限账户；需要 root 维护时再显式开启，见 [维护权限模式](docs/MAINTENANCE_MODES.md)。

## 文档

- [部署说明](docs/DEPLOYMENT.md)：依赖、源码构建、配置、反向代理、排障
- [Firefly RK3399 部署实战](docs/RK3399_GUIDE.md)
- [升级与恢复](docs/UPGRADE_RECOVERY.md)
- [维护权限模式](docs/MAINTENANCE_MODES.md) · [会话 Cookie 策略](docs/SESSION_COOKIES.md) · [上传限制](docs/UPLOAD_LIMITS.md) · [HID 配置迁移](docs/HID_CONFIGURATION.md)
- [测试说明](docs/TESTING.md)
- [变更日志](CHANGELOG.md)

## 开发

```bash
cargo build
cargo test --locked
python3 scripts/verify_audit2_ui.py   # 需要 Playwright 与本机 Chrome
```

前端源码在 `web/`（HTML、按功能拆分的 CSS 与 JS），编译时嵌入二进制。测试与 CI 详见 [测试说明](docs/TESTING.md)。

## 许可证

[MIT](LICENSE)
