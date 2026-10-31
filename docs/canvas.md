# Retained canvas drawing

`canvas(|(width, height)| Canvas)` is a normal styled view. It composes with
`.child`, providers, event handlers, clipping and retained layers. Its callback
tracks reactive reads and runs when a dependency or allocated content size
changes. Empty allocations do not invoke the callback. Drawing is local to and
clipped by the allocated content box. Use ordinary child views for text and
controls that require their own interaction and accessibility nodes.

`Canvas::text` adds a line of text (`CanvasText`), shaped and drawn as any text
is, over the canvas's shapes whatever their order; no clip of the canvas applies
to it. `zgx::canvas::measure_text` gives its size while a canvas draws, for
placing what goes around it. Assistive technology doesn't read it: label the
canvas (`.label`, Luau `label`) with what it shows.

`Path::builder()` provides lines, quadratic and cubic curves, circular arcs,
closed contours and affine transforms. `Canvas::fill`, `fill_rule` and `stroke`
accept solid colors, linear gradients and slash patterns. Stroke options include
width, cap, join, miter limit, dash lengths and offset. Angles are radians, with
clockwise rotation in screen coordinates. `PathBuilder::build` rejects nonfinite
coordinates and paths exceeding 65,536 segments.

A canvas whose every command the shader can draw needs no raster at all, so a
chart redrawing each tick stays cheap: solid rectangles, circles, axis lines, strokes
of straight lines running left to right, and filled areas (a closed path of
straight lines whose edges run left to right in two chains that don't cross, as
a chart's area under its line or a band between two lines is, in a colour or a
linear gradient of up to 16 stops; on Windows these rasterize, since FXC
can't compile their shader). One command it can't draw rasterizes the whole
canvas. `GpuStats::canvas_quads` counts what it drew as shapes.

Otherwise the GPU backend rasterizes vectors with tiny-skia at the current
display scale and caches their straight-alpha pixels and uploaded texture. Equal
commands, size and scale reuse the cache; changed commands invalidate paint
without relayout. It keeps unchanged graphics cheap while complex continuously
changing paths still incur CPU rasterization. The software reference uses the
same rasterizer.

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
