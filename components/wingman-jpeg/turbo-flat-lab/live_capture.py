#!/usr/bin/env python3
"""Read only the named V4L2 mmap ranges of an explicitly selected process.

Never opens the capture device, changes queues, stops a process, or reads other
process-memory mappings. Requires root/ptrace permission. Captures stay private.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import time


def jpeg_end(data):
    """Locate the real EOI, skipping segment payloads, stuffing and restarts."""
    if data[:2] != b"\xff\xd8":
        raise ValueError("missing SOI")
    pos, entropy, seen_scan = 2, False, False
    metadata = {}
    while pos < len(data):
        if entropy:
            pos = data.find(b"\xff", pos)
            if pos < 0:
                break
        if data[pos] != 255:
            raise ValueError("invalid marker boundary")
        pos += 1
        while pos < len(data) and data[pos] == 255:
            pos += 1
        if pos >= len(data):
            break
        marker = data[pos]
        pos += 1
        if entropy and (marker == 0 or 0xD0 <= marker <= 0xD7):
            continue
        entropy = False
        if marker == 0xD9:
            if not metadata or not seen_scan:
                raise ValueError("EOI without frame header and scan")
            return pos, metadata
        if marker in (0, 0xD8) or 0xD0 <= marker <= 0xD7:
            raise ValueError("unexpected standalone marker")
        if pos + 2 > len(data):
            break
        size = int.from_bytes(data[pos:pos + 2], "big")
        if size < 2 or pos + size > len(data):
            raise ValueError("truncated segment")
        if marker in (0xC0, 0xC1, 0xC2):
            if size < 8:
                raise ValueError("short frame header")
            count = data[pos + 7]
            if size != 8 + count * 3:
                raise ValueError("invalid component count")
            metadata = {
                "sof": hex(marker), "precision": data[pos + 2],
                "height": int.from_bytes(data[pos + 3:pos + 5], "big"),
                "width": int.from_bytes(data[pos + 5:pos + 7], "big"),
                "components": [list(data[pos + 8 + i * 3:pos + 11 + i * 3])
                               for i in range(count)],
            }
        pos += size
        entropy = marker == 0xDA
        seen_scan |= entropy
    raise ValueError("missing EOI")


def capture(pid, device, output):
    if output.exists() or output.with_suffix(".json").exists():
        raise ValueError("output already exists")
    ranges = []
    for line in Path(f"/proc/{pid}/maps").read_text().splitlines():
        fields = line.split()
        if len(fields) == 6 and fields[-1] == device and fields[1].startswith("r"):
            start, end = (int(value, 16) for value in fields[0].split("-"))
            if not 0 < end - start <= 16 * 1024 * 1024:
                raise ValueError("unexpected capture mapping size")
            ranges.append((start, end - start))
    if not 1 <= len(ranges) <= 8:
        raise ValueError("expected one to eight exact device mappings")
    fd = os.open(f"/proc/{pid}/mem", os.O_RDONLY)
    try:
        for attempt in range(4):
            for start, size in ranges:
                first = os.pread(fd, size, start)
                try:
                    end, metadata = jpeg_end(first)
                except ValueError:
                    continue
                # Longer than the negotiated 30-FPS frame interval. A stable
                # payload avoids using a buffer while USB/DMA is changing it.
                time.sleep(0.04)
                second = os.pread(fd, end, start)
                if first[:end] != second:
                    continue
                metadata.update({
                    "captured_utc": datetime.now(timezone.utc).isoformat(),
                    "pid": pid, "device": device, "mapping": hex(start),
                    "bytes": end, "sha256": hashlib.sha256(second).hexdigest(),
                    "stable_read_gap_ms": 40, "attempt": attempt,
                    "method": "read-only exact V4L2 process mmap; no dequeue or stop",
                })
                output.parent.mkdir(parents=True, exist_ok=True)
                with output.open("xb") as file:
                    file.write(second)
                with output.with_suffix(".json").open("x") as file:
                    json.dump(metadata, file, indent=2)
                    file.write("\n")
                return metadata
    finally:
        os.close(fd)
    raise RuntimeError("no JPEG payload stable across 40 ms; retry later")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", required=True, type=int)
    parser.add_argument("--device", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.pid <= 1 or not args.device.startswith("/dev/video"):
        parser.error("explicit capture process and /dev/video device required")
    print(json.dumps(capture(args.pid, args.device, args.output), indent=2))
