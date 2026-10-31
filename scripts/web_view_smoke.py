#!/usr/bin/env python3
"""A web page in a zgx window on Linux, in an owned session: WebKitGTK's
page, drawn offscreen and shown as an image (`web_view_probe`, built with
`--features webkitgtk`).

Checks that the page shows, clipped to its box's rounded corners, with zgx
content drawn over it; that clicks land where they are on the page, typing
reaches its field and the wheel scrolls it; that a `<select>`'s popup (a zgx
menu) chooses from the keyboard; that keys go to the page first and those it
leaves alone reach the app's shortcuts, while a reserved one never reaches
the page; that what the page copies reaches other clients and what they
copy pastes into the page; and that a script evaluates and the page's
messages carry their origin.

`--wayland`: the app on Wayland (Sway on an X11 output, no DISPLAY for the
app; the clipboard through wl-clipboard). `--ime`: Pinyin composed into the
page's field instead (IBus over XIM on X11; Fcitx5 over text-input-v3 in
Sway with `--wayland`), checked as the page's composition events and value.
`--transparent`: a transparent page with no background of its own shows
its box's yellow through it. `--orca` (X11): over a private AT-SPI bus, the
page's own accessible tree is a child of its box (an internal frame) and
has the box for its parent, and Orca reads the page's field as it takes
focus.
"""
import argparse
import json
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import sys
import time

from platform_smoke import private_runtime, stop, use_ibus_engine, wait_for_ibus_xim, wait_for_wm
from sway_host import wait_for_sway_host

# The probe's geometry (window points; probes run at scale 1).
PAGE = (40, 40, 600, 400)
COLOURS = {
    "page": ((340, 340), (0, 0, 255)),
    "target": ((70, 150), (0, 255, 0)),
    "square over the page": ((620, 420), (255, 0, 0)),
    "clipped corner": ((42, 42), (0x20, 0x20, 0x20)),
}


def near(found, wanted, tolerance=24):
    return all(abs(a - b) <= tolerance for a, b in zip(found[:3], wanted))


class Session:
    def __init__(self, output, env, window, origin=(0, 0), wayland=None):
        self.output, self.env, self.window, self.origin = output, env, window, origin
        self.wayland = wayland
        self.log = output / "application.log"

    def xdo(self, *parts):
        subprocess.run(["xdotool", *map(str, parts)], env=self.env, check=True)

    def click(self, x, y, button=1):
        self.xdo("mousemove", "--window", self.window, x, y, "click", button)
        time.sleep(.5)

    def key(self, *keys):
        self.xdo("key", "--clearmodifiers", *keys)
        time.sleep(.5)

    def type(self, text):
        self.xdo("type", "--clearmodifiers", "--delay", 90, text)
        time.sleep(.5)

    def grab(self):
        from PIL import ImageGrab
        image = ImageGrab.grab(xdisplay=self.env["DISPLAY"]).convert("RGB")
        image.save(self.output / "window.png")
        return lambda x, y: image.getpixel((self.origin[0] + x, self.origin[1] + y))

    def clipboard(self, text=None):
        """Another client's view of the clipboard: read it, or copy `text`."""
        if self.wayland:
            command = ["wl-copy"] if text is not None else ["wl-paste", "--no-newline"]
            env = self.wayland
        else:
            command = ["xclip", "-selection", "clipboard"] + (["-i"] if text is not None else ["-o"])
            env = self.env
        found = subprocess.run(command, env=env, input=text, capture_output=text is None, text=True, timeout=5)
        return found.stdout if text is None else None

    def lines(self, prefix):
        return [line[len(prefix):] for line in self.log.read_text().splitlines() if line.startswith(prefix)]

    def messages(self):
        out = []
        for line in self.lines("PAGE "):
            body, origin, main = re.fullmatch(r"(.*) origin=(\S*) main=(\w+)", line).groups()
            out.append((json.loads(body), origin, main == "true"))
        return out

    def wait(self, check, what, seconds=10):
        deadline = time.monotonic() + seconds
        while True:
            found = check()
            if found:
                return found
            if time.monotonic() > deadline:
                raise AssertionError(f"{what}; see application.log")
            time.sleep(.1)


def end(owned):
    """Stop what the probe started, last first; those in sessions of their
    own (D-Bus, the input method) with their children."""
    for process in reversed(owned):
        if os.getpgid(process.pid) == process.pid if process.poll() is None else False:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except (ProcessLookupError, PermissionError):
                pass
        stop(process)


def page_checks(session, ime, transparent=False):
    session.wait(lambda: session.lines("EVAL "), "the page never loaded", 30)
    colours = dict(COLOURS)
    if transparent:
        colours["page"] = (colours["page"][0], (255, 255, 0))
    checks = {}
    evaluated = session.lines("EVAL ")[0]
    if json.loads(evaluated) != "probe page 3":
        raise AssertionError(f"evaluated {evaluated}")
    checks["script evaluated"] = evaluated
    # The page's frame reaches the window a frame after it loads.
    def shown():
        pixel = session.grab()
        found = {name: pixel(*at) for name, (at, _) in colours.items()}
        return found if all(near(found[name], colour) for name, (_, colour) in colours.items()) else None
    try:
        checks["pixels"] = session.wait(shown, "the page did not show as expected")
    except AssertionError:
        pixel = session.grab()
        raise AssertionError("pixels: " + repr({name: (pixel(*at), colour) for name, (at, colour) in colours.items()}))
    if transparent:
        return checks
    if ime:
        return ime(session, checks)
    session.click(160, 180)
    click = session.wait(lambda: [m for m in session.messages() if "click" in m[0]], "the click never reached the page")[0]
    if click[0]["click"] != [100, 40] or click[1] != "https://probe.zgx.test" or not click[2]:
        raise AssertionError(f"click heard as {click}")
    checks["click"] = click[0]
    session.click(120, 80)
    session.type("hello")
    session.wait(lambda: any(m[0].get("value") == "hello" for m in session.messages()), "typing never reached the field")
    checks["typed"] = "hello"
    for key in ("ctrl+j", "ctrl+k", "ctrl+l"):
        session.key(key)
    # Ctrl+K reaches the app after a round trip through the page's process,
    # Ctrl+L (reserved) at once: either may come first.
    session.wait(lambda: {"k", "l"} <= set(session.lines("ACTION ")), "the app's shortcuts never fired")
    actions = sorted(session.lines("ACTION "))
    heard = [m[0]["key"] for m in session.messages() if m[0].get("ctrl") and m[0].get("key") != "Control"]
    # Ctrl+J: the page's. Ctrl+K: left alone, so the app's. Ctrl+L: reserved.
    if actions != ["k", "l"] or heard != ["j", "k"]:
        raise AssertionError(f"shortcuts: app {actions}, page {heard}")
    checks["keys"] = {"app": actions, "page": heard}
    # Copied in the page, read by another client; copied there, pasted here.
    session.key("ctrl+a")
    session.key("ctrl+c")
    copied = session.wait(lambda: session.clipboard() == "hello", "the page's copy never reached the clipboard")
    checks["copied"] = copied and "hello"
    session.clipboard("from elsewhere")
    time.sleep(.3)
    session.key("ctrl+a")
    session.key("ctrl+v")
    session.wait(lambda: any(m[0].get("value") == "from elsewhere" for m in session.messages()), "the clipboard never pasted into the page")
    checks["pasted"] = "from elsewhere"
    # The select's popup, a zgx menu: Down to Beta, Return chooses it.
    session.click(500, 75)
    session.key("Down")
    session.key("Return")
    session.wait(lambda: any(m[0].get("chose") == "Beta" for m in session.messages()), "the select's popup never chose")
    checks["chose"] = "Beta"
    for _ in range(3):
        session.click(300, 300, 5)
    scrolled = session.wait(lambda: [m for m in session.messages() if "scrolled" in m[0]], "the wheel never scrolled the page")
    checks["scrolled"] = scrolled[0][0]["scrolled"]
    return checks


def compose(session, checks):
    """Pinyin "nihao", then Space: 你好 composed in the page's field."""
    session.wait(lambda: any(m[0].get("composition") == "start" for m in session.messages()), "no composition started", 15)
    session.key("space")
    session.wait(lambda: any(m[0].get("value") == "你好" for m in session.messages()), "你好 never reached the field", 15)
    ended = [m[0] for m in session.messages() if m[0].get("composition") == "end"]
    if not ended or ended[-1].get("data") != "你好":
        raise AssertionError(f"composition ended as {ended}")
    checks["composed"] = "你好"
    return checks


def start_orca(env, output, owned):
    """A private AT-SPI bus, accessibility on, and Orca on it (its speech in
    its debug log, which needs no speech server)."""
    env["GSETTINGS_BACKEND"] = "memory"
    with (output / "a11y-bus.log").open("w") as log:
        owned.append(subprocess.Popen(["/usr/libexec/at-spi-bus-launcher", "--launch-immediately", "--a11y=1", "--screen-reader=1"],
                                      env=env, stdout=log, stderr=log, start_new_session=True))
    def bus(*call):
        return subprocess.run(["gdbus", "call", "--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", *call],
                              env=env, capture_output=True, text=True)
    deadline = time.monotonic() + 10
    while True:
        address = bus("org.a11y.Bus.GetAddress")
        if address.returncode == 0:
            break
        if time.monotonic() > deadline:
            raise RuntimeError("the accessibility bus never answered: " + address.stderr)
        time.sleep(.1)
    for name in ("IsEnabled", "ScreenReaderEnabled"):
        bus("org.freedesktop.DBus.Properties.Set", "org.a11y.Status", name, "<true>")
    env["AT_SPI_BUS_ADDRESS"] = re.search(r"'([^']+)'", address.stdout).group(1)
    with (output / "orca.log").open("w") as log:
        orca = subprocess.Popen(["orca", "--replace", "--debug-file=" + str(output / "orca-debug.log")], env=env, stdout=log, stderr=log)
    owned.append(orca)
    deadline = time.monotonic() + 20
    while "Screen reader on." not in spoken(output):
        if time.monotonic() > deadline or orca.poll() is not None:
            raise RuntimeError("Orca never started; see orca.log")
        time.sleep(.2)


def spoken(output):
    """What Orca said so far (its debug log's speech)."""
    path = output / "orca-debug.log"
    text = path.read_text(errors="replace") if path.exists() else ""
    return [m.group(1) for m in re.finditer(r"SPEECH OUTPUT: '([^']*)'", text)]


def orca_checks(session, env, output, checks):
    """The page's tree under its box, and what Orca reads of it."""
    tree = subprocess.run([sys.executable, "-c", TREE], env=env, capture_output=True, text=True, timeout=60)
    (output / "tree.txt").write_text(tree.stdout + tree.stderr)
    if "EMBEDDED" not in tree.stdout:
        raise AssertionError("the page's tree is not under its box; see tree.txt")
    checks["tree"] = tree.stdout.strip().splitlines()[0]
    checks["heading in the tree"] = "Probe heading"
    session.click(120, 80)
    session.wait(lambda: "Probe field entry." in spoken(output), "Orca never read the focused field", 20)
    checks["focused field"] = "Probe field entry."
    # Orca's heading keys read the page's heading too, in browse mode; but
    # GTK's own window, active for the page's focus, sometimes swaps Orca's
    # script as Escape switches modes, so that isn't asserted (the tree
    # check above finds the heading).
    return checks


# Over the accessibility bus: the probe's box (an internal frame named Web
# page) holds the page's document, which holds its heading, and the
# document's ancestors reach the box and the window. One application.
TREE = """
import gi, time
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi
Atspi.init()
Atspi.set_timeout(1500, 3000)
def children(node):
    node.clear_cache()
    return [node.get_child_at_index(i) for i in range(node.get_child_count())]
def find(node, role, name=None, depth=0):
    if node is None or depth > 14:
        return None
    if node.get_role_name() == role and (name is None or node.get_name() == name):
        return node
    for child in children(node):
        found = find(child, role, name, depth + 1)
        if found:
            return found
deadline = time.monotonic() + 30
frame = document = None
while time.monotonic() < deadline:
    apps = [a for a in children(Atspi.get_desktop(0)) if a and a.get_name() == "web_view_probe"]
    frame = next(filter(None, (find(a, "internal frame", "Web page") for a in apps)), None)
    document = frame and find(frame, "document web")
    if document and find(document, "heading", "Probe heading"):
        break
    time.sleep(.5)
chain, node = [], document
while node is not None and len(chain) < 10:
    node.clear_cache()
    node = node.get_parent()
    if node is not None:
        chain.append(node.get_role_name() + " " + repr(node.get_name()))
print("chain: " + " <- ".join(chain))
if document and "internal frame 'Web page'" in chain and "frame 'Web view probe'" in chain:
    print("EMBEDDED")
"""


def x11(args, output, private):
    env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1", LANG="C.UTF-8",
               XDG_CONFIG_HOME=private + "/config", XDG_CACHE_HOME=private + "/cache", XDG_DATA_HOME=private + "/data")
    for name in ("WAYLAND_DISPLAY", "IBUS_ADDRESS", "GTK_IM_MODULE", "XMODIFIERS"):
        env.pop(name, None)
    owned = []
    try:
        with (output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"],
                                    stdout=subprocess.PIPE, stderr=log, text=True)
        owned.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        ime = None
        if args.ime or args.orca:
            with (output / "dbus.log").open("w") as log:
                bus = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], stdout=subprocess.PIPE,
                                       stderr=log, env=env, text=True, start_new_session=True)
            owned.append(bus)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("D-Bus startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
        if args.ime:
            memconf = next(p for p in ("/usr/libexec/ibus-memconf", "/usr/lib/ibus/ibus-memconf") if os.access(p, os.X_OK))
            env.update(XMODIFIERS="@im=ibus", IBUS_ENABLE_SYNC_MODE="0")
            subprocess.run(["gsettings", "set", "org.freedesktop.ibus.general", "use-system-keyboard-layout", "true"], env=env, check=True)
        with (output / "openbox.log").open("w") as log:
            owned.append(subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log))
        wait_for_wm(env)
        if args.ime:
            with (output / "ibus.log").open("w") as log:
                ime = subprocess.Popen(["ibus-daemon", "--replace", "--cache=none", "--emoji-extension=disable",
                                        "--config=" + memconf, "--xim"], env=env, stdout=log, stderr=log, start_new_session=True)
            owned.append(ime)
            wait_for_ibus_xim(env)
        if args.orca:
            start_orca(env, output, owned)
        with (output / "application.log").open("w") as log:
            app = subprocess.Popen([str(args.binary.resolve())] + (["--transparent"] if args.transparent else []), env=env, stdout=log, stderr=subprocess.STDOUT)
        owned.append(app)
        deadline = time.monotonic() + 20
        while True:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.split()[0]
                break
            if time.monotonic() > deadline or app.poll() is not None:
                raise RuntimeError("the probe's window never opened; see application.log")
            time.sleep(.1)
        subprocess.run(["xdotool", "windowactivate", "--sync", window], env=env, check=True)
        info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
        origin = tuple(int(re.search(rf"Absolute upper-left {axis}:\s+(-?\d+)", info).group(1)) for axis in "XY")
        session = Session(output, env, window, origin)

        def ime_x11(session, checks):
            session.click(120, 80)
            use_ibus_engine(env, "libpinyin", ime)
            session.type("nihao")
            # IBus's candidates show under the page's caret, in its field
            # (window points 60 to 360 across, 60 to 100 down).
            def candidates():
                found = []
                listed = subprocess.run(["xdotool", "search", "--onlyvisible", "--class", "Ibus-ui-gtk3"], env=env, capture_output=True, text=True)
                for candidate in listed.stdout.split():
                    try:
                        info = subprocess.check_output(["xwininfo", "-id", candidate], env=env, text=True)
                    except subprocess.CalledProcessError:
                        continue
                    box = [int(re.search(pattern, info).group(1)) for pattern in (r"Absolute upper-left X:\s+(-?\d+)", r"Absolute upper-left Y:\s+(-?\d+)", r"Width:\s+(\d+)", r"Height:\s+(\d+)")]
                    x, y = box[0] - origin[0], box[1] - origin[1]
                    if box[2] > 100 and box[3] > 20 and 60 <= x <= 360 and 80 <= y <= 140:
                        found.append((x, y))
                return found
            checks["candidates at the caret"] = session.wait(candidates, "IBus's candidates are not under the page's caret")[0]
            return compose(session, checks)
        def read(session, checks):
            return orca_checks(session, env, output, checks)
        check = ime_x11 if args.ime else read if args.orca else None
        return page_checks(session, check, args.transparent), "X11"
    finally:
        end(owned)


def wayland(args, output, private):
    env = dict(os.environ, XDG_RUNTIME_DIR=private, LANG="C.UTF-8", XDG_CONFIG_HOME=private + "/config",
               XDG_CACHE_HOME=private + "/cache", XDG_DATA_HOME=private + "/data")
    for name in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "IBUS_ADDRESS", "XMODIFIERS", "GTK_IM_MODULE", "QT_IM_MODULE"):
        env.pop(name, None)
    owned = []
    try:
        with (output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x960x24", "-ac"],
                                    stdout=subprocess.PIPE, stderr=log, text=True)
        owned.append(xvfb)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        # A private session bus: Fcitx5 talks on it, and nothing else starts.
        bus_config = pathlib.Path(private, "dbus.conf")
        bus_config.write_text("<busconfig><type>session</type><listen>unix:tmpdir=" + private + "</listen><auth>EXTERNAL</auth>"
                              '<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        with (output / "dbus.log").open("w") as log:
            bus = subprocess.Popen(["dbus-daemon", "--config-file=" + str(bus_config), "--nofork", "--print-address=1"],
                                   stdout=subprocess.PIPE, stderr=log, env=env, text=True, start_new_session=True)
        owned.append(bus)
        if not select.select([bus.stdout], [], [], 10)[0]:
            raise RuntimeError("D-Bus startup timed out")
        env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
        with (output / "openbox.log").open("w") as log:
            owned.append(subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log))
        wait_for_wm(env)
        if args.ime:
            profile = pathlib.Path(private, "config/fcitx5/profile")
            profile.parent.mkdir(parents=True)
            profile.write_text("[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=pinyin\n\n[Groups/0/Items/0]\nName=keyboard-us\n"
                               "Layout=\n\n[Groups/0/Items/1]\nName=pinyin\nLayout=\n\n[GroupOrder]\n0=Default\n")
        # The probe's window fills the output: its points are the output's.
        config = pathlib.Path(private, "sway.conf")
        config.write_text("xwayland disable\nseat seat0 fallback true\noutput * mode 900x650\ndefault_border none\n")
        sway_env = dict(env, WLR_BACKENDS="x11", WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman")
        with (output / "sway.log").open("w") as log:
            compositor = subprocess.Popen(["sway", "--unsupported-gpu", "--config", str(config)], env=sway_env, stdout=log, stderr=log)
        owned.append(compositor)
        window = wait_for_sway_host(env, output, compositor)
        deadline = time.monotonic() + 10
        while not [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]:
            if time.monotonic() > deadline:
                raise RuntimeError("Sway's socket never appeared")
            time.sleep(.05)
        socket = [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"][0].name
        native = dict(env, WAYLAND_DISPLAY=socket, WINIT_UNIX_BACKEND="wayland")
        native.pop("DISPLAY")
        if args.ime:
            with (output / "fcitx.log").open("w") as log:
                ime = subprocess.Popen(["fcitx5", "-D", "--disable=xcb"], env=native, stdout=log, stderr=log, start_new_session=True)
            owned.append(ime)
            deadline = time.monotonic() + 10
            while subprocess.run(["fcitx5-remote", "--check"], env=env, capture_output=True).returncode:
                if time.monotonic() > deadline or ime.poll() is not None:
                    raise RuntimeError("Fcitx startup failed")
                time.sleep(.1)
        with (output / "application.log").open("w") as log:
            app = subprocess.Popen([str(args.binary.resolve())] + (["--transparent"] if args.transparent else []), env=native, stdout=log, stderr=subprocess.STDOUT)
        owned.append(app)
        subprocess.run(["xdotool", "windowfocus", "--sync", window], env=env, check=False)
        info = subprocess.check_output(["xwininfo", "-id", str(window)], env=env, text=True)
        origin = tuple(int(re.search(rf"Absolute upper-left {axis}:\s+(-?\d+)", info).group(1)) for axis in "XY")
        session = Session(output, env, window, origin, wayland=native)

        def ime_wayland(session, checks):
            session.click(120, 80)
            deadline = time.monotonic() + 10
            while subprocess.run(["fcitx5-remote", "-s", "pinyin"], env=env, capture_output=True).returncode:
                if time.monotonic() > deadline:
                    raise RuntimeError("Fcitx never loaded Pinyin")
                time.sleep(.1)
            subprocess.run(["fcitx5-remote", "-o"], env=env, check=True)
            time.sleep(.3)
            for character in "nihao":
                session.xdo("type", "--clearmodifiers", "--delay", 90, character)
                time.sleep(.3)
            return compose(session, checks)
        return page_checks(session, ime_wayland if args.ime else None, args.transparent), "Wayland"
    finally:
        end(owned)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--wayland", action="store_true", help="the app on Wayland")
    parser.add_argument("--ime", action="store_true", help="Pinyin composed into the page's field")
    parser.add_argument("--transparent", action="store_true", help="a transparent page over its box's colour")
    parser.add_argument("--orca", action="store_true", help="the page's accessible tree, and Orca reading it (X11)")
    args = parser.parse_args()
    needed = ["Xvfb", "openbox", "xdotool", "xwininfo"]
    needed += ["sway", "dbus-daemon", "wl-copy", "wl-paste"] + (["fcitx5", "fcitx5-remote"] if args.ime else []) if args.wayland else []
    needed += ["xclip"] if not args.wayland and not args.ime and not args.orca else []
    needed += ["orca", "gdbus", "dbus-daemon"] if args.orca else []
    needed += ["ibus-daemon", "ibus", "dbus-daemon", "gsettings"] if args.ime and not args.wayland else []
    for name in needed:
        if shutil.which(name) is None:
            parser.error("Missing executable: " + name)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "result.json").unlink(missing_ok=True)
    with private_runtime("zgx-web-view-") as private:
        checks, backend = (wayland if args.wayland else x11)(args, args.output, private)
    result = {"result": "pass", "backend": backend + (" IME" if args.ime else "") + (" Orca" if args.orca else "") + (" transparent" if args.transparent else ""), "checks": checks}
    (args.output / "result.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print("PASS: " + ", ".join(checks))


if __name__ == "__main__":
    main()
