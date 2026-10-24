//! Luau scripts that describe zgui interfaces, animated with anime.js's API.
//!
//! A script runs once per (re)load and returns a description of components,
//! state bindings and animations. Rust lowers that description to ordinary
//! retained views: layout, drawing, state and animation all stay in Rust,
//! and the script is called again only for event handlers, components as
//! they mount, and properties the script explicitly computes with a
//! function. Text templates such as `"Count: {count}"` and every animation
//! run without calling the script.
//!
//! State is Rust-owned and keyed, so a hot reload keeps it. Each script gets
//! its own sandboxed VM with a memory limit and a time budget per call, which
//! makes plugins safe to load next to the application script.
//!
//! ```lua
//! local count = ui.state("count", 0)
//! anime.animate(".dot", { y = { 0, -8, 0 }, delay = anime.stagger(40), loop = true })
//! return ui.column {
//!     p = 24, gap = 12, bg = "#172030",
//!     ui.text { "Count: {count}", text_size = 18 },
//!     ui.button { on_click = ui.add(count), px = 16, py = 10, "Increment" },
//! }
//! ```
//!
//! The `ui` and `anime` libraries are documented in `docs/scripting.md`.
#![warn(missing_docs)]
mod anim;
mod anime;
mod bundle;
mod calls;
mod canvas;
pub mod check;
mod command;
mod data;
mod design;
mod element;
#[cfg(target_os = "macos")]
mod file_events;
#[cfg(windows)]
#[path = "file_events_windows.rs"]
mod file_events;
#[cfg(target_os = "linux")]
#[path = "file_events_linux.rs"]
mod file_events;
mod interrupt;
mod io;
#[cfg(feature = "components")]
mod library;
pub mod lint;
mod lower;
pub mod markdown;
mod meter;
mod motion;
mod node;
mod opens;
mod persist;
mod sorted;
mod term;
pub mod testing;
mod theme;
mod timers;
mod transform;
mod value;
mod variants;
mod windows;

pub use bundle::{Bundle, BundleFile, build};
pub use calls::{Request, Response};
pub use command::Command;
pub use design::DesignLint;
pub use persist::app_data_dir;
pub use value::{ElementHandle, PathKey, Shared, Store, Value, tables_converted, values_converted};
pub use windows::Window;

use mlua::Lua;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};
use zgui::{
    compose::{View, prelude::*, switch},
    reactive::{Runtime, Signal},
};

/// Resource limits for each script.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Heap limit for each of the app's own script VMs: generous, since an
    /// app holds its data there (100,000 table rows take some 60 MB); it
    /// stops a runaway allocation before the process runs out.
    pub memory: usize,
    /// Heap limit for each plugin's VM, which the app needn't trust, with
    /// what its state holds (values Rust keeps, outside the VM) counted in.
    pub plugin_memory: usize,
    /// Budget for evaluating a script on load.
    pub load: Duration,
    /// Budget for one handler or computed property call. Budgets count the
    /// CPU time from a reading up to 1 ms old, so a script may be stopped up
    /// to 1 ms short of one, never past it.
    pub call: Duration,
}
impl Default for Limits {
    /// In a debug build the time budgets are ten times longer: the same
    /// script runs several times slower there (the dashboard's sort of
    /// 100,000 rows, 128 ms of a 50 ms budget, with zgui-script's Rust
    /// unoptimized, as `cargo run` builds it), and a budget is to stop a
    /// runaway script, which half a second still does.
    fn default() -> Self {
        let scale = if cfg!(debug_assertions) { 10 } else { 1 };
        Self {
            memory: 1 << 30,
            plugin_memory: 32 << 20,
            load: Duration::from_millis(500 * scale),
            call: Duration::from_millis(50 * scale),
        }
    }
}

pub(crate) type Actions = Rc<RefCell<HashMap<String, Rc<dyn Fn(Value)>>>>;
/// Host functions scripts call for a value now (`ui.call_sync`).
pub(crate) type Functions = Rc<RefCell<HashMap<String, Rc<dyn Fn(Value) -> Value>>>>;

/// One script's VM.
/// `.luaurc`'s `aliases` in `dir`, resolved against it.
fn read_aliases(dir: &std::path::Path, files: Files) -> HashMap<String, PathBuf> {
    let Ok(text) = files.read_to_string(&dir.join(".luaurc")) else {
        return HashMap::new();
    };
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&text) else {
        eprintln!("{}: .luaurc is not JSON", dir.display());
        return HashMap::new();
    };
    config
        .get("aliases")
        .and_then(|a| a.as_object())
        .map(|aliases| {
            aliases
                .iter()
                .filter_map(|(name, path)| {
                    Some((name.to_lowercase(), normalize(&dir.join(path.as_str()?))))
                })
                .collect()
        })
        .unwrap_or_default()
}
/// Whether `path` is in `root` (a canonical directory): by its name, and,
/// where it exists, by what it is, so a symbolic link inside the root to a
/// file outside it (a plugin shipping one to `~/.ssh`) is outside too.
pub(crate) fn within(root: &std::path::Path, path: &std::path::Path) -> bool {
    path.starts_with(root)
        && path
            .canonicalize()
            .map_or(true, |real| real.starts_with(root))
}
/// `path` with `.` and `..` resolved in place, without touching the disk.
pub(crate) fn normalize(path: &std::path::Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A script call's time budget. It counts the CPU time the thread spends,
/// so a busy machine does not fail a script that is not looping; the cheap
/// wall clock decides when the CPU time is worth reading.
#[derive(Clone, Copy)]
pub(crate) struct Deadline {
    wall: Instant,
    cpu: Duration,
    budget: Duration,
}
/// How old a CPU-time reading an outermost call's budget may start from:
/// reading the thread's CPU clock costs more than a short call itself, and a
/// flush runs hundreds of them. A reading this old is this much CPU early at
/// most, so a script is stopped at most this much short of its budget, never
/// past it. Starts that shift another budget (a nested call, work off the
/// clock) read the clock afresh: an early start there would give the outer
/// call back more than it spent, every time.
const CPU_READING_AGE: Duration = Duration::from_millis(1);
thread_local! {
    /// The thread's last CPU-time reading, and when it was taken.
    static CPU_READING: Cell<Option<(Instant, Duration)>> = const { Cell::new(None) };
}
/// The thread's CPU time, or a reading of it at most `CPU_READING_AGE` old.
fn recent_thread_cpu(now: Instant) -> Duration {
    CPU_READING.with(|reading| match reading.get() {
        Some((at, cpu)) if now.saturating_duration_since(at) <= CPU_READING_AGE => cpu,
        _ => {
            let cpu = thread_cpu();
            reading.set(Some((now, cpu)));
            cpu
        }
    })
}
impl Deadline {
    /// A budget from now, its CPU start a recent reading (`CPU_READING_AGE`).
    fn start_recent(budget: Duration) -> Self {
        let wall = Instant::now();
        Self {
            wall,
            cpu: recent_thread_cpu(wall),
            budget,
        }
    }
    /// A budget from now, read exactly: one that will shift another.
    fn start(budget: Duration) -> Self {
        Self {
            wall: Instant::now(),
            cpu: thread_cpu(),
            budget,
        }
    }
    /// The same budget with the time `other` has run since it started
    /// taken off the clock.
    fn shifted(self, other: &Deadline) -> Self {
        Self {
            wall: self.wall + other.wall.elapsed(),
            cpu: self.cpu + thread_cpu().saturating_sub(other.cpu),
            budget: self.budget,
        }
    }
    pub(crate) fn exceeded(&self) -> bool {
        self.wall.elapsed() > self.budget && thread_cpu().saturating_sub(self.cpu) > self.budget
    }
}
/// CPU time this thread has used.
#[cfg(unix)]
fn thread_cpu() -> Duration {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `t` is a valid out-pointer for the duration of the call.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) } == 0;
    if ok {
        Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
    } else {
        Duration::ZERO
    }
}
/// CPU time this thread has used. Windows advances it a scheduler tick
/// (about 15.6 ms) at a time, so it reads up to a tick short: a runaway
/// script stops that much later.
#[cfg(windows)]
fn thread_cpu() -> Duration {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    // SAFETY: as `processthreadsapi.h` declares them: `GetCurrentThread`
    // returns a pseudo handle (never closed), and `GetThreadTimes` takes a
    // thread handle and four `FILETIME` out-pointers, `FileTime` being its
    // layout (two `DWORD`s, low first), and returns a `BOOL`.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn GetThreadTimes(
            thread: *mut std::ffi::c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let [mut creation, mut exit, mut kernel, mut user] = Default::default();
    // SAFETY: Fills four FILETIMEs for this thread (a pseudo handle).
    let ok = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } != 0;
    // FILETIME durations count 100 ns intervals.
    let time = |t: &FileTime| Duration::from_nanos(((t.high as u64) << 32 | t.low as u64) * 100);
    if ok {
        time(&kernel) + time(&user)
    } else {
        Duration::ZERO
    }
}
/// Without a thread CPU clock, wall time stands in.
#[cfg(not(any(unix, windows)))]
fn thread_cpu() -> Duration {
    thread_local! { static START: Instant = Instant::now(); }
    START.with(|s| s.elapsed())
}

pub(crate) struct Vm {
    name: String,
    /// The time budget and memory watch (see `interrupt`).
    _interrupt: interrupt::Installed,
    /// Keeps the state alive: script values only hold weak references to it.
    pub(crate) lua: Lua,
    /// Where relative image and SVG paths resolve from.
    pub(crate) base: Option<PathBuf>,
    deadline: Rc<Cell<Option<Deadline>>>,
    pub(crate) budget: Duration,
    pub(crate) runtime: Runtime,
    errors: Signal<Option<String>>,
    /// The script's mounted `layout_id` nodes: ids are scoped per script.
    pub(crate) layout_ids: motion::LayoutIds,
    /// Whether nodes record the script line that creates them (for the
    /// layout inspector): shared with the host, off unless it records.
    pub(crate) locate: Rc<Cell<bool>>,
    /// Calls into the script so far, shared with the host (`script_calls`).
    calls: Rc<Cell<u64>>,
}
impl Vm {
    /// Runs `f` with its own budget. A nested call (a component built
    /// while a handler runs) stops the outer call's clock meanwhile.
    pub(crate) fn guard<R>(
        &self,
        budget: Duration,
        f: impl FnOnce() -> mlua::Result<R>,
    ) -> mlua::Result<R> {
        let _span = zgui::profile::span(zgui::profile::Phase::Script);
        self.calls.set(self.calls.get() + 1);
        // Outermost, nothing shifts by this start: a recent reading will do.
        let started = match self.deadline.get() {
            None => Deadline::start_recent(budget),
            Some(_) => Deadline::start(budget),
        };
        let outer = self.deadline.replace(Some(started));
        let result = f();
        self.deadline.set(outer.map(|d| d.shifted(&started)));
        result
    }
    /// Runs Rust work a script asked for (a state write and the interface
    /// it rebuilds) off the script's clock: the budget stops runaway
    /// scripts, not the updates they cause.
    pub(crate) fn off_clock<R>(&self, f: impl FnOnce() -> R) -> R {
        let started = Deadline::start(Duration::ZERO);
        let result = f();
        if let Some(d) = self.deadline.get() {
            self.deadline.set(Some(d.shifted(&started)));
        }
        result
    }
    pub(crate) fn call<R: mlua::FromLuaMulti>(
        &self,
        f: &mlua::Function,
        args: impl mlua::IntoLuaMulti,
    ) -> mlua::Result<R> {
        self.guard(self.budget, || f.call(args))
    }
    pub(crate) fn report(&self, error: mlua::Error) {
        let message = describe(&self.name, &error);
        eprintln!("{message}");
        self.errors.set(Some(message));
    }
}

/// An error with Rust-side frames removed from its traceback.
fn describe(name: &str, error: &mlua::Error) -> String {
    let text = error.to_string();
    let lines: Vec<_> = text
        .lines()
        .filter(|line| line.trim() != "[C]: in ?")
        .collect();
    let lines = match lines.as_slice() {
        [message @ .., last] if *last == "stack traceback:" => message,
        all => all,
    };
    let mut message = format!("{name}: {}", lines.join("\n"));
    // State values are read-only snapshots: say how to change them.
    if text.contains("attempt to modify a readonly table") {
        message.push_str(
            "\n(state values are read-only: change state with state:set_key(key, value), \
             state:patch({ … }), state:update(function(t) … end), state:insert(i, v) \
             or state:set(value))",
        );
    }
    // Luau says only "not enough memory": say which limit and its knob.
    if text.contains("not enough memory") {
        message.push_str(
            "\n(the script's VM reached its memory limit: `Limits::memory` for the app's \
             scripts, 1 GiB by default, or `Limits::plugin_memory` for a plugin's, 32 MiB; \
             a host raises them with `ScriptHost::limits`)",
        );
    }
    message
}

/// The host's script warnings: each once, in the order they came.
#[derive(Default)]
pub(crate) struct Warnings {
    seen: std::collections::HashSet<String>,
    list: Vec<String>,
    /// How strictly styles must use theme tokens (see `design`).
    pub(crate) design: DesignLint,
    /// `ZGUI_DESIGN_LINT` chose it: scripts do not change it.
    pub(crate) design_forced: bool,
}
pub(crate) type SharedWarnings = Rc<RefCell<Warnings>>;
/// Where script warnings go: stderr and the host's list
/// ([`ScriptHost::warnings`]). Everything that warns comes through here, so
/// a host can show them elsewhere too.
pub(crate) fn warning(warnings: Option<&SharedWarnings>, message: String) {
    if let Some(warnings) = warnings {
        let mut w = warnings.borrow_mut();
        if !w.seen.insert(message.clone()) {
            return;
        }
        w.list.push(message.clone());
    }
    eprintln!("warning: {message}");
}
/// A warning about the script code calling into Rust, once per call site
/// (across reloads), with its file and line.
pub(crate) fn warn(lua: &Lua, message: &str) {
    let message = match call_site(lua) {
        Some(site) => format!("{site}: {message}"),
        None => message.to_owned(),
    };
    let warnings = lua
        .app_data_ref::<node::Env>()
        .map(|env| env.warnings.clone());
    warning(warnings.as_ref(), message);
}
/// The script line calling into Rust (`app.luau:12`), if it has one.
pub(crate) fn call_site(lua: &Lua) -> Option<String> {
    lua.inspect_stack(1, |d| {
        let source = d.source().short_src.map(|s| s.into_owned());
        (source, d.current_line())
    })
    .and_then(|(source, line)| Some(format!("{}:{}", source?, line?)))
}
/// Durations are milliseconds everywhere: warn about one that reads as
/// seconds (fractional and under 10) or that runs over a minute once.
pub(crate) fn check_duration(lua: &Lua, what: &str, ms: f64, looping: bool) {
    if ms > 0. && ms < 10. && ms.fract() != 0. {
        let as_ms = (ms * 1e6).round() / 1e3;
        warn(
            lua,
            &format!(
                "`{what} = {ms}` is milliseconds, under a frame; for {ms} seconds write {as_ms}"
            ),
        );
    } else if !looping && ms > 60_000. {
        warn(
            lua,
            &format!(
                "`{what} = {ms}` is milliseconds: over a minute ({} s)",
                ms / 1000.
            ),
        );
    }
}

/// A script error, split for the banner: where it happened (`app.luau:12`),
/// what happened, and the lines after (the traceback).
#[derive(Debug, PartialEq)]
struct ErrorText {
    location: Option<String>,
    message: String,
    details: Vec<String>,
}
fn split_error(text: &str) -> ErrorText {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "stack traceback:" && !l.starts_with("[C]:"))
        .collect();
    // The headline: the first line naming a script line, else the first.
    let index = lines.iter().position(|l| locate(l).is_some()).unwrap_or(0);
    let headline = lines.get(index).copied().unwrap_or_default();
    let (location, message) = match locate(headline) {
        Some((start, end)) => (
            Some(headline[start..end].to_string()),
            headline[end..].trim_start_matches(':').trim().to_string(),
        ),
        None => (None, headline.to_string()),
    };
    let details = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, l)| l.to_string())
        .collect();
    ErrorText {
        location,
        message,
        details,
    }
}
/// The byte range of the first `chunk:12` followed by `:` in `line` (a
/// script's name and line, as Luau reports them: `app.luau:12: …`).
fn locate(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut search = 0;
    while let Some(found) = line[search..].find(':') {
        let colon = search + found;
        search = colon + 1;
        let digits = bytes[colon + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 || bytes.get(colon + 1 + digits) != Some(&b':') {
            continue;
        }
        let start = line[..colon]
            .rfind(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '[')
            .map_or(0, |i| i + 1);
        let chunk = &line[start..colon];
        if chunk.is_empty() || chunk.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        return Some((start, colon + 1 + digits));
    }
    None
}

/// The error banner: small and dark over the bottom of the window, the
/// location in monospace, then the message and as much of the traceback
/// as fits; × dismisses it.
fn banner(text: &str, dismiss: impl Fn() + 'static) -> View {
    let parts = split_error(text);
    let mono = |line: String| {
        zgui::compose::text(line)
            .font_family(FontFamily::Monospace)
            .text_size(11.)
    };
    let mut headline = row().w_full().gap(8.).items_start();
    if let Some(location) = parts.location {
        headline = headline.child(
            mono(location)
                .id("script-error-location")
                .text_size(12.)
                .text_color(rgb(0xff8f8f)),
        );
    }
    headline = headline
        .child(
            zgui::compose::text(parts.message)
                .id("script-error")
                .text_size(12.5)
                .text_color(rgb(0xf1f2f5))
                .text_wrap(true)
                .grow()
                .flex_shrink(1.)
                .min_w(0.),
        )
        .child(
            button()
                .id("script-error-dismiss")
                .on_click(dismiss)
                .w(18.)
                .h(18.)
                .rounded(4.)
                .items_center()
                .justify_center()
                .text_color(rgb(0x9aa0ab))
                .hover(|s| s.bg(rgba(0xffffff1a)).text_color(rgb(0xf1f2f5)))
                .child(zgui::compose::text("×").text_size(13.)),
        );
    let details = parts
        .details
        .into_iter()
        .take(8)
        .map(|line| mono(line).text_color(rgb(0x9aa0ab)).truncate());
    column()
        .id("script-error-banner")
        .absolute()
        .left(12.)
        .right(12.)
        .bottom(12.)
        .max_h(200.)
        .overflow_hidden()
        .px(12.)
        .py(9.)
        .gap(3.)
        .rounded(8.)
        .bg(rgb(0x17191e))
        .border(1.)
        .border_color(rgba(0xff6b6b59))
        .child(headline)
        .children(details)
}

#[derive(Clone)]
enum Script {
    File(PathBuf),
    Inline {
        name: String,
        code: String,
    },
    /// A plugin the app manages (`set_plugins`): named by the app, read
    /// from the disk.
    Plugin {
        name: String,
        path: PathBuf,
    },
}
/// What a plugin may do beyond its sandbox, if its host grants it
/// (`ScriptHost::grant`). The application's own scripts may do all of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Capability {
    /// `ui.fetch`: the network, through the host's fetcher.
    Fetch,
    /// `ui.clipboard`: read and write the clipboard.
    Clipboard,
    /// `ui.open_files`, `ui.save_file` and reading and writing what the
    /// user picks in them.
    Files,
    /// `ui.term`: terminal sessions, to read and to type into.
    Terminals,
    /// `ui.web` and `ui.webview`: web pages.
    Web,
    /// `ui.window`: windows of its own, which could pose as the app's or
    /// the system's (asking for a password).
    Windows,
    /// What the app defines (`expose_action`, `expose_shared`): a host
    /// action or shared state a plugin reaches only if granted it.
    Named(String),
}

/// A plugin an app loads (`ScriptHost::set_plugins`): its name (what errors
/// and grants say), its entry script on the disk, and what it may do.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    /// What errors and grants call it.
    pub name: String,
    /// Its entry script.
    pub entry: PathBuf,
    /// What it may do beyond its sandbox.
    pub grants: Vec<Capability>,
}

/// What a script may do beyond its sandbox: everything (the application's
/// own scripts) or what its host granted it (a plugin).
#[derive(Clone, Debug)]
pub(crate) enum Grants {
    All,
    Only(Vec<Capability>),
}
impl Grants {
    pub(crate) fn allows(&self, capability: &Capability) -> bool {
        match self {
            Grants::All => true,
            Grants::Only(granted) => granted.contains(capability),
        }
    }
}
/// The host actions and shared keys plugins reach (`expose_*`), and what
/// capability each needs, if any. A plugin reaches nothing else of the
/// app's; the app's own scripts reach all of it.
pub(crate) type Exposed = Rc<RefCell<HashMap<String, Option<Capability>>>>;

/// Where a host's files come from: the disk, or a bundle compiled into the
/// app (which reads nothing from the disk).
#[derive(Clone, Copy)]
pub(crate) enum Files {
    Disk,
    Bundle(&'static Bundle),
}
impl Files {
    /// A chunk read from here, loading as the files allow: an app's bundle
    /// holds the bytecode its build compiled from its own scripts; what
    /// comes from the disk is source, always. Luau checks no bytecode it
    /// loads, so a file of crafted bytecode (a plugin's, or a module one
    /// requires) would corrupt the VM and step out of the sandbox: read as
    /// source, it just fails to parse.
    fn chunk(self, chunk: mlua::chunk::Chunk<'_>) -> mlua::chunk::Chunk<'_> {
        match self {
            Files::Bundle(_) => chunk,
            Files::Disk => chunk.set_mode(mlua::chunk::ChunkMode::Text),
        }
    }
    pub(crate) fn read(self, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
        match self {
            Files::Disk => std::fs::read(path),
            Files::Bundle(bundle) => bundle.get(path).map(<[u8]>::to_vec).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "not in the app's bundle")
            }),
        }
    }
    fn read_to_string(self, path: &std::path::Path) -> std::io::Result<String> {
        String::from_utf8(self.read(path)?).map_err(std::io::Error::other)
    }
    fn is_dir(self, path: &std::path::Path) -> bool {
        match self {
            Files::Disk => path.is_dir(),
            Files::Bundle(bundle) => bundle.is_dir(path),
        }
    }
    /// The files in directory `dir`.
    fn list(self, dir: &std::path::Path) -> Vec<PathBuf> {
        match self {
            Files::Disk => std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .collect(),
            Files::Bundle(bundle) => bundle.list(dir),
        }
    }
}
impl Script {
    /// Shown in errors and tracebacks.
    fn name(&self) -> String {
        match self {
            Script::File(path) => path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            Script::Inline { name, .. } | Script::Plugin { name, .. } => name.clone(),
        }
    }
    /// The directory a file script is in.
    fn dir(&self) -> Option<PathBuf> {
        match self {
            Script::File(path) | Script::Plugin { path, .. } => path.parent().map(PathBuf::from),
            Script::Inline { .. } => None,
        }
    }
    /// Keys the script's state across reloads.
    fn key(&self) -> String {
        match self {
            Script::File(path) => path.display().to_string(),
            Script::Inline { name, .. } => format!("inline:{name}"),
            Script::Plugin { name, .. } => format!("plugin:{name}"),
        }
    }
    /// Its source, or (from a bundle) its bytecode.
    fn code(&self, files: Files) -> Result<Vec<u8>, String> {
        match self {
            Script::File(path) | Script::Plugin { path, .. } => files
                .read(path)
                .map_err(|e| format!("{}: {e}", path.display())),
            Script::Inline { code, .. } => Ok(code.clone().into_bytes()),
        }
    }
}

struct Loaded {
    _vms: Vec<Rc<Vm>>,
    /// Plugins left out because they failed, and why.
    broken: Vec<String>,
    root: Rc<node::Node>,
    lowering: Rc<lower::Lowering>,
}

type ErrorHooks = Rc<RefCell<Vec<Rc<dyn Fn(&str)>>>>;
struct Inner {
    runtime: Runtime,
    app: Script,
    files: Files,
    /// `async_action`s, which `ui.call` runs.
    async_actions: calls::AsyncActions,
    /// The transport `ui.fetch` and remote images use (`fetcher`).
    fetcher: calls::SharedFetcher,
    /// What each plugin may do beyond its sandbox (`grant`), by name.
    grants: RefCell<HashMap<String, Vec<Capability>>>,
    /// The plugins the app manages (`set_plugins`).
    managed: RefCell<Vec<Script>>,
    /// Host actions and shared keys plugins reach (`expose_*`).
    exposed_actions: Exposed,
    exposed_shared: Exposed,
    /// Tells `watch` to look at what it watches again (after a reload).
    rewatch: RefCell<Option<zgui::task::LatestSender<()>>>,
    /// The window's clipboard and dialogs, once the view mounts in one.
    platform: io::SharedPlatform,
    /// The files the user picked in this host's dialogs.
    picked: io::Picked,
    /// `ui.args` and `ui.on_open` (`ScriptHost::args`, `open`).
    opens: Rc<opens::Opens>,
    /// Whether errors show over the interface (`error_banner`).
    show_errors: bool,
    /// What hears each script error (`on_error`), and what follows them.
    error_hooks: ErrorHooks,
    error_watch: RefCell<Option<Box<dyn std::any::Any>>>,
    plugin_dir: Option<PathBuf>,
    plugins: Vec<Script>,
    limits: Limits,
    stores: RefCell<HashMap<String, Rc<Store>>>,
    shared: Rc<Store>,
    /// Keeps `ui.window.titlebar_area` and `controls` as the window's
    /// (`window_chrome`).
    chrome: RefCell<Option<zgui::reactive::Effect>>,
    elements: Rc<element::Elements>,
    engine: Rc<anime::Engine>,
    assets: Rc<lower::Assets>,
    actions: Actions,
    functions: Functions,
    current: RefCell<Option<Rc<Loaded>>>,
    generation: Signal<u64>,
    error: Signal<Option<String>>,
    stamps: RefCell<Vec<(PathBuf, Option<SystemTime>)>>,
    /// Files scripts `require`d in the last load: watched with the scripts.
    required: RefCell<Vec<PathBuf>>,
    /// Terminal sessions (`ui.term`): they outlive reloads.
    terms: Rc<term::Terms>,
    /// Web pages by id (`ui.web(id)`): they outlive reloads too.
    webs: Rc<node::Webs>,
    /// App commands (`ui.command`) the last successful load declared.
    commands: Rc<command::Commands>,
    /// Windows the scripts declare (`ui.window`).
    windows: Rc<windows::Windows>,
    /// State saved across runs (`ui.persist`).
    persist: Rc<persist::Persist>,
    /// `ui.every` and `ui.after`.
    timers: Rc<timers::Timers>,
    /// Script warnings so far.
    warnings: SharedWarnings,
    /// Nodes record where scripts create them (see `Vm::locate`).
    locate: Rc<Cell<bool>>,
    /// Calls into scripts so far (`script_calls`).
    calls: Rc<Cell<u64>>,
    /// The window's layout inspector, from the mounted view's services.
    inspector: Rc<RefCell<Option<zgui::inspect::Inspector>>>,
    /// `inspect(true)` before the view mounted.
    inspect: Cell<bool>,
    /// Asks the view's task to reload, so nodes record their sources.
    relocate: Rc<Notify>,
    /// Type check after reloads (`check_on_reload`, `ZGUI_CHECK`); unset,
    /// a debug build's `watch` turns it on when luau-lsp is installed.
    check: Cell<Option<bool>>,
    /// The check running since the last reload.
    checker: RefCell<Option<check::Background>>,
    type_errors: Signal<Option<String>>,
    /// What `zgui lint` finds in the scripts as loaded (debug builds).
    lint: Signal<Option<String>>,
    /// The theme file error shown, if any (cleared when the file reads).
    theme_error: RefCell<Option<String>>,
}

/// A wake-up for one waiting task: `notify` before or during the wait ends it.
#[derive(Default)]
struct Notify {
    set: Cell<bool>,
    waker: RefCell<Option<std::task::Waker>>,
}
impl Notify {
    fn notify(&self) {
        self.set.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
    fn wait(self: &Rc<Self>) -> impl std::future::Future<Output = ()> + use<> {
        let this = self.clone();
        std::future::poll_fn(move |cx| {
            if this.set.replace(false) {
                std::task::Poll::Ready(())
            } else {
                *this.waker.borrow_mut() = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        })
    }
}

/// Whether `ZGUI_INSPECT` or `ZGUI_LAYOUT_WARNINGS` asks for diagnostics.
fn diagnostics_requested() -> bool {
    ["ZGUI_INSPECT", "ZGUI_LAYOUT_WARNINGS"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0"))
}

/// Loads an application script and its plugins and mounts what they describe.
#[derive(Clone)]
pub struct ScriptHost(Rc<Inner>);
/// A script run: its VM, the view it returned and its slot contributions.
type Ran = (
    Rc<Vm>,
    Option<Rc<node::Node>>,
    Vec<(String, Rc<str>, Rc<node::Node>)>,
);

impl ScriptHost {
    /// A host for the application script at `path`.
    pub fn new(runtime: &Runtime, path: impl Into<PathBuf>) -> Self {
        Self::with(runtime, Script::File(path.into()))
    }
    /// A host for script source held in memory.
    pub fn inline(runtime: &Runtime, name: impl Into<String>, code: impl Into<String>) -> Self {
        Self::with(
            runtime,
            Script::Inline {
                name: name.into(),
                code: code.into(),
            },
        )
    }
    fn with(runtime: &Runtime, app: Script) -> Self {
        let elements = Rc::new(element::Elements::default());
        let warnings = SharedWarnings::default();
        if let Some(mode) = DesignLint::from_env() {
            let mut w = warnings.borrow_mut();
            w.design = mode;
            w.design_forced = true;
        }
        let host = Self(Rc::new(Inner {
            runtime: runtime.clone(),
            app,
            files: Files::Disk,
            async_actions: Rc::default(),
            fetcher: Rc::default(),
            grants: RefCell::default(),
            managed: RefCell::default(),
            exposed_actions: Rc::default(),
            exposed_shared: Rc::default(),
            rewatch: RefCell::default(),
            platform: Rc::default(),
            picked: Rc::default(),
            opens: Rc::default(),
            show_errors: cfg!(debug_assertions),
            error_hooks: Rc::default(),
            error_watch: RefCell::default(),
            plugin_dir: None,
            plugins: Vec::new(),
            limits: Limits::default(),
            stores: RefCell::default(),
            elements: elements.clone(),
            engine: {
                let engine = anime::Engine::new(runtime, elements.clone());
                let weak = Rc::downgrade(&engine);
                elements.set_wake(Rc::new(move || {
                    if let Some(engine) = weak.upgrade() {
                        engine.wake();
                    }
                }));
                engine
            },
            assets: Rc::default(),
            shared: Rc::new(Store::new(runtime)),
            chrome: RefCell::default(),
            actions: Actions::default(),
            functions: Functions::default(),
            current: RefCell::default(),
            generation: runtime.signal(0),
            error: runtime.signal(None),
            stamps: RefCell::default(),
            required: RefCell::default(),
            terms: Rc::default(),
            webs: Rc::default(),
            commands: Rc::new(command::Commands::new(runtime)),
            windows: Rc::new(windows::Windows::new(runtime)),
            persist: persist::Persist::new(runtime, warnings.clone()),
            timers: Rc::default(),
            warnings,
            locate: Rc::new(Cell::new(diagnostics_requested())),
            calls: Rc::default(),
            inspector: Rc::default(),
            inspect: Cell::new(false),
            relocate: Rc::default(),
            check: Cell::new(
                std::env::var_os("ZGUI_CHECK")
                    .filter(|v| !v.is_empty())
                    .map(|v| v != "0"),
            ),
            checker: RefCell::default(),
            type_errors: runtime.signal(None),
            lint: runtime.signal(None),
            theme_error: RefCell::default(),
        }));
        host.0
            .assets
            .connect(host.0.timers.clone(), host.0.fetcher.clone());
        host
    }
    /// The scripts compiled into the app (`include_bundle!`), starting at
    /// `entry` (`"app.luau"`): what ships. Nothing is read from the disk,
    /// `require`, `.luaurc` aliases, theme files, images and plugins
    /// included, and `watch` watches nothing.
    pub fn bundled(runtime: &Runtime, bundle: &'static Bundle, entry: &str) -> Self {
        let root = PathBuf::from(bundle.root);
        let host = Self::with(runtime, Script::File(root.join(entry)))
            .configure(|inner| inner.files = Files::Bundle(bundle));
        host.0.assets.set_files(Files::Bundle(bundle));
        // Theme files from the bundle; other hosts' still from the disk.
        zgui::theme::Themes::of(runtime).set_reader(Some(Rc::new(
            move |path: &std::path::Path| {
                if path.starts_with(&root) {
                    Files::Bundle(bundle).read_to_string(path)
                } else {
                    std::fs::read_to_string(path)
                }
            },
        )));
        host
    }
    /// Whether a script error shows over the interface, with where it
    /// happened (development), or not (what ships: the last good interface
    /// keeps running and [`on_error`](Self::on_error) hears it). On in debug
    /// builds, off in release ones.
    pub fn error_banner(self, show: bool) -> Self {
        self.configure(|inner| inner.show_errors = show)
    }
    /// Call `hook` with each script error (a failed load, an error in a
    /// handler): for an app's log or crash reports.
    pub fn on_error(&self, hook: impl Fn(&str) + 'static) {
        self.0.error_hooks.borrow_mut().push(Rc::new(hook));
        if self.0.error_watch.borrow().is_some() {
            return;
        }
        let (error, hooks) = (self.0.error.clone(), self.0.error_hooks.clone());
        let watch = self.0.runtime.effect(move || {
            if let Some(message) = error.get() {
                let hooks: Vec<_> = hooks.borrow().clone();
                for hook in hooks {
                    hook(&message);
                }
            }
        });
        *self.0.error_watch.borrow_mut() = Some(Box::new(watch));
    }
    fn configure(self, f: impl FnOnce(&mut Inner)) -> Self {
        let mut inner = Rc::try_unwrap(self.0)
            .unwrap_or_else(|_| panic!("configure a ScriptHost before cloning it"));
        f(&mut inner);
        Self(Rc::new(inner))
    }
    /// Loads every `.luau` file in `dir` as a plugin.
    pub fn plugins(self, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        self.configure(|inner| inner.plugin_dir = Some(dir))
    }
    /// Adds a plugin from source held in memory.
    pub fn plugin(self, name: impl Into<String>, code: impl Into<String>) -> Self {
        let script = Script::Inline {
            name: name.into(),
            code: code.into(),
        };
        self.configure(|inner| inner.plugins.push(script))
    }
    /// `ui.args`: what the app was launched with after its script (a
    /// command line's `notes.txt`).
    pub fn args(self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.0
            .opens
            .set_args(args.into_iter().map(Into::into).collect());
        self
    }
    /// Hand the scripts' `ui.on_open` handlers files and URLs the system
    /// asks the app to open (a file opened with it, a link, a second
    /// launch's): the files become ones they may read and write, as a
    /// dialog's would. Before a script registers a handler, they wait for it.
    pub fn open(&self, files: Vec<std::path::PathBuf>, urls: Vec<String>) {
        self.0.opens.open(&self.0.picked, files, urls);
    }
    /// Run scripts within `limits`: heap, and time to load and per call.
    pub fn limits(self, limits: Limits) -> Self {
        self.configure(|inner| inner.limits = limits)
    }
    /// Names the application: `ui.persist` saves under its data directory
    /// ([`app_data_dir`]), in `script-state`.
    pub fn app_id(self, id: &str) -> Self {
        let dir = app_data_dir(id).map(|dir| dir.join("script-state"));
        self.configure(|inner| inner.persist.set_dir(dir))
    }
    /// Where `ui.persist` saves, instead of the app's data directory. Without
    /// this or [`ScriptHost::app_id`], saved state lasts until the app quits.
    pub fn storage(self, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        self.configure(|inner| inner.persist.set_dir(Some(dir)))
    }
    /// The directory `ui.persist` saves in, if it has one.
    pub fn storage_dir(&self) -> Option<PathBuf> {
        self.0.persist.dir()
    }
    /// Saves every `ui.persist` change now and waits until it is on disk.
    /// Changes are otherwise saved in batches, 250 ms after the first, off
    /// the main thread, and when the host is dropped.
    pub fn save(&self) {
        self.0.persist.save();
    }

    /// Registers a Rust function scripts can bind with `ui.action(name, arg)`.
    pub fn action(&self, name: impl Into<String>, action: impl Fn(Value) + 'static) {
        self.0
            .actions
            .borrow_mut()
            .insert(name.into(), Rc::new(action));
    }
    /// Registers a Rust function scripts call for its value now, with
    /// `ui.call_sync(name, arg)`: a read of data the host keeps (the rows a
    /// virtualized list shows of a large diff), so the data never has to
    /// be copied into the script whole. It runs on the UI thread: keep it
    /// quick (no I/O; `async_action` is for that). Plugins reach it as
    /// they reach actions (`expose_action`).
    pub fn function(&self, name: impl Into<String>, function: impl Fn(Value) -> Value + 'static) {
        self.0
            .functions
            .borrow_mut()
            .insert(name.into(), Rc::new(function));
    }
    /// Registers an async Rust function scripts call with `ui.call(name,
    /// arg)`, getting a promise of its result: `Ok` settles it with the
    /// value, `Err` rejects it with the message. The future runs on the
    /// window's tasks (the main thread, so it need not be `Send`), woken by
    /// what it waits for; hand blocking work to a thread and await its
    /// answer (`zgui::task::latest`).
    pub fn async_action<F, Fut>(&self, name: impl Into<String>, action: F)
    where
        F: Fn(Value) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + 'static,
    {
        self.0.async_actions.borrow_mut().insert(
            name.into(),
            Rc::new(move |arg| -> calls::Call { Box::pin(action(arg)) }),
        );
    }
    /// An image the app made (a picture it decoded, rendered or processed),
    /// shown by `ui.image { src = "host:<name>" }` and `bg_image =
    /// "host:<name>"`: what shows it follows each replacement; `None`
    /// clears it.
    pub fn image(&self, name: &str, image: Option<std::sync::Arc<zgui::image::ImageData>>) {
        self.0.assets.set_hosted(name, image, &self.0.runtime);
    }
    /// The transport for `ui.fetch` (and remote images): the app owns the
    /// HTTP stack, headers and credentials (a token minted per request), and
    /// answers each `Request` with a `Response`, or an error message that
    /// rejects the script's promise. Without one, `ui.fetch` is an error.
    pub fn fetcher<F, Fut>(&self, fetch: F)
    where
        F: Fn(Request) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<Response, String>> + 'static,
    {
        *self.0.fetcher.borrow_mut() = Some(Rc::new(move |request| -> calls::Fetch {
            Box::pin(fetch(request))
        }));
    }
    /// Let plugin `plugin` (its file name, `stats.luau`, or `plugin`'s
    /// name) do `capability`. Plugins get none unless granted; the
    /// application's scripts have every one.
    pub fn grant(self, plugin: impl Into<String>, capability: Capability) -> Self {
        let plugin = plugin.into();
        self.configure(|inner| {
            inner
                .grants
                .get_mut()
                .entry(plugin)
                .or_default()
                .push(capability)
        })
    }
    /// The plugins the app loads, each with what it may do, from the next
    /// `reload` on (an app that asks its user what to allow loads a plugin
    /// once they have). Plugins read from the disk, a bundled app's too. A
    /// plugin that fails to load is left out and its error reported.
    pub fn set_plugins(&self, plugins: Vec<Plugin>) {
        let mut grants = self.0.grants.borrow_mut();
        let mut managed = self.0.managed.borrow_mut();
        for old in managed.drain(..) {
            if let Script::Plugin { name, .. } = old {
                grants.remove(&name);
            }
        }
        for plugin in plugins {
            grants.insert(plugin.name.clone(), plugin.grants);
            managed.push(Script::Plugin {
                name: plugin.name,
                path: plugin.entry,
            });
        }
    }
    /// Let plugins bind, invoke and call host action `name` (`action`,
    /// `async_action`): all of them, or (`Some`) those granted
    /// `capability`. Plugins reach no host action the app does not expose;
    /// the app's own scripts reach every one.
    pub fn expose_action(&self, name: impl Into<String>, capability: Option<Capability>) {
        self.0
            .exposed_actions
            .borrow_mut()
            .insert(name.into(), capability);
    }
    /// Let plugins read and write shared state `key` (`ui.shared`): all of
    /// them, or (`Some`) those granted `capability`. Plugins reach no shared
    /// state the app does not expose.
    pub fn expose_shared(&self, key: impl Into<String>, capability: Option<Capability>) {
        self.0
            .exposed_shared
            .borrow_mut()
            .insert(key.into(), capability);
    }
    /// State shared by the application and every script (`ui.shared`). Rust
    /// can publish model data here and observe what scripts write.
    /// Its `set` keeps the parts of the value that did not change (see
    /// [`Shared`]): publish a whole model each time, and only what changed
    /// costs anything.
    pub fn shared(&self, key: &str, initial: impl Into<Value>) -> Shared {
        Shared(self.0.shared.state(key, initial.into()))
    }
    /// Keep `ui.window.titlebar_area` and `ui.window.controls` as the
    /// window the view shows in has them: pass its
    /// `WindowInfo::titlebar_area` and `controls`. Each is a rect in points
    /// (`{ x, y, width, height }`), or nil where the window has none.
    pub fn window_chrome(
        &self,
        titlebar_area: Signal<Option<zgui::scene::Rect>>,
        controls: Signal<Option<zgui::scene::Rect>>,
    ) {
        fn rect(rect: Option<zgui::scene::Rect>) -> Value {
            rect.map_or(Value::Nil, |r| {
                let n = |v: f32| Value::Num(f64::from(v));
                Value::Map(Rc::from([
                    (Rc::from("height"), n(r.height)),
                    (Rc::from("width"), n(r.width)),
                    (Rc::from("x"), n(r.x)),
                    (Rc::from("y"), n(r.y)),
                ]))
            })
        }
        let area = self.0.shared.state(windows::TITLEBAR_AREA, Value::Nil);
        let buttons = self.0.shared.state(windows::CONTROLS, Value::Nil);
        let effect = self.0.runtime.effect_now(move || {
            area.set(rect(titlebar_area.get()));
            buttons.set(rect(controls.get()));
        });
        *self.0.chrome.borrow_mut() = Some(effect);
    }
    /// The app commands scripts declare (`ui.command`), in declaration
    /// order, as of the last successful load: show them as menu items.
    pub fn commands(&self) -> Signal<Vec<Command>> {
        self.0.commands.list.clone()
    }
    /// The windows the scripts declare (`ui.window`), as of the last good
    /// load: open each while its `open` state is true, showing
    /// `window_view(id)`, and set the state false when it is closed.
    pub fn windows(&self) -> Signal<Vec<Window>> {
        self.0.windows.list.clone()
    }
    /// Window `id`'s content (`ui.window`), following reloads; empty while
    /// no load declares it. Mount it in a window on this host's runtime.
    pub fn window_view(&self, id: &str) -> View {
        let host = self.0.clone();
        let generation = self.0.generation.clone();
        let id = id.to_owned();
        switch(
            move || generation.get(),
            move |_, _| {
                let current = host.current.borrow().clone();
                let content = host.windows.content.borrow().get(&id).cloned();
                match (current, content) {
                    (Some(loaded), Some(node)) => {
                        lower::lower(&node, &loaded.lowering).takes_unfocused_keys()
                    }
                    _ => div(),
                }
            },
        )
    }
    /// Run the command `id` (a menu item was chosen); whether one ran.
    pub fn run_command(&self, id: &str) -> bool {
        self.0.commands.run(id)
    }
    /// The warnings scripts have caused (a duration that reads as seconds,
    /// a saved value that is not JSON), each once, oldest first. They also
    /// go to stderr.
    pub fn warnings(&self) -> Vec<String> {
        self.0.warnings.borrow().list.clone()
    }
    /// How strictly the scripts' styles must use theme tokens: hard-coded
    /// colours, spacing, radii, type and shadows warn, or fail (see
    /// [`DesignLint`]). Scripts choose with `ui.theme.strict(mode)`;
    /// `ZGUI_DESIGN_LINT=off|warn|error` overrides both. It applies to what
    /// scripts build from now on: reload them to check what they built.
    pub fn design_lint(&self, mode: DesignLint) {
        let mut w = self.0.warnings.borrow_mut();
        if !w.design_forced {
            w.design = mode;
        }
    }
    /// How many times Rust has called into the scripts (a prop, an effect,
    /// a handler, a component built), counting from the host's start: what
    /// a test reads before and after an interaction to check that it runs
    /// no script (a resize the layout engine handles alone).
    pub fn script_calls(&self) -> u64 {
        self.0.calls.get()
    }
    /// Bytes the loaded scripts' VMs hold after a full collection (each is
    /// capped at `Limits::memory` or `Limits::plugin_memory`): what a test reads over many updates to
    /// check that nothing keeps what it no longer needs.
    pub fn script_memory(&self) -> usize {
        let loaded = self.0.current.borrow().clone();
        loaded.map_or(0, |l| {
            l._vms
                .iter()
                .map(|vm| {
                    let _ = vm.lua.gc_collect();
                    vm.lua.used_memory()
                })
                .sum()
        })
    }
    /// How many converted values the loaded scripts' VMs keep (each with
    /// its table, until nothing else holds the value): with
    /// `script_memory`, what tells data the scripts hold from data the
    /// host holds.
    pub fn converted_values(&self) -> usize {
        let loaded = self.0.current.borrow().clone();
        loaded.map_or(0, |l| {
            l._vms.iter().map(|vm| value::converted_len(&vm.lua)).sum()
        })
    }
    /// The latest load or script error, shown over the interface until fixed.
    pub fn error(&self) -> Signal<Option<String>> {
        self.0.error.clone()
    }
    /// Type errors the last background check found (`check_on_reload`),
    /// shown over the interface while there is no script error.
    pub fn type_errors(&self) -> Signal<Option<String>> {
        self.0.type_errors.clone()
    }
    /// What `zgui lint` found in the scripts at the last load, in a debug
    /// build (see `docs/scripting.md`), shown over the interface while there
    /// is no error or type error.
    pub fn lint_findings(&self) -> Signal<Option<String>> {
        self.0.lint.clone()
    }
    /// Turns the window's layout inspector on or off (`zgui::inspect`):
    /// boxes outlined, the one under the pointer described with the script
    /// line that created it, and layout warnings. Before the view mounts,
    /// it turns on with it. Ctrl+Option+Cmd+I toggles it in a desktop
    /// window (Ctrl+Alt+Shift+I elsewhere), as `ZGUI_INSPECT=1` does.
    pub fn inspect(&self, on: bool) {
        self.0.inspect.set(on);
        let inspector = self.0.inspector.borrow().clone();
        match inspector {
            Some(inspector) => inspector.set_enabled(on),
            // Record sources from the first load.
            None if on => self.0.locate.set(true),
            None => {}
        }
    }
    /// Type checks the scripts in the background after every reload, as
    /// `zgui check` does, and shows the errors over the interface (with
    /// `watch`, which collects the results). It needs luau-lsp; a check
    /// runs luau-lsp on another thread, about 0.1 s of its CPU for a
    /// twenty-module app. In a debug build `watch` turns it on when
    /// luau-lsp is installed; `ZGUI_CHECK=1` or `0` (or this) decides
    /// instead.
    pub fn check_on_reload(&self, on: bool) {
        self.0.check.set(Some(on));
        if !on {
            *self.0.checker.borrow_mut() = None;
            self.0.type_errors.set(None);
        }
    }
    /// Collects a finished background check (see `check_on_reload`).
    pub fn poll_check(&self) {
        let result = self.0.checker.borrow().as_ref().and_then(|c| c.take());
        let Some(result) = result else {
            return;
        };
        *self.0.checker.borrow_mut() = None;
        let shown = match result {
            Ok(report) if report.errors() == 0 => None,
            Ok(report) => {
                let errors: Vec<String> = report
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity == check::Severity::Error)
                    .map(|d| {
                        let file = d.file.file_name().unwrap_or(d.file.as_os_str());
                        let message = d.message.lines().next().unwrap_or_default();
                        format!(
                            "{}:{}:{}: {message}",
                            file.to_string_lossy(),
                            d.line,
                            d.column
                        )
                    })
                    .collect();
                Some(format!(
                    "type check: {} error{}\n{}",
                    errors.len(),
                    if errors.len() == 1 { "" } else { "s" },
                    errors.join("\n")
                ))
            }
            Err(check::CheckError::NotInstalled) => {
                eprintln!("zgui-script: check_on_reload: {}", check::INSTALL);
                self.0.check.set(Some(false));
                None
            }
            Err(error) => Some(format!("type check: {error}")),
        };
        if let Some(text) = &shown {
            eprintln!("{text}");
        }
        self.0.type_errors.set(shown);
    }
    /// In a debug build, `zgui lint` over the scripts as loaded, after each
    /// load: each finding once on stderr and in `warnings`, and over the
    /// interface while there is no error. Scripts read from a bundle, and
    /// `ZGUI_LINT=0`, skip it.
    fn lint_loaded(&self) {
        if !cfg!(debug_assertions)
            || matches!(self.0.files, Files::Bundle(_))
            || std::env::var_os("ZGUI_LINT").is_some_and(|v| v == "0")
        {
            return;
        }
        let mut files: Vec<PathBuf> = self
            .0
            .stamps
            .borrow()
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| path.extension().is_some_and(|e| e == "luau"))
            .collect();
        files.sort();
        files.dedup();
        let mut found = Vec::new();
        for file in &files {
            let Ok(source) = std::fs::read_to_string(file) else {
                continue;
            };
            for d in lint::lint_source(file, &source) {
                let name = d.file.file_name().unwrap_or(d.file.as_os_str());
                let line = format!(
                    "{}:{}:{}: lint[{}]: {}",
                    name.to_string_lossy(),
                    d.line,
                    d.column,
                    d.code,
                    d.message
                );
                warning(Some(&self.0.warnings), line.clone());
                found.push(line);
            }
        }
        self.0.lint.set((!found.is_empty()).then(|| {
            format!(
                "zgui lint: {} finding{}\n{}",
                found.len(),
                if found.len() == 1 { "" } else { "s" },
                found.join("\n")
            )
        }));
    }
    fn start_check(&self) {
        if self.0.check.get() != Some(true) {
            return;
        }
        let Script::File(app) = &self.0.app else {
            return;
        };
        let mut paths = vec![app.clone()];
        paths.extend(self.0.plugin_dir.iter().filter(|d| d.is_dir()).cloned());
        *self.0.checker.borrow_mut() =
            Some(check::Background::start(paths, check::Options::default()));
    }
    /// Connects the window's inspector: nodes record their sources while
    /// it records, which takes a reload to apply to what is mounted.
    fn attach(&self, inspector: &zgui::inspect::Inspector) {
        let attached = self.0.inspector.borrow().is_some();
        if attached {
            return;
        }
        *self.0.inspector.borrow_mut() = Some(inspector.clone());
        let weak = Rc::downgrade(&self.0);
        inspector.on_recording(move |recording| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if recording && !inner.locate.get() {
                inner.locate.set(true);
                inner.relocate.notify();
            } else if !recording {
                inner.locate.set(false);
            }
        });
        if self.0.inspect.get() {
            inspector.set_enabled(true);
        }
        if inspector.recording() && !self.0.locate.get() {
            self.0.locate.set(true);
            self.0.relocate.notify();
        }
    }

    /// Evaluates every script again and remounts the interface. State is kept.
    /// On failure the previous interface stays mounted and the error is shown.
    pub fn reload(&self) -> Result<(), String> {
        self.0.commands.begin();
        self.0.windows.begin();
        let loaded = self.load();
        // After loading: the files it required are watched too.
        *self.0.stamps.borrow_mut() = self.stamps();
        match loaded {
            Ok(loaded) => {
                let broken = (!loaded.broken.is_empty()).then(|| loaded.broken.join("\n"));
                if let Some(broken) = &broken {
                    eprintln!("{broken}");
                }
                *self.0.current.borrow_mut() = Some(Rc::new(loaded));
                self.0.commands.publish();
                self.0.windows.publish();
                // What the system asked to open before a script could take it.
                self.0.opens.deliver_waiting(&self.0.picked);
                self.0.runtime.batch(|| {
                    self.0.error.set(broken);
                    self.0.generation.update(|g| *g += 1);
                });
                // The replaced scripts' animations end with them.
                self.0.engine.drop_orphans();
                if let Some(rewatch) = &*self.0.rewatch.borrow() {
                    rewatch.send(());
                }
                self.lint_loaded();
                self.start_check();
                Ok(())
            }
            Err(error) => {
                eprintln!("{error}");
                self.0.error.set(Some(error.clone()));
                self.start_check();
                Err(error)
            }
        }
    }
    /// Reloads if any script file, or an image or SVG file it shows,
    /// changed since the last load. Theme files (`ui.theme.load`) that
    /// changed are read again without reloading the scripts: what their new
    /// values change restyles in place.
    pub fn reload_if_changed(&self) -> bool {
        let themes = self.reload_themes();
        // `changed` forgets edited images, so the reload reads them again.
        let assets = self.0.assets.changed();
        if self.stamps() == *self.0.stamps.borrow() && !assets {
            return themes;
        }
        let _ = self.reload();
        true
    }
    /// Read again the theme files that changed; whether any did. An error
    /// shows over the interface until the file reads again.
    fn reload_themes(&self) -> bool {
        match zgui::theme::Themes::of(&self.0.runtime).reload_if_changed() {
            Ok(changed) => {
                if changed
                    && let Some(shown) = self.0.theme_error.take()
                    && self
                        .0
                        .error
                        .with_untracked(|e| e.as_deref() == Some(&*shown))
                {
                    self.0.error.set(None);
                }
                changed
            }
            Err(error) => {
                let message = error.to_string();
                eprintln!("{message}");
                self.0.error.set(Some(message.clone()));
                *self.0.theme_error.borrow_mut() = Some(message);
                true
            }
        }
    }
    /// Reloads when script files change. Spawn this on the window's tasks
    /// during development: on macOS, Windows and Linux it waits for the
    /// system to say a file changed, elsewhere it stats them `every` so often. In a debug
    /// build it also turns on type checks after each reload (see
    /// `check_on_reload`) when luau-lsp is installed and nothing chose
    /// otherwise.
    pub fn watch(&self, every: Duration) -> impl std::future::Future<Output = ()> + 'static {
        if self.0.check.get().is_none()
            && cfg!(debug_assertions)
            && check::find_luau_lsp().is_some()
        {
            self.0.check.set(Some(true));
            // The load before this watch is checked too.
            if self.0.current.borrow().is_some() || self.0.error.with_untracked(Option::is_some) {
                self.start_check();
            }
        }
        let host = self.clone();
        async move {
            // A bundle never changes; the plugins beside it (on the disk)
            // are watched as any file is.
            // The kernel says when a file changes; nothing runs meanwhile.
            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            if let Some(mut events) = file_events::FileEvents::new() {
                // A reload (new plugins) has the watched files looked at again.
                let (rewatch, mut rewatched) = zgui::task::latest::<()>();
                *host.0.rewatch.borrow_mut() = Some(rewatch);
                let mut heard = false;
                loop {
                    // After a change, open them again: a file an editor
                    // replaced is a new file.
                    events.watch(host.watched_files(), host.watched_dirs(), heard);
                    // While a type check runs on its thread, its result is
                    // looked for this often; a change still wakes this at
                    // once.
                    let poll = host.0.checker.borrow().is_some().then_some(every);
                    match wake(&mut events, &mut rewatched, poll).await {
                        Woke::Changed => {
                            // An editor's save can be several writes: reload
                            // once they stop.
                            let mut waited = Duration::ZERO;
                            while waited < SETTLE_MAX {
                                match wake(&mut events, &mut rewatched, Some(SETTLE)).await {
                                    Woke::Changed => waited += SETTLE,
                                    Woke::Ended => return,
                                    Woke::Timer | Woke::Reloaded => break,
                                }
                            }
                            heard = true;
                        }
                        Woke::Ended => return,
                        // Reloaded: watch what it loads now.
                        Woke::Reloaded => {
                            heard = true;
                            continue;
                        }
                        Woke::Timer => heard = false,
                    }
                    host.reload_if_changed();
                    host.poll_check();
                }
            }
            if matches!(host.0.files, Files::Bundle(_)) && host.0.managed.borrow().is_empty() {
                return;
            }
            loop {
                zgui::timer::sleep(every).await;
                host.reload_if_changed();
                host.poll_check();
            }
        }
    }
    /// Every file a reload depends on: the scripts, what they required, the
    /// images they read and the theme files.
    #[cfg_attr(
        not(any(target_os = "macos", windows, target_os = "linux")),
        allow(dead_code)
    )]
    fn watched_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.stamps().into_iter().map(|(path, _)| path).collect();
        files.extend(self.0.assets.files());
        files.extend(zgui::theme::Themes::of(&self.0.runtime).files());
        files
    }
    /// The directories whose every entry a reload depends on: the plugins',
    /// so the first plugin dropped into an empty one loads.
    #[cfg_attr(
        not(any(target_os = "macos", windows, target_os = "linux")),
        allow(dead_code)
    )]
    fn watched_dirs(&self) -> Vec<PathBuf> {
        self.0.plugin_dir.iter().cloned().collect()
    }

    /// The mounted interface, with an error banner above it.
    pub fn view(&self) -> View {
        let host = self.0.clone();
        let generation = self.0.generation.clone();
        let app = switch(
            move || generation.get(),
            move |_, _| {
                let current = host.current.borrow().clone();
                // Its root's `on_key` hears what nothing focused takes.
                current.map_or_else(div, |loaded| {
                    lower::lower(&loaded.root, &loaded.lowering).takes_unfocused_keys()
                })
            },
        );
        let (error, type_errors) = (self.0.error.clone(), self.0.type_errors.clone());
        let lint = self.0.lint.clone();
        let (dismiss_error, dismiss_types, dismiss_lint) =
            (error.clone(), type_errors.clone(), lint.clone());
        let show_errors = self.0.show_errors;
        let engine = self.0.engine.clone();
        let persist = self.0.persist.clone();
        let timers = self.0.timers.clone();
        let host = self.clone();
        component(move |cx| {
            // One frame loop ticks every animation; it idles at rest.
            if let Some(frames) = cx.try_service::<zgui::frame::FrameClock>() {
                cx.tasks().spawn(engine.run((*frames).clone()));
            }
            // `ui.persist` saves on a timer from here, and `ui.every` runs.
            if cx.try_service::<zgui::compose::TaskRunner>().is_some() {
                persist.set_tasks(cx.tasks());
                timers.set_tasks(cx.tasks());
            }
            // The clipboard and file dialogs, from the window.
            if let Some(platform) = cx.try_service::<zgui::platform::PlatformService>() {
                *host.0.platform.borrow_mut() = Some((*platform).clone());
            }
            if let Some(inspector) = cx.try_service::<zgui::inspect::Inspector>() {
                // When the inspector starts recording, a reload records
                // where every node comes from. The task waits otherwise.
                let (relocate, weak) = (host.0.relocate.clone(), Rc::downgrade(&host.0));
                cx.tasks().spawn(async move {
                    loop {
                        relocate.wait().await;
                        match weak.upgrade() {
                            Some(inner) => {
                                let _ = ScriptHost(inner).reload();
                            }
                            None => return,
                        }
                    }
                });
                host.attach(&inspector);
            }
            // The banner exists only while there is an error: no node, no
            // work otherwise. It is absolute, over the interface and taking
            // none of its room; insets resolve against the window.
            let (error, type_errors, lint) = (error.clone(), type_errors.clone(), lint.clone());
            let (dismiss_error, dismiss_types, dismiss_lint) = (
                dismiss_error.clone(),
                dismiss_types.clone(),
                dismiss_lint.clone(),
            );
            let root = overlay().w_full().h_full().child(app.w_full().h_full());
            // `ui.toast`'s, while one shows: nothing otherwise.
            #[cfg(feature = "components")]
            let root = root.child_when(
                {
                    let toaster = zgui_components::Toaster::of(&cx.runtime());
                    move || toaster.any().then_some(())
                },
                |_, _| zgui_components::toast_host(),
            );
            root.child_when(
                move || {
                    show_errors
                        .then(|| {
                            error
                                .get()
                                .or_else(|| type_errors.get())
                                .or_else(|| lint.get())
                        })
                        .flatten()
                },
                move |message, _| {
                    let (error, types, lint) = (
                        dismiss_error.clone(),
                        dismiss_types.clone(),
                        dismiss_lint.clone(),
                    );
                    banner(&message, move || {
                        error.set(None);
                        types.set(None);
                        lint.set(None);
                    })
                },
            )
        })
    }

    fn scripts(&self) -> Vec<Script> {
        let mut files: Vec<_> = self
            .0
            .plugin_dir
            .as_ref()
            .map(|dir| self.0.files.list(dir))
            .into_iter()
            .flatten()
            .filter(|path| path.extension().is_some_and(|ext| ext == "luau"))
            .collect();
        files.sort();
        files.into_iter().map(Script::File).collect()
    }
    fn stamps(&self) -> Vec<(PathBuf, Option<SystemTime>)> {
        let mut paths: Vec<_> = self
            .scripts()
            .into_iter()
            .filter_map(|s| match s {
                Script::File(path) | Script::Plugin { path, .. } => Some(path),
                Script::Inline { .. } => None,
            })
            .collect();
        if let Script::File(path) = &self.0.app {
            paths.push(path.clone());
        }
        for script in self.0.managed.borrow().iter() {
            if let Script::Plugin { path, .. } = script {
                paths.push(path.clone());
            }
        }
        paths.extend(self.0.required.borrow().iter().cloned());
        paths
            .into_iter()
            .map(|path| {
                let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                (path, modified)
            })
            .collect()
    }

    fn load(&self) -> Result<Loaded, String> {
        self.0.required.borrow_mut().clear();
        let mut vms = Vec::new();
        let mut slots = node::Slots::new();
        let files = self.scripts();
        let mut broken = Vec::new();
        let managed = self.0.managed.borrow().clone();
        for script in self.0.plugins.iter().chain(&files).chain(&managed) {
            // A plugin that fails is left out, not the app: the error is
            // reported, and the app and the other plugins load.
            let (vm, root, contributions) = match self.run(script, true) {
                Ok(ran) => ran,
                Err(error) => {
                    broken.push(error);
                    continue;
                }
            };
            if root.is_some() {
                broken.push(format!(
                    "{}: a plugin contributes with ui.contribute; it returns nothing",
                    script.name()
                ));
                continue;
            }
            for (slot, owner, node) in contributions {
                slots.entry(slot).or_default().push((owner, node));
            }
            vms.push(vm);
        }
        let (vm, root, contributions) = self.run(&self.0.app, false)?;
        for (slot, owner, node) in contributions {
            slots.entry(slot).or_default().push((owner, node));
        }
        vms.push(vm);
        let root = root.ok_or_else(|| {
            format!(
                "{}: the application script must return a ui node",
                self.0.app.name()
            )
        })?;
        Ok(Loaded {
            _vms: vms,
            broken,
            root,
            lowering: Rc::new(lower::Lowering {
                slots,
                actions: self.0.actions.clone(),
                elements: self.0.elements.clone(),
                runtime: self.0.runtime.clone(),
                assets: self.0.assets.clone(),
                inspector: self.0.inspector.clone(),
                depth: Default::default(),
                #[cfg(feature = "components")]
                commands: self.0.commands.clone(),
            }),
        })
    }

    #[allow(clippy::type_complexity)]
    /// `require(path)`, as Luau resolves it: `"./card"` and `"../ui/card"`
    /// from the requiring file (from beside its directory for an
    /// `init.luau`), `"@alias/card"` from the `.luaurc` aliases
    /// beside the script, `"@self/card"` from the requiring file's own
    /// directory, a directory as its `init.luau`; a bare `"card"` from the
    /// script's directory. Modules stay inside the script's directory, run
    /// once and return what they return; a change to one reloads the scripts.
    fn install_require(&self, lua: &Lua, dir: Option<PathBuf>, files: Files) -> mlua::Result<()> {
        let host = Rc::downgrade(&self.0);
        let modules: Rc<RefCell<HashMap<PathBuf, Option<mlua::Value>>>> = Rc::default();
        let root = dir.map(|d| d.canonicalize().unwrap_or(d));
        let aliases = root
            .as_deref()
            .map(|root| read_aliases(root, files))
            .unwrap_or_default();
        let require = lua.create_function(move |lua, name: String| {
            let Some(root) = &root else {
                return Err(mlua::Error::runtime(
                    "require needs a script loaded from a file",
                ));
            };
            let fail = |why: String| mlua::Error::runtime(format!("require(\"{name}\"): {why}"));
            // The requiring file, from its chunk name (its path in the root).
            let caller = lua
                .inspect_stack(1, |d| d.source().source.map(|s| s.into_owned()))
                .flatten()
                .and_then(|s| s.strip_prefix('@').map(|rel| root.join(rel)));
            let here = caller
                .as_deref()
                .and_then(std::path::Path::parent)
                .unwrap_or(root)
                .to_path_buf();
            let joined = if let Some(rest) = name.strip_prefix("@self") {
                here.join(rest.trim_start_matches('/'))
            } else if let Some(rest) = name.strip_prefix('@') {
                let (alias, rest) = rest.split_once('/').unwrap_or((rest, ""));
                let base = aliases
                    .get(alias)
                    .ok_or_else(|| fail(format!("no alias `@{alias}` in .luaurc")))?;
                base.join(rest)
            } else if name.starts_with("./") || name.starts_with("../") {
                // An `init.luau` is its directory as a module: its relative
                // requires start beside that directory (as Luau's and
                // luau-lsp's do); `@self` reaches inside it.
                let is_init = caller
                    .as_deref()
                    .and_then(std::path::Path::file_stem)
                    .is_some_and(|stem| stem == "init");
                let base = if is_init {
                    here.parent().unwrap_or(&here)
                } else {
                    &here
                };
                base.join(&name)
            } else {
                root.join(&name)
            };
            let mut path = normalize(&joined);
            if files.is_dir(&path) {
                path = path.join("init.luau");
            } else if path.extension().is_none() {
                path.set_extension("luau");
            }
            if !within(root, &path) {
                return Err(fail(
                    "modules come from the script's directory or below".into(),
                ));
            }
            match modules.borrow().get(&path) {
                Some(Some(value)) => return Ok(value.clone()),
                Some(None) => return Err(fail("a require cycle".into())),
                None => {}
            }
            let code = files
                .read(&path)
                .map_err(|e| fail(format!("{}: {e}", path.display())))?;
            if let Some(host) = host.upgrade() {
                host.required.borrow_mut().push(path.clone());
            }
            modules.borrow_mut().insert(path.clone(), None);
            // Named by its path in the root: errors show it, and its own
            // requires resolve from it.
            let chunk = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let value = match files
                .chunk(lua.load(code).set_name(format!("@{chunk}")))
                .eval::<mlua::Value>()
            {
                Ok(mlua::Value::Nil) => mlua::Value::Boolean(true),
                Ok(value) => value,
                Err(e) => {
                    modules.borrow_mut().remove(&path);
                    return Err(e);
                }
            };
            modules.borrow_mut().insert(path, Some(value.clone()));
            Ok(value)
        })?;
        lua.globals().set("require", require)
    }

    /// Run `script`: the application's, or a `plugin` (sandboxed further:
    /// only what the host grants it).
    fn run(&self, script: &Script, plugin: bool) -> Result<Ran, String> {
        let name = script.name();
        let fail = |error: mlua::Error| describe(&name, &error);
        // A plugin outside the app's bundle reads from the disk.
        let files = match (self.0.files, script) {
            (Files::Bundle(bundle), Script::File(path) | Script::Plugin { path, .. })
                if plugin && !path.starts_with(bundle.root) =>
            {
                Files::Disk
            }
            (files, _) => files,
        };
        let grants = Rc::new(if plugin {
            Grants::Only(
                self.0
                    .grants
                    .borrow()
                    .get(&name)
                    .cloned()
                    .unwrap_or_default(),
            )
        } else {
            Grants::All
        });
        let code = script.code(files)?;
        #[cfg(all(luau_asserts, not(debug_assertions)))]
        warning(
            Some(&self.0.warnings),
            "Luau was built with its asserts, which slow scripts by about a fifth: \
             build with LUAU_CXXFLAGS=-DNDEBUG (docs/scripting.md, Cost)"
                .into(),
        );
        // The handle `Lua::new` returns collects all garbage when it drops,
        // a full pass over everything the script just built; a clone does
        // not, and the VM frees all it holds when the last one goes.
        let lua = Lua::new().clone();
        let limits = self.0.limits;
        let memory = if plugin {
            limits.plugin_memory
        } else {
            limits.memory
        };
        lua.set_memory_limit(memory).map_err(fail)?;
        let deadline: Rc<Cell<Option<Deadline>>> = Rc::default();
        let interrupt = interrupt::install(&lua, deadline.clone(), memory);
        let watch = interrupt.watch();
        let vm = Rc::new(Vm {
            name: name.clone(),
            lua: lua.clone(),
            _interrupt: interrupt,
            base: script.dir(),
            deadline,
            budget: limits.call,
            runtime: self.0.runtime.clone(),
            errors: self.0.error.clone(),
            layout_ids: Default::default(),
            locate: self.0.locate.clone(),
            calls: self.0.calls.clone(),
        });
        let store = self
            .0
            .stores
            .borrow_mut()
            .entry(script.key())
            .or_insert_with(|| Rc::new(Store::new(&self.0.runtime)))
            .clone();
        value::install_cache(&lua);
        markdown::install(&lua);
        lua.set_app_data(node::Env {
            vm: Rc::downgrade(&vm),
            store,
            shared: self.0.shared.clone(),
            contributions: RefCell::default(),
            engine: self.0.engine.clone(),
            runtime: self.0.runtime.clone(),
            scope: RefCell::default(),
            retained: RefCell::default(),
            actions: self.0.actions.clone(),
            functions: self.0.functions.clone(),
            persist: self.0.persist.clone(),
            warnings: self.0.warnings.clone(),
            timers: self.0.timers.clone(),
            grants: grants.clone(),
            exposed_actions: self.0.exposed_actions.clone(),
            exposed_shared: self.0.exposed_shared.clone(),
            owner: name.as_str().into(),
            persist_as: match &script {
                Script::Plugin { .. } => script.key().into(),
                _ => name.as_str().into(),
            },
            meter: plugin.then(|| meter::Meter::new(&lua, memory, watch)),
        });
        node::install(&lua).map_err(fail)?;
        let ui: mlua::Table = lua.globals().get("ui").map_err(fail)?;
        term::install(&lua, &ui, self.0.terms.clone()).map_err(fail)?;
        node::install_web(&lua, &ui, self.0.webs.clone()).map_err(fail)?;
        command::install(&lua, &ui, self.0.commands.clone()).map_err(fail)?;
        windows::install(&lua, &ui, self.0.windows.clone(), &self.0.shared).map_err(fail)?;
        let may = |capability: Capability| grants.allows(&capability);
        let fetch = may(Capability::Fetch).then(|| self.0.fetcher.clone());
        calls::install(&lua, &ui, self.0.async_actions.clone(), fetch).map_err(fail)?;
        io::install(
            &lua,
            &ui,
            self.0.platform.clone(),
            self.0.picked.clone(),
            may(Capability::Clipboard),
            may(Capability::Files),
        )
        .map_err(fail)?;
        theme::install(&lua, &ui, script.dir()).map_err(fail)?;
        opens::install(&lua, &ui, self.0.opens.clone(), plugin).map_err(fail)?;
        ui.set_readonly(true);
        anime::api::install(&lua).map_err(fail)?;
        self.install_require(&lua, script.dir(), files)
            .map_err(fail)?;
        lua.sandbox(true).map_err(fail)?;
        // Building runs untracked: reading state while describing the tree
        // must not subscribe whatever is currently mounting.
        let result = self.0.runtime.untracked(|| {
            vm.guard(limits.load, || {
                files
                    .chunk(lua.load(code).set_name(format!("@{name}")))
                    .eval::<mlua::Value>()
            })
        });
        let root = match result.map_err(fail)? {
            mlua::Value::Nil => None,
            mlua::Value::UserData(ud) if ud.is::<node::NodeUd>() => {
                Some(ud.borrow::<node::NodeUd>().map_err(fail)?.0.clone())
            }
            other => {
                return Err(format!(
                    "{name}: a script returns a ui node or nothing, not {}",
                    other.type_name()
                ));
            }
        };
        let contributions = lua
            .app_data_ref::<node::Env>()
            .map(|env| env.contributions.take())
            .unwrap_or_default();
        Ok((vm, root, contributions))
    }
}

/// How long a save's writes must stop for before it reloads (`watch`): an
/// editor's save as a write and a rename comes within a millisecond or two.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
const SETTLE: Duration = Duration::from_millis(10);
/// At most this long after a change, it reloads even if writes go on.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
const SETTLE_MAX: Duration = Duration::from_millis(100);

/// What woke `watch`.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
enum Woke {
    /// A watched file changed.
    Changed,
    /// Nothing can change any more.
    Ended,
    /// A reload asks for the watched files to be looked at again.
    Reloaded,
    /// `after` went by.
    Timer,
}
/// The first of a change, a reload's rewatch, or `after` going by.
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
async fn wake(
    events: &mut file_events::FileEvents,
    rewatched: &mut zgui::task::LatestReceiver<()>,
    after: Option<Duration>,
) -> Woke {
    let mut changed = std::pin::pin!(events.changed());
    let mut reloaded = std::pin::pin!(rewatched.recv());
    let mut timer = std::pin::pin!(async move {
        match after {
            Some(after) => zgui::timer::sleep(after).await,
            None => std::future::pending::<()>().await,
        }
    });
    std::future::poll_fn(|cx| {
        use std::task::Poll;
        if let Poll::Ready(more) = changed.as_mut().poll(cx) {
            return Poll::Ready(if more { Woke::Changed } else { Woke::Ended });
        }
        if reloaded.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Woke::Reloaded);
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Woke::Timer);
        }
        Poll::Pending
    })
    .await
}
