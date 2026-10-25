//! A variable font's glyph advances and GPOS deltas, kept once worked out
//! for its instance (zgx-harfrust's `AdvanceCache` and `DeltaCache`), shape
//! every glyph where working each out again does: the same glyphs, advances
//! and offsets, to the bit, at every weight and size, the first time a text
//! is shaped and every time after.
use cosmic_text::{
    Attrs, AttrsList, BufferLine, Family, FontSystem, LineEnding, Shaping, Weight, fontdb,
};

const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
/// The system font (variable, instanced by weight and optical size), where
/// the machine has it.
const SYSTEM: &str = "/System/Library/Fonts/SFNS.ttf";
const TEXTS: [&str; 4] = [
    "Requests 48,213 per second, latency 12.7 ms",
    "AVAWAY To. Wa, fluffy office kerning: Tj Ty Yo",
    "The quick brown fox jumps over the lazy dog 0123456789",
    "émoji-free accents: déjà vu, naïve, Ångström, façade",
];

type Glyphs = Vec<(u16, usize, usize, u32, u32, u32, u32)>;

fn fonts(bytes: &[u8]) -> (FontSystem, String) {
    let mut db = fontdb::Database::new();
    db.load_font_data(bytes.to_vec());
    let family = db.faces().next().expect("a face").families[0].0.clone();
    (
        FontSystem::new_with_locale_and_db("en-US".into(), db),
        family,
    )
}

fn shape(fonts: &mut FontSystem, family: &str, weight: u16, size: f32, text: &str) -> Glyphs {
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(Weight(weight))
        .optical(size.round() as u16);
    let mut line = BufferLine::new(
        text,
        LineEnding::None,
        AttrsList::new(&attrs),
        Shaping::Advanced,
    );
    line.shape(fonts, 8)
        .spans
        .iter()
        .flat_map(|span| &span.words)
        .flat_map(|word| &word.glyphs)
        .map(|glyph| {
            (
                glyph.glyph_id,
                glyph.start,
                glyph.end,
                glyph.x_advance.to_bits(),
                glyph.y_advance.to_bits(),
                glyph.x_offset.to_bits(),
                glyph.y_offset.to_bits(),
            )
        })
        .collect()
}

/// Every text at every weight and size, shaped in a font system of its own.
fn all(bytes: &[u8], weights: &[u16], sizes: &[f32]) -> Vec<Glyphs> {
    let (mut fonts, family) = fonts(bytes);
    let mut out = Vec::new();
    for &weight in weights {
        for &size in sizes {
            for text in TEXTS {
                out.push(shape(&mut fonts, &family, weight, size, text));
            }
        }
    }
    out
}

fn compare(bytes: &[u8], weights: &[u16], sizes: &[f32]) {
    harfrust::zgx_keep_variations(false);
    let worked_out = all(bytes, weights, sizes);
    harfrust::zgx_keep_variations(true);
    // The first shaping fills what is kept; the second reads it.
    let (mut fonts, family) = fonts(bytes);
    for pass in 0..2 {
        let mut index = 0;
        for &weight in weights {
            for &size in sizes {
                for text in TEXTS {
                    let kept = shape(&mut fonts, &family, weight, size, text);
                    assert!(
                        kept == worked_out[index],
                        "{text:?} at weight {weight}, size {size} (pass {pass}) shaped otherwise"
                    );
                    index += 1;
                }
            }
        }
        // Shaped words are kept too: the second pass reshapes them.
        fonts.shape_run_cache = Default::default();
    }
}

#[test]
fn a_variable_fonts_kept_advances_shape_as_advances_worked_out() {
    compare(
        GEIST,
        &[100, 250, 400, 550, 700, 900],
        &[11., 13., 16., 24.],
    );
    if let Ok(bytes) = std::fs::read(SYSTEM) {
        compare(&bytes, &[300, 400, 600, 800], &[11., 13., 17., 28., 64.]);
    }
}
