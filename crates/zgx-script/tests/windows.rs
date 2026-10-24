//! `ui.window`: windows a script declares, open while their state is.
mod common;
use common::Harness;
use zgx::{scene::NodeKind, widgets::Ui};

fn texts(ui: &Ui) -> Vec<String> {
    let scene = ui.scene.borrow();
    scene
        .paint_items()
        .filter_map(|item| match scene.kind(item.id) {
            NodeKind::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_declared_window_shares_the_apps_state_and_follows_reloads() {
    let mut h = Harness::inline(
        r##"local open = ui.state("settings", false)
        local dark = ui.state("dark", false)
        ui.window { id = "settings", title = "Settings", width = 400, height = 300, open = open,
            ui.column { ui.text { function() return if dark:get() then "Dark" else "Light" end },
                ui.button { id = "toggle", on_click = function() dark:set(not dark:peek()) end, "Toggle" } } }
        return ui.column {
            ui.button { id = "open", on_click = ui.set(open, true), "Settings…" },
            ui.text { function() return if dark:get() then "main dark" else "main light" end, id = "main" } }"##,
    );
    h.frames(2);
    let windows = h.host.windows().get();
    assert_eq!(windows.len(), 1);
    let window = &windows[0];
    assert_eq!(
        (window.id.as_str(), window.title.as_str()),
        ("settings", "Settings")
    );
    assert_eq!((window.width, window.height), (400., 300.));
    assert!(!window.open.get().truthy());
    h.click("open");
    assert!(window.open.get().truthy(), "the button opens it");
    // The host shows its view in a window on the same runtime.
    let mut other = Ui::with_runtime(h.ui.runtime.clone(), 400., 300.);
    other.mount(h.host.window_view("settings"));
    other.prepare_frame();
    assert!(texts(&other).contains(&"Light".to_owned()));
    // Its state is the app's: its toggle changes the main window.
    let at = {
        let scene = other.scene.borrow();
        let toggle = scene
            .paint_items()
            .find(|item| matches!(scene.kind(item.id), NodeKind::Text { text, .. } if &**text == "Toggle"))
            .map(|item| scene.bounds(item.id))
            .expect("its button");
        (toggle.x + 2., toggle.y + 2.)
    };
    use zgx::input::{InputEvent, PointerButton};
    let button = PointerButton::Primary;
    other.dispatch(InputEvent::PointerDown {
        x: at.0,
        y: at.1,
        button,
    });
    other.dispatch(InputEvent::PointerUp {
        x: at.0,
        y: at.1,
        button,
    });
    h.frames(1);
    other.prepare_frame();
    assert_eq!(h.text("main"), "main dark");
    assert!(texts(&other).contains(&"Dark".to_owned()));
    // A reload keeps it showing, from the new load.
    h.host.reload().expect("reloads");
    other.prepare_frame();
    assert!(
        texts(&other).contains(&"Dark".to_owned()),
        "kept across a reload"
    );
}

#[test]
fn the_window_titlebar_area_and_controls_follow_what_the_host_says() {
    let h = Harness::inline(
        r##"return ui.text {
            id = "area",
            function()
                local area = ui.window.titlebar_area:get()
                local buttons = ui.window.controls:get()
                if not area then return "none" end
                return `{area.x} {area.width}x{area.height} buttons {if buttons then buttons.x else "none"}`
            end,
        }"##,
    );
    h.frames(1);
    assert!(texts(&h.ui).contains(&"none".to_owned()));
    let area = h.ui.signal(Some(zgx::scene::Rect::new(0., 0., 662., 32.)));
    let controls =
        h.ui.signal(Some(zgx::scene::Rect::new(662., 0., 138., 32.)));
    h.host.window_chrome(area.clone(), controls.clone());
    h.frames(1);
    assert!(texts(&h.ui).contains(&"0 662x32 buttons 662".to_owned()));
    // A resize, then fullscreen (no title bar).
    area.set(Some(zgx::scene::Rect::new(0., 0., 1062., 32.)));
    controls.set(Some(zgx::scene::Rect::new(1062., 0., 138., 32.)));
    h.frames(1);
    assert!(texts(&h.ui).contains(&"0 1062x32 buttons 1062".to_owned()));
    area.set(None);
    controls.set(None);
    h.frames(1);
    assert!(texts(&h.ui).contains(&"none".to_owned()));
}

/// A window's frame name and tabbing reach its host; a tabbing it does not
/// know is an error.
#[test]
fn a_window_says_how_it_reopens_and_tabs() {
    let h = Harness::inline(
        r##"ui.window { id = "inspector", open = ui.state("inspector", false),
            frame_autosave = "inspector", tabbing = "disallowed", ui.text("Inspector") }
        return ui.text("main")"##,
    );
    let windows = h.host.windows().get();
    assert_eq!(windows[0].frame_autosave.as_deref(), Some("inspector"));
    assert_eq!(windows[0].tabbing.as_deref(), Some("disallowed"));
}
