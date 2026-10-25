# Reaching the floor: structural plan (October 2026)

What a frame costs today, what a program that knows its own layout pays
for the same pixels, and the internal changes that close the gap. Nothing
here changes the public API, Rust or Luau.

## Rules every change follows

1. **No public API change.** `Styles`, `View`, signals, the Luau surface and
   their behaviour stay as they are. Everything below is internal.
2. **Faster on every metric, never a trade.** A structural change ships only
   if it is no worse on any tracked metric: suite CPU, GPU power, memory,
   pacing, WindowServer, startup, the gate scenes, apps_bench and the new
   microbenchmarks.
3. **Nothing breaks.** All tests, pixel goldens, layer_frames and native
   probes pass, and output is exact: pixel-identical frames, bit-identical
   bounds and damage. The window path (`debug_for_layer`, present) is the
   one that is measured.
4. **No benchmark-only paths.** A fast path must be the general case a
   real app hits (a colour binding, a scrolled list), not a shape only a
   microbenchmark has.

## Where a frame goes

Retired instructions a frame, macOS, `sol_probe` (scratch probe in
`~/sol-tools`), bundled DejaVu, main after #484, medians of 5.

| scene | update | prepare | flush | render | total |
|---|---|---|---|---|---|
| heavy both | 2,395 K | 1,586 K | 1,414 K | 3,881 K | 9,276 K |
| heavy stream | 2,218 K | 1,510 K | 1,370 K | 3,205 K | 8,304 K |
| heavy scroll | 127 K | 162 K | 10 K | 372 K | 670 K |
| one colour write, alone | 8.2 K | 2.5 K | 2.9 K | 107 K | 121 K |
| one colour write, beside heavy | 8.2 K | 2.5 K | 2.9 K | 116 K | 130 K |

**The floor.** `sol_floor` (branch `sol/floor`, scratch) captures one full
zgx frame of heavy stream, then each tick writes only what changed into the
instances: bar heights, progress fills, pill colours, and the glyphs of
texts whose string changed, shaped through the same word cache. It submits
through the renderer's own direct path. Its frames are byte-identical to
zgx's at 300, 777 and 1,500 frames.

| heavy stream | instructions a frame |
|---|---|
| model (the app's own values) | 0.14 M |
| text (log 0.36 M, ~80 value texts at ~13 K) | 1.42 M |
| patching instances | 0.17 M |
| submit (encode and commit) | 0.99 M |
| **floor total** | **2.72 M** (zgx: 8.30 M) |

zgx's text costs about what the floor's does. The gap is the rest:
- update: ~2.0 M above the model;
- layout: ~1.5 M (the floor does none);
- damage: ~0.5 M above patching;
- per-item render work: ~1.6 M above submit.

**One write.**
- zgx's own work is about 24 K instructions: update 8.2 K, prepare 2.5 K,
  flush 2.9 K and its own render code about 10 K.
- The rest of the ~107 K render is Metal:
  - creating the render encoder (the driver zero-fills its state);
  - making and committing the command buffer;
  - presenting.
- GPUI pays that Metal cost too, and the present path belongs to the perf
  chat.
- The external target (0.189 µs to a "direct paint slot") is the write
  alone, about a thousand instructions: compare it with zgx's 8.2 K update,
  not with the frame.

## The changes

### A. Paint slots: a paint-only change patches the instance in place

Today a colour change goes through four steps:
1. **Update.** The style effect takes its memo fast path, resolves the
   paint over base and variants, then calls five scene setters (effects,
   mask, clip inset, transform, local transform), each a lookup and a
   compare.
2. **Flush.** The node is on the dirty list, and flush works out its world
   rect and damage.
3. **Render.** It walks `layer_items_within(damage)`, makes the items
   inside the damage again, and patches them into the kept frame (#441).
4. **Frame.** The kept frame is drawn.

The change: a node whose change is paint alone (colour, background,
border colour, opacity strictly inside 0 to 1, text colour of a text with
no inheriting children):
- is queued with its new paint on a paint list, not the dirty list;
- flush damages its last painted rect, which is unchanged, so there is no
  world walk;
- render rewrites the colour fields of that node's instance range in the
  kept buffer. A node-to-range map from the last whole frame is kept
  beside the kept frame.

Anything else falls back to the current path:
- a fill whose type changes (solid to gradient, or a shader);
- opacity reaching 0 or 1, which changes visibility or layering;
- a colour inherited by children;
- effects or layers;
- a node not in the kept frame.

Exactness test: for every paint field, the patched instances equal a whole
rebuild's, bit for bit, as the kept-frame tests do.

Owners:
- **Render:** the instance-range map, the in-place write and the upload.
- **Flush:** the paint list, damage from the last rect, no world walk.
- **Update (sol):** the effect hands a paint delta straight to the scene in
  one setter, not five, and resolves only the fields `produced` sets.

### B. Scrolling translates what is kept and builds only entering rows

Each scroll step today:
- **Update.** The list effect recomputes the range, allocates the key list
  and a set, and sets style and transform on every visible row. It sets
  each row's index signal, and reorders the children.
- **Prepare.** Layout runs again (162 K in heavy scroll), though the rows
  only moved.
- **Render.** Rendering is about 372 K. Content is copied by scroll copy
  only where that applies; otherwise the rows are made again.

The change:
- **Layout:** a scroll is a translation of the content. Rows that only
  moved keep their layout; only rows entering are measured and arranged.
  The content node's transform already carries the offset, so layout must
  not be dirtied by it.
- **Update (sol):**
  - the list effect diffs the old and new ranges, mounting and removing
    only the delta;
  - rows that stay touch nothing (no style, transform or index writes);
  - no allocation per step.
- **Flush:** one damage rect for the scrolled viewport, no subtree damage
  per moved row.
- **Render:**
  - offset the kept instances of the content range by the delta, and
    splice in the entering rows' instances;
  - this revives the parked "offset rows that only translated" step for
    the case where it pays: a list scrolling, where everything translates.
  - Exact only when the delta is whole device pixels (snapping commutes
    with it). Wheel scrolling already eases in whole pixels; other deltas
    take today's path.

### C. What a single write still pays (update, flush, frame loop)

- **Effect scheduling.** Each flush:
  - takes the queue;
  - orders it by `newest_first`, which sorts when ids are sparse: about 5%
    of heavy's update;
  - pops it against the pending heap.
- **Subscriptions.** Each effect run clears and re-records its
  subscriptions.
- **The effect body.** The style effect builds a 440-byte `Styles` and
  compares it with the last.
- **Planned cuts (update, sol):**
  - keep subscriptions when an effect reads the same signals as last time;
  - a queue that is already in order skips ordering;
  - paint deltas as in A;
  - no allocation on the write path.
  - Target: a colour write's update in the low thousands of instructions.
- **Flush (flush lane):**
  - the dirty-list walk and grid work for one small rect (2.9 K);
  - the frame loop's fixed work around update, flush, render and present.
- **Prepare (layout lane):** with nothing to lay out, prepare should cost
  next to nothing. It costs 2.5 K today in the observer loop and checks.

### D. Heavy's floor, by lane (in progress)

- **Update (sol):** heavy's ~800 bar and fill style effects. Each builds a
  `Styles`, compares it, then patches sizes and calls `set_box`.
- **Layout:** a fixed-size absolute leaf whose text changes neither
  measures again nor arranges its ancestors. A bar whose height changes
  re-places only itself.
- **Flush:** cost in proportion to dirty nodes, not repaints.
- **Render:** text shaping and layout per changed text (9.6 K each), and
  the TextCache trim and retire that walk every shaped text every frame.

## Order

1. Cheap, exact, general cuts in each lane (C, D), each its own PR with
   numbers.
2. Paint slots (A). It needs C's single-setter paint delta and the render
   instance map first.
3. Scroll translation (B), once the perf chat's 10k-row scroll
   microbenchmark gives the per-phase split. Today's split for heavy's
   table is above.

Each lane posts before it starts a structural piece: the scene.rs
functions it touches, and the numbers it expects.
