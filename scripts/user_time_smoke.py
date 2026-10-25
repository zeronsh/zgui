#!/usr/bin/env python3
"""A window's last user input on X11 (Xvfb/Openbox): the vendored winit's
`_NET_WM_USER_TIME` (vendor/winit/ZGX_PATCH.md), which window managers
weigh against focus stealing, as GTK sets it.

Checks, with window_probe opening Second 3 s after First:

- before any input, First carries none (an app's first window is focused as
  launched);
- a click in First sets it, and a key pressed there later moves it on;
- Second, opened after, carries the time of that last press: a window the
  app opens long after the user turned to another app is weighed by when
  they last used it."""
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
    parser.add_argument("probe", type=pathlib.Path, help="the window_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", WINDOW_PROBE_SECOND_MS="3000",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL))
        wait_for_wm(env)
        log = args.output / "window_probe.log"
        app = subprocess.Popen([str(args.probe)], env=env, stdout=log.open("w"), stderr=subprocess.STDOUT)
        processes.append(app)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def window(name, seconds=15):
            deadline = time.monotonic() + seconds
            while not (found := run("xdotool", "search", "--onlyvisible", "--name", f"^{name}$").split()):
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError(f"{name} did not show: " + log.read_text()[-2000:])
                time.sleep(.05)
            return found[0]

        def user_time(id):
            found = re.search(r"= (\d+)", run("xprop", "-id", id, "_NET_WM_USER_TIME"))
            return int(found.group(1)) if found else None

        first = window("First")
        time.sleep(.3)
        checks["before any input"] = user_time(first)
        shell = dict(line.split("=", 1) for line in run("xdotool", "getwindowgeometry", "--shell",
                                                        first).splitlines())
        run("xdotool", "mousemove", int(shell["X"]) + 200, int(shell["Y"]) + 150, "click", 1)
        time.sleep(.3)
        checks["after a click"] = user_time(first)
        run("xdotool", "key", "a")
        time.sleep(.3)
        checks["after a key"] = user_time(first)
        second = window("Second")
        time.sleep(.3)
        checks["second window's"] = user_time(second)
        checks["none before input"] = checks["before any input"] is None
        checks["a click sets it"] = bool(checks["after a click"])
        checks["a key moves it on"] = (checks["after a key"] or 0) > (checks["after a click"] or 0)
        checks["a later window takes the last press's"] = checks["second window's"] == checks["after a key"]
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: X11 windows carry their last user input (_NET_WM_USER_TIME)")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
