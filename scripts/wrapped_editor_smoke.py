#!/usr/bin/env python3
"""Validate responsive wrapped editing on an owned X11 desktop."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
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
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        time.sleep(.4)
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
            raise RuntimeError("wrapped editor window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.2)
        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.08)
        def key(*keys):
            xdo("key", "--clearmodifiers", *keys)
            time.sleep(.08)
        from PIL import ImageGrab
        def report(stage):
            click(70, 244)
            ImageGrab.grab(xdisplay=display).save(args.output / f"wrapped-{stage}.png")
        click(50, 80)
        key("ctrl+Home", "Right", "Right", "Down", "shift+Down")
        report(1)
        click(50, 80)
        key("ctrl+End")
        xdo("type", "--clearmodifiers", "!")
        report(2)
        click(50, 80)
        key("ctrl+End")
        xdo("windowsize", window, 420, 330)
        time.sleep(.3)
        report(3)
        click(50, 80)
        key("ctrl+Home")
        click(42, 94)
        xdo("type", "--clearmodifiers", "CLICK")
        report(4)
        click(50, 80)
        key("ctrl+a")
        xdo("type", "--clearmodifiers", "Short wrapped text")
        report(5)
        output, _ = app.communicate(timeout=15)
        (args.output / "wrapped-editor.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        pattern = r'WRAP (\d+) width=([\d.]+) text_height=([\d.]+) anchor=(\d+) focus=(\d+) caret_y=([-\d.]+) model=(.*)'
        rows=[]
        for match in re.finditer(pattern, output):
            stage,width,height,anchor,focus,caret,model=match.groups()
            rows.append(dict(stage=int(stage),width=float(width),height=float(height),anchor=int(anchor),focus=int(focus),caret_y=float(caret),model=json.loads(model)))
        assert len(rows)==5, output
        a,b,c,d,e=rows
        assert a["focus"]>a["anchor"]>2, rows
        assert b["model"]==a["model"]+"!", rows
        assert b["focus"]==b["anchor"]==len(b["model"]), rows
        assert c["width"]==380 and c["height"]>b["height"], rows
        assert c["model"]==b["model"] and c["focus"]==b["focus"], rows
        assert 62 <= c["caret_y"] <= 184, rows
        marker=d["model"].index("CLICK")
        assert 0<marker<150 and d["model"].replace("CLICK", "", 1)==c["model"], rows
        assert e["model"]=="Short wrapped text" and e["height"]<c["height"], rows
        (args.output / "results.json").write_text(json.dumps({"passed": True, "backend": "X11", "stages": rows},indent=2)+"\n")
        print("Native wrapped-editor checks passed")
    finally:
        for process in (app, wm, xvfb):
            stop(process)


if __name__ == "__main__":
    main()
