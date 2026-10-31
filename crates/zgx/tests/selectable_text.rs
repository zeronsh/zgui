//! Display text selection (`View::selectable`): the headless core's
//! approximate layout puts each grapheme in a cell 0.6 of the text size
//! wide, so at size 10 a character is 6 wide and a line 20 tall here.
use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    cursor::Cursor,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeKind,
    widgets::Ui,
};

fn document(ui: &mut Ui, children: Vec<View>) {
    ui.mount(
        column()
            .w(300.)
            .gap(4.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .children(children),
    );
    ui.prepare_frame();
}
fn press(ui: &mut Ui, x: f32, y: f32, shift: bool) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        },
        Modifiers {
            shift,
            ..Modifiers::default()
        },
    );
}
fn release(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        },
        Modifiers::default(),
    );
}
fn move_to(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch_with_modifiers(InputEvent::PointerMove { x, y }, Modifiers::default());
}
fn drag(ui: &mut Ui, from: (f32, f32), to: (f32, f32)) {
    press(ui, from.0, from.1, false);
    move_to(ui, to.0, to.1);
    release(ui, to.0, to.1);
}
fn shortcut(key: &str) -> InputEvent {
    InputEvent::KeyDown {
        key: Key::Character(key.into()),
        modifiers: Modifiers {
            meta: cfg!(target_os = "macos"),
            control: !cfg!(target_os = "macos"),
            ..Modifiers::default()
        },
        repeat: false,
    }
}
/// The text nodes in the scene, in document order.
fn texts(ui: &Ui) -> Vec<zgx::scene::NodeId> {
    let scene = ui.scene.borrow();
    let mut found = Vec::new();
    let mut stack = vec![scene.root()];
    while let Some(id) = stack.pop() {
        if matches!(
            scene.kind(id),
            NodeKind::Text { .. } | NodeKind::RichText { .. }
        ) {
            found.push(id);
        }
        stack.extend(scene.children(id).iter().rev());
    }
    found
}
/// The text nodes that show (in view, inside every clip).
fn visible(ui: &Ui) -> Vec<zgx::scene::NodeId> {
    let scene = ui.scene.borrow();
    texts(ui)
        .into_iter()
        .filter(|id| {
            scene
                .visible_bounds(*id)
                .is_some_and(|r| r.width > 0. && r.height > 0.)
        })
        .collect()
}
fn highlighted(ui: &Ui) -> Vec<usize> {
    let scene = ui.scene.borrow();
    texts(ui)
        .iter()
        .enumerate()
        .filter(|(_, id)| scene.text_highlight(**id).is_some())
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn a_drag_selects_across_text_nodes_in_document_order() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![text("Hello world"), text("Second paragraph"), text("Third")],
    );
    drag(&mut ui, (12., 10.), (30., 34.));
    assert_eq!(ui.selected_text(), "llo world\nSecon");
    assert_eq!(highlighted(&ui), [0, 1]);
    // Backwards, from the same anchor, past the end of the document.
    press(&mut ui, 30., 34., false);
    move_to(&mut ui, 12., 10.);
    assert_eq!(ui.selected_text(), "llo world\nSecon");
    move_to(&mut ui, 100., 290.);
    release(&mut ui, 100., 290.);
    assert_eq!(ui.selected_text(), "d paragraph\nThird");
    assert_eq!(highlighted(&ui), [1, 2]);
    // A press that does not move collapses it.
    press(&mut ui, 5., 10., false);
    release(&mut ui, 5., 10.);
    assert!(!ui.has_text_selection());
    assert!(highlighted(&ui).is_empty());
}

#[test]
fn double_and_triple_presses_select_words_and_paragraphs_and_drag_by_them() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![text("alpha beta gamma"), text("one\ntwo three")],
    );
    for _ in 0..2 {
        press(&mut ui, 40., 10., false);
        release(&mut ui, 40., 10.);
    }
    assert_eq!(ui.selected_text(), "beta");
    // Dragging after a double press extends in whole words. (Moving away
    // ends the chain of presses.)
    move_to(&mut ui, 200., 200.);
    press(&mut ui, 40., 10., false);
    release(&mut ui, 40., 10.);
    press(&mut ui, 40., 10., false);
    move_to(&mut ui, 80., 10.);
    assert_eq!(ui.selected_text(), "beta gamma");
    move_to(&mut ui, 4., 10.);
    assert_eq!(ui.selected_text(), "alpha beta");
    release(&mut ui, 4., 10.);
    // A triple press selects the line between hard breaks.
    move_to(&mut ui, 200., 200.);
    for _ in 0..3 {
        press(&mut ui, 10., 54., false);
        release(&mut ui, 10., 54.);
    }
    assert_eq!(ui.selected_text(), "two three");
    // Presses past the third keep it, as Chrome's do: no caret again.
    for _ in 0..2 {
        press(&mut ui, 10., 54., false);
        release(&mut ui, 10., 54.);
        assert_eq!(ui.selected_text(), "two three");
    }
}

/// A double press selects a word of Chinese, Japanese or Thai by
/// dictionary (`cjk-thai-words`, on by default), as Chrome does; without it,
/// by Unicode's word boundaries: a character at a time.
#[test]
fn a_double_press_selects_a_chinese_japanese_or_thai_word() {
    let on = cfg!(feature = "cjk-thai-words");
    for (line, at, word, fallback) in [
        ("我们在北京大学学习。", 4, "北京", "京"),
        ("東京に行きます", 1, "東京", "京"),
        ("สวัสดีครับ", 3, "สวัสดี", "ดี"),
    ] {
        let mut ui = Ui::new(400., 300.);
        document(&mut ui, vec![text(line)]);
        let x = 3. + 6. * at as f32;
        for _ in 0..2 {
            press(&mut ui, x, 10., false);
            release(&mut ui, x, 10.);
        }
        assert_eq!(
            ui.selected_text(),
            if on { word } else { fallback },
            "{line}"
        );
    }
}

#[test]
fn a_word_at_a_point_selects_as_a_double_press_does() {
    // A touch screen's long press.
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("editable".to_owned());
    document(
        &mut ui,
        vec![text("alpha beta gamma"), text_input("Field", value).w(200.)],
    );
    assert!(ui.select_word_at(40., 10.));
    assert_eq!(ui.selected_text(), "beta");
    assert_eq!(highlighted(&ui), [0]);
    // Select-all then takes the region it is in.
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    assert_eq!(ui.selected_text(), "alpha beta gamma");
    // An editor selects its own, and nothing is there below the text.
    let field = ui.scene.borrow().bounds(texts(&ui)[1]);
    assert!(!ui.select_word_at(field.x + 2., field.y + 5.));
    assert!(!ui.select_word_at(380., 290.));
    assert_eq!(ui.selected_text(), "alpha beta gamma");
}

#[test]
fn a_word_selected_during_a_press_extends_by_words_as_the_press_moves() {
    // A finger lands, rests (the long press selects the word), then moves.
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("alpha beta gamma")]);
    press(&mut ui, 40., 10., false);
    assert!(ui.select_word_at(40., 10.));
    assert_eq!(ui.selected_text(), "beta");
    move_to(&mut ui, 80., 10.);
    assert_eq!(ui.selected_text(), "beta gamma");
    move_to(&mut ui, 4., 10.);
    assert_eq!(ui.selected_text(), "alpha beta");
    // Lifting keeps it.
    release(&mut ui, 4., 10.);
    assert_eq!(ui.selected_text(), "alpha beta");
    // Without a press the pointer moving extends nothing.
    assert!(ui.select_word_at(40., 10.));
    move_to(&mut ui, 80., 10.);
    assert_eq!(ui.selected_text(), "beta");
}

#[test]
fn a_long_press_on_a_button_in_text_selects_its_word_and_never_clicks_it() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    document(
        &mut ui,
        vec![
            text("Before"),
            button()
                .child(text("Press"))
                .on_click(move || counted.set(counted.get() + 1)),
            text("After"),
        ],
    );
    let label = ui.scene.borrow().bounds(texts(&ui)[1]);
    let (x, y) = (label.x + 1., label.y + label.height / 2.);
    // Held, then lifted where it was (the host moves the pointer in place
    // as the hold selects): the word, and no click.
    press(&mut ui, x, y, false);
    assert!(ui.select_word_at(x, y));
    move_to(&mut ui, x, y);
    assert_eq!(ui.selected_text(), "Press");
    release(&mut ui, x, y);
    assert_eq!(clicks.get(), 0);
    assert_eq!(ui.selected_text(), "Press");
    // Held, then dragged on: the word stays the start, extended by words.
    move_to(&mut ui, 200., 200.);
    press(&mut ui, x, y, false);
    assert!(ui.select_word_at(x, y));
    move_to(&mut ui, x, y);
    move_to(&mut ui, 200., 290.);
    release(&mut ui, 200., 290.);
    assert_eq!(clicks.get(), 0);
    assert_eq!(
        ui.selected_text(),
        "Press
After"
    );
}

#[test]
fn shift_press_extends_from_the_anchor() {
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("abcdefghij"), text("klmnopqrst")]);
    drag(&mut ui, (12., 10.), (24., 10.));
    assert_eq!(ui.selected_text(), "cd");
    press(&mut ui, 30., 34., true);
    release(&mut ui, 30., 34.);
    assert_eq!(ui.selected_text(), "cdefghij\nklmno");
}

/// After a double or triple press, a shift-press extends by words or
/// paragraphs, as Safari and AppKit do.
#[test]
fn a_shift_press_after_a_word_selection_extends_by_words() {
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("one two three"), text("four five six")]);
    // Characters 6 points wide: "two" spans 24 to 42.
    for _ in 0..2 {
        press(&mut ui, 30., 10., false);
        release(&mut ui, 30., 10.);
    }
    assert_eq!(ui.selected_text(), "two");
    press(&mut ui, 40., 34., true);
    release(&mut ui, 40., 34.);
    assert_eq!(
        ui.selected_text(),
        "two three\nfour five",
        "to the whole word pressed"
    );
    press(&mut ui, 4., 10., true);
    release(&mut ui, 4., 10.);
    assert_eq!(
        ui.selected_text(),
        "one two",
        "back past the first, from its far edge"
    );
}

#[test]
fn select_all_and_presses_elsewhere() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    ui.mount(
        column().children([
            column()
                .w(300.)
                .text_size(10.)
                .line_height(20.)
                .selectable(true)
                .children([
                    text("First"),
                    text("Kept out").selectable(false),
                    text("Last"),
                ]),
            button()
                .child(text("Act"))
                .on_click(move || counted.set(counted.get() + 1)),
            text("Outside"),
        ]),
    );
    ui.prepare_frame();
    // Nothing selected or pressed yet: select-all has no region.
    assert!(!ui.dispatch(shortcut("a")).default_prevented);
    press(&mut ui, 4., 10., false);
    release(&mut ui, 4., 10.);
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    // What is kept out leaves a gap: a blank line.
    assert_eq!(ui.selected_text(), "First\n\nLast");
    // A button elsewhere acts and keeps the selection; plain text ends it.
    let act = ui.scene.borrow().bounds(texts(&ui)[3]);
    press(&mut ui, act.x + 2., act.y + 2., false);
    release(&mut ui, act.x + 2., act.y + 2.);
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.selected_text(), "First\n\nLast");
    let outside = ui.scene.borrow().bounds(texts(&ui)[4]);
    press(&mut ui, outside.x + 2., outside.y + 2., false);
    assert!(!ui.has_text_selection());
    assert!(!ui.dispatch(shortcut("a")).default_prevented);
}

#[test]
fn links_and_buttons_inside_click_and_a_drag_from_one_selects() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    document(
        &mut ui,
        vec![
            text("Before"),
            button()
                .child(text("Press"))
                .on_click(move || counted.set(counted.get() + 1)),
            text("After"),
        ],
    );
    let label = ui.scene.borrow().bounds(texts(&ui)[1]);
    let (x, y) = (label.x + 1., label.y + label.height / 2.);
    press(&mut ui, x, y, false);
    release(&mut ui, x, y);
    assert_eq!(clicks.get(), 1);
    assert!(!ui.has_text_selection());
    // A drag from it selects, and it does not activate.
    press(&mut ui, x, y, false);
    move_to(&mut ui, x + 2., y);
    assert!(!ui.has_text_selection(), "within the press's slop");
    move_to(&mut ui, 200., 290.);
    release(&mut ui, 200., 290.);
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.selected_text(), "Press\nAfter");
}

/// A button that is itself the selectable root clicks, and a drag from it
/// selects, as one inside a selectable box does.
#[test]
fn a_button_that_is_the_selectable_root_clicks_and_a_drag_selects() {
    let mut ui = Ui::new(400., 300.);
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    ui.mount(
        button()
            .child(text("Press"))
            .on_click(move || counted.set(counted.get() + 1))
            .selectable(true),
    );
    ui.prepare_frame();
    let label = ui.scene.borrow().bounds(texts(&ui)[0]);
    let (x, y) = (label.x + 1., label.y + label.height / 2.);
    press(&mut ui, x, y, false);
    release(&mut ui, x, y);
    assert_eq!(clicks.get(), 1);
    assert!(!ui.has_text_selection());
    press(&mut ui, x, y, false);
    move_to(&mut ui, 200., 290.);
    release(&mut ui, 200., 290.);
    assert_eq!(clicks.get(), 1, "a drag does not activate it");
    assert_eq!(ui.selected_text(), "Press");
}

/// A heading that is itself the selectable root copies as a heading, as
/// one inside a selectable box does.
#[test]
fn selected_heading_root_stays_a_heading() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(text("Notes").heading(2).selectable(true));
    ui.prepare_frame();
    assert!(ui.select_all_text(view.node()));
    assert_eq!(ui.selected_html(), "<meta charset=\"utf-8\"><h2>Notes</h2>");
    let mut ui = Ui::new(400., 300.);
    ui.mount(column().selectable(true).child(text("Notes").heading(2)));
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(ui.selected_html(), "<meta charset=\"utf-8\"><h2>Notes</h2>");
}

#[test]
fn editors_inside_a_selectable_region_select_their_own_text() {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("editable".to_owned());
    document(
        &mut ui,
        vec![text("Label"), text_input("Field", value).w(200.)],
    );
    let field = ui.scene.borrow().bounds(texts(&ui)[1]);
    drag(
        &mut ui,
        (field.x + 2., field.y + 5.),
        (field.x + 30., field.y + 5.),
    );
    assert!(!ui.has_text_selection());
    let editor = ui.focused_editor().expect("the press focused the editor");
    let (anchor, focus) = editor.selection();
    assert_ne!(anchor, focus);
    // Select-all goes to the focused editor.
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    assert!(!ui.has_text_selection());
}

#[test]
fn text_is_not_selectable_unless_asked_and_costs_nothing() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .children([text("Plain"), text("Text")]),
    );
    ui.prepare_frame();
    let listeners = texts(&ui)
        .iter()
        .map(|id| ui.input.has_listeners(*id))
        .collect::<Vec<_>>();
    drag(&mut ui, (2., 5.), (20., 30.));
    assert!(!ui.has_text_selection());
    assert!(!ui.scene.borrow().has_text_highlights());
    assert_eq!(ui.scene.borrow().cursor_at(2., 5.), None);
    assert!(listeners.iter().all(|listens| !listens));
}

#[test]
fn the_cursor_is_an_i_beam_over_selectable_text_only() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            text("Text"),
            row().child(text("Pointer")).cursor(Cursor::Pointer),
        ],
    );
    let [first, second] = texts(&ui)[..] else {
        panic!()
    };
    let scene = ui.scene.borrow();
    let (a, b) = (scene.bounds(first), scene.bounds(second));
    assert_eq!(scene.cursor_at(a.x + 2., a.y + 2.), Some(Cursor::Text));
    assert_eq!(scene.cursor_at(b.x + 2., b.y + 2.), Some(Cursor::Pointer));
    // Blank room in the region is not text.
    assert_eq!(scene.cursor_at(290., 290.), None);
}

#[test]
fn rich_text_spans_select_as_one_paragraph() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            rich_text()
                .child(text_span("Bold ").font_bold())
                .child(text_span("and plain"))
                .into(),
            text("Next"),
        ],
    );
    drag(&mut ui, (32., 10.), (12., 34.));
    assert_eq!(ui.selected_text(), "and plain\nNe");
}

#[test]
fn a_selection_change_repaints_only_the_text_it_changes() {
    let mut ui = Ui::new(400., 300.);
    document(&mut ui, vec![text("abcdefghij"), text("klmnopqrst")]);
    press(&mut ui, 0., 10., false);
    move_to(&mut ui, 30., 34.);
    ui.prepare_frame();
    let _ = ui.scene.borrow_mut().flush();
    // Extending within the second line leaves the first alone, but for
    // where its corners meet the second's.
    move_to(&mut ui, 42., 34.);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert!(!report.damage.is_empty());
    assert!(
        report.damage.iter().all(|rect| rect.y >= 20.),
        "{:?}",
        report.damage
    );
    // An unchanged selection repaints nothing.
    move_to(&mut ui, 42., 34.);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().damage.is_empty());
}

#[test]
fn removing_selected_text_drops_its_part_and_unmounting_ends_it() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let visible = shown.clone();
    let handle = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Kept"))
            .child(switch(
                move || visible.get(),
                |shown, _| if shown { text("Gone") } else { column() },
            ))
            .child(text("Last")),
    );
    ui.prepare_frame();
    drag(&mut ui, (0., 10.), (100., 290.));
    assert_eq!(ui.selected_text(), "Kept\nGone\nLast");
    shown.set(false);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "Kept\nLast");
    handle.unmount();
    ui.prepare_frame();
    assert!(!ui.has_text_selection());
    assert!(!ui.scene.borrow().has_text_highlights());
}

#[test]
fn the_inactive_window_colour_and_copy_text_spacing() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .gap(16.)
            .selectable(true)
            .child(row().gap(8.).children([text("Name"), text("Value")]))
            .child(text("Paragraph")),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(ui.selected_text(), "Name\tValue\n\nParagraph");
    let first = texts(&ui)[0];
    let active = ui.scene.borrow().text_highlight(first).unwrap().color;
    ui.set_window_active(false);
    let inactive = ui.scene.borrow().text_highlight(first).unwrap().color;
    assert_ne!(active, inactive);
    ui.set_window_active(true);
    assert_eq!(
        ui.scene.borrow().text_highlight(first).unwrap().color,
        active
    );
}

fn rows(ui: &mut Ui, offset: zgx::reactive::Signal<f32>) -> zgx::compose::ViewHandle {
    let handle = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Header"))
            .child(
                virtual_list(
                    offset,
                    20.,
                    1,
                    || 1000,
                    |index| index,
                    |_, key, _| text(format!("Row {key}")),
                )
                .w(300.)
                .h(100.),
            ),
    );
    ui.prepare_frame();
    handle
}

#[test]
fn a_selection_outlives_virtual_rows_recycled_out_of_view() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let _view = rows(&mut ui, offset.clone());
    // Header at 0, the list's rows from 20: row 1 at 40, row 3 at 80.
    drag(&mut ui, (6., 50.), (18., 90.));
    assert_eq!(ui.selected_text(), "ow 1\nRow 2\nRow");
    offset.set(10_000.);
    ui.prepare_frame();
    assert!(!ui.has_text_selection(), "nothing selected is mounted");
    assert!(ui.text_selection().is_some(), "but the selection remains");
    offset.set(0.);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "ow 1\nRow 2\nRow");
    assert_eq!(highlighted(&ui).len(), 3);
    // A selection from the header into the rows keeps the rows between.
    drag(&mut ui, (0., 10.), (12., 70.));
    assert_eq!(ui.selected_text(), "Header\nRow 0\nRow 1\nRo");
    offset.set(30.);
    ui.prepare_frame();
    // Row 0 is still mounted in the overscan; what shows is still selected.
    assert!(ui.selected_text().starts_with("Header\n"));
    assert!(ui.selected_text().ends_with("Row 1\nRo"));
}

#[test]
fn a_drag_held_past_a_scroll_viewport_scrolls_it_and_extends() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let _view = rows(&mut ui, offset.clone());
    press(&mut ui, 0., 30., false);
    // Below the list (it ends at 120), held there.
    move_to(&mut ui, 40., 160.);
    let start = std::time::Instant::now();
    let mut now = start;
    let deadline = ui
        .next_interaction_deadline()
        .expect("autoscroll scheduled");
    assert!(deadline > start);
    for _ in 0..30 {
        now += std::time::Duration::from_millis(16);
        ui.advance_interactions(now).unwrap();
    }
    assert!(offset.get() > 100., "scrolled: {}", offset.get());
    let text = ui.selected_text();
    let last = text.lines().last().unwrap().to_owned();
    // The focus followed the rows scrolled under the pointer's edge.
    let row: usize = last.trim_start_matches("Row ").parse().unwrap();
    assert!(
        row as f32 * 20. >= offset.get(),
        "{last} at {}",
        offset.get()
    );
    release(&mut ui, 40., 160.);
    assert!(
        ui.next_interaction_deadline().is_none(),
        "released, it stops"
    );
    // Back at the top, the anchor's row shows selected again.
    offset.set(0.);
    ui.prepare_frame();
    assert!(ui.selected_text().starts_with("Row 0\nRow 1\n"));
}

#[test]
fn the_context_menu_copies_selected_text() {
    struct Clipboard(std::cell::RefCell<Option<String>>);
    impl zgx::platform::Platform for Clipboard {
        fn clipboard_text(&self) -> Option<String> {
            self.0.borrow().clone()
        }
        fn set_clipboard_text(&self, text: String) -> bool {
            *self.0.borrow_mut() = Some(text);
            true
        }
        fn open_files(
            &self,
            _: zgx::platform::FilePicker,
        ) -> zgx::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn save_file(
            &self,
            _: zgx::platform::FilePicker,
        ) -> zgx::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
            Box::pin(async { Ok(None) })
        }
    }
    let clipboard = Rc::new(Clipboard(Default::default()));
    let mut ui = Ui::new(400., 300.);
    let outer_menu = Rc::new(Cell::new(0));
    let opened = outer_menu.clone();
    let view = ui.mount(provide(
        zgx::platform::PlatformService(clipboard.clone()),
        column()
            .size(400., 300.)
            .context_menu(move |_| {
                opened.set(opened.get() + 1);
                vec![menu_item("Outer")]
            })
            .child(
                column()
                    .w(300.)
                    .text_size(10.)
                    .line_height(20.)
                    .selectable(true)
                    .children([text("Copy these words"), text("Not these")]),
            ),
    ));
    ui.prepare_frame();
    let secondary = |ui: &mut Ui, x, y| {
        let button = PointerButton::Secondary;
        ui.dispatch(InputEvent::PointerDown { x, y, button });
        ui.dispatch(InputEvent::PointerUp { x, y, button });
        ui.prepare_frame();
    };
    drag(&mut ui, (0., 10.), (40., 10.));
    assert_eq!(ui.selected_text(), "Copy th");
    secondary(&mut ui, 10., 10.);
    assert_eq!(outer_menu.get(), 0);
    let copy = view.find("selection-copy").expect("Copy shows");
    ui.input.dispatch_to(&ui.scene, copy, InputEvent::Activate);
    assert_eq!(clipboard.0.borrow().as_deref(), Some("Copy th"));
    secondary(&mut ui, 10., 10.);
    let all = view.find("selection-select-all").unwrap();
    ui.input.dispatch_to(&ui.scene, all, InputEvent::Activate);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "Copy these words\nNot these");
    // On text that is not selected: the enclosing view's menu.
    ui.clear_text_selection();
    secondary(&mut ui, 10., 30.);
    assert_eq!(outer_menu.get(), 1);
}

#[test]
fn unmounting_a_region_under_a_drag_ends_the_drag() {
    let mut ui = Ui::new(400., 300.);
    let handle = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .selectable(true)
            .child(text("Dragged over")),
    );
    ui.prepare_frame();
    press(&mut ui, 0., 5., false);
    move_to(&mut ui, 30., 5.);
    assert!(ui.is_selecting_text());
    handle.unmount();
    ui.prepare_frame();
    assert!(!ui.is_selecting_text());
    assert!(!ui.has_text_selection());
}

#[test]
fn copied_html_keeps_paragraphs_line_breaks_and_inline_styles() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .gap(16.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(
                rich_text()
                    .child(text_span("Bold").font_bold())
                    .child(text_span(" & "))
                    .child(text_span("code").font_family(zgx::text_layout::FontFamily::Monospace)),
            )
            .child(text("a <tag>\nnext line")),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(
        ui.selected_html(),
        "<meta charset=\"utf-8\"><p><b>Bold</b> &amp; <code>code</code></p>\
         <p>a &lt;tag&gt;<br>next line</p>"
    );
}

/// Monospace text of more than one line copies as preformatted (a code
/// block), and runs coloured apart from the text the selection starts in
/// say their colour, as a page's copy keeps them.
#[test]
fn copied_html_keeps_preformatted_blocks_and_coloured_runs() {
    use zgx::{scene::Color, text_layout::FontFamily};
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(300.)
            .gap(16.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(
                rich_text()
                    .child(text_span("Plain "))
                    .child(text_span("red").text_color(Color(255, 0, 0, 255)))
                    .child(text_span(" and "))
                    .child(text_span("faint").text_color(Color(0, 0, 255, 128))),
            )
            .child(text("fn main() {\n}").font_family(FontFamily::Monospace))
            .child(text("inline").font_family(FontFamily::Monospace)),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(
        ui.selected_html(),
        "<meta charset=\"utf-8\"><p>Plain <span style=\"color:#ff0000\">red</span> and \
         <span style=\"color:rgba(0,0,255,0.502)\">faint</span></p>\
         <pre><code>fn main() {<br>}</code></pre><p><code>inline</code></p>"
    );
}

/// A list item's marker ("•", "3."), drawn as accessible-hidden text that
/// leads the item, is the list's, as a browser draws an `<li>`'s: a
/// selection takes no part of it and its plain text leaves it out, while
/// its HTML is a `<ul>`, or an `<ol>` from the number the marker reads.
#[test]
fn list_markers_are_the_lists_not_the_selections() {
    use zgx::semantics::Role;
    let item = |marker: String, body: &str| {
        row()
            .role(Role::ListItem)
            .gap(8.)
            .child(text(marker).accessible_hidden())
            .child(text(body.to_owned()))
    };
    let list = |markers: [&str; 2], bodies: [&str; 2]| {
        column().role(Role::List).children([
            item(markers[0].into(), bodies[0]),
            item(markers[1].into(), bodies[1]),
        ])
    };
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            text("Before"),
            list(["•", "•"], ["one", "two"]),
            list(["3.", "4."], ["three", "four"]),
            text("After"),
        ],
    );
    let all = texts(&ui);
    assert!(ui.select_all_text(all[0]));
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "Before\none\ntwo\nthree\nfour\nAfter");
    let scene = ui.scene.borrow();
    for marker in [all[1], all[3], all[5], all[7]] {
        assert!(
            scene.text_highlight(marker).is_none(),
            "a marker shows unselected"
        );
    }
    drop(scene);
    assert_eq!(
        ui.selected_html(),
        "<meta charset=\"utf-8\"><p>Before</p><ul><li>one</li><li>two</li></ul>\
         <ol start=\"3\"><li>three</li><li>four</li></ol><p>After</p>"
    );
}

/// A list in a list item nests, in its `<li>`.
#[test]
fn copied_html_nests_lists() {
    use zgx::semantics::Role;
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![
            column().role(Role::List).children([
                row()
                    .role(Role::ListItem)
                    .child(text("1.").accessible_hidden())
                    .child(
                        column().child(text("outer")).child(
                            column().role(Role::List).child(
                                row()
                                    .role(Role::ListItem)
                                    .child(text("•").accessible_hidden())
                                    .child(text("inner")),
                            ),
                        ),
                    ),
                row()
                    .role(Role::ListItem)
                    .child(text("2.").accessible_hidden())
                    .child(text("next")),
            ]),
        ],
    );
    assert!(ui.select_all_text(texts(&ui)[0]));
    assert_eq!(
        ui.selected_html(),
        "<meta charset=\"utf-8\"><ol><li>outer<ul><li>inner</li></ul></li><li>next</li></ol>"
    );
    assert_eq!(ui.selected_text(), "outer\ninner\nnext");
}

/// A list that is itself the selectable root copies as the same list does
/// inside a selectable box: as a list, not a paragraph of its items.
#[test]
fn a_list_that_is_the_selectable_root_copies_as_a_list() {
    use zgx::semantics::Role;
    let item = |marker: &str, body: &str| {
        row()
            .role(Role::ListItem)
            .gap(8.)
            .child(text(marker.to_owned()).accessible_hidden())
            .child(text(body.to_owned()))
    };
    let list = || {
        column()
            .role(Role::List)
            .children([item("3.", "three"), item("4.", "four")])
    };
    let copied = |view: View| {
        let mut ui = Ui::new(400., 300.);
        ui.mount(view);
        ui.prepare_frame();
        assert!(ui.select_all_text(texts(&ui)[0]));
        ui.prepare_frame();
        (ui.selected_html(), ui.selected_text())
    };
    let wrapped = copied(column().selectable(true).child(list()));
    let root = copied(list().selectable(true));
    assert_eq!(
        wrapped.0,
        "<meta charset=\"utf-8\"><ol start=\"3\"><li>three</li><li>four</li></ol>"
    );
    assert_eq!(root, wrapped);
    assert_eq!(root.1, "three\nfour");
    // A list in one of its items nests in the root list's `<li>`.
    let nested = copied(
        column()
            .role(Role::List)
            .selectable(true)
            .child(
                row()
                    .role(Role::ListItem)
                    .child(text("1.").accessible_hidden())
                    .child(
                        column()
                            .child(text("outer"))
                            .child(column().role(Role::List).child(item("•", "inner"))),
                    ),
            )
            .child(item("2.", "next")),
    );
    assert_eq!(
        nested.0,
        "<meta charset=\"utf-8\"><ol><li>outer<ul><li>inner</li></ul></li><li>next</li></ol>"
    );
    assert_eq!(nested.1, "outer\ninner\nnext");
}

#[test]
fn copied_html_keeps_headings_links_and_rows_a_list_has_not_mounted() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(300.)
            .gap(16.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Notes").heading(1))
            .child(
                rich_text()
                    .child(text_span("See "))
                    .child(text_span("the \"docs\"").href("https://zgx.dev/a?b=1&c=2"))
                    .child(text_span(".")),
            )
            .child(
                virtual_list(
                    offset.clone(),
                    20.,
                    1,
                    || 100,
                    |index| index,
                    |_, key, _| text(format!("Row {key}")),
                )
                .row_text(|index| (index < 100).then(|| format!("Row {index}")))
                .w(300.)
                .h(60.),
            ),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    let html = ui.selected_html();
    assert!(
        html.starts_with(
            "<meta charset=\"utf-8\"><h1>Notes</h1><p>See \
             <a href=\"https://zgx.dev/a?b=1&amp;c=2\">the &quot;docs&quot;</a>.</p><p>Row 0<br>Row 1"
        ),
        "{html}"
    );
    // Every row, the unmounted ones from the row text, as plain text has them.
    assert!(html.ends_with("<br>Row 98<br>Row 99</p>"), "{html}");
    assert_eq!(html.matches("Row ").count(), 100);
    assert_eq!(ui.selected_text().matches("Row ").count(), 100);
    // Rows mounted later show selected too, as all of it is.
    offset.set(1000.);
    ui.prepare_frame();
    let shown = visible(&ui);
    assert!(!shown.is_empty());
    let scene = ui.scene.borrow();
    assert!(shown.iter().all(|id| scene.text_highlight(*id).is_some()));
    drop(scene);
    assert_eq!(ui.selected_text().matches("Row ").count(), 100);
}

#[test]
fn rows_of_lists_next_to_each_other_copy_in_document_order() {
    let mut ui = Ui::new(400., 300.);
    let list = |ui: &Ui, name: &'static str| {
        virtual_list(
            ui.signal(0.),
            20.,
            0,
            || 50,
            |index| index,
            move |_, key, _| text(format!("{name} {key}")),
        )
        .row_text(move |index| (index < 50).then(|| format!("{name} {index}")))
        .w(300.)
        .h(40.)
    };
    let (a, b, c) = (list(&ui, "A"), list(&ui, "B"), list(&ui, "C"));
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Start"))
            .children(vec![a, b, c]),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    let copied = ui.selected_text();
    let expected: Vec<String> = std::iter::once("Start".to_owned())
        .chain(
            ["A", "B", "C"]
                .iter()
                .flat_map(|name| (0..50).map(move |i| format!("{name} {i}"))),
        )
        .collect();
    assert_eq!(copied.lines().collect::<Vec<_>>(), expected);
}

#[test]
fn highlights_out_of_view_are_laid_out_as_they_scroll_in() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(
                scroll(offset.clone())
                    .w(300.)
                    .h(100.)
                    .child(column().children((0..200).map(|i| text(format!("Line {i}"))))),
            ),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    ui.prepare_frame();
    let all = texts(&ui);
    let lit = |ui: &Ui, id| ui.scene.borrow().text_highlight(id).is_some();
    // A screenful laid out; the rest wait for view.
    assert!(lit(&ui, all[0]) && lit(&ui, all[4]));
    assert!(!lit(&ui, all[150]));
    assert_eq!(
        ui.selected_text().lines().count(),
        200,
        "all of it is selected"
    );
    offset.set(150. * 20.);
    ui.prepare_frame();
    assert!(lit(&ui, all[150]) && lit(&ui, all[152]));
    let scene = ui.scene.borrow();
    let rects = &scene.text_highlight(all[150]).unwrap().rects;
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].height, 20.);
}

#[test]
fn a_list_without_row_text_copies_the_text_its_rows_show() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Header"))
            .child(
                virtual_list(
                    offset,
                    20.,
                    1,
                    || 500,
                    |index| index,
                    |_, key, _| {
                        row()
                            .child(text(format!("{key:03}")))
                            .child(text_signal(move || format!("Row {key}")))
                    },
                )
                .w(300.)
                .h(100.),
            ),
    );
    ui.prepare_frame();
    assert!(ui.select_all_text(texts(&ui)[0]));
    let copied = ui.selected_text();
    let lines: Vec<&str> = copied.lines().collect();
    assert_eq!(lines.len(), 501, "{copied}");
    assert_eq!(lines[1], "000\tRow 0");
    assert_eq!(lines[500], "499\tRow 499");
    // A press on a row collapses the selection there: nothing to copy, the
    // row it is in included.
    press(&mut ui, 4., 50., false);
    release(&mut ui, 4., 50.);
    assert_eq!(ui.selected_text(), "");
    // Ending at the start of a row takes none of it.
    drag(&mut ui, (6., 30.), (0., 70.));
    assert_eq!(ui.selected_text(), "00\tRow 0\n001\tRow 1");
}

#[cfg(target_os = "macos")]
#[test]
fn a_control_click_on_macos_opens_the_menu_and_keeps_the_selection() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Keep this selected")),
    );
    ui.prepare_frame();
    drag(&mut ui, (0., 10.), (24., 10.));
    assert_eq!(ui.selected_text(), "Keep");
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let button = PointerButton::Primary;
    ui.dispatch_with_modifiers(
        InputEvent::PointerDown {
            x: 6.,
            y: 10.,
            button,
        },
        control,
    );
    ui.dispatch_with_modifiers(
        InputEvent::PointerUp {
            x: 6.,
            y: 10.,
            button,
        },
        control,
    );
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "Keep");
    assert!(view.find("selection-copy").is_some(), "the menu opened");
}

fn shift(key: Key, word: bool, line: bool) -> InputEvent {
    let mac = cfg!(target_os = "macos");
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift: true,
            alt: mac && word,
            control: !mac && word,
            meta: mac && line,
        },
        repeat: false,
    }
}

#[test]
fn shift_keys_extend_the_selection_across_text_nodes() {
    let mut ui = Ui::new(400., 300.);
    document(
        &mut ui,
        vec![text("alpha beta"), text("gamma delta"), text("a👩‍💻b")],
    );
    // A press leaves a caret at "al|pha"; Shift-keys extend from it.
    press(&mut ui, 12., 10., false);
    release(&mut ui, 12., 10.);
    for _ in 0..3 {
        assert!(
            ui.dispatch(shift(Key::ArrowRight, false, false))
                .default_prevented
        );
    }
    assert_eq!(ui.selected_text(), "pha");
    ui.dispatch(shift(Key::ArrowRight, true, false));
    assert_eq!(ui.selected_text(), "pha beta");
    // Past the node's end, a word step lands at the next text's first word.
    ui.dispatch(shift(Key::ArrowRight, true, false));
    assert_eq!(ui.selected_text(), "pha beta\ngamma");
    ui.dispatch(shift(Key::ArrowLeft, false, false));
    assert_eq!(ui.selected_text(), "pha beta\ngamm");
    // To the line's end, then a line down: into the next text at the caret's x.
    let edge = if cfg!(target_os = "macos") {
        shift(Key::ArrowRight, false, true)
    } else {
        shift(Key::End, false, false)
    };
    ui.dispatch(edge);
    assert_eq!(ui.selected_text(), "pha beta\ngamma delta");
    ui.dispatch(shift(Key::ArrowDown, false, false));
    assert_eq!(ui.selected_text(), "pha beta\ngamma delta\na👩‍💻b");
    // Back over the emoji: a whole grapheme cluster at a time.
    ui.dispatch(shift(Key::ArrowLeft, false, false));
    ui.dispatch(shift(Key::ArrowLeft, false, false));
    assert_eq!(ui.selected_text(), "pha beta\ngamma delta\na");
    // To the region's start, before the anchor.
    let start = if cfg!(target_os = "macos") {
        shift(Key::ArrowUp, false, true)
    } else {
        InputEvent::KeyDown {
            key: Key::Home,
            modifiers: Modifiers {
                shift: true,
                control: true,
                ..Modifiers::default()
            },
            repeat: false,
        }
    };
    ui.dispatch(start);
    assert_eq!(ui.selected_text(), "al");
    // Without a selection, Shift-arrows are left to the app.
    ui.clear_text_selection();
    assert!(
        !ui.dispatch(shift(Key::ArrowRight, false, false))
            .default_prevented
    );
}

/// Shift with Right or Left extends the way the block runs, as Chrome does:
/// Right goes back through a right-to-left paragraph's text, Left on.
#[test]
fn shift_arrows_extend_the_way_a_right_to_left_block_runs() {
    use zgx::{scene::Direction, selection::TextPoint};
    for rtl in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let view = ui.mount(
            column()
                .w(300.)
                .text_size(10.)
                .line_height(20.)
                .selectable(true)
                .direction(if rtl { Direction::Rtl } else { Direction::Ltr })
                .child(text("שלום עולם").id("text")),
        );
        ui.prepare_frame();
        let node = view.find("text").unwrap();
        // Between the words.
        let caret = TextPoint {
            node,
            offset: "שלום".len(),
        };
        assert!(ui.select_text(caret, caret));
        ui.dispatch(shift(Key::ArrowRight, false, false));
        assert_eq!(ui.selected_text(), if rtl { "ם" } else { " " }, "rtl {rtl}");
        ui.dispatch(shift(Key::ArrowLeft, false, false));
        ui.dispatch(shift(Key::ArrowLeft, false, false));
        assert_eq!(ui.selected_text(), if rtl { " " } else { "ם" }, "rtl {rtl}");
    }
}

#[test]
fn a_triple_press_on_a_wrapped_paragraph_selects_all_its_lines() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(66.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("one two three four").w_full().text_wrap(true)),
    );
    ui.prepare_frame();
    // The second visual line ("three").
    for _ in 0..3 {
        press(&mut ui, 6., 30., false);
        release(&mut ui, 6., 30.);
    }
    assert_eq!(ui.selected_text(), "one two three four");
}

#[test]
fn a_selection_across_a_scroll_viewport_takes_what_it_scrolled_away() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Above"))
            .child(
                scroll(offset.clone())
                    .w(300.)
                    .h(40.)
                    .child(column().children((0..5).map(|i| text(format!("Row {i}"))))),
            )
            .child(text("Below")),
    );
    ui.prepare_frame();
    offset.set(40.);
    ui.prepare_frame();
    // Rows 0 and 1 are scrolled out of view, rows 4 below it: all between.
    drag(&mut ui, (0., 10.), (60., 70.));
    assert_eq!(
        ui.selected_text(),
        "Above\nRow 0\nRow 1\nRow 2\nRow 3\nRow 4\nBelow"
    );
    // From inside the viewport out past its bottom, onto the text there
    // (a click first: a drag from inside the selection would drag its text).
    press(&mut ui, 200., 290., false);
    release(&mut ui, 200., 290.);
    drag(&mut ui, (0., 30.), (12., 70.));
    assert_eq!(ui.selected_text(), "Row 2\nRow 3\nRow 4\nBe");
}

#[test]
fn text_in_a_popover_selects_and_its_press_does_not_end_it() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(true);
    let view = ui.mount(
        column().w(300.).text_size(10.).line_height(20.).child(
            popover("Details", open.clone(), button().child(text("Open")))
                .w(200.)
                .selectable(true)
                .child(text("Popover text").id("inside")),
        ),
    );
    ui.prepare_frame();
    let inside = view.find("inside").unwrap();
    let b = ui.scene.borrow().bounds(inside);
    drag(&mut ui, (b.x, b.y + 10.), (b.x + 42., b.y + 10.));
    assert_eq!(ui.selected_text(), "Popover");
    assert!(open.get(), "selecting in it keeps it open");
}

#[test]
fn extending_past_a_scroll_viewport_with_the_keyboard_scrolls_it() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        scroll(offset.clone())
            .w(300.)
            .h(40.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(column().children((0..6).map(|i| text(format!("Row {i}"))))),
    );
    ui.prepare_frame();
    press(&mut ui, 0., 10., false);
    release(&mut ui, 0., 10.);
    for _ in 0..3 {
        ui.dispatch(shift(Key::ArrowDown, false, false));
        ui.prepare_frame();
    }
    assert_eq!(ui.selected_text(), "Row 0\nRow 1\nRow 2");
    assert!(
        offset.get() >= 40.,
        "scrolled to the focus: {}",
        offset.get()
    );
}

#[test]
fn selected_text_keeps_its_colour_unless_styled_and_regions_override_the_theme() {
    use zgx::{scene::Color, theme::ThemeOverride, theme::tok};
    let mut ui = Ui::new(400., 300.);
    let tint = ui.signal(Color(255, 0, 0, 255));
    let followed = tint.clone();
    ui.mount(
        column().w(300.).text_size(10.).line_height(20.).children([
            column().selectable(true).child(text("Plain")),
            column()
                .selectable(true)
                .selection_color(Color(0, 0, 255, 128))
                .selection_radius(6.)
                .selected_text_color_when(move || followed.get())
                .child(text("Styled")),
            theme_scope(
                ThemeOverride::new()
                    .with(tok::SELECTION_TEXT, Color(0, 255, 0, 255))
                    .with(tok::SELECTION_RADIUS, 3.),
                column().selectable(true).child(text("Scoped")),
            ),
        ]),
    );
    ui.prepare_frame();
    let [plain, styled, scoped] = texts(&ui)[..] else {
        panic!()
    };
    let highlight = |ui: &Ui, node| {
        assert!(ui.select_all_text(node));
        ui.scene.borrow().text_highlight(node).unwrap().clone()
    };
    let default = highlight(&ui, plain);
    assert_eq!(default.text_color, None, "glyphs keep their colour");
    assert_eq!(default.radius, 3., "rounded by default");
    let own = highlight(&ui, styled);
    assert_eq!(own.color, Color(0, 0, 255, 128));
    assert_eq!(own.radius, 6.);
    assert_eq!(own.text_color, Some(Color(255, 0, 0, 255)));
    // The region's colours hold in a window at the back too.
    ui.set_window_active(false);
    let behind = ui.scene.borrow().text_highlight(styled).unwrap().clone();
    assert_eq!(
        (behind.color, behind.text_color),
        (own.color, own.text_color)
    );
    ui.set_window_active(true);
    tint.set(Color(255, 255, 0, 255));
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().text_highlight(styled).unwrap().text_color,
        Some(Color(255, 255, 0, 255)),
        "a reactive colour follows"
    );
    let themed = highlight(&ui, scoped);
    assert_eq!(themed.text_color, Some(Color(0, 255, 0, 255)));
    assert_eq!(themed.radius, 3.);
    // A glyph recolours when its centre is in a highlight rectangle.
    let r = themed.rects[0];
    assert_eq!(
        themed.glyph_color(r.x + 1., r.y + 1.),
        Some(Color(0, 255, 0, 255))
    );
    assert_eq!(themed.glyph_color(r.x + r.width + 5., r.y + 1.), None);
}

#[test]
fn hit_testing_follows_scale_and_rotation() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .w(100.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .scale(2.)
            .transform_origin(0., 0.)
            .child(text("abcdefghij")),
    );
    ui.prepare_frame();
    // Drawn twice as large: a character is 12 points wide on screen.
    drag(&mut ui, (24., 20.), (72., 20.));
    assert_eq!(ui.selected_text(), "cdef");
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column().size(400., 300.).child(
            column()
                .ml(100.)
                .w(60.)
                .h(20.)
                .text_size(10.)
                .line_height(20.)
                .selectable(true)
                .rotate(std::f32::consts::FRAC_PI_2)
                .transform_origin(0., 0.)
                .child(text("abcdefghij")),
        ),
    );
    ui.prepare_frame();
    // Turned a quarter clockwise about its corner (100, 0): the text runs
    // down the screen, left of the corner.
    drag(&mut ui, (90., 12.), (90., 36.));
    assert_eq!(ui.selected_text(), "cdef");
}

#[test]
fn copying_takes_rows_a_virtual_list_has_not_mounted_from_its_row_text() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .selectable(true)
            .child(text("Header"))
            .child(
                virtual_list(
                    offset.clone(),
                    20.,
                    1,
                    || 1000,
                    |index| index,
                    |_, key, _| text(format!("Row {key}")),
                )
                .row_text(|index| (index < 1000).then(|| format!("Row {index}")))
                .w(300.)
                .h(100.),
            )
            .child(text("Footer")),
    );
    ui.prepare_frame();
    let every = std::iter::once("Header".to_owned())
        .chain((0..1000).map(|i| format!("Row {i}")))
        .chain(std::iter::once("Footer".to_owned()))
        .collect::<Vec<_>>()
        .join("\n");
    press(&mut ui, 0., 10., false);
    release(&mut ui, 0., 10.);
    assert!(ui.dispatch(shortcut("a")).default_prevented);
    assert_eq!(ui.selected_text(), every, "the rows it has not mounted too");
    // Scrolled to the middle, the mounted rows copy as they show, the
    // rest from the row text.
    offset.set(10_000.);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), every);
    // A selection whose rows all scrolled away still copies them, cut
    // where it was: no more than was selected.
    offset.set(0.);
    ui.prepare_frame();
    // A click first: a drag from inside the selection drags its text.
    press(&mut ui, 200., 290., false);
    release(&mut ui, 200., 290.);
    drag(&mut ui, (6., 50.), (18., 90.));
    assert_eq!(ui.selected_text(), "ow 1\nRow 2\nRow");
    offset.set(10_000.);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "ow 1\nRow 2\nRow");
    // One end scrolled away, the other in view.
    offset.set(0.);
    ui.prepare_frame();
    press(&mut ui, 200., 290., false);
    release(&mut ui, 200., 290.);
    drag(&mut ui, (12., 50.), (12., 110.));
    assert_eq!(ui.selected_text(), "w 1\nRow 2\nRow 3\nRo");
    // Row 1 is two rows above the view, past the overscan.
    offset.set(60.);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), "w 1\nRow 2\nRow 3\nRo");
    // Within one row, scrolled away.
    drag(&mut ui, (6., 70.), (18., 70.));
    let within = ui.selected_text();
    offset.set(10_000.);
    ui.prepare_frame();
    assert_eq!(ui.selected_text(), within);
}

/// Select-all and copy over a long document, as a page of 20,000 paragraphs:
/// `cargo test --release -p zgx --test text long_document -- --ignored --nocapture`.
#[test]
#[ignore = "a timing probe"]
fn long_document_select_all_and_copy_timings() {
    use std::time::Instant;
    let mut ui = Ui::new(800., 600.);
    let offset = ui.signal(0.);
    let paragraphs = 20_000;
    ui.mount(
        column()
            .w(800.)
            .h(600.)
            .text_size(14.)
            .line_height(20.)
            .selectable(true)
            .child(
                scroll(offset.clone())
                    .w(800.)
                    .h(600.)
                    .child(column().gap(10.).children((0..paragraphs).map(|i| {
                        text(format!(
                            "Paragraph {i}: the quick brown fox jumps over the lazy dog, \
                         and keeps running past the edge of the field until it wraps."
                        ))
                        .w_full()
                        .text_wrap(true)
                    }))),
            ),
    );
    let start = Instant::now();
    ui.prepare_frame();
    eprintln!("mount and first frame: {:?}", start.elapsed());
    let first = texts(&ui)[0];
    let start = Instant::now();
    assert!(ui.select_all_text(first));
    eprintln!("select all: {:?}", start.elapsed());
    let start = Instant::now();
    ui.prepare_frame();
    eprintln!("frame after select all: {:?}", start.elapsed());
    let start = Instant::now();
    let text = ui.selected_text();
    eprintln!("copy as text: {:?} ({} bytes)", start.elapsed(), text.len());
    let start = Instant::now();
    let html = ui.selected_html();
    eprintln!("copy as HTML: {:?} ({} bytes)", start.elapsed(), html.len());
    assert_eq!(text.lines().filter(|l| !l.is_empty()).count(), paragraphs);
    let start = Instant::now();
    for step in 1..=10 {
        offset.set(step as f32 * 300.);
        ui.prepare_frame();
    }
    eprintln!("scroll frame, all selected: {:?}", start.elapsed() / 10);
    ui.clear_text_selection();
    let start = Instant::now();
    for step in 1..=10 {
        offset.set(3000. + step as f32 * 300.);
        ui.prepare_frame();
    }
    eprintln!("scroll frame, nothing selected: {:?}", start.elapsed() / 10);
}

/// Selected text drags out of a selectable region as browsers drag a
/// selection: into a field it copies (the region's text stays), to other
/// apps as plain text; a press on the selection let go is a click there.
#[test]
fn a_selection_drags_into_a_field_and_a_press_on_it_let_go_clicks() {
    use zgx::input::{DragItem, DraggedText};
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal(String::new());
    let view = ui.mount(
        column()
            .w(300.)
            .text_size(10.)
            .line_height(20.)
            .child(column().selectable(true).child(text("Hello world")))
            .child(
                text_input("Field", value.clone())
                    .id("field")
                    .size(200., 30.),
            ),
    );
    ui.prepare_frame();
    // Characters 6 points wide: "world" spans 36 to 66.
    for _ in 0..2 {
        press(&mut ui, 50., 10., false);
        release(&mut ui, 50., 10.);
    }
    assert_eq!(ui.selected_text(), "world");
    // Pressed elsewhere on it than the double click (so not a third click).
    press(&mut ui, 62., 10., false);
    ui.dispatch(InputEvent::PointerMove { x: 72., y: 10. });
    let state = ui.input.drag_state().expect("dragging the selection");
    assert_eq!(
        state.payload.downcast_ref::<DraggedText>().unwrap().text(),
        "world"
    );
    assert_eq!(
        state.export.as_deref(),
        Some(&[DragItem::Text("world".into())][..])
    );
    let field = ui.scene.borrow().bounds(view.find("field").unwrap());
    let (x, y) = (field.x + 20., field.y + field.height / 2.);
    ui.dispatch(InputEvent::PointerMove { x, y });
    release(&mut ui, x, y);
    ui.prepare_frame();
    assert_eq!(value.get(), "world", "dropped into the field");
    assert_eq!(
        ui.selected_text(),
        "world",
        "the region's text stays, still selected"
    );
    // Pressed on the selection and let go without moving: a click there.
    press(&mut ui, 50., 10., false);
    assert_eq!(ui.selected_text(), "world", "kept while pressed");
    release(&mut ui, 50., 10.);
    assert_eq!(ui.selected_text(), "", "a click collapses it");
}

/// Extending past a zoomed scroll view's edge with the keyboard scrolls it
/// just enough to show the focus, in its own points, as unzoomed.
#[test]
fn extending_in_a_zoomed_scroll_view_scrolls_just_enough() {
    for zoom in [1., 2.] {
        let mut ui = Ui::new(300., 200.);
        let offset = ui.signal(0_f32);
        ui.mount(
            column().zoom(zoom).child(
                scroll(offset.clone()).w(150.).h(100.).child(
                    column()
                        .w(150.)
                        .text_size(10.)
                        .line_height(20.)
                        .selectable(true)
                        .children((0..40).map(|i| text(format!("Line {i}")))),
                ),
            ),
        );
        ui.prepare_frame();
        press(&mut ui, 2., 10., false);
        release(&mut ui, 2., 10.);
        for _ in 0..10 {
            ui.dispatch(shift(Key::ArrowDown, false, false));
            ui.prepare_frame();
        }
        // Line 10 ends at 220; the viewport is 100 tall in its own points.
        assert!(
            (offset.get() - 120.).abs() < 1.,
            "at {zoom}×: scrolled {}",
            offset.get()
        );
    }
}

mod highlight_shape {
    use zgx::scene::{Color, HighlightPiece, Rect, TextHighlight};

    fn pieces(rects: &[Rect], radius: f32, origin: (f32, f32), scale: f32) -> Vec<HighlightPiece> {
        TextHighlight {
            color: Color(0, 0, 255, 80),
            rects: rects.to_vec(),
            text_color: None,
            radius,
        }
        .pieces(origin.0, origin.1, scale)
    }
    fn runs(pieces: &[HighlightPiece]) -> Vec<(Rect, [f32; 4])> {
        (pieces.iter())
            .filter_map(|piece| match piece {
                HighlightPiece::Run { rect, radii } => Some((*rect, *radii)),
                _ => None,
            })
            .collect()
    }
    fn fillets(pieces: &[HighlightPiece]) -> Vec<(Rect, (f32, f32), f32)> {
        (pieces.iter())
            .filter_map(|piece| match piece {
                HighlightPiece::Fillet {
                    rect,
                    centre,
                    radius,
                } => Some((*rect, *centre, *radius)),
                _ => None,
            })
            .collect()
    }
    /// By more than rounding: every edge lies on a whole pixel.
    fn overlap(a: Rect, b: Rect) -> bool {
        let e = 1e-3;
        a.x + e < b.x + b.width
            && b.x + e < a.x + a.width
            && a.y + e < b.y + b.height
            && b.y + e < a.y + a.height
    }
    /// No two pieces cover the same point, so a translucent colour stays even.
    fn assert_no_overlap(pieces: &[HighlightPiece]) {
        let area = |piece: &HighlightPiece| match piece {
            HighlightPiece::Run { rect, .. } | HighlightPiece::Fillet { rect, .. } => *rect,
        };
        for (i, a) in pieces.iter().enumerate() {
            for b in &pieces[i + 1..] {
                assert!(!overlap(area(a), area(b)), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn lines_of_different_lengths_are_one_shape() {
        // A selection from mid-line over a full line to mid-line.
        let shape = pieces(
            &[
                Rect::new(40., 0., 120., 20.),
                Rect::new(0., 20., 160., 20.),
                Rect::new(0., 40., 60., 20.),
            ],
            4.,
            (0., 0.),
            2.,
        );
        assert_no_overlap(&shape);
        let lines = runs(&shape);
        // Round where nothing meets it; square where a line runs on from it.
        assert_eq!(lines[0].1, [4., 4., 0., 0.]);
        assert_eq!(lines[1].1, [4., 0., 4., 0.]);
        assert_eq!(lines[2].1, [0., 0., 4., 4.]);
        // The two inner corners curve the other way, outside their lines.
        assert_eq!(
            fillets(&shape),
            [
                (Rect::new(36., 16., 4., 4.), (36., 16.), 4.),
                (Rect::new(60., 40., 4., 4.), (64., 44.), 4.),
            ]
        );
    }

    #[test]
    fn lines_share_an_edge_on_whole_pixels_at_any_scale() {
        for scale in [1., 1.25, 1.5, 1.75, 2., 3.] {
            for origin in [0., 0.3, 7.6, 100.45] {
                let rects: Vec<Rect> = (0..6)
                    .map(|i| Rect::new(0., i as f32 * 17.3, 200. - i as f32 * 25., 17.3))
                    .collect();
                let shape = pieces(&rects, 3., (origin, origin), scale);
                assert_no_overlap(&shape);
                let lines = runs(&shape);
                assert_eq!(lines.len(), 6);
                for pair in lines.windows(2) {
                    let (above, below) = (pair[0].0, pair[1].0);
                    let seam = (above.y + above.height - below.y).abs();
                    assert!(seam < 1e-3, "{above:?} over {below:?} at {scale}");
                }
                for (rect, _) in &lines {
                    let device = rect.y * scale;
                    assert!(
                        (device - device.round()).abs() < 1e-3,
                        "{rect:?} at {scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn lines_overlapping_in_a_tight_line_height_do_not_darken() {
        let shape = pieces(
            &[Rect::new(0., 0., 100., 20.), Rect::new(0., 16., 80., 20.)],
            3.,
            (0., 0.),
            2.,
        );
        assert_no_overlap(&shape);
        let lines = runs(&shape);
        assert_eq!(lines[0].0.y + lines[0].0.height, 18.);
        assert_eq!(lines[1].0.y, 18.);
    }

    #[test]
    fn a_step_shorter_than_the_radius_takes_half_of_it_each_way() {
        let shape = pieces(
            &[Rect::new(0., 0., 100., 20.), Rect::new(0., 20., 104., 20.)],
            4.,
            (0., 0.),
            2.,
        );
        assert_eq!(
            runs(&shape)[0].1[2],
            0.,
            "the shorter line's corner curves in"
        );
        assert_eq!(runs(&shape)[1].1[1], 2., "the longer line's stays round");
        assert_eq!(
            fillets(&shape),
            [(Rect::new(100., 18., 2., 2.), (102., 18.), 2.)]
        );
    }

    #[test]
    fn the_radius_is_at_most_half_a_line_and_half_a_run() {
        let shape = pieces(&[Rect::new(0., 0., 100., 12.)], 40., (0., 0.), 2.);
        assert_eq!(runs(&shape)[0].1, [6.; 4]);
        // An empty line's sliver.
        let shape = pieces(&[Rect::new(0., 0., 5., 20.)], 4., (0., 0.), 2.);
        assert_eq!(runs(&shape)[0].1, [2.5; 4]);
    }

    #[test]
    fn runs_of_mixed_direction_stay_apart_or_join_on_a_line() {
        // Two runs apart on one line, then a whole line under them.
        let shape = pieces(
            &[
                Rect::new(0., 0., 40., 20.),
                Rect::new(100., 0., 40., 20.),
                Rect::new(0., 20., 140., 20.),
            ],
            4.,
            (0., 0.),
            2.,
        );
        assert_no_overlap(&shape);
        assert_eq!(runs(&shape).len(), 3);
        assert_eq!(fillets(&shape).len(), 2, "either side of the gap");
        // Touching runs of one line are one run, with no seam.
        let shape = pieces(
            &[Rect::new(0., 0., 40., 20.), Rect::new(40.2, 0., 40., 20.)],
            4.,
            (0., 0.),
            2.,
        );
        assert_eq!(runs(&shape).len(), 1);
    }

    #[test]
    fn lines_with_a_gap_between_them_round_apart() {
        let shape = pieces(
            &[Rect::new(0., 0., 100., 20.), Rect::new(0., 40., 100., 20.)],
            4.,
            (0., 0.),
            2.,
        );
        assert_eq!(runs(&shape)[0].1, [4.; 4]);
        assert_eq!(runs(&shape)[1].1, [4.; 4]);
    }
}
