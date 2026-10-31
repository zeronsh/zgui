#!/usr/bin/env python3
"""Owned Xvfb/Openbox check of a window made hidden (`WindowOptions::visible`).

The hidden_window example runs hidden, counting, then shows itself. Checked:
the window is not mapped before it says SHOW; from its mapping until its
first frame after (X11 keeps no picture of an unmapped window) the screen
shows its background colour, never black; then its content.

Requires Xvfb, Openbox, xdotool and libX11.
"""
import argparse
import ctypes
import ctypes.util
import os
from pathlib import Path
import select
import subprocess
import threading
import time

from platform_smoke import stop, wait_for_wm


class XWindowAttributes(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_int), ("y", ctypes.c_int),
        ("width", ctypes.c_int), ("height", ctypes.c_int),
        ("border_width", ctypes.c_int), ("depth", ctypes.c_int),
        ("visual", ctypes.c_void_p), ("root", ctypes.c_ulong),
        ("class_", ctypes.c_int), ("bit_gravity", ctypes.c_int),
        ("win_gravity", ctypes.c_int), ("backing_store", ctypes.c_int),
        ("backing_planes", ctypes.c_ulong), ("backing_pixel", ctypes.c_ulong),
        ("save_under", ctypes.c_int), ("colormap", ctypes.c_ulong),
        ("map_installed", ctypes.c_int), ("map_state", ctypes.c_int),
        ("all_event_masks", ctypes.c_long), ("your_event_mask", ctypes.c_long),
        ("do_not_propagate_mask", ctypes.c_long), ("override_redirect", ctypes.c_int),
        ("screen", ctypes.c_void_p),
    ]


class XImage(ctypes.Structure):
    _fields_ = [
        ("width", ctypes.c_int), ("height", ctypes.c_int),
        ("xoffset", ctypes.c_int), ("format", ctypes.c_int),
        ("data", ctypes.POINTER(ctypes.c_uint8)),
        ("byte_order", ctypes.c_int), ("bitmap_unit", ctypes.c_int),
        ("bitmap_bit_order", ctypes.c_int), ("bitmap_pad", ctypes.c_int),
        ("depth", ctypes.c_int), ("bytes_per_line", ctypes.c_int),
        ("bits_per_pixel", ctypes.c_int),
    ]


IS_VIEWABLE = 2
Z_PIXMAP = 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=5)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    x = ctypes.CDLL(ctypes.util.find_library("X11"))
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XWindowAttributes)]
    x.XTranslateCoordinates.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
        ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong),
    ]
    x.XGetImage.restype = ctypes.POINTER(XImage)
    x.XGetImage.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
        ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int,
    ]
    x.XFree.argtypes = [ctypes.c_void_p]
    xvfb = wm = None
    try:
        with (output / "xvfb.log").open("w") as xlog, (output / "openbox.log").open("w") as wlog:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=xlog, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb did not start")
            display = xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=":" + display, XDG_RUNTIME_DIR="/tmp", WINIT_X11_SCALE_FACTOR="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            wm = subprocess.Popen(["openbox"], env=env, stdout=wlog, stderr=wlog)
            wait_for_wm(env)
            dpy = x.XOpenDisplay(env["DISPLAY"].encode())
            if not dpy:
                raise RuntimeError("no display")
            root = x.XDefaultRootWindow(dpy)
            for run in range(1, args.runs + 1):
                check(x, dpy, root, args.binary.resolve(), env, output / f"run{run}.log", run)
    finally:
        stop(wm)
        stop(xvfb)
    print(f"PASS: hidden until shown, its background (not black) until its first frame, {args.runs} runs")


def capture(x, dpy, root, window):
    """Whether the window is mapped, and its area's colour on screen (one
    when flat, two when anything is drawn)."""
    attributes = XWindowAttributes()
    if not x.XGetWindowAttributes(dpy, window, ctypes.byref(attributes)):
        return None, None
    if attributes.map_state != IS_VIEWABLE:
        return False, None
    sx, sy, child = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
    x.XTranslateCoordinates(dpy, window, root, 0, 0, ctypes.byref(sx), ctypes.byref(sy), ctypes.byref(child))
    image = x.XGetImage(dpy, root, sx.value, sy.value, attributes.width, attributes.height, 0xFFFFFFFF, Z_PIXMAP)
    if not image:
        return True, None
    im = image.contents
    data = ctypes.string_at(im.data, im.bytes_per_line * im.height)
    # Flat (one colour, nothing drawn) compared at C speed, so a gap of a
    # millisecond or two is still caught; its colour, or two for drawn.
    first = data[:4]
    if data == first * (len(data) // 4):
        colours = {int.from_bytes(first[:3], "little")}
    else:
        colours = {0, 1}
    x.XFree(im.data)
    x.XFree(image)
    return True, colours


def check(x, dpy, root, binary, env, log_path, run):
    with log_path.open("w") as log:
        app = subprocess.Popen([str(binary)], env=env, stdout=subprocess.PIPE, stderr=log, text=True)
        try:
            lines = []
            threading.Thread(target=lambda: lines.extend(app.stdout), daemon=True).start()
            window = None
            shown_at = None
            flats = []
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                said = "".join(lines)
                if window is None and "HIDDEN" in said:
                    found = subprocess.run(["xdotool", "search", "--name", "^zgx hidden window$"], env=env, capture_output=True, text=True).stdout.split()
                    window = int(found[0]) if found else None
                if window is None:
                    time.sleep(.01)
                    continue
                mapped, colours = capture(x, dpy, root, window)
                if mapped and "SHOW" not in said:
                    # It may have printed SHOW as it mapped: read again first.
                    said = "".join(lines)
                    if "SHOW" not in said:
                        raise RuntimeError(f"run {run}: mapped before SHOW")
                if not mapped or colours is None:
                    continue
                shown_at = shown_at or time.monotonic()
                if len(colours) <= 1:
                    flats.append(next(iter(colours)))
                    continue
                break
            else:
                raise RuntimeError(f"run {run}: never drawn after it mapped; said {said!r}")
            if 0x000000 in flats:
                raise RuntimeError(f"run {run}: black between its mapping and its first frame")
            if len(set(flats)) > 1:
                raise RuntimeError(f"run {run}: more than its background before its first frame: {sorted(set(flats))}")
            drawn_ms = (time.monotonic() - shown_at) * 1e3
            colour = f"#{flats[0]:06x}" if flats else "none"
            print(f"run {run}: {len(flats)} captures of its background ({colour}) then drawn, {drawn_ms:.1f} ms after mapping")
        finally:
            app.wait(timeout=15)
            if app.returncode != 0:
                raise RuntimeError(f"run {run}: exited {app.returncode}")


if __name__ == "__main__":
    main()
