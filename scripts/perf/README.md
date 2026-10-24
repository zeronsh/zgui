# Performance tools

Measure in retired instructions, not time: a fixed workload's count repeats
to well under 0.5%, where its time moves by several percent with the
machine's state. Time and GPU numbers come from the windowed comparison
(`scripts/run_macos_comparison.py`) on a quiet desktop.

## A change's cost, in two minutes

```sh
python3 scripts/perf/suite.py                  # every headless scene, this checkout
python3 scripts/perf/suite.py ab main .        # main against this checkout's files
python3 scripts/perf/suite.py ab abc123 def456 # two commits
python3 scripts/perf/suite.py --only heavy ab main .
```

The suite runs each headless scene (heavy dashboard in three modes, lab
stream and scroll, animate, script mount, anime, layout, list state, GPU
layout, frame, text, style, widgets; `--all` adds the 10-second state and
core benches) a few times and reports the least disturbed run's
instructions with the spread above it. `ab` builds each revision in a
worktree of its own under `/tmp/zgx-perf` (kept: the next comparison
builds incrementally), interleaves the runs, and marks differences larger
than either side's spread. `suite.py dirs A B` compares two directories of
examples built otherwise from the same source (other `RUSTFLAGS`, a
profile, an allocator). `suite.py clean` removes the worktrees.

## Where a frame's work goes: `ZGX_PROFILE=1`

Any zgx app or bench run with `ZGX_PROFILE=1` prints, every 120 frames,
what each phase cost per frame:

```text
profile 120 frames: | update 3555.4k instr 191.5 us | layout 3182.2k instr 145.0 us | damage 2285.1k instr 134.2 us | paint 3090.2k instr 324.5 us | encode 737.8k instr 35.0 us | submit 17.5k instr 4.6 us
```

Each phase's numbers leave out the phases it encloses (text shaping during
damage resolution counts as damage). Instructions are the calling thread's.
Unset, a phase boundary costs one relaxed load, and the suite shows no
scene moving (`docs/benchmarking.md`). Building with
`--features zgx/profile-nested` also times styles and text within their
phases (a check in every style and text run, so never in normal builds).
On macOS the phases are `os_signpost` intervals (subsystem `dev.zgx`):
record with Instruments' os_signpost instrument to see them on a timeline
beside a Time Profiler. At startup it also prints a line per milestone
(window created, renderer made, first frame presented, ...) with the
process's instructions and time so far (`zgx::profile::mark`).

## Where a phase's work goes: samples

```sh
scripts/perf/record.sh /tmp/heavy.trace 5 -- target/release/examples/heavy_headless
scripts/perf/record.sh /tmp/app.trace 10 --attach $(pgrep -n my_app)
python3 scripts/perf/samples.py /tmp/heavy.xml --children=render_once
python3 scripts/perf/samples.py /tmp/heavy.xml --under=Host::draw --incl
python3 scripts/perf/samples.py /tmp/app.xml --threads
python3 scripts/perf/lines.py /tmp/heavy.xml target/release/examples/heavy_headless render_flat
python3 scripts/perf/frames.py target/release/examples/heavy_headless zgx::scene
```

`record.sh` wraps xctrace's Time Profiler and exports the samples.
`samples.py` aggregates them by function: self, inclusive, under a
function, its callees, its callers, or per thread. `lines.py` resolves the
samples inside one function to source lines and inlined helpers (build with
`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`). `frames.py` lists stack
frame sizes, which finds functions paying for large values on the stack.

## Profile-guided builds

`python3 scripts/perf/pgo.py -p PACKAGE --bin NAME [TRAINING COMMAND ...]`
builds a `dist` binary trained on zgx's headless scenes (and the commands,
`{bin}` being the instrumented binary) into `target/dist-pgo/`.
`docs/getting-started.md` has the recipe for an app outside the repository.
The published comparison builds neither zgx nor GPUI this way.

## The real window path, quickly

```sh
python3 scripts/perf/preflight.py           # is the machine quiet? what runs?
python3 scripts/perf/preflight.py --state   # displays, wallpaper, power mode, build, uptime
python3 scripts/run_macos_comparison.py --scene heavy --quick   # ~10 min, zgx vs GPUI Fast
```

The comparison waits for the preflight, hides Zeron, and records both in
its audit; see `docs/benchmarking.md`.

## The window server

`scripts/windowserver_pair.py` and `scripts/windowserver_alternate.py`
measure the window server's CPU for two apps, or two configurations of one,
in paired seconds; see `docs/performance-feature-audit.md`.
