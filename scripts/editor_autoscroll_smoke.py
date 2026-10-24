#!/usr/bin/env python3
"""Validate stationary captured editor autoscroll on a private X11 display."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgx-editor-autoscroll-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache", GSETTINGS_BACKEND="memory", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            output = args.output / "editor-autoscroll.log"
            with output.open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=log, stderr=log)
                def xdo(*parts):
                    return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()
                deadline = time.monotonic() + 20
                window = None
                while time.monotonic() < deadline and app.poll() is None:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, text=True, capture_output=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(.05)
                if window is None:
                    raise RuntimeError("editor autoscroll window did not open: " + output.read_text())
                xdo("windowactivate", "--sync", window)
                time.sleep(1.)
                def move(x, y):
                    xdo("mousemove", "--window", window, x, y)
                def key(*keys):
                    xdo("key", "--clearmodifiers", *keys)
                def report():
                    count = output.read_text().count("AUTOSCROLL ")
                    xdo("key", "r")  # Preserve the held mouse button while reporting.
                    deadline = time.monotonic() + 8
                    while output.read_text().count("AUTOSCROLL ") < count + 2:
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise RuntimeError("editor report timed out: " + output.read_text())
                        time.sleep(.02)
                    rows = re.findall(r'^AUTOSCROLL (.*)$', output.read_text(), re.MULTILINE)[-2:]
                    result = []
                    for row in rows:
                        state = {}
                        for field in row.split():
                            name, value = field.split("=")
                            state[name] = value == "true" if value in ("true", "false") else float(value)
                        assert state["unchanged"], state
                        result.append(state)
                    return result
                def capture(name):
                    subprocess.run(["import", "-window", window, str(args.output / (name + ".png"))], env=env, check=True, timeout=10)
                move(60,80)
                xdo("click",1)
                key("ctrl+Home")
                time.sleep(.6)
                move(60,80)
                xdo("mousedown",1)
                move(60,240)
                reports = {"vertical_start": report()}
                time.sleep(.5)  # No pointer or keyboard events during the hold.
                reports["vertical_hold"] = report()
                start, held = reports["vertical_start"][0], reports["vertical_hold"][0]
                assert held["captured"] and held["focus"] > start["focus"] and held["y"] - start["y"] > 100, reports
                time.sleep(.5)
                reports["vertical_hold_again"] = report()
                assert reports["vertical_hold_again"][0]["y"] > held["y"], reports
                capture("vertical-hold")
                move(60,110)
                reports["inside"] = report()
                time.sleep(.4)
                reports["inside_still"] = report()
                assert reports["inside"][0] == reports["inside_still"][0], reports
                move(60,240)
                time.sleep(.2)
                xdo("mouseup",1)
                reports["released"] = report()
                time.sleep(.4)
                reports["released_still"] = report()
                assert not reports["released"][0]["captured"], reports
                assert reports["released"][0] == reports["released_still"][0], reports
                move(100,276)
                xdo("click",1)
                move(60,80)
                xdo("click",1)
                key("ctrl+Home")
                time.sleep(.6)
                move(60,80)
                xdo("mousedown",1)
                move(60,240)
                reports["readonly_start"] = report()
                time.sleep(.5)
                reports["readonly_hold"] = report()
                xdo("mouseup",1)
                assert reports["readonly_hold"][0]["readonly"], reports
                assert reports["readonly_hold"][0]["y"] > reports["readonly_start"][0]["y"], reports
                move(60,228)
                xdo("click",1)
                key("Home")
                time.sleep(.6)
                move(60,228)
                xdo("mousedown",1)
                move(460,228)
                reports["horizontal_start"] = report()
                time.sleep(.5)
                reports["horizontal_hold"] = report()
                horizontal = reports["horizontal_hold"][1]
                assert horizontal["captured"] and horizontal["x"] - reports["horizontal_start"][1]["x"] > 100, reports
                assert horizontal["focus"] > reports["horizontal_start"][1]["focus"], reports
                capture("horizontal-hold")
                xdo("mouseup",1)
                reports["horizontal_released"] = report()
                time.sleep(.4)
                reports["horizontal_stopped"] = report()
                assert reports["horizontal_released"][1] == reports["horizontal_stopped"][1], reports
                move(60,80)
                xdo("click",1)
                key("ctrl+Home")
                time.sleep(.6)
                move(60,80)
                xdo("mousedown",1)
                move(60,240)
                reports["before_hide"] = report()
                assert reports["before_hide"][0]["captured"], reports
                xdo("key", "h")  # Keep the physical mouse button held while hiding.
                def visible():
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, text=True, capture_output=True)
                    return window in found.stdout.splitlines()
                deadline = time.monotonic() + 5
                while visible():
                    assert time.monotonic() < deadline, "window never hid"
                    time.sleep(.01)
                while not visible():
                    assert time.monotonic() < deadline, "window did not restore"
                    time.sleep(.02)
                xdo("windowactivate", "--sync", window)
                reports["restored_cancelled"] = report()
                assert not reports["restored_cancelled"][0]["captured"], reports
                time.sleep(.4)
                reports["restored_stable"] = report()
                assert reports["restored_cancelled"][0] == reports["restored_stable"][0], reports
                xdo("mouseup",1)
                capture("restored-cancelled")
                key("Escape")
                app.wait(timeout=10)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", no_periodic_diagnostic_wakes=True, reports=reports), indent=2) + "\n")
            print("Native editor autoscroll checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
