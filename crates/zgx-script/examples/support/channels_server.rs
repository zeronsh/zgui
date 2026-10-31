//! The channels example's server, in memory: a team's chat over a
//! WebSocket (`ScriptHost::socket_connector`) speaking JSON, its history a
//! page at a time and its uploads over HTTP (`streaming_fetcher`).
//!
//! The socket's messages, each a JSON object with a `type`:
//! - to the app: `welcome` (`me`, `channels`, `users` with `online`, and
//!   each channel's latest page: `history[channel] = { items, before }`),
//!   `message` (`message`), `ack` (`id`, the local id, and `message`, as
//!   stored), `refused` (`id`, `reason`), `presence` (`user`, `online`),
//!   `typing` (`user`, `channel`);
//! - from it: `send` (`id`, `channel`, `text`, `files`), `typing`
//!   (`channel`).
//!
//! `GET https://chat.test/history?channel=c&before=n` answers the page of
//! messages before `n` (`{ items, before }`, `before` nil at the start);
//! `POST https://chat.test/upload` takes a multipart form, telling its
//! progress as it reads it, and answers `{ url }`.
//!
//! `say` posts as someone else, `fail(n)` refuses the next `n` sends,
//! `drop_all()` drops every connection (the app reconnects), and
//! `set_online` changes someone's presence: the example's own pacing and
//! the tests use them.
// Shared with its tests, which use what the example does not.
#![allow(dead_code)]
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    task::{Poll, Waker},
    time::Duration,
};
use zgx_script::{Message, Request, Response, ScriptHost, Socket};

const PAGE: usize = 30;

struct Connection {
    socket: Socket,
    /// Told to end (`drop_all`): its loop wakes and returns.
    dropped: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

/// A send to answer, after the server's pace.
struct Send {
    local: Json,
    channel: String,
    text: String,
    files: Vec<Json>,
    refused: bool,
}

#[derive(Default)]
struct Inner {
    me: String,
    channels: Vec<(String, String)>,
    users: RefCell<Vec<(String, String, bool)>>,
    /// Each channel's messages, oldest first.
    history: RefCell<HashMap<String, Vec<Json>>>,
    connections: RefCell<Vec<Rc<Connection>>>,
    next: Cell<u64>,
    refuse: Cell<u32>,
    connects: Cell<u32>,
    pace: Cell<Duration>,
}

#[derive(Clone)]
pub struct ChannelsServer(Rc<Inner>);

const LINES: [&str; 12] = [
    "Morning! Anyone up for a review of the **scroll** changes?",
    "I pushed a fix for the flaky test, `cargo test -p zgx` is green again.",
    "Lunch at 12:30?",
    "The new icons look *great*.",
    "Can someone check the release notes before I post them?",
    "Heads up: the build machine restarts at five.",
    "👍",
    "I'm out tomorrow, ping me on email if it's urgent.",
    "Does anyone remember why we pinned that crate?",
    "Measured it: about 4% faster on the M1, flat on the M4.",
    "That's a wrap for today, see you all tomorrow.",
    "Moved the design doc to the shared folder.",
];

impl ChannelsServer {
    pub fn new() -> Self {
        let inner = Inner {
            me: "u-you".into(),
            channels: vec![
                ("general".into(), "general".into()),
                ("design".into(), "design".into()),
                ("releases".into(), "releases".into()),
            ],
            users: RefCell::new(vec![
                ("u-you".into(), "You".into(), true),
                ("u-ada".into(), "Ada".into(), true),
                ("u-grace".into(), "Grace".into(), true),
                ("u-alan".into(), "Alan".into(), false),
                ("u-susan".into(), "Susan".into(), true),
            ]),
            pace: Cell::new(Duration::from_millis(300)),
            ..Inner::default()
        };
        let server = Self(Rc::new(inner));
        // A long #general, a page at a time; a few in the others.
        let others = ["u-ada", "u-grace", "u-alan", "u-susan"];
        for n in 0..90_usize {
            let from = others[n % others.len()];
            server.store("general", from, LINES[n % LINES.len()], Vec::new());
        }
        for (n, line) in LINES.iter().take(5).enumerate() {
            server.store("design", others[(n + 1) % others.len()], line, Vec::new());
        }
        server.store("releases", "u-grace", "0.1.6 is out.", Vec::new());
        server
    }

    /// How long a send's acknowledgement and an upload's steps take.
    pub fn pace(&self, pace: Duration) {
        self.0.pace.set(pace);
    }

    fn store(&self, channel: &str, from: &str, text: &str, files: Vec<Json>) -> Json {
        let id = self.0.next.get() + 1;
        self.0.next.set(id);
        let message = json!({
            "id": id,
            "channel": channel,
            "from": from,
            "text": text,
            "at": 1_761_700_000 + id * 37,
            "files": files,
        });
        self.0
            .history
            .borrow_mut()
            .entry(channel.into())
            .or_default()
            .push(message.clone());
        message
    }

    fn page(&self, channel: &str, before: Option<u64>) -> Json {
        let history = self.0.history.borrow();
        let all = history.get(channel).map(Vec::as_slice).unwrap_or_default();
        let end = match before {
            Some(before) => all
                .iter()
                .position(|m| m["id"].as_u64() == Some(before))
                .unwrap_or(0),
            None => all.len(),
        };
        let start = end.saturating_sub(PAGE);
        json!({
            "items": &all[start..end],
            "before": if start > 0 { all[start]["id"].clone() } else { Json::Null },
        })
    }

    fn broadcast(&self, message: Json) {
        let text = message.to_string();
        for connection in self.0.connections.borrow().iter() {
            connection.socket.receive(Message::Text(text.clone()));
        }
    }

    /// `from` says `text` in `channel`, to everyone connected.
    pub fn say(&self, from: &str, channel: &str, text: &str) {
        let message = self.store(channel, from, text, Vec::new());
        self.broadcast(json!({ "type": "message", "message": message }));
    }
    /// `user` starts typing in `channel` (the app shows it a few seconds).
    pub fn typing(&self, user: &str, channel: &str) {
        self.broadcast(json!({ "type": "typing", "user": user, "channel": channel }));
    }
    /// The next `n` sends are refused.
    pub fn fail(&self, n: u32) {
        self.0.refuse.set(n);
    }
    pub fn set_online(&self, user: &str, online: bool) {
        for entry in self.0.users.borrow_mut().iter_mut() {
            if entry.0 == user {
                entry.2 = online;
            }
        }
        self.broadcast(json!({ "type": "presence", "user": user, "online": online }));
    }
    /// End every connection, as a network change would.
    pub fn drop_all(&self) {
        for connection in self.0.connections.borrow_mut().drain(..) {
            connection.dropped.set(true);
            if let Some(waker) = connection.waker.take() {
                waker.wake();
            }
        }
    }
    /// How many times a socket has connected.
    pub fn connects(&self) -> u32 {
        self.0.connects.get()
    }
    /// A line someone might say.
    pub fn line(n: usize) -> &'static str {
        LINES[n % LINES.len()]
    }

    fn welcome(&self) -> Json {
        let channels: Vec<Json> = (self.0.channels.iter())
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect();
        let users: Vec<Json> = (self.0.users.borrow().iter())
            .map(|(id, name, online)| json!({ "id": id, "name": name, "online": online }))
            .collect();
        let history: serde_json::Map<String, Json> = (self.0.channels.iter())
            .map(|(id, _)| (id.clone(), self.page(id, None)))
            .collect();
        json!({
            "type": "welcome",
            "me": self.0.me,
            "channels": channels,
            "users": users,
            "history": history,
        })
    }

    /// What the app sent: a send to answer once the server's pace has
    /// passed.
    fn heard(&self, text: &str) -> Option<Send> {
        let asked = serde_json::from_str::<Json>(text).ok()?;
        if asked["type"].as_str() != Some("send") {
            return None;
        }
        let refuse = self.0.refuse.get();
        if refuse > 0 {
            self.0.refuse.set(refuse - 1);
        }
        Some(Send {
            local: asked["id"].clone(),
            channel: asked["channel"].as_str().unwrap_or("general").to_string(),
            text: asked["text"].as_str().unwrap_or_default().to_string(),
            files: asked["files"].as_array().cloned().unwrap_or_default(),
            refused: refuse > 0,
        })
    }

    /// Answer `send` on `connection`: refused, or stored, acknowledged to
    /// its sender and told to the rest as news.
    fn answer(&self, connection: &Rc<Connection>, send: Send) {
        if send.refused {
            let refused =
                json!({ "type": "refused", "id": send.local, "reason": "the server is busy" });
            connection
                .socket
                .receive(Message::Text(refused.to_string()));
            return;
        }
        let me = self.0.me.clone();
        let message = self.store(&send.channel, &me, &send.text, send.files);
        connection.socket.receive(Message::Text(
            json!({ "type": "ack", "id": send.local, "message": message }).to_string(),
        ));
        let news = json!({ "type": "message", "message": message }).to_string();
        for other in self.0.connections.borrow().iter() {
            if !Rc::ptr_eq(other, connection) {
                other.socket.receive(Message::Text(news.clone()));
            }
        }
    }

    /// Serve `host`'s sockets and fetches.
    pub fn serve(&self, host: &ScriptHost) {
        let server = self.clone();
        host.socket_connector(move |_request, socket| {
            let server = server.clone();
            async move {
                server.0.connects.set(server.0.connects.get() + 1);
                zgx::timer::sleep(Duration::from_millis(20)).await;
                let connection = Rc::new(Connection {
                    socket: socket.clone(),
                    dropped: Cell::new(false),
                    waker: RefCell::default(),
                });
                server.0.connections.borrow_mut().push(connection.clone());
                socket.opened();
                socket.receive(Message::Text(server.welcome().to_string()));
                let mut next = Box::pin(socket.next());
                // Sends waiting for their answer, the first one's wait.
                let mut sends: std::collections::VecDeque<Send> = Default::default();
                let mut wait: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>> =
                    None;
                let ended = std::future::poll_fn(|cx| {
                    loop {
                        if connection.dropped.get() {
                            return Poll::Ready(Err("the connection dropped".to_string()));
                        }
                        *connection.waker.borrow_mut() = Some(cx.waker().clone());
                        if wait.is_none() && !sends.is_empty() {
                            wait = Some(Box::pin(zgx::timer::sleep(server.0.pace.get())));
                        }
                        if let Some(waiting) = &mut wait
                            && waiting.as_mut().poll(cx).is_ready()
                        {
                            wait = None;
                            if let Some(send) = sends.pop_front() {
                                server.answer(&connection, send);
                            }
                            continue;
                        }
                        match next.as_mut().poll(cx) {
                            Poll::Pending => return Poll::Pending,
                            Poll::Ready(None) => return Poll::Ready(Ok(())),
                            Poll::Ready(Some(Message::Text(text))) => {
                                sends.extend(server.heard(&text));
                                next = Box::pin(socket.next());
                            }
                            Poll::Ready(Some(Message::Binary(_))) => {
                                next = Box::pin(socket.next());
                            }
                        }
                    }
                })
                .await;
                server
                    .0
                    .connections
                    .borrow_mut()
                    .retain(|c| !Rc::ptr_eq(c, &connection));
                ended
            }
        });
        let server = self.clone();
        host.streaming_fetcher(move |request: Request, body| {
            let server = server.clone();
            async move {
                let pace = server.0.pace.get();
                let json = |value: Json| Response {
                    status: 200,
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: value.to_string().into_bytes(),
                };
                if request.url.starts_with("https://chat.test/history") {
                    zgx::timer::sleep(pace).await;
                    let query = |name: &str| {
                        request.url.split(['?', '&']).find_map(|pair| {
                            let (key, value) = pair.split_once('=')?;
                            (key == name).then(|| value.to_string())
                        })
                    };
                    let channel = query("channel").unwrap_or_else(|| "general".into());
                    let before = query("before").and_then(|n| n.parse().ok());
                    return Ok(json(server.page(&channel, before)));
                }
                if request.url == "https://chat.test/upload" {
                    let total = request.body.as_ref().map_or(0, Vec::len) as u64;
                    for step in 1..=4 {
                        zgx::timer::sleep(pace).await;
                        body.sent(total * step / 4, total);
                    }
                    let id = server.0.next.get() + 1;
                    server.0.next.set(id);
                    return Ok(json(
                        json!({ "url": format!("https://chat.test/files/{id}") }),
                    ));
                }
                Ok(Response {
                    status: 404,
                    ..Response::default()
                })
            }
        });
    }
}
