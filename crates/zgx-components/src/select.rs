//! A select: a button showing the chosen option that opens a menu of them
//! over itself, the chosen one under the pointer, as AppKit's pop-up button
//! (and WAI-ARIA's select-only combobox). Closed, typing chooses the next
//! option starting with what was typed, Up and Down and Enter and Space
//! open it; open, it is a menu (arrows, typeahead, Enter, Escape).
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    compose::{View, button, column, component, keyed, menu, menu_item, text, text_signal},
    input::{EventPhase, InputEvent, Key},
    reactive::Signal,
    semantics::Role,
    style::Styled,
    theme::tok,
};

/// An option of a [`select`]: its value, what it shows, and whether it can
/// be chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectOption {
    /// What `select`'s value is when it is chosen.
    pub value: String,
    /// What it shows.
    pub label: String,
    /// Shown but not chosen.
    pub disabled: bool,
}
impl SelectOption {
    /// An option showing `label`.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }
    /// One shown but not chosen.
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// How long typed letters add up to one search, as a menu's typeahead.
const TYPEAHEAD: Duration = Duration::from_millis(750);

/// A select's options: a list fixed as it is built, or one that changes.
enum Options {
    Fixed(Vec<SelectOption>),
    Live(Signal<Vec<SelectOption>>),
}
impl Options {
    /// The options now (tracked, where they change).
    fn with<R>(&self, f: impl FnOnce(&[SelectOption]) -> R) -> R {
        match self {
            Self::Fixed(options) => f(options),
            Self::Live(options) => options.with(|o| f(o)),
        }
    }
    fn with_untracked<R>(&self, f: impl FnOnce(&[SelectOption]) -> R) -> R {
        match self {
            Self::Fixed(options) => f(options),
            Self::Live(options) => options.with_untracked(|o| f(o)),
        }
    }
}

/// A button showing the option `value` names (`placeholder` when none
/// does) that opens a menu of `options`, chosen one over the button; its
/// menu is as wide as it, at least. `label` names it.
pub fn select(
    label: impl Into<String>,
    value: Signal<String>,
    options: Vec<SelectOption>,
    placeholder: impl Into<String>,
) -> View {
    build(
        label.into(),
        value,
        Options::Fixed(options),
        placeholder.into(),
    )
}

/// A [`select`] of options that change (relabelled in another locale,
/// added, removed): its menu and the label it shows follow them, options
/// kept by their value.
pub fn select_live(
    label: impl Into<String>,
    value: Signal<String>,
    options: Signal<Vec<SelectOption>>,
    placeholder: impl Into<String>,
) -> View {
    build(
        label.into(),
        value,
        Options::Live(options),
        placeholder.into(),
    )
}

fn build(label: String, value: Signal<String>, options: Options, placeholder: String) -> View {
    let options = Rc::new(options);
    component(move |cx| {
        // A label that changes (a translation, a state), where the caller
        // gave one (`FollowedLabel::around`).
        let followed = zgx::compose::FollowedLabel::take(cx);
        let open = cx.runtime().signal(false);
        let shown = {
            let (value, options, placeholder) =
                (value.clone(), options.clone(), placeholder.clone());
            move || {
                value.with(|v| {
                    options.with(|options| {
                        options
                            .iter()
                            .find(|o| o.value == *v)
                            .map_or(placeholder.clone(), |o| o.label.clone())
                    })
                })
            }
        };
        let search: Rc<RefCell<(String, Instant)>> =
            Rc::new(RefCell::new((String::new(), Instant::now())));
        let (key_value, key_options, key_open) = (value.clone(), options.clone(), open.clone());
        let (expanded, value_text, label_text) = (open.clone(), shown.clone(), shown.clone());
        let trigger = button()
            .role(Role::ComboBox)
            .accessible_label_or(label.clone(), followed.as_ref())
            .value_when(value_text)
            .expanded_when(move || expanded.get())
            .w_full()
            .min_w(120.)
            .gap(8.)
            .justify_between()
            .child(text_signal(label_text).grow())
            .child(text("⌄").text_color(tok::TEXT_SECONDARY))
            .on_event(move |cx| {
                if cx.phase == EventPhase::Capture || key_open.with_untracked(|o| *o) {
                    return;
                }
                let InputEvent::KeyDown { key, modifiers, .. } = &cx.event else {
                    return;
                };
                match key {
                    Key::ArrowDown | Key::ArrowUp => {
                        key_open.set(true);
                    }
                    // Typing chooses, closed, as a native select does.
                    Key::Character(typed)
                        if !(modifiers.meta || modifiers.control || modifiers.alt) =>
                    {
                        let mut search = search.borrow_mut();
                        if search.1.elapsed() > TYPEAHEAD {
                            search.0.clear();
                        }
                        search.0.push_str(&typed.to_lowercase());
                        search.1 = Instant::now();
                        let current = key_value.with_untracked(Clone::clone);
                        let found = key_options.with_untracked(|key_options| {
                            let at = key_options.iter().position(|o| o.value == current);
                            // One letter again cycles through those starting with it.
                            let first = search.0.chars().next();
                            let repeated = search.0.chars().all(|c| Some(c) == first);
                            let prefix = if repeated {
                                search.0[..search.0.chars().next().map_or(0, char::len_utf8)]
                                    .to_owned()
                            } else {
                                search.0.clone()
                            };
                            let start = if repeated {
                                at.map_or(0, |a| a + 1)
                            } else {
                                at.unwrap_or(0)
                            };
                            let n = key_options.len();
                            (0..n)
                                .map(|i| (start + i) % n)
                                .find(|&i| {
                                    let o = &key_options[i];
                                    !o.disabled && o.label.to_lowercase().starts_with(&prefix)
                                })
                                .map(|i| key_options[i].value.clone())
                        });
                        if let Some(found) = found {
                            key_value.set(found);
                        }
                    }
                    _ => return,
                }
                cx.prevent_default();
                cx.stop_propagation();
            });
        let mut list = menu(label.clone(), open.clone(), trigger)
            .accessible_label_or(label.clone(), followed.as_ref())
            .match_anchor_width()
            .open_over({
                let value = value.clone();
                move || value.with(Clone::clone)
            });
        match &*options {
            Options::Fixed(fixed) => {
                for option in fixed {
                    list = list.child(item(&value, option).disabled(option.disabled));
                }
            }
            // Kept by value: one relabelled updates in place, its label
            // and whether it is disabled read from the list.
            Options::Live(live) => {
                let (keys, live, value) = (live.clone(), live.clone(), value.clone());
                list = list.child(keyed(
                    move || keys.with(|o| o.iter().map(|o| o.value.clone()).collect()),
                    move |id: String, _| {
                        let option = live
                            .with_untracked(|o| o.iter().find(|o| o.value == id).cloned())
                            .unwrap_or_else(|| SelectOption::new(id.clone(), id.clone()));
                        let (label, disabled) = (live.clone(), live.clone());
                        let (label_id, disabled_id) = (id.clone(), id);
                        item(&value, &option)
                            .accessible_label_when(move || {
                                label.with(|o| {
                                    o.iter()
                                        .find(|o| o.value == label_id)
                                        .map_or(String::new(), |o| o.label.clone())
                                })
                            })
                            .disabled_when(move || {
                                disabled.with(|o| {
                                    o.iter()
                                        .find(|o| o.value == disabled_id)
                                        .is_some_and(|o| o.disabled)
                                })
                            })
                    },
                ));
            }
        }
        // What the select is given (its `id`, its size) lands on this box,
        // which the trigger fills: on the menu it would reach the panel.
        column().child(list)
    })
}

/// The menu's row for `option`, checked while `value` is it.
fn item(value: &Signal<String>, option: &SelectOption) -> View {
    let (checked, chosen, id, set) = (
        value.clone(),
        value.clone(),
        option.value.clone(),
        option.value.clone(),
    );
    menu_item(option.label.clone())
        .id(option.value.clone())
        .menu_radio()
        .menu_checked_when(move || checked.with(|v| *v == id))
        .on_click(move || {
            chosen.set(set.clone());
        })
}
