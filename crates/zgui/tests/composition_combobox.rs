//! The combobox (suggestions as typed, the keyboard, choosing, async
//! options) and the command palette (fuzzy matching, running).
use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::{ComboItem, ComboboxOptions, PaletteCommand, ViewHandle, fuzzy_score, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    semantics::Role,
    widgets::Ui,
};

fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}
fn type_text(ui: &mut Ui, text: &str) {
    ui.dispatch(InputEvent::Text(text.into()));
    ui.prepare_frame();
}
fn under(ui: &Ui, root: NodeId, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if semantics.get(id).is_some_and(|n| n.role == role) {
            out.push(id);
        }
        pending.extend(scene.children(id).iter().rev().copied());
    }
    out
}
fn label(ui: &Ui, node: NodeId) -> String {
    let scene = ui.scene.borrow();
    let mut pending = vec![node];
    while let Some(n) = pending.pop() {
        if let zgui::scene::NodeKind::Text { text, .. } = scene.kind(n)
            && !text.is_empty()
        {
            return text.to_string();
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    String::new()
}

const FRUIT: &[&str] = &["Apple", "Apricot", "Banana", "Blackberry", "Cherry"];

/// A combobox filtering FRUIT by what is typed, as an app would.
fn fruit(ui: &mut Ui) -> (ViewHandle, Signal<String>, Signal<Option<String>>) {
    let text_value = ui.signal(String::new());
    let value = ui.signal(None::<String>);
    let items = ui.signal(Vec::<ComboItem>::new());
    let (query, list) = (text_value.clone(), items.clone());
    let filter = ui.runtime.effect(move || {
        let q = query.get().to_lowercase();
        list.set(
            FRUIT
                .iter()
                .filter(|f| f.to_lowercase().starts_with(&q))
                .map(|f| ComboItem::new(f.to_lowercase(), *f))
                .collect(),
        );
    });
    std::mem::forget(filter);
    let view = ui.mount(column().p(20.).w(300.).child(combobox(
        "Fruit",
        text_value.clone(),
        value.clone(),
        items,
        ComboboxOptions::default(),
    )));
    ui.prepare_frame();
    (view, text_value, value)
}
use zgui::reactive::Signal;

#[test]
fn typing_shows_suggestions_the_arrows_move_and_enter_chooses() {
    let mut ui = Ui::new(500., 400.);
    let (view, text_value, value) = fruit(&mut ui);
    let field = under(&ui, view.node(), Role::ComboBox)[0];
    ui.input.focus(&ui.scene, Some(field));
    ui.prepare_frame();
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().expanded,
        Some(false)
    );
    type_text(&mut ui, "a");
    ui.prepare_frame();
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().expanded,
        Some(true)
    );
    let root = ui.root();
    let options = under(&ui, root, Role::ListBoxOption);
    assert_eq!(
        options.iter().map(|o| label(&ui, *o)).collect::<Vec<_>>(),
        ["Apple", "Apricot"]
    );
    // The first highlighted, and the field's active descendant.
    let active = |ui: &Ui| ui.semantics.borrow().get(field).unwrap().active_descendant;
    assert_eq!(active(&ui), Some(options[0]));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(active(&ui), Some(options[1]));
    assert_eq!(ui.input.focused(), Some(field), "focus stays in the field");
    key(&mut ui, Key::Enter);
    assert_eq!(value.get().as_deref(), Some("apricot"));
    assert_eq!(text_value.get(), "Apricot");
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().expanded,
        Some(false)
    );
    // Escape closes the list, then clears the field.
    key(&mut ui, Key::ArrowDown);
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().expanded,
        Some(true)
    );
    key(&mut ui, Key::Escape);
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().expanded,
        Some(false)
    );
    key(&mut ui, Key::Escape);
    assert_eq!(text_value.get(), "");
}

#[test]
fn a_click_chooses_without_taking_focus_from_the_field() {
    let mut ui = Ui::new(500., 400.);
    let (view, text_value, value) = fruit(&mut ui);
    let field = under(&ui, view.node(), Role::ComboBox)[0];
    ui.input.focus(&ui.scene, Some(field));
    type_text(&mut ui, "b");
    ui.prepare_frame();
    let root = ui.root();
    let blackberry = under(&ui, root, Role::ListBoxOption)
        .into_iter()
        .find(|o| label(&ui, *o) == "Blackberry")
        .unwrap();
    let b = ui.scene.borrow().bounds(blackberry);
    let (x, y) = (b.x + 10., b.y + b.height / 2.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
    assert_eq!(value.get().as_deref(), Some("blackberry"));
    assert_eq!(text_value.get(), "Blackberry");
    assert_eq!(ui.input.focused(), Some(field));
}

#[test]
fn while_options_load_the_list_says_so() {
    let mut ui = Ui::new(500., 400.);
    let text_value = ui.signal(String::new());
    let loading = ui.signal(true);
    let items = ui.signal(Vec::<ComboItem>::new());
    let view = ui.mount(column().w(300.).child(combobox(
        "City",
        text_value.clone(),
        ui.signal(None),
        items.clone(),
        ComboboxOptions {
            loading: Some(loading.clone()),
            ..Default::default()
        },
    )));
    ui.prepare_frame();
    let field = under(&ui, view.node(), Role::ComboBox)[0];
    ui.input.focus(&ui.scene, Some(field));
    type_text(&mut ui, "par");
    ui.prepare_frame();
    let texts = |ui: &Ui| {
        let scene = ui.scene.borrow();
        let mut out = Vec::new();
        let mut pending = vec![scene.root()];
        while let Some(n) = pending.pop() {
            if let zgui::scene::NodeKind::Text { text, .. } = scene.kind(n) {
                out.push(text.to_string());
            }
            pending.extend(scene.children(n).iter().copied());
        }
        out
    };
    assert!(texts(&ui).contains(&"Searching…".to_owned()));
    // The fetch answers: thousands, of which only what shows is built.
    items.set(
        (0..5000)
            .map(|i| ComboItem::new(format!("{i}"), format!("Paris {i}")))
            .collect(),
    );
    loading.set(false);
    ui.prepare_frame();
    let root = ui.root();
    let rows = under(&ui, root, Role::ListBoxOption).len();
    assert!(rows > 0 && rows < 40, "virtual: {rows} rows built");
    key(&mut ui, Key::End);
}

#[test]
fn fuzzy_matching_ranks_word_starts_and_runs() {
    assert!(fuzzy_score("", "Anything").is_some());
    assert!(fuzzy_score("ops", "Open Settings").is_some());
    assert!(fuzzy_score("xyz", "Open Settings").is_none());
    // A word's start beats a letter inside one.
    let starts = fuzzy_score("os", "Open Settings").unwrap();
    let inside = fuzzy_score("os", "Close Tabs").unwrap();
    assert!(starts > inside, "{starts} {inside}");
    // A run beats scattered letters.
    let run = fuzzy_score("save", "Save All").unwrap();
    let scattered = fuzzy_score("save", "Stash Away Valid Edits").unwrap();
    assert!(run > scattered, "{run} {scattered}");
}

#[test]
fn the_palette_filters_moves_and_runs_after_closing() {
    let mut ui = Ui::new(800., 600.);
    let open = ui.signal(true);
    let ran = Rc::new(RefCell::new(Vec::<(String, bool)>::new()));
    let (log, was_open) = (ran.clone(), open.clone());
    let commands = || {
        vec![
            PaletteCommand::new("open", "Open File…")
                .detail("File")
                .shortcut("⌘O"),
            PaletteCommand::new("settings", "Open Settings").detail("App"),
            PaletteCommand::new("save", "Save")
                .detail("File")
                .shortcut("⌘S"),
            PaletteCommand::new("theme", "Toggle Theme").detail("View"),
        ]
    };
    let view = ui.mount(
        column().child(command_palette(open.clone(), commands, move |id| {
            log.borrow_mut().push((id.to_owned(), was_open.get()));
        })),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    let root = ui.root();
    let field = under(&ui, root, Role::ComboBox)[0];
    assert_eq!(ui.input.focused(), Some(field), "the field takes focus");
    assert_eq!(under(&ui, root, Role::ListBoxOption).len(), 4);
    type_text(&mut ui, "ost");
    ui.prepare_frame();
    let shown: Vec<String> = under(&ui, root, Role::ListBoxOption)
        .iter()
        .map(|o| {
            let scene = ui.scene.borrow();
            let texts: Vec<String> = scene
                .children(*o)
                .iter()
                .filter_map(|c| match scene.kind(*c) {
                    zgui::scene::NodeKind::Text { text, .. } => Some(text.to_string()),
                    _ => None,
                })
                .collect();
            texts.join(" ")
        })
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("Open Settings"));
    key(&mut ui, Key::Enter);
    assert_eq!(
        *ran.borrow(),
        [("settings".to_owned(), false)],
        "run once closed"
    );
    assert!(!open.get());
    let _ = view;
}
