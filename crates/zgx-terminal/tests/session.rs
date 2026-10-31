//! Scripted sessions on real pseudoterminals: what programs write lands in
//! exact cells, input reaches them, and their reports come back as events.
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use zgx_terminal::{
    GridSize, Session, SessionEvent, SessionOptions, Term,
    alacritty_terminal::{
        grid::Dimensions,
        index::{Column, Line},
        term::cell::Flags,
    },
};
#[cfg(unix)]
use zgx_terminal::{
    Progress,
    alacritty_terminal::{
        index::Point,
        vte::ansi::{Color, NamedColor},
    },
};

/// A session and everything it reported so far.
struct Scripted {
    session: Session,
    woken: Arc<(Mutex<bool>, Condvar)>,
    events: Vec<SessionEvent>,
}
impl Scripted {
    fn run(command: &str, columns: u16, rows: u16) -> Self {
        let woken = Arc::new((Mutex::new(false), Condvar::new()));
        let wake = woken.clone();
        let session = Session::spawn(
            &SessionOptions {
                command: Some(command.into()),
                ..Default::default()
            },
            GridSize::new(columns, rows),
            move || {
                *wake.0.lock().unwrap() = true;
                wake.1.notify_all();
            },
        )
        .expect("a pty");
        Self {
            session,
            woken,
            events: Vec::new(),
        }
    }
    /// Read what the session reports until `done` holds, as the app does
    /// when woken.
    fn until(&mut self, what: &str, mut done: impl FnMut(&[SessionEvent], &Term) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.events.extend(self.session.take_events());
            if done(&self.events, &self.session.term()) {
                return;
            }
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out waiting for {what}: {:?}\n{}",
                self.events,
                self.screen()
            );
            let (lock, wake) = &*self.woken;
            let mut woken = lock.lock().unwrap();
            if !*woken {
                woken = wake.wait_timeout(woken, deadline - now).unwrap().0;
            }
            *woken = false;
        }
    }
    fn exited(&mut self) -> i32 {
        self.until("the exit", |events, _| exit_code(events).is_some());
        exit_code(&self.events).unwrap()
    }
    fn screen(&self) -> String {
        rows(&self.session.term()).join("\n")
    }
}
fn exit_code(events: &[SessionEvent]) -> Option<i32> {
    events.iter().find_map(|event| match event {
        SessionEvent::Exited(code) => Some(*code),
        _ => None,
    })
}
/// The visible rows' text, trailing blanks trimmed (a wide character's
/// spacer cell left out).
fn rows(term: &Term) -> Vec<String> {
    let grid = term.grid();
    (0..grid.screen_lines() as i32)
        .map(|line| {
            let row = &grid[Line(line)];
            let text: String = (0..grid.columns())
                .map(|column| &row[Column(column)])
                .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                .map(|cell| cell.c)
                .collect();
            text.trim_end().to_owned()
        })
        .collect()
}
fn shows(term: &Term, line: &str) -> bool {
    rows(term).iter().any(|row| row == line)
}

#[cfg(unix)]
#[test]
fn output_lands_in_exact_cells() {
    let mut run = Scripted::run(
        r"printf 'hello\r\n\033[1;31mred\033[0m \033[38;2;1;2;3mtrue\033[0m wide:\344\270\255\346\226\207!\r\n'",
        20,
        4,
    );
    assert_eq!(run.exited(), 0);
    let term = run.session.term();
    assert_eq!(rows(&term), ["hello", "red true wide:中文!", "", ""]);
    let grid = term.grid();
    let red = &grid[Point::new(Line(1), Column(0))];
    assert_eq!(red.c, 'r');
    assert_eq!(red.fg, Color::Named(NamedColor::Red));
    assert!(red.flags.contains(Flags::BOLD));
    let truecolor = &grid[Point::new(Line(1), Column(4))];
    assert!(matches!(truecolor.fg, Color::Spec(rgb) if (rgb.r, rgb.g, rgb.b) == (1, 2, 3)));
    // "red true wide:" is 14 columns: the wide characters take two each.
    let wide = &grid[Point::new(Line(1), Column(14))];
    assert_eq!(wide.c, '中');
    assert!(wide.flags.contains(Flags::WIDE_CHAR));
    assert!(
        grid[Point::new(Line(1), Column(15))]
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
    );
    assert_eq!(grid[Point::new(Line(1), Column(16))].c, '文');
    assert_eq!(grid[Point::new(Line(1), Column(18))].c, '!');
}

#[cfg(unix)]
#[test]
fn input_reaches_the_program_and_it_echoes() {
    let mut run = Scripted::run("cat", 20, 4);
    run.session.write(b"abc\r".as_slice());
    run.until("the echo and cat's copy", |_, term| {
        rows(term)[..2] == ["abc", "abc"]
    });
    // ^D at a line's start: end of input.
    run.session.write(b"\x04".as_slice());
    assert_eq!(run.exited(), 0);
}

#[cfg(unix)]
#[test]
fn ctrl_c_interrupts_the_foreground_program() {
    let mut run = Scripted::run("sh -c 'echo ready; sleep 30'", 20, 4);
    run.until("ready", |_, term| shows(term, "ready"));
    run.session.write(b"\x03".as_slice());
    // SIGINT: 128 + 2.
    assert_eq!(run.exited(), 130);
}

#[cfg(unix)]
#[test]
fn resizing_tells_the_program() {
    let mut run = Scripted::run("sh -c 'stty size; read line; stty size'", 40, 5);
    run.until("the first size", |_, term| shows(term, "5 40"));
    run.session.resize(GridSize::new(50, 7));
    run.session.write(b"\r".as_slice());
    run.until("the new size", |_, term| shows(term, "7 50"));
    assert_eq!(run.exited(), 0);
    let term = run.session.term();
    assert_eq!((term.grid().columns(), term.grid().screen_lines()), (50, 7));
}

#[cfg(unix)]
#[test]
fn exit_codes_and_reports_come_back_as_events() {
    let mut run = Scripted::run(
        r"printf '\033]2;Building\007\033]7;file://host/tmp/a%%20b\007\007\033]133;C\007\033]9;4;1;42\007\033]133;D;3\007\033]777;notify;CI;green\007'; exit 3",
        20,
        4,
    );
    assert_eq!(run.exited(), 3);
    let events: Vec<String> = run
        .events
        .iter()
        .map(|event| match event {
            SessionEvent::CommandFinished { exit_code, .. } => format!("finished {exit_code:?}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        events,
        [
            r#"Title("Building")"#,
            r#"Pwd("/tmp/a b")"#,
            "Bell",
            &format!("{:?}", SessionEvent::Progress(Progress::Set(42))),
            "finished Some(3)",
            r#"Notification { title: "CI", body: "green" }"#,
            "Exited(3)",
        ]
    );
}

#[cfg(unix)]
#[test]
fn queries_are_answered_and_the_foreground_is_known() {
    // Device attributes (DA1): the screen model answers through the pty.
    let mut run = Scripted::run(
        r#"exec sh -c 'stty raw -echo; printf "\033[c"; dd bs=1 count=4 2>/dev/null | od -An -c | tr -d " \n"; echo; exec sleep 30'"#,
        30,
        4,
    );
    run.until("the answer", |_, term| shows(term, r"033[?6"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while zgx_terminal::process_name(run.session.foreground_pid()) != "sleep" {
        assert!(Instant::now() < deadline, "sleep never came to the front");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
#[test]
fn a_windows_program_writes_exact_cells_and_exits() {
    let mut run = Scripted::run("cmd.exe /d /c echo hello& exit 3", 30, 4);
    assert_eq!(run.exited(), 3);
    assert!(shows(&run.session.term(), "hello"), "{}", run.screen());
}

#[cfg(windows)]
#[test]
fn a_windows_program_gets_input_and_resizes() {
    let mut run = Scripted::run("cmd.exe /d /q /k prompt $G", 40, 5);
    run.session.write(b"echo typed\r".as_slice());
    run.until("the echo", |_, term| shows(term, "typed"));
    run.session.resize(GridSize::new(60, 8));
    run.session.write(b"mode con\r".as_slice());
    run.until("the new size", |_, term| {
        rows(term)
            .iter()
            .any(|row| row.trim_start().starts_with("Columns:") && row.ends_with("60"))
    });
    run.session.write(b"exit 7\r".as_slice());
    assert_eq!(run.exited(), 7);
}

#[cfg(windows)]
#[test]
fn ctrl_c_reaches_a_windows_program() {
    // As when an app starts from a parent ignoring Ctrl+C (an agent's
    // shell): inherited, the shell would never get it.
    // SAFETY: no handler, TRUE: this process ignores Ctrl+C.
    unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1) };
    let mut run = Scripted::run("cmd.exe /d /c ping -t 127.0.0.1", 60, 6);
    run.until("ping running", |_, term| {
        rows(term).iter().any(|row| row.starts_with("Pinging"))
    });
    run.session.write(b"\x03".as_slice());
    // ping stops with its statistics and cmd /c ends with it.
    run.exited();
    assert!(
        shows_prefix(&run.session.term(), "Ping statistics"),
        "{}",
        run.screen()
    );
}
#[cfg(windows)]
fn shows_prefix(term: &Term, prefix: &str) -> bool {
    rows(term)
        .iter()
        .any(|row| row.trim_start().starts_with(prefix))
}
