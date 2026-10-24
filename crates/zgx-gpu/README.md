# zgx-gpu

Native wgpu renderer for Linux (Vulkan) and macOS (Metal), with offscreen rendering for validation. Desktop windows use the same pipeline as the pixel tests.

```rust,no_run
use zgx::scene::Scene;
use zgx_gpu::GpuRenderer;
let mut renderer = GpuRenderer::new(800, 600)?;
let mut scene = Scene::new(800.0, 600.0);
let fonts = renderer.text_system();
scene.set_text_measurer(move |text: &str, size: f32, width: Option<f32>| {
    zgx_gpu::measure_text(&mut fonts.borrow_mut(), text, size, width)
});
let frame = scene.flush();
renderer.render(&scene, &frame.damage)?;
let rgba = renderer.readback()?;
# Ok::<(), zgx_gpu::GpuError>(())
```

The scene uses logical pixels; `resize` uses physical surface pixels. `set_scale_factor` invalidates raster/shaping caches and the retained target. Use `for_window(Arc<Window>)` for window presentation and call `present` after rendering, or on exposure when the retained target is already current.

Rectangles, shaped Unicode text, inherited opacity, clipping, edge fades, and separable Gaussian backdrop blur execute on the GPU. Shaping uses cosmic-text/font fallback, with one current shaped result per node, at most 1,024 cached nodes between frames. The RGBA glyph atlas starts as a one-pixel placeholder, allocates 512 × 512 (1 MiB) on the first visible glyph, and grows geometrically up to 2,048 × 2,048 (16 MiB) when a frame needs more capacity. It resets between frames at 75% shelf occupancy. Rasterizer results are not additionally cached on the CPU. An individual frame exceeding atlas capacity returns an explicit error. Readback yields tightly packed **premultiplied RGBA8**, with rows in top-to-bottom order; it deliberately blocks for completion and is for tests/screenshots, not normal presentation.

Ordinary frames replay only intersecting primitives into merged damage scissors on a persistent render texture. Unchanged text does not reshape; idle renders submit no GPU work. Backdrop blur reconstructs connected damage and filter dependency regions in paint order, including the sampling halo, to avoid feedback from previously composited foreground. Distant updates remain local. When the expanded damage touches no filter output, ordinary draws use one batched render pass despite clean filters elsewhere; those filter outputs remain in the retained target without filter passes or backdrop copies. Touching a filter dependency conservatively reconstructs its full clipped output and halo; overlapping dependencies can still require a full-target repaint. Blur shader/pipeline and two full-size scratch textures are allocated lazily and retained until resize. Each backdrop copy covers only the required source rectangle; horizontal and vertical passes scissor to their respective dependency regions. Sigma is capped at 64 physical pixels (192-pixel kernel support). Scene damage remains conservative and can exceed this capped GPU support for large logical radii. This optimization reduces submitted work and copy bandwidth, not the scratch texture allocation.

`GpuStats` reports submitted draw calls, quad instances, shaping misses, glyph uploads, and damaged pixel count. `render_passes` counts encoded rendering passes, including repainted isolated layers but excluding native presentation; `blur_passes` counts the two separable passes for each applied blur and is a subset of `render_passes`. Idle renders report zero. Repainting an isolated layer still replays its full target, so a distant change inside that layer can rerun its internal filters. These are work counters, not GPU timings. Pixel tests require an available Vulkan or Metal adapter and intentionally fail when none is available:

```sh
cargo test -p zgx-gpu
```

macOS surface validation must run on a macOS host; Linux software Vulkan validation does not prove Metal correctness or hardware performance.

## Quads, images, and window backgrounds

`NodeKind::Quad(QuadStyle)` supports antialiased rounded corners, inset borders, and an optional `BoxShadow` with offset/spread/blur. Shadows use a distance-field Gaussian falloff approximation rather than a sampled convolution. Shadow overflow participates in scene damage and ancestor clipping, including negative offsets. The software reference backend supports the same primitives, with one-pixel analytic antialiasing and nearest-neighbor image scaling.

`NodeKind::Image(Arc<ImageData>)` retains immutable straight RGBA data, derives intrinsic dimensions from the image, and stretches to explicit layout bounds. GPU sampling uses linear filtering with premultiplied uploads to avoid dark transparent-edge halos. GPU image textures are shared by image identity and reclaimed when absent from the scene. Their total allocation is bounded at 64 MiB; oversized images/live texture sets return errors. `assets::decode_image` accepts PNG/JPEG bytes with dimension/allocation limits. `assets::decode_svg` rasterizes SVG paths to a requested pixel size; external resource resolution is disabled and SVG text must be outlined. Perform decoding off the UI thread for large assets.

`set_background(Color)` changes the retained clear color and forces repaint even with empty scene damage. Transparent windows prefer premultiplied surface composition; postmultiplied surfaces receive appropriately converted colors. Surface loss recreates the surface and outdated surfaces reconfigure, with one bounded retry. Timeout/occlusion skips presentation; persistent failures return to the host for recovery.

## Retained memory and editor geometry

Text nodes retain local glyph quads as well as shaping results. A transform or opacity update reuses these quads without glyph cache lookup/rasterization. Atlas resets invalidate geometry by epoch. The shaped-node LRU is bounded by both 1,024 nodes and an 8 MiB budget covering retained text, glyph records and quad capacities. Glyph metadata (including empty glyphs) is capped at 8,192 entries. GPU vertex storage grows geometrically and is reused, with an explicit 32 MiB frame geometry ceiling. `debug_cache_stats()` exposes these allocations plus image textures and Swash image/outline entries; the latter remain zero because rasterization deliberately uses Swash's uncached API. Swash's internal scaler has an eight-font LRU. Cosmic-text's font-match cache is capped at 256 entries; loaded fonts/codepoint coverage follow the installed font database, and its optional shape-run cache is disabled.

`text::ShapedText` provides reusable hit testing, caret positions and disjoint selection spans for bidirectional text. It implements the core `TextLayout` trait and can be installed through `Scene::set_text_shaper`. Coordinates share the renderer's font choice and `ceil(size * 1.4)` line pitch. Ligature interiors use proportional grapheme advances (matching cosmic-text's editor convention), rather than font GDEF caret tables. The core's default text layout is an explicit approximate grapheme fallback, so desktop hosts should install the native factory.

The integration tests exercise 1,000 streaming/font-size/node-reuse frames and assert retained cache limits, no stale node entries, empty CPU bitmap caches, and reused vertex storage. Separate tests cover image cache reclamation and oversized-allocation errors.

## Explicit isolated subtrees

`Scene::set_isolated(node, true)` turns any subtree into a retained offscreen group. Its opacity and edge fade are applied once to the composited group, so overlapping opaque children do not leak through each other. Existing non-isolated nodes keep their original inherited, per-primitive opacity behavior.

GPU layer textures cover unclipped subtree content (including overflowing shadows) and are clipped when composited into their parent. A transparent texel border preserves fractional translation sampling. Group translation, opacity and edge-fade changes reuse cached pixels and avoid walking that subtree's paint items; content/layout changes invalidate that layer and isolated ancestors. Nested layer translation repaints its parent but reuses the child's texture. Content repaints reuse the texture allocation when its size is unchanged. Removing/disabling boundaries releases textures on the next render. The cache is bounded by 64 MiB and 1,024 textures, with explicit capacity errors.

Nodes with a local transform (`Scene::set_local_transform`, the `scale`/`rotate`/`skew` styles) are layers too. Their texture holds the subtree untransformed and is drawn as one quad whose corners the vertex shader moves by the node's matrix (as affine images are); the texel border antialiases its edges, and edge fades stay in the layer's own frame. A transform-only change is a cache hit: a frame of N changed matrices encodes N composite instances in one pass, no layer render pass, and on macOS goes through the direct Metal path, which samples layer textures as it does images. Layers render at the device scale times a raster step of √2^k (k in −2..=4), chosen from the accumulated scale of the node and its transformed ancestors (rotation and skew do not count); the step changes only once the scale is three quarters of a step away, so a zoom re-renders a handful of times. Steps above 1× shrink until the texture fits the device's texture size and 16 MiB. A transformed layer that returns to identity paints inline again but keeps its texture for 60 rendered frames (or until `trim` or budget pressure): the scene keeps counting its layer revision, so an animation passing through identity reuses its pixels. Backdrop filters on a transformed root use its untransformed bounds.

Backdrop filters inside a layer sample that isolated group's content, not pixels outside the group. They conservatively repaint the affected layer on content changes. A filter on the isolated root runs at group composition against its parent's backdrop; the parent's connected filter dependency reconstruction policy applies. `GpuStats` adds layer cache hits, repaints and texture allocation counts; `DebugCacheStats` reports layer bytes/textures. The RGB software reference uses black/white matte extraction for group alpha and does not retain layer textures. Its integer coverage and nearest-neighbor sampling can differ slightly from GPU output.

Pixel tests cover overlapping group opacity, nested invalidation, cached translations (including fractional offsets), overflow, blur dependencies, texture reuse, budget failure recovery and teardown.


## Shared native contexts

`GpuRenderer::context()` returns a cheap `GpuContext` clone. Use `for_window_with_context(window, &context)` for additional native windows, or `new_with_context(width, height, &context)` for offscreen targets. The context shares the wgpu instance, adapter, device, queue, quad pipelines, and cosmic-text font database/cache. Surfaces, viewport uniforms, retained render targets, layer/image textures, and glyph atlases belong to individual renderers. Closing one window does not invalidate another; the shared device/fonts live until the last context/renderer is dropped.

A surface unsupported by the shared adapter returns `GpuError::is_surface_incompatible() == true`; a host may create a separate context for that window. Other errors should not trigger unconditional device recreation loops. Existing `new` and `for_window` remain convenience constructors that create an independent context.

Windows without visible text allocate only a one-pixel glyph placeholder. Normal text starts with a 1 MiB atlas, and a capacity retry grows it as needed up to the existing 16 MiB ceiling. Atlas growth invalidates local glyph UV geometry but preserves existing composited layer textures. Tests verify context/font sharing, renderer-independent lifetimes, text-free atlas allocation, and growth for oversized glyphs.


## Backend initialization

The default instance enables wgpu's primary backends: Vulkan on Linux and Metal on macOS. It does not initialize an unused OpenGL/EGL stack just to enumerate adapters. Wgpu environment overrides, including `WGPU_BACKEND`, are applied; an unavailable explicitly requested backend returns a GPU initialization error. Non-primary native backends are not part of the validated platform matrix.

A same-binary Linux llvmpipe ablation measured approximately 22 MiB lower idle RSS and 49 fewer threads with Vulkan-only initialization than with Vulkan plus GL enabled. This is driver-specific idle evidence, not a hardware-GPU or active-workload guarantee; raw mapping snapshots and source hashes are in [the memory report](../../docs/results/component-memory/README.md).

## Extended painting and native media

The retained component layer exposes [rich text](../../docs/rich-text.md),
[canvas paths](../../docs/canvas.md), [detailed decorations](../../docs/paint-styles.md)
and [SVG images/transforms](../../docs/affine-images.md). Canvas and SVG caches
rasterize at device scale and reuse image identities for unchanged content;
affine image changes reuse uploaded pixels. Their separate 32 MiB raster budgets
are additional to the image-texture budget. Source data ownership and cache
entry bounds are documented in those guides. Default solid/rounded rectangles
continue using the direct quad shader.

On macOS, [CoreVideo surfaces](../../docs/native-surfaces.md) import BGRA or
full-range NV12 planes on the renderer's Metal device without CPU readback.
Source lifetimes extend through submitted GPU work. The software reference does
not render native video buffers. Target-platform runtime qualification remains
separate from cross-compilation.

Native Wayland hosts must honor compositor-authorized redraws. The desktop host
calls `present_with_notify` from that path, and the callback invokes winit's
`pre_present_notify` immediately before successful presentation. It does not
notify when acquisition is skipped. Covered-window model tasks remain runnable;
damage is retained until presentation can resume. See the
[real covered-window regression evidence](../../docs/platform-validation/wayland-frame-pacing).
