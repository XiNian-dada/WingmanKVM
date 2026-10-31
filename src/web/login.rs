use super::*;

#[derive(Deserialize)]
pub(super) struct LoginRequest {
    pub(super) username: String,
    pub(super) password: String,
}

pub(super) async fn login(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<LoginRequest>,
) -> Result<Response, ApiError> {
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Reserve at admission so a burst cannot retry around the per-IP budget
    // merely because the global password workers are busy.
    if !state.login_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }
    let Some(password_slot) = state.login_limiter.try_acquire_password_slot() else {
        return Err(too_many_login_attempts());
    };
    let auth = state.auth.clone();
    let username = request.username;
    let username_for_audit = username.clone();
    let password = request.password;
    let password_for_verification = password.clone();
    let verified = run_with_password_slot(password_slot, move || {
        let Some(record) = auth.load()? else {
            return Ok(None);
        };
        Ok(record
            .verify_credentials(&username, &password_for_verification)?
            .then_some(record))
    })
    .await?;

    let Some(verified) = verified else {
        state
            .audit
            .record(
                AuditEvent::LoginFailed,
                Some(client_ip),
                None,
                Some(login_failure_detail(&username_for_audit)),
            )
            .await;
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "用户名或密码不正确",
        ));
    };

    // Older deployments predate password synchronization during setup. A
    // successful administrator login is the only safe point at which the
    // plaintext password is available again. The verified record binds the
    // migration to the exact password hash that just passed verification.
    let mut terminal_ready = verified.system_password_synced;
    if !terminal_ready {
        // The migration supervisor owns the lock guard, the current-state
        // recheck, the bounded helper and the conditional marker write. If this
        // HTTP task is cancelled, dropping the JoinHandle does not cancel that
        // task or release the guard while the helper is still running.
        terminal_ready = migrate_system_password_on_login(
            state.password_migration.clone(),
            state.auth.clone(),
            verified,
            password,
            SystemPasswordHelper::default(),
        )
        .await;
    }
    let terminal_message = (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE);

    let session = state
        .sessions
        .create_with_metadata(SessionMetadata {
            client_ip: Some(client_ip),
            user_agent: client_user_agent(&headers),
            login_method: Some(LoginMethod::Password),
        })
        .map_err(ApiError::internal)?;
    let session_id = state.sessions.session_id(&session);
    state
        .audit
        .record(
            AuditEvent::LoginSucceeded,
            Some(client_ip),
            session_id,
            Some("method=password".to_owned()),
        )
        .await;
    let session_secure = state.config.read().await.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "terminal_ready": terminal_ready,
            "terminal_message": terminal_message,
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

pub(super) fn too_many_login_attempts() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "登录尝试过多，请稍后再试",
    )
}

/// Run password verification on the blocking pool while keeping the global
/// verification permit inside the blocking closure. Aborting or dropping the
/// HTTP task therefore cannot release the slot while Argon2 is still running.
pub(super) async fn run_with_password_slot<T, F>(
    password_slot: OwnedSemaphorePermit,
    verify: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AuthError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let _password_slot = password_slot;
        verify()
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(map_auth_error)
}

pub(super) fn login_failure_detail(username: &str) -> String {
    // Only the submitted username may be recorded. Passwords, passkey
    // material, and every other request field never reach the audit log.
    let username: String = username
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();
    format!("method=password username={username}")
}

pub(super) async fn logout(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Response {
    let ip = Some(state.client_address(connection, &headers).await.ip);
    let mut revoked_session = None;
    if let Some(token) = session_token(&headers) {
        revoked_session = state.sessions.session_id(token);
        state.sessions.revoke(token);
    }
    if let Some(session_id) = revoked_session {
        state
            .audit
            .record(AuditEvent::Logout, ip, Some(session_id), None)
            .await;
    }
    let session_secure = state.config.read().await.server.session_cookie_secure();
    with_session_cookie(
        Json(json!({"ok": true})).into_response(),
        None,
        session_secure,
    )
}

pub(super) const LOGIN_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);

pub(super) const MAX_LOGIN_ATTEMPTS_PER_IP: usize = 5;

pub(super) const MAX_PASSKEY_STARTS_PER_IP: usize = 10;

pub(super) const MAX_LOGIN_LIMITER_RECORDS: usize = 4096;

pub(super) const MAX_CONCURRENT_PASSWORD_VERIFICATIONS: usize = 2;

#[derive(Clone)]
pub(super) struct LoginLimiter {
    pub(super) attempts: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    pub(super) password_slots: Arc<Semaphore>,
    pub(super) per_ip: usize,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::with_budget(MAX_LOGIN_ATTEMPTS_PER_IP)
    }
}

impl LoginLimiter {
    pub(super) fn with_budget(per_ip: usize) -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
            password_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_PASSWORD_VERIFICATIONS)),
            per_ip,
        }
    }

    /// Atomically admit one authentication submission for `ip`.
    ///
    /// At most five admissions are recorded per client IP in the rolling
    /// 60-second window. Every admitted password attempt and Passkey finish is
    /// counted exactly once, regardless of whether it succeeds; a successful
    /// login never clears earlier in-flight admissions. Expired timestamps are
    /// pruned during every admission, and the record map is bounded to 4096
    /// entries: an unseen IP at capacity fails closed.
    pub(super) fn try_reserve(&self, ip: IpAddr) -> bool {
        self.try_reserve_at(ip, Instant::now())
    }

    pub(super) fn try_reserve_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut attempts = self.attempts.lock().unwrap_or_else(|p| p.into_inner());
        prune_login_attempts(&mut attempts, now);
        if let Some(timestamps) = attempts.get_mut(&ip) {
            if timestamps.len() >= self.per_ip {
                return false;
            }
            timestamps.push_back(now);
            true
        } else if attempts.len() >= MAX_LOGIN_LIMITER_RECORDS {
            false
        } else {
            attempts.insert(ip, VecDeque::from([now]));
            true
        }
    }

    /// Check admission capacity without recording an attempt or allocating a
    /// new per-IP record. Also prunes expired records.
    pub(super) fn has_capacity(&self, ip: IpAddr) -> bool {
        self.has_capacity_at(ip, Instant::now())
    }

    pub(super) fn has_capacity_at(&self, ip: IpAddr, now: Instant) -> bool {
        let mut attempts = self.attempts.lock().unwrap_or_else(|p| p.into_inner());
        prune_login_attempts(&mut attempts, now);
        attempts.get(&ip).map_or(
            attempts.len() < MAX_LOGIN_LIMITER_RECORDS,
            |timestamps| timestamps.len() < self.per_ip,
        )
    }

    pub(super) fn try_acquire_password_slot(&self) -> Option<OwnedSemaphorePermit> {
        self.password_slots.clone().try_acquire_owned().ok()
    }
}

pub(super) fn prune_login_attempts(attempts: &mut HashMap<IpAddr, VecDeque<Instant>>, now: Instant) {
    attempts.retain(|_, timestamps| {
        // Concurrent callers can sample time before acquiring the mutex in a
        // different order, so expired entries need not be at the front.
        timestamps.retain(|at| now.saturating_duration_since(*at) < LOGIN_ATTEMPT_WINDOW);
        !timestamps.is_empty()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_login_audit_contains_only_the_username_and_method() {
        let detail = login_failure_detail("admin");
        assert_eq!(detail, "method=password username=admin");
        assert!(!detail.contains("Correct-Horse7!"));
        assert!(!detail.contains("password="));

        let noisy = login_failure_detail(&format!("bad\nname{}", "x".repeat(200)));
        assert!(!noisy.contains('\n'));
        assert!(noisy.chars().count() <= "method=password username=".len() + 64);
    }

    #[test]
    fn twelve_concurrent_reservations_admit_exactly_five() {
        let limiter = LoginLimiter::default();
        let ip: IpAddr = "203.0.113.10".parse().unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(13));
        let admitted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..12 {
            let limiter = limiter.clone();
            let barrier = Arc::clone(&barrier);
            let admitted = Arc::clone(&admitted);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                if limiter.try_reserve_at(ip, Instant::now()) {
                    admitted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }));
        }
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(
            admitted.load(std::sync::atomic::Ordering::SeqCst),
            MAX_LOGIN_ATTEMPTS_PER_IP
        );
    }

    #[test]
    fn per_ip_budgets_are_independent_and_refill_after_the_window() {
        let limiter = LoginLimiter::default();
        let first: IpAddr = "198.51.100.1".parse().unwrap();
        let second: IpAddr = "198.51.100.2".parse().unwrap();
        let start = Instant::now();
        for _ in 0..MAX_LOGIN_ATTEMPTS_PER_IP {
            assert!(limiter.try_reserve_at(first, start));
        }
        assert!(!limiter.has_capacity_at(first, start));
        assert!(!limiter.try_reserve_at(first, start));
        assert!(limiter.try_reserve_at(second, start));

        let just_before = start + LOGIN_ATTEMPT_WINDOW - Duration::from_millis(1);
        assert!(!limiter.try_reserve_at(first, just_before));
        let refilled = start + LOGIN_ATTEMPT_WINDOW;
        assert!(limiter.try_reserve_at(first, refilled));
        assert!(limiter.has_capacity_at(first, refilled));

        let reordered = LoginLimiter::default();
        assert!(reordered.try_reserve_at(first, refilled));
        for _ in 1..MAX_LOGIN_ATTEMPTS_PER_IP {
            assert!(reordered.try_reserve_at(first, start));
        }
        assert!(reordered.try_reserve_at(first, refilled));
    }

    #[test]
    fn limiter_record_cap_fails_closed_and_prunes_expired_entries() {
        let limiter = LoginLimiter::default();
        let start = Instant::now();
        for index in 0..MAX_LOGIN_LIMITER_RECORDS {
            let ip = IpAddr::V4(std::net::Ipv4Addr::from(index as u32 + 1));
            assert!(limiter.try_reserve_at(ip, start));
        }
        let new_ip: IpAddr = "203.0.113.250".parse().unwrap();
        assert!(!limiter.has_capacity_at(new_ip, start));
        assert!(!limiter.try_reserve_at(new_ip, start));

        let after_window = start + LOGIN_ATTEMPT_WINDOW;
        assert!(limiter.try_reserve_at(new_ip, after_window));
        assert_eq!(limiter.attempts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn global_password_slots_cap_verification_across_ips_and_release() {
        let limiter = LoginLimiter::default();
        let now = Instant::now();
        assert!(limiter.try_reserve_at("198.51.100.1".parse().unwrap(), now));
        assert!(limiter.try_reserve_at("198.51.100.2".parse().unwrap(), now));

        let first = limiter.try_acquire_password_slot().expect("first slot");
        let second = limiter.try_acquire_password_slot().expect("second slot");
        assert!(limiter.try_acquire_password_slot().is_none());
        drop(first);
        let replacement = limiter
            .try_acquire_password_slot()
            .expect("released slot");
        assert!(limiter.try_acquire_password_slot().is_none());
        drop((second, replacement));
        assert_eq!(limiter.password_slots.available_permits(), 2);
    }

    #[tokio::test]
    async fn aborting_the_http_task_keeps_the_slot_until_blocking_verification_ends() {
        let limiter = LoginLimiter::default();
        let held = limiter.try_acquire_password_slot().expect("held slot");
        let moved = limiter
            .try_acquire_password_slot()
            .expect("verification slot");
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let task = tokio::spawn(run_with_password_slot(moved, move || {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            Ok::<bool, AuthError>(true)
        }));

        started_rx.await.expect("verification should start");
        task.abort();
        let _ = task.await;
        // The HTTP task is gone, but the blocking work still owns its permit.
        assert!(limiter.try_acquire_password_slot().is_none());

        release_tx.send(()).expect("release blocking verification");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if limiter.try_acquire_password_slot().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("slot should be released when blocking verification finishes");
        drop(held);
    }

    #[test]
    fn custom_limiter_budget_applies_per_ip() {
        let limiter = LoginLimiter::with_budget(MAX_PASSKEY_STARTS_PER_IP);
        let ip = IpAddr::from([192, 0, 2, 10]);
        let now = Instant::now();
        for _ in 0..MAX_PASSKEY_STARTS_PER_IP {
            assert!(limiter.try_reserve_at(ip, now));
        }
        assert!(!limiter.try_reserve_at(ip, now));
    }
}
