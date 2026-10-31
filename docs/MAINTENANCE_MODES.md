# Maintenance modes and privileged root access

WingmanKVM has two explicit host-root-selected maintenance modes. Fresh
installs default to **restricted**; there is no HTTP, API, environment, or
user-config activation for either mode.

## Restricted (default)

`/etc/sudoers.d/wingmankvm` grants only:

- `wingmankvm` -> the fixed root-owned password helper, with **no arguments**
  (sudoers empty-argument syntax `""`);
- `wingmankvm` -> the fixed `/bin/bash -l` command as the fixed `wingman`
  account.
- `wingmankvm` -> `/usr/local/sbin/wingmankvm-online-upgrade --start` as root,
  with exactly that argument list. The helper takes no input and only starts
  the fixed `wingmankvm-online-upgrade.service` oneshot unit, which installs
  the latest official release transactionally (see
  [UPGRADE_RECOVERY.md](UPGRADE_RECOVERY.md)). It never changes the
  maintenance mode and grants nothing to `wingman`.

There is no `wingman ALL` rule in the managed default. The helper still reads
the password only from stdin and always targets `wingman`; its behavior is
unchanged.

Fresh install:

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm
# equivalent to:
sudo ./deploy/install.sh --binary /path/to/wingmankvm --maintenance-mode restricted
```

Disable privileged mode on an existing host:

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm --maintenance-mode restricted
```

That command may remove **only** the exact unmodified managed file
`/etc/sudoers.d/wingmankvm-maintenance-root`. Modified files, arbitrary
sudoers files, user data, and custom group memberships are never deleted
automatically.

Restricted mode also refuses to proceed if `wingman` is already a member of
`sudo`, `wheel`, or `admin`. Remove that membership manually or explicitly
select privileged mode. After managed rules are installed, the installer runs
`sudo -l -U wingman`; if any allowed sudo command is reported, installation
fails with an unexpected-host-policy error. The transactional upgrade layer is
responsible for restoring prior managed files.

This check does not claim isolation from unrelated polkit rules, setuid
programs, custom administrator grants, or other host policy.

## Privileged root maintenance

Enable it explicitly as host root:

```bash
sudo ./deploy/install.sh --binary /path/to/wingmankvm --maintenance-mode privileged
```

This installs root-owned `0440` sudoers content from
`deploy/wingmankvm-maintenance-root-sudoers` at
`/etc/sudoers.d/wingmankvm-maintenance-root`:

```sudoers
wingman ALL=(ALL:ALL) ALL
```

A password is still required. However, the service can reset that password, so
this mode deliberately accepts **service compromise -> host compromise**. Use
it only when root maintenance for the web terminal is an explicit requirement.

## Legacy upgrades

Older managed rules contained `wingman ALL=(ALL:ALL) ALL`. If that legacy rule
or the optional privileged file is present, the installer requires an explicit
`--maintenance-mode` before any account or file mutation:

```bash
# Remove the old managed privileged grant:
sudo ./deploy/install.sh --binary /path/to/wingmankvm --maintenance-mode restricted

# Keep root maintenance in the separate optional file:
sudo ./deploy/install.sh --binary /path/to/wingmankvm --maintenance-mode privileged
```

A deployment already using restricted managed rules upgrades normally without
the flag. Existing access is never silently revoked or silently preserved. Switching
policy does not terminate already-running root sessions or repair an already
compromised host; the host administrator must handle those separately.

## External grants

Custom sudoers entries, `%sudo`/`%wheel`/`%admin` memberships, polkit rules, or
other host authorization remain the administrator's responsibility. Restricted
mode rejects known administrator-group memberships and fails postflight if
`sudo -l -U wingman` reports allowed commands; it does not rewrite arbitrary
host policy. Review these grants manually.

## Actual unit writable paths

`deploy/wingmankvm.service` keeps these functional paths:

```ini
ReadWritePaths=/var/lib/wingmankvm /run/sudo /etc -/sys/kernel/config/usb_gadget
```

`/etc` is a writable mount inside the service mount namespace because the
fixed root-owned password helper updates `/etc/shadow` and its lock file.
Ordinary Unix permission checks still apply to the unprivileged service user.
The `nologin` shell on `wingmankvm` is not a security boundary by itself.

## Disposable-container verifier

`scripts/verify_p2c_privileges.py` is intended only inside a disposable Linux
container. It refuses to run without `--disposable-root`, uid 0, and a container
marker, and it never uses the network or invokes the real host password helper.
It creates fixture users and temporary container-only sudoers rules, runs real
`visudo` and `sudo`, and reports BLOCKED if real `sudo` is absent.

```bash
python3 scripts/verify_p2c_privileges.py \
  --repo . --report /tmp/p2c-privileges.json --disposable-root
```

Do not run it on a real host. No live host migration is performed or implied.
