use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeKind,
    semantics::Role,
    text_layout::FontFamily,
    widgets::Ui,
};

#[test]
fn checkbox_input_model_semantics_and_owned_cleanup() {
    let mut ui = Ui::new(500., 200.);
    let checked = ui.signal(false);
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    let callback_value = checked.clone();
    let mounted = ui.mount(
        checkbox("Enable notifications", checked.clone()).on_click(move || {
            callback_calls.set(callback_calls.get() + 1);
            assert_eq!(callback_value.get(), callback_calls.get() % 2 == 1);
        }),
    );
    let root = mounted.node();
    ui.scene.borrow_mut().flush();
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().role,
        Role::CheckBox
    );
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().label,
        "Enable notifications"
    );
    assert!(ui.input.focus(&ui.scene, Some(root)));
    for key in [Key::Space, Key::Enter] {
        ui.dispatch(InputEvent::KeyDown {
            key: key.clone(),
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.dispatch(InputEvent::KeyUp {
            key,
            modifiers: Modifiers::default(),
        });
    }
    assert_eq!(calls.get(), 2);
    let label = ui.scene.borrow().children(root)[1];
    let bounds = ui.scene.borrow().bounds(label);
    ui.dispatch(InputEvent::PointerDown {
        x: bounds.x + 2.,
        y: bounds.y + 2.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: bounds.x + 2.,
        y: bounds.y + 2.,
        button: PointerButton::Primary,
    });
    assert_eq!(calls.get(), 3);
    assert!(checked.get());
    checked.set(false);
    assert_eq!(
        ui.semantics.borrow().get(root).unwrap().checked,
        Some(false)
    );
    // The indicator is a drawn box, not a glyph, with no mark while off.
    let indicator = ui.scene.borrow().children(root)[0];
    assert!(
        matches!(ui.scene.borrow().kind(indicator), NodeKind::Panel { .. }),
        "{:?}",
        ui.scene.borrow().kind(indicator)
    );
    assert!(drawn_mark(&ui, indicator).is_none());
    checked.set(true);
    ui.prepare_frame();
    assert!(drawn_mark(&ui, indicator).is_some(), "on: a checkmark");
    checked.set(false);
    ui.prepare_frame();
    mounted.unmount();
    assert!(!ui.input.has_listeners(root));
    assert!(ui.semantics.borrow().get(root).is_none());
    checked.set(true);
    assert!(!ui.scene.borrow().contains(indicator));
}

#[test]
fn checkbox_inherits_typography_and_disabled_state_styles() {
    let mut ui = Ui::new(500., 200.);
    let checked = ui.signal(false);
    let disabled = ui.signal(false);
    let read_disabled = disabled.clone();
    let mounted = ui.mount(
        column()
            .font_family(FontFamily::Monospace)
            .text_size(22.)
            .child(
                checkbox("Settings", checked.clone())
                    .id("settings")
                    .focus(|style| style.bg(rgb(0x123456)))
                    .disabled_when(move || read_disabled.get())
                    .disabled_style(|style| style.opacity(0.3)),
            ),
    );
    let root = mounted.find("settings").unwrap();
    // Its label takes the inherited typography.
    let label = ui.scene.borrow().children(root)[1];
    assert_eq!(ui.scene.borrow().font(label).family, FontFamily::Monospace);
    assert!(
        matches!(ui.scene.borrow().kind(label), NodeKind::Text { font_size, .. } if *font_size == 22.)
    );
    assert!(ui.input.focus(&ui.scene, Some(root)));
    assert!(
        matches!(ui.scene.borrow().kind(root), NodeKind::Panel { quad, .. } if quad.fill == rgb(0x123456))
    );
    disabled.set(true);
    assert_eq!(ui.input.focused(), None);
    assert!(ui.semantics.borrow().get(root).unwrap().disabled);
    assert_eq!(ui.scene.borrow().effects(root).opacity, 0.3);
    ui.input.dispatch_to(&ui.scene, root, InputEvent::Activate);
    assert!(!checked.get());
    checked.set(true);
    assert_eq!(ui.semantics.borrow().get(root).unwrap().checked, Some(true));
    disabled.set(false);
    assert!(ui.input.focus(&ui.scene, Some(root)));
    ui.input.dispatch_to(&ui.scene, root, InputEvent::Activate);
    assert!(!checked.get());
}

#[test]
fn checkbox_rejects_extra_children_without_leaking_partial_mounts() {
    let mut ui = Ui::new(500., 200.);
    let checked = ui.signal(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.mount(checkbox("label", checked).child(text("extra")));
    }));
    assert!(result.is_err());
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}

#[test]
fn checkbox_indicator_keeps_its_size_beside_large_typography() {
    let mut ui = Ui::new(900., 300.);
    let checked = ui.signal(true);
    let mounted = ui.mount(column().text_size(64.).child(checkbox("Large", checked)));
    ui.scene.borrow_mut().flush();
    let scene = ui.scene.borrow();
    let control = scene.children(mounted.node())[0];
    let indicator = scene.children(control)[0];
    let label = scene.children(control)[1];
    // The box keeps the platform's size, as a browser's does, centred on
    // the label's line.
    assert_eq!(scene.bounds(indicator).width, 16.);
    assert!(scene.bounds(label).x >= scene.bounds(indicator).x + scene.bounds(indicator).width);
}

/// The checkmark (or dash) drawn in checkbox `indicator`, if any.
fn drawn_mark(ui: &Ui, indicator: zgx::scene::NodeId) -> Option<usize> {
    let scene = ui.scene.borrow();
    let mut stack = vec![indicator];
    while let Some(n) = stack.pop() {
        if let NodeKind::Canvas(canvas) = scene.kind(n)
            && !canvas.commands().is_empty()
        {
            return Some(canvas.commands().len());
        }
        stack.extend(scene.children(n).iter().copied());
    }
    None
}

#[test]
fn a_mixed_checkbox_shows_a_dash_and_tells_assistive_technology() {
    let mut ui = Ui::new(300., 100.);
    let (checked, mixed) = (ui.signal(false), ui.signal(true));
    let mounted = ui.mount(checkbox("Select all", checked.clone()).mixed(mixed.clone()));
    ui.prepare_frame();
    let root = mounted.node();
    let indicator = ui.scene.borrow().children(root)[0];
    assert!(drawn_mark(&ui, indicator).is_some(), "the dash");
    assert!(ui.semantics.borrow().get(root).unwrap().mixed);
    mixed.set(false);
    ui.prepare_frame();
    assert!(drawn_mark(&ui, indicator).is_none(), "off again");
    assert!(!ui.semantics.borrow().get(root).unwrap().mixed);
}

#[test]
fn a_disabled_checkbox_dims_unless_styled_otherwise() {
    let mut ui = Ui::new(300., 100.);
    let checked = ui.signal(true);
    let mounted = ui.mount(checkbox("Locked", checked).disabled(true));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().effects(mounted.node()).opacity, 0.45);
}
