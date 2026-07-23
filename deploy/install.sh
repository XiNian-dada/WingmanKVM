#!/usr/bin/env bash
set -Eeuo pipefail

PROGRAM=${0##*/}
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}

BINARY=
NO_START=false
WINGMAN_CREATED=false

usage() {
    cat <<EOF
Usage: sudo ./deploy/$PROGRAM --binary /path/to/wingmankvm [--no-start]

Options:
  --binary PATH  Linux WingmanKVM executable to install (required)
  --no-start     Install files only; do not enable or start services
  -h, --help     Show this help
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

source_file() {
    local path=$SCRIPT_DIR/$1
    [[ -f $path && -r $path ]] || die "deployment file is missing or unreadable: $path"
    printf '%s\n' "$path"
}

install_managed_file() {
    local source=$1
    local destination=$2
    local mode=$3
    local owner=${4:-root}
    local group=${5:-root}
    local temporary

    [[ ! -e $destination || -f $destination || -L $destination ]] \
        || die "installation target is not a regular file: $destination"
    [[ ! -L $destination ]] || die "refusing to replace managed symlink: $destination"
    temporary=$(mktemp "${destination}.tmp.XXXXXX")
    install -o "$owner" -g "$group" -m "$mode" "$source" "$temporary"

    if [[ -f $destination ]] && cmp -s "$temporary" "$destination"; then
        rm -f -- "$temporary"
        chown "$owner:$group" "$destination"
        chmod "$mode" "$destination"
        log "already current: $destination"
    else
        mv -f -- "$temporary" "$destination"
        log "installed: $destination"
    fi
}

install_if_missing() {
    local source=$1
    local destination=$2
    local mode=$3
    local owner=${4:-root}
    local group=${5:-root}

    [[ ! -e $destination || -f $destination || -L $destination ]] \
        || die "preserved target is not a regular file: $destination"
    if [[ -e $destination || -L $destination ]]; then
        log "preserved existing file: $destination"
        return
    fi
    install_managed_file "$source" "$destination" "$mode" "$owner" "$group"
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

start_services() {
    systemctl enable wingmankvm-gadget.service wingmankvm.service

    if systemctl is-active --quiet wingmankvm-gadget.service; then
        log "Gadget service is already active; leaving the current USB connection intact"
    elif ! systemctl start wingmankvm-gadget.service; then
        systemctl --no-pager --full status wingmankvm-gadget.service || true
        die "failed to start USB Gadget; check /etc/wingmankvm/gadget.env and the service log"
    fi

    if ! systemctl restart wingmankvm.service; then
        systemctl --no-pager --full status wingmankvm.service || true
        die "failed to start WingmanKVM"
    fi
    systemctl is-active --quiet wingmankvm.service || die "WingmanKVM did not remain active"
}

show_first_run_access() {
    local service_log=
    local setup_token=
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
    if [[ ! -s /var/lib/wingmankvm/auth.json && -n $setup_token ]]; then
        setup_fragment="#setup=$setup_token"
    fi
    for index in "${!access_hosts[@]}"; do
        if ((index == 0)); then
            log "open WingmanKVM: http://${access_hosts[$index]}:$port/$setup_fragment"
        else
            log "also detected: http://${access_hosts[$index]}:$port/$setup_fragment"
        fi
    done

    if [[ -s /var/lib/wingmankvm/auth.json ]]; then
        log "existing administrator configuration was preserved"
        return
    fi
    if [[ -n $setup_token ]]; then
        log "the first-run link contains the one-time setup token"
    else
        log "get the first-run token with: journalctl -u wingmankvm.service -b | grep setup_token"
    fi
}

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
        --no-start)
            NO_START=true
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

[[ -n $BINARY ]] || die "--binary is required"
[[ $(id -u) -eq 0 ]] || die "run this installer as root (for example with sudo)"

require_command awk
require_command cmp
require_command getent
require_command groupadd
require_command install
require_command mktemp
require_command od
require_command systemctl
require_command udevadm
require_command useradd
require_command usermod
require_command visudo

[[ -x /bin/bash ]] || die "/bin/bash is required for the web terminal"
[[ -x /usr/bin/sudo ]] || die "sudo is required; install it before running this installer"
[[ -x /usr/sbin/chpasswd ]] || die "/usr/sbin/chpasswd is required for terminal password setup"

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

UDEV_RULES=$(source_file 99-wingmankvm.rules)
WINGMAN_PROFILE=$(source_file wingman.bash_profile)
PASSWORD_HELPER=$(source_file wingmankvm-set-wingman-password)
SUDOERS_RULE=$(source_file wingmankvm-sudoers)
GADGET_SCRIPT=$(source_file wingmankvm-gadget)
GADGET_ENV_EXAMPLE=$(source_file gadget.env.example)
GADGET_SERVICE=$(source_file wingmankvm-gadget.service)
APPLICATION_SERVICE=$(source_file wingmankvm.service)

sh -n "$GADGET_SCRIPT" || die "Gadget helper failed syntax validation"

ENVIRONMENT_CHECK=$GADGET_ENV_EXAMPLE
if [[ -e /etc/wingmankvm/gadget.env || -L /etc/wingmankvm/gadget.env ]]; then
    ENVIRONMENT_CHECK=/etc/wingmankvm/gadget.env
fi
check_platform "$ENVIRONMENT_CHECK"

create_users
visudo -cf "$SUDOERS_RULE" >/dev/null || die "sudoers rule failed validation"

install -d -o root -g root -m 0755 /etc/wingmankvm
install -d -o root -g root -m 0750 /etc/sudoers.d
install -d -o root -g root -m 0755 /etc/udev/rules.d
install -d -o root -g root -m 0755 /etc/systemd/system
install -d -o root -g root -m 0755 /usr/local/bin /usr/local/sbin
install_if_missing "$GADGET_ENV_EXAMPLE" /etc/wingmankvm/gadget.env 0644
install_managed_file "$BINARY" /usr/local/bin/wingmankvm 0755
install_managed_file "$GADGET_SCRIPT" /usr/local/sbin/wingmankvm-gadget 0755
install_managed_file "$PASSWORD_HELPER" /usr/local/sbin/wingmankvm-set-wingman-password 0755
install_managed_file "$SUDOERS_RULE" /etc/sudoers.d/wingmankvm 0440
visudo -cf /etc/sudoers.d/wingmankvm >/dev/null || die "installed sudoers rule failed validation"

WINGMAN_GROUP=$(id -gn wingman)
if $WINGMAN_CREATED; then
    install_managed_file "$WINGMAN_PROFILE" /home/wingman/.bash_profile 0644 wingman "$WINGMAN_GROUP"
else
    install_if_missing "$WINGMAN_PROFILE" /home/wingman/.bash_profile 0644 wingman "$WINGMAN_GROUP"
fi
install_managed_file "$UDEV_RULES" /etc/udev/rules.d/99-wingmankvm.rules 0644
install_managed_file "$GADGET_SERVICE" /etc/systemd/system/wingmankvm-gadget.service 0644
install_managed_file "$APPLICATION_SERVICE" /etc/systemd/system/wingmankvm.service 0644

reload_udev
systemctl daemon-reload

if $NO_START; then
    log "installation complete; the installer did not enable or start services"
    log "any services that were already running were left unchanged"
    log "review /etc/wingmankvm/gadget.env, then run: systemctl enable --now wingmankvm-gadget.service wingmankvm.service"
else
    start_services
    log "installation complete; WingmanKVM is running"
    show_first_run_access
fi
