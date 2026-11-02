use std::time::Duration;

use anyhow::Context;
use axum::serve::ListenerExt;
use tokio::signal::unix::{Signal, SignalKind, signal};

mod auth;
mod audit;
mod client_ip;
mod config;
mod devices;
mod notify;
mod proxy_protocol;
mod system;
mod web;
mod web_ui;

/// Longest a stop may spend releasing input. A healthy target takes
/// milliseconds; this only runs out when the USB host has stopped reading.
const SHUTDOWN_RELEASE_TIMEOUT: Duration = Duration::from_secs(3);
/// In-flight requests get this long to finish after the release. Long-lived
/// responses (MJPEG, WebSockets) never finish, so this is what ends them.
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// Ends the process even if the steps above cannot make progress. Needs its
/// own thread, because a blocked runtime could not run a timer.
const SHUTDOWN_HARD_LIMIT: Duration = Duration::from_secs(5);

/// SIGTERM (`systemctl stop/restart`, the online upgrade) and SIGINT.
struct ShutdownSignals {
    terminate: Signal,
    interrupt: Signal,
}

impl ShutdownSignals {
    fn install() -> anyhow::Result<Self> {
        Ok(Self {
            terminate: signal(SignalKind::terminate()).context("failed to handle SIGTERM")?,
            interrupt: signal(SignalKind::interrupt()).context("failed to handle SIGINT")?,
        })
    }

    async fn recv(&mut self) -> &'static str {
        tokio::select! {
            _ = self.terminate.recv() => "SIGTERM",
            _ = self.interrupt.recv() => "SIGINT",
        }
    }
}

fn arm_hard_exit() {
    std::thread::spawn(|| {
        std::thread::sleep(SHUTDOWN_HARD_LIMIT);
        tracing::warn!("shutdown did not finish in time; exiting anyway");
        std::process::exit(0);
    });
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wingmankvm=info,tower_http=info".into()),
        )
        .init();

    let mut signals = ShutdownSignals::install()?;
    let state = web::AppState::load().context("failed to load WingmanKVM state")?;
    let address = state.server_address().await?;
    let proxy_mode = state.proxy_protocol_mode().await;
    let proxy_updates = state.proxy_protocol_updates();
    let app = web::router(state.clone());
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to listen on {address}"))?;
    let listener = proxy_protocol::ProxyProtocolListener::with_mode_updates(listener, proxy_updates);
    let listener = client_ip::ClientIpListener(listener);

    tracing::info!(%address, ?proxy_mode, "WingmanKVM is listening");
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    let server = axum::serve(
        listener.tap_io(|_| {}),
        app.into_make_service_with_connect_info::<client_ip::ClientConnection>(),
    )
    .with_graceful_shutdown(async move {
        let _ = stop_rx.wait_for(|stop| *stop).await;
    });
    let mut server = tokio::spawn(std::future::IntoFuture::into_future(server));

    let signal = tokio::select! {
        result = &mut server => {
            return result.context("HTTP server task failed")?.context("HTTP server failed");
        }
        signal = signals.recv() => signal,
    };

    tracing::info!(signal, "shutting down");
    arm_hard_exit();
    // Stop accepting first, so the port is free for a restarting successor.
    let _ = stop_tx.send(true);
    if tokio::time::timeout(SHUTDOWN_RELEASE_TIMEOUT, state.release_input_for_shutdown())
        .await
        .is_err()
    {
        tracing::warn!("timed out releasing input during shutdown");
    }
    let _ = tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT, &mut server).await;
    tracing::info!("stopped");
    // Not a return: dropping the runtime waits for every blocking task, which
    // can include a stuck read on a terminal or a device. A clean status keeps
    // `Restart=on-failure` from restarting a service that was asked to stop.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        os::unix::{fs::OpenOptionsExt, process::ExitStatusExt},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        time::Instant,
    };

    use super::*;
    use crate::config::Config;

    const CHILD_ENV: &str = "WINGMANKVM_TEST_SERVICE_CHILD";

    /// Not a test of its own: the real service loop, run in a child process by
    /// the tests below. Does nothing in a normal test run.
    #[test]
    fn service_child() {
        if std::env::var_os(CHILD_ENV).is_none() {
            return;
        }
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run())
            .unwrap();
    }

    struct Service {
        root: PathBuf,
        relative: std::fs::File,
        absolute: std::fs::File,
        port: u16,
        child: Child,
    }

    impl Service {
        fn start(name: &str, prepare: impl FnOnce(&Path)) -> Self {
            let root = crate::devices::hotplug::tests::temp_root(name);
            let (relative_path, absolute_path) = (root.join("hidg1"), root.join("hidg2"));
            let relative = crate::devices::hid::tests::fake_hidg(&relative_path);
            let absolute = crate::devices::hid::tests::fake_hidg(&absolute_path);
            prepare(&relative_path);

            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            let mut config = Config::default();
            config.server.listen_address = "127.0.0.1".to_owned();
            config.server.port = port;
            // No keyboard node: the LED reader would compete for the FIFO.
            config.hid.auto_detect = false;
            config.hid.mouse_device = Some(relative_path);
            config.hid.absolute_pointer_device = Some(absolute_path);
            config.video.auto_detect = false;
            // Never reach for a real encoder from a test.
            config.video.h264.ffmpeg_path = Some(root.join("no-ffmpeg"));
            assert!(!config.power.enabled && config.power.power_led.is_none());
            config.save_atomic(root.join("config.json")).unwrap();

            let child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "tests::service_child", "--nocapture"])
                .env(CHILD_ENV, "1")
                .env(crate::config::STATE_DIR_ENV, &root)
                .env("NO_COLOR", "1")
                // tracing writes to stdout; kept for failure messages.
                .stdout(std::fs::File::create(root.join("service.log")).unwrap())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            Self {
                root,
                relative,
                absolute,
                port,
                child,
            }
        }

        fn wait_for_http(&mut self) -> TcpStream {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Ok(stream) = TcpStream::connect(("127.0.0.1", self.port)) {
                    return stream;
                }
                assert!(self.child.try_wait().unwrap().is_none(), "service exited early");
                assert!(Instant::now() < deadline, "service did not start listening");
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        /// Sends `signal` and returns how long the process took to exit.
        fn stop(&mut self, signal: i32) -> Duration {
            let pid = i32::try_from(self.child.id()).unwrap();
            let start = Instant::now();
            // SAFETY: kill(2) on our own child, which has not been reaped yet.
            assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "must exit cleanly, not die from the signal: {:?} / {:?}",
                        status.code(),
                        status.signal()
                    );
                    return start.elapsed();
                }
                assert!(
                    start.elapsed() < Duration::from_secs(9),
                    "stop must finish well inside TimeoutStopSec"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Service {
        /// The child's log, for failure messages.
        fn log(&self) -> String {
            std::fs::read_to_string(self.root.join("service.log")).unwrap_or_default()
        }
    }

    impl Drop for Service {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn read_exact_within(mut reader: &std::fs::File, length: usize) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 64];
        while bytes.len() < length {
            match reader.read(&mut chunk) {
                Ok(count) => bytes.extend_from_slice(&chunk[..count]),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock),
            }
            assert!(Instant::now() < deadline, "only got {bytes:?}, wanted {length} bytes");
            std::thread::sleep(Duration::from_millis(10));
        }
        bytes
    }

    fn read_now(mut reader: &std::fs::File) -> Vec<u8> {
        let mut chunk = [0_u8; 64];
        match reader.read(&mut chunk) {
            Ok(count) => chunk[..count].to_vec(),
            Err(_) => Vec::new(),
        }
    }

    const IDLE_RELATIVE: [u8; 4] = [0; 4];
    /// Buttons up at the last position (still the center here).
    const IDLE_ABSOLUTE: [u8; 6] = [0, 0x00, 0x40, 0x00, 0x40, 0];

    #[test]
    fn sigterm_releases_input_at_startup_and_on_the_way_out() {
        let mut service = Service::start("service-sigterm", |_| {});
        assert_eq!(read_exact_within(&service.relative, 4), IDLE_RELATIVE, "startup");
        service.wait_for_http();
        // Startup leaves the absolute pointer alone: any report would move it.
        assert!(read_now(&service.absolute).is_empty());

        let elapsed = service.stop(libc::SIGTERM);

        assert_eq!(read_exact_within(&service.relative, 4), IDLE_RELATIVE, "shutdown");
        assert_eq!(read_exact_within(&service.absolute, 6), IDLE_ABSOLUTE, "shutdown");
        assert!(
            elapsed < Duration::from_secs(2),
            "idle stop took {elapsed:?}\n{}",
            service.log()
        );
    }

    #[test]
    fn sigint_stops_the_service_too() {
        let mut service = Service::start("service-sigint", |_| {});
        assert_eq!(read_exact_within(&service.relative, 4), IDLE_RELATIVE, "startup");
        service.wait_for_http();
        service.stop(libc::SIGINT);
        assert_eq!(read_exact_within(&service.relative, 4), IDLE_RELATIVE, "shutdown");
    }

    #[test]
    fn stop_stays_bounded_with_a_wedged_host_and_an_open_connection() {
        let mut service = Service::start("service-wedged", |relative| {
            // A host that stopped reading: the pipe is full, so every report
            // to this node blocks until its write timeout.
            let mut writer = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(relative)
                .unwrap();
            while writer.write(&[0xee; 4096]).is_ok() {}
            std::mem::forget(writer);
        });
        // A request that never completes keeps its connection in the drain.
        let mut connection = service.wait_for_http();
        connection
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\n")
            .unwrap();

        let elapsed = service.stop(libc::SIGTERM);

        // Finishes on the normal path (blocked release, then the bounded
        // drain), well before the hard limit has to step in.
        assert!(
            elapsed < SHUTDOWN_HARD_LIMIT - Duration::from_secs(1),
            "took {elapsed:?}\n{}",
            service.log()
        );
        // The absolute node still works, so it is released regardless.
        assert_eq!(read_exact_within(&service.absolute, 6), IDLE_ABSOLUTE, "shutdown");
    }
}
