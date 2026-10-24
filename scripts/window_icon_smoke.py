#!/usr/bin/env python3
"""A window's icon on X11 (Xvfb/Openbox, `linux_icon.rs`): window_probe, which
names no application id, takes its executable's icon from the hicolor
theme in a private `XDG_DATA_HOME` (a 48 pixel PNG of one colour), and its
window carries it as `_NET_WM_ICON`: 48 by 48, of that colour."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from PIL import Image

from platform_smoke import stop, wait_for_wm

COLOUR = (10, 120, 230, 255)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="the window_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    data = (args.output / "data").resolve()
    icon = data / "icons/hicolor/48x48/apps" / (args.probe.stem + ".png")
    icon.parent.mkdir(parents=True, exist_ok=True)
    Image.new("RGBA", (48, 48), COLOUR).save(icon)
    processes, checks = [], {}
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", XDG_DATA_HOME=str(data),
                   XDG_STATE_HOME=str((args.output / "state").resolve()),
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL))
        wait_for_wm(env)
        log = args.output / "window_probe.log"
        app = subprocess.Popen([str(args.probe), "saved"], env=env, stdout=log.open("w"), stderr=subprocess.STDOUT)
        processes.append(app)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        deadline = time.monotonic() + 15
        while not (window := run("xdotool", "search", "--onlyvisible", "--name", "^Saved$").split()):
            if time.monotonic() > deadline or app.poll() is not None:
                raise RuntimeError("the window did not show: " + log.read_text()[-2000:])
            time.sleep(.05)
        # The property's numbers: width, height, then a pixel each (ARGB).
        raw = run("xprop", "-id", window[0], "-f", "_NET_WM_ICON", "32c", "_NET_WM_ICON")
        values = [int(value) for value in re.findall(r"\d+", raw.split("=", 1)[-1])] if "=" in raw else []
        checks["icon size"] = values[:2]
        checks["has the icon"] = values[:2] == [48, 48] and len(values) == 2 + 48 * 48
        r, g, b, a = COLOUR
        checks["of its colour"] = bool(values[2:]) and values[2] == (a << 24 | r << 16 | g << 8 | b)
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}\n{raw[:300]}"
        print("PASS: an X11 window carries its app's icon from the icon theme")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
