#!/usr/bin/env python3
"""A new window fits its monitor's work area on X11 (Xvfb/Openbox), though
the window manager would show it past the screen's edge: on a 1024x700
screen whose top 40 pixels a panel reserves (`_NET_WM_STRUT`), the gallery
(decorated) and the kitchen sink (its own title bar, undecorated) each fit,
frame and all, below the panel."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN, PANEL = (1024, 700), 40

DOCK = r'''
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
panel = Gtk.Window(title="panel")
panel.set_type_hint(Gdk.WindowTypeHint.DOCK)
panel.set_decorated(False)
panel.move(0, 0)
panel.set_default_size(%d, %d)
panel.show_all()
print("PANEL", panel.get_window().get_xid(), flush=True)
Gtk.main()
''' % (SCREEN[0], PANEL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=pathlib.Path, help="the zgx-desktop examples directory")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    checks = {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
               DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
    env.pop("WAYLAND_DISPLAY", None)
    try:
        xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)

        def run(*argv):
            return subprocess.check_output(list(map(str, argv)), env=env, text=True).strip()

        panel = launch("panel", ["/usr/bin/python3", "-c", DOCK], env, stdout=subprocess.PIPE, text=True)
        line = panel.stdout.readline() if select.select([panel.stdout], [], [], 10)[0] else ""
        if not line.startswith("PANEL"):
            raise RuntimeError("the panel did not start; see panel.log")
        # A panel reserves the screen's top (EWMH struts).
        run("xprop", "-id", line.split()[1], "-f", "_NET_WM_STRUT", "32c", "-set", "_NET_WM_STRUT",
            f"0, 0, {PANEL}, 0")
        deadline = time.monotonic() + 10
        while f"0, {PANEL}, {SCREEN[0]}, {SCREEN[1] - PANEL}" not in run("xprop", "-root", "_NET_WORKAREA"):
            if time.monotonic() > deadline:
                raise AssertionError("the panel's strut never reached _NET_WORKAREA: "
                                     + run("xprop", "-root", "_NET_WORKAREA"))
            time.sleep(.05)

        for example in ("gallery", "kitchen_sink"):
            app = launch(example, [str(args.binaries / example)], env)
            deadline = time.monotonic() + 20
            while True:
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                       env=env, capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window = found.stdout.split()[0]
                    break
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError(f"{example}'s window did not appear")
                time.sleep(.05)
            time.sleep(.5)
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            x, y = value("Absolute upper-left X"), value("Absolute upper-left Y")
            w, h = value("Width"), value("Height")
            extents = run("xprop", "-id", window, "_NET_FRAME_EXTENTS")
            left, right, top, bottom = map(int, re.findall(r"\d+", extents.split("=")[-1])) \
                if "=" in extents else (0, 0, 0, 0)
            outer = (x - left, y - top, w + left + right, h + top + bottom)
            checks[example] = {"inner": [w, h], "frame": [left, right, top, bottom], "outer": list(outer)}
            assert outer[0] >= 0 and outer[2] <= SCREEN[0], checks
            assert outer[1] >= PANEL and outer[1] + outer[3] <= SCREEN[1], checks
            stop(app)
        (args.output / "result.json").write_text(json.dumps({"result": "pass", "checks": checks}, indent=2) + "\n")
        print("PASS: decorated and undecorated windows fit the work area below a panel")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
