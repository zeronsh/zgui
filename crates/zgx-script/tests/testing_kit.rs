//! The public testing kit (`zgx_script::testing::TestApp`), used as an app
//! author would: typing, keys, waiting for async host calls, and holding a
//! hot path to no script calls.
use std::time::Duration;
use zgx::input::Key;
use zgx_script::{ScriptHost, Value, testing::TestApp};

const APP: &str = r#"
local name = ui.state("name", "")
local greeting = ui.state("greeting", "")
local pressed = ui.state("pressed", "")
local user = ui.call("load_user", 7)
return ui.column {
    on_key = function(e) pressed:set(e.key) end,
    ui.input { id = "name", value = name },
    ui.text { id = "echo", "Hello, {name}" },
    ui.text { id = "key", "{pressed}" },
    ui.text { id = "user", function() return if user.done then user.value else "…" end },
    ui.button { id = "greet", w = 60, h = 24, "Greet", on_click = function()
        greeting:set(`Hi {name:get()}`)
    end },
    ui.text { id = "greeting", "{greeting}" },
}
"#;

fn app() -> TestApp {
    TestApp::new(|runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.async_action("load_user", |_| async {
            zgx::timer::sleep(Duration::from_millis(3)).await;
            Ok(Value::Str("Ada".into()))
        });
        host
    })
}

#[test]
fn typing_clicking_and_waiting_for_the_host() {
    let mut app = app();
    app.type_text("name", "Grace");
    assert_eq!(app.text("echo"), "Hello, Grace");
    app.click("greet");
    assert_eq!(app.text("greeting"), "Hi Grace");
    assert!(app.wait_until(|app| app.text("user") == "Ada"));
    assert!(app.texts().contains(&"Hi Grace".to_string()));
    let b = app.bounds("greet");
    assert!(b.width == 60. && b.height == 24., "{b:?}");
    assert_eq!(app.error(), None);
}

#[test]
fn keys_reach_what_has_focus() {
    let mut app = app();
    app.click("name");
    app.key(Key::Escape);
    assert_eq!(app.text("key"), "Escape");
}

#[test]
fn frames_with_nothing_changing_run_no_script() {
    let app = app();
    let calls = app.calls();
    app.frames(30);
    assert_eq!(app.calls(), calls);
}
