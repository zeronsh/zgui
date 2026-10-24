# Reproducible GUI comparison

The three adapters use the same `comparisons/workload` Rust crate. GPUI is pinned to crates.io **0.2.2**; QuickGUI is pinned to upstream commit **811d6e2816d5229711f59683c4c9dfbb6fc74133**. Their standalone Cargo workspaces isolate large platform dependency graphs from zgui core. Commit the generated Cargo.lock files to preserve transitive versions.

The target UI is a 960×720 logical-pixel dark window. Header: “zgui performance lab”. Three controls append three items, perform two equal writes to the first item, and set done to 999. Count/title/duplicate count match the provided service example. The list initially has capacity four and count zero; append preserves successful earlier writes if the fourth-slot limit is reached, reporting done=-1; the virtual dataset is independently 100,000 synthetic rows. Streaming pane: (20,110), 920×160. List: (20,290), 920×400. Rows: 28px. Font: DejaVu Sans, 14px; header 20px. OS window decorations are outside the content coordinates.

Each current adapter mounts the viewport-derived visible row range plus two overscan rows on each side (17 rows initially and 19 at interior benchmark offsets), rather than allocating 100,000 strings. Earlier archived direct-scene runs used a hardcoded range of up to 20 rows; their frozen source and results remain unchanged. This compares equivalent application-level fixed-height virtualization, not each framework's built-in virtual-list widget. The UI framework's text shaping, retention, layout and rendering still participate. Streaming adds the same ASCII token with its sequence number each tick, retains at most 8192 bytes, and displays the last 735 bytes in seven fixed 105-character lines. Text is clipped by the pane. A timer targets 60 updates/second; scrolling advances 14px per update. Modes are `idle`, `stream`, `scroll`, `both`, selected by `ZGUI_MODE`; `ZGUI_SECONDS` controls process lifetime.

zgui's native comparison adapter now defaults to the retained `wgpu` GPU renderer, alongside GPUI and QuickGUI's GPU renderers. It uses physical-size surfaces, logical-pixel scene coordinates, DPI-aware input, and the same cosmic-text font system for measurement and drawing. `ZGUI_RENDERER=software` explicitly selects the older CPU raster/softbuffer reference path; `--headless` always uses that software path. Native logs identify the selected renderer in JSON and report the GPU adapter on stderr. CPU/RSS results compare these concrete applications and backends, not an intrinsic framework ranking. GPU memory, GPU time, energy, frame latency, presentation count, dropped frames, and visual parity need separate measurement. A process can consume less CPU by failing to present, so always inspect output and independently verify presented frame cadence. Defaults disable costly blur effects in the comparison; measure effects separately with identical settings.

Build once with the `dist` profile, then measure the executable—not `cargo run`. `dist` is what an app ships: `release` with the whole program optimized as one unit (`lto = "fat"`, one codegen unit). zgui's workspace and every comparison adapter define it the same way, so each framework is measured as it would ship, under equal settings. Day-to-day `release` builds stay on thin LTO, which links in half the time; a `dist` build is about 14% smaller (macOS `heavy_workload`: 13.2 MB against 11.4) and runs about 1% fewer instructions. Compare binary sizes between `dist` builds only.

```sh
cargo build --profile dist --locked -p zgui-desktop --example component_workload
cargo build --profile dist --locked --manifest-path comparisons/gpui/Cargo.toml
cargo build --profile dist --locked --manifest-path comparisons/quickgui/Cargo.toml
python3 scripts/compare.py \
  --zgui target/dist/examples/component_workload \
  --gpui comparisons/gpui/target/dist/zgui-compare-gpui \
  --quickgui comparisons/quickgui/target/dist/zgui-compare-quickgui \
  --seconds 30 --warmup 5 --repeats 3 --output comparison.csv
```

Use a real Linux desktop with the same machine, monitor refresh, DPI, font installation, GPU driver, power mode and window visibility. Install DejaVu Sans for all adapters. Do not cover, minimize, resize, interact with, or move benchmark windows. Exclude compilation and first-run shader/cache setup; run once before recording. GPUI requires its native X11/Wayland/font dependencies, and QuickGUI requires current platform libraries and Rust. See [GPUI](https://gpui.rs/) and [QuickGUI upstream](https://github.com/egoist/quickgui) for platform support.

The Linux sampler rotates framework order across repeats, runs all four modes, excludes warmup, samples process-wide CPU time and RSS every 50ms, and writes CSV, per-run logs, and JSON containing raw timestamped CPU/RSS samples and application reports. CPU percentage is relative to one fully occupied core (can exceed 100%); RSS includes resident mapped pages and is not exclusively private memory. Peak RSS is a sampled peak after warmup, not lifetime high-water memory. It does not include child-process memory or GPU allocations. Failed/early-exiting runs stop the harness instead of producing misleading rows. Report median and spread per framework/mode across repeats; preserve hardware, OS, compiler, commit, backend and driver metadata with results. The metadata includes executable SHA256 hashes and a supplementary worktree source snapshot; executable hashes identify the actual measured build when development continues in the worktree. Use `python3 scripts/summarize_comparison.py comparison.csv --output comparison-summary.json` to preserve medians, trial ranges, and delivered tick counts.

No cross-framework speed or memory advantage is asserted before these GUI runs have completed under equivalent conditions. Core headless microbenchmarks establish algorithmic behavior and are not substitutes for this end-to-end comparison.

For deterministic screenshots, set `ZGUI_INITIAL_TICKS=120 ZGUI_MODE=idle ZGUI_SECONDS=10` for every executable. The shared workload first applies 120 combined streaming/scroll ticks, then stays idle. This produces identical text and a scroll offset of 1680px for visual inspection. Clear `ZGUI_INITIAL_TICKS` for measurements. The adapters are functionally matched; text shaping and glyph baselines can differ across the three renderers, so pixel identity is not claimed.

## Iterating on performance

For a change's cost, compare instruction counts of the headless scenes:
`python3 scripts/perf/suite.py ab main .` builds both, interleaves the runs
and marks differences beyond their spread, in about two minutes. Run any
app or bench with `ZGUI_PROFILE=1` for a per-phase line every 120 frames
(update, layout, damage, paint, encode, submit, present: instructions and
time per frame). Off, its phase boundaries cost nothing measurable: an A/B
of all scenes, five runs each, showed every scene within its spread. Time
Profiler helpers and their use are in `scripts/perf/README.md`. The
windowed comparison below stays the evidence for time, GPU and memory.

## The macOS comparison, quick and full

`python3 scripts/run_macos_comparison.py --scene heavy --quick` measures zgui
against GPUI Fast in about ten minutes: two repeats of each active mode, idle
once, frame pacing captured during the sampling trials. Use it to check a
change on the real window path; its window-server numbers read high for
every framework (the capture costs the window server), and the site
builders refuse it. The full run (no `--quick`: three repeats, idle once,
pacing apart, QuickGUI with `--quickgui`) is for publishing.

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

The timer drops missed deadlines rather than processing a backlog of updates; every adapter uses this policy. Logs report completed workload ticks so you can detect unequal delivered work. `ZGUI_SECONDS=0` keeps the UI open for interactive use. Wheel events scroll the viewport independently of the automatic workload. Screenshots in `docs/images` show the shared 120-tick frozen state; font baseline/rasterization differences remain.

## Startup on macOS

`scripts/startup_probe.swift` times a launch to its first content, the same way for every framework: it spawns the binary, waits for its window, then captures the window's top-left strip until the header text has drawn, and reports the process's instructions retired and CPU time at that point. Alternate the frameworks over many launches and compare medians of instructions; wall times swing with system load and the window server.

```sh
swiftc -O scripts/startup_probe.swift -o /tmp/zgui-startup-probe
for i in $(seq 10); do
  for app in target/dist/examples/component_workload comparisons/gpui/target/dist/zgui-compare-gpui; do
    env -i HOME=$HOME PATH=$PATH ZGUI_MODE=idle /tmp/zgui-startup-probe $app
  done
done
```

Within zgui's start, `ZGUI_PROFILE=1` prints the process's instructions and milliseconds at each milestone (run, event loop, menus, app built, window created, GPU adapter, device and pipelines, system fonts, renderer made, text installed, first frame rendered and presented), so each step's cost is the difference between two lines. On the heavy dashboard (October 2026, about 810 M instructions to the first present): the loader and Rust start 54 M, AppKit's application 153 M, its launch 86 M, the window (AppKit's `NSWindow` and title bar) 249 M, the Metal device 64 M, pipelines 22 M, and the first frame's layout, text and paint about 75 M of the 155 M before it is encoded (AppKit orders the window in and displays it meanwhile). Activation then takes about 130 M more before the window's content shows, as it does for GPUI.

`crates/zgui-gpu/examples/startup_bench.rs` isolates the renderer's share headless: the GPU device, the system font database and the first text in four styles.

## Selecting and validating the zgui renderer

`zgui-demo` is `comparisons/demo` (`cargo build --release -p zgui-demo`), apart from the published crates.

```sh
# Native GPU adapter (default); save retained GPU output when the run ends.
ZGUI_MODE=both ZGUI_SECONDS=10 ZGUI_SCREENSHOT=/tmp/zgui-gpu.ppm target/release/zgui-demo
# Explicit CPU reference backend, including HiDPI surface scaling.
ZGUI_RENDERER=software ZGUI_MODE=both ZGUI_SECONDS=10 target/release/zgui-demo
# Deterministic CPU correctness workload; not a native performance result.
ZGUI_TICKS=600 target/release/zgui-demo --headless
```

Do not combine the earlier software-rendered zgui result files with new GPU runs as if they measured the same backend. Record `ZGUI_RENDERER` and the emitted adapter information with new results. Native GPU screenshots use physical surface dimensions; headless screenshots use the fixed 960×720 logical reference raster. On macOS the same application uses winit/wgpu's native platform integration, but this Linux `/proc` sampler cannot collect macOS measurements. Hardware-backed macOS validation remains a separate requirement.

## Public component companion

`component_workload` renders the same content, colors, logical geometry and bounded streaming data through `Application::run`, providers, component state, a caller-owned slot, fluent styles and the public `virtual_list` widget. Its application code does not append or mutate scene nodes. Button actions preserve the example's partial-append and equal-write semantics. The shared timer scheduler targets the same 60 Hz deadlines and drops missed deadlines; idle schedules only the exit deadline. Wheel scrolling remains interactive.

```sh
cargo build --profile dist -p zgui-desktop --example component_workload
ZGUI_MODE=both ZGUI_SECONDS=10 target/dist/examples/component_workload
python3 scripts/compare.py \
  --zgui target/dist/examples/component_workload \
  --gpui comparisons/gpui/target/dist/zgui-compare-gpui \
  --quickgui comparisons/quickgui/target/dist/zgui-compare-quickgui \
  --seconds 30 --warmup 5 --repeats 3 --output component-comparison.csv
```

This is a separate measurement path from the historical `zgui-demo` direct retained-scene results. The component comparison series measure the public component API; direct-scene results do not establish its performance. Preserve the executable hash and label new output as the component adapter; its final JSON includes `"adapter":"components"`. The companion supports `ZGUI_MODE`, `ZGUI_SECONDS` and `ZGUI_INITIAL_TICKS`. It uses the application's native GPU backend; the baseline's `--headless`, `ZGUI_RENDERER=software`, `ZGUI_EFFECTS` and `ZGUI_SCREENSHOT` switches are not implemented here. Capture the native window externally for visual checks.

The companion's built-in list and the shared reference workload now use the same viewport-derived range and two overscan rows on each side: 17 rows initially and 19 at interior benchmark offsets. Heading line height is explicitly 28 px in the reference adapters; body line height remains 20 px. Status labels, child label boxes and deadline-overrun handling are aligned; see the [source audit](results/component-comparison-audit.md). Earlier archived runs used the previous range policy and must not be treated as measurements of this normalized implementation. The component path also includes the production application's accessibility, input, ownership and timer integration; it is not an isolated measurement of component dispatch overhead.

The [normalized component release series](results/component-comparison/README.md) is complete, with raw samples, executable/source hashes, native screenshots and an artifact audit. It is separate from historical direct-scene and pre-normalization smoke runs.

## Current framework series

The [current framework comparison](results/current-framework/README.md) measures the public component workload after the platform, editor and inherited line-height work. All 24 trials completed, with 596–599 active model updates per ten requested seconds. Sampling excluded the first three seconds after process launch and covered 6.990–7.104 seconds per process. The audit recomputed 3,386 samples and verified 156 archived inputs against the measured binaries’ build proofs. Seeded native screenshots were inspected separately; model ticks are not presentation timestamps.

These measurements include each adapter’s backend initialization policy. Current zgui uses primary backends and avoids unused GL initialization; the pinned QuickGUI adapter still maps EGL despite selecting Vulkan for rendering. Its source and reference binary remain unchanged. This comparison is not an isolated measurement of component overhead or GPU memory. Older results and the backend-only ablation remain archived separately; do not infer isolated regressions or savings by subtracting results from different sampling sessions.

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
  -p zgui-desktop --example component_workload
cargo build --release --locked --manifest-path comparisons/gpui/Cargo.toml \
  --target-dir comparisons/gpui/target
cargo build --release --locked --manifest-path comparisons/quickgui/Cargo.toml \
  --target-dir comparisons/quickgui/target
LP_NUM_THREADS=4 python3 scripts/compare.py \
  --zgui target/release/examples/component_workload \
  --gpui comparisons/gpui/target/release/zgui-compare-gpui \
  --quickgui comparisons/quickgui/target/release/zgui-compare-quickgui \
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
