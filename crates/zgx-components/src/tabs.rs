//! Tabs choosing a panel, as WAI-ARIA's tabs pattern and an editor's tab
//! strip: one Tab stop, the arrows moving among the tabs (the choice
//! following), Home and End to the ends; closable tabs close with their ×,
//! Delete, or a middle click; reorderable ones drag along the strip or move
//! with ⌘ (Ctrl) Shift and an arrow; a strip too long for its width scrolls,
//! and a menu at its end lists every tab.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use zgx::{
    compose::{
        Context, NodeHandle, View, button, column, component, div, keyed, menu, menu_item, row,
        scroll_x, switch, text, text_signal,
    },
    input::{EventPhase, FocusGroup, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Color, Direction, Rect},
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};

/// A tab of [`tabs`]: its id (unique among them), what it shows, and
/// whether it closes.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// Unique among the tabs.
    pub id: String,
    /// What it shows.
    pub label: String,
    /// A × closes it (and Delete, and a middle click).
    pub closable: bool,
}
impl Tab {
    /// A tab that doesn't close.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            closable: false,
        }
    }
    /// One that closes.
    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }
}

/// What a closed tab asks first: `true` closes it (an editor asking to save
/// first returns `false` and closes it itself later).
pub type CloseTab = Rc<dyn Fn(&str) -> bool>;

/// How [`tabs`] behave beyond choosing.
#[derive(Clone, Default)]
pub struct TabsOptions {
    /// Tabs drag along the strip, and move with ⌘ (Ctrl) Shift and an arrow.
    pub reorderable: bool,
    /// Asked before a tab closes; without it, it closes.
    pub on_close: Option<CloseTab>,
    /// Each tab's panel stays mounted once chosen, until its tab closes:
    /// hidden, it keeps its state (an editor's undo, caret and scroll) and
    /// takes no layout, paint or frames.
    pub keep: bool,
}

/// The ids in `items`, in order.
fn ids(items: &[Tab]) -> Vec<String> {
    items.iter().map(|t| t.id.clone()).collect()
}

/// A strip of `items` and the panel `panel` builds for the one `selected`
/// (its id). `items` is two-way: closing a tab removes it, reordering
/// moves it; closing the selected tab chooses its neighbour. The panel is
/// built as its tab is chosen and dropped as another is (`keep` holds it);
/// `label` names the strip.
pub fn tabs(
    label: impl Into<String>,
    items: Signal<Vec<Tab>>,
    selected: Signal<String>,
    options: TabsOptions,
    panel: impl FnMut(String, &mut Context) -> View + 'static,
) -> View {
    let label = label.into();
    component(move |cx| {
        // A label that changes (a translation, a state), where the caller
        // gave one (`FollowedLabel::around`).
        let followed = zgx::compose::FollowedLabel::take(cx);
        let runtime = cx.runtime();
        // Each tab's node, for dragging, scrolling into view and focus.
        let nodes: Rc<RefCell<HashMap<String, NodeHandle>>> = Rc::default();
        let offset = runtime.signal(0_f32);
        let viewport: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
        let strip_bounds = runtime.signal(Rect::new(0., 0., 0., 0.));
        let content_bounds = runtime.signal(Rect::new(0., 0., 0., 0.));
        let overflow_open = runtime.signal(false);

        // Close `id`: asked first, then gone, its neighbour chosen if it was.
        let close = {
            let (items, selected, on_close) =
                (items.clone(), selected.clone(), options.on_close.clone());
            Rc::new(move |id: &str| {
                if on_close.as_ref().is_some_and(|ask| !ask(id)) {
                    return;
                }
                let list = items.with_untracked(Clone::clone);
                let Some(at) = list.iter().position(|t| t.id == id) else {
                    return;
                };
                let mut next = list.clone();
                next.remove(at);
                if selected.with_untracked(|s| s == id)
                    && let Some(neighbour) = next.get(at).or_else(|| next.get(at.wrapping_sub(1)))
                {
                    selected.set(neighbour.id.clone());
                }
                items.set(next);
            })
        };
        // Bring `id`'s tab into the strip's view.
        let reveal = {
            let (nodes, viewport, offset) = (nodes.clone(), viewport.clone(), offset.clone());
            Rc::new(move |id: &str| {
                let (Some(tab), Some(view)) =
                    (nodes.borrow().get(id).cloned(), viewport.borrow().clone())
                else {
                    return;
                };
                let Some(scene) = tab.scene() else { return };
                let (b, v) = {
                    let scene = scene.borrow();
                    (scene.bounds(tab.node()), scene.bounds(view.node()))
                };
                let now = offset.with_untracked(|o| *o);
                if b.x < v.x {
                    offset.set((now - (v.x - b.x)).max(0.));
                } else if b.x + b.width > v.x + v.width {
                    offset.set(now + (b.x + b.width - v.x - v.width));
                }
            })
        };
        // Move `id` to `to` among the tabs.
        let move_to = {
            let items = items.clone();
            Rc::new(move |id: &str, to: usize| {
                let mut list = items.with_untracked(Clone::clone);
                let Some(from) = list.iter().position(|t| t.id == id) else {
                    return;
                };
                let to = to.min(list.len() - 1);
                if from != to {
                    let tab = list.remove(from);
                    list.insert(to, tab);
                    items.set(list);
                }
            })
        };

        let tab_view = {
            let (items, selected, nodes) = (items.clone(), selected.clone(), nodes.clone());
            let (close, move_to, reveal) = (close.clone(), move_to.clone(), reveal.clone());
            let reorderable = options.reorderable;
            move |id: String, _: &mut Context| {
                let lookup = {
                    let items = items.clone();
                    let id = id.clone();
                    move || items.with(|list| list.iter().find(|t| t.id == id).cloned())
                };
                let (chosen, styled, clicked, focused) = (
                    selected.clone(),
                    selected.clone(),
                    selected.clone(),
                    selected.clone(),
                );
                let (id_chosen, id_styled, id_clicked, id_focused) =
                    (id.clone(), id.clone(), id.clone(), id.clone());
                let closable = lookup().is_some_and(|t| t.closable);
                let (shown, named) = (lookup.clone(), lookup.clone());
                // A drag along the strip: where it started, and whether it moved.
                let drag: Rc<Cell<Option<(f32, bool)>>> = Rc::default();
                let mut tab = button()
                    .role(Role::Tab)
                    .accessible_label_when(move || named().map(|t| t.label).unwrap_or_default())
                    .selected_when(move || chosen.with(|s| *s == id_chosen))
                    .px(12.)
                    .py(6.)
                    .gap(6.)
                    .rounded(6.)
                    .border(0.)
                    .shrink_0()
                    .reactive_style(move || {
                        if styled.with(|s| *s == id_styled) {
                            Styles::new()
                                .bg(tok::SURFACE_RAISED)
                                .text_color(tok::TEXT_PRIMARY)
                        } else {
                            Styles::new()
                                .bg(Color(0, 0, 0, 0))
                                .text_color(tok::TEXT_SECONDARY)
                        }
                    })
                    .hover(|s| s.text_color(tok::TEXT_PRIMARY))
                    .on_click(move || {
                        if not_a_drag() {
                            clicked.set(id_clicked.clone());
                        }
                    })
                    .on_mount({
                        let (nodes, id) = (nodes.clone(), id.clone());
                        move |handle| {
                            nodes.borrow_mut().insert(id, handle);
                        }
                    })
                    .child(text_signal(move || {
                        shown().map(|t| t.label).unwrap_or_default()
                    }));
                if closable {
                    let (close, id) = (close.clone(), id.clone());
                    let name = lookup.clone();
                    tab = tab.child(
                        button()
                            .focusable(false)
                            .accessible_label(format!(
                                "Close {}",
                                name().map(|t| t.label).unwrap_or_default()
                            ))
                            .w(16.)
                            .h(16.)
                            .p(0.)
                            .rounded(4.)
                            .border(0.)
                            .bg(Color(0, 0, 0, 0))
                            .hover(|s| s.bg(tok::HOVER))
                            .items_center()
                            .justify_center()
                            .on_click(move || close(&id))
                            .on_event(|cx| {
                                if cx.phase != EventPhase::Capture
                                    && matches!(cx.event, InputEvent::PointerDown { .. })
                                {
                                    cx.stop_propagation();
                                }
                            })
                            .child(text("×").text_size(12.).text_color(tok::TEXT_TERTIARY)),
                    );
                }
                let (close, move_to, reveal, nodes, items) = (
                    close.clone(),
                    move_to.clone(),
                    reveal.clone(),
                    nodes.clone(),
                    items.clone(),
                );
                tab.on_event(move |cx| {
                    if cx.phase == EventPhase::Capture {
                        return;
                    }
                    // Focus is the tab's own; a press may be on its text (its
                    // × keeps its presses to itself).
                    let on_tab = cx.target == cx.current_target;
                    if matches!(cx.event, InputEvent::Focus | InputEvent::KeyDown { .. }) && !on_tab
                    {
                        return;
                    }
                    let index =
                        || items.with_untracked(|l| l.iter().position(|t| t.id == id_focused));
                    match &cx.event {
                        // Selection follows focus, as WAI-ARIA's automatic tabs.
                        InputEvent::Focus => {
                            if focused.with_untracked(|s| *s != id_focused) {
                                focused.set(id_focused.clone());
                            }
                            reveal(&id_focused);
                        }
                        InputEvent::KeyDown { key, modifiers, .. } => {
                            let rtl = nodes
                                .borrow()
                                .get(&id_focused)
                                .and_then(NodeHandle::scene)
                                .is_some_and(|scene| {
                                    scene.borrow().direction(cx.target) == Direction::Rtl
                                });
                            let command = modifiers.meta || modifiers.control;
                            match key {
                                Key::Delete if closable => {
                                    close(&id_focused);
                                    cx.prevent_default();
                                    cx.stop_propagation();
                                }
                                Key::ArrowLeft | Key::ArrowRight
                                    if reorderable && command && modifiers.shift =>
                                {
                                    let Some(at) = index() else { return };
                                    let toward_end = (*key == Key::ArrowRight) != rtl;
                                    let to = if toward_end {
                                        at + 1
                                    } else {
                                        at.saturating_sub(1)
                                    };
                                    move_to(&id_focused, to);
                                    cx.prevent_default();
                                    cx.stop_propagation();
                                }
                                _ => {}
                            }
                        }
                        InputEvent::PointerDown {
                            button: PointerButton::Middle,
                            ..
                        } if closable => {
                            close(&id_focused);
                            cx.stop_propagation();
                        }
                        InputEvent::PointerDown {
                            x,
                            button: PointerButton::Primary,
                            ..
                        } if reorderable => {
                            drag.set(Some((*x, false)));
                            DRAGGED.with(|d| d.set(false));
                            cx.capture_pointer();
                        }
                        InputEvent::PointerMove { x, .. } => {
                            let Some((start, moved)) = drag.get() else {
                                return;
                            };
                            if !moved && (x - start).abs() < 4. {
                                return;
                            }
                            drag.set(Some((start, true)));
                            DRAGGED.with(|d| d.set(true));
                            // Past a neighbour's middle, it takes its place.
                            let order = items.with_untracked(|l| ids(l));
                            let nodes = nodes.borrow();
                            let middles: Vec<(String, f32)> = order
                                .iter()
                                .filter_map(|tab| {
                                    let handle = nodes.get(tab)?;
                                    let scene = handle.scene()?;
                                    let b = scene.borrow().bounds(handle.node());
                                    Some((tab.clone(), b.x + b.width / 2.))
                                })
                                .collect();
                            drop(nodes);
                            let Some(at) = index() else { return };
                            let rtl = middles.len() > 1 && middles[0].1 > middles[1].1;
                            let to = middles
                                .iter()
                                .filter(|(tab, mid)| {
                                    *tab != id_focused && if rtl { *x < *mid } else { *x > *mid }
                                })
                                .count();
                            if to != at {
                                move_to(&id_focused, to);
                            }
                        }
                        InputEvent::PointerUp {
                            button: PointerButton::Primary,
                            ..
                        }
                        | InputEvent::PointerCancel
                            if drag.take().is_some() =>
                        {
                            cx.release_pointer();
                        }
                        _ => {}
                    }
                })
            }
        };

        let strip_items = items.clone();
        let list = row()
            .shrink_0()
            .role(Role::TabList)
            .accessible_label_or(label.clone(), followed.as_ref())
            .focus_group(FocusGroup::Horizontal)
            .gap(2.)
            .p(2.)
            .on_mount({
                let (content_bounds, runtime) = (content_bounds.clone(), runtime.clone());
                move |handle| follow(&runtime, &content_bounds, &handle)
            })
            // The tabs lie in a row in the keyed region.
            .child(
                keyed(move || strip_items.with(|l| ids(l)), tab_view)
                    .flex_row()
                    .gap(2.)
                    .shrink_0(),
            );
        let strip = scroll_x(offset)
            .grow()
            .min_w(0.)
            .on_mount({
                let (viewport, strip_bounds) = (viewport.clone(), strip_bounds.clone());
                let runtime = runtime.clone();
                move |handle| {
                    follow(&runtime, &strip_bounds, &handle);
                    *viewport.borrow_mut() = Some(handle);
                }
            })
            .child(list);
        // Too many to show: a menu of them all at the strip's end.
        let overflowing = {
            let (strip, content) = (strip_bounds.clone(), content_bounds.clone());
            move || content.get().width > strip.get().width + 0.5
        };
        let (menu_items, menu_selected, menu_reveal) =
            (items.clone(), selected.clone(), reveal.clone());
        let more = switch(overflowing, move |over, _| {
            if !over {
                return div();
            }
            let mut list = menu(
                "All tabs",
                overflow_open.clone(),
                button()
                    .accessible_label("All tabs")
                    .px(8.)
                    .py(4.)
                    .border(0.)
                    .bg(Color(0, 0, 0, 0))
                    .child(text("⌄").text_color(tok::TEXT_SECONDARY)),
            );
            for tab in menu_items.with(|l| l.clone()) {
                let (checked, chosen, reveal) = (
                    menu_selected.clone(),
                    menu_selected.clone(),
                    menu_reveal.clone(),
                );
                let (id, id2) = (tab.id.clone(), tab.id.clone());
                list = list.child(
                    menu_item(tab.label)
                        .menu_radio()
                        .menu_checked_when(move || checked.with(|s| *s == id))
                        .on_click(move || {
                            chosen.set(id2.clone());
                            reveal(&id2);
                        }),
                );
            }
            list
        });

        let (panel_selected, mut panel) = (selected.clone(), panel);
        // The panel's root lays out as a child of the tab panel, so `grow`
        // and `h_full` fill it.
        let panels = if options.keep {
            // The tabs chosen so far, while they are open.
            let mut chosen: Vec<String> = Vec::new();
            let shown = selected.clone();
            keyed(
                move || {
                    let open = items.with(|items| ids(items));
                    chosen.retain(|id| open.contains(id));
                    let now = panel_selected.get();
                    if open.contains(&now) && !chosen.contains(&now) {
                        chosen.push(now);
                    }
                    chosen.clone()
                },
                move |id: String, cx| {
                    let (shown, own) = (shown.clone(), id.clone());
                    column()
                        .grow()
                        .reactive_style(move || {
                            if shown.with(|s| *s == own) {
                                Styles::new().flex()
                            } else {
                                Styles::new().hidden()
                            }
                        })
                        .child(panel(id, cx))
                },
            )
            .flex_col()
            .items_stretch()
            .grow()
        } else {
            switch(move || panel_selected.get(), move |id, cx| panel(id, cx)).contents()
        };
        column()
            .child(row().items_center().child(strip).child(more))
            .child(
                div()
                    .h(1.)
                    .shrink_0()
                    .bg(tok::BORDER_SUBTLE)
                    .accessible_hidden(),
            )
            .child(div().role(Role::TabPanel).grow().child(panels))
    })
}

thread_local! {
    /// Whether the last press on a tab dragged it: its click then chooses
    /// nothing more (the drag already moved it).
    static DRAGGED: Cell<bool> = const { Cell::new(false) };
}
/// Whether a click on a tab chooses it: not at a drag's end.
fn not_a_drag() -> bool {
    !DRAGGED.with(|d| d.replace(false))
}
/// Keep `into` equal to `handle`'s node's bounds, for as long as the node is.
fn follow(runtime: &zgx::reactive::Runtime, into: &Signal<Rect>, handle: &NodeHandle) {
    let Some(from) = handle.observe_bounds() else {
        return;
    };
    let into = into.clone();
    handle.retain(runtime.effect(move || {
        into.set(from.get());
    }));
}
