use super::*;
use crate::config::{NormalizedOrigin, TlsMode};

pub(super) async fn require_https(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.tls.enabled() {
        return next.run(request).await;
    }
    let connection = request
        .extensions()
        .get::<ConnectInfo<ClientConnection>>()
        .map(|c| c.0);
    if connection.is_some_and(|c| c.tls) {
        return next.run(request).await;
    }
    // The transaction installer and doctor probe the local listener without
    // credentials. Only this non-sensitive endpoint remains available locally.
    if matches!(*request.method(), Method::GET | Method::HEAD)
        && request.uri().path() == "/healthz"
        && connection.is_some_and(|c| {
            c.client_addr.ip().is_loopback()
                && c.transport_peer.is_some_and(|p| p.ip().is_loopback())
        })
    {
        return next.run(request).await;
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return ApiError::new(StatusCode::UPGRADE_REQUIRED, "请通过 HTTPS 访问此接口")
            .into_response();
    }
    let configured = state.config.read().await.server.public_origin.clone();
    let origin = configured.or_else(|| {
        request
            .headers()
            .get(HOST)
            .and_then(|v| v.to_str().ok())
            .map(|host| format!("https://{host}"))
    });
    let Some(origin) = origin.filter(|s| NormalizedOrigin::parse(s).is_ok_and(|o| !o.is_http()))
    else {
        return ApiError::bad_request("无效的 HTTPS 访问地址").into_response();
    };
    let suffix = request.uri().path_and_query().map_or("/", |p| p.as_str());
    let Ok(location) = HeaderValue::from_str(&format!("{}{suffix}", origin.trim_end_matches('/')))
    else {
        return ApiError::bad_request("无效的访问地址").into_response();
    };
    (
        StatusCode::TEMPORARY_REDIRECT,
        [
            (LOCATION, location),
            (CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
    )
        .into_response()
}

async fn tls_status_json(state: &AppState) -> serde_json::Value {
    let server = state.config.read().await.server.clone();
    json!({
        "mode": server.tls.mode,
        "enabled": state.tls.enabled(),
        "public_origin": server.public_origin,
        "custom_configured": server.tls.custom_certificate.is_some(),
        "certificate": state.tls.current().map(|identity| identity.certificate.clone()),
    })
}

pub(super) async fn tls_status(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(tls_status_json(&state).await))
}

pub(super) async fn download_certificate(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let identity = state
        .tls
        .current()
        .ok_or_else(|| ApiError::bad_request("尚未启用 HTTPS"))?;
    Ok((
        [
            (
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-pem-file"),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=wingmankvm-certificate.pem"),
            ),
            (CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        identity.certificate_pem.clone(),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TlsUpdate {
    mode: TlsMode,
    public_origin: String,
    certificate_pem: Option<String>,
    private_key_pem: Option<String>,
    #[serde(default)]
    regenerate: bool,
}

pub(super) async fn update_tls(
    State(state): State<AppState>,
    ConnectInfo(connection): ConnectInfo<ClientConnection>,
    headers: HeaderMap,
    ApiJson(update): ApiJson<TlsUpdate>,
) -> Result<Response, ApiError> {
    if update.mode == TlsMode::Disabled {
        return Err(ApiError::bad_request(
            "网页仅支持自签或自定义 HTTPS 证书；HTTP 兼容模式须在主机配置",
        ));
    }
    let origin = NormalizedOrigin::parse(&update.public_origin).map_err(ApiError::bad_request)?;
    if origin.is_http() {
        return Err(ApiError::bad_request("访问地址必须以 https:// 开头"));
    }
    if update.certificate_pem.is_some() != update.private_key_pem.is_some() {
        return Err(ApiError::bad_request("证书和私钥必须一起提供"));
    }
    if update.mode != TlsMode::Custom && update.certificate_pem.is_some() {
        return Err(ApiError::bad_request("自动自签模式不接受证书文件"));
    }
    if update.private_key_pem.is_some() && !connection.tls {
        // A legacy HTTP deployment can enable a generated certificate first.
        // Never transport a user's reusable private key over that HTTP entry.
        return Err(ApiError::bad_request(
            "请先启用自动自签 HTTPS，再通过 HTTPS 导入证书和私钥",
        ));
    }
    run_to_completion(async move {
        let _guard = state.config_update.lock().await;
        let previous = state.config.read().await.clone();
        let mut config = previous.clone();
        config.server.public_origin =
            Some(update.public_origin.trim().trim_end_matches('/').to_owned());
        config.server.tls.mode = update.mode;
        let mut created = None;
        if let (Some(certificate), Some(key)) = (update.certificate_pem, update.private_key_pem) {
            let (manager, server) = (state.tls.clone(), config.server.clone());
            let id =
                tokio::task::spawn_blocking(move || manager.import(&certificate, &key, &server))
                    .await
                    .map_err(ApiError::internal)?
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
            config.server.tls.custom_certificate = Some(id.clone());
            created = Some(id);
        } else if update.mode == TlsMode::SelfSigned
            && (update.regenerate || previous.server.public_origin != config.server.public_origin)
        {
            let (manager, server) = (state.tls.clone(), config.server.clone());
            let id = tokio::task::spawn_blocking(move || manager.create_self_signed(&server))
                .await
                .map_err(ApiError::internal)?
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            config.server.tls.self_signed_certificate = Some(id.clone());
            created = Some(id);
        }
        if let Err(error) = commit_config(&state, &config).await {
            if let Some(id) = created {
                state.tls.discard_unpublished(&id);
            }
            return Err(error);
        }
        audit_config_change(&state, connection, &headers, &previous, &config).await;
        let mut status = tls_status_json(&state).await;
        let reauthenticate =
            !previous.server.session_cookie_secure() && config.server.session_cookie_secure();
        status["reauthenticate"] = json!(reauthenticate);
        let response = no_store(Json(status)).into_response();
        Ok(if reauthenticate {
            // Expire the old plaintext cookie on this response; all old sessions
            // were revoked by commit_config before publishing the new policy.
            with_session_cookie(response, None, false)
        } else {
            response
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::tests::{Directory, pem_pair};

    fn client(state: &AppState) -> reqwest::Client {
        let pem = state.tls.current().unwrap().certificate_pem.clone();
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(reqwest::Certificate::from_pem(pem.as_bytes()).unwrap())
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn tls_http_migration_revokes_sessions_and_failed_save_keeps_http_configuration() {
        let directory = Directory::new();
        let path = directory.0.join("config.json");
        std::fs::create_dir(&path).unwrap(); // A directory makes config publication fail.
        let mut config = Config::default();
        config.server = config::ServerConfig::legacy_http();
        let state = config_api::tests::test_state(config.clone(), path.clone());
        let token = state.sessions.create().unwrap();
        let connection = ClientConnection {
            tls: false,
            client_addr: "127.0.0.1:8081".parse().unwrap(),
            transport_peer: Some("127.0.0.1:8081".parse().unwrap()),
        };
        let update = || TlsUpdate {
            mode: TlsMode::SelfSigned,
            public_origin: "https://localhost:38319".into(),
            certificate_pem: None,
            private_key_pem: None,
            regenerate: false,
        };
        assert!(
            update_tls(
                State(state.clone()),
                ConnectInfo(connection),
                HeaderMap::new(),
                ApiJson(update())
            )
            .await
            .is_err()
        );
        assert!(!state.tls.enabled());
        assert_eq!(*state.config.read().await, config);
        assert!(state.sessions.validate(&token).is_some());
        assert_eq!(
            std::fs::read_dir(directory.0.join("tls")).unwrap().count(),
            0
        );
        std::fs::remove_dir(&path).unwrap();
        let response = update_tls(
            State(state.clone()),
            ConnectInfo(connection),
            HeaderMap::new(),
            ApiJson(update()),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert!(state.sessions.validate(&token).is_none());
        assert!(state.tls.enabled());
        assert!(Config::load(&path).unwrap().server.session_cookie_secure());
        let mut insecure_upload = update();
        insecure_upload.mode = TlsMode::Custom;
        insecure_upload.certificate_pem = Some("test certificate".into());
        insecure_upload.private_key_pem = Some("test key".into());
        let error = update_tls(
            State(state.clone()),
            ConnectInfo(connection),
            HeaderMap::new(),
            ApiJson(insecure_upload),
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("HTTPS"));
        assert_eq!(
            std::fs::read_dir(directory.0.join("tls")).unwrap().count(),
            1
        );
    }

    async fn wss_status(
        state: &AppState,
        address: SocketAddr,
        origin: &str,
        session: &str,
    ) -> StatusCode {
        use tokio_rustls::rustls::{self, pki_types::CertificateDer};
        use tokio_tungstenite::tungstenite::{Error, client::IntoClientRequest};
        let certificate = state.tls.current().unwrap().certificate_pem.clone();
        let cert = openssl::x509::X509::from_pem(certificate.as_bytes()).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(CertificateDer::from(cert.to_der().unwrap()))
            .unwrap();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect("localhost".try_into().unwrap(), tcp)
            .await
            .unwrap();
        let mut request = format!("wss://localhost:{}/api/input/ws", address.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        request
            .headers_mut()
            .insert(COOKIE, HeaderValue::from_str(session).unwrap());
        match tokio_tungstenite::client_async(request, tls).await {
            Ok((mut socket, response)) => {
                socket.close(None).await.unwrap();
                response.status()
            }
            Err(Error::Http(response)) => response.status(),
            Err(error) => panic!("WSS failed: {error}"),
        }
    }

    #[tokio::test]
    async fn https_settings_require_auth_reject_bad_keys_and_switch_without_losing_certificates() {
        let directory = Directory::new();
        let state =
            config_api::tests::test_state(Config::default(), directory.0.join("config.json"));
        state.tls.activate(
            state
                .tls
                .prepare(&state.config.read().await.server)
                .unwrap(),
        );
        let initial = state
            .tls
            .current()
            .unwrap()
            .certificate
            .fingerprint_sha256
            .clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let origin = format!("https://localhost:{}", address.port());
        let listener = crate::proxy_protocol::ProxyProtocolListener::new(
            listener,
            ProxyProtocolMode::Disabled,
        );
        let listener = crate::tls::TlsListener::new(
            crate::client_ip::ClientIpListener(listener),
            state.tls.clone(),
        )
        .unwrap();
        use axum::serve::ListenerExt;
        let app = router(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<ClientConnection>(),
            )
            .await
            .unwrap();
        });
        let session = format!("wingman_session={}", state.sessions.create().unwrap());
        let https = client(&state);
        // New installs have no public_origin yet: the verified TLS transport
        // allows the matching HTTPS Origin, while HTTP and foreign Origins fail.
        assert_eq!(
            wss_status(&state, address, &origin, &session).await,
            StatusCode::SWITCHING_PROTOCOLS
        );
        assert_eq!(
            wss_status(
                &state,
                address,
                &origin.replace("https:", "http:"),
                &session
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            wss_status(&state, address, "https://elsewhere.example", &session).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            https
                .get(format!("{origin}/api/tls"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let status = https
            .get(format!("{origin}/api/tls"))
            .header(COOKIE, &session)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["mode"], "self_signed");
        assert!(!status.to_string().contains("PRIVATE KEY"));
        let certificate = https
            .get(format!("{origin}/api/tls/certificate"))
            .header(COOKIE, &session)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(certificate.contains("BEGIN CERTIFICATE"));
        assert!(!certificate.contains("PRIVATE KEY"));
        let http = format!("http://localhost:{}", address.port());
        assert_eq!(
            https
                .get(format!("{http}/healthz"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let redirect = https.get(format!("{http}/")).send().await.unwrap();
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(redirect.headers()[LOCATION], format!("{origin}/"));
        assert_eq!(
            https
                .post(format!("{http}/api/tls"))
                .header(COOKIE, &session)
                .header(CONTENT_TYPE, "application/json")
                .body(json!({"mode":"self_signed","public_origin":origin}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UPGRADE_REQUIRED
        );

        let cert_server = config::ServerConfig {
            public_origin: Some(origin.clone()),
            ..Default::default()
        };
        let (pem, key) = pem_pair(&cert_server);
        let (_, wrong_key) = pem_pair(&cert_server);
        let response = https.post(format!("{origin}/api/tls")).header(COOKIE, &session).header(ORIGIN, &origin)
            .header(CONTENT_TYPE, "application/json").body(json!({"mode":"custom","public_origin":origin,"certificate_pem":pem,"private_key_pem":wrong_key}).to_string()).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            state.tls.current().unwrap().certificate.fingerprint_sha256,
            initial
        );
        let response = https.post(format!("{origin}/api/tls")).header(COOKIE, &session).header(ORIGIN, &origin)
            .header(CONTENT_TYPE, "application/json").body(json!({"mode":"custom","public_origin":origin,"certificate_pem":pem,"private_key_pem":key}).to_string()).send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let custom = state
            .tls
            .current()
            .unwrap()
            .certificate
            .fingerprint_sha256
            .clone();
        assert_ne!(initial, custom);
        // A fresh connection must use the newly installed identity, with normal
        // certificate verification (no accept-invalid-certificates flag).
        let https = client(&state);
        let response = https
            .post(format!("{origin}/api/tls"))
            .header(COOKIE, &session)
            .header(ORIGIN, &origin)
            .header(CONTENT_TYPE, "application/json")
            .body(json!({"mode":"self_signed","public_origin":origin}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            state.tls.current().unwrap().certificate.fingerprint_sha256,
            initial
        );
        let https = client(&state);
        let response = https
            .post(format!("{origin}/api/tls"))
            .header(COOKIE, &session)
            .header(ORIGIN, &origin)
            .header(CONTENT_TYPE, "application/json")
            .body(json!({"mode":"custom","public_origin":origin}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            state.tls.current().unwrap().certificate.fingerprint_sha256,
            custom
        );
        let saved = Config::load(directory.0.join("config.json")).unwrap();
        let reloaded = crate::tls::TlsManager::new(&directory.0)
            .prepare(&saved.server)
            .unwrap()
            .unwrap();
        assert_eq!(reloaded.certificate.fingerprint_sha256, custom);
        assert!(
            !std::fs::read_to_string(directory.0.join("config.json"))
                .unwrap()
                .contains("PRIVATE KEY")
        );
        server.abort();
    }
}
