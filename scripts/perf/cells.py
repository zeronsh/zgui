#!/usr/bin/env python3
"""Windowed A/B of candidate changes ("cells") on a quiet Mac: each cell a base
commit with PR heads merged in and environment variables set, built from git
and measured interleaved. The comparison suite (run_macos_comparison.py)
compares frameworks; this compares changes to zgx.

    python3 scripts/perf/cells.py build cells.json   # every distinct build
    python3 scripts/perf/cells.py run cells.json     # measure (a quiet desktop)
    python3 scripts/perf/cells.py report [results.json]

cells.json:

    {"base": "df93f39e",
     "cells": {"base":      {"merge": ["5f242aaf"]},
               "base+warm": {"merge": ["5f242aaf"], "env": {"ZGX_GPU_WARM": "1"}},
               "timer":     {"merge": ["4b06f949", "5f242aaf"]}},
     "parts": {"gestures": ["base", "base+warm", "timer"],
               "animate":  ["base", "base+warm"],
               "scroll":   ["base", "timer"]},
     "rounds": {"gestures": 4, "animate": 3, "scroll": 3}}

Cells merging the same commits share one build (an environment variable is
the cheapest A/B: one binary, no build noise). Builds go to
`$ZGX_CELLS_DIR` (default /tmp/zgx-cells): a detached worktree of this
repository at `src/`, its own target directory, and each build's binaries
under `builds/<merges>/` with `built-from.txt`. Merges are local only.

Parts:
- gestures: the mail example (crates/zgx-script/examples/mail) under
  `zgx-run --inspect`, with the present log. Per round: idle 4 s (the window
  trims at 1.5 s), the fling tape at '#messages' with the app's CPU and
  instructions sampled over it, idle 4 s, the edge tape, then 5 s sampled at
  rest; each cell launches twice a round, once with the present log for the
  first frames' timings (the log's layer sampler is work of its own) and once
  without it for everything else. Reported per cell: each gesture's first
  frame after the trim (forced whole?, warmed?), its render-to-present and
  render-to-second-present, late frames (over 1.5 refreshes), CPU and
  instructions per frame shown, and CPU at rest.
- animate: the animation workload at the display's rate, 15 s trials
  (macsample: CPU, GPU power, instructions, WindowServer), and one short
  logged launch a cell counting warm-ups (none: it has no input).
- scroll: the heavy and lab workloads scrolling, 15 s trials and 16 s pacing
  captures (framepace: p99, long intervals).

Every launch is a copy staged under /tmp (staging.py) and the run waits for a
quiet desktop and stops at a system dialog without touching it (preflight.py).
The samplers are built as the comparison builds them (/tmp/zgx-macsample,
/tmp/zgx-framepace). Pacing results depend on the display: a 60 Hz Mac
cannot judge 120 Hz pacing.
"""
import glob, json, os, re, statistics, subprocess, sys, threading, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts')); sys.path.insert(0, str(ROOT / 'scripts/perf'))
from staging import clean_env, stage, CWD  # noqa: E402

DIR = Path(os.environ.get('ZGX_CELLS_DIR', '/tmp/zgx-cells'))
SRC, BUILDS, OUT = DIR / 'src', DIR / 'builds', DIR / 'out'
TAPES = {'fling': ROOT / 'scripts/perf/tapes/fling-modeled.tape',
         'edge': ROOT / 'scripts/perf/tapes/edge-top-modeled.tape'}
SAMPLER, PACER = '/tmp/zgx-macsample', '/tmp/zgx-framepace'
EXAMPLES = ['animation_workload', 'heavy_workload', 'component_workload']
SCENES = {'heavy': 'heavy_workload', 'lab': 'component_workload'}
FONT = ROOT / 'comparisons/fonts/DejaVuSans.ttf'


def run(command, **kw):
    print('+', ' '.join(map(str, command)), flush=True)
    subprocess.run(command, check=True, **kw)


def build_key(cell):
    return '+'.join(m[:8] for m in cell.get('merge', [])) or 'base'


def build_dir(spec, cell):
    return BUILDS / f"{spec['base'][:8]}-{build_key(cell)}"


def build(spec):
    if not SRC.exists():
        run(['git', '-C', ROOT, 'worktree', 'add', '--detach', SRC, 'HEAD'])
    run(['git', '-C', SRC, 'fetch', '-q', 'origin'])
    done = set()
    for cell in spec['cells'].values():
        out = build_dir(spec, cell)
        if out in done:
            continue
        done.add(out)
        run(['git', '-C', SRC, 'checkout', '-q', '--detach', spec['base']])
        for sha in cell.get('merge', []):
            run(['git', '-C', SRC, '-c', 'user.name=cells', '-c', 'user.email=cells@localhost',
                 'merge', '-q', '--no-edit', sha])
        env = {k: v for k, v in os.environ.items() if k != 'CARGO_TARGET_DIR'}
        env['CARGO_INCREMENTAL'] = '0'
        run(['cargo', 'build', '-q', '--profile', 'dist', '--locked', '-p', 'zgx-run', '-p', 'zgx-script', '--bin', 'zgx-run', '--bin', 'zgx'],
            cwd=SRC, env=env)
        run(['cargo', 'build', '-q', '--profile', 'dist', '--locked', '-p', 'zgx-desktop']
            + [a for e in EXAMPLES for a in ('--example', e)], cwd=SRC, env=env)
        out.mkdir(parents=True, exist_ok=True)
        for name in ['zgx-run', 'zgx']:
            stage_copy(SRC / 'target/dist' / name, out / name)
        for name in EXAMPLES:
            stage_copy(SRC / 'target/dist/examples' / name, out / name)
        head = subprocess.run(['git', '-C', SRC, 'rev-parse', 'HEAD'], capture_output=True, text=True).stdout.strip()
        (out / 'built-from.txt').write_text(f"base {spec['base']}\n" + ''.join(f'merged {m}\n' for m in cell.get('merge', [])) + f'head {head}\n')
        print('built', out, flush=True)
    run(['git', '-C', SRC, 'checkout', '-q', '--detach', spec['base']])
    if not Path(SAMPLER).exists():
        run(['swiftc', '-O', ROOT / 'scripts/macsample.swift', '-o', SAMPLER, '-lIOReport'])
    if not Path(PACER).exists():
        run(['swiftc', '-O', ROOT / 'scripts/framepace.swift', '-o', PACER])


def stage_copy(source, target):
    target.unlink(missing_ok=True)
    target.write_bytes(source.read_bytes())
    target.chmod(0o755)


def checked(row, log, seconds, mode):
    """`row` with what the app reported in `log` (its ticks, frame rate) and
    whether the run counts, as the comparison's `valid` judges it: it exited
    cleanly, and ticked 58-61 times a second scrolling, or animated at the
    rate it reported."""
    text = log.read_text(errors='replace') if log.exists() else ''
    ticks = re.search(r'"ticks":(\d+)|workload_ticks=(\d+)|animation_frames=(\d+)', text)
    fps = re.search(r'fps=([0-9.]+)', text)
    row['workload_ticks'] = int(next(g for g in ticks.groups() if g)) if ticks else -1
    row['reported_fps'] = float(fps.group(1)) if fps else None
    row['log'] = str(log)
    rate = row['workload_ticks'] / seconds
    if mode == 'animate':
        paced = row['workload_ticks'] > 0 and row['reported_fps'] and abs(rate - row['reported_fps']) <= 0.15 * row['reported_fps']
    else:
        paced = 58 <= rate <= 61
    row['valid'] = bool(row.get('exit_code') == 0 and 'error' not in row and paced)
    return row


def measure(spec):
    import preflight
    OUT.mkdir(parents=True, exist_ok=True)
    rounds = spec.get('rounds', {})
    font = str(stage(FONT))
    # The mail example, outside ~/Documents (staging.py's reason).
    mail = Path('/tmp/zgx-cells-mail')
    subprocess.run(['rsync', '-a', '--delete', f'{ROOT}/crates/zgx-script/examples/mail/', f'{mail}/'], check=True)
    def binary(name, what):
        return str(stage(build_dir(spec, spec['cells'][name]) / what))
    def env_of(name, **extra):
        return clean_env(**extra, **spec['cells'][name].get('env', {}))
    def front(pid):
        subprocess.run(['osascript', '-e', f'tell application "System Events" to set frontmost of (first process whose unix id is {pid}) to true'], capture_output=True)
    def sample(pid, delay, seconds):
        text = subprocess.run([SAMPLER, str(pid), str(delay), str(seconds), '0.05'], capture_output=True, text=True).stdout
        try:
            return json.loads(text)
        except ValueError as e:
            return {'error': f'{e}: {text[:200]}'}
    def dialogs():
        found = preflight.dialogs()
        if found:
            raise SystemExit('a system dialog is on screen; stopping (not clicking it): ' + ' | '.join(found))
    def order(names, r):
        return names if r % 2 == 0 else names[::-1]
    def stop(p):
        p.terminate()
        try:
            return p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            p.kill(); return p.wait()
    findings = preflight.wait_until_quiet(log=lambda line: print(line, flush=True))
    out = {'spec': spec, 'machine': preflight.state(), 'preflight': findings, 'gestures': {}, 'animate': {}, 'scroll': {'trials': {}, 'pacing': {}}}
    hide = 'tell application "System Events" to if exists process "Zeron" then set visible of process "Zeron" to {}'
    subprocess.run(['osascript', '-e', hide.format('false')], capture_output=True)
    try:
        preflight.desktop_still()
        parts = spec.get('parts', {})
        # Each round launches every cell twice: with the present log, for the
        # first frames' timings only (the log samples the layer tree every
        # 4 ms on the main thread, work of its own), and without it, for late
        # frames, CPU and instructions per frame shown, and CPU at rest.
        def gesture_launch(name, r, logged):
            log = OUT / f'{name}-{r}.present.log' if logged else None
            extra = {'ZGX_PRESENT_LOG': '1'} if logged else {}
            app = subprocess.Popen([binary(name, 'zgx-run'), '--inspect', str(mail)], env=env_of(name, **extra), cwd=CWD,
                                   stdout=open(log, 'w') if log else subprocess.DEVNULL, stderr=subprocess.STDOUT)
            time.sleep(3)
            front(app.pid)
            row = {}
            for tape, path in TAPES.items():
                time.sleep(4)  # idle: trimmed, the gesture starts cold
                got = {}
                sampler = None if logged else threading.Thread(target=lambda: got.update(sample(app.pid, 0, 2.6)))
                if sampler:
                    sampler.start()
                answer = subprocess.run([binary(name, 'zgx'), 'inspect', 'replay', '#messages', str(path)],
                                        capture_output=True, text=True, cwd=CWD, timeout=60)
                if sampler:
                    sampler.join()
                text = (answer.stdout + answer.stderr).strip()
                row[tape] = {'replay': text, 'sample': got}
                print(f'gestures {r} {name:12} {"logged" if logged else "plain ":6} {tape:5} {text.splitlines()[0][:150] if text else "no answer"}', flush=True)
            if not logged:
                time.sleep(2)
                row['rest'] = sample(app.pid, 0, 5)
            row['exit_code'] = stop(app)
            time.sleep(2)
            return row
        for r in range(rounds.get('gestures', 4)):
            for name in order(parts.get('gestures', []), r):
                for logged in (True, False) if r % 2 == 0 else (False, True):
                    dialogs()
                    row = gesture_launch(name, r, logged)
                    if not logged:
                        out['gestures'].setdefault(name, []).append(row)
        # Animate has no input, so a warm-up must never fire: one short logged
        # launch a cell counts them; the trials run without the log.
        for name in parts.get('animate', []):
            dialogs()
            p = subprocess.Popen([binary(name, 'animation_workload')], cwd=CWD, stdout=subprocess.DEVNULL,
                                 stderr=open(OUT / f'animate-{name}.present.log', 'w'),
                                 env=env_of(name, ZGX_MODE='animate', ZGX_SECONDS='6', ZGX_FONT_FILE=font, ZGX_PRESENT_LOG='1'))
            time.sleep(0.5); front(p.pid)
            p.wait(timeout=30)
            time.sleep(1)
        for r in range(rounds.get('animate', 3)):
            for name in order(parts.get('animate', []), r):
                dialogs()
                log = OUT / f'animate-{name}-{r}.log'
                p = subprocess.Popen([binary(name, 'animation_workload')], cwd=CWD, stdout=open(log, 'w'), stderr=subprocess.STDOUT,
                                     env=env_of(name, ZGX_MODE='animate', ZGX_SECONDS='15', ZGX_FONT_FILE=font))
                time.sleep(0.5); front(p.pid)
                s = sample(p.pid, 4.5, 10)
                s['exit_code'] = p.wait(timeout=30)
                out['animate'].setdefault(name, []).append(checked(s, log, 15, 'animate'))
                print(f"animate {r} {name:12} cpu {s.get('cpu_percent_one_core', float('nan')):5.2f} gpu {s.get('gpu_system_watts', float('nan')) * 1000:6.1f} mW", flush=True)
                time.sleep(1)
        for r in range(rounds.get('scroll', 3)):
            for scene, example in SCENES.items():
                for name in order(parts.get('scroll', []), r):
                    dialogs()
                    def launch(seconds, log):
                        p = subprocess.Popen([binary(name, example)], cwd=CWD, stdout=open(log, 'w'), stderr=subprocess.STDOUT,
                                             env=env_of(name, ZGX_MODE='scroll', ZGX_SECONDS=str(seconds), ZGX_FONT_FILE=font))
                        time.sleep(0.5); front(p.pid)
                        return p
                    log = OUT / f'scroll-{scene}-{name}-{r}.log'
                    p = launch(15, log)
                    s = sample(p.pid, 4.5, 10)
                    s['exit_code'] = p.wait(timeout=30)
                    out['scroll']['trials'].setdefault(f'{scene}/{name}', []).append(checked(s, log, 15, 'scroll'))
                    time.sleep(1)
                    log = OUT / f'scroll-{scene}-{name}-{r}.pacing.log'
                    p = launch(16, log)
                    try:
                        pace = json.loads(subprocess.run([PACER, str(p.pid), '5', '8'], capture_output=True, text=True).stdout)
                    except ValueError as e:
                        pace = {'error': str(e)}
                    pace['exit_code'] = p.wait(timeout=30)
                    if pace.get('frames', 0) <= 0:
                        pace.setdefault('error', 'no frames captured')
                    out['scroll']['pacing'].setdefault(f'{scene}/{name}', []).append(checked(pace, log, 16, 'scroll'))
                    print(f"scroll {r} {scene}/{name:12} cpu {s.get('cpu_percent_one_core', float('nan')):5.2f} p99 {pace.get('interval_p99_ms', float('nan')):5.1f}", flush=True)
                    time.sleep(1)
    finally:
        subprocess.run(['osascript', '-e', hide.format('true')], capture_output=True)
        (OUT / 'results.json').write_text(json.dumps(out, indent=1))
    print('results', OUT / 'results.json')


def gestures(path):
    """Each gesture after a trim in a present log (`ZGX_PRESENT_LOG=1`): its
    first frame's render line and present line, render to present, first to
    second present, and whether a warm-up (`warm`) came first."""
    lines = []
    for line in open(path, errors='replace'):
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[0] == 'PRESENT':
            lines.append((float(parts[1]), parts[2].strip() if len(parts) == 3 else ''))
    found = []
    for i, (_, what) in enumerate(lines):
        if not what.endswith(' trim'):
            continue
        g, presents = {'warm': False}, []
        for t, w in lines[i + 1:]:
            if w.endswith(' trim'):
                break
            if w == 'warm' and 'render' not in g:
                g['warm'], g['warm_t'] = True, t
            elif w.startswith('render ') and 'render' not in g:
                g['render'], g['render_t'] = w, t
            elif ' draw ' in w and 'render' in g and 'draw' not in g:
                g['draw'] = w
            elif ' present frame=' in w and 'render' in g:
                presents.append((t, w))
                if len(presents) == 2:
                    break
        if len(presents) == 2:
            g['first_present'] = presents[0][1]
            g['render_to_present_ms'] = (presents[0][0] - g['render_t']) * 1e3
            g['first_to_second_ms'] = (presents[1][0] - presents[0][0]) * 1e3
            g['render_to_second_ms'] = (presents[1][0] - g['render_t']) * 1e3
            if g['warm']:
                g['warm_lead_ms'] = (g['render_t'] - g['warm_t']) * 1e3
            found.append(g)
    return found


def report(path):
    res = json.loads(Path(path).read_text())
    out = Path(path).parent
    med = lambda xs: statistics.median(xs) if xs else float('nan')
    late = re.compile(r'(\d+) frames,.*?(\d+) over 1\.5 frames')
    if res['gestures']:
        print('gestures after idle (mail example)')
    for name, rows in res['gestures'].items():
        gs = [g for p in sorted(glob.glob(f'{out}/{glob.escape(name)}-[0-9]*.present.log')) for g in gestures(p)]
        cpu, instr, lates = [], [], []
        for row in rows:
            for tape in TAPES:
                m, s = late.search(row[tape]['replay']), row[tape]['sample']
                if m and 'error' not in s:
                    frames = int(m.group(1))
                    lates.append(int(m.group(2)))
                    cpu.append(s['cpu_percent_one_core'] / 100 * s['wall_seconds'] / frames * 1e6)
                    instr.append(s['instructions'] / frames / 1e3)
        rest = [row['rest']['cpu_percent_one_core'] for row in rows if 'error' not in row['rest']]
        print(f"  {name:12} n={len(gs):2} forced {sum('forced=true' in g['render'] for g in gs)} caught-up {sum(bool(re.search(r'catch_up=[1-9]', g.get('draw', ''))) for g in gs)} "
              f"warm {sum(g['warm'] for g in gs)} (lead {med([g['warm_lead_ms'] for g in gs if 'warm_lead_ms' in g]):4.1f} ms) | "
              f"first render→present {med([g['render_to_present_ms'] for g in gs]):5.2f} ms, render→second {med([g['render_to_second_ms'] for g in gs]):5.2f} ms "
              f"(first→second {med([g['first_to_second_ms'] for g in gs]):5.2f}) | late {sum(lates)} | "
              f"per frame shown {med(cpu):6.1f} µs, {med(instr):6.0f} K instr | rest {med(rest):5.3f}%")
    if res['animate']:
        print('animate')
    for name, rows in res['animate'].items():
        ok = [r for r in rows if r.get('valid', 'error' not in r)]
        warms = sum(1 for p in glob.glob(f'{out}/animate-{glob.escape(name)}.present.log') for line in open(p, errors='replace') if line.rstrip().endswith(' warm'))
        print(f"  {name:12} cpu {med([r['cpu_percent_one_core'] for r in ok]):5.2f}% gpu {med([r['gpu_system_watts'] * 1000 for r in ok]):6.1f} mW "
              f"instr {med([r['instructions'] / r['wall_seconds'] / 1e6 for r in ok]):6.1f} M/s ws cpu {med([r['windowserver_cpu_percent'] for r in ok]):5.1f} warm-ups {warms} n={len(ok)}")
    if res['scroll']['trials']:
        print('scroll')
    for key, rows in res['scroll']['trials'].items():
        ok = [r for r in rows if r.get('valid', 'error' not in r)]
        pc = [p for p in res['scroll']['pacing'].get(key, []) if p.get('valid', 'error' not in p)]
        print(f"  {key:16} cpu {med([r['cpu_percent_one_core'] for r in ok]):5.2f}% gpu {med([r['gpu_system_watts'] * 1000 for r in ok]):6.1f} mW "
              f"ws cpu {med([r['windowserver_cpu_percent'] for r in ok]):5.1f} instr {med([r['instructions'] / r['wall_seconds'] / 1e6 for r in ok]):6.1f} M/s "
              f"p99 {med([p['interval_p99_ms'] for p in pc]):5.1f} ms (range {min([p['interval_p99_ms'] for p in pc], default=float('nan')):.1f}–{max([p['interval_p99_ms'] for p in pc], default=float('nan')):.1f}) "
              f"long {med([p['long_interval_fraction'] * 100 for p in pc]):5.2f}% valid {len(ok)}/{len(rows)} trials, {len(pc)}/{len(res['scroll']['pacing'].get(key, []))} captures")


if __name__ == '__main__':
    if len(sys.argv) < 2 or sys.argv[1] not in ('build', 'run', 'report'):
        raise SystemExit(__doc__)
    if sys.argv[1] == 'report':
        report(sys.argv[2] if len(sys.argv) > 2 else OUT / 'results.json')
    else:
        spec = json.loads(Path(sys.argv[2]).read_text())
        build(spec) if sys.argv[1] == 'build' else measure(spec)
