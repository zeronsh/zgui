#!/usr/bin/env python3
"""Images through the Windows clipboard, in and out of `clipboard_image`:

- images other apps put there paste whole: a 24-bit DIB (as screenshots and
  older apps copy), a 32-bit DIB whose fourth byte is unused (a GDI bitmap's,
  as Paint copies: opaque, not transparent), a DIBV5 whose alpha mask gives
  it alpha (as Firefox copies), a DIBV5 without an alpha mask whose first
  pixels read like colour masks, its block longer than its pixels (a V5
  header holds its masks: none follow it), a DIB with BI_BITFIELDS masks
  after its header (read as placed: the DIBV5 Windows makes of it repeats
  the masks after its header), and a PNG alone (as browsers and image
  editors add);
- an image zgx copies is there as a PNG and as a DIB with its alpha, and
  Windows offers the plain DIB older apps read: each read back here, every
  pixel compared;
- text alone on the clipboard pastes no image.

The user's clipboard is saved first and restored after, and what the smoke
puts there itself stays out of clipboard history. With cloud clipboard sync
on, the copy check is skipped, so nothing copied leaves the machine.

    cargo build -p zgx-desktop --example clipboard_image
    python scripts/windows_clipboard_image_smoke.py target/debug/examples/clipboard_image.exe
"""
import argparse
import ctypes
import pathlib
import re
import struct
import subprocess
import sys
import zlib

from windows_text_smoke import (
    CF_UNICODETEXT,
    cloud_clipboard,
    kernel32,
    open_clipboard,
    put_clipboard,
    save_clipboard,
    user32,
)

CF_DIB, CF_DIBV5 = 8, 17
PNG = user32.RegisterClipboardFormatW("PNG")


def fnv(data):
    hash = 0xCBF29CE484222325
    for byte in data:
        hash = ((hash ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return hash


def pattern(width, height, alpha):
    """RGBA rows top to bottom; `alpha(x, y)` or opaque."""
    out = bytearray()
    for y in range(height):
        for x in range(width):
            out += bytes((x * 6 % 256, y * 8 % 256, (x + y) * 3 % 256, alpha(x, y)))
    return bytes(out)


def dib(width, height, rgba, bits, fourth=None):
    """A bottom-up BI_RGB DIB of `rgba`; at 32 bits, the fourth byte of each
    pixel is `fourth` (or the alpha)."""
    header = struct.pack("<IiiHHIIiiII", 40, width, height, 1, bits, 0, 0, 0, 0, 0, 0)
    stride = (width * bits // 8 + 3) & ~3
    rows = []
    for y in reversed(range(height)):
        row = bytearray()
        for x in range(width):
            r, g, b, a = rgba[(y * width + x) * 4 : (y * width + x) * 4 + 4]
            row += bytes((b, g, r)) if bits == 24 else bytes((b, g, r, a if fourth is None else fourth))
        rows.append(bytes(row.ljust(stride, b"\0")))
    return header + b"".join(rows)


def dibv5(width, height, rgba, alpha=True, slack=0):
    """A bottom-up DIBV5 of `rgba` (BI_BITFIELDS), with an alpha mask or
    without one (its fourth bytes then 0), and `slack` bytes after it."""
    masks = struct.pack("<IIII", 0xFF0000, 0xFF00, 0xFF, 0xFF000000 if alpha else 0)
    header = struct.pack("<IiiHHIIiiII", 124, width, height, 1, 32, 3, width * height * 4, 0, 0, 0, 0)
    header += masks + struct.pack("<I", 0x73524742) + bytes(36) + bytes(12) + struct.pack("<IIII", 4, 0, 0, 0)
    assert len(header) == 124
    return header + dib(width, height, rgba, 32, fourth=None if alpha else 0)[40:] + bytes(slack)


def dib_fields(width, height, rgba):
    """A bottom-up 32-bit DIB of `rgba` whose colour masks (BI_BITFIELDS)
    follow its BITMAPINFOHEADER, the fourth byte unused."""
    header = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 32, 3, 0, 0, 0, 0, 0)
    masks = struct.pack("<III", 0xFF0000, 0xFF00, 0xFF)
    return header + masks + dib(width, height, rgba, 32, fourth=0)[40:]


def masks_first(width, height):
    """An opaque pattern whose stored first pixels (the bottom row's) are
    pure red, green and blue: words equal to the colour masks."""
    rgba = bytearray(pattern(width, height, lambda x, y: 255))
    bottom = (height - 1) * width * 4
    for i, colour in enumerate(((255, 0, 0), (0, 255, 0), (0, 0, 255))):
        rgba[bottom + i * 4 : bottom + i * 4 + 3] = bytes(colour)
    return bytes(rgba)


def png(width, height, rgba):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + rgba[y * width * 4 : (y + 1) * width * 4] for y in range(height))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def read_png(data):
    """RGBA rows of an 8-bit RGBA PNG, unfiltered."""
    assert data[:8] == b"\x89PNG\r\n\x1a\n", data[:8]
    at, idat = 8, b""
    while at < len(data):
        (length,), kind = struct.unpack(">I", data[at : at + 4]), data[at + 4 : at + 8]
        body = data[at + 8 : at + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
            assert (depth, colour) == (8, 6), (depth, colour)
        elif kind == b"IDAT":
            idat += body
        at += 12 + length
    raw, stride, out, previous = zlib.decompress(idat), width * 4, bytearray(), bytearray(width * 4)
    for y in range(height):
        kind, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b = previous[i]
            c = previous[i - 4] if i >= 4 else 0
            if kind == 1:
                line[i] = (line[i] + a) & 255
            elif kind == 2:
                line[i] = (line[i] + b) & 255
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        out += line
        previous = line
    return width, height, bytes(out)


def read_dib(data):
    """(width, height, RGBA rows top to bottom, header size, compression) of
    a 32-bit DIB or DIBV5 as the clipboard holds it."""
    size, width, height, _, bits, compression = struct.unpack("<IiiHHI", data[:20])
    assert bits == 32, bits
    pixels = data[size + (12 if compression == 3 and size == 40 else 0) :]
    rows = []
    for y in range(abs(height)):
        row = pixels[y * width * 4 : (y + 1) * width * 4]
        rows.append(b"".join(bytes((row[i + 2], row[i + 1], row[i], row[i + 3])) for i in range(0, len(row), 4)))
    if height > 0:
        rows.reverse()
    return width, abs(height), b"".join(rows), size, compression


def clipboard_formats():
    open_clipboard()
    try:
        found, fmt = {}, 0
        while fmt := user32.EnumClipboardFormats(fmt):
            handle = user32.GetClipboardData(fmt) if fmt in (CF_DIB, CF_DIBV5, PNG) else None
            if handle:
                pointer = kernel32.GlobalLock(handle)
                found[fmt] = ctypes.string_at(pointer, kernel32.GlobalSize(handle))
                kernel32.GlobalUnlock(handle)
            else:
                found.setdefault(fmt, None)
        return found
    finally:
        user32.CloseClipboard()


def run(exe, *args):
    result = subprocess.run([str(exe), "--smoke", *args], capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        raise SystemExit(f"FAIL: the example exited {result.returncode}: {result.stderr[-2000:]}")
    return result.stdout


def pasted(out):
    match = re.search(r"PASTED (none|(\d+)x(\d+) ([0-9a-f]{16}))", out)
    if not match:
        raise SystemExit(f"FAIL: no PASTED line in {out!r}")
    if match[1] == "none":
        return None
    return int(match[2]), int(match[3]), int(match[4], 16)


def gradient():
    """`clipboard_image`'s Copy: 64 by 48, its left quarter fading in."""
    out = bytearray()
    for y in range(48):
        for x in range(64):
            out += bytes((x * 4, y * 5, 160, x * 16 if x < 16 else 255))
    return bytes(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("exe", type=pathlib.Path)
    exe = parser.parse_args().exe.resolve()
    w, h = 40, 30
    opaque = pattern(w, h, lambda x, y: 255)
    alpha = pattern(w, h, lambda x, y: (x * 7 + y) % 256)
    cases = [
        ("24-bit DIB", [(CF_DIB, dib(w, h, opaque, 24))], opaque),
        ("32-bit DIB, fourth byte unused", [(CF_DIB, dib(w, h, opaque, 32, fourth=0))], opaque),
        ("DIBV5 with alpha", [(CF_DIBV5, dibv5(w, h, alpha))], alpha),
        (
            "DIBV5, first pixels like masks, with slack",
            [(CF_DIBV5, dibv5(w, h, masks_first(w, h), alpha=False, slack=12))],
            masks_first(w, h),
        ),
        ("DIB with masks after its header", [(CF_DIB, dib_fields(w, h, opaque))], opaque),
        ("PNG alone", [(PNG, png(w, h, alpha))], alpha),
    ]
    saved = save_clipboard()
    try:
        results = []
        for name, items, expected in cases:
            put_clipboard(items)
            got = pasted(run(exe))
            if got != (w, h, fnv(expected)):
                raise SystemExit(f"FAIL: {name} pasted as {got}, expected {(w, h, fnv(expected))}")
            results.append(name)
        put_clipboard([(CF_UNICODETEXT, "no image here\0".encode("utf-16-le"))])
        if pasted(run(exe)) is not None:
            raise SystemExit("FAIL: text alone pasted as an image")
        if cloud_clipboard():
            print(f"PASS: pasted {', '.join(results)}; none from text; copy skipped (cloud clipboard on)")
            return
        out = run(exe, "--copy")
        if "COPIED written=true" not in out:
            raise SystemExit(f"FAIL: the copy was refused: {out!r}")
        formats, expected = clipboard_formats(), gradient()
        if formats.get(PNG) is None or read_png(formats[PNG]) != (64, 48, expected):
            raise SystemExit("FAIL: the copied PNG is missing or differs")
        width, height, rgba, _, _ = read_dib(formats.get(CF_DIBV5) or b"\0" * 40)
        if (width, height, rgba) != (64, 48, expected):
            raise SystemExit("FAIL: the copied DIBV5 differs")
        width, height, rgba, size, _ = read_dib(formats.get(CF_DIB) or b"\0" * 40)
        if (width, height) != (64, 48) or [p[:3] for p in zip(*[iter(rgba)] * 4)] != [
            p[:3] for p in zip(*[iter(expected)] * 4)
        ]:
            raise SystemExit("FAIL: the DIB Windows offers older apps differs")
        print(
            f"PASS: pasted {', '.join(results)}; none from text; copied 64x48 as PNG, "
            f"DIBV5 with alpha and plain DIB (header {size}), every pixel alike"
        )
    finally:
        put_clipboard(saved)


if __name__ == "__main__":
    sys.exit(main())
