#!/usr/bin/env python3
"""Live DPI changes on Windows (dpi_transition_smoke.py's twin): the display's
scale goes 100% -> 150% -> 100% while dpi_transition runs.

At each scale the window must take the new DPI (per-monitor v2), keep its
logical size (the physical size scales), keep the editor's layout in logical
pixels, and take typing at the scaled position. The display's own scale is
restored afterwards, whatever happens; the script changes it the way
Settings does, for the signed-in user only.

    cargo build -p zgx-desktop --example dpi_transition
    python scripts/windows_dpi_smoke.py target/debug/examples/dpi_transition.exe
"""
import argparse
import ctypes
import pathlib
import re
import subprocess
import threading
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32")
STEPS = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500]


class LUID(ctypes.Structure):
    _fields_ = [("low", wintypes.DWORD), ("high", wintypes.LONG)]


class Header(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int32), ("size", ctypes.c_uint32), ("adapter", LUID), ("id", ctypes.c_uint32)]


class GetScale(ctypes.Structure):
    _fields_ = [("header", Header), ("min", ctypes.c_int32), ("cur", ctypes.c_int32), ("max", ctypes.c_int32)]


class SetScale(ctypes.Structure):
    _fields_ = [("header", Header), ("scale", ctypes.c_int32)]


def primary_source():
    """(adapter LUID, source id) of the first active display path."""
    paths, modes = wintypes.UINT(), wintypes.UINT()
    user32.GetDisplayConfigBufferSizes(2, ctypes.byref(paths), ctypes.byref(modes))  # QDC_ONLY_ACTIVE_PATHS
    path_buffer = ctypes.create_string_buffer(72 * paths.value)
    mode_buffer = ctypes.create_string_buffer(64 * modes.value)
    if user32.QueryDisplayConfig(2, ctypes.byref(paths), path_buffer, ctypes.byref(modes), mode_buffer, None):
        raise OSError("QueryDisplayConfig failed")
    source = LUID.from_buffer_copy(path_buffer.raw[:8])
    return source, ctypes.c_uint32.from_buffer_copy(path_buffer.raw[8:12]).value


def get_scale(adapter, source):
    request = GetScale(Header(-3, ctypes.sizeof(GetScale), adapter, source))
    if user32.DisplayConfigGetDeviceInfo(ctypes.byref(request)):
        raise OSError("cannot read the display's scale")
    recommended = -request.min
    return STEPS[recommended + request.cur], recommended


def set_scale(adapter, source, percent):
    _, recommended = get_scale(adapter, source)
    request = SetScale(Header(-4, ctypes.sizeof(SetScale), adapter, source), STEPS.index(percent) - recommended)
    if user32.DisplayConfigSetDeviceInfo(ctypes.byref(request)):
        raise OSError(f"cannot set the display's scale to {percent}%")


def wait_for(check, what, timeout=8.0):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--scale", type=int, default=150, choices=STEPS[1:])
    parser.add_argument("--screenshots", type=pathlib.Path, help="directory for a capture at each scale")
    args = parser.parse_args()
    adapter, source = primary_source()
    original, _ = get_scale(adapter, source)
    proc = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()

    def reports():
        return [line for line in list(lines) if line.startswith("DPI ")]

    try:
        if original != 100:
            set_scale(adapter, source, 100)
            time.sleep(1)
        window = win.wait_window(proc)
        time.sleep(0.5)
        results = []
        for percent in [100, args.scale, 100]:
            if win.dpi(window) * 100 // 96 != percent:
                set_scale(adapter, source, percent)
            wait_for(lambda: win.dpi(window) * 100 // 96 == percent, f"the window at {percent}%")
            scale = percent / 100
            expected = (round(800 * scale), round(500 * scale))
            wait_for(lambda: win.client_size(window) == expected,
                     f"a {expected} client area at {percent}% (has {win.client_size(window)})")
            time.sleep(0.4)
            win.activate(window)
            # Type at the end of the editor, then report through the button,
            # both found at their logical position times the scale.
            win.click(window, 400 * scale, 76 * scale)
            win.key("end")
            win.type_text(str(percent // 25))
            before = len(reports())
            win.click(window, 85 * scale, 128 * scale)
            wait_for(lambda: len(reports()) > before, f"a report at {percent}%")
            report = reports()[-1]
            if args.screenshots:
                args.screenshots.mkdir(parents=True, exist_ok=True)
                win.save_png(win.capture(window), args.screenshots / f"dpi-{len(results)}-{percent}.png")
            results.append((percent, report))
        for percent, report in results:
            if "viewport=(800.0, 500.0)" not in report:
                raise AssertionError(f"logical viewport changed at {percent}%: {report}")
            if not re.search(r"editor=Rect \{ x: 20\.0, y: 54\.0, width: 400\.0, height: 44\.0 \}", report):
                raise AssertionError(f"editor layout changed at {percent}%: {report}")
        typed = re.search(r'model="(\d+)"', results[-1][1]).group(1)
        if not typed.endswith("4" + str(args.scale // 25) + "4"):
            raise AssertionError(f"typing at each scale did not all reach the editor: {typed}")
        win.close(window)
        proc.wait(timeout=10)
        print(f"PASS: live DPI 100% -> {args.scale}% -> 100%: per-monitor DPI, physical size, "
              f"logical layout, pointer and typing at each scale ({typed})")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if get_scale(adapter, source)[0] != original:
            set_scale(adapter, source, original)


if __name__ == "__main__":
    main()
