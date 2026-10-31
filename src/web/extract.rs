use super::*;

use axum::extract::rejection::{FormRejection, JsonRejection, PathRejection};
use axum::extract::multipart::MultipartRejection;
use axum::extract::{FromRequest, FromRequestParts};
use axum::http::request::Parts;

/// `axum::Json` with rejections mapped to the unified `{ "error": ... }` body.
///
/// The original rejection status is preserved so clients still see 400 for
/// malformed JSON, 415 for a missing content type and 422 for type mismatches.
pub(super) struct ApiJson<T>(pub(super) T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(rejection)),
        }
    }
}

/// `axum::Form` with the same unified JSON error shape as [`ApiJson`].
pub(super) struct ApiForm<T>(pub(super) T);

impl<T, S> FromRequest<S> for ApiForm<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Form::<T>::from_request(req, state).await {
            Ok(Form(value)) => Ok(Self(value)),
            Err(rejection) => Err(form_rejection(rejection)),
        }
    }
}

/// `axum::Path` with the same unified JSON error shape as [`ApiJson`].
pub(super) struct ApiPath<T>(pub(super) T);

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AxumPath::<T>::from_request_parts(parts, state).await {
            Ok(AxumPath(value)) => Ok(Self(value)),
            Err(rejection) => Err(path_rejection(rejection)),
        }
    }
}

/// `axum::Multipart` with the same unified JSON error shape as [`ApiJson`].
pub(super) struct ApiMultipart(pub(super) Multipart);

impl<S> FromRequest<S> for ApiMultipart
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Multipart::from_request(req, state).await {
            Ok(multipart) => Ok(Self(multipart)),
            Err(rejection) => Err(multipart_rejection(rejection)),
        }
    }
}

fn json_rejection(rejection: JsonRejection) -> ApiError {
    let message = match &rejection {
        JsonRejection::JsonDataError(_) => "请求体 JSON 字段类型或格式不正确",
        JsonRejection::JsonSyntaxError(_) => "请求体不是有效的 JSON",
        JsonRejection::MissingJsonContentType(_) => "请求头 Content-Type 必须为 application/json",
        _ => "无法解析 JSON 请求体",
    };
    ApiError::new(rejection.status(), message)
}

fn form_rejection(rejection: FormRejection) -> ApiError {
    let message = match &rejection {
        FormRejection::InvalidFormContentType(_) => {
            "请求头 Content-Type 必须为 application/x-www-form-urlencoded"
        }
        FormRejection::FailedToDeserializeForm(_)
        | FormRejection::FailedToDeserializeFormBody(_) => "请求体不是有效的表单",
        _ => "无法解析表单请求体",
    };
    ApiError::new(rejection.status(), message)
}

fn path_rejection(rejection: PathRejection) -> ApiError {
    let message = match &rejection {
        PathRejection::FailedToDeserializePathParams(_) => "路径参数格式不正确",
        PathRejection::MissingPathParams(_) => "缺少路径参数",
        _ => "无法解析路径参数",
    };
    ApiError::new(rejection.status(), message)
}

fn multipart_rejection(rejection: MultipartRejection) -> ApiError {
    ApiError::new(rejection.status(), "无法解析 multipart 请求体")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn malformed_json_rejection_keeps_status_and_returns_json_error_body() {
        let request = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"duration":"#))
            .unwrap();

        let rejection = ApiJson::<PowerForm>::from_request(request, &())
            .await
            .err()
            .expect("malformed JSON must be rejected");
        assert_eq!(rejection.status, StatusCode::BAD_REQUEST);

        let response = rejection.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"], "请求体不是有效的 JSON");
    }

    #[tokio::test]
    async fn form_rejection_uses_json_error_body() {
        let request = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("duration=not-a-number"))
            .unwrap();

        let rejection = ApiForm::<PowerForm>::from_request(request, &())
            .await
            .err()
            .expect("invalid form must be rejected");
        assert_eq!(rejection.status, StatusCode::UNPROCESSABLE_ENTITY);
        let response = rejection.into_response();
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }
}
