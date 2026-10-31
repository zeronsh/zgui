//! Values a script can store in Rust-owned state.
//!
//! A value is an immutable snapshot: lists and maps are shared (`Rc`), so an
//! update builds a new value that shares every subtree it did not touch, and
//! comparing two values (which decides whether state changed) stops at the
//! first shared subtree. Scripts see values as read-only tables and change
//! state through its methods: `set`, `set_key`, `patch`, `update`, `insert`.
use mlua::{FromLua, IntoLua, Lua, Table, UserData, UserDataMethods};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use zgx::reactive::{Runtime, Signal};

/// A script value. State holds these, so equal writes are suppressed like any
/// other signal.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub enum Value {
    /// `nil`.
    #[default]
    Nil,
    /// A boolean.
    Bool(bool),
    /// A number.
    Num(f64),
    /// A string.
    Str(Rc<str>),
    /// A long string (over 64 KiB, as a document is), in chunks: shared
    /// with an editor bound to it and its edits, and a string to scripts.
    Text(zgx::text::Text),
    /// A table used as a list (keys 1 to n).
    List(Rc<[Value]>),
    /// A table with string keys, sorted by key so equal tables compare equal
    /// (and `field` finds them by halves): build one with
    /// [`Value::record`], which sorts them.
    Map(Rc<[(Rc<str>, Value)]>),
    /// An element a script keeps (`ui.use_state(el)`), or a folder a person
    /// picked (`ui.open_folder`): opaque to Rust, equal only to itself, and
    /// `null` as JSON. Sharing a variant keeps a rare handle off every
    /// conversion's match.
    Element(ElementHandle),
    /// An image a script keeps (a `Bitmap`: `ui.bitmap`'s, or the
    /// clipboard's): equal only to itself, and `null` as JSON.
    Image(std::sync::Arc<zgx::image::ImageData>),
}

/// A value's room in a list or map, for `Value::weight`.
const SLOT: usize = std::mem::size_of::<Value>();

/// A mounted (or once mounted) element held in state, or a picked folder.
#[derive(Clone)]
pub struct ElementHandle(pub(crate) Rc<Held>);
/// What an [`ElementHandle`] holds, behind one pointer: `Value`'s other
/// variants are one too, so its clone and drop stay a single path.
pub(crate) enum Held {
    Element(Rc<crate::element::Element>),
    Folder(Rc<crate::folder::Folder>),
}
impl ElementHandle {
    /// Whether the element is in the interface now.
    pub fn is_mounted(&self) -> bool {
        match &*self.0 {
            Held::Element(element) => element.is_mounted(),
            Held::Folder(_) => false,
        }
    }
    /// What it holds, as an address: two handles to one thing share it.
    pub(crate) fn address(&self) -> usize {
        match &*self.0 {
            Held::Element(element) => Rc::as_ptr(element).cast::<()>() as usize,
            Held::Folder(folder) => Rc::as_ptr(folder).cast::<()>() as usize,
        }
    }
}
impl PartialEq for ElementHandle {
    fn eq(&self, other: &Self) -> bool {
        self.address() == other.address()
    }
}
impl std::fmt::Debug for ElementHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &*self.0 {
            Held::Element(element) => match &element.id {
                Some(id) => write!(f, "Element({}#{id})", element.tag),
                None => write!(f, "Element({})", element.tag),
            },
            Held::Folder(_) => write!(f, "Folder"),
        }
    }
}

/// Shared subtrees compare equal without being walked.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => Rc::ptr_eq(a, b) || a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Str(s), Value::Text(t)) | (Value::Text(t), Value::Str(s)) => *t == **s,
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b) || a[..] == b[..],
            (Value::Map(a), Value::Map(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter().zip(b.iter()).all(|((ka, va), (kb, vb))| {
                            (Rc::ptr_eq(ka, kb) || ka == kb) && va == vb
                        }))
            }
            // `{}` is both.
            (Value::List(l), Value::Map(m)) | (Value::Map(m), Value::List(l)) => {
                l.is_empty() && m.is_empty()
            }
            (Value::Element(a), Value::Element(b)) => a == b,
            (Value::Image(a), Value::Image(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// One step of a path into a value: a map's key or a list's 1-based index.
#[derive(Clone, Debug, PartialEq)]
pub enum PathKey {
    /// A map's key.
    Key(Rc<str>),
    /// A list's index, from 1.
    Index(usize),
}
impl std::fmt::Display for PathKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathKey::Key(k) => write!(f, "{k:?}"),
            PathKey::Index(i) => write!(f, "{i}"),
        }
    }
}

impl Value {
    /// As JSON (lists as arrays, maps as objects), for a host to keep.
    pub fn to_json(&self) -> String {
        fn convert(value: &Value) -> serde_json::Value {
            match value {
                Value::Nil | Value::Element(_) | Value::Image(_) => serde_json::Value::Null,
                Value::Bool(b) => serde_json::Value::Bool(*b),
                // Whole numbers as integers: `2`, not `2.0`.
                Value::Num(n) if n.fract() == 0. && n.abs() < 9e15 => {
                    serde_json::Value::Number((*n as i64).into())
                }
                Value::Num(n) => serde_json::Number::from_f64(*n)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number),
                Value::Str(s) => serde_json::Value::String(s.to_string()),
                Value::Text(t) => serde_json::Value::String(t.to_string()),
                Value::List(items) => serde_json::Value::Array(items.iter().map(convert).collect()),
                Value::Map(entries) => serde_json::Value::Object(
                    entries
                        .iter()
                        .map(|(k, v)| (k.to_string(), convert(v)))
                        .collect(),
                ),
            }
        }
        convert(self).to_string()
    }
    /// About what the value holds on the heap, in bytes, for a plugin's
    /// memory limit (`meter`): a list or map its items and their slots, a
    /// string its text. A map's keys count as pointers alone: rows share
    /// them (one string for 100,000 rows' `id`).
    pub(crate) fn weight(&self) -> usize {
        match self {
            Value::Str(s) => 16 + s.len(),
            Value::Text(t) => 16 + t.len(),
            Value::List(items) => 16 + items.iter().map(|v| SLOT + v.weight()).sum::<usize>(),
            Value::Map(fields) => {
                16 + fields
                    .iter()
                    .map(|(_, v)| 16 + SLOT + v.weight())
                    .sum::<usize>()
            }
            Value::Element(_) => 64,
            // Its pixels: a pasted 4K screenshot is 33 MB.
            Value::Image(image) => 64 + image.bytes(),
            Value::Nil | Value::Bool(_) | Value::Num(_) => 0,
        }
    }
    /// JSON as a value (`to_json`'s inverse), or `None` if it is not JSON.
    pub fn from_json(json: &str) -> Option<Value> {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .map(|_| crate::node::json_value(json))
    }
    /// This value, keeping `old`'s own parts wherever they are equal (as
    /// Solid's `reconcile`): what did not change stays the same `Rc`, so
    /// comparing, converting for scripts and `ui.each`'s rows skip it at a
    /// pointer's glance. List items that are tables with an `id` are matched
    /// by it (a row inserted above keeps the rows below theirs); others by
    /// position. Equal throughout, it is `old` itself.
    pub fn reconcile(self, old: &Value) -> Value {
        fn same(a: &Value, b: &Value) -> bool {
            match (a, b) {
                (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
                (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
                (Value::Str(a), Value::Str(b)) => Rc::ptr_eq(a, b),
                (Value::Text(a), Value::Text(b)) => a.ptr_eq(b),
                (a, b) => a == b,
            }
        }
        /// An item's `id`, borrowed: a string, or a number by value (both
        /// zeros alike).
        #[derive(Hash, PartialEq, Eq)]
        enum Id<'a> {
            Str(&'a str),
            Num(u64),
        }
        fn id_of(item: &Value) -> Option<Id<'_>> {
            match item.field("id")? {
                Value::Str(s) => Some(Id::Str(s)),
                Value::Num(n) => Some(Id::Num(if *n == 0. { 0 } else { n.to_bits() })),
                _ => None,
            }
        }
        match (self, old) {
            (Value::List(new), Value::List(was)) => {
                if Rc::ptr_eq(&new, was) {
                    return old.clone();
                }
                // By id when every old item has one; by position when the
                // ids are where they were (the usual update: some items
                // edited in place), with no table of them to build.
                let in_place = new.len() == was.len()
                    && new
                        .iter()
                        .zip(was.iter())
                        .all(|(a, b)| id_of(a) == id_of(b));
                let ids: Option<HashMap<Id<'_>, &Value>> = (was.len() > 1 && !in_place)
                    .then(|| was.iter().map(|v| id_of(v).map(|id| (id, v))).collect())
                    .flatten();
                let mut unchanged = new.len() == was.len();
                let items: Vec<Value> = new
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let prior = match &ids {
                            Some(ids) => id_of(item).and_then(|id| ids.get(&id).copied()),
                            None => was.get(i),
                        };
                        let item = match prior {
                            Some(prior) => item.clone().reconcile(prior),
                            None => item.clone(),
                        };
                        unchanged = unchanged && was.get(i).is_some_and(|w| same(&item, w));
                        item
                    })
                    .collect();
                if unchanged {
                    old.clone()
                } else {
                    Value::List(items.into())
                }
            }
            (Value::Map(new), Value::Map(was)) => {
                // Equal throughout (most of a republished model): the old
                // value, found without building a new one first.
                if Rc::ptr_eq(&new, was) || new == *was {
                    return old.clone();
                }
                // Both sorted by key: walk them together.
                let mut unchanged = new.len() == was.len();
                let mut at = 0;
                let fields: Vec<(Rc<str>, Value)> = new
                    .iter()
                    .map(|(key, value)| {
                        while at < was.len() && *was[at].0 < **key {
                            at += 1;
                        }
                        match was.get(at) {
                            Some((k, v)) if *k == *key => {
                                let value = value.clone().reconcile(v);
                                unchanged = unchanged && same(&value, v);
                                (k.clone(), value)
                            }
                            _ => {
                                unchanged = false;
                                (key.clone(), value.clone())
                            }
                        }
                    })
                    .collect();
                if unchanged {
                    old.clone()
                } else {
                    Value::Map(fields.into())
                }
            }
            (new, old) if new == *old => old.clone(),
            (new, _) => new,
        }
    }
    /// A map of `fields`, in any order: sorted by key, the last of a key
    /// given twice kept (`Value::record([("name", "Ada".into()), ("id", 7.0.into())])`).
    pub fn record<K: Into<Rc<str>>>(fields: impl IntoIterator<Item = (K, Value)>) -> Value {
        let mut fields: Vec<(Rc<str>, Value)> =
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect();
        // Stable: of a key given twice, the later stays last, and is kept.
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let mut kept: Vec<(Rc<str>, Value)> = Vec::with_capacity(fields.len());
        for field in fields {
            match kept.last_mut() {
                Some(last) if last.0 == field.0 => *last = field,
                _ => kept.push(field),
            }
        }
        Value::Map(kept.into())
    }
    /// Whether Luau counts it as true: anything but `nil` and `false`.
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }
    /// It as a number: a number, or a boolean as 1 or 0.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Bool(b) => Some(*b as u8 as f64),
            _ => None,
        }
    }
    /// Its items if it is a list, else none.
    pub fn list(&self) -> &[Value] {
        match self {
            Value::List(items) => items,
            _ => &[],
        }
    }
    /// A field of a map value.
    pub fn field(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(fields) => fields
                .binary_search_by(|(k, _)| (**k).cmp(key))
                .ok()
                .map(|i| &fields[i].1),
            _ => None,
        }
    }
    /// What is at `path`, if anything.
    pub fn at(&self, path: &[PathKey]) -> Option<&Value> {
        let mut value = self;
        for step in path {
            value = match step {
                PathKey::Key(k) => value.field(k)?,
                PathKey::Index(i) => value.list().get(i.checked_sub(1)?)?,
            };
        }
        Some(value)
    }
    /// This value with `new` at `path`, sharing everything off the path. The
    /// result is this value itself (the same `Rc`s) when nothing changes, so
    /// writing it to state notifies nobody. Maps and lists along the path
    /// that do not exist yet are made; `nil` removes a key, or a list's last
    /// item.
    pub fn set_path(&self, path: &[PathKey], new: Value) -> Result<Value, String> {
        let Some((step, rest)) = path.split_first() else {
            return Ok(if new == *self { self.clone() } else { new });
        };
        let kind = self.kind();
        match step {
            PathKey::Key(key) => {
                let fields: &[(Rc<str>, Value)] = match self {
                    Value::Map(fields) => fields,
                    Value::Nil => &[],
                    Value::List(items) if items.is_empty() => &[],
                    _ => return Err(format!("key {step} of {kind}: only tables have keys")),
                };
                let found = fields.binary_search_by(|(k, _)| (**k).cmp(key));
                let old = found.ok().map(|i| &fields[i].1);
                let child = old
                    .unwrap_or(&Value::Nil)
                    .set_path(rest, new)
                    .map_err(|e| format!("{step}: {e}"))?;
                Ok(match (found, child) {
                    (Ok(i), child) if child == fields[i].1 => self.clone(),
                    (Err(_), Value::Nil) => self.clone(),
                    (Ok(i), Value::Nil) => {
                        let mut out = Vec::with_capacity(fields.len() - 1);
                        out.extend_from_slice(&fields[..i]);
                        out.extend_from_slice(&fields[i + 1..]);
                        Value::map(out)
                    }
                    (Ok(i), child) => {
                        let mut out: Vec<_> = fields.to_vec();
                        out[i].1 = child;
                        Value::Map(out.into())
                    }
                    (Err(i), child) => {
                        let mut out = Vec::with_capacity(fields.len() + 1);
                        out.extend_from_slice(&fields[..i]);
                        out.push((key.clone(), child));
                        out.extend_from_slice(&fields[i..]);
                        Value::Map(out.into())
                    }
                })
            }
            PathKey::Index(index) => {
                let items: &[Value] = match self {
                    Value::List(items) => items,
                    Value::Nil => &[],
                    _ => return Err(format!("index {step} of {kind}: only lists have indices")),
                };
                let len = items.len();
                if *index == 0 || *index > len + 1 {
                    return Err(format!(
                        "index {index} is outside the list (1 to {}; use insert or push to grow it)",
                        len + 1
                    ));
                }
                let old = items.get(index - 1);
                let child = old
                    .unwrap_or(&Value::Nil)
                    .set_path(rest, new)
                    .map_err(|e| format!("{step}: {e}"))?;
                Ok(match (old, child) {
                    (Some(old), child) if child == *old => self.clone(),
                    (None, Value::Nil) => self.clone(),
                    (Some(_), Value::Nil) if *index == len => Value::List(items[..len - 1].into()),
                    (Some(_), Value::Nil) => {
                        return Err(format!(
                            "a list has no holes: state:remove({index}) takes item {index} out"
                        ));
                    }
                    (Some(_), child) => {
                        let mut out = items.to_vec();
                        out[index - 1] = child;
                        Value::List(out.into())
                    }
                    (None, child) => {
                        let mut out = Vec::with_capacity(len + 1);
                        out.extend_from_slice(items);
                        out.push(child);
                        Value::List(out.into())
                    }
                })
            }
        }
    }
    /// `set_path`, done in place where each container on the way is held
    /// only here: a long list's row written without copying the list.
    /// `None`, having changed nothing, where it would copy one (it is
    /// shared) or add, remove or grow; else whether anything changed.
    pub(crate) fn set_path_in_place(
        &mut self,
        path: &[PathKey],
        new: &Value,
    ) -> Result<Option<bool>, String> {
        let Some((step, rest)) = path.split_first() else {
            return Ok(None);
        };
        let slot: &mut Value = match (self, step) {
            (Value::List(items), PathKey::Index(i)) if *i >= 1 && *i <= items.len() => {
                let Some(items) = Rc::get_mut(items) else {
                    return Ok(None);
                };
                &mut items[i - 1]
            }
            (Value::Map(fields), PathKey::Key(key)) => {
                let Ok(at) = fields.binary_search_by(|(k, _)| (**k).cmp(key)) else {
                    return Ok(None);
                };
                let Some(fields) = Rc::get_mut(fields) else {
                    return Ok(None);
                };
                &mut fields[at].1
            }
            _ => return Ok(None),
        };
        if rest.is_empty() {
            if matches!(new, Value::Nil) {
                return Ok(None);
            }
            if *slot == *new {
                return Ok(Some(false));
            }
            *slot = new.clone();
            return Ok(Some(true));
        }
        if let Some(changed) = slot.set_path_in_place(rest, new)? {
            return Ok(Some(changed));
        }
        let child = slot
            .set_path(rest, new.clone())
            .map_err(|e| format!("{step}: {e}"))?;
        if matches!(child, Value::Nil) {
            return Ok(None);
        }
        let changed = child != *slot;
        *slot = child;
        Ok(Some(changed))
    }
    /// A map's fields, sorted by key; an empty map is `{}`, as Luau gives it.
    fn map(fields: Vec<(Rc<str>, Value)>) -> Value {
        if fields.is_empty() {
            Value::List(Rc::new([]))
        } else {
            Value::Map(fields.into())
        }
    }
    /// What kind of value this is, for messages.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "a boolean",
            Value::Num(_) => "a number",
            Value::Str(_) | Value::Text(_) => "a string",
            Value::List(_) => "a list",
            Value::Map(_) => "a table",
            Value::Element(_) => "an element",
            Value::Image(_) => "an image",
        }
    }
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Nil | Value::Element(_) | Value::Image(_) => Ok(()),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) if n.fract() == 0. && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => f.write_str(s),
            Value::Text(t) => write!(f, "{t}"),
            Value::List(items) => {
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                Ok(())
            }
            Value::Map(fields) => {
                f.write_str("{")?;
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key} = {value}")?;
                }
                f.write_str("}")
            }
        }
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::Str(value.into())
    }
}
impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Num(value)
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

/// A script value as a state value. Where `old` has an equal part (a
/// string, a subtree), the result shares it, so the new state compares
/// cheaply with the old and keeps one copy of what did not change.
/// `ui.null`: a value that is nil to Rust (JSON's null) but, unlike nil,
/// keeps its key in a table: `{ caps = ui.null }` sends `{"caps": null}`.
static NULL: u8 = 0;
pub(crate) fn null() -> mlua::Value {
    mlua::Value::LightUserData(mlua::LightUserData(
        &NULL as *const u8 as *mut std::ffi::c_void,
    ))
}
/// `ui.object(t?)`: `t` (or a new table) marked a JSON object, so that
/// empty it sends `{}`, not `[]` (a Lua table can't tell). An empty object
/// from Rust comes to a script marked, so it goes back as one.
pub(crate) fn object(lua: &Lua, table: Option<Table>) -> mlua::Result<Table> {
    let table = match table {
        Some(table) => table,
        None => lua.create_table()?,
    };
    table.set_metatable(Some(object_marker(lua)?))?;
    Ok(table)
}
/// The metatable that marks a table a JSON object (`ui.object`).
fn object_marker(lua: &Lua) -> mlua::Result<Table> {
    const KEY: &str = "zgx.json_object";
    if let Ok(marker) = lua.named_registry_value::<Table>(KEY) {
        return Ok(marker);
    }
    let marker = lua.create_table()?;
    marker.raw_set("__json", "object")?;
    lua.set_named_registry_value(KEY, &marker)?;
    Ok(marker)
}
fn is_object(t: &Table) -> bool {
    t.metatable()
        .and_then(|m| {
            m.raw_get::<Option<mlua::LuaString>>("__json")
                .ok()
                .flatten()
        })
        .is_some_and(|kind| kind.as_bytes() == &b"object"[..])
}
pub(crate) fn convert(value: mlua::Value, old: Option<&Value>) -> mlua::Result<Value> {
    convert_in(value, old, 0)
}
/// How many tables deep state nests at most, as serde_json's limit: deeper
/// (or a table holding itself) would overflow the stack converting it.
const DEPTH: u32 = 128;
/// `convert`, `depth` tables in.
fn convert_in(value: mlua::Value, old: Option<&Value>, depth: u32) -> mlua::Result<Value> {
    Ok(match value {
        mlua::Value::Nil => Value::Nil,
        mlua::Value::LightUserData(p) if std::ptr::eq(p.0 as *const u8, &NULL) => Value::Nil,
        mlua::Value::Boolean(b) => Value::Bool(b),
        mlua::Value::Integer(n) => Value::Num(n as f64),
        mlua::Value::Number(n) => Value::Num(n),
        mlua::Value::String(s) => match old {
            Some(Value::Str(o)) if *s.as_bytes() == *o.as_bytes() => Value::Str(o.clone()),
            Some(Value::Text(o)) if same_text(o, &s.as_bytes()) => Value::Text(o.clone()),
            _ if s.as_bytes().len() > zgx::text::LONG => long_string(&s.as_bytes()),
            _ => Value::Str(str_of(&s.as_bytes())),
        },
        mlua::Value::Table(t) => match t
            .weak_lua()
            .try_upgrade()
            .and_then(|lua| converted_from(&t, &lua))
        {
            Some(value) => match old {
                Some(old) if *old == value => old.clone(),
                _ => value,
            },
            None => table(&t, old, depth)?,
        },
        mlua::Value::UserData(ud) if ud.is::<State>() => ud.borrow::<State>()?.0.get(),
        mlua::Value::UserData(ud) if ud.is::<ReadState>() => ud.borrow::<ReadState>()?.0.0.get(),
        // `ui.raw(value, reason)` is its value; only the design lint looks.
        mlua::Value::UserData(ud) if ud.is::<crate::design::RawUd>() => {
            ud.borrow::<crate::design::RawUd>()?.0.clone()
        }
        mlua::Value::UserData(ud) if ud.is::<crate::element::ElementUd>() => {
            Value::Element(ElementHandle(Rc::new(Held::Element(
                ud.borrow::<crate::element::ElementUd>()?.0.clone(),
            ))))
        }
        mlua::Value::UserData(ud) if ud.is::<crate::folder::FolderUd>() => {
            Value::Element(ElementHandle(Rc::new(Held::Folder(
                ud.borrow::<crate::folder::FolderUd>()?.0.clone(),
            ))))
        }
        mlua::Value::UserData(ud) if ud.is::<crate::node::BitmapUd>() => {
            Value::Image(ud.borrow::<crate::node::BitmapUd>()?.0.clone())
        }
        // An error (`ui.try`'s, a promise's) is its message here: a state, a
        // prop or a call's argument holds text, as it did before errors had
        // fields.
        mlua::Value::UserData(ud) if ud.is::<crate::errors::UiError>() => {
            Value::Str(ud.borrow::<crate::errors::UiError>()?.message().into())
        }
        mlua::Value::UserData(ud) if ud.is::<crate::element::RefUd>() => {
            return Err(mlua::Error::runtime(
                "state holds elements, not refs: keep the ref itself, or store ref:get()",
            ));
        }
        other => {
            return Err(mlua::Error::runtime(format!(
                "state holds nil, booleans, numbers, strings, lists, tables and elements, not {}",
                other.type_name()
            )));
        }
    })
}
/// What `raw_table` leaves to the general walk, or the error it found.
enum Unread {
    /// A state handle, an element, `ui.raw`, a metatable, an odd key or
    /// value (whose error the general walk words), or deep nesting.
    General,
    Error(&'static str),
}
/// How deep `raw_table` reads: each level holds three stack slots, and a
/// C function is given 20 (`LUA_MINSTACK`) without asking for more (which
/// could raise an error out of frames holding values).
const RAW_DEPTH: u32 = 5;
/// `table` through Luau's C API, for a tree of plain values; `None` leaves
/// it to the general walk (see `Unread::General`). Through mlua each field
/// costs more than its value: 100,000 rows of seven fields took 95 ms.
/// Strings are shared by the Luau string they came from, so 100,000 rows
/// hold their seven keys once.
fn raw_table(t: &Table, old: Option<&Value>) -> Option<mlua::Result<Value>> {
    use mlua::ffi;
    use std::ffi::c_int;
    struct Cx<'a> {
        held: Option<std::cell::Ref<'a, HashMap<usize, (Value, usize)>>>,
        strings: HashMap<usize, Rc<str>>,
    }
    const TABLE: c_int = 1;
    const IDS: c_int = 2;
    /// # Safety
    ///
    /// (For both.) `state` is inside a C call (`exec_raw`'s) whose stack
    /// holds the value at absolute index `idx` (a table, for `read_table`),
    /// the cache's `ids` (or nil) at 2, and room for three slots a level
    /// below `depth` (`RAW_DEPTH`). Only raw reads and type checks are made:
    /// no metamethod or script runs, nothing allocates (so the collector
    /// does not run, and every string read stays where it is, held by the
    /// table on the stack, while `cx.strings` keys it by address), and no
    /// Luau error is raised. Each push is popped before returning.
    unsafe fn read(
        state: *mut ffi::lua_State,
        idx: c_int,
        old: Option<&Value>,
        depth: u32,
        cx: &mut Cx,
    ) -> Result<Value, Unread> {
        // SAFETY: as this function's; a string is read without conversion
        // (its type checked), its bytes valid while the table holds it.
        unsafe {
            Ok(match ffi::lua_type(state, idx) {
                ffi::LUA_TNIL => Value::Nil,
                ffi::LUA_TBOOLEAN => Value::Bool(ffi::lua_toboolean(state, idx) != 0),
                ffi::LUA_TNUMBER => {
                    Value::Num(ffi::lua_tonumberx(state, idx, std::ptr::null_mut()))
                }
                ffi::LUA_TSTRING => {
                    let mut n = 0;
                    let data = ffi::lua_tolstring(state, idx, &mut n);
                    let bytes = std::slice::from_raw_parts(data.cast::<u8>(), n);
                    match old {
                        Some(Value::Str(o)) if o.as_bytes() == bytes => Value::Str(o.clone()),
                        Some(Value::Text(o)) if same_text(o, bytes) => Value::Text(o.clone()),
                        _ if bytes.len() > zgx::text::LONG => long_string(bytes),
                        _ => Value::Str(string(cx, data as usize, bytes)),
                    }
                }
                ffi::LUA_TLIGHTUSERDATA
                    if std::ptr::eq(ffi::lua_touserdata(state, idx).cast::<u8>(), &NULL) =>
                {
                    Value::Nil
                }
                ffi::LUA_TTABLE => read_table(state, idx, old, depth, cx)?,
                _ => return Err(Unread::General),
            })
        }
    }
    fn string(cx: &mut Cx, at: usize, bytes: &[u8]) -> Rc<str> {
        cx.strings
            .entry(at)
            .or_insert_with(|| String::from_utf8_lossy(bytes).into())
            .clone()
    }
    unsafe fn read_table(
        state: *mut ffi::lua_State,
        idx: c_int,
        old: Option<&Value>,
        depth: u32,
        cx: &mut Cx,
    ) -> Result<Value, Unread> {
        if depth >= RAW_DEPTH {
            return Err(Unread::General);
        }
        // SAFETY: as `read`'s. The walk (`lua_rawiter`) pushes a key and a
        // value, popped after each is read; nothing writes the table
        // meanwhile. Only a string key is read as one (`lua_tolstring` of a
        // number would convert it in place).
        unsafe {
            // A table a script was given (read-only, filed): its value.
            if ffi::lua_getreadonly(state, idx) != 0 && ffi::lua_type(state, IDS) == ffi::LUA_TTABLE
            {
                ffi::lua_pushvalue(state, idx);
                let found = ffi::lua_rawget(state, IDS) == ffi::LUA_TLIGHTUSERDATA;
                let id = ffi::lua_touserdata(state, -1) as usize;
                ffi::lua_pop(state, 1);
                if found && let Some((value, _)) = cx.held.as_ref().and_then(|held| held.get(&id)) {
                    return Ok(match old {
                        Some(old) if old == value => old.clone(),
                        _ => value.clone(),
                    });
                }
            }
            if ffi::lua_getmetatable(state, idx) != 0 {
                ffi::lua_pop(state, 1);
                return Err(Unread::General);
            }
            let len = ffi::lua_objlen(state, idx);
            let was = old.map_or(&[][..], Value::list);
            let was_fields: &[(Rc<str>, Value)] = match old {
                Some(Value::Map(f)) => f,
                _ => &[],
            };
            let mut items: Vec<(usize, Value)> = Vec::with_capacity(len);
            let mut keyed: Vec<(Rc<str>, Value)> = Vec::new();
            let mut iter = 0;
            loop {
                iter = ffi::lua_rawiter(state, idx, iter);
                if iter < 0 {
                    break;
                }
                let top = ffi::lua_gettop(state);
                let read_one = match ffi::lua_type(state, top - 1) {
                    ffi::LUA_TNUMBER => {
                        let n = ffi::lua_tonumberx(state, top - 1, std::ptr::null_mut());
                        if n.fract() == 0. && n >= 1. && n as usize <= len {
                            let i = n as usize;
                            read(state, top, was.get(i - 1), depth + 1, cx)
                                .map(|v| items.push((i, v)))
                        } else {
                            Err(Unread::General)
                        }
                    }
                    ffi::LUA_TSTRING => {
                        let mut n = 0;
                        let data = ffi::lua_tolstring(state, top - 1, &mut n);
                        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), n);
                        let prior = was_fields
                            .binary_search_by(|(k, _)| k.as_bytes().cmp(bytes))
                            .ok()
                            .map(|i| &was_fields[i]);
                        let key = match prior {
                            Some((k, _)) => k.clone(),
                            None => string(cx, data as usize, bytes),
                        };
                        read(state, top, prior.map(|(_, v)| v), depth + 1, cx)
                            .map(|v| keyed.push((key, v)))
                    }
                    _ => Err(Unread::General),
                };
                ffi::lua_pop(state, 2);
                read_one?;
            }
            if !keyed.is_empty() {
                if len > 0 {
                    return Err(Unread::Error(
                        "a state table is a list or has string keys, not both",
                    ));
                }
                keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
                let fresh = Value::Map(keyed.into());
                return Ok(match old {
                    Some(old) if *old == fresh => old.clone(),
                    _ => fresh,
                });
            }
            // The array part comes in order; items run up to the first
            // hole, as a sequence does.
            if !items.is_sorted_by_key(|(i, _)| *i) {
                items.sort_unstable_by_key(|(i, _)| *i);
            }
            let mut list = Vec::with_capacity(items.len());
            for (at, (i, item)) in items.into_iter().enumerate() {
                if i != at + 1 {
                    break;
                }
                list.push(item);
            }
            Ok(match old {
                Some(old @ Value::List(o)) if o[..] == list[..] => old.clone(),
                _ => Value::List(list.into()),
            })
        }
    }
    let lua = t.weak_lua().try_upgrade()?;
    let cache = lua.app_data_ref::<Converted>();
    let ids = cache.as_ref().map(|c| c.ids.clone());
    let mut cx = Cx {
        held: cache.as_ref().map(|c| c.held.borrow()),
        strings: HashMap::new(),
    };
    let mut out = Err(Unread::General);
    // SAFETY: `exec_raw` runs the closure as a C function inside a protected
    // call, the table and `ids` (or nil) its arguments at 1 and 2, with
    // `LUA_MINSTACK` (20) slots: `read_table` needs 3 a level, 5 levels at
    // most. `cx.held` is borrowed throughout; nothing in the walk runs
    // code that could write it.
    unsafe {
        lua.exec_raw::<()>((t, ids), |state| {
            out = read_table(state, TABLE, old, 0, &mut cx);
        })
        .ok()?;
    }
    match out {
        Ok(value) => Some(Ok(value)),
        Err(Unread::General) => None,
        Err(Unread::Error(e)) => Some(Err(mlua::Error::runtime(e))),
    }
}
fn table(t: &Table, old: Option<&Value>, depth: u32) -> mlua::Result<Value> {
    if depth == DEPTH {
        return Err(mlua::Error::runtime(
            "state nests at most 128 tables deep (does a table hold itself?)",
        ));
    }
    if let Some(result) = raw_table(t, old) {
        return result;
    }
    let convert = |value, old| convert_in(value, old, depth + 1);
    let len = t.raw_len();
    let mut keyed: Vec<(String, mlua::Value)> = Vec::new();
    // One walk, unprotected per step: a computed's list of thousands of
    // rows is converted every time it runs.
    let mut items: Vec<(usize, mlua::Value)> = Vec::with_capacity(len);
    t.for_each(|key: mlua::Value, value: mlua::Value| {
        match key {
            mlua::Value::Integer(i) if i >= 1 && i as usize <= len => {
                items.push((i as usize, value))
            }
            mlua::Value::Number(n) if n.fract() == 0. && n >= 1. && n as usize <= len => {
                items.push((n as usize, value))
            }
            mlua::Value::String(k) => keyed.push((k.to_string_lossy(), value)),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "state tables are lists or have string keys, not {} keys",
                    other.type_name()
                )));
            }
        }
        Ok(())
    })?;
    if !keyed.is_empty() {
        if len > 0 {
            return Err(mlua::Error::runtime(
                "a state table is a list or has string keys, not both",
            ));
        }
        keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        // Walk the old fields alongside (both sorted): a key both have
        // keeps its old string and converts against its old value.
        let was: &[(Rc<str>, Value)] = match old {
            Some(Value::Map(f)) => f,
            _ => &[],
        };
        let mut at = 0;
        let mut fields = Vec::with_capacity(keyed.len());
        for (key, value) in keyed {
            while at < was.len() && *was[at].0 < *key {
                at += 1;
            }
            fields.push(match was.get(at) {
                Some((k, v)) if **k == *key => (k.clone(), convert(value, Some(v))?),
                _ => (Rc::from(key), convert(value, None)?),
            });
        }
        let fresh = Value::Map(fields.into());
        return Ok(match old {
            Some(old) if *old == fresh => old.clone(),
            _ => fresh,
        });
    }
    // Empty, but an object (`ui.object`).
    if len == 0 && is_object(t) {
        return Ok(match old {
            Some(old @ Value::Map(f)) if f.is_empty() => old.clone(),
            _ => Value::Map(Rc::from([])),
        });
    }
    let was = old.map_or(&[][..], Value::list);
    // The array part comes in order; items run up to the first hole, as a
    // sequence does.
    if !items.is_sorted_by_key(|(i, _)| *i) {
        items.sort_unstable_by_key(|(i, _)| *i);
    }
    let mut list = Vec::with_capacity(items.len());
    for (at, (i, item)) in items.into_iter().enumerate() {
        if i != at + 1 {
            break;
        }
        list.push(convert(item, was.get(at))?);
    }
    Ok(match old {
        Some(old @ Value::List(o)) if o.len() == list.len() && o[..] == list[..] => old.clone(),
        _ => Value::List(list.into()),
    })
}
impl FromLua for Value {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        convert(value, None)
    }
}
/// As script data: tables are read-only (`frozen`) when they are state's
/// own, or a mutable copy for `state:update`.
pub(crate) fn to_lua(value: &Value, lua: &Lua, frozen: bool) -> mlua::Result<mlua::Value> {
    if matches!(value, Value::List(_) | Value::Map(_)) {
        CONVERTED.set(CONVERTED.get() + 1);
    }
    match value {
        Value::Nil => Ok(mlua::Value::Nil),
        Value::Bool(b) => Ok(mlua::Value::Boolean(*b)),
        Value::Num(n) => Ok(mlua::Value::Number(*n)),
        Value::Str(s) => lua.create_string(&**s).map(mlua::Value::String),
        // Its chunks straight into the string, not joined first.
        // SAFETY: `push_text` leaves one string, `exec_raw`'s result.
        Value::Text(t) => unsafe {
            lua.exec_raw::<mlua::LuaString>((), |state| push_text(state, t))
                .map(mlua::Value::String)
        },
        Value::List(items) => {
            count_items(items.len());
            if items.len() >= 64 && items.iter().all(is_plain) {
                let table = plain_table(lua, items)?;
                table.set_readonly(frozen);
                return Ok(mlua::Value::Table(table));
            }
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                table.raw_set(i + 1, to_lua(item, lua, frozen)?)?;
            }
            table.set_readonly(frozen);
            Ok(mlua::Value::Table(table))
        }
        Value::Map(fields) if !fields.is_empty() && fields.iter().all(|(_, v)| is_plain(v)) => {
            count_items(fields.len());
            let table = plain_map(lua, fields)?;
            table.set_readonly(frozen);
            Ok(mlua::Value::Table(table))
        }
        Value::Map(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (key, value) in fields.iter() {
                table.raw_set(&**key, to_lua(value, lua, frozen)?)?;
                count_items(1);
            }
            // Empty, it stays an object going back (`ui.object`).
            if fields.is_empty() {
                table.set_metatable(Some(object_marker(lua)?))?;
            }
            table.set_readonly(frozen);
            Ok(mlua::Value::Table(table))
        }
        Value::Element(handle) => match &*handle.0 {
            Held::Element(e) => lua.create_userdata(crate::element::ElementUd(e.clone())),
            Held::Folder(folder) => lua.create_userdata(crate::folder::FolderUd(folder.clone())),
        }
        .map(mlua::Value::UserData),
        Value::Image(image) => lua
            .create_userdata(crate::node::BitmapUd(image.clone()))
            .map(mlua::Value::UserData),
    }
}
/// Push `text` as a string, its chunks copied straight in.
///
/// # Safety
/// `state` is a Luau state with room on its stack; a memory error raises a
/// Luau error, which unwinds these frames (holding nothing to drop) to the
/// caller's protected call. The buffer stays where it is made until the
/// string is.
unsafe fn push_text(state: *mut mlua::ffi::lua_State, text: &zgx::text::Text) {
    use mlua::ffi;
    let mut buffer = std::mem::MaybeUninit::<ffi::luaL_Strbuf>::uninit();
    let buffer = buffer.as_mut_ptr();
    // SAFETY: as this function's: a buffer the text's size (its storage
    // held on the stack), filled, then made the string in its place.
    unsafe {
        ffi::luaL_buffinitsize(state, buffer, text.len());
        for chunk in text.chunks() {
            ffi::luaL_addlstring(buffer, chunk.as_ptr().cast(), chunk.len());
        }
        ffi::luaL_pushresult(buffer);
    }
}
thread_local! {
    /// The last long string a script set, while state holds it.
    static RECENT_TEXT: RefCell<zgx::text::WeakText> = RefCell::default();
}
/// A long string from a script, in chunks: an editor bound to it shares it.
/// The same text set again (a file kept as the editor's and as last saved)
/// is the same `Text`, and the edits of one share the other's chunks.
/// A Luau string's text, invalid UTF-8 replaced (as `to_string_lossy`), in
/// one allocation: valid text (nearly every string) is copied once, not
/// scanned for invalid sequences into a `String` and copied again.
fn str_of(bytes: &[u8]) -> Rc<str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Rc::from(text),
        Err(_) => String::from_utf8_lossy(bytes).into(),
    }
}
fn long_string(bytes: &[u8]) -> Value {
    let string = String::from_utf8_lossy(bytes);
    RECENT_TEXT.with_borrow_mut(|recent| {
        if let Some(text) = recent.upgrade()
            && text.len() == string.len()
            && text == *string
        {
            return Value::Text(text);
        }
        let text = zgx::text::Text::new(&string);
        *recent = text.downgrade();
        Value::Text(text)
    })
}
/// Whether a script's string is `text` (kept, then: not copied again).
fn same_text(text: &zgx::text::Text, bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|s| *text == *s)
}
fn is_plain(value: &Value) -> bool {
    matches!(value, Value::Num(_) | Value::Bool(_) | Value::Str(_))
}
/// A list of numbers, booleans and strings as a table, made through Luau's
/// C API (`plain_list`'s inverse): through mlua each item costs more than
/// the table (6 ms for 100,000 numbers, 0.5 here).
fn plain_table(lua: &Lua, items: &[Value]) -> mlua::Result<Table> {
    use mlua::ffi;
    let n = std::ffi::c_int::try_from(items.len())
        .map_err(|_| mlua::Error::runtime("a list too long for a table"))?;
    // SAFETY: makes a table and fills its array part, one value on the stack
    // above it at a time, leaving the table as `exec_raw`'s result. An
    // allocation failing (the memory limit) raises a Luau error, which
    // unwinds this closure (holding nothing to drop: `items` is borrowed)
    // to `exec_raw`'s protected call, an `Err`. The collector may run as
    // strings are made; the table is on the stack, and so kept.
    unsafe {
        lua.exec_raw::<Table>((), |state| {
            ffi::lua_createtable(state, n, 0);
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::Num(n) => ffi::lua_pushnumber(state, *n),
                    Value::Bool(b) => ffi::lua_pushboolean(state, std::ffi::c_int::from(*b)),
                    Value::Str(s) => {
                        ffi::lua_pushlstring(state, s.as_ptr().cast(), s.len());
                    }
                    _ => ffi::lua_pushnil(state),
                }
                ffi::lua_rawseti_(state, -2, i as std::ffi::c_int + 1);
            }
        })
    }
}
/// A table of fields that are numbers, booleans and strings (a row), made
/// through Luau's C API in one protected call: through mlua each key and
/// string value is made in a protected call of its own (a closure pushed
/// and called for each), most of a row's conversion.
fn plain_map(lua: &Lua, fields: &[(Rc<str>, Value)]) -> mlua::Result<Table> {
    use mlua::ffi;
    let n = std::ffi::c_int::try_from(fields.len())
        .map_err(|_| mlua::Error::runtime("a table with too many fields"))?;
    // SAFETY: as `plain_table`'s: a table made and filled, a key and a value
    // above it at a time, left as `exec_raw`'s result; an allocation failing
    // raises a Luau error that unwinds this closure (holding nothing to
    // drop: `fields` is borrowed) to `exec_raw`'s protected call.
    unsafe {
        lua.exec_raw::<Table>((), |state| {
            ffi::lua_createtable(state, 0, n);
            for (key, value) in fields {
                ffi::lua_pushlstring(state, key.as_ptr().cast(), key.len());
                match value {
                    Value::Num(n) => ffi::lua_pushnumber(state, *n),
                    Value::Bool(b) => ffi::lua_pushboolean(state, std::ffi::c_int::from(*b)),
                    Value::Str(s) => {
                        ffi::lua_pushlstring(state, s.as_ptr().cast(), s.len());
                    }
                    _ => ffi::lua_pushnil(state),
                }
                ffi::lua_rawset(state, -3);
            }
        })
    }
}
impl IntoLua for Value {
    fn into_lua(self, lua: &Lua) -> mlua::Result<mlua::Value> {
        shared_lua(&self, lua)
    }
}

thread_local! {
    static CONVERTED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static ITEMS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// How many tables state values have been converted into for scripts on
/// this thread, ever: for budgets (a delta's work must not grow with what
/// did not change).
pub fn tables_converted() -> u64 {
    CONVERTED.get()
}
/// How many items and fields those tables were given, ever: the work a
/// conversion does, which a table converted whole counts once in
/// `tables_converted` however many rows it holds.
pub fn values_converted() -> u64 {
    ITEMS.get()
}
fn count_items(n: usize) {
    ITEMS.set(ITEMS.get() + n as u64);
}

/// Values this large (items and fields, counted all the way down) are
/// converted once and the read-only table reused while the value stays the
/// same, and so is every large part of them: a list read by every computed
/// that needs it costs one conversion, not one per read, and a new list
/// that keeps most of its rows (a transcript as a turn streams) converts
/// only the rows that are new.
const CACHED_FROM: usize = 16;
fn large(value: &Value) -> bool {
    // Counts down from CACHED_FROM, stopping as soon as it gets there.
    fn count(value: &Value, left: &mut usize) -> bool {
        let children: &mut dyn Iterator<Item = &Value> = match value {
            Value::List(items) => &mut items.iter(),
            Value::Map(fields) => &mut fields.iter().map(|(_, v)| v),
            _ => return false,
        };
        for child in children {
            *left -= 1;
            if *left == 0 || count(child, left) {
                return true;
            }
        }
        false
    }
    matches!(value, Value::List(_) | Value::Map(_)) && count(value, &mut CACHED_FROM.clone())
}
/// Give a new VM its cache of converted values.
pub(crate) fn install_cache(lua: &Lua) {
    let weak = |mode: &str| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.raw_set("__mode", mode)?;
        t.set_metatable(Some(meta))?;
        Ok(t)
    };
    let tables = lua.create_table();
    let clone = lua
        .globals()
        .get::<Table>("table")
        .and_then(|t| t.get::<mlua::Function>("clone"));
    if let (Ok(tables), Ok(ids), Ok(clone)) = (tables, weak("k"), clone) {
        lua.set_app_data(Converted {
            held: Default::default(),
            tables,
            ids,
            clone,
            bases: Default::default(),
            weight: std::cell::Cell::new(0),
            swept: std::cell::Cell::new(0),
            recent: Default::default(),
            next_recent: std::cell::Cell::new(0),
        });
    }
}
/// A VM's converted values, by the identity of their contents. `held` keeps
/// each value alive while it is cached, so an address is never reused for
/// other contents while a table is filed under it; it lets go of a value
/// nothing else holds, and of its table with it. The tables are not held
/// weakly: one the script had let go of would then be converted again or
/// not as the collector happened to have run, and what a delta costs would
/// vary from run to run.
struct Converted {
    /// id → the value and its `weight`.
    held: std::cell::RefCell<HashMap<usize, (Value, usize)>>,
    /// id → table, while `held` holds its value. (Handles, not registry
    /// keys: looked up for every row converted.)
    tables: Table,
    /// table → id, weak keys: a read-only table given back (a computed
    /// returning a row it read) is its value again, without a walk.
    ids: Table,
    /// Luau's `table.clone`: a table copied in C (see `derive`).
    clone: mlua::Function,
    /// id → the value a list was written from (`derives_from`), whose
    /// table its own is derived from when a script reads it.
    bases: std::cell::RefCell<HashMap<usize, Value>>,
    /// The `weight` of what is held.
    weight: std::cell::Cell<usize>,
    /// The `weight` of what was held after the last sweep.
    swept: std::cell::Cell<usize>,
    /// The last few small tables converted (`recent_lua`): id, the value it
    /// was made from (held, so the id stays its own), the table.
    recent: std::cell::RefCell<Vec<(usize, Value, Table)>>,
    /// Where the next one goes once `recent` is full.
    next_recent: std::cell::Cell<usize>,
}
/// Sweeps wait until the cache holds this much (`weight`) at least.
const SWEPT_FROM: usize = 1 << 12;
/// The values a cached value's table holds, its small parts counted all the
/// way down (each large part is cached, and weighed, on its own): what its
/// table costs the VM.
fn weight(value: &Value) -> usize {
    fn children(value: &Value) -> Box<dyn Iterator<Item = &Value> + '_> {
        match value {
            Value::List(items) => Box::new(items.iter()),
            Value::Map(fields) => Box::new(fields.iter().map(|(_, v)| v)),
            _ => Box::new(std::iter::empty()),
        }
    }
    1 + children(value).map(part_weight).sum::<usize>()
}
/// What a part adds to its container's `weight`: 1 for a large one (cached,
/// and weighed, on its own), all of a small one.
fn part_weight(value: &Value) -> usize {
    fn all(value: &Value) -> usize {
        1 + match value {
            Value::List(items) => items.iter().map(all).sum::<usize>(),
            Value::Map(fields) => fields.iter().map(|(_, v)| all(v)).sum::<usize>(),
            _ => 0,
        }
    }
    if large(value) { 1 } else { all(value) }
}
pub(crate) fn identity(value: &Value) -> usize {
    match value {
        Value::List(items) => Rc::as_ptr(items) as *const u8 as usize,
        Value::Map(fields) => Rc::as_ptr(fields) as *const u8 as usize,
        _ => 0,
    }
}
fn id_key(id: usize) -> mlua::Value {
    mlua::Value::LightUserData(mlua::LightUserData(id as *mut std::ffi::c_void))
}
/// As script data, read-only, its large parts shared with every other
/// conversion of them.
pub(crate) fn shared_lua(value: &Value, lua: &Lua) -> mlua::Result<mlua::Value> {
    if !large(value) {
        return recent_lua(value, lua);
    }
    shared_part(value, lua)
}
/// Small tables kept from their last conversions (`Converted::recent`).
const RECENT: usize = 32;
/// `to_lua` for a small value, a table converted lately given again: a row's
/// item read by its key and then by the row (`row:get()`), or by every
/// function of the row, is converted once until it is written.
fn recent_lua(value: &Value, lua: &Lua) -> mlua::Result<mlua::Value> {
    if !matches!(value, Value::List(_) | Value::Map(_)) {
        return to_lua(value, lua, true);
    }
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return to_lua(value, lua, true);
    };
    let id = identity(value);
    if let Some((_, _, table)) = cache.recent.borrow().iter().find(|(at, ..)| *at == id) {
        return Ok(mlua::Value::Table(table.clone()));
    }
    drop(cache);
    let converted = to_lua(value, lua, true)?;
    if let (mlua::Value::Table(table), Some(cache)) = (&converted, lua.app_data_ref::<Converted>())
    {
        let mut recent = cache.recent.borrow_mut();
        // Held with its value, so its address is not another's meanwhile.
        let entry = (id, value.clone(), table.clone());
        if recent.len() < RECENT {
            recent.push(entry);
        } else {
            let at = cache.next_recent.get();
            recent[at] = entry;
            cache.next_recent.set((at + 1) % RECENT);
        }
    }
    Ok(converted)
}
/// `shared_lua` for a large value or any table in one: each row of a long
/// list keeps its table, so a computed returning some of the rows (a
/// filter) gives back tables that are their values again, without a walk.
fn shared_part(value: &Value, lua: &Lua) -> mlua::Result<mlua::Value> {
    if !matches!(value, Value::List(_) | Value::Map(_)) {
        return to_lua(value, lua, true);
    }
    // (The VM registers its cache when it starts: `install_cache`.)
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return to_lua(value, lua, true);
    };
    let id = identity(value);
    let tables = cache.tables.clone();
    if let mlua::Value::Table(t) = tables.raw_get::<mlua::Value>(id_key(id))? {
        return Ok(mlua::Value::Table(t));
    }
    // Written since a script last read it: its table from the one read.
    let base = cache.bases.borrow_mut().remove(&id);
    drop(cache);
    if let Some(base) = base {
        derive(lua, &base, value)?;
        if let Some(t) = cached(lua, value)? {
            return Ok(mlua::Value::Table(t));
        }
    }
    if !has_element(value) {
        return raw_shared(lua, value).map(mlua::Value::Table);
    }
    CONVERTED.set(CONVERTED.get() + 1);
    let table = match value {
        Value::List(items) => {
            count_items(items.len());
            if items.len() >= 64 && items.iter().all(is_plain) {
                plain_table(lua, items)?
            } else {
                let table = lua.create_table_with_capacity(items.len(), 0)?;
                for (i, item) in items.iter().enumerate() {
                    table.raw_set(i + 1, shared_part(item, lua)?)?;
                }
                table
            }
        }
        Value::Map(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (key, value) in fields.iter() {
                table.raw_set(&**key, shared_part(value, lua)?)?;
                count_items(1);
            }
            table
        }
        _ => unreachable!("shared parts are lists and tables"),
    };
    table.set_readonly(true);
    file(lua, value, &table)?;
    Ok(mlua::Value::Table(table))
}
/// Whether `value` holds an element or an image: userdata, which only
/// mlua makes (`raw_shared` pushes neither).
fn has_element(value: &Value) -> bool {
    match value {
        Value::Element(_) | Value::Image(_) => true,
        Value::List(items) => items.iter().any(has_element),
        Value::Map(fields) => fields.iter().any(|(_, v)| has_element(v)),
        _ => false,
    }
}
/// `shared_part` for a value holding no element, made through Luau's C
/// API: the same tables, each read-only and filed, a cached part reused.
/// Through mlua, every field costs more than its table: 100,000 rows of
/// seven fields and a short history took 380 ms to read the first time.
fn raw_shared(lua: &Lua, value: &Value) -> mlua::Result<Table> {
    use mlua::ffi;
    use std::ffi::{c_int, c_void};
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return Err(mlua::Error::runtime("no conversion cache"));
    };
    let (tables, ids) = (cache.tables.clone(), cache.ids.clone());
    drop(cache);
    struct Made<'a> {
        filed: Vec<&'a Value>,
        items: usize,
    }
    /// Push `value`'s script value (`tables` and `ids`, the cache's, at
    /// stack indices 1 and 2).
    ///
    /// # Safety
    ///
    /// `state` is inside a C call (`exec_raw`'s) with the two cache tables
    /// at 1 and 2. Each level asks for the 4 slots it uses
    /// (`luaL_checkstack`). An allocation failing (the memory limit) raises
    /// a Luau error, which unwinds these frames (holding nothing to drop:
    /// `made` is the caller's) to the protected call. The collector may run
    /// as tables and strings are made: each is on the stack until it is
    /// filed or set into its parent. Raw sets only, so no script runs.
    unsafe fn push<'a>(state: *mut ffi::lua_State, value: &'a Value, made: &mut Made<'a>) {
        const TABLES: c_int = 1;
        const IDS: c_int = 2;
        // SAFETY: as this function's.
        unsafe {
            match value {
                Value::Nil | Value::Element(_) | Value::Image(_) => ffi::lua_pushnil(state),
                Value::Bool(b) => ffi::lua_pushboolean(state, c_int::from(*b)),
                Value::Num(n) => ffi::lua_pushnumber(state, *n),
                Value::Str(s) => {
                    ffi::lua_pushlstring(state, s.as_ptr().cast(), s.len());
                }
                Value::Text(t) => push_text(state, t),
                Value::List(_) | Value::Map(_) => {
                    let id = identity(value) as *mut c_void;
                    ffi::lua_pushlightuserdata(state, id);
                    if ffi::lua_rawget(state, TABLES) == ffi::LUA_TTABLE {
                        return;
                    }
                    ffi::lua_pop(state, 1);
                    // A table, a key and a value a level, all the way down.
                    ffi::luaL_checkstack(state, 4, c"a value nested too deep".as_ptr());
                    match value {
                        Value::List(items) => {
                            ffi::lua_createtable(state, items.len() as c_int, 0);
                            for (i, item) in items.iter().enumerate() {
                                push(state, item, made);
                                ffi::lua_rawseti_(state, -2, i as c_int + 1);
                            }
                            made.items += items.len();
                        }
                        Value::Map(fields) => {
                            ffi::lua_createtable(state, 0, fields.len() as c_int);
                            for (key, item) in fields.iter() {
                                ffi::lua_pushlstring(state, key.as_ptr().cast(), key.len());
                                push(state, item, made);
                                ffi::lua_rawset(state, -3);
                            }
                            made.items += fields.len();
                        }
                        _ => {}
                    }
                    ffi::lua_setreadonly(state, -1, 1);
                    // Held before it is filed: `tables[id]` must never
                    // outlive the value whose address `id` is, or a new
                    // value at that address would be given this table. A
                    // filing that fails part way (an allocation over the
                    // limit) leaves it held, which a sweep lets go of.
                    made.filed.push(value);
                    // Filed: tables[id] = it, ids[it] = id.
                    ffi::lua_pushlightuserdata(state, id);
                    ffi::lua_pushvalue(state, -2);
                    ffi::lua_rawset(state, TABLES);
                    ffi::lua_pushvalue(state, -1);
                    ffi::lua_pushlightuserdata(state, id);
                    ffi::lua_rawset(state, IDS);
                }
            }
        }
    }
    let mut made = Made {
        filed: Vec::new(),
        items: 0,
    };
    // SAFETY: `exec_raw` pushes the two cache tables at 1 and 2, as `push`
    // needs, and takes the table it leaves as its result.
    let table = unsafe {
        lua.exec_raw::<Table>((&tables, &ids), |state| {
            push(state, value, &mut made);
            // Only the result stays.
            ffi::lua_remove(state, 1);
            ffi::lua_remove(state, 1);
        })
    };
    // What was filed is held, even if the conversion failed part way: its
    // tables are in the cache.
    if let Some(cache) = lua.app_data_ref::<Converted>() {
        let mut held = cache.held.borrow_mut();
        for part in &made.filed {
            let added = weight(part);
            held.insert(identity(part), ((*part).clone(), added));
            cache.weight.set(cache.weight.get() + added);
        }
    }
    CONVERTED.set(CONVERTED.get() + made.filed.len() as u64);
    count_items(made.items);
    let table = table?;
    sweep(lua)?;
    Ok(table)
}
/// Cache `table` (read-only) as `value`'s, for `shared_part` to find, and
/// sweep what nothing else holds as the cache grows.
fn file(lua: &Lua, value: &Value, table: &Table) -> mlua::Result<()> {
    file_weighed(lua, value, table, weight(value))
}
/// `file`, with `value`'s `weight` known.
fn file_weighed(lua: &Lua, value: &Value, table: &Table, added: usize) -> mlua::Result<()> {
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return Ok(());
    };
    let id = identity(value);
    // Held before it is filed, as in `raw_shared`: a filing that fails
    // part way (an allocation over the limit) must not leave `tables[id]`
    // without the value that keeps `id`'s address from being reused.
    cache.held.borrow_mut().insert(id, (value.clone(), added));
    cache.weight.set(cache.weight.get() + added);
    let (tables, ids) = (cache.tables.clone(), cache.ids.clone());
    drop(cache);
    tables.raw_set(id_key(id), table)?;
    ids.raw_set(table, id_key(id))?;
    sweep(lua)
}
/// Sweep as the cache doubles in weight: amortized, each conversion pays a
/// little, and what nothing else holds costs at most as much again as what
/// is held.
fn sweep(lua: &Lua) -> mlua::Result<()> {
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return Ok(());
    };
    let (tables, ids) = (cache.tables.clone(), cache.ids.clone());
    if cache.weight.get() >= (cache.swept.get() * 2).max(SWEPT_FROM) {
        let mut gone: Vec<usize> = Vec::new();
        {
            let mut held = cache.held.borrow_mut();
            // Until none is left: letting go of a value lets go of its
            // parts.
            loop {
                let before = gone.len();
                gone.extend(
                    held.iter()
                        .filter(|(_, (v, _))| only_cached(v))
                        .map(|(id, _)| *id),
                );
                if gone.len() == before {
                    break;
                }
                for id in &gone[before..] {
                    if let Some((_, w)) = held.remove(id) {
                        cache.weight.set(cache.weight.get() - w);
                    }
                }
            }
            cache.swept.set(cache.weight.get());
        }
        for id in gone {
            // Its address may be reused now: nothing may find it by it.
            if let mlua::Value::Table(t) = tables.raw_get::<mlua::Value>(id_key(id))? {
                ids.raw_set(t, mlua::Value::Nil)?;
            }
            tables.raw_set(id_key(id), mlua::Value::Nil)?;
        }
    }
    Ok(())
}
/// The cached table of `value`, if it has one.
fn cached(lua: &Lua, value: &Value) -> mlua::Result<Option<Table>> {
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return Ok(None);
    };
    let tables = cache.tables.clone();
    Ok(
        match tables.raw_get::<mlua::Value>(id_key(identity(value)))? {
            mlua::Value::Table(t) => Some(t),
            _ => None,
        },
    )
}
/// `new`, written from `old` (`set_key` on a row, `push`), gets its table
/// from `old`'s when a script reads it (`derive`), or from what `old` was
/// written from if no script read `old` (`set_key`s in a batch). Lazily: a
/// list no script read since its last write is the state's alone, which
/// lets `set_key` write it in place.
fn derives_from(lua: &Lua, old: &Value, new: &Value) {
    let (Value::List(before), Value::List(after)) = (old, new) else {
        return;
    };
    if Rc::ptr_eq(before, after) || !large(new) {
        return;
    }
    let Some(cache) = lua.app_data_ref::<Converted>() else {
        return;
    };
    let mut bases = cache.bases.borrow_mut();
    let base = match bases.remove(&identity(old)) {
        Some(base) => base,
        None => old.clone(),
    };
    // A few states written at a time; a value dropped unread leaves its
    // entry, harmless (any base derives right, if not as cheaply) until
    // they are cleared here.
    if bases.len() >= 64 {
        bases.clear();
    }
    bases.insert(identity(new), base);
}
/// After a write that kept most of a long list (`set_key` on a row, `push`),
/// its table for scripts: the old one's, copied in C, with only the items
/// that changed converted. Converting every row again made one write to a
/// list of thousands cost each script reading it as much as building it.
pub(crate) fn derive(lua: &Lua, old: &Value, new: &Value) -> mlua::Result<()> {
    let (Value::List(before), Value::List(after)) = (old, new) else {
        return Ok(());
    };
    if Rc::ptr_eq(before, after) || after.len() < before.len() || !large(new) {
        return Ok(());
    }
    let changed = before
        .iter()
        .zip(after.iter())
        .filter(|(a, b)| !same_item(a, b))
        .count()
        + (after.len() - before.len());
    // Mostly new: converting it whole is as cheap.
    if changed * 4 > after.len() || cached(lua, new)?.is_some() {
        return Ok(());
    }
    let Some(base) = cached(lua, old)? else {
        return Ok(());
    };
    let clone = match lua.app_data_ref::<Converted>() {
        Some(cache) => cache.clone.clone(),
        None => return Ok(()),
    };
    let table: Table = clone.call(base)?;
    table.set_readonly(false);
    for (i, (a, b)) in before.iter().zip(after.iter()).enumerate() {
        if !same_item(a, b) {
            table.raw_set(i + 1, shared_part(b, lua)?)?;
            count_items(1);
        }
    }
    for (i, b) in after.iter().enumerate().skip(before.len()) {
        table.raw_set(i + 1, shared_part(b, lua)?)?;
        count_items(1);
    }
    table.set_readonly(true);
    CONVERTED.set(CONVERTED.get() + 1);
    // Its weight from the old one's: only what changed is weighed.
    let old_weight = lua
        .app_data_ref::<Converted>()
        .and_then(|cache| cache.held.borrow().get(&identity(old)).map(|(_, w)| *w));
    let added = match old_weight {
        Some(w) => {
            let changed = before
                .iter()
                .zip(after.iter())
                .filter(|(a, b)| !same_item(a, b));
            let (gone, came) = changed.fold((0, 0), |(g, c), (a, b)| {
                (g + part_weight(a), c + part_weight(b))
            });
            let appended: usize = after[before.len()..].iter().map(part_weight).sum();
            w - gone + came + appended
        }
        None => weight(new),
    };
    file_weighed(lua, new, &table, added)
}
/// Whether two list items convert to the same script value: one table, or
/// equal plain values.
fn same_item(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
        (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
        (Value::List(_) | Value::Map(_), _) | (_, Value::List(_) | Value::Map(_)) => false,
        (a, b) => a == b,
    }
}
/// A computed's result, `table` as the script returned it, kept as its
/// value's table (a copy, read-only, the script's own left as it is): every
/// script reading the computed gets it back without converting it again.
/// Only for one whose tables are read-only already (rows of a state), whose
/// copy no later change of the script's can reach.
pub(crate) fn remember(lua: &Lua, table: &Table, value: &Value) -> mlua::Result<()> {
    if table.is_readonly() || !large(value) || cached(lua, value)?.is_some() {
        return Ok(());
    }
    let mut frozen = true;
    table.for_each(|_: mlua::Value, v: mlua::Value| {
        if let mlua::Value::Table(t) = v
            && !t.is_readonly()
        {
            frozen = false;
        }
        Ok(())
    })?;
    if !frozen {
        return Ok(());
    }
    let clone = match lua.app_data_ref::<Converted>() {
        Some(cache) => cache.clone.clone(),
        None => return Ok(()),
    };
    let copy: Table = clone.call(table)?;
    copy.set_readonly(true);
    file(lua, value, &copy)
}
/// How many values a VM's conversion cache holds.
pub(crate) fn converted_len(lua: &Lua) -> usize {
    lua.app_data_ref::<Converted>()
        .map_or(0, |cache| cache.held.borrow().len())
}
/// The value a read-only table converted from a cached value came from.
fn converted_from(t: &Table, lua: &Lua) -> Option<Value> {
    if !t.is_readonly() {
        return None;
    }
    let cache = lua.app_data_ref::<Converted>()?;
    let ids = &cache.ids;
    let mlua::Value::LightUserData(id) = ids.raw_get::<mlua::Value>(t).ok()? else {
        return None;
    };
    cache
        .held
        .borrow()
        .get(&(id.0 as usize))
        .map(|(v, _)| v.clone())
}
/// Whether the cache holds the only reference to `value`'s contents.
fn only_cached(value: &Value) -> bool {
    match value {
        Value::List(items) => Rc::strong_count(items) == 1,
        Value::Map(fields) => Rc::strong_count(fields) == 1,
        _ => false,
    }
}

/// A path for `set_key`: a key, an index, or a list of them.
/// `path_of`, a table step (`{ id = 7 }`) picking the first item of the list
/// there whose fields are those, in `current` (the state's value).
fn path_in(lua: &Lua, value: mlua::Value, current: &Value) -> mlua::Result<Vec<PathKey>> {
    let mlua::Value::Table(t) = &value else {
        return path_of(lua, value);
    };
    if raw_path(lua, t).is_some() {
        return path_of(lua, value);
    }
    let mut path = Vec::new();
    let mut at = Some(current);
    for step in t.sequence_values::<mlua::Value>() {
        let key = match step? {
            mlua::Value::Table(fields) => {
                let mut want: Vec<(Rc<str>, Value)> = Vec::new();
                for pair in fields.pairs::<String, mlua::Value>() {
                    let (name, v) = pair?;
                    want.push((name.as_str().into(), convert(v, None)?));
                }
                let found = at.and_then(|list| {
                    list.list()
                        .iter()
                        .position(|item| want.iter().all(|(name, v)| item.field(name) == Some(v)))
                });
                match found {
                    Some(i) => PathKey::Index(i + 1),
                    None => {
                        let said: Vec<String> =
                            want.iter().map(|(k, v)| format!("{k} = {v}")).collect();
                        return Err(mlua::Error::runtime(format!(
                            "no item has {{ {} }} along the state path",
                            said.join(", ")
                        )));
                    }
                }
            }
            other => path_of(lua, other)?.remove(0),
        };
        at = at.and_then(|v| v.at(std::slice::from_ref(&key)));
        path.push(key);
    }
    if path.is_empty() {
        return Err(mlua::Error::runtime(
            "state:set_key needs a key; state:set replaces the whole value",
        ));
    }
    Ok(path)
}
fn path_of(lua: &Lua, value: mlua::Value) -> mlua::Result<Vec<PathKey>> {
    let step = |value: mlua::Value| -> mlua::Result<PathKey> {
        Ok(match value {
            mlua::Value::String(s) => PathKey::Key(s.to_str()?.as_ref().into()),
            mlua::Value::Integer(i) if i >= 1 => PathKey::Index(i as usize),
            mlua::Value::Number(n) if n.fract() == 0. && n >= 1. => PathKey::Index(n as usize),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "a state path is made of string keys and indices from 1, not {}",
                    match &other {
                        mlua::Value::Integer(i) => i.to_string(),
                        mlua::Value::Number(n) => n.to_string(),
                        other => other.type_name().to_owned(),
                    }
                )));
            }
        })
    };
    match value {
        mlua::Value::Table(t) => {
            let path = match raw_path(lua, &t) {
                Some(path) => path,
                // A step neither a string nor an index from 1: said as mlua
                // reads it.
                None => t
                    .sequence_values::<mlua::Value>()
                    .map(|v| step(v?))
                    .collect::<mlua::Result<Vec<_>>>()?,
            };
            if path.is_empty() {
                return Err(mlua::Error::runtime(
                    "state:set_key needs a key; state:set replaces the whole value",
                ));
            }
            Ok(path)
        }
        other => Ok(vec![step(other)?]),
    }
}

/// A path table's steps read in one C call (mlua reads each in a protected
/// call of its own, most of a short write's cost), or `None` if one is not
/// a string key or an index from 1 (or not UTF-8).
fn raw_path(lua: &Lua, t: &Table) -> Option<Vec<PathKey>> {
    use mlua::ffi;
    let mut path: Option<Vec<PathKey>> = Some(Vec::new());
    // SAFETY: the table is at 1 inside `exec_raw`'s C call; only raw reads
    // and type checks are made, each read popped before the next, and a
    // string's bytes are copied out while the table holds it. Nothing
    // allocates in Luau or raises a Luau error.
    let read = unsafe {
        lua.exec_raw::<()>(t, |state| {
            let n = ffi::lua_objlen(state, 1);
            let steps = path.as_mut().expect("made above");
            steps.reserve(n);
            for i in 1..=n {
                let step = match ffi::lua_rawgeti_(state, 1, i as std::ffi::c_int) {
                    ffi::LUA_TNUMBER => {
                        let n = ffi::lua_tonumberx(state, -1, std::ptr::null_mut());
                        (n.fract() == 0. && n >= 1.).then_some(PathKey::Index(n as usize))
                    }
                    ffi::LUA_TSTRING => {
                        let mut len = 0;
                        let data = ffi::lua_tolstring(state, -1, &mut len);
                        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
                        std::str::from_utf8(bytes)
                            .ok()
                            .map(|key| PathKey::Key(key.into()))
                    }
                    _ => None,
                };
                ffi::lua_pop(state, 1);
                match step {
                    Some(step) => steps.push(step),
                    None => {
                        path = None;
                        return;
                    }
                }
            }
        })
    };
    read.ok().and(path)
}

/// Shared state (`ui.shared`) as Rust holds it: a signal whose `set`
/// reconciles (`Value::reconcile`), so a model republished whole keeps the
/// parts that did not change, and what reads them does no work for them.
/// It derefs to the signal for reading (`get`, `with`) and observing.
#[derive(Clone)]
pub struct Shared(pub(crate) Signal<Value>);
impl Shared {
    /// Sets the value, keeping the current one's parts where they are
    /// equal; whether anything changed.
    pub fn set(&self, value: Value) -> bool {
        let next = self.0.with_untracked(|old| value.reconcile(old));
        self.0.set(next)
    }
    /// The signal itself (its `set` replaces without reconciling).
    pub fn signal(&self) -> Signal<Value> {
        self.0.clone()
    }
}
impl std::ops::Deref for Shared {
    type Target = Signal<Value>;
    fn deref(&self) -> &Signal<Value> {
        &self.0
    }
}
impl From<Shared> for Signal<Value> {
    fn from(shared: Shared) -> Self {
        shared.0
    }
}

/// A handle to Rust-owned state. Scripts read it with `:get()`, which
/// subscribes the reading text or style, and write it from event handlers.
#[derive(Clone)]
pub struct State(pub Signal<Value>, pub(crate) Option<Fresh>);
/// What brings a derived state up to date before it is read: the effects
/// that make it, run if a write queued them, the one it depends on first
/// (a sorted view's spec, then its order).
#[derive(Clone)]
pub(crate) enum Fresh {
    One(zgx::reactive::Refresh),
    Two(zgx::reactive::Refresh, zgx::reactive::Refresh),
}
impl State {
    /// A state over `signal`.
    pub fn new(signal: Signal<Value>) -> Self {
        Self(signal, None)
    }
    /// A computed's value is current when read, even in a handler or a
    /// batch, where its effect would otherwise run only afterwards.
    fn fresh(&self) -> &Signal<Value> {
        match &self.1 {
            None => {}
            Some(Fresh::One(refresh)) => {
                refresh.refresh();
            }
            Some(Fresh::Two(first, then)) => {
                first.refresh();
                then.refresh();
            }
        }
        &self.0
    }
    /// Sets the value as a script's write does (`ui.async`'s optimistic
    /// writes).
    pub(crate) fn write_from(&self, lua: &mlua::Lua, value: Value) {
        self.write(lua, value);
    }
    /// Sets the value; what it updates runs off the script's clock.
    fn write(&self, lua: &mlua::Lua, value: Value) {
        let vm = lua
            .app_data_ref::<crate::node::Env>()
            .and_then(|env| env.vm.upgrade());
        match vm {
            Some(vm) => vm.write_off_clock(|| self.0.set(value)),
            None => self.0.set(value),
        };
    }
    /// A plugin's state to hold what `bytes` makes of what it is counted
    /// as holding (`meter`): an error if the plugin has no room for it.
    /// `leaf` written at `path`, where `at` was: in place while nothing
    /// else holds the list (no script has read it since the last write), so
    /// a batch of row writes to a list of 100,000 copies it once, not once
    /// a write.
    fn write_at(
        &self,
        lua: &Lua,
        path: &[PathKey],
        at: Option<&Value>,
        leaf: Value,
        method: &str,
    ) -> mlua::Result<()> {
        let s = self;
        // What it holds less the leaf it replaces, and the new one.
        let was = at.map_or(0, Value::weight);
        s.charge(lua, |held| held.saturating_sub(was) + leaf.weight())?;
        let mut done = Ok(false);
        // The row written, for a sorted view reading the list.
        let (before, row) = (
            s.0.version(),
            match path.first() {
                Some(PathKey::Index(i)) => i.checked_sub(1),
                _ => None,
            },
        );
        let mut write = || {
            s.0.mutate(|v| match v.set_path_in_place(path, &leaf) {
                Ok(Some(changed)) => {
                    done = Ok(true);
                    changed
                }
                Ok(None) => false,
                Err(e) => {
                    done = Err(e);
                    false
                }
            })
        };
        match lua
            .app_data_ref::<crate::node::Env>()
            .and_then(|env| env.vm.upgrade())
        {
            Some(vm) => vm.write_off_clock(&mut write),
            None => write(),
        };
        if done.map_err(|e| fail(method, e))? {
            crate::sorted::note_write(&s.0, before, row);
            return Ok(());
        }
        // (Charged above.)
        let written = s.change_by(
            lua,
            |old| old.set_path(path, leaf).map_err(|e| fail(method, e)),
            |_, held| held,
        );
        if s.0.version() != before {
            crate::sorted::note_write(&s.0, before, row);
        }
        written
    }
    fn charge(&self, lua: &Lua, bytes: impl FnOnce(usize) -> usize) -> mlua::Result<()> {
        let meter = lua
            .app_data_ref::<crate::node::Env>()
            .and_then(|env| env.meter.clone());
        match meter {
            Some(meter) => meter.hold(&self.0, bytes(meter.held_by(&self.0))),
            None => Ok(()),
        }
    }
    /// Writes what `f` makes of the current value.
    fn change(&self, lua: &Lua, f: impl FnOnce(&Value) -> mlua::Result<Value>) -> mlua::Result<()> {
        self.change_by(lua, f, |next, _| next.weight())
    }
    /// `change`, a plugin's state then holding what `bytes` makes of the
    /// next value and what it held.
    fn change_by(
        &self,
        lua: &Lua,
        f: impl FnOnce(&Value) -> mlua::Result<Value>,
        bytes: impl FnOnce(&Value, usize) -> usize,
    ) -> mlua::Result<()> {
        let old = self.0.with_untracked(Clone::clone);
        let next = f(&old)?;
        self.charge(lua, |held| bytes(&next, held))?;
        // When a script reads it: its table from the old one's.
        derives_from(lua, &old, &next);
        self.write(lua, next);
        Ok(())
    }
}
fn fail(method: &str, error: String) -> mlua::Error {
    mlua::Error::runtime(format!("state:{method}: {error}"))
}
impl UserData for State {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        add_reads(methods, |s: &State| s);
        methods.add_method("set", |lua, s, value: mlua::Value| {
            s.change(lua, |old| convert(value, Some(old)))
        });
        // `fn(t)` gets a mutable copy: change it in place and return nothing,
        // or return the new value.
        methods.add_method("update", |lua, s, f: mlua::Function| {
            let old = s.0.with_untracked(Clone::clone);
            let copy = to_lua(&old, lua, false)?;
            let returned: mlua::Value = f.call(copy.clone())?;
            let next = if returned.is_nil() { copy } else { returned };
            let next = convert(next, Some(&old))?;
            s.charge(lua, |_| next.weight())?;
            s.write(lua, next);
            Ok(())
        });
        // `set_key("k", v)`, `set_key(3, v)`, `set_key({ "tabs", 3, "title" }, v)`;
        // a table step picks a list's item by its fields: `{ { id = 7 }, "title" }`.
        methods.add_method(
            "set_key",
            |lua, s, (path, value): (mlua::Value, mlua::Value)| {
                let path = s.0.with_untracked(|old| path_in(lua, path, old))?;
                let at = s.0.with_untracked(|old| old.at(&path).cloned());
                let leaf = convert(value, at.as_ref())?;
                s.write_at(lua, &path, at.as_ref(), leaf, "set_key")
            },
        );
        // `append(path, text)`: text added to the string at `path` (none
        // there: the text), concatenated in Rust: a reply's token, without
        // its text so far copied into the script and back.
        methods.add_method(
            "append",
            |lua, s, (path, text): (mlua::Value, mlua::LuaString)| {
                let path = s.0.with_untracked(|old| path_in(lua, path, old))?;
                let at = s.0.with_untracked(|old| old.at(&path).cloned());
                let text = text.to_str()?;
                let joined: Rc<str> = match &at {
                    Some(Value::Str(was)) => format!("{was}{}", &*text).into(),
                    None | Some(Value::Nil) => (&*text).into(),
                    Some(_) => {
                        return Err(fail(
                            "append",
                            "adds text to a string, and that is not one".into(),
                        ));
                    }
                };
                s.write_at(lua, &path, at.as_ref(), Value::Str(joined), "append")
            },
        );
        // A shallow merge: `patch({ a = 1, b = 2 })`.
        methods.add_method("patch", |lua, s, patch: Table| {
            // What the fields it replaces held, and what the new ones do.
            let (replaced, added) = (Cell::new(0), Cell::new(0));
            s.change_by(
                lua,
                |old| {
                    let mut next = old.clone();
                    for pair in patch.pairs::<mlua::Value, mlua::Value>() {
                        let (key, value) = pair?;
                        let mlua::Value::String(key) = key else {
                            return Err(fail(
                                "patch",
                                format!("takes string keys, not {} keys", key.type_name()),
                            ));
                        };
                        let path = [PathKey::Key(key.to_str()?.as_ref().into())];
                        let was = next.at(&path);
                        replaced.set(replaced.get() + was.map_or(0, |v| 16 + SLOT + v.weight()));
                        let leaf = convert(value, was)?;
                        added.set(added.get() + 16 + SLOT + leaf.weight());
                        next = next.set_path(&path, leaf).map_err(|e| fail("patch", e))?;
                    }
                    Ok(next)
                },
                |_, held| held.saturating_sub(replaced.get()) + added.get(),
            )
        });
        methods.add_method("toggle", |lua, s, ()| {
            s.charge(lua, |_| 0)?;
            s.write(lua, Value::Bool(!s.0.with_untracked(Value::truthy)));
            Ok(())
        });
        methods.add_method("add", |lua, s, by: Option<f64>| {
            let n = s.0.with_untracked(|v| v.as_f64().unwrap_or(0.));
            s.charge(lua, |_| 0)?;
            s.write(lua, Value::Num(n + by.unwrap_or(1.)));
            Ok(())
        });
        methods.add_method("push", |lua, s, item: mlua::Value| {
            let item = convert(item, None)?;
            let bytes = SLOT + item.weight();
            s.change_by(
                lua,
                |old| {
                    let items = list_of(old, "push")?;
                    // One allocation the right size: no regrowth.
                    let mut next = Vec::with_capacity(items.len() + 1);
                    next.extend_from_slice(items);
                    next.push(item);
                    Ok(Value::List(next.into()))
                },
                |_, held| held + bytes,
            )
        });
        methods.add_method("insert", |lua, s, (index, item): (f64, mlua::Value)| {
            let item = convert(item, None)?;
            let bytes = SLOT + item.weight();
            s.change_by(
                lua,
                |old| {
                    let items = list_of(old, "insert")?;
                    let len = items.len();
                    if index.fract() != 0. || index < 1. || index as usize > len + 1 {
                        return Err(fail(
                            "insert",
                            format!("index {index} is outside the list (1 to {})", len + 1),
                        ));
                    }
                    let at = index as usize - 1;
                    let mut next = Vec::with_capacity(len + 1);
                    next.extend_from_slice(&items[..at]);
                    next.push(item);
                    next.extend_from_slice(&items[at..]);
                    Ok(Value::List(next.into()))
                },
                |_, held| held + bytes,
            )
        });
        methods.add_method("remove", |lua, s, index: usize| {
            let next = s.0.with_untracked(|v| {
                let items = v.list();
                (1..=items.len()).contains(&index).then(|| {
                    let mut next = Vec::with_capacity(items.len() - 1);
                    next.extend_from_slice(&items[..index - 1]);
                    next.extend_from_slice(&items[index..]);
                    (items[index - 1].clone(), next)
                })
            });
            let Some((removed, next)) = next else {
                return Ok(Value::Nil);
            };
            let bytes = SLOT + removed.weight();
            s.charge(lua, |held| held.saturating_sub(bytes))?;
            s.write(lua, Value::List(next.into()));
            Ok(removed)
        });
    }
}
/// A plugin's handle to shared state the host exposes read only
/// (`ScriptHost::expose_shared_read`): it reads and follows the state as a
/// `State` does, and writing it is an error naming the key. Its own type,
/// so the app's states write without a check.
#[derive(Clone)]
pub(crate) struct ReadState(pub State, pub Rc<str>);
/// What a `State` writes with: a `ReadState` refuses each.
const WRITES: [&str; 10] = [
    "set", "update", "set_key", "append", "patch", "toggle", "add", "push", "insert", "remove",
];
impl UserData for ReadState {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        add_reads(methods, |s: &ReadState| &s.0);
        for name in WRITES {
            methods.add_method(name, move |_, s, _: mlua::MultiValue| -> mlua::Result<()> {
                Err(mlua::Error::runtime(format!(
                    "`{name}` on shared state `{}`: the app shares it with plugins read only",
                    s.1
                )))
            });
        }
    }
}
/// What a state's script handle reads: the same for a state and for a
/// read-only one (`ReadState`).
fn add_reads<T: 'static, M: UserDataMethods<T>>(
    methods: &mut M,
    state: impl Fn(&T) -> &State + Copy + 'static,
) {
    methods.add_method("get", move |_, s: &T, ()| Ok(state(s).fresh().get()));
    methods.add_method("peek", move |_, s: &T, ()| {
        Ok(state(s).fresh().with_untracked(Clone::clone))
    });
    // Of a derived list (a sorted view) as it is now: brought up to date
    // first, as `at` is, inside a batch that wrote what it derives from.
    methods.add_method("len", move |_, s: &T, ()| {
        Ok(state(s).fresh().with(|v| v.list().len()))
    });
    // Where the first item whose `field` is `value` is (from 1), or nil,
    // tracked as `get` is: `messages:index_of("id", 42)`, found without
    // making the list a table.
    methods.add_method(
        "index_of",
        move |_, s: &T, (field, value): (mlua::LuaString, mlua::Value)| {
            let value = convert(value, None)?;
            let field = field.to_str()?;
            Ok(state(s).fresh().with(|v| {
                v.list()
                    .iter()
                    .position(|item| item.field(&field) == Some(&value))
                    .map(|i| i + 1)
            }))
        },
    );
    // One item of a list (from 1), tracked as `get` is: read without
    // converting the list (a long one's rows, as a list shows them).
    methods.add_method("at", move |_, s: &T, index: usize| {
        Ok(state(s).fresh().with(|v| {
            index
                .checked_sub(1)
                .and_then(|i| v.list().get(i).cloned())
                .unwrap_or(Value::Nil)
        }))
    });
    // Where `query` is in the string held (tracked, as `get` is), read
    // where it is held: a long document is not copied into the script.
    methods.add_method(
        "find",
        move |lua, s: &T, (query, options): (mlua::LuaString, Option<Table>)| {
            let (case, limit) = search_options(options.as_ref())?;
            let signal = state(s).fresh();
            let needle = query.as_bytes();
            // Tracked as `get` is, found here or not.
            signal.with(|_| ());
            let found = recent_find(signal, &needle, case, limit).unwrap_or_else(|| {
                let found = Rc::new(
                    signal.with(|value| find_all(&string_pieces(value), &needle, case, limit)),
                );
                keep_find(signal, &needle, case, limit, found.clone());
                found
            });
            let (found, more) = (&found.0, found.1);
            let list = lua.create_table_with_capacity(found.len(), 0)?;
            for (i, &(start, end)) in found.iter().enumerate() {
                let pair = lua.create_table_with_capacity(2, 0)?;
                pair.raw_set(1, start + 1)?;
                pair.raw_set(2, end)?;
                list.raw_set(i + 1, pair)?;
            }
            Ok((list, more))
        },
    );
    // How many times (`find`'s matches, at most `limit`, and whether
    // there were more), with no list of them made for the script.
    methods.add_method(
        "count",
        move |_, s: &T, (query, options): (mlua::LuaString, Option<Table>)| {
            let (case, limit) = search_options(options.as_ref())?;
            let signal = state(s).fresh();
            let needle = query.as_bytes();
            // Tracked as `get` is, found here or not; what it finds kept
            // for `find` (the same search's matches) and the next count.
            signal.with(|_| ());
            let found = recent_find(signal, &needle, case, limit).unwrap_or_else(|| {
                let found = Rc::new(
                    signal.with(|value| find_all(&string_pieces(value), &needle, case, limit)),
                );
                keep_find(signal, &needle, case, limit, found.clone());
                found
            });
            Ok((found.0.len(), found.1))
        },
    );
}
/// `{ case, limit }`: with case unless `case = false`; no limit unless one.
fn search_options(options: Option<&Table>) -> mlua::Result<(bool, usize)> {
    Ok(match options {
        Some(o) => (
            o.get::<Option<bool>>("case")?.unwrap_or(true),
            o.get::<Option<f64>>("limit")?
                .map_or(usize::MAX, |n| n.max(0.) as usize),
        ),
        None => (true, usize::MAX),
    })
}
/// The bytes of the string `value` holds, a piece at a time (a long text's
/// chunks); none for anything else.
/// What a search found: its matches' byte ranges, and whether there were
/// more than its limit.
type Matches = Rc<(Vec<(usize, usize)>, bool)>;
/// A search lately made (`state:find`): of which state, at which version
/// (`Signal::version`), for what, and what it found.
struct Found {
    signal: zgx::reactive::WeakSignal<Value>,
    version: u64,
    needle: Box<[u8]>,
    case: bool,
    limit: usize,
    found: Matches,
}
/// Searches kept: a search box's query searched again as it was (a letter
/// typed and taken back, the search pane drawn again) is answered from here
/// while the text is unchanged, not by reading all of it.
const KEPT_FINDS: usize = 4;
thread_local! {
    static FINDS: RefCell<Vec<Found>> = const { RefCell::new(Vec::new()) };
}
/// What `find` last found for this search, if the text has not changed
/// since (the same state, alive, at the same version).
fn recent_find(signal: &Signal<Value>, needle: &[u8], case: bool, limit: usize) -> Option<Matches> {
    let version = signal.version();
    FINDS.with(|finds| {
        finds
            .borrow()
            .iter()
            .find(|f| {
                f.version == version
                    && f.case == case
                    && f.limit == limit
                    && *f.needle == *needle
                    && f.signal.upgrade().is_some_and(|s| s.same(signal))
            })
            .map(|f| f.found.clone())
    })
}
fn keep_find(signal: &Signal<Value>, needle: &[u8], case: bool, limit: usize, found: Matches) {
    FINDS.with(|finds| {
        let mut finds = finds.borrow_mut();
        // The dead and the stale go first, then the oldest.
        finds.retain(|f| f.signal.is_alive());
        if finds.len() >= KEPT_FINDS {
            finds.remove(0);
        }
        finds.push(Found {
            signal: signal.downgrade(),
            version: signal.version(),
            needle: needle.into(),
            case,
            limit,
            found,
        });
    });
}
pub(crate) fn string_pieces(value: &Value) -> Vec<&[u8]> {
    match value {
        Value::Str(text) => vec![text.as_bytes()],
        Value::Text(text) => text.chunks().map(str::as_bytes).collect(),
        _ => Vec::new(),
    }
}
/// Where `needle` is in the text `pieces` make, as byte ranges, at most
/// `limit` of them (and whether there were more), the next looked for after
/// the last: plain bytes, as `string.find(text, needle, at, true)` finds
/// them, or with ASCII letters' case folded, as `string.lower` folds it. A
/// piece at a time, the end of the one before it carried over so a match
/// across two is found.
pub(crate) fn find_all(
    pieces: &[&[u8]],
    needle: &[u8],
    case: bool,
    limit: usize,
) -> (Vec<(usize, usize)>, bool) {
    let mut found = Vec::new();
    let more = search(pieces, needle, case, limit, |range| found.push(range));
    (found, more)
}
/// `find_all`, each match told to `on` as it is found: whether there were
/// more than `limit`.
fn search(
    pieces: &[&[u8]],
    needle: &[u8],
    case: bool,
    limit: usize,
    mut on: impl FnMut((usize, usize)),
) -> bool {
    if needle.is_empty() {
        return false;
    }
    let needle = if case {
        needle.to_vec()
    } else {
        needle.to_ascii_lowercase()
    };
    let finder = memchr::memmem::Finder::new(&needle);
    let carry = needle.len() - 1;
    let mut window = Vec::new();
    // Where the next match may start: after the last one.
    let (mut next, mut offset, mut count) = (0_usize, 0_usize, 0_usize);
    for piece in pieces {
        // The window: the last `carry` bytes before this piece, then it.
        let keep = window.len().min(carry);
        window.drain(..window.len() - keep);
        let window_start = offset - keep;
        if case {
            window.extend_from_slice(piece);
        } else {
            window.extend(piece.iter().map(u8::to_ascii_lowercase));
        }
        offset += piece.len();
        let mut from = next.saturating_sub(window_start);
        while let Some(at) = finder.find(&window[from.min(window.len())..]) {
            let start = window_start + from + at;
            if count == limit {
                return true;
            }
            count += 1;
            on((start, start + needle.len()));
            next = start + needle.len();
            from = next - window_start;
        }
    }
    false
}
/// The list a list method works on (nil is an empty list).
fn list_of<'a>(value: &'a Value, method: &str) -> mlua::Result<&'a [Value]> {
    match value {
        Value::List(items) => Ok(items),
        Value::Nil => Ok(&[]),
        other => Err(fail(
            method,
            format!("the state holds {}, not a list", other.kind()),
        )),
    }
}

/// Keyed state that outlives any one evaluation of a script, so a reload keeps
/// counters, toggles and lists where they were.
pub struct Store {
    runtime: Runtime,
    signals: RefCell<HashMap<String, Signal<Value>>>,
    /// Keys a plugin's default made (`shared`), which the app's own default
    /// replaces.
    provisional: RefCell<std::collections::HashSet<String>>,
}
impl Store {
    /// An empty store on `runtime`.
    pub fn new(runtime: &Runtime) -> Self {
        Self {
            runtime: runtime.clone(),
            signals: RefCell::default(),
            provisional: RefCell::default(),
        }
    }
    /// Shared state `key` (`ui.shared`), as [`Store::state`], except that a
    /// plugin's default gives way to the app's: plugins load before the app,
    /// so without a host's value (`ScriptHost::shared`) the app's own
    /// default is what shows, as a Luau app run by `zgx run` declares it.
    pub(crate) fn shared(&self, key: &str, initial: Value, plugin: bool) -> Signal<Value> {
        let existing = self.signals.borrow().get(key).cloned();
        match existing {
            Some(signal) => {
                if !plugin && self.provisional.borrow_mut().remove(key) {
                    signal.set(initial);
                }
                signal
            }
            None => {
                if plugin {
                    self.provisional.borrow_mut().insert(key.to_owned());
                }
                self.state(key, initial)
            }
        }
    }
    /// The existing state for `key`, or new state starting at `initial`.
    pub fn state(&self, key: &str, initial: Value) -> Signal<Value> {
        self.signals
            .borrow_mut()
            .entry(key.to_owned())
            .or_insert_with(|| self.runtime.signal(initial))
            .clone()
    }
    /// The state for `key`, if there is any.
    pub fn get(&self, key: &str) -> Option<Signal<Value>> {
        self.signals.borrow().get(key).cloned()
    }
    /// Every state it holds, by key, in key order.
    pub fn entries(&self) -> Vec<(String, Signal<Value>)> {
        let mut out: Vec<_> = self
            .signals
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, Value)]) -> Value {
        Value::record(pairs.iter().map(|(k, v)| (*k, v.clone())))
    }
    fn key(k: &str) -> PathKey {
        PathKey::Key(k.into())
    }

    /// A userdata method checks its `self` as it did with the check inlined
    /// (research B9): a value that isn't userdata, another type's, one
    /// borrowed or destroyed, each with its error, and calls after them work.
    #[test]
    fn userdata_checks_keep_type_borrow_and_destruction_errors() {
        fn cause(error: &mlua::Error) -> &mlua::Error {
            match error {
                mlua::Error::CallbackError { cause: inner, .. }
                | mlua::Error::BadArgument { cause: inner, .. } => cause(inner),
                other => other,
            }
        }
        struct Other;
        impl UserData for Other {}
        let lua = Lua::new();
        let runtime = Runtime::new();
        let state = lua
            .create_userdata(State::new(runtime.signal(Value::Num(7.))))
            .unwrap();
        lua.globals().set("s", state.clone()).unwrap();
        let get: mlua::Function = lua.load("return s.get").eval().unwrap();
        assert_eq!(get.call::<f64>(state.clone()).unwrap(), 7.);
        let wrong = get.call::<f64>(3).unwrap_err();
        assert!(
            matches!(cause(&wrong), mlua::Error::FromLuaConversionError {
            from: "integer" | "number", to, message: Some(message)
        } if to == "userdata" && message == "expected userdata of type 'State'")
        );
        let other = lua.create_userdata(Other).unwrap();
        let wrong = get.call::<f64>(other).unwrap_err();
        assert!(matches!(cause(&wrong), mlua::Error::UserDataTypeMismatch));
        assert_eq!(get.call::<f64>(state.clone()).unwrap(), 7.);
        let guard = state.borrow_mut::<State>().unwrap();
        let borrowed = get.call::<f64>(state.clone()).unwrap_err();
        assert!(matches!(cause(&borrowed), mlua::Error::UserDataBorrowError));
        drop(guard);
        assert_eq!(get.call::<f64>(state.clone()).unwrap(), 7.);
        state.destroy().unwrap();
        let destroyed = get.call::<f64>(state).unwrap_err();
        assert!(matches!(cause(&destroyed), mlua::Error::UserDataDestructed));
    }
    #[test]
    fn a_luau_string_converts_as_its_text_invalid_bytes_replaced() {
        let lua = Lua::new();
        for bytes in [
            &b"plain ascii"[..],
            "d\u{e9}j\u{e0} vu \u{1f600}".as_bytes(),
            // A multibyte character cut in two, and a stray continuation byte.
            &"\u{e9}t\u{e9}".as_bytes()[..4],
            b"a\x80b",
            b"",
        ] {
            let string = lua.create_string(bytes).unwrap();
            let expected = string.to_string_lossy();
            match convert(mlua::Value::String(string), None).unwrap() {
                Value::Str(text) => assert_eq!(&*text, expected, "{bytes:?}"),
                other => panic!("{bytes:?} converted to {other:?}"),
            }
        }
    }

    #[test]
    fn a_map_finds_its_fields_given_in_any_order() {
        let value = Value::record([
            ("name", "Ada".into()),
            ("id", Value::Num(7.)),
            ("name", "Grace".into()),
        ]);
        assert_eq!(value.field("id"), Some(&Value::Num(7.)));
        assert_eq!(value.field("name"), Some(&"Grace".into()), "the last given");
        assert_eq!(
            value,
            map(&[("id", Value::Num(7.)), ("name", "Grace".into())])
        );
        assert_eq!(value.to_json(), r#"{"id":7,"name":"Grace"}"#);
    }

    #[test]
    fn reconcile_keeps_what_did_not_change() {
        let row = |id: f64, label: &str| map(&[("id", Value::Num(id)), ("label", label.into())]);
        let list = |labels: &[(f64, &str)]| {
            Value::List(
                labels
                    .iter()
                    .map(|(id, l)| row(*id, l))
                    .collect::<Vec<_>>()
                    .into(),
            )
        };
        let ptr = |v: &Value| identity(v);
        let old = list(&[(1., "a"), (2., "b"), (3., "c")]);
        // Equal throughout: the old value itself.
        let same = list(&[(1., "a"), (2., "b"), (3., "c")]).reconcile(&old);
        assert_eq!(ptr(&same), ptr(&old));
        // One edited in place: the others are the old items.
        let edited = list(&[(1., "a"), (2., "B"), (3., "c")]).reconcile(&old);
        assert_eq!(edited, list(&[(1., "a"), (2., "B"), (3., "c")]));
        assert_eq!(ptr(&edited.list()[0]), ptr(&old.list()[0]));
        assert_eq!(ptr(&edited.list()[2]), ptr(&old.list()[2]));
        assert_ne!(ptr(&edited.list()[1]), ptr(&old.list()[1]));
        // Reordered: matched by id, each the old item still.
        let moved = list(&[(3., "c"), (1., "a"), (2., "b")]).reconcile(&old);
        assert_eq!(ptr(&moved.list()[0]), ptr(&old.list()[2]));
        assert_eq!(ptr(&moved.list()[1]), ptr(&old.list()[0]));
    }

    #[test]
    fn set_path_shares_what_it_does_not_touch() {
        let tab = |t: &str| map(&[("title", t.into()), ("root", map(&[("id", "p".into())]))]);
        let doc = map(&[
            (
                "tabs",
                Value::List(vec![tab("a"), tab("b"), tab("c")].into()),
            ),
            ("active", "a".into()),
        ]);
        let next = doc
            .set_path(&[key("tabs"), PathKey::Index(2), key("title")], "x".into())
            .unwrap();
        let (Value::List(old), Value::List(new)) =
            (doc.field("tabs").unwrap(), next.field("tabs").unwrap())
        else {
            panic!("tabs are lists")
        };
        assert!(!Rc::ptr_eq(old, new));
        // Untouched tabs, and the touched tab's untouched root, are shared.
        let ptr = |v: &Value| match v {
            Value::Map(m) => Rc::as_ptr(m) as *const u8,
            _ => std::ptr::null(),
        };
        assert_eq!(ptr(&old[0]), ptr(&new[0]));
        assert_eq!(ptr(&old[2]), ptr(&new[2]));
        assert_eq!(
            ptr(old[1].field("root").unwrap()),
            ptr(new[1].field("root").unwrap())
        );
        assert_eq!(new[1].field("title"), Some(&"x".into()));
        // An equal write gives the very same value back.
        let same = doc
            .set_path(&[key("tabs"), PathKey::Index(2), key("title")], "b".into())
            .unwrap();
        assert_eq!(ptr(&same), ptr(&doc));
    }

    #[test]
    fn a_filtered_long_list_gives_back_its_own_rows() {
        // A mail list: thousands of small rows, read by a computed that
        // keeps some (a mailbox, a search). Its rows go back as the very
        // values they were, not walked field by field.
        let lua = Lua::new();
        install_cache(&lua);
        let rows: Vec<Value> = (0..200)
            .map(|i| map(&[("id", (i as f64).into()), ("subject", "Hello".into())]))
            .collect();
        let list = Value::List(rows.clone().into());
        lua.globals()
            .set("rows", shared_lua(&list, &lua).unwrap())
            .unwrap();
        let kept: mlua::Value = lua
            .load(
                "local out = {} for i, r in rows do if i % 2 == 0 then table.insert(out, r) end end return out",
            )
            .eval()
            .unwrap();
        let back = convert(kept, None).unwrap();
        let ptr = |v: &Value| match v {
            Value::Map(m) => Rc::as_ptr(m) as *const u8,
            _ => std::ptr::null(),
        };
        assert_eq!(back.list().len(), 100);
        for (i, row) in back.list().iter().enumerate() {
            assert_eq!(ptr(row), ptr(&rows[2 * i + 1]), "row {i}");
        }
    }

    /// A long string from a script is held in chunks; the same one set
    /// again (as another state) is the same text, and it reads back whole.
    #[test]
    fn a_long_string_set_twice_is_one_shared_text() {
        let lua = Lua::new();
        let long: String = (0..10_000).map(|i| format!("line {i}\n")).collect();
        let set = |text: &str| {
            convert(mlua::Value::String(lua.create_string(text).unwrap()), None).unwrap()
        };
        let (first, second) = (set(&long), set(&long));
        let (Value::Text(a), Value::Text(b)) = (&first, &second) else {
            panic!("a long string is a text");
        };
        assert!(a.ptr_eq(b));
        assert!(first == Value::Str(long.as_str().into()));
        let back = to_lua(&first, &lua, true).unwrap();
        assert!(*back.as_string().unwrap().as_bytes() == *long.as_bytes());
        assert!(matches!(set("short"), Value::Str(_)));
    }

    /// A search finds what `string.find` would, one match after another,
    /// across the pieces a long text is held in.
    #[test]
    fn a_search_finds_matches_across_pieces() {
        let whole = "abcABCabcabc";
        let expect = |text: &str, needle: &str| {
            let mut out = Vec::new();
            let mut at = 0;
            while let Some(i) = text[at..].find(needle) {
                out.push((at + i, at + i + needle.len()));
                at += i + needle.len();
            }
            out
        };
        for cut in 0..=whole.len() {
            let pieces = [&whole.as_bytes()[..cut], &whole.as_bytes()[cut..]];
            assert_eq!(
                find_all(&pieces, b"bca", true, usize::MAX).0,
                expect(whole, "bca")
            );
            let lower = whole.to_ascii_lowercase();
            assert_eq!(
                find_all(&pieces, b"BcA", false, usize::MAX).0,
                expect(&lower, "bca")
            );
        }
        // One after another, not overlapping: "aaaa" holds "aa" twice.
        assert_eq!(
            find_all(&[b"aa", b"aa"], b"aa", true, usize::MAX).0,
            [(0, 2), (2, 4)]
        );
        assert_eq!(find_all(&[b"aaaa"], b"aa", true, 1), (vec![(0, 2)], true));
        assert_eq!(find_all(&[b"aaaa"], b"", true, 9), (vec![], false));
    }

    #[test]
    fn set_path_adds_removes_and_refuses_holes() {
        let v = Value::Nil
            .set_path(&[key("a"), key("b")], 1.0.into())
            .unwrap();
        assert_eq!(v, map(&[("a", map(&[("b", 1.0.into())]))]));
        let v = v.set_path(&[key("a"), key("b")], Value::Nil).unwrap();
        assert_eq!(v, map(&[("a", Value::List(Rc::new([])))]));
        let list = Value::List(vec![1.0.into(), 2.0.into()].into());
        assert_eq!(
            list.set_path(&[PathKey::Index(3)], 3.0.into())
                .unwrap()
                .list()
                .len(),
            3
        );
        assert_eq!(
            list.set_path(&[PathKey::Index(2)], Value::Nil)
                .unwrap()
                .list()
                .len(),
            1
        );
        assert!(list.set_path(&[PathKey::Index(1)], Value::Nil).is_err());
        assert!(list.set_path(&[PathKey::Index(5)], 1.0.into()).is_err());
        assert!(list.set_path(&[key("x")], 1.0.into()).is_err());
        assert!(Value::Num(1.).set_path(&[key("x")], 1.0.into()).is_err());
    }
}
