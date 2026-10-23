//! The component library from scripts: segmented controls, steppers,
//! disclosures and accordions, separators, and what menu items, sliders,
//! progress bars, overlays and plain nodes tell assistive technology.
mod common;

use common::Harness;
use zgui::{
    input::{Key, Modifiers},
    scene::NodeId,
    semantics::{Role, SemanticNode},
};

fn node(h: &mut Harness, id: &str) -> SemanticNode {
    let n = h.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
    h.ui.semantics
        .borrow()
        .get(n)
        .cloned()
        .unwrap_or_else(|| panic!("no semantics on `{id}`"))
}
/// The nodes with `role`, in tree order.
fn with_role(h: &mut Harness, role: Role) -> Vec<NodeId> {
    h.ui.prepare_frame();
    let scene = h.ui.scene.borrow();
    let semantics = h.ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(id) = pending.pop() {
        if semantics.get(id).is_some_and(|n| n.role == role) {
            out.push(id);
        }
        pending.extend(scene.children(id).iter().rev().copied());
    }
    out
}
fn click_node(h: &mut Harness, node: NodeId) {
    let b = h.ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    let button = zgui::input::PointerButton::Primary;
    h.ui.dispatch(zgui::input::InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(zgui::input::InputEvent::PointerUp { x, y, button });
    h.settle();
}

#[test]
fn a_segmented_control_chooses_values_and_a_stepper_steps() {
    let mut h = Harness::inline(
        r#"local view = ui.state("view", "week")
        local copies = ui.state("copies", 2)
        return ui.column {
            ui.segmented { label = "Range", value = view, options = { { label = "Day", value = "day" }, { label = "Week", value = "week" }, "Month" } },
            ui.text { id = "view", "{view}" },
            ui.stepper { id = "copies", label = "Copies", value = copies, min = 1, max = 5 },
            ui.text { id = "copies-shown", "{copies}" },
        }"#,
    );
    assert_eq!(h.error(), None);
    let radios = with_role(&mut h, Role::RadioButton);
    assert_eq!(radios.len(), 3);
    let checked: Vec<_> = radios
        .iter()
        .map(|r| h.ui.semantics.borrow().get(*r).unwrap().checked)
        .collect();
    assert_eq!(checked, [Some(false), Some(true), Some(false)]);
    click_node(&mut h, radios[2]);
    assert_eq!(h.text("view"), "Month", "a bare option is its own value");
    click_node(&mut h, radios[0]);
    assert_eq!(h.text("view"), "day");
    let field = with_role(&mut h, Role::SpinButton)[0];
    h.ui.input.focus(&h.ui.scene, Some(field));
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    assert_eq!(h.text("copies-shown"), "4");
    h.key(Key::End);
    assert_eq!(h.text("copies-shown"), "5");
    assert_eq!(h.error(), None);
}

#[test]
fn disclosures_and_accordions_show_their_content() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        local section = ui.state("section", nil :: string?)
        return ui.column {
            ui.disclosure { open = open, summary = "Advanced", ui.text { id = "advanced", "Options" } },
            ui.accordion {
                open = section,
                ui.section { id = "ship", title = "Shipping", ui.text { id = "ship-body", "By sea" } },
                ui.section { id = "pay", title = "Payment", ui.text { id = "pay-body", "By card" } },
            },
            ui.text { id = "section", function() return tostring(section:get()) end },
            ui.separator {},
        }"#,
    );
    assert_eq!(h.error(), None);
    assert!(h.find("advanced").is_none());
    let buttons = with_role(&mut h, Role::Button);
    assert_eq!(buttons.len(), 3);
    click_node(&mut h, buttons[0]);
    assert!(h.find("advanced").is_some());
    assert_eq!(
        h.ui.semantics.borrow().get(buttons[0]).unwrap().expanded,
        Some(true)
    );
    click_node(&mut h, buttons[2]);
    assert_eq!(h.text("section"), "pay");
    assert!(h.find("pay-body").is_some() && h.find("ship-body").is_none());
    click_node(&mut h, buttons[1]);
    assert_eq!(h.text("section"), "ship");
    assert!(h.find("pay-body").is_none());
}

#[test]
fn menu_items_check_and_show_shortcuts_and_nodes_say_their_state() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", true)
        local wrap = ui.state("wrap", true)
        local on = ui.state("on", false)
        local anchor = ui.button { id = "anchor", "View" }
        return ui.column {
            anchor,
            ui.menu {
                open = open,
                anchor = anchor,
                match_anchor_width = true,
                ui.menu_item { id = "wrap", label = "Word Wrap", checked = wrap, shortcut = "⌥Z", keep_open = true, on_click = ui.toggle(wrap) },
                ui.menu_item { id = "small", label = "Small", radio = true, checked = true },
            },
            ui.div { id = "switch", role = "switch", checked = on, focusable = true, on_click = ui.toggle(on), w = 30, h = 16 },
            ui.slider { id = "volume", label = "Volume", value = ui.state("v", 0.5), step = 0.25 },
            ui.progress { id = "busy", label = "Syncing", value = ui.state("p", nil :: number?) },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let wrap = node(&mut h, "wrap");
    assert_eq!(wrap.checked, Some(true));
    assert_eq!(wrap.shortcut.as_deref(), Some("⌥Z"));
    assert!(!wrap.radio);
    assert!(h.texts().contains(&"⌥Z".to_owned()), "the shortcut shows");
    assert!(h.texts().contains(&"✓".to_owned()), "and the check");
    let small = node(&mut h, "small");
    assert!(small.radio && small.checked == Some(true));
    h.click("wrap");
    assert_eq!(
        node(&mut h, "wrap").checked,
        Some(false),
        "it follows its state"
    );
    h.key(Key::Escape);
    let switch = node(&mut h, "switch");
    assert_eq!((switch.role, switch.checked), (Role::Switch, Some(false)));
    h.click("switch");
    assert_eq!(node(&mut h, "switch").checked, Some(true));
    let volume = h.find("volume").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(volume));
    h.key_with(Key::ArrowRight, Modifiers::default());
    assert_eq!(node(&mut h, "volume").numeric_value, Some(0.75), "a step");
    assert_eq!(
        node(&mut h, "busy").numeric_value,
        None,
        "nil: indeterminate"
    );
}

#[test]
fn a_menus_button_opens_it() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        local anchor = ui.button { id = "anchor", "View" }
        return ui.column {
            ui.menu { open = open, anchor = anchor, ui.menu_item { id = "item", "Item" } },
            ui.text { id = "state", function() return tostring(open:get()) end },
        }"#,
    );
    h.click("anchor");
    h.frames(2);
    assert_eq!(h.text("state"), "true");
    assert!(h.find("item").is_some());
    // From the keyboard: Down (or Enter, or Space) on the button.
    h.key(Key::Escape);
    assert_eq!(h.text("state"), "false");
    let anchor = h.find("anchor").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(anchor));
    h.key(Key::ArrowDown);
    assert_eq!(h.text("state"), "true");
}
