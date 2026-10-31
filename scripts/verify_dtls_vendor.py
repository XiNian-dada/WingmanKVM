#!/usr/bin/env python3
"""Verify the vendored dtls crate against vendor/dtls/upstream-manifest.json."""
from __future__ import annotations

import argparse, hashlib, json, os, re, stat, sys
from pathlib import Path, PurePosixPath

VENDOR = ("vendor", "dtls")
MANIFEST = "upstream-manifest.json"
PATCHES_DOC = "PATCHES.md"
EXTRA_FILES = {MANIFEST, PATCHES_DOC}
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
# Files intentionally modified from the upstream crate. Each entry is the
# SHA-256 of the local patched file and must be documented in PATCHES.md.
PATCHED = {
    "Cargo.toml":
        "bcd16f5f51c78f54c4642106219e93c1f46dc53237f189bf7b14ce06c84b0bbd",
    "src/curve/named_curve.rs":
        "609f25a6b5c2744a2f66394ce3da1dcf93c1d439efd88e7bea2d77d6178a0ecb",
    "src/prf/mod.rs":
        "1447abc5fe5066e725fc1d6a8cc2da90f061a2c829b36694362ce56cc3bc7955",
    "src/crypto/crypto_cbc.rs":
        "278ed87076d7f4393d35ea698373e10705fe599dd185a3c15be5b1788b4ddab5",
    "src/prf/prf_test.rs":
        "2786f0b0f5a12c3c484e4a9d163baa79ab48f855cd1bd3253c712292bef1467e",
}


def fail(message):
    raise ValueError(message)


def sha(value, label):
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"{label}: invalid SHA256")
    return value


def _relative_parts(value, label):
    if not isinstance(value, str) or not value or value.startswith("/") \
            or "\\" in value or "\x00" in value:
        fail(f"{label}: unsafe path {value!r}")
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        fail(f"{label}: unsafe path {value!r}")
    if PurePosixPath(*parts).as_posix() != value:
        fail(f"{label}: non-normalized path {value!r}")
    return parts


def vendor_path(value, label):
    parts = _relative_parts(value, label)
    if parts[0] == MANIFEST or parts[0] == PATCHES_DOC:
        fail(f"{label}: metadata path treated as a source file: {value!r}")
    return parts


def validate_manifest(doc):
    if not isinstance(doc, dict):
        fail("manifest: expected an object mapping relative paths to SHA256")
    if not doc:
        fail("manifest: no files recorded")
    mapped = {}
    for rel, digest in doc.items():
        vendor_path(rel, "manifest")
        sha(digest, rel)
        mapped[rel] = digest
    missing = sorted(set(PATCHED) - set(mapped))
    if missing:
        fail(f"manifest: missing patched file(s): {', '.join(missing)}")
    for rel, digest in PATCHED.items():
        if mapped[rel] == digest:
            fail(f"{rel}: patched hash is identical to upstream; update PATCHED")
    return mapped


def read_regular(path, label):
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            fail(f"{label}: not a regular file: {path}")
        with os.fdopen(fd, "rb") as handle:
            fd = -1
            return handle.read()
    finally:
        if fd != -1:
            os.close(fd)


def local_bytes(repo, rel, label):
    parts = _relative_parts(rel, label)
    current = repo
    for part in VENDOR:
        current = current / part
        mode = os.lstat(current).st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            fail(f"{label}: unsafe directory: {current}")
    for part in parts[:-1]:
        current = current / part
        mode = os.lstat(current).st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            fail(f"{label}: unsafe directory: {current}")
    path = current / parts[-1]
    mode = os.lstat(path).st_mode
    if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
        fail(f"{label}: unsafe file: {path}")
    return read_regular(path, label)


def walk_vendor(repo):
    root = repo
    for part in VENDOR:
        root = root / part
    stack = [(root, PurePosixPath())]
    while stack:
        directory, relbase = stack.pop()
        try:
            entries = sorted(os.scandir(directory), key=lambda entry: entry.name)
        except OSError as exc:
            fail(f"cannot list {directory}: {exc}")
        for entry in entries:
            rel = (relbase / entry.name).as_posix()
            mode = os.lstat(entry.path).st_mode
            if stat.S_ISLNK(mode):
                fail(f"unsafe symlink in vendored tree: {rel}")
            if stat.S_ISDIR(mode):
                stack.append((Path(entry.path), PurePosixPath(rel)))
            elif stat.S_ISREG(mode):
                yield rel, Path(entry.path)
            else:
                fail(f"unsupported file type in vendored tree: {rel}")


def verify_offline(repo):
    doc = json.loads(local_bytes(repo, MANIFEST, "manifest").decode("utf-8"))
    upstream = validate_manifest(doc)
    actual = {}
    for rel, path in walk_vendor(repo):
        if rel in EXTRA_FILES:
            continue
        if rel not in upstream:
            fail(f"{rel}: file is not recorded in {MANIFEST}")
        actual[rel] = hashlib.sha256(read_regular(path, rel)).hexdigest()
    missing = sorted(set(upstream) - set(actual))
    if missing:
        fail(f"manifest entries without files: {', '.join(missing)}")

    patches = local_bytes(repo, PATCHES_DOC, "PATCHES.md").decode("utf-8")
    upstream_count = patched_count = 0
    for rel, expected in upstream.items():
        digest = actual[rel]
        if digest == expected:
            upstream_count += 1
            continue
        if rel not in PATCHED:
            fail(f"{rel}: SHA256 mismatch; expected upstream {expected}, got {digest}")
        if digest != PATCHED[rel]:
            fail(f"{rel}: patched SHA256 mismatch; expected {PATCHED[rel]}, got {digest}")
        if rel not in patches:
            fail(f"{rel}: patched file is not documented in {PATCHES_DOC}")
        patched_count += 1
        print(f"PASS patched {rel}")
    if patched_count != len(PATCHED):
        fail(f"patched manifest coverage mismatch: {patched_count}/{len(PATCHED)}")
    print(
        f"PASS offline: manifest metadata and {upstream_count}/{len(upstream)} "
        f"upstream + {patched_count}/{len(PATCHED)} patched SHA256 checks"
    )
    return upstream, actual


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo")
    args = parser.parse_args(argv)
    try:
        default = Path(__file__).resolve().parent.parent
        repo = Path(os.path.abspath(os.fspath(default if args.repo is None else args.repo)))
        mode = os.lstat(repo).st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            fail(f"repo: not a directory: {repo}")
        verify_offline(repo)
        return 0
    except (ValueError, OSError, json.JSONDecodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
