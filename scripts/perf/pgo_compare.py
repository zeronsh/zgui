#!/usr/bin/env python3
"""Profile-guided builds of every framework's comparison apps, each trained
on its own runs, for the macOS comparison's PGO supplement
(`run_macos_comparison.py --pgo`).

    python3 scripts/perf/pgo_compare.py generate   # instrumented builds
    python3 scripts/perf/pgo_compare.py train      # each app's own scene, windowed
    python3 scripts/perf/pgo_compare.py use        # optimized builds, to target/pgo-compare/

zgx's heavy, lab and animate workloads, and GPUI's and GPUI Fast's: built
with the `dist` profile instrumented, each app run in a window on its own
scene (`both`; `animate`) for 15 s, the profiles of a framework's apps
merged, and the apps built again with that. Training opens windows: run it
on a quiet desktop. Needs `rustup component add llvm-tools`.
"""
import argparse, glob, os, pathlib, shutil, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(os.environ.get('ZGX_PGO_DIR', '/tmp/zgx-pgo-compare'))
OUT = ROOT / 'target' / 'pgo-compare'
# framework: (cargo checkout, cargo arguments, {binary: training mode}).
APPS = {
    'zgx': (ROOT, ['-p', 'zgx-desktop', '--example', 'heavy_workload', '--example', 'component_workload',
                    '--example', 'animation_workload'],
             {'examples/heavy_workload': 'both', 'examples/component_workload': 'both',
              'examples/animation_workload': 'animate'}),
    'gpui': (ROOT / 'comparisons/gpui', [], {'heavy': 'both', 'zgx-compare-gpui': 'both', 'animate': 'animate'}),
    'gpui-fast': (ROOT / 'comparisons/gpui-fast', [],
                  {'heavy': 'both', 'zgx-compare-gpui-fast': 'both', 'animate': 'animate'}),
}


def profdata():
    sysroot = subprocess.run(['rustc', '--print', 'sysroot'], capture_output=True, text=True).stdout.strip()
    found = glob.glob(f'{sysroot}/lib/rustlib/*/bin/llvm-profdata')
    if not found:
        sys.exit('llvm-profdata not found: rustup component add llvm-tools')
    return found[0]


def cargo(framework, stage, flags):
    checkout, args, _ = APPS[framework]
    env = dict(os.environ, CARGO_TARGET_DIR=str(WORK / framework / stage), CARGO_INCREMENTAL='0', RUSTFLAGS=flags)
    subprocess.run(['cargo', 'build', '--profile', 'dist', '--locked', *args], cwd=checkout, env=env, check=True)


def generate():
    for framework in APPS:
        shutil.rmtree(WORK / framework / 'raw', ignore_errors=True)
        cargo(framework, 'generate', f'-Cprofile-generate={WORK / framework / "raw"}')


def train():
    hide = 'tell application "System Events" to if exists process "Zeron" then set visible of process "Zeron" to {}'
    subprocess.run(['osascript', '-e', hide.format('false')], capture_output=True)
    try:
        for framework, (_, _, binaries) in APPS.items():
            for binary, mode in binaries.items():
                print(f'training {framework} {binary} ({mode})', file=sys.stderr)
                env = {'HOME': os.environ['HOME'], 'PATH': os.environ['PATH'], 'ZGX_MODE': mode, 'ZGX_SECONDS': '15'}
                app = subprocess.Popen([str(WORK / framework / 'generate' / 'dist' / binary)], env=env,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    app.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    # macOS apps may outlive their last window: their profile is written at exit.
                    app.terminate()
                    app.wait()
    finally:
        subprocess.run(['osascript', '-e', hide.format('true')], capture_output=True)


def use():
    for framework, (_, _, binaries) in APPS.items():
        raw = list((WORK / framework / 'raw').glob('*.profraw'))
        if not raw:
            sys.exit(f'{framework}: no profiles; run `train` first')
        merged = WORK / framework / 'merged.profdata'
        subprocess.run([profdata(), 'merge', '-o', str(merged), *map(str, raw)], check=True)
        cargo(framework, 'use', f'-Cprofile-use={merged}')
        (OUT / framework).mkdir(parents=True, exist_ok=True)
        for binary in binaries:
            shutil.copy2(WORK / framework / 'use' / 'dist' / binary, OUT / framework / pathlib.Path(binary).name)
            print(OUT / framework / pathlib.Path(binary).name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('stage', choices=['generate', 'train', 'use'])
    {'generate': generate, 'train': train, 'use': use}[parser.parse_args().stage]()
