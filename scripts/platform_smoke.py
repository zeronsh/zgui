#!/usr/bin/env python3
"""Validate zgx on isolated X11 HiDPI and headless Weston Wayland desktops.

Uses already-built gallery/windows examples. Each private compositor is stopped
on exit. This checks native behavior, not hardware GPU performance.
"""
import argparse
import contextlib
import ctypes
import json
import os
import pathlib
import re
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

    On a stock session bus zgx's settings read starts xdg-desktop-portal,
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


@contextlib.contextmanager
def private_runtime(prefix):
    """A private runtime directory (`XDG_RUNTIME_DIR`, mode 0700), removed
    when done past the FUSE filesystems session services mounted in it
    (`release_mounts`), which stopping their bus leaves disconnected: under
    load, removing it before they are released fails the probe."""
    runtime = tempfile.mkdtemp(prefix=prefix)
    try:
        yield runtime
    finally:
        release_mounts(runtime)
        shutil.rmtree(runtime, ignore_errors=True)


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
            settle_keyboard(env, deadline)
            start_keep_awake(env, check)
            return
        time.sleep(.05)
    raise RuntimeError("the window manager did not start")


def start_keep_awake(env, check):
    """`keep_awake` on `env`'s display, whose window manager's check window
    is `check`, in a process of its own, which ends with the display or
    this process."""
    code = f"import sys; sys.path.insert(0, {str(pathlib.Path(__file__).resolve().parent)!r}); " \
           f"import os, platform_smoke; platform_smoke.keep_awake(os.environ, {check!r}, {os.getpid()})"
    subprocess.Popen([sys.executable, "-c", code], env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)


class _ClientMessage(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
                ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                ("format", ctypes.c_int), ("data", ctypes.c_long * 5), ("pad", ctypes.c_long * 12)]


_x11 = ctypes.CDLL("libX11.so.6")
_x11.XOpenDisplay.argtypes, _x11.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
_x11.XInternAtom.argtypes, _x11.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
_x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(_ClientMessage)]
_x11.XFlush.argtypes = [ctypes.c_void_p]
_x11.XCloseDisplay.argtypes = [ctypes.c_void_p]


def keep_awake(env, check, parent, period=.5):
    """Wake the window manager every `period` seconds while its display
    lives, from a process of its own (`start_keep_awake`): Xlib exits the
    process its display goes from. Openbox 3.6 keeps a queue of X events
    of its own (obt/xqueue.c) beside Xlib's, filled when it looks ahead for
    an event (managing a window, a timer), but its main loop's source asks
    only XPending, Xlib's:
    events left in its own queue wait for the next X traffic to wake it. On
    a probe's idle desktop that is never, and a request the probe made (a
    window to map, a key to close it, a desktop count) went unanswered: 10
    stuck requests in 1,200 loaded starts were each answered as soon as an
    unrelated root property changed. The wake is a client message of a type
    no one knows (`_ZGX_WAKE`) sent to the root window for its substructure
    redirect, which only the window manager selects: nothing else hears it,
    and Openbox does nothing with it but wake."""
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.argtypes, x11.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    x11.XDefaultRootWindow.argtypes, x11.XDefaultRootWindow.restype = [ctypes.c_void_p], ctypes.c_ulong
    x11.XInternAtom.argtypes, x11.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
    x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long,
                               ctypes.POINTER(_ClientMessage)]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    # The display's number may be another's by now: this one's window
    # manager, or none.
    found = subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], env=env, capture_output=True, text=True)
    display = x11.XOpenDisplay(env["DISPLAY"].encode())
    if not display or found.stdout.rsplit("# ", 1)[-1].strip() != check:
        return
    root = x11.XDefaultRootWindow(display)
    event = _ClientMessage(type=33, send_event=1, display=display, window=root,  # ClientMessage
                           message_type=x11.XInternAtom(display, b"_ZGX_WAKE", 0), format=32)
    substructure_redirect = 1 << 20
    # Until the display goes, when Xlib exits this process, or the probe.
    while os.getppid() == parent:
        x11.XSendEvent(display, root, 0, substructure_redirect, ctypes.byref(event))
        x11.XFlush(display)
        time.sleep(period)


def settle_keyboard(env, deadline):
    """Send the display's first key now, before any app, and wait until the
    window manager has taken it in. The first key xdotool sends (through
    XTEST) switches the core keyboard to XTEST's: a keyboard map change, on
    which Openbox ungrabs its bindings and grabs them again. An Alt+F4 sent
    meanwhile reached the app instead of Openbox's Close, and the app stayed
    open (one probe run in thirty, under load). The window manager handles X
    events in order, so once it has answered a request sent after the key,
    it has rebound: a desktop added and taken away. A request left in
    Openbox's own queue (`keep_awake`) is sent again each half second, which
    wakes it; any it answers was sent after the key. The desktop count it
    settled on, or None without one."""
    def desktops():
        found = subprocess.run(["xprop", "-root", "_NET_NUMBER_OF_DESKTOPS"], env=env, capture_output=True,
                               text=True).stdout.rsplit("=", 1)[-1].strip()
        return int(found) if found.isdigit() else None
    subprocess.run(["xdotool", "key", "shift"], env=env, check=True)
    count = desktops()
    if count is None:
        return None
    for want in (count + 1, count):
        asked = 0.
        while (shown := desktops()) != want:
            if time.monotonic() > deadline:
                raise RuntimeError(f"the window manager did not answer after the first key: asked for {want} "
                                   f"desktops (from {count}), it shows {shown}")
            if time.monotonic() - asked > .5:
                if asked:
                    print(f"settle_keyboard: asked again for {want} desktops (from {count}), it shows {shown}",
                          file=sys.stderr, flush=True)
                subprocess.run(["xdotool", "set_num_desktops", str(want)], env=env, check=True)
                asked = time.monotonic()
            time.sleep(.02)
    return count


def close_window(env, window):
    """Ask `window` (an id) to close as a window manager does for Alt+F4 or
    its close button: `WM_DELETE_WINDOW`, with no key sent. Under IBus's
    asynchronous mode a key still being handled as the window closes (Alt+F4's
    Alt) crashed ibus-x11 1.5.29 (SIGSEGV, 92 of 120 loaded runs with keys in
    flight; none in its default synchronous mode), and libX11 waits for a
    crashed input method's reply forever (#564)."""
    display = _x11.XOpenDisplay(env["DISPLAY"].encode())
    if not display:
        raise RuntimeError("no display to close the window on")
    try:
        event = _ClientMessage(type=33, send_event=1, display=display, window=int(window, 0),  # ClientMessage
                               message_type=_x11.XInternAtom(display, b"WM_PROTOCOLS", 0), format=32)
        event.data[0] = _x11.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        event.data[1] = 0  # CurrentTime
        _x11.XSendEvent(display, int(window, 0), 0, 0, ctypes.byref(event))
        _x11.XFlush(display)
    finally:
        _x11.XCloseDisplay(display)


def wait_for_exit(app, env, output, seconds):
    """Wait `seconds` for `app` to exit, as one closed should. One that is
    still running is a hang on the way out: its threads' stacks and the
    display go to `output`/display.log before the timeout is raised (one
    native-ime run on CI printed its last words, its window closed, and
    did not exit within 25 s)."""
    try:
        return app.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        dump_display(env, output, app.pid)
        # The input method's side too (#564: blocked in an XIM round trip,
        # the app waits on ibus-x11 or the two wait on each other).
        with (output / "display.log").open("a") as log:
            # Who holds each input method's selection (`@server=…`): with
            # #578 an app asks before a round trip, and one still hung.
            log.write("\n" + input_method_owners(env))
            ibus = [pid for name in ("ibus-x11", "ibus-daemon", "ibus-ui-gtk3", "ibus-engine-lib")
                    for pid in display_processes(env, name)]
            for pid in ibus:
                state = subprocess.run(["ps", "-o", "pid,stat,wchan:30,comm", "-p", str(pid)], capture_output=True,
                                       text=True).stdout
                stacks = subprocess.run(["sudo", "-n", "gdb", "-q", "-batch", "-p", str(pid), "-ex",
                                         "thread apply all bt 25"], capture_output=True, text=True, timeout=60)
                log.write(f"\n$ ps; gdb -p {pid}\n{state}{stacks.stdout}{stacks.stderr}")
            if not ibus:
                log.write("\nno IBus process on this display\n")
        raise


def input_method_owners(env):
    """Each XIM server the root names (`XIM_SERVERS`) and who owns its
    selection: the window, and what xwininfo says of it."""
    import ctypes
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.argtypes, x11.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    x11.XInternAtom.argtypes, x11.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
    x11.XGetSelectionOwner.argtypes, x11.XGetSelectionOwner.restype = [ctypes.c_void_p, ctypes.c_ulong], ctypes.c_ulong
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    servers = subprocess.run(["xprop", "-root", "XIM_SERVERS"], env=env, capture_output=True, text=True).stdout
    names = re.findall(r"@server=[^,\s]+", servers)
    display = x11.XOpenDisplay(env["DISPLAY"].encode())
    if not display:
        return "input method owners: no display\n"
    report = f"input method owners ({servers.strip()}):\n"
    for name in names:
        owner = x11.XGetSelectionOwner(display, x11.XInternAtom(display, name.encode(), 1))
        info = subprocess.run(["xwininfo", "-id", hex(owner)], env=env, capture_output=True, text=True).stdout \
            if owner else ""
        report += f"{name}: owner {hex(owner)}\n{info}"
    x11.XCloseDisplay(display)
    return report


def display_processes(env, name):
    """The processes called `name` on `env`'s display (by their DISPLAY)."""
    found = []
    for proc in pathlib.Path("/proc").iterdir():
        try:
            if proc.name.isdigit() and (proc / "comm").read_text().strip() == name and \
                    f"DISPLAY={env.get('DISPLAY')}\0".encode() in (proc / "environ").read_bytes():
                found.append(int(proc.name))
        except OSError:
            continue
    return found


def dump_display(env, output, pid=None):
    """What the display shows, for a probe that cannot find its window: the
    window tree, the root's properties, the pid's windows and a screenshot."""
    def run(*command):
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
        return f"$ {' '.join(command)}\n{result.stdout}{result.stderr}\n"
    report = run("xwininfo", "-root", "-tree") + run("xprop", "-root")
    if pid:
        report += run("xdotool", "search", "--pid", str(pid))
        # Where a window that never showed is stuck (zgx shows a window once
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
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0",
                                     "2304x1800x24", "-ac"], stdout=subprocess.PIPE,
                                    stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
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
    with tempfile.TemporaryDirectory(prefix="zgx-wayland-") as runtime:
        env = dict(os.environ, XDG_RUNTIME_DIR=runtime, WAYLAND_DISPLAY="zgx-test",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("DISPLAY", None)
        env.pop("WINIT_X11_SCALE_FACTOR", None)
        with (output / "weston-process.log").open("w") as log:
            try:
                compositor = subprocess.Popen(["weston", "--backend=headless", "--renderer=pixman",
                    "--width=1280", "--height=960", "--idle-time=0", "--no-config", "--debug",
                    "--socket=zgx-test", f"--log={output / 'weston.log'}"], env=env,
                    stdout=log, stderr=log)
                deadline = time.monotonic() + 10
                while not pathlib.Path(runtime, "zgx-test").exists():
                    if compositor.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Weston failed to create its Wayland socket")
                    time.sleep(.05)
                gallery_env = dict(env, ZGX_SECONDS="5", ZGX_SNAPSHOT="1")
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
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp/zgx-platform-validation"))
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
