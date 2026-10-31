use super::*;
use crate::config::{NtfyChannelConfig, TelegramChannelConfig, WebhookChannelConfig};
use axum::{Router, body::Bytes, extract::State, http::{HeaderMap, StatusCode, Uri}, routing::post};

#[derive(Debug, Clone)]
struct Captured {
    path: String,
    headers: HeaderMap,
    body: String,
}

#[derive(Clone, Default)]
struct Server {
    requests: Arc<Mutex<Vec<Captured>>>,
    /// Status codes returned in order; 200 once exhausted.
    statuses: Arc<Mutex<VecDeque<u16>>>,
}

async fn capture(State(server): State<Server>, uri: Uri, headers: HeaderMap, body: Bytes) -> StatusCode {
    server.requests.lock().unwrap().push(Captured {
        path: uri.path().to_owned(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    });
    let status = server.statuses.lock().unwrap().pop_front().unwrap_or(200);
    StatusCode::from_u16(status).unwrap()
}

impl Server {
    async fn start(statuses: &[u16]) -> (Self, String) {
        let server = Self::default();
        server.statuses.lock().unwrap().extend(statuses.iter().copied());
        let app = Router::new()
            .route("/{*path}", post(capture))
            .with_state(server.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (server, format!("http://{address}"))
    }

    fn requests(&self) -> Vec<Captured> {
        self.requests.lock().unwrap().clone()
    }
}

fn options(base: &str) -> NotifierOptions {
    NotifierOptions {
        capacity: 8,
        policy: DeliveryPolicy {
            timeout: Duration::from_secs(2),
            attempts: 3,
            backoff: Duration::from_millis(10),
        },
        telegram_api_base: base.to_owned(),
        host: "kvm-test".to_owned(),
    }
}

fn webhook_config(base: &str) -> NotificationsConfig {
    NotificationsConfig {
        webhook: WebhookChannelConfig {
            enabled: true,
            url: Some(format!("{base}/hook/secret-path")),
        },
        ..NotificationsConfig::default()
    }
}

fn message(kind: EventKind, detail: &str) -> Message {
    Message {
        event: NotifyEvent::new(kind, detail),
        count: 1,
        window_seconds: 60,
    }
}

fn sender(notifier: &Notifier) -> Sender {
    notifier.inner.sender.clone()
}

#[tokio::test]
async fn webhook_posts_a_json_body() {
    let (server, base) = Server::start(&[]).await;
    let config = webhook_config(&base);
    let notifier = Notifier::with_options(config.clone(), options(&base));
    sender(&notifier)
        .deliver(Channel::Webhook, &config, &message(EventKind::LoginFailed, "来源 IP：203.0.113.9"))
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/hook/secret-path");
    assert_eq!(requests[0].headers["content-type"], "application/json");
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(body["source"], "wingmankvm");
    assert_eq!(body["event"], "login_failed");
    assert_eq!(body["host"], "kvm-test");
    assert_eq!(body["title"], "[WingmanKVM kvm-test] 登录失败");
    let text = body["message"].as_str().unwrap();
    assert!(text.contains("主机：kvm-test") && text.contains("时间：") && text.contains("203.0.113.9"), "{text}");
    assert!(body["timestamp"].as_u64().unwrap() > 1_700_000_000);
    assert_eq!(body["count"], 1);
}

#[tokio::test]
async fn ntfy_uses_topic_path_headers_and_plain_body() {
    let (server, base) = Server::start(&[]).await;
    let config = NotificationsConfig {
        ntfy: NtfyChannelConfig {
            enabled: true,
            server: format!("{base}/"),
            topic: Some("kvm-alerts".to_owned()),
            token: Some("tk_secret".to_owned()),
        },
        ..NotificationsConfig::default()
    };
    let notifier = Notifier::with_options(config.clone(), options(&base));
    sender(&notifier)
        .deliver(Channel::Ntfy, &config, &message(EventKind::VideoLost, "视频已中断 30 秒"))
        .await
        .unwrap();
    let request = &server.requests()[0];
    assert_eq!(request.path, "/kvm-alerts");
    assert_eq!(request.headers["authorization"], "Bearer tk_secret");
    assert_eq!(request.headers["priority"], "high");
    assert_eq!(request.headers["tags"], "warning");
    let title = request.headers["title"].to_str().unwrap();
    let encoded = title.strip_prefix("=?UTF-8?B?").and_then(|t| t.strip_suffix("?=")).unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "[WingmanKVM kvm-test] 视频信号丢失");
    assert!(request.headers["content-type"].to_str().unwrap().starts_with("text/plain"));
    assert!(request.body.contains("视频已中断 30 秒") && !request.body.contains("tk_secret"));
}

#[tokio::test]
async fn telegram_calls_send_message_with_chat_and_text() {
    let (server, base) = Server::start(&[]).await;
    let config = NotificationsConfig {
        telegram: TelegramChannelConfig {
            enabled: true,
            bot_token: Some("12345:AAsecret".to_owned()),
            chat_id: Some("-100777".to_owned()),
        },
        ..NotificationsConfig::default()
    };
    let notifier = Notifier::with_options(config.clone(), options(&base));
    notifier.send_test(Channel::Telegram).await.unwrap();
    let request = &server.requests()[0];
    assert_eq!(request.path, "/bot12345:AAsecret/sendMessage");
    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(body["chat_id"], "-100777");
    assert_eq!(body["disable_web_page_preview"], true);
    let text = body["text"].as_str().unwrap();
    assert!(text.starts_with("[WingmanKVM kvm-test] 测试消息\n主机：kvm-test"), "{text}");
    assert!(!text.contains("AAsecret"));
}

#[tokio::test]
async fn server_errors_are_retried_then_succeed() {
    let (server, base) = Server::start(&[500, 503]).await;
    let config = webhook_config(&base);
    let notifier = Notifier::with_options(config.clone(), options(&base));
    sender(&notifier)
        .deliver(Channel::Webhook, &config, &message(EventKind::LoginSucceeded, ""))
        .await
        .unwrap();
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn delivery_gives_up_after_the_attempt_budget() {
    let (server, base) = Server::start(&[500, 500, 500, 500]).await;
    let config = webhook_config(&base);
    let notifier = Notifier::with_options(config.clone(), options(&base));
    let error = sender(&notifier)
        .deliver(Channel::Webhook, &config, &message(EventKind::LoginSucceeded, ""))
        .await
        .unwrap_err();
    assert_eq!(server.requests().len(), 3);
    assert!(error.contains("HTTP 500") && error.contains("已重试 2 次"), "{error}");
    assert!(!error.contains("secret-path"), "{error}");
}

#[tokio::test]
async fn client_errors_are_not_retried_and_never_leak_the_token() {
    let (server, base) = Server::start(&[401]).await;
    let config = NotificationsConfig {
        telegram: TelegramChannelConfig {
            enabled: true,
            bot_token: Some("999:leakme".to_owned()),
            chat_id: Some("1".to_owned()),
        },
        ..NotificationsConfig::default()
    };
    let notifier = Notifier::with_options(config, options(&base));
    let error = notifier.send_test(Channel::Telegram).await.unwrap_err();
    assert_eq!(server.requests().len(), 1);
    assert!(error.contains("HTTP 401") && !error.contains("leakme"), "{error}");

    // Unreachable host: network errors are reported without the URL.
    let mut config = notifier.config();
    config.telegram.bot_token = Some("999:leakme".to_owned());
    let notifier = Notifier::with_options(config, options("http://127.0.0.1:9"));
    let error = notifier.send_test(Channel::Telegram).await.unwrap_err();
    assert!(!error.contains("leakme"), "{error}");
}

#[tokio::test]
async fn test_message_requires_a_configured_channel() {
    let notifier = Notifier::with_options(NotificationsConfig::default(), options("http://127.0.0.1:9"));
    assert!(notifier.send_test(Channel::Ntfy).await.unwrap_err().contains("尚未配置"));
}

#[test]
fn rate_limiter_sends_once_per_window_and_summarises_repeats() {
    let mut limiter = RateLimiter::default();
    let window = Duration::from_secs(60);
    let start = Instant::now();
    let first = limiter.offer(NotifyEvent::new(EventKind::LoginFailed, "a"), start, window);
    assert_eq!(first.unwrap().count, 1);
    for (offset, detail) in [(1, "b"), (2, "c"), (3, "d")] {
        let message = limiter.offer(
            NotifyEvent::new(EventKind::LoginFailed, detail),
            start + Duration::from_secs(offset),
            window,
        );
        assert!(message.is_none());
    }
    // Other kinds have their own budget.
    assert!(limiter
        .offer(NotifyEvent::new(EventKind::PowerLedChanged, "x"), start + Duration::from_secs(4), window)
        .is_some());
    assert!(limiter.flush(start + Duration::from_secs(30), window).is_empty());
    let summary = limiter.flush(start + Duration::from_secs(61), window);
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].count, 3);
    assert_eq!(summary[0].event.detail, "d");
    let text = render("h", &summary[0]).body;
    assert!(text.contains("60 秒内共发生 3 次同类事件"), "{text}");
    // Nothing pending afterwards, and the window restarted at the summary.
    assert!(limiter.flush(start + Duration::from_secs(200), window).is_empty());
    assert!(limiter
        .offer(NotifyEvent::new(EventKind::LoginFailed, "e"), start + Duration::from_secs(70), window)
        .is_none());
}

#[test]
fn a_new_window_reports_suppressed_events_it_supersedes() {
    let mut limiter = RateLimiter::default();
    let window = Duration::from_secs(60);
    let start = Instant::now();
    limiter.offer(NotifyEvent::new(EventKind::LoginFailed, "a"), start, window);
    limiter.offer(NotifyEvent::new(EventKind::LoginFailed, "b"), start + Duration::from_secs(5), window);
    let next = limiter
        .offer(NotifyEvent::new(EventKind::LoginFailed, "c"), start + Duration::from_secs(61), window)
        .unwrap();
    assert_eq!(next.count, 2);
}

#[tokio::test]
async fn full_queue_drops_and_counts_without_blocking() {
    let mut config = webhook_config("http://127.0.0.1:9");
    config.events.login_failed = true;
    let notifier = Notifier::with_options(config, NotifierOptions { capacity: 2, ..options("") });
    // Worker not started: the queue fills up.
    for _ in 0..5 {
        notifier.emit(EventKind::LoginFailed, "x");
    }
    assert_eq!(notifier.dropped(), 3);
}

#[tokio::test]
async fn disabled_events_and_channels_are_not_queued() {
    let mut config = webhook_config("http://127.0.0.1:9");
    config.events.login_succeeded = false;
    let notifier = Notifier::with_options(config, NotifierOptions { capacity: 1, ..options("") });
    for _ in 0..3 {
        notifier.emit(EventKind::LoginSucceeded, "x");
    }
    assert_eq!(notifier.dropped(), 0);
    notifier.update_config(NotificationsConfig::default());
    for _ in 0..3 {
        notifier.emit(EventKind::LoginFailed, "x");
    }
    assert_eq!(notifier.dropped(), 0);
}

#[tokio::test]
async fn worker_delivers_emitted_events_once_per_window() {
    let (server, base) = Server::start(&[]).await;
    let notifier = Notifier::with_options(webhook_config(&base), options(&base));
    notifier.start();
    for index in 0..4 {
        notifier.emit(EventKind::LoginFailed, format!("attempt {index}"));
    }
    notifier.emit(EventKind::PowerLedChanged, "电源指示灯：灭 → 亮");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let events: Vec<String> = server
        .requests()
        .iter()
        .map(|request| serde_json::from_str::<serde_json::Value>(&request.body).unwrap()["event"]
            .as_str()
            .unwrap()
            .to_owned())
        .collect();
    let mut sorted = events.clone();
    sorted.sort();
    assert_eq!(sorted, ["login_failed", "power_led_changed"], "{events:?}");
}

#[test]
fn hid_tracker_alerts_once_after_threshold_without_success() {
    let mut tracker = HidFailureTracker::default();
    let window = Duration::from_secs(60);
    let start = Instant::now();
    assert_eq!(tracker.observe(10, 0, start, 5, window), None);
    assert_eq!(tracker.observe(10, 3, start + Duration::from_secs(1), 5, window), None);
    assert_eq!(tracker.observe(10, 5, start + Duration::from_secs(2), 5, window), Some(5));
    assert_eq!(tracker.observe(10, 9, start + Duration::from_secs(3), 5, window), None, "latched");
    // A success resets the latch.
    assert_eq!(tracker.observe(11, 9, start + Duration::from_secs(4), 5, window), None);
    assert_eq!(tracker.observe(11, 13, start + Duration::from_secs(5), 5, window), None);
    // Old failures fall out of the window.
    assert_eq!(tracker.observe(11, 14, start + Duration::from_secs(100), 5, window), None);
    assert_eq!(tracker.observe(11, 18, start + Duration::from_secs(101), 5, window), Some(5));
}

#[test]
fn hid_tracker_ignores_failures_mixed_with_successes() {
    let mut tracker = HidFailureTracker::default();
    let window = Duration::from_secs(60);
    let start = Instant::now();
    tracker.observe(0, 0, start, 3, window);
    for step in 1..10 {
        assert_eq!(tracker.observe(step, step * 2, start + Duration::from_secs(step), 3, window), None);
    }
}

#[test]
fn video_tracker_reports_long_losses_and_recovery() {
    let mut tracker = VideoLossTracker::default();
    let threshold = Duration::from_secs(30);
    let start = Instant::now();
    let at = |seconds| start + Duration::from_secs(seconds);
    // Lost before ever being ready (no capture card yet): silent.
    assert_eq!(tracker.observe(VideoHealth::Lost, at(0), threshold), None);
    assert_eq!(tracker.observe(VideoHealth::Ready, at(1), threshold), None);
    // Short glitch: silent both ways.
    assert_eq!(tracker.observe(VideoHealth::Lost, at(2), threshold), None);
    assert_eq!(tracker.observe(VideoHealth::Ready, at(10), threshold), None);
    assert_eq!(tracker.observe(VideoHealth::Lost, at(20), threshold), None);
    assert_eq!(tracker.observe(VideoHealth::Neutral, at(30), threshold), None);
    assert_eq!(
        tracker.observe(VideoHealth::Lost, at(50), threshold),
        Some(VideoTransition::Lost { seconds: 30 })
    );
    assert_eq!(tracker.observe(VideoHealth::Lost, at(60), threshold), None);
    assert_eq!(
        tracker.observe(VideoHealth::Ready, at(80), threshold),
        Some(VideoTransition::Recovered { seconds: 60 })
    );
}

#[test]
fn power_tracker_reports_only_known_changes() {
    let mut tracker = PowerLedTracker::default();
    assert_eq!(tracker.observe(None), None);
    assert_eq!(tracker.observe(Some(true)), None);
    assert_eq!(tracker.observe(None), None);
    assert_eq!(tracker.observe(Some(true)), None);
    assert_eq!(tracker.observe(Some(false)), Some(false));
    assert_eq!(tracker.observe(Some(true)), Some(true));
}

#[test]
fn local_time_and_rfc2047_formats() {
    let time = format_local_time(UNIX_EPOCH + Duration::from_secs(1_790_000_000));
    assert_eq!(time.len(), "2026-09-21 12:00:00 +0000".len(), "{time}");
    assert_eq!(rfc2047("plain"), "plain");
    assert!(rfc2047("中文").starts_with("=?UTF-8?B?"));
    assert!(!host_name().is_empty());
}
