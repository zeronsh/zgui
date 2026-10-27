//! macOS's menu bar, on the main thread (`fn main`), where AppKit's menus
//! are made:
//! - the Services submenu it shows is the one AppKit fills with the
//!   system's services, when the menu bar is installed and when it is
//!   replaced (the app's menus changed);
//! - a shortcut pressed before the menu bar is made (it comes after the
//!   first frame) still runs its item;
//! - each of the app's menus shows once.
fn main() {
    #[cfg(target_os = "macos")]
    {
        use std::sync::{Arc, Mutex};
        use zgx::input::Key;
        use zgx_desktop::app_menu::{
            AppMenu, AppMenuEntry, MenuAccelerator, debug_key_before_menu_bar,
            debug_menu_bar_services_filled, debug_menu_bar_titles,
        };
        for install in ["installed", "replaced"] {
            assert!(
                debug_menu_bar_services_filled(&[]).unwrap(),
                "AppKit fills another Services menu than the menu bar's ({install})"
            );
        }
        println!("menu_services: AppKit fills the menu bar's Services submenu");

        let chosen = Arc::new(Mutex::new(Vec::new()));
        muda::MenuEvent::set_event_handler(Some({
            let chosen = chosen.clone();
            move |event: muda::MenuEvent| chosen.lock().unwrap().push(event.id.0)
        }));
        let menus = [AppMenu::new(
            "File",
            [AppMenuEntry::item("Export", "export")
                .accelerator(MenuAccelerator::primary(Key::Character("e".into())))],
        )];
        assert!(
            debug_key_before_menu_bar(&menus, "e").unwrap(),
            "⌘E before the menu bar was made was not taken by it"
        );
        assert_eq!(*chosen.lock().unwrap(), ["export"]);
        assert!(
            !debug_key_before_menu_bar(&menus, "j").unwrap(),
            "a key no item has was taken"
        );
        println!("menu_services: a shortcut before the menu bar is made runs its item");

        // Each of the app's menus shows once, with the items zgx adds to an
        // Edit menu (Find) and a View menu (zoom).
        let menus = [
            AppMenu::new(
                "Edit",
                [AppMenuEntry::Standard(
                    zgx_desktop::app_menu::StandardItem::Copy,
                )],
            ),
            AppMenu::new("View", [AppMenuEntry::item("Sidebar", "sidebar")]),
        ];
        assert_eq!(
            debug_menu_bar_titles(&menus).unwrap(),
            ["Application", "Edit", "View", "Window"]
        );
        println!("menu_services: each of the app's menus shows once");
    }
}
