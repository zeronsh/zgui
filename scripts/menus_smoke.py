#!/usr/bin/env python3
"""Validate the menus example on an owned Xvfb/Openbox desktop."""
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
            raise RuntimeError("menus window did not open")
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
            ImageGrab.grab(xdisplay=display).save(args.output/f"menus-{stage}.png")
            samples.append({"stage":stage})
        click(80,84)
        report(1)
        click(80,164)  # Disabled item must not activate or dismiss the menu.
        report(2)
        key("Down")
        report(3)
        key("End")
        report(4)
        key("Home")
        report(5)
        key("m")
        report(6)
        key("Return")
        report(7)
        click(80,84)
        click(80,236)
        report(8)
        click(80,84)
        key("Escape")
        report(9)
        click(80,84)
        key("Tab")
        report(10)
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait past.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "menus.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required=[
            'SNAPSHOT 1 open=true focus=Some("First") selections=[]',
            'SNAPSHOT 2 open=true focus=Some("First") selections=[]',
            'SNAPSHOT 3 open=true focus=Some("Middle") selections=[]',
            'SNAPSHOT 4 open=true focus=Some("Last") selections=[]',
            'SNAPSHOT 5 open=true focus=Some("First") selections=[]',
            'SNAPSHOT 6 open=true focus=Some("Middle") selections=[]',
            'SELECT Middle open=false',
            'SNAPSHOT 7 open=false focus=Some("Open menu") selections=["Middle"]',
            'SELECT Last open=false',
            'SNAPSHOT 8 open=false focus=Some("Open menu") selections=["Middle", "Last"]',
            'SNAPSHOT 9 open=false focus=Some("Open menu") selections=["Middle", "Last"]',
            'SNAPSHOT 10 open=false focus=Some("Next control") selections=["Middle", "Last"]',
            'MENUS open=false selections=["Middle", "Last"] reports=10']
        if any(item not in output for item in required) or output.count("SELECT ")!=2:raise AssertionError(output)
        result={"platform":"X11/Xvfb","scale":1,"result":"pass","checks":["first enabled focus","disabled pointer suppression","ArrowDown disabled skip","Home End navigation","printable typeahead","Tab closes and advances focus","Enter closes before callback","pointer activation closes before callback","Escape focus restoration","automatic close"],"samples":samples,"limitations":"Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
