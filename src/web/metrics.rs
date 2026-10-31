use super::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Process-wide counters shown in the diagnostics panel. They answer "is
/// input reaching the target, and since when is it failing" without logs.
#[derive(Default)]
pub(crate) struct RuntimeMetrics {
    started: std::sync::OnceLock<Instant>,
    hid_ok: AtomicU64,
    hid_failed: AtomicU64,
    last_hid_error: Mutex<Option<(String, Instant)>>,
    input_sockets: AtomicUsize,
}

impl RuntimeMetrics {
    pub(crate) fn new() -> Self {
        let metrics = Self::default();
        let _ = metrics.started.set(Instant::now());
        metrics
    }

    /// Records the outcome of one keyboard/mouse operation.
    pub(super) fn record_hid<T>(&self, result: &Result<T, ApiError>) {
        match result {
            Ok(_) => {
                self.hid_ok.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                self.hid_failed.fetch_add(1, Ordering::Relaxed);
                *self.last_hid_error.lock().unwrap_or_else(|p| p.into_inner()) =
                    Some((error.message.clone(), Instant::now()));
            }
        }
    }

    /// Cumulative (ok, failed) HID counters and the latest error message.
    pub(super) fn hid_counts(&self) -> (u64, u64, Option<String>) {
        let last = self.last_hid_error.lock().unwrap_or_else(|p| p.into_inner());
        (
            self.hid_ok.load(Ordering::Relaxed),
            self.hid_failed.load(Ordering::Relaxed),
            last.as_ref().map(|(message, _)| message.clone()),
        )
    }

    /// Counts an open input WebSocket until the returned guard is dropped.
    pub(super) fn input_socket_opened(self: &Arc<Self>) -> InputSocketGuard {
        self.input_sockets.fetch_add(1, Ordering::Relaxed);
        InputSocketGuard(self.clone())
    }

    pub(super) fn snapshot(&self) -> serde_json::Value {
        let last = self.last_hid_error.lock().unwrap_or_else(|p| p.into_inner()).clone();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": self.started.get().map_or(0, |at| at.elapsed().as_secs()),
            "hid": {
                "ok": self.hid_ok.load(Ordering::Relaxed),
                "failed": self.hid_failed.load(Ordering::Relaxed),
                "last_error": last.as_ref().map(|(message, _)| message),
                "last_error_age_seconds": last.as_ref().map(|(_, at)| at.elapsed().as_secs()),
            },
            "input_sockets": self.input_sockets.load(Ordering::Relaxed),
        })
    }
}

pub(super) struct InputSocketGuard(Arc<RuntimeMetrics>);

impl Drop for InputSocketGuard {
    fn drop(&mut self) {
        self.0.input_sockets.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hid_outcomes_and_open_sockets_are_counted() {
        let metrics = Arc::new(RuntimeMetrics::new());
        metrics.record_hid::<()>(&Ok(()));
        metrics.record_hid::<()>(&Err(ApiError::new(StatusCode::GATEWAY_TIMEOUT, "写入超时")));
        let guard = metrics.input_socket_opened();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot["hid"]["ok"], 1);
        assert_eq!(snapshot["hid"]["failed"], 1);
        assert_eq!(snapshot["hid"]["last_error"], "写入超时");
        assert_eq!(snapshot["input_sockets"], 1);
        drop(guard);
        assert_eq!(metrics.snapshot()["input_sockets"], 0);
    }
}
