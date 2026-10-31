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

/// `ui.window.zoom` follows the window's zoom and sets it; `zoomable`
/// turns its keys off. Neither is made until a script reads it.
#[test]
fn the_window_zoom_is_a_state_a_script_reads_and_sets() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.text { id = "zoom", function() return `zoom {ui.window.zoom:get()}` end },
            ui.button { id = "larger", on_click = function() ui.window.zoom:set(2) end, "Larger" },
            ui.button { id = "lock", on_click = function() ui.window.zoomable:set(false) end, "Lock" },
        }"##,
    );
    h.host.window_zoom(&h.ui);
    h.frames(1);
    assert!(
        texts(&h.ui).contains(&"zoom 1".to_owned()),
        "{:?}",
        texts(&h.ui)
    );
    // The keys (or the View menu) zoom the window: the state follows.
    h.ui.set_zoom(1.25);
    h.frames(1);
    assert!(
        texts(&h.ui).contains(&"zoom 1.25".to_owned()),
        "{:?}",
        texts(&h.ui)
    );
    // The script zooms it.
    h.click("larger");
    assert_eq!(h.ui.zoom(), 2.);
    assert!(h.ui.zoomable());
    h.click("lock");
    assert!(!h.ui.zoomable());
}

/// A script that never reads `ui.window.zoom` makes no state for it.
#[test]
fn the_window_zoom_is_made_on_first_read() {
    let h = Harness::inline(
        r##"return ui.text { id = "plain", function()
            return tostring(rawget(ui.window, "zoom") == nil)
        end }"##,
    );
    h.frames(1);
    assert!(
        texts(&h.ui).contains(&"true".to_owned()),
        "{:?}",
        texts(&h.ui)
    );
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

/// `ui.window.size`: the window's size in points, followed, for decisions
/// layout can't make (a sidebar collapsing to icons below a width).
#[test]
fn the_window_size_is_followed_and_read_only() {
    let mut h = Harness::inline(
        r##"local narrow = ui.computed(function() return ui.window.size:get().width < 700 end)
        return ui.column {
            ui.text { id = "size", function()
                local size = ui.window.size:get()
                return `{size.width}x{size.height}`
            end },
            ui.text { id = "sidebar", function() return if narrow:get() then "icons" else "full" end },
            ui.button { id = "set", w = 20, h = 20, on_click = function()
                ui.window.size:set({ width = 1, height = 1 })
            end },
        }"##,
    );
    let viewport = h.ui.runtime.signal((960_f32, 720_f32));
    h.host.window_size(viewport.clone());
    h.frames(1);
    assert_eq!(h.text("size"), "960x720");
    assert_eq!(h.text("sidebar"), "full");
    viewport.set((640., 480.));
    h.frames(1);
    assert_eq!(h.text("size"), "640x480");
    assert_eq!(h.text("sidebar"), "icons");
    assert_eq!(h.error(), None);
    // The system's to say, not the script's.
    h.click("set");
    assert!(
        h.error().is_some_and(|e| e.contains("size")),
        "{:?}",
        h.error()
    );
}

#[test]
fn a_window_an_item_opens_and_closes_with_its_item() {
    let mut h = Harness::inline(
        r##"local details = ui.state("details", {} :: { { id: string, name: string } })
        local closed = ui.state("closed", "")
        local count = ui.state("count", 0)
        ui.windows(details, function(item)
            return {
                title = item.name,
                width = 300,
                height = 200,
                under_pointer = { 12, 8 },
                ui.column { ui.text { item.name }, ui.text { function() return `count {count:get()}` end } },
            }
        end, {
            on_close = function(item)
                closed:set(item.id)
                details:update(function(list)
                    local kept = {}
                    for _, d in list do if d.id ~= item.id then table.insert(kept, d) end end
                    return kept
                end)
            end,
        })
        return ui.column {
            ui.button { id = "a", on_click = function() details:push({ id = "a", name = "Apple" }) end, "a" },
            ui.button { id = "b", on_click = function() details:push({ id = "b", name = "Berry" }) end, "b" },
            ui.button { id = "more", on_click = function() count:add(1) end, "more" },
            ui.text { id = "closed", "{closed}" },
        }"##,
    );
    h.frames(2);
    assert!(h.host.windows().get().is_empty());
    h.click("a");
    h.click("b");
    h.frames(1);
    let windows = h.host.windows().get();
    assert_eq!(windows.len(), 2);
    assert_eq!(
        windows.iter().map(|w| w.title.as_str()).collect::<Vec<_>>(),
        ["Apple", "Berry"]
    );
    assert_eq!((windows[0].width, windows[0].height), (300., 200.));
    assert_eq!(windows[0].under_pointer, Some((12., 8.)));
    assert!(windows[0].open.get().truthy(), "open as its item appears");
    // Its content follows the app's state.
    let mut other = Ui::with_runtime(h.ui.runtime.clone(), 300., 200.);
    other.mount(h.host.window_view(&windows[0].id));
    other.prepare_frame();
    assert!(texts(&other).contains(&"Apple".to_owned()));
    h.click("more");
    other.prepare_frame();
    assert!(texts(&other).contains(&"count 1".to_owned()));
    // The person closes Apple's: the app hears and lets the item go.
    windows[0].open.set(zgx_script::Value::Bool(false));
    h.frames(1);
    assert_eq!(h.text("closed"), "a");
    let left = h.host.windows().get();
    assert_eq!(
        left.iter().map(|w| w.title.as_str()).collect::<Vec<_>>(),
        ["Berry"]
    );
    // A reload keeps the windows its list still has.
    let berry = left[0].id.clone();
    h.host.reload().expect("reloads");
    h.frames(1);
    let after = h.host.windows().get();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, berry);
}

#[test]
fn an_item_windows_build_must_give_a_window() {
    let h = Harness::inline(
        r##"local items = ui.state("items", { { id = "x" } })
        ui.windows(items, function(item) return { title = "x", colour = "red", ui.text { "x" } } end)
        return ui.text { "app" }"##,
    );
    h.frames(1);
    let error = h.host.error().get().unwrap_or_default();
    assert!(error.contains("unknown option `colour`"), "{error}");
}
