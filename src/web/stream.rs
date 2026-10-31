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

#[cfg(test)]
mod tests {
    use super::*;

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
