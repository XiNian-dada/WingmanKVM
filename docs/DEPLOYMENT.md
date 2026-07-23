# 部署说明

WingmanKVM 的推荐部署方式是：准备一个与目标机架构匹配的 Linux 二进制，然后运行仓库自带的 `deploy/install.sh`。安装器会创建用户、权限、USB Gadget、systemd 服务和状态目录；首次网页向导负责扫描并回填采集卡、HID 与虚拟介质设备。正常安装不需要手工编写 configfs 脚本。

## 1. 系统要求

目标 KVM 主机需要：

- Linux `aarch64`、`armv7` 或 `x86_64`；
- systemd、udev、Bash 和 `sudo`；
- 可用或可切换到 Device 模式的 USB Device/OTG 控制器（Linux UDC）；
- 内核 USB Gadget/configfs、HID function 和 Mass Storage function 支持；
- 支持 V4L2 mmap 的 USB HDMI 采集卡；
- 电源控制需要 libgpiod 2.x 的 `gpioset`，不使用 GPIO 时可以不安装。

Debian、Ubuntu、Armbian 的常用运行与诊断工具：

```bash
sudo apt update
sudo apt install -y sudo udev libgpiod-tools v4l-utils
```

官方安装器要求 systemd 和 Linux UDC，并部署完整的 HID 与 Mass Storage 复合 Gadget；当前不提供无 UDC 的 video-only 安装模式。手工启动二进制只适合源码开发和诊断，不代表受支持的生产部署，也不会替代 Gadget、权限和持久化服务。

## 2. 构建 Linux 二进制

### 在目标机或同架构 Linux 上构建

```bash
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev

git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

需要近期的 stable Rust。`v4l2-sys-mit` 会通过 bindgen 使用 `linux/videodev2.h`、clang 和 libclang；如果构建时报找不到该头文件，请确认 Linux UAPI headers 已安装。

### 在 Apple Silicon Mac 上构建 Linux ARM64 版本

macOS ARM64 二进制不能直接在 ARM64 Linux 上运行。可以使用原生 ARM64 Docker 容器构建：

```bash
mkdir -p dist
docker run --rm --platform linux/arm64 \
  -v "$PWD:/src:ro" \
  -v "$PWD/dist:/out" \
  -w /src \
  rust:1.94-bookworm bash -c '
    apt-get update &&
    apt-get install -y --no-install-recommends \
      clang libclang-dev linux-libc-dev pkg-config &&
    CARGO_TARGET_DIR=/tmp/wingman-target \
      cargo build --release --locked &&
    install -m 0755 \
      /tmp/wingman-target/release/wingmankvm \
      /out/wingmankvm
  '
```

生成文件为 `dist/wingmankvm`。目标板只需要该二进制和仓库中的 `deploy/` 目录，不需要 Rust 工具链。容器发行版的 glibc 不应比目标系统更新太多，否则请改用与目标系统接近的构建镜像或合适的交叉编译 sysroot。

## 3. 一条命令安装

在目标 KVM 主机上保留完整的 `deploy/` 目录，然后运行：

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm
```

如果刚在目标机完成构建，可以直接使用：

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

安装器会验证二进制格式与 CPU 架构，并幂等地完成：

- 创建 `wingmankvm` 服务用户、`wingman` 网页终端用户和 `wingmankvm-hw` 硬件组；
- 创建 `/var/lib/wingmankvm` 和 `/var/lib/wingmankvm/images`；
- 安装 udev 规则、终端 profile、最小密码同步 helper 与 sudoers 规则；
- 安装并启动 `wingmankvm-gadget.service`；
- 创建 Boot Keyboard、Boot Relative Mouse、Absolute Pointer 和 Mass Storage LUN；
- 在内核提供 HID function 设备号映射时创建 `/dev/wingmankvm-keyboard`、`/dev/wingmankvm-mouse`、`/dev/wingmankvm-absolute` 稳定链接；
- 安装、启用并启动 `wingmankvm.service`。

安装完成后默认监听 `0.0.0.0:8080`。安装器会直接打印带一次性令牌的首次设置地址；页面读取令牌后会立即从地址栏清除，无需手工复制或先查日志。安装器不会把密码写进命令行或 unit 文件；首次网页初始化时，管理员密码会通过受限 helper 同步给本机终端用户 `wingman`。

重复运行同一命令可用于升级。安装器会更新二进制和托管的部署文件，但保留：

- `/var/lib/wingmankvm/config.json`；
- `/var/lib/wingmankvm/auth.json`；
- `/var/lib/wingmankvm/images/`；
- 已存在的 `/etc/wingmankvm/gadget.env`；
- 已存在的 `wingman` 用户 profile。

## 4. UDC 与板级 USB role

默认配置会选择第一个未被占用的 UDC。多数已经把 OTG 口置于 Device 模式的开发板可以直接安装，无需修改任何配置：

```bash
ls -l /sys/class/udc
```

如果 `/sys/class/udc` 目录存在但暂时为空、板上有多个 UDC，或必须先通过 sysfs 切换 USB role，请先只安装文件：

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm --no-start
sudoedit /etc/wingmankvm/gadget.env
```

这只适用于硬件确实具备 Device/OTG 控制器、配置 USB role 后能够出现 UDC 的情况。如果系统没有 `/sys/class/udc`，或物理控制器只能作为 Host，`--no-start` 只能暂存文件，不能形成受支持的 WingmanKVM 部署。

常用配置项：

```ini
# 留空时选择第一个空闲 UDC
WINGMANKVM_GADGET_UDC=

# 某些板卡需要先写入 device；路径由板卡和内核决定
WINGMANKVM_USB_ROLE_PATH=
WINGMANKVM_USB_ROLE_VALUE=device

# configfs 中的 Gadget 目录名
WINGMANKVM_GADGET_NAME=wingmankvm
```

`WINGMANKVM_USB_ROLE_PATH` 无法跨板卡可靠猜测，应以开发板文档、设备树和当前内核接口为准。配置完成后启动服务：

```bash
sudo systemctl enable --now \
  wingmankvm-gadget.service wingmankvm.service
```

一个 UDC 同时只能绑定一个 Gadget。若旧 Python 脚本、板厂 service 或其他 configfs Gadget 已占用目标 UDC，请先停止其管理服务；WingmanKVM 不会抢占另一个 Gadget 正在使用的 UDC。可以用下面的命令定位占用者：

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
systemctl --no-pager --full status wingmankvm-gadget.service
```

不要把 `WINGMANKVM_GADGET_NAME` 指向不准备由 WingmanKVM 接管的第三方 Gadget。托管 Gadget 配置变化时，服务可能需要短暂解绑并重新枚举 USB；如果虚拟介质仍挂载，服务会拒绝重建以避免损坏镜像。

## 5. 首次网页设置

正常安装完成后，直接打开安装器最后显示的一次性设置地址。若终端输出已经丢失，再从日志取得仅在创建管理员前有效的地址或 token：

```bash
sudo journalctl -u wingmankvm -b --no-pager
```

打开 `http://KVM主机地址:8080/`，然后按两步向导完成设置：

1. 创建管理员账号；一次性地址会自动填入 setup token，密码至少 12 位，并包含大写字母、小写字母、数字和符号；
2. 在“检查连接”中确认自动检测结果，选择是否启用虚拟介质；GPIO 电源控制可以跳过并在之后配置。

自动检测会：

- 优先选择支持 MJPEG 的 V4L2 视频节点；
- 优先使用官方 Gadget service 按 function 设备号创建的稳定 HID 路径；
- 自动回填唯一可用且已绑定的 Mass Storage LUN；
- 使用默认镜像目录 `/var/lib/wingmankvm/images`。

提交向导时后端还会重新扫描一次，因此不依赖浏览器缓存的路径。对于自定义 Gadget，角色识别读取 configfs 的 `subclass`、`protocol`、`report_length`，再通过 function `dev` 的 major:minor 匹配 `/dev/hidg*`，并检查 function 已链接且 Gadget 已绑定。原始设备路径放在“高级设置”中；默认安装通常无需展开。部分旧内核不提供 function `dev` 属性，无法可靠自动判定角色，此时必须人工确认并填写键盘、相对鼠标和绝对指针路径。

以下情况仍需要人工判断：

- 采集卡暴露多个都支持 MJPEG 的视频节点；
- 系统中存在多个可用 Gadget 或 LUN；
- GPIO 芯片、实际线路和触发电平。自动扫描只能看到 GPIO 字符设备，不能从软件推断物理接线；需要在网页选择高电平或低电平触发。

完成初始化后 setup token 立即失效。随后可以在网页“设备”和“虚拟介质”面板重新扫描或调整配置。

## 6. 更新与重新配置

更新二进制时重新运行安装器即可：

```bash
sudo ./deploy/install.sh --binary /path/to/new/wingmankvm
```

如果 Gadget service 已经正常运行，安装器会保持当前 USB 连接，不主动重新枚举。修改 `/etc/wingmankvm/gadget.env` 后需要显式重启 Gadget service：

```bash
sudo systemctl restart wingmankvm-gadget.service
```

这会让被控机短暂断开键盘、鼠标和虚拟 USB 设备。重启前必须先从被控机安全卸载并在 WingmanKVM 中弹出读写 IMG；仍有 backing file 时 Gadget service 会拒绝解绑。

`--no-start` 只安装文件，不启用或启动服务，也不会停止已经运行的服务。适合先处理板级 USB role、迁移旧 Gadget 或安排维护窗口。

## 7. 安装后检查

检查两个服务和应用接口：

```bash
systemctl --no-pager --full status \
  wingmankvm-gadget.service wingmankvm.service
journalctl -u wingmankvm-gadget -u wingmankvm -b --no-pager
curl -fsS http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/api/bootstrap
```

检查 UDC、稳定 HID 链接和 Mass Storage LUN：

```bash
cat /sys/kernel/config/usb_gadget/wingmankvm/UDC
ls -l \
  /dev/wingmankvm-keyboard \
  /dev/wingmankvm-mouse \
  /dev/wingmankvm-absolute
find /sys/kernel/config/usb_gadget \
  -path '*/functions/mass_storage.*/lun.0' -type d -print
```

如果修改过 `WINGMANKVM_GADGET_NAME`，相应替换第一条命令中的目录名。页面状态应显示实际协商的视频分辨率和帧率。视频设备被拔出或被其他进程占用时，采集 supervisor 会显示错误并按 1 秒退避重试，HID、GPIO、终端和虚拟介质不会因此一起退出。

## 8. 常见故障排查

### 找不到 UDC

```bash
ls -l /sys/class/udc
mount | grep configfs
journalctl -u wingmankvm-gadget -b --no-pager
```

确认使用的是具备 Device/OTG 能力的物理 USB 口，而不是只能作为 Host 的接口。部分开发板还需要设备树 overlay、内核模块或板级 role-switch 路径；在硬件确实支持 Device 模式的前提下，可以先用安装器的 `--no-start` 完成文件部署，再修改 `gadget.env`。没有 Linux UDC 的主机不属于当前官方安装范围。

### Gadget service 启动失败

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
systemctl --no-pager --full status wingmankvm-gadget.service
journalctl -u wingmankvm-gadget -b --no-pager
```

最常见原因是 UDC 被旧 Gadget 占用、指定的 role 路径不存在，或内核缺少 `libcomposite`、HID、Mass Storage function。若日志提示虚拟介质仍挂载，请先在被控机卸载文件系统，再从网页弹出镜像。

### 网页没有检测到 HID

```bash
ls -l /dev/hidg* /dev/wingmankvm-* 2>/dev/null
find /sys/kernel/config/usb_gadget \
  -path '*/functions/hid.*' -type d -print
```

官方 Gadget service 会通过每个 HID function `dev` 属性中的 major:minor 建立稳定链接，不依赖全局 `hidgN` 编号。网页对自定义 Gadget 的角色判定也依赖 `subclass`、`protocol`、`report_length` 和这项设备号映射，并不会只凭 `/dev/hidgN` 的编号猜测。旧内核若没有暴露 function `dev`，扫描只能把 `/dev/hidg*` 列为未验证节点；请先根据自定义 Gadget 的创建顺序和 descriptor 核对角色，再在网页高级设置中人工填写路径。

### 网页没有检测到采集卡

```bash
v4l2-ctl --list-devices
ls -l /dev/video*
v4l2-ctl -d /dev/video0 --list-formats-ext
```

选择实际支持 `MJPG` 的 capture 节点。一个采集卡可能同时暴露 metadata、raw video 或多个接口，不要只凭 `/dev/videoN` 的编号判断。确认旧 Python/OpenCV 测试程序没有继续占用设备。

### GPIO 电源控制不可用

```bash
gpiodetect
gpioinfo gpiochip1
gpioset --version
```

网页中填写的是 `gpiochipN` 和 line offset，不是排针物理编号；同时选择继电器是高电平还是低电平触发。请结合原理图和万用表确认继电器逻辑，再启用电源控制。当前命令格式要求 libgpiod 2.x。

### 虚拟介质不可用

```bash
find /sys/kernel/config/usb_gadget \
  -path '*/functions/mass_storage.*/lun.0' -type d -print
```

官方 Gadget service 会创建 LUN 并授权 `file`、`ro`、`cdrom`、`removable` 和可选的 `forced_eject` 属性。ISO 以 `cdrom=1, ro=1` 暴露；IMG 以 `cdrom=0` 暴露，可选择 `ro=0` 读写。

读写 IMG 与真实 U 盘一样需要安全弹出：先在被控机卸载或弹出，再在 WingmanKVM 中点击“弹出”。不要让 KVM 主机与被控机同时读写同一个镜像。强制弹出或 USB 解绑都可能损坏文件系统。

建议在网页中保留默认镜像目录 `/var/lib/wingmankvm/images`。如果确实要移到 `/var/lib/wingmankvm` 之外，需要先创建目录并为应用服务增加 systemd 写权限，例如：

```bash
sudo install -d -o wingmankvm -g wingmankvm -m 0700 /srv/wingmankvm-images
sudo systemctl edit wingmankvm.service
```

在编辑器中写入：

```ini
[Service]
ReadWritePaths=/srv/wingmankvm-images
```

然后执行：

```bash
sudo systemctl daemon-reload
sudo systemctl restart wingmankvm.service
```

最后再把网页中的镜像目录改为 `/srv/wingmankvm-images`。仅修改网页路径或文件所有者还不够；`ProtectSystem=strict` 会阻止未列入 `ReadWritePaths=` 的上传和写入。

## 9. 高级：自定义 Gadget 检查

本节仅适用于已经有板厂 Gadget、需要自定义 USB VID/PID/descriptor，或正在排查内核兼容性的人。默认安装请使用 `wingmankvm-gadget.service`，不要手工重复创建同名 function。

如果改用自己的 Gadget manager，需要让应用 unit 依赖你的 Gadget service，而不是同时启动官方 `wingmankvm-gadget.service`。一个 UDC 只能有一个管理者；修改 `deploy/wingmankvm.service` 的副本或创建合适的 systemd override 后再部署。

WingmanKVM 对自定义 Gadget 的 HID 角色判定要求 function 已链接到 USB config、Gadget 已绑定 UDC，并且能通过 function `dev` 的 major:minor 映射到实际 `/dev/hidg*`。角色参数应满足：

| 接口 | `subclass` | `protocol` | `report_length` |
| --- | ---: | ---: | ---: |
| Boot Keyboard | `1` | `1` | `8` |
| Boot Relative Mouse | `1` | `2` | `4` |
| Absolute Pointer | `0` | `0` | `6` |

检查自定义 function：

```bash
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/subclass
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/protocol
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/report_length
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/dev
```

如果旧内核没有 `dev` 属性，WingmanKVM 无法把 function 可靠对应到全局编号的 `/dev/hidgN`，需要根据 Gadget manager 的创建与链接顺序人工确认路径。report descriptor 必须与 report length 一致。Absolute Pointer 使用 3 个按钮、16 位绝对 X/Y（`0..=32767`）和 8 位相对滚轮；官方无 Report ID descriptor 为：

```text
05 01 09 02 A1 01 09 01 A1 00
05 09 19 01 29 03 15 00 25 01 95 03 75 01 81 02
95 01 75 05 81 03
05 01 09 30 09 31 16 00 00 26 FF 7F 75 10 95 02 81 02
09 38 15 81 25 7F 75 08 95 01 81 06
C0 C0
```

自定义 Mass Storage function 必须创建 `lun.0`，链接到已绑定 UDC 的 config，并允许 `wingmankvm-hw` 组修改必要属性：

```bash
LUN=/sys/kernel/config/usb_gadget/你的gadget/functions/mass_storage.0/lun.0
for ATTR in file ro cdrom removable forced_eject; do
  if [ -e "$LUN/$ATTR" ]; then
    sudo chgrp wingmankvm-hw "$LUN/$ATTR"
    sudo chmod g+rw,o-rw "$LUN/$ATTR"
  fi
done
```

`forced_eject` 并非所有内核都有。configfs 重新创建 function 后权限可能恢复默认，因此这些授权应放入自定义 Gadget 启动服务，而不是只手工执行一次。镜像文件也必须允许 `wingmankvm` 用户读取，读写 IMG 还需要写权限。

## 10. 视频模式

推荐优先级：

1. 采集卡原生 MJPEG + `mjpeg_passthrough`；
2. 降低采集分辨率或帧率；
3. 只有需要限制网络带宽时使用 `transcode_jpeg` 和 JPEG quality。

直通仍需从 mmap buffer 复制一次 `bytesused` 范围，因为驱动会在下一次 dequeue/queue 后复用缓冲区；它消除的是 JPEG 解码与重新编码。

## 11. HTTPS

直接 HTTP 模式不会给 Cookie 添加 `Secure`，只适合可信管理网内调试。生产环境建议：

- 使用 WireGuard/Tailscale 等 VPN；或
- 在 Caddy/Nginx 后终止 HTTPS，并限制来源网络。

不要把控制台端口直接映射到公网。
