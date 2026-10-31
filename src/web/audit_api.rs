use super::*;

use crate::audit::clamp_limit;

#[derive(Deserialize)]
pub(super) struct AuditQuery {
    pub(super) limit: Option<String>,
}

pub(super) async fn audit_log(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Response, ApiError> {
    let limit = clamp_limit(query.limit.as_deref().and_then(|value| value.parse().ok()));
    let log = state.audit.clone();
    let entries = tokio::task::spawn_blocking(move || log.read_latest(limit))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    Ok(no_store(Json(json!({ "entries": entries }))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_query_limit_is_clamped() {
        assert_eq!(clamp_limit(None), 100);
        assert_eq!(clamp_limit(Some(500)), 500);
        assert_eq!(clamp_limit(Some(501)), 500);
        assert_eq!(clamp_limit(Some(0)), 1);
    }

    #[test]
    fn audit_query_accepts_numeric_and_rejects_non_numeric_limits() {
        let numeric: AuditQuery = serde_json::from_value(json!({ "limit": "250" })).unwrap();
        assert_eq!(
            clamp_limit(numeric.limit.as_deref().and_then(|value| value.parse().ok())),
            250
        );
        let invalid: AuditQuery = serde_json::from_value(json!({ "limit": "lots" })).unwrap();
        assert_eq!(
            clamp_limit(invalid.limit.as_deref().and_then(|value| value.parse().ok())),
            100
        );
    }
}
