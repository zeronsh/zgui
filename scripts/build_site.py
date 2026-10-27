#!/usr/bin/env python3
"""The public performance page (site/): one page of the latest accepted macOS
suite run (scripts/run_macos_suite.py), every scene of it; with `--platform
windows`, one of the latest Windows suite run (scripts/run_windows_suite.py)
into site/windows/, in Windows' own metrics.

    python3 scripts/build_site.py docs/results/suite-macos-<date>-<commit>
    python3 scripts/build_site.py --platform windows docs/results/suite-windows-<date>-<commit>
    python3 scripts/build_site.py <suite> --gate docs/results/gate-<from>-<to>/gate.json --previous <last suite>

`--gate` with `--previous` adds a release-over-release section: the CI
gate's exact instruction counts per scene for two releases, and zgx's
change per cell since the last published run, GPUI Fast's beside it.

Only an accepted suite run of one clean commit is published, every scene of
it accepted. Every median and range shown is recomputed from the scenes'
trials (current.csv, pacing.csv, blank.csv) and checked against their
summaries. The headline says only what the numbers show: its verdict and
figures are worked out from them, never written in. Writes site/index.html
(charts as text and CSS, no script), site/data.json (its `platform` says
which) and site/data.csv.
"""
import argparse, shutil, csv, html, json, math, statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = 'https://github.com/zeronsh/zgx'
# zgx's variants (named zgx-*) are never counted among the rivals.
NAMES = {'zgx': 'zgx', 'zgx-luau': 'zgx (Luau)', 'zgx-d3d11': 'zgx (Direct3D 11)',
         'zgx-d3d11-threaded': 'zgx (Direct3D 11, driver threads on)', 'gpui-fast': 'GPUI Fast', 'mygo': 'MyGo',
         'rune': 'Rune'}
ORDER = ['heavy', 'lab', 'animate', 'vlist']
SCENES = {
    'heavy': ('Heavy dashboard', 'A 1280×800 dashboard: 6 tiles, 30 cards of 26 sparkline bars, a status table '
              'and a log. Most values change 60 times a second; the table scrolls.'),
    'lab': ('Text and a 100,000-row list', 'A streaming transcript beside a 100,000-row virtual list, updated 60 '
            'times a second.'),
    'animate': ('Animation', '48 blinking dots and a shimmering line of text, animated on every refresh of a '
                '120 Hz display.'),
    'vlist': ('Variable-height list', '100,000 chat rows of wrapped text, 1 to 8 lines each, about one in four '
              'with an image, in each framework\'s own list widget, which measures the rows: scrolled steadily, '
              'flung, and scrolled with a jump to a far row every half second.'),
}
MODES = {'idle': 'Idle', 'stream': 'Streaming', 'scroll': 'Scrolling', 'both': 'Both', 'animate': 'Animating',
         'fling': 'Flinging', 'jump': 'Jumping'}
# metric: (title, unit, digits, scale, source, modes it applies to)
METRICS = {
    'cpu': ('CPU', '% of one core', 1, 1, 'cpu_percent_one_core', 'all'),
    'server': ('WindowServer CPU', '% of one core: the system compositor, while the app runs', 1, 1,
               'windowserver_cpu_percent', 'all'),
    'gpu': ('GPU power', 'mW, the whole GPU (the window server\'s compositing included), less the desktop alone', 1, 1,
            'gpu_power_mw', 'active'),
    'gpu_time': ('App GPU time', 'million GPU cycles a second, the app\'s own; includes per-frame wake latency', 1, 1,
                 'gpu_mcycles_per_second', 'active'),
    'p99': ('Frame time, 99th percentile', 'ms between frames', 1, 1, 'interval_p99_ms', 'paced'),
    'long': ('Long frames', '% of frames over 25 ms', 1, 100, 'long_interval_fraction', 'paced'),
    'blank': ('Unfilled rows', '% of window captures showing a gap', 1, 100, 'blank_fraction', 'paced'),
    'memory': ('Memory', 'MB, physical footprint', 0, 1e-6, 'mean_footprint_bytes', 'all'),
}
# Rune's adapters' fidelity choices (comparisons/rune/README.md, PATCH.md),
# each with its reason and what it changes.
RUNE_NOTES = '''<li><strong>Ticks through <code>AppHandle::update_if</code>.</strong> Rune's per-frame <code>poll</code>, woken by <code>wake_after</code>, ran on frame boundaries up to 16 ms late and dropped about 5% of ticks. A timer thread now hands each tick to the UI thread through <code>update_if</code>, Rune's API for a timer or feed, and waits for it to run, as the other adapters' loops do. This changes the measurement: Rune now does the same 60 ticks a second of work as the others.</li>
<li><strong>Updates apply at once (<code>Tx::instant</code>).</strong> Rune animates every layout and colour change on a 0.3 s spring by default. Each tick is applied instantly, as the other frameworks apply live data. Leaving the springs on would add animation frames the other frameworks don't draw.</li>
<li><strong>sRGB blending (<code>App::srgb_blend(true)</code>).</strong> Rune blends translucency in linear light by default, so 10% opacity looked brighter than in the others. Its documented sRGB mode matches CSS and the other three. It is a surface format and a shader flag, with no extra pass, so it changes pixels, not cost.</li>
<li><strong>Heavy's panel contents sit 1 pt in.</strong> A Rune border takes no layout space, while CSS, which the scene's geometry follows, places absolute children inside the border. The children are offset by the border's width so that everything lands on the same pixels. Geometry only; the cost doesn't change.</li>
<li><strong>vlist scrolls through a small patch exposing Rune's own wheel path (<code>rune-wheel.patch</code>, 14 lines).</strong> Rune's public API can't set a vertical scroll offset, and only user wheel input moves a list. The patch adds <code>Tx::wheel</code>, which feeds the same <code>Runtime::wheel_xy</code> a real wheel event goes through, at once, between frames. Nothing else in Rune changes. This is what makes Rune's scroll and fling measurable; it adds no work of its own.</li>
<li><strong>Jump reveals the row (nearest), not at the top.</strong> Rune's <code>scroll_into_view</code> scrolls just far enough to show a row, where the other adapters put the jumped-to row at the top. A jump still mounts and lays out the far rows, but the row lands at a different place in the viewport.</li>
<li><strong>Rune's window isn't colour-matched by the window server.</strong> Its Metal layer is untagged (wgpu 30's default, which Rune keeps), so sRGB values reach the display unconverted and look slightly more saturated. The window server converts the other three, whose layers are tagged sRGB. Rune may skip that colour-matching work, which could lower its WindowServer cost. Kept as Rune ships it.</li>
<li><strong>The vlist probe runs headless.</strong> Row heights are read from Rune's headless runtime, with the same font and 720 pt width, after revealing each row. A row's height doesn't depend on where it sits. Only the layout check uses this; it doesn't feed any measured figure.</li>'''
TRIAL_KEYS = ['cpu_percent_one_core', 'gpu_system_watts', 'gpu_mcycles_per_second', 'mean_footprint_bytes',
              'windowserver_cpu_percent']
PACING_KEYS = ['interval_p99_ms', 'long_interval_fraction', 'frames_per_second']
BLANK_KEYS = ['blank_fraction', 'longest_blank_points']
# Windows (scripts/run_windows_suite.py): no system GPU power to read, so the
# GPU is the app's own GPU time and the compositor's (DWM) beside it; the
# NVIDIA board's power only with --board-power, where it held still across
# repeats. CPU is the process's cycles (Windows' CPU time counts scheduler
# ticks of 15.6 ms: zgx's same animation read 0.94% and 0.31% in two
# trials of 1.59 and 1.50 M cycles a frame), a second and per frame.
WINDOWS_METRICS = {
    'cpu': ('CPU', 'million cycles a second, the app\'s process', 1, 1, 'mcycles_per_second', 'all'),
    'cycles': ('CPU cycles per frame', 'million cycles of the app\'s process per update (the animation: per frame drawn)',
               2, 1, 'mcycles_per_frame', 'active'),
    'server': ('DWM CPU', 'million cycles a second of the system compositor (Desktop Window Manager), while the app runs',
               1, 1, 'dwm_mcycles_per_second', 'all'),
    'gpu_time': ('App GPU time', 'ms a second on the GPU\'s 3D engines, the app\'s own', 2, 1, 'gpu_ms_per_second', 'active'),
    'dwm_gpu': ('DWM GPU time', 'ms a second on the GPU\'s 3D engines, the compositor\'s, while the app runs', 2, 1,
                'dwm_gpu_ms_per_second', 'active'),
    'board': ('Board power', 'W, the NVIDIA board, less the desktop alone', 1, 1, 'gpu_board_power_w', 'active'),
    'p99': ('Frame time, 99th percentile', 'ms between frames', 1, 1, 'interval_p99_ms', 'paced'),
    'long': ('Long frames', '% of frames over 25 ms', 1, 100, 'long_interval_fraction', 'paced'),
    'memory': ('Memory', 'MB, private bytes (committed memory)', 0, 1e-6, 'mean_private_bytes', 'all'),
}
WINDOWS_TRIAL_KEYS = ['mcycles_per_second', 'cpu_percent_one_core', 'mcycles_per_frame', 'gpu_ms_per_second', 'dwm_cpu_percent',
                      'dwm_mcycles_per_second', 'dwm_gpu_ms_per_second', 'mean_private_bytes', 'mean_working_set_bytes']
PLATFORM = 'macos'
PLATFORM_NAMES = {'macos': 'macOS', 'windows': 'Windows'}
# Windows: whether the board's power is published (--board-power).
BOARD_POWER = False


def spread(values):
    return {'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def check(summary_cell, values, what):
    for field, value in spread(values).items():
        assert math.isclose(summary_cell[field], value, rel_tol=1e-9, abs_tol=1e-9), (what, field, summary_cell[field], value)


def load(directory):
    """One accepted scene of the suite: its cells, each recomputed from the trials."""
    audit = json.loads((directory / 'audit.json').read_text())
    summary = json.loads((directory / 'summary.json').read_text())
    protocol = audit['protocol']
    assert audit['accepted'] and not audit.get('invalid_trials'), f'{directory}: not accepted'
    assert protocol.get('suite') and not protocol.get('quick') and not protocol.get('pgo'), f'{directory}: not a suite run'
    assert not audit['source_dirty'], f'{directory}: measured with uncommitted changes'
    assert audit.get('platform', 'macos') == PLATFORM, f'{directory}: a {audit.get("platform", "macos")} run'
    frameworks, modes = protocol['frameworks'], protocol['modes']
    trials = list(csv.DictReader((directory / 'current.csv').open()))
    paced = list(csv.DictReader((directory / 'pacing.csv').open()))
    blanks = list(csv.DictReader((directory / 'blank.csv').open())) if (directory / 'blank.csv').exists() else []
    extended = set(protocol.get('extended_cells') or [])
    desktop_rows = [t for t in trials if t['framework'] == 'desktop']
    if PLATFORM == 'windows':
        return load_windows(directory, audit, summary, trials, paced, extended, desktop_rows)
    # The whole GPU's power with no app running: each repeat's desktop baseline.
    desktop_gpu = statistics.median(float(t['gpu_system_watts']) for t in trials if t['framework'] == 'desktop')
    desktop_server = {t['repeat']: float(t['windowserver_cpu_percent']) for t in desktop_rows}
    assert all(t['repeat'] in desktop_server for t in trials if t['mode'] == 'idle'), f'{directory}: an idle round without its desktop'
    cells = []
    for framework in frameworks:
        for mode in modes:
            rows = [t for t in trials if (t['framework'], t['mode']) == (framework, mode)]
            want = protocol['idle_repeats'] if mode == 'idle' else protocol['repeats']
            want += protocol['extra_trials'] if f'{framework}/{mode}' in extended else 0
            assert len(rows) == want and {int(t['repeat']) for t in rows} == set(range(want)), (directory, framework, mode)
            group = next(g for g in summary['groups'] if (g['framework'], g['mode']) == (framework, mode))
            cell = {'framework': framework, 'mode': mode, 'repeats': want}
            for key in TRIAL_KEYS:
                values = [float(t[key]) for t in rows if t.get(key) not in (None, '')]
                if values:
                    check(group[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
            # What the app adds to the GPU's power, in mW.
            cell['gpu_power_mw'] = spread([(float(t['gpu_system_watts']) - desktop_gpu) * 1000 for t in rows])
            if mode == 'idle' and audit['scene'] != 'field':
                # Idle WindowServer less its own round's desktop alone (the
                # field shows its carets' whole cost, as measured): the
                # window server reads one of a few levels launch to launch,
                # for every app and the desktop alike (M1 Max, 2026-10-28:
                # no launch of 32 over its round's desktop by 0.2 points).
                cell['windowserver_over_desktop'] = spread(
                    [float(t['windowserver_cpu_percent']) - desktop_server[t['repeat']] for t in rows])
            if mode != 'idle':
                captures = [p for p in paced if (p['framework'], p['mode']) == (framework, mode)]
                assert len(captures) == protocol['pacing_repeats'], (directory, framework, mode)
                pace = next(p for p in summary['pacing'] if (p['framework'], p['mode']) == (framework, mode))
                for key in PACING_KEYS:
                    values = [float(p[key]) for p in captures]
                    check(pace[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
                cell['pacing_captures'] = len(captures)
                shots = [b for b in blanks if (b['framework'], b['mode']) == (framework, mode)]
                if shots:
                    assert all(not b.get('error') for b in shots), (directory, framework, mode, 'blank capture failed')
                    group = next(g for g in summary['blank'] if (g['framework'], g['mode']) == (framework, mode))
                    for key in BLANK_KEYS:
                        values = [float(b[key]) for b in shots]
                        check(group[key], values, (directory.name, framework, mode, key))
                        cell[key] = spread(values)
                    cell['blank_captures'] = sum(int(b['captures']) for b in shots)
            cells.append(cell)
    # WindowServer with no app running, each repeat's desktop baseline.
    desktop = [float(t['windowserver_cpu_percent']) for t in trials if t['framework'] == 'desktop']
    over = [c['windowserver_over_desktop']['median'] for c in cells if 'windowserver_over_desktop' in c]
    return {
        'desktop_server': statistics.median(desktop), 'desktop_gpu_mw': desktop_gpu * 1000,
        # Idle WindowServer is noise while no app's median over its round's
        # desktop exceeds what the desktop alone spreads over the rounds.
        **({'idle_server': {'desktop': spread(desktop),
                            'noise': all(abs(o) <= max(desktop) - min(desktop) for o in over)}} if over else {}),
        'scene': audit['scene'], 'run': directory.relative_to(ROOT).as_posix(), 'commit': audit['commit'],
        'started_utc': audit['started_utc'], 'frameworks': frameworks, 'modes': modes, 'cells': cells,
        'trials': audit['trials'], 'pacing_trials': audit['pacing_trials'], 'protocol': protocol,
        'environment': audit['environment'], 'binaries': audit['binaries'],
        'toolchains': audit.get('toolchains', {}), 'preflight': audit.get('preflight'),
        'abnormal_exits': audit.get('abnormal_exits'), 'launches': audit.get('launches', {}),
    }


def load_windows(directory, audit, summary, trials, paced, extended, desktop_rows):
    """`load` for a Windows scene: its cells in Windows' metrics."""
    protocol = audit['protocol']
    frameworks, modes = protocol['frameworks'], protocol['modes']
    board = [float(t['gpu_board_watts']) for t in desktop_rows if t.get('gpu_board_watts') not in (None, '')]
    desktop_board = statistics.median(board) if board else None
    cells = []
    for framework in frameworks:
        for mode in modes:
            rows = [t for t in trials if (t['framework'], t['mode']) == (framework, mode)]
            want = protocol['idle_repeats'] if mode == 'idle' else protocol['repeats']
            want += protocol['extra_trials'] if f'{framework}/{mode}' in extended else 0
            assert len(rows) == want and {int(t['repeat']) for t in rows} == set(range(want)), (directory, framework, mode)
            group = next(g for g in summary['groups'] if (g['framework'], g['mode']) == (framework, mode))
            cell = {'framework': framework, 'mode': mode, 'repeats': want}
            for key in WINDOWS_TRIAL_KEYS:
                values = [float(t[key]) for t in rows if t.get(key) not in (None, '')]
                if values:
                    check(group[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
            if BOARD_POWER and mode != 'idle':
                # What the app adds to the board's power, in W.
                cell['gpu_board_power_w'] = spread([float(t['gpu_board_watts']) - desktop_board for t in rows])
            if mode != 'idle':
                captures = [p for p in paced if (p['framework'], p['mode']) == (framework, mode)]
                assert len(captures) == protocol['pacing_repeats'], (directory, framework, mode)
                pace = next(p for p in summary['pacing'] if (p['framework'], p['mode']) == (framework, mode))
                for key in PACING_KEYS:
                    values = [float(p[key]) for p in captures]
                    check(pace[key], values, (directory.name, framework, mode, key))
                    cell[key] = spread(values)
                cell['pacing_captures'] = len(captures)
            cells.append(cell)
    median = lambda key: statistics.median(float(t[key]) for t in desktop_rows)
    # The graphics APIs each app's process loaded (the driver's memory floor differs).
    graphics = {f: sorted({m for t in trials if t['framework'] == f for m in t.get('graphics_modules', '').split()})
                for f in frameworks}
    return {
        'graphics_modules': graphics,
        'desktop_server': median('dwm_mcycles_per_second'), 'desktop_dwm_gpu': median('dwm_gpu_ms_per_second'),
        'desktop_board_w': desktop_board,
        'scene': audit['scene'], 'run': directory.relative_to(ROOT).as_posix(), 'commit': audit['commit'],
        'started_utc': audit['started_utc'], 'frameworks': frameworks, 'modes': modes, 'cells': cells,
        'trials': audit['trials'], 'pacing_trials': audit['pacing_trials'], 'protocol': protocol,
        'environment': audit['environment'], 'binaries': audit['binaries'],
        'toolchains': audit.get('toolchains', {}), 'preflight': audit.get('preflight'),
        'abnormal_exits': audit.get('abnormal_exits'), 'launches': audit.get('launches', {}),
    }


def cell(scene, framework, mode):
    return next(c for c in scene['cells'] if (c['framework'], c['mode']) == (framework, mode))


def value(scene, framework, mode, key, field='median'):
    return cell(scene, framework, mode)[key][field]


def fmt(v, digits):
    return f'{v:,.{digits}f}'


def percent_less(ours, theirs):
    """`ours` against `theirs` in words: "43% less" or "12% more"."""
    change = 1 - ours / theirs
    return f'{abs(change) * 100:.0f}% {"less" if change >= 0 else "more"}'


def headline(by):
    """The verdict and headline figures, worked out from the numbers."""
    rivals = [f for f in by['heavy']['frameworks'] if not f.startswith('zgx')]
    named = ', '.join(NAMES[r] for r in rivals[:-1]) + f' and {NAMES[rivals[-1]]}' if len(rivals) > 1 else NAMES[rivals[0]]
    active = [(s, m) for s in by.values() for m in s['modes'] if m != 'idle']
    # Windows has no system GPU power: its GPU figure is the app's GPU time.
    gpu, gpu_name = ('gpu_ms_per_second', 'app GPU time') if PLATFORM == 'windows' else ('gpu_power_mw', 'GPU power')
    # And its CPU figure is cycles (see WINDOWS_METRICS).
    cpu = 'mcycles_per_second' if PLATFORM == 'windows' else 'cpu_percent_one_core'
    led = {key: sum(all(value(s, 'zgx', m, key) < value(s, r, m, key) for r in rivals if r in s['frameworks'])
                    for s, m in active)
           for key in (cpu, gpu)}
    tests = len(active)
    long_zgx = max(value(s, 'zgx', m, 'long_interval_fraction') for s, m in active)
    if led[cpu] == tests and led[gpu] == tests:
        verdict = f'In every test, zgx uses less CPU and less {gpu_name} than {named}'
    else:
        verdict = (f'zgx uses the least CPU in {led[cpu]} of {tests} tests and the least {gpu_name} '
                   f'in {led[gpu]}, against {named}')
    verdict += ', and none of its frames run long.' if long_zgx == 0 else '.'
    heavy = by['heavy']
    against = lambda key, mode: ', '.join(
        f'{percent_less(value(heavy, "zgx", mode, key), value(heavy, r, mode, key))} than {NAMES[r]}' for r in rivals)
    long_rivals = max(value(s, r, m, 'long_interval_fraction') for s, m in active for r in rivals if r in s['frameworks'])
    idle = {f: value(heavy, f, 'idle', cpu) for f in ['zgx', *rivals]}
    idle_unit = ' M cycles/s' if PLATFORM == 'windows' else '%'
    gpu_card = ((f"{fmt(value(heavy, 'zgx', 'both', gpu), 1)} ms/s", 'App GPU time, heavy dashboard (both)')
                if PLATFORM == 'windows' else
                (f"{fmt(value(heavy, 'zgx', 'both', gpu), 0)} mW", 'GPU power, heavy dashboard (both)'))
    cards = [
        (f"{fmt(value(heavy, 'zgx', 'both', cpu), 1)}{' M cycles/s' if PLATFORM == 'windows' else '%'}",
         'CPU, heavy dashboard (both)', against(cpu, 'both')),
        (*gpu_card, against(gpu, 'both')),
        (f'{long_zgx * 100:.0f}%' if long_zgx == 0 else f'{long_zgx * 100:.1f}%', 'long frames (over 25 ms), at most',
         f'median of each mode\'s captures, every scene; {named}: up to {long_rivals * 100:.1f}%'),
        (f"{fmt(idle['zgx'], 2)}{idle_unit}", 'CPU while idle',
         ', '.join(f"{NAMES[r]} {fmt(idle[r], 2)}{idle_unit}" for r in rivals)),
    ]
    return verdict, cards


def metrics():
    """The page's metrics: macOS's, or Windows' (board power only with --board-power)."""
    if PLATFORM == 'windows':
        return {m: v for m, v in WINDOWS_METRICS.items() if m != 'board' or BOARD_POWER}
    return METRICS


def idle_server_label(scene):
    """The idle row of WindowServer's chart: over the desktop alone, and whether that is noise."""
    idle = scene['idle_server']
    return (f"{MODES['idle']}, less the desktop"
            + (f": within noise (no app above the desktop alone's own spread, "
               f"{fmt(idle['desktop']['min'], 2)}–{fmt(idle['desktop']['max'], 2)})" if idle['noise'] else ''))


def chart(scene, metric):
    title, unit, digits, scale, key, applies = metrics()[metric]
    if metric == 'server':
        unit += f' (the desktop alone: {fmt(scene["desktop_server"], 1)})'
    if metric == 'gpu':
        unit += f' ({fmt(scene["desktop_gpu_mw"], 1)} mW)'
    if metric == 'dwm_gpu':
        unit += f' (the desktop alone: {fmt(scene["desktop_dwm_gpu"], 2)})'
    if metric == 'board':
        unit += f' ({fmt(scene["desktop_board_w"], 1)} W)'
    modes = [m for m in scene['modes'] if applies == 'all' or m != 'idle']
    # Idle WindowServer: what each app adds over its round's desktop alone.
    over = metric == 'server' and 'idle_server' in scene
    source = lambda m: 'windowserver_over_desktop' if over and m == 'idle' else key
    rows = [(m, f, c[source(m)]) for m in modes for f in scene['frameworks']
            for c in [cell(scene, f, m)] if source(m) in c]
    top = max(max(s['median'], 0) * scale for _, _, s in rows) or 1
    out = [f'<figure class="chart"><table><caption><span class="title">{title}</span> '
           f'<span class="unit">{unit}</span> <span class="better">lower is better</span></caption>',
           '<colgroup><col class="c-mode"><col class="c-fw"><col></colgroup>'
           '<thead><tr><th scope="col">Mode</th><th scope="col">Framework</th>'
           '<th scope="col">Median <span class="range-head">(range)</span></th></tr></thead>']
    for mode in modes:
        group = [(f, s) for m, f, s in rows if m == mode]
        out.append('<tbody>')
        for i, (framework, s) in enumerate(group):
            median, low, high = (s[k] * scale for k in ('median', 'min', 'max'))
            width = 100 * max(median, 0) / top
            # Idle CPU is a fraction of a percent: two places show it.
            places = 2 if (metric, mode) == ('cpu', 'idle') else digits
            label = idle_server_label(scene) if over and mode == 'idle' else MODES[mode]
            head = f'<th scope="rowgroup" rowspan="{len(group)}" class="mode">{label}</th>' if i == 0 else ''
            low, high = fmt(low, places), fmt(high, places)
            spread_text = '' if low == high else f' <span class="range">({low}–{high})</span>'
            out.append(f'<tr class="fw-{framework}">{head}<th scope="row">{NAMES[framework]}</th>'
                       f'<td><span class="track" aria-hidden="true"><span class="bar" style="width:{width:.1f}%"></span></span>'
                       f'<span class="value">{fmt(median, places)}</span>{spread_text}</td></tr>')
        out.append('</tbody>')
    out.append('</table></figure>')
    return '\n'.join(out)


MICRO_SCENES = {
    'state': ('State update', 'One counter bound to one text, over a static grid of 1,000 texts: set the counter.'),
    'vscroll-fixed': ('Scroll step, fixed rows', '10,000 rows of 28 pt: scroll 12 pt.'),
    'vscroll-variable': ('Scroll step, variable rows', '10,000 of the list scene\'s rows (wrapped text, some images), measured as they mount: scroll 12 pt.'),
}


def load_micro(path):
    """The microbenchmarks' run (micro.jsonl): frame figures by scene and column."""
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.startswith('{')]
    frames = {}
    for row in rows:
        if row['figure'] != 'frame' or row.get('note'):
            continue
        if row['framework'] in ('zgx', 'zgx-luau'):
            column = row['framework'] + ('' if row.get('render') else '-noencode')
        elif row['framework'] == 'mygo':
            column = 'mygo' if row.get('raster') == '0.05x' else 'mygo-1x'
        else:
            column = row['framework']
        frames.setdefault(row['scene'], {})[column] = row
    return frames


def micro_section(frames, e):
    """Per scene, two like-for-like groups: frames without the GPU's encode
    (zgx's RENDER=0, GPUI Fast's headless frame) and with it (zgx, Rune, and
    MyGo, whose frame also rasterizes on the CPU)."""
    groups = [('Without the GPU encode', [('zgx-noencode', 'zgx'), ('zgx-luau-noencode', 'zgx (Luau)'), ('gpui-fast', 'GPUI Fast')]),
              ('With the GPU encode', [('zgx', 'zgx'), ('zgx-luau', 'zgx (Luau)'), ('rune', 'Rune'), ('mygo', 'MyGo')])]
    charts = []
    for scene, (title, note) in MICRO_SCENES.items():
        cells = frames.get(scene, {})
        out = [f'<figure class="chart"><table><caption><span class="title">{e(title)}</span> '
               f'<span class="unit">µs of CPU per operation</span> '
               f'<span class="better">lower is better</span><span class="unit"> {e(note)}</span></caption>',
               '<colgroup><col class="c-mode"><col class="c-fw"><col></colgroup>'
               '<thead><tr><th scope="col">Frame</th><th scope="col">Framework</th>'
               '<th scope="col">Median <span class="range-head">(p10–p90)</span></th></tr></thead>']
        for label, members in groups:
            present = [(key, name, cells[key]) for key, name in members if key in cells]
            if not present:
                continue
            top = max(row['median_ns'] for _, _, row in present) or 1
            out.append('<tbody>')
            for i, (key, name, row) in enumerate(present):
                fw = 'zgx-luau' if key.startswith('zgx-luau') else 'zgx' if key.startswith('zgx') else key
                head = f'<th scope="rowgroup" rowspan="{len(present)}" class="mode">{e(label)}</th>' if i == 0 else ''
                us = lambda ns: fmt(ns / 1000, 1 if row['median_ns'] < 100_000 else 0)
                out.append(f'<tr class="fw-{fw}">{head}<th scope="row">{e(name)}</th>'
                           f'<td><span class="track" aria-hidden="true"><span class="bar" style="width:{100 * row["median_ns"] / top:.1f}%"></span></span>'
                           f'<span class="value">{us(row["median_ns"])}</span> <span class="range">({us(row["p10_ns"])}–{us(row["p90_ns"])})</span></td></tr>')
            out.append('</tbody>')
        out.append('</table></figure>')
        charts.append('\n'.join(out))
    return (
        '<section id="micro" aria-labelledby="micro-h"><h2 id="micro-h">Microbenchmarks</h2>'
        '<p class="scene-note">One state change, and one scroll step of 10,000 rows, timed headless in process CPU per operation '
        '(the median over batches after a warm-up) from the change through to a frame ready to present. '
        'Each framework\'s frame is compared with the frames that do the same work: GPUI Fast\'s headless frame stops before the GPU encode, '
        'so it sits beside zgx\'s frame without one; Rune\'s and MyGo\'s include it (MyGo\'s also rasterizes on the CPU), beside zgx\'s with it. '
        f'<a href="{REPO}/blob/main/docs/benchmarking.md#microbenchmarks-one-state-change-one-scroll-step">How they are defined</a></p>'
        f'<div class="charts">{"".join(charts)}</div>'
        '<ul class="facts">'
        '<li><strong>The change alone</strong> ("write") is not compared: GPUI Fast, Rune and MyGo store the value and mark the window to draw '
        '(0.02–0.14 µs), leaving all their work to the frame, where zgx\'s bindings update their nodes as the value is set (about 1–8 µs). '
        'The frame is the figure that counts, and it includes the write.</li>'
        '<li><strong>MyGo\'s frame</strong> rasterizes its scene on the CPU, as its headless tester does, where a window would draw on the GPU; '
        'it is shown at a raster scale of 0.05× (the charts\' MyGo), which leaves its layout and paint and makes the raster almost free (at 1×: '
        + ', '.join(f"{MICRO_SCENES[sc][0].lower()} {fmt(rows['mygo-1x']['median_ns'] / 1000, 0)} µs" for sc, rows in frames.items() if 'mygo-1x' in rows)
        + ').</li>'
        '<li><strong>Most of a frame with the GPU encode is what any Metal frame costs:</strong> a bare Metal frame (a command buffer, one empty render pass, '
        'commit) takes 44–94 µs of process CPU back to back on this Mac, most of it in Metal\'s own threads; zgx\'s work over it is the few microseconds '
        'of its frame without the encode and about 33,000 instructions of drawing and presenting.</li>'
        + ('' if any('zgx-luau' in cells for cells in frames.values()) else '<li><strong>Not yet measured:</strong> zgx through Luau.</li>') +
        '</ul></section>')


STARTUP_METRICS = [('content_ms', 'Launch to first content', 'ms', 0), ('cpu_ms', 'CPU to first content', 'ms of CPU', 0),
                   ('instructions', 'Instructions to first content', 'million instructions', 0)]
# Windows counts cycles where macOS counts instructions (WINDOWS_METRICS).
WINDOWS_STARTUP_METRICS = STARTUP_METRICS[:2] + [('cycles', 'Cycles to first content', 'million cycles', 0)]


def startup_section(startup, e):
    """Launch to first content, one chart per metric: medians over the
    launches, their range beside them."""
    names = {'zgx': 'zgx', 'gpui-fast': 'GPUI Fast', 'mygo': 'MyGo', 'rune': 'Rune'}
    charts = []
    for key, title, unit, digits in (WINDOWS_STARTUP_METRICS if PLATFORM == 'windows' else STARTUP_METRICS):
        rows = [(fw, startup['medians'][fw][key]) for fw in names if fw in startup['medians']]
        scale = 1e6 if key in ('instructions', 'cycles') else 1
        top = max(row['median'] for _, row in rows) or 1
        out = [f'<figure class="chart"><table><caption><span class="title">{e(title)}</span> '
               f'<span class="unit">{e(unit)}</span> <span class="better">lower is better</span></caption>',
               '<colgroup><col class="c-fw"><col></colgroup>'
               '<thead><tr><th scope="col">Framework</th><th scope="col">Median <span class="range-head">(range)</span></th></tr></thead><tbody>']
        for fw, row in rows:
            value = lambda v: fmt(v / scale, digits)
            out.append(f'<tr class="fw-{fw}"><th scope="row">{e(names[fw])}</th>'
                       f'<td><span class="track" aria-hidden="true"><span class="bar" style="width:{100 * row["median"] / top:.1f}%"></span></span>'
                       f'<span class="value">{value(row["median"])}</span> <span class="range">({value(row["min"])}–{value(row["max"])})</span></td></tr>')
        out.append('</tbody></table></figure>')
        charts.append('\n'.join(out))
    launches = len(next(iter(startup['launches'].values())))
    if PLATFORM == 'windows':
        return (
            '<section id="startup" aria-labelledby="startup-h"><h2 id="startup-h">Startup</h2>'
            f'<p class="scene-note">From launch to the window&#8217;s first content, {launches} launches of each in rotating order: '
            'the probe starts the app (the lab scene, idle, in the suite&#8217;s font), waits for its window, then reads the top of '
            'its content as the window drew it until the first text has drawn, and reads the process&#8217;s CPU time and cycles '
            'at that moment. zgx shows its window with its first frame; MyGo and Rune show theirs before they draw. '
            f'Measured after the suite, on the same PC, at <code>{e(startup["commit"][:7])}</code>.</p>'
            f'<div class="charts">{"".join(charts)}</div></section>'
        )
    return (
        '<section id="startup" aria-labelledby="startup-h"><h2 id="startup-h">Startup</h2>'
        f'<p class="scene-note">From launch to the window&#8217;s first content, {launches} launches of each in alternating order: '
        'the probe starts the app, waits for its window, then captures the top of its content until the first text has drawn, '
        'and reads the process&#8217;s CPU time and instructions retired at that moment. '
        'Both apps build the same menu bar at launch (Services, Hide, Quit, and the Window menu), so they start the same work. '
        f'Measured separately from the suite, on the same Mac, at <code>{e(startup["commit"][:7])}</code>.</p>'
        f'<div class="charts">{"".join(charts)}</div></section>'
    )

# Release-over-release: the metrics compared between two suite runs.
RELEASE_METRICS = [('cpu', 'CPU'), ('gpu', 'GPU power'), ('memory', 'Memory'), ('server', 'WindowServer CPU')]


def release_changes(previous, by):
    """Per scene, mode and framework present in both runs, each metric's
    median change between the runs, as a fraction."""
    out = []
    for n in ORDER:
        old, new = previous[n], by[n]
        for mode in new['modes']:
            for fw in new['frameworks']:
                try:
                    a, b = cell(old, fw, mode), cell(new, fw, mode)
                except (KeyError, StopIteration):
                    continue
                change = {}
                for metric, _ in RELEASE_METRICS:
                    key, applies = metrics()[metric][4], metrics()[metric][5]
                    # GPU power idle is the desktop's to the milliwatt: no ratio.
                    if applies == 'active' and mode == 'idle':
                        continue
                    if key in a and key in b and a[key]['median']:
                        scale = metrics()[metric][3]
                        change[metric] = {'from': a[key]['median'] * scale, 'to': b[key]['median'] * scale,
                                          'change': b[key]['median'] / a[key]['median'] - 1}
                out.append({'scene': n, 'mode': mode, 'framework': fw, 'change': change})
    return out


def release_section(release, e):
    """What changed since the last release: the instruction gate's exact
    counts, then this run against the last published one, for zgx, with
    GPUI Fast beside it as the measure of what the session itself moved."""
    gate = release['gate']
    rows = sorted(gate['scenes'].items(), key=lambda kv: kv[1][2])
    lower = sum(v[2] < 0 for v in gate['scenes'].values())
    signed = lambda f: ('&#177;0.0%' if round(f * 100, 1) == 0 else f'{f * 100:+.1f}%'.replace('-', '&#8722;'))
    out = [f'<section id="release" aria-labelledby="release-h"><h2 id="release-h">{e(gate["from"]["version"])} &#8594; '
           f'{e(gate["to"]["version"])}</h2>',
           f'<p class="scene-note">Instructions each scene of the CI gate runs ({e(gate["how"])}), '
           f'{e(gate["from"]["version"])} (<code>{e(gate["from"]["commit"][:8])}</code>) against '
           f'{e(gate["to"]["version"])} (<code>{e(gate["to"]["commit"][:8])}</code>): exact counts, not samples. '
           f'{lower} of {len(rows)} scenes run fewer.</p>',
           '<figure class="chart"><table><caption><span class="title">Instructions per scene</span> '
           f'<span class="unit">{e(gate["unit"])}</span> <span class="better">lower is better</span></caption>'
           '<thead><tr><th scope="col">Scene</th><th scope="col">' + e(gate['from']['version']) + '</th><th scope="col">'
           + e(gate['to']['version']) + '</th><th scope="col">Change</th></tr></thead><tbody>']
    for name, (a, b, change) in rows:
        out.append(f'<tr><th scope="row">{e(name)}</th><td>{fmt(a, 1)}</td><td>{fmt(b, 1)}</td>'
                   f'<td>{f"{change * 100:+.2f}%".replace("-", "&#8722;")}</td></tr>')
    out.append('</tbody></table></figure>')
    previous = release['previous_commit']
    out.append(f'<p class="scene-note">The suite, this run against the last one published (<code>{e(previous[:7])}</code>, '
               f'{e(release["previous_date"])}), each cell&#8217;s median. The runs are in different sessions, and a session '
               'moves a whole run (see the caveats), so GPUI Fast is shown beside zgx'
               + (f' ({e(release["rebuilt"])})' if release.get('rebuilt') else '') +
               ': the part of a change it shares is the session&#8217;s. Idle shows its values, which are near nothing.</p>')
    changes = {(c['scene'], c['mode'], c['framework']): c['change'] for c in release['changes']}
    out.append('<figure class="chart"><table><caption><span class="title">Change since the last run</span> '
               '<span class="unit">zgx, and GPUI Fast in brackets</span> <span class="better">lower is better</span></caption>'
               '<thead><tr><th scope="col">Scene</th>' + ''.join(f'<th scope="col">{e(t)}</th>' for _, t in RELEASE_METRICS)
               + '</tr></thead><tbody>')
    for n in ORDER:
        for mode in release['modes'][n]:
            for fw in ('zgx', 'zgx-luau'):
                ours = changes.get((n, mode, fw))
                if ours is None:
                    continue
                theirs = changes.get((n, mode, 'gpui-fast'), {})
                label = f'{SCENES[n][0]}, {MODES[mode].lower()}' + (' (Luau)' if fw == 'zgx-luau' else '')
                cells = []
                for metric, _ in RELEASE_METRICS:
                    if metric not in ours:
                        cells.append('<td>&#8211;</td>')
                        continue
                    # Idle is near nothing: its values, not a ratio of them.
                    digits = 2 if metrics()[metric][2] else 0
                    show = ((lambda c: f"{fmt(c['from'], digits)} &#8594; {fmt(c['to'], digits)}") if mode == 'idle'
                            else (lambda c: signed(c['change'])))
                    ref = f' <span class="range">({show(theirs[metric])})</span>' if metric in theirs else ''
                    cells.append(f'<td>{show(ours[metric])}{ref}</td>')
                out.append(f'<tr><th scope="row">{e(label)}</th>{"".join(cells)}</tr>')
    out.append('</tbody></table></figure></section>')
    return '\n'.join(out)


# The field scene: each framework's own text field, focused, left alone;
# how its caret draws, which is what decides its memory.
FIELD_CARETS = {
    'zgx': 'zgx&#8217;s text input: its caret blinks as the system&#8217;s, a Core Animation layer shown and hidden each phase, so the window draws no frame while it blinks',
    'gpui-fast': 'GPUI has no text field of its own: a 2 &#215; 20 point bar shown and hidden every 530 ms in a still window, as a caret blinks',
    'mygo': 'MyGo&#8217;s <code>TextInput</code>: its caret blinks every 530 ms, a frame each time',
    'rune': 'Rune&#8217;s <code>text_input</code>: its caret does not blink, so the window draws no frame while it is left alone',
}


def field_section(field, e):
    """A focused field left alone: memory first, then CPU and WindowServer."""
    charts = ''.join(chart(field, metric) for metric in ('memory', 'cpu', 'server'))
    carets = ''.join(f'<li><strong>{e(NAMES[f])}:</strong> {FIELD_CARETS[f]}.</li>' for f in field['frameworks'] if f in FIELD_CARETS)
    return (
        '<section id="field" aria-labelledby="field-h"><h2 id="field-h">A focused field, left alone</h2>'
        '<p class="scene-note">The window most apps show most of the time: a text field with the focus, its user reading. '
        f'Each framework&#8217;s own field in a 960 &#215; 720 window, sampled {field["protocol"]["idle_window_seconds"][0]}&#8211;'
        f'{field["protocol"]["idle_window_seconds"][1]} s after launch, {field["protocol"]["idle_repeats"]} launches each. '
        'A window that draws a frame every few seconds keeps the Metal driver&#8217;s memory for its command queue, about 176 MB, '
        'for as long as it does. A window that changes only its layers (a caret Core Animation draws) settles at one of two '
        'levels, launch to launch, about 25 MB apart: after any Metal work, macOS keeps part of that memory in about half the '
        'launches while a process keeps changing its layers (reproduced in a plain Swift app: two Metal frames, then a layer '
        'flipped twice a second). Read zgx&#8217;s memory with its range. '
        f'Measured separately from the suite, at <code>{e(field["commit"][:7])}</code>.</p>'
        f'<ul class="facts">{carets}</ul>'
        f'<div class="charts">{charts}</div></section>'
    )


def page(by, verdict, cards, micro=None, startup=None, release=None, field=None):
    heavy = by['heavy']
    env, protocol, commit = heavy['environment'], heavy['protocol'], heavy['commit']
    # Since 2026-10-26 (#637), samples start from the window's readiness,
    # not from launch, and apps run a little longer: runs on either side of
    # that are not ranked against each other.
    ready = 'sampling_anchor' in protocol
    anchor = 'after its window was ready (frontmost and uncovered)' if ready else 'after launch'
    anchor_short = 'after its window was ready' if ready else 'after launch'
    anchor_note = ('<li>Since 2026-10-26 a trial&#8217;s sample starts from its window being ready (frontmost and '
                   'uncovered, polled up to {:g} s), not from its launch, and each app runs {} s longer; this '
                   'run is therefore not ranked against earlier ones, which measured from launch.</li>').format(
        protocol.get('readiness_timeout_seconds', 2.5), protocol.get('lifetime_margin_seconds', 3)) if ready else ''
    date = heavy['started_utc'][:10]
    tools = heavy['toolchains']
    mygo = tools.get('mygo', {})
    rune = tools.get('rune', {})
    e = html.escape
    has_rune = any('rune' in by[n]['frameworks'] for n in ORDER)
    rune_lede = ' and Rune (Rust, wgpu, Taffy and Parley)' if has_rune else ''
    rune_build = (f"; Rune at <code>{e(rune.get('rune_rev', '?')[:8])}</code> ({e(rune.get('rune_branch', '?'))}, "
                  f"{e(rune.get('rune_license', '?'))}), profile {e(rune.get('profile', 'perf'))} (Rune's own bench scripts use perf), and a 14-line patch "
                  f"exposing its wheel path (see below)" if has_rune else '')
    # The window server's colour matching is macOS's.
    rune_notes = ('\n'.join(n for n in RUNE_NOTES.split('\n') if PLATFORM == 'macos' or 'colour-matched' not in n)
                  if has_rune else '')
    displays = env['displays']
    display = (f"{len(displays)} × {displays[0]['name']} ({displays[0]['resolution']})"
               if all(d['name'] == displays[0]['name'] and d['resolution'] == displays[0]['resolution'] for d in displays)
               else ', '.join(f"{d['name']} ({d['resolution']})" for d in displays))
    e = html.escape
    card_html = '\n'.join(f'<div class="card"><dt>{e(label)}</dt><dd><span class="big">{e(big)}</span>'
                          f'<span class="note">{e(note)}</span></dd></div>' for big, label, note in cards)
    sections = []
    for name in ORDER:
        scene = by[name]
        title, note = SCENES[name]
        charts = '\n'.join(chart(scene, m) for m in metrics() if any(metrics()[m][4] in c for c in scene['cells']))
        sections.append(
            f'<section id="{name}" aria-labelledby="{name}-h"><h2 id="{name}-h">{e(title)}</h2>'
            f'<p class="scene-note">{e(note)} <a href="{REPO}/tree/main/{scene["run"]}">Raw data</a></p>'
            f'<div class="charts">{charts}</div></section>')
    runs = ''.join(f'<li><a href="{REPO}/tree/main/{by[n]["run"]}"><code>{by[n]["run"]}</code></a> '
                   f'({by[n]["trials"]} trials, {by[n]["pacing_trials"]} pacing captures)</li>' for n in ORDER)
    luau = 'zgx-luau' in by['vlist']['frameworks']
    # Apps that exited abnormally (each launched once more), against their
    # launches: recorded since the run that started counting them.
    crash_note = ''
    if all(by[n].get('abnormal_exits') is not None for n in ORDER):
        exits, launches = {}, {}
        for n in ORDER:
            for x in by[n]['abnormal_exits']:
                exits[x['framework']] = exits.get(x['framework'], 0) + 1
            for fw, kinds in by[n]['launches'].items():
                launches[fw] = launches.get(fw, 0) + sum(kinds.values())
        if exits:
            listed = ', '.join(f"{NAMES.get(fw, fw)} {count} of {launches.get(fw, 0)} launches" for fw, count in sorted(exits.items()))
            crash_note = (f'<li>Apps that exited abnormally during the run, each then launched once more: {e(listed)}.'
                          + (' GPUI Fast aborts now and then inside AppKit while the window is being captured for frame timing '
                             '(<code>-[NSWindow _updateButtonsForWindowSharingSession]</code>).' if PLATFORM == 'macos' else '')
                          + '</li>')
        else:
            crash_note = '<li>No app exited abnormally during the run.</li>'
    nav = ' '.join(f'<a href="#{n}">{SCENES[n][0]}</a>' for n in ORDER) + (' <a href="#micro">Microbenchmarks</a>' if micro else '') + (
        f' <a href="#release">Since {e(release["gate"]["from"]["version"])}</a>' if release else '') + (
        ' <a href="#field">A focused field</a>' if field else '')
    mygo_build = (f"MyGo {e(mygo.get('mygo_version', '?'))} (<code>{e(mygo.get('mygo_commit', '?')[:7])}</code>, "
                  f"{e(mygo.get('mygo_license', '?'))}) built with {e(mygo.get('go', tools.get('go', 'Go')))}, "
                  f"<code>go build {e(mygo.get('flags', ''))}</code>, cgo {'on' if mygo.get('cgo') else 'off'}") + rune_build
    if PLATFORM == 'windows':
        where, method, caveats = windows_facts(**locals())
    else:
        where = 'one Mac'
        method, caveats = macos_facts(**locals())
    return f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>zgx performance: the latest benchmark</title>
<meta name="description" content="{e(verdict)}">
<meta name="color-scheme" content="light dark">
<link rel="stylesheet" href="{'' if PLATFORM == 'macos' else '../'}style.css">
</head>
<body>
<header>
<p class="eyebrow">zgx performance · {PLATFORM_NAMES[PLATFORM]} · {date}</p>
<h1>{e(verdict)}</h1>
<p class="lede">zgx against GPUI Fast (Zed's GPUI with its fast-path optimizations, as apps ship it) MyGo (a Go desktop framework, through its GPU-drawn native UI){rune_lede}: the same four scenes, built from the same workload code, measured on {where} in one session. Lower is better throughout.</p>
<dl class="cards">
{card_html}
</dl>
<nav aria-label="Scenes">{nav} <a href="#method">How this was measured</a></nav>
</header>
<main>
{''.join(sections)}
{field_section(field, e) if field else ''}
{release_section(release, e) if release else ''}
{startup_section(startup, e) if startup else ''}
{micro_section(micro, e) if micro else ''}
<section id="method" aria-labelledby="method-h">
<h2 id="method-h">How this was measured</h2>
<ul class="facts">
{method}
</ul>
{'<h3>How Rune is driven</h3><ul class="facts">' + rune_notes + '</ul>' if rune_notes else ''}
<h3>Caveats</h3>
<ul class="facts">
{caveats}
</ul>
<h3>Data</h3>
<ul class="facts">
<li>This page's numbers: <a href="data.csv">data.csv</a>, <a href="data.json">data.json</a>{', and the microbenchmarks&#8217; <a href="micro.jsonl">micro.jsonl</a>' if micro else ''}.</li>
<li>Every trial, raw:<ul>{runs}</ul></li>
<li>How the runs are made: <a href="{REPO}/blob/main/docs/benchmarking.md">benchmarking</a>; the adapters: <a href="{REPO}/tree/main/comparisons">comparisons</a>; the code: <a href="{REPO}">github.com/zeronsh/zgx</a>.</li>
</ul>
</section>
</main>
<footer><p>Built by <code>scripts/build_site.py</code> from the accepted run; every number recomputed from its trials.</p></footer>
</body>
</html>
'''

def macos_facts(by, e, env, display, tools, mygo_build, commit, protocol, anchor, anchor_short, anchor_note,
                luau, has_rune, crash_note, heavy, **_):
    """The macOS page's How this was measured and Caveats items."""
    # Where GPU power does not clearly favour zgx: a rival lower, or the
    # trials' ranges overlapping.
    close = [f'{SCENES[n][0].lower()} ({MODES[m].lower()}) against {NAMES[r]}'
             for n in ORDER for m in by[n]['modes'] if m != 'idle' for r in by[n]['frameworks'] if not r.startswith('zgx')
             if cell(by[n], 'zgx', m)['gpu_power_mw']['max'] >= cell(by[n], r, m)['gpu_power_mw']['min']]
    power_note = (f'<li>GPU power is within the trials&#8217; spread, or higher for zgx, in: {e(", ".join(close))}.</li>'
                  if close else '')
    anim = by['animate']
    animate_time = ', '.join(
        f"{percent_less(value(anim, 'zgx', 'animate', 'gpu_mcycles_per_second'), value(anim, r, 'animate', 'gpu_mcycles_per_second'))} than {NAMES[r]}"
        for r in anim['frameworks'] if not r.startswith('zgx'))
    animate_power = ', '.join(
        f"{percent_less(value(anim, 'zgx', 'animate', 'gpu_power_mw'), value(anim, r, 'animate', 'gpu_power_mw'))} than {NAMES[r]}"
        for r in anim['frameworks'] if not r.startswith('zgx'))
    # Where WindowServer works harder for zgx's window than for a rival's.
    active = [(s, m) for s in by.values() for m in s['modes'] if m != 'idle']
    rivals = [f for f in heavy['frameworks'] if not f.startswith('zgx')]
    # How far above the cheapest rival's window zgx's costs WindowServer, per test.
    server_gaps = [value(s, 'zgx', m, 'windowserver_cpu_percent')
                   - min(value(s, r, m, 'windowserver_cpu_percent') for r in rivals if r in s['frameworks'])
                   for s, m in active]
    server_more = sum(gap > 0 for gap in server_gaps)
    server_gap = max(server_gaps)
    method = f'''<li><strong>Machine:</strong> {e(env['chip'])} ({e(env['machine'])}, {env['cores']} cores, {env['memory_bytes'] // 2**30} GB), macOS {e(env['macos'])}, {e(env['power'])}.</li>
<li><strong>Displays:</strong> {e(display)}; the window on the main display, nothing covering it.</li>
<li><strong>Builds:</strong> zgx and GPUI Fast with their <code>dist</code> profile (release, fat LTO, one codegen unit), {e(tools.get('rust', env.get('rust', 'Rust')))}; {mygo_build}. Every framework uses the system allocator. zgx at <a href="{REPO}/commit/{commit}"><code>{commit[:7]}</code></a>.</li>
<li><strong>Trials:</strong> each a fresh launch, the first {protocol['warmup_seconds']} s {anchor} discarded and the next {protocol['sampled_seconds']} s sampled every {int(protocol['interval_seconds'] * 1000)} ms; {protocol['repeats']} trials of each mode, in alternating order, and {protocol['extra_trials']} more of any whose CPU or GPU spread past {protocol['spread_for_extra_trials'] * 100:.0f}% of its median; idle {protocol['idle_repeats']} times in the dashboard, {protocol['idle_window_seconds'][0]}–{protocol['idle_window_seconds'][1]} s {anchor_short}. Every trial had to run as long as asked, at 60 updates a second. Medians are shown, the trials' range beside them. Frame timing is {protocol['pacing_repeats']} separate {protocol['pacing_seconds']} s captures of each mode.</li>
<li><strong>CPU:</strong> the app process's CPU time over the sample, as a percentage of one core (it can exceed 100).</li>
<li><strong>WindowServer CPU:</strong> the CPU of macOS's compositor, which puts every app's frames on screen, over the same sample: what showing the app costs the system beside the app's own CPU. It includes what WindowServer does anyway (the desktop alone, measured each repeat, is beside each chart's title).</li>
<li><strong>GPU power:</strong> the whole GPU's energy over the sample, from the system's energy model (IOReport), in milliwatts, less the desktop alone (measured each repeat; beside each chart's title). It counts everything the GPU does for the app, the window server's compositing of its window included, so a framework is charged for how it presents its frames as well as for drawing them.</li>
<li><strong>App GPU time</strong> (secondary): the GPU time macOS accounts to the app's process, as cycles at the GPU's mean clock. It includes the time from each submission to the GPU starting it, about half a millisecond after the GPU idles, so an app that draws small frames often reads high: measured with Metal System Trace, zgx's animation frames execute in 46 µs against GPUI Fast's 318 µs, yet this counter charges zgx more. It also moves with whatever else keeps the GPU awake. Kept for comparison with earlier runs.</li>
<li><strong>Frame time:</strong> the 99th percentile of the intervals between frames shown on screen, read through ScreenCaptureKit; at 60 updates a second, 16.7 ms is on time (8.3 ms for the animation, drawn at 120 Hz). <strong>Long frames</strong> are intervals over 25 ms.</li>
<li><strong>Unfilled rows</strong> (the list): the window captured 10 times a second for {by['vlist']['protocol'].get('blank_seconds', 8)} s in {' and '.join(MODES[m].lower() for m in by['vlist']['protocol'].get('blank_modes', by['vlist']['modes']))}; a capture counts when a stretch of the rows' padding over 24 points shows the window's background, where no row was drawn.</li>
<li><strong>Memory:</strong> the process's physical footprint (what Activity Monitor shows: resident memory plus GPU memory it owns), averaged over the sample.</li>
<li><strong>The list widgets:</strong> zgx's measured virtual list{', the same list from a Luau script (<code>ui.list { measured = true }</code>),' if luau else ''} and GPUI Fast's <code>list</code> measure each row as they lay it out; MyGo's <code>ui.List</code> keeps its place by an anchor row and estimates unmeasured rows at the average height{"; Rune's virtual rows estimate unmeasured rows (28 pt fixed, a three-line row variable) and measure as they mount" if has_rune else ''}.</li>'''
    caveats = f'''{anchor_note}
<li>Frame time and frames shown are measured while the window is captured, and capturing a window changes its pacing: macOS skips frames it would otherwise show, more for some ways of presenting than others (zgx at 120 Hz, by the system&#8217;s own count: 0.2% skipped uncaptured on M1 Max and M5, 12.2% and 3.7% captured). They show pacing under a capture (screen sharing, recording), compared under the same capture; CPU, GPU power, memory and WindowServer CPU come from trials with nothing attached. See <a href="{REPO}/blob/main/docs/benchmarking.md#frame-pacing-a-capture-changes-it">the benchmarking notes</a>.</li>
<li>Compare within this run only: the same binaries measured hours apart read differently (the machine's state changes), so a cell is not comparable across runs, while the frameworks in one run share its session.</li>
<li>Idle memory is two-level: an idle window settles at one of two levels about 25 MB apart, launch to launch, as the Metal driver returns its startup memory. Which level a launch lands on is the driver's timing, not the app's, so read idle memory with its range.</li>
<li>MyGo is immediate mode: each frame it draws builds and draws its whole window, which is how MyGo apps work.</li>
{'' if luau else '<li>zgx through Luau is not in the list scene: zgx-script&#8217;s <code>ui.list</code> does not measure its rows yet.</li>'}
{f'<li>WindowServer spends more CPU on zgx&#8217;s window than on the cheapest rival&#8217;s in {server_more} of {len(active)} tests, by at most {fmt(server_gap, 1)} points of a core (its chart follows CPU in each scene).</li>' if server_more else ''}
{power_note}
<li>Until 2026-10-25 these pages' GPU figure was app GPU time; it is now GPU power, which measures what the GPU spends rather than how long it was held (see How this was measured). In the animation, zgx uses {animate_time} on app GPU time and {animate_power} on power.</li>
{crash_note}
<li>Binary size is not compared.</li>
<li>One machine, one session; a personal Mac in use, other applications kept running (the run waited until the machine was quiet). Windows and Linux are not compared here.</li>'''
    return method, caveats


def windows_facts(by, e, env, display, tools, mygo_build, commit, protocol, anchor, anchor_short, anchor_note,
                  luau, has_rune, crash_note, heavy, **_):
    """The Windows page's machine, How this was measured and Caveats items."""
    active = [(s, m) for s in by.values() for m in s['modes'] if m != 'idle']
    rivals = [f for f in heavy['frameworks'] if not f.startswith('zgx')]
    # How far above the cheapest rival's window zgx's costs DWM, per test.
    server_gaps = [value(s, 'zgx', m, 'dwm_mcycles_per_second')
                   - min(value(s, r, m, 'dwm_mcycles_per_second') for r in rivals if r in s['frameworks'])
                   for s, m in active]
    server_more = sum(gap > 0 for gap in server_gaps)
    hz = sorted({d['resolution'].rsplit('@ ', 1)[-1] for d in env['displays'] if d['main']})
    board = ('<li><strong>Board power:</strong> the NVIDIA board&#8217;s energy over the sample (NVML&#8217;s total energy '
             'counter, read at the sample&#8217;s two ends), in watts, less the desktop alone (beside each chart&#8217;s title). '
             'It is the whole board, compositing included, as macOS&#8217;s GPU power is the whole GPU&#8217;s.</li>' if BOARD_POWER else '')
    ratios = worst_ratios(by)
    short = {'cpu': ' M cycles/s', 'cycles': ' M cycles', 'server': ' M cycles/s', 'gpu_time': ' ms/s', 'dwm_gpu': ' ms/s',
             'board': ' W', 'p99': ' ms', 'long': '%', 'memory': ' MB'}

    def shown(metric, scene, framework, mode):
        _, _, digits, scale, key, _ = metrics()[metric]
        return fmt(value(by[scene], framework, mode, key) * scale, digits) + short[metric]

    ratio_items = ''.join(
        f"<li>{e(metrics()[m][0])}: {r['ratio']:.2f}× ({e(SCENES[r['scene']][0].lower())}, {e(MODES[r['mode']].lower())}: "
        f"{shown(m, r['scene'], 'zgx', r['mode'])} against {e(NAMES[r['rival']])}&#8217;s {shown(m, r['scene'], r['rival'], r['mode'])})</li>"
        for m, r in ratios.items())
    graphics = {}
    for scene in by.values():
        for framework, found in scene['graphics_modules'].items():
            graphics.setdefault(framework, set()).update(found)
    apis = {'d3d11.dll': 'Direct3D 11', 'd3d12.dll': 'Direct3D 12', 'vulkan-1.dll': 'Vulkan', 'opengl32.dll': 'OpenGL'}
    graphics_note = '; '.join(f"{NAMES[f]} {' and '.join(apis.get(m, m) for m in sorted(found))}"
                              for f, found in graphics.items() if found)
    hosts = env.get('remote_display_hosts_running') or []
    where = 'one Windows PC'
    method = f'''<li><strong>Machine:</strong> {e(env['chip'])} ({env['cores']} logical processors, {env['memory_bytes'] // 2**30} GB), {e(env['gpu'])} (driver {e(env['gpu_driver'])}), Windows {e(env['windows'])}, {e(env['power'])}, the High performance power plan held for the run.</li>
<li><strong>Displays:</strong> {e(display)}; the window on the main display, brought to the front, nothing covering it.</li>
<li><strong>Builds:</strong> zgx and GPUI Fast with their <code>dist</code> profile (release, fat LTO, one codegen unit), {e(tools.get('rust', env.get('rust', 'Rust')))}; {mygo_build}. Every framework uses the system allocator. zgx at <a href="{REPO}/commit/{commit}"><code>{commit[:7]}</code></a>.</li>
<li><strong>Trials:</strong> each a fresh launch, the first {protocol['warmup_seconds']} s {anchor} discarded and the next {protocol['sampled_seconds']} s sampled; {protocol['repeats']} trials of each mode, in alternating order, and {protocol['extra_trials']} more of any whose CPU cycles or app GPU time spread past {protocol['spread_for_extra_trials'] * 100:.0f}% of its median; idle {protocol['idle_repeats']} times in the dashboard, {protocol['idle_window_seconds'][0]}–{protocol['idle_window_seconds'][1]} s {anchor_short}. Every trial had to run as long as asked, at 60 updates a second, its window uncovered as its sample began and ended. Medians are shown, the trials' range beside them. Frame timing is {protocol['pacing_repeats']} separate {protocol['pacing_seconds']} s captures of each mode.</li>
<li><strong>CPU:</strong> the app process&#8217;s CPU cycles over the sample (<code>QueryProcessCycleTime</code>), in millions a second. Windows counts CPU time in scheduler ticks of 15.6 ms, too coarse for these loads (one app&#8217;s same animation read 0.9% and 0.3% of a core in two trials of equal cycles), so cycles are the figure; CPU time stays in the raw data.</li>
<li><strong>CPU cycles per frame:</strong> the same cycles per update, or in the animation per frame drawn: what each frame costs, whatever the frame rate.</li>
<li><strong>DWM CPU:</strong> the CPU cycles of the Desktop Window Manager (<code>dwm.exe</code>), Windows&#8217; compositor, over the same sample, in millions a second: what showing the app costs the system beside the app&#8217;s own CPU. Read from the system&#8217;s process list (<code>SystemProcessInformation</code>&#8217;s cycle time), which needs no administrator rights where <code>QueryProcessCycleTime</code> needs a handle DWM gives no other user. It includes what DWM does anyway (the desktop alone, measured each repeat, is beside each chart&#8217;s title).</li>
<li><strong>App GPU time:</strong> the time the GPU&#8217;s 3D engines ran the app&#8217;s work, from Windows&#8217; GPU engine counters, in milliseconds a second. Windows reads no system GPU power, so this is the GPU figure here: how long the GPU was held, not what it spent.</li>
<li><strong>DWM GPU time:</strong> the compositor&#8217;s time on the GPU&#8217;s 3D engines over the same sample: the GPU cost of putting the app&#8217;s frames on screen, plus what DWM does anyway (the desktop alone is beside each chart&#8217;s title).</li>
{board}<li><strong>Frame time:</strong> the 99th percentile of the intervals between frames shown on screen, read through Desktop Duplication (each frame the compositor put on screen in which the window&#8217;s pixels changed); at 60 updates a second, 16.7 ms is on time. <strong>Long frames</strong> are intervals over 25 ms.</li>
<li><strong>Memory:</strong> the process&#8217;s private bytes (the memory it committed: Task Manager&#8217;s commit size), averaged over the sample.</li>
<li><strong>The list widgets:</strong> zgx's measured virtual list{', the same list from a Luau script (<code>ui.list { measured = true }</code>),' if luau else ''} and GPUI Fast's <code>list</code> measure each row as they lay it out; MyGo's <code>ui.List</code> keeps its place by an anchor row and estimates unmeasured rows at the average height{"; Rune's virtual rows estimate unmeasured rows (28 pt fixed, a three-line row variable) and measure as they mount" if has_rune else ''}.</li>'''
    caveats = f'''{anchor_note}
<li>Compare within this run only: the same binaries measured hours apart read differently (the machine's state changes), so a cell is not comparable across runs, while the frameworks in one run share its session.</li>
<li>The display refreshes at {e(', '.join(hz))}: the animation is drawn at that rate here, so its figures are not comparable with a page measured at another.</li>
<li>Where zgx is furthest behind the best rival in each metric (zgx&#8217;s median over the lowest rival&#8217;s, in its worst test; under 1 is less than every rival there):<ul>{ratio_items}</ul></li>
<li>MyGo is immediate mode: each frame it draws builds and draws its whole window, which is how MyGo apps work.</li>
{'' if luau else '<li>zgx through Luau is not in the list scene.</li>'}
{f'<li>DWM spends more CPU on zgx&#8217;s window than on the cheapest rival&#8217;s in {server_more} of {len(active)} tests, by at most {fmt(max(server_gaps), 1)} M cycles a second (its chart follows CPU in each scene).</li>' if server_more else ''}
<li>Memory includes the graphics driver&#8217;s own floor, which differs by API. The processes had these loaded: {e(graphics_note)}.</li>
<li>Until 2026-10-27 this page&#8217;s DWM CPU was DWM&#8217;s CPU time from performance counters, in % of a core. Windows counts that time in 15.6 ms scheduler ticks, so its trials spread widely and its ratios moved with the rivals&#8217; noise. It is now DWM&#8217;s CPU cycles, in millions a second (see How this was measured); the tick-counted figure stays in the raw data.</li>
{f'<li>A remote display host ({e(", ".join(hosts))}) streamed the desktop throughout, capturing and encoding the screen: work beside every framework&#8217;s, in the desktop baseline too, which is why the run accepted up to {e(fmt(heavy["preflight"]["threshold_percent"], 0))}% of a core of other use.</li>' if hosts and heavy.get('preflight') else ''}
<li>The list&#8217;s fling ends slowly, under a pixel a refresh for its last ticks (160 px a tick decaying to 0.6). zgx scrolls in whole device pixels, as Chrome and Windows&#8217; own lists do, to keep text crisp, so in that tail some refreshes move nothing on screen and the capture counts them as long frames: zgx presented on every refresh, its GPU work done within 0.2 ms of each present. Scrolled at a steady 12 or 3 px a tick it shows none; at 0.6 px a tick, 36 frames a second of whole-pixel steps. Rune draws at fractional positions, which change its pixels on every tick.</li>
<li>The list scene&#8217;s unfilled-row captures are macOS&#8217;s only.</li>
{crash_note}
<li>Binary size is not compared.</li>
<li>One machine, one session; a personal PC in use, other applications kept running (the run waited until the machine was quiet, Zeron&#8217;s windows minimized). macOS is compared on its own page; Linux is not compared here.</li>'''
    return where, method, caveats


def worst_ratios(by):
    """Per metric, the test where zgx's median stands highest over the
    lowest rival's (lower is better throughout): the ratio, the test and
    that rival."""
    worst = {}
    for metric, (_, _, _, _, key, applies) in metrics().items():
        for scene in by.values():
            rivals = [f for f in scene['frameworks'] if not f.startswith('zgx')]
            for mode in scene['modes']:
                if (applies != 'all' and mode == 'idle') or key not in cell(scene, 'zgx', mode):
                    continue
                best = min(rivals, key=lambda r: value(scene, r, mode, key))
                if value(scene, best, mode, key) <= 0:
                    continue
                ratio = value(scene, 'zgx', mode, key) / value(scene, best, mode, key)
                if metric not in worst or ratio > worst[metric]['ratio']:
                    worst[metric] = {'ratio': ratio, 'scene': scene['scene'], 'mode': mode, 'rival': best}
    return worst


def main():
    global PLATFORM, BOARD_POWER
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('suite', type=Path, help='docs/results/suite-<platform>-<date>-<commit>')
    parser.add_argument('--platform', choices=sorted(PLATFORM_NAMES), default='macos')
    parser.add_argument('--output', type=Path, help='default site/ (macOS), site/windows/ (Windows)')
    parser.add_argument('--micro', type=Path, help='the microbenchmarks\' run (micro.jsonl), shown in its own section')
    parser.add_argument('--startup', type=Path, help='a startup run (docs/results/startup-macos-*/startup.json), shown in its own section')
    parser.add_argument('--gate', type=Path, help='the instruction gate\'s counts for two releases (from, to, scenes: name → [from, to]), shown with --previous')
    parser.add_argument('--previous', type=Path, help='the last published suite run, compared with this one per cell, for zgx')
    parser.add_argument('--field', type=Path, help='a run of the field scene (a focused field left alone), shown in its own section')
    parser.add_argument('--rebuilt', help='which rival builds differ between the two runs (said beside the comparison)')
    parser.add_argument('--board-power', action='store_true',
                        help='Windows: publish the NVIDIA board\'s power (only where it held still across repeats)')
    args = parser.parse_args()
    PLATFORM, BOARD_POWER = args.platform, args.board_power
    args.output = args.output or ROOT / ('site' if PLATFORM == 'macos' else f'site/{PLATFORM}')
    suite = args.suite.resolve()
    by = {name: load(suite / name) for name in ORDER}
    assert len({s['commit'] for s in by.values()}) == 1, 'every scene measured on one commit'
    verdict, cards = headline(by)
    data = {
        'verdict': verdict,
        'commit': by['heavy']['commit'],
        # Which suite: the website shows each platform's in its own column.
        'platform': PLATFORM,
        # Samples from window readiness (#637, 2026-10-26) or from launch:
        # runs with different anchors are not ranked against each other.
        'sampling_anchor': by['heavy']['protocol'].get('sampling_anchor', 'launch'),
        'frameworks': NAMES,
        'metrics': {m: {'title': t, 'unit': u, 'source': k} for m, (t, u, _, _, k, _) in metrics().items()},
        'scenes': [{**{k: by[n][k] for k in ('scene', 'run', 'started_utc', 'frameworks', 'modes', 'trials',
                                             'pacing_trials', 'protocol', 'environment', 'toolchains', 'cells')},
                    **({'idle_server': by[n]['idle_server']} if 'idle_server' in by[n] else {})}
                   for n in ORDER],
        # Per metric, zgx's median over the lowest rival's in its worst test.
        **({'worst_ratios': worst_ratios(by)} if PLATFORM == 'windows' else {}),
    }
    startup = json.loads(args.startup.read_text()) if args.startup else None
    if startup:
        # Medians and ranges only; every launch stays in the run's file.
        data['startup'] = {k: startup[k] for k in ('commit', 'machine', 'probe', 'apps', 'protocol', 'medians')}
    release = None
    if args.gate and args.previous:
        previous = {name: load(args.previous.resolve() / name) for name in ORDER}
        release = {'gate': json.loads(args.gate.read_text()), 'previous_commit': previous['heavy']['commit'],
                   'previous_date': previous['heavy']['started_utc'][:10], 'rebuilt': args.rebuilt,
                   'modes': {n: by[n]['modes'] for n in ORDER}, 'changes': release_changes(previous, by)}
        data['release'] = {k: release[k] for k in ('gate', 'previous_commit', 'previous_date', 'rebuilt', 'changes')}
    field = load(args.field.resolve()) if args.field else None
    if field:
        data['field'] = {k: field[k] for k in ('scene', 'run', 'commit', 'started_utc', 'frameworks', 'modes', 'trials',
                                               'protocol', 'environment', 'toolchains', 'cells')}
    keys = (WINDOWS_TRIAL_KEYS + (['gpu_board_power_w'] if BOARD_POWER else []) if PLATFORM == 'windows'
            else TRIAL_KEYS) + PACING_KEYS + BLANK_KEYS
    args.output.mkdir(parents=True, exist_ok=True)
    # UTF-8 and \n wherever it is built (Windows' default is neither).
    (args.output / 'data.json').write_text(json.dumps(data, indent=1) + '\n', encoding='utf-8', newline='\n')
    with (args.output / 'data.csv').open('w', newline='') as out:
        writer = csv.writer(out, lineterminator='\n')
        writer.writerow(['scene', 'mode', 'framework', 'metric', 'median', 'min', 'max', 'trials'])
        for n in ORDER:
            for c in by[n]['cells']:
                for key in keys:
                    if key in c:
                        trials = c['pacing_captures'] if key in PACING_KEYS else 1 if key in BLANK_KEYS else c['repeats']
                        writer.writerow([n, c['mode'], c['framework'], key, c[key]['median'], c[key]['min'], c[key]['max'], trials])
    micro = load_micro(args.micro) if args.micro else None
    if micro:
        shutil.copy(args.micro, args.output / 'micro.jsonl')
    (args.output / 'index.html').write_text(page(by, verdict, cards, micro, startup, release, field), encoding='utf-8', newline='\n')
    counts = ', '.join(f"{n} ({by[n]['trials']} trials)" for n in ORDER)
    print(f"Published {counts} at {by['heavy']['commit'][:7]} to {args.output}")


if __name__ == '__main__':
    main()
