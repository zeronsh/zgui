//! The tree view: the keyboard (moving, opening, closing, typeahead,
//! activating), what assistive technology hears, children loaded as an
//! item opens, thousands of rows, right to left, and the pointer.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::{ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    reactive::Signal,
    scene::{Direction, NodeId},
    semantics::Role,
    widgets::Ui,
};
use zgx_components::*;

struct Fixture {
    ui: Ui,
    view: ViewHandle,
    open: Signal<Vec<String>>,
    selected: Signal<Option<String>>,
    activated: Rc<RefCell<Vec<String>>>,
    /// The lazily loaded children of `b`, once loaded.
    loaded: Signal<Option<Vec<TreeItem>>>,
}

fn fixture(big: usize) -> Fixture {
    let mut ui = Ui::new(400., 400.);
    let open = ui.signal(Vec::<String>::new());
    let selected = ui.signal(None::<String>);
    let activated = Rc::new(RefCell::new(Vec::new()));
    let loaded = ui.signal(None::<Vec<TreeItem>>);
    let (log, lazy) = (activated.clone(), loaded.clone());
    let children = move |parent: Option<&str>| match parent {
        None => TreeChildren::Ready(vec![
            TreeItem::parent("a", "Alpha"),
            TreeItem::parent("b", "Beta"),
            TreeItem::leaf("c", "Charlie"),
        ]),
        Some("a") => TreeChildren::Ready(vec![
            TreeItem::leaf("a1", "Apple"),
            TreeItem::parent("a2", "Avocado"),
        ]),
        Some("a2") => TreeChildren::Ready(
            (0..big)
                .map(|i| TreeItem::leaf(format!("x{i}"), format!("Item {i}")))
                .collect(),
        ),
        Some("b") => match lazy.get() {
            Some(items) => TreeChildren::Ready(items),
            None => TreeChildren::Loading,
        },
        Some(_) => TreeChildren::Ready(Vec::new()),
    };
    let view = ui.mount(column().id("root").w(300.).h(400.).child(tree(
        "Files",
        children,
        open.clone(),
        selected.clone(),
        move |id| log.borrow_mut().push(id.to_owned()),
    )));
    ui.prepare_frame();
    ui.prepare_frame();
    Fixture {
        ui,
        view,
        open,
        selected,
        activated,
        loaded,
    }
}
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}
fn with_role(ui: &Ui, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(n) = pending.pop() {
        if semantics.get(n).is_some_and(|s| s.role == role) {
            out.push(n);
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    out
}
fn labels(ui: &Ui) -> Vec<String> {
    with_role(ui, Role::TreeItem)
        .iter()
        .map(|n| ui.semantics.borrow().get(*n).unwrap().label.clone())
        .collect()
}
fn focus_tree(f: &mut Fixture) -> NodeId {
    let tree = with_role(&f.ui, Role::Tree)[0];
    f.ui.input.focus(&f.ui.scene, Some(tree));
    f.ui.prepare_frame();
    tree
}

#[test]
fn the_keyboard_moves_opens_closes_and_activates() {
    let mut f = fixture(3);
    assert_eq!(labels(&f.ui), ["Alpha", "Beta", "Charlie"]);
    let tree = focus_tree(&mut f);
    assert_eq!(
        f.selected.get().as_deref(),
        Some("a"),
        "focused: the first is current"
    );
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(f.open.get(), ["a"], "Right opens it");
    assert_eq!(
        labels(&f.ui),
        ["Alpha", "Apple", "Avocado", "Beta", "Charlie"]
    );
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(f.selected.get().as_deref(), Some("a1"), "then enters it");
    key(&mut f.ui, Key::ArrowLeft);
    assert_eq!(
        f.selected.get().as_deref(),
        Some("a"),
        "Left goes to the parent"
    );
    key(&mut f.ui, Key::ArrowLeft);
    assert!(f.open.get().is_empty(), "then closes it");
    key(&mut f.ui, Key::End);
    assert_eq!(f.selected.get().as_deref(), Some("c"));
    key(&mut f.ui, Key::Home);
    key(&mut f.ui, Key::Character("c".into()));
    assert_eq!(f.selected.get().as_deref(), Some("c"), "typeahead");
    key(&mut f.ui, Key::Enter);
    assert_eq!(*f.activated.borrow(), ["c"]);
    // Focus stays on the tree; the current row is its active descendant.
    assert_eq!(f.ui.input.focused(), Some(tree));
    let charlie = with_role(&f.ui, Role::TreeItem)[2];
    let semantics = f.ui.semantics.borrow();
    assert_eq!(
        semantics.get(tree).unwrap().active_descendant,
        Some(charlie)
    );
    let node = semantics.get(charlie).unwrap();
    assert_eq!(
        (
            node.level,
            node.position_in_set,
            node.size_of_set,
            node.selected
        ),
        (Some(1), Some(3), Some(3), Some(true))
    );
}

#[test]
fn children_load_as_an_item_opens_and_thousands_stay_virtual() {
    let mut f = fixture(2000);
    focus_tree(&mut f);
    key(&mut f.ui, Key::ArrowDown);
    key(&mut f.ui, Key::ArrowRight);
    assert_eq!(f.open.get(), ["b"]);
    assert!(labels(&f.ui).contains(&"Loading…".to_owned()));
    f.loaded.set(Some(vec![TreeItem::leaf("b1", "Banana")]));
    f.ui.prepare_frame();
    assert_eq!(labels(&f.ui), ["Alpha", "Beta", "Banana", "Charlie"]);
    let banana = with_role(&f.ui, Role::TreeItem)[2];
    assert_eq!(f.ui.semantics.borrow().get(banana).unwrap().level, Some(2));
    // Two thousand under Avocado: only what shows is built.
    f.open.set(vec!["a".into(), "a2".into()]);
    f.ui.prepare_frame();
    f.ui.prepare_frame();
    let built = with_role(&f.ui, Role::TreeItem).len();
    assert!(built < 40, "{built} rows built");
    key(&mut f.ui, Key::End);
    f.ui.prepare_frame();
    assert_eq!(f.selected.get().as_deref(), Some("c"));
    assert!(
        labels(&f.ui).contains(&"Charlie".to_owned()),
        "scrolled to it"
    );
}

#[test]
fn right_to_left_left_opens() {
    let mut f = fixture(3);
    let root = f.view.find("root").unwrap();
    f.ui.scene
        .borrow_mut()
        .set_direction(root, Some(Direction::Rtl));
    f.ui.prepare_frame();
    focus_tree(&mut f);
    key(&mut f.ui, Key::ArrowRight);
    assert!(f.open.get().is_empty(), "Right goes back, right to left");
    key(&mut f.ui, Key::ArrowLeft);
    assert_eq!(f.open.get(), ["a"]);
}

#[test]
fn the_chevron_opens_a_click_chooses_and_a_double_click_activates() {
    let mut f = fixture(3);
    let rows = with_role(&f.ui, Role::TreeItem);
    let b = f.ui.scene.borrow().bounds(rows[0]);
    let button = PointerButton::Primary;
    // The chevron, at the row's start.
    let (x, y) = (b.x + 6., b.y + b.height / 2.);
    f.ui.dispatch(InputEvent::PointerDown { x, y, button });
    f.ui.dispatch(InputEvent::PointerUp { x, y, button });
    f.ui.prepare_frame();
    assert_eq!(f.open.get(), ["a"]);
    assert_eq!(f.selected.get(), None, "the chevron chooses nothing");
    let charlie = *with_role(&f.ui, Role::TreeItem).last().unwrap();
    let c = f.ui.scene.borrow().bounds(charlie);
    let (x, y) = (c.x + 60., c.y + c.height / 2.);
    f.ui.dispatch_with_modifiers(
        InputEvent::PointerDown { x, y, button },
        Modifiers::default(),
    );
    f.ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, Modifiers::default());
    assert_eq!(f.selected.get().as_deref(), Some("c"));
    f.ui.dispatch_with_modifiers(
        InputEvent::PointerDown { x, y, button },
        Modifiers::default(),
    );
    f.ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, Modifiers::default());
    assert_eq!(*f.activated.borrow(), ["c"]);
}
