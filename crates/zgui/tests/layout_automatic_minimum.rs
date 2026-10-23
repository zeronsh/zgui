//! A flex item shrinks no further than its content's min-content size
//! (CSS's `min-width: auto`): a word it cannot break keeps its width while
//! a sibling gives way. One that clips (a scroll container) may shrink to
//! nothing, and a minimum the item sets, `min_w(0.)` say, is its own.
use zgui::{compose::prelude::*, scene::Align, scene::NodeId, scene::Rect, widgets::Ui};

const WORD: &str = "Unbreakable";

/// Mount `view` in a 400 px window; the bounds of its children.
fn children(view: View) -> Vec<Rect> {
    let mut ui = Ui::new(400., 300.);
    let _view = ui.mount(view);
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let id: NodeId = scene.children(scene.root())[0];
    scene
        .children(id)
        .iter()
        .map(|c| scene.bounds(*c))
        .collect()
}
/// The word's own width.
fn word_width() -> f32 {
    children(row().child(text(WORD).text_wrap(false)))[0].width
}
/// A 200 px row: a 180 px box, then `item`, both free to shrink.
fn squeezed(item: View) -> Vec<Rect> {
    children(
        row()
            .w(200.)
            .child(div().w(180.).h(10.).flex_shrink(1.))
            .child(item.flex_shrink(1.)),
    )
}

#[test]
fn a_word_keeps_its_width_and_its_sibling_gives_way() {
    let word = word_width();
    assert!(word > 40. && word < 200., "{word}");
    let rects = squeezed(text(WORD).text_wrap(false));
    assert_eq!(rects[1].width, word, "{rects:?}");
    assert!((rects[0].width - (200. - word)).abs() < 0.01, "{rects:?}");
}

#[test]
fn wrapping_text_shrinks_to_its_longest_word() {
    let word = word_width();
    let rects = squeezed(text(format!("a {WORD} b")).text_wrap(true));
    assert_eq!(rects[1].width, word, "{rects:?}");
}

#[test]
fn a_box_needs_what_its_content_needs() {
    let word = word_width();
    let rects = squeezed(row().px(5.).child(text(WORD).text_wrap(false)));
    assert_eq!(rects[1].width, word + 10., "{rects:?}");
}

#[test]
fn a_clipping_box_and_an_explicit_minimum_shrink_past_the_content() {
    // Both shrink in proportion to their sizes: a 180 px box and the word.
    let word = word_width();
    let share = |rects: &[Rect]| rects[1].width;
    let proportional = word - (180. + word - 200.) * word / (180. + word);
    for rects in [
        squeezed(row().overflow_hidden().child(text(WORD).text_wrap(false))),
        // Clipping one axis makes it a scroll container on both.
        squeezed(row().overflow_y_hidden().child(text(WORD).text_wrap(false))),
        squeezed(text(WORD).text_wrap(false).min_w(0.)),
    ] {
        assert!((share(&rects) - proportional).abs() < 0.01, "{rects:?}");
    }
}

#[test]
fn a_set_size_is_a_ceiling_on_the_automatic_minimum() {
    // A 30 px item holding the wider word needs no more than its 30.
    let rects = squeezed(row().w(30.).child(text(WORD).text_wrap(false)));
    assert!(rects[1].width <= 30., "{rects:?}");
}

#[test]
fn taffy_laid_out_rows_keep_the_same_minimum() {
    // A child's own alignment lays the row out with taffy.
    let word = word_width();
    let rects = squeezed(text(WORD).text_wrap(false).align_self(Align::Center));
    assert!((rects[1].width - word).abs() < 0.01, "{rects:?}");
    let wrapping = text(format!("a {WORD} b")).text_wrap(true);
    let rects = squeezed(wrapping.align_self(Align::Center));
    assert!((rects[1].width - word).abs() < 0.01, "{rects:?}");
}
