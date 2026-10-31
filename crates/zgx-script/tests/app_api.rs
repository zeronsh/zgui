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
#[test]
fn a_script_answers_a_programs_clipboard_read() {
    // `on_clipboard_read` gets the request; a press allows it later, as a
    // prompt the app shows would.
    let mut h = Harness::inline(
        r##"local pending = nil
        local asked = ui.state("asked", 0)
        local term = ui.term("p1", {
            command = "zsh",
            on_clipboard_read = function(request)
                pending = request
                asked:set(asked:peek() + 1)
            end,
        })
        return ui.column {
            ui.terminal { term = term, w = 200, h = 100 },
            ui.text { id = "asked", "{asked}" },
            ui.button { id = "allow", w = 20, h = 20, on_click = function() pending:allow() end },
        }"##,
    );
    let term = terminal(&mut h);
    let answer = std::rc::Rc::new(std::cell::Cell::new(None));
    let told = answer.clone();
    let unanswered = term.request_clipboard_read(zgx::terminal::ClipboardRead::new(move |allow| {
        told.set(Some(allow))
    }));
    assert!(unanswered.is_none(), "the script's handler takes it");
    h.settle();
    assert_eq!(h.text("asked"), "1");
    assert_eq!(answer.get(), None, "not answered yet");
    h.click("allow");
    assert_eq!(answer.get(), Some(true));
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
    let image =
        |shade: u8| std::sync::Arc::new(zgx::image::ImageData::new(2, 2, vec![shade; 16]).unwrap());
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

/// A terminal that shows a 2 by 1 picture.
struct Pictured;
impl zgx::terminal::TerminalBackend for Pictured {
    fn run(&self, _: zgx::terminal::TerminalCommand) {}
    fn snapshot(&self, max_width: u32) -> Option<std::sync::Arc<zgx::image::ImageData>> {
        assert_eq!(max_width, 120);
        let pixels = vec![255u8; 2 * 4];
        Some(std::sync::Arc::new(
            zgx::image::ImageData::new(2, 1, pixels).unwrap(),
        ))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn an_elements_snapshot_is_the_hosts_picture_of_its_box() {
    let mut h = Harness::inline(
        r##"local card = ui.ref()
        local src = ui.state("src", "none")
        return ui.column {
            ui.div { ref = card, w = 60, h = 30 },
            ui.button { id = "take", w = 20, h = 20, on_click = function()
                src:set(card:get():snapshot(100) or "none")
            end },
            ui.text { "{src}", id = "src" },
            ui.image { id = "picture", w = 40, h = 20, src = function()
                local s = src:get()
                return if s ~= "none" then s else "host:nothing"
            end },
        }"##,
    );
    // A host that takes none: no picture.
    h.click("take");
    h.settle();
    assert_eq!(h.text("src"), "none");
    // One that does: a pixel a point, of the box it is asked for.
    let asked = std::rc::Rc::new(std::cell::Cell::new(None));
    let seen = asked.clone();
    h.ui.on_snapshot(move |rect, max_width| {
        seen.set(Some((rect.width, rect.height, max_width)));
        let (w, h) = (rect.width as u32, rect.height as u32);
        zgx::image::ImageData::new(w, h, vec![255; (w * h * 4) as usize])
            .ok()
            .map(std::sync::Arc::new)
    });
    h.click("take");
    h.settle();
    assert_eq!(asked.get(), Some((60., 30., 100)));
    let src = h.text("src");
    assert!(src.starts_with("host:zgx.element:"), "{src}");
    h.ui.prepare_frame();
    let node = h.find("picture").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = None;
    while let Some(n) = stack.pop() {
        if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
            found = Some((image.width(), image.height()));
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert_eq!(
        found,
        Some((60, 30)),
        "the image shows the element's picture"
    );
}

/// A picture of an element goes with its node: the host keeps none of what
/// is gone.
#[test]
fn an_elements_snapshot_goes_with_its_node() {
    let mut h = Harness::inline(
        r##"local card = ui.ref()
        local src = ui.state("src", "none")
        local open = ui.state("open", true)
        local look = ui.state("look", false)
        return ui.column {
            ui.show(open, ui.div { ref = card, w = 60, h = 30 }),
            ui.button { id = "take", w = 20, h = 20, on_click = function()
                src:set(card:get():snapshot(100) or "none")
            end },
            ui.button { id = "close", w = 20, h = 20, on_click = function() open:set(false) end },
            ui.button { id = "look", w = 20, h = 20, on_click = function() look:set(true) end },
            ui.show(look, ui.image { id = "picture", w = 40, h = 20, src = function()
                return src:get()
            end }),
        }"##,
    );
    h.ui.on_snapshot(|rect, _| {
        let (w, h) = (rect.width as u32, rect.height as u32);
        zgx::image::ImageData::new(w, h, vec![255; (w * h * 4) as usize])
            .ok()
            .map(std::sync::Arc::new)
    });
    h.click("take");
    h.settle();
    h.click("close");
    h.settle();
    h.click("look");
    h.settle();
    let node = h.find("picture").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = None;
    while let Some(n) = stack.pop() {
        if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
            found = Some((image.width(), image.height()));
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert_eq!(found, Some((1, 1)), "its picture went with it");
}

/// A terminal's picture goes as it closes.
#[test]
fn a_terminals_snapshot_goes_as_it_closes() {
    let mut h = Harness::inline(
        r##"local term = ui.term("p1")
        local src = ui.state("src", "none")
        local look = ui.state("look", false)
        return ui.column {
            ui.terminal { term = term, w = 200, h = 100 },
            ui.button { id = "take", w = 20, h = 20, on_click = function()
                src:set(term:snapshot(120) or "none")
            end },
            ui.button { id = "close", w = 20, h = 20, on_click = function() term:close() end },
            ui.button { id = "look", w = 20, h = 20, on_click = function() look:set(true) end },
            ui.show(look, ui.image { id = "picture", w = 40, h = 20, src = function()
                return src:get()
            end }),
        }"##,
    );
    terminal(&mut h).attach(std::rc::Rc::new(Pictured));
    h.click("take");
    h.settle();
    h.click("close");
    h.settle();
    h.click("look");
    h.settle();
    let node = h.find("picture").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = None;
    while let Some(n) = stack.pop() {
        if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
            found = Some((image.width(), image.height()));
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert_eq!(found, Some((1, 1)), "its picture went as it closed");
}

#[test]
fn a_terminals_snapshot_shows_in_an_image() {
    let mut h = Harness::inline(
        r##"local term = ui.term("p1")
        local src = ui.state("src", "none")
        return ui.column {
            ui.terminal { term = term, w = 200, h = 100 },
            ui.button { id = "take", w = 20, h = 20, on_click = function()
                src:set(term:snapshot(120) or "none")
            end },
            ui.text { "{src}", id = "src" },
            ui.image { id = "picture", w = 40, h = 20, src = function()
                local s = src:get()
                return if s ~= "none" then s else "host:nothing"
            end },
        }"##,
    );
    // Before a platform terminal shows it: no picture.
    h.click("take");
    h.settle();
    assert_eq!(h.text("src"), "none");
    let term = terminal(&mut h);
    term.attach(std::rc::Rc::new(Pictured));
    h.click("take");
    h.settle();
    assert_eq!(h.text("src"), "host:zgx.term:p1");
    h.ui.prepare_frame();
    let node = h.find("picture").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = None;
    while let Some(n) = stack.pop() {
        if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
            found = Some((image.width(), image.height()));
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert_eq!(
        found,
        Some((2, 1)),
        "the image shows the terminal's picture"
    );
}

#[test]
fn a_destination_springs_open_unless_it_declines() {
    use std::time::{Duration, Instant};
    use zgx::input::{InputEvent, PointerButton};
    let mut h = Harness::inline(
        r##"local sprang = ui.state("sprang", "")
        local function target(name: string, open: boolean)
            return ui.div { id = name, w = 80, h = 40, on_drop = function() end,
                on_drag_spring = function(payload)
                    sprang:set(sprang:peek() .. name .. ":" .. payload .. " ")
                    if not open then
                        return false
                    end
                    return nil
                end }
        end
        return ui.column {
            ui.div { id = "source", w = 80, h = 40, on_drag = function() return "card" end },
            -- What a target declines goes on to what holds it.
            ui.column {
                on_drag_spring = function(payload)
                    sprang:set(sprang:peek() .. "column:" .. payload .. " ")
                end,
                target("opens", true),
                target("declines", false),
            },
            ui.text { "{sprang}", id = "log" },
        }"##,
    );
    let at = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        (b.x + b.width / 2., b.y + b.height / 2.)
    };
    let (sx, sy) = at(&mut h, "source");
    h.ui.dispatch(InputEvent::PointerMove { x: sx, y: sy });
    h.ui.dispatch(InputEvent::PointerDown {
        x: sx,
        y: sy,
        button: PointerButton::Primary,
    });
    for target in ["opens", "declines"] {
        let (x, y) = at(&mut h, target);
        h.ui.dispatch(InputEvent::PointerMove { x, y: y - 1. });
        h.ui.dispatch(InputEvent::PointerMove { x, y });
        h.ui.advance_interactions(Instant::now() + Duration::from_secs(1))
            .unwrap();
        h.settle();
    }
    assert_eq!(h.text("log"), "opens:card declines:card column:card ");
    h.ui.dispatch(InputEvent::PointerUp {
        x: sx,
        y: sy,
        button: PointerButton::Primary,
    });
}

#[test]
fn an_image_with_dither_is_drawn_in_dots_of_that_size() {
    let mut h = Harness::inline(
        r##"local dots = ui.bitmap(4, 4, function() return 1 end)
        return ui.column {
            ui.image { id = "plain", src = dots, w = 40, h = 40 },
            ui.image { id = "dithered", src = dots, w = 40, h = 40, fit = "cover", dither = 2 },
        }"##,
    );
    h.ui.prepare_frame();
    let dither = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
                return image.dither();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no image in {id}")
    };
    assert_eq!(dither(&mut h, "plain"), 0.);
    assert_eq!(dither(&mut h, "dithered"), 2.);
}

#[test]
fn an_images_dither_follows_its_state_and_keeps_to_a_fallbacks_picture() {
    let mut h = Harness::inline(
        r##"local dots = ui.bitmap(4, 4, function() return 1 end)
        local dot = ui.state("dot", 2)
        return ui.column {
            ui.button { id = "finer", on_click = function() dot:set(1) end, "finer" },
            ui.image { id = "pic", src = dots, w = 40, h = 40, fallback = ui.text { "…" },
                dither = function() return dot:get() end },
        }"##,
    );
    h.settle();
    let dither = |h: &mut Harness| {
        let node = h.find("pic").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
                return image.dither();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no image")
    };
    assert_eq!(dither(&mut h), 2.);
    h.click("finer");
    h.settle();
    assert_eq!(dither(&mut h), 1.);
}

#[test]
fn an_images_shader_params_reach_its_node_and_animate_its_reveal() {
    let mut h = Harness::inline(
        r##"local dots = ui.bitmap(4, 4, function() return 1 end)
        return ui.image { id = "pic", src = dots, w = 40, h = 40, characters = 2,
            shader_params = { { 0, 0.25, 0.2, 0 }, { 0, 0.1, 0, 0 } },
            on_mount = function(el)
                anime.animate(el, { shaderParam0 = 0.5, duration = 100, ease = "linear" })
            end }"##,
    );
    h.ui.prepare_frame();
    let image = |h: &mut Harness| {
        let node = h.find("pic").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
                return (n, image.effect());
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no image")
    };
    let (node, effect) = image(&mut h);
    assert_eq!(effect, zgx::image::ImageEffect::Characters(2.));
    let params = h.ui.scene.borrow().shader_uniforms(node).params;
    assert_eq!(params[0][1..3], [0.25, 0.2], "its band and raggedness");
    assert_eq!(params[1][1], 0.1, "its centre");
    for _ in 0..12 {
        h.frame();
    }
    let params = h.ui.scene.borrow().shader_uniforms(node).params;
    assert!(
        (params[0][0] - 0.5).abs() < 0.01,
        "the front moved: {params:?}"
    );
    assert_eq!(params[0][1], 0.25, "the band kept");
}

#[test]
fn reserved_commands_publish_their_shortcuts() {
    let h = Harness::inline(
        r##"ui.command { id = "tab", title = "New Tab", menu = "Shell", key = "cmd+t", reserved = true, run = function() end }
        ui.command { id = "find", title = "Find", menu = "Edit", key = "cmd+f", run = function() end }
        ui.command { id = "close", title = "Close", menu = "Shell", key = "cmd+w", reserved = true, run = function() end }
        ui.command { id = "keyless", title = "No key", menu = "Shell", reserved = true, run = function() end }
        return ui.column {}"##,
    );
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.host.reserved_keys().get(), ["cmd+t", "cmd+w"]);
    assert_eq!(
        h.host.commands().get().len(),
        4,
        "reserved commands are menu items too"
    );
}
