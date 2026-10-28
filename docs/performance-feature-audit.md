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

Separate trials first put the window server's CPU for zgx's heavy dashboard
above GPUI Fast's in `both` and `stream`: 7.4 points over the desktop alone
against 3.6 in `both` (October 2026). Paired measurements on a quiet desktop
(below) settle it. With two apps' windows side by side taking turns,
`stream` and `scroll` cost the same as GPUI Fast's (−1.4 ± 0.8, −1.0 ± 0.7),
and `both` costs at most about 2 points more (+1.8 ± 0.9, +3.8 ± 0.5,
+1.6 ± 0.3). The same pairing of zgx against itself reads ±1.5, so that is
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

What zgx does, and what was checked in `both` against `stream` (no
difference):

- One layer change a frame. Counting every CALayer, NSView and NSWindow
  change, and cursor and input-context calls, between 3 and 8 seconds of
  either mode finds exactly 60 `setContents:` a second and nothing else.
- No frame without damage. zgx presents only when a flush reports damage;
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
# ZGX_PARITY=even|odd; unset, it always runs). Self-pairing first:
python3 scripts/windowserver_pair.py 60 both target/dist/examples/heavy_workload target/dist/examples/heavy_workload
python3 scripts/windowserver_pair.py 60 both target/dist/examples/heavy_workload comparisons/gpui-fast/target/dist/heavy
# One app switching its own configuration by the second (a scratch toggle
# reading the Unix second's parity):
python3 scripts/windowserver_alternate.py 60 path/to/app
```

Run them on a quiet desktop, with nothing else drawing. The terminal needs
Accessibility access, so the pair script can place the windows.

**What remains: every commit reaches every context.** Since tiles (each
surface a sublayer, shown by flipping `hidden`), zgx's window server CPU
still trails the best rival by 0.2 to 0.8 points in every 60 Hz cell (M5
suite of 2026-10-26: heavy, lab and vlist), while it wins animate. Each tile
flip is one Core Animation commit, and a commit, explicit or implicit
(`CATransaction flush`), sends one message to every Core Animation context in
the process. The heavy workload has 11: its window, a text-input window, an
empty one, and 8 `NSMenuBarReplicantWindow`s, which AppKit makes when an app
with a main menu activates (here with 2 displays). So a flip sends one
1048-byte commit for the window and ten 72-byte ones for nothing.

- **Cost.** Twenty more empty contexts on even seconds cost +1.32 ± 0.21
  points with 60 commits a second, and +0.44 ± 0.18 at idle, which is only
  their creation. That is about 0.045 points per context at 60 commits a
  second, so about 0.45 for zgx's ten extra.
- **It comes with the menu bar.** A bare AppKit app has 2 contexts. Give it a
  one-item main menu and activate it, and it has 10. The number of items
  doesn't matter, and `setMainMenu(nil)` afterwards doesn't remove them. The
  GPUI Fast of the earlier comparisons had no menu (3 contexts). It has one
  since #692 but presents without commits, so it never pays.
- **No layer property changes it,** and no commit can be scoped to one
  context. What escapes it is a present without a commit. A `CAMetalLayer`
  drawable is one image-queue ping: with a pool of three it costs the window
  server −0.3 ± 0.2 against tiles in heavy `stream`. With two the pool is
  always full, which costs +4.4. At 60 Hz it has so far paced worse than tiles
  (p99 24.5 against 19 to 23 ms, 0.8% long frames). The video layer also
  enqueues without a commit, but it keeps a third surface, wakes a CoreMedia
  thread and costs app CPU (+9.5 to 11.6 MB and +16 to 23% CPU at full rate),
  which is too high a price for 0.45 points.

Frames below the display's rate stay on tiles. Drawables presented at the
refresh the host's latch picks (`presentDrawable:atTime:`) were tried and are
not in the shipped path.

**Which path, per machine (`ZGX_PRESENT_DRAWABLE=auto`, opt-in).** `auto`
tries both once per GPU, display, refresh rate and OS build, keeps the
verdict in the app's defaults (`ZGX_PRESENT_RETRIAL=1` tries again), and
full-rate frames take it. Each path runs 240 frames. Drawables win only
where tiles skip clearly more of their flips (5 points of the frames). With
nothing capturing the screen, tiles skip about 0.16% on an M1 Max, so in
practice `auto` keeps tiles.

It doesn't judge by energy, because a process can't measure it. It can read
its CPU energy, and its GPU busy time and the window server's, but not the
GPU's power. What a busy second costs depends on the GPU's clock: fitted
from system GPU power against busy time, tiles against drawables (2026-10-28),
it is 0.78 W on an M5 Max and 4.44 W on an M1 Max. With one weight, the M5
Max's trial chose drawables, which measure worse there. `ZGX_TRIAL_RULE=energy`
still runs that rule, uncalibrated.

**When to choose drawables (`ZGX_PRESENT_DRAWABLE=1`).** For a GPU-heavy
app drawing at the display's full rate on an M1-class Mac. There drawables
cost the GPU about 150 mW less and the window server 5.7 points less, for
about 4.7 points more of the app's CPU (animate, M1 Max: GPU 206 against
358 mW, window server 31.7% against 37.4, app CPU 18.0% against 13.3). On
an M5 Max, tiles cost less at full rate (the window server's CPU 1.3 points
and the app's 0.8 lower, for 4–7 mW more GPU). At 60 Hz, drawables trade
0.5 points of window-server CPU for 0.6 of the app's.

Drawables are paced by the view's own display link and kept in phase
(skipping a tick when frames start to wait a refresh for a drawable), from
a pool of two. Their frame shows 16 ms after it starts, against 27–32 before
(M5 Max).

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

## Kept frames: what they patch, and what waits (October 2026)

A window draws its frame whole once and keeps it (#441). While the scene
repaints only leaves, a frame makes just those items' instances again
and writes them into place; with damage over a tenth of the window it is
drawn from the kept instances (about 30 instructions an item rather than
1,500). The heavy dashboard's `stream` mode renders 10% fewer
instructions. A window that loses its kept frame to a change no patch
makes keeps one again after two frames in a row that would have patched
(#446).

A plain box among the repainted leaves (a flat colour, or a box without
decoration, shadows or a pass over its content, outside any clip region)
is made straight into its place by the quad and clip code the frame loop
uses, rather than through the frame loop: about 900 instructions a box,
found and made, rather than 1,250. Heavy `stream` renders 6% fewer instructions again
(3,150 K to 2,955 K a frame), and a single bound colour write about 1.5%
fewer.

One that layout alone moved or resized (the flush marks it `BOX_ONLY`)
and that is drawn as one quad keeps its quad's colours, corners and
border. Only its box is written, with its shading, its cull against the
clip and its draw worked out again. That costs about a third of making it
again, and heavy `stream` renders 7% fewer instructions (2,424 K to
2,243 K a frame).

A repainted subtree (a container repainting, a composite change, nodes
mounted or removed under it) is walked again from what its root
inherited, its items made as a frame makes them, and spliced over the
range its old items held (#615). Splicing pays only while the subtrees
are under a quarter of the window's items and the damage covers at least
half of it. Below that, drawing the frame as usual walks less, and the
frame is drawn so. The heavy dashboard's `both` mode (a feed streaming
while its list scrolls, every frame) renders 7.9% fewer instructions at 1×
and 2×, and 11.6% fewer at 1.5×, where the scroll can't be copied. Its
`stream` mode renders 2.9% fewer. Its frames no longer walk the whole
window.

Moving the items of rows that only translated, instead of making them
again, was measured and dropped. It is exact only on whole device pixels
at a power-of-two scale, and checking and moving each row's quads costs
more than making them again from their cached shapes. With it, heavy
`both` rendered 5% more than with plain splicing.

Anything else draws whole: a layer, an effect, a shader fill or polyline.

Drawing from the kept frame, the frame's list holds every draw in the
window, where a frame drawn as usual holds only what its damage reaches.
A batch for a damage region spans every instance between the draws it
takes: draws outside the region and texts' spare room, which the GPU sets
up only to scissor away. Splicing put heavy `both` on that path (#615): it
submitted 41% more instances for the same damage, and its GPU time on an
RTX 4090 rose from 249 to 297 µs a frame. Three changes bound it again:

- A batch from the kept frame ends before 32 such instances
  (`SKIPPED_SPLIT`), at one or two draw calls a frame more.
- Spare room goes only to texts a kept frame patched, a quarter of their
  glyphs (at least 2). On that GPU the empty quads cost more than the
  instance count suggests. A text outgrowing its room moves its quads to
  the kept frame's end, and its draws keep their places in paint order.
  A batch spans or merges quads only in the paint-ordered prefix
  (`Kept::ordered`).
- The kept frame's quads stay in the vertex buffer they were last written
  to (`upload::KeptQuads`, `MappedRing::write_kept`). A buffer still
  holding them takes only the runs written since. Each slot records which
  kept frame it holds and as of which patch, so a slot made again never
  matches. Writes are logged only while the kept frame's length holds
  steady between frames drawn from it.

Heavy `both` submits 2,961 instances (3,172 before #615) and its GPU time
is back to 249 µs. Heavy `stream` submits 2,557 (5,334), at 198 µs (264).
Render instructions fell 2.6–3% on both, and 5% on lab `stream`.

## Accepted costs

- **A moved or resized leaf's damage as one area** (fe7a9886, October 2026)
  costs `widget_bench` +0.45% retired instructions: 3,148 → 3,163 M, all of
  it the new leaf path (with it off, 3,147.5 M), not the inlining that came
  with it. The bench mounts 1,500 widgets, each placed once, which pays the
  two-area work without a pair to merge. Kept: the same change cut the heavy
  dashboard's prepare by 1.4%, and the cost is under the gate's 1%. Found by
  `suite.py ab 7d559ad origin/main` and bisecting.
- **A scroll copy of antialiased edges by an odd number of device
  pixels** (found with #800, October 2026) differs from a fresh render by a
  level or two in those edges' pixels: invisible, and kept. The quad shader
  sizes its antialiasing by fine derivatives (`fwidthFine`, `dpdxFine` in
  `draw.wgsl`), and a pixel's fine derivative is the difference with its
  neighbour in its 2×2 block, left or right by its parity: a curved edge
  drawn an odd number of pixels away has another neighbour, so a slightly
  different width. The copy itself is exact (`copy_offset`'s `textureLoad`),
  and even shifts reproduce a fresh render. Making odd shifts exact would
  take an analytic antialiasing width (every rounded edge's pixels change,
  and transformed content still needs the derivatives) or copying only even
  shifts (half the copies gone): neither is worth two levels on an edge.
  Tests that compare copies with a fresh render use square content, or
  content that moves with its copy by even device pixels.
- **A region repainted through a window's surfaces** (found with #834,
  October 2026) can differ from the scene drawn whole by a level on rounded
  corners, and by up to five on glyphs, at fractional offsets: the same
  class, invisible, and kept. Scroll copies and partial repaints switched
  off change nothing in it. `layer_frames`' random scrolling scenes allow
  six levels on edges for both; anything drawn in the wrong place differs
  by far more (every bug it found was off by 80 or more).
