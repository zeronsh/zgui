#!/usr/bin/env python3
"""A launcher's token reaches the first window, so the desktop focuses it
and ends the launch's busy cursor. On X11 (Xvfb) with DESKTOP_STARTUP_ID
set, the window announces the launch complete (`_NET_STARTUP_INFO` on the
root window, as `xev` sees it); as a native Wayland client of a headless
sway with XDG_ACTIVATION_TOKEN set, it activates its surface with the token
(`xdg_activation_v1.activate`, in the client's WAYLAND_DEBUG log).

Then a running app with an application ID is relaunched: a D-Bus
`Activate` hands over the new launch's token (GApplication's
`platform_data`), and the running window takes it, on X11 ending that
launch's startup notification, on Wayland activating its surface."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time

from platform_smoke import private_runtime, stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the gallery example")
    parser.add_argument("relaunched", type=pathlib.Path, help="the native_platform example (an application ID)")
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

    def private_bus(env):
        bus = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        if not select.select([bus.stdout], [], [], 10)[0]:
            raise RuntimeError("dbus-daemon startup timed out")
        return bus, bus.stdout.readline().strip()

    def relaunch(env, log, platform_data, done):
        """native_platform running; `Activate` with `platform_data`; until `done`."""
        with log.open("w") as out:
            app = subprocess.Popen([str(args.relaunched.resolve())], env=env, stdout=out, stderr=out)
        try:
            deadline = time.monotonic() + 15
            while subprocess.run(["gdbus", "call", "--session", "--dest", "org.zgx.NativePlatform",
                                  "--object-path", "/org/zgx/NativePlatform", "--method",
                                  "org.freedesktop.DBus.Peer.Ping"], env=env, capture_output=True).returncode:
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("native_platform's D-Bus service did not start; see " + log.name)
                time.sleep(.1)
            time.sleep(1)
            subprocess.check_call(["gdbus", "call", "--session", "--dest", "org.zgx.NativePlatform",
                                   "--object-path", "/org/zgx/NativePlatform", "--method",
                                   "org.freedesktop.Application.Activate", platform_data],
                                  env=env, stdout=subprocess.DEVNULL)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline and not done():
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
        xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
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
        announced = run_until([str(args.binary.resolve())], dict(env, DESKTOP_STARTUP_ID="zgx-probe_TIME0"),
                              args.output / "x11.log", lambda: "_NET_STARTUP_INFO" in xev_log.read_text())
        result["x11_startup_notification"] = announced
        bus, address = private_bus(env)
        processes.append(bus)
        before = xev_log.read_text().count("_NET_STARTUP_INFO_BEGIN")
        result["x11_relaunch_completed"] = relaunch(
            dict(env, DBUS_SESSION_BUS_ADDRESS=address), args.output / "x11-relaunch.log",
            "{'desktop-startup-id': <'zgx-relaunch_TIME0'>}",
            lambda: xev_log.read_text().count("_NET_STARTUP_INFO_BEGIN") > before)
    finally:
        for process in reversed(processes):
            stop(process)

    # Wayland: the token activates the window's surface.
    with private_runtime("zgx-launch-token-") as private:
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
                dict(env, WAYLAND_DISPLAY=display, WAYLAND_DEBUG="1", XDG_ACTIVATION_TOKEN="zgx-probe-token"),
                log, lambda: 'activate("zgx-probe-token"' in log.read_text(errors="replace"))
            result["wayland_activation"] = activated
            bus, address = private_bus(env)
            try:
                log = args.output / "wayland-relaunch.log"
                result["wayland_relaunch_activation"] = relaunch(
                    dict(env, WAYLAND_DISPLAY=display, WAYLAND_DEBUG="1", DBUS_SESSION_BUS_ADDRESS=address),
                    log, "{'activation-token': <'zgx-relaunch-token'>}",
                    lambda: 'activate("zgx-relaunch-token"' in log.read_text(errors="replace"))
            finally:
                stop(bus)
        finally:
            stop(sway)

    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert all(result.values()), result
    print("PASS: a launch's token, at start or relaunched through D-Bus, ends X11's startup "
          "notification and activates the Wayland window")


if __name__ == "__main__":
    main()
