#!/usr/bin/env python3
"""Whether a window appears with its content, never empty: launch an app at
a fixed place (STARTUPINFO's STARTF_USEPOSITION), watch the strip of its
header (the comparison adapters draw "zgx performance lab" at 20, 20) on
screen from before the window opens (Desktop Duplication, `framepace` with
a folder), and check every composition from the first that differs from the
screen before: none may be the window without its content, its background
alone (the strip's luminance spread under `EMPTY`) where the settled window
shows text. Windows fades and scales a new window in from what was behind
it: those compositions show the text however faint, and pass.

    cargo build --release -p zgx-gpu --example framepace
    cargo build --release -p zgx-desktop --example component_workload
    python scripts/windows_first_frame_probe.py target/release/examples/component_workload.exe target/release/examples/framepace.exe

zgx's windows draw through a DirectComposition visual, with no redirection
bitmap: the compositor has nothing to show of one until its first frame is
presented, so it appears with its content. An app that shows its window
before drawing fails here. Exits 1 if any launch shows an empty frame, or
is covered.
"""
import argparse
import ctypes
import os
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

import windows_compare as compare
import windows_input as win

# Where the window opens, and the header strip (logical pixels in its client area).
X, Y = 300, 150
STRIP = (20, 20, 240, 28)
# A composition is the window's background alone when its luminance's
# spread over the strip is under this (0-255); the settled header's text
# spreads it well over (about 40 for the lab scene).
EMPTY = 2.0


class STARTUPINFOW(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("lpReserved", wintypes.LPWSTR), ("lpDesktop", wintypes.LPWSTR),
                ("lpTitle", wintypes.LPWSTR), ("dwX", wintypes.DWORD), ("dwY", wintypes.DWORD),
                ("dwXSize", wintypes.DWORD), ("dwYSize", wintypes.DWORD), ("dwXCountChars", wintypes.DWORD),
                ("dwYCountChars", wintypes.DWORD), ("dwFillAttribute", wintypes.DWORD), ("dwFlags", wintypes.DWORD),
                ("wShowWindow", wintypes.WORD), ("cbReserved2", wintypes.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", wintypes.HANDLE), ("hStdOutput", wintypes.HANDLE), ("hStdError", wintypes.HANDLE)]


class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [("hProcess", wintypes.HANDLE), ("hThread", wintypes.HANDLE), ("dwProcessId", wintypes.DWORD),
                ("dwThreadId", wintypes.DWORD)]


class Launched:
    """A process started with CreateProcessW at (X, Y), as `win.wait_window` takes it."""

    def __init__(self, exe, env):
        info = STARTUPINFOW(cb=ctypes.sizeof(STARTUPINFOW), dwX=X, dwY=Y, dwFlags=0x4)  # STARTF_USEPOSITION
        process = PROCESS_INFORMATION()
        block = "".join(f"{k}={v}\0" for k, v in env.items()) + "\0"
        flags = 0x400 | 0x08000000  # CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW
        if not ctypes.windll.kernel32.CreateProcessW(str(exe), None, None, None, False, flags,
                                                     ctypes.c_wchar_p(block), None, ctypes.byref(info),
                                                     ctypes.byref(process)):
            raise OSError(ctypes.GetLastError(), f"CreateProcessW {exe}")
        ctypes.windll.kernel32.CloseHandle(process.hThread)
        self.pid, self.handle, self.returncode = process.dwProcessId, process.hProcess, None

    def poll(self):
        return None

    def wait(self, ms=15000):
        ctypes.windll.kernel32.WaitForSingleObject(self.handle, ms)
        ctypes.windll.kernel32.CloseHandle(self.handle)


def strip_of(hwnd):
    scale = win.dpi(hwnd) / 96
    x, y = win.to_screen(hwnd, round(STRIP[0] * scale), round(STRIP[1] * scale))
    return x, y, round(STRIP[2] * scale), round(STRIP[3] * scale)


def spread(bgra):
    """The standard deviation of the strip's luminance (0-255)."""
    values = [bgra[i] * 0.114 + bgra[i + 1] * 0.587 + bgra[i + 2] * 0.299 for i in range(0, len(bgra), 4)]
    mean = sum(values) / len(values)
    return (sum((v - mean) ** 2 for v in values) / len(values)) ** 0.5


def launch_once(exe, framepace, env, strip, folder):
    """The strip's compositions for one launch, [(ms, pixels)], and whether
    the window was uncovered once it had settled."""
    for old in folder.glob("*.bgra"):
        old.unlink()
    capture = subprocess.Popen([str(framepace), *map(str, strip), "3", str(folder)], stdout=subprocess.PIPE, text=True)
    time.sleep(0.5)  # the capture's device made and duplicating
    app = Launched(exe, env)
    try:
        hwnd = win.wait_window(app, timeout=5)
        out, _ = capture.communicate()
        # Still in front and uncovered as the capture ends (the app outlives it).
        uncovered = compare.uncovered(hwnd)
    finally:
        app.wait()
    saved = [line.split() for line in out.splitlines() if line.startswith("SAVED")]
    return [(float(ms), (folder / f"{n}.bgra").read_bytes()) for _, n, ms in saved], uncovered


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("app", type=Path)
    parser.add_argument("framepace", type=Path)
    parser.add_argument("--launches", type=int, default=5)
    args = parser.parse_args()
    # The app outlives the capture (3 s after it starts).
    env = dict(os.environ, ZGX_MODE="idle", ZGX_SECONDS="6", ZGX_INITIAL_TICKS="0")
    failed = 0
    with compare.Minimized(["Zeron"]), tempfile.TemporaryDirectory() as folder:
        folder = Path(folder)
        calibration = Launched(args.app, env)
        try:
            strip = strip_of(win.wait_window(calibration, timeout=10))
        finally:
            ctypes.windll.user32.PostMessageW(win.windows_of(calibration.pid)[0], 0x0010, 0, 0)  # WM_CLOSE
            calibration.wait()
        time.sleep(1)
        for n in range(args.launches):
            frames, uncovered = launch_once(args.app, args.framepace, env, strip, folder)
            if not uncovered or len(frames) < 2:
                print(f"launch {n}: invalid ({'covered' if not uncovered else 'nothing captured'})")
                failed += 1
                continue
            spreads = [(ms, spread(f)) for ms, f in frames]
            settled = spreads[-1][1]
            # The compositions after the screen before: none the window's background alone.
            empty = [(ms, s) for ms, s in spreads[1:] if s < EMPTY] if settled > 4 * EMPTY else []
            failed += bool(empty)
            shown = " ".join(f"{s:.1f}" for _, s in spreads[:8])
            print(f"launch {n}: luminance spread, the screen before then each composition: {shown}; "
                  f"settled {settled:.1f}: " + (f"EMPTY at {empty[0][0]:.0f} ms" if empty else "ok"))
    print("PASS" if not failed else f"FAIL: {failed} of {args.launches} launches")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
