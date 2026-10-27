#!/usr/bin/env python3
"""A window's zoom on X11 (Xvfb/Openbox), with real keys: zoom_probe's
window, Ctrl+= twice, Ctrl+-, Ctrl+0, then Control with a wheel notch up
and one down (in Xvfb's own pointer, inside the window).

Checks that each key steps through Chrome's levels (110%, 125%, back to
110% and 100%), that the window's logical viewport shrinks and grows by the
zoom while the window keeps its size, that text keeps its size in points
(it draws larger in pixels), and that Control with the wheel over the
zoomable panel at the top zooms the panel, not the window."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time

from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    with tempfile.TemporaryDirectory(prefix="zgx-zoom-") as runtime:
        try:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"],
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 8)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env = dict(os.environ, DISPLAY=":" + xvfb.stdout.readline().strip(), WINIT_X11_SCALE_FACTOR="1",
                       XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent", NO_AT_BRIDGE="1")
            env.pop("WAYLAND_DISPLAY", None)
            processes.append(subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            wait_for_wm(env)
            log = args.output / "app.log"
            processes.append(subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log.open("w"),
                                              stderr=subprocess.STDOUT))

            def run(*argv):
                return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

            def lines():
                return [dict(re.findall(r"(\w+)=(\S+)", line)) for line in re.findall(r"^ZOOM (.*)$", log.read_text(), re.M)]

            def until(what, test, seconds=15):
                deadline = time.monotonic() + seconds
                while True:
                    value = test()
                    if value:
                        return value
                    if time.monotonic() > deadline:
                        raise AssertionError(f"timed out waiting for {what}: {lines()} {checks}")
                    time.sleep(.05)

            window = until("the window", lambda: run("xdotool", "search", "--onlyvisible", "--name", "^zgx zoom$").split())[0]
            until("the probe", lines)
            run("xdotool", "windowactivate", "--sync", window)
            time.sleep(.3)
            geometry = run("xdotool", "getwindowgeometry", window)
            for keys, zoom in [("ctrl+equal", "1.10"), ("ctrl+equal", "1.25"), ("ctrl+minus", "1.10"), ("ctrl+0", "1.00")]:
                run("xdotool", "key", "--window", window, "--clearmodifiers", keys)
                until(f"{zoom} after {keys}", lambda: lines()[-1].get("zoom") == zoom)
            # Control with the wheel: a level a notch. Xvfb's own pointer and
            # keyboard (XTEST, no `--window`): winit reads the pointer through
            # XInput2, which never sees a sent (`--window`) button event.
            run("xdotool", "mousemove", "--window", window, "200", "150")
            for button, zoom in [("4", "1.10"), ("5", "1.00")]:
                run("xdotool", "keydown", "ctrl", "click", button, "keyup", "ctrl")
                until(f"{zoom} after ctrl+wheel {button}", lambda: lines()[-1].get("zoom") == zoom)
            checks["control-wheel steps a level a notch"] = True
            # Over the zoomable panel at the top: the panel zooms, a level a
            # notch, and the window doesn't (a gesture of its own: one stays
            # with what took it for 300 ms).
            time.sleep(.4)
            run("xdotool", "mousemove", "--window", window, "20", "16")
            for button, panel in [("4", "1.10"), ("5", "1.00")]:
                run("xdotool", "keydown", "ctrl", "click", button, "keyup", "ctrl")
                until(f"panel {panel} after ctrl+wheel {button}", lambda: lines()[-1].get("panel") == panel)
                time.sleep(.4)
            checks["control-wheel over a zoomable panel zooms the panel, not the window"] = all(
                line["zoom"] == "1.00" for line in lines() if line.get("panel") == "1.10")
            seen = lines()
            sizes = {line["zoom"]: tuple(map(float, line["viewport"].split("x"))) for line in seen}
            base = sizes["1.00"]
            for zoom in ("1.10", "1.25"):
                want = (base[0] / float(zoom), base[1] / float(zoom))
                got = sizes[zoom]
                checks[f"viewport at {zoom}"] = got
                checks[f"viewport shrinks by {zoom}"] = all(abs(g - w) <= 1.5 for g, w in zip(got, want))
            checks["window kept its size"] = run("xdotool", "getwindowgeometry", window) == geometry
            checks["text keeps its points"] = len({line["text_points"] for line in seen}) == 1
            (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
            failed = [k for k, v in checks.items() if v is False]
            assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
            print("PASS: X11 zoom: Ctrl+= and Ctrl+- step through Chrome's levels and Ctrl+0 resets, Control with "
                  "the wheel steps a level a notch; the logical viewport follows the zoom, the window keeps its size")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
