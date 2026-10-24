//! Retained menu keyboard navigation and trigger metadata.
use crate::{
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::NodeId,
    semantics::{PopupKind, Role},
    widgets::Ui,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
pub(crate) struct MenuContext {
    pub open: Signal<bool>,
    pub parent: Option<Rc<MenuContext>>,
    pub panel: Cell<Option<NodeId>>,
    overlay: Cell<Option<NodeId>>,
    trigger: Option<NodeId>,
    submenu: bool,
    children: RefCell<Vec<Weak<MenuContext>>>,
}
impl MenuContext {
    pub(crate) fn new(
        open: Signal<bool>,
        parent: Option<Rc<MenuContext>>,
        trigger: Option<NodeId>,
        submenu: bool,
    ) -> Rc<Self> {
        let context = Rc::new(Self {
            open,
            parent,
            panel: Cell::new(None),
            overlay: Cell::new(None),
            trigger,
            submenu,
            children: RefCell::new(Vec::new()),
        });
        if let Some(parent) = &context.parent {
            let mut children = parent.children.borrow_mut();
            children.retain(|child| child.strong_count() > 0);
            children.push(Rc::downgrade(&context));
        }
        context
    }
    /// Close this menu's open submenus but `keep` (the one the pointer
    /// moved onto), as moving over another item does.
    pub(crate) fn close_children(&self, keep: Option<&Signal<bool>>) {
        let children: Vec<Rc<MenuContext>> = self
            .children
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for child in children {
            if keep.is_none_or(|keep| !keep.same(&child.open)) && child.open.with_untracked(|o| *o)
            {
                child.open.set(false);
            }
        }
    }
    /// Focus the first enabled item of this menu's open submenu `open`.
    pub(crate) fn focus_submenu(&self, ui: &Ui, open: &Signal<bool>) {
        let panel = self
            .children
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .find(|child| child.open.same(open))
            .and_then(|child| child.panel.get());
        if let Some(first) = panel.and_then(|panel| items(ui, panel).first().copied()) {
            ui.input.focus(&ui.scene, Some(first));
        }
    }
    pub(crate) fn close_chain(&self) {
        self.open.set(false);
        if let Some(parent) = &self.parent {
            parent.close_chain();
        }
    }
}
/// A menu panel's enabled items, in order.
fn items(ui: &Ui, panel: NodeId) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut pending = scene
        .children(panel)
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    while let Some(node) = pending.pop() {
        if semantics
            .get(node)
            .is_some_and(|node| node.role == Role::MenuItem)
            && ui.input.is_enabled(&scene, node)
        {
            items.push(node);
        }
        pending.extend(scene.children(node).iter().rev().copied());
    }
    items
}
#[derive(Default)]
struct Search {
    prefix: String,
    last: Option<Instant>,
}
pub(crate) fn mount(
    ui: &mut Ui,
    panel: NodeId,
    menu: Rc<MenuContext>,
    anchor: NodeId,
    overlay: NodeId,
    dismiss_backdrop: bool,
) {
    menu.overlay.set(Some(overlay));
    if menu.submenu {
        let current = menu.clone();
        ui.bind(panel, move || {
            if !current.open.get() {
                return;
            }
            let siblings = current
                .parent
                .as_ref()
                .map(|parent| {
                    parent
                        .children
                        .borrow()
                        .iter()
                        .filter_map(Weak::upgrade)
                        .filter(|child| !Rc::ptr_eq(child, &current))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for sibling in siblings {
                sibling.open.set(false);
            }
        });
    }
    let search = Rc::new(RefCell::new(Search::default()));
    let reset = search.clone();
    let open = menu.open.clone();
    let semantics = ui.semantics.clone();
    ui.bind(panel, move || {
        let expanded = open.get();
        *reset.borrow_mut() = Search::default();
        semantics.borrow_mut().update(anchor, |node| {
            node.has_popup = Some(PopupKind::Menu);
            node.expanded = Some(expanded);
        });
    });
    let weak = ui.downgrade();
    ui.on_event(overlay, false, move |cx| {
        if cx.phase == EventPhase::Capture || cx.default_prevented() {
            return;
        }
        if cx.phase == EventPhase::Target
            && cx.target == overlay
            && let InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } = cx.event
        {
            if let Some(ui) = weak.upgrade() {
                let mut ancestor = menu.parent.clone();
                let mut destination = None;
                while let Some(parent) = ancestor {
                    if parent.open.get()
                        && let Some(panel) = parent.panel.get()
                    {
                        let scene = ui.scene.borrow();
                        if scene.contains(panel) {
                            let bounds = scene.bounds(panel);
                            if x >= bounds.x
                                && y >= bounds.y
                                && x < bounds.x + bounds.width
                                && y < bounds.y + bounds.height
                            {
                                destination = Some(parent.clone());
                                break;
                            }
                        }
                    }
                    ancestor = parent.parent.clone();
                }
                if destination.is_none() && !dismiss_backdrop {
                    cx.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                let mut closing = Some(menu.clone());
                while let Some(current) = closing {
                    if destination
                        .as_ref()
                        .is_some_and(|target| Rc::ptr_eq(target, &current))
                    {
                        break;
                    }
                    current.open.set(false);
                    if let Some(overlay) = current.overlay.get() {
                        crate::compose_modal::deactivate(&ui, overlay);
                    }
                    closing = current.parent.clone();
                }
                if let Some(parent) = destination
                    && parent.open.get()
                    && parent.overlay.get() == ui.input.focus_scope()
                    && parent
                        .panel
                        .get()
                        .is_some_and(|panel| ui.scene.borrow().contains(panel))
                {
                    ui.input.dispatch(&ui.scene, cx.event.clone());
                }
            }
            cx.prevent_default();
            cx.stop_propagation();
            return;
        }
        let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
            return;
        };
        if *key == Key::Tab {
            menu.close_chain();
            cx.stop_propagation();
            return;
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if cx.target != panel
            && cx.target != overlay
            && !ui
                .semantics
                .borrow()
                .get(cx.target)
                .is_some_and(|node| node.role == Role::MenuItem)
        {
            return;
        }
        // A submenu opens towards the line's end: right to left, the left.
        // The panels are in an overlay; the menu's own trigger says which
        // way its text runs.
        let rtl = {
            let mut root = menu.clone();
            while let Some(parent) = root.parent.clone() {
                root = parent;
            }
            let scene = ui.scene.borrow();
            root.trigger
                .filter(|t| scene.contains(*t))
                .is_some_and(|t| scene.direction(t) == crate::scene::Direction::Rtl)
        };
        let (back, into) = if rtl {
            (Key::ArrowRight, Key::ArrowLeft)
        } else {
            (Key::ArrowLeft, Key::ArrowRight)
        };
        if *key == back && menu.submenu {
            menu.open.set(false);
            if let Some(trigger) = menu.trigger {
                ui.input.focus(&ui.scene, Some(trigger));
            }
            cx.prevent_default();
            cx.stop_propagation();
            return;
        }
        if *key == into {
            let child = menu
                .children
                .borrow()
                .iter()
                .filter_map(Weak::upgrade)
                .find(|child| child.submenu && child.trigger == Some(cx.target));
            if let Some(child) = child {
                child.open.set(true);
                cx.prevent_default();
                cx.stop_propagation();
            }
            return;
        }
        let items = items(&ui, panel);
        if items.is_empty() {
            return;
        }
        let current = items
            .iter()
            .position(|node| Some(*node) == ui.input.focused());
        let target = match key {
            Key::ArrowDown => Some(items[current.map_or(0, |index| (index + 1) % items.len())]),
            Key::ArrowUp => Some(
                items[current.map_or(items.len() - 1, |index| {
                    (index + items.len() - 1) % items.len()
                })],
            ),
            Key::Home => Some(items[0]),
            Key::End => items.last().copied(),
            Key::Character(text) if !modifiers.control && !modifiers.alt && !modifiers.meta => {
                let Some(text) = bounded_prefix(text) else {
                    return;
                };
                let mut search = search.borrow_mut();
                let now = Instant::now();
                if search
                    .last
                    .is_none_or(|last| now.duration_since(last) > Duration::from_millis(750))
                {
                    search.prefix.clear();
                }
                search.last = Some(now);
                if search.prefix.len() + text.len() > 256 {
                    search.prefix.clear();
                }
                // Repeating the same prefix cycles matching items, including
                // labels such as "Aaron" that also match a doubled letter.
                if search.prefix != text {
                    search.prefix.push_str(&text);
                }
                let find = |prefix: &str| {
                    (1..=items.len())
                        .map(|step| {
                            items[current.map_or(step - 1, |index| (index + step) % items.len())]
                        })
                        .find(|node| {
                            ui.semantics
                                .borrow()
                                .get(*node)
                                .is_some_and(|node| matches_prefix(&node.label, prefix))
                        })
                };
                let target = find(&search.prefix);
                if target.is_none() {
                    search.prefix = text;
                    find(&search.prefix)
                } else {
                    target
                }
            }
            _ => return,
        };
        if let Some(target) = target {
            ui.input.focus(&ui.scene, Some(target));
        }
        cx.prevent_default();
        cx.stop_propagation();
    });
}

/// A chosen item flashes off and on, then its menu closes and `then` (its
/// action) runs, as AppKit's menus confirm a choice (macOS). Choosing again
/// meanwhile does nothing.
pub(crate) fn flash(
    ui: &Ui,
    runner: &crate::compose::TaskRunner,
    menu: Rc<MenuContext>,
    item: NodeId,
    then: impl FnOnce() + 'static,
) {
    thread_local! {
        /// Whether a flash runs (one at most), and its task, kept until the
        /// next.
        static FLASHING: Cell<bool> = const { Cell::new(false) };
        static TASK: RefCell<Option<crate::compose::TaskToken>> = const { RefCell::new(None) };
    }
    if FLASHING.replace(true) {
        return;
    }
    // Off: neither focused nor hovered.
    if let Some(panel) = menu.panel.get() {
        ui.input.focus(&ui.scene, Some(panel));
    }
    ui.input.clear_hover(&ui.scene);
    /// Ends the flash however its task does (dropped with its window too).
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            FLASHING.set(false);
        }
    }
    let weak = ui.downgrade();
    let task = runner.spawn(Box::pin(async move {
        let _done = Done;
        let step = Duration::from_millis(50);
        crate::timer::sleep(step).await;
        if let Some(ui) = weak.upgrade() {
            ui.input.focus(&ui.scene, Some(item));
        }
        crate::timer::sleep(step).await;
        drop(_done);
        menu.close_chain();
        then();
    }));
    TASK.with(|slot| *slot.borrow_mut() = Some(task));
}
/// A menu's trigger opens it as it is pressed, as AppKit's pop-up and
/// pull-down buttons do: dragged on to an item and released there, that
/// item is chosen; released over the trigger (a click), the menu stays open
/// to click in; dragged away and released elsewhere, it closes. A press on
/// the trigger of an open menu lands on its backdrop, which closes it. Only
/// a trigger listening already (a button) is one: a bare anchor (a context
/// menu's point) stays out of hit testing.
pub(crate) fn mount_press(
    ui: &mut Ui,
    anchor: NodeId,
    overlay: NodeId,
    panel: NodeId,
    open: Signal<bool>,
) {
    if !ui.input.has_listeners(anchor) {
        return;
    }
    // Where the press opening the menu was, while it is held. The menu's
    // focus scope cancels the press on the trigger, so its overlay (inside
    // the scope, over the window and holding the items) follows it.
    let pressed = Rc::new(Cell::new(None::<(f32, f32)>));
    let (start, close) = (pressed.clone(), open.clone());
    let binding = ui.input.listen(anchor, move |cx| {
        if cx.phase == EventPhase::Capture || cx.default_prevented() {
            return;
        }
        match cx.event {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } if !open.with_untracked(|open| *open) => {
                // The press opens the menu, focus going into it, rather
                // than to the trigger.
                cx.prevent_default();
                start.set(Some((x, y)));
                open.set(true);
            }
            // Released before the menu showed (a click in one batch of
            // input): on the trigger, it stays open.
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => start.set(None),
            _ => {}
        }
    });
    ui.retain(anchor, binding);
    let weak = ui.downgrade();
    let binding = ui.input.listen(overlay, move |cx| {
        // Before what is under the pointer hears it (or as the backdrop).
        if cx.phase == EventPhase::Bubble || pressed.get().is_none() {
            return;
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        match cx.event {
            // A new press: the one that opened the menu has ended unheard.
            InputEvent::PointerDown { .. } => pressed.set(None),
            InputEvent::PointerMove { x, y } => {
                if let Some(item) = item_at(&ui, panel, x, y)
                    && ui.input.focused() != Some(item)
                {
                    ui.input.focus(&ui.scene, Some(item));
                }
            }
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                let (px, py) = pressed.take().expect("checked above");
                cx.prevent_default();
                cx.stop_propagation();
                // A click (no drag): it stays open to click in, though an
                // item opened under the pointer (a pop-up button's,
                // `open_over`), as AppKit's does.
                let clicked = (x - px).hypot(y - py) <= 4.;
                if clicked {
                    return;
                }
                if let Some(item) = item_at(&ui, panel, x, y) {
                    ui.input.dispatch_to(&ui.scene, item, InputEvent::Activate);
                    return;
                }
                let trigger = ui.scene.borrow().bounds(anchor);
                let on_trigger = x >= trigger.x
                    && y >= trigger.y
                    && x < trigger.x + trigger.width
                    && y < trigger.y + trigger.height;
                if !on_trigger && (x - px).hypot(y - py) > 4. {
                    close.set(false);
                }
            }
            _ => {}
        }
    });
    ui.retain(anchor, binding);
}
/// The enabled item of the menu `panel` at window point `(x, y)`.
fn item_at(ui: &Ui, panel: NodeId, x: f32, y: f32) -> Option<NodeId> {
    let scene = ui.scene.borrow();
    if !scene.contains(panel) {
        return None;
    }
    let semantics = ui.semantics.borrow();
    let hit = scene.hit_test(x, y)?;
    std::iter::once(hit)
        .chain(scene.ancestors(hit))
        .take_while(|node| *node != panel)
        .find(|node| {
            semantics
                .get(*node)
                .is_some_and(|semantic| semantic.role == Role::MenuItem)
                && ui.input.is_enabled(&scene, *node)
        })
        .filter(|item| scene.is_within(*item, panel))
}

fn bounded_prefix(text: &str) -> Option<String> {
    let mut result = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_control() {
            return None;
        }
        if result.len() + character.len_utf8() > 256 {
            break;
        }
        result.push(character);
    }
    (!result.is_empty()).then_some(result)
}
fn matches_prefix(label: &str, prefix: &str) -> bool {
    let mut label = label.chars().flat_map(char::to_lowercase);
    prefix
        .chars()
        .all(|character| label.next() == Some(character))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typeahead_storage_is_bounded_with_unicode_case_expansion() {
        let prefix = bounded_prefix(&"İ".repeat(10000)).unwrap();
        assert!(prefix.len() <= 256);
        assert!(prefix.starts_with("i\u{307}"));
        assert!(matches_prefix(&"İ".repeat(10000), &prefix));
        assert!(bounded_prefix("\n").is_none());
    }
}
