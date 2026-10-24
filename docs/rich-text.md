# Retained inline display text

A rich paragraph shapes all inline spans together. Font changes and Unicode bidi
runs share one wrapping layout rather than separate row/column text boxes:

```rust
use zgx::compose::prelude::*;
use zgx::scene::Color;
let paragraph = rich_text().w(320.).text_wrap(true).text_size(18.)
    .child(text_span("Hello "))
    .child(text_span("world").font_bold().text_color(Color(80, 190, 255, 255)))
    .child(text_span(" — العربية").italic(true));
let view = column().child(paragraph);
```

`rich_text_signal(move || Vec<TextSpan>)` updates inline content while retaining
the paragraph node. Parent typography supplies unspecified span properties.
Span properties currently include family, weight, italic slant, size, color,
line height and letter spacing, plus background color and solid underline/strikethrough decorations. `Decoration::new(thickness).color(color)` configures line thickness and optional explicit color; otherwise the foreground color is used. Decoration fragments follow shaped bidi/wrapped runs and clip to the paragraph. Root views use the usual fluent layout and
styling system. One retained rich paragraph shapes the inline content; interactive spans add owned hit fragments beneath its styled overlay owner.
Empty spans are ignored. Equal resolved content suppresses damage; color-only
changes preserve layout. The native host measures with the same mixed-run shaper
used for GPU rendering. The headless core uses approximate grapheme advances.

Low-level `rich_text::RichText` validates contiguous UTF-8 byte ranges covering
its immutable text. A `TextRun` holds a resolved font, size and color. Ranges
must be nonempty and end at character boundaries; sizes must be finite and
positive. An empty string has no runs. Mixed runs shape continuously through
cosmic-text, including wrapping and bidirectional layout. Glyph ink clips to
the paragraph's allocated bounds, matching ordinary text.

GPU and software raster paths render the run colors. GPU shaping uses the
existing bounded retained node cache. The software rich-glyph cache holds at
most 128 entries within an 8 MiB accounted budget; an oversized entry is used
for the current call and then discarded. Driver and font database allocations
are outside these cache budgets.

Run the native example with `cargo run -p zgx-desktop --example rich_text`.
This API is display text, not a rich-text editor. `text_span("Action").on_click(callback)` creates an accessible Link with one Tab stop, Enter/Space activation, pointer hit fragments derived from shaped selection geometry, and hover/focus outlines. Wrapped fragments do not make intervening gaps clickable. Disabled ancestors suppress activation. The accessible location is the first visible fragment; the name covers the whole span. Native shaping changes refresh hit geometry even when the allocated bounds stay equal. Wavy underlines are not yet supported.

A link shows the pointing hand. `text_span("Docs").href("https://…")` is a link to a URL: a click (after `on_click`'s, if it has one) opens it with the system's handler through `Platform::open_url` (the desktop host checks it has a scheme and no control characters, and launches it detached), if it is a web or mail link (`http:`, `https:`, `mailto:`): text often isn't the app's own, and a `file:` link or another scheme's handler can run what it names, so those don't open (an app's own schemes it handles in `on_click`); and a secondary click (Control-click on macOS) opens a context menu with Open Link and Copy Link. A link without an `href` has no menu of its own, so the press goes on to the views it is in. `on_click_with(|keys| …)` is `on_click` told the keys held, so a ⌘-click (Control elsewhere) can open a link aside; a middle click passes as one, as a browser opens a background tab.

Typography also supports inherited `.font_features(FontFeatures::new([(*b"liga", 0)]))`, `.font_fallbacks(vec![FontFamily::from("Your Font")])`, and `.text_align(TextAlign::Center)` (also Start, Left, Right). Feature settings sort by tag and retain the last value, keeping equivalent cache keys equal. Native shaping tries explicit fallback families in order for a whole grapheme before using platform fallback. The headless fallback remains an approximate grapheme-cell layout, without OpenType substitutions.

Register bundled TTF/OTF font data or collections before opening windows:

```rust,no_run
# use zgx_desktop::{Application, FontData};
# fn example(bytes: std::sync::Arc<[u8]>) -> Result<(), Box<dyn std::error::Error>> {
let font = FontData::new(bytes)?;
let app = Application::new().font(font);
# Ok(())
# }
```

`FontData` validates a font and exposes its family names. Each blob is limited to 64 MiB and shares its bytes across window databases. Application registration occurs once per GPU context before shaping; it is not a live font replacement API. Software `Raster::register_font` clears its glyph caches.

Display paragraphs support `.line_clamp(2)` and `.text_overflow(TextOverflow::Ellipsis)`; `.truncate()` combines an ellipsis with a one-line clamp. `.line_clamp(0)` removes an inherited clamp, and `TextOverflow::Clip` restores ordinary clipping. Ellipsis without an explicit clamp uses one line. Native truncation shapes a grapheme-safe prefix and the ellipsis in the preceding span's font. It retains the full semantic string and excludes synthetic ellipsis glyphs and hidden text from link hit regions. These options never truncate editor documents. A width narrower than the ellipsis clips its ink at the normal text bounds.

For low-level scene users, `Style::text_options` controls `NodeKind::Text`; `NodeKind::RichText` takes its authoritative options from `RichText::with_options`. The component API lowers its fluent settings into that immutable rich content, allowing measurement, rendering, and link geometry to share one value without rebuilding content during each paint.
