#!/usr/bin/env python3
"""A `Titlebar::Inset` window's chrome on Windows: the caption buttons zgx
draws (hover, maximize, restore, minimize, close through the system's own
commands) and the resize bands of the frameless window, driven by SendInput.

    cargo build -p zgx-desktop --example kitchen_sink
    python scripts/windows_chrome_smoke.py target/debug/examples/kitchen_sink.exe

Windows 11 also shows snap layouts on the maximize button (the window reports
it as `HTMAXBUTTON`); its flyout is not shown for synthesized hovers, so it is
not checked here.
"""
import argparse
import ctypes
import pathlib
import subprocess
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32")
user32.IsZoomed.argtypes = [wintypes.HWND]
user32.IsIconic.argtypes = [wintypes.HWND]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
CLOSE, MAXIMIZE, MINIMIZE = 0, 1, 2


def button(window, index):
    """Client point at the middle of caption button `index` (from the right)."""
    scale = win.dpi(window) / 96
    width, _ = win.client_size(window)
    return width - 46 * scale * (index + 0.5), 16 * scale


def pixel_at(window, point):
    """The button's fill beside its glyph."""
    offset = 14 * win.dpi(window) / 96
    return win.pixel(win.capture(window), round(point[0] - offset), round(point[1]))


def wait_for(check, what, timeout=5.0):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    proc = subprocess.Popen([str(args.binary.resolve())], stdout=subprocess.DEVNULL)
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.move(window, 200, 300)
        time.sleep(0.3)
        rest = pixel_at(window, button(window, CLOSE))
        win.move(window, *button(window, CLOSE))
        def red_hover():
            r, g, b = pixel_at(window, button(window, CLOSE))
            return r > 150 and g < 100 and b < 100
        wait_for(red_hover, "the close button's red hover")
        red = pixel_at(window, button(window, CLOSE))
        win.move(window, 200, 300)
        wait_for(lambda: pixel_at(window, button(window, CLOSE)) == rest, "the hover to clear")

        size = win.client_size(window)
        win.click(window, *button(window, MAXIMIZE))
        wait_for(lambda: user32.IsZoomed(window), "maximize")
        maximized = win.client_size(window)
        win.click(window, *button(window, MAXIMIZE))
        wait_for(lambda: not user32.IsZoomed(window) and win.client_size(window) == size, "restore")

        width, height = win.client_size(window)
        win.drag(window, (width - 2, height // 2), (width + 98, height // 2))
        wait_for(lambda: win.client_size(window)[0] > width + 50, "a resize from the right edge band")
        resized = win.client_size(window)

        win.click(window, *button(window, MINIMIZE))
        wait_for(lambda: user32.IsIconic(window), "minimize")
        user32.ShowWindow(window, 9)  # SW_RESTORE
        # Restoring animates: the buttons take presses once it has ended.
        time.sleep(1.5)
        win.activate(window)
        time.sleep(0.3)
        win.click(window, *button(window, CLOSE))
        proc.wait(timeout=5)
        print(f"PASS: caption buttons (close hover {red}), maximize {size} -> {maximized} -> restore, "
              f"edge resize -> {resized}, minimize, close")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    main()
