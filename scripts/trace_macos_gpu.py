#!/usr/bin/env python3
"""GPU execution time of each framework in an accepted macOS comparison, from
Metal System Trace: how long the GPU actually ran each app's work.

    python3 scripts/trace_macos_gpu.py docs/results/latest-macos-<scene>-<date>-<commit>

The runner's GPU figure (scripts/run_macos_comparison.py) is the app's
accumulated GPU time from IOKit, which also counts each command buffer's
fixed cost and stretches as the GPU clocks down. A Metal System Trace records
when the GPU ran each encoder's vertex and fragment work; their union per
second is the work itself. Each app runs alone in the foreground per
capture, three repeats per framework and mode, in rotated order. Writes
gputrace.csv and adds `gpu_trace` groups to the run's summary.json.
"""
import argparse, csv, json, os, statistics, subprocess, tempfile, time, xml.etree.ElementTree as ET
from pathlib import Path
from run_macos_comparison import SCENES, WARMUP, finish, launch

SECONDS = 5
TRACE = ['gpu_busy_ms_per_second', 'command_buffers_per_second', 'gpu_busy_us_per_command_buffer']


def busy(table, pid):
    """GPU busy time (the union of `pid`'s execution intervals), command
    buffers and the traced span, from a metal-gpu-intervals export."""
    root = ET.parse(table).getroot()
    ids = {el.get('id'): el for el in root.iter() if el.get('id')}
    schema = [c.find('mnemonic').text for c in root.iter('col')]
    resolve = lambda el: ids[el.get('ref')] if el is not None and el.get('ref') else el
    spans, buffers, lo, hi = [], set(), None, 0
    for row in root.iter('row'):
        cells = dict(zip(schema, map(resolve, row)))
        start, duration = int(cells['start'].text), int(cells['duration'].text)
        lo, hi = start if lo is None else min(lo, start), max(hi, start + duration)
        label = cells.get('event-label')
        if label is None or f'({pid}) )' not in (label.get('fmt') or ''):
            continue
        spans.append((start, start + duration))
        buffer = cells.get('cmdbuffer-id')
        if buffer is not None:
            buffers.add(buffer.text or buffer.get('fmt'))
    total, current = 0, None
    for start, end in sorted(spans):
        if current and start <= current[1]:
            current[1] = max(current[1], end)
        else:
            total += current[1] - current[0] if current else 0
            current = [start, end]
    total += current[1] - current[0] if current else 0
    return total, len(buffers), (hi - (lo or 0))


def capture(scene, framework, mode, repeat, out):
    # xctrace occasionally aborts or stalls while recording: one more try.
    for attempt in range(2):
        try:
            return trace(scene, framework, mode, repeat, out)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            if attempt:
                raise
            time.sleep(5)


def trace(scene, framework, mode, repeat, out):
    seconds = WARMUP + SECONDS + 4
    process = launch(SCENES[scene]['binaries'][framework], mode, seconds, out / f'{framework}.{mode}.{repeat}.trace.log')
    time.sleep(WARMUP - 0.5)
    # xctrace leaves its raw kernel trace (gigabytes) in the user temporary
    # directory whatever TMPDIR says: remove the ones this capture adds.
    kernel_traces = Path(subprocess.run(['getconf', 'DARWIN_USER_TEMP_DIR'], check=True,
                                        capture_output=True, text=True).stdout.strip())
    before = set(kernel_traces.glob('instruments*.ktrace'))
    try:
        with tempfile.TemporaryDirectory() as scratch:
            trace, table = Path(scratch) / 'gpu.trace', Path(scratch) / 'intervals.xml'
            try:
                subprocess.run(['xcrun', 'xctrace', 'record', '--template', 'Metal System Trace', '--all-processes',
                                '--time-limit', f'{SECONDS}s', '--output', str(trace)],
                               check=True, capture_output=True, timeout=120)
            finally:
                finish(process, seconds)
            with table.open('w') as handle:
                subprocess.run(['xcrun', 'xctrace', 'export', '--input', str(trace), '--xpath',
                                '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]'],
                               check=True, stdout=handle, stderr=subprocess.DEVNULL, timeout=600)
            total, buffers, span = busy(table, process.pid)
    finally:
        for leftover in set(kernel_traces.glob('instruments*.ktrace')) - before:
            leftover.unlink(missing_ok=True)
    span = span / 1e9 or SECONDS
    return {'framework': framework, 'mode': mode, 'repeat': repeat,
            'gpu_busy_ms_per_second': total / 1e6 / span,
            'command_buffers_per_second': buffers / span,
            'gpu_busy_us_per_command_buffer': total / 1e3 / buffers if buffers else 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('results', type=Path)
    out = parser.parse_args().results
    audit = json.loads((out / 'audit.json').read_text())
    assert audit['accepted'], f'{out} was not accepted'
    scene = audit['scene']
    frameworks = list(SCENES[scene]['binaries'])
    modes = [m for m in SCENES[scene]['modes'] if m != 'idle']
    rows = []
    for repeat in range(3):
        order = frameworks[repeat:] + frameworks[:repeat]
        for mode in modes:
            for framework in order:
                row = capture(scene, framework, mode, repeat, out)
                print(f"{framework:9} {mode:8} {repeat}: gpu busy {row['gpu_busy_ms_per_second']:6.2f} ms/s, "
                      f"{row['command_buffers_per_second']:6.1f} command buffers/s, "
                      f"{row['gpu_busy_us_per_command_buffer']:6.0f} us each", flush=True)
                rows.append(row)
                time.sleep(2)
    with (out / 'gputrace.csv').open('w', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=['framework', 'mode', 'repeat', *TRACE], lineterminator='\n')
        writer.writeheader()
        writer.writerows(rows)
    groups = []
    for framework in frameworks:
        for mode in modes:
            chosen = [r for r in rows if (r['framework'], r['mode']) == (framework, mode)]
            groups.append({'framework': framework, 'mode': mode, 'repeats': len(chosen), **{
                key: {'median': statistics.median(r[key] for r in chosen),
                      'min': min(r[key] for r in chosen), 'max': max(r[key] for r in chosen)}
                for key in TRACE}})
    summary = json.loads((out / 'summary.json').read_text())
    summary['gpu_trace'] = groups
    (out / 'summary.json').write_text(json.dumps(summary, indent=1) + '\n')
    print('Wrote', out / 'gputrace.csv')


if __name__ == '__main__':
    main()
