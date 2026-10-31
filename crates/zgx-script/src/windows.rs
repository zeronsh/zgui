//! Windows scripts declare: `ui.window { id, title, width, height, open,
//! frame_autosave, tabbing, node }`. A settings window, an inspector: each is open while its `open`
//! state is true, and closing it sets the state false.
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
use mlua::{Lua, Table};
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
        ) == (
            &other.id,
            &other.title,
            other.width,
            other.height,
            &other.frame_autosave,
            &other.tabbing,
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

pub(crate) struct Windows {
    pending: RefCell<Vec<(Window, Rc<Node>)>>,
    /// The published windows' content, by id.
    pub content: RefCell<HashMap<String, Rc<Node>>>,
    pub list: Signal<Vec<Window>>,
}
impl Windows {
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            pending: RefCell::default(),
            content: RefCell::default(),
            list: runtime.signal(Vec::new()),
        }
    }
    /// A load begins: what it declares replaces what is published.
    pub fn begin(&self) {
        self.pending.borrow_mut().clear();
    }
    /// The load succeeded: publish what it declared.
    pub fn publish(&self) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let mut content = HashMap::new();
        let list: Vec<Window> = pending
            .into_iter()
            .map(|(window, node)| {
                content.insert(window.id.clone(), node);
                window
            })
            .collect();
        *self.content.borrow_mut() = content;
        if self.list.with_untracked(|old| *old != list) {
            self.list.set(list);
        }
    }
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
            let node = expect_node(table.get::<mlua::Value>(1)?, &what("content ([1])"))?;
            let number = |key: &str, default: f64| -> mlua::Result<f64> {
                Ok(table.get::<Option<f64>>(key)?.unwrap_or(default))
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
                    Some(tabbing)
                        if !["automatic", "preferred", "disallowed"].contains(&&*tabbing) =>
                    {
                        return Err(mlua::Error::runtime(format!(
                            "{}: \"automatic\", \"preferred\" or \"disallowed\", not \"{tabbing}\"",
                            what("tabbing")
                        )));
                    }
                    tabbing => tabbing,
                },
                id,
            };
            for key in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, _) = key?;
                if let mlua::Value::String(key) = key {
                    let key = key.to_str()?;
                    if ![
                        "id",
                        "title",
                        "width",
                        "height",
                        "open",
                        "frame_autosave",
                        "tabbing",
                    ]
                    .contains(&&*key)
                    {
                        return Err(mlua::Error::runtime(format!(
                            "ui.window `{}`: unknown option `{key}`",
                            window.id
                        )));
                    }
                }
            }
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
    ui.set("window", window)
}
