#!/usr/bin/env python3
"""GL where no Vulkan driver is (Linux, `GpuContext::create`): the gallery on
Xvfb drawn twice, through Vulkan (lavapipe), then with no Vulkan driver to
find (`VK_ICD_FILENAMES` naming none), through GL (Mesa's, over EGL).

Checks that:

- with no Vulkan driver the app starts and draws, the same pixels as through
  Vulkan, with Mesa's GL driver loaded and no Vulkan driver;
- with one, it loads no GL driver: the fallback costs a Vulkan system
  nothing."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time

from platform_smoke import stop

SCREEN = (1280, 800)
VULKAN = ("libvulkan_",)
GL = ("libgallium", "swrast_dri", "libEGL_mesa")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gallery", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"],
                                env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()

        def draw(name, extra):
            log = args.output / f"{name}.log"
            app = subprocess.Popen([str(args.gallery)], env=dict(env, **extra), stdout=log.open("w"),
                                   stderr=subprocess.STDOUT)
            processes.append(app)
            # Drawn and settled: the window shows and its pixels stop changing.
            shot, last, deadline = args.output / f"{name}.png", None, time.monotonic() + 30
            while time.monotonic() < deadline and app.poll() is None:
                time.sleep(1)
                subprocess.run(["import", "-window", "root", str(shot)], env=env, capture_output=True)
                data = shot.read_bytes() if shot.exists() else None
                if data and data == last:
                    break
                last = data
            alive = app.poll() is None
            maps = pathlib.Path(f"/proc/{app.pid}/maps").read_text() if alive else ""
            stop(app)
            return {
                "started": alive,
                "vulkan driver": any(name in maps for name in VULKAN),
                "gl driver": any(name in maps for name in GL),
                "log": log.read_text()[-600:] if not alive else "",
            }, shot

        vulkan, vulkan_shot = draw("vulkan", {})
        gl, gl_shot = draw("gl", {"VK_ICD_FILENAMES": "/nonexistent/none.json",
                                  "VK_DRIVER_FILES": "/nonexistent/none.json"})
        checks["vulkan"], checks["gl"] = vulkan, gl
        from PIL import Image, ImageChops
        a, b = (Image.open(shot).convert("RGB") for shot in (vulkan_shot, gl_shot))
        differing = sum(1 for pixel in ImageChops.difference(a, b).getdata() if max(pixel) > 24)
        colours = len(b.getcolors(1 << 22) or [])
        checks["pixels differing"], checks["colours drawn through GL"] = differing, colours

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        assert vulkan["started"] and vulkan["vulkan driver"] and not vulkan["gl driver"], checks
        assert gl["started"] and gl["gl driver"] and not gl["vulkan driver"], checks
        # A drawn gallery, not a blank window, and the same through both.
        assert colours > 100 and differing <= a.width * a.height // 1000, checks
        print(f"PASS: with no Vulkan driver the gallery draws through GL ({differing} pixels differ from "
              "Vulkan's); with one, no GL driver loads")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
