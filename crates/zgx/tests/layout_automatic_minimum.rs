//! A flex item shrinks no further than its content's min-content size
//! (CSS's `min-width: auto`): a word it cannot break keeps its width while
//! a sibling gives way. One that clips (a scroll container) may shrink to
//! nothing, and a minimum the item sets, `min_w(0.)` say, is its own. An
//! icon or an image that sets its size keeps it, as CSS's replaced elements
//! (`<img>`, `<canvas>`) do: its box holds no content to measure. An image
//! sized by its pixels keeps that natural size, as `<img>` does, unless its
//! minimum is set.
use zgx::{compose::prelude::*, scene::Align, scene::NodeId, scene::Rect, widgets::Ui};

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

#[test]
fn a_wrapping_row_needs_only_its_widest_child() {
    // Three 60 px chips that wrap: the row gives way down to one chip.
    let chip = || div().w(60.).h(10.).shrink_0();
    let chips = row().flex_wrap().child(chip()).child(chip()).child(chip());
    let rects = squeezed(chips);
    assert!(rects[1].width < 180., "{rects:?}");
    assert!(rects[1].width >= 60., "{rects:?}");
}

/// A 15 px icon.
fn icon() -> View {
    let source = std::sync::Arc::new(SvgData::new(&b"<svg viewBox='0 0 24 24'/>"[..]).unwrap());
    svg("Icon", source).size(15., 15.)
}

#[test]
fn an_icon_keeps_its_size_and_its_sibling_gives_way() {
    // A 200 px row: a 190 px box and the icon, 5 px too many.
    let rects = children(
        row()
            .w(200.)
            .child(div().w(190.).h(10.).flex_shrink(1.))
            .child(icon().flex_shrink(1.)),
    );
    assert_eq!((rects[1].width, rects[1].height), (15., 15.), "{rects:?}");
    assert!((rects[0].width - 185.).abs() < 0.01, "{rects:?}");
    // Across a column too.
    let rects = children(
        column()
            .h(100.)
            .child(div().w(10.).h(95.).flex_shrink(1.))
            .child(icon().flex_shrink(1.)),
    );
    assert_eq!(rects[1].height, 15., "{rects:?}");
}

#[test]
fn an_icon_in_a_padded_button_overflows_the_padding_centred_not_squashed() {
    // A 28 px square button whose 12 px padding leaves 4 px for content: the
    // icon stays 15 px, centred over the padding.
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        row()
            .size(28., 28.)
            .px(12.)
            .items_center()
            .justify_center()
            .child(icon().id("icon")),
    );
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(view.find("icon").unwrap());
    assert_eq!((bounds.width, bounds.height), (15., 15.), "{bounds:?}");
    assert!((bounds.x - 6.5).abs() < 0.01, "centred: {bounds:?}");
}

/// A 120 × 60 image sized by its pixels, 20 px of padding around them.
fn photo() -> View {
    let pixels =
        std::sync::Arc::new(zgx::image::ImageData::new(120, 60, vec![255; 120 * 60 * 4]).unwrap());
    image("Photo", pixels).p(20.).flex_shrink(1.)
}

#[test]
fn an_image_keeps_its_natural_size_unless_its_minimum_is_set() {
    // CSS's `<img>` with no width: its natural size is its automatic
    // minimum, so the 180 px box gives way to its 160...
    let rects = squeezed(photo());
    assert_eq!(rects[1].width, 160., "{rects:?}");
    assert!((rects[0].width - 40.).abs() < 0.01, "{rects:?}");
    // ...and `min_w(0.)` (`min-width: 0`) lets it shrink with the box, in
    // proportion to their sizes, as `object-fit` then fits its pixels in.
    let rects = squeezed(photo().min_w(0.));
    let proportional = 160. - 140. * 160. / 340.;
    assert!((rects[1].width - proportional).abs() < 0.01, "{rects:?}");
}

#[test]
fn taffy_laid_out_rows_keep_an_icons_size() {
    // A child's own alignment lays the row out with taffy.
    let rects = children(
        row()
            .w(200.)
            .child(div().w(190.).h(10.).flex_shrink(1.))
            .child(icon().flex_shrink(1.).align_self(Align::Center)),
    );
    assert_eq!(rects[1].width, 15., "{rects:?}");
}
