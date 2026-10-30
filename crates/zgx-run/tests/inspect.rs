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

#[test]
fn search_answers_the_count_the_current_match_and_the_first_few() {
    let app = TestApp::new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r#"return ui.column { ui.text { "Café one" }, ui.text { "cafe two" }, ui.text { "CAFE three" },
    ui.text { "cafés four" }, ui.text { "café five" }, ui.text { "cafe six" }, ui.text { "nothing" } }"#,
        )
    });
    let session = Session::new(&app.ui, app.handle.clone(), &app.host, None);
    let answer = ask(&app, &session, r#"{"cmd":"search","query":"cafe"}"#);
    let said = text(&answer);
    assert!(
        said.starts_with("6 matches for \"cafe\", 1 current"),
        "{said}"
    );
    let data = answer.field("data").unwrap();
    assert_eq!(data.field("count").and_then(Value::as_f64), Some(6.));
    assert!(data.field("done").is_some_and(Value::truthy));
    assert_eq!(data.field("current").and_then(Value::as_f64), Some(1.));
    let Some(Value::List(matches)) = data.field("matches") else {
        panic!("matches: {}", answer.to_json());
    };
    assert_eq!(matches.len(), 5, "the first five, by default");
    assert_eq!(
        matches[0].field("context").map(Value::to_string).as_deref(),
        Some("[Café] one")
    );
    assert!(matches[0].field("rect").is_some(), "shown: its box");
    // Every match, the next current in the window too.
    let answer = ask(&app, &session, r#"{"cmd":"search","next":true,"all":true}"#);
    let data = answer.field("data").unwrap();
    assert_eq!(data.field("current").and_then(Value::as_f64), Some(2.));
    let Some(Value::List(matches)) = data.field("matches") else {
        panic!("matches");
    };
    assert_eq!(matches.len(), 6);
    assert_eq!(app.ui.find_status().current, Some(1));
    text(&ask(&app, &session, r#"{"cmd":"search","close":true}"#));
    assert!(!app.ui.find_status().open);
    // Nothing open, no query: says so.
    let said = error(&ask(&app, &session, r#"{"cmd":"search"}"#));
    assert!(said.contains("query"), "{said}");
}

/// Play `request` (a replay) through `session`, clicking meanwhile so each
/// frame does something (idle ones are not kept): its answer.
fn replay(app: &mut TestApp, session: &Session, request: &Value) -> Value {
    let answer = match session.answer(&request.to_json()) {
        Reply::Now(answer) => answer,
        Reply::Later(later) => {
            let out = std::rc::Rc::new(std::cell::RefCell::new(None));
            let into = out.clone();
            app.executor.borrow_mut().spawn(async move {
                *into.borrow_mut() = Some(later.await);
            });
            let deadline = Instant::now() + Duration::from_secs(20);
            while out.borrow().is_none() {
                assert!(Instant::now() < deadline, "the replay never answered");
                app.click("add");
                app.frame();
                std::thread::sleep(Duration::from_millis(2));
            }
            out.borrow_mut().take().unwrap()
        }
    };
    Value::from_json(&answer).unwrap()
}

/// A host's wheel tape: it answers `stop` with `recorded` and keeps what it
/// was asked to play.
fn tape_host(
    app: &TestApp,
    recorded: &'static str,
) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
    let played = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let into = played.clone();
    app.ui.on_wheel_tape(move |request| match request {
        zgx::widgets::WheelTape::Record => Ok("recording".into()),
        zgx::widgets::WheelTape::Stop => Ok(recorded.into()),
        zgx::widgets::WheelTape::Play { tape, .. } => {
            into.borrow_mut().push(tape);
            Ok("playing".into())
        }
    });
    played
}

#[test]
fn frames_read_since_a_mark_with_the_runs_asked_for_the_gpu_and_the_engines_notes() {
    let (mut app, session) = app();
    text(&ask(&app, &session, r##"{"cmd":"click","target":"#add"}"##));
    let marked = ask(&app, &session, r#"{"cmd":"mark","name":"here"}"#);
    let after = marked
        .field("data")
        .and_then(|d| d.field("frame"))
        .and_then(Value::as_f64)
        .unwrap();
    // A frame the engine says something of, and the GPU's own time for it.
    zgx::profile::note(|| "a scroll repainted its view: a test said so".into());
    zgx::profile::gpu_time(Duration::from_micros(1500));
    app.click("add");
    app.frame();
    let since = ask(&app, &session, r#"{"cmd":"frames","since":"here"}"#);
    let frames = text(&since);
    assert!(frames.contains("): count"), "{frames}");
    assert!(
        frames.contains("why: a scroll repainted its view: a test said so"),
        "{frames}"
    );
    assert!(frames.contains("gpu: 1500 µs"), "{frames}");
    let listed = since.field("data").unwrap().list();
    assert!(!listed.is_empty());
    for frame in listed.iter() {
        assert!(frame.field("frame").and_then(Value::as_f64).unwrap() > after);
    }
    assert!(listed.iter().any(
        |f| f.field("gpu_micros").and_then(Value::as_f64) == Some(1500.)
            && f.field("notes").is_some_and(|n| !n.list().is_empty())
    ));
    // As many runs as asked: none, or all of them.
    let none = text(&ask(
        &app,
        &session,
        r#"{"cmd":"frames","since":"here","runs":0}"#,
    ));
    assert!(none.contains("more (`runs N` or `all`)"), "{none}");
    let all = ask(
        &app,
        &session,
        r#"{"cmd":"frames","since":"here","all":true}"#,
    );
    assert!(!text(&all).contains("more (`runs N`"));
    assert!(all.field("data").unwrap().list().iter().any(|f| {
        f.field("run_list")
            .is_some_and(|l| l.list().iter().any(|r| r.field("cause").is_some()))
    }));
    // A frame's number reads the same, given as the command line gives it.
    let numbered = ask(
        &app,
        &session,
        &format!(r#"{{"cmd":"frames","since":"{after}"}}"#),
    );
    assert_eq!(
        numbered.field("data").unwrap().list().len(),
        listed.len(),
        "{}",
        text(&numbered)
    );
    // An unknown mark says how to set one.
    let unknown = error(&ask(&app, &session, r#"{"cmd":"frames","since":"nope"}"#));
    assert!(unknown.contains("mark nope"), "{unknown}");
    // How many are kept: fewer, then the default again.
    let kept = ask(&app, &session, r#"{"cmd":"frames","n":100,"keep":2}"#);
    assert!(kept.field("data").unwrap().list().len() <= 2);
    text(&ask(&app, &session, r#"{"cmd":"frames","keep":480}"#));
    assert_eq!(zgx::trace::kept(), 480);
}

#[test]
fn memory_says_what_holds_it_a_line_each() {
    let (app, session) = app();
    let answer = ask(&app, &session, r#"{"cmd":"memory"}"#);
    let table = text(&answer);
    assert!(table.contains("scene nodes mounted"), "{table}");
    let line = |area: &str, name: &str| {
        answer
            .field("data")
            .unwrap()
            .list()
            .iter()
            .find(|l| {
                l.field("area").map(Value::to_string).as_deref() == Some(area)
                    && l.field("name")
                        .is_some_and(|n| n.to_string().starts_with(name))
            })
            .unwrap_or_else(|| panic!("no {area} {name}: {}", answer.to_json()))
            .clone()
    };
    let number = |line: Value, field: &str| line.field(field).and_then(Value::as_f64).unwrap();
    assert_eq!(
        number(line("scene", "nodes mounted"), "n") as usize,
        app.ui.scene.borrow().len()
    );
    assert!(number(line("scene", "slots"), "bytes") > 0.);
    assert!(number(line("luau", "app"), "bytes") > 100_000.);
    // Nothing shown, nothing cached.
    assert_eq!(number(line("images", "decoded shown"), "n"), 0.);
    assert_eq!(number(line("images", "decoded cached"), "bytes"), 0.);
    // Frames kept for `frames`, here where devtools are.
    line("devtools", "frame log");
}

#[test]
fn switches_are_listed_and_switched_by_name() {
    let (app, session) = app();
    let listed = text(&ask(&app, &session, r#"{"cmd":"switch"}"#));
    for name in ["scroll_copy on", "refresh_grid on", "partial_repaint on"] {
        assert!(listed.contains(name), "{listed}");
    }
    let off = text(&ask(
        &app,
        &session,
        r#"{"cmd":"switch","name":"partial_repaint","on":"off"}"#,
    ));
    assert!(off.starts_with("partial_repaint off"), "{off}");
    assert!(!zgx::switches::PARTIAL_REPAINT.on());
    text(&ask(
        &app,
        &session,
        r#"{"cmd":"switch","name":"partial_repaint","on":true}"#,
    ));
    assert!(zgx::switches::PARTIAL_REPAINT.on());
    let unknown = error(&ask(&app, &session, r#"{"cmd":"switch","name":"warp"}"#));
    assert!(unknown.contains("no switch `warp`"), "{unknown}");
}

#[test]
fn a_window_not_presenting_is_said_and_refuses_a_replay() {
    let (mut app, session) = app();
    tape_host(&app, "");
    app.ui.set_presented(false);
    let frames = text(&ask(&app, &session, r#"{"cmd":"frames"}"#));
    assert!(
        frames.starts_with("the window isn't presenting"),
        "{frames}"
    );
    let request = Value::record([
        ("cmd", Value::from("replay")),
        ("target", Value::from("#add")),
        (
            "tape",
            Value::from("# zgx wheel tape 1\n0.000 pixel 0 -4 started\n"),
        ),
    ]);
    let refused = error(&ask(&app, &session, &request.to_json()));
    assert!(
        refused.contains("isn't presenting") && refused.contains("show the window"),
        "{refused}"
    );
    // Shown again: the next frame says it stopped and presents again.
    app.ui.set_presented(true);
    app.click("add");
    app.frame();
    let frames = text(&ask(&app, &session, r#"{"cmd":"frames"}"#));
    assert!(!frames.starts_with("the window isn't"), "{frames}");
    assert!(
        frames.contains("why: the window stopped presenting")
            && frames.contains("why: the window presents again"),
        "{frames}"
    );
}

#[test]
fn a_replay_repeats_with_its_spread_and_spreads_a_coalesced_tape() {
    let (mut app, session) = app();
    // Recorded every other refresh at 120 Hz: the window fell behind.
    let mut recorded = String::from("# zgx wheel tape 1\n0.000 pixel 0 -2 started\n");
    for i in 1..=10 {
        recorded.push_str(&format!("{:.3} pixel 0 -8 moved\n", i as f64 * 16.667));
    }
    recorded.push_str("180.000 pixel 0 0 ended\n");
    let recorded: &'static str = Box::leak(recorded.into_boxed_str());
    let played = tape_host(&app, recorded);
    app.ui.set_refresh_interval(Duration::from_nanos(8_333_333));
    text(&ask(&app, &session, r#"{"cmd":"record"}"#));
    let stopped = ask(&app, &session, r#"{"cmd":"record","stop":true}"#);
    let said = text(&stopped);
    assert!(
        said.contains("coalesced") && said.contains("replay --every 8.33"),
        "{said}"
    );
    assert_eq!(
        stopped.field("data").unwrap().field("coalesced"),
        Some(&Value::Bool(true))
    );
    // Spread over every 8 ms, each recorded event's motion in halves.
    let request = Value::record([
        ("cmd", Value::from("replay")),
        ("target", Value::from("#add")),
        ("tape", Value::from(recorded)),
        ("every", Value::Num(8.3335)),
        ("repeat", Value::Num(2.)),
    ]);
    let answer = replay(&mut app, &session, &request);
    let said = text(&answer);
    assert!(
        said.contains(", 2 times:") && said.contains("run 2: ") && said.contains("2 runs: every"),
        "{said}"
    );
    assert_eq!(
        answer
            .field("data")
            .and_then(|d| d.field("runs"))
            .map(|r| r.list().len()),
        Some(2)
    );
    let played = played.borrow();
    assert_eq!(played.len(), 2, "played twice");
    let moves: Vec<&str> = played[0].lines().filter(|l| l.ends_with("moved")).collect();
    assert_eq!(moves.len(), 20, "{}", played[0]);
    assert!(moves.iter().all(|l| l.contains(" 0 -4 ")), "{}", played[0]);
    assert!(played[0].contains("0.000 pixel 0 -2 started"));
    assert!(played[0].contains("180.000 pixel 0 0 ended"));
}

#[test]
fn a_target_matching_hidden_nodes_too_names_the_one_that_shows() {
    let app = TestApp::new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r##"
local pressed = ui.state("pressed", 0)
return ui.column {
    ui.button { id = "shown", w = 60, h = 24, on_click = ui.add(pressed, 1), "Go" },
    ui.column { hidden = true, ui.button { id = "kept", w = 60, h = 24, "Go" } },
}
"##,
        )
    });
    let session = Session::new(&app.ui, app.handle.clone(), &app.host, None);
    text(&ask(&app, &session, r#"{"cmd":"click","target":"\"Go\""}"#));
    assert_eq!(
        text(&ask(&app, &session, r#"{"cmd":"state","name":"pressed"}"#)),
        "pressed = 1"
    );
    // Both still match; the hidden one says so.
    let found = text(&ask(&app, &session, r#"{"cmd":"find","target":"\"Go\""}"#));
    assert_eq!(found.lines().count(), 2, "{found}");
    assert_eq!(found.matches(" hidden").count(), 1, "{found}");
}
