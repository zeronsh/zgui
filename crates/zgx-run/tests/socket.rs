//! The connector `zgx-run` gives a script (`zgx_run::install_connector`),
//! against a WebSocket server on this machine: text and binary messages
//! each way, a dropped connection opened again, and the script's close
//! reaching the server as a closing handshake.
#![cfg(feature = "socket")]
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tungstenite::Message;
use zgx_script::{ScriptHost, testing::TestApp};

/// What the server saw.
#[derive(Default)]
struct Seen {
    accepted: AtomicUsize,
    /// A client closed with a closing handshake.
    closed: AtomicBool,
}

/// A server echoing each message back: text `"drop"` drops the connection
/// without a word.
fn serve() -> (String, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/room", listener.local_addr().unwrap());
    let seen = Arc::new(Seen::default());
    let server = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = server.clone();
            std::thread::spawn(move || {
                let Ok(mut ws) = tungstenite::accept(stream) else {
                    return;
                };
                seen.accepted.fetch_add(1, Ordering::SeqCst);
                loop {
                    match ws.read() {
                        Ok(Message::Text(text)) if text.as_str() == "drop" => return,
                        Ok(message @ (Message::Text(_) | Message::Binary(_))) => {
                            if ws.send(message).is_err() {
                                return;
                            }
                        }
                        Ok(Message::Close(_)) => {
                            seen.closed.store(true, Ordering::SeqCst);
                            let _ = ws.flush();
                            return;
                        }
                        Ok(_) => {}
                        Err(_) => return,
                    }
                }
            });
        }
    });
    (url, seen)
}

fn app(url: &str) -> TestApp {
    let script = format!(
        r#"
local log = ui.state("log", "")
local socket = ui.socket("{url}", {{
    reconnect = {{ min = 20, jitter = 0 }},
    on_message = function(m, binary)
        log:set(`{{log:peek()}}{{if binary then "bytes " .. #m else m}};`)
    end,
}})
return ui.column {{
    ui.text {{ id = "status", function() return socket.status:get() end }},
    ui.text {{ id = "log", "{{log}}" }},
    ui.button {{ id = "send", w = 40, h = 20, "s", on_click = function()
        socket.send("hello")
        socket.send_bytes("\0\1\2\3")
        socket.send({{ n = 1 }})
    end }},
    ui.button {{ id = "drop", w = 40, h = 20, "d", on_click = function() socket.send("drop") end }},
    ui.button {{ id = "close", w = 40, h = 20, "c", on_click = socket.close }},
}}
"#
    );
    TestApp::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", script);
        zgx_run::install_connector(&host);
        host
    })
}

#[test]
fn messages_go_each_way() {
    let (url, _) = serve();
    let mut app = app(&url);
    assert!(
        app.wait_until(|app| app.text("status") == "open"),
        "{:?}",
        app.error()
    );
    app.click("send");
    assert!(app.wait_until(|app| app.text("log") == r#"hello;bytes 4;{"n":1};"#));
}

#[test]
fn a_dropped_connection_opens_again() {
    let (url, seen) = serve();
    let mut app = app(&url);
    assert!(app.wait_until(|app| app.text("status") == "open"));
    app.click("drop");
    assert!(app.wait_until(|_| seen.accepted.load(Ordering::SeqCst) == 2));
    assert!(app.wait_until(|app| app.text("status") == "open"));
    app.click("send");
    assert!(app.wait_until(|app| app.text("log").starts_with("hello;")));
}

#[test]
fn the_scripts_close_reaches_the_server() {
    let (url, seen) = serve();
    let mut app = app(&url);
    assert!(app.wait_until(|app| app.text("status") == "open"));
    app.click("close");
    let until = Instant::now() + Duration::from_secs(5);
    while !seen.closed.load(Ordering::SeqCst) && Instant::now() < until {
        app.settle();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(seen.closed.load(Ordering::SeqCst), "no closing handshake");
    assert_eq!(app.text("status"), "closed");
    std::thread::sleep(Duration::from_millis(60));
    app.settle();
    assert_eq!(
        seen.accepted.load(Ordering::SeqCst),
        1,
        "closed: not opened again"
    );
}

#[test]
fn nothing_listening_is_a_network_error() {
    let mut app = TestApp::new(|runtime| {
        let host = ScriptHost::inline(
            runtime,
            "app",
            r#"
local socket = ui.socket("ws://127.0.0.1:1/", { reconnect = false })
return ui.text { id = "error", function()
    local err = socket.error
    return if err then `{socket.status:get()} {err.kind}` else socket.status:get()
end }
"#,
        );
        zgx_run::install_connector(&host);
        host
    });
    assert!(
        app.wait_until(|app| app.text("error") == "closed network"),
        "{}",
        app.text("error")
    );
}

/// `wss://` through rustls, against an echo server on the internet: run by
/// hand (`ZGX_WSS_ECHO=wss://echo.websocket.org cargo test -p zgx-run
/// --test socket -- --ignored`).
#[test]
#[ignore]
fn a_secure_connection_echoes() {
    let url = std::env::var("ZGX_WSS_ECHO").unwrap_or_else(|_| "wss://echo.websocket.org".into());
    let mut app = app(&url);
    assert!(
        app.wait_until(|app| app.text("status") == "open"),
        "{} {:?}",
        app.text("status"),
        app.error()
    );
    app.click("send");
    assert!(
        app.wait_until(|app| app.text("log").contains("hello;")),
        "{}",
        app.text("log")
    );
}
