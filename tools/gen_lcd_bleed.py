#!/usr/bin/env python3
"""
Generate an LCD domain bleed texture from a surface normal map.

On a real DMG under magnification, liquid crystal domains within each pixel
cell don't uniformly rotate polarisation. Irregular patches let the reflector
(shade 0) bleed through the tint. This script converts a normal map into a
greyscale mask that encodes those domain shapes:

  1. Compute slope-deviation magnitude from the R,G normal channels.
  2. Threshold to binary blobs (LC domain boundaries).
  3. Gaussian blur to soften edges into organic shapes.
  4. Normalise to full 0-255 range.

Output: a single-channel (greyscale, L mode) PNG suitable for tiling within
each game-pixel cell in the Screen shader.

Usage:
    python3 tools/gen_lcd_bleed.py [--size 512] [--threshold 0.45] [--blur 28]
"""

import argparse
import math
from pathlib import Path
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SRC_NORMAL = ROOT / "gameboy_ref" / "GAMEBOY_TEXTURES" / "Plastic004_4K_NormalGL.png"
DST_TEXTURE = ROOT / "ui" / "src" / "components" / "lcd_bleed.png"


def generate(size: int = 512, threshold: float = 0.45, blur_radius: int = 28):
    print(f"Loading normal map: {SRC_NORMAL}")
    img = Image.open(SRC_NORMAL).convert("RGB")

    # Downsample to target size first — the averaging acts as a pre-blur
    # and makes subsequent processing much faster.
    print(f"Downsampling {img.size[0]}x{img.size[1]} → {size}x{size}")
    img = img.resize((size, size), Image.LANCZOS)

    pixels = img.load()
    out = Image.new("L", (size, size))
    out_px = out.load()

    # Step 1: Compute slope-deviation magnitude from R,G channels.
    # Normal map encodes tangent-space slope in R (X) and G (Y).
    # Flat surface = (128, 128); deviation = distance from that center.
    print("Computing slope magnitude...")
    mag_min = float("inf")
    mag_max = float("-inf")
    mags = []

    for y in range(size):
        row = []
        for x in range(size):
            r, g, _ = pixels[x, y]
            dx = r / 255.0 - 0.5
            dy = g / 255.0 - 0.5
            mag = math.sqrt(dx * dx + dy * dy)
            row.append(mag)
            mag_min = min(mag_min, mag)
            mag_max = max(mag_max, mag)
        mags.append(row)

    # Normalise magnitudes to 0-1
    rng = mag_max - mag_min if mag_max > mag_min else 1.0
    for y in range(size):
        for x in range(size):
            mags[y][x] = (mags[y][x] - mag_min) / rng

    # Step 2: Threshold to binary blobs.
    # Values above threshold → 1.0 (domain boundary / bleed-through region),
    # below → 0.0 (solid LC fill).
    print(f"Thresholding at {threshold}...")
    for y in range(size):
        for x in range(size):
            mags[y][x] = 1.0 if mags[y][x] > threshold else 0.0

    # Write binary into image for blur step
    for y in range(size):
        for x in range(size):
            out_px[x, y] = int(mags[y][x] * 255)

    # Step 3: Gaussian blur to soften into organic domain shapes.
    # The radius controls feature size — larger = bigger blobs.
    # At 512px output, radius ~28 gives features ~1/5 to 1/4 of the texture,
    # which maps to ~1/5 to 1/4 of a pixel cell when tiled per-cell.
    print(f"Blurring with radius {blur_radius}...")
    out = out.filter(ImageFilter.GaussianBlur(radius=blur_radius))

    # Step 4: Normalise to full 0-255 range for maximum dynamic range.
    out_px = out.load()
    lo = 255
    hi = 0
    for y in range(size):
        for x in range(size):
            v = out_px[x, y]
            lo = min(lo, v)
            hi = max(hi, v)

    if hi > lo:
        print(f"Normalising range [{lo}, {hi}] → [0, 255]")
        scale = 255.0 / (hi - lo)
        for y in range(size):
            for x in range(size):
                out_px[x, y] = int((out_px[x, y] - lo) * scale)

    out.save(DST_TEXTURE, optimize=True)
    file_kb = DST_TEXTURE.stat().st_size / 1024
    print(f"Saved: {DST_TEXTURE}  ({size}x{size}, L mode, {file_kb:.0f} KB)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=512, help="Output texture size (default: 512)")
    parser.add_argument("--threshold", type=float, default=0.45, help="Magnitude threshold for blob detection (default: 0.45)")
    parser.add_argument("--blur", type=int, default=28, help="Gaussian blur radius for softening (default: 28)")
    args = parser.parse_args()
    generate(size=args.size, threshold=args.threshold, blur_radius=args.blur)
