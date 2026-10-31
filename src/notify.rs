//! Outbound event notifications: generic Webhook, ntfy and Telegram (issue #84).
//!
//! Event producers call [`Notifier::emit`], which never blocks: events go into
//! a bounded queue and are dropped (and counted) when it is full. A single
//! worker applies per-event toggles and rate limiting, then delivers each
//! message to every enabled channel in its own task with a timeout and a
//! small number of retries. Messages never contain secrets: channel
//! credentials only appear in request URLs/headers, and delivery errors are
//! stripped of URLs before they are logged or returned.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use serde::Deserialize;
use tokio::sync::{mpsc, watch};

use crate::config::NotificationsConfig;

pub const QUEUE_CAPACITY: usize = 64;
pub const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
const MAX_DETAIL_CHARS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    LoginSucceeded,
    LoginFailed,
    PowerLedChanged,
    HidFailing,
    VideoLost,
    VideoRecovered,
    Test,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoginSucceeded => "login_succeeded",
            Self::LoginFailed => "login_failed",
            Self::PowerLedChanged => "power_led_changed",
            Self::HidFailing => "hid_failing",
            Self::VideoLost => "video_lost",
            Self::VideoRecovered => "video_recovered",
            Self::Test => "test",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::LoginSucceeded => "登录成功",
            Self::LoginFailed => "登录失败",
            Self::PowerLedChanged => "被控机电源状态变化",
            Self::HidFailing => "键鼠写入持续失败",
            Self::VideoLost => "视频信号丢失",
            Self::VideoRecovered => "视频信号已恢复",
            Self::Test => "测试消息",
        }
    }

    fn enabled(self, config: &NotificationsConfig) -> bool {
        let events = &config.events;
        match self {
            Self::LoginSucceeded => events.login_succeeded,
            Self::LoginFailed => events.login_failed,
            Self::PowerLedChanged => events.power_led_changed,
            Self::HidFailing => events.hid_failing,
            Self::VideoLost | Self::VideoRecovered => events.video_lost,
            Self::Test => true,
        }
    }

    fn ntfy_tags(self) -> &'static str {
        match self {
            Self::LoginSucceeded => "key",
            Self::LoginFailed => "warning,key",
            Self::PowerLedChanged => "electric_plug",
            Self::HidFailing | Self::VideoLost => "warning",
            Self::VideoRecovered => "white_check_mark",
            Self::Test => "bell",
        }
    }

    fn ntfy_priority(self) -> &'static str {
        match self {
            Self::LoginFailed | Self::HidFailing | Self::VideoLost => "high",
            _ => "default",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NotifyEvent {
    pub kind: EventKind,
    pub detail: String,
    pub at: SystemTime,
}

impl NotifyEvent {
    pub fn new(kind: EventKind, detail: impl Into<String>) -> Self {
        let detail: String = detail.into();
        Self {
            kind,
            detail: detail.chars().take(MAX_DETAIL_CHARS).collect(),
            at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Webhook,
    Ntfy,
    Telegram,
}

impl Channel {
    pub const ALL: [Self; 3] = [Self::Webhook, Self::Ntfy, Self::Telegram];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Ntfy => "ntfy",
            Self::Telegram => "telegram",
        }
    }

    fn ready(self, config: &NotificationsConfig) -> bool {
        match self {
            Self::Webhook => config.webhook_ready(),
            Self::Ntfy => config.ntfy_ready(),
            Self::Telegram => config.telegram_ready(),
        }
    }

    fn enabled(self, config: &NotificationsConfig) -> bool {
        let enabled = match self {
            Self::Webhook => config.webhook.enabled,
            Self::Ntfy => config.ntfy.enabled,
            Self::Telegram => config.telegram.enabled,
        };
        enabled && self.ready(config)
    }
}

/// A message ready for delivery: one event, possibly summarising repeats.
#[derive(Debug, Clone)]
pub struct Message {
    pub event: NotifyEvent,
    /// Number of events this message stands for (1 unless repeats were merged).
    pub count: u32,
    pub window_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct DeliveryPolicy {
    pub timeout: Duration,
    pub attempts: u32,
    pub backoff: Duration,
}

impl Default for DeliveryPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            attempts: 3,
            backoff: Duration::from_secs(2),
        }
    }
}

/// Construction knobs; production uses the defaults, tests point the
/// Telegram API at a local server and shorten retries.
#[derive(Debug, Clone)]
pub struct NotifierOptions {
    pub capacity: usize,
    pub policy: DeliveryPolicy,
    pub telegram_api_base: String,
    pub host: String,
}

impl Default for NotifierOptions {
    fn default() -> Self {
        Self {
            capacity: QUEUE_CAPACITY,
            policy: DeliveryPolicy::default(),
            telegram_api_base: TELEGRAM_API_BASE.to_owned(),
            host: host_name(),
        }
    }
}

#[derive(Clone)]
pub struct Notifier {
    inner: Arc<Inner>,
}

struct Inner {
    tx: mpsc::Sender<NotifyEvent>,
    pending_rx: Mutex<Option<mpsc::Receiver<NotifyEvent>>>,
    dropped: AtomicU64,
    config: watch::Sender<NotificationsConfig>,
    sender: Sender,
}

impl Notifier {
    /// Create a notifier and start its worker on the current Tokio runtime.
    pub fn new(config: NotificationsConfig) -> Self {
        let notifier = Self::with_options(config, NotifierOptions::default());
        notifier.start();
        notifier
    }

    /// Create a notifier without starting the worker; call [`Self::start`].
    pub fn with_options(config: NotificationsConfig, options: NotifierOptions) -> Self {
        let (tx, rx) = mpsc::channel(options.capacity.max(1));
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("WingmanKVM/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                tx,
                pending_rx: Mutex::new(Some(rx)),
                dropped: AtomicU64::new(0),
                config: watch::channel(config).0,
                sender: Sender {
                    client,
                    host: options.host,
                    telegram_api_base: options.telegram_api_base.trim_end_matches('/').to_owned(),
                    policy: options.policy,
                },
            }),
        }
    }

    pub fn start(&self) {
        let Some(rx) = self
            .inner
            .pending_rx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        else {
            return;
        };
        tokio::spawn(run_worker(
            rx,
            self.inner.sender.clone(),
            self.inner.config.subscribe(),
        ));
    }

    pub fn update_config(&self, config: NotificationsConfig) {
        self.inner.config.send_replace(config);
    }

    pub fn config(&self) -> NotificationsConfig {
        self.inner.config.borrow().clone()
    }

    /// Queue an event without ever blocking the caller.
    pub fn emit(&self, kind: EventKind, detail: impl Into<String>) {
        {
            let config = self.inner.config.borrow();
            if !kind.enabled(&config) || !Channel::ALL.iter().any(|channel| channel.enabled(&config)) {
                return;
            }
        }
        if self.inner.tx.try_send(NotifyEvent::new(kind, detail)).is_err() {
            let dropped = self.inner.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::warn!(dropped, kind = kind.as_str(), "notification queue full; event dropped");
        }
    }

    pub fn dropped(&self) -> u64 {
        self.inner.dropped.load(Ordering::Relaxed)
    }

    /// Send a test message to one channel using the saved configuration,
    /// bypassing toggles and rate limiting. Errors are safe to show.
    pub async fn send_test(&self, channel: Channel) -> Result<(), String> {
        let config = self.config();
        if !channel.ready(&config) {
            return Err("该通道尚未配置完整，请先填写并保存".to_owned());
        }
        let message = Message {
            event: NotifyEvent::new(
                EventKind::Test,
                "这是一条来自 WingmanKVM 的测试消息；收到即表示通知通道配置正确。",
            ),
            count: 1,
            window_seconds: config.rate_limit_seconds,
        };
        let sender = Sender {
            policy: DeliveryPolicy {
                attempts: 1,
                ..self.inner.sender.policy.clone()
            },
            ..self.inner.sender.clone()
        };
        sender.deliver(channel, &config, &message).await
    }
}

async fn run_worker(
    mut rx: mpsc::Receiver<NotifyEvent>,
    sender: Sender,
    config: watch::Receiver<NotificationsConfig>,
) {
    let mut limiter = RateLimiter::default();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let messages = tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                let window = Duration::from_secs(config.borrow().rate_limit_seconds);
                limiter.offer(event, Instant::now(), window).into_iter().collect::<Vec<_>>()
            }
            _ = tick.tick() => {
                let window = Duration::from_secs(config.borrow().rate_limit_seconds);
                limiter.flush(Instant::now(), window)
            }
        };
        if messages.is_empty() {
            continue;
        }
        let config = config.borrow().clone();
        for message in messages {
            sender.dispatch(&config, message);
        }
    }
}

/// At most one message per event kind per window; repeats inside the window
/// are merged into one summary sent when the window ends.
#[derive(Default)]
pub struct RateLimiter {
    slots: HashMap<EventKind, Slot>,
}

struct Slot {
    last_sent: Instant,
    suppressed: u32,
    latest: Option<NotifyEvent>,
}

impl RateLimiter {
    pub fn offer(&mut self, event: NotifyEvent, now: Instant, window: Duration) -> Option<Message> {
        if let Some(slot) = self.slots.get_mut(&event.kind) {
            if now.saturating_duration_since(slot.last_sent) < window {
                slot.suppressed = slot.suppressed.saturating_add(1);
                slot.latest = Some(event);
                return None;
            }
        }
        // A pending summary is superseded by this message, which reports it.
        let count = self
            .slots
            .get(&event.kind)
            .map_or(0, |slot| slot.suppressed)
            .saturating_add(1);
        self.slots.insert(
            event.kind,
            Slot {
                last_sent: now,
                suppressed: 0,
                latest: None,
            },
        );
        Some(Message {
            event,
            count,
            window_seconds: window.as_secs(),
        })
    }

    pub fn flush(&mut self, now: Instant, window: Duration) -> Vec<Message> {
        let mut out = Vec::new();
        for slot in self.slots.values_mut() {
            if slot.suppressed == 0 || now.saturating_duration_since(slot.last_sent) < window {
                continue;
            }
            if let Some(event) = slot.latest.take() {
                out.push(Message {
                    event,
                    count: slot.suppressed,
                    window_seconds: window.as_secs(),
                });
            }
            slot.suppressed = 0;
            slot.last_sent = now;
        }
        out
    }
}

/// Rendered, secret-free message text.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub kind: EventKind,
    pub title: String,
    pub body: String,
    pub host: String,
    pub time: String,
    pub timestamp: u64,
    pub count: u32,
}

pub fn render(host: &str, message: &Message) -> Rendered {
    let time = format_local_time(message.event.at);
    let mut body = format!("主机：{host}\n时间：{time}");
    if !message.event.detail.trim().is_empty() {
        body.push('\n');
        body.push_str(message.event.detail.trim());
    }
    if message.count > 1 {
        body.push_str(&format!(
            "\n（{} 秒内共发生 {} 次同类事件，已合并，以上为最近一次）",
            message.window_seconds, message.count
        ));
    }
    Rendered {
        kind: message.event.kind,
        title: format!("[WingmanKVM {host}] {}", message.event.kind.title()),
        body,
        host: host.to_owned(),
        time,
        timestamp: message
            .event
            .at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        count: message.count,
    }
}

struct SendError {
    message: String,
    retryable: bool,
}

#[derive(Clone)]
struct Sender {
    client: reqwest::Client,
    host: String,
    telegram_api_base: String,
    policy: DeliveryPolicy,
}

impl Sender {
    fn dispatch(&self, config: &NotificationsConfig, message: Message) {
        for channel in Channel::ALL {
            if !channel.enabled(config) {
                continue;
            }
            let sender = self.clone();
            let config = config.clone();
            let message = message.clone();
            tokio::spawn(async move {
                if let Err(error) = sender.deliver(channel, &config, &message).await {
                    tracing::warn!(
                        channel = channel.as_str(),
                        event = message.event.kind.as_str(),
                        %error,
                        "notification delivery failed"
                    );
                }
            });
        }
    }

    async fn deliver(
        &self,
        channel: Channel,
        config: &NotificationsConfig,
        message: &Message,
    ) -> Result<(), String> {
        let rendered = render(&self.host, message);
        let attempts = self.policy.attempts.max(1);
        let mut delay = self.policy.backoff;
        for attempt in 1..=attempts {
            match self.send_once(channel, config, &rendered).await {
                Ok(()) => return Ok(()),
                Err(error) if !error.retryable || attempt == attempts => {
                    return Err(if attempt > 1 {
                        format!("{}（已重试 {} 次）", error.message, attempt - 1)
                    } else {
                        error.message
                    });
                }
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay = delay.saturating_mul(2);
                }
            }
        }
        unreachable!("the final attempt always returns")
    }

    async fn send_once(
        &self,
        channel: Channel,
        config: &NotificationsConfig,
        rendered: &Rendered,
    ) -> Result<(), SendError> {
        let request = match channel {
            Channel::Webhook => {
                let url = config.webhook_url().unwrap_or_default();
                let body = serde_json::json!({
                    "source": "wingmankvm",
                    "event": rendered.kind.as_str(),
                    "title": rendered.title,
                    "message": rendered.body,
                    "host": rendered.host,
                    "time": rendered.time,
                    "timestamp": rendered.timestamp,
                    "count": rendered.count,
                });
                self.client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.to_string())
            }
            Channel::Ntfy => {
                let url = format!(
                    "{}/{}",
                    config.ntfy.server.trim().trim_end_matches('/'),
                    config.ntfy_topic().unwrap_or_default()
                );
                let mut request = self
                    .client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                    .header("Title", rfc2047(&rendered.title))
                    .header("Tags", rendered.kind.ntfy_tags())
                    .header("Priority", rendered.kind.ntfy_priority())
                    .body(rendered.body.clone());
                if let Some(token) = config.ntfy_token() {
                    request = request.bearer_auth(token);
                }
                request
            }
            Channel::Telegram => {
                let url = format!(
                    "{}/bot{}/sendMessage",
                    self.telegram_api_base,
                    config.telegram_bot_token().unwrap_or_default()
                );
                let body = serde_json::json!({
                    "chat_id": config.telegram_chat_id().unwrap_or_default(),
                    "text": format!("{}\n{}", rendered.title, rendered.body),
                    "disable_web_page_preview": true,
                });
                self.client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.to_string())
            }
        };
        let response = request
            .timeout(self.policy.timeout)
            .send()
            .await
            .map_err(|error| SendError {
                message: if error.is_timeout() {
                    "请求超时".to_owned()
                } else {
                    // Never echo the URL: Telegram's carries the bot token.
                    format!("网络错误：{}", error.without_url())
                },
                retryable: true,
            })?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        Err(SendError {
            message: format!("服务返回 HTTP {}", status.as_u16()),
            retryable: status.is_server_error() || status.as_u16() == 429,
        })
    }
}

/// Encode a UTF-8 header value as an RFC 2047 encoded word (ntfy decodes it).
fn rfc2047(value: &str) -> String {
    if value.is_ascii() && !value.contains("=?") {
        return value.to_owned();
    }
    format!(
        "=?UTF-8?B?{}?=",
        base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
    )
}

pub fn host_name() -> String {
    let mut buffer = [0_u8; 256];
    // SAFETY: the buffer is valid for its full length; gethostname writes at
    // most that many bytes and the result is read only up to the first NUL.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    let name = if result == 0 {
        let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..end]).trim().to_owned()
    } else {
        String::new()
    };
    if name.is_empty() { "wingmankvm".to_owned() } else { name }
}

/// Local wall-clock time with UTC offset, e.g. `2026-11-01 12:00:00 +0800`.
pub fn format_local_time(at: SystemTime) -> String {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs()) as libc::time_t;
    // SAFETY: `tm` is plain data; localtime_r only writes into it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let converted = unsafe { !libc::localtime_r(&seconds, &mut tm).is_null() };
    if !converted {
        return format!("UNIX {seconds}");
    }
    let offset = tm.tm_gmtoff;
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.unsigned_abs();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {sign}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        offset / 3600,
        offset % 3600 / 60
    )
}

/// Detects "N keyboard/mouse failures within a window with no success" from
/// the cumulative HID counters. Alerts once until a write succeeds again.
#[derive(Default)]
pub struct HidFailureTracker {
    last: Option<(u64, u64)>,
    failures: VecDeque<Instant>,
    alerted: bool,
}

impl HidFailureTracker {
    /// Returns the number of failures in the window when an alert is due.
    pub fn observe(
        &mut self,
        ok: u64,
        failed: u64,
        now: Instant,
        threshold: u32,
        window: Duration,
    ) -> Option<usize> {
        let (last_ok, last_failed) = self.last.replace((ok, failed)).unwrap_or((ok, failed));
        if ok > last_ok {
            self.failures.clear();
            self.alerted = false;
        }
        let new_failures = failed.saturating_sub(last_failed).min(10_000);
        for _ in 0..new_failures {
            self.failures.push_back(now);
        }
        while self
            .failures
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > window)
        {
            self.failures.pop_front();
        }
        if !self.alerted && self.failures.len() >= threshold.max(1) as usize {
            self.alerted = true;
            return Some(self.failures.len());
        }
        None
    }
}

/// Video health as seen by the notifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoHealth {
    Ready,
    Lost,
    /// Starting, unconfigured or unsupported: neither good nor lost.
    Neutral,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VideoTransition {
    Lost { seconds: u64 },
    Recovered { seconds: u64 },
}

/// Reports a loss only after the video was ready and then stayed lost longer
/// than the threshold, and a recovery only after such a reported loss.
#[derive(Default)]
pub struct VideoLossTracker {
    was_ready: bool,
    lost_since: Option<Instant>,
    alerted: bool,
}

impl VideoLossTracker {
    pub fn observe(&mut self, health: VideoHealth, now: Instant, threshold: Duration) -> Option<VideoTransition> {
        match health {
            VideoHealth::Ready => {
                let lost = self.lost_since.take();
                self.was_ready = true;
                if std::mem::take(&mut self.alerted) {
                    let seconds = lost.map_or(0, |at| now.saturating_duration_since(at).as_secs());
                    return Some(VideoTransition::Recovered { seconds });
                }
                None
            }
            VideoHealth::Lost if self.was_ready => {
                let since = *self.lost_since.get_or_insert(now);
                let elapsed = now.saturating_duration_since(since);
                if !self.alerted && elapsed >= threshold {
                    self.alerted = true;
                    return Some(VideoTransition::Lost {
                        seconds: elapsed.as_secs(),
                    });
                }
                None
            }
            VideoHealth::Lost | VideoHealth::Neutral => None,
        }
    }
}

/// Reports On/Off changes of the power LED, ignoring unknown readings.
#[derive(Default)]
pub struct PowerLedTracker {
    last: Option<bool>,
}

impl PowerLedTracker {
    /// `state` is `Some(true)` for on, `Some(false)` for off, `None` unknown.
    /// Returns the new state when it changed from a known previous state.
    pub fn observe(&mut self, state: Option<bool>) -> Option<bool> {
        let state = state?;
        let previous = self.last.replace(state)?;
        (previous != state).then_some(state)
    }
}

#[cfg(test)]
mod tests;
