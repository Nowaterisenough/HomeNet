"""Rasterize the Lucide Waypoints mark at 2x without third-party dependencies."""

import math
from pathlib import Path
import struct
import zlib


SIZE = 44
SAMPLES = 8
STROKE = 1.7
CIRCLES = ((12, 4.5), (4.5, 12), (19.5, 12), (12, 19.5))
LINES = ((10.2, 6.3, 6.3, 10.2), (7, 12, 17, 12), (13.8, 17.7, 17.7, 13.8))


def covered(x, y):
    for cx, cy in CIRCLES:
        if abs(math.hypot(x - cx, y - cy) - 2.5) <= STROKE / 2:
            return True
    for ax, ay, bx, by in LINES:
        dx, dy = bx - ax, by - ay
        t = max(0, min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
        if math.hypot(x - ax - t * dx, y - ay - t * dy) <= STROKE / 2:
            return True
    return False


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


pixels = bytearray()
for y in range(SIZE):
    pixels.append(0)
    for x in range(SIZE):
        hits = sum(
            covered((x + (sx + 0.5) / SAMPLES) * 24 / SIZE,
                    (y + (sy + 0.5) / SAMPLES) * 24 / SIZE)
            for sy in range(SAMPLES) for sx in range(SAMPLES)
        )
        pixels.extend((142, 142, 147, round(hits * 255 / SAMPLES**2)))

output = Path(__file__).resolve().parents[1] / "src-tauri/icons/tray-icon.png"
output.write_bytes(
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(pixels, 9))
    + chunk(b"IEND", b"")
)
print(output)
