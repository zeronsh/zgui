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
   `ShapeLine::build` builds that span directly. So does `build_mixed` for
   a line beyond ASCII that `classify` finds nothing right to left, no
   bidi control and no paragraph separator in, by one look at its bytes
   (`LineKind::LeftToRight`: accents, `—`, `…`, emoji); the rest takes the
   upstream path. `classify_finds_every_character_that_needs_the_analysis_or_hints`
   checks every code point the analysis treats otherwise than a
   left-to-right paragraph's is caught. ASCII's path is a copy of
   `build_spans` with no bidi in it at all, so a line beyond ASCII costs it
   nothing (the gate's lab-scroll, such text, runs 4.5% fewer
   instructions).

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
    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml -- bench mono
    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml -- kept

A layout is counted apart where upstream shaped a glyph of infinite or no
advance and this crate shaped none (`advances_finite!`): upstream divides
by a face's zero units per em, macOS's GB18030Bitmap, which this crate
passes by ("A font without units per em is no font", below). Upstream's
layout then shows `inf` or `NaN`, or loses the visual line such a glyph is
in. Rich text 120, a line of ASCII and controls with a tab, is that last
case: upstream shapes its `\x08` and `\x00` (in a monospace span) with
GB18030Bitmap, their advance infinite and the tab stop after them `NaN`,
and its word wrap drops the visual line holding bytes 0 to 17; this crate
shapes them with another face and draws the line, one more than upstream.
With GB18030Bitmap removed from upstream's fonts, every layout is identical
and none is counted apart. `kept` lays the texts out only with this crate,
monospaced and with an installed monospace face, with fonts kept against
nothing kept ("A run's first font is kept", below).

15,076 layouts are identical, and so are 8,451 of the same texts in up to
four spans of different weight, family, metadata and size, cut at random
places (`set_rich_text`); 3,582 differ only by GB18030Bitmap, as above
(12,033 identical without it); on 968 (lines mixing paragraph directions)
both versions panic, as upstream documents. 11,307 monospaced layouts are
identical with fonts kept and with none. Reshaping a
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

Monospace is kept too. With a face of its own, its search gives that face
first whatever the word, but as it does it weighs every fixed-width face
against the word's characters, for the faces it gives next. That cost was
paid for every new word: each line a terminal printed scanned every
installed face (a quarter of an engine-drawn terminal's CPU while output
streamed on macOS, more with Windows' fonts). A kept monospace face is now
used at once, and the search takes its first step only when a character is
missing from it (`shape_run`'s `searched`), leaving it where a search from
scratch would be. A face is kept only when it is the family's own
(`FontFallbackIter::is_default_face`): without one, the search gives the
fixed-width face with most of the word's characters, which depends on the
word. The `first_font` test adds monospaced runs whose characters only a
second fixed-width face or a proportional one has; `check/`'s `kept` mode
(below) lays its texts out monospaced, whole and in spans, with an installed
monospace face, kept against nothing kept: 11,307 layouts are identical on
macOS (Menlo). Shaping a terminal's tick of 24 new lines (`-- bench mono`)
takes 32 µs instead of 135 µs, and 1,100 µs upstream.

`FontSystem::query_face` finds a family's face at a weight and style once,
forgotten with the font matches (`db_mut`): zgx-gpu asks it for each
fallback family of a text whenever the text is laid out anew, and a
`fontdb` query reads every installed face's names.

`ShapeRunCache::clear` empties the word cache at once. `trim` sweeps only
every `TRIM_PERIOD` ages (above), so it cannot drop runs shaped with a font
that was just replaced; zgx-gpu's `FontData::replace_in` clears it, and its
`font_replace` test checks a replaced font measures and draws as a fresh one.

## A font without units per em is no font

macOS has a bitmap-only face, GB18030Bitmap, with no `head` table: its units
per em read as 0, so every advance it gave divided by zero. Monospaced text
fell back to it for CJK (it is marked monospaced), and the line took
infinite width: nothing after the first CJK character showed, and the
text's box broke its layout. `Font::new_at` now gives no font for a face
without units per em, so fallback passes it by (to Arial Unicode MS, or
PingFang). zgx-gpu's `cjk_in_monospaced_text_takes_finite_room` checks it.

## Words break with a hyphen

Text whose features carry `FontFeatures::HYPHENS` (`_hyp`, a tag outside
OpenType's two tag spaces, so no font has it) breaks words at the points
`FontSystem::set_hyphenator`'s function gives, a hyphen (U+2010, or `-`)
ending the line, and after a soft hyphen (U+00AD), as CSS's
`hyphens: auto`. Nothing is kept per line: `BufferLine::layout` works out
a line's points and shapes the hyphen for the layout that needs it,
into the font system's `ShapeBuffer`, which `layout_to_buffer` takes
(`ShapeLine::hyphenate`). Text without the tag, and every process that
never sets it (`note_hyphens`), pays one test.

## Emoji presentation takes a color font

A cluster asking for emoji presentation (U+FE0F after it, or a keycap's
U+20E3) is missing from a font without color glyphs (`Font::is_color`:
`COLR`, `CBDT` or `sbix`, read when asked), so fallback reaches the emoji
font, as browsers do: Windows' Segoe UI has `1` and the keycap mark, and
drew 1️⃣ in black. Found as a run's scripts are (`run_scripts`), in text
beyond ASCII; with no emoji font installed, the text glyphs stay.

## A line's kana or hangul say its Han characters' language

Han characters took the fallback font of the system's language, so on an
English system a Japanese line's kanji came from another font than its kana
(Arial Unicode MS beside Hiragino Sans on macOS). A line beyond ASCII with
kana (or hangul) is built again (`build_hinted`) with `FontFeatures::HAN`
(`_han`, 1 Japanese, 2 Korean, no font's) on its attributes: its Han
characters take Hiragana's (or Hangul's) fallback list, and the shape-run
cache, keyed by attributes, keeps its runs apart from a Chinese line's
without a change to its key. zgx-gpu's
`kanji_in_a_japanese_line_come_from_its_kana_font_whatever_the_system_language`
checks both, a Chinese line before and after.

An app's language comes first (`FontSystem::set_language`, from zgx's
`ui.i18n` locale): set to Chinese, Japanese or Korean, it decides every
line's Han and the kana hint stands down. Setting it rebuilds the fallback
lists and clears the shape-run cache, whose glyphs carry the old lists'
faces. The lists name Han's languages `ja`, `ko`, `zh-HK`, `zh-TW` and
`zh-CN` (`han_unification`), which a system locale such as `ja-JP` never
matched, so upstream gave a Japanese system Chinese Han; `han_locale`
maps any locale onto them.

A text's own language (zgx's `lang`, HTML's) comes before both: zgx sets
`_han` on its attributes (1 or 2, 4 simplified Chinese, 5 traditional, 6
Hong Kong's, 3 another language), and the hint and the app's language
fill only the spans that say nothing (`hinted`). Chinese has no script of
its own to look its fonts up by, so `Fallbacks` keeps the three Chinese
lists beside the locale's (`chinese`, built with the others), and
`run_scripts`, as it finds a run's Han, notes their language in the shape
buffer (`ShapeBuffer::han`), which the search reads for Han's fallbacks: a
Chinese quote in a Japanese app gets a Chinese face whatever the locale.
ASCII runs pass as before, and the search looks only when it reaches Han's
fallbacks.

## PingFang on macOS 26

macOS 26 keeps PingFang (its simplified, traditional and Hong Kong Chinese
faces) in `FontServices.framework`'s `Reserved` directory, where Core Text
finds it, and not in the font directories `load_system_fonts` reads. The
fallback lists named only PingFang, so Chinese Han drew in Arial Unicode MS
whatever the Chinese. `load_pingfang` loads that file after the system's
fonts if no PingFang face is loaded (another macOS has it in
`/System/Library/Fonts`), and the lists name Heiti SC or TC after PingFang
in case the file moves. zgx-gpu's
`chinese_text_on_macos_takes_the_systems_chinese_face_in_each_chinese`
checks both.
