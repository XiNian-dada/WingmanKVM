# Upgrade transactions and recovery

WingmanKVM upgrades are transactional for the fixed managed deployment files.
All deployment sources are read and validated before any managed file changes.

## Transactional files

- `/usr/local/bin/wingmankvm`
- `/usr/local/sbin/wingmankvm-gadget`
- `/usr/local/sbin/wingmankvm-set-wingman-password`
- `/etc/sudoers.d/wingmankvm`
- `/etc/udev/rules.d/99-wingmankvm.rules`
- `/etc/systemd/system/wingmankvm-gadget.service`
- `/etc/systemd/system/wingmankvm.service`
- `/etc/wingmankvm/gadget.env` (preserve-if-missing only)

An existing `gadget.env` is never overwritten, snapshotted, or removed. If it is
absent, the installer may create it; rollback removes only that exact file and
only while it still matches the staged bytes. The destination allowlist is an
exact list; the transaction never publishes arbitrary manifest paths.

The `wingman` login profile is **not** part of the code transaction. A missing
profile may be initialized before the transaction, but an existing profile is
never read, rewritten, or removed. Initialization can remain after a later
failure. The same applies to users, groups, directories, `systemctl enable`
state, and other first-install setup: this is not full OS, account, or package
rollback.

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

If the new app fails to start or fails the bounded `/healthz` probe, the whole
managed set is restored, udev/systemd are reloaded, enabled state is restored,
a Gadget started by this installer is stopped, and a previously active app is
restarted and **health-verified** before rollback success is reported. Both the
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

`SIGKILL`, power loss, or kernel failure cannot run traps. `PENDING` remains and
the next installer refuses to change files. Recover with the self-contained
command in the transaction directory:

```bash
sudo /var/lib/wingmankvm-deploy/txn.<...>/recover.sh
```

It needs only Bash, coreutils, and `flock`; it does not need a source checkout.
It validates the same exact destination list, trusted store/directory modes,
manifest and snapshot ownership, and refuses unsafe content. It restores
snapshots in reverse order, clears `PENDING` only after all file operations
succeed, and writes `ROLLED_BACK`. It does not restart services; after recovery
reload and restart explicitly:

```bash
sudo udevadm control --reload-rules
sudo systemctl daemon-reload
sudo systemctl restart wingmankvm-gadget.service   # only if it should run
sudo systemctl restart wingmankvm.service
```

If recovery reports failures, keep the snapshot and fix the reported paths
before rerunning. A successful upgrade also retains its snapshot and prints the
path. Retain last-known-good snapshots according to local policy; the installer
does not bulk-delete old transaction directories.

## Harness

`scripts/verify_p2c_upgrade.py` runs the real library and installer integration
functions against disposable roots. It covers success, missing helper with zero
mutation, partial replacement, trapped `TERM`, `SIGKILL` plus pending rejection
and manual recovery, new-start failure, health mismatch, invalid JSON, delayed
healthy polling, rollback failure, and `--no-start`. File bytes/modes/absence,
trap rollback, recovery and the HTTP probe are real; systemd/udev and two failure
injection wrappers are mocked. The JSON report records that distinction.
