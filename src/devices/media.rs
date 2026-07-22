use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    sync::Mutex,
    time::{Instant, sleep, timeout},
};

const ATTRIBUTE_IO_TIMEOUT: Duration = Duration::from_secs(1);
const MEDIA_STATE_TIMEOUT: Duration = Duration::from_secs(2);
const MEDIA_STATE_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone)]
pub struct MediaConfigSnapshot {
    pub image_dir: PathBuf,
    pub lun_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    #[default]
    Auto,
    Cdrom,
    Disk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaState {
    Detached,
    Attached,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaImage {
    pub name: String,
    pub size: u64,
    pub mounted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaStatus {
    pub state: MediaState,
    pub mounted_path: Option<String>,
    pub mounted_name: Option<String>,
    pub media_type: Option<MediaType>,
    pub read_only: Option<bool>,
    pub cdrom: Option<bool>,
    pub removable: Option<bool>,
    pub forced_eject_supported: bool,
}

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("virtual media is not configured")]
    NotConfigured,
    #[error("invalid image name")]
    InvalidName,
    #[error("image is outside the configured storage directory")]
    OutsideStorage,
    #[error("unsupported image type; expected .iso or .img")]
    UnsupportedType,
    #[error("image does not exist")]
    ImageNotFound,
    #[error("image is not a regular file")]
    NotRegularFile,
    #[error("image is empty")]
    EmptyImage,
    #[error("virtual media LUN attribute is missing: {0}")]
    MissingLunAttribute(&'static str),
    #[error("forced eject is not supported by this kernel or LUN")]
    ForceEjectUnsupported,
    #[error("virtual media is already attached: {0}")]
    AlreadyAttached(String),
    #[error("the backing file could not be attached read-write; the kernel exposed it read-only")]
    ReadOnlyFallback,
    #[error("virtual media {0} timed out")]
    IoTimedOut(&'static str),
    #[error("timed out waiting for virtual media to become {0}")]
    StateTimedOut(&'static str),
    #[error("the kernel did not apply the requested LUN attribute: {0}")]
    LunAttributeRejected(&'static str),
    #[error("virtual media I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Default)]
pub struct MediaManager {
    operation: Mutex<()>,
}

impl MediaManager {
    pub async fn list(
        &self,
        config: Option<MediaConfigSnapshot>,
    ) -> Result<(Vec<MediaImage>, MediaStatus), MediaError> {
        let config = config.ok_or(MediaError::NotConfigured)?;
        let _guard = self.operation.lock().await;
        io_timeout(
            "creating the image directory",
            fs::create_dir_all(&config.image_dir),
        )
        .await?;
        validate_required_lun_attributes(&config.lun_path).await?;
        let status = read_status(&config).await?;
        let mounted = status.mounted_path.as_deref().map(Path::new);
        let mut entries = io_timeout(
            "reading the image directory",
            fs::read_dir(&config.image_dir),
        )
        .await?;
        let mut images = Vec::new();
        while let Some(entry) = io_timeout("reading an image entry", entries.next_entry()).await? {
            let path = entry.path();
            let file_type = io_timeout("reading image metadata", entry.file_type()).await?;
            if !file_type.is_file() || !supported_image(&path) {
                continue;
            }
            let canonical = io_timeout("resolving an image path", fs::canonicalize(&path)).await?;
            let metadata = io_timeout("reading image metadata", entry.metadata()).await?;
            images.push(MediaImage {
                name: entry.file_name().to_string_lossy().to_string(),
                size: metadata.len(),
                mounted: mounted == Some(canonical.as_path()),
            });
        }
        images.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((images, status))
    }

    pub async fn attach(
        &self,
        config: Option<MediaConfigSnapshot>,
        name: &str,
        media_type: MediaType,
        read_only: bool,
    ) -> Result<MediaStatus, MediaError> {
        let config = config.ok_or(MediaError::NotConfigured)?;
        let _guard = self.operation.lock().await;

        io_timeout(
            "creating the image directory",
            fs::create_dir_all(&config.image_dir),
        )
        .await?;
        let image = resolve_image(&config.image_dir, name).await?;
        validate_required_lun_attributes(&config.lun_path).await?;

        let is_iso = has_extension(&image, "iso");
        let resolved_type = match media_type {
            MediaType::Auto if is_iso => MediaType::Cdrom,
            MediaType::Auto => MediaType::Disk,
            selected => selected,
        };
        let cdrom = resolved_type == MediaType::Cdrom;
        let read_only = read_only || is_iso || cdrom;

        let current = read_status(&config).await?;
        if current.state == MediaState::Attached {
            if status_matches_request(&current, &image, cdrom, read_only) {
                return Ok(current);
            }
            return Err(MediaError::AlreadyAttached(
                current
                    .mounted_name
                    .unwrap_or_else(|| "unknown media".to_string()),
            ));
        }

        clear_backing_file(&config.lun_path).await?;
        write_attr(
            &config.lun_path.join("cdrom"),
            if cdrom { "1\n" } else { "0\n" },
        )
        .await?;
        write_attr(
            &config.lun_path.join("ro"),
            if read_only { "1\n" } else { "0\n" },
        )
        .await?;
        write_attr(&config.lun_path.join("removable"), "1\n").await?;
        write_attr(
            &config.lun_path.join("file"),
            &format!("{}\n", image.display()),
        )
        .await?;
        wait_for_backing_file(&config.lun_path.join("file"), Some(&image)).await?;

        let status = read_status(&config).await?;
        verify_attached_status_or_rollback(&status, cdrom, read_only, &config.lun_path).await?;
        Ok(status)
    }

    pub async fn detach(
        &self,
        config: Option<MediaConfigSnapshot>,
        force: bool,
    ) -> Result<MediaStatus, MediaError> {
        let config = config.ok_or(MediaError::NotConfigured)?;
        let _guard = self.operation.lock().await;

        validate_lun_attribute(&config.lun_path, "file").await?;
        if force && !lun_attribute_exists(&config.lun_path, "forced_eject").await? {
            return Err(MediaError::ForceEjectUnsupported);
        }

        let status = read_status(&config).await?;
        if status.state == MediaState::Detached {
            return Ok(status);
        }

        if force {
            write_attr(&config.lun_path.join("forced_eject"), "1\n").await?;
        } else {
            write_attr(&config.lun_path.join("file"), "\n").await?;
        }
        wait_for_backing_file(&config.lun_path.join("file"), None).await?;
        read_status(&config).await
    }
}

pub fn sanitize_upload_name(name: &str) -> Result<String, MediaError> {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(MediaError::InvalidName)?;
    if file_name.is_empty() || file_name.starts_with('.') || file_name != name {
        return Err(MediaError::InvalidName);
    }
    if !supported_image(Path::new(file_name)) {
        return Err(MediaError::UnsupportedType);
    }
    Ok(file_name.to_string())
}

async fn resolve_image(root: &Path, name: &str) -> Result<PathBuf, MediaError> {
    let name = sanitize_upload_name(name)?;
    let root = io_timeout("resolving the image directory", fs::canonicalize(root)).await?;
    let image = match timeout(ATTRIBUTE_IO_TIMEOUT, fs::canonicalize(root.join(name))).await {
        Ok(Ok(image)) => image,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(MediaError::ImageNotFound);
        }
        Ok(Err(error)) => return Err(error.into()),
        Err(_) => return Err(MediaError::IoTimedOut("resolving the image path")),
    };
    if !image.starts_with(&root) {
        return Err(MediaError::OutsideStorage);
    }
    let metadata = io_timeout("reading image metadata", fs::metadata(&image)).await?;
    if !metadata.is_file() {
        return Err(MediaError::NotRegularFile);
    }
    if metadata.len() == 0 {
        return Err(MediaError::EmptyImage);
    }
    Ok(image)
}

fn supported_image(path: &Path) -> bool {
    has_extension(path, "iso") || has_extension(path, "img")
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn status_matches_request(
    status: &MediaStatus,
    image: &Path,
    cdrom: bool,
    read_only: bool,
) -> bool {
    status.mounted_path.as_deref().map(Path::new) == Some(image)
        && status.cdrom == Some(cdrom)
        && status.read_only == Some(read_only)
        && status.removable == Some(true)
}

fn verify_attached_status(
    status: &MediaStatus,
    cdrom: bool,
    read_only: bool,
) -> Result<(), MediaError> {
    if status.cdrom != Some(cdrom) {
        return Err(MediaError::LunAttributeRejected("cdrom"));
    }
    if !read_only && status.read_only == Some(true) {
        return Err(MediaError::ReadOnlyFallback);
    }
    if status.read_only != Some(read_only) {
        return Err(MediaError::LunAttributeRejected("ro"));
    }
    if status.removable != Some(true) {
        return Err(MediaError::LunAttributeRejected("removable"));
    }
    Ok(())
}

async fn verify_attached_status_or_rollback(
    status: &MediaStatus,
    cdrom: bool,
    read_only: bool,
    lun_path: &Path,
) -> Result<(), MediaError> {
    if let Err(error) = verify_attached_status(status, cdrom, read_only) {
        let _ = clear_backing_file(lun_path).await;
        return Err(error);
    }
    Ok(())
}

async fn validate_required_lun_attributes(lun_path: &Path) -> Result<(), MediaError> {
    for name in ["file", "ro", "cdrom", "removable"] {
        validate_lun_attribute(lun_path, name).await?;
    }
    Ok(())
}

async fn validate_lun_attribute(lun_path: &Path, name: &'static str) -> Result<(), MediaError> {
    if lun_attribute_exists(lun_path, name).await? {
        Ok(())
    } else {
        Err(MediaError::MissingLunAttribute(name))
    }
}

async fn lun_attribute_exists(lun_path: &Path, name: &'static str) -> Result<bool, MediaError> {
    let path = lun_path.join(name);
    match timeout(ATTRIBUTE_IO_TIMEOUT, fs::metadata(path)).await {
        Ok(Ok(metadata)) => Ok(metadata.is_file()),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(MediaError::IoTimedOut("checking a LUN attribute")),
    }
}

async fn clear_backing_file(lun_path: &Path) -> Result<(), MediaError> {
    let file_attr = lun_path.join("file");
    write_attr(&file_attr, "\n").await?;
    wait_for_backing_file(&file_attr, None).await
}

async fn wait_for_backing_file(path: &Path, expected: Option<&Path>) -> Result<(), MediaError> {
    let deadline = Instant::now() + MEDIA_STATE_TIMEOUT;
    loop {
        let current = read_optional_attr(path).await?;
        let matches = match (current.as_deref(), expected) {
            (None, None) => true,
            (Some(current), Some(expected)) => Path::new(current) == expected,
            _ => false,
        };
        if matches {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(MediaError::StateTimedOut(if expected.is_some() {
                "attached"
            } else {
                "detached"
            }));
        }
        sleep(MEDIA_STATE_POLL_INTERVAL).await;
    }
}

async fn read_status(config: &MediaConfigSnapshot) -> Result<MediaStatus, MediaError> {
    let file_path = config.lun_path.join("file");
    let ro_path = config.lun_path.join("ro");
    let cdrom_path = config.lun_path.join("cdrom");
    let removable_path = config.lun_path.join("removable");
    let (mounted_path, read_only, cdrom, removable, forced_eject_supported) = tokio::join!(
        read_optional_attr(&file_path),
        read_bool_attr(&ro_path),
        read_bool_attr(&cdrom_path),
        read_bool_attr(&removable_path),
        lun_attribute_exists(&config.lun_path, "forced_eject"),
    );
    let mounted_path = mounted_path?;
    let read_only = read_only?;
    let cdrom = cdrom?;
    let removable = removable?;
    let forced_eject_supported = forced_eject_supported?;
    let state = if mounted_path.is_some() {
        MediaState::Attached
    } else {
        MediaState::Detached
    };
    let mounted_name = mounted_path.as_deref().and_then(|path| {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    });
    let media_type = mounted_path.as_ref().and_then(|_| {
        cdrom.map(|is_cdrom| {
            if is_cdrom {
                MediaType::Cdrom
            } else {
                MediaType::Disk
            }
        })
    });

    Ok(MediaStatus {
        state,
        mounted_path,
        mounted_name,
        media_type,
        read_only,
        cdrom,
        removable,
        forced_eject_supported,
    })
}

async fn read_bool_attr(path: &Path) -> Result<Option<bool>, MediaError> {
    Ok(read_optional_attr(path).await?.map(|value| value == "1"))
}

async fn read_optional_attr(path: &Path) -> Result<Option<String>, MediaError> {
    match timeout(ATTRIBUTE_IO_TIMEOUT, fs::read_to_string(path)).await {
        Ok(Ok(value)) => {
            let value = value.trim().to_string();
            Ok((!value.is_empty()).then_some(value))
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(MediaError::IoTimedOut("reading a LUN attribute")),
    }
}

async fn write_attr(path: &Path, value: &str) -> Result<(), MediaError> {
    io_timeout("writing a LUN attribute", fs::write(path, value.as_bytes())).await
}

async fn io_timeout<T>(
    operation: &'static str,
    future: impl Future<Output = std::io::Result<T>>,
) -> Result<T, MediaError> {
    match timeout(ATTRIBUTE_IO_TIMEOUT, future).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(MediaError::IoTimedOut(operation)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs as std_fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct TestTree {
        root: PathBuf,
        image_dir: PathBuf,
        lun_path: PathBuf,
    }

    impl TestTree {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("wingmankvm-media-test-{}-{id}", std::process::id()));
            let image_dir = root.join("images");
            let lun_path = root.join("lun.0");
            std_fs::create_dir_all(&image_dir).unwrap();
            std_fs::create_dir_all(&lun_path).unwrap();
            for (name, value) in [
                ("file", ""),
                ("ro", "1\n"),
                ("cdrom", "0\n"),
                ("removable", "1\n"),
            ] {
                std_fs::write(lun_path.join(name), value).unwrap();
            }
            Self {
                root,
                image_dir,
                lun_path,
            }
        }

        fn config(&self) -> MediaConfigSnapshot {
            MediaConfigSnapshot {
                image_dir: self.image_dir.clone(),
                lun_path: self.lun_path.clone(),
            }
        }

        fn image(&self, name: &str, contents: &[u8]) {
            std_fs::write(self.image_dir.join(name), contents).unwrap();
        }
    }

    impl Drop for TestTree {
        fn drop(&mut self) {
            let _ = std_fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn media_type_uses_stable_json_names() {
        assert_eq!(serde_json::to_string(&MediaType::Auto).unwrap(), "\"auto\"");
        assert_eq!(
            serde_json::from_str::<MediaType>("\"cdrom\"").unwrap(),
            MediaType::Cdrom
        );
        assert_eq!(
            serde_json::from_str::<MediaType>("\"disk\"").unwrap(),
            MediaType::Disk
        );
    }

    #[test]
    fn upload_names_cannot_escape_storage() {
        assert!(sanitize_upload_name("rescue.iso").is_ok());
        assert!(sanitize_upload_name("disk.img").is_ok());
        assert!(sanitize_upload_name("../secret.iso").is_err());
        assert!(sanitize_upload_name("firmware.bin").is_err());
    }

    #[tokio::test]
    async fn auto_iso_is_attached_as_read_only_cdrom() {
        let tree = TestTree::new();
        tree.image("rescue.iso", b"not-empty");

        let status = MediaManager::default()
            .attach(Some(tree.config()), "rescue.iso", MediaType::Auto, false)
            .await
            .unwrap();

        assert_eq!(status.state, MediaState::Attached);
        assert_eq!(status.mounted_name.as_deref(), Some("rescue.iso"));
        assert_eq!(status.media_type, Some(MediaType::Cdrom));
        assert_eq!(status.read_only, Some(true));
        assert_eq!(status.cdrom, Some(true));
        assert_eq!(status.removable, Some(true));
    }

    #[tokio::test]
    async fn img_disk_can_be_attached_read_write() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");

        let status = MediaManager::default()
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        assert_eq!(status.media_type, Some(MediaType::Disk));
        assert_eq!(status.read_only, Some(false));
        assert_eq!(status.cdrom, Some(false));
    }

    #[tokio::test]
    async fn img_cdrom_mode_is_forced_read_only() {
        let tree = TestTree::new();
        tree.image("recovery.img", b"not-empty");

        let status = MediaManager::default()
            .attach(Some(tree.config()), "recovery.img", MediaType::Cdrom, false)
            .await
            .unwrap();

        assert_eq!(status.media_type, Some(MediaType::Cdrom));
        assert_eq!(status.read_only, Some(true));
    }

    #[tokio::test]
    async fn iso_remains_read_only_in_disk_mode() {
        let tree = TestTree::new();
        tree.image("installer.iso", b"not-empty");

        let status = MediaManager::default()
            .attach(Some(tree.config()), "installer.iso", MediaType::Disk, false)
            .await
            .unwrap();

        assert_eq!(status.media_type, Some(MediaType::Disk));
        assert_eq!(status.read_only, Some(true));
    }

    #[tokio::test]
    async fn empty_images_are_rejected() {
        let tree = TestTree::new();
        tree.image("empty.img", b"");

        let error = MediaManager::default()
            .attach(Some(tree.config()), "empty.img", MediaType::Auto, false)
            .await
            .unwrap_err();

        assert!(matches!(error, MediaError::EmptyImage));
    }

    #[tokio::test]
    async fn missing_images_have_a_specific_error() {
        let tree = TestTree::new();

        let error = MediaManager::default()
            .attach(Some(tree.config()), "missing.img", MediaType::Auto, false)
            .await
            .unwrap_err();

        assert!(matches!(error, MediaError::ImageNotFound));
    }

    #[tokio::test]
    async fn missing_required_lun_attribute_is_reported() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");
        std_fs::remove_file(tree.lun_path.join("cdrom")).unwrap();

        let error = MediaManager::default()
            .attach(Some(tree.config()), "storage.img", MediaType::Auto, false)
            .await
            .unwrap_err();

        assert!(matches!(error, MediaError::MissingLunAttribute("cdrom")));
    }

    #[tokio::test]
    async fn list_reports_an_incomplete_lun() {
        let tree = TestTree::new();
        std_fs::remove_file(tree.lun_path.join("ro")).unwrap();

        let error = MediaManager::default()
            .list(Some(tree.config()))
            .await
            .unwrap_err();

        assert!(matches!(error, MediaError::MissingLunAttribute("ro")));
    }

    #[tokio::test]
    async fn force_detach_requires_kernel_support() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");
        let manager = MediaManager::default();
        manager
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        let error = manager.detach(Some(tree.config()), true).await.unwrap_err();
        assert!(matches!(error, MediaError::ForceEjectUnsupported));
    }

    #[tokio::test]
    async fn identical_attach_is_idempotent() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");
        let manager = MediaManager::default();
        let first = manager
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        let second = manager
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        assert_eq!(first.mounted_path, second.mounted_path);
        assert_eq!(second.state, MediaState::Attached);
        assert_eq!(second.read_only, Some(false));
    }

    #[tokio::test]
    async fn replacing_attached_media_requires_detach() {
        let tree = TestTree::new();
        tree.image("first.img", b"first");
        tree.image("second.img", b"second");
        let manager = MediaManager::default();
        manager
            .attach(Some(tree.config()), "first.img", MediaType::Disk, false)
            .await
            .unwrap();

        let error = manager
            .attach(Some(tree.config()), "second.img", MediaType::Disk, false)
            .await
            .unwrap_err();

        assert!(matches!(error, MediaError::AlreadyAttached(name) if name == "first.img"));
        assert!(
            std_fs::read_to_string(tree.lun_path.join("file"))
                .unwrap()
                .ends_with("first.img\n")
        );
    }

    #[tokio::test]
    async fn writable_fallback_is_reported_and_rolled_back() {
        let tree = TestTree::new();
        std_fs::write(tree.lun_path.join("file"), "/images/storage.img\n").unwrap();
        let status = MediaStatus {
            state: MediaState::Attached,
            mounted_path: Some("/images/storage.img".to_string()),
            mounted_name: Some("storage.img".to_string()),
            media_type: Some(MediaType::Disk),
            read_only: Some(true),
            cdrom: Some(false),
            removable: Some(true),
            forced_eject_supported: false,
        };

        assert!(matches!(
            verify_attached_status_or_rollback(&status, false, false, &tree.lun_path).await,
            Err(MediaError::ReadOnlyFallback)
        ));
        assert_eq!(
            std_fs::read_to_string(tree.lun_path.join("file")).unwrap(),
            "\n"
        );
    }

    #[tokio::test]
    async fn normal_detach_waits_until_the_backing_file_is_empty() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");
        let manager = MediaManager::default();
        manager
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        let status = manager.detach(Some(tree.config()), false).await.unwrap();

        assert_eq!(status.state, MediaState::Detached);
        assert_eq!(status.mounted_path, None);
        assert_eq!(status.mounted_name, None);
        assert_eq!(status.media_type, None);
    }

    #[tokio::test]
    async fn force_detach_waits_for_the_kernel_to_eject() {
        let tree = TestTree::new();
        tree.image("storage.img", b"not-empty");
        std_fs::write(tree.lun_path.join("forced_eject"), "").unwrap();
        let manager = MediaManager::default();
        manager
            .attach(Some(tree.config()), "storage.img", MediaType::Disk, false)
            .await
            .unwrap();

        let forced_eject_path = tree.lun_path.join("forced_eject");
        let backing_file_path = tree.lun_path.join("file");
        let kernel = tokio::spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                if std_fs::read_to_string(&forced_eject_path).is_ok_and(|value| value.trim() == "1")
                {
                    std_fs::write(&backing_file_path, "").unwrap();
                    return;
                }
                assert!(Instant::now() < deadline, "forced_eject was not written");
                sleep(Duration::from_millis(5)).await;
            }
        });

        let status = manager.detach(Some(tree.config()), true).await.unwrap();
        kernel.await.unwrap();

        assert_eq!(status.state, MediaState::Detached);
        assert!(status.forced_eject_supported);
    }
}
