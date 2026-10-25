#!/usr/bin/env python3
"""Drag previews in windows of their own on X11 (Xvfb/Openbox/picom),
driving the drag_drop example: a card's preview shows in an override-redirect
window, drawn, that the pointer passes through to what is under it; it
follows the pointer past its window's edge with nothing drawn again, neither
it nor the window (the X server moves it); a drag nothing takes slides it
back to its card, and one taken there goes at once; and nothing runs at
rest after."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN = (1200, 900)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the drag_drop example")
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
        xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)
        # A compositor, as desktops have: windows a preview uncovers keep
        # their pixels (no Expose to repaint).
        config = args.output / "picom.conf"
        config.write_text('backend = "xrender"; shadow = false; fading = false; unredir-if-possible = false;\n')
        compositor = launch("picom", ["picom", "--config", str(config)], env)
        time.sleep(.5)
        assert compositor.poll() is None, "picom exited"
        log = args.output / "application.log"
        app = launch("application", [str(args.binary.resolve())], dict(env, ZGX_PRESENT_LOG="1"),
                     stdout=log.open("w"), stderr=subprocess.STDOUT)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def until(what, test, seconds=15):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline or app.poll() is not None:
                    raise AssertionError(f"timed out waiting for {what}: {checks}\n{log.read_text()[-3000:]}")
                time.sleep(.02)

        def windows():
            return run("xdotool", "search", "--onlyvisible", "--pid", app.pid).split()

        def geometry(window):
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

        def draws():
            lines = log.read_text().splitlines()
            return (sum(" render " in line for line in lines), sum(" popup draw " in line for line in lines))

        main_window = until("the window", lambda: run("xdotool", "search", "--name", "^zgx typed drag$").split())[0]
        run("xdotool", "windowactivate", "--sync", main_window)
        time.sleep(.5)
        x, y, w, h = geometry(main_window)

        def at(wx, wy):
            return x + wx, y + wy

        # A drag from the Red card: its preview in a window of its own.
        run("xdotool", "mousemove", *at(60, 110), "mousedown", 1)
        run("xdotool", "mousemove", *at(90, 110))
        preview = until("the preview's window", lambda: next((win for win in windows() if win != main_window), None))
        time.sleep(.5)
        px, py, pw, ph = geometry(preview)
        checks["preview window"] = [px, py, pw, ph]
        checks["override-redirect"] = "Override Redirect State: yes" in run("xwininfo", "-id", preview)
        shot = args.output / "preview.png"
        run("import", "-window", "root", str(shot))
        from PIL import Image
        red = Image.open(shot).convert("RGB").getpixel((px + pw // 2, py + ph - 12))
        checks["drawn"] = red[0] > 90 and red[0] > red[1] + 40
        # The pointer goes through it: what is under it is the window.
        under = re.search(r"window:(\d+)", run("xdotool", "getmouselocation")).group(1)
        checks["pointer passes through"] = under != preview

        # Past the window's right edge, in steps: the server moves it.
        before = draws()
        start = (px, py)
        for step in range(1, 31):
            run("xdotool", "mousemove", *at(90 + step * 20, 110 + step * 2))
            time.sleep(.02)
        time.sleep(.3)
        qx, qy, _, _ = geometry(preview)
        checks["follows the pointer"] = (qx - start[0], qy - start[1]) == (600, 60)
        checks["past the window"] = qx > x + w
        renders, popup_draws = (now - then for now, then in zip(draws(), before))
        checks["window renders while it moves"] = renders
        checks["preview draws while it moves"] = popup_draws

        # Released on nothing: it slides back to its card, and hides.
        run("xdotool", "mousemove", *at(470, 330))
        time.sleep(.2)
        ox, oy, _, _ = geometry(preview)
        before = draws()
        run("xdotool", "mouseup", 1)
        path = []
        deadline = time.monotonic() + 1
        while preview in windows() and time.monotonic() < deadline:
            at_ = geometry(preview)[:2]
            # Hidden between the two calls, it is parked off screen.
            if at_[0] > -5000:
                path.append(at_)
        checks["slide path"] = path
        # Home is the card's top-left (the preview has no shadow to offset).
        home = at(20, 70)
        distances = [abs(home[0] - sx) + abs(home[1] - sy) for sx, sy in [(ox, oy)] + path]
        checks["slides back"] = (len(set(path)) >= 3 and distances == sorted(distances, reverse=True)
                                 and distances[-1] <= distances[0] // 8)
        until("its window hidden", lambda: preview not in windows(), 2)
        until("the drag's end", lambda: "DRAG_END Red false" in log.read_text())
        checks["window renders while it slides"] = draws()[0] - before[0]

        # Dropped on the destination: taken, the preview goes at once.
        time.sleep(.3)
        run("xdotool", "mousemove", *at(60, 110), "mousedown", 1)
        run("xdotool", "mousemove", *at(90, 110))
        preview = until("the preview's window again",
                        lambda: next((win for win in windows() if win != main_window), None))
        run("xdotool", "mousemove", *at(150, 230))
        time.sleep(.3)
        run("xdotool", "mouseup", 1)
        until("the drop", lambda: "DRAG_DROP Red" in log.read_text() and "DRAG_END Red true" in log.read_text())
        until("its window hidden at once", lambda: windows() == [main_window], .3)
        checks["taken goes at once"] = True

        # At rest again: no wakeups to speak of.
        switches = lambda: sum(int(line.split()[1]) for task in pathlib.Path(f"/proc/{app.pid}/task").iterdir()
                               for line in (task / "status").read_text().splitlines()
                               if line.startswith("voluntary_ctxt_switches"))
        time.sleep(2)
        before = switches()
        time.sleep(3)
        checks["wakeups at rest (3 s)"] = switches() - before
        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        assert all(checks[key] for key in ("override-redirect", "drawn", "pointer passes through",
                                           "follows the pointer", "past the window", "slides back")), checks
        assert checks["window renders while it moves"] == 0, checks
        assert checks["preview draws while it moves"] == 0, checks
        assert checks["window renders while it slides"] <= 1, checks
        assert checks["wakeups at rest (3 s)"] <= 1, checks
        print("PASS: an X11 drag preview in its own override-redirect window, passed through by the pointer, "
              "moved by the server with nothing drawn, sliding back when nothing takes it")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
