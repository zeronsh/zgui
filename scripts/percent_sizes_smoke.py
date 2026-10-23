#!/usr/bin/env python3
"""Resize a native percentage-sized editor and verify bounds, input and pixels."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from PIL import Image
from platform_smoke import stop

INITIAL = "Resize this window. Both panels keep half the available width, and this editor wraps without losing its text or selection."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-percent-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1000x700x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            time.sleep(.4)
            with (args.output / "percent-sizes.log").open("w") as log:
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
                    raise RuntimeError("percentage-size window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def click(x, y):
                    xdo("mousemove", "--window", window, x, y, "click", 1)
                    time.sleep(.1)
                def report():
                    click(75, 32)
                def insert(character):
                    click(33, 69)
                    xdo("type", "--clearmodifiers", character)
                    time.sleep(.15)
                insert("A")
                report()
                for stage, width, height in [(1, 640, 420), (2, 440, 340), (3, 800, 500)]:
                    if stage > 1:
                        xdo("windowsize", "--sync", window, width, height)
                        time.sleep(.4)
                        report()  # Resizing alone preserves the model and selection.
                        insert("B" if stage == 2 else "C")
                        report()  # Physical input uses the newly allocated editor.
                    screenshot = args.output / f"percent-{stage}.png"
                    subprocess.run(["import", "-window", window, str(screenshot)], env=env, check=True, timeout=10)
                    image = Image.open(screenshot).convert("RGB")
                    assert image.size == (width, height), image.size
                    assert image.getpixel((40, height - 35)) == (32, 48, 80)
                    assert image.getpixel((width // 2 + 20, height - 35)) == (40, 59, 50)
                app.wait(timeout=16)
            output = (args.output / "percent-sizes.log").read_text()
            assert app.returncode == 0, output
            rows = []
            for line in output.splitlines():
                if not line.startswith("PERCENT "):
                    continue
                dimensions = re.search(r"viewport=\(([\d.]+), ([\d.]+)\)", line)
                row = {"viewport": [float(value) for value in dimensions.groups()]}
                for name in ("split", "left", "right", "caret"):
                    match = re.search(name + r"=Rect \{ x: ([\d.-]+), y: ([\d.-]+), width: ([\d.]+), height: ([\d.]+) \}", line)
                    row[name] = dict(zip(("x", "y", "width", "height"), map(float, match.groups())))
                row["focus"] = int(re.search(r"focus=(\d+)", line).group(1))
                row["model"] = json.loads(line.split(" model=", 1)[1])
                rows.append(row)
            assert len(rows) == 5, output
            expected_sizes = [(640, 420), (440, 340), (440, 340), (800, 500), (800, 500)]
            for row, (width, height), prefix in zip(rows, expected_sizes, ["A", "A", "BA", "BA", "CBA"]):
                assert row["viewport"] == [width, height], row
                assert row["split"] == dict(x=20., y=56., width=width-40., height=height-76.), row
                assert row["left"] == dict(x=20., y=56., width=(width-40.)/2, height=height-76.), row
                assert row["right"] == dict(x=width/2., y=56., width=(width-40.)/2, height=height-76.), row
                assert row["model"] == prefix + INITIAL and row["focus"] == 1, row
                assert row["left"]["x"] <= row["caret"]["x"] < row["right"]["x"], row
                assert row["left"]["y"] <= row["caret"]["y"] < height-20, row
            (args.output / "results.json").write_text(json.dumps({"passed": True, "backend": "owned X11/Xvfb/Openbox", "reports": rows}, indent=2) + "\n")
            print("Native percentage-size checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
