//! Host calls with results: `ui.call(name, arg)` runs a host action and
//! returns a promise of what it returns. An async action
//! (`ScriptHost::async_action`) is a future on the window's tasks, woken by
//! what it waits for (nothing polls); a plain one (`ScriptHost::action`)
//! settles at once with nil.
//!
//! A promise is read three ways: `p:after(on_value, on_error)`, its
//! reactive fields (`p.done`, `p.value`, `p.error`: an interface reading
//! them shows loading and errors with no handler), or `p:await()` inside
//! `ui.task(function() ... end)`, a coroutine resumed when the promise
//! settles. A rejection nothing handles is reported as a script error.
//! Everything a promise calls back runs on the script's budget; a script
//! that has reloaded since hears nothing.
use crate::{
    Vm,
    errors::{Failure, Kind, UiError},
    node::env,
    value::Value,
};
use mlua::{
    Function, IntoLuaMulti, Lua, MetaMethod, MultiValue, Table, Thread, UserData, UserDataFields,
    UserDataMethods,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
};
use zgx::reactive::{Runtime, Signal};

pub(crate) type Call = Pin<Box<dyn Future<Output = Result<Value, String>>>>;

/// An HTTP request a script makes (`ui.fetch`), for the host's fetcher.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The URL.
    pub url: String,
    /// "GET" unless the script says otherwise.
    pub method: String,
    /// Its headers, in order.
    pub headers: Vec<(String, String)>,
    /// Its body, if it has one.
    pub body: Option<Vec<u8>>,
}
/// What the host's fetcher answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    /// The HTTP status (200 for OK).
    pub status: u16,
    /// Its headers, in order.
    pub headers: Vec<(String, String)>,
    /// Its body.
    pub body: Vec<u8>,
}
/// Where a streaming fetcher (`ScriptHost::streaming_fetcher`) writes a
/// response's body as it arrives, before it answers with the status and
/// headers: a script's `on_chunk` and `on_event` hear each piece as it is
/// written. Clones write to the same response. It lives on the main thread:
/// a transport reading on another sends its pieces here.
#[derive(Clone)]
pub struct Body(Rc<BodyInner>);
/// What hears each piece of a body: the script's handlers.
type Sink = Box<dyn FnMut(&[u8])>;
struct BodyInner {
    /// What hears each piece. None: kept whole.
    sink: RefCell<Option<Sink>>,
    kept: RefCell<Vec<u8>>,
    open: Cell<bool>,
}
impl Body {
    fn new(sink: Option<Sink>) -> Self {
        Self(Rc::new(BodyInner {
            sink: RefCell::new(sink),
            kept: RefCell::default(),
            open: Cell::new(true),
        }))
    }
    /// A body kept whole, for the response's `body`.
    pub(crate) fn kept() -> Self {
        Self::new(None)
    }
    /// Write the next piece of the body. Nothing hears it once the request
    /// is cancelled (`is_open` is false).
    pub fn write(&self, bytes: &[u8]) {
        if !self.is_open() || bytes.is_empty() {
            return;
        }
        // Out while it runs: a handler cancelling the request closes it.
        let taken = self.0.sink.borrow_mut().take();
        match taken {
            Some(mut sink) => {
                sink(bytes);
                if self.is_open() {
                    *self.0.sink.borrow_mut() = Some(sink);
                }
            }
            None => self.0.kept.borrow_mut().extend_from_slice(bytes),
        }
    }
    /// Whether the script still listens: false once it cancelled the
    /// request (`p:cancel()`, or the task awaiting it cancelled) or
    /// reloaded. A transport stops reading then.
    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }
    fn close(&self) {
        self.0.open.set(false);
        self.0.sink.borrow_mut().take();
    }
    /// What was written, kept whole, then `rest` (what the answer held).
    pub(crate) fn whole(&self, rest: Vec<u8>) -> Vec<u8> {
        let mut kept = std::mem::take(&mut *self.0.kept.borrow_mut());
        if kept.is_empty() {
            return rest;
        }
        kept.extend_from_slice(&rest);
        kept
    }
}

pub(crate) type Fetch = Pin<Box<dyn Future<Output = Result<Response, String>>>>;
/// The host's transport (`ScriptHost::fetcher`): it owns the HTTP stack,
/// headers and credentials.
pub(crate) type Fetcher = Rc<dyn Fn(Request, Body) -> Fetch>;
pub(crate) type SharedFetcher = Rc<RefCell<Option<Fetcher>>>;
pub(crate) type AsyncAction = Rc<dyn Fn(Value) -> Call>;
/// The host's async actions, by name.
pub(crate) type AsyncActions = Rc<RefCell<HashMap<String, AsyncAction>>>;

#[derive(Clone, PartialEq)]
enum Settled {
    Pending,
    Value(Value),
    Error(Rc<Failure>),
}

/// Who hears a promise settle.
enum Waiter {
    After {
        vm: Weak<Vm>,
        on_value: Option<Function>,
        on_error: Option<Function>,
    },
    Task {
        vm: Weak<Vm>,
        thread: Thread,
        task: Rc<TaskState>,
    },
}

/// A task as its handle sees it (`ui.task`'s result): cancelled, it is not
/// resumed again, stopping at the await it is at.
#[derive(Default)]
pub(crate) struct TaskState {
    cancelled: Cell<bool>,
    /// The promise it waits for: cancelled, a fetch nothing else hears
    /// stops.
    awaiting: RefCell<Option<Weak<Promise>>>,
}
impl TaskState {
    pub(crate) fn cancel(&self) {
        self.cancelled.set(true);
        if let Some(promise) = self.awaiting.take().and_then(|p| p.upgrade()) {
            promise.abort_unheard();
        }
    }
}
/// `ui.task`'s handle: `cancel()`, and whether it has been (`cancelled`).
pub(crate) struct TaskUd(pub(crate) Rc<TaskState>);
impl UserData for TaskUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("cancelled", |_, t| Ok(t.0.cancelled.get()));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |_, t, ()| {
            t.0.cancel();
            Ok(())
        });
    }
}

pub(crate) struct Promise {
    /// What `ui.call` was asked, for errors.
    name: String,
    state: Signal<Settled>,
    waiters: RefCell<Vec<Waiter>>,
    /// Something reads or awaits it: a rejection is not reported.
    handled: Cell<bool>,
    /// The script that called, told of a rejection nothing handles.
    caller: Weak<Vm>,
    /// Stops the host's work, where it can stop (a fetch).
    abort: RefCell<Option<Box<dyn FnOnce()>>>,
}

#[derive(Clone)]
pub(crate) struct PromiseUd(pub(crate) Rc<Promise>);

impl Promise {
    fn settle(self: &Rc<Self>, result: Result<Value, Failure>) {
        // Cancelled first, a late answer changes nothing.
        if self.state.with_untracked(|s| *s != Settled::Pending) {
            return;
        }
        self.abort.borrow_mut().take();
        let settled = match result {
            Ok(value) => Settled::Value(value),
            Err(error) => Settled::Error(Rc::new(error)),
        };
        self.state.set(settled.clone());
        let waiters = std::mem::take(&mut *self.waiters.borrow_mut());
        if let Settled::Error(error) = &settled
            && waiters.is_empty()
            && !self.handled.get()
            && let Some(vm) = self.caller.upgrade()
        {
            vm.report(mlua::Error::runtime(format!(
                "ui.call(\"{}\"): {} (nothing handled it: `:after(_, on_error)`, `.error` or `:await()` in a pcall)",
                self.name, error.message
            )));
        }
        for waiter in waiters {
            hear(waiter, &settled);
        }
    }
}

impl Promise {
    /// Stop the host's work, if it can stop.
    fn abort(&self) {
        let abort = self.abort.borrow_mut().take();
        if let Some(abort) = abort {
            abort();
        }
    }
    /// A task awaiting it was cancelled: stop the host's work if nothing
    /// else hears the answer (no `after`, no task still running).
    fn abort_unheard(&self) {
        let heard = self.waiters.borrow().iter().any(|waiter| match waiter {
            Waiter::After { .. } => true,
            Waiter::Task { task, .. } => !task.cancelled.get(),
        });
        if !heard {
            self.abort();
        }
    }
}

/// Tell `waiter` what a promise settled as.
fn hear(waiter: Waiter, settled: &Settled) {
    match waiter {
        Waiter::After {
            vm,
            on_value,
            on_error,
        } => {
            let Some(vm) = vm.upgrade() else { return };
            let result = match settled {
                Settled::Value(value) => on_value.map(|f| vm.call::<()>(&f, value.clone())),
                Settled::Error(error) => on_error.map(|f| {
                    vm.lua
                        .create_userdata(UiError::of(error.clone()))
                        .and_then(|error| vm.call::<()>(&f, error))
                }),
                Settled::Pending => None,
            };
            if let Some(Err(error)) = result {
                vm.report(error);
            }
        }
        Waiter::Task { vm, thread, task } => {
            let Some(vm) = vm.upgrade() else { return };
            if task.cancelled.get() {
                return;
            }
            let resumed = match settled {
                Settled::Value(value) => (true, value.clone()).into_lua_multi(&vm.lua),
                Settled::Error(error) => {
                    (false, UiError::of(error.clone())).into_lua_multi(&vm.lua)
                }
                Settled::Pending => return,
            };
            match resumed {
                Ok(resumed) => drive(&vm, thread, resumed, &task),
                Err(error) => vm.report(error),
            }
        }
    }
}

/// Run task `thread` from where it is until it ends, awaits a promise
/// still pending, or `task` is cancelled.
pub(crate) fn drive(vm: &Rc<Vm>, thread: Thread, args: impl IntoLuaMulti, task: &Rc<TaskState>) {
    let mut args = Some(args.into_lua_multi(&vm.lua));
    loop {
        if task.cancelled.get() {
            return;
        }
        let input = match args.take() {
            Some(Ok(input)) => input,
            Some(Err(error)) => return vm.report(error),
            None => return,
        };
        let yielded = vm.guard(vm.budget, || thread.resume::<MultiValue>(input));
        let values = match yielded {
            Ok(values) => values,
            Err(error) => return vm.report(error),
        };
        if thread.status() != mlua::thread::ThreadStatus::Resumable {
            return;
        }
        let promise = values.front().and_then(|value| match value {
            mlua::Value::UserData(ud) => ud.borrow::<PromiseUd>().ok().map(|p| p.0.clone()),
            _ => None,
        });
        let Some(promise) = promise else {
            return vm.report(mlua::Error::runtime(
                "a ui.task yields only by awaiting a promise (`p:await()`)",
            ));
        };
        promise.handled.set(true);
        match promise.state.with_untracked(Clone::clone) {
            Settled::Pending => {
                *task.awaiting.borrow_mut() = Some(Rc::downgrade(&promise));
                promise.waiters.borrow_mut().push(Waiter::Task {
                    vm: Rc::downgrade(vm),
                    thread,
                    task: task.clone(),
                });
                return;
            }
            Settled::Value(value) => args = Some((true, value).into_lua_multi(&vm.lua)),
            Settled::Error(error) => {
                args = Some((false, UiError::of(error.clone())).into_lua_multi(&vm.lua))
            }
        }
    }
}

impl UserData for PromiseUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        // Tracked: what reads them follows the promise.
        fields.add_field_method_get("done", |_, p| {
            p.0.handled.set(true);
            Ok(p.0.state.with(|s| *s != Settled::Pending))
        });
        fields.add_field_method_get("value", |_, p| {
            p.0.handled.set(true);
            Ok(p.0.state.with(|s| match s {
                Settled::Value(value) => value.clone(),
                _ => Value::Nil,
            }))
        });
        fields.add_field_method_get("error", |_, p| {
            p.0.handled.set(true);
            Ok(p.0.state.with(|s| match s {
                Settled::Error(error) => Some(UiError::of(error.clone())),
                _ => None,
            }))
        });
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "after",
            |lua, p, (on_value, on_error): (Option<Function>, Option<Function>)| {
                p.0.handled.set(true);
                let vm = env(lua)?.vm.clone();
                let waiter = Waiter::After {
                    vm,
                    on_value,
                    on_error,
                };
                match p.0.state.with_untracked(Clone::clone) {
                    Settled::Pending => p.0.waiters.borrow_mut().push(waiter),
                    settled => hear(waiter, &settled),
                }
                Ok(())
            },
        );
        // Rejects it with "cancelled" (reported nowhere), stopping the
        // host's work where it can stop: a fetch. An action's goes on, its
        // answer unheard.
        methods.add_method("cancel", |lua, p, ()| {
            p.0.handled.set(true);
            p.0.abort();
            let runtime = env(lua)?.runtime.clone();
            batch(&runtime, || p.0.settle(Err(Failure::cancelled())));
            Ok(())
        });
        // `await` is Luau (it yields its coroutine): found through `__index`.
        methods.add_meta_function(MetaMethod::Index, |lua, (_, key): (mlua::Value, String)| {
            if key == "await" {
                lua.named_registry_value::<Function>(AWAIT)
            } else {
                Err(mlua::Error::runtime(format!("a promise has no `{key}`")))
            }
        });
    }
}

const AWAIT: &str = "zgx.await";
/// `p:await()`: the task's coroutine yields the promise and is resumed with
/// `(ok, value)` when it settles.
///
/// The same chunk (one made for every script) gives `ui.try(f, ...)`:
/// `f(...)`'s value and nil, or nil and what it raised as a `UiError`
/// (`ui.__error`, `errors::raised`: made the first time one is, so a script
/// that never fails makes none). Luau, so a task's await inside `f` yields
/// through it as through `pcall`. Kept short: it is compiled once a process.
const AWAIT_SOURCE: &str = r#"
local ui = ...
local yield, yieldable, pcall = coroutine.yield, coroutine.isyieldable, pcall
local function await(promise)
    if not yieldable() then
        error("await runs inside ui.task(function() ... end)", 2)
    end
    local ok, value = yield(promise)
    if not ok then
        error(value, 0)
    end
    return value
end
local function try(f, ...)
    local ok, value = pcall(f, ...)
    if ok then
        return value, nil
    end
    return nil, ui.__error(value)
end
return await, try
"#;

/// A new promise, pending, for what the script calling now asked.
pub(crate) fn pending(lua: &Lua, name: String) -> mlua::Result<Rc<Promise>> {
    let env = env(lua)?;
    Ok(Rc::new(Promise {
        name,
        state: env.runtime.signal(Settled::Pending),
        waiters: RefCell::default(),
        handled: Cell::new(false),
        caller: env.vm.clone(),
        abort: RefCell::default(),
    }))
}
/// Settle `promise` with what `future` gives, on the window's tasks.
pub(crate) fn settle_with(
    lua: &Lua,
    promise: &Rc<Promise>,
    kind: Kind,
    future: impl Future<Output = Result<Value, String>> + 'static,
) -> mlua::Result<()> {
    settle_failing(lua, promise, async move {
        future.await.map_err(|message| Failure::new(kind, message))
    })
}
/// As `settle_with`, for a future that tells its failures apart (a fetch:
/// the transport, a cancel, a status).
fn settle_failing(
    lua: &Lua,
    promise: &Rc<Promise>,
    future: impl Future<Output = Result<Value, Failure>> + 'static,
) -> mlua::Result<()> {
    let env = env(lua)?;
    let (settling, runtime) = (promise.clone(), env.runtime.clone());
    env.timers.spawn(async move {
        let result = future.await;
        batch(&runtime, || settling.settle(result));
    });
    Ok(())
}

/// `ui.fetch(url, opts)`'s request: `method`, `headers` (a table of
/// strings) and `body` (a string, or data sent as JSON).
fn request(lua: &Lua, url: String, opts: Option<Table>) -> mlua::Result<Request> {
    let mut request = Request {
        url,
        method: "GET".into(),
        headers: Vec::new(),
        body: None,
    };
    let Some(opts) = opts else {
        return Ok(request);
    };
    if let Some(method) = opts.get::<Option<String>>("method")? {
        request.method = method.to_uppercase();
    }
    if let Some(headers) = opts.get::<Option<Table>>("headers")? {
        for pair in headers.pairs::<String, String>() {
            request.headers.push(pair?);
        }
    }
    match opts.get::<mlua::Value>("body")? {
        mlua::Value::Nil => {}
        mlua::Value::String(text) => request.body = Some(text.as_bytes().to_vec()),
        other => {
            let value: Value = mlua::FromLua::from_lua(other, lua)
                .map_err(|e| mlua::Error::runtime(format!("ui.fetch body: {e}")))?;
            request.body = Some(value.to_json().into_bytes());
            if !request
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            {
                request
                    .headers
                    .push(("content-type".into(), "application/json".into()));
            }
        }
    }
    Ok(request)
}
/// A response as a script reads it: `status`, `ok` (2xx), `headers`
/// (lower-case names), `body` (text) and `json` (when it is JSON).
fn response_value(response: Response) -> Value {
    let headers: Vec<(Rc<str>, Value)> = {
        let mut headers: Vec<(Rc<str>, Value)> = response
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    name.to_lowercase().into(),
                    Value::Str(value.as_str().into()),
                )
            })
            .collect();
        headers.sort_by(|a, b| a.0.cmp(&b.0));
        headers.dedup_by(|a, b| a.0 == b.0);
        headers
    };
    let json = headers.iter().any(|(name, value)| {
        &**name == "content-type" && matches!(value, Value::Str(v) if v.contains("json"))
    });
    let body = String::from_utf8_lossy(&response.body);
    let parsed = if json {
        Value::from_json(&body).unwrap_or(Value::Nil)
    } else {
        Value::Nil
    };
    let mut fields: Vec<(Rc<str>, Value)> = vec![
        ("body".into(), Value::Str(body.as_ref().into())),
        ("headers".into(), Value::Map(headers.into())),
        ("json".into(), parsed),
        (
            "ok".into(),
            Value::Bool((200..300).contains(&response.status)),
        ),
        ("status".into(), Value::Num(response.status as f64)),
    ];
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(fields.into())
}

/// A request's way to stop: its future, ended as cancelled once `stop`
/// is called (dropping the host's work).
#[derive(Default)]
struct Stop {
    stopped: Cell<bool>,
    waker: RefCell<Option<std::task::Waker>>,
}
impl Stop {
    fn stop(&self) {
        self.stopped.set(true);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    async fn or_stopped<T>(
        self: Rc<Self>,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<T, Failure> {
        let mut future = std::pin::pin!(future);
        std::future::poll_fn(move |cx| {
            if self.stopped.get() {
                return std::task::Poll::Ready(Err(Failure::cancelled()));
            }
            *self.waker.borrow_mut() = Some(cx.waker().clone());
            // What the transport says failed: no answer came.
            future
                .as_mut()
                .poll(cx)
                .map(|done| done.map_err(|message| Failure::new(Kind::Network, message)))
        })
        .await
    }
}

/// The most of a streamed body kept to tell a failure (an error's JSON)
/// once the status says the request failed.
const FAILURE_KEPT: usize = 64 * 1024;

/// A streamed response's handlers (`on_chunk`, `on_event`), fed the body
/// as the host writes it: text in whole characters, and server-sent events
/// (`text/event-stream`) as each ends.
struct Listener {
    on_chunk: Option<Function>,
    on_event: Option<Function>,
    vm: Weak<Vm>,
    runtime: Runtime,
    promise: Weak<Promise>,
    /// A character split between pieces, waiting for its end.
    partial: Vec<u8>,
    events: Events,
    /// The start of the body, kept until the status is known.
    failure: Vec<u8>,
}
impl Listener {
    fn of(
        lua: &Lua,
        opts: &Table,
        promise: &Rc<Promise>,
    ) -> mlua::Result<Option<Rc<RefCell<Self>>>> {
        let on_chunk: Option<Function> = opts.get("on_chunk")?;
        let on_event: Option<Function> = opts.get("on_event")?;
        if on_chunk.is_none() && on_event.is_none() {
            return Ok(None);
        }
        let env = env(lua)?;
        Ok(Some(Rc::new(RefCell::new(Self {
            on_chunk,
            on_event,
            vm: env.vm.clone(),
            runtime: env.runtime.clone(),
            promise: Rc::downgrade(promise),
            partial: Vec::new(),
            events: Events::default(),
            failure: Vec::new(),
        }))))
    }
    fn hear(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if self.failure.len() < FAILURE_KEPT {
            let room = FAILURE_KEPT - self.failure.len();
            self.failure
                .extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
        self.partial.extend_from_slice(bytes);
        // Up to a character still to come, which waits for the next piece;
        // bytes that are no UTF-8 at all become U+FFFD, as lossy decoding of
        // the whole body makes them, and decoding goes on past them.
        let mut text = String::new();
        let mut rest = &self.partial[..];
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &[];
                    break;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    text.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    match e.error_len() {
                        Some(bad) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[bad..];
                        }
                        None => {
                            rest = after;
                            break;
                        }
                    }
                }
            }
        }
        let whole = self.partial.len() - rest.len();
        self.partial.drain(..whole);
        self.tell(&text);
    }
    /// The body has ended: what is left of a character, as U+FFFD. An event
    /// with no blank line after it is dropped, as browsers drop it.
    fn end(&mut self) {
        if !self.partial.is_empty() {
            let text = String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned();
            self.tell(&text);
        }
    }
    fn tell(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(vm) = self.vm.upgrade() else {
            // The script reloaded: nothing hears the rest.
            if let Some(promise) = self.promise.upgrade() {
                promise.abort();
            }
            return;
        };
        let events = match self.on_event {
            Some(_) => self.events.feed(text),
            None => Vec::new(),
        };
        batch(&self.runtime, || {
            if let Some(f) = &self.on_chunk
                && let Err(error) = vm.call::<()>(f, text)
            {
                vm.report(error);
            }
            if let Some(f) = &self.on_event {
                for event in events {
                    if let Err(error) = vm.call::<()>(f, event) {
                        vm.report(error);
                    }
                }
            }
        });
    }
}

/// Server-sent events read from a stream's text, as the HTML standard
/// reads `text/event-stream`: `field: value` lines, an event ending at a
/// blank line, `data` lines joined by line breaks.
#[derive(Default)]
struct Events {
    line: String,
    event: String,
    data: String,
    id: Option<String>,
    has_data: bool,
    /// The last piece ended in a carriage return: a line feed starting the
    /// next ends no second line.
    after_cr: bool,
    /// Some text has come: a byte order mark the stream starts with is
    /// behind it.
    started: bool,
}
impl Events {
    fn feed(&mut self, text: &str) -> Vec<Value> {
        let mut out = Vec::new();
        let mut text = text;
        // One byte order mark the stream starts with is no part of its
        // first field, as the HTML standard's parser drops it.
        if !self.started && !text.is_empty() {
            self.started = true;
            text = text.strip_prefix('\u{feff}').unwrap_or(text);
        }
        if self.after_cr {
            text = text.strip_prefix('\n').unwrap_or(text);
            self.after_cr = false;
        }
        while let Some(end) = text.find(['\r', '\n']) {
            self.line.push_str(&text[..end]);
            let line = std::mem::take(&mut self.line);
            if let Some(event) = self.field(&line) {
                out.push(event);
            }
            let crlf = text[end..].starts_with("\r\n");
            if text[end..].starts_with('\r') && end + 1 == text.len() {
                self.after_cr = true;
            }
            text = &text[end + if crlf { 2 } else { 1 }..];
        }
        self.line.push_str(text);
        out
    }
    /// One line: a field, a comment, or (blank) the event's end.
    fn field(&mut self, line: &str) -> Option<Value> {
        if line.is_empty() {
            if !self.has_data {
                self.event.clear();
                return None;
            }
            let event = match self.event.is_empty() {
                true => "message".to_string(),
                false => std::mem::take(&mut self.event),
            };
            let mut fields = vec![
                ("event", Value::Str(event.as_str().into())),
                (
                    "data",
                    Value::Str(std::mem::take(&mut self.data).as_str().into()),
                ),
            ];
            if let Some(id) = &self.id {
                fields.push(("id", Value::Str(id.as_str().into())));
            }
            self.has_data = false;
            return Some(Value::record(fields));
        }
        let (name, value) = match line.split_once(':') {
            Some(("", _)) => return None,
            Some((name, value)) => (name, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match name {
            "event" => self.event = value.to_string(),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            "id" if !value.contains('\0') => self.id = Some(value.to_string()),
            _ => {}
        }
        None
    }
}

/// Installs `ui.call`, `ui.task` and `ui.fetch` (`fetcher`, unless the
/// script may not reach the network: a plugin the host did not grant it).
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    actions: AsyncActions,
    fetcher: Option<SharedFetcher>,
) -> mlua::Result<()> {
    let (await_fn, try_fn): (Function, Function) =
        crate::lazy::builtin(lua, "=await", AWAIT_SOURCE)?.call(ui.clone())?;
    lua.set_named_registry_value(AWAIT, await_fn)?;
    ui.set("try", try_fn)?;
    // `ui.try`'s error maker, made on the first error (`errors::raised`):
    // registered here, so only a VM that names these pays even that.
    if let Some(lazy) = lua.app_data_ref::<crate::lazy::Lazy>() {
        lazy.ui(&["__error"], |lua, ui| {
            ui.set("__error", crate::errors::raised(lua)?)
        });
    }
    ui.set(
        "call",
        lua.create_function(move |lua, (name, arg): (String, Option<Value>)| {
            {
                let env = env(lua)?;
                env.permits(&env.exposed_actions, &name, "ui.call")?;
            }
            let arg = arg.unwrap_or(Value::Nil);
            let promise = pending(lua, name.clone())?;
            let action = actions.borrow().get(&name).cloned();
            let plain = env(lua)?.actions.borrow().get(&name).cloned();
            if let Some(action) = action {
                settle_with(lua, &promise, Kind::Action, action(arg))?;
            } else if let Some(action) = plain {
                action(arg);
                promise.state.set(Settled::Value(Value::Nil));
            } else {
                return Err(mlua::Error::runtime(format!(
                    "no host action named `{name}`"
                )));
            }
            Ok(PromiseUd(promise))
        })?,
    )?;
    ui.set(
        "fetch",
        lua.create_function(move |lua, (url, opts): (String, Option<Table>)| {
            let Some(fetcher) = &fetcher else {
                return Err(mlua::Error::runtime(
                    "ui.fetch: a plugin reaches the network only if the host grants it \
                     (`ScriptHost::grant(plugin, Capability::Fetch)`)",
                ));
            };
            let Some(fetch) = fetcher.borrow().clone() else {
                return Err(mlua::Error::runtime(
                    "ui.fetch: the host has no fetcher (`ScriptHost::fetcher`)",
                ));
            };
            let promise = pending(lua, format!("fetch {url}"))?;
            let listener = match &opts {
                Some(opts) => Listener::of(lua, opts, &promise)?,
                None => None,
            };
            // `raise = true`: a status outside 2xx rejects, as an `http`
            // error with its status and body (else it resolves, `ok` false).
            let raise = match &opts {
                Some(opts) => opts.get::<Option<bool>>("raise")?.unwrap_or(false),
                None => false,
            };
            let request = request(lua, url.clone(), opts)?;
            let body = match &listener {
                Some(listener) => {
                    let listener = listener.clone();
                    Body::new(Some(Box::new(move |bytes: &[u8]| {
                        listener.borrow_mut().hear(bytes)
                    })))
                }
                None => Body::kept(),
            };
            let future = fetch(request, body.clone());
            let stop = Rc::new(Stop::default());
            *promise.abort.borrow_mut() = Some(Box::new({
                let (stop, body) = (stop.clone(), body.clone());
                move || {
                    body.close();
                    stop.stop();
                }
            }));
            settle_failing(lua, &promise, async move {
                let mut response = stop.or_stopped(future).await?;
                response.body = match listener {
                    // Heard in pieces: the body only to tell a failure.
                    Some(listener) => {
                        let mut listener = listener.borrow_mut();
                        listener.hear(&response.body);
                        listener.end();
                        match (200..300).contains(&response.status) {
                            true => Vec::new(),
                            false => std::mem::take(&mut listener.failure),
                        }
                    }
                    None => body.whole(response.body),
                };
                let status = response.status;
                let value = response_value(response);
                if raise && !(200..300).contains(&status) {
                    let body = match value.field("json") {
                        Some(json) if *json != Value::Nil => json.clone(),
                        _ => value.field("body").cloned().unwrap_or(Value::Nil),
                    };
                    return Err(Failure {
                        kind: Kind::Http,
                        message: format!("HTTP {status}").into(),
                        status: Some(status),
                        body,
                    });
                }
                Ok(value)
            })?;
            Ok(PromiseUd(promise))
        })?,
    )?;
    ui.set("resource", lua.create_function(resource)?)?;
    ui.set(
        "task",
        lua.create_function(|lua, (f, args): (Function, MultiValue)| {
            let vm = env(lua)?
                .vm
                .upgrade()
                .ok_or_else(|| mlua::Error::runtime("the script has gone"))?;
            let thread = lua.create_thread(f)?;
            let task = Rc::new(TaskState::default());
            drive(&vm, thread, args, &task);
            Ok(TaskUd(task))
        })?,
    )?;
    Ok(())
}

/// `ui.resource(load, { initial })`: what `load` returns, loaded in a
/// task (it may await), as states: `data` (the last value it returned,
/// kept while it loads again; writable, as an optimistic update writes
/// it), `loading`, `error` (the last load's, or nil) and `status`
/// (`"loading"` until a load has answered, then `"ready"` or `"failed"`).
/// It loads again on `reload()` and when a state `load` read before its
/// first await changes (its key: the person a page shows); an answer to a
/// load since begun again, or to one whose owner has gone, is dropped.
fn resource(lua: &Lua, (load, opts): (Function, Option<Table>)) -> mlua::Result<Table> {
    let initial: Value = match &opts {
        Some(opts) => opts.get("initial")?,
        None => Value::Nil,
    };
    let env = env(lua)?;
    let runtime = env.runtime.clone();
    let data = runtime.signal(initial);
    let loading = runtime.signal(Value::Bool(true));
    let error = runtime.signal(Value::Nil);
    let status = runtime.signal(Value::Str("loading".into()));
    let reloads = runtime.signal(0u64);
    // Which load answers: each begun again counts on, and its owner gone
    // stops them all.
    let current = Rc::new(Cell::new(0u64));
    let wrap: Function = match lua.named_registry_value::<Function>(RESOURCE) {
        Ok(wrap) => wrap,
        Err(_) => {
            let wrap: Function = crate::lazy::builtin(lua, "=resource", RESOURCE_SOURCE)?.eval()?;
            lua.set_named_registry_value(RESOURCE, wrap.clone())?;
            wrap
        }
    };
    let vm = env.vm.clone();
    let meter = env.meter.clone();
    let (signals, counter) = ((data, loading, error, status), current.clone());
    // The load under way: cancelled when another begins, or its owner goes.
    let running: Rc<RefCell<Option<Rc<TaskState>>>> = Rc::default();
    let (running_in_effect, running_owned) = (running.clone(), running);
    let effect = runtime.effect_now({
        let (data, loading, error, status) = signals.clone();
        let reloads = reloads.clone();
        move || {
            reloads.get();
            let Some(vm) = vm.upgrade() else { return };
            if counter.get() == u64::MAX {
                return;
            }
            let mine = counter.get() + 1;
            counter.set(mine);
            loading.set(Value::Bool(true));
            if !matches!(status.with_untracked(Clone::clone), Value::Str(s) if &*s == "ready") {
                status.set(Value::Str("loading".into()));
            }
            let (data, loading, error, status) =
                (data.clone(), loading.clone(), error.clone(), status.clone());
            let (counter, meter) = (counter.clone(), meter.clone());
            let done = vm
                .lua
                .create_function(move |_, (ok, value): (bool, mlua::Value)| {
                    if counter.get() != mine {
                        return Ok(());
                    }
                    let answer = if ok {
                        let old = data.with_untracked(Clone::clone);
                        crate::value::convert(value, Some(&old)).and_then(|value| {
                            if let Some(meter) = &meter {
                                meter.hold(&data, value.weight())?;
                            }
                            Ok(value)
                        })
                    } else {
                        Err(mlua::Error::runtime(match &value {
                            mlua::Value::String(s) => s.to_string_lossy(),
                            mlua::Value::UserData(ud) if ud.is::<UiError>() => {
                                ud.borrow::<UiError>()?.message().to_string()
                            }
                            other => format!("{other:?}"),
                        }))
                    };
                    let (finished, failure) = match answer {
                        Ok(value) => {
                            data.set(value);
                            ("ready", Value::Nil)
                        }
                        Err(e) => ("failed", Value::Str(error_text(&e).into())),
                    };
                    error.set(failure);
                    status.set(Value::Str(finished.into()));
                    loading.set(Value::Bool(false));
                    Ok(())
                });
            let thread = done
                .and_then(|done| wrap.call::<Function>((load.clone(), done)))
                .and_then(|run| vm.lua.create_thread(run));
            let task = Rc::new(TaskState::default());
            if let Some(previous) = running_in_effect.borrow_mut().replace(task.clone()) {
                previous.cancel();
            }
            match thread {
                Ok(thread) => drive(&vm, thread, (), &task),
                Err(e) => vm.report(e),
            }
        }
    });
    // Its loads stop with its owner.
    struct Owned(
        #[allow(dead_code)] zgx::reactive::Effect,
        Rc<Cell<u64>>,
        Rc<RefCell<Option<Rc<TaskState>>>>,
    );
    impl Drop for Owned {
        fn drop(&mut self) {
            self.1.set(u64::MAX);
            if let Some(task) = self.2.borrow_mut().take() {
                task.cancel();
            }
        }
    }
    crate::node::retain(&env, Box::new(Owned(effect, current, running_owned)));
    drop(env);
    let (data, loading, error, status) = signals;
    let out = lua.create_table()?;
    out.set("data", crate::value::State::new(data))?;
    out.set("loading", crate::value::State::new(loading))?;
    out.set("error", crate::value::State::new(error))?;
    out.set("status", crate::value::State::new(status))?;
    out.set(
        "reload",
        lua.create_function(move |_, _: MultiValue| {
            reloads.update(|n| *n += 1);
            Ok(())
        })?,
    )?;
    Ok(out)
}
/// A load's error as a person reads it: what was raised, without Luau's
/// traceback.
fn error_text(error: &mlua::Error) -> String {
    match error {
        mlua::Error::RuntimeError(message) => message.clone(),
        mlua::Error::CallbackError { cause, .. } => error_text(cause),
        other => other.to_string(),
    }
}

const RESOURCE: &str = "zgx.resource";
/// A resource's load, run as a task: `done(ok, value)` with what it
/// returned or raised.
const RESOURCE_SOURCE: &str = r#"
return function(load, done)
    return function()
        done(pcall(load))
    end
end
"#;

fn batch(runtime: &Runtime, f: impl FnOnce()) {
    runtime.clone().batch(f);
}
