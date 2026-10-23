#!/usr/bin/env python3
"""Validate the submenus example on an owned Xvfb/Openbox desktop."""
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
            raise RuntimeError("submenus window did not open")
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
        def report(stage):
            key("r")
            ImageGrab.grab(xdisplay=display).save(args.output/f"submenus-{stage}.png")
            samples.append({"stage":stage})
        click(300,84)
        report(1)
        key("Down","Right")
        report(2)
        xdo("windowsize",window,600,420)
        time.sleep(.3)
        report(3)
        key("Left")
        report(4)
        key("Return")
        report(5)
        key("Escape")
        report(6)
        click(300,170)
        report(7)
        key("Return")
        report(8)
        click(300,84)
        click(300,170)
        report(9)
        click(300,130)  # A parent item receives the same click across child scope.
        report(10)
        click(300,84)
        click(300,170)
        report(11)
        click(80,328)   # Outside all menus: close chain without background action.
        report(12)
        output, _ = app.communicate(timeout=15)
        (args.output / "submenus.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required=[
            'SNAPSHOT 1 parent=true child=false focus=Some("First")',
            'SNAPSHOT 2 parent=true child=true focus=Some("Action")',
            'SNAPSHOT 3 parent=true child=true focus=Some("Action")',
            'SNAPSHOT 4 parent=true child=false focus=Some("More")',
            'SNAPSHOT 5 parent=true child=true focus=Some("Action")',
            'SNAPSHOT 6 parent=true child=false focus=Some("More")',
            'SNAPSHOT 7 parent=true child=true focus=Some("Action")',
            'ACTION parent=false child=false selections=1',
            'SNAPSHOT 8 parent=false child=false focus=Some("Open actions")',
            'SNAPSHOT 9 parent=true child=true focus=Some("Action")',
            'PARENT_ACTION parent=false child=false count=1',
            'SNAPSHOT 10 parent=false child=false focus=Some("Open actions")',
            'SNAPSHOT 11 parent=true child=true focus=Some("Action")',
            'SNAPSHOT 12 parent=false child=false focus=Some("Open actions")',
            'SUBMENUS parent=false child=false selections=1 reports=12 parent_actions=1 background_hits=0' ]
        if any(item not in output for item in required):raise AssertionError(output)
        for stage,expected_x in [(2,456),(3,84)]:
            if not re.search(rf"BOUNDS {stage} child-menu Rect \{{ x: {expected_x}.0, y: 152.0,",output):raise AssertionError(output)
        result={"platform":"X11/Xvfb","scale":1,"result":"pass","checks":["Right opens child without closing parent","Left closes only child and restores trigger","Enter and pointer open child","viewport resize flips side placement","Escape closes only child","action closes chain before callback","focus restores root anchor","single click activates parent through child scope","outside click closes chain without background activation","automatic close"],"samples":samples,"limitations":"Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
