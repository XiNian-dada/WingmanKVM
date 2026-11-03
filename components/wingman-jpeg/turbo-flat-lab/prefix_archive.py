#!/usr/bin/env python3
"""Prefix archive-defined globals, including assembly constants, for dual linking."""
import argparse
from pathlib import Path
import re
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("archive", type=Path)
p.add_argument("output", type=Path)
p.add_argument("prefix")
p.add_argument("--tools", default="aarch64-linux-musl-")
p.add_argument("--nm-output", type=Path)
p.add_argument("--mapping-only", action="store_true")
a = p.parse_args()
if a.output.exists():
    p.error("output must not already exist")
if not re.fullmatch(r"[a-z]+_", a.prefix):
    p.error("expected lowercase prefix with trailing underscore")
symbols = a.nm_output.read_text() if a.nm_output else subprocess.check_output(
    [a.tools + "nm", "-g", "--defined-only", str(a.archive)], text=True)
names = set()
for line in symbols.splitlines():
    parts = line.split()
    if len(parts) == 3 and re.fullmatch(r"[A-Za-z_][A-Za-z_0-9.]*", parts[-1]):
        names.add(parts[-1])
mapping = a.output.with_suffix(".symbols")
mapping.write_text("".join(f"{name} {a.prefix}{name}\n" for name in sorted(names)))
if not a.mapping_only:
    subprocess.run([a.tools + "objcopy", "--redefine-syms=" + str(mapping), str(a.archive), str(a.output)], check=True)
print(f"{len(names)} globals prefixed: {a.output}")
