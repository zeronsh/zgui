#!/usr/bin/env python3
"""Drags out of zgx on Windows (`native_drag/win32.rs`): `drag_probe`'s chip
(exporting its text) dragged by real input out of its window onto
`drop_target_probe`, a test-owned window in another process registered as an
OLE drop target, placed against the probe's right edge. It never drops on
anything else: before letting go it checks that the window under the
pointer is the test target, and cancels with Escape if not.

Checks that:

- the drop target receives the chip's text, and the source hears its drag
  accepted (`DRAG_END accepted=true`);
- dragged out and back into its own window and let go there, the drag stays
  zgx's: Windows drops nothing, and the source hears it not accepted;
- Escape over the target cancels: nothing is dropped, not accepted;
- the target's description of the drop (`CFSTR_DROPDESCRIPTION`, "Copy to
  …", as Explorer gives) shows under the drag's image: Windows draws it, in
  a white label, only for a source that allows it.

It takes the pointer and keyboard while it runs.

    cargo build -p zgx-desktop --example drag_probe --example drop_target_probe
    python scripts/windows_drag_smoke.py target/debug/examples
"""
import argparse
import ctypes
import os
import pathlib
import re
import subprocess
import threading
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.WindowFromPoint.restype = wintypes.HWND
user32.WindowFromPoint.argtypes = [wintypes.POINT]
user32.GetAncestor.restype = wintypes.HWND
user32.FindWindowW.restype = wintypes.HWND


def reader(proc):
    lines = []
    threading.Thread(target=lambda: lines.extend(line.strip() for line in proc.stdout), daemon=True).start()
    return lines


def wait_for(check, what, timeout=5.0):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.02)


def button(flag):
    return win.INPUT(win.INPUT_MOUSE, win._INPUTUNION(mi=win.MOUSEINPUT(0, 0, 0, flag, 0, 0)))


def drag(window, start, path, release_on=None, hold=None):
    """Press at `start` (client points of `window`), move through `path`
    (screen pixels), then let go if the window under the pointer is
    `release_on`'s (else Escape cancels first). Returns whether it let go
    over `release_on`. `hold`: a list to put the span the drag rested at
    the end of its path in (perf_counter seconds), for a second, and how
    many white pixels Windows' drag image window then showed (a drop
    description's label)."""
    win.move(window, *start)
    time.sleep(0.15)
    win._send(button(win.MOUSEEVENTF_LEFTDOWN))
    time.sleep(0.1)
    for x, y in path:
        user32.SetCursorPos(x, y)
        time.sleep(0.02)
    time.sleep(0.3)
    if hold is not None:
        began = time.perf_counter()
        time.sleep(0.5)
        # What shows at the pointer: Windows' drag image (the preview's
        # last frame), over the target.
        point = wintypes.POINT()
        user32.GetCursorPos(ctypes.byref(point))
        pathlib.Path("target/windows-drag").mkdir(parents=True, exist_ok=True)
        win.save_png(win.capture_rect(point.x - 150, point.y - 60, 300, 120),
                     pathlib.Path("target/windows-drag/drag-image.png"))
        white = 0
        image = user32.FindWindowW("SysDragImage", None)
        if image:
            rect = wintypes.RECT()
            user32.GetWindowRect(image, ctypes.byref(rect))
            _, _, data = win.capture_rect(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
            white = sum(1 for i in range(0, len(data), 4) if min(data[i:i + 3]) >= 245)
        time.sleep(0.5)
        hold.append((began, time.perf_counter()))
        hold.append(white)
    point = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(point))
    under = window_at(point.x, point.y)
    safe = release_on is not None and under == release_on
    if not safe and release_on is not None:
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(under, ctypes.byref(pid))
        kernel32 = ctypes.WinDLL("kernel32")
        kernel32.OpenProcess.restype = wintypes.HANDLE
        handle = kernel32.OpenProcess(0x1000, False, pid.value)  # PROCESS_QUERY_LIMITED_INFORMATION
        path = ctypes.create_unicode_buffer(512)
        size = wintypes.DWORD(512)
        if handle:
            kernel32.QueryFullProcessImageNameW(handle, 0, path, ctypes.byref(size))
            kernel32.CloseHandle(handle)
        print(f"another window was over the target as it let go: {win.title(under)!r} "
              f"(process {pid.value}, {path.value}); cancelled")
    if not safe and release_on is not None:
        win._send(win._vk(0x1B), win._vk(0x1B, up=True), guard=False)  # Escape: nothing drops
        time.sleep(0.2)
    win._send(button(win.MOUSEEVENTF_LEFTUP), guard=False)
    time.sleep(0.6)
    return safe


def presents(log, span):
    """Presents DXGI reports shown within `span` (perf_counter seconds, the
    present log's clock), from `ZGX_PRESENT_LOG` lines."""
    began, ended = span
    total = 0
    for line in list(log):
        match = re.match(r"PRESENT (\d+\.\d+) dxgi shown .* \+(\d+) presents", line)
        if match and began <= float(match.group(1)) <= ended:
            total += int(match.group(2))
    return total


def window_at(x, y):
    """The top-level window at screen point (x, y)."""
    return user32.GetAncestor(user32.WindowFromPoint(wintypes.POINT(x, y)), 2)  # GA_ROOT


def only_ours(path, windows):
    """Whether every point of `path` is over one of `windows` (the probe's
    and the test target's): a drag along it reaches no other app."""
    return all(window_at(x, y) in windows for x, y in path)


def steps(a, b, n=20):
    return [(round(a[0] + (b[0] - a[0]) * i / n), round(a[1] + (b[1] - a[1]) * i / n)) for i in range(1, n + 1)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", type=pathlib.Path)
    args = parser.parse_args()
    # Its bar moves every display frame; its presents are logged.
    env = dict(os.environ, DRAG_PROBE_ANIMATE="1", ZGX_PRESENT_LOG="1")
    source = subprocess.Popen([str(args.examples / "drag_probe.exe")], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True, env=env)
    source_lines = reader(source)
    log = []
    threading.Thread(target=lambda: log.extend(line.strip() for line in source.stderr), daemon=True).start()
    target = None
    try:
        # Its two windows: the source ("Drag"), and "Other" to its right.
        wait_for(lambda: len(win.windows_of(source.pid)) >= 2, "the probe's two windows", 15)
        time.sleep(1.0)
        window = next(w for w in win.windows_of(source.pid) if win.title(w) == "Drag")
        rect = wintypes.RECT()
        user32.GetWindowRect(window, ctypes.byref(rect))
        # The target goes below the source, clear of "Other".
        target = subprocess.Popen([str(args.examples / "drop_target_probe.exe"), f"--at={rect.left},{rect.bottom}"],
                                  stdout=subprocess.PIPE, text=True)
        target_lines = reader(target)
        wait_for(lambda: any(line.startswith("TARGET") for line in target_lines), "the drop target")
        target_window = win.wait_window(target)
        _, tx, ty, tw, th = next(line for line in target_lines if line.startswith("TARGET")).split()
        inside = (int(tx) + int(tw) // 2, int(ty) + int(th) // 2)
        scale = win.dpi(window) / 96
        chip = (70 * scale, 24 * scale)  # the chip's middle, 10 points in
        start = win.to_screen(window, *chip)
        back = win.to_screen(window, 300 * scale, 150 * scale)

        def front():
            """Both in front of every other window (the target, then the
            probe), and nothing else on the drags' way: else no drag."""
            win.activate(target_window)
            win.activate(window)
            time.sleep(0.3)
            if not only_ours(steps(start, inside) + steps(inside, back), (window, target_window)):
                raise SystemExit("another window lies between the probe and its target: not dragging")

        front()

        # At rest, animating: its presents a second.
        began = time.perf_counter()
        time.sleep(1.0)
        resting = presents(log, (began, time.perf_counter()))

        # Out onto the test target, held there a second (the window still
        # animating inside Windows' drag loop), let go there: it takes the
        # text.
        held = []
        front()
        let_go = drag(window, chip, steps(start, inside), release_on=target_window, hold=held)
        assert let_go, "the pointer was not over the test target: cancelled"
        time.sleep(0.3)
        dragging = presents(log, held[0])
        print(f"ok: {dragging} frames shown in the second the drag was Windows' "
              f"({resting} at rest, animating)")
        assert dragging >= 0.8 * resting and dragging > 20, "the window stopped animating during the drag"
        wait_for(lambda: any(line.startswith("DROPPED") for line in target_lines), "the drop")
        dropped = next(line for line in target_lines if line.startswith("DROPPED"))
        assert "text=Some(\"Some text\")" in dropped, dropped
        wait_for(lambda: "DRAG_END accepted=true" in source_lines, "the source to hear it accepted")
        print(f"ok: dragged out onto another app's window, which took the text ({dropped})")
        assert "DESCRIBED" in target_lines, "the target did not describe the drop"
        assert held[1] > 300 * scale * scale, f"no drop description under the drag image ({held[1]} white pixels)"
        print(f"ok: the target's drop description shows under the drag image ({held[1]} white label pixels)")

        # Out and back in, let go in its own window: zgx's, not Windows'.
        front()
        drops = sum(line.startswith("DROPPED") for line in target_lines)
        drag(window, chip, steps(start, inside, 10) + steps(inside, back, 10), release_on=window)
        wait_for(lambda: source_lines.count("DRAG_END accepted=false") >= 1, "the drag to end in its window")
        assert sum(line.startswith("DROPPED") for line in target_lines) == drops, "Windows dropped it"
        print("ok: dragged out and back, let go in its own window: Windows dropped nothing")

        # Escape over the target: cancelled, nothing dropped.
        front()
        user32.SetCursorPos(*start)
        win.move(window, *chip)
        time.sleep(0.15)
        win._send(button(win.MOUSEEVENTF_LEFTDOWN))
        for x, y in steps(start, inside):
            user32.SetCursorPos(x, y)
            time.sleep(0.02)
        time.sleep(0.3)
        win._send(win._vk(0x1B), win._vk(0x1B, up=True), guard=False)
        # The preview, shown where the drag was let go, slides back to the
        # chip in a window of its own, then goes.
        slid = []
        began = time.monotonic()
        while time.monotonic() - began < 1.0:
            for popup in win.popups_of(source.pid):
                rect = wintypes.RECT()
                user32.GetWindowRect(popup, ctypes.byref(rect))
                slid.append((rect.left, rect.top))
            time.sleep(0.01)
        win._send(button(win.MOUSEEVENTF_LEFTUP), guard=False)
        wait_for(lambda: source_lines.count("DRAG_END accepted=false") >= 2, "the cancelled drag to end")
        assert sum(line.startswith("DROPPED") for line in target_lines) == drops, "Escape still dropped"
        print("ok: Escape over the target cancels the drag; nothing dropped")
        chip_screen = win.to_screen(window, 10 * scale, 10 * scale)
        assert slid, "no preview window showed after the cancel"
        first, last = slid[0], slid[-1]
        assert abs(last[0] - chip_screen[0]) < 20 * scale and abs(last[1] - chip_screen[1]) < 20 * scale, (
            f"the preview did not slide back to the chip: {first} -> {last}, chip at {chip_screen}")
        assert not win.popups_of(source.pid), "the preview window stayed"
        print(f"ok: the preview slid back from {first} to {last} (the chip at {chip_screen}) in "
              f"{len(set(slid))} steps, then went")

        # A tab carries nothing to other apps: it stays zgx's, between the
        # app's windows (the source holds the pointer, so the gap between
        # them reaches no other app). Dropped onto the other window's strip,
        # it lands there.
        other = next(w for w in win.windows_of(source.pid) if win.title(w) == "Other")
        win.activate(other)
        front()
        tab = (70 * scale, 62 * scale)
        tab_start = win.to_screen(window, *tab)
        width, _ = win.client_size(other)
        strip = win.to_screen(other, width // 2, 40 * scale)
        if window_at(*strip) != other:
            raise SystemExit("another window covers the other window's strip: not dragging")
        drag(window, tab, steps(tab_start, strip))
        wait_for(lambda: "DROPPED tab" in source_lines, "the tab to drop on the other window's strip")
        print("ok: a tab dragged onto the other window's strip drops there")

        # Let go outside both windows (over the test target), it tears off
        # (`on_drag_outside`, where an app opens it a window of its own).
        front()
        drag(window, tab, steps(tab_start, inside))
        wait_for(lambda: "TORN tab" in source_lines, "the tab to tear off")
        print("ok: a tab let go outside the app's windows tears off")
        print("drag smoke passed")
    finally:
        for proc in (source, target):
            if proc and proc.poll() is None:
                proc.kill()


if __name__ == "__main__":
    main()
