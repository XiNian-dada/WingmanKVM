# WingmanKVM

一个轻量、开放配置的网页 KVM。它运行在 Linux 主机或开发板上，把 USB HDMI 采集卡、USB HID Gadget、GPIO 继电器和可选的 USB Mass Storage Gadget 组合起来，让你在浏览器中完成：

- 查看被控机画面；
- 转发键盘、鼠标和滚轮；
- 使用绝对坐标指针，让网页中的指针位置与远端画面保持一致；
- 通过 GPIO 短按或长按电源键；
- 打开一个运行在 KVM 主机本机的真实交互式终端；
- 上传 ISO/IMG 并挂载为被控机的虚拟介质。

WingmanKVM 是 Rust 单二进制程序。视频默认走 V4L2 MJPEG 直通，不经过 OpenCV 解码和再次编码，尽量把 CPU 和内存留给被控机控制本身。

## 支持范围

### 软件平台

程序可以在 Linux `x86_64`、`aarch64`（ARM64）和 `armv7` 等架构上构建运行。常见的 RK3399、树莓派、其他 ARM SBC，以及带有相应 USB 控制器的 x86 小主机都可以作为 KVM 主机。

不过，**完整的 USB KVM 功能取决于硬件，而不是 CPU 架构**：

- 必须有可用的 USB Device/OTG 控制器（Linux UDC），才能把键盘、鼠标或存储功能暴露给被控机；
- 必须有 USB Host 口连接 HDMI 采集卡；
- GPIO 电源控制需要一条接到继电器或电源按键的可用 GPIO；
- 没有 OTG/UDC 的普通 x86 主机仍可运行网页、视频和部分调试功能，但不能凭空模拟 USB 键鼠或 U 盘。

GPIO 和虚拟介质都是可选模块。没有继电器或 Mass Storage Gadget 时，视频、键鼠和终端仍可单独使用。

### 硬件组成

| 模块 | 作用 | 必需性 |
| --- | --- | --- |
| USB HDMI 采集卡 | 提供被控机画面，暴露为 `/dev/video*` | 必需 |
| USB OTG/Device 控制器 | 让 KVM 主机模拟 USB 外设 | 键鼠/虚拟介质必需 |
| HID Gadget 键盘 | BIOS/UEFI 和系统键盘输入 | 键盘控制必需 |
| HID Gadget 相对鼠标 | BIOS/UEFI 兼容的鼠标输入 | 建议保留 |
| HID Gadget 绝对指针 | 桌面系统中的坐标同步 | 可选，推荐 |
| GPIO + 继电器 | 短按/长按被控机电源 | 电源控制可选 |
| Mass Storage Gadget LUN | 向被控机挂载 ISO/IMG | 虚拟介质可选 |

## 工作方式

```text
被控机 HDMI ──> USB HDMI 采集卡 ──> /dev/video*
                                      │ V4L2 mmap / MJPEG
                                      ▼
浏览器 <── HTTP MJPEG / WebSocket 终端 ── WingmanKVM
   │                                  │
   ├── 键盘/鼠标请求 ────────────────> /dev/hidg*
   ├── 电源请求 ────────────────────> GPIO / gpioset
   └── ISO/IMG ──> Mass Storage LUN ─> USB OTG ──> 被控机
```

USB Gadget 由系统启动脚本或发行版配置创建；WingmanKVM 会扫描和使用已经存在的 function，不会在每次启动时擅自重建整个 Gadget。这使项目可以适配不同板卡、不同 report descriptor 和不同 Gadget 拓扑。

## 功能

- V4L2 mmap 采集，支持 MJPEG 直通和可选 JPEG 重编码。
- 视频采集预设：4K、1440p、1080p、720p、480p、自定义；帧率可选 60/30/25/24/15 FPS 或自定义。
- 采集分辨率与浏览器显示缩放分离，支持适应窗口、原始像素、拉伸，以及像素锐利/平滑插值。
- Boot Keyboard、Boot Relative Mouse 和 Absolute Pointer；HID 写入非阻塞并带有界超时。
- 绝对指针按视频内容区域换算坐标，黑边区域不会误发鼠标请求。
- 可拖动、缩放、全屏的远程画面窗口；视频和终端在同一个工作区顶部切换。
- 内嵌 xterm.js + PTY 的本机终端，支持 ANSI、光标、颜色、方向键、Ctrl 组合键和窗口尺寸同步。
- GPIO 短按/长按电源脉冲（依赖 libgpiod 2.x `gpioset`）。
- ISO/IMG 上传、只读优先挂载、正常弹出和强制弹出。
- 首次启动向导、硬件扫描、管理员认证、Argon2id 密码哈希和会话限时。
- 所有硬件路径均可在网页中手动指定，不依赖固定的 `/dev/video5` 或 `/dev/hidg0` 编号。

## 快速开始

### 1. 安装构建依赖

Debian、Ubuntu、Armbian 示例：

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev \
  libgpiod-tools v4l-utils
```

需要近期的 stable Rust（包含 `cargo`）。`v4l` 在 Linux 构建时会通过 bindgen 使用 `linux/videodev2.h`、clang 和 libclang。

### 2. 构建并启动

```bash
git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release

# 直接运行，适合第一次验证
WINGMANKVM_STATE_DIR=./wingmankvm-state \
  ./target/release/wingmankvm
```

默认监听 `0.0.0.0:8080`。打开 `http://设备地址:8080/`，首次进入时按向导创建管理员并选择硬件。初始化令牌会写入启动日志；创建管理员后立即失效。

构建机不必和目标板同架构。在 Apple Silicon Mac 上可以用 Linux ARM64 容器构建，再把 `target/release/wingmankvm` 复制到 ARM 板；目标板只需要运行二进制，不需要安装 Rust 工具链。交叉编译的 sysroot 和 bindgen 注意事项见 [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md)。

生产环境建议使用 systemd 用户、udev 权限和持久化状态目录。完整安装示例见 [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md)。

### 3. systemd 部署（简要）

```bash
sudo install -m 0755 target/release/wingmankvm /usr/local/bin/wingmankvm
sudo install -m 0644 deploy/wingmankvm.service \
  /etc/systemd/system/wingmankvm.service
sudo systemctl daemon-reload
sudo systemctl enable --now wingmankvm
sudo journalctl -u wingmankvm -f
```

网页终端默认以本机用户 `wingman` 启动。需要使用终端的设备，请按部署文档创建该用户、安装 profile 和最小 sudo helper；首次网页初始化时会把 `wingman` 的系统密码同步为网页密码，不要把密码写进命令行或 systemd unit。

## 如何找到并配置硬件

首次向导会列出候选设备，但它只能发现设备节点，不能替你判断哪一根线接着继电器。建议先在设备上用下面的命令确认，再把结果填入网页。

### HDMI 采集卡

```bash
v4l2-ctl --list-devices
ls -l /dev/video*
v4l2-ctl -d /dev/video5 --list-formats-ext
```

选择实际对应的 `/dev/videoX`。优先选择支持 `MJPG` 的节点，并在网页中先使用“设备默认”测试；如果采集卡列出了多个节点，不要仅凭编号猜测用途。

### USB HID Gadget

```bash
ls -l /dev/hidg*
find /sys/kernel/config/usb_gadget -path '*/functions/hid.*' -type d -print
cat /sys/class/udc/*/uevent 2>/dev/null
```

建议的 function 参数：

| 接口 | `subclass` | `protocol` | `report_length` |
| --- | ---: | ---: | ---: |
| Boot Keyboard | `1` | `1` | `8` |
| Boot Relative Mouse | `1` | `2` | `4` |
| Absolute Pointer | `0` | `0` | `6` |

在网页“设备”面板中分别填写键盘、相对鼠标和绝对指针路径。默认指针模式为“自动”：有绝对接口时优先使用绝对坐标，没有时回退到相对鼠标。BIOS/UEFI 通常需要保留 Boot Keyboard 和 Boot Relative Mouse。

如果没有任何 `/dev/hidg*`，先检查内核是否启用了 USB Gadget/configfs，以及 Gadget 是否已经绑定到 UDC：

```bash
test -d /sys/kernel/config/usb_gadget && echo configfs-ok
ls -l /sys/class/udc
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
```

WingmanKVM 不会替你猜测或重建 report descriptor；Gadget 需要先由板卡启动脚本正确创建。

### GPIO 电源线

使用 GPIO 字符设备工具查找芯片和线路：

```bash
gpiodetect
gpioinfo gpiochip1
gpioset --version
```

网页中填写 `gpiochipN` 和线路号（line offset）。线路号不是排针上的物理脚位编号；请以 `gpioinfo` 和开发板原理图为准，并先断开被控机电源或使用万用表验证高低电平。`gpioset` 需要 libgpiod 2.x，短按和长按默认分别为 500 ms 与 5 s。

### USB Mass Storage LUN

虚拟介质要求 Gadget 已经创建 Mass Storage function 和 `lun.0`：

```bash
find /sys/kernel/config/usb_gadget \
  -path '*/functions/mass_storage.*/lun.0' -type d -print
```

把找到的 `lun.0` 目录填入“虚拟介质”面板，并指定镜像目录。只接受 `.iso` 和 `.img`；默认按只读连接。不要在 KVM 主机上以读写方式同时挂载正在提供给被控机的镜像。

## 配置和状态

默认状态目录为 `/var/lib/wingmankvm`，也可以通过环境变量覆盖：

```bash
WINGMANKVM_STATE_DIR=/path/to/state ./target/release/wingmankvm
```

其中包括：

```text
config.json   硬件和视频/输入配置（0600）
auth.json     管理员账号与 Argon2id 哈希（0600）
images/       上传的 ISO/IMG
```

常用检查接口：

```bash
curl -fsS http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/api/bootstrap
```

主要登录后接口包括 `/video_feed`、`/api/status`、`/api/devices/scan`、`/api/key`、`/api/mouse/*`、`/api/terminal/ws`、`/power` 和 `/api/media/*`。接口细节以当前源码和网页行为为准。

## 安全注意事项

WingmanKVM 能够控制电源、键盘、鼠标并向 USB 总线提供磁盘镜像，应当按基础设施控制面来部署：

- 不要把 8080 端口直接暴露到公网；优先使用可信管理网、WireGuard/Tailscale，或在 Caddy/Nginx 后终止 HTTPS。
- 首次启动后立即设置强密码；不要分享 setup token、会话 Cookie 或 systemd 日志中的敏感信息。
- 生产环境使用专用 `wingmankvm` 用户和 `wingmankvm-hw` 组，通过 udev 只授权实际使用的 `/dev/video*`、`/dev/hidg*` 和 GPIO 节点。示例规则见 [`deploy/99-wingmankvm.rules`](deploy/99-wingmankvm.rules)。
- GPIO 自动扫描只能列出芯片和线路，不能判断电气连接。变更线路前先确认继电器逻辑和 `active_high`。
- 虚拟介质默认只读。可写 IMG 不要被 KVM 主机和被控机同时挂载。
- 网页终端是 KVM 主机本机 shell，不是被控机终端；请按最小权限配置 `wingman` 用户。

## 当前边界

当前版本有意保持简单：

- 不动态创建或重组整个 USB Gadget；
- 不提供 H.264/WebRTC 视频管线；
- 不包含多用户/细粒度 RBAC；
- 终端字符输入按 PTY/键盘事件工作，不负责把任意 Unicode 文本转换成目标机键盘布局；
- x86 主机若没有 USB Device/OTG 控制器，不能提供 USB HID/存储模拟。

## 文档与许可证

- 设计与实现边界：[`DESIGN.md`](DESIGN.md)
- Linux、Gadget、udev 和 systemd 部署：[`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md)
- 许可证：[`MIT`](LICENSE)

欢迎提交 issue、硬件适配经验和改进建议。
