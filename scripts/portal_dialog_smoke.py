#!/usr/bin/env python3
"""File dialogs through the desktop portal, on X11 (Xvfb/Openbox): a
private session bus starts xdg-desktop-portal and its GTK backend, whose
chooser shows on the display, as on a GNOME, Xfce or other GTK desktop.

Checks, with the platform_services example:

- Open: the chooser carries its title, is modal and transient for the
  app's window (the portal's `parent_window`), and the path typed into it
  reaches the app;
- Open, then Escape: no choice (`Ok(None)`), not an error. GTK's chooser
  answers 2, "ended another way", for Escape and its close button;
- Save: the chooser starts in the folder given and with the name given,
  and Enter there saves to it."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time

from platform_smoke import release_mounts, stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the platform_services example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    fixture = output / "fixture.txt"
    fixture.write_text("chosen through the portal\n")
    folder = output / "save-here"
    folder.mkdir(exist_ok=True)
    result = {}
    processes = []

    def launch(name, command, env, **options):
        log = (output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    with tempfile.TemporaryDirectory(prefix="zgx-portal-") as runtime:
        os.chmod(runtime, 0o700)
        try:
            env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, GSK_RENDERER="cairo",
                       GSETTINGS_BACKEND="memory", XDG_CONFIG_HOME=str(pathlib.Path(runtime, "config")),
                       XDG_CACHE_HOME=str(pathlib.Path(runtime, "cache")),
                       XDG_DATA_HOME=str(pathlib.Path(runtime, "data")))
            for name in ("WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP", "AT_SPI_BUS_ADDRESS"):
                env.pop(name, None)
            xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                          stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            # A session bus of the probe's own, which starts the portal (and
            # its GTK backend, on this display) when zgx asks for it.
            bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                         stdout=subprocess.PIPE, text=True)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("dbus-daemon startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            subprocess.run(["dbus-update-activation-environment", "DISPLAY", "XDG_RUNTIME_DIR", "GSK_RENDERER",
                            "GSETTINGS_BACKEND", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME"],
                           env=env, check=True)
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

            def window(title):
                found = run("xdotool", "search", "--onlyvisible", "--name", f"^{re.escape(title)}$")
                return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None

            def app(name, action):
                log = output / (name + ".log")
                child = launch(name + "-app", [str(args.binary.resolve()), "--smoke"],
                               dict(env, ZGX_DIALOG_ACTION=action, ZGX_DIALOG_DIRECTORY=str(folder)),
                               stdout=log.open("w"))
                return child, log

            def answer(log, action):
                return until(f"the {action} dialog's answer",
                             lambda: re.search(rf"DIALOG {action} (.*)", log.read_text()))[1]

            def key(*keys):
                run("xdotool", "key", "--clearmodifiers", *keys)

            # Open: a modal chooser over the app's window, its path reaching
            # the app. The first request starts the portal and GTK: slower.
            child, log = app("open", "open")
            chooser = until("the portal's chooser", lambda: window("Open fixture"), 40)
            owner = until("the app's window", lambda: window("zgx platform services"))
            transient = run("xprop", "-id", chooser, "WM_TRANSIENT_FOR").stdout
            result["transient for the app's window"] = "#" in transient and int(transient.split("#")[-1], 16) == int(owner)
            result["modal"] = "_NET_WM_STATE_MODAL" in run("xprop", "-id", chooser, "_NET_WM_STATE").stdout
            run("xdotool", "windowactivate", "--sync", chooser)
            time.sleep(.5)
            # The location entry, then the path; a Return may only complete
            # the entry, so another may be needed to open it.
            key("ctrl+l")
            time.sleep(.3)
            run("xdotool", "type", "--clearmodifiers", "--delay", "15", fixture)
            time.sleep(.3)
            for _ in range(3):
                key("Return")
                try:
                    until("the chosen path", lambda: "DIALOG open" in log.read_text(), 3)
                    break
                except AssertionError:
                    if not window("Open fixture"):
                        raise
            result["open"] = answer(log, "open")
            result["the path typed reaches the app"] = result["open"] == f'Ok(Some("{fixture}"))'
            stop(child)

            # Open, then Escape: no choice, and no error.
            child, log = app("escape", "open")
            chooser = until("the chooser again", lambda: window("Open fixture"))
            run("xdotool", "windowactivate", "--sync", chooser)
            time.sleep(.5)
            key("Escape")
            result["escape"] = answer(log, "open")
            result["Escape is no choice"] = result["escape"] == "Ok(None)"
            stop(child)

            # Save: in the folder and with the name given; Enter saves there.
            child, log = app("save", "save")
            chooser = until("the save chooser", lambda: window("Save fixture"))
            run("xdotool", "windowactivate", "--sync", chooser)
            time.sleep(.8)
            key("Return")
            result["save"] = answer(log, "save")
            result["saved in the folder, by the name, given"] = result["save"] == f'Ok(Some("{folder / "saved.txt"}"))'
            stop(child)

            (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            failed = [name for name, value in result.items() if value is False]
            assert not failed, f"failed: {failed}\n{json.dumps(result, indent=2)}"
            print("PASS: the portal's chooser opens over its window, answers, and Escape is no choice")
        finally:
            for process in reversed(processes):
                stop(process)
            release_mounts(runtime)


if __name__ == "__main__":
    main()
