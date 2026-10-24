//! Toasts: short messages stacked in a corner of the window, over
//! everything, taking no focus, as Radix's and the system's notifications.
//! Anything shows one through the runtime's [`Toaster`]; one
//! [`toast_host`] in the window shows them. Each goes after its duration
//! (paused while the pointer is over it), or at its ×; an error stays
//! until dismissed. Assistive technology hears each as it comes (an error
//! at once). F8 moves focus into the stack, as Radix's does.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zgx::{
    compose::{
        ModalPlacement, NodeHandle, PlacementEdge as Edge, TaskRunner, View, button, column,
        component, div, keyed, modal, row, text,
    },
    input::{EventPhase, InputEvent, Key},
    reactive::{Runtime, Signal},
    scene::Color,
    semantics::{Live, Role},
    style::{Styled, Styles},
    theme::tok,
};

/// What a toast tells: its colour, and how urgently it is announced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastKind {
    /// Something happened (announced politely).
    #[default]
    Info,
    /// Something worked.
    Success,
    /// Something needs attention.
    Warning,
    /// Something failed: announced at once, and stays until dismissed.
    Error,
}

/// A button a toast offers: "Undo", "Retry".
#[derive(Clone)]
pub struct ToastAction {
    /// What it says.
    pub label: String,
    /// What it does; the toast goes after.
    pub run: Rc<dyn Fn()>,
}

/// A toast: a title, a body under it, its kind, how long it stays, and an
/// action.
#[derive(Clone, Default)]
pub struct Toast {
    /// What it says first.
    pub title: String,
    /// More, under the title.
    pub body: Option<String>,
    /// Its colour and urgency.
    pub kind: ToastKind,
    /// How long it stays: 5 seconds by default (7 for a warning); an error
    /// stays until dismissed.
    pub duration: Option<Duration>,
    /// A button on it.
    pub action: Option<ToastAction>,
}
impl Toast {
    /// A toast saying `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }
    /// With `body` under its title.
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
    /// Of `kind`.
    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }
    /// Staying `duration`.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
    /// With a button `label` doing `run`.
    pub fn action(mut self, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            run: Rc::new(run),
        });
        self
    }
    fn stays(&self) -> Option<Duration> {
        match (self.duration, self.kind) {
            (Some(d), _) => Some(d),
            (None, ToastKind::Error) => None,
            (None, ToastKind::Warning) => Some(Duration::from_secs(7)),
            (None, _) => Some(Duration::from_secs(5)),
        }
    }
}

/// A toast showing, by its id.
#[derive(Clone)]
struct Showing {
    id: u64,
    toast: Toast,
}
impl PartialEq for Showing {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// The runtime's toasts (`Toaster::of`): what shows them and what dismisses
/// them, from anywhere.
pub struct Toaster {
    showing: Signal<Vec<Showing>>,
    next: Cell<u64>,
}
impl Toaster {
    /// The runtime's toaster.
    pub fn of(runtime: &Runtime) -> Rc<Toaster> {
        runtime.extension(|runtime| Toaster {
            showing: runtime.signal(Vec::new()),
            next: Cell::new(0),
        })
    }
    /// Show `toast`; its id dismisses it.
    pub fn show(&self, toast: Toast) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.showing.update(|list| list.push(Showing { id, toast }));
        id
    }
    /// Dismiss toast `id`, if it shows.
    pub fn dismiss(&self, id: u64) {
        self.showing.update(|list| list.retain(|s| s.id != id));
    }
    /// Dismiss them all.
    pub fn clear(&self) {
        self.showing.set(Vec::new());
    }
    /// Whether any shows; read in an effect, it follows them.
    pub fn any(&self) -> bool {
        self.showing.with(|list| !list.is_empty())
    }
    /// The toasts showing, oldest first: their ids and titles.
    pub fn showing(&self) -> Vec<(u64, String)> {
        self.showing
            .with(|list| list.iter().map(|s| (s.id, s.toast.title.clone())).collect())
    }
}

/// At most this many show; older ones wait.
const VISIBLE: usize = 4;

/// Where the runtime's toasts show: put one in the window. Its corner is
/// the bottom, at the end of the line (the left, right to left).
pub fn toast_host() -> View {
    component(|cx| {
        let runtime = cx.runtime();
        let toaster = Toaster::of(&runtime);
        let open = runtime.signal(false);
        {
            let (showing, open) = (toaster.showing.clone(), open.clone());
            cx.retain(runtime.effect(move || {
                let any = showing.with(|l| !l.is_empty());
                if open.with_untracked(|o| *o) != any {
                    open.set(any);
                }
            }));
        }
        let stack: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
        let ids = {
            let showing = toaster.showing.clone();
            move || {
                showing.with(|l| {
                    let skip = l.len().saturating_sub(VISIBLE);
                    l.iter().skip(skip).map(|s| s.id).collect::<Vec<_>>()
                })
            }
        };
        let build = {
            let toaster = toaster.clone();
            move |id: u64, cx: &mut zgx::compose::Context| {
                let toast = toaster
                    .showing
                    .with_untracked(|l| l.iter().find(|s| s.id == id).map(|s| s.toast.clone()))
                    .unwrap_or_default();
                toast_view(id, toast, toaster.clone(), cx)
            }
        };
        let focus_into = stack.clone();
        modal("Notifications", open)
            .lazy()
            .layer()
            .autofocus(false)
            .modal_placement(ModalPlacement {
                x: Edge::End,
                y: Edge::End,
                insets: [16.; 4],
            })
            .p(0.)
            .bg(Color(0, 0, 0, 0))
            .border(0.)
            .shadow_none()
            .on_document_event(move |event| {
                // F8: into the stack, as Radix's toasts.
                if let InputEvent::KeyDown {
                    key: Key::Function(8),
                    ..
                } = event
                    && let Some(handle) = focus_into.borrow().clone()
                {
                    // The stack's first focusable node: its newest toast.
                    handle.focus();
                }
            })
            .child(
                column()
                    .w(360.)
                    .gap(8.)
                    .on_mount(move |handle| *stack.borrow_mut() = Some(handle))
                    .child(keyed(ids, build).gap(8.)),
            )
    })
}

/// One toast: its colour, title and body, its action, its ×.
fn toast_view(
    id: u64,
    toast: Toast,
    toaster: Rc<Toaster>,
    cx: &mut zgx::compose::Context,
) -> View {
    let hovered = Rc::new(Cell::new(false));
    // Its time: dismissed once it has passed with the pointer elsewhere (a
    // task of the toast's own, ending as it goes). Without a task runner,
    // toasts stay until dismissed.
    if let Some(stays) = toast.stays()
        && cx.try_service::<TaskRunner>().is_some()
    {
        let (toaster, hovered) = (toaster.clone(), hovered.clone());
        cx.tasks().spawn(async move {
            zgx::timer::sleep(stays).await;
            while hovered.get() {
                zgx::timer::sleep(Duration::from_millis(500)).await;
            }
            toaster.dismiss(id);
        });
    }
    let stripe = match toast.kind {
        ToastKind::Info => tok::INFO,
        ToastKind::Success => tok::SUCCESS,
        ToastKind::Warning => tok::WARNING,
        ToastKind::Error => tok::DANGER,
    };
    let urgent = toast.kind == ToastKind::Error;
    let mut words = column()
        .grow()
        .gap(2.)
        .min_w(0.)
        .child(text(toast.title.clone()).font_weight(600).text_wrap(true));
    if let Some(body) = &toast.body {
        words = words.child(
            text(body.clone())
                .text_color(tok::TEXT_SECONDARY)
                .text_size(tok::FONT_SIZE_SM)
                .text_wrap(true),
        );
    }
    let mut card = row()
        .role(if urgent { Role::Alert } else { Role::Status })
        .live(if urgent {
            Live::Assertive
        } else {
            Live::Polite
        })
        .accessible_label(match &toast.body {
            Some(body) => format!("{}. {}", toast.title, body),
            None => toast.title.clone(),
        })
        .focusable(true)
        .w_full()
        .p(12.)
        .gap(10.)
        .rounded(10.)
        .bg(tok::POPOVER_BG)
        .border(1.)
        .border_color(tok::BORDER)
        .shadow(tok::SHADOW_LG)
        .on_event({
            let (hovered, toaster) = (hovered.clone(), toaster.clone());
            move |cx| {
                if cx.phase == EventPhase::Capture {
                    return;
                }
                match &cx.event {
                    InputEvent::PointerEnter if cx.target == cx.current_target => hovered.set(true),
                    InputEvent::PointerLeave if cx.target == cx.current_target => {
                        hovered.set(false)
                    }
                    InputEvent::KeyDown {
                        key: Key::Escape | Key::Delete,
                        ..
                    } => {
                        toaster.dismiss(id);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }
        })
        .child(
            div()
                .w(3.)
                .shrink_0()
                .rounded(2.)
                .bg(stripe)
                .reactive_style(|| Styles::new().h_full())
                .accessible_hidden(),
        )
        .child(words);
    if let Some(action) = toast.action {
        let toaster = toaster.clone();
        card = card.child(
            button()
                .shrink_0()
                .on_click(move || {
                    (action.run)();
                    toaster.dismiss(id);
                })
                .child(text(action.label)),
        );
    }
    card.child(
        button()
            .accessible_label("Dismiss")
            .shrink_0()
            .w(22.)
            .h(22.)
            .p(0.)
            .rounded(6.)
            .border(0.)
            .bg(Color(0, 0, 0, 0))
            .hover(|s| s.bg(tok::HOVER))
            .items_center()
            .justify_center()
            .on_click(move || toaster.dismiss(id))
            .child(text("×").text_color(tok::TEXT_TERTIARY)),
    )
}
