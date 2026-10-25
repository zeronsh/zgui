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
every frame (a dashboard's figures) shape new words every frame. Its
kerning pairs and mark anchors work their deltas out again the same way.

## What

1. `src/hb/face.rs`: a shaper whose instance has variation coordinates
   keeps each glyph's advance once worked out (`AdvanceCache`: a slot a
   glyph, made at the first run shaped, lock-free, empty again in a clone),
   and `glyph_h_advances` reads it before working one out. Shapers without
   coordinates, and builds without `std`, work them out as before.
2. `src/hb/glyph_metrics.rs`: `populated_advance_width`, one glyph's advance
   exactly as `populate_advance_widths` works it out, and `num_glyphs`.
3. `src/hb/ot/mod.rs`: the same shaper keeps each variation delta of its
   GPOS (kerning values and anchors, by delta-set index) once worked out
   (`DeltaCache`: a slot a delta set, each item variation data's slots made
   at its first delta asked, lock-free, empty again in a clone);
   `src/hb/ot/gpos/mod.rs` and `resolve_anchor` read it.
4. `zgx_keep_variations` (hidden) turns both off process-wide, for a test
   comparing shaping with and without (zgx-gpu's `variable_advances.rs`).

A shaper's instance is fixed for its life, so a glyph's advance and a delta
set's delta are too: neither cache changes a glyph, offset or advance.

Drop the fork when cosmic-text shapes with a harfrust that takes font
functions of its own (0.9+, `FontFuncs`), keeping the advances there, and
keeps GPOS deltas.
