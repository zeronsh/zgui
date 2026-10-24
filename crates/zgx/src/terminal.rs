//! Embedded terminals: a [`Terminal`] controller and the [`terminal`]
//! component showing it.
//!
//! ```no_run
//! use zgx::{compose::prelude::*, terminal::{Terminal, TerminalOptions, terminal}};
//! fn pane(cx: &mut Context) -> View {
//!     let term = Terminal::new(&cx.runtime(), TerminalOptions::default());
//!     let title = term.clone();
//!     column()
//!         .grow()
//!         .child(text_signal(move || title.title()))
//!         .child(terminal(&term).grow().rounded(10.))
//! }
//! ```
//!
//! The terminal is the platform's own view (a Ghostty surface on macOS, with
//! the desktop host's `ghostty` feature), placed like a web page: laid out,
//! clipped, moved and faded as its node, with zgx content drawn over it (see
//! [`crate::native_view`]). It renders, reads the keyboard (IME included) and
//! the mouse itself. Without a backend the node shows a labelled placeholder.
//!
//! The controller owns the session: unmounting hides the terminal and keeps
//! its process running; mounting it again (another split, another tab)
//! shows it there. The process ends when the controller drops or on
//! [`Terminal::close`]. State is reactive; platform events reach
//! [`Terminal::on_event`] subscribers on the UI thread at a safe point.
use crate::{
    compose::View,
    native_view::NativeContent,
    reactive::{Runtime, Signal},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
};

/// How a terminal session starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalOptions {
    /// The command line to run; `None` runs the user's login shell.
    pub command: Option<String>,
    /// Where it starts; `None` is the home directory.
    pub working_directory: Option<PathBuf>,
    /// Added to the inherited environment.
    pub env: Vec<(String, String)>,
    /// Points; `None` uses the configured size.
    pub font_size: Option<f32>,
}

/// A terminal session's state, kept by the controller.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalState {
    /// The title the running program set (OSC 0/2); empty until one does.
    pub title: String,
    /// The working directory the shell reported (OSC 7); empty until it does.
    pub pwd: String,
    /// The foreground process's name (`zsh`, `nvim`, `claude`); empty
    /// before the first is known.
    pub process: String,
    /// The foreground process's id (0: unknown).
    pub pid: u32,
    /// The terminal has keyboard focus.
    pub focused: bool,
    /// The bell rang since the terminal last had focus.
    pub bell: bool,
    /// A program's progress report (OSC 9;4): 0 to 1, or negative while
    /// indeterminate; `None` without one.
    pub progress: Option<f32>,
    /// The progress report is in its error state (OSC 9;4;2).
    pub progress_error: bool,
    /// What the running program says it is doing (the Program Status
    /// Protocol, OSC 7501), when it says; see [`ProgramStatus`].
    pub status: Option<ProgramStatus>,
    /// The process ended, with this exit code.
    pub exited: Option<i32>,
    /// Grid width in cells.
    pub columns: u16,
    /// Grid height in cells.
    pub rows: u16,
}

/// A program's own report of its state (the Program Status Protocol, OSC
/// 7501, as Rex defines it): its root record, and whether any record under
/// it (a worker) is blocked.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProgramStatus {
    /// The root record's state.
    pub state: ProgramState,
    /// With [`ProgramState::Blocked`]: what it waits for (`permission`,
    /// `question`, `auth`).
    pub kind: Option<String>,
    /// 0 to 100, while working or blocked; `None`: indeterminate.
    pub progress: Option<u8>,
    /// The program's stable identifier (`claude-code`), not a label.
    pub app: Option<String>,
    /// A short label, as the program wrote it.
    pub title: Option<String>,
    /// One line of detail, as the program wrote it.
    pub msg: Option<String>,
    /// A record under the root is blocked, whatever the root says.
    pub child_blocked: bool,
    /// When the root record last changed state (Unix milliseconds).
    pub since_ms: u64,
}
/// A program's state, as the Program Status Protocol names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProgramState {
    /// At rest, waiting for the user's next instruction.
    #[default]
    Idle,
    /// Busy on a task.
    Working,
    /// Finished; the result is ready and not yet seen.
    Done,
    /// Cannot continue until the user does something (`kind` says what).
    Blocked,
    /// Failed and stopped.
    Error,
}
impl ProgramState {
    /// Its protocol name: `idle`, `working`, `done`, `blocked` or `error`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }
    /// The state `name` names (see [`ProgramState::name`]); `None` for any other.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "done" => Self::Done,
            "blocked" => Self::Blocked,
            "error" => Self::Error,
            _ => return None,
        })
    }
}

/// An event from a terminal session's platform terminal (see
/// [`Terminal::on_event`]).
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEvent {
    /// The running program set its title (OSC 0/2).
    TitleChanged {
        /// The new title.
        title: String,
    },
    /// The shell reported its working directory (OSC 7).
    PwdChanged {
        /// The new working directory.
        pwd: String,
    },
    /// The foreground process changed (a command started or finished).
    ProcessChanged {
        /// The foreground process's name.
        process: String,
        /// Its process id (0: unknown).
        pid: u32,
    },
    /// The bell rang.
    Bell,
    /// A shell-integrated command finished (OSC 133): its exit code (when
    /// reported) and how long it ran.
    CommandFinished {
        /// Its exit code, when reported.
        exit_code: Option<i32>,
        /// How long it ran, in milliseconds.
        duration_ms: u64,
    },
    /// A desktop notification the program asked for (OSC 9/777).
    Notification {
        /// The notification's title.
        title: String,
        /// Its body text.
        body: String,
    },
    /// The terminal asks to close (its process exited, or a close binding):
    /// whether a process other than an idle shell is still running.
    CloseRequested {
        /// Whether a process other than an idle shell is still running.
        process_alive: bool,
    },
    /// The process ended.
    Exited {
        /// Its exit code.
        code: i32,
    },
}

/// What a controller asks of its platform terminal.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalCommand {
    /// Give the terminal keyboard focus, or take it back (`false`).
    Focus(bool),
    /// Send text as if typed.
    Input(String),
    /// Paste text (bracketed when the program asked for it).
    Paste(String),
    /// Run a Ghostty binding action (`copy_to_clipboard`, `clear_screen`,
    /// `increase_font_size:1`, `scroll_to_bottom`, ...).
    Binding(String),
    /// End the session: the process is hung up and the view goes.
    Close,
}

/// A platform's terminal (implemented by the desktop host).
pub trait TerminalBackend {
    /// Carry out `command`.
    fn run(&self, command: TerminalCommand);
    /// Whether closing would end something other than an idle shell.
    fn needs_confirm_close(&self) -> bool {
        false
    }
    /// The backend as `Any`, for the host to downcast to its own type.
    fn as_any(&self) -> &dyn Any;
}

type Listener = Rc<RefCell<dyn FnMut(&TerminalEvent)>>;
struct Inner {
    id: u64,
    /// How it starts; settable until a platform terminal starts it.
    options: RefCell<TerminalOptions>,
    /// A platform terminal started it (its process runs with `options`).
    started: Cell<bool>,
    state: Signal<TerminalState>,
    /// The pointer over the terminal, in points from its top left.
    pointer: Signal<Option<(f32, f32)>>,
    attached: Signal<bool>,
    closed: Cell<bool>,
    backend: RefCell<Option<Rc<dyn TerminalBackend>>>,
    queued: RefCell<Vec<TerminalCommand>>,
    listeners: RefCell<Vec<(u64, Listener)>>,
    next_listener: Cell<u64>,
}
/// A terminal session's controller; clones share it.
#[derive(Clone)]
pub struct Terminal(Rc<Inner>);
/// A weak controller reference, for platform callbacks.
#[derive(Clone)]
pub struct WeakTerminal(Weak<Inner>);
impl WeakTerminal {
    /// The controller, if it still exists.
    pub fn upgrade(&self) -> Option<Terminal> {
        self.0.upgrade().map(Terminal)
    }
}
/// Keeps an event subscription; dropping it unsubscribes.
pub struct TerminalSubscription {
    term: Weak<Inner>,
    id: u64,
}
impl Drop for TerminalSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.term.upgrade()
            && let Ok(mut listeners) = inner.listeners.try_borrow_mut()
        {
            listeners.retain(|(id, _)| *id != self.id);
        }
    }
}
thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}
impl PartialEq for Terminal {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal").field("id", &self.0.id).finish()
    }
}
impl Terminal {
    /// A controller whose session starts with `options` once a platform terminal is
    /// attached; its state lives in `runtime`.
    pub fn new(runtime: &Runtime, options: TerminalOptions) -> Self {
        let id = NEXT_ID.with(|next| {
            let id = next.get() + 1;
            next.set(id);
            id
        });
        Self(Rc::new(Inner {
            id,
            options: RefCell::new(options),
            started: Cell::new(false),
            state: runtime.signal(TerminalState::default()),
            pointer: runtime.signal(None),
            attached: runtime.signal(false),
            closed: Cell::new(false),
            backend: RefCell::new(None),
            queued: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
            next_listener: Cell::new(0),
        }))
    }
    /// Unique per controller in this thread.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// How the session starts, or started.
    pub fn options(&self) -> TerminalOptions {
        self.0.options.borrow().clone()
    }
    /// Change how it starts, before a platform terminal has started it
    /// (one made it with defaults, before its owner gave its command).
    /// Returns whether they apply: a started terminal keeps its own.
    pub fn set_options(&self, options: TerminalOptions) -> bool {
        if self.0.started.get() || self.0.closed.get() {
            return false;
        }
        *self.0.options.borrow_mut() = options;
        true
    }
    /// A weak reference, which does not keep the session alive.
    pub fn downgrade(&self) -> WeakTerminal {
        WeakTerminal(Rc::downgrade(&self.0))
    }
    /// The session state, reactive.
    pub fn state(&self) -> Signal<TerminalState> {
        self.0.state.clone()
    }
    /// The title the running program set (reactive); empty until one does.
    pub fn title(&self) -> String {
        self.0.state.with(|state| state.title.clone())
    }
    /// The working directory the shell reported (reactive); empty until it does.
    pub fn pwd(&self) -> String {
        self.0.state.with(|state| state.pwd.clone())
    }
    /// The foreground process's name (reactive); empty until it is known.
    pub fn process(&self) -> String {
        self.0.state.with(|state| state.process.clone())
    }
    /// Whether the terminal has keyboard focus (reactive).
    pub fn focused(&self) -> bool {
        self.0.state.with(|state| state.focused)
    }
    /// The process's exit code, once it has ended (reactive).
    pub fn exited(&self) -> Option<i32> {
        self.0.state.with(|state| state.exited)
    }
    /// Where the pointer is over the terminal, in points from its top left;
    /// `None` outside it (reactive, its own signal: following it does not
    /// rerun what reads the rest of the state).
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.0.pointer.get()
    }
    /// Report the pointer (the platform's report; call from a delivery).
    pub fn set_pointer(&self, pointer: Option<(f32, f32)>) {
        if self.0.pointer.with_untracked(|p| *p != pointer) {
            self.0.pointer.set(pointer);
        }
    }
    /// Whether a platform terminal exists for it (reactive).
    pub fn is_attached(&self) -> bool {
        self.0.attached.get()
    }
    /// Whether [`Terminal::close`] ran: hosts make no new platform terminal.
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    fn run(&self, command: TerminalCommand) {
        let backend = self.0.backend.borrow().clone();
        match backend {
            Some(backend) => backend.run(command),
            None => self.0.queued.borrow_mut().push(command),
        }
    }
    /// Give the terminal keyboard focus.
    pub fn focus(&self) {
        self.run(TerminalCommand::Focus(true));
    }
    /// Take keyboard focus back from the terminal.
    pub fn blur(&self) {
        self.run(TerminalCommand::Focus(false));
    }
    /// Send `text` as if typed (`"ls\r"` runs `ls`).
    pub fn input(&self, text: impl Into<String>) {
        self.run(TerminalCommand::Input(text.into()));
    }
    /// Paste `text`, bracketed when the program asked for bracketed paste.
    pub fn paste(&self, text: impl Into<String>) {
        self.run(TerminalCommand::Paste(text.into()));
    }
    /// Run a Ghostty binding action, e.g. `copy_to_clipboard`.
    pub fn binding(&self, action: impl Into<String>) {
        self.run(TerminalCommand::Binding(action.into()));
    }
    /// End the session now (no confirmation; see
    /// [`Terminal::needs_confirm_close`]).
    pub fn close(&self) {
        if self.0.closed.replace(true) {
            return;
        }
        self.0.queued.borrow_mut().clear();
        if let Some(backend) = self.detach() {
            backend.run(TerminalCommand::Close);
        }
    }
    /// Whether closing would end something other than an idle shell.
    pub fn needs_confirm_close(&self) -> bool {
        let backend = self.0.backend.borrow().clone();
        backend.is_some_and(|backend| backend.needs_confirm_close())
    }
    /// Call `listener` with each platform event, until the subscription drops.
    pub fn on_event(&self, listener: impl FnMut(&TerminalEvent) + 'static) -> TerminalSubscription {
        let id = self.0.next_listener.get() + 1;
        self.0.next_listener.set(id);
        self.0
            .listeners
            .borrow_mut()
            .push((id, Rc::new(RefCell::new(listener))));
        TerminalSubscription {
            term: Rc::downgrade(&self.0),
            id,
        }
    }

    // The platform side.

    /// Attach the platform terminal; queued commands run now, in order.
    pub fn attach(&self, backend: Rc<dyn TerminalBackend>) {
        self.0.started.set(true);
        *self.0.backend.borrow_mut() = Some(backend.clone());
        let queued = std::mem::take(&mut *self.0.queued.borrow_mut());
        for command in queued {
            backend.run(command);
        }
        self.0.attached.set(true);
    }
    /// Drop the platform terminal (it goes once nothing else holds it).
    pub fn detach(&self) -> Option<Rc<dyn TerminalBackend>> {
        let backend = self.0.backend.borrow_mut().take();
        if backend.is_some() {
            self.0.attached.set(false);
        }
        backend
    }
    /// The attached platform terminal, if any.
    pub fn backend(&self) -> Option<Rc<dyn TerminalBackend>> {
        self.0.backend.borrow().clone()
    }
    /// Update the state (the platform's report). Call from a delivery
    /// ([`crate::native_view::post`]), not from inside a platform callback.
    pub fn update_state(&self, change: impl FnOnce(&mut TerminalState)) {
        self.0.state.update(change);
    }
    /// Deliver `event` to subscribers. Call from a delivery, as above.
    pub fn emit(&self, event: TerminalEvent) {
        let listeners: Vec<Listener> = self
            .0
            .listeners
            .borrow()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            if let Ok(mut listener) = listener.try_borrow_mut() {
                listener(&event);
            }
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        // The session goes with its controller.
        if let Some(backend) = self.backend.get_mut().take() {
            backend.run(TerminalCommand::Close);
        }
    }
}

/// Show `term` in a node laid out like any other: give it a size (or let it
/// grow), round it, clip it, move and fade it.
pub fn terminal(term: &Terminal) -> View {
    crate::web_view::native_view(NativeContent::Terminal(term.clone()))
}
