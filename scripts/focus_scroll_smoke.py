#!/usr/bin/env python3
"""Validate the focus_scroll example on an owned Xvfb/Openbox desktop."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        wait_for_wm(env)
        app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

        def xdo(*args):
            return subprocess.check_output(["xdotool", *map(str, args)], env=env, text=True).strip()

        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if window is None:
            raise RuntimeError("focus_scroll window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        from PIL import ImageGrab
        window_info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
        origin_x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", window_info).group(1))
        origin_y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", window_info).group(1))
        samples = []
        def report(stage, expected_rows):
            xdo("key", "--clearmodifiers", "r")
            time.sleep(.2)
            screenshot = ImageGrab.grab(xdisplay=display)
            screenshot.save(args.output / f"focus-scroll-{stage}.png")
            blue_rows = [y for y in range(76,220) if screenshot.getpixel((origin_x+36, origin_y+y))[:3] == (64,160,224)]
            if blue_rows != list(expected_rows):
                raise AssertionError(f"stage{stage} focus pixels: {blue_rows}")
            samples.append({"stage": stage, "visible_focus_rows": len(blue_rows)})

        xdo("key", "--clearmodifiers", *("Tab",)*8)
        time.sleep(.2)
        report(1,range(180,220))
        xdo("key", "--clearmodifiers", *("shift+Tab",)*7)
        time.sleep(.2)
        report(2,range(76,116))
        # Four wheel notches of 40 points (XTEST: a click is a notch).
        xdo("mousemove", "--window", window, 100,150,"click","--repeat",4,"--delay",100,5)
        time.sleep(.2)
        report(3,range(0))
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait past.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "focus_scroll.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required = ["SNAPSHOT 1 offset=176.000 focused=Some(7)", "SNAPSHOT 2 offset=0.000 focused=Some(0)", "SNAPSHOT 3 offset=160.000 focused=Some(0)", "FOCUS_SCROLL offset=160.000 reports=3"]
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["Tab reveals last button", "ShiftTab reveals first button", "focused control visible pixels", "manualwheel does not snap back", "focused identity retained", "automatic close"], "pixel_samples": samples, "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
