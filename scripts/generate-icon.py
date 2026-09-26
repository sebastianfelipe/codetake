#!/usr/bin/env python3
"""Generate the CodeTake app icon source (1024x1024 PNG) with no dependencies.

The icon is a dark rounded square with a code-bracket pair around a red
"record" dot. Run `pnpm --filter @codetake/desktop tauri icon <output>` afterwards
to derive every platform-specific size.
"""

import math
import struct
import sys
import zlib

SIZE = 1024
BG = (21, 23, 28)
RECORD = (239, 68, 68)
BRACKET = (226, 232, 240)


def smoothstep(edge0: float, edge1: float, x: float) -> float:
    t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0)))
    return t * t * (3 - 2 * t)


def rounded_rect_sdf(x: float, y: float, half: float, radius: float) -> float:
    qx = abs(x) - (half - radius)
    qy = abs(y) - (half - radius)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    inside = min(max(qx, qy), 0.0)
    return outside + inside - radius


def segment_sdf(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
    pax, pay = px - ax, py - ay
    bax, bay = bx - ax, by - ay
    h = max(0.0, min(1.0, (pax * bax + pay * bay) / (bax * bax + bay * bay)))
    return math.hypot(pax - bax * h, pay - bay * h)


def bracket_sdf(x: float, y: float, direction: float) -> float:
    # A "<" (direction=-1) or ">" (direction=1) chevron centred at the origin.
    tip_x = 170.0 * direction
    back_x = 70.0 * direction
    d1 = segment_sdf(x, y, back_x, -120.0, tip_x, 0.0)
    d2 = segment_sdf(x, y, tip_x, 0.0, back_x, 120.0)
    return min(d1, d2) - 26.0


def pixel(ix: int, iy: int) -> tuple[int, int, int, int]:
    x = ix + 0.5 - SIZE / 2
    y = iy + 0.5 - SIZE / 2

    bg_alpha = 1.0 - smoothstep(-1.0, 1.0, rounded_rect_sdf(x, y, 440.0, 190.0))
    if bg_alpha <= 0.0:
        return (0, 0, 0, 0)

    color = list(BG)

    def blend(src: tuple[int, int, int], alpha: float) -> None:
        for i in range(3):
            color[i] = color[i] * (1 - alpha) + src[i] * alpha

    dot = math.hypot(x, y) - 92.0
    blend(RECORD, 1.0 - smoothstep(-1.0, 1.0, dot))

    left = bracket_sdf(x + 90.0, y, -1.0)
    right = bracket_sdf(x - 90.0, y, 1.0)
    blend(BRACKET, 1.0 - smoothstep(-1.0, 1.0, min(left, right)))

    return (round(color[0]), round(color[1]), round(color[2]), round(bg_alpha * 255))


def write_png(path: str) -> None:
    rows = bytearray()
    for iy in range(SIZE):
        rows.append(0)
        for ix in range(SIZE):
            rows.extend(pixel(ix, iy))

    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    write_png(sys.argv[1] if len(sys.argv) > 1 else "app-icon.png")
