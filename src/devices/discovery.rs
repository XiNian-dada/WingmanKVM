use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDiscovery {
    pub video: Vec<DeviceCandidate>,
    pub hid: Vec<DeviceCandidate>,
    pub keyboard: Vec<DeviceCandidate>,
    pub mouse: Vec<DeviceCandidate>,
    pub absolute_pointer: Vec<DeviceCandidate>,
    pub gpio: Vec<DeviceCandidate>,
    pub mass_storage_luns: Vec<DeviceCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCandidate {
    pub path: PathBuf,
    pub label: String,
    pub kind: String,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gadget: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gadget_bound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_linked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_major: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_minor: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subclass: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_capture: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_mjpeg: Option<bool>,
}

impl DeviceCandidate {
    fn basic(path: PathBuf, label: String, kind: impl Into<String>) -> Self {
        Self {
            path,
            label,
            kind: kind.into(),
            warnings: Vec::new(),
            gadget: None,
            function: None,
            udc: None,
            gadget_bound: None,
            function_linked: None,
            compatible: None,
            device_major: None,
            device_minor: None,
            subclass: None,
            protocol: None,
            report_length: None,
            card: None,
            driver: None,
            video_capture: None,
            supports_mjpeg: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DeviceNumber {
    major: u64,
    minor: u64,
}

#[derive(Debug, Clone)]
struct HidDeviceNode {
    candidate: DeviceCandidate,
    device_number: Option<DeviceNumber>,
    stable_aliases: Vec<StableHidAlias>,
}

#[derive(Debug, Clone)]
struct StableHidAlias {
    role: HidRole,
    path: PathBuf,
}

#[derive(Default)]
struct HidRoles {
    keyboard: Vec<DeviceCandidate>,
    mouse: Vec<DeviceCandidate>,
    absolute_pointer: Vec<DeviceCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HidRole {
    Keyboard,
    Mouse,
    AbsolutePointer,
}

const STABLE_HID_LINKS: [(HidRole, &str); 3] = [
    (HidRole::Keyboard, "wingmankvm-keyboard"),
    (HidRole::Mouse, "wingmankvm-mouse"),
    (HidRole::AbsolutePointer, "wingmankvm-absolute"),
];

impl HidRole {
    fn kind(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Mouse => "mouse",
            Self::AbsolutePointer => "absolute_pointer",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "Boot Keyboard",
            Self::Mouse => "Boot Mouse",
            Self::AbsolutePointer => "Absolute Pointer",
        }
    }
}

pub async fn scan() -> DeviceDiscovery {
    let configfs_root = Path::new("/sys/kernel/config/usb_gadget");
    let (video, hid_nodes, gpio) = tokio::join!(
        scan_video(Path::new("/dev")),
        scan_hid_nodes(Path::new("/dev")),
        scan_prefix(Path::new("/dev"), "gpiochip", "gpio_chip"),
    );
    let hid = hid_nodes
        .iter()
        .map(|node| node.candidate.clone())
        .collect();
    let (hid_roles, mass_storage_luns) = tokio::join!(
        scan_hid_roles(configfs_root, hid_nodes),
        scan_luns(configfs_root),
    );

    DeviceDiscovery {
        video,
        hid,
        keyboard: hid_roles.keyboard,
        mouse: hid_roles.mouse,
        absolute_pointer: hid_roles.absolute_pointer,
        gpio,
        mass_storage_luns,
    }
}

async fn scan_video(root: &Path) -> Vec<DeviceCandidate> {
    let candidates = scan_prefix(root, "video", "video").await;
    #[cfg(target_os = "linux")]
    {
        return tokio::task::spawn_blocking(move || probe_video_candidates(candidates))
            .await
            .unwrap_or_default();
    }
    #[cfg(not(target_os = "linux"))]
    {
        candidates
    }
}

#[cfg(target_os = "linux")]
fn probe_video_candidates(mut candidates: Vec<DeviceCandidate>) -> Vec<DeviceCandidate> {
    use v4l::{Device, FourCC, capability::Flags, video::Capture};

    for candidate in &mut candidates {
        let device = match Device::with_path(&candidate.path) {
            Ok(device) => device,
            Err(error) => {
                candidate
                    .warnings
                    .push(format!("无法打开 V4L2 设备: {error}"));
                continue;
            }
        };
        let capabilities = match device.query_caps() {
            Ok(capabilities) => capabilities,
            Err(error) => {
                candidate
                    .warnings
                    .push(format!("无法读取 V4L2 能力: {error}"));
                continue;
            }
        };
        let video_capture = capabilities.capabilities.contains(Flags::VIDEO_CAPTURE);
        let supports_mjpeg = if video_capture {
            match device.enum_formats() {
                Ok(formats) => Some(
                    formats
                        .iter()
                        .any(|format| format.fourcc == FourCC::new(b"MJPG")),
                ),
                Err(error) => {
                    candidate
                        .warnings
                        .push(format!("无法读取 V4L2 格式: {error}"));
                    None
                }
            }
        } else {
            Some(false)
        };

        candidate.card = Some(capabilities.card.clone());
        candidate.driver = Some(capabilities.driver);
        candidate.video_capture = Some(video_capture);
        candidate.supports_mjpeg = supports_mjpeg;
        if !capabilities.card.is_empty() {
            candidate.label = format!("{} ({})", capabilities.card, candidate.label);
        }
        if !video_capture {
            candidate.warnings.push("设备不支持 V4L2 视频采集".into());
        } else if supports_mjpeg == Some(false) {
            candidate.warnings.push("设备不支持 MJPEG 直通".into());
        }
    }
    sort_video_candidates(&mut candidates);
    candidates
}

#[cfg(any(target_os = "linux", test))]
fn sort_video_candidates(candidates: &mut [DeviceCandidate]) {
    candidates.sort_by(|left, right| {
        video_rank(left)
            .cmp(&video_rank(right))
            .then_with(|| left.path.cmp(&right.path))
    });
}

#[cfg(any(target_os = "linux", test))]
fn video_rank(candidate: &DeviceCandidate) -> u8 {
    match (candidate.video_capture, candidate.supports_mjpeg) {
        (Some(true), Some(true)) => 0,
        (Some(true), _) => 1,
        (Some(false), _) => 2,
        (None, _) => 3,
    }
}

async fn scan_prefix(root: &Path, prefix: &str, kind: &str) -> Vec<DeviceCandidate> {
    let mut candidates = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return candidates;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) {
            candidates.push(DeviceCandidate::basic(entry.path(), name, kind));
        }
    }
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    candidates
}

async fn scan_hid_nodes(root: &Path) -> Vec<HidDeviceNode> {
    let mut nodes = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return nodes;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("hidg") {
            continue;
        }
        let device_number = entry
            .metadata()
            .await
            .ok()
            .as_ref()
            .and_then(device_number_from_metadata);
        let mut candidate = DeviceCandidate::basic(entry.path(), name, "hid_gadget");
        candidate
            .warnings
            .push("未能从 configfs 确认此 HID 节点的角色".into());
        nodes.push(HidDeviceNode {
            candidate,
            device_number,
            stable_aliases: Vec::new(),
        });
    }
    nodes.sort_by(|a, b| a.candidate.path.cmp(&b.candidate.path));
    attach_stable_hid_aliases(root, &mut nodes);
    nodes
}

#[cfg(unix)]
fn attach_stable_hid_aliases(root: &Path, nodes: &mut [HidDeviceNode]) {
    use std::os::unix::fs::FileTypeExt;

    for (role, name) in STABLE_HID_LINKS {
        let alias_path = root.join(name);
        let Ok(alias_link_metadata) = std::fs::symlink_metadata(&alias_path) else {
            continue;
        };
        if !alias_link_metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(alias_metadata) = std::fs::metadata(&alias_path) else {
            continue;
        };
        if !alias_metadata.file_type().is_char_device() {
            continue;
        }
        let Some(alias_device_number) = device_number_from_metadata(&alias_metadata) else {
            continue;
        };
        let Ok(alias_target) = std::fs::canonicalize(&alias_path) else {
            continue;
        };

        let Some(node) = nodes.iter_mut().find(|node| {
            if node.device_number != Some(alias_device_number)
                || !is_numbered_hid_node(&node.candidate.path)
            {
                return false;
            }
            let Ok(node_metadata) = std::fs::symlink_metadata(&node.candidate.path) else {
                return false;
            };
            node_metadata.file_type().is_char_device()
                && std::fs::canonicalize(&node.candidate.path).ok().as_ref() == Some(&alias_target)
        }) else {
            continue;
        };

        node.stable_aliases.push(StableHidAlias {
            role,
            path: alias_path,
        });
    }
}

#[cfg(not(unix))]
fn attach_stable_hid_aliases(_root: &Path, _nodes: &mut [HidDeviceNode]) {}

fn is_numbered_hid_node(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("hidg"))
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[cfg(unix)]
fn device_number_from_metadata(metadata: &std::fs::Metadata) -> Option<DeviceNumber> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    if !metadata.file_type().is_char_device() {
        return None;
    }
    let device = metadata.rdev() as libc::dev_t;
    Some(DeviceNumber {
        major: libc::major(device) as u64,
        minor: libc::minor(device) as u64,
    })
}

#[cfg(not(unix))]
fn device_number_from_metadata(_metadata: &std::fs::Metadata) -> Option<DeviceNumber> {
    None
}

async fn scan_hid_roles(root: &Path, nodes: Vec<HidDeviceNode>) -> HidRoles {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || classify_hid_functions(&root, &nodes))
        .await
        .unwrap_or_default()
}

fn classify_hid_functions(root: &Path, nodes: &[HidDeviceNode]) -> HidRoles {
    let mut roles = HidRoles::default();
    let mut seen = HashSet::new();

    for gadget_path in sorted_directories(root) {
        let gadget = file_name(&gadget_path);
        let udc = read_trimmed(&gadget_path.join("UDC")).filter(|value| !value.is_empty());
        let gadget_bound = udc.is_some();
        for function_path in sorted_directories(&gadget_path.join("functions")) {
            let function = file_name(&function_path);
            if !function.starts_with("hid.") {
                continue;
            }
            let Some(subclass) = read_number::<u8>(&function_path.join("subclass")) else {
                continue;
            };
            let Some(protocol) = read_number::<u8>(&function_path.join("protocol")) else {
                continue;
            };
            let Some(report_length) = read_number::<u16>(&function_path.join("report_length"))
            else {
                continue;
            };
            let Some(role) = hid_role(subclass, protocol, report_length) else {
                continue;
            };
            let Some(device_number) = read_trimmed(&function_path.join("dev"))
                .as_deref()
                .and_then(parse_device_number)
            else {
                continue;
            };
            let function_linked = function_is_linked(&gadget_path, &function_path);
            for node in nodes
                .iter()
                .filter(|node| node.device_number == Some(device_number))
            {
                let stable_path = node
                    .stable_aliases
                    .iter()
                    .find(|alias| alias.role == role)
                    .map(|alias| alias.path.clone());
                let candidate_path = stable_path.unwrap_or_else(|| node.candidate.path.clone());
                if !seen.insert((role, candidate_path.clone())) {
                    continue;
                }
                let mut candidate = node.candidate.clone();
                candidate.path = candidate_path;
                candidate.kind = role.kind().into();
                candidate.label = format!("{} ({})", role.label(), file_name(&candidate.path));
                candidate.warnings.clear();
                if !function_linked {
                    candidate
                        .warnings
                        .push("HID function 尚未链接到 USB config".into());
                }
                if !gadget_bound {
                    candidate.warnings.push("Gadget 尚未绑定 UDC".into());
                }
                candidate.gadget = Some(gadget.clone());
                candidate.function = Some(function.clone());
                candidate.udc = udc.clone();
                candidate.gadget_bound = Some(gadget_bound);
                candidate.function_linked = Some(function_linked);
                candidate.compatible = Some(true);
                candidate.device_major = Some(device_number.major);
                candidate.device_minor = Some(device_number.minor);
                candidate.subclass = Some(subclass);
                candidate.protocol = Some(protocol);
                candidate.report_length = Some(report_length);
                match role {
                    HidRole::Keyboard => roles.keyboard.push(candidate),
                    HidRole::Mouse => roles.mouse.push(candidate),
                    HidRole::AbsolutePointer => roles.absolute_pointer.push(candidate),
                }
            }
        }
    }

    sort_gadget_candidates(&mut roles.keyboard);
    sort_gadget_candidates(&mut roles.mouse);
    sort_gadget_candidates(&mut roles.absolute_pointer);
    roles
}

fn hid_role(subclass: u8, protocol: u8, report_length: u16) -> Option<HidRole> {
    match (subclass, protocol, report_length) {
        (1, 1, 8) => Some(HidRole::Keyboard),
        (1, 2, 4) => Some(HidRole::Mouse),
        (0, 0, 6) => Some(HidRole::AbsolutePointer),
        _ => None,
    }
}

fn parse_device_number(value: &str) -> Option<DeviceNumber> {
    let (major, minor) = value.trim().split_once(':')?;
    Some(DeviceNumber {
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
    })
}

async fn scan_luns(root: &Path) -> Vec<DeviceCandidate> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || scan_mass_storage_luns(&root))
        .await
        .unwrap_or_default()
}

fn scan_mass_storage_luns(root: &Path) -> Vec<DeviceCandidate> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    for gadget_path in sorted_directories(root) {
        let gadget = file_name(&gadget_path);
        let udc = read_trimmed(&gadget_path.join("UDC")).filter(|value| !value.is_empty());
        let gadget_bound = udc.is_some();
        for function_path in sorted_directories(&gadget_path.join("functions")) {
            let function = file_name(&function_path);
            if !function.starts_with("mass_storage.") {
                continue;
            }
            let function_linked = function_is_linked(&gadget_path, &function_path);
            for lun_path in sorted_directories(&function_path) {
                if !file_name(&lun_path).starts_with("lun.") {
                    continue;
                }
                let canonical = std::fs::canonicalize(&lun_path).unwrap_or(lun_path);
                if !seen.insert(canonical.clone()) {
                    continue;
                }
                let missing: Vec<_> = ["file", "ro", "cdrom", "removable"]
                    .into_iter()
                    .filter(|name| !canonical.join(name).is_file())
                    .collect();
                let compatible = missing.is_empty();
                let mut candidate = DeviceCandidate::basic(
                    canonical.clone(),
                    canonical.display().to_string(),
                    "mass_storage_lun",
                );
                if !compatible {
                    candidate
                        .warnings
                        .push(format!("LUN 缺少属性: {}", missing.join(", ")));
                }
                if !function_linked {
                    candidate
                        .warnings
                        .push("Mass Storage function 尚未链接到 USB config".into());
                }
                if !gadget_bound {
                    candidate.warnings.push("Gadget 尚未绑定 UDC".into());
                }
                candidate.gadget = Some(gadget.clone());
                candidate.function = Some(function.clone());
                candidate.udc = udc.clone();
                candidate.gadget_bound = Some(gadget_bound);
                candidate.function_linked = Some(function_linked);
                candidate.compatible = Some(compatible);
                results.push(candidate);
            }
        }
    }

    sort_gadget_candidates(&mut results);
    results
}

fn sorted_directories(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut directories: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();
    directories
}

fn function_is_linked(gadget_path: &Path, function_path: &Path) -> bool {
    let canonical_function =
        std::fs::canonicalize(function_path).unwrap_or_else(|_| function_path.to_path_buf());
    for config_path in sorted_directories(&gadget_path.join("configs")) {
        let Ok(entries) = std::fs::read_dir(config_path) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry
                .file_type()
                .is_ok_and(|file_type| file_type.is_symlink())
            {
                continue;
            }
            if std::fs::canonicalize(entry.path()).ok().as_ref() == Some(&canonical_function) {
                return true;
            }
        }
    }
    false
}

fn sort_gadget_candidates(candidates: &mut [DeviceCandidate]) {
    candidates.sort_by(|left, right| {
        gadget_candidate_rank(left)
            .cmp(&gadget_candidate_rank(right))
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn gadget_candidate_rank(candidate: &DeviceCandidate) -> u8 {
    u8::from(candidate.compatible != Some(true))
        + u8::from(candidate.function_linked != Some(true))
        + u8::from(candidate.gadget_bound != Some(true))
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn read_number<T>(path: &Path) -> Option<T>
where
    T: std::str::FromStr,
{
    read_trimmed(path)?.parse().ok()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct TestTree(PathBuf);

    impl TestTree {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "wingmankvm-discovery-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_hid_function(
        path: &Path,
        subclass: u8,
        protocol: u8,
        report_length: u16,
        device: DeviceNumber,
    ) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("subclass"), format!("{subclass}\n")).unwrap();
        fs::write(path.join("protocol"), format!("{protocol}\n")).unwrap();
        fs::write(path.join("report_length"), format!("{report_length}\n")).unwrap();
        fs::write(
            path.join("dev"),
            format!("{}:{}\n", device.major, device.minor),
        )
        .unwrap();
    }

    fn hid_node(path: &str, major: u64, minor: u64) -> HidDeviceNode {
        let mut candidate = DeviceCandidate::basic(
            PathBuf::from(path),
            file_name(Path::new(path)),
            "hid_gadget",
        );
        candidate.warnings.push("unclassified".into());
        HidDeviceNode {
            candidate,
            device_number: Some(DeviceNumber { major, minor }),
            stable_aliases: Vec::new(),
        }
    }

    fn add_stable_alias(node: &mut HidDeviceNode, role: HidRole, path: &str) {
        node.stable_aliases.push(StableHidAlias {
            role,
            path: PathBuf::from(path),
        });
    }

    #[cfg(unix)]
    #[test]
    fn hid_functions_map_device_numbers_to_explicit_roles() {
        use std::os::unix::fs::symlink;

        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let functions = gadget.join("functions");
        let config = gadget.join("configs/c.1");
        fs::create_dir_all(&config).unwrap();
        fs::write(gadget.join("UDC"), "fe800000.usb\n").unwrap();

        let keyboard = functions.join("hid.keyboard");
        let mouse = functions.join("hid.mouse");
        let absolute = functions.join("hid.absolute");
        write_hid_function(
            &keyboard,
            1,
            1,
            8,
            DeviceNumber {
                major: 240,
                minor: 0,
            },
        );
        write_hid_function(
            &mouse,
            1,
            2,
            4,
            DeviceNumber {
                major: 240,
                minor: 1,
            },
        );
        write_hid_function(
            &absolute,
            0,
            0,
            6,
            DeviceNumber {
                major: 240,
                minor: 2,
            },
        );
        symlink(&keyboard, config.join("hid.keyboard")).unwrap();
        symlink(&mouse, config.join("hid.mouse")).unwrap();
        symlink(&absolute, config.join("hid.absolute")).unwrap();

        let nodes = vec![
            hid_node("/dev/hidg2", 240, 2),
            hid_node("/dev/hidg0", 240, 0),
            hid_node("/dev/hidg1", 240, 1),
        ];
        let roles = classify_hid_functions(tree.path(), &nodes);

        assert_eq!(roles.keyboard[0].path, Path::new("/dev/hidg0"));
        assert_eq!(roles.mouse[0].path, Path::new("/dev/hidg1"));
        assert_eq!(roles.absolute_pointer[0].path, Path::new("/dev/hidg2"));
        assert_eq!(roles.keyboard[0].protocol, Some(1));
        assert_eq!(roles.mouse[0].report_length, Some(4));
        assert_eq!(roles.absolute_pointer[0].subclass, Some(0));
        assert_eq!(roles.keyboard[0].function_linked, Some(true));
        assert_eq!(roles.keyboard[0].gadget_bound, Some(true));
        assert!(roles.keyboard[0].warnings.is_empty());
    }

    #[test]
    fn unsupported_hid_tuple_is_not_assigned_a_role() {
        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let function = gadget.join("functions/hid.vendor");
        fs::create_dir_all(gadget.join("configs/c.1")).unwrap();
        fs::write(gadget.join("UDC"), "\n").unwrap();
        write_hid_function(
            &function,
            0,
            0,
            8,
            DeviceNumber {
                major: 240,
                minor: 0,
            },
        );

        let roles = classify_hid_functions(tree.path(), &[hid_node("/dev/hidg0", 240, 0)]);

        assert!(roles.keyboard.is_empty());
        assert!(roles.mouse.is_empty());
        assert!(roles.absolute_pointer.is_empty());
    }

    #[test]
    fn matching_stable_alias_replaces_numbered_hid_path() {
        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let function = gadget.join("functions/hid.keyboard");
        fs::create_dir_all(gadget.join("configs/c.1")).unwrap();
        fs::write(gadget.join("UDC"), "fe800000.usb\n").unwrap();
        write_hid_function(
            &function,
            1,
            1,
            8,
            DeviceNumber {
                major: 240,
                minor: 0,
            },
        );

        let mut node = hid_node("/dev/hidg0", 240, 0);
        add_stable_alias(&mut node, HidRole::Keyboard, "/dev/wingmankvm-keyboard");
        let roles = classify_hid_functions(tree.path(), &[node]);

        assert_eq!(roles.keyboard.len(), 1);
        assert_eq!(
            roles.keyboard[0].path,
            Path::new("/dev/wingmankvm-keyboard")
        );
        assert_eq!(
            roles.keyboard[0].label,
            "Boot Keyboard (wingmankvm-keyboard)"
        );
    }

    #[test]
    fn stable_alias_for_another_role_does_not_misclassify_node() {
        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let function = gadget.join("functions/hid.mouse");
        fs::create_dir_all(gadget.join("configs/c.1")).unwrap();
        fs::write(gadget.join("UDC"), "fe800000.usb\n").unwrap();
        write_hid_function(
            &function,
            1,
            2,
            4,
            DeviceNumber {
                major: 240,
                minor: 1,
            },
        );

        let mut node = hid_node("/dev/hidg1", 240, 1);
        add_stable_alias(&mut node, HidRole::Keyboard, "/dev/wingmankvm-keyboard");
        let roles = classify_hid_functions(tree.path(), &[node]);

        assert!(roles.keyboard.is_empty());
        assert_eq!(roles.mouse.len(), 1);
        assert_eq!(roles.mouse[0].path, Path::new("/dev/hidg1"));
    }

    #[cfg(unix)]
    #[test]
    fn stable_alias_requires_a_real_character_device_target() {
        use std::os::unix::fs::symlink;

        let tree = TestTree::new();
        let raw_path = tree.path().join("hidg0");
        fs::write(&raw_path, "not a HID gadget device").unwrap();
        symlink(&raw_path, tree.path().join("wingmankvm-keyboard")).unwrap();
        let mut nodes = vec![hid_node(raw_path.to_str().unwrap(), 240, 0)];

        attach_stable_hid_aliases(tree.path(), &mut nodes);

        assert!(nodes[0].stable_aliases.is_empty());
    }

    #[test]
    fn stable_alias_only_targets_numbered_hid_nodes() {
        assert!(is_numbered_hid_node(Path::new("/dev/hidg0")));
        assert!(is_numbered_hid_node(Path::new("/dev/hidg42")));
        assert!(!is_numbered_hid_node(Path::new("/dev/hidg")));
        assert!(!is_numbered_hid_node(Path::new("/dev/hidg-mouse")));
        assert!(!is_numbered_hid_node(Path::new("/dev/wingmankvm-keyboard")));
    }

    fn write_lun(path: &Path, attributes: &[&str]) {
        fs::create_dir_all(path).unwrap();
        for attribute in attributes {
            fs::write(path.join(attribute), "\n").unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn mass_storage_scan_uses_function_tree_and_deduplicates_canonical_luns() {
        use std::os::unix::fs::symlink;

        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let function = gadget.join("functions/mass_storage.0");
        let lun = function.join("lun.0");
        let config = gadget.join("configs/c.1");
        fs::create_dir_all(&config).unwrap();
        fs::write(gadget.join("UDC"), "fe800000.usb\n").unwrap();
        write_lun(&lun, &["file", "ro", "cdrom", "removable"]);
        symlink(&function, gadget.join("functions/mass_storage.alias")).unwrap();
        symlink(&function, config.join("mass_storage.0")).unwrap();

        let results = scan_mass_storage_luns(tree.path());

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, fs::canonicalize(&lun).unwrap());
        assert_eq!(results[0].gadget.as_deref(), Some("wingman"));
        assert_eq!(results[0].function.as_deref(), Some("mass_storage.0"));
        assert_eq!(results[0].udc.as_deref(), Some("fe800000.usb"));
        assert_eq!(results[0].gadget_bound, Some(true));
        assert_eq!(results[0].function_linked, Some(true));
        assert_eq!(results[0].compatible, Some(true));
        assert!(results[0].warnings.is_empty());
    }

    #[test]
    fn mass_storage_scan_marks_unlinked_unbound_and_incompatible_lun() {
        let tree = TestTree::new();
        let gadget = tree.path().join("wingman");
        let lun = gadget.join("functions/mass_storage.0/lun.1");
        fs::create_dir_all(gadget.join("configs/c.1")).unwrap();
        fs::write(gadget.join("UDC"), "\n").unwrap();
        write_lun(&lun, &["file", "ro", "cdrom"]);

        let results = scan_mass_storage_luns(tree.path());

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].gadget_bound, Some(false));
        assert_eq!(results[0].function_linked, Some(false));
        assert_eq!(results[0].compatible, Some(false));
        assert!(
            results[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("removable"))
        );
    }

    #[test]
    fn basic_candidate_keeps_original_json_shape() {
        let value = serde_json::to_value(DeviceCandidate::basic(
            PathBuf::from("/dev/video0"),
            "video0".into(),
            "video",
        ))
        .unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "path": "/dev/video0",
                "label": "video0",
                "kind": "video",
                "warnings": [],
            })
        );
    }

    #[test]
    fn video_candidates_prefer_capture_devices_with_mjpeg() {
        let mut unsupported =
            DeviceCandidate::basic(PathBuf::from("/dev/video0"), "video0".into(), "video");
        unsupported.video_capture = Some(false);
        unsupported.supports_mjpeg = Some(false);
        let mut jpeg_only =
            DeviceCandidate::basic(PathBuf::from("/dev/video1"), "video1".into(), "video");
        jpeg_only.video_capture = Some(true);
        jpeg_only.supports_mjpeg = Some(false);
        let mut passthrough =
            DeviceCandidate::basic(PathBuf::from("/dev/video5"), "video5".into(), "video");
        passthrough.video_capture = Some(true);
        passthrough.supports_mjpeg = Some(true);
        let mut candidates = vec![unsupported, jpeg_only, passthrough];

        sort_video_candidates(&mut candidates);

        assert_eq!(candidates[0].path, Path::new("/dev/video5"));
        assert_eq!(candidates[1].path, Path::new("/dev/video1"));
        assert_eq!(candidates[2].path, Path::new("/dev/video0"));
    }
}
