//! Menus opened at the pointer: a secondary click (or a control-click on
//! macOS) on a view with [`View::context_menu`] builds its entries then and
//! shows them in a menu where the pointer is (or under the view, with
//! [`View::context_menu_below`]), over everything. The innermost view with a
//! context menu takes the click.
use crate::{
    compose::{Environment, View, div, menu},
    input::{EventPhase, InputEvent, PointerButton},
    scene::NodeId,
    style::{Styled, Styles},
    widgets::Ui,
};
use std::{cell::Cell, rc::Rc};

/// Where a context menu was asked for: in window points, and from the top
/// left of the view that has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenuEvent {
    pub x: f32,
    pub y: f32,
    pub local_x: f32,
    pub local_y: f32,
}

pub(crate) type ContextMenuBuilder = Box<dyn FnMut(&ContextMenuEvent) -> Vec<View>>;

impl View {
    /// Show a menu at the pointer on a secondary click (a control-click on
    /// macOS): `build` returns its entries (`menu_item`s, `submenu`s,
    /// separators) when asked; none shows nothing and lets the click through.
    pub fn context_menu(
        mut self,
        build: impl FnMut(&ContextMenuEvent) -> Vec<View> + 'static,
    ) -> Self {
        self.context_menu = Some(Box::new(build));
        self
    }
    /// Open the context menu under the view, aligned to its left edge, as a
    /// tab's or a toolbar button's does, rather than at the pointer.
    pub fn context_menu_below(mut self, below: bool) -> Self {
        self.context_menu_below = Some(below);
        self
    }
    /// Styles for the context menu's panel, over a menu's defaults.
    pub fn context_menu_style(mut self, styles: Styles) -> Self {
        self.context_menu_style = Some(Box::new(styles));
        self
    }
}

/// The menu mounted for a view, removed when the next one opens or the view
/// goes (a closed menu shows nothing until then).
struct Open {
    ui: crate::widgets::WeakUi,
    node: Cell<Option<NodeId>>,
}
impl Open {
    fn close(&self) {
        if let (Some(node), Some(mut ui)) = (self.node.take(), self.ui.upgrade())
            && ui.scene.borrow().contains(node)
        {
            ui.remove(node);
        }
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn mount(
    ui: &mut Ui,
    node: NodeId,
    mut build: ContextMenuBuilder,
    style: Option<Styles>,
    below: bool,
    environment: Environment,
) {
    let weak = ui.downgrade();
    let open = Rc::new(Open {
        ui: ui.downgrade(),
        node: Cell::new(None),
    });
    let current = open.clone();
    let binding = ui.input.listen(node, move |cx| {
        if cx.phase == EventPhase::Capture || cx.default_prevented() {
            return;
        }
        let (x, y) = match cx.event {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Secondary,
            } => (x, y),
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } if cfg!(target_os = "macos") && cx.pointer_modifiers().control => (x, y),
            _ => return,
        };
        let Some(mut ui) = weak.upgrade() else {
            return;
        };
        let origin = ui.scene.borrow().bounds(node);
        let event = ContextMenuEvent {
            x,
            y,
            local_x: x - origin.x,
            local_y: y - origin.y,
        };
        let items = build(&event);
        if items.is_empty() {
            return;
        }
        cx.prevent_default();
        cx.stop_propagation();
        current.close();
        let shown = ui.runtime.signal(true);
        let anchor = if below {
            div().w(origin.width).h(origin.height)
        } else {
            div().w(1.).h(1.)
        };
        let mut panel = menu("Context menu", shown, anchor).children(items);
        if let Some(style) = &style {
            panel = panel.merge_styles(style);
        }
        // What it hangs from: the pointer's point, or the view's box (a
        // menu opens under its anchor).
        let view = if below {
            div()
                .absolute()
                .left(origin.x)
                .top(origin.y)
                .w(origin.width)
                .h(origin.height)
                .child(panel)
        } else {
            div().absolute().left(x).top(y).w(0.).h(0.).child(panel)
        };
        let root = ui.root();
        let mounted = crate::compose::mount(&mut ui, root, view, environment.clone());
        current.node.set(Some(mounted));
    });
    ui.retain(node, binding);
    ui.retain(node, open);
}
