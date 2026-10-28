//! Themes from scripts: `ui.theme` and the `theme = …` element property.
//!
//! Styles name tokens as strings: `bg = "$surface.raised"`. The string is
//! interned once, when the script is lowered, and the node styles itself
//! from the token, following theme changes without calling the script. The
//! runtime's themes (`zgx::theme::Themes`) are shared with the Rust
//! application, so a theme a script sets applies to Rust views too.
use crate::{node::env, value::Value};
use mlua::{Lua, Table};
use std::{path::PathBuf, rc::Rc};
use zgx::theme::{Theme, ThemeOverride, Themes, TokenInput, TokenValue, token};

/// What `theme = …` gives a subtree: a theme's name, or a table of token
/// overrides with an optional `theme` (or `extends`) to start from.
pub(crate) fn override_of(value: &Value) -> Result<ThemeOverride, String> {
    match value {
        Value::Nil | Value::Bool(false) => Ok(ThemeOverride::new()),
        Value::Str(name) => Ok(ThemeOverride::new().theme(&**name)),
        Value::Map(fields) => {
            let mut over = ThemeOverride::new();
            for (key, value) in fields.iter() {
                if &**key == "theme" || &**key == "extends" {
                    match value {
                        Value::Str(name) => over = over.theme(&**name),
                        _ => return Err(format!("`theme.{key}` is a theme's name")),
                    }
                    continue;
                }
                for (name, input) in inputs(key, value)? {
                    over = over.with(token(&name), input);
                }
            }
            Ok(over)
        }
        other => Err(format!(
            "`theme` takes a theme's name or a table of tokens, not {other}"
        )),
    }
}
/// A table of tokens (dotted names, or nested groups) as theme inputs.
fn inputs(name: &str, value: &Value) -> Result<Vec<(String, TokenInput)>, String> {
    match value {
        Value::Str(text) => {
            let input = TokenInput::parse(text).map_err(|e| format!("`{name}`: {e}"))?;
            Ok(vec![(name.to_owned(), input)])
        }
        Value::Num(n) => Ok(vec![(name.to_owned(), TokenInput::from(*n as f32))]),
        Value::Map(fields) => {
            let mut out = Vec::new();
            for (key, value) in fields.iter() {
                out.extend(inputs(&format!("{name}.{key}"), value)?);
            }
            Ok(out)
        }
        other => Err(format!("`{name}` cannot be {other}")),
    }
}
/// A Luau theme table (`{ name, type, extends, tokens }`) as a theme.
fn theme_of(value: &Value) -> Result<Theme, String> {
    Theme::from_json(&value.to_json()).map_err(|e| e.to_string())
}
/// A token's value as scripts see it: colours as `#rrggbbaa`, numbers,
/// font names, shadows as `{ x, y, blur, spread, color }` lists.
fn script_value(value: TokenValue) -> Value {
    match value {
        TokenValue::Unset => Value::Nil,
        TokenValue::Color(c) => {
            let zgx::scene::Color(r, g, b, a) = c;
            Value::Str(format!("#{r:02x}{g:02x}{b:02x}{a:02x}").into())
        }
        TokenValue::Number(n) => Value::Num(n as f64),
        TokenValue::Font(family) => Value::Str(zgx::theme::family_name(&family).into()),
        TokenValue::Text(text) => Value::Str(text.to_string().into()),
        TokenValue::Shadow(list) => Value::List(
            list.iter()
                .map(|s| {
                    let zgx::scene::Color(r, g, b, a) = s.color;
                    let color = format!("#{r:02x}{g:02x}{b:02x}{a:02x}");
                    let mut fields: Vec<(Rc<str>, Value)> = vec![
                        ("blur".into(), Value::Num((s.blur_radius * 2.) as f64)),
                        ("color".into(), Value::Str(color.into())),
                        ("spread".into(), Value::Num(s.spread as f64)),
                        ("x".into(), Value::Num(s.offset.x as f64)),
                        ("y".into(), Value::Num(s.offset.y as f64)),
                    ];
                    fields.sort_by(|a, b| a.0.cmp(&b.0));
                    Value::Map(fields.into())
                })
                .collect(),
        ),
    }
}

/// `path` resolved from the script's directory, or `@name/…` from one of
/// the app's packages: it must stay in one of them.
fn file(
    dir: &Option<PathBuf>,
    packages: &[(String, PathBuf)],
    path: &str,
) -> mlua::Result<PathBuf> {
    let Some(dir) = dir else {
        return Err(mlua::Error::runtime(
            "ui.theme.load needs a script loaded from a file",
        ));
    };
    let root = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    let package = |name: &str| packages.iter().find(|(n, _)| n == name).map(|(_, d)| d);
    let full = match path
        .strip_prefix('@')
        .map(|rest| rest.split_once('/').unwrap_or((rest, "")))
    {
        Some((name, rest)) => {
            let Some(dir) = package(name) else {
                return Err(mlua::Error::runtime(format!(
                    "ui.theme.load(\"{path}\"): no package `{name}`"
                )));
            };
            crate::normalize(&dir.join(rest))
        }
        None => crate::normalize(&root.join(path)),
    };
    if !crate::within(&root, &full) && !packages.iter().any(|(_, dir)| crate::within(dir, &full)) {
        return Err(mlua::Error::runtime(format!(
            "ui.theme.load(\"{path}\"): theme files come from the script's directory or below, \
             or a package"
        )));
    }
    Ok(full)
}

/// Run `f` off the script's clock: what a theme change restyles is the
/// engine's work, not the script's.
fn off_clock<R>(lua: &Lua, f: impl FnOnce() -> R) -> R {
    let vm = lua
        .app_data_ref::<crate::node::Env>()
        .and_then(|e| e.vm.upgrade());
    match vm {
        Some(vm) => vm.off_clock(f),
        None => f(),
    }
}
fn themes(lua: &Lua) -> mlua::Result<Rc<Themes>> {
    Ok(Themes::of(&env(lua)?.runtime))
}
fn fail(e: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(e.to_string())
}

/// `ui.tokens`: every token as a path of fields, its value the token's
/// string (`ui.tokens.surface.raised` is `"$surface.raised"`), for editors
/// to complete and type check (`zgx types`). A group that is a token too
/// has it as `value` (`ui.tokens.accent.value`, beside `.hover`); a
/// number in a name is an index (`ui.tokens.space[3]`). Built from the
/// applied theme the first time it is read, and again once its tokens
/// change (`Themes::revision`): a script that never reads it builds
/// nothing. A name no token has is an error, as its type says.
fn install_tokens(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    let built: Rc<std::cell::RefCell<Option<(u64, Table)>>> = Rc::default();
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (_, key): (mlua::Value, mlua::Value)| {
            let themes = themes(lua)?;
            let revision = themes.revision();
            let cached = built
                .borrow()
                .as_ref()
                .filter(|(at, _)| *at == revision)
                .map(|(_, table)| table.clone());
            let table = match cached {
                Some(table) => table,
                None => {
                    let table = token_tree(lua, &themes.token_names())?;
                    *built.borrow_mut() = Some((revision, table.clone()));
                    table
                }
            };
            missing(table.raw_get(key.clone())?, "", &key)
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    proxy.set_readonly(true);
    ui.set("tokens", proxy)
}
/// `value`, or the error a name no token has gets.
fn missing(value: mlua::Value, group: &str, key: &mlua::Value) -> mlua::Result<mlua::Value> {
    if !value.is_nil() {
        return Ok(value);
    }
    let key = match key {
        mlua::Value::String(s) => s.to_string_lossy(),
        mlua::Value::Integer(i) => i.to_string(),
        other => format!("{other:?}"),
    };
    let name = if group.is_empty() {
        key
    } else {
        format!("{group}.{key}")
    };
    Err(fail(format!(
        "ui.tokens has no `{name}`: no token or group of tokens has that name"
    )))
}
/// The tables of `ui.tokens` for `names`.
fn token_tree(lua: &Lua, names: &[std::sync::Arc<str>]) -> mlua::Result<Table> {
    let root = lua.create_table()?;
    let mut groups: Vec<Table> = Vec::new();
    let key_of = |part: &str| -> mlua::Result<mlua::Value> {
        Ok(match part.parse::<i64>() {
            Ok(n) if !part.starts_with('0') || part == "0" => mlua::Value::Integer(n),
            _ => mlua::Value::String(lua.create_string(part)?),
        })
    };
    let group = |path: &str, groups: &mut Vec<Table>| -> mlua::Result<Table> {
        let table = lua.create_table()?;
        let meta = lua.create_table()?;
        let path = path.to_owned();
        meta.set(
            "__index",
            lua.create_function(move |_, (_, key): (mlua::Value, mlua::Value)| {
                missing(mlua::Value::Nil, &path, &key)
            })?,
        )?;
        table.set_metatable(Some(meta))?;
        groups.push(table.clone());
        Ok(table)
    };
    for name in names {
        let leaf = mlua::Value::String(lua.create_string(format!("${name}"))?);
        let parts: Vec<&str> = name.split('.').collect();
        let mut node = root.clone();
        for (i, part) in parts.iter().enumerate() {
            let key = key_of(part)?;
            let existing: mlua::Value = node.raw_get(key.clone())?;
            if i + 1 == parts.len() {
                match existing {
                    mlua::Value::Table(table) => table.raw_set("value", leaf.clone())?,
                    _ => node.raw_set(key, leaf.clone())?,
                }
                break;
            }
            let path = parts[..=i].join(".");
            node = match existing {
                mlua::Value::Table(table) => table,
                // A token that is a group too: its own value as `value`.
                value => {
                    let table = group(&path, &mut groups)?;
                    if !value.is_nil() {
                        table.raw_set("value", value)?;
                    }
                    node.raw_set(key, &table)?;
                    table
                }
            };
        }
    }
    for table in groups {
        table.set_readonly(true);
    }
    root.set_readonly(true);
    Ok(root)
}

/// Installs `ui.theme`.
pub(crate) fn install(
    lua: &Lua,
    ui: &Table,
    dir: Option<PathBuf>,
    packages: Vec<(String, PathBuf)>,
) -> mlua::Result<()> {
    let theme = lua.create_table()?;
    theme.set(
        "set",
        lua.create_function(|lua, name: String| {
            let themes = themes(lua)?;
            off_clock(lua, || themes.set(&name)).map_err(fail)
        })?,
    )?;
    theme.set(
        "default",
        lua.create_function(|lua, name: String| {
            let themes = themes(lua)?;
            off_clock(lua, || themes.set_default(&name)).map_err(fail)
        })?,
    )?;
    theme.set(
        "names",
        lua.create_function(|lua, ()| Ok(themes(lua)?.names()))?,
    )?;
    theme.set(
        "load",
        lua.create_function(move |lua, path: String| {
            let path = file(&dir, &packages, &path)?;
            let themes = themes(lua)?;
            off_clock(lua, || themes.load(&path)).map_err(fail)
        })?,
    )?;
    theme.set(
        "register",
        lua.create_function(|lua, table: Value| {
            let theme = theme_of(&table).map_err(fail)?;
            let name = theme.name.clone();
            let themes = themes(lua)?;
            off_clock(lua, || themes.register(theme));
            Ok(name)
        })?,
    )?;
    theme.set(
        "tokens",
        lua.create_function(|lua, table: Value| {
            let Value::Map(fields) = &table else {
                return Err(fail("ui.theme.tokens takes a table of token defaults"));
            };
            let mut declared = Vec::new();
            for (key, value) in fields.iter() {
                declared.extend(inputs(key, value).map_err(fail)?);
            }
            let themes = themes(lua)?;
            off_clock(lua, || {
                for (name, input) in declared {
                    themes.declare(token(&name), input);
                }
            });
            Ok(())
        })?,
    )?;
    theme.set(
        "get",
        lua.create_function(|lua, name: String| Ok(script_value(themes(lua)?.get(token(&name)))))?,
    )?;
    theme.set(
        "strict",
        lua.create_function(|lua, mode: Value| {
            let mode = match &mode {
                Value::Bool(on) => Some(if *on {
                    crate::DesignLint::Warn
                } else {
                    crate::DesignLint::Off
                }),
                Value::Str(s) => crate::DesignLint::parse(s),
                _ => None,
            }
            .ok_or_else(|| {
                fail("ui.theme.strict takes \"warn\", \"error\", \"off\" or a boolean")
            })?;
            let env = crate::node::env(lua)?;
            let mut warnings = env.warnings.borrow_mut();
            if !warnings.design_forced {
                warnings.design = mode;
            }
            Ok(())
        })?,
    )?;
    theme.set(
        "follow_system",
        lua.create_function(|lua, pair: Value| {
            let themes = themes(lua)?;
            let names = match &pair {
                Value::Nil | Value::Bool(false) => None,
                Value::Map(_) => {
                    let name = |key: &str| match pair.field(key) {
                        Some(Value::Str(s)) => Ok(s.to_string()),
                        _ => Err(fail(format!(
                            "ui.theme.follow_system takes {{ dark = name, light = name }}, without `{key}`"
                        ))),
                    };
                    Some((name("dark")?, name("light")?))
                }
                _ => {
                    return Err(fail(
                        "ui.theme.follow_system takes { dark = name, light = name } or false",
                    ));
                }
            };
            off_clock(lua, || {
                themes.follow_system(names.as_ref().map(|(d, l)| (d.as_str(), l.as_str())))
            });
            Ok(())
        })?,
    )?;
    // `ui.theme.current` is state holding the applied theme's name: `:get()`
    // reads it (tracked, so what reads it follows), `:set(name)` applies a
    // theme. (A field could not follow: Luau caches `ui.theme.current`.)
    let runtime = env(lua)?.runtime.clone();
    let shared = Themes::of(&runtime);
    let name = runtime.untracked(|| shared.current());
    let current = runtime.signal(Value::Str(name.as_ref().into()));
    let (from, mirror) = (shared.clone(), current.clone());
    let follow = runtime.effect(move || {
        mirror.set(Value::Str(from.current().as_ref().into()));
    });
    let (to, chosen, untracked) = (shared, current.clone(), runtime.clone());
    let apply = runtime.effect(move || {
        let Value::Str(name) = chosen.get() else {
            return;
        };
        let applied = untracked.untracked(|| to.current());
        if *name != *applied
            && let Err(e) = to.set(&name)
        {
            eprintln!("ui.theme.current: {e}");
            chosen.set(Value::Str(applied.as_ref().into()));
        }
    });
    {
        let env = env(lua)?;
        let mut retained = env.retained.borrow_mut();
        retained.push(Box::new(follow));
        retained.push(Box::new(apply));
    }
    theme.set("current", crate::value::State::new(current))?;
    theme.set(
        "kind",
        lua.create_function(|lua, ()| {
            let themes = themes(lua)?;
            themes.current();
            Ok(themes.kind().as_str())
        })?,
    )?;
    theme.set_readonly(true);
    ui.set("theme", theme)?;
    install_tokens(lua, ui)?;
    // `ui.prefers`: the system's display settings as CSS's media queries
    // read them, each state that follows the system (`:get()` tracked).
    let prefers = lua.create_table()?;
    let runtime = env(lua)?.runtime.clone();
    let shared = Themes::of(&runtime);
    type Setting = fn(zgx::theme::SystemPreferences) -> bool;
    let settings: [(&str, Setting); 3] = [
        ("reduced_motion", |p| p.reduced_motion),
        ("more_contrast", |p| p.increased_contrast),
        ("reduced_transparency", |p| p.reduced_transparency),
    ];
    for (name, read) in settings {
        let initial = runtime.untracked(|| read(shared.system_preferences()));
        let state = runtime.signal(Value::Bool(initial));
        let (from, mirror) = (shared.clone(), state.clone());
        let follow = runtime.effect(move || {
            mirror.set(Value::Bool(read(from.system_preferences())));
        });
        env(lua)?.retained.borrow_mut().push(Box::new(follow));
        prefers.set(name, crate::value::State::new(state))?;
    }
    prefers.set_readonly(true);
    ui.set("prefers", prefers)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn element_overrides_read_names_and_token_tables() {
        let over = override_of(&Value::Str("zgx-light".into())).unwrap();
        assert_eq!(over, ThemeOverride::new().theme("zgx-light"));
        let table = Value::Map(
            vec![
                (Rc::from("accent"), Value::Str("#e5484d".into())),
                (Rc::from("theme"), Value::Str("zgx-light".into())),
            ]
            .into(),
        );
        let over = override_of(&table).unwrap();
        assert_eq!(
            over,
            ThemeOverride::new()
                .theme("zgx-light")
                .with_expr(zgx::theme::tok::ACCENT, "#e5484d")
                .unwrap()
        );
        assert!(override_of(&Value::Num(1.)).is_err());
    }
}
