# Feature expansion: default costs and resource bounds

This source audit records checks made during the GPUI capability expansion. It does not replace the matched streaming/list benchmarks and does not claim universal performance superiority.

## Default paths

- Ordinary row/column/overlay nodes continue to use the original cached layout algorithm. Extended layout has an optional shared options block; default styles allocate no block. Only containers requiring the advanced algorithm allocate a retained Taffy tree. The tree retains immediate child IDs and removes obsolete generations.
- `Style` and `QuadStyle` are now `Clone`, rather than `Copy`, because optional extended data is shared. Default clones do not allocate an extended block. Extended options retain their shared allocation across unrelated dimension refinements; an integration regression checks pointer identity after a reactive width update. Reusing a canonical scene options allocation skips redundant normalization.
- Plain text retains its original shaping path. Display overflow/clamp options are compact default values; rich shaping is selected for nondefault display options or explicit rich text. Empty font fallbacks share one empty allocation, and default font features contain no allocation.
- Visibility observers are registered by consumers, not for every scene node. They run after geometry revision changes. Hidden animated images release their timer task. Hidden-focus ancestry checks run only after a layout/geometry change. Idle frame preparation publishes no observer updates.
- Explicit cursors maintain an exact count. When the last custom cursor is cleared or removed, `cursor_at` returns through the no-cursor fast path without hit-test allocation. A regression covers changing, clearing and subtree removal.
- A drag session exists only during an active gesture. Preview components are retained and translated, not rebuilt on pointer movement. Completion, cancellation, disabled/hidden source and removal dispose the preview. No drag timer is allocated.

## Raster caches

Canvas and SVG caches each enforce **32 MiB of raster pixels and 1,024 entries**, evicting least-recently-used entries before insertion. Entry count independently bounds metadata even for one-pixel rasters; tests fill each cache with 1,025 one-pixel entries and check count, bytes and oldest-entry eviction.

Canvas entries use `Weak<Canvas>`, so removed scene nodes release command and path storage even if a hidden host has not painted again. The weak control block can remain until cache eviction, but the source object's owned buffers are dropped. Generated decoration canvases need no strong retention after rasterization: unchanged detailed styles reuse the raster directly. Accepted cached decoration styles contain at most 1,024 gradient stops and 32 shadows. With 1,024 entries, their retained variable-size style arrays are bounded by 1,048,576 gradient-stop records and 32,768 shadow records, plus constant-size metadata. Shared style allocations are counted conservatively here; application-owned scene/style objects have their own lifetimes.

SVG entries retain a stable numeric source identity and tint, not XML bytes. Source identity survives clone, tint and affine refinement. A pure transform change reuses pixels even after the previous source wrapper is dropped. Unchanged transform hits also reuse the same `Arc<ImageData>` rather than allocating a new image wrapper. Source inputs are limited to 4 MiB each. Lifetime regressions remove sources before the next paint and confirm that cached rasters do not keep source payloads alive.

## Other bounded data

- Async image cache: at most 256 entries, configurable pixel budget capped at 64 MiB, 1,024-byte keys; abandoned pending requests are removed. Decoded animations permit at most 4,096 frames and 64 MiB of decoded pixels, with bounded nonzero frame durations.
- Canvas rasterization rejects more than 65,536 commands; paths reject more than 65,536 segments. Clip depth is bounded and mask storage has a separate 32 MiB check. These checks bound renderer work; caller-owned builders remain ordinary owned Rust data.
- Native grouped file transfers reject oversized complete batches rather than truncating them. X11/macOS assembly permits 1,024 paths and 1 MiB of encoded path bytes. Wayland additionally bounds URI transfer bytes and read duration. `FilesDropRejected` exposes `TooManyFiles`, `TooLarge`, `InvalidData` or `TimedOut`.
- Virtualized rows continue to own only the mounted range and overscan. Removing rows releases component registrations, tasks, visibility observers, previews and scene nodes. Renderer caches hold bounded pixels/metadata rather than keeping removed Canvas/XML source payloads alive until a later presentation.

The integrated regression suite and final benchmark packet record the tested revision. These bounds describe framework-owned storage; application models, active SVG/path sources, driver allocations and external font databases are not included in a cache's pixel counter.

## The window server's cost of heavy content (macOS)

Separate trials first put the window server's CPU for zgui's heavy dashboard
above GPUI Fast's in `both` and `stream`: 7.4 points over the desktop alone
against 3.6 in `both` (October 2026). Paired measurements on a quiet desktop
(below) settle it. With two apps' windows side by side taking turns,
`stream` and `scroll` cost the same as GPUI Fast's (−1.4 ± 0.8, −1.0 ± 0.7),
and `both` costs at most about 2 points more (+1.8 ± 0.9, +3.8 ± 0.5,
+1.6 ± 0.3). The same pairing of zgui against itself reads ±1.5, so that is
the method's own spread.

What the window server's cost depends on, from a synthetic app swapping
IOSurfaces as a layer's contents at 60 Hz:

- Swapping at all: about 6.5 points against no swap. Most of the cost comes
  with each swapped frame.
- The swapped layer's size: a full-window swap costs about 1.4 points more
  than a 64 or 256-point sublayer's. Splitting the window into tiles or
  damage-sized layers could recover no more than that.
- What changed in it: nothing. A whole window changed costs the same as a
  256-point square changed (−0.4 ± 0.35).

What zgui does, and what was checked in `both` against `stream` (no
difference):

- One layer change a frame. Counting every CALayer, NSView and NSWindow
  change, and cursor and input-context calls, between 3 and 8 seconds of
  either mode finds exactly 60 `setContents:` a second and nothing else.
- No frame without damage. zgui presents only when a flush reports damage;
  with nothing drawn, the shown surface stands (`present_layer`).
- Handing a frame over as soon as it is done instead of at its point in the
  refresh: −0.7 ± 0.6.
- Repainting the whole window instead of the damage: −0.2 ± 0.5.
- No cursor, IME-area or redraw requests while the workload runs, and
  accessibility inactive.

The present mechanism: blitting each frame into a `CAMetalLayer` drawable, as
GPUI presents, costs the window server 5.7 ± 0.6 points more in `both` than
the plain layer's contents swap, and 1.6 points more app CPU. The video layer
(`AVSampleBufferDisplayLayer`) costs about 2 points less in `both` than the
plain layer and nothing less in `scroll`, for about 0.6 points more app CPU
(and 60 Hz frames pace worse on it; see `layer_present.rs`).

**How to measure it.** Separate trials can't resolve this on a busy desktop:
the desktop alone swung from 41% to 55% of a core between runs. Alternate
two configurations on even and odd Unix seconds, and credit the window
server's CPU time (`ps -o time=`) in each whole second to the configuration
that ran: the desktop's drift cancels between neighbouring seconds. Within
one app, switch its configuration by the second. Between two apps, run their
windows side by side without overlapping (an overlap costs the covered one),
have each run its workload in seconds of one parity, and repeat with their
places swapped. With 60 seconds a run, the paired difference has a standard
error of about 0.3 to 0.6 points. Pair an app with itself as a control
before trusting a difference smaller than the control's.

Two scripts do this; check any window server claim with them, not with
single runs:

```sh
# Two apps, side by side, taking turns (the heavy workload honours
# ZGUI_PARITY=even|odd; unset, it always runs). Self-pairing first:
python3 scripts/windowserver_pair.py 60 both target/dist/examples/heavy_workload target/dist/examples/heavy_workload
python3 scripts/windowserver_pair.py 60 both target/dist/examples/heavy_workload comparisons/gpui-fast/target/dist/heavy
# One app switching its own configuration by the second (a scratch toggle
# reading the Unix second's parity):
python3 scripts/windowserver_alternate.py 60 path/to/app
```

Run them on a quiet desktop, with nothing else drawing. The terminal needs
Accessibility access, so the pair script can place the windows.

## The heavy dashboard's malloc fragmentation (macOS)

What the heavy dashboard's malloc zones hold beyond their live allocations
(`vmmap -summary`: dirty less allocated) is about 8 MiB, windowed and
headless alike (2026-10-24). Investigated and closed:

- About 2.5 MiB are holes in pages that still hold live objects: 135–160
  pages under half full, as many before and after the memory work of
  #354–#361 and #373. Survivors spread thin across many size classes keep
  them resident (1,536, 768, 512, 384 and 3,584 bytes, and small blocks), so
  no one allocator stands out.
- The rest is the allocator's own resident slack: size-class rounding and
  chunks kept for reuse. `malloc_zone_pressure_relief` after startup frees
  nothing, and mimalloc holds about 1 MB more, not less.
- Fewer live allocations do not bring footprint down one for one: #354–#361
  cut the live heap by 3.3 MiB and fragmentation rose by 2.1, so footprint
  fell by about 1 MB.
- `heavy_headless` reads its frame back every 60 ticks to settle the GPU, a
  15.6 MiB allocation per readback: a burst of the benchmark's own, which the
  windowed app never makes and which the large-allocation path frees whole.

Page occupancy came from `heap --addresses=all --noContent` (live blocks and
their sizes, grouped by 16 KB page). Under `MallocStackLogging` macOS uses
other size classes and `malloc_history -allBySize` lists VM mappings too
(a fallback font's clean mapping once read as heap), so attribute sizes
without it.
