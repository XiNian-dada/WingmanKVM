use super::*;

pub(super) const XTERM_JS: &str = include_str!("../../web/vendor/xterm/xterm.js");

pub(super) const XTERM_FIT_JS: &str = include_str!("../../web/vendor/xterm/addon-fit.js");

pub(super) const XTERM_CSS: &str = include_str!("../../web/vendor/xterm/xterm.css");

pub(super) async fn xterm_js() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/javascript; charset=utf-8")], XTERM_JS)
}

pub(super) async fn xterm_fit_js() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/javascript; charset=utf-8")],
        XTERM_FIT_JS,
    )
}

// App assets change with every release; never let a browser pair a fresh
// index with a stale script or stylesheet.
pub(super) async fn app_css() -> impl IntoResponse {
    no_store(([(CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS.as_str()))
}

/// One app ES module (`/assets/js/<name>`); index.html loads `main.js` and the
/// browser fetches its imports from the same place.
pub(super) async fn app_js_module(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    match crate::web_ui::app_js_module(&name) {
        Some(source) => no_store(([(CONTENT_TYPE, "text/javascript; charset=utf-8")], source)),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn xterm_css() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], XTERM_CSS)
}

pub(super) async fn index() -> impl IntoResponse {
    no_store(Html(INDEX_HTML))
}

pub(super) async fn favicon() -> impl IntoResponse {
    ([(CONTENT_TYPE, "image/svg+xml")], ICON_SVG)
}

pub(super) async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "wingmankvm"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn app_modules_are_served_as_uncached_javascript() {
        let resp = app_js_module(axum::extract::Path("main.js".to_string())).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
        for missing in ["missing.js", "../web_ui.rs", "main.js.map"] {
            let resp = app_js_module(axum::extract::Path(missing.to_string())).await;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{missing}");
        }
    }

    #[tokio::test]
    async fn favicon_and_icon_endpoints_return_svg() {
        let resp = favicon().await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml"
        );
    }
}
