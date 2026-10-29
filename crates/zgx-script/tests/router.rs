//! `ui.router`: named routes with params, history, the platform's back and
//! forward input (keys, the mouse's buttons), deep links by scheme, and a
//! modal route through an overlay's controlled `open` with `on_close`.
mod common;
use common::Harness;
use std::{cell::RefCell, rc::Rc};
use zgx::input::{InputEvent, PointerButton};
use zgx_script::ScriptHost;

const APP: &str = r##"
local router = ui.router {
    routes = { home = "/", thread = "/thread/:id", settings = "/settings" },
    scheme = "mail",
}
return ui.column {
    ui.text { id = "where", function()
        local r = router.route:get()
        return `{r.name} {r.path} {r.params.id or "-"}`
    end },
    ui.text { id = "can", function()
        return `{tostring(router.can_back:get())} {tostring(router.can_forward:get())}`
    end },
    ui.button { id = "open-7", w = 40, h = 20, "7", on_click = function() router.navigate("thread", { id = 7 }) end },
    ui.button { id = "open-9", w = 40, h = 20, "9", on_click = function() router.open("/thread/9") end },
    ui.button { id = "settings", w = 40, h = 20, "s", on_click = function() router.navigate("settings") end },
    router.view({
        home = ui.text { id = "home-view", "Home" },
        thread = function(params) return ui.text { id = "thread-view", `Thread {params.id}` } end,
        settings = ui.text { id = "settings-view", "Settings" },
    }, { id = "view", w = 200, h = 100 }),
    ui.modal {
        open = function() return router.is("settings") end,
        on_close = router.back,
        ui.text { id = "in-modal", "In the modal" },
    },
}
"##;

fn app() -> (Harness, Rc<RefCell<Vec<String>>>) {
    let errors: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = errors.clone();
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.on_error(move |message| seen.borrow_mut().push(message.to_string()));
        host
    });
    (h, errors)
}

#[test]
fn routes_navigate_by_name_and_by_path() {
    let (mut h, _) = app();
    assert_eq!(h.error(), None);
    assert_eq!(h.text("where"), "home / -");
    assert_eq!(h.text("home-view"), "Home");
    h.click("open-7");
    assert_eq!(h.text("where"), "thread /thread/7 7");
    assert_eq!(h.text("thread-view"), "Thread 7");
    h.click("open-9");
    assert_eq!(
        h.text("thread-view"),
        "Thread 9",
        "a function case is made again for new params"
    );
}

#[test]
fn a_route_is_named_and_a_path_is_opened() {
    let mut h = Harness::inline(
        r##"local router = ui.router { routes = { home = "/", thread = "/thread/:id" } }
        local result = ui.state("result", "")
        local function try(f) local ok, err = pcall(f) result:set(if ok then "ok" else tostring(err)) end
        return ui.column {
            ui.text { id = "result", "{result}" },
            ui.button { id = "path-as-name", w = 40, h = 20, "a", on_click = function() try(function() router.navigate("/thread/3") end) end },
            ui.button { id = "missing-param", w = 40, h = 20, "b", on_click = function() try(function() router.navigate("thread") end) end },
            ui.button { id = "open-replace", w = 40, h = 20, "c", on_click = function() try(function() router.open("/thread/3", { replace = true }) end) end },
            ui.text { id = "can", function() return tostring(router.can_back:get()) end },
        }"##,
    );
    h.click("path-as-name");
    assert!(
        h.text("result").contains("no route is named"),
        "{}",
        h.text("result")
    );
    h.click("missing-param");
    assert!(
        h.text("result").contains("needs `id`"),
        "{}",
        h.text("result")
    );
    h.click("open-replace");
    assert_eq!(h.text("result"), "ok");
    assert_eq!(h.text("can"), "false", "replaced: nothing behind");
}

#[test]
fn history_goes_back_and_forward() {
    let (mut h, _) = app();
    assert_eq!(h.text("can"), "false false");
    h.click("open-7");
    h.click("open-9");
    assert_eq!(h.text("can"), "true false");
    // Its commands, which the desktop host binds to Alt+←/→ (⌘[ and ⌘] on
    // macOS) as it does every command's keys.
    assert!(h.host.run_command("router.back"));
    h.settle();
    assert_eq!(h.text("where"), "thread /thread/7 7", "the back command");
    assert_eq!(h.text("can"), "true true");
    assert!(h.host.run_command("router.forward"));
    h.settle();
    assert_eq!(h.text("where"), "thread /thread/9 9");
    // The mouse's back button over the view.
    let view = h.bounds("view");
    let (x, y) = (view.x + 20., view.y + 20.);
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Other(4),
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Other(4),
    });
    h.settle();
    assert_eq!(
        h.text("where"),
        "thread /thread/7 7",
        "the mouse's back button"
    );
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Other(5),
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Other(5),
    });
    h.settle();
    assert_eq!(
        h.text("where"),
        "thread /thread/9 9",
        "and its forward button"
    );
}

#[test]
fn deep_links_navigate_and_one_matching_nothing_is_reported_once() {
    let (mut h, errors) = app();
    h.host.open(Vec::new(), vec!["mail://thread/42".into()]);
    h.frames(1);
    assert_eq!(h.text("where"), "thread /thread/42 42");
    h.host.open(Vec::new(), vec!["mail://nowhere".into()]);
    h.host.open(Vec::new(), vec!["mail://nowhere".into()]);
    h.frames(2);
    let errors = errors.borrow();
    let reported = errors
        .iter()
        .filter(|e| e.contains("mail://nowhere"))
        .count();
    assert_eq!(reported, 1, "{errors:?}");
}

#[test]
fn a_modal_route_opens_with_its_route_and_closing_goes_back() {
    let (mut h, _) = app();
    assert!(!h.shown("in-modal"));
    h.click("settings");
    h.frames(2);
    assert!(h.shown("in-modal"), "open while the route is settings");
    h.key(zgx::input::Key::Escape);
    assert!(h.wait_until(|h| !h.shown("in-modal")), "Escape closes it");
    assert_eq!(h.text("where"), "home / -", "on_close went back");
}
