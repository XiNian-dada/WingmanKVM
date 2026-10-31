//! Device-node presence watching for hot-plug recovery.
//!
//! On Linux this arms an inotify watch (raw syscalls through `libc`, already a
//! dependency) on the nearest existing ancestor directory of the watched path,
//! so the caller wakes as soon as `/dev/video0` or a udev
//! `/dev/v4l/by-id/...` symlink appears. Elsewhere, and if inotify cannot be
//! armed, it falls back to cheap `stat` polling. Each call is stateless: the
//! watch is armed before the existence check, so no creation is missed.

use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Fallback `stat` interval when inotify is unavailable.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missing,
}

/// Whether `path` exists, following symlinks (a dangling by-id link is missing).
pub fn presence(path: &Path) -> Presence {
    if path.exists() {
        Presence::Present
    } else {
        Presence::Missing
    }
}

/// Waits up to `timeout` for `path` to exist. Returns early as soon as it does.
pub fn wait_until_present(path: &Path, timeout: Duration) -> Presence {
    let deadline = Instant::now() + timeout;
    loop {
        if presence(path) == Presence::Present {
            return Presence::Present;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Presence::Missing;
        }
        // Each wait re-arms on the nearest existing ancestor, so nested
        // directories (by-id appearing after the node) are followed step by step.
        if !platform::wait_for_change(path, remaining) {
            std::thread::sleep(remaining.min(POLL_INTERVAL));
        }
    }
}

#[cfg(target_os = "linux")]
fn nearest_existing_ancestor(path: &Path) -> Option<std::path::PathBuf> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path, time::Duration};

    /// Blocks until an entry changes in the watched directory or `timeout`
    /// elapses. Returns false when inotify could not be armed (caller polls).
    pub fn wait_for_change(path: &Path, timeout: Duration) -> bool {
        let Some(dir) = super::nearest_existing_ancestor(path) else {
            return false;
        };
        let Ok(c_dir) = CString::new(dir.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: plain syscalls on a descriptor owned and closed here.
        unsafe {
            let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
            if fd < 0 {
                return false;
            }
            let mask = libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_MOVED_TO
                | libc::IN_MOVED_FROM
                | libc::IN_ATTRIB
                | libc::IN_DELETE_SELF;
            if libc::inotify_add_watch(fd, c_dir.as_ptr(), mask) < 0 {
                libc::close(fd);
                return false;
            }
            // Re-check after arming: the entry may have appeared in between.
            if super::presence(path) == super::Presence::Present {
                libc::close(fd);
                return true;
            }
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX).max(1);
            let _ = libc::poll(&mut pollfd, 1, millis);
            libc::close(fd);
        }
        true
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use std::{path::Path, time::Duration};

    pub fn wait_for_change(_path: &Path, _timeout: Duration) -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "wingmankvm-hotplug-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn wakes_promptly_when_node_appears() {
        let root = temp_root("appear");
        let node = root.join("video0");
        let writer = node.clone();
        let spawned = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            std::fs::write(writer, b"").unwrap();
        });
        let start = Instant::now();
        assert_eq!(
            wait_until_present(&node, Duration::from_secs(5)),
            Presence::Present
        );
        assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
        spawned.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn follows_nested_by_id_directory_and_symlink() {
        let root = temp_root("byid");
        let target = root.join("video0");
        let link = root.join("v4l/by-id/usb-capture-video-index0");
        let (t, l) = (target.clone(), link.clone());
        let spawned = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            std::fs::write(&t, b"").unwrap();
            std::fs::create_dir_all(l.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&t, &l).unwrap();
        });
        assert_eq!(
            wait_until_present(&link, Duration::from_secs(5)),
            Presence::Present
        );
        spawned.join().unwrap();
        // A dangling symlink (target node removed) counts as missing.
        std::fs::remove_file(&target).unwrap();
        assert_eq!(presence(&link), Presence::Missing);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn times_out_while_absent() {
        let root = temp_root("absent");
        let start = Instant::now();
        assert_eq!(
            wait_until_present(&root.join("video9"), Duration::from_millis(300)),
            Presence::Missing
        );
        assert!(start.elapsed() >= Duration::from_millis(300));
        std::fs::remove_dir_all(root).unwrap();
    }
}
