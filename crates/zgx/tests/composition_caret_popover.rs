//! A popover at an editor's caret (`View::at_caret`), as a mention's
//! suggestions are: placed under the caret and following it as text is
//! typed, a line breaks or the editor scrolls, with focus left in the
//! editor; and the editor telling assistive technology the suggestion it is
//! on (`View::active_descendant`).
use zgx::{compose::prelude::*, input::InputEvent, reactive::Signal, scene::Rect, widgets::Ui};

struct Composer {
    ui: Ui,
    open: Signal<bool>,
    active: Signal<Option<String>>,
    editor: zgx::scene::NodeId,
    panel: zgx::scene::NodeId,
    first: zgx::scene::NodeId,
    second: zgx::scene::NodeId,
}

fn composer() -> Composer {
    let mut ui = Ui::new(500., 400.);
    let value = ui.signal(String::new());
    let open = ui.signal(false);
    let active = ui.signal(None::<String>);
    let reading = active.clone();
    let view = ui.mount(
        column().p(20.).child(
            popover(
                "Mentions",
                open.clone(),
                text_area("Message", value)
                    .id("composer")
                    .size(300., 60.)
                    .active_descendant(move || reading.get()),
            )
            .at_caret()
            .size(120., 60.)
            .p(0.)
            .children([
                div().id("ada").h(20.).child(text("Ada")),
                div().id("grace").h(20.).child(text("Grace")),
            ]),
        ),
    );
    let editor = view.find("composer").unwrap();
    let first = view.find("ada").unwrap();
    let second = view.find("grace").unwrap();
    let panel = ui.scene.borrow().parent(first).unwrap();
    Composer {
        ui,
        open,
        active,
        editor,
        panel,
        first,
        second,
    }
}

impl Composer {
    fn caret(&self) -> Rect {
        self.ui.editor(self.editor).unwrap().input_method_area()
    }
    fn panel(&self) -> Rect {
        self.ui.scene.borrow().transformed_bounds(self.panel)
    }
    /// The panel sits 4 points under the caret, its start at the caret's.
    fn assert_under_caret(&self) {
        let (caret, panel) = (self.caret(), self.panel());
        assert!(
            (panel.x - caret.x).abs() < 0.5
                && (panel.y - (caret.y + caret.height + 4.)).abs() < 0.5,
            "panel {panel:?}, caret {caret:?}"
        );
    }
}

#[test]
fn it_opens_under_the_caret_and_follows_it() {
    let mut c = composer();
    assert!(c.ui.input.focus(&c.ui.scene, Some(c.editor)));
    c.ui.dispatch(InputEvent::Text("Hi @".into()));
    c.open.set(true);
    c.ui.prepare_frame();
    c.assert_under_caret();
    let at = c.panel();
    // Typing moves it along with the caret.
    c.ui.dispatch(InputEvent::Text("gra".into()));
    c.ui.prepare_frame();
    c.assert_under_caret();
    assert!(c.panel().x > at.x, "{:?} then {:?}", at, c.panel());
    // An input method's composition moves the caret, and it with it.
    let before = c.panel();
    c.ui.dispatch(InputEvent::ImePreedit {
        text: "にほん".into(),
        cursor: Some((9, 9)),
    });
    c.ui.prepare_frame();
    assert!(c.panel().x > before.x, "{:?} then {:?}", before, c.panel());
    c.ui.dispatch(InputEvent::ImeCommit("日本".into()));
    c.ui.prepare_frame();
    c.assert_under_caret();
    // A new line takes it down.
    c.ui.dispatch(InputEvent::Text("\n@".into()));
    c.ui.prepare_frame();
    c.assert_under_caret();
    assert!(c.panel().y > at.y);
}

#[test]
fn it_follows_the_caret_as_the_editor_scrolls() {
    let mut c = composer();
    assert!(c.ui.input.focus(&c.ui.scene, Some(c.editor)));
    c.open.set(true);
    // Lines past the editor's 60 points: it scrolls to keep the caret shown.
    for n in 0..8 {
        c.ui.dispatch(InputEvent::Text(format!("line {n}\n")));
        c.ui.prepare_frame();
    }
    c.ui.dispatch(InputEvent::Text("@".into()));
    c.ui.prepare_frame();
    c.assert_under_caret();
    let editor = c.ui.scene.borrow().transformed_bounds(c.editor);
    let caret = c.caret();
    assert!(
        caret.y < editor.y + editor.height,
        "the caret shows in the scrolled editor: {caret:?} in {editor:?}"
    );
}

#[test]
fn focus_stays_in_the_editor() {
    let mut c = composer();
    assert!(c.ui.input.focus(&c.ui.scene, Some(c.editor)));
    c.open.set(true);
    c.ui.prepare_frame();
    assert_eq!(c.ui.input.focused(), Some(c.editor));
    // Keys still type into it.
    c.ui.dispatch(InputEvent::Text("@a".into()));
    c.ui.prepare_frame();
    assert_eq!(c.ui.editor(c.editor).unwrap().editor.borrow().text(), "@a");
    c.open.set(false);
    c.ui.prepare_frame();
    assert_eq!(c.ui.input.focused(), Some(c.editor));
}

#[test]
fn the_editor_tells_which_suggestion_it_is_on() {
    let c = composer();
    let active = |c: &Composer| {
        c.ui.semantics
            .borrow()
            .get(c.editor)
            .unwrap()
            .active_descendant
    };
    assert_eq!(active(&c), None);
    // Opening and choosing in one go: the items are found once laid out.
    c.open.set(true);
    c.active.set(Some("ada".into()));
    c.ui.prepare_frame();
    c.ui.prepare_frame();
    assert_eq!(active(&c), Some(c.first));
    c.active.set(Some("grace".into()));
    c.ui.prepare_frame();
    assert_eq!(active(&c), Some(c.second));
    c.active.set(None);
    c.ui.prepare_frame();
    assert_eq!(active(&c), None);
}
