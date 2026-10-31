#!/usr/bin/env bash
# WingmanKVM one-line installer.
#
#   curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh | sudo bash
#
# Downloads the latest GitHub release for this CPU, verifies SHA256SUMS and
# runs the release's deploy/install.sh --release. All temporary files are
# removed on success, failure and interruption.
set -Eeuo pipefail

PROGRAM=${0##*/}
REPO=${WINGMANKVM_REPO:-XiNian-dada/WingmanKVM}
API_URL=${WINGMANKVM_API_URL:-https://api.github.com/repos/$REPO/releases/latest}
DOWNLOAD_BASE=${WINGMANKVM_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download}
WORKDIR=
VERSION=
DOWNLOAD_ONLY=
INSTALL_ARGS=()

usage() {
    cat <<EOF
Usage: sudo bash $PROGRAM [options] [-- <deploy/install.sh options>]

Downloads the latest WingmanKVM release for this machine, verifies its
SHA256SUMS and runs the bundled deploy/install.sh --release.

Options:
  --version vX.Y.Z        Install a specific release instead of the latest
  --download-only DIR     Verify and save the archive and SHA256SUMS to DIR;
                          do not install. Use this to inspect a release first.
  --no-start              Passed to deploy/install.sh
  --install-deps          Passed to deploy/install.sh (Debian/Ubuntu/Armbian)
  --maintenance-mode MODE Passed to deploy/install.sh
  --health-url URL        Passed to deploy/install.sh
  --mdns                  Passed to deploy/install.sh (LAN name wingmankvm.local)
  --mdns-hostname NAME    Passed to deploy/install.sh
  -h, --help              Show this help

Supported CPUs: aarch64 (arm64) and armv7l (armv7/armhf).

Examples:
  curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh | sudo bash
  curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh | sudo bash -s -- --install-deps
  curl -fsSL https://raw.githubusercontent.com/XiNian-dada/WingmanKVM/main/scripts/get.sh -o get.sh
  less get.sh && sudo bash get.sh
EOF
}

log() {
    printf '%s: %s\n' "$PROGRAM" "$*"
}

die() {
    printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

cleanup() {
    local dir=${WORKDIR:-}
    [[ -n $dir ]] || return 0
    local tmp_root=${TMPDIR:-/tmp}
    tmp_root=${tmp_root%/}
    [[ -n $tmp_root ]] || tmp_root=/tmp
    case $dir in
        "$tmp_root"/wingmankvm-get.*) ;;
        *)
            printf '%s: warning: refusing to remove unexpected temporary path: %s\n' "$PROGRAM" "$dir" >&2
            WORKDIR=
            return 0
            ;;
    esac
    if [[ -d $dir && ! -L $dir ]]; then
        rm -rf -- "$dir" || printf '%s: warning: could not remove temporary directory: %s\n' "$PROGRAM" "$dir" >&2
    fi
    WORKDIR=
}

on_exit() {
    local status=$?
    cleanup
    exit "$status"
}

on_signal() {
    local name=$1 status=$2
    trap - HUP INT TERM
    printf '%s: interrupted by SIG%s\n' "$PROGRAM" "$name" >&2
    cleanup
    exit "$status"
}

detect_target() {
    local machine
    machine=$(uname -m)
    case $machine in
        aarch64 | arm64)
            TARGET=aarch64-unknown-linux-gnu
            ;;
        armv7l | armv7 | armhf)
            TARGET=armv7-unknown-linux-gnueabihf
            ;;
        *)
            die "unsupported architecture '$machine'; WingmanKVM releases support aarch64 and armv7l only"
            ;;
    esac
}

download_file() {
    local url=$1 destination=$2
    log "downloading $url"
    curl -fsSL --retry 3 --connect-timeout 20 --output "$destination" "$url" \
        || die "download failed: $url"
}

resolve_version() {
    local api_json
    if [[ -n $VERSION ]]; then
        case $VERSION in
            v*) ;;
            *) VERSION="v$VERSION" ;;
        esac
    else
        log "resolving the latest release from $API_URL"
        api_json=$(curl -fsSL --retry 3 --connect-timeout 20 \
            -H 'Accept: application/vnd.github+json' "$API_URL") \
            || die "cannot query the latest release; retry or pass --version vX.Y.Z"
        VERSION=$(printf '%s\n' "$api_json" \
            | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
            | head -n 1)
        [[ -n $VERSION ]] \
            || die "the release API response did not contain tag_name; pass --version vX.Y.Z"
    fi
    [[ $VERSION =~ ^v[0-9A-Za-z._-]+$ ]] \
        || die "invalid release version: $VERSION"
}

verify_archive() {
    local archive=$1 sums=$2 name expected actual
    name=$(basename -- "$archive")
    [[ -f $archive && -r $archive && -s $archive ]] \
        || die "downloaded archive is missing or empty: $archive"
    [[ -f $sums && -r $sums ]] || die "SHA256SUMS is missing or unreadable: $sums"
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

save_for_review() {
    local destination=$1 archive=$2 sums=$3 archive_name out_dir review_dir
    archive_name=$(basename -- "$archive")
    mkdir -p -- "$destination" || die "cannot create download directory: $destination"
    out_dir=$(CDPATH='' cd -- "$destination" 2>/dev/null && pwd -P) \
        || die "download directory is not accessible: $destination"
    cp -f -- "$archive" "$out_dir/$archive_name" || die "cannot save $archive_name"
    cp -f -- "$sums" "$out_dir/SHA256SUMS" || die "cannot save SHA256SUMS"
    chmod 0644 -- "$out_dir/$archive_name" "$out_dir/SHA256SUMS" || true
    review_dir=$(mktemp -d "$out_dir/wingmankvm-review.XXXXXXXX") \
        || die "cannot create a review directory under $out_dir"
    tar -xzf "$archive" -C "$review_dir" \
        --no-same-owner --no-same-permissions \
        || die "cannot extract $archive_name for review"
    log "saved verified release files to $out_dir"
    log "extracted for review to $review_dir"
    log "inspect $review_dir/deploy/install.sh, then run it with --release $out_dir/$archive_name"
    log "or rerun this script without --download-only"
}

main() {
    while (($# > 0)); do
        case $1 in
            --version)
                (($# >= 2)) || die "--version requires a value"
                VERSION=$2
                shift 2
                ;;
            --version=*)
                VERSION=${1#*=}
                shift
                ;;
            --download-only)
                (($# >= 2)) || die "--download-only requires a directory"
                DOWNLOAD_ONLY=$2
                shift 2
                ;;
            --download-only=*)
                DOWNLOAD_ONLY=${1#*=}
                shift
                ;;
            --no-start | --install-deps | --mdns)
                INSTALL_ARGS+=("$1")
                shift
                ;;
            --maintenance-mode | --health-url | --mdns-hostname)
                (($# >= 2)) || die "$1 requires a value"
                INSTALL_ARGS+=("$1" "$2")
                shift 2
                ;;
            --maintenance-mode=* | --health-url=* | --mdns-hostname=*)
                INSTALL_ARGS+=("$1")
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            *)
                die "unknown argument: $1 (use --help)"
                ;;
        esac
    done

    for command in curl tar sha256sum mktemp; do
        require_command "$command"
    done
    if [[ -z $DOWNLOAD_ONLY && $(id -u) -ne 0 ]]; then
        die "run this installer as root (for example: curl ... | sudo bash)"
    fi

    local tmp_root=${TMPDIR:-/tmp}
    tmp_root=${tmp_root%/}
    [[ -n $tmp_root ]] || tmp_root=/tmp
    WORKDIR=$(mktemp -d "$tmp_root/wingmankvm-get.XXXXXXXX") \
        || die "cannot create a temporary directory"
    trap on_exit EXIT
    trap 'on_signal HUP 129' HUP
    trap 'on_signal INT 130' INT
    trap 'on_signal TERM 143' TERM

    detect_target
    resolve_version

    local archive_name archive sums base installer extract_dir
    archive_name="wingmankvm-${VERSION}-${TARGET}.tar.gz"
    archive=$WORKDIR/$archive_name
    sums=$WORKDIR/SHA256SUMS
    base=${DOWNLOAD_BASE%/}/$VERSION

    download_file "$base/$archive_name" "$archive"
    download_file "$base/SHA256SUMS" "$sums"
    verify_archive "$archive" "$sums"

    if [[ -n $DOWNLOAD_ONLY ]]; then
        save_for_review "$DOWNLOAD_ONLY" "$archive" "$sums"
        return 0
    fi

    extract_dir=$WORKDIR/release
    mkdir -p -- "$extract_dir" || die "cannot create extraction directory"
    tar -xzf "$archive" -C "$extract_dir" \
        --no-same-owner --no-same-permissions \
        || die "cannot extract $archive_name"
    installer=$extract_dir/deploy/install.sh
    [[ -f $installer && -r $installer ]] \
        || die "release archive does not contain deploy/install.sh"

    log "installing WingmanKVM $VERSION for $TARGET"
    if ((${#INSTALL_ARGS[@]} > 0)); then
        bash "$installer" --release "$archive" "${INSTALL_ARGS[@]}"
    else
        bash "$installer" --release "$archive"
    fi
}

main "$@"
