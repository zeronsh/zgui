//! Native dialogs, shared application menus, and scoped UI-thread result handling.
use std::{cell::RefCell, rc::Rc};
use zgx::compose::prelude::*;
use zgx_desktop::{
    AppMenu, AppMenuEntry, Application, FileDialogOptions, FileFilter, ScopedTask,
    has_native_app_menu,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // For overwrite-sheet teardown: set ZGX_DIALOG_DIRECTORY to a directory
    // containing saved.txt, launch --close-save, then press Save before the timer.
    // This fixture selects paths only and never writes the existing file.
    // --smoke opens a file; ZGX_DIALOG_ACTION=save (or another menu action)
    // shows that dialog instead.
    let close_save = std::env::args().any(|arg| arg == "--close-save");
    let smoke = close_save || std::env::args().any(|arg| arg == "--smoke");
    let close_picker = close_save || std::env::args().any(|arg| arg == "--close-picker");
    let menus = vec![AppMenu::new(
        "File",
        [
            AppMenuEntry::item("Open file…", "open"),
            AppMenuEntry::item("Open folder…", "folder"),
            AppMenuEntry::Separator,
            AppMenuEntry::item("Save as…", "save"),
            AppMenuEntry::item("Confirm…", "prompt"),
            AppMenuEntry::item("Open URL", "url"),
            AppMenuEntry::item("Open multiple files…", "open-files"),
        ],
    )];
    Application::new().menus(menus)?.run(move |cx| {
        cx.window.set_title("zgx platform services");
        if close_picker {
            cx.windows.open(
                zgx_desktop::WindowOptions {
                    title: "zgx dialog ownership keeper".into(),
                    width: 240.,
                    height: 100.,
                    ..Default::default()
                },
                |cx| {
                    cx.render(column().child("Owner-close observer"));
                },
            );
        }
        let message = cx
            .ui
            .signal(String::from("Choose File to use a native picker."));
        let guards = Rc::new(RefCell::new(Vec::<ScopedTask>::new()));
        let dialogs = cx.dialogs.clone();
        let tasks = cx.tasks.clone();
        let update = message.clone();
        let owned = guards.clone();
        let directory = std::env::var_os("ZGX_DIALOG_DIRECTORY").map(std::path::PathBuf::from);
        cx.on_menu_action(move |action| {
            let dialogs = dialogs.clone();
            let update = update.clone();
            let kind = action.0;
            let directory = directory.clone();
            owned.borrow_mut().retain(|task| !task.is_finished());
            owned.borrow_mut().push(tasks.spawn_scoped(async move {
                if kind == "prompt" {
                    let result = dialogs
                        .prompt(
                            zgx_desktop::PromptOptions::new(
                                "Confirm native action",
                                "Proceed with the selected action?",
                            )
                            .buttons(zgx_desktop::PromptButtons::OkCancel),
                        )
                        .await;
                    println!("PROMPT {result:?}");
                    update.set(format!("Prompt: {result:?}"));
                    return;
                }
                if kind == "url" {
                    let result = zgx_desktop::open_url("zgx-smoke:accepted").await;
                    println!("URL {result:?}");
                    update.set(format!("URL: {result:?}"));
                    return;
                }
                if kind == "open-files" {
                    let result = dialogs
                        .open_files(FileDialogOptions {
                            directory,
                            ..FileDialogOptions::new("Open multiple fixtures")
                                .filter(FileFilter::new("Text", ["txt"]))
                        })
                        .await;
                    println!("DIALOG open-files {result:?}");
                    update.set(format!("open-files: {result:?}"));
                    return;
                }
                let result = match kind.as_str() {
                    "open" => {
                        dialogs
                            .open_file(
                                FileDialogOptions::new("Open fixture")
                                    .filter(FileFilter::new("Text", ["txt"])),
                            )
                            .await
                    }
                    "folder" => {
                        dialogs
                            .open_folder(FileDialogOptions {
                                directory: directory.clone(),
                                ..FileDialogOptions::new("Choose folder")
                            })
                            .await
                    }
                    "save" => {
                        dialogs
                            .save_file(FileDialogOptions {
                                directory,
                                ..FileDialogOptions::new("Save fixture").file_name("saved.txt")
                            })
                            .await
                    }
                    _ => return,
                };
                println!("DIALOG {kind} {result:?}");
                update.set(format!("{kind}: {result:?}"));
            }));
        });
        let window = cx.window.clone();
        let open = button()
            .h(36.)
            .child("Open file")
            .on_click(move || window.dispatch_menu_action("open"));
        let window = cx.window.clone();
        let open_files = button()
            .h(36.)
            .child("Open multiple files")
            .on_click(move || window.dispatch_menu_action("open-files"));
        let window = cx.window.clone();
        let folder = button()
            .h(36.)
            .child("Choose folder")
            .on_click(move || window.dispatch_menu_action("folder"));
        let window = cx.window.clone();
        let save = button()
            .h(36.)
            .child("Save as")
            .on_click(move || window.dispatch_menu_action("save"));
        let window = cx.window.clone();
        let prompt = button()
            .h(36.)
            .child("Confirm action")
            .on_click(move || window.dispatch_menu_action("prompt"));
        let window = cx.window.clone();
        let url = button()
            .h(36.)
            .child("Open URL handler")
            .on_click(move || window.dispatch_menu_action("url"));
        let mut content = column().p(20.).gap(12.);
        if !has_native_app_menu() {
            content = content.child(cx.app_menu_bar());
        }
        content = content
            .child(open)
            .child(folder)
            .child(save)
            .child(prompt)
            .child(url)
            .child(text_signal(move || message.get()))
            .child(open_files);
        cx.render(content);
        // Keep component callbacks owned until window teardown.
        cx.on_closed(move || {
            drop(guards);
            println!("CLOSED");
        });
        if smoke {
            let window = cx.window.clone();
            let action = std::env::var("ZGX_DIALOG_ACTION")
                .unwrap_or_else(|_| (if close_save { "save" } else { "open" }).into());
            cx.tasks.spawn(async move {
                zgx::timer::sleep(std::time::Duration::from_secs(1)).await;
                window.dispatch_menu_action(action.as_str());
                zgx::timer::sleep(std::time::Duration::from_secs(25)).await;
                window.close();
            });
        }
        println!("READY");
    })
}
