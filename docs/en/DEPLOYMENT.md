# Deployment guide

> This is a translation of [docs/DEPLOYMENT.md](../DEPLOYMENT.md); the Chinese version is authoritative.

The recommended way to deploy WingmanKVM is to prepare a Linux binary matching the target machine's architecture, then run the `deploy/install.sh` script included in the repository. The installer creates the user, permissions, USB Gadget, systemd services, and state directories; the first-run web wizard scans for and fills in the capture card, HID, and virtual media devices. A normal installation does not require writing configfs scripts by hand.

## One-line install

```bash
curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh | sudo bash -s -- --install-deps
```

The script detects `aarch64` / `armv7l`, downloads the latest Release, verifies it against `SHA256SUMS` and runs the installer. To pin a version or install without starting:

```bash
curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh \
  | sudo bash -s -- --version v1.2.3 --no-start
```

To review the script first, download it, read it, then run `sudo bash get.sh`.

## 1. System requirements

The target KVM host needs:

- Linux `aarch64`, `armv7`, or `x86_64`;
- systemd, udev, Bash, `sudo`, Python 3, GNU coreutils (`timeout`), and util-linux (`flock`);
- a USB Device/OTG controller that is available or can be switched to Device mode (Linux UDC);
- kernel support for USB Gadget/configfs, the HID function, and the Mass Storage function;
- a USB HDMI capture card that supports V4L2 mmap;
- for power control, libgpiod 2.x `gpioset`; this can be omitted when GPIO is not used;
- for optional HDMI audio, FFmpeg with ALSA input and the `libopus` encoder (the Debian/Ubuntu `ffmpeg` package has both) and the capture card's USB sound card visible in `/proc/asound`; nothing extra is needed while audio stays disabled.

Common runtime and diagnostic tools on Debian, Ubuntu, and Armbian:

```bash
sudo apt update
sudo apt install -y sudo udev python3 coreutils util-linux libgpiod-tools v4l-utils
```

The official installer requires systemd and a Linux UDC, and deploys the full HID and Mass Storage composite Gadget; a video-only installation mode without a UDC is not currently provided. Starting the binary manually is suitable only for source development and diagnostics; it is not a supported production deployment and does not replace the Gadget, permissions, and persistent services.

## 2. Obtaining the Linux binary

### Installing from a release package (recommended)

Release packages are built by GitHub Actions when a `v*` tag is pushed. They are named `wingmankvm-v<version>-<target>.tar.gz` and contain the binary, the complete `deploy/` directory, `LICENSE`, `README.md`, and `CHANGELOG.md`. Choose `aarch64-unknown-linux-gnu` for ARM64 hosts and `armv7-unknown-linux-gnueabihf` for 32-bit ARM hosts.

1. Download the `.tar.gz` for your architecture and the `SHA256SUMS` from the same Release from [Releases](https://github.com/XiNian-dada/WingmanKVM/releases), and put them in the same directory.
2. Verify and extract:

   ```bash
   sha256sum -c --ignore-missing SHA256SUMS
   mkdir -p wingmankvm-release
   tar -xzf wingmankvm-v<version>-<target>.tar.gz -C wingmankvm-release
   ```

   `--ignore-missing` allows you to download only the release package for the current architecture; do not continue if verification fails.
3. Run the installer from the extracted `deploy/` directory and point `--release` at the release package:

   ```bash
   cd wingmankvm-release
   sudo ./deploy/install.sh --release ../wingmankvm-v<version>-<target>.tar.gz
   ```

When the installer finds `SHA256SUMS` in the same directory as the release package, it verifies the checksums again, then unpacks the package into a securely created temporary directory and reuses the file installation, health check, failure rollback, and upgrade recovery flow of `--binary`. The temporary directory is cleaned up on success, failure, and rollback paths.

### Building from source

#### Building on the target machine or on same-architecture Linux

```bash
sudo apt install -y \
  build-essential pkg-config clang libclang-dev linux-libc-dev

git clone https://github.com/XiNian-dada/WingmanKVM.git
cd WingmanKVM
cargo build --release --locked
```

A recent stable Rust is required. `v4l2-sys-mit` uses `linux/videodev2.h`, clang, and libclang through bindgen; if the build reports that the header cannot be found, make sure the Linux UAPI headers are installed.

#### Building a Linux ARM64 version on an Apple Silicon Mac

macOS ARM64 binaries cannot run directly on ARM64 Linux. You can build in a native ARM64 Docker container:

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

The generated file is `dist/wingmankvm`. The target board only needs this binary and the repository's `deploy/` directory; no Rust toolchain is required. The container distribution's glibc should not be much newer than the target system; otherwise use a build image close to the target system or an appropriate cross-compilation sysroot.

## 3. One-command installation

Keep the complete `deploy/` directory on the target KVM host, then run:

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm
```

If you just finished building on the target machine, you can use:

```bash
sudo ./deploy/install.sh --binary ./target/release/wingmankvm
```

If you are using a release package from section 2, use `--release` instead; there is no need to extract the binary manually:

```bash
sudo ./deploy/install.sh --release /path/to/wingmankvm-v<version>-<target>.tar.gz
```

The installer verifies the contents of `SHA256SUMS` when it exists in the same directory as the release package; `--binary` and `--release` are mutually exclusive.

### LAN name access (mDNS, `wingmankvm.local`)

With `--mdns` the installer advertises `wingmankvm.local` on the LAN through avahi-daemon, and both the post-install output and `wingmankvm-setup-url` print `http://wingmankvm.local:8080/...` first, followed by the IP addresses. This gives a stable address when the IP changes. Note that Passkey (WebAuthn) requires a secure context, which `http://*.local` is not, so Passkey still needs access through an HTTPS reverse proxy.

```bash
# Combined with --install-deps, avahi-daemon is installed automatically
sudo ./deploy/install.sh --install-deps --mdns --release /path/to/wingmankvm-v<version>-<target>.tar.gz
# Custom label (1-63 lowercase letters, digits or hyphens), advertised as kvm-rack1.local
sudo ./deploy/install.sh --mdns --mdns-hostname kvm-rack1 --release /path/to/wingmankvm-v<version>-<target>.tar.gz
```

Notes:

- Off by default. Without `--install-deps` the installer stops if `avahi-daemon` is missing.
- The installer writes `/etc/avahi/services/wingmankvm.service` (`_http._tcp` on `server.port` from `config.json`) and never changes the system hostname. Only when the name avahi would announce differs from the requested one does it set `host-name=` in the `[server]` section of `/etc/avahi/avahi-daemon.conf`, keeping a one-time backup at `avahi-daemon.conf.wingmankvm.bak`; it then enables and restarts `avahi-daemon`.
- The avahi files are not managed by the upgrade transaction: an install rollback does not undo them, and upgrades without `--mdns` leave an existing advertisement unchanged. To stop advertising, delete the service file and restore `avahi-daemon.conf` from the backup.
- On name collisions avahi renames automatically (e.g. `wingmankvm-2.local`); give each device its own `--mdns-hostname`. Clients need mDNS support (macOS, iOS, most Linux desktops and Windows 10+ do; some Android versions do not).
- Changing the port in the web UI later does not affect `.local` resolution, but the advertised service port is only refreshed by rerunning the installer.

The installer validates the binary format and CPU architecture and idempotently:

- creates the `wingmankvm` service user, the `wingman` web terminal user, and the `wingmankvm-hw` hardware group;
- creates `/var/lib/wingmankvm` and `/var/lib/wingmankvm/images`;
- installs udev rules, the terminal profile, a minimal password sync helper, and sudoers rules;
- installs and starts `wingmankvm-gadget.service`;
- creates the Boot Keyboard, Boot Relative Mouse, Absolute Pointer, and Mass Storage LUN;
- creates stable symlinks `/dev/wingmankvm-keyboard`, `/dev/wingmankvm-mouse`, and `/dev/wingmankvm-absolute` when the kernel provides a HID function device number mapping;
- installs, enables, and starts `wingmankvm.service`.

After installation it listens on `0.0.0.0:8080` by default. The installer prints the first-run setup address with a one-time token directly; the page removes the token from the address bar as soon as it has been read, so there is no need to copy it manually or check the logs first. The installer does not write the password into the command line or unit files; during first-run web initialization, the administrator password is synced to the local terminal user `wingman` through a restricted helper.

The web terminal runs as the `wingman` user, and a new installation does not grant that user sudo privileges by default. The host administrator can explicitly enable root maintenance with `--maintenance-mode privileged`; in that mode the service can reset the maintenance password, so it accepts the boundary that "if the service is compromised, the host can also be taken over." When an older version already has management authorization, upgrades must explicitly choose `restricted` or `privileged`; existing access is never changed silently. For full enablement, disablement, and external authorization checks, see [maintenance privilege mode](../MAINTENANCE_MODES.md) (Chinese).

The application still runs as `wingmankvm` with no login shell, but nologin by itself is not complete isolation. The unit actually opens writable mounts for `/var/lib/wingmankvm`, `/run/sudo`, `/etc`, and optionally `/sys/kernel/config/usb_gadget`; the root password helper needs `/etc` to update shadow, and normal Unix permissions still apply.

Running the same command again performs an upgrade. The installer updates the binary and managed deployment files but preserves:

- `/var/lib/wingmankvm/config.json`;
- `/var/lib/wingmankvm/auth.json`;
- `/var/lib/wingmankvm/images/`;
- an existing `/etc/wingmankvm/gadget.env`;
- an existing `wingman` user profile.

## 4. UDC and board-level USB role

The default configuration selects the first unoccupied UDC. Most development boards that already put the OTG port in Device mode can be installed directly without changing any configuration:

```bash
ls -l /sys/class/udc
```

If the `/sys/class/udc` directory exists but is temporarily empty, the board has multiple UDCs, or the USB role must first be switched through sysfs, install the files only first:

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm --no-start
sudoedit /etc/wingmankvm/gadget.env
```

This applies only when the hardware really has a Device/OTG controller and a UDC can appear after the USB role is configured. If the system has no `/sys/class/udc`, or the physical controller can only act as a Host, `--no-start` can only stage the files and cannot form a supported WingmanKVM deployment.

Common configuration items:

```ini
# When empty, select the first free UDC
WINGMANKVM_GADGET_UDC=

# Some boards require writing device first; the path depends on the board and kernel
WINGMANKVM_USB_ROLE_PATH=
WINGMANKVM_USB_ROLE_VALUE=device

# Gadget directory name in configfs
WINGMANKVM_GADGET_NAME=wingmankvm
```

`WINGMANKVM_USB_ROLE_PATH` cannot be reliably guessed across boards; rely on the board documentation, device tree, and current kernel interfaces. After configuring it, start the services:

```bash
sudo systemctl enable --now \
  wingmankvm-gadget.service wingmankvm.service
```

A UDC can be bound to only one Gadget at a time. If an old Python script, a board vendor service, or another configfs Gadget already occupies the target UDC, stop its management service first; WingmanKVM will not take over a UDC that another Gadget is using. You can locate the occupant with the following commands:

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
systemctl --no-pager --full status wingmankvm-gadget.service
```

Do not point `WINGMANKVM_GADGET_NAME` at a third-party Gadget that you do not intend WingmanKVM to take over. When the managed Gadget configuration changes, the service may need to briefly unbind and re-enumerate USB; if virtual media is still mounted, the service refuses to rebuild to avoid corrupting the image.

## 5. First-run web setup

After a normal installation, open the one-time setup address shown at the end of the installer output. If the terminal output has been lost, obtain the address or token that is valid only before the administrator is created from the logs:

```bash
sudo journalctl -u wingmankvm -b --no-pager
```

Open `http://KVM-host-address:8080/`, then complete setup with the two-step wizard:

1. Create the administrator account; the one-time address fills in the setup token automatically. The password must be at least 12 characters and contain uppercase letters, lowercase letters, digits, and symbols;
2. In "Check connection", confirm the automatic detection results and choose whether to enable virtual media; GPIO power control can be skipped and configured later.

Automatic detection:

- prefers V4L2 video nodes that support MJPEG;
- prefers the stable HID paths created by the official Gadget service from function device numbers;
- automatically fills in the single available and bound Mass Storage LUN;
- uses the default image directory `/var/lib/wingmankvm/images`.

When the wizard is submitted, the backend scans again, so it does not depend on paths cached by the browser. For custom Gadgets, role detection reads configfs `subclass`, `protocol`, and `report_length`, then matches `/dev/hidg*` through the function `dev` major:minor, and checks that the function is linked and the Gadget is bound. Raw device paths are under "Advanced settings"; a default installation usually does not need to expand them. Some older kernels do not provide the function `dev` attribute and cannot reliably determine roles automatically; in that case you must confirm manually and fill in the keyboard, relative mouse, and absolute pointer paths.

The following cases still require manual judgment:

- the capture card exposes multiple video nodes that all support MJPEG;
- multiple usable Gadgets or LUNs exist in the system;
- the GPIO chip, actual wiring, and trigger level. Automatic scanning can only see GPIO character devices and cannot infer the physical wiring from software; you need to select active-high or active-low in the web UI.

The setup token becomes invalid immediately after initialization completes. Afterwards you can rescan or adjust the configuration in the web UI's "Devices" and "Virtual media" panels.

## 6. Updating and reconfiguring

To update the binary, run the installer again:

```bash
sudo ./deploy/install.sh --binary /path/to/new/wingmankvm
```

Before upgrading, it pre-checks and stages all managed files; snapshots are stored in `/var/lib/wingmankvm-deploy/`, accessible only to root and separate from application data. If the new service does not pass `/healthz` within the 15-second health window, the entire set of files and the previous service state are restored; a SIGKILL leaves a pending-recovery marker, and later installations stop and print a standalone recovery script. Snapshot integrity after a power loss or filesystem crash must be checked manually; recovery from storage failure is not guaranteed. `auth.json`, `config.json`, images, and existing Gadget configuration are not overwritten by rollback. `--no-start` only verifies file installation and does not claim runtime acceptance. See [upgrade and recovery](UPGRADE_RECOVERY.md) for details.

If an older version's sudoers includes root maintenance authorization, the command above requires adding `--maintenance-mode restricted` or `--maintenance-mode privileged`.

If the Gadget service is already running normally, the installer keeps the current USB connection and does not actively re-enumerate. After modifying `/etc/wingmankvm/gadget.env`, restart the Gadget service explicitly:

```bash
sudo systemctl restart wingmankvm-gadget.service
```

This briefly disconnects the keyboard, mouse, and virtual USB devices from the target machine. Before restarting, you must unmount them safely from the target machine and eject any writable IMG in WingmanKVM; the Gadget service refuses to unbind while a backing file still exists.

`--no-start` installs files only; it does not enable or start services and does not stop services that are already running. It is suitable for handling board-level USB roles first, migrating an old Gadget, or scheduling a maintenance window.

## 7. Post-installation checks

Check the two services and the application endpoints:

```bash
systemctl --no-pager --full status \
  wingmankvm-gadget.service wingmankvm.service
journalctl -u wingmankvm-gadget -u wingmankvm -b --no-pager
curl -fsS http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/api/bootstrap
```

Check the UDC, stable HID symlinks, and Mass Storage LUN:

```bash
cat /sys/kernel/config/usb_gadget/wingmankvm/UDC
ls -l \
  /dev/wingmankvm-keyboard \
  /dev/wingmankvm-mouse \
  /dev/wingmankvm-absolute
find /sys/kernel/config/usb_gadget \
  -path '*/functions/mass_storage.*/lun.0' -type d -print
```

If you changed `WINGMANKVM_GADGET_NAME`, replace the directory name in the first command accordingly. The page status should show the actually negotiated video resolution and frame rate. If the video device is unplugged or occupied by another process, the capture supervisor displays an error and retries with a 1-second backoff; HID, GPIO, the terminal, and virtual media do not exit along with it.

## 8. Common troubleshooting

### UDC not found

```bash
ls -l /sys/class/udc
mount | grep configfs
journalctl -u wingmankvm-gadget -b --no-pager
```

Confirm that you are using a physical USB port with Device/OTG capability, not an interface that can only act as a Host. Some development boards also require a device tree overlay, kernel modules, or a board-level role-switch path; provided the hardware really supports Device mode, you can first complete file deployment with the installer's `--no-start`, then modify `gadget.env`. Hosts without a Linux UDC are outside the current official installation scope.

### Gadget service fails to start

```bash
cat /sys/kernel/config/usb_gadget/*/UDC 2>/dev/null
systemctl --no-pager --full status wingmankvm-gadget.service
journalctl -u wingmankvm-gadget -b --no-pager
```

The most common causes are the UDC being occupied by an old Gadget, the specified role path not existing, or the kernel lacking `libcomposite`, the HID function, or the Mass Storage function. If the log says virtual media is still mounted, unmount the filesystem on the target machine first, then eject the image in the web UI.

### The web UI does not detect HID

```bash
ls -l /dev/hidg* /dev/wingmankvm-* 2>/dev/null
find /sys/kernel/config/usb_gadget \
  -path '*/functions/hid.*' -type d -print
```

The official Gadget service creates stable symlinks from the major:minor in each HID function's `dev` attribute and does not depend on the global `hidgN` numbering. Role detection for custom Gadgets in the web UI also relies on `subclass`, `protocol`, `report_length`, and this device number mapping; it does not guess based only on the `/dev/hidgN` number. If an older kernel does not expose the function `dev`, scanning can only list `/dev/hidg*` as unverified nodes; check the roles against the custom Gadget's creation order and descriptors first, then fill in the paths manually in the web UI's advanced settings.

### The web UI does not detect the capture card

```bash
v4l2-ctl --list-devices
ls -l /dev/video*
v4l2-ctl -d /dev/video0 --list-formats-ext
```

Choose the capture node that actually supports `MJPG`. A single capture card may expose metadata, raw video, or multiple interfaces at the same time; do not judge only by the `/dev/videoN` number. Make sure an old Python/OpenCV test program is not still occupying the device.

### No sound after enabling HDMI audio

```bash
cat /proc/asound/cards          # the capture card should appear as a USB-Audio card
cat /proc/asound/pcm            # its entry should include capture
id wingmankvm                   # should include the audio group
ffmpeg -hide_banner -encoders | grep libopus
```

Check, in order: the browser is in H.264 (WebRTC) mode (MJPEG carries no audio); the speaker button in the toolbar is unmuted (every page load starts muted); the "HDMI audio" state in the diagnostics panel. `no_device` means automatic selection found no sound card on the same USB device as the capture card, or several USB sound cards it cannot tell apart; pick the device under "Video → HDMI audio". `error` carries FFmpeg's last error line (for example, the device is busy in another program) and the service retries with backoff. The installer adds `wingmankvm` to the `audio` group; after upgrading an older installation, restart the service so the membership takes effect. Audio failures never disconnect or restart video.

### GPIO power control is unavailable

```bash
gpiodetect
gpioinfo gpiochip1
gpioset --version
```

The web UI asks for `gpiochipN` and the line offset, not the physical header pin number; also select whether the relay is active-high or active-low. Confirm the relay logic with the schematic and a multimeter before enabling power control. The current command format requires libgpiod 2.x.

### Virtual media is unavailable

```bash
find /sys/kernel/config/usb_gadget \
  -path '*/functions/mass_storage.*/lun.0' -type d -print
```

The official Gadget service creates the LUN and authorizes the `file`, `ro`, `cdrom`, `removable`, and optional `forced_eject` attributes. ISOs are exposed with `cdrom=1, ro=1`; IMGs are exposed with `cdrom=0` and can optionally use `ro=0` for read-write.

A writable IMG needs safe ejection just like a real USB drive: unmount or eject it on the target machine first, then click "Eject" in WingmanKVM. Do not let the KVM host and the target machine read and write the same image at the same time. Forced ejection or USB unbinding can corrupt the filesystem.

It is recommended to keep the default image directory `/var/lib/wingmankvm/images` in the web UI. If you really need to move it outside `/var/lib/wingmankvm`, create the directory first and add systemd write permission for the application service, for example:

```bash
sudo install -d -o wingmankvm -g wingmankvm -m 0700 /srv/wingmankvm-images
sudo systemctl edit wingmankvm.service
```

Write the following in the editor:

```ini
[Service]
ReadWritePaths=/srv/wingmankvm-images
```

Then run:

```bash
sudo systemctl daemon-reload
sudo systemctl restart wingmankvm.service
```

Finally, change the image directory in the web UI to `/srv/wingmankvm-images`. Changing only the web UI path or the file owner is not enough; `ProtectSystem=strict` blocks uploads and writes to paths not listed in `ReadWritePaths=`.

## 9. Advanced: custom Gadget checks

This section applies only to people who already have a board vendor Gadget, need custom USB VID/PID/descriptors, or are troubleshooting kernel compatibility. For a default installation, use `wingmankvm-gadget.service` and do not manually create duplicate functions with the same name.

If you use your own Gadget manager, the application unit must depend on your Gadget service instead of starting the official `wingmankvm-gadget.service` at the same time. A UDC can have only one manager; modify a copy of `deploy/wingmankvm.service` or create an appropriate systemd override before deploying.

For custom Gadgets, WingmanKVM's HID role detection requires the function to be linked to the USB config, the Gadget to be bound to a UDC, and the function `dev` major:minor to map to the actual `/dev/hidg*`. The role parameters should satisfy:

| Interface | `subclass` | `protocol` | `report_length` |
| --- | ---: | ---: | ---: |
| Boot Keyboard | `1` | `1` | `8` |
| Boot Relative Mouse | `1` | `2` | `4` |
| Absolute Pointer | `0` | `0` | `6` |

Check a custom function:

```bash
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/subclass
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/protocol
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/report_length
cat /sys/kernel/config/usb_gadget/*/functions/hid.*/dev
```

If an older kernel has no `dev` attribute, WingmanKVM cannot reliably map functions to the globally numbered `/dev/hidgN`, and you must confirm the paths manually based on the Gadget manager's creation and linking order. The report descriptor must be consistent with the report length. The Absolute Pointer uses 3 buttons, 16-bit absolute X/Y (`0..=32767`), and an 8-bit relative wheel; the official no-Report-ID descriptor is:

```text
05 01 09 02 A1 01 09 01 A1 00
05 09 19 01 29 03 15 00 25 01 95 03 75 01 81 02
95 01 75 05 81 03
05 01 09 30 09 31 16 00 00 26 FF 7F 75 10 95 02 81 02
09 38 15 81 25 7F 75 08 95 01 81 06
C0 C0
```

A custom Mass Storage function must create `lun.0`, be linked to a config bound to a UDC, and allow the `wingmankvm-hw` group to modify the necessary attributes:

```bash
LUN=/sys/kernel/config/usb_gadget/你的gadget/functions/mass_storage.0/lun.0
for ATTR in file ro cdrom removable forced_eject; do
  if [ -e "$LUN/$ATTR" ]; then
    sudo chgrp wingmankvm-hw "$LUN/$ATTR"
    sudo chmod g+rw,o-rw "$LUN/$ATTR"
  fi
done
```

`forced_eject` is not available on all kernels. Permissions may return to defaults after configfs recreates the function, so these grants should be placed in the custom Gadget's startup service rather than executed only once by hand. Image files must also be readable by the `wingmankvm` user; writable IMGs also require write permission.

## 10. Video modes

Recommended priority:

1. capture card native MJPEG + `mjpeg_passthrough`;
2. lower the capture resolution or frame rate;
3. use `transcode_jpeg` and JPEG quality only when you need to limit network bandwidth.

Passthrough still copies the `bytesused` range once from the mmap buffer, because the driver reuses the buffer after the next dequeue/queue; what it eliminates is JPEG decoding and re-encoding.

### HDMI audio (optional, WebRTC only)

Most UVC capture cards also expose a USB Audio sound card. When enabled, the card's sound is encoded to Opus and sent with the WebRTC session; MJPEG mode does not support audio, and the speaker button on the page shows itself as unavailable with the reason. The feature is off by default. Enable it under "Video → HDMI audio", or merge this configuration fragment:

```json
{
  "audio": {
    "enabled": true,
    "device": null,
    "bitrate_kbps": 96
  }
}
```

- With `device` set to `null`, selection is automatic: the sound card on the same USB device as the current capture card (`video.device`) first, otherwise the only USB sound card; with several indistinguishable cards nothing is guessed. You can also enter an ALSA name such as `plughw:CARD=MS2109,DEV=0` or `hw:1,0` (only `hw`, `plughw`, `dsnoop`, `sysdefault` and `default` are accepted).
- Detection only reads `/proc/asound/cards`, `/proc/asound/pcm` and `/sys/class/sound`; it never opens an ALSA device, so it cannot interrupt a running capture. "Detect audio devices" in the settings and the `audio` list of `POST /api/devices/scan` come from it.
- `bitrate_kbps` ranges from 16 to 256. Audio uses the same FFmpeg as H.264 (`video.h264.ffmpeg_path`) and encodes 48 kHz stereo Opus in 20 ms frames.
- All WebRTC sessions share one FFmpeg capture process: the first session that needs audio starts it and the last one to end stops it and reaps its whole process group. When capture fails or produces no data for 10 seconds the state becomes `error`, and it is retried with a 1–30 second backoff while viewers remain.
- Audio and video are independent: the browser only asks for an audio track when the server has audio enabled, and the server only adds the Opus track when a device is detected; otherwise only video is negotiated, as before. Audio forwarding is not part of the session watchdog, so a failing capture, an unplugged device or a changed audio setting never disconnects, restarts or renegotiates video. Changing only the audio settings does not restart video capture either. Established sessions keep their tracks: a session with an audio track switches to the new capture device, and a session opened before audio was enabled needs to reconnect to get audio (applying the settings page reconnects the current page automatically).
- The browser always starts muted (to satisfy autoplay policies); click the speaker button in the toolbar to unmute. If the browser refuses playback with sound, the page returns to muted and keeps showing the picture.
- The `audio` field of `/api/status` reports `state` (`disabled`, `no_device`, `idle`, `starting`, `running`, `error`), the current device, the error message, the number of viewers and the cumulative retry count; the "HDMI audio" entry of the diagnostics panel shows the same.

Permissions: ALSA device nodes `/dev/snd/*` usually belong to the `audio` group. When that group exists, the installer adds the service user `wingmankvm` to it, and systemd applies the supplementary group when it starts the service. The unit's `ProtectSystem=strict` only makes the file system read-only; it does not affect `/dev`, `/proc` or `/sys`, and the unit sets neither `PrivateDevices` nor a device allow-list, so no hardening needs to be relaxed.

## 11. HTTPS

When `server.public_origin` is not configured or `http://...` is used explicitly, cookies do not carry `Secure` and are suitable only for debugging on a trusted management network. After configuring `https://...`, first-run setup, password/Passkey login, and logout responses uniformly use Secure cookies and do not trust forwarded headers to downgrade. Before migrating, revoke old sessions and clear old cookies, then log in again over HTTPS and close the public HTTP entry point. See [session cookie policy](../SESSION_COOKIES.md) (Chinese) for details. For production, it is recommended to:

- use a VPN such as WireGuard/Tailscale; or
- terminate HTTPS behind Caddy/Nginx and restrict the source network.

Do not map the console port directly to the public internet.

### Reverse proxy, NAT traversal, and client IP

`server.proxy_protocol` supports `disabled`, `auto`, and `required`, and handles PROXY Protocol v2 before the TCP connection. It does not automatically make the HTTP `Host` consistent with the browser `Origin`. An Nginx HTTP reverse proxy should preserve the public Host:

```nginx
proxy_set_header Host $http_host;
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
```

The browser terminal WebSocket strictly validates `Origin` before the upgrade and does not trust `X-Forwarded-Proto`, `Forwarded`, or other forwarded headers to determine the protocol. A direct HTTP deployment accepts only `http` origins identical to the request `Host`. If a reverse proxy terminates HTTPS, configure the single public origin (scheme + host + port; the default port may be omitted) in `config.json` in the state directory:

```json
{
  "server": {
    "public_origin": "https://kvm.example.com"
  }
}
```

After configuration, the terminal accepts only that exact origin and requires the request `Host` to match its host and port; the reverse proxy must preserve the external `Host` the browser used, as in the Nginx example above. Do not treat an internal direct address as a bypass, and do not keep using the old configuration after the proxy rewrites `Host`. Missing, `null`, multiple, comma-separated, or malformed `Origin` values are all rejected; when `public_origin` is not configured, HTTPS origins are likewise rejected. This field can also be set through `POST /api/config`'s `server.public_origin`; pass `null` explicitly to clear it.

If the path is "browser → Nginx → local tunnel port → internal tunnel → KVM", PPv2 may carry only Nginx's `127.0.0.1`, while the browser's public egress IP is in the HTTP forwarded headers. You can configure this explicitly in `config.json` in the state directory:

```json
{
  "server": {
    "proxy_protocol": "auto",
    "trusted_http_proxies": ["192.168.1.2", "127.0.0.1"]
  }
}
```

This is a fragment to merge into the existing configuration; do not overwrite the entire configuration file. The addresses must be replaced with the actual TCP tunnel peer and the proxy addresses in PPv2; only explicit IPv4/IPv6 addresses are supported, not domain names, CIDR, or wildcards. The default is an empty list, trusting no HTTP forwarded headers. Restart the main service after modifying the file; a logged-in administrator can also update or clear the list through `POST /api/config`'s `server.trusted_http_proxies`.

The program checks both the actual TCP peer and the PPv2 address, then parses `X-Forwarded-For` from right to left and selects the first address not in the list; only when that header is absent does it use `X-Real-IP`. Duplicate, invalid, or overly long headers safely fall back to the connection address. IPv4-mapped IPv6 is normalized so the same source cannot bypass login rate limiting through a different representation. The list should contain only controlled proxies; Nginx must override/append source headers as in the configuration above and must not trust values supplied by the browser as-is.

This address is used uniformly for web diagnostics, `/api/bootstrap`, `/api/status`, and password/Passkey login rate limiting. HTTP headers do not provide the browser's TCP source port, so `client.port` is `null` for such addresses; `client.source` indicates `x_forwarded_for`, `x_real_ip`, `proxy_protocol`, or `socket`. Visitors using a VPN or other proxy see their public egress IP, not the device address behind NAT.

The trusted HTTP list does not change the existing PPv2 acceptance policy; a KVM listening port with PPv2 enabled should still allow access only from trusted tunnels/management networks and must not be exposed to untrusted clients. It also does not relax Origin/Host validation.

### Fixed UDP port for WebRTC H.264

HTTP signaling uses TCP, and WebRTC media uses a separate UDP socket. TCP and UDP can use the same port number. For example, if the router is already configured with public `38319/TCP, UDP → KVM 8080/TCP, UDP`, you can merge the following fragment into the existing configuration:

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

Replace the domain name with a real domain that points to the mapped public address; `public-IP:port` also works. The program resolves the domain name from the KVM host on each negotiation and writes the public IP and mapped port into ICE candidates; all WebRTC sessions share the local listening port through UDP mux. The IP protocol must match the listen address.

These two fields can also be updated through `POST /api/config`'s `video.h264`. Omitting a field keeps the current setting, and explicitly setting both to `null` restores the default random UDP port. Saving the normal video form does not clear the mapping settings. This configuration requires the corresponding UDP forwarding to be reachable; a path with only an HTTP reverse proxy still needs a separate media channel or TURN relay.

### WebRTC session lifecycle and encoder probe resource limits

WebRTC sessions are no longer forcibly disconnected after a fixed 10 minutes. Each session is managed by a lifecycle supervisor and bound to the login session that created the offer:

- the corresponding session ends immediately when the login is logged out, expires, or the service stops;
- `New`/`Connecting` sessions that have not yet established a connection are kept for at most 30 seconds;
- `Disconnected` waits at most 15 seconds, and the timer restarts if the connection recovers;
- connected sessions are reclaimed when no written H.264 access unit appears for 60 seconds;
- as long as encoded frames continue to be produced, a connected session can keep working beyond 10 minutes, until the login session ends or the browser closes it actively.

Closing, reconfiguring, connection failure/close, and session replacement all go through the same shutdown path: first stop the encoder subprocess and pipe tasks, then remove the entry from the session table, and finally close the PeerConnection. The session table entry is removed before shutdown completes to avoid deadlocks caused by state callback reentry; shutdown waiting is bounded and never blocks the configuration API indefinitely.

FFmpeg encoder probing uses a separate process group and bounded supervision: the probe deadline is 3 seconds on startup or reconfiguration (reaping after abnormal termination may wait up to 2 more seconds), and stdout and stderr combined are read up to 1 MiB. On timeout, output overrun, or read failure, the entire process group is terminated first and the child process reaped; errors return only a generic diagnostic category and do not output arbitrary command content. Probe failure does not prevent HTTP/MJPEG from starting; WebRTC is shown as unavailable, and `webrtc.available` in `/api/bootstrap` is `false` with an explanation. Reconfiguration is also subject to the 3-second probe deadline and 1 MiB output limit; in extreme cases, subprocess handles that cannot be reaped in time remain in a bounded registry of at most 4 entries, subsequent probes check the reaping state first, and new probes are refused when capacity is full. This implementation depends on Unix non-blocking pipes and process groups; non-Unix platforms explicitly report WebRTC as unavailable instead of falling back to unbounded reads.

You can use `scripts/verify_p2b_probe.py --binary ... --scratch-dir ... --report ...` with a fake FFmpeg to verify the bounded behavior of normal, hung, output-flooding, health check, and video configuration requests; the script does not need a real FFmpeg or capture hardware.

## Appendix: terminal password sync and listen address

The following content was originally in the README and covers operations and development details.

### Terminal password sync and degradation

During initialization or migration of legacy accounts, WingmanKVM attempts to sync the administrator password to the built-in terminal user `wingman`. The helper's input and wait deadline is 5 seconds, after which it waits up to 2 more seconds to reap the process. The password is passed only through standard input and is never written in plaintext to `auth.json` or logs.

When upgrading, the deployment helper must be updated at the same time: it uses GNU coreutils' `/usr/bin/timeout`, sets a 3-second deadline for `chpasswd` on the privileged side, and forcibly terminates it after an additional 1 second, avoiding reliance on the service user to terminate a root process.

- When sync succeeds, `auth.json` records only `system_password_synced: true`, and the web terminal is available.
- When sync fails or times out, initialization/KVM login still completes, and video, keyboard/mouse, power, and virtual media are unaffected; however, the web terminal remains unavailable, shows a generic reason, and the terminal WebSocket returns `503`. First-run setup does not fail because of this, and login is not falsely reported as fully ready.
- After fixing it, use the same correct password to **log out and log in again** to retry; once successful it is marked as synced, and subsequent logins skip the helper directly. Passkey login does not fake terminal readiness while the flag is false.
- Concurrent logins allow only one migration attempt to run; other logins complete immediately in the degraded terminal state and do not queue.

### Listen address configuration

`server.listen_address` accepts IPv4 or IPv6 literal addresses without brackets, and `server.port` ranges from 1–65535; loading and saving use the same validation. After changing the listen address or port through a full configuration PUT, the service must be restarted to rebind; such successful responses carry `X-WingmanKVM-Restart-Required: true`. The response header indicates that this submission changed the listen settings; it is not a query of the current process's listening state.

HID's `write_timeout_ms` and `retry_interval_ms` are deprecated: old configuration files load with a warning, and HTTP configuration updates explicitly reject these two fields. Actual behavior still uses a fixed 500 ms deadline per report and readiness waiting; see [HID configuration migration](../HID_CONFIGURATION.md) (Chinese) for details.
