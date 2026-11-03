#!/usr/bin/env python3
"""Verify the pinned baseline and restrict changes to the reviewed fork points."""
from pathlib import Path
import argparse
import hashlib
import json

parser = argparse.ArgumentParser()
parser.add_argument("baseline", type=Path, help="unmodified jpeg-encoder-0.6.1 source directory")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / "UPSTREAM_SHA256.json").read_text())
changed = []
for name, expected in manifest.items():
    assert not Path(name).is_absolute() and ".." not in Path(name).parts
    assert hashlib.sha256((args.baseline / name).read_bytes()).hexdigest() == expected, name
    if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
        changed.append(name)
assert set(changed) == {"src/encoder.rs", "src/quantization.rs", "src/lib.rs"}, changed
print(json.dumps({"baseline": "jpeg-encoder 0.6.1", "modified_upstream_files": changed,
                  "unchanged_upstream_files": len(manifest) - len(changed)}, indent=2))
