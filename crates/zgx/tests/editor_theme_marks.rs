//! What an editor draws in theme colours only some editors have (a default
//! placeholder, line numbers, a composition's underline) follows the theme,
//! and its caret listener hears where its own callback left the caret.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgx::{
    compose::prelude::*,
    scene::{Color, NodeId, NodeKind},
    theme::{Theme, ThemeKind, ThemeOverride, tok},
    widgets::Ui,
};

const RED: Color = Color(255, 0, 0, 255);
const GREEN: Color = Color(0, 255, 0, 255);

/// The colours of the text nodes under `root` showing `label`.
fn colors(ui: &Ui, root: NodeId, label: &str) -> Vec<Color> {
    let scene = ui.scene.borrow();
    let mut todo = vec![root];
    let mut out = vec![];
    while let Some(n) = todo.pop() {
        if let NodeKind::Text { text, color, .. } = scene.kind(n)
            && &**text == label
        {
            out.push(*color);
        }
        todo.extend_from_slice(scene.children(n));
    }
    out
}

/// The fills of the underlines (quads a point or two high) under `root`.
fn underlines(ui: &Ui, root: NodeId) -> Vec<Color> {
    let scene = ui.scene.borrow();
    let mut todo = vec![root];
    let mut out = vec![];
    while let Some(id) = todo.pop() {
        let b = scene.bounds(id);
        if b.width > 1.
            && (b.height == 1. || b.height == 2.)
            && let NodeKind::Quad(q) = scene.kind(id)
        {
            out.push(q.fill);
        }
        todo.extend_from_slice(scene.children(id));
    }
    out
}

/// Themes "a" and "b", `token` red in one and green in the other, "a" set.
fn red_then_green(ui: &Ui, tokens: &[zgx::theme::ColorToken]) {
    let (mut a, mut b) = (
        Theme::new("a", ThemeKind::Dark),
        Theme::new("b", ThemeKind::Dark),
    );
    for token in tokens {
        a = a.with(*token, RED);
        b = b.with(*token, GREEN);
    }
    ui.theme().register(a);
    ui.theme().register(b);
    ui.theme().set("a").unwrap();
}

#[test]
fn caret_listener_observes_selection_changed_from_its_callback() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("abcd".to_owned());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let armed = Rc::new(Cell::new(false));
    let act = armed.clone();
    let inside = editor.clone();
    editor.on_caret(move |c| {
        sink.borrow_mut().push((c.anchor, c.focus));
        if act.replace(false) {
            inside.set_selection(2, 2);
        }
    });
    armed.set(true);
    editor.set_selection(1, 1);
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(editor.selection(), (2, 2));
    assert_eq!(seen.borrow().last().copied(), Some((2, 2)));
}

#[test]
fn a_caret_listener_moving_the_caret_every_time_is_told_a_few_times_not_forever() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("abcdefghijklmnop".to_owned());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    let told = Rc::new(Cell::new(0));
    let count = told.clone();
    let inside = editor.clone();
    editor.on_caret(move |c| {
        count.set(count.get() + 1);
        inside.set_selection(c.focus + 1, c.focus + 1);
    });
    told.set(0);
    editor.set_selection(1, 1);
    ui.prepare_frame();
    assert!((1..=4).contains(&told.get()), "{}", told.get());
}

#[test]
fn placeholder_follows_tertiary_only_theme_change() {
    let mut ui = Ui::new(400., 200.);
    red_then_green(&ui, &[tok::TEXT_TERTIARY]);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.set_placeholder(Some("placeholder"), None);
    ui.prepare_frame();
    assert_eq!(colors(&ui, view.node(), "placeholder"), [RED]);
    ui.theme().set("b").unwrap();
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(colors(&ui, view.node(), "placeholder"), [GREEN]);
}

#[test]
fn placeholder_follows_scoped_tertiary_change() {
    let mut ui = Ui::new(400., 200.);
    let color = ui.signal(RED);
    let read = color.clone();
    let value = ui.signal(String::new());
    let view = ui.mount(theme_scope_with(
        move || ThemeOverride::new().with(tok::TEXT_TERTIARY, read.get()),
        text_input("Text", value).id("input").w(300.),
    ));
    ui.prepare_frame();
    let node = view.find("input").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.set_placeholder(Some("placeholder"), None);
    ui.prepare_frame();
    assert_eq!(colors(&ui, node, "placeholder"), [RED]);
    color.set(GREEN);
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(colors(&ui, node, "placeholder"), [GREEN]);
}

#[test]
fn a_placeholder_in_its_own_colour_keeps_it() {
    let mut ui = Ui::new(400., 200.);
    red_then_green(&ui, &[tok::TEXT_TERTIARY]);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    let editor = ui.editor(view.node()).unwrap();
    let blue = Color(0, 0, 255, 255);
    editor.set_placeholder(Some("placeholder"), Some(blue));
    ui.prepare_frame();
    ui.theme().set("b").unwrap();
    ui.prepare_frame();
    assert_eq!(colors(&ui, view.node(), "placeholder"), [blue]);
}

#[test]
fn gutter_follows_secondary_and_tertiary_theme_change() {
    let mut ui = Ui::new(400., 200.);
    red_then_green(&ui, &[tok::TEXT_TERTIARY, tok::TEXT_SECONDARY]);
    let value = ui.signal("first\nsecond".to_owned());
    let view = ui.mount(text_area("Text", value).size(300., 150.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.set_line_numbers(true);
    ui.prepare_frame();
    assert_eq!(colors(&ui, view.node(), "1"), [RED]);
    ui.theme().set("b").unwrap();
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(colors(&ui, view.node(), "1"), [GREEN]);
    assert_eq!(colors(&ui, view.node(), "2"), [GREEN]);
}

#[test]
fn existing_composition_underlines_follow_accent_even_after_refresh() {
    let mut ui = Ui::new(400., 200.);
    red_then_green(&ui, &[tok::ACCENT]);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.set_composition("日本語", Some((0, 6)), &[]);
    ui.prepare_frame();
    assert_eq!(underlines(&ui, view.node()), [RED, RED]);
    ui.theme().set("b").unwrap();
    editor.refresh();
    ui.prepare_frame();
    assert_eq!(underlines(&ui, view.node()), [GREEN, GREEN]);
}

#[test]
fn composition_underlines_follow_accent_on_their_own() {
    let mut ui = Ui::new(400., 200.);
    red_then_green(&ui, &[tok::ACCENT]);
    let value = ui.signal(String::new());
    let view = ui.mount(text_input("Text", value).w(300.));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    let editor = ui.focused_editor().unwrap();
    editor.set_composition("日本語", Some((0, 6)), &[]);
    ui.prepare_frame();
    ui.theme().set("b").unwrap();
    ui.prepare_frame();
    assert_eq!(underlines(&ui, view.node()), [GREEN, GREEN]);
}
