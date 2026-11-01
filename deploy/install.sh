#!/usr/bin/env bash
set -Eeuo pipefail

PROGRAM=${0##*/}
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}

BINARY=
RELEASE_ARCHIVE=
RELEASE_TMPDIR=
NO_START=false
INSTALL_DEPS=false
MAINTENANCE_MODE=
WINGMAN_CREATED=false
DEST_ROOT=/
HEALTH_URL=
STATE_DIR=
APP_WAS_ACTIVE=false
APP_WAS_ENABLED=false
GADGET_WAS_ACTIVE=false
GADGET_WAS_ENABLED=false
GADGET_STARTED_BY_US=false
START_FAILURE=
SERVICES_START_ATTEMPTED=false
MDNS_ENABLED=false
MDNS_HOSTNAME=wingmankvm
MDNS_HOSTNAME_EXPLICIT=false
CONFIGURED_PORT=8080

usage() {
    cat <<EOF
Usage: sudo ./deploy/$PROGRAM (--binary /path/to/wingmankvm | --release /path/to/wingmankvm-VERSION-TARGET.tar.gz) [--no-start] [--install-deps] [--health-url URL] [--maintenance-mode MODE] [--mdns [--mdns-hostname NAME]]

Options:
  --binary PATH  Linux WingmanKVM executable to install
  --release FILE Release tar.gz to verify and install; the binary is read from it
  --no-start     Install files only; do not enable or start services
  --install-deps Install missing runtime packages with apt-get on Debian, Ubuntu or Armbian
  --health-url U Local loopback http /healthz override (IP literal only)
  --maintenance-mode MODE  restricted (default) or explicit privileged root maintenance
  --mdns         Advertise WingmanKVM on the LAN as NAME.local with avahi-daemon
                 (stable LAN address; Passkey still requires HTTPS)
  --mdns-hostname NAME  mDNS host label used with --mdns (default: wingmankvm)
  -h, --help     Show this help

Exactly one of --binary or --release is required.
EOF
}

log() {
    printf '%s: %s\n' "$PROGRAM" "$*"
}
warn() {
    printf '%s: warning: %s\n' "$PROGRAM" "$*" >&2
}
die() {
    printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

MISSING_ITEMS=()
MISSING_PACKAGES=()

add_missing_package() {
    local package=$1 existing
    for existing in "${MISSING_PACKAGES[@]}"; do
        [[ $existing == "$package" ]] && return 0
    done
    MISSING_PACKAGES+=("$package")
}

check_dependency() {
    local command=$1 package=$2
    if [[ $command == /* ]]; then
        if [[ ! -x $command ]]; then
            MISSING_ITEMS+=("$command")
            add_missing_package "$package"
        fi
    elif ! command -v "$command" >/dev/null 2>&1; then
        MISSING_ITEMS+=("$command")
        add_missing_package "$package"
    fi
}

collect_missing_dependencies() {
    local entry command package
    local -a dependencies=(
        "awk:mawk" "cmp:diffutils" "flock:util-linux" "getent:libc-bin"
        "groupadd:passwd" "install:coreutils" "mktemp:coreutils" "od:coreutils"
        "python3:python3" "systemctl:systemd" "timeout:coreutils"
        "udevadm:udev" "useradd:passwd" "usermod:passwd" "visudo:sudo"
    )

    MISSING_ITEMS=()
    MISSING_PACKAGES=()
    for entry in "${dependencies[@]}"; do
        IFS=: read -r command package <<<"$entry"
        check_dependency "$command" "$package"
    done
    if [[ -n $RELEASE_ARCHIVE ]]; then
        check_dependency tar tar
        check_dependency sha256sum coreutils
    fi
    check_dependency /bin/bash bash
    check_dependency /usr/bin/sudo sudo
    check_dependency /usr/sbin/chpasswd passwd

    # v4l-utils is used for post-install diagnostics; it is recommended but not
    # required by the installer itself.
    if ! command -v v4l2-ctl >/dev/null 2>&1; then
        add_missing_package v4l-utils
    fi
    if $MDNS_ENABLED; then
        check_dependency avahi-daemon avahi-daemon
    fi
}

debian_family_supported() {
    local family
    [[ -r /etc/os-release ]] || return 1
    family=$( (
        # shellcheck source=/dev/null
        . /etc/os-release
        printf '%s %s' "${ID:-}" "${ID_LIKE:-}"
    ) 2>/dev/null | tr '[:upper:]' '[:lower:]')
    case " $family " in
        *" debian "* | *" ubuntu "* | *" armbian "*) return 0 ;;
    esac
    return 1
}

install_missing_dependencies() {
    collect_missing_dependencies
    if ((${#MISSING_PACKAGES[@]} == 0)); then
        log "all runtime packages are already installed"
        return 0
    fi
    if ! debian_family_supported; then
        die "--install-deps supports Debian, Ubuntu and Armbian only; missing commands: ${MISSING_ITEMS[*]}; install them manually before running the installer"
    fi
    require_command apt-get
    log "installing runtime packages: ${MISSING_PACKAGES[*]}"
    DEBIAN_FRONTEND=noninteractive apt-get update \
        || die "apt-get update failed; check network and package sources, then retry with --install-deps"
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${MISSING_PACKAGES[@]}" \
        || die "apt-get install failed; install these packages manually: ${MISSING_PACKAGES[*]}"
    collect_missing_dependencies
    if ((${#MISSING_ITEMS[@]} > 0)); then
        die "dependencies are still missing after apt-get: ${MISSING_ITEMS[*]}"
    fi
    log "runtime packages installed"
}

check_libgpiod() {
    local version=
    if ! command -v gpioset >/dev/null 2>&1; then
        log "optional GPIO power control: gpioset is not installed; the installer never installs libgpiod automatically"
        log "on Debian bookworm use libgpiod-tools 2.x from bookworm-backports when GPIO power control is needed"
        return 0
    fi
    version=$(gpioset --version 2>/dev/null | head -n 1 || true)
    case $version in
        *"v2."*) log "detected libgpiod 2.x: $version" ;;
        *)
            warn "gpioset is not libgpiod 2.x ($version); timed power pulses need libgpiod 2.x from bookworm-backports; the installer will not replace it automatically"
            ;;
    esac
}

ensure_runtime_dependencies() {
    if $INSTALL_DEPS; then
        install_missing_dependencies
    fi
    collect_missing_dependencies
    if ((${#MISSING_ITEMS[@]} > 0)); then
        die "missing required commands: ${MISSING_ITEMS[*]}; rerun with --install-deps on Debian/Ubuntu/Armbian, or install them manually"
    fi
    check_libgpiod
}

source_file() {
    local path=$SCRIPT_DIR/$1
    [[ -f $path && -r $path ]] || die "deployment file is missing or unreadable: $path"
    printf '%s\n' "$path"
}

wingmankvm_release_cleanup() {
    local dir=${RELEASE_TMPDIR:-}
    [[ -n $dir ]] || return 0
    case $dir in
        */wingmankvm-release.*) ;;
        *)
            warn "refusing to remove unexpected release temp path: $dir"
            RELEASE_TMPDIR=
            return 0
            ;;
    esac
    if [[ -d $dir && ! -L $dir ]]; then
        rm -rf -- "$dir" || warn "could not remove release temp directory: $dir"
    fi
    RELEASE_TMPDIR=
}

_wingmankvm_release_exit_trap() {
    local status=$?
    wingmankvm_release_cleanup
    exit "$status"
}

verify_release_archive() {
    local archive=$1 sums name expected actual
    sums=$(dirname -- "$archive")/SHA256SUMS
    name=$(basename -- "$archive")
    if [[ ! -e $sums && ! -L $sums ]]; then
        log "SHA256SUMS not found next to $name; skipping checksum verification"
        return 0
    fi
    [[ -f $sums && -r $sums && ! -L $sums ]] \
        || die "SHA256SUMS is not a readable regular file: $sums"
    require_command sha256sum
    expected=$(awk -v name="$name" '
        {
            file = $0
            sub(/^[0-9a-fA-F]+[[:space:]]+[*]?/, "", file)
            if (file == name) { print tolower($1); exit }
        }' "$sums")
    [[ $expected =~ ^[0-9a-f]{64}$ ]] \
        || die "SHA256SUMS does not contain a valid checksum for $name"
    actual=$(sha256sum -- "$archive" | awk '{ print tolower($1); exit }')
    [[ $actual == "$expected" ]] || die "SHA256 checksum mismatch for $name"
    log "verified SHA256 checksum for $name"
}

prepare_release_archive() {
    local release_dir tmp_root entry listing
    release_dir=$(CDPATH='' cd -- "$(dirname -- "$RELEASE_ARCHIVE")" 2>/dev/null && pwd -P) \
        || die "release archive directory does not exist: $(dirname -- "$RELEASE_ARCHIVE")"
    RELEASE_ARCHIVE=$release_dir/$(basename -- "$RELEASE_ARCHIVE")
    [[ -f $RELEASE_ARCHIVE && -r $RELEASE_ARCHIVE && -s $RELEASE_ARCHIVE ]] \
        || die "release archive is missing, unreadable, or empty: $RELEASE_ARCHIVE"
    case $RELEASE_ARCHIVE in
        *.tar.gz | *.tgz) ;;
        *) die "release archive must be a .tar.gz or .tgz file: $RELEASE_ARCHIVE" ;;
    esac
    require_command tar
    verify_release_archive "$RELEASE_ARCHIVE"

    tmp_root=${TMPDIR:-/tmp}
    [[ -n $tmp_root ]] || tmp_root=/tmp
    tmp_root=${tmp_root%/}
    RELEASE_TMPDIR=$(mktemp -d "$tmp_root/wingmankvm-release.XXXXXXXX") \
        || die "cannot create a temporary directory for the release archive"
    trap _wingmankvm_release_exit_trap EXIT
    trap 'wingmankvm_release_cleanup; exit 129' HUP
    trap 'wingmankvm_release_cleanup; exit 130' INT
    trap 'wingmankvm_release_cleanup; exit 143' TERM

    listing=$(tar -tzf "$RELEASE_ARCHIVE") \
        || die "cannot read release archive: $RELEASE_ARCHIVE"
    while IFS= read -r entry; do
        case $entry in
            '' | '.' | './') continue ;;
        esac
        case $entry in
            /* | .. | ../* | */.. | */../*)
                die "release archive contains an unsafe path: $entry"
                ;;
        esac
    done <<<"$listing"

    tar -xzf "$RELEASE_ARCHIVE" -C "$RELEASE_TMPDIR" \
        --no-same-owner --no-same-permissions \
        || die "cannot extract release archive: $RELEASE_ARCHIVE"
    BINARY=$RELEASE_TMPDIR/wingmankvm
    [[ -f $BINARY && -r $BINARY && -s $BINARY && ! -L $BINARY ]] \
        || die "release archive does not contain a top-level wingmankvm binary"
    log "extracted release archive to a temporary directory"
}

# shellcheck source=upgrade-transaction.sh
. "$SCRIPT_DIR/upgrade-transaction.sh" || die "cannot load upgrade transaction library"

dest_path() {
    local path=$1
    [[ $path == /* ]] || return 1
    printf '%s\n' "${DEST_ROOT%/}$path"
}

install_directory() {
    local destination=$1 mode=$2 owner=${3:-root} group=${4:-root}
    install -d -m "$mode" -- "$destination"
    if [[ $(id -u) -eq 0 ]]; then
        chown "$owner:$group" -- "$destination"
    fi
}

ensure_install_directories() {
    install_directory "$(dest_path /etc/wingmankvm)" 0755
    install_directory "$(dest_path /etc/sudoers.d)" 0750
    install_directory "$(dest_path /etc/udev/rules.d)" 0755
    install_directory "$(dest_path /etc/systemd/system)" 0755
    install_directory "$(dest_path /usr/local/bin)" 0755
    install_directory "$(dest_path /usr/local/sbin)" 0755
}


# Root-selected policy only: never controlled by a web request or service state.
managed_sudoers_grants_wingman_all() {
    [[ -f $1 && -r $1 ]] || return 1
    grep -Eq '^[[:space:]]*wingman[[:space:]]+ALL[[:space:]]*=' "$1"
}
resolve_maintenance_mode() {
    local optional
    optional=$(dest_path /etc/sudoers.d/wingmankvm-maintenance-root)
    if managed_sudoers_grants_wingman_all "$(dest_path /etc/sudoers.d/wingmankvm)" \
        || [[ -e $optional || -L $optional ]]; then
        [[ -n $MAINTENANCE_MODE ]] || die "existing root maintenance detected; explicitly select --maintenance-mode restricted or privileged before upgrading; no managed files changed"
    fi
    MAINTENANCE_MODE=${MAINTENANCE_MODE:-restricted}
    case $MAINTENANCE_MODE in
        restricted | privileged) ;;
        *) die "--maintenance-mode must be restricted or privileged" ;;
    esac
}
preflight_maintenance_mode() {
    local group groups optional
    [[ $MAINTENANCE_MODE == restricted ]] || return 0
    if getent passwd wingman >/dev/null; then
        [[ $(id -u wingman) -ne 0 ]] || die "existing wingman account must not be root"
        groups=$(id -nG wingman)
        for group in sudo wheel admin; do
            [[ " $groups " != *" $group "* ]] || die "wingman is in host administrator group '$group'; review this grant manually before choosing restricted mode"
        done
    fi
    optional=$(dest_path /etc/sudoers.d/wingmankvm-maintenance-root)
    if [[ -e $optional || -L $optional ]]; then
        [[ -f $optional && ! -L $optional ]] || die "refusing to remove non-regular optional privilege rule: $optional"
        cmp -s "$MAINTENANCE_ROOT_SUDOERS" "$optional" || die "optional privilege rule was customized; review it manually: $optional"
    fi
}
stage_maintenance_mode() {
    local target
    target=$(dest_path /etc/sudoers.d/wingmankvm-maintenance-root)
    if [[ ${MAINTENANCE_MODE:-restricted} == privileged ]]; then
        tx_add_managed "$MAINTENANCE_ROOT_SUDOERS" "$target" 0440 root root
    else
        tx_add_absent "$MAINTENANCE_ROOT_SUDOERS" "$target"
    fi
}
postflight_maintenance_mode() {
    local listing
    [[ $MAINTENANCE_MODE == restricted ]] || return 0
    if listing=$(LC_ALL=C sudo -n -l -U wingman 2>&1); then
        die "unexpected host sudo grant for wingman in restricted mode; review foreign sudoers/group policy; managed files will be rolled back"
    elif [[ $listing == *"not allowed to run sudo"* || $listing == *"may not run sudo"* ]]; then
        return 0
    fi
    die "could not verify effective sudo policy for wingman"
}

create_group() {
    local group=$1
    local kind=${2:-system}

    if getent group "$group" >/dev/null; then
        log "group already exists: $group"
    elif [[ $kind == system ]]; then
        groupadd --system "$group"
        log "created system group: $group"
    else
        groupadd "$group"
        log "created group: $group"
    fi
}

nologin_shell() {
    local candidate
    for candidate in /usr/sbin/nologin /sbin/nologin /bin/false; do
        if [[ -x $candidate ]]; then
            printf '%s\n' "$candidate"
            return
        fi
    done
    die "no nologin shell found"
}

create_users() {
    local service_shell
    local wingman_group
    local wingman_home

    create_group wingmankvm-hw system
    create_group wingmankvm system

    if getent passwd wingmankvm >/dev/null; then
        [[ $(id -u wingmankvm) -ne 0 ]] || die "existing wingmankvm account must not be root"
        log "user already exists: wingmankvm"
    else
        service_shell=$(nologin_shell)
        useradd --system --gid wingmankvm --home-dir /var/lib/wingmankvm \
            --shell "$service_shell" --no-create-home wingmankvm
        log "created service user: wingmankvm"
    fi
    usermod -a -G wingmankvm-hw wingmankvm

    create_group wingman regular
    if getent passwd wingman >/dev/null; then
        [[ $(id -u wingman) -ne 0 ]] || die "existing wingman account must not be root"
        log "user already exists: wingman"
    else
        useradd --gid wingman --home-dir /home/wingman --shell /bin/bash \
            --create-home wingman
        WINGMAN_CREATED=true
        log "created terminal user: wingman"
    fi

    wingman_group=$(id -gn wingman)
    wingman_home=$(getent passwd wingman | awk -F: '{print $6}')
    if [[ $wingman_home != /home/wingman ]]; then
        warn "wingman uses home $wingman_home; WingmanKVM terminal sessions use /home/wingman"
    fi

    install -d -o wingmankvm -g wingmankvm -m 0700 /var/lib/wingmankvm
    install -d -o wingmankvm -g wingmankvm -m 0700 /var/lib/wingmankvm/images
    install -d -o wingman -g "$wingman_group" -m 0750 /home/wingman
}

has_configured_role_switch() {
    local environment_file=$1
    local line
    local value
    [[ -r $environment_file ]] || return 1

    while IFS= read -r line; do
        if [[ $line =~ ^[[:space:]]*WINGMANKVM_USB_ROLE_PATH[[:space:]]*= ]]; then
            value=${line#*=}
            value=${value%%#*}
            value=$(printf '%s' "$value" | tr -d "[:space:]\"'")
            [[ -n $value ]] && return 0
        fi
    done <"$environment_file"
    return 1
}

# Advisory only: print the doctor's key findings before the installer's own
# platform gates run. Failures here never block installation; check_platform
# below remains the single authority for what is fatal.
run_doctor_preflight() {
    local doctor=$1 environment_file=$2 output status
    log "running advisory preflight: wingmankvm-doctor --preflight"
    set +e
    output=$(bash "$doctor" --preflight --env-file "$environment_file" 2>&1)
    status=$?
    set -e
    [[ -z $output ]] || printf '%s\n' "$output"
    if ((status != 0)); then
        warn "wingmankvm-doctor preflight exited with status $status; installer checks remain authoritative"
    fi
    return 0
}

check_platform() {
    local environment_file=$1
    local udcs=()

    [[ $(uname -s) == Linux ]] || die "WingmanKVM must be installed on Linux"
    [[ -d /run/systemd/system ]] || die "systemd is not running as the system manager"
    systemctl show-environment >/dev/null 2>&1 || die "cannot communicate with systemd"

    [[ -d /sys/kernel/config ]] || die "configfs mount point is missing: /sys/kernel/config"
    [[ -d /sys/class/udc ]] || die "USB Device Controller class is unavailable: /sys/class/udc"

    if [[ ! -d /sys/kernel/config/usb_gadget ]]; then
        if command -v modprobe >/dev/null 2>&1 && modprobe -n libcomposite >/dev/null 2>&1; then
            :
        elif ! grep -qw configfs /proc/filesystems; then
            die "kernel USB Gadget/configfs support is unavailable"
        fi
    fi

    shopt -s nullglob
    udcs=(/sys/class/udc/*)
    shopt -u nullglob
    if ((${#udcs[@]} == 0)); then
        if has_configured_role_switch "$environment_file"; then
            warn "no UDC is visible yet; the configured USB role switch will be applied at service start"
        elif $NO_START; then
            warn "no UDC is visible; configure the OTG port or USB role before starting wingmankvm-gadget.service"
        else
            die "no USB Device Controller is visible; rerun with --no-start, configure /etc/wingmankvm/gadget.env, then start the services"
        fi
    else
        log "detected USB Device Controller: ${udcs[0]##*/}"
    fi
}

reload_udev() {
    udevadm control --reload-rules
    udevadm trigger --subsystem-match=video4linux --action=change >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=gpio --action=change >/dev/null 2>&1 || true
}

load_deployment_sources() {
    UDEV_RULES=$(source_file 99-wingmankvm.rules)
    WINGMAN_PROFILE=$(source_file wingman.bash_profile)
    PASSWORD_HELPER=$(source_file wingmankvm-set-wingman-password)
    SETUP_URL_HELPER=$(source_file wingmankvm-setup-url)
    SUDOERS_RULE=$(source_file wingmankvm-sudoers)
    MAINTENANCE_ROOT_SUDOERS=$(source_file wingmankvm-maintenance-root-sudoers)
    GADGET_SCRIPT=$(source_file wingmankvm-gadget)
    DOCTOR_SCRIPT=$(source_file wingmankvm-doctor)
    ONLINE_UPGRADE_HELPER=$(source_file wingmankvm-online-upgrade)
    ONLINE_UPGRADE_SERVICE=$(source_file wingmankvm-online-upgrade.service)
    GADGET_ENV_EXAMPLE=$(source_file gadget.env.example)
    GADGET_SERVICE=$(source_file wingmankvm-gadget.service)
    APPLICATION_SERVICE=$(source_file wingmankvm.service)

    bash -n "$SCRIPT_DIR/install.sh" || die "installer failed shell syntax validation"
    bash -n "$WINGMAN_PROFILE" || die "login profile failed shell syntax validation"
    sh -n "$GADGET_SCRIPT" || die "Gadget helper failed shell syntax validation"
    bash -n "$DOCTOR_SCRIPT" || die "doctor helper failed shell syntax validation"
    bash -n "$ONLINE_UPGRADE_HELPER" || die "online upgrade helper failed shell syntax validation"
    sh -n "$PASSWORD_HELPER" || die "password helper failed shell syntax validation"
    bash -n "$SETUP_URL_HELPER" || die "setup URL helper failed shell syntax validation"
    visudo -cf "$SUDOERS_RULE" >/dev/null || die "sudoers source failed validation"
    visudo -cf "$MAINTENANCE_ROOT_SUDOERS" >/dev/null || die "optional root sudoers source failed validation"
}

install_managed_files() {
    tx_begin
    tx_add_managed "$BINARY" \
        "$(dest_path /usr/local/bin/wingmankvm)" 0755 root root
    tx_add_managed "$GADGET_SCRIPT" \
        "$(dest_path /usr/local/sbin/wingmankvm-gadget)" 0755 root root
    tx_add_managed "$DOCTOR_SCRIPT" \
        "$(dest_path /usr/local/sbin/wingmankvm-doctor)" 0755 root root
    tx_add_managed "$ONLINE_UPGRADE_HELPER" \
        "$(dest_path /usr/local/sbin/wingmankvm-online-upgrade)" 0755 root root
    tx_add_managed "$PASSWORD_HELPER" \
        "$(dest_path /usr/local/sbin/wingmankvm-set-wingman-password)" 0755 root root
    tx_add_managed "$SETUP_URL_HELPER" \
        "$(dest_path /usr/local/sbin/wingmankvm-setup-url)" 0755 root root
    tx_add_managed "$SUDOERS_RULE" \
        "$(dest_path /etc/sudoers.d/wingmankvm)" 0440 root root
    tx_add_managed "$UDEV_RULES" \
        "$(dest_path /etc/udev/rules.d/99-wingmankvm.rules)" 0644 root root
    tx_add_managed "$GADGET_SERVICE" \
        "$(dest_path /etc/systemd/system/wingmankvm-gadget.service)" 0644 root root
    tx_add_managed "$APPLICATION_SERVICE" \
        "$(dest_path /etc/systemd/system/wingmankvm.service)" 0644 root root
    tx_add_managed "$ONLINE_UPGRADE_SERVICE" \
        "$(dest_path /etc/systemd/system/wingmankvm-online-upgrade.service)" 0644 root root
    tx_add_preserved "$GADGET_ENV_EXAMPLE" \
        "$(dest_path /etc/wingmankvm/gadget.env)" 0644 root root
    stage_maintenance_mode
    tx_stage_all
    tx_apply
    log "managed files staged and published transactionally"
}

# This intentionally runs outside the managed-file transaction. It only creates
# a missing profile; an existing profile is never read, rewritten, or removed.
# A later failure can leave this first-install initialization in place.
install_missing_login_profile() {
    local destination destination_group
    if ! $WINGMAN_CREATED; then
        log "existing terminal account: login profile left unchanged"
        return 0
    fi
    destination=$(dest_path /home/wingman/.bash_profile)
    destination_group=${WINGMAN_GROUP:-root}
    [[ ! -e $destination && ! -L $destination ]] || {
        log "preserved existing login profile: $destination"
        return 0
    }
    install -m 0644 -- "$WINGMAN_PROFILE" "$destination"
    if [[ $(id -u) -eq 0 ]]; then
        chown wingman:"$destination_group" -- "$destination"
    fi
    log "initialized missing login profile outside the rollback transaction: $destination"
}

resolve_health_url() {
    local config host port parsed
    config=$(dest_path /var/lib/wingmankvm/config.json)
    if [[ -e $config || -L $config ]]; then
        [[ -f $config && -r $config && ! -L $config ]] \
            || die "config.json is not a readable regular file: $config"
        parsed=$(python3 -I - "$config" <<'PY'
import ipaddress, json, sys
path = sys.argv[1]
try:
    with open(path, "rb") as handle:
        data = json.load(handle)
except Exception as exc:
    print(f"cannot parse {path}: {exc}", file=sys.stderr)
    raise SystemExit(1)
if not isinstance(data, dict):
    print("config.json root must be an object", file=sys.stderr)
    raise SystemExit(1)
server = data.get("server")
if server is None:
    server = {}
if not isinstance(server, dict):
    print("server must be an object", file=sys.stderr)
    raise SystemExit(1)
address = server.get("listen_address", "0.0.0.0")
port = server.get("port", 8080)
if not isinstance(address, str):
    print("server.listen_address must be a bare IP string", file=sys.stderr)
    raise SystemExit(1)
try:
    ip = ipaddress.ip_address(address)
except ValueError as exc:
    print(f"server.listen_address must be a bare IP literal: {exc}", file=sys.stderr)
    raise SystemExit(1)
if isinstance(port, bool) or not isinstance(port, int) or not 1 <= port <= 65535:
    print("server.port must be an integer in 1..65535", file=sys.stderr)
    raise SystemExit(1)
if ip.is_unspecified:
    host = "127.0.0.1" if ip.version == 4 else "::1"
else:
    host = str(ip)
print(host)
print(port)
PY
) || die "existing config.json has an invalid server.listen_address/server.port"
        host=$(printf '%s\n' "$parsed" | sed -n '1p')
        port=$(printf '%s\n' "$parsed" | sed -n '2p')
    else
        host=127.0.0.1
        port=8080
    fi
    CONFIGURED_PORT=$port
    if [[ -n $HEALTH_URL ]]; then
        python3 -I - "$HEALTH_URL" <<'PY' || die "invalid --health-url"
import ipaddress, sys, urllib.parse
value = sys.argv[1]
try:
    parsed = urllib.parse.urlsplit(value)
    port = parsed.port
except ValueError as exc:
    print(f"invalid URL: {exc}", file=sys.stderr)
    raise SystemExit(1)
if parsed.scheme != "http" or parsed.username is not None or parsed.password is not None:
    print("health URL must be plain http without credentials", file=sys.stderr)
    raise SystemExit(1)
if parsed.path != "/healthz" or parsed.query or parsed.fragment or port is None:
    print("health URL must be exactly http://IP:PORT/healthz", file=sys.stderr)
    raise SystemExit(1)
try:
    ip = ipaddress.ip_address(parsed.hostname or "")
except ValueError as exc:
    print(f"health URL host must be a bare IP literal: {exc}", file=sys.stderr)
    raise SystemExit(1)
if not ip.is_loopback:
    print("health URL must use a loopback IP literal", file=sys.stderr)
    raise SystemExit(1)
if not 1 <= port <= 65535:
    print("health URL port is out of range", file=sys.stderr)
    raise SystemExit(1)
PY
        return 0
    fi
    if [[ $host == *:* ]]; then
        HEALTH_URL="http://[$host]:$port/healthz"
    else
        HEALTH_URL="http://$host:$port/healthz"
    fi
}

probe_health() {
    local url=$1
    timeout --kill-after=1s 16s python3 -I - "$url" <<'PY'
import http.client
import json
import sys
import time
import urllib.parse

url = sys.argv[1]
parsed = urllib.parse.urlsplit(url)
deadline = time.monotonic() + 15.0
last = "no attempt completed"
while True:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        break
    try:
        connection = http.client.HTTPConnection(
            parsed.hostname, parsed.port, timeout=min(2.0, remaining)
        )
        connection.request("GET", parsed.path, headers={"Accept": "application/json"})
        response = connection.getresponse()
        body = response.read(65537)
        status = response.status
        content_type = (response.getheader("Content-Type") or "").split(";")[0].strip().lower()
        connection.close()
        if status != 200:
            last = f"HTTP {status}"
        elif len(body) > 65536:
            print("health response is too large", file=sys.stderr)
            raise SystemExit(1)
        elif content_type != "application/json":
            print(f"health response content type is {content_type!r}", file=sys.stderr)
            raise SystemExit(1)
        else:
            try:
                payload = json.loads(body.decode("utf-8"))
            except Exception as exc:
                print(f"health response is not valid JSON: {exc}", file=sys.stderr)
                raise SystemExit(1)
            if (
                not isinstance(payload, dict)
                or payload.get("status") != "ok"
                or payload.get("service") != "wingmankvm"
            ):
                print("health JSON must contain status=ok and service=wingmankvm", file=sys.stderr)
                raise SystemExit(1)
            raise SystemExit(0)
    except SystemExit:
        raise
    except Exception as exc:
        last = f"{type(exc).__name__}: {exc}"
    time.sleep(min(0.25, max(0.0, deadline - time.monotonic())))
print(f"health check at {url} failed within 15s: {last}", file=sys.stderr)
raise SystemExit(1)
PY
}

runtime_prepare() {
    APP_WAS_ACTIVE=false
    APP_WAS_ENABLED=false
    GADGET_WAS_ACTIVE=false
    GADGET_WAS_ENABLED=false
    GADGET_STARTED_BY_US=false
    START_FAILURE=
    SERVICES_START_ATTEMPTED=false
    systemctl is-active --quiet wingmankvm.service && APP_WAS_ACTIVE=true
    systemctl is-enabled --quiet wingmankvm.service && APP_WAS_ENABLED=true
    systemctl is-active --quiet wingmankvm-gadget.service && GADGET_WAS_ACTIVE=true
    systemctl is-enabled --quiet wingmankvm-gadget.service && GADGET_WAS_ENABLED=true
    return 0
}

restore_enabled_state() {
    local ok=true
    if $APP_WAS_ENABLED; then
        systemctl enable wingmankvm.service >/dev/null 2>&1 \
            || { warn "could not re-enable wingmankvm.service"; ok=false; }
    else
        systemctl disable wingmankvm.service >/dev/null 2>&1 \
            || { warn "could not restore wingmankvm.service disabled state"; ok=false; }
    fi
    if $GADGET_WAS_ENABLED; then
        systemctl enable wingmankvm-gadget.service >/dev/null 2>&1 \
            || { warn "could not re-enable wingmankvm-gadget.service"; ok=false; }
    else
        systemctl disable wingmankvm-gadget.service >/dev/null 2>&1 \
            || { warn "could not restore wingmankvm-gadget.service disabled state"; ok=false; }
    fi
    $ok
}

wingmankvm_files_rollback() {
    wingmankvm_release_cleanup
    reload_udev && systemctl daemon-reload
}

wingmankvm_runtime_pre_rollback() {
    wingmankvm_release_cleanup
    # Before start_services, no enabled/active state has been changed. In
    # particular, missing first-install units need no disable/stop operation.
    $SERVICES_START_ATTEMPTED || return 0
    local ok=true
    restore_enabled_state || ok=false
    if $GADGET_STARTED_BY_US; then
        systemctl stop wingmankvm-gadget.service \
            || { warn "could not stop the Gadget started by this installer"; ok=false; }
    fi
    if ! $APP_WAS_ACTIVE; then
        systemctl stop wingmankvm.service >/dev/null 2>&1 \
            || { warn "could not leave the previously inactive app stopped"; ok=false; }
    fi
    $ok
}

wingmankvm_runtime_rollback() {
    wingmankvm_release_cleanup
    local ok=true
    reload_udev || { warn "could not reload udev rules during rollback"; ok=false; }
    systemctl daemon-reload || { warn "could not reload systemd during rollback"; ok=false; }
    if $APP_WAS_ACTIVE; then
        if systemctl restart wingmankvm.service; then
            if probe_health "$HEALTH_URL"; then
                log "previously active WingmanKVM app was restored and passed /healthz"
            else
                warn "previously active WingmanKVM app restarted but did not pass /healthz"
                ok=false
            fi
        else
            warn "could not restart the previously active WingmanKVM app"
            ok=false
        fi
    fi
    $ok
}

start_services() {
    SERVICES_START_ATTEMPTED=true
    systemctl enable wingmankvm-gadget.service wingmankvm.service \
        || { START_FAILURE="could not enable WingmanKVM services"; return 1; }
    if systemctl is-active --quiet wingmankvm-gadget.service; then
        log "Gadget service is already active; leaving the current USB connection intact"
    else
        GADGET_STARTED_BY_US=true
        if ! systemctl start wingmankvm-gadget.service; then
            systemctl --no-pager --full status wingmankvm-gadget.service || true
            START_FAILURE="failed to start USB Gadget; check /etc/wingmankvm/gadget.env and the service log"
            return 1
        fi
    fi
    if ! systemctl restart wingmankvm.service; then
        systemctl --no-pager --full status wingmankvm.service || true
        START_FAILURE="failed to start WingmanKVM"
        return 1
    fi
    if ! systemctl is-active --quiet wingmankvm.service; then
        systemctl --no-pager --full status wingmankvm.service || true
        START_FAILURE="WingmanKVM did not remain active"
        return 1
    fi
    if ! probe_health "$HEALTH_URL"; then
        START_FAILURE="WingmanKVM did not pass /healthz verification at $HEALTH_URL"
        return 1
    fi
    return 0
}

handle_start_failure() {
    local original=$START_FAILURE
    [[ -n $original ]] || original="new services failed to start or pass health verification"
    if tx_rollback "$original"; then
        die "$original; previous managed files were restored and runtime recovery succeeded"
    fi
    die "$original; automatic rollback failed; keep the printed snapshot and run the recovery command"
}

finish_no_start() {
    tx_commit
    log "installation complete; the installer did not enable or start services"
    log "file publication succeeded but was NOT runtime verified (--no-start)"
    log "any services that were already running were left unchanged"
    log "review /etc/wingmankvm/gadget.env, then run: systemctl enable --now wingmankvm-gadget.service wingmankvm.service"
    log "after the first start, print the one-time setup link with: sudo wingmankvm-setup-url"
}

# mDNS (avahi) support. Runs after the managed-file transaction commits and is
# intentionally best effort: avahi files are owned by the avahi package, not by
# the upgrade transaction, so a later failure never rolls them back and an
# upgrade without --mdns leaves an existing advertisement unchanged.
valid_mdns_hostname() {
    local name=$1
    ((${#name} >= 1 && ${#name} <= 63)) || return 1
    [[ $name =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]]
}

mdns_service_path() {
    dest_path /etc/avahi/services/wingmankvm.service
}

mdns_conf_path() {
    dest_path /etc/avahi/avahi-daemon.conf
}

write_mdns_service_file() {
    local port=$1 target tmp
    target=$(mdns_service_path)
    install -d -m 0755 "$(dirname -- "$target")" || return 1
    tmp=$(mktemp "$(dirname -- "$target")/.wingmankvm.service.XXXXXX") || return 1
    cat >"$tmp" <<AVAHI_EOF
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<!-- Installed by WingmanKVM deploy/install.sh --mdns -->
<service-group>
  <name replace-wildcards="yes">WingmanKVM on %h</name>
  <service>
    <type>_http._tcp</type>
    <port>$port</port>
    <txt-record>path=/</txt-record>
  </service>
</service-group>
AVAHI_EOF
    if ! chmod 0644 "$tmp" || ! mv -f -- "$tmp" "$target"; then
        rm -f -- "$tmp"
        return 1
    fi
}

# Prints the host-name= value from the [server] section, if any.
mdns_conf_host_name() {
    local conf=$1
    [[ -f $conf && -r $conf ]] || return 0
    awk '
        /^[[:space:]]*\[/ { in_server = ($0 ~ /^[[:space:]]*\[server\]/); next }
        in_server && /^[[:space:]]*host-name[[:space:]]*=/ {
            sub(/^[^=]*=[[:space:]]*/, ""); sub(/[[:space:]]*$/, ""); print; exit
        }
    ' "$conf"
}

# Sets host-name= in the [server] section without touching anything else. The
# first edit keeps a one-time backup next to the file for manual restore.
set_mdns_conf_host_name() {
    local conf=$1 name=$2 tmp
    [[ -f $conf && ! -L $conf ]] || return 1
    [[ -e $conf.wingmankvm.bak ]] || cp -p -- "$conf" "$conf.wingmankvm.bak" || return 1
    tmp=$(mktemp "$conf.XXXXXX") || return 1
    if ! awk -v name="$name" '
        function emit() { if (!done) { print "host-name=" name; done = 1 } }
        /^[[:space:]]*\[/ {
            if (in_server) emit()
            in_server = ($0 ~ /^[[:space:]]*\[server\]/)
            if (in_server) seen_server = 1
            print; next
        }
        in_server && /^[[:space:]]*#?[[:space:]]*host-name[[:space:]]*=/ { emit(); next }
        { print }
        END {
            if (in_server) emit()
            else if (!seen_server) { print "[server]"; print "host-name=" name }
        }
    ' "$conf" >"$tmp"; then
        rm -f -- "$tmp"
        return 1
    fi
    chmod 0644 "$tmp"
    if ! mv -f -- "$tmp" "$conf"; then
        rm -f -- "$tmp"
        return 1
    fi
}

configure_mdns() {
    local port=$1 conf system_name current restart_needed=false
    $MDNS_ENABLED || return 0
    if ! write_mdns_service_file "$port"; then
        warn "mDNS: could not write $(mdns_service_path); LAN name advertising was skipped"
        return 0
    fi
    log "mDNS: advertising _http._tcp on port $port via $(mdns_service_path)"

    # Least intrusive: never change the system hostname. Only set avahi's own
    # host-name when the requested label differs from what avahi would use.
    conf=$(mdns_conf_path)
    system_name=$(hostname -s 2>/dev/null || hostname 2>/dev/null || true)
    system_name=$(printf '%s' "$system_name" | tr '[:upper:]' '[:lower:]')
    current=$(mdns_conf_host_name "$conf")
    if [[ -n $current ]]; then
        current=$(printf '%s' "$current" | tr '[:upper:]' '[:lower:]')
    else
        current=$system_name
    fi
    if [[ $current != "$MDNS_HOSTNAME" ]]; then
        if [[ -f $conf ]] && set_mdns_conf_host_name "$conf" "$MDNS_HOSTNAME"; then
            log "mDNS: set host-name=$MDNS_HOSTNAME in $conf (backup: $conf.wingmankvm.bak); the system hostname is unchanged"
            restart_needed=true
        else
            warn "mDNS: could not set host-name in $conf; avahi keeps announcing ${current:-the system hostname}.local"
        fi
    fi

    if $NO_START; then
        log "mDNS: --no-start given; run later: systemctl enable --now avahi-daemon.service"
        return 0
    fi
    systemctl enable avahi-daemon.service >/dev/null 2>&1 \
        || warn "mDNS: could not enable avahi-daemon.service"
    if $restart_needed || ! systemctl is-active --quiet avahi-daemon.service; then
        systemctl restart avahi-daemon.service \
            || warn "mDNS: avahi-daemon.service failed to (re)start; check: systemctl status avahi-daemon.service"
    else
        systemctl reload avahi-daemon.service >/dev/null 2>&1 || true
    fi
}

# Prints NAME.local when avahi-daemon is running, otherwise nothing.
mdns_access_host() {
    local name
    command -v systemctl >/dev/null 2>&1 || return 0
    systemctl is-active --quiet avahi-daemon.service 2>/dev/null || return 0
    name=$(mdns_conf_host_name "$(mdns_conf_path)")
    [[ -n $name ]] || name=$(hostname -s 2>/dev/null || true)
    name=$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')
    valid_mdns_hostname "$name" || return 0
    printf '%s.local\n' "$name"
}

show_first_run_access() {
    local service_log=
    local setup_token=
    local token_file=$STATE_DIR/setup-token
    local token_value=
    local listen_address=
    local port=8080
    local -a access_hosts=()
    local -a octets=()
    local candidate
    local duplicate
    local existing
    local first_octet
    local index
    local setup_fragment=
    local valid
    local value

    if command -v journalctl >/dev/null 2>&1; then
        service_log=$(journalctl --unit wingmankvm.service --boot --no-pager \
            --output cat --lines 100 2>/dev/null || true)
        while IFS= read -r value; do
            if [[ $value == *setup_token=* ]]; then
                candidate=${value#*setup_token=}
                setup_token=${candidate%% *}
            fi
            if [[ $value == *address=* ]]; then
                candidate=${value#*address=}
                listen_address=${candidate%% *}
            fi
        done <<<"$service_log"
    fi

    if [[ $listen_address =~ :([0-9]+)$ ]]; then
        port=${BASH_REMATCH[1]}
    fi

    # The service publishes the token in its 0600 state file. Keep the journal
    # fallback for upgrades from releases that logged the token instead.
    if [[ -f $token_file && -r $token_file && ! -L $token_file ]]; then
        read -r token_value <"$token_file" || true
        token_value=${token_value%$'\r'}
        if [[ $token_value =~ ^[A-Za-z0-9_-]{16,}$ ]]; then
            setup_token=$token_value
        fi
    fi

    # The route lookup yields the source address the host would normally use
    # first. The remaining sources make bridge, VLAN, and secondary addresses
    # visible without letting one of them accidentally become the primary URL.
    while IFS= read -r candidate; do
        [[ $candidate =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || continue
        IFS=. read -r -a octets <<<"$candidate"
        ((${#octets[@]} == 4)) || continue

        valid=true
        for value in "${octets[@]}"; do
            if [[ ! $value =~ ^[0-9]{1,3}$ ]] || ((10#$value > 255)); then
                valid=false
                break
            fi
        done
        $valid || continue

        first_octet=$((10#${octets[0]}))
        ((first_octet > 0 && first_octet < 224 && first_octet != 127)) || continue

        duplicate=false
        for existing in "${access_hosts[@]}"; do
            if [[ $existing == "$candidate" ]]; then
                duplicate=true
                break
            fi
        done
        $duplicate || access_hosts+=("$candidate")
    done < <(
        if command -v ip >/dev/null 2>&1; then
            ip -4 route get 1.1.1.1 2>/dev/null \
                | awk '{ for (i = 1; i <= NF; i++) if ($i == "src") print $(i + 1) }' \
                || true
        fi
        if command -v hostname >/dev/null 2>&1; then
            hostname -I 2>/dev/null \
                | awk '{ for (i = 1; i <= NF; i++) print $i }' \
                || true
        fi
        if command -v ip >/dev/null 2>&1; then
            ip -o -4 address show scope global 2>/dev/null \
                | awk '{ split($4, address, "/"); print address[1] }' \
                || true
        fi
    )

    if ((${#access_hosts[@]} == 0)); then
        access_hosts+=(127.0.0.1)
    fi
    # The mDNS name survives IP changes, so it goes first when available.
    candidate=$(mdns_access_host)
    if [[ -n $candidate ]]; then
        access_hosts=("$candidate" "${access_hosts[@]}")
    fi
    if [[ ! -s $STATE_DIR/auth.json && -n $setup_token ]]; then
        setup_fragment="#setup=$setup_token"
    fi
    for index in "${!access_hosts[@]}"; do
        if ((index == 0)); then
            log "open WingmanKVM: http://${access_hosts[$index]}:$port/$setup_fragment"
        else
            log "also detected: http://${access_hosts[$index]}:$port/$setup_fragment"
        fi
    done

    if [[ -s $STATE_DIR/auth.json ]]; then
        log "existing administrator configuration was preserved"
        return
    fi
    if [[ -n $setup_token ]]; then
        log "the first-run link contains the one-time setup token"
    else
        log "start WingmanKVM once, then print the link with: sudo wingmankvm-setup-url"
    fi
    log "reprint the one-time link at any time before setup with: sudo wingmankvm-setup-url"
}

# Source guard: the verification harness sources only the definitions above.
if [[ ${BASH_SOURCE[0]} != "$0" ]]; then
    return 0
fi

while (($# > 0)); do
    case $1 in
        --binary)
            (($# >= 2)) || die "--binary requires a path"
            BINARY=$2
            shift 2
            ;;
        --binary=*)
            BINARY=${1#*=}
            shift
            ;;
        --release)
            (($# >= 2)) || die "--release requires a path"
            RELEASE_ARCHIVE=$2
            shift 2
            ;;
        --release=*)
            RELEASE_ARCHIVE=${1#*=}
            shift
            ;;
        --health-url)
            (($# >= 2)) || die "--health-url requires a URL"
            HEALTH_URL=$2
            shift 2
            ;;
        --health-url=*)
            HEALTH_URL=${1#*=}
            shift
            ;;
        --maintenance-mode)
            (($# >= 2)) || die "--maintenance-mode requires a mode"
            MAINTENANCE_MODE=$2
            shift 2
            ;;
        --maintenance-mode=*)
            MAINTENANCE_MODE=${1#*=}
            shift
            ;;
        --no-start)
            NO_START=true
            shift
            ;;
        --install-deps)
            INSTALL_DEPS=true
            shift
            ;;
        --mdns)
            MDNS_ENABLED=true
            shift
            ;;
        --mdns-hostname)
            (($# >= 2)) || die "--mdns-hostname requires a name"
            MDNS_HOSTNAME=$2
            MDNS_HOSTNAME_EXPLICIT=true
            shift 2
            ;;
        --mdns-hostname=*)
            MDNS_HOSTNAME=${1#*=}
            MDNS_HOSTNAME_EXPLICIT=true
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1"
            ;;
    esac
done

[[ -n $BINARY || -n $RELEASE_ARCHIVE ]] || die "--binary or --release is required"
[[ -z $BINARY || -z $RELEASE_ARCHIVE ]] \
    || die "--binary and --release are mutually exclusive"
[[ $(id -u) -eq 0 ]] || die "run this installer as root (for example with sudo)"
DEST_ROOT=/
MDNS_HOSTNAME=${MDNS_HOSTNAME%.local}
MDNS_HOSTNAME=$(printf '%s' "$MDNS_HOSTNAME" | tr '[:upper:]' '[:lower:]')
if $MDNS_HOSTNAME_EXPLICIT && ! $MDNS_ENABLED; then
    die "--mdns-hostname requires --mdns"
fi
valid_mdns_hostname "$MDNS_HOSTNAME" \
    || die "invalid --mdns-hostname: use 1-63 letters, digits or hyphens, not starting or ending with a hyphen"

ensure_runtime_dependencies

if [[ -n $RELEASE_ARCHIVE ]]; then
    prepare_release_archive
fi

BINARY_DIR=$(CDPATH='' cd -- "$(dirname -- "$BINARY")" 2>/dev/null && pwd -P) \
    || die "binary directory does not exist: $(dirname -- "$BINARY")"
BINARY=$BINARY_DIR/$(basename -- "$BINARY")
[[ -f $BINARY && -r $BINARY && -s $BINARY ]] || die "binary is missing, unreadable, or empty: $BINARY"
ELF_HEADER=$(od -An -tx1 -N20 -v "$BINARY" | tr -d ' \n')
[[ ${ELF_HEADER:0:8} == 7f454c46 && ${ELF_HEADER:10:2} == 01 ]] \
    || die "binary is not a little-endian ELF executable: $BINARY"
ELF_CLASS=${ELF_HEADER:8:2}
ELF_MACHINE=${ELF_HEADER:36:4}
case $(uname -m) in
    aarch64 | arm64)
        [[ $ELF_CLASS == 02 && $ELF_MACHINE == b700 ]] \
            || die "binary architecture does not match this ARM64 system"
        ;;
    x86_64 | amd64)
        [[ $ELF_CLASS == 02 && $ELF_MACHINE == 3e00 ]] \
            || die "binary architecture does not match this x86_64 system"
        ;;
    armv6l | armv7l | armhf)
        [[ $ELF_CLASS == 01 && $ELF_MACHINE == 2800 ]] \
            || die "binary architecture does not match this 32-bit ARM system"
        ;;
    *)
        warn "cannot validate the binary architecture for host $(uname -m)"
        ;;
esac

load_deployment_sources
resolve_maintenance_mode
preflight_maintenance_mode
ENVIRONMENT_CHECK=$GADGET_ENV_EXAMPLE
if [[ -e $(dest_path /etc/wingmankvm/gadget.env) || -L $(dest_path /etc/wingmankvm/gadget.env) ]]; then
    ENVIRONMENT_CHECK=$(dest_path /etc/wingmankvm/gadget.env)
fi
run_doctor_preflight "$DOCTOR_SCRIPT" "$ENVIRONMENT_CHECK"
check_platform "$ENVIRONMENT_CHECK"
resolve_health_url

STATE_DIR=$(dest_path /var/lib/wingmankvm)
tx_init "$(dest_path /var/lib/wingmankvm-deploy)" "$DEST_ROOT"

create_users
ensure_install_directories
WINGMAN_GROUP=$(id -gn wingman)
install_missing_login_profile
if $NO_START; then
    TX_ROLLBACK_HOOK=wingmankvm_files_rollback
else
    runtime_prepare
    TX_PRE_ROLLBACK_HOOK=wingmankvm_runtime_pre_rollback
    TX_ROLLBACK_HOOK=wingmankvm_runtime_rollback
fi
install_managed_files
# The extracted binary has been copied into the transaction stage and
# published; the release archive is no longer needed.
wingmankvm_release_cleanup
postflight_maintenance_mode

reload_udev
systemctl daemon-reload || die "systemd daemon-reload failed after publishing managed files"

if $NO_START; then
    finish_no_start
    configure_mdns "$CONFIGURED_PORT"
else
    if ! start_services; then
        handle_start_failure
    fi
    tx_commit
    log "installation complete; WingmanKVM is running and passed /healthz"
    configure_mdns "$CONFIGURED_PORT"
    show_first_run_access
fi
