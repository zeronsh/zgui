#!/usr/bin/env python3
"""Validate stationary-pointer hit testing across a live X11 XSETTINGS Xft/DPI change."""
import argparse
import ctypes
import struct
import hashlib
import json
import os
import pathlib
import re
import select
import subprocess
import time
from platform_smoke import stop, wait_for_wm


class XSettings:
    """Minimal owned XSETTINGS provider; native winit subscribes to this owner."""
    def __init__(self, display):
        self.x = ctypes.CDLL("libX11.so.6")
        signatures = {
            "XOpenDisplay": ([ctypes.c_char_p], ctypes.c_void_p),
            "XDefaultRootWindow": ([ctypes.c_void_p], ctypes.c_ulong),
            "XCreateSimpleWindow": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong], ctypes.c_ulong),
            "XInternAtom": ([ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong),
            "XSetSelectionOwner": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong], ctypes.c_int),
            "XChangeProperty": ([ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.POINTER(ctypes.c_ubyte), ctypes.c_int], ctypes.c_int),
            "XFlush": ([ctypes.c_void_p], ctypes.c_int),
            "XCloseDisplay": ([ctypes.c_void_p], ctypes.c_int),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.x, name)
            function.argtypes, function.restype = arguments, result
        self.display = self.x.XOpenDisplay(display.encode())
        if not self.display:
            raise RuntimeError("Cannot open owned XSETTINGS display")
        self.window = self.x.XCreateSimpleWindow(self.display, self.x.XDefaultRootWindow(self.display), 0, 0, 1, 1, 0, 0, 0)
        self.selection = self.x.XInternAtom(self.display, b"_XSETTINGS_S0", 0)
        self.property = self.x.XInternAtom(self.display, b"_XSETTINGS_SETTINGS", 0)
        self.x.XSetSelectionOwner(self.display, self.selection, self.window, 0)
        self.serial = 0
        self.set_dpi(96)

    def set_dpi(self, dpi):
        self.serial += 1
        name = b"Xft/DPI"
        value = struct.pack("<B3xII", 0, self.serial, 1)
        value += struct.pack("<BBH", 0, 0, len(name)) + name + b"\0"
        value += struct.pack("<Ii", self.serial, dpi * 1024)
        data = (ctypes.c_ubyte * len(value)).from_buffer_copy(value)
        self.x.XChangeProperty(self.display, self.window, self.property, self.property, 8, 0, data, len(value))
        self.x.XFlush(self.display)

    def close(self):
        self.x.XCloseDisplay(self.display)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--maximized", action="store_true", help="Verify a WM-constrained maximized window")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    xvfb = wm = app = settings = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1400x1000x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("WINIT_X11_SCALE_FACTOR", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        wait_for_wm(env)
        settings = XSettings(display)
        app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        def xdo(*arguments):
            return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^zgx live DPI pointer$"], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if not window:
            raise RuntimeError("DPI pointer window did not open")
        xdo("windowmove", window, 30, 40)
        xdo("windowactivate", "--sync", window)
        time.sleep(.2)
        def geometry():
            return {key: int(value) for key, value in (line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())}
        if args.maximized:
            xdo("mousemove", "--window", window, 100, -10, "click", "--repeat", 2, "--delay", 100, 1)
            deadline = time.monotonic() + 2
            while "_NET_WM_STATE_MAXIMIZED_HORZ" not in subprocess.check_output(["xprop", "-id", window, "_NET_WM_STATE"], env=env, text=True):
                if time.monotonic() >= deadline:
                    raise AssertionError("Window manager did not maximize the test window")
                time.sleep(.05)
            time.sleep(.2)
        before = geometry()
        xdo("mousemove", "--window", window, 300, 60)
        time.sleep(.2)
        pointer_before = xdo("getmouselocation", "--shell")
        settings.set_dpi(192)
        if args.maximized:
            time.sleep(.6)
        else:
            deadline = time.monotonic() + 3
            while geometry()["WIDTH"] != before["WIDTH"] * 2:
                if time.monotonic() >= deadline:
                    failed_geometry = geometry()
                    app.terminate()
                    output, _ = app.communicate(timeout=3)
                    (args.output / "live-dpi-pointer.log").write_text(output)
                    raise AssertionError(f"No observed 2x live DPI resize: before={before}, after={failed_geometry}, log={output}")
                time.sleep(.05)
            time.sleep(.15)
        after = geometry()
        pointer_after = xdo("getmouselocation", "--shell")
        if pointer_before != pointer_after:
            raise AssertionError("Pointer physically moved during DPI transition")
        # Click and wheel without issuing another pointer motion.
        xdo("click", 1)
        xdo("click", 5)
        time.sleep(.2)
        from PIL import ImageGrab
        ImageGrab.grab(xdisplay=display).save(args.output / "live-dpi-pointer.png")
        output, _ = app.communicate(timeout=12)
        (args.output / "live-dpi-pointer.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        counts = re.search(r"DPI_POINTER left_click=(\d+) right_click=(\d+) left_wheel=(\d+) right_wheel=(\d+)", output)
        values = tuple(map(int, counts.groups())) if counts else None
        viewports = re.findall(r"VIEWPORT \(([^,]+), ([^)]+)\)", output)
        final_viewport = tuple(map(float, viewports[-1])) if viewports else None
        passed = values is not None and values[0:2] == (1, 0) and values[2] > 0 and values[3] == 0 and final_viewport == (after["WIDTH"] / 2., after["HEIGHT"] / 2.)
        result = {"passed": passed, "event_counts": values, "final_viewport": final_viewport, "backend": "X11", "maximized": args.maximized, "before": before, "after": after, "stationary_pointer": pointer_after, "binary_sha256": binary_sha256, "script_sha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()}
        (args.output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
        if not passed:
            raise AssertionError(output)
        print("Native live DPI stationary-pointer checks passed")
    finally:
        for process in (app, wm):
            stop(process)
        if settings is not None:
            settings.close()
        stop(xvfb)


if __name__ == "__main__":
    main()
