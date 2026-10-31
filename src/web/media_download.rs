use super::*;

use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TOTAL_TIMEOUT: Duration = UPLOAD_TOTAL_TIMEOUT;
const DOWNLOAD_MAX_REDIRECTS: usize = 5;
const DOWNLOAD_BUSY_MESSAGE: &str = "另一个镜像上传或下载正在进行，请稍后重试";

#[derive(Deserialize)]
pub(super) struct DownloadMediaRequest {
    pub(super) url: String,
    #[serde(default)]
    pub(super) sha256: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DownloadState {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DownloadStatus {
    pub(super) id: String,
    pub(super) state: DownloadState,
    pub(super) received: u64,
    pub(super) total: Option<u64>,
    pub(super) error: Option<String>,
    pub(super) name: String,
    /// HTTP status that describes a failure once the background job finishes.
    /// The start request itself always returns 202, so this lets the UI keep
    /// showing the same error semantics as a synchronous endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) code: Option<u16>,
}

struct DownloadJob {
    status: Mutex<DownloadStatus>,
    cancel: Arc<Cancellation>,
}

#[derive(Default)]
struct Cancellation {
    flag: AtomicBool,
    notify: Notify,
}

impl Cancellation {
    fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn cancelled(&self) {
        loop {
            if self.flag.load(Ordering::SeqCst) {
                return;
            }
            self.notify.notified().await;
        }
    }
}

pub(super) struct MediaDownloadManager {
    current: Mutex<Option<Arc<DownloadJob>>>,
    client: reqwest::Client,
}

impl MediaDownloadManager {
    pub(super) fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
            .timeout(DOWNLOAD_TOTAL_TIMEOUT)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if !matches!(attempt.url().scheme(), "http" | "https") {
                    return attempt.error("redirect target is not http or https");
                }
                if attempt.previous().len() > DOWNLOAD_MAX_REDIRECTS {
                    return attempt.error("too many redirects");
                }
                attempt.follow()
            }))
            .build()?;
        Ok(Self {
            current: Mutex::new(None),
            client,
        })
    }

    pub(super) async fn start(
        self: &Arc<Self>,
        upload_slot: Arc<AsyncMutex<()>>,
        config_store: Arc<RwLock<Config>>,
        request: DownloadMediaRequest,
    ) -> Result<DownloadStatus, ApiError> {
        let url = reqwest::Url::parse(request.url.trim())
            .map_err(|_| ApiError::bad_request("下载 URL 不是有效的绝对地址"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "只支持 http 或 https 下载地址",
            ));
        }
        let expected_sha256 = request.sha256.as_deref().map(parse_sha256).transpose()?;
        let name = match request.name {
            Some(name) => name,
            None => url
                .path_segments()
                .and_then(|segments| segments.filter(|part| !part.is_empty()).next_back())
                .map(str::to_owned)
                .ok_or_else(|| ApiError::bad_request("无法从下载地址推断文件名，请指定 name"))?,
        };
        let name = sanitize_upload_name(&name).map_err(map_media_error)?;

        let upload_guard = upload_slot
            .try_lock_owned()
            .map_err(|_| ApiError::new(StatusCode::CONFLICT, DOWNLOAD_BUSY_MESSAGE))?;
        let config = config_store.read().await.media.clone();
        if !config.enabled {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "虚拟介质尚未配置",
            ));
        }
        let image_dir = config
            .image_directory
            .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "镜像目录尚未配置"))?;

        let id = generate_token().map_err(ApiError::internal)?;
        let status = DownloadStatus {
            id: id.clone(),
            state: DownloadState::Running,
            received: 0,
            total: None,
            error: None,
            name: name.clone(),
            code: None,
        };
        let job = Arc::new(DownloadJob {
            status: Mutex::new(status.clone()),
            cancel: Arc::new(Cancellation::default()),
        });
        *self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(job.clone());

        let manager = self.clone();
        let max_upload_bytes = config.max_upload_bytes;
        tokio::spawn(async move {
            let _upload_guard = upload_guard;
            manager
                .run(job, url, name, expected_sha256, image_dir, max_upload_bytes)
                .await;
        });
        Ok(status)
    }

    pub(super) fn status(&self, id: Option<&str>) -> Result<DownloadStatus, ApiError> {
        let current = self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let job = current
            .as_ref()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "当前没有下载任务"))?;
        let status = job
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if id.is_some_and(|id| id != status.id) {
            return Err(ApiError::new(StatusCode::NOT_FOUND, "下载任务不存在"));
        }
        Ok(status)
    }

    pub(super) fn cancel(&self, id: Option<&str>) -> Result<DownloadStatus, ApiError> {
        let current = self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let job = current
            .as_ref()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "当前没有下载任务"))?;
        let status = job
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if id.is_some_and(|id| id != status.id) {
            return Err(ApiError::new(StatusCode::NOT_FOUND, "下载任务不存在"));
        }
        if status.state != DownloadState::Running {
            return Err(ApiError::new(StatusCode::CONFLICT, "下载任务已经结束"));
        }
        job.cancel.cancel();
        Ok(status)
    }

    async fn run(
        &self,
        job: Arc<DownloadJob>,
        url: reqwest::Url,
        name: String,
        expected_sha256: Option<String>,
        image_dir: PathBuf,
        max_upload_bytes: u64,
    ) {
        let cancel = job.cancel.as_ref();
        let result = self
            .transfer(
                &url,
                &name,
                expected_sha256.as_deref(),
                &image_dir,
                max_upload_bytes,
                &job,
                cancel,
            )
            .await;
        let mut status = job
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(size) => {
                status.state = DownloadState::Done;
                status.received = size;
                status.total = Some(size);
                status.error = None;
                status.code = None;
            }
            Err(DownloadFailure::Cancelled) => {
                status.state = DownloadState::Cancelled;
                status.error = None;
                status.code = None;
            }
            Err(DownloadFailure::Api(error)) => {
                status.state = DownloadState::Failed;
                status.error = Some(error.message);
                status.code = Some(error.status.as_u16());
            }
        }
    }

    async fn transfer(
        &self,
        url: &reqwest::Url,
        name: &str,
        expected_sha256: Option<&str>,
        image_dir: &Path,
        max_upload_bytes: u64,
        job: &Arc<DownloadJob>,
        cancel: &Cancellation,
    ) -> Result<u64, DownloadFailure> {
        fs::create_dir_all(image_dir)
            .await
            .map_err(|error| DownloadFailure::Api(ApiError::internal(error)))?;
        let final_path = image_dir.join(name);
        let exists = fs::try_exists(&final_path)
            .await
            .map_err(|error| DownloadFailure::Api(ApiError::internal(error)))?;
        if exists {
            return Err(DownloadFailure::Api(ApiError::new(
                StatusCode::CONFLICT,
                "同名镜像已经存在",
            )));
        }
        let total_deadline = Instant::now() + DOWNLOAD_TOTAL_TIMEOUT;
        let mut response = {
            let request = self
                .client
                .get(url.clone())
                .header(reqwest::header::USER_AGENT, "WingmanKVM");
            let remaining = total_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DownloadFailure::Api(total_timeout_error()));
            }
            let wait = DOWNLOAD_IDLE_TIMEOUT.min(remaining);
            tokio::select! {
                _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                result = tokio::time::timeout(wait, request.send()) => match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(error)) => return Err(DownloadFailure::Api(request_error(error))),
                    Err(_) if remaining <= DOWNLOAD_IDLE_TIMEOUT => {
                        return Err(DownloadFailure::Api(total_timeout_error()));
                    }
                    Err(_) => return Err(DownloadFailure::Api(idle_timeout_error())),
                },
            }
        };
        if !response.status().is_success() {
            return Err(DownloadFailure::Api(ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("远端返回 HTTP {}", response.status().as_u16()),
            )));
        }
        let total = response.content_length();
        if total.is_some_and(|length| length > max_upload_bytes) {
            return Err(DownloadFailure::Api(too_large_error()));
        }
        update_progress(job, 0, total);
        let mut partial =
            PartialUpload::create(image_dir).map_err(|error| DownloadFailure::Api(error))?;
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        loop {
            let remaining = total_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DownloadFailure::Api(total_timeout_error()));
            }
            let wait = DOWNLOAD_IDLE_TIMEOUT.min(remaining);
            let next = tokio::select! {
                _ = cancel.cancelled() => return Err(DownloadFailure::Cancelled),
                result = tokio::time::timeout(wait, response.chunk()) => match result {
                    Ok(result) => result,
                    Err(_) if remaining <= DOWNLOAD_IDLE_TIMEOUT => {
                        return Err(DownloadFailure::Api(total_timeout_error()));
                    }
                    Err(_) => return Err(DownloadFailure::Api(idle_timeout_error())),
                },
            };
            let chunk = next.map_err(|error| DownloadFailure::Api(request_error(error)))?;
            let Some(chunk) = chunk else { break };
            if chunk.is_empty() {
                continue;
            }
            let next_size = received.saturating_add(chunk.len() as u64);
            if next_size > max_upload_bytes {
                return Err(DownloadFailure::Api(too_large_error()));
            }
            received = next_size;
            hasher.update(&chunk);
            partial
                .file_mut()
                .write_all(&chunk)
                .await
                .map_err(|error| DownloadFailure::Api(ApiError::internal(error)))?;
            update_progress(job, received, total);
        }
        if cancel.flag.load(Ordering::SeqCst) {
            return Err(DownloadFailure::Cancelled);
        }
        if let Some(expected) = expected_sha256 {
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected {
                return Err(DownloadFailure::Api(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "SHA256 校验失败，已删除下载的镜像",
                )));
            }
        }
        partial
            .file_mut()
            .sync_all()
            .await
            .map_err(|error| DownloadFailure::Api(ApiError::internal(error)))?;
        if cancel.flag.load(Ordering::SeqCst) {
            return Err(DownloadFailure::Cancelled);
        }
        match partial.publish(&final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(DownloadFailure::Api(ApiError::new(
                    StatusCode::CONFLICT,
                    "同名镜像已经存在",
                )));
            }
            Err(error) => return Err(DownloadFailure::Api(ApiError::internal(error))),
        }
        Ok(received)
    }
}

#[derive(Debug)]
enum DownloadFailure {
    Cancelled,
    Api(ApiError),
}

fn update_progress(job: &Arc<DownloadJob>, received: u64, total: Option<u64>) {
    let mut status = job
        .status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if status.state == DownloadState::Running {
        status.received = received;
        status.total = total.or(status.total);
    }
}

fn parse_sha256(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("SHA256 必须是 64 位十六进制字符串"));
    }
    Ok(value.to_ascii_lowercase())
}

fn request_error(error: reqwest::Error) -> ApiError {
    if error.is_timeout() {
        ApiError::new(StatusCode::GATEWAY_TIMEOUT, "下载连接超时")
    } else if error.is_redirect() {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "下载重定向过多或目标不是 http/https",
        )
    } else {
        ApiError::new(StatusCode::BAD_GATEWAY, format!("下载请求失败：{error}"))
    }
}

fn idle_timeout_error() -> ApiError {
    ApiError::new(
        StatusCode::REQUEST_TIMEOUT,
        "下载空闲超过 30 秒，已中止；请检查网络后重试",
    )
}

fn total_timeout_error() -> ApiError {
    ApiError::new(StatusCode::REQUEST_TIMEOUT, "下载超过 2 小时总时限，已中止")
}

fn too_large_error() -> ApiError {
    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "镜像超过配置的大小上限")
}

pub(super) async fn download_media(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<DownloadMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let status = state
        .media_download
        .start(state.media_upload.clone(), state.config.clone(), request)
        .await?;
    Ok((StatusCode::ACCEPTED, no_store(Json(status))))
}

pub(super) async fn download_media_status(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(no_store(Json(state.media_download.status(None)?)))
}

pub(super) async fn download_media_status_by_id(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(no_store(Json(state.media_download.status(Some(&id))?)))
}

pub(super) async fn cancel_media_download(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(no_store(Json(state.media_download.cancel(None)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "wingmankvm-download-test-{}-{id}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_config(dir: &Path, max: u64) -> Config {
        let mut config = Config::default();
        config.media.enabled = true;
        config.media.image_directory = Some(dir.to_path_buf());
        config.media.max_upload_bytes = max;
        config
    }

    struct TestServer {
        address: SocketAddr,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn spawn_server() -> TestServer {
        let app = Router::new()
            .route("/file.iso", get(file))
            .route("/large.iso", get(large))
            .route("/chunked.iso", get(chunked))
            .route("/slow.iso", get(slow));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        TestServer { address, task }
    }

    async fn file() -> &'static [u8] {
        b"wingman image contents"
    }

    async fn large() -> Vec<u8> {
        vec![7_u8; 128]
    }

    async fn chunked() -> Response {
        let stream = async_stream::stream! {
            for _ in 0..8 {
                yield Ok::<_, std::io::Error>(Bytes::from_static(&[9_u8; 8]));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        (
            [(CONTENT_TYPE, "application/octet-stream")],
            Body::from_stream(stream),
        )
            .into_response()
    }

    async fn slow() -> Response {
        let stream = async_stream::stream! {
            for _ in 0..200 {
                yield Ok::<_, std::io::Error>(Bytes::from_static(&[3_u8; 4096]));
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        };
        (
            [(CONTENT_TYPE, "application/octet-stream")],
            Body::from_stream(stream),
        )
            .into_response()
    }

    struct Harness {
        dir: TestDir,
        manager: Arc<MediaDownloadManager>,
        slot: Arc<AsyncMutex<()>>,
        config: Arc<RwLock<Config>>,
    }

    impl Harness {
        fn new(max: u64) -> Self {
            let dir = TestDir::new();
            let config = test_config(dir.path(), max);
            Self {
                dir,
                manager: Arc::new(MediaDownloadManager::new().unwrap()),
                slot: Arc::new(AsyncMutex::new(())),
                config: Arc::new(RwLock::new(config)),
            }
        }

        async fn start(
            &self,
            url: String,
            sha256: Option<String>,
            name: Option<String>,
        ) -> Result<DownloadStatus, ApiError> {
            self.manager
                .start(
                    self.slot.clone(),
                    self.config.clone(),
                    DownloadMediaRequest { url, sha256, name },
                )
                .await
        }

        async fn wait_for(&self, id: &str, wanted: &[DownloadState]) -> DownloadStatus {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let status = self.manager.status(Some(id)).unwrap();
                if wanted.contains(&status.state) {
                    return status;
                }
                assert!(
                    Instant::now() < deadline,
                    "download stayed in {:?} (received {})",
                    status.state,
                    status.received
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        fn partials(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.dir.path())
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "partial")
                })
                .collect()
        }
    }

    fn sha256(contents: &[u8]) -> String {
        use sha2::Digest as _;
        format!("{:x}", Sha256::digest(contents))
    }

    #[tokio::test]
    async fn downloads_and_publishes_with_matching_sha256() {
        let server = spawn_server().await;
        let harness = Harness::new(1024);
        let url = format!("http://{}/file.iso", server.address);
        let status = harness
            .start(url, Some(sha256(b"wingman image contents")), None)
            .await
            .unwrap();
        assert_eq!(status.state, DownloadState::Running);
        assert_eq!(status.name, "file.iso");
        let finished = harness
            .wait_for(&status.id, &[DownloadState::Done, DownloadState::Failed])
            .await;
        assert_eq!(finished.state, DownloadState::Done, "{:?}", finished.error);
        assert_eq!(finished.received, 22);
        assert_eq!(finished.total, Some(22));
        assert_eq!(
            std::fs::read(harness.dir.path().join("file.iso")).unwrap(),
            b"wingman image contents"
        );
        assert!(harness.partials().is_empty());
        assert!(harness.slot.try_lock().is_ok());
    }

    #[tokio::test]
    async fn sha256_mismatch_removes_the_partial_file() {
        let server = spawn_server().await;
        let harness = Harness::new(1024);
        let url = format!("http://{}/file.iso", server.address);
        let status = harness
            .start(url, Some("0".repeat(64)), None)
            .await
            .unwrap();
        let finished = harness
            .wait_for(&status.id, &[DownloadState::Done, DownloadState::Failed])
            .await;
        assert_eq!(finished.state, DownloadState::Failed);
        assert_eq!(finished.code, Some(422));
        assert!(finished.error.unwrap().contains("SHA256"));
        assert!(!harness.dir.path().join("file.iso").exists());
        assert!(harness.partials().is_empty());
    }

    #[tokio::test]
    async fn existing_image_is_not_replaced() {
        let server = spawn_server().await;
        let harness = Harness::new(1024);
        let destination = harness.dir.path().join("file.iso");
        std::fs::write(&destination, b"existing image").unwrap();
        let status = harness
            .start(format!("http://{}/file.iso", server.address), None, None)
            .await
            .unwrap();
        let finished = harness
            .wait_for(&status.id, &[DownloadState::Done, DownloadState::Failed])
            .await;
        assert_eq!(finished.state, DownloadState::Failed);
        assert_eq!(finished.code, Some(409));
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing image");
        assert!(harness.partials().is_empty());
    }

    #[tokio::test]
    async fn size_limit_prechecks_content_length_and_streamed_bytes() {
        let server = spawn_server().await;
        let harness = Harness::new(16);
        let url = format!("http://{}/large.iso", server.address);
        let status = harness.start(url, None, None).await.unwrap();
        let finished = harness
            .wait_for(&status.id, &[DownloadState::Done, DownloadState::Failed])
            .await;
        assert_eq!(finished.state, DownloadState::Failed);
        assert_eq!(finished.code, Some(413));
        assert!(!harness.dir.path().join("large.iso").exists());
        assert!(harness.partials().is_empty());

        let harness = Harness::new(16);
        let url = format!("http://{}/chunked.iso", server.address);
        let status = harness.start(url, None, None).await.unwrap();
        let finished = harness
            .wait_for(&status.id, &[DownloadState::Done, DownloadState::Failed])
            .await;
        assert_eq!(finished.state, DownloadState::Failed);
        assert_eq!(finished.code, Some(413));
        assert!(!harness.dir.path().join("chunked.iso").exists());
        assert!(harness.partials().is_empty());
    }

    #[tokio::test]
    async fn only_http_and_https_urls_are_accepted() {
        let harness = Harness::new(1024);
        for url in [
            "ftp://127.0.0.1/file.iso".to_string(),
            "file:///etc/passwd".to_string(),
        ] {
            let error = harness.start(url, None, None).await.unwrap_err();
            assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        }
        assert!(harness.manager.status(None).is_err());
    }

    #[tokio::test]
    async fn invalid_names_and_hashes_are_rejected_before_downloading() {
        let harness = Harness::new(1024);
        let error = harness
            .start("http://127.0.0.1/firmware.bin".to_string(), None, None)
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = harness
            .start(
                "http://127.0.0.1/image.iso".to_string(),
                None,
                Some("../escape.iso".to_string()),
            )
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = harness
            .start(
                "http://127.0.0.1/image.iso".to_string(),
                Some("not-a-hash".to_string()),
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn cancel_stops_a_running_download_and_cleans_up() {
        let server = spawn_server().await;
        let harness = Harness::new(4 * 1024 * 1024);
        let url = format!("http://{}/slow.iso", server.address);
        let status = harness.start(url, None, None).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let current = harness.manager.status(Some(&status.id)).unwrap();
            if current.received > 0 {
                break;
            }
            assert!(Instant::now() < deadline, "download produced no data");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        harness.manager.cancel(Some(&status.id)).unwrap();
        let finished = harness
            .wait_for(
                &status.id,
                &[
                    DownloadState::Cancelled,
                    DownloadState::Done,
                    DownloadState::Failed,
                ],
            )
            .await;
        assert_eq!(finished.state, DownloadState::Cancelled);
        assert!(!harness.dir.path().join("slow.iso").exists());
        assert!(harness.partials().is_empty());
    }

    #[tokio::test]
    async fn download_and_upload_share_one_exclusive_slot() {
        let server = spawn_server().await;
        let harness = Harness::new(4 * 1024 * 1024);
        let held = harness.slot.clone().lock_owned().await;
        let error = harness
            .start(format!("http://{}/file.iso", server.address), None, None)
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        drop(held);

        let status = harness
            .start(format!("http://{}/slow.iso", server.address), None, None)
            .await
            .unwrap();
        assert!(harness.slot.clone().try_lock_owned().is_err());
        harness.manager.cancel(Some(&status.id)).unwrap();
        harness
            .wait_for(&status.id, &[DownloadState::Cancelled])
            .await;
        assert!(harness.slot.clone().try_lock_owned().is_ok());
    }
}
