//! Two panes and a divider between them that drags to resize the first: a
//! sidebar beside its content, an editor over its terminal.
use crate::{
    compose::{NodeHandle, View, column, component, div, row},
    cursor::Cursor,
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::Direction,
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// How a [`split`] lays out and where its divider stops.
#[derive(Clone, Debug)]
pub struct SplitOptions {
    /// One pane over the other (a divider across), instead of side by side.
    pub vertical: bool,
    /// `size` is the second pane's (an inspector at the right), and the
    /// first fills the rest.
    pub second: bool,
    /// The sized pane's smallest size.
    pub min: f32,
    /// The sized pane's largest size.
    pub max: f32,
    /// The other pane's smallest size: the divider stops short of it, and
    /// a window too small for both shrinks the sized pane down to `min`.
    pub min_rest: f32,
    /// What assistive technology calls the divider ("Resize" by default):
    /// "Sidebar width".
    pub label: Option<String>,
}
impl Default for SplitOptions {
    fn default() -> Self {
        Self {
            vertical: false,
            second: false,
            min: 0.,
            max: f32::INFINITY,
            min_rest: 0.,
            label: None,
        }
    }
}

/// The divider's thickness, and how far a key moves it (ten times that
/// with Shift held, as macOS's split views do).
const DIVIDER: f32 = 5.;
const STEP: f32 = 10.;

/// `first` and `second` side by side (`vertical`: one over the other),
/// the first `size` points across (`options.second`: the second) and the
/// other filling the rest, with a divider between them that drags to
/// resize it. `size` is two-way:
/// the divider writes it as it moves (a persisted state keeps the layout
/// across launches) and writing it moves the divider. The divider is
/// focusable: the arrow keys move it, Home and End take it to its ends,
/// and a double click puts it back where it started. Dragging runs no
/// script and lays out only the split.
pub fn split(size: Signal<f32>, options: SplitOptions, first: View, second: View) -> View {
    let SplitOptions {
        vertical,
        second: sized_second,
        min,
        max,
        min_rest,
        label,
    } = options;
    let max = max.max(min);
    let initial = size.with_untracked(|s| *s);
    component(move |cx| {
        let dragging = cx.runtime().signal(false);
        let pane_size = size.clone();
        // The cross axis stretches each pane's content across it.
        let sized = |pane: View| {
            let pane_size = pane_size.clone();
            if vertical { row() } else { column() }
                .reactive_style(move || {
                    let s = pane_size.get();
                    let s = if s.is_finite() {
                        s.clamp(min, max)
                    } else {
                        min
                    };
                    let styles = Styles::default();
                    if vertical {
                        styles.h(s).min_h(min)
                    } else {
                        styles.w(s).min_w(min)
                    }
                })
                .child(pane)
        };
        let rest = |pane: View| {
            if vertical { row() } else { column() }
                .grow()
                .flex_basis(0.)
                .apply(|v| {
                    if vertical {
                        v.min_h(min_rest)
                    } else {
                        v.min_w(min_rest)
                    }
                })
                .child(pane)
        };
        let (first, second) = if sized_second {
            (rest(first), sized(second))
        } else {
            (sized(first), rest(second))
        };
        // Where the drag started: the pointer, and the first pane's size.
        let grab: Rc<Cell<Option<(f32, f32)>>> = Rc::default();
        // The divider's scene, for the split's size as it drags.
        let mounted: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
        let found = mounted.clone();
        let (shown, moving) = (dragging.clone(), dragging.clone());
        let divider = div()
            .shrink_0()
            .apply(|v| if vertical { v.h(DIVIDER) } else { v.w(DIVIDER) })
            .cursor(if vertical {
                Cursor::RowResize
            } else {
                Cursor::ColResize
            })
            .focusable(true)
            .role(Role::Splitter)
            .accessible_label(label.clone().unwrap_or_else(|| "Resize".into()))
            .numeric_when(
                min as f64,
                if max.is_finite() {
                    max as f64
                } else {
                    f64::MAX
                },
                {
                    let size = size.clone();
                    move || size.get() as f64
                },
            )
            .hover(|s| s.bg(tok::BORDER_STRONG))
            .focus(|s| s.bg(tok::ACCENT))
            .reactive_style(move || {
                let styles = Styles::default();
                if shown.get() {
                    styles.bg(tok::ACCENT)
                } else {
                    styles
                }
            })
            .child(
                // The line itself, centred in the divider's wider hit area.
                div()
                    .absolute()
                    .bg(tok::BORDER)
                    .apply(|v| {
                        if vertical {
                            v.left(0.).right(0.).top(DIVIDER / 2. - 0.5).h(1.)
                        } else {
                            v.top(0.).bottom(0.).left(DIVIDER / 2. - 0.5).w(1.)
                        }
                    })
                    .pointer_events(false),
            )
            .relative()
            .on_mount(move |handle| *mounted.borrow_mut() = Some(handle))
            .on_event(move |cx| {
                if cx.phase == EventPhase::Capture {
                    return;
                }
                let Some(scene) = found.borrow().as_ref().and_then(NodeHandle::scene) else {
                    return;
                };
                let (room, rtl) = {
                    let scene = scene.borrow();
                    let parent = scene.parent(cx.current_target);
                    let bounds = parent.map(|p| scene.bounds(p));
                    let room = bounds.map_or(f32::INFINITY, |b| {
                        (if vertical { b.height } else { b.width }) - min_rest - DIVIDER
                    });
                    let rtl = !vertical && scene.direction(cx.current_target) == Direction::Rtl;
                    (room, rtl)
                };
                let clamp = |s: f32| s.min(max).min(room).max(min);
                let along = |x: f32, y: f32| if vertical { y } else { x };
                let now = size.with_untracked(|s| clamp(*s));
                let next = match cx.event.clone() {
                    InputEvent::PointerDown {
                        x,
                        y,
                        button: PointerButton::Primary,
                    } => {
                        cx.focus();
                        if cx.click_count() >= 2 {
                            Some(initial)
                        } else {
                            grab.set(Some((along(x, y), now)));
                            moving.set(true);
                            cx.capture_pointer();
                            None
                        }
                    }
                    InputEvent::PointerMove { x, y } => grab.get().map(|(from, start)| {
                        let moved = along(x, y) - from;
                        // The second pane grows as the divider moves back.
                        start + if rtl != sized_second { -moved } else { moved }
                    }),
                    InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    }
                    | InputEvent::PointerCancel
                    | InputEvent::Blur => {
                        if grab.take().is_some() {
                            moving.set(false);
                            cx.release_pointer();
                        }
                        None
                    }
                    InputEvent::KeyDown { key, modifiers, .. } => {
                        let step = if modifiers.shift { STEP * 10. } else { STEP };
                        let (back, on) = if vertical {
                            (Key::ArrowUp, Key::ArrowDown)
                        } else if rtl {
                            (Key::ArrowRight, Key::ArrowLeft)
                        } else {
                            (Key::ArrowLeft, Key::ArrowRight)
                        };
                        // The arrows move the divider; what it sizes follows.
                        let (less, more) = if sized_second { (on, back) } else { (back, on) };
                        match key {
                            k if k == less => Some(now - step),
                            k if k == more => Some(now + step),
                            Key::Home => Some(min),
                            Key::End => Some(f32::INFINITY),
                            _ => None,
                        }
                    }
                    InputEvent::Increment => Some(now + STEP),
                    InputEvent::Decrement => Some(now - STEP),
                    InputEvent::SetNumericValue(value) if value.is_finite() => Some(value as f32),
                    _ => return,
                };
                if let Some(next) = next {
                    size.set(clamp(next));
                }
                cx.prevent_default();
                cx.stop_propagation();
            });
        if vertical { column() } else { row() }
            .apply(|v| if vertical { v.w_full() } else { v.h_full() })
            .child(first)
            .child(divider)
            .child(second)
    })
}
