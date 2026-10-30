# Detailed paint styles

Fluent styles support `bg_gradient(angle, stops)`, `bg_fill(brush)`,
`rounded_corners(Corners)`, `border_edges(Insets)`, `border_style(BorderStyle)`
and `shadows(...)`. Angles use clockwise radians. Gradient endpoints scale with
the allocated box; pattern brush spacing is expressed in logical pixels.

These compose with hover/focus/reactive styles and ordinary view children.
`bg(color)` clears a previously selected gradient/pattern; `rounded(radius)`
clears per-corner overrides; `border(width)` clears independent edge widths;
`shadow` and `shadow_none` clear the multiple-shadow override. Corners normalize
when their adjacent radii would exceed the box size. Borders paint inside their
allocation. Shadows contribute to damage and retained-layer extents.

Ordinary rounded rectangles keep the existing GPU shader path. Detailed
backgrounds/borders use the bounded device-scale canvas raster cache; unchanged
style and geometry reuse both paths and pixels. Shadows remain analytic GPU
quads, with per-corner radii and at most 32 shadows per panel (the rest are
left off, with an issue). The software
reference follows the same geometry. Gradients and dashed borders are validated
by the renderer; malformed values return a GPU rendering error.

The [paint styles example](../crates/zgx-desktop/examples/paint_styles.rs) shows
asymmetric corners and borders, two shadows, a resizable gradient, and a dashed
patterned card. GPU tests compare partial and fresh full frames through style
changes and shadow removal, including isolated layers and cache release.

`cursor(Cursor::Grab)` and the other platform-independent cursor variants work on
any view, including passive and disabled regions. Explicit cursor styles inherit
through visual ancestors and override the host's automatic control cursor.
Cursor changes update input geometry without layout or paint damage. The native
host caches the geometric query between pointer/geometry changes; ordinary scenes
without custom cursors avoid that query entirely.
