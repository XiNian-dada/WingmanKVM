//! Defense-in-depth response headers applied to every response (#92).

use std::sync::LazyLock;

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::web_ui::INDEX_HTML;

const CSP_TEMPLATE: &str = include_str!("../../web/csp-policy.txt");

const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=(), payment=(), \
usb=(), serial=(), hid=(), bluetooth=(), midi=(), display-capture=(), \
clipboard-read=(self), clipboard-write=(self), fullscreen=(self), picture-in-picture=(self), \
publickey-credentials-get=(self), publickey-credentials-create=(self)";

/// Text of the single attribute-less inline `<script>` in index.html.
fn inline_script(html: &str) -> &str {
    let start = html
        .find("<script>")
        .expect("index.html has an inline <script>")
        + "<script>".len();
    let len = html[start..]
        .find("</script>")
        .expect("inline <script> is closed");
    &html[start..start + len]
}

fn script_hash(script: &str) -> String {
    let digest = Sha256::digest(script.as_bytes());
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

/// Policy with the script hash filled in; only the WebSocket sources vary per request.
static CSP_BASE: LazyLock<String> = LazyLock::new(|| {
    CSP_TEMPLATE
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("; ")
        .replace(
            "{inline_script_hash}",
            &script_hash(inline_script(INDEX_HTML)),
        )
});

/// Only a plain host[:port] (incl. bracketed IPv6) is echoed into the policy,
/// so a forged Host header cannot inject directives.
fn websocket_sources(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| {
            !host.is_empty()
                && host.len() <= 255
                && host.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']')
                })
        });
    match host {
        Some(host) => format!("ws://{host} wss://{host}"),
        None => String::new(),
    }
}

pub(crate) fn content_security_policy(headers: &HeaderMap) -> String {
    CSP_BASE
        .replace(
            " {websocket_sources}",
            &format!(" {}", websocket_sources(headers)),
        )
        .replace(" ;", ";")
}

pub(crate) async fn security_headers(request: Request, next: Next) -> Response {
    let csp = content_security_policy(request.headers());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let fixed = [
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (
            HeaderName::from_static("permissions-policy"),
            PERMISSIONS_POLICY,
        ),
        (
            HeaderName::from_static("cross-origin-opener-policy"),
            "same-origin",
        ),
    ];
    for (name, value) in fixed {
        headers
            .entry(name)
            .or_insert(HeaderValue::from_static(value));
    }
    if let Ok(value) = HeaderValue::from_str(&csp) {
        headers
            .entry(header::CONTENT_SECURITY_POLICY)
            .or_insert(value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, middleware, routing::get};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn script_hash_matches_inline_theme_script() {
        let script = inline_script(INDEX_HTML);
        assert!(
            script.contains("wingman_theme"),
            "hashed the theme bootstrap script"
        );
        // Exactly one inline script: every other <script> must load from 'self'.
        assert_eq!(INDEX_HTML.matches("<script>").count(), 1);
        assert_eq!(
            INDEX_HTML.matches("<script").count(),
            INDEX_HTML.matches("<script src=\"/").count() + 1
        );
        let expected = format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script.as_bytes()))
        );
        assert!(
            CSP_BASE.contains(&format!("script-src 'self' {expected};")),
            "{}",
            *CSP_BASE
        );
        let csp = content_security_policy(&HeaderMap::new());
        assert!(!csp.contains('{'), "all placeholders filled: {csp}");
        assert!(
            !CSP_BASE
                .split("; ")
                .any(|d| d.starts_with("script-src") && d.contains("unsafe"))
        );
    }

    #[test]
    fn websocket_sources_follow_host_and_reject_injection() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("kvm.local:8080"));
        let csp = content_security_policy(&headers);
        assert!(
            csp.contains("connect-src 'self' ws://kvm.local:8080 wss://kvm.local:8080;"),
            "{csp}"
        );
        headers.insert(header::HOST, HeaderValue::from_static("evil; script-src *"));
        let csp = content_security_policy(&headers);
        assert!(csp.contains("connect-src 'self';"), "{csp}");
        assert!(!csp.contains("evil"));
    }

    async fn get_raw(address: std::net::SocketAddr, path: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut body = Vec::new();
        stream.read_to_end(&mut body).await.unwrap();
        String::from_utf8_lossy(&body).to_ascii_lowercase()
    }

    #[tokio::test]
    async fn headers_present_on_index_api_and_assets() {
        let app = Router::new()
            .route("/", get(super::super::assets::index))
            .route(
                "/assets/js/{name}",
                get(super::super::assets::app_js_module),
            )
            .route("/assets/xterm.css", get(super::super::assets::xterm_css))
            .route("/healthz", get(super::super::assets::health))
            .route(
                "/api/status",
                get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "{}") }),
            )
            .layer(middleware::from_fn(security_headers));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await });
        for path in [
            "/",
            "/assets/js/main.js",
            "/assets/xterm.css",
            "/healthz",
            "/api/status",
            "/missing",
        ] {
            let response = get_raw(address, path).await;
            for expected in [
                "x-frame-options: deny",
                "x-content-type-options: nosniff",
                "referrer-policy: no-referrer",
                "cross-origin-opener-policy: same-origin",
                "permissions-policy: camera=(), microphone=()",
                "content-security-policy: default-src 'self'",
                "frame-ancestors 'none'",
                &format!("wss://{address}"),
            ] {
                assert!(
                    response.contains(expected),
                    "{path} missing {expected}:\n{response}"
                );
            }
        }
        task.abort();
    }
}
