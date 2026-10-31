//! Windows scripts declare: `ui.window { id, title, width, height, open,
//! frame_autosave, tabbing, under_pointer, node }`. A settings window, an inspector: each is open
//! while its `open` state is true, and closing it sets the state false.
//! `ui.windows(list, build, { key, on_close })` is a window an item of a
//! list: opened as the item appears, closed as it goes.
//!
//! As commands are, each run declares its windows afresh, and after a
//! successful load the host publishes them ([`crate::ScriptHost::windows`]);
//! the application opens and closes a window as its state says, showing
//! [`crate::ScriptHost::window_view`] in it. The windows share the app's
//! runtime (`Ui::with_runtime`), so their state is the app's.
use crate::{
    node::{Node, expect_node, expect_state},
    value::{ReadState, State, Store, Value},
};
use mlua::{Function, Lua, Table};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use zgx::reactive::{Runtime, Signal};

/// A window as the application opens it.
#[derive(Clone)]
pub struct Window {
    /// Its id, as the script declared it (`ui.window { id = … }`).
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its width in logical points.
    pub width: f64,
    /// Its height in logical points.
    pub height: f64,
    /// Open while true: the application opens the window when it turns
    /// true, closes it when it turns false, and sets it false when the user
    /// closes the window.
    pub open: Signal<Value>,
    /// Its frame saved under this name, and reopened where it was left
    /// (macOS).
    pub frame_autosave: Option<String>,
    /// AppKit's window tabs (macOS): `"automatic"` (the user's setting),
    /// `"preferred"` or `"disallowed"`.
    pub tabbing: Option<String>,
    /// Where it opens: the window's top-left this far up and left of the
    /// pointer (a tab torn off, kept under the grab).
    pub under_pointer: Option<(f64, f64)>,
    /// Its own state, which the host keeps as the system says and its
    /// content reads (`win.active`, `win.shown`, `win.size`); none for a
    /// window whose content never asked.
    pub state: Option<WindowState>,
}
/// A window's own `active`, `shown` and `size`, as its host keeps them.
#[derive(Clone)]
pub struct WindowState {
    /// Whether it is the active window, its keys going to it.
    pub active: Signal<Value>,
    /// Whether it shows: not hidden, minimized or covered.
    pub shown: Signal<Value>,
    /// Its size in points, `{ width, height }`.
    pub size: Signal<Value>,
}
impl WindowState {
    fn new(runtime: &Runtime) -> Self {
        Self {
            active: runtime.signal(Value::Bool(false)),
            shown: runtime.signal(Value::Bool(false)),
            size: runtime.signal(size(0., 0.)),
        }
    }
    /// `win` as a script reads it: each read-only.
    fn table(&self, lua: &Lua) -> mlua::Result<Table> {
        let win = lua.create_table()?;
        win.set(
            "active",
            ReadState(State::new(self.active.clone()), "active".into()),
        )?;
        win.set(
            "shown",
            ReadState(State::new(self.shown.clone()), "shown".into()),
        )?;
        win.set(
            "size",
            ReadState(State::new(self.size.clone()), "size".into()),
        )?;
        win.set_readonly(true);
        Ok(win)
    }
}
impl PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        (
            &self.id,
            &self.title,
            self.width,
            self.height,
            &self.frame_autosave,
            &self.tabbing,
            self.under_pointer,
        ) == (
            &other.id,
            &other.title,
            other.width,
            other.height,
            &other.frame_autosave,
            &other.tabbing,
            other.under_pointer,
        ) && self.open.ptr_eq(&other.open)
    }
}
impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("size", &(self.width, self.height))
            .finish()
    }
}

/// `ui.windows(list, build, { key, on_close })`: a window an item.
struct Group {
    list: Signal<Value>,
    build: Function,
    key: Option<Function>,
    on_close: Option<Function>,
    vm: std::rc::Weak<crate::Vm>,
}
/// A group's window: built once, as its item first appears.
struct Built {
    window: Window,
    node: Rc<Node>,
    item: Value,
    /// `on_close` was told the person closed it.
    told: bool,
}

pub(crate) struct Windows {
    runtime: Runtime,
    pending: RefCell<Vec<(Window, Rc<Node>)>>,
    pending_groups: RefCell<Vec<Group>>,
    /// What follows the published groups' lists, while they are published.
    following: RefCell<Option<zgx::reactive::Effect>>,
    /// Errors building windows while `publish` runs: the load's, to show
    /// with its own (later ones are reported as they happen).
    first_errors: RefCell<Option<Vec<String>>>,
    /// The published windows' content, by id.
    pub content: RefCell<HashMap<String, Rc<Node>>>,
    pub list: Signal<Vec<Window>>,
}
impl Windows {
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            runtime: runtime.clone(),
            pending: RefCell::default(),
            pending_groups: RefCell::default(),
            following: RefCell::default(),
            first_errors: RefCell::default(),
            content: RefCell::default(),
            list: runtime.signal(Vec::new()),
        }
    }
    /// A load begins: what it declares replaces what is published.
    pub fn begin(&self) {
        self.pending.borrow_mut().clear();
        self.pending_groups.borrow_mut().clear();
    }
    /// The load succeeded: publish what it declared. What failed to build
    /// as it did (an item's window), for the load's error.
    pub fn publish(self: &Rc<Self>) -> Vec<String> {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let groups = std::mem::take(&mut *self.pending_groups.borrow_mut());
        let mut content = HashMap::new();
        let fixed: Vec<Window> = pending
            .into_iter()
            .map(|(window, node)| {
                content.insert(window.id.clone(), node);
                window
            })
            .collect();
        *self.content.borrow_mut() = content;
        self.following.borrow_mut().take();
        if groups.is_empty() {
            if self.list.with_untracked(|old| *old != fixed) {
                self.list.set(fixed);
            }
            return Vec::new();
        }
        // Each group's windows by id, kept across its list's changes.
        let mut built: Vec<HashMap<String, Built>> =
            groups.iter().map(|_| HashMap::new()).collect();
        let weak = Rc::downgrade(self);
        let runtime = self.runtime.clone();
        *self.first_errors.borrow_mut() = Some(Vec::new());
        let effect = self.runtime.effect(move || {
            let Some(windows) = weak.upgrade() else {
                return;
            };
            let mut list = fixed.clone();
            let mut content: HashMap<String, Rc<Node>> = HashMap::new();
            for (n, group) in groups.iter().enumerate() {
                let Some(vm) = group.vm.upgrade() else {
                    continue;
                };
                let items = group.list.get();
                let mut seen = std::collections::HashSet::new();
                for item in items.list() {
                    let Some(id) = runtime.untracked(|| key_of(&vm, group, n, item)) else {
                        continue;
                    };
                    seen.insert(id.clone());
                    if !built[n].contains_key(&id) {
                        let made = runtime.untracked(|| build(&vm, group, &id, item));
                        match made {
                            Ok((window, node)) => {
                                built[n].insert(
                                    id.clone(),
                                    Built {
                                        window,
                                        node,
                                        item: item.clone(),
                                        told: false,
                                    },
                                );
                            }
                            Err(error) => {
                                match windows.first_errors.borrow_mut().as_mut() {
                                    Some(first) => first.push(error.to_string()),
                                    None => vm.report(error),
                                }
                                continue;
                            }
                        }
                    }
                }
                built[n].retain(|id, _| seen.contains(id));
                for b in built[n].values_mut() {
                    // The person closed it: the app hears, to let the item go.
                    if !b.window.open.get().truthy() && !b.told {
                        b.told = true;
                        if let Some(on_close) = &group.on_close {
                            let item = b.item.clone();
                            runtime.untracked(|| {
                                if let Err(error) = vm.call::<()>(on_close, item) {
                                    vm.report(error);
                                }
                            });
                        }
                    }
                }
                let mut ids: Vec<&String> = built[n].keys().collect();
                ids.sort();
                for id in ids {
                    let b = &built[n][id];
                    content.insert(id.clone(), b.node.clone());
                    list.push(b.window.clone());
                }
            }
            {
                let mut all = windows.content.borrow_mut();
                all.retain(|id, _| fixed.iter().any(|w| &w.id == id));
                all.extend(content);
            }
            if windows.list.with_untracked(|old| *old != list) {
                windows.list.set(list);
            }
        });
        *self.following.borrow_mut() = Some(effect);
        self.first_errors.borrow_mut().take().unwrap_or_default()
    }
}

/// An item's window id: `key(item)`, else its `id` field.
fn key_of(vm: &crate::Vm, group: &Group, n: usize, item: &Value) -> Option<String> {
    let key = match &group.key {
        Some(key) => match vm.call::<mlua::Value>(key, item.clone()) {
            Ok(mlua::Value::String(s)) => s.to_string_lossy(),
            Ok(mlua::Value::Integer(i)) => i.to_string(),
            Ok(mlua::Value::Number(f)) => f.to_string(),
            Ok(_) => return None,
            Err(error) => {
                vm.report(error);
                return None;
            }
        },
        None => item.field("id").map(Value::to_string)?,
    };
    Some(format!("windows{n}:{key}"))
}

/// `build(item)`'s window, as `ui.window`'s options give one.
fn build(
    vm: &crate::Vm,
    group: &Group,
    id: &str,
    item: &Value,
) -> mlua::Result<(Window, Rc<Node>)> {
    // `win` made only for a build that takes it.
    let state = (group.build.info().num_params >= 2).then(|| WindowState::new(&vm.runtime));
    let table: Table = match &state {
        Some(state) => vm.call(&group.build, (item.clone(), state.table(&vm.lua)?))?,
        None => vm.call(&group.build, item.clone())?,
    };
    let open = vm.runtime.signal(Value::Bool(true));
    let (mut window, node) = window_of(&table, id.to_owned(), open, false, None)?;
    window.state = state;
    Ok((window, node))
}

/// A window from `ui.window`'s options; `declared` for `ui.window`'s own,
/// which takes `id` and `open` and checks every key.
fn window_of(
    table: &Table,
    id: String,
    open: Signal<Value>,
    declared: bool,
    runtime: Option<(&Lua, &Runtime)>,
) -> mlua::Result<(Window, Rc<Node>)> {
    let what = |field: &str| format!("ui.window `{id}`'s `{field}`");
    // `ui.window`'s content may be `function(win)`: the window's own state.
    let mut state = None;
    let content = match (table.get::<mlua::Value>(1)?, runtime) {
        (mlua::Value::Function(f), Some((lua, runtime))) => {
            let made = WindowState::new(runtime);
            let node = f.call::<mlua::Value>(made.table(lua)?)?;
            state = Some(made);
            node
        }
        (content, _) => content,
    };
    let node = expect_node(content, &what("content ([1])"))?;
    let number = |key: &str, default: f64| -> mlua::Result<f64> {
        Ok(table.get::<Option<f64>>(key)?.unwrap_or(default))
    };
    let under_pointer = match table.get::<Option<Table>>("under_pointer")? {
        Some(at) => Some((at.get::<f64>(1)?, at.get::<f64>(2)?)),
        None => None,
    };
    let window = Window {
        title: table
            .get::<Option<String>>("title")?
            .unwrap_or_else(|| id.clone()),
        width: number("width", 480.)?,
        height: number("height", 360.)?,
        open,
        frame_autosave: table.get::<Option<String>>("frame_autosave")?,
        tabbing: match table.get::<Option<String>>("tabbing")? {
            Some(tabbing) if !["automatic", "preferred", "disallowed"].contains(&&*tabbing) => {
                return Err(mlua::Error::runtime(format!(
                    "{}: \"automatic\", \"preferred\" or \"disallowed\", not \"{tabbing}\"",
                    what("tabbing")
                )));
            }
            tabbing => tabbing,
        },
        under_pointer,
        state,
        id,
    };
    let known: &[&str] = if declared {
        &[
            "id",
            "title",
            "width",
            "height",
            "open",
            "frame_autosave",
            "tabbing",
            "under_pointer",
        ]
    } else {
        &[
            "title",
            "width",
            "height",
            "frame_autosave",
            "tabbing",
            "under_pointer",
        ]
    };
    for key in table.pairs::<mlua::Value, mlua::Value>() {
        let (key, _) = key?;
        if let mlua::Value::String(key) = key {
            let key = key.to_str()?;
            if !known.contains(&&*key) {
                return Err(mlua::Error::runtime(format!(
                    "ui.window `{}`: unknown option `{key}`",
                    window.id
                )));
            }
        }
    }
    Ok((window, node))
}

/// The shared keys holding the window's title bar area and buttons
/// (`ScriptHost::window_chrome`), which `ui.window.titlebar_area` and
/// `ui.window.controls` read.
pub(crate) const TITLEBAR_AREA: &str = "zgx.window.titlebar_area";
pub(crate) const CONTROLS: &str = "zgx.window.controls";
/// The shared keys holding the app's window's zoom (1 at 100%) and whether
/// its keys zoom it (`ScriptHost::window_zoom`): `ui.window.zoom` and
/// `ui.window.zoomable`, which a script reads and sets.
pub(crate) const ZOOM: &str = "zgx.window.zoom";
pub(crate) const ZOOMABLE: &str = "zgx.window.zoomable";
/// The shared keys holding whether the app's window is the active one and
/// whether it shows (`ScriptHost::window_state`): `ui.window.active` and
/// `ui.window.shown`, which a script reads.
pub(crate) const ACTIVE: &str = "zgx.window.active";
pub(crate) const SHOWN: &str = "zgx.window.shown";
/// The shared key holding the app's window's size in points
/// (`ScriptHost::window_size`): `ui.window.size`, `{ width, height }`,
/// which a script reads.
pub(crate) const SIZE: &str = "zgx.window.size";

/// A window's size as `ui.window.size` holds it.
pub(crate) fn size(width: f32, height: f32) -> Value {
    let n = |v: f32| Value::Num(f64::from(v));
    Value::Map(Rc::from([
        (Rc::from("height"), n(height)),
        (Rc::from("width"), n(width)),
    ]))
}

/// Installs `ui.window`: called, it declares a window; its
/// `titlebar_area` and `controls` are the app's own window's.
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    windows: Rc<Windows>,
    shared: &Rc<Store>,
) -> mlua::Result<()> {
    let windows_for_groups = windows.clone();
    // Where an element shows in the window, through every zoom and
    // transform around it (its own included): `{ x, y, width, height }` in
    // the window's points, as a pointer's `x` and `y` are; nil if it is not
    // laid out. An element's `x`, `y`, `width`, `height` are as laid out.
    // Here, not a method: one more on every element's metatable costs each
    // method lookup.
    ui.set(
        "window_rect",
        lua.create_function(|lua, el: mlua::UserDataRef<crate::element::ElementUd>| {
            let Some(rect) = el.0.read(|s, id| s.transformed_bounds(id)) else {
                return Ok(None);
            };
            let t = lua.create_table()?;
            t.set("x", rect.x)?;
            t.set("y", rect.y)?;
            t.set("width", rect.width)?;
            t.set("height", rect.height)?;
            Ok(Some(t))
        })?,
    )?;
    let window = lua.create_table()?;
    // The app's own window's title bar: the app's, not a plugin's to read
    // or to move.
    let plugin =
        crate::node::env(lua).is_ok_and(|env| matches!(*env.grants, crate::Grants::Only(_)));
    if !plugin {
        window.set(
            "titlebar_area",
            State::new(shared.state(TITLEBAR_AREA, Value::Nil)),
        )?;
        window.set("controls", State::new(shared.state(CONTROLS, Value::Nil)))?;
    }
    let meta = lua.create_table()?;
    // `zoom` and `zoomable` made as a script first reads them: a script that
    // never does makes neither.
    if !plugin {
        let shared = shared.clone();
        meta.set(
            "__index",
            lua.create_function(move |_, (window, key): (Table, mlua::Value)| {
                let name = key.as_string().map(|key| key.to_string_lossy());
                let state = match name.as_deref() {
                    Some("zoom") => shared.state(ZOOM, Value::Num(1.)),
                    Some("zoomable") => shared.state(ZOOMABLE, Value::Bool(true)),
                    // Read-only: the window's, as the system says.
                    Some(read @ ("active" | "shown" | "size")) => {
                        let state = State::new(match read {
                            "active" => shared.state(ACTIVE, Value::Bool(true)),
                            "shown" => shared.state(SHOWN, Value::Bool(true)),
                            _ => shared.state(SIZE, size(0., 0.)),
                        });
                        window.raw_set(key.clone(), ReadState(state, read.into()))?;
                        return window.raw_get::<mlua::Value>(key);
                    }
                    _ => return Ok(mlua::Value::Nil),
                };
                window.raw_set(key.clone(), State::new(state))?;
                window.raw_get::<mlua::Value>(key)
            })?,
        )?;
    }
    meta.set(
        "__call",
        lua.create_function(move |lua, (_, table): (Table, Table)| {
            crate::node::env(lua)?.may(crate::Capability::Windows, "ui.window")?;
            let id: String = table
                .get::<Option<String>>("id")?
                .ok_or_else(|| mlua::Error::runtime("ui.window needs an `id`"))?;
            let what = |field: &str| format!("ui.window `{id}`'s `{field}`");
            let open = expect_state(&table.get::<mlua::Value>("open")?, &what("open"))?;
            let runtime = crate::node::env(lua)?.runtime.clone();
            let (window, node) = window_of(&table, id, open, true, Some((lua, &runtime)))?;
            let mut pending = windows.pending.borrow_mut();
            if pending.iter().any(|(w, _)| w.id == window.id) {
                return Err(mlua::Error::runtime(format!(
                    "ui.window `{}` is declared twice",
                    window.id
                )));
            }
            pending.push((window, node));
            Ok(())
        })?,
    )?;
    window.set_metatable(Some(meta))?;
    ui.set("window", window)?;
    // `ui.windows(list, build, { key, on_close })`.
    let groups = windows_for_groups;
    ui.set(
        "windows",
        lua.create_function(
            move |lua, (list, build, options): (mlua::Value, Function, Option<Table>)| {
                let env = crate::node::env(lua)?;
                env.may(crate::Capability::Windows, "ui.windows")?;
                let list = crate::node::state_of(&list).ok_or_else(|| {
                    mlua::Error::runtime("ui.windows' list must be a ui.state of items")
                })?;
                let (key, on_close) = match &options {
                    Some(options) => (options.get("key")?, options.get("on_close")?),
                    None => (None, None),
                };
                groups.pending_groups.borrow_mut().push(Group {
                    list,
                    build,
                    key,
                    on_close,
                    vm: env.vm.clone(),
                });
                Ok(())
            },
        )?,
    )
}
