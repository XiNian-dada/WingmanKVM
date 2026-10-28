#!/usr/bin/env bash
# deploy/upgrade-transaction.sh - sourced by deploy/install.sh.
# The installer supplies the fixed destination list. This library never
# publishes a destination read from a manifest.

TX_STORE=''
TX_TXN=''
TX_SNAPSHOT=''
TX_STAGE=''
TX_MANIFEST=''
TX_STATE=idle
TX_DEST_ROOT=''
TX_ENTRY_COUNT=0
TX_EUID=0
TX_ROLLBACK_HOOK=''
TX_PRE_ROLLBACK_HOOK=''
TX_POLICY=()
TX_OLD=()
TX_MODE=()
TX_UID=()
TX_GID=()
TX_DEST=()
TX_NEW_MODE=()
TX_NEW_OWNER=()
TX_NEW_GROUP=()

tx_log() { printf 'upgrade-transaction: %s\n' "$*"; }
tx_error() { printf 'upgrade-transaction: error: %s\n' "$*" >&2; }
tx_fail() { tx_error "$*"; exit 1; }
_tx_stamp() { date -u +%Y%m%dT%H%M%SZ; }
_tx_root() { [[ $TX_EUID -eq 0 ]]; }

_tx_trusted_dir() {
    local path=$1 meta owner _group mode
    [[ -d $path && ! -L $path ]] || return 1
    meta=$(stat -c '%u %g %a' -- "$path" 2>/dev/null) || return 1
    read -r owner _group mode <<<"$meta"
    [[ $owner -eq $TX_EUID ]] || return 1
    (( (8#$mode & 8#022) == 0 ))
}

_tx_private_dir() {
    local mode
    _tx_trusted_dir "$1" || return 1
    mode=$(stat -c '%a' -- "$1") || return 1
    [[ $mode == 700 ]]
}

_tx_private_file() {
    local path=$1 meta owner _group mode
    [[ -f $path && ! -L $path ]] || return 1
    meta=$(stat -c '%u %g %a' -- "$path" 2>/dev/null) || return 1
    read -r owner _group mode <<<"$meta"
    [[ $owner -eq $TX_EUID ]] || return 1
    (( (8#$mode & 8#077) == 0 ))
}

_tx_exact_destination() {
    local dest=$1 rel
    [[ $dest != *$'\t'* && $dest != *$'\n'* ]] || return 1
    if [[ -n $TX_DEST_ROOT ]]; then
        [[ $dest == "$TX_DEST_ROOT"/* ]] || return 1
        rel=${dest#"$TX_DEST_ROOT"}
    else
        [[ $dest == /* ]] || return 1
        rel=$dest
    fi
    case $rel in
        /usr/local/bin/wingmankvm | \
            /usr/local/sbin/wingmankvm-gadget | \
            /usr/local/sbin/wingmankvm-set-wingman-password | \
            /etc/sudoers.d/wingmankvm | \
            /etc/sudoers.d/wingmankvm-maintenance-root | \
            /etc/udev/rules.d/99-wingmankvm.rules | \
            /etc/systemd/system/wingmankvm-gadget.service | \
            /etc/systemd/system/wingmankvm.service | \
            /etc/wingmankvm/gadget.env)
            return 0
            ;;
    esac
    return 1
}

_tx_pending_recovery() {
    local line rec=
    [[ -f $TX_STORE/PENDING && ! -L $TX_STORE/PENDING ]] || return 1
    while IFS= read -r line; do
        case $line in recover=/*) rec=${line#recover=} ;; esac
    done <"$TX_STORE/PENDING"
    [[ -n $rec ]] || return 1
    printf '%s\n' "$rec"
}

_tx_clear_pending() {
    local pending=$TX_STORE/PENDING
    [[ -e $pending || -L $pending ]] || return 0
    [[ -f $pending && ! -L $pending ]] || {
        tx_error "pending marker is not a regular file: $pending"
        return 1
    }
    grep -Fxq "transaction=$TX_TXN" -- "$pending" || {
        tx_error "pending marker does not belong to $TX_TXN"
        return 1
    }
    rm -f -- "$pending"
}

_tx_mutating() {
    [[ -n $TX_TXN && -f $TX_STORE/PENDING && ! -L $TX_STORE/PENDING ]]
}

_tx_print_recovery() {
    local rec
    if rec=$(_tx_pending_recovery); then
        tx_error "manual recovery command: sudo $rec"
    else
        tx_error "manual recovery: inspect $TX_STORE and $TX_TXN"
    fi
    tx_error "see docs/UPGRADE_RECOVERY.md; keep the snapshot"
}

_tx_on_signal() {
    local name=$1 signo=$2
    trap - INT TERM HUP
    set +e
    tx_error "received SIG$name during installation"
    _tx_rollback "interrupted by SIG$name"
    exit $((128 + signo))
}

_tx_on_exit() {
    local status=${1:-$?}
    trap - EXIT INT TERM HUP
    case $TX_STATE in
        committed | rolled_back) exit "$status" ;;
        rollback_failed) exit 1 ;;
    esac
    if _tx_mutating; then
        if _tx_rollback "installer exited with status $status"; then
            [[ $status -ne 0 ]] && exit "$status"
            exit 1
        fi
        exit 1
    fi
    if [[ -n $TX_TXN ]]; then
        tx_error "uncommitted transaction left incomplete: $TX_TXN"
        exit 1
    fi
    exit "$status"
}

tx_init() {
    local store=$1 root=${2:-/} lock state_dir recovery
    [[ $TX_STATE == idle ]] || tx_fail "transaction library already initialized"
    [[ $store == /* && $store != *'/../'* ]] || tx_fail "store must be absolute and normalized"
    store=${store%/}
    TX_EUID=$(id -u)
    root=${root%/}
    [[ $root == / ]] && root=''
    TX_DEST_ROOT=$root
    TX_STORE=$store
    state_dir=$TX_DEST_ROOT/var/lib/wingmankvm
    [[ $store != "$state_dir" && $store != "$state_dir"/* ]] \
        || tx_fail "transaction store must be outside service state"
    if [[ -e $store || -L $store ]]; then
        _tx_private_dir "$store" || tx_fail "store must be a real owned 0700 directory: $store"
    else
        _tx_trusted_dir "$(dirname -- "$store")" \
            || tx_fail "store parent is not trusted: $(dirname -- "$store")"
        install -d -m 0700 -- "$store" || tx_fail "cannot create store: $store"
        _tx_private_dir "$store" || tx_fail "store has unsafe owner or mode: $store"
    fi
    lock=$store/install.lock
    if [[ -e $lock || -L $lock ]]; then
        _tx_private_file "$lock" || tx_fail "unsafe transaction lock: $lock"
    else
        (umask 077 && : >"$lock") || tx_fail "cannot create transaction lock"
        _tx_private_file "$lock" || tx_fail "unsafe transaction lock: $lock"
    fi
    exec 9>>"$lock" || tx_fail "cannot open transaction lock"
    flock -n 9 || tx_fail "another WingmanKVM installer is already running"
    if [[ -e $store/PENDING || -L $store/PENDING ]]; then
        tx_error "a previous upgrade did not complete; no files were changed"
        recovery=$(_tx_pending_recovery || true)
        if [[ -n $recovery ]]; then
            tx_error "review $store/PENDING, then run: sudo $recovery"
        else
            tx_error "review $store/PENDING and the txn.* directories"
        fi
        exit 1
    fi
}

tx_begin() {
    [[ -n $TX_STORE ]] || tx_fail "tx_init must run first"
    [[ -z $TX_TXN ]] || tx_fail "transaction already begun"
    TX_TXN=$TX_STORE/txn.$(_tx_stamp).$$
    mkdir -m 0700 -- "$TX_TXN" || tx_fail "cannot create transaction directory"
    TX_SNAPSHOT=$TX_TXN/snapshot
    TX_STAGE=$TX_TXN/stage
    install -d -m 0700 -- "$TX_SNAPSHOT" "$TX_STAGE" || tx_fail "cannot create transaction subdirectories"
    TX_MANIFEST=$TX_TXN/manifest
    (umask 077 && : >"$TX_MANIFEST") || tx_fail "cannot create manifest"
    _tx_write_recovery_script || tx_fail "cannot write recovery script"
    TX_STATE=staging
    trap '_tx_on_exit $?' EXIT
    trap '_tx_on_signal INT 2' INT
    trap '_tx_on_signal TERM 15' TERM
    trap '_tx_on_signal HUP 1' HUP
}

_tx_common() {
    local policy=$1 source=$2 dest=$3 mode=$4 owner=$5 group=$6
    local idx presence old=absent old_mode=- old_uid=- old_gid=- meta
    [[ $TX_STATE == staging ]] || tx_fail "can only add files while staging"
    [[ -f $source && -r $source ]] || tx_fail "source missing or unreadable: $source"
    _tx_exact_destination "$dest" || tx_fail "destination is not in the fixed managed list: $dest"
    _tx_trusted_dir "$(dirname -- "$dest")" \
        || tx_fail "destination directory is not trusted: $(dirname -- "$dest")"
    [[ ! -d $dest ]] || tx_fail "destination is a directory: $dest"
    for ((idx = 0; idx < TX_ENTRY_COUNT; idx++)); do
        [[ ${TX_DEST[idx]} != "$dest" ]] || tx_fail "duplicate destination: $dest"
    done
    if [[ -L $dest ]]; then
        [[ $policy == preserved ]] || tx_fail "refusing to replace managed symlink: $dest"
        presence=present
    elif [[ -e $dest ]]; then
        [[ -f $dest ]] || tx_fail "destination is not a regular file: $dest"
        presence=present
    else
        presence=absent
    fi
    idx=$TX_ENTRY_COUNT
    if [[ $presence == present && ( $policy == managed || $policy == absent ) ]]; then
        meta=$(stat -c '%a %u %g' -- "$dest") || tx_fail "cannot stat $dest"
        read -r old_mode old_uid old_gid <<<"$meta"
        cp -- "$dest" "$TX_SNAPSHOT/$idx" || tx_fail "cannot snapshot $dest"
        chmod 0600 -- "$TX_SNAPSHOT/$idx"
        old=file
    elif [[ $presence == present ]]; then
        old=present
    fi
    if [[ $policy == managed || $policy == absent || $old == absent ]]; then
        cp -- "$source" "$TX_STAGE/$idx" || tx_fail "cannot stage $dest"
        chmod "$mode" -- "$TX_STAGE/$idx" || tx_fail "cannot set staged mode for $dest"
        if _tx_root; then
            chown "$owner:$group" -- "$TX_STAGE/$idx" || tx_fail "cannot set staged owner for $dest"
        fi
    fi
    TX_POLICY[idx]=$policy
    TX_OLD[idx]=$old
    TX_MODE[idx]=$old_mode
    TX_UID[idx]=$old_uid
    TX_GID[idx]=$old_gid
    TX_DEST[idx]=$dest
    TX_NEW_MODE[idx]=$mode
    TX_NEW_OWNER[idx]=$owner
    TX_NEW_GROUP[idx]=$group
    TX_ENTRY_COUNT=$((idx + 1))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$idx" "$policy" "$old" "$old_mode" "$old_uid" "$old_gid" "$dest" >>"$TX_MANIFEST" \
        || tx_fail "cannot write manifest"
}

tx_add_managed() { _tx_common managed "$@"; }
tx_add_preserved() { _tx_common preserved "$@"; }

tx_add_absent() {
    [[ $2 == "$TX_DEST_ROOT/etc/sudoers.d/wingmankvm-maintenance-root" ]] \
        || tx_fail "only the optional managed root grant can be removed"
    _tx_common absent "$1" "$2" 0440 root root
}


tx_stage_all() {
    local i
    [[ $TX_STATE == staging ]] || tx_fail "cannot stage in state $TX_STATE"
    ((TX_ENTRY_COUNT > 0)) || tx_fail "no managed destinations declared"
    for ((i = 0; i < TX_ENTRY_COUNT; i++)); do
        if [[ ${TX_POLICY[i]} == managed || ${TX_POLICY[i]} == absent || ${TX_OLD[i]} == absent ]]; then
            [[ -f $TX_STAGE/$i && ! -L $TX_STAGE/$i ]] || tx_fail "staged file missing for ${TX_DEST[i]}"
        fi
    done
    chmod 0600 -- "$TX_MANIFEST"
    TX_STATE=staged
}

_tx_publish_one() {
    local i=$1 parent tmp
    local dest=${TX_DEST[i]}
    if [[ ${TX_POLICY[i]} == absent ]]; then
        [[ -e $dest || -L $dest ]] || return 0
        [[ -f $dest && ! -L $dest ]] || return 1
        cmp -s "$TX_SNAPSHOT/$i" "$dest" || return 1
        rm -- "$dest"
        return
    fi
    parent=$(dirname -- "$dest")
    [[ ! -d $dest ]] || { tx_error "refusing to publish over directory: $dest"; return 1; }
    _tx_trusted_dir "$parent" || { tx_error "untrusted destination directory: $parent"; return 1; }
    tmp=$(mktemp "$parent/.wingmankvm-publish.XXXXXX") || return 1
    if ! cp -- "$TX_STAGE/$i" "$tmp"; then rm -f -- "$tmp"; return 1; fi
    if ! chmod "${TX_NEW_MODE[i]}" -- "$tmp"; then rm -f -- "$tmp"; return 1; fi
    if _tx_root && ! chown "${TX_NEW_OWNER[i]}:${TX_NEW_GROUP[i]}" -- "$tmp"; then
        rm -f -- "$tmp"
        return 1
    fi
    mv -f -- "$tmp" "$dest" || { rm -f -- "$tmp"; return 1; }
}

tx_apply() {
    local i tmp
    [[ $TX_STATE == staged ]] || tx_fail "cannot apply in state $TX_STATE"
    tmp=$TX_STORE/.PENDING.$$
    {
        printf 'transaction=%s\n' "$TX_TXN"
        printf 'recover=%s\n' "$TX_TXN/recover.sh"
        printf 'started=%s\n' "$(_tx_stamp)"
    } >"$tmp" || tx_fail "cannot prepare pending marker"
    chmod 0600 -- "$tmp" || tx_fail "cannot secure pending marker"
    mv -f -- "$tmp" "$TX_STORE/PENDING" || tx_fail "cannot publish pending marker"
    TX_STATE=applying
    for ((i = 0; i < TX_ENTRY_COUNT; i++)); do
        [[ ${TX_POLICY[i]} == preserved && ${TX_OLD[i]} == present ]] && continue
        _tx_publish_one "$i" || tx_fail "failed to publish ${TX_DEST[i]}"
    done
    TX_STATE=applied
}

_tx_restore_one() {
    local i=$1 parent tmp
    local dest=${TX_DEST[i]}
    parent=$(dirname -- "$dest")
    [[ -f $TX_SNAPSHOT/$i && ! -L $TX_SNAPSHOT/$i ]] \
        || { tx_error "snapshot missing for $dest"; return 1; }
    [[ ! -d $dest ]] || { tx_error "refusing to restore over directory: $dest"; return 1; }
    _tx_trusted_dir "$parent" || { tx_error "untrusted restore directory: $parent"; return 1; }
    tmp=$(mktemp "$parent/.wingmankvm-restore.XXXXXX") || return 1
    if ! cp -- "$TX_SNAPSHOT/$i" "$tmp"; then rm -f -- "$tmp"; return 1; fi
    if ! chmod "${TX_MODE[i]}" -- "$tmp"; then rm -f -- "$tmp"; return 1; fi
    if _tx_root && ! chown "${TX_UID[i]}:${TX_GID[i]}" -- "$tmp"; then
        rm -f -- "$tmp"
        return 1
    fi
    mv -f -- "$tmp" "$dest" || { rm -f -- "$tmp"; return 1; }
}

_tx_remove_one() {
    local i=$1
    local dest=${TX_DEST[i]} stage=$TX_STAGE/$i
    [[ -e $dest || -L $dest ]] || return 0
    [[ ! -d $dest ]] || { tx_error "refusing to remove directory: $dest"; return 1; }
    [[ -f $dest && ! -L $dest ]] || { tx_error "refusing to remove non-regular file: $dest"; return 1; }
    [[ -f $stage && ! -L $stage ]] || { tx_error "staged copy missing: $dest"; return 1; }
    cmp -s -- "$stage" "$dest" || { tx_error "file changed after install; not removing: $dest"; return 1; }
    rm -f -- "$dest"
}

_tx_rollback() {
    local reason=$1 failures=0 i
    [[ -n $TX_TXN && -d $TX_TXN ]] || return 0
    set +e
    TX_STATE=rolling_back
    printf '%s\n' "$reason" >"$TX_TXN/rollback-reason" 2>/dev/null
    tx_error "rollback started: $reason"
    # Runtime state may need the newly published unit files. Restore it before
    # removing files that were absent on first installation.
    if [[ -n $TX_PRE_ROLLBACK_HOOK ]]; then
        if declare -F "$TX_PRE_ROLLBACK_HOOK" >/dev/null 2>&1; then
            "$TX_PRE_ROLLBACK_HOOK" || failures=$((failures + 1))
        else
            tx_error "pre-rollback hook is not a function: $TX_PRE_ROLLBACK_HOOK"
            failures=$((failures + 1))
        fi
    fi
    for ((i = TX_ENTRY_COUNT - 1; i >= 0; i--)); do
        if [[ ${TX_POLICY[i]} == preserved && ${TX_OLD[i]} == present ]]; then
            continue
        fi
        if [[ ${TX_POLICY[i]} == absent && ${TX_OLD[i]} == absent ]]; then continue; fi
        case ${TX_OLD[i]} in
            file) _tx_restore_one "$i" || failures=$((failures + 1)) ;;
            absent) _tx_remove_one "$i" || failures=$((failures + 1)) ;;
        esac
    done
    if ((failures == 0)) && [[ -n $TX_ROLLBACK_HOOK ]]; then
        if declare -F "$TX_ROLLBACK_HOOK" >/dev/null 2>&1; then
            "$TX_ROLLBACK_HOOK" || failures=$((failures + 1))
        else
            tx_error "rollback hook is not a function: $TX_ROLLBACK_HOOK"
            failures=$((failures + 1))
        fi
    fi
    if ((failures > 0)); then
        TX_STATE=rollback_failed
        tx_error "rollback incomplete; snapshot retained: $TX_TXN"
        _tx_print_recovery
        return 1
    fi
    if ! _tx_clear_pending; then
        TX_STATE=rollback_failed
        tx_error "files restored but the pending marker could not be cleared"
        _tx_print_recovery
        return 1
    fi
    TX_STATE=rolled_back
    printf 'rolled_back=%s\nreason=%s\n' "$(_tx_stamp)" "$reason" >"$TX_TXN/ROLLED_BACK" || {
        TX_STATE=rollback_failed
        tx_error "cannot write rollback marker"
        return 1
    }
    chmod 0600 -- "$TX_TXN/ROLLED_BACK" 2>/dev/null
    tx_log "rollback complete; previous managed files restored"
}

tx_rollback() { _tx_rollback "${1:-requested by installer}"; }

tx_commit() {
    local i
    [[ $TX_STATE == applied ]] || tx_fail "cannot commit in state $TX_STATE"
    {
        printf 'committed=%s\n' "$(_tx_stamp)"
        printf 'snapshot=%s\n' "$TX_SNAPSHOT"
        for ((i = 0; i < TX_ENTRY_COUNT; i++)); do
            printf 'managed=%s\n' "${TX_DEST[i]}"
        done
    } >"$TX_TXN/COMMITTED" || tx_fail "cannot write commit marker"
    chmod 0600 -- "$TX_TXN/COMMITTED"
    _tx_clear_pending || tx_fail "cannot clear pending marker after commit"
    TX_ROLLBACK_HOOK=''
    TX_PRE_ROLLBACK_HOOK=''
    TX_STATE=committed
    tx_log "transaction committed; snapshot retained at $TX_TXN"
}

# Appended to deploy/upgrade-transaction.sh (same file, final section).

_tx_write_recovery_script() {
    local script=$TX_TXN/recover.sh
    cat >"$script" <<'RECOVERY_SCRIPT'
#!/usr/bin/env bash
# Self-contained recovery for one WingmanKVM deployment transaction.
# Does not need the WingmanKVM source tree. Restores only the fixed managed
# file list; user data and existing gadget.env/profile are never touched.
set -u
TX_DIR=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P) || exit 1
STORE=$(dirname -- "$TX_DIR")
PREFIX=${STORE%/var/lib/wingmankvm-deploy}
EUID_NOW=$(id -u)
MANIFEST=$TX_DIR/manifest
SNAPSHOT=$TX_DIR/snapshot
STAGE=$TX_DIR/stage
PENDING=$STORE/PENDING
LOCK=$STORE/install.lock
FAILURES=0
say() { printf 'wingmankvm-recover: %s\n' "$*"; }
err() { printf 'wingmankvm-recover: error: %s\n' "$*" >&2; }
trusted_dir() {
    local path=$1 meta owner _group mode
    [ -d "$path" ] && [ ! -L "$path" ] || return 1
    meta=$(stat -c '%u %g %a' -- "$path" 2>/dev/null) || return 1
    read -r owner _group mode <<<"$meta"
    [ "$owner" -eq "$EUID_NOW" ] || return 1
    [ $((8#$mode & 8#022)) -eq 0 ] || return 1
}
private_dir() { trusted_dir "$1" && [ "$(stat -c '%a' -- "$1")" = 700 ]; }
private_meta_file() {
    local path=$1 meta owner _group mode
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    meta=$(stat -c '%u %g %a' -- "$path" 2>/dev/null) || return 1
    read -r owner _group mode <<<"$meta"
    [ "$owner" -eq "$EUID_NOW" ] || return 1
    [ $((8#$mode & 8#022)) -eq 0 ] || return 1
}
safe_dest() {
    local dest=$1 rel
    if [ -n "$PREFIX" ]; then
        case $dest in "$PREFIX"/*) ;; *) return 1 ;; esac
        rel=${dest#"$PREFIX"}
    else
        rel=$dest
    fi
    case $rel in
        /usr/local/bin/wingmankvm | /usr/local/sbin/wingmankvm-gadget | \
            /usr/local/sbin/wingmankvm-set-wingman-password | \
            /etc/sudoers.d/wingmankvm | /etc/sudoers.d/wingmankvm-maintenance-root | /etc/udev/rules.d/99-wingmankvm.rules | \
            /etc/systemd/system/wingmankvm-gadget.service | \
            /etc/systemd/system/wingmankvm.service | /etc/wingmankvm/gadget.env)
            return 0 ;;
    esac
    return 1
}
restore_one() {
    local dest=$1 snap=$2 mode=$3 uid=$4 gid=$5 parent tmp
    parent=$(dirname -- "$dest")
    [ -f "$snap" ] && [ ! -L "$snap" ] || { err "snapshot missing: $dest"; return 1; }
    private_meta_file "$snap" || { err "untrusted snapshot metadata: $snap"; return 1; }
    [ ! -d "$dest" ] || { err "refusing to restore over directory: $dest"; return 1; }
    trusted_dir "$parent" || { err "untrusted destination directory: $parent"; return 1; }
    tmp=$(mktemp "$parent/.wingmankvm-recover.XXXXXX") || return 1
    cp -- "$snap" "$tmp" || { rm -f -- "$tmp"; return 1; }
    chmod "$mode" -- "$tmp" || { rm -f -- "$tmp"; return 1; }
    if [ "$EUID_NOW" -eq 0 ]; then
        chown "$uid:$gid" -- "$tmp" || { rm -f -- "$tmp"; return 1; }
    fi
    mv -f -- "$tmp" "$dest" || { rm -f -- "$tmp"; return 1; }
    say "restored $dest"
}
remove_created() {
    local dest=$1 staged=$2
    [ -e "$dest" ] || [ -L "$dest" ] || return 0
    [ ! -d "$dest" ] || { err "refusing to remove directory: $dest"; return 1; }
    [ -f "$dest" ] && [ ! -L "$dest" ] || { err "not a regular file: $dest"; return 1; }
    [ -f "$staged" ] && [ ! -L "$staged" ] || { err "staged copy missing: $dest"; return 1; }
    private_meta_file "$staged" || { err "untrusted staged metadata: $staged"; return 1; }
    cmp -s -- "$staged" "$dest" || { err "changed since install; not removing: $dest"; return 1; }
    rm -f -- "$dest"
    say "removed newly created managed file $dest"
}
case $STORE in */var/lib/wingmankvm-deploy) ;; *) err "unexpected store path: $STORE"; exit 1 ;; esac
case ${TX_DIR##*/} in txn.*) ;; *) err "unexpected transaction directory name"; exit 1 ;; esac
private_dir "$TX_DIR" || { err "transaction directory ownership/mode is unsafe"; exit 1; }
private_dir "$STORE" || { err "store ownership/mode is unsafe"; exit 1; }
private_dir "$SNAPSHOT" || { err "snapshot directory ownership/mode is unsafe"; exit 1; }
private_dir "$STAGE" || { err "stage directory ownership/mode is unsafe"; exit 1; }
private_meta_file "$MANIFEST" || { err "manifest ownership/mode is unsafe"; exit 1; }
private_meta_file "$LOCK" || { err "lock ownership/mode is unsafe"; exit 1; }
exec 9>>"$LOCK" || { err "cannot open installation lock"; exit 1; }
flock -n 9 || { err "another installer is running"; exit 1; }
if [ -e "$PENDING" ]; then
    private_meta_file "$PENDING" || { err "pending marker is unsafe"; exit 1; }
    grep -Fxq "transaction=$TX_DIR" -- "$PENDING" || { err "pending marker mismatch"; exit 1; }
fi
mapfile -t LINES <"$MANIFEST"
for ((i = ${#LINES[@]} - 1; i >= 0; i--)); do
    IFS=$'\t' read -r idx policy old mode uid gid dest <<<"${LINES[i]}"
    [ -n "${idx:-}" ] || continue
    safe_dest "$dest" || { err "unsafe manifest destination: $dest"; FAILURES=$((FAILURES + 1)); continue; }
    if [ "$policy" = preserved ] && [ "$old" = present ]; then continue; fi
    if [[ $policy == absent && $old == absent ]]; then continue; fi
    case $old in
        file) restore_one "$dest" "$SNAPSHOT/$idx" "$mode" "$uid" "$gid" || FAILURES=$((FAILURES + 1)) ;;
        absent) remove_created "$dest" "$STAGE/$idx" || FAILURES=$((FAILURES + 1)) ;;
        *) err "unexpected snapshot state for $dest"; FAILURES=$((FAILURES + 1)) ;;
    esac
done
if [ "$FAILURES" -gt 0 ]; then
    err "$FAILURES restore operation(s) failed; snapshot retained: $TX_DIR"
    exit 1
fi
if [ -e "$PENDING" ]; then
    rm -f -- "$PENDING" || { err "cannot clear pending marker"; exit 1; }
fi
printf 'rolled_back=%s\nreason=manual recovery\n' "$(date -u +%Y%m%dT%H%M%SZ)" \
    >"$TX_DIR/ROLLED_BACK" || { err "cannot write rollback marker"; exit 1; }
chmod 0600 -- "$TX_DIR/ROLLED_BACK"
say "transaction rolled back successfully; review and restart services manually"
RECOVERY_SCRIPT
    chmod 0700 -- "$script" || return 1
    if _tx_root; then chown 0:0 -- "$script" || return 1; fi
}
