//! `ui.each(list, row, { key, reorder = function(from, to) end })`: rows
//! the person drags into a new order, or moves with Alt+↑ and Alt+↓, as
//! dnd-kit's sortable lists. Each row takes the drag and drops above or
//! below itself by where the pointer is, with a line showing where; the
//! list calls `reorder(from, to)` (positions from 1, `to` where the row
//! ends up) and the script moves its items. A move by keyboard is heard by
//! assistive technology ("Moved Inbox to position 2 of 5"), named by the
//! item's `item_label` (or its key). It works with `animate`, rows sliding to
//! their new places.
//!
//! The row builder wrapped in Luau, made the first time a script gives an
//! `each` a `reorder`: a plain `each` is unchanged.
use mlua::{Function, Lua, Table};

const MADE: &str = "zgx.reorder";

const SOURCE: &str = r#"
local ui, new_state = ...

return function(list, build, opts)
    local reorder, key, name_of = opts.reorder, opts.key, opts.item_label
    if type(reorder) ~= "function" then
        error("ui.each's `reorder` is a function of the positions from and to", 3)
    end
    if type(key) ~= "function" then
        error("ui.each's `reorder` needs `key`: each item's identity, to find it as the list changes", 3)
    end
    -- Where a drag would drop: the row's key, and whether after it.
    local target = new_state(nil)
    -- Which list a drag is from: a drag's payload crosses as data, so a
    -- string, not a table's identity.
    local token = tostring(newproxy and newproxy() or {})
    local function position(k)
        for i, item in list:peek() do
            if key(item) == k then
                return i
            end
        end
        return nil
    end
    local function move(from, to)
        if from and to and from ~= to then
            reorder(from, to)
        end
    end
    return function(item, ...)
        local k = key(item)
        local name = if name_of then tostring(name_of(item)) else tostring(k)
        local heard = ui.use_state("")
        local row = build(item, ...)
        return ui.div {
            relative = true,
            -- A Tab stop a reader names, works and is told how to move, as
            -- dnd-kit's sortable items are.
            focusable = true,
            role = "button",
            label = name,
            description = "Alt+Up and Alt+Down move it",
            on_drag = function()
                return { reorder = token, key = k }
            end,
            drop_when = function(p)
                return type(p) == "table" and p.reorder == token and p.key ~= k
            end,
            on_drag_over = function(_, e)
                target:set({ key = k, after = e.local_y > e.h / 2 })
            end,
            on_drag_leave = function()
                local t = target:peek()
                if t and t.key == k then
                    target:set(nil)
                end
            end,
            on_drop = function(p, e)
                target:set(nil)
                local from, at = position(p.key), position(k)
                if not from or not at then
                    return
                end
                local to = if e.local_y > e.h / 2 then at + 1 else at
                if from < to then
                    to -= 1
                end
                move(from, to)
            end,
            on_key = function(e)
                if not e.alt or (e.key ~= "ArrowUp" and e.key ~= "ArrowDown") then
                    return false
                end
                local from, count = position(k), #list:peek()
                if not from then
                    return true
                end
                local to = if e.key == "ArrowUp" then from - 1 else from + 1
                if to >= 1 and to <= count then
                    move(from, to)
                    heard:set(`Moved {name} to position {to} of {count}`)
                end
                return true
            end,
            row,
            -- Where a drag would drop, over the row's edge.
            ui.div {
                absolute = true,
                left = 0,
                right = 0,
                h = 2,
                bg = "$accent",
                aria_hidden = true,
                top = function()
                    local t = target:get()
                    return if t and t.key == k and not t.after then 0 else nil
                end,
                bottom = function()
                    local t = target:get()
                    return if t and t.key == k and t.after then 0 else nil
                end,
                hidden = function()
                    local t = target:get()
                    return not (t and t.key == k)
                end,
            },
            -- What a keyboard move says, heard and not seen.
            ui.text {
                absolute = true,
                w = 0,
                h = 0,
                overflow = "hidden",
                live = "polite",
                function()
                    return heard:get()
                end,
            },
        }
    end
end
"#;

fn made(lua: &Lua) -> mlua::Result<Function> {
    if let Ok(made) = lua.named_registry_value::<Function>(MADE) {
        return Ok(made);
    }
    let new_state = lua.create_function(|lua, initial: crate::value::Value| {
        Ok(crate::value::State::new(
            crate::node::env(lua)?.runtime.signal(initial),
        ))
    })?;
    let ui: Table = lua.globals().get("ui")?;
    let wrap: Function = crate::lazy::builtin(lua, "=reorder", SOURCE)?.call((ui, new_state))?;
    lua.set_named_registry_value(MADE, wrap.clone())?;
    Ok(wrap)
}

/// `build` wrapped for a reorderable `each` (its options `opts`).
#[cold]
#[inline(never)]
pub(crate) fn wrap(
    lua: &Lua,
    list: mlua::Value,
    build: Function,
    opts: &Table,
) -> mlua::Result<Function> {
    made(lua)?.call((list, build, opts.clone()))
}
