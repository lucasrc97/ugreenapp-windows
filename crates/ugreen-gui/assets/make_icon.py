#!/usr/bin/env python3
"""Draws the application icon: white headphones on a green rounded square.

Standard library only. Writes, next to this script:
  icon.ico  embedded in the Windows executable by ../build.rs
  icon.png  loaded at run time for the window and the taskbar

Run again after changing the drawing:  python3 assets/make_icon.py
"""

import math
import pathlib
import struct
import zlib

GREEN = (0x1A, 0xB3, 0x4B)
WHITE = (0xFF, 0xFF, 0xFF)
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 256]


def rounded_box(x, y, cx, cy, half_w, half_h, radius):
    """Signed distance to a rounded rectangle (negative inside)."""
    dx = abs(x - cx) - (half_w - radius)
    dy = abs(y - cy) - (half_h - radius)
    outside = math.hypot(max(dx, 0.0), max(dy, 0.0))
    return outside + min(max(dx, dy), 0.0) - radius


def headphones(x, y):
    """Signed distance to the headphones, in units of the icon's side."""
    centre_x, centre_y, radius = 0.5, 0.50, 0.235
    ring = abs(math.hypot(x - centre_x, y - centre_y) - radius) - 0.038
    band = max(ring, y - centre_y)  # upper half of the ring only
    cups = min(
        rounded_box(x, y, centre_x - radius, 0.615, 0.08, 0.145, 0.07),
        rounded_box(x, y, centre_x + radius, 0.615, 0.08, 0.145, 0.07),
    )
    return min(band, cups)


def coverage(distance, size):
    return min(max(0.5 - distance * size, 0.0), 1.0)


def render(size):
    """RGBA rows, top to bottom, straight (not premultiplied) alpha."""
    rows = []
    for row in range(size):
        line = bytearray()
        for column in range(size):
            x, y = (column + 0.5) / size, (row + 0.5) / size
            alpha = coverage(rounded_box(x, y, 0.5, 0.5, 0.47, 0.47, 0.21), size)
            ink = coverage(headphones(x, y), size)
            colour = [round(g + (w - g) * ink) for g, w in zip(GREEN, WHITE)]
            line += bytes(colour + [round(alpha * 255)])
        rows.append(bytes(line))
    return rows


def png(rows):
    size = len(rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\x00" + row for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def bitmap(rows):
    """Icon bitmap: 32-bit BGRA, bottom-up, followed by an empty AND mask."""
    size = len(rows)
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = b"".join(
        bytes(row[i + c] for i in range(0, len(row), 4) for c in (2, 1, 0, 3))
        for row in reversed(rows)
    )
    mask_row = bytes(((size + 31) // 32) * 4)
    return header + pixels + mask_row * size


def main():
    here = pathlib.Path(__file__).parent
    images = []
    for size in ICO_SIZES:
        rows = render(size)
        images.append((size, png(rows) if size >= 256 else bitmap(rows)))
        if size == 256:
            (here / "icon.png").write_bytes(png(rows))

    directory = struct.pack("<HHH", 0, 1, len(images))
    offset = len(directory) + 16 * len(images)
    entries = b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    (here / "icon.ico").write_bytes(directory + entries + b"".join(d for _, d in images))


if __name__ == "__main__":
    main()
