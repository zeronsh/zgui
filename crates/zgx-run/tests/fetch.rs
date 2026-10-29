//! The fetcher `zgx-run` gives a script (`zgx_run::install_fetcher`),
//! against a server on this machine: answers read whole (status, ok,
//! headers, body, json), a request's method, headers and body sent, a body
//! streamed to `on_chunk`, and a cancelled fetch closing its connection.
#![cfg(feature = "fetch")]
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zgx_script::{ScriptHost, testing::TestApp};

/// A server answering each connection once (`Connection: close`); true in
/// the flag once a stream it was writing was closed by the client.
fn serve() -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let flag = flag.clone();
            std::thread::spawn(move || answer(stream, &flag));
        }
    });
    (base, closed)
}
fn answer(mut stream: TcpStream, closed: &AtomicBool) {
    let mut asked = Vec::new();
    let mut buffer = [0; 4096];
    let head_end = loop {
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            return;
        }
        asked.extend_from_slice(&buffer[..n]);
        if let Some(at) = asked.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&asked[..head_end]).into_owned();
    let header = |name: &str| {
        head.lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_owned())
            })
            .unwrap_or_default()
    };
    let length: usize = header("content-length").parse().unwrap_or(0);
    while asked.len() < head_end + length {
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            break;
        }
        asked.extend_from_slice(&buffer[..n]);
    }
    let body = String::from_utf8_lossy(&asked[head_end..]).into_owned();
    let mut first = head.lines().next().unwrap_or_default().split(' ');
    let (method, path) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or_default(),
    );
    let reply = |status: &str, kind: &str, body: &str| {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nX-Served: here\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    let reply = match path {
        "/user" => reply("200 OK", "application/json", r#"{"name":"Ada","id":7}"#),
        "/missing" => reply("404 Not Found", "text/plain", "not here"),
        "/echo" => reply(
            "200 OK",
            "text/plain",
            &format!("{method} {} {body}", header("x-test")),
        ),
        "/stream" => {
            // Pieces until the client goes, or five seconds.
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
            );
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if stream
                    .write_all(b"ab")
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    closed.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            return;
        }
        _ => reply("500 Internal Server Error", "text/plain", "unknown"),
    };
    let _ = stream.write_all(reply.as_bytes());
}

fn app(base: &str) -> TestApp {
    let upload = std::env::temp_dir().join(format!("zgx-run-upload-{}.txt", std::process::id()));
    std::fs::write(&upload, "a file's text").unwrap();
    let upload = upload.display();
    let script = format!(
        r#"
local base = "{base}"
local UPLOAD = "{upload}"
local shown = ui.state("shown", "")
local chunks = ui.state("chunks", "")
local status = ui.state("status", "")
return ui.column {{
    ui.text {{ id = "shown", function() return shown:get() end }},
    ui.text {{ id = "chunks", function() return chunks:get() end }},
    ui.text {{ id = "status", function() return status:get() end }},
    ui.button {{ id = "get", w = 40, h = 20, "g", on_click = function()
        ui.task(function()
            local user = ui.fetch(base .. "/user"):await()
            local missing = ui.fetch(base .. "/missing"):await()
            local echo = ui.fetch(base .. "/echo", {{
                method = "post", headers = {{ ["x-test"] = "yes" }}, body = "hello",
            }}):await()
            shown:set(`{{user.status}} {{user.ok}} {{user.json.name}} {{user.headers["x-served"]}} / {{missing.status}} {{missing.ok}} {{missing.body}} / {{echo.body}}`)
        end)
    end }},
    ui.button {{ id = "stream", w = 40, h = 20, "s", on_click = function()
        local p
        p = ui.fetch(base .. "/stream", {{
            on_chunk = function(text)
                chunks:set(chunks:peek() .. text)
                if #chunks:peek() >= 6 then
                    p:cancel()
                end
            end,
        }})
        p:after(function() status:set("settled") end, function(e) status:set(e) end)
    end }},
    ui.button {{ id = "upload", w = 40, h = 20, "u", on_click = function()
        ui.task(function()
            local sent = ""
            local r = ui.fetch(base .. "/echo", {{
                file = {{ path = UPLOAD }},
                on_progress = function(n, total) sent = `{{n}}/{{total}}` end,
            }}):await()
            shown:set(`{{r.body}} {{sent}}`)
        end)
    end }},
    ui.button {{ id = "nowhere", w = 40, h = 20, "n", on_click = function()
        ui.fetch("http://127.0.0.1:1/"):after(function() status:set("answered") end, function(e) status:set("failed") end)
    end }},
}}
"#
    );
    TestApp::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", script);
        zgx_run::install_fetcher(&host);
        host
    })
}

#[test]
fn a_fetch_reads_status_headers_body_and_json_and_sends_what_it_is_given() {
    let (base, _) = serve();
    let mut app = app(&base);
    app.click("get");
    assert!(
        app.wait_until(|app| !app.text("shown").is_empty()),
        "{:?}",
        app.error()
    );
    assert_eq!(
        app.text("shown"),
        "200 true Ada here / 404 false not here / POST yes hello"
    );
}

#[test]
fn a_streamed_body_arrives_in_pieces_and_a_cancelled_fetch_closes_its_connection() {
    let (base, closed) = serve();
    let mut app = app(&base);
    app.click("stream");
    assert!(app.wait_until(|app| app.text("chunks").len() >= 6));
    assert!(app.wait_until(|app| app.text("status") == "cancelled"));
    let until = Instant::now() + Duration::from_secs(5);
    while !closed.load(Ordering::SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        closed.load(Ordering::SeqCst),
        "the server wrote on to a reader that stopped"
    );
    // Nothing more is heard once cancelled.
    let heard = app.text("chunks");
    app.frames(10);
    std::thread::sleep(Duration::from_millis(100));
    app.settle();
    assert_eq!(app.text("chunks"), heard);
}

#[test]
fn a_request_nothing_answers_rejects() {
    let (base, _) = serve();
    let mut app = app(&base);
    app.click("nowhere");
    assert!(app.wait_until(|app| !app.text("status").is_empty()));
    assert_eq!(app.text("status"), "failed");
}

#[test]
fn a_file_uploads_and_tells_its_progress() {
    let (base, _) = serve();
    let mut app = app(&base);
    app.click("upload");
    assert!(
        app.wait_until(|app| !app.text("shown").is_empty()),
        "{:?}",
        app.error()
    );
    assert_eq!(app.text("shown"), "POST  a file's text 13/13");
}
