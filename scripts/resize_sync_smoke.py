#!/usr/bin/env python3
"""A window manager's resize waits for zgx's frames on X11 (Xvfb/Openbox): the
vendored winit's `_NET_WM_SYNC_REQUEST` (vendor/winit/ZGX_PATCH.md), as GTK
and Chromium do.

Checks that window_probe's window takes part in the protocol
(`WM_PROTOCOLS` names it, and `_NET_WM_SYNC_REQUEST_COUNTER` its counter),
and that through an interactive resize (Openbox's Alt and right button drag)
it answers the window manager's requests: the counter, at 0 before, reaches
the window manager's last value, the window takes the size the pointer
gives it, and a frame is drawn for each step. The counter is read with
libxcb-sync, through ctypes."""
import argparse
import ctypes
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm


def counter_value(display, counter):
    """The SYNC counter's value, read from the X server."""
    xcb, sync = ctypes.CDLL("libxcb.so.1"), ctypes.CDLL("libxcb-sync.so.1")

    class Cookie(ctypes.Structure):
        _fields_ = [("sequence", ctypes.c_uint)]

    class Reply(ctypes.Structure):
        _fields_ = [("response_type", ctypes.c_uint8), ("pad", ctypes.c_uint8), ("sequence", ctypes.c_uint16),
                    ("length", ctypes.c_uint32), ("hi", ctypes.c_int32), ("lo", ctypes.c_uint32)]

    xcb.xcb_connect.restype = ctypes.c_void_p
    xcb.xcb_connect.argtypes = [ctypes.c_char_p, ctypes.c_void_p]
    xcb.xcb_disconnect.argtypes = [ctypes.c_void_p]
    sync.xcb_sync_query_counter.restype = Cookie
    sync.xcb_sync_query_counter.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    sync.xcb_sync_query_counter_reply.restype = ctypes.POINTER(Reply)
    sync.xcb_sync_query_counter_reply.argtypes = [ctypes.c_void_p, Cookie, ctypes.c_void_p]
    connection = xcb.xcb_connect(display.encode(), None)
    try:
        reply = sync.xcb_sync_query_counter_reply(connection, sync.xcb_sync_query_counter(connection, counter), None)
        return (reply.contents.hi << 32) | reply.contents.lo if reply else None
    finally:
        xcb.xcb_disconnect(connection)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="the window_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", ZGX_PRESENT_LOG="1",
                   XDG_STATE_HOME=str((args.output / "state").resolve()),
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL))
        wait_for_wm(env)
        log = args.output / "window_probe.log"
        app = subprocess.Popen([str(args.probe), "saved"], env=env, stdout=log.open("w"), stderr=subprocess.STDOUT)
        processes.append(app)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        deadline = time.monotonic() + 15
        while not (window := run("xdotool", "search", "--onlyvisible", "--name", "^Saved$").split()):
            if time.monotonic() > deadline or app.poll() is not None:
                raise RuntimeError("the window did not show: " + log.read_text()[-2000:])
            time.sleep(.05)
        window = window[0]
        time.sleep(.5)
        protocols = run("xprop", "-id", window, "WM_PROTOCOLS")
        checks["takes part"] = "_NET_WM_SYNC_REQUEST" in protocols
        found = re.search(r"= (\d+)", run("xprop", "-id", window, "_NET_WM_SYNC_REQUEST_COUNTER"))
        assert found, protocols
        counter = int(found.group(1))
        checks["counter before"] = counter_value(env["DISPLAY"], counter)

        def geometry():
            shell = dict(line.split("=", 1) for line in run("xdotool", "getwindowgeometry", "--shell",
                                                            window).splitlines())
            return int(shell["X"]), int(shell["Y"]), int(shell["WIDTH"]), int(shell["HEIGHT"])

        x, y, width, height = geometry()
        renders = log.read_text().count(" render ")
        # Openbox resizes with Alt and the right button, from the nearest
        # corner: the bottom right here.
        start = (x + width - 60, y + height - 40)
        run("xdotool", "mousemove", *start, "keydown", "alt", "mousedown", 3)
        steps = 12
        for step in range(1, steps + 1):
            run("xdotool", "mousemove", start[0] + step * 15, start[1] + step * 10)
            time.sleep(.04)
        run("xdotool", "mouseup", 3, "keyup", "alt")
        time.sleep(.5)
        checks["counter after"] = counter_value(env["DISPLAY"], counter)
        checks["size"] = list(geometry()[2:])
        checks["frames drawn"] = log.read_text().count(" render ") - renders
        checks["answered the window manager"] = (checks["counter before"] == 0
                                                 and (checks["counter after"] or 0) >= steps - 2)
        checks["resized with the pointer"] = checks["size"] == [width + steps * 15, height + steps * 10]
        checks["a frame each step"] = checks["frames drawn"] >= steps - 2
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: an X11 window manager's resize waits for zgx's frames (_NET_WM_SYNC_REQUEST)")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
