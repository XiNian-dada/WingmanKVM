# MS2130 虚拟显示器与 EDID RAM

WingmanKVM 将分辨率拆成三个互不等价的层级：

```text
被控机 GPU
  ↓  Virtual Monitor：由 HDMI EDID 决定桌面逻辑分辨率
MS2130 HDMI RX
  ↓  Capture：由 V4L2/UVC 决定采集和传输尺寸
MJPEG 或 H.264/WebRTC
  ↓  Viewer：浏览器 Fit、1:1、Stretch、Fullscreen
网页
```

选择 `Virtual Monitor = 1280×720 @ 60Hz` 会给被控机呈现只声明该模式的 EDID，并通过 HPD 低/高模拟重新连接。选择 `Capture = Native` 时，只有 EDID RAM 回读成功且 HDMI RX 输入时序也确认变为 1280×720，V4L2 才跟随为 `1280×720 @ 60fps`。固定 Capture 模式仍可用于“1080p 桌面、720p 网络视频”。Viewer 设置只改变 CSS 显示，不参与 EDID 或采集配置。

## 支持范围

- 仅支持经过验证的 `345f:2130` MS2130。
- 使用设备的厂商 HID Feature Report（XDATA `B5/B6`），不是标准 UVC、V4L2 或 DRM EDID API。
- 只写 256 字节易失 EDID RAM，不包含任何 EEPROM/Flash 写命令。
- 当前内置 `1920×1080 @ 60Hz` 和 `1280×720 @ 60Hz` 两个严格模式；两者保留双声道 LPCM 声明。
- MS2130 没有标准 V4L2 接口报告 HDMI Source 时序。本项目在经过验证的 `345f:2130` 上读取运行寄存器 `F660–F663`（小端宽、高）作为 HDMI RX 输入时序证据，避免把 UVC 缩放后的 720p 误报为 Source 已输出 720p。
- HPD 断连使用固件初始化序列中的 `F014 bit 4`；在目标设备上，置位期间 HDMI 输入会消失，清除后恢复。旧实现使用的 `F015 bit 3` 只能回读，不能让 Source 重新枚举，已经弃用。

## 安全事务

每次切换按以下顺序执行：

1. 等待视频线程释放 V4L2 mmap 和设备句柄；
2. 读取并核对芯片 ID、HPD、EDID RAM 所有权和 DDC 控制寄存器；
3. 通过 `F014 bit 4` 断开 HDMI 输入并等待；
4. 禁用 DDC，将 EDID RAM 所有权切给 8051；
5. 备份原有 256 字节 RAM；
6. 写入内置 EDID，完整回读并比较；
7. 恢复原来的 RAM 所有权和 DDC 寄存器；
8. 恢复 `F014` 原值，等待 Source 重新建立信号；
9. 轮询 `F660–F663`，确认 HDMI 输入宽高与所选模式一致；
10. 只有输入时序确认后才保存配置并恢复相应 V4L2 采集。

步骤 3 之后的任何错误都会尽力写回备份并恢复原始控制寄存器。若 EDID 已写入但输入时序仍未改变，也会执行第二次断连周期恢复旧 EDID，并把切换判定为失败。硬件事务成功后才原子保存配置；若保存失败，同样会恢复切换前的 RAM。切换操作由全局配置锁串行化，不会与另一项网页配置并发写设备。

WingmanKVM 启动时会重新应用已保存的受管模式，以覆盖整机掉电造成的 RAM 丢失。成功应用后还会记录 USB 设备代次；采集卡拔插或 USB 复位重新枚举后，会暂停视频并重新应用 EDID。设备缺席期间不会反复暂停视频。

## 默认行为与权限

默认值是：

```json
{
  "display": {
    "virtual_monitor": "unmanaged",
    "control_device": null
  },
  "video": {
    "follow_display": false
  }
}
```

因此升级既有安装不会触发 HPD，也不会修改 EDID。`unmanaged` 表示不再写入采集卡；它不会猜测或恢复未知的出厂 EDID，采集卡下次掉电后会自行恢复固件默认值。

安装器只为 `345f:2130` 的 `hidraw` 节点授予 `wingmankvm-hw` 组读写权限。自动匹配同时要求：

- HID 和所选 `/dev/videoN` 属于同一个 USB 父设备；
- VID:PID 精确匹配；
- HID report descriptor 与已验证设备完全一致；
- 只有一个合格控制接口。

任何歧义都会拒绝写入。手动 `control_device` 也必须通过同样的父设备和描述符校验。

## 首次生产验证

首次启用建议在维护窗口进行：

1. 保持 `Virtual Monitor = Unmanaged` 完成安装升级，确认视频链路仍正常；
2. 选择 720p60，接受 HPD 断连提示；
3. 确认页面显示“EDID 已回读，HDMI 输入已确认 1280×720，HPD 已恢复”；
4. 在被控机显示设置中确认桌面实际变为 1280×720；
5. 使用 Native Capture，确认状态中的实际 V4L2 格式为 1280×720、约 60 FPS；
6. 再切回 1080p60，验证窗口布局和视频恢复；
7. 最后验证一次 WingmanKVM 服务重启或整机掉电重启后的自动恢复。

若切换失败，保留页面错误信息并查看：

```bash
journalctl -u wingmankvm.service -n 100 --no-pager
ls -l /dev/hidraw* /dev/video*
```

不要使用第三方工具写 MS2130 Flash，也不要把 `/dev/hidrawN` 编号写死到不同采集卡之间复用。
