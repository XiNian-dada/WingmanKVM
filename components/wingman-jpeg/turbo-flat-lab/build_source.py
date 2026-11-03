#!/usr/bin/env python3
"""Offline native/cross build with cmake and a preinstalled toolchain; no install."""
import argparse
from pathlib import Path
import subprocess
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("source", type=Path, help="extracted official libjpeg-turbo 2.1.5")
p.add_argument("output", type=Path, help="new task-owned build directory")
p.add_argument("--tool-prefix", default="", help="e.g. aarch64-linux-musl-")
a = p.parse_args()
source, output = a.source.resolve(), a.output.resolve()
lab = Path(__file__).resolve().parent
if source in output.parents:
    p.error("output must be outside the source tree")
if output.exists():
    p.error("output must not already exist")
output.mkdir(parents=True)

def run(cmd):
    subprocess.run([str(v) for v in cmd], check=True)

with (output / "flat.patch").open("w") as patch:
    subprocess.run([sys.executable, str(lab / "patch_source.py"), str(source),
                    str(output / "patched-source")], stdout=patch, check=True)
for name, tree in [("stock", source), ("patched", output / "patched-source")]:
    build = output / name
    config = ["cmake", "-S", tree, "-B", build, "-DCMAKE_BUILD_TYPE=Release",
              "-DENABLE_SHARED=OFF", "-DENABLE_STATIC=ON", "-DWITH_TURBOJPEG=OFF",
              "-DWITH_SIMD=ON", "-DREQUIRE_SIMD=ON"]
    if a.tool_prefix:
        config += ["-DCMAKE_SYSTEM_NAME=Linux", "-DCMAKE_SYSTEM_PROCESSOR=aarch64",
                   "-DCMAKE_C_COMPILER=" + a.tool_prefix + "gcc",
                   "-DCMAKE_ASM_COMPILER=" + a.tool_prefix + "gcc"]
    run(config)
    run(["cmake", "--build", build, "--target", "jpeg-static", "-j2"])
    prefix = "stock_" if name == "stock" else "flat_"
    run([sys.executable, lab / "prefix_archive.py", build / "libjpeg.a",
         output / (prefix[:-1] + "-prefixed.a"), prefix, "--tools=" + a.tool_prefix])
for file, name, flags in [("dual_compare.c", "source-compare", []),
                          ("test.c", "source-test", ["-DWK_SOURCE"]),
                          ("real_compare.c", "real-compare", [])]:
    run([a.tool_prefix + "gcc", "-O3", "-static", "-Wall", "-Wextra", "-Werror",
         *flags, "-I" + str(output / "stock"), "-I" + str(source), "-I" + str(lab),
         lab / file, output / "stock-prefixed.a", output / "flat-prefixed.a",
         "-lm", "-o", output / name])
print("Built isolated static benchmark/test binaries in", output)
