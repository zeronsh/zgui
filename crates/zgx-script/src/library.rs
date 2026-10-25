//! The component library's `ui.*` (`zgx-components`), behind the
//! `components` feature (on by default): the data grid, tree, calendar,
//! date picker and time field, combobox, select, tabs, command palette,
//! segmented control, stepper, disclosure, accordion and separator, each a
//! [`Library`] kind of node, built by [`register`]'s functions and lowered by
//! [`lower`]. Toasts (`ui.toast`) are `data.rs`'s.
use crate::{
    Vm,
    lower::{Lowering, bridge, lower, need, shortcut_label},
    node::{Kind, Node, expect_node, expect_state, fill, label, node_ud, owner},
    value::Value,
};
use mlua::{Function, Lua, Table};
use std::rc::Rc;
use zgx::{
    compose::{Context, View, prelude::*},
    reactive::Signal,
};
use zgx_components::*;

/// A component of the library, as a node of the interface.
pub(crate) enum Library {
    /// `ui.tabs { items = state, value = state, panel = fn(id), … }`.
    Tabs {
        label: String,
        items: Signal<Value>,
        value: Signal<Value>,
        reorderable: bool,
        on_close: Option<Function>,
        panel: Function,
    },
    /// `ui.select { value = state, options = { … }, placeholder }`.
    Select {
        label: String,
        value: Signal<Value>,
        /// Each option's value, what it shows, and whether it is disabled.
        options: Vec<(Value, String, bool)>,
        placeholder: String,
    },
    /// `ui.combobox { text = state, items = state, value?, loading?, … }`.
    Combobox {
        label: String,
        text: Signal<Value>,
        value: Option<Signal<Value>>,
        items: Signal<Value>,
        loading: Option<Signal<Value>>,
        empty: Option<String>,
    },
    /// `ui.command_palette { open = state, commands?, on_run? }`: the app's
    /// commands, unless it lists its own.
    CommandPalette {
        open: Signal<Value>,
        commands: Option<Vec<PaletteCommand>>,
        on_run: Option<Function>,
    },
    /// `ui.tree { items = state, open = state, value = state, … }`.
    Tree {
        label: String,
        items: Signal<Value>,
        open: Signal<Value>,
        value: Signal<Value>,
        on_open: Option<Function>,
        on_activate: Option<Function>,
    },
    /// `ui.data_grid { columns, rows = state, sort?, value?, … }`.
    DataGrid {
        label: String,
        /// A state (resizing and moving write it), or the columns as given.
        columns: Result<Signal<Value>, Value>,
        rows: Signal<Value>,
        sort: Option<Signal<Value>>,
        value: Option<Signal<Value>>,
        /// The selected rows, indices into `rows` from 1.
        selection: Option<Signal<Value>>,
        /// A cell's text from its value, column and record.
        cell: Option<Function>,
        on_edit: Option<Function>,
        on_activate: Option<Function>,
    },
    /// `ui.calendar { value = state, … }`, and `ui.date_picker` (a button
    /// opening one): a date as `"2026-10-23"`.
    Calendar {
        label: String,
        value: Signal<Value>,
        picker: bool,
        min: Option<String>,
        max: Option<String>,
        first_weekday: u8,
        unavailable: Option<Function>,
        format: Option<Function>,
    },
    /// `ui.time_field { value = state, hour12?, minute_step? }`: a time as
    /// `"09:30"`.
    TimeField {
        label: String,
        value: Signal<Value>,
        hour12: bool,
        minute_step: u8,
    },
    /// `ui.separator { vertical? }`: a line between groups.
    Separator { vertical: bool },
    /// `ui.segmented { value = state, options = { … } }`: one of a few.
    Segmented {
        label: String,
        value: Signal<Value>,
        /// Each option's value and what it shows.
        options: Vec<(Value, String)>,
    },
    /// `ui.stepper { value = state, min, max, step, decimals }`.
    Stepper {
        label: String,
        value: Signal<Value>,
        options: StepperOptions,
    },
    /// `ui.disclosure { open = state, summary = "…", content… }`.
    Disclosure {
        open: Signal<Value>,
        summary: Summary,
    },
    /// `ui.accordion { open = state, ui.section { … }… }`: sections, one
    /// open at a time (`open` its id, or nil).
    Accordion { open: Signal<Value> },
    /// `ui.section { id, title, content… }`: an accordion's section.
    Section { id: Value, title: String },
}
impl Library {
    /// Its name in `ui` (the element's tag).
    pub(crate) fn tag(&self) -> &'static str {
        match self {
            Library::Separator { .. } => "separator",
            Library::Tabs { .. } => "tabs",
            Library::Tree { .. } => "tree",
            Library::DataGrid { .. } => "data_grid",
            Library::Calendar { picker: false, .. } => "calendar",
            Library::Calendar { picker: true, .. } => "date_picker",
            Library::TimeField { .. } => "time_field",
            Library::Combobox { .. } => "combobox",
            Library::CommandPalette { .. } => "command_palette",
            Library::Select { .. } => "select",
            Library::Segmented { .. } => "segmented",
            Library::Stepper { .. } => "stepper",
            Library::Disclosure { .. } => "disclosure",
            Library::Accordion { .. } => "accordion",
            Library::Section { .. } => "section",
        }
    }
    /// Whether it holds children (content, sections).
    pub(crate) fn takes_children(&self) -> bool {
        matches!(
            self,
            Library::Disclosure { .. } | Library::Accordion { .. } | Library::Section { .. }
        )
    }
    /// Whether it needs a component context (state bridges, effects): all
    /// but a separator and a section.
    pub(crate) fn needs_context(&self) -> bool {
        !matches!(self, Library::Separator { .. } | Library::Section { .. })
    }
}

/// An accordion's section's id and title, if `kind` is one.
pub(crate) fn section(kind: &Kind) -> Option<(&Value, &String)> {
    match kind {
        Kind::Library(library) => match &**library {
            Library::Section { id, title } => Some((id, title)),
            _ => None,
        },
        _ => None,
    }
}

/// Add the library's functions to `ui`.
pub(crate) fn register(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    // `ui.<widget> { label?, value = state, … }`.
    let widget = |build: fn(String, Signal<Value>, &Table) -> mlua::Result<Kind>| {
        lua.create_function(move |lua, table: Table| {
            crate::form::apply_field(lua, &table, false)?;
            let label: Option<String> = table.get("label")?;
            let value = expect_state(&table.get("value")?, "`value`")?;
            let kind = build(label.unwrap_or_default(), value, &table)?;
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })
    };
    ui.set("toast", lua.create_function(toast)?)?;
    ui.set(
        "dismiss_toast",
        lua.create_function(|lua, id: u64| {
            let runtime = crate::node::env(lua)?.runtime.clone();
            Toaster::of(&runtime).dismiss(id);
            Ok(())
        })?,
    )?;
    ui.set(
        "tabs",
        lua.create_function(|lua, table: Table| {
            let items = expect_state(&table.get("items")?, "ui.tabs's `items`")?;
            // Not the `items` alignment style: the tabs.
            table.raw_remove("items")?;
            let value = expect_state(&table.get("value")?, "ui.tabs's `value`")?;
            let panel: Function = table
                .get("panel")
                .map_err(|_| mlua::Error::runtime("ui.tabs needs `panel`, fn(id) -> node"))?;
            let kind = Kind::Library(Box::new(Library::Tabs {
                label: label(&table)?,
                items,
                value,
                reorderable: table.get::<Option<bool>>("reorderable")?.unwrap_or(false),
                on_close: table.get("on_close")?,
                panel,
            }));
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "select",
        widget(|label, value, t| {
            let list: Table = t
                .get("options")
                .map_err(|_| mlua::Error::runtime("ui.select needs `options`"))?;
            let mut options = Vec::new();
            for option in list.sequence_values::<mlua::Value>() {
                options.push(match option? {
                    mlua::Value::Table(o) => {
                        let shown: String = o.get("label")?;
                        let value = match o.get::<mlua::Value>("value")? {
                            mlua::Value::Nil => Value::Str(shown.as_str().into()),
                            v => crate::value::convert(v, None)?,
                        };
                        let disabled = o.get::<Option<bool>>("disabled")?.unwrap_or(false);
                        (value, shown, disabled)
                    }
                    other => {
                        let value = crate::value::convert(other, None)?;
                        let shown = value.to_string();
                        (value, shown, false)
                    }
                });
            }
            Ok(Kind::Library(Box::new(Library::Select {
                label,
                value,
                options,
                placeholder: t.get::<Option<String>>("placeholder")?.unwrap_or_default(),
            })))
        })?,
    )?;
    ui.set(
        "tree",
        lua.create_function(|lua, table: Table| {
            let kind = Kind::Library(Box::new(Library::Tree {
                label: label(&table)?,
                items: expect_state(&table.get("items")?, "ui.tree's `items`")?,
                open: expect_state(&table.get("open")?, "ui.tree's `open`")?,
                value: expect_state(&table.get("value")?, "ui.tree's `value`")?,
                on_open: table.get("on_open")?,
                on_activate: table.get("on_activate")?,
            }));
            // Not the `items` alignment style: the tree's.
            table.raw_remove("items")?;
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "data_grid",
        lua.create_function(|lua, table: Table| {
            let optional_state = |key: &str| -> mlua::Result<Option<Signal<Value>>> {
                match table.get::<mlua::Value>(key)? {
                    mlua::Value::Nil => Ok(None),
                    v => expect_state(&v, &format!("ui.data_grid's `{key}`")).map(Some),
                }
            };
            let columns = match table.get::<mlua::Value>("columns")? {
                v @ mlua::Value::Table(_) => Err(crate::value::convert(v, None)?),
                mlua::Value::Nil => {
                    return Err(mlua::Error::runtime("ui.data_grid needs `columns`"));
                }
                v => Ok(expect_state(&v, "ui.data_grid's `columns`")?),
            };
            let kind = Kind::Library(Box::new(Library::DataGrid {
                label: label(&table)?,
                columns,
                rows: expect_state(&table.get("rows")?, "ui.data_grid's `rows`")?,
                sort: optional_state("sort")?,
                value: optional_state("value")?,
                selection: optional_state("selection")?,
                cell: table.get("cell")?,
                on_edit: table.get("on_edit")?,
                on_activate: table.get("on_activate")?,
            }));
            // Not the `rows` grid style: the grid's.
            table.raw_remove("rows")?;
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    for (name, picker) in [("calendar", false), ("date_picker", true)] {
        ui.set(
            name,
            lua.create_function(move |lua, table: Table| {
                let kind = Kind::Library(Box::new(Library::Calendar {
                    label: label(&table)?,
                    value: expect_state(&table.get("value")?, &format!("ui.{name}'s `value`"))?,
                    picker,
                    min: table.get("min")?,
                    max: table.get("max")?,
                    first_weekday: table.get::<Option<u8>>("first_weekday")?.unwrap_or(0) % 7,
                    unavailable: table.get("unavailable")?,
                    format: table.get("format")?,
                }));
                node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
            })?,
        )?;
    }
    ui.set(
        "time_field",
        lua.create_function(|lua, table: Table| {
            let kind = Kind::Library(Box::new(Library::TimeField {
                label: label(&table)?,
                value: expect_state(&table.get("value")?, "ui.time_field's `value`")?,
                hour12: table.get::<Option<bool>>("hour12")?.unwrap_or(false),
                minute_step: table
                    .get::<Option<u8>>("minute_step")?
                    .unwrap_or(1)
                    .clamp(1, 30),
            }));
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "combobox",
        lua.create_function(|lua, table: Table| {
            crate::form::apply_field(lua, &table, false)?;
            let optional_state = |key: &str| -> mlua::Result<Option<Signal<Value>>> {
                match table.get::<mlua::Value>(key)? {
                    mlua::Value::Nil => Ok(None),
                    v => expect_state(&v, &format!("ui.combobox's `{key}`")).map(Some),
                }
            };
            let kind = Kind::Library(Box::new(Library::Combobox {
                label: label(&table)?,
                text: expect_state(&table.get("text")?, "ui.combobox's `text`")?,
                value: optional_state("value")?,
                items: expect_state(&table.get("items")?, "ui.combobox's `items`")?,
                loading: optional_state("loading")?,
                empty: table.get("empty")?,
            }));
            // Not the `items` alignment style: the options.
            table.raw_remove("items")?;
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "command_palette",
        lua.create_function(|lua, table: Table| {
            let open = expect_state(&table.get("open")?, "ui.command_palette's `open`")?;
            let commands = match table.get::<Option<Table>>("commands")? {
                None => None,
                Some(list) => Some(
                    list.sequence_values::<Table>()
                        .map(|c| palette_command(&c?))
                        .collect::<mlua::Result<Vec<_>>>()?,
                ),
            };
            let kind = Kind::Library(Box::new(Library::CommandPalette {
                open,
                commands,
                on_run: table.get("on_run")?,
            }));
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "separator",
        lua.create_function(|lua, table: Option<Table>| {
            let vertical = match &table {
                Some(t) => t.get::<Option<bool>>("vertical")?.unwrap_or(false),
                None => false,
            };
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Library(Box::new(Library::Separator { vertical })),
                    ),
                    table.as_ref(),
                )?,
            )
        })?,
    )?;
    ui.set(
        "segmented",
        widget(|label, value, t| {
            let list: Table = t
                .get("options")
                .map_err(|_| mlua::Error::runtime("ui.segmented needs `options`"))?;
            let mut options = Vec::new();
            for option in list.sequence_values::<mlua::Value>() {
                options.push(match option? {
                    // `{ value = "w", label = "Week" }`, or the label alone.
                    mlua::Value::Table(o) => {
                        let shown: String = o.get("label")?;
                        let value = match o.get::<mlua::Value>("value")? {
                            mlua::Value::Nil => Value::Str(shown.as_str().into()),
                            v => crate::value::convert(v, None)?,
                        };
                        (value, shown)
                    }
                    other => {
                        let value = crate::value::convert(other, None)?;
                        (value.clone(), value.to_string())
                    }
                });
            }
            if options.is_empty() {
                return Err(mlua::Error::runtime(
                    "ui.segmented needs at least one option",
                ));
            }
            Ok(Kind::Library(Box::new(Library::Segmented {
                label,
                value,
                options,
            })))
        })?,
    )?;
    ui.set(
        "stepper",
        widget(|label, value, t| {
            let defaults = StepperOptions::default();
            let options = StepperOptions {
                min: t.get::<Option<f64>>("min")?.unwrap_or(defaults.min),
                max: t.get::<Option<f64>>("max")?.unwrap_or(defaults.max),
                step: t.get::<Option<f64>>("step")?.unwrap_or(defaults.step),
                decimals: t
                    .get::<Option<f64>>("decimals")?
                    .map_or(defaults.decimals, |d| d.clamp(0., 10.) as usize),
            };
            if !(options.min <= options.max && options.step > 0. && options.step.is_finite()) {
                return Err(mlua::Error::runtime(
                    "ui.stepper needs min <= max and a positive step",
                ));
            }
            Ok(Kind::Library(Box::new(Library::Stepper {
                label,
                value,
                options,
            })))
        })?,
    )?;
    ui.set(
        "disclosure",
        lua.create_function(|lua, table: Table| {
            let open = expect_state(&table.get("open")?, "ui.disclosure's `open`")?;
            let summary = match table.get::<mlua::Value>("summary")? {
                mlua::Value::String(s) => Summary::Text(s.to_string_lossy()),
                mlua::Value::Nil => {
                    return Err(mlua::Error::runtime("ui.disclosure needs a `summary`"));
                }
                node => Summary::Node(expect_node(node, "ui.disclosure's `summary`")?),
            };
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Library(Box::new(Library::Disclosure { open, summary })),
                    ),
                    Some(&table),
                )?,
            )
        })?,
    )?;
    ui.set(
        "accordion",
        lua.create_function(|lua, table: Table| {
            let open = expect_state(&table.get("open")?, "ui.accordion's `open`")?;
            let node = fill(
                lua,
                Node::new(
                    owner(lua)?,
                    Kind::Library(Box::new(Library::Accordion { open })),
                ),
                Some(&table),
            )?;
            if let Some(other) = node.children.iter().find(|c| section(&c.kind).is_none()) {
                return Err(mlua::Error::runtime(format!(
                    "ui.accordion holds ui.section nodes, not {}",
                    other.tag()
                )));
            }
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "section",
        lua.create_function(|lua, table: Table| {
            let id = match table.get::<mlua::Value>("id")? {
                mlua::Value::Nil => return Err(mlua::Error::runtime("ui.section needs an `id`")),
                v => crate::value::convert(v, None)?,
            };
            let title: String = table
                .get("title")
                .map_err(|_| mlua::Error::runtime("ui.section needs a `title`"))?;
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Library(Box::new(Library::Section { id, title })),
                    ),
                    Some(&table),
                )?,
            )
        })?,
    )?;
    Ok(())
}

/// The view `library`, a node of the interface, lowers to.
pub(crate) fn view(
    library: &Library,
    node: &Rc<Node>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    mut cx: Option<&mut Context>,
) -> View {
    match library {
        Library::Separator { vertical } => separator(*vertical),
        Library::Tree {
            label,
            items,
            open,
            value,
            on_open,
            on_activate,
        } => {
            let cx = need(&mut cx);
            let open_signal = bridge(
                cx,
                open,
                |v| v.list().iter().map(Value::to_string).collect::<Vec<_>>(),
                |ids: Vec<String>| {
                    Value::List(ids.into_iter().map(|s| Value::Str(s.into())).collect())
                },
            );
            let selected = bridge(
                cx,
                value,
                |v| (!matches!(v, Value::Nil)).then(|| v.to_string()),
                |s: Option<String>| s.map_or(Value::Nil, |s| Value::Str(s.into())),
            );
            // An item: `{ id, label, children?, lazy? }`; one lazy without
            // children is loading.
            fn find<'v>(list: &'v [Value], id: &str) -> Option<&'v Value> {
                for item in list {
                    if item.field("id").map(Value::to_string).as_deref() == Some(id) {
                        return Some(item);
                    }
                    if let Some(found) = item.field("children").and_then(|c| find(c.list(), id)) {
                        return Some(found);
                    }
                }
                None
            }
            fn items_of(list: &[Value]) -> Vec<TreeItem> {
                list.iter()
                    .map(|item| {
                        let id = item.field("id").map(Value::to_string).unwrap_or_default();
                        let shown = item
                            .field("label")
                            .map(Value::to_string)
                            .unwrap_or_else(|| id.clone());
                        let parent = item.field("children").is_some_and(|c| !c.list().is_empty())
                            || item.field("lazy").is_some_and(Value::truthy);
                        TreeItem {
                            id,
                            label: shown,
                            parent,
                        }
                    })
                    .collect()
            }
            let source = items.clone();
            let children = move |parent: Option<&str>| {
                source.with(|v| match parent {
                    None => TreeChildren::Ready(items_of(v.list())),
                    Some(id) => match find(v.list(), id).and_then(|item| item.field("children")) {
                        Some(c) if !c.list().is_empty() => TreeChildren::Ready(items_of(c.list())),
                        _ if find(v.list(), id)
                            .is_some_and(|i| i.field("lazy").is_some_and(Value::truthy)) =>
                        {
                            TreeChildren::Loading
                        }
                        _ => TreeChildren::Ready(Vec::new()),
                    },
                })
            };
            // A lazy item opened without its children: the app is asked.
            if let Some(on_open) = on_open.clone() {
                let (open, vm, runtime) = (open_signal.clone(), vm.clone(), cx.runtime());
                let mut asked: std::collections::HashSet<String> = Default::default();
                let quiet = runtime.clone();
                cx.retain(runtime.effect(move || {
                    let now = open.get();
                    for id in now
                        .iter()
                        .filter(|id| !asked.contains(*id))
                        .cloned()
                        .collect::<Vec<_>>()
                    {
                        asked.insert(id.clone());
                        quiet.untracked(|| {
                            if let Err(error) = vm.call::<()>(&on_open, id) {
                                vm.report(error);
                            }
                        });
                    }
                    asked.retain(|id| now.contains(id));
                }));
            }
            let (on_activate, vm) = (on_activate.clone(), vm.clone());
            tree(label.clone(), children, open_signal, selected, move |id| {
                if let Some(f) = &on_activate
                    && let Err(error) = vm.call::<()>(f, id)
                {
                    vm.report(error);
                }
            })
        }
        Library::DataGrid {
            label,
            columns,
            rows,
            sort,
            value,
            selection,
            cell,
            on_edit,
            on_activate,
        } => {
            use crate::value::PathKey;
            let cx = need(&mut cx);
            // A column: `{ id, title?, width?, min_width?, sortable?,
            // editable?, align? }`.
            fn column_of(v: &Value) -> Option<GridColumn> {
                let id = v.field("id")?.to_string();
                let title = v
                    .field("title")
                    .map_or_else(|| id.clone(), Value::to_string);
                let width = v.field("width").and_then(Value::as_f64).unwrap_or(120.);
                let mut column = GridColumn::new(id, title, width as f32);
                if let Some(min) = v.field("min_width").and_then(Value::as_f64) {
                    column.min_width = min as f32;
                }
                column.sortable = v.field("sortable").is_some_and(Value::truthy);
                column.editable = v.field("editable").is_some_and(Value::truthy);
                column.align_end = v.field("align").is_some_and(|a| a.to_string() == "end");
                Some(column)
            }
            fn columns_of(v: &Value) -> Vec<GridColumn> {
                v.list().iter().filter_map(column_of).collect()
            }
            fn sort_of(v: &Value) -> Option<GridSort> {
                Some(GridSort {
                    column: v.field("column")?.to_string(),
                    descending: v.field("descending").is_some_and(Value::truthy),
                })
            }
            fn sort_value(sort: Option<GridSort>) -> Value {
                let Some(sort) = sort else {
                    return Value::Nil;
                };
                Value::Nil
                    .set_path(
                        &[PathKey::Key("column".into())],
                        Value::Str(sort.column.as_str().into()),
                    )
                    .and_then(|v| {
                        v.set_path(
                            &[PathKey::Key("descending".into())],
                            Value::Bool(sort.descending),
                        )
                    })
                    .unwrap_or(Value::Nil)
            }
            // Numbers by value, text without case, then the rest; nil last.
            fn compare(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
                let rank = |v: Option<&Value>| match v {
                    Some(Value::Num(_) | Value::Bool(_)) => 0,
                    Some(Value::Str(_)) => 1,
                    None | Some(Value::Nil) => 3,
                    _ => 2,
                };
                match (a, b) {
                    (Some(Value::Num(x)), Some(Value::Num(y))) => x.total_cmp(y),
                    (Some(Value::Bool(x)), Some(Value::Bool(y))) => x.cmp(y),
                    (Some(Value::Str(x)), Some(Value::Str(y))) => x
                        .chars()
                        .flat_map(char::to_lowercase)
                        .cmp(y.chars().flat_map(char::to_lowercase))
                        .then_with(|| x.cmp(y)),
                    _ => rank(a).cmp(&rank(b)),
                }
            }
            let columns_state = match columns {
                Ok(state) => state.clone(),
                Err(given) => cx.state(given.clone()),
            };
            // Written back in their new order and widths, all else as the
            // app had it.
            let written = columns_state.clone();
            let columns = bridge_with(
                cx,
                &columns_state,
                columns_of,
                move |next: Vec<GridColumn>| {
                    written.with_untracked(|old| {
                        let items: Vec<Value> = next
                            .iter()
                            .filter_map(|c| {
                                let item = old.list().iter().find(|o| {
                                    o.field("id").map(Value::to_string).as_deref() == Some(&*c.id)
                                })?;
                                item.set_path(
                                    &[PathKey::Key("width".into())],
                                    Value::Num(c.width as f64),
                                )
                                .ok()
                            })
                            .collect();
                        Value::List(items.into())
                    })
                },
            );
            let sort = match sort {
                Some(state) => bridge(cx, state, sort_of, sort_value),
                None => cx.state(None),
            };
            // The rows as they show: `rows` sorted, by their indices (none:
            // as they are).
            let order: Signal<Option<Rc<Vec<usize>>>> = cx.state(None);
            {
                let (rows, sort, order) = (rows.clone(), sort.clone(), order.clone());
                cx.retain(cx.runtime().effect(move || {
                    let next = sort.get().map(|by| {
                        rows.with(|v| {
                            let list = v.list();
                            let mut index: Vec<usize> = (0..list.len()).collect();
                            index.sort_by(|&a, &b| {
                                let (a, b) = (list[a].field(&by.column), list[b].field(&by.column));
                                if by.descending {
                                    compare(b, a)
                                } else {
                                    compare(a, b)
                                }
                            });
                            Rc::new(index)
                        })
                    });
                    order.set(next);
                }));
            }
            // A row shown's index in `rows`, from 0.
            let original = {
                let order = order.clone();
                move |r: usize| {
                    order.with_untracked(|o| {
                        o.as_ref().map_or(r, |o| o.get(r).copied().unwrap_or(r))
                    })
                }
            };
            let (count, cells, cell_order) = (rows.clone(), rows.clone(), order.clone());
            let (format, format_vm) = (cell.clone(), vm.clone());
            let mut source = GridSource::new(
                move || count.with(|v| v.list().len()),
                move |r, column| {
                    let i = cell_order
                        .with(|o| o.as_ref().map_or(r, |o| o.get(r).copied().unwrap_or(r)));
                    let Some(format) = &format else {
                        return cells.with(|v| {
                            v.list()
                                .get(i)
                                .and_then(|row| row.field(column))
                                .map(Value::to_string)
                                .unwrap_or_default()
                        });
                    };
                    // `cell(value, column, record)`: the app's text for it.
                    let record = cells
                        .with(|v| v.list().get(i).cloned())
                        .unwrap_or(Value::Nil);
                    let value = record.field(column).cloned().unwrap_or(Value::Nil);
                    format_vm
                        .call::<String>(format, (value.clone(), column.to_owned(), record))
                        .unwrap_or_else(|error| {
                            format_vm.report(error);
                            value.to_string()
                        })
                },
            );
            {
                // Told to `on_edit`, or written into the row: a number stays
                // one if the text is one.
                let (rows, vm, on_edit, at) =
                    (rows.clone(), vm.clone(), on_edit.clone(), original.clone());
                source = source.on_edit(move |r, column, text| {
                    let i = at(r);
                    if let Some(f) = &on_edit {
                        if let Err(error) = vm.call::<()>(f, (i + 1, column.to_owned(), text)) {
                            vm.report(error);
                        }
                        return;
                    }
                    let path = [PathKey::Index(i + 1), PathKey::Key(column.into())];
                    let old = rows.with_untracked(|v| v.at(&path).cloned());
                    let leaf = match old {
                        Some(Value::Num(_)) if text.trim().is_empty() => Value::Nil,
                        Some(Value::Num(_)) => text
                            .trim()
                            .parse::<f64>()
                            .map_or_else(|_| Value::Str(text.as_str().into()), Value::Num),
                        Some(Value::Bool(_)) if text == "true" || text == "false" => {
                            Value::Bool(text == "true")
                        }
                        _ => Value::Str(text.as_str().into()),
                    };
                    match rows.with_untracked(|v| v.set_path(&path, leaf)) {
                        Ok(next) => {
                            rows.set(next);
                        }
                        Err(error) => vm.report(mlua::Error::runtime(error)),
                    }
                });
            }
            if let Some(f) = on_activate.clone() {
                let (vm, at) = (vm.clone(), original.clone());
                source = source.on_activate(move |r| {
                    if let Err(error) = vm.call::<()>(&f, at(r) + 1) {
                        vm.report(error);
                    }
                });
            }
            // The current cell, its row an index into `rows` from 1. That
            // state is the truth: as the rows' order changes, the cell
            // follows its record (as a sorted list keeps its selection).
            let state = match value {
                Some(state) => state.clone(),
                None => cx.state(Value::Nil),
            };
            fn cell_of(v: &Value, order: &Option<Rc<Vec<usize>>>) -> Option<GridCell> {
                let row = v.field("row").and_then(Value::as_f64)? as usize;
                let column = v.field("column")?.to_string();
                let r = match order {
                    Some(o) => o.iter().position(|&i| i + 1 == row),
                    None => row.checked_sub(1),
                }?;
                Some(GridCell::new(r, column))
            }
            fn value_of(cell: &Option<GridCell>, order: &Option<Rc<Vec<usize>>>) -> Value {
                let Some(cell) = cell else {
                    return Value::Nil;
                };
                let i = order
                    .as_ref()
                    .map_or(cell.row, |o| o.get(cell.row).copied().unwrap_or(cell.row));
                Value::Nil
                    .set_path(&[PathKey::Key("row".into())], Value::Num((i + 1) as f64))
                    .and_then(|v| {
                        v.set_path(
                            &[PathKey::Key("column".into())],
                            Value::Str(cell.column.as_str().into()),
                        )
                    })
                    .unwrap_or(Value::Nil)
            }
            let current: Signal<Option<GridCell>> =
                cx.state(state.with_untracked(|v| order.with_untracked(|o| cell_of(v, o))));
            {
                let (state, order, current) = (state.clone(), order.clone(), current.clone());
                cx.retain(cx.runtime().effect(move || {
                    let o = order.get();
                    current.set(state.with(|v| cell_of(v, &o)));
                }));
            }
            {
                let (state, order, current) = (state.clone(), order.clone(), current.clone());
                let mut first = true;
                cx.retain(cx.runtime().effect(move || {
                    let cell = current.get();
                    if !std::mem::take(&mut first) {
                        state.set(order.with_untracked(|o| value_of(&cell, o)));
                    }
                }));
            }
            // The selected rows, indices into `rows` from 1: that state is the
            // truth, the rows following their records as the order changes.
            if let Some(state) = selection {
                fn shown_of(v: &Value, order: &Option<Rc<Vec<usize>>>) -> GridSelection {
                    let given = v
                        .list()
                        .iter()
                        .filter_map(Value::as_f64)
                        .filter(|i| *i >= 1.)
                        .map(|i| i as usize - 1);
                    match order {
                        // Each record's place as shown.
                        Some(o) => {
                            let mut place = vec![usize::MAX; o.len()];
                            for (shown, &i) in o.iter().enumerate() {
                                place[i] = shown;
                            }
                            GridSelection::from_rows(
                                given.filter_map(|i| {
                                    place.get(i).copied().filter(|p| *p != usize::MAX)
                                }),
                            )
                        }
                        None => GridSelection::from_rows(given),
                    }
                }
                let chosen = cx.state(GridSelection::default());
                {
                    let (state, order, chosen) = (state.clone(), order.clone(), chosen.clone());
                    cx.retain(cx.runtime().effect(move || {
                        let o = order.get();
                        let next = state.with(|v| shown_of(v, &o));
                        // The same rows keep what Shift extends from.
                        if !chosen.with_untracked(|c| c.rows().eq(next.rows())) {
                            chosen.set(next);
                        }
                    }));
                }
                {
                    let (state, order, chosen) = (state.clone(), order.clone(), chosen.clone());
                    let mut first = true;
                    cx.retain(cx.runtime().effect(move || {
                        let rows: Vec<usize> = chosen.with(|c| c.rows().collect());
                        if std::mem::take(&mut first) {
                            return;
                        }
                        let records = order.with_untracked(|o| {
                            rows.iter()
                                .map(|&r| {
                                    let i =
                                        o.as_ref().map_or(r, |o| o.get(r).copied().unwrap_or(r));
                                    Value::Num((i + 1) as f64)
                                })
                                .collect::<Vec<_>>()
                        });
                        let same = state.with_untracked(|v| state_rows_equal(v, &records));
                        if !same {
                            state.set(Value::List(records.into()));
                        }
                    }));
                }
                source = source.selection(chosen);
            }
            data_grid(label.clone(), columns, sort, current, source)
        }
        Library::Calendar {
            label,
            value,
            picker,
            min,
            max,
            first_weekday,
            unavailable,
            format,
        } => {
            let cx = need(&mut cx);
            // A date as `"2026-10-23"`; anything else is none.
            let date = bridge(
                cx,
                value,
                |v| match v {
                    Value::Str(s) => CalendarDate::parse(s),
                    _ => None,
                },
                |d: Option<CalendarDate>| {
                    d.map_or(Value::Nil, |d| Value::Str(d.to_string().as_str().into()))
                },
            );
            let mut options = CalendarOptions {
                min: min.as_deref().and_then(CalendarDate::parse),
                max: max.as_deref().and_then(CalendarDate::parse),
                first_weekday: *first_weekday,
                ..Default::default()
            };
            if let Some(f) = unavailable.clone() {
                let vm = vm.clone();
                options.disabled = Some(Rc::new(move |d: CalendarDate| {
                    vm.call::<bool>(&f, (d.to_string(), d.weekday()))
                        .unwrap_or_else(|error| {
                            vm.report(error);
                            false
                        })
                }));
            }
            if let Some(f) = format.clone() {
                let vm = vm.clone();
                options.format = Some(Rc::new(move |d: CalendarDate| {
                    vm.call::<String>(&f, d.to_string())
                        .unwrap_or_else(|error| {
                            vm.report(error);
                            d.to_string()
                        })
                }));
            }
            if *picker {
                date_picker(label.clone(), date, options)
            } else {
                calendar(label.clone(), date, options)
            }
        }
        Library::TimeField {
            label,
            value,
            hour12,
            minute_step,
        } => {
            let cx = need(&mut cx);
            // A time as `"09:30"`; anything else is none.
            let time = bridge(
                cx,
                value,
                |v| match v {
                    Value::Str(s) => TimeOfDay::parse(s),
                    _ => None,
                },
                |t: Option<TimeOfDay>| {
                    t.map_or(Value::Nil, |t| Value::Str(t.to_string().as_str().into()))
                },
            );
            time_field(
                label.clone(),
                time,
                TimeOptions {
                    minute_step: *minute_step,
                    hour12: *hour12,
                },
            )
        }
        Library::Combobox {
            label,
            text: text_state,
            value,
            items,
            loading,
            empty,
        } => {
            let cx = need(&mut cx);
            let text_signal = bridge(cx, text_state, |v| v.to_string(), |s| Value::Str(s.into()));
            let value_signal = match value {
                Some(value) => bridge(
                    cx,
                    value,
                    |v| (!matches!(v, Value::Nil)).then(|| v.to_string()),
                    |s: Option<String>| s.map_or(Value::Nil, |s| Value::Str(s.into())),
                ),
                None => cx.state(None),
            };
            // Each option: `{ value, label, detail? }`, or text alone.
            let item_of = |item: &Value| match item {
                Value::Map(_) => {
                    let shown = item
                        .field("label")
                        .map(Value::to_string)
                        .unwrap_or_default();
                    let mut option = ComboItem::new(
                        item.field("value").map_or(shown.clone(), Value::to_string),
                        shown,
                    );
                    option.detail = item.field("detail").map(Value::to_string);
                    option
                }
                other => ComboItem::new(other.to_string(), other.to_string()),
            };
            // The list is the app's: followed, never written.
            let items_signal = follow_with(cx, items, move |v: &Value| {
                v.list().iter().map(item_of).collect::<Vec<_>>()
            });
            let mut options = ComboboxOptions::default();
            if let Some(loading) = loading {
                options.loading = Some(bridge(cx, loading, Value::truthy, Value::Bool));
            }
            if let Some(empty) = empty {
                options.empty = empty.clone();
            }
            combobox(
                label.clone(),
                text_signal,
                value_signal,
                items_signal,
                options,
            )
        }
        Library::CommandPalette {
            open,
            commands,
            on_run,
        } => {
            let open = bridge(need(&mut cx), open, Value::truthy, Value::Bool);
            let listed = commands.clone();
            let registry = low.commands.clone();
            let list = move || match &listed {
                Some(list) => list.clone(),
                // The app's menu commands, with their menus and keys.
                None => registry.list.with(|all| {
                    all.iter()
                        .filter(|c| !c.separator && !c.id.is_empty())
                        .map(|c| {
                            let mut command = PaletteCommand::new(c.id.clone(), c.title.clone())
                                .detail(c.menu.clone());
                            command.shortcut = c.key.as_deref().map(shortcut_label);
                            command
                        })
                        .collect()
                }),
            };
            let (on_run, vm, registry) = (on_run.clone(), vm.clone(), low.commands.clone());
            command_palette(open, list, move |id| match &on_run {
                Some(f) => {
                    if let Err(error) = vm.call::<()>(f, id) {
                        vm.report(error);
                    }
                }
                None => {
                    registry.run(id);
                }
            })
        }
        Library::Tabs {
            label,
            items,
            value,
            reorderable,
            on_close,
            panel,
        } => {
            // The script's items, as tabs; written back as its own tables,
            // reordered or removed.
            let source = items.clone();
            let tabs_of = |v: &Value| -> Vec<Tab> {
                v.list()
                    .iter()
                    .map(|item| Tab {
                        id: item.field("id").map(Value::to_string).unwrap_or_default(),
                        label: item
                            .field("label")
                            .map(Value::to_string)
                            .unwrap_or_default(),
                        closable: item.field("closable").is_some_and(Value::truthy),
                    })
                    .collect()
            };
            let tabs_signal = bridge_with(need(&mut cx), items, tabs_of, move |tabs: Vec<Tab>| {
                let old = source.with_untracked(Clone::clone);
                let kept: Vec<Value> = tabs
                    .iter()
                    .filter_map(|tab| {
                        old.list()
                            .iter()
                            .find(|item| {
                                item.field("id").map(Value::to_string).as_deref()
                                    == Some(tab.id.as_str())
                            })
                            .cloned()
                    })
                    .collect();
                Value::List(kept.into())
            });
            let selected = bridge(
                need(&mut cx),
                value,
                |v| v.to_string(),
                |s| Value::Str(s.into()),
            );
            let on_close = on_close.clone().map(|f| {
                let vm = vm.clone();
                let ask: CloseTab = Rc::new(move |id: &str| {
                    !matches!(
                        vm.call::<mlua::Value>(&f, id),
                        Ok(mlua::Value::Boolean(false))
                    )
                });
                ask
            });
            let options = TabsOptions {
                reorderable: *reorderable,
                on_close,
            };
            let (panel, vm, low) = (panel.clone(), vm.clone(), low.clone());
            tabs(
                label.clone(),
                tabs_signal,
                selected,
                options,
                move |id, _| match vm.call::<mlua::Value>(&panel, id) {
                    Ok(mlua::Value::UserData(ud)) if ud.is::<crate::node::NodeUd>() => {
                        match ud.borrow::<crate::node::NodeUd>() {
                            Ok(node) => lower(&node.0, &low),
                            Err(_) => div(),
                        }
                    }
                    Ok(_) => div(),
                    Err(error) => {
                        vm.report(error);
                        div()
                    }
                },
            )
        }
        Library::Select {
            label,
            value,
            options,
            placeholder,
        } => {
            // Options by their value's text; written back as the value itself.
            let values: Rc<[Value]> = options.iter().map(|(v, _, _)| v.clone()).collect();
            let texts: Vec<String> = values.iter().map(Value::to_string).collect();
            let chosen = bridge_with(
                need(&mut cx),
                value,
                |v| v.to_string(),
                move |text: String| {
                    values
                        .iter()
                        .find(|v| v.to_string() == text)
                        .cloned()
                        .unwrap_or_default()
                },
            );
            let list = options
                .iter()
                .zip(texts)
                .map(|((_, shown, disabled), text)| {
                    let option = SelectOption::new(text, shown.clone());
                    if *disabled { option.disabled() } else { option }
                })
                .collect();
            select(label.clone(), chosen, list, placeholder.clone())
        }
        Library::Segmented {
            label,
            value,
            options,
        } => {
            // The chosen option's value, as its index both ways.
            let values: Rc<[Value]> = options.iter().map(|(v, _)| v.clone()).collect();
            let index = {
                let values = values.clone();
                move |v: &Value| values.iter().position(|o| o == v).unwrap_or(0)
            };
            let chosen = bridge_with(need(&mut cx), value, index, move |i: usize| {
                values.get(i).cloned().unwrap_or_default()
            });
            segmented(
                label.clone(),
                chosen,
                options.iter().map(|(_, shown)| shown.clone()),
            )
        }
        Library::Stepper {
            label,
            value,
            options,
        } => stepper(
            label.clone(),
            bridge(
                need(&mut cx),
                value,
                |v| v.as_f64().unwrap_or(0.),
                Value::Num,
            ),
            *options,
        ),
        Library::Disclosure { open, summary } => {
            let open = bridge(need(&mut cx), open, Value::truthy, Value::Bool);
            let summary = match summary {
                Summary::Text(text) => zgx::compose::text(text.clone()),
                Summary::Node(node) => lower(node, low),
            };
            let (children, low) = (node.children.clone(), low.clone());
            disclosure(open, summary, move || {
                column()
                    .gap(8.)
                    .children(children.iter().map(|c| lower(c, &low)))
            })
        }
        Library::Accordion { open } => {
            let ids: Rc<[Value]> = node
                .children
                .iter()
                .map(|c| section(&c.kind).map_or(Value::Nil, |(id, _)| id.clone()))
                .collect();
            let index = {
                let ids = ids.clone();
                move |v: &Value| ids.iter().position(|id| id == v)
            };
            let open = bridge_with(need(&mut cx), open, index, move |i: Option<usize>| {
                i.and_then(|i| ids.get(i).cloned()).unwrap_or_default()
            });
            let sections = node.children.iter().map(|section| {
                let title = crate::library::section(&section.kind)
                    .map_or_else(String::new, |(_, title)| title.clone());
                let (content, low) = (section.children.clone(), low.clone());
                let build: Rc<dyn Fn() -> View> = Rc::new(move || {
                    column()
                        .gap(8.)
                        .children(content.iter().map(|c| lower(c, &low)))
                });
                (title, build)
            });
            accordion(open, sections.collect::<Vec<_>>())
        }
        // Lowered by its accordion.
        Library::Section { .. } => div(),
    }
}

/// `{ id, title?, detail?, shortcut? }`: a command a palette lists.
fn palette_command(c: &Table) -> mlua::Result<PaletteCommand> {
    let id: String = c.get("id")?;
    let title: Option<String> = c.get("title")?;
    let mut command = PaletteCommand::new(id.clone(), title.unwrap_or(id));
    command.detail = c.get("detail")?;
    command.shortcut = c
        .get::<Option<String>>("shortcut")?
        .map(crate::lower::shown_shortcut);
    Ok(command)
}

/// `ui.toast { title, body?, kind?, duration?, action = { label, run }? }`:
/// shows a toast; its id dismisses it (`ui.dismiss_toast`).
fn toast(lua: &Lua, table: Table) -> mlua::Result<u64> {
    let env = crate::node::env(lua)?;
    let (runtime, vm) = (env.runtime.clone(), env.vm.clone());
    drop(env);
    let mut toast = Toast::new(
        table
            .get::<String>("title")
            .map_err(|_| mlua::Error::runtime("ui.toast needs a `title`"))?,
    );
    toast.body = table.get("body")?;
    toast.kind = match table.get::<Option<String>>("kind")?.as_deref() {
        None | Some("info") => ToastKind::Info,
        Some("success") => ToastKind::Success,
        Some("warning") => ToastKind::Warning,
        Some("error") => ToastKind::Error,
        Some(other) => {
            return Err(mlua::Error::runtime(format!(
                "ui.toast's `kind` is \"info\", \"success\", \"warning\" or \"error\", not {other:?}"
            )));
        }
    };
    toast.duration = table
        .get::<Option<f64>>("duration")?
        .map(|ms| std::time::Duration::from_secs_f64(ms.max(0.) / 1000.));
    if let Some(action) = table.get::<Option<Table>>("action")? {
        let label: String = action.get("label")?;
        let run: mlua::Function = action.get("run")?;
        toast = toast.action(label, move || {
            if let Some(vm) = vm.upgrade()
                && let Err(error) = vm.call::<()>(&run, ())
            {
                vm.report(error);
            }
        });
    }
    Ok(Toaster::of(&runtime).show(toast))
}

/// What a disclosure's summary shows: text, or a node of the app's.
pub(crate) enum Summary {
    Text(String),
    Node(Rc<Node>),
}

/// `value` as a typed signal that follows it, one way (an app's list).
fn follow_with<T: PartialEq + Clone + 'static>(
    cx: &mut Context,
    value: &Signal<Value>,
    from: impl Fn(&Value) -> T + 'static,
) -> Signal<T> {
    let typed = cx.state(value.with_untracked(&from));
    let (v, t) = (value.clone(), typed.clone());
    cx.retain(cx.runtime().effect(move || {
        t.set(v.with(&from));
    }));
    typed
}

/// `bridge`, converting with closures (a value to its index among options).
fn bridge_with<T: PartialEq + Clone + 'static>(
    cx: &mut Context,
    value: &Signal<Value>,
    from: impl Fn(&Value) -> T + 'static,
    to: impl Fn(T) -> Value + 'static,
) -> Signal<T> {
    let from = Rc::new(from);
    let typed = cx.state(value.with_untracked(|v| from(v)));
    let runtime = cx.runtime();
    let (v, t, f) = (value.clone(), typed.clone(), from.clone());
    cx.retain(runtime.effect(move || {
        t.set(f(&v.get()));
    }));
    let (v, t) = (value.clone(), typed.clone());
    let mut first = true;
    cx.retain(runtime.effect(move || {
        let next = t.get();
        if !std::mem::take(&mut first) {
            v.set(to(next));
        }
    }));
    typed
}

/// Whether a state's list holds `records`, in any order.
fn state_rows_equal(state: &Value, records: &[Value]) -> bool {
    let mut held: Vec<i64> = state
        .list()
        .iter()
        .filter_map(Value::as_f64)
        .map(|n| n as i64)
        .collect();
    let mut given: Vec<i64> = records
        .iter()
        .filter_map(Value::as_f64)
        .map(|n| n as i64)
        .collect();
    held.sort_unstable();
    given.sort_unstable();
    held == given
}
