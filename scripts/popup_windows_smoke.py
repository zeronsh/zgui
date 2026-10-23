#!/usr/bin/env python3
"""Popups past their window on X11 (Xvfb/Openbox), driving the popup_probe
example: a menu hanging past its window's bottom edge shows in a window of
its own, override-redirect, typed a popup menu and transient for its parent,
its rows past the parent's edge drawn and chosen there; a press on the bare
desktop closes it (the pointer grab a menu holds); and no popup window
exists before one is wanted, nor shows once it closed."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN = (1280, 1024)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the popup_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
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
        log = args.output / "application.log"
        app = launch("application", [str(args.binary.resolve())], env, stdout=log.open("w"))

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def until(what, test, seconds=15):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline or app.poll() is not None:
                    raise AssertionError(f"timed out waiting for {what}: {checks}\n{log.read_text()}")
                time.sleep(.05)

        def windows(visible):
            return run("xdotool", "search", *(["--onlyvisible"] if visible else []), "--pid", app.pid).split()

        def geometry(window):
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

        def state():
            lines = [line for line in log.read_text().splitlines() if line.startswith("OPEN ")]
            return lines[-1] if lines else ""

        until("READY", lambda: "READY" in log.read_text())
        main_window = until("the window", lambda: (windows(True) or [None])[0])
        run("xdotool", "windowactivate", "--sync", main_window)
        x, y, w, h = geometry(main_window)
        # Nothing made before a popup is near.
        checks["no popup window at rest"] = windows(False) == [main_window]
        assert checks["no popup window at rest"], windows(False)

        # The menu hangs past the window's bottom edge, in its own window.
        trigger = (x + 76, y + h - 30)
        run("xdotool", "mousemove", *trigger)
        time.sleep(.3)
        run("xdotool", "click", 1)
        until("the menu open", lambda: state().startswith("OPEN true"))
        popup = until("the menu's window",
                      lambda: next((win for win in windows(True) if win != main_window), None))
        px, py, pw, ph = geometry(popup)
        checks["menu window"] = [px, py, pw, ph]
        assert py + ph > y + h, f"the menu does not reach past the window: {checks}"
        attributes = run("xwininfo", "-id", popup)
        checks["override-redirect"] = "Override Redirect State: yes" in attributes
        checks["transient for its window"] = (
            int(main_window) == int(re.search(r"window id # (0x[0-9a-f]+)",
                                              run("xprop", "-id", popup, "WM_TRANSIENT_FOR")).group(1), 16))
        checks["popup menu type"] = "_NET_WM_WINDOW_TYPE_POPUP_MENU" in run("xprop", "-id", popup,
                                                                           "_NET_WM_WINDOW_TYPE")
        # A row past the window's edge is drawn: not the desktop's black.
        below = (px + pw // 2, y + h + (py + ph - y - h) // 2)
        shot = args.output / "open.png"
        run("import", "-window", "root", str(shot))
        from PIL import Image
        checks["drawn past the window"] = Image.open(shot).convert("RGB").getpixel(below) != (0, 0, 0)
        # The last item, past the window's edge, is chosen there.
        run("xdotool", "mousemove", px + pw // 2, py + ph - 20)
        time.sleep(.3)
        run("xdotool", "click", 1)
        until("the item chosen", lambda: 'CHOSEN "Item 8"' in log.read_text())
        until("the menu closed", lambda: state().startswith("OPEN false"))
        checks["chosen past the window"] = True
        until("its window hidden", lambda: windows(True) == [main_window], 5)

        # A press on the bare desktop closes it.
        run("xdotool", "mousemove", *trigger)
        time.sleep(.3)
        run("xdotool", "click", 1)
        until("the menu open again", lambda: state().startswith("OPEN true"))
        run("xdotool", "mousemove", SCREEN[0] - 5, 5)
        time.sleep(.3)
        run("xdotool", "click", 1)
        until("the menu closed by a press on the desktop", lambda: state().startswith("OPEN false"))
        checks["desktop press closes"] = True
        until("its window hidden", lambda: windows(True) == [main_window], 5)

        # At rest again: no wakeups to speak of.
        switches = lambda: sum(int(line.split()[1]) for task in pathlib.Path(f"/proc/{app.pid}/task").iterdir()
                               for line in (task / "status").read_text().splitlines()
                               if line.startswith("voluntary_ctxt_switches"))
        time.sleep(2)  # the menu's fade out
        before = switches()
        time.sleep(3)
        checks["wakeups at rest (3 s)"] = switches() - before
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        assert all(checks[key] for key in ("override-redirect", "transient for its window", "popup menu type",
                                           "drawn past the window")), checks
        assert checks["wakeups at rest (3 s)"] <= 1, checks
        print("PASS: a menu past its window on X11: its own override-redirect popup-menu window, "
              "drawn and chosen past the edge, closed by a press on the desktop, nothing at rest")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
