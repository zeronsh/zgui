//! Forms: `ui.form { fields, validate, submit, on_success }` holds each
//! field's value, whether it has been left, and what is wrong with it
//! (checked as it is typed, shown once it has been left or a submit tried),
//! and submits in a task with its `pending` and `failure` states. A control
//! given `field = form.fields.email` binds to it: its value, its leaving,
//! its error (heard by assistive technology, as `aria-invalid`), and, for a
//! one-line input, Enter submitting the form.
//!
//! The form is Luau over the library's states and computeds; Rust gives it
//! states of its own (no key: each form's) and binds the controls.
use crate::node::env;
use crate::value::{State, Value};
use mlua::{Function, Lua, Table};

const BIND: &str = "zgx.form.bind";

const SOURCE: &str = r#"
local ui, new_state = ...

local function message(err: any): string
    return if type(err) == "string" then err else tostring(err)
end

local function form(spec)
    if type(spec) ~= "table" or type(spec.fields) ~= "table" then
        error("ui.form needs `fields`: each field's name and its first value", 2)
    end
    local checks = spec.validate or {}
    local self = { fields = {}, values = {} }
    local attempted = new_state(false)
    self.attempted = attempted
    -- The submit under way, if any.
    local running = nil
    self.pending = new_state(false)
    self.failure = new_state(nil)
    -- The values as a check reads them: each read follows that field alone.
    local values = setmetatable({}, {
        __index = function(_, name)
            local field = self.fields[name]
            return if field then field.value:get() else nil
        end,
    })
    for name, initial in spec.fields do
        local field = { name = name, form = self }
        field.value = new_state(initial)
        field.touched = new_state(false)
        field.ref = ui.ref()
        local check = checks[name]
        field.problem = ui.computed(function()
            if not check then
                return nil
            end
            local problem = check(field.value:get(), values)
            return if problem == "" or problem == false then nil else problem
        end)
        field.error = ui.computed(function()
            if field.touched:get() or attempted:get() then
                return field.problem:get()
            end
            return nil
        end)
        self.fields[name] = field
        self.values[name] = field.value
    end
    for name in checks do
        if self.fields[name] == nil then
            error(`ui.form: \`validate.{name}\` checks no field`, 2)
        end
    end
    self.valid = ui.computed(function()
        for _, field in self.fields do
            if field.problem:get() ~= nil then
                return false
            end
        end
        return true
    end)
    function self.snapshot()
        local out = {}
        for name, field in self.fields do
            out[name] = field.value:peek()
        end
        return out
    end
    function self.submit()
        attempted:set(true)
        if self.pending:peek() then
            return
        end
        -- The first field wrong, as it shows (top, then left), takes focus.
        local first, fy, fx = nil, math.huge, math.huge
        for _, field in self.fields do
            if field.problem:peek() ~= nil then
                local el = field.ref:peek()
                local y, x = if el then el.y else math.huge, if el then el.x else math.huge
                if first == nil or y < fy or (y == fy and x < fx) then
                    first, fy, fx = field, y, x
                end
            end
        end
        if first then
            local el = first.ref:peek()
            if el then
                el:focus()
            end
            return
        end
        if not spec.submit then
            return
        end
        local values = self.snapshot()
        self.failure:set(nil)
        self.pending:set(true)
        running = ui.task(function()
            local ok, result = pcall(spec.submit, values)
            running = nil
            self.pending:set(false)
            if ok then
                if spec.on_success then
                    spec.on_success(result, values)
                end
            else
                self.failure:set(message(result))
            end
        end)
    end
    -- Stop a submit under way: what it awaits is not heard.
    function self.cancel()
        if running then
            running:cancel()
            running = nil
            self.pending:set(false)
        end
    end
    -- A form gone (its component, a reload) submits nothing more.
    ui.on_cleanup(function()
        if running then
            running:cancel()
        end
    end)
    function self.reset(to)
        ui.batch(function()
            for name, field in self.fields do
                local value = if to and to[name] ~= nil then to[name] else spec.fields[name]
                field.value:set(value)
                field.touched:set(false)
            end
            attempted:set(false)
            self.failure:set(nil)
        end)
    end
    return self
end

local function bind(props, enter_submits)
    local field = props.field
    props.field = nil
    if type(field) ~= "table" or field.value == nil or field.form == nil then
        error("`field` takes one of a ui.form's fields (`form.fields.email`)", 3)
    end
    if props.value == nil then
        props.value = field.value
    end
    if props.error == nil then
        props.error = field.error
    end
    if props.ref == nil then
        props.ref = field.ref
    end
    local blur = props.on_blur
    if blur ~= nil and type(blur) ~= "function" then
        error("with `field`, `on_blur` is a function", 3)
    end
    props.on_blur = function()
        field.touched:set(true)
        if blur then
            blur()
        end
    end
    if enter_submits then
        local key = props.on_key
        if key ~= nil and type(key) ~= "function" then
            error("with `field`, `on_key` is a function", 3)
        end
        props.on_key = function(e)
            if key and key(e) then
                return true
            end
            if e.key == "Enter" and not (e.shift or e.alt or e.ctrl or e.meta) then
                field.form.submit()
                return true
            end
            return false
        end
    end
end

return form, bind
"#;

/// The form functions, made the first time a script asks for one (most
/// load none).
fn loaded(lua: &Lua) -> mlua::Result<(Function, Function)> {
    if let Ok(made) = lua.named_registry_value::<Table>(BIND) {
        return Ok((made.raw_get(1)?, made.raw_get(2)?));
    }
    let new_state = lua
        .create_function(|lua, initial: Value| Ok(State::new(env(lua)?.runtime.signal(initial))))?;
    let ui: Table = lua.globals().get("ui")?;
    let (form, bind): (Function, Function) =
        crate::lazy::builtin(lua, "=form", SOURCE)?.call((ui, new_state))?;
    let made = lua.create_sequence_from([form.clone(), bind.clone()])?;
    lua.set_named_registry_value(BIND, made)?;
    Ok((form, bind))
}

/// Installs `ui.form`.
pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set(
        "form",
        lua.create_function(|lua, spec: mlua::Value| loaded(lua)?.0.call::<Table>(spec))?,
    )
}

/// A control's `field = form.fields.name`: its value, leaving, error and
/// (`enter_submits`, a one-line input) Enter, bound to the form's field,
/// written into its props before they are read.
pub(crate) fn apply_field(lua: &Lua, props: &Table, enter_submits: bool) -> mlua::Result<()> {
    if props.raw_get::<mlua::Value>("field")?.is_nil() {
        return Ok(());
    }
    loaded(lua)?.1.call((props.clone(), enter_submits))
}
