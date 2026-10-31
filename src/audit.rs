//! Append-only JSON Lines audit log.
//!
//! Audit writes must never fail a business request. Callers use [`AuditLog::record`],
//! which moves the blocking file append to the blocking pool and downgrades every
//! failure to a `tracing::warn!`.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub const AUDIT_FILE_NAME: &str = "audit.log";
pub const AUDIT_ROTATED_FILE_NAME: &str = "audit.log.1";
/// Rotate before the active file grows past this size.
pub const AUDIT_MAX_BYTES: u64 = 1024 * 1024;
pub const AUDIT_DEFAULT_LIMIT: usize = 100;
pub const AUDIT_MAX_LIMIT: usize = 500;
const MAX_DETAIL_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    LoginSucceeded,
    LoginFailed,
    Logout,
    SessionRevoked,
    SessionsRevokedOthers,
    PowerAction,
    GpioTest,
    ConfigChanged,
    PasskeyAdded,
    PasskeyRemoved,
    SetupCompleted,
    UpdateStarted,
    UpdateSucceeded,
    UpdateFailed,
}

impl AuditEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoginSucceeded => "login_succeeded",
            Self::LoginFailed => "login_failed",
            Self::Logout => "logout",
            Self::SessionRevoked => "session_revoked",
            Self::SessionsRevokedOthers => "sessions_revoked_others",
            Self::PowerAction => "power_action",
            Self::GpioTest => "gpio_test",
            Self::ConfigChanged => "config_changed",
            Self::PasskeyAdded => "passkey_added",
            Self::PasskeyRemoved => "passkey_removed",
            Self::SetupCompleted => "setup_completed",
            Self::UpdateStarted => "update_started",
            Self::UpdateSucceeded => "update_succeeded",
            Self::UpdateFailed => "update_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix time in milliseconds.
    pub ts: u64,
    pub event: String,
    pub ip: Option<IpAddr>,
    /// Public session id; never the bearer token.
    pub session: Option<String>,
    pub detail: Option<String>,
}

impl AuditEntry {
    pub fn new(
        event: AuditEvent,
        ip: Option<IpAddr>,
        session: Option<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            ts: unix_millis_now(),
            event: event.as_str().to_owned(),
            ip,
            session,
            detail: normalize_detail(detail),
        }
    }
}

type AuditObserver = Box<dyn Fn(&AuditEntry) + Send + Sync>;

#[derive(Clone)]
pub struct AuditLog {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
    /// Optional non-blocking listener (event notifications) told about every entry.
    observer: Arc<OnceLock<AuditObserver>>,
}

impl AuditLog {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            path: state_dir.into().join(AUDIT_FILE_NAME),
            write_lock: Arc::new(Mutex::new(())),
            observer: Arc::new(OnceLock::new()),
        }
    }

    /// Install the observer once; it must not block.
    pub fn set_observer(&self, observer: impl Fn(&AuditEntry) + Send + Sync + 'static) {
        let _ = self.observer.set(Box::new(observer));
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record an event without ever failing the calling request.
    pub async fn record(
        &self,
        event: AuditEvent,
        ip: Option<IpAddr>,
        session: Option<String>,
        detail: Option<String>,
    ) {
        let entry = AuditEntry::new(event, ip, session, detail);
        if let Some(observer) = self.observer.get() {
            observer(&entry);
        }
        let log = self.clone();
        match tokio::task::spawn_blocking(move || log.append(&entry)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "audit log write failed"),
            Err(error) => tracing::warn!(%error, "audit log task failed"),
        }
    }

    /// Append one entry synchronously. Exposed for tests and for callers that
    /// already run on the blocking pool.
    pub fn append(&self, entry: &AuditEntry) -> io::Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
        line.push(b'\n');

        let current_len = match fs::metadata(&self.path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        if current_len > 0 && current_len.saturating_add(line.len() as u64) > AUDIT_MAX_BYTES {
            self.rotate()?;
        }

        let mut options = OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&self.path)?;
        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        file.write_all(&line)?;
        file.sync_data()
    }

    fn rotate(&self) -> io::Result<()> {
        let rotated = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(AUDIT_ROTATED_FILE_NAME);
        match fs::remove_file(&rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.path, rotated)
    }

    /// Read the newest `limit` entries, newest first. Malformed lines are
    /// skipped rather than failing the whole endpoint.
    pub fn read_latest(&self, limit: usize) -> io::Result<Vec<AuditEntry>> {
        let rotated = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(AUDIT_ROTATED_FILE_NAME);
        let mut entries = Vec::new();
        for path in [rotated, self.path.clone()] {
            let contents = match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for line in contents.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(line) {
                    entries.push(entry);
                }
            }
        }
        let start = entries.len().saturating_sub(limit);
        let mut newest = entries.split_off(start);
        newest.reverse();
        Ok(newest)
    }
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditLog")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// Clamp a user-supplied audit limit to the documented range.
pub fn clamp_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(AUDIT_DEFAULT_LIMIT)
        .clamp(1, AUDIT_MAX_LIMIT)
}

fn normalize_detail(detail: Option<String>) -> Option<String> {
    detail.map(|detail| {
        detail
            .chars()
            .map(|character| if character.is_control() { ' ' } else { character })
            .take(MAX_DETAIL_CHARS)
            .collect()
    })
}

pub fn unix_millis_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, sync::atomic::{AtomicU64, Ordering}};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "wingmankvm-audit-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rotates_at_one_mebibyte_and_keeps_one_old_file() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        let detail = "x".repeat(1024);
        let entry = AuditEntry::new(
            AuditEvent::ConfigChanged,
            Some("203.0.113.10".parse().unwrap()),
            Some("public-id".to_owned()),
            Some(detail),
        );
        let mut line = serde_json::to_string(&entry).unwrap();
        line.push('\n');
        let oversized = line.repeat((AUDIT_MAX_BYTES as usize / line.len()) + 1);
        fs::write(log.path(), oversized).unwrap();
        log.append(&entry).unwrap();

        let rotated = directory.0.join(AUDIT_ROTATED_FILE_NAME);
        assert!(rotated.is_file(), "the previous file must be rotated");
        assert!(log.path().is_file());
        assert!(
            fs::metadata(log.path()).unwrap().len() <= AUDIT_MAX_BYTES,
            "the active log must stay within the rotation bound"
        );

        let newest = log.read_latest(2).unwrap();
        assert_eq!(newest.len(), 2);
        assert!(newest[0].ts >= newest[1].ts);
        assert_eq!(newest[0].event, AuditEvent::ConfigChanged.as_str());
        assert_eq!(newest[0].detail.as_deref().map(str::len), Some(512));
    }

    #[cfg(unix)]
    #[test]
    fn audit_files_are_private() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        let entry = AuditEntry::new(AuditEvent::LoginSucceeded, None, None, None);
        log.append(&entry).unwrap();
        let mode = fs::metadata(log.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        // Force a rotation and check the rotated file keeps the same mode.
        let filler = AuditEntry::new(
            AuditEvent::ConfigChanged,
            None,
            None,
            Some("y".repeat(1024)),
        );
        let mut line = serde_json::to_string(&filler).unwrap();
        line.push('\n');
        fs::write(
            log.path(),
            line.repeat((AUDIT_MAX_BYTES as usize / line.len()) + 1),
        )
        .unwrap();
        log.append(&filler).unwrap();
        let rotated = directory.0.join(AUDIT_ROTATED_FILE_NAME);
        let mode = fs::metadata(rotated).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn read_latest_skips_malformed_lines_and_returns_newest_first() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        let first = AuditEntry::new(AuditEvent::LoginFailed, None, None, Some("first".into()));
        let second = AuditEntry::new(AuditEvent::Logout, None, None, Some("second".into()));
        log.append(&first).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(log.path())
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        log.append(&second).unwrap();

        let entries = log.read_latest(10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].detail.as_deref(), Some("second"));
        assert_eq!(entries[1].detail.as_deref(), Some("first"));
        assert_eq!(log.read_latest(1).unwrap().len(), 1);
    }

    #[test]
    fn limit_defaults_and_caps() {
        assert_eq!(clamp_limit(None), 100);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(50)), 50);
        assert_eq!(clamp_limit(Some(501)), 500);
        assert_eq!(clamp_limit(Some(usize::MAX)), 500);
    }

    #[test]
    fn details_are_bounded_and_control_characters_are_neutralized() {
        let entry = AuditEntry::new(
            AuditEvent::LoginFailed,
            None,
            None,
            Some(format!("line\nbreak{}", "z".repeat(1000))),
        );
        let detail = entry.detail.unwrap();
        assert_eq!(detail.chars().count(), MAX_DETAIL_CHARS);
        assert!(!detail.contains('\n'));
    }
}
