//! A terminal multiplexer written in Luau: tabs, split panes you can resize
//! and drag, and Ghostty terminals, in a dark native window.
//! Run: cargo run --release -p zgx-desktop --features ghostty --example terminal
//! The interface is `examples/terminal/app.luau` and the modules it
//! requires (see `docs/reference/app-structure.md`); edit and save to reload it
//! (shells keep running).
//! Devtools: add `terminal-inspect` to the features and set
//! `ZGX_INSPECT_SOCKET=1`; `zgx inspect` then sees and drives the window.
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
use zgx::terminal::TerminalColorScheme;
use zgx_desktop::{
    Application, Glass, GlassBlend, GlassGeometry, GlassInnerShadow, GlassLayer, GlassMaterial,
    GlassRadius, GlassUnderlay, Titlebar, WindowOptions,
};
use zgx_script::{Capability, ScriptHost, Value};

const TITLE: &str = "Terminal";
const WIDTH: f64 = 910.;
const HEIGHT: f64 = 522.;

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
    zgx_desktop::resolve_terminal_processes(session::resolve);
    // The interface: compiled in (`terminal-bundle`, as it ships), unless
    // `ZGX_SCRIPTS=source` asks for the source tree's; without it, the
    // source tree's, reloaded as it is edited.
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/terminal/app.luau");
    let from_source = !cfg!(feature = "terminal-bundle")
        || zgx::env::var("ZGX_SCRIPTS").as_deref() == Ok("source");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((520., 360.)),
            // Dark tinted chrome, content under inset traffic lights.
            titlebar: Titlebar::Inset,
            glass: Some(Glass::new(GlassMaterial::Hud)),
            transparent: true,
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
            let active = host.shared("window_active", false);
            let focused = cx.window_info.focused.clone();
            let follow_active = cx.ui.runtime.effect_now(move || {
                active.set(focused.get().into());
            });
            let window = cx.window.clone();
            let terminal_colors = cx.ui.signal(None::<TerminalColorScheme>);
            let appearance_host = host.clone();
            let colors = terminal_colors.clone();
            let follow_terminal_colors = cx.ui.runtime.effect(move || {
                if let Some(scheme) = colors.get() {
                    appearance_host.terminal_color_scheme(scheme);
                }
            });
            let system_colors = host.shared("system_colors", native_system_colors(None));
            host.action("set_appearance", move |request| {
                let theme = match request.field("scheme") {
                    Some(Value::Str(scheme)) if scheme.as_ref() == "dark" => {
                        Some(zgx::theme::Appearance::Dark)
                    }
                    Some(Value::Str(scheme)) if scheme.as_ref() == "light" => {
                        Some(zgx::theme::Appearance::Light)
                    }
                    _ => None,
                };
                window.set_theme(theme);
                let resolved = match request.field("resolved_scheme") {
                    Some(Value::Str(scheme)) => Some(scheme.as_ref()),
                    _ => None,
                };
                system_colors.set(native_system_colors(resolved));
                window.set_glass(appearance_glass(&request));
                terminal_colors.set(Some(appearance_terminal_colors(&request)));
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
            host.shared("terminal_font_family", "Menlo");
            host.shared("native_liquid", cfg!(target_os = "macos"));
            host.shared("native_surface_recipes", cfg!(target_os = "macos"));
            host.shared(
                "native_surface_recipe_schema",
                if cfg!(target_os = "macos") { 1.0 } else { 0.0 },
            );
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
            let (heard, mut hearing) = zgx::task::latest::<String>();
            std::thread::spawn(move || session::watch(|json| heard.send(json)));
            cx.tasks.spawn(async move {
                while let Some(json) = hearing.recv().await {
                    agents.set(Value::from_json(&json).unwrap_or(Value::Nil));
                    // The snapshot says what runs in front of every session:
                    // the terminals take it from there, and nothing polls.
                    zgx_desktop::terminal_processes_pushed(session::fronts_told());
                    zgx_desktop::terminal_processes_changed();
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
                            zgx::timer::sleep(Duration::from_millis(500)).await;
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
            let (rescan, mut rescans) = zgx::task::latest::<()>();
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
                let (answer, mut answered) = zgx::task::latest();
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
                if let Err(error) = windows.set_menus(zgx_run::menus(&list)) {
                    eprintln!("terminal: menus: {error}");
                }
            });
            let commands = host.clone();
            cx.on_menu_action(move |action| {
                commands.run_command(&action.0);
            });
            // `ui.window.titlebar_area`: where the title bar leaves the script room.
            host.window_chrome(
                cx.window_info.titlebar_area.clone(),
                cx.window_info.controls.clone(),
            );
            #[cfg(feature = "terminal-inspect")]
            let inspect_asked = zgx_script::inspect::asked();
            #[cfg(feature = "terminal-inspect")]
            if inspect_asked {
                zgx_script::inspect::prepare();
            }
            let handle = cx.render(host.view());
            #[cfg(not(feature = "terminal-inspect"))]
            let _ = handle;
            #[cfg(feature = "terminal-inspect")]
            let devtools = inspect_asked
                .then(|| {
                    let session = zgx_script::inspect::Session::new(
                        &cx.ui,
                        handle,
                        &host,
                        Some(cx.inspector.clone()),
                    );
                    let tasks = cx.tasks.clone();
                    match zgx_script::inspect::serve(
                        session,
                        &script.display().to_string(),
                        move |task| tasks.spawn(task),
                    ) {
                        Ok(server) => {
                            eprintln!("zterm: devtools at {}", server.path().display());
                            Some(server)
                        }
                        Err(error) => {
                            eprintln!("zterm: devtools: {error}");
                            None
                        }
                    }
                })
                .flatten();
            #[cfg(not(feature = "terminal-inspect"))]
            let devtools = ();
            let keep = std::cell::RefCell::new(Some((
                menus,
                follow_radius,
                follow_active,
                follow_terminal_colors,
                devtools,
            )));
            cx.on_closed(move || {
                save();
                drop(keep.borrow_mut().take());
            });
        })
}

fn appearance_terminal_colors(request: &Value) -> TerminalColorScheme {
    let color = |field| request.field(field).and_then(appearance_color);
    let ansi = match request.field("terminal_ansi") {
        Some(Value::List(colors)) if colors.len() == 16 => colors
            .iter()
            .map(appearance_color)
            .collect::<Option<Vec<_>>>()
            .and_then(|colors| colors.try_into().ok()),
        _ => None,
    };
    TerminalColorScheme {
        background: color("terminal_background"),
        foreground: color("terminal_foreground"),
        ansi,
        cursor: color("terminal_cursor"),
        cursor_text: color("terminal_cursor_text"),
        selection_background: color("terminal_selection_background"),
        selection_foreground: color("terminal_selection_foreground"),
    }
}

fn appearance_blend(value: Option<&Value>) -> GlassBlend {
    match value {
        Some(Value::Str(blend)) => match blend.as_ref() {
            "color" => GlassBlend::Color,
            "overlay" => GlassBlend::Overlay,
            "plusLighter" => GlassBlend::PlusLighter,
            "plusDarker" => GlassBlend::PlusDarker,
            _ => GlassBlend::Normal,
        },
        _ => GlassBlend::Normal,
    }
}

fn appearance_glass(request: &Value) -> Option<Glass> {
    let glass_enabled = match request.field("glass_enabled") {
        Some(Value::Bool(enabled)) => *enabled,
        _ => matches!(request.field("mode"), Some(Value::Str(mode)) if mode.as_ref() == "glass"),
    };
    let dark = matches!(request.field("resolved_scheme"), Some(Value::Str(scheme)) if scheme.as_ref() == "dark");
    let macos_palette = matches!(request.field("tint"), Some(Value::Bool(false)));
    let classic = matches!(request.field("interface_style"), Some(Value::Str(style)) if matches!(style.as_ref(), "system" | "useTheme"));
    let material = if !glass_enabled {
        GlassMaterial::None
    } else {
        match request.field("material") {
            Some(Value::Str(material)) if material.as_ref() == "liquid" => GlassMaterial::Liquid,
            Some(Value::Str(material)) if material.as_ref() == "windowBackground" => {
                GlassMaterial::WindowBackground
            }
            Some(Value::Str(material)) if material.as_ref() == "underWindowBackground" => {
                GlassMaterial::UnderWindowBackground
            }
            _ => GlassMaterial::Hud,
        }
    };
    let mut glass = Glass::new(material);
    glass.active = false;
    glass.underlay = Some(GlassUnderlay {
        material: if classic {
            GlassMaterial::WindowBackground
        } else {
            GlassMaterial::UnderWindowBackground
        },
        hidden: !(dark && macos_palette && !glass_enabled),
    });
    glass.tint = glass_enabled
        .then(|| request.field("tint_color").and_then(appearance_color))
        .flatten();
    glass.corner_radius = match request.field("corner_radius") {
        Some(Value::Num(radius)) if radius.is_finite() && *radius >= 0. => Some(*radius),
        _ => None,
    };
    let layer = |color, blend| {
        let color = request.field(color).and_then(appearance_color)?;
        let blend = appearance_blend(request.field(blend));
        Some(GlassLayer { color, blend })
    };
    glass.backdrop = layer("backdrop_color", "backdrop_blend");
    glass.fill = layer("fill_color", "fill_blend");
    if let Some(shadow) = request.field("inner_shadow")
        && let Some(color) = shadow.field("color").and_then(appearance_color)
    {
        let number = |key, default| match shadow.field(key) {
            Some(Value::Num(value)) if value.is_finite() => *value,
            _ => default,
        };
        let blend = appearance_blend(shadow.field("blend"));
        let mut geometry = GlassGeometry::default();
        if let Some(spec) = shadow.field("geometry") {
            geometry.clips_to_surface =
                matches!(spec.field("clipsToSurface"), Some(Value::Bool(true)));
            if let Some(radius) = spec.field("radius") {
                let value = match radius.field("value") {
                    Some(Value::Num(value)) if value.is_finite() => *value,
                    _ => 0.,
                };
                geometry.radius = match radius.field("mode") {
                    Some(Value::Str(mode)) if mode.as_ref() == "fixed" => {
                        GlassRadius::Fixed(value.max(0.))
                    }
                    Some(Value::Str(mode)) if mode.as_ref() == "concentric" => {
                        GlassRadius::Concentric(value)
                    }
                    _ => GlassRadius::MatchSurface,
                };
            }
            if let Some(insets) = spec.field("insets") {
                geometry.insets =
                    ["top", "trailing", "bottom", "leading"].map(|key| match insets.field(key) {
                        Some(Value::Num(value)) if value.is_finite() => *value,
                        _ => 0.,
                    });
            }
        }
        glass.inner_shadow = Some(GlassInnerShadow {
            color,
            blur: number("blur", 0.).max(0.),
            x: number("x", 0.),
            y: number("y", 0.),
            blend,
            knockout: !matches!(shadow.field("knockout"), Some(Value::Bool(false))),
            geometry,
        });
    }
    Some(glass)
}

fn appearance_color(value: &Value) -> Option<u32> {
    let Value::Str(value) = value else {
        return None;
    };
    let hex = value.strip_prefix('#')?;
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok().map(|rgb| rgb << 8 | 255),
        8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn native_system_colors(scheme: Option<&str>) -> Value {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSColor, NSColorSpace,
    };
    let values = std::cell::RefCell::new(Vec::new());
    let read = || {
        let mut values = values.borrow_mut();
        for (name, color) in [
            ("window_background", NSColor::windowBackgroundColor()),
            ("under_page_background", NSColor::underPageBackgroundColor()),
            ("label", NSColor::labelColor()),
            ("secondary_label", NSColor::secondaryLabelColor()),
            ("tertiary_label", NSColor::tertiaryLabelColor()),
        ] {
            if let Some(color) = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) {
                let c = |n: f64| (n.clamp(0., 1.) * 255.).round() as u8;
                let hex = format!(
                    "#{:02x}{:02x}{:02x}{:02x}",
                    c(color.redComponent()),
                    c(color.greenComponent()),
                    c(color.blueComponent()),
                    c(color.alphaComponent())
                );
                values.push((std::rc::Rc::from(name), Value::Str(hex.into())));
            }
        }
    };
    let appearance = match scheme {
        // SAFETY: AppKit's immutable global appearance names.
        Some("dark") => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
        Some("light") => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
        _ => None,
    };
    if let Some(appearance) = appearance {
        let block = block2::RcBlock::new(read);
        appearance.performAsCurrentDrawingAppearance(&block);
    } else {
        read();
    }
    let mut values: Vec<(std::rc::Rc<str>, Value)> = values.into_inner();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(values.into())
}
#[cfg(not(target_os = "macos"))]
fn native_system_colors(_: Option<&str>) -> Value {
    Value::Nil
}

#[cfg(test)]
mod appearance_tests {
    use super::*;

    #[test]
    fn terminal_scheme_keeps_all_scoped_colors_and_rejects_incomplete_ansi() {
        let colors = (0..16)
            .map(|index| format!("\"#{index:06x}\""))
            .collect::<Vec<_>>()
            .join(",");
        let request = Value::from_json(&format!(r##"{{"terminal_background":"#141517","terminal_foreground":"#ffffff","terminal_ansi":[{colors}],"terminal_cursor":"#123456","terminal_cursor_text":"#654321","terminal_selection_background":"#112233","terminal_selection_foreground":"#ddeeff"}}"##)).unwrap();
        let scheme = appearance_terminal_colors(&request);
        assert_eq!(scheme.background, Some(0x141517ff));
        assert_eq!(scheme.foreground, Some(0xffffffff));
        assert_eq!(
            scheme.ansi,
            Some(std::array::from_fn(|index| ((index as u32) << 8) | 255))
        );
        assert_eq!(scheme.cursor, Some(0x123456ff));
        assert_eq!(scheme.cursor_text, Some(0x654321ff));
        assert_eq!(scheme.selection_background, Some(0x112233ff));
        assert_eq!(scheme.selection_foreground, Some(0xddeeffff));
        let incomplete = Value::from_json(r##"{"terminal_ansi":["#112233"]}"##).unwrap();
        assert_eq!(appearance_terminal_colors(&incomplete).ansi, None);
        assert_eq!(
            appearance_terminal_colors(&Value::Nil),
            TerminalColorScheme::default()
        );
    }

    #[test]
    fn window_underlay_tracks_native_palette_glass_and_scheme() {
        for (style, glass_enabled, dark, tinted, material, visible) in [
            (
                "system",
                false,
                true,
                false,
                GlassMaterial::WindowBackground,
                true,
            ),
            (
                "system",
                false,
                false,
                false,
                GlassMaterial::WindowBackground,
                false,
            ),
            (
                "useTheme",
                false,
                true,
                true,
                GlassMaterial::WindowBackground,
                false,
            ),
            (
                "modern",
                true,
                true,
                false,
                GlassMaterial::UnderWindowBackground,
                false,
            ),
            (
                "modern",
                false,
                true,
                false,
                GlassMaterial::UnderWindowBackground,
                true,
            ),
        ] {
            let request = Value::from_json(&format!(r#"{{"mode":"opaque","glass_enabled":{glass_enabled},"resolved_scheme":"{}","tint":{tinted},"interface_style":"{style}"}}"#, if dark { "dark" } else { "light" })).unwrap();
            let glass = appearance_glass(&request).unwrap();
            assert!(!glass.active);
            assert_eq!(glass.material == GlassMaterial::None, !glass_enabled);
            assert_eq!(
                glass.underlay,
                Some(GlassUnderlay {
                    material,
                    hidden: !visible
                })
            );
            assert!(glass.backdrop.is_none());
        }
    }

    #[test]
    fn additive_shadow_keeps_its_own_radius_and_unclipped_geometry() {
        let request = Value::from_json(r##"{"mode":"glass","inner_shadow":{"color":"#ffffff26","blur":1,"x":0,"y":0.5,"blend":"plusLighter","knockout":true,"geometry":{"radius":{"mode":"fixed","value":16},"clipsToSurface":false,"insets":{"top":0,"trailing":0,"bottom":0,"leading":0}}}}"##).unwrap();
        let shadow = appearance_glass(&request).unwrap().inner_shadow.unwrap();
        assert_eq!(shadow.blend, GlassBlend::PlusLighter);
        assert_eq!(shadow.geometry.radius, GlassRadius::Fixed(16.));
        assert!(!shadow.geometry.clips_to_surface);
        assert_eq!(shadow.geometry.insets, [0.; 4]);
        assert_eq!(
            appearance_blend(Some(&Value::Str("plusDarker".into()))),
            GlassBlend::PlusDarker
        );
    }

    #[test]
    fn resource_layers_keep_their_alpha_and_blend_operations() {
        let request = Value::from_json(r##"{"mode":"glass","tint_color":"#14151780","backdrop_color":"#24252880","backdrop_blend":"color","fill_color":"#0000004d","fill_blend":"overlay","corner_radius":12,"inner_shadow":{"color":"#ffffff99","blur":2.5,"x":0,"y":1.5,"blend":"overlay","knockout":true}}"##).unwrap();
        let glass = appearance_glass(&request).unwrap();
        assert_eq!(glass.material, GlassMaterial::Hud);
        assert!(!glass.active);
        assert_eq!(
            glass.underlay,
            Some(GlassUnderlay {
                material: GlassMaterial::UnderWindowBackground,
                hidden: true
            })
        );
        assert_eq!(glass.tint, Some(0x14151780));
        assert_eq!(glass.corner_radius, Some(12.));
        assert_eq!(
            glass.inner_shadow,
            Some(GlassInnerShadow {
                color: 0xffffff99,
                blur: 2.5,
                x: 0.,
                y: 1.5,
                blend: GlassBlend::Overlay,
                knockout: true,
                geometry: GlassGeometry::default()
            })
        );
        assert_eq!(
            glass.backdrop,
            Some(GlassLayer {
                color: 0x24252880,
                blend: GlassBlend::Color
            })
        );
        assert_eq!(
            glass.fill,
            Some(GlassLayer {
                color: 0x0000004d,
                blend: GlassBlend::Overlay
            })
        );
        assert_eq!(
            appearance_glass(&Value::from_json(r#"{"mode":"opaque"}"#).unwrap())
                .unwrap()
                .material,
            GlassMaterial::None
        );
    }
}

/// The interface compiled in (`terminal-bundle`).
#[cfg(feature = "terminal-bundle")]
fn bundled(runtime: &zgx::reactive::Runtime) -> ScriptHost {
    static UI: zgx_script::Bundle = zgx_script::include_bundle!("terminal");
    ScriptHost::bundled(runtime, &UI, "app.luau")
}
#[cfg(not(feature = "terminal-bundle"))]
fn bundled(_: &zgx::reactive::Runtime) -> ScriptHost {
    unreachable!("built without terminal-bundle")
}
