//! A tree view: items in a hierarchy, opened and closed, as WAI-ARIA's tree
//! and Finder's sidebar. The tree is one Tab stop and keeps focus, its
//! current item the active descendant: Up and Down move, Right opens an
//! item and then enters it, Left closes it and then goes to its parent
//! (mirrored right to left), Home and End go to the ends, typing goes to
//! the next item starting with it, Enter activates; the choice follows the
//! current item. Children come from the app as each item opens (loaded
//! then, if it likes: "loading" until they come); the rows are virtual, so
//! a tree of thousands builds only what shows.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};
use zgui::{
    compose::{Context, NodeHandle, View, component, div, text, text_signal, virtual_list},
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Color, Direction},
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};

/// An item of a [`tree`]: its id (unique in the tree), what it shows, and
/// whether it has children (shown with a chevron, which opens it).
#[derive(Clone, Debug, PartialEq)]
pub struct TreeItem {
    /// Unique in the tree.
    pub id: String,
    /// What it shows.
    pub label: String,
    /// It opens: its children come from `children` as it does.
    pub parent: bool,
}
impl TreeItem {
    /// A leaf showing `label`.
    pub fn leaf(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            parent: false,
        }
    }
    /// An item with children.
    pub fn parent(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            parent: true,
        }
    }
}

/// What `children` answers for an item: its children, or not yet (they
/// are loading: the tree shows "Loading…" until it answers again).
pub enum TreeChildren {
    /// These, in order.
    Ready(Vec<TreeItem>),
    /// Still coming.
    Loading,
}

/// A row of the tree as it shows.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    item: TreeItem,
    /// 1 at the roots.
    level: usize,
    open: bool,
    /// Its place among its siblings, from 1, and how many there are.
    position: usize,
    siblings: usize,
    parent: Option<String>,
    /// A "Loading…" row standing for children still coming.
    loading: bool,
}

/// The rows showing: the roots, and the children of each open item.
fn rows(children: &dyn Fn(Option<&str>) -> TreeChildren, open: &HashSet<String>) -> Vec<Row> {
    fn walk(
        children: &dyn Fn(Option<&str>) -> TreeChildren,
        open: &HashSet<String>,
        parent: Option<&str>,
        level: usize,
        out: &mut Vec<Row>,
    ) {
        match children(parent) {
            TreeChildren::Loading => out.push(Row {
                item: TreeItem::leaf(format!("{}\u{0}loading", parent.unwrap_or("")), "Loading…"),
                level,
                open: false,
                position: 1,
                siblings: 1,
                parent: parent.map(str::to_owned),
                loading: true,
            }),
            TreeChildren::Ready(items) => {
                let n = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    let is_open = item.parent && open.contains(&item.id);
                    let id = item.id.clone();
                    out.push(Row {
                        item,
                        level,
                        open: is_open,
                        position: i + 1,
                        siblings: n,
                        parent: parent.map(str::to_owned),
                        loading: false,
                    });
                    if is_open {
                        walk(children, open, Some(&id), level + 1, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(children, open, None, 1, &mut out);
    out
}

/// A row's height, and how far each level indents.
const ROW: f32 = 26.;
const INDENT: f32 = 16.;

/// A tree of what `children` gives for each item (`None`: the roots), its
/// open items' ids in `open` and the chosen one in `selected`, both
/// two-way. `activate` runs on Enter or a double click. `children` is read
/// as the tree shows, and followed (a signal it reads changing shows the
/// change); `label` names it.
pub fn tree(
    label: impl Into<String>,
    children: impl Fn(Option<&str>) -> TreeChildren + 'static,
    open: Signal<Vec<String>>,
    selected: Signal<Option<String>>,
    activate: impl Fn(&str) + 'static,
) -> View {
    let label = label.into();
    let (children, activate) = (Rc::new(children), Rc::new(activate));
    component(move |cx| {
        let runtime = cx.runtime();
        let shown = runtime.signal(Vec::<Row>::new());
        {
            let (children, open, shown) = (children.clone(), open.clone(), shown.clone());
            cx.retain(runtime.effect(move || {
                let open: HashSet<String> = open.get().into_iter().collect();
                shown.set(rows(&*children, &open));
            }));
        }
        let offset = runtime.signal(0_f32);
        let tree_node: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
        let row_nodes: Rc<RefCell<HashMap<String, NodeHandle>>> = Rc::default();
        let mounted = runtime.signal(0u64);
        let search: Rc<RefCell<(String, Instant)>> =
            Rc::new(RefCell::new((String::new(), Instant::now())));

        let toggle = {
            let open = open.clone();
            Rc::new(move |id: &str, to: Option<bool>| {
                let mut list = open.with_untracked(Clone::clone);
                let is = list.iter().any(|o| o == id);
                let want = to.unwrap_or(!is);
                if want && !is {
                    list.push(id.to_owned());
                } else if !want && is {
                    list.retain(|o| o != id);
                } else {
                    return;
                }
                open.set(list);
            })
        };
        // The current item: in view, and the tree's active descendant.
        {
            let (selected, shown, offset, tree_node, row_nodes, mounted) = (
                selected.clone(),
                shown.clone(),
                offset.clone(),
                tree_node.clone(),
                row_nodes.clone(),
                mounted.clone(),
            );
            cx.retain(runtime.effect(move || {
                let id = selected.get();
                mounted.with(|_| ());
                let index = id
                    .as_ref()
                    .and_then(|id| shown.with(|rows| rows.iter().position(|r| r.item.id == *id)));
                let Some(handle) = tree_node.borrow().clone() else {
                    return;
                };
                if let Some(i) = index
                    && let Some(scene) = handle.scene()
                {
                    let height = scene.borrow().bounds(handle.node()).height.max(ROW);
                    let top = i as f32 * ROW;
                    let now = offset.with_untracked(|o| *o);
                    if top < now {
                        offset.set(top);
                    } else if top + ROW > now + height {
                        offset.set(top + ROW - height);
                    }
                }
                let target = id.and_then(|id| row_nodes.borrow().get(&id).map(NodeHandle::node));
                handle.update_semantics(|node| node.active_descendant = target);
                // Each row's place: its level, and where among its siblings.
                let nodes = row_nodes.borrow();
                shown.with(|rows| {
                    for r in rows {
                        if let Some(n) = nodes.get(&r.item.id) {
                            n.update_semantics(|node| {
                                node.level = Some(r.level);
                                node.position_in_set = Some(r.position);
                                node.size_of_set = Some(r.siblings);
                            });
                        }
                    }
                });
            }));
        }

        let row_view = {
            let (shown, selected, toggle, activate, row_nodes, mounted) = (
                shown.clone(),
                selected.clone(),
                toggle.clone(),
                activate.clone(),
                row_nodes.clone(),
                mounted.clone(),
            );
            move |index: Signal<usize>, key: String, _: &mut Context| {
                let row = {
                    let (shown, index) = (shown.clone(), index.clone());
                    move || shown.with(|rows| rows.get(index.get()).cloned())
                };
                let current = row().unwrap_or_else(|| Row {
                    item: TreeItem::leaf(key.clone(), ""),
                    level: 1,
                    open: false,
                    position: 1,
                    siblings: 1,
                    parent: None,
                    loading: false,
                });
                let id = current.item.id.clone();
                let (chosen, styled) = (selected.clone(), selected.clone());
                let (id_chosen, id_styled) = (id.clone(), id.clone());
                let (label_row, chevron_row, expanded_row) =
                    (row.clone(), row.clone(), row.clone());
                let mut view = zgui::compose::row()
                    .role(Role::TreeItem)
                    .accessible_label(current.item.label.clone())
                    .selected_when(move || chosen.with(|s| s.as_deref() == Some(&*id_chosen)))
                    .h(ROW)
                    .pr(8.)
                    .gap(4.)
                    .items_center()
                    .rounded(5.)
                    .reactive_style(move || {
                        if styled.with(|s| s.as_deref() == Some(&*id_styled)) {
                            Styles::new().bg(tok::SELECTION_ITEM)
                        } else {
                            Styles::new().bg(Color(0, 0, 0, 0))
                        }
                    })
                    .on_mount({
                        let (row_nodes, mounted, id) =
                            (row_nodes.clone(), mounted.clone(), id.clone());
                        move |handle| {
                            row_nodes.borrow_mut().insert(id, handle);
                            mounted.set(mounted.with_untracked(|m| m + 1));
                        }
                    })
                    .child(div().shrink_0().reactive_style(move || {
                        let level = label_row().map_or(1, |r| r.level);
                        Styles::new().w((level - 1) as f32 * INDENT)
                    }));
                if current.item.parent {
                    view = view.expanded_when(move || expanded_row().is_some_and(|r| r.open));
                }
                let (toggle_click, id_toggle) = (toggle.clone(), id.clone());
                let (select_click, id_select) = (selected.clone(), id.clone());
                let (activate_click, id_activate) = (activate.clone(), id.clone());
                let loading = current.loading;
                view.child(
                    text_signal(move || match chevron_row() {
                        Some(r) if r.item.parent && r.open => "▾".into(),
                        Some(r) if r.item.parent => "▸".into(),
                        _ => String::new(),
                    })
                    .w(INDENT)
                    .shrink_0()
                    .text_size(10.)
                    .text_color(tok::TEXT_SECONDARY)
                    .accessible_hidden()
                    .on_event(move |cx| {
                        // The chevron opens and closes, not choosing.
                        if cx.phase != EventPhase::Capture
                            && matches!(
                                cx.event,
                                InputEvent::PointerDown {
                                    button: PointerButton::Primary,
                                    ..
                                }
                            )
                        {
                            toggle_click(&id_toggle, None);
                            cx.prevent_default();
                            cx.stop_propagation();
                        }
                    }),
                )
                .child(text(current.item.label.clone()).truncate().apply(|t| {
                    if loading {
                        t.text_color(tok::TEXT_TERTIARY)
                    } else {
                        t
                    }
                }))
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Capture || loading {
                        return;
                    }
                    if let InputEvent::PointerDown {
                        button: PointerButton::Primary,
                        ..
                    } = cx.event
                    {
                        if cx.click_count() >= 2 {
                            activate_click(&id_activate);
                        } else {
                            select_click.set(Some(id_select.clone()));
                        }
                    }
                })
            }
        };

        let (count, keys) = (shown.clone(), shown.clone());
        let list = virtual_list(
            offset,
            ROW,
            6,
            move || count.with(Vec::len),
            move |i| keys.with(|rows| rows.get(i).map(|r| r.item.id.clone()).unwrap_or_default()),
            row_view,
        );
        let (key_shown, key_selected, key_toggle, key_activate) = (
            shown.clone(),
            selected.clone(),
            toggle.clone(),
            activate.clone(),
        );
        let key_tree = tree_node.clone();
        div()
            .role(Role::Tree)
            .accessible_label(label.clone())
            .focusable(true)
            .h_full()
            .on_mount(move |handle| *tree_node.borrow_mut() = Some(handle))
            .on_event(move |cx| {
                if cx.phase == EventPhase::Capture {
                    return;
                }
                let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
                    // Focused with nothing chosen: the first item is current.
                    if matches!(cx.event, InputEvent::Focus)
                        && key_selected.with_untracked(Option::is_none)
                        && let Some(first) = key_shown.with_untracked(|r| r.first().cloned())
                    {
                        key_selected.set(Some(first.item.id));
                    }
                    return;
                };
                let rows = key_shown.with_untracked(Clone::clone);
                if rows.is_empty() {
                    return;
                }
                let at = key_selected
                    .with_untracked(Clone::clone)
                    .and_then(|id| rows.iter().position(|r| r.item.id == id));
                let rtl = key_tree
                    .borrow()
                    .as_ref()
                    .and_then(NodeHandle::scene)
                    .is_some_and(|scene| {
                        scene.borrow().direction(cx.current_target) == Direction::Rtl
                    });
                let (open_key, close_key) = if rtl {
                    (Key::ArrowLeft, Key::ArrowRight)
                } else {
                    (Key::ArrowRight, Key::ArrowLeft)
                };
                let go = |i: usize| {
                    key_selected.set(Some(rows[i].item.id.clone()));
                };
                let current = at.map(|i| &rows[i]);
                match key {
                    Key::ArrowDown => go(at.map_or(0, |i| (i + 1).min(rows.len() - 1))),
                    Key::ArrowUp => go(at.map_or(0, |i| i.saturating_sub(1))),
                    Key::Home => go(0),
                    Key::End => go(rows.len() - 1),
                    k if *k == open_key => match current {
                        Some(r) if r.item.parent && !r.open => key_toggle(&r.item.id, Some(true)),
                        Some(r) if r.item.parent => {
                            if let Some(i) = at.filter(|i| i + 1 < rows.len()) {
                                go(i + 1);
                            }
                        }
                        _ => {}
                    },
                    k if *k == close_key => match current {
                        Some(r) if r.item.parent && r.open => key_toggle(&r.item.id, Some(false)),
                        Some(r) => {
                            if let Some(p) = r
                                .parent
                                .as_ref()
                                .and_then(|p| rows.iter().position(|x| x.item.id == *p))
                            {
                                go(p);
                            }
                        }
                        None => {}
                    },
                    Key::Enter => {
                        if let Some(r) = current.filter(|r| !r.loading) {
                            key_activate(&r.item.id);
                        }
                    }
                    Key::Character(typed)
                        if !(modifiers.meta || modifiers.control || modifiers.alt) =>
                    {
                        let mut search = search.borrow_mut();
                        if search.1.elapsed() > Duration::from_millis(750) {
                            search.0.clear();
                        }
                        search.0.push_str(&typed.to_lowercase());
                        search.1 = Instant::now();
                        let start = at.map_or(0, |i| {
                            if search.0.chars().count() == 1 {
                                i + 1
                            } else {
                                i
                            }
                        });
                        let n = rows.len();
                        if let Some(i) = (0..n).map(|k| (start + k) % n).find(|&k| {
                            !rows[k].loading
                                && rows[k].item.label.to_lowercase().starts_with(&search.0)
                        }) {
                            go(i);
                        }
                    }
                    _ => return,
                }
                cx.prevent_default();
                cx.stop_propagation();
            })
            .child(list.h_full())
    })
}
