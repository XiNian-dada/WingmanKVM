use super::*;

pub(super) const PASSKEY_CHALLENGE_TTL: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub(super) enum PasskeyChallengeState {
    Registration {
        state: PasskeyRegistration,
        rp_id: String,
        rp_origin: Url,
        expires_at: Instant,
    },
    Authentication {
        state: PasskeyAuthentication,
        rp_id: String,
        rp_origin: Url,
        expires_at: Instant,
    },
}

/// Bound on outstanding WebAuthn challenges; unauthenticated login starts
/// must not grow server memory without limit.
pub(super) const MAX_PASSKEY_CHALLENGES: usize = 256;

pub(super) fn insert_passkey_challenge(
    challenges: &Mutex<HashMap<String, PasskeyChallengeState>>,
    challenge_id: String,
    challenge: PasskeyChallengeState,
    now: Instant,
) -> Result<(), ApiError> {
    let mut map = challenges.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, state| match state {
        PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
        PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
    });
    // Each kind has its own bound, so an unauthenticated login-start flood
    // cannot block registration by a signed-in admin. At the bound the oldest
    // challenge of that kind is evicted rather than rejecting everyone.
    let is_registration = matches!(challenge, PasskeyChallengeState::Registration { .. });
    let same_kind = |state: &PasskeyChallengeState| {
        matches!(state, PasskeyChallengeState::Registration { .. }) == is_registration
    };
    if map.values().filter(|state| same_kind(state)).count() >= MAX_PASSKEY_CHALLENGES {
        let oldest = map
            .iter()
            .filter(|(_, state)| same_kind(state))
            .min_by_key(|(_, state)| match state {
                PasskeyChallengeState::Registration { expires_at, .. }
                | PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at,
            })
            .map(|(id, _)| id.clone());
        if let Some(oldest) = oldest {
            map.remove(&oldest);
        }
    }
    map.insert(challenge_id, challenge);
    Ok(())
}

pub(super) fn prune_passkey_challenges(challenges: &Arc<Mutex<HashMap<String, PasskeyChallengeState>>>) {
    let now = Instant::now();
    let mut map = challenges.lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, state| match state {
        PasskeyChallengeState::Registration { expires_at, .. } => *expires_at > now,
        PasskeyChallengeState::Authentication { expires_at, .. } => *expires_at > now,
    });
}

pub(super) fn build_webauthn(headers: &HeaderMap) -> Result<(Webauthn, String, Url), ApiError> {
    if !origin_matches_host(headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }
    let origin_url = if let Some(origin) = headers.get(ORIGIN) {
        let origin_str = origin.to_str().map_err(|_| {
            ApiError::new(StatusCode::BAD_REQUEST, "无效的 Origin 请求标头")
        })?;
        Url::parse(origin_str).map_err(|e| {
            ApiError::new(StatusCode::BAD_REQUEST, format!("无效的 Origin URL: {e}"))
        })?
    } else if let Some(host) = headers.get(HOST) {
        let host_str = host.to_str().map_err(|_| {
            ApiError::new(StatusCode::BAD_REQUEST, "无效的 Host 请求标头")
        })?;
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|p| p.to_str().ok())
            .unwrap_or("http");
        Url::parse(&format!("{proto}://{host_str}")).map_err(|e| {
            ApiError::new(StatusCode::BAD_REQUEST, format!("无效的主机 URL: {e}"))
        })?
    } else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "缺少 Origin 或 Host 标头"));
    };

    let Some(rp_id) = origin_url.host_str() else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无法解析 Relying Party ID"));
    };

    if rp_id.parse::<IpAddr>().is_ok() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Passkey (WebAuthn) 规范要求使用域名（如 localhost 或 wingman.local），不支持直接使用 IP 地址。请使用域名访问。",
        ));
    }

    let builder = WebauthnBuilder::new(rp_id, &origin_url).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    Ok((webauthn, rp_id.to_owned(), origin_url))
}

pub(super) async fn passkey_login_start(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Starting authentication is not an authentication submission: check the
    // current budget without reserving an attempt. Finish reserves exactly one.
    if !state.login_limiter.has_capacity(client_ip) {
        return Err(too_many_login_attempts());
    }
    // Each start allocates a server-side challenge, so it has its own
    // recorded budget even though it is not an authentication submission.
    if !state.passkey_start_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }
    let Some(record) = state.auth.load_async().await.map_err(map_auth_error)? else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"));
    };
    if record.passkeys.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "尚未注册任何 Passkey"));
    }

    let (webauthn, rp_id, rp_origin) = build_webauthn(&headers)?;
    let passkeys: Vec<Passkey> = record.passkeys.iter().map(|c| c.passkey.clone()).collect();
    let (rcr, auth_state) = webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| ApiError::internal(format!("启动认证失败: {e}")))?;

    let challenge_id = generate_token()
        .map_err(|e| ApiError::internal(format!("生成 challenge 失败: {e}")))?;

    let now = Instant::now();
    let expires_at = now + PASSKEY_CHALLENGE_TTL;
    insert_passkey_challenge(
        &state.passkey_challenges,
        challenge_id.clone(),
        PasskeyChallengeState::Authentication {
            state: auth_state,
            rp_id,
            rp_origin,
            expires_at,
        },
        now,
    )?;

    Ok(no_store(Json(json!({
        "challenge_id": challenge_id,
        "options": rcr,
    }))).into_response())
}

#[derive(Deserialize)]
pub(super) struct PasskeyLoginFinishRequest {
    pub(super) challenge_id: String,
    pub(super) credential: PublicKeyCredential,
}

pub(super) async fn passkey_login_finish(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<PasskeyLoginFinishRequest>,
) -> Result<Response, ApiError> {
    if !origin_matches_host(&headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }
    let client_ip = state.client_address(connection, &headers).await.ip;
    // Completing authentication is the submitted attempt: reserve once before
    // validating the challenge. Success and every failure keep the reservation.
    if !state.login_limiter.try_reserve(client_ip) {
        return Err(too_many_login_attempts());
    }

    let challenge_state = {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.remove(&request.challenge_id)
    };

    let Some(PasskeyChallengeState::Authentication {
        state: auth_state,
        rp_id,
        rp_origin,
        expires_at,
    }) = challenge_state else {
        state
            .audit
            .record(
                AuditEvent::LoginFailed,
                Some(client_ip),
                None,
                Some("method=passkey".to_owned()),
            )
            .await;
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无效或已过期的认证挑战"));
    };

    if Instant::now() > expires_at {
        state
            .audit
            .record(
                AuditEvent::LoginFailed,
                Some(client_ip),
                None,
                Some("method=passkey".to_owned()),
            )
            .await;
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "认证挑战已过期"));
    }

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    let auth_result = match webauthn.finish_passkey_authentication(&request.credential, &auth_state) {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!(error = %e, "WebAuthn authentication failed");
            state
                .audit
                .record(
                    AuditEvent::LoginFailed,
                    Some(client_ip),
                    None,
                    Some("method=passkey".to_owned()),
                )
                .await;
            return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Passkey 验证失败"));
        }
    };

    let cred_id = request.credential.raw_id;
    // The challenge snapshots the credential set at start time; re-check that
    // the credential still exists so deleting a Passkey revokes it immediately.
    let login = state.auth.update_async(move |record| {
        Ok(record
            .update_passkey_credential(cred_id.as_slice(), &auth_result)
            .then(|| (record.username.clone(), record.system_password_synced)))
    }).await.map_err(map_auth_error)?;
    let Some((username, terminal_ready)) = login else {
        tracing::warn!("Passkey login rejected: credential was removed after the challenge started");
        state
            .audit
            .record(
                AuditEvent::LoginFailed,
                Some(client_ip),
                None,
                Some("method=passkey".to_owned()),
            )
            .await;
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "Passkey 验证失败"));
    };

    let session = state
        .sessions
        .create_with_metadata(SessionMetadata {
            client_ip: Some(client_ip),
            user_agent: client_user_agent(&headers),
            login_method: Some(LoginMethod::Passkey),
        })
        .map_err(ApiError::internal)?;
    let session_id = state.sessions.session_id(&session);
    state
        .audit
        .record(
            AuditEvent::LoginSucceeded,
            Some(client_ip),
            session_id,
            Some("method=passkey".to_owned()),
        )
        .await;
    let session_secure = state.config.read().await.server.session_cookie_secure();
    Ok(with_session_cookie(
        Json(json!({
            "ok": true,
            "username": username,
            "terminal_ready": terminal_ready,
            "terminal_message": (!terminal_ready).then_some(TERMINAL_DEGRADED_MESSAGE),
        }))
        .into_response(),
        Some(&session),
        session_secure,
    ))
}

#[derive(Deserialize)]
pub(super) struct PasskeyRegisterStartRequest {
    #[serde(default)]
    pub(super) name: Option<String>,
}

pub(super) async fn passkey_register_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<PasskeyRegisterStartRequest>,
) -> Result<Response, ApiError> {
    let Some(record) = state.auth.load_async().await.map_err(map_auth_error)? else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"));
    };

    let (webauthn, rp_id, rp_origin) = build_webauthn(&headers)?;

    let exclude_credentials = if record.passkeys.is_empty() {
        None
    } else {
        Some(record.passkeys.iter().map(|p| p.passkey.cred_id().clone()).collect())
    };

    let user_unique_id = record.user_unique_id();
    let (ccr, reg_state) = webauthn
        .start_passkey_registration(
            user_unique_id,
            &record.username,
            &record.username,
            exclude_credentials,
        )
        .map_err(|e| ApiError::internal(format!("启动注册失败: {e}")))?;

    let challenge_id = generate_token()
        .map_err(|e| ApiError::internal(format!("生成 challenge 失败: {e}")))?;

    let now = Instant::now();
    let expires_at = now + PASSKEY_CHALLENGE_TTL;
    insert_passkey_challenge(
        &state.passkey_challenges,
        challenge_id.clone(),
        PasskeyChallengeState::Registration {
            state: reg_state,
            rp_id,
            rp_origin,
            expires_at,
        },
        now,
    )?;

    Ok(no_store(Json(json!({
        "challenge_id": challenge_id,
        "options": ccr,
        "suggested_name": request.name,
    }))).into_response())
}

#[derive(Deserialize)]
pub(super) struct PasskeyRegisterFinishRequest {
    pub(super) challenge_id: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    pub(super) credential: RegisterPublicKeyCredential,
}

pub(super) async fn passkey_register_finish(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<PasskeyRegisterFinishRequest>,
) -> Result<Response, ApiError> {
    if !origin_matches_host(&headers) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "Origin 与 Host 不符"));
    }

    let challenge_state = {
        let mut challenges = state.passkey_challenges.lock().unwrap_or_else(|p| p.into_inner());
        challenges.remove(&request.challenge_id)
    };

    let Some(PasskeyChallengeState::Registration {
        state: reg_state,
        rp_id,
        rp_origin,
        expires_at,
    }) = challenge_state else {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "无效或已过期的注册挑战"));
    };

    if Instant::now() > expires_at {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "注册挑战已过期"));
    }

    let builder = WebauthnBuilder::new(&rp_id, &rp_origin).map_err(|e| {
        ApiError::new(StatusCode::BAD_REQUEST, format!("WebAuthn 配置错误: {e}"))
    })?;
    let webauthn = builder
        .rp_name("WingmanKVM")
        .build()
        .map_err(|e| ApiError::internal(format!("初始化 WebAuthn 失败: {e}")))?;

    let passkey = webauthn
        .finish_passkey_registration(&request.credential, &reg_state)
        .map_err(|e| {
            tracing::warn!(error = %e, "WebAuthn finish registration failed");
            ApiError::new(StatusCode::BAD_REQUEST, format!("Passkey 注册验证失败: {e}"))
        })?;

    let passkey_name = request.name.unwrap_or_default();
    let (id, name) = state.auth.update_async(move |record| {
        let cred = record.add_passkey(passkey_name, passkey)?;
        Ok((cred.id.clone(), cred.name.clone()))
    }).await.map_err(map_auth_error)?;

    let (ip, session) = audit_actor(&state, connection, &headers).await;
    state
        .audit
        .record(
            AuditEvent::PasskeyAdded,
            ip,
            session,
            Some(format!("id={id}")),
        )
        .await;

    Ok(Json(json!({
        "ok": true,
        "id": id,
        "name": name,
    })).into_response())
}

#[derive(Serialize)]
pub(super) struct PasskeyInfo {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) created_at_unix_seconds: u64,
}

pub(super) async fn passkey_list(State(state): State<AppState>) -> Result<Response, ApiError> {
    let record = state.auth.load_async().await.map_err(map_auth_error)?.ok_or_else(|| {
        ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化")
    })?;

    let list: Vec<PasskeyInfo> = record.passkeys.iter().map(|c| PasskeyInfo {
        id: c.id.clone(),
        name: c.name.clone(),
        created_at_unix_seconds: c.created_at_unix_seconds,
    }).collect();

    Ok(no_store(Json(json!({
        "ok": true,
        "passkeys": list,
    }))).into_response())
}

#[derive(Deserialize)]
pub(super) struct PasskeyDeleteRequest {
    pub(super) id: String,
}

pub(super) async fn passkey_delete(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<PasskeyDeleteRequest>,
) -> Result<Response, ApiError> {
    delete_passkey(state, request.id, connection, headers).await
}

pub(super) async fn passkey_delete_path(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<String>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    delete_passkey(state, id, connection, headers).await
}

async fn delete_passkey(
    state: AppState,
    id: String,
    connection: ClientConnection,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let audit_id = id.clone();
    let removed = state.auth.update_async(move |record| {
        record.remove_passkey(&id)
    }).await.map_err(map_auth_error)?;

    if !removed {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "找不到该 Passkey"));
    }

    let (ip, session) = audit_actor(&state, connection, &headers).await;
    state
        .audit
        .record(
            AuditEvent::PasskeyRemoved,
            ip,
            session,
            Some(format!("id={audit_id}")),
        )
        .await;

    Ok(Json(json!({ "ok": true })).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_webauthn_validates_domains_and_rejects_ip_addresses() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("localhost:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:8080"));
        let res = build_webauthn(&headers);
        assert!(res.is_ok());
        let (_, rp_id, _) = res.unwrap();
        assert_eq!(rp_id, "localhost");

        // Domain name
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("wingman.local:8443"));
        headers.insert(ORIGIN, HeaderValue::from_static("https://wingman.local:8443"));
        let res = build_webauthn(&headers);
        assert!(res.is_ok());
        let (_, rp_id, _) = res.unwrap();
        assert_eq!(rp_id, "wingman.local");

        // IP address should be rejected with helpful error
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("192.168.1.100:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://192.168.1.100:8080"));
        let res = build_webauthn(&headers);
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("域名"));

        // Host mismatch
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("wingman.local:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://attacker.com"));
        let res = build_webauthn(&headers);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn passkey_challenge_table_is_bounded_and_reclaims_expired_entries() {
        let challenges = Mutex::new(HashMap::new());
        let rp_origin = Url::parse("http://localhost:8080").unwrap();
        let webauthn = WebauthnBuilder::new("localhost", &rp_origin)
            .unwrap()
            .rp_name("WingmanKVM")
            .build()
            .unwrap();
        let (_ccr, reg_state) = webauthn
            .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
            .unwrap();
        let challenge = |expires_at| PasskeyChallengeState::Registration {
            state: reg_state.clone(),
            rp_id: "localhost".to_owned(),
            rp_origin: rp_origin.clone(),
            expires_at,
        };
        let now = Instant::now();
        for index in 0..MAX_PASSKEY_CHALLENGES {
            // Index 0 expires first, so it is the eviction candidate.
            let expires_at = now + Duration::from_secs(60 + index as u64);
            insert_passkey_challenge(&challenges, index.to_string(), challenge(expires_at), now).unwrap();
        }
        insert_passkey_challenge(
            &challenges,
            "overflow".to_owned(),
            challenge(now + Duration::from_secs(600)),
            now,
        )
        .expect("a full table evicts instead of rejecting");
        {
            let map = challenges.lock().unwrap();
            assert_eq!(map.len(), MAX_PASSKEY_CHALLENGES);
            assert!(!map.contains_key("0"), "oldest challenge is evicted");
            assert!(map.contains_key("overflow"));
        }

        // Once the outstanding challenges expire, they are reclaimed.
        let later = now + Duration::from_secs(601);
        insert_passkey_challenge(&challenges, "fresh".to_owned(), challenge(later + Duration::from_secs(60)), later)
            .unwrap();
        assert_eq!(challenges.lock().unwrap().len(), 1);
    }

    #[test]
    fn passkey_challenge_pruning_removes_expired_entries() {
        let challenges = Arc::new(Mutex::new(HashMap::new()));
        let now = Instant::now();
        {
            let mut map = challenges.lock().unwrap();
            let rp_origin = Url::parse("http://localhost:8080").unwrap();
            // Fresh registration
            let builder = WebauthnBuilder::new("localhost", &rp_origin).unwrap();
            let webauthn = builder.rp_name("WingmanKVM").build().unwrap();
            let (_ccr, reg_state) = webauthn
                .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
                .unwrap();
            map.insert(
                "fresh".to_owned(),
                PasskeyChallengeState::Registration {
                    state: reg_state.clone(),
                    rp_id: "localhost".to_owned(),
                    rp_origin: rp_origin.clone(),
                    expires_at: now + Duration::from_secs(60),
                },
            );
            // Expired registration
            map.insert(
                "expired".to_owned(),
                PasskeyChallengeState::Registration {
                    state: reg_state,
                    rp_id: "localhost".to_owned(),
                    rp_origin,
                    expires_at: now - Duration::from_secs(10),
                },
            );
        }

        prune_passkey_challenges(&challenges);
        let map = challenges.lock().unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("fresh"));
        assert!(!map.contains_key("expired"));
    }

    #[test]
    fn passkey_serialization_shape() {
        let rp_origin = Url::parse("http://localhost:8080").unwrap();
        let builder = WebauthnBuilder::new("localhost", &rp_origin).unwrap();
        let webauthn = builder.rp_name("WingmanKVM").build().unwrap();
        let (ccr, _) = webauthn
            .start_passkey_registration(Uuid::new_v4(), "admin", "admin", None)
            .unwrap();
        let ccr_json = serde_json::to_value(&ccr).unwrap();
        assert!(ccr_json.get("publicKey").is_some());

        let (rcr, _) = webauthn
            .start_passkey_authentication(&[])
            .unwrap();
        let rcr_json = serde_json::to_value(&rcr).unwrap();
        assert!(rcr_json.get("publicKey").is_some());

        // Test RegisterPublicKeyCredential structure
        let reg_dummy = json!({
            "id": "AAECAw",
            "rawId": "AAECAw",
            "response": {
                "clientDataJSON": "AAECAw",
                "attestationObject": "AAECAw"
            },
            "type": "public-key",
            "extensions": {}
        });
        let reg_deser: Result<RegisterPublicKeyCredential, _> = serde_json::from_value(reg_dummy);
        assert!(reg_deser.is_ok());

        // Test PublicKeyCredential structure
        let auth_dummy = json!({
            "id": "AAECAw",
            "rawId": "AAECAw",
            "response": {
                "clientDataJSON": "AAECAw",
                "authenticatorData": "AAECAw",
                "signature": "AAECAw",
                "userHandle": null
            },
            "type": "public-key",
            "extensions": {}
        });
        let auth_deser: Result<PublicKeyCredential, _> = serde_json::from_value(auth_dummy);
        assert!(auth_deser.is_ok());
    }
}
