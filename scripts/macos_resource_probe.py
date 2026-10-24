#!/usr/bin/env python3
"""Sample real component_workload processes on macOS; not a matched comparison.

Pause builds and other validation before running. Keep the window visible. CPU
is process user+system time divided by elapsed wall time (100% = one core).
RSS and physical footprint are sampled process values, not total GPU memory.
The idle 1% reference is reported, not treated as a universal pass/fail limit.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import errno
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import resource
import subprocess
import tarfile
import time


class RusageInfoV0(ctypes.Structure):
    # macOS SDK sys/resource.h; CPU counters use Mach absolute-time units.
    _fields_ = [('uuid', ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            'user_time', 'system_time', 'pkg_idle_wkups', 'interrupt_wkups',
            'pageins', 'wired_size', 'resident_size', 'phys_footprint',
            'proc_start_abstime', 'proc_exit_abstime')]


def make_sampler():
    class TimebaseInfo(ctypes.Structure):
        _fields_ = [('numer', ctypes.c_uint32), ('denom', ctypes.c_uint32)]

    system = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
    timebase = TimebaseInfo()
    system.mach_timebase_info.argtypes = [ctypes.POINTER(TimebaseInfo)]
    system.mach_timebase_info.restype = ctypes.c_int
    if system.mach_timebase_info(ctypes.byref(timebase)) or not timebase.denom:
        raise RuntimeError('cannot read Mach CPU timebase')
    seconds_per_tick = timebase.numer / timebase.denom / 1e9
    library = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    function = library.proc_pid_rusage
    function.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    function.restype = ctypes.c_int

    def sample(pid):
        usage = RusageInfoV0()
        if function(pid, 0, ctypes.byref(usage)):
            error = ctypes.get_errno()
            if error in (errno.ESRCH, errno.ENOENT):
                return None
            raise OSError(error, os.strerror(error))
        if usage.proc_exit_abstime or not usage.resident_size:
            return None
        return dict(cpu_seconds=(usage.user_time + usage.system_time) * seconds_per_tick,
                    rss_bytes=usage.resident_size,
                    physical_footprint_bytes=usage.phys_footprint)
    sample.timebase = dict(numer=timebase.numer, denom=timebase.denom, seconds_per_tick=seconds_per_tick)
    return sample


def verify_sampler(sample):
    """Catch ABI/unit mistakes against the current process's getrusage."""
    usage_before = resource.getrusage(resource.RUSAGE_SELF)
    before = sample(os.getpid())
    end = time.process_time() + .05
    while time.process_time() < end:
        pass
    after = sample(os.getpid())
    usage_after = resource.getrusage(resource.RUSAGE_SELF)
    expected = (usage_after.ru_utime + usage_after.ru_stime
                - usage_before.ru_utime - usage_before.ru_stime)
    observed = after['cpu_seconds'] - before['cpu_seconds']
    if abs(expected - observed) > .005:
        raise RuntimeError(f'sampler CPU units/ABI check failed: {observed} vs {expected}')
    return dict(proc_pid_rusage_cpu_seconds=observed, getrusage_cpu_seconds=expected)


def trial(binary, mode, repeat, args, sample):
    path = args.output / f'{mode}-{repeat:02d}'
    raw = dict(mode=mode, repeat=repeat, requested_seconds=args.seconds,
               warmup_seconds=args.warmup, samples=[], status='running')
    started = time.monotonic()
    process = None
    try:
        env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=str(args.seconds),
                   ZGX_INITIAL_TICKS='0')
        with path.with_suffix('.log').open('w') as log:
            process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
            raw['pid'] = process.pid
            while process.poll() is None:
                elapsed = time.monotonic() - started
                if elapsed > args.seconds + 30:
                    raise RuntimeError('native workload did not terminate')
                value = sample(process.pid)
                if value is not None:
                    raw['samples'].append(dict(elapsed_seconds=time.monotonic() - started, **value))
                time.sleep(args.interval)
        raw['exit_code'] = process.wait()
        reports = []
        for line in path.with_suffix('.log').read_text().splitlines():
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict) and value.get('framework') == 'zgx':
                reports.append(value)
        raw['application_reports'] = reports
        if raw['exit_code'] != 0 or len(reports) != 1:
            raise RuntimeError('workload must exit successfully and report exactly once')
        report = reports[0]
        if report.get('renderer') != 'gpu' or report.get('adapter') != 'components':
            raise RuntimeError('expected the native GPU component adapter')
        if not 1 <= report['mounted_rows'] <= 32:
            raise RuntimeError('mounted row bound exceeded')
        if not args.seconds - .1 <= report['elapsed_seconds'] <= args.seconds + 2:
            raise RuntimeError('reported workload duration does not match request')
        rate = report['ticks'] / args.seconds
        if (mode == 'idle' and report['ticks'] != 0) or (mode == 'both' and not 58 <= rate <= 61):
            raise RuntimeError(f'delivered model-work gate failed: {rate} updates/s')
        samples = [item for item in raw['samples'] if item['elapsed_seconds'] >= args.warmup]
        if len(samples) < 2:
            raise RuntimeError('insufficient post-warmup samples')
        for previous, current in zip(samples, samples[1:]):
            if current['elapsed_seconds'] <= previous['elapsed_seconds'] or current['cpu_seconds'] < previous['cpu_seconds']:
                raise RuntimeError('non-monotonic sample time or CPU accounting')
        wall = samples[-1]['elapsed_seconds'] - samples[0]['elapsed_seconds']
        if wall < args.seconds - args.warmup - 1:
            raise RuntimeError('insufficient sampled workload duration')
        cpu = samples[-1]['cpu_seconds'] - samples[0]['cpu_seconds']
        raw['summary'] = dict(
            sampled_wall_seconds=wall, sampled_cpu_seconds=cpu,
            cpu_percent_one_core=100 * cpu / wall,
            mean_rss_bytes=sum(item['rss_bytes'] for item in samples) / len(samples),
            peak_sampled_rss_bytes=max(item['rss_bytes'] for item in samples),
            mean_physical_footprint_bytes=sum(item['physical_footprint_bytes'] for item in samples) / len(samples),
            rss_first_bytes=samples[0]['rss_bytes'], rss_last_bytes=samples[-1]['rss_bytes'],
            post_warmup_samples=len(samples), model_updates_per_requested_second=rate,
            idle_cpu_at_or_below_one_percent=(100 * cpu / wall <= 1) if mode == 'idle' else None)
        raw['status'] = 'functional-and-sampling-gates-passed'
        return raw['summary']
    except BaseException as error:
        raw.update(status='failed', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        if process is not None:
            if process.poll() is None:
                process.kill()
            raw['exit_code'] = process.wait()
        raw['process_wall_seconds'] = time.monotonic() - started
        path.with_suffix('.json').write_text(json.dumps(raw, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--seconds', type=float, default=10)
    parser.add_argument('--warmup', type=float, default=2)
    parser.add_argument('--interval', type=float, default=.1)
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        parser.error('requires native macOS process accounting and a logged-in desktop')
    if not (all(math.isfinite(value) for value in (args.seconds, args.warmup, args.interval))
            and args.seconds > args.warmup >= 0 and args.interval > 0 and args.repeats >= 1):
        parser.error('require seconds > warmup >= 0, positive interval and repeats >= 1')
    args.binary = args.binary.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error('binary must be an existing executable')
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[1]
    metadata = dict(status='running', started_utc=datetime.now(timezone.utc).isoformat(),
                    platform=platform.platform(), arguments={key: str(value) for key, value in vars(args).items()},
                    binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                    trials=[], note=__doc__, sampler='proc_pid_rusage RUSAGE_INFO_V0; CPU Mach ticks converted through mach_timebase_info; RSS bytes',
                    limitations='No matched reference apps, presented-frame, GPU/WindowServer allocation, energy, latency, long-run leak or universal resource-minimum claim. Model update rate is not display cadence.')
    try:
        metadata['commit'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        metadata['git_status'] = subprocess.check_output(['git', 'status', '--porcelain=v1'], cwd=root, text=True)
        diff = subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=root)
        (args.output / 'source.diff').write_bytes(diff)
        metadata['source_diff_sha256'] = hashlib.sha256(diff).hexdigest()
        paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root).split(b'\0')
        sources = {}
        with tarfile.open(args.output / 'source.tar.gz', 'w:gz') as archive:
            for raw in sorted(set(paths)):
                if not raw:
                    continue
                path = Path(os.fsdecode(raw))
                if path.parts[0] not in ('crates', 'comparisons', 'assets', 'scripts', '.github') and len(path.parts) != 1 and not (path.parts[0] == 'docs' and len(path.parts) == 2):
                    continue
                source = root / path
                if source.is_file() and not source.is_symlink():
                    data = source.read_bytes()
                    sources[str(path)] = hashlib.sha256(data).hexdigest()
                    entry = archive.gettarinfo(str(source), arcname=str(path))
                    entry.size = len(data)
                    entry.uid = entry.gid = 0
                    entry.uname = entry.gname = ''
                    archive.addfile(entry, io.BytesIO(data))
        (args.output / 'source-files.json').write_text(json.dumps(sources, indent=2) + '\n')
        metadata['source_archive_sha256'] = hashlib.sha256((args.output / 'source.tar.gz').read_bytes()).hexdigest()
        metadata['hardware'] = subprocess.check_output(['sysctl', 'hw.model', 'hw.memsize', 'hw.ncpu', 'machdep.cpu.brand_string'], text=True)
        metadata['rustc'] = subprocess.check_output(['rustc', '-vV'], text=True)
        sample = make_sampler()
        metadata['mach_timebase'] = sample.timebase
        metadata['sampler_self_check'] = verify_sampler(sample)
        for repeat in range(args.repeats):
            for mode in ('idle', 'both') if repeat % 2 == 0 else ('both', 'idle'):
                summary = trial(args.binary, mode, repeat, args, sample)
                metadata['trials'].append(dict(mode=mode, repeat=repeat, **summary))
                print(f'{mode} {repeat}: CPU {summary["cpu_percent_one_core"]:.3f}% of one core; '
                      f'RSS {summary["mean_rss_bytes"] / 1048576:.2f} MiB', flush=True)
        if hashlib.sha256(args.binary.read_bytes()).hexdigest() != metadata['binary_sha256']:
            raise RuntimeError('executable changed during sampling; preserve this attempt and retry without builds')
        metadata['status'] = 'functional-and-sampling-gates-passed'
    except BaseException as error:
        metadata.update(status='failed', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        metadata['finished_utc'] = datetime.now(timezone.utc).isoformat()
        (args.output / 'results.json').write_text(json.dumps(metadata, indent=2) + '\n')


if __name__ == '__main__':
    main()
