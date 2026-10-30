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

type Listener = dyn Fn(&str);

#[derive(Default)]
struct Listeners(RefCell<Vec<Weak<Listener>>>);

/// Calls `listener` with each issue in `runtime`, while the returned
/// [`IssueListener`] lives.
pub fn on_issue(runtime: &Runtime, listener: impl Fn(&str) + 'static) -> IssueListener {
    let listener: Rc<Listener> = Rc::new(listener);
    let listeners = runtime.extension(|_| Listeners::default());
    listeners.0.borrow_mut().push(Rc::downgrade(&listener));
    IssueListener(listener)
}

/// Tells every listener of `message`.
pub(crate) fn report(runtime: &Runtime, message: &str) {
    let listeners = runtime.extension(|_| Listeners::default());
    let live: Vec<_> = {
        let mut listeners = listeners.0.borrow_mut();
        listeners.retain(|listener| listener.strong_count() > 0);
        listeners.iter().filter_map(Weak::upgrade).collect()
    };
    for listener in live {
        listener(message);
    }
}
