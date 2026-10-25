//! A text squeezed in a row shrinks to its min-content width, as CSS's
//! `min-width: auto` has it: its longest word, never a break inside one.
//! Rich text and plain text agree.
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};
use zgx_gpu::{GpuRenderer, text::FontData};

const GEIST: &[u8] = include_bytes!("fonts/GeistVariable-subset.ttf");
const WORDS: &str = "antidisestablishment of the words";

/// A row too narrow for `text` beside a fixed box: the text shrinks.
fn squeezed(text: View) -> View {
    row()
        .w(80.)
        .child(text)
        .child(div().w(70.).h(10.).shrink_0().bg(Color(0, 0, 0, 255)))
}

#[test]
fn a_squeezed_text_keeps_its_longest_word_rich_or_plain() {
    let gpu = GpuRenderer::new(400, 200).unwrap();
    FontData::new(GEIST.to_vec())
        .unwrap()
        .install(&mut gpu.text_system().borrow_mut());
    let mut ui = Ui::new(400., 200.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let mounted = ui.mount(
        column()
            .font_family("Geist")
            .text_size(16.)
            .child(squeezed(text(WORDS).text_wrap(true).id("plain")))
            .child(squeezed(
                rich_text()
                    .child(text_span(WORDS))
                    .text_wrap(true)
                    .id("rich")
                    .into(),
            ))
            .child(squeezed(text("antidisestablishment").id("word"))),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let width = |id: &str| scene.bounds(mounted.find(id).unwrap()).width;
    let (plain, rich, word) = (width("plain"), width("rich"), width("word"));
    // Wider than the 10 points the row leaves: the longest word holds.
    assert!(word > 100., "the longest word is {word} wide");
    assert_eq!(plain, word, "plain text shrinks to its longest word");
    assert_eq!(
        rich, word,
        "rich text shrinks to its longest word, as plain text"
    );
}
