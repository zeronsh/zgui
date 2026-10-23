#!/usr/bin/env python3
"""Validate wrapped Unicode visual-page navigation on a private X11 display."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-editor-paging-") as runtime:
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
            time.sleep(.4)
            output = args.output / "editor-paging.log"
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
                    raise RuntimeError("editor paging window did not open: " + output.read_text())
                xdo("windowactivate", "--sync", window)
                deadline = time.monotonic() + 12
                while output.read_text().count("PAGING ") < 2:
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("editor did not finish initial presentation: " + output.read_text())
                    time.sleep(.1)
                def report():
                    count = output.read_text().count("PAGING ")
                    deadline = time.monotonic() + 8
                    while output.read_text().count("PAGING ") <= count:
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise RuntimeError("live editor report timed out: " + output.read_text())
                        time.sleep(.05)
                    time.sleep(.1)
                    rows = re.findall(r'PAGING anchor=(\d+) focus=(\d+) caret_y=([-\d.]+) caret_h=([\d.]+) viewport_y=([-\d.]+) viewport_h=([\d.]+) width=([\d.]+) scroll=([-\d.]+) readonly=(true|false) unchanged=(true|false)', output.read_text())
                    assert rows, output.read_text()
                    anchor, focus, caret, height, top, viewport, width, scroll, readonly, unchanged = rows[-1]
                    state = dict(anchor=int(anchor), focus=int(focus), caret_y=float(caret), caret_h=float(height), viewport_y=float(top), viewport_h=float(viewport), width=float(width), scroll=float(scroll), readonly=readonly == "true", unchanged=unchanged == "true")
                    assert state["unchanged"], state
                    assert state["caret_y"] >= state["viewport_y"] - 1, state
                    assert state["caret_y"] + state["caret_h"] <= state["viewport_y"] + state["viewport_h"] + 1, state
                    return state
                def key(*keys):
                    xdo("key", "--clearmodifiers", *keys)
                def capture(name):
                    subprocess.run(["import", "-window", window, str(args.output / (name + ".png"))], env=env, check=True, timeout=10)
                xdo("mousemove", "--sync", "--window", window, 80, 90, "click", 1)
                key("ctrl+Home", "Right", "Right", "Right")
                reports = {"start": report()}
                start = reports["start"]
                key("Next")
                reports["page_down"] = report()
                down = reports["page_down"]
                assert down["focus"] > start["focus"] and down["anchor"] == down["focus"], reports
                assert down["scroll"] >= down["viewport_h"] - 2 * down["caret_h"], reports
                assert abs(down["caret_y"] - start["caret_y"]) <= down["caret_h"], reports
                capture("page-down")
                key("shift+Next")
                reports["extend_down"] = report()
                extended = reports["extend_down"]
                assert extended["anchor"] == down["focus"] and extended["focus"] > down["focus"], reports
                capture("selection")
                key("shift+Prior")
                reports["extend_back"] = report()
                assert reports["extend_back"]["focus"] == down["focus"], reports
                key("Prior")
                reports["page_up"] = report()
                assert reports["page_up"]["focus"] == start["focus"], reports
                assert reports["page_up"]["scroll"] == 0, reports
                xdo("mousemove", "--sync", "--window", window, 90, 324, "click", 1)
                xdo("mousemove", "--sync", "--window", window, 80, 90, "click", 1)
                key("ctrl+Home", "Next", "shift+Next")
                xdo("type", "--clearmodifiers", "BLOCKED")
                key("BackSpace")
                reports["readonly"] = report()
                assert reports["readonly"]["readonly"] and reports["readonly"]["focus"] > reports["readonly"]["anchor"], reports
                capture("readonly")
                xdo("windowsize", window, 360, 300)
                time.sleep(.3)
                key("ctrl+Home", "Next")
                reports["resized"] = report()
                assert reports["resized"]["width"] == 320, reports
                assert reports["resized"]["viewport_h"] == 136, reports
                assert reports["resized"]["focus"] > 0 and reports["resized"]["scroll"] > 0, reports
                capture("resized")
                key("ctrl+End", "Next")
                reports["end"] = report()
                key("Next")
                reports["still_end"] = report()
                assert reports["end"]["focus"] == reports["still_end"]["focus"], reports
                key("ctrl+Home", "Prior")
                reports["beginning"] = report()
                assert reports["beginning"]["focus"] == 0 and reports["beginning"]["scroll"] == 0, reports
                app.wait(timeout=25)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native editor paging checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
