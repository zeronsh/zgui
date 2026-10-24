//! Tabs (choosing, the keyboard, closing, reordering, overflow) and the
//! select (its menu, typeahead while closed, what it tells).
use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::{ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    semantics::Role,
    widgets::Ui,
};
use zgx_components::*;

fn key_with(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers,
        repeat: false,
    });
    ui.prepare_frame();
}
fn key(ui: &mut Ui, key: Key) {
    key_with(ui, key, Modifiers::default());
}
fn press(ui: &mut Ui, node: NodeId, button: PointerButton) {
    let b = ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
}
fn click(ui: &mut Ui, node: NodeId) {
    press(ui, node, PointerButton::Primary);
}
fn with_role(ui: &Ui, view: &ViewHandle, role: Role) -> Vec<NodeId> {
    with_role_under(ui, view.node(), role)
}
/// Under `root`; the scene's root reaches overlays too.
fn with_role_under(ui: &Ui, root: NodeId, role: Role) -> Vec<NodeId> {
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
fn labels(ui: &Ui, nodes: &[NodeId]) -> Vec<String> {
    nodes
        .iter()
        .map(|n| ui.semantics.borrow().get(*n).unwrap().label.clone())
        .collect()
}
fn order(items: &zgx::reactive::Signal<Vec<Tab>>) -> Vec<String> {
    items.get().iter().map(|t| t.id.clone()).collect()
}

fn mounted(
    width: f32,
    options: TabsOptions,
) -> (
    Ui,
    ViewHandle,
    zgx::reactive::Signal<Vec<Tab>>,
    zgx::reactive::Signal<String>,
) {
    let mut ui = Ui::new(width, 300.);
    let items = ui.signal(vec![
        Tab::new("a", "Alpha").closable(),
        Tab::new("b", "Beta").closable(),
        Tab::new("c", "Gamma"),
    ]);
    let selected = ui.signal("a".to_owned());
    let view = ui.mount(column().w(width).h(300.).child(tabs(
        "Files",
        items.clone(),
        selected.clone(),
        options,
        |id, _| text(format!("Panel {id}")).id("panel"),
    )));
    ui.prepare_frame();
    ui.prepare_frame();
    (ui, view, items, selected)
}
fn panel(ui: &Ui, view: &ViewHandle) -> String {
    let scene = ui.scene.borrow();
    match scene.kind(view.find("panel").unwrap()) {
        zgx::scene::NodeKind::Text { text, .. } => text.to_string(),
        _ => String::new(),
    }
}

#[test]
fn tabs_choose_their_panel_by_pointer_and_keyboard() {
    let (mut ui, view, _, selected) = mounted(500., TabsOptions::default());
    assert_eq!(with_role(&ui, &view, Role::TabList).len(), 1);
    assert_eq!(with_role(&ui, &view, Role::TabPanel).len(), 1);
    let tabs = with_role(&ui, &view, Role::Tab);
    assert_eq!(labels(&ui, &tabs), ["Alpha", "Beta", "Gamma"]);
    assert_eq!(panel(&ui, &view), "Panel a");
    click(&mut ui, tabs[1]);
    assert_eq!(selected.get(), "b");
    assert_eq!(panel(&ui, &view), "Panel b");
    assert_eq!(
        ui.semantics.borrow().get(tabs[1]).unwrap().selected,
        Some(true)
    );
    // One Tab stop; the arrows choose, as focus moves.
    ui.input.focus(&ui.scene, None);
    key(&mut ui, Key::Tab);
    assert_eq!(
        ui.input.focused(),
        Some(tabs[1]),
        "Tab lands on the chosen one"
    );
    key(&mut ui, Key::ArrowRight);
    assert_eq!(selected.get(), "c");
    key(&mut ui, Key::Home);
    assert_eq!(selected.get(), "a");
    assert_eq!(panel(&ui, &view), "Panel a");
}

#[test]
fn closable_tabs_close_and_the_neighbour_is_chosen() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let log = asked.clone();
    let options = TabsOptions {
        on_close: Some(Rc::new(move |id: &str| {
            log.borrow_mut().push(id.to_owned());
            id != "b"
        })),
        ..Default::default()
    };
    let (mut ui, view, items, selected) = mounted(500., options);
    // Its ×: "Close Alpha".
    let close = with_role(&ui, &view, Role::Button)
        .into_iter()
        .find(|b| ui.semantics.borrow().get(*b).unwrap().label == "Close Alpha")
        .expect("a close button");
    click(&mut ui, close);
    assert_eq!(order(&items), ["b", "c"]);
    assert_eq!(selected.get(), "b", "the chosen one closed: its neighbour");
    // Asked first: "b" stays open.
    let tabs = with_role(&ui, &view, Role::Tab);
    ui.input.focus(&ui.scene, Some(tabs[0]));
    key(&mut ui, Key::Delete);
    assert_eq!(order(&items), ["b", "c"], "vetoed");
    assert_eq!(*asked.borrow(), ["a", "b"]);
    // A middle click on one that doesn't close does nothing.
    press(&mut ui, tabs[1], PointerButton::Middle);
    assert_eq!(order(&items), ["b", "c"]);
}

#[test]
fn reorderable_tabs_move_by_keyboard_and_drag() {
    let options = TabsOptions {
        reorderable: true,
        ..Default::default()
    };
    let (mut ui, view, items, _) = mounted(500., options);
    let tabs = with_role(&ui, &view, Role::Tab);
    ui.input.focus(&ui.scene, Some(tabs[0]));
    let move_keys = Modifiers {
        meta: true,
        shift: true,
        ..Default::default()
    };
    key_with(&mut ui, Key::ArrowRight, move_keys);
    assert_eq!(order(&items), ["b", "a", "c"]);
    key_with(&mut ui, Key::ArrowLeft, move_keys);
    assert_eq!(order(&items), ["a", "b", "c"]);
    // Dragged past Gamma's middle, Alpha goes after it.
    let tabs = with_role(&ui, &view, Role::Tab);
    let (from, past) = {
        let scene = ui.scene.borrow();
        let (a, c) = (scene.bounds(tabs[0]), scene.bounds(tabs[2]));
        ((a.x + 10., a.y + a.height / 2.), c.x + c.width - 4.)
    };
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown {
        x: from.0,
        y: from.1,
        button,
    });
    for step in 1..=6 {
        let x = from.0 + (past - from.0) * step as f32 / 6.;
        ui.dispatch(InputEvent::PointerMove { x, y: from.1 });
        ui.prepare_frame();
    }
    ui.dispatch(InputEvent::PointerUp {
        x: past,
        y: from.1,
        button,
    });
    ui.prepare_frame();
    assert_eq!(order(&items), ["b", "c", "a"]);
}

#[test]
fn an_overflowing_strip_lists_its_tabs_in_a_menu() {
    let (mut ui, view, items, selected) = mounted(500., TabsOptions::default());
    assert!(
        !with_role(&ui, &view, Role::Button).iter().any(|b| ui
            .semantics
            .borrow()
            .get(*b)
            .unwrap()
            .label
            == "All tabs"),
        "they fit: no menu"
    );
    items.set(
        (0..12)
            .map(|i| Tab::new(format!("t{i}"), format!("Document {i}")))
            .collect(),
    );
    selected.set("t0".into());
    ui.prepare_frame();
    ui.prepare_frame();
    let more = with_role(&ui, &view, Role::Button)
        .into_iter()
        .find(|b| ui.semantics.borrow().get(*b).unwrap().label == "All tabs")
        .expect("too many to fit: the menu");
    click(&mut ui, more);
    ui.prepare_frame();
    let root = ui.root();
    let item = with_role_under(&ui, root, Role::MenuItem)
        .into_iter()
        .find(|b| ui.semantics.borrow().get(*b).unwrap().label == "Document 11")
        .expect("every tab in it");
    // Past the window's bottom (the menu scrolls): chosen from the keyboard.
    ui.input.focus(&ui.scene, Some(item));
    key(&mut ui, Key::Enter);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    ui.prepare_frame();
    assert_eq!(selected.get(), "t11");
    // Scrolled into the strip's view.
    let last = *with_role(&ui, &view, Role::Tab).last().unwrap();
    let b = ui.scene.borrow().bounds(last);
    assert!(b.x + b.width <= 500., "in view: {b:?}");
}

#[test]
fn a_select_chooses_from_its_menu_and_by_typing_while_closed() {
    let mut ui = Ui::new(400., 300.);
    let value = ui.signal("b".to_owned());
    let options = vec![
        SelectOption::new("a", "Apple"),
        SelectOption::new("b", "Banana"),
        SelectOption::new("c", "Blueberry").disabled(),
        SelectOption::new("d", "Bread"),
    ];
    let view = ui.mount(
        column()
            .p(20.)
            .child(select("Fruit", value.clone(), options, "Choose…")),
    );
    ui.prepare_frame();
    let trigger = with_role(&ui, &view, Role::ComboBox)[0];
    let read = |ui: &Ui| {
        let semantics = ui.semantics.borrow();
        let node = semantics.get(trigger).unwrap();
        (node.label.clone(), node.value.clone(), node.expanded)
    };
    assert_eq!(
        read(&ui),
        ("Fruit".into(), Some("Banana".into()), Some(false))
    );
    // Opened by a click, the chosen one checked; choosing closes it.
    click(&mut ui, trigger);
    ui.prepare_frame();
    assert_eq!(read(&ui).2, Some(true));
    let apple = view.find("a").expect("its items");
    click(&mut ui, apple);
    assert_eq!(value.get(), "a");
    assert_eq!(
        read(&ui),
        ("Fruit".into(), Some("Apple".into()), Some(false))
    );
    // Closed and focused, typing chooses; "b" again goes on, past the
    // disabled one.
    ui.input.focus(&ui.scene, Some(trigger));
    key(&mut ui, Key::Character("b".into()));
    assert_eq!(value.get(), "b");
    key(&mut ui, Key::Character("b".into()));
    assert_eq!(value.get(), "d", "Blueberry can't be chosen");
    key(&mut ui, Key::ArrowDown);
    assert_eq!(read(&ui).2, Some(true), "Down opens it");
}

#[test]
fn a_bare_button_opens_its_menu() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let view = ui.mount(
        column().p(20.).child(
            menu("M", open.clone(), button().id("b").child(text("Menu")))
                .child(menu_item("One").id("one")),
        ),
    );
    ui.prepare_frame();
    let b = view.find("b").unwrap();
    click(&mut ui, b);
    assert!(open.get());
}
