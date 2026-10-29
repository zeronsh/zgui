//! Handlers that await and optimistic writes, as React's async handlers
//! and `useOptimistic`:
//! - `ui.async(f)` is a handler running `f` as a task each time it is
//!   called (a save on a click, a toast's Undo): it may await, a task per
//!   call, all cancelled as what made the handler goes (a component, a
//!   reload). What it raises and doesn't catch is reported once, as a task's
//!   error is.
//! - `ui.optimistic(state, next, save)`, inside a task, writes `next` now and
//!   runs `save`: on success the write stays; on failure the state goes back
//!   and the error is returned (nil on success). Calls on one state may
//!   overlap: the newest write shows, and a failure puts back only what its
//!   own write replaced, never over a newer one still pending or kept (a
//!   write kept settles the older ones still out).
//!
//! Luau over `ui.task` and `ui.try`, made the first time a script names
//! either (a state's methods are untouched: a field on it would slow every
//! method's lookup); Rust keeps each state's pending writes (by signal, so two
//! handles on one state share them).
use crate::value::{State, Value};
use mlua::{Function, Lua, Table};
use std::{cell::RefCell, rc::Rc};
use zgx::reactive::Signal;

const MADE: &str = "zgx.handlers";

const SOURCE: &str = r#"
local ui, enter, settle = ...

local function async(f)
    if type(f) ~= "function" then
        error("ui.async takes a function to run as a task", 2)
    end
    local running = {}
    ui.on_cleanup(function()
        for task in running do
            task:cancel()
        end
    end)
    return function(...)
        local task
        task = ui.task(function(...)
            local ok, err = pcall(f, ...)
            running[task] = nil
            if not ok then
                error(err, 0)
            end
        end, ...)
        if not task.cancelled then
            running[task] = true
        end
        return task
    end
end

local function optimistic(state, next, save)
    if type(save) ~= "function" then
        error("ui.optimistic(state, next, save) takes the save to run", 2)
    end
    local token = enter(state, next)
    local _, err = ui.try(save)
    settle(state, token, err == nil)
    return err
end

return async, optimistic
"#;

/// A state's optimistic writes still pending, oldest first: each one's
/// token and what it replaced (the value to go back to, as it stands).
type Pending = Vec<(Signal<Value>, Vec<(u64, Value)>)>;

/// The functions, made the first time a script asks for one.
fn made(lua: &Lua) -> mlua::Result<(Function, Function)> {
    if let Ok(made) = lua.named_registry_value::<Table>(MADE) {
        return Ok((made.raw_get(1)?, made.raw_get(2)?));
    }
    let pending: Rc<RefCell<Pending>> = Rc::default();
    let next_token = Rc::new(std::cell::Cell::new(0u64));
    let enter = {
        let pending = pending.clone();
        lua.create_function(move |lua, (state, next): (mlua::AnyUserData, Value)| {
            let state = state.borrow::<State>()?.clone();
            let token = next_token.get() + 1;
            next_token.set(token);
            let before = state.0.with_untracked(Clone::clone);
            {
                let mut pending = pending.borrow_mut();
                match pending.iter_mut().find(|(s, _)| s.ptr_eq(&state.0)) {
                    Some((_, writes)) => writes.push((token, before)),
                    None => pending.push((state.0.clone(), vec![(token, before)])),
                }
            }
            state.write_from(lua, next);
            Ok(token)
        })?
    };
    let settle = lua.create_function(
        move |lua, (state, token, kept): (mlua::AnyUserData, u64, bool)| {
            let state = state.borrow::<State>()?.clone();
            let back = {
                let mut pending = pending.borrow_mut();
                let Some(at) = pending.iter().position(|(s, _)| s.ptr_eq(&state.0)) else {
                    return Ok(());
                };
                let writes = &mut pending[at].1;
                let Some(i) = writes.iter().position(|(t, _)| *t == token) else {
                    return Ok(());
                };
                if kept {
                    // Kept: it and every older write are settled (what one of
                    // those does after cannot put back over it).
                    writes.drain(..=i);
                    if writes.is_empty() {
                        pending.remove(at);
                    }
                    return Ok(());
                }
                let (_, before) = writes.remove(i);
                let back = if i == writes.len() {
                    // The newest failed: back to what it replaced.
                    Some(before)
                } else {
                    // An older one failed: the newer write showing stays,
                    // and goes back (if it fails) to what this one replaced.
                    writes[i].1 = before;
                    None
                };
                if pending[at].1.is_empty() {
                    pending.remove(at);
                }
                back
            };
            if let Some(before) = back {
                state.write_from(lua, before);
            }
            Ok(())
        },
    )?;
    let ui: Table = lua.globals().get("ui")?;
    let (async_fn, optimistic): (Function, Function) =
        crate::lazy::builtin(lua, "=handlers", SOURCE)?.call((ui, enter, settle))?;
    let made = lua.create_sequence_from([async_fn.clone(), optimistic.clone()])?;
    lua.set_named_registry_value(MADE, made)?;
    Ok((async_fn, optimistic))
}

/// Installs `ui.async` and `ui.optimistic` (the Luau functions themselves,
/// so a save awaits with no Rust call between it and its task).
pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    let (async_fn, optimistic) = made(lua)?;
    ui.set("async", async_fn)?;
    ui.set("optimistic", optimistic)
}
