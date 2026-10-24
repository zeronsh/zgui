//! Notes in Luau: the same app as NSH's `examples/notes`, for comparison.
//! Run: cargo run --release -p zgx-script --example notes
//! Edit `examples/notes/app.luau` and save to reload it; notes are kept.
//!
//! Rust owns what a script can't reach: the native menu bar (its commands
//! arrive in the script as `ui.shared("command")`). The script saves its
//! notes itself, with `ui.persist`.
use std::time::Duration;
use zgx::input::{Key, Modifiers};
use zgx_desktop::{
    AppMenu, AppMenuEntry, Application, Glass, MenuAccelerator, Titlebar, WindowOptions,
};
use zgx_script::{ScriptHost, Value};

const TITLE: &str = "zgx · notes";
const WIDTH: f64 = 1200.;
const HEIGHT: f64 = 800.;

fn menus() -> Vec<AppMenu> {
    let key = |c: &str| Key::Character(c.into());
    vec![
        AppMenu::new(
            "File",
            [
                AppMenuEntry::item("New Note", "new")
                    .accelerator(MenuAccelerator::primary(key("n"))),
                AppMenuEntry::item("Duplicate Note", "duplicate")
                    .accelerator(MenuAccelerator::primary(key("d"))),
                AppMenuEntry::item("Pin Note", "pin"),
                AppMenuEntry::item("Delete Note…", "delete"),
            ],
        ),
        AppMenu::new(
            "View",
            [AppMenuEntry::item("Show Preview", "preview")
                .checked(true)
                .accelerator(MenuAccelerator::new(
                    key("p"),
                    Modifiers {
                        shift: true,
                        meta: cfg!(target_os = "macos"),
                        control: !cfg!(target_os = "macos"),
                        ..Default::default()
                    },
                ))],
        ),
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/notes/app.luau");
    Application::new()
        .menus(menus())?
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((760., 480.)),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            // The script keeps its notes with `ui.persist`, in this app's
            // data directory.
            let host = ScriptHost::new(&cx.ui.runtime, script).app_id("zgx-notes");
            import_old_notes(&host);

            // The preview's state is shared, so the menu's check mark follows
            // the script's toolbar button too.
            // The note text names the platform's keys and menus.
            host.shared("platform", std::env::consts::OS);
            let preview = host.shared("preview", true);
            let windows = cx.windows.clone();
            let check = cx.ui.runtime.effect({
                let preview = preview.clone();
                move || {
                    let _ = windows.set_menu_state("preview", None, Some(preview.get().truthy()));
                }
            });

            // Other menu commands reach the script as `{ name, seq }`.
            let command = host.shared("command", Value::Nil);
            let mut seq = 0.;
            cx.on_menu_action(move |action| {
                let _keep = &check;
                if action.0 == "preview" {
                    preview.set(Value::Bool(!preview.with_untracked(Value::truthy)));
                    return;
                }
                seq += 1.;
                command.set(Value::Map(
                    [
                        ("name".into(), Value::Str(action.0.as_str().into())),
                        ("seq".into(), Value::Num(seq)),
                    ]
                    .into(),
                ));
            });

            let _ = host.reload();
            cx.tasks.spawn(host.watch(Duration::from_millis(250)));
            // Edits from the last 250 ms, before the window goes.
            let saving = host.clone();
            cx.on_closed(move || saving.save());
            // `ui.window.titlebar_area`: where the title bar leaves the script room.
            host.window_chrome(
                cx.window_info.titlebar_area.clone(),
                cx.window_info.controls.clone(),
            );
            cx.render(host.view());
        })
}

/// Notes saved before the script kept them itself (`~/.zgx-notes.json`)
/// become its saved `notes`, once: only while it has none. The old file stays.
fn import_old_notes(host: &ScriptHost) {
    let (Some(dir), Some(home)) = (host.storage_dir(), std::env::var_os("HOME")) else {
        return;
    };
    let old = std::path::Path::new(&home).join(".zgx-notes.json");
    let new = dir.join("app.luau").join("notes.json");
    if old.is_file() && !new.exists() {
        let copied =
            std::fs::create_dir_all(new.parent().unwrap()).and_then(|()| std::fs::copy(&old, &new));
        if let Err(e) = copied {
            eprintln!("notes: could not import {}: {e}", old.display());
        }
    }
}
