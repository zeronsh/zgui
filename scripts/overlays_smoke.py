#!/usr/bin/env python3
"""Validate the overlays example on an owned Xvfb/Openbox desktop."""
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
            xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
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
            raise RuntimeError("overlays window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        from PIL import ImageGrab
        window_info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
        origin_x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", window_info).group(1))
        origin_y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", window_info).group(1))
        samples=[]
        def key(*keys):
            xdo("key","--clearmodifiers",*keys)
            time.sleep(.15)
        def report(stage, check=None):
            key("r")
            screenshot=ImageGrab.grab(xdisplay=display)
            screenshot.save(args.output/f"overlays-{stage}.png")
            if check:
                # Native resizing can move the window; re-read client coordinates.
                info=subprocess.check_output(["xwininfo","-id",window],env=env,text=True)
                x=int(re.search(r"Absolute upper-left X:\s+(-?\d+)",info).group(1))
                y=int(re.search(r"Absolute upper-left Y:\s+(-?\d+)",info).group(1))
                px,py,color=check
                actual=screenshot.getpixel((x+px,y+py))[:3]
                if actual!=color: raise AssertionError(f"stage{stage}: panel pixel {actual}, expected {color}")
            samples.append({"stage":stage,"panel_pixel_checked":bool(check)})
        click(80,168)
        report(1,(30,278,(54,81,110)))
        key("s")
        report(2,(30,238,(54,81,110)))
        xdo("windowsize",window,600,240)
        time.sleep(.3)
        # Popups reach past the window: the panel keeps below its trigger,
        # past the 240-point window's bottom edge, where only its own
        # window paints.
        report(3,(30,244,(54,81,110)))
        xdo("windowsize",window,600,420)
        time.sleep(.3)
        key("Escape")
        report(4)
        click(80,84)
        report(5,(145,200,(40,61,84)))
        # The modal's two buttons are its Tab cycle; the panel is no stop.
        key("Tab","Tab")
        report(6)
        click(200,168)
        report(7,(162,250,(74,55,104)))
        key("Escape")
        report(8)
        click(580,390)
        report(9)
        click(80,128)
        report(10)
        click(580,390)
        report(11)
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait past.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "overlays.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required=[
            'SNAPSHOT 1 popup=true modal=false nested=false offset=0 focus=Some("Close popup")',
            'SNAPSHOT 2 popup=true modal=false nested=false offset=40',
            'SNAPSHOT 4 popup=false modal=false nested=false offset=40 focus=Some("Open anchored popup")',
            'SNAPSHOT 5 popup=false modal=true nested=false offset=40 focus=Some("Open nested popup")',
            'SNAPSHOT 6 popup=false modal=true nested=false offset=40 focus=Some("Open nested popup")',
            'SNAPSHOT 7 popup=false modal=true nested=true offset=40 focus=Some("Close nested")',
            'SNAPSHOT 8 popup=false modal=true nested=false offset=40 focus=Some("Open nested popup")',
            'SNAPSHOT 9 popup=false modal=false nested=false offset=40 focus=Some("Open modal")',
            'SNAPSHOT 10 popup=true modal=false nested=false offset=40',
            'SNAPSHOT 11 popup=false modal=false nested=false offset=40 focus=Some("Open anchored popup")',
            'OVERLAYS popup=false modal=false nested=false offset=40 reports=11']
        if any(item not in output for item in required):raise AssertionError(output)
        # Stage 3: past the window's bottom edge, not flipped above.
        for stage,expected_y in [(1,188),(2,148),(3,148)]:
            if not re.search(rf"BOUNDS {stage} popup Rect \{{ x: 24.0, y: {expected_y}.0,",output):raise AssertionError(output)
        result={"platform":"X11/Xvfb","scale":1,"result":"pass","checks":["anchored popup native opening","anchor compositor scroll tracking","popup past the window's edge","Escape focus restore","modal Tab containment","nested popup focus scope","backdrop dismissal","panel pixels","automatic close"],"samples":samples,"limitations":"Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
