# Retained canvas drawing

`canvas(|(width, height)| Canvas)` is a normal styled view. It composes with
`.child`, providers, event handlers, clipping and retained layers. Its callback
tracks reactive reads and runs when a dependency or allocated content size
changes. Empty allocations do not invoke the callback. Drawing is local to and
clipped by the allocated content box. Use ordinary child views for text and
controls that require their own interaction and accessibility nodes.

`Path::builder()` provides lines, quadratic and cubic curves, circular arcs,
closed contours and affine transforms. `Canvas::fill`, `fill_rule` and `stroke`
accept solid colors, linear gradients and slash patterns. Stroke options include
width, cap, join, miter limit, dash lengths and offset. Angles are radians, with
clockwise rotation in screen coordinates. `PathBuilder::build` rejects nonfinite
coordinates and paths exceeding 65,536 segments.

The GPU backend rasterizes vectors with tiny-skia at the current display scale
and caches their straight-alpha pixels and uploaded texture. Equal commands,
size and scale reuse the cache; changed commands invalidate paint without
relayout. This is cached vector rasterization, not a GPU tessellation pipeline.
It keeps unchanged graphics cheap while complex continuously changing paths
still incur CPU rasterization. The software reference uses the same rasterizer.

The raster cache and a single canvas image are each limited to 32 MiB. Commands,
gradient stops and dash arrays have bounded renderer acceptance limits. Invalid
geometry returns a GPU render error; the software reference omits invalid
canvas content. `GpuStats::canvas_rasterizations` and
`DebugCacheStats::canvas_raster_bytes` expose the cost. Entries are released when
their scene nodes are removed; least-recently-used pixel entries are evicted
under pressure. Existing renderer texture limits apply separately.

Run `cargo run -p zgx-desktop --example canvas` for a resizable gradient,
quadratic/cubic curve and patterned rectangle. Regression tests compare changed
content against a fresh frame, exercise isolated layers, verify cache reuse and
check reactive callback disposal.
