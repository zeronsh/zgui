//! Running a zgui Luau script as a desktop app: its commands (`ui.command`)
//! as the native menu bar (`menus`), and a whole app from a script and an
//! optional `zgui.json` beside it (`run`), as the `zgui-run` binary does.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use zgui::input::{Key, Modifiers};
use zgui_desktop::{AppMenu, AppMenuEntry, Application, MenuAccelerator, WindowOptions};
use zgui_script::{Command, ScriptHost};

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

/// The window and app a script runs in (`zgui.json` beside the script):
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
    pub titlebar: zgui_desktop::Titlebar,
    pub glass: bool,
    /// Where `ui.persist` keeps its data (the app's data directory).
    pub app_id: String,
}
impl AppConfig {
    /// The config for the script at `script`: its directory's `zgui.json`,
    /// or the defaults (named after the directory).
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
            titlebar: zgui_desktop::Titlebar::Standard,
            glass: false,
            app_id: name,
        };
        let path = dir.join("zgui.json");
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
            None | Some("standard") => zgui_desktop::Titlebar::Standard,
            Some("inset") => zgui_desktop::Titlebar::Inset,
            Some("hidden") => zgui_desktop::Titlebar::Hidden,
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
/// app, until its window closes: a window from its `zgui.json`, its
/// commands as the menu bar, hot reload as its files change, and
/// `ui.persist` saved as it quits.
pub fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
        glass: config.glass.then(zgui_desktop::Glass::default),
        ..Default::default()
    };
    // Its own app on the Linux desktop, not every script one "zgui-run": its
    // windows' Wayland app_id and X11 class, matched to `app_id.desktop`.
    Application::new()
        .window(options)
        .desktop_id(config.app_id.clone())
        .run(move |cx| {
            let host = ScriptHost::new(&cx.ui.runtime, script.clone()).app_id(&config.app_id);
            // `ui.window.titlebar_area`: where the title bar leaves the script room.
            host.window_chrome(
                cx.window_info.titlebar_area.clone(),
                cx.window_info.controls.clone(),
            );
            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            // The script's commands are the menu bar; their keys work wherever
            // focus is.
            let commands = host.commands();
            let windows = cx.windows.clone();
            let menus_follow = cx.ui.runtime.effect(move || {
                if let Err(error) = windows.set_menus(menus(&commands.get())) {
                    eprintln!("zgui run: menus: {error}");
                }
            });
            // The script's windows (`ui.window`): each open while its state is.
            let windows_follow = follow_windows(&host, cx);
            let run = host.clone();
            cx.on_menu_action(move |action| {
                run.run_command(&action.0);
            });
            let saving = host.clone();
            let keep = std::cell::RefCell::new(Some((menus_follow, windows_follow)));
            cx.on_closed(move || {
                saving.save();
                drop(keep.borrow_mut().take());
            });
            cx.render(host.view());
        })?;
    Ok(())
}

/// Open and close `host`'s windows (`ui.window`) as their `open` states
/// say, and set a state false when its window is closed. The effect it
/// returns keeps this going.
pub fn follow_windows(
    host: &ScriptHost,
    cx: &zgui_desktop::WindowContext,
) -> zgui::reactive::Effect {
    use std::{cell::RefCell, collections::HashMap, rc::Rc};
    use zgui_script::Value;
    let factory = cx.windows.clone();
    let runtime = cx.ui.runtime.clone();
    let open: Rc<RefCell<HashMap<String, zgui_desktop::WindowHandle>>> = Rc::default();
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
                        Some("preferred") => zgui_desktop::Tabbing::Preferred,
                        Some("disallowed") => zgui_desktop::Tabbing::Disallowed,
                        _ => zgui_desktop::Tabbing::Automatic,
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
    fn a_zgui_json_configures_the_window_and_rejects_what_it_does_not_know() {
        let dir = std::env::temp_dir().join(format!("zgui-run-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("app.luau");
        std::fs::write(&script, "return ui.text 'x'").unwrap();
        let config = AppConfig::for_script(&script).unwrap();
        assert_eq!(config.width, 960.);
        std::fs::write(
            dir.join("zgui.json"),
            r#"{ "title": "Mail", "width": 1100, "min_width": 600, "min_height": 400, "titlebar": "inset", "app_id": "mail" }"#,
        )
        .unwrap();
        let config = AppConfig::for_script(&script).unwrap();
        assert_eq!(
            (config.title.as_str(), config.width, config.min_size),
            ("Mail", 1100., Some((600., 400.)))
        );
        assert_eq!(config.titlebar, zgui_desktop::Titlebar::Inset);
        std::fs::write(dir.join("zgui.json"), r#"{ "widht": 3 }"#).unwrap();
        let error = AppConfig::for_script(&script).unwrap_err();
        assert!(error.contains("unknown key `widht`"), "{error}");
        assert!(accelerator("cmd+shift+d").is_some());
        // Cmd+N is Ctrl+N off macOS: there, Win+N is Windows' own.
        let new = accelerator("cmd+n").unwrap().0;
        assert_eq!(new.modifiers.meta, cfg!(target_os = "macos"));
        assert_eq!(new.modifiers.control, !cfg!(target_os = "macos"));
        assert!(accelerator("super+n").unwrap().0.modifiers.meta);
        std::fs::remove_dir_all(dir).ok();
    }
}
