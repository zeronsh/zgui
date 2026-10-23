#!/usr/bin/env python3
"""A file dialog where no portal answers, on X11 (Xvfb/Openbox, a session
bus with nothing on it): zgui shows zenity's chooser, and the path typed
into it reaches the app. Then, with the chooser open again, closing its
window ends the chooser and leaves the app running (its other window)."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the platform_services example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    result = {}
    processes = []
    fixture = args.output.resolve() / "fixture.txt"
    fixture.write_text("chosen through zenity\n")

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", GSK_RENDERER="cairo")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        # A session bus with no portal on it.
        bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                     stdout=subprocess.PIPE, text=True)
        if not select.select([bus.stdout], [], [], 10)[0]:
            raise RuntimeError("dbus-daemon startup timed out")
        env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True)

        def until(what, test, seconds=20):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline:
                    raise AssertionError(f"timed out waiting for {what}: {result}")
                time.sleep(.1)

        def chooser():
            found = run("xdotool", "search", "--onlyvisible", "--name", "^Open fixture$")
            return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None

        def zenities():
            return run("pgrep", "-x", "zenity").stdout.split()

        # Open: the typed path reaches the app.
        log = args.output / "application.log"
        app = launch("application", [str(args.binary.resolve()), "--smoke"], env, stdout=log.open("w"))
        window = until("zenity's chooser", chooser)
        run("xdotool", "windowactivate", "--sync", window)
        time.sleep(1)
        # The location entry, then the path: as GTK's chooser takes a typed
        # path. A Return may only complete the entry: another opens it.
        run("xdotool", "key", "--clearmodifiers", "ctrl+l")
        time.sleep(.5)
        run("xdotool", "type", "--clearmodifiers", "--delay", "20", str(fixture))
        time.sleep(.5)
        for _ in range(3):
            run("xdotool", "key", "--clearmodifiers", "Return")
            try:
                until("the chosen path in the app", lambda: str(fixture) in log.read_text(), 4)
                break
            except AssertionError:
                if not chooser():
                    raise
        until("the chosen path in the app", lambda: str(fixture) in log.read_text(), 1)
        result["chosen"] = "DIALOG open Ok(Some(" in log.read_text()
        stop(app)
        until("zenity gone", lambda: not zenities(), 5)

        # Owner closed while the chooser shows: the chooser goes, the app stays.
        log = args.output / "close.log"
        app = launch("close", [str(args.binary.resolve()), "--smoke", "--close-picker"], env, stdout=log.open("w"))
        until("zenity's chooser", chooser)
        owner = until("the app's window", lambda: (run("xdotool", "search", "--onlyvisible", "--name",
                                                       "^zgui platform services$").stdout.split() or [None])[0])
        run("xdotool", "windowclose", owner)
        until("the chooser ending with its window", lambda: not zenities(), 10)
        result["chooser_ends_with_owner"] = True
        result["app_survives"] = app.poll() is None
        assert "DIALOG open " not in log.read_text(), "the closed owner received a result"
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        assert all(result.values()), result
        print("PASS: with no portal, zenity's chooser answers, and ends with its window")
    finally:
        for process in reversed(processes):
            stop(process)
        subprocess.run(["pkill", "-x", "zenity"], capture_output=True)


if __name__ == "__main__":
    main()
