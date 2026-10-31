//! What a script is handed when something it asked for fails, or when
//! `ui.try` catches what was raised: a `UiError`, the same shape from every
//! source (an action, a fetch, a file, a cancel, a script's `error(...)`),
//! so a script branches on `err.kind` or `err.status` instead of parsing
//! text.
//!
//! It is still text where text was expected: `tostring(err)` and `"…" ..
//! err` give its message, string methods read it (`err:find("404")`), a
//! state set to it holds its message, and a prop or a text given it shows
//! it. Made only when a script reads an error: success makes none.
use crate::value::Value;
use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods};
use std::rc::Rc;

/// Where a failure came from (`UiError.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A host action rejected (`ui.call`).
    Action,
    /// A request never got an answer: the transport failed (`ui.fetch`).
    Network,
    /// Cancelled: `p:cancel()`, a task or a resource cancelling its load.
    Cancelled,
    /// A response outside 2xx, from a fetch asked to raise on one
    /// (`ui.fetch(url, { raise = true })`): its status and body come too.
    Http,
    /// A file or the clipboard failed (`ui.read_file`, …).
    Io,
    /// A script raised it (`error(...)`, or a Rust function's error).
    Raised,
    /// The system can't do it here (`ui.notify` without an app bundle).
    Unsupported,
    /// The person has not allowed it (`ui.notify`'s permission).
    Denied,
}
impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Action => "action",
            Kind::Network => "network",
            Kind::Cancelled => "cancelled",
            Kind::Http => "http",
            Kind::Io => "io",
            Kind::Raised => "raised",
            Kind::Unsupported => "unsupported",
            Kind::Denied => "denied",
        }
    }
}

/// A failure as the host knows it: settled into a promise, handed to a
/// script as a `UiError`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Failure {
    pub(crate) kind: Kind,
    /// What a person reads: no traceback, no `runtime error:`.
    pub(crate) message: Rc<str>,
    /// An HTTP failure's status.
    pub(crate) status: Option<u16>,
    /// An HTTP failure's body: its JSON decoded, else its text.
    pub(crate) body: Value,
}
impl Failure {
    pub(crate) fn new(kind: Kind, message: impl Into<Rc<str>>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
            body: Value::Nil,
        }
    }
    pub(crate) fn cancelled() -> Self {
        Self::new(Kind::Cancelled, "cancelled")
    }
}

/// A failure as a script holds it; for one raised as a value (a table),
/// that value too.
pub(crate) struct UiError {
    pub(crate) failure: Rc<Failure>,
    raised: Option<mlua::Value>,
}
impl UiError {
    pub(crate) fn of(failure: Rc<Failure>) -> Self {
        Self {
            failure,
            raised: None,
        }
    }
    pub(crate) fn message(&self) -> &str {
        &self.failure.message
    }
}

/// An operand of `..` as text: an error's message, a string or a number.
fn concat_text(lua: &Lua, value: &mlua::Value) -> mlua::Result<String> {
    match value {
        mlua::Value::UserData(ud) if ud.is::<UiError>() => {
            Ok(ud.borrow::<UiError>()?.message().to_string())
        }
        mlua::Value::String(s) => Ok(s.to_str()?.to_string()),
        mlua::Value::Integer(n) => Ok(n.to_string()),
        mlua::Value::Number(_) => lua
            .coerce_string(value.clone())?
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| mlua::Error::runtime("attempt to concatenate a number")),
        other => Err(mlua::Error::runtime(format!(
            "attempt to concatenate an error with a {}",
            other.type_name()
        ))),
    }
}

impl UserData for UiError {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("message", |_, e| Ok(e.message().to_string()));
        fields.add_field_method_get("kind", |_, e| Ok(e.failure.kind.name()));
        fields.add_field_method_get("status", |_, e| Ok(e.failure.status));
        fields.add_field_method_get("body", |_, e| Ok(e.failure.body.clone()));
        fields.add_field_method_get("value", |_, e| Ok(e.raised.clone()));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, e, ()| Ok(e.message().to_string()));
        methods.add_meta_function(
            MetaMethod::Concat,
            |lua, (a, b): (mlua::Value, mlua::Value)| {
                Ok(concat_text(lua, &a)? + &concat_text(lua, &b)?)
            },
        );
        // What it was before it had fields: text. A string method reads its
        // message (`err:find("404")`, `err:upper()`).
        methods.add_meta_function(MetaMethod::Index, |lua, (_, key): (mlua::Value, String)| {
            let string: mlua::Table = lua.globals().get("string")?;
            match string.get::<Option<mlua::Function>>(key.as_str())? {
                Some(method) => {
                    let wrapped = lua.create_function(
                        move |_, (this, rest): (mlua::AnyUserData, mlua::MultiValue)| {
                            let message = this.borrow::<UiError>()?.message().to_string();
                            method.call::<mlua::MultiValue>((message, rest))
                        },
                    )?;
                    Ok(mlua::Value::Function(wrapped))
                }
                None => Err(mlua::Error::runtime(format!(
                    "an error has no `{key}` (its fields: message, kind, status, body, value)"
                ))),
            }
        });
    }
}

/// Text an API takes from a script (a toast's title and body): a string,
/// a number, or an error's message, so `body = err` shows what failed.
/// The component library's (`components`).
#[cfg(feature = "components")]
pub(crate) struct Text(pub(crate) String);
#[cfg(feature = "components")]
impl mlua::FromLua for Text {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        match &value {
            mlua::Value::UserData(ud) if ud.is::<UiError>() => {
                Ok(Text(ud.borrow::<UiError>()?.message().to_string()))
            }
            _ => String::from_lua(value, lua).map(Text),
        }
    }
}

/// What was raised, as a person reads it: a string as it is; a table's
/// `message`; anything else, a Rust function's error included, as its text
/// without the traceback.
fn message_of(lua: &Lua, raised: &mlua::Value) -> mlua::Result<String> {
    Ok(match raised {
        mlua::Value::String(text) => text.to_string_lossy(),
        mlua::Value::Table(table) if table.raw_get::<mlua::Value>("message")?.is_string() => {
            table.raw_get::<String>("message")?
        }
        other => {
            let text: String = lua
                .globals()
                .get::<mlua::Function>("tostring")?
                .call(other.clone())?;
            let text = match text.find("\nstack traceback:") {
                Some(cut) => text[..cut].to_string(),
                None => text,
            };
            text.strip_prefix("runtime error: ")
                .map(str::to_string)
                .unwrap_or(text)
        }
    })
}

/// What a load raised (`ui.paged`'s), as a failure: an error's own, else a
/// "raised" one with its message.
pub(crate) fn failure_of(lua: &Lua, raised: &mlua::Value) -> Rc<Failure> {
    if let mlua::Value::UserData(ud) = raised
        && let Ok(error) = ud.borrow::<UiError>()
    {
        return error.failure.clone();
    }
    let message = message_of(lua, raised).unwrap_or_else(|e| e.to_string());
    Rc::new(Failure::new(Kind::Raised, message))
}

/// `UiError`'s constructor for `ui.try` (`calls.rs`'s Luau, as
/// `ui.__error`): what was raised, as a person reads it (`message_of`),
/// kept as it was (`err.value`) unless it is a string or already an error.
pub(crate) fn raised(lua: &Lua) -> mlua::Result<mlua::Function> {
    lua.create_function(|lua, raised: mlua::Value| {
        if let mlua::Value::UserData(ud) = &raised
            && ud.is::<UiError>()
        {
            return Ok(mlua::Value::UserData(ud.clone()));
        }
        let message = message_of(lua, &raised)?;
        let kept = match &raised {
            mlua::Value::String(_) | mlua::Value::Nil => None,
            other => Some(other.clone()),
        };
        let error = UiError {
            failure: Rc::new(Failure::new(Kind::Raised, message)),
            raised: kept,
        };
        lua.create_userdata(error).map(mlua::Value::UserData)
    })
}
