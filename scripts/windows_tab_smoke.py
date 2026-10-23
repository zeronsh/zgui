#!/usr/bin/env python3
"""Tabs between windows on Windows (drag_windows_smoke.py's twin):
`tab_windows`' tab torn off its window opens in a window of its own, the tab
under the pointer where it was grabbed; that window's only tab moves the
window itself (`drag_moves_window`), following the pointer to the pixel; and
dropped on the other window's strip it merges there, its window gone.

The tab is torn off over `drop_target_probe`, a test-owned window in
another process (a tab carries nothing to other apps, and the drag never
leaves the app: the pointer stays captured). Every drag's path is checked to
lie over the probe's and the test's windows before it starts. It moves the
app's windows to known places first, and takes the pointer while it runs.

    cargo build -p zgui-desktop --example tab_windows --example drop_target_probe
    python scripts/windows_tab_smoke.py target/debug/examples
"""
import argparse
import ctypes
import pathlib
import subprocess
import threading
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.WindowFromPoint.restype = wintypes.HWND
user32.WindowFromPoint.argtypes = [wintypes.POINT]
user32.GetAncestor.restype = wintypes.HWND
SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE = 0x1, 0x4, 0x10


def wait_for(check, what, timeout=8.0):
    deadline = time.monotonic() + timeout
    while True:
        found = check()
        if found:
            return found
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.05)


def named(pid, title):
    return next((w for w in win.windows_of(pid) if win.title(w) == title), None)


def origin(window):
    """Its client's top-left, in the screen's pixels."""
    return win.to_screen(window, 0, 0)


def place(window, x, y):
    """Its client's top-left to (x, y), the screen's pixels."""
    frame = wintypes.RECT()
    user32.GetWindowRect(window, ctypes.byref(frame))
    cx, cy = origin(window)
    user32.SetWindowPos(window, None, x - (cx - frame.left), y - (cy - frame.top), 0, 0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)
    time.sleep(0.3)


def window_at(x, y):
    return user32.GetAncestor(user32.WindowFromPoint(wintypes.POINT(x, y)), 2)  # GA_ROOT


def steps(a, b, n=20):
    return [(round(a[0] + (b[0] - a[0]) * i / n), round(a[1] + (b[1] - a[1]) * i / n)) for i in range(1, n + 1)]


def button(flag):
    return win.INPUT(win.INPUT_MOUSE, win._INPUTUNION(mi=win.MOUSEINPUT(0, 0, 0, flag, 0, 0)))


def drag(start, path, ours):
    """Press at screen point `start`, move through `path`, let go. Refuses
    (SystemExit) unless every point lies over `ours()`, the windows of the
    app and of the test."""
    windows = ours()
    if not all(window_at(x, y) in windows for x, y in [start, *path]):
        raise SystemExit("another window lies on the drag's way: not dragging")
    user32.SetCursorPos(*start)
    win._send(button(win.MOUSEEVENTF_MOVE))
    time.sleep(0.15)
    win._send(button(win.MOUSEEVENTF_LEFTDOWN))
    time.sleep(0.1)
    for x, y in path:
        user32.SetCursorPos(x, y)
        win._send(button(win.MOUSEEVENTF_MOVE))
        time.sleep(0.02)
    time.sleep(0.3)
    win._send(button(win.MOUSEEVENTF_LEFTUP), guard=False)
    time.sleep(0.6)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", type=pathlib.Path)
    args = parser.parse_args()
    app = subprocess.Popen([str(args.examples / "tab_windows.exe")], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
    backdrop = None
    try:
        one = wait_for(lambda: named(app.pid, "One"), "window One", 15)
        two = wait_for(lambda: named(app.pid, "Two"), "window Two", 15)
        scale = win.dpi(one) / 96
        # One at the top left, Two clear of it to the right, the test's own
        # window below One, where the tab is torn off.
        place(one, round(40 * scale), round(60 * scale))
        place(two, round(40 * scale) + round(600 * scale), round(60 * scale))
        ox, oy = origin(one)
        _, height = win.client_size(one)
        backdrop = subprocess.Popen([str(args.examples / "drop_target_probe.exe"), f"--at={ox},{oy + height + 40}"],
                                    stdout=subprocess.PIPE, text=True)
        backdrop_lines = []
        threading.Thread(target=lambda: backdrop_lines.extend(line.strip() for line in backdrop.stdout),
                         daemon=True).start()
        wait_for(lambda: any(line.startswith("TARGET") for line in backdrop_lines), "the backdrop")
        backdrop_window = win.wait_window(backdrop)
        _, bx, by, bw, bh = next(line for line in backdrop_lines if line.startswith("TARGET")).split()
        win.activate(backdrop_window)
        win.activate(two)
        win.activate(one)
        time.sleep(0.3)

        def ours():
            return set(win.windows_of(app.pid)) | {backdrop_window}

        # The Inbox tab's middle: the strip's padding and half a tab across,
        # the tab's top and half its height down.
        grab = (round((10 + 75) * scale), round((6 + 16) * scale))
        start = (ox + grab[0], oy + grab[1])
        release = (int(bx) + int(bw) // 2, int(by) + int(bh) // 2)
        drag(start, steps(start, release), ours)
        torn = wait_for(lambda: named(app.pid, "Inbox"), "the torn-off window")
        time.sleep(0.4)
        tx, ty = origin(torn)
        expected = (release[0] - grab[0], release[1] - grab[1])
        print(f"torn-off content at {(tx, ty)}, expected {expected}")
        assert abs(tx - expected[0]) <= 1 and abs(ty - expected[1]) <= 1, "the torn-off tab is not under the pointer"
        print("ok: a tab torn off opens in a window of its own, the tab under the pointer")

        # Its only tab moves the window itself. The drag starts past its
        # threshold at the first step; that point stays under the pointer.
        win.activate(torn)
        start = (tx + grab[0], ty + grab[1])
        first = (start[0] + 12, start[1] - 4)
        end = (start[0] + round(160 * scale), start[1] - round(120 * scale))
        path = [first, *steps(first, end)]
        windows = ours()
        if window_at(*start) != torn:
            raise SystemExit("another window covers the torn-off window's tab: not dragging")
        user32.SetCursorPos(*start)
        win._send(button(win.MOUSEEVENTF_MOVE))
        time.sleep(0.15)
        win._send(button(win.MOUSEEVENTF_LEFTDOWN))
        time.sleep(0.1)
        followed = []
        for x, y in path:
            user32.SetCursorPos(x, y)
            win._send(button(win.MOUSEEVENTF_MOVE))
            time.sleep(0.03)
            # The window under the pointer is the one it moves.
            followed.append(window_at(x, y) in windows)
        time.sleep(0.3)
        win._send(button(win.MOUSEEVENTF_LEFTUP), guard=False)
        time.sleep(0.6)
        mx, my = origin(torn)
        moved = (mx - tx, my - ty)
        wanted = (end[0] - first[0], end[1] - first[1])
        print(f"moved by {moved}, the pointer by {wanted} after the drag began")
        assert all(followed), "the pointer left the app's windows while moving one"
        assert abs(moved[0] - wanted[0]) <= 1 and abs(moved[1] - wanted[1]) <= 1, "the window did not follow"
        assert named(app.pid, "Inbox"), "the moved window went"
        print("ok: a window's only tab moves the window, following the pointer to the pixel")

        # Onto One's strip: it merges, its window gone.
        win.activate(one)
        win.activate(torn)
        start = (mx + grab[0], my + grab[1])
        target = (ox + round(400 * scale), oy + grab[1])
        drag(start, steps(start, target), ours)
        wait_for(lambda: not named(app.pid, "Inbox"), "the moved window to merge into One")
        print("ok: dropped on the other window's strip, it merges there, its window gone")
        print("tab smoke passed")
    finally:
        for proc in (app, backdrop):
            if proc and proc.poll() is None:
                proc.kill()


if __name__ == "__main__":
    main()
