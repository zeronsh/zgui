# Reproducible GUI comparison

The three adapters use the same `comparisons/workload` Rust crate. GPUI is pinned to crates.io **0.2.2**; QuickGUI is pinned to upstream commit **811d6e2816d5229711f59683c4c9dfbb6fc74133**. Their standalone Cargo workspaces isolate large platform dependency graphs from zgx core. Commit the generated Cargo.lock files to preserve transitive versions.

The target UI is a 960×720 logical-pixel dark window. Header: “zgx performance lab”. Three controls append three items, perform two equal writes to the first item, and set done to 999. Count/title/duplicate count match the provided service example. The list initially has capacity four and count zero; append preserves successful earlier writes if the fourth-slot limit is reached, reporting done=-1; the virtual dataset is independently 100,000 synthetic rows. Streaming pane: (20,110), 920×160. List: (20,290), 920×400. Rows: 28px. Font: DejaVu Sans, 14px; header 20px. OS window decorations are outside the content coordinates.

Each current adapter mounts the viewport-derived visible row range plus two overscan rows on each side (17 rows initially and 19 at interior benchmark offsets), rather than allocating 100,000 strings. Earlier archived direct-scene runs used a hardcoded range of up to 20 rows; their frozen source and results remain unchanged. This compares equivalent application-level fixed-height virtualization, not each framework's built-in virtual-list widget. The UI framework's text shaping, retention, layout and rendering still participate. Streaming adds the same ASCII token with its sequence number each tick, retains at most 8192 bytes, and displays the last 735 bytes in seven fixed 105-character lines. Text is clipped by the pane. A timer targets 60 updates/second; scrolling advances 14px per update. Modes are `idle`, `stream`, `scroll`, `both`, selected by `ZGX_MODE`; `ZGX_SECONDS` controls process lifetime.

zgx's native comparison adapter now defaults to the retained `wgpu` GPU renderer, alongside GPUI and QuickGUI's GPU renderers. It uses physical-size surfaces, logical-pixel scene coordinates, DPI-aware input, and the same cosmic-text font system for measurement and drawing. `ZGX_RENDERER=software` explicitly selects the older CPU raster/softbuffer reference path; `--headless` always uses that software path. Native logs identify the selected renderer in JSON and report the GPU adapter on stderr. CPU/RSS results compare these concrete applications and backends, not an intrinsic framework ranking. GPU memory, GPU time, energy, frame latency, presentation count, dropped frames, and visual parity need separate measurement. A process can consume less CPU by failing to present, so always inspect output and independently verify presented frame cadence. Defaults disable costly blur effects in the comparison; measure effects separately with identical settings.

Build once with the `dist` profile, then measure the executable—not `cargo run`. `dist` is what an app ships: `release` with the whole program optimized as one unit (`lto = "fat"`, one codegen unit). zgx's workspace and every comparison adapter define it the same way, so each framework is measured as it would ship, under equal settings. Day-to-day `release` builds stay on thin LTO, which links in half the time; a `dist` build is about 14% smaller (macOS `heavy_workload`: 13.2 MB against 11.4) and runs about 1% fewer instructions. Compare binary sizes between `dist` builds only.

```sh
cargo build --profile dist --locked -p zgx-desktop --example component_workload
cargo build --profile dist --locked --manifest-path comparisons/gpui/Cargo.toml
cargo build --profile dist --locked --manifest-path comparisons/quickgui/Cargo.toml
python3 scripts/compare.py \
  --zgx target/dist/examples/component_workload \
  --gpui comparisons/gpui/target/dist/zgx-compare-gpui \
  --quickgui comparisons/quickgui/target/dist/zgx-compare-quickgui \
  --seconds 30 --warmup 5 --repeats 3 --output comparison.csv
```

Use a real Linux desktop with the same machine, monitor refresh, DPI, font installation, GPU driver, power mode and window visibility. Install DejaVu Sans for all adapters. Do not cover, minimize, resize, interact with, or move benchmark windows. Exclude compilation and first-run shader/cache setup; run once before recording. GPUI requires its native X11/Wayland/font dependencies, and QuickGUI requires current platform libraries and Rust. See [GPUI](https://gpui.rs/) and [QuickGUI upstream](https://github.com/egoist/quickgui) for platform support.

The Linux sampler rotates framework order across repeats, runs all four modes, excludes warmup, samples process-wide CPU time and RSS every 50ms, and writes CSV, per-run logs, and JSON containing raw timestamped CPU/RSS samples and application reports. CPU percentage is relative to one fully occupied core (can exceed 100%); RSS includes resident mapped pages and is not exclusively private memory. Peak RSS is a sampled peak after warmup, not lifetime high-water memory. It does not include child-process memory or GPU allocations. Failed/early-exiting runs stop the harness instead of producing misleading rows. Report median and spread per framework/mode across repeats; preserve hardware, OS, compiler, commit, backend and driver metadata with results. The metadata includes executable SHA256 hashes and a supplementary worktree source snapshot; executable hashes identify the actual measured build when development continues in the worktree. Use `python3 scripts/summarize_comparison.py comparison.csv --output comparison-summary.json` to preserve medians, trial ranges, and delivered tick counts.

No cross-framework speed or memory advantage is asserted before these GUI runs have completed under equivalent conditions. Core headless microbenchmarks establish algorithmic behavior and are not substitutes for this end-to-end comparison.

For deterministic screenshots, set `ZGX_INITIAL_TICKS=120 ZGX_MODE=idle ZGX_SECONDS=10` for every executable. The shared workload first applies 120 combined streaming/scroll ticks, then stays idle. This produces identical text and a scroll offset of 1680px for visual inspection. Clear `ZGX_INITIAL_TICKS` for measurements. The adapters are functionally matched; text shaping and glyph baselines can differ across the three renderers, so pixel identity is not claimed.

## Iterating on performance

For a change's cost, compare instruction counts of the headless scenes:
`python3 scripts/perf/suite.py ab origin/main .` builds both, interleaves the runs
and marks differences beyond their spread, in about two minutes. Run any
app or bench with `ZGX_PROFILE=1` for a per-phase line every 120 frames
(input, script, update, layout, damage, paint, encode, submit, present:
instructions and time per frame; `zgx::profile::take` gives a bench the
same split for its own intervals). `apps_bench` (zgx-script) measures the
example apps' common interactions that way, each to its frame drawn: mail's
open, search and archive, the dashboard's feed tick, sort and filter,
Sketch's drag, pan and zoom among 500 shapes, and the code editor's
keystroke, scroll and find-all keystroke on 100,000 lines; CI's instruction
gate counts each app as a scene, small and undrawn, run by `list_state_bench`
(`APP` set) so that it links no binary of its own. Off, its phase boundaries cost nothing measurable: an A/B
of all scenes, five runs each, showed every scene within its spread. Time
Profiler helpers and their use are in `scripts/perf/README.md`. The
windowed comparison below stays the evidence for time, GPU and memory.

## The macOS suite: every scene in under an hour

`python3 scripts/run_macos_suite.py` measures zgx against GPUI Fast, MyGo and Rune
on four scenes, one after another, into
`docs/results/suite-macos-<date>-<commit>/<scene>/`:

- `heavy`: the live dashboard (idle, stream, scroll, both);
- `lab`: streaming text beside a 100,000-row list (stream, scroll, both);
- `animate`: dots and a shimmer redrawn every display refresh;
- `vlist`: 100,000 chat rows of wrapped text, 1 to 8 lines, some with an image,
  in each framework's own variable-height list, moved by a steady scroll, flings,
  and a scroll with a jump every 30 ticks.

The frameworks: GPUI Fast, the faster of the two GPUI configurations and one
apps ship with (`gpui = { package = "gpui-fast" }`); MyGo (egoist/mygo) through
its native UI; Rune (anomalyco/rune: Rust, wgpu, Taffy, Parley), pinned, with
a 14-line patch exposing its own wheel path so a run can scroll it
(`comparisons/rune/PATCH.md`); and zgx, through its Rust measured list in vlist and through
the same list in Luau (`ui.list { measured = true }`, framework `zgx-luau`). `comparisons/README.md` is the
contract each adapter keeps (environment, report, window, build).

Its protocol (`run_macos_comparison.py --suite`) is shorter than the full one:

| | Suite | Full |
| --- | --- | --- |
| a trial | 5 s warm-up, 10 s sampled | 5 s warm-up, 15 s sampled |
| repeats | 3, alternating order; 1 more for a cell whose CPU or GPU power spreads over 10% of its median | 3 |
| idle | heavy only, 2 launches sampled 20–30 s after launch | every scene, 5 launches, 20–35 s |
| frame pacing | 3 captures of 8 s per cell (of two, one bad capture moved a median: lab scroll on 2026-10-25 read 30.0 and 17.8 ms, and ten more captures of the same build 18.0) | 8 |
| blank rows | vlist, scroll and jump: one 8 s capture of the window at 10 Hz per cell | none |

An app that exits abnormally (a signal, or no report before the runner ends
it) is launched once more, in a timed trial, a pacing capture or a blank-row
capture alike, and the run records each such exit beside the launches of its
framework (`abnormal_exits`, `launches` in audit.json), which the site states.
GPUI Fast aborts now and then inside AppKit while ScreenCaptureKit captures its
window (`-[NSWindow _updateButtonsForWindowSharingSession]`, about 1 launch in
12 on 2026-10-25); its crash dialog still stops the next preflight until it is
dismissed by hand.

Every trial is checked: it must run as long as asked (its exit within 3 s of
`ZGX_SECONDS`) with the ticks that length makes (58–61 a second while active,
none while idle; animate's frames matching the rate it reports), or the run is
rejected. The runner sets each variable under its old `ZGUI_` name too, for a
build from before the rename.

**Read a run only within itself.** Two checks on the heavy dashboard, with
the same binaries (`docs/results/suite-protocol-check-2026-10-25/`):

- A 3 s warm-up read 5–10% more CPU for every framework than the full
  protocol does: startup's own work. The warm-up went back to 5 s.
- With 5 s, the suite and the full protocol, run in the same quiet window on
  the same GPUI Fast binary, agree: CPU 18.46 against 17.57–18.32, GPU 22.02
  against 21.94–22.18, memory 271 MB against 258–275 MB. Both read GPUI Fast's
  CPU 15% above a run of the same binary a few hours earlier (15.73–15.95):
  the session differed, not the protocol. A run's frameworks share its session,
  so their comparison holds; a cell compared across runs may not.

### GPU: the whole GPU's power, not the app's GPU time

The suite's GPU figure is the GPU's power over each trial's sample
(macsample's `gpu_system_watts`: IOReport's energy model, "GPU Energy"), less
the desktop alone (each repeat's `desktop` baseline, about 0.6 mW on a quiet
machine). It is the whole GPU, so it includes the window server compositing
the app's window: a framework pays for the way it presents frames (an AV
sample-buffer layer, a plain layer, a `CAMetalLayer`) as well as for drawing
them. With Zeron or anything else drawing, it counts that too, which is why
the suite hides Zeron and checks the desktop baseline.

It replaced app GPU time (`gpu_mcycles_per_second`: the process's
`accumulatedGPUTime` at the GPU's mean active clock), which the suite still
records and the site shows second. On 2026-10-25 a Metal System Trace of the
animate scene (speed-of-light, `xctrace record --attach` to staged binaries)
showed why it misleads for small, frequent frames:

- Per frame, first command start to last end, zgx's GPU executes 46 µs (one
  command buffer, one render encoder: vertex about 21 µs, fragment about
  28; 0.11 Mpx damaged, 5.6 draws, 84 instances), GPUI Fast's 318 µs (it
  draws the whole window).
- zgx's `accumulatedGPUTime` was 61 ms a second against 6.1 ms executed:
  about 0.47 ms charged per submission beyond its work, the commit-to-start
  latency of a GPU waking from idle (about 0.6 ms for both apps). GPUI Fast's
  charge is 0.1–0.2 ms a frame beyond its work.
- It moves with the desktop: with Zeron visible, which keeps the GPU awake,
  zgx read 11.2–11.7 and GPUI Fast 26.1–26.5 million cycles a second, where
  quiet they read 23.0 and 17.8.
- GPU power, quiet: zgx 0.116–0.120 W, GPUI Fast 0.130–0.146 W.

What GPU power still charges zgx for: the window server's GPU time for zgx's
animation is 9.1–9.2% of the GPU against GPUI Fast's 4.9–8.3%
(`windowserver_gpu_percent`), the compositing of its AV sample-buffer layer.
GPU power counts that cost, against zgx.

Its noise, across the 2026-10-25 suite's repeats: every cell of heavy, lab and
vlist within 4% of its median (most within 2%), the desktop 0.5–0.8 mW;
animate the widest (zgx 114.7–121.2 mW, MyGo 117.0–122.3, GPUI Fast
127.8–145.6). In animate zgx and MyGo are within each other's range (zgx's
median 1% above), so the site says it is not a clear lead; the page lists
every cell where zgx's range reaches a rival's.

Blank rows (`scripts/blankprobe.swift`): during a capture of the window, a
run down the rows' left padding that is closer to the window's background
than to either row colour, longer than 24 points, is a place no row was
drawn. It needs Screen Recording permission, which it only checks (it never
asks). Each run also records the agent app's own processes (`preflight.zeron`),
whose UI drew frames while hidden on 2026-10-25.

## Microbenchmarks: one state change, one scroll step

`crates/zgx-desktop/examples/micro_bench.rs` times two operations headless,
per operation, in the process's CPU time (all threads), as the median, p10,
p90, minimum and maximum over batches after a warm-up. Their constants are
`zgx_workload::micro`, which every framework's port takes.

- `SCENE=state`: a static grid of 40 × 25 texts under one text bound to a
  counter (`Count: N`), 960 × 720. An operation sets the counter.
- `SCENE=vscroll-fixed` and `SCENE=vscroll-variable`: 10,000 rows in a 720 ×
  800 viewport, either 28 pt rows of text (`virtual_list`) or the vlist scene's
  rows (1–8 wrapped lines, about one in four with an image), measured as they
  mount (`measured_virtual_list`). An operation scrolls 12 pt, and goes back
  to the top past the end.

Each reports two figures:

- **frame**: the operation and everything up to a frame ready to present
  (layout, damage, and the renderer's encode and submit; the GPU is settled
  between batches, untimed). **This is the headline, and the figure the engine
  work chases.**
- **write**: the operation alone, with whatever it runs at once (zgx's
  retained bindings update their nodes as the value is set). It is reported so
  a fast write cannot hide a slow frame, and is never made faster at the
  frame's expense.

`RENDER=0` measures text as a renderer would and draws nothing, the frame's
CPU work without a GPU, as CI's instruction gate counts it (`micro-state`,
`micro-vscroll-fixed`, `micro-vscroll-variable`, at 300 operations each).

`micro_luau_bench` runs the same scenes written in Luau and loaded through
`ScriptHost` (`support/micro_luau.rs`), a binary of its own so `micro_bench`
links no script engine: the counter is a `ui.state` its
handler (`ui.command`) adds to, the lists are `ui.list` rows built by the
script, each row's text read from the host with `ui.call_sync`, and a scroll
step moves the list's `ui.shared` offset as a host would. The JSON says
`"framework":"zgx-luau"`. What it adds to Rust's figures is the script's own
cost: running the handler, and building each row that scrolls in from Luau
(its tables, props and lowering). The gate counts these as `micro-*-luau`.
Engine changes that move these keep the public API and ship only if they make
every measured figure faster.

A Luau scene's count also follows where the heap places things: the value
cache and Luau's tables hash by address, and its collector marks in their
order, so a change that resizes any allocated type moves it a few hundredths
of a percent either way (one pair of builds read +0.025%, −0.006% and
−0.125% under three placements of glibc's allocator). The gate counts a Luau
scene that moves more than 0.01% again under two other placements
(`GLIBC_TUNABLES`), each side, gates on the median, and marks it `layout`
where the three disagree in sign.

**The floor under a frame with the GPU encode.** Most of zgx's `RENDER=1`
frame is what any Metal frame costs. `crates/zgx-gpu/examples/mtl_floor.rs`
times a bare one (a command buffer, one empty render pass into a 2048 × 1536
target, scissor, end, commit, waiting for it before the next):

| on this Mac (2026-10-25) | process CPU | main thread | Metal's own threads |
| --- | --- | --- | --- |
| back to back | 44–94 µs | 12–24 µs | about 30–70 µs |
| one frame per 16.7 ms | about 209 µs | about 80 µs | about 130 µs |

The main thread retires the same 70,000 instructions either way; paced frames
take longer because each starts on cold caches and a clocked-down core.
zgx's own work over the floor is about 15,000 instructions drawing and 18,000
presenting, beside its frame without the encode. So judge renderer changes by
the main thread's retired instructions, and quote a frame's process CPU against
this floor.

## Changes to zgx, A/B on a Mac: cells

The comparison measures frameworks; `scripts/perf/cells.py` measures
candidate changes to zgx against each other, windowed. A cell is a base commit
with PR heads merged in and environment variables set. Cells merging the same
commits share one binary, so a change behind a toggle is A/B'd with no build
noise.

```sh
python3 scripts/perf/cells.py build cells.json   # builds each distinct cell from git
python3 scripts/perf/cells.py run cells.json     # measures, interleaved (quiet desktop)
python3 scripts/perf/cells.py report             # /tmp/zgx-cells/out/results.json
```

```json
{"base": "e5d796d1",
 "cells": {"base":       {"merge": ["5f242aaf"]},
           "base+warm":  {"merge": ["5f242aaf"], "env": {"ZGX_GPU_WARM": "1"}},
           "timer":      {"merge": ["4b06f949", "5f242aaf"]}},
 "parts": {"gestures": ["base", "base+warm", "timer"],
           "animate":  ["base", "base+warm"],
           "scroll":   ["base", "timer"]},
 "rounds": {"gestures": 4, "animate": 3, "scroll": 3}}
```

Builds happen in a worktree of their own under `$ZGX_CELLS_DIR` (default
`/tmp/zgx-cells`), and merges stay local. A run waits for a quiet desktop
(`preflight.py`). It launches staged copies from `/tmp` (`staging.py`), and
it stops at a system dialog without touching it. There are three parts:

- **gestures**, the first frame after idle. The mail example runs under
  `zgx-run --inspect`. Each round idles 4 s, so the window trims (at 1.5 s),
  then plays a wheel tape (`scripts/perf/tapes/`, modeled flings) through
  `zgx inspect replay`, twice, and samples 5 s at rest. Each cell launches
  twice a round:
  - **With `ZGX_PRESENT_LOG=1`**, for the timings only: the first frame after
    each trim is checked for being forced whole (`render forced=`), caught up
    from the shown surface (`draw … catch_up=`) and warmed. Then its render
    to present and render to second present are read.
  - **Without the log**, for late frames (over 1.5 refreshes), CPU and
    instructions per frame shown, and CPU at rest. The log samples the layer
    tree every 4 ms on the main thread: 1.25% of a core and about 32 M
    instructions a second on a window otherwise at 0.002%.

  Render to second present is the time the second frame shows. First to
  second present widens whenever the first frame shows sooner, so judge by
  render to second.
- **animate**: the animation workload at the display's rate, without the log.
  This gives CPU, GPU power, instructions and WindowServer CPU (`macsample`).
  One short logged launch per cell counts warm-ups; it has no input, so there
  should be none.
- **scroll**: the heavy and lab workloads scrolling, as 15 s trials plus
  pacing captures (`framepace`: p99, long intervals).

Pacing depends on the display: a 60 Hz Mac (most M1 MacBooks) cannot judge
pacing at 120 Hz, so those cells need a ProMotion Mac.

## The macOS comparison, quick and full

Sampling starts from window readiness: the runner polls for up to 2.5 seconds
until the app has a window that is frontmost and uncovered, then warms it for
five seconds before the sample. It only activates the benchmark app; system
dialogs stop the run without being touched. Apps get three extra seconds of
lifetime; readiness taking over 2.5 seconds is a slow launch, retried once.
Timeouts and covered windows are retried once too. Exhausted retries are
recorded without a sample and reject the audit, while other cells continue.
Logs retain readiness time,
and the audit records the sampling anchor and actual requested lifetime. A
shortened sample is rejected. Startup remains a separate measurement.

`python3 scripts/run_macos_comparison.py --scene heavy --quick` measures zgx
against GPUI Fast in about ten minutes: two repeats of each mode, frame
pacing captured during the sampling trials. Use it to check a
change on the real window path; its window-server numbers read high for
every framework (the capture costs the window server), and the site
builders refuse it. The full run (no `--quick`: three repeats of each active
mode, five of idle, pacing apart, QuickGUI with `--quickgui`) is for
publishing. `--modes idle --into docs/results/<run>` measures the idle cells
alone and replaces a published run's with them (its first idle trials move to
`idle-5-20s/`; its desktop baselines and pacing stay as they were, the
re-run's baselines going to `idle-rerun-baselines/`; and `audit.json` records
the re-run under `idle_rerun`).

The PGO supplement measures every framework built with profile-guided
optimization, each app trained on its own scene:
`python3 scripts/perf/pgo_compare.py generate`, then `train` (each app's
scene in a window for 15 s: run it on a quiet desktop), then `use` (into
`target/pgo-compare/`), and `run_macos_comparison.py --scene <scene> --pgo
--modes stream,scroll,both --no-pacing` (or `--modes animate` for animate)
for a CPU, GPU and memory column beside the main run, written to
`latest-macos-<scene>-pgo-<date>-<commit>`.

### Apps run from /tmp, never from ~/Documents

Every macOS runner (`run_macos_comparison.py`, `trace_macos_gpu.py`, `windowserver_pair.py`, `macos_resource_probe.py`, PGO training) launches a copy of each app staged under `/tmp/zgx-staged/<content hash>/`, with `/tmp` as its working directory (`scripts/staging.py`). macOS protects `~/Documents` (privacy, TCC). An unsigned GUI app is its own responsible process, and each ad-hoc build of it is a new identity, so an app launched from a checkout under `~/Documents`, or with its working directory there, can stop mid-run to ask for access to the folder. The dialog sits over the window being measured.

Never answer such a dialog from a script. On 2026-10-25 a scripted "Don't Allow" wedged the privacy service: every new file access under `~/Documents` blocked for 53 minutes, until the Mac was restarted. The preflight (`scripts/perf/preflight.py`) reads the system dialogs on screen (privacy prompts, crash reports) and waits, logging their text, until a person dismisses them.

Two probes read the checkout by design and launch their app as a direct child: `macos_native_validation.py` (examples run in the repository root) and `macos_accessibility_probe.py` (zgx-run opens the example scripts). If a prompt appears during one of them, stop and dismiss it by hand.

### Why idle is sampled 20–35 s after launch

Any process that has used the GPU lately holds about 180 MB of Metal driver
memory ("Owned physical footprint (unmapped) (graphics)"), and the driver
gives it back a few seconds after the process stops drawing. Active cells sit
on that floor throughout; an idle window gets it back partway through a
5–20 s sample, so that mean measures *when* more than what the app holds.
Sampled every 100 ms over 25 s of 41 idle launches on 2026-10-24
(`heavy_workload`, `component_workload` and GPUI Fast's `heavy` and lab app):

- The floor came back about 6 or about 8 s after launch, for zgx and GPUI
  Fast alike. Which one follows the desktop's state, not the app: an
  unchanged zgx lab build gave it back at 8 s in 5 of 5 launches, and at
  6 s in all 7 two hours later. zgx's own idle trim, after 300 ms, 1.5 s or
  never, changed neither (21 launches).
- What a window then settles to is bimodal too, a step of about 25 MB
  apart: zgx heavy 54 or 79 MB, GPUI Fast heavy 70 or 95; zgx lab 41 or
  66, GPUI Fast lab 53 or 78.
- The 5–20 s means of the same binary ranged 92–117 MB (zgx heavy) and
  87–116 MB (GPUI Fast lab) between launches.

So idle is now sampled 20–35 s after launch, when every framework has had
its floor back, five times per framework (the step between settled levels
still shows: report the median and the range). Each idle trial keeps the
5–20 s figures too, as `early_*` fields in its JSON and in `current.csv`,
and `idle-5-20s.json` summarizes them: runs before 2026-10-24 sampled idle
that way, once.

Both start with a preflight (`scripts/perf/preflight.py`): they wait while
anything heavy runs (compilers and linkers of any agent, VMs, Instruments,
Spotlight indexing, Time Machine, software updates) or the rest of the
machine uses more than 75% of a core (agents' resident engines idle near
50%), logging what they find, and they hide
Zeron's windows for the run. `audit.json` records what the preflight found
and the machine's state: displays (count, resolution, refresh), a live
wallpaper, power mode, macOS build and uptime, beside the desktop-alone
baseline each repeat measures. Those explain a shift between days in the
desktop's own cost.

## Validation environment

On this development machine native windows were smoke-tested using Xvfb `:98` with Openbox and Mesa llvmpipe (LLVM 21.1.8, Mesa 26.0.8). GPUI required a window manager to present rather than a black mapped window. QuickGUI startup encountered an unresponsive desktop portal on the inherited D-Bus session; the isolated smoke runs used `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent`, `DISPLAY=:98`, and removed `WAYLAND_DISPLAY`. These are environment-specific test workarounds, not recommended desktop settings. This software Vulkan environment is unsuitable for claims about native GPU performance.

The timer drops missed deadlines rather than processing a backlog of updates; every adapter uses this policy. Logs report completed workload ticks so you can detect unequal delivered work. `ZGX_SECONDS=0` keeps the UI open for interactive use. Wheel events scroll the viewport independently of the automatic workload. Screenshots in `docs/images` show the shared 120-tick frozen state; font baseline/rasterization differences remain.

## Startup on macOS

`scripts/startup_probe.swift` times a launch to its first content, the same way for every framework: it spawns the binary, waits for its window, then captures the window's top-left strip until the header text has drawn, and reports the process's instructions retired and CPU time at that point. Alternate the frameworks over many launches and compare medians of instructions; wall times swing with system load and the window server.

```sh
swiftc -O scripts/startup_probe.swift -o /tmp/zgx-startup-probe
for i in $(seq 10); do
  for app in target/dist/examples/component_workload comparisons/gpui/target/dist/zgx-compare-gpui; do
    env -i HOME=$HOME PATH=$PATH ZGX_MODE=idle /tmp/zgx-startup-probe $app
  done
done
```

Within zgx's start, `ZGX_PROFILE=1` prints the process's instructions and milliseconds at each milestone (run, event loop, menus, app built, window created, GPU adapter, device and pipelines, system fonts, renderer made, text installed, first frame rendered and presented), so each step's cost is the difference between two lines. On the heavy dashboard (October 2026, about 810 M instructions to the first present): the loader and Rust start 54 M, AppKit's application 153 M, its launch 86 M, the window (AppKit's `NSWindow` and title bar) 249 M, the Metal device 64 M, pipelines 22 M, and the first frame's layout, text and paint about 75 M of the 155 M before it is encoded (AppKit orders the window in and displays it meanwhile). Activation then takes about 130 M more before the window's content shows, as it does for GPUI.

`crates/zgx-gpu/examples/startup_bench.rs` isolates the renderer's share headless: the GPU device, the system font database and the first text in four styles.

Measured again in October 2026 (dist builds, instructions retired, the lab window at rest), zgx's start against GPUI Fast's:
- **Overall:** over the first 2.5 s, zgx retires about 966 M instructions and GPUI Fast about 902 M (7% more for zgx). zgx's peak footprint is lower, 229.6 against 241 MB, and the wall time to content is the same within noise.
- **zgx to its first present, about 754 M:**
  - the loader and Rust start: 55 M;
  - wgpu's adapter, 40–52 M, and its device, about 65 M, both on a background thread while AppKit launches;
  - the event loop and menus: about 114 M;
  - AppKit's launch up to the window request: about 88 M;
  - AppKit making the `NSWindow`: about 217 M;
  - the first frame: about 145 M.
- **Inside the GPU steps:**
  - `BufferWrites::new` (vendor/wgpu-hal), about 12 M, is the process's first shader compile.
  - `MetalFrame::new` in the first frame was about 15 M, about 10.5 M of it naga translating `draw.wgsl` and `copy_offset.wgsl` to MSL. Their MSL is now translated ahead (`src/draw.metal`, `src/copy_offset.metal`, kept current by a test), which cut the first frame by about 10 M. Metal's `newLibraryWithSource` is 0.4 M in-process; Metal compiles out of process and caches binaries itself.
- **What doesn't account for the gap:**
  - Linking WebKit, AVFoundation and CoreMedia: about 2 M at launch.
  - The accessibility adapter: 2–5 M.
- **What does:** the gap with GPUI's direct Metal is mostly wgpu's adapter and device setup.
- **The menu bar:** AppKit's first `setMainMenu` is about 46 M on the main thread, and building the menus' items about 27 M more. winit's default menu bar paid the first at launch, only to be replaced. Now zgx makes its menu bar once the first frame shows (at once with no window about to show one), and winit makes none. The first present went from about 742 M to 657 M instructions (median of 6 launches), and from about 89 to 80 ms.

## On Windows

`scripts/windows_compare.py` runs the comparison scenes (`comparisons/README.md`) for zgx, GPUI Fast, MyGo and the older GPUI adapter in turn on the same desktop, the Windows counterpart of the macOS runner:
- **Scenes and modes:** `lab` (`idle`, `stream`, `scroll`, `both`), `animate`, `heavy` (`both`) and `vlist` (`scroll`, `fling`, `jump`), each framework's binary as `comparisons/README.md` names it; `busy` for the old GPUI adapter. A scene runs for the frameworks whose binary is there.
- **Samples**, every 100 ms after a 3 s warm-up, for 10 s: CPU time (`GetProcessTimes`), CPU cycles (`QueryProcessCycleTime`, finer than CPU time's scheduler ticks), working set and private bytes.
- **Valid runs only.** A run must last as long as it was asked and report what it did (`workload_ticks`, `"ticks"` or `animation_frames`), at least nine tenths of 60 a second outside `idle`; an invalid run is run again once, then recorded as invalid. Cycles are given per tick or frame too, so a run that did less cannot look cheaper.
- **Order.** The frameworks alternate in rotating order over 3 rounds; a scene and mode whose cycles spread wider than 5% of their median gets another round, up to 6. It prints a table and writes `summary.json` with every run, the binaries' sizes and hashes.

```powershell
cargo build --profile dist --locked -p zgx-desktop --example component_workload --example animation_workload --example heavy_workload --example vlist_workload
cargo build --profile dist --locked --manifest-path comparisons/gpui-fast/Cargo.toml
cd comparisons/mygo; ./build.sh; cd ../..   # Go 1.27, with CGO_ENABLED=0 as build.sh sets it
python scripts/windows_compare.py --zgx target/dist/examples --gpui-fast comparisons/gpui-fast/target/dist --mygo comparisons/mygo/bin --output target/windows-compare
```

Startup and keystroke latency are measured apart:
- **`scripts/windows_startup_probe.py`** times a launch to its first content as macOS's probe does: spawn, the window, then its header strip until the header's text has drawn, read as the window drew it (`PrintWindow`), so a window covering it cannot pass for its text. It reports medians of wall time, CPU time and cycles over launches in rotating order; `--settle 5` then keeps each at rest five seconds and takes its private bytes and working set.
- **`scripts/windows_latency_probe.py --press`** times a key press to the composed screen. Use `key_workload` (zgx-desktop's example) against GPUI's `keys`: each redraws a counter at every key.

Each run also records whether a UI Automation client read zgx's window (zgx prints `accessibility: a client asked for the tree` when `ZGX_ACCESSIBILITY_LOG` is set, which the harness sets), and the summary says whether a screen reader ran (`SPI_GETSCREENREADER`) and which known UIA client processes did (the shell's touch keyboard host, Voice Access, Magnifier, test tools). Real desktops have such clients, so runs measure the machine as it is; zgx's accessibility work for them is part of its cost, and GPUI Fast and MyGo have no UI Automation provider. Compare only runs whose clients match. To size that work, `ZGX_ACCESSIBILITY=0` runs zgx with no provider at all: a diagnostic, never a figure to publish, since users get one.

Each run's window is brought to the front and must be uncovered as sampling begins and ends (a window opened from a process in the background stays behind the foreground one, and a covered window cost the compositor about three times as much and its own process far less to present); Zeron's windows are minimized for the runs. Keep the machine otherwise idle and on mains power, and repeat a comparison: the same binary's cycles a frame have moved by 15% between sessions on one machine (`animation_workload`, 2.73, then 3.16 after a day of GPU driver stress tests, then 2.66 after a reboot), so compare only runs that alternate within one session. A process's private bytes include the graphics driver's floor, which differs between Direct3D 12 (zgx) and Direct3D 11 (GPUI Fast, MyGo): see `vendor/wgpu-hal/ZGX_PATCH.md` and [the Windows notes](windows.md).

The runner holds Windows' High performance power plan for its runs and restores the user's after (`--power-plan keep` leaves it). Under Balanced, Windows clocked a busy UI thread up and down from run to run, and since the cycles it counts are time, one binary's cycles a frame spread 30% (5% under High performance). Each run records the processors' clock against nominal (`processor_performance_percent`).

## Selecting and validating the zgx renderer

`zgx-demo` is `comparisons/demo` (`cargo build --release -p zgx-demo`), apart from the published crates.

```sh
# Native GPU adapter (default); save retained GPU output when the run ends.
ZGX_MODE=both ZGX_SECONDS=10 ZGX_SCREENSHOT=/tmp/zgx-gpu.ppm target/release/zgx-demo
# Explicit CPU reference backend, including HiDPI surface scaling.
ZGX_RENDERER=software ZGX_MODE=both ZGX_SECONDS=10 target/release/zgx-demo
# Deterministic CPU correctness workload; not a native performance result.
ZGX_TICKS=600 target/release/zgx-demo --headless
```

Do not combine the earlier software-rendered zgx result files with new GPU runs as if they measured the same backend. Record `ZGX_RENDERER` and the emitted adapter information with new results. Native GPU screenshots use physical surface dimensions; headless screenshots use the fixed 960×720 logical reference raster. On macOS the same application uses winit/wgpu's native platform integration, but this Linux `/proc` sampler cannot collect macOS measurements. Hardware-backed macOS validation remains a separate requirement.

## Public component companion

`component_workload` renders the same content, colors, logical geometry and bounded streaming data through `Application::run`, providers, component state, a caller-owned slot, fluent styles and the public `virtual_list` widget. Its application code does not append or mutate scene nodes. Button actions preserve the example's partial-append and equal-write semantics. The shared timer scheduler targets the same 60 Hz deadlines and drops missed deadlines; idle schedules only the exit deadline. Wheel scrolling remains interactive.

```sh
cargo build --profile dist -p zgx-desktop --example component_workload
ZGX_MODE=both ZGX_SECONDS=10 target/dist/examples/component_workload
python3 scripts/compare.py \
  --zgx target/dist/examples/component_workload \
  --gpui comparisons/gpui/target/dist/zgx-compare-gpui \
  --quickgui comparisons/quickgui/target/dist/zgx-compare-quickgui \
  --seconds 30 --warmup 5 --repeats 3 --output component-comparison.csv
```

This is a separate measurement path from the historical `zgx-demo` direct retained-scene results. The component comparison series measure the public component API; direct-scene results do not establish its performance. Preserve the executable hash and label new output as the component adapter; its final JSON includes `"adapter":"components"`. The companion supports `ZGX_MODE`, `ZGX_SECONDS` and `ZGX_INITIAL_TICKS`. It uses the application's native GPU backend; the baseline's `--headless`, `ZGX_RENDERER=software`, `ZGX_EFFECTS` and `ZGX_SCREENSHOT` switches are not implemented here. Capture the native window externally for visual checks.

The companion's built-in list and the shared reference workload now use the same viewport-derived range and two overscan rows on each side: 17 rows initially and 19 at interior benchmark offsets. Heading line height is explicitly 28 px in the reference adapters; body line height remains 20 px. Status labels, child label boxes and deadline-overrun handling are aligned; see the [source audit](results/component-comparison-audit.md). Earlier archived runs used the previous range policy and must not be treated as measurements of this normalized implementation. The component path also includes the production application's accessibility, input, ownership and timer integration; it is not an isolated measurement of component dispatch overhead.

The [normalized component release series](results/component-comparison/README.md) is complete, with raw samples, executable/source hashes, native screenshots and an artifact audit. It is separate from historical direct-scene and pre-normalization smoke runs.

## Current framework series

The [current framework comparison](results/current-framework/README.md) measures the public component workload after the platform, editor and inherited line-height work. All 24 trials completed, with 596–599 active model updates per ten requested seconds. Sampling excluded the first three seconds after process launch and covered 6.990–7.104 seconds per process. The audit recomputed 3,386 samples and verified 156 archived inputs against the measured binaries’ build proofs. Seeded native screenshots were inspected separately; model ticks are not presentation timestamps.

These measurements include each adapter’s backend initialization policy. Current zgx uses primary backends and avoids unused GL initialization; the pinned QuickGUI adapter still maps EGL despite selecting Vulkan for rendering. Its source and reference binary remain unchanged. This comparison is not an isolated measurement of component overhead or GPU memory. Older results and the backend-only ablation remain archived separately; do not infer isolated regressions or savings by subtracting results from different sampling sessions.

## Preserving failed trials

The sampler writes raw post-warmup samples and failure metadata before propagating
a failed process, timeout, missing tick report or launch error. Failed trials
have `summary: null` and never become successful CSV rows. Evidence-write errors
remain visible; a full disk can prevent any artifact from being saved.
Run `python3 scripts/test_compare.py -v` to check these paths.

The [attempted longer refresh](results/refined-framework/README.md) stopped amid
disk exhaustion and unequal update rates. It is preserved as a failed attempt,
not a replacement for the earlier complete comparison.

The [complete retry](results/refined-framework-retry/README.md) preserved 36 trials
and 10,793 samples, but rejected one scrolling trial that delivered only
54.95 updates/second. Unrelated builds restarted on the shared host during
sampling. No new CPU/RSS ranking is accepted from either refresh attempt.

## Four-worker measured-framework series

The [new 36-trial series](results/measured-framework-workers4/README.md) applies
`LP_NUM_THREADS=4` identically to all three adapters. It passed the unchanged
58–61 logical-updates/second gate (observed 59.4–59.95), with 10,808 audited
samples and 228 archived sources. The report includes median and min/max CPU/RSS,
exact source/binary proofs, and the preserved preregistration. Unrelated builds
were observed on the shared host; this supports only the recorded software-driver
condition, not isolated worker-count effects or hardware/macOS rankings. The
comparison workload still uses fixed-height rows. Historical rejected attempts
remain rejected and are not pooled with this series.

### Fresh builds without archived temporary paths

These reproduce the archived series as it was built (`release`, before the
`dist` profile); new series use `dist` as above.

From the repository root, build each adapter from its locked sources. The explicit
target directories below make the executable paths independent of an inherited
`CARGO_TARGET_DIR` and of the archived host's `/tmp` layout:

```sh
cargo build --release --locked --target-dir target \
  -p zgx-desktop --example component_workload
cargo build --release --locked --manifest-path comparisons/gpui/Cargo.toml \
  --target-dir comparisons/gpui/target
cargo build --release --locked --manifest-path comparisons/quickgui/Cargo.toml \
  --target-dir comparisons/quickgui/target
LP_NUM_THREADS=4 python3 scripts/compare.py \
  --zgx target/release/examples/component_workload \
  --gpui comparisons/gpui/target/release/zgx-compare-gpui \
  --quickgui comparisons/quickgui/target/release/zgx-compare-quickgui \
  --seconds 20 --warmup 5 --repeats 3 --output fresh-comparison.csv
```

Run on a working Linux desktop and use a new output filename for every series.
Record the display, GPU/driver, loader, common environment, source and executable
hashes; `LP_NUM_THREADS=4` only defines the software-driver worker condition when
the relevant Mesa driver is in use. These general commands do not recreate the
archived private Xvfb or patched-loader setup. The frozen workers4 runner and its
[supplemental build script](results/measured-framework-workers4/build-orchestration.py)
document that exact host-specific setup and require the corresponding archived
source snapshot. Later documentation and CI changes were not in its measured build.
