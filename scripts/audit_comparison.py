#!/usr/bin/env python3
"""Validate complete comparison artifacts and recompute summaries from raw samples."""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import re
import tarfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('csv', type=Path)
    parser.add_argument('--source-manifest', type=Path)
    parser.add_argument('--source-archive', type=Path)
    parser.add_argument('--binaries', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    rows = list(csv.DictReader(args.csv.open()))
    metadata = json.loads(Path(str(args.csv) + '.metadata.json').read_text())
    repeats = int(metadata['arguments']['repeats'])
    expected = {(framework, mode, repeat) for framework in ['zgx', 'gpui', 'quickgui']
                for mode in ['idle', 'stream', 'scroll', 'both'] for repeat in range(repeats)}
    actual = [(r['framework'], r['mode'], int(r['repeat'])) for r in rows]
    require(len(actual) == len(expected) and set(actual) == expected, 'missing, duplicate or unexpected trials')
    samples_total = 0
    rates = []
    for row in rows:
        stem = f"{args.csv}.{row['framework']}.{row['mode']}.{row['repeat']}"
        raw = json.loads(Path(stem + '.json').read_text())
        samples = raw['samples']
        require(int(row['exit_code']) == 0 and len(samples) >= 2, stem + ': failed or unsampled trial')
        require(all(s['rss_bytes'] > 0 and s['elapsed_seconds'] >= raw['warmup_seconds'] for s in samples), stem + ': invalid RSS/warmup')
        require(all(b['elapsed_seconds'] > a['elapsed_seconds'] and b['cpu_seconds'] >= a['cpu_seconds']
                    for a, b in zip(samples, samples[1:])), stem + ': nonmonotonic samples')
        wall = samples[-1]['elapsed_seconds'] - samples[0]['elapsed_seconds']
        cpu = samples[-1]['cpu_seconds'] - samples[0]['cpu_seconds']
        recomputed = dict(samples=len(samples), wall_seconds=wall, cpu_seconds=cpu,
            cpu_percent_one_core=100*cpu/wall, mean_rss_bytes=sum(s['rss_bytes'] for s in samples)/len(samples),
            peak_rss_bytes=max(s['rss_bytes'] for s in samples))
        for key, value in recomputed.items():
            require(math.isclose(float(row[key]), value, rel_tol=1e-8, abs_tol=1e-7), stem + ': CSV ' + key)
            require(math.isclose(float(raw['summary'][key]), value, rel_tol=1e-8, abs_tol=1e-7), stem + ': raw ' + key)
        match = re.search(r'workload_ticks=(\d+)|"ticks":(\d+)', Path(stem + '.log').read_text())
        require(match is not None, stem + ': missing tick report')
        ticks = int(next(group for group in match.groups() if group is not None))
        require(ticks == int(row['workload_ticks']) == raw['summary']['workload_ticks'], stem + ': tick mismatch')
        if row['mode'] == 'idle':
            require(ticks == 0, stem + ': nonidle work')
        else:
            rates.append(ticks / raw['requested_seconds'])
        samples_total += len(samples)
    archived = 0
    if args.source_manifest or args.source_archive:
        require(args.source_manifest and args.source_archive, 'manifest and archive must be paired')
        hashes = json.loads(args.source_manifest.read_text())
        with tarfile.open(args.source_archive, 'r:gz') as archive:
            files = {m.name: m for m in archive.getmembers() if m.isfile()}
            require(set(files) == set(hashes), 'archive file inventory differs')
            for name, sha in hashes.items():
                require(hashlib.sha256(archive.extractfile(files[name]).read()).hexdigest() == sha, 'archive hash: ' + name)
        archived = len(hashes)
    if args.binaries:
        for framework, filename in [('zgx', 'component_workload'), ('gpui', 'zgx-compare-gpui'), ('quickgui', 'zgx-compare-quickgui')]:
            require(hashlib.sha256((args.binaries / filename).read_bytes()).hexdigest() == metadata['binary_sha256'][framework], 'binary hash: ' + framework)
    args.output.write_text(json.dumps(dict(trials=len(rows), samples=samples_total, archived_sources=archived,
        active_ticks_per_requested_second=dict(min=min(rates), max=max(rates)),
        near_60hz_workload_updates=all(58 <= rate <= 61 for rate in rates),
        verified='CSV/raw recomputation, complete trial inventory, successful exits, positive RSS, monotonic post-warmup samples, log ticks',
        limitations='Ticks are model updates, not presented frames. Optional source/binary checks only verify supplied files.'), indent=2) + '\n')


if __name__ == '__main__':
    main()
