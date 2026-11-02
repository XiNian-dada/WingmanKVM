use super::*;

pub(super) async fn list_media(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let snapshot = media_snapshot(&state).await;
    let (images, status) = state.media.list(snapshot).await.map_err(map_media_error)?;
    Ok(no_store(Json(json!({"images": images, "status": status}))))
}

pub(super) async fn upload_media(
    State(state): State<AppState>,
    multipart: ApiMultipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    upload_media_with_limits(state.media_upload.clone(), state.config.clone(), multipart.0, UploadLimits::default()).await
}

pub(super) const UPLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) const UPLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

pub(super) const UPLOAD_BUSY_MESSAGE: &str = "另一个镜像上传正在进行，请稍后重试";

#[derive(Clone, Copy)]
pub(super) struct UploadLimits { idle: Duration, total: Duration }

impl Default for UploadLimits {
    fn default() -> Self { Self { idle: UPLOAD_IDLE_TIMEOUT, total: UPLOAD_TOTAL_TIMEOUT } }
}

#[derive(Clone, Copy)]
pub(super) enum UploadDeadline { Idle, Total }

pub(super) fn upload_timeout_error(deadline: UploadDeadline) -> ApiError {
    let message = match deadline {
        UploadDeadline::Idle => "上传停滞超过 15 秒，已中止；请检查网络后重试",
        UploadDeadline::Total => "上传超过 2 小时总时限，已中止；请重新上传",
    };
    ApiError::new(StatusCode::REQUEST_TIMEOUT, message)
}

pub(super) async fn with_total_deadline<T>(
    future: impl std::future::Future<Output = T>, total_deadline: Instant,
) -> Result<T, UploadDeadline> {
    let remaining = total_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() { return Err(UploadDeadline::Total); }
    tokio::time::timeout(remaining, future).await.map_err(|_| UploadDeadline::Total)
}

pub(super) async fn with_upload_deadlines<T>(
    future: impl std::future::Future<Output = T>, idle_deadline: Instant, total_deadline: Instant,
) -> Result<T, UploadDeadline> {
    if total_deadline <= idle_deadline { return with_total_deadline(future, total_deadline).await; }
    let remaining = idle_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() { return Err(UploadDeadline::Idle); }
    tokio::time::timeout(remaining, future).await.map_err(|_| UploadDeadline::Idle)
}

pub(super) async fn next_upload_chunk(
    field: &mut axum::extract::multipart::Field<'_>, last_progress: &mut Instant,
    limits: UploadLimits, total_deadline: Instant,
) -> Result<Option<Bytes>, ApiError> {
    let idle_deadline = *last_progress + limits.idle;
    let chunk = with_upload_deadlines(field.chunk(), idle_deadline, total_deadline)
        .await.map_err(upload_timeout_error)?.map_err(ApiError::internal)?;
    if let Some(chunk) = &chunk {
        if !chunk.is_empty() { *last_progress = Instant::now(); }
    }
    Ok(chunk)
}

pub(super) async fn write_upload_chunk<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W, chunk: &[u8], uploaded: &mut u64,
    max_upload_bytes: u64, total_deadline: Instant,
) -> Result<(), ApiError> {
    let next_size = uploaded.saturating_add(chunk.len() as u64);
    if next_size > max_upload_bytes {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "镜像超过配置的大小上限"));
    }
    match with_total_deadline(writer.write_all(chunk), total_deadline).await {
        Ok(Ok(())) => { *uploaded = next_size; Ok(()) }
        Ok(Err(error)) => Err(ApiError::internal(error)),
        Err(_) => Err(upload_timeout_error(UploadDeadline::Total)),
    }
}

pub(super) struct PartialUpload { path: Option<PathBuf>, file: Option<fs::File> }

/// Removes uploads a crash or restart left behind; they are hidden from the
/// image list and share a filesystem with config.json. Call only before any
/// upload can be running.
pub(super) fn sweep_partial_uploads(image_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(image_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".wingmankvm-upload-") || !name.ends_with(".partial") {
            continue;
        }
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        match std::fs::remove_file(entry.path()) {
            Ok(()) => tracing::info!(path = %entry.path().display(), "removed an interrupted upload"),
            Err(error) => tracing::warn!(path = %entry.path().display(), %error, "cannot remove an interrupted upload"),
        }
    }
}

impl PartialUpload {
    pub(super) fn create(image_dir: &Path) -> Result<Self, ApiError> {
        let token = generate_token().map_err(ApiError::internal)?;
        let path = image_dir.join(format!(".wingmankvm-upload-{token}.partial"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt as _; options.mode(0o600); }
        let file = options.open(&path).map_err(ApiError::internal)?;
        Ok(Self { path: Some(path), file: Some(fs::File::from_std(file)) })
    }
    pub(super) fn file_mut(&mut self) -> &mut fs::File {
        self.file.as_mut().expect("partial upload file is open")
    }
    pub(super) fn publish(mut self, final_path: &Path) -> std::io::Result<()> {
        self.file.take();
        let partial_path = self.path.take().expect("partial upload path is set");
        match std::fs::hard_link(&partial_path, final_path) {
            Ok(()) => {
                if let Err(error) = std::fs::remove_file(&partial_path) {
                    tracing::warn!(error = %error, path = %partial_path.display(),
                        "published upload but could not remove its temporary file");
                    self.path = Some(partial_path);
                }
                Ok(())
            }
            Err(error) => { self.path = Some(partial_path); Err(error) }
        }
    }
}

impl Drop for PartialUpload {
    fn drop(&mut self) {
        self.file.take();
        if let Some(path) = self.path.take() {
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(error = %error, path = %path.display(),
                        "could not remove aborted upload temporary file");
                }
            }
        }
    }
}

pub(super) async fn upload_media_with_limits(
    upload_slot: Arc<AsyncMutex<()>>, config_store: Arc<RwLock<Config>>,
    mut multipart: Multipart, limits: UploadLimits,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Ok(_upload_guard) = upload_slot.try_lock_owned() else {
        return Err(ApiError::new(StatusCode::CONFLICT, UPLOAD_BUSY_MESSAGE));
    };
    let started = Instant::now();
    let total_deadline = started + limits.total;
    let mut last_progress = started;
    let config = config_store.read().await.media.clone();
    if !config.enabled {
        return Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "虚拟介质尚未配置"));
    }
    let image_dir = config.image_directory
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "镜像目录尚未配置"))?;
    match with_total_deadline(fs::create_dir_all(&image_dir), total_deadline).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(ApiError::internal(error)),
        Err(deadline) => return Err(upload_timeout_error(deadline)),
    }
    let mut uploaded = None;
    loop {
        let next = with_upload_deadlines(
            multipart.next_field(), last_progress + limits.idle, total_deadline,
        ).await.map_err(upload_timeout_error)?;
        let Some(mut field) = next.map_err(ApiError::internal)? else { break };
        last_progress = Instant::now();
        if !matches!(field.name(), Some("image" | "file")) {
            while next_upload_chunk(&mut field, &mut last_progress, limits, total_deadline).await?.is_some() {}
            continue;
        }
        let original = field.file_name()
            .ok_or_else(|| ApiError::bad_request("上传内容缺少文件名"))?.to_string();
        let name = sanitize_upload_name(&original).map_err(map_media_error)?;
        let final_path = image_dir.join(&name);
        let exists = with_total_deadline(fs::try_exists(&final_path), total_deadline)
            .await.map_err(upload_timeout_error)?.map_err(ApiError::internal)?;
        if exists { return Err(ApiError::new(StatusCode::CONFLICT, "同名镜像已经存在")); }
        let mut partial = PartialUpload::create(&image_dir)?;
        let mut size = 0_u64;
        while let Some(chunk) = next_upload_chunk(&mut field, &mut last_progress, limits, total_deadline).await? {
            if chunk.is_empty() { continue; }
            write_upload_chunk(partial.file_mut(), chunk.as_ref(), &mut size, config.max_upload_bytes, total_deadline).await?;
        }
        match with_total_deadline(partial.file_mut().sync_all(), total_deadline).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(ApiError::internal(error)),
            Err(deadline) => return Err(upload_timeout_error(deadline)),
        }
        match partial.publish(&final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists =>
                return Err(ApiError::new(StatusCode::CONFLICT, "同名镜像已经存在")),
            Err(error) => return Err(ApiError::internal(error)),
        }
        uploaded = Some(json!({"name": name, "size": size}));
        break;
    }
    uploaded.map(Json).ok_or_else(|| ApiError::bad_request("表单中没有 image 文件字段"))
}

#[derive(Deserialize)]
pub(super) struct AttachMediaRequest {
    pub(super) name: String,
    #[serde(default)]
    pub(super) media_type: MediaType,
    pub(super) read_only: Option<bool>,
}

pub(super) async fn attach_media(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<AttachMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let default_read_only = state.config.read().await.media.read_only_by_default;
    let status = state
        .media
        .attach(
            media_snapshot(&state).await,
            &request.name,
            request.media_type,
            request.read_only.unwrap_or(default_read_only),
        )
        .await
        .map_err(map_media_error)?;
    Ok(no_store(Json(status)))
}

#[derive(Deserialize)]
pub(super) struct DetachMediaRequest {
    #[serde(default)]
    pub(super) force: bool,
}

pub(super) async fn detach_media(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<DetachMediaRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let status = state
        .media
        .detach(media_snapshot(&state).await, request.force)
        .await
        .map_err(map_media_error)?;
    Ok(no_store(Json(status)))
}

pub(super) async fn media_snapshot(state: &AppState) -> Option<MediaConfigSnapshot> {
    let config = state.config.read().await.media.clone();
    if !config.enabled {
        return None;
    }
    Some(MediaConfigSnapshot {
        image_dir: config.image_directory?,
        lun_path: config.lun_path?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_removes_only_interrupted_uploads() {
        let dir = UploadTestDir::new();
        let leftover = dir.path().join(".wingmankvm-upload-abc.partial");
        let image = dir.path().join("keep.iso");
        let other = dir.path().join(".wingmankvm-upload-note.txt");
        for path in [&leftover, &image, &other] {
            std::fs::write(path, b"x").unwrap();
        }
        sweep_partial_uploads(dir.path());
        assert!(!leftover.exists());
        assert!(image.exists() && other.exists());
    }

    #[test]
    fn virtual_media_attach_payload_supports_writable_disks() {
        let request: AttachMediaRequest = serde_json::from_value(serde_json::json!({
            "name": "storage.img",
            "media_type": "disk",
            "read_only": false
        }))
        .unwrap();
        assert_eq!(request.name, "storage.img");
        assert_eq!(request.media_type, MediaType::Disk);
        assert_eq!(request.read_only, Some(false));

        let request: AttachMediaRequest = serde_json::from_value(serde_json::json!({
            "name": "installer.iso"
        }))
        .unwrap();
        assert_eq!(request.media_type, MediaType::Auto);
        assert_eq!(request.read_only, None);
    }

    use tokio::io::AsyncReadExt as _;

    struct UploadTestDir(PathBuf);

    impl UploadTestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("wingmankvm-upload-test-{}-{}", std::process::id(), generate_token().unwrap()));
            std::fs::create_dir_all(&path).unwrap(); Self(path)
        }
        fn path(&self) -> &Path { &self.0 }
    }

    impl Drop for UploadTestDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    fn upload_test_config(dir: &Path, max: u64) -> Config {
        let mut config = Config::default();
        config.media.enabled = true; config.media.lun_path = Some(dir.join("fake-lun.0"));
        config.media.image_directory = Some(dir.to_path_buf()); config.media.max_upload_bytes = max; config
    }

    #[derive(Clone)]
    struct UploadTestState { slot: Arc<AsyncMutex<()>>, config: Arc<RwLock<Config>>, limits: UploadLimits }

    async fn upload_test_handler(State(state): State<UploadTestState>, multipart: Multipart) -> Response {
        match upload_media_with_limits(state.slot, state.config, multipart, state.limits).await {
            Ok(response) => response.into_response(), Err(error) => error.into_response(),
        }
    }

    struct UploadTestServer { address: SocketAddr, task: tokio::task::JoinHandle<()> }

    impl Drop for UploadTestServer { fn drop(&mut self) { self.task.abort(); } }

    async fn spawn_upload_test_server(config: Config, limits: UploadLimits) -> UploadTestServer {
        let app = Router::new().route("/upload", post(upload_test_handler)).with_state(UploadTestState {
            slot: Arc::new(AsyncMutex::new(())), config: Arc::new(RwLock::new(config)), limits,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
        UploadTestServer { address, task }
    }

    const UPLOAD_BOUNDARY: &str = "wingmankvm-upload-test";

    fn upload_body(name: &str, contents: &[u8]) -> Vec<u8> {
        let mut body = format!("--{UPLOAD_BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
        body.extend_from_slice(contents);
        body.extend_from_slice(format!("\r\n--{UPLOAD_BOUNDARY}--\r\n").as_bytes()); body
    }

    async fn open_upload(address: SocketAddr, length: usize) -> tokio::net::TcpStream {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let header = format!("POST /upload HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: multipart/form-data; boundary={UPLOAD_BOUNDARY}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
        stream.write_all(header.as_bytes()).await.unwrap(); stream.flush().await.unwrap(); stream
    }

    async fn read_upload_status<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) -> StatusCode {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut data = Vec::new(); let mut buf = [0_u8; 1024];
        while !data.windows(4).any(|window| window == b"\r\n\r\n") {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            match tokio::time::timeout(remaining, reader.read(&mut buf)).await {
                Ok(Ok(0)) | Err(_) => break, Ok(Ok(n)) => data.extend_from_slice(&buf[..n]), Ok(Err(_)) => break,
            }
        }
        let code = String::from_utf8_lossy(&data).lines().next()
            .and_then(|line| line.split_whitespace().nth(1)).and_then(|code| code.parse().ok()).unwrap_or(500);
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn partials(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir).unwrap().flatten().map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "partial")).collect()
    }

    async fn wait_partial(dir: &Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while partials(dir).is_empty() {
            assert!(Instant::now() < deadline, "upload did not create a partial file");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn complete_upload(server: &UploadTestServer, name: &str, contents: &[u8]) -> StatusCode {
        let body = upload_body(name, contents);
        let mut stream = open_upload(server.address, body.len()).await;
        stream.write_all(&body).await.unwrap(); read_upload_status(&mut stream).await
    }

    fn limits(idle_ms: u64, total_s: u64) -> UploadLimits {
        UploadLimits { idle: Duration::from_millis(idle_ms), total: Duration::from_secs(total_s) }
    }

    #[tokio::test]
    async fn idle_stall_before_and_midfield_times_out_and_cleans() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 1024), limits(120, 5)).await;
        let mut stalled = open_upload(server.address, 512).await;
        assert_eq!(read_upload_status(&mut stalled).await, StatusCode::REQUEST_TIMEOUT);
        let body = upload_body("mid.iso", &[7_u8; 256]);
        let mut mid = open_upload(server.address, body.len()).await;
        mid.write_all(&body[..body.len() / 2]).await.unwrap();
        assert_eq!(read_upload_status(&mut mid).await, StatusCode::REQUEST_TIMEOUT);
        assert!(partials(dir.path()).is_empty());
        assert!(!dir.path().join("mid.iso").exists());
    }

    #[tokio::test]
    async fn progress_resets_idle_but_total_still_bounds() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 4096), limits(250, 5)).await;
        let contents = [3_u8; 128]; let body = upload_body("progress.iso", &contents);
        let mut stream = open_upload(server.address, body.len()).await;
        let prefix_end = body.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        stream.write_all(&body[..prefix_end]).await.unwrap();
        for chunk in body[prefix_end..].chunks(32) { stream.write_all(chunk).await.unwrap(); tokio::time::sleep(Duration::from_millis(70)).await; }
        assert_eq!(read_upload_status(&mut stream).await, StatusCode::OK);
        assert_eq!(std::fs::read(dir.path().join("progress.iso")).unwrap(), contents);
        let total = spawn_upload_test_server(upload_test_config(dir.path(), 4096), limits(5000, 1)).await;
        let body = upload_body("total.iso", &[5_u8; 256]);
        let stream = open_upload(total.address, body.len()).await;
        let (mut reader, mut writer) = stream.into_split();
        let sender = tokio::spawn(async move {
            for chunk in body.chunks(32) { if writer.write_all(chunk).await.is_err() { break; } tokio::time::sleep(Duration::from_millis(80)).await; }
        });
        assert_eq!(read_upload_status(&mut reader).await, StatusCode::REQUEST_TIMEOUT);
        let _ = sender.await; assert!(!dir.path().join("total.iso").exists());
    }

    #[tokio::test]
    async fn busy_disconnect_size_and_collision_are_safe() {
        let dir = UploadTestDir::new();
        let server = spawn_upload_test_server(upload_test_config(dir.path(), 16), limits(2000, 10)).await;
        let body = upload_body("first.iso", b"first-content");
        let mut first = open_upload(server.address, body.len()).await;
        first.write_all(&body[..body.len() - 8]).await.unwrap(); wait_partial(dir.path()).await;
        let started = Instant::now();
        assert_eq!(complete_upload(&server, "second.iso", b"second").await, StatusCode::CONFLICT);
        assert!(started.elapsed() < Duration::from_secs(1));
        first.write_all(&body[body.len() - 8..]).await.unwrap();
        assert_eq!(read_upload_status(&mut first).await, StatusCode::OK);
        assert_eq!(complete_upload(&server, "large.iso", &[1_u8; 32]).await, StatusCode::PAYLOAD_TOO_LARGE);
        let final_path = dir.path().join("existing.iso"); std::fs::write(&final_path, b"existing").unwrap();
        assert_eq!(complete_upload(&server, "existing.iso", b"replacement").await, StatusCode::CONFLICT);
        assert_eq!(std::fs::read(&final_path).unwrap(), b"existing");
        assert!(partials(dir.path()).is_empty());
    }

    struct FailingUploadWriter;

    impl tokio::io::AsyncWrite for FailingUploadWriter {
        fn poll_write(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>, _buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Err(std::io::Error::other("injected upload write failure")))
        }
        fn poll_flush(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
        fn poll_shutdown(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> { std::task::Poll::Ready(Ok(())) }
    }

    #[tokio::test]
    async fn injected_write_failure_is_reported() {
        let mut writer = FailingUploadWriter; let mut size = 0;
        let error = write_upload_chunk(&mut writer, b"abc", &mut size, 10, Instant::now() + Duration::from_secs(5)).await.unwrap_err();
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR); assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn cancelled_upload_removes_only_its_owned_partial() {
        let dir = UploadTestDir::new();
        let destination = dir.path().join("preserved.iso");
        std::fs::write(&destination, b"valid image").unwrap();
        let partial = PartialUpload::create(dir.path()).unwrap();
        let partial_path = partial.path.clone().unwrap();
        let (ready, running) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _partial = partial;
            let _ = ready.send(());
            std::future::pending::<()>().await;
        });
        running.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!partial_path.exists());
        assert_eq!(std::fs::read(destination).unwrap(), b"valid image");
    }

    #[tokio::test]
    async fn raced_destination_is_never_replaced_during_publish() {
        let dir = UploadTestDir::new();
        let destination = dir.path().join("raced.iso");
        let mut partial = PartialUpload::create(dir.path()).unwrap();
        partial.file_mut().write_all(b"incoming").await.unwrap();
        partial.file_mut().sync_all().await.unwrap();
        std::fs::write(&destination, b"raced valid image").unwrap();
        assert_eq!(partial.publish(&destination).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(destination).unwrap(), b"raced valid image");
        assert!(partials(dir.path()).is_empty());
    }
}
