//! A run's first font, kept for the attributes it was found for
//! (zgx-cosmic-text's `FontSystem::first_font`), is the font a fallback
//! search finds: texts shaped in turn under changing families, weights,
//! sizes and scripts, and with characters only another font has, shape to
//! the same glyphs, from the same fonts, as each shaped alone in a font
//! system of its own.
use cosmic_text::{
    Attrs, AttrsList, BufferLine, Family, FontSystem, LineEnding, Shaping, Weight, fontdb,
};

const FONTS: [&[u8]; 4] = [
    include_bytes!("fonts/GeistVariable-subset.ttf"),
    include_bytes!("fonts/Geist-500-subset.ttf"),
    include_bytes!("fonts/Geist-600-subset.ttf"),
    include_bytes!("../../../comparisons/fonts/DejaVuSans.ttf"),
];
/// Family, weight, size and text, in the order shaped.
const RUNS: [(&str, u16, f32, &str); 12] = [
    ("Geist", 400, 13., "Requests 48,213"),
    ("DejaVu Sans", 400, 13., "Requests 48,213"),
    ("Geist", 400, 24., "Requests 48,213"),
    ("Geist", 600, 13., "latency 12.7 ms"),
    ("Geist", 500, 11., "latency 12.7 ms"),
    ("DejaVu Sans", 700, 16., "Ωmega ≠ Ω"),
    ("Geist", 400, 13., "Ωmega ≠ Ω"),
    ("monospace", 400, 13., "fn main() {}"),
    ("Geist", 400, 13., "déjà vu, naïve"),
    ("sans-serif", 400, 12., "déjà vu, naïve"),
    ("Geist", 400, 13., "Requests 48,213"),
    ("DejaVu Sans", 400, 40., "Requests 48,213"),
];

type Glyphs = Vec<(u16, usize, usize, u32, u32, u32, String)>;

fn fonts() -> FontSystem {
    let mut db = fontdb::Database::new();
    for bytes in FONTS {
        db.load_font_data(bytes.to_vec());
    }
    FontSystem::new_with_locale_and_db("en-US".into(), db)
}

fn shape(fonts: &mut FontSystem, (family, weight, size, text): (&str, u16, f32, &str)) -> Glyphs {
    let family = match family {
        "monospace" => Family::Monospace,
        "sans-serif" => Family::SansSerif,
        name => Family::Name(name),
    };
    let attrs = Attrs::new()
        .family(family)
        .weight(Weight(weight))
        .optical(size.round() as u16);
    let mut line = BufferLine::new(
        text,
        LineEnding::None,
        AttrsList::new(&attrs),
        Shaping::Advanced,
    );
    let shape = line.shape(fonts, 8).clone();
    shape
        .spans
        .iter()
        .flat_map(|span| &span.words)
        .flat_map(|word| &word.glyphs)
        .map(|glyph| {
            let face = fonts.db().face(glyph.font_id).expect("a face");
            (
                glyph.glyph_id,
                glyph.start,
                glyph.end,
                glyph.x_advance.to_bits(),
                glyph.x_offset.to_bits(),
                glyph.font_weight.0.into(),
                format!("{} {:?}", face.post_script_name, glyph.font_weight),
            )
        })
        .collect()
}

#[test]
fn a_kept_first_font_is_the_font_a_fallback_search_finds() {
    // Each alone, in a font system of its own: nothing kept.
    let alone: Vec<Glyphs> = RUNS.iter().map(|run| shape(&mut fonts(), *run)).collect();
    let mut shared = fonts();
    for pass in 0..2 {
        for (run, expected) in RUNS.iter().zip(&alone) {
            assert!(
                shape(&mut shared, *run) == *expected,
                "{run:?} (pass {pass}) shaped otherwise"
            );
        }
        // Words are kept too: shape every run again, not served from them.
        shared.shape_run_cache = Default::default();
    }
}
