#!/usr/bin/env python3
"""A launcher's token reaches the first window, so the desktop focuses it
and ends the launch's busy cursor. On X11 (Xvfb) with DESKTOP_STARTUP_ID
set, the window announces the launch complete (`_NET_STARTUP_INFO` on the
root window, as `xev` sees it); as a native Wayland client of a headless
sway with XDG_ACTIVATION_TOKEN set, it activates its surface with the token
(`xdg_activation_v1.activate`, in the client's WAYLAND_DEBUG log)."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import tempfile
import time

from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the gallery example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    result = {}

    def run_until(command, env, log, done, seconds=15):
        with log.open("w") as out:
            app = subprocess.Popen(command, env=env, stdout=subprocess.DEVNULL, stderr=out)
        try:
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline and app.poll() is None:
                if done():
                    return True
                time.sleep(.1)
            return done()
        finally:
            stop(app)

    # X11: the startup notification's end, sent to the root window.
    processes = []
    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        processes.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        xev_log = args.output / "xev.log"
        with xev_log.open("w") as out:
            processes.append(subprocess.Popen(["xev", "-root", "-event", "property"], env=env,
                                              stdout=out, stderr=subprocess.STDOUT))
        time.sleep(.5)
        announced = run_until([str(args.binary.resolve())], dict(env, DESKTOP_STARTUP_ID="zgui-probe_TIME0"),
                              args.output / "x11.log", lambda: "_NET_STARTUP_INFO" in xev_log.read_text())
        result["x11_startup_notification"] = announced
    finally:
        for process in reversed(processes):
            stop(process)

    # Wayland: the token activates the window's surface.
    with tempfile.TemporaryDirectory(prefix="zgui-launch-token-") as private:
        os.chmod(private, 0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                   WLR_LIBINPUT_NO_DEVICES="1")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        config = pathlib.Path(private, "sway.conf")
        config.write_text("output * mode 1280x800\n")
        with (args.output / "sway.log").open("w") as out:
            sway = subprocess.Popen(["sway", "--unsupported-gpu", "-c", str(config)], env=env, stdout=out, stderr=out)
        try:
            deadline = time.monotonic() + 10
            while not [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]:
                if time.monotonic() > deadline:
                    raise RuntimeError("sway did not start; see sway.log")
                time.sleep(.05)
            display = next(p.name for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock")
            log = args.output / "wayland.log"
            activated = run_until(
                [str(args.binary.resolve())],
                dict(env, WAYLAND_DISPLAY=display, WAYLAND_DEBUG="1", XDG_ACTIVATION_TOKEN="zgui-probe-token"),
                log, lambda: 'activate("zgui-probe-token"' in log.read_text(errors="replace"))
            result["wayland_activation"] = activated
        finally:
            stop(sway)

    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert all(result.values()), result
    print("PASS: the launch token ends X11's startup notification and activates the Wayland window")


if __name__ == "__main__":
    main()
