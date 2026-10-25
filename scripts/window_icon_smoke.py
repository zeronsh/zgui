#!/usr/bin/env python3
"""A window's icon and class on X11 (Xvfb/Openbox, `linux_icon.rs`):
window_probe, which names no application id, takes its executable's icon
from the hicolor theme in a private `XDG_DATA_HOME` (a 48 pixel PNG of one
colour), and its window carries it as `_NET_WM_ICON`: 48 by 48, of that
colour. Run again with a desktop file ID (`Application::desktop_id`, as
zgx-run gives each script's `app_id`), its window's class is that ID and
its icon the one of that name (another colour)."""
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
# A desktop file ID, as zgx-run gives a script's `app_id`, and its icon's.
DESKTOP_ID, DESKTOP_COLOUR = "zgx-probe-app", (200, 60, 30, 255)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="the window_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    data = (args.output / "data").resolve()
    for name, colour in ((args.probe.stem, COLOUR), (DESKTOP_ID, DESKTOP_COLOUR)):
        icon = data / "icons/hicolor/48x48/apps" / (name + ".png")
        icon.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGBA", (48, 48), colour).save(icon)
    processes, checks = [], {}
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", XDG_DATA_HOME=str(data),
                   XDG_STATE_HOME=str((args.output / "state").resolve()),
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL))
        wait_for_wm(env)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def shown(name, desktop_id=None):
            """The window's class and its icon's numbers: width, height, then
            a pixel each (ARGB)."""
            log = args.output / f"{name}.log"
            app_env = dict(env, WINDOW_PROBE_DESKTOP_ID=desktop_id) if desktop_id else env
            app = subprocess.Popen([str(args.probe), "saved"], env=app_env, stdout=log.open("w"),
                                   stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 15
                while not (window := run("xdotool", "search", "--onlyvisible", "--name", "^Saved$").split()):
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("the window did not show: " + log.read_text()[-2000:])
                    time.sleep(.05)
                raw = run("xprop", "-id", window[0], "-f", "_NET_WM_ICON", "32c", "_NET_WM_ICON")
                values = [int(value) for value in re.findall(r"\d+", raw.split("=", 1)[-1])] if "=" in raw else []
                return re.findall(r'"([^"]*)"', run("xprop", "-id", window[0], "WM_CLASS")), values
            finally:
                stop(app)

        def argb(colour):
            r, g, b, a = colour
            return a << 24 | r << 16 | g << 8 | b

        # By its executable's name.
        class_, values = shown("executable")
        checks["icon size"] = values[:2]
        checks["has the icon"] = values[:2] == [48, 48] and len(values) == 2 + 48 * 48
        checks["of its colour"] = bool(values[2:]) and values[2] == argb(COLOUR)
        checks["class"] = class_
        checks["classed by its executable"] = class_ == [args.probe.stem] * 2
        # By its desktop file ID.
        class_, values = shown("desktop-id", DESKTOP_ID)
        checks["desktop id class"] = class_
        checks["classed by its desktop file ID"] = class_ == [DESKTOP_ID] * 2
        checks["the icon of that name"] = bool(values[2:]) and values[2] == argb(DESKTOP_COLOUR)
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: an X11 window carries its app's class and icon, by its executable or desktop file ID")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
