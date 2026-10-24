#!/usr/bin/env python3
"""Exercise the gallery's native X11 input/clipboard bridge on an existing test display."""
import argparse
import os
import pathlib
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--display", default=os.environ.get("DISPLAY"))
    parser.add_argument("--screenshot", type=pathlib.Path)
    parser.add_argument("--scale", type=float, default=1.0,
                        help="force initial X11 scale; pointer coordinates and resize use physical pixels")
    args = parser.parse_args()
    if args.scale <= 0 or not __import__("math").isfinite(args.scale):
        parser.error("--scale must be finite and positive")
    if not args.display:
        parser.error("--display must name an X11 test display with a window manager")
    env = dict(os.environ, DISPLAY=args.display, ZGX_SECONDS="15", ZGX_SNAPSHOT="1")
    env.pop("WAYLAND_DISPLAY", None)
    env["WINIT_X11_SCALE_FACTOR"] = str(args.scale)
    # An isolated test display need not inherit a potentially unrelated portal bus.
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True)
    def xdo(*arguments):
        return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()
    try:
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and proc.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(proc.pid)],
                                   env=env, text=True, capture_output=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if not window:
            raise RuntimeError("gallery did not open a visible window")
        geometry = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())
        expected_size = (round(900 * args.scale), round(760 * args.scale))
        actual_size = (int(geometry["WIDTH"]), int(geometry["HEIGHT"]))
        if actual_size != expected_size:
            raise AssertionError(f"initial scale not applied: expected {expected_size}, got {actual_size}")
        xdo("windowactivate", "--sync", window)
        time.sleep(.2)
        xdo("mousemove", "--window", window, round(60 * args.scale), round(126 * args.scale), "click", 1)
        xdo("mousemove", "--window", window, round(80 * args.scale), round(430 * args.scale), "click", 1)
        xdo("key", "--clearmodifiers", "ctrl+a")
        xdo("type", "--clearmodifiers", "--delay", 15, "Native bridge")
        xdo("key", "--clearmodifiers", "ctrl+a", "ctrl+c", "Tab", "ctrl+a", "ctrl+v")
        # A resize checks the native logical/physical viewport handoff too.
        xdo("windowsize", window, round(1000 * args.scale), round(800 * args.scale))
        expected_resize = (round(1000 * args.scale), round(800 * args.scale))
        deadline = time.monotonic() + 5
        while True:
            geometry = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())
            resized = (int(geometry["WIDTH"]), int(geometry["HEIGHT"]))
            if resized == expected_resize:
                break
            if time.monotonic() >= deadline:
                raise AssertionError(f"resize not applied: expected {expected_resize}, got {resized}")
            time.sleep(.05)
        time.sleep(.3)
        if args.screenshot:
            from PIL import ImageGrab
            ImageGrab.grab(xdisplay=args.display).save(args.screenshot)
        # Closed as a user closes it (Alt+F4 to the window manager), not by
        # the app's own timer; it prints what it holds as it closes.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = proc.communicate(timeout=25)
        if proc.returncode:
            raise RuntimeError(output)
        for expected in ['SEMANTIC "Count: 1"', 'SEMANTIC "Name" Some("Native bridge")',
                         'SEMANTIC "Notes" Some("Native bridge")', 'VIEWPORT (1000.0, 800.0)']:
            if expected not in output:
                raise AssertionError(f"missing native interaction result {expected!r}:\n{output}")
        print(f"PASS: X11 scale={args.scale:g}, initial physical size={actual_size}, native pointer activation, text entry, selection, clipboard, Tab focus, resize, close")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    main()
