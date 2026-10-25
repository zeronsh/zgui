#!/usr/bin/env python3
"""The public performance page (site/): one page of the latest accepted macOS
suite run (scripts/run_macos_suite.py), every scene of it.

    python3 scripts/build_site.py docs/results/suite-macos-<date>-<commit>

Only an accepted suite run of one clean commit is published, every scene of
it accepted. Every median and range shown is recomputed from the scenes'
trials (current.csv, pacing.csv, blank.csv) and checked against their
summaries. The headline says only what the numbers show: its verdict and
figures are worked out from them, never written in. Writes site/index.html
(charts as text and CSS, no script), site/data.json and site/data.csv.
"""
import argparse, csv, html, json, math, statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = 'https://github.com/zeronsh/zgx'
NAMES = {'zgx': 'zgx', 'zgx-luau': 'zgx (Luau)', 'gpui-fast': 'GPUI Fast', 'mygo': 'MyGo', 'rune': 'Rune'}
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
    frameworks, modes = protocol['frameworks'], protocol['modes']
    trials = list(csv.DictReader((directory / 'current.csv').open()))
    paced = list(csv.DictReader((directory / 'pacing.csv').open()))
    blanks = list(csv.DictReader((directory / 'blank.csv').open())) if (directory / 'blank.csv').exists() else []
    extended = set(protocol.get('extended_cells') or [])
    # The whole GPU's power with no app running: each repeat's desktop baseline.
    desktop_gpu = statistics.median(float(t['gpu_system_watts']) for t in trials if t['framework'] == 'desktop')
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
    return {
        'desktop_server': statistics.median(desktop), 'desktop_gpu_mw': desktop_gpu * 1000,
        'scene': audit['scene'], 'run': str(directory.relative_to(ROOT)), 'commit': audit['commit'],
        'started_utc': audit['started_utc'], 'frameworks': frameworks, 'modes': modes, 'cells': cells,
        'trials': audit['trials'], 'pacing_trials': audit['pacing_trials'], 'protocol': protocol,
        'environment': audit['environment'], 'binaries': audit['binaries'],
        'toolchains': audit.get('toolchains', {}), 'preflight': audit.get('preflight'),
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
    named = ' and '.join(NAMES[r] for r in rivals)
    active = [(s, m) for s in by.values() for m in s['modes'] if m != 'idle']
    led = {key: sum(all(value(s, 'zgx', m, key) < value(s, r, m, key) for r in rivals if r in s['frameworks'])
                    for s, m in active)
           for key in ('cpu_percent_one_core', 'gpu_power_mw')}
    tests = len(active)
    long_zgx = max(value(s, 'zgx', m, 'long_interval_fraction') for s, m in active)
    if led['cpu_percent_one_core'] == tests and led['gpu_power_mw'] == tests:
        verdict = f'In every test, zgx uses less CPU and less GPU power than {named}'
    else:
        verdict = (f'zgx uses the least CPU in {led["cpu_percent_one_core"]} of {tests} tests and the least GPU power '
                   f'in {led["gpu_power_mw"]}, against {named}')
    verdict += ', and none of its frames run long.' if long_zgx == 0 else '.'
    heavy = by['heavy']
    against = lambda key, mode: ', '.join(
        f'{percent_less(value(heavy, "zgx", mode, key), value(heavy, r, mode, key))} than {NAMES[r]}' for r in rivals)
    long_rivals = max(value(s, r, m, 'long_interval_fraction') for s, m in active for r in rivals if r in s['frameworks'])
    idle = {f: value(heavy, f, 'idle', 'cpu_percent_one_core') for f in ['zgx', *rivals]}
    cards = [
        (f"{fmt(value(heavy, 'zgx', 'both', 'cpu_percent_one_core'), 1)}%", 'CPU, heavy dashboard (both)',
         against('cpu_percent_one_core', 'both')),
        (f"{fmt(value(heavy, 'zgx', 'both', 'gpu_power_mw'), 0)} mW", 'GPU power, heavy dashboard (both)',
         against('gpu_power_mw', 'both')),
        (f'{long_zgx * 100:.0f}%' if long_zgx == 0 else f'{long_zgx * 100:.1f}%', 'long frames (over 25 ms), at most',
         f'median of each mode\'s captures, every scene; {named}: up to {long_rivals * 100:.1f}%'),
        (f"{fmt(idle['zgx'], 2)}%", 'CPU while idle', ', '.join(f"{NAMES[r]} {fmt(idle[r], 2)}%" for r in rivals)),
    ]
    return verdict, cards


def chart(scene, metric):
    title, unit, digits, scale, key, applies = METRICS[metric]
    if metric == 'server':
        unit += f' (the desktop alone: {fmt(scene["desktop_server"], 1)})'
    if metric == 'gpu':
        unit += f' ({fmt(scene["desktop_gpu_mw"], 1)} mW)'
    modes = [m for m in scene['modes'] if applies == 'all' or m != 'idle']
    rows = [(m, f, c[key]) for m in modes for f in scene['frameworks']
            for c in [cell(scene, f, m)] if key in c]
    top = max(s['median'] * scale for _, _, s in rows) or 1
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
            width = 100 * median / top
            # Idle CPU is a fraction of a percent: two places show it.
            places = 2 if (metric, mode) == ('cpu', 'idle') else digits
            head = f'<th scope="rowgroup" rowspan="{len(group)}" class="mode">{MODES[mode]}</th>' if i == 0 else ''
            low, high = fmt(low, places), fmt(high, places)
            spread_text = '' if low == high else f' <span class="range">({low}–{high})</span>'
            out.append(f'<tr class="fw-{framework}">{head}<th scope="row">{NAMES[framework]}</th>'
                       f'<td><span class="track" aria-hidden="true"><span class="bar" style="width:{width:.1f}%"></span></span>'
                       f'<span class="value">{fmt(median, places)}</span>{spread_text}</td></tr>')
        out.append('</tbody>')
    out.append('</table></figure>')
    return '\n'.join(out)


def page(by, verdict, cards):
    heavy = by['heavy']
    env, protocol, commit = heavy['environment'], heavy['protocol'], heavy['commit']
    date = heavy['started_utc'][:10]
    tools = heavy['toolchains']
    mygo = tools.get('mygo', {})
    rune = tools.get('rune', {})
    has_rune = any('rune' in by[n]['frameworks'] for n in ORDER)
    rune_lede = ' and Rune (Rust, wgpu, Taffy and Parley)' if has_rune else ''
    rune_build = (f"; Rune at <code>{e(rune.get('rune_rev', '?')[:8])}</code> ({e(rune.get('rune_branch', '?'))}, "
                  f"{e(rune.get('rune_license', '?'))}), profile {e(rune.get('profile', 'perf'))} (Rune's own bench scripts use perf), and a 14-line patch "
                  f"exposing its wheel path (see below)" if has_rune else '')
    rune_notes = RUNE_NOTES if has_rune else ''
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
        charts = '\n'.join(chart(scene, m) for m in METRICS if any(METRICS[m][4] in c for c in scene['cells']))
        sections.append(
            f'<section id="{name}" aria-labelledby="{name}-h"><h2 id="{name}-h">{e(title)}</h2>'
            f'<p class="scene-note">{e(note)} <a href="{REPO}/tree/main/{scene["run"]}">Raw data</a></p>'
            f'<div class="charts">{charts}</div></section>')
    runs = ''.join(f'<li><a href="{REPO}/tree/main/{by[n]["run"]}"><code>{by[n]["run"]}</code></a> '
                   f'({by[n]["trials"]} trials, {by[n]["pacing_trials"]} pacing captures)</li>' for n in ORDER)
    luau = 'zgx-luau' in by['vlist']['frameworks']
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
    server_more = sum(any(value(s, 'zgx', m, 'windowserver_cpu_percent') > value(s, r, m, 'windowserver_cpu_percent')
                          for r in rivals if r in s['frameworks']) for s, m in active)
    nav = ' '.join(f'<a href="#{n}">{SCENES[n][0]}</a>' for n in ORDER)
    mygo_build = (f"MyGo {e(mygo.get('mygo_version', '?'))} (<code>{e(mygo.get('mygo_commit', '?')[:7])}</code>, "
                  f"{e(mygo.get('mygo_license', '?'))}) built with {e(mygo.get('go', tools.get('go', 'Go')))}, "
                  f"<code>go build {e(mygo.get('flags', ''))}</code>, cgo {'on' if mygo.get('cgo') else 'off'}") + rune_build
    return f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>zgx performance: the latest benchmark</title>
<meta name="description" content="{e(verdict)}">
<meta name="color-scheme" content="light dark">
<link rel="stylesheet" href="style.css">
</head>
<body>
<header>
<p class="eyebrow">zgx performance · macOS · {date}</p>
<h1>{e(verdict)}</h1>
<p class="lede">zgx against GPUI Fast (Zed's GPUI with its fast-path optimizations, as apps ship it) MyGo (a Go desktop framework, through its GPU-drawn native UI){rune_lede}: the same four scenes, built from the same workload code, measured on one Mac in one session. Lower is better throughout.</p>
<dl class="cards">
{card_html}
</dl>
<nav aria-label="Scenes">{nav} <a href="#method">How this was measured</a></nav>
</header>
<main>
{''.join(sections)}
<section id="method" aria-labelledby="method-h">
<h2 id="method-h">How this was measured</h2>
<ul class="facts">
<li><strong>Machine:</strong> {e(env['chip'])} ({e(env['machine'])}, {env['cores']} cores, {env['memory_bytes'] // 2**30} GB), macOS {e(env['macos'])}, {e(env['power'])}.</li>
<li><strong>Displays:</strong> {e(display)}; the window on the main display, nothing covering it.</li>
<li><strong>Builds:</strong> zgx and GPUI Fast with their <code>dist</code> profile (release, fat LTO, one codegen unit), {e(tools.get('rust', env.get('rust', 'Rust')))}; {mygo_build}. Every framework uses the system allocator. zgx at <a href="{REPO}/commit/{commit}"><code>{commit[:7]}</code></a>.</li>
<li><strong>Trials:</strong> each a fresh launch, the first {protocol['warmup_seconds']} s discarded and the next {protocol['sampled_seconds']} s sampled every {int(protocol['interval_seconds'] * 1000)} ms; {protocol['repeats']} trials of each mode, in alternating order, and {protocol['extra_trials']} more of any whose CPU or GPU spread past {protocol['spread_for_extra_trials'] * 100:.0f}% of its median; idle {protocol['idle_repeats']} times in the dashboard, {protocol['idle_window_seconds'][0]}–{protocol['idle_window_seconds'][1]} s after launch. Every trial had to run as long as asked, at 60 updates a second. Medians are shown, the trials' range beside them. Frame timing is {protocol['pacing_repeats']} separate {protocol['pacing_seconds']} s captures of each mode.</li>
<li><strong>CPU:</strong> the app process's CPU time over the sample, as a percentage of one core (it can exceed 100).</li>
<li><strong>WindowServer CPU:</strong> the CPU of macOS's compositor, which puts every app's frames on screen, over the same sample: what showing the app costs the system beside the app's own CPU. It includes what WindowServer does anyway (the desktop alone, measured each repeat, is beside each chart's title).</li>
<li><strong>GPU power:</strong> the whole GPU's energy over the sample, from the system's energy model (IOReport), in milliwatts, less the desktop alone (measured each repeat; beside each chart's title). It counts everything the GPU does for the app, the window server's compositing of its window included, so a framework is charged for how it presents its frames as well as for drawing them.</li>
<li><strong>App GPU time</strong> (secondary): the GPU time macOS accounts to the app's process, as cycles at the GPU's mean clock. It includes the time from each submission to the GPU starting it, about half a millisecond after the GPU idles, so an app that draws small frames often reads high: measured with Metal System Trace, zgx's animation frames execute in 46 µs against GPUI Fast's 318 µs, yet this counter charges zgx more. It also moves with whatever else keeps the GPU awake. Kept for comparison with earlier runs.</li>
<li><strong>Frame time:</strong> the 99th percentile of the intervals between frames shown on screen; at 60 updates a second, 16.7 ms is on time (8.3 ms for the animation, drawn at 120 Hz). <strong>Long frames</strong> are intervals over 25 ms.</li>
<li><strong>Unfilled rows</strong> (the list): the window captured 10 times a second for {by['vlist']['protocol'].get('blank_seconds', 8)} s per mode; a capture counts when a stretch of the rows' padding over 24 points shows the window's background, where no row was drawn.</li>
<li><strong>Memory:</strong> the process's physical footprint (what Activity Monitor shows: resident memory plus GPU memory it owns), averaged over the sample.</li>
<li><strong>The list widgets:</strong> zgx's measured virtual list{', the same list from a Luau script (<code>ui.list {{ measured = true }}</code>),' if luau else ''} and GPUI Fast's <code>list</code> measure each row as they lay it out; MyGo's <code>ui.List</code> keeps its place by an anchor row and estimates unmeasured rows at the average height{"; Rune's virtual rows estimate unmeasured rows (28 pt fixed, a three-line row variable) and measure as they mount" if has_rune else ''}.</li>
</ul>
{'<h3>How Rune is driven</h3><ul class="facts">' + rune_notes + '</ul>' if rune_notes else ''}
<h3>Caveats</h3>
<ul class="facts">
<li>Compare within this run only: the same binaries measured hours apart read differently (the machine's state changes), so a cell is not comparable across runs, while the frameworks in one run share its session.</li>
<li>Idle memory is two-level: an idle window settles at one of two levels about 25 MB apart, launch to launch, as the Metal driver returns its startup memory. Which level a launch lands on is the driver's timing, not the app's, so read idle memory with its range.</li>
<li>MyGo is immediate mode: each frame it draws builds and draws its whole window, which is how MyGo apps work.</li>
{'' if luau else '<li>zgx through Luau is not in the list scene: zgx-script&#8217;s <code>ui.list</code> does not measure its rows yet.</li>'}
{f'<li>WindowServer spends more CPU on zgx&#8217;s window than on a rival&#8217;s in {server_more} of {len(active)} tests (its chart follows CPU in each scene): showing zgx&#8217;s frames costs the system more than the others&#8217;, a cost zgx has yet to bring down.</li>' if server_more else ''}
{power_note}
<li>Until 2026-10-25 these pages' GPU figure was app GPU time; it is now GPU power, which measures what the GPU spends rather than how long it was held (see How this was measured). In the animation, zgx uses {animate_time} on app GPU time and {animate_power} on power.</li>
<li>Binary size is not compared.</li>
<li>One machine, one session; a personal Mac in use, other applications kept running (the run waited until the machine was quiet). Windows and Linux are not compared here.</li>
</ul>
<h3>Data</h3>
<ul class="facts">
<li>This page's numbers: <a href="data.csv">data.csv</a>, <a href="data.json">data.json</a>.</li>
<li>Every trial, raw:<ul>{runs}</ul></li>
<li>How the runs are made: <a href="{REPO}/blob/main/docs/benchmarking.md">benchmarking</a>; the adapters: <a href="{REPO}/tree/main/comparisons">comparisons</a>; the code: <a href="{REPO}">github.com/zeronsh/zgx</a>.</li>
</ul>
</section>
</main>
<footer><p>Built by <code>scripts/build_site.py</code> from the accepted run; every number recomputed from its trials.</p></footer>
</body>
</html>
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('suite', type=Path, help='docs/results/suite-macos-<date>-<commit>')
    parser.add_argument('--output', type=Path, default=ROOT / 'site')
    args = parser.parse_args()
    suite = args.suite.resolve()
    by = {name: load(suite / name) for name in ORDER}
    assert len({s['commit'] for s in by.values()}) == 1, 'every scene measured on one commit'
    verdict, cards = headline(by)
    data = {
        'verdict': verdict,
        'commit': by['heavy']['commit'],
        'frameworks': NAMES,
        'metrics': {m: {'title': t, 'unit': u, 'source': k} for m, (t, u, _, _, k, _) in METRICS.items()},
        'scenes': [{k: by[n][k] for k in ('scene', 'run', 'started_utc', 'frameworks', 'modes', 'trials',
                                          'pacing_trials', 'protocol', 'environment', 'toolchains', 'cells')}
                   for n in ORDER],
    }
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'data.json').write_text(json.dumps(data, indent=1) + '\n')
    with (args.output / 'data.csv').open('w', newline='') as out:
        writer = csv.writer(out, lineterminator='\n')
        writer.writerow(['scene', 'mode', 'framework', 'metric', 'median', 'min', 'max', 'trials'])
        for n in ORDER:
            for c in by[n]['cells']:
                for key in TRIAL_KEYS + PACING_KEYS + BLANK_KEYS:
                    if key in c:
                        trials = c['pacing_captures'] if key in PACING_KEYS else 1 if key in BLANK_KEYS else c['repeats']
                        writer.writerow([n, c['mode'], c['framework'], key, c[key]['median'], c[key]['min'], c[key]['max'], trials])
    (args.output / 'index.html').write_text(page(by, verdict, cards))
    counts = ', '.join(f"{n} ({by[n]['trials']} trials)" for n in ORDER)
    print(f"Published {counts} at {by['heavy']['commit'][:7]} to {args.output}")


if __name__ == '__main__':
    main()
