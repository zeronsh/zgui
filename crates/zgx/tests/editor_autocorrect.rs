use std::{cell::RefCell, ops::Range, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::InputEvent,
    platform::{Platform, PlatformService, Substitutions},
    widgets::{EditorHandle, Ui},
};

/// A system that curls quotes, makes "--" a dash, expands "omw" and corrects
/// "teh", as macOS's checker does with those settings on; it counts what it
/// was asked.
#[derive(Default)]
struct System {
    asked: RefCell<Vec<Substitutions>>,
}
impl Platform for System {
    fn clipboard_text(&self) -> Option<String> {
        None
    }
    fn set_clipboard_text(&self, _: String) -> bool {
        false
    }
    fn substitutions(&self, text: &str, kinds: Substitutions) -> Vec<(Range<usize>, String)> {
        self.asked.borrow_mut().push(kinds);
        let mut found = Vec::new();
        if kinds.punctuation {
            let mut open = true;
            for (i, c) in text.char_indices() {
                if c == '"' {
                    found.push((i..i + 1, if open { "“" } else { "”" }.into()));
                }
                if c == '"' || c == '“' || c == '”' {
                    open = !open;
                }
            }
            found.extend(
                text.match_indices("--")
                    .map(|(i, _)| (i..i + 2, "—".into())),
            );
        }
        if kinds.replacements {
            found.extend(
                text.match_indices("omw")
                    .filter(|(i, _)| !text[i + 3..].starts_with(char::is_alphanumeric))
                    .map(|(i, _)| (i..i + 3, "On my way!".into())),
            );
        }
        found
    }
    fn correction(&self, text: &str, word: Range<usize>) -> Option<String> {
        (&text[word] == "teh").then(|| "the".into())
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

/// A text area made by `editor`, focused: its editor and what it holds.
fn field(editor: impl FnOnce(View) -> View) -> (Ui, Field, Rc<System>) {
    let mut ui = Ui::new(400., 200.);
    let system = Rc::new(System::default());
    let value = ui.signal(String::new());
    let view = ui.mount(provide(
        PlatformService(system.clone()),
        column().size(400., 200.).child(editor(
            text_area("Notes", value.clone()).id("field").w(300.).h(80.),
        )),
    ));
    ui.prepare_frame();
    let node = view.find("field").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.editor(node).unwrap();
    (ui, Field(editor, value), system)
}

struct Field(EditorHandle, zgx::reactive::Signal<String>);
impl Field {
    fn text(&self) -> String {
        self.1.get()
    }
    fn selection(&self) -> (usize, usize) {
        self.0.selection()
    }
    fn undo(&self) -> bool {
        self.0.undo()
    }
}

fn type_text(ui: &mut Ui, text: &str) {
    for c in text.chars() {
        ui.dispatch(InputEvent::Text(c.into()));
    }
    ui.prepare_frame();
}

#[test]
fn smart_punctuation_curls_quotes_and_makes_dashes_as_they_are_typed() {
    let (mut ui, editor, _) = field(|v| v.smart_punctuation(true));
    type_text(&mut ui, "He said \"hi\" -- twice");
    assert_eq!(editor.text(), "He said “hi” — twice");
    let end = editor.text().len();
    assert_eq!(editor.selection(), (end, end), "the caret stays at the end");
    // Undo puts back what was typed, and what was put back stays.
    let (mut ui, editor, _) = field(|v| v.smart_punctuation(true));
    type_text(&mut ui, "a \"");
    assert_eq!(editor.text(), "a “");
    editor.undo();
    assert_eq!(editor.text(), "a \"");
    type_text(&mut ui, "b");
    assert_eq!(editor.text(), "a \"b");
}

#[test]
fn autocorrect_corrects_a_word_as_it_ends_and_expands_replacements() {
    let (mut ui, editor, _) = field(|v| v.autocorrect(true));
    type_text(&mut ui, "fix teh");
    assert_eq!(editor.text(), "fix teh", "not while it is typed");
    type_text(&mut ui, " bug, omw");
    assert_eq!(editor.text(), "fix the bug, omw", "nor a replacement");
    type_text(&mut ui, ".");
    assert_eq!(editor.text(), "fix the bug, On my way!.");
    let end = editor.text().len();
    assert_eq!(
        editor.selection(),
        (end, end),
        "the caret after what was typed"
    );
    // Undone, the typed word stays as typed; quotes around a word are not
    // part of it; and "--" stays without smart punctuation.
    editor.undo();
    assert_eq!(editor.text(), "fix the bug, omw.");
    type_text(&mut ui, " 'teh' --x");
    assert_eq!(editor.text(), "fix the bug, omw. 'the' --x");
}

#[test]
fn an_editor_without_them_asks_the_system_nothing() {
    let (mut ui, editor, system) = field(|v| v.spellcheck(true));
    type_text(&mut ui, "teh \"x\"");
    assert_eq!(editor.text(), "teh \"x\"");
    assert!(system.asked.borrow().is_empty());
    // A password field never substitutes.
    let (mut ui, editor, system) =
        field(|v| v.secure(true).autocorrect(true).smart_punctuation(true));
    type_text(&mut ui, "teh \"x\"");
    assert_eq!(editor.text(), "teh \"x\"");
    assert!(system.asked.borrow().is_empty());
}
