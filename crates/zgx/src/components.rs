//! Composable scrolling, virtualization, and modal overlays.
//!
//! Handles refer to widgets owned by `Ui`. Remove their root through `Ui::remove`
//! to dispose subscriptions and event handlers; dropping a handle alone does not
//! unmount the widget. Overlay components belong in a full-window Overlay layer.
use crate::{
    input::{EventPhase, InputDispatcher, InputEvent, Key, NodeInput, PointerButton},
    keyed::KeyedChildren,
    reactive::Signal,
    scene::{Effects, Layout, NodeId, NodeKind, Style, Transform},
    semantics::{Role, SemanticNode},
    view::ViewScope,
    virtual_list::VirtualList,
    widgets::{Ui, fixed},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::Hash,
    rc::{Rc, Weak},
};

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
fn clamped_offset(offset: f32, content: f32, viewport: f32) -> f32 {
    finite_nonnegative(offset).min((content - viewport).max(0.0))
}

/// Vertical scroll viewport. Content extent is explicit to avoid layout polling
/// and can be changed reactively as data grows. Wheel deltas use positive-down
/// logical pixels. Nested scrolling chains to its parent when already at an edge.
#[derive(Clone)]
pub struct ScrollView {
    /// The clipping viewport node.
    pub root: NodeId,
    /// The column holding the content: mount children here.
    pub content: NodeId,
    /// The scroll offset in logical pixels, 0 at the top; kept between 0 and the
    /// content height less the viewport's.
    pub offset: Signal<f32>,
    /// The content's height in logical pixels, which bounds scrolling.
    pub content_height: Signal<f32>,
    /// The viewport's width and height in logical pixels.
    pub size: Signal<(f32, f32)>,
}
impl ScrollView {
    /// Mount a `width` by `height` viewport under `parent`, scrolled by the wheel.
    /// Non-finite or negative sizes count as 0.
    pub fn mount(ui: &mut Ui, parent: NodeId, width: f32, height: f32) -> Self {
        let width = finite_nonnegative(width);
        let height = finite_nonnegative(height);
        let root = ui.container(
            parent,
            Layout::Overlay,
            Style {
                clip: true,
                ..fixed(width, height)
            },
        );
        let content = ui.container(root, Layout::Column, fixed(width, 0.0));
        let offset = ui.signal(0.0);
        let content_height = ui.signal(0.0);
        let size = ui.signal((width, height));
        let scroll = Self {
            root,
            content,
            offset,
            content_height,
            size,
        };
        ui.semantics
            .borrow_mut()
            .set(root, SemanticNode::new(Role::ScrollView, ""));
        let state = scroll.clone();
        let scene = ui.scene.clone();
        ui.bind(root, move || {
            let (width, height) = state.size.get();
            let width = finite_nonnegative(width);
            let height = finite_nonnegative(height);
            let extent = finite_nonnegative(state.content_height.get());
            let offset = clamped_offset(state.offset.get(), extent, height);
            state.offset.set(offset);
            let mut scene = scene.borrow_mut();
            if !scene.contains(root) {
                return;
            }
            let mut viewport_style = scene.style(root);
            viewport_style.width = Some(width);
            viewport_style.height = Some(height);
            viewport_style.clip = true;
            scene.set_style(root, viewport_style);
            let mut content_style = scene.style(content);
            content_style.width = Some(width);
            content_style.height = Some(extent);
            scene.set_style(content, content_style);
            scene.set_transform(content, Transform { x: 0.0, y: -offset });
        });
        let state = scroll.clone();
        ui.on_event(root, false, move |cx| {
            if cx.phase == EventPhase::Capture || cx.default_prevented() {
                return;
            }
            // A Control-wheel zooms the window instead.
            if let InputEvent::Scroll { delta_y, .. } = cx.event
                && !crate::zoom::wheel_zooms()
            {
                let before = state.offset.get();
                state.scroll_to(before + delta_y);
                if state.offset.get() != before {
                    cx.prevent_default();
                    cx.stop_propagation();
                }
            }
        });
        scroll
    }
    /// Scroll to `offset`, clamped to the scrollable range.
    pub fn scroll_to(&self, offset: f32) {
        self.offset.set(clamped_offset(
            offset,
            self.content_height.get(),
            self.size.get().1,
        ));
    }
    /// Set the content's height; non-finite or negative is 0.
    pub fn set_content_height(&self, height: f32) {
        self.content_height.set(finite_nonnegative(height));
    }
    /// Set the viewport's size; non-finite or negative sizes are 0.
    pub fn resize(&self, width: f32, height: f32) {
        self.size
            .set((finite_nonnegative(width), finite_nonnegative(height)));
    }
}

/// Fixed-height, keyed virtual rows. Only visible rows plus overscan retain scene
/// nodes and subscriptions. Keys must be unique in the visible range. Offscreen
/// rows are disposed; put persistent row state in your model, keyed by identity.
pub struct VirtualListView<K: Eq + Hash> {
    /// The viewport the rows scroll in.
    pub scroll: ScrollView,
    /// The number of rows; set it as the data changes.
    pub count: Signal<usize>,
    rows: Weak<RefCell<KeyedChildren<K>>>,
}
impl<K: Eq + Hash + Clone + 'static> VirtualListView<K> {
    /// Mount a `width` by `height` list of `count` rows, each `row_height` tall,
    /// under `parent`. The rows in view, and `overscan` more on either side, are
    /// mounted: `key` names the row at an index, and `initialize` fills a row's
    /// scope when its key is first shown. Panics unless `row_height` is finite and
    /// positive, and when keys in view repeat.
    #[allow(clippy::too_many_arguments)]
    pub fn mount(
        ui: &mut Ui,
        parent: NodeId,
        width: f32,
        height: f32,
        count: usize,
        row_height: f32,
        overscan: usize,
        key: impl Fn(usize) -> K + 'static,
        mut initialize: impl FnMut(usize, &K, &mut ViewScope) + 'static,
    ) -> Self {
        assert!(row_height.is_finite() && row_height > 0.0);
        let scroll = ScrollView::mount(ui, parent, width, height);
        let count = ui.signal(count);
        let rows = Rc::new(RefCell::new(KeyedChildren::mount(
            &ui.runtime,
            ui.scene.clone(),
            scroll.content,
            Layout::Overlay,
            Style::default(),
        )));
        let state = scroll.clone();
        let row_scopes = rows.clone();
        let length = count.clone();
        let scene = ui.scene.clone();
        ui.bind(scroll.root, move || {
            let count = length.get();
            let (width, height) = state.size.get();
            let model = VirtualList::new(count, row_height, overscan);
            state.set_content_height(model.content_height());
            let range = model.visible_range(state.offset.get(), height);
            let mut indices = HashMap::with_capacity(range.len());
            let keys: Vec<K> = range
                .map(|index| {
                    let key = key(index);
                    indices.insert(key.clone(), index);
                    key
                })
                .collect();
            let mut rows = row_scopes.borrow_mut();
            rows.reconcile(keys, |key, scope| initialize(indices[key], key, scope))
                .expect("virtual list keys must be unique");
            let mut scene = scene.borrow_mut();
            for key in rows.keys() {
                let node = rows.get(key).unwrap().root();
                scene.set_style(node, fixed(width, row_height));
                scene.set_transform(
                    node,
                    Transform {
                        x: 0.0,
                        y: model.row_offset(indices[key]),
                    },
                );
            }
        });
        Self {
            scroll,
            count,
            rows: Rc::downgrade(&rows),
        }
    }
    /// How many rows are mounted now (0 once the list is gone).
    pub fn mounted_rows(&self) -> usize {
        self.rows
            .upgrade()
            .map_or(0, |rows| rows.borrow().keys().len())
    }
    /// The root node of row `key`, if it is mounted.
    pub fn row(&self, key: &K) -> Option<NodeId> {
        self.rows
            .upgrade()
            .and_then(|rows| rows.borrow().get(key).map(ViewScope::root))
    }
}

/// Modal overlay with focus trapping and restoration. Populate `body`, then call
/// `show`. `Escape` dismisses; backdrop dismissal is configurable. Nest dialogs
/// under the currently active dialog root so scope nesting follows scene ancestry.
#[derive(Clone)]
pub struct Dialog {
    /// The full-viewport root holding the backdrop and the panel.
    pub root: NodeId,
    /// The panel: mount the dialog's content here.
    pub body: NodeId,
    /// Whether it is open. `Escape`, a backdrop press (when enabled) and the
    /// closing of its focus scope set it false.
    pub open: Signal<bool>,
    scene: Weak<RefCell<crate::scene::Scene>>,
    centered: bool,
}
impl Dialog {
    /// Mount a closed `width` by `height` modal titled `title` (its accessible
    /// name) under `overlay_parent`, centred over a dimming backdrop. With
    /// `dismiss_on_backdrop`, a press on the backdrop closes it.
    pub fn mount(
        ui: &mut Ui,
        overlay_parent: NodeId,
        title: impl Into<String>,
        width: f32,
        height: f32,
        dismiss_on_backdrop: bool,
    ) -> Self {
        let viewport = ui.scene.borrow().style(ui.root());
        let vw = viewport.width.unwrap_or(width);
        let vh = viewport.height.unwrap_or(height);
        let root = ui.container(overlay_parent, Layout::Overlay, fixed(vw, vh));
        ui.scene.borrow_mut().append(
            root,
            NodeKind::Rect(crate::scene::Color(0, 0, 0, 130)),
            fixed(vw, vh),
        );
        let body = ui.container(root, Layout::Overlay, fixed(width, height));
        ui.scene.borrow_mut().set_transform(
            body,
            Transform {
                x: ((vw - width) / 2.0).max(0.0),
                y: ((vh - height) / 2.0).max(0.0),
            },
        );
        let surface = ui
            .theme_scope()
            .color_untracked(crate::theme::tok::SURFACE_OVERLAY);
        ui.scene
            .borrow_mut()
            .append(body, NodeKind::Rect(surface), fixed(width, height));
        let mut semantics = SemanticNode::new(Role::Dialog, title);
        semantics.modal = true;
        ui.semantics.borrow_mut().set(root, semantics);
        let open = ui.signal(false);
        let value = open.clone();
        ui.on_event(root, true, move |cx| match cx.event {
            InputEvent::FocusScopeClosed if cx.phase == EventPhase::Target => {
                value.set(false);
            }
            InputEvent::KeyDown {
                key: Key::Escape, ..
            } if cx.phase != EventPhase::Capture && !cx.default_prevented() => {
                value.set(false);
                cx.prevent_default();
                cx.stop_propagation();
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } if dismiss_on_backdrop
                && !cx.default_prevented()
                && cx.phase == EventPhase::Target
                && cx.target == root =>
            {
                value.set(false);
                cx.prevent_default();
                cx.stop_propagation();
            }
            _ => {}
        });
        // The body is a hit boundary, so clicks in empty panel space do not
        // become backdrop clicks. Child controls still receive ordinary events.
        ui.on_event(body, false, |_| {});
        let scene = ui.scene.clone();
        let input = ui.input.clone();
        let value = open.clone();
        ui.bind(root, move || {
            let visible = value.get();
            if !scene.borrow().contains(root) {
                return;
            }
            scene.borrow_mut().set_effects(
                root,
                Effects {
                    opacity: if visible { 1.0 } else { 0.0 },
                    ..Effects::default()
                },
            );
            input.set_options(
                root,
                NodeInput {
                    focusable: true,
                    disabled: !visible,
                    pass_through: input.options(root).is_some_and(|o| o.pass_through),
                    ..NodeInput::default()
                },
            );
            if visible {
                if input.focus_scope() != Some(root) && !input.push_focus_scope(&scene, root) {
                    value.set(false);
                }
            } else {
                while let Some(active) = input.focus_scope() {
                    let within =
                        active == root || scene.borrow().ancestors(active).any(|id| id == root);
                    if !within {
                        break;
                    }
                    input.pop_focus_scope(&scene);
                }
            }
        });
        Self {
            root,
            body,
            open,
            scene: Rc::downgrade(&ui.scene),
            centered: true,
        }
    }
    /// Refreshes the backdrop and optional panel centering after a viewport
    /// change. Bind this to the host viewport signal for live resize support.
    pub fn resize_to_viewport(&self) {
        let Some(scene) = self.scene.upgrade() else {
            return;
        };
        let mut scene = scene.borrow_mut();
        if !scene.contains(self.root) {
            return;
        }
        let viewport = scene.style(scene.root());
        let (width, height) = (
            viewport.width.unwrap_or(0.0),
            viewport.height.unwrap_or(0.0),
        );
        let body = scene.style(self.body);
        scene.set_style(self.root, fixed(width, height));
        let backdrop = scene.children(self.root)[0];
        scene.set_style(backdrop, fixed(width, height));
        if self.centered {
            scene.set_transform(
                self.body,
                Transform {
                    x: ((width - body.width.unwrap_or(0.0)) / 2.0).max(0.0),
                    y: ((height - body.height.unwrap_or(0.0)) / 2.0).max(0.0),
                },
            );
        }
    }
    /// Fit it to the viewport and open it: it takes focus and traps it.
    pub fn show(&self) {
        self.resize_to_viewport();
        self.open.set(true);
    }
    /// Close it; focus returns to where it was.
    pub fn close(&self) {
        self.open.set(false);
    }
}

/// Anchored modal popover. It uses the same focus and dismissal behavior as a
/// dialog; callers place menu item buttons in `body`. Position is refreshed on
/// `show` so a moved/scrolled anchor is followed, and clamped to the viewport.
#[derive(Clone)]
pub struct Popover {
    /// The modal it is built on; its `body` holds the items.
    pub dialog: Dialog,
    anchor: NodeId,
    width: f32,
    height: f32,
    items: Rc<RefCell<Vec<NodeId>>>,
}
impl Popover {
    /// Mount a closed `width` by `height` menu under `overlay_parent`, placed by
    /// `anchor` when shown. The up and down arrows, Home and End move focus between
    /// its items.
    pub fn mount(
        ui: &mut Ui,
        overlay_parent: NodeId,
        anchor: NodeId,
        width: f32,
        height: f32,
    ) -> Self {
        let mut dialog = Dialog::mount(ui, overlay_parent, "Menu", width, height, true);
        dialog.centered = false;
        ui.semantics
            .borrow_mut()
            .set(dialog.body, SemanticNode::new(Role::Menu, ""));
        let items: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let entries = items.clone();
        let scene = ui.scene.clone();
        let input = ui.input.clone();
        ui.on_event(dialog.body, false, move |cx| {
            if cx.phase == EventPhase::Capture || cx.default_prevented() {
                return;
            }
            let InputEvent::KeyDown { ref key, .. } = cx.event else {
                return;
            };
            let items = entries.borrow();
            if items.is_empty() {
                return;
            }
            let current = input
                .focused()
                .and_then(|id| items.iter().position(|node| *node == id));
            let (start, backward) = match key {
                Key::ArrowDown => (current.map_or(0, |i| (i + 1) % items.len()), false),
                Key::ArrowUp => (
                    current.map_or(items.len() - 1, |i| (i + items.len() - 1) % items.len()),
                    true,
                ),
                Key::Home => (0, false),
                Key::End => (items.len() - 1, true),
                _ => return,
            };
            for step in 0..items.len() {
                let index = if backward {
                    (start + items.len() - step) % items.len()
                } else {
                    (start + step) % items.len()
                };
                if input.focus(&scene, Some(items[index])) {
                    break;
                }
            }
            cx.prevent_default();
            cx.stop_propagation();
        });
        Self {
            dialog,
            anchor,
            width,
            height,
            items,
        }
    }
    /// Adds a keyboard-navigable menu item. Activation closes the menu before
    /// running the action, restoring focus without pointer click-through.
    pub fn item(
        &self,
        ui: &mut Ui,
        label: impl Into<String>,
        mut action: impl FnMut() + 'static,
    ) -> NodeId {
        let label = label.into();
        let open = self.dialog.open.clone();
        let node = ui.button(self.dialog.body, &label, self.width, move || {
            open.set(false);
            action();
        });
        ui.scene.borrow_mut().set_transform(
            node,
            Transform {
                x: 0.0,
                y: self.items.borrow().len() as f32
                    * ui.theme_scope()
                        .number_untracked(crate::theme::tok::CONTROL_HEIGHT),
            },
        );
        ui.semantics
            .borrow_mut()
            .set(node, SemanticNode::new(Role::MenuItem, label));
        self.items.borrow_mut().push(node);
        node
    }
    /// Place it below `anchor` (above when there is no room below), open it and
    /// focus its first item.
    pub fn show(&self, ui: &Ui) {
        self.show_with(&ui.scene, &ui.input);
    }
    /// Opens from an event handler without capturing the entire Ui owner.
    pub fn show_with(
        &self,
        scene_handle: &Rc<RefCell<crate::scene::Scene>>,
        input: &InputDispatcher,
    ) {
        self.dialog.resize_to_viewport();
        scene_handle.borrow_mut().prepare_layout();
        let mut scene = scene_handle.borrow_mut();
        if !scene.contains(self.anchor)
            || !scene.contains(self.dialog.root)
            || !scene.contains(self.dialog.body)
        {
            return;
        }
        let anchor = scene.bounds(self.anchor);
        let viewport = scene.bounds(scene.root());
        let parent = scene.bounds(self.dialog.root);
        let x = anchor
            .x
            .min((viewport.width - self.width).max(0.0))
            .max(0.0);
        let below = anchor.y + anchor.height;
        let y = if below + self.height <= viewport.height {
            below
        } else {
            (anchor.y - self.height).max(0.0)
        };
        scene.set_transform(
            self.dialog.body,
            Transform {
                x: x - parent.x,
                y: y - parent.y,
            },
        );
        drop(scene);
        self.dialog.show();
        for node in self.items.borrow().iter() {
            if input.focus(scene_handle, Some(*node)) {
                break;
            }
        }
    }
    /// Close it.
    pub fn close(&self) {
        self.dialog.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input::Modifiers, scene::Color};
    fn overlay_ui() -> Ui {
        let ui = Ui::new(400.0, 300.0);
        let root = ui.root();
        ui.scene
            .borrow_mut()
            .set_kind(root, NodeKind::Container(Layout::Overlay));
        ui
    }
    #[test]
    fn scroll_clamps_resizes_and_uses_compositor_only_updates() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let scroll = ScrollView::mount(&mut ui, root, 100.0, 100.0);
        ui.label(scroll.content, "content", fixed(100.0, 500.0));
        scroll.set_content_height(500.0);
        ui.scene.borrow_mut().flush();
        scroll.scroll_to(10_000.0);
        assert_eq!(scroll.offset.get(), 400.0);
        let report = ui.scene.borrow_mut().flush();
        assert_eq!(report.layout_nodes, 0);
        assert!(report.composite_nodes > 0);
        {
            let mut scene = ui.scene.borrow_mut();
            let mut style = scene.style(scroll.content);
            style.gap = 7.0;
            scene.set_style(scroll.content, style);
        }
        scroll.resize(100.0, 300.0);
        assert_eq!(ui.scene.borrow().style(scroll.content).gap, 7.0);
        assert_eq!(scroll.offset.get(), 200.0);
        scroll.scroll_to(f32::NAN);
        assert_eq!(scroll.offset.get(), 0.0);
        ui.dispatch(InputEvent::Scroll {
            x: 10.0,
            y: 10.0,
            delta_x: 0.0,
            delta_y: 50.0,
        });
        assert_eq!(scroll.offset.get(), 50.0);
    }
    #[test]
    fn virtual_rows_are_bounded_retain_overlap_and_dispose_on_unmount() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let list = VirtualListView::mount(
            &mut ui,
            root,
            200.0,
            100.0,
            1_000_000,
            20.0,
            1,
            |i| i,
            |index, _, scope| {
                let label = format!("row {index}");
                scope.text(
                    fixed(200.0, 20.0),
                    Color(255, 255, 255, 255),
                    16.0,
                    move || label.clone(),
                );
            },
        );
        assert_eq!(list.mounted_rows(), 6);
        let retained = list.row(&2).unwrap();
        list.scroll.scroll_to(20.0);
        assert_eq!(list.row(&2), Some(retained));
        assert!(list.mounted_rows() <= 7);
        list.scroll.scroll_to(19_999_000.0);
        assert!(list.row(&2).is_none());
        assert!(list.mounted_rows() <= 7);
        assert!(ui.scene.borrow().len() < 25);
        list.count.set(2);
        assert_eq!(list.scroll.offset.get(), 0.0);
        assert_eq!(list.mounted_rows(), 2);
        ui.remove(list.scroll.root);
        assert_eq!(ui.runtime.effect_count(), 0);
        assert_eq!(list.mounted_rows(), 0);
        assert_eq!(ui.scene.borrow().len(), 1);
    }
    fn click(ui: &mut Ui, x: f32, y: f32) {
        ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
    }
    #[test]
    fn dialog_traps_input_escape_restores_focus_and_backdrop_dismisses() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let count = ui.signal(0);
        let value = count.clone();
        let background = ui.button(root, "background", 100.0, move || {
            value.update(|n| *n += 1);
        });
        ui.input.focus(&ui.scene, Some(background));
        let dialog = Dialog::mount(&mut ui, root, "Dialog", 200.0, 100.0, true);
        let inside = ui.button(dialog.body, "inside", 100.0, || {});
        dialog.show();
        assert_eq!(ui.input.focus_scope(), Some(dialog.root));
        assert!(!ui.input.focus(&ui.scene, Some(background)));
        ui.input.focus(&ui.scene, Some(inside));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert!(!dialog.open.get());
        assert_eq!(ui.input.focused(), Some(background));
        dialog.show();
        click(&mut ui, 10.0, 10.0);
        assert!(!dialog.open.get());
        assert_eq!(count.get(), 0);
        click(&mut ui, 10.0, 10.0);
        assert_eq!(count.get(), 1);
    }
    #[test]
    fn dialog_refreshes_backdrop_and_center_after_resize() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let dialog = Dialog::mount(&mut ui, root, "resize", 200.0, 100.0, true);
        ui.scene.borrow_mut().resize(600.0, 500.0);
        dialog.show();
        ui.scene.borrow_mut().prepare_layout();
        assert_eq!(
            ui.scene.borrow().bounds(dialog.root),
            crate::scene::Rect::new(0.0, 0.0, 600.0, 500.0)
        );
        assert_eq!(
            ui.scene.borrow().bounds(dialog.body),
            crate::scene::Rect::new(200.0, 200.0, 200.0, 100.0)
        );
    }
    #[test]
    fn menu_arrows_skip_disabled_items_and_activation_restores_focus() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let anchor = ui.button(root, "menu", 80.0, || {});
        ui.input.focus(&ui.scene, Some(anchor));
        let popover = Popover::mount(&mut ui, root, anchor, 120.0, 120.0);
        let a = popover.item(&mut ui, "first", || {});
        let b = popover.item(&mut ui, "disabled", || {});
        let selected = ui.signal(false);
        let value = selected.clone();
        let c = popover.item(&mut ui, "last", move || {
            value.set(true);
        });
        ui.set_disabled(b, true);
        popover.show(&ui);
        assert_eq!(ui.input.focused(), Some(a));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::ArrowDown,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert_eq!(ui.input.focused(), Some(c));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.dispatch(InputEvent::KeyUp {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        });
        assert!(selected.get());
        assert!(!popover.dialog.open.get());
        assert_eq!(ui.input.focused(), Some(anchor));
    }
    #[test]
    fn closing_outer_dialog_unwinds_nested_focus_scopes() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let anchor = ui.button(root, "open", 80.0, || {});
        ui.input.focus(&ui.scene, Some(anchor));
        let outer = Dialog::mount(&mut ui, root, "outer", 200.0, 150.0, true);
        outer.show();
        let inner = Dialog::mount(&mut ui, outer.root, "inner", 100.0, 100.0, true);
        inner.show();
        assert_eq!(ui.input.focus_scope(), Some(inner.root));
        outer.close();
        assert_eq!(ui.input.focus_scope(), None);
        assert!(!inner.open.get());
        assert_eq!(ui.input.focused(), Some(anchor));
    }
    #[test]
    fn panel_blank_space_does_not_dismiss_and_popover_flips_above_anchor() {
        let mut ui = overlay_ui();
        let root = ui.root();
        let anchor = ui.button(root, "menu", 80.0, || {});
        ui.scene
            .borrow_mut()
            .set_transform(anchor, Transform { x: 350.0, y: 260.0 });
        let popover = Popover::mount(&mut ui, root, anchor, 100.0, 100.0);
        popover.show(&ui);
        ui.scene.borrow_mut().prepare_layout();
        let bounds = ui.scene.borrow().bounds(popover.dialog.body);
        assert_eq!(bounds.x, 300.0);
        assert_eq!(bounds.y, 160.0);
        click(&mut ui, 350.0, 200.0);
        assert!(popover.dialog.open.get());
        popover.close();
    }
}
