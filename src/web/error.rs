use super::*;

pub(super) fn map_auth_error(error: AuthError) -> ApiError {
    match error {
        AuthError::WeakPassword(policy) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            password_policy_message(policy),
        ),
        AuthError::InvalidUsername => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "账号需为 3–64 位，仅使用英文字母、数字、点、下划线或连字符",
        ),
        AuthError::AlreadyInitialized => ApiError::new(StatusCode::CONFLICT, error.to_string()),
        AuthError::NotInitialized => ApiError::new(StatusCode::BAD_REQUEST, "系统尚未初始化"),
        AuthError::PasskeyAlreadyExists => ApiError::new(StatusCode::CONFLICT, "此 Passkey 凭据已存在"),
        AuthError::InvalidPasskey(message) => ApiError::new(StatusCode::BAD_REQUEST, message),
        _ => ApiError::internal(error),
    }
}

pub(super) fn password_policy_message(error: PasswordPolicyError) -> &'static str {
    match error {
        PasswordPolicyError::TooShort { .. }
        | PasswordPolicyError::MissingUppercase
        | PasswordPolicyError::MissingLowercase
        | PasswordPolicyError::MissingNumber
        | PasswordPolicyError::MissingSymbol => {
            "密码至少 12 位，并包含大写字母、小写字母、数字和符号"
        }
        PasswordPolicyError::TooLong { .. } => "密码过长",
        PasswordPolicyError::ControlCharacter => "密码不能包含控制字符",
    }
}

pub(super) fn map_hid_error(error: HidError) -> ApiError {
    let status = match error {
        HidError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
        HidError::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        HidError::UnsupportedKey(_)
        | HidError::InvalidButton(_)
        | HidError::InvalidAbsoluteCoordinates { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        HidError::NotConfigured | HidError::Io { .. } | HidError::ShortWrite { .. } => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        HidError::WorkerStopped => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ApiError::new(status, error.to_string())
}

pub(super) fn map_power_error(error: PowerError) -> ApiError {
    let status = match error {
        PowerError::Busy => StatusCode::CONFLICT,
        PowerError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        PowerError::Read(_) => StatusCode::SERVICE_UNAVAILABLE,
        PowerError::Spawn(_) | PowerError::UnsupportedVersion(_) => StatusCode::SERVICE_UNAVAILABLE,
        PowerError::WorkerStopped => StatusCode::INTERNAL_SERVER_ERROR,
    };
    ApiError::new(status, error.to_string())
}

pub(super) fn map_media_error(error: MediaError) -> ApiError {
    let status = match error {
        MediaError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
        MediaError::InvalidName
        | MediaError::OutsideStorage
        | MediaError::UnsupportedType
        | MediaError::NotRegularFile
        | MediaError::EmptyImage => StatusCode::UNPROCESSABLE_ENTITY,
        MediaError::ImageNotFound => StatusCode::NOT_FOUND,
        MediaError::ForceEjectUnsupported
        | MediaError::AlreadyAttached(_)
        | MediaError::ReadOnlyFallback => StatusCode::CONFLICT,
        MediaError::IoTimedOut(_) | MediaError::StateTimedOut(_) => StatusCode::GATEWAY_TIMEOUT,
        MediaError::MissingLunAttribute(_) | MediaError::LunAttributeRejected(_) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        MediaError::Io(_) => StatusCode::SERVICE_UNAVAILABLE,
    };
    ApiError::new(status, error.to_string())
}

pub(super) fn map_webrtc_error(error: WebRtcError) -> ApiError {
    let status = match &error {
        WebRtcError::InvalidOffer(_) => StatusCode::UNPROCESSABLE_ENTITY,
        WebRtcError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        WebRtcError::Capacity => StatusCode::TOO_MANY_REQUESTS,
        WebRtcError::Unauthorized => StatusCode::UNAUTHORIZED,
        WebRtcError::SessionNotFound => StatusCode::NOT_FOUND,
        WebRtcError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
        WebRtcError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if matches!(&error, WebRtcError::Internal(_)) {
        tracing::error!(%error, "WebRTC request failed");
    }
    ApiError::new(status, error.to_string())
}

#[derive(Debug)]
pub(super) struct ApiError {
    pub(super) status: StatusCode,
    pub(super) message: String,
}

impl ApiError {
    pub(super) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(super) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub(super) fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "服务器内部错误")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_media_timeouts_map_to_gateway_timeout() {
        assert_eq!(
            map_media_error(MediaError::StateTimedOut("detached")).status,
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            map_media_error(MediaError::ForceEjectUnsupported).status,
            StatusCode::CONFLICT
        );
    }
}
