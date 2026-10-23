//! Everyday controls built from views: a segmented control, a number
//! stepper, and a disclosure (an accordion's sections). Each is keyboard
//! complete as WAI-ARIA describes its pattern, says what it is to
//! assistive technology, mirrors right to left, and takes its colours from
//! the theme.
use crate::{
    compose::{View, button, column, component, div, row, switch, text, text_input},
    input::{EventPhase, FocusGroup, InputEvent, Key},
    reactive::Signal,
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};
use std::{cell::Cell, rc::Rc};

/// `options` side by side, one chosen (`selected`, its index): AppKit's
/// segmented control, a radio group to assistive technology. One Tab
/// stop: the arrows move the choice (as radios do), Home and End to the
/// ends; a click chooses.
pub fn segmented(
    label: impl Into<String>,
    selected: Signal<usize>,
    options: impl IntoIterator<Item = impl Into<String>>,
) -> View {
    let mut track = row()
        .role(Role::RadioGroup)
        .accessible_label(label)
        .focus_group(FocusGroup::Horizontal)
        .shrink_0()
        .p(2.)
        .gap(2.)
        .rounded(7.)
        .bg(tok::SURFACE_SUNKEN)
        .border(1.)
        .border_color(tok::BORDER_SUBTLE);
    for (i, option) in options.into_iter().enumerate() {
        let (checked, styled, clicked, focused) = (
            selected.clone(),
            selected.clone(),
            selected.clone(),
            selected.clone(),
        );
        track = track.child(
            button()
                .role(Role::RadioButton)
                .checked_when(move || checked.get() == i)
                .px(12.)
                .py(3.)
                .rounded(5.)
                .border(0.)
                .reactive_style(move || {
                    if styled.get() == i {
                        Styles::new()
                            .bg(tok::SURFACE_RAISED)
                            .text_color(tok::TEXT_PRIMARY)
                    } else {
                        Styles::new()
                            .bg(crate::scene::Color(0, 0, 0, 0))
                            .text_color(tok::TEXT_SECONDARY)
                    }
                })
                .hover(|s| s.text_color(tok::TEXT_PRIMARY))
                .on_click(move || {
                    clicked.set(i);
                })
                // The arrows move focus along the group; the choice follows.
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Target
                        && matches!(cx.event, InputEvent::Focus)
                        && focused.with_untracked(|s| *s) != i
                    {
                        focused.set(i);
                    }
                })
                .child(text(option)),
        );
    }
    track
}

/// How a [`stepper`] counts.
#[derive(Clone, Copy, Debug)]
pub struct StepperOptions {
    /// Its smallest value.
    pub min: f64,
    /// Its largest value.
    pub max: f64,
    /// What an arrow or a button adds or takes away; ten of them with Page
    /// Up and Page Down.
    pub step: f64,
    /// Digits shown after the point.
    pub decimals: usize,
}
impl Default for StepperOptions {
    fn default() -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: 1.,
            decimals: 0,
        }
    }
}

/// A number field with buttons that step it (`value`, kept between `min`
/// and `max`): AppKit's stepper beside its field, a spin button to
/// assistive technology. The arrows step it, Page Up and Down ten steps,
/// Home and End to its ends; what is typed applies on Enter or on leaving
/// the field, and anything not a number goes back to the value.
pub fn stepper(label: impl Into<String>, value: Signal<f64>, options: StepperOptions) -> View {
    let label = label.into();
    let StepperOptions {
        min,
        max,
        step,
        decimals,
    } = options;
    let clamp = move |v: f64| {
        let v = if v.is_finite() { v } else { 0. };
        let scale = 10f64.powi(decimals as i32);
        ((v * scale).round() / scale).clamp(min, max)
    };
    let shown = move |v: f64| format!("{v:.decimals$}");
    component(move |cx| {
        let text_value = cx.state(shown(value.with_untracked(|v| *v)));
        // While the field is being typed in, it keeps what is typed.
        let editing = Rc::new(Cell::new(false));
        {
            let (value, text_value, editing) = (value.clone(), text_value.clone(), editing.clone());
            cx.retain(cx.runtime().effect(move || {
                let now = shown(value.get());
                if !editing.get() {
                    text_value.set(now);
                }
            }));
        }
        let set = {
            let value = value.clone();
            move |v: f64| {
                value.set(clamp(v));
            }
        };
        let commit = {
            let (value, text_value, set) = (value.clone(), text_value.clone(), set.clone());
            move || {
                let typed = text_value.with_untracked(Clone::clone);
                if let Ok(v) = typed.trim().parse::<f64>() {
                    set(v);
                }
                text_value.set(shown(value.with_untracked(|v| *v)));
            }
        };
        let field = {
            let (value, set, commit, editing) =
                (value.clone(), set.clone(), commit.clone(), editing.clone());
            let typed = text_value.clone();
            text_input(label.clone(), text_value.clone())
                .role(Role::SpinButton)
                .numeric_when(min, max, {
                    let value = value.clone();
                    move || value.get()
                })
                .w(72.)
                // Going down, before the editor (whose arrows move its caret).
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Bubble {
                        return;
                    }
                    let now = value.with_untracked(|v| *v);
                    let next = match &cx.event {
                        InputEvent::Focus => {
                            editing.set(true);
                            return;
                        }
                        InputEvent::Blur => {
                            editing.set(false);
                            commit();
                            return;
                        }
                        InputEvent::KeyDown { key, .. } => match key {
                            Key::Enter => {
                                commit();
                                None
                            }
                            Key::ArrowUp => Some(now + step),
                            Key::ArrowDown => Some(now - step),
                            Key::PageUp => Some(now + step * 10.),
                            Key::PageDown => Some(now - step * 10.),
                            Key::Home if min.is_finite() => Some(min),
                            Key::End if max.is_finite() => Some(max),
                            _ => return,
                        },
                        InputEvent::Increment => Some(now + step),
                        InputEvent::Decrement => Some(now - step),
                        InputEvent::SetNumericValue(v) if v.is_finite() => Some(*v),
                        _ => return,
                    };
                    if let Some(next) = next {
                        set(next);
                        // Shown at once, though the field is focused.
                        typed.set(shown(value.with_untracked(|v| *v)));
                    }
                    cx.prevent_default();
                    cx.stop_propagation();
                })
        };
        let arrow = |up: bool| {
            let (set, value) = (set.clone(), value.clone());
            let at_end = value.clone();
            button()
                .accessible_label(if up { "Increment" } else { "Decrement" })
                .focusable(false)
                .w(18.)
                .h(13.)
                .p(0.)
                .rounded(0.)
                .border(0.)
                .bg(crate::scene::Color(0, 0, 0, 0))
                .hover(|s| s.bg(tok::HOVER))
                .items_center()
                .justify_center()
                .disabled_when(move || {
                    let v = at_end.get();
                    if up { v >= max } else { v <= min }
                })
                .on_click(move || {
                    let now = value.with_untracked(|v| *v);
                    set(if up { now + step } else { now - step });
                })
                .child(text(if up { "▲" } else { "▼" }).text_size(7.))
        };
        // The arrows in a box of their own beside the field, as AppKit's
        // stepper sits beside its text field.
        row().gap(4.).items_center().child(field).child(
            column()
                .shrink_0()
                .rounded(5.)
                .border(1.)
                .border_color(tok::BORDER)
                .bg(tok::INPUT_BG)
                .overflow_hidden()
                .child(arrow(true))
                .child(div().h(1.).bg(tok::BORDER).accessible_hidden())
                .child(arrow(false)),
        )
    })
}

/// A summary that shows and hides `content` (`open`), as HTML's
/// `<details>`: a button, `expanded` to assistive technology, whose
/// chevron turns as it opens; Enter or Space toggles it. Sections sharing
/// one choice make an accordion ([`accordion`]).
pub fn disclosure(open: Signal<bool>, summary: View, content: impl Fn() -> View + 'static) -> View {
    let (expanded, turned, toggled, shown) = (open.clone(), open.clone(), open.clone(), open);
    column()
        .child(
            button()
                .expanded_when(move || expanded.get())
                .w_full()
                .gap(8.)
                .px(8.)
                .py(6.)
                .items_center()
                .rounded(6.)
                .bg(crate::scene::Color(0, 0, 0, 0))
                .border(0.)
                .hover(|s| s.bg(tok::HOVER))
                .on_click(move || {
                    toggled.set(!toggled.with_untracked(|o| *o));
                })
                .child(
                    text("›")
                        .text_color(tok::TEXT_SECONDARY)
                        .w(10.)
                        .reactive_style(move || {
                            // Down when open; it points along the line
                            // when closed, the start's way right to left.
                            Styles::new().rotate(if turned.get() { 90. } else { 0. })
                        }),
                )
                .child(div().grow().child(summary)),
        )
        .child(switch(
            move || shown.get(),
            move |open, _| {
                if open {
                    div().pl(26.).pr(8.).pb(8.).child(content())
                } else {
                    div()
                }
            },
        ))
}

/// Sections of which one at most is open (`open`, its index, or `None`):
/// each a [`disclosure`] of a title and what `content` builds for it.
/// Opening one closes the one open.
pub fn accordion(
    open: Signal<Option<usize>>,
    sections: impl IntoIterator<Item = (String, Rc<dyn Fn() -> View>)>,
) -> View {
    let mut list = column().gap(2.);
    for (i, (title, content)) in sections.into_iter().enumerate() {
        let this = component({
            let open = open.clone();
            move |cx| {
                let shown = cx.state(open.with_untracked(|o| *o == Some(i)));
                {
                    let (open, shown) = (open.clone(), shown.clone());
                    cx.retain(cx.runtime().effect(move || {
                        shown.set(open.get() == Some(i));
                    }));
                }
                {
                    let (open, shown) = (open.clone(), shown.clone());
                    cx.retain(cx.runtime().effect(move || {
                        let now = shown.get();
                        let current = open.with_untracked(|o| *o);
                        if now && current != Some(i) {
                            open.set(Some(i));
                        } else if !now && current == Some(i) {
                            open.set(None);
                        }
                    }));
                }
                let content = content.clone();
                disclosure(shown, text(title.clone()).font_weight(600), move || {
                    content()
                })
            }
        });
        list = list.child(this);
    }
    list
}

/// A line between groups of items, across a menu or a list (`vertical`: down
/// a toolbar): the theme's subtle border, hidden from assistive technology.
pub fn separator(vertical: bool) -> View {
    let line = div().shrink_0().bg(tok::BORDER_SUBTLE).accessible_hidden();
    if vertical {
        line.w(1.).h_full().mx(4.)
    } else {
        line.h(1.).w_full().my(4.)
    }
}
