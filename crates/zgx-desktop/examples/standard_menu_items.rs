//! A standard Edit item (Copy) and an ordinary one, chosen as the rendered
//! menu bar chooses them (`WindowHandle::dispatch_menu_action`). Outside
//! macOS both reach the app's `on_menu_action` while nothing focused takes
//! the edit: `--smoke-test` chooses each and checks they arrive in order.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::{AppMenu, AppMenuEntry, Application, StandardItem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let menus = vec![AppMenu::new(
        "Edit",
        [
            AppMenuEntry::Standard(StandardItem::Copy),
            AppMenuEntry::item("Control", "control"),
        ],
    )];
    Application::new().menus(menus)?.run(move |cx| {
        cx.window.set_title("zgx standard menu items");
        let heard = Rc::new(RefCell::new(Vec::<String>::new()));
        let log = heard.clone();
        cx.on_menu_action(move |action| {
            println!("MENU {}", action.0);
            log.borrow_mut().push(action.0);
        });
        let window = cx.window.clone();
        cx.render(
            column()
                .p(12.)
                .gap(8.)
                .child("Edit → Copy and Edit → Control reach the app.")
                .child(
                    button()
                        .child("Copy")
                        .on_click(move || window.dispatch_menu_action(StandardItem::Copy.action())),
                ),
        );
        if smoke {
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                sleep(Duration::from_millis(500)).await;
                window.dispatch_menu_action(StandardItem::Copy.action());
                window.dispatch_menu_action("control");
                sleep(Duration::from_millis(500)).await;
                let heard = heard.borrow().clone();
                let expected = if cfg!(target_os = "macos") {
                    // AppKit's own item there, sent to the first responder.
                    vec!["control".to_string()]
                } else {
                    vec!["standard:copy".to_string(), "control".to_string()]
                };
                if heard == expected {
                    println!("standard menu items smoke passed: {heard:?}");
                    window.close();
                } else {
                    eprintln!(
                        "standard menu items smoke FAILED: heard {heard:?}, expected {expected:?}"
                    );
                    std::process::exit(1);
                }
            });
        }
    })?;
    Ok(())
}
