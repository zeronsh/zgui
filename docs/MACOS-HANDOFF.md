# Continue zgx on macOS

Continue in the private `zeronsh/zgx` checkout on a logged-in Mac desktop.
Read repository instructions, then inspect the **current** GitHub Actions results
before starting builds or changing code. Earlier failures and later source fixes
are not interchangeable evidence:

```sh
rtk proxy gh run list --repo zeronsh/zgx --limit 10
rtk proxy gh run view RUN_ID --repo zeronsh/zgx
rtk proxy gh run view RUN_ID --repo zeronsh/zgx --log-failed
```

Download relevant failure artifacts into a fresh directory. Match each run to its
commit and check both Linux and macOS jobs. Do not assume the latest push passed,
or that a passing older commit validates subsequent changes.

## Continuation prompt

Continue practical GPUI capability parity on Linux and macOS. This is completion
of an implemented Rust framework. Read README.md, docs/gpui-parity.md,
docs/ROADMAP.md, docs/native-surfaces.md, docs/platform-services.md,
docs/platform-validation/macos-continuation/README.md,
scripts/macos_native_validation.py and scripts/macos_resource_probe.py.
Use their source-specific evidence and explicit limitations as authoritative.

Preserve the component/children API, lexical providers/slots, fine-grained
subscriptions, retained subtree identity, damage tracking, cached layout, fluent
styling, bounded caches and low idle CPU. The future template language should
lower naturally to these APIs; do not replace them with application-level scene
append calls.

Real Mac execution is now available, not merely cross-compilation. The Apple M5
Max/macOS 26.4 packet records complete Metal-suite execution, real CoreVideo
BGRA/NV12 pixels/reuse/teardown, native component and multiwindow probes, Retina
layout/path/gradient/SVG painting, rich links, decoded-GIF playback/pause/resume,
bidirectional Unicode clipboard exchange with TextEdit, native AX tree/actions/
selection, AppKit dialog results (including two-file selection/cancel) and
nested-sheet owner close. Direct AppKit window queries also verified actual
minimized/restored/hidden/shown visibility and key-window state. Native reopen,
fullscreen/restoration/minimum bounds and OS URL launch/delivery also ran. URL
validation used an explicitly registered test receiver bundle; scheme registration
remains application packaging.

The Mac session exposed and repaired failures that compilation did not reveal:
font-dependent test assumptions, native menu placement/standard application menu,
orphaned owner sheets (including nested overwrite confirmation), application
delegate conflicts, a borderless maximize query producing an event storm, and
stale native dead-key state on editor focus transfer. Consult the packet's exact
source and binary identities before treating any earlier result as validation of
the final fixes. Preserve failed attempts and observations.

Native Canadian dead-key input is **partial IME evidence**: Option-E produced
marked `´`, then E produced exactly one `é` commit. Before the focus fix, composing
in the first editor then typing E in the second incorrectly inserted `é` there.
AppKit input-context discard now gives plain `e` in the second editor; fresh
Option-E/E still commits `é`, yielding `eé`. Attempted Finder AX interaction does not
prove native app deactivation: the final `--native-observe` run stayed
active=true/key=true for all 40 ticks. Obtain a real native active/key transition
before counting app-focus behavior; the same-editor accented attempt is
inconclusive, not a product failure. Escape with this source commits
a literal accent. It is not a substitute for Pinyin candidate cancellation.

The remaining native qualification work is concrete. Items 2–4 have since been probed ([native checks probe](validation.md#macos-cursors-insert-held-drags-file-drops-and-title-bars), 2026-10-28): cursor shapes, Insert, a held drag's Escape, a Finder-style two-file drop (which went to the pointer's last place, now fixed) and the title bar's drag request pass; the window server's move following the real pointer stays unprobed.

1. Resolve the pending explicit permission/access request for a Pinyin input
   source. Do not silently change system input-source settings. Use three fresh
   `native_ime` launches (each closes after 20 seconds): candidate commit with
   exactly one COMMIT; cancellation with unchanged committed model; editor/app
   focus transfer without a stale commit. Capture candidate placement near the
   caret, including after window movement, and retain PREEDIT/COMMIT/FINAL logs.
   A separate explicit permission request for native CGEvent input is pending for
   held-mouse, real app-focus and Insert cases unsupported by the available tool.
   Neither this request nor the Pinyin setting request is implicitly granted;
   respect input-tool restrictions until authorization/access is resolved.
2. Independently inspect native cursor shapes. Actual NSMenuItem checked/enabled
   transitions now have native property readback, command and replacement proof;
   checkmark pixels were not captured. The automation screenshot's own
   pointer indicator does not prove the native cursor. The available Mac
   automation does not synthesize Insert, so obtain another real input path or
   record it as unverified.
3. Finish held drag-preview/Escape cancellation and Finder multi-file drop,
   cancellation and region-leave behavior. Atomic automation drags verified three
   accepted card payloads and outside-drop rejection, but not those other cases.
   Two-file dialog selection/cancellation has separate observed evidence; it is
   not Finder drag/drop evidence.
4. Validate custom-titlebar dragging using a true held-native mouse gesture. The
   available automation left both custom and standard decorated AppKit titlebars
   stationary. Diagnostics showed LeftMouseDown/button 0 but
   `NSEvent.pressedMouseButtons == 0`; this is a gesture-tool limitation, not a
   confirmed framework drag failure or pass. A tentative synchronous-scope change
   did not help and is excluded from the final production fix set. Use the
   `native_platform --decorated` control and event diagnostics for continuation.
   Resize/maximize/restore/fullscreen/minimum bounds and actual AppKit
   hide/show/minimize/focus states have evidence. The [same-scale display packet](platform-validation/macos-continuation/zgui-macos-display-01.tar.gz)
   records physical monitor 0→1→0 with exact bounds settled for ten 100 ms
   observations at each destination, both displays 5120×2880 at scale 2.0.
   Mixed-scale transitions, hotplug, machine sleep/wake and real driver loss
   remain distinct checks; no pixel-restoration proof follows from bounds.
   Record unavailable hardware cases explicitly.
5. Read the completed [current-source native GIF packet](platform-validation/macos-continuation/zgui-macos-gif-02.tar.gz):
   `animated_image <path.gif>` decoded and presented three full-canvas frames;
   a freshly bound distinct wrapper paused identical preview pixels for 46.082
   seconds, resumed to two other colors and closed with exit 0. Input, source
   and executable hashes are retained. This closes simple native decode/playback/
   pause integration. Complex offsets/disposal/loop-limit decoder tests remain
   separate; this does not establish cadence or invisible GPU-submission pause.
   The integrated gallery evidence is a suite of native component fixtures,
   not a claim that one all-in-one application was exercised.
6. Run affected regressions after changes, then formatting, workspace tests/
   doctests and strict Clippy. Execute the complete GPU/native bundle in a fresh
   output directory, for example:

   ```sh
   rtk proxy python3 scripts/macos_native_validation.py --output /tmp/zgx-macos-validation-NEXT
   ```

   `--interactive` launches individual checklist cases; pending/skipped cases
   must not become passes. Existing separately recorded GUI observations belong
   to their recorded binary/source, even if an automated bundle still lists its
   manual checklist as pending. Generated animation frames are not GIF-decoding
   evidence. Use actual UI automation or operator observations for native input.
7. Preserve source archives/diffs, executable hashes, raw logs, useful captures,
   and explicit automated/observed/failed/pending/skipped status. Update the
   capability matrix and current documentation from those observations. Review
   every finite completion gate; test counts, model-update logs and passing
   cross-compilation do not prove full platform parity.

Linux X11/XIM has a recorded IBus 1.5.29 default synchronous-bridge
interoperability limit: post-commit preedit and Escape lifecycle signals are
omitted before zgx receives them. The [diagnostic packet](platform-validation/macos-continuation/zgui-linux-native-ime-followup.tar.gz)
preserves the failure, raw boundary traces and rejected private winit experiment.
Original, unpatched zgx/winit passes Pinyin commit/cancel/editor transfer and
candidate placement with task-owned `IBUS_ENABLE_SYNC_MODE=0`, plus external-model/
read-only cancellation and ordinary key recovery. The [official IBus fix](https://github.com/ibus/ibus/commit/719792d300579c1bfdf43251a83c6ed4e5594c07)
addresses missing show/hide preedit handling. The smoke harness now selects and
records asynchronous mode; this does not fix or qualify the affected default
synchronous configuration. No winit production patch was adopted.

The Linux implementation and its historical evidence remain intact. The latest
matched Linux performance packet is docs/results/gpui-parity-workers4/README.md:
36 GPUI 0.2.2 / QuickGUI / zgx trials, 10,808 audited samples and active model
updates at 59.6–59.95/s. It measures software Vulkan on a shared Linux host, not
Mac hardware rankings or presented frames. Keep historical evidence immutable.

A matched GPUI/QuickGUI/zgx rerun for the final stabilized source remains a
completion gate. Most continuation changes are native Mac behavior or validation,
but shared RGBA iteration also changed from `chunks_exact` to equivalent typed
chunks. The prior result remains valid for its own source; semantic equivalence
and the newer zgx-only Mac resource probes are not a new matched measurement.
Use an equivalent recorded comparison environment and fresh result directory;
do not relabel historical Linux rankings as measurements of the final source.

The Linux `/proc` sampler does not run on macOS. For a Mac resource smoke probe,
build `component_workload` in release mode, stop all builds and other validation,
keep its window visible, and use a fresh directory:

```sh
rtk cargo build -p zgx-desktop --release --example component_workload --locked
rtk proxy python3 scripts/macos_resource_probe.py --binary target/release/examples/component_workload --output /tmp/zgx-macos-resources-NEXT
```

This probe preserves Mach-time CPU counters, RSS/physical-footprint samples,
source/binary identities and delivered-model-work checks. Its idle 1% reference
is an observation criterion, not a universal performance limit. It is not a
matched comparison, GPU allocation/energy measurement, presentation-cadence
check or long-duration leak proof. Any Mac comparison needs equivalent reference
workloads and its own evidence; do not weaken delivered-work gates or drop failed
trials. Consult the continuation packet for any already completed resource probe
before scheduling redundant work.

Use parallel subagents for independent bounded tasks when useful; coordinate
shared-file edits and Cargo builds. Push reviewed fixes to the private repository
and inspect their Actions results. Continue until all required completion gates
have evidence; never mark completion while required native checks remain pending.
