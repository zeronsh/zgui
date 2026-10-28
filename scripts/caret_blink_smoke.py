#!/usr/bin/env python3
"""The text caret's blink on X11, as GTK and Qt take it from the session.
The primary_selection example's first editor is clicked into and its caret
timed from screen captures of the field (one image shown, one hidden):

- XSETTINGS (xsettingsd) `Net/CursorBlinkTime` 600 and
  `Gtk/CursorBlinkTimeout` 3, over a stand-in settings portal's other
  values: shown 400 ms and hidden 200 ms (GTK's two thirds and one), then
  solid from 3 s after the click, the field still focused. Changed to
  `Net/CursorBlink` 0 and the window activated again: solid;
- no blink in XSETTINGS: the portal's GNOME `cursor-blink-time` 900 and
  `cursor-blink-timeout` 2: shown 600 ms, hidden 300, solid after 2 s;
- the portal's KDE `CursorBlinkRate` 500: 250 ms each way, still blinking
  past GTK's timeouts (Qt's caret has none)."""
import argparse
import hashlib
import json
import os
import pathlib
import select
import signal
import statistics
import subprocess
import time

from platform_smoke import private_runtime, stop, wait_for_wm

PORTAL = r'''
import json, sys
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
settings = {tuple(key.split(" ")): value for key, value in json.loads(sys.argv[1]).items()}

def call(conn, sender, path, iface, method, params, invocation):
    value = settings.get(tuple(params.unpack()))
    if value is None:
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    variant = GLib.Variant("s", value) if isinstance(value, str) else GLib.Variant("i", value)
    invocation.return_value(GLib.Variant("(v)", (variant,)))

def acquired(conn, name):
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call,
                         lambda *_: GLib.Variant("u", 2), None)
    print("PORTAL ready", flush=True)

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.MainLoop().run()
'''

GNOME = "org.gnome.desktop.interface"
# The first editor, window-relative: its field's padding and text.
FIELD = (20, 20, 460, 84)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("primary_selection", type=pathlib.Path, help="the primary_selection example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, result = [], {}

    def launch(label, command, env, **options):
        log = (args.output / f"{label}.log").open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    def until(what, check, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            found = check()
            if found:
                return found
            time.sleep(.05)
        raise RuntimeError(f"timed out waiting for {what}")

    def session(label, portal_settings, xsettings):
        """A bus with the stand-in portal, Xvfb, Openbox and xsettingsd."""
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
        for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        bus = launch(f"{label}-dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                     stdout=subprocess.PIPE, text=True)
        if not select.select([bus.stdout], [], [], 10)[0]:
            raise RuntimeError("dbus-daemon startup timed out")
        env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
        portal = launch(f"{label}-portal", ["/usr/bin/python3", "-c", PORTAL, json.dumps(portal_settings)], env,
                        stdout=subprocess.PIPE, text=True)
        if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
            raise RuntimeError("the stand-in portal did not start")
        xvfb = launch(f"{label}-xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0",
                                        "1024x768x24", "-ac"], env, stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch(f"{label}-openbox", ["openbox"], env)
        wait_for_wm(env)
        config = args.output / f"{label}-xsettingsd.conf"
        config.write_text(xsettings)
        daemon = launch(f"{label}-xsettingsd", ["xsettingsd", "-c", str(config)], env)
        until("xsettingsd's selection", lambda: "Took ownership" in (args.output / f"{label}-xsettingsd.log").read_text())
        return env, config, daemon

    def opened(label, env):
        app = launch(label, [str(args.primary_selection.resolve())], env)

        def visible():
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                   env=env, capture_output=True, text=True)
            if app.poll() is not None:
                raise RuntimeError("the example exited")
            return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None
        window = until("the example's window", visible)
        xdo(env, "windowactivate", "--sync", window)
        time.sleep(.6)
        return app, window

    def xdo(env, *parts):
        subprocess.check_call(["xdotool", *map(str, parts)], env=env)

    def field(env, window):
        """The editor's box on the screen."""
        found = subprocess.run(["xdotool", "getwindowgeometry", "--shell", window], env=env,
                               capture_output=True, text=True, check=True).stdout
        at = dict(line.split("=", 1) for line in found.split())
        x, y = int(at["X"]), int(at["Y"])
        return (x + FIELD[0], y + FIELD[1], x + FIELD[2], y + FIELD[3])

    def focus(env, window):
        """Click into the editor and put the caret at its end: from here it
        shows solid, and its blink (and timeout) start again."""
        xdo(env, "mousemove", "--window", window, 110, 42, "click", 1)
        xdo(env, "key", "End")
        return time.monotonic()

    def watch(env, box, seconds, shown):
        """The caret over `seconds`: (time, shown) at each capture, and any
        image of the field that is neither `shown` nor the first hidden."""
        from PIL import ImageGrab
        samples, images = [], {}
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            at = time.monotonic()
            image = ImageGrab.grab(bbox=box, xdisplay=env["DISPLAY"]).tobytes()
            key = hashlib.sha1(image).hexdigest()
            images.setdefault(key, image)
            samples.append((at, key == shown, key))
            time.sleep(.008)
        return samples, set(images)

    def phases(samples):
        """How long each whole phase lasted (shown, hidden), in ms: the
        first and last are cut by the window watched."""
        runs, start = [], 0
        for index in range(1, len(samples) + 1):
            if index == len(samples) or samples[index][1] != samples[start][1]:
                runs.append((samples[start][1], samples[start][0], samples[index - 1][0]))
                start = index
        whole = []
        for (shown, began, _), (_, after, _) in zip(runs[1:-1], runs[2:]):
            whole.append((shown, (after - began) * 1000))
        return ([ms for shown, ms in whole if shown], [ms for shown, ms in whole if not shown])

    def blinks(env, window, box, seconds):
        """Focus the editor and time its blink for `seconds`: the shown image
        (captured at once, solid as it is focused), and each phase."""
        from PIL import ImageGrab
        clicked = focus(env, window)
        time.sleep(.12)
        shown = hashlib.sha1(ImageGrab.grab(bbox=box, xdisplay=env["DISPLAY"]).tobytes()).hexdigest()
        samples, images = watch(env, box, seconds, shown)
        on, off = phases(samples)
        return clicked, shown, samples, images, on, off

    def near(measured, expected, slack=.25):
        return bool(measured) and abs(statistics.median(measured) - expected) <= max(expected * slack, 60)

    def solid(env, box, shown, seconds):
        samples, _ = watch(env, box, seconds, shown)
        return bool(samples) and all(s[1] for s in samples)

    with private_runtime("zgx-caret-blink-") as private:
        try:
            # XSETTINGS first, over the portal's.
            env, config, daemon = session("xsettings", {
                f"{GNOME} cursor-blink-time": 2000, f"{GNOME} cursor-blink-timeout": 20,
            }, "Net/CursorBlink 1\nNet/CursorBlinkTime 600\nGtk/CursorBlinkTimeout 3\n")
            app, window = opened("xsettings-app", env)
            box = field(env, window)
            clicked, shown, samples, images, on, off = blinks(env, window, box, 2.4)
            result["xsettings: images of the field"] = len(images)
            result["xsettings: shown, hidden (ms)"] = (on, off)
            result["xsettings: 400 ms shown, 200 hidden"] = len(images) == 2 and near(on, 400) and near(off, 200)
            # Past the timeout (3 s from the click), still focused.
            time.sleep(max(0, clicked + 3.4 - time.monotonic()))
            result["xsettings: solid past the 3 s timeout"] = solid(env, box, shown, 1.2)
            # Blink turned off in the settings app: solid once activated again.
            config.write_text("Net/CursorBlink 0\nNet/CursorBlinkTime 600\nGtk/CursorBlinkTimeout 3\n")
            daemon.send_signal(signal.SIGHUP)
            log = args.output / "xsettings-xsettingsd.log"
            until("xsettingsd's reload", lambda: log.read_text().count("Loaded 3 settings") >= 2)
            other = launch("xmessage", ["xmessage", "-geometry", "+600+500", "elsewhere"], env)
            elsewhere = until("xmessage's window", lambda: subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--class", "xmessage"], env=env,
                capture_output=True, text=True).stdout.split()[:1])[0]
            xdo(env, "windowactivate", "--sync", elsewhere)
            time.sleep(.3)
            xdo(env, "windowactivate", "--sync", window)
            time.sleep(.4)
            focus(env, window)
            time.sleep(.12)
            result["blink off: solid"] = solid(env, box, shown, 1.2)
            stop(other)
            stop(app)

            # GNOME's, from the portal, where XSETTINGS gives no blink.
            env, _, _ = session("gnome", {
                f"{GNOME} cursor-blink-time": 900, f"{GNOME} cursor-blink-timeout": 2,
            }, "Net/DoubleClickTime 400\n")
            app, window = opened("gnome-app", env)
            box = field(env, window)
            clicked, shown, _, images, on, off = blinks(env, window, box, 1.8)
            result["gnome: shown, hidden (ms)"] = (on, off)
            result["gnome: 600 ms shown, 300 hidden"] = len(images) == 2 and near(on, 600) and near(off, 300)
            time.sleep(max(0, clicked + 2.4 - time.monotonic()))
            result["gnome: solid past the 2 s timeout"] = solid(env, box, shown, 1.2)
            stop(app)

            # KDE's flash time: half each way, no timeout.
            env, _, _ = session("kde", {"org.kde.kdeglobals.KDE CursorBlinkRate": "500"}, "Net/DoubleClickTime 400\n")
            app, window = opened("kde-app", env)
            box = field(env, window)
            clicked, shown, _, images, on, off = blinks(env, window, box, 1.6)
            result["kde: shown, hidden (ms)"] = (on, off)
            result["kde: 250 ms each way"] = len(images) == 2 and near(on, 250) and near(off, 250)
            # Past GTK's longest timeout here: blinking still.
            time.sleep(max(0, clicked + 3.0 - time.monotonic()))
            samples, images = watch(env, box, 1.2, shown)
            on, off = phases(samples)
            result["kde: still blinking at 3 s"] = len(images) == 2 and bool(on) and bool(off)
            stop(app)
        finally:
            for process in reversed(processes):
                stop(process)
    (args.output / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    failed = [name for name, value in result.items() if value is False]
    if failed:
        raise SystemExit(f"FAIL: {', '.join(failed)}")
    print("PASS: the caret blinks at the session's rate, stops at its timeout, and stays solid with blink off")


if __name__ == "__main__":
    main()
