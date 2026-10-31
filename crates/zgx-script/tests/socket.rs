//! A connection a script keeps open (`ui.socket`): messages each way, sent
//! in order once it is open, opened again after a growing wait when it
//! ends, closed for good by the script, or out of attempts; and a promise
//! that settles after a wait (`ui.sleep`).
mod common;
use common::Harness;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::Poll,
    time::{Duration, Instant},
};
use zgx_script::{Message, ScriptHost, Socket, SocketRequest};

const APP: &str = r#"
local log = ui.state("log", "")
local room = ui.state("room", "a")
local socket = ui.socket(function() return `ws://chat.test/{room:get()}` end, {
    reconnect = { min = 5, max = 20, jitter = 0, attempts = 3 },
    json = true,
    max_buffered = 64,
    on_message = function(m, binary)
        -- A message decoded from JSON is the script's own, to change.
        if not binary then
            m.seen = true
        end
        local text = if binary then `bytes {#m}` else m.text
        log:set(`{log:peek()}{text};`)
    end,
    on_close = function(err) log:set(`{log:peek()}closed {if err then err.kind else "-"};`) end,
})
-- Sent before it opens: queued, then sent in order.
socket.send({ text = "first" })
return ui.column {
    ui.text { id = "status", function()
        return `{socket.status:get()} {socket.attempt:get()} {socket.buffered:get()}`
    end },
    ui.text { id = "log", "{log}" },
    ui.text { id = "error", function()
        local err = socket.error
        return if err then `{err.kind}: {err}` else "none"
    end },
    ui.button { id = "send", w = 40, h = 20, "s", on_click = function()
        log:set(`{log:peek()}sent {socket.send({ text = "hi" })};`)
    end },
    ui.button { id = "bytes", w = 40, h = 20, "b", on_click = function()
        socket.send_bytes("\0\1\2")
    end },
    ui.button { id = "flood", w = 40, h = 20, "f", on_click = function()
        local taken = 0
        while socket.send(string.rep("x", 10)) do
            taken += 1
        end
        log:set(`{log:peek()}took {taken};`)
    end },
    ui.button { id = "close", w = 40, h = 20, "c", on_click = socket.close },
    ui.button { id = "reconnect", w = 40, h = 20, "r", on_click = socket.reconnect },
    ui.button { id = "room", w = 40, h = 20, "m", on_click = function() room:set("b") end },
    ui.button { id = "sleep", w = 40, h = 20, "z", on_click = function()
        ui.task(function()
            ui.sleep(20):await()
            log:set(`{log:peek()}slept;`)
        end)
    end },
}
"#;

/// A server in memory: it echoes each text message back, tells binary ones
/// by their length, and is told to refuse, hold or drop connections.
#[derive(Default)]
struct Server {
    /// Every URL asked for, in order.
    urls: RefCell<Vec<String>>,
    /// Attempts still to refuse.
    refuse: Cell<u32>,
    /// While held, it takes nothing the script sends.
    held: Cell<bool>,
    /// What it received.
    received: RefCell<Vec<Message>>,
    /// Set: each open connection ends so (`Ok`: closed; `Err`: failed).
    drop_all: RefCell<Option<Result<(), String>>>,
    open: Cell<u32>,
    /// The open connection's task, woken by what the test changes.
    waker: RefCell<Option<std::task::Waker>>,
}
impl Server {
    fn poke(&self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    fn drop_all(&self, end: Result<(), String>) {
        *self.drop_all.borrow_mut() = Some(end);
        self.poke();
    }
    fn hold(&self, held: bool) {
        self.held.set(held);
        self.poke();
    }
}

fn connector(
    server: Rc<Server>,
) -> impl Fn(SocketRequest, Socket) -> std::pin::Pin<Box<dyn Future<Output = Result<(), String>>>> {
    move |request, socket| {
        let server = server.clone();
        Box::pin(async move {
            server.urls.borrow_mut().push(request.url.clone());
            zgx::timer::sleep(Duration::from_millis(1)).await;
            if server.refuse.get() > 0 {
                server.refuse.set(server.refuse.get() - 1);
                return Err("connection refused".into());
            }
            socket.opened();
            server.open.set(server.open.get() + 1);
            let generation = server.open.get();
            let mut next = Box::pin(socket.next());
            let result = std::future::poll_fn(|cx| {
                loop {
                    if let Some(end) = server.drop_all.borrow().clone()
                        && server.open.get() >= generation
                    {
                        return Poll::Ready(Some(end));
                    }
                    *server.waker.borrow_mut() = Some(cx.waker().clone());
                    if server.held.get() {
                        return Poll::Pending;
                    }
                    match next.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(None) => return Poll::Ready(None),
                        Poll::Ready(Some(message)) => {
                            server.received.borrow_mut().push(message.clone());
                            // JSON echoed back; other text only kept.
                            match message {
                                Message::Text(text) if text.starts_with('{') => {
                                    socket.receive(Message::Text(text))
                                }
                                Message::Text(_) => {}
                                Message::Binary(bytes) => socket.receive(Message::Binary(bytes)),
                            }
                            next = Box::pin(socket.next());
                        }
                    }
                }
            })
            .await;
            match result {
                Some(end) => {
                    server.drop_all.borrow_mut().take();
                    end
                }
                None => Ok(()),
            }
        })
    }
}

fn app(server: &Rc<Server>) -> Harness {
    let server = server.clone();
    Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", APP);
        host.socket_connector(connector(server));
        host
    })
}

/// Settle until `id` reads `text`.
fn wait_for(h: &mut Harness, id: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        if h.text(id) == text {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("`{id}` reads {:?}, not {text:?}", h.text(id));
}

#[test]
fn messages_go_each_way_once_it_opens() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    assert!(
        h.text("status").starts_with("connecting"),
        "{}",
        h.text("status")
    );
    wait_for(&mut h, "status", "open 0 0");
    wait_for(&mut h, "log", "first;");
    h.click("send");
    wait_for(&mut h, "log", "first;sent true;hi;");
    h.click("bytes");
    wait_for(&mut h, "log", "first;sent true;hi;bytes 3;");
    assert_eq!(
        server.received.borrow().as_slice(),
        [
            Message::Text(r#"{"text":"first"}"#.into()),
            Message::Text(r#"{"text":"hi"}"#.into()),
            Message::Binary(vec![0, 1, 2]),
        ]
    );
    assert_eq!(h.error(), None);
}

#[test]
fn a_dropped_connection_opens_again_after_a_wait() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    wait_for(&mut h, "status", "open 0 0");
    server.drop_all(Err("connection reset".into()));
    wait_for(&mut h, "error", "network: connection reset");
    assert!(
        h.text("log").contains("closed network;"),
        "{}",
        h.text("log")
    );
    // Open again: its attempts from none, its error gone.
    wait_for(&mut h, "status", "open 0 0");
    assert_eq!(h.text("error"), "none");
    assert_eq!(server.urls.borrow().len(), 2);
    // What it sends now reaches the new connection.
    h.click("send");
    wait_for(&mut h, "log", "first;closed network;sent true;hi;");
}

#[test]
fn out_of_attempts_it_closes() {
    let server = Rc::new(Server::default());
    server.refuse.set(10);
    let mut h = app(&server);
    // The first, then three more: 5, 10 and 20 ms apart.
    wait_for(&mut h, "status", "closed 3 16");
    assert_eq!(h.text("error"), "network: connection refused");
    assert_eq!(server.urls.borrow().len(), 4);
    // Closed for good: sending is refused, and it waits for no more.
    h.click("send");
    h.frames(3);
    assert!(h.text("log").ends_with("sent false;"), "{}", h.text("log"));
    std::thread::sleep(Duration::from_millis(40));
    h.settle();
    assert_eq!(server.urls.borrow().len(), 4);
    // `reconnect()` opens it again.
    server.refuse.set(0);
    h.click("reconnect");
    wait_for(&mut h, "status", "open 0 0");
}

#[test]
fn the_script_closes_it_and_opens_it_again() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    wait_for(&mut h, "status", "open 0 0");
    h.click("close");
    h.ui.prepare_frame();
    assert_eq!(h.text("status"), "closed 0 0");
    std::thread::sleep(Duration::from_millis(30));
    h.settle();
    assert_eq!(
        server.urls.borrow().len(),
        1,
        "closed by the script: not opened again"
    );
    h.click("reconnect");
    wait_for(&mut h, "status", "open 0 0");
    assert_eq!(server.urls.borrow().len(), 2);
}

#[test]
fn a_state_its_url_reads_opens_it_again() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    wait_for(&mut h, "status", "open 0 0");
    h.click("room");
    wait_for(&mut h, "status", "open 0 0");
    assert_eq!(
        server.urls.borrow().as_slice(),
        ["ws://chat.test/a", "ws://chat.test/b"]
    );
}

#[test]
fn sending_stops_at_its_buffer_while_the_peer_takes_nothing() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    wait_for(&mut h, "status", "open 0 0");
    wait_for(&mut h, "log", "first;");
    server.hold(true);
    h.click("flood");
    h.ui.prepare_frame();
    // 64 bytes at most: six 10-byte messages ("xxxxxxxxxx") queue.
    assert!(h.text("log").ends_with("took 6;"), "{}", h.text("log"));
    assert_eq!(h.text("status"), "open 0 60");
    server.hold(false);
    wait_for(&mut h, "status", "open 0 0");
    assert_eq!(server.received.borrow().len(), 7);
}

#[test]
fn a_task_sleeps() {
    let server = Rc::new(Server::default());
    let mut h = app(&server);
    wait_for(&mut h, "log", "first;");
    let began = Instant::now();
    h.click("sleep");
    wait_for(&mut h, "log", "first;slept;");
    assert!(began.elapsed() >= Duration::from_millis(20));
}
