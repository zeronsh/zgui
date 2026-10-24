#!/usr/bin/env python3
"""Drags between windows on X11 (Xvfb/Openbox/picom): drag_probe's tab
dragged onto its other window's strip drops there; over another app's
window covering that strip (xmessage), it does not, and released there it
is torn off, as over the desktop; tab_windows' tab torn off opens in a
window of its own with the tab under the pointer where it was grabbed; and
that window's only tab moves the window itself (`drag_moves_window`),
following the pointer to the pixel, and dropped on the other window's strip
merges into it."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN = (2400, 1400)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=pathlib.Path, help="the zgx-desktop examples' directory")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log),
                                 stderr=options.pop("stderr", log), **options)
        processes.append(child)
        return child

    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)
        # A compositor, as desktops have: its overlay window covers the
        # screen, and the pointer goes through it.
        config = args.output / "picom.conf"
        config.write_text('backend = "xrender"; shadow = false; fading = false; unredir-if-possible = false;\n')
        compositor = launch("picom", ["picom", "--config", str(config)], env)
        time.sleep(.5)
        assert compositor.poll() is None, "picom exited"

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def until(what, test, app, log, seconds=15):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline or app.poll() is not None:
                    raise AssertionError(f"timed out waiting for {what}: {checks}\n{log.read_text()[-3000:]}")
                time.sleep(.05)

        def named(name):
            return run("xdotool", "search", "--onlyvisible", "--name", f"^{name}$").split()

        def geometry(window):
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

        def drag(start, end):
            run("xdotool", "mousemove", *start, "mousedown", 1)
            # Past the drag threshold, then on in steps, as a hand moves.
            run("xdotool", "mousemove", start[0] + 12, start[1] + 4)
            for step in range(1, 11):
                run("xdotool", "mousemove", start[0] + (end[0] - start[0]) * step // 10,
                    start[1] + (end[1] - start[1]) * step // 10)
                time.sleep(.03)
            time.sleep(.3)
            run("xdotool", "mouseup", 1)

        # drag_probe: a tab onto the other window's strip.
        log = args.output / "drag_probe.log"
        app = launch("drag_probe", [str(args.binaries / "drag_probe")], env, stdout=log.open("w"))
        source = until("the Drag window", lambda: named("Drag"), app, log)[0]
        other = until("the Other window", lambda: named("Other"), app, log)[0]
        run("xdotool", "windowactivate", "--sync", source)
        time.sleep(.5)
        sx, sy, _, _ = geometry(source)
        ox, oy, ow, _ = geometry(other)
        tab = (sx + 70, sy + 62)
        drag(tab, (ox + 200, oy + 40))
        until("the drop on the other window", lambda: "DROPPED tab" in log.read_text(), app, log)
        until("the tab's end", lambda: "TAB_END tab accepted=true" in log.read_text(), app, log)
        checks["drops on the other window"] = True

        # Another app's window over the other's strip: in front, it has the
        # drag, and the tab released there is torn off.
        cover = launch("xmessage", ["xmessage", "-geometry", f"300x200+{ox + ow - 320}+{oy - 40}",
                                    "another app"], env)
        until("the covering window", lambda: named("xmessage"), cover, log)
        time.sleep(.5)
        drag(tab, (ox + ow - 170, oy + 40))
        until("the tab torn off", lambda: "TORN tab" in log.read_text(), app, log)
        checks["not over another app's window"] = log.read_text().count("DROPPED tab") == 1
        stop(cover)
        stop(app)

        # tab_windows: a tab torn off opens under the pointer.
        log = args.output / "tab_windows.log"
        app = launch("tab_windows", [str(args.binaries / "tab_windows")], env, stdout=log.open("w"))
        one = until("window One", lambda: named("One"), app, log)[0]
        until("window Two", lambda: named("Two"), app, log)
        run("xdotool", "windowactivate", "--sync", one)
        time.sleep(.5)
        x, y, _, h = geometry(one)
        # The Inbox tab's centre: the strip's padding, half a tab, and the
        # tab's top less half its height.
        grab = (10 + 75, 6 + 16)
        release = (x + 200, y + h + 120)
        drag((x + grab[0], y + grab[1]), release)
        torn = until("the torn-off window", lambda: named("Inbox"), app, log)[0]
        time.sleep(.5)
        tx, ty, _, _ = geometry(torn)
        checks["torn-off content at"] = [tx, ty]
        checks["expected at"] = [release[0] - grab[0], release[1] - grab[1]]
        checks["torn off under the pointer"] = (abs(tx - release[0] + grab[0]) <= 1
                                                and abs(ty - release[1] + grab[1]) <= 1)

        # The torn-off window's only tab moves the window itself: it follows
        # the pointer, and left on the desktop stays there.
        start = (tx + grab[0], ty + grab[1])
        end = (start[0] + 160, start[1] + 90)
        drag(start, end)
        time.sleep(.5)
        mx, my, _, _ = geometry(torn)
        checks["moved window at"] = [mx, my]
        # The point under the pointer as the drag began (past `drag`'s
        # threshold step) stays under it.
        checks["only tab moves its window"] = (mx - tx, my - ty) == (end[0] - start[0] - 12, end[1] - start[1] - 4)
        checks["moved window stays"] = bool(named("Inbox"))
        # Onto the other window's strip: it merges there, its window gone.
        x, y, _, _ = geometry(one)
        drag((mx + grab[0], my + grab[1]), (x + 400, y + grab[1]))
        checks["merges into the other strip"] = bool(until("the moved window to merge", lambda: not named("Inbox"),
                                                           app, log))

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        assert all(checks[key] for key in ("not over another app's window", "torn off under the pointer",
                                           "only tab moves its window", "moved window stays",
                                           "merges into the other strip")), checks
        print("PASS: X11 drags between windows: dropped on the other window's strip, not through another app's "
              "window, a tab torn off opens under the pointer, and a window's only tab moves it and merges")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
