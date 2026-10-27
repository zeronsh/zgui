//! The menu bar zgx's apps show (zgx_desktop's application and Window
//! menus, built for every app), for every GPUI Fast comparison app: a real
//! macOS app has one, its replicant windows take part in each commit the
//! window server composites, and building it is part of a start. Same items,
//! same shortcuts. Included by each binary (`#[path]`).
use gpui::*;

actions!(
    menu,
    [
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        Minimize,
        Zoom,
        BringAllToFront
    ]
);
/// Install it, the application menu named `name`.
pub fn menu_bar(cx: &mut App, name: &'static str) {
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &BringAllToFront, cx| cx.activate(true));
    let in_window = |cx: &mut App, f: fn(&mut Window)| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| f(window));
        }
    };
    cx.on_action(move |_: &Minimize, cx| in_window(cx, |w| w.minimize_window()));
    cx.on_action(move |_: &Zoom, cx| in_window(cx, |w| w.zoom_window()));
    cx.bind_keys([
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);
    cx.set_menus(vec![
        Menu {
            name: name.into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Bring All to Front", BringAllToFront),
            ],
            disabled: false,
        },
    ]);
}
