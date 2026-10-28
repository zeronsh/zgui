use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::{ViewHandle, prelude::*},
    input::{InputEvent, PointerButton},
    widgets::Ui,
};

#[derive(Default)]
struct Clipboard(RefCell<Option<String>>);
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

fn mount(ui: &mut Ui, editor: View) -> (ViewHandle, Rc<Clipboard>) {
    let clipboard = Rc::new(Clipboard::default());
    let view = ui.mount(provide(
        zgx::platform::PlatformService(clipboard.clone()),
        column()
            .size(400., 200.)
            .child(editor.id("field").w(300.).h(32.)),
    ));
    ui.prepare_frame();
    (view, clipboard)
}

fn context_click(ui: &mut Ui, x: f32, y: f32) {
    let button = PointerButton::Secondary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
}

fn choose(ui: &mut Ui, view: &ViewHandle, id: &str) {
    let item = view.find(id).unwrap_or_else(|| panic!("{id} shows"));
    ui.input.dispatch_to(&ui.scene, item, InputEvent::Activate);
    ui.prepare_frame();
}

fn enabled(ui: &Ui, view: &ViewHandle, id: &str) -> bool {
    let item = view.find(id).unwrap();
    ui.input.is_enabled(&ui.scene.borrow(), item)
}

#[test]
fn an_editors_context_menu_cuts_copies_pastes_and_selects_all() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("Hello world".to_owned());
    let (view, clipboard) = mount(&mut ui, text_input("Name", value.clone()));
    let editor = ui.editor(view.find("field").unwrap()).unwrap();
    editor.set_selection(6, 11);
    let field = ui.scene.borrow().bounds(view.find("field").unwrap());
    let inside = (field.x + 4., field.y + field.height / 2.);
    // A press on the selection keeps it.
    editor.set_selection(0, 11);
    context_click(&mut ui, inside.0, inside.1);
    assert!(!enabled(&ui, &view, "editor-paste"), "nothing to paste");
    choose(&mut ui, &view, "editor-copy");
    assert_eq!(clipboard.0.borrow().as_deref(), Some("Hello world"));
    context_click(&mut ui, inside.0, inside.1);
    choose(&mut ui, &view, "editor-cut");
    assert_eq!(value.get(), "", "cut");
    context_click(&mut ui, inside.0, inside.1);
    assert!(
        !enabled(&ui, &view, "editor-copy"),
        "nothing selected to copy"
    );
    assert!(!enabled(&ui, &view, "editor-cut"));
    choose(&mut ui, &view, "editor-paste");
    assert_eq!(value.get(), "Hello world", "pasted back");
    context_click(&mut ui, inside.0, inside.1);
    choose(&mut ui, &view, "editor-select-all");
    assert_eq!(editor.selection(), (0, 11));
}

#[test]
fn a_context_click_outside_the_selection_moves_the_caret_there() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("Hello world".to_owned());
    let (view, _) = mount(&mut ui, text_input("Name", value));
    let node = view.find("field").unwrap();
    let editor = ui.editor(node).unwrap();
    editor.set_selection(0, 0);
    let field = ui.scene.borrow().bounds(node);
    // Well right of the text's end: the caret goes to the end.
    context_click(
        &mut ui,
        field.x + field.width - 4.,
        field.y + field.height / 2.,
    );
    let (anchor, focus) = editor.selection();
    assert_eq!(focus, 11, "{anchor}..{focus}");
    // The menu has the keys while open; closed, the editor has them.
    ui.dispatch(InputEvent::KeyDown {
        key: zgx::input::Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    });
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), Some(node), "focus comes back to it");
}

#[test]
fn read_only_and_password_fields_offer_only_what_they_allow() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("secret".to_owned());
    let (view, clipboard) = mount(&mut ui, text_input("Password", value).secure(true));
    *clipboard.0.borrow_mut() = Some("pasted".into());
    let editor = ui.editor(view.find("field").unwrap()).unwrap();
    editor.select_all();
    let field = ui.scene.borrow().bounds(view.find("field").unwrap());
    context_click(&mut ui, field.x + 4., field.y + field.height / 2.);
    assert!(
        !enabled(&ui, &view, "editor-copy"),
        "a password never leaves"
    );
    assert!(!enabled(&ui, &view, "editor-cut"));
    assert!(enabled(&ui, &view, "editor-paste"));

    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("fixed".to_owned());
    let (view, clipboard) = mount(&mut ui, text_input("Fixed", value).read_only(true));
    *clipboard.0.borrow_mut() = Some("pasted".into());
    let editor = ui.editor(view.find("field").unwrap()).unwrap();
    editor.select_all();
    let field = ui.scene.borrow().bounds(view.find("field").unwrap());
    context_click(&mut ui, field.x + 4., field.y + field.height / 2.);
    assert!(enabled(&ui, &view, "editor-copy"));
    assert!(!enabled(&ui, &view, "editor-cut"), "read-only");
    assert!(!enabled(&ui, &view, "editor-paste"));
}

#[test]
fn a_secondary_press_during_a_drag_neither_moves_the_selection_nor_opens_a_menu() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("Hello wide world".to_owned());
    let (view, _) = mount(&mut ui, text_input("Name", value));
    let node = view.find("field").unwrap();
    let editor = ui.editor(node).unwrap();
    let field = ui.scene.borrow().bounds(node);
    let y = field.y + field.height / 2.;
    let primary = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown {
        x: field.x + 10.,
        y,
        button: primary,
    });
    ui.dispatch(InputEvent::PointerMove {
        x: field.x + 60.,
        y,
    });
    ui.prepare_frame();
    let before = editor.selection();
    context_click(&mut ui, field.x + 60., y);
    assert!(view.find("editor-copy").is_none(), "no menu mid-drag");
    ui.dispatch(InputEvent::PointerMove {
        x: field.x + 120.,
        y,
    });
    ui.prepare_frame();
    let after = editor.selection();
    assert_eq!(after.0, before.0, "the drag keeps its anchor");
    assert!(after.1 > before.1, "and goes on: {before:?} → {after:?}");
}

/// A checker that knows one misspelling, "teh", and guesses "the".
#[derive(Default)]
struct Speller {
    clipboard: Clipboard,
    learned: RefCell<Vec<String>>,
}
impl zgx::platform::Platform for Speller {
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.clipboard_text()
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        self.clipboard.set_clipboard_text(text)
    }
    fn misspellings(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        text.match_indices("teh")
            .filter(|_| !self.learned.borrow().iter().any(|w| w == "teh"))
            .map(|(i, w)| i..i + w.len())
            .collect()
    }
    fn spelling_guesses(&self, word: &str) -> Vec<String> {
        if word == "teh" {
            vec!["the".into(), "ten".into()]
        } else {
            Vec::new()
        }
    }
    fn learn_spelling(&self, word: &str) {
        self.learned.borrow_mut().push(word.into());
    }
    fn open_files(
        &self,
        picker: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
        self.clipboard.open_files(picker)
    }
    fn save_file(
        &self,
        picker: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
        self.clipboard.save_file(picker)
    }
}

/// The dotted marks under misspelled words: where each is.
fn marks(ui: &Ui) -> Vec<zgx::scene::Rect> {
    let scene = ui.scene.borrow();
    scene
        .paint_items()
        .filter(|item| {
            matches!(scene.kind(item.id), zgx::scene::NodeKind::Quad(quad)
            if quad.decoration.as_ref().is_some_and(|d| matches!(
                d.border_style,
                zgx::decoration::BorderStyle::Dashed { .. }
            )))
        })
        .map(|item| scene.bounds(item.id))
        .collect()
}

#[test]
fn a_spellchecked_editor_marks_misspellings_and_its_menu_corrects_them() {
    let mut ui = Ui::new(400., 200.);
    let speller = Rc::new(Speller::default());
    let value = ui.signal("fix teh bug".to_owned());
    let view = ui.mount(provide(
        zgx::platform::PlatformService(speller.clone()),
        column().size(400., 200.).child(
            text_area("Notes", value.clone())
                .id("field")
                .spellcheck(true)
                .w(300.)
                .h(80.),
        ),
    ));
    ui.prepare_frame();
    assert_eq!(marks(&ui).len(), 1, "teh is marked");
    let node = view.find("field").unwrap();
    let editor = ui.editor(node).unwrap();
    // In a focused editor, the word being typed is not marked until the
    // caret leaves it.
    ui.input.focus(&ui.scene, Some(node));
    editor.set_selection(7, 7);
    ui.prepare_frame();
    assert!(marks(&ui).is_empty());
    editor.set_selection(0, 0);
    ui.prepare_frame();
    assert_eq!(marks(&ui).len(), 1);
    // A context click on it: the guesses, then the editing items.
    let on = |ui: &Ui| {
        let mark = marks(ui)[0];
        (mark.x + mark.width / 2., mark.y - 6.)
    };
    let (x, y) = on(&ui);
    context_click(&mut ui, x, y);
    assert!(view.find("spelling-learn").is_some());
    choose(&mut ui, &view, "spelling-guess-0");
    assert_eq!(value.get(), "fix the bug", "corrected");
    assert!(marks(&ui).is_empty(), "and no longer marked");
    // Learn Spelling: it stays unmarked from then on.
    editor.set_selection(11, 11);
    editor.paste(" teh");
    ui.prepare_frame();
    assert!(marks(&ui).is_empty(), "still being typed");
    editor.set_selection(0, 0);
    ui.prepare_frame();
    assert_eq!(marks(&ui).len(), 1, "left, it is marked");
    let (x, y) = on(&ui);
    context_click(&mut ui, x, y);
    choose(&mut ui, &view, "spelling-learn");
    assert!(marks(&ui).is_empty());
    assert_eq!(*speller.learned.borrow(), ["teh"]);
}

#[test]
fn turning_spellcheck_off_clears_the_marks_and_on_checks_again() {
    let mut ui = Ui::new(400., 200.);
    let speller = Rc::new(Speller::default());
    let value = ui.signal("fix teh bug".to_owned());
    let on = ui.signal(true);
    let read = on.clone();
    ui.mount(provide(
        zgx::platform::PlatformService(speller),
        column().size(400., 200.).child(
            text_area("Notes", value.clone())
                .spellcheck_when(move || read.get())
                .w(300.)
                .h(80.),
        ),
    ));
    ui.prepare_frame();
    assert_eq!(marks(&ui).len(), 1);
    on.set(false);
    ui.prepare_frame();
    assert!(marks(&ui).is_empty(), "off: no marks");
    value.set("teh fix teh bug".to_owned());
    ui.prepare_frame();
    assert!(marks(&ui).is_empty(), "and nothing checked");
    on.set(true);
    ui.prepare_frame();
    assert_eq!(marks(&ui).len(), 2, "on: the whole text checked");
}

/// A platform that looks words up (macOS's dictionary), recording what it
/// was asked.
#[derive(Default)]
struct Dictionary(RefCell<Vec<LookedUp>>);
/// What was looked up: the text, its baseline's start, its size.
type LookedUp = (String, (f32, f32), f32);
impl zgx::platform::Platform for Dictionary {
    fn clipboard_text(&self) -> Option<String> {
        None
    }
    fn set_clipboard_text(&self, _: String) -> bool {
        true
    }
    fn looks_up(&self) -> bool {
        true
    }
    fn look_up(&self, text: &str, baseline: (f32, f32), size: f32) -> bool {
        self.0.borrow_mut().push((text.to_owned(), baseline, size));
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

/// Where the platform looks words up, an editor's menu, and a selectable
/// region's, offer Look Up first, as AppKit's do: the selection's text at
/// its first character's baseline. A password offers none; a platform that
/// looks nothing up, none.
#[test]
fn context_menus_look_the_selection_up_where_the_platform_does() {
    let mut ui = Ui::new(400., 200.);
    let dictionary = Rc::new(Dictionary::default());
    let value = ui.signal("alpha serendipity".to_owned());
    let view = ui.mount(provide(
        zgx::platform::PlatformService(dictionary.clone()),
        column()
            .size(400., 200.)
            .child(
                text_input("Name", value)
                    .id("field")
                    .w(300.)
                    .h(32.)
                    .text_size(14.),
            )
            .child(
                column()
                    .selectable(true)
                    .child(text("Look ephemeral up").text_size(14.)),
            ),
    ));
    ui.prepare_frame();
    let field = view.find("field").unwrap();
    let editor = ui.editor(field).unwrap();
    editor.set_selection(6, 17);
    let at = ui.scene.borrow().bounds(field);
    context_click(&mut ui, at.x + 60., at.y + at.height / 2.);
    let item = view.find("editor-look-up").expect("Look Up shows");
    let first = ui
        .scene
        .borrow()
        .children(ui.scene.borrow().parent(item).unwrap())[0];
    assert_eq!(first, item, "first in the menu");
    let label = ui
        .semantics
        .borrow()
        .get(item)
        .map(|n| n.label.clone())
        .unwrap_or_default();
    assert_eq!(label, "Look Up “serendipity”");
    choose(&mut ui, &view, "editor-look-up");
    let asked = dictionary.0.borrow().clone();
    assert_eq!(asked.len(), 1);
    let (text, (x, y), size) = &asked[0];
    assert_eq!(text, "serendipity");
    assert_eq!(*size, 14.);
    assert!(
        *x > at.x + 6. && *x < at.x + at.width,
        "at the word: {x} in {at:?}"
    );
    assert!(
        *y > at.y && *y < at.y + at.height,
        "on its line: {y} in {at:?}"
    );
    // Selected display text: the same, from its menu.
    let label = {
        let scene = ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(node) = stack.pop() {
            if matches!(scene.kind(node), zgx::scene::NodeKind::Text { text, .. } if &**text == "Look ephemeral up")
            {
                found = Some(node);
            }
            stack.extend(scene.children(node).iter().copied());
        }
        found.unwrap()
    };
    assert!(ui.select_text(
        zgx::selection::TextPoint {
            node: label,
            offset: 5
        },
        zgx::selection::TextPoint {
            node: label,
            offset: 14
        },
    ));
    ui.prepare_frame();
    let shown = ui.scene.borrow().bounds(label);
    context_click(&mut ui, shown.x + 60., shown.y + shown.height / 2.);
    choose(&mut ui, &view, "selection-look-up");
    let asked = dictionary.0.borrow().clone();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1].0, "ephemeral");
    assert!(
        asked[1].1.0 > shown.x && asked[1].1.1 > shown.y,
        "{:?} in {shown:?}",
        asked[1]
    );
}

/// Large type in a short field (its line taller than the box inside its
/// padding): a misspelling's marks stay inside the field, where its clip
/// shows them.
#[test]
fn a_line_taller_than_its_field_keeps_its_misspelling_marks() {
    let mut ui = Ui::new(640., 200.);
    let speller = Rc::new(Speller::default());
    let value = ui.signal("fix teh bug".to_owned());
    let view = ui.mount(provide(
        zgx::platform::PlatformService(speller.clone()),
        column().size(640., 200.).child(
            text_input("Name", value.clone())
                .id("field")
                .spellcheck(true)
                .size(560., 44.)
                .p(10.)
                .text_size(22.),
        ),
    ));
    ui.prepare_frame();
    let field = ui.scene.borrow().bounds(view.find("field").unwrap());
    let marks = marks(&ui);
    assert_eq!(marks.len(), 1, "teh is marked");
    assert!(marks[0].y + marks[0].height <= field.y + field.height - 10. + 0.01);
}
