#!/usr/bin/env python3
"""The whole macOS comparison in one run, under an hour: heavy, lab, animate
and vlist, zgx against GPUI Fast and MyGo, with the suite's protocol
(scripts/run_macos_comparison.py --suite).

    python3 scripts/run_macos_suite.py              # build, then measure
    python3 scripts/run_macos_suite.py --skip-build

Builds every scene's apps once, waits for a quiet machine once (the
preflight also waits out any system dialog on screen), hides Zeron, and runs
the scenes one after another into docs/results/suite-macos-<date>-<commit>/
<scene>/. Each scene is accepted or rejected on its own (its audit.json);
the suite's manifest.json lists them. Apps run from copies staged in /tmp
(scripts/staging.py).
"""
import argparse, datetime, json, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
sys.path.insert(0, str(ROOT / 'scripts/perf'))
import preflight  # noqa: E402
import run_macos_comparison as comparison  # noqa: E402

SCENES = ['heavy', 'lab', 'animate', 'vlist']


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--scenes', default=','.join(SCENES), help='comma-separated, from ' + ', '.join(SCENES))
    parser.add_argument('--hide', default='Zeron')
    args = parser.parse_args()
    scenes = [s for s in args.scenes.split(',') if s in SCENES]
    if not args.skip_build:
        for scene in scenes:
            comparison.build(scene, [f for f in comparison.SUITE_FRAMEWORKS if f in comparison.SCENES[scene]['binaries']])
    else:
        comparison.tools()
    missing = [f'{scene}/{f}: {path}' for scene in scenes for f, path in comparison.SCENES[scene]['binaries'].items()
               if f in comparison.SUITE_FRAMEWORKS and not path.exists()]
    assert not missing, 'not built:\n' + '\n'.join(missing)
    commit = comparison.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT)
    started = datetime.datetime.now(datetime.timezone.utc)
    out = ROOT / 'docs/results' / f'suite-macos-{started:%Y-%m-%d}-{commit[:7]}'
    out.mkdir(parents=True, exist_ok=True)
    # Package power where sudo runs powermetrics without asking.
    power = comparison.powermetrics_allowed()
    print(f"powermetrics: {'on' if power else 'off (sudo would ask)'}", flush=True)
    found = preflight.wait_until_quiet(log=lambda line: print(line, flush=True))
    hidden = comparison.hide(args.hide) if args.hide else False
    results = {}
    try:
        for scene in scenes:
            began = time.monotonic()
            print(f'== {scene}', flush=True)
            done = subprocess.run([sys.executable, '-u', str(ROOT / 'scripts/run_macos_comparison.py'), '--suite',
                                   '--scene', scene, '--skip-build', '--no-preflight', '--hide', '',
                                   *(['--powermetrics'] if power else []),
                                   '--out', str(out / scene)], cwd=ROOT)
            audit = out / scene / 'audit.json'
            accepted = done.returncode == 0 and audit.exists() and json.loads(audit.read_text())['accepted']
            results[scene] = {'accepted': accepted, 'minutes': round((time.monotonic() - began) / 60, 1)}
            print(f"== {scene}: {'ACCEPTED' if accepted else 'REJECTED'} in {results[scene]['minutes']} min", flush=True)
    finally:
        if hidden:
            comparison.show(args.hide)
    manifest = {'commit': commit, 'started_utc': started.isoformat(),
                'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'preflight': found, 'scenes': results, 'frameworks': comparison.SUITE_FRAMEWORKS,
                'powermetrics': power,
                'protocol': {**comparison.SUITE, 'spread_for_extra_trials': comparison.SPREAD,
                             'extra_trials': comparison.EXTRA}}
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=1) + '\n')
    print(('ACCEPTED' if all(r['accepted'] for r in results.values()) else 'REJECTED'), out)


if __name__ == '__main__':
    main()
