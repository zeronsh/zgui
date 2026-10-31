#!/usr/bin/env python3
"""A declared window's own state follows the real window: a Luau app under
zgx-run opens a window of its own (`ui.windows`), whose content copies its
`win.active`, `win.shown` and `win.size` into the app's states, and this
activates, resizes and minimizes that X11 window under Openbox, reading the
states over the devtools socket after each. Its setter once stopped as the
window opened (a second close handler dropped the first)."""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time

APP = """
local sides = ui.state("sides", { { id = "side" } })
local active = ui.state("side_active", false)
local shown = ui.state("side_shown", false)
local size = ui.state("side_size", "")
ui.windows(sides, function(_, win)
    ui.effect(function()
        active:set(win.active:get())
    end)
    ui.effect(function()
        shown:set(win.shown:get())
    end)
    ui.effect(function()
        local s = win.size:get()
        size:set(`{math.floor(s.width)}x{math.floor(s.height)}`)
    end)
    return { title = "Side", width = 320, height = 200, ui.text { "side" } }
end)
return ui.text { id = "main", "main" }
"""


def ask(path, request):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(15)
        client.connect(str(path))
        client.sendall((json.dumps(request) + "\n").encode())
        answer = b""
        while not answer.endswith(b"\n"):
            chunk = client.recv(1 << 20)
            if not chunk:
                break
            answer += chunk
    return json.loads(answer)


def state(path, name):
    answer = ask(path, {"cmd": "state", "name": name})
    assert answer["ok"], answer
    return answer["text"].split(" = ", 1)[1]


def until(path, name, want, what, seconds=10):
    deadline = time.monotonic() + seconds
    seen = None
    while time.monotonic() < deadline:
        seen = state(path, name)
        if seen == want:
            return
        time.sleep(0.1)
    raise AssertionError(f"{what}: {name} is {seen}, not {want}")


def window(env, title):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        found = subprocess.run(["xdotool", "search", "--name", f"^{title}$"], env=env,
                               capture_output=True, text=True).stdout.split()
        if found:
            return found[0]
        time.sleep(0.2)
    raise AssertionError(f"no window titled {title}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("zgx_run", type=pathlib.Path, help="zgx-run")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    work = pathlib.Path(tempfile.mkdtemp(dir=args.output)).resolve()
    (work / "app").mkdir()
    (work / "app/app.luau").write_text(APP)
    (work / "app/zgx.json").write_text('{ "title": "Main", "width": 300, "height": 200 }')
    xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    wm = app = None
    try:
        display = ":" + xvfb.stdout.readline().strip()
        env = dict(os.environ, DISPLAY=display, TMPDIR=str(work), ZGX_INSPECT_SOCKET="1",
                   XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        wm = subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.5)
        app = subprocess.Popen([str(args.zgx_run), str(work / "app")], env=env, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL)
        path = work / f"zgx-inspect-{os.getuid()}" / f"{app.pid}.sock"
        deadline = time.monotonic() + 60
        while not path.exists():
            assert time.monotonic() < deadline, "no devtools socket"
            time.sleep(0.1)
        side, main_window = window(env, "Side"), window(env, "Main")
        until(path, "side_size", '"320x200"', "its size as it opens")
        xdo = lambda *a: subprocess.run(["xdotool", *a], env=env, check=True)
        xdo("windowactivate", "--sync", side)
        until(path, "side_active", "true", "activated")
        until(path, "side_shown", "true", "activated")
        xdo("windowactivate", "--sync", main_window)
        until(path, "side_active", "false", "another window activated")
        xdo("windowsize", "--sync", side, "400", "300")
        until(path, "side_size", '"400x300"', "resized")
        xdo("windowminimize", "--sync", side)
        until(path, "side_shown", "false", "minimized")
        xdo("windowactivate", "--sync", side)
        until(path, "side_shown", "true", "restored")
        until(path, "side_active", "true", "restored")
        print("PASS: a declared window's active, shown and size follow it: activated, "
              "another activated, resized, minimized and restored")
    finally:
        if app:
            app.kill()
            app.wait()
        if wm:
            wm.terminate()
        xvfb.terminate()


if __name__ == "__main__":
    main()
