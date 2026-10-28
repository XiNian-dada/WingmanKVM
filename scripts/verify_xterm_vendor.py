#!/usr/bin/env python3
"""Verify vendored xterm assets against the authoritative SOURCES.json."""
from __future__ import annotations

import argparse, base64, binascii, hashlib, io, json, os, re, stat, sys, tarfile
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit

SCHEMA = 1
MANIFEST = "web/vendor/xterm/SOURCES.json"
PREFIX = ("web", "vendor", "xterm")
REPOSITORY = "https://github.com/xtermjs/xterm.js"
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
HEX40 = re.compile(r"[0-9a-f]{40}\Z")
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?\Z")
PACKAGES = {
    "@xterm/xterm": {
        "web/vendor/xterm/xterm.js": "package/lib/xterm.js",
        "web/vendor/xterm/xterm.css": "package/css/xterm.css",
        "web/vendor/xterm/LICENSE": "package/LICENSE",
    },
    "@xterm/addon-fit": {
        "web/vendor/xterm/addon-fit.js": "package/lib/addon-fit.js",
        "web/vendor/xterm/LICENSE.addon-fit": "package/LICENSE",
    },
}


def fail(message):
    raise ValueError(message)


def text(obj, key, label):
    value = obj.get(key)
    if not isinstance(value, str) or not value:
        fail(f"{label}: missing or empty {key}")
    return value


def sha(value, label):
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"{label}: invalid SHA256")
    return value


def sri(value, label):
    if not isinstance(value, str) or not value.startswith("sha512-"):
        fail(f"{label}: invalid SHA512 SRI")
    try:
        raw = base64.b64decode(value[7:], validate=True)
    except (binascii.Error, ValueError):
        fail(f"{label}: invalid SHA512 SRI")
    if len(raw) != 64:
        fail(f"{label}: invalid SHA512 SRI")
    return raw


def vendor_path(value, label):
    if not isinstance(value, str) or value.startswith("/") or "\\" in value or "\x00" in value:
        fail(f"{label}: unsafe path {value!r}")
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        fail(f"{label}: unsafe path {value!r}")
    if tuple(parts[:3]) != PREFIX or len(parts) <= 3:
        fail(f"{label}: path outside web/vendor/xterm: {value!r}")
    if PurePosixPath(*parts).as_posix() != value:
        fail(f"{label}: non-normalized path {value!r}")
    return parts


def validate_manifest(doc):
    if not isinstance(doc, dict) or set(doc) != {"schema_version", "packages"}:
        fail("manifest: expected schema_version and packages")
    if type(doc.get("schema_version")) is not int or doc["schema_version"] != SCHEMA:
        fail(f"manifest: schema_version must be {SCHEMA}")
    packages = doc.get("packages")
    if not isinstance(packages, list) or len(packages) != len(PACKAGES):
        fail(f"manifest: expected {len(PACKAGES)} packages")
    by_name = {}
    for index, package in enumerate(packages):
        if not isinstance(package, dict):
            fail(f"packages[{index}]: must be an object")
        name = text(package, "name", f"packages[{index}]")
        if name in by_name:
            fail(f"manifest: duplicate package {name}")
        by_name[name] = package
    if set(by_name) != set(PACKAGES):
        fail("manifest: package coverage mismatch")

    required = ("name", "version", "repository", "source_commit", "license",
                "license_file", "archive_url", "archive_integrity", "archive_sha256")
    seen = set()
    for name, expected_files in PACKAGES.items():
        package = by_name[name]
        for key in required:
            text(package, key, name)
        if not VERSION.fullmatch(package["version"]):
            fail(f"{name}: invalid version syntax")
        if package["repository"] != REPOSITORY:
            fail(f"{name}: repository must be {REPOSITORY}")
        if not HEX40.fullmatch(package["source_commit"]):
            fail(f"{name}: invalid source_commit")
        if package["license"] != "MIT":
            fail(f"{name}: license must be MIT")
        leaf = name.split("/", 1)[1]
        expected_url = f"/{name}/-/{leaf}-{package['version']}.tgz"
        url = urlsplit(package["archive_url"])
        if (url.scheme, url.netloc, url.path, url.query, url.fragment) != (
                "https", "registry.npmjs.org", expected_url, "", ""):
            fail(f"{name}: archive_url must be https://registry.npmjs.org{expected_url}")
        sha(package["archive_sha256"], name)
        sri(package["archive_integrity"], name)

        files = package.get("files")
        if not isinstance(files, list):
            fail(f"{name}: files must be a list")
        mapped = {}
        for index, entry in enumerate(files):
            label = f"{name}.files[{index}]"
            if not isinstance(entry, dict) or set(entry) != {"path", "source", "sha256"}:
                fail(f"{label}: expected path/source/sha256")
            rel, source = text(entry, "path", label), text(entry, "source", label)
            vendor_path(rel, label)
            sha(entry.get("sha256"), label)
            if rel in seen:
                fail(f"{label}: duplicate local path {rel}")
            seen.add(rel)
            mapped[rel] = source
        if mapped != expected_files:
            fail(f"{name}: file coverage or member mapping mismatch")
        license_file = package["license_file"]
        if license_file not in mapped or mapped[license_file] != "package/LICENSE":
            fail(f"{name}: license_file must map package/LICENSE")
    return by_name


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
    parts = vendor_path(rel, label)
    current = repo
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


def verify_offline(repo):
    doc = json.loads(local_bytes(repo, MANIFEST, "manifest").decode("utf-8"))
    by_name = validate_manifest(doc)
    local = {}
    for name, package in by_name.items():
        for entry in package["files"]:
            rel = entry["path"]
            data = local_bytes(repo, rel, f"{name}:{rel}")
            actual = hashlib.sha256(data).hexdigest()
            if actual != entry["sha256"]:
                fail(f"{rel}: SHA256 mismatch; expected {entry['sha256']}, got {actual}")
            local[rel] = data
        print(f"PASS {name}: {len(package['files'])} file(s)")
    print("PASS offline: manifest metadata and 5/5 SHA256 checks")
    return doc, local


def verify_archives(doc, local, directory):
    directory = Path(os.path.abspath(os.fspath(directory)))
    if directory.is_symlink() or not directory.is_dir():
        fail(f"archives-dir: unsafe directory: {directory}")
    for package in doc["packages"]:
        basename = package["archive_url"].rsplit("/", 1)[-1]
        path = directory / basename
        mode = os.lstat(path).st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
            fail(f"{basename}: unsafe archive file")
        data = read_regular(path, basename)
        if hashlib.sha256(data).hexdigest() != package["archive_sha256"]:
            fail(f"{basename}: archive SHA256 mismatch")
        if hashlib.sha512(data).digest() != sri(package["archive_integrity"], basename):
            fail(f"{basename}: archive SHA512 mismatch")
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for entry in package["files"]:
                try:
                    member = archive.getmember(entry["source"])
                except KeyError:
                    fail(f"{basename}: missing member {entry['source']}")
                if not member.isreg():
                    fail(f"{basename}: member is not REGULAR: {entry['source']}")
                handle = archive.extractfile(member)
                if handle is None:
                    fail(f"{basename}: unreadable member {entry['source']}")
                with handle:
                    payload = handle.read()
                if hashlib.sha256(payload).hexdigest() != entry["sha256"]:
                    fail(f"{basename}: member SHA256 mismatch: {entry['source']}")
                if local[entry["path"]] != payload:
                    fail(f"{basename}: member differs from {entry['path']}")
        print(f"PASS archive {basename}: SHA256, SHA512, {len(package['files'])} member(s)")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo")
    parser.add_argument("--archives-dir")
    args = parser.parse_args(argv)
    try:
        default = Path(__file__).resolve().parent.parent
        repo = Path(os.path.abspath(os.fspath(default if args.repo is None else args.repo)))
        mode = os.lstat(repo).st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            fail(f"repo: not a directory: {repo}")
        doc, local = verify_offline(repo)
        if args.archives_dir is not None:
            verify_archives(doc, local, args.archives_dir)
            print("PASS archives: 2/2 tarballs verified without extraction")
        return 0
    except (ValueError, OSError, tarfile.TarError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
