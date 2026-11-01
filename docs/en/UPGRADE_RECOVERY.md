# Upgrade transactions and recovery

> This is a translation of [docs/UPGRADE_RECOVERY.md](../UPGRADE_RECOVERY.md); the Chinese version is authoritative.

WingmanKVM upgrades are transactional for the fixed managed deployment files.
All deployment sources are read and validated before any managed file changes.

## Transactional files

- `/usr/local/bin/wingmankvm`
- `/usr/local/sbin/wingmankvm-gadget`
- `/usr/local/sbin/wingmankvm-set-wingman-password`
- `/usr/local/sbin/wingmankvm-online-upgrade`
- `/etc/sudoers.d/wingmankvm`
- `/etc/sudoers.d/wingmankvm-maintenance-root` (explicit privileged mode only;
  restricted mode removes only the exact unmodified managed rule)
- `/etc/udev/rules.d/99-wingmankvm.rules`
- `/etc/systemd/system/wingmankvm-gadget.service`
- `/etc/systemd/system/wingmankvm.service`
- `/etc/systemd/system/wingmankvm-online-upgrade.service`
- `/etc/wingmankvm/gadget.env` (preserve-if-missing only)

An existing `gadget.env` is never overwritten, snapshotted, or removed. If it is
absent, the installer may create it; rollback removes only that exact file and
only while it still matches the staged bytes. The destination allowlist is an
exact list; the transaction never publishes arbitrary manifest paths.

The `wingman` login profile is **not** part of the code transaction. A missing
profile may be initialized before the transaction, but an existing profile is
never read, rewritten, or removed. Initialization can remain after a later
failure. The same applies to users, groups, directories, and other first-install
setup: this is not full OS, account, or package rollback. Service enabled/active
state is recorded and restored by runtime rollback as described below.

User/service state is never touched by rollback:

- `/var/lib/wingmankvm/config.json`
- `/var/lib/wingmankvm/auth.json`
- `/var/lib/wingmankvm/images/`
- any existing `gadget.env` or `wingman` profile
- attached virtual media and the active Gadget USB connection

## Store and validation

```
/var/lib/wingmankvm-deploy/          root:root 0700
  install.lock                       flock serialization
  PENDING                            present while files may be mutated
  txn.<UTC>.<pid>/
    manifest
    snapshot/<index>
    snapshot/<index>.sha256          digest recorded when the snapshot is taken
    stage/<index>
    recover.sh                       root-owned 0700, self-contained
    COMMITTED or ROLLED_BACK
```

The store is outside `/var/lib/wingmankvm` so the service account cannot rewrite
transaction metadata. A store with a symlink, wrong owner, or unsafe mode is
rejected. A new installer refuses to run while `PENDING` exists or another
installer holds the lock.

Before mutation the installer checks all sources exist/readable, runs `bash -n`
for the installer and profile, `sh -n` for both helpers, and `visudo -cf` for the
source sudoers file. It also validates the binary architecture, the transaction
store, and the health target. Missing or invalid input fails before managed
files are replaced.

## Health target

The health endpoint comes from `/var/lib/wingmankvm/config.json`:

```json
{"server":{"listen_address":"0.0.0.0","port":8080}}
```

`listen_address` must be a bare IP literal; `port` must be an integer from 1 to
65535. The default is `0.0.0.0:8080`. Wildcard IPv4 uses `127.0.0.1`; wildcard
IPv6 uses `::1`. Hostnames, embedded ports, and bracketed listen strings are
rejected. `--health-url` accepts only `http://LOOPBACK_IP:PORT/healthz` without
credentials or query data.

`python3` and GNU `timeout` are required. A single HTTP attempt has a short
socket timeout, and the whole probe is wrapped in
`timeout --kill-after=1s 16s python3 ...` so a slow body cannot defeat the
wall-clock bound. The probe polls for up to 15 seconds, retrying connection
failure and not-ready HTTP responses. A 200 response must be
`application/json`, at most 64 KiB, with exactly
`"status":"ok"` and `"service":"wingmankvm"`.

`--no-start` is file-only: it does not enable, start, restart, or health-check
services, and it reports explicitly that the result was **not runtime verified**.

## Runtime rollback

Before starting services the installer records enabled **and** active state for
both units. An already-active Gadget is never restarted, so USB is not
re-enumerated. `GADGET_STARTED_BY_US` is set before the new start attempt, so a
partially failed start is stopped on rollback.

If startup was attempted and fails, enabled state is restored and newly started
services are stopped while their unit files still exist. Before startup is
attempted, no service-state reversal is needed. The whole managed set is then
restored and udev/systemd are reloaded. A previously active app is restarted and
**health-verified** before rollback success is reported. Both the
original failure and rollback result are printed; a failed recovery remains
nonzero and retains the snapshot and pending marker.

## Traps and manual recovery

`EXIT`, `INT`, `TERM`, and `HUP` traps restore the complete managed set after
mutation starts. This includes `die`, ordinary nonzero exits, and trapped
signals. Rollback restores old bytes, mode, and ownership with per-file atomic
renames; it removes only destinations that were absent in the snapshot and still
match the staged bytes. It never recursively deletes, removes user data, or runs
git cleanup. An uncommitted transaction cannot exit successfully merely because
the shell would otherwise return zero. If the pending marker cannot be cleared,
the transaction is not reported complete.

Once `tx_commit` has written `COMMITTED` and cleared `PENDING`, the installer
removes the `INT`, `TERM`, and `HUP` traps, and the signal handler and rollback
entry point both refuse to act on a committed transaction. A later signal (for
example Ctrl-C while the installer prints the first-run link) therefore cannot
restore the previous files or create a contradictory `ROLLED_BACK` marker.

`SIGKILL` cannot run traps; the published `PENDING` marker remains and the next
installer refuses to change files. Power loss or kernel failure also cannot run
traps. Every snapshot is recorded with its SHA-256 digest, and the installer
runs `sync` before publishing `PENDING` and before clearing it on commit, so the
snapshots and digests that `recover.sh` relies on reach the disk first. This is
not a per-file fsync of every directory: it does not guarantee recovery from
storage loss or a filesystem crash. Inspect the retained store and managed files
after such an event. For an interrupted transaction, use the self-contained
recovery command:

```bash
sudo /var/lib/wingmankvm-deploy/txn.<...>/recover.sh
```

It needs only Bash, coreutils (including `sha256sum`), and `flock`; it does not
need a source checkout. It validates the same exact destination list, trusted
store/directory modes, manifest and snapshot ownership, and refuses unsafe
content. Before restoring a file it checks the snapshot against its recorded
SHA-256 digest; if the digest is missing, malformed, or does not match (for
example an empty or truncated snapshot after a power cut), that file fails, its
destination is left exactly as it is, and the error names the destination. The
automatic rollback performs the same check. It restores snapshots in reverse
order, clears `PENDING` only after all file operations succeed, and writes
`ROLLED_BACK`. If the transaction directory contains
`COMMITTED` and no `PENDING` marker, `recover.sh` refuses to run: that transaction
already succeeded, and restoring its snapshot would downgrade the managed files.
Use `--force` only when intentionally reverting a committed deployment:

```bash
sudo /var/lib/wingmankvm-deploy/txn.<...>/recover.sh --force
```

It does not restart services; after recovery reload and restart explicitly:

```bash
sudo udevadm control --reload-rules
sudo systemctl daemon-reload
sudo systemctl restart wingmankvm-gadget.service   # only if it should run
sudo systemctl restart wingmankvm.service
```

If recovery reports failures, keep the snapshot and fix the reported paths
before rerunning. A digest failure means that snapshot cannot be trusted: do not
edit the digest to make it pass; reinstall that file from a known-good release
instead. A successful upgrade also retains its snapshot and prints the
path. Retain last-known-good snapshots according to local policy; the installer
does not bulk-delete old transaction directories.

## Online upgrade from the web UI (#91)

Settings → System → **Online upgrade** checks the latest GitHub Release of
`XiNian-dada/WingmanKVM`, shows its notes and, after confirmation, upgrades
through the same transaction as `deploy/install.sh --release`.

Privilege path (the service itself stays unprivileged and hardened):

1. `POST /api/update/apply {"version":"vX.Y.Z"}` needs an authenticated
   session, passes the same-origin check of every mutating route, is
   single-flight and is audit-logged (`update_started`). The service looks up
   the latest release again and refuses unless the confirmed tag equals it and
   is newer than the running version. The tag is not passed on.
2. The service runs `sudo -n /usr/local/sbin/wingmankvm-online-upgrade --start`.
   `/etc/sudoers.d/wingmankvm` allows this for `wingmankvm` with exactly that
   argument list. `--start` reads no input and only runs
   `systemctl --no-block start wingmankvm-online-upgrade.service` (exit 3 if it
   is already running).
3. The root oneshot unit runs `wingmankvm-online-upgrade --run`. It ignores the
   environment, resolves the latest release of the fixed repository itself,
   accepts only a plain `vX.Y.Z` tag, detects `aarch64`/`armv7l`, downloads the
   archive and `SHA256SUMS`, verifies them with the same functions as
   `scripts/get.sh`, and runs the release's `deploy/install.sh --release`.
   The current maintenance mode is preserved (`--maintenance-mode privileged`
   only when the optional root grant already exists); a legacy managed
   `wingman ALL` rule makes it refuse, because that needs a host-root decision.
4. Progress is written to `/run/wingmankvm-online-upgrade/status` (root-owned,
   0644, kept after the run). The UI polls `/api/update/status`, keeps retrying
   while the service restarts, and reloads after success. Sessions are held in
   memory, so the restart signs the browser out; the outcome is shown after
   signing in again and recorded once as `update_succeeded`/`update_failed`.

Failure handling is the installer's: any error after the transaction starts
rolls back the managed files and restores the previous service; errors before
it (network, checksum, policy) change nothing. Inspect a run with
`journalctl -u wingmankvm-online-upgrade`. Manual recovery is unchanged (see
above).

Scope and limits:

- Only the latest published, non-prerelease release can be installed; there is
  no downgrade and no way to supply a URL, path or script.
- `SHA256SUMS` comes from the same GitHub release over HTTPS, so it protects
  against corrupted downloads, not against a compromised GitHub account or
  release; this is the same trust as `scripts/get.sh`.
- A compromised service can at most start an upgrade to the latest official
  release; it cannot choose the version or influence what root executes.
- The helper and unit are installed by this release's installer. A device
  running an older release must be upgraded once with `scripts/get.sh` or
  `deploy/install.sh` before the web UI can upgrade it.

Managed files added for this feature:
`/usr/local/sbin/wingmankvm-online-upgrade` (0755) and
`/etc/systemd/system/wingmankvm-online-upgrade.service` (0644).

## Harness

`scripts/verify_p2c_upgrade.py` runs the real library and installer integration
functions against disposable roots. It covers success, missing helper with zero
mutation, partial replacement, trapped `TERM`, a signal after commit, committed
`recover.sh` refusal/`--force`, `SIGKILL` plus pending rejection and manual
recovery, isolated-CWD Python imports in the health probe, new-start failure,
health mismatch, invalid JSON, delayed healthy polling, rollback failure,
`--no-start`, explicit legacy maintenance selection, optional root-grant
removal/rollback, and first-install failure before startup, during
enable/start, or during health checks. These first-install cases
use real `systemctl --root` enable/disable operations in an isolated file tree.
File bytes/modes/ownership/absence,
trap rollback, recovery and the HTTP probe are real; systemd/udev and two failure
injection wrappers are mocked. The JSON report records that distinction.
