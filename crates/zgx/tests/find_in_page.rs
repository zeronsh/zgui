//! Find in page as a browser's: Command-F (Control-F elsewhere) opens a bar,
//! what is typed is found in all the window's text, case and accents
//! ignored, in document order, virtual lists' rows counted without mounting
//! them, each match marked and the current one apart; Enter and Command-G
//! step, wrapping; Escape closes it, the match selected. An app binding the
//! key keeps it, and it can be turned off.
use std::{cell::Cell, rc::Rc};
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    find::{FindStatus, FoundAt},
    input::{InputEvent, Key, Modifiers},
    scene::NodeKind,
    widgets::Ui,
};

fn command() -> Modifiers {
    Modifiers {
        meta: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}
fn key(ui: &mut Ui, key: Key, modifiers: Modifiers) -> bool {
    let prevented = ui
        .dispatch(InputEvent::KeyDown {
            key,
            modifiers,
            repeat: false,
        })
        .default_prevented;
    ui.prepare_frame();
    prevented
}
/// The text nodes showing marks, by their text, and how many each (an
/// editor's matches, on an empty node of their own, left out).
fn marked(ui: &Ui) -> Vec<(String, usize)> {
    let scene = ui.scene.borrow();
    let mut marked: Vec<(String, usize)> = scene
        .marked_text()
        .filter_map(|id| match scene.kind(id) {
            NodeKind::Text { text, .. } if text.is_empty() => None,
            NodeKind::Text { text, .. } => Some((text.to_string(), scene.text_marks(id).len())),
            NodeKind::RichText { text } => {
                Some((text.text().to_string(), scene.text_marks(id).len()))
            }
            _ => None,
        })
        .collect();
    marked.sort();
    marked
}
/// Run counting slices until none is due, as a host does on its timer.
fn settle(ui: &mut Ui) -> usize {
    let mut slices = 0;
    while let Some(due) = ui.next_interaction_deadline() {
        ui.advance_interactions(due).unwrap();
        slices += 1;
        assert!(slices < 10_000, "counting never ends");
    }
    slices
}

#[test]
fn matches_ignore_case_and_accents_and_go_in_document_order_wrapping() {
    let mut ui = Ui::new(500., 400.);
    let field = ui.signal(String::from("a CAFE order"));
    ui.mount(
        column()
            .child(text("Café latte"))
            .child(
                rich_text()
                    .child(text_span("then "))
                    .child(text_span("cafe").href("https://example.org")),
            )
            .child(text_input("Order", field).id("field").w(200.))
            .child(text("no match here")),
    );
    ui.prepare_frame();
    ui.find_text("cafe");
    let status = ui.find_status();
    assert_eq!(
        status,
        FindStatus {
            open: true,
            query: "cafe".into(),
            count: 3,
            done: true,
            current: Some(0),
        }
    );
    let places: Vec<_> = ui
        .find_results(10)
        .iter()
        .map(|found| ui.find_context(found, 3))
        .collect();
    assert_eq!(places, ["[Café] la", "en [cafe]", "a [CAFE] or"]);
    assert!(matches!(ui.find_results(10)[2].at, FoundAt::Editor(_)));
    // Every mounted match marked, the bar's own text not searched.
    assert_eq!(
        marked(&ui),
        [("Café latte".to_string(), 1), ("then cafe".to_string(), 1)]
    );
    ui.find_next();
    assert_eq!(ui.find_status().current, Some(1));
    ui.find_next();
    ui.find_next();
    assert_eq!(
        ui.find_status().current,
        Some(0),
        "past the last, the first"
    );
    ui.find_previous();
    assert_eq!(ui.find_status().current, Some(2), "and back");
    ui.close_find();
    assert!(marked(&ui).is_empty(), "closed: no marks");
    assert_eq!(ui.find_status(), FindStatus::default());
}

#[test]
fn a_lists_rows_are_counted_unmounted_in_slices_and_then_nothing_runs() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let name = |n: usize| match n {
        150_000 => "a needle here".to_string(),
        n => format!("Row {n}"),
    };
    ui.mount(
        column().child(text("needle before the list")).child(
            virtual_list(
                offset.clone(),
                20.,
                1,
                || 200_000,
                |n| n,
                move |_, n, _| {
                    count.set(count.get() + 1);
                    text(name(n))
                },
            )
            .row_text(move |n| (n < 200_000).then(|| name(n)))
            .w(300.)
            .h(100.),
        ),
    );
    ui.prepare_frame();
    let built = builds.get();
    ui.find_text("NEEDLE");
    assert!(!ui.find_status().done, "the rows left to count");
    assert!(settle(&mut ui) > 1, "counted a slice a frame");
    let status = ui.find_status();
    assert!(status.done);
    assert_eq!(status.count, 2, "{status:?}");
    assert!(matches!(
        ui.find_results(5)[1].at,
        FoundAt::Row { row: 150_000, .. }
    ));
    assert!(builds.get() - built < 20, "counting built no rows");
    // Open and counted: nothing left to run.
    assert!(ui.next_interaction_deadline().is_none(), "idle");
    // To the row: it mounts, its match marked as the current one.
    ui.find_next();
    ui.prepare_frame();
    assert!(
        (offset.get() - 150_000. * 20.).abs() < 200.,
        "{}",
        offset.get()
    );
    let current = ui
        .theme_scope()
        .color_untracked(zgx::theme::tok::FIND_CURRENT);
    let scene = ui.scene.borrow();
    let row = scene
        .marked_text()
        .find(|id| matches!(scene.kind(*id), NodeKind::Text { text, .. } if &**text == "a needle here"))
        .expect("the row mounted and marked");
    assert_eq!(scene.text_marks(row)[0].color, current);
}

#[test]
fn command_f_opens_the_bar_typing_finds_and_escape_selects_the_match() {
    let mut ui = Ui::new(500., 400.);
    let view = ui.mount(
        column()
            .selectable(true)
            .child(text("first apple"))
            .child(text("second apple"))
            .child(button().id("button").child(text("Go"))),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("button"));
    assert!(key(&mut ui, Key::Character("f".into()), command()));
    assert!(ui.find_status().open);
    let field = ui.focused_editor().expect("the bar's field focused");
    for c in "apple".chars() {
        ui.dispatch(InputEvent::Text(c.to_string()));
    }
    ui.prepare_frame();
    assert_eq!(field.value().get(), "apple");
    assert_eq!(ui.find_status().count, 2);
    assert_eq!(ui.find_status().current, Some(0));
    key(&mut ui, Key::Enter, Modifiers::default());
    assert_eq!(ui.find_status().current, Some(1), "Enter: the next");
    key(
        &mut ui,
        Key::Enter,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ui.find_status().current,
        Some(0),
        "Shift-Enter: the one before"
    );
    key(&mut ui, Key::Character("g".into()), command());
    assert_eq!(ui.find_status().current, Some(1), "Command-G");
    key(&mut ui, Key::Escape, Modifiers::default());
    assert!(!ui.find_status().open);
    assert_eq!(ui.selected_text(), "apple", "the current match selected");
    assert!(marked(&ui).is_empty());
}

#[test]
fn an_app_binding_command_f_keeps_it_and_find_can_be_turned_off() {
    #[derive(Debug)]
    struct Search;
    let mut ui = Ui::new(400., 300.);
    let asked = Rc::new(Cell::new(0));
    let seen = asked.clone();
    let view = ui.mount(
        column()
            .keymap(
                Keymap::new().bind(
                    KeyBinding::new(
                        [Keystroke::new(Key::Character("f".into()), command())],
                        Action::new(Search),
                    )
                    .unwrap(),
                ),
            )
            .on_action(move |_: &Search, cx| {
                seen.set(seen.get() + 1);
                cx.prevent_default();
            })
            .child(text("apple"))
            .child(button().id("button").child(text("Go"))),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("button"));
    key(&mut ui, Key::Character("f".into()), command());
    assert_eq!(asked.get(), 1, "the app's own");
    assert!(!ui.find_status().open, "no bar of zgx's");
    let mut ui = Ui::new(400., 300.);
    ui.mount(column().child(text("apple")));
    ui.prepare_frame();
    ui.set_find_in_page(false);
    assert!(!key(&mut ui, Key::Character("f".into()), command()));
    assert!(!ui.find_status().open);
}

#[test]
fn a_fields_match_leaves_its_selection_until_find_closes_on_it() {
    let mut ui = Ui::new(400., 300.);
    let notes = ui.signal(format!(
        "{}a needle at the end",
        "filler line\n".repeat(200)
    ));
    let view = ui.mount(column().child(text_area("Notes", notes).id("notes").size(300., 80.)));
    ui.prepare_frame();
    let editor = ui.editor(view.find("notes").unwrap()).unwrap();
    editor.set_selection(0, 0);
    ui.prepare_frame();
    ui.find_text("needle");
    ui.prepare_frame();
    assert_eq!(ui.find_status().count, 1);
    assert_eq!(editor.selection(), (0, 0), "found, not selected yet");
    ui.close_find();
    let at = 200 * "filler line\n".len() + 2;
    assert_eq!(editor.selection(), (at, at + 6), "closed on it: selected");
    assert_eq!(ui.input.focused(), view.find("notes"));
}

#[test]
fn use_selection_for_find_looks_for_what_is_selected() {
    let mut ui = Ui::new(400., 300.);
    ui.mount(
        column()
            .selectable(true)
            .child(text("an apple here"))
            .child(text("another apple there")),
    );
    ui.prepare_frame();
    assert!(!ui.find_selection(), "nothing selected");
    let first = {
        let scene = ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(id) = stack.pop() {
            if matches!(scene.kind(id), NodeKind::Text { text, .. } if &**text == "an apple here") {
                found = Some(id);
            }
            stack.extend(scene.children(id).iter().copied());
        }
        found.unwrap()
    };
    let point = |offset| zgx::selection::TextPoint {
        node: first,
        offset,
    };
    assert!(ui.select_text(point(3), point(8)));
    if cfg!(target_os = "macos") {
        // Command-E, as AppKit's.
        key(&mut ui, Key::Character("e".into()), command());
    } else {
        assert!(ui.find_selection());
    }
    assert_eq!(ui.find_status().query, "apple");
    assert_eq!(ui.find_status().count, 2);
}

/// Each match is a shape as a selection is, rounded by `$mark.radius`,
/// which follows `$selection.radius` unless a theme sets it.
#[test]
fn matches_are_shapes_rounded_by_the_mark_radius() {
    let mut ui = Ui::new(500., 400.);
    ui.mount(column().child(text("a cafe, then cafe")));
    ui.prepare_frame();
    ui.find_text("cafe");
    let theme = ui.theme().root().clone();
    assert_eq!(
        theme.number_untracked(zgx::theme::tok::MARK_RADIUS),
        theme.number_untracked(zgx::theme::tok::SELECTION_RADIUS)
    );
    let scene = ui.scene.borrow();
    let node = scene.marked_text().next().expect("a marked text");
    let pieces = scene
        .text_mark_pieces(node, 0., 0., 2.)
        .expect("drawn as shapes");
    assert_eq!(pieces.len(), 2, "two matches, a shape each");
    let radius = theme.number_untracked(zgx::theme::tok::MARK_RADIUS);
    for (piece, _) in pieces {
        assert!(
            matches!(piece, zgx::scene::HighlightPiece::Run { radii, .. } if radii == [radius; 4])
        );
    }
}
