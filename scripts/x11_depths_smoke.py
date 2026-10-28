#!/usr/bin/env python3
"""Windows on X screens of 16 and 30 bits (some VNC servers and older setups;
deep colour): the gallery on Xvfb at each depth takes a visual of 24 or 32
bits (`x11_visual`) and draws exactly what it draws on a 24-bit screen.
Mesa's Vulkan offered no format to present to a 16- or 30-bit window, so
the app panicked making its renderer and showed no window at all."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the gallery example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    from PIL import Image, ImageChops
    checks = {}

    def draw(depth):
        """The gallery's window on a screen of `depth` bits: its depth, its
        image, and its log."""
        processes = []
        try:
            env = dict(os.environ, XDG_RUNTIME_DIR="/tmp", WINIT_X11_SCALE_FACTOR="1",
                       DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", f"1280x900x{depth}", "-ac"], env=env,
                                    stdout=subprocess.PIPE, stderr=(args.output / f"xvfb-{depth}.log").open("w"),
                                    text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            log = args.output / f"gallery-{depth}.log"
            app = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log.open("w"),
                                   stderr=subprocess.STDOUT)
            processes.append(app)
            run = lambda *argv: subprocess.run(argv, env=env, capture_output=True, text=True).stdout
            deadline, window = time.monotonic() + 20, None
            while time.monotonic() < deadline and app.poll() is None and not window:
                window = (run("xdotool", "search", "--onlyvisible", "--pid", str(app.pid)).split() or [None])[0]
                time.sleep(.1)
            if not window:
                return None, None, log.read_text(errors="replace")
            # Settled: the same image twice running.
            path, last = args.output / f"gallery-{depth}.png", None
            for _ in range(40):
                time.sleep(.25)
                run("import", "-window", window, str(path))
                image = Image.open(path).convert("RGB")
                if last is not None and ImageChops.difference(image, last).getbbox() is None:
                    break
                last = image
            depth_of = int(re.search(r"Depth:\s+(\d+)", run("xwininfo", "-id", window)).group(1))
            return depth_of, image, log.read_text(errors="replace")
        finally:
            for process in reversed(processes):
                stop(process)

    _, usual, _ = draw(24)
    assert usual is not None, "no window on a 24-bit screen"
    for depth in (16, 30):
        window_depth, image, log = draw(depth)
        checks[f"{depth}-bit screen: window depth"] = window_depth
        checks[f"{depth}-bit screen: a window shows, of 24 or 32 bits"] = window_depth in (24, 32)
        checks[f"{depth}-bit screen: no panic"] = "panicked" not in log
        checks[f"{depth}-bit screen: drawn as on a 24-bit one"] = (
            image is not None and image.size == usual.size and ImageChops.difference(image, usual).getbbox() is None)
    (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
    failed = [key for key, value in checks.items() if value is False]
    assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
    print("PASS: on X screens of 16 and 30 bits, windows take a 24- or 32-bit visual and draw as on a 24-bit one")


if __name__ == "__main__":
    main()
