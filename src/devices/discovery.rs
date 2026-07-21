use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDiscovery {
    pub video: Vec<DeviceCandidate>,
    pub hid: Vec<DeviceCandidate>,
    pub gpio: Vec<DeviceCandidate>,
    pub mass_storage_luns: Vec<DeviceCandidate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCandidate {
    pub path: PathBuf,
    pub label: String,
    pub kind: String,
    pub warnings: Vec<String>,
}

pub async fn scan() -> DeviceDiscovery {
    let video = scan_prefix(Path::new("/dev"), "video", "video").await;
    let hid = scan_prefix(Path::new("/dev"), "hidg", "hid_gadget").await;
    let gpio = scan_prefix(Path::new("/dev"), "gpiochip", "gpio_chip").await;
    let mass_storage_luns = scan_luns(Path::new("/sys/kernel/config/usb_gadget")).await;
    DeviceDiscovery {
        video,
        hid,
        gpio,
        mass_storage_luns,
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
            candidates.push(DeviceCandidate {
                path: entry.path(),
                label: name,
                kind: kind.to_string(),
                warnings: if kind == "hid_gadget" {
                    vec!["请通过向导测试确认这是键盘还是鼠标；设备编号本身不可靠".into()]
                } else {
                    Vec::new()
                },
            });
        }
    }
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    candidates
}

async fn scan_luns(root: &Path) -> Vec<DeviceCandidate> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        walk_luns(&root, 0, &mut results);
        results.sort_by(|a, b| a.path.cmp(&b.path));
        results
    })
    .await
    .unwrap_or_default()
}

fn walk_luns(path: &Path, depth: usize, results: &mut Vec<DeviceCandidate>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            if child.file_name().is_some_and(|name| name == "lun.0") && child.join("file").exists()
            {
                results.push(DeviceCandidate {
                    label: child.display().to_string(),
                    path: child.clone(),
                    kind: "mass_storage_lun".into(),
                    warnings: Vec::new(),
                });
            } else {
                walk_luns(&child, depth + 1, results);
            }
        }
    }
}
