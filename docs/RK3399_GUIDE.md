# Firefly RK3399 部署实战指南

本文档记录在 Firefly RK3399 开发板（运行 Armbian / Debian ARM64）上完整部署 WingmanKVM 的实际步骤。直奔主题，照着步骤操作即可。

---

## 1. 硬件与接线

Firefly RK3399 板载接口丰富，具体连接如下：

```text
被控机 HDMI 显卡输出 ──> HDMI 采集卡 ──USB──> RK3399 上层 USB 3.0 Host 口
被控机 USB 接口      <── 双头 Type-C/USB ──> RK3399 Type-C OTG 口
被控机主板 POWER SW  <── 继电器常开端 ──────> RK3399 GPIO 排针（可选）
```

- **HDMI 采集卡**：插入 RK3399 的 **USB 3.0 Host 接口**（蓝色接口）。
- **OTG 线缆**：将 RK3399 板载的 **Type-C 接口**（具备 OTG / Device 模式）连接至被控机的 USB 接口，用于向被控机模拟键盘、鼠标和虚拟介质。
- **电源控制（可选）**：将继电器模块的控制端连接至 RK3399 的 GPIO（实例中使用 `gpiochip1` 的 `Line 7`），继电器的干接点并联到被控机主板的电源跳线开关。

---

## 2. 安装系统依赖

在 RK3399 的终端中执行：

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev \
  libgpiod-tools v4l-utils usbutils curl python3
```

> **版本要求**：GPIO 电源控制依赖 **libgpiod >= 2.x**（`gpioset` 2.x 命令语法）；安装器的健康检查与配置解析依赖 `python3`。Debian bookworm 的 `libgpiod-tools` 为 1.x，启用电源控制前请从 bookworm-backports 或更新的发行版安装；不使用 GPIO 时可跳过电源部分。完整依赖见 [DEPLOYMENT.md](DEPLOYMENT.md)。

---

## 3. 获取源码与编译

安装 Rust 工具链并编译 WingmanKVM：

```bash
# 如果尚未安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# 克隆并编译
git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

> **提示**：如果是在 PC 上交叉编译好的 Linux ARM64 二进制，直接将编译出的 `wingmankvm` 文件拷贝到板子上即可。

---

## 4. 一键安装与服务配置

在仓库根目录下运行官方安装脚本：

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

安装脚本会自动完成：
- 创建系统专属运行用户与权限组；
- 创建 Composite USB Gadget 规则（键盘、鼠标、绝对指针、虚拟介质）；
- 注册并启动 `wingmankvm-gadget.service` 与 `wingmankvm.service`；
- 开放默认端口 `8080`。

---

## 5. 首次使用与初始化

脚本执行完毕后，控制台会输出一个带初始化令牌的访问链接：

```text
http://192.168.1.xxx:8080/#setup=...
```

在同一局域网的浏览器中打开该地址：

1. **设置管理员账号与密码**：填写用户名并设置管理员强密码（该密码会同步给系统终端用户 `wingman`）；
2. **确认硬件自检结果**：
   - **视频设备**：系统会自动识别插入在 Host 口的 HDMI 采集卡（通常为 `/dev/video*`，支持 1080p 60fps MJPEG）；
   - **HID 控制**：自动匹配安装器建立的稳定设备节点 `/dev/wingmankvm-keyboard`、`/dev/wingmankvm-mouse` 与 `/dev/wingmankvm-absolute`；
   - **虚拟介质**：自动识别 USB Mass Storage LUN；
   - **电源控制（可选）**：若连接了继电器，选择控制器为 `gpiochip1`，线路填写 `7`，触发电平选择高电平。
3. 点击 **进入控制台**，即可开始接管被控机。

---

## 6. 常用状态检查命令速查

### 检查服务运行状态

```bash
# 查看服务状态
systemctl status wingmankvm-gadget.service wingmankvm.service

# 查看实时日志
journalctl -u wingmankvm -f
```

`wingmankvm-gadget.service` 为开机初始化脚本，状态为 `active (exited)` 即表示 Gadget 规则配置成功；`wingmankvm.service` 状态应为 `active (running)`。

### 检查 USB OTG 连通状态

被控机接上 OTG 线并开机后，在 RK3399 上查看 UDC 状态：

```bash
cat /sys/class/udc/*/state
```

正常连通枚举完成后输出应为：
```text
configured
```

### 检查采集卡识别与分辨率

```bash
# 查看系统中的视频设备
v4l2-ctl --list-devices

# 查看采集卡输出格式与最大帧率
v4l2-ctl -d /dev/video0 --list-formats-ext
```

### 检查接口健康状态

```bash
curl -fsS http://127.0.0.1:8080/healthz
```

返回 `{"service":"wingmankvm","status":"ok"}` 即代表服务完全健康。
