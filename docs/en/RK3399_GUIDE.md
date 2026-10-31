# Firefly RK3399 deployment walkthrough

> This is a translation of [docs/RK3399_GUIDE.md](../RK3399_GUIDE.md); the Chinese version is authoritative.

This document records the actual steps for a complete WingmanKVM deployment on a Firefly RK3399 development board (running Armbian / Debian ARM64). Get straight to the point; just follow the steps.

---

## 1. Hardware and wiring

The Firefly RK3399 has a rich set of onboard interfaces; the connections are as follows:

```text
Target machine HDMI graphics output ──> HDMI capture card ──USB──> RK3399 upper USB 3.0 Host port
Target machine USB port              <── double-ended Type-C/USB cable ──> RK3399 Type-C OTG port
Target machine motherboard POWER SW  <── relay normally-open contact ──────> RK3399 GPIO header (optional)
```

- **HDMI capture card**: plug it into the RK3399's **USB 3.0 Host port** (the blue port).
- **OTG cable**: connect the RK3399's onboard **Type-C port** (with OTG / Device mode) to the target machine's USB port, to emulate a keyboard, mouse, and virtual media for the target machine.
- **Power control (optional)**: connect the relay module's control side to the RK3399 GPIO (the example uses `Line 7` of `gpiochip1`), and wire the relay's dry contacts in parallel with the target machine motherboard's power jumper switch.

---

## 2. Installing system dependencies

Run the following in the RK3399 terminal:

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev \
  libgpiod-tools v4l-utils usbutils curl python3
```

> **Version requirement**: GPIO power control depends on **libgpiod >= 2.x** (`gpioset` 2.x command syntax); the installer's health check and configuration parsing depend on `python3`. Debian bookworm's `libgpiod-tools` is 1.x; before enabling power control, install it from bookworm-backports or a newer distribution; if you do not use GPIO, you can skip the power section. For the full dependency list, see [DEPLOYMENT.md](DEPLOYMENT.md).

---

## 3. Getting the source and building

Install the Rust toolchain and build WingmanKVM:

```bash
# If Rust is not installed yet
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Clone and build
git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

> **Tip**: If you cross-compiled a Linux ARM64 binary on a PC, just copy the resulting `wingmankvm` file to the board.

---

## 4. One-command installation and service configuration

Run the official installation script from the repository root:

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

The installation script automatically:
- creates the dedicated system runtime user and permission group;
- creates the Composite USB Gadget rules (keyboard, mouse, absolute pointer, virtual media);
- registers and starts `wingmankvm-gadget.service` and `wingmankvm.service`;
- opens the default port `8080`.

---

## 5. First use and initialization

After the script finishes, the console prints an access link with an initialization token:

```text
http://192.168.1.xxx:8080/#setup=...
```

Open that address in a browser on the same LAN:

1. **Set the administrator account and password**: enter a username and set a strong administrator password (this password is synced to the system terminal user `wingman`);
2. **Confirm the hardware self-check results**:
   - **Video device**: the system automatically detects the HDMI capture card plugged into the Host port (usually `/dev/video*`, supporting 1080p 60fps MJPEG);
   - **HID control**: automatically matches the stable device nodes created by the installer, `/dev/wingmankvm-keyboard`, `/dev/wingmankvm-mouse`, and `/dev/wingmankvm-absolute`;
   - **Virtual media**: automatically detects the USB Mass Storage LUN;
   - **Power control (optional)**: if a relay is connected, select `gpiochip1` as the controller, enter `7` for the line, and select active-high as the trigger level.
3. Click **Enter console** to start controlling the target machine.

---

## 6. Quick reference for common status checks

### Checking service status

```bash
# View service status
systemctl status wingmankvm-gadget.service wingmankvm.service

# View live logs
journalctl -u wingmankvm -f
```

`wingmankvm-gadget.service` is a boot-time initialization script; a status of `active (exited)` means the Gadget rules were configured successfully; `wingmankvm.service` should have the status `active (running)`.

### Checking USB OTG connectivity

After the OTG cable is connected to the target machine and it is powered on, check the UDC state on the RK3399:

```bash
cat /sys/class/udc/*/state
```

After a normal connection and enumeration, the output should be:
```text
configured
```

### Checking capture card detection and resolution

```bash
# List video devices in the system
v4l2-ctl --list-devices

# View the capture card's output formats and maximum frame rate
v4l2-ctl -d /dev/video0 --list-formats-ext
```

### Checking endpoint health

```bash
curl -fsS http://127.0.0.1:8080/healthz
```

A response of `{"service":"wingmankvm","status":"ok"}` means the service is fully healthy.
