# zgx patch to cosmic-text 0.19.0

Vendored from crates.io `cosmic-text` 0.19.0 (MIT OR Apache-2.0, license files
unchanged) and published as `zgx-cosmic-text`, its library still
`cosmic_text`, which zgx's crates depend on by path and version
(`cosmic-text = { package = "zgx-cosmic-text", … }`): they build from
crates.io with these changes and no `[patch]` section. Changes are marked "zgx patch" or "zgx:" in the source. The
crate's tests, benchmarks, fonts, samples and scripts are left out, and
`check/` is new.

## Why

Shaping is the largest share of zgx's CPU while text streams: about 40% of
the lab workload's streaming frames, a fifth of the heavy dashboard's. Each
tick reshapes the lines whose text changed. With the shape-run cache most
words are already shaped, so the cost is in what surrounds the lookups:
building a cache key per word, and running the Unicode bidi algorithm on every
line, which allocates its per-byte tables.

## What

1. **One reusable cache key.** `shape_run_cached` built a `ShapeRunKey` (a
   `String` of the word and copies of its attributes) for every word it
   looked up. The shape buffer now keeps the last key and refills it; its
   default attributes are rebuilt only when they differ.
2. **No bidi analysis for plain ASCII lines.** A line of ASCII without
   paragraph separators (`\n`, `\r`, U+001C–U+001E) holds no right-to-left
   or explicit embedding characters, so the bidi algorithm finds one
   left-to-right paragraph at level 0: one span over the whole line.
   `ShapeLine::build` builds that span directly; every other line takes the
   upstream path.

3. **Span attributes by id.** A rich text's line has attribute spans
   (zgx gives each run of a paragraph its own, with its metrics and
   metadata). For every word, the shape-run cache's key looked up the spans
   overlapping it, compared each with the defaults, and cloned it into the
   key, which the cache then hashed and compared whole. The spans that
   differ from a line's defaults now get ids from the cache's intern table
   once a line (`ShapeBuffer::line_spans`). A word's key holds the
   overlapping spans' ranges and ids, so it is equal to another exactly when
   the attributes are. The intern table only grows (`trim` never prunes it,
   as it never pruned the defaults): one entry for each distinct set of
   attributes. zgx's span metadata is the run's index in its paragraph, so
   that is about the longest paragraph's run count times the styles it uses.
4. **Ids from the cache they were given by.** A renderer at rest replaces
   its shape-run cache, and the new cache numbers attributes from 0 again.
   The key kept for reuse still held the old cache's id for its defaults,
   so a word shaped next under the same defaults went into the new cache
   under an id the new table could then give other attributes, and their
   words were served its glyphs (a bold label drawn regular). Each cache
   now has a generation of its own (a clone too: its table diverges), and
   a key from another's re-interns its defaults (zgx-gpu's
   `tests/word_cache.rs`).

## Checked

`check/` lays out 4,000 random lines (ASCII with every control and
separator, a quarter mixed with accents, CJK, Hebrew, Arabic, emoji and bidi
controls) at four widths and wrap modes with this crate and with upstream,
and requires identical glyphs, positions, levels and lines to upstream
shaping every run afresh (without its word cache, so the patched cache is
checked never to change what is drawn):

    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml
    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml -- bench

15,076 layouts are identical, and so are 12,033 of the same texts in up
to four spans of different weight, family, metadata and size, cut at
random places (`set_rich_text`); on the other 968 (lines mixing paragraph
directions) both versions panic, as upstream documents. Reshaping a
streaming transcript's last seven 105-byte lines takes 40.2 µs a tick instead
of 48.7 µs (M5 Max), and zgx's headless lab stream lays out in 0.047 ms a
tick instead of 0.061 ms.

## Axis ranges from the font file

A font file can give a variation axis a minimum above its maximum, and neither
skrifa nor swash checks: upstream's weight clamp in `swash.rs` called
`f32::clamp` on that range and panicked, so one malformed installed or
registered font ended the process the first time its glyphs were drawn. It
goes through `clamp_to_axis` (`src/lib.rs`), which never panics, as
`read-fonts` guards its own. `variation_axis_tests` in zgx-gpu's
`src/text.rs` shapes and draws a font with its `wght` or `opsz` axis
reversed.

## Font matching without reopening font files

Each new family, weight and style asked for (`get_font_matches`) scored every
installed face, and for every face of another weight mapped its file to see
whether a `wght` axis spans the weight asked for: about 800 faces and 4 ms
per new style on macOS, at startup and again the first time an app shows,
say, bold or monospace. The axis range is now read once per face and kept
(`src/font/system.rs`, "zgx patch"). `FontSystem::new_with_db`,
`weight_axis` and `set_weight_axis` let zgx-gpu load the system's faces and
their axes from a cache instead of opening their files (`font_cache.rs`).

## The word cache sweeps every eighth shaping frame

`ShapeRunCache::trim` visited every cached word on every call, and zgx
calls it on each frame that shapes text: a dashboard changing a few values
a frame swept a table of thousands each time, and zgx dropped the whole
table every 512 texts shaped to bound its size, after which every word was
shaped again. The sweep now runs every `TRIM_PERIOD` (8) ages, a word
lives `keep_ages` to `keep_ages + 8` ages unused, and a table left more
than twice as large as what it holds is shrunk, so zgx no longer
drops it (`len` and `is_empty` report its size). zgx-gpu's
`tests/word_cache.rs` checks it stays the size of what is shown.

A run's key now holds its direction (`ShapeRunKey::rtl`): upstream keyed a
word by its text and attributes alone, so a word shaped right to left (in
a bidi line) could be served for the same text left to right. Kept longer,
words made that reachable, and `check/` caught it on a mixed line; with the
direction in the key, how long a word is kept cannot change what is drawn. This is
an upstream bug (0.19.0's `ShapeRunKey` has no direction), worth fixing
there; the one-field change is all it needs.

## Tab stops only in lines with tabs

`ShapeLine::build` sliced every glyph's text to see whether it was a tab
before laying out tab stops. It now looks only in a line that has a tab
(one byte search); `check/` finds every layout identical.

## A cached word is looked up without comparing attributes

Every word shaped looked its run up in the shape run cache, and each
lookup compared the line's default attributes with the reused key's (a
whole `Attrs`, a family name among them), queried the attribute spans
even when there were none, and on a hit compared the key's attributes
again, whole; a miss cloned them, family string included. Now the cache
gives each set of default attributes an id (`ShapeRunCache::intern`) that
keys compare and hash, keeping the attributes behind an `Arc`; a line's
words check its defaults once (`ShapeBuffer::run_key_line`); and an empty
span map is not queried. `check/` finds every layout identical.

## Attributes are borrowed, not copied out

`AttrsList::get_span` built the default `Attrs` (a copy of every field, the
font-feature list cloned) for every lookup, even one a span answered, and
shaping called it for each glyph. It now builds them only when no span
holds the index. Without spans, `shape_fallback` takes a run's attributes
once rather than for each glyph, `ShapeWord::build` has no spans to compare,
and the word cache key and decoration check borrow the defaults
(`AttrsList::defaults_owned`). An ASCII run skips the per-character script
lookup: ASCII is all Common or Latin, which the lookup skipped anyway.
`check/` finds every layout identical.

## A visual line's display order goes into a kept list

`ShapeLine::reorder` returned a new list of ranges for every visual line
laid out, the usual all-left-to-right line included. It now fills a list
kept in the `ShapeBuffer`. `check/` finds every layout identical.

## No font at all shapes to nothing

With no font installed, or none fontconfig finds (a minimal system, a
container), `shape_run` and `shape_skip` panicked (`no default font found`)
for every run of text, every frame: zgx contained the panics, and its window
drew nothing at all. A run with no font now shapes to no glyphs, so
everything but the text draws, and the first such run says once on stderr
why the text does not show. `font_cache`'s
`text_shapes_to_nothing_with_no_font_installed` and the Linux `no-fonts`
probe check it.

## A run's first font is kept for its attributes

Every run shaped without the word cache (a new value: a dashboard's figure)
looked its attributes' font matches up by an owned key (`get_font_matches`)
and walked them to its family's face again (`FontFallbackIter::next`). For a
family other than monospace, which looks at each word's characters, that face
depends only on the attributes, so `FontSystem::first_font` keeps it with the
matches for the last eight attribute sets runs shaped with (a font without an
optical size axis matching at any size, as `get_font_at` instances it once).
`shape_run` starts its fallback search past the face it knows. Both are
forgotten when the font matches are (`db_mut`, or the cache's limit).
`shape_run` is out of line, so the word cache's path through `Shaping::run`
keeps its code. zgx-gpu's `first_font` test shapes texts in turn under
changing families, weights, sizes and scripts, with characters only another
font has, to the same glyphs from the same fonts as each alone.

`ShapeRunCache::clear` empties the word cache at once. `trim` sweeps only
every `TRIM_PERIOD` ages (above), so it cannot drop runs shaped with a font
that was just replaced; zgx-gpu's `FontData::replace_in` clears it, and its
`font_replace` test checks a replaced font measures and draws as a fresh one.
