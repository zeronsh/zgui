//! Balanced text (CSS's `text-wrap: balance`) wraps at the narrowest width
//! that keeps as many lines; its box, and text that doesn't ask, as before.
use zgx::{compose::prelude::*, scene::NodeId, widgets::Ui};

const HEADING: &str = "Balance this heading so that its words wrap more nicely";

/// A text `width` wide in a column, its node, after layout.
fn mounted(view: View) -> (Ui, NodeId) {
    let mut ui = Ui::new(600., 400.);
    let handle = ui.mount(column().w(600.).child(view.id("t")));
    ui.prepare_frame();
    let node = handle.find("t").unwrap();
    (ui, node)
}

#[test]
fn balanced_lines_wrap_narrower_and_keep_their_count() {
    let (ui, node) = mounted(text(HEADING).text_size(16.).w(300.).text_balance(true));
    let scene = ui.scene.borrow();
    let wrap = scene.text_wrap(node, 300.).0;
    let lines = |width: f32| scene.shape_text(HEADING, 16., Some(width)).size().1;
    assert!(wrap < 300., "{wrap}");
    assert_eq!(lines(wrap), lines(300.), "as many lines");
    assert!(lines(wrap - 2.) > lines(300.), "the narrowest: {wrap}");
    assert_eq!(scene.layout_bounds(node).width, 300., "its box as it was");
}

#[test]
fn unbalanced_one_line_and_long_text_wrap_at_their_box() {
    let (ui, node) = mounted(text(HEADING).text_size(16.).w(300.));
    assert_eq!(ui.scene.borrow().text_wrap(node, 300.).0, 300.);
    let (ui, node) = mounted(text("Short").text_size(16.).w(300.).text_balance(true));
    assert_eq!(ui.scene.borrow().text_wrap(node, 300.).0, 300., "one line");
    let long = HEADING.repeat(5);
    let (ui, node) = mounted(text(long).text_size(16.).w(300.).text_balance(true));
    assert_eq!(
        ui.scene.borrow().text_wrap(node, 300.).0,
        300.,
        "past six lines"
    );
}

#[test]
fn a_selection_follows_the_balanced_lines() {
    let (ui, node) = mounted(
        text(HEADING)
            .text_size(16.)
            .w(300.)
            .text_balance(true)
            .selectable(true),
    );
    let wrap = ui.scene.borrow().text_wrap(node, 300.).0;
    assert!(ui.select_all_text(node));
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let rects = &scene.text_highlight(node).unwrap().rects;
    let right = rects.iter().map(|r| r.x + r.width).fold(0., f32::max);
    assert!(right <= wrap + 0.5, "{right} past {wrap}");
}

/// Asked of a container, as CSS's `text-wrap` is inherited: the text in it
/// balances, unless it says otherwise.
#[test]
fn balance_is_inherited_and_a_text_can_stop_it() {
    let mut ui = Ui::new(600., 400.);
    let handle = ui.mount(
        column()
            .w(600.)
            .text_balance(true)
            .child(text(HEADING).text_size(16.).w(300.).id("in"))
            .child(
                text(HEADING)
                    .text_size(16.)
                    .w(300.)
                    .text_balance(false)
                    .id("out"),
            ),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let inner = handle.find("in").unwrap();
    let out = handle.find("out").unwrap();
    assert!(scene.text_wrap(inner, 300.).0 < 300.);
    assert_eq!(scene.text_wrap(out, 300.).0, 300.);
}

/// Centred, balanced lines are centred in the box, not in the narrower width
/// they wrap at: their selection too.
#[test]
fn centred_balanced_lines_are_centred_in_the_box() {
    use zgx::text_layout::TextAlign;
    let (ui, node) = mounted(
        text(HEADING)
            .text_size(16.)
            .w(300.)
            .text_align(TextAlign::Center)
            .text_balance(true)
            .selectable(true),
    );
    {
        let scene = ui.scene.borrow();
        let wrap = scene.text_wrap(node, 300.).0;
        assert_eq!(scene.text_wrap(node, 300.).1, (300. - wrap) / 2.);
    }
    assert!(ui.select_all_text(node));
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    for rect in &scene.text_highlight(node).unwrap().rects {
        let centre = rect.x + rect.width / 2.;
        assert!((centre - 150.).abs() < 6., "{rect:?}");
    }
}

/// A link in centred, balanced lines is pressed where it is drawn.
#[test]
fn a_link_in_balanced_lines_is_where_it_shows() {
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        input::{InputEvent, PointerButton},
        scene::NodeKind,
        text_layout::TextAlign,
    };
    let pressed = Rc::new(Cell::new(false));
    let seen = pressed.clone();
    let mut ui = Ui::new(600., 400.);
    let handle = ui.mount(
        column().w(600.).child(
            rich_text()
                .child(text_span(
                    "Balance this heading so that its words wrap more ",
                ))
                .child(text_span("nicely").on_click(move || seen.set(true)))
                .text_size(16.)
                .w(300.)
                .text_align(TextAlign::Center)
                .text_balance(true)
                .id("t"),
        ),
    );
    ui.prepare_frame();
    let (x, y) = {
        let scene = ui.scene.borrow();
        let mut node = handle.find("t").unwrap();
        while !matches!(scene.kind(node), NodeKind::RichText { .. }) {
            node = scene.children(node)[0];
        }
        let NodeKind::RichText { text } = scene.kind(node) else {
            unreachable!()
        };
        let bounds = scene.bounds(node);
        let (wrap, inset) = scene.text_wrap(node, bounds.width);
        assert!(inset > 0., "centred and balanced");
        let at = text.text().find("nicely").unwrap();
        let rect = scene
            .shape_rich_text(text, Some(wrap))
            .selection(at..at + 6)[0];
        (
            bounds.x + inset + rect.x + rect.width / 2.,
            bounds.y + rect.y + rect.height / 2.,
        )
    };
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    assert!(pressed.get(), "pressed at ({x}, {y})");
    // Its overlay (the pointing hand, what assistive technology is told)
    // over it too.
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let mut overlays = Vec::new();
    let mut pending = vec![handle.node()];
    while let Some(node) = pending.pop() {
        if scene.style(node).absolute {
            overlays.push(scene.bounds(node));
        }
        pending.extend(scene.children(node).iter().copied());
    }
    assert!(
        overlays
            .iter()
            .any(|r| (r.x + r.width / 2. - x).abs() < 2. && (r.y + r.height / 2. - y).abs() < 2.),
        "{overlays:?} at ({x}, {y})"
    );
}
