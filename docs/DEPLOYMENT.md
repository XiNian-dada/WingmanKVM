# 部署说明

## 1. 系统要求

- Linux ARM64/ARMv7 或 x86_64。
- 支持 V4L2 mmap 的 USB 视频采集卡。
- Linux USB Gadget/configfs 和可用 UDC。
- libgpiod 2.x 的 `gpioset`。
- systemd 为推荐但不是强制要求。

构建依赖：

```bash
sudo apt install build-essential pkg-config clang libclang-dev linux-libc-dev libgpiod-tools
```

如果 `v4l2-sys-mit` 报错找不到 `linux/videodev2.h`，先确认 Linux UAPI headers 已安装，并确保交叉编译 clang 的 include path 指向目标 sysroot。

## 2. 创建系统用户

```bash
sudo useradd --system --home /var/lib/wingmankvm --create-home --shell /usr/sbin/nologin wingmankvm
sudo install -d -o wingmankvm -g wingmankvm -m 0700 /var/lib/wingmankvm

# 创建用于网页终端登录的本机用户。密码会在首次网页初始化时同步，
# 不要在命令行或 unit 文件中写入密码。
sudo useradd --create-home --shell /bin/bash wingman
sudo install -o root -g root -m 0755 deploy/wingmankvm-set-wingman-password \
  /usr/local/sbin/wingmankvm-set-wingman-password
sudo install -o root -g root -m 0440 deploy/wingmankvm-sudoers \
  /etc/sudoers.d/wingmankvm
sudo visudo -cf /etc/sudoers.d/wingmankvm
```

安装示例 unit：

```bash
sudo install -m 0644 deploy/wingmankvm.service /etc/systemd/system/wingmankvm.service
sudo systemctl daemon-reload
sudo systemctl enable --now wingmankvm
sudo journalctl -u wingmankvm -f
```

首次日志中的 setup token 只在管理员创建前有效。

## 3. 设备权限

参考 [udev 规则示例](../deploy/99-wingmankvm.rules)，根据实际发行版和设备 VID/PID 收窄规则：

```bash
sudo groupadd --system wingmankvm-hw
sudo usermod -aG wingmankvm-hw wingmankvm
sudo install -m 0644 deploy/99-wingmankvm.rules /etc/udev/rules.d/99-wingmankvm.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

示例规则会授权所有 `hidg*`，生产环境最好创建稳定的 `/dev/wingmankvm-keyboard`、`/dev/wingmankvm-mouse` 和 `/dev/wingmankvm-absolute-pointer` symlink，并只授权实际使用的设备。

## 4. Gadget 检查

程序启动前检查：

```bash
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/subclass
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/protocol
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/report_length
find /sys/kernel/config/usb_gadget -path '*/mass_storage.*/lun.0' -type d
```

Boot Keyboard 应为 `1 / 1 / 8`，Boot Mouse 应为 `1 / 2 / 4`，Absolute Pointer 应为 `0 / 0 / 6`。report descriptor 必须与 report length 一致。建议同时保留相对和绝对鼠标接口：相对接口用于 BIOS/UEFI，绝对接口用于桌面系统的网页坐标同步。

Absolute Pointer 使用 3 个按钮、16 位绝对 X/Y（`0..=32767`）和 8 位相对滚轮。其无 Report ID descriptor 可写为：

```text
05 01 09 02 A1 01 09 01 A1 00
05 09 19 01 29 03 15 00 25 01 95 03 75 01 81 02
95 01 75 05 81 03
05 01 09 30 09 31 16 00 00 26 FF 7F 75 10 95 02 81 02
09 38 15 81 25 7F 75 08 95 01 81 06
C0 C0
```

Mass Storage LUN 初始化脚本可在创建 function 后，仅把目标属性授权给服务用户：

```bash
LUN=/sys/kernel/config/usb_gadget/你的gadget/functions/mass_storage.0/lun.0
sudo chgrp wingmankvm-hw "$LUN"/{file,ro,cdrom,removable,forced_eject}
sudo chmod g+rw "$LUN"/{file,ro,cdrom,removable,forced_eject}
```

不同内核/configfs 挂载方式可能会重置权限，应在 Gadget 初始化服务中执行，而不是依赖一次性的手工修改。

## 5. 视频模式

推荐优先级：

1. 采集卡原生 MJPEG + `mjpeg_passthrough`。
2. 降低采集分辨率或帧率。
3. 只有需要限制网络带宽时使用 `transcode_jpeg` 和 JPEG quality。

直通仍需从 mmap buffer 复制一次 `bytesused` 范围，因为驱动会在下一次 dequeue/queue 后复用缓冲区；它消除的是 JPEG 解码与重新编码。

## 6. 运行检查

```bash
curl -fsS http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/api/bootstrap
```

页面状态中应显示实际协商的分辨率和帧率。设备被拔出或被其它进程占用时，视频 supervisor 会显示错误并按 1 秒退避重试，其它 HID/GPIO/介质功能仍可继续使用。

## 7. HTTPS

直接 HTTP 模式不会给 Cookie 添加 `Secure`，适合可信管理网内调试。生产环境建议：

- 使用 WireGuard/Tailscale 等 VPN；或
- 在 Caddy/Nginx 后终止 HTTPS，并限制来源网络。

不要把控制台端口直接映射到公网。
