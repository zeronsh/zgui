#!/usr/bin/env python3
"""Declared loops played by Core Animation on macOS (zgx-gpu's loops.rs), in
a real window of zgx-run.

    python3 scripts/macos_baked_loops_probe.py [--target-dir DIR]

A Luau app shows a shader ring and a pulsing dot, both `anime.animate` loops
at 30 fps. The app must present no frame while they play (`ZGX_PRESENT_LOG`),
spend almost no CPU, and still animate: captures over two seconds differ, at
the loops alone. Its footprint then falls past the driver's hold (the Metal
driver keeps about 176 MB for a command queue some 8 s after it last ran, M5). `ZGX_PROBE_DIFFERENT=1` is
for a machine without that hold (the M1): it skips the footprint check.

Needs Screen Recording permission for the shell running it, which it only
checks. Launched from /tmp with a clean environment. If a system dialog
appears, stop and report it; never click it. macOS only.
"""
import argparse
import json
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
APP = """
local RING = ui.shader([[
fn fill(input:FillInput)->vec4<f32> {
 let d=input.p-input.size*.5;
 let a=fract(atan2(d.y,d.x)/6.2831853+input.time/2.);
 let ring=clamp(.5-(abs(length(d)-min(input.size.x,input.size.y)*.4)-1.)*input.scale,0.,1.);
 return vec4(1.,1.,1.,ring*a);
}
]])
return ui.column {
    p = 40, gap = 20, bg = "#202020", w_full = true, h_full = true,
    ui.text { color = "#ffffff", "Loops" },
    ui.div {
        id = "ring", w = 32, h = 32, shrink = 0, bg_shader = RING,
        on_mount = function(el)
            anime.animate(el, { shaderTime = { 0, 2 }, duration = 2000, ease = "linear",
                loop = true, frameRate = 30 })
        end,
    },
    ui.div {
        id = "dot", w = 12, h = 12, shrink = 0, rounded = 6, bg = "#30d060",
        on_mount = function(el)
            anime.animate(el, { opacity = { 1, 0.5 }, duration = 200, ease = "steps(6)",
                loop = true, alternate = true, loopDelay = 300, frameRate = 30 })
        end,
    },
}
"""


def main():
    if platform.system() != 'Darwin':
        print('macOS only')
        return
    parser = argparse.ArgumentParser()
    parser.add_argument('--target-dir', default=str(ROOT / 'target'))
    args = parser.parse_args()
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'zgx-run'], cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory() as stage:
        stage = pathlib.Path(stage)
        binary = stage / 'zgx-run'
        shutil.copy2(pathlib.Path(args.target_dir) / 'release/zgx-run', binary)
        pixels = stage / 'window_pixels'
        subprocess.run(['swiftc', '-O', str(ROOT / 'scripts/window_pixels.swift'), '-o', str(pixels)], check=True)
        app_dir = stage / 'app'
        app_dir.mkdir()
        (app_dir / 'app.luau').write_text(APP)
        clean = {'HOME': os.environ['HOME'], 'PATH': '/usr/bin:/bin', 'ZGX_PRESENT_LOG': '1'}
        if 'TMPDIR' in os.environ:
            clean['TMPDIR'] = os.environ['TMPDIR']
        log_path = stage / 'log'
        with open(log_path, 'w') as log:
            app = subprocess.Popen([str(binary), str(app_dir)], cwd='/tmp', env=clean, stdout=log, stderr=log)
        try:
            time.sleep(5)
            presents = lambda: log_path.read_text(errors='replace').count('plain-set')
            cpu = lambda: subprocess.run(['ps', '-o', 'time=', '-p', str(app.pid)], capture_output=True, text=True).stdout.strip()
            before, cpu_before = presents(), cpu()
            shots = []
            for at in range(5):
                shot = stage / f'shot{at}.png'
                done = subprocess.run([str(pixels), 'capture', str(app.pid), str(shot)], capture_output=True, text=True)
                if done.returncode != 0:
                    sys.exit(f'capture failed: {done.stdout} {done.stderr}')
                shots.append(shot)
                time.sleep(0.37)
            time.sleep(3)
            drawn = presents() - before
            changed = 0
            for a, b in zip(shots, shots[1:]):
                diff = json.loads(subprocess.run([str(pixels), 'diff', str(a), str(b)], capture_output=True, text=True).stdout)
                changed += diff.get('differing', 0) > 0
            print(f'presents while playing {drawn}; captures that changed {changed}/4; CPU time {cpu_before} -> {cpu()}')
            if drawn > 0:
                sys.exit('FAIL: the app presented frames while its loops played')
            if changed < 2:
                sys.exit('FAIL: the loops do not animate')
            if 'loop-bake' not in log_path.read_text(errors='replace'):
                sys.exit('FAIL: no loop was baked')
            for line in log_path.read_text(errors='replace').splitlines():
                if 'loop-bake' in line:
                    print(line[:100])
            time.sleep(14)
            footprint = float(re.search(r'Footprint:\s*([\d.]+)\s*MB', subprocess.run(
                ['footprint', '-p', str(app.pid)], capture_output=True, text=True).stdout).group(1))
            print(f'footprint {footprint} MB')
            if footprint > 120 and not os.environ.get('ZGX_PROBE_DIFFERENT'):
                sys.exit('FAIL: the driver still holds its memory')
        finally:
            app.terminate()
            app.wait()
    print('PASS')


main()
