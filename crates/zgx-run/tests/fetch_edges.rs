//! The fetcher's edges, against a server on this machine (research B's
//! batch): a fetch cancelled while its peer is silent closes the
//! connection at once, before its head or partway through its body; and
//! an invalid byte in a body's piece does not swallow a character split
//! across the next.
#![cfg(feature = "fetch")]
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use zgx_script::{ScriptHost, testing::TestApp};

/// Read a request's head.
fn request(s: &mut TcpStream) {
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut h = Vec::new();
    let mut b = [0; 1024];
    while !h.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = s.read(&mut b).unwrap();
        assert!(n > 0);
        h.extend_from_slice(&b[..n]);
    }
}
/// A server taking one request, then doing `f` with the connection.
fn server(f: impl FnOnce(TcpStream) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        request(&mut s);
        f(s)
    });
    (url, t)
}
fn app(url: &str) -> TestApp {
    let code = format!(
        r#"
local output = ui.state("output", "")
local status = ui.state("status", "")
local p
return ui.column {{
    ui.text {{ id = "output", function() return output:get() end }},
    ui.text {{ id = "status", function() return status:get() end }},
    ui.button {{ id = "start", w = 80, h = 24, "start", on_click = function()
        p = ui.fetch("{url}", {{
            headers = {{ ["User-Agent"] = "zgx-release-check" }},
            on_chunk = function(s) output:set(output:peek() .. s) end,
        }})
        p:after(function(r) status:set(`ok:{{r.status}}:{{r.body}}`) end,
            function(e) status:set("err:" .. e) end)
    end }},
    ui.button {{ id = "cancel", w = 80, h = 24, "cancel", on_click = function() p:cancel() end }},
}}
"#
    );
    TestApp::new(move |r| {
        let host = ScriptHost::inline(r, "fetch_edge", code);
        zgx_run::install_fetcher(&host);
        host
    })
}
#[test]
fn cancelling_a_silent_peer_releases_the_connection() {
    let mut failed = Vec::new();
    for body_started in [false, true] {
        let (tx, ready) = mpsc::channel();
        let (closed, result) = mpsc::channel();
        let (url, t) = server(move |mut s| {
            if body_started {
                s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\nx",
                )
                .unwrap();
            }
            tx.send(()).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut b = [0];
            let r = s.read(&mut b);
            closed.send(matches!(r, Ok(0))).unwrap();
        });
        let mut a = app(&url);
        a.click("start");
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        if body_started {
            assert!(a.wait_until(|a| a.text("output") == "x"));
        }
        a.click("cancel");
        assert!(a.wait_until(|a| a.text("status") == "err:cancelled"));
        let eof = result.recv_timeout(Duration::from_secs(4)).unwrap();
        t.join().unwrap();
        if !eof {
            failed.push(body_started);
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}
#[test]
fn invalid_byte_does_not_consume_a_following_split_utf8_character() {
    let go = Arc::new(AtomicBool::new(false));
    let allow = go.clone();
    let (url, t) = server(move |mut s| {
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\xff\xe2")
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !allow.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        s.write_all(b"\x82\xac").unwrap();
    });
    let mut a = app(&url);
    a.click("start");
    assert!(a.wait_until(|a| !a.text("output").is_empty()));
    go.store(true, Ordering::SeqCst);
    assert!(a.wait_until(|a| !a.text("status").is_empty()));
    t.join().unwrap();
    assert_eq!(
        a.text("output"),
        String::from_utf8_lossy(b"\xff\xe2\x82\xac")
    );
}
/// Server-sent events that start with a byte order mark: both events
/// arrive (the mark is dropped before the first field), and the body's
/// text as it came still starts with it.
#[test]
fn a_byte_order_mark_does_not_hide_the_first_event() {
    let (url, t) = server(|mut s| {
        let body = "\u{feff}data: first\n\ndata: second\n\n";
        s.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
    });
    let code = format!(
        r#"
local events = ui.state("events", "")
local chunks = ui.state("chunks", "")
return ui.column {{
    ui.text {{ id = "events", function() return events:get() end }},
    ui.text {{ id = "chunks", function() return chunks:get() end }},
    ui.button {{ id = "start", w = 80, h = 24, "start", on_click = function()
        ui.fetch("{url}", {{
            headers = {{ ["User-Agent"] = "zgx-release-check" }},
            on_event = function(e) events:set(events:peek() .. e.data .. ";") end,
            on_chunk = function(s) chunks:set(chunks:peek() .. s) end,
        }})
    end }},
}}
"#
    );
    let mut a = TestApp::new(move |r| {
        let host = ScriptHost::inline(r, "fetch_bom", code);
        zgx_run::install_fetcher(&host);
        host
    });
    a.click("start");
    assert!(
        a.wait_until(|a| a.text("events") == "first;second;"),
        "{}",
        a.text("events")
    );
    t.join().unwrap();
    assert!(a.text("chunks").starts_with('\u{feff}'));
}
/// Two fetches from one host at once, on connections of the same pool:
/// cancelling the first closes its connection alone, and the second
/// reads on to its end.
#[test]
fn a_cancel_closes_only_its_own_connection() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let (closed_tx, closed) = mpsc::channel();
    let release = Arc::new(AtomicBool::new(false));
    let releasing = release.clone();
    let server = thread::spawn(move || {
        let mut handlers = Vec::new();
        for _ in 0..2 {
            let (mut s, _) = l.accept().unwrap();
            let (closed_tx, releasing) = (closed_tx.clone(), releasing.clone());
            handlers.push(thread::spawn(move || {
                let mut head = Vec::new();
                let mut b = [0; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = s.read(&mut b).unwrap();
                    head.extend_from_slice(&b[..n]);
                }
                let first = String::from_utf8_lossy(&head).contains("GET /first");
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\na")
                    .unwrap();
                if first {
                    // Held open: the cancel must close it.
                    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                    let r = s.read(&mut b);
                    closed_tx.send(matches!(r, Ok(0))).unwrap();
                } else {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !releasing.load(Ordering::SeqCst) && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(2));
                    }
                    s.write_all(b"b").unwrap();
                }
            }));
        }
        for h in handlers {
            h.join().unwrap();
        }
    });
    let code = format!(
        r#"
local first, second = ui.state("first", ""), ui.state("second", "")
local p
return ui.column {{
    ui.text {{ id = "first", function() return first:get() end }},
    ui.text {{ id = "second", function() return second:get() end }},
    ui.button {{ id = "start", w = 80, h = 24, "start", on_click = function()
        local headers = {{ ["User-Agent"] = "zgx-release-check" }}
        p = ui.fetch("{base}/first", {{ headers = headers,
            on_chunk = function(s) first:set(first:peek() .. s) end }})
        ui.fetch("{base}/second", {{ headers = headers,
            on_chunk = function(s) second:set(second:peek() .. s) end }})
            :after(function(r) second:set(second:peek() .. ":" .. r.status) end,
                function(e) second:set("err:" .. e) end)
    end }},
    ui.button {{ id = "cancel", w = 80, h = 24, "cancel", on_click = function() p:cancel() end }},
}}
"#
    );
    let mut a = TestApp::new(move |r| {
        let host = ScriptHost::inline(r, "fetch_two", code);
        zgx_run::install_fetcher(&host);
        host
    });
    a.click("start");
    assert!(a.wait_until(|a| a.text("first") == "a" && a.text("second") == "a"));
    a.click("cancel");
    assert!(
        closed.recv_timeout(Duration::from_secs(4)).unwrap(),
        "the cancelled fetch's connection closed"
    );
    release.store(true, Ordering::SeqCst);
    assert!(
        a.wait_until(|a| a.text("second") == "ab:200"),
        "{}",
        a.text("second")
    );
    server.join().unwrap();
}
