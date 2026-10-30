//! An accessibility audit of a mounted interface, without a screen reader or
//! a window: the tree a reader would get (`AccessibilityTree::update`, all of
//! it, labels made from state included) checked for what a reader could not
//! use, and Tab walked through it as a person would.
//!
//! [`audit`] returns a [`Report`]; [`Report::to_json`] is its compact form,
//! for agents and CI. Each finding names a rule:
//!
//! - `no-role`: something takes focus, but is nothing a reader can name the
//!   kind of (no accessible node, or a container or text);
//! - `no-name`: a control, image or dialog with no name;
//! - `no-action`: a control a reader cannot work (press, focus, set);
//! - `no-state`: a check box, switch, radio button, tab or option that does
//!   not say whether it is checked or chosen, a slider or spin button with no
//!   value, or a slider or progress bar with a value and no range (a
//!   progress bar with no value is indeterminate);
//! - `focus-unreachable`: Tab never reaches something that takes focus;
//! - `focus-trap`: Tab stops coming round, held on one control or a few;
//! - `not-announced`: an alert or status that is not live, or an invalid
//!   field whose error is not given to a reader.
//!
//! It turns assistive technology on for `ui` (`Ui::set_assistive_technology`)
//! and leaves it on; nothing of it runs in an app a reader is not reading.
use super::AccessibilityTree;
use accesskit::{Action, Live, Role as Native};
use std::collections::{HashMap, HashSet};
use zgx::{
    input::{InputEvent, Key, Modifiers},
    scene::NodeId,
    widgets::Ui,
};

/// What [`audit`] found of one interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The accessible nodes the tree has.
    pub nodes: usize,
    /// The scene's nodes that take focus by Tab.
    pub focusable: usize,
    /// The stops Tab made, in order, as long as it came round.
    pub tab_stops: usize,
    /// What a reader could not use, in tree order, then focus findings.
    pub findings: Vec<Finding>,
}

/// One thing a reader could not use.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The rule it breaks (see the module's list).
    pub rule: &'static str,
    /// The scene node.
    pub node: Option<NodeId>,
    /// Its accessible role, as AccessKit names it (`"none"` without a node).
    pub role: String,
    /// Its accessible name, or its text when it has none.
    pub name: String,
    /// What is missing.
    pub detail: String,
}

impl Report {
    /// The findings of `rule`.
    pub fn of(&self, rule: &str) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.rule == rule)
    }
    /// One line of JSON: the counts, then each finding as
    /// `{"rule","node","role","name","detail"}` (`node` the scene node's
    /// index, or null).
    pub fn to_json(&self) -> String {
        let findings: Vec<String> = self
            .findings
            .iter()
            .map(|f| {
                format!(
                    "{{\"rule\":{},\"node\":{},\"role\":{},\"name\":{},\"detail\":{}}}",
                    quote(f.rule),
                    f.node.map_or("null".to_owned(), |n| n.number().to_string()),
                    quote(&f.role),
                    quote(&f.name),
                    quote(&f.detail),
                )
            })
            .collect();
        format!(
            "{{\"nodes\":{},\"focusable\":{},\"tab_stops\":{},\"findings\":[{}]}}",
            self.nodes,
            self.focusable,
            self.tab_stops,
            findings.join(",")
        )
    }
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Controls: what a person works, which a reader must name and work.
fn control(role: Native) -> bool {
    use Native::*;
    matches!(
        role,
        Button
            | Link
            | CheckBox
            | Switch
            | RadioButton
            | Slider
            | SpinButton
            | ComboBox
            | TextInput
            | MultilineTextInput
            | Tab
            | MenuItem
            | MenuItemCheckBox
            | MenuItemRadio
            | ListBoxOption
            | TreeItem
            | Splitter
            | ScrollBar
    )
}
/// Members a focus group makes one Tab stop of (arrows move among them).
fn grouped(role: Native) -> bool {
    use Native::*;
    matches!(
        role,
        RadioButton
            | Tab
            | MenuItem
            | MenuItemCheckBox
            | MenuItemRadio
            | ListBoxOption
            | ListItem
            | TreeItem
            | Row
            | Cell
            | ColumnHeader
            // Scrollbars take focus for a reader, not by Tab, as a
            // browser's.
            | ScrollBar
    )
}

/// Containers whose controls the arrows move among (one Tab stop).
fn arrows_within(role: Native) -> bool {
    use Native::*;
    matches!(
        role,
        Toolbar | RadioGroup | TabList | ListBox | Menu | MenuBar | Tree | TreeGrid | Grid | List
    )
}

/// Audit what `ui` shows now (see the module).
pub fn audit(ui: &mut Ui) -> Report {
    ui.set_assistive_technology(true);
    ui.prepare_frame();
    let mut tree = AccessibilityTree::new();
    let update = tree.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Audit",
        1.,
    );
    let scene_of: HashMap<accesskit::NodeId, NodeId> =
        tree.ids.iter().map(|(scene, id)| (*id, *scene)).collect();
    let mut report = Report {
        nodes: update.nodes.len(),
        ..Default::default()
    };
    let nodes: HashMap<_, _> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let mut accessible: HashMap<NodeId, (accesskit::NodeId, &accesskit::Node)> = HashMap::new();
    // Tree order, from the root: findings come out as a reader meets them.
    let mut order = Vec::new();
    let mut parent_of: HashMap<accesskit::NodeId, accesskit::NodeId> = HashMap::new();
    if let Some(root) = update.tree.as_ref().map(|t| t.root) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = nodes.get(&id) else { continue };
            order.push((id, *node));
            for child in node.children() {
                parent_of.insert(*child, id);
            }
            stack.extend(node.children().iter().rev());
        }
    }
    for (id, node) in &order {
        let scene = scene_of.get(id).copied();
        if let Some(scene) = scene {
            accessible.insert(scene, (*id, node));
        }
        check_node(node, scene, &mut report.findings);
    }

    // What takes focus by Tab, in the scene, and what Tab reaches.
    let focusable: Vec<NodeId> = {
        let scene = ui.scene.borrow();
        scene
            .paint_items()
            .filter(|p| p.effects.opacity > 0.)
            .map(|p| p.id)
            .filter(|id| {
                ui.input
                    .options(*id)
                    .is_some_and(|o| o.focusable && !o.disabled && o.tab_index >= 0)
                    && ui.input.is_enabled(&scene, *id)
            })
            .collect()
    };
    report.focusable = focusable.len();
    for &id in &focusable {
        let node = accessible.get(&id).map(|(_, n)| *n);
        let role = node.map(|n| n.role());
        if role.is_none_or(|r| {
            matches!(
                r,
                Native::Unknown | Native::GenericContainer | Native::Label | Native::Paragraph
            )
        }) {
            report.findings.push(finding(
                "no-role",
                Some(id),
                node,
                "takes focus but has no control's role".into(),
            ));
        }
    }
    let walked = walk_tab(ui, focusable.len());
    report.tab_stops = walked.stops.len();
    let reached: HashSet<NodeId> = walked.stops.iter().copied().collect();
    for &id in &focusable {
        let entry = accessible.get(&id).copied();
        let node = entry.map(|(_, n)| n);
        // A group's members are one stop: arrows reach the others.
        let in_group = entry.is_some_and(|(at, n)| {
            grouped(n.role())
                || std::iter::successors(parent_of.get(&at), |p| parent_of.get(p))
                    .any(|p| nodes.get(p).is_some_and(|n| arrows_within(n.role())))
        });
        if reached.contains(&id) || in_group {
            continue;
        }
        // Only a walk that came round saw every stop.
        if walked.round && walked.trap.is_none() && !walked.escaped {
            report.findings.push(finding(
                "focus-unreachable",
                Some(id),
                node,
                "Tab never reaches it".into(),
            ));
        }
    }
    if let Some((at, detail)) = walked.trap {
        let node = accessible.get(&at).map(|(_, n)| *n);
        report
            .findings
            .push(finding("focus-trap", Some(at), node, detail));
    }
    report
}

fn finding(
    rule: &'static str,
    scene: Option<NodeId>,
    node: Option<&accesskit::Node>,
    detail: String,
) -> Finding {
    Finding {
        rule,
        node: scene,
        role: node.map_or("none".into(), |n| format!("{:?}", n.role())),
        name: node
            .and_then(|n| n.label().or(n.value()))
            .unwrap_or_default()
            .chars()
            .take(60)
            .collect(),
        detail,
    }
}

fn check_node(node: &accesskit::Node, scene: Option<NodeId>, out: &mut Vec<Finding>) {
    use Native::*;
    if node.is_hidden() {
        return;
    }
    let role = node.role();
    let mut push = |rule, detail: &str| out.push(finding(rule, scene, Some(node), detail.into()));
    let named =
        node.label().is_some_and(|l| !l.trim().is_empty()) || !node.labelled_by().is_empty();
    if control(role) && !named {
        // A link or option named by its text is named.
        if !(matches!(role, Link | ListBoxOption | TreeItem | MenuItem)
            && node.value().is_some_and(|v| !v.trim().is_empty()))
        {
            push("no-name", "a control with no name");
        }
    }
    if role == Image && !named {
        push(
            "no-name",
            "an image with no name (hide it if it says nothing)",
        );
    }
    if matches!(role, Dialog | AlertDialog) && !named {
        push("no-name", "a dialog with no name");
    }
    if !node.is_disabled() {
        let pressable = matches!(
            role,
            Button
                | Link
                | CheckBox
                | Switch
                | RadioButton
                | Tab
                | MenuItem
                | MenuItemCheckBox
                | MenuItemRadio
                | ListBoxOption
        );
        if pressable && !node.supports_action(Action::Click) {
            push("no-action", "cannot be pressed");
        }
        if control(role) && !node.supports_action(Action::Focus) {
            push("no-action", "cannot take focus");
        }
        if matches!(role, Slider | SpinButton)
            && !(node.supports_action(Action::Increment) || node.supports_action(Action::SetValue))
        {
            push("no-action", "its value cannot be changed");
        }
    }
    match role {
        CheckBox | Switch | RadioButton | MenuItemCheckBox | MenuItemRadio
            if node.toggled().is_none() =>
        {
            push("no-state", "does not say whether it is checked");
        }
        Tab if node.is_selected().is_none() => {
            push("no-state", "does not say whether it is chosen")
        }
        // A progress bar with no value is indeterminate, as ARIA's.
        Slider | SpinButton | ScrollBar if node.numeric_value().is_none() => {
            push("no-state", "has no value")
        }
        Slider | ProgressIndicator
            if (role == Slider || node.numeric_value().is_some())
                && (node.min_numeric_value().is_none() || node.max_numeric_value().is_none()) =>
        {
            push("no-state", "has no range")
        }
        ComboBox if node.value().is_none() && node.is_expanded().is_none() => {
            push("no-state", "says neither its value nor whether it is open")
        }
        _ => {}
    }
    if matches!(role, Alert | Status) && node.live().is_none_or(|l| l == Live::Off) {
        push("not-announced", "an alert or status that is not live");
    }
    if node.invalid().is_some() && node.error_message().is_none() && node.description().is_none() {
        push("not-announced", "invalid, with no error given to a reader");
    }
}

struct Walk {
    stops: Vec<NodeId>,
    /// Focus came round to the first stop: every stop was seen.
    round: bool,
    trap: Option<(NodeId, String)>,
    /// Escape left a modal scope Tab kept: what is outside it went unwalked.
    escaped: bool,
}

fn press(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key: key.clone(),
        modifiers,
        repeat: false,
    });
    ui.dispatch(InputEvent::KeyUp { key, modifiers });
}

/// Press Tab from nothing focused until focus comes round to its first
/// stop, as a person would; then put focus back. A control that keeps Tab
/// (a code editor indenting) is left by Control-Tab, as AppKit's text views
/// are, and a modal scope (a dialog, a popover) by Escape: a trap is one
/// that keeps all three, or a round that leaves stops out.
fn walk_tab(ui: &mut Ui, focusable: usize) -> Walk {
    let before = ui.input.focused();
    let _ = ui.input.focus(&ui.scene, None);
    let mut stops: Vec<NodeId> = Vec::new();
    let mut trap = None;
    let mut escaped = false;
    let mut round = false;
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    // Tab can stop at more than the focusable nodes painted (a list's own
    // stop): a bound far past them, only against a walk that never ends.
    for _ in 0..focusable * 2 + 16 {
        press(ui, Key::Tab, Modifiers::default());
        let Some(mut now) = ui.input.focused() else {
            break;
        };
        if focusable > 1 && stops.last() == Some(&now) {
            press(ui, Key::Tab, control);
            match ui.input.focused() {
                Some(next) if next != now => now = next,
                _ => {
                    // A modal scope (a dialog, a popover) keeps Tab by
                    // design; Escape must leave it.
                    press(ui, Key::Escape, Modifiers::default());
                    if ui.input.focused() == Some(now) {
                        trap = Some((now, "Tab, Control-Tab and Escape stay on it".into()));
                    } else {
                        escaped = true;
                    }
                    break;
                }
            }
        }
        if stops.first() == Some(&now) {
            round = true;
            break;
        }
        if let Some(at) = stops.iter().position(|s| *s == now) {
            trap = Some((
                now,
                format!(
                    "Tab goes round {} stops, not back to the first",
                    stops.len() - at
                ),
            ));
            break;
        }
        stops.push(now);
    }
    let _ = ui.input.focus(&ui.scene, before);
    Walk {
        stops,
        round,
        trap,
        escaped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Node, Toggled};

    fn rules(node: &Node) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        check_node(node, None, &mut out);
        out.into_iter().map(|f| (f.rule, f.detail)).collect()
    }
    fn with(role: Native, actions: &[Action]) -> Node {
        let mut node = Node::new(role);
        for action in actions {
            node.add_action(*action);
        }
        node
    }

    #[test]
    fn a_named_working_control_passes() {
        let mut button = with(Native::Button, &[Action::Click, Action::Focus]);
        button.set_label("Save");
        assert_eq!(rules(&button), []);
        let mut check = with(Native::CheckBox, &[Action::Click, Action::Focus]);
        check.set_label("Wrap");
        check.set_toggled(Toggled::False);
        assert_eq!(rules(&check), []);
        let mut slider = with(
            Native::Slider,
            &[Action::Focus, Action::Increment, Action::SetValue],
        );
        slider.set_label("Volume");
        slider.set_numeric_value(0.4);
        slider.set_min_numeric_value(0.);
        slider.set_max_numeric_value(1.);
        assert_eq!(rules(&slider), []);
    }

    #[test]
    fn controls_and_images_need_names() {
        let button = with(Native::Button, &[Action::Click, Action::Focus]);
        assert_eq!(rules(&button)[0].0, "no-name");
        assert_eq!(rules(&Node::new(Native::Image))[0].0, "no-name");
        assert_eq!(rules(&Node::new(Native::Dialog))[0].0, "no-name");
        // A link named by its text is named; a hidden image says nothing.
        let mut link = with(Native::Link, &[Action::Click, Action::Focus]);
        link.set_value("Docs");
        assert_eq!(rules(&link), []);
        let mut image = Node::new(Native::Image);
        image.set_hidden();
        assert_eq!(rules(&image), []);
    }

    #[test]
    fn controls_need_their_actions_unless_disabled() {
        let mut button = with(Native::Button, &[Action::Focus]);
        button.set_label("Save");
        assert_eq!(rules(&button), [("no-action", "cannot be pressed".into())]);
        button.set_disabled();
        assert_eq!(rules(&button), []);
        let mut slider = with(Native::Slider, &[Action::Focus]);
        slider.set_label("Volume");
        slider.set_numeric_value(0.);
        slider.set_min_numeric_value(0.);
        slider.set_max_numeric_value(1.);
        assert_eq!(
            rules(&slider),
            [("no-action", "its value cannot be changed".into())]
        );
    }

    #[test]
    fn checkable_and_ranged_controls_say_their_state() {
        let mut check = with(Native::Switch, &[Action::Click, Action::Focus]);
        check.set_label("Wi-Fi");
        assert_eq!(rules(&check)[0].0, "no-state");
        for role in [Native::MenuItemCheckBox, Native::MenuItemRadio] {
            let mut item = with(role, &[Action::Click, Action::Focus]);
            item.set_label("Word wrap");
            assert_eq!(rules(&item)[0].0, "no-state");
            item.set_toggled(accesskit::Toggled::True);
            assert!(rules(&item).is_empty(), "{:?}", rules(&item));
        }
        let mut tab = with(Native::Tab, &[Action::Click, Action::Focus]);
        tab.set_label("General");
        assert_eq!(rules(&tab)[0].0, "no-state");
        // Indeterminate: no value, and none needed.
        assert_eq!(rules(&Node::new(Native::ProgressIndicator)), []);
        let mut progress = Node::new(Native::ProgressIndicator);
        progress.set_numeric_value(3.);
        assert_eq!(rules(&progress), [("no-state", "has no range".into())]);
    }

    #[test]
    fn alerts_and_errors_reach_a_reader() {
        let mut alert = Node::new(Native::Alert);
        assert_eq!(rules(&alert)[0].0, "not-announced");
        alert.set_live(Live::Assertive);
        assert_eq!(rules(&alert), []);
        let mut field = with(Native::TextInput, &[Action::Focus, Action::SetValue]);
        field.set_label("Email");
        field.set_invalid(accesskit::Invalid::True);
        assert_eq!(rules(&field)[0].0, "not-announced");
        field.set_description("That is not an email address");
        assert_eq!(rules(&field), []);
    }

    #[test]
    fn the_json_is_one_escaped_line() {
        let report = Report {
            nodes: 2,
            focusable: 1,
            tab_stops: 1,
            findings: vec![Finding {
                rule: "no-name",
                node: None,
                role: "Button".into(),
                name: "say \"hi\"\n".into(),
                detail: "a control with no name".into(),
            }],
        };
        assert_eq!(
            report.to_json(),
            r#"{"nodes":2,"focusable":1,"tab_stops":1,"findings":[{"rule":"no-name","node":null,"role":"Button","name":"say \"hi\"\n","detail":"a control with no name"}]}"#
        );
    }
}
