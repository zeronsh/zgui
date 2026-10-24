#!/usr/bin/env python3
"""The catalogue's components on Linux, through a private live AT-SPI bus and
real X11 keys: each one's role and name, the data grid's selection and
header sort, a tree opening left to right and right to left (its child
showing; AccessKit's bridge maps no expanded state), rows selected
and copied to the clipboard (text and HTML), and the segmented control,
stepper, date picker, time field and tabs answering the keyboard. The
example prints what each change reaches (`COMPONENTS …`); the probe checks
that beside the accessibility tree."""
import argparse
import json
import os
import pathlib
import re
import select
import signal
import subprocess
import sys
import tempfile
import time
from platform_smoke import release_mounts, stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--session", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error("build the components_probe example first")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if not args.session:
        (args.output / "results.json").unlink(missing_ok=True)
        env = dict(os.environ, ZGUI_PRIVATE_ATSPI="1")
        for name in ("AT_SPI_BUS_ADDRESS", "DBUS_SESSION_BUS_ADDRESS", "DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(name, None)
        result = subprocess.run(["dbus-run-session", "--", sys.executable, str(pathlib.Path(__file__).resolve()),
                                 str(args.binary), "--output", str(args.output), "--session"], env=env)
        raise SystemExit(result.returncode)
    if os.environ.get("ZGUI_PRIVATE_ATSPI") != "1":
        raise RuntimeError("the private session must be created by this runner")
    xvfb = wm = launcher = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-components-") as runtime:
        os.chmod(runtime, 0o700)
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1000x800x24", "-ac"],
                                        stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            os.environ.update(DISPLAY=display, XDG_RUNTIME_DIR=runtime, WINIT_X11_SCALE_FACTOR="1",
                              XDG_CONFIG_HOME=str(pathlib.Path(runtime, "config")),
                              XDG_CACHE_HOME=str(pathlib.Path(runtime, "cache")),
                              GSETTINGS_BACKEND="memory")
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], stdout=log, stderr=log)
            wait_for_wm(os.environ)
            with (args.output / "a11y-bus.log").open("w") as log:
                launcher = subprocess.Popen(["/usr/libexec/at-spi-bus-launcher", "--launch-immediately", "--a11y=1", "--screen-reader=1"],
                                            stdout=log, stderr=log, start_new_session=True)
            import gi
            gi.require_version("Atspi", "2.0")
            from gi.repository import Atspi, Gio, GLib
            session = Gio.bus_get_sync(Gio.BusType.SESSION, None)

            def call(interface, method, parameters):
                return session.call_sync("org.a11y.Bus", "/org/a11y/bus", interface, method, parameters, None,
                                         Gio.DBusCallFlags.NO_AUTO_START, 2000, None)
            deadline = time.monotonic() + 8
            while True:
                try:
                    for name in ("IsEnabled", "ScreenReaderEnabled"):
                        call("org.freedesktop.DBus.Properties", "Set",
                             GLib.Variant("(ssv)", ("org.a11y.Status", name, GLib.Variant("b", True))))
                    address = call("org.a11y.Bus", "GetAddress", None).unpack()[0]
                    break
                except GLib.Error:
                    if time.monotonic() > deadline:
                        raise
                    time.sleep(.05)
            os.environ["AT_SPI_BUS_ADDRESS"] = address
            Atspi.init()
            context = GLib.MainContext.default()

            def pump():
                while context.pending():
                    context.iteration(False)
            log_path = args.output / "application.log"
            with log_path.open("w") as log:
                app = subprocess.Popen([str(args.binary)], stdout=log, stderr=log)

                def tree():
                    result = []

                    def visit(node, depth, parent):
                        if node is None or depth > 40:
                            return
                        node.clear_cache()
                        result.append((node, depth, parent))
                        for index in range(node.get_child_count()):
                            visit(node.get_child_at_index(index), depth + 1, node)
                    visit(Atspi.get_desktop(0), 0, None)
                    return result

                def snapshot():
                    out = []
                    for node, depth, _ in tree():
                        try:
                            out.append(dict(depth=depth, name=node.get_name(), role=node.get_role_name(),
                                            states=[s.value_nick for s in node.get_state_set().get_states()]))
                        except GLib.Error:
                            pass
                    return out

                def latest():
                    lines = [l for l in log_path.read_text().splitlines() if l.startswith("COMPONENTS ")]
                    return lines[-1] if lines else ""

                def fail(message):
                    (args.output / "tree.json").write_text(json.dumps(snapshot(), indent=2) + "\n")
                    raise AssertionError(f"{message}\nlast state: {latest()}\ntree: {args.output / 'tree.json'}")

                def wait_for(function, description, timeout=8):
                    deadline = time.monotonic() + timeout
                    while time.monotonic() < deadline:
                        pump()
                        try:
                            value = function()
                            if value:
                                return value
                        except GLib.Error:
                            pass
                        time.sleep(.05)
                    fail("timed out: " + description)

                def find(name, within=None, role=None):
                    for node, _, _ in tree() if within is None else [(n, 0, None) for n in descendants(within)]:
                        try:
                            if node.get_name() == name and (role is None or node.get_role_name() == role):
                                return node
                        except GLib.Error:
                            pass
                    return None

                def descendants(node):
                    out = []

                    def visit(n, depth):
                        if n is None or depth > 30:
                            return
                        n.clear_cache()
                        out.append(n)
                        for index in range(n.get_child_count()):
                            visit(n.get_child_at_index(index), depth + 1)
                    visit(node, 0)
                    return out

                def state(node, flag):
                    node.clear_cache()
                    return node.get_state_set().contains(flag)

                def reached(pattern, description):
                    wait_for(lambda: re.search(pattern, latest()), description)

                def focus(node, description):
                    if not node.get_component_iface().grab_focus():
                        fail("could not focus " + description)
                    # A tree's or grid's focus shows on its current item, its
                    # active descendant.
                    wait_for(lambda: any(state(n, Atspi.StateType.FOCUSED) for n in descendants(node)),
                             description + " focused")

                def key(*keys):
                    subprocess.run(["xdotool", "key", "--clearmodifiers", *keys], check=True)
                    time.sleep(.1)

                wait_for(lambda: any(node.get_process_id() == app.pid for node, _, _ in tree()), "the probe on the AT-SPI desktop")
                checks = []

                # Roles and names.
                expected = [
                    ("Small", ("radio button",)),
                    ("Probe quantity", ("spin button",)),
                    ("Probe fruit", ("combo box",)),
                    ("Probe due: 2026-10-23", ("push button", "button")),
                    ("Hours", ("spin button",)),
                    ("Minutes", ("spin button",)),
                    ("Probe tabs", ("page tab list",)),
                    ("One", ("page tab",)),
                    ("Probe files", ("tree",)),
                    ("Probe files right to left", ("tree",)),
                    ("Probe grid", ("table", "tree table")),
                    ("Name", ("column header", "table column header")),
                ]
                for name, roles in expected:
                    node = wait_for(lambda: find(name), f"`{name}`")
                    if node.get_role_name() not in roles:
                        fail(f"`{name}` is a {node.get_role_name()!r}, not {roles}")
                checks.append("roles and names")

                # AccessKit's AT-SPI bridge has no Table interface (row and
                # column counts reach macOS and Windows): the grid says that
                # several rows can be selected, and its rows which are.
                grid = find("Probe grid")
                if not state(grid, Atspi.StateType.MULTISELECTABLE):
                    fail("the grid doesn't say several rows can be selected")
                checks.append("grid multiselectable")

                # A tree opens with Right left to right, with Left right to left.
                ltr = find("Probe files")
                focus(ltr, "the tree")
                key("Right")
                reached(r'open=\["a"\]', "Right opens the tree's first item")
                # Its child shows in the tree. (AccessKit's AT-SPI bridge maps
                # no expanded state: an item can't say it is open on Linux.)
                wait_for(lambda: find("Apple", within=ltr, role="tree item"), "the opened item's child in the tree")
                rtl = find("Probe files right to left")
                focus(rtl, "the right-to-left tree")
                key("Left")
                reached(r'open_rtl=\["a"\]', "Left opens it right to left")
                checks.append("tree keyboard, opened children, right to left")

                # The grid: rows selected by the arrows, Shift extending; ⌘C
                # (Ctrl+C here) copies them as text and HTML.
                focus(grid, "the grid")
                reached(r"selection=\[0\]", "focus selects the first row")
                key("Down")
                reached(r"selection=\[1\]", "Down selects the next row")
                key("shift+Down")
                reached(r"selection=\[1, 2\]", "Shift+Down extends the selection")

                def selected_rows():
                    return [n for n in descendants(grid) if n.get_role_name() == "table row"
                            and state(n, Atspi.StateType.SELECTED)]
                wait_for(lambda: len(selected_rows()) == 2, "two rows say they are selected")
                key("ctrl+c")

                def clipboard(target=None):
                    command = ["xclip", "-o", "-selection", "clipboard"] + (["-t", target] if target else [])
                    result = subprocess.run(command, capture_output=True, text=True, timeout=5)
                    return result.stdout if result.returncode == 0 else ""
                wait_for(lambda: clipboard() == "Item 1\t7\nItem 2\t14", "Ctrl+C copies the rows, tab-separated")
                html = wait_for(lambda: clipboard("text/html"), "the rows as HTML too")
                if "<table><tr><td>Item 1</td><td>7</td></tr>" not in html:
                    fail("the HTML copied: " + html)
                checks.append("grid selection and copy (text/plain and text/html)")
                # Up to the header, and Enter sorts its column.
                key("Up")
                key("Up")
                key("Up")
                key("Return")
                reached(r'sort=Some\("name\+"\)', "Enter on the header sorts")
                checks.append("grid header sort from the keyboard")

                # The controls.
                focus(find("Small"), "the segmented control")
                key("Right")
                reached(r"size=1 ", "Right chooses the next segment")
                focus(find("Probe quantity"), "the stepper")
                key("Up")
                reached(r"quantity=2 ", "Up steps the stepper")
                focus(find("Hours"), "the time field's hours")
                key("9")
                reached(r"start=09:30 ", "a digit types the hour")
                focus(find("Probe due: 2026-10-23"), "the date picker")
                key("Down")
                wait_for(lambda: find("Saturday, 24 October 2026"), "Down opens the calendar")
                key("Right")
                key("Return")
                reached(r"due=2026-10-24 ", "Enter chooses the day")
                focus(find("One"), "the first tab")
                key("Right")
                key("Return")
                reached(r"tab=two ", "the next tab is chosen")
                checks.append("segmented, stepper, time field, date picker and tabs from the keyboard")

                (args.output / "tree.json").write_text(json.dumps(snapshot(), indent=2) + "\n")
                subprocess.run(["import", "-display", display, "-window", "root", str(args.output / "components.png")], check=True, timeout=10)
                (args.output / "results.json").write_text(json.dumps(dict(passed=True, checks=checks, state=latest(),
                    backend="live AccessKit Unix AT-SPI bridge / private D-Bus / owned X11 / xdotool keys"), indent=2) + "\n")
                print("Components AT-SPI checks passed")
        finally:
            stop(app)
            stop(wm)
            if launcher is not None:
                try:
                    os.killpg(launcher.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                launcher.wait(timeout=5)
            stop(xvfb)
            release_mounts(runtime)


if __name__ == "__main__":
    main()
