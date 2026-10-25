#!/usr/bin/env python3
"""Open a file of 100,000 lines in a native text area on an owned X11 desktop
with IBus, and edit it as a person would: compose Chinese with libpinyin, type
plain text, scroll with the wheel, go to the end, type and undo there. The
document stays one text shared by the editor and its binding throughout."""
import argparse
import json
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time

from platform_smoke import release_mounts, stop, use_ibus_engine, wait_for_ibus_xim, wait_for_wm

LINES = 100_000
REPORT = re.compile(
    r'DOCUMENT tick=(\d+) lines=(\d+) len=(\d+) anchor=(\d+) focus=(\d+) caret_line=(\d+) '
    r'shared=(true|false) open_ms=([\d.]+) resident_kb=(\d+) first_line=(".*?") last_line=(".*?") '
    r'preedit=(".*?") top=(".*?") bottom=(".*?")$', re.M)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    for executable in ("Xvfb", "openbox", "dbus-daemon", "gsettings", "ibus-daemon", "ibus", "xdotool", "import"):
        if shutil.which(executable) is None:
            parser.error("Missing required executable: " + executable)
    memconf = next((candidate for candidate in (shutil.which("ibus-memconf"), "/usr/libexec/ibus-memconf", "/usr/lib/ibus/ibus-memconf", "/usr/lib/x86_64-linux-gnu/ibus/ibus-memconf") if candidate and os.access(candidate, os.X_OK)), None)
    if memconf is None:
        parser.error("Missing ibus-memconf; install the IBus package with its memory configuration service")
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    lines = [f"fn line_{i}() -> usize {{ {i} }} // a line of the file" for i in range(LINES)]
    document = args.output / "document.rs"
    document.write_text("\n".join(lines))
    xvfb = wm = bus = ime = app = None
    with tempfile.TemporaryDirectory(prefix="zgx-large-document-") as private:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=private,
                       XDG_CONFIG_HOME=private + "/config", XDG_CACHE_HOME=private + "/cache",
                       XDG_DATA_HOME=private + "/data", XMODIFIERS="@im=ibus",
                       GTK_IM_MODULE="ibus", QT_IM_MODULE="ibus", WINIT_X11_SCALE_FACTOR="1", LANG="C.UTF-8",
                       # As native_ime_smoke.py: IBus 1.5.29's synchronous
                       # bridge drops preedit signals.
                       IBUS_ENABLE_SYNC_MODE="0")
            for key in ("WAYLAND_DISPLAY", "IBUS_ADDRESS", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS"):
                env.pop(key, None)
            with (args.output / "dbus.log").open("w") as log:
                bus = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], stdout=subprocess.PIPE, stderr=log, env=env, text=True, start_new_session=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError("D-Bus startup timed out")
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            subprocess.run(["gsettings", "set", "org.freedesktop.ibus.general", "use-system-keyboard-layout", "true"], env=env, check=True)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            with (args.output / "ibus.log").open("w") as log:
                ime = subprocess.Popen(["ibus-daemon", "--xim", "--replace", "--cache=none", "--emoji-extension=disable", "--config=" + memconf, "--verbose"], env=env, stdout=log, stderr=log, start_new_session=True)
            deadline = time.monotonic() + 10
            while True:
                ready = subprocess.run(["ibus", "list-engine"], env=env, capture_output=True, text=True)
                if ready.returncode == 0 and "libpinyin" in ready.stdout and "xkb:us::eng" in ready.stdout:
                    break
                if time.monotonic() > deadline or ime.poll() is not None:
                    raise RuntimeError("IBus failed: " + ready.stdout + ready.stderr)
                time.sleep(.2)
            wait_for_ibus_xim(env)
            log_path = args.output / "large-document.log"
            with log_path.open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), str(document), "--smoke-test"], env=env, stdout=log, stderr=log)

                def xdo(*parts):
                    return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()

                deadline = time.monotonic() + 30
                window = None
                while time.monotonic() < deadline and app.poll() is None:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, text=True, capture_output=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(.05)
                if window is None:
                    raise RuntimeError("large document window did not open: " + log_path.read_text())
                xdo("windowactivate", "--sync", window)

                def latest():
                    rows = REPORT.findall(log_path.read_text())
                    if not rows:
                        return None
                    tick, count, length, anchor, focus, caret, shared, open_ms, resident, first, last, preedit, top, bottom = rows[-1]
                    return dict(tick=int(tick), lines=int(count), len=int(length), anchor=int(anchor), focus=int(focus),
                                caret_line=int(caret), shared=shared == "true", open_ms=float(open_ms),
                                resident_kb=int(resident), first_line=json.loads(first), last_line=json.loads(last),
                                preedit=json.loads(preedit), top=json.loads(top), bottom=json.loads(bottom))

                def until(what, holds, seconds=10):
                    """The first report, after this moment, for which `holds`."""
                    after = (latest() or {}).get("tick", -1)
                    deadline = time.monotonic() + seconds
                    while True:
                        state = latest()
                        if state and state["tick"] > after and holds(state):
                            assert state["shared"], ("the editor and its binding hold two texts", what, state)
                            return state
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise AssertionError(f"{what}: {state!r}\n" + log_path.read_text()[-4000:])
                        time.sleep(.05)

                def key(*keys):
                    xdo("key", "--clearmodifiers", *keys)

                def type_text(text):
                    xdo("type", "--clearmodifiers", "--delay", 60, text)

                def capture(name):
                    subprocess.run(["import", "-window", window, str(args.output / (name + ".png"))], env=env, check=True, timeout=10)

                def plain_engine():
                    """IBus's keyboard layout engine, which commits each key."""
                    deadline = time.monotonic() + 10
                    while subprocess.run(["ibus", "engine"], env=env, capture_output=True, text=True).stdout.strip() != "xkb:us::eng":
                        subprocess.run(["ibus", "engine", "xkb:us::eng"], env=env, capture_output=True)
                        if time.monotonic() > deadline:
                            raise RuntimeError("IBus did not switch to xkb:us::eng")
                        time.sleep(.1)
                    time.sleep(.3)

                reports = {}
                reports["opened"] = opened = until("the file opens at its start", lambda s: s["top"] == lines[0], 20)
                assert opened["lines"] == LINES and opened["len"] == document.stat().st_size, opened
                assert opened["first_line"] == lines[0] and opened["last_line"] == lines[-1], opened
                capture("opened")
                xdo("mousemove", "--sync", "--window", window, 200, 70, "click", 1)
                key("ctrl+Home")
                until("the caret at the start", lambda s: s["focus"] == 0)
                use_ibus_engine(env, "libpinyin", ime)
                type_text("nihao")
                reports["composing"] = composing = until("a composition shows", lambda s: s["preedit"] != "")
                assert composing["first_line"] == lines[0], ("a composition is not the text's", composing)
                capture("composing")
                key("space")
                reports["committed"] = until("the composition commits", lambda s: s["preedit"] == "" and s["first_line"] == "你好" + lines[0])
                plain_engine()
                type_text(" typed ")
                typed = "你好 typed " + lines[0]
                reports["typed"] = until("typing goes in after it", lambda s: s["first_line"] == typed)
                assert reports["typed"]["len"] == opened["len"] + len("你好 typed ".encode()), reports["typed"]
                capture("typed")
                xdo("mousemove", "--sync", "--window", window, 400, 300, "click", "--repeat", 20, "--delay", 20, 5)
                reports["scrolled"] = scrolled = until("the wheel scrolls it", lambda s: s["top"] not in ("", typed) and s["top"].startswith("fn line_"))
                assert scrolled["caret_line"] == 0, ("scrolling moves the view, not the caret", scrolled)
                capture("scrolled")
                key("ctrl+End")
                reports["end"] = until("the end shows", lambda s: s["caret_line"] == LINES - 1 and s["bottom"] == lines[-1])
                capture("end")
                type_text(" end")
                reports["typed_at_end"] = until("typing at the end", lambda s: s["last_line"] == lines[-1] + " end" and s["bottom"] == lines[-1] + " end")
                key("ctrl+z")
                reports["undone"] = until("undo takes it back", lambda s: s["last_line"] == lines[-1])
                assert reports["undone"]["first_line"] == typed, reports["undone"]
                capture("undone")
                # Closed as a user closes it (Openbox's Alt+F4), not by the
                # app's own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=25)
            assert app.returncode == 0, log_path.read_text()[-4000:]
            result = dict(passed=True, backend="owned X11/Xvfb/Openbox, IBus libpinyin over XIM", lines=LINES,
                          bytes=document.stat().st_size, open_ms=opened["open_ms"], resident_kb=opened["resident_kb"],
                          checks=["opens 100,000 lines at the start", "composition stays out of the text until committed",
                                  "commit and typing at the start", "wheel scrolls the view, not the caret",
                                  "the end shows and takes typing", "undo", "editor and binding share one text"],
                          reports=reports)
            (args.output / "results.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
            print(f"PASS: 100,000 lines open in {opened['open_ms']:.0f} ms; IME, typing, scrolling and undo")
        finally:
            stop(app)
            for owned in (ime, bus):
                if owned:
                    try:
                        os.killpg(owned.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    stop(owned)
                    # Reap any service descendant that outlives its supervisor.
                    try:
                        os.killpg(owned.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            stop(wm)
            stop(xvfb)
            release_mounts(private)


if __name__ == "__main__":
    main()
