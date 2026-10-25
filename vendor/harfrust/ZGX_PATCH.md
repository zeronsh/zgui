# zgx patch to harfrust 0.5.2

Vendored from crates.io `harfrust` 0.5.2 (MIT, `LICENSE` from the
repository at the commit the crate was published from, efdae31, which the
published crate leaves out) and published as `zgx-harfrust`, its library
still `harfrust`. `zgx-cosmic-text` shapes with it, so zgx's crates build
from crates.io without a `[patch]` section. The change is marked `zgx:` in
the source.

## Why

A variable font's glyph advance is its default advance plus its instance's
delta (HVAR, or the outline's phantom points), and harfrust works the delta
out again for every glyph of every run it shapes. With the system font
(SF, variable, instanced per weight and optical size), a word shaped for
the first time spent about a fifth of its shaping there: values that change
every frame (a dashboard's figures) shape new words every frame.

## What

1. `src/hb/face.rs`: a shaper whose instance has variation coordinates
   keeps each glyph's advance once worked out (`AdvanceCache`: a slot a
   glyph, made at the first run shaped, lock-free, empty again in a clone),
   and `glyph_h_advances` reads it before working one out. Shapers without
   coordinates, and builds without `std`, work them out as before.
2. `src/hb/glyph_metrics.rs`: `populated_advance_width`, one glyph's advance
   exactly as `populate_advance_widths` works it out, and `num_glyphs`.
3. `zgx_keep_advances` (hidden) turns the cache off process-wide, for a test
   comparing shaping with and without (zgx-gpu's `variable_advances.rs`).

A shaper's instance is fixed for its life, so a glyph's advance is too: the
cache changes no glyph, offset or advance.

Drop the fork when cosmic-text shapes with a harfrust that takes font
functions of its own (0.9+, `FontFuncs`), keeping the advances there.
