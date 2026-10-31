# zgx architecture

The implementation has a platform-independent Rust core, a retained wgpu renderer, a software reference renderer, and a native desktop host. Linux and macOS are the first target platforms; platform runtime support must be validated on each OS. Its optimization target is to avoid work before making necessary work faster. It does not establish that zgx uses less CPU or memory than mature frameworks; rendered comparisons and measurements are required for that claim.

## State and subscriptions

`Runtime` owns effects and their dynamically collected signal dependencies. A signal read inside an effect subscribes that effect. Equal writes do nothing. Before rerunning an effect, the runtime removes its old dependencies, so conditional reads do not leave stale subscriptions. Effect handles own their subscriptions; dropping a handle unsubscribes it. Explicit `batch` scopes coalesce notifications while making each write immediately visible. This is batching, not transactional rollback.

`List<T>` separates its length signal from each row's value signal. Appending three rows inside a batch causes a count observer to run once. Editing a row does not notify count observers, and writing the same row value twice produces one effective change. Rows have stable identities and can detect removal; the error API distinguishes capacity, bounds, removed rows, and exhausted identifiers. `ServiceScope` provides typed, hierarchically inherited services. The wake-driven local executor polls ready tasks and implements yielding without continuously polling sleeping tasks.

## Retained fibers and layout

`Scene` stores nodes in an arena. A `NodeId` pairs a slot index with a generation, preventing a removed node's handle from addressing a later node in the same slot. Each node retains its parent, ordered children, element data, style, cached bounds, transform, effects, and dirty flags. Removed slots are reused, and removing a subtree reclaims all its nodes. Arena capacity follows the peak retained count rather than shrinking after every removal.

This tree supplies the identity and retained lifetime portion of a fiber architecture. There is no React-compatible reconciler, priority scheduler, speculative render tree, or concurrent interruptible commit. Applications construct owned components and children through `compose`; mounting and reactive bindings perform scene mutations internally. `compose::keyed` preserves mounted owners across reorder, while the lower-level `KeyedChildren` API provides the same retained-scope mechanism for custom integrations. Effects can lower state changes directly to their affected node handles without rebuilding the entire view.

`ViewScope` connects this scene tree to reactive ownership. A mounted scope owns its root, child scopes, effects, and services; text bindings update their retained text nodes. Dropping the scope removes its subscriptions and subtree. Explicit child cleanup supports conditional slot lifetimes, and stale node checks keep bindings from addressing a reused slot after external removal.

Layout supports rows, columns, and overlays; fixed/intrinsic, percentage width/height and min/max dimensions; margin insets; padding; gap; flex grow/shrink; alignment; justification; and clipping. A measurement cache keys on available, forced and definite percentage-reference constraints and keeps a few recent results per node, because flex layout measures a child under its natural, flexed and stretched constraints within one pass; within a pass, a changed node also reuses its own results. Text leaves additionally cache their intrinsic size by wrap width. Clean measurement and arrangement subtrees are retained. A laid-out container with an explicit width and height is a layout boundary: a change inside it re-lays out only its subtree, because its parent never measures it from its content. Intrinsic changes propagate to ancestors and may reposition siblings; fixed-width **and** fixed-height text writes require no layout work. Flex distribution redistributes remaining space after children hit minimum or maximum limits. Shrink defaults to zero for compatibility with explicit overflow.

Text measurement is pluggable through `TextMeasurer`; desktop applications install the renderer's real font system, while a standalone core scene has a deterministic approximate fallback. `Style::text_wrap` requests measured wrapping at available width. `prepare_layout` updates geometry for input without consuming damage or queued frame statistics. Percentage width/height resolve against definite parent content axes, with intrinsic fallback for indefinite normal-flow axes. Full CSS percentage semantics and flex line wrapping are not implemented. Scale, rotation and skew (`Scene::set_local_transform`) apply after layout and never affect it.

## Damage and composition

Nodes enter a deduplicated dirty queue. Separate layout, paint, and composition flags distinguish necessary work. An idle `flush` returns immediately. A fixed-size text write queues one paint node. Translation, local transform (scale, rotation, skew), opacity, blur radius, and edge-fade changes queue composition work without regenerating layout or paint state. In this core, “paint” counts invalidated retained primitives, not GPU draw calls or rasterized pixels.

Damage includes the old and new visible geometry. Moving a parent invalidates its visible descendants at both locations. Removing a subtree damages its former pixels. Changing a clip or its size invalidates pixels that become visible or hidden. Damage rounds outward to pixel boundaries before merging, so fractional transforms cannot miss or double-blend a touched pixel. Overlapping damage rectangles merge; more than 64 separate regions collapse to a viewport update, bounding bookkeeping fragmentation. This is conservative rectangular damage rather than exact shape subtraction.

The GPU renderer draws damage in at most 16 regions, each redrawing every batch it touches, and damage all over a window (a dashboard's changing values) mostly ends up one region around it. Measured and shelved (branch `sol/region-encode`): drawing it as up to 16 least-area regions instead shades 24% fewer pixels in the heavy dashboard (3.59 → 2.73 Mpx, about −3.5% of the frame's GPU cycles) with identical pixels, but costs +13% of the frame's CPU as first written, and +4% at best (draws binned by region through a grid, one pass finding each region's covering quad, and a cost model keeping regions apart only where they save 50,000 device pixels). Draw calls were not the cost: testing each frame's thousand draws and three thousand items against the regions, and choosing them, was. Drawing through a stencil instead (frames drawn directly with Metal stamp the damage's own rectangles into a memoryless stencil, clearing them, then draw every batch once, tested against it) cuts 5.9% of the GPU cycles in heavy (both and stream) for 0.27% of the CPU. On the same branch, flat opaque quads in rounded clips draw their untouched interior unmasked and unblended (identical pixels), for about one more draw call each.

Backdrop blur has a dependency on underlying pixels. The scene keeps a separate registry of blur nodes, avoiding a full tree scan on ordinary text changes when no blur exists. Damage expands through overlapping filter regions until no additional filter output becomes dirty. The GPU renderer expands damage through connected clipped filter outputs and their physical sampling halos, then reconstructs those regions in paint order. Outputs are rounded to physical pixels before adding the integer kernel support, including backdrop outside the view clip. Backdrop copies and both filter passes cover only their required regions. Unrelated damage remains local; overlapping dependencies may cover the entire target. The scene remains conservative for large radii, while GPU sigma is capped. Redrawing only the changed translucent primitive would produce incorrect pixels. Two full-size scratch textures remain retained once blur is used.

The paint iterator walks retained nodes in insertion order, carrying inherited translation, clipping, and opacity. Each node caches the bounds of everything its subtree can paint; the GPU renderer visits only subtrees whose cached bounds meet the frame's damage (scenes with backdrop blur keep the full walk), so per-frame work follows what changed rather than the size of the scene. The renderer retains its output texture, clears damaged regions, and draws intersecting primitives in paint order, merging consecutive primitives that share a texture and clip into one instanced draw. A frame's render and presentation blit share one command buffer and one queue submission, and per-frame vertices go through reused upload memory. It shapes text with cosmic-text, caches glyphs in an atlas, supports rounded quads, borders, shadows, shared images, alpha, backdrop blur, and vertical edge fades. Presentation composites the retained texture into a native surface. A composition-only update avoids scene layout/paint rebuilding but still updates affected GPU pixels; it is not a promise of zero draw calls.

The explicit software reference backend retains a framebuffer and glyph masks. It uses softbuffer buffer age and four frames of damage history to repair recycled presentation buffers. When backdrop blur is present, this reference path conservatively redraws the whole surface to reconstruct filter inputs. Unknown buffer ages also require a complete copy. The native comparison demo defaults to GPU rendering; `ZGX_RENDERER=software` selects the reference, and `--headless` uses it for deterministic correctness runs.

A node with a local transform paints as a layer, as if isolated (CSS layer promotion). Its content stays in its own untransformed space, where layout put it; damage, culling ink, visible and accessibility bounds use the viewport-aligned box around the transformed corners, for both the old and the new matrix, and nested transforms compose. Hit testing maps the pointer through each transformed ancestor's inverse, including its clips, which therefore clip to the rotated box. Changing only the transform damages those two boxes and bumps no layer revision: the renderer redraws the cached texture under the new matrix. While no node has a local transform, every geometry query and damage walk takes the translation-only path.

Effects on ordinary nodes apply per primitive. `Scene::set_isolated(node, true)` instead retains the subtree in an offscreen texture and applies group opacity and edge fade once. Translation and group-opacity changes reuse cached pixels; content changes invalidate the layer and isolated ancestors. Nested groups preserve correct overlap. Layer storage is bounded at 64 MiB and 1,024 textures, with explicit capacity errors. Backdrop blur has conservative invalidation; see [renderer limits](../crates/zgx-gpu/README.md). Physical surface scaling is separate from logical scene coordinates and text metrics.

## Virtual lists and memory

`VirtualList` computes a visible `Range<usize>` using a fixed row height, viewport, offset, and overscan. The operation uses constant time and storage and does not allocate one object per logical row. For a 480-pixel viewport, 24-pixel rows, and three rows of overscan, at most 27 rows are selected, including a partially visible row. The public `compose::virtual_list` retains only the visible row components and derives their labels lazily; applications supply row views rather than scene nodes.

`VirtualListView` composes this range calculation with a clipped scroll viewport and `KeyedChildren`, retaining overlapping visible row keys and disposing offscreen scopes, subscriptions, and scoped resources. Bare `VirtualList` remains an allocation-free range helper. `variable_virtual_list` uses a reactive prefix index for application-supplied heights. `measured_virtual_list` populates this index from batched mounted-row layout; unseen rows keep estimates. Geometric discovery handles oversized estimates, while a one-pixel minimum bounds undersized and empty rows. Passing one million items to the helper does not itself allocate their data, but materializing a million application model rows naturally consumes memory.

## Cost model and limits

| Operation | Current work |
| --- | --- |
| Idle flush | Constant time, no tree traversal |
| Equal signal/node write | Equality comparison, no queued work |
| Fixed-size text mutation | Text equality/allocation plus one dirty node; ancestor clip lookup |
| Intrinsic size mutation | Ancestor invalidation and layout through affected parent child lists; clean subtrees skip measurement |
| Parent translation | Descendant damage traversal; no layout/paint rebuilding |
| Local transform (scale/rotation/skew) | As translation, plus one matrix per transformed ancestor; GPU redraws the cached layer, painting none of its content |
| Paint traversal | Linear in mounted scene nodes, explicit stack; cached isolated subtrees skip descendant traversal |
| Backdrop dependency update | Blur registry scan; overlapping filter chains may require multiple passes |
| Fixed-height virtual range | Constant time and storage |
| Variable-height range/offset/update | O(log rows), O(rows) height metadata |
| Variable-height visible geometry | O(log rows + mounted rows) |
| Variable-height growth | O(added rows + log rows), amortized allocation |

Targeted damage computes effective clips by looking up ancestors; its worst-case depth cost is quadratic because clipped ancestors also need their world translations. Deep trees are therefore less efficient than the ordinary shallow demo. Recursive layout, removal, and subtree damage are not designed for adversarially deep trees. Damage merging has bounded but nonlinear rectangle bookkeeping. The renderer culls whole subtrees against damage but does not keep a spatial index, so a single container with very many direct children is still scanned child by child. Nodes store absolute bounds, so moving a subtree (for example content below a growing fold) updates every descendant. Virtualization bounds that work for the list demo.

`cargo run -p zgx --release --example core_bench` measures invalidation and range costs with assertions about bounded work. It includes 1,002 retained nodes for idle and text workloads. These are CPU-only microbenchmarks, not a substitute for windowed end-to-end comparisons. See [benchmarking.md](benchmarking.md) for the shared workload and measurement protocol.

## Component composition and future template lowering

The public [component API](composition.md) models views as mounted components, declarative children, typed providers and lexical slots. [Fluent styles](styling.md) attach directly to those views. `Scene::append` is a backend primitive; a compiler should emit component/child builders instead of forcing application view bodies to manage scene parents. Component boundaries attach resources to the returned visual root without an extra layout box. Keyed/conditional regions retain child ownership and replace only structural changes. Property bindings update existing targets without rerunning component constructors.


The proposed template language can compile to the existing Rust primitives without becoming the Rust API:

| Template concept | Lowering target |
| --- | --- |
| `service Model` | A Rust model containing `List<T>` and `Signal<T>` |
| `provider Shared`, `provide` | `provide` / `provide_with`, typed `Context::service` lookup |
| `view`, stable identity | `component` with mount-time `Context`, local signals, children and owned resources |
| keyed loop | `compose::keyed`; `virtual_list` / `variable_virtual_list` / `measured_virtual_list` provide viewport virtualization |
| `text expression` | `text_signal`, tracking reads and updating the retained text property |
| `state done` | A view-owned signal |
| `slot body` | `Context::slot` captures caller providers while mounting under receiver ownership |
| `on click`, `task.yield()` | An event handler and local async task using `yield_now` |
| `list.len`, `list.at`, `row.write` | Separate length and row signal APIs |
| `catch` | Rust `Result` matching over `ListError` |

Generated views should use the component ownership APIs to retain and dispose nodes, effects and tasks together, preserving lexical ownership for slot captures. Event actions should explicitly batch the mutations after an async yield when they represent one update. No batch guard should span an arbitrary suspension. A compiler can assign stable keys to conditionals and list rows and use the existing keyed owner to preserve node IDs and local state across reorder.

There is no parser or stable binary ABI yet. The compatibility boundary is a small source-level lowering API: state reads/writes, batch scopes, providers, node creation/removal, property setters, task wakeups, and explicit error handling. This leaves syntax design independent of scene storage and rendering backends.

## Input, editing, and the desktop host

The input dispatcher performs transformed/clipped hit testing and capture-target-bubble routing, with keyboard focus, pointer capture, modal scopes, and RAII registrations. Widgets share activation behavior between pointer and keyboard input. Text editing stores Unicode grapheme-aware selections and bounded undo deltas; the native host forwards IME composition and clipboard operations. Accessibility semantics are retained separately from visual nodes and exported through AccessKit. Native application windows use winit and wgpu; asynchronous wakeups request event-loop work instead of continuous polling.

`Ui` owns widget subscriptions and handlers; `Ui::remove` unmounts their metadata together with scene nodes. `ViewScope` provides independent RAII lifetime for custom subtrees, including resource retention for virtual row input bindings. Component handles are references to Ui-owned widgets and do not implicitly unmount on drop. See [the application guide](application.md) for APIs, examples, resizing, and concrete limitations.

Native accessibility projection tracks per-node semantic revisions. Equal writes
keep a token stable; tokens also identify their semantic store, so a new store
cannot accidentally reuse a prior projection. Bounds, inherited disabled state
and child sequences are checked against the retained AccessKit node before
rebuilding its owned values. Hidden/removed nodes discard these tokens, and a
bridge reconnect requests a complete snapshot. Projection still walks visible
scene metadata; changed nodes still need owned AccessKit values.
