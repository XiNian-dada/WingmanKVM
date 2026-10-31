use super::*;

pub(super) const SESSION_COOKIE: &str = "wingman_session";

pub(super) const SESSION_COOKIE_MAX_AGE_SECONDS: u32 = 43_200;

pub(super) async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_authenticated(&state, request.headers()) {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    }
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && !origin_matches_host(request.headers())
    {
        return ApiError::new(StatusCode::FORBIDDEN, "请求来源与当前主机不一致").into_response();
    }
    next.run(request).await
}

pub(super) fn is_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    session_token(headers)
        .and_then(|token| state.sessions.validate(token))
        .is_some()
}

pub(super) fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| cookie.strip_prefix(&format!("{SESSION_COOKIE}=")))
}

pub(super) fn origin_matches_host(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        return false;
    };
    let Some(origin_authority) = uri.authority() else {
        return false;
    };
    headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| origin_authority.as_str().eq_ignore_ascii_case(host))
}

pub(super) fn generate_token() -> Result<String, getrandom::Error> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(super) fn constant_time_eq(expected: &str, actual: &str) -> bool {
    let expected = expected.as_bytes();
    let actual = actual.as_bytes();
    let mut difference = expected.len() ^ actual.len();
    for index in 0..expected.len().max(actual.len()) {
        difference |= usize::from(
            expected.get(index).copied().unwrap_or_default()
                ^ actual.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

pub(super) fn session_cookie_value(token: Option<&str>, secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}={}", token.unwrap_or_default());
    cookie.push_str("; HttpOnly; SameSite=Strict; Path=/");
    if secure {
        cookie.push_str("; Secure");
    }
    let max_age = if token.is_some() {
        SESSION_COOKIE_MAX_AGE_SECONDS
    } else {
        0
    };
    cookie.push_str(&format!("; Max-Age={max_age}"));
    cookie
}

pub(super) fn with_session_cookie(
    mut response: Response,
    token: Option<&str>,
    secure: bool,
) -> Response {
    if let Ok(value) = HeaderValue::from_str(&session_cookie_value(token, secure)) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

pub(super) fn no_store<T: IntoResponse>(value: T) -> Response {
    let mut response = value.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Resolve the acting client IP and public session id for an audit entry.
pub(super) async fn audit_actor(
    state: &AppState,
    connection: ClientConnection,
    headers: &HeaderMap,
) -> (Option<IpAddr>, Option<String>) {
    let ip = Some(state.client_address(connection, headers).await.ip);
    let session = session_token(headers).and_then(|token| state.sessions.session_id(token));
    (ip, session)
}

pub(super) fn client_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[derive(Deserialize)]
pub(super) struct RevokeSessionRequest {
    pub(super) id: String,
}

#[derive(Serialize)]
pub(super) struct SessionListItem {
    #[serde(flatten)]
    pub(super) summary: SessionSummary,
    pub(super) current: bool,
}

pub(super) async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let current = session_token(&headers).and_then(|token| state.sessions.session_id(token));
    let sessions: Vec<_> = state
        .sessions
        .list()
        .into_iter()
        .map(|summary| SessionListItem {
            current: current.as_deref() == Some(summary.id.as_str()),
            summary,
        })
        .collect();
    no_store(Json(json!({ "sessions": sessions }))).into_response()
}

pub(super) async fn revoke_session(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<RevokeSessionRequest>,
) -> Result<Response, ApiError> {
    if request.id.is_empty() || request.id.len() > 128 {
        return Err(ApiError::bad_request("会话标识不正确"));
    }
    let (ip, current_id) = audit_actor(&state, connection, &headers).await;
    let revoking_self = current_id.as_deref() == Some(request.id.as_str());
    if !state.sessions.revoke_by_id(&request.id) {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "会话不存在或已下线"));
    }
    state
        .audit
        .record(
            AuditEvent::SessionRevoked,
            ip,
            Some(request.id.clone()),
            Some(if revoking_self {
                "self".to_owned()
            } else {
                "other".to_owned()
            }),
        )
        .await;

    let response = Json(json!({ "ok": true })).into_response();
    if revoking_self {
        // Revoking the acting session is a logout: clear the HttpOnly cookie
        // in the same response so the browser cannot keep presenting it.
        let session_secure = state.config.read().await.server.session_cookie_secure();
        Ok(with_session_cookie(response, None, session_secure))
    } else {
        Ok(no_store(response))
    }
}

pub(super) async fn revoke_other_sessions(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(token) = session_token(&headers) else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    };
    let Some(current_id) = state.sessions.session_id(token) else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    };
    let ip = Some(state.client_address(connection, &headers).await.ip);
    let revoked = state.sessions.revoke_all_except(token);
    state
        .audit
        .record(
            AuditEvent::SessionsRevokedOthers,
            ip,
            Some(current_id),
            Some(format!("count={revoked}")),
        )
        .await;
    Ok(no_store(Json(json!({ "ok": true, "revoked": revoked }))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_token_comparison_checks_length_and_contents() {
        assert!(constant_time_eq("correct-token", "correct-token"));
        assert!(!constant_time_eq("correct-token", "wrong-token"));
        assert!(!constant_time_eq("correct-token", "correct-token-extra"));
    }

    #[test]
    fn session_cookie_issuance_and_deletion_share_attributes() {
        let issued = session_cookie_value(Some("token"), false);
        assert_eq!(
            issued,
            "wingman_session=token; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200"
        );
        assert!(!issued.contains("; Secure"));

        let issued_secure = session_cookie_value(Some("token"), true);
        assert_eq!(
            issued_secure,
            "wingman_session=token; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=43200"
        );

        let deleted = session_cookie_value(None, false);
        assert_eq!(
            deleted,
            "wingman_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
        );
        let deleted_secure = session_cookie_value(None, true);
        assert_eq!(
            deleted_secure,
            "wingman_session=; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=0"
        );

        let response = with_session_cookie(StatusCode::OK.into_response(), None, true);
        assert_eq!(
            response.headers().get(SET_COOKIE).unwrap(),
            deleted_secure.as_str()
        );
    }

    #[test]
    fn session_cookie_policy_ignores_host_origin_and_forwarding_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("kvm.example.com"));
        headers.insert("origin", HeaderValue::from_static("https://kvm.example.com"));
        headers.insert(
            "x-forwarded-proto",
            HeaderValue::from_static("https"),
        );
        headers.insert("x-forwarded-ssl", HeaderValue::from_static("on"));
        headers.insert("forwarded", HeaderValue::from_static("proto=https"));

        let mut server = config::ServerConfig::default();
        assert!(!server.session_cookie_secure());
        assert!(
            !session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );

        server.public_origin = Some("http://kvm.example.com".to_owned());
        assert!(!server.session_cookie_secure());
        assert!(
            !session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );

        // HTTPS stays HTTPS even when every request header tries to downgrade
        // it. The headers above are never an input to this decision.
        server.public_origin = Some("https://kvm.example.com".to_owned());
        headers.insert(
            "x-forwarded-proto",
            HeaderValue::from_static("http"),
        );
        assert!(server.session_cookie_secure());
        assert!(
            session_cookie_value(Some("token"), server.session_cookie_secure())
                .contains("; Secure")
        );
    }

    #[test]
    fn origin_must_match_host_when_present() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("kvm.local:8080"));
        headers.insert(ORIGIN, HeaderValue::from_static("http://kvm.local:8080"));
        assert!(origin_matches_host(&headers));
        headers.insert(ORIGIN, HeaderValue::from_static("http://evil.invalid"));
        assert!(!origin_matches_host(&headers));
    }
}
