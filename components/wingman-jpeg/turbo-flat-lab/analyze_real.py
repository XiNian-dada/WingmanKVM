#!/usr/bin/env python3
"""Local-only quality analysis. Requires numpy, scipy, scikit-image and Pillow.

INPUT directory contains raw-results.json, captures/, comparisons/.
No upload, deletion, or changes to captured/encoded images.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import sys

os.environ.setdefault("OMP_NUM_THREADS", "1")
import numpy as np
import PIL
import scipy
import skimage
from PIL import Image
from scipy.ndimage import binary_dilation
from skimage.metrics import structural_similarity

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("directory", type=Path)
a = p.parse_args()
root = a.directory
raw = json.loads((root / "raw-results.json").read_text())
cache_path = root / "quality-cache.json"
cache = json.loads(cache_path.read_text()) if cache_path.exists() else {}
references = {}
rows = []


def finite(value):
    return float(value) if math.isfinite(float(value)) else None


def quality(source, output):
    source_sha = hashlib.sha256(source.read_bytes()).hexdigest()
    output_sha = hashlib.sha256(output.read_bytes()).hexdigest()
    key = source_sha + ":" + output_sha
    if key in cache:
        return {**cache[key], "output_sha256": output_sha}
    if source_sha not in references:
        rgb = np.asarray(Image.open(source).convert("RGB"))
        # Explicit TTY metric: any channel above 32, expanded by one pixel.
        mask = binary_dilation(rgb.max(axis=2) > 32, iterations=1)
        y = (rgb.astype(np.float32) * np.array([.2126, .7152, .0722], np.float32)).sum(axis=2)
        references[source_sha] = rgb, mask, y
    reference, mask, y = references[source_sha]
    output_rgb = np.asarray(Image.open(output).convert("RGB"))
    if output_rgb.shape != reference.shape:
        raise ValueError("decoded geometry changed: " + str(output))
    delta = reference.astype(np.float32) - output_rgb.astype(np.float32)
    full_mse = float(np.mean(delta * delta, dtype=np.float64))
    text_mse = float(np.mean(delta[mask] ** 2, dtype=np.float64))
    identical = not np.any(delta)
    if identical:
        ssim = text_ssim = 1.0
    else:
        out_y = (output_rgb.astype(np.float32) * np.array([.2126, .7152, .0722], np.float32)).sum(axis=2)
        ssim, field = structural_similarity(y, out_y, data_range=255, gaussian_weights=True,
                                           sigma=1.5, use_sample_covariance=False, full=True)
        text_ssim = float(np.mean(field[5:-5, 5:-5][mask[5:-5, 5:-5]], dtype=np.float64))
    result = {
        "python_rgb_psnr_db": 10 * math.log10(255 ** 2 / full_mse) if full_mse else None,
        "text_rgb_psnr_db": 10 * math.log10(255 ** 2 / text_mse) if text_mse else None,
        "luma_ssim": float(ssim), "text_luma_ssim": text_ssim,
        "decoded_pixels_identical": identical, "text_mask_fraction": float(np.mean(mask)),
    }
    cache[key] = result
    if len(cache) % 10 == 0:
        cache_path.write_text(json.dumps(cache, indent=2, allow_nan=False) + "\n")
        print("Analyzed", len(cache), "distinct encoded/reference pairs", flush=True)
    return {**result, "output_sha256": output_sha}


for name, log in sorted(raw["logs"].items()):
    match = re.search(r"(sparse|dense|color)", name)
    if not match:
        continue
    scene = match.group(1)
    capture = re.search(r"repeat-tty-\w+-(\d+)-", name)
    number = capture.group(1) if capture else "01"
    source = root / "captures" / f"tty-{scene}-{number}.jpg"
    for line in log.splitlines():
        if not (line.startswith("result mode=") or line.startswith("mode=")):
            continue
        row = dict(re.findall(r"(\w+)=([^ ]+)", line))
        output = root / "comparisons" / (name.removesuffix(".log") + "-" + row["mode"] + ".jpg")
        if not output.exists():
            raise FileNotFoundError(output)
        for key in ["q", "rounds", "bytes", "changed_quantizers", "clamped_quantizers"]:
            if key in row:
                row[key] = int(row[key])
        for key in ["median_ms", "p95_ms", "encode_only_ms", "psnr_db", "mbps_24"]:
            if key in row:
                row[key] = finite(row[key])
        row.update({"log": name, "scene": scene, "capture": source.name,
                    "output": output.name, "metrics": quality(source, output)})
        rows.append(row)

cache_path.write_text(json.dumps(cache, indent=2, allow_nan=False) + "\n")
(root / "quality-results.json").write_text(json.dumps({
    "analysis_versions": {"python": sys.version.split()[0], "numpy": np.__version__,
                          "Pillow": PIL.__version__, "scipy": scipy.__version__,
                          "scikit-image": skimage.__version__},
    "psnr_reference": "decoded original capture JPEG, not uncompressed HDMI ground truth",
    "text_mask": "max(R,G,B)>32 dilated one pixel; SSIM excludes 5-pixel image border",
    "ssim": "BT.709-weighted RGB luma, Gaussian 11x11 sigma=1.5, population covariance",
    "null_psnr": "identical decoded pixels (positive infinity)", "rows": rows,
}, indent=2, allow_nan=False) + "\n")
print("Wrote", len(rows), "quality/size/timing rows; distinct pairs", len(cache))
