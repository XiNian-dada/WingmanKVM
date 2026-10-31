use super::*;

pub(super) const SYSTEM_PASSWORD_SYNC_DEADLINE: Duration = Duration::from_secs(5);

pub(super) const MAX_SYSTEM_PASSWORD_BYTES: usize = 1024;

pub(super) const TERMINAL_DEGRADED_MESSAGE: &str =
    "终端凭证尚未同步，终端当前不可用；请退出后重新登录以重试。KVM 画面与输入不受影响。";

#[derive(Debug, thiserror::Error)]
pub(super) enum PasswordSyncError {
    #[error("system password is not valid for helper synchronization")]
    InvalidPassword,
    #[error("failed to start system password helper")]
    Spawn,
    #[error("failed to write system password helper request")]
    Write,
    #[error("system password helper exited unsuccessfully")]
    Exit,
    #[error("system password helper timed out")]
    Timeout,
    #[error("system password helper supervisor stopped")]
    Supervisor,
}

#[derive(Clone, Debug)]
pub(super) struct SystemPasswordHelper {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) deadline: Duration,
}

impl Default for SystemPasswordHelper {
    fn default() -> Self {
        Self {
            program: PathBuf::from("sudo"),
            args: vec![
                "-n".to_owned(),
                "/usr/local/sbin/wingmankvm-set-wingman-password".to_owned(),
            ],
            deadline: SYSTEM_PASSWORD_SYNC_DEADLINE,
        }
    }
}

impl SystemPasswordHelper {
    pub(super) fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        command
    }
}

pub(super) async fn sync_system_password(password: &str) -> Result<(), PasswordSyncError> {
    sync_system_password_with(password, &SystemPasswordHelper::default()).await
}

/// Run the helper under a supervisor task that owns the child process. If the
/// HTTP request is cancelled, the supervisor still enforces the deadline, kills
/// the child and reaps it. The deadline covers both the stdin write and wait.
pub(super) async fn sync_system_password_with(
    password: &str,
    helper: &SystemPasswordHelper,
) -> Result<(), PasswordSyncError> {
    if password.is_empty()
        || password.len() > MAX_SYSTEM_PASSWORD_BYTES
        || password
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r' | 0))
    {
        return Err(PasswordSyncError::InvalidPassword);
    }
    let password = password.to_owned();
    let helper = helper.clone();
    tokio::spawn(async move { supervise_system_password_sync(&helper, &password).await })
        .await
        .map_err(|_| PasswordSyncError::Supervisor)?
}

pub(super) async fn supervise_system_password_sync(
    helper: &SystemPasswordHelper,
    password: &str,
) -> Result<(), PasswordSyncError> {
    let mut child = helper.command().spawn().map_err(|_| PasswordSyncError::Spawn)?;
    let Some(mut stdin) = child.stdin.take() else {
        kill_and_reap_system_password_child(&mut child).await;
        return Err(PasswordSyncError::Write);
    };
    let exchange = async {
        stdin.write_all(password.as_bytes()).await?;
        stdin.shutdown().await?;
        drop(stdin);
        child.wait().await
    };
    match tokio::time::timeout(helper.deadline, exchange).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => Err(PasswordSyncError::Exit),
        Ok(Err(_)) => {
            kill_and_reap_system_password_child(&mut child).await;
            Err(PasswordSyncError::Write)
        }
        Err(_) => {
            kill_and_reap_system_password_child(&mut child).await;
            Err(PasswordSyncError::Timeout)
        }
    }
}

pub(super) async fn migrate_system_password_on_login(
    migration: Arc<AsyncMutex<()>>,
    auth: AuthStore,
    verified: AuthRecord,
    password: String,
    helper: SystemPasswordHelper,
) -> bool {
    let Ok(guard) = migration.try_lock_owned() else {
        // Do not queue login attempts behind a slow helper.
        return false;
    };
    tokio::spawn(supervise_system_password_migration(
        auth, verified, password, helper, guard,
    ))
    .await
    .unwrap_or(false)
}

pub(super) async fn supervise_system_password_migration(
    auth: AuthStore,
    verified: AuthRecord,
    password: String,
    helper: SystemPasswordHelper,
    guard: tokio::sync::OwnedMutexGuard<()>,
) -> bool {
    let _migration_guard = guard;
    let Ok(Some(current)) = auth.load() else {
        return false;
    };
    if current.username != verified.username
        || current.password_hash != verified.password_hash
    {
        return false;
    }
    if current.system_password_synced {
        return true;
    }
    if let Err(error) = sync_system_password_with(&password, &helper).await {
        tracing::warn!(%error, "system password migration failed during login");
        return false;
    }
    match auth.update(|record| {
        if record.username == verified.username
            && record.password_hash == verified.password_hash
        {
            record.mark_system_password_synced()?;
            Ok(true)
        } else {
            // The password changed while the helper was running; do not
            // claim synchronization.
            Ok(false)
        }
    }) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to persist system password migration marker"
            );
            false
        }
    }
}

#[cfg(unix)]
pub(super) fn signal_password_process_group(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        if pid > 1 {
            // The unreaped child is the leader of the private group created
            // by process_group(0). Signal it directly without spawning a tool.
            unsafe { libc::kill(-pid, libc::SIGKILL); }
        }
    }
}

pub(super) async fn kill_and_reap_system_password_child(child: &mut tokio::process::Child) {
    // Signal the task-owned group before reaping the leader so its id cannot be
    // reused for a different group. process_group(0) makes the child the leader
    // of a fresh group, never the server's shared group.
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            signal_password_process_group(pid);
        }
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::test_support::*;
    #[cfg(unix)]
    use std::{fs, sync::atomic::Ordering};

    #[cfg(unix)]
    fn helper_fixture(name: &str) -> PathBuf {
        let id = TEST_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "wingmankvm-helper-{}-{id}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(unix)]
    fn script_helper(root: &Path, script: &str, deadline: Duration) -> SystemPasswordHelper {
        SystemPasswordHelper {
            program: PathBuf::from("/bin/sh"),
            args: vec![
                "-c".to_owned(),
                script.to_owned(),
                "wingman-helper-test".to_owned(),
                root.to_string_lossy().into_owned(),
            ],
            deadline,
        }
    }

    #[cfg(unix)]
    fn process_is_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(unix)]
    async fn wait_for_pid_file(path: &Path) -> String {
        for _ in 0..100 {
            if let Ok(pid) = fs::read_to_string(path) {
                if !pid.trim().is_empty() {
                    return pid.trim().to_owned();
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("helper fixture did not record its pid");
    }

    #[cfg(unix)]
    async fn wait_until_process_exits(pid: &str) {
        for _ in 0..200 {
            if !process_is_alive(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("helper child {pid} was not killed and reaped");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_pipes_exact_password_without_newline() {
        let root = helper_fixture("success");
        let helper = script_helper(&root, r#"cat > "$1/password""#, Duration::from_secs(2));
        sync_system_password_with("Correct-Horse7!", &helper)
            .await
            .unwrap();
        assert_eq!(
            fs::read(root.join("password")).unwrap(),
            b"Correct-Horse7!"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_reports_a_failed_exit() {
        let root = helper_fixture("failure");
        let helper = script_helper(
            &root,
            r#"cat >/dev/null; exit 7"#,
            Duration::from_secs(2),
        );
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Exit)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_password_helper_times_out_and_reaps_the_child() {
        let root = helper_fixture("hang");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; exec sleep 30"#,
            Duration::from_secs(1),
        );
        let started = Instant::now();
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid = wait_for_pid_file(&root.join("pid")).await;
        wait_until_process_exits(&pid).await;
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_the_http_side_still_kills_and_reaps_the_helper() {
        let root = helper_fixture("cancel");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; /bin/cat >/dev/null; exec sleep 30"#,
            Duration::from_secs(1),
        );
        let pid_path = root.join("pid");
        let task = tokio::spawn(async move {
            sync_system_password_with("Correct-Horse7!", &helper).await
        });
        let pid = wait_for_pid_file(&pid_path).await;
        task.abort();
        let _ = task.await;
        wait_until_process_exits(&pid).await;
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_login_migration_keeps_the_guard_until_the_helper_ends() {
        let root = helper_fixture("migration-cancel");
        let store = AuthStore::new(root.join("auth.json"));
        store.initialize("admin", "Correct-Horse7!").unwrap();
        let verified = store.load().unwrap().unwrap();
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; exec sleep 2"#,
            Duration::from_secs(4),
        );
        let migration = Arc::new(AsyncMutex::new(()));
        let task = tokio::spawn(migrate_system_password_on_login(
            migration.clone(),
            store.clone(),
            verified,
            "Correct-Horse7!".to_owned(),
            helper,
        ));
        let pid = wait_for_pid_file(&root.join("pid")).await;
        task.abort();
        let _ = task.await;
        assert!(
            migration.clone().try_lock_owned().is_err(),
            "caller cancellation must not release the migration guard"
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(guard) = migration.clone().try_lock_owned() {
                drop(guard);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "migration guard was not released after the helper ended"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        wait_until_process_exits(&pid).await;
        assert!(store.load().unwrap().unwrap().system_password_synced);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn group_cleanup_terminates_owned_descendants() {
        let root = helper_fixture("descendants");
        let helper = script_helper(
            &root,
            r#"printf '%s' "$$" > "$1/pid"; /bin/sleep 30 & printf '%s' "$!" > "$1/descendant"; wait"#,
            Duration::from_secs(1),
        );
        assert!(matches!(
            sync_system_password_with("Correct-Horse7!", &helper).await,
            Err(PasswordSyncError::Timeout)
        ));
        let leader = wait_for_pid_file(&root.join("pid")).await;
        let descendant = wait_for_pid_file(&root.join("descendant")).await;
        wait_until_process_exits(&leader).await;
        wait_until_process_exits(&descendant).await;
        fs::remove_dir_all(root).unwrap();
    }
}
