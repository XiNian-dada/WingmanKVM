# 部署说明

WingmanKVM 的推荐部署方式是：准备一个与目标机架构匹配的 Linux 二进制，然后运行仓库自带的 `deploy/install.sh`。安装器会创建用户、权限、USB Gadget、systemd 服务和状态目录；首次网页向导负责扫描并回填采集卡、HID 与虚拟介质设备。正常安装不需要手工编写 configfs 脚本。

## 1. 系统要求

目标 KVM 主机需要：

- Linux `aarch64`、`armv7` 或 `x86_64`；
- systemd、udev、Bash、`sudo`、Python 3、GNU coreutils（`timeout`）和 util-linux（`flock`）；
- 可用或可切换到 Device 模式的 USB Device/OTG 控制器（Linux UDC）；
- 内核 USB Gadget/configfs、HID function 和 Mass Storage function 支持；
- 支持 V4L2 mmap 的 USB HDMI 采集卡；
- 电源控制需要 libgpiod 2.x 的 `gpioset`，不使用 GPIO 时可以不安装。

Debian、Ubuntu、Armbian 的常用运行与诊断工具：

```bash
sudo apt update
sudo apt install -y sudo udev python3 coreutils util-linux libgpiod-tools v4l-utils
```

官方安装器要求 systemd 和 Linux UDC，并部署完整的 HID 与 Mass Storage 复合 Gadget；当前不提供无 UDC 的 video-only 安装模式。手工启动二进制只适合源码开发和诊断，不代表受支持的生产部署，也不会替代 Gadget、权限和持久化服务。

## 2. 获取 Linux 二进制

### 使用发布包安装（推荐）

发布包由 GitHub Actions 在推送 `v*` tag 时构建，命名格式为 `wingmankvm-<版本>-<target>.tar.gz`，包含二进制、完整的 `deploy/` 目录、`LICENSE`、`README.md` 和 `CHANGELOG.md`。ARM64 主机选择 `aarch64-unknown-linux-gnu`，32 位 ARM 主机选择 `armv7-unknown-linux-gnueabihf`。

1. 从 [Releases](https://github.com/XiNian-dada/WingmanKVM/releases) 下载对应架构的 `.tar.gz` 和同一 Release 的 `SHA256SUMS`，放在同一目录。
2. 校验并解压：

   ```bash
   sha256sum -c --ignore-missing SHA256SUMS
   mkdir -p wingmankvm-release
   tar -xzf wingmankvm-<版本>-<target>.tar.gz -C wingmankvm-release
   ```

   `--ignore-missing` 允许只下载当前架构的发布包；校验失败时不要继续。
3. 从解压出的 `deploy/` 目录运行安装器，并通过 `--release` 指向发布包：

   ```bash
   cd wingmankvm-release
   sudo ./deploy/install.sh --release ../wingmankvm-<版本>-<target>.tar.gz
   ```

安装器发现发布包同目录存在 `SHA256SUMS` 时会再次核对校验和，然后解包到安全创建的临时目录，并复用 `--binary` 的文件安装、健康检查、失败回滚和升级恢复流程。临时目录在成功、失败和回滚路径都会清理。

### 从源码构建

#### 在目标机或同架构 Linux 上构建

```bash
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev

git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

需要近期的 stable Rust。`v4l2-sys-mit` 会通过 bindgen 使用 `linux/videodev2.h`、clang 和 libclang；如果构建时报找不到该头文件，请确认 Linux UAPI headers 已安装。

#### 在 Apple Silicon Mac 上构建 Linux ARM64 版本

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

如果使用第 2 节的发布包，则改用 `--release`，无需手工解出二进制：

```bash
sudo ./deploy/install.sh --release /path/to/wingmankvm-<版本>-<target>.tar.gz
```

安装器会在发布包同目录存在 `SHA256SUMS` 时校验其内容；`--binary` 与 `--release` 只能选择一个。

安装器会验证二进制格式与 CPU 架构，并幂等地完成：

- 创建 `wingmankvm` 服务用户、`wingman` 网页终端用户和 `wingmankvm-hw` 硬件组；
- 创建 `/var/lib/wingmankvm` 和 `/var/lib/wingmankvm/images`；
- 安装 udev 规则、终端 profile、最小密码同步 helper 与 sudoers 规则；
- 安装并启动 `wingmankvm-gadget.service`；
- 创建 Boot Keyboard、Boot Relative Mouse、Absolute Pointer 和 Mass Storage LUN；
- 在内核提供 HID function 设备号映射时创建 `/dev/wingmankvm-keyboard`、`/dev/wingmankvm-mouse`、`/dev/wingmankvm-absolute` 稳定链接；
- 安装、启用并启动 `wingmankvm.service`。

安装完成后默认监听 `0.0.0.0:8080`。安装器会直接打印带一次性令牌的首次设置地址；页面读取令牌后会立即从地址栏清除，无需手工复制或先查日志。安装器不会把密码写进命令行或 unit 文件；首次网页初始化时，管理员密码会通过受限 helper 同步给本机终端用户 `wingman`。

网页终端以 `wingman` 用户运行，新装默认不授予该用户 sudo 权限。主机管理员可用 `--maintenance-mode privileged` 显式启用 root 维护；该模式下服务可重置维护密码，因此接受“服务被攻陷后宿主也可被接管”的边界。旧版已有管理授权时，升级必须显式选择 `restricted` 或 `privileged`，不会静默更改既有访问。完整启用、关闭与外部授权检查见[维护权限模式](MAINTENANCE_MODES.md)。

应用仍以无登录 shell 的 `wingmankvm` 身份运行，但 nologin 本身不构成完整隔离。unit 实际开放 `/var/lib/wingmankvm`、`/run/sudo`、`/etc` 和可选的 `/sys/kernel/config/usb_gadget` 写入挂载；root 密码 helper 需要 `/etc` 更新 shadow，普通 Unix 权限仍然有效。

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

升级前预检并暂存所有托管文件，快照保存在仅 root 可访问的 `/var/lib/wingmankvm-deploy/`，与应用数据分开。新服务未在 15 秒健康窗口内通过 `/healthz` 时，会恢复整组文件及此前服务状态；SIGKILL 留下待恢复标记，后续安装会停止并打印独立恢复脚本。断电或文件系统崩溃后的快照完整性需人工检查，不保证存储故障恢复。`auth.json`、`config.json`、镜像和已有 Gadget 配置不被回滚覆盖。`--no-start` 仅验证文件安装，不宣称运行验收。详见[升级恢复说明](UPGRADE_RECOVERY.md)。

旧版 sudoers 含 root 维护授权时，上面的命令会要求补充 `--maintenance-mode restricted` 或 `--maintenance-mode privileged`。

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

未配置 `server.public_origin` 或显式使用 `http://...` 时，Cookie 不带 `Secure`，只适合可信管理网内调试。配置 `https://...` 后，首次设置、密码/Passkey 登录和注销响应统一使用 Secure Cookie，且不会信任转发头降级。迁移前须撤销旧会话并清除旧 Cookie，再通过 HTTPS 重新登录，同时关闭公开 HTTP 入口。详见[会话 Cookie 策略](SESSION_COOKIES.md)。生产环境建议：

- 使用 WireGuard/Tailscale 等 VPN；或
- 在 Caddy/Nginx 后终止 HTTPS，并限制来源网络。

不要把控制台端口直接映射到公网。

### 反向代理、内网穿透与客户端 IP

`server.proxy_protocol` 支持 `disabled`、`auto` 和 `required`，处理的是 TCP 连接前的 PROXY Protocol v2。它不会自动让 HTTP `Host` 与浏览器 `Origin` 一致。Nginx HTTP 反代应保留公网 Host：

```nginx
proxy_set_header Host $http_host;
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
```

浏览器终端 WebSocket 在升级前强制校验 `Origin`，不会信任 `X-Forwarded-Proto`、`Forwarded` 或其他转发头来决定协议。直接 HTTP 部署只接受与请求 `Host` 相同的 `http` 来源。若反向代理终止 HTTPS，必须在状态目录的 `config.json` 中配置唯一的公网来源（scheme + 主机 + 端口，默认端口可省略）：

```json
{
  "server": {
    "public_origin": "https://kvm.example.com"
  }
}
```

配置后终端只接受该精确来源，并要求请求 `Host` 与其主机和端口一致；反向代理必须像上面的 Nginx 示例一样保留浏览器访问的外部 `Host`。不要把内网直连地址当作旁路，也不要在代理改写 `Host` 后继续使用旧配置。缺失、`null`、多个、逗号分隔或格式错误的 `Origin` 都会被拒绝；未配置 `public_origin` 时 HTTPS 来源同样会被拒绝。该字段也可通过 `POST /api/config` 的 `server.public_origin` 设置，显式传入 `null` 可清除。

如果链路是“浏览器 → Nginx → 本机隧道端口 → 内网隧道 → KVM”，PPv2 可能只携带 Nginx 的 `127.0.0.1`，而浏览器的公网出口 IP 在 HTTP 转发头中。可在状态目录的 `config.json` 中显式配置：

```json
{
  "server": {
    "proxy_protocol": "auto",
    "trusted_http_proxies": ["192.168.1.2", "127.0.0.1"]
  }
}
```

这是应合并进现有配置的片段，不要覆盖整个配置文件。地址必须替换为实际 TCP 隧道对端与 PPv2 中的代理地址；只支持明确的 IPv4/IPv6 地址，不支持域名、CIDR 或通配符。默认空列表，不信任任何 HTTP 转发头。通过文件修改后重启主服务；已登录管理员也可通过 `POST /api/config` 的 `server.trusted_http_proxies` 更新或清空名单。

程序同时检查实际 TCP 对端和 PPv2 地址，再从右向左解析 `X-Forwarded-For`，选择首个不在名单内的地址；只有该头不存在时才使用 `X-Real-IP`。重复、无效或过长的头会安全回退到连接地址。IPv4-mapped IPv6 会规范化，避免同一来源以不同表示绕过登录限速。名单只应包含受控代理，Nginx 必须按上面的配置覆盖/追加来源头，不能原样信任浏览器提供的值。

该地址统一用于网页诊断、`/api/bootstrap`、`/api/status` 和密码/Passkey 登录限速。HTTP 头不提供浏览器的 TCP 源端口，因此这类地址的 `client.port` 为 `null`；`client.source` 表示 `x_forwarded_for`、`x_real_ip`、`proxy_protocol` 或 `socket`。使用 VPN 或其他代理的访客显示的是其公网出口 IP，而非 NAT 后的设备地址。

可信 HTTP 名单不会改变已有 PPv2 的接受策略；启用 PPv2 的 KVM 监听端口仍应只允许可信隧道/管理网访问，不能暴露给不受信任的客户端。它也不会放宽 Origin/Host 校验。

### WebRTC H.264 的固定 UDP 端口

HTTP 信令使用 TCP，WebRTC 媒体使用独立的 UDP socket。TCP 和 UDP 可以使用相同端口号。例如路由器已配置公网 `38319/TCP、UDP → KVM 8080/TCP、UDP`，可合并以下片段到现有配置：

```json
{
  "video": {
    "h264": {
      "udp_bind_address": "0.0.0.0:8080",
      "public_udp_endpoint": "kvm.example.com:38319"
    }
  }
}
```

将域名替换为指向该映射公网地址的真实域名，也可使用 `公网IP:端口`。程序在每次协商时从 KVM 主机解析域名，将公网 IP 和映射后的端口写入 ICE 候选；所有 WebRTC 会话通过 UDP mux 共用本地监听端口。IP 协议必须与监听地址一致。

这两个字段也支持通过 `POST /api/config` 的 `video.h264` 更新。省略字段会保留当前设置，显式将两项设为 `null` 可恢复默认的随机 UDP 端口。普通视频表单保存不会清除映射设置。此配置要求对应的 UDP 转发可达；只有 HTTP 反向代理的链路仍需要独立的媒体通道或 TURN 中继。

### WebRTC 会话生命周期与编码器探测资源限制

WebRTC 会话不再按固定 10 分钟强制断开。每个会话由一个生命周期 supervisor 管理，并绑定发起 offer 的登录会话：

- 登录被注销、过期或服务停止时立即结束对应会话；
- 尚未建立连接的 `New`/`Connecting` 会话最多保留 30 秒；
- `Disconnected` 最多等待 15 秒，恢复连接则重新计时；
- 已连接但 60 秒内没有任何已写入的 H.264 访问单元时回收；
- 只要持续产出编码帧，已连接会话可以超过 10 分钟继续工作，直到登录会话结束或浏览器主动关闭。

关闭、重新配置、连接失败/关闭和会话替换都会走同一条关闭路径：先停止编码子进程与管道任务，再从会话表移除条目，最后关闭 PeerConnection。会话表条目会在关闭完成前移除，避免状态回调重入造成死锁；关闭等待有界，不会无限阻塞配置接口。

FFmpeg 编码器探测使用独立进程组和有界监督：启动或重新配置时探测期限为 3 秒（异常终止后的回收最多另等 2 秒），stdout 与 stderr 合计最多读取 1 MiB。超时、输出超限或读取失败时会先终止整个进程组并回收子进程，错误只返回通用诊断类别，不输出任意命令内容。探测失败不会阻止 HTTP/MJPEG 启动；WebRTC 会显示为不可用，`/api/bootstrap` 中的 `webrtc.available` 为 `false` 并带有说明。重新配置同样受 3 秒探测期限及 1 MiB 输出限制；极端情况下无法及时回收的子进程句柄会保留在最多 4 项的有界注册表中，后续探测先检查回收状态，容量占满时拒绝启动新探测。该实现依赖 Unix 非阻塞管道与进程组；非 Unix 平台会明确报告 WebRTC 不可用，而不会退回到无界读取。

可用 `scripts/verify_p2b_probe.py --binary ... --scratch-dir ... --report ...` 使用伪 FFmpeg 验证正常、挂起、输出洪泛、健康检查和视频配置请求的有界行为；该脚本不需要真实 FFmpeg 或采集硬件。

## 附录：终端密码同步与监听地址

以下内容原位于 README，属于运维与开发细节。

### 终端密码同步与降级说明

初始化或迁移旧版账号时，WingmanKVM 会尝试把管理员密码同步给内置终端用户 `wingman`。helper 的输入与等待期限为 5 秒，超时后最多再等待 2 秒回收进程。密码只通过标准输入传递，不会以明文写入 `auth.json` 或日志。

升级时须同时更新部署 helper：它使用 GNU coreutils 的 `/usr/bin/timeout`，在提权侧对 `chpasswd` 设置 3 秒期限，并在额外 1 秒后强制终止，避免依赖服务用户终止 root 进程。

- 同步成功时，`auth.json` 仅记录 `system_password_synced: true`，网页终端可用。
- 同步失败或超时时，初始化/KVM 登录仍会完成，画面、键鼠、电源与虚拟介质不受影响；但网页终端会保持不可用，并显示通用原因，终端 WebSocket 返回 `503`。首次设置不会因此失败，登录也不会被误报为完全就绪。
- 修复后使用同一正确密码**退出并重新登录**即可重试；成功后标记为已同步，后续登录直接跳过 helper。Passkey 登录不会在标记为 false 时伪造终端就绪状态。
- 并发登录只允许一个迁移尝试运行；其他登录立即以终端降级状态完成，不会排队等待。

### 监听地址配置

`server.listen_address` 接受不带方括号的 IPv4 或 IPv6 字面地址，`server.port` 范围为 1–65535；加载与保存使用相同校验。通过完整配置 PUT 修改监听地址或端口后，需要重启服务才会重新绑定；这类成功响应带有 `X-WingmanKVM-Restart-Required: true`。响应头表示本次提交更改了监听设置，不是当前进程监听状态的查询。

HID 的 `write_timeout_ms` 和 `retry_interval_ms` 已弃用：旧配置文件兼容加载并提示，HTTP 配置更新会明确拒绝这两个字段。实际仍使用每份报告 500 ms 的固定期限和就绪等待，详见 [HID 配置迁移](docs/HID_CONFIGURATION.md)。
