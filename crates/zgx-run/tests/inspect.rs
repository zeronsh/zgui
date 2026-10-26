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
    // And draw, as a window does after it: the frame ends there.
    app.frame();
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
    // What ran and why: the click's write, the binding it ran.
    let frames = text(&ask(&app, &session, r#"{"cmd":"frames","n":20}"#));
    assert!(frames.contains("count →"), "{frames}");
    assert!(
        frames.contains("binding") && frames.contains("← count"),
        "{frames}"
    );
    // And what each frame took, phase by phase.
    assert!(
        frames.contains("phases: ") && frames.contains("layout"),
        "{frames}"
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

#[test]
fn a_snapshot_is_a_png_of_the_node_only_its_user_reads() {
    let (app, session) = app();
    // Without pictures from the host, it says why.
    let refused = error(&ask(
        &app,
        &session,
        r##"{"cmd":"snapshot","target":"#add"}"##,
    ));
    assert!(refused.contains("no picture"), "{refused}");
    // A host's picture of a box: one colour, two pixels a point.
    app.ui.on_snapshot(|rect, max_width| {
        let (w, h) = (
            ((rect.width * 2.) as u32).min(max_width),
            (rect.height * 2.) as u32,
        );
        let pixels: Vec<u8> = (0..w * h).flat_map(|_| [10, 20, 30, 255]).collect();
        zgx::image::ImageData::new(w, h, pixels)
            .ok()
            .map(std::sync::Arc::new)
    });
    let answer = ask(&app, &session, r##"{"cmd":"snapshot","target":"#add"}"##);
    let said = text(&answer);
    let data = answer.field("data").unwrap();
    let path = data.field("path").map(Value::to_string).unwrap();
    assert_eq!(
        data.field("width").and_then(Value::as_f64),
        Some(120.),
        "{said}"
    );
    assert_eq!(
        data.field("height").and_then(Value::as_f64),
        Some(48.),
        "{said}"
    );
    assert!(said.contains("120x48") && said.contains(&path), "{said}");
    let png = std::fs::read(&path).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (120, 48));
    assert_eq!(&pixels[..4], &[10, 20, 30, 255]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the picture is its user's");
    }
    // At most as wide as asked.
    let narrow = ask(
        &app,
        &session,
        r##"{"cmd":"snapshot","target":"#add","width":50}"##,
    );
    assert_eq!(
        narrow
            .field("data")
            .unwrap()
            .field("width")
            .and_then(Value::as_f64),
        Some(50.)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_tape_records_and_replays_through_the_host_saying_how_frames_were_paced() {
    let (mut app, session) = app();
    // Without a host's recorder, it says why.
    let refused = error(&ask(&app, &session, r#"{"cmd":"record"}"#));
    assert!(refused.contains("no wheel input"), "{refused}");
    // A host's: it keeps what devtools asked of it.
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = asked.clone();
    app.ui.on_wheel_tape(move |request| {
        seen.borrow_mut().push(request.clone());
        match request {
            zgx::widgets::WheelTape::Record => Ok("recording".into()),
            zgx::widgets::WheelTape::Stop => {
                Ok("# zgx wheel tape 1\n0.000 pixel 0 -4 started\n8.000 pixel 0 -6 moved\n".into())
            }
            zgx::widgets::WheelTape::Play { .. } => Ok("playing 2 events over 8 ms".into()),
        }
    });
    text(&ask(&app, &session, r#"{"cmd":"record"}"#));
    let stopped = ask(&app, &session, r#"{"cmd":"record","stop":true}"#);
    assert_eq!(text(&stopped), "recorded 2 wheel events");
    let tape = stopped
        .field("data")
        .and_then(|d| d.field("tape"))
        .map(Value::to_string)
        .unwrap();
    // Played back at the target's middle, it answers once the tape and what
    // follows it have played: the frames' pacing.
    let request = Value::record([
        ("cmd", Value::from("replay")),
        ("target", Value::from("#add")),
        ("tape", Value::from(tape.as_str())),
    ]);
    let Reply::Later(later) = session.answer(&request.to_json()) else {
        panic!("a replay answers once played");
    };
    let out = std::rc::Rc::new(std::cell::RefCell::new(None));
    let into = out.clone();
    app.executor.borrow_mut().spawn(async move {
        *into.borrow_mut() = Some(later.await);
    });
    // Frames meanwhile, each doing something (idle ones are not kept).
    let deadline = Instant::now() + Duration::from_secs(10);
    while out.borrow().is_none() {
        assert!(Instant::now() < deadline, "the replay never answered");
        app.click("add");
        app.frame();
        std::thread::sleep(Duration::from_millis(2));
    }
    let replayed = Value::from_json(&out.borrow_mut().take().unwrap()).unwrap();
    let said = text(&replayed);
    assert!(
        said.starts_with("playing 2 events over 8 ms: ") && said.contains("frames, every"),
        "{said}"
    );
    let frames = replayed
        .field("data")
        .and_then(|d| d.field("frames"))
        .and_then(Value::as_f64)
        .unwrap();
    assert!(frames >= 2., "{said}");
    let add = app.handle.find("add").unwrap();
    let b = app.ui.scene.borrow().bounds(add);
    assert_eq!(
        asked.borrow().last(),
        Some(&zgx::widgets::WheelTape::Play {
            tape,
            at: (b.x + b.width / 2., b.y + b.height / 2.)
        })
    );
}
