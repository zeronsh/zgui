//! Owned overlay portals shared by modal and anchored component views.
use crate::{
    input::{EventPhase, InputEvent, Key, NodeInput, PointerButton},
    reactive::Signal,
    scene::{Effects, Layout, NodeId, Style, Transform},
    semantics::{Role, SemanticNode},
    widgets::{Ui, WeakUi},
};

pub(crate) struct Portal {
    pub overlay: NodeId,
    pub position: NodeId,
    pub visible: Signal<bool>,
}
struct Owner {
    ui: WeakUi,
    overlay: NodeId,
}
fn unwind(ui: &Ui, overlay: NodeId) {
    ui.input.remove_focus_scope(&ui.scene, overlay);
}
impl Drop for Owner {
    fn drop(&mut self) {
        let Some(mut ui) = self.ui.upgrade() else {
            return;
        };
        unwind(&ui, self.overlay);
        ui.remove(self.overlay);
    }
}
pub(crate) fn create(ui: &mut Ui, owner: NodeId, parent: NodeId) -> Portal {
    let overlay = ui.container(
        parent,
        Layout::Overlay,
        Style {
            absolute: true,
            ..Default::default()
        },
    );
    ui.retain(
        owner,
        Owner {
            ui: ui.downgrade(),
            overlay,
        },
    );
    let position = ui.container(overlay, Layout::Overlay, Default::default());
    Portal {
        overlay,
        position,
        visible: ui.signal(false),
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn activate(
    ui: &mut Ui,
    owner: NodeId,
    portal: &Portal,
    panel: NodeId,
    label: String,
    open: Signal<bool>,
    parent_visible: Option<Signal<bool>>,
    dismiss_backdrop: bool,
    role: Role,
    logical_parent: Option<NodeId>,
    return_focus: Option<NodeId>,
    focusable: bool,
    // A submenu: its menu stays live under it.
    porous: bool,
    // Focus its first control as it opens (else the panel itself).
    autofocus: bool,
    // A non-modal layer (`View::layer`).
    layer: bool,
) {
    if layer {
        ui.input.set_layer(portal.overlay, true);
        let mut options = ui.input.options(portal.overlay).unwrap_or_default();
        options.pass_through = true;
        ui.input.set_options(portal.overlay, options);
    }
    let raised = std::rc::Rc::new(std::cell::Cell::new(false));
    let overlay = portal.overlay;
    let mut semantics = SemanticNode::new(role, label);
    semantics.modal = role == Role::Dialog;
    semantics.logical_parent = logical_parent;
    semantics.disabled = ui
        .semantics
        .borrow()
        .get(panel)
        .is_some_and(|node| node.disabled);
    ui.semantics.borrow_mut().set(panel, semantics);
    let mut options = ui.input.options(panel).unwrap_or_default();
    options.focusable = focusable;
    // Focus lands on the panel only as it opens with nothing in it to take
    // focus: Tab moves among its controls, never to the panel itself, as
    // in a native dialog.
    if focusable {
        options.tab_index = -1;
    }
    ui.input.set_options(panel, options);
    // Blank panel space is a hit boundary, separate from the backdrop.
    ui.on_event(panel, true, |_| {});
    let close = open.clone();
    let input = ui.input.clone();
    ui.on_event(overlay, false, move |cx| match cx.event {
        InputEvent::FocusScopeClosed if cx.phase == EventPhase::Target => {
            close.set(false);
        }
        InputEvent::KeyDown {
            key: Key::Escape, ..
        } if cx.phase != EventPhase::Capture
            && !cx.default_prevented()
            && input.focus_scope() == Some(overlay) =>
        {
            close.set(false);
            cx.prevent_default();
            cx.stop_propagation();
        }
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            ..
        } if cx.phase == EventPhase::Target
            && !cx.default_prevented()
            && input.focus_scope() == Some(overlay) =>
        {
            if dismiss_backdrop {
                close.set(false);
            }
            cx.prevent_default();
            cx.stop_propagation();
        }
        _ => {}
    });
    let root_bounds = ui.observe_bounds(ui.root());
    let scene = ui.scene.clone();
    ui.bind(owner, move || {
        let bounds = root_bounds.get();
        let mut scene = scene.borrow_mut();
        scene.set_style(
            overlay,
            Style {
                absolute: true,
                width: Some(bounds.width),
                height: Some(bounds.height),
                ..Default::default()
            },
        );
    });
    // Portals escape visual ancestry, but retain the logical owner's disability.
    let ancestors = {
        let scene = ui.scene.borrow();
        std::iter::once(owner)
            .chain(scene.ancestors(owner))
            .collect::<Vec<_>>()
    };
    let disabled = ancestors
        .into_iter()
        .map(|node| ui.input.observe_disabled(node, &ui.runtime))
        .collect::<Vec<_>>();
    let weak = ui.downgrade();
    let visible = portal.visible.clone();
    ui.bind(owner, move || {
        let blocked = disabled.iter().any(|disabled| disabled.get());
        let requested = open.get();
        if blocked && requested {
            open.set(false);
        }
        let desired =
            requested && !blocked && parent_visible.as_ref().is_none_or(|parent| parent.get());
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if !ui.scene.borrow().contains(overlay) {
            return;
        }
        ui.scene.borrow_mut().set_effects(
            overlay,
            Effects {
                opacity: if desired { 1. } else { 0. },
                ..Default::default()
            },
        );
        ui.input.set_options(
            overlay,
            NodeInput {
                disabled: !desired,
                pass_through: ui.input.options(overlay).is_some_and(|o| o.pass_through),
                ..Default::default()
            },
        );
        if layer {
            // Raised over every dialog as it opens; no focus, no scope.
            if desired && !raised.get() {
                let parent = ui.scene.borrow().parent(overlay).unwrap();
                let mut children = ui.scene.borrow().children(parent).to_vec();
                children.retain(|node| *node != overlay);
                children.push(overlay);
                ui.scene.borrow_mut().reorder_children(parent, &children);
            }
            raised.set(desired);
            visible.set(desired && ui.scene.borrow().contains(overlay));
            return;
        }
        if desired {
            let already_active = {
                let scene = ui.scene.borrow();
                ui.input.focus_scope().is_some_and(|active| {
                    active == overlay
                        || (scene.contains(active)
                            && scene.ancestors(active).any(|node| node == overlay))
                })
            };
            if !already_active {
                // Raise only when opening, preserving paint order within the scope.
                let parent = ui.scene.borrow().parent(overlay).unwrap();
                let mut children = ui.scene.borrow().children(parent).to_vec();
                children.retain(|node| *node != overlay);
                children.push(overlay);
                ui.scene.borrow_mut().reorder_children(parent, &children);
                if let Some(trigger) = return_focus {
                    ui.input.focus(&ui.scene, Some(trigger));
                }
                let pushed = if porous {
                    ui.input.push_porous_focus_scope(&ui.scene, overlay)
                } else {
                    ui.input.push_focus_scope(&ui.scene, overlay)
                };
                if !pushed {
                    open.set(false);
                    visible.set(false);
                    return;
                }
                if !ui.scene.borrow().contains(panel) || !ui.scene.borrow().contains(overlay) {
                    open.set(false);
                    visible.set(false);
                    return;
                }
                // Prefer a usable child control; the panel remains the empty-dialog fallback.
                let candidates = {
                    let scene = ui.scene.borrow();
                    let mut result = Vec::new();
                    let mut pending = scene
                        .children(panel)
                        .iter()
                        .rev()
                        .copied()
                        .collect::<Vec<_>>();
                    while let Some(node) = pending.pop() {
                        result.push(node);
                        pending.extend(scene.children(node).iter().rev().copied());
                    }
                    result
                };
                // Not focusing a control: the panel holds focus (keys reach it).
                let candidates = if autofocus { candidates } else { vec![panel] };
                for candidate in candidates {
                    if candidate == panel && !autofocus {
                        if ui.input.focus(&ui.scene, Some(panel)) {
                            break;
                        }
                        continue;
                    }
                    if (role != Role::Menu
                        || ui
                            .semantics
                            .borrow()
                            .get(candidate)
                            .is_some_and(|node| node.role == Role::MenuItem))
                        && ui
                            .input
                            .options(candidate)
                            .is_some_and(|options| options.focusable)
                        && (ui.input.focused() == Some(candidate)
                            || ui.input.focus(&ui.scene, Some(candidate)))
                    {
                        break;
                    }
                }
            }
            visible.set(ui.scene.borrow().contains(overlay));
        } else {
            visible.set(false);
            unwind(&ui, overlay);
        }
    });
}
/// Synchronously close presentation during pointer handoff, even inside a batch.
pub(crate) fn deactivate(ui: &Ui, overlay: NodeId) {
    if ui.scene.borrow().contains(overlay) {
        let mut scene = ui.scene.borrow_mut();
        let mut effects = scene.effects(overlay);
        effects.opacity = 0.;
        scene.set_effects(overlay, effects);
        drop(scene);
        let mut options = ui.input.options(overlay).unwrap_or_default();
        options.disabled = true;
        ui.input.set_options(overlay, options);
    }
    ui.input.remove_focus_scope(&ui.scene, overlay);
}
/// Where a modal's panel sits in the window: lined up with its start,
/// centre or end on each axis, inside `insets` (top, right, bottom, left)
/// from the window's edges. Centred, with no insets, unless set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModalPlacement {
    /// Where it lines up across the window.
    pub x: crate::compose_popover::Edge,
    /// Where it lines up down the window.
    pub y: crate::compose_popover::Edge,
    /// Room kept from the window's top, right, bottom and left edges.
    pub insets: [f32; 4],
}
impl ModalPlacement {
    /// Centred on both axes, with no insets.
    pub fn centered() -> Self {
        Self {
            x: crate::compose_popover::Edge::Center,
            y: crate::compose_popover::Edge::Center,
            insets: [0.; 4],
        }
    }
}
pub(crate) fn center(ui: &mut Ui, owner: NodeId, position: NodeId) {
    place(ui, owner, position, ModalPlacement::centered());
}
pub(crate) fn place(ui: &mut Ui, owner: NodeId, position: NodeId, at: ModalPlacement) {
    use crate::compose_popover::Edge;
    let viewport = ui.observe_bounds(ui.root());
    let panel = ui.observe_bounds(position);
    let scene = ui.scene.clone();
    ui.bind(owner, move || {
        let viewport = viewport.get();
        let panel = panel.get();
        let mut scene = scene.borrow_mut();
        let prior = scene.transform(position);
        let [top, right, bottom, left] = at.insets;
        let along = |edge: Edge, start: f32, extent: f32, before: f32, after: f32, size: f32| {
            let room = extent - before - after;
            start
                + before
                + match edge {
                    Edge::Start => 0.,
                    Edge::Center => ((room - size) / 2.).max(0.),
                    Edge::End => (room - size).max(0.),
                }
        };
        let x = along(at.x, viewport.x, viewport.width, left, right, panel.width);
        let y = along(at.y, viewport.y, viewport.height, top, bottom, panel.height);
        if (x - panel.x).abs() > 0.01 || (y - panel.y).abs() > 0.01 {
            scene.set_transform(
                position,
                Transform {
                    x: prior.x + x - panel.x,
                    y: prior.y + y - panel.y,
                },
            );
        }
    });
}
