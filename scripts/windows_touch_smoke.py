#!/usr/bin/env python3
"""Touch and pen on Windows, through synthetic touch (InjectTouchInput) and a
synthetic pen (InjectSyntheticPointerInput): no touchscreen needed. Windows
gives touch and pen to the window as pointer messages, not a mouse's, so
this checks what zgx makes of them:

- a tap clicks (the gallery's Increment);
- a finger dragged along a slider drags it;
- a finger dragged over a list pans it, and a flick coasts on after the
  lift and comes to rest;
- a finger held, then lifted, opens the context menu (an editor's);
- a finger held on selectable text selects the word, a drag extends it by
  words, and the lift opens the selection's menu; held on a button in the
  text, it selects and does not press it;
- a pen is a mouse: its stroke over the list does not pan it.

    cargo build -p zgx-desktop --example gallery --example workbench --example form --example selectable_text
    python scripts/windows_touch_smoke.py target/debug/examples
"""
import argparse
import ctypes
import pathlib
import subprocess
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.CreateSyntheticPointerDevice.restype = wintypes.HANDLE
user32.InjectSyntheticPointerInput.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.UINT]

FLAG_INRANGE, FLAG_INCONTACT = 0x2, 0x4
FLAG_DOWN, FLAG_UPDATE, FLAG_UP = 0x10000, 0x20000, 0x40000
PT_TOUCH, PT_PEN = 2, 3
PHASE = {"down": FLAG_DOWN | FLAG_INRANGE | FLAG_INCONTACT,
         "move": FLAG_UPDATE | FLAG_INRANGE | FLAG_INCONTACT,
         "up": FLAG_UP}


class POINTER_INFO(ctypes.Structure):
    _fields_ = [("pointerType", wintypes.DWORD), ("pointerId", wintypes.UINT), ("frameId", wintypes.UINT),
                ("pointerFlags", wintypes.DWORD), ("sourceDevice", wintypes.HANDLE),
                ("hwndTarget", wintypes.HWND), ("ptPixelLocation", wintypes.POINT),
                ("ptHimetricLocation", wintypes.POINT), ("ptPixelLocationRaw", wintypes.POINT),
                ("ptHimetricLocationRaw", wintypes.POINT), ("dwTime", wintypes.DWORD),
                ("historyCount", wintypes.UINT), ("InputData", ctypes.c_int32),
                ("dwKeyStates", wintypes.DWORD), ("PerformanceCount", ctypes.c_uint64),
                ("ButtonChangeType", ctypes.c_int)]


class POINTER_TOUCH_INFO(ctypes.Structure):
    _fields_ = [("pointerInfo", POINTER_INFO), ("touchFlags", wintypes.DWORD), ("touchMask", wintypes.DWORD),
                ("rcContact", wintypes.RECT), ("rcContactRaw", wintypes.RECT), ("orientation", wintypes.UINT),
                ("pressure", wintypes.UINT)]


class POINTER_PEN_INFO(ctypes.Structure):
    _fields_ = [("pointerInfo", POINTER_INFO), ("penFlags", wintypes.DWORD), ("penMask", wintypes.DWORD),
                ("pressure", wintypes.UINT), ("rotation", wintypes.UINT), ("tiltX", ctypes.c_int32),
                ("tiltY", ctypes.c_int32)]


class POINTER_TYPE_INFO(ctypes.Structure):
    class _Info(ctypes.Union):
        _fields_ = [("touch", POINTER_TOUCH_INFO), ("pen", POINTER_PEN_INFO)]
    _fields_ = [("type", wintypes.DWORD), ("info", _Info)]


def touch(window, contacts):
    """One frame of contacts: [(id, client x, client y, "down" | "move" | "up")]."""
    win.require_foreground(window)
    frame = (POINTER_TOUCH_INFO * len(contacts))()
    for slot, (pointer, x, y, phase) in zip(frame, contacts):
        sx, sy = win.to_screen(window, x, y)
        slot.pointerInfo.pointerType = PT_TOUCH
        slot.pointerInfo.pointerId = pointer
        slot.pointerInfo.pointerFlags = PHASE[phase]
        slot.pointerInfo.ptPixelLocation = wintypes.POINT(sx, sy)
        slot.touchMask = 0x7  # contact area, orientation, pressure
        slot.rcContact = wintypes.RECT(sx - 2, sy - 2, sx + 2, sy + 2)
        slot.orientation, slot.pressure = 90, 32000
    if not user32.InjectTouchInput(len(contacts), frame):
        raise ctypes.WinError(ctypes.get_last_error())


def slide(window, start, end, steps, delay, inject=None):
    inject = inject or (lambda x, y, phase: touch(window, [(0, x, y, phase)]))
    inject(*start, "down")
    for step in range(1, steps + 1):
        time.sleep(delay)
        inject(start[0] + (end[0] - start[0]) * step // steps, start[1] + (end[1] - start[1]) * step // steps, "move")
    time.sleep(delay)
    inject(*end, "up")


def hold(window, x, y, seconds):
    touch(window, [(0, x, y, "down")])
    # Injected contacts time out unless they are updated.
    for _ in range(int(seconds / 0.05)):
        time.sleep(0.05)
        touch(window, [(0, x, y, "move")])
    touch(window, [(0, x, y, "up")])


def region(capture, left, top, right, bottom):
    """A sample of a capture's pixels in client rectangle (left, top)-(right, bottom)."""
    return [win.pixel(capture, x, y) for y in range(top, bottom, 3) for x in range(left, right, 3)]


def launch(examples, name):
    proc = subprocess.Popen([str(examples / f"{name}.exe")], stdout=subprocess.DEVNULL)
    window = win.wait_window(proc)
    time.sleep(1.5)
    win.activate(window)
    return proc, window


def finish(proc, window):
    win.close(window)
    try:
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()


def check(passed, what):
    if not passed:
        raise AssertionError(what)
    print(f"ok: {what}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", type=pathlib.Path)
    examples = parser.parse_args().examples
    if not user32.InitializeTouchInjection(10, 1):
        raise ctypes.WinError(ctypes.get_last_error())

    proc, window = launch(examples, "gallery")
    try:
        before = win.capture(window)
        touch(window, [(0, 88, 128, "down")])
        time.sleep(0.05)
        touch(window, [(0, 88, 128, "up")])
        time.sleep(0.5)
        check(region(before, 24, 160, 300, 190) != region(win.capture(window), 24, 160, 300, 190),
              "a tap clicks the gallery's Increment")
        before = win.capture(window)
        slide(window, (190, 313), (330, 313), 12, 0.016)
        time.sleep(0.4)
        check(region(before, 24, 296, 444, 332) != region(win.capture(window), 24, 296, 444, 332),
              "a finger along the slider drags it")
    finally:
        finish(proc, window)

    proc, window = launch(examples, "workbench")
    try:
        library = (44, 190, 290, 300)
        before = win.capture(window)
        slide(window, (160, 700), (160, 400), 20, 0.016)
        time.sleep(0.8)
        panned = win.capture(window)
        check(region(before, *library) != region(panned, *library), "a finger pans the library")
        slide(window, (160, 700), (160, 500), 5, 0.008)
        lifted = win.capture(window)
        time.sleep(0.4)
        coasted = win.capture(window)
        check(region(lifted, *library) != region(coasted, *library), "a flick coasts on after the lift")
        time.sleep(1.5)
        still = win.capture(window)
        time.sleep(0.3)
        check(region(still, *library) == region(win.capture(window), *library), "and comes to rest")
        before = win.capture(window)
        device = user32.CreateSyntheticPointerDevice(PT_PEN, 1, 1)
        if not device:
            raise ctypes.WinError(ctypes.get_last_error())

        def pen(x, y, phase):
            win.require_foreground(window)
            info = POINTER_TYPE_INFO()
            info.type = PT_PEN
            sx, sy = win.to_screen(window, x, y)
            pointer = info.info.pen.pointerInfo
            pointer.pointerType, pointer.pointerId = PT_PEN, 1
            pointer.pointerFlags = PHASE[phase] | (FLAG_INRANGE if phase == "up" else 0)
            pointer.ptPixelLocation = wintypes.POINT(sx, sy)
            info.info.pen.penMask, info.info.pen.pressure = 1, 512
            if not user32.InjectSyntheticPointerInput(device, ctypes.byref(info), 1):
                raise ctypes.WinError(ctypes.get_last_error())

        slide(window, (160, 700), (160, 400), 15, 0.016, inject=pen)
        time.sleep(0.6)
        check(region(before, *library) == region(win.capture(window), *library),
              "a pen's stroke over the list does not pan it")
    finally:
        finish(proc, window)

    proc, window = launch(examples, "form")
    try:
        below = (300, 330, 460, 420)
        before = win.capture(window)
        hold(window, 230, 240, 0.8)
        time.sleep(0.6)
        check(region(before, *below) != region(win.capture(window), *below),
              "a finger held on the notes editor opens its context menu")
        win.key("escape")
    finally:
        finish(proc, window)

    proc, window = launch(examples, "selectable_text")
    try:
        before = win.capture(window)
        touch(window, [(0, 45, 86, "down")])  # "Drag", the first word
        for _ in range(16):
            time.sleep(0.05)
            touch(window, [(0, 45, 86, "move")])
        for step in range(1, 13):
            time.sleep(0.02)
            touch(window, [(0, 45 + 12 * step, 86, "move")])
        touch(window, [(0, 189, 86, "up")])
        time.sleep(0.6)
        check(region(before, 200, 110, 420, 190) != region(win.capture(window), 200, 110, 420, 190),
              "a finger held on text, dragged and lifted opens the selection's menu")
        win.key("escape")
        time.sleep(0.2)
        win.key("ctrl+c")
        time.sleep(0.3)
        copied = subprocess.run(["powershell", "-NoProfile", "-Command", "Get-Clipboard"],
                                capture_output=True, text=True).stdout.strip()
        check(copied == "Drag across this paragraph",
              f"the hold selected the word and the drag extended it by words ({copied!r})")
        # Held on the "Press me" button in the text: its word, and no click.
        win.key("escape")
        before = win.capture(window)
        hold(window, 55, 240, 0.8)
        time.sleep(0.6)
        win.key("escape")
        time.sleep(0.2)
        win.key("ctrl+c")
        time.sleep(0.3)
        copied = subprocess.run(["powershell", "-NoProfile", "-Command", "Get-Clipboard"],
                                capture_output=True, text=True).stdout.strip()
        check(copied == "Press" and region(before, 123, 230, 230, 250) == region(win.capture(window), 123, 230, 230, 250),
              "a finger held on a button in the text selects its word and does not press it")
    finally:
        finish(proc, window)
    print("touch smoke passed")


if __name__ == "__main__":
    main()
