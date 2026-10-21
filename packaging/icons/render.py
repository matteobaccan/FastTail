#!/usr/bin/env python3
# FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
# Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
# SPDX-License-Identifier: MIT
"""Renders the application icon, assets/icon.svg, into the files the packages and the
window use, so the release jobs need no image tool. Run from the repository root after
changing the SVG, and commit the output:

    pip install cairosvg pillow
    python3 packaging/icons/render.py

Writes packaging/icons/fasttail-<N>.png (16 to 512), fasttail.ico (16 to 256),
FastTail.icns (16 to 512), and the window icon assets/icon.png plus its raw 128 x 128
RGBA copy assets/icon-128.rgba (embedded with include_bytes!).
"""

import io
import pathlib

import cairosvg
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parents[2]
SVG = ROOT / "assets" / "icon.svg"
OUT = ROOT / "packaging" / "icons"
SIZES = [16, 24, 32, 48, 64, 128, 256, 512]


def render(size: int) -> Image.Image:
    png = cairosvg.svg2png(url=str(SVG), output_width=size, output_height=size)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main() -> None:
    images = {size: render(size) for size in SIZES}
    for size, image in images.items():
        image.save(OUT / f"fasttail-{size}.png", optimize=True)
    # Each size rendered from the vector, not scaled down from the largest.
    ico = [s for s in SIZES if s <= 256]
    images[256].save(
        OUT / "fasttail.ico",
        sizes=[(s, s) for s in ico],
        append_images=[images[s] for s in ico if s != 256],
    )
    images[512].save(
        OUT / "FastTail.icns",
        append_images=[images[s] for s in (16, 32, 64, 128, 256)],
    )
    images[128].save(ROOT / "assets" / "icon.png", optimize=True)
    (ROOT / "assets" / "icon-128.rgba").write_bytes(images[128].tobytes())


if __name__ == "__main__":
    main()
