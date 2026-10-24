#!/usr/bin/env python3
"""A touchpad flick coasts on after the fingers lift, as a native Wayland
client of a headless sway: a virtual pointer (`wayland_pointer.py`) scrolls
the scroll example's rows with two fingers at 120 Hz and lifts them (axis
stop). Its rows must still be moving between 80 ms and 500 ms after the
lift, and still between 2 s and 2.5 s after it (the fling over)."""
import argparse
import json
import os
import pathlib
import subprocess
import tempfile
import time

from platform_smoke import stop
from wayland_pointer import VirtualPointer

OUTPUT = (1280, 800)
# The scroll example's viewport colour, rgb(0x203040), around its rows.
VIEWPORT = (0x20, 0x30, 0x40)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the scroll example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    from PIL import Image, ImageChops
    processes = []

    def launch(name, command, env):
        log = (args.output / (name + ".log")).open("w")
        processes.append(subprocess.Popen(command, env=env, stdout=log, stderr=log))
        return processes[-1]

    with tempfile.TemporaryDirectory(prefix="zgx-touchpad-") as private:
        os.chmod(private, 0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                   WLR_LIBINPUT_NO_DEVICES="1")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        config = pathlib.Path(private, "sway.conf")
        config.write_text("output * mode %dx%d\ndefault_border none\n"
                          "for_window [app_id=\".*\"] floating enable, move position 0 0\n" % OUTPUT)
        try:
            launch("sway", ["sway", "--unsupported-gpu", "-c", str(config)], env)
            deadline = time.monotonic() + 10
            while not [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]:
                if time.monotonic() > deadline:
                    raise RuntimeError("sway did not start; see sway.log")
                time.sleep(.05)
            display = next(p.name for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock")
            env["WAYLAND_DISPLAY"] = display
            app = launch("application", [str(args.binary.resolve())], env)

            def shot(name):
                path = args.output / f"{name}.png"
                subprocess.check_call(["grim", str(path)], env=env)
                return Image.open(path).convert("RGB")

            # The viewport, once the window shows.
            deadline = time.monotonic() + 20
            while True:
                image = shot("start")
                pixels = image.load()
                hits = [(x, y) for y in range(0, image.height, 4) for x in range(0, image.width, 4)
                        if pixels[x, y] == VIEWPORT]
                if hits:
                    break
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the scroll example did not show")
                time.sleep(.2)
            xs, ys = [x for x, _ in hits], [y for _, y in hits]
            viewport = (min(xs), min(ys), max(xs), max(ys))
            centre = ((viewport[0] + viewport[2]) // 2, (viewport[1] + viewport[3]) // 2)

            before = image.crop(viewport)
            pointer = VirtualPointer(private, display)
            pointer.move_to(*centre, *OUTPUT)
            time.sleep(.3)
            # A flick: 6 points a frame at 120 Hz, 720 points a second.
            pointer.swipe(dy=6, frames=8)
            lifted = time.monotonic()

            def at(seconds, name):
                time.sleep(max(0., lifted + seconds - time.monotonic()))
                return shot(name).crop(viewport)

            early, late = at(.08, "lift-80ms"), at(.5, "lift-500ms")
            settled, after = at(2., "lift-2s"), at(2.5, "lift-2500ms")
            pointer.close()
            moved = ImageChops.difference(early, late).getbbox() is not None
            still = ImageChops.difference(settled, after).getbbox() is None
            started = ImageChops.difference(before, early).getbbox() is not None
            result = {"viewport": viewport, "scrolled_by_the_fingers": started,
                      "coasting_after_lift": moved, "stopped_by_2s": still}
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            assert moved, f"the rows stopped as the fingers lifted: {result}"
            assert still, f"the fling never stopped: {result}"
            print("PASS: a touchpad flick coasts on after the fingers lift, then stops")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
