use super::*;

pub(super) async fn video_feed(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // Bind the stream to the live session so logout, revocation, or expiry
    // ends it, matching the terminal and WebRTC lifecycles.
    let Some(guard) = session_token(&headers).and_then(|token| state.sessions.validate_guard(token))
    else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "需要登录").into_response();
    };
    let frames = state.video.subscribe();
    let mut response = mjpeg_body(frames, guard).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("multipart/x-mixed-replace; boundary=frame"),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    response
}

pub(super) fn mjpeg_body(
    mut frames: watch::Receiver<Option<Arc<crate::devices::video::VideoFrame>>>,
    guard: SessionGuard,
) -> Body {
    Body::from_stream(async_stream::stream! {
        loop {
            let frame = frames.borrow_and_update().clone();
            if let Some(frame) = frame {
                let header = Bytes::from(format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nX-Wingman-Sequence: {}\r\n\r\n",
                    frame.jpeg.len(), frame.sequence
                ));
                yield Ok::<Bytes, Infallible>(header);
                yield Ok::<Bytes, Infallible>(frame.jpeg.clone());
                yield Ok::<Bytes, Infallible>(Bytes::from_static(b"\r\n"));
            }
            tokio::select! {
                biased;
                _ = guard.wait_for_end() => break,
                changed = frames.changed() => if changed.is_err() { break },
            }
        }
    })
}

pub(super) async fn webrtc_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<WebRtcOfferRequest>,
) -> Result<Response, ApiError> {
    // Acquire the live guard atomically with validation. A logout that races
    // this request cannot leave an authorized peer behind, because the guard
    // is passed into the manager and supervises revocation/expiry.
    let Some(guard) = session_token(&headers)
        .and_then(|token| state.sessions.validate_guard(token))
    else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    };
    let response = state
        .webrtc
        .offer(request, guard)
        .await
        .map_err(map_webrtc_error)?;
    Ok(no_store(Json(response)).into_response())
}

#[derive(Debug, Deserialize)]
pub(super) struct WebRtcCloseRequest {
    pub(super) session_id: String,
}

pub(super) async fn webrtc_close(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<WebRtcCloseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.session_id.len() > 128 || request.session_id.is_empty() {
        return Err(ApiError::bad_request("WebRTC 会话标识不正确"));
    }
    Ok(no_store(Json(json!({
        "closed": state.webrtc.close(&request.session_id).await,
    }))))
}

#[derive(Debug, Deserialize)]
pub(super) struct WebRtcBitrateRequest {
    pub(super) session_id: String,
    pub(super) kbps: u32,
}

/// Adaptive bitrate request from the browser. The peer session must belong to
/// the caller's login session; the manager clamps the value to
/// `[256, configured bitrate]` and rate-limits encoder restarts.
pub(super) async fn webrtc_bitrate(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<WebRtcBitrateRequest>,
) -> Result<Response, ApiError> {
    apply_webrtc_bitrate(&state.sessions, &state.webrtc, &headers, &request)
}

fn apply_webrtc_bitrate(
    sessions: &SessionStore,
    webrtc: &WebRtcManager,
    headers: &HeaderMap,
    request: &WebRtcBitrateRequest,
) -> Result<Response, ApiError> {
    let Some(guard) = session_token(headers).and_then(|token| sessions.validate_guard(token)) else {
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "需要登录"));
    };
    if request.session_id.len() > 128 || request.session_id.is_empty() {
        return Err(ApiError::bad_request("WebRTC 会话标识不正确"));
    }
    let change = webrtc
        .set_bitrate(&request.session_id, request.kbps, &guard)
        .map_err(map_webrtc_error)?;
    Ok(no_store(Json(change)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}")).unwrap(),
        );
        headers
    }

    fn bitrate_request(session_id: &str, kbps: u32) -> WebRtcBitrateRequest {
        WebRtcBitrateRequest {
            session_id: session_id.to_owned(),
            kbps,
        }
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn bitrate_endpoint_requires_a_live_login() {
        let sessions = SessionStore::default();
        let webrtc = WebRtcManager::for_tests();
        let token = sessions.create().unwrap();
        let bitrate =
            webrtc.insert_test_session("peer", sessions.validate_guard(&token).unwrap(), 4_000);
        let request = bitrate_request("peer", 1_000);

        let error = apply_webrtc_bitrate(&sessions, &webrtc, &HeaderMap::new(), &request)
            .unwrap_err();
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        let error =
            apply_webrtc_bitrate(&sessions, &webrtc, &cookie_headers("forged"), &request)
                .unwrap_err();
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(sessions.revoke(&token));
        let error = apply_webrtc_bitrate(&sessions, &webrtc, &cookie_headers(&token), &request)
            .unwrap_err();
        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(*bitrate.borrow(), 4_000);
    }

    #[tokio::test]
    async fn bitrate_endpoint_clamps_and_reports_the_applied_value() {
        let sessions = SessionStore::default();
        let webrtc = WebRtcManager::for_tests();
        let token = sessions.create().unwrap();
        let bitrate =
            webrtc.insert_test_session("peer", sessions.validate_guard(&token).unwrap(), 3_000);
        let headers = cookie_headers(&token);

        let response =
            apply_webrtc_bitrate(&sessions, &webrtc, &headers, &bitrate_request("peer", 1))
                .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["kbps"], 256);
        assert_eq!(body["min_kbps"], 256);
        assert_eq!(body["max_kbps"], 3_000);
        assert_eq!(body["changed"], true);
        assert_eq!(*bitrate.borrow(), 256);

        // The next real change inside the interval is refused with 429.
        let error =
            apply_webrtc_bitrate(&sessions, &webrtc, &headers, &bitrate_request("peer", u32::MAX))
                .unwrap_err();
        assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(*bitrate.borrow(), 256);
    }

    #[tokio::test]
    async fn bitrate_endpoint_hides_unknown_and_foreign_sessions() {
        let sessions = SessionStore::default();
        let webrtc = WebRtcManager::for_tests();
        let owner = sessions.create().unwrap();
        let other = sessions.create().unwrap();
        let bitrate =
            webrtc.insert_test_session("peer", sessions.validate_guard(&owner).unwrap(), 4_000);

        for (token, session_id) in [(&owner, "missing"), (&other, "peer")] {
            let error = apply_webrtc_bitrate(
                &sessions,
                &webrtc,
                &cookie_headers(token),
                &bitrate_request(session_id, 1_000),
            )
            .unwrap_err();
            assert_eq!(error.status, StatusCode::NOT_FOUND);
        }
        let error = apply_webrtc_bitrate(
            &sessions,
            &webrtc,
            &cookie_headers(&owner),
            &bitrate_request("", 1_000),
        )
        .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(*bitrate.borrow(), 4_000);
    }

    #[tokio::test]
    async fn mjpeg_stream_ends_when_the_session_is_revoked() {
        let sessions = SessionStore::default();
        let token = sessions.create().unwrap();
        let guard = sessions.validate_guard(&token).unwrap();
        let (_frames_tx, frames) = watch::channel(None);
        let body = mjpeg_body(frames, guard);
        let reader = tokio::spawn(axum::body::to_bytes(body, usize::MAX));
        tokio::task::yield_now().await;
        assert!(sessions.revoke(&token));
        let bytes = tokio::time::timeout(Duration::from_secs(1), reader)
            .await
            .expect("stream must end after revocation")
            .unwrap()
            .unwrap();
        assert!(bytes.is_empty());
    }
}
