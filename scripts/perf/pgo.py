#!/usr/bin/env python3
"""Build a binary with profile-guided optimization, trained on zgx's headless scenes.

    python3 scripts/perf/pgo.py -p zgx-run --bin zgx-run
    python3 scripts/perf/pgo.py -p my-app --bin my-app -- '{bin} --some-run'
    python3 scripts/perf/pgo.py -p zgx-desktop --example heavy_workload

Three builds with the `dist` profile, in a target directory of their own:
the binary and zgx's headless scenes instrumented; the scenes run (heavy
dashboard, lab stream, animate, script mount, anime, text shaping, list
state: the engine's hot paths), and so does each training command given
(`{bin}` is the instrumented binary: run what the app does, so its own code
is profiled too); then the binary built again with what they recorded. The
binary lands in target/dist-pgo/. Trained on these seven scenes, every one
of zgx's headless scenes runs 2-19% fewer instructions (2026-10-24: heavy
-10%, anime -19%, style -18%, script layout -14%, text -10%, list state
-6%). Without the text and list-state scenes, those two ran 5% and 2% more
instructions than without PGO: what a profile never ran is laid out as
cold. Profiles fit the source they were recorded on: build again after
changing it. Needs `rustup component add llvm-tools`.
"""
import argparse
import glob
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
# The training scenes: (package, example, environment).
TRAINING = [
    ('zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'both', 'TICKS': '600'}),
    ('zgx-desktop', 'workload_headless', {'ZGX_MODE': 'stream'}),
    ('zgx-desktop', 'animation_headless', {'FRAMES': '1200'}),
    ('zgx-script', 'mount_bench', {}),
    ('zgx-script', 'anime_bench', {}),
    ('zgx-gpu', 'text_bench', {}),
    ('zgx-script', 'list_state_bench', {}),
]


def profdata():
    sysroot = subprocess.run(['rustc', '--print', 'sysroot'], capture_output=True, text=True).stdout.strip()
    found = glob.glob(f'{sysroot}/lib/rustlib/*/bin/llvm-profdata')
    if not found:
        sys.exit('llvm-profdata not found: rustup component add llvm-tools')
    return found[0]


def cargo(target_dir, flags, args):
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir), CARGO_INCREMENTAL='0',
               RUSTFLAGS=' '.join(filter(None, [os.environ.get('RUSTFLAGS'), flags])))
    subprocess.run(['cargo', 'build', '--profile', 'dist', '--locked', *args], cwd=ROOT, env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('-p', '--package', required=True)
    kind = parser.add_mutually_exclusive_group(required=True)
    kind.add_argument('--bin')
    kind.add_argument('--example')
    parser.add_argument('train', nargs='*', help='training commands; {bin} is the instrumented binary')
    args = parser.parse_args()
    work = ROOT / 'target' / 'pgo'
    raw = work / 'raw'
    shutil.rmtree(raw, ignore_errors=True)
    what = ['--bin', args.bin] if args.bin else ['--example', args.example]
    name = args.bin or args.example
    scenes = []
    for package, example, _ in TRAINING:
        scenes += ['-p', package, '--example', example]
    # 1. Instrumented: the binary and the scenes.
    generate = f'-Cprofile-generate={raw}'
    cargo(work / 'generate', generate, ['-p', args.package, *what])
    cargo(work / 'generate', generate, scenes)
    built = work / 'generate' / 'dist' / ('examples' if args.example else '')
    # 2. Training.
    for package, example, env in TRAINING:
        print(f'training: {example}', file=sys.stderr)
        subprocess.run([str(built / example)], env=dict(os.environ, **env), check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for command in args.train:
        print(f'training: {command}', file=sys.stderr)
        subprocess.run(command.replace('{bin}', str(built / name)), shell=True, check=True)
    merged = work / 'merged.profdata'
    subprocess.run([profdata(), 'merge', '-o', str(merged), *map(str, raw.glob('*.profraw'))], check=True)
    # 3. Optimized with the profile.
    cargo(work / 'use', f'-Cprofile-use={merged}', ['-p', args.package, *what])
    out = ROOT / 'target' / 'dist-pgo'
    out.mkdir(parents=True, exist_ok=True)
    source = work / 'use' / 'dist' / ('examples' if args.example else '') / name
    shutil.copy2(source, out / name)
    print(out / name)


if __name__ == '__main__':
    main()
