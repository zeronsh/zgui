#!/usr/bin/env python3
"""Copy an accepted macOS comparison (scripts/run_macos_comparison.py) into
the static results site, under site/macos."""
import argparse, csv, hashlib, json, math, statistics
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('results', type=Path)
p.add_argument('--output', type=Path, default=Path('site/macos'))
a = p.parse_args()
r = a.results
audit = json.loads((r / 'audit.json').read_text())
scene = audit.get('scene', 'lab')
assert scene in ('lab', 'heavy')
# Newer runs add GPUI Fast (shown on the compare page) and the desktop
# baseline; this site shows the original frameworks.
frameworks = [f for f in audit['protocol'].get('frameworks', ['zgui', 'gpui', 'quickgui']) if f in ('zgui', 'gpui', 'quickgui')]
# QuickGUI is optional in lab (`--quickgui`) and absent from heavy.
assert frameworks[:2] == ['zgui', 'gpui'] and (scene == 'lab' or frameworks == ['zgui', 'gpui'])
modes = ['idle', 'stream', 'scroll', 'both']
assert audit['protocol']['requested_seconds'] == 20 and audit['protocol']['repeats'] == 3
assert not audit['protocol'].get('quick'), 'quick runs are for iterating, not publishing'
# Runs since October 2026 measure idle once.
repeats = {m: audit['protocol'].get('idle_repeats', 3) if m == 'idle' else 3 for m in modes}
summary = json.loads((r / 'summary.json').read_text())
summary['groups'] = [g for g in summary['groups'] if g['framework'] in frameworks]
trials = [t for t in csv.DictReader((r / 'current.csv').open()) if t['framework'] in frameworks]
assert audit['accepted'], 'only accepted runs are published'
assert len(trials) == len(frameworks) * sum(repeats.values()) <= audit['trials']
assert len(summary['groups']) == len(frameworks) * 4
assert {(g['framework'], g['mode']) for g in summary['groups']} == {(f, m) for f in frameworks for m in modes}
assert {(t['framework'], t['mode'], int(t['repeat'])) for t in trials} == {(f, m, n) for f in frameworks for m in modes for n in range(repeats[m])}
assert all(int(t['exit_code']) == 0 or t['terminated_after_report'].lower() == 'true' for t in trials)
# Recompute every published aggregate from the trials.
for group in summary['groups']:
    rows = [t for t in trials if (t['framework'], t['mode']) == (group['framework'], group['mode'])]
    assert group['repeats'] == repeats[group['mode']]
    for key in ['cpu_percent_one_core', 'mean_footprint_bytes', 'peak_footprint_bytes', 'mean_rss_bytes', 'peak_rss_bytes', 'workload_ticks', 'wall_seconds']:
        values = [float(t[key]) for t in rows]
        assert all(math.isfinite(v) and v >= 0 for v in values)
        for field, value in [('median', statistics.median(values)), ('min', min(values)), ('max', max(values))]:
            assert math.isclose(group[key][field], value, rel_tol=1e-10, abs_tol=1e-10), (group['framework'], group['mode'], key)
assert sum(int(t['samples']) for t in trials) <= audit['samples']
assert all(58 <= int(t['workload_ticks']) / 20 <= 61 for t in trials if t['mode'] != 'idle')
assert all(int(t['workload_ticks']) == 0 for t in trials if t['mode'] == 'idle')
a.output.mkdir(parents=True, exist_ok=True)
(a.output / 'data.json').write_text(json.dumps({'groups': summary['groups']}, indent=2) + '\n')
fields = ['framework', 'mode', 'repeat', 'wall_seconds', 'cpu_seconds', 'cpu_percent_one_core', 'mean_footprint_bytes',
          'peak_footprint_bytes', 'mean_rss_bytes', 'peak_rss_bytes', 'samples', 'workload_ticks', 'exit_code', 'terminated_after_report']
with (a.output / 'data.csv').open('w', newline='') as output:
    writer = csv.DictWriter(output, fieldnames=fields, extrasaction='ignore')
    writer.writeheader()
    writer.writerows(trials)
env = audit['environment']
metadata = {
    'date': audit['started_utc'][:10],
    'commit': audit['commit'],
    'scene': scene,
    'source_dirty': audit.get('source_dirty', False),
    'trials': audit['trials'],
    'samples': audit['samples'],
    'updates': audit['active_ticks_per_requested_second'],
    'environment': f"{env['chip']} ({env['machine']}), macOS {env['macos']}, Metal; on {env['power']}",
    'host_note': 'A personal Mac in use: other applications kept running, so this host was not isolated.',
    'memory': 'Physical footprint (as Activity Monitor reports it: resident memory plus GPU memory the process owns on unified memory); resident size also recorded.',
    'protocol': audit['protocol'],
    'frameworks': {f: {'gpui': '0.2.2', 'quickgui': '811d6e2816d5229711f59683c4c9dfbb6fc74133', 'zgui': audit['commit']}[f] for f in frameworks},
    'csv_sha256': hashlib.sha256((a.output / 'data.csv').read_bytes()).hexdigest(),
    'source_csv_sha256': hashlib.sha256((r / 'current.csv').read_bytes()).hexdigest(),
    'binaries': audit['binaries'],
}
if scene == 'heavy':
    metadata['excluded_frameworks'] = {'quickgui': 'Adapter panicked at startup: text renderer batch index 32 exceeded its 32-batch pool.'}
    metadata['source_note'] = 'Measured working tree based on 92fd6a4, before adapters were committed. The original diff hash omitted untracked adapter files; their hashes are documented separately in the result README. This is not a clean-commit measurement.'
(a.output / 'measurement.json').write_text(json.dumps(metadata, indent=2) + '\n')
print('Prepared accepted macOS results for', metadata['commit'])
