#!/usr/bin/env python3
"""Text dragged into zgx from another app on X11 (Xvfb/Openbox, winit's XDND
destination, `platform::text_drop`): a GTK window of the probe's own
(`text_drag_source.py`) drags text, with real input, onto drag_probe's Words
field. The display is the probe's own: nothing else is under the pointer.

Checks that:

- over the field the drag is taken (the source asks for no other place),
  and hovering changes nothing; let go there, the text lands at the drop
  point (the field's end) and the source hears it taken;
- let go over the chip (no destination there), nothing is dropped and the
  source hears it refused;
- Escape over the field cancels: nothing is dropped."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN = (1600, 1000)
# In drag_probe's Drag window: the chip's centre, and the Words field's far
# end (its left 10, 240 wide; 104 down, 30 high).
CHIP = (10 + 60, 10 + 14)
FIELD_END = (10 + 230, 104 + 15)
START = "drag these words"


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
        if not select.select([xvfb.stdout], [], [], 30)[0]:
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
                    raise AssertionError(f"timed out waiting for {what}: {checks}\n{app_log.read_text()[-3000:]}\n"
                                         f"{source_log.read_text()[-2000:]}")
                time.sleep(.05)

        def named(name):
            return run("xdotool", "search", "--onlyvisible", "--name", f"^{name}$").split()

        def geometry(window):
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

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

        def words():
            return re.findall(r'^WORDS "(.*)"$', app_log.read_text(), re.M)

        def lines(pattern):
            return len(re.findall(pattern, source_log.read_text(), re.M))

        app, app_log = launch("drag_probe", [args.binaries / "drag_probe"], env)
        window = until("the Drag window", lambda: named("Drag"))[0]
        until("the field's first value", lambda: words())
        wx, wy, _, wh = geometry(window)
        source, source_log = launch("source", ["python3", "scripts/text_drag_source.py", "Text source",
                                               wx, wy + wh + 80, " and more"], env)
        source_window = until("the source window", lambda: named("Text source"))[0]
        until("the source ready", lambda: "READY" in source_log.read_text())
        time.sleep(.5)

        def grab():
            """Press on the source's button and pull it past GTK's threshold."""
            sx, sy, sw, sh = geometry(source_window)
            button = (sx + sw // 2, sy + sh // 2)
            run("xdotool", "mousemove", *button, "mousedown", 1)
            glide(button, (button[0] + 20, button[1] - 10), 4)
            return (button[0] + 20, button[1] - 10)

        chip, field = (wx + CHIP[0], wy + CHIP[1]), (wx + FIELD_END[0], wy + FIELD_END[1])

        # Over the chip, then the field: let go there, it drops at the end.
        at = grab()
        glide(at, chip)
        time.sleep(.3)
        glide(chip, field)
        time.sleep(.4)
        checks["hovering changes nothing"] = words() == [START]
        assert over(window), f"the pointer is not over the probe's window: {pointer()}"
        run("xdotool", "mouseup", 1)
        until("the drop", lambda: len(words()) > 1)
        until("the drag's end", lambda: lines(r"^END$") == 1)
        checks["dropped"] = words()[-1]
        checks["the text lands at the field's end"] = words()[-1] == START + " and more"
        checks["asked for UTF-8 text"] = re.search(r"^SENT (\S+)$", source_log.read_text(), re.M).group(1)
        checks["the source hears it taken"] = lines(r"^FAILED") == 0

        # Let go over the chip: refused.
        time.sleep(.5)
        at = grab()
        glide(at, chip)
        time.sleep(.4)
        assert over(window), "the pointer is not over the probe's window"
        run("xdotool", "mouseup", 1)
        until("the drag's end", lambda: lines(r"^END$") == 2)
        time.sleep(.3)
        checks["over the chip: nothing dropped"] = len(words()) == 2
        checks["over the chip: refused"] = lines(r"^FAILED") == 1

        # Escape over the field: cancelled.
        time.sleep(.5)
        at = grab()
        glide(at, field)
        time.sleep(.4)
        assert over(window), "the pointer is not over the probe's window"
        run("xdotool", "key", "Escape")
        until("the drag's end", lambda: lines(r"^END$") == 3)
        run("xdotool", "mouseup", 1)
        time.sleep(.5)
        checks["escape: nothing dropped"] = len(words()) == 2
        checks["app alive"] = app.poll() is None

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: X11 text drops into zgx: a GTK source's text lands where it was let go over a field; "
              "over the chip it is refused, and Escape drops nothing")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
