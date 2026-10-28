#!/usr/bin/env python3
"""Copying display text on Linux, read by another client: selectable_text's
document selected whole (a press on its heading, Ctrl+A) and copied (Ctrl+C)
puts plain text and HTML on the CLIPBOARD, read back with xclip (X11) or
wl-paste through Sway (`--wayland`). Both hold every line of the transcript's
virtual list of 10,000, the rows it never mounted included, in order, and
leave out the part marked unselectable; the HTML is a fragment of the
document's elements, the plain text a line per line."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time

from platform_smoke import stop, wait_for_wm
from sway_host import wait_for_sway_host

TITLE = "zgx selectable text"
LINE = "A transcript line in a virtual list of 10,000 (line {})"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the selectable_text example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--wayland", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, result = [], {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    with tempfile.TemporaryDirectory(prefix="zgx-selection-copy-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private + "/config",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS"):
            env.pop(key, None)
        try:
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
                          env, stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)

            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()

            if args.wayland:
                env.update(WLR_BACKENDS="x11", WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman")
                config = pathlib.Path(private, "sway.conf")
                config.write_text("xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\n"
                                  f"default_border none\nfor_window [title=\"{TITLE}\"] floating enable\n")
                sway = launch("sway", ["sway", "--unsupported-gpu", "--config", str(config)], env)
                host = wait_for_sway_host(env, args.output, sway)
                deadline = time.monotonic() + 10
                while True:
                    sockets = list(pathlib.Path(private).glob("sway-ipc*.sock"))
                    displays = [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
                    if sockets and displays:
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError("Sway startup timed out")
                    time.sleep(.05)
                env.update(SWAYSOCK=str(sockets[0]), WAYLAND_DISPLAY=displays[0].name)
                client = dict(env, WINIT_UNIX_BACKEND="wayland")
                client.pop("DISPLAY")
            else:
                client = dict(env, WINIT_X11_SCALE_FACTOR="1")
            app = launch("application", [str(args.binary.resolve())], client)

            # The window: an X11 window, or the client's box on Sway's
            # output, inside Sway's own X11 window.
            deadline = time.monotonic() + 20
            while True:
                if args.wayland:
                    tree = json.loads(subprocess.check_output(["swaymsg", "-t", "get_tree", "-r"], env=env, text=True))
                    pending, rect = [tree], None
                    while pending:
                        node = pending.pop()
                        pending.extend(node.get("nodes", []) + node.get("floating_nodes", []))
                        if node.get("app_id") == "selectable_text" or node.get("name") == TITLE:
                            rect = node["rect"]
                    if rect:
                        window, origin = host, (rect["x"], rect["y"])
                        break
                else:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window, origin = found.stdout.split()[0], (0, 0)
                        break
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the example's window did not appear")
                time.sleep(.05)
            xdo("windowactivate", "--sync", window)
            time.sleep(.6)

            # A press on the heading puts the document's selection there;
            # Ctrl+A takes it all, Ctrl+C copies it.
            xdo("mousemove", "--window", window, origin[0] + 60, origin[1] + 44, "click", 1)
            time.sleep(.2)
            xdo("key", "--window", window, "ctrl+a")
            time.sleep(.3)
            xdo("key", "--window", window, "ctrl+c")

            def read(kind):
                command = (["wl-paste", "--no-newline", "--type", kind] if args.wayland
                           else ["xclip", "-o", "-selection", "clipboard", "-t", kind])
                done = subprocess.run(command, env=env, capture_output=True, timeout=10)
                return done.stdout.decode("utf-8", "replace") if done.returncode == 0 else None

            def offered():
                command = (["wl-paste", "--list-types"] if args.wayland
                           else ["xclip", "-o", "-selection", "clipboard", "-t", "TARGETS"])
                done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
                return done.stdout.split() if done.returncode == 0 else []

            deadline = time.monotonic() + 10
            while "text/html" not in (types := offered()):
                if time.monotonic() > deadline or app.poll() is not None:
                    break
                time.sleep(.1)
            plain_type = "text/plain;charset=utf-8" if args.wayland else "UTF8_STRING"
            text, html = read(plain_type) or "", read("text/html") or ""
            (args.output / "copied.txt").write_text(text)
            (args.output / "copied.html").write_text(html)
            result["types"] = types
            lines = text.split("\n")
            numbered = [int(n) for n in re.findall(r"\(line (\d+)\)", text)]
            html_numbered = [int(n) for n in re.findall(r"\(line (\d+)\)", html)]
            result["plain text lines"] = len(lines)
            result["plain text starts"] = lines[:3]
            result["plain text: the heading first"] = lines[0] == "Selectable text"
            result["plain text: every transcript line, in order"] = numbered == list(range(1, 10_001))
            result["plain text: the last line whole"] = text.rstrip("\n").endswith("10000\t" + LINE.format(10000)) or \
                text.rstrip("\n").endswith(LINE.format(10000))
            result["plain text: no unselectable text"] = "marked unselectable" not in text
            result["html bytes"] = len(html)
            result["html starts"] = html[:160]
            result["html: every transcript line, in order"] = html_numbered == list(range(1, 10_001))
            result["html: the heading and the paragraph's spans"] = (
                "Selectable text" in html and "<b>one selection</b>" in html.replace("<strong>", "<b>").replace("</strong>", "</b>"))
            result["html: no unselectable text"] = "marked unselectable" not in html
            result["html and plain text offered"] = "text/html" in types and any(
                t in types for t in ("UTF8_STRING", "text/plain;charset=utf-8", "text/plain"))
        finally:
            for process in reversed(processes):
                stop(process)
    (args.output / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    failed = [name for name, value in result.items() if value is False]
    if failed:
        raise SystemExit(f"FAIL: {', '.join(failed)}")
    print(f"PASS: a whole document's copy reaches another client ({'Wayland' if args.wayland else 'X11'}) as "
          "plain text and HTML, every row of its virtual list included")


if __name__ == "__main__":
    main()
