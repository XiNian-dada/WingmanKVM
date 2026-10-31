#!/usr/bin/env python3
"""Check that a release tag matches the version in Cargo.toml.

The tag may be passed as the first positional argument. When omitted, the
script falls back to GITHUB_REF_NAME so the release workflow can call it
without repeating the ref. A leading ``v`` is accepted; everything else must
match the Cargo version exactly.

Examples:
    python3 scripts/check_release_version.py v0.1.0
    GITHUB_REF_NAME=v0.1.0 python3 scripts/check_release_version.py
    python3 scripts/check_release_version.py --self-test
"""

from __future__ import annotations

import os
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CARGO_TOML = REPO_ROOT / "Cargo.toml"

_PACKAGE_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"\s*(?:#.*)?$')


def read_cargo_version(path: Path) -> str:
    """Return the [package] version, ignoring dependency versions."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ValueError(f"无法读取 {path}: {exc}") from exc

    in_package = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_package = stripped == "[package]"
            continue
        if not in_package:
            continue
        match = _PACKAGE_VERSION.match(stripped)
        if match:
            version = match.group(1).strip()
            if version:
                return version
            break
    raise ValueError(f"{path} 的 [package] 段缺少 version")


def normalize_tag(tag: str) -> str:
    """Strip surrounding whitespace and one optional leading ``v``."""
    value = tag.strip()
    if value[:1] in {"v", "V"}:
        value = value[1:]
    return value


def check(tag: str, version: str) -> int:
    return 0 if normalize_tag(tag) == version else 1


def self_test() -> int:
    """Run small in-process checks; a mismatched version must be non-zero."""
    failures: list[str] = []
    cases = [
        ("v0.1.0", "0.1.0", 0),
        ("0.1.0", "0.1.0", 0),
        (" v0.1.0 ", "0.1.0", 0),
        ("v0.1.0-rc.1", "0.1.0-rc.1", 0),
        ("v0.2.0", "0.1.0", 1),
        ("v0.1.1", "0.1.0", 1),
        ("v0.1.0", "0.1.1", 1),
    ]
    for tag, version, expected in cases:
        actual = check(tag, version)
        if actual != expected:
            failures.append(
                f"check({tag!r}, {version!r}) = {actual}, 期望 {expected}"
            )

    with tempfile.TemporaryDirectory(prefix="wingmankvm-release-selftest.") as tmp:
        manifest = Path(tmp) / "Cargo.toml"
        manifest.write_text(
            '[package]\nname = "wingmankvm"\nversion = "9.8.7"\n\n'
            '[dependencies]\nserde = "1"\n',
            encoding="utf-8",
        )
        try:
            parsed = read_cargo_version(manifest)
        except ValueError as exc:
            failures.append(f"无法解析测试 Cargo.toml: {exc}")
        else:
            if parsed != "9.8.7":
                failures.append(f"解析到 {parsed!r}，期望 '9.8.7'")

    if failures:
        for failure in failures:
            print(f"self-test: {failure}", file=sys.stderr)
        return 1
    print("self-test: ok")
    return 0


def usage() -> str:
    return (
        "用法: check_release_version.py [TAG] [--cargo-toml PATH] "
        "[--print-version] [--self-test]\n"
        "TAG 省略时读取 GITHUB_REF_NAME；TAG 可带一个前导 v。"
    )


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()

    cargo_toml = DEFAULT_CARGO_TOML
    print_version = False
    positional: list[str] = []
    index = 0
    while index < len(argv):
        arg = argv[index]
        if arg in {"-h", "--help"}:
            print(usage())
            return 0
        if arg == "--print-version":
            print_version = True
            index += 1
            continue
        if arg == "--cargo-toml":
            if index + 1 >= len(argv):
                print("error: --cargo-toml 需要路径", file=sys.stderr)
                return 2
            cargo_toml = Path(argv[index + 1])
            index += 2
            continue
        if arg.startswith("--cargo-toml="):
            cargo_toml = Path(arg.split("=", 1)[1])
            index += 1
            continue
        if arg.startswith("-"):
            print(f"error: 未知选项: {arg}", file=sys.stderr)
            print(usage(), file=sys.stderr)
            return 2
        positional.append(arg)
        index += 1

    if len(positional) > 1:
        print("error: 只能提供一个 tag", file=sys.stderr)
        return 2

    try:
        version = read_cargo_version(cargo_toml)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    if print_version:
        print(version)
        return 0

    tag = positional[0] if positional else os.environ.get("GITHUB_REF_NAME", "").strip()
    tag = tag.strip()
    if not tag:
        print(
            "error: 未提供 tag，且 GITHUB_REF_NAME 为空；"
            "无法校验发布版本",
            file=sys.stderr,
        )
        return 2

    if check(tag, version) != 0:
        print(
            f"error: tag {tag!r} 与 Cargo.toml version {version!r} 不一致"
            f"（规范化后为 {normalize_tag(tag)!r}）",
            file=sys.stderr,
        )
        return 1

    print(f"ok: tag {tag!r} 与 Cargo.toml version {version!r} 一致")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
