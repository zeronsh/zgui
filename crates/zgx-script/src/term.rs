//! Terminal sessions for scripts: `ui.term(id, opts)` returns the session
//! for `id`, starting it the first time; `ui.terminal { term = t }` shows it.
//!
//! Sessions belong to the host, not to a script run: reloading keeps every
//! shell running, and the run that asks for `id` again gets the same session
//! (its handlers replace the previous run's). A session ends with
//! `term:close()`, or when its process exits and the script closes it.
use crate::{Vm, node::env};
use mlua::{Function, Lua, Table, UserData, Value as LuaValue};
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::{Rc, Weak},
};
use zgx::{
    reactive::{Effect, Runtime, Signal},
    terminal::{Terminal, TerminalEvent, TerminalOptions, TerminalState, TerminalSubscription},
};

/// What a session's events call: the latest run's handlers.
#[derive(Default)]
struct Handlers {
    vm: Weak<Vm>,
    title: Option<Function>,
    pwd: Option<Function>,
    process: Option<Function>,
    exit: Option<Function>,
    close: Option<Function>,
    bell: Option<Function>,
    command: Option<Function>,
    notify: Option<Function>,
}
const HANDLERS: [&str; 8] = [
    "on_title",
    "on_pwd",
    "on_process",
    "on_exit",
    "on_close",
    "on_bell",
    "on_command",
    "on_notify",
];

struct Entry {
    term: Terminal,
    handlers: Rc<RefCell<Handlers>>,
    fields: Rc<Fields>,
    _subscription: TerminalSubscription,
}

/// The fields of a session's state scripts read, each its own signal: a
/// read follows its field, not the whole state, so a resize (`columns`,
/// `rows`) reruns nothing that reads `title` or `process`. Made on the first
/// read of each, kept up by an effect that compares.
#[derive(Default)]
struct Fields(RefCell<HashMap<&'static str, Field>>);
/// A field's signal (a `Signal<T>`) and the effect keeping it up.
type Field = (Rc<dyn Any>, Effect);
impl Fields {
    fn read<T: Clone + PartialEq + 'static>(
        &self,
        runtime: &Runtime,
        state: &Signal<TerminalState>,
        name: &'static str,
        get: fn(&TerminalState) -> T,
    ) -> T {
        let known = self.0.borrow().get(name).map(|(signal, _)| signal.clone());
        let signal = match known.and_then(|signal| signal.downcast::<Signal<T>>().ok()) {
            Some(signal) => signal,
            None => {
                let signal = runtime.signal(state.with_untracked(get));
                let effect = runtime.untracked(|| {
                    let (state, field) = (state.clone(), signal.clone());
                    runtime.effect(move || {
                        field.set(state.with(get));
                    })
                });
                self.0
                    .borrow_mut()
                    .insert(name, (Rc::new(signal.clone()), effect));
                Rc::new(signal)
            }
        };
        signal.get()
    }
}
/// The host's sessions by id.
#[derive(Default)]
pub(crate) struct Terms {
    entries: RefCell<HashMap<String, Entry>>,
}
impl Terms {
    fn session(
        self: &Rc<Self>,
        lua: &Lua,
        id: &str,
        opts: Option<&Table>,
    ) -> mlua::Result<(Terminal, Rc<Fields>)> {
        let env = env(lua)?;
        let mut handlers = Handlers {
            vm: env.vm.clone(),
            ..Default::default()
        };
        if let Some(opts) = opts {
            let get = |key: &str| opts.get::<Option<Function>>(key);
            handlers.title = get("on_title")?;
            handlers.pwd = get("on_pwd")?;
            handlers.process = get("on_process")?;
            handlers.exit = get("on_exit")?;
            handlers.close = get("on_close")?;
            handlers.bell = get("on_bell")?;
            handlers.command = get("on_command")?;
            handlers.notify = get("on_notify")?;
        }
        let mut entries = self.entries.borrow_mut();
        if let Some(entry) = entries.get(id)
            && !entry.term.is_closed()
        {
            // A lookup (no options) keeps the handlers; options replace them,
            // and say how it starts if nothing has shown it yet (a lookup
            // made it first, as a list reading every pane's state does).
            if let Some(opts) = opts {
                *entry.handlers.borrow_mut() = handlers;
                entry.term.set_options(options(opts)?);
            }
            return Ok((entry.term.clone(), entry.fields.clone()));
        }
        let options = match opts {
            Some(opts) => options(opts)?,
            None => TerminalOptions::default(),
        };
        let term = Terminal::new(&env.runtime, options);
        let handlers = Rc::new(RefCell::new(handlers));
        let fields = Rc::new(Fields::default());
        let subscription = {
            let handlers = Rc::downgrade(&handlers);
            term.on_event(move |event| {
                if let Some(handlers) = handlers.upgrade() {
                    dispatch(&handlers.borrow(), event);
                }
            })
        };
        entries.insert(
            id.to_owned(),
            Entry {
                term: term.clone(),
                handlers,
                fields: fields.clone(),
                _subscription: subscription,
            },
        );
        Ok((term, fields))
    }
    fn remove(&self, id: &str) {
        let entry = self.entries.borrow_mut().remove(id);
        if let Some(entry) = entry {
            entry.term.close();
        }
    }
}

fn options(opts: &Table) -> mlua::Result<TerminalOptions> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let working_directory =
        opts.get::<Option<String>>("cwd")?
            .map(|dir| match (dir.strip_prefix("~"), &home) {
                (Some(rest), Some(home)) => home.join(rest.trim_start_matches('/')),
                _ => PathBuf::from(dir),
            });
    let mut env = Vec::new();
    if let Some(table) = opts.get::<Option<Table>>("env")? {
        for pair in table.pairs::<String, String>() {
            env.push(pair?);
        }
    }
    Ok(TerminalOptions {
        command: opts.get::<Option<String>>("command")?,
        working_directory,
        env,
        font_size: opts.get::<Option<f32>>("font_size")?,
    })
}

fn dispatch(handlers: &Handlers, event: &TerminalEvent) {
    let Some(vm) = handlers.vm.upgrade() else {
        return;
    };
    let call = |f: &Option<Function>, args: mlua::MultiValue| {
        if let Some(f) = f {
            let result = vm.runtime.clone().batch(|| vm.call::<()>(f, args));
            if let Err(error) = result {
                vm.report(error);
            }
        }
    };
    let lua = &vm.lua;
    let pack = |values: Vec<LuaValue>| mlua::MultiValue::from_vec(values);
    let text = |s: &str| {
        lua.create_string(s)
            .map(LuaValue::String)
            .unwrap_or(LuaValue::Nil)
    };
    match event {
        TerminalEvent::TitleChanged { title } => call(&handlers.title, pack(vec![text(title)])),
        TerminalEvent::PwdChanged { pwd } => call(&handlers.pwd, pack(vec![text(pwd)])),
        TerminalEvent::ProcessChanged { process, .. } => {
            call(&handlers.process, pack(vec![text(process)]))
        }
        TerminalEvent::Exited { code } => {
            call(&handlers.exit, pack(vec![LuaValue::Integer(*code as i64)]))
        }
        TerminalEvent::CloseRequested { process_alive } => call(
            &handlers.close,
            pack(vec![LuaValue::Boolean(*process_alive)]),
        ),
        TerminalEvent::Bell => call(&handlers.bell, pack(vec![])),
        TerminalEvent::CommandFinished {
            exit_code,
            duration_ms,
        } => call(
            &handlers.command,
            pack(vec![
                exit_code.map_or(LuaValue::Nil, |c| LuaValue::Integer(c as i64)),
                LuaValue::Number(*duration_ms as f64),
            ]),
        ),
        TerminalEvent::Notification { title, body } => {
            call(&handlers.notify, pack(vec![text(title), text(body)]))
        }
    }
}

/// `ui.term(id)`'s value: a session's controller.
#[derive(Clone)]
pub(crate) struct TermUd {
    pub id: String,
    pub term: Terminal,
    terms: Weak<Terms>,
    fields: Rc<Fields>,
}
impl TermUd {
    /// A state field, tracked alone (see `Fields`).
    fn field<T: Clone + PartialEq + 'static>(
        &self,
        lua: &Lua,
        name: &'static str,
        get: fn(&TerminalState) -> T,
    ) -> mlua::Result<T> {
        let env = env(lua)?;
        Ok(self
            .fields
            .read(&env.runtime, &self.term.state(), name, get))
    }
}
impl UserData for TermUd {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Reads are tracked, each field alone: a text or computed that reads
        // one follows it.
        fields.add_field_method_get("id", |_, t| Ok(t.id.clone()));
        fields.add_field_method_get("title", |lua, t| t.field(lua, "title", |s| s.title.clone()));
        fields.add_field_method_get("pwd", |lua, t| t.field(lua, "pwd", |s| s.pwd.clone()));
        fields.add_field_method_get("process", |lua, t| {
            t.field(lua, "process", |s| s.process.clone())
        });
        fields.add_field_method_get("pid", |lua, t| t.field(lua, "pid", |s| s.pid));
        fields.add_field_method_get("focused", |lua, t| t.field(lua, "focused", |s| s.focused));
        fields.add_field_method_get("bell", |lua, t| t.field(lua, "bell", |s| s.bell));
        fields.add_field_method_get("progress", |lua, t| {
            t.field(lua, "progress", |s| s.progress)
        });
        fields.add_field_method_get("progress_error", |lua, t| {
            t.field(lua, "progress_error", |s| s.progress_error)
        });
        // What the program reports of itself (OSC 7501), as a table; nil
        // until it reports.
        fields.add_field_method_get("status", |lua, t| {
            let Some(status) = t.field(lua, "status", |s| s.status.clone())? else {
                return Ok(mlua::Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("state", status.state.name())?;
            table.set("kind", status.kind)?;
            table.set("progress", status.progress)?;
            table.set("app", status.app)?;
            table.set("title", status.title)?;
            table.set("msg", status.msg)?;
            table.set("child_blocked", status.child_blocked)?;
            table.set("since", status.since_ms as f64 / 1000.)?;
            Ok(mlua::Value::Table(table))
        });
        fields.add_field_method_get("exited", |lua, t| t.field(lua, "exited", |s| s.exited));
        fields.add_field_method_get("columns", |lua, t| t.field(lua, "columns", |s| s.columns));
        fields.add_field_method_get("rows", |lua, t| t.field(lua, "rows", |s| s.rows));
        // `{ x, y }` in points from the top left, or nil outside.
        fields.add_field_method_get("pointer", |lua, t| {
            t.term
                .pointer()
                .map(|(x, y)| {
                    let table = lua.create_table()?;
                    table.set("x", x)?;
                    table.set("y", y)?;
                    Ok(table)
                })
                .transpose()
        });
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("focus", |_, t, ()| {
            t.term.focus();
            Ok(())
        });
        methods.add_method("blur", |_, t, ()| {
            t.term.blur();
            Ok(())
        });
        methods.add_method("input", |_, t, text: String| {
            t.term.input(text);
            Ok(())
        });
        methods.add_method("paste", |_, t, text: String| {
            t.term.paste(text);
            Ok(())
        });
        // A Ghostty binding action: "copy_to_clipboard", "clear_screen", ...
        methods.add_method("binding", |_, t, action: String| {
            t.term.binding(action);
            Ok(())
        });
        // Whether closing would end something other than an idle shell.
        methods.add_method("needs_confirm", |_, t, ()| Ok(t.term.needs_confirm_close()));
        methods.add_method("close", |_, t, ()| {
            match t.terms.upgrade() {
                Some(terms) => terms.remove(&t.id),
                None => t.term.close(),
            }
            Ok(())
        });
    }
}

/// Installs `ui.term`.
pub(crate) fn install(lua: &Lua, ui: &Table, terms: Rc<Terms>) -> mlua::Result<()> {
    ui.set(
        "term",
        lua.create_function(move |lua, (id, opts): (String, Option<Table>)| {
            crate::node::env(lua)?.may(crate::Capability::Terminals, "ui.term")?;
            if let Some(opts) = &opts {
                for (key, _) in opts.clone().pairs::<String, LuaValue>().flatten() {
                    let known = matches!(key.as_str(), "command" | "cwd" | "env" | "font_size")
                        || HANDLERS.contains(&key.as_str());
                    if !known {
                        return Err(mlua::Error::runtime(format!(
                            "ui.term: unknown option `{key}`"
                        )));
                    }
                }
            }
            let (term, fields) = terms.session(lua, &id, opts.as_ref())?;
            Ok(TermUd {
                id,
                term,
                terms: Rc::downgrade(&terms),
                fields,
            })
        })?,
    )
}
