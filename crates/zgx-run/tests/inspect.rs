//! Devtools for agents (`zgx_script::inspect`, which zgx-run builds with):
//! what a running app answers `zgx inspect`, through a session and over
//! its socket.
use std::time::{Duration, Instant};
use zgx_script::{
    ScriptHost, Value,
    inspect::{Reply, Session},
    testing::TestApp,
};

const APP: &str = r##"
local count = ui.state("count", 0)
local name = ui.state("name", "")
ui.command { id = "reset", run = function() count:set(0) end }
return ui.column {
    ui.button { id = "add", w = 60, h = 24, on_click = ui.add(count, 1), "Add" },
    ui.text { id = "count", "{count}" },
    ui.input { id = "name", label = "Name", value = name, w = 120 },
    ui.show(function() return count:get() >= 2 end, ui.text { id = "many", "Many" }),
}
"##;

fn app() -> (TestApp, Session) {
    let app = TestApp::new(|runtime| ScriptHost::inline(runtime, "app", APP));
    let session = Session::new(&app.ui, app.handle.clone(), &app.host, None);
    (app, session)
}
/// Ask `session` `request`, settling `app` until a wait is over.
fn ask(app: &TestApp, session: &Session, request: &str) -> Value {
    let answer = match session.answer(request) {
        Reply::Now(answer) => answer,
        Reply::Later(later) => {
            let out = std::rc::Rc::new(std::cell::RefCell::new(None));
            let into = out.clone();
            app.executor.borrow_mut().spawn(async move {
                *into.borrow_mut() = Some(later.await);
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while out.borrow().is_none() {
                assert!(Instant::now() < deadline, "the wait never answered");
                app.frame();
                std::thread::sleep(Duration::from_millis(2));
            }
            out.borrow_mut().take().unwrap()
        }
    };
    app.settle();
    Value::from_json(&answer).unwrap_or_else(|| panic!("not JSON: {answer}"))
}
fn text(answer: &Value) -> String {
    assert!(
        answer.field("ok").is_some_and(Value::truthy),
        "failed: {}",
        answer.to_json()
    );
    answer
        .field("text")
        .map(Value::to_string)
        .unwrap_or_default()
}
fn error(answer: &Value) -> String {
    assert!(
        !answer.field("ok").is_some_and(Value::truthy),
        "succeeded: {}",
        answer.to_json()
    );
    answer
        .field("error")
        .map(Value::to_string)
        .unwrap_or_default()
}

#[test]
fn a_session_answers_the_tree_states_and_actions() {
    let (app, session) = app();
    let tree = text(&ask(&app, &session, r#"{"cmd":"tree"}"#));
    assert!(tree.contains(r#"#add button "Add""#), "{tree}");
    assert!(tree.contains(r##"#count "0""##), "{tree}");
    // JSON too: a line a node, with its number, id, role and box.
    let data = ask(&app, &session, r##"{"cmd":"tree","target":"#add"}"##);
    let first = data.field("data").unwrap().list()[0].clone();
    assert_eq!(
        first.field("id").map(Value::to_string).as_deref(),
        Some("add")
    );
    assert_eq!(
        first.field("role").map(Value::to_string).as_deref(),
        Some("button")
    );
    // Actions, by the same targets tests use.
    text(&ask(
        &app,
        &session,
        r#"{"cmd":"click","target":"role:button \"Add\""}"#,
    ));
    assert_eq!(app.handle.find("count").map(|_| ()), Some(()));
    let state = text(&ask(&app, &session, r#"{"cmd":"state","name":"count"}"#));
    assert_eq!(state, "count = 1");
    text(&ask(
        &app,
        &session,
        r#"{"cmd":"type","target":"\"Name\"","text":"Ada"}"#,
    ));
    assert!(text(&ask(&app, &session, r#"{"cmd":"state"}"#)).contains(r#"name = "Ada""#));
    text(&ask(
        &app,
        &session,
        r#"{"cmd":"set","name":"count","value":5}"#,
    ));
    let node = text(&ask(
        &app,
        &session,
        r##"{"cmd":"node","target":"#count"}"##,
    ));
    assert!(node.contains("text: 5"), "{node}");
    text(&ask(&app, &session, r#"{"cmd":"command","id":"reset"}"#));
    assert!(
        text(&ask(
            &app,
            &session,
            r##"{"cmd":"tree","target":"#count"}"##
        ))
        .contains(r#""0""#)
    );
    // A wait until something shows, and one that times out saying why.
    text(&ask(
        &app,
        &session,
        r#"{"cmd":"set","name":"count","value":3}"#,
    ));
    text(&ask(
        &app,
        &session,
        r##"{"cmd":"wait","target":"#many","timeout_ms":2000}"##,
    ));
    let late = error(&ask(
        &app,
        &session,
        r##"{"cmd":"wait","target":"#many","gone":true,"timeout_ms":50}"##,
    ));
    assert!(late.contains("timed out"), "{late}");
    // What is wrong is said: an unknown command, a target matching nothing.
    assert!(error(&ask(&app, &session, r#"{"cmd":"fly"}"#)).contains("help"));
    assert!(
        error(&ask(
            &app,
            &session,
            r#"{"cmd":"click","target":"\"Nope\""}"#
        ))
        .contains("nothing matches")
    );
    assert_eq!(text(&ask(&app, &session, r#"{"cmd":"errors"}"#)), "none");
    assert!(text(&ask(&app, &session, r#"{"cmd":"help"}"#)).contains("targets:"));
}

#[test]
fn an_app_serves_its_session_on_a_socket_only_its_user_reaches() {
    let (app, session) = app();
    let executor = app.executor.clone();
    let server = zgx_script::inspect::serve(session, "test app", move |task| {
        executor.borrow_mut().spawn(task);
    })
    .expect("listening");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(server.path()), 0o600, "the socket is its user's");
        assert_eq!(
            mode(server.path().parent().unwrap()),
            0o700,
            "and so is its directory"
        );
    }
    let pid = std::process::id();
    // The client blocks on the answer, which the app's thread makes.
    let client = std::thread::spawn(move || {
        let ask = |request: &str| {
            zgx_script::inspect_client::request(pid, &Value::from_json(request).unwrap()).unwrap()
        };
        (
            ask(r##"{"cmd":"click","target":"#add"}"##),
            ask(r#"{"cmd":"state","name":"count"}"#),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "no answer");
        app.settle();
        std::thread::sleep(Duration::from_millis(1));
    }
    let (clicked, state) = client.join().unwrap();
    assert!(clicked.contains(r#""ok":true"#), "{clicked}");
    assert!(state.contains("count = 1"), "{state}");
    assert!(
        zgx_script::inspect_client::apps()
            .iter()
            .any(|a| a.pid == pid && a.app == "test app"),
        "listed"
    );
    drop(server);
}
