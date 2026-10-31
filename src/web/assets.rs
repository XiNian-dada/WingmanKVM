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

pub(super) async fn app_js() -> impl IntoResponse {
    no_store(([(CONTENT_TYPE, "text/javascript; charset=utf-8")], APP_JS.as_str()))
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
    async fn favicon_and_icon_endpoints_return_svg() {
        let resp = favicon().await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml"
        );
    }
}
