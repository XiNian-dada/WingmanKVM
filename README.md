# WingmanKVM

WingmanKVM 是运行在 Linux 开发板或主机上的网页 KVM：把 HDMI 采集卡、USB Gadget 和可选 GPIO 电源控制组合起来，在浏览器中查看画面、操作键鼠、控制电源和挂载 ISO/IMG。

> 第一次部署只需阅读下面的“部署前确认”和“四步部署”。遇到板卡、USB OTG 或 GPIO 问题时，再打开文末对应的专项文档。

## 部署前确认

完整 KVM 需要以下硬件条件：

- 一台运行 Debian、Ubuntu 或 Armbian 等 Linux 发行版的主机；
- 一个可用或可切换到 **Device/OTG 模式**的 USB 控制器（Linux 中能看到 UDC）；
- 一个 USB Host 口连接 HDMI 采集卡；
- 被控机与 KVM 主机之间的一根 OTG/Device USB 线，用于键盘、鼠标和虚拟介质；
- 可选：接到继电器或隔离电路的 GPIO，用于模拟被控机电源按钮。

没有 UDC/OTG 的普通 x86 主机不能使用官方的一键完整 KVM 安装；它不能向被控机模拟 USB 键盘、鼠标或 U 盘。

### 接线概览

```text
被控机 HDMI 输出 ──> HDMI 采集卡 ──USB──> KVM 主机 Host 口
被控机 USB Host  <──USB OTG/Device──> KVM 主机 OTG/Device 口
被控机 POWER SW <──继电器干接点──> KVM 主机 GPIO（可选）
浏览器          <──────局域网──────> WingmanKVM :8080
```

GPIO 不应直接接到被控机主板电源针脚；请使用继电器干接点或合适的隔离电路。

## 四步部署

### 1. 在 KVM 主机准备依赖并构建

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev \
  libgpiod-tools v4l-utils

git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

需要安装近期稳定版 Rust（包含 `cargo`）。如果你在 Mac 或另一台机器上为 ARM 板构建，请使用[部署说明中的跨机器构建步骤](docs/DEPLOYMENT.md#2-构建-linux-二进制)。

### 2. 安装并启动

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

安装器会创建服务、USB Gadget、终端用户、设备权限和状态目录，然后监听 `0.0.0.0:8080`。

若安装器提示没有 UDC，先不要反复重试。确认 OTG 口接对，并按[UDC 与 USB role 指南](docs/DEPLOYMENT.md#4-udc-与板级-usb-role)配置板卡；需要先落盘配置时可使用 `--no-start`。

### 3. 打开首次设置链接

安装器会输出类似地址：

```text
http://192.168.1.20:8080/#setup=...
```

在同一局域网的浏览器中打开它。这个一次性令牌只在创建管理员前有效，页面读取后会自动从地址栏删除。

依次完成：

1. 创建管理员账号和强密码；
2. 在“检查连接”中确认自动检测到的采集卡、键盘、鼠标和虚拟介质；
3. 仅在已确认接线与触发电平后启用 GPIO 电源控制。

管理员密码会同步给网页终端中的 `wingman` 用户；终端中执行 `sudo` 时使用同一密码。

### 4. 确认服务正常

在 KVM 主机执行：

```bash
systemctl --no-pager --full status \
  wingmankvm-gadget.service wingmankvm.service
curl -fsS http://127.0.0.1:8080/healthz
```

`wingmankvm-gadget.service` 显示 `active (exited)` 是正常的；`wingmankvm.service` 应显示 `active (running)`。被控机接入 OTG 后，以下状态应为 `configured`：

```bash
cat /sys/class/udc/*/state
```

## 使用方式

网页默认使用低 CPU 占用的 MJPEG 直通，适合局域网。可在视频设置中按需切换 WebRTC H.264；它需要目标机上带相应编码器的 FFmpeg，详细条件见[低带宽与 WebRTC 说明](docs/RK3399_GUIDE.md#8-webrtc--h264-低带宽模式)。

网页还提供：

- 键盘、相对鼠标和绝对坐标鼠标；
- 短按和长按电源按钮；
- KVM 主机本机的交互式终端；
- ISO（只读）与 IMG（可选读写）虚拟介质。

写入型 IMG 在弹出前必须先从被控机安全卸载，不能同时被 KVM 主机和被控机读写。

## 常见下一步

| 你要做什么 | 请看 |
| --- | --- |
| Firefly RK3399 从接线到验证 | [RK3399 从零部署实例](docs/RK3399_GUIDE.md) |
| 配置 OTG/UDC、升级、服务检查 | [部署说明](docs/DEPLOYMENT.md) |
| 找采集卡、HID、GPIO 或虚拟介质 | [硬件配置与排障](docs/DEPLOYMENT.md#8-常见故障排查) |
| MS2130 虚拟显示器、EDID RAM 与安全回滚 | [MS2130 EDID 说明](docs/MS2130_EDID.md) |
| 使用 GPIO 控制电源 | [GPIO 电源按钮说明](docs/RK3399_GUIDE.md#9-gpio-电源按钮必须按接线确认) |
| 处理低上行带宽、VPN 或 WebRTC | [WebRTC/H.264 说明](docs/RK3399_GUIDE.md#8-webrtc--h264-低带宽模式) |
| 自定义 USB Gadget、VID/PID 或 descriptor | [高级 Gadget 检查](docs/DEPLOYMENT.md#9-高级自定义-gadget-检查) |

## 安全边界

- 不要直接把 `8080` 暴露到公网；优先使用 WireGuard、Tailscale 或可信管理网。
- 网页终端是 **KVM 主机** 的 shell，不是被控机 shell；能登录网页并知道管理员密码的用户可以通过 `sudo` 管理该主机。
- GPIO 自动扫描只能列出候选线路，无法判断物理接线、IO 电压或继电器极性。先用原理图和万用表确认。
- 一个 UDC 一次只能由一个 USB Gadget 管理；启用官方 Gadget 服务前，停用旧脚本或板厂服务。

## 更多文档

- [部署说明：构建、UDC、更新与排障](docs/DEPLOYMENT.md)
- [Firefly RK3399 从零部署实例](docs/RK3399_GUIDE.md)
- [设计与实现边界](DESIGN.md)
- [MIT 许可证](LICENSE)
