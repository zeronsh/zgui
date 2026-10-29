//! `ui.paged(load)` and `ui.debounced(state, ms)`, made the first time a
//! script names either:
//! - `ui.paged` loads a list a page at a time (an inbox, a feed): `load`
//!   takes a cursor (nil for the first page) and returns `{ items, next }`,
//!   in a task, so it may await. Its rows show `items`; `more()` (a list's
//!   `on_end`) loads the next page, and does nothing while one loads or
//!   once there is no `next`. As `ui.resource`, it loads again from the
//!   start on `reload()` and when a state its first page read changes (a
//!   search's query), a load still out cancelled, an answer to one
//!   superseded dropped.
//! - `ui.debounced(state, ms)` is a read-only state following `state` once
//!   it has stayed unchanged for `ms` (a search box's query): one timer,
//!   begun again on each change, gone with its owner.
use crate::{
    calls::{TaskState, drive},
    errors::{self, Failure, UiError},
    node::env,
    value::{ReadState, State, Value},
};
use mlua::{Function, Lua, MetaMethod, MultiValue, Table};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgx::reactive::Signal;

pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set("paged", lua.create_function(paged)?)?;
    ui.set("debounced", lua.create_function(debounced)?)
}

const PAGE: &str = "zgx.page";
/// A page's load, run as a task: `done(ok, value)` with what `load(cursor)`
/// returned or raised.
const PAGE_SOURCE: &str = r#"
return function(load, cursor, done)
    return function()
        done(pcall(load, cursor))
    end
end
"#;

/// What a paged list holds while it lives.
struct Pages {
    items: Signal<Value>,
    loading: Signal<Value>,
    status: Signal<Value>,
    done: Signal<Value>,
    error: Signal<Option<Rc<Failure>>>,
    /// The next page's cursor (`next` of the last page loaded).
    next: RefCell<Value>,
    /// Which load answers: each begun counts on; `u64::MAX` once its owner
    /// has gone.
    current: Cell<u64>,
    /// The load under way, cancelled when another begins from the start.
    running: RefCell<Option<Rc<TaskState>>>,
}

impl Pages {
    /// Begin loading the page at `cursor`, after the pages already shown
    /// (`fresh`: in place of them).
    fn load(self: &Rc<Self>, lua: &Lua, load: &Function, cursor: Value, fresh: bool) {
        let Ok(env) = env(lua) else { return };
        let Some(vm) = env.vm.upgrade() else { return };
        drop(env);
        if self.current.get() == u64::MAX {
            return;
        }
        let mine = self.current.get() + 1;
        self.current.set(mine);
        self.loading.set(Value::Bool(true));
        if fresh {
            self.status.set(Value::Str("loading".into()));
        }
        let pages = self.clone();
        let done = vm
            .lua
            .create_function(move |lua, (ok, value): (bool, mlua::Value)| {
                if pages.current.get() != mine {
                    return Ok(());
                }
                pages.answer(lua, ok, value, fresh);
                Ok(())
            });
        let wrap = match lua.named_registry_value::<Function>(PAGE) {
            Ok(wrap) => Ok(wrap),
            Err(_) => crate::lazy::builtin(lua, "=page", PAGE_SOURCE)
                .and_then(|chunk| chunk.eval::<Function>())
                .and_then(|wrap| {
                    lua.set_named_registry_value(PAGE, wrap.clone())?;
                    Ok(wrap)
                }),
        };
        let thread = done
            .and_then(|done| wrap?.call::<Function>((load.clone(), cursor, done)))
            .and_then(|run| vm.lua.create_thread(run));
        let task = Rc::new(TaskState::default());
        if let Some(previous) = self.running.borrow_mut().replace(task.clone()) {
            previous.cancel();
        }
        match thread {
            Ok(thread) => drive(&vm, thread, (), &task),
            Err(e) => vm.report(e),
        }
    }

    /// A page's answer: `{ items, next }`, or what its load raised.
    fn answer(&self, lua: &Lua, ok: bool, value: mlua::Value, fresh: bool) {
        let page = match (ok, &value) {
            (true, mlua::Value::Table(page)) => page
                .get::<Value>("items")
                .and_then(|items| Ok((items, page.get::<Value>("next")?)))
                .map_err(|e| Rc::new(Failure::new(errors::Kind::Raised, e.to_string()))),
            (true, _) => Err(Rc::new(Failure::new(
                errors::Kind::Raised,
                "ui.paged's load returns { items, next }",
            ))),
            (false, raised) => Err(errors::failure_of(lua, raised)),
        };
        match page {
            Ok((items, next)) => {
                let added = match items {
                    Value::List(items) => items,
                    Value::Nil => Rc::from([]),
                    _ => {
                        self.fail(Rc::new(Failure::new(
                            errors::Kind::Raised,
                            "ui.paged's load returns { items, next }: `items` is a list",
                        )));
                        return;
                    }
                };
                let shown = match fresh {
                    true => added,
                    false => self.items.with_untracked(|items| match items {
                        Value::List(shown) => shown.iter().chain(added.iter()).cloned().collect(),
                        _ => added,
                    }),
                };
                let last = next == Value::Nil;
                *self.next.borrow_mut() = next;
                self.items.set(Value::List(shown));
                self.done.set(Value::Bool(last));
                self.error.set(None);
                self.status.set(Value::Str("ready".into()));
                self.loading.set(Value::Bool(false));
            }
            Err(failure) => self.fail(failure),
        }
    }

    fn fail(&self, failure: Rc<Failure>) {
        self.error.set(Some(failure));
        self.status.set(Value::Str("failed".into()));
        self.loading.set(Value::Bool(false));
    }
}

/// Its loads stop with its owner.
struct Owned(#[allow(dead_code)] zgx::reactive::Effect, Rc<Pages>);
impl Drop for Owned {
    fn drop(&mut self) {
        self.1.current.set(u64::MAX);
        if let Some(task) = self.1.running.borrow_mut().take() {
            task.cancel();
        }
    }
}

fn paged(lua: &Lua, load: Function) -> mlua::Result<Table> {
    let env = env(lua)?;
    let runtime = env.runtime.clone();
    let pages = Rc::new(Pages {
        items: runtime.signal(Value::List(Rc::from([]))),
        loading: runtime.signal(Value::Bool(true)),
        status: runtime.signal(Value::Str("loading".into())),
        done: runtime.signal(Value::Bool(false)),
        error: runtime.signal(None),
        next: RefCell::new(Value::Nil),
        current: Cell::new(0),
        running: RefCell::default(),
    });
    let reloads = runtime.signal(0u64);
    let vm = env.vm.clone();
    // The first page, keyed by what it reads (as `ui.resource`'s load is):
    // a change, or `reload()`, loads again from the start.
    let effect = runtime.effect_now({
        let (pages, load, reloads) = (pages.clone(), load.clone(), reloads.clone());
        move || {
            reloads.get();
            let Some(vm) = vm.upgrade() else { return };
            pages.load(&vm.lua, &load, Value::Nil, true);
        }
    });
    crate::node::retain(&env, Box::new(Owned(effect, pages.clone())));
    drop(env);
    let out = lua.create_table()?;
    out.set("items", State::new(pages.items.clone()))?;
    out.set("loading", State::new(pages.loading.clone()))?;
    out.set("status", State::new(pages.status.clone()))?;
    out.set("done", State::new(pages.done.clone()))?;
    out.set("more", {
        let pages = pages.clone();
        let runtime = runtime.clone();
        lua.create_function(move |lua, _: MultiValue| {
            let busy = pages.loading.with_untracked(|v| *v == Value::Bool(true));
            let done = pages.done.with_untracked(|v| *v == Value::Bool(true));
            if !busy && !done {
                let cursor = pages.next.borrow().clone();
                // Not a key: what a later page reads loads nothing again.
                runtime.untracked(|| pages.load(lua, &load, cursor, false));
            }
            Ok(())
        })?
    })?;
    out.set(
        "reload",
        lua.create_function(move |_, _: MultiValue| {
            reloads.update(|n| *n += 1);
            Ok(())
        })?,
    )?;
    // `error`: the last load's `UiError` (tracked), or nil; read through
    // `__index`, a state holding only plain values.
    let error = pages.error.clone();
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

fn debounced(lua: &Lua, (state, ms): (mlua::AnyUserData, f64)) -> mlua::Result<ReadState> {
    if !(ms.is_finite() && ms >= 0.) {
        return Err(mlua::Error::runtime(
            "ui.debounced: a number of milliseconds",
        ));
    }
    let source = match state.borrow::<State>() {
        Ok(state) => state.0.clone(),
        Err(_) => state.borrow::<ReadState>()?.0.0.clone(),
    };
    let env = env(lua)?;
    let runtime = env.runtime.clone();
    let period = crate::duration_ms("ui.debounced", ms)?;
    let out = runtime.signal(source.with_untracked(Clone::clone));
    // Each change begins the wait again; the timer that outlives the rest
    // writes what the state then holds.
    let generation = Rc::new(Cell::new(0u64));
    let (timers, write, read) = (env.timers.clone(), out.clone(), source.clone());
    let mut first = true;
    let effect = runtime.effect({
        let generation = generation.clone();
        move || {
            source.with(|_| ());
            if std::mem::take(&mut first) {
                return;
            }
            let mine = generation.get().wrapping_add(1);
            generation.set(mine);
            let (generation, write, read) = (generation.clone(), write.clone(), read.clone());
            timers.spawn(async move {
                zgx::timer::sleep(period).await;
                if generation.get() == mine {
                    write.set(read.with_untracked(Clone::clone));
                }
            });
        }
    });
    struct Owned(#[allow(dead_code)] zgx::reactive::Effect, Rc<Cell<u64>>);
    impl Drop for Owned {
        fn drop(&mut self) {
            // A wait still out writes nothing.
            self.1.set(u64::MAX);
        }
    }
    crate::node::retain(&env, Box::new(Owned(effect, generation)));
    Ok(ReadState(State::new(out), "debounced".into()))
}
