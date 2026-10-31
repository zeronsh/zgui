#!/usr/bin/env python3
"""A covered window draws nothing, unless ZGX_RENDER_WHEN_COVERED=1 asks: a
Luau app whose state changes all the time, covered whole by another X11
window, counts its frames over its devtools socket each way."""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time

APP = """
local ticks = ui.state("ticks", 0)
ui.every(40, function() ticks:update(function(n) return n + 1 end) end)
return ui.text { id = "ticks", "{ticks}" }
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


def frames(path, seconds=1.5):
    """Frames drawn over `seconds`."""
    assert ask(path, {"cmd": "mark", "name": "m"})["ok"]
    time.sleep(seconds)
    answer = ask(path, {"cmd": "frames", "n": 1000, "since": "m"})
    assert answer["ok"], answer
    return answer["text"].count("\nframe ") + answer["text"].startswith("frame ")


def run(zgx_run, app_dir, work, env, cover_it):
    """(frames uncovered, frames covered)."""
    app = subprocess.Popen([str(zgx_run), str(app_dir)], env=env, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
    cover = None
    try:
        path = work / f"zgx-inspect-{os.getuid()}" / f"{app.pid}.sock"
        deadline = time.monotonic() + 60
        while not path.exists():
            assert time.monotonic() < deadline, "no devtools socket"
            time.sleep(0.1)
        time.sleep(1)
        shown = frames(path)
        cover = subprocess.Popen(cover_it, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)
        return shown, frames(path)
    finally:
        if cover:
            cover.kill()
        app.kill()
        app.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("zgx_run", type=pathlib.Path, help="zgx-run")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    work = pathlib.Path(tempfile.mkdtemp(dir=args.output)).resolve()
    (work / "app").mkdir()
    (work / "app/app.luau").write_text(APP)
    (work / "app/zgx.json").write_text('{ "title": "Covered", "width": 300, "height": 200 }')
    xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "800x600x24", "-ac"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    wm = None
    try:
        display = ":" + xvfb.stdout.readline().strip()
        env = dict(os.environ, DISPLAY=display, TMPDIR=str(work), ZGX_INSPECT_SOCKET="1",
                   XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("ZGX_RENDER_WHEN_COVERED", None)
        wm = subprocess.Popen(["openbox"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.5)
        cover = ["xmessage", "-geometry", "800x600+0+0", "covering"]
        shown, covered = run(args.zgx_run, work / "app", work, env, cover)
        print(f"by default: {shown} frames shown, {covered} covered")
        assert shown >= 5, f"it should draw while shown: {shown}"
        assert covered <= 2, f"a covered window does no work: {covered} frames"
        asked = dict(env, ZGX_RENDER_WHEN_COVERED="1")
        shown, covered = run(args.zgx_run, work / "app", work, asked, cover)
        print(f"asked: {shown} frames shown, {covered} covered")
        assert covered >= 5, f"it should go on drawing: {covered} frames"
        print("PASS: covered, it stops, and goes on when asked")
    finally:
        if wm:
            wm.terminate()
        xvfb.terminate()


if __name__ == "__main__":
    main()
