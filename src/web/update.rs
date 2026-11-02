//! Online upgrade from GitHub Releases (#91).
//!
//! The service stays unprivileged. It may only:
//! - read the latest release of the fixed repository to show what is new;
//! - run `sudo -n /usr/local/sbin/wingmankvm-online-upgrade --start`, the one
//!   sudoers rule with that exact argument list, which starts a root oneshot
//!   unit taking no input from the service;
//! - read the root-written status file to report progress.
//!
//! The root unit resolves the latest release on its own, downloads the
//! archive for its CPU, verifies SHA256SUMS and runs the transactional
//! installer, which rolls back on failure. Nothing the browser sends reaches
//! it: the requested version is only compared against a fresh lookup here so
//! the administrator confirms the version they were shown.

use super::*;

pub(super) const RELEASE_REPO: &str = "XiNian-dada/WingmanKVM";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/XiNian-dada/WingmanKVM/releases/latest";
const UPGRADE_HELPER: &str = "/usr/local/sbin/wingmankvm-online-upgrade";
const UPGRADE_STATUS: &str = "/run/wingmankvm-online-upgrade/status";
const MAX_RELEASE_JSON_BYTES: usize = 1024 * 1024;
const MAX_NOTES_CHARS: usize = 4000;
const MAX_STATUS_BYTES: u64 = 4096;
const RELEASE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const TRIGGER_DEADLINE: Duration = Duration::from_secs(15);
/// The unit times out after 20 minutes; a `running` status older than this
/// was left by a killed run.
const STALE_RUNNING_SECS: u64 = 25 * 60;
/// Exit status of `--start` when the unit is already active.
const HELPER_BUSY_EXIT: i32 = 3;

/// A plain `vX.Y.Z` / `X.Y.Z[-pre]` version. Pre-releases sort before the
/// release with the same core version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Version {
    core: (u32, u32, u32),
    release: bool,
}

impl Version {
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        if let Some(pre) = pre
            && (pre.is_empty()
                || !pre
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.'))
        {
            return None;
        }
        let mut parts = core.split('.');
        let mut next = || -> Option<u32> {
            let part = parts.next()?;
            if part.is_empty() || part.len() > 6 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let core = (next()?, next()?, next()?);
        if parts.next().is_some() {
            return None;
        }
        Some(Self { core, release: pre.is_none() })
    }
}

/// Only plain release tags can be installed; this mirrors `valid_tag` in
/// deploy/wingmankvm-online-upgrade.
pub(super) fn installable_tag(tag: &str) -> bool {
    tag.starts_with('v') && Version::parse(tag).is_some_and(|version| version.release)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ReleaseInfo {
    pub(super) tag: String,
    pub(super) name: String,
    pub(super) published_at: Option<String>,
    /// Plain text release notes (CHANGELOG excerpt), truncated.
    pub(super) notes: String,
    pub(super) notes_truncated: bool,
    /// Built from the fixed repository, never taken from the API response.
    pub(super) url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

fn clean_text(text: &str, max_chars: usize) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for (count, ch) in text.chars().filter(|ch| *ch != '\r').enumerate() {
        if count >= max_chars {
            truncated = true;
            break;
        }
        out.push(if ch.is_control() && ch != '\n' && ch != '\t' { ' ' } else { ch });
    }
    (out, truncated)
}

pub(super) fn parse_release(body: &[u8]) -> Result<ReleaseInfo, String> {
    let release: GithubRelease =
        serde_json::from_slice(body).map_err(|_| "Release 信息格式无效".to_owned())?;
    if release.draft || release.prerelease || !installable_tag(&release.tag_name) {
        return Err("最新 Release 不是可安装的正式版本".to_owned());
    }
    let (notes, notes_truncated) = clean_text(release.body.as_deref().unwrap_or(""), MAX_NOTES_CHARS);
    let (name, _) = clean_text(release.name.as_deref().unwrap_or(&release.tag_name), 200);
    let published_at = release
        .published_at
        .filter(|value| value.len() <= 40 && value.bytes().all(|b| b.is_ascii_graphic()));
    Ok(ReleaseInfo {
        url: format!("https://github.com/{RELEASE_REPO}/releases/tag/{}", release.tag_name),
        tag: release.tag_name,
        name: name.lines().next().unwrap_or("").to_owned(),
        published_at,
        notes,
        notes_truncated,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(super) struct UpgradeStatus {
    /// idle | running | succeeded | failed
    pub(super) state: String,
    pub(super) phase: String,
    pub(super) version: String,
    pub(super) error: String,
    #[serde(skip)]
    pub(super) id: String,
    pub(super) updated: u64,
}

fn status_token(value: &str, max: usize) -> String {
    if value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        value.to_owned()
    } else {
        String::new()
    }
}

/// Parse the root-written key=value status. Unknown keys and malformed values
/// are dropped, so the file cannot inject arbitrary text into the UI.
pub(super) fn parse_status(text: &str, now: u64) -> UpgradeStatus {
    let mut status = UpgradeStatus::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "state" => status.state = status_token(value, 16),
            "phase" => status.phase = status_token(value, 16),
            "version" => status.version = status_token(value, 32),
            "error" => status.error = status_token(value, 32),
            "id" => status.id = status_token(value, 40),
            "updated" => status.updated = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    if !matches!(status.state.as_str(), "running" | "succeeded" | "failed") {
        return UpgradeStatus { state: "idle".to_owned(), ..UpgradeStatus::default() };
    }
    if status.state == "running" && now.saturating_sub(status.updated) > STALE_RUNNING_SECS {
        status.state = "failed".to_owned();
        status.error = "stale".to_owned();
    }
    status
}

#[derive(Clone)]
pub(super) struct OnlineUpdate {
    inner: Arc<OnlineUpdateInner>,
}

struct OnlineUpdateInner {
    api_url: String,
    sudo: PathBuf,
    helper: PathBuf,
    status_path: PathBuf,
    audited_path: PathBuf,
    current: String,
    client: reqwest::Client,
    apply: AsyncMutex<()>,
    audit_outcome: AsyncMutex<()>,
    cache: Mutex<Option<(Instant, ReleaseInfo)>>,
}

pub(super) struct OnlineUpdateOptions {
    pub(super) api_url: String,
    pub(super) sudo: PathBuf,
    pub(super) helper: PathBuf,
    pub(super) status_path: PathBuf,
    pub(super) state_dir: PathBuf,
    pub(super) current: String,
}

impl OnlineUpdateOptions {
    pub(super) fn system(state_dir: PathBuf) -> Self {
        Self {
            api_url: LATEST_RELEASE_API.to_owned(),
            sudo: PathBuf::from("sudo"),
            helper: PathBuf::from(UPGRADE_HELPER),
            status_path: PathBuf::from(UPGRADE_STATUS),
            state_dir,
            current: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum UpdateError {
    #[error("此安装不支持在线升级：缺少 {0}，请先用安装脚本升级一次")]
    Unsupported(String),
    #[error("已有在线升级正在进行")]
    Busy,
    #[error("无法获取最新版本：{0}")]
    Lookup(String),
    #[error("所选版本 {requested} 不是最新正式版本 {latest}，请重新检查更新")]
    NotLatest { requested: String, latest: String },
    #[error("当前版本 {current} 已是最新，无需升级")]
    NotNewer { current: String },
    #[error("无法启动升级任务")]
    Trigger,
}

impl UpdateError {
    fn status(&self) -> StatusCode {
        match self {
            Self::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            Self::Busy | Self::NotLatest { .. } | Self::NotNewer { .. } => StatusCode::CONFLICT,
            Self::Lookup(_) => StatusCode::BAD_GATEWAY,
            Self::Trigger => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<UpdateError> for ApiError {
    fn from(error: UpdateError) -> Self {
        ApiError::new(error.status(), error.to_string())
    }
}

impl OnlineUpdate {
    pub(super) fn new(options: OnlineUpdateOptions) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("WingmanKVM/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(OnlineUpdateInner {
                api_url: options.api_url,
                sudo: options.sudo,
                helper: options.helper,
                status_path: options.status_path,
                audited_path: options.state_dir.join("online-update-audited"),
                current: options.current,
                client,
                apply: AsyncMutex::new(()),
                audit_outcome: AsyncMutex::new(()),
                cache: Mutex::new(None),
            }),
        }
    }

    pub(super) fn current(&self) -> &str {
        &self.inner.current
    }

    pub(super) fn supported(&self) -> bool {
        self.inner.helper.is_file()
    }

    pub(super) async fn status(&self) -> UpgradeStatus {
        let path = self.inner.status_path.clone();
        let text = tokio::task::spawn_blocking(move || -> Option<String> {
            let file = std::fs::File::open(&path).ok()?;
            let mut text = String::new();
            file.take(MAX_STATUS_BYTES).read_to_string(&mut text).ok()?;
            Some(text)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
        parse_status(&text, crate::audit::unix_millis_now() / 1000)
    }

    pub(super) async fn latest(&self, fresh: bool) -> Result<ReleaseInfo, UpdateError> {
        if !fresh
            && let Some((at, release)) = self.inner.cache.lock().unwrap().as_ref()
            && at.elapsed() < RELEASE_CACHE_TTL
        {
            return Ok(release.clone());
        }
        let mut response = self
            .inner
            .client
            .get(&self.inner.api_url)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|_| UpdateError::Lookup("网络请求失败".to_owned()))?;
        if !response.status().is_success() {
            return Err(UpdateError::Lookup(format!("HTTP {}", response.status().as_u16())));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| UpdateError::Lookup("读取响应失败".to_owned()))?
        {
            if body.len() + chunk.len() > MAX_RELEASE_JSON_BYTES {
                return Err(UpdateError::Lookup("响应过大".to_owned()));
            }
            body.extend_from_slice(&chunk);
        }
        let release = parse_release(&body).map_err(UpdateError::Lookup)?;
        *self.inner.cache.lock().unwrap() = Some((Instant::now(), release.clone()));
        Ok(release)
    }

    pub(super) fn is_newer(&self, tag: &str) -> bool {
        match (Version::parse(tag), Version::parse(&self.inner.current)) {
            (Some(latest), Some(current)) => latest > current,
            _ => false,
        }
    }

    /// Validate the confirmed version and start the root unit. Holding the
    /// mutex makes concurrent requests single-flight inside this process; the
    /// helper, systemd and the installer lock enforce it across restarts.
    pub(super) async fn apply(&self, requested: &str) -> Result<ReleaseInfo, UpdateError> {
        if !self.supported() {
            return Err(UpdateError::Unsupported(self.inner.helper.display().to_string()));
        }
        let Ok(_guard) = self.inner.apply.try_lock() else {
            return Err(UpdateError::Busy);
        };
        if self.status().await.state == "running" {
            return Err(UpdateError::Busy);
        }
        let latest = self.latest(true).await?;
        if requested != latest.tag {
            return Err(UpdateError::NotLatest {
                requested: status_token(requested, 32),
                latest: latest.tag,
            });
        }
        if !self.is_newer(&latest.tag) {
            return Err(UpdateError::NotNewer { current: self.inner.current.clone() });
        }
        self.trigger().await?;
        Ok(latest)
    }

    async fn trigger(&self) -> Result<(), UpdateError> {
        let mut command = tokio::process::Command::new(&self.inner.sudo);
        command
            .arg("-n")
            .arg(&self.inner.helper)
            .arg("--start")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let status = tokio::time::timeout(TRIGGER_DEADLINE, command.status())
            .await
            .map_err(|_| UpdateError::Trigger)?
            .map_err(|_| UpdateError::Trigger)?;
        match status.code() {
            Some(0) => Ok(()),
            Some(HELPER_BUSY_EXIT) => Err(UpdateError::Busy),
            _ => Err(UpdateError::Trigger),
        }
    }

    /// Record the outcome of a finished run once. The result is usually read
    /// by the restarted service, so the run id is remembered in the state
    /// directory rather than in memory.
    pub(super) async fn audit_outcome(&self, audit: &AuditLog, status: &UpgradeStatus) {
        let event = match status.state.as_str() {
            "succeeded" => AuditEvent::UpdateSucceeded,
            "failed" => AuditEvent::UpdateFailed,
            _ => return,
        };
        if status.id.is_empty() {
            return;
        }
        let _guard = self.inner.audit_outcome.lock().await;
        let path = self.inner.audited_path.clone();
        let seen = fs::read_to_string(&path).await.unwrap_or_default();
        if seen.trim() == status.id {
            return;
        }
        if fs::write(&path, format!("{}\n", status.id)).await.is_err() {
            return;
        }
        let mut detail = format!("version={}", status.version);
        if !status.error.is_empty() {
            detail.push_str(&format!(" error={}", status.error));
        }
        audit.record(event, None, None, Some(detail)).await;
    }
}

pub(super) async fn update_status(State(state): State<AppState>) -> Response {
    let status = state.update.status().await;
    state.update.audit_outcome(&state.audit, &status).await;
    no_store(Json(json!({
        "current_version": state.update.current(),
        "supported": state.update.supported(),
        "status": status,
    })))
}

pub(super) async fn update_check(State(state): State<AppState>) -> Result<Response, ApiError> {
    let latest = state.update.latest(false).await?;
    Ok(no_store(Json(json!({
        "current_version": state.update.current(),
        "supported": state.update.supported(),
        "update_available": state.update.is_newer(&latest.tag),
        "latest": latest,
    }))))
}

#[derive(Deserialize)]
pub(super) struct UpdateApplyRequest {
    version: String,
}

pub(super) async fn update_apply(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<UpdateApplyRequest>,
) -> Result<Response, ApiError> {
    let (ip, session) = audit_actor(&state, connection, &headers).await;
    let from = state.update.current().to_owned();
    match state.update.apply(&request.version).await {
        Ok(release) => {
            state
                .audit
                .record(
                    AuditEvent::UpdateStarted,
                    ip,
                    session,
                    Some(format!("from={from} to={}", release.tag)),
                )
                .await;
            Ok((
                StatusCode::ACCEPTED,
                no_store(Json(json!({ "ok": true, "version": release.tag }))),
            )
                .into_response())
        }
        Err(error) => {
            if matches!(error, UpdateError::Trigger) {
                state
                    .audit
                    .record(
                        AuditEvent::UpdateFailed,
                        ip,
                        session,
                        Some(format!("from={from} error=trigger")),
                    )
                    .await;
            }
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_semantically() {
        let v = |text| Version::parse(text).unwrap();
        assert!(v("v0.10.0") > v("0.9.9"));
        assert!(v("v1.0.0") > v("1.0.0-rc.1"));
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        for bad in ["", "v1", "1.2", "1.2.3.4", "v1.2.x", "1.2.3-", "1.2.3-a b", "v1.2.3;rm", "01234567.0.0"] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn only_plain_v_tags_are_installable() {
        assert!(installable_tag("v0.2.0"));
        for bad in ["0.2.0", "v0.2.0-rc.1", "v0.2.0/../x", "v0.2", "https://evil/v1.0.0"] {
            assert!(!installable_tag(bad), "{bad}");
        }
    }

    #[test]
    fn release_json_is_sanitized_and_url_is_fixed() {
        let body = json!({
            "tag_name": "v0.2.0",
            "name": "WingmanKVM 0.2.0\nsecond line",
            "body": "## Added\r\n- online upgrade\u{1b}[31m",
            "published_at": "2026-11-01T00:00:00Z",
            "html_url": "https://evil.example/",
        });
        let release = parse_release(body.to_string().as_bytes()).unwrap();
        assert_eq!(release.tag, "v0.2.0");
        assert_eq!(release.name, "WingmanKVM 0.2.0");
        assert_eq!(release.notes, "## Added\n- online upgrade [31m");
        assert_eq!(release.url, "https://github.com/XiNian-dada/WingmanKVM/releases/tag/v0.2.0");

        let long = json!({ "tag_name": "v0.2.0", "body": "x".repeat(MAX_NOTES_CHARS + 5) });
        let release = parse_release(long.to_string().as_bytes()).unwrap();
        assert!(release.notes_truncated);
        assert_eq!(release.notes.chars().count(), MAX_NOTES_CHARS);

        for bad in [
            json!({ "tag_name": "v0.2.0-rc.1" }),
            json!({ "tag_name": "v0.2.0", "prerelease": true }),
            json!({ "tag_name": "v0.2.0", "draft": true }),
            json!({ "tag_name": "v0.2.0 && id" }),
        ] {
            assert!(parse_release(bad.to_string().as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn status_file_is_parsed_strictly() {
        let now = 10_000;
        let status = parse_status(
            "state=running\nphase=downloading\nversion=v0.2.0\nerror=\nid=99-1\nupdated=9990\nextra=<script>\n",
            now,
        );
        assert_eq!(status.state, "running");
        assert_eq!(status.phase, "downloading");
        assert_eq!(status.version, "v0.2.0");
        assert_eq!(status.id, "99-1");

        let injected = parse_status("state=failed\nerror=<img src=x>\nversion=v1 0\n", now);
        assert_eq!(injected.state, "failed");
        assert_eq!(injected.error, "");
        assert_eq!(injected.version, "");

        assert_eq!(parse_status("", now).state, "idle");
        assert_eq!(parse_status("state=bogus\n", now).state, "idle");

        let stale = parse_status(&format!("state=running\nupdated={}\n", now - STALE_RUNNING_SECS - 1), now);
        assert_eq!((stale.state.as_str(), stale.error.as_str()), ("failed", "stale"));
    }

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "wingmankvm-update-{name}-{}-{}",
                std::process::id(),
                crate::audit::unix_millis_now()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        /// A fake `sudo` that logs its arguments and exits with `code`.
        #[cfg(unix)]
        fn sudo(&self, code: i32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.join("sudo");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >>'{}'\nexit {code}\n",
                    self.dir.join("sudo.log").display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        fn update(&self, api_url: String, sudo: PathBuf, current: &str) -> OnlineUpdate {
            let helper = self.dir.join("wingmankvm-online-upgrade");
            std::fs::write(&helper, "#!/bin/sh\n").unwrap();
            OnlineUpdate::new(OnlineUpdateOptions {
                api_url,
                sudo,
                helper,
                status_path: self.dir.join("status"),
                state_dir: self.dir.clone(),
                current: current.to_owned(),
            })
        }

        fn sudo_log(&self) -> String {
            std::fs::read_to_string(self.dir.join("sudo.log")).unwrap_or_default()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Serve one fixed JSON body to any number of requests.
    async fn release_server(body: serde_json::Value) -> String {
        let app = Router::new().route(
            "/latest",
            get(move || {
                let body = body.clone();
                async move { Json(body) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{address}/latest")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn apply_triggers_only_the_fixed_helper_for_the_latest_newer_release() {
        let fixture = Fixture::new("apply");
        let api = release_server(json!({ "tag_name": "v0.2.0", "body": "notes" })).await;
        let update = fixture.update(api, fixture.sudo(0), "0.1.0");

        let error = update.apply("v0.1.5").await.unwrap_err();
        assert!(matches!(error, UpdateError::NotLatest { .. }), "{error}");
        let error = update.apply("--run").await.unwrap_err();
        assert!(matches!(error, UpdateError::NotLatest { .. }), "{error}");
        assert_eq!(fixture.sudo_log(), "", "rejected requests never reach sudo");

        let release = update.apply("v0.2.0").await.unwrap();
        assert_eq!(release.tag, "v0.2.0");
        let helper = fixture.dir.join("wingmankvm-online-upgrade");
        assert_eq!(fixture.sudo_log(), format!("-n {} --start\n", helper.display()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn apply_refuses_downgrades_busy_runs_and_missing_helper() {
        let fixture = Fixture::new("refuse");
        let api = release_server(json!({ "tag_name": "v0.2.0" })).await;

        let current = fixture.update(api.clone(), fixture.sudo(0), "0.2.0");
        let error = current.apply("v0.2.0").await.unwrap_err();
        assert!(matches!(error, UpdateError::NotNewer { .. }), "{error}");

        let update = fixture.update(api.clone(), fixture.sudo(0), "0.1.0");
        std::fs::write(
            fixture.dir.join("status"),
            format!("state=running\nupdated={}\n", crate::audit::unix_millis_now() / 1000),
        )
        .unwrap();
        assert!(matches!(update.apply("v0.2.0").await.unwrap_err(), UpdateError::Busy));
        std::fs::remove_file(fixture.dir.join("status")).unwrap();

        let busy = fixture.update(api.clone(), fixture.sudo(HELPER_BUSY_EXIT), "0.1.0");
        assert!(matches!(busy.apply("v0.2.0").await.unwrap_err(), UpdateError::Busy));
        let broken = fixture.update(api.clone(), fixture.sudo(1), "0.1.0");
        assert!(matches!(broken.apply("v0.2.0").await.unwrap_err(), UpdateError::Trigger));

        std::fs::remove_file(fixture.dir.join("wingmankvm-online-upgrade")).unwrap();
        let unsupported = OnlineUpdate::new(OnlineUpdateOptions {
            api_url: api,
            sudo: fixture.sudo(0),
            helper: fixture.dir.join("missing-helper"),
            status_path: fixture.dir.join("status"),
            state_dir: fixture.dir.clone(),
            current: "0.1.0".to_owned(),
        });
        assert!(matches!(
            unsupported.apply("v0.2.0").await.unwrap_err(),
            UpdateError::Unsupported(_)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn concurrent_applies_are_single_flight() {
        let fixture = Fixture::new("single");
        let api = release_server(json!({ "tag_name": "v0.2.0" })).await;
        let update = fixture.update(api, fixture.sudo(0), "0.1.0");
        let guard = update.inner.apply.lock().await;
        assert!(matches!(update.apply("v0.2.0").await.unwrap_err(), UpdateError::Busy));
        drop(guard);
        assert!(update.apply("v0.2.0").await.is_ok());
    }

    #[tokio::test]
    async fn logged_out_browser_can_load_the_page_and_every_module() {
        let fixture = Fixture::new("assets");
        let state = super::super::config_api::tests::test_state(
            Config::default(),
            fixture.dir.join("config.json"),
        );
        let app = router(state);
        let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let address = listener.local_addr().unwrap();
        let listener = crate::client_ip::ClientIpListener(
            crate::proxy_protocol::ProxyProtocolListener::new(listener, ProxyProtocolMode::Disabled),
        );
        tokio::spawn(async move {
            use axum::serve::ListenerExt;
            let _ = axum::serve(
                listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<ClientConnection>(),
            )
            .await;
        });
        let client = reqwest::Client::new();
        let base = format!("http://{address}");
        let mut paths = vec!["/".to_owned(), "/assets/app.css".to_owned()];
        paths.extend(
            crate::web_ui::APP_JS_MODULES
                .iter()
                .map(|(name, _)| format!("/assets/js/{name}")),
        );
        for path in paths {
            let response = client.get(format!("{base}{path}")).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path} without a session");
            assert!(!response.bytes().await.unwrap().is_empty(), "{path} is empty");
        }
        let missing = client.get(format!("{base}/assets/js/nope.js")).send().await.unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_routes_require_a_session_and_same_origin_posts() {
        let fixture = Fixture::new("routes");
        let state = super::super::config_api::tests::test_state(
            Config::default(),
            fixture.dir.join("config.json"),
        );
        let token = state.sessions.create().unwrap();
        let app = router(state);
        let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
            eprintln!("Skipping network test: bind not permitted");
            return;
        };
        let address = listener.local_addr().unwrap();
        let listener = crate::client_ip::ClientIpListener(
            crate::proxy_protocol::ProxyProtocolListener::new(listener, ProxyProtocolMode::Disabled),
        );
        tokio::spawn(async move {
            use axum::serve::ListenerExt;
            let _ = axum::serve(
                listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<ClientConnection>(),
            )
            .await;
        });
        let client = reqwest::Client::new();
        let cookie = format!("{SESSION_COOKIE}={token}");
        let base = format!("http://{address}");
        let body = r#"{"version":"v9.9.9"}"#;

        for path in ["/api/update/check", "/api/update/apply"] {
            let response = client.post(format!("{base}{path}")).body(body).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path} without a session");
        }
        let response = client.get(format!("{base}/api/update/status")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        for path in ["/api/update/check", "/api/update/apply"] {
            let response = client
                .post(format!("{base}{path}"))
                .header("cookie", &cookie)
                .header("origin", "https://evil.example")
                .header("content-type", "application/json")
                .body(body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} cross-origin");
        }

        let response = client
            .get(format!("{base}/api/update/status"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(value["current_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["status"]["state"], "idle");
    }

    #[tokio::test]
    async fn outcome_is_audited_once_per_run() {
        let fixture = Fixture::new("audit");
        let update = fixture.update("http://127.0.0.1:9/".to_owned(), PathBuf::from("false"), "0.1.0");
        let audit = AuditLog::new(fixture.dir.clone());
        let status = parse_status("state=failed\nversion=v0.2.0\nerror=install_failed\nid=1-2\nupdated=1\n", 2);
        update.audit_outcome(&audit, &status).await;
        update.audit_outcome(&audit, &status).await;
        let running = parse_status("state=running\nid=3-4\nupdated=2\n", 2);
        update.audit_outcome(&audit, &running).await;
        let entries = audit.read_latest(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event, "update_failed");
        assert_eq!(entries[0].detail.as_deref(), Some("version=v0.2.0 error=install_failed"));
    }
}
