//! `ui.sorted(list, spec)`: a list state's items in order, kept in order as
//! the list changes. A computed that re-sorts a whole list (`ui.sort` of a
//! copy) does all its work each time one item changes; this re-places only
//! the items that changed, as their writes leave them (an item a write did
//! not touch is the same value, found at a pointer's glance). `spec` is
//! `{ by, descending, filter }`, or a function returning one (tracked: a
//! new spec sorts it all again). `by` is a field's name, read in Rust, or a
//! function of an item; `filter` keeps the items a function is true for,
//! or whose fields pass a table's tests, read in Rust (`{ sector = "Tech",
//! symbol = { contains = "AB" } }`). Functions are called only for the
//! items that changed, untracked.
use crate::{
    Vm,
    value::{State, Value},
};
use mlua::{Function, Lua, Table};
use std::{cmp::Ordering, rc::Rc};

/// A sort key: numbers before strings before booleans (the other way round
/// descending); nil (no key) last, whichever way it sorts (as `ui.sort`'s).
#[derive(Clone, Debug)]
enum Key {
    Num(f64),
    Str(Rc<str>),
    Bool(bool),
    Nil,
}
impl Key {
    fn rank(&self) -> u8 {
        match self {
            Key::Num(_) => 0,
            Key::Str(_) => 1,
            Key::Bool(_) => 2,
            Key::Nil => 3,
        }
    }
    fn cmp(&self, other: &Key, descending: bool) -> Ordering {
        let order = match (self, other) {
            // Nil last, whichever way.
            (Key::Nil, _) | (_, Key::Nil) => return self.rank().cmp(&other.rank()),
            (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
            (Key::Str(a), Key::Str(b)) => a.cmp(b),
            (Key::Bool(a), Key::Bool(b)) => a.cmp(b),
            // Kinds apart: reversed too, as `ui.sort`'s.
            _ => self.rank().cmp(&other.rank()),
        };
        if descending { order.reverse() } else { order }
    }
    fn of_value(value: &Value) -> Key {
        match value {
            Value::Num(n) => Key::Num(*n),
            Value::Str(s) => Key::Str(s.clone()),
            Value::Text(t) => Key::Str(t.to_cow().into()),
            Value::Bool(b) => Key::Bool(*b),
            _ => Key::Nil,
        }
    }
    fn of_lua(value: &mlua::Value) -> Key {
        match value {
            mlua::Value::Integer(n) => Key::Num(*n as f64),
            mlua::Value::Number(n) => Key::Num(*n),
            mlua::Value::String(s) => Key::Str(s.to_string_lossy().into()),
            mlua::Value::Boolean(b) => Key::Bool(*b),
            _ => Key::Nil,
        }
    }
}

/// A whole list judged by script in one call (not one call an item, each
/// paying a protected call and a budget): whether `filter` keeps each item,
/// and `by`'s key of each kept one.
const JUDGE_ALL: &str = r#"
return function(list, filter, by)
	local n = #list
	local keep = if filter then table.create(n, false) else nil
	local keys = if by then table.create(n) else nil
	for i = 1, n do
		local item = list[i]
		local kept = true
		if filter then
			kept = filter(item) and true or false
			keep[i] = kept
		end
		if by and kept then
			keys[i] = by(item)
		end
	end
	return keep, keys
end
"#;

/// What an item is sorted by.
#[derive(Clone)]
enum By {
    /// The item itself (a list of numbers or strings).
    Item,
    /// A field of it, read in Rust.
    Field(Rc<str>),
    /// What a function of it returns.
    Function(Function),
}
/// What a table `filter` asks of a field: to equal a value, or (a string)
/// to hold a text, `case = false` folding ASCII letters' case.
#[derive(Clone, PartialEq)]
enum Test {
    Equals(Value),
    Contains { text: Rc<str>, case: bool },
}
impl Test {
    fn passes(&self, value: Option<&Value>) -> bool {
        match self {
            Test::Equals(want) => value == Some(want),
            Test::Contains { text, case } => {
                let held = match value {
                    Some(Value::Str(s)) => std::borrow::Cow::Borrowed(&**s),
                    Some(Value::Text(t)) => t.to_cow(),
                    _ => return false,
                };
                if *case {
                    held.contains(&**text)
                } else {
                    let (held, text) = (held.as_bytes(), text.as_bytes());
                    text.is_empty()
                        || held
                            .windows(text.len())
                            .any(|w| w.eq_ignore_ascii_case(text))
                }
            }
        }
    }
}
/// Which items a spec keeps.
#[derive(Clone)]
enum Filter {
    /// Those a function of the item is true for.
    Function(Function),
    /// Those whose fields pass their tests (`{ sector = "Tech", symbol =
    /// { contains = "AB" } }`), read in Rust: no script call an item. By
    /// field name.
    Fields(Rc<[(Rc<str>, Test)]>),
}
impl Filter {
    fn from_lua(lua: &Lua, value: mlua::Value) -> mlua::Result<Option<Filter>> {
        Ok(match value {
            mlua::Value::Nil => None,
            mlua::Value::Function(f) => Some(Filter::Function(f)),
            mlua::Value::Table(fields) => {
                let mut tests = Vec::new();
                for pair in fields.pairs::<String, mlua::Value>() {
                    let (name, value) = pair?;
                    let test = match value {
                        mlua::Value::Table(t) if t.contains_key("contains")? => {
                            let case = t.get::<Option<bool>>("case")?.unwrap_or(true);
                            let text: String = t.get("contains")?;
                            let text = if case {
                                text
                            } else {
                                text.to_ascii_lowercase()
                            };
                            Test::Contains {
                                text: text.into(),
                                case,
                            }
                        }
                        value => Test::Equals(<Value as mlua::FromLua>::from_lua(value, lua)?),
                    };
                    tests.push((Rc::<str>::from(name), test));
                }
                tests.sort_by(|(a, _), (b, _)| a.cmp(b));
                Some(Filter::Fields(tests.into()))
            }
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ui.sorted's `filter` is a function or a table of fields' tests, not {}",
                    other.type_name()
                )));
            }
        })
    }
    fn same(&self, other: &Filter) -> bool {
        match (self, other) {
            (Filter::Function(a), Filter::Function(b)) => a.to_pointer() == b.to_pointer(),
            (Filter::Fields(a), Filter::Fields(b)) => a == b,
            _ => false,
        }
    }
    /// Whether `item` passes the fields' tests (`hints`: where each field
    /// was in the item before).
    fn fields_pass(tests: &[(Rc<str>, Test)], item: &Value, hints: &mut [usize]) -> bool {
        tests
            .iter()
            .zip(hints.iter_mut())
            .all(|((name, test), hint)| test.passes(Kept::field(item, name, hint)))
    }
}
#[derive(Clone)]
struct Spec {
    by: By,
    descending: bool,
    filter: Option<Filter>,
}
impl Spec {
    fn from_table(lua: &Lua, table: &Table) -> mlua::Result<Spec> {
        let by = match table.get::<mlua::Value>("by")? {
            mlua::Value::Nil => By::Item,
            mlua::Value::String(s) => By::Field(s.to_str()?.as_ref().into()),
            mlua::Value::Function(f) => By::Function(f),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ui.sorted's `by` is a field's name or a function, not {}",
                    other.type_name()
                )));
            }
        };
        Ok(Spec {
            by,
            descending: table.get::<Option<bool>>("descending")?.unwrap_or(false),
            filter: Filter::from_lua(lua, table.get("filter")?)?,
        })
    }
    /// The same order: the same field or the same function, way and filter.
    fn same(&self, other: &Spec) -> bool {
        let by = match (&self.by, &other.by) {
            (By::Item, By::Item) => true,
            (By::Field(a), By::Field(b)) => a == b,
            (By::Function(a), By::Function(b)) => a.to_pointer() == b.to_pointer(),
            _ => false,
        };
        let filter = match (&self.filter, &other.filter) {
            (None, None) => true,
            (Some(a), Some(b)) => a.same(b),
            _ => false,
        };
        by && filter && self.descending == other.descending
    }
}

/// The view's work kept between runs: the items it last saw, each item's
/// key and whether the filter keeps it, and the kept items' order.
#[derive(Default)]
struct Kept {
    /// `JUDGE_ALL`, made when first needed.
    judge_all: Option<Function>,
    spec: Option<Spec>,
    /// The items it last saw, each held (not the list holding them): the
    /// list stays its state's alone, so a write to one row is made in place
    /// rather than copying all of them, and an item written compares as
    /// another (the one held keeps its place in memory).
    items: Option<Vec<Value>>,
    /// The list's version (`Signal::version`) when they were taken: the
    /// same, nothing was written since (only the spec changed).
    version: u64,
    keys: Vec<Key>,
    kept: Vec<bool>,
    order: Vec<u32>,
}

/// Whether a write left item `a` as it was: the same table (unchanged rows
/// share it), or an equal plain value.
fn same_item(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Map(a), Value::Map(b)) => Rc::ptr_eq(a, b),
        (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
        (a, b) => a == b,
    }
}

impl Kept {
    /// Item `i`'s key and whether it is kept, by `spec` (scripts called
    /// untracked, given the item's table as readers get it: kept for the
    /// item while it lives, not made again for each call).
    fn judge(vm: &Vm, spec: &Spec, item: &Value) -> mlua::Result<(Key, bool)> {
        let kept = match &spec.filter {
            None => true,
            Some(Filter::Fields(tests)) => {
                // Where its fields are, from the start (a few on the stack).
                let (mut stack, mut heap) = ([0; 8], Vec::new());
                let hints = if tests.len() <= stack.len() {
                    &mut stack[..tests.len()]
                } else {
                    heap.resize(tests.len(), 0);
                    &mut heap[..]
                };
                Filter::fields_pass(tests, item, hints)
            }
            Some(Filter::Function(filter)) => {
                let item = crate::value::shared_lua(item, &vm.lua)?;
                !matches!(
                    vm.call::<mlua::Value>(filter, item)?,
                    mlua::Value::Nil | mlua::Value::Boolean(false)
                )
            }
        };
        if !kept {
            return Ok((Key::Nil, false));
        }
        let key = match &spec.by {
            By::Item => Key::of_value(item),
            By::Field(name) => item.field(name).map_or(Key::Nil, Key::of_value),
            By::Function(f) => {
                let item = crate::value::shared_lua(item, &vm.lua)?;
                Key::of_lua(&vm.call::<mlua::Value>(f, item)?)
            }
        };
        Ok((key, true))
    }
    fn before(&self, descending: bool, a: u32, b: u32) -> Ordering {
        self.keys[a as usize]
            .cmp(&self.keys[b as usize], descending)
            .then(a.cmp(&b))
    }
    /// Everything judged and sorted again.
    fn rebuild(&mut self, vm: &Vm, spec: &Spec, items: &Rc<[Value]>) -> mlua::Result<()> {
        self.keys.clear();
        self.kept.clear();
        let by_script = matches!(spec.by, By::Function(_));
        let tests = match &spec.filter {
            Some(Filter::Fields(tests)) => Some(tests.clone()),
            _ => None,
        };
        if matches!(spec.filter, Some(Filter::Function(_))) || by_script {
            self.judge_all(vm, spec, items)?;
        } else if let (By::Field(name), Some(tests)) = (&spec.by, &tests) {
            let (mut hint, mut hints) = (0, vec![0; tests.len()]);
            for item in items.iter() {
                let kept = Filter::fields_pass(tests, item, &mut hints);
                let key = kept.then(|| Self::field(item, name, &mut hint)).flatten();
                self.keys.push(key.map_or(Key::Nil, Key::of_value));
                self.kept.push(kept);
            }
        } else if let By::Field(name) = &spec.by {
            let mut hint = 0;
            for item in items.iter() {
                let key = Self::field(item, name, &mut hint);
                self.keys.push(key.map_or(Key::Nil, Key::of_value));
                self.kept.push(true);
            }
        } else {
            for item in items.iter() {
                let (key, kept) = Self::judge(vm, spec, item)?;
                self.keys.push(key);
                self.kept.push(kept);
            }
        }
        let order: Vec<u32> = (0..items.len() as u32)
            .filter(|i| self.kept[*i as usize])
            .collect();
        self.order = self.sorted(spec.descending, order);
        Ok(())
    }
    /// `item`'s field `name`, looked for first where the item before had it
    /// (items of one shape keep their fields in one order), else searched.
    fn field<'a>(item: &'a Value, name: &str, hint: &mut usize) -> Option<&'a Value> {
        let Value::Map(fields) = item else {
            return None;
        };
        if let Some((key, value)) = fields.get(*hint)
            && **key == *name
        {
            return Some(value);
        }
        let at = fields.binary_search_by(|(key, _)| (**key).cmp(name)).ok()?;
        *hint = at;
        Some(&fields[at].1)
    }
    /// `order` sorted by `before`. Where every key is a number (or nil), or
    /// every key a string (or nil), it radix-sorts words beside their items
    /// (no comparisons, the order's own ties kept): a number as the bits that
    /// order as `total_cmp` does, a string by its first 8 bytes, the runs of
    /// those that share them and run longer compared whole. Nil goes last
    /// either way, and ties by index, as `before`'s.
    fn sorted(&self, descending: bool, order: Vec<u32>) -> Vec<u32> {
        let key = |i: &u32| &self.keys[*i as usize];
        let numbers = order
            .iter()
            .all(|i| matches!(key(i), Key::Num(_) | Key::Nil));
        let strings = !numbers
            && order
                .iter()
                .all(|i| matches!(key(i), Key::Str(_) | Key::Nil));
        if !numbers && !strings {
            let mut order = order;
            order.sort_unstable_by(|a, b| self.before(descending, *a, *b));
            return order;
        }
        let text = |i: u32| match &self.keys[i as usize] {
            Key::Str(s) => s.as_bytes(),
            _ => &[][..],
        };
        // Nil last, in its order; the rest as words.
        let (mut words, nils): (Vec<(u64, u32)>, Vec<u32>) = {
            let mut words = Vec::with_capacity(order.len());
            let mut nils = Vec::new();
            for &i in &order {
                let word = match key(&i) {
                    Key::Num(n) => {
                        // `f64::total_cmp`'s order, as unsigned bits.
                        let bits = n.to_bits() as i64;
                        (bits ^ (((bits >> 63) as u64) >> 1) as i64) as u64 ^ (1 << 63)
                    }
                    Key::Str(_) => {
                        let (mut head, bytes) = ([0; 8], text(i));
                        let n = bytes.len().min(8);
                        head[..n].copy_from_slice(&bytes[..n]);
                        u64::from_be_bytes(head)
                    }
                    _ => {
                        nils.push(i);
                        continue;
                    }
                };
                words.push((if descending { !word } else { word }, i));
            }
            (words, nils)
        };
        radix_sort(&mut words);
        if strings {
            // Strings sharing their first 8 bytes and differing (past them,
            // or by a zero byte): whole.
            let mut start = 0;
            while start < words.len() {
                let mut end = start + 1;
                while end < words.len() && words[end].0 == words[start].0 {
                    end += 1;
                }
                let first = text(words[start].1);
                if end - start > 1 && words[start + 1..end].iter().any(|(_, i)| text(*i) != first) {
                    words[start..end].sort_by(|(_, i), (_, j)| {
                        let order = text(*i).cmp(text(*j));
                        let order = if descending { order.reverse() } else { order };
                        order.then(i.cmp(j))
                    });
                }
                start = end;
            }
        }
        words.into_iter().map(|(_, i)| i).chain(nils).collect()
    }

    /// Every item judged by script in one call (`JUDGE_ALL`), what it
    /// returns read through Luau's C API; field keys read in Rust.
    fn judge_all(&mut self, vm: &Vm, spec: &Spec, items: &Rc<[Value]>) -> mlua::Result<()> {
        let judge = match &self.judge_all {
            Some(f) => f.clone(),
            None => {
                let f: Function = vm.lua.load(JUDGE_ALL).set_name("=ui.sorted").eval()?;
                self.judge_all = Some(f.clone());
                f
            }
        };
        let list = crate::value::shared_lua(&Value::List(items.clone()), &vm.lua)?;
        let by = match &spec.by {
            By::Function(f) => Some(f.clone()),
            _ => None,
        };
        let filter = match &spec.filter {
            Some(Filter::Function(f)) => Some(f.clone()),
            _ => None,
        };
        let (keep, keys): (mlua::Value, mlua::Value) =
            vm.call(&judge, (list, filter, by.clone()))?;
        let n = items.len();
        let mut kept = vec![true; n];
        let mut script_keys: Vec<Key> = Vec::new();
        // SAFETY: `exec_raw` runs the closure as a C function inside a
        // protected call, `keep` and `keys` (each a table or nil) at 1 and
        // 2. Only raw reads of their array slots and type checks are made,
        // each pushed value popped at once (a string copied out first):
        // nothing allocates in Luau or runs a script.
        unsafe {
            vm.lua.exec_raw::<()>((keep, keys), |state| {
                use mlua::ffi;
                let has_keep = ffi::lua_type(state, 1) == ffi::LUA_TTABLE;
                let has_keys = ffi::lua_type(state, 2) == ffi::LUA_TTABLE;
                if has_keys {
                    script_keys.reserve(n);
                }
                for i in 1..=n {
                    if has_keep {
                        ffi::lua_rawgeti_(state, 1, i as std::ffi::c_int);
                        kept[i - 1] = ffi::lua_toboolean(state, -1) != 0;
                        ffi::lua_pop(state, 1);
                    }
                    if has_keys {
                        let key = match ffi::lua_rawgeti_(state, 2, i as std::ffi::c_int) {
                            ffi::LUA_TNUMBER => {
                                Key::Num(ffi::lua_tonumberx(state, -1, std::ptr::null_mut()))
                            }
                            ffi::LUA_TBOOLEAN => Key::Bool(ffi::lua_toboolean(state, -1) != 0),
                            ffi::LUA_TSTRING => {
                                let mut len = 0;
                                let data = ffi::lua_tolstring(state, -1, &mut len);
                                let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
                                Key::Str(String::from_utf8_lossy(bytes).into())
                            }
                            _ => Key::Nil,
                        };
                        ffi::lua_pop(state, 1);
                        script_keys.push(key);
                    }
                }
            })?;
        }
        // Fields' tests, in Rust, beside the function's keys.
        if let Some(Filter::Fields(tests)) = &spec.filter {
            let mut hints = vec![0; tests.len()];
            for (i, item) in items.iter().enumerate() {
                kept[i] = kept[i] && Filter::fields_pass(tests, item, &mut hints);
            }
        }
        let mut hint = 0;
        for (i, item) in items.iter().enumerate() {
            self.kept.push(kept[i]);
            self.keys.push(if !kept[i] {
                Key::Nil
            } else {
                match &spec.by {
                    By::Item => Key::of_value(item),
                    By::Field(name) => {
                        Self::field(item, name, &mut hint).map_or(Key::Nil, Key::of_value)
                    }
                    By::Function(_) => script_keys.get(i).cloned().unwrap_or(Key::Nil),
                }
            });
        }
        Ok(())
    }
    /// Items `changed` judged again and put back in their places: taken out
    /// in one pass, sorted among themselves, merged back in another.
    // Kept out of the view's effect: inlined there it changes how the
    // whole sort (the same effect's other path) is compiled.
    #[inline(never)]
    fn update(
        &mut self,
        vm: &Vm,
        spec: &Spec,
        items: &Rc<[Value]>,
        changed: &[u32],
    ) -> mlua::Result<()> {
        let mut moved = vec![false; items.len()];
        for &i in changed {
            moved[i as usize] = true;
        }
        // Appended items are judged as changed ones.
        self.keys.resize(items.len(), Key::Nil);
        self.kept.resize(items.len(), false);
        for &i in changed {
            let (key, kept) = Self::judge(vm, spec, &items[i as usize])?;
            self.keys[i as usize] = key;
            self.kept[i as usize] = kept;
        }
        self.order.retain(|i| !moved[*i as usize]);
        let mut placed: Vec<u32> = changed
            .iter()
            .copied()
            .filter(|i| self.kept[*i as usize])
            .collect();
        placed.sort_unstable_by(|a, b| self.before(spec.descending, *a, *b));
        // Each placed one's place among the rest, found by halving (a few
        // hundred rows placed in 100,000 compare a few thousand times, not
        // 100,000), then the runs between them copied whole.
        let mut merged = Vec::with_capacity(self.order.len() + placed.len());
        let mut from = 0;
        for &item in &placed {
            let rest = &self.order[from..];
            let at = from
                + rest.partition_point(|&other| {
                    self.before(spec.descending, other, item) == Ordering::Less
                });
            merged.extend_from_slice(&self.order[from..at]);
            merged.push(item);
            from = at;
        }
        merged.extend_from_slice(&self.order[from..]);
        self.order = merged;
        Ok(())
    }
}

/// `words` sorted by their first, stably (equal ones keep their order): a
/// pass a byte, least significant first, skipping a byte every word shares.
fn radix_sort(words: &mut Vec<(u64, u32)>) {
    if words.len() < 64 {
        words.sort_by_key(|(word, _)| *word);
        return;
    }
    let mut scratch = vec![(0, 0); words.len()];
    for byte in 0..8 {
        let shift = byte * 8;
        let mut counts = [0usize; 256];
        for (word, _) in words.iter() {
            counts[(word >> shift) as usize & 0xff] += 1;
        }
        if counts.contains(&words.len()) {
            continue;
        }
        let mut at = 0;
        for count in counts.iter_mut() {
            (*count, at) = (at, at + *count);
        }
        for &(word, i) in words.iter() {
            let bucket = &mut counts[(word >> shift) as usize & 0xff];
            scratch[*bucket] = (word, i);
            *bucket += 1;
        }
        std::mem::swap(words, &mut scratch);
    }
}

/// What the view should do with `items`, given what it last saw: nothing,
/// re-place `changed`, or sort it all (another length that isn't items
/// added at the end, or a first look).
enum Change {
    None,
    Some(Vec<u32>),
    All,
}
fn change(was: Option<&Vec<Value>>, now: &[Value]) -> Change {
    let Some(was) = was else {
        return Change::All;
    };
    if now.len() < was.len() {
        return Change::All;
    }
    let mut changed: Vec<u32> = (0..was.len())
        .filter(|&i| !same_item(&was[i], &now[i]))
        .map(|i| i as u32)
        .collect();
    changed.extend(was.len() as u32..now.len() as u32);
    if changed.is_empty() {
        return Change::None;
    }
    // Most of it: sorting it all is as quick.
    if changed.len() > now.len() / 4 {
        return Change::All;
    }
    Change::Some(changed)
}

/// `ui.sorted(list, spec)`.
pub(crate) fn sorted(lua: &Lua, (list, spec): (mlua::Value, mlua::Value)) -> mlua::Result<State> {
    let source = crate::node::expect_state(&list, "ui.sorted's list")?;
    let spec = match spec {
        mlua::Value::Table(t) => Ok(Spec::from_table(lua, &t)?),
        mlua::Value::Function(f) => Err(f),
        other => {
            return Err(mlua::Error::runtime(format!(
                "ui.sorted takes a spec, `{{ by, descending, filter }}`, or a function of one, not {}",
                other.type_name()
            )));
        }
    };
    let env = crate::node::env(lua)?;
    let out = env.runtime.signal(Value::List(Rc::from([])));
    // A spec function runs in an effect of its own: only when what it
    // reads changes (a new spec sorts it all again), not on every write.
    let current: Rc<std::cell::RefCell<Option<Spec>>> = Rc::default();
    let version = env.runtime.signal(0_u64);
    let spec_effect = match spec {
        Ok(spec) => {
            *current.borrow_mut() = Some(spec);
            None
        }
        Err(f) => {
            let (vm, current, version) = (env.vm.clone(), current.clone(), version.clone());
            Some(env.runtime.effect_now(move || {
                let Some(vm) = vm.upgrade() else {
                    return;
                };
                match vm
                    .call::<Table>(&f, ())
                    .and_then(|t| Spec::from_table(&vm.lua, &t))
                {
                    Ok(spec) => {
                        *current.borrow_mut() = Some(spec);
                        version.update(|v| *v += 1);
                    }
                    Err(e) => vm.report(e),
                }
            }))
        }
    };
    let (vm, signal, runtime) = (env.vm.clone(), out.clone(), env.runtime.clone());
    let mut kept = Kept::default();
    let effect = env.runtime.effect_now(move || {
        let Some(vm) = vm.upgrade() else {
            return;
        };
        // The spec, then the list.
        version.with(|_| ());
        let Some(spec) = current.borrow().clone() else {
            return;
        };
        let items: Rc<[Value]> = match source.with(Clone::clone) {
            Value::List(items) => items,
            Value::Nil => Rc::from([]),
            other => {
                return vm.report(mlua::Error::runtime(format!(
                    "ui.sorted sorts a list, not {}",
                    other.kind()
                )));
            }
        };
        let same_spec = kept.spec.as_ref().is_some_and(|s| s.same(&spec));
        // What to take again of the items held: none, those written, or all.
        let version = source.version();
        let written = kept.items.is_none() || kept.version != version;
        kept.version = version;
        let change = if written {
            change(kept.items.as_ref(), &items)
        } else {
            Change::None
        };
        let result = runtime.untracked(|| match change {
            Change::None if same_spec => Ok(None),
            Change::Some(changed) if same_spec => kept
                .update(&vm, &spec, &items, &changed)
                .map(|_| Some((Some(changed), false))),
            // A new spec over the same items (a sort, a filter): held as they are.
            Change::None => kept
                .rebuild(&vm, &spec, &items)
                .map(|_| Some((Some(Vec::new()), true))),
            _ => kept.rebuild(&vm, &spec, &items).map(|_| Some((None, true))),
        });
        match result {
            Ok(None) => {}
            Ok(Some((changed, sorted_again))) => {
                kept.spec = Some(spec);
                // The items held: the written ones taken again, or all.
                match (&changed, kept.items.as_mut()) {
                    (Some(changed), Some(held)) => {
                        held.resize(items.len(), Value::Nil);
                        for &i in changed {
                            held[i as usize] = items[i as usize].clone();
                        }
                    }
                    _ => kept.items = Some(items.to_vec()),
                }
                // The view written where it changed, in place while nothing
                // else holds it: a few rows moving in 100,000 write those.
                // Sorted again, most of it moves: made anew.
                let order = &kept.order;
                signal.mutate(|view| {
                    if !sorted_again
                        && let Value::List(list) = view
                        && list.len() == order.len()
                        && let Some(slots) = Rc::get_mut(list)
                    {
                        let mut any = false;
                        for (slot, &i) in slots.iter_mut().zip(order) {
                            let item = &items[i as usize];
                            if !same_item(slot, item) {
                                *slot = item.clone();
                                any = true;
                            }
                        }
                        return any;
                    }
                    *view = Value::List(order.iter().map(|i| items[*i as usize].clone()).collect());
                    true
                });
            }
            Err(e) => {
                // Judged again in full next time.
                kept.spec = None;
                vm.report(e);
            }
        }
    });
    let fresh = match &spec_effect {
        Some(spec) => crate::value::Fresh::Two(spec.refresher(), effect.refresher()),
        None => crate::value::Fresh::One(effect.refresher()),
    };
    if let Some(spec_effect) = spec_effect {
        crate::node::retain(&env, Box::new(spec_effect));
    }
    crate::node::retain(&env, Box::new(effect));
    Ok(State(out, Some(fresh)))
}
