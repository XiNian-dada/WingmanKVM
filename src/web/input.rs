use super::*;

// HTTP handlers and the input WebSocket share these operations so both
// transports enforce identical pointer-mode checks and error mapping.

pub(super) async fn send_key(state: &AppState, request: KeyRequest) -> Result<serde_json::Value, ApiError> {
    let path = state.config.read().await.hid.keyboard_device.clone();
    state.hid.key(path, request).await.map_err(map_hid_error)?;
    Ok(json!({ "keyboard_leds": state.hid.leds() }))
}

pub(super) async fn send_mouse_move(state: &AppState, request: MouseMoveRequest) -> Result<(), ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state.hid.mouse_move(path, request).await.map_err(map_hid_error)
}

pub(super) async fn send_mouse_absolute(
    state: &AppState,
    request: AbsolutePointerRequest,
) -> Result<(), ApiError> {
    let config = state.config.read().await.hid.clone();
    if resolved_pointer_mode(&config) != PointerMode::Absolute {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "当前配置未启用绝对指针模式",
        ));
    }
    state
        .hid
        .mouse_absolute(config.absolute_pointer_device, request)
        .await
        .map_err(map_hid_error)
}

pub(super) async fn send_mouse_click(state: &AppState, request: MouseClickRequest) -> Result<(), ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state.hid.mouse_click(path, request).await.map_err(map_hid_error)
}

pub(super) async fn send_mouse_scroll(state: &AppState, request: MouseScrollRequest) -> Result<(), ApiError> {
    let config = state.config.read().await.hid.clone();
    let path = relative_mouse_path(&config)?;
    state.hid.mouse_scroll(path, request).await.map_err(map_hid_error)
}

pub(super) async fn send_release_all(state: &AppState) -> Result<(), ApiError> {
    let config = state.config.read().await.hid.clone();
    state
        .hid
        .release_all(
            config.keyboard_device,
            config.mouse_device,
            config.absolute_pointer_device,
        )
        .await
        .map_err(map_hid_error)
}

pub(super) async fn key(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<KeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(no_store(Json(send_key(&state, request).await?)))
}

pub(super) async fn get_keyboard_leds(State(state): State<AppState>) -> impl IntoResponse {
    no_store(Json(state.hid.leds()))
}

pub(super) async fn mouse_move(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<MouseMoveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    send_mouse_move(&state, request).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn mouse_absolute(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<AbsolutePointerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    send_mouse_absolute(&state, request).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn mouse_click(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<MouseClickRequest>,
) -> Result<impl IntoResponse, ApiError> {
    send_mouse_click(&state, request).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn mouse_scroll(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<MouseScrollRequest>,
) -> Result<impl IntoResponse, ApiError> {
    send_mouse_scroll(&state, request).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn release_all(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    send_release_all(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn resolved_pointer_mode(config: &HidConfig) -> PointerMode {
    match config.pointer_mode {
        PointerMode::Auto if config.absolute_pointer_device.is_some() => PointerMode::Absolute,
        PointerMode::Auto => PointerMode::Relative,
        mode => mode,
    }
}

pub(super) fn relative_mouse_path(config: &HidConfig) -> Result<Option<PathBuf>, ApiError> {
    if resolved_pointer_mode(config) != PointerMode::Relative {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "当前配置未启用相对鼠标模式",
        ));
    }
    Ok(config.mouse_device.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::hid::KeyboardLeds;

    #[test]
    fn absolute_pointer_api_payloads_use_an_explicit_action_tag() {
        let request: AbsolutePointerRequest = serde_json::from_value(serde_json::json!({
            "action": "click",
            "x": 32767,
            "y": 0,
            "button": 1
        }))
        .unwrap();
        assert_eq!(
            request,
            AbsolutePointerRequest::Click {
                x: 32767,
                y: 0,
                button: 1
            }
        );
        assert!(
            serde_json::from_value::<AbsolutePointerRequest>(serde_json::json!({
                "x": 1,
                "y": 2
            }))
            .is_err()
        );
    }

    #[tokio::test]
    async fn keyboard_leds_serialization_and_endpoint_shape() {
        let hid = HidManager::new();
        hid.set_leds_raw(0x02);
        assert_eq!(hid.leds().caps_lock, true);
        assert_eq!(hid.leds().num_lock, false);
        assert_eq!(hid.leds().scroll_lock, false);

        let leds_json = serde_json::to_value(hid.leds()).unwrap();
        assert_eq!(leds_json["caps_lock"], true);
        assert_eq!(leds_json["num_lock"], false);
        assert_eq!(leds_json["scroll_lock"], false);
        assert_eq!(leds_json["raw"], 2);

        let parsed: KeyboardLeds = serde_json::from_value(leds_json).unwrap();
        assert_eq!(parsed, hid.leds());

        let status_json = json!({
            "hid": {
                "keyboard_leds": hid.leds(),
            }
        });
        assert_eq!(status_json["hid"]["keyboard_leds"]["caps_lock"], true);
    }
}
