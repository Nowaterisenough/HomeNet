"""Rasterize the unframed menu bar SVG at 2x without third-party dependencies."""

import math
from pathlib import Path
import re
import struct
import xml.etree.ElementTree as ET
import zlib

ROOT = Path(__file__).resolve().parents[1]
SVG = ET.parse(ROOT / "src-tauri/icons/tray-icon.svg").getroot()
NS = {"svg": "http://www.w3.org/2000/svg"}
SIZE = int(SVG.attrib["width"])
VIEW_SIZE = float(SVG.attrib["viewBox"].split()[2])
SAMPLES = 8
COLOR = tuple(bytes.fromhex(SVG.attrib["stroke"].removeprefix("#")))
STROKE = float(SVG.attrib["stroke-width"])
CIRCLES = [tuple(float(circle.attrib[key]) for key in ("cx", "cy", "r")) for circle in SVG.findall("svg:circle", NS)]


def line_segment(path):
    # The menu bar mark uses relative straight segments and a horizontal connector.
    data = path.attrib["d"]
    values = list(map(float, re.findall(r"[-+]?(?:\d*\.\d+|\d+)", data)))
    if data.startswith("m") and len(values) == 4:
        x, y, dx, dy = values
        return x, y, x + dx, y + dy
    if data.startswith("M") and "h" in data and len(values) == 3:
        x, y, dx = values
        return x, y, x + dx, y
    raise ValueError(f"Unsupported icon path: {data}")


LINES = [line_segment(path) for path in SVG.findall("svg:path", NS)]


def covered(x, y):
    for cx, cy, radius in CIRCLES:
        if abs(math.hypot(x - cx, y - cy) - radius) <= STROKE / 2:
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
            covered((x + (sx + 0.5) / SAMPLES) * VIEW_SIZE / SIZE,
                    (y + (sy + 0.5) / SAMPLES) * VIEW_SIZE / SIZE)
            for sy in range(SAMPLES) for sx in range(SAMPLES)
        )
        pixels.extend((*COLOR, round(hits * 255 / SAMPLES**2)))

output = ROOT / "src-tauri/icons/tray-icon.png"
output.write_bytes(
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(pixels, 9))
    + chunk(b"IEND", b"")
)
print(output)
