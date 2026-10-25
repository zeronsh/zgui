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
use crate::{Vm, node::env, value::Value};
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
pub(crate) type Fetch = Pin<Box<dyn Future<Output = Result<Response, String>>>>;
/// The host's transport (`ScriptHost::fetcher`): it owns the HTTP stack,
/// headers and credentials.
pub(crate) type Fetcher = Rc<dyn Fn(Request) -> Fetch>;
pub(crate) type SharedFetcher = Rc<RefCell<Option<Fetcher>>>;
pub(crate) type AsyncAction = Rc<dyn Fn(Value) -> Call>;
/// The host's async actions, by name.
pub(crate) type AsyncActions = Rc<RefCell<HashMap<String, AsyncAction>>>;

#[derive(Clone, PartialEq)]
enum Settled {
    Pending,
    Value(Value),
    Error(String),
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
}
impl TaskState {
    pub(crate) fn cancel(&self) {
        self.cancelled.set(true);
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
}

#[derive(Clone)]
pub(crate) struct PromiseUd(pub(crate) Rc<Promise>);

impl Promise {
    fn settle(self: &Rc<Self>, result: Result<Value, String>) {
        let settled = match result {
            Ok(value) => Settled::Value(value),
            Err(error) => Settled::Error(error),
        };
        self.state.set(settled.clone());
        let waiters = std::mem::take(&mut *self.waiters.borrow_mut());
        if let Settled::Error(error) = &settled
            && waiters.is_empty()
            && !self.handled.get()
            && let Some(vm) = self.caller.upgrade()
        {
            vm.report(mlua::Error::runtime(format!(
                "ui.call(\"{}\"): {error} (nothing handled it: `:after(_, on_error)`, `.error` or `:await()` in a pcall)",
                self.name
            )));
        }
        for waiter in waiters {
            hear(waiter, &settled);
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
                Settled::Error(error) => on_error.map(|f| vm.call::<()>(&f, error.clone())),
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
                Settled::Value(value) => (true, value.clone()),
                Settled::Error(error) => (false, Value::Str(error.as_str().into())),
                Settled::Pending => return,
            };
            drive(&vm, thread, resumed, &task);
        }
    }
}

/// Run task `thread` from where it is until it ends, awaits a promise
/// still pending, or `task` is cancelled.
fn drive(vm: &Rc<Vm>, thread: Thread, args: impl IntoLuaMulti, task: &Rc<TaskState>) {
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
                promise.waiters.borrow_mut().push(Waiter::Task {
                    vm: Rc::downgrade(vm),
                    thread,
                    task: task.clone(),
                });
                return;
            }
            Settled::Value(value) => args = Some((true, value).into_lua_multi(&vm.lua)),
            Settled::Error(error) => {
                args = Some((false, Value::Str(error.as_str().into())).into_lua_multi(&vm.lua))
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
                Settled::Error(error) => Some(error.clone()),
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
const AWAIT_SOURCE: &str = r#"
local yield, yieldable = coroutine.yield, coroutine.isyieldable
return function(promise)
    if not yieldable() then
        error("await runs inside ui.task(function() ... end)", 2)
    end
    local ok, value = yield(promise)
    if not ok then
        -- The host's message as it wrote it.
        error(value, 0)
    end
    return value
end
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
    }))
}
/// Settle `promise` with what `future` gives, on the window's tasks.
pub(crate) fn settle_with(
    lua: &Lua,
    promise: &Rc<Promise>,
    future: impl Future<Output = Result<Value, String>> + 'static,
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

/// Installs `ui.call`, `ui.task` and `ui.fetch` (`fetcher`, unless the
/// script may not reach the network: a plugin the host did not grant it).
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    actions: AsyncActions,
    fetcher: Option<SharedFetcher>,
) -> mlua::Result<()> {
    let await_fn: Function = lua.load(AWAIT_SOURCE).set_name("=await").eval()?;
    lua.set_named_registry_value(AWAIT, await_fn)?;
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
                settle_with(lua, &promise, action(arg))?;
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
            let request = request(lua, url.clone(), opts)?;
            let promise = pending(lua, format!("fetch {url}"))?;
            let future = fetch(request);
            settle_with(
                lua,
                &promise,
                async move { future.await.map(response_value) },
            )?;
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
            let wrap: Function = lua.load(RESOURCE_SOURCE).set_name("=resource").eval()?;
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
