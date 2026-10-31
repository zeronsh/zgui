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

/// `ui.window.visible`: the app's window opened hidden, as its host says,
/// and shown as the script sets it; the host told once per change.
#[test]
fn the_window_visible_is_a_state_a_script_sets_to_show_it() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.text { id = "shown", function() return `visible {ui.window.visible:get()}` end },
            ui.button { id = "show", on_click = function() ui.window.visible:set(true) end, "Show" },
        }"##,
    );
    let told = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let told = told.clone();
        h.host
            .window_visible(false, move |visible| told.borrow_mut().push(visible));
    }
    h.frames(1);
    assert!(
        texts(&h.ui).contains(&"visible false".to_owned()),
        "{:?}",
        texts(&h.ui)
    );
    assert!(told.borrow().is_empty(), "opened hidden: nothing to show");
    h.click("show");
    h.click("show");
    assert_eq!(*told.borrow(), [true]);
}

/// A declared window made hidden (`visible = false`), or shown as a state
/// says; a `visible` that is neither is an error.
#[test]
fn a_window_may_be_made_hidden() {
    let h = Harness::inline(
        r##"ui.window { id = "intro", open = ui.state("intro", true), visible = false, ui.text("Intro") }
        ui.window { id = "later", open = ui.state("later", true),
            visible = ui.state("later_visible", false), ui.text("Later") }
        ui.window { id = "plain", open = ui.state("plain", true), ui.text("Plain") }
        return ui.text("main")"##,
    );
    let windows = h.host.windows().get();
    let visible = |id: &str| {
        let window = windows.iter().find(|w| w.id == id).unwrap();
        window
            .visible
            .as_ref()
            .map(|v| v.with_untracked(|v| v.truthy()))
    };
    assert_eq!(visible("intro"), Some(false));
    assert_eq!(visible("later"), Some(false));
    assert_eq!(visible("plain"), None);
    let error = zgx_script::testing::TestApp::try_new(|runtime| {
        zgx_script::ScriptHost::inline(
            runtime,
            "app",
            r##"ui.window { id = "w", open = ui.state("w", true), visible = "no", ui.text("w") }
            return ui.text("main")"##,
        )
    })
    .err()
    .expect("it doesn't load");
    assert!(error.contains("`visible` must be a boolean"), "{error}");
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

#[test]
fn a_window_reads_its_own_state_and_no_other() {
    let h = Harness::inline(
        r##"local items = ui.state("items", { { id = "a" }, { id = "b" } })
        local settings = ui.state("settings", true)
        ui.windows(items, function(item, win)
            return { title = item.id,
                ui.text { function() return `{item.id} {if win.active:get() then "front" else "back"} {win.size:get().width}` end } }
        end)
        ui.window { id = "settings", open = settings, function(win)
            return ui.text { function() return if win.shown:get() then "settings shown" else "settings hidden" end }
        end }
        local plain = ui.state("plain", { { id = "p" } })
        ui.windows(plain, function(item) return { ui.text { item.id } } end)
        return ui.text { "app" }"##,
    );
    h.frames(2);
    let windows = h.host.windows().get();
    let by = |title: &str| windows.iter().find(|w| w.title == title).unwrap().clone();
    let (a, b, settings) = (by("a"), by("b"), by("settings"));
    assert!(
        windows
            .iter()
            .find(|w| w.id.ends_with(":p"))
            .expect("p")
            .state
            .is_none(),
        "a build that takes no `win` makes none"
    );
    let view = |id: &str| {
        let mut other = Ui::with_runtime(h.ui.runtime.clone(), 300., 200.);
        other.mount(h.host.window_view(id));
        other.prepare_frame();
        other
    };
    let (va, vb, vs) = (view(&a.id), view(&b.id), view(&settings.id));
    assert!(texts(&va).contains(&"a back 0".to_owned()));
    let state = a.state.clone().expect("its state");
    state.active.set(zgx_script::Value::Bool(true));
    state.size.set(zgx_script::window_size_value(320., 200.));
    h.frames(1);
    va.prepare_frame();
    vb.prepare_frame();
    assert!(texts(&va).contains(&"a front 320".to_owned()));
    assert!(
        texts(&vb).contains(&"b back 0".to_owned()),
        "b's is its own"
    );
    assert!(texts(&vs).contains(&"settings hidden".to_owned()));
    settings
        .state
        .clone()
        .unwrap()
        .shown
        .set(zgx_script::Value::Bool(true));
    vs.prepare_frame();
    assert!(texts(&vs).contains(&"settings shown".to_owned()));
}

/// A window's own state is the same across reloads: the host binds it once,
/// as the window opens, and a reload (hot, or devtools starting to record)
/// builds the window's content again with that state, not a new one no host
/// sets.
#[test]
fn a_windows_own_state_survives_a_reload() {
    let h = Harness::inline(
        r##"local items = ui.state("items", { { id = "a" } })
        local settings = ui.state("settings", true)
        ui.windows(items, function(item, win)
            return { title = item.id,
                ui.text { function() return `{item.id} {if win.active:get() then "front" else "back"}` end } }
        end)
        ui.window { id = "settings", open = settings, function(win)
            return ui.text { function() return if win.shown:get() then "settings shown" else "settings hidden" end }
        end }
        return ui.text { "app" }"##,
    );
    h.frames(2);
    let bound = |title: &str| {
        h.host
            .windows()
            .get()
            .iter()
            .find(|w| w.title == title)
            .unwrap()
            .clone()
    };
    // As a host bound them when the windows opened.
    let (a, settings) = (bound("a"), bound("settings"));
    h.host.reload().unwrap();
    h.frames(1);
    let view = |id: &str| {
        let mut other = Ui::with_runtime(h.ui.runtime.clone(), 300., 200.);
        other.mount(h.host.window_view(id));
        other.prepare_frame();
        other
    };
    let (va, vs) = (view(&a.id), view(&settings.id));
    a.state.unwrap().active.set(zgx_script::Value::Bool(true));
    settings
        .state
        .unwrap()
        .shown
        .set(zgx_script::Value::Bool(true));
    h.frames(1);
    va.prepare_frame();
    vs.prepare_frame();
    assert!(
        texts(&va).contains(&"a front".to_owned()),
        "{:?}",
        texts(&va)
    );
    assert!(
        texts(&vs).contains(&"settings shown".to_owned()),
        "{:?}",
        texts(&vs)
    );
}
