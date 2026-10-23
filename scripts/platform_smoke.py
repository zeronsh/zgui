#!/usr/bin/env python3
"""Validate zgui on isolated X11 HiDPI and headless Weston Wayland desktops.

Uses already-built gallery/windows examples. Each private compositor is stopped
on exit. This checks native behavior, not hardware GPU performance.
"""
import argparse
import json
import os
import pathlib
import select
import shutil
import subprocess
import sys
import tempfile
import time


def stop(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def release_mounts(runtime):
    """Unmount the FUSE filesystems session services mounted in the private
    `runtime` directory.

    On a stock session bus zgui's settings read starts xdg-desktop-portal,
    which mounts xdg-document-portal's at `runtime`/doc, and GTK (IBus's
    panel) starts gvfs, which mounts its own at `runtime`/gvfs. A killed bus
    leaves them disconnected, and the directory can't be removed past them.
    Call once the bus is stopped.
    """
    prefix = os.path.realpath(runtime) + os.sep
    try:
        with open('/proc/mounts') as mounts:
            points = [line.split()[1] for line in mounts]
    except OSError:
        return
    tool = shutil.which('fusermount3') or shutil.which('fusermount')
    for point in points:
        if tool and point.startswith(prefix):
            subprocess.run([tool, '-u', '-z', point],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def wait_for_wm(env, seconds=20):
    """Wait until the window manager on `env`'s display manages windows:
    started is not ready, and a loaded machine takes longer than any sleep."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        def prop(*args):
            return subprocess.run(["xprop", *args], env=env, capture_output=True, text=True).stdout
        check = prop("-root", "_NET_SUPPORTING_WM_CHECK").rsplit("# ", 1)[-1].strip()
        if check.startswith("0x") and "_NET_WM_NAME" in prop("-id", check, "_NET_WM_NAME") \
                and "_NET_ACTIVE_WINDOW" in prop("-root", "_NET_SUPPORTED"):
            return
        time.sleep(.05)
    raise RuntimeError("the window manager did not start")


def dump_display(env, output, pid=None):
    """What the display shows, for a probe that cannot find its window: the
    window tree, the root's properties, the pid's windows and a screenshot."""
    def run(*command):
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
        return f"$ {' '.join(command)}\n{result.stdout}{result.stderr}\n"
    report = run("xwininfo", "-root", "-tree") + run("xprop", "-root")
    if pid:
        report += run("xdotool", "search", "--pid", str(pid))
        # Where a window that never showed is stuck (zgui shows a window once
        # its renderer exists). Runners restrict ptrace to descendants.
        report += run("sudo", "-n", "gdb", "-q", "-batch", "-p", str(pid),
                      "-ex", "thread apply all bt 25")
    (output / "display.log").write_text(f"DISPLAY={env.get('DISPLAY')}\n" + report)
    subprocess.run(["import", "-window", "root", str(output / "display.png")], env=env,
                   capture_output=True, timeout=10)


def wait_for_ibus_xim(env, seconds=20):
    """Wait until IBus's XIM frontend serves the display: a client started
    before then runs without the input method."""
    deadline = time.monotonic() + seconds
    while "@server=ibus" not in subprocess.run(["xprop", "-root", "XIM_SERVERS"], env=env,
                                               capture_output=True, text=True).stdout:
        if time.monotonic() > deadline:
            raise RuntimeError("IBus's XIM server did not register")
        time.sleep(.05)


def use_ibus_engine(env, engine, daemon, seconds=20):
    """Switch IBus to `engine` and wait until it is the engine and its
    process (a child of `daemon`, this probe's ibus-daemon) runs: keys sent
    before then reach no engine.

    IBus's panel picks its first engine as it starts, asynchronously, and
    whichever switch comes second cancels the other ("Set global engine
    failed: Operation was cancelled"). So wait (briefly) for the panel's
    choice first, and switch again while `engine` isn't the one IBus reports.
    (`ibus engine` fails, silently, for the engine already set, so the
    command's status says nothing.)"""
    def current():
        return subprocess.run(["ibus", "engine"], env=env, capture_output=True, text=True).stdout.strip()

    deadline = time.monotonic() + seconds
    settled = time.monotonic() + 5
    while not current() and time.monotonic() < settled:
        time.sleep(.05)
    while True:
        now = current()
        if now != engine:
            subprocess.run(["ibus", "engine", engine], env=env, capture_output=True)
            now = current()
        running = subprocess.run(["pgrep", "-P", str(daemon.pid), "-f", "ibus-engine-" + engine],
                                 capture_output=True).returncode == 0
        if now == engine and running:
            time.sleep(.3)
            return
        if time.monotonic() > deadline:
            raise RuntimeError(f"IBus engine {engine} did not start ({now!r})")
        time.sleep(.05)


def x11(args, output):
    for tool in ("Xvfb", "openbox", "xdotool"):
        if not shutil.which(tool):
            raise RuntimeError(f"missing native validation tool: {tool}")
    xvfb = wm = None
    with (output / "xvfb.log").open("w") as log:
        try:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0",
                                     "2304x1800x24", "-ac"], stdout=subprocess.PIPE,
                                    stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb did not report a display number")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb failed to start")
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR="/tmp",
                       DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            with (output / "openbox.log").open("w") as wm_log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=wm_log, stderr=wm_log)
                wait_for_wm(env)
                results = []
                for scale in (1.5, 2.0):
                    screenshot = output / f"x11-scale-{scale:g}.png"
                    command = [sys.executable, str(pathlib.Path(__file__).with_name("desktop_smoke.py")),
                               str(args.gallery), "--display", display, "--scale", str(scale),
                               "--screenshot", str(screenshot)]
                    run = subprocess.run(command, env=env, text=True, capture_output=True, timeout=40)
                    if run.returncode:
                        raise RuntimeError(run.stdout + run.stderr)
                    print(run.stdout.strip(), flush=True)
                    results.append({"backend": "X11", "scale": scale,
                                    "result": run.stdout.strip(), "screenshot": screenshot.name})
                return results
        finally:
            stop(wm)
            stop(xvfb)


def wayland(args, output):
    for tool in ("weston", "weston-screenshooter"):
        if not shutil.which(tool):
            raise RuntimeError(f"missing native validation tool: {tool}")
    compositor = gallery = None
    with tempfile.TemporaryDirectory(prefix="zgui-wayland-") as runtime:
        env = dict(os.environ, XDG_RUNTIME_DIR=runtime, WAYLAND_DISPLAY="zgui-test",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("DISPLAY", None)
        env.pop("WINIT_X11_SCALE_FACTOR", None)
        with (output / "weston-process.log").open("w") as log:
            try:
                compositor = subprocess.Popen(["weston", "--backend=headless", "--renderer=pixman",
                    "--width=1280", "--height=960", "--idle-time=0", "--no-config", "--debug",
                    "--socket=zgui-test", f"--log={output / 'weston.log'}"], env=env,
                    stdout=log, stderr=log)
                deadline = time.monotonic() + 10
                while not pathlib.Path(runtime, "zgui-test").exists():
                    if compositor.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Weston failed to create its Wayland socket")
                    time.sleep(.05)
                gallery_env = dict(env, ZGUI_SECONDS="5", ZGUI_SNAPSHOT="1")
                gallery = subprocess.Popen([str(args.gallery)], env=gallery_env,
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
                time.sleep(2)
                if gallery.poll() is not None:
                    raise RuntimeError("Wayland gallery exited before capture:\n" + gallery.stdout.read())
                shots = set(output.glob("*.png"))
                capture = subprocess.run(["weston-screenshooter"], env=dict(env, XDG_PICTURES_DIR=str(output)),
                    cwd=output, text=True, capture_output=True, timeout=10)
                if capture.returncode:
                    raise RuntimeError(capture.stdout + capture.stderr)
                screenshots = set(output.glob("*.png")) - shots
                if not screenshots:
                    raise RuntimeError("Weston capture produced no PNG")
                screenshot = sorted(screenshots)[0]
                target = output / "wayland-gallery.png"
                screenshot.rename(target)
                from PIL import Image
                with Image.open(target) as image:
                    colors = image.convert("RGB").getcolors(image.width * image.height)
                    background_pixels = next((count for count, color in colors if color == (16, 20, 28)), 0)
                    if background_pixels < 10000:
                        raise AssertionError(f"Wayland screenshot lacks the gallery surface: {background_pixels} theme pixels")
                text, _ = gallery.communicate(timeout=15)
                (output / "wayland-gallery.log").write_text(text)
                if gallery.returncode or 'SEMANTIC "Name"' not in text:
                    raise RuntimeError("Wayland gallery did not close normally:\n" + text)
                if args.windows:
                    lifecycle = subprocess.run([str(args.windows), "--smoke-test"], env=env,
                        text=True, capture_output=True, timeout=20)
                    if lifecycle.returncode or "multi-window smoke passed" not in lifecycle.stdout:
                        raise RuntimeError(lifecycle.stdout + lifecycle.stderr)
                    (output / "wayland-windows.log").write_text(lifecycle.stdout + lifecycle.stderr)
                print("PASS: Wayland native presentation, semantic snapshot, timed close, window lifecycle", flush=True)
                return {"backend": "Wayland", "compositor": "Weston headless pixman",
                        "theme_background_pixels": background_pixels, "screenshot": target.name,
                        "input_validation": "No seat: physical keyboard/pointer/IME not tested"}
            finally:
                stop(gallery)
                stop(compositor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gallery", type=pathlib.Path)
    parser.add_argument("--windows", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp/zgui-platform-validation"))
    parser.add_argument("--backend", choices=("all", "x11", "wayland"), default="all")
    args = parser.parse_args()
    args.gallery = args.gallery.resolve()
    if args.windows:
        args.windows = args.windows.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    results = []
    if args.backend in ("all", "x11"):
        results.extend(x11(args, output))
    if args.backend in ("all", "wayland"):
        results.append(wayland(args, output))
    (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
