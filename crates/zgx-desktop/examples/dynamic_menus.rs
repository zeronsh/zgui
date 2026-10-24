//! Live shared menus with checked state and native/logical-key shortcuts.
use zgx::{compose::prelude::*, input::Key};
use zgx_desktop::{AppMenu, AppMenuEntry, Application, MenuAccelerator, WindowOptions};
fn observe_native_menu(label: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSMenu};
        fn visit(menu: &NSMenu, prefix: &str, label: &str) {
            for item in menu.itemArray() {
                let path = format!("{prefix}/{}", item.title());
                println!(
                    "NATIVE_MENU {label} path={path:?} state={} enabled={}",
                    item.state(),
                    item.isEnabled()
                );
                if let Some(submenu) = item.submenu() {
                    visit(&submenu, &path, label);
                }
            }
        }
        let mtm = MainThreadMarker::new().expect("menu observation runs on the UI thread");
        let menu = NSApplication::sharedApplication(mtm)
            .mainMenu()
            .expect("installed application menu");
        visit(&menu, "", label);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = label;
}
fn schedule_observation(tasks: &zgx_desktop::TaskSpawner, label: String) {
    tasks.spawn(async move {
        zgx::timer::sleep(std::time::Duration::from_millis(100)).await;
        observe_native_menu(&label);
    });
}
fn model(checked: bool) -> Vec<AppMenu> {
    vec![AppMenu::new(
        "File",
        [
            AppMenuEntry::item("Save", "save")
                .enabled(checked)
                .accelerator(MenuAccelerator::primary(Key::Character("s".into()))),
            AppMenuEntry::item("Enabled", "toggle")
                .checked(checked)
                .accelerator(MenuAccelerator::primary(Key::Character("b".into()))),
            AppMenuEntry::item("Replace menu", "replace")
                .accelerator(MenuAccelerator::primary(Key::Character("r".into()))),
        ],
    )]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observe = std::env::args().any(|arg| arg == "--native-observe");
    Application::new()
        .menus(model(false))?
        .window(WindowOptions {
            title: "zgx dynamic menus".into(),
            width: 500.,
            height: 300.,
            ..Default::default()
        })
        .run(move |cx| {
            let observation_tasks = cx.tasks.clone();
            if observe {
                schedule_observation(&observation_tasks, "initial".into());
            }
            let windows = cx.windows.clone();
            let mut checked = false;
            cx.on_menu_action(move |action| {
                println!("ACTION {}", action.0);
                match action.0.as_str() {
                    "toggle" => {
                        checked = !checked;
                        windows
                            .set_menu_state("toggle", None, Some(checked))
                            .unwrap();
                        windows.set_menu_state("save", Some(checked), None).unwrap();
                        println!("CHECKED {checked}");
                    }
                    "replace" => {
                        windows
                            .set_menus(vec![AppMenu::new(
                                "Tools",
                                [AppMenuEntry::item("New command", "new").accelerator(
                                    MenuAccelerator::primary(Key::Character("d".into())),
                                )],
                            )])
                            .unwrap();
                        println!("REPLACED");
                    }
                    _ => {}
                }
                if observe {
                    schedule_observation(&observation_tasks, format!("after-{}", action.0));
                }
            });
            let editor = cx.ui.signal(String::new());
            cx.render(
                column()
                    .p(12.)
                    .gap(12.)
                    .children((!zgx_desktop::has_native_app_menu()).then(|| cx.app_menu_bar()))
                    .child(text_input("Editor", editor).w(400.).h(40.))
                    .child("Ctrl/Cmd+B enables Save; Ctrl/Cmd+R replaces menus."),
            );
            cx.windows.open(
                WindowOptions {
                    title: "zgx dynamic menus secondary".into(),
                    width: 420.,
                    height: 220.,
                    ..Default::default()
                },
                |cx| {
                    cx.on_menu_action(|action| println!("SECOND {}", action.0));
                    cx.render(
                        column()
                            .p(12.)
                            .children(
                                (!zgx_desktop::has_native_app_menu()).then(|| cx.app_menu_bar()),
                            )
                            .child("Menu changes apply here too."),
                    );
                },
            );
            println!("READY");
        })
}
