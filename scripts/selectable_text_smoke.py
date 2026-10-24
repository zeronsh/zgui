#!/usr/bin/env python3
"""Validate native pointer selection of display text on a private X11 display."""
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


# Reads the X11 primary selection (what a middle click pastes) through GTK.
PRIMARY = """
import gi, sys
gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, Gtk
text = Gtk.Clipboard.get(Gdk.SELECTION_PRIMARY).wait_for_text()
sys.stdout.write(text or "")
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgx-selectable-text-") as runtime:
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
            output = args.output / "selectable-text.log"
            with output.open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=log, stderr=log)

                def xdo(*parts):
                    return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()
                deadline = time.monotonic() + 8
                window = None
                while time.monotonic() < deadline and app.poll() is None:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, text=True, capture_output=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(.05)
                if window is None:
                    raise RuntimeError("selectable text window did not open: " + output.read_text())
                xdo("windowactivate", "--sync", window)

                def report():
                    # A report once the input settled: one taken between a
                    # double click's clicks shows the first's. Two the same in
                    # a row, both after the input, are the state it left.
                    count = output.read_text().count("SELECTABLE ")
                    deadline = time.monotonic() + 8
                    while True:
                        rows = re.findall(r'^SELECTABLE (.*)$', output.read_text(), re.MULTILINE)
                        if len(rows) >= count + 2 and rows[-1] == rows[-2]:
                            break
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise RuntimeError("selection report timed out: " + output.read_text())
                        time.sleep(.05)
                    row = rows[-1]
                    boxes = dict(re.findall(r'(\w+)=([\d.,-]+) ', row))
                    state = {name: [float(v) for v in value.split(",")] for name, value in boxes.items()}
                    state["text"] = json.loads(row.split(" text=", 1)[1])
                    return state

                def move(x, y):
                    xdo("mousemove", "--sync", "--window", window, round(x), round(y))

                def click(x, y, count=1):
                    time.sleep(.6)  # Start a distinct native multi-click sequence.
                    move(x, y)
                    xdo("click", "--repeat", count, "--delay", 100, 1)

                def drag(start, end):
                    time.sleep(.6)
                    move(*start)
                    xdo("mousedown", 1)
                    for step in range(1, 9):
                        move(start[0] + (end[0] - start[0]) * step / 8, start[1] + (end[1] - start[1]) * step / 8)
                    xdo("mouseup", 1)

                def capture(name):
                    subprocess.run(["import", "-window", window, str(args.output / (name + ".png"))], env=env, check=True, timeout=10)
                deadline = time.monotonic() + 12
                while output.read_text().count("SELECTABLE ") < 2:
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("selectable text did not present: " + output.read_text())
                    time.sleep(.1)
                reports = {"initial": report()}
                heading, intro, last = (reports["initial"][name] for name in ("heading", "intro", "last"))
                assert reports["initial"]["text"] == "", reports
                # A drag from the heading into the last paragraph spans every block.
                drag((heading[0] + 1, heading[1] + heading[3] / 2), (last[0] + 60, last[1] + 8))
                reports["drag"] = report()
                text = reports["drag"]["text"]
                assert text.startswith("Selectable text\n\nDrag across"), reports
                assert "Double-press a word" in text and "Press me" in text, reports
                assert "left out of copies" not in text, reports
                # Once the drag ends, the selection is the primary selection.
                primary = subprocess.run(["python3", "-c", PRIMARY], env=env, text=True, capture_output=True, timeout=10)
                assert primary.returncode == 0 and primary.stdout == text, (primary.stdout, primary.stderr, text)
                reports["primary"] = primary.stdout
                capture("drag")
                click(heading[0] + 20, heading[1] + heading[3] / 2, 2)
                reports["word"] = report()
                assert reports["word"]["text"] == "Selectable", reports
                click(intro[0] + 20, intro[1] + 8, 3)
                reports["paragraph"] = report()
                assert reports["paragraph"]["text"].startswith("Drag across") and reports["paragraph"]["text"].endswith("still clicks."), reports
                capture("paragraph")
                time.sleep(.6)
                xdo("key", "--clearmodifiers", "ctrl+a")
                reports["all"] = report()
                assert reports["all"]["text"].startswith("Selectable text") and "between paragraphs.\n\n00001\tA transcript line" in reports["all"]["text"], reports
                capture("select-all")
                click(600, 540)  # Blank room: a press there collapses it.
                reports["cleared"] = report()
                assert reports["cleared"]["text"] == "", reports
                # Held below the transcript, the drag scrolls it and selects
                # the lines it brings into view.
                transcript = reports["initial"]["transcript"]
                time.sleep(.6)
                move(transcript[0] + 80, transcript[1] + 10)
                xdo("mousedown", 1)
                move(transcript[0] + 80, transcript[1] + transcript[3] + 30)
                time.sleep(1.5)
                xdo("mouseup", 1)
                reports["autoscroll"] = report()
                # Copy takes the mounted rows: line 1's row was recycled as the
                # transcript scrolled past it, the selection kept.
                lines = [int(n) for n in re.findall(r"line (\d+)\)", reports["autoscroll"]["text"])]
                assert lines and lines[-1] > 20 and lines == list(range(lines[0], lines[-1] + 1)), reports["autoscroll"]
                capture("autoscroll")
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=60)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native selectable text checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
