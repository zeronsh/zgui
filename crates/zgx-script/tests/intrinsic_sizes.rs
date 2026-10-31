//! Widths from what a box holds, as scripts write them: `w = "max-content"`.
mod common;

use common::Harness;

fn width(h: &mut Harness, id: &str) -> f32 {
    h.frames(2);
    let node = h.find(id).unwrap();
    h.ui.scene.borrow().bounds(node).width
}

/// A text that wraps, in a wide column: as wide as its line, as narrow as
/// its longest word, and as wide as the room in between.
#[test]
fn text_takes_its_line_its_longest_word_or_the_room() {
    let mut h = Harness::inline(
        r##"local words = "several words that wrap"
        return ui.column { w = 1000, h = 400,
            ui.text { id = "stretched", words },
            ui.text { id = "line", w = "max-content", wrap = true, words },
            ui.text { id = "word", w = "min-content", wrap = true, words },
            ui.column { w = 60, ui.text { id = "fit", w = "fit-content", wrap = true, words } },
            ui.row { ui.text { id = "auto", words } },
        }"##,
    );
    let (line, word, auto) = (
        width(&mut h, "line"),
        width(&mut h, "word"),
        width(&mut h, "auto"),
    );
    assert_eq!(width(&mut h, "stretched"), 1000.);
    assert_eq!(line, auto, "its line, as a row lays it out");
    assert!(word < line && word > 0., "{word} {line}");
    let fit = width(&mut h, "fit");
    assert_eq!(fit, word.max(60.), "the room, or its longest word if wider");
    assert_eq!(h.error(), None);
}

#[test]
fn not_on_the_spacing() {
    let error = zgx_script::testing::TestApp::try_new(|runtime| {
        zgx_script::ScriptHost::inline(runtime, "app", r#"return ui.div { p = "min-content" }"#)
    })
    .err()
    .expect("an error");
    assert!(error.contains("size w, h"), "{error}");
}
