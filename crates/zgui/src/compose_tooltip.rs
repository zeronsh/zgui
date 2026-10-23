//! Tooltips (`View::tooltip`), as a browser's and the system's behave: shown
//! over a view 300 ms after the pointer comes to rest on it, at once within a
//! second of another (as a tooltip provider's `timeout`), gone as the pointer
//! leaves or presses, or a key is pressed anywhere. One shows at a time. It
//! fades in over 140 ms and out over 100, takes no focus and lets the pointer
//! through. An interactive one (a hover card) keeps open while the pointer is
//! on the view or on it, takes clicks and typing, and closes on Escape.
use crate::{
    compose::{Placement, PlacementEdge, PlacementSide, TaskRunner, TaskToken, View},
    input::{EventPhase, InputEvent, Key},
    style::{Styled, Styles},
    theme::tok,
    widgets::WeakUi,
};
use std::{
    any::Any,
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

/// What a tooltip shows and how (`View::tooltip`): text in the theme's look
/// (an overlay card, small text), or a view of the app's own.
pub struct Tooltip {
    content: Content,
    placement: Option<Placement>,
    interactive: bool,
}
enum Content {
    Text(String),
    View(Box<View>),
}
impl From<&str> for Tooltip {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}
impl From<String> for Tooltip {
    fn from(text: String) -> Self {
        Self {
            content: Content::Text(text),
            placement: None,
            interactive: false,
        }
    }
}
impl From<View> for Tooltip {
    fn from(view: View) -> Self {
        Self {
            content: Content::View(Box::new(view)),
            placement: None,
            interactive: false,
        }
    }
}
impl Tooltip {
    /// Where it sits: above the view, centred, 6 points off by default.
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }
    /// A hover card the pointer can move into and click in: it stays while
    /// the pointer is on the view or the card (a moment's grace to cross),
    /// typing goes to its fields, and Escape closes it.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }
}

impl View {
    /// Show `tip` over this view while the pointer rests on it (see the
    /// module). Call it last: what follows styles the tooltip's panel.
    pub fn tooltip(self, tip: impl Into<Tooltip>) -> View {
        wrap(self, tip.into())
    }
}

/// A tooltip showing: its number, what closes it, and whether it is an
/// interactive card.
type Showing = (u64, Rc<dyn Fn()>, bool);
thread_local! {
    /// Tooltips showing, and when the last one closed: within a second of
    /// one, the next opens at once.
    static TOOLTIPS: Cell<(usize, Option<Instant>)> = const { Cell::new((0, None)) };
    /// The tooltip showing (one at most): its number, and what closes it.
    static SHOWING: RefCell<Option<Showing>> = const { RefCell::new(None) };
    static NEXT: Cell<u64> = const { Cell::new(0) };
    /// What hears keys while a tooltip shows: one document listener.
    static KEYS: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
}

/// A key pressed anywhere: the tooltip showing closes, so it never covers
/// what is typed; an interactive card only on Escape.
fn on_key(event: &InputEvent) {
    let InputEvent::KeyDown { key, .. } = event else {
        return;
    };
    let close = SHOWING.with(|s| {
        s.borrow_mut()
            .take_if(|(_, _, interactive)| !*interactive || *key == Key::Escape)
    });
    if let Some((_, close, _)) = close {
        close();
    }
}

/// Tooltip `open`'s showing ends (`shown` says whether it was showing).
fn hide(open: &crate::reactive::Signal<bool>, shown: &Cell<bool>) {
    if !shown.replace(false) {
        return;
    }
    let left = TOOLTIPS.with(|t| {
        let (count, _) = t.get();
        let left = count.saturating_sub(1);
        t.set((left, Some(Instant::now())));
        left
    });
    if left == 0 {
        let keys = KEYS.with(|k| k.borrow_mut().take());
        drop(keys);
    }
    open.set(false);
}

/// `f` after `delay` on `runner`'s tasks; at once without one.
fn after(
    runner: &Option<Rc<TaskRunner>>,
    delay: Duration,
    f: impl FnOnce() + 'static,
) -> Option<TaskToken> {
    match runner {
        Some(runner) => Some(runner.spawn(Box::pin(async move {
            crate::timer::sleep(delay).await;
            f();
        }))),
        None => {
            f();
            None
        }
    }
}

fn wrap(anchor: View, tip: Tooltip) -> View {
    crate::compose::component(move |cx| {
        let runtime = cx.runtime();
        let runner = cx.try_service::<TaskRunner>();
        let Tooltip {
            content,
            placement,
            interactive,
        } = tip;
        let content = match content {
            Content::View(view) => *view,
            Content::Text(text) => crate::compose::column()
                .px(8.)
                .py(4.)
                .max_w(320.)
                .bg(tok::SURFACE_OVERLAY)
                .border(0.5)
                .border_color(tok::BORDER)
                .rounded(tok::RADIUS_SM)
                .text_size(tok::FONT_SIZE_XS)
                .text_wrap(true)
                .child(crate::compose::text(text)),
        };
        let open = runtime.signal(false);
        let shown = Rc::new(Cell::new(false));
        let ui: Rc<OnceCell<WeakUi>> = Rc::default();
        // Its fade (in over 140 ms, out over 100, both easing out; none
        // when it takes over from another at once).
        let fade = runtime.signal(0_f32);
        let fading: Rc<RefCell<Option<TaskToken>>> = Rc::default();
        let animate = {
            let (fade, fading, runner) = (fade.clone(), fading.clone(), runner.clone());
            move |to: f32, ms: f32, done: Option<Rc<dyn Fn()>>| {
                fading.borrow_mut().take();
                let from = fade.with_untracked(|v| *v);
                let Some(runner) = runner.as_ref().filter(|_| ms > 0. && from != to) else {
                    fade.set(to);
                    if let Some(done) = done {
                        done();
                    }
                    return;
                };
                let start = Instant::now();
                let fade = fade.clone();
                *fading.borrow_mut() = Some(runner.spawn(Box::pin(async move {
                    loop {
                        crate::timer::sleep(Duration::from_millis(8)).await;
                        let t = (start.elapsed().as_secs_f32() * 1000. / ms).min(1.);
                        fade.set(from + (to - from) * (1. - (1. - t).powi(3)));
                        if t >= 1. {
                            break;
                        }
                    }
                    if let Some(done) = done {
                        done();
                    }
                })));
            }
        };
        let number = NEXT.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        let set = {
            let (open, shown, fade, fading, ui) = (
                open.clone(),
                shown.clone(),
                fade.clone(),
                fading.clone(),
                ui.clone(),
            );
            move |on: bool, instant: bool| {
                if !on {
                    SHOWING.with(|s| {
                        let mut s = s.borrow_mut();
                        if s.as_ref().is_some_and(|(n, _, _)| *n == number) {
                            *s = None;
                        }
                    });
                    if !shown.get() {
                        return;
                    }
                    let (open, shown) = (open.clone(), shown.clone());
                    let close: Rc<dyn Fn()> = Rc::new(move || hide(&open, &shown));
                    animate(0., if instant { 0. } else { 100. }, Some(close));
                    return;
                }
                let close: Rc<dyn Fn()> = {
                    let (open, shown, fade, fading) =
                        (open.clone(), shown.clone(), fade.clone(), fading.clone());
                    Rc::new(move || {
                        fading.borrow_mut().take();
                        fade.set(0.);
                        hide(&open, &shown);
                    })
                };
                // One shows at a time: this one closes the last, at once.
                let last = SHOWING.with(|s| s.borrow_mut().replace((number, close, interactive)));
                if let Some((n, close_last, _)) = last
                    && n != number
                {
                    close_last();
                }
                if !shown.replace(true) {
                    let first = TOOLTIPS.with(|t| {
                        let (count, last) = t.get();
                        t.set((count + 1, last));
                        count == 0
                    });
                    if first && let Some(ui) = ui.get().and_then(WeakUi::upgrade) {
                        let keys: Box<dyn Any> = Box::new(ui.listen_document(on_key));
                        KEYS.with(|k| *k.borrow_mut() = Some(keys));
                    }
                    open.set(true);
                }
                animate(1., if instant { 0. } else { 140. }, None);
            }
        };
        let waiting: Rc<RefCell<Option<TaskToken>>> = Rc::default();
        // An interactive card: the pointer may cross from the view to the
        // card (a moment's grace) and stay on either.
        let (over_anchor, over_card) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let leave = {
            let (set, runner, waiting) = (set.clone(), runner.clone(), waiting.clone());
            let (over_anchor, over_card) = (over_anchor.clone(), over_card.clone());
            move || {
                if !interactive || runner.is_none() {
                    waiting.borrow_mut().take();
                    set(false, false);
                    return;
                }
                let (set, over_anchor, over_card) =
                    (set.clone(), over_anchor.clone(), over_card.clone());
                *waiting.borrow_mut() = after(&runner, Duration::from_millis(120), move || {
                    if !over_anchor.get() && !over_card.get() {
                        set(false, false);
                    }
                });
            }
        };
        let anchor = {
            let (leave, over_anchor, ui) = (leave.clone(), over_anchor.clone(), ui.clone());
            anchor
                .on_mount(move |handle| {
                    let _ = ui.set(handle.weak_ui());
                })
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Capture {
                        return;
                    }
                    match cx.event {
                        InputEvent::PointerEnter if cx.target == cx.current_target => {
                            over_anchor.set(true);
                            let now = shown.get()
                                || TOOLTIPS.with(|t| {
                                    let (count, last) = t.get();
                                    count > 0
                                        || last
                                            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
                                });
                            if now || runner.is_none() {
                                // Taking over from another (or back while it
                                // fades): no fade in from nothing.
                                waiting.borrow_mut().take();
                                set(true, !shown.get());
                            } else {
                                let set = set.clone();
                                *waiting.borrow_mut() =
                                    after(&runner, Duration::from_millis(300), move || {
                                        set(true, false)
                                    });
                            }
                        }
                        InputEvent::PointerLeave if cx.target == cx.current_target => {
                            over_anchor.set(false);
                            leave();
                        }
                        InputEvent::PointerDown { .. } | InputEvent::Scroll { .. }
                            if !interactive =>
                        {
                            waiting.borrow_mut().take();
                            set(false, true);
                        }
                        _ => {}
                    }
                })
        };
        let content = if interactive {
            content.on_event(move |cx| {
                if cx.phase == EventPhase::Capture || cx.target != cx.current_target {
                    return;
                }
                match cx.event {
                    InputEvent::PointerEnter => over_card.set(true),
                    InputEvent::PointerLeave => {
                        over_card.set(false);
                        leave();
                    }
                    _ => {}
                }
            })
        } else {
            content
        };
        let placement = placement.unwrap_or(Placement {
            side: PlacementSide::Top,
            align: PlacementEdge::Center,
            offset: 6.,
        });
        let content = crate::compose::column()
            .child(content)
            .reactive_style(move || Styles::new().opacity(fade.get()));
        crate::compose::popover("Tooltip", open, anchor)
            .layer()
            .lazy()
            .autofocus(false)
            .placement(placement)
            .pointer_events(interactive)
            .child(content)
    })
}
