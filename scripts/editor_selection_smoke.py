#!/usr/bin/env python3
"""Validate Unicode mouse selection on a private X11 display."""
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
    with tempfile.TemporaryDirectory(prefix="zgui-editor-selection-") as runtime:
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
            output = args.output / "editor-selection.log"
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
                    raise RuntimeError("editor selection window did not open: " + output.read_text())
                xdo("windowactivate", "--sync", window)
                deadline = time.monotonic() + 12
                while output.read_text().count("SELECTION ") < 2:
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("editor did not finish initial presentation: " + output.read_text())
                    time.sleep(.1)
                def report():
                    count = output.read_text().count("SELECTION ")
                    deadline = time.monotonic() + 8
                    while output.read_text().count("SELECTION ") <= count:
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise RuntimeError("live editor report timed out: " + output.read_text())
                        time.sleep(.05)
                    rows = re.findall(r'^SELECTION (.*)$', output.read_text(), re.MULTILINE)
                    state = {}
                    for field in rows[-1].split():
                        name, value = field.split("=")
                        state[name] = value == "true" if value in ("true", "false") else float(value)
                    assert state["unchanged"], state
                    return state
                def move(x, y):
                    xdo("mousemove", "--sync", "--window", window, round(x), round(y))
                def click(x, y, count=1):
                    time.sleep(.6)  # Start a distinct native multi-click sequence.
                    move(x, y)
                    xdo("click", "--repeat", count, "--delay", 100, 1)
                def capture(name):
                    subprocess.run(["import", "-window", window, str(args.output / (name + ".png"))], env=env, check=True, timeout=10)
                reports = {"initial": report()}
                target = reports["initial"]
                click(target["ax"],target["y"])
                reports["single"] = report()
                assert reports["single"]["anchor"] == reports["single"]["focus"] == 2, reports
                time.sleep(.6)
                xdo("keydown", "Shift_L")
                move(target["gx"],target["y"])
                xdo("click",1)
                xdo("keyup", "Shift_L")
                reports["shift"] = report()
                assert reports["shift"]["anchor"] == 2 and reports["shift"]["focus"] == 14, reports
                capture("shift-click")
                click(target["bx"],target["y"],2)
                reports["word"] = report()
                assert (reports["word"]["anchor"],reports["word"]["focus"]) == (6,10), reports
                capture("double-click")
                time.sleep(.6)
                move(target["bx"],target["y"])
                xdo("click",1)
                time.sleep(.1)
                xdo("mousedown",1)
                move(target["ax"],target["y"])
                xdo("mouseup",1)
                reports["reverse_word_drag"] = report()
                if (reports["reverse_word_drag"]["anchor"], reports["reverse_word_drag"]["focus"]) != (10, 0):
                    # Preserve later native states to distinguish event dispatch
                    # lag from an incorrect gesture. The first failure stays fatal.
                    start = len(output.read_text())
                    deadline = time.monotonic() + 1
                    while time.monotonic() < deadline and app.poll() is None:
                        time.sleep(.05)
                    later = output.read_text()[start:]
                    (args.output / "reverse-word-drag-later.log").write_text(later)
                    print("REVERSE_WORD_DRAG_LATER \n" + later, flush=True)
                assert (reports["reverse_word_drag"]["anchor"],reports["reverse_word_drag"]["focus"]) == (10,0), reports
                capture("word-drag")
                click(target["line_x"],target["line_y"],3)
                reports["visual_line"] = report()
                line = reports["visual_line"]
                assert (line["anchor"],line["focus"]) == (target["line_start"],target["line_end"]), reports
                assert line["focus"] > line["anchor"] > 0, reports
                capture("triple-click")
                click(target["bx"],target["y"],4)
                reports["fourth_click"] = report()
                assert reports["fourth_click"]["anchor"] == reports["fourth_click"]["focus"] == 8, reports
                click(90,264)
                click(target["bx"],target["y"],2)
                reports["readonly"] = report()
                assert reports["readonly"]["readonly"], reports
                assert (reports["readonly"]["anchor"],reports["readonly"]["focus"]) == (6,10), reports
                xdo("type", "--clearmodifiers", "BLOCKED")
                xdo("key", "--clearmodifiers", "BackSpace", "ctrl+z")
                reports["readonly_blocked"] = report()
                assert reports["readonly_blocked"]["anchor"] == 6 and reports["readonly_blocked"]["focus"] == 10, reports
                capture("readonly")
                app.wait(timeout=30)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native editor selection checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
