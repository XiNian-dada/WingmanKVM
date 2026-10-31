//! Event sources and the test endpoint for outbound notifications (#84).
//!
//! Sources observe existing state instead of instrumenting call sites: the
//! audit log reports logins, and a 1 s poller watches the power LED, the video
//! state and the HID success/failure counters.

use super::*;
use crate::{
    audit::AuditEntry,
    devices::{power::PowerLedState, video::VideoState},
    notify::{
        Channel, EventKind, HidFailureTracker, Notifier, PowerLedTracker, VideoHealth,
        VideoLossTracker, VideoTransition,
    },
};

/// Map login audit entries to notifications. Only the login method and the
/// client IP are forwarded; the submitted username is not, because users
/// sometimes type a password into that field.
pub(super) fn audit_observer(notifier: Notifier) -> impl Fn(&AuditEntry) + Send + Sync + 'static {
    move |entry| {
        let kind = match entry.event.as_str() {
            "login_succeeded" => EventKind::LoginSucceeded,
            "login_failed" => EventKind::LoginFailed,
            _ => return,
        };
        let method = entry
            .detail
            .as_deref()
            .and_then(|detail| detail.split_whitespace().find_map(|part| part.strip_prefix("method=")))
            .map(|method| match method {
                "password" => "密码",
                "passkey" => "Passkey",
                _ => "其他",
            })
            .unwrap_or("未知");
        let ip = entry.ip.map_or_else(|| "未知".to_owned(), |ip| ip.to_string());
        notifier.emit(kind, format!("来源 IP：{ip}\n登录方式：{method}"));
    }
}

fn video_health(state: VideoState) -> VideoHealth {
    match state {
        VideoState::Ready => VideoHealth::Ready,
        VideoState::Stalled | VideoState::Offline => VideoHealth::Lost,
        _ => VideoHealth::Neutral,
    }
}

pub(super) fn spawn_event_sources(
    notifier: Notifier,
    video: VideoManager,
    power: PowerManager,
    metrics: Arc<RuntimeMetrics>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut power_led = PowerLedTracker::default();
        let mut video_loss = VideoLossTracker::default();
        let mut hid = HidFailureTracker::default();
        loop {
            interval.tick().await;
            let config = notifier.config();
            let now = Instant::now();

            let led = match power.status().await.power_led.state {
                PowerLedState::On => Some(true),
                PowerLedState::Off => Some(false),
                PowerLedState::Unknown => None,
            };
            if let Some(on) = power_led.observe(led) {
                notifier.emit(
                    EventKind::PowerLedChanged,
                    if on {
                        "电源指示灯：灭 → 亮（被控机可能已开机）"
                    } else {
                        "电源指示灯：亮 → 灭（被控机可能已关机）"
                    },
                );
            }

            let status = video.status();
            match video_loss.observe(
                video_health(status.state),
                now,
                Duration::from_secs(config.video_lost_seconds),
            ) {
                Some(VideoTransition::Lost { seconds }) => notifier.emit(
                    EventKind::VideoLost,
                    format!("视频采集已中断约 {seconds} 秒，当前状态：{}", video_state_label(status.state)),
                ),
                Some(VideoTransition::Recovered { seconds }) => notifier.emit(
                    EventKind::VideoRecovered,
                    format!("视频采集已恢复，中断时长约 {seconds} 秒"),
                ),
                None => {}
            }

            let (ok, failed, last_error) = metrics.hid_counts();
            if let Some(count) = hid.observe(
                ok,
                failed,
                now,
                config.hid_failure_threshold,
                Duration::from_secs(config.hid_failure_window_seconds),
            ) {
                let mut detail = format!(
                    "{} 秒内键鼠写入失败 {count} 次，期间没有成功写入",
                    config.hid_failure_window_seconds
                );
                if let Some(error) = last_error {
                    detail.push_str(&format!("\n最近错误：{error}"));
                }
                notifier.emit(EventKind::HidFailing, detail);
            }
        }
    });
}

fn video_state_label(state: VideoState) -> &'static str {
    match state {
        VideoState::Stalled => "画面停滞",
        VideoState::Offline => "采集设备离线",
        _ => "异常",
    }
}

#[derive(Deserialize)]
pub(super) struct NotificationTestRequest {
    channel: Channel,
}

pub(super) async fn test_notification(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<NotificationTestRequest>,
) -> Result<impl IntoResponse, ApiError> {
    match state.notifier.send_test(request.channel).await {
        Ok(()) => Ok(no_store(Json(json!({ "ok": true, "channel": request.channel.as_str() })))),
        Err(error) => Err(ApiError::new(StatusCode::BAD_GATEWAY, format!("发送失败：{error}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_audit_entries_become_notifications_without_usernames() {
        let mut config = config::NotificationsConfig::default();
        config.webhook.enabled = true;
        config.webhook.url = Some("http://127.0.0.1:9/hook".to_owned());
        let notifier = Notifier::with_options(
            config,
            crate::notify::NotifierOptions { capacity: 1, ..Default::default() },
        );
        let observer = audit_observer(notifier.clone());
        let entry = AuditEntry::new(
            crate::audit::AuditEvent::Logout,
            None,
            None,
            None,
        );
        observer(&entry);
        assert_eq!(notifier.dropped(), 0, "logout is not forwarded");
        let entry = AuditEntry::new(
            crate::audit::AuditEvent::LoginFailed,
            Some("203.0.113.1".parse().unwrap()),
            None,
            Some("method=password username=hunter2".to_owned()),
        );
        observer(&entry);
        observer(&entry);
        // The worker is not running: one queued, the second dropped.
        assert_eq!(notifier.dropped(), 1);
    }
}
