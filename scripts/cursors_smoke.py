#!/usr/bin/env python3
"""Check native X11 cursor images; run within a private Xvfb desktop."""
import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('binary', type=Path)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=False)

class CursorImage(C.Structure):
    _fields_ = [('x', C.c_short), ('y', C.c_short), ('width', C.c_ushort),
                ('height', C.c_ushort), ('xhot', C.c_ushort), ('yhot', C.c_ushort),
                ('serial', C.c_ulong), ('pixels', C.POINTER(C.c_ulong)),
                ('atom', C.c_ulong), ('name', C.c_char_p)]

x11 = C.CDLL('libX11.so.6')
x11.XOpenDisplay.argtypes = [C.c_char_p]
x11.XOpenDisplay.restype = C.c_void_p
x11.XCloseDisplay.argtypes = [C.c_void_p]
x11.XFree.argtypes = [C.c_void_p]
fixes = C.CDLL('libXfixes.so.3')
fixes.XFixesGetCursorImage.argtypes = [C.c_void_p]
fixes.XFixesGetCursorImage.restype = C.POINTER(CursorImage)
display = x11.XOpenDisplay(None)
assert display

def cursor_at(window, x, y):
    subprocess.run(['xdotool', 'mousemove', '--window', window, str(x), str(y)], check=True)
    time.sleep(.2)
    pointer = fixes.XFixesGetCursorImage(display)
    assert pointer
    try:
        image = pointer.contents
        pixels = bytes().join((int(image.pixels[i]) & 0xffffffff).to_bytes(4, 'little') for i in range(image.width * image.height))
        return dict(width=image.width, height=image.height, name=image.name.decode() if image.name else None,
                    sha256=hashlib.sha256(pixels).hexdigest())
    finally:
        x11.XFree(pointer)

env = dict(os.environ, WINIT_UNIX_BACKEND='x11', LP_NUM_THREADS='4')
env.pop('WAYLAND_DISPLAY', None)
with (a.output / 'application.log').open('w') as log:
    app = subprocess.Popen([str(a.binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
try:
    for _ in range(150):
        found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', '^zgx cursor styles$'], capture_output=True, text=True)
        if found.returncode == 0:
            window = found.stdout.splitlines()[-1]
            break
        assert app.poll() is None
        time.sleep(.05)
    else:
        raise RuntimeError('cursor window timeout')
    time.sleep(.4)
    shapes = {name: cursor_at(window, *point) for name, point in [
        ('default', (590, 310)), ('move', (90, 125)),
        ('disabled', (265, 125)), ('crosshair', (450, 125))]}
    assert shapes['crosshair']['sha256'] != shapes['disabled']['sha256'], shapes
    assert shapes['move']['name'] == 'move', shapes
    assert shapes['disabled']['name'] == 'not-allowed', shapes
    assert shapes['crosshair']['name'] == 'crosshair', shapes
    subprocess.run(['xdotool', 'mousemove', '--window', window, '80', '220', 'click', '1'], check=True)
    shapes['wait'] = cursor_at(window, 450, 125)
    assert shapes['wait']['sha256'] != shapes['crosshair']['sha256'], shapes
    shapes['restored'] = cursor_at(window, 590, 310)
    (a.output / 'cursor-observations.json').write_text(json.dumps(shapes, indent=2))
    # winit's initial Default inherits the X server cursor until a named
    # cursor is selected. Restoration must select the named platform default.
    assert shapes['restored']['name'] == 'default', shapes
    cursor_at(window, 265, 125)
    again = cursor_at(window, 590, 310)
    assert again['sha256'] == shapes['restored']['sha256'], shapes
    subprocess.run(['import', '-window', window, str(a.output / 'window.png')], check=True)
    (a.output / 'result.json').write_text(json.dumps(dict(passed=True, cursors=shapes,
        binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest()), indent=2) + '\n')
    print('Native default, move, disabled, crosshair, reactive wait and restoration PASS')
finally:
    x11.XCloseDisplay(display)
    if app.poll() is None:
        app.terminate()
        try:
            app.wait(timeout=5)
        except subprocess.TimeoutExpired:
            app.kill()
            app.wait()
