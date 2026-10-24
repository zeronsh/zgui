#!/usr/bin/env python3
"""An app on a system with no fonts (a minimal install, a container): the
gallery on Xvfb, its fontconfig listing only an empty directory, draws its
window (every control but their text), says once why its text does not
show, and never panics. Without a font, shaping text panicked every frame,
and the window drew nothing at all."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import tempfile
import time

from platform_smoke import stop

WARNING = "zgui: no font to draw text with"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the gallery example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    from PIL import Image
    processes, checks = [], {}
    with tempfile.TemporaryDirectory(prefix="zgui-no-fonts-") as runtime:
        try:
            empty = pathlib.Path(runtime, "fonts")
            empty.mkdir()
            config = pathlib.Path(runtime, "fonts.conf")
            config.write_text(f'<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n'
                              f'<fontconfig><dir>{empty}</dir><cachedir>{runtime}/cache</cachedir></fontconfig>\n')
            env = dict(os.environ, FONTCONFIG_FILE=str(config), XDG_RUNTIME_DIR=runtime, WINIT_X11_SCALE_FACTOR="1",
                       XDG_CACHE_HOME=f"{runtime}/cache", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env=env,
                                    stdout=subprocess.PIPE, stderr=(args.output / "xvfb.log").open("w"), text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            log = args.output / "gallery.log"
            app = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log.open("w"),
                                   stderr=subprocess.STDOUT)
            processes.append(app)

            def window():
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env,
                                       capture_output=True, text=True).stdout.split()
                return found[0] if found else None

            def drawn(id):
                path = args.output / "window.png"
                if subprocess.run(["import", "-window", id, str(path)], env=env, capture_output=True).returncode:
                    return 0.
                pixels = list(Image.open(path).convert("RGB").getdata())
                return sum(p != (0, 0, 0) for p in pixels) / len(pixels)

            deadline = time.monotonic() + 20
            id, share = None, 0.
            while time.monotonic() < deadline and app.poll() is None:
                id = id or window()
                share = drawn(id) if id else 0.
                if share > .9:
                    break
                time.sleep(.2)
            time.sleep(1)
            output = log.read_text(errors="replace")
            checks["window drawn (share not black)"] = round(share, 3)
            checks["the window is drawn"] = share > .9
            checks["says why its text does not show, once"] = output.count(WARNING) == 1
            checks["no panic"] = "panicked" not in output
            checks["still running"] = app.poll() is None
        finally:
            for process in reversed(processes):
                stop(process)
    (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
    failed = [key for key, value in checks.items() if value is False]
    assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}\n{output[-3000:]}"
    print("PASS: with no fonts, the gallery draws its window, says once why its text does not show, "
          "and never panics")


if __name__ == "__main__":
    main()
