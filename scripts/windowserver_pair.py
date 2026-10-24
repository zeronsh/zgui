#!/usr/bin/env python3
"""The window server's CPU for two heavy-dashboard apps, in paired seconds.

    python3 scripts/windowserver_pair.py SECONDS MODE APP_A APP_B

Runs both apps' windows side by side, A's workload in even Unix seconds and
B's in odd (`ZGX_PARITY`, read by `Heavy::turn`), credits the window
server's CPU time in each whole second to the app whose turn it was, then
repeats with the windows' places swapped so a place on screen favours
neither. MODE is the heavy workload's `ZGX_MODE` (stream, scroll, both); an
app given as `mode:path` runs in a mode of its own. Prints A - B over all
paired seconds with its standard error. Pair an app with itself first: that
spread is the method's own. macOS only; the terminal needs Accessibility
access to place the windows. See docs/performance-feature-audit.md.
"""
import os
import statistics
import subprocess
import sys
import time

# Each heavy window is 1280 points wide: these places don't overlap (a
# covered window costs the window server less).
PLACES = ((0, 40), (1280, 40))


def cpu_seconds(pid):
    """A process's CPU time so far, or None once it is gone."""
    text = subprocess.run(['ps', '-o', 'time=', '-p', str(pid)], capture_output=True, text=True).stdout.strip()
    if not text:
        return None
    days = 0
    if '-' in text:
        days, text = text.split('-')
    value = 0.
    for part in text.split(':'):
        value = value * 60 + float(part)
    return value + int(days) * 86400


def sleep_to_next_second():
    now = time.time()
    time.sleep(int(now) + 1 - now + 0.02)


def window(pid, action):
    """Ask System Events to `action` (`get ...` or `set ... to ...`) the window of `pid`."""
    verb, _, rest = action.partition(' ')
    what, _, value = rest.partition(' to ')
    script = (f'tell application "System Events" to {verb} {what} of window 1 of '
              f'(first process whose unix id is {pid})' + (f' to {value}' if value else ''))
    return subprocess.run(['osascript', '-e', script], capture_output=True, text=True).stdout.strip()


def launch(app, mode, parity, seconds):
    own, path = app.split(':', 1) if ':' in app else (mode, app)
    env = {'HOME': os.environ['HOME'], 'PATH': os.environ['PATH'], 'ZGX_MODE': own,
           'ZGX_SECONDS': str(seconds + 12), 'ZGX_PARITY': parity}
    return subprocess.Popen([path], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def trial(server, seconds, mode, even_app, odd_app):
    """Paired seconds of `even_app` (left) minus `odd_app` (right)."""
    apps = [launch(even_app, mode, 'even', seconds), launch(odd_app, mode, 'odd', seconds)]
    time.sleep(3)
    for app, (x, y) in zip(apps, PLACES):
        window(app.pid, f'set position to {{{x}, {y}}}')
    time.sleep(0.5)
    for app in apps:
        print('window', app.pid, window(app.pid, 'get {position, size}'), flush=True)
    time.sleep(3.5)
    by = {0: [], 1: []}
    sleep_to_next_second()
    previous = (int(time.time()), cpu_seconds(server))
    end = time.time() + seconds
    while time.time() < end and all(app.poll() is None for app in apps):
        sleep_to_next_second()
        current = (int(time.time()), cpu_seconds(server))
        by[previous[0] % 2].append((current[1] - previous[1]) / (current[0] - previous[0]) * 100)
        previous = current
    for app in apps:
        app.terminate()
        app.wait()
    return [even - odd for even, odd in zip(by[0], by[1])]


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    seconds, mode, a, b = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
    server = subprocess.run(['pgrep', '-x', 'WindowServer'], capture_output=True, text=True).stdout.split()[0]
    first = trial(server, seconds, mode, a, b)
    time.sleep(3)
    # Places swapped: B now runs in even seconds, at the left.
    second = [-d for d in trial(server, seconds, mode, b, a)]
    pairs = first + second
    if len(pairs) < 2:
        sys.exit('too few paired seconds')
    print(f'{mode}: A {a} - B {b}: window server {statistics.mean(pairs):+.2f} points '
          f'(se {statistics.stdev(pairs) / len(pairs) ** 0.5:.2f}, n={len(pairs)}; '
          f'A left {statistics.mean(first):+.2f}, A right {statistics.mean(second):+.2f})')


if __name__ == '__main__':
    main()
