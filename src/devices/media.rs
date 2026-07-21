use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;
use tokio::{fs, sync::Mutex};

#[derive(Debug, Clone)]
pub struct MediaConfigSnapshot {
    pub image_dir: PathBuf,
    pub lun_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaImage {
    pub name: String,
    pub size: u64,
    pub mounted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaStatus {
    pub mounted_path: Option<String>,
    pub read_only: Option<bool>,
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
        fs::create_dir_all(&config.image_dir).await?;
        let status = read_status(&config).await?;
        let mounted = status.mounted_path.as_deref();
        let mut entries = fs::read_dir(&config.image_dir).await?;
        let mut images = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !entry.file_type().await?.is_file() || !supported_image(&path) {
                continue;
            }
            let canonical = fs::canonicalize(&path).await?;
            images.push(MediaImage {
                name: entry.file_name().to_string_lossy().to_string(),
                size: entry.metadata().await?.len(),
                mounted: mounted == canonical.to_str(),
            });
        }
        images.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((images, status))
    }

    pub async fn attach(
        &self,
        config: Option<MediaConfigSnapshot>,
        name: &str,
        read_only: bool,
    ) -> Result<MediaStatus, MediaError> {
        let config = config.ok_or(MediaError::NotConfigured)?;
        let _guard = self.operation.lock().await;
        let image = resolve_image(&config.image_dir, name).await?;
        let is_iso = image
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iso"));

        write_attr(&config.lun_path.join("file"), "\n").await?;
        write_attr(
            &config.lun_path.join("cdrom"),
            if is_iso { "1\n" } else { "0\n" },
        )
        .await?;
        write_attr(
            &config.lun_path.join("ro"),
            if is_iso || read_only { "1\n" } else { "0\n" },
        )
        .await?;
        write_attr(&config.lun_path.join("removable"), "1\n").await?;
        write_attr(
            &config.lun_path.join("file"),
            &format!("{}\n", image.display()),
        )
        .await?;
        read_status(&config).await
    }

    pub async fn detach(
        &self,
        config: Option<MediaConfigSnapshot>,
        force: bool,
    ) -> Result<MediaStatus, MediaError> {
        let config = config.ok_or(MediaError::NotConfigured)?;
        let _guard = self.operation.lock().await;
        if force {
            write_attr(&config.lun_path.join("forced_eject"), "1\n").await?;
        } else {
            write_attr(&config.lun_path.join("file"), "\n").await?;
        }
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
    let root = fs::canonicalize(root).await?;
    let image = fs::canonicalize(root.join(name)).await?;
    if !image.starts_with(&root) {
        return Err(MediaError::OutsideStorage);
    }
    Ok(image)
}

fn supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("iso") || extension.eq_ignore_ascii_case("img")
        })
}

async fn read_status(config: &MediaConfigSnapshot) -> Result<MediaStatus, MediaError> {
    let mounted_path = read_optional_attr(&config.lun_path.join("file")).await?;
    let read_only = read_optional_attr(&config.lun_path.join("ro"))
        .await?
        .map(|value| value == "1");
    Ok(MediaStatus {
        mounted_path,
        read_only,
    })
}

async fn read_optional_attr(path: &Path) -> Result<Option<String>, MediaError> {
    match fs::read_to_string(path).await {
        Ok(value) => {
            let value = value.trim().to_string();
            Ok((!value.is_empty()).then_some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn write_attr(path: &Path, value: &str) -> Result<(), MediaError> {
    fs::write(path, value.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_names_cannot_escape_storage() {
        assert!(sanitize_upload_name("rescue.iso").is_ok());
        assert!(sanitize_upload_name("disk.img").is_ok());
        assert!(sanitize_upload_name("../secret.iso").is_err());
        assert!(sanitize_upload_name("firmware.bin").is_err());
    }
}
