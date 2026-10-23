//! The segmented control, the stepper, and disclosures and accordions:
//! pointer, keyboard, what assistive technology is told, and right to left.
use std::rc::Rc;
use zgui::{
    compose::{StepperOptions, ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{Direction, NodeId},
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
fn click(ui: &mut Ui, node: NodeId) {
    let b = ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
}
/// The nodes with `role` under `view`, in tree order.
fn with_role(ui: &Ui, view: &ViewHandle, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![view.node()];
    while let Some(id) = pending.pop() {
        if semantics.get(id).is_some_and(|n| n.role == role) {
            out.push(id);
        }
        pending.extend(scene.children(id).iter().rev().copied());
    }
    out
}
fn checked(ui: &Ui, node: NodeId) -> Option<bool> {
    ui.semantics.borrow().get(node).and_then(|n| n.checked)
}

#[test]
fn a_segmented_control_is_a_radio_group_the_arrows_choose_in() {
    let mut ui = Ui::new(400., 100.);
    let chosen = ui.signal(1usize);
    let view = ui.mount(row().id("row").child(segmented(
        "View",
        chosen.clone(),
        ["Day", "Week", "Month"],
    )));
    ui.prepare_frame();
    let group = with_role(&ui, &view, Role::RadioGroup);
    assert_eq!(group.len(), 1);
    assert_eq!(ui.semantics.borrow().get(group[0]).unwrap().label, "View");
    let radios = with_role(&ui, &view, Role::RadioButton);
    assert_eq!(radios.len(), 3);
    assert_eq!(
        radios.iter().map(|r| checked(&ui, *r)).collect::<Vec<_>>(),
        [Some(false), Some(true), Some(false)]
    );
    assert_eq!(ui.semantics.borrow().get(radios[2]).unwrap().label, "Month");
    click(&mut ui, radios[2]);
    assert_eq!(chosen.get(), 2);
    assert_eq!(checked(&ui, radios[2]), Some(true));
    // One Tab stop, the chosen one; the arrows move the choice.
    ui.input.focus(&ui.scene, None);
    key(&mut ui, Key::Tab);
    assert_eq!(ui.input.focused(), Some(radios[2]));
    key(&mut ui, Key::ArrowLeft);
    assert_eq!(chosen.get(), 1);
    key(&mut ui, Key::Home);
    assert_eq!(chosen.get(), 0);
    key(&mut ui, Key::End);
    assert_eq!(chosen.get(), 2);
    // Right to left, the first is at the right, and Left goes on.
    let root = view.find("row").unwrap();
    ui.scene
        .borrow_mut()
        .set_direction(root, Some(Direction::Rtl));
    ui.prepare_frame();
    let (first, last) = {
        let scene = ui.scene.borrow();
        (scene.bounds(radios[0]).x, scene.bounds(radios[2]).x)
    };
    assert!(first > last, "mirrored: {first} {last}");
    key(&mut ui, Key::Home);
    key(&mut ui, Key::ArrowLeft);
    assert_eq!(chosen.get(), 1);
}

#[test]
fn a_stepper_steps_by_key_and_button_and_takes_what_is_typed() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal(5.);
    let options = StepperOptions {
        min: 0.,
        max: 20.,
        step: 1.,
        decimals: 0,
    };
    let view = ui.mount(row().child(stepper("Copies", value.clone(), options)));
    ui.prepare_frame();
    let field = with_role(&ui, &view, Role::SpinButton)[0];
    {
        let semantics = ui.semantics.borrow();
        let node = semantics.get(field).unwrap();
        assert_eq!(
            (node.min, node.max, node.numeric_value),
            (Some(0.), Some(20.), Some(5.))
        );
    }
    ui.input.focus(&ui.scene, Some(field));
    key(&mut ui, Key::ArrowUp);
    assert_eq!(value.get(), 6.);
    key(&mut ui, Key::PageUp);
    assert_eq!(value.get(), 16.);
    key(&mut ui, Key::PageUp);
    assert_eq!(value.get(), 20., "held at its max");
    key(&mut ui, Key::Home);
    assert_eq!(value.get(), 0.);
    assert_eq!(
        ui.semantics.borrow().get(field).unwrap().numeric_value,
        Some(0.)
    );
    // Typed, it applies on Enter; what isn't a number goes back.
    ui.dispatch(InputEvent::Text("12".into()));
    key(&mut ui, Key::Enter);
    assert_eq!(
        value.get(),
        12.,
        "\"0\" then \"12\" typed after it is 12, at the caret"
    );
    for _ in 0..4 {
        key(&mut ui, Key::Backspace);
    }
    ui.dispatch(InputEvent::Text("lots".into()));
    key(&mut ui, Key::Enter);
    assert_eq!(value.get(), 12.);
    // The buttons step it, and the one at the end is disabled.
    let buttons = with_role(&ui, &view, Role::Button);
    assert_eq!(buttons.len(), 2);
    click(&mut ui, buttons[1]);
    assert_eq!(value.get(), 11.);
    value.set(20.);
    ui.prepare_frame();
    assert!(ui.semantics.borrow().get(buttons[0]).unwrap().disabled);
}

#[test]
fn a_disclosure_shows_its_content_and_an_accordion_one_section() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let section = ui.signal(None::<usize>);
    let sections: Vec<(String, Rc<dyn Fn() -> View>)> = vec![
        ("One".into(), Rc::new(|| text("First body").id("first"))),
        ("Two".into(), Rc::new(|| text("Second body").id("second"))),
    ];
    let view = ui.mount(
        column()
            .child(disclosure(open.clone(), text("Details"), || {
                text("Hidden body").id("body")
            }))
            .child(accordion(section.clone(), sections)),
    );
    ui.prepare_frame();
    let buttons = with_role(&ui, &view, Role::Button);
    assert_eq!(buttons.len(), 3);
    let expanded = |ui: &Ui, b: NodeId| ui.semantics.borrow().get(b).unwrap().expanded;
    assert_eq!(expanded(&ui, buttons[0]), Some(false));
    assert!(view.find("body").is_none());
    click(&mut ui, buttons[0]);
    assert!(open.get());
    assert!(view.find("body").is_some());
    assert_eq!(expanded(&ui, buttons[0]), Some(true));
    // Focused, Space toggles it back.
    key(&mut ui, Key::Space);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    });
    ui.prepare_frame();
    assert!(!open.get());
    // One section at a time.
    click(&mut ui, buttons[1]);
    assert_eq!(section.get(), Some(0));
    assert!(view.find("first").is_some());
    click(&mut ui, buttons[2]);
    assert_eq!(section.get(), Some(1));
    assert!(view.find("first").is_none() && view.find("second").is_some());
    click(&mut ui, buttons[2]);
    assert_eq!(section.get(), None);
}
