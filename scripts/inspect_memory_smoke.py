#!/usr/bin/env python3
"""`zgx inspect memory` from a real window: a Luau app on a private X display
answers, over its socket, with the lines only its host knows (the process's,
the renderer's) beside the script's, and with what it shows counted."""
import argparse
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time

APP = """
return ui.column {
    ui.text { id = "hello", "Measured, not guessed" },
}
"""


def ask(path, request):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(10)
        client.connect(str(path))
        client.sendall((json.dumps(request) + "\n").encode())
        answer = b""
        while not answer.endswith(b"\n"):
            chunk = client.recv(65536)
            if not chunk:
                break
            answer += chunk
    return json.loads(answer)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="zgx-run")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    work = pathlib.Path(tempfile.mkdtemp(dir=args.output))
    app_dir = work / "app"
    app_dir.mkdir()
    (app_dir / "app.luau").write_text(APP)
    xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"],
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    app = None
    try:
        display = ":" + xvfb.stdout.readline().strip()
        env = dict(os.environ, DISPLAY=display, TMPDIR=str(work), ZGX_INSPECT_SOCKET="1")
        env.pop("WAYLAND_DISPLAY", None)
        app = subprocess.Popen([str(args.probe), str(app_dir)], env=env, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL)
        path = work / f"zgx-inspect-{os.getuid()}" / f"{app.pid}.sock"
        deadline = time.monotonic() + 60
        lines = None
        while time.monotonic() < deadline:
            time.sleep(0.25)
            if not path.exists():
                continue
            answer = ask(path, {"cmd": "memory"})
            assert answer["ok"], answer
            lines = answer["data"]
            # The text drawn: its glyphs reach the atlas with the first frame.
            atlas = [l for l in lines if l["area"] == "gpu" and l["name"] == "glyph atlas"]
            if atlas and atlas[0]["n"] > 0:
                break
        else:
            raise AssertionError(f"no glyphs in the atlas: {lines}")

        def line(area, name):
            found = [l for l in lines if l["area"] == area and l["name"].startswith(name)]
            assert found, f"no {area} {name}: {lines}"
            return found[0]

        assert line("process", "resident")["bytes"] > 1 << 20, lines
        assert line("gpu", "glyph atlas")["bytes"] > 0, lines
        assert line("text", "shaped nodes")["n"] >= 1, lines
        assert line("luau", "app")["bytes"] > 0, lines
        assert line("scene", "nodes mounted")["n"] >= 2, lines
        assert [l for l in lines if l["area"] == "heap"], lines
        print("PASS: the window's host answers with process, heap, text, GPU and script lines")
    finally:
        if app:
            app.kill()
        xvfb.terminate()


if __name__ == "__main__":
    main()
