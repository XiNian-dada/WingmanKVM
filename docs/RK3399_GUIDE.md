# Firefly RK3399 从零部署实例

这是一份面向第一次接触 Linux USB Gadget 的实例教程。示例机器是 Firefly RK3399，运行 ARM64 Armbian / Debian；教程中的探测过程来自一台已经验证可用的实例机。

下面出现的 `/dev/video5`、`fe800000.usb`、`gpiochip1` 和 line 7 都只是这台实例机的结果，不是 WingmanKVM 的通用默认值。换开发板、内核、USB 口或接线后，编号都可能不同，必须先探测再填写。

## 1. 先分清四条连接

```text
被控机 HDMI 输出 ──> HDMI 采集卡 ──USB──> RK3399 的 USB Host 口
被控机 USB Host 口 <──────────────USB──> RK3399 的 OTG / Device 口
被控机电源按键   <────继电器干接点────> RK3399 GPIO 控制侧
浏览器           <──────局域网────────> RK3399
```

- 采集卡接在 RK3399 的 **Host 口**，负责输入视频。
- 键盘、鼠标和虚拟 U 盘通过 RK3399 的 **OTG / Device 口**输出给被控机。
- Host 口和 Device 口不是同一个角色。采集卡能工作，不代表 USB Gadget 一定可用。
- GPIO 不应直接接到被控机电源针脚。应使用继电器或合适的隔离电路，让继电器干接点并联到被控机电源按钮。

先安装诊断工具：

```bash
sudo apt update
sudo apt install -y \
  sudo udev v4l-utils libgpiod-tools usbutils psmisc file curl
```

## 2. 用插入前后对比找到采集卡

RK3399 本身可能已经有多个 `/dev/video*`。它们可能是 VPU、解码器或图像处理单元，不能因为编号靠前就把 `/dev/video0` 当作采集卡。

### 2.1 采集卡未插入时保存基线

先拔掉 **HDMI 采集卡的 USB 线**，不要拔错 RK3399 与被控机之间的 OTG 线，然后执行：

```bash
mkdir -p ~/wingmankvm-probe
v4l2-ctl --list-devices > ~/wingmankvm-probe/devices-before.txt
ls -l /dev/video* > ~/wingmankvm-probe/nodes-before.txt 2>&1 || true
```

### 2.2 插入采集卡后再次保存

把采集卡插入 RK3399 的 USB Host 口，等待两三秒，再执行：

```bash
v4l2-ctl --list-devices > ~/wingmankvm-probe/devices-after.txt
ls -l /dev/video* > ~/wingmankvm-probe/nodes-after.txt 2>&1 || true

diff -u \
  ~/wingmankvm-probe/devices-before.txt \
  ~/wingmankvm-probe/devices-after.txt || true

diff -u \
  ~/wingmankvm-probe/nodes-before.txt \
  ~/wingmankvm-probe/nodes-after.txt || true
```

`diff` 返回非零状态表示文件确实有差异，在这里是正常结果。重点看插入后新增了哪个设备组和哪些 `/dev/videoN`。

本实例插入采集卡前，`/dev/video0` 到 `/dev/video4` 都属于 RK3399 内部视频模块；插入后新增：

```text
OCap Video: OCap Video (usb-xhci-hcd.0.auto-1):
        /dev/video5
        /dev/video6
        /dev/media2
```

这只能证明 `/dev/video5` 和 `/dev/video6` 来自同一张采集卡，还不能确定哪个节点真正输出画面。

## 3. 区分视频节点和 metadata 节点

对每个新增节点检查 udev 属性和 V4L2 能力：

```bash
udevadm info --query=property --name=/dev/video5
v4l2-ctl -d /dev/video5 --all
v4l2-ctl -d /dev/video5 --list-formats-ext

udevadm info --query=property --name=/dev/video6
v4l2-ctl -d /dev/video6 --all
v4l2-ctl -d /dev/video6 --list-formats-ext
```

按下面四项判断，不要只看 `/dev/videoN` 的编号：

1. `udevadm` 中应有 `ID_BUS=usb`，说明它来自 USB，而不是 RK3399 的 platform 视频模块。
2. `Card type` 应与采集卡名称一致。本实例为 `OCap Video: OCap Video`。
3. `v4l2-ctl --all` 的 `Device Caps` 必须包含 `Video Capture`。只有 `Metadata Capture` 的节点不能作为视频源。
4. `--list-formats-ext` 中应包含 `MJPG`，这样 WingmanKVM 才能直接转发采集卡输出的 JPEG 帧。

注意 `Capabilities` 是驱动的总体能力，同一张 UVC 设备可能同时列出 `Video Capture` 和 `Metadata Capture`。真正用于判断当前节点的是下面的 `Device Caps`。

本实例的结果是：

| 节点 | 总线 | Card type | Device Caps | MJPG | 结论 |
| --- | --- | --- | --- | --- | --- |
| `/dev/video5` | USB | OCap Video | Video Capture | 支持 | 选择它 |
| `/dev/video6` | USB | OCap Video | Metadata Capture | 不提供视频格式 | 不选择 |

`/dev/video5` 的格式表确认 `1920x1080`、`MJPG`、`30 FPS` 可用。实例机最终使用的也是：

```text
设备        /dev/video5
分辨率      1920 × 1080
像素格式    MJPG
帧率        30 FPS
编码方式    MJPEG 直通
```

这张采集卡虽然也列出 `3840x2160`，但该模式只有 18 FPS，因此不能把“支持 4K”直接理解成“支持 4K 30 FPS”。网页中的分辨率和帧率应以 `--list-formats-ext` 的实际组合为准。

如果需要在安装前单独验证一帧，并且没有其他程序占用该节点，可以执行：

```bash
timeout 10s v4l2-ctl -d /dev/video5 \
  --set-fmt-video=width=1920,height=1080,pixelformat=MJPG \
  --set-parm=30 \
  --stream-mmap=4 \
  --stream-count=1 \
  --stream-to=/tmp/wingmankvm-test.jpg

file /tmp/wingmankvm-test.jpg
```

如果 WingmanKVM 或旧的 Python/OpenCV 脚本已经在采集，应先找出占用者，不要同时运行这个测试：

```bash
sudo fuser -v /dev/video5
```

## 4. 确认 RK3399 的 UDC 和 Device role

先看内核是否暴露 USB Device Controller：

```bash
ls -l /sys/class/udc
```

本实例检测到的 UDC 是：

```text
fe800000.usb
```

这块板的 USB role 接口为：

```bash
cat /sys/kernel/debug/usb/fe800000.usb/mode
```

正确结果是：

```text
device
```

如果路径不存在，先检查 debugfs 是否已挂载：

```bash
findmnt /sys/kernel/debug
```

role 路径由开发板、设备树和内核决定，不能把 RK3399 实例中的路径照搬到其他板卡。应先查板卡文档；没有 Device/OTG 能力的 USB 控制器不能通过软件变成 UDC。

本实例在 `/etc/wingmankvm/gadget.env` 中使用的关键覆盖项是：

```ini
WINGMANKVM_GADGET_UDC=fe800000.usb
WINGMANKVM_USB_ROLE_PATH=/sys/kernel/debug/usb/fe800000.usb/mode
WINGMANKVM_USB_ROLE_VALUE=device
```

如果 `/sys/class/udc/fe800000.usb` 已经可见且 role 已是 `device`，通常可以直接运行安装器。只有 UDC 尚未出现、需要先写 role，或者一个板上有多个 UDC 时，才需要先用 `--no-start` 安装文件并编辑 `gadget.env`。

## 5. 安装 WingmanKVM

准备好与目标系统匹配的 **Linux ARM64** WingmanKVM 二进制，并让仓库中的 `deploy/` 目录保持完整。macOS ARM64 二进制不能在 RK3399 Linux 上运行；构建方式见[部署说明](DEPLOYMENT.md#2-构建-linux-二进制)。

正常情况下只需要一条命令：

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm
```

安装器会创建服务用户、网页终端用户、硬件权限、udev 规则、systemd 服务和完整的复合 USB Gadget。重复执行同一命令可用于升级，并会保留现有网页账号、配置、镜像和 `/etc/wingmankvm/gadget.env`。

如果 UDC 需要先切换 role：

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm --no-start
sudoedit /etc/wingmankvm/gadget.env
sudo systemctl enable --now \
  wingmankvm-gadget.service wingmankvm.service
```

不要同时运行旧的 Gadget 脚本。一个 UDC 同时只能绑定一个 Gadget，可以用下面的命令查看占用情况：

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
```

## 6. 安装器创建了哪些 USB 设备

官方 Gadget service 会一次创建四种功能：

| 功能 | 用途 | 关键参数 |
| --- | --- | --- |
| Boot Keyboard | BIOS/UEFI 和系统键盘 | subclass 1、protocol 1、8 字节 |
| Boot Relative Mouse | BIOS/UEFI 兼容鼠标 | subclass 1、protocol 2、4 字节 |
| Absolute Pointer | 网页坐标与远端坐标同步 | subclass 0、protocol 0、6 字节 |
| Mass Storage | ISO/IMG 虚拟介质 | removable LUN |

HID 的全局 `/dev/hidgN` 编号可能变化，因此应用优先使用安装器建立的稳定路径：

```text
/dev/wingmankvm-keyboard
/dev/wingmankvm-mouse
/dev/wingmankvm-absolute
```

在这台实例机上，它们当前分别指向 `hidg0`、`hidg1` 和 `hidg2`，但网页配置应填写稳定路径，而不是依赖这次启动时的编号。

默认使用绝对指针，因为浏览器可以把视频内容区域中的位置直接换算为远端绝对坐标，光标反馈最接近真实操作。相对鼠标仍会保留在复合 Gadget 中，因为部分 BIOS/UEFI 只可靠支持 Boot Mouse；如果 BIOS 中绝对指针不响应，可以在设备设置中临时切换到相对模式。

Mass Storage LUN 也由安装器创建。ISO 始终应只读；IMG 默认可以作为可读写 U 盘，但弹出前必须先在被控机中卸载文件系统，不能让 RK3399 和被控机同时读写同一个 IMG。

## 7. 两步完成网页首次设置

安装完成后，打开安装器最后打印的一次性初始化链接。不要把该链接、初始化令牌或密码截图上传到 README、Issue 或聊天记录。

### 第一步：管理员

- 设置管理员账号。
- 密码至少 12 位，并同时包含大写字母、小写字母、数字和符号。
- 同一密码会安全同步给 RK3399 本机网页终端使用的 `wingman` 用户。

### 第二步：检查连接

页面会自动扫描视频、HID、Mass Storage LUN 和 GPIO chip。此实例应看到或选择：

```text
视频             /dev/video5
键盘             /dev/wingmankvm-keyboard
相对鼠标         /dev/wingmankvm-mouse
绝对指针         /dev/wingmankvm-absolute
指针模式         绝对
镜像目录         /var/lib/wingmankvm/images
虚拟介质         自动检测到的 mass_storage.0/lun.0
```

自动检测只会把同时具备 `Video Capture` 和 `MJPG` 的节点列为可用视频候选，所以本实例的 metadata 节点 `/dev/video6` 不应被选中。存在多个合格候选时，页面会要求人工选择。

GPIO 可以先跳过。软件只能列出 `gpiochipN`，无法知道继电器接到了哪一条 line，也无法判断高电平还是低电平触发。

进入控制台后，在视频设置中选择 `1080p`、`30 FPS` 和 `MJPEG 直通`。MJPEG 直通仍是默认的局域网方案；需要控制家庭宽带上行占用时，再启用下面的 WebRTC / H.264 传输。

## 8. WebRTC / H.264 低带宽模式

WingmanKVM 始终先从采集卡接收 MJPEG。两种播放方式的区别发生在 RK3399 到浏览器这一段：

| 播放方式 | RK3399 上的处理 | 适用场景 |
| --- | --- | --- |
| MJPEG | JPEG 帧直接发送，不解码、不重新编码 | 局域网、CPU 和延迟优先 |
| WebRTC / H.264 | FFmpeg 把最新的 MJPEG 帧转换为 H.264，再通过 WebRTC 发送 | 家庭宽带上行有限、跨网访问 |

H.264 可以把带宽稳定在设定的目标码率附近，但编码不是零成本。默认目标码率是 `4000 Kbps`，默认最多同时建立 `1` 个 H.264 会话；每增加一个会话，通常都会再启动一份编码任务。建议先从 `2 Mbps` 或 `4 Mbps` 开始，根据文字清晰度和网络状况逐步调整。MJPEG 仍可随时作为兼容回退。

网页中的“自动”模式只有在检测到可用 H.264 编码器时才会尝试 WebRTC；编码器不可用、协商失败或没有收到首帧时，会自动返回 MJPEG。视频设置中的编码器顺序为：

1. `Rockchip MPP`：FFmpeg 编码器名为 `h264_rkmpp`；
2. `V4L2 M2M`：FFmpeg 编码器名为 `h264_v4l2m2m`；
3. `软件编码`：FFmpeg 编码器名为 `libx264`，必须人工允许。

“自动”默认只选择硬件编码器，不会在后台偷偷启用 `libx264`。只有同时勾选“允许软件编码”后，自动模式或软件编码选项才可能使用 CPU 编码。

### 8.1 检查 FFmpeg 和编码器

先确认系统能找到 FFmpeg：

```bash
command -v ffmpeg
ffmpeg -version
```

没有安装时，可以先安装发行版提供的软件包：

```bash
sudo apt update
sudo apt install -y ffmpeg
```

然后检查 WingmanKVM 识别的三个编码器名：

```bash
ffmpeg -hide_banner -encoders 2>&1 \
  | grep -E 'h264_rkmpp|h264_v4l2m2m|libx264'
```

发行版自带 FFmpeg 通常不包含 `h264_rkmpp`；只有带 Rockchip MPP 补丁、插件和对应用户态库的构建才会列出它。`h264_v4l2m2m` 出现在列表中也只说明 FFmpeg 编译了这个接口，不代表当前内核一定暴露了可用的 H.264 编码设备。最终仍要结合设备节点和实际拉流结果判断。

检查 Rockchip MPP / VPU / RGA 设备以及 V4L2 节点：

```bash
ls -l /dev/mpp_service /dev/vpu_service /dev/rga 2>/dev/null || true
v4l2-ctl --list-devices
media-ctl -p
```

对疑似编解码器的 `/dev/videoN` 继续检查：

```bash
v4l2-ctl -d /dev/videoN --all
v4l2-ctl -d /dev/videoN --list-formats
v4l2-ctl -d /dev/videoN --list-formats-out
```

硬件 H.264 编码节点通常应表现为 V4L2 Memory-to-Memory 设备，并在编码输出一侧提供 `H264`。不要把采集卡的 `Video Capture` 节点，也不要把只提供 H.264 解码的节点误当成编码器。保存视频设置后可通过服务日志确认探测和运行结果：

```bash
journalctl -u wingmankvm -b --no-pager \
  | grep -E 'H\.264|WebRTC|FFmpeg'
```

### 8.2 这台 RK3399 实例的探测结论

本教程使用的实例机当前没有安装 FFmpeg、GStreamer 或 Rockchip MPP 用户态库，也没有 `/dev/mpp_service`、`/dev/vpu_service`、`/dev/rga`。内核暴露的 Hantro VPU 节点只提供 JPEG 编码和 H.264 解码，没有可供 FFmpeg 使用的 H.264 硬件编码能力。

因此，这台实例机按当前内核和用户态环境运行时会明确显示 H.264 不可用，并继续使用 MJPEG；程序不会自动改用软件编码占满 RK3399 的 CPU。这是对本实例当前软件栈的结论，不代表所有 RK3399 BSP 或第三方 FFmpeg 构建都不支持 H.264 硬编码。更换内核、MPP 用户态库或 FFmpeg 后，应重新执行上面的探测。

### 8.3 软件编码风险

`libx264` 可以在没有硬件编码器时提供 H.264，但 1080p 30 FPS 的解码、色彩转换和编码会持续占用多个 CPU 核心，可能导致温度升高、降频、视频延迟增加，并影响 HID、终端和网页响应。RK3399 上不建议把它作为长期默认方案。

如果确实需要临时测试，先选择较低分辨率、帧率和码率，只允许一个会话，同时监控：

```bash
top
cat /sys/class/thermal/thermal_zone0/temp
journalctl -u wingmankvm -f
```

温度值通常以千分之一摄氏度表示，例如 `75000` 约为 `75°C`；具体 thermal zone 含义以板卡内核为准。出现持续满载、降频或控制延迟时，应立即关闭软件编码并切回 MJPEG。

### 8.4 家庭网络、UDP、VPN 和 TURN

WebRTC 的 offer/answer 信令仍通过 WingmanKVM 的 HTTP 服务完成，但视频媒体通常通过 ICE 协商后的 UDP 路径传输。只把 TCP `8080` 放到 Nginx、Caddy 或其他反向代理后面，并不等于 WebRTC 视频也能穿过公网。

家庭网络建议按以下顺序选择：

1. 优先让浏览器和 RK3399 通过 WireGuard 等 VPN 进入同一可路由网络，最容易保留 UDP 直连；
2. 能控制两端防火墙和 NAT 时，可配合明确的 UDP 放行、端口映射和可达 ICE 候选直连；
3. 无法建立直连时使用 TURN 中继，但 TURN 服务器本身也需要足够的公网出口带宽。

当前版本默认只使用本机 ICE 候选，没有内置 TURN 配置。仅部署一个 TURN 服务不会自动生效，还需要在 WingmanKVM 中配置对应的 ICE server 后才能使用。公网开放前还应启用 HTTPS、限制访问来源，并优先把管理页面放在 VPN 内，不要直接裸露到互联网。

目标码率决定的主要是 RK3399 一侧的上行占用：`2 Mbps` 约等于每小时 `0.9 GB`，`4 Mbps` 约等于每小时 `1.8 GB`，实际还会包含 WebRTC、RTP、重传和网络协议开销。TURN 中继解决的是连通性，不会进一步压低视频本身的码率。

## 9. GPIO 电源按钮：必须按接线确认

先列出控制器：

```bash
sudo gpiodetect
```

再结合开发板原理图查看候选芯片。libgpiod 2.x 的命令示例：

```bash
sudo gpioinfo -c gpiochip1
sudo gpioinfo -c gpiochip1 7
gpioset --version
```

`line 7` 是 GPIO chip 内的 line offset，不是排针上的物理第 7 脚。不要选择已经被内核驱动占用、显示有其他 `consumer` 的线路。

这台实例机的继电器经过实物接线确认后使用：

| 项目 | 此实例值 |
| --- | --- |
| GPIO chip | `gpiochip1` |
| line offset | `7` |
| 触发电平 | 高电平 |
| 短按 | 500 ms |
| 长按 | 5000 ms |

这些值 **只属于本实例**。新用户应先断开被控机电源，按原理图和万用表确认线路，再在网页中启用电源控制。不要靠连续试点未知 GPIO 的方式猜线路；选错可能影响电源、存储或其他板载设备。

## 10. 安装完成后的验证

检查服务：

```bash
systemctl --no-pager --full status \
  wingmankvm-gadget.service wingmankvm.service

journalctl -u wingmankvm-gadget -u wingmankvm \
  -b --no-pager
```

`wingmankvm-gadget.service` 是 `oneshot` 服务，成功后显示 `active (exited)` 是正常状态；`wingmankvm.service` 应显示 `active (running)`。

如果修改过 `WINGMANKVM_GADGET_NAME` 等环境项，手工调用 helper 时要先加载同一份环境：

```bash
sudo sh -c '
  set -a
  . /etc/wingmankvm/gadget.env
  set +a
  exec /usr/local/sbin/wingmankvm-gadget status
'
```

本实例的状态应包含 `udc=fe800000.usb`，以及三个 `/dev/wingmankvm-*` 稳定路径和一个 Mass Storage LUN。

检查 UDC 与 HID：

```bash
cat /sys/class/udc/fe800000.usb/state
ls -l \
  /dev/wingmankvm-keyboard \
  /dev/wingmankvm-mouse \
  /dev/wingmankvm-absolute
```

OTG 线已接到开机的被控机并完成枚举时，实例机的 UDC 状态为 `configured`。未接线时可能是其他状态，不代表 Gadget 创建失败。

检查视频协商结果：

```bash
v4l2-ctl -d /dev/video5 --get-fmt-video
v4l2-ctl -d /dev/video5 --get-parm
```

应看到 `1920x1080`、`MJPG` 和 `30 FPS`。最后检查 HTTP 服务：

```bash
curl -fsS http://127.0.0.1:8080/healthz
```

正常返回：

```json
{"service":"wingmankvm","status":"ok"}
```

然后在被控机上依次验证：

1. BIOS/UEFI 能收到键盘和相对鼠标输入；
2. 操作系统桌面中绝对指针位置与网页视频区域一致；
3. 视频能持续显示，切换到 1080p 30 FPS 后状态正常；
4. GPIO 接线确认无误后再测试短按；
5. 虚拟介质先用可丢弃的测试镜像验证，并始终执行安全弹出。

## 11. 常见故障

### 插入采集卡后没有新增节点

```bash
sudo dmesg --follow
lsusb
v4l2-ctl --list-devices
```

`dmesg --follow` 会持续显示新日志，检查完成后按 `Ctrl-C` 退出。

确认插入的是 RK3399 的 Host 口，换数据线和供电可靠的 USB 口测试。只有充电能力的线不会枚举设备。

### 两个 OCap 节点不知道选哪个

不要猜编号。分别运行 `v4l2-ctl -d 节点 --all`，选择 `Device Caps` 为 `Video Capture` 且格式表包含 `MJPG` 的节点。只有 `Metadata Capture` 的节点必须排除。本实例是 `/dev/video5`，不是 `/dev/video6`。

### 视频黑屏或设备忙

```bash
sudo fuser -v /dev/video5
journalctl -u wingmankvm -b --no-pager
```

停止仍在占用采集卡的旧 Python/OpenCV 测试程序或其 systemd 服务，同时确认被控机确实输出了采集卡支持的 HDMI 模式。

### `/sys/class/udc` 为空

确认使用的是板上的 OTG / Device 口，并检查 role：

```bash
cat /sys/kernel/debug/usb/fe800000.usb/mode
journalctl -u wingmankvm-gadget -b --no-pager
```

`fe800000.usb` 和对应 debugfs 路径只适用于本实例。其他板卡必须根据自己的设备树和内核接口配置；如果硬件没有 Device 控制器，则无法提供 HID 和虚拟介质。

### Gadget service 启动失败

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
journalctl -u wingmankvm-gadget -b --no-pager
```

常见原因是旧 Gadget 已占用 UDC、role 路径错误、内核缺少 `libcomposite` / HID / Mass Storage function，或仍有虚拟介质挂载导致服务拒绝解绑。

### 没有稳定 HID 路径

```bash
ls -l /dev/hidg* /dev/wingmankvm-* 2>/dev/null
find /sys/kernel/config/usb_gadget \
  -path '*/functions/hid.*' -type d -print
```

先看 Gadget service 日志。部分旧内核不提供 HID function 的 `dev` 设备号映射，安装器无法可靠建立稳定链接；这种情况需要核对 function 的 `subclass`、`protocol` 和 `report_length`，再在网页高级设置中人工填写实际 `/dev/hidgN`。

### 网页打不开或 8080 被旧程序占用

```bash
sudo fuser -v 8080/tcp
systemctl --no-pager --full status wingmankvm.service
journalctl -u wingmankvm -b --no-pager
```

先确认占用进程属于哪个服务，再停止旧服务；不要在不知道进程来源时直接永久禁用系统组件。

### 绝对鼠标在 BIOS 中无响应

这是部分固件的兼容性限制。WingmanKVM 同时保留了 Boot Relative Mouse，在网页设备设置中切换到相对模式即可；进入支持绝对指针的桌面系统后可以再切回绝对模式。

更通用的 UDC、自定义 Gadget、权限与升级说明见[部署说明](DEPLOYMENT.md)。
