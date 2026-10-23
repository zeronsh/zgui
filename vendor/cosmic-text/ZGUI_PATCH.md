# zgui patch to cosmic-text 0.19.0

Vendored from crates.io `cosmic-text` 0.19.0 (MIT OR Apache-2.0, license files
unchanged) and applied through `[patch.crates-io]` in the workspace
`Cargo.toml`. Changes are marked "zgui patch" or "zgui:" in the source. The
crate's tests, benchmarks, fonts, samples and scripts are left out, and
`check/` is new.

## Why

Shaping is the largest share of zgui's CPU while text streams: about 40% of
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

## Checked

`check/` lays out 4,000 random lines (ASCII with every control and
separator, a quarter mixed with accents, CJK, Hebrew, Arabic, emoji and bidi
controls) at four widths and wrap modes with this crate and with upstream,
and requires identical glyphs, positions, levels and lines:

    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml
    cargo run --release --manifest-path vendor/cosmic-text/check/Cargo.toml -- bench

15,076 layouts are identical; on the other 968 (lines mixing paragraph
directions) both versions panic, as upstream documents. Reshaping a
streaming transcript's last seven 105-byte lines takes 40.2 µs a tick instead
of 48.7 µs (M5 Max), and zgui's headless lab stream lays out in 0.047 ms a
tick instead of 0.061 ms.

## Axis ranges from the font file

A font file can give a variation axis a minimum above its maximum, and neither
skrifa nor swash checks: upstream's weight clamp in `swash.rs` called
`f32::clamp` on that range and panicked, so one malformed installed or
registered font ended the process the first time its glyphs were drawn. It
goes through `clamp_to_axis` (`src/lib.rs`), which never panics, as
`read-fonts` guards its own. `variation_axis_tests` in zgui-gpu's
`src/text.rs` shapes and draws a font with its `wght` or `opsz` axis
reversed.

## Font matching without reopening font files

Each new family, weight and style asked for (`get_font_matches`) scored every
installed face, and for every face of another weight mapped its file to see
whether a `wght` axis spans the weight asked for: about 800 faces and 4 ms
per new style on macOS, at startup and again the first time an app shows,
say, bold or monospace. The axis range is now read once per face and kept
(`src/font/system.rs`, "zgui patch"). `FontSystem::new_with_db`,
`weight_axis` and `set_weight_axis` let zgui-gpu load the system's faces and
their axes from a cache instead of opening their files (`font_cache.rs`).
