//! A terminal multiplexer written in Luau: tabs, split panes you can resize
//! and drag, and Ghostty terminals, on frosted glass.
//! Run: cargo run --release -p zgui-desktop --features ghostty --example terminal
//! The interface is `examples/terminal/app.luau` and the modules it
//! requires (see `docs/reference/app-structure.md`); edit and save to reload it
//! (shells keep running).
//!
//! Shells outlive the window: they run in a session server (this binary,
//! `--session-daemon`) and each pane attaches to its own (`--attach`), so
//! closing the app detaches them and opening it again, with the tabs and
//! panes it had, attaches them again (see `terminal/session.rs`).
#[path = "terminal/activity.rs"]
mod activity;
#[path = "terminal/plugins.rs"]
mod plugins;
#[path = "terminal/program_status.rs"]
mod program_status;
#[path = "terminal/session.rs"]
mod session;

use std::{path::Path, time::Duration};
use zgui_desktop::{Application, FontData, Glass, Titlebar, WindowOptions};
use zgui_script::{Capability, ScriptHost, Value};

const TITLE: &str = "Terminal";
const WIDTH: f64 = 1180.;
const HEIGHT: f64 = 760.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The session server, or a pane's client to it.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--session-daemon") {
        session::daemon();
    }
    if let Some(at) = args.iter().position(|a| a == "--attach") {
        let id = args.get(at + 1).cloned().unwrap_or_default();
        let cwd = args
            .iter()
            .position(|a| a == "--cwd")
            .and_then(|at| args.get(at + 1).cloned())
            .unwrap_or_default();
        session::attach(&id, &cwd);
    }
    session::ensure_daemon();
    zgui_desktop::resolve_terminal_processes(session::resolve);
    // The interface: compiled in (`terminal-bundle`, as it ships), unless
    // `ZGUI_SCRIPTS=source` asks for the source tree's; without it, the
    // source tree's, reloaded as it is edited.
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/terminal/app.luau");
    let from_source = !cfg!(feature = "terminal-bundle")
        || std::env::var("ZGUI_SCRIPTS").as_deref() == Ok("source");
    let mut application = Application::new();
    // Geist, the interface's typeface (`font.sans` in terminal/glass.json).
    for bytes in [
        &include_bytes!("support/fonts/Geist.ttf")[..],
        include_bytes!("support/fonts/Geist-Medium.ttf"),
        include_bytes!("support/fonts/Geist-SemiBold.ttf"),
    ] {
        application = application.font(FontData::new(std::sync::Arc::<[u8]>::from(bytes))?);
    }
    application
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((520., 360.)),
            // Frosted glass, content under inset traffic lights.
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            let host = if from_source {
                ScriptHost::new(&cx.ui.runtime, script.clone()).error_banner(true)
            } else {
                bundled(&cx.ui.runtime)
            };
            // What scripts save (`ui.persist`: a plugin's notes, an
            // editor's last file) beside the layout.
            let host = host.storage(session::dir().join("script-state"));
            host.on_error(|message| eprintln!("zterm: {message}"));
            // The script's view of the window: its corner radius (panes
            // stay concentric with it) and where the app was started.
            let radius = host.shared("window_radius", 10.);
            let corner = cx.window_info.corner_radius.clone();
            let follow_radius = cx.ui.runtime.effect(move || {
                let r = corner.get();
                if r > 0. {
                    radius.set(r.into());
                }
            });
            // Opened from the Finder or `open`, the app starts in `/`: new
            // shells start at home then, as Terminal's do.
            let launch = std::env::current_dir()
                .ok()
                .filter(|dir| dir != std::path::Path::new("/"))
                .or_else(|| std::env::var_os("HOME").map(Into::into))
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default();
            host.shared("launch_dir", launch.as_str());
            let home = std::env::var("HOME").unwrap_or_default();
            host.shared("home", home.as_str());
            // Panes attach to the session server by this command; it keeps
            // their shells. The layout they were in comes back from disk.
            let attach = session::attach_command("", "");
            let attach = attach.split(" --attach").next().unwrap_or_default();
            host.shared("attach", attach);
            let layout_file = session::dir().join("layout.json");
            let saved = std::fs::read_to_string(&layout_file)
                .ok()
                .and_then(|json| Value::from_json(&json))
                .unwrap_or(Value::Nil);
            host.shared("saved_layout", saved);
            host.action("end_session", |id| {
                if let Value::Str(id) = id {
                    session::kill(&id);
                }
            });
            // Every session's activity, from the session server as it
            // changes (an agent in a tab not shown, or finished while the
            // window was closed): what the agents sidebar lists. A thread
            // waits on the server; nothing runs while nothing changes.
            let agents = host.shared("agents", Value::Nil);
            let (heard, mut hearing) = zgui::task::latest::<String>();
            std::thread::spawn(move || session::watch(|json| heard.send(json)));
            cx.tasks.spawn(async move {
                while let Some(json) = hearing.recv().await {
                    agents.set(Value::from_json(&json).unwrap_or(Value::Nil));
                    // The snapshot says what runs in front of every session:
                    // the terminals take it from there, and nothing polls.
                    zgui_desktop::terminal_processes_pushed(session::fronts_told());
                    zgui_desktop::terminal_processes_changed();
                }
            });
            host.action("seen_session", |id| {
                if let Value::Str(id) = id {
                    session::seen(&id);
                }
            });
            host.action("keep_sessions", |ids| {
                let ids: Vec<String> = match ids {
                    Value::List(ids) => ids
                        .iter()
                        .filter_map(|id| match id {
                            Value::Str(id) => Some(id.to_string()),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                session::keep(&ids);
            });
            // Saved a moment after it changes (a divider drag changes it
            // on every move), and when the window closes. Nothing runs while
            // it does not change.
            let pending = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
            let save = {
                let (pending, layout_file) = (pending.clone(), layout_file.clone());
                move || {
                    if let Some(json) = pending.borrow_mut().take() {
                        let _ = std::fs::create_dir_all(session::dir());
                        let temp = layout_file.with_extension("json.tmp");
                        if std::fs::write(&temp, json).is_ok() {
                            let _ = std::fs::rename(&temp, &layout_file);
                        }
                    }
                }
            };
            {
                let (pending, save, tasks) = (pending.clone(), save.clone(), cx.tasks.clone());
                host.action("save_layout", move |layout| {
                    let waiting = pending.borrow_mut().replace(layout.to_json()).is_some();
                    if !waiting {
                        let save = save.clone();
                        tasks.spawn(async move {
                            zgui::timer::sleep(Duration::from_millis(500)).await;
                            save();
                        });
                    }
                });
            }
            let (window, zoomed) = (cx.window.clone(), std::cell::Cell::new(false));
            host.action("zoom_window", move |_| {
                zoomed.set(!zoomed.get());
                window.set_maximized(zoomed.get());
            });
            let window = cx.window.clone();
            host.action("close_window", move |_| window.close());
            let window = cx.window.clone();
            host.action("set_title", move |title| {
                if let Value::Str(title) = title {
                    window.set_title(title.to_string());
                }
            });

            // Plugins (terminal/plugins.rs): found in the plugins directory
            // when the app starts and when ⌘K opens, and run once the user
            // allows what they ask for. The interface lists them
            // (`plugins`); a change to the ones that run reloads it.
            let listed = host.shared("plugins", Value::Nil);
            host.shared(
                "plugins_dir",
                plugins::dir().to_string_lossy().into_owned().as_str(),
            );
            let found = plugins::find();
            listed.set(plugins::published(&found));
            let running = plugins::enabled(&found);
            host.set_plugins(running.clone());
            let (rescan, mut rescans) = zgui::task::latest::<()>();
            {
                let host = host.clone();
                cx.tasks.spawn(async move {
                    let mut running = running;
                    while rescans.recv().await.is_some() {
                        let found = plugins::find();
                        listed.set(plugins::published(&found));
                        let now = plugins::enabled(&found);
                        if now != running {
                            running = now;
                            host.set_plugins(running.clone());
                            let _ = host.reload();
                        }
                    }
                });
            }
            let again = rescan.clone();
            host.action("plugins_rescan", move |_| {
                again.send(());
            });
            host.action("plugin_decide", move |answer| {
                if let (Some(Value::Str(id)), Some(Value::Bool(allowed))) =
                    (answer.field("id"), answer.field("allowed"))
                {
                    plugins::decide(id, *allowed);
                    rescan.send(());
                }
            });
            host.action("open_plugins_folder", |_| {
                let _ = std::process::Command::new("open")
                    .arg(plugins::dir())
                    .spawn();
            });
            // What plugins reach: the agents list with `terminals` (as
            // `ui.term` is), the network with `network`; nothing else of
            // the app's.
            host.expose_shared("agents", Some(Capability::Terminals));
            host.fetcher(|request| {
                let (answer, mut answered) = zgui::task::latest();
                std::thread::spawn(move || answer.send(plugins::fetch(&request)));
                async move {
                    answered
                        .recv()
                        .await
                        .unwrap_or_else(|| Err("the request was dropped".into()))
                }
            });

            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));

            // The menus are the script's commands: their shortcuts work
            // wherever focus is, a terminal included.
            let commands = host.commands();
            let windows = cx.windows.clone();
            let menus = cx.ui.runtime.effect(move || {
                let list = commands.get();
                if let Err(error) = windows.set_menus(zgui_run::menus(&list)) {
                    eprintln!("terminal: menus: {error}");
                }
            });
            let commands = host.clone();
            cx.on_menu_action(move |action| {
                commands.run_command(&action.0);
            });
            let keep = std::cell::RefCell::new(Some((menus, follow_radius)));
            cx.on_closed(move || {
                save();
                drop(keep.borrow_mut().take());
            });
            cx.render(host.view());
        })
}

/// The interface compiled in (`terminal-bundle`).
#[cfg(feature = "terminal-bundle")]
fn bundled(runtime: &zgui::reactive::Runtime) -> ScriptHost {
    static UI: zgui_script::Bundle = zgui_script::include_bundle!("terminal");
    ScriptHost::bundled(runtime, &UI, "app.luau")
}
#[cfg(not(feature = "terminal-bundle"))]
fn bundled(_: &zgui::reactive::Runtime) -> ScriptHost {
    unreachable!("built without terminal-bundle")
}
