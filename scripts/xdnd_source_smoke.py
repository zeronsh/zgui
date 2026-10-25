#!/usr/bin/env python3
"""Drags out of zgx on X11 (Xvfb/Openbox, `native_drag/x11.rs`): drag_probe's
chip, exporting its text, dragged with real input out of its window onto
test-owned GTK drop targets (`xdnd_target.py`) below it. It never drops on
anything else: the display is the probe's own, and before letting go it
checks that the pointer is over the target it means.

Checks that:

- the target takes the chip's text over XDND, and the source hears its drag
  accepted (`DRAG_END accepted=true`); out of the window the preview follows
  the pointer pixel for pixel, and neither it nor the window draws a frame;
- dragged out over the target and back into its own window and let go
  there, the drag stays zgx's: the target gets nothing, and the source
  hears it not accepted;
- Escape over the target cancels: nothing is dropped, not accepted;
- over a target that refuses it (and hears its positions), let go there,
  nothing is dropped, not accepted."""
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
# The chip's centre in drag_probe's Drag window: its padding and half the chip.
CHIP = (10 + 60, 10 + 14)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=pathlib.Path, help="the zgx-desktop examples' directory")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env):
        log = args.output / (name + ".log")
        child = subprocess.Popen(list(map(str, command)), env=env, stdout=log.open("w"), stderr=subprocess.STDOUT)
        processes.append(child)
        return child, log

    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", GDK_SCALE="1", XDG_RUNTIME_DIR="/tmp",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent", NO_AT_BRIDGE="1")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"],
                                env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def until(what, test, seconds=15):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline:
                    raise AssertionError(f"timed out waiting for {what}: {checks}\n{app_log.read_text()[-3000:]}")
                time.sleep(.05)

        def named(name):
            return run("xdotool", "search", "--onlyvisible", "--name", f"^{name}$").split()

        def geometry(window):
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

        def centre(window):
            x, y, w, h = geometry(window)
            return x + w // 2, y + h // 2

        def pointer():
            out = run("xdotool", "getmouselocation", "--shell")
            return int(re.search(r"X=(\d+)", out).group(1)), int(re.search(r"Y=(\d+)", out).group(1))

        def over(window):
            (x, y), (left, top, w, h) = pointer(), geometry(window)
            return left <= x < left + w and top <= y < top + h

        def glide(start, end, steps=12):
            for step in range(1, steps + 1):
                run("xdotool", "mousemove", start[0] + (end[0] - start[0]) * step // steps,
                    start[1] + (end[1] - start[1]) * step // steps)
                time.sleep(.03)

        def grab():
            """Press on the chip and pull it past the drag threshold."""
            sx, sy, _, _ = geometry(source)
            chip = (sx + CHIP[0], sy + CHIP[1])
            run("xdotool", "mousemove", *chip, "mousedown", 1)
            run("xdotool", "mousemove", chip[0] + 12, chip[1] + 4)
            time.sleep(.05)
            return chip

        def count(pattern, log):
            return len(re.findall(pattern, log.read_text(), re.M))

        app, app_log = launch("drag_probe", [str(args.binaries / "drag_probe")], dict(env, ZGX_PRESENT_LOG="1"))

        def draws():
            lines = app_log.read_text().splitlines()
            return (sum(" render " in line for line in lines), sum(" popup draw " in line for line in lines))
        source = until("the Drag window", lambda: named("Drag"))[0]
        other = until("the Other window", lambda: named("Other"))[0]
        sx, sy, sw, sh = geometry(source)
        takes, takes_log = launch("takes", ["python3", "scripts/xdnd_target.py", "Takes drops",
                                            sx, sy + sh + 120], env)
        refuses, refuses_log = launch("refuses", ["python3", "scripts/xdnd_target.py", "Refuses drops",
                                                  sx + 460, sy + sh + 120, "--refuse"], env)
        target = until("the taking target", lambda: named("Takes drops"))[0]
        refusing = until("the refusing target", lambda: named("Refuses drops"))[0]
        until("the targets ready", lambda: "READY" in takes_log.read_text() and "READY" in refuses_log.read_text())
        run("xdotool", "windowactivate", "--sync", source)
        time.sleep(.5)

        # Onto the target: it takes the text. Out of the window, the drag is
        # X11's; the preview follows the pointer, nothing drawn.
        chip = grab()
        below = (sx + sw // 2, sy + sh + 40)
        glide(chip, below)
        time.sleep(.3)
        others = {source, other}
        preview = until("the preview", lambda: next((window for window in run(
            "xdotool", "search", "--onlyvisible", "--pid", app.pid).split() if window not in others), None))
        before, then = draws(), geometry(preview)[:2]
        glide(below, centre(target))
        time.sleep(.3)
        now = geometry(preview)[:2]
        moved = (now[0] - then[0], now[1] - then[1])
        checks["preview follows the pointer"] = moved == (centre(target)[0] - below[0], centre(target)[1] - below[1])
        checks["preview moved"] = moved
        renders, popup_draws = (now - then for now, then in zip(draws(), before))
        checks["window renders as it moves out"] = renders
        checks["preview draws as it moves out"] = popup_draws
        assert over(target), f"the pointer is not over the test target: {pointer()} {geometry(target)}"
        run("xdotool", "mouseup", 1)
        until("the drop's text", lambda: "RECEIVED" in takes_log.read_text())
        until("the drag's end", lambda: count(r"^DRAG_END", app_log) == 1)
        received = re.search(r"^RECEIVED (\S+) '(.*)'$", takes_log.read_text(), re.M)
        checks["received"] = received.group(1, 2) if received else None
        checks["the target takes the text"] = bool(received) and received.group(2) == "Some text"
        checks["accepted"] = count(r"^DRAG_END accepted=true$", app_log) == 1

        # Out over the target and back home: zgx's, nothing dropped.
        time.sleep(.5)
        chip = grab()
        glide(chip, centre(target))
        time.sleep(.3)
        glide(centre(target), (sx + sw // 2, sy + sh // 2))
        time.sleep(.3)
        assert over(source), "the pointer is not back over its window"
        run("xdotool", "mouseup", 1)
        until("the drag's end", lambda: count(r"^DRAG_END", app_log) == 2)
        time.sleep(.5)
        checks["back home: nothing dropped"] = count(r"^RECEIVED", takes_log) == 1
        checks["back home: not accepted"] = count(r"^DRAG_END accepted=false$", app_log) == 1

        # Escape over the target: cancelled.
        chip = grab()
        glide(chip, centre(target))
        time.sleep(.3)
        assert over(target), "the pointer is not over the test target"
        run("xdotool", "key", "Escape")
        until("the drag's end", lambda: count(r"^DRAG_END", app_log) == 3)
        run("xdotool", "mouseup", 1)
        time.sleep(.5)
        checks["escape: nothing dropped"] = count(r"^RECEIVED", takes_log) == 1
        checks["escape: not accepted"] = count(r"^DRAG_END accepted=false$", app_log) == 2

        # Over a target that refuses it: not dropped.
        chip = grab()
        glide(chip, centre(refusing))
        time.sleep(.3)
        assert over(refusing), "the pointer is not over the refusing target"
        run("xdotool", "mouseup", 1)
        until("the drag's end", lambda: count(r"^DRAG_END", app_log) == 4)
        time.sleep(.3)
        checks["refused: heard positions"] = "MOTION" in refuses_log.read_text()
        checks["refused: nothing dropped"] = "RECEIVED" not in refuses_log.read_text()
        checks["refused: not accepted"] = count(r"^DRAG_END accepted=false$", app_log) == 3
        checks["app alive"] = app.poll() is None

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        failed += [key for key in ("window renders as it moves out", "preview draws as it moves out") if checks[key]]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: X11 drags out of zgx: a GTK target takes the text; back home, Escape and a refusing "
              "target drop nothing")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
