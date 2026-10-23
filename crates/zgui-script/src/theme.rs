//! Themes from scripts: `ui.theme` and the `theme = …` element property.
//!
//! Styles name tokens as strings: `bg = "$surface.raised"`. The string is
//! interned once, when the script is lowered, and the node styles itself
//! from the token, following theme changes without calling the script. The
//! runtime's themes (`zgui::theme::Themes`) are shared with the Rust
//! application, so a theme a script sets applies to Rust views too.
use crate::{node::env, value::Value};
use mlua::{Lua, Table};
use std::{path::PathBuf, rc::Rc};
use zgui::theme::{Theme, ThemeOverride, Themes, TokenInput, TokenValue, token};

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
            let zgui::scene::Color(r, g, b, a) = c;
            Value::Str(format!("#{r:02x}{g:02x}{b:02x}{a:02x}").into())
        }
        TokenValue::Number(n) => Value::Num(n as f64),
        TokenValue::Font(family) => Value::Str(zgui::theme::family_name(&family).into()),
        TokenValue::Text(text) => Value::Str(text.to_string().into()),
        TokenValue::Shadow(list) => Value::List(
            list.iter()
                .map(|s| {
                    let zgui::scene::Color(r, g, b, a) = s.color;
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

/// `path` resolved from the script's directory, which it must stay in.
fn file(dir: &Option<PathBuf>, path: &str) -> mlua::Result<PathBuf> {
    let Some(dir) = dir else {
        return Err(mlua::Error::runtime(
            "ui.theme.load needs a script loaded from a file",
        ));
    };
    let root = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    let full = crate::normalize(&root.join(path));
    if !full.starts_with(&root) {
        return Err(mlua::Error::runtime(format!(
            "ui.theme.load(\"{path}\"): theme files come from the script's directory or below"
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

/// Installs `ui.theme`.
pub(crate) fn install(lua: &Lua, ui: &Table, dir: Option<PathBuf>) -> mlua::Result<()> {
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
            let path = file(&dir, &path)?;
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
    theme.set("current", crate::value::State(current))?;
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
    // `ui.prefers`: the system's display settings as CSS's media queries
    // read them, each state that follows the system (`:get()` tracked).
    let prefers = lua.create_table()?;
    let runtime = env(lua)?.runtime.clone();
    let shared = Themes::of(&runtime);
    type Setting = fn(zgui::theme::SystemPreferences) -> bool;
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
        prefers.set(name, crate::value::State(state))?;
    }
    prefers.set_readonly(true);
    ui.set("prefers", prefers)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn element_overrides_read_names_and_token_tables() {
        let over = override_of(&Value::Str("zgui-light".into())).unwrap();
        assert_eq!(over, ThemeOverride::new().theme("zgui-light"));
        let table = Value::Map(
            vec![
                (Rc::from("accent"), Value::Str("#e5484d".into())),
                (Rc::from("theme"), Value::Str("zgui-light".into())),
            ]
            .into(),
        );
        let over = override_of(&table).unwrap();
        assert_eq!(
            over,
            ThemeOverride::new()
                .theme("zgui-light")
                .with_expr(zgui::theme::tok::ACCENT, "#e5484d")
                .unwrap()
        );
        assert!(override_of(&Value::Num(1.)).is_err());
    }
}
