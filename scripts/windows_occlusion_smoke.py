#!/usr/bin/env python3
"""A covered window stops drawing on Windows (winit's `occlusion`), and a
window anyone can see never does. A Luau app whose state changes all the
time counts its frames (`zgx inspect frames`) while windows of this script's
own, never another app's, cover it each way:

- opaque windows leaving none of it stop it: one larger, one exactly its
  size without a frame, two meeting edge to edge, a topmost one that never
  took the focus, one with tabbed Mica, and over it maximized; with the app's
  `ZGX_RENDER_WHEN_COVERED=1` none do;
- it goes on drawing with a pixel left, at its frame's rounded corners, partly
  off the monitor, under a cover reaching past the monitor's edge, or
  under anything that may show it: layered, click-through, tool, acrylic,
  composed (`WS_EX_NOREDIRECTIONBITMAP`), shaped, cloaked and its own owned
  windows;
- it draws again however the cover went: closed, hidden, minimized, moved
  away, sent to the bottom, or turned translucent;
- minimized and restored it draws again, covered before or not;
- `--snap`: snapped beside a window (Win+Left), it goes on drawing;
- `--desktops`: on another virtual desktop it stops, and draws again back
  on its own (Win+Ctrl+D makes the desktop, Win+Ctrl+F4 removes it).

    cargo build --release -p zgx-run -p zgx-script
    python scripts/windows_occlusion_smoke.py target/release --snap --desktops

Covers are only this script's own windows, never another app's. Before a
case that presses keys, another app's window holding the front (a
notification, a prompt) is waited for up to 60 s and never touched; if it
stays, the case is reported BLOCKED and skipped, and the app is raised
without input where it needs to be in front.

Every case is reported; a failure stops nothing but fails the run. Keys go
through `windows_input.guard_windows`: a window nobody expected stops the
run with nothing sent, and only this script's covers and the app's own
windows are expected. Stopped that way on the second desktop, it says so
rather than press anything.
"""
import argparse
import ctypes
import os
import pathlib
import queue
import subprocess
import tempfile
import threading
import time
from ctypes import wintypes

import windows_input as win

APP = """
local ticks = ui.state("ticks", 0)
ui.every(40, function() ticks:update(function(n) return n + 1 end) end)
return ui.text { id = "ticks", "{ticks}" }
"""

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32")
dwmapi = ctypes.WinDLL("dwmapi")
kernel32 = ctypes.WinDLL("kernel32")

WS_POPUP = 0x80000000
WS_OVERLAPPEDWINDOW = 0x00CF0000
WS_VISIBLE = 0x10000000
WS_EX_TOPMOST = 0x8
WS_EX_TRANSPARENT = 0x20
WS_EX_TOOLWINDOW = 0x80
WS_EX_NOACTIVATE = 0x08000000
WS_EX_LAYERED = 0x80000
WS_EX_NOREDIRECTIONBITMAP = 0x200000
GWL_EXSTYLE = -20
LWA_ALPHA = 2
SWP_NOSIZE = 1
SWP_NOMOVE = 2
SWP_NOACTIVATE = 0x10
HWND_BOTTOM = 1
HWND_TOPMOST = -1
HWND_NOTOPMOST = -2
SW_HIDE = 0
SW_MAXIMIZE = 3
SW_MINIMIZE = 6
SW_RESTORE = 9
DWMWA_EXTENDED_FRAME_BOUNDS = 9
DWMWA_CLOAKED = 14
DWMWA_CLOAK = 13
DWMWA_SYSTEMBACKDROP_TYPE = 38
GW_HWNDNEXT = 2
WM_APP = 0x8000
COLOR_WINDOW = 5
COVER_CLASS = "ZgxOcclusionCover"

WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, wintypes.HWND, wintypes.UINT, wintypes.WPARAM,
                             wintypes.LPARAM)


class WNDCLASSW(ctypes.Structure):
    _fields_ = [("style", wintypes.UINT), ("lpfnWndProc", WNDPROC), ("cbClsExtra", ctypes.c_int),
                ("cbWndExtra", ctypes.c_int), ("hInstance", wintypes.HINSTANCE),
                ("hIcon", wintypes.HANDLE), ("hCursor", wintypes.HANDLE),
                ("hbrBackground", wintypes.HANDLE), ("lpszMenuName", wintypes.LPCWSTR),
                ("lpszClassName", wintypes.LPCWSTR)]


kernel32.GetModuleHandleW.restype = wintypes.HMODULE
kernel32.GetModuleHandleW.argtypes = [wintypes.LPCWSTR]
user32.GetMessageW.argtypes = [ctypes.POINTER(wintypes.MSG), wintypes.HWND, wintypes.UINT, wintypes.UINT]
user32.DefWindowProcW.restype = ctypes.c_ssize_t
user32.DefWindowProcW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.CreateWindowExW.restype = wintypes.HWND
user32.CreateWindowExW.argtypes = [wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD,
                                   ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                   wintypes.HWND, wintypes.HMENU, wintypes.HINSTANCE, wintypes.LPVOID]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                ctypes.c_int, wintypes.UINT]
user32.SetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.LONG]
user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.SetLayeredWindowAttributes.argtypes = [wintypes.HWND, wintypes.COLORREF, wintypes.BYTE, wintypes.DWORD]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetWindow.restype = wintypes.HWND
user32.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
user32.DestroyWindow.argtypes = [wintypes.HWND]
user32.SetWindowRgn.argtypes = [wintypes.HWND, wintypes.HANDLE, wintypes.BOOL]
gdi32.CreateEllipticRgn.restype = wintypes.HANDLE
dwmapi.DwmGetWindowAttribute.argtypes = [wintypes.HWND, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD]
dwmapi.DwmSetWindowAttribute.argtypes = [wintypes.HWND, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD]


class Covers:
    """Windows of this script's own, on a thread pumping their messages:
    `call` runs a function there, as window calls must."""

    def __init__(self):
        self.target = None
        self.jobs = queue.Queue()
        self.ready = threading.Event()
        threading.Thread(target=self.pump, daemon=True).start()
        self.ready.wait()

    def pump(self):
        self.thread = kernel32.GetCurrentThreadId()
        self.proc = WNDPROC(self.window_proc)
        instance = kernel32.GetModuleHandleW(None)
        kind = WNDCLASSW(lpfnWndProc=self.proc, hInstance=instance, hbrBackground=COLOR_WINDOW + 1,
                         lpszClassName=COVER_CLASS)
        user32.RegisterClassW(ctypes.byref(kind))
        self.window = user32.CreateWindowExW(0, COVER_CLASS, "pump", 0, 0, 0, 0, 0, None, None,
                                             instance, None)
        self.ready.set()
        message = wintypes.MSG()
        while user32.GetMessageW(ctypes.byref(message), None, 0, 0) > 0:
            user32.TranslateMessage(ctypes.byref(message))
            user32.DispatchMessageW(ctypes.byref(message))

    def window_proc(self, hwnd, message, wparam, lparam):
        if message == WM_APP:
            job, done = self.jobs.get()
            done["value"] = job()
            done["event"].set()
            return 0
        return user32.DefWindowProcW(hwnd, message, wparam, lparam)

    def call(self, job):
        done = {"event": threading.Event()}
        self.jobs.put((job, done))
        user32.PostMessageW(self.window, WM_APP, 0, 0)
        assert done["event"].wait(10), "the covers' thread stopped"
        return done["value"]

    def open(self, area, style=WS_POPUP, ex=0, owner=None, alpha=None):
        """A window at `area` (left, top, right, bottom), shown."""
        left, top, right, bottom = area

        def make():
            hwnd = user32.CreateWindowExW(ex, COVER_CLASS, "zgx occlusion cover",
                                          style, left, top, right - left, bottom - top, owner, None,
                                          kernel32.GetModuleHandleW(None), None)
            assert hwnd, ctypes.get_last_error()
            if alpha is not None:
                user32.SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)
            user32.ShowWindow(hwnd, 4)  # SW_SHOWNOACTIVATE
            # Shown without activating, it stays below the foreground window,
            # and `HWND_TOP` doesn't pass another process's: leaving the
            # topmost band puts it at the head of the rest.
            if not ex & WS_EX_TOPMOST:
                flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
                user32.SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags)
                user32.SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags)
            return hwnd

        hwnd = self.call(make)
        if self.target is not None and owner is None:
            if not is_above(hwnd, self.target):
                front(hwnd, "activating a cover", wait=0)
            if not is_above(hwnd, self.target):
                report("cover above the app", False, f"{hwnd:#x} is not above {self.target:#x}")
        return hwnd

    def close(self, hwnd):
        self.call(lambda: user32.DestroyWindow(hwnd))


def bounds(hwnd):
    """What a window shows: (left, top, right, bottom)."""
    rect = wintypes.RECT()
    assert dwmapi.DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, ctypes.byref(rect),
                                        ctypes.sizeof(rect)) == 0
    return rect.left, rect.top, rect.right, rect.bottom


def grown(area, by):
    return area[0] - by, area[1] - by, area[2] + by, area[3] + by


def on_screen(area):
    """A cover past the monitor's edge isn't believed: keep to it."""
    width, height = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    return max(area[0], 0), max(area[1], 0), min(area[2], width), min(area[3], height)


class Run:
    def __init__(self, tools, app_dir, shots, env_extra=None):
        env = {k: v for k, v in os.environ.items() if k != "ZGX_RENDER_WHEN_COVERED"}
        env.update(env_extra or {})
        self.zgx = str(tools / "zgx.exe")
        self.app = subprocess.Popen([str(tools / "zgx-run.exe"), "--inspect", str(app_dir)], env=env,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 60
        while not win.windows_of(self.app.pid):
            assert time.monotonic() < deadline, "no window"
            time.sleep(0.1)
        self.hwnd = win.windows_of(self.app.pid)[0]
        # Fail closed: every key below stops at a window nobody expected.
        win.guard_windows(shots, allowed=set(), app=(self.app.pid, "Occlusion"),
                          own=(os.getpid(), COVER_CLASS))
        if not front(self.hwnd, "activating the app"):
            # Nothing pressed: raised past the foreground window, unfocused.
            flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
            user32.SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags)
            user32.SetWindowPos(self.hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags)
        user32.SetWindowPos(self.hwnd, None, 300, 200, 640, 420, SWP_NOACTIVATE)
        time.sleep(1)

    def inspect(self, *args):
        result = subprocess.run([self.zgx, "inspect", "--app", str(self.app.pid), *args],
                                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30)
        assert result.returncode == 0, result.stderr
        return result.stdout

    def frames(self, seconds=1.5):
        """Frames drawn over `seconds`."""
        self.inspect("mark", "m")
        time.sleep(seconds)
        out = self.inspect("frames", "1000", "--since", "m")
        return sum(line.startswith("frame ") for line in out.splitlines())

    def stop(self):
        self.app.kill()
        self.app.wait()


SETTLE = 2.2  # Two checks half a second apart, the frames between.
AGAIN = 2.0  # A look each second, for a change no event reports.
FAILED = []
BLOCKED = []


# Shell surfaces that hold the front over apps: notifications, flyouts.
SHELL_OVERLAY = "Windows.UI.Core.CoreWindow"
# Always shown topmost, never over an app's client area of its own accord.
TASKBARS = {"Shell_TrayWnd", "Shell_SecondaryTrayWnd"}


def foreign_front(hwnd):
    """Another app's window holding the front over `hwnd`, checked before any
    key is sent (`activate`'s included): the foreground window if it is a
    shell overlay or topmost, or any other visible topmost window not this
    script's or the app's. None when clear."""
    app = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(app))
    ours = {os.getpid(), app.value}

    def describe(window):
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(pid))
        name = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(window, name, 256)
        return pid.value, name.value

    foreground = user32.GetForegroundWindow()
    if foreground:
        pid, kind = describe(foreground)
        topmost = user32.GetWindowLongW(foreground, GWL_EXSTYLE) & WS_EX_TOPMOST
        if pid not in ours and (kind == SHELL_OVERLAY or topmost):
            return f"foreground {kind!r} of pid {pid}, {win.title(foreground)!r}"
    for window, pid, kind, text, _ in win.visible_windows():
        if pid in ours or kind in TASKBARS:
            continue
        cloaked = wintypes.DWORD()
        dwmapi.DwmGetWindowAttribute(window, DWMWA_CLOAKED, ctypes.byref(cloaked), 4)
        if not cloaked.value and user32.GetWindowLongW(window, GWL_EXSTYLE) & WS_EX_TOPMOST:
            return f"topmost {kind!r} of pid {pid}, {text!r}"
    return None


def front(hwnd, what, wait=60):
    """Bring `hwnd` to the front, unless another app's window holds it (a
    notification, a prompt): then wait up to `wait` seconds for it to go,
    touching nothing, and if it stays record `what` as blocked."""
    deadline = time.monotonic() + wait
    while True:
        holder = foreign_front(hwnd)
        if holder:
            reason = holder
        else:
            try:
                win.activate(hwnd)
                return True
            except (RuntimeError, win.UnexpectedWindow) as error:
                reason = error
        if time.monotonic() >= deadline:
            BLOCKED.append(what)
            print(f"BLOCKED {what}: {reason}", flush=True)
            return False
        time.sleep(5)


def report(name, ok, detail):
    print(f"{'ok  ' if ok else 'FAIL'} {name}: {detail}", flush=True)
    if not ok:
        FAILED.append(name)


def is_above(cover, target):
    """A cover below the app covers nothing: its case would prove nothing."""
    below = cover
    while below:
        below = user32.GetWindow(below, GW_HWNDNEXT)
        if below == target:
            return True
    return False


def stops(run, name):
    time.sleep(SETTLE)
    n = run.frames()
    report(name, n <= 2, f"{n} frames covered")


def draws(run, name):
    time.sleep(SETTLE)
    n = run.frames()
    report(name, n >= 15, f"{n} frames with it there")


def back(run, name, wait=0.15):
    time.sleep(wait)
    n = run.frames(0.8)
    report(name, n >= 8, f"{n} frames in 0.8 s after {wait} s")


def leave_desktop():
    """Remove the desktop made for `--desktops`. The app isn't in front there,
    so only the window guard applies; it may refuse, sending nothing."""
    win._window_guard()
    codes = [win._code(part) for part in "win+ctrl+f4".split("+")]
    win._send(*[win._vk(c) for c in codes], *[win._vk(c, up=True) for c in reversed(codes)], guard=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tools", type=pathlib.Path, help="the directory of zgx-run.exe and zgx.exe")
    parser.add_argument("--snap", action="store_true")
    parser.add_argument("--desktops", action="store_true")
    args = parser.parse_args()
    ctypes.WinDLL("user32").SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    work = pathlib.Path(tempfile.mkdtemp(prefix="zgx-occlusion-"))
    (work / "app.luau").write_text(APP)
    (work / "zgx.json").write_text('{ "title": "Occlusion", "width": 640, "height": 420 }')
    shots = work / "stopped"
    covers = Covers()
    run = Run(args.tools, work, shots)
    covers.target = run.hwnd
    on_other_desktop = False
    try:
        n = run.frames()
        report("shown", n >= 15, f"{n} frames")
        at = bounds(run.hwnd)
        left, top, right, bottom = at
        middle = (left + right) // 2

        # Stops, and comes back as the cover closes.
        cover = covers.open(grown(at, 40))
        stops(run, "a larger window")
        covers.close(cover)
        back(run, "closed")
        cover = covers.open(at)
        stops(run, "a borderless window its size")
        covers.close(cover)
        back(run, "closed again")
        one, two = covers.open((left, top, middle, bottom)), covers.open((middle, top, right, bottom))
        stops(run, "two meeting edge to edge")
        covers.close(one)
        back(run, "one of two closed")
        covers.close(two)
        cover = covers.open(grown(at, 40), ex=WS_EX_TOPMOST | WS_EX_NOACTIVATE)
        stops(run, "topmost, never focused")
        covers.close(cover)
        back(run, "topmost closed")

        # Comes back however the cover goes.
        cover = covers.open(grown(at, 40))
        stops(run, "(to hide)")
        covers.call(lambda: user32.ShowWindow(cover, SW_HIDE))
        back(run, "hidden")
        covers.close(cover)
        cover = covers.open(grown(at, 40), style=WS_OVERLAPPEDWINDOW)
        stops(run, "(to minimize)")
        covers.call(lambda: user32.ShowWindow(cover, SW_MINIMIZE))
        back(run, "minimized")
        covers.close(cover)
        cover = covers.open(grown(at, 40))
        stops(run, "(to move)")
        covers.call(lambda: user32.SetWindowPos(cover, None, right + 100, top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE))
        back(run, "moved away")
        covers.close(cover)
        cover = covers.open(grown(at, 40))
        stops(run, "(to send back)")
        covers.call(lambda: user32.SetWindowPos(cover, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE))
        back(run, "sent to the bottom", AGAIN)
        covers.close(cover)

        def translucent(hwnd):
            user32.SetWindowLongW(hwnd, GWL_EXSTYLE, user32.GetWindowLongW(hwnd, GWL_EXSTYLE) | WS_EX_LAYERED)
            user32.SetLayeredWindowAttributes(hwnd, 0, 200, LWA_ALPHA)

        cover = covers.open(grown(at, 40))
        stops(run, "(to turn translucent)")
        covers.call(lambda: translucent(cover))
        back(run, "turned translucent", AGAIN)
        covers.close(cover)

        # Never stops.
        cases = [
            ("a pixel left", (left, top, right - 1, bottom), {}),
            ("a pixel left along the top", (left, top + 1, right, bottom), {}),
            ("a cover past the monitor's edge", (-40, top - 40, right + 40, bottom + 40), {}),
            ("layered", grown(at, 40), {"ex": WS_EX_LAYERED, "alpha": 200}),
            ("click-through", grown(at, 40), {"ex": WS_EX_LAYERED | WS_EX_TRANSPARENT, "alpha": 255}),
            ("tool window", grown(at, 40), {"ex": WS_EX_TOOLWINDOW}),
            ("composed", grown(at, 40), {"ex": WS_EX_NOREDIRECTIONBITMAP}),
            ("its own owned window", grown(at, 40), {"owner": run.hwnd}),
        ]
        for name, area, options in cases:
            cover = covers.open(area, **options)
            draws(run, name)
            covers.close(cover)

        one, two = covers.open((left, top, middle, bottom)), covers.open((middle + 1, top, right, bottom))
        draws(run, "two with a pixel between")
        covers.close(one)
        covers.close(two)

        cover = covers.open(grown(at, 40))
        ellipse = gdi32.CreateEllipticRgn(0, 0, right - left + 80, bottom - top + 80)
        covers.call(lambda: user32.SetWindowRgn(cover, ellipse, True))
        draws(run, "shaped")
        covers.close(cover)

        cover = covers.open(grown(at, 40))
        cloak = wintypes.BOOL(True)
        covers.call(lambda: dwmapi.DwmSetWindowAttribute(cover, DWMWA_CLOAK, ctypes.byref(cloak), 4))
        draws(run, "cloaked")
        covers.close(cover)

        # Partly off the monitor: covered or not, it can't be sure, and draws.
        user32.SetWindowPos(run.hwnd, None, -60, 200, 640, 420, SWP_NOACTIVATE)
        cover = covers.open((0, 0, 1000, 800))
        draws(run, "partly off the monitor")
        covers.close(cover)
        user32.SetWindowPos(run.hwnd, None, 300, 200, 640, 420, SWP_NOACTIVATE)
        time.sleep(0.5)

        # A backdrop: acrylic shows what's behind it, tabbed Mica the wallpaper.
        def backdrop(hwnd, kind):
            value = wintypes.INT(kind)
            assert dwmapi.DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, ctypes.byref(value), 4) == 0

        cover = covers.open(grown(at, 40), style=WS_OVERLAPPEDWINDOW)
        covers.call(lambda: backdrop(cover, 3))
        draws(run, "an acrylic backdrop")
        covers.close(cover)
        cover = covers.open(grown(at, 40), style=WS_OVERLAPPEDWINDOW)
        covers.call(lambda: backdrop(cover, 4))
        stops(run, "tabbed Mica")
        covers.close(cover)
        back(run, "tabbed Mica closed")

        # A framed window exactly its size: the frame's corners stay.
        framed = covers.open(at, style=WS_OVERLAPPEDWINDOW)
        have = bounds(framed)
        wanted = (2 * left - have[0], 2 * top - have[1], 2 * right - have[2], 2 * bottom - have[3])
        covers.call(lambda: user32.SetWindowPos(framed, None, wanted[0], wanted[1], wanted[2] - wanted[0],
                                                wanted[3] - wanted[1], SWP_NOACTIVATE))
        report("framed cover matches", bounds(framed) == at, f"{bounds(framed)} against {at}")
        draws(run, "a framed window its size")
        covers.close(framed)

        # Minimized and restored.
        user32.ShowWindow(run.hwnd, SW_MINIMIZE)
        time.sleep(0.5)
        n = run.frames()
        report("minimized", n <= 2, f"{n} frames")
        user32.ShowWindow(run.hwnd, SW_RESTORE)
        back(run, "restored", 0.3)
        cover = covers.open(grown(at, 40))
        stops(run, "(to minimize under it)")
        user32.ShowWindow(run.hwnd, SW_MINIMIZE)
        time.sleep(1.5)
        covers.close(cover)
        user32.ShowWindow(run.hwnd, SW_RESTORE)
        back(run, "restored after a cover", 0.3)

        # Maximized.
        user32.ShowWindow(run.hwnd, SW_MAXIMIZE)
        time.sleep(1)
        area = bounds(run.hwnd)
        strip = covers.open((area[0], area[1], area[2], area[3] - 1))
        draws(run, "maximized, a row left")
        covers.close(strip)
        cover = covers.open(area)
        stops(run, "maximized under its size")
        covers.close(cover)
        back(run, "maximized, closed")
        user32.ShowWindow(run.hwnd, SW_RESTORE)
        time.sleep(0.5)

        if args.snap and front(run.hwnd, "snap"):
            win.key("win+left")
            time.sleep(2)
            area = bounds(run.hwnd)
            other = covers.open((area[2], area[1], area[2] + (area[2] - area[0]), area[3]),
                                style=WS_OVERLAPPEDWINDOW)
            draws(run, f"snapped at {area}")
            covers.close(other)
            cover = covers.open(on_screen(grown(area, 40)))
            stops(run, "snapped, covered")
            covers.close(cover)
            back(run, "snapped, closed")
            user32.SetWindowPos(run.hwnd, None, 300, 200, 640, 420, SWP_NOACTIVATE)
            time.sleep(0.5)

        if args.desktops and front(run.hwnd, "virtual desktops"):
            win.key("win+ctrl+d")
            on_other_desktop = True
            stops(run, "on another desktop")
            leave_desktop()
            on_other_desktop = False
            back(run, "its desktop back", 0.5)

        run.stop()
        run = Run(args.tools, work, shots, {"ZGX_RENDER_WHEN_COVERED": "1"})
        covers.target = run.hwnd
        at = bounds(run.hwnd)
        cover = covers.open(grown(at, 40))
        draws(run, "ZGX_RENDER_WHEN_COVERED=1, covered")
        covers.close(cover)
    finally:
        try:
            if on_other_desktop:
                leave_desktop()
        except win.UnexpectedWindow as error:
            print(f"LEFT BEHIND: a second virtual desktop, not removed: {error}", flush=True)
            raise
        finally:
            run.stop()
    if BLOCKED:
        print(f"BLOCKED, not run: {', '.join(BLOCKED)}")
    if FAILED:
        raise SystemExit(f"FAIL: {', '.join(FAILED)}")
    print("PASS: covered it stops, seen it never does, and it comes back")


if __name__ == "__main__":
    main()
