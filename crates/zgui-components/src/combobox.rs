//! A combobox: a text field with a list of suggestions under it, as
//! WAI-ARIA's editable combobox with a list. Focus stays in the field:
//! typing shows the list, Down and Up move through it (the highlighted
//! option the field's active descendant), Enter chooses, Escape closes it
//! (or clears the field); a click on an option chooses it. The app supplies
//! the options as they should show for the text: filtered at once, or
//! fetched (with `loading` while they come); the list is virtual, so
//! thousands cost what shows.
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use zgui::{
    compose::{
        NodeHandle, Placement, PlacementEdge, PlacementSide, View, column, component, div, popover,
        switch, text, text_input, virtual_list,
    },
    input::{EventPhase, InputEvent, Key},
    reactive::Signal,
    scene::Color,
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};

/// An option of a [`combobox`]: its value, what it shows, and a detail
/// shown after it (a path, a kind).
#[derive(Clone, Debug, PartialEq)]
pub struct ComboItem {
    /// What the combobox's value becomes when it is chosen.
    pub value: String,
    /// What it shows, and what the field shows once it is chosen.
    pub label: String,
    /// Shown after the label in the secondary text colour.
    pub detail: Option<String>,
}
impl ComboItem {
    /// An option showing `label`.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            detail: None,
        }
    }
    /// With a detail after its label.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// How a [`combobox`]'s list shows.
#[derive(Clone)]
pub struct ComboboxOptions {
    /// While true, the list says it is searching (options being fetched).
    pub loading: Option<Signal<bool>>,
    /// What the list says when there are no options.
    pub empty: String,
    /// The list's tallest, in points; it scrolls past that.
    pub max_height: f32,
}
impl Default for ComboboxOptions {
    fn default() -> Self {
        Self {
            loading: None,
            empty: "No results".into(),
            max_height: 280.,
        }
    }
}

/// A row's height in the list.
const ROW: f32 = 30.;

/// A field editing `text` with `items` listed under it as it is typed in;
/// choosing one sets `value` to its value and the field to its label.
/// `items` is the app's: what should show for the text now.
pub fn combobox(
    label: impl Into<String>,
    text_value: Signal<String>,
    value: Signal<Option<String>>,
    items: Signal<Vec<ComboItem>>,
    options: ComboboxOptions,
) -> View {
    let label = label.into();
    component(move |cx| {
        let runtime = cx.runtime();
        let open = runtime.signal(false);
        let highlight = runtime.signal(None::<usize>);
        let offset = runtime.signal(0_f32);
        // Set while the combobox writes the text itself (a choice), so the
        // list doesn't open for it.
        let writing = Rc::new(std::cell::Cell::new(false));
        let focused = Rc::new(std::cell::Cell::new(false));
        let field_node: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
        let rows: Rc<RefCell<HashMap<String, NodeHandle>>> = Rc::default();
        // Bumped as rows mount, so the active descendant finds its row.
        let mounted = runtime.signal(0u64);

        // Typing shows the list, its first option highlighted.
        {
            let (text_value, open, highlight, writing, focused, items) = (
                text_value.clone(),
                open.clone(),
                highlight.clone(),
                writing.clone(),
                focused.clone(),
                items.clone(),
            );
            let mut first = true;
            cx.retain(runtime.effect(move || {
                text_value.with(|_| ());
                if std::mem::take(&mut first) || writing.get() || !focused.get() {
                    return;
                }
                open.set(true);
                let any = items.with_untracked(|l| !l.is_empty());
                highlight.set(any.then_some(0));
            }));
        }
        // New options: the highlight stays on the list.
        {
            let (items, highlight) = (items.clone(), highlight.clone());
            cx.retain(runtime.effect(move || {
                let n = items.with(Vec::len);
                let now = highlight.with_untracked(|h| *h);
                let next = match now {
                    Some(i) if n == 0 => {
                        let _ = i;
                        None
                    }
                    Some(i) => Some(i.min(n - 1)),
                    None => None,
                };
                if next != now {
                    highlight.set(next);
                }
            }));
        }
        // The highlighted option: in the list's view, and the field's
        // active descendant.
        {
            let (highlight, offset, items, rows, field_node, open, mounted) = (
                highlight.clone(),
                offset.clone(),
                items.clone(),
                rows.clone(),
                field_node.clone(),
                open.clone(),
                mounted.clone(),
            );
            let max_height = options.max_height;
            cx.retain(runtime.effect(move || {
                let at = highlight.get();
                let shown = open.get();
                mounted.with(|_| ());
                if let Some(i) = at {
                    let top = i as f32 * ROW;
                    let height = (items.with_untracked(Vec::len) as f32 * ROW).min(max_height);
                    let now = offset.with_untracked(|o| *o);
                    if top < now {
                        offset.set(top);
                    } else if top + ROW > now + height {
                        offset.set(top + ROW - height);
                    }
                }
                let Some(field) = field_node.borrow().clone() else {
                    return;
                };
                let target = at.filter(|_| shown).and_then(|i| {
                    let value = items.with_untracked(|l| l.get(i).map(|o| o.value.clone()))?;
                    rows.borrow().get(&value).map(NodeHandle::node)
                });
                field.update_semantics(|node| node.active_descendant = target);
            }));
        }
        let choose = {
            let (items, value, text_value, open, writing) = (
                items.clone(),
                value.clone(),
                text_value.clone(),
                open.clone(),
                writing.clone(),
            );
            Rc::new(move |i: usize| {
                let Some(item) = items.with_untracked(|l| l.get(i).cloned()) else {
                    return;
                };
                writing.set(true);
                value.set(Some(item.value));
                text_value.set(item.label);
                writing.set(false);
                open.set(false);
            })
        };

        let field = {
            let (open, highlight, items, choose, text_value, focused) = (
                open.clone(),
                highlight.clone(),
                items.clone(),
                choose.clone(),
                text_value.clone(),
                focused.clone(),
            );
            let expanded = open.clone();
            text_input(label.clone(), text_value.clone())
                .role(Role::ComboBox)
                .expanded_when(move || expanded.get())
                .w_full()
                .on_mount({
                    let field_node = field_node.clone();
                    move |handle| *field_node.borrow_mut() = Some(handle)
                })
                // Going down, before the editor (whose arrows move its caret).
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Bubble {
                        return;
                    }
                    let n = items.with_untracked(Vec::len);
                    let shown = open.with_untracked(|o| *o);
                    let at = highlight.with_untracked(|h| *h);
                    match &cx.event {
                        InputEvent::Focus => {
                            focused.set(true);
                            return;
                        }
                        InputEvent::Blur => {
                            focused.set(false);
                            open.set(false);
                            return;
                        }
                        InputEvent::KeyDown { key, .. } => match key {
                            Key::ArrowDown => {
                                if !shown {
                                    open.set(true);
                                    highlight.set((n > 0).then_some(0));
                                } else if n > 0 {
                                    highlight.set(Some(at.map_or(0, |i| (i + 1) % n)));
                                }
                            }
                            Key::ArrowUp if shown && n > 0 => {
                                highlight.set(Some(at.map_or(n - 1, |i| (i + n - 1) % n)));
                            }
                            Key::PageDown if shown && n > 0 => {
                                highlight.set(Some(at.map_or(0, |i| (i + 8).min(n - 1))));
                            }
                            Key::PageUp if shown && n > 0 => {
                                highlight.set(Some(at.map_or(0, |i| i.saturating_sub(8))));
                            }
                            Key::Enter if shown && at.is_some() => choose(at.unwrap()),
                            Key::Escape if shown => {
                                open.set(false);
                            }
                            Key::Escape if !text_value.with_untracked(String::is_empty) => {
                                text_value.set(String::new());
                            }
                            _ => return,
                        },
                        _ => return,
                    }
                    cx.prevent_default();
                    cx.stop_propagation();
                })
        };

        let row = {
            let (items, highlight, choose, rows) = (
                items.clone(),
                highlight.clone(),
                choose.clone(),
                rows.clone(),
            );
            move |index: Signal<usize>, key: String, _: &mut zgui::compose::Context| {
                let item = {
                    let items = items.clone();
                    let index = index.clone();
                    move || items.with(|l| l.get(index.get()).cloned())
                };
                let (shown, detail) = (item.clone(), item.clone());
                let (selected, styled) = (highlight.clone(), highlight.clone());
                let (at, at_styled) = (index.clone(), index.clone());
                let (choose, hover, at_event) = (choose.clone(), highlight.clone(), index.clone());
                zgui::compose::row()
                    .role(Role::ListBoxOption)
                    .selected_when(move || selected.get() == Some(at.get()))
                    .h(ROW)
                    .px(8.)
                    .gap(8.)
                    .items_center()
                    .rounded(5.)
                    .reactive_style(move || {
                        if styled.get() == Some(at_styled.get()) {
                            Styles::new().bg(tok::SELECTION_ITEM)
                        } else {
                            Styles::new().bg(Color(0, 0, 0, 0))
                        }
                    })
                    .on_mount({
                        let (rows, mounted) = (rows.clone(), mounted.clone());
                        move |handle| {
                            rows.borrow_mut().insert(key, handle);
                            mounted.set(mounted.with_untracked(|m| m + 1));
                        }
                    })
                    .on_event(move |cx| {
                        if cx.phase == EventPhase::Capture {
                            return;
                        }
                        match cx.event {
                            // Chosen as pressed: the field keeps focus.
                            InputEvent::PointerDown { .. } => {
                                cx.prevent_default();
                                cx.stop_propagation();
                                choose(at_event.get());
                            }
                            InputEvent::PointerMove { .. }
                                if hover.with_untracked(|h| *h) != Some(at_event.get()) =>
                            {
                                hover.set(Some(at_event.get()));
                            }
                            _ => {}
                        }
                    })
                    .child(
                        zgui::compose::text_signal(move || {
                            shown().map(|i| i.label).unwrap_or_default()
                        })
                        .truncate(),
                    )
                    .child(
                        zgui::compose::text_signal(move || {
                            detail().and_then(|i| i.detail).unwrap_or_default()
                        })
                        .text_color(tok::TEXT_SECONDARY)
                        .text_size(tok::FONT_SIZE_SM)
                        .truncate(),
                    )
            }
        };

        let loading = options.loading.clone();
        let (list_items, count_items, empty) =
            (items.clone(), items.clone(), options.empty.clone());
        let max_height = options.max_height;
        let content = switch(
            move || {
                let busy = loading.as_ref().is_some_and(|l| l.get());
                (busy, list_items.with(Vec::is_empty))
            },
            move |(busy, none), _| {
                if busy {
                    return text("Searching…").p(8.).text_color(tok::TEXT_SECONDARY);
                }
                if none {
                    return text(empty.clone()).p(8.).text_color(tok::TEXT_SECONDARY);
                }
                let (count, keys) = (count_items.clone(), count_items.clone());
                let height = count_items.clone();
                virtual_list(
                    offset.clone(),
                    ROW,
                    4,
                    move || count.with(Vec::len),
                    move |i| keys.with(|l| l.get(i).map(|o| o.value.clone()).unwrap_or_default()),
                    row.clone(),
                )
                .role(Role::ListBox)
                .reactive_style(move || {
                    Styles::new().h((height.with(Vec::len) as f32 * ROW).min(max_height))
                })
            },
        );
        popover(format!("{label} suggestions"), open.clone(), field)
            .layer()
            .lazy()
            .autofocus(false)
            .placement(Placement {
                side: PlacementSide::Bottom,
                align: PlacementEdge::Start,
                offset: 4.,
            })
            .match_anchor_width()
            .p(4.)
            .child(column().child(content).child(div()))
    })
}
