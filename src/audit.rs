//! Append-only JSON Lines audit log.
//!
//! Audit writes must never fail a business request. Callers use [`AuditLog::record`],
//! which moves the blocking file append to the blocking pool and downgrades every
//! failure to a `tracing::warn!`.
//!
//! Failed logins can be triggered without authentication, so they go to their own
//! pair of files (`audit-login-failures.log` and `.1`). A flood of them can then only
//! rotate away older failed logins, never power, configuration or update records in
//! `audit.log`. Those files are not `sync_data`'d per entry so a flood cannot queue
//! fsyncs; every other event is still synced before `append` returns.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub const AUDIT_FILE_NAME: &str = "audit.log";
pub const AUDIT_ROTATED_FILE_NAME: &str = "audit.log.1";
/// Separate files for unauthenticated failed logins; same rotation and mode.
pub const AUDIT_LOGIN_FAILURES_FILE_NAME: &str = "audit-login-failures.log";
pub const AUDIT_LOGIN_FAILURES_ROTATED_FILE_NAME: &str = "audit-login-failures.log.1";
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

/// One JSON Lines file plus its single rotated predecessor, with its own lock so a
/// burst on one file never waits behind writes to the other.
struct AuditFile {
    path: PathBuf,
    rotated: PathBuf,
    /// Whether every append is flushed to stable storage before returning.
    sync: bool,
    write_lock: Mutex<()>,
}

impl AuditFile {
    fn new(directory: &Path, name: &str, rotated_name: &str, sync: bool) -> Self {
        Self {
            path: directory.join(name),
            rotated: directory.join(rotated_name),
            sync,
            write_lock: Mutex::new(()),
        }
    }

    fn append(&self, line: &[u8]) -> io::Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

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
        file.write_all(line)?;
        if self.sync {
            file.sync_data()?;
        }
        Ok(())
    }

    fn rotate(&self) -> io::Result<()> {
        match fs::remove_file(&self.rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.path, &self.rotated)
    }

    /// Up to `limit` entries in reverse append order. The rotated file is only
    /// read when the active one holds fewer than `limit` parseable entries.
    fn read_newest(&self, limit: usize) -> io::Result<Vec<AuditEntry>> {
        let mut newest = Vec::new();
        for path in [&self.path, &self.rotated] {
            let remaining = limit.saturating_sub(newest.len());
            if remaining == 0 {
                break;
            }
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            // Lossy so a torn, unsynced trailing line cannot fail the whole read.
            let contents = String::from_utf8_lossy(&bytes);
            newest.extend(
                contents
                    .lines()
                    .rev()
                    .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
                    .take(remaining),
            );
        }
        Ok(newest)
    }
}

#[derive(Clone)]
pub struct AuditLog {
    /// `audit.log`: every event except failed logins, synced per entry.
    main: Arc<AuditFile>,
    /// `audit-login-failures.log`: failed logins only, not synced per entry.
    login_failures: Arc<AuditFile>,
    /// Optional non-blocking listener (event notifications) told about every entry.
    observer: Arc<OnceLock<AuditObserver>>,
    /// Entries that could not be written; reported by /api/status.
    write_failures: Arc<AtomicU64>,
}

impl AuditLog {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        let state_dir = state_dir.into();
        Self {
            main: Arc::new(AuditFile::new(
                &state_dir,
                AUDIT_FILE_NAME,
                AUDIT_ROTATED_FILE_NAME,
                true,
            )),
            login_failures: Arc::new(AuditFile::new(
                &state_dir,
                AUDIT_LOGIN_FAILURES_FILE_NAME,
                AUDIT_LOGIN_FAILURES_ROTATED_FILE_NAME,
                false,
            )),
            observer: Arc::new(OnceLock::new()),
            write_failures: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn write_failures(&self) -> u64 {
        self.write_failures.load(Ordering::Relaxed)
    }

    /// Install the observer once; it must not block.
    pub fn set_observer(&self, observer: impl Fn(&AuditEntry) + Send + Sync + 'static) {
        let _ = self.observer.set(Box::new(observer));
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.main.path
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
            Ok(Err(error)) => {
                self.write_failures.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "audit log write failed");
            }
            Err(error) => {
                self.write_failures.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "audit log task failed");
            }
        }
    }

    /// Append one entry synchronously. Exposed for tests and for callers that
    /// already run on the blocking pool. Failed logins go to their own files.
    pub fn append(&self, entry: &AuditEntry) -> io::Result<()> {
        let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
        line.push(b'\n');
        if entry.event == AuditEvent::LoginFailed.as_str() {
            self.login_failures.append(&line)
        } else {
            self.main.append(&line)
        }
    }

    /// Read the newest `limit` entries across both file pairs, newest first.
    /// Each file keeps its append order; the two are interleaved by `ts`, and on
    /// equal `ts` the `audit.log` entry comes first. Malformed lines are skipped
    /// rather than failing the whole endpoint. At most `limit` entries are parsed
    /// from each pair and every file is read at most once.
    pub fn read_latest(&self, limit: usize) -> io::Result<Vec<AuditEntry>> {
        let main = self.main.read_newest(limit)?;
        let login_failures = self.login_failures.read_newest(limit)?;
        Ok(merge_newest_first(main, login_failures, limit))
    }
}

/// Merge two newest-first lists into one, preferring `main` on equal `ts`.
fn merge_newest_first(
    main: Vec<AuditEntry>,
    login_failures: Vec<AuditEntry>,
    limit: usize,
) -> Vec<AuditEntry> {
    let mut merged = Vec::with_capacity(limit.min(main.len() + login_failures.len()));
    let mut main = main.into_iter().peekable();
    let mut login_failures = login_failures.into_iter().peekable();
    while merged.len() < limit {
        let take_main = match (main.peek(), login_failures.peek()) {
            (Some(left), Some(right)) => left.ts >= right.ts,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        merged.extend(if take_main {
            main.next()
        } else {
            login_failures.next()
        });
    }
    merged
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditLog")
            .field("path", &self.main.path)
            .field("login_failures_path", &self.login_failures.path)
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

    #[tokio::test]
    async fn failed_writes_are_counted() {
        let directory = TestDirectory::new();
        // A file where the state directory should be makes every write fail.
        let blocked = directory.0.join("not-a-directory");
        fs::write(&blocked, b"x").unwrap();
        let log = AuditLog::new(&blocked);
        log.record(AuditEvent::ConfigChanged, None, None, None).await;
        assert_eq!(log.write_failures(), 1);

        let log = AuditLog::new(&directory.0);
        log.record(AuditEvent::ConfigChanged, None, None, None).await;
        assert_eq!(log.write_failures(), 0);
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
        let first = AuditEntry::new(AuditEvent::LoginSucceeded, None, None, Some("first".into()));
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

    fn entry_at(event: AuditEvent, ts: u64, detail: &str) -> AuditEntry {
        AuditEntry {
            ts,
            ..AuditEntry::new(event, None, None, Some(detail.to_owned()))
        }
    }

    /// Append enough maximum-size failed logins, with `ts` from `first_ts`, to
    /// rotate the failed-login file at least three times. Returns how many.
    fn flood_failed_logins(log: &AuditLog, first_ts: u64) -> u64 {
        let sample = entry_at(AuditEvent::LoginFailed, first_ts, &"f".repeat(MAX_DETAIL_CHARS));
        let line_len = serde_json::to_vec(&sample).unwrap().len() as u64 + 1;
        let count = 3 * AUDIT_MAX_BYTES / line_len + 1;
        for index in 0..count {
            let mut entry = sample.clone();
            entry.ts = first_ts + index;
            log.append(&entry).unwrap();
        }
        count
    }

    #[test]
    fn failed_login_flood_keeps_earlier_high_value_entries() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        log.append(&entry_at(AuditEvent::PowerAction, 1_000, "reboot"))
            .unwrap();
        log.append(&entry_at(AuditEvent::ConfigChanged, 1_001, "video"))
            .unwrap();

        let flood = flood_failed_logins(&log, 2_000);

        let all = log.read_latest(flood as usize + 10).unwrap();
        let events: Vec<&str> = all.iter().map(|entry| entry.event.as_str()).collect();
        assert_eq!(events[0], AuditEvent::LoginFailed.as_str());
        assert_eq!(
            events[events.len() - 2..],
            [
                AuditEvent::ConfigChanged.as_str(),
                AuditEvent::PowerAction.as_str()
            ],
            "earlier high-value entries must survive a failed-login flood"
        );
        assert!(
            all.len() < flood as usize,
            "failed logins themselves stay bounded by rotation"
        );
        assert!(
            directory.0.join(AUDIT_LOGIN_FAILURES_ROTATED_FILE_NAME).is_file(),
            "the flood must have rotated the failed-login file"
        );
    }

    #[test]
    fn limit_sized_read_merges_newest_of_both_kinds_in_order() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        log.append(&entry_at(AuditEvent::PowerAction, 1_000, "early"))
            .unwrap();
        let flood = flood_failed_logins(&log, 2_000);
        let after = 2_000 + flood;
        log.append(&entry_at(AuditEvent::ConfigChanged, after, "config"))
            .unwrap();
        // Same millisecond as the config change but appended later.
        log.append(&entry_at(AuditEvent::LoginFailed, after, "tie"))
            .unwrap();
        log.append(&entry_at(AuditEvent::LoginFailed, after + 1, "failed"))
            .unwrap();
        log.append(&entry_at(AuditEvent::PowerAction, after + 2, "late"))
            .unwrap();

        let newest = log.read_latest(5).unwrap();
        let summary: Vec<(&str, u64, Option<&str>)> = newest
            .iter()
            .map(|entry| (entry.event.as_str(), entry.ts, entry.detail.as_deref()))
            .collect();
        let flood_detail = "f".repeat(MAX_DETAIL_CHARS);
        assert_eq!(
            summary,
            [
                ("power_action", after + 2, Some("late")),
                ("login_failed", after + 1, Some("failed")),
                ("config_changed", after, Some("config")),
                ("login_failed", after, Some("tie")),
                ("login_failed", after - 1, Some(flood_detail.as_str())),
            ],
            "newest first by ts; on equal ts the audit.log entry comes first"
        );

        let page = log.read_latest(AUDIT_MAX_LIMIT).unwrap();
        assert_eq!(page.len(), AUDIT_MAX_LIMIT);
        assert!(page.windows(2).all(|pair| pair[0].ts >= pair[1].ts));
    }

    #[cfg(unix)]
    #[test]
    fn failed_login_files_are_private() {
        let directory = TestDirectory::new();
        let log = AuditLog::new(&directory.0);
        let active = directory.0.join(AUDIT_LOGIN_FAILURES_FILE_NAME);
        let rotated = directory.0.join(AUDIT_LOGIN_FAILURES_ROTATED_FILE_NAME);
        let failed = AuditEntry::new(AuditEvent::LoginFailed, None, None, None);
        log.append(&failed).unwrap();
        let mode = fs::metadata(&active).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        // A pre-existing, too-permissive file is tightened on the next append.
        fs::set_permissions(&active, fs::Permissions::from_mode(0o644)).unwrap();
        log.append(&failed).unwrap();
        let mode = fs::metadata(&active).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        flood_failed_logins(&log, 2_000);
        let mode = fs::metadata(&rotated).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let mode = fs::metadata(&active).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn audit_log_from_older_versions_stays_readable() {
        let directory = TestDirectory::new();
        // Older versions wrote every event, failed logins included, to audit.log.
        fs::write(
            directory.0.join(AUDIT_ROTATED_FILE_NAME),
            concat!(
                r#"{"ts":1700000000000,"event":"power_action","ip":"203.0.113.9","session":"abc","detail":"action=reset"}"#,
                "\n",
                r#"{"ts":1700000000001,"event":"login_failed","ip":"203.0.113.10","session":null,"detail":"username=admin"}"#,
                "\n",
            ),
        )
        .unwrap();
        let legacy_active = concat!(
            r#"{"ts":1700000000002,"event":"login_failed","ip":"2001:db8::1","session":null,"detail":null}"#,
            "\n",
            r#"{"ts":1700000000003,"event":"config_changed","ip":"203.0.113.9","session":"abc","detail":"video"}"#,
            "\n",
        );
        fs::write(directory.0.join(AUDIT_FILE_NAME), legacy_active).unwrap();

        let log = AuditLog::new(&directory.0);
        log.append(&AuditEntry::new(AuditEvent::LoginFailed, None, None, Some("new".into())))
            .unwrap();
        log.append(&AuditEntry::new(AuditEvent::Logout, None, None, Some("new".into())))
            .unwrap();

        let entries = log.read_latest(10).unwrap();
        let events: Vec<(&str, u64)> = entries
            .iter()
            .map(|entry| (entry.event.as_str(), entry.ts))
            .collect();
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].0, "logout");
        assert_eq!(events[1].0, "login_failed");
        assert_eq!(
            events[2..],
            [
                ("config_changed", 1_700_000_000_003),
                ("login_failed", 1_700_000_000_002),
                ("login_failed", 1_700_000_000_001),
                ("power_action", 1_700_000_000_000),
            ]
        );
        assert_eq!(entries[5].ip, Some("203.0.113.9".parse().unwrap()));

        // The legacy file is appended to, never rewritten, and new failed logins
        // go to their own file.
        let active = fs::read_to_string(directory.0.join(AUDIT_FILE_NAME)).unwrap();
        assert!(active.starts_with(legacy_active));
        assert!(!active[legacy_active.len()..].contains("login_failed"));
        let failures =
            fs::read_to_string(directory.0.join(AUDIT_LOGIN_FAILURES_FILE_NAME)).unwrap();
        assert_eq!(failures.lines().count(), 1);
    }
}
