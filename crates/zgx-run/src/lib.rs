//! Running a zgx Luau script as a desktop app: its commands (`ui.command`)
//! as the native menu bar (`menus`), and a whole app from a script and an
//! optional `zgx.json` beside it (`run`), as the `zgx-run` binary does.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zgx::input::{Key, Modifiers};
use zgx_desktop::{AppMenu, AppMenuEntry, Application, MenuAccelerator, WindowOptions};
use zgx_script::{Command, ScriptHost};

/// Menus in the order the script first names them, items in its order.
pub fn menus(commands: &[Command]) -> Vec<AppMenu> {
    let mut menus: Vec<AppMenu> = Vec::new();
    for command in commands {
        let index = match menus.iter().position(|m| m.label == command.menu) {
            Some(index) => index,
            None => {
                menus.push(AppMenu::new(command.menu.clone(), []));
                menus.len() - 1
            }
        };
        let entry = if command.separator {
            AppMenuEntry::Separator
        } else {
            let item = AppMenuEntry::item(command.title.clone(), command.id.as_str());
            match command.key.as_deref().and_then(accelerator) {
                Some(key) => item.accelerator(key),
                None => item,
            }
        };
        menus[index].entries.push(entry);
    }
    menus
}

/// `cmd+shift+d`, `cmd+alt+arrowleft`, `cmd+1`, `cmd+[` as an accelerator.
/// `cmd` is the platform's command key, as menus' shortcuts use it: Command
/// on macOS, Control elsewhere (`super` and `meta` name the key itself).
pub fn accelerator(text: &str) -> Option<MenuAccelerator> {
    let mut modifiers = Modifiers::default();
    let mut parts: Vec<&str> = text.split('+').collect();
    // `cmd++` would be a plus key; none of ours use it.
    let key = parts.pop()?.to_ascii_lowercase();
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" if cfg!(target_os = "macos") => modifiers.meta = true,
            "cmd" | "command" => modifiers.control = true,
            "super" | "meta" => modifiers.meta = true,
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "opt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            other => {
                eprintln!("menus: unknown modifier `{other}` in `{text}`");
                return None;
            }
        }
    }
    let key = match key.as_str() {
        "arrowleft" | "left" => Key::ArrowLeft,
        "arrowright" | "right" => Key::ArrowRight,
        "arrowup" | "up" => Key::ArrowUp,
        "arrowdown" | "down" => Key::ArrowDown,
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "space" => Key::Space,
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            Key::Function(f[1..].parse().ok()?)
        }
        single if single.chars().count() == 1 => Key::Character(single.into()),
        other => {
            eprintln!("menus: unknown key `{other}` in `{text}`");
            return None;
        }
    };
    Some(MenuAccelerator::new(key, modifiers))
}

/// The window and app a script runs in (`zgx.json` beside the script):
/// every field optional.
///
/// ```json
/// { "title": "Mail", "width": 1100, "height": 720, "min_width": 640,
///   "min_height": 400, "titlebar": "inset", "glass": true, "app_id": "mail" }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct AppConfig {
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub min_size: Option<(f64, f64)>,
    /// `"standard"`, `"inset"` (content under a transparent title bar) or
    /// `"hidden"`.
    pub titlebar: zgx_desktop::Titlebar,
    pub glass: bool,
    /// Where `ui.persist` keeps its data (the app's data directory).
    pub app_id: String,
    /// A reverse-DNS application ID (`"dev.zgx.code"`): the system then
    /// hands the app the files and URLs it opens with it (`ui.on_open`),
    /// and on Windows a second launch's go to the first.
    pub application_id: Option<String>,
}
impl AppConfig {
    /// The config for the script at `script`: its directory's `zgx.json`
    /// (or `zgui.json`, deprecated), or the defaults (named after the directory). rename-to-zgx: keep
    pub fn for_script(script: &Path) -> Result<Self, String> {
        let dir = script.parent().unwrap_or(Path::new("."));
        let name = dir
            .canonicalize()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "app".into());
        let mut config = AppConfig {
            title: name.clone(),
            width: 960.,
            height: 640.,
            min_size: None,
            titlebar: zgx_desktop::Titlebar::Standard,
            glass: false,
            app_id: name,
            application_id: None,
        };
        // Or, for one release, the name it had before the rename to zgx.
        let old = dir.join("zgui.json"); // rename-to-zgx: keep
        let mut path = dir.join("zgx.json");
        if !path.exists() && old.exists() {
            eprintln!("zgx: {} is deprecated; name it zgx.json", old.display());
            path = old;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(config);
        };
        let fail = |e: String| format!("{}: {e}", path.display());
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
        let object = json
            .as_object()
            .ok_or_else(|| fail("not an object".into()))?;
        let number = |key: &str| -> Result<Option<f64>, String> {
            match object.get(key) {
                None => Ok(None),
                Some(v) => v
                    .as_f64()
                    .map(Some)
                    .ok_or_else(|| fail(format!("`{key}` is not a number"))),
            }
        };
        let string = |key: &str| -> Result<Option<String>, String> {
            match object.get(key) {
                None => Ok(None),
                Some(v) => v
                    .as_str()
                    .map(|s| Some(s.to_owned()))
                    .ok_or_else(|| fail(format!("`{key}` is not a string"))),
            }
        };
        for key in object.keys() {
            if ![
                "title",
                "width",
                "height",
                "min_width",
                "min_height",
                "titlebar",
                "glass",
                "app_id",
                "application_id",
            ]
            .contains(&key.as_str())
            {
                return Err(fail(format!("unknown key `{key}`")));
            }
        }
        if let Some(title) = string("title")? {
            config.title = title;
        }
        config.width = number("width")?.unwrap_or(config.width);
        config.height = number("height")?.unwrap_or(config.height);
        if let (Some(w), Some(h)) = (number("min_width")?, number("min_height")?) {
            config.min_size = Some((w, h));
        } else if number("min_width")?.is_some() || number("min_height")?.is_some() {
            return Err(fail("`min_width` and `min_height` go together".into()));
        }
        config.titlebar = match string("titlebar")?.as_deref() {
            None | Some("standard") => zgx_desktop::Titlebar::Standard,
            Some("inset") => zgx_desktop::Titlebar::Inset,
            Some("hidden") => zgx_desktop::Titlebar::Hidden,
            Some(other) => {
                return Err(fail(format!(
                    "`titlebar` is `{other}`: one of standard, inset, hidden"
                )));
            }
        };
        config.glass = match object.get("glass") {
            None => false,
            Some(v) => v
                .as_bool()
                .ok_or_else(|| fail("`glass` is not a boolean".into()))?,
        };
        if let Some(id) = string("app_id")? {
            config.app_id = id;
        }
        config.application_id = string("application_id")?;
        Ok(config)
    }
}

/// The script `path` names: a file, or a directory's `app.luau`.
pub fn script_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("app.luau")
    } else {
        path.to_path_buf()
    }
}

/// Run the script at `path` (a file, or a directory with `app.luau`) as an
/// app, until its window closes: a window from its `zgx.json`, its
/// commands as the menu bar, hot reload as its files change, and
/// `ui.persist` saved as it quits.
pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    run_with(path, Vec::new())
}

/// The local path a `file://` URL names (`None` for another URL).
fn file_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::with_capacity(rest.len());
    let mut input = rest.bytes();
    while let Some(b) = input.next() {
        if b == b'%' {
            let hex = |c: u8| (c as char).to_digit(16);
            let (a, b) = (hex(input.next()?)?, hex(input.next()?)?);
            bytes.push((a * 16 + b) as u8);
        } else {
            bytes.push(b);
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    // `file:///C:/a.txt` on Windows.
    let path = match path.as_bytes() {
        [b'/', drive, b':', ..] if cfg!(windows) && drive.is_ascii_alphabetic() => &path[1..],
        _ => &path,
    };
    Some(PathBuf::from(path))
}

/// As [`run`], with `args` after the script: the script's `ui.args`, and
/// those that name files opened with it (its `ui.on_open`), as a file
/// opened with an app is.
pub fn run_with(path: &Path, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    run_hosting(path, args, |_| {})
}

/// As [`run_with`], `host` given the script's host before it loads: an app
/// that serves its script (a fetcher, actions, shared state) keeps
/// everything `run` gives it, the menu bar of its commands included.
pub fn run_hosting(
    path: &Path,
    args: Vec<String>,
    host: impl Fn(&ScriptHost) + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::{cell::RefCell, rc::Rc};
    let configure = host;
    let script = script_path(path);
    if !script.is_file() {
        return Err(format!("{}: no such script", script.display()).into());
    }
    let config = AppConfig::for_script(&script)?;
    let options = WindowOptions {
        title: config.title.clone(),
        width: config.width,
        height: config.height,
        min_size: config.min_size,
        titlebar: config.titlebar,
        glass: config.glass.then(zgx_desktop::Glass::default),
        ..Default::default()
    };
    let launched: Vec<PathBuf> = args
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect();
    // What the system opens goes to the host once there is one (before,
    // it waits).
    type Opened = (Vec<PathBuf>, Vec<String>);
    let host_slot: Rc<RefCell<Option<ScriptHost>>> = Rc::default();
    let early: Rc<RefCell<Vec<Opened>>> = Rc::default();
    // Its own app on the Linux desktop, not every script one "zgx-run": its
    // windows' Wayland app_id and X11 class, matched to `app_id.desktop`.
    let mut app = Application::new()
        .window(options)
        .desktop_id(config.app_id.clone());
    if let Some(id) = &config.application_id {
        let (slot, early) = (host_slot.clone(), early.clone());
        app = app
            .application_id(id.clone())?
            .on_application_event(move |event, _| {
                let zgx_desktop::ApplicationEvent::OpenUrls(urls) = event else {
                    return;
                };
                let (files, urls): (Vec<_>, Vec<_>) =
                    urls.into_iter().partition(|url| file_path(url).is_some());
                let files = files.iter().filter_map(|url| file_path(url)).collect();
                match &*slot.borrow() {
                    Some(host) => host.open(files, urls),
                    None => early.borrow_mut().push((files, urls)),
                }
            });
    }
    app.run(move |cx| {
        let host = ScriptHost::new(&cx.ui.runtime, script.clone())
            .app_id(&config.app_id)
            .args(args.clone());
        // The launch's files, and what the system opened meanwhile:
        // they wait for the script's `ui.on_open`.
        host.open(launched.clone(), Vec::new());
        for (files, urls) in early.borrow_mut().drain(..) {
            host.open(files, urls);
        }
        *host_slot.borrow_mut() = Some(host.clone());
        // `ui.window.titlebar_area`: where the title bar leaves the script room.
        host.window_chrome(
            cx.window_info.titlebar_area.clone(),
            cx.window_info.controls.clone(),
        );
        configure(&host);
        let _ = host.reload();
        cx.tasks.spawn(host.watch(Duration::from_millis(250)));
        let menus_follow = serve_commands(&host, cx);
        // The script's windows (`ui.window`): each open while its state is.
        let windows_follow = follow_windows(&host, cx);
        let saving = host.clone();
        let devtools_asked = inspect_asked();
        #[cfg(feature = "inspect")]
        if devtools_asked {
            zgx_script::inspect::prepare();
        }
        let handle = cx.render(host.view());
        let devtools = devtools_asked
            .then(|| inspect(&host, cx, handle, &script))
            .flatten();
        let keep = std::cell::RefCell::new(Some((menus_follow, windows_follow, devtools)));
        cx.on_closed(move || {
            saving.save();
            drop(keep.borrow_mut().take());
        });
    })?;
    Ok(())
}

static INSPECT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Serve devtools (`zgx inspect`) from the next window `run` opens, as
/// `zgx run --inspect` asks (`ZGX_INSPECT_SOCKET=1` asks too). Built
/// without the `inspect` feature, it says so and does nothing.
pub fn set_inspect(on: bool) {
    INSPECT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Devtools for the window, if asked: the socket that serves them, alive
/// while what this returns is.
/// Whether devtools are asked for: `--inspect`, or `ZGX_INSPECT_SOCKET=1`.
fn inspect_asked() -> bool {
    #[cfg(feature = "inspect")]
    let env = zgx_script::inspect::asked();
    #[cfg(not(feature = "inspect"))]
    let env = false;
    INSPECT.load(std::sync::atomic::Ordering::Relaxed) || env
}

#[cfg(feature = "inspect")]
fn inspect(
    host: &ScriptHost,
    cx: &mut zgx_desktop::WindowContext,
    handle: zgx::compose::ViewHandle,
    script: &Path,
) -> Option<zgx_script::inspect::Server> {
    let session =
        zgx_script::inspect::Session::new(&cx.ui, handle, host, Some(cx.inspector.clone()));
    let tasks = cx.tasks.clone();
    match zgx_script::inspect::serve(session, &script.display().to_string(), move |task| {
        tasks.spawn(task)
    }) {
        Ok(server) => {
            eprintln!(
                "zgx run: devtools at {} (zgx inspect help)",
                server.path().display()
            );
            Some(server)
        }
        Err(error) => {
            eprintln!("zgx run: devtools: {error}");
            None
        }
    }
}
#[cfg(not(feature = "inspect"))]
fn inspect(
    _: &ScriptHost,
    _: &mut zgx_desktop::WindowContext,
    _: zgx::compose::ViewHandle,
    _: &Path,
) -> Option<()> {
    eprintln!("zgx run: built without devtools (the `inspect` feature)");
    None
}

/// `host`'s commands (`ui.command`) as the application's menu bar, kept
/// current as a reload changes them, each item running its command: their
/// shortcuts work wherever focus is, as `run` gives them. For a Rust host
/// that makes its own window; keep the effect it returns while the window
/// lives.
pub fn serve_commands(
    host: &ScriptHost,
    cx: &mut zgx_desktop::WindowContext,
) -> zgx::reactive::Effect {
    let commands = host.commands();
    let windows = cx.windows.clone();
    let follow = cx.ui.runtime.effect(move || {
        if let Err(error) = windows.set_menus(menus(&commands.get())) {
            eprintln!("zgx run: menus: {error}");
        }
    });
    let run = host.clone();
    cx.on_menu_action(move |action| {
        run.run_command(&action.0);
    });
    follow
}

/// Open and close `host`'s windows (`ui.window`) as their `open` states
/// say, and set a state false when its window is closed. The effect it
/// returns keeps this going.
pub fn follow_windows(host: &ScriptHost, cx: &zgx_desktop::WindowContext) -> zgx::reactive::Effect {
    use std::{cell::RefCell, collections::HashMap, rc::Rc};
    use zgx_script::Value;
    let factory = cx.windows.clone();
    let runtime = cx.ui.runtime.clone();
    let open: Rc<RefCell<HashMap<String, zgx_desktop::WindowHandle>>> = Rc::default();
    let (list, host) = (host.windows(), host.clone());
    cx.ui.runtime.effect(move || {
        for window in list.get() {
            let wanted = window.open.get().truthy();
            let shown = open
                .borrow()
                .get(&window.id)
                .is_some_and(|handle| !handle.is_closed());
            if wanted && !shown {
                let options = WindowOptions {
                    title: window.title.clone(),
                    width: window.width,
                    height: window.height,
                    frame_autosave: window.frame_autosave.clone(),
                    tabbing: match window.tabbing.as_deref() {
                        Some("preferred") => zgx_desktop::Tabbing::Preferred,
                        Some("disallowed") => zgx_desktop::Tabbing::Disallowed,
                        _ => zgx_desktop::Tabbing::Automatic,
                    },
                    ..Default::default()
                };
                let (view_host, state, id) = (host.clone(), window.open.clone(), window.id.clone());
                let handle = factory.open(options, move |wcx| {
                    wcx.on_closed(move || {
                        if state.with_untracked(Value::truthy) {
                            state.set(Value::Bool(false));
                        }
                    });
                    wcx.render(view_host.window_view(&id));
                });
                open.borrow_mut().insert(window.id.clone(), handle);
            } else if !wanted
                && shown
                && let Some(handle) = open.borrow_mut().remove(&window.id)
            {
                runtime.untracked(|| handle.close());
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_zgx_json_configures_the_window_and_rejects_what_it_does_not_know() {
        let dir = std::env::temp_dir().join(format!("zgx-run-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("app.luau");
        std::fs::write(&script, "return ui.text 'x'").unwrap();
        let config = AppConfig::for_script(&script).unwrap();
        assert_eq!(config.width, 960.);
        std::fs::write(
            dir.join("zgx.json"),
            r#"{ "title": "Mail", "width": 1100, "min_width": 600, "min_height": 400, "titlebar": "inset", "app_id": "mail" }"#,
        )
        .unwrap();
        let config = AppConfig::for_script(&script).unwrap();
        assert_eq!(
            (config.title.as_str(), config.width, config.min_size),
            ("Mail", 1100., Some((600., 400.)))
        );
        assert_eq!(config.titlebar, zgx_desktop::Titlebar::Inset);
        std::fs::write(dir.join("zgx.json"), r#"{ "widht": 3 }"#).unwrap();
        let error = AppConfig::for_script(&script).unwrap_err();
        assert!(error.contains("unknown key `widht`"), "{error}");
        assert!(accelerator("cmd+shift+d").is_some());
        // Cmd+N is Ctrl+N off macOS: there, Win+N is Windows' own.
        let new = accelerator("cmd+n").unwrap().0;
        assert_eq!(new.modifiers.meta, cfg!(target_os = "macos"));
        assert_eq!(new.modifiers.control, !cfg!(target_os = "macos"));
        assert!(accelerator("super+n").unwrap().0.modifiers.meta);
        std::fs::write(
            dir.join("zgx.json"),
            r#"{ "app_id": "code", "application_id": "dev.zgx.code" }"#,
        )
        .unwrap();
        let config = AppConfig::for_script(&script).unwrap();
        assert_eq!(config.application_id.as_deref(), Some("dev.zgx.code"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_config_under_its_name_from_before_the_rename_still_configures() {
        let dir = std::env::temp_dir().join(format!("zgx-run-old-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("app.luau");
        std::fs::write(&script, "return ui.text 'x'").unwrap();
        let old = dir.join("zgui.json"); // rename-to-zgx: keep
        std::fs::write(&old, r#"{ "title": "Old" }"#).unwrap();
        assert_eq!(AppConfig::for_script(&script).unwrap().title, "Old");
        std::fs::write(dir.join("zgx.json"), r#"{ "title": "New" }"#).unwrap();
        assert_eq!(AppConfig::for_script(&script).unwrap().title, "New");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn file_urls_the_system_opens_are_paths_and_others_are_not() {
        assert_eq!(
            file_path("file:///tmp/a%20b.rs"),
            Some(PathBuf::from("/tmp/a b.rs"))
        );
        assert_eq!(
            file_path("file://localhost/tmp/x"),
            Some(PathBuf::from("/tmp/x"))
        );
        assert_eq!(file_path("https://example.org/a"), None);
        assert_eq!(file_path("file:///bad%zz"), None);
        if cfg!(windows) {
            assert_eq!(
                file_path("file:///C:/a%20b.txt"),
                Some(PathBuf::from("C:/a b.txt"))
            );
        }
    }
}
