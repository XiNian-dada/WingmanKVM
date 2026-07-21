# WingmanKVM

WingmanKVM 是一个面向 ARM Linux 开发板的轻量 KVM 服务。它把 USB HDMI 采集卡、USB Gadget 键鼠、GPIO 电源控制和 USB Mass Storage Gadget 组合成一个 Rust 二进制，并通过内嵌网页提供远程控制台。

项目的默认策略是：

- 视频优先使用 V4L2 MJPEG 直通，只复制有效 JPEG 数据，不解码、不重新编码。
- 只有明确选择“JPEG 压缩”时才启用全局解码/重编码管线。
- HID 使用 `O_NONBLOCK`、500 ms 写超时和有界 worker，保证按下/释放报告不会因 HTTP 请求取消而穿插或永久阻塞。
- 所有硬件路径默认未绑定，由首次设置向导扫描并由管理员确认。
- 运行期间只管理已经存在的 USB Gadget function，不自动重建整个 Gadget。

## 当前功能

- 首次启动初始化令牌和强密码管理员。
- Argon2id 密码哈希、12 小时内存会话、登录限速、SameSite 会话 Cookie。
- V4L2 mmap MJPEG 采集、最新帧广播、MJPEG HTTP 输出和断线重连。
- 可选 JPEG 质量压缩；默认不消耗重新编码 CPU。
- Boot Keyboard 8 字节报告和 Boot Mouse 4 字节报告。
- 键盘、鼠标、GPIO、视频设备和 Mass Storage LUN 自动扫描候选。
- `gpioset` 2.x 短按/长按电源脉冲，子进程回收和状态记录。
- ISO/IMG 流式上传、只读优先连接、正常弹出和显式强制弹出。
- 单文件内嵌 Web 控制台：可拖动/缩放窗口、特殊键、虚拟键盘、输入背压、视频设置和设备设置。

## 快速开始

开发环境：

```bash
cargo run
```

设备默认监听 `0.0.0.0:8080`。首次启动时，程序会在终端或 systemd journal 中打印一次性初始化令牌：

```text
首次初始化需要此令牌 setup_token=...
```

打开 `http://设备地址:8080/`，输入令牌并创建管理员。初始化成功后，令牌立即失效。

状态目录默认为 `/var/lib/wingmankvm`，开发时可覆盖：

```bash
WINGMANKVM_STATE_DIR=./wingmankvm-state cargo run
```

状态目录包含：

```text
config.json   硬件和服务配置，0600
auth.json     管理员账号和 Argon2id 哈希，0600
images/       上传的 ISO/IMG
```

## ARM Linux 构建依赖

Debian/Ubuntu/Armbian 示例：

```bash
sudo apt install build-essential pkg-config clang libclang-dev linux-libc-dev libgpiod-tools
cargo build --release
sudo install -m 0755 target/release/wingmankvm /usr/local/bin/wingmankvm
```

`v4l 0.14` 的底层绑定会在构建时运行 bindgen，因此交叉编译环境必须能找到目标 Linux 的 `linux/videodev2.h`、clang 和 libclang。详见 [部署说明](docs/DEPLOYMENT.md)。

## HTTP 接口

公开接口：

```text
GET  /
GET  /healthz
GET  /api/bootstrap
GET|POST /api/setup/devices
POST /api/setup
POST /api/login
```

登录后接口：

```text
GET  /video_feed
GET  /api/status
GET|POST /api/devices/scan
GET|PUT|POST /api/config
POST /api/logout

POST /api/key
POST /api/mouse/move
POST /api/mouse/click
POST /api/mouse/scroll
POST /api/input/release-all

POST /power

GET  /api/media
POST /api/media/upload
POST /api/media/attach
POST /api/media/detach
```

`PUT /api/config` 替换完整配置；`POST /api/config` 合并网页使用的局部配置。

## USB Gadget 前提

程序假设 Gadget 已由系统启动脚本创建。BIOS/UEFI 兼容配置至少需要：

键盘：

```text
subclass = 1
protocol = 1
report_length = 8
```

鼠标：

```text
subclass = 1
protocol = 2
report_length = 4
```

虚拟介质配置填写 `mass_storage.*` 下的 `lun.0` 目录，而不是 `lun.0/file` 文件。切换介质只修改现有 LUN，不会解绑 UDC，因此不会主动让键鼠一起掉线。

## 安全边界

- 不要把直接使用 HTTP 的控制台暴露到不可信网络。建议放在可信管理网，或使用提供 HTTPS 的反向代理/VPN。
- Web 进程不应长期以 root 运行。视频、HID 和 GPIO 应通过 group/udev ACL 授权。
- configfs Mass Storage 通常需要额外权限。推荐由 Gadget 初始化脚本只授权目标 LUN 属性；更严格的低权限 helper 会在后续版本实现。
- GPIO 自动扫描无法知道物理上哪一根线连接了继电器，必须由管理员选择并现场验证。
- 可写 IMG 不得同时在 ARM 本机以读写方式挂载，否则可能损坏文件系统。ISO 始终按只读光盘连接。

## 尚未包含

- 动态创建或重组整个 USB Gadget。
- H.264/WebRTC 视频管线。
- 多用户/细粒度权限。
- 非美式键盘布局的字符层转换和 Unicode 文本粘贴。
- 独立特权 helper 进程。

这些边界是有意保留的：首个版本优先把 MJPEG 直通、HID 超时、输入范围和可恢复配置做可靠。
