#!/usr/bin/env python3
"""Exercise native function-key actions and pending-text replay on an owned X11 desktop."""
import argparse
import ast
import ctypes
import json
import os
from pathlib import Path
import select
import subprocess
import time
from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, default=Path("/tmp/zgx-actions-smoke"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    logs = []
    try:
        for name in ("xvfb", "wm", "app"):
            logs.append((args.output / (name + ".log")).open("w"))
        xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1000x700x24", "-ac", "-noreset"], stdout=subprocess.PIPE, stderr=logs[0], text=True)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timeout")
        env = dict(os.environ, DISPLAY=":" + xvfb.stdout.readline().strip(), ZGX_ACTION_TRACE="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("WINIT_X11_SCALE_FACTOR", None)
        # Xvfb can inherit a mapping where xdotool chooses Alt+Fn for the
        # requested keysym. Normalize only this owned server's three test keys.
        subprocess.run(["xmodmap", "-e", "keycode 67 = F1", "-e", "keycode 68 = F2", "-e", "keycode 71 = F5"], env=env, check=True, capture_output=True)
        wm = subprocess.Popen(["openbox"], env=env, stdout=logs[1], stderr=logs[1])
        wait_for_wm(env)
        app = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=logs[2], stderr=logs[2])
        def xdo(*command):
            return subprocess.run(["xdotool", *map(str, command)], env=env, check=True, text=True, capture_output=True).stdout.strip()
        # Send physical Fn presses so xdotool's keysym resolver cannot choose
        # an alternate Alt+Fn combination from this server's mapping.
        xlib = ctypes.CDLL("libX11.so.6")
        xtst = ctypes.CDLL("libXtst.so.6")
        xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        xlib.XOpenDisplay.restype = ctypes.c_void_p
        xlib.XFlush.argtypes = [ctypes.c_void_p]
        xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
        xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        def function_keys(*numbers):
            connection = xlib.XOpenDisplay(env["DISPLAY"].encode())
            if not connection:
                raise RuntimeError("XTest display connection failed")
            try:
                for number in numbers:
                    for pressed in (1, 0):
                        xtst.XTestFakeKeyEvent(connection, 66 + number, pressed, 0)
                        xlib.XFlush(connection)
                        time.sleep(.04)
            finally:
                xlib.XCloseDisplay(connection)
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline:
            result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^zgx actions$"], env=env, text=True, capture_output=True)
            if result.returncode == 0:
                window = result.stdout.splitlines()[0]
                break
            if app.poll() is not None:
                raise RuntimeError("app exited during startup")
            time.sleep(.05)
        if not window:
            raise RuntimeError("native window not found")
        xdo("windowactivate", "--sync", window)
        time.sleep(1.5)
        xdo("mousemove", "--window", window, 140, 80)
        xdo("click", 1)
        time.sleep(.2)
        xdo("key", "--clearmodifiers", "ctrl+a")
        xdo("type", "--clearmodifiers", "base")
        time.sleep(.2)
        function_keys(5)
        time.sleep(.3)
        xdo("type", "x")
        time.sleep(1.3)  # The host must wake itself to expire the prefix.
        before_next_key = (args.output / "app.log").read_text()
        if 'ACTION_MODEL "basex"' not in before_next_key:
            raise AssertionError("prefix text did not replay on the host timer before the next key")
        time.sleep(.2)
        function_keys(5)
        time.sleep(.3)
        xdo("type", "--delay", 80, "xy")
        time.sleep(.3)
        function_keys(1, 2)
        time.sleep(.3)
        xdo("type", "--delay", 80, "xz")
        time.sleep(.2)
        function_keys(5)
        time.sleep(.3)
        time.sleep(.3)
        expected = ["base", "basex", "basex", "basex", "basexxz"]
        data = (args.output / "app.log").read_text()
        saved = [ast.literal_eval(line.split(" ", 1)[1]) for line in data.splitlines() if line.startswith("ACTION_SAVE ")]
        if saved != expected:
            raise AssertionError({"expected": expected, "actual": saved, "log": data})
        result = {"passed": True, "backend": "X11/Xvfb", "saved_models": saved, "cases": ["F5 dispatch", "native prefix text timeout", "matched character chord", "function key chord", "mismatch text replay"], "external_file_drag_tested": False}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)
        for log in logs:
            log.close()


if __name__ == "__main__":
    main()
