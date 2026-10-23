#!/usr/bin/env python3
"""Validate the scrollbars example on an owned Xvfb/Openbox desktop."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        time.sleep(.5)
        app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

        def xdo(*args):
            return subprocess.check_output(["xdotool", *map(str, args)], env=env, text=True).strip()

        deadline = time.monotonic() + 6
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if window is None:
            raise RuntimeError("scrollbars window did not open")
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
        def key(*keys):
            xdo("key","--clearmodifiers",*keys)
            time.sleep(.15)
        def report(stage):
            key("r")
            screenshot=ImageGrab.grab(xdisplay=display)
            screenshot.save(args.output / f"scrollbars-{stage}.png")
            thumb_colors=((94,165,255),(229,237,247))
            vertical=[y for y in range(76,220) if screenshot.getpixel((origin_x+212,origin_y+y))[:3] in thumb_colors]
            horizontal=[x for x in range(248,472) if screenshot.getpixel((origin_x+x,origin_y+136))[:3] in thumb_colors]
            samples.append({"stage":stage,"vertical_thumb_pixels":len(vertical),"horizontal_thumb_pixels":len(horizontal)})
            if stage==8:
                empty_pixels = [screenshot.getpixel((origin_x+x,origin_y+y))[:3] for x,y in ((212,200),(460,136))]
                if vertical or horizontal or any(pixel != (32,48,64) for pixel in empty_pixels):
                    raise AssertionError("scrollbar remained visible without overflow")
            # Thumbs span viewport²/content less a 2px inset at each end:
            # 144²/320-4 and 224²/640-4. Rounded ends blend one pixel each.
            elif not (58<=len(vertical)<=62 and 72<=len(horizontal)<=76):
                raise AssertionError(f"missing native scrollbar thumbs: {samples[-1]}")
        def drag(x,y,end_x,end_y):
            xdo("mousemove","--window",window,x,y,"mousedown",1)
            xdo("mousemove","--window",window,end_x,end_y)
            time.sleep(.15)
            xdo("mouseup",1)
            time.sleep(.15)

        click(212,205)  # Track click pages down by the 144px viewport.
        report(1)
        drag(212,170,212,300)  # Captured pointer travels outside viewport.
        report(2)
        key("Home")
        report(3)
        key("End")
        report(4)
        key("Prior")
        report(5)
        key("Next")
        report(6)
        drag(270,136,530,136)
        report(7)
        click(100,262)
        report(8)
        output, _ = app.communicate(timeout=15)
        (args.output / "scrollbars.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required = ["SNAPSHOT 1 vertical=144.000 horizontal=0.000 count=8", "SNAPSHOT 2 vertical=176.000 horizontal=0.000 count=8", "SNAPSHOT 3 vertical=0.000 horizontal=0.000 count=8", "SNAPSHOT 4 vertical=176.000 horizontal=0.000 count=8", "SNAPSHOT 5 vertical=32.000 horizontal=0.000 count=8", "SNAPSHOT 6 vertical=176.000 horizontal=0.000 count=8", "SNAPSHOT 7 vertical=176.000 horizontal=416.000 count=8", "SNAPSHOT 8 vertical=0.000 horizontal=0.000 count=2", "SCROLLBARS vertical=0.000 horizontal=0.000 count=2 reports=8"]
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["vertical track paging", "captured thumb drag", "Home End PageUp PageDown", "horizontal thumb drag", "hide without overflow", "shrink offset clamp", "automatic close"], "pixel_samples": samples, "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
