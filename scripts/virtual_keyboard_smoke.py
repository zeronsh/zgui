#!/usr/bin/env python3
"""Validate the virtual_keyboard example on an owned Xvfb/Openbox desktop."""
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
            raise RuntimeError("virtual_keyboard window did not open")
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
        def report(stage):
            xdo("key","--clearmodifiers","r")
            time.sleep(.2)
            screenshot=ImageGrab.grab(xdisplay=display)
            screenshot.save(args.output / f"virtual-keyboard-{stage}.png")
            focus_rows=[y for y in range(76,236) if screenshot.getpixel((origin_x+36,origin_y+y))[:3]==(47,64,88)]
            expected_start={1:204,2:76,3:76,4:108}[stage]
            if focus_rows!=list(range(expected_start,expected_start+32)):
                raise AssertionError(f"stage{stage} focus pixels: {focus_rows}")
            marker_rows=[y for y in range(76,236) if screenshot.getpixel((origin_x+33,origin_y+y))[:3]==(94,165,255)]
            if marker_rows!=list(range(expected_start,expected_start+32)):
                raise AssertionError(f"stage{stage} focus marker pixels: {marker_rows}")
            samples.append({"stage":stage,"visible_focus_rows":len(focus_rows),"visible_marker_rows":len(marker_rows)})
        xdo("key","--clearmodifiers","Tab","End")
        time.sleep(.2)
        report(1)
        xdo("key","--clearmodifiers","Prior")
        time.sleep(.2)
        report(2)
        xdo("key","--clearmodifiers","Home")
        time.sleep(.2)
        report(3)
        xdo("key","--clearmodifiers","Down")
        time.sleep(.2)
        report(4)
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait past.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "virtual_keyboard.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        reports=re.findall(r"SNAPSHOT (\d+) offset=([\d.]+) position=Some\((\d+)\) live=(\d+) built=(\d+)",output)
        expected_positions=[1_000_000,999_995,1,2]
        if len(reports)!=4:
            raise AssertionError(output)
        for sample,report_values,position in zip(samples,reports,expected_positions):
            number,offset,actual_position,live,built=map(int,report_values)
            if number!=sample["stage"] or actual_position!=position or not 1<=live<=8 or built>35:
                raise AssertionError(output)
            sample.update({"offset":offset,"position":position,"live_rows":live,"total_built":built})
        if samples[0]["offset"]!=31_999_840 or samples[2]["offset"]!=0 or samples[3]["offset"]!=0:
            raise AssertionError(output)
        result={"platform":"X11/Xvfb","scale":1,"result":"pass","checks":["End navigates to unmounted millionth row","PageUp across virtual rows","Home returns first","ArrowDown selects next","bounded live and total builds","automatic close"],"samples":samples,"limitations":"Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
