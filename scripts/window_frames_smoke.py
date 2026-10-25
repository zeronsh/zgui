#!/usr/bin/env python3
"""Where windows open on Linux (`window_place/linux.rs`), on X11 (Xvfb/Openbox)
and Wayland (Weston), through `window_probe`:

- a second window with no position opens a step on from the first, down and
  right by its title bar, rather than where the window manager would put it;
- a window with `frame_autosave`, moved and resized, has its frame saved once
  the moves settle, even if the app is then killed, and reopens there at that
  size;
- maximized and closed at once (Alt+F4, before the save's delay), it reopens
  maximized, and restored, at the frame it had before;
- at a saved frame no monitor shows, it opens on the screen at the saved size;
- on Wayland (Weston), where the compositor places windows, it reopens at
  its saved size, or maximized, keeping the frame from before.

Frames are kept under a private XDG_STATE_HOME."""
import argparse
import json
import os
import pathlib
import re
import select
import signal
import subprocess
import time

from platform_smoke import stop, wait_for_wm

SCREEN = (1600, 1000)
NAME = "zgx window_probe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="the window_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    state = args.output / "state"
    frames = state / "zgx" / "window_probe" / "frames"
    if frames.exists():
        frames.unlink()
    processes, checks = [], {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log),
                                 stderr=options.pop("stderr", log), **options)
        processes.append(child)
        return child

    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
                   XDG_STATE_HOME=str(state.resolve()), DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = launch("xvfb", ["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "%dx%dx24" % SCREEN, "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 10)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True).stdout

        def until(what, test, app=None, seconds=15):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline or (app and app.poll() is not None):
                    raise AssertionError(f"timed out waiting for {what}: {checks}")
                time.sleep(.05)

        def named(name):
            return run("xdotool", "search", "--onlyvisible", "--name", f"^{name}$").split()

        def geometry(window):
            """The content's top-left on the root and its size."""
            info = run("xwininfo", "-id", window)
            value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
            return (value("Absolute upper-left X"), value("Absolute upper-left Y"),
                    value("Width"), value("Height"))

        def title_height(window):
            extents = run("xprop", "-id", window, "_NET_FRAME_EXTENTS")
            numbers = [int(n) for n in re.findall(r"\d+", extents.split("=")[-1])]
            return numbers[2] if len(numbers) == 4 else 0

        def maximized(window):
            state = run("xprop", "-id", window, "_NET_WM_STATE")
            return "_NET_WM_STATE_MAXIMIZED_VERT" in state and "_NET_WM_STATE_MAXIMIZED_HORZ" in state

        def settled(window):
            """Its geometry, once it stops changing."""
            last = None
            for _ in range(40):
                now = geometry(window)
                if now == last:
                    return now
                last = now
                time.sleep(.1)
            return last

        def saved():
            text = frames.read_text() if frames.exists() else ""
            return next((line.split("\t", 1)[1] for line in text.splitlines()
                         if line.startswith(NAME + "\t")), None)

        def open_saved():
            app = launch(f"saved-{len(processes)}", [str(args.probe), "saved"], env)
            window = until("the Saved window", lambda: named("Saved"), app)[0]
            time.sleep(.3)
            return app, window

        # A second window, opened a moment after the first.
        app = launch("cascade", [str(args.probe)], env)
        first = until("window First", lambda: named("First"), app)[0]
        second = until("window Second", lambda: named("Second"), app)[0]
        a, b = settled(first), settled(second)
        step = title_height(first)
        checks["cascade"] = {"first": a, "second": b, "title": step}
        checks["cascades a title bar on"] = step > 0 and (b[0] - a[0], b[1] - a[1]) == (step, step)
        stop(app)

        # Moved and resized, then killed once the moves settled: saved.
        app, window = open_saved()
        x, y, _, _ = settled(window)
        step = title_height(window)
        # xdotool moves the frame's top-left, as the window manager has it.
        run("xdotool", "windowmove", window, x + 140, y - step + 90)
        run("xdotool", "windowsize", window, 520, 260)
        moved = settled(window)
        time.sleep(1)
        checks["saved"] = saved()
        os.kill(app.pid, signal.SIGKILL)
        app.wait()
        app, window = open_saved()
        reopened = settled(window)
        checks["moved"], checks["reopened"] = moved, reopened
        checks["reopens where it was moved, at its size"] = reopened == moved and moved[2:] == (520, 260)

        # Maximized, then closed before the save's delay: saved as it closes.
        x, y, w, _ = reopened
        run("xdotool", "mousemove", x + w // 2, y - step // 2, "click", "--repeat", 2, "--delay", 80, 1)
        until("the window maximized", lambda: maximized(window), app)
        run("xdotool", "windowactivate", "--sync", window, "key", "alt+F4")
        # Under load Openbox can miss a key sent as the window settles
        # maximized: pressed again after a while, as a user would. An app
        # that ignored the close would still not exit.
        deadline = time.monotonic() + 5
        while app.poll() is None and time.monotonic() < deadline:
            time.sleep(.05)
        if app.poll() is None:
            checks["closed on a second Alt+F4"] = True
            run("xdotool", "windowactivate", "--sync", window, "key", "alt+F4")
        until("the app to exit", lambda: app.poll() is not None, seconds=15)
        app, window = open_saved()
        checks["reopens maximized"] = until("the window maximized", lambda: maximized(window), app, seconds=3)
        x, y, w, _ = settled(window)
        run("xdotool", "mousemove", x + w // 2, y - title_height(window) // 2, "click", "--repeat", 2,
            "--delay", 80, 1)
        until("the window restored", lambda: not maximized(window), app)
        restored = settled(window)
        checks["restored"] = restored
        checks["restores to the frame before"] = restored == moved
        stop(app)

        # A frame no monitor shows: on the screen, at the saved size.
        frames.write_text(f"{NAME}\t60000 60000 500 240 0\n")
        app, window = open_saved()
        far = settled(window)
        checks["far"] = far
        checks["on the screen, at the saved size"] = (0 <= far[0] < SCREEN[0] and 0 <= far[1] < SCREEN[1]
                                                     and far[2:] == (500, 240))
        stop(app)

        # Wayland (Weston): the compositor places windows, so a saved frame
        # keeps its size and whether it was maximized alone.
        runtime = args.output / "wayland-runtime"
        runtime.mkdir(mode=0o700, exist_ok=True)
        wayland = dict(env, XDG_RUNTIME_DIR=str(runtime.resolve()), WAYLAND_DISPLAY="zgx-frames")
        wayland.pop("DISPLAY")
        weston = launch("weston", ["weston", "--backend=headless", "--renderer=pixman", "--width=1280",
                                   "--height=960", "--idle-time=0", "--no-config", "--socket=zgx-frames"], wayland)
        until("Weston's socket", lambda: (runtime / "zgx-frames").exists(), weston, seconds=10)

        def wayland_size(frame):
            frames.write_text(f"{NAME}\t{frame}\n")
            log = args.output / f"wayland-{len(processes)}.log"
            app = launch(log.stem, [str(args.probe), "saved"], wayland, stdout=log.open("w"))
            line = until("its size", lambda: re.search(r"^SIZE (\d+) (\d+)$", log.read_text(), re.M), app)
            # Saved again once shown: as it was.
            time.sleep(.8)
            stop(app)
            return int(line.group(1)), int(line.group(2))

        checks["wayland size"] = wayland_size("- - 500 240 0")
        checks["wayland reopens at its size"] = checks["wayland size"] == (500, 240)
        checks["wayland maximized size"] = wayland_size("- - 500 240 1")
        checks["wayland reopens maximized"] = checks["wayland maximized size"][0] == 1280
        checks["wayland keeps the frame before"] = saved() == "- - 500 240 1"
        stop(weston)

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: X11 windows cascade from the front window, and saved frames reopen where they were left "
              "(moved, killed, maximized, closed at once, off every monitor), and on Wayland at their size or maximized")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
