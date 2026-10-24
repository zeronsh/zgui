#!/usr/bin/env python3
"""Publish accepted macOS comparisons (scripts/run_macos_comparison.py) of
every scene to the results site's comparison page, site/compare.

    python3 scripts/build_compare_site.py \\
        docs/results/latest-macos-<date>-<commit> \\
        docs/results/latest-macos-heavy-<date>-<commit> \\
        docs/results/latest-macos-animate-<date>-<commit>

Only accepted runs are published: their medians and ranges (as the runner
summarized every trial), the frame pacing trials, GPU execution traced by
scripts/trace_macos_gpu.py if it ran, and the run's identity.
"""
import argparse, json
from pathlib import Path

NAMES = {'zgx': 'zgx', 'gpui': 'GPUI', 'gpui-fast': 'GPUI Fast', 'quickgui': 'QuickGUI'}
METRICS = ['cpu_percent_one_core', 'gpu_mcycles_per_second', 'gpu_percent', 'mean_footprint_bytes',
           'peak_footprint_bytes', 'reported_fps', 'windowserver_cpu_percent', 'cpu_watts']
PACING = ['frames_per_second', 'interval_p50_ms', 'interval_p95_ms', 'interval_p99_ms', 'long_interval_fraction']
TRACE = ['gpu_busy_ms_per_second', 'command_buffers_per_second', 'gpu_busy_us_per_command_buffer']

p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
p.add_argument('results', type=Path, nargs='+')
p.add_argument('--output', type=Path, default=Path('site/compare'))
a = p.parse_args()

scenes = []
for directory in a.results:
    summary = json.loads((directory / 'summary.json').read_text())
    audit = json.loads((directory / 'audit.json').read_text())
    assert audit['accepted'], f'{directory} was not accepted; only accepted runs are published'
    groups = [g for g in summary['groups'] if g['framework'] in NAMES]
    frameworks = [f for f in NAMES if any(g['framework'] == f for g in groups)]
    modes = list(dict.fromkeys(g['mode'] for g in groups))
    assert not audit['protocol'].get('quick'), f'{directory} is a quick run, not for publishing'
    idle = audit['protocol'].get('idle_repeats', 3)
    assert all(g['repeats'] == (idle if g['mode'] == 'idle' else 3) for g in groups)
    scenes.append({
        'scene': audit['scene'],
        'frameworks': frameworks,
        'modes': modes,
        'groups': [{'framework': g['framework'], 'mode': g['mode'],
                    **{k: g[k] for k in METRICS if g.get(k)}} for g in groups],
        'pacing': [{'framework': g['framework'], 'mode': g['mode'],
                    **{k: g[k] for k in PACING if g.get(k)}} for g in summary.get('pacing', [])
                   if g['framework'] in NAMES],
        # GPU execution from Metal System Trace (scripts/trace_macos_gpu.py), when traced.
        **({'gpu_trace': [{'framework': g['framework'], 'mode': g['mode'], **{k: g[k] for k in TRACE}}
                          for g in summary['gpu_trace'] if g['framework'] in NAMES]}
           if 'gpu_trace' in summary else {}),
        'run': {
            'started_utc': audit['started_utc'],
            'commit': audit['commit'],
            'source_dirty': audit['source_dirty'],
            'source_diff_sha256': audit['source_diff_sha256'],
            'trials': audit['trials'],
            'pacing_trials': audit.get('pacing_trials'),
            'environment': audit['environment'],
        },
    })

order = {'lab': 0, 'heavy': 1, 'animate': 2}
scenes.sort(key=lambda s: order.get(s['scene'], 9))
a.output.mkdir(parents=True, exist_ok=True)
(a.output / 'data.json').write_text(json.dumps({'names': NAMES, 'scenes': scenes}, indent=1) + '\n')
print('Published', ', '.join(f"{s['scene']} ({s['run']['trials']} trials)" for s in scenes), 'to', a.output)
