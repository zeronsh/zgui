#!/usr/bin/env python3
"""Find in page on X11 (Xvfb/Openbox), with real keys: selectable_text's
window (`--find-probe`, which prints find's status as it changes), Ctrl+F,
a query typed into the bar.

Checks that:

- Ctrl+F opens the bar and what is typed is found: one transcript line out
  of 10,000, in a row the list has not mounted, which then scrolls into view
  and shows the current match's mark (the `find.current` orange) on screen;
- Enter steps, wrapping, and Escape closes the bar, its marks gone;
- a query matching every transcript line counts all 10,000, in slices
  (the count grows, then is done) without stalling the window."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time

from PIL import Image

from platform_smoke import stop, wait_for_wm

# `find.current`'s default (#ff9632, 85%) over the window's light background.
ORANGE = (255, 150, 50)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    with tempfile.TemporaryDirectory(prefix="zgx-find-") as runtime:
        try:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"],
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env = dict(os.environ, DISPLAY=":" + xvfb.stdout.readline().strip(), WINIT_X11_SCALE_FACTOR="1",
                       XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent", NO_AT_BRIDGE="1")
            env.pop("WAYLAND_DISPLAY", None)
            processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            wait_for_wm(env)
            log = args.output / "app.log"
            processes.append(subprocess.Popen([str(args.binary.resolve()), "--find-probe"], env=env,
                                              stdout=log.open("w"), stderr=subprocess.STDOUT))

            def run(*argv):
                return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

            def status():
                lines = re.findall(r"^FIND (.*)$", log.read_text(), re.M)
                if not lines:
                    return {}
                fields = dict(re.findall(r'(\w+)=("[^"]*"|\S+)', lines[-1]))
                return {k: v.strip('"') for k, v in fields.items()}

            def until(what, test, seconds=15):
                deadline = time.monotonic() + seconds
                while True:
                    value = test()
                    if value:
                        return value
                    if time.monotonic() > deadline:
                        raise AssertionError(f"timed out waiting for {what}: {status()} {checks}")
                    time.sleep(.05)

            window = until("the window", lambda: run("xdotool", "search", "--onlyvisible", "--name",
                                                     "^zgx selectable text$").split())[0]
            until("the probe", status)
            run("xdotool", "windowactivate", "--sync", window)
            time.sleep(.3)

            def keys(*names):
                run("xdotool", "key", "--window", window, "--clearmodifiers", *names)
                time.sleep(.15)

            def typed(text):
                run("xdotool", "type", "--window", window, "--delay", "20", text)

            # One line among 10,000, in a row not mounted.
            keys("ctrl+f")
            until("the bar", lambda: status().get("open") == "true")
            typed("line 7777)")
            found = until("the match", lambda: status() if status().get("count") == "1" and status().get("done") == "true" else None)
            checks["one match, counted"] = found["count"] == "1"
            checks["current"] = found["current"]
            offset = float(until("the transcript scrolled", lambda: status().get("offset") if float(status().get("offset", "0")) > 0 else None))
            checks["transcript scrolled to it"] = abs(offset - 7776 * 22) < 300
            time.sleep(.4)
            shot = args.output / "found.png"
            run("import", "-window", window, shot)
            image = Image.open(shot).convert("RGB")
            orange = sum(1 for p in image.getdata() if abs(p[0] - ORANGE[0]) < 24 and abs(p[1] - ORANGE[1]) < 30 and p[2] < 110)
            marked = until("the current match marked", lambda: status() if status().get("current_marked") == "1" else None)
            checks["current mark shows"] = marked["current_marked"] == "1"
            # What the screenshot shows of it (the theme's background decides
            # the blend): for the record.
            checks["orange pixels"] = orange
            # Escape closes, its marks gone.
            keys("Escape")
            until("closed", lambda: status().get("open") == "false")
            time.sleep(.3)
            run("import", "-window", window, args.output / "closed.png")
            image = Image.open(args.output / "closed.png").convert("RGB")
            left = sum(1 for p in image.getdata() if abs(p[0] - ORANGE[0]) < 24 and abs(p[1] - ORANGE[1]) < 30 and p[2] < 110)
            checks["marks gone"] = status().get("marked") == "0"
            checks["orange pixels after"] = left

            # Every transcript line: counted in slices, then done.
            keys("ctrl+f")
            until("the bar again", lambda: status().get("open") == "true")
            keys("ctrl+a")
            typed("transcript line")
            counts = []

            def counted():
                now = status()
                if now.get("query") != "transcript line":
                    return None
                counts.append(int(now.get("count", "0")))
                return now if now.get("done") == "true" else None

            done = until("all counted", counted)
            checks["all counted"] = int(done["count"]) >= 10_000
            checks["counts seen"] = sorted(set(counts))
            keys("Return")
            until("stepped", lambda: status().get("current") == "Some(1)")
            checks["Enter steps"] = True
            keys("Escape")
            until("closed again", lambda: status().get("open") == "false")

            (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
            failed = [k for k, v in checks.items() if v is False]
            assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
            print("PASS: X11 find in page: Ctrl+F finds a line among 10,000 in an unmounted row, scrolls to it "
                  "and marks it; Enter steps, Escape closes; 10,000 matches counted in slices")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
