//! Letter spacing in a scene whose text shaper takes no font
//! (`Scene::set_text_shaper`): the shaper's own advances spaced as the GPU
//! font shaper spaces glyphs, so both measure, wrap and place carets alike.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    scene::Scene,
    text_layout::{FontStyle, LetterSpacing, TextLayout},
};
use zgx_gpu::{TextServices, text::ShapedText};

fn scenes() -> (Scene, Scene) {
    let services = TextServices::new();
    let fonts = services.text_system();
    let mut gpu = Scene::new(400., 300.);
    services.install(&mut gpu);
    let mut legacy = Scene::new(400., 300.);
    let shared: Rc<RefCell<_>> = fonts;
    legacy.set_text_shaper(
        move |text: &str, size: f32, width: Option<f32>| -> Box<dyn TextLayout> {
            Box::new(ShapedText::new(&mut shared.borrow_mut(), text, size, width))
        },
    );
    (gpu, legacy)
}

#[test]
fn a_legacy_shaper_spaces_and_wraps_as_the_gpu_shaper() {
    let (gpu, legacy) = scenes();
    let mut differences = Vec::new();
    for text in [
        "Hello world",
        "Wavy tracking, kerned",
        "ab cd ef gh ij",
        "x\ny z",
        "two  spaces and a trailing one ",
        "Supercalifragilistic word",
        "Crème brûlée, s'il vous plaît.",
    ] {
        for spacing in [2., 5.5, -1.] {
            for width in [None, Some(40.), Some(70.), Some(150.)] {
                let font = FontStyle {
                    letter_spacing: LetterSpacing::px(spacing),
                    ..FontStyle::default()
                };
                let size = if spacing == 2. { 20. } else { 14. };
                let a = gpu.shape_text_with_font(text, size, width, &font);
                let b = legacy.shape_text_with_font(text, size, width, &font);
                let close = |p: f32, q: f32| (p - q).abs() < 0.01;
                let (sa, sb) = (a.size(), b.size());
                if !close(sa.0, sb.0) || !close(sa.1, sb.1) {
                    differences.push(format!("{text:?} {spacing} {width:?}: size {sa:?} {sb:?}"));
                }
                for (offset, _) in text.char_indices().chain([(text.len(), ' ')]) {
                    let (ca, cb) = (a.caret(offset), b.caret(offset));
                    // A space the GPU shaper drops at a wrap (one starting
                    // past the edge) is on no line of its: its caret reads
                    // as the text's start. Here it ends its line.
                    let dropped =
                        offset > 0 && (ca.x, ca.y) == (0., 0.) && text[offset..].starts_with(' ');
                    if !dropped && (!close(ca.x, cb.x) || !close(ca.y, cb.y)) {
                        differences.push(format!(
                            "{text:?} {spacing} {width:?}: caret {offset} at ({}, {}) and ({}, {})",
                            ca.x, ca.y, cb.x, cb.y
                        ));
                    }
                }
                assert_eq!(
                    gpu.measure_text_with_font(text, size, width, &font),
                    sa,
                    "measuring is shaping's size"
                );
                assert_eq!(legacy.measure_text_with_font(text, size, width, &font), sb);
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// Without spacing, the legacy shaper is used as it was.
#[test]
fn no_spacing_is_the_shaper_untouched() {
    let (_, legacy) = scenes();
    let plain = legacy.shape_text("Hello world", 14., None);
    let font = legacy.shape_text_with_font("Hello world", 14., None, &FontStyle::default());
    assert_eq!(plain.size(), font.size());
}
