//! A popover opened from a row of another's panel (a nested menu of
//! popovers) closes on a press anywhere outside it, the parent's panel
//! included, and only it: the parent stays, and the press does what it would.
use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::{Placement, PlacementEdge, PlacementSide, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::NodeId,
    widgets::Ui,
};

struct Nested {
    ui: Ui,
    outer: Signal<bool>,
    inner: Signal<bool>,
    toggles: Rc<Cell<u32>>,
    view: zgx::compose::ViewHandle,
}

/// `popups`: as a desktop host shows them, each in a window of its own
/// (`Ui::set_popup_screens`); else in the window, as headless.
fn nested(popups: bool) -> Nested {
    // Popups reach past the window: most of the parent's panel is below it.
    let mut ui = Ui::new(600., if popups { 120. } else { 400. });
    if popups {
        ui.set_popup_screens(vec![zgx::scene::Rect::new(0., 0., 1000., 800.)]);
    }
    let outer = ui.signal(false);
    let inner = ui.signal(false);
    let query = ui.signal(String::new());
    let toggles = Rc::new(Cell::new(0));
    let row = {
        let (inner, toggles) = (inner.clone(), toggles.clone());
        button()
            .id("row")
            .size(160., 24.)
            .p(0.)
            .on_click(move || {
                toggles.set(toggles.get() + 1);
                inner.set(!inner.get());
            })
            .child(text("Work in"))
    };
    let view = ui.mount(
        column().p(20.).child(
            popover(
                "Outer",
                outer.clone(),
                button().id("open").size(60., 20.).p(0.).child(text("Open")),
            )
            .lazy()
            .w(200.)
            .h(200.)
            .p(0.)
            .child(
                popover("Inner", inner.clone(), row)
                    .placement(Placement {
                        side: PlacementSide::Right,
                        align: PlacementEdge::Start,
                        offset: 4.,
                    })
                    .lazy()
                    .w(120.)
                    .h(80.)
                    .p(0.)
                    .child(button().id("local").size(100., 20.).child(text("Local"))),
            )
            .child(text_input("Search", query).id("search").w(160.).h(24.))
            .child(text("blank-ish area")),
        ),
    );
    Nested {
        ui,
        outer,
        inner,
        toggles,
        view,
    }
}

impl Nested {
    fn node(&self, id: &str) -> NodeId {
        self.view.find(id).unwrap()
    }
    fn center(&self, id: &str) -> (f32, f32) {
        let bounds = self.ui.scene.borrow().bounds(self.node(id));
        (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
    }
    fn press(&mut self, (x, y): (f32, f32)) {
        self.ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
        self.ui.prepare_frame();
    }
    fn key(&mut self, key: Key) {
        let modifiers = Modifiers::default();
        self.ui.dispatch(InputEvent::KeyDown {
            key: key.clone(),
            modifiers,
            repeat: false,
        });
        self.ui.dispatch(InputEvent::KeyUp { key, modifiers });
        self.ui.prepare_frame();
    }
    fn open_both(&mut self) {
        self.outer.set(true);
        self.ui.prepare_frame();
        let row = self.center("row");
        self.press(row);
        assert!(self.inner.get(), "the row opens the nested one");
        assert!(self.outer.get());
    }
}

#[test]
fn press_in_parent_panel_closes_only_the_nested_popover() {
    for popups in [false, true] {
        let mut app = nested(popups);
        app.open_both();
        // Blank space in the parent's panel, below its contents.
        let row = app.ui.scene.borrow().bounds(app.node("row"));
        app.press((row.x + 20., row.y + 180.));
        assert!(!app.inner.get(), "a press in the parent closes it");
        assert!(app.outer.get(), "the parent stays open");

        // On the parent's text field: it closes, and the field takes focus.
        let row = app.center("row");
        app.press(row);
        assert!(app.inner.get());
        let search = app.center("search");
        app.press(search);
        assert!(!app.inner.get());
        assert!(app.outer.get());
        let focused = app.ui.input.focused().unwrap();
        let search = app.node("search");
        assert!(
            focused == search
                || app
                    .ui
                    .scene
                    .borrow()
                    .ancestors(focused)
                    .any(|node| node == search),
            "the press still focuses the field"
        );
    }
}

#[test]
fn press_inside_nested_popover_closes_nothing() {
    for popups in [false, true] {
        let mut app = nested(popups);
        app.open_both();
        let local = app.ui.scene.borrow().bounds(app.node("local"));
        // On its control, and on its blank panel space.
        app.press((local.x + 10., local.y + 10.));
        app.press((local.x + 10., local.y + 60.));
        assert!(app.inner.get());
        assert!(app.outer.get());
        // Keys stay its own: Tab keeps to it, its trigger live to the
        // pointer alone; Escape closes it alone.
        let local = app.node("local");
        assert!(app.ui.input.focus(&app.ui.scene, Some(local)));
        app.key(Key::Tab);
        assert_eq!(app.ui.input.focused(), Some(local));
        app.key(Key::Escape);
        assert!(!app.inner.get());
        assert!(app.outer.get());
    }
}

#[test]
fn press_on_nested_anchor_runs_its_own_toggle_once() {
    for popups in [false, true] {
        let mut app = nested(popups);
        app.open_both();
        assert_eq!(app.toggles.get(), 1);
        let row = app.center("row");
        app.press(row);
        assert_eq!(app.toggles.get(), 2, "the row's own click runs");
        assert!(!app.inner.get(), "toggled closed, not reopened");
        assert!(app.outer.get());
        app.press(row);
        assert_eq!(app.toggles.get(), 3);
        assert!(app.inner.get(), "and open again");
    }
}

#[test]
fn press_outside_both_closes_both() {
    for popups in [false, true] {
        let mut app = nested(popups);
        app.open_both();
        app.press((590., 10.));
        assert!(!app.inner.get());
        assert!(!app.outer.get());
    }
}
