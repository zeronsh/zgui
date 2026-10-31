//! What went wrong that an app goes on through, for whoever keeps its logs:
//! a window's host shows these, and a script host tells `on_error`. A
//! listener hears each issue as it arises, once, not while it lasts.
use crate::reactive::Runtime;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Keeps a function registered with [`on_issue`]: dropped, it hears no more.
#[must_use = "dropping it unregisters the function"]
pub struct IssueListener(#[allow(dead_code)] Rc<Listener>);

type Listener = dyn Fn(IssueKind, &str);

/// What an issue is: who should hear of it, and how loudly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IssueKind {
    /// Something the app should fix: a window shows it in its banner.
    Error,
    /// The app went on with less than asked (a layer drawn coarser, an
    /// image softer, a glyph left out, a frame skipped, a layout that didn't
    /// settle): logged once, shown in its window's banner in debug builds
    /// only.
    Degraded,
}
impl IssueKind {
    /// Its name, as `zgx inspect` and logs give it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Degraded => "degraded",
        }
    }
}

#[derive(Default)]
struct Listeners(RefCell<Vec<Weak<Listener>>>);

/// Calls `listener` with each issue in `runtime`, while the returned
/// [`IssueListener`] lives.
pub fn on_issue(runtime: &Runtime, listener: impl Fn(&str) + 'static) -> IssueListener {
    on_issue_kind(runtime, move |_, message| listener(message))
}

/// [`on_issue`], with each issue's [`IssueKind`].
pub fn on_issue_kind(
    runtime: &Runtime,
    listener: impl Fn(IssueKind, &str) + 'static,
) -> IssueListener {
    let listener: Rc<Listener> = Rc::new(listener);
    let listeners = runtime.extension(|_| Listeners::default());
    listeners.0.borrow_mut().push(Rc::downgrade(&listener));
    IssueListener(listener)
}

/// Tells every listener of `message`, an [`IssueKind::Error`].
pub fn report(runtime: &Runtime, message: &str) {
    report_kind(runtime, IssueKind::Error, message);
}

/// Tells every listener of `message`, of `kind`.
pub fn report_kind(runtime: &Runtime, kind: IssueKind, message: &str) {
    let listeners = runtime.extension(|_| Listeners::default());
    let live: Vec<_> = {
        let mut listeners = listeners.0.borrow_mut();
        listeners.retain(|listener| listener.strong_count() > 0);
        listeners.iter().filter_map(Weak::upgrade).collect()
    };
    for listener in live {
        listener(kind, message);
    }
}
