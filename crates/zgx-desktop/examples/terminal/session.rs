//! Shells that outlive the window: a session server, the same binary run
//! with `--session-daemon`, owns every pane's pseudo-terminal and shell.
//! Each pane's Ghostty surface runs this binary with `--attach <id>`, a thin
//! client relaying its terminal to the session `<id>` (made the first time),
//! so closing the window detaches and the next window attaches again.
//!
//! The server keeps each session's screen (and some scrollback) with `vt100`
//! and redraws it for a client that attaches, then streams the shell's
//! output as it comes. It answers what runs in front of a session (the app
//! names panes by it), ends sessions the app closes, and exits once it has
//! had none for a while.
//!
//! Frames on the socket: a kind byte, a little-endian u32 length, the
//! payload. JSON for control; raw bytes for input and output.
use std::{
    collections::HashMap,
    ffi::CString,
    io::{self, Read, Write},
    os::{
        fd::RawFd,
        unix::{
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const HELLO: u8 = 1;
const INPUT: u8 = 2;
const RESIZE: u8 = 3;
const INFO: u8 = 4;
const KILL: u8 = 5;
const KEEP: u8 = 6;
/// A window looked at a session (focused its pane): what it finished is seen.
const SEEN: u8 = 7;
/// Stay connected and hear every session's activity as it changes.
const WATCH: u8 = 8;
const OUTPUT: u8 = 10;
const EXIT: u8 = 11;
const REPLY: u8 = 12;

/// Lines of scrollback a session keeps to redraw.
const SCROLLBACK: usize = 2000;
/// How long the server waits with no sessions before it exits.
const IDLE_EXIT: Duration = Duration::from_secs(30);

/// Where the app keeps its state (`ZGX_TERMINAL_DIR` to keep another, as
/// a second instance or a test does).
pub fn dir() -> PathBuf {
    if let Some(dir) = zgx::env::var_os("ZGX_TERMINAL_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    home.join("Library/Application Support/zgx-terminal")
}
fn socket_path() -> PathBuf {
    dir().join("sessions.sock")
}

fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    frame.push(kind);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    w.write_all(&frame)
}
fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > 64 << 20 {
        return Err(io::Error::other("frame too large"));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok((head[0], payload))
}

// The app's side ---------------------------------------------------------------

/// Start the server unless one is listening.
pub fn ensure_daemon() {
    if UnixStream::connect(socket_path()).is_ok() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::fs::create_dir_all(dir());
    let mut command = std::process::Command::new(exe);
    command
        .arg("--session-daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Its own session: no controlling terminal to hang it up with.
    // SAFETY: the closure runs in the forked child before exec, where only
    // async-signal-safe calls are allowed: `setsid` is one, and nothing
    // allocates.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if command.spawn().is_err() {
        return;
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if UnixStream::connect(socket_path()).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn request(kind: u8, body: serde_json::Value) -> Option<serde_json::Value> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .ok()?;
    write_frame(&mut stream, kind, body.to_string().as_bytes()).ok()?;
    let (reply, payload) = read_frame(&mut stream).ok()?;
    (reply == REPLY).then(|| serde_json::from_slice(&payload).ok())?
}

/// End session `id` (its pane closed).
pub fn kill(id: &str) {
    request(KILL, serde_json::json!({ "id": id }));
}
/// End every session but `ids` (the panes a restored layout has).
pub fn keep(ids: &[String]) {
    request(KEEP, serde_json::json!({ "ids": ids }));
}
/// Session `id`'s pane took focus: what it finished and said is seen.
pub fn seen(id: &str) {
    request(SEEN, serde_json::json!({ "id": id }));
}
/// Hear every session's activity (see `snapshot`) as it changes, until
/// `heard` returns false; reconnects if the server goes and comes back. A
/// server older than `WATCH` (still running from before an update, keeping
/// its shells) answers nothing: then `{"unsupported":true}`, once.
/// Blocks: run it on a thread of its own.
pub fn watch(mut heard: impl FnMut(String) -> bool) {
    let mut told = false;
    loop {
        if let Ok(mut stream) = UnixStream::connect(socket_path())
            && write_frame(&mut stream, WATCH, b"{}").is_ok()
        {
            let mut answered = false;
            while let Ok((REPLY, payload)) = read_frame(&mut stream) {
                answered = true;
                remember_fronts(&payload);
                if !heard(String::from_utf8_lossy(&payload).into_owned()) {
                    return;
                }
            }
            forget_fronts();
            if !answered && !told {
                told = true;
                if !heard(r#"{"unsupported":true}"#.into()) {
                    return;
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

/// The command a pane's terminal runs to attach to session `id`, starting
/// it in `cwd`.
pub fn attach_command(id: &str, cwd: &str) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    format!(
        "{} --attach {} --cwd {}",
        quote(&exe),
        quote(id),
        quote(cwd)
    )
}

/// What runs in front of each session, from the server's last snapshot
/// (`watch`): `None` until one arrives, or while the server is one that
/// does not send it (older than this; `resolve` asks then).
static FRONTS: Mutex<Option<HashMap<String, zgx_desktop::ForegroundProcess>>> = Mutex::new(None);
fn remember_fronts(snapshot: &[u8]) {
    let parsed: serde_json::Value = serde_json::from_slice(snapshot).unwrap_or_default();
    let fronts = parsed["sessions"].as_object().and_then(|sessions| {
        sessions
            .iter()
            .map(|(id, s)| {
                Some((
                    id.clone(),
                    zgx_desktop::ForegroundProcess {
                        name: s["process"].as_str()?.to_owned(),
                        pid: s["pid"].as_u64()? as u32,
                        idle: s["idle"].as_bool()?,
                        status: reported_from(&s["reported"]),
                    },
                ))
            })
            .collect::<Option<HashMap<_, _>>>()
    });
    *FRONTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = fronts;
}
fn forget_fronts() {
    *FRONTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}
/// Whether the server tells what runs in front of its sessions as it
/// changes (`resolve` needs no asking, and nothing need poll it).
pub fn fronts_told() -> bool {
    FRONTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

/// Seen through a terminal's attach client (by its pid): what runs in front
/// of its session.
pub fn resolve(pid: u32) -> Option<zgx_desktop::ForegroundProcess> {
    let id = attached_session(pid)?;
    // Told; a session newer than the last snapshot is asked about.
    let told = FRONTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|fronts| fronts.get(&id).cloned());
    if told.is_some() {
        return told;
    }
    let info = request(INFO, serde_json::json!({ "id": id }))?;
    if !info["exists"].as_bool()? {
        return None;
    }
    Some(zgx_desktop::ForegroundProcess {
        name: info["name"].as_str()?.to_owned(),
        pid: info["pid"].as_u64()? as u32,
        idle: info["idle"].as_bool()?,
        status: reported_from(&info["status"]),
    })
}
/// Now, in Unix milliseconds.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

thread_local! {
    /// Attach clients' sessions by pid (their arguments never change).
    static ATTACHED: std::cell::RefCell<HashMap<u32, String>> = Default::default();
}
fn attached_session(pid: u32) -> Option<String> {
    if let Some(id) = ATTACHED.with(|cache| cache.borrow().get(&pid).cloned()) {
        return Some(id);
    }
    // Ghostty's foreground group is `login`'s, whose arguments are root's
    // and unreadable: the client is its child. A terminal just started may
    // not have it yet: asked again next time.
    let id = std::iter::once(pid as libc::pid_t)
        .chain(children(pid as libc::pid_t))
        .find_map(|pid| session_in(&arguments(pid)?))?;
    ATTACHED.with(|cache| cache.borrow_mut().insert(pid, id.clone()));
    Some(id)
}
/// `pid`'s child processes.
fn children(pid: libc::pid_t) -> Vec<libc::pid_t> {
    let mut pids = vec![0 as libc::pid_t; 16];
    let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: the buffer holds `bytes` writable bytes.
    let filled = unsafe { libc::proc_listchildpids(pid, pids.as_mut_ptr().cast(), bytes) };
    // A count of pids on macOS (the man page says bytes; the kernel returns
    // entries).
    let count = (filled.max(0) as usize).min(pids.len());
    pids.truncate(count);
    pids.retain(|&child| child > 0);
    pids
}
/// The session an attach client's arguments name. The terminal's foreground
/// process may be the client or what started it: Ghostty runs
/// `login … bash -c "exec -l client --attach 'p4' …"`, and the client stays in
/// login's process group, so the group is login's and the session is inside
/// one argument there.
fn session_in(args: &[String]) -> Option<String> {
    if let Some(at) = args.iter().position(|a| a == "--attach") {
        return args.get(at + 1).cloned();
    }
    args.iter().find_map(|arg| {
        let (_, rest) = arg.split_once("--attach ")?;
        let rest = rest.trim_start();
        let quoted = rest.strip_prefix('\'').and_then(|r| r.split_once('\''));
        let id = match quoted {
            Some((id, _)) => id,
            None => rest.split_whitespace().next()?,
        };
        (!id.is_empty()).then(|| id.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{Live, Recorder, SCROLLBACK, session_in};
    #[test]
    fn a_panic_in_the_screen_model_starts_it_over_and_keeps_the_session() {
        let mut live = Live {
            screen: vt100::Parser::new_with_callbacks(30, 100, SCROLLBACK, Recorder::default()),
            client: None,
            closed: false,
            activity: crate::activity::Activity::new(0),
            front: (0, String::new()),
        };
        live.with_screen(|screen| screen.process(b"\x1b]2;build\x07hello"));
        // vt100 panics on some wrapping after a resize; stand in for it.
        let result: Option<()> = live.with_screen(|_| panic!("vt100"));
        assert!(result.is_none());
        let screen = live.screen.screen();
        assert_eq!(screen.size(), (30, 100), "the same size");
        assert_eq!(screen.contents(), "", "blank");
        assert_eq!(
            live.screen.callbacks().title.as_deref(),
            Some(&b"build"[..])
        );
        // And it goes on.
        live.with_screen(|screen| screen.process(b"again")).unwrap();
        assert_eq!(live.screen.screen().contents(), "again");
    }
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn finds_the_session_from_the_client_or_from_login() {
        let client = args(&["-/app/terminal", "--attach", "p24", "--cwd", "/x"]);
        assert_eq!(session_in(&client).as_deref(), Some("p24"));
        let login = args(&[
            "/usr/bin/login",
            "-q",
            "-flp",
            "wing",
            "/bin/bash",
            "--noprofile",
            "--norc",
            "-c",
            "exec -l '/app/terminal' --attach 'p24' --cwd '/Users/wing'",
        ]);
        assert_eq!(session_in(&login).as_deref(), Some("p24"));
        let bare = args(&["sh", "-c", "exec client --attach p7 --cwd /"]);
        assert_eq!(session_in(&bare).as_deref(), Some("p7"));
        assert_eq!(session_in(&args(&["/bin/zsh", "-l"])), None);
    }
}

// The attach client --------------------------------------------------------------

/// Relay this terminal to session `id` until it ends (exit with its code)
/// or the terminal goes.
pub fn attach(id: &str, cwd: &str) -> ! {
    ensure_daemon();
    let Ok(mut stream) = UnixStream::connect(socket_path()) else {
        eprintln!("zgx-terminal: no session server");
        std::process::exit(1);
    };
    let (cols, rows) = tty_size();
    let env: Vec<(String, String)> = std::env::vars().collect();
    let hello = serde_json::json!({
        "id": id, "cwd": cwd, "cols": cols, "rows": rows, "env": env,
    });
    if write_frame(&mut stream, HELLO, hello.to_string().as_bytes()).is_err() {
        std::process::exit(1);
    }
    let restore = raw_mode();
    let exit = move |code: i32| -> ! {
        if let Some(termios) = &restore {
            // SAFETY: a termios `tcgetattr` filled, read for the call.
            unsafe { libc::tcsetattr(0, libc::TCSANOW, termios) };
        }
        std::process::exit(code)
    };
    // Keystrokes to the session.
    let mut input = stream.try_clone().expect("socket");
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            // SAFETY: reads at most `buf.len()` bytes into `buf`.
            let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 || write_frame(&mut input, INPUT, &buf[..n as usize]).is_err() {
                // The terminal went: detach (the session runs on).
                std::process::exit(0);
            }
        }
    });
    // Size changes, passed on as they happen: until the program hears one,
    // what it draws for the old size wraps and scrolls in the new. The
    // signal handler wakes the thread through a pipe.
    static WAKE: AtomicI32 = AtomicI32::new(-1);
    extern "C" fn on_winch(_: libc::c_int) {
        let fd = WAKE.load(Ordering::Relaxed);
        if fd >= 0 {
            // SAFETY: one byte from a live array; `write` (and the atomic
            // load) are async-signal-safe.
            unsafe { libc::write(fd, [0u8].as_ptr().cast(), 1) };
        }
    }
    let mut pipe = [-1; 2];
    // SAFETY: `pipe` writes two descriptors into the array.
    if unsafe { libc::pipe(pipe.as_mut_ptr()) } == 0 {
        // SAFETY: the pipe's write end, just made (open for the process's
        // life).
        unsafe { libc::fcntl(pipe[1], libc::F_SETFL, libc::O_NONBLOCK) };
        WAKE.store(pipe[1], Ordering::Relaxed);
        // SAFETY: `on_winch` is an `extern "C" fn(c_int)` doing only
        // async-signal-safe work.
        unsafe { libc::signal(libc::SIGWINCH, on_winch as *const () as libc::sighandler_t) };
    }
    let mut sizes = stream.try_clone().expect("socket");
    thread::spawn(move || {
        let mut last = (cols, rows);
        let mut wakes = [0u8; 64];
        // SAFETY: reads at most `wakes.len()` bytes into `wakes`, from the
        // pipe's read end (open for the process's life).
        while unsafe { libc::read(pipe[0], wakes.as_mut_ptr().cast(), wakes.len()) } > 0 {
            let size = tty_size();
            if size != last {
                last = size;
                let mut payload = Vec::with_capacity(4);
                payload.extend_from_slice(&size.0.to_le_bytes());
                payload.extend_from_slice(&size.1.to_le_bytes());
                if write_frame(&mut sizes, RESIZE, &payload).is_err() {
                    return;
                }
            }
        }
    });
    // The session's output to the terminal.
    let mut stdout = io::stdout().lock();
    loop {
        match read_frame(&mut stream) {
            Ok((OUTPUT, bytes)) => {
                if stdout
                    .write_all(&bytes)
                    .and_then(|()| stdout.flush())
                    .is_err()
                {
                    exit(0);
                }
            }
            Ok((EXIT, code)) => {
                let code = code.first().copied().unwrap_or(0) as i32;
                exit(code);
            }
            Ok(_) => {}
            // Another window took the session, or the server went.
            Err(_) => exit(0),
        }
    }
}

fn tty_size() -> (u16, u16) {
    // SAFETY: `winsize` is four integers: all zeroes is a valid one.
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ fills a `winsize`.
    if unsafe { libc::ioctl(0, libc::TIOCGWINSZ, &mut size) } == 0 && size.ws_col > 0 {
        (size.ws_col, size.ws_row)
    } else {
        (80, 24)
    }
}
fn raw_mode() -> Option<libc::termios> {
    // SAFETY: `termios` is integers and arrays of them (all zeroes is valid);
    // `tcgetattr` fills it and the others read or rewrite it in place.
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut termios) != 0 {
            return None;
        }
        let saved = termios;
        libc::cfmakeraw(&mut termios);
        libc::tcsetattr(0, libc::TCSANOW, &termios);
        Some(saved)
    }
}

// The server -----------------------------------------------------------------------

/// What the screen copy also keeps: the title and directory the shell last
/// set, to tell a terminal that attaches.
#[derive(Default)]
struct Recorder {
    title: Option<Vec<u8>>,
    pwd: Option<Vec<u8>>,
    /// What programs report of themselves (OSC 7501).
    status: crate::program_status::Records,
    /// The foreground process group while this output was written, and now.
    group: i32,
    now_ms: u64,
    /// Replies owed to the program (a status feature query).
    reply: Vec<u8>,
    /// What this output said to get attention (notifications, the bell) and
    /// its progress reports, for the session's activity (`Live::observe`).
    heard: Vec<Heard>,
    /// The title, directory or a status report changed with this output.
    changed: bool,
}
enum Heard {
    Note(String),
    Progress(u8),
}
impl vt100::Callbacks for Recorder {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.changed |= self.title.as_deref() != Some(title);
        self.title = Some(title.to_vec());
    }
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.heard.push(Heard::Note("bell".into()));
    }
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        match params.first().copied() {
            Some(b"7") => {
                let pwd = params[1..].join(&b';');
                self.changed |= self.pwd.as_deref() != Some(&pwd[..]);
                self.pwd = Some(pwd);
            }
            // A progress report (`9;4;state;value`), else a notification.
            Some(b"9") if params.get(1) == Some(&&b"4"[..]) => {
                let state = params.get(2).and_then(|s| std::str::from_utf8(s).ok());
                self.heard.push(Heard::Progress(
                    state.and_then(|s| s.parse().ok()).unwrap_or(0),
                ));
            }
            Some(b"9") if params.len() > 1 => {
                let text = String::from_utf8_lossy(&params[1..].join(&b';')).into_owned();
                self.heard.push(Heard::Note(text));
            }
            // `777;notify;title;body`: the body, else the title.
            Some(b"777") if params.get(1) == Some(&&b"notify"[..]) => {
                let part = |i: usize| {
                    params
                        .get(i)
                        .map(|p| String::from_utf8_lossy(p).into_owned())
                };
                let text = part(3).filter(|b| !b.is_empty()).or_else(|| part(2));
                self.heard.extend(text.map(Heard::Note));
            }
            Some(b"7501") => {
                self.changed = true;
                let payload = params[1..].join(&b';');
                if self.status.report(&payload, self.group, self.now_ms) {
                    self.reply
                        .extend_from_slice(crate::program_status::QUERY_REPLY);
                }
            }
            // A shell prompt begins (OSC 133 A): busy reports end.
            Some(b"133") if params.get(1).is_some_and(|p| p.starts_with(b"A")) => {
                self.changed = true;
                self.status.prompt(self.now_ms)
            }
            _ => {}
        }
    }
}

/// A session: its pseudo-terminal and shell, fixed for its life, and what
/// changes, under a lock of its own (writing to a slow terminal holds it,
/// never the server's).
struct Session {
    master: RawFd,
    child: libc::pid_t,
    live: Mutex<Live>,
    /// The reader saw the terminal close (all the output is relayed).
    drained: AtomicBool,
    /// Its reader and its waiter that have finished: the last closes
    /// `master`.
    finished: std::sync::atomic::AtomicU8,
    /// Its shell's reaping, which hang-ups wait for (`hang_up`).
    reap: Mutex<Reap>,
    reaped: Condvar,
}
/// Whether the shell is reaped, and the hang-ups not yet over.
#[derive(Default)]
struct Reap {
    reaped: bool,
    hanging_up: u32,
}
struct Live {
    screen: vt100::Parser<Recorder>,
    client: Option<(u64, UnixStream)>,
    /// `master` is closed: nothing may write to it (its number may be
    /// another file's by now).
    closed: bool,
    /// What its program is doing (the agents sidebar's source).
    activity: crate::activity::Activity,
    /// The foreground process group and its program's name.
    front: (libc::pid_t, String),
}
impl Session {
    /// Its live state, even after a thread panicked holding it: a session
    /// outlives a bug in one of its threads.
    fn live(&self) -> std::sync::MutexGuard<'_, Live> {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn reap(&self) -> std::sync::MutexGuard<'_, Reap> {
        self.reap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    /// Hang up its shell's process group, then kill what is left of it two
    /// seconds later. The shell stays unreaped until then (its waiter
    /// waits, `wait_session`), and an unreaped leader's pid, so its group's
    /// id, is no other process's: the kill reaches this group, not one
    /// that took its number. Nothing, once the shell is reaped.
    fn hang_up(self: &Arc<Self>) {
        let mut reap = self.reap();
        if reap.reaped {
            return;
        }
        reap.hanging_up += 1;
        // SAFETY: signals the shell's process group (it leads one, made by
        // `forkpty`), its id its own while the shell is unreaped (above).
        unsafe { libc::killpg(self.child, libc::SIGHUP) };
        drop(reap);
        let session = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            let mut reap = session.reap();
            // SAFETY: as above: the waiter reaps only once this is over.
            unsafe { libc::killpg(session.child, libc::SIGKILL) };
            reap.hanging_up -= 1;
            session.reaped.notify_all();
        });
    }
    /// One of its two threads is done; the second closes the terminal.
    fn finish(&self) {
        if self.finished.fetch_add(1, Ordering::AcqRel) == 1 {
            let mut live = self.live();
            live.closed = true;
            // SAFETY: the terminal's descriptor, ours: closed once (by the
            // second thread to finish), under the lock its other users check
            // `closed` under; the reader, which does not, is done.
            unsafe { libc::close(self.master) };
        }
    }
}
impl Live {
    /// After output: take what it said into the session's activity, and see
    /// whether the program in front changed. Whether anything a watcher
    /// shows changed.
    fn observe(&mut self, session: &Session, now_ms: u64) -> bool {
        let recorder = self.screen.callbacks_mut();
        let heard = std::mem::take(&mut recorder.heard);
        let mut changed = std::mem::take(&mut recorder.changed);
        let title = recorder.title.clone();
        let group = recorder.group;
        for heard in heard {
            changed |= match heard {
                Heard::Note(text) => self.activity.note(&text, now_ms),
                Heard::Progress(state) => self.activity.progress(state),
            };
        }
        changed |= self.activity.observe(title.as_deref(), now_ms);
        changed |= self.front_changed(session, group);
        changed
    }
    /// Note the foreground group `group` (and its program's name) if it is
    /// new.
    fn front_changed(&mut self, session: &Session, group: libc::pid_t) -> bool {
        if group == self.front.0 && !self.front.1.is_empty() {
            return false;
        }
        let pid = if group > 0 { group } else { session.child };
        self.front = (group, process_name(pid));
        true
    }
    /// Work on the screen model. `vt100` can panic on some sequences after a
    /// resize (a wide character wrapping): then the model starts over, blank
    /// at its size with its records kept, and the session goes on (the
    /// shell's output still reaches the terminal; only the replay of what
    /// came before is lost).
    fn with_screen<R>(&mut self, f: impl FnOnce(&mut vt100::Parser<Recorder>) -> R) -> Option<R> {
        let screen = &mut self.screen;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(screen))) {
            Ok(result) => Some(result),
            Err(_) => {
                let (rows, cols) = self.screen.screen().size();
                let records = std::mem::take(self.screen.callbacks_mut());
                self.screen = vt100::Parser::new_with_callbacks(rows, cols, SCROLLBACK, records);
                None
            }
        }
    }
}
#[derive(Default)]
struct Server {
    sessions: HashMap<String, Arc<Session>>,
    empty_since: Option<Instant>,
}
type Shared = Arc<Mutex<Server>>;
static TOKENS: AtomicU64 = AtomicU64::new(1);

/// Run the server: one per user, until it has had no sessions for a while.
pub fn daemon() -> ! {
    let _ = std::fs::create_dir_all(dir());
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        // One is running.
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else {
        std::process::exit(1);
    };
    // SAFETY: ignoring a signal installs no handler.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    let server: Shared = Arc::new(Mutex::new(Server {
        empty_since: Some(Instant::now()),
        ..Default::default()
    }));
    // Leave once there has been nothing to keep for a while.
    {
        let server = server.clone();
        let path = path.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                let server = server
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if server
                    .empty_since
                    .is_some_and(|since| since.elapsed() > IDLE_EXIT)
                {
                    let _ = std::fs::remove_file(&path);
                    std::process::exit(0);
                }
            }
        });
    }
    {
        let server = server.clone();
        thread::spawn(move || broadcast(server));
    }
    if let Some(queue) = fronts_queue() {
        let server = server.clone();
        thread::spawn(move || watch_fronts(queue, server));
    }
    for stream in listener.incoming().flatten() {
        let server = server.clone();
        thread::spawn(move || serve(stream, server));
    }
    std::process::exit(0)
}

/// What the kernel says of the sessions' processes, so a program coming to
/// the front is noticed without its output (`sleep 60` prints nothing) and
/// without polling: a shell forking (to run a command), and the program in
/// front changing (exec) or ending. Each is a moment later checked against
/// the terminal's foreground group, which the shell hands over just after
/// it forks.
fn fronts_queue() -> Option<RawFd> {
    static QUEUE: std::sync::OnceLock<Option<RawFd>> = std::sync::OnceLock::new();
    *QUEUE.get_or_init(|| {
        // SAFETY: takes nothing; the queue lives as long as the process.
        let queue = unsafe { libc::kqueue() };
        (queue >= 0).then_some(queue)
    })
}
/// Hear `pid`'s forks, execs and exit, for the session whose shell is
/// `shell`.
fn watch_process(pid: libc::pid_t, shell: libc::pid_t) {
    let Some(queue) = fronts_queue() else { return };
    let change = libc::kevent {
        ident: pid as usize,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: libc::NOTE_FORK | libc::NOTE_EXEC | libc::NOTE_EXIT,
        data: 0,
        udata: shell as usize as *mut libc::c_void,
    };
    // SAFETY: one change read from a local, no events asked for; `udata` is
    // a pid, never dereferenced.
    unsafe { libc::kevent(queue, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
}
/// Check the session of shell `shell` in `ms` milliseconds.
fn check_front_in(queue: RawFd, shell: libc::pid_t, ms: isize) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let change = libc::kevent {
        ident: NEXT.fetch_add(1, Ordering::Relaxed) as usize,
        filter: libc::EVFILT_TIMER,
        flags: libc::EV_ADD | libc::EV_ONESHOT,
        fflags: 0,
        data: ms,
        udata: shell as usize as *mut libc::c_void,
    };
    // SAFETY: as `watch_process`'s.
    unsafe { libc::kevent(queue, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
}
fn watch_fronts(queue: RawFd, server: Shared) {
    // SAFETY: `kevent` is integers and a pointer: all zeroes is valid.
    let mut events: [libc::kevent; 16] = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: no changes; fills at most `events.len()` events.
        let n = unsafe {
            libc::kevent(
                queue,
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as libc::c_int,
                std::ptr::null(),
            )
        };
        if n < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        for event in &events[..n as usize] {
            let shell = event.udata as usize as libc::pid_t;
            if event.filter == libc::EVFILT_TIMER {
                check_front(&server, shell);
            } else if event.fflags & libc::NOTE_EXEC != 0 {
                // A new program in the same process: its name.
                check_front(&server, shell);
            } else if event.fflags & (libc::NOTE_FORK | libc::NOTE_EXIT) != 0
                && !(event.ident as libc::pid_t == shell && event.fflags & libc::NOTE_EXIT != 0)
            {
                // The shell runs something, or what ran ended: the
                // terminal changes hands a moment later.
                check_front_in(queue, shell, 20);
                check_front_in(queue, shell, 150);
            }
        }
    }
}
/// The program in front of the session of shell `shell` now; watchers hear
/// if it changed.
fn check_front(server: &Shared, shell: libc::pid_t) {
    let session = server
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .sessions
        .values()
        .find(|session| session.child == shell)
        .cloned();
    let Some(session) = session else { return };
    let mut live = session.live();
    if live.closed {
        return;
    }
    // SAFETY: the terminal's descriptor, open (`closed` checked under the lock).
    let group = unsafe { libc::tcgetpgrp(session.master) };
    let pid = if group > 0 { group } else { shell };
    let front = (group, process_name(pid));
    if live.front != front {
        live.front = front;
        watchers().changed();
    }
    if group > 0 && group != shell {
        watch_process(group, shell);
    }
}

/// The windows watching every session's activity (`WATCH`), and whether it
/// changed since they last heard.
#[derive(Default)]
struct Watchers {
    changed: Mutex<bool>,
    wake: std::sync::Condvar,
    streams: Mutex<Vec<UnixStream>>,
}
fn watchers() -> &'static Watchers {
    static WATCHERS: std::sync::OnceLock<Watchers> = std::sync::OnceLock::new();
    WATCHERS.get_or_init(Watchers::default)
}
impl Watchers {
    /// Something a watcher shows changed: tell them (soon, coalesced).
    fn changed(&self) {
        *self
            .changed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.wake.notify_one();
    }
}
/// Tell watchers what changed: a snapshot of every session, at most every
/// `COALESCE`, and only when it differs from the last they heard. Idle
/// sessions send nothing.
fn broadcast(server: Shared) {
    const COALESCE: Duration = Duration::from_millis(80);
    let watchers = watchers();
    let mut last = String::new();
    loop {
        {
            let mut changed = watchers
                .changed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*changed {
                changed = watchers
                    .wake
                    .wait(changed)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        thread::sleep(COALESCE);
        *watchers
            .changed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        let mut streams = watchers
            .streams
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if streams.is_empty() {
            continue;
        }
        let snapshot = snapshot(&server);
        if snapshot == last {
            continue;
        }
        streams.retain_mut(|stream| write_frame(stream, REPLY, snapshot.as_bytes()).is_ok());
        last = snapshot;
    }
}
/// Every session's activity as JSON: `{ sessions = { [id] = { process,
/// idle, title, pwd, status = { state, kind, since_ms, msg, app, reported }
/// } } }`.
fn snapshot(server: &Shared) -> String {
    let sessions: Vec<(String, Arc<Session>)> = server
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .sessions
        .iter()
        .map(|(id, session)| (id.clone(), session.clone()))
        .collect();
    let now = unix_ms();
    let mut out = serde_json::Map::new();
    for (id, session) in sessions {
        let mut live = session.live();
        if live.closed {
            continue;
        }
        // SAFETY: the terminal's descriptor, open (`closed` checked under the
        // lock).
        let group = unsafe { libc::tcgetpgrp(session.master) };
        live.front_changed(&session, group);
        let recorder = live.screen.callbacks_mut();
        recorder.status.foreground(group, now);
        let reported = recorder.status.status();
        let text = |bytes: &Option<Vec<u8>>| {
            bytes
                .as_deref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default()
        };
        let (title, pwd) = (text(&recorder.title), text(&recorder.pwd));
        let status = live.activity.status(reported.as_ref());
        let msg = reported
            .as_ref()
            .and_then(|r| r.msg.clone())
            .or_else(|| live.activity.note_text().map(str::to_owned));
        let pid = if group > 0 { group } else { session.child };
        out.insert(
            id,
            serde_json::json!({
                "process": live.front.1,
                "pid": pid,
                "idle": pid == session.child,
                // What the program reports itself, as `INFO` has it (the
                // window's terminals take their program from here).
                "reported": reported.as_ref().map(reported_json),
                "title": title,
                "pwd": pwd,
                "status": {
                    "state": status.state,
                    "kind": status.kind,
                    "since_ms": status.since_ms,
                    "msg": msg,
                    "app": reported.as_ref().and_then(|r| r.app.clone()),
                    // The program reported it (OSC 7501), not worked out.
                    "reported": reported.is_some(),
                },
            }),
        );
    }
    serde_json::json!({ "sessions": out }).to_string()
}
/// A program's own report of its status, as `INFO` and snapshots send it.
fn reported_json(s: &zgx::terminal::ProgramStatus) -> serde_json::Value {
    serde_json::json!({
        "state": s.state.name(),
        "kind": s.kind,
        "progress": s.progress,
        "app": s.app,
        "title": s.title,
        "msg": s.msg,
        "child_blocked": s.child_blocked,
        "since_ms": s.since_ms,
    })
}
/// The other way: what `reported_json` made.
fn reported_from(s: &serde_json::Value) -> Option<zgx::terminal::ProgramStatus> {
    let s = s.as_object()?;
    let text = |key: &str| s.get(key).and_then(|v| v.as_str()).map(str::to_owned);
    Some(zgx::terminal::ProgramStatus {
        state: zgx::terminal::ProgramState::parse(s.get("state")?.as_str()?)?,
        kind: text("kind"),
        progress: s.get("progress").and_then(|v| v.as_u64()).map(|p| p as u8),
        app: text("app"),
        title: text("title"),
        msg: text("msg"),
        child_blocked: s
            .get("child_blocked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        since_ms: s.get("since_ms").and_then(|v| v.as_u64()).unwrap_or(0),
    })
}

fn session(server: &Shared, id: &str) -> Option<Arc<Session>> {
    server
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .sessions
        .get(id)
        .cloned()
}

fn serve(mut stream: UnixStream, server: Shared) {
    let Ok((kind, payload)) = read_frame(&mut stream) else {
        return;
    };
    let body: serde_json::Value = serde_json::from_slice(&payload).unwrap_or_default();
    let id = body["id"].as_str().unwrap_or_default().to_owned();
    match kind {
        HELLO => attach_client(stream, server, &id, &body),
        INFO => {
            let reply = match session(&server, &id) {
                Some(session) => {
                    let mut live = session.live();
                    // SAFETY: the terminal's descriptor, read only while open
                    // (`closed` checked under the lock `finish` closes it under).
                    let front = if live.closed {
                        -1
                    } else {
                        unsafe { libc::tcgetpgrp(session.master) }
                    };
                    let pid = if front > 0 { front } else { session.child };
                    let status = {
                        let records = &mut live.screen.callbacks_mut().status;
                        records.foreground(front, unix_ms());
                        records.status()
                    };
                    serde_json::json!({
                        "exists": true,
                        "pid": pid,
                        "name": process_name(pid),
                        "idle": pid == session.child,
                        "status": status.as_ref().map(reported_json),
                    })
                }
                None => serde_json::json!({ "exists": false }),
            };
            let _ = write_frame(&mut stream, REPLY, reply.to_string().as_bytes());
        }
        KILL => {
            end(&server, |session_id| session_id == id);
            let _ = write_frame(&mut stream, REPLY, b"{}");
        }
        KEEP => {
            let keep: Vec<String> = body["ids"]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            end(&server, |session_id| !keep.iter().any(|k| k == session_id));
            let _ = write_frame(&mut stream, REPLY, b"{}");
        }
        SEEN => {
            if let Some(session) = session(&server, &id)
                && session.live().activity.seen(unix_ms())
            {
                watchers().changed();
            }
            let _ = write_frame(&mut stream, REPLY, b"{}");
        }
        // What is true now, then every change.
        WATCH if write_frame(&mut stream, REPLY, snapshot(&server).as_bytes()).is_ok() => {
            watchers()
                .streams
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(stream);
        }
        _ => {}
    }
}

/// Hang up the shells of the sessions `which` picks (their waiter threads
/// tidy up when they exit).
fn end(server: &Shared, which: impl Fn(&str) -> bool) {
    let server = server
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (id, session) in &server.sessions {
        if which(id) {
            session.hang_up();
        }
    }
}

fn attach_client(mut stream: UnixStream, server: Shared, id: &str, hello: &serde_json::Value) {
    let cols = hello["cols"].as_u64().unwrap_or(80).clamp(1, 1000) as u16;
    let rows = hello["rows"].as_u64().unwrap_or(24).clamp(1, 1000) as u16;
    let token = TOKENS.fetch_add(1, Ordering::Relaxed);
    let session = {
        let mut guard = server
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.sessions.get(id) {
            Some(session) => session.clone(),
            None => {
                let Some(session) = spawn(hello, cols, rows) else {
                    let _ = write_frame(&mut stream, EXIT, &[127]);
                    return;
                };
                let session = Arc::new(session);
                watch_process(session.child, session.child);
                guard.sessions.insert(id.to_owned(), session.clone());
                guard.empty_since = None;
                let reading = session.clone();
                thread::spawn(move || read_session(reading));
                let (server, id, waiting) = (server.clone(), id.to_owned(), session.clone());
                thread::spawn(move || wait_session(server, id, waiting));
                watchers().changed();
                session
            }
        }
    };
    {
        let mut live = session.live();
        resize(&session, &mut live, cols, rows);
        let replay = live
            .with_screen(replay)
            .unwrap_or_else(|| replay(&mut live.screen));
        if write_frame(&mut stream, OUTPUT, &replay).is_err() {
            return;
        }
        // The last terminal to attach has it; an earlier one is let go.
        if let Some((_, old)) = live.client.take() {
            let _ = old.shutdown(std::net::Shutdown::Both);
        }
        live.client = stream.try_clone().ok().map(|s| (token, s));
    }
    loop {
        match read_frame(&mut stream) {
            Ok((INPUT, bytes)) => {
                let live = session.live();
                if !live.closed {
                    write_all(session.master, &bytes);
                }
            }
            Ok((RESIZE, size)) if size.len() == 4 => {
                let cols = u16::from_le_bytes([size[0], size[1]]);
                let rows = u16::from_le_bytes([size[2], size[3]]);
                let mut live = session.live();
                resize(&session, &mut live, cols, rows);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    // Detached: the shell runs on.
    let mut live = session.live();
    if live.client.as_ref().is_some_and(|(t, _)| *t == token) {
        live.client = None;
    }
}

fn resize(session: &Session, live: &mut Live, cols: u16, rows: u16) {
    if live.screen.screen().size() == (rows, cols) {
        return;
    }
    live.with_screen(|screen| screen.screen_mut().set_size(rows, cols));
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if !live.closed {
        // SAFETY: the terminal's descriptor, open (`closed` checked under the
        // lock); TIOCSWINSZ reads a `winsize`.
        unsafe { libc::ioctl(session.master, libc::TIOCSWINSZ, &size) };
    }
}

/// What redraws a session for a terminal that attaches: its scrollback
/// (scrolled up out of view), then its screen, modes, title and directory.
fn replay(screen: &mut vt100::Parser<Recorder>) -> Vec<u8> {
    let mut out = b"\x1b[0m\x1b[H\x1b[2J".to_vec();
    let s = screen.screen_mut();
    if !s.alternate_screen() {
        s.set_scrollback(usize::MAX);
        let depth = s.scrollback();
        let (rows, cols) = s.size();
        for offset in (1..=depth).rev() {
            s.set_scrollback(offset);
            if let Some(row) = s.rows_formatted(0, cols).next() {
                out.extend_from_slice(&row);
            }
            out.extend_from_slice(b"\x1b[0m\r\n");
        }
        s.set_scrollback(0);
        if depth > 0 {
            // Push it all above the screen.
            for _ in 0..rows {
                out.extend_from_slice(b"\r\n");
            }
        }
    }
    out.extend_from_slice(&screen.screen().state_formatted());
    let recorded = screen.callbacks();
    if let Some(title) = &recorded.title {
        out.extend_from_slice(b"\x1b]2;");
        out.extend_from_slice(title);
        out.extend_from_slice(b"\x07");
    }
    if let Some(pwd) = &recorded.pwd {
        out.extend_from_slice(b"\x1b]7;");
        out.extend_from_slice(pwd);
        out.extend_from_slice(b"\x07");
    }
    out
}

/// The shell's output: into the screen copy and to the attached terminal.
/// When it exits, the session ends and its terminal is told the code.
/// Relay a session's output to its terminal and keep its screen model,
/// until the terminal closes (every process holding it has gone).
fn read_session(session: Arc<Session>) {
    let mut buf = vec![0u8; 65536];
    loop {
        // SAFETY: the terminal's descriptor, open until this reader finishes
        // (`finish` closes it only after both threads); at most `buf.len()`
        // bytes into `buf`.
        let n = unsafe { libc::read(session.master, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
        let bytes = &buf[..n as usize];
        let mut live = session.live();
        {
            let recorder = live.screen.callbacks_mut();
            // SAFETY: as the read above.
            recorder.group = unsafe { libc::tcgetpgrp(session.master) };
            recorder.now_ms = unix_ms();
        }
        live.with_screen(|screen| screen.process(bytes));
        if live.observe(&session, unix_ms()) {
            watchers().changed();
        }
        let reply = std::mem::take(&mut live.screen.callbacks_mut().reply);
        if !reply.is_empty() {
            write_all(session.master, &reply);
        }
        if let Some((_, client)) = &mut live.client
            && write_frame(client, OUTPUT, bytes).is_err()
        {
            live.client = None;
        }
    }
    session.drained.store(true, Ordering::Release);
    session.finish();
}

/// Wait for the session's shell to exit and reap it, once no hang-up
/// signals its group any more; its wait status.
fn reap_shell(session: &Session) -> libc::c_int {
    // Its exit, leaving it unreaped (`WNOWAIT`): its group's id stays its
    // own while a hang-up still signals the group (`Session::hang_up`).
    // SAFETY: waits for the session's own child, writing `info`.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    while unsafe {
        libc::waitid(
            libc::P_PID,
            session.child as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOWAIT,
        )
    } < 0
        && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
    {}
    let mut status = 0;
    {
        let mut reap = session.reap();
        while reap.hanging_up > 0 {
            reap = session
                .reaped
                .wait(reap)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        // SAFETY: reaps the session's own child, writing its status.
        while unsafe { libc::waitpid(session.child, &mut status, 0) } < 0
            && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
        {}
        reap.reaped = true;
    }
    status
}

/// End a session when its shell exits, whatever still holds its terminal
/// (a background job the shell left keeps it open, so the reader would
/// never hear the end): it goes from the server and its terminal hears the
/// exit code, after the shell's last output.
fn wait_session(server: Shared, id: String, session: Arc<Session>) {
    let status = reap_shell(&session);
    let code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        1
    };
    // The reader relays what the shell wrote last, if the terminal closes.
    for _ in 0..20 {
        if session.drained.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    {
        let mut guard = server
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Only this session: the pane may have a new one by now.
        if guard
            .sessions
            .get(&id)
            .is_some_and(|current| Arc::ptr_eq(current, &session))
        {
            guard.sessions.remove(&id);
        }
        if guard.sessions.is_empty() {
            guard.empty_since = Some(Instant::now());
        }
    }
    watchers().changed();
    {
        let mut live = session.live();
        if let Some((_, client)) = &mut live.client {
            let _ = write_frame(client, EXIT, &[code as u8]);
        }
    }
    session.finish();
}

fn write_all(fd: RawFd, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        // SAFETY: writes at most `bytes.len()` bytes from `bytes`; callers
        // pass the terminal's descriptor while it is open.
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

/// A login shell on a new pseudo-terminal, in the terminal's environment
/// (Ghostty's TERM and shell integration included).
fn spawn(hello: &serde_json::Value, cols: u16, rows: u16) -> Option<Session> {
    let mut env: Vec<(String, String)> = hello["env"]
        .as_array()
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|pair| {
                    Some((
                        pair.get(0)?.as_str()?.to_owned(),
                        pair.get(1)?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let var = |env: &[(String, String)], key: &str| {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };
    let shell = var(&env, "SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let name = shell.rsplit('/').next().unwrap_or("zsh").to_owned();
    // Ghostty's zsh integration (prompt marks, the directory, titles), as it
    // sets it up for a zsh of its own.
    if name == "zsh"
        && let Some(resources) = var(&env, "GHOSTTY_RESOURCES_DIR")
    {
        if let Some(zdotdir) = var(&env, "ZDOTDIR") {
            env.push(("GHOSTTY_ZSH_ZDOTDIR".into(), zdotdir));
        }
        env.retain(|(k, _)| k != "ZDOTDIR");
        env.push((
            "ZDOTDIR".into(),
            format!("{resources}/shell-integration/zsh"),
        ));
    }
    let cwd = hello["cwd"].as_str().unwrap_or_default();
    let cwd = CString::new(if cwd.is_empty() { "/" } else { cwd }).ok()?;
    let path = CString::new(shell.clone()).ok()?;
    let argv0 = CString::new(format!("-{name}")).ok()?;
    let argv = [argv0.as_ptr(), std::ptr::null()];
    let envs: Vec<CString> = env
        .iter()
        .filter_map(|(k, v)| CString::new(format!("{k}={v}")).ok())
        .collect();
    let mut envp: Vec<*const libc::c_char> = envs.iter().map(|e| e.as_ptr()).collect();
    envp.push(std::ptr::null());
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut master: libc::c_int = -1;
    // SAFETY: writes the terminal's descriptor into `master`; no name or
    // termios asked for, and `size` is only read.
    let child = unsafe {
        libc::forkpty(
            &mut master,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &size as *const _ as *mut _,
        )
    };
    if child < 0 {
        return None;
    }
    if child == 0 {
        // The child: only async-signal-safe calls until exec.
        // SAFETY: every string and array was made before the fork and is
        // NUL-terminated (`argv` and `envp` end in null); `chdir`, `execve`
        // and `_exit` are async-signal-safe and allocate nothing.
        unsafe {
            libc::chdir(cwd.as_ptr());
            libc::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            libc::_exit(127);
        }
    }
    Some(Session {
        master,
        child,
        drained: AtomicBool::new(false),
        finished: Default::default(),
        reap: Mutex::default(),
        reaped: Condvar::new(),
        live: Mutex::new(Live {
            screen: vt100::Parser::new_with_callbacks(rows, cols, SCROLLBACK, Recorder::default()),
            client: None,
            closed: false,
            activity: crate::activity::Activity::new(unix_ms()),
            front: (0, String::new()),
        }),
    })
}

// Processes ----------------------------------------------------------------------

/// A process's name as it presents itself: its `argv[0]` (Claude Code sets
/// its own), else its executable's.
fn process_name(pid: libc::pid_t) -> String {
    arguments(pid)
        .and_then(|args| args.into_iter().next())
        .or_else(|| {
            let mut buffer = [0u8; 256];
            // SAFETY: the buffer is writable for its full length, and the
            // length returned is within it.
            let len =
                unsafe { libc::proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
            (len > 0).then(|| String::from_utf8_lossy(&buffer[..len as usize]).into_owned())
        })
        .map(|name| {
            let first = name.split(' ').next().unwrap_or(&name).to_owned();
            let base = first.rsplit('/').next().unwrap_or(&first).to_owned();
            base.strip_prefix('-').map(str::to_owned).unwrap_or(base)
        })
        .unwrap_or_default()
}

/// A process's arguments (`KERN_PROCARGS2`).
fn arguments(pid: libc::pid_t) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    // SAFETY: a three-name MIB; first a size query (null buffer), then a
    // buffer of that size, which `size` bounds the kernel's write to.
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buffer = vec![0u8; size];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buffer.truncate(size);
        let argc = i32::from_ne_bytes(buffer.get(..4)?.try_into().ok()?) as usize;
        let rest = buffer.get(4..)?;
        // The executable's path, NUL padding, then argv.
        let exe_end = rest.iter().position(|b| *b == 0)?;
        let after = &rest[exe_end..];
        let start = after.iter().position(|b| *b != 0)?;
        let args = after[start..]
            .split(|b| *b == 0)
            .take(argc)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        Some(args)
    }
}

#[cfg(test)]
mod hang_up_tests {
    use super::*;

    /// A shell ignoring its hang-up is killed two seconds later, with its
    /// group, while it is unreaped (its group's id its own), and reaped
    /// only then; once it is reaped, a hang-up signals nothing.
    #[test]
    fn a_hang_up_kills_what_ignores_it_before_the_shell_is_reaped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("zgx-hang-up-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shell = dir.join("stubborn");
        std::fs::write(&shell, "#!/bin/sh\ntrap '' HUP\nsleep 30\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let hello = serde_json::json!({ "env": [["SHELL", shell.to_str().unwrap()]] });
        let session = Arc::new(spawn(&hello, 80, 24).unwrap());
        // Time for the shell to ignore the hang-up.
        thread::sleep(Duration::from_millis(500));
        let started = Instant::now();
        session.hang_up();
        let status = reap_shell(&session);
        assert!(started.elapsed() >= Duration::from_millis(1900));
        assert!(
            libc::WIFSIGNALED(status) && libc::WTERMSIG(status) == libc::SIGKILL,
            "{status}"
        );
        assert!(session.reap().reaped);
        session.hang_up();
        assert_eq!(session.reap().hanging_up, 0);
    }
}
