use std::{
    collections::HashMap,
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub const AUTH_VERSION: u32 = 1;
pub const STATE_DIR_ENV: &str = "WINGMANKVM_STATE_DIR";
pub const DEFAULT_STATE_DIR: &str = "/var/lib/wingmankvm";
pub const AUTH_FILE_NAME: &str = "auth.json";
pub const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const SESSION_TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordPolicy {
    pub minimum_characters: usize,
    pub maximum_bytes: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_number: bool,
    pub require_symbol: bool,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            minimum_characters: 12,
            maximum_bytes: 1_024,
            require_uppercase: true,
            require_lowercase: true,
            require_number: true,
            require_symbol: true,
        }
    }
}

impl PasswordPolicy {
    pub fn validate(&self, password: &str) -> Result<(), PasswordPolicyError> {
        if password.len() > self.maximum_bytes {
            return Err(PasswordPolicyError::TooLong {
                maximum_bytes: self.maximum_bytes,
            });
        }
        if password.chars().count() < self.minimum_characters {
            return Err(PasswordPolicyError::TooShort {
                minimum_characters: self.minimum_characters,
            });
        }
        if password.chars().any(char::is_control) {
            return Err(PasswordPolicyError::ControlCharacter);
        }
        if self.require_uppercase && !password.chars().any(char::is_uppercase) {
            return Err(PasswordPolicyError::MissingUppercase);
        }
        if self.require_lowercase && !password.chars().any(char::is_lowercase) {
            return Err(PasswordPolicyError::MissingLowercase);
        }
        if self.require_number && !password.chars().any(char::is_numeric) {
            return Err(PasswordPolicyError::MissingNumber);
        }
        if self.require_symbol
            && !password
                .chars()
                .any(|character| !character.is_alphanumeric() && !character.is_whitespace())
        {
            return Err(PasswordPolicyError::MissingSymbol);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PasswordPolicyError {
    #[error("password must contain at least {minimum_characters} characters")]
    TooShort { minimum_characters: usize },
    #[error("password must not exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },
    #[error("password must contain an uppercase character")]
    MissingUppercase,
    #[error("password must contain a lowercase character")]
    MissingLowercase,
    #[error("password must contain a number")]
    MissingNumber,
    #[error("password must contain a symbol")]
    MissingSymbol,
    #[error("password must not contain control characters")]
    ControlCharacter,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AdminPasswordHash(String);

impl AdminPasswordHash {
    pub fn new(password: &str) -> Result<Self, AuthError> {
        Self::new_with_policy(password, &PasswordPolicy::default())
    }

    pub fn new_with_policy(password: &str, policy: &PasswordPolicy) -> Result<Self, AuthError> {
        policy.validate(password)?;
        let mut salt_bytes = [0_u8; 16];
        getrandom::fill(&mut salt_bytes)
            .map_err(|error| AuthError::Randomness(error.to_string()))?;
        let salt = SaltString::encode_b64(&salt_bytes)
            .map_err(|error| AuthError::PasswordHash(error.to_string()))?;
        let encoded = argon2id()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|error| AuthError::PasswordHash(error.to_string()))?
            .to_string();
        Ok(Self(encoded))
    }

    pub fn from_encoded(encoded: impl Into<String>) -> Result<Self, AuthError> {
        let encoded = encoded.into();
        let parsed = PasswordHash::new(&encoded)
            .map_err(|error| AuthError::InvalidPasswordHash(error.to_string()))?;
        if parsed.algorithm.as_str() != "argon2id" {
            return Err(AuthError::InvalidPasswordHash(
                "expected an Argon2id password hash".to_owned(),
            ));
        }
        Ok(Self(encoded))
    }

    pub fn verify(&self, password: &str) -> Result<bool, AuthError> {
        let parsed = PasswordHash::new(&self.0)
            .map_err(|error| AuthError::InvalidPasswordHash(error.to_string()))?;
        match argon2id().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(error) => Err(AuthError::PasswordHash(error.to_string())),
        }
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AdminPasswordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminPasswordHash([REDACTED])")
    }
}

impl Serialize for AdminPasswordHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AdminPasswordHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        Self::from_encoded(encoded).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRecord {
    pub version: u32,
    pub username: String,
    pub password_hash: AdminPasswordHash,
    pub created_at_unix_seconds: u64,
    pub updated_at_unix_seconds: u64,
}

impl AuthRecord {
    pub fn new(username: impl Into<String>, password: &str) -> Result<Self, AuthError> {
        let username = validate_username(username.into())?;
        let now = unix_time_now()?;
        Ok(Self {
            version: AUTH_VERSION,
            username,
            password_hash: AdminPasswordHash::new(password)?,
            created_at_unix_seconds: now,
            updated_at_unix_seconds: now,
        })
    }

    pub fn verify_credentials(&self, username: &str, password: &str) -> Result<bool, AuthError> {
        let password_matches = self.password_hash.verify(password)?;
        Ok(self.username == username && password_matches)
    }

    #[allow(dead_code)]
    pub fn set_password(&mut self, password: &str) -> Result<(), AuthError> {
        self.password_hash = AdminPasswordHash::new(password)?;
        self.updated_at_unix_seconds = unix_time_now()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_username(&mut self, username: impl Into<String>) -> Result<(), AuthError> {
        self.username = validate_username(username.into())?;
        self.updated_at_unix_seconds = unix_time_now()?;
        Ok(())
    }

    fn validate(&self) -> Result<(), AuthError> {
        if self.version != AUTH_VERSION {
            return Err(AuthError::UnsupportedVersion {
                found: self.version,
                supported: AUTH_VERSION,
            });
        }
        validate_username_value(&self.username)?;
        if self.updated_at_unix_seconds < self.created_at_unix_seconds {
            return Err(AuthError::InvalidRecord(
                "updated_at_unix_seconds precedes created_at_unix_seconds".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AuthStore {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl AuthStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn at_default_path() -> Self {
        Self::new(default_auth_path())
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_initialized(&self) -> Result<bool, AuthError> {
        match fs::metadata(&self.path) {
            Ok(metadata) => Ok(metadata.is_file()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(AuthError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }

    pub fn load(&self) -> Result<Option<AuthRecord>, AuthError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(AuthError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        let record: AuthRecord =
            serde_json::from_slice(&bytes).map_err(|source| AuthError::Json {
                path: self.path.clone(),
                source,
            })?;
        record.validate()?;
        Ok(Some(record))
    }

    #[allow(dead_code)]
    pub fn save(&self, record: &AuthRecord) -> Result<(), AuthError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.save_unlocked(record)
    }

    fn save_unlocked(&self, record: &AuthRecord) -> Result<(), AuthError> {
        record.validate()?;
        let mut contents = serde_json::to_vec_pretty(record).map_err(AuthError::Serialize)?;
        contents.push(b'\n');
        atomic_write(&self.path, &contents)
    }

    #[allow(dead_code)]
    pub fn initialize(
        &self,
        username: impl Into<String>,
        password: &str,
    ) -> Result<AuthRecord, AuthError> {
        let record = AuthRecord::new(username, password)?;
        self.initialize_record(record)
    }

    pub fn initialize_record(&self, record: AuthRecord) -> Result<AuthRecord, AuthError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.is_initialized()? {
            return Err(AuthError::AlreadyInitialized);
        }
        self.save_unlocked(&record)?;
        Ok(record)
    }
}

impl Default for AuthStore {
    fn default() -> Self {
        Self::at_default_path()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionInfo {
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
}

#[derive(Clone)]
pub struct SessionStore {
    ttl: Duration,
    sessions: Arc<RwLock<HashMap<String, StoredSession>>>,
}

#[derive(Debug, Clone, Copy)]
struct StoredSession {
    info: SessionInfo,
    expires_at: Instant,
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create(&self) -> Result<String, AuthError> {
        let mut token_bytes = [0_u8; SESSION_TOKEN_BYTES];
        getrandom::fill(&mut token_bytes)
            .map_err(|error| AuthError::Randomness(error.to_string()))?;
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        let created_at = SystemTime::now();
        let expires_at_system = created_at
            .checked_add(self.ttl)
            .ok_or(AuthError::TimeOverflow)?;
        let expires_at = Instant::now()
            .checked_add(self.ttl)
            .ok_or(AuthError::TimeOverflow)?;
        let session = StoredSession {
            info: SessionInfo {
                created_at,
                expires_at: expires_at_system,
            },
            expires_at,
        };
        write_sessions(&self.sessions).insert(token.clone(), session);
        Ok(token)
    }

    pub fn validate(&self, token: &str) -> Option<SessionInfo> {
        let now = Instant::now();
        let mut sessions = write_sessions(&self.sessions);
        match sessions.get(token).copied() {
            Some(session) if session.expires_at > now => Some(session.info),
            Some(_) => {
                sessions.remove(token);
                None
            }
            None => None,
        }
    }

    pub fn revoke(&self, token: &str) -> bool {
        write_sessions(&self.sessions).remove(token).is_some()
    }

    #[allow(dead_code)]
    pub fn revoke_all(&self) {
        write_sessions(&self.sessions).clear();
    }

    pub fn prune_expired(&self) -> usize {
        let now = Instant::now();
        let mut sessions = write_sessions(&self.sessions);
        let before = sessions.len();
        sessions.retain(|_, session| session.expires_at > now);
        before - sessions.len()
    }

    pub fn len(&self) -> usize {
        read_sessions(&self.sessions).len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_SESSION_TTL)
    }
}

impl fmt::Debug for SessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionStore")
            .field("ttl", &self.ttl)
            .field("session_count", &self.len())
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    WeakPassword(#[from] PasswordPolicyError),
    #[error("username must contain between 3 and 64 ASCII letters, numbers, '.', '_' or '-'")]
    InvalidUsername,
    #[error("failed to obtain secure randomness: {0}")]
    Randomness(String),
    #[error("password hashing failed: {0}")]
    PasswordHash(String),
    #[error("invalid stored password hash: {0}")]
    InvalidPasswordHash(String),
    #[error("authentication I/O failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid authentication JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize authentication record: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("unsupported authentication version {found}; this build supports version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("administrator credentials have already been initialized")]
    AlreadyInitialized,
    #[error("invalid authentication record: {0}")]
    InvalidRecord(String),
    #[error("system time is before the Unix epoch")]
    InvalidSystemTime,
    #[error("session expiration time overflowed")]
    TimeOverflow,
}

pub fn default_auth_path() -> PathBuf {
    let state_dir = env::var_os(STATE_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_DIR));
    state_dir.join(AUTH_FILE_NAME)
}

fn argon2id() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

fn validate_username(username: String) -> Result<String, AuthError> {
    validate_username_value(&username)?;
    Ok(username)
}

fn validate_username_value(username: &str) -> Result<(), AuthError> {
    ((3..=64).contains(&username.len())
        && username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then_some(())
    .ok_or(AuthError::InvalidUsername)
}

fn unix_time_now() -> Result<u64, AuthError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AuthError::InvalidSystemTime)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), AuthError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| AuthError::Io {
        path: parent.to_owned(),
        source,
    })?;
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).map_err(|error| AuthError::Randomness(error.to_string()))?;
    let suffix = u64::from_ne_bytes(random);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(AUTH_FILE_NAME);
    let temporary_path = parent.join(format!(".{file_name}.{suffix:016x}.tmp"));

    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&temporary_path)
            .map_err(|source| AuthError::Io {
                path: temporary_path.clone(),
                source,
            })?;
        file.write_all(contents).map_err(|source| AuthError::Io {
            path: temporary_path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| AuthError::Io {
            path: temporary_path.clone(),
            source,
        })?;
        drop(file);

        #[cfg(unix)]
        fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o600)).map_err(
            |source| AuthError::Io {
                path: temporary_path.clone(),
                source,
            },
        )?;

        fs::rename(&temporary_path, path).map_err(|source| AuthError::Io {
            path: path.to_owned(),
            source,
        })?;
        let directory = File::open(parent).map_err(|source| AuthError::Io {
            path: parent.to_owned(),
            source,
        })?;
        directory.sync_all().map_err(|source| AuthError::Io {
            path: parent.to_owned(),
            source,
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn read_sessions(
    sessions: &RwLock<HashMap<String, StoredSession>>,
) -> std::sync::RwLockReadGuard<'_, HashMap<String, StoredSession>> {
    sessions
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_sessions(
    sessions: &RwLock<HashMap<String, StoredSession>>,
) -> std::sync::RwLockWriteGuard<'_, HashMap<String, StoredSession>> {
    sessions
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let mut random = [0_u8; 8];
            getrandom::fill(&mut random).unwrap();
            let path = env::temp_dir().join(format!(
                "wingmankvm-auth-test-{}-{:016x}",
                std::process::id(),
                u64::from_ne_bytes(random)
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
    fn password_policy_rejects_each_missing_character_class() {
        let policy = PasswordPolicy::default();
        assert!(matches!(
            policy.validate("Short1!"),
            Err(PasswordPolicyError::TooShort { .. })
        ));
        assert_eq!(
            policy.validate("lowercase123!"),
            Err(PasswordPolicyError::MissingUppercase)
        );
        assert_eq!(
            policy.validate("UPPERCASE123!"),
            Err(PasswordPolicyError::MissingLowercase)
        );
        assert_eq!(
            policy.validate("NoNumbersHere!"),
            Err(PasswordPolicyError::MissingNumber)
        );
        assert_eq!(
            policy.validate("NoSymbolsHere1"),
            Err(PasswordPolicyError::MissingSymbol)
        );
        assert!(policy.validate("Correct-Horse7!").is_ok());
    }

    #[test]
    fn password_hash_is_argon2id_and_verifies_passwords() {
        let password_hash = AdminPasswordHash::new("Correct-Horse7!").unwrap();
        assert!(password_hash.as_str().starts_with("$argon2id$v=19$"));
        assert!(password_hash.verify("Correct-Horse7!").unwrap());
        assert!(!password_hash.verify("Wrong-Horse99!").unwrap());
        assert!(!format!("{password_hash:?}").contains(password_hash.as_str()));
    }

    #[test]
    fn auth_store_persists_credentials_and_refuses_reinitialization() {
        let directory = TestDirectory::new();
        let store = AuthStore::new(directory.0.join("nested/auth.json"));
        assert!(!store.is_initialized().unwrap());

        store.initialize("admin", "Correct-Horse7!").unwrap();
        assert!(store.is_initialized().unwrap());
        let loaded = store.load().unwrap().unwrap();
        assert!(
            loaded
                .verify_credentials("admin", "Correct-Horse7!")
                .unwrap()
        );
        assert!(
            !loaded
                .verify_credentials("other", "Correct-Horse7!")
                .unwrap()
        );
        assert!(matches!(
            store.initialize("admin", "Another-Good8!"),
            Err(AuthError::AlreadyInitialized)
        ));
    }

    #[test]
    fn sessions_are_random_revocable_and_expire() {
        let sessions = SessionStore::new(Duration::from_secs(60));
        let first = sessions.create().unwrap();
        let second = sessions.create().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.len(), 43);
        assert!(sessions.validate(&first).is_some());
        assert!(sessions.revoke(&first));
        assert!(sessions.validate(&first).is_none());

        let expired = SessionStore::new(Duration::ZERO);
        let token = expired.create().unwrap();
        assert!(expired.validate(&token).is_none());
        assert!(expired.is_empty());
    }

    #[test]
    fn auth_record_rejects_invalid_username_and_weak_password() {
        assert!(matches!(
            AuthRecord::new("bad user", "Correct-Horse7!"),
            Err(AuthError::InvalidUsername)
        ));
        assert!(matches!(
            AuthRecord::new("admin", "weak"),
            Err(AuthError::WeakPassword(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn persisted_auth_record_is_private() {
        let directory = TestDirectory::new();
        let store = AuthStore::new(directory.0.join("auth.json"));
        store.initialize("admin", "Correct-Horse7!").unwrap();
        let mode = fs::metadata(store.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
