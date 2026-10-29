//! `ui.router { routes, scheme? }`: where an app is, as React Router keeps
//! it. Routes are named paths with `:params` (`thread = "/thread/:id"`);
//! the router holds the current one as a state, its history behind and
//! ahead (`navigate` by name, `open` by path), and goes back and forward on
//! the platform's own input: the mouse's
//! back and forward buttons over its view, and Alt+←/→ (⌘[ and ⌘] on
//! macOS) as commands. With a `scheme`, deep links the system opens the app
//! with (`mail://thread/42`) navigate to their route; one that matches no
//! route is reported once.
//!
//! Luau over the library's states, made the first time a script names
//! `ui.router`.
use crate::node::env;
use crate::value::{State, Value};
use mlua::{Function, Lua, Table};

const MADE: &str = "zgx.router";

const SOURCE: &str = r#"
local ui, new_state, mac = ...

local function segments(path: string): { string }
    local out = {}
    for segment in path:gmatch("[^/]+") do
        table.insert(out, segment)
    end
    return out
end

-- `path`'s params if it matches `pattern`'s segments, else nil.
local function match(pattern, path)
    local parts = segments(path)
    if #parts ~= #pattern then
        return nil
    end
    local params = {}
    for i, part in pattern do
        if part:sub(1, 1) == ":" then
            params[part:sub(2)] = parts[i]
        elseif part ~= parts[i] then
            return nil
        end
    end
    return params
end

local function router(spec)
    if type(spec) ~= "table" or type(spec.routes) ~= "table" then
        error("ui.router needs `routes`: each route's name and path (`thread = \"/thread/:id\"`)", 2)
    end
    -- The routes in a fixed order (by name), so the first match is stable.
    local order = {}
    for name, path in spec.routes do
        if type(path) ~= "string" or path:sub(1, 1) ~= "/" then
            error(`ui.router: route \`{name}\`'s path starts with /`, 2)
        end
        table.insert(order, { name = name, pattern = segments(path) })
    end
    table.sort(order, function(a, b)
        return a.name < b.name
    end)
    local by_name = {}
    for _, r in order do
        by_name[r.name] = r
    end
    local function resolve(path: string)
        for _, r in order do
            local params = match(r.pattern, path)
            if params then
                return { name = r.name, path = path, params = params }
            end
        end
        return nil
    end
    -- The route a path matches ("/thread/42").
    local function route_at(path, level)
        local found = if type(path) == "string" then resolve(path) else nil
        if not found then
            error(`ui.router: no route matches \`{tostring(path)}\``, level)
        end
        return found
    end
    -- Route `to` by its name, with its params.
    local function route_of(to, params, level)
        local r = by_name[to]
        if not r then
            error(`ui.router: no route is named \`{tostring(to)}\``, level)
        end
        local parts = {}
        for _, part in r.pattern do
            if part:sub(1, 1) == ":" then
                local value = if params then params[part:sub(2)] else nil
                if value == nil then
                    error(`ui.router: route \`{to}\` needs \`{part:sub(2)}\``, level)
                end
                table.insert(parts, tostring(value))
            else
                table.insert(parts, part)
            end
        end
        local path = "/" .. table.concat(parts, "/")
        return { name = to, path = path, params = match(r.pattern, path) }
    end

    local self = {}
    self.route = new_state(route_at(spec.initial or "/", 2))
    local behind, ahead = {}, {}
    local depth = new_state({ 0, 0 })
    local function moved()
        depth:set({ #behind, #ahead })
    end
    self.can_back = ui.computed(function()
        return depth:get()[1] > 0
    end)
    self.can_forward = ui.computed(function()
        return depth:get()[2] > 0
    end)
    local function go(next, replace)
        if replace then
            self.route:set(next)
            return
        end
        ui.batch(function()
            table.insert(behind, self.route:peek())
            table.clear(ahead)
            self.route:set(next)
            moved()
        end)
    end
    -- To a route by its name and params (typed per app: a misspelled name
    -- or a missing param is a type error).
    function self.navigate(to, params)
        go(route_of(to, params, 3), false)
    end
    function self.replace(to, params)
        go(route_of(to, params, 3), true)
    end
    -- To the route a path matches ("/thread/42"), as a deep link goes;
    -- `{ replace = true }` in place of the route now.
    function self.open(path, opts)
        go(route_at(path, 3), opts ~= nil and opts.replace == true)
    end
    function self.back()
        if #behind == 0 then
            return
        end
        ui.batch(function()
            table.insert(ahead, self.route:peek())
            self.route:set(table.remove(behind))
            moved()
        end)
    end
    function self.forward()
        if #ahead == 0 then
            return
        end
        ui.batch(function()
            table.insert(behind, self.route:peek())
            self.route:set(table.remove(ahead))
            moved()
        end)
    end
    -- Whether the route is `name` now (tracked).
    function self.is(name: string): boolean
        return self.route:get().name == name
    end
    -- What the route shows: `cases[name]`, a node or a function of the
    -- route's params, made again as the route changes; the mouse's back and
    -- forward buttons over it go back and forward.
    function self.view(cases, props)
        local view = props or {}
        if view.grow == nil and view.w == nil and view.h == nil then
            view.grow = 1
        end
        view.on_aux_click = function(e)
            if e.button == 4 then
                self.back()
            elseif e.button == 5 then
                self.forward()
            end
        end
        table.insert(
            view,
            ui.presence(self.route, function(route)
                local case = cases[route.name]
                if type(case) == "function" then
                    return case(route.params)
                end
                return case or ui.div {}
            end)
        )
        return ui.column(view)
    end
    ui.command {
        id = "router.back",
        title = "Back",
        key = if mac then "cmd+[" else "alt+ArrowLeft",
        run = self.back,
    }
    ui.command {
        id = "router.forward",
        title = "Forward",
        key = if mac then "cmd+]" else "alt+ArrowRight",
        run = self.forward,
    }
    if spec.scheme then
        local prefix = `{spec.scheme}://`
        local reported = {}
        ui.on_open(function(e)
            for _, url in e.urls do
                if url:sub(1, #prefix) == prefix then
                    local path = "/" .. url:sub(#prefix + 1):gsub("^/+", "")
                    local found = resolve(path)
                    if found then
                        go(found, false)
                    elseif not reported[url] then
                        reported[url] = true
                        ui.task(function()
                            error(`ui.router: the link \`{url}\` matches no route`, 0)
                        end)
                    end
                end
            end
        end)
    end
    return self
end

return router
"#;

/// The router function, made the first time a script asks for it.
fn made(lua: &Lua) -> mlua::Result<Function> {
    if let Ok(made) = lua.named_registry_value::<Function>(MADE) {
        return Ok(made);
    }
    let new_state = lua
        .create_function(|lua, initial: Value| Ok(State::new(env(lua)?.runtime.signal(initial))))?;
    let ui: Table = lua.globals().get("ui")?;
    let router: Function = crate::lazy::builtin(lua, "=router", SOURCE)?.call((
        ui,
        new_state,
        cfg!(target_os = "macos"),
    ))?;
    lua.set_named_registry_value(MADE, router.clone())?;
    Ok(router)
}

/// Installs `ui.router`.
pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set(
        "router",
        lua.create_function(|lua, spec: mlua::Value| made(lua)?.call::<Table>(spec))?,
    )
}
