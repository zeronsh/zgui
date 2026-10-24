#!/usr/bin/env python3
"""Native X11 paint/media regression. Run inside a private Xvfb desktop."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image, ImageChops

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binaries', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
results = []


def capture(window, path):
    subprocess.run(['import', '-window', window, str(path)], check=True)
    return Image.open(path).convert('RGB')


def click(window, x, y):
    subprocess.run(['xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1',
                    'mousemove', '990', '690'], check=True)


cases = [('paint_styles', 'zgx detailed paint styles', (120, 195)),
         ('canvas', 'zgx retained canvas', (85, 95)),
         ('svg_transform', 'zgx SVG transformations', (85, 112)),
         ('animated_image', 'zgx animated image', (85, 105))]
for name, title, position in cases:
    binary = args.binaries / name
    env = dict(os.environ, WINIT_UNIX_BACKEND='x11', LP_NUM_THREADS='4')
    env.pop('WAYLAND_DISPLAY', None)
    with (args.output / (name + '.log')).open('w') as log:
        process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            window = None
            for _ in range(160):
                found = subprocess.run(['xdotool', 'search', '--onlyvisible', '--pid', str(process.pid), '--name', '^' + title + '$'],
                                       text=True, capture_output=True)
                if found.returncode == 0:
                    window = found.stdout.splitlines()[-1]
                    try:
                        initial = capture(window, args.output / (name + '-initial.png'))
                    except subprocess.CalledProcessError:
                        time.sleep(.05)
                        continue
                    colors = initial.getcolors(1000000)
                    if colors and len(colors) > 20:
                        break
                if process.poll() is not None:
                    raise RuntimeError(name + ' exited before first frame')
                time.sleep(.05)
            else:
                raise RuntimeError(name + ' first frame timeout')
            # Move the pointer away to prevent hover from counting as content change.
            subprocess.run(['xdotool', 'mousemove', '990', '690'], check=True)
            time.sleep(.15)
            initial = capture(window, args.output / (name + '-initial.png'))
            if name == 'animated_image':
                # Detect changing pixels below the pause button, then require complete
                # stability across several frame deadlines while paused.
                region = (32, 150, 272, 310)
                changed = False
                for _ in range(12):
                    time.sleep(.1)
                    live = capture(window, args.output / (name + '-live.png'))
                    if ImageChops.difference(initial.crop(region), live.crop(region)).getbbox():
                        changed = True
                        break
                assert changed, 'animation never advanced'
                click(window, *position)
                time.sleep(.2)
                paused = capture(window, args.output / (name + '-paused.png'))
                time.sleep(.85)
                still = capture(window, args.output / (name + '-still.png'))
                assert ImageChops.difference(paused.crop(region), still.crop(region)).getbbox() is None, 'paused animation advanced'
                click(window, *position)
                resumed = False
                for _ in range(12):
                    time.sleep(.1)
                    live = capture(window, args.output / (name + '-resumed.png'))
                    if ImageChops.difference(still.crop(region), live.crop(region)).getbbox():
                        resumed = True
                        break
                assert resumed, 'resumed animation did not advance'
                changed_bounds = region
            else:
                click(window, *position)
                time.sleep(.3)
                changed = capture(window, args.output / (name + '-changed.png'))
                # Exclude buttons: require the card/path/SVG drawing itself to change.
                region = {'paint_styles': (220, 80, 350, 240),
                          'canvas': (24, 133, 736, 383),
                          'svg_transform': (50, 180, 350, 400)}[name]
                changed_bounds = ImageChops.difference(initial.crop(region), changed.crop(region)).getbbox()
                assert changed_bounds, name + ' content did not change after click'
            assert process.poll() is None
            results.append(dict(example=name, passed=True, changed_bounds=changed_bounds,
                                binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()))
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
(args.output / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps(results))
