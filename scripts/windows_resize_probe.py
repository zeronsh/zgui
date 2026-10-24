#!/usr/bin/env python3
"""How a window looks while it is resized on Windows: the `resize_probe`
example (edged in red on every side) is resized step by step, maximized,
restored and snapped, and its client area is captured as fast as GDI can
after each change, until a frame drawn for the new size shows (red at its
new right and bottom edges, blue just inside). Each capture before that is
classed as black (nothing drawn there: the window's background) or stale (an
earlier frame, at its old size or stretched).

    cargo build --release -p zgx-desktop --example resize_probe
    python scripts/windows_resize_probe.py target/release/examples/resize_probe.exe

It reports, per kind of change, how long the new size took to show and how
many black and stale captures came first. `--drag` also resizes by dragging
the right edge with the pointer (the system's live resize, its modal loop),
which takes the pointer while it runs. `--reference` measures a plain GDI
window painting the same thing (`WM_PAINT`) instead: what Windows itself
shows for each change, its animations included.
"""
import argparse
import ctypes
import pathlib
import statistics
import subprocess
import time

import windows_input as win

user32 = ctypes.WinDLL("user32", use_last_error=True)
SWP_NOMOVE, SWP_NOZORDER, SWP_NOACTIVATE = 0x2, 0x4, 0x10
SW_MAXIMIZE, SW_RESTORE = 3, 9


def kind(capture, scale):
    """What a capture of the client area shows: "new", "black" or "stale"."""
    width, height, _ = capture
    inset = round(8 * scale)
    red = lambda p: p[0] > 200 and p[1] < 60 and p[2] < 60
    blue = lambda p: p[2] > 150 and p[0] < 80
    black = lambda p: max(p) < 40
    right = win.pixel(capture, width - 2, height // 2)
    bottom = win.pixel(capture, width // 2, height - 2)
    inside = win.pixel(capture, width - inset - 6, height // 2)
    if red(right) and red(bottom) and blue(inside):
        return "new"
    if black(right) or black(bottom):
        return "black"
    return "stale"


def settle(window, scale, changed_at, expect=None, timeout=1.0):
    """Capture until the window shows a frame for its size: (ms to it, kinds before)."""
    seen = []
    while time.perf_counter() - changed_at < timeout:
        size = win.client_size(window)
        if expect and size != expect:
            continue
        capture = win.capture(window)
        what = kind(capture, scale)
        if what == "new":
            return (time.perf_counter() - changed_at) * 1000, seen
        seen.append(what)
    return None, seen


def report(name, results):
    shown = [ms for ms, _ in results if ms is not None]
    black = sum(seen.count("black") for _, seen in results)
    stale = sum(seen.count("stale") for _, seen in results)
    missing = len(results) - len(shown)
    timing = f"median {statistics.median(shown):.1f} ms, max {max(shown):.1f} ms" if shown else "never shown"
    print(f"{name}: {len(results)} changes, new size shown {timing}; "
          f"captures before it: {black} black, {stale} stale; never shown: {missing}")
    return black, stale, missing


def reference_window():
    """A plain Win32 window painting resize_probe's picture with GDI."""
    from ctypes import wintypes
    gdi32 = ctypes.WinDLL("gdi32")
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # per-monitor v2, as winit
    WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, wintypes.HWND, wintypes.UINT, wintypes.WPARAM,
                                 wintypes.LPARAM)
    user32.DefWindowProcW.restype = ctypes.c_ssize_t
    user32.DefWindowProcW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]

    class PAINTSTRUCT(ctypes.Structure):
        _fields_ = [("hdc", wintypes.HDC), ("fErase", wintypes.BOOL), ("rcPaint", wintypes.RECT),
                    ("fRestore", wintypes.BOOL), ("fIncUpdate", wintypes.BOOL), ("rgb", ctypes.c_byte * 32)]

    class WNDCLASSW(ctypes.Structure):
        _fields_ = [("style", wintypes.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                    ("cbWndExtra", ctypes.c_int), ("hInstance", wintypes.HINSTANCE), ("hIcon", wintypes.HICON),
                    ("hCursor", wintypes.HANDLE), ("hbrBackground", wintypes.HBRUSH),
                    ("lpszMenuName", wintypes.LPCWSTR), ("lpszClassName", wintypes.LPCWSTR)]

    red, blue = gdi32.CreateSolidBrush(0x0000FF), gdi32.CreateSolidBrush(0xC06020)

    def proc(hwnd, message, wparam, lparam):
        if message == 0x000F:  # WM_PAINT
            paint = PAINTSTRUCT()
            dc = user32.BeginPaint(hwnd, ctypes.byref(paint))
            rect = wintypes.RECT()
            user32.GetClientRect(hwnd, ctypes.byref(rect))
            user32.FillRect(dc, ctypes.byref(rect), red)
            inset = round(8 * user32.GetDpiForWindow(hwnd) / 96)
            inner = wintypes.RECT(inset, inset, rect.right - inset, rect.bottom - inset)
            user32.FillRect(dc, ctypes.byref(inner), blue)
            user32.EndPaint(hwnd, ctypes.byref(paint))
            return 0
        if message == 0x0014:  # WM_ERASEBKGND: painted whole in WM_PAINT
            return 1
        if message == 0x0002:  # WM_DESTROY
            user32.PostQuitMessage(0)
            return 0
        return user32.DefWindowProcW(hwnd, message, wparam, lparam)

    callback = WNDPROC(proc)
    window_class = WNDCLASSW(3, callback, 0, 0, None, None, None, None, None, "ResizeReference")  # H|V redraw
    user32.RegisterClassW(ctypes.byref(window_class))
    user32.CreateWindowExW.restype = wintypes.HWND
    hwnd = user32.CreateWindowExW(0, "ResizeReference", "Resize probe", 0x10CF0000,  # overlapped, visible
                                  0x80000000, 0x80000000, 616, 439, None, None, None, None)
    message = wintypes.MSG()
    while user32.GetMessageW(ctypes.byref(message), None, 0, 0) > 0:
        user32.TranslateMessage(ctypes.byref(message))
        user32.DispatchMessageW(ctypes.byref(message))
    return hwnd


def main():
    import sys
    if "--serve-reference" in sys.argv:
        reference_window()
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, nargs="?")
    parser.add_argument("--drag", action="store_true")
    parser.add_argument("--reference", action="store_true")
    parser.add_argument("--steps", type=int, default=20)
    args = parser.parse_args()
    command = ([sys.executable, __file__, "--serve-reference"] if args.reference
               else [str(args.binary.resolve())])
    proc = subprocess.Popen(command)
    totals = []
    try:
        window = win.wait_window(proc)
        time.sleep(1.5)
        win.activate(window)
        scale = win.dpi(window) / 96
        rect = ctypes.wintypes.RECT()
        user32.GetWindowRect(window, ctypes.byref(rect))
        width, height = rect.right - rect.left, rect.bottom - rect.top

        # Programmatic resizes (SetWindowPos from another process), growing
        # then shrinking by 40 pixels a step.
        results = []
        for step in range(args.steps):
            grow = step < args.steps // 2
            width += 40 if grow else -40
            height += 24 if grow else -24
            changed = time.perf_counter()
            user32.SetWindowPos(window, None, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)
            results.append(settle(window, scale, changed))
            time.sleep(0.15)
        totals.append(report("resize", results))

        # Maximize and restore, a few times.
        results = []
        for _ in range(3):
            for command in (SW_MAXIMIZE, SW_RESTORE):
                before = win.client_size(window)
                changed = time.perf_counter()
                user32.ShowWindowAsync(window, command)
                while win.client_size(window) == before and time.perf_counter() - changed < 1:
                    pass
                results.append(settle(window, scale, changed))
                time.sleep(0.4)
        totals.append(report("maximize/restore", results))

        # Snapped to the left half (Win+Left), then restored. Snap Assist may
        # take the foreground to offer other windows for the right half:
        # bringing this window back to the front dismisses it.
        results = []
        for _ in range(2):
            win.activate(window)
            before = win.client_size(window)
            changed = time.perf_counter()
            win.key("win+left")
            while win.client_size(window) == before and time.perf_counter() - changed < 1:
                pass
            results.append(settle(window, scale, changed))
            time.sleep(0.5)
            win.activate(window)
            user32.ShowWindowAsync(window, SW_RESTORE)
            time.sleep(0.6)
        totals.append(report("snap", results))

        if args.drag:
            # The system's live resize: the right edge dragged by the pointer.
            user32.ShowWindowAsync(window, SW_RESTORE)
            time.sleep(0.5)
            win.activate(window)
            cw, ch = win.client_size(window)
            # Just past the client's right edge: the frame's resize border.
            x, y = cw + 3, ch // 2
            win.move(window, x, y)
            time.sleep(0.2)
            button = lambda flag: win.INPUT(win.INPUT_MOUSE, win._INPUTUNION(mi=win.MOUSEINPUT(0, 0, 0, flag, 0, 0)))
            win._send(button(win.MOUSEEVENTF_LEFTDOWN))
            time.sleep(0.2)
            results = []
            try:
                for step in range(args.steps):
                    x += 30 if step < args.steps // 2 else -30
                    before = win.client_size(window)
                    changed = time.perf_counter()
                    win.move(window, x, y)
                    while win.client_size(window) == before and time.perf_counter() - changed < 0.5:
                        pass
                    results.append(settle(window, scale, changed, timeout=0.5))
            finally:
                win._send(button(win.MOUSEEVENTF_LEFTUP))
            totals.append(report("live drag", results))

        win.close(window)
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()


if __name__ == "__main__":
    main()
