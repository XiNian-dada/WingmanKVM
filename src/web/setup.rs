use super::*;

#[derive(Serialize)]
pub(super) struct BootstrapResponse {
    pub(super) setup_required: bool,
    pub(super) token_required: bool,
    pub(super) authenticated: bool,
    pub(super) has_passkeys: bool,
    pub(super) terminal_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) terminal_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) client_ip: Option<String>,
    pub(super) client: ClientAddress,
    /// Redacted: notification secrets are replaced by `*_configured` flags.
    pub(super) config: Option<serde_json::Value>,
    pub(super) video: Option<crate::devices::video::VideoStatus>,
    pub(super) webrtc: Option<WebRtcStatus>,
    pub(super) capabilities: Option<Capabilities>,
}

pub(super) async fn load_bootstrap_auth(auth: AuthStore) -> Result<(bool, Option<AuthRecord>), String> {
    tokio::task::spawn_blocking(move || Ok::<_, AuthError>((auth.is_initialized()?, auth.load()?)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

pub(super) async fn bootstrap(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Response {
    let client = state.client_address(connection, &headers).await;
    // An unreadable auth store is not "first run": report it instead of
    // steering the operator into a setup flow whose token is already gone.
    let (initialized, record) = match load_bootstrap_auth(state.auth.clone()).await {
        Ok(stored) => stored,
        Err(error) => {
            tracing::error!(%error, "authentication store is unreadable");
            return ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "认证数据不可读，请检查 auth.json 的权限与服务日志",
            )
            .into_response();
        }
    };
    let setup_required = !initialized;
    let authenticated = !setup_required && is_authenticated(&state, &headers);
    let has_passkeys = record
        .as_ref()
        .is_some_and(|record| !record.passkeys.is_empty());
    let terminal_ready = authenticated
        && record
            .as_ref()
            .is_some_and(|record| record.system_password_synced);
    let terminal_message = (authenticated && !terminal_ready)
        .then(|| TERMINAL_DEGRADED_MESSAGE.to_owned());
    let (config, video, webrtc, capabilities) = if authenticated {
        let config = state.config.read().await.clone();
        let webrtc = state.webrtc.status();
        let capabilities = capabilities_with_webrtc(&config, webrtc.available);
        (
            Some(config.redacted_json()),
            Some(state.video.status()),
            Some(webrtc),
            Some(capabilities),
        )
    } else {
        (None, None, None, None)
    };
    no_store(Json(BootstrapResponse {
        setup_required,
        token_required: setup_required,
        authenticated,
        has_passkeys,
        terminal_ready,
        terminal_message,
        client_ip: Some(client.ip.to_string()),
        client,
        config,
        video,
        webrtc,
        capabilities,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct SetupRequest {
    pub(super) username: String,
    pub(super) password: String,
    pub(super) setup_token: String,
    #[serde(default)]
    pub(super) video_device: Option<String>,
    #[serde(default)]
    pub(super) keyboard_device: Option<String>,
    #[serde(default)]
    pub(super) mouse_device: Option<String>,
    #[serde(default)]
    pub(super) absolute_pointer_device: Option<String>,
    #[serde(default)]
    pub(super) pointer_mode: Option<PointerMode>,
    #[serde(default)]
    pub(super) power_enabled: Option<bool>,
    #[serde(default)]
    pub(super) gpio_chip: Option<String>,
    #[serde(default)]
    pub(super) gpio_line: Option<u32>,
    #[serde(default)]
    pub(super) active_high: Option<bool>,
    #[serde(default)]
    pub(super) media_enabled: Option<bool>,
    #[serde(default)]
    pub(super) lun_path: Option<String>,
    #[serde(default)]
    pub(super) image_directory: Option<String>,
}

pub(super) async fn setup(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<SetupRequest>,
) -> Result<Response, ApiError> {
    let setup_guard = state.setup.clone().lock_owned().await;
    if state.auth.is_initialized_async().await.map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::CONFLICT, "管理员已经初始化"));
    }
    let valid_token = {
        let token = state
            .setup_token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        token
            .as_deref()
            .is_some_and(|expected| constant_time_eq(expected, &request.setup_token))
    };
    if !valid_token {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "初始化令牌不正确"));
    }

    let discovered = discovery::scan().await;
    let mut new_config = state.config.read().await.clone();
    new_config.video.device =
        resolve_setup_video(optional_path(request.video_device), &discovered.video)?;
    new_config.video.auto_detect = new_config.video.device.is_none();
    new_config.hid.keyboard_device = resolve_setup_hid(
        optional_path(request.keyboard_device),
        SetupHidRole::Keyboard,
        &discovered,
    )?;
    // New clients always send an explicit mode. If an older client omits it,
    // infer the legacy choice from the path it supplied, otherwise prefer the
    // new absolute-pointer default.
    let pointer_mode = request.pointer_mode.unwrap_or_else(|| {
        if request.absolute_pointer_device.is_some() {
            PointerMode::Absolute
        } else if request.mouse_device.is_some() {
            PointerMode::Relative
        } else {
            PointerMode::Absolute
        }
    });
    let requested_mouse = optional_path(request.mouse_device);
    let requested_absolute = optional_path(request.absolute_pointer_device);
    new_config.hid.mouse_device = match pointer_mode {
        PointerMode::Relative | PointerMode::Auto => {
            resolve_setup_hid(requested_mouse, SetupHidRole::Mouse, &discovered)?
        }
        PointerMode::Absolute => None,
    };
    new_config.hid.absolute_pointer_device = match pointer_mode {
        PointerMode::Absolute | PointerMode::Auto => resolve_setup_hid(
            requested_absolute,
            SetupHidRole::AbsolutePointer,
            &discovered,
        )?,
        PointerMode::Relative => None,
    };
    new_config.hid.pointer_mode = pointer_mode;
    new_config.hid.auto_detect = hid_needs_auto_detection(&new_config.hid);
    new_config.power.gpio_chip = optional_string(request.gpio_chip);
    new_config.power.gpio_line = request.gpio_line;
    new_config.power.active_high = request.active_high.unwrap_or(new_config.power.active_high);
    new_config.power.enabled = request
        .power_enabled
        .unwrap_or(new_config.power.gpio_chip.is_some() && new_config.power.gpio_line.is_some());
    let requested_lun = optional_path(request.lun_path);
    let media_enabled = request.media_enabled.unwrap_or(requested_lun.is_some());
    new_config.media.lun_path =
        resolve_setup_lun(requested_lun, media_enabled, &discovered.mass_storage_luns)?;
    new_config.media.image_directory =
        optional_path(request.image_directory).or_else(|| Some(config::state_dir().join("images")));
    new_config.media.enabled = media_enabled;
    validate_config(&new_config)?;
    let username = request.username;
    let password = request.password;
    let session_metadata = SessionMetadata {
        client_ip: Some(state.client_address(connection, &headers).await.ip),
        user_agent: client_user_agent(&headers),
        login_method: Some(LoginMethod::Setup),
    };

    // Validate the username, enforce the complete password policy and finish
    // the expensive hash before changing the local maintenance account.
    let auth_record = {
        let password = password.clone();
        tokio::task::spawn_blocking(move || AuthRecord::new(username, &password))
            .await
            .map_err(ApiError::internal)?
            .map_err(map_auth_error)?
    };

    // Keep the complete initialization transaction and its lock alive if the
    // HTTP client disconnects after privileged password synchronization starts.
    tokio::spawn(async move {
        let _setup_guard = setup_guard;
        finish_setup(state, new_config, auth_record, password, session_metadata).await
    })
    .await
    .map_err(ApiError::internal)?
}

pub(super) async fn finish_setup(
    state: AppState,
    new_config: Config,
    auth_record: AuthRecord,
    password: String,
    session_metadata: SessionMetadata,
) -> Result<Response, ApiError> {
    persist_config(state.config_path.as_ref(), &new_config).await?;

    // Keep the local maintenance account in sync with the web administrator.
    // The account is deliberately fixed to `wingman`; never interpolate a
    // user-supplied name into a shell command. A helper failure must not abort
    // setup: KVM control is still usable, only the browser terminal is gated.
    let terminal_ready = match sync_system_password(&password).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(error = %error, "system password synchronization failed during setup");
            false
        }
    };
    let mut auth_record = auth_record;
    auth_record.system_password_synced = terminal_ready;

    let auth = state.auth.clone();
    tokio::task::spawn_blocking(move || auth.initialize_record(auth_record))
        .await
        .map_err(ApiError::internal)?
        .map_err(map_auth_error)?;

    *state.config.write().await = new_config.clone();
    state
        .video
        .reconfigure(new_config.video.clone())
        .map_err(ApiError::internal)?;
    state.webrtc.reconfigure(new_config.video.h264).await;
    state
        .power
        .set_power_led_config(power_led_snapshot(&new_config.power))
        .await;
    state
        .setup_token
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let audit_ip = session_metadata.client_ip;
    let session = state
        .sessions
        .create_with_metadata(session_metadata)
        .map_err(ApiError::internal)?;
    let session_id = state.sessions.session_id(&session);
    state
        .audit
        .record(
            AuditEvent::SetupCompleted,
            audit_ip,
            session_id,
            Some("method=setup".to_owned()),
        )
        .await;
    let session_secure = new_config.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "terminal_ready": terminal_ready,
            "terminal_message": (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE),
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

pub(super) fn candidate_is_ready(candidate: &discovery::DeviceCandidate) -> bool {
    candidate.compatible == Some(true)
        && candidate.gadget_bound == Some(true)
        && candidate.function_linked == Some(true)
}

pub(super) fn resolve_setup_video(
    requested: Option<PathBuf>,
    candidates: &[discovery::DeviceCandidate],
) -> Result<Option<PathBuf>, ApiError> {
    if let Some(path) = requested {
        let candidate = find_candidate(&path, candidates).ok_or_else(|| {
            ApiError::bad_request("所选视频设备不在本次扫描结果中，请重新扫描后再试")
        })?;
        validate_video_candidate(candidate)?;
        return Ok(Some(path));
    }

    unique_usable_candidate(
        candidates,
        video_candidate_is_usable,
        "检测到多个可用的 MJPEG 视频设备，请在设备列表中选择一个",
    )
}

pub(super) fn validate_video_candidate(candidate: &discovery::DeviceCandidate) -> Result<(), ApiError> {
    if candidate.video_capture == Some(false) {
        return Err(ApiError::bad_request("所选视频设备不支持 V4L2 视频采集"));
    }
    if candidate.supports_mjpeg == Some(false) {
        return Err(ApiError::bad_request(
            "所选视频设备不支持 MJPEG，无法用于当前视频流",
        ));
    }
    if !video_candidate_is_usable(candidate) {
        return Err(ApiError::bad_request(
            "无法确认所选视频设备的采集与 MJPEG 能力，请检查设备权限后重新扫描",
        ));
    }
    Ok(())
}

pub(super) fn video_candidate_is_usable(candidate: &discovery::DeviceCandidate) -> bool {
    video_candidate_is_usable_with_probe_requirement(candidate, cfg!(target_os = "linux"))
}

pub(super) fn video_candidate_is_usable_with_probe_requirement(
    candidate: &discovery::DeviceCandidate,
    require_verified_probe: bool,
) -> bool {
    if require_verified_probe {
        candidate.video_capture == Some(true) && candidate.supports_mjpeg == Some(true)
    } else {
        candidate.video_capture != Some(false) && candidate.supports_mjpeg != Some(false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupHidRole {
    Keyboard,
    Mouse,
    AbsolutePointer,
}

impl SetupHidRole {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "键盘设备",
            Self::Mouse => "相对鼠标设备",
            Self::AbsolutePointer => "绝对指针设备",
        }
    }

    pub(super) fn audit_name(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Mouse => "relative_mouse",
            Self::AbsolutePointer => "absolute_pointer",
        }
    }

    pub(super) fn candidates(self, discovered: &discovery::DeviceDiscovery) -> &[discovery::DeviceCandidate] {
        match self {
            Self::Keyboard => &discovered.keyboard,
            Self::Mouse => &discovered.mouse,
            Self::AbsolutePointer => &discovered.absolute_pointer,
        }
    }
}

pub(super) fn resolve_setup_hid(
    requested: Option<PathBuf>,
    role: SetupHidRole,
    discovered: &discovery::DeviceDiscovery,
) -> Result<Option<PathBuf>, ApiError> {
    let candidates = role.candidates(discovered);
    let Some(path) = requested else {
        return unique_usable_candidate(
            candidates,
            candidate_is_ready,
            format!("检测到多个可用的{}，请在设备列表中选择一个", role.label()),
        );
    };

    if let Some(candidate) = find_candidate(&path, candidates) {
        validate_gadget_candidate(role.label(), candidate)?;
        return Ok(Some(path));
    }

    if let Some(actual_role) = recognized_hid_role(&path, discovered) {
        return Err(ApiError::bad_request(format!(
            "所选{}实际被识别为{}，请重新选择",
            role.label(),
            actual_role.label()
        )));
    }

    // Some kernels do not expose the configfs `dev` attribute needed to map
    // hidg nodes back to functions. An explicit advanced choice may use a
    // node found by the generic HID scan, but it is never selected
    // automatically and the unverified role mapping is always audited.
    if find_candidate(&path, &discovered.hid).is_some() {
        tracing::warn!(
            path = %path.display(),
            requested_role = role.audit_name(),
            "首次设置采用无法从 configfs 验证角色的 HID 高级配置"
        );
        return Ok(Some(path));
    }

    Err(ApiError::bad_request(format!(
        "所选{}不在本次 HID 扫描结果中，请重新扫描后再试",
        role.label()
    )))
}

pub(super) fn recognized_hid_role(
    path: &Path,
    discovered: &discovery::DeviceDiscovery,
) -> Option<SetupHidRole> {
    [
        SetupHidRole::Keyboard,
        SetupHidRole::Mouse,
        SetupHidRole::AbsolutePointer,
    ]
    .into_iter()
    .find(|role| find_candidate(path, role.candidates(discovered)).is_some())
}

pub(super) fn resolve_setup_lun(
    requested: Option<PathBuf>,
    enabled: bool,
    candidates: &[discovery::DeviceCandidate],
) -> Result<Option<PathBuf>, ApiError> {
    if let Some(path) = requested {
        let candidate = find_candidate(&path, candidates).ok_or_else(|| {
            ApiError::bad_request("所选虚拟介质 LUN 不在本次扫描结果中，请重新扫描后再试")
        })?;
        validate_gadget_candidate("虚拟介质 LUN", candidate)?;
        return Ok(Some(path));
    }
    if !enabled {
        return Ok(None);
    }

    unique_usable_candidate(
        candidates,
        candidate_is_ready,
        "检测到多个可用的 USB 虚拟介质，请在设备列表中选择一个",
    )?
    .ok_or_else(|| ApiError::bad_request("未发现可用的 USB 虚拟介质，请先运行安装程序"))
    .map(Some)
}

pub(super) fn validate_gadget_candidate(
    label: &str,
    candidate: &discovery::DeviceCandidate,
) -> Result<(), ApiError> {
    match candidate.compatible {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}与 WingmanKVM 所需的 USB 描述符不兼容"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}的 USB 描述符，请重新扫描"
            )));
        }
    }
    match candidate.function_linked {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}尚未链接到 USB Gadget 配置"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}是否已链接到 USB Gadget 配置，请重新扫描"
            )));
        }
    }
    match candidate.gadget_bound {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::bad_request(format!(
                "所选{label}所属 Gadget 尚未绑定 UDC"
            )));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "无法确认所选{label}所属 Gadget 的 UDC 状态，请重新扫描"
            )));
        }
    }
    Ok(())
}

pub(super) fn unique_usable_candidate(
    candidates: &[discovery::DeviceCandidate],
    usable: impl Fn(&discovery::DeviceCandidate) -> bool,
    multiple_message: impl Into<String>,
) -> Result<Option<PathBuf>, ApiError> {
    let mut usable = candidates.iter().filter(|candidate| usable(candidate));
    let Some(candidate) = usable.next() else {
        return Ok(None);
    };
    if usable.next().is_some() {
        return Err(ApiError::bad_request(multiple_message));
    }
    Ok(Some(candidate.path.clone()))
}

pub(super) fn find_candidate<'a>(
    requested: &Path,
    candidates: &'a [discovery::DeviceCandidate],
) -> Option<&'a discovery::DeviceCandidate> {
    candidates
        .iter()
        .find(|candidate| paths_refer_to_same_node(requested, &candidate.path))
}

pub(super) fn paths_refer_to_same_node(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(super) async fn scan_devices() -> impl IntoResponse {
    no_store(Json(discovery::scan().await))
}

/// Results are reused briefly so unauthenticated callers during first setup
/// cannot queue unbounded device probes.
pub(super) const SETUP_SCAN_CACHE_TTL: Duration = Duration::from_secs(3);

pub(super) async fn scan_setup_devices(State(state): State<AppState>) -> Result<Response, ApiError> {
    if state.auth.is_initialized_async().await.map_err(ApiError::internal)? {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    }
    Ok(no_store(Json(cached_setup_scan(&state.setup_scan, discovery::scan).await)))
}

/// Serializes setup scans and shares one result across callers within the TTL.
pub(super) async fn cached_setup_scan<F, Fut>(
    cache: &AsyncMutex<Option<(Instant, discovery::DeviceDiscovery)>>,
    scan: F,
) -> discovery::DeviceDiscovery
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = discovery::DeviceDiscovery>,
{
    let mut cached = cache.lock().await;
    if let Some((scanned_at, result)) = cached.as_ref()
        && scanned_at.elapsed() < SETUP_SCAN_CACHE_TTL
    {
        return result.clone();
    }
    let result = scan().await;
    *cached = Some((Instant::now(), result.clone()));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::test_support::*;

    #[cfg(unix)]
    use std::{
        fs,
        os::unix::fs::symlink,
        sync::atomic::Ordering,
    };

    #[test]
    fn automatic_gadget_selection_requires_verified_ready_state() {
        let mut unknown = candidate("/dev/hidg0", "keyboard");
        assert!(!candidate_is_ready(&unknown));

        unknown.compatible = Some(true);
        unknown.function_linked = Some(true);
        unknown.gadget_bound = Some(false);
        assert!(!candidate_is_ready(&unknown));

        unknown.gadget_bound = Some(true);
        assert!(candidate_is_ready(&unknown));
    }

    #[test]
    fn automatic_video_selection_uses_only_unique_mjpeg_capture_device() {
        let mut raw = candidate("/dev/video0", "video");
        raw.video_capture = Some(true);
        raw.supports_mjpeg = Some(false);
        let mut mjpeg = candidate("/dev/video1", "video");
        mjpeg.video_capture = Some(true);
        mjpeg.supports_mjpeg = Some(true);

        let selected = unique_usable_candidate(
            &[raw, mjpeg.clone()],
            |candidate| video_candidate_is_usable_with_probe_requirement(candidate, true),
            "multiple",
        )
        .unwrap();
        assert_eq!(selected, Some(PathBuf::from("/dev/video1")));

        let error = unique_usable_candidate(
            &[mjpeg.clone(), {
                let mut second = mjpeg;
                second.path = PathBuf::from("/dev/video2");
                second
            }],
            |candidate| video_candidate_is_usable_with_probe_requirement(candidate, true),
            "检测到多个可用视频设备",
        )
        .unwrap_err();
        assert_eq!(error.message, "检测到多个可用视频设备");
    }

    #[test]
    fn linux_video_probe_must_confirm_capture_and_mjpeg() {
        let unprobed = candidate("/dev/video0", "video");
        assert!(!video_candidate_is_usable_with_probe_requirement(
            &unprobed, true
        ));
        assert!(video_candidate_is_usable_with_probe_requirement(
            &unprobed, false
        ));

        let mut capture = unprobed;
        capture.video_capture = Some(true);
        capture.supports_mjpeg = Some(true);
        assert!(video_candidate_is_usable_with_probe_requirement(
            &capture, true
        ));
    }

    #[test]
    fn explicit_video_must_come_from_the_latest_scan() {
        let error = resolve_setup_video(Some(PathBuf::from("/dev/video9")), &[]).unwrap_err();
        assert!(error.message.contains("不在本次扫描结果"));

        let mut raw = candidate("/dev/video0", "video");
        raw.video_capture = Some(true);
        raw.supports_mjpeg = Some(false);
        let error = resolve_setup_video(Some(raw.path.clone()), &[raw]).unwrap_err();
        assert!(error.message.contains("不支持 MJPEG"));
    }

    #[test]
    fn explicit_hid_rejects_a_known_different_role() {
        let mouse = ready_gadget_candidate("/dev/hidg1", "mouse");
        let mut discovered = empty_discovery();
        discovered.hid.push(candidate("/dev/hidg1", "hid_gadget"));
        discovered.mouse.push(mouse);

        let error = resolve_setup_hid(
            Some(PathBuf::from("/dev/hidg1")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap_err();
        assert!(error.message.contains("实际被识别为相对鼠标设备"));
    }

    #[cfg(unix)]
    #[test]
    fn explicit_stable_symlink_matches_the_scanned_device_node() {
        let id = TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wingmankvm-web-device-match-{}-{id}",
            std::process::id()
        ));
        let node = root.join("hidg0");
        let stable = root.join("wingmankvm-keyboard");
        fs::create_dir_all(&root).unwrap();
        fs::write(&node, []).unwrap();
        symlink(&node, &stable).unwrap();

        let scanned = ready_gadget_candidate(node.to_str().unwrap(), "keyboard");
        assert!(find_candidate(&stable, &[scanned]).is_some());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_unclassified_hid_is_the_only_manual_role_escape_hatch() {
        let mut discovered = empty_discovery();
        discovered.hid.push(candidate("/dev/hidg0", "hid_gadget"));

        let selected = resolve_setup_hid(
            Some(PathBuf::from("/dev/hidg0")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap();
        assert_eq!(selected, Some(PathBuf::from("/dev/hidg0")));

        assert_eq!(
            resolve_setup_hid(None, SetupHidRole::Keyboard, &discovered).unwrap(),
            None
        );
        let error = resolve_setup_hid(
            Some(PathBuf::from("/dev/not-a-hid-node")),
            SetupHidRole::Keyboard,
            &discovered,
        )
        .unwrap_err();
        assert!(error.message.contains("不在本次 HID 扫描结果"));
    }

    #[test]
    fn explicit_and_automatic_luns_require_bound_linked_compatible_candidates() {
        let path = "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.0/lun.0";

        let mut incompatible = ready_gadget_candidate(path, "mass_storage_lun");
        incompatible.compatible = Some(false);
        let error =
            resolve_setup_lun(Some(incompatible.path.clone()), true, &[incompatible]).unwrap_err();
        assert!(error.message.contains("描述符不兼容"));

        let mut unlinked = ready_gadget_candidate(path, "mass_storage_lun");
        unlinked.function_linked = Some(false);
        let error = resolve_setup_lun(Some(unlinked.path.clone()), true, &[unlinked]).unwrap_err();
        assert!(error.message.contains("尚未链接"));

        let mut unbound = ready_gadget_candidate(path, "mass_storage_lun");
        unbound.gadget_bound = Some(false);
        let error = resolve_setup_lun(Some(unbound.path.clone()), true, &[unbound]).unwrap_err();
        assert!(error.message.contains("尚未绑定 UDC"));

        let first = ready_gadget_candidate(
            "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.0/lun.0",
            "mass_storage_lun",
        );
        let second = ready_gadget_candidate(
            "/sys/kernel/config/usb_gadget/kvm/functions/mass_storage.1/lun.0",
            "mass_storage_lun",
        );
        let error = resolve_setup_lun(None, true, &[first, second]).unwrap_err();
        assert!(error.message.contains("多个可用的 USB 虚拟介质"));
    }

    #[test]
    fn setup_payload_keeps_explicit_capability_choices() {
        let request: SetupRequest = serde_json::from_value(serde_json::json!({
            "username": "admin",
            "password": "Strong-password-123",
            "setup_token": "token",
            "power_enabled": true,
            "gpio_chip": "gpiochip1",
            "gpio_line": 7,
            "active_high": false,
            "media_enabled": true
        }))
        .unwrap();

        assert_eq!(request.power_enabled, Some(true));
        assert_eq!(request.media_enabled, Some(true));
        assert_eq!(request.gpio_chip.as_deref(), Some("gpiochip1"));
        assert_eq!(request.gpio_line, Some(7));
        assert_eq!(request.active_high, Some(false));
    }

    #[tokio::test]
    async fn bootstrap_auth_distinguishes_missing_from_unreadable_store() {
        let root = std::env::temp_dir().join(format!("wingman-bootstrap-{}", generate_token().unwrap()));
        std::fs::create_dir_all(&root).unwrap();
        let missing = AuthStore::new(root.join("auth.json"));
        let (initialized, record) = load_bootstrap_auth(missing).await.unwrap();
        assert!(!initialized);
        assert!(record.is_none());

        // A regular file used as a parent directory makes metadata fail with ENOTDIR.
        std::fs::write(root.join("not-a-dir"), b"x").unwrap();
        let unreadable = AuthStore::new(root.join("not-a-dir").join("auth.json"));
        assert!(load_bootstrap_auth(unreadable).await.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_setup_scans_share_one_probe() {
        let cache = Arc::new(AsyncMutex::new(None));
        let probes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let probes = probes.clone();
            tasks.push(tokio::spawn(async move {
                cached_setup_scan(&cache, || async move {
                    probes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    empty_discovery()
                })
                .await
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(probes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
