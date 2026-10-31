#!/usr/bin/env python3
"""Whether this Mac is quiet enough to measure, and what it is like.

    python3 scripts/perf/preflight.py          # check once, print what was found
    python3 scripts/perf/preflight.py --state  # the machine state recorded with a run

`quiet()` samples every process's CPU time over a few seconds. The machine
is quiet when nothing known to be heavy runs (compilers and linkers of any
agent, Linux VMs, Instruments, stress tools, Spotlight indexing, Time
Machine, software updates) and the processes other than the benchmark's and
the window server together use less than a threshold of one core.
A system dialog on screen (a privacy prompt, a crash report) makes it not
quiet too: it would sit over the windows measured. `dialogs()` only reads
them; nothing here clicks one (a scripted answer to a privacy prompt once
wedged macOS's privacy service): the person at the Mac dismisses it.
`wait_until_quiet()` checks again until it is, logging what it found.
`desktop_still()` stops a run whose desktop draws by itself (a live
wallpaper): the agent app hidden, the window server should be about idle.
`state()` records what changes a desktop's own cost between days: displays
(count, resolution, refresh), a live-wallpaper app, power mode, macOS
build and uptime.
"""
import json
import re
import subprocess
import sys
import time

# Processes that make a measurement meaningless while they run, by name.
HEAVY = ['cargo', 'rustc', 'miri', 'cargo-miri', 'clippy-driver', 'rustdoc', 'rust-lld', 'clang', 'clang++', 'cc1', 'cc1plus', 'ld', 'ld64', 'swift-frontend',
         'swiftc', 'xctrace', 'Instruments', 'yes', 'stress', 'stress-ng', 'OrbStack Helper', 'com.docker.backend',
         'qemu-system-aarch64', 'qemu-system-x86_64', 'backupd', 'softwareupdated', 'mdworker_shared']
# A heavy process counts once it uses this much of a core (daemons such as
# backupd and softwareupdated stay resident, idle; a cargo waits on rustc).
HEAVY_PERCENT = 2.0
# Heavy only while busy: always running, indexing now and then.
BUSY_ONLY = {'mds_stores': 20.0, 'mds': 20.0, 'mdworker_shared': 20.0}
# Never counted against the threshold: what a GUI measurement measures, and
# the desktop's own residents (a live wallpaper: recorded by `state`, and in
# the desktop-alone baseline every run measures).
EXEMPT = ['WindowServer', 'kernel_task', 'zgx-macsample', 'zgx-framepace']
RESIDENTS = ['Wallspace', 'Wallpaper Engine', 'Plash']
# The window server's share of a core on a still desktop: 0.0-0.7 in the
# suites of 2026-10-26 to 10-30. One on 2026-10-30 had Wallspace drawing
# behind its windows: 14.5%, and every WindowServer and GPU row of it void.
STILL_DESKTOP = 2.0
# Where system dialogs come from: privacy prompts, crash reports, keychain.
DIALOGS = ['UserNotificationCenter', 'CoreServicesUIAgent', 'Problem Reporter', 'universalAccessAuthWarn', 'SecurityAgent']


def dialogs():
    """The system dialogs on screen, as their text (read through System
    Events; never clicked)."""
    script = '''set found to {}
tell application "System Events"
  repeat with n in {%s}
    if exists process (n as text) then
      repeat with w in (every window of process (n as text))
        set texts to ""
        try
          repeat with t in (every static text of w)
            set texts to texts & (value of t as text) & " "
          end repeat
        end try
        set end of found to (n as text) & ": " & texts
      end repeat
    end if
  end repeat
end tell
set AppleScript's text item delimiters to linefeed
found as text''' % ', '.join(f'"{n}"' for n in DIALOGS)
    done = subprocess.run(['osascript', '-e', script], capture_output=True, text=True)
    if done.returncode != 0:
        # Unread is not none: a check that cannot see dialogs (System Events
        # not allowed to this terminal) counts as one on screen.
        return [f'the dialog check failed (allow System Events for this terminal): {done.stderr.strip()[:200]}']
    return [line.strip() for line in done.stdout.splitlines() if line.strip()]


def cpu_times():
    """pid -> (name, CPU seconds so far) for every process."""
    out = subprocess.run(['ps', '-A', '-o', 'pid=,time=,comm='], capture_output=True, text=True).stdout
    times = {}
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        pid, clock, path = parts
        days = 0
        if '-' in clock:
            day, clock = clock.split('-')
            days = int(day)
        seconds = 0.
        for part in clock.split(':'):
            seconds = seconds * 60 + float(part)
        times[int(pid)] = (path.rsplit('/', 1)[-1], seconds + days * 86400)
    return times


def quiet(seconds=3.0, threshold=75.0, ignore=()):
    """(quiet, findings): `findings` lists the heavy processes and the busiest others."""
    before = cpu_times()
    time.sleep(seconds)
    after = cpu_times()
    usage = []
    for pid, (name, total) in after.items():
        if pid in before and before[pid][0] == name:
            usage.append((name, pid, (total - before[pid][1]) / seconds * 100))
    usage.sort(key=lambda u: -u[2])
    heavy = [u for u in usage if u[2] >= BUSY_ONLY.get(u[0], HEAVY_PERCENT) and (u[0] in HEAVY or u[0] in BUSY_ONLY)]
    others = [u for u in usage
              if u[0] not in EXEMPT + RESIDENTS and u[1] not in ignore and u not in heavy]
    load = sum(u[2] for u in others)
    findings = {
        'heavy': [f'{name} ({pid}) {cpu:.0f}%' for name, pid, cpu in heavy],
        'other_cpu_percent': round(load, 1),
        'busiest': [f'{name} {cpu:.1f}%' for name, _, cpu in others[:6] if cpu >= 0.5],
        'window_server_percent': round(sum(u[2] for u in usage if u[0] == 'WindowServer'), 1),
        'residents': [f'{name} {cpu:.1f}%' for name, _, cpu in usage if name in RESIDENTS],
        # The agent app's own processes (its UI drew frames while hidden,
        # 2026-10-25): recorded with every run.
        'zeron': [f'{name} ({pid}) {cpu:.1f}%' for name, pid, cpu in usage if name in ('zeron', 'Zeron', 'Zeron Engine') and cpu >= 0.5],
        'dialogs': dialogs(),
    }
    return not heavy and not findings['dialogs'] and load < threshold, findings


def wait_until_quiet(log=print, threshold=75.0, patience=1800, ignore=(), dialog_patience=120):
    """Check until quiet (or `patience` seconds pass: then raise). A system
    dialog that stays `dialog_patience` seconds stops it at once: it needs a
    person, and a quiet window should not wait it out."""
    started = time.monotonic()
    dialog_since = None
    while True:
        ok, findings = quiet(threshold=threshold, ignore=ignore)
        if ok:
            log(f'preflight: quiet (others {findings["other_cpu_percent"]}% of a core; busiest '
                f'{", ".join(findings["busiest"]) or "none"})')
            return findings
        if findings['dialogs']:
            dialog_since = dialog_since or time.monotonic()
            log('preflight: waiting for a system dialog to be dismissed (not clicking it): '
                + ' | '.join(findings['dialogs']))
            if time.monotonic() - dialog_since > dialog_patience:
                raise SystemExit('preflight: a system dialog is on screen; dismiss it by hand: '
                                 + ' | '.join(findings['dialogs']))
        else:
            dialog_since = None
            log(f'preflight: waiting (above {threshold:g}%): heavy {findings["heavy"] or "none"}, others '
                f'{findings["other_cpu_percent"]}% ({", ".join(findings["busiest"])})')
        if time.monotonic() - started > patience:
            raise SystemExit('preflight: the machine stayed busy; not measuring')
        time.sleep(10)


def desktop_still(seconds=5.0, limit=STILL_DESKTOP):
    """The window server's share of a core, once the agent app is hidden;
    stops at `limit`: what draws by itself is in every number after this,
    and only a person can decide to stop it."""
    before = cpu_times()
    time.sleep(seconds)
    after = cpu_times()
    server = sum((total - before[pid][1]) / seconds * 100 for pid, (name, total) in after.items()
                 if name == 'WindowServer' and pid in before)
    if server >= limit:
        running = [name for name in RESIDENTS if subprocess.run(['pgrep', '-x', name], capture_output=True).returncode == 0]
        raise SystemExit(f'preflight: the window server uses {server:.1f}% of a core on a still desktop (limit {limit:g}%); '
                         f'live wallpaper: {", ".join(running) or "none found"}; not measuring')
    return round(server, 1)


def state():
    """What the desktop is like now."""
    def text(*command):
        return subprocess.run(command, capture_output=True, text=True).stdout.strip()
    displays = []
    try:
        data = json.loads(text('system_profiler', 'SPDisplaysDataType', '-json'))
        for gpu in data.get('SPDisplaysDataType', []):
            for display in gpu.get('spdisplays_ndrvs', []):
                displays.append({
                    'name': display.get('_name'),
                    'resolution': display.get('_spdisplays_resolution') or display.get('spdisplays_resolution'),
                    'pixels': display.get('_spdisplays_pixels'),
                    'main': display.get('spdisplays_main') == 'spdisplays_yes',
                })
    except (json.JSONDecodeError, OSError):
        pass
    power = text('pmset', '-g')
    mode = re.search(r'\b(powermode|lowpowermode)\s+(\d)', power)
    boot = re.search(r'sec = (\d+)', text('sysctl', '-n', 'kern.boottime'))
    return {
        'displays': displays,
        'display_count': len(displays),
        'wallpaper_apps': [name for name in ('Wallspace', 'Wallpaper Engine', 'Plash')
                           if subprocess.run(['pgrep', '-x', name], capture_output=True).returncode == 0],
        'power_mode': f'{mode.group(1)} {mode.group(2)}' if mode else None,
        'on_battery': 'Battery Power' in text('pmset', '-g', 'batt'),
        'macos': f"{text('sw_vers', '-productVersion')} ({text('sw_vers', '-buildVersion')})",
        'uptime_hours': round((time.time() - int(boot.group(1))) / 3600, 1) if boot else None,
    }


if __name__ == '__main__':
    if '--state' in sys.argv:
        print(json.dumps(state(), indent=1))
    else:
        ok, findings = quiet()
        print(json.dumps({'quiet': ok, **findings}, indent=1))
        sys.exit(0 if ok else 1)
