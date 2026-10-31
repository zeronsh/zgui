//! The terminal zgx draws, headless: a real program's output in its rows,
//! typing reaching the program, its reports in the controller's state and
//! events, and the session outliving the view that showed it.
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    compose::{TaskRunner, prelude::*},
    scene::NodeKind,
    task::LocalExecutor,
    terminal::{Terminal, TerminalOptions},
    widgets::Ui,
};
#[cfg(unix)]
use zgx::{
    input::{InputEvent, Key, Modifiers},
    terminal::TerminalEvent,
};

struct Harness {
    ui: Ui,
    executor: Rc<RefCell<LocalExecutor>>,
}
impl Harness {
    fn new() -> Self {
        Self {
            ui: Ui::new(640., 360.),
            executor: Rc::new(RefCell::new(LocalExecutor::new())),
        }
    }
    fn mount(&mut self, view: View) {
        self.ui.mount(provide(
            TaskRunner::from_executor(self.executor.clone()),
            view,
        ));
        self.ui.prepare_frame();
    }
    /// Run what is due (the session's wakes, posted deliveries, a frame).
    fn pump(&mut self) {
        while self.executor.borrow_mut().tick() > 0 {}
        zgx::native_view::deliver_posted();
        self.ui.prepare_frame();
    }
    fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:?}",
                self.texts()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    /// The text the rows draw, run by run, in order.
    fn texts(&self) -> Vec<String> {
        let scene = self.ui.scene.borrow();
        scene
            .paint_items()
            .filter_map(|item| match scene.kind(item.id) {
                NodeKind::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }
    fn shows(&self, text: &str) -> bool {
        self.texts().iter().any(|shown| shown == text)
    }
}

fn session(command: &str) -> TerminalOptions {
    TerminalOptions {
        command: Some(command.into()),
        ..Default::default()
    }
}

#[cfg(unix)]
#[test]
fn output_shows_in_rows_and_reports_reach_the_controller() {
    let mut app = Harness::new();
    let term = Terminal::new(
        &app.ui.runtime,
        session(r"printf '\033]2;Building\007hello world\r\n\033[31mred\033[0m\r\n'; sleep 30"),
    );
    // As a desktop host with the `terminal` feature provides it.
    let renderer = zgx::terminal::TerminalRenderer::new(zgx_terminal::terminal_view);
    app.mount(provide(
        renderer,
        zgx::terminal::terminal(&term).w(600.).h(300.),
    ));
    app.until("the output", |app| {
        app.shows("hello world") && app.shows("red")
    });
    assert_eq!(term.title(), "Building");
    let state = term.state().get();
    assert!(state.columns > 10 && state.rows > 5, "{state:?}");
    assert!(zgx_terminal::is_engine_drawn(&term));
    // The shell running it is in front (`sh -c` doesn't take its
    // commands to the front).
    app.until("the foreground process", |_| {
        term.process() == "sh" && term.state().get().pid > 0
    });
}

#[cfg(unix)]
#[test]
fn typing_reaches_the_program() {
    let mut app = Harness::new();
    let term = Terminal::new(&app.ui.runtime, session("cat"));
    app.mount(zgx_terminal::terminal_view(&term).w(600.).h(300.));
    term.focus();
    app.until("focus", |_| term.focused());
    app.ui.dispatch(InputEvent::Text("abc".into()));
    app.ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // The tty's echo, then cat's copy.
    app.until("the echo and the copy", |app| {
        app.texts().iter().filter(|t| *t == "abc").count() == 2
    });
    // Control chords reach it as control characters: ^D ends cat.
    let events = Rc::new(RefCell::new(Vec::new()));
    let heard = events.clone();
    let _subscription = term.on_event(move |event| heard.borrow_mut().push(event.clone()));
    app.ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("d".into()),
        modifiers: Modifiers {
            control: true,
            ..Modifiers::default()
        },
        repeat: false,
    });
    app.until("the exit", |_| term.exited().is_some());
    assert_eq!(term.exited(), Some(0));
    assert!(
        events.borrow().iter().any(|event| matches!(
            event,
            TerminalEvent::CloseRequested {
                process_alive: false
            }
        )),
        "{:?}",
        events.borrow()
    );
}

#[cfg(unix)]
#[test]
fn the_session_outlives_its_view() {
    let mut app = Harness::new();
    let term = Terminal::new(&app.ui.runtime, session(r"printf 'kept\r\n'; sleep 30"));
    let shown = app.ui.runtime.signal(true);
    let (view_term, view_shown) = (term.clone(), shown.clone());
    app.mount(switch(
        move || view_shown.get(),
        move |showing, _| {
            if showing {
                zgx_terminal::terminal_view(&view_term).w(600.).h(300.)
            } else {
                text("elsewhere")
            }
        },
    ));
    app.until("the output", |app| app.shows("kept"));
    let backend = term.backend().expect("a backend");
    shown.set(false);
    app.pump();
    assert!(!app.shows("kept"), "{:?}", app.texts());
    assert!(term.exited().is_none(), "unmounting keeps the session");
    shown.set(true);
    app.until("the output again", |app| app.shows("kept"));
    assert!(
        Rc::ptr_eq(&backend, &term.backend().unwrap()),
        "the same session"
    );
    term.close();
    assert!(term.backend().is_none());
}

#[cfg(unix)]
#[test]
fn an_apps_resolver_names_what_is_in_front() {
    // A session server's client in front: the app says what is behind it.
    zgx_terminal::front::resolve_with(|pid| {
        (pid != 0).then(|| zgx_terminal::Foreground {
            name: "claude".into(),
            pid: 4242,
            idle: false,
            status: Some(zgx::terminal::ProgramStatus {
                state: zgx::terminal::ProgramState::Blocked,
                kind: Some("permission".into()),
                ..Default::default()
            }),
        })
    });
    zgx_terminal::front::set_pushed(true);
    let mut app = Harness::new();
    let term = Terminal::new(&app.ui.runtime, session("sleep 30"));
    app.mount(zgx_terminal::terminal_view(&term).w(600.).h(300.));
    app.until("the resolved process", |_| term.process() == "claude");
    let state = term.state().get();
    assert_eq!(state.pid, 4242);
    assert_eq!(
        state.status.map(|status| status.state),
        Some(zgx::terminal::ProgramState::Blocked)
    );
    assert!(term.needs_confirm_close(), "it isn't idle");
    // The app says it changed: each terminal asks again.
    zgx_terminal::front::resolve_with(|pid| {
        (pid != 0).then(|| zgx_terminal::Foreground {
            name: "zsh".into(),
            pid: 4243,
            idle: true,
            status: None,
        })
    });
    zgx_terminal::front::changed();
    app.until("the new process", |_| term.process() == "zsh");
    assert!(!term.needs_confirm_close());
}

#[cfg(windows)]
#[test]
fn a_windows_programs_output_shows_in_rows() {
    let mut app = Harness::new();
    let term = Terminal::new(
        &app.ui.runtime,
        session("cmd.exe /d /c echo hello world& ping -n 30 127.0.0.1 >nul"),
    );
    app.mount(zgx_terminal::terminal_view(&term).w(600.).h(300.));
    app.until("the output", |app| app.shows("hello world"));
    let state = term.state().get();
    assert!(state.columns > 10 && state.rows > 5, "{state:?}");
    assert!(term.needs_confirm_close(), "ping runs in front");
}
