//! Windows scripts declare: `ui.window { id, title, width, height, open,
//! node }`. A settings window, an inspector: each is open while its `open`
//! state is true, and closing it sets the state false.
//!
//! As commands are, each run declares its windows afresh, and after a
//! successful load the host publishes them ([`crate::ScriptHost::windows`]);
//! the application opens and closes a window as its state says, showing
//! [`crate::ScriptHost::window_view`] in it. The windows share the app's
//! runtime (`Ui::with_runtime`), so their state is the app's.
use crate::{
    node::{Node, expect_node, expect_state},
    value::Value,
};
use mlua::{Lua, Table};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use zgui::reactive::{Runtime, Signal};

/// A window as the application opens it.
#[derive(Clone)]
pub struct Window {
    pub id: String,
    pub title: String,
    pub width: f64,
    pub height: f64,
    /// Open while true: the application opens the window when it turns
    /// true, closes it when it turns false, and sets it false when the user
    /// closes the window.
    pub open: Signal<Value>,
}
impl PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        (&self.id, &self.title, self.width, self.height)
            == (&other.id, &other.title, other.width, other.height)
            && self.open.ptr_eq(&other.open)
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

/// Installs `ui.window`.
pub(crate) fn install(lua: &Lua, ui: &Table, windows: Rc<Windows>) -> mlua::Result<()> {
    ui.set(
        "window",
        lua.create_function(move |_, table: Table| {
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
                id,
            };
            for key in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, _) = key?;
                if let mlua::Value::String(key) = key {
                    let key = key.to_str()?;
                    if !["id", "title", "width", "height", "open"].contains(&&*key) {
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
    )
}
