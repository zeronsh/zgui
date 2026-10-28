#!/usr/bin/env python3
"""Verify native inherited letter-spacing changes preserve editor selection and text."""
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
    with tempfile.TemporaryDirectory(prefix="zgx-letter-spacing-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
                if display == ":":
                    raise RuntimeError("Xvfb startup failed")
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            def xdo(*arguments):
                return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline and app.poll() is None:
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window = found.stdout.splitlines()[0]
                    break
                time.sleep(.05)
            if window is None:
                raise RuntimeError("Letter-spacing window did not open")
            xdo("windowactivate", "--sync", window)
            time.sleep(.2)
            def click(x, y):
                xdo("mousemove", "--window", window, x, y, "click", 1)
                time.sleep(.15)
            click(50, 125)
            xdo("key", "--clearmodifiers", "ctrl+Home", "Right", "Right", "Right")
            time.sleep(.15)
            from PIL import ImageGrab
            for stage, button_x in enumerate([None, 178, 286, 70], 1):
                if button_x is not None:
                    click(button_x, 236)
                click(394, 236)
                ImageGrab.grab(xdisplay=display).save(args.output / f"letter-spacing-{stage}.png")
            output, _ = app.communicate(timeout=15)
            (args.output / "letter-spacing.log").write_text(output)
            if app.returncode:
                raise RuntimeError(output)
            pattern = r"TRACKING spacing=(-?[\d.]+) x=([\d.]+) width=([\d.]+) anchor=(\d+) focus=(\d+) model=(.*)"
            rows = []
            for match in re.finditer(pattern, output):
                spacing, x, width, anchor, focus, model = match.groups()
                rows.append(dict(spacing=float(spacing), x=float(x), width=float(width), anchor=int(anchor), focus=int(focus), model=json.loads(model)))
            assert len(rows) == 4, output
            assert [row["spacing"] for row in rows] == [0., 3., -1., 0.], rows
            for row, delta in zip(rows, [0., 9., -3., 0.]):
                assert row["anchor"] == row["focus"] == 3, rows
                assert row["model"] == "ABCDEF", rows
                assert abs(row["x"] - rows[0]["x"] - delta) < .1, rows
            assert rows[1]["width"] > rows[0]["width"] > rows[2]["width"], rows
            assert rows[3]["width"] == rows[0]["width"], rows
            (args.output / "results.json").write_text(json.dumps({"passed": True, "backend": "X11", "stages": rows}, indent=2) + "\n")
            print("Native letter-spacing checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
