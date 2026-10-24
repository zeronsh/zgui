//! What scripts ask of their application: commands for its menus, host
//! actions called directly, terminal sessions that outlive reloads, and
//! drag and drop between their nodes.
mod common;

use common::Harness;
use zgx::{
    input::{InputEvent, PointerButton},
    native_view::NativeContent,
};
use zgx_script::{ScriptHost, Value};

#[test]
fn commands_are_published_after_a_load_and_run_by_id() {
    let mut h = Harness::inline(
        r##"local count = ui.state("count", 0)
        ui.command { id = "bump", title = "Bump", menu = "Shell", key = "cmd+b", run = function() count:add(1) end }
        ui.command { menu = "Shell", separator = true }
        ui.command { id = "reset", title = "Reset", menu = "Edit", run = function() count:set(0) end }
        return ui.text { "{count}", id = "count" }"##,
    );
    let commands = h.host.commands().get();
    let shown: Vec<_> = commands
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                c.menu.as_str(),
                c.key.as_deref(),
                c.separator,
            )
        })
        .collect();
    assert_eq!(
        shown,
        vec![
            ("bump", "Shell", Some("cmd+b"), false),
            ("", "Shell", None, true),
            ("reset", "Edit", None, false),
        ]
    );
    assert!(h.host.run_command("bump"));
    assert!(h.host.run_command("bump"));
    assert!(!h.host.run_command("missing"));
    h.settle();
    assert_eq!(h.text("count"), "2");
}

#[test]
fn a_failed_load_keeps_the_published_commands() {
    let runtime = zgx::reactive::Runtime::new();
    let dir = std::env::temp_dir().join(format!("zgx-commands-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.luau");
    std::fs::write(
        &path,
        r#"ui.command { id = "a", run = function() end }
        return ui.div {}"#,
    )
    .unwrap();
    let host = ScriptHost::new(&runtime, &path);
    host.reload().unwrap();
    std::fs::write(
        &path,
        "ui.command { id = \"b\", run = function() end }\nerror(\"broken\")",
    )
    .unwrap();
    assert!(host.reload().is_err());
    let ids: Vec<String> = host.commands().get().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, vec!["a".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scripts_invoke_host_actions() {
    let runtime = zgx::reactive::Runtime::new();
    let host = ScriptHost::inline(
        &runtime,
        "app",
        r#"ui.invoke("set_title", "Hello")
        return ui.div {}"#,
    );
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Value::Nil));
    let sink = seen.clone();
    host.action("set_title", move |value| *sink.borrow_mut() = value);
    host.reload().unwrap();
    assert_eq!(*seen.borrow(), Value::Str("Hello".into()));
    let missing = ScriptHost::inline(&runtime, "app", r#"ui.invoke("nope") return ui.div {}"#);
    assert!(
        missing
            .reload()
            .unwrap_err()
            .contains("no host action named `nope`")
    );
}

/// The terminal a mounted `ui.terminal` shows.
fn terminal(h: &mut Harness) -> zgx::terminal::Terminal {
    h.ui.prepare_frame();
    let node = h.natives.nodes()[0];
    match h.natives.content(node) {
        Some(NativeContent::Terminal(term)) => term,
        _ => panic!("no terminal mounted"),
    }
}

#[test]
fn terminal_sessions_outlive_reloads_and_end_when_closed() {
    let mut h = Harness::inline(
        r##"local term = ui.term("p1", { cwd = "/tmp", command = "zsh", env = { A = "1" } })
        return ui.column {
            ui.terminal { term = term, w = 200, h = 100 },
            ui.button { id = "close", w = 20, h = 20, on_click = function() term:close() end },
        }"##,
    );
    let first = terminal(&mut h);
    let options = first.options();
    assert_eq!(options.command.as_deref(), Some("zsh"));
    assert_eq!(
        options.working_directory.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
    assert_eq!(options.env, vec![("A".to_string(), "1".to_string())]);
    // A reload asks for "p1" again: the same session.
    h.host.reload().unwrap();
    h.settle();
    let again = terminal(&mut h);
    assert!(first == again, "a reload keeps the session");
    h.click("close");
    assert!(first.is_closed());
}

#[test]
fn options_after_a_lookup_still_say_how_the_session_starts() {
    // A list reading every pane's state looks a session up before the code
    // owning it gives its command: the command still applies.
    let mut h = Harness::inline(
        r##"local first = ui.term("p1")
        local _ = first.title
        local term = ui.term("p1", { command = "attach p1" })
        return ui.terminal { term = term, w = 200, h = 100 }"##,
    );
    let term = terminal(&mut h);
    assert_eq!(term.options().command.as_deref(), Some("attach p1"));
    // Once started, a terminal keeps how it started.
    term.attach(std::rc::Rc::new(NoBackend));
    assert!(!term.set_options(zgx::terminal::TerminalOptions::default()));
    assert_eq!(term.options().command.as_deref(), Some("attach p1"));
}
#[test]
fn reading_a_terminal_field_follows_that_field_alone() {
    // A list of panes reads titles; a resize (columns, rows) must not rerun it.
    let mut h = Harness::inline(
        r##"local term = ui.term("p1", { command = "zsh" })
        local runs = ui.state("runs", 0)
        ui.effect(function()
            local _ = term.title
            runs:set(runs:peek() + 1)
        end)
        return ui.column {
            ui.terminal { term = term, w = 200, h = 100 },
            ui.text { id = "runs", function() return tostring(runs:get()) end },
        }"##,
    );
    let term = terminal(&mut h);
    h.settle();
    let before = h.text("runs");
    term.update_state(|state| {
        state.columns = 120;
        state.rows = 40;
    });
    h.settle();
    assert_eq!(h.text("runs"), before, "a resize reran a title reader");
    term.update_state(|state| state.title = "vim".into());
    h.settle();
    assert_ne!(h.text("runs"), before, "a title change reruns it");
}
struct NoBackend;
impl zgx::terminal::TerminalBackend for NoBackend {
    fn run(&self, _: zgx::terminal::TerminalCommand) {}
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn terminal_handlers_follow_the_latest_run() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local term = ui.term("p1", { on_title = function(t) log:set(log:peek() .. t .. ";") end })
        -- A lookup without options keeps them.
        local same = ui.term("p1")
        return ui.column {
            ui.terminal { term = same, w = 200, h = 100 },
            ui.text { "{log}", id = "log" },
            ui.text { function() return term.title end, id = "title" },
        }"##,
    );
    let term = terminal(&mut h);
    term.update_state(|s| s.title = "vim".into());
    term.emit(zgx::terminal::TerminalEvent::TitleChanged {
        title: "vim".into(),
    });
    h.settle();
    assert_eq!(h.text("log"), "vim;");
    assert_eq!(h.text("title"), "vim");
}

#[test]
fn nodes_drag_and_drop_script_values() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local function note(s) log:set(log:peek() .. s .. ";") end
        return ui.column {
            ui.div { id = "src", w = 100, h = 40,
                on_drag = function() return { kind = "pane", id = "p7" } end,
                on_drag_end = function(p, accepted) note("end " .. p.id .. " " .. tostring(accepted)) end },
            ui.div { id = "dst", w = 100, h = 100,
                drop_when = function(p) return p.kind == "pane" end,
                on_drag_over = function(p, e) note("over " .. math.floor(e.local_y) .. "/" .. e.h) end,
                on_drop = function(p, e) note("drop " .. p.id) end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    let bounds = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    let (src, dst) = (bounds(&mut h, "src"), bounds(&mut h, "dst"));
    h.ui.dispatch(InputEvent::PointerDown {
        x: src.x + 10.,
        y: src.y + 10.,
        button: PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerMove {
        x: src.x + 30.,
        y: src.y + 20.,
    });
    h.ui.dispatch(InputEvent::PointerMove {
        x: dst.x + 50.,
        y: dst.y + 25.,
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x: dst.x + 50.,
        y: dst.y + 25.,
        button: PointerButton::Primary,
    });
    h.settle();
    let log = h.text("log");
    assert!(log.contains("over 25/100;"), "{log}");
    assert!(log.contains("drop p7;"), "{log}");
    assert!(log.ends_with("end p7 true;"), "{log}");
}

/// A tab's drag in a script: `drag_moves_window` says it moves its window,
/// `drag_export` what it carries to other apps, and `on_drag_outside` hears
/// it torn off, with where it was held.
#[test]
fn a_scripts_drag_may_move_its_window_or_tear_off() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        return ui.column {
            ui.div { id = "tab", w = 100, h = 40,
                on_drag = function() return { id = "t1", alone = true } end,
                drag_moves_window = function(p) return p.alone end,
                drag_export = function(p) return { text = p.id } end,
                on_drag_outside = function(p, e)
                    log:set(log:peek() .. "outside " .. p.id .. " " .. math.floor(e.grab_x) .. ";")
                end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    let tab = {
        let node = h.find("tab").unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    h.ui.dispatch(InputEvent::PointerDown {
        x: tab.x + 10.,
        y: tab.y + 10.,
        button: PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerMove {
        x: tab.x + 30.,
        y: tab.y + 20.,
    });
    let drag = h.ui.input.drag_state().expect("dragging");
    assert!(drag.moves_window);
    assert_eq!(
        drag.export.as_deref(),
        Some(&[zgx::input::DragItem::Text("t1".into())][..])
    );
    // Released outside every window (as a host says).
    h.ui.input.end_drag_elsewhere(&h.ui.scene, false, true);
    h.settle();
    let log = h.text("log");
    assert_eq!(log, "outside t1 10;");
}

#[test]
fn a_press_that_follows_the_pointer_keeps_it() {
    let mut h = Harness::inline(
        r##"local x = ui.state("x", 0)
        local dragging = false
        return ui.row {
            ui.div { id = "divider", w = 6, h = 100,
                on_press = function() dragging = true end,
                on_pointer_move = function(e) if dragging then x:set(math.floor(e.x)) end end,
                on_release = function() dragging = false end },
            ui.div { w = 200, h = 100 },
            ui.text { "{x}", id = "x" },
        }"##,
    );
    h.ui.prepare_frame();
    let node = h.find("divider").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    h.ui.dispatch(InputEvent::PointerDown {
        x: b.x + 3.,
        y: b.y + 50.,
        button: PointerButton::Primary,
    });
    // Far outside the divider: it still hears the pointer.
    h.ui.dispatch(InputEvent::PointerMove { x: 150., y: 60. });
    h.ui.dispatch(InputEvent::PointerUp {
        x: 150.,
        y: 60.,
        button: PointerButton::Primary,
    });
    h.settle();
    assert_eq!(h.text("x"), "150");
}

#[test]
fn any_container_without_an_id_takes_drops_and_drags() {
    for area in ["ui.div", "ui.overlay", "ui.column", "ui.row"] {
        let code = format!(
            r##"local log = ui.state("log", "")
            return ui.column {{
                {area} {{ w = 40, h = 40, on_drag = function() return {{ k = 1 }} end, ui.div {{ id = "src", w = 40, h = 40 }} }},
                {area} {{ w = 200, h = 200,
                    on_drag_over = function(p, e) log:set("over " .. math.floor(e.local_x)) end,
                    on_drop = function(p, e) log:set(log:peek() .. " drop " .. p.k) end,
                    ui.div {{ id = "child", w = 100, h = 100 }},
                }},
                ui.text {{ "{{log}}", id = "log" }},
            }}"##
        );
        let mut h = Harness::inline(&code);
        h.ui.prepare_frame();
        let bounds = |h: &mut Harness, id: &str| {
            let node = h.find(id).unwrap();
            h.ui.scene.borrow().bounds(node)
        };
        let (src, dst) = (bounds(&mut h, "src"), bounds(&mut h, "child"));
        h.ui.dispatch(InputEvent::PointerDown {
            x: src.x + 5.,
            y: src.y + 5.,
            button: PointerButton::Primary,
        });
        h.ui.dispatch(InputEvent::PointerMove {
            x: src.x + 15.,
            y: src.y + 15.,
        });
        h.ui.dispatch(InputEvent::PointerMove {
            x: dst.x + 20.,
            y: dst.y + 20.,
        });
        h.ui.dispatch(InputEvent::PointerUp {
            x: dst.x + 20.,
            y: dst.y + 20.,
            button: PointerButton::Primary,
        });
        h.settle();
        assert_eq!(h.text("log"), "over 20 drop 1", "{area}");
    }
}

#[test]
fn context_menus_show_what_scripts_return_at_the_pointer() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        return ui.column {
            ui.div { id = "target", w = 200, h = 100,
                context_menu = function(e)
                    return {
                        ui.menu_item { "Copy", id = "copy", on_click = function() log:set("copy at " .. e.local_x) end },
                        ui.menu_item { "Paste", id = "paste", on_click = function() log:set("paste") end },
                    }
                end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    let target = h.find("target").unwrap();
    let b = h.ui.scene.borrow().bounds(target);
    let secondary = |h: &mut Harness, x: f32, y: f32| {
        h.ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Secondary,
        });
        h.ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Secondary,
        });
        h.settle();
        h.ui.prepare_frame();
    };
    secondary(&mut h, b.x + 25., b.y + 10.);
    let copy = h.find("copy").expect("the menu shows");
    let c = h.ui.scene.borrow().bounds(copy);
    assert!(
        c.x >= b.x + 25. && c.y >= b.y + 10.,
        "at the pointer: {c:?}"
    );
    h.click("copy");
    assert_eq!(h.text("log"), "copy at 25");
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn host_functions_answer_now() {
    let mut h = Harness::new(|runtime| {
        let host = zgx_script::ScriptHost::inline(
            runtime,
            "app.luau",
            r##"local row = ui.call_sync("row", 41)
            return ui.text { row.label .. " " .. tostring(row.n), id = "t" }"##,
        );
        host.function("row", |arg| {
            let n = arg.as_f64().unwrap_or(0.) + 1.;
            Value::Map(vec![("label".into(), "row".into()), ("n".into(), Value::Num(n))].into())
        });
        host
    });
    assert_eq!(h.text("t"), "row 42");
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn host_images_show_and_follow_replacement() {
    let image = |shade: u8| {
        std::sync::Arc::new(zgx::image::ImageData::new(2, 2, vec![shade; 16]).unwrap())
    };
    let mut h = Harness::inline(
        r##"return ui.column { ui.image { id = "i", src = "host:art", w = 10, h = 10 } }"##,
    );
    let shown = |h: &mut Harness| {
        h.ui.prepare_frame();
        let node = h.find("i").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
                return Some(image.pixels()[0]);
            }
            stack.extend(scene.children(n).iter().copied());
        }
        None
    };
    assert_ne!(
        shown(&mut h),
        Some(10),
        "nothing of it until the host has it"
    );
    h.host.image("art", Some(image(10)));
    h.frame();
    assert_eq!(shown(&mut h), Some(10));
    h.host.image("art", Some(image(200)));
    h.frame();
    assert_eq!(shown(&mut h), Some(200));
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn menu_items_draw_content_of_their_own() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local open = ui.state("open", true)
        return ui.column {
            ui.menu {
                open = open,
                anchor = ui.button { id = "anchor", w = 40, h = 20 },
                ui.menu_item {
                    label = "Rename",
                    id = "rename",
                    on_click = function() log:set("renamed") end,
                    ui.div { id = "icon", w = 14, h = 14 },
                    ui.text { "Rename", id = "name" },
                    ui.text { "⌘R", id = "shortcut" },
                },
            },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    assert!(
        h.find("icon").is_some() && h.find("shortcut").is_some(),
        "the item's own content"
    );
    assert_eq!(h.text("name"), "Rename");
    h.click("rename");
    assert_eq!(h.text("log"), "renamed");
    assert_eq!(h.host.error().get(), None);
}

fn page(h: &mut Harness) -> zgx::web_view::WebView {
    h.ui.prepare_frame();
    let node = h.natives.nodes()[0];
    match h.natives.content(node) {
        Some(NativeContent::Web(web)) => web,
        _ => panic!("no page mounted"),
    }
}

#[test]
fn keyed_web_pages_outlive_reloads_until_closed() {
    let mut h = Harness::inline(
        r##"local web = ui.web("b1", { profile = "ephemeral" })
        return ui.column {
            ui.webview { web = web, w = 200, h = 100 },
            ui.button { id = "close", w = 20, h = 20, on_click = function() web:close() end },
        }"##,
    );
    let first = page(&mut h);
    h.host.reload().unwrap();
    h.settle();
    assert!(first == page(&mut h), "a reload keeps the page");
    // Closed, the next run asking for "b1" gets a new one.
    h.click("close");
    h.host.reload().unwrap();
    h.settle();
    assert!(first != page(&mut h), "closing ends it");
}

#[test]
fn an_element_takes_focus_when_asked() {
    let mut h = Harness::inline(
        r##"local field = nil
        local took = ui.state("took", false)
        return ui.column {
            ui.input { id = "field", value = ui.state("v", ""), w = 100, h = 24,
                on_mount = function(el) field = el end },
            ui.button { id = "go", w = 20, h = 20, on_click = function() took:set(field:focus()) end },
            ui.text { id = "took", function() return tostring(took:get()) end },
        }"##,
    );
    h.click("go");
    h.settle();
    assert_eq!(h.text("took"), "true");
    let field = h.find("field").unwrap();
    let focused = h.ui.input.focused().unwrap();
    let scene = h.ui.scene.borrow();
    let mut node = Some(focused);
    let mut inside = false;
    while let Some(n) = node {
        inside |= n == field;
        node = scene.parent(n);
    }
    assert!(inside, "the input has focus");
}

#[test]
fn values_round_trip_through_json() {
    let value = Value::Map(
        vec![
            (
                "a".into(),
                Value::List(vec![Value::Num(1.), Value::Str("x".into())].into()),
            ),
            ("b".into(), Value::Bool(true)),
        ]
        .into(),
    );
    let json = value.to_json();
    assert_eq!(Value::from_json(&json), Some(value));
    assert_eq!(Value::from_json("not json"), None);
}
