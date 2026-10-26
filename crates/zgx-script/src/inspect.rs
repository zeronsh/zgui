//! Devtools for agents: a running app answers `zgx inspect` (and its MCP
//! server) over a local socket, one JSON request a line, one JSON answer a
//! line. The requests name nodes as tests do ([`crate::driver`]): the tree,
//! a node, states, errors and lint findings; clicks, typing, keys, scrolls,
//! commands, setting a state, waiting for a condition.
//!
//! Built only with the `inspect` feature (zgx-run's default), and serving
//! only when the host asks ([`serve`]: `zgx run --inspect`, or
//! `ZGX_INSPECT_SOCKET=1`): otherwise there is no thread, no socket and no
//! bookkeeping. The socket is the user's alone: a Unix socket (mode 0600)
//! in a directory only they may enter (0700); on Windows, a loopback port
//! that answers only a request carrying the token written, for the user
//! alone, beside it.
//!
//! An answer is `{"ok":true,"text":…,"data":…}`: `text` is what the CLI
//! prints (compact, a node a line), `data` the same as JSON. A failure is
//! `{"ok":false,"error":…}`. See `docs/agents/inspect.md`.
use crate::{ScriptHost, Value, driver};
use std::{
    collections::VecDeque,
    future::Future,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    task::{Poll, Waker},
    time::{Duration, Instant},
};
use zgx::{compose::ViewHandle, inspect::Inspector, widgets::Ui};

/// What a session answers with: at once, or once a wait is over.
pub enum Reply {
    /// The answer, a JSON line.
    Now(String),
    /// A wait (`wait`, `sleep`): the answer once it is over.
    Later(Pin<Box<dyn Future<Output = String>>>),
}

/// A running app as devtools see it: its window's interface, its view and
/// its script host.
#[derive(Clone)]
pub struct Session(Rc<Parts>);
struct Parts {
    ui: zgx::widgets::WeakUi,
    handle: ViewHandle,
    host: ScriptHost,
    inspector: Option<Inspector>,
}

fn ok(text: String, data: Value) -> String {
    Value::record([
        ("ok", Value::Bool(true)),
        ("text", Value::from(text.as_str())),
        ("data", data),
    ])
    .to_json()
}
fn failed(error: impl Into<String>) -> String {
    let error: String = error.into();
    Value::record([
        ("ok", Value::Bool(false)),
        ("error", Value::from(error.as_str())),
    ])
    .to_json()
}

/// Snapshots kept for an app: older ones are removed.
const SNAPSHOTS_KEPT: u32 = 16;

/// Write `image` as a PNG beside the app's socket, readable by its user
/// alone: its path.
fn save_png(image: &zgx::image::ImageData) -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static TAKEN: AtomicU32 = AtomicU32::new(0);
    let n = TAKEN.fetch_add(1, Ordering::Relaxed);
    let dir = crate::inspect_client::socket_dir();
    make_private_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let pid = std::process::id();
    let path = dir.join(format!("{pid}-snapshot-{n}.png"));
    if let Some(old) = n.checked_sub(SNAPSHOTS_KEPT) {
        let _ = std::fs::remove_file(dir.join(format!("{pid}-snapshot-{old}.png")));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder =
        png::Encoder::new(std::io::BufWriter::new(file), image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(image.pixels()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// How frames since `from` (`trace::now_ms`) were paced: how many, the
/// time between them, and the work in each.
fn pacing(from: f64) -> (String, Value) {
    let frames: Vec<_> = zgx::trace::frames(usize::MAX)
        .into_iter()
        .filter(|f| f.ended_ms >= from)
        .collect();
    if frames.len() < 2 {
        return (
            format!("{} frames: nothing to pace", frames.len()),
            Value::record([("frames", Value::Num(frames.len() as f64))]),
        );
    }
    let median = |mut values: Vec<f64>| {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let gaps: Vec<f64> = frames
        .windows(2)
        .map(|w| w[1].ended_ms - w[0].ended_ms)
        .collect();
    let every = median(gaps.clone());
    let longest = gaps.iter().copied().fold(0., f64::max);
    // Later than half a refresh past its turn: a frame shown late.
    let late = gaps.iter().filter(|g| **g > every * 1.5).count();
    let work: Vec<f64> = frames
        .iter()
        .map(|f| {
            f.phases
                .iter()
                .filter(|p| p.0 != "wait")
                .map(|p| p.2 as f64)
                .sum::<f64>()
                / 1e3
        })
        .collect();
    let (most, at) =
        work.iter().zip(&frames).fold(
            (0., 0),
            |(most, at), (w, f)| {
                if *w > most { (*w, f.index) } else { (most, at) }
            },
        );
    let text = format!(
        "{} frames, every {every:.2} ms (median), longest gap {longest:.2} ms, {late} over 1.5 \
         frames; work {:.2} ms median, {most:.2} ms most (frame {at})",
        frames.len(),
        median(work.clone())
    );
    let round = |v: f64| Value::Num((v * 100.).round() / 100.);
    let data = Value::record([
        ("frames", Value::Num(frames.len() as f64)),
        ("interval_ms", round(every)),
        ("longest_gap_ms", round(longest)),
        ("late", Value::Num(late as f64)),
        ("work_median_ms", round(median(work))),
        ("work_most_ms", round(most)),
    ]);
    (text, data)
}

const HELP: &str = "commands (one JSON object a line, `cmd` and its fields):
  tree    [target] [depth] [budget=200] [all]   the interface, a node a line
  node    target                                 one node: kind, source, role, name, value, box
  find    target                                 every node a target matches
  state   [name] [full]                          named states and their values
  set     name value                             set a named state (value is JSON)
  click   target                                 press and release at its middle
  type    target text                            click it, then type text
  key     keys                                   e.g. \"Enter\", \"cmd+k\", \"alt+ArrowDown\"
  scroll  target dy                              scroll the viewport under it
  snapshot target [width=4096]                   its pixels as the window last drew them: a PNG, user-only
  record  [stop]                                 record the window's wheel input; stop answers the tape
  replay  target tape                            play a tape back through the window's input path: its pacing
  command id                                     run a ui.command
  errors                                         script error, type errors, lint, layout warnings
  lint                                           zgx lint over the app's scripts, file:line
  frames  [n=10]                                 the last frames that did anything: their phases, what ran and why
  wait    target [text] [gone] [timeout_ms=3000] until it shows (the text), or is gone
  sleep   ms                                     let the app run
targets: #id, \"label or text\", role:button, @12, row 3, [2], a >> b (inside)";

impl Session {
    /// The session of a window's interface `ui`, the script view mounted
    /// in it (`handle`) and its host; with the window's inspector, nodes
    /// are described with the script line that made them.
    pub fn new(
        ui: &Ui,
        handle: ViewHandle,
        host: &ScriptHost,
        inspector: Option<Inspector>,
    ) -> Self {
        // Names and values computed from state wait for a reader (an
        // assistive technology): devtools are one.
        ui.set_assistive_technology(true);
        if let Some(inspector) = &inspector {
            // Its labels (the script line of each node) while devtools run.
            inspector.set_warnings(true);
        }
        // Why things run, from now (bindings and effects made from now are
        // named; a reload names the script's).
        zgx::trace::set_enabled(true);
        Self(Rc::new(Parts {
            ui: ui.downgrade(),
            handle,
            host: host.clone(),
            inspector,
        }))
    }

    /// The answer to one request (a JSON object, a line).
    pub fn answer(&self, request: &str) -> Reply {
        let Some(request) = Value::from_json(request.trim()) else {
            return Reply::Now(failed("a request is one JSON object: {\"cmd\":\"tree\"}"));
        };
        let Some(mut ui) = self.0.ui.upgrade() else {
            return Reply::Now(failed("the window has closed"));
        };
        let field = |name: &str| request.field(name).cloned();
        let text = |name: &str| field(name).map(|v| v.to_string());
        let number = |name: &str| field(name).and_then(|v| v.as_f64());
        let Some(cmd) = text("cmd") else {
            return Reply::Now(failed("a request names its `cmd` (`help` lists them)"));
        };
        ui.prepare_frame();
        let target = |ui: &Ui| -> Result<zgx::scene::NodeId, String> {
            let target = text("target").ok_or("this command takes a `target`")?;
            driver::find(ui, &self.0.handle, &target)
        };
        let answer = match cmd.as_str() {
            "help" => Ok((HELP.to_string(), Value::Nil)),
            "tree" => (|| {
                let from = match text("target") {
                    Some(t) => Some(driver::find(&ui, &self.0.handle, &t)?),
                    None => None,
                };
                let options = driver::TreeOptions {
                    depth: number("depth").map(|d| d as usize),
                    budget: number("budget").map_or(200, |b| b as usize),
                    all: field("all").is_some_and(|v| v.truthy()),
                };
                let (lines, hidden) = driver::tree(&ui, &self.0.handle, from, &options);
                let data = Value::List(
                    lines
                        .iter()
                        .map(|l| {
                            let mut fields = vec![
                                ("n", Value::Num(l.number as f64)),
                                ("depth", Value::Num(l.depth as f64)),
                                (
                                    "box",
                                    Value::List(
                                        [l.bounds.x, l.bounds.y, l.bounds.width, l.bounds.height]
                                            .iter()
                                            .map(|v| Value::Num(v.round() as f64))
                                            .collect::<Vec<_>>()
                                            .into(),
                                    ),
                                ),
                            ];
                            if let Some(id) = &l.id {
                                fields.push(("id", Value::from(id.as_str())));
                            }
                            if let Some(role) = l.role {
                                fields.push(("role", Value::from(role)));
                            }
                            if let Some(text) = &l.text {
                                fields.push(("text", Value::from(text.as_str())));
                            }
                            if let Some(value) = &l.value {
                                fields.push(("value", Value::from(value.as_str())));
                            }
                            if l.focused {
                                fields.push(("focused", Value::Bool(true)));
                            }
                            if l.disabled {
                                fields.push(("disabled", Value::Bool(true)));
                            }
                            if let Some(c) = l.checked {
                                fields.push(("checked", Value::Bool(c)));
                            }
                            Value::record(fields)
                        })
                        .collect::<Vec<_>>()
                        .into(),
                );
                Ok((driver::tree_text(&lines, hidden), data))
            })(),
            "node" => target(&ui).map(|node| {
                let fields = driver::node(&ui, self.0.inspector.as_ref(), node);
                let text = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let data = Value::record(
                    fields
                        .iter()
                        .map(|(k, v)| (k.as_str(), Value::from(v.as_str()))),
                );
                (text, data)
            }),
            "find" => (|| {
                let t = text("target").ok_or("find takes a `target`")?;
                let nodes = driver::find_all(&ui, &self.0.handle, &driver::parse(&t)?)?;
                let (lines, _) = (
                    nodes
                        .iter()
                        .map(|n| {
                            let fields = driver::node(&ui, None, *n);
                            let get = |k: &str| {
                                fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone())
                            };
                            format!(
                                "{} {} {}",
                                get("node").unwrap_or_default(),
                                get("role").or(get("kind")).unwrap_or_default(),
                                get("name")
                                    .or(get("text"))
                                    .map(|t| format!("{t:?} "))
                                    .unwrap_or_default()
                                    + &get("box").unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>(),
                    (),
                );
                let data = Value::List(
                    nodes
                        .iter()
                        .map(|n| Value::Num(driver::number(*n) as f64))
                        .collect::<Vec<_>>()
                        .into(),
                );
                Ok((lines.join("\n"), data))
            })(),
            "state" => {
                let only = text("name");
                let full = field("full").is_some_and(|v| v.truthy());
                let states: Vec<_> = self
                    .0
                    .host
                    .states()
                    .into_iter()
                    .filter(|(name, _)| {
                        only.as_ref()
                            .is_none_or(|o| o == name || name.ends_with(&format!("::{o}")))
                    })
                    .collect();
                if let (Some(name), true) = (&only, states.is_empty()) {
                    Err(format!("no state named `{name}`"))
                } else {
                    let text = states
                        .iter()
                        .map(|(name, value)| {
                            let json = value.to_json();
                            let shown = if full || json.len() <= 200 {
                                json
                            } else {
                                format!(
                                    "{}… ({} bytes; `full` for all)",
                                    head(&json, 200),
                                    json.len()
                                )
                            };
                            format!("{name} = {shown}")
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let data = Value::record(states.iter().map(|(k, v)| (k.as_str(), v.clone())));
                    Ok((text, data))
                }
            }
            "set" => (|| {
                let name = text("name").ok_or("set takes a `name`")?;
                let value = field("value").unwrap_or(Value::Nil);
                self.0.host.set_state(&name, value)?;
                Ok((format!("{name} set"), Value::Nil))
            })(),
            "click" => target(&ui).map(|node| {
                let b = ui.scene.borrow().bounds(node);
                driver::click_at(&mut ui, b);
                (format!("clicked @{}", driver::number(node)), Value::Nil)
            }),
            "type" => (|| {
                let node = target(&ui)?;
                let typed = text("text").ok_or("type takes a `text`")?;
                driver::type_into(&mut ui, node, &typed);
                Ok((format!("typed into @{}", driver::number(node)), Value::Nil))
            })(),
            "key" => (|| {
                let keys = text("keys").ok_or("key takes `keys` (\"Enter\", \"cmd+k\")")?;
                let (key, modifiers) = driver::parse_keys(&keys)?;
                driver::press(&mut ui, key, modifiers);
                Ok((format!("pressed {keys}"), Value::Nil))
            })(),
            "snapshot" => (|| {
                let node = target(&ui)?;
                let width = number("width").map_or(4096, |w| w.clamp(1., 16384.) as u32);
                let image = ui.snapshot(node, width).ok_or(
                    "no picture: the node is hidden or not laid out, or this window takes none \
                     (macOS windows do)",
                )?;
                let path = save_png(&image)?;
                let path_text = path.display().to_string();
                Ok((
                    format!(
                        "@{} {}x{} px as last drawn → {path_text}",
                        driver::number(node),
                        image.width(),
                        image.height()
                    ),
                    Value::record([
                        ("path", Value::from(path_text.as_str())),
                        ("width", Value::Num(image.width() as f64)),
                        ("height", Value::Num(image.height() as f64)),
                    ]),
                ))
            })(),
            "scroll" => (|| {
                let node = target(&ui)?;
                driver::scroll(&mut ui, node, number("dy").unwrap_or(100.) as f32);
                Ok((format!("scrolled @{}", driver::number(node)), Value::Nil))
            })(),
            "command" => (|| {
                let id = text("id").ok_or("command takes an `id`")?;
                if self.0.host.run_command(&id) {
                    Ok((format!("ran {id}"), Value::Nil))
                } else {
                    Err(format!("no command `{id}`"))
                }
            })(),
            "errors" => {
                let host = &self.0.host;
                let mut lines = Vec::new();
                for (what, message) in [
                    ("error", host.error().with_untracked(Clone::clone)),
                    ("types", host.type_errors().with_untracked(Clone::clone)),
                    ("lint", host.lint_findings().with_untracked(Clone::clone)),
                ] {
                    if let Some(message) = message {
                        lines.push(format!("{what}: {message}"));
                    }
                }
                for warning in self
                    .0
                    .inspector
                    .as_ref()
                    .map(Inspector::warnings)
                    .unwrap_or_default()
                {
                    lines.push(format!("layout: {warning}"));
                }
                let text = if lines.is_empty() {
                    "none".to_string()
                } else {
                    lines.join("\n")
                };
                Ok((text, Value::Nil))
            }
            "lint" => (|| {
                let path = self
                    .0
                    .host
                    .script_path()
                    .ok_or("the app has no script file to lint")?;
                let dir = path.parent().map(PathBuf::from).unwrap_or(path);
                let report = crate::lint::lint(&[dir]).map_err(|e| e.to_string())?;
                let text: Vec<String> =
                    report.diagnostics.iter().map(ToString::to_string).collect();
                Ok((
                    if text.is_empty() {
                        "clean".to_string()
                    } else {
                        text.join("\n")
                    },
                    Value::Nil,
                ))
            })(),
            "frames" => Ok(self.frames(number("n").map_or(10, |n| n as usize))),
            "wait" => {
                let session = self.clone();
                let request = request.clone();
                return Reply::Later(Box::pin(async move { session.wait(&request).await }));
            }
            "record" => {
                let stop = field("stop").is_some_and(|v| v.truthy());
                if stop {
                    ui.wheel_tape(zgx::widgets::WheelTape::Stop).map(|tape| {
                        let events = tape.lines().filter(|l| !l.starts_with('#')).count();
                        (
                            format!("recorded {events} wheel events"),
                            Value::record([("tape", Value::from(tape.as_str()))]),
                        )
                    })
                } else {
                    ui.wheel_tape(zgx::widgets::WheelTape::Record).map(|_| {
                        (
                            "recording wheel input: scroll in the window, then `record stop`"
                                .to_string(),
                            Value::Nil,
                        )
                    })
                }
            }
            "replay" => {
                let played = (|| {
                    let node = target(&ui)?;
                    let tape = text("tape").ok_or("replay takes a `tape` (its text)")?;
                    // How long it plays: its last event's time.
                    let length = tape
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                        .and_then(|l| l.split_whitespace().next()?.parse::<f64>().ok())
                        .unwrap_or(0.)
                        .clamp(0., 60_000.);
                    let b = ui.scene.borrow().bounds(node);
                    let at = (b.x + b.width / 2., b.y + b.height / 2.);
                    let from = zgx::trace::now_ms();
                    let said = ui.wheel_tape(zgx::widgets::WheelTape::Play { tape, at })?;
                    Ok::<_, String>((said, from, length))
                })();
                return match played {
                    Err(error) => Reply::Now(failed(error)),
                    Ok((said, from, length)) => Reply::Later(Box::pin(async move {
                        // Its frames, and the momentum and edges' after it.
                        zgx::timer::sleep(Duration::from_millis(length as u64 + 400)).await;
                        let (text, data) = pacing(from);
                        ok(format!("{said}: {text}"), data)
                    })),
                };
            }
            "sleep" => {
                let ms = number("ms").unwrap_or(100.).clamp(0., 60_000.);
                return Reply::Later(Box::pin(async move {
                    zgx::timer::sleep(Duration::from_millis(ms as u64)).await;
                    ok(format!("slept {ms} ms"), Value::Nil)
                }));
            }
            other => Err(format!("no command `{other}` (`help` lists them)")),
        };
        // What an action changed, laid out before the next request reads it.
        ui.prepare_frame();
        Reply::Now(match answer {
            Ok((text, data)) => ok(text, data),
            Err(error) => failed(error),
        })
    }

    /// The last `n` frames that did anything: per frame, the states
    /// written and the effects they ran, each with its time.
    fn frames(&self, n: usize) -> (String, Value) {
        let names: std::collections::HashMap<usize, String> = self
            .0
            .host
            .named_states()
            .into_iter()
            .map(|(name, signal)| (signal.addr(), name))
            .collect();
        let name_of = |addr: usize| {
            names
                .get(&addr)
                .cloned()
                .unwrap_or_else(|| "a state".into())
        };
        // A binding names its node by number: its id too, where it has one.
        let ids: std::collections::HashMap<u32, String> = self
            .0
            .handle
            .ids()
            .into_iter()
            .map(|(node, id)| (node.number(), id))
            .collect();
        // And what the node is: how the script named it, else its kind.
        let kinds: std::collections::HashMap<u32, String> = match self.0.ui.upgrade() {
            Some(ui) => {
                let scene = ui.scene.borrow();
                let mut out = std::collections::HashMap::new();
                let mut stack = vec![self.0.handle.node()];
                while let Some(node) = stack.pop() {
                    let kind = self
                        .0
                        .inspector
                        .as_ref()
                        .and_then(|i| i.label_of(node))
                        .map(|l| match l.source {
                            Some(source) => format!("{} {source}", l.name),
                            None => l.name.to_string(),
                        })
                        .unwrap_or_else(|| zgx::inspect::kind_name(&scene, node));
                    out.insert(node.number(), kind);
                    stack.extend(scene.children(node).iter().copied());
                }
                out
            }
            None => Default::default(),
        };
        let named = |name: &str| -> String {
            let number = name
                .strip_prefix('@')
                .and_then(|r| r.split(' ').next()?.parse::<u32>().ok());
            let Some(number) = number else {
                return name.to_string();
            };
            let mut out = name.to_string();
            if let Some(id) = ids.get(&number) {
                out.push_str(&format!(" #{id}"));
            }
            if let Some(kind) = kinds.get(&number) {
                out.push_str(&format!(" ({kind})"));
            }
            out
        };
        let mut text = Vec::new();
        let mut data = Vec::new();
        for frame in zgx::trace::frames(n) {
            let total: f32 = frame.runs.iter().map(|r| r.micros).sum();
            // Named states by name; the rest (a component's, the engine's)
            // counted.
            let mut writes: Vec<String> = frame
                .writes
                .iter()
                .filter(|(s, _)| names.contains_key(s))
                .map(|(s, n)| {
                    if *n > 1 {
                        format!("{}×{n}", name_of(*s))
                    } else {
                        name_of(*s)
                    }
                })
                .collect();
            let unnamed = frame
                .writes
                .iter()
                .filter(|(s, _)| !names.contains_key(s))
                .count();
            if unnamed > 0 {
                writes.push(format!("{unnamed} unnamed"));
            }
            // The frame's time is its phases' (a phase inside another is
            // taken off that one, so each counts once), less what it waited
            // for the display: that is pacing, not work.
            let waited: f32 = frame
                .phases
                .iter()
                .filter(|p| p.0 == "wait")
                .map(|p| p.2)
                .sum();
            let frame_micros: f32 = frame.phases.iter().map(|p| p.2).sum::<f32>() - waited;
            let mut line = format!(
                "frame {} ({:.2} ms{}): {} → {} runs, {:.0} µs",
                frame.index,
                frame_micros / 1000.,
                if waited >= 1. {
                    format!(", then {:.2} ms waiting for the display", waited / 1000.)
                } else {
                    String::new()
                },
                if writes.is_empty() {
                    "no writes".into()
                } else {
                    writes.join(", ")
                },
                frame.runs.len(),
                total
            );
            // Where the frame's time went, the costliest phase first.
            let mut phases = frame.phases.clone();
            phases.sort_by(|a, b| b.2.total_cmp(&a.2));
            let split: Vec<String> = phases
                .iter()
                .map(|(name, instructions, micros)| match instructions {
                    0 => format!("{name} {micros:.0} µs"),
                    n => format!("{name} {micros:.0} µs ({}K instr)", n / 1000),
                })
                .collect();
            if !split.is_empty() {
                line.push_str(&format!("\n  phases: {}", split.join(", ")));
            }
            // The costliest runs first, a few: what to look at.
            let mut runs = frame.runs.clone();
            runs.sort_by(|a, b| b.micros.total_cmp(&a.micros));
            for run in runs.iter().take(5) {
                line.push_str(&format!(
                    "\n  {:.0} µs {}{}",
                    run.micros,
                    run.name
                        .as_deref()
                        .map_or_else(|| format!("effect {}", run.effect), named),
                    run.cause
                        .map(|c| format!(" ← {}", name_of(c)))
                        .unwrap_or_default()
                ));
            }
            if runs.len() > 5 {
                line.push_str(&format!("\n  … {} more", runs.len() - 5));
            }
            text.push(line);
            data.push(Value::record([
                ("frame", Value::Num(frame.index as f64)),
                (
                    "ended_ms",
                    Value::Num((frame.ended_ms * 100.).round() / 100.),
                ),
                (
                    "writes",
                    Value::List(
                        writes
                            .iter()
                            .map(|w| Value::from(w.as_str()))
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                ),
                ("runs", Value::Num(frame.runs.len() as f64)),
                ("micros", Value::Num(total.round() as f64)),
                ("frame_micros", Value::Num(frame_micros.round() as f64)),
                ("wait_micros", Value::Num(waited.round() as f64)),
                (
                    "phases",
                    Value::List(
                        phases
                            .iter()
                            .map(|(name, instructions, micros)| {
                                Value::record([
                                    ("phase", Value::from(*name)),
                                    ("micros", Value::Num(micros.round() as f64)),
                                    ("instructions", Value::Num(*instructions as f64)),
                                ])
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                ),
            ]));
        }
        let text = if text.is_empty() {
            "nothing ran".to_string()
        } else {
            text.join("\n")
        };
        (text, Value::List(data.into()))
    }

    async fn wait(&self, request: &Value) -> String {
        let field = |name: &str| request.field(name).map(|v| v.to_string());
        let Some(target) = field("target") else {
            return failed("wait takes a `target`");
        };
        let want = field("text");
        let gone = request.field("gone").is_some_and(|v| v.truthy());
        let timeout = request
            .field("timeout_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(3000.);
        let deadline = Instant::now() + Duration::from_millis(timeout.clamp(0., 120_000.) as u64);
        loop {
            let Some(ui) = self.0.ui.upgrade() else {
                return failed("the window has closed");
            };
            ui.prepare_frame();
            let found = driver::find(&ui, &self.0.handle, &target);
            let met = match (&found, gone) {
                (Err(_), true) => true,
                (Ok(_), true) => false,
                (Ok(node), false) => match &want {
                    None => true,
                    Some(text) => {
                        let scene = ui.scene.borrow();
                        driver::first_text(&scene, *node).is_some_and(|t| t.contains(text.as_str()))
                    }
                },
                (Err(_), false) => false,
            };
            if met {
                return ok(
                    format!(
                        "met after {} ms",
                        (timeout as u64).saturating_sub(
                            deadline
                                .saturating_duration_since(Instant::now())
                                .as_millis() as u64
                        )
                    ),
                    Value::Nil,
                );
            }
            if Instant::now() >= deadline {
                let why = match found {
                    Ok(node) => format!(
                        "still there: {}",
                        driver::first_text(&ui.scene.borrow(), node).unwrap_or_default()
                    ),
                    Err(error) => error,
                };
                return failed(format!("timed out after {timeout} ms ({why})"));
            }
            drop(ui);
            zgx::timer::sleep(Duration::from_millis(16)).await;
        }
    }
}

/// The first `max` bytes of `text`, cut at a character's start.
fn head(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Requests from the socket's threads, for the window's thread: each with
/// where its answer goes. Pushing wakes the task that answers them.
#[derive(Clone, Default)]
struct Queue(Arc<Mutex<Waiting>>);
/// A request, and where its answer goes.
type Job = (String, mpsc::Sender<String>);
/// The requests not yet answered, and the task to wake for the next.
type Waiting = (VecDeque<Job>, Option<Waker>);
impl Queue {
    fn push(&self, request: String, reply: mpsc::Sender<String>) {
        let waker = {
            let mut queue = self.0.lock().unwrap();
            queue.0.push_back((request, reply));
            queue.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    fn next(&self) -> impl Future<Output = Job> + '_ {
        std::future::poll_fn(move |cx| {
            let mut queue = self.0.lock().unwrap();
            match queue.0.pop_front() {
                Some(job) => Poll::Ready(job),
                None => {
                    queue.1 = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
    }
}

pub use crate::inspect_client::socket_dir;

/// A socket answering a session: it serves while this lives.
pub struct Server {
    path: PathBuf,
}
impl Server {
    /// Where it listens (a socket, or on Windows the file naming its port
    /// and token).
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Before the view mounts, for a host about to serve devtools: what it
/// mounts is named in traces from the first frame (`frames`).
pub fn prepare() {
    zgx::trace::set_enabled(true);
}

/// Whether the environment asks for devtools: `ZGX_INSPECT_SOCKET=1`.
pub fn asked() -> bool {
    zgx::env::var_os("ZGX_INSPECT_SOCKET").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Serve `session` on this user's socket for this process
/// (`socket_dir()/<pid>.sock`), its requests answered by a task `spawn`
/// runs on the window's thread. `about` says what the app is (its title,
/// its script) to `zgx inspect list`.
pub fn serve(
    session: Session,
    about: &str,
    spawn: impl FnOnce(Pin<Box<dyn Future<Output = ()>>>),
) -> std::io::Result<Server> {
    let dir = socket_dir();
    make_private_dir(&dir)?;
    let pid = std::process::id();
    let queue = Queue::default();
    let path = listen(&dir, pid, about, queue.clone())?;
    spawn(Box::pin(async move {
        loop {
            let (request, reply) = queue.next().await;
            let answer = match session.answer(&request) {
                Reply::Now(answer) => answer,
                Reply::Later(later) => later.await,
            };
            let _ = reply.send(answer);
        }
    }));
    Ok(Server { path })
}

#[cfg(unix)]
fn own_uid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    // SAFETY: getuid has no preconditions.
    unsafe { getuid() }
}

fn make_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        match std::fs::DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let meta = std::fs::symlink_metadata(dir)?;
        if !meta.is_dir() || meta.uid() != own_uid() {
            return Err(std::io::Error::other(format!(
                "{} is not a directory of this user's: devtools will not listen there",
                dir.display()
            )));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Each connection: lines in, answers out, through `queue`.
fn connection(stream: impl std::io::Read + Write, queue: Queue, token: Option<String>) {
    let mut stream = BufReader::new(stream);
    let mut line = String::new();
    let mut authorized = token.is_none();
    loop {
        line.clear();
        match stream.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if !authorized {
            let given =
                Value::from_json(line.trim()).and_then(|v| v.field("token").map(|t| t.to_string()));
            if given.as_deref() != token.as_deref() {
                let _ = writeln!(
                    stream.get_mut(),
                    "{}",
                    failed("a request carries the token beside the socket")
                );
                return;
            }
            authorized = true;
        }
        let (send, receive) = mpsc::channel();
        queue.push(line.clone(), send);
        let Ok(answer) = receive.recv() else { return };
        if writeln!(stream.get_mut(), "{answer}").is_err() {
            return;
        }
    }
}

#[cfg(unix)]
fn listen(dir: &std::path::Path, pid: u32, about: &str, queue: Queue) -> std::io::Result<PathBuf> {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let path = dir.join(format!("{pid}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let about_path = dir.join(format!("{pid}.json"));
    std::fs::write(
        &about_path,
        Value::record([("pid", Value::Num(pid as f64)), ("app", Value::from(about))]).to_json(),
    )?;
    std::fs::set_permissions(&about_path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::Builder::new()
        .name("zgx-inspect".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let queue = queue.clone();
                let _ = std::thread::Builder::new()
                    .name("zgx-inspect-client".into())
                    .spawn(move || connection(stream, queue, None));
            }
        })?;
    Ok(path)
}

#[cfg(not(unix))]
fn listen(dir: &std::path::Path, pid: u32, about: &str, queue: Queue) -> std::io::Result<PathBuf> {
    use std::net::TcpListener;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    // A token only this user can read: the port alone admits no one.
    let token = format!("{:032x}", {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(Instant::now().elapsed().as_nanos() ^ u128::from(pid));
        (u128::from(h.finish()) << 64)
            | u128::from(
                std::collections::hash_map::RandomState::new()
                    .build_hasher()
                    .finish(),
            )
    });
    let path = dir.join(format!("{pid}.json"));
    std::fs::write(
        &path,
        Value::record([
            ("pid", Value::Num(pid as f64)),
            ("app", Value::from(about)),
            ("port", Value::Num(port as f64)),
            ("token", Value::from(token.as_str())),
        ])
        .to_json(),
    )?;
    std::thread::Builder::new()
        .name("zgx-inspect".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let (queue, token) = (queue.clone(), token.clone());
                let _ = std::thread::Builder::new()
                    .name("zgx-inspect-client".into())
                    .spawn(move || connection(stream, queue, Some(token)));
            }
        })?;
    Ok(path)
}
