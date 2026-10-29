//! `ui.socket(url, opts)` and `ui.sleep(ms)`, made the first time a script
//! names either:
//! - `ui.socket` keeps a connection open to `url` (a chat's, a feed's)
//!   through the host's connector (`ScriptHost::socket_connector`), as
//!   React's `useWebSocket`: its `status` a state (`"connecting"`,
//!   `"open"`, `"reconnecting"`, `"closed"`), each message to `on_message`,
//!   and `send(v)` (text, or a table as JSON) and `send_bytes(s)` queued
//!   while it is not open and sent in order once it is. A connection that
//!   ends or fails opens again after a wait growing with each attempt
//!   (`reconnect`), until the script closes it or its owner goes. `url` may
//!   be a function: a state it reads changing (a room, a token) opens it
//!   again at once.
//! - `ui.sleep(ms)` is a promise settled after `ms`, to await in a task (a
//!   retry's wait).
use crate::{
    calls::{PromiseUd, pending, settle_with},
    errors::{Failure, Kind, UiError},
    node::env,
    value::{State, Value},
};
use mlua::{Function, Lua, MetaMethod, MultiValue, Table};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
    time::Duration,
};
use zgx::reactive::{Runtime, Signal};

/// A connection a script opens (`ui.socket`), for the host's connector.
#[derive(Clone, Debug, PartialEq)]
pub struct SocketRequest {
    /// The URL (`ws://` or `wss://`, or whatever the host's connector takes).
    pub url: String,
    /// The subprotocols the script asks for, in order (`protocols`).
    pub protocols: Vec<String>,
}

/// One message, either way.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A text message: a script's string, or a table it sent as JSON.
    Text(String),
    /// A binary message (`send_bytes`), a script's string of bytes.
    Binary(Vec<u8>),
}
impl Message {
    fn len(&self) -> usize {
        match self {
            Message::Text(text) => text.len(),
            Message::Binary(bytes) => bytes.len(),
        }
    }
}

/// The script's end of one connection, handed to the host's connector: the
/// transport tells it the connection is open (`opened`), hands it each
/// message (`receive`) and takes what the script sends (`next`), one at a
/// time, as it can write them: what it has not taken is the socket's
/// `buffered`. Once `next` answers `None` the script has closed it (or
/// reloaded, or its owner has gone): the transport closes the connection,
/// and its future is dropped at its next await. It lives on the main
/// thread: a transport on another sends here.
#[derive(Clone)]
pub struct Socket(Rc<Link>);

/// One attempt at a connection: the transport's `Socket`, and the task
/// running its future.
struct Link {
    conn: Weak<Conn>,
    /// The script still wants this connection.
    live: Cell<bool>,
    /// The transport waiting for something to send.
    sender: RefCell<Option<Waker>>,
    /// The task running the transport, woken to drop it once `live` is
    /// false.
    task: RefCell<Option<Waker>>,
}
impl Link {
    fn end(&self) {
        self.live.set(false);
        for waker in [self.sender.take(), self.task.take()].into_iter().flatten() {
            waker.wake();
        }
    }
    /// The connection this link is, while the script wants it.
    fn conn(&self) -> Option<Rc<Conn>> {
        self.live.get().then(|| self.conn.upgrade()).flatten()
    }
}

impl Socket {
    /// The connection is open: the socket's `status` is `"open"`, its
    /// attempts start again from none, `on_open` runs and what was queued
    /// is sent.
    pub fn opened(&self) {
        if let Some(conn) = self.0.conn() {
            conn.opened();
        }
    }
    /// A message arrived, for the script's `on_message`.
    pub fn receive(&self, message: Message) {
        if let Some(conn) = self.0.conn() {
            conn.receive(message);
        }
    }
    /// Whether the script still wants the connection.
    pub fn is_open(&self) -> bool {
        self.0.live.get()
    }
    /// The next message the script sent, once there is one; `None` once it
    /// has closed the connection.
    pub fn next(&self) -> impl Future<Output = Option<Message>> + use<> {
        let link = self.0.clone();
        std::future::poll_fn(move |cx| link.poll_next(cx))
    }
}
impl Link {
    fn poll_next(&self, cx: &mut Context<'_>) -> Poll<Option<Message>> {
        let Some(conn) = self.conn() else {
            return Poll::Ready(None);
        };
        let next = conn.queue.borrow_mut().pop_front();
        match next {
            Some(message) => {
                conn.add_buffered(-(message.len() as f64));
                Poll::Ready(Some(message))
            }
            None => {
                *self.sender.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

pub(crate) type Connect = Pin<Box<dyn Future<Output = Result<(), String>>>>;
/// The host's transport (`ScriptHost::socket_connector`).
pub(crate) type Connector = Rc<dyn Fn(SocketRequest, Socket) -> Connect>;
pub(crate) type SharedConnector = Rc<RefCell<Option<Connector>>>;

/// How a socket waits before it opens again: `min` ms after the first
/// failure, `factor` times longer each attempt after, at most `max`, give
/// or take `jitter` of it, for at most `attempts`.
#[derive(Clone, Copy)]
struct Backoff {
    min: f64,
    max: f64,
    factor: f64,
    jitter: f64,
    attempts: f64,
}
impl Backoff {
    const DEFAULT: Backoff = Backoff {
        min: 500.,
        max: 30_000.,
        factor: 2.,
        jitter: 0.2,
        attempts: f64::INFINITY,
    };
    fn of(value: mlua::Value) -> mlua::Result<Option<Self>> {
        let table = match value {
            mlua::Value::Nil | mlua::Value::Boolean(true) => return Ok(Some(Self::DEFAULT)),
            mlua::Value::Boolean(false) => return Ok(None),
            mlua::Value::Table(table) => table,
            _ => {
                return Err(mlua::Error::runtime(
                    "ui.socket: `reconnect` is true, false or { min, max, factor, jitter, attempts }",
                ));
            }
        };
        let mut backoff = Self::DEFAULT;
        for (name, field) in [
            ("min", &mut backoff.min),
            ("max", &mut backoff.max),
            ("factor", &mut backoff.factor),
            ("jitter", &mut backoff.jitter),
            ("attempts", &mut backoff.attempts),
        ] {
            if let Some(n) = table.get::<Option<f64>>(name)? {
                if n.is_nan() || n < 0. {
                    return Err(mlua::Error::runtime(format!(
                        "ui.socket: `reconnect.{name}` is a number, 0 or more"
                    )));
                }
                *field = n;
            }
        }
        backoff.jitter = backoff.jitter.min(1.);
        Ok(Some(backoff))
    }
    /// The wait before attempt `attempt` (1 for the first again), in ms.
    fn wait(&self, attempt: f64, spread: f64) -> f64 {
        let grown = self.min * self.factor.powf(attempt - 1.);
        let wait = grown.min(self.max);
        (wait * (1. + self.jitter * spread)).max(0.)
    }
}

/// A number in [-1, 1), different each call: a wait's jitter.
fn spread() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(0);
    (hasher.finish() >> 11) as f64 / (1u64 << 52) as f64 - 1.
}

/// A socket while it lives: its states, its handlers and the connection
/// under way.
struct Conn {
    runtime: Runtime,
    vm: Weak<crate::Vm>,
    connector: SharedConnector,
    timers: Rc<crate::timers::Timers>,
    status: Signal<Value>,
    attempt: Signal<Value>,
    buffered: Signal<Value>,
    retry_in: Signal<Value>,
    error: Signal<Option<Rc<Failure>>>,
    url: RefCell<String>,
    protocols: Vec<String>,
    backoff: Option<Backoff>,
    json: bool,
    max_buffered: f64,
    on_message: Option<Function>,
    on_open: Option<Function>,
    on_close: Option<Function>,
    /// What the script sent that the transport has not taken.
    queue: RefCell<VecDeque<Message>>,
    link: RefCell<Option<Rc<Link>>>,
    /// The script closed it, or its owner went: it opens again only on
    /// `reconnect()`.
    closed: Cell<bool>,
    /// The wait before it opens again: counted on to drop one superseded.
    waiting: Cell<u64>,
}

impl Conn {
    /// Open the connection again from the start of the queue's sending.
    fn connect(self: &Rc<Self>) {
        self.drop_link();
        self.waiting.set(self.waiting.get().wrapping_add(1));
        self.retry_in.set(Value::Num(0.));
        let first = self.attempt.with_untracked(|a| *a == Value::Num(0.));
        self.status.set(Value::Str(
            if first { "connecting" } else { "reconnecting" }.into(),
        ));
        let Some(connect) = self.connector.borrow().clone() else {
            self.failed(Failure::new(
                Kind::Network,
                "ui.socket: the host has no connector (`ScriptHost::socket_connector`)",
            ));
            return;
        };
        let link = Rc::new(Link {
            conn: Rc::downgrade(self),
            live: Cell::new(true),
            sender: RefCell::default(),
            task: RefCell::default(),
        });
        *self.link.borrow_mut() = Some(link.clone());
        let request = SocketRequest {
            url: self.url.borrow().clone(),
            protocols: self.protocols.clone(),
        };
        let mut future = self
            .runtime
            .untracked(|| connect(request, Socket(link.clone())));
        let runtime = self.runtime.clone();
        self.timers.spawn(async move {
            // Run until it ends, or until the script stops wanting it: then
            // polled once more, to see `next` answer `None` and close.
            let ended = std::future::poll_fn(|cx| {
                let dead = !link.live.get();
                *link.task.borrow_mut() = Some(cx.waker().clone());
                match future.as_mut().poll(cx) {
                    Poll::Ready(result) => Poll::Ready(Some(result)),
                    Poll::Pending if dead => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            })
            .await;
            drop(future);
            if let (Some(result), Some(conn)) = (ended, link.conn()) {
                runtime.clone().batch(|| conn.ended(result));
            }
        });
    }

    fn drop_link(&self) {
        if let Some(link) = self.link.borrow_mut().take() {
            link.end();
        }
    }

    fn opened(&self) {
        self.runtime.clone().batch(|| {
            self.status.set(Value::Str("open".into()));
            self.attempt.set(Value::Num(0.));
            self.error.set(None);
        });
        if let Some(on_open) = &self.on_open {
            self.call(on_open, ());
        }
        self.wake_sender();
    }

    fn receive(&self, message: Message) {
        let Some(on_message) = &self.on_message else {
            return;
        };
        let Some(vm) = self.vm.upgrade() else { return };
        let lua = &vm.lua;
        let args = match message {
            Message::Binary(bytes) => lua
                .create_string(&bytes)
                .map(|s| (mlua::Value::String(s), true)),
            Message::Text(text) if self.json => match Value::from_json(&text) {
                Some(value) => mlua::IntoLua::into_lua(value, lua).map(|v| (v, false)),
                None => {
                    vm.report(mlua::Error::runtime(format!(
                        "ui.socket: a message is not JSON: {}",
                        text.chars().take(80).collect::<String>()
                    )));
                    return;
                }
            },
            Message::Text(text) => lua
                .create_string(&text)
                .map(|s| (mlua::Value::String(s), false)),
        };
        match args {
            Ok(args) => self.call(on_message, args),
            Err(e) => vm.report(e),
        }
    }

    /// The connection ended (`Ok`) or failed: open it again after a wait,
    /// or, out of attempts, close.
    fn ended(self: &Rc<Self>, result: Result<(), String>) {
        self.drop_link();
        let failure = result
            .err()
            .map(|message| Failure::new(Kind::Network, message));
        let attempt = self.attempt.with_untracked(|a| match a {
            Value::Num(n) => *n,
            _ => 0.,
        }) + 1.;
        match self.backoff {
            Some(backoff) if attempt <= backoff.attempts => {
                let wait = backoff.wait(attempt, spread());
                self.attempt.set(Value::Num(attempt));
                self.error.set(failure.map(Rc::new));
                self.status.set(Value::Str("reconnecting".into()));
                self.retry_in.set(Value::Num(wait.round()));
                let mine = self.waiting.get().wrapping_add(1);
                self.waiting.set(mine);
                let conn = Rc::downgrade(self);
                let period = Duration::from_secs_f64(
                    (wait / 1000.).min(zgx::timer::MAX_SLEEP.as_secs_f64()),
                );
                self.timers.spawn(async move {
                    zgx::timer::sleep(period).await;
                    if let Some(conn) = conn.upgrade()
                        && conn.waiting.get() == mine
                        && !conn.closed.get()
                    {
                        let runtime = conn.runtime.clone();
                        runtime.batch(|| conn.connect());
                    }
                });
            }
            _ => self.failed(failure.unwrap_or_else(|| Failure::new(Kind::Network, "closed"))),
        }
        self.closing();
    }

    /// Closed for good, by `failure`.
    fn failed(&self, failure: Failure) {
        self.drop_link();
        self.error.set(Some(Rc::new(failure)));
        self.status.set(Value::Str("closed".into()));
        self.retry_in.set(Value::Num(0.));
    }

    /// Tell `on_close` the connection ended, with its error (a `UiError`)
    /// if it failed.
    fn closing(&self) {
        let Some(on_close) = &self.on_close else {
            return;
        };
        let Some(vm) = self.vm.upgrade() else { return };
        let error = match self.error.with_untracked(Clone::clone) {
            Some(failure) => match vm.lua.create_userdata(UiError::of(failure)) {
                Ok(ud) => mlua::Value::UserData(ud),
                Err(e) => return vm.report(e),
            },
            None => mlua::Value::Nil,
        };
        self.call(on_close, error);
    }

    fn call(&self, f: &Function, args: impl mlua::IntoLuaMulti) {
        let Some(vm) = self.vm.upgrade() else { return };
        if let Err(e) = vm.call::<()>(f, args) {
            vm.report(e);
        }
    }

    fn buffered_now(&self) -> f64 {
        self.buffered.with_untracked(|n| match n {
            Value::Num(n) => *n,
            _ => 0.,
        })
    }
    fn add_buffered(&self, bytes: f64) {
        let n = (self.buffered_now() + bytes).max(0.);
        self.buffered.set(Value::Num(n));
    }

    fn wake_sender(&self) {
        let waker = match &*self.link.borrow() {
            Some(link) => link.sender.take(),
            None => None,
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Queue `message`, unless it would pass `max_buffered` or the socket
    /// is closed for good.
    fn send(&self, message: Message) -> bool {
        let gone = self.closed.get()
            || self
                .status
                .with_untracked(|s| matches!(s, Value::Str(s) if &**s == "closed"));
        let size = message.len() as f64;
        if gone || self.buffered_now() + size > self.max_buffered {
            return false;
        }
        self.queue.borrow_mut().push_back(message);
        self.add_buffered(size);
        if self
            .status
            .with_untracked(|s| matches!(s, Value::Str(s) if &**s == "open"))
        {
            self.wake_sender();
        }
        true
    }

    /// Closed by the script, or its owner gone: nothing opens it again but
    /// `reconnect()`.
    fn close(&self) {
        self.closed.set(true);
        self.waiting.set(self.waiting.get().wrapping_add(1));
        self.drop_link();
        self.status.set(Value::Str("closed".into()));
        self.retry_in.set(Value::Num(0.));
    }
}

/// Its connection closes with its owner.
struct Owned(#[allow(dead_code)] zgx::reactive::Effect, Rc<Conn>);
impl Drop for Owned {
    fn drop(&mut self) {
        self.1.closed.set(true);
        self.1.waiting.set(self.1.waiting.get().wrapping_add(1));
        self.1.drop_link();
        self.1.queue.borrow_mut().clear();
    }
}

pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    connector: Option<SharedConnector>,
) -> mlua::Result<()> {
    ui.set(
        "socket",
        lua.create_function(move |lua, (url, opts): (mlua::Value, Option<Table>)| {
            let Some(connector) = &connector else {
                return Err(mlua::Error::runtime(
                    "ui.socket: a plugin reaches the network only if the host grants it \
                     (`ScriptHost::grant(plugin, Capability::Fetch)`)",
                ));
            };
            socket(lua, url, opts, connector.clone())
        })?,
    )?;
    ui.set(
        "sleep",
        lua.create_function(|lua, ms: f64| {
            let period = crate::duration_ms("ui.sleep", ms)?;
            let promise = pending(lua, "sleep".into())?;
            settle_with(lua, &promise, Kind::Raised, async move {
                zgx::timer::sleep(period).await;
                Ok(Value::Nil)
            })?;
            Ok(PromiseUd(promise))
        })?,
    )
}

fn socket(
    lua: &Lua,
    url: mlua::Value,
    opts: Option<Table>,
    connector: SharedConnector,
) -> mlua::Result<Table> {
    let url = match url {
        mlua::Value::String(s) => Err(s.to_str()?.to_string()),
        mlua::Value::Function(f) => Ok(f),
        _ => {
            return Err(mlua::Error::runtime(
                "ui.socket: a URL, or a function returning one",
            ));
        }
    };
    let get = |name: &str| -> mlua::Result<mlua::Value> {
        match &opts {
            Some(opts) => opts.get(name),
            None => Ok(mlua::Value::Nil),
        }
    };
    let protocols = match get("protocols")? {
        mlua::Value::Nil => Vec::new(),
        mlua::Value::Table(t) => t.sequence_values::<String>().collect::<mlua::Result<_>>()?,
        _ => {
            return Err(mlua::Error::runtime(
                "ui.socket: `protocols` is a list of names",
            ));
        }
    };
    let max_buffered = match get("max_buffered")? {
        mlua::Value::Nil => 16. * 1024. * 1024.,
        mlua::Value::Integer(n) => n as f64,
        mlua::Value::Number(n) if n >= 0. => n,
        _ => {
            return Err(mlua::Error::runtime(
                "ui.socket: `max_buffered` is a number of bytes",
            ));
        }
    };
    let function = |name: &str| -> mlua::Result<Option<Function>> {
        match get(name)? {
            mlua::Value::Nil => Ok(None),
            mlua::Value::Function(f) => Ok(Some(f)),
            _ => Err(mlua::Error::runtime(format!(
                "ui.socket: `{name}` is a function"
            ))),
        }
    };
    let env = env(lua)?;
    let runtime = env.runtime.clone();
    let conn = Rc::new(Conn {
        runtime: runtime.clone(),
        vm: env.vm.clone(),
        connector,
        timers: env.timers.clone(),
        status: runtime.signal(Value::Str("connecting".into())),
        attempt: runtime.signal(Value::Num(0.)),
        buffered: runtime.signal(Value::Num(0.)),
        retry_in: runtime.signal(Value::Num(0.)),
        error: runtime.signal(None),
        url: RefCell::default(),
        protocols,
        backoff: Backoff::of(get("reconnect")?)?,
        json: matches!(get("json")?, mlua::Value::Boolean(true)),
        max_buffered,
        on_message: function("on_message")?,
        on_open: function("on_open")?,
        on_close: function("on_close")?,
        queue: RefCell::default(),
        link: RefCell::default(),
        closed: Cell::new(false),
        waiting: Cell::new(0),
    });
    // Its URL, keyed by what a function reads: a change opens it again, its
    // attempts from none.
    let effect = runtime.effect_now({
        let conn = Rc::downgrade(&conn);
        move || {
            let Some(conn) = conn.upgrade() else { return };
            let url = match &url {
                Err(url) => url.clone(),
                Ok(f) => {
                    let Some(vm) = conn.vm.upgrade() else { return };
                    match vm.call::<String>(f, ()) {
                        Ok(url) => url,
                        Err(e) => return vm.report(e),
                    }
                }
            };
            conn.runtime.untracked(|| {
                *conn.url.borrow_mut() = url;
                if !conn.closed.get() {
                    conn.attempt.set(Value::Num(0.));
                    conn.connect();
                }
            });
        }
    });
    crate::node::retain(&env, Box::new(Owned(effect, conn.clone())));
    drop(env);
    let out = lua.create_table()?;
    out.set("status", State::new(conn.status.clone()))?;
    out.set("attempt", State::new(conn.attempt.clone()))?;
    out.set("retry_in", State::new(conn.retry_in.clone()))?;
    out.set("buffered", State::new(conn.buffered.clone()))?;
    let sending = |binary: bool| {
        let conn = Rc::downgrade(&conn);
        lua.create_function(move |lua, value: mlua::Value| {
            let Some(conn) = conn.upgrade() else {
                return Ok(false);
            };
            let message = match value {
                mlua::Value::String(s) if binary => Message::Binary(s.as_bytes().to_vec()),
                mlua::Value::String(s) => Message::Text(s.to_str()?.to_string()),
                _ if binary => {
                    return Err(mlua::Error::runtime("ui.socket: send_bytes takes a string"));
                }
                other => {
                    let value: Value = mlua::FromLua::from_lua(other, lua)
                        .map_err(|e| mlua::Error::runtime(format!("ui.socket: send: {e}")))?;
                    Message::Text(value.to_json())
                }
            };
            Ok(conn.send(message))
        })
    };
    out.set("send", sending(false)?)?;
    out.set("send_bytes", sending(true)?)?;
    out.set("close", {
        let conn = Rc::downgrade(&conn);
        lua.create_function(move |_, _: MultiValue| {
            if let Some(conn) = conn.upgrade() {
                conn.close();
            }
            Ok(())
        })?
    })?;
    out.set("reconnect", {
        let conn = Rc::downgrade(&conn);
        lua.create_function(move |_, _: MultiValue| {
            if let Some(conn) = conn.upgrade() {
                conn.closed.set(false);
                let runtime = conn.runtime.clone();
                runtime.batch(|| conn.connect());
            }
            Ok(())
        })?
    })?;
    // `error`: why it last closed (a `UiError`, tracked), or nil; read
    // through `__index`, a state holding only plain values.
    let error = conn.error.clone();
    let meta = lua.create_table()?;
    meta.set(
        MetaMethod::Index.name(),
        lua.create_function(move |lua, (_, key): (Table, String)| match key.as_str() {
            "error" => match error.get() {
                Some(failure) => lua
                    .create_userdata(UiError::of(failure))
                    .map(mlua::Value::UserData),
                None => Ok(mlua::Value::Nil),
            },
            _ => Ok(mlua::Value::Nil),
        })?,
    )?;
    out.set_metatable(Some(meta))?;
    Ok(out)
}
