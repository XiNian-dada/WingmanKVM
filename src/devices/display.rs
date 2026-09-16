#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::{
    fs::{self, OpenOptions},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
};

use serde::Serialize;
use thiserror::Error;

use crate::config::{DisplayConfig, VirtualMonitorMode};

const EDID_BLOCK_LEN: usize = 128;
const EDID_RAM_LEN: usize = 256;
#[cfg(target_os = "linux")]
const MS2130_VENDOR_ID: u16 = 0x345f;
#[cfg(target_os = "linux")]
const MS2130_PRODUCT_ID: u16 = 0x2130;
const CHIP_ID_REGISTER: u16 = 0xf800;
const MS2130_CHIP_ID: u8 = 0x00;
const HPD_CONTROL_REGISTER: u16 = 0xf015;
const HPD_DISCONNECTED: u8 = 0x08;
const EDID_OWNER_REGISTER: u16 = 0xf062;
const EDID_OWNER_8051: u8 = 0x80;
const DDC_CONTROL_REGISTER: u16 = 0xf063;
const DDC_ENABLED: u8 = 0x08;
const EDID_RAM_START: u16 = 0xf900;
const HPD_LOW_DELAY: Duration = Duration::from_millis(300);
const HPD_HIGH_DELAY: Duration = Duration::from_millis(800);
#[cfg(target_os = "linux")]
const MS2130_REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0x00, 0xff, 0x09, 0x01, 0xa1, 0x01, 0x15, 0x00, 0x26, 0xff, 0x00, 0x19, 0x01, 0x29, 0x02,
    0x75, 0x08, 0x95, 0x08, 0xb1, 0x02, 0xc0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayState {
    Unmanaged,
    Applying,
    Applied,
    Error,
    #[cfg(not(target_os = "linux"))]
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayStatus {
    pub state: DisplayState,
    pub requested_mode: VirtualMonitorMode,
    pub applied_mode: Option<VirtualMonitorMode>,
    pub control_device: Option<PathBuf>,
    pub message: Option<String>,
}

impl Default for DisplayStatus {
    fn default() -> Self {
        Self {
            state: DisplayState::Unmanaged,
            requested_mode: VirtualMonitorMode::Unmanaged,
            applied_mode: None,
            control_device: None,
            message: Some("未接管采集卡 EDID".to_owned()),
        }
    }
}

#[derive(Debug, Error)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum DisplayError {
    #[cfg(not(target_os = "linux"))]
    #[error("虚拟显示器控制仅支持 Linux")]
    Unsupported,
    #[error("需要先选择视频采集设备，才能匹配同一块 MS2130 的控制接口")]
    VideoDeviceRequired,
    #[error("找不到与视频设备属于同一块 MS2130 的厂商 HID 控制接口")]
    ControlDeviceNotFound,
    #[error("检测到多个匹配的 MS2130 控制接口，拒绝猜测目标设备")]
    AmbiguousControlDevice,
    #[error("控制设备 {path} 不属于所选视频采集卡")]
    DeviceMismatch { path: PathBuf },
    #[error("控制设备 {path} 的 HID 报告描述符与已验证的 MS2130 不一致")]
    DescriptorMismatch { path: PathBuf },
    #[error("访问 {path} 失败: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("MS2130 私有 HID 协议失败: {0}")]
    Protocol(String),
}

#[derive(Clone)]
pub struct DisplayManager {
    status: Arc<Mutex<DisplayStatus>>,
    applied_generation: Arc<Mutex<Option<DeviceGeneration>>>,
    operation: Arc<Mutex<()>>,
}

impl Default for DisplayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(DisplayStatus::default())),
            applied_generation: Arc::new(Mutex::new(None)),
            operation: Arc::new(Mutex::new(())),
        }
    }

    pub fn status(&self) -> DisplayStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_unmanaged(&self) {
        *self
            .applied_generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.replace_status(DisplayStatus::default());
    }

    /// Detect a USB re-enumeration after an EDID was successfully applied.
    /// Missing devices are not retried until they reappear with a new USB
    /// generation, avoiding repeated capture interruptions while unplugged.
    pub fn needs_reapply(&self, config: &DisplayConfig, video_device: Option<&Path>) -> bool {
        if config.virtual_monitor == VirtualMonitorMode::Unmanaged {
            return false;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = video_device;
            false
        }
        #[cfg(target_os = "linux")]
        {
            let Some(applied) = self
                .applied_generation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
            else {
                return false;
            };
            let Ok(control_device) = resolve_control_device(config, video_device) else {
                return false;
            };
            let Ok(current) = control_device_generation(&control_device) else {
                return false;
            };
            current != applied
        }
    }

    pub fn mark_unapplied(&self, message: impl Into<String>) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        status.state = DisplayState::Error;
        status.applied_mode = None;
        status.message = Some(message.into());
    }

    pub fn apply(
        &self,
        config: &DisplayConfig,
        video_device: Option<&Path>,
    ) -> Result<Option<DisplayRollback>, DisplayError> {
        if config.virtual_monitor == VirtualMonitorMode::Unmanaged {
            self.set_unmanaged();
            return Ok(None);
        }

        let _guard = self
            .operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_applied_mode = self.status().applied_mode;
        self.replace_status(DisplayStatus {
            state: DisplayState::Applying,
            requested_mode: config.virtual_monitor,
            applied_mode: previous_applied_mode,
            control_device: config.control_device.clone(),
            message: Some("正在安全切换 EDID 与 HDMI HPD".to_owned()),
        });

        let result = self.apply_inner(config, video_device);
        match &result {
            Ok(Some(rollback)) => {
                *self
                    .applied_generation
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(rollback.generation.clone());
                self.replace_status(DisplayStatus {
                    state: DisplayState::Applied,
                    requested_mode: config.virtual_monitor,
                    applied_mode: Some(config.virtual_monitor),
                    control_device: Some(rollback.control_device.clone()),
                    message: Some("EDID RAM 已回读校验，HPD 已恢复".to_owned()),
                });
            }
            Ok(None) => self.set_unmanaged(),
            Err(error) => self.replace_status(DisplayStatus {
                state: display_error_state(error),
                requested_mode: config.virtual_monitor,
                applied_mode: previous_applied_mode,
                control_device: config.control_device.clone(),
                message: Some(error.to_string()),
            }),
        }
        result
    }

    pub fn rollback(
        &self,
        rollback: DisplayRollback,
        previous_mode: VirtualMonitorMode,
    ) -> Result<(), DisplayError> {
        let _guard = self
            .operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut transport = HidrawTransport::open(&rollback.control_device)?;
        apply_edid_transaction(&mut transport, &rollback.previous_edid)?;
        *self
            .applied_generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rollback.generation.clone());
        self.replace_status(DisplayStatus {
            state: if previous_mode == VirtualMonitorMode::Unmanaged {
                DisplayState::Unmanaged
            } else {
                DisplayState::Applied
            },
            requested_mode: previous_mode,
            applied_mode: (previous_mode != VirtualMonitorMode::Unmanaged).then_some(previous_mode),
            control_device: Some(rollback.control_device),
            message: Some("配置保存失败，已恢复切换前的 EDID RAM".to_owned()),
        });
        Ok(())
    }

    fn apply_inner(
        &self,
        config: &DisplayConfig,
        video_device: Option<&Path>,
    ) -> Result<Option<DisplayRollback>, DisplayError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (config, video_device);
            Err(DisplayError::Unsupported)
        }
        #[cfg(target_os = "linux")]
        {
            let control_device = resolve_control_device(config, video_device)?;
            let generation = control_device_generation(&control_device)?;
            let image = EdidImage::for_mode(config.virtual_monitor)
                .ok_or_else(|| DisplayError::Protocol("缺少 EDID 配置".to_owned()))?;
            image.validate().map_err(|error| {
                DisplayError::Protocol(format!("内置 EDID 未通过校验: {error}"))
            })?;
            let mut transport = HidrawTransport::open(&control_device)?;
            let previous_edid = apply_edid_transaction(&mut transport, image.as_bytes())?;
            Ok(Some(DisplayRollback {
                control_device,
                previous_edid,
                generation,
            }))
        }
    }

    fn replace_status(&self, status: DisplayStatus) {
        *self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
    }
}

fn display_error_state(error: &DisplayError) -> DisplayState {
    #[cfg(not(target_os = "linux"))]
    if matches!(error, DisplayError::Unsupported) {
        return DisplayState::Unsupported;
    }
    let _ = error;
    DisplayState::Error
}

#[derive(Debug)]
pub struct DisplayRollback {
    control_device: PathBuf,
    previous_edid: [u8; EDID_RAM_LEN],
    generation: DeviceGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceGeneration {
    usb_path: PathBuf,
    device_number: String,
}

trait RegisterIo {
    fn read_register(&mut self, address: u16) -> Result<u8, DisplayError>;
    fn write_register(&mut self, address: u16, value: u8) -> Result<(), DisplayError>;
    fn wait(&mut self, duration: Duration);
}

fn apply_edid_transaction(
    transport: &mut impl RegisterIo,
    new_edid: &[u8; EDID_RAM_LEN],
) -> Result<[u8; EDID_RAM_LEN], DisplayError> {
    let chip_id = transport.read_register(CHIP_ID_REGISTER)?;
    if chip_id != MS2130_CHIP_ID {
        return Err(DisplayError::Protocol(format!(
            "芯片 ID 0x{chip_id:02x} 不是已验证的 MS2130"
        )));
    }

    let original = RegisterSnapshot {
        hpd: transport.read_register(HPD_CONTROL_REGISTER)?,
        owner: transport.read_register(EDID_OWNER_REGISTER)?,
        ddc: transport.read_register(DDC_CONTROL_REGISTER)?,
    };
    let mut previous_edid = None;
    let operation = (|| {
        write_checked(
            transport,
            HPD_CONTROL_REGISTER,
            original.hpd | HPD_DISCONNECTED,
        )?;
        transport.wait(HPD_LOW_DELAY);
        write_checked(transport, DDC_CONTROL_REGISTER, original.ddc & !DDC_ENABLED)?;
        write_checked(
            transport,
            EDID_OWNER_REGISTER,
            original.owner | EDID_OWNER_8051,
        )?;

        let backup = read_edid(transport)?;
        previous_edid = Some(backup);
        write_edid(transport, new_edid)?;
        let verification = read_edid(transport)?;
        if verification != *new_edid {
            return Err(DisplayError::Protocol(
                "EDID RAM 回读内容与写入内容不一致".to_owned(),
            ));
        }
        Ok(())
    })();

    if let Err(error) = operation {
        let mut rollback_errors = Vec::new();
        if let Some(backup) = previous_edid
            && let Err(rollback_error) = write_edid(transport, &backup)
        {
            rollback_errors.push(format!("恢复 EDID 失败: {rollback_error}"));
        }
        restore_registers(transport, original, &mut rollback_errors);
        return if rollback_errors.is_empty() {
            Err(error)
        } else {
            Err(DisplayError::Protocol(format!(
                "{error}; 回滚时另有错误: {}",
                rollback_errors.join("; ")
            )))
        };
    }

    let mut restore_errors = Vec::new();
    restore_ddc(transport, original, &mut restore_errors);
    transport.wait(HPD_LOW_DELAY);
    if let Err(error) = write_checked(
        transport,
        HPD_CONTROL_REGISTER,
        original.hpd & !HPD_DISCONNECTED,
    ) {
        restore_errors.push(format!("恢复 HPD 失败: {error}"));
    }
    if !restore_errors.is_empty() {
        // The new EDID may already be visible, but the control state is not
        // known-good. Try a full best-effort rollback before reporting failure.
        if let Some(backup) = previous_edid {
            let _ = rollback_after_commit_failure(transport, &backup, original);
        }
        return Err(DisplayError::Protocol(restore_errors.join("; ")));
    }
    transport.wait(HPD_HIGH_DELAY);
    previous_edid.ok_or_else(|| DisplayError::Protocol("未取得原始 EDID 备份".to_owned()))
}

#[derive(Clone, Copy)]
struct RegisterSnapshot {
    hpd: u8,
    owner: u8,
    ddc: u8,
}

fn write_checked(
    transport: &mut impl RegisterIo,
    address: u16,
    value: u8,
) -> Result<(), DisplayError> {
    transport.write_register(address, value)?;
    let actual = transport.read_register(address)?;
    if actual == value {
        Ok(())
    } else {
        Err(DisplayError::Protocol(format!(
            "寄存器 0x{address:04x} 写入 0x{value:02x} 后回读为 0x{actual:02x}"
        )))
    }
}

fn read_edid(transport: &mut impl RegisterIo) -> Result<[u8; EDID_RAM_LEN], DisplayError> {
    let mut edid = [0_u8; EDID_RAM_LEN];
    for (offset, byte) in edid.iter_mut().enumerate() {
        *byte = transport.read_register(EDID_RAM_START + offset as u16)?;
    }
    Ok(edid)
}

fn write_edid(
    transport: &mut impl RegisterIo,
    edid: &[u8; EDID_RAM_LEN],
) -> Result<(), DisplayError> {
    for (offset, byte) in edid.iter().enumerate() {
        transport.write_register(EDID_RAM_START + offset as u16, *byte)?;
    }
    Ok(())
}

fn restore_ddc(
    transport: &mut impl RegisterIo,
    original: RegisterSnapshot,
    errors: &mut Vec<String>,
) {
    if let Err(error) = write_checked(transport, EDID_OWNER_REGISTER, original.owner) {
        errors.push(format!("恢复 EDID RAM 所有权失败: {error}"));
    }
    if let Err(error) = write_checked(transport, DDC_CONTROL_REGISTER, original.ddc) {
        errors.push(format!("恢复 DDC 失败: {error}"));
    }
}

fn restore_registers(
    transport: &mut impl RegisterIo,
    original: RegisterSnapshot,
    errors: &mut Vec<String>,
) {
    restore_ddc(transport, original, errors);
    transport.wait(HPD_LOW_DELAY);
    if let Err(error) = write_checked(transport, HPD_CONTROL_REGISTER, original.hpd) {
        errors.push(format!("恢复 HPD 原始状态失败: {error}"));
    }
}

fn rollback_after_commit_failure(
    transport: &mut impl RegisterIo,
    backup: &[u8; EDID_RAM_LEN],
    original: RegisterSnapshot,
) -> Result<(), DisplayError> {
    write_checked(
        transport,
        HPD_CONTROL_REGISTER,
        original.hpd | HPD_DISCONNECTED,
    )?;
    transport.wait(HPD_LOW_DELAY);
    write_checked(transport, DDC_CONTROL_REGISTER, original.ddc & !DDC_ENABLED)?;
    write_checked(
        transport,
        EDID_OWNER_REGISTER,
        original.owner | EDID_OWNER_8051,
    )?;
    write_edid(transport, backup)?;
    let mut errors = Vec::new();
    restore_registers(transport, original, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DisplayError::Protocol(errors.join("; ")))
    }
}

struct HidrawTransport {
    #[cfg(target_os = "linux")]
    file: fs::File,
}

impl HidrawTransport {
    #[cfg(target_os = "linux")]
    fn open(path: &Path) -> Result<Self, DisplayError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).custom_flags(libc::O_CLOEXEC);
        let file = options.open(path).map_err(|source| DisplayError::Io {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self { file })
    }

    #[cfg(not(target_os = "linux"))]
    fn open(_path: &Path) -> Result<Self, DisplayError> {
        Err(DisplayError::Unsupported)
    }
}

#[cfg(target_os = "linux")]
impl RegisterIo for HidrawTransport {
    fn read_register(&mut self, address: u16) -> Result<u8, DisplayError> {
        let [high, low] = address.to_be_bytes();
        let mut report = [0x00, 0xb5, high, low, 0x00, 0x00, 0x00, 0x00, 0x00];
        hidraw_ioctl(
            self.file.as_raw_fd(),
            hid_set_feature(report.len()),
            &mut report,
        )?;
        report.fill(0);
        hidraw_ioctl(
            self.file.as_raw_fd(),
            hid_get_feature(report.len()),
            &mut report,
        )?;
        Ok(report[4])
    }

    fn write_register(&mut self, address: u16, value: u8) -> Result<(), DisplayError> {
        let [high, low] = address.to_be_bytes();
        let mut report = [0x00, 0xb6, high, low, value, 0x00, 0x00, 0x00, 0x00];
        hidraw_ioctl(
            self.file.as_raw_fd(),
            hid_set_feature(report.len()),
            &mut report,
        )
    }

    fn wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(not(target_os = "linux"))]
impl RegisterIo for HidrawTransport {
    fn read_register(&mut self, _address: u16) -> Result<u8, DisplayError> {
        Err(DisplayError::Unsupported)
    }

    fn write_register(&mut self, _address: u16, _value: u8) -> Result<(), DisplayError> {
        Err(DisplayError::Unsupported)
    }

    fn wait(&mut self, _duration: Duration) {}
}

#[cfg(target_os = "linux")]
fn hid_set_feature(length: usize) -> libc::c_ulong {
    0xc000_4806_u64.wrapping_add((length as u64) << 16) as libc::c_ulong
}

#[cfg(target_os = "linux")]
fn hid_get_feature(length: usize) -> libc::c_ulong {
    0xc000_4807_u64.wrapping_add((length as u64) << 16) as libc::c_ulong
}

#[cfg(target_os = "linux")]
fn hidraw_ioctl(
    fd: libc::c_int,
    request: libc::c_ulong,
    report: &mut [u8],
) -> Result<(), DisplayError> {
    // SAFETY: `report` is a valid writable buffer for the request's encoded
    // length and remains alive for the duration of the ioctl call.
    let result = unsafe { libc::ioctl(fd, request, report.as_mut_ptr()) };
    if result < 0 {
        Err(DisplayError::Protocol(
            io::Error::last_os_error().to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn resolve_control_device(
    config: &DisplayConfig,
    video_device: Option<&Path>,
) -> Result<PathBuf, DisplayError> {
    let video_device = video_device.ok_or(DisplayError::VideoDeviceRequired)?;
    let video_usb = usb_parent_for_node(video_device, "video4linux")?;
    ensure_ms2130_usb_device(&video_usb, video_device)?;

    if let Some(explicit) = &config.control_device {
        let control_usb = usb_parent_for_node(explicit, "hidraw")?;
        if control_usb != video_usb {
            return Err(DisplayError::DeviceMismatch {
                path: explicit.clone(),
            });
        }
        validate_hid_descriptor(explicit)?;
        return Ok(explicit.clone());
    }

    let entries = fs::read_dir("/sys/class/hidraw").map_err(|source| DisplayError::Io {
        path: PathBuf::from("/sys/class/hidraw"),
        source,
    })?;
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let path = PathBuf::from("/dev").join(&name);
        if usb_parent_for_class_entry(&entry.path()).ok().as_ref() != Some(&video_usb) {
            continue;
        }
        if validate_hid_descriptor(&path).is_ok() {
            matches.push(path);
        }
    }
    match matches.len() {
        0 => Err(DisplayError::ControlDeviceNotFound),
        1 => Ok(matches.remove(0)),
        _ => Err(DisplayError::AmbiguousControlDevice),
    }
}

#[cfg(target_os = "linux")]
fn usb_parent_for_node(path: &Path, class: &str) -> Result<PathBuf, DisplayError> {
    let resolved = fs::canonicalize(path).map_err(|source| DisplayError::Io {
        path: path.to_owned(),
        source,
    })?;
    let name = resolved
        .file_name()
        .ok_or_else(|| DisplayError::DeviceMismatch {
            path: path.to_owned(),
        })?;
    usb_parent_for_class_entry(&Path::new("/sys/class").join(class).join(name))
}

#[cfg(target_os = "linux")]
fn usb_parent_for_class_entry(class_entry: &Path) -> Result<PathBuf, DisplayError> {
    let device_link = class_entry.join("device");
    let device = fs::canonicalize(&device_link).map_err(|source| DisplayError::Io {
        path: device_link,
        source,
    })?;
    device
        .ancestors()
        .find(|ancestor| {
            ancestor.join("idVendor").is_file() && ancestor.join("idProduct").is_file()
        })
        .map(Path::to_owned)
        .ok_or_else(|| DisplayError::DeviceMismatch {
            path: class_entry.to_owned(),
        })
}

#[cfg(target_os = "linux")]
fn ensure_ms2130_usb_device(usb: &Path, reported_path: &Path) -> Result<(), DisplayError> {
    let vendor = read_hex_u16(&usb.join("idVendor"))?;
    let product = read_hex_u16(&usb.join("idProduct"))?;
    if (vendor, product) == (MS2130_VENDOR_ID, MS2130_PRODUCT_ID) {
        Ok(())
    } else {
        Err(DisplayError::DeviceMismatch {
            path: reported_path.to_owned(),
        })
    }
}

#[cfg(target_os = "linux")]
fn read_hex_u16(path: &Path) -> Result<u16, DisplayError> {
    let text = fs::read_to_string(path).map_err(|source| DisplayError::Io {
        path: path.to_owned(),
        source,
    })?;
    u16::from_str_radix(text.trim(), 16).map_err(|error| {
        DisplayError::Protocol(format!(
            "{} 不是有效的十六进制设备 ID: {error}",
            path.display()
        ))
    })
}

#[cfg(target_os = "linux")]
fn validate_hid_descriptor(path: &Path) -> Result<(), DisplayError> {
    let resolved = fs::canonicalize(path).map_err(|source| DisplayError::Io {
        path: path.to_owned(),
        source,
    })?;
    let name = resolved
        .file_name()
        .ok_or_else(|| DisplayError::DescriptorMismatch {
            path: path.to_owned(),
        })?;
    let descriptor_path = Path::new("/sys/class/hidraw")
        .join(name)
        .join("device/report_descriptor");
    let descriptor = fs::read(&descriptor_path).map_err(|source| DisplayError::Io {
        path: descriptor_path,
        source,
    })?;
    if descriptor == MS2130_REPORT_DESCRIPTOR {
        Ok(())
    } else {
        Err(DisplayError::DescriptorMismatch {
            path: path.to_owned(),
        })
    }
}

#[cfg(target_os = "linux")]
fn control_device_generation(path: &Path) -> Result<DeviceGeneration, DisplayError> {
    let usb_path = usb_parent_for_node(path, "hidraw")?;
    let devnum_path = usb_path.join("devnum");
    let device_number = fs::read_to_string(&devnum_path).map_err(|source| DisplayError::Io {
        path: devnum_path,
        source,
    })?;
    Ok(DeviceGeneration {
        usb_path,
        device_number: device_number.trim().to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdidImage([u8; EDID_RAM_LEN]);

impl EdidImage {
    pub fn for_mode(mode: VirtualMonitorMode) -> Option<Self> {
        let timing = match mode {
            VirtualMonitorMode::Unmanaged => return None,
            VirtualMonitorMode::Hd1080p60 => EdidTiming {
                width: 1920,
                height: 1080,
                pixel_clock_10khz: 14_850,
                horizontal_blank: 280,
                vertical_blank: 45,
                horizontal_sync_offset: 88,
                horizontal_sync_width: 44,
                vertical_sync_offset: 4,
                vertical_sync_width: 5,
                cea_vic: 16,
                product_code: 0x1080,
            },
            VirtualMonitorMode::Hd720p60 => EdidTiming {
                width: 1280,
                height: 720,
                pixel_clock_10khz: 7_425,
                horizontal_blank: 370,
                vertical_blank: 30,
                horizontal_sync_offset: 110,
                horizontal_sync_width: 40,
                vertical_sync_offset: 5,
                vertical_sync_width: 5,
                cea_vic: 4,
                product_code: 0x0720,
            },
        };
        Some(Self(build_edid(timing)))
    }

    pub fn as_bytes(&self) -> &[u8; EDID_RAM_LEN] {
        &self.0
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.0[..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
            return Err("invalid EDID header");
        }
        if self.0[126] != 1 {
            return Err("EDID must contain exactly one CTA extension");
        }
        for block in self.0.chunks_exact(EDID_BLOCK_LEN) {
            if block.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
                return Err("invalid EDID checksum");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct EdidTiming {
    width: u16,
    height: u16,
    pixel_clock_10khz: u16,
    horizontal_blank: u16,
    vertical_blank: u16,
    horizontal_sync_offset: u16,
    horizontal_sync_width: u16,
    vertical_sync_offset: u8,
    vertical_sync_width: u8,
    cea_vic: u8,
    product_code: u16,
}

fn build_edid(timing: EdidTiming) -> [u8; EDID_RAM_LEN] {
    let mut edid = [0_u8; EDID_RAM_LEN];
    let base = &mut edid[..EDID_BLOCK_LEN];
    base[..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    base[8..10].copy_from_slice(&manufacturer_id(*b"WKM").to_be_bytes());
    base[10..12].copy_from_slice(&timing.product_code.to_le_bytes());
    base[12..16].copy_from_slice(&(u32::from(timing.product_code) | 0x574b_0000).to_le_bytes());
    base[16] = 1;
    base[17] = 36; // 1990 + 36 = 2026
    base[18] = 1;
    base[19] = 4;
    base[20] = 0x80; // digital input
    base[21] = 51;
    base[22] = 29;
    base[23] = 0x78; // gamma 2.2
    base[24] = 0x06; // sRGB + preferred timing
    base[25..35].copy_from_slice(&[0xee, 0x91, 0xa3, 0x54, 0x4c, 0x99, 0x26, 0x0f, 0x50, 0x54]);
    for standard_timing in base[38..54].chunks_exact_mut(2) {
        standard_timing.copy_from_slice(&[0x01, 0x01]);
    }
    write_detailed_timing(&mut base[54..72], timing);
    write_text_descriptor(&mut base[72..90], 0xfc, b"WingmanKVM");
    let serial = if timing.width == 1920 {
        b"WMK-1080P60".as_slice()
    } else {
        b"WMK-720P60".as_slice()
    };
    write_text_descriptor(&mut base[90..108], 0xff, serial);
    // An unused descriptor avoids advertising range-derived or fallback modes.
    base[108..126].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ]);
    base[126] = 1;
    set_checksum(base);

    let extension = &mut edid[EDID_BLOCK_LEN..];
    extension[0] = 0x02; // CTA-861 extension
    extension[1] = 0x03;
    extension[2] = 14; // start of detailed timings (none are present)
    extension[3] = 0x40; // basic audio
    extension[4] = 0x41; // video data block, one VIC
    extension[5] = timing.cea_vic | 0x80; // the sole VIC is native
    extension[6..10].copy_from_slice(&[0x23, 0x09, 0x07, 0x07]); // 2ch LPCM
    extension[10..14].copy_from_slice(&[0x83, 0x01, 0x00, 0x00]); // front L/R
    set_checksum(extension);
    edid
}

fn manufacturer_id(name: [u8; 3]) -> u16 {
    let letter = |byte: u8| u16::from(byte.saturating_sub(b'A').saturating_add(1) & 0x1f);
    (letter(name[0]) << 10) | (letter(name[1]) << 5) | letter(name[2])
}

fn write_detailed_timing(bytes: &mut [u8], timing: EdidTiming) {
    bytes[..2].copy_from_slice(&timing.pixel_clock_10khz.to_le_bytes());
    bytes[2] = timing.width as u8;
    bytes[3] = timing.horizontal_blank as u8;
    bytes[4] = (((timing.width >> 8) as u8) << 4) | ((timing.horizontal_blank >> 8) as u8);
    bytes[5] = timing.height as u8;
    bytes[6] = timing.vertical_blank as u8;
    bytes[7] = (((timing.height >> 8) as u8) << 4) | ((timing.vertical_blank >> 8) as u8);
    bytes[8] = timing.horizontal_sync_offset as u8;
    bytes[9] = timing.horizontal_sync_width as u8;
    bytes[10] = (timing.vertical_sync_offset << 4) | timing.vertical_sync_width;
    bytes[11] = (((timing.horizontal_sync_offset >> 8) as u8) << 6)
        | (((timing.horizontal_sync_width >> 8) as u8) << 4)
        | ((timing.vertical_sync_offset >> 4) << 2)
        | (timing.vertical_sync_width >> 4);
    bytes[12] = 0xfd; // 509 mm x 286 mm
    bytes[13] = 0x1e;
    bytes[14] = 0x21;
    bytes[17] = 0x1e; // digital separate sync, positive H/V
}

fn write_text_descriptor(bytes: &mut [u8], tag: u8, text: &[u8]) {
    bytes[..5].copy_from_slice(&[0x00, 0x00, 0x00, tag, 0x00]);
    bytes[5..].fill(b' ');
    let length = text.len().min(12);
    bytes[5..5 + length].copy_from_slice(&text[..length]);
    bytes[5 + length] = b'\n';
}

fn set_checksum(block: &mut [u8]) {
    let checksum_index = block.len() - 1;
    block[checksum_index] = 0;
    let sum = block.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    block[checksum_index] = 0_u8.wrapping_sub(sum);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct MockTransport {
        registers: BTreeMap<u16, u8>,
        writes: Vec<(u16, u8)>,
        waits: Vec<Duration>,
        fail_write_once: Option<u16>,
    }

    impl MockTransport {
        fn new(previous_edid: [u8; EDID_RAM_LEN]) -> Self {
            let mut registers = BTreeMap::from([
                (CHIP_ID_REGISTER, MS2130_CHIP_ID),
                (HPD_CONTROL_REGISTER, 0x16),
                (EDID_OWNER_REGISTER, 0x00),
                (DDC_CONTROL_REGISTER, 0x1e),
            ]);
            for (offset, byte) in previous_edid.into_iter().enumerate() {
                registers.insert(EDID_RAM_START + offset as u16, byte);
            }
            Self {
                registers,
                writes: Vec::new(),
                waits: Vec::new(),
                fail_write_once: None,
            }
        }

        fn edid(&self) -> [u8; EDID_RAM_LEN] {
            let mut result = [0_u8; EDID_RAM_LEN];
            for (offset, byte) in result.iter_mut().enumerate() {
                *byte = self.registers[&(EDID_RAM_START + offset as u16)];
            }
            result
        }
    }

    impl RegisterIo for MockTransport {
        fn read_register(&mut self, address: u16) -> Result<u8, DisplayError> {
            self.registers
                .get(&address)
                .copied()
                .ok_or_else(|| DisplayError::Protocol(format!("missing register {address:04x}")))
        }

        fn write_register(&mut self, address: u16, value: u8) -> Result<(), DisplayError> {
            self.writes.push((address, value));
            if self.fail_write_once == Some(address) {
                self.fail_write_once = None;
                return Err(DisplayError::Protocol("injected write failure".to_owned()));
            }
            self.registers.insert(address, value);
            Ok(())
        }

        fn wait(&mut self, duration: Duration) {
            self.waits.push(duration);
        }
    }

    fn decoded_preferred_size(edid: &EdidImage) -> (u16, u16) {
        let dtd = &edid.as_bytes()[54..72];
        let width = u16::from(dtd[2]) | (u16::from(dtd[4] >> 4) << 8);
        let height = u16::from(dtd[5]) | (u16::from(dtd[7] >> 4) << 8);
        (width, height)
    }

    #[test]
    fn strict_profiles_are_valid_and_advertise_only_the_selected_vic() {
        for (mode, size, vic) in [
            (VirtualMonitorMode::Hd1080p60, (1920, 1080), 16),
            (VirtualMonitorMode::Hd720p60, (1280, 720), 4),
        ] {
            let edid = EdidImage::for_mode(mode).unwrap();
            assert_eq!(edid.validate(), Ok(()));
            assert_eq!(decoded_preferred_size(&edid), size);
            assert_eq!(edid.as_bytes()[128 + 4], 0x41);
            assert_eq!(edid.as_bytes()[128 + 5], vic | 0x80);
        }
    }

    #[test]
    fn unmanaged_mode_has_no_edid_image() {
        assert!(EdidImage::for_mode(VirtualMonitorMode::Unmanaged).is_none());
    }

    #[test]
    fn transaction_backs_up_ram_and_restores_control_registers() {
        let previous = [0xa5; EDID_RAM_LEN];
        let expected = EdidImage::for_mode(VirtualMonitorMode::Hd720p60).unwrap();
        let mut transport = MockTransport::new(previous);

        let backup = apply_edid_transaction(&mut transport, expected.as_bytes()).unwrap();

        assert_eq!(backup, previous);
        assert_eq!(transport.edid(), *expected.as_bytes());
        assert_eq!(transport.registers[&HPD_CONTROL_REGISTER], 0x16);
        assert_eq!(transport.registers[&EDID_OWNER_REGISTER], 0x00);
        assert_eq!(transport.registers[&DDC_CONTROL_REGISTER], 0x1e);
        assert_eq!(
            transport.waits,
            [HPD_LOW_DELAY, HPD_LOW_DELAY, HPD_HIGH_DELAY]
        );
        assert_eq!(
            &transport.writes[..3],
            &[
                (HPD_CONTROL_REGISTER, 0x1e),
                (DDC_CONTROL_REGISTER, 0x16),
                (EDID_OWNER_REGISTER, 0x80),
            ]
        );
    }

    #[test]
    fn transaction_restores_previous_edid_after_a_partial_write() {
        let previous = [0x5a; EDID_RAM_LEN];
        let expected = EdidImage::for_mode(VirtualMonitorMode::Hd1080p60).unwrap();
        let mut transport = MockTransport::new(previous);
        transport.fail_write_once = Some(EDID_RAM_START + 42);

        let error = apply_edid_transaction(&mut transport, expected.as_bytes()).unwrap_err();

        assert!(error.to_string().contains("injected write failure"));
        assert_eq!(transport.edid(), previous);
        assert_eq!(transport.registers[&HPD_CONTROL_REGISTER], 0x16);
        assert_eq!(transport.registers[&EDID_OWNER_REGISTER], 0x00);
        assert_eq!(transport.registers[&DDC_CONTROL_REGISTER], 0x1e);
    }

    #[test]
    fn unknown_chip_id_is_rejected_before_any_write() {
        let mut transport = MockTransport::new([0; EDID_RAM_LEN]);
        transport.registers.insert(CHIP_ID_REGISTER, 0x21);
        let expected = EdidImage::for_mode(VirtualMonitorMode::Hd720p60).unwrap();

        let error = apply_edid_transaction(&mut transport, expected.as_bytes()).unwrap_err();

        assert!(error.to_string().contains("不是已验证的 MS2130"));
        assert!(transport.writes.is_empty());
    }
}
