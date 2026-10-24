//! The word cache (cosmic-text's shape run cache) keeps the words shown
//! and a few frames' worth of the rest: a value changing every frame does
//! not grow it.
use zgx::scene::*;
use zgx::text_layout::{FontFamily, FontStyle};
use zgx_gpu::GpuRenderer;
use zgx_gpu::text::FontData;

const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");

#[test]
fn a_changing_value_keeps_the_word_cache_the_size_of_what_is_shown() {
    let mut gpu = GpuRenderer::new(400, 60).unwrap();
    FontData::new(GEIST.to_vec())
        .unwrap()
        .install(&mut gpu.text_system().borrow_mut());
    let mut scene = Scene::new(400., 60.);
    gpu.install_text(&mut scene);
    let label = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "Requests 0 per second".into(),
            color: Color(0, 0, 0, 255),
            font_size: 16.,
        },
        Style::default(),
    );
    scene.set_font(
        label,
        FontStyle {
            family: FontFamily::Named("Geist".into()),
            ..Default::default()
        },
    );
    let mut most = 0;
    for frame in 1..400 {
        scene.set_text(label, format!("Requests {} per second", frame * 7919));
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        most = most.max(gpu.text_system().borrow().shape_run_cache.len());
    }
    // "Requests", "per", "second", spaces and the last few values.
    assert!(most < 40, "{most} words cached for one changing label");
    assert!(gpu.text_system().borrow().shape_run_cache.len() >= 4);
}

/// A word is shaped for its direction: one kept from a left-to-right line
/// is not served for the same text in a right-to-left run, where brackets
/// mirror.
#[test]
fn a_word_cached_left_to_right_is_shaped_again_right_to_left() {
    use cosmic_text::{Attrs, AttrsList, BufferLine, Family, FontSystem, LineEnding, Shaping};
    let gpu = GpuRenderer::new(16, 16).unwrap();
    let fonts = gpu.text_system();
    let mut fonts = fonts.borrow_mut();
    FontData::new(GEIST.to_vec()).unwrap().install(&mut fonts);
    let glyphs = |fonts: &mut FontSystem, text: &str| -> Vec<(usize, u16)> {
        let attrs = Attrs::new().family(Family::Name("Geist"));
        let mut line = BufferLine::new(
            text,
            LineEnding::None,
            AttrsList::new(&attrs),
            Shaping::Advanced,
        );
        let shape = line.shape(fonts, 8);
        shape
            .spans
            .iter()
            .flat_map(|span| &span.words)
            .flat_map(|word| &word.glyphs)
            .map(|glyph| (glyph.start, glyph.glyph_id))
            .collect()
    };
    let rtl = "\u{5e9}\u{5dc}\u{5d5}\u{5dd} (";
    fonts.shape_run_cache = Default::default();
    let fresh = glyphs(&mut fonts, rtl);
    fonts.shape_run_cache = Default::default();
    glyphs(&mut fonts, "a (");
    assert_eq!(glyphs(&mut fonts, rtl), fresh);
}
