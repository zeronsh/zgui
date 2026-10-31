//! Tabs choosing a panel, as WAI-ARIA's tabs pattern and an editor's tab
//! strip: one Tab stop, the arrows moving among the tabs (the choice
//! following), Home and End to the ends; closable tabs close with their ×,
//! Delete, or a middle click; reorderable ones drag along the strip or move
//! with ⌘ (Ctrl) Shift and an arrow, and with `on_tear_off` tear off into
//! windows of their own; a strip too long for its width scrolls, and a menu
//! at its end lists every tab.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};
use zgx::{
    compose::{
        Context, NodeHandle, View, button, column, component, div, keyed, menu, menu_item, row,
        scroll_x, switch, text, text_signal,
    },
    input::{DragEvent, DragPhase, EventPhase, FocusGroup, InputEvent, Key, PointerButton},
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

/// Told a tab went to another strip of its group: its id, and that strip's
/// label.
pub type MoveTab = Rc<dyn Fn(&str, &str)>;

/// Told a tab was released outside every window of the app: its id, and
/// where (the drag's `x`, `y`, `grab_x` and `grab_y`), to open it in a
/// window of its own under the pointer. `true` takes it: it leaves the
/// strip. `false` keeps it, its preview sliding home.
pub type TearOffTab = Rc<dyn Fn(&str, &DragEvent) -> bool>;

/// How [`tabs`] behave beyond choosing.
#[derive(Clone, Default)]
pub struct TabsOptions {
    /// Tabs drag along the strip, and move with ⌘ (Ctrl) Shift and an arrow.
    pub reorderable: bool,
    /// Asked before a tab closes; without it, it closes.
    pub on_close: Option<CloseTab>,
}

/// More of how [`tabs_with`] behave: kept panels, groups of strips, tabs
/// torn off. Made from `Default` and its fields set, as more may come.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct TabsMore {
    /// Each tab's panel stays mounted once chosen, until its tab closes:
    /// hidden, it keeps its state (an editor's undo, caret and scroll) and
    /// takes no layout, paint or frames.
    pub keep: bool,
    /// Strips of one group take each other's tabs: a tab dragged over
    /// another strip or its panel goes there, as an editor's split panes do.
    /// Reorderable strips only.
    pub group: Option<String>,
    /// Told a tab of this strip went to another of its group.
    pub on_move: Option<MoveTab>,
    /// Tabs tear off: one dragged off the strip is carried as a drag, to a
    /// strip of its group in any of the app's windows (`on_move` told), or
    /// out of every window, where it leaves the strip and this is told (the
    /// host telling where it can: macOS and X11). Reorderable strips only.
    pub on_tear_off: Option<TearOffTab>,
}

/// How far past its strip a dragged tab goes to tear off.
const TEAR: f32 = 24.;

/// A tab torn off its strip, in flight.
struct TabDrag {
    tab: Tab,
    group: Option<String>,
    /// The strip it came from: dropped back there, it stays.
    home: Rc<()>,
    /// The label of the strip of its group it was dropped on.
    landed: RefCell<Option<String>>,
}

/// A strip of a group: what another's dragged tab needs of it.
struct Strip {
    group: String,
    label: String,
    /// Which window it is in: a strip takes tabs from its own.
    scene: Cell<usize>,
    bounds: Signal<Rect>,
    items: Signal<Vec<Tab>>,
    selected: Signal<String>,
    /// A tab is over it.
    over: Signal<bool>,
    /// Where this strip's dragged tab would go.
    target: RefCell<Option<Rc<Strip>>>,
}
thread_local! {
    /// The strips of every group.
    static STRIPS: RefCell<Vec<Weak<Strip>>> = const { RefCell::new(Vec::new()) };
}
impl Strip {
    /// Follow the pointer at `x`, `y`: whether it is over another strip of
    /// the group.
    fn aim(self: &Rc<Self>, x: f32, y: f32) -> bool {
        let found = STRIPS.with_borrow_mut(|strips| {
            strips.retain(|strip| strip.strong_count() > 0);
            strips.iter().filter_map(Weak::upgrade).find(|strip| {
                let b = strip.bounds.with_untracked(|b| *b);
                !Rc::ptr_eq(strip, self)
                    && strip.group == self.group
                    && strip.scene.get() == self.scene.get()
                    && (b.x..b.x + b.width).contains(&x)
                    && (b.y..b.y + b.height).contains(&y)
            })
        });
        let aimed = found.is_some();
        if let Some(was) = self.target.replace(found.clone()) {
            was.over.set(false);
        }
        if let Some(now) = found {
            now.over.set(true);
        }
        aimed
    }
    /// The drag ended: `id` goes where it was aimed, if `drop`; the strip
    /// it went to.
    fn land(&self, id: &str, drop: bool) -> Option<Rc<Strip>> {
        let to = self.target.take()?;
        to.over.set(false);
        if !drop {
            return None;
        }
        let tab = take(&self.items, &self.selected, id)?;
        give(&to.items, &to.selected, tab);
        Some(to)
    }
}

/// Take `id` out of `items`, its neighbour chosen if it was: the tab.
fn take(items: &Signal<Vec<Tab>>, selected: &Signal<String>, id: &str) -> Option<Tab> {
    let mut list = items.with_untracked(Clone::clone);
    let at = list.iter().position(|t| t.id == id)?;
    let tab = list.remove(at);
    if selected.with_untracked(|s| s == id) {
        let next = list.get(at).or_else(|| list.get(at.wrapping_sub(1)));
        selected.set(next.map_or(String::new(), |t| t.id.clone()));
    }
    items.set(list);
    Some(tab)
}
/// Give `tab` to `items`, chosen.
fn give(items: &Signal<Vec<Tab>>, selected: &Signal<String>, tab: Tab) {
    let id = tab.id.clone();
    if !items.with_untracked(|l| l.iter().any(|t| t.id == id)) {
        items.update(|l| l.push(tab));
    }
    selected.set(id);
}
/// A torn tab under the pointer.
fn chip(label: &str) -> View {
    row()
        .px(12.)
        .py(6.)
        .rounded(6.)
        .bg(tok::SURFACE_RAISED)
        .text_color(tok::TEXT_PRIMARY)
        .child(text(label))
}

/// The ids in `items`, in order.
fn ids(items: &[Tab]) -> Vec<String> {
    items.iter().map(|t| t.id.clone()).collect()
}

/// A strip of `items` and the panel `panel` builds for the one `selected`
/// (its id). `items` is two-way: closing a tab removes it, reordering
/// moves it; closing the selected tab chooses its neighbour. The panel is
/// built as its tab is chosen and dropped as another is (`TabsMore::keep`
/// holds it);
/// `label` names the strip.
pub fn tabs(
    label: impl Into<String>,
    items: Signal<Vec<Tab>>,
    selected: Signal<String>,
    options: TabsOptions,
    panel: impl FnMut(String, &mut Context) -> View + 'static,
) -> View {
    tabs_with(label, items, selected, options, TabsMore::default(), panel)
}

/// [`tabs`], with `more`: kept panels, a group, tear-off.
pub fn tabs_with(
    label: impl Into<String>,
    items: Signal<Vec<Tab>>,
    selected: Signal<String>,
    options: TabsOptions,
    more: TabsMore,
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
        let bounds = runtime.signal(Rect::new(0., 0., 0., 0.));
        let over = runtime.signal(false);
        // This strip, as a torn tab knows where it came from.
        let home: Rc<()> = Rc::new(());
        let strip_of_group = more.group.clone().map(|group| {
            let strip = Rc::new(Strip {
                group,
                label: label.clone(),
                scene: Cell::new(0),
                bounds: bounds.clone(),
                items: items.clone(),
                selected: selected.clone(),
                over: over.clone(),
                target: RefCell::default(),
            });
            STRIPS.with_borrow_mut(|strips| strips.push(Rc::downgrade(&strip)));
            cx.retain(strip.clone());
            strip
        });

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
            let (grouped, on_move) = (strip_of_group.clone(), more.on_move.clone());
            let tear = more
                .on_tear_off
                .clone()
                .filter(|_| reorderable)
                .map(|told| (told, home.clone(), more.group.clone()));
            let strip_bounds = strip_bounds.clone();
            move |id: String, _: &mut Context| {
                let (grouped, on_move) = (grouped.clone(), on_move.clone());
                let (tear, strip_bounds) = (tear.clone(), strip_bounds.clone());
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
                let drag: Rc<Cell<Option<(f32, f32, bool)>>> = Rc::default();
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
                // Torn off: dropped on a strip of its group it went there;
                // out of every window, it leaves.
                if let Some((told, _, _)) = &tear {
                    let (moved, gone) = (
                        (items.clone(), selected.clone(), on_move.clone()),
                        (items.clone(), selected.clone(), told.clone()),
                    );
                    tab = tab
                        .drag_preview(|torn: &TabDrag| chip(&torn.tab.label))
                        .on_drag_end(move |torn: &TabDrag, accepted| {
                            let (items, selected, on_move) = &moved;
                            let landed = torn.landed.borrow().clone();
                            if let (true, Some(to)) = (accepted, landed) {
                                take(items, selected, &torn.tab.id);
                                if let Some(told) = on_move {
                                    told(&torn.tab.id, &to);
                                }
                            }
                        })
                        // Out of every window: torn off if the app takes it
                        // (its preview goes), else it stays and slides home.
                        .on_event(move |cx| {
                            if cx.phase != EventPhase::Target {
                                return;
                            }
                            let InputEvent::Drag(event) = &cx.event else {
                                return;
                            };
                            if event.phase != DragPhase::End || !event.outside {
                                return;
                            }
                            let event = event.clone();
                            let Some(torn) = event.payload.downcast_ref::<TabDrag>() else {
                                return;
                            };
                            let (items, selected, told) = &gone;
                            if told(&torn.tab.id, &event) {
                                cx.prevent_default();
                                take(items, selected, &torn.tab.id);
                            }
                        });
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
                            y,
                            button: PointerButton::Primary,
                        } if reorderable => {
                            drag.set(Some((*x, *y, false)));
                            DRAGGED.with(|d| d.set(false));
                            cx.capture_pointer();
                        }
                        InputEvent::PointerMove { x, y, .. } => {
                            let Some((start, start_y, moved)) = drag.get() else {
                                return;
                            };
                            let off = tear.is_some() && (y - start_y).abs() >= 4.;
                            if !moved && (x - start).abs() < 4. && !off {
                                return;
                            }
                            drag.set(Some((start, start_y, true)));
                            DRAGGED.with(|d| d.set(true));
                            if grouped.as_ref().is_some_and(|strip| strip.aim(*x, *y)) {
                                return;
                            }
                            // Off the strip, it tears off: carried as a drag.
                            if let Some((_, home, group)) = &tear {
                                let b = strip_bounds.with_untracked(|b| *b);
                                if *y < b.y - TEAR || *y > b.y + b.height + TEAR {
                                    if let Some(strip) = &grouped {
                                        strip.land(&id_focused, false);
                                    }
                                    drag.set(None);
                                    let tab = items.with_untracked(|l| {
                                        l.iter().find(|t| t.id == id_focused).cloned()
                                    });
                                    if let Some(tab) = tab {
                                        let torn = TabDrag {
                                            tab,
                                            group: group.clone(),
                                            home: home.clone(),
                                            landed: RefCell::default(),
                                        };
                                        cx.start_drag_from(torn, start, start_y);
                                    }
                                    return;
                                }
                            }
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
                            let drop = matches!(cx.event, InputEvent::PointerUp { .. });
                            cx.release_pointer();
                            let landed = grouped
                                .as_ref()
                                .and_then(|strip| strip.land(&id_focused, drop));
                            if let (Some(to), Some(told)) = (landed, &on_move) {
                                told(&id_focused, &to.label);
                            }
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
        let all_tabs = switch(overflowing, move |over, _| {
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

        // What a torn tab dropped here needs.
        let torn_to = (items.clone(), selected.clone(), over.clone(), label.clone());
        let (panel_selected, mut panel) = (selected.clone(), panel);
        // The panel's root lays out as a child of the tab panel, so `grow`
        // and `h_full` fill it.
        let panels = if more.keep {
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
        let tabs_row = row().items_center().child(strip).child(all_tabs);
        let tabs_row = match &strip_of_group {
            // Lit while a tab of its group is over it.
            Some(_) => tabs_row.reactive_style(move || {
                if over.get() {
                    Styles::new().bg(tok::SELECTION)
                } else {
                    Styles::new().bg(Color(0, 0, 0, 0))
                }
            }),
            None => tabs_row,
        };
        let root = column();
        // A torn tab of its group (or its own, back home) drops here.
        let takes_torn = more.on_tear_off.is_some() || more.group.is_some();
        let root = if takes_torn {
            let (items, selected, over, label) = torn_to;
            // Another strip's tab of its group, or its own back home.
            let accepts = {
                let (group, home) = (more.group.clone(), home.clone());
                Rc::new(move |torn: &TabDrag| {
                    Rc::ptr_eq(&torn.home, &home) || (torn.group.is_some() && torn.group == group)
                })
            };
            let (accepted, lit, own) = (accepts.clone(), over.clone(), home.clone());
            root.on_drop_when(
                move |torn: &TabDrag| accepts(torn),
                move |torn: &TabDrag, _| {
                    over.set(false);
                    if !Rc::ptr_eq(&torn.home, &home) {
                        give(&items, &selected, torn.tab.clone());
                        *torn.landed.borrow_mut() = Some(label.clone());
                    }
                },
            )
            // Lit while another strip's tab is over it.
            .on_event(move |cx| {
                let InputEvent::Drag(event) = &cx.event else {
                    return;
                };
                let Some(torn) = event.payload.downcast_ref::<TabDrag>() else {
                    return;
                };
                match event.phase {
                    DragPhase::Over => {
                        lit.set(accepted(torn) && !Rc::ptr_eq(&torn.home, &own));
                    }
                    DragPhase::Leave | DragPhase::End => {
                        lit.set(false);
                    }
                    _ => {}
                }
            })
        } else {
            root
        };
        root.on_mount({
            let (strip, runtime) = (strip_of_group, runtime.clone());
            move |handle| {
                if let Some(strip) = strip {
                    follow(&runtime, &bounds, &handle);
                    let scene = handle.scene().map_or(0, |s| Rc::as_ptr(&s) as usize);
                    strip.scene.set(scene);
                }
            }
        })
        .child(tabs_row)
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
