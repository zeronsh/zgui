//! What runs in a terminal when its own child only relays: an app that keeps
//! shells in a session server attaches each terminal to one with a small
//! client, so the terminal's foreground process is that client. The app says
//! what is really in front ([`resolve_terminal_processes`]), and terminals
//! name themselves (and ask before closing) by that instead.
use std::cell::{Cell, RefCell};

/// The program in front of a terminal's session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForegroundProcess {
    /// As it presents itself (`claude`, `vim`, `zsh`).
    pub name: String,
    /// Its process ID.
    pub pid: u32,
    /// At rest: the session's shell itself is in front, nothing running in
    /// it that closing would end.
    pub idle: bool,
    /// What the program reports it is doing (OSC 7501), when it reports.
    pub status: Option<zgui::terminal::ProgramStatus>,
}

type Resolver = Box<dyn Fn(u32) -> Option<ForegroundProcess>>;
thread_local! {
    static RESOLVER: RefCell<Option<Resolver>> = const { RefCell::new(None) };
    static PUSHED: Cell<bool> = const { Cell::new(false) };
}

/// Install how to see through a terminal's foreground process (given its
/// pid): `Some` replaces what the terminal reports, `None` keeps it. Called
/// on the UI thread, about twice a second per terminal (unless the app says
/// when it changes: [`terminal_processes_pushed`]) and when one is asked to
/// close.
pub fn resolve_terminal_processes(resolver: impl Fn(u32) -> Option<ForegroundProcess> + 'static) {
    RESOLVER.with(|slot| *slot.borrow_mut() = Some(Box::new(resolver)));
}

/// Whether the app hears every change to what is in front of the terminals
/// it resolves and says so ([`terminal_processes_changed`]): those are not
/// polled then, and nothing runs while nothing changes. Others still are.
/// On the UI thread.
pub fn terminal_processes_pushed(pushed: bool) {
    if PUSHED.replace(pushed) && !pushed {
        terminal_processes_changed();
    }
}

/// What is in front of the terminals may have changed: each asks the
/// resolver again now. From any thread.
pub fn terminal_processes_changed() {
    #[cfg(all(target_os = "macos", feature = "ghostty"))]
    crate::native_host::processes_changed();
}

#[allow(dead_code)]
pub(crate) fn installed() -> bool {
    RESOLVER.with(|slot| slot.borrow().is_some())
}
#[allow(dead_code)]
pub(crate) fn pushed() -> bool {
    PUSHED.get()
}
#[allow(dead_code)]
pub(crate) fn resolve(pid: u32) -> Option<ForegroundProcess> {
    RESOLVER.with(|slot| slot.borrow().as_ref().and_then(|resolve| resolve(pid)))
}
