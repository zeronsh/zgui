//! Data helpers whose Luau versions cost too much at an app's sizes, or
//! that every app writes again: `ui.sort`, which sorts a list by a key
//! without calling a comparator for every comparison, `ui.number`, numbers
//! formatted as `Intl.NumberFormat` would, and `ui.json`, a document saved
//! and opened.
use mlua::{Lua, Table, ffi};
use std::{cmp::Ordering, ffi::c_int};

/// A sort key: numbers before strings before booleans; nil (no key) last.
/// A string borrows the key list's string, which the list keeps alive.
#[derive(Clone, Copy)]
enum Key<'a> {
    Num(f64),
    Str(&'a [u8]),
    Bool(bool),
    Nil,
}
impl Key<'_> {
    fn rank(&self) -> u8 {
        match self {
            Key::Num(_) => 0,
            Key::Str(_) => 1,
            Key::Bool(_) => 2,
            Key::Nil => 3,
        }
    }
    fn cmp(&self, other: &Key) -> Ordering {
        match (self, other) {
            (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
            (Key::Str(a), Key::Str(b)) => a.cmp(b),
            (Key::Bool(a), Key::Bool(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

/// `ui.sort(list, by, descending)`: sorts `list` in place, stably, by `by`,
/// a field of each item (`"price"`) or a function of it, called once an
/// item; returns `list`. `table.sort` with a comparator calls Luau
/// n·log(n) times (some 1.7 million for 100,000 rows: 115 ms, over a
/// call's budget). Here Luau reads each key once, into a list, and Rust
/// sorts by it (8 ms). Nil keys go last either way.
const SORT: &str = r#"
local sort_by_keys = ...
return function(list, by, descending)
	local n = #list
	local keys = table.create(n)
	if type(by) == "string" then
		for i = 1, n do
			keys[i] = list[i][by]
		end
	elseif type(by) == "function" then
		for i = 1, n do
			keys[i] = by(list[i])
		end
	else
		error(`ui.sort sorts by a field's name or a function, not {typeof(by)}`, 2)
	end
	sort_by_keys(list, keys, descending == true)
	return list
end
"#;

/// Reorder `list` (stack index 1) by `keys` (index 2), as `SORT` says.
/// Reads and writes go through Luau's C API: through mlua, each read costs
/// more than the whole Luau loop that made the keys (34 ms for 100,000).
///
/// SAFETY: `state` is a Luau state whose stack holds two tables at 1 and 2.
/// Nothing here raises a Luau error (which would skip this frame's drops):
/// raw reads, type checks, and raw writes to slots that hold a value
/// already (no new key, so no allocation) of a table checked writable.
unsafe fn sort_by_keys(state: *mut ffi::lua_State, descending: bool) -> Result<(), String> {
    let (list, keys) = (1, 2);
    // SAFETY: reads of the two tables (`lua_objlen` takes no metamethod).
    let n = unsafe { ffi::lua_objlen(state, list) };
    if n > c_int::MAX as usize {
        return Err("ui.sort: the list is too long".into());
    }
    if unsafe { ffi::lua_getreadonly(state, list) } != 0 {
        return Err(
            "ui.sort sorts a list in place: this one is read-only (state's values \
                    are; sort a copy, `table.clone(list)`)"
                .into(),
        );
    }
    let mut sorted: Vec<(Key, u32)> = Vec::with_capacity(n);
    for i in 1..=n as c_int {
        // SAFETY: pushes `list[i]`, checks it, and pops it.
        let hole = unsafe { ffi::lua_rawgeti_(state, list, i) } == ffi::LUA_TNIL;
        unsafe { ffi::lua_pop(state, 1) };
        if hole {
            return Err(format!("ui.sort: the list has a hole at {i}"));
        }
        // SAFETY: pushes `keys[i]`, reads it, and pops it. A string's bytes
        // stay valid after the pop: `keys` holds the string, and nothing
        // here runs the collector.
        let key = unsafe {
            let kind = ffi::lua_rawgeti_(state, keys, i);
            let key = match kind {
                ffi::LUA_TNIL => Key::Nil,
                ffi::LUA_TNUMBER => Key::Num(ffi::lua_tonumberx(state, -1, std::ptr::null_mut())),
                ffi::LUA_TBOOLEAN => Key::Bool(ffi::lua_toboolean(state, -1) != 0),
                ffi::LUA_TSTRING => {
                    let mut len = 0;
                    let data = ffi::lua_tolstring(state, -1, &mut len);
                    Key::Str(std::slice::from_raw_parts(data.cast(), len))
                }
                other => {
                    let name = std::ffi::CStr::from_ptr(ffi::lua_typename(state, other));
                    ffi::lua_pop(state, 1);
                    return Err(format!(
                        "ui.sort: a key is a number, string or boolean, not {}",
                        name.to_string_lossy()
                    ));
                }
            };
            ffi::lua_pop(state, 1);
            key
        };
        sorted.push((key, i as u32));
    }
    // Equal keys keep their order (the index breaks ties): a stable sort,
    // done unstably, which is faster. Numbers alone (the usual column)
    // sort as plain pairs, without the key's match.
    let from: Vec<u32> = if sorted.iter().all(|(k, _)| matches!(k, Key::Num(_))) {
        let num = |k: &Key| match k {
            Key::Num(n) => *n,
            _ => unreachable!(),
        };
        let mut pairs: Vec<(f64, u32)> = sorted.iter().map(|(k, i)| (num(k), *i)).collect();
        if descending {
            pairs.sort_unstable_by(|(x, a), (y, b)| y.total_cmp(x).then(a.cmp(b)));
        } else {
            pairs.sort_unstable_by(|(x, a), (y, b)| x.total_cmp(y).then(a.cmp(b)));
        }
        pairs.into_iter().map(|(_, i)| i).collect()
    } else {
        sorted.sort_unstable_by(|(x, a), (y, b)| {
            let by_key = match (x, y) {
                (Key::Nil, _) | (_, Key::Nil) => x.cmp(y),
                _ if descending => y.cmp(x),
                _ => x.cmp(y),
            };
            by_key.then(a.cmp(b))
        });
        sorted.into_iter().map(|(_, i)| i).collect()
    };
    // `list[to] = old list[from]`, a cycle at a time: one item held aside.
    let mut done = vec![false; n + 1];
    for start in 1..=n {
        if done[start] || from[start - 1] as usize == start {
            continue;
        }
        // SAFETY: pushes `list[start]`, then moves each item of the cycle
        // to its place (`lua_rawseti_` pops), and puts the held one last.
        // Every slot written holds a value: no new key, no allocation.
        unsafe {
            ffi::lua_rawgeti_(state, list, start as c_int);
            let mut to = start;
            loop {
                done[to] = true;
                let next = from[to - 1] as usize;
                if next == start {
                    ffi::lua_rawseti_(state, list, to as c_int);
                    break;
                }
                ffi::lua_rawgeti_(state, list, next as c_int);
                ffi::lua_rawseti_(state, list, to as c_int);
                to = next;
            }
        }
    }
    Ok(())
}

/// `ui.number(n, opts)` options, as `Intl.NumberFormat`'s.
#[derive(Default)]
struct NumberOptions {
    /// Digits after the point (default: none for whole numbers, else 2;
    /// compact numbers 1 under 100).
    decimals: Option<usize>,
    /// `"percent"`: `n` is a fraction (0.05 is 5%).
    percent: bool,
    /// `"compact"`: 1.2K, 3.4M, 5.6B, 7.8T.
    compact: bool,
    /// `"always"`: a plus sign on positive numbers; `"except_zero"`: on
    /// positive numbers but not zero.
    sign: Option<String>,
    /// Thousands separators (default true).
    grouping: bool,
    /// A currency symbol before the number (`"$"`), after its sign.
    currency: Option<String>,
}
impl NumberOptions {
    fn from(table: Option<Table>) -> mlua::Result<Self> {
        let mut o = NumberOptions {
            grouping: true,
            ..Default::default()
        };
        let Some(t) = table else {
            return Ok(o);
        };
        o.decimals = t
            .get::<Option<f64>>("decimals")?
            .map(|d| d.clamp(0., 20.) as usize);
        match t.get::<Option<String>>("style")?.as_deref() {
            None | Some("decimal") => {}
            Some("percent") => o.percent = true,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "ui.number: `style` is \"decimal\" or \"percent\", not {other:?}"
                )));
            }
        }
        match t.get::<Option<String>>("notation")?.as_deref() {
            None | Some("standard") => {}
            Some("compact") => o.compact = true,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "ui.number: `notation` is \"standard\" or \"compact\", not {other:?}"
                )));
            }
        }
        o.sign = t.get("sign")?;
        if let Some(sign) = &o.sign
            && !matches!(sign.as_str(), "auto" | "always" | "except_zero" | "never")
        {
            return Err(mlua::Error::runtime(format!(
                "ui.number: `sign` is \"auto\", \"always\", \"except_zero\" or \"never\", not {sign:?}"
            )));
        }
        o.grouping = t.get::<Option<bool>>("grouping")?.unwrap_or(true);
        o.currency = t.get("currency")?;
        Ok(o)
    }
}

/// `n` formatted as `o` says.
fn number(n: f64, o: &NumberOptions) -> String {
    if !n.is_finite() {
        return if n.is_nan() {
            "NaN".into()
        } else if n > 0. {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    let mut value = if o.percent { n * 100. } else { n };
    let mut suffix = "";
    if o.compact {
        for (scale, unit) in [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")] {
            if value.abs() >= scale {
                value /= scale;
                suffix = unit;
                break;
            }
        }
    }
    let decimals = o.decimals.unwrap_or(if o.compact {
        usize::from(value.abs() < 100. && !suffix.is_empty())
    } else if value.fract() == 0. {
        0
    } else {
        2
    });
    let mut digits = format!("{:.*}", decimals, value.abs());
    // Compact numbers drop a zero fraction (1.0K is 1K), as Intl does.
    if o.compact && o.decimals.is_none() && digits.ends_with(".0") {
        digits.truncate(digits.len() - 2);
    }
    let zero = digits.bytes().all(|b| b == b'0' || b == b'.');
    let negative = value < 0. && !zero;
    let (whole, fraction) = match digits.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (digits.as_str(), None),
    };
    let mut out = String::with_capacity(digits.len() + 8);
    let sign = match o.sign.as_deref() {
        Some("never") => "",
        _ if negative => "-",
        Some("always") => "+",
        Some("except_zero") if !zero => "+",
        _ => "",
    };
    out.push_str(sign);
    if let Some(currency) = &o.currency {
        out.push_str(currency);
    }
    if o.grouping {
        let first = whole.len() % 3;
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (i + 3 - first) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
    } else {
        out.push_str(whole);
    }
    if let Some(fraction) = fraction {
        out.push('.');
        out.push_str(fraction);
    }
    out.push_str(suffix);
    if o.percent {
        out.push('%');
    }
    out
}

/// `ui.json.encode(value, { pretty })`: `value` as JSON, as state holds it
/// (lists as arrays, tables as objects, `ui.null` as null).
fn encode(_: &Lua, (value, options): (mlua::Value, Option<Table>)) -> mlua::Result<String> {
    let value = crate::value::convert(value, None)
        .map_err(|e| mlua::Error::runtime(format!("ui.json.encode: {e}")))?;
    let text = value.to_json();
    let pretty = match &options {
        Some(o) => o.get::<Option<bool>>("pretty")?.unwrap_or(false),
        None => false,
    };
    if !pretty {
        return Ok(text);
    }
    serde_json::from_str::<serde_json::Value>(&text)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .map_err(|e| mlua::Error::runtime(format!("ui.json.encode: {e}")))
}
/// `ui.json.decode(text)`: the data JSON `text` holds, as tables the script
/// may change; an error says where JSON that isn't fails.
fn decode(lua: &Lua, text: mlua::LuaString) -> mlua::Result<mlua::Value> {
    let parsed = serde_json::from_slice::<serde_json::Value>(&text.as_bytes())
        .map_err(|e| mlua::Error::runtime(format!("ui.json.decode: {e}")))?;
    crate::value::to_lua(&crate::node::from_json(parsed), lua, false)
}

/// `ui.toast { title, body?, kind?, duration?, action = { label, run }? }`:
/// shows a toast; its id dismisses it (`ui.dismiss_toast`).
fn toast(lua: &Lua, table: Table) -> mlua::Result<u64> {
    use zgui::compose::{Toast, ToastKind, Toaster};
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

pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set("toast", lua.create_function(toast)?)?;
    ui.set(
        "dismiss_toast",
        lua.create_function(|lua, id: u64| {
            let runtime = crate::node::env(lua)?.runtime.clone();
            zgui::compose::Toaster::of(&runtime).dismiss(id);
            Ok(())
        })?,
    )?;
    let json = lua.create_table()?;
    json.set("encode", lua.create_function(encode)?)?;
    json.set("decode", lua.create_function(decode)?)?;
    json.set_readonly(true);
    ui.set("json", json)?;
    let sort_by_keys =
        lua.create_function(|lua, (list, keys, descending): (Table, Table, bool)| {
            let mut result = Ok(());
            // SAFETY: the stack holds `list` and `keys` at 1 and 2 (`exec_raw`
            // pushes them), as `sort_by_keys` needs; it raises no error.
            unsafe {
                lua.exec_raw::<()>((list, keys), |state| {
                    result = sort_by_keys(state, descending);
                })?;
            }
            result.map_err(mlua::Error::runtime)
        })?;
    let sort: mlua::Function = lua.load(SORT).set_name("=ui.sort").call(sort_by_keys)?;
    ui.set("sort", sort)?;
    ui.set(
        "number",
        lua.create_function(|_, (n, options): (f64, Option<Table>)| {
            Ok(number(n, &NumberOptions::from(options)?))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(n: f64, f: impl FnOnce(&mut NumberOptions)) -> String {
        let mut o = NumberOptions {
            grouping: true,
            ..Default::default()
        };
        f(&mut o);
        number(n, &o)
    }

    #[test]
    fn numbers_format_as_intl_would() {
        assert_eq!(fmt(1234567., |_| {}), "1,234,567");
        assert_eq!(fmt(-1234.5, |_| {}), "-1,234.50");
        assert_eq!(fmt(999., |_| {}), "999");
        assert_eq!(fmt(1234.5678, |o| o.decimals = Some(1)), "1,234.6");
        assert_eq!(fmt(0.0123, |o| o.percent = true), "1.23%");
        assert_eq!(
            fmt(0.0123, |o| {
                o.percent = true;
                o.sign = Some("always".into())
            }),
            "+1.23%"
        );
        assert_eq!(fmt(0., |o| o.sign = Some("except_zero".into())), "0");
        assert_eq!(fmt(-0.001, |o| o.decimals = Some(2)), "0.00");
        assert_eq!(fmt(1500., |o| o.compact = true), "1.5K");
        assert_eq!(fmt(1000., |o| o.compact = true), "1K");
        assert_eq!(fmt(123_456., |o| o.compact = true), "123K");
        assert_eq!(fmt(-2_500_000_000., |o| o.compact = true), "-2.5B");
        assert_eq!(fmt(950., |o| o.compact = true), "950");
        assert_eq!(fmt(1234.5, |o| o.grouping = false), "1234.50");
        assert_eq!(
            fmt(-1234.5, |o| o.currency = Some("$".into())),
            "-$1,234.50"
        );
    }
}
