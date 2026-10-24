//! Lowers a script's node tree to retained zgui views. Literal styles become
//! ordinary fluent styles; state, computed and animated properties share one
//! reactive style per node; text templates interpolate state in Rust.
//! Targetable nodes (`id`, `class`, `ref`, `on_mount`) also carry an element:
//! what animations write, resolved over the node's own styles.
use crate::{
    Actions, Vm, anim,
    anime::{Key as Property, target::Target, value::V},
    element::{Element, ElementUd},
    node::{
        Action, Handler, Kind, LayoutAnim, Lit, Node, NodeUd, Prop, Pseudo, Scope, Segment, Source,
        Span, Text, Ty, lit,
    },
    value::Value,
};
use mlua::FromLua;
use std::{cell::OnceCell, collections::HashMap, rc::Rc, sync::Arc};
use zgui::{
    compose::{Context, NodeHandle, TextSpan, View, prelude::*},
    image::ImageData,
    input::{EventPhase, InputEvent, Key, PointerButton},
    layout::{ContentAlign, Length},
    reactive::Signal,
    scene::{Align, Color},
    svg::SvgData,
};

/// Decoded images and SVGs by source, kept across reloads.
#[derive(Default)]
pub(crate) struct Assets {
    images: std::cell::RefCell<HashMap<String, Arc<ImageData>>>,
    svgs: std::cell::RefCell<HashMap<String, Arc<SvgData>>>,
    /// Each file's path and modification time when it was read, so a
    /// watching host sees an edited image (see `changed`).
    files: std::cell::RefCell<HashMap<String, (std::path::PathBuf, Option<std::time::SystemTime>)>>,
    /// Where they are read from: the disk, or the app's bundle.
    source: std::cell::Cell<Option<crate::Files>>,
    /// Remote sources (`https://…`) by URL: fetched once, whoever shows
    /// them; what shows one follows its status (`LOADING`, `READY`, `FAILED`).
    remote: std::cell::RefCell<HashMap<String, zgui::reactive::Signal<u8>>>,
    /// Failed remote sources: when, and how many times in a row. Asked
    /// for again after a backoff, they're fetched again (as a browser
    /// retries a broken image when a new element shows it).
    failed: std::cell::RefCell<HashMap<String, (std::time::Instant, u32)>>,
    /// The host's tasks and transport, which remote sources go through.
    links: std::cell::RefCell<Option<(Rc<crate::timers::Timers>, crate::calls::SharedFetcher)>>,
    /// Images the host made (`ScriptHost::image`), as `host:<name>`
    /// sources: what shows one follows its version.
    hosted: std::cell::RefCell<HashMap<String, zgui::reactive::Signal<u64>>>,
}
impl Assets {
    /// `name`'s version signal, made with `runtime` the first time.
    fn hosted(&self, src: &str, runtime: &zgui::reactive::Runtime) -> zgui::reactive::Signal<u64> {
        self.hosted
            .borrow_mut()
            .entry(src.to_owned())
            .or_insert_with(|| runtime.signal(0))
            .clone()
    }
    /// The host sets (or, with None, clears) image `host:<name>`.
    pub(crate) fn set_hosted(
        &self,
        name: &str,
        image: Option<Arc<ImageData>>,
        runtime: &zgui::reactive::Runtime,
    ) {
        let src = format!("host:{name}");
        match image {
            Some(image) => self.images.borrow_mut().insert(src.clone(), image),
            None => self.images.borrow_mut().remove(&src),
        };
        let version = self.hosted(&src, runtime);
        version.update(|v| *v = v.wrapping_add(1));
    }
    pub(crate) fn connect(
        &self,
        timers: Rc<crate::timers::Timers>,
        fetcher: crate::calls::SharedFetcher,
    ) {
        *self.links.borrow_mut() = Some((timers, fetcher));
    }
    pub(crate) fn set_files(&self, files: crate::Files) {
        self.source.set(Some(files));
    }
    fn read(&self, src: &str, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let bytes = match self.source.get() {
            Some(files) => files.read(path)?,
            None => std::fs::read(path)?,
        };
        self.files
            .borrow_mut()
            .insert(src.to_owned(), (path.to_path_buf(), modified));
        Ok(bytes)
    }
    /// The files read.
    pub fn files(&self) -> Vec<std::path::PathBuf> {
        self.files
            .borrow()
            .values()
            .map(|(path, _)| path.clone())
            .collect()
    }
    /// Forgets the files edited since they were read, so the next build
    /// reads them again; whether there were any. Only stats files.
    pub fn changed(&self) -> bool {
        let stale: Vec<String> = self
            .files
            .borrow()
            .iter()
            .filter(|(_, (path, modified))| {
                std::fs::metadata(path).and_then(|m| m.modified()).ok() != *modified
            })
            .map(|(src, _)| src.clone())
            .collect();
        for src in &stale {
            self.files.borrow_mut().remove(src);
            self.images.borrow_mut().remove(src);
            self.svgs.borrow_mut().remove(src);
        }
        !stale.is_empty()
    }
}

pub(crate) struct Lowering {
    /// What each slot shows: its contributions and who made them.
    pub slots: crate::node::Slots,
    pub actions: Actions,
    pub elements: Rc<crate::element::Elements>,
    pub runtime: zgui::reactive::Runtime,
    pub assets: Rc<Assets>,
    /// The window's layout inspector, once the host view mounted: nodes
    /// with a recorded source are labelled for it while it records.
    pub inspector: Rc<std::cell::RefCell<Option<zgui::inspect::Inspector>>>,
    /// How deep the lowering running now is (`zgui::deep`).
    pub depth: std::cell::Cell<u32>,
    /// The app's commands, for a command palette listing them.
    #[cfg(feature = "components")]
    pub commands: Rc<crate::command::Commands>,
}

pub(crate) fn lower(node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    let depth = low.depth.get();
    low.depth.set(depth + 1);
    let view = zgui::deep(depth, || lower_node(node, low));
    low.depth.set(depth);
    view
}
fn lower_node(node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    let Some(vm) = node.owner.upgrade() else {
        return div();
    };
    let view = if node.needs_context() {
        let (node, low, vm) = (node.clone(), low.clone(), vm.clone());
        component(move |cx| build(&node, &vm, &low, Some(cx)))
    } else {
        build(node, &vm, low, None)
    };
    decorated(view, node, &vm, low)
}

/// A lowered node with what wraps it: its inspector label, tooltip and
/// theme. Apart from `lower`, whose frame stays live while the node's
/// children lower (debug builds give each view a stack slot of its own).
#[cfg_attr(debug_assertions, inline(never))]
fn decorated(view: View, node: &Rc<Node>, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    // Labelled for the layout inspector while it records.
    let view = match (&node.source, low.inspector.borrow().as_ref()) {
        (Some(source), Some(inspector)) if inspector.recording() => {
            let (inspector, source, name) = (inspector.clone(), source.clone(), node.label());
            view.on_mount(move |handle| inspector.label(handle.node(), name, Some(source)))
        }
        _ => view,
    };
    let view = match node.tooltip.as_deref() {
        Some(
            tip @ crate::node::Tooltip {
                content: Some(_), ..
            },
        ) => tooltip(view, tip, low),
        _ => view,
    };
    match &node.theme {
        None => view,
        Some(spec) => themed(spec, vm, view),
    }
}

/// `anchor` with its tooltip: zgui's own (`View::tooltip`), its content the
/// script's node or text in the theme's look.
fn tooltip(anchor: View, tip: &crate::node::Tooltip, low: &Rc<Lowering>) -> View {
    use crate::node::TooltipContent;
    let mut core: zgui::compose::Tooltip = match tip.content.as_ref().expect("a tooltip's content")
    {
        TooltipContent::Node(node) => lower(node, low).into(),
        TooltipContent::Text(text) => text.clone().into(),
    };
    if let Some(placement) = tip.placement {
        core = core.placement(placement);
    }
    if let Some(delay) = tip.delay {
        core = core.delay(delay);
    }
    anchor.tooltip(core.interactive(tip.interactive))
}

/// `view` under the theme scope `theme = …` asks for.
fn themed(spec: &crate::node::ThemeSpec, vm: &Rc<Vm>, view: View) -> View {
    use crate::node::ThemeSpec;
    let compute = |value: Value, vm: &Vm| {
        crate::theme::override_of(&value).unwrap_or_else(|e| {
            vm.report(mlua::Error::runtime(format!("theme: {e}")));
            zgui::theme::ThemeOverride::new()
        })
    };
    match spec {
        ThemeSpec::Fixed(over) => zgui::compose::theme_scope(over.clone(), view),
        ThemeSpec::State(state) => {
            let (state, vm) = (state.clone(), vm.clone());
            zgui::compose::theme_scope_with(move || compute(state.get(), &vm), view)
        }
        ThemeSpec::Computed(f) => {
            let (f, vm) = (f.clone(), vm.clone());
            zgui::compose::theme_scope_with(
                move || {
                    let value = vm.call::<Value>(&f, ()).unwrap_or_else(|e| {
                        vm.report(e);
                        Value::Nil
                    });
                    compute(value, &vm)
                },
                view,
            )
        }
    }
}
/// The style property a theme token in `prop` sets.
fn token_field(prop: Prop) -> Option<zgui::theme::Field> {
    use Prop::*;
    use zgui::theme::Field;
    Some(match prop {
        Bg => Field::Background,
        TextColor => Field::TextColor,
        BorderColor => Field::BorderColor,
        OutlineColor => Field::OutlineColor,
        Rounded => Field::Radius,
        Border => Field::BorderWidth,
        P => Field::Padding(15),
        Px => Field::Padding(10),
        Py => Field::Padding(5),
        Pt => Field::Padding(1),
        Pr => Field::Padding(2),
        Pb => Field::Padding(4),
        Pl => Field::Padding(8),
        M => Field::Margin(15),
        Mx => Field::Margin(10),
        My => Field::Margin(5),
        Mt => Field::Margin(1),
        Mr => Field::Margin(2),
        Mb => Field::Margin(4),
        Ml => Field::Margin(8),
        Gap => Field::Gap,
        TextSize => Field::TextSize,
        LineHeight => Field::LineHeight,
        LetterSpacing => Field::LetterSpacing,
        Weight => Field::FontWeight,
        Font => Field::FontFamily,
        Shadow => Field::Shadow,
        InsetShadow => Field::InsetShadow,
        Opacity => Field::Opacity,
        BackdropBlur => Field::Blur,
        W => Field::Width,
        H => Field::Height,
        _ => return None,
    })
}

pub(crate) fn bridge<T: PartialEq + Clone + 'static>(
    cx: &mut Context,
    value: &Signal<Value>,
    from: fn(&Value) -> T,
    to: fn(T) -> Value,
) -> Signal<T> {
    let typed = cx.state(value.with_untracked(from));
    let runtime = cx.runtime();
    let (v, t) = (value.clone(), typed.clone());
    cx.retain(runtime.effect(move || {
        t.set(from(&v.get()));
    }));
    let (v, t) = (value.clone(), typed.clone());
    // The widget writes back only after its first change, so mounting never
    // rewrites the script's value (for example 0.1 through f32).
    let mut first = true;
    cx.retain(runtime.effect(move || {
        let next = t.get();
        if !std::mem::take(&mut first) {
            v.set(to(next));
        }
    }));
    typed
}

/// A controlled input's text (`value = function`): what the function shows
/// while the input isn't focused; focused, what is being typed, each edit
/// told to `on_change`. Enter, or leaving it changed, tells `on_commit`, and
/// it shows the function's text again. With the listener that does that.
fn controlled_input(
    cx: &mut Context,
    shown: &Source,
    change: Option<Handler>,
    commit: Option<Handler>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
) -> (
    Signal<String>,
    impl FnMut(&mut zgui::input::EventContext) + 'static,
) {
    let text = cx.state(String::new());
    let runtime = cx.runtime();
    // The function's latest text, and whether the input is being edited.
    let latest: Rc<std::cell::RefCell<String>> = Rc::default();
    let editing: Rc<std::cell::Cell<bool>> = Rc::default();
    let (source, svm) = (resolve_owned(shown), vm.clone());
    let (t, l, e) = (text.clone(), latest.clone(), editing.clone());
    cx.retain(runtime.effect(move || {
        let now = resolve_value(&source, &svm).to_string();
        *l.borrow_mut() = now.clone();
        if !e.get() {
            t.set(now);
        }
    }));
    if let Some(change) = change {
        let (t, l, e, vm, actions) = (
            text.clone(),
            latest.clone(),
            editing.clone(),
            vm.clone(),
            low.actions.clone(),
        );
        let quiet = runtime.clone();
        cx.retain(runtime.effect(move || {
            let now = t.get();
            if e.get() && now != *l.borrow() {
                quiet.untracked(|| run(&change, &vm, &actions, now));
            }
        }));
    }
    let (t, l, e, vm, actions) = (
        text.clone(),
        latest,
        editing,
        vm.clone(),
        low.actions.clone(),
    );
    let listener = move |cx: &mut zgui::input::EventContext| {
        if cx.phase == EventPhase::Capture || cx.target != cx.current_target {
            return;
        }
        let commit_now = |cx: &mut zgui::input::EventContext| {
            let typed = t.with_untracked(Clone::clone);
            if typed != *l.borrow()
                && let Some(commit) = &commit
            {
                run(commit, &vm, &actions, typed);
            }
            let _ = cx;
        };
        match &cx.event {
            InputEvent::Focus => e.set(true),
            InputEvent::KeyDown {
                key: zgui::input::Key::Enter,
                modifiers,
                ..
            } if !modifiers.shift => {
                commit_now(cx);
                // Shown as the function has it now, still focused.
                let now = l.borrow().clone();
                t.set(now);
            }
            InputEvent::Blur => {
                commit_now(cx);
                e.set(false);
                let now = l.borrow().clone();
                t.set(now);
            }
            _ => {}
        }
    };
    (text, listener)
}

/// A modifier's names in key text (`cmd+shift+s`), and how it shows: its
/// glyph on macOS, its word elsewhere. In macOS's order (⌃⌥⇧⌘).
const MODIFIERS: [(&[&str], &str, &str); 5] = [
    (&["ctrl", "control"], "⌃", "Ctrl"),
    (&["alt", "opt", "option"], "⌥", "Alt"),
    (&["shift"], "⇧", "Shift"),
    (&["cmd", "command"], "⌘", "Ctrl"),
    (&["super", "meta"], "⌘", "Super"),
];

fn modifier(name: &str) -> Option<usize> {
    let name = name.to_ascii_lowercase();
    MODIFIERS
        .iter()
        .position(|(names, _, _)| names.contains(&name.as_str()))
}

/// Whether `text` is keys (`cmd+shift+s`, `alt+arrowleft`): modifiers and
/// a key joined by `+`, rather than text already shown as is (`⌘S`).
pub(crate) fn is_shortcut_keys(text: &str) -> bool {
    let parts: Vec<&str> = text.split('+').collect();
    parts.len() > 1
        && parts.last().is_some_and(|key| !key.is_empty())
        && parts[..parts.len() - 1]
            .iter()
            .all(|part| modifier(part).is_some())
}

/// Keys (`cmd+shift+s`) as a menu shows them: ⇧⌘S on macOS, Ctrl+Shift+S
/// elsewhere, `cmd` being Command on macOS and Control elsewhere. A lone
/// modifier (`cmd`) shows as itself: ⌘, or Ctrl.
pub(crate) fn shortcut_label(keys: &str) -> String {
    let mac = cfg!(target_os = "macos");
    let mut parts: Vec<&str> = keys.split('+').collect();
    let key = match parts.pop() {
        Some(last) if modifier(last).is_none() => last,
        Some(last) => {
            parts.push(last);
            ""
        }
        None => "",
    };
    // Ctrl, Alt, Shift, Super elsewhere, `cmd` being Ctrl.
    let order: [usize; 5] = if mac {
        [0, 1, 2, 3, 4]
    } else {
        [0, 3, 1, 2, 4]
    };
    let mut shown: Vec<&str> = Vec::new();
    for index in order {
        let (_, glyph, word) = MODIFIERS[index];
        let label = if mac { glyph } else { word };
        if parts.iter().any(|part| modifier(part) == Some(index)) && !shown.contains(&label) {
            shown.push(label);
        }
    }
    let lower = key.to_ascii_lowercase();
    let named = match lower.as_str() {
        "arrowleft" | "left" => Some(("←", "Left")),
        "arrowright" | "right" => Some(("→", "Right")),
        "arrowup" | "up" => Some(("↑", "Up")),
        "arrowdown" | "down" => Some(("↓", "Down")),
        "enter" | "return" => Some(("↩", "Enter")),
        "tab" => Some(("⇥", "Tab")),
        "escape" | "esc" => Some(("⎋", "Esc")),
        "backspace" => Some(("⌫", "Backspace")),
        "delete" => Some(("⌦", "Delete")),
        "home" => Some(("↖", "Home")),
        "end" => Some(("↘", "End")),
        "pageup" => Some(("⇞", "Page Up")),
        "pagedown" => Some(("⇟", "Page Down")),
        "space" => Some(("Space", "Space")),
        _ => None,
    };
    let key = match named {
        Some((glyph, word)) => (if mac { glyph } else { word }).to_owned(),
        None => key.to_uppercase(),
    };
    if mac {
        shown.concat() + &key
    } else {
        let mut words: Vec<String> = shown.into_iter().map(str::to_owned).collect();
        if !key.is_empty() {
            words.push(key);
        }
        words.join("+")
    }
}

/// A menu item's or command's `shortcut`: keys (`cmd+c`) shown as the
/// platform shows them, other text as it is (`⌘C`).
pub(crate) fn shown_shortcut(text: String) -> String {
    if is_shortcut_keys(&text) {
        shortcut_label(&text)
    } else {
        text
    }
}

pub(crate) fn need<'a>(cx: &'a mut Option<&mut Context>) -> &'a mut Context {
    cx.as_deref_mut()
        .expect("node lowered without a component context")
}

pub(crate) fn number(value: &Value) -> f32 {
    value.as_f64().unwrap_or(0.) as f32
}

/// Calls a script function that builds part of the interface, as a
/// component's body: the effects and computed values it creates live as long
/// as what it built (not as long as the script), and its `ui.on_mount`
/// hooks run once that is mounted.
/// A slot's context, for what is contributed to it.
pub(crate) struct SlotContext(pub Value);
/// A copy of `props` with `slot = context`.
fn with_slot(lua: &mlua::Lua, props: &mlua::Table, context: &Value) -> mlua::Result<mlua::Table> {
    let copy = lua.create_table()?;
    for pair in props.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair?;
        copy.raw_set(key, value)?;
    }
    copy.raw_set("slot", context.clone())?;
    Ok(copy)
}

/// The keys a `ui.each`'s `key` function gave its items, by item. An item
/// that is the same value as at the last pass (a change elsewhere in the
/// list shares the rest) keeps its key without calling the script: one
/// changed row of a thousand calls `key` once, not a thousand times.
#[derive(Default)]
pub(crate) struct ItemKeys {
    last: HashMap<usize, (Value, String)>,
}
impl ItemKeys {
    /// Each item's key; its index where `key` fails.
    pub fn of(&mut self, items: &[Value], key: &mlua::Function, vm: &Vm) -> Vec<String> {
        let mut next = HashMap::with_capacity(items.len());
        let keys = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // Lists and tables are known by their allocation (`identity`),
                // kept alive here so it is not another value's by the next pass.
                let id = crate::value::identity(item);
                if let Some((_, key)) = self.last.get(&id).filter(|_| id != 0) {
                    next.insert(id, (item.clone(), key.clone()));
                    return key.clone();
                }
                match vm.call::<Value>(key, item.clone()) {
                    Ok(k) => {
                        let k = k.to_string();
                        if id != 0 {
                            next.insert(id, (item.clone(), k.clone()));
                        }
                        k
                    }
                    Err(e) => {
                        vm.report(e);
                        i.to_string()
                    }
                }
            })
            .collect();
        self.last = next;
        keys
    }
}

/// A row of a `ui.each` as the key pass last saw it.
struct EachRow {
    /// Its item, to tell when it changed (rows built again then).
    last: Value,
    /// Bumped when the item changes: a new identity, so a new row.
    generation: u64,
    /// A live row's item, as the state its builder was given.
    item: Option<Signal<Value>>,
    /// Seen in this pass.
    seen: bool,
}
/// What tells an item without a `key` from the others at a glance: a
/// table by its allocation (state keeps what a change does not touch the
/// same allocation), anything else by its value.
#[derive(Hash, PartialEq, Eq)]
enum Quick {
    Nil,
    Bool(bool),
    Num(u64),
    Str(Rc<str>),
    At(usize),
}
fn quick(item: &Value) -> Quick {
    match item {
        Value::Nil => Quick::Nil,
        Value::Bool(b) => Quick::Bool(*b),
        Value::Num(n) => Quick::Num(if *n == 0. { 0 } else { n.to_bits() }),
        Value::Str(s) => Quick::Str(s.clone()),
        Value::List(l) => Quick::At(Rc::as_ptr(l).cast::<()>() as usize),
        Value::Map(m) => Quick::At(Rc::as_ptr(m).cast::<()>() as usize),
        Value::Element(e) => Quick::At(Rc::as_ptr(&e.0).cast::<()>() as usize),
    }
}
/// An item's contents, hashed: equal items hash equal.
fn content_hash(item: &Value, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match item {
        Value::List(items) => {
            items.len().hash(hasher);
            items.iter().for_each(|item| content_hash(item, hasher));
        }
        Value::Map(entries) => {
            entries.len().hash(hasher);
            for (key, value) in entries.iter() {
                key.hash(hasher);
                content_hash(value, hasher);
            }
        }
        other => quick(other).hash(hasher),
    }
}
/// The rows of a `ui.each` without a `key` as the last pass left them: their
/// names in list order, and the next name to give.
#[derive(Default)]
struct Unkeyed {
    order: Vec<String>,
    next: u64,
}
/// The rows of a `ui.each` without a `key` for `list`: each item keeps the
/// row of the same item last pass, wherever it moved (an insert or removal
/// builds one row, not every row after it). Items are matched where they
/// were first (a pointer compare each, in the usual pass), then the rest by
/// `quick`, then by their contents (a list built afresh from state holds
/// equal tables, new ones). An item still unmatched takes the free row at
/// its place, built again if its item differs (or, `live`, given the new
/// item), as rows keyed by position were. Returns each item's row name;
/// `rows` keeps the rows still shown.
fn unkeyed_rows(
    list: &[Value],
    rows: &mut HashMap<String, EachRow>,
    state: &mut Unkeyed,
    live: bool,
    runtime: &zgui::reactive::Runtime,
) -> Vec<String> {
    let mut names: Vec<Option<String>> = vec![None; list.len()];
    for (index, item) in list.iter().enumerate() {
        if let Some(name) = state.order.get(index)
            && rows[name].last == *item
        {
            rows.get_mut(name).unwrap().seen = true;
            names[index] = Some(name.clone());
        }
    }
    let free: Vec<&String> = state
        .order
        .iter()
        .filter(|name| !rows[*name].seen)
        .collect();
    if !free.is_empty() && names.iter().any(Option::is_none) {
        // Earlier rows first: equal items keep their order.
        let mut by_item: HashMap<Quick, Vec<String>> = HashMap::new();
        for name in free.iter().rev() {
            by_item
                .entry(quick(&rows[*name].last))
                .or_default()
                .push((*name).clone());
        }
        for (item, name) in list.iter().zip(names.iter_mut()) {
            if name.is_none()
                && let Some(found) = by_item.get_mut(&quick(item)).and_then(Vec::pop)
            {
                rows.get_mut(&found).unwrap().seen = true;
                *name = Some(found);
            }
        }
        let hash = |item: &Value| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            content_hash(item, &mut hasher);
            std::hash::Hasher::finish(&hasher)
        };
        let mut by_contents: HashMap<u64, Vec<String>> = HashMap::new();
        for name in free.iter().rev().filter(|name| !rows[**name].seen) {
            by_contents
                .entry(hash(&rows[*name].last))
                .or_default()
                .push((*name).clone());
        }
        for (item, name) in list.iter().zip(names.iter_mut()) {
            if name.is_some() || by_contents.is_empty() {
                continue;
            }
            if let Some(candidates) = by_contents.get_mut(&hash(item))
                && let Some(at) = candidates.iter().rposition(|n| rows[n].last == *item)
            {
                let found = candidates.remove(at);
                rows.get_mut(&found).unwrap().seen = true;
                *name = Some(found);
            }
        }
    }
    for (index, item) in list.iter().enumerate() {
        if names[index].is_some() {
            continue;
        }
        let name = match state.order.get(index).filter(|name| !rows[*name].seen) {
            Some(name) => {
                let row = rows.get_mut(name).unwrap();
                if let (true, Some(signal)) = (live, &row.item) {
                    let (signal, item) = (signal.clone(), item.clone());
                    runtime.untracked(|| signal.set(item));
                } else if !live {
                    row.generation += 1;
                }
                row.last = item.clone();
                row.seen = true;
                name.clone()
            }
            None => {
                state.next += 1;
                let name = state.next.to_string();
                rows.insert(
                    name.clone(),
                    EachRow {
                        last: item.clone(),
                        generation: 0,
                        item: None,
                        seen: true,
                    },
                );
                name
            }
        };
        names[index] = Some(name);
    }
    state.order = names.into_iter().flatten().collect();
    state.order.clone()
}
pub(crate) fn build_scoped(
    vm: &Rc<Vm>,
    cx: &mut Context,
    low: &Rc<Lowering>,
    f: &mlua::Function,
    args: impl mlua::IntoLuaMulti,
) -> View {
    let scope = Rc::new(Scope::default());
    let previous = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .map(|env| env.scope.replace(Some(scope.clone())));
    let result = vm.call::<mlua::Value>(f, args);
    if let (Some(previous), Some(env)) = (previous, vm.lua.app_data_ref::<crate::node::Env>()) {
        *env.scope.borrow_mut() = previous;
    }
    for resource in scope.retained.borrow_mut().drain(..) {
        cx.retain(resource);
    }
    let mounted: Vec<mlua::Function> = scope.mounted.borrow_mut().drain(..).collect();
    let view = built(vm, result, low);
    if mounted.is_empty() {
        return view;
    }
    let vm = vm.clone();
    view.on_mount(move |_| {
        for f in mounted {
            if let Err(e) = vm.call::<()>(&f, ()) {
                vm.report(e);
            }
        }
    })
}

/// A web view: its page from `src` (followed) or `html`, its handlers
/// subscribed for as long as the view is mounted.
fn web_view_of(spec: &crate::node::WebSpec, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    use zgui::web_view::{WebViewEvent, web_view, web_view_src};
    let web = spec.web.clone();
    let view = match (&spec.src, &spec.html) {
        (Some(src), _) => {
            let (src, vm) = (resolve_owned(src), vm.clone());
            web_view_src(&web, move || match resolve(&src, Ty::Str, None, &vm) {
                Some(Lit::Str(url)) => url.to_string(),
                _ => String::new(),
            })
        }
        (None, Some((html, base))) => {
            web.load_html(html.clone(), base.as_deref());
            web_view(&web)
        }
        (None, None) => web_view(&web),
    };
    let handlers = [
        spec.on_navigate.clone(),
        spec.on_load.clone(),
        spec.on_title.clone(),
        spec.on_message.clone(),
        spec.on_error.clone(),
        spec.on_new_window.clone(),
    ];
    if handlers.iter().all(Option::is_none) {
        return view;
    }
    let (vm, _low) = (vm.clone(), low.clone());
    view.on_mount(move |handle| {
        let [navigate, load, title, message, error, new_window] = handlers;
        let vm = Rc::downgrade(&vm);
        let subscription = web.on_event(move |event| {
            let Some(vm) = vm.upgrade() else { return };
            let call = |f: &Option<mlua::Function>, args: mlua::MultiValue| {
                if let Some(f) = f
                    && let Err(e) = vm.call::<()>(f, args)
                {
                    vm.report(e);
                }
            };
            let lua = &vm.lua;
            let pack = |args: &[&str]| -> mlua::MultiValue {
                args.iter()
                    .filter_map(|a| lua.create_string(a).ok().map(mlua::Value::String))
                    .collect()
            };
            if navigate.is_some() {
                let (kind, url) = match event {
                    WebViewEvent::NavigationStarted { url } => ("started", url.as_str()),
                    WebViewEvent::NavigationCommitted { url } => ("committed", url.as_str()),
                    WebViewEvent::NavigationFinished { url } => ("finished", url.as_str()),
                    WebViewEvent::NavigationFailed { url, .. } => ("failed", url.as_str()),
                    WebViewEvent::UrlChanged { url } => ("changed", url.as_str()),
                    _ => ("", ""),
                };
                if !kind.is_empty()
                    && let Ok(t) = lua.create_table()
                {
                    let _ = t.set("kind", kind);
                    let _ = t.set("url", url);
                    call(
                        &navigate,
                        mlua::MultiValue::from_vec(vec![mlua::Value::Table(t)]),
                    );
                }
            }
            match event {
                WebViewEvent::NavigationFinished { url } => call(&load, pack(&[url])),
                WebViewEvent::TitleChanged { title: t } => call(&title, pack(&[t])),
                WebViewEvent::NavigationFailed { url, error: e } => call(&error, pack(&[url, e])),
                WebViewEvent::NewWindowRequested { url } => call(&new_window, pack(&[url])),
                WebViewEvent::ProcessTerminated => {
                    call(&error, pack(&["", "the page's process ended"]))
                }
                WebViewEvent::Message {
                    body,
                    origin,
                    main_frame,
                } => {
                    if let Some(f) = &message {
                        let value = crate::node::json_value(body);
                        // Who sent it: check before acting on it.
                        if let Err(e) = vm.call::<()>(f, (value, origin.as_str(), *main_frame)) {
                            vm.report(e);
                        }
                    }
                }
                _ => {}
            }
        });
        handle.retain(subscription);
    })
}

/// Builds a node a script function returned.
fn built(vm: &Rc<Vm>, result: mlua::Result<mlua::Value>, low: &Rc<Lowering>) -> View {
    match result.and_then(|v| match v {
        mlua::Value::UserData(ud) => ud.borrow::<NodeUd>().map(|n| n.0.clone()),
        mlua::Value::Nil => Err(mlua::Error::runtime(
            "the function returned nothing; return a ui node",
        )),
        other => Err(mlua::Error::runtime(format!(
            "expected a ui node, got {}",
            other.type_name()
        ))),
    }) {
        Ok(node) => lower(&node, low),
        Err(error) => {
            vm.report(error);
            div()
        }
    }
}

/// The view a node's kind makes, before its styles, handlers and
/// children. Apart from `build`, whose frame stays live while its children
/// lower: in a debug build each arm's views take stack of their own.
#[cfg_attr(debug_assertions, inline(never))]
fn kind_view(
    node: &Rc<Node>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    mut cx: Option<&mut Context>,
    element: Option<&Element>,
) -> View {
    match &node.kind {
        Kind::Column => column(),
        Kind::Row => row(),
        Kind::Div => div(),
        Kind::Overlay => overlay(),
        Kind::Button => button(),
        Kind::Scroll {
            offset,
            horizontal,
            follow,
            scrollbar,
        } => {
            let offset = match offset {
                Some(offset) => bridge(need(&mut cx), offset, number, |x| Value::Num(x as f64)),
                None => need(&mut cx).state(0_f32),
            };
            if let Some(element) = &element {
                let _ = element.scroll.set((offset.clone(), *horizontal));
            }
            let view = if *horizontal {
                scroll_x(offset)
            } else {
                scroll(offset)
            };
            let view = view.follow_end(*follow);
            if *scrollbar {
                view.scrollbar(true)
            } else {
                view
            }
        }
        Kind::Text(text) => lower_text(text, vm),
        Kind::Rich(spans) => lower_rich(spans, vm, low),
        Kind::Image { .. } => lower_image(&node.kind, vm, low),
        Kind::Svg { label, src } => {
            let (src, vm2, low2) = (resolve_owned(src), vm.clone(), low.clone());
            let theme = cx.as_deref().map(Context::theme);
            // The tint may be state or computed: the icon re-tints when it changes.
            let tint = node
                .props
                .iter()
                .find(|(p, _)| *p == Prop::Tint)
                .map(|(_, source)| resolve_owned(source));
            svg_signal(label.clone(), move || {
                let source = match resolve(&src, Ty::Str, None, &vm2) {
                    Some(Lit::Str(path)) => path,
                    _ => "code:<svg xmlns='http://www.w3.org/2000/svg'/>".into(),
                };
                let data = load_svg(&vm2, &low2, &source);
                match tint
                    .as_ref()
                    .and_then(|t| resolve(t, Ty::Color, None, &vm2))
                {
                    Some(Lit::Color(c)) => Arc::new(data.tinted(c)),
                    // `"currentColor"`: the inherited text colour, as CSS's.
                    Some(Lit::Str(_)) => Arc::new(data.current_color()),
                    Some(Lit::Token(t)) => match theme.as_ref().map(|theme| theme.get(t).color()) {
                        Some(Some(c)) => Arc::new(data.tinted(c)),
                        _ => data,
                    },
                    _ => data,
                }
            })
        }
        Kind::WebView(spec) => web_view_of(spec, vm, low),
        Kind::Terminal(term) => zgui::terminal::terminal(term),
        Kind::Canvas(draw) => {
            let (draw, vm) = (draw.clone(), vm.clone());
            let theme = match cx.as_deref().map(Context::theme) {
                Some(theme) => theme,
                None => zgui::theme::Themes::of(&low.runtime).root().clone(),
            };
            canvas(move |(w, h)| {
                let recorder = crate::canvas::Recorder(Default::default());
                let result = vm.lua.create_userdata(recorder).and_then(|ud| {
                    crate::canvas::draw_in(theme.clone(), || {
                        vm.call::<()>(&draw, (ud.clone(), w, h))
                    })?;
                    ud.take::<crate::canvas::Recorder>()
                });
                match result {
                    Ok(recorder) => recorder.0.into_inner(),
                    Err(error) => {
                        vm.report(error);
                        Default::default()
                    }
                }
            })
        }
        Kind::Checkbox {
            label,
            value,
            mixed,
        } => {
            let view = checkbox(
                label.clone(),
                bridge(need(&mut cx), value, Value::truthy, Value::Bool),
            );
            match mixed {
                Some(mixed) => view.mixed(bridge(need(&mut cx), mixed, Value::truthy, Value::Bool)),
                None => view,
            }
        }
        Kind::Input {
            label,
            value,
            multiline,
            placeholder,
            placeholder_color,
            select_on_focus,
            line_numbers,
            secure,
            spellcheck,
        } => {
            let (value, controlled) = match value {
                crate::node::InputValue::State(value) => (
                    bridge(
                        need(&mut cx),
                        value,
                        |v| v.to_string(),
                        |s| Value::Str(s.into()),
                    ),
                    None,
                ),
                crate::node::InputValue::Shown {
                    shown,
                    change,
                    commit,
                } => {
                    let (text, control) = controlled_input(
                        need(&mut cx),
                        shown,
                        change.clone(),
                        commit.clone(),
                        vm,
                        low,
                    );
                    (text, Some(control))
                }
            };
            let mut view = if *multiline {
                text_area(label.clone(), value)
            } else {
                text_input(label.clone(), value)
            };
            if let Some(text) = placeholder {
                let (text, vm) = (resolve_owned(text), vm.clone());
                view = view.placeholder_when(move || match resolve(&text, Ty::Str, None, &vm) {
                    Some(Lit::Str(text)) => text.to_string(),
                    _ => String::new(),
                });
            }
            match placeholder_color {
                Some(Source::Lit(Lit::Color(color))) => view = view.placeholder_color(*color),
                Some(color) => {
                    // A token (`"$text.tertiary"`) follows the element's theme;
                    // anything unresolved shows the default placeholder colour.
                    let (color, vm) = (resolve_owned(color), vm.clone());
                    let theme = match cx.as_deref().map(Context::theme) {
                        Some(theme) => theme,
                        None => zgui::theme::Themes::of(&low.runtime).root().clone(),
                    };
                    view = view.placeholder_color_when(move || {
                        let muted = || theme.color(zgui::theme::tok::TEXT_TERTIARY);
                        match resolve(&color, Ty::Color, None, &vm) {
                            Some(Lit::Color(color)) => color,
                            Some(Lit::Token(t)) => theme.get(t).color().unwrap_or_else(muted),
                            _ => muted(),
                        }
                    });
                }
                None => {}
            }
            if let Some(control) = controlled {
                view = view.on_event(control);
            }
            if *secure {
                view = view.secure(true);
            }
            match spellcheck {
                None | Some(Source::Lit(Lit::Bool(false))) => {}
                Some(Source::Lit(Lit::Bool(true))) => view = view.spellcheck(true),
                Some(check) => {
                    let (check, vm) = (resolve_owned(check), vm.clone());
                    view = view.spellcheck_when(move || {
                        resolve(&check, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
                    });
                }
            }
            if *select_on_focus {
                view = view.select_on_focus(true);
            }
            if *line_numbers {
                view = view.line_numbers(true);
            }
            view
        }
        // `nil` is indeterminate: work of unknown size.
        Kind::Progress { label, value } => progress(
            label.clone(),
            bridge(
                need(&mut cx),
                value,
                |v| v.as_f64().map_or(f32::NAN, |n| n as f32),
                |x| {
                    if x.is_nan() {
                        Value::Nil
                    } else {
                        Value::Num(x as f64)
                    }
                },
            ),
        ),
        Kind::Slider {
            label,
            value,
            min,
            max,
            step,
        } => {
            let view = slider(
                label.clone(),
                bridge(need(&mut cx), value, number, |x| Value::Num(x as f64)),
                *min..=*max,
            );
            match step {
                Some(step) => view.step(*step),
                None => view,
            }
        }
        #[cfg(feature = "components")]
        Kind::Library(library) => crate::library::view(library, node, vm, low, cx),
        Kind::Split { size, options } => {
            let size = bridge(need(&mut cx), size, number, |x| Value::Num(x as f64));
            let mut panes = node.children.iter().map(|child| lower(child, low));
            let (first, second) = (
                panes.next().unwrap_or_else(div),
                panes.next().unwrap_or_else(div),
            );
            split(size, options.clone(), first, second)
        }
        Kind::Show {
            when,
            then,
            otherwise,
        } => {
            let (when_vm, vm) = (vm.clone(), vm.clone());
            let (then, otherwise, low) = (then.clone(), otherwise.clone(), low.clone());
            let when = Rc::new(resolve_owned(when));
            switch(
                move || {
                    resolve(&when, Ty::Bool, None, &when_vm).is_some_and(|l| l == Lit::Bool(true))
                },
                move |on, _| {
                    let _keep = &vm;
                    match (on, &otherwise) {
                        (true, _) => lower(&then, &low),
                        (false, Some(otherwise)) => lower(otherwise, &low),
                        // Nothing: not a flex item either (no gap for it).
                        (false, None) => div().hidden(),
                    }
                },
            )
            // Lays out as what it shows, as if in its place.
            .contents()
        }
        Kind::Switch {
            value,
            cases,
            default,
        } => {
            let (value_vm, vm) = (vm.clone(), vm.clone());
            let cases: Vec<(Value, Rc<Node>)> = cases.clone();
            let (default, low) = (default.clone(), low.clone());
            let value = Rc::new(resolve_owned(value));
            switch(
                move || resolve_value(&value, &value_vm),
                move |key, _| {
                    let _keep = &vm;
                    let found = cases
                        .iter()
                        .find(|(case, _)| *case == key || case.to_string() == key.to_string());
                    match (found, &default) {
                        (Some((_, node)), _) => lower(node, &low),
                        (None, Some(node)) => lower(node, &low),
                        (None, None) => div().hidden(),
                    }
                },
            )
            .contents()
        }
        Kind::Each {
            items,
            build: builder,
            key,
            motion: Some(motion),
        } => crate::motion::each(
            need(&mut cx),
            items.clone(),
            builder.clone(),
            key.clone(),
            motion.clone(),
            vm,
            low,
        ),
        Kind::Each {
            items,
            build: builder,
            key,
            motion: None,
        } => {
            // Rows are keyed by the script's `key` (they keep their state as
            // the list reorders), else by their items (`unkeyed_rows`). A
            // row whose item changes
            // under its key is built again, unless its builder takes a third
            // parameter: then it gets its item as state, kept current, and
            // updates in place (a streaming message's row, its text growing).
            // The key pass notes each row's index for the build that follows.
            let live = builder.info().num_params >= 3;
            let rows: Rc<std::cell::RefCell<HashMap<String, EachRow>>> = Rc::default();
            let noted: Rc<std::cell::RefCell<HashMap<String, (usize, String)>>> = Rc::default();
            let unkeyed = std::cell::RefCell::new(Unkeyed::default());
            let (keys_from, items) = (items.clone(), items.clone());
            let (key_fn, key_vm) = (key.clone(), vm.clone());
            let mut item_keys = ItemKeys::default();
            let (rows_in_keys, noted_in_keys) = (rows.clone(), noted.clone());
            let runtime = vm.runtime.clone();
            let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
            keyed(
                move || {
                    keys_from.with(|list| {
                        let mut noted = noted_in_keys.borrow_mut();
                        noted.clear();
                        let mut rows = rows_in_keys.borrow_mut();
                        if key_fn.is_none() {
                            let names = unkeyed_rows(
                                list.list(),
                                &mut rows,
                                &mut unkeyed.borrow_mut(),
                                live,
                                &runtime,
                            );
                            rows.retain(|_, state| std::mem::take(&mut state.seen));
                            return names
                                .into_iter()
                                .enumerate()
                                .map(|(i, row)| {
                                    let identity = if live {
                                        row.clone()
                                    } else {
                                        format!("{row}@{}", rows[&row].generation)
                                    };
                                    noted.insert(identity.clone(), (i, row));
                                    identity
                                })
                                .collect();
                        }
                        let mut seen: HashMap<String, usize> = HashMap::new();
                        let keys = key_fn
                            .as_ref()
                            .map(|f| item_keys.of(list.list(), f, &key_vm));
                        let identities = list
                            .list()
                            .iter()
                            .enumerate()
                            .map(|(i, item)| {
                                let base = match &keys {
                                    Some(keys) => keys[i].clone(),
                                    None => i.to_string(),
                                };
                                let n = seen.entry(base.clone()).or_default();
                                *n += 1;
                                let row = format!("{base}#{n}");
                                let identity = match rows.get_mut(&row) {
                                    Some(state) if live => {
                                        state.seen = true;
                                        if let Some(signal) = &state.item {
                                            let item = item.clone();
                                            let signal = signal.clone();
                                            runtime.untracked(|| signal.set(item));
                                        }
                                        row.clone()
                                    }
                                    Some(state) => {
                                        state.seen = true;
                                        if state.last != *item {
                                            state.last = item.clone();
                                            state.generation += 1;
                                        }
                                        format!("{row}@{}", state.generation)
                                    }
                                    None => {
                                        rows.insert(
                                            row.clone(),
                                            EachRow {
                                                last: item.clone(),
                                                generation: 0,
                                                item: None,
                                                seen: true,
                                            },
                                        );
                                        if live {
                                            row.clone()
                                        } else {
                                            format!("{row}@0")
                                        }
                                    }
                                };
                                noted.insert(identity.clone(), (i, row));
                                identity
                            })
                            .collect::<Vec<_>>();
                        // Rows gone from the list go from here too.
                        rows.retain(|_, state| std::mem::take(&mut state.seen));
                        identities
                    })
                },
                move |identity, cx| {
                    let (index, row) = noted.borrow().get(&identity).cloned().unwrap_or_default();
                    let item = items
                        .with_untracked(|list| list.list().get(index).cloned().unwrap_or_default());
                    if !live {
                        return build_scoped(&vm, cx, &low, &builder, (item, index + 1));
                    }
                    let signal = vm.runtime.signal(item.clone());
                    if let Some(state) = rows.borrow_mut().get_mut(&row) {
                        state.item = Some(signal.clone());
                    }
                    build_scoped(
                        &vm,
                        cx,
                        &low,
                        &builder,
                        (item, index + 1, crate::value::State::new(signal)),
                    )
                },
            )
        }
        Kind::List { scrollbar, .. } => {
            let list = lower_list(&node.kind, vm, low, &mut cx);
            if *scrollbar {
                list.scrollbar(true)
            } else {
                list
            }
        }
        Kind::Modal {
            label,
            open,
            placement,
            autofocus,
            layer,
            dismiss,
        } => {
            // Built when it first opens (`View::lazy`), as every script
            // overlay: closed, a list's menus and cards cost nothing.
            let view = modal(
                label.clone(),
                bridge(need(&mut cx), open, Value::truthy, Value::Bool),
            )
            .lazy();
            let view = if *autofocus {
                view
            } else {
                view.autofocus(false)
            };
            let view = if *layer { view.layer() } else { view };
            let view = match dismiss {
                Some(d) => view.dismiss_on_backdrop(*d),
                None => view,
            };
            match placement {
                Some(at) => view.modal_placement(*at),
                None => view,
            }
        }
        Kind::Popover {
            label,
            open,
            anchor,
            placement,
            autofocus,
            arrow,
            dismiss,
            match_anchor,
        } => {
            let view = popover(
                label.clone(),
                bridge(need(&mut cx), open, Value::truthy, Value::Bool),
                lower(anchor, low),
            )
            .lazy();
            let view = if *autofocus {
                view
            } else {
                view.autofocus(false)
            };
            let view = match arrow {
                Some(size) => view.arrow(*size),
                None => view,
            };
            let view = match dismiss {
                Some(d) => view.dismiss_on_backdrop(*d),
                None => view,
            };
            let view = if *match_anchor {
                view.match_anchor_width()
            } else {
                view
            };
            match placement {
                Some(at) => view.placement(*at),
                None => view,
            }
        }
        Kind::Menu {
            label,
            open,
            anchor,
            placement,
            autofocus,
            over,
            match_anchor,
        } => {
            let mut view = menu(
                label.clone(),
                bridge(need(&mut cx), open, Value::truthy, Value::Bool),
                lower(anchor, low),
            )
            .lazy();
            if *match_anchor {
                view = view.match_anchor_width();
            }
            if let Some(over) = over.clone() {
                let vm = vm.clone();
                view = view.open_over(move || {
                    vm.call::<Option<String>>(&over, ())
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                });
            }
            let view = if *autofocus {
                view
            } else {
                view.autofocus(false)
            };
            match placement {
                Some(at) => view.placement(*at),
                None => view,
            }
        }
        Kind::MenuItem(label, options) => {
            let mut item = menu_item(label.clone());
            if options.keep_open {
                item = item.keep_open();
            }
            if let Some(checked) = &options.checked {
                let (checked, vm) = (resolve_owned(checked), vm.clone());
                item = item.menu_checked_when(move || {
                    resolve(&checked, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
                });
            }
            if options.radio {
                item = item.menu_radio();
            }
            if let Some(keys) = &options.shortcut {
                item = item.shortcut(keys.clone());
            }
            item
        }
        Kind::Submenu {
            label,
            open,
            trigger,
        } => {
            let view = submenu(
                label.clone(),
                bridge(need(&mut cx), open, Value::truthy, Value::Bool),
            );
            match trigger {
                // A menu item is the row; anything else goes in one.
                Some(node) if matches!(node.kind, Kind::MenuItem(..)) => {
                    view.trigger(lower(node, low))
                }
                Some(node) => view.trigger(menu_item(label.clone()).child(lower(node, low))),
                None => view,
            }
        }
        Kind::Component {
            build: builder,
            props,
        } => {
            let (builder, props, vm, low) =
                (builder.clone(), props.clone(), vm.clone(), low.clone());
            component(move |cx| {
                // Contributed to a slot with a context: its own props, with
                // the context as `slot`.
                let props = match cx.try_service::<SlotContext>() {
                    Some(context) => match with_slot(&vm.lua, &props, &context.0) {
                        Ok(props) => props,
                        Err(error) => {
                            vm.report(error);
                            props.clone()
                        }
                    },
                    None => props.clone(),
                };
                build_scoped(&vm, cx, &low, &builder, props)
            })
        }
        Kind::Presence {
            value,
            build: builder,
            enter,
            exit,
            wait,
            initial,
        } => presence(
            need(&mut cx),
            Rc::new(resolve_owned(value)),
            builder.clone(),
            enter.clone(),
            exit.clone(),
            *wait,
            *initial,
            node.props.iter().any(|(p, _)| BOX.contains(p)),
            vm,
            low,
        ),
        Kind::Slot {
            name,
            context,
            from,
        } => column().children(
            low.slots
                .get(name)
                .into_iter()
                .flatten()
                .filter(|(owner, _)| from.as_deref().is_none_or(|from| **owner == *from))
                .map(|(_, n)| match context {
                    // What is contributed here hears it (`props.slot`).
                    Some(context) => {
                        zgui::compose::provide(SlotContext(context.clone()), lower(n, low))
                    }
                    None => lower(n, low),
                }),
        ),
    }
}

fn build(node: &Rc<Node>, vm: &Rc<Vm>, low: &Rc<Lowering>, mut cx: Option<&mut Context>) -> View {
    // Layout moves animate through an element too, one selectors do not find.
    let element = (node.targetable() || node.tracks_layout()).then(|| {
        Element::new(
            &low.runtime,
            node.id.clone(),
            node.classes.clone(),
            node.tag(),
        )
    });
    let mut view = kind_view(node, vm, low, cx.as_deref_mut(), element.as_deref());
    if let Some(mask) = &node.mask {
        view = view
            .mask(mask.image.clone(), mask.size)
            .mask_offset(mask.offset.0, mask.offset.1);
        // The engine moves a sweeping mask itself, frame by frame.
        if let Some(sweep) = mask.sweep.clone() {
            let cx = need(&mut cx);
            let (tasks, frames) = (cx.tasks(), cx.frames());
            view = view.on_mount(move |mounted| {
                tasks.spawn(mounted.sweep_mask(frames.clone(), sweep.by, sweep.period, sweep.rate));
            });
        }
    }
    // A split, a disclosure and an accordion lower their children
    // themselves.
    if !node.children.is_empty()
        && !matches!(node.kind, Kind::Split { .. })
        && !node.kind.library_takes_children()
    {
        // Only regions among the children take the container's flow.
        let flow = node
            .children
            .iter()
            .any(|child| child.kind.flows())
            .then(|| flow(node));
        view = view.children(node.children.iter().map(|child| {
            let view = lower(child, low);
            match &flow {
                Some(flow) if child.kind.flows() => apply_all(view, flow),
                _ => view,
            }
        }));
    }
    styled(view, node, vm, low, cx, element)
}

/// What a node sets on its view, after its children: styles, handlers,
/// observers. Apart from `build` for its frame, as `kind_view`.
#[cfg_attr(debug_assertions, inline(never))]
fn styled(
    mut view: View,
    node: &Rc<Node>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    mut cx: Option<&mut Context>,
    element: Option<Rc<Element>>,
) -> View {
    // Literal styles apply once. Anything that can change, and an element's
    // animated values, share one reactive style.
    let mut statics: smallvec::SmallVec<[(Prop, Lit); 8]> = smallvec::SmallVec::new();
    let mut dynamic = Vec::new();
    for (prop, source) in &node.props {
        match source {
            Source::Lit(lit) => statics.push((*prop, lit.clone())),
            Source::Anim(spec) => {
                let scalar = anim::drive(need(&mut cx), spec);
                dynamic.push((*prop, resolve_owned(source), Some(scalar)));
            }
            _ => dynamic.push((*prop, resolve_owned(source), None)),
        }
    }
    // A dynamic part of a translation or transform carries its literal
    // parts along: each is set as a whole.
    let groups: Vec<&[Prop]> = TRANSFORM_GROUPS
        .iter()
        .copied()
        .filter(|g| dynamic.iter().any(|(p, _, _)| g.contains(p)))
        .collect();
    for group in groups {
        {
            let moved: Vec<_> = statics
                .iter()
                .filter(|(p, _)| group.contains(p))
                .cloned()
                .collect();
            statics.retain(|(p, _)| !group.contains(p));
            dynamic.extend(moved.into_iter().map(|(p, l)| (p, Owned::Lit(l), None)));
        }
    }
    if let Some(element) = &element {
        // What the node's own literals place it at, for animations that
        // start before the scene has them (an `on_mount` animation of
        // `scale` keeps a literal `rotate`).
        let (translate, local) = literal_transform(&statics);
        let literal = |prop| {
            statics.iter().find_map(|(p, l)| match (p == &prop, l) {
                (true, Lit::Num(v)) => Some(*v),
                _ => None,
            })
        };
        let effects = (
            literal(Prop::Opacity).unwrap_or(1.),
            literal(Prop::Blur).unwrap_or(0.),
        );
        element.set_base(translate, local, effects);
        element.set_base_color([
            literal(Prop::Brightness).unwrap_or(1.),
            literal(Prop::Contrast).unwrap_or(1.),
            literal(Prop::Saturate).unwrap_or(1.),
        ]);
        let mut corners = None;
        for (prop, lit) in &statics {
            match (prop, lit) {
                (Prop::Rounded, Lit::Num(r)) => corners = Some(zgui::decoration::Corners::all(*r)),
                (Prop::Corners, Lit::Corners(c)) => corners = Some(*c),
                _ => {}
            }
        }
        element.set_base_corners(corners);
        element.set_base_shader(
            node.shader_params
                .map(|params| zgui::decoration::ShaderUniforms { time: 0., params }),
        );
        element.set_base_clip(statics.iter().find_map(|(p, l)| match (p, l) {
            (Prop::ClipInset, Lit::Corners(c)) => {
                Some([c.top_left, c.top_right, c.bottom_right, c.bottom_left])
            }
            _ => None,
        }));
    }
    view = apply_all(view, &statics);
    // A background image: over `bg`, under the border and children, cut by
    // the corners. It goes in the node's one reactive style, after anything
    // animated, so an animated `bg` keeps it.
    let bg_image = node
        .props
        .iter()
        .find(|(p, _)| *p == Prop::BgImage)
        .map(|(_, source)| {
            let fit = node.props.iter().find_map(|(p, s)| match (p, s) {
                (Prop::BgFit, Source::Lit(Lit::Str(v))) => Some(match &**v {
                    "contain" => ObjectFit::Contain,
                    "fill" => ObjectFit::Fill,
                    "none" => ObjectFit::None,
                    "scale_down" => ObjectFit::ScaleDown,
                    _ => ObjectFit::Cover,
                }),
                _ => None,
            });
            (
                resolve_owned(source),
                fit.unwrap_or(ObjectFit::Cover),
                low.clone(),
            )
        });
    let shader = node.shader.clone().map(|shader| {
        (
            shader,
            node.shader_params.unwrap_or_default(),
            node.shader_tokens,
        )
    });
    if !dynamic.is_empty() || element.is_some() || bg_image.is_some() || shader.is_some() {
        let vm = vm.clone();
        let element = element.clone();
        view = view.reactive_style(move || {
            let lits: Vec<(Prop, Lit)> = dynamic
                .iter()
                .filter_map(|(prop, source, scalar)| {
                    Some((*prop, resolve_style(source, *prop, scalar.as_ref(), &vm)?))
                })
                .collect();
            let styles = match &element {
                Some(element) => with_animated(element, lits),
                None => apply_all(Styles::new(), &lits),
            };
            let image = bg_image.as_ref().and_then(|(src, fit, low)| {
                match resolve(src, Ty::Str, None, &vm) {
                    Some(Lit::Str(path)) => load_image(&vm, low, &path).map(|image| (image, *fit)),
                    _ => None,
                }
            });
            let styles = match image {
                Some((image, fit)) => styles.bg_image(image, fit),
                None => styles,
            };
            // A shader fill, with its literal parameters unless animated.
            match &shader {
                Some((fill, params, tokens)) => {
                    let animated = element.as_ref().and_then(|e| e.animated.borrow().shader);
                    let uniforms = animated.unwrap_or(zgui::decoration::ShaderUniforms {
                        time: 0.,
                        params: *params,
                    });
                    // Theme colours bind to their slots: resolved where the
                    // node is styled, and again when the theme changes.
                    let mut styles = styles;
                    for (slot, token) in tokens.iter().enumerate() {
                        if let Some(token) = token {
                            styles =
                                styles.token(zgui::theme::Field::ShaderParam(slot as u8), *token);
                        }
                    }
                    styles.bg_shader(fill.clone()).shader_uniforms(uniforms)
                }
                None => styles,
            }
        });
    }
    if let Some(selection) = &node.variant_interaction {
        // Variants chosen by state set interaction styles: the choice's,
        // the element's own over them, swapped when the choice changes.
        let (selection, own) = (selection.clone(), crate::node::interaction_of(&node.pseudo));
        let mut last: Option<Box<[u16]>> = None;
        view = view.reactive_interaction(move || {
            let key = selection.key();
            if last.as_ref() == Some(&key) {
                return None;
            }
            let layer = selection.layer(&key);
            last = Some(key);
            Some(std::array::from_fn(|slot| {
                let mut lits = layer.interaction[slot].clone();
                crate::variants::over(&mut lits, &own[slot]);
                (!lits.is_empty()).then(|| apply_all(Styles::new(), &lits))
            }))
        });
    } else {
        for (pseudo, lits) in &node.pseudo {
            let lits = lits.clone();
            let style = move |s: Styles| apply_all(s, &lits);
            view = match pseudo {
                Pseudo::Hover => view.hover(style),
                Pseudo::Active => view.active(style),
                Pseudo::Focus => view.focus(style),
                Pseudo::Disabled => view.disabled_style(style),
            };
        }
    }
    if let Some(disabled) = &node.disabled {
        let (disabled, vm) = (resolve_owned(disabled), vm.clone());
        view = view.disabled_when(move || {
            resolve(&disabled, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
        });
    }
    if let Some(focusable) = node.focusable {
        view = view.focusable(focusable);
    }
    if node.window_drag {
        view = view.window_drag();
    }
    if let Some(selectable) = node.selectable {
        view = view.selectable(selectable);
    }
    if let Some(colors) = &node.selection_colors {
        // A token (`"$accent"`) follows the node's theme; anything that
        // does not resolve keeps the theme's own colour.
        let theme = match cx.as_deref().map(Context::theme) {
            Some(theme) => theme,
            None => zgui::theme::Themes::of(&low.runtime).root().clone(),
        };
        for (at, color) in colors.iter().enumerate() {
            let Some(color) = color else {
                continue;
            };
            let (color, vm, theme) = (resolve_owned(color), vm.clone(), theme.clone());
            let fallback = [tok::SELECTION, tok::SELECTION_TEXT][at];
            let follow = move || match resolve(&color, Ty::Color, None, &vm) {
                Some(Lit::Color(color)) => color,
                Some(Lit::Token(t)) => theme
                    .get(t)
                    .color()
                    .unwrap_or_else(|| theme.color(fallback)),
                _ => theme.color(fallback),
            };
            view = match at {
                0 => view.selection_color_when(follow),
                _ => view.selected_text_color_when(follow),
            };
        }
    }
    match &node.pointer_events {
        Some(Source::Lit(Lit::Bool(enabled))) => view = view.pointer_events(*enabled),
        Some(source) => {
            // Followed: a state or a function (`"none"`, `"auto"` or a boolean).
            let (source, vm) = (resolve_owned(source), vm.clone());
            view = view.pointer_events_when(move || {
                let value = match &source {
                    Owned::State(s) => s.with(|v| v.clone()),
                    Owned::Computed(f) => match vm.call::<Value>(f, ()) {
                        Ok(value) => value,
                        Err(error) => {
                            vm.report(error);
                            return true;
                        }
                    },
                    Owned::Lit(Lit::Bool(enabled)) => return *enabled,
                    _ => return true,
                };
                crate::node::pointer_events(&value).unwrap_or(true)
            });
        }
        None => {}
    }
    if let Some(handler) = &node.events().click {
        let (handler, vm, actions) = (handler.clone(), vm.clone(), low.actions.clone());
        // Told the keys held: `{ shift, ctrl, alt, meta }` (a ⌘-click).
        view = view.on_click_with(move |held| {
            let keys = modifiers_table(&vm.lua, held).ok();
            run(&handler, &vm, &actions, keys);
        });
    }
    // The node's handle, for local pointer coordinates and elements: made
    // only for what reads it.
    let events = node.events();
    let handle: Option<Rc<OnceCell<NodeHandle>>> = (element.is_some()
        || events.any_input()
        || events.any_drag()
        || events.drag_preview.is_some())
    .then(Rc::default);
    let handle_for = || handle.clone().expect("made for what reads it");
    if events.any_input() {
        view = with_events(view, node, vm, low, handle_for());
    }
    if events.any_drag() || events.drag_preview.is_some() {
        view = with_drag(view, node, vm, low, handle_for());
    }
    if let Some(style) = &node.events().context_menu_style {
        view = view.context_menu_style(apply_all(Styles::new(), style));
    }
    if node.events().context_menu_below {
        view = view.context_menu_below(true);
    }
    if let Some(build) = node.events().context_menu.clone() {
        let (vm, low) = (vm.clone(), low.clone());
        view = view.context_menu(move |e| {
            let point = vm.lua.create_table().and_then(|t| {
                t.set("x", e.x)?;
                t.set("y", e.y)?;
                t.set("local_x", e.local_x)?;
                t.set("local_y", e.local_y)?;
                Ok(t)
            });
            let items = point.and_then(|p| {
                vm.runtime
                    .clone()
                    .batch(|| vm.call::<Option<mlua::Table>>(&build, p))
            });
            match items {
                Ok(Some(items)) => items
                    .sequence_values::<mlua::AnyUserData>()
                    .filter_map(|item| {
                        match item.and_then(|ud| ud.borrow::<NodeUd>().map(|n| n.0.clone())) {
                            Ok(node) => Some(lower(&node, &low)),
                            Err(error) => {
                                vm.report(error);
                                None
                            }
                        }
                    })
                    .collect(),
                Ok(None) => Vec::new(),
                Err(error) => {
                    vm.report(error);
                    Vec::new()
                }
            }
        });
    }
    // Listeners scripts add later (a draggable) receive the element's events.
    if let Some(element) = &element {
        let element = element.clone();
        view = view.on_event(move |event| {
            if element.listeners.borrow().is_empty() {
                return;
            }
            let listeners = element.listeners.borrow().clone();
            for listener in listeners {
                listener(event);
            }
        });
    }
    // Input and drag handlers read the node's handle (local coordinates).
    if element.is_some() || node.events().any_input() || node.events().any_drag() {
        let (element, elements, refs) = (element.clone(), low.elements.clone(), node.refs.clone());
        let targetable = node.targetable();
        // Moves are measured against the parent; with a size box, against
        // the box's parent.
        let track = node.tracks_layout().then(|| {
            let timing = crate::motion::Timing {
                ease: node.layout.as_ref().and_then(|l| l.ease.clone()),
                duration: node.layout.as_ref().and_then(|l| l.duration),
            };
            let depth = if node.sizes_layout() { 2 } else { 0 };
            (node.layout_id.clone(), timing, depth)
        });
        let (mount, resize, visible, vm) = (
            node.events().mount.clone(),
            node.events().resize.clone(),
            node.events().visible.clone(),
            vm.clone(),
        );
        let handle = handle_for();
        view = view.on_mount(move |mounted| {
            let _ = handle.set(mounted.clone());
            let Some(element) = element else { return };
            if targetable {
                elements.register(&element, mounted.clone());
            } else {
                element.attach(mounted.clone());
            }
            if let Some((layout_id, timing, depth)) = track {
                crate::motion::track(&element, &mounted, layout_id, timing, depth, &vm);
            }
            for r in &refs {
                r.attach(&element, &mounted);
            }
            if let Some(f) = mount {
                let arg = vm.lua.create_userdata(ElementUd(element.clone()));
                if let Err(e) = arg.and_then(|arg| vm.call::<()>(&f, arg)) {
                    vm.report(e);
                }
            }
            // Observers only for handlers: each costs every frame that moves anything.
            if let Some(f) = resize.clone()
                && let Some(sizes) = mounted.observe_size()
            {
                // Size changes only: moving does not report.
                let (vm, element, runtime) = (vm.clone(), element.clone(), vm.runtime.clone());
                let last = std::cell::Cell::new(None::<(f32, f32)>);
                let effect = runtime.effect(move || {
                    let size = sizes.get();
                    // Nothing to report before the first layout.
                    if last.get().is_none() && size == (0., 0.) {
                        return;
                    }
                    if last.replace(Some(size)) == Some(size) {
                        return;
                    }
                    let (w, h) = size;
                    let call = || -> mlua::Result<()> {
                        let size = vm.lua.create_table()?;
                        size.set("w", w)?;
                        size.set("h", h)?;
                        let el = vm.lua.create_userdata(ElementUd(element.clone()))?;
                        vm.call::<()>(&f, (size, el))
                    };
                    if let Err(e) = vm.runtime.untracked(call) {
                        vm.report(e);
                    }
                });
                mounted.retain(effect);
            }
            if let Some(f) = visible.clone()
                && let Some(shown) = mounted.observe_visibility()
            {
                // Changes only, and the first layout's answer, delivered
                // after the frame as an IntersectionObserver does: what the
                // handler builds or drops lays out next frame, never feeding
                // back into the layout that reported it.
                let timers = crate::node::env(&vm.lua).ok().map(|env| env.timers.clone());
                let (vm, element, runtime) = (vm.clone(), element.clone(), vm.runtime.clone());
                let sizes = mounted.observe_size();
                let latest = Rc::new(std::cell::Cell::new(None::<bool>));
                let told = Rc::new(std::cell::Cell::new(None::<bool>));
                let pending: Rc<std::cell::RefCell<Option<crate::timers::Stop>>> = Rc::default();
                let deliver = {
                    let (latest, told, pending) = (latest.clone(), told.clone(), pending.clone());
                    move || {
                        pending.borrow_mut().take();
                        let Some(on) = latest.get() else { return };
                        if told.replace(Some(on)) == Some(on) {
                            return;
                        }
                        let call = || -> mlua::Result<()> {
                            let el = vm.lua.create_userdata(ElementUd(element.clone()))?;
                            vm.call::<()>(&f, (on, el))
                        };
                        if let Err(e) = vm.runtime.untracked(call) {
                            vm.report(e);
                        }
                    }
                };
                let deliver = Rc::new(deliver);
                let effect = runtime.effect(move || {
                    let on = shown.get();
                    // Nothing to report before the first layout.
                    if latest.get().is_none() && sizes.as_ref().is_some_and(|s| s.get() == (0., 0.))
                    {
                        return;
                    }
                    latest.set(Some(on));
                    if pending.borrow().is_some() {
                        return;
                    }
                    match timers.as_ref() {
                        Some(timers) => {
                            let deliver = deliver.clone();
                            let stop = timers.start(
                                std::time::Duration::from_millis(1),
                                false,
                                move || {
                                    deliver();
                                    false
                                },
                            );
                            *pending.borrow_mut() = Some(stop);
                        }
                        None => deliver(),
                    }
                });
                mounted.retain(effect);
            }
        });
    }
    if let Some(id) = &node.id {
        view = view.id(id.to_string());
    }
    if let Some(label) = &node.label {
        view = view.accessible_label(label.to_string());
    }
    if let Some(a11y) = &node.a11y {
        if let Some(role) = a11y.role {
            view = view.role(role);
        }
        if let Some(level) = a11y.level {
            view = view.heading(level);
        }
        if let Some(selected) = &a11y.selected {
            let (selected, vm) = (resolve_owned(selected), vm.clone());
            view = view.selected_when(move || {
                resolve(&selected, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
            });
        }
        if let Some(checked) = &a11y.checked {
            let (checked, vm) = (resolve_owned(checked), vm.clone());
            view = view.checked_when(move || {
                resolve(&checked, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
            });
        }
        if let Some(expanded) = &a11y.expanded {
            let (expanded, vm) = (resolve_owned(expanded), vm.clone());
            view = view.expanded_when(move || {
                resolve(&expanded, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
            });
        }
        if let Some(live) = a11y.live {
            view = view.live(live);
        }
        if let Some(description) = &a11y.description {
            view = view.accessible_description(description.to_string());
        }
        if a11y.hidden {
            view = view.accessible_hidden();
        }
        if let Some(axis) = a11y.focus_group {
            view = view.focus_group(axis);
        }
        if let Some(label) = &a11y.label {
            let (label, vm) = (resolve_owned(label), vm.clone());
            view = view.accessible_label_when(move || match resolve(&label, Ty::Str, None, &vm) {
                Some(Lit::Str(s)) => s.to_string(),
                _ => String::new(),
            });
        }
    }
    if let Some(layout) = node.layout.as_ref().filter(|_| node.sizes_layout()) {
        view = layout_box(view, layout, vm, low);
    }
    view
}

/// How a container lays out its children, for the regions among them that
/// hold any number (`each`, `slot`): a region is a box of its own, and lays
/// its children out as its parent would, stretched across the parent's
/// cross axis in a column. (`show`, `switch` and `presence` hold one view
/// at a time and are contents nodes: CSS `display: contents`, exactly.)
fn flow(node: &Node) -> Vec<(Prop, Lit)> {
    let mut row = matches!(node.kind, Kind::Row);
    let mut out = Vec::new();
    for (prop, source) in &node.props {
        let Source::Lit(lit) = source else { continue };
        match prop {
            Prop::Direction => {
                row = matches!(lit, Lit::Str(v) if v.starts_with("row"));
                out.push((*prop, lit.clone()));
            }
            Prop::Items | Prop::Justify | Prop::Gap | Prop::GapX | Prop::GapY | Prop::FlexWrap => {
                out.push((*prop, lit.clone()));
            }
            _ => {}
        }
    }
    if row {
        // In a row the region takes the row's room, so its children wrap
        // and shrink against the row's width, not their own.
        out.insert(0, (Prop::Direction, Lit::Str("row".into())));
        out.push((Prop::Grow, Lit::Num(1.)));
        out.push((Prop::Shrink, Lit::Num(1.)));
        out.push((Prop::MinW, Lit::Len(zgui::layout::Length::Px(0.))));
    } else {
        out.push((Prop::AlignSelf, Lit::Str("stretch".into())));
    }
    out
}

/// Resolves an element's animated values over `lits`: what an animation
/// writes wins, as the inline styles anime.js writes do.
pub(crate) fn with_animated(element: &Element, mut lits: Vec<(Prop, Lit)>) -> Styles {
    element.revision.get();
    let animated = element.animated.borrow();
    if animated.styles.iter().any(|(p, _)| is_corner(*p)) {
        // A single animated corner keeps the node's other corners.
        lits.extend(
            element
                .base_corners()
                .map(|c| (Prop::Corners, Lit::Corners(c))),
        );
    }
    for (prop, lit) in &animated.styles {
        lits.retain(|(p, _)| p != prop);
        lits.push((*prop, lit.clone()));
    }
    let mut styles = apply_all(Styles::new(), &lits);
    if let Some((x, y)) = element.shown_translate(literal_transform(&lits).0) {
        styles = styles.translate(x, y);
    }
    if let Some(opacity) = animated.opacity {
        styles = styles.opacity(opacity);
    }
    if let Some(blur) = animated.blur {
        styles = styles.filter_blur(blur);
    }
    if let Some([brightness, contrast, saturate]) = animated.color {
        styles = styles
            .brightness(brightness)
            .contrast(contrast)
            .saturate(saturate);
    }
    if let Some((x, y)) = animated.mask {
        styles = styles.mask_offset(x, y);
    }
    if let Some([t, r, b, l]) = animated.clip {
        styles = styles.clip_inset(t, r, b, l);
    }
    if let Some(uniforms) = animated.shader {
        styles = styles.shader_uniforms(uniforms);
    }
    if let Some(local) = animated.local {
        styles = crate::transform::style(styles, local);
    }
    styles
}

/// A view carrying `element`'s animated values, registered once mounted.
fn element_view(
    view: View,
    element: Rc<Element>,
    low: &Rc<Lowering>,
    mounted: impl FnOnce(NodeHandle) + 'static,
) -> View {
    let (style_element, elements) = (element.clone(), low.elements.clone());
    view.reactive_style(move || with_animated(&style_element, Vec::new()))
        .on_mount(move |handle| {
            elements.register(&element, handle.clone());
            mounted(handle);
        })
}

/// The springy default for size changes, as for moves.
fn layout_ease(layout: &LayoutAnim) -> (crate::anime::ease::Ease, f64) {
    crate::motion::Timing {
        ease: layout.ease.clone(),
        duration: layout.duration,
    }
    .resolve()
}

/// Wraps `inner` in a box whose size follows the inner content's, animated.
/// The box is a contents node: `inner` lays out exactly as it would without
/// it, at its new size at once (its percentages, `grow` and the scroll views
/// inside it included), and what animates is the space it takes, as a margin
/// of the box's own (the lab's technique): a negative bottom (or right)
/// margin holds the space at its last size and eases to zero. The content's
/// constraints never change, so a frame of the animation lays out only the
/// box and its ancestors. Nothing is clipped unless `layout_clip` asks: then
/// a clip inset follows the margin, cutting the content to the space.
fn layout_box(inner: View, layout: &LayoutAnim, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    let slack = Element::new(&low.runtime, None, Rc::new([]), "layout");
    let (enter, grow_h, grow_w, clip) = (layout.enter, layout.height, layout.width, layout.clip);
    let (layout, target, vm2) = (layout.clone(), slack.clone(), Rc::downgrade(vm));
    let engine = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .map(|env| env.engine.clone());
    let inner = inner.on_mount(move |handle| {
        let (Some(sizes), Some(engine)) = (handle.observe_size(), engine) else {
            return;
        };
        let last: std::cell::Cell<Option<(f32, f32)>> = Default::default();
        let runtime = engine.runtime.clone();
        let effect = runtime.effect(move || {
            let size = sizes.get();
            if size.0 <= 0. && size.1 <= 0. && last.get().is_none() {
                return;
            }
            let previous = last.replace(Some(size));
            // The margin now: what the box shows beyond (or short of) the
            // content's last size.
            let current = |prop| {
                target
                    .animated
                    .borrow()
                    .styles
                    .iter()
                    .find(|(p, _)| *p == prop)
                    .and_then(|(_, lit)| match lit {
                        Lit::Len(zgui::layout::Length::Px(v)) => Some(*v),
                        Lit::Num(v) => Some(*v),
                        _ => None,
                    })
            };
            let element = Target::Element(target.clone());
            let mut from = Vec::new();
            let mut to = Vec::new();
            let mut snap = Vec::new();
            // A height-only box rewrapped by its container (a panel opening,
            // a sidebar dragged) takes its new height at once rather than
            // easing to it, which would slide what follows the box. That is a
            // width change the height answers the other way (narrower and
            // taller, wider and no taller); content that grows (a bubble
            // fitting its text) widens and lengthens it together, and eases.
            let reflowed = grow_h
                && !grow_w
                && previous.is_some_and(|(width, height)| {
                    let (dw, dh) = (size.0 - width, size.1 - height);
                    dw.abs() > 0.5 && dw * dh <= 0.
                });
            for (on, prop, side, now, before) in [
                (
                    grow_h,
                    Prop::Mb,
                    Property::ClipBottom,
                    size.1,
                    previous.map(|p| p.1),
                ),
                (
                    grow_w,
                    Prop::Mr,
                    Property::ClipRight,
                    size.0,
                    previous.map(|p| p.0),
                ),
            ] {
                if !on {
                    continue;
                }
                if reflowed && prop == Prop::Mb {
                    snap.push((element.clone(), Property::Style(prop), V::Num(0.), None));
                    if clip {
                        snap.push((element.clone(), side, V::Num(0.), None));
                    }
                    continue;
                }
                let shown = match before {
                    // Entering grows from nothing; a first size otherwise holds.
                    None if enter => 0.,
                    None => continue,
                    Some(before) if before == now => continue,
                    Some(before) => before + current(prop).unwrap_or(0.),
                };
                let margin = (shown - now) as f64;
                from.push((element.clone(), Property::Style(prop), V::Num(margin), None));
                to.push((element.clone(), Property::Style(prop), V::Num(0.)));
                if clip {
                    from.push((element.clone(), side.clone(), V::Num(-margin), None));
                    to.push((element.clone(), side, V::Num(0.)));
                }
            }
            if !snap.is_empty() {
                engine.remove_targets(std::slice::from_ref(&element));
                engine.set_now(snap);
            }
            if to.is_empty() {
                return;
            }
            engine.set_now(from);
            let (ease, duration) = layout_ease(&layout);
            engine.tween(vm2.clone(), to, duration, ease, Default::default());
        });
        handle.retain(effect);
    });
    let mut space = column().contents().child(inner);
    // Closed until the content's first size arrives: no frame at full size.
    let closed = |on: bool| if enter && on { 100_000. } else { 0. };
    if enter && grow_h {
        space = space.mb(-100_000.);
    }
    if enter && grow_w {
        space = space.mr(-100_000.);
    }
    if clip {
        space = space.clip_inset(0., closed(grow_w), closed(grow_h), 0.);
    }
    element_view(space, slack, low, |_| {})
}

/// Properties set together: a dynamic one carries the literal rest along.
const TRANSFORM_GROUPS: &[&[Prop]] = &[
    &[Prop::X, Prop::Y],
    &[
        Prop::Scale,
        Prop::ScaleX,
        Prop::ScaleY,
        Prop::Rotate,
        Prop::Skew,
        Prop::SkewX,
        Prop::SkewY,
        Prop::Origin,
    ],
];

fn placeholder() -> Arc<ImageData> {
    thread_local! {
        static EMPTY: Arc<ImageData> = Arc::new(ImageData::new(1, 1, vec![0, 0, 0, 0]).expect("1×1 image"));
    }
    EMPTY.with(Clone::clone)
}
fn resolve_path(vm: &Vm, path: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(path);
    match (&vm.base, path.is_relative()) {
        (Some(base), true) => base.join(path),
        _ => path.to_path_buf(),
    }
}
fn is_remote(src: &str) -> bool {
    src.starts_with("https://") || src.starts_with("http://")
}
/// What a remote source decodes to, on a worker thread.
enum Decoded {
    Image(Arc<ImageData>),
    Svg(Arc<SvgData>),
}
const LOADING: u8 = 0;
const READY: u8 = 1;
const FAILED: u8 = 2;
/// Whether remote `src` is in the caches: the first ask fetches it (once,
/// through the host's fetcher), decodes it off the main thread and caches
/// it. Tracked: what asked shows it once it arrives.
fn remote_ready(vm: &Rc<Vm>, low: &Lowering, src: &str, svg: bool) -> bool {
    remote_status(vm, low, src, svg) == READY
}
/// Remote `src`'s status (`LOADING`, `READY`, `FAILED`), fetching it the
/// first time. Tracked.
fn remote_status(vm: &Rc<Vm>, low: &Lowering, src: &str, svg: bool) -> u8 {
    let existing = low.assets.remote.borrow().get(src).cloned();
    let ready = match existing {
        Some(status) if status.get() != FAILED => return status.get(),
        Some(status) => {
            // 5 s, doubling each time in a row it fails, up to 5 minutes.
            let failed = low.assets.failed.borrow().get(src).copied();
            let wait = |times: u32| {
                std::time::Duration::from_secs((5u64 << times.saturating_sub(1).min(6)).min(300))
            };
            if failed.is_some_and(|(at, times)| at.elapsed() < wait(times)) {
                return FAILED;
            }
            status.set(LOADING);
            status
        }
        None => {
            let ready = low.runtime.signal(LOADING);
            low.assets
                .remote
                .borrow_mut()
                .insert(src.to_owned(), ready.clone());
            ready
        }
    };
    let links = low.assets.links.borrow().clone();
    let fetch = links
        .as_ref()
        .and_then(|(_, fetcher)| fetcher.borrow().clone());
    let (Some((timers, _)), Some(fetch)) = (links, fetch) else {
        vm.report(mlua::Error::runtime(format!(
            "{src}: remote sources need the host's fetcher (`ScriptHost::fetcher`)"
        )));
        return ready.get();
    };
    let future = fetch(crate::calls::Request {
        url: src.to_owned(),
        method: "GET".into(),
        headers: Vec::new(),
        body: None,
    });
    let (assets, vm, src, runtime) = (
        low.assets.clone(),
        Rc::downgrade(vm),
        src.to_owned(),
        low.runtime.clone(),
    );
    let arriving = ready.clone();
    timers.spawn(async move {
        let bytes = match future.await {
            Ok(response) if (200..300).contains(&response.status) => Ok(response.body),
            Ok(response) => Err(format!("HTTP {}", response.status)),
            Err(error) => Err(error),
        };
        let decoded = match bytes {
            Ok(bytes) => {
                // Decoding a large image takes long enough to drop frames.
                let (tell, mut heard) = zgui::task::latest::<Result<Decoded, String>>();
                std::thread::spawn(move || {
                    tell.send(if svg {
                        SvgData::new(bytes)
                            .map(|svg| Decoded::Svg(Arc::new(svg)))
                            .map_err(|e| e.to_string())
                    } else {
                        zgui_gpu::assets::decode_image(&bytes)
                            .map(Decoded::Image)
                            .map_err(|e| e.to_string())
                    });
                });
                heard
                    .recv()
                    .await
                    .unwrap_or_else(|| Err("the decoder stopped".into()))
            }
            Err(error) => Err(error),
        };
        match decoded {
            Ok(Decoded::Image(image)) => {
                assets.failed.borrow_mut().remove(&src);
                assets.images.borrow_mut().insert(src, image);
            }
            Ok(Decoded::Svg(svg)) => {
                assets.failed.borrow_mut().remove(&src);
                assets.svgs.borrow_mut().insert(src, svg);
            }
            Err(error) => {
                // A picture that doesn't come is the network's doing, not
                // the script's: logged, and `fallback` shows in its place.
                let _ = vm;
                eprintln!("zgui: {src} didn't load: {error}");
                let now = std::time::Instant::now();
                assets
                    .failed
                    .borrow_mut()
                    .entry(src)
                    .and_modify(|(at, times)| (*at, *times) = (now, *times + 1))
                    .or_insert((now, 1));
                runtime.batch(|| arriving.set(FAILED));
                return;
            }
        }
        runtime.batch(|| arriving.set(READY));
    });
    ready.get()
}
fn load_image(vm: &Rc<Vm>, low: &Lowering, src: &str) -> Option<Arc<ImageData>> {
    // The host's own images, followed as it replaces them.
    if src.starts_with("host:") {
        low.assets.hosted(src, &low.runtime).get();
        return low.assets.images.borrow().get(src).cloned();
    }
    if let Some(image) = low.assets.images.borrow().get(src) {
        return Some(image.clone());
    }
    if is_remote(src) {
        remote_ready(vm, low, src, false);
        return None;
    }
    let path = resolve_path(vm, src);
    let decoded = low
        .assets
        .read(src, &path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .and_then(|bytes| {
            zgui_gpu::assets::decode_image(&bytes).map_err(|e| format!("{}: {e}", path.display()))
        });
    match decoded {
        Ok(image) => {
            low.assets
                .images
                .borrow_mut()
                .insert(src.to_owned(), image.clone());
            Some(image)
        }
        Err(e) => {
            vm.report(mlua::Error::runtime(e));
            None
        }
    }
}
fn load_svg(vm: &Rc<Vm>, low: &Lowering, src: &str) -> Arc<SvgData> {
    if let Some(svg) = low.assets.svgs.borrow().get(src) {
        return svg.clone();
    }
    if is_remote(src) {
        remote_ready(vm, low, src, true);
        thread_local! {
            static NOTHING: Arc<SvgData> = Arc::new(
                SvgData::new(&b"<svg xmlns='http://www.w3.org/2000/svg'/>"[..]).expect("empty svg"),
            );
        }
        return NOTHING.with(Clone::clone);
    }
    let bytes = match src.strip_prefix("code:") {
        Some(code) => Ok(code.as_bytes().to_vec()),
        None => {
            let path = resolve_path(vm, src);
            low.assets
                .read(src, &path)
                .map_err(|e| format!("{}: {e}", path.display()))
        }
    };
    let parsed = bytes.and_then(|b| SvgData::new(b).map_err(|e| format!("{src}: {e}")));
    match parsed {
        Ok(svg) => {
            let svg = Arc::new(svg);
            low.assets
                .svgs
                .borrow_mut()
                .insert(src.to_owned(), svg.clone());
            svg
        }
        Err(e) => {
            vm.report(mlua::Error::runtime(e));
            Arc::new(
                SvgData::new(&b"<svg xmlns='http://www.w3.org/2000/svg'/>"[..]).expect("empty svg"),
            )
        }
    }
}

/// The modifier keys held, as a script sees them.
fn modifiers_table(lua: &mlua::Lua, held: zgui::input::Modifiers) -> mlua::Result<mlua::Table> {
    let t = lua.create_table()?;
    t.set("shift", held.shift)?;
    t.set("ctrl", held.control)?;
    t.set("alt", held.alt)?;
    t.set("meta", held.meta)?;
    t.set("cmd", held.primary_shortcut())?;
    Ok(t)
}

fn key_name(key: &Key) -> String {
    match key {
        Key::Character(c) => c.clone(),
        Key::Space => " ".into(),
        Key::Function(n) => format!("F{n}"),
        other => format!("{other:?}"),
    }
}

/// Input handlers: one event listener dispatches to the script's handlers.
fn with_events(
    view: View,
    node: &Node,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    handle: Rc<OnceCell<NodeHandle>>,
) -> View {
    let events = node.events();
    let (press, release, hover, moved) = (
        events.press.clone(),
        events.release.clone(),
        events.hover.clone(),
        events.pointer_move.clone(),
    );
    let (key, key_up, focus, blur, wheel, pinch) = (
        events.key.clone(),
        events.key_up.clone(),
        events.focus.clone(),
        events.blur.clone(),
        events.wheel.clone(),
        events.pinch.clone(),
    );
    // A press that follows the pointer (a divider, a slider of the
    // script's own) keeps it until release, wherever it goes.
    let captures = press.is_some() && (moved.is_some() || release.is_some());
    let (vm, actions) = (vm.clone(), low.actions.clone());
    view.on_event(move |event| {
        let lua = &vm.lua;
        let local = |x: f32, y: f32| {
            let origin = handle
                .get()
                .and_then(|h| h.scene().map(|s| s.borrow().bounds(h.node())))
                .map_or((0., 0.), |b| (b.x, b.y));
            (x - origin.0, y - origin.1)
        };
        let held = event.pointer_modifiers();
        let point = |x: f32, y: f32| -> mlua::Result<mlua::Table> {
            let (lx, ly) = local(x, y);
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            t.set("local_x", lx)?;
            t.set("local_y", ly)?;
            t.set("shift", held.shift)?;
            t.set("ctrl", held.control)?;
            t.set("alt", held.alt)?;
            t.set("meta", held.meta)?;
            t.set("cmd", held.primary_shortcut())?;
            Ok(t)
        };
        if captures
            && event.phase == EventPhase::Target
            && matches!(
                event.event,
                InputEvent::PointerDown {
                    button: PointerButton::Primary,
                    ..
                }
            )
        {
            event.capture_pointer();
        }
        // A child's events reach this node going down and coming back up:
        // the handlers hear them once, as they bubble.
        if event.phase == EventPhase::Capture {
            return;
        }
        match &event.event {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                if let (Some(h), Ok(p)) = (&press, point(*x, *y)) {
                    // The press's count: 2 for a double click's second.
                    let _ = p.set("clicks", event.click_count());
                    run(h, &vm, &actions, p);
                }
            }
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                if let (Some(h), Ok(p)) = (&release, point(*x, *y)) {
                    run(h, &vm, &actions, p);
                }
            }
            InputEvent::PointerMove { x, y } => {
                if let (Some(h), Ok(p)) = (&moved, point(*x, *y)) {
                    run(h, &vm, &actions, p);
                }
            }
            InputEvent::PointerEnter => {
                if let Some(h) = &hover {
                    run(h, &vm, &actions, true);
                }
            }
            InputEvent::PointerLeave => {
                if let Some(h) = &hover {
                    run(h, &vm, &actions, false);
                }
            }
            InputEvent::KeyDown {
                key: k,
                modifiers,
                repeat,
            } => {
                if let Some(h) = &key {
                    let table = lua.create_table().and_then(|t| {
                        t.set("key", key_name(k))?;
                        t.set("shift", modifiers.shift)?;
                        t.set("ctrl", modifiers.control)?;
                        t.set("alt", modifiers.alt)?;
                        t.set("meta", modifiers.meta)?;
                        t.set("cmd", modifiers.primary_shortcut())?;
                        t.set("repeat", *repeat)?;
                        t.set("code", event.key_code())?;
                        t.set("editing", handle.get().is_some_and(|h| h.editing()))?;
                        Ok(t)
                    });
                    if let Ok(t) = table
                        && run(h, &vm, &actions, t).is_some_and(|v| v.as_boolean() == Some(true))
                    {
                        event.prevent_default();
                        event.stop_propagation();
                    }
                }
            }
            InputEvent::KeyUp { key: k, .. } => {
                if let Some(h) = &key_up {
                    run(h, &vm, &actions, key_name(k));
                }
            }
            InputEvent::Focus => {
                if let Some(h) = &focus {
                    run(h, &vm, &actions, ());
                }
            }
            InputEvent::Blur => {
                if let Some(h) = &blur {
                    run(h, &vm, &actions, ());
                }
            }
            // Where, and the keys held: a canvas zooms about the pointer
            // (⌘ or Ctrl with the wheel). `true` back takes the gesture
            // from the scroll views around it.
            InputEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                if let Some(h) = &wheel
                    && let Ok(t) = point(*x, *y)
                    && t.set("dx", *delta_x)
                        .and_then(|_| t.set("dy", *delta_y))
                        .is_ok()
                    && run(h, &vm, &actions, t).is_some_and(|v| v.as_boolean() == Some(true))
                {
                    event.prevent_default();
                    event.stop_propagation();
                }
            }
            InputEvent::Pinch { x, y, delta } => {
                if let Some(h) = &pinch
                    && let Ok(t) = point(*x, *y)
                    && t.set("scale", *delta).is_ok()
                    && run(h, &vm, &actions, t).is_some_and(|v| v.as_boolean() == Some(true))
                {
                    event.prevent_default();
                    event.stop_propagation();
                }
            }
            _ => {}
        }
    })
}

/// Drag sources and destinations; payloads are script values.
fn with_drag(
    mut view: View,
    node: &Node,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    handle: Rc<OnceCell<NodeHandle>>,
) -> View {
    use zgui::input::DragPhase;
    let events = node.events();
    fn call(vm: &Rc<Vm>, f: &mlua::Function, args: impl mlua::IntoLuaMulti) -> Option<mlua::Value> {
        vm.runtime
            .clone()
            .batch(|| match vm.call::<mlua::Value>(f, args) {
                Ok(value) => Some(value),
                Err(error) => {
                    vm.report(error);
                    None
                }
            })
    }
    if let Some(drag) = events.drag.clone() {
        let vm = vm.clone();
        view = view.on_drag(move || -> Value {
            call(&vm, &drag, ())
                .and_then(|v| Value::from_lua(v, &vm.lua).ok())
                .unwrap_or(Value::Nil)
        });
    }
    if let Some(preview) = events.drag_preview.clone() {
        let (vm, low) = (vm.clone(), low.clone());
        view = view.drag_preview(move |payload: &Value| {
            let result = vm.call::<mlua::Value>(&preview, payload.clone());
            built(&vm, result, &low)
        });
    }
    if let Some(end) = events.drag_end.clone() {
        let vm = vm.clone();
        view = view.on_drag_end(move |payload: &Value, accepted| {
            call(&vm, &end, (payload.clone(), accepted));
        });
    }
    if let Some(export) = events.drag_export.clone() {
        let vm = vm.clone();
        view = view.drag_export(move |payload: &Value| {
            use zgui::input::DragItem;
            let Some(mlua::Value::Table(items)) = call(&vm, &export, payload.clone()) else {
                return Vec::new();
            };
            let text = |key: &str| items.get::<Option<String>>(key).ok().flatten();
            let mut out = Vec::new();
            out.extend(text("text").map(DragItem::Text));
            out.extend(text("url").map(DragItem::Url));
            if let Ok(Some(files)) = items.get::<Option<Vec<String>>>("files") {
                out.extend(files.into_iter().map(|file| DragItem::File(file.into())));
            }
            out
        });
    }
    if let Some(outside) = events.drag_outside.clone() {
        let vm = vm.clone();
        view = view.on_drag_outside(move |payload: &Value, event| {
            let at = vm.lua.create_table().ok();
            if let Some(at) = &at {
                for (key, value) in [
                    ("x", event.x),
                    ("y", event.y),
                    ("grab_x", event.grab_x),
                    ("grab_y", event.grab_y),
                ] {
                    let _ = at.set(key, value);
                }
            }
            call(&vm, &outside, (payload.clone(), at));
        });
    }
    if let Some(moves) = events.drag_moves_window.clone() {
        let vm = vm.clone();
        view = view.drag_moves_window(move |payload: &Value| {
            call(&vm, &moves, payload.clone()).is_some_and(|moves| {
                !matches!(moves, mlua::Value::Nil | mlua::Value::Boolean(false))
            })
        });
    }
    let (drop, accept, over, leave) = (
        events.drop.clone(),
        events.drop_when.clone(),
        events.drag_over.clone(),
        events.drag_leave.clone(),
    );
    if drop.is_none() && over.is_none() && leave.is_none() {
        return view;
    }
    let vm = vm.clone();
    view.on_event(move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        let InputEvent::Drag(event) = cx.event.clone() else {
            return;
        };
        let Some(payload) = event.payload.downcast_ref::<Value>() else {
            return;
        };
        let accepts = || {
            accept.as_ref().is_none_or(|f| {
                call(&vm, f, payload.clone())
                    .is_some_and(|v| !matches!(v, mlua::Value::Nil | mlua::Value::Boolean(false)))
            })
        };
        let point = || -> Option<mlua::Table> {
            let bounds = handle
                .get()
                .and_then(|h| h.scene().map(|s| s.borrow().bounds(h.node())))?;
            let t = vm.lua.create_table().ok()?;
            t.set("x", event.x).ok()?;
            t.set("y", event.y).ok()?;
            t.set("local_x", event.x - bounds.x).ok()?;
            t.set("local_y", event.y - bounds.y).ok()?;
            t.set("w", bounds.width).ok()?;
            t.set("h", bounds.height).ok()?;
            Some(t)
        };
        match event.phase {
            DragPhase::Over if accepts() => {
                cx.accept_drag();
                // The nearest accepting destination is the one that shows it.
                cx.stop_propagation();
                if let (Some(f), Some(p)) = (&over, point()) {
                    call(&vm, f, (payload.clone(), p));
                }
            }
            DragPhase::Leave => {
                if let Some(f) = &leave {
                    call(&vm, f, payload.clone());
                }
            }
            DragPhase::Drop if !cx.default_prevented() && accepts() => {
                if let (Some(f), Some(p)) = (&drop, point()) {
                    call(&vm, f, (payload.clone(), p));
                }
                cx.prevent_default();
                cx.stop_propagation();
            }
            _ => {}
        }
    })
}

/// Runs a handler in one reactive batch; returns what a script returned.
fn run(
    handler: &Handler,
    vm: &Rc<Vm>,
    actions: &Actions,
    args: impl mlua::IntoLuaMulti,
) -> Option<mlua::Value> {
    vm.runtime.clone().batch(|| match handler {
        Handler::Lua(f) => match vm.call::<mlua::Value>(f, args) {
            Ok(v) => Some(v),
            Err(error) => {
                vm.report(error);
                None
            }
        },
        Handler::Action(action) => {
            match action {
                Action::Set(s, v) => {
                    s.set(v.clone());
                }
                Action::Toggle(s) => {
                    s.update(|v| *v = Value::Bool(!v.truthy()));
                }
                Action::Add(s, by) => {
                    s.update(|v| *v = Value::Num(v.as_f64().unwrap_or(0.) + by));
                }
                Action::Host(name, arg) => {
                    let action = actions.borrow().get(&**name).cloned();
                    match action {
                        Some(action) => action(arg.clone()),
                        None => vm.report(mlua::Error::runtime(format!(
                            "no host action named `{name}`"
                        ))),
                    }
                }
            }
            None
        }
    })
}

/// A source with its Lua function or animation detached from the node, so a
/// reactive closure can own it.
#[derive(Clone)]
enum Owned {
    Lit(Lit),
    State(Signal<Value>),
    Computed(mlua::Function),
    Anim(Option<Value>, Option<Value>),
    Variant(Rc<crate::variants::Selection>, Prop),
}
fn resolve_owned(source: &Source) -> Owned {
    match source {
        Source::Lit(l) => Owned::Lit(l.clone()),
        Source::State(s) => Owned::State(s.clone()),
        Source::Computed(f) => Owned::Computed(f.clone()),
        Source::Anim(a) => Owned::Anim(a.from.clone(), a.to.clone()),
        Source::Variant(selection, prop) => Owned::Variant(selection.clone(), *prop),
    }
}
/// A style's value now. What a function returns is checked by the design
/// lint, unless it is `ui.raw`.
fn resolve_style(source: &Owned, prop: Prop, scalar: Option<&Signal<f32>>, vm: &Vm) -> Option<Lit> {
    if let Owned::Computed(f) = source
        && crate::design::mode(&vm.lua) != crate::DesignLint::Off
    {
        let value = vm
            .call::<mlua::Value>(f, ())
            .map_err(|e| vm.report(e))
            .ok()?;
        let raw = crate::design::is_raw(&value);
        let value = vm.lua.unpack::<Value>(value).ok()?;
        let lit = lit(&value, prop.ty())?;
        if !raw {
            let name = prop.name();
            if let Err(message) = crate::design::check_returned(&vm.lua, f, prop, name, &lit) {
                vm.report(mlua::Error::runtime(message));
                return None;
            }
        }
        return Some(lit);
    }
    resolve(source, prop.ty(), scalar, vm)
}
fn resolve(source: &Owned, ty: Ty, scalar: Option<&Signal<f32>>, vm: &Vm) -> Option<Lit> {
    match source {
        Owned::Lit(l) => Some(l.clone()),
        Owned::Variant(selection, prop) => selection
            .current()
            .props
            .iter()
            .find_map(|(p, l)| (p == prop).then(|| l.clone())),
        Owned::State(s) => s.with(|v| lit(v, ty)),
        Owned::Computed(f) => vm
            .call::<Value>(f, ())
            .map_err(|e| vm.report(e))
            .ok()
            .and_then(|v| lit(&v, ty)),
        Owned::Anim(from, to) => {
            let t = scalar?.get();
            match (from, to) {
                (Some(a), Some(b)) => {
                    let (a, b) = (lit(a, ty)?, lit(b, ty)?);
                    match ty {
                        Ty::Color => mix_theme(&a, &b, t).or_else(|| lerp(&a, &b, t)),
                        _ => lerp(&a, &b, t),
                    }
                }
                _ => Some(Lit::Num(t)),
            }
        }
    }
}
fn resolve_value(source: &Owned, vm: &Vm) -> Value {
    match source {
        Owned::Lit(Lit::Str(s)) => Value::Str(s.clone()),
        Owned::Lit(Lit::Num(n)) => Value::Num(*n as f64),
        Owned::Lit(Lit::Bool(b)) => Value::Bool(*b),
        Owned::State(s) => s.get(),
        Owned::Computed(f) => vm.call::<Value>(f, ()).unwrap_or_else(|e| {
            vm.report(e);
            Value::Nil
        }),
        _ => Value::Nil,
    }
}
/// `t` of the way between two colours, one or both theme tokens: a blend
/// the node resolves from its theme, so it eases between theme colours and
/// follows theme changes.
fn mix_theme(a: &Lit, b: &Lit, t: f32) -> Option<Lit> {
    use zgui::theme::Themed;
    Some(match (a, b) {
        (Lit::Token(a), Lit::Token(b)) => Lit::Mix(*a, Themed::Token(*b), t),
        (Lit::Token(a), Lit::Color(b)) => Lit::Mix(*a, Themed::Value(*b), t),
        (Lit::Color(a), Lit::Token(b)) => Lit::Mix(*b, Themed::Value(*a), 1. - t),
        _ => return None,
    })
}
fn lerp(a: &Lit, b: &Lit, t: f32) -> Option<Lit> {
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Some(match (a, b) {
        (Lit::Num(x), Lit::Num(y)) => Lit::Num(mix(*x, *y)),
        (Lit::Len(Length::Px(x)), Lit::Len(Length::Px(y))) => Lit::Len(Length::Px(mix(*x, *y))),
        (Lit::Pair(x0, y0), Lit::Pair(x1, y1)) => Lit::Pair(mix(*x0, *x1), mix(*y0, *y1)),
        (Lit::Color(c0), Lit::Color(c1)) => {
            let ch = |x: u8, y: u8| mix(x as f32, y as f32).round().clamp(0., 255.) as u8;
            Lit::Color(Color(
                ch(c0.0, c1.0),
                ch(c0.1, c1.1),
                ch(c0.2, c1.2),
                ch(c0.3, c1.3),
            ))
        }
        (_, other) if t >= 0.5 => other.clone(),
        (other, _) => other.clone(),
    })
}

fn render_text(text: &Text, vm: &Vm) -> String {
    match text {
        Text::Literal(s) => s.clone(),
        Text::Template(segments) => {
            let mut out = String::new();
            for segment in segments {
                match segment {
                    Segment::Literal(literal) => out.push_str(literal),
                    Segment::State(state) => state.with(|v| {
                        use std::fmt::Write;
                        let _ = write!(out, "{v}");
                    }),
                }
            }
            out
        }
        Text::Computed(f) => match vm.call::<Value>(f, ()) {
            Ok(v) => v.to_string(),
            Err(error) => {
                vm.report(error);
                "⚠".into()
            }
        },
    }
}
fn lower_text(text: &Text, vm: &Rc<Vm>) -> View {
    match text {
        Text::Literal(s) => zgui::compose::text(s.clone()),
        Text::Template(segments) => {
            let segments: Vec<_> = segments
                .iter()
                .map(|s| match s {
                    Segment::Literal(l) => Segment::Literal(l.clone()),
                    Segment::State(s) => Segment::State(s.clone()),
                })
                .collect();
            let template = Text::Template(segments);
            let vm = vm.clone();
            text_signal(move || render_text(&template, &vm))
        }
        Text::Computed(f) => {
            let (f, vm) = (Text::Computed(f.clone()), vm.clone());
            text_signal(move || render_text(&f, &vm))
        }
    }
}
/// A `ui.image` (out of `build`, as `lower_list` is).
#[inline(never)]
fn lower_image(kind: &Kind, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    let Kind::Image {
        label,
        src,
        fallback,
    } = kind
    else {
        unreachable!("lower_image takes images")
    };
    // The picture, as a view (made again each time it shows).
    let picture = {
        let (src, vm2, low2, label) = (resolve_owned(src), vm.clone(), low.clone(), label.clone());
        move || {
            let (src, vm2, low2) = (src.clone(), vm2.clone(), low2.clone());
            image_signal(label.clone(), move || {
                let path = match resolve(&src, Ty::Str, None, &vm2) {
                    Some(Lit::Str(path)) => path,
                    _ => return placeholder(),
                };
                load_image(&vm2, &low2, &path).unwrap_or_else(placeholder)
            })
        }
    };
    match fallback {
        None => picture(),
        Some(fallback) => {
            // The picture once a remote one has arrived (a local one
            // at once), else the fallback.
            let (src, vm2, low2) = (resolve_owned(src), vm.clone(), low.clone());
            let shown = move || match resolve(&src, Ty::Str, None, &vm2) {
                Some(Lit::Str(path)) if is_remote(&path) => {
                    remote_status(&vm2, &low2, &path, false) == READY
                }
                Some(Lit::Str(_)) => true,
                _ => false,
            };
            let (fallback, low3) = (fallback.clone(), low.clone());
            zgui::compose::switch(shown, move |ready, _| {
                if ready {
                    picture().w_full().h_full()
                } else {
                    lower(&fallback, &low3).w_full().h_full()
                }
            })
        }
    }
}

/// A `ui.list` (kept out of `build`, which recurses: its locals would
/// weigh on every level of a deep interface's stack).
#[inline(never)]
fn lower_list(kind: &Kind, vm: &Rc<Vm>, low: &Rc<Lowering>, cx: &mut Option<&mut Context>) -> View {
    let Kind::List {
        count,
        height,
        heights,
        sticky,
        sticky_top,
        overscan,
        build: builder,
        offset,
        keyboard,
        ..
    } = kind
    else {
        unreachable!("lower_list takes lists")
    };
    let keyboard = *keyboard;
    let mut cx = cx.as_deref_mut();
    let offset = match offset {
        Some(offset) => bridge(need(&mut cx), offset, number, |x| Value::Num(x as f64)),
        None => need(&mut cx).state(0_f32),
    };
    if let Some(heights) = heights {
        let cx = need(&mut cx);
        let rows = zgui::compose::VariableHeights::new(&cx.runtime(), 0, *height);
        // Every height replaced when the list changes: linear, in Rust.
        let (list, heights_vm, r) = (heights.clone(), vm.clone(), rows.clone());
        cx.retain(cx.runtime().effect(move || {
            let value = match &list {
                crate::node::Heights::Lit(v) => v.clone(),
                crate::node::Heights::State(s) => s.get(),
                crate::node::Heights::Computed(f) => heights_vm
                    .call::<Value>(f, ())
                    .map_err(|e| heights_vm.report(e))
                    .unwrap_or_default(),
            };
            let values = value.list().iter().map(|v| match v {
                Value::Num(n) if n.is_finite() && *n > 0. => *n as f32,
                _ => 1.,
            });
            let _ = r.replace(values);
        }));
        let list = {
            let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
            variable_virtual_list(
                offset.clone(),
                rows.clone(),
                *overscan,
                |index| index,
                move |_, index, cx| build_scoped(&vm, cx, &low, &builder, index + 1),
            )
            .keyboard_navigation(keyboard)
        };
        match sticky {
            None => list,
            Some(sticky) => sticky_list(
                list,
                sticky,
                *sticky_top,
                offset,
                rows,
                builder,
                vm,
                low,
                cx,
            ),
        }
    } else {
        let (count, count_vm) = (Rc::new(resolve_owned(count)), vm.clone());
        let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
        virtual_list(
            offset,
            *height,
            *overscan,
            move || match resolve(&count, Ty::Num, None, &count_vm) {
                Some(Lit::Num(n)) => n.max(0.) as usize,
                _ => 0,
            },
            |index| index,
            // Each row owns what it makes, as `each` rows do.
            move |_, index, cx| build_scoped(&vm, cx, &low, &builder, index + 1),
        )
        .keyboard_navigation(keyboard)
    }
}

/// A variable list with sticky rows: the list, and over it the sticky row
/// whose section covers the top of the view (the last one at or above it),
/// built as any row is and pushed up as the next sticky row arrives: CSS's
/// `position: sticky` for section headers, with the rows virtualized.
#[allow(clippy::too_many_arguments)]
fn sticky_list(
    list: View,
    sticky: &crate::node::Heights,
    // How far below the top they stick (CSS's sticky `top`).
    inset: f32,
    offset: Signal<f32>,
    rows: zgui::compose::VariableHeights,
    builder: &mlua::Function,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    cx: &mut Context,
) -> View {
    // The sticky rows, from 0, kept sorted.
    let indices = cx.state(Rc::<[usize]>::from([]));
    {
        let (sticky, vm, indices) = (sticky.clone(), vm.clone(), indices.clone());
        cx.retain(cx.runtime().effect(move || {
            let value = match &sticky {
                crate::node::Heights::Lit(v) => v.clone(),
                crate::node::Heights::State(s) => s.get(),
                crate::node::Heights::Computed(f) => vm
                    .call::<Value>(f, ())
                    .map_err(|e| vm.report(e))
                    .unwrap_or_default(),
            };
            let mut list: Vec<usize> = value
                .list()
                .iter()
                .filter_map(|v| v.as_f64().filter(|n| *n >= 1.).map(|n| n as usize - 1))
                .collect();
            list.sort_unstable();
            list.dedup();
            indices.set(list.into());
        }));
    }
    // The sticky row in charge at the top, if any.
    let current = {
        let (offset, rows, indices) = (offset.clone(), rows.clone(), indices.clone());
        move || -> Option<usize> {
            let top = rows.row_at((offset.get() + inset).max(0.))?;
            indices.with(|list| list.iter().rev().find(|&&i| i <= top).copied())
        }
    };
    // How far the next sticky row pushes it up.
    let shift = {
        let (offset, rows, indices) = (offset.clone(), rows.clone(), indices.clone());
        let current = current.clone();
        move || -> f32 {
            let Some(at) = current() else { return 0. };
            let height = rows.row_height(at).unwrap_or(0.);
            let next = indices.with(|list| list.iter().find(|&&i| i > at).copied());
            match next {
                Some(next) => (rows.row_offset(next) - offset.get() - inset - height).min(0.),
                None => 0.,
            }
        }
    };
    let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
    let header = zgui::compose::switch(current, move |at, cx| match at {
        Some(at) => build_scoped(&vm, cx, &low, &builder, at + 1),
        None => div(),
    });
    overlay().child(list.w_full().h_full()).child(
        column()
            .absolute()
            .top(inset)
            .left(0.)
            .right(0.)
            .child(header)
            .reactive_style(move || zgui::style::Styles::new().translate(0., shift())),
    )
}

fn lower_rich(spans: &[Span], vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    /// A span with its text and handler owned by the closure.
    struct Owned {
        text: Text,
        style: SpanStyle,
        click: Option<Handler>,
        href: Option<Arc<str>>,
        icon: Option<crate::node::SpanIcon>,
    }
    #[derive(Clone, Copy)]
    struct SpanStyle {
        color: Option<zgui::theme::Themed<Color>>,
        background: Option<zgui::theme::Themed<Color>>,
        size: Option<f32>,
        weight: Option<u16>,
        italic: bool,
        underline: bool,
        strike: bool,
        mono: bool,
        padding: [f32; 2],
        radius: f32,
        underline_offset: Option<f32>,
        hover: Option<(bool, Option<zgui::theme::Themed<Color>>)>,
    }
    let owned: Vec<Owned> = spans
        .iter()
        .map(|s| Owned {
            text: match &s.text {
                Text::Literal(l) => Text::Literal(l.clone()),
                Text::Template(segments) => Text::Template(
                    segments
                        .iter()
                        .map(|seg| match seg {
                            Segment::Literal(l) => Segment::Literal(l.clone()),
                            Segment::State(s) => Segment::State(s.clone()),
                        })
                        .collect(),
                ),
                Text::Computed(f) => Text::Computed(f.clone()),
            },
            style: SpanStyle {
                color: s.color,
                background: s.background,
                size: s.size,
                weight: s.weight,
                italic: s.italic,
                underline: s.underline,
                strike: s.strike,
                mono: s.mono,
                padding: s.padding,
                radius: s.radius,
                underline_offset: s.underline_offset,
                hover: s.hover,
            },
            click: s.click.clone(),
            href: s.href.clone(),
            icon: s.icon.clone(),
        })
        .collect();
    let (vm, actions, low) = (vm.clone(), low.actions.clone(), low.clone());
    rich_text_signal(move || {
        owned
            .iter()
            .map(|span| {
                let SpanStyle {
                    color,
                    background,
                    size,
                    weight,
                    italic,
                    underline,
                    strike,
                    mono,
                    padding,
                    radius,
                    underline_offset,
                    hover,
                } = span.style;
                let decoration = || {
                    let d = zgui::rich_text::Decoration::default();
                    match underline_offset {
                        Some(offset) => d.offset(offset),
                        None => d,
                    }
                };
                let mut out: TextSpan = text_span(render_text(&span.text, &vm));
                if let Some(c) = color {
                    out = out.text_color(c);
                }
                if let Some(c) = background {
                    out = out.background(c);
                }
                if let Some(s) = size {
                    out = out.text_size(s);
                }
                if let Some(w) = weight {
                    out = out.font_weight(w);
                }
                if italic {
                    out = out.italic(true);
                }
                if underline {
                    out = out.underline(decoration());
                }
                if let Some((underlined, color)) = hover {
                    out = out.hover(zgui::compose::SpanHover {
                        underline: underlined.then(decoration),
                        text_color: color,
                    });
                }
                if strike {
                    out = out.strikethrough(Default::default());
                }
                if mono {
                    out = out.font_family(FontFamily::Monospace);
                }
                if padding != [0.; 2] {
                    out = out.padding(padding[0], padding[1]);
                }
                if radius > 0. {
                    out = out.radius(radius);
                }
                if let Some(handler) = &span.click {
                    let (handler, vm, actions) = (handler.clone(), vm.clone(), actions.clone());
                    // Told the keys held, as an element's `on_click` is.
                    out = out.on_click_with(move |held| {
                        let keys = modifiers_table(&vm.lua, held).ok();
                        run(&handler, &vm, &actions, keys);
                    });
                }
                if let Some(href) = &span.href {
                    out = out.href(href.clone());
                }
                if let Some(icon) = &span.icon {
                    // Tracked: a remote icon shows once it comes.
                    let data = load_svg(&vm, &low, &icon.src);
                    // `"currentColor"`: the span's text colour.
                    let (svg, tint) = match &icon.tint {
                        Some(crate::value::Value::Str(s)) if &**s == "currentColor" => {
                            (Arc::new(data.current_color()), None)
                        }
                        Some(tint) => (data, crate::node::themed_color(tint)),
                        None => (data, None),
                    };
                    out = out.icon(zgui::compose::SpanIcon {
                        svg,
                        size: icon.size,
                        gap: icon.gap,
                        tint,
                    });
                }
                out
            })
            .collect()
    })
}

/// The translation and local transform a node's literals set, if any.
fn literal_transform(
    lits: &[(Prop, Lit)],
) -> (Option<(f32, f32)>, Option<crate::transform::LocalTransform>) {
    let mut translate: Option<(f32, f32)> = None;
    let mut local: Option<crate::transform::LocalTransform> = None;
    for (prop, lit) in lits {
        macro_rules! transform {
            () => {
                local.get_or_insert(crate::transform::LocalTransform::IDENTITY)
            };
        }
        match (prop, lit) {
            (Prop::X, Lit::Num(x)) => translate.get_or_insert((0., 0.)).0 = *x,
            (Prop::Y, Lit::Num(y)) => translate.get_or_insert((0., 0.)).1 = *y,
            (Prop::Scale, Lit::Num(v)) => transform!().scale = (*v, *v),
            (Prop::ScaleX, Lit::Num(v)) => transform!().scale.0 = *v,
            (Prop::ScaleY, Lit::Num(v)) => transform!().scale.1 = *v,
            (Prop::Rotate, Lit::Num(v)) => transform!().rotate = v.to_radians(),
            (Prop::Skew, Lit::Num(v)) => transform!().skew = (v.to_radians(), v.to_radians()),
            (Prop::SkewX, Lit::Num(v)) => transform!().skew.0 = v.to_radians(),
            (Prop::SkewY, Lit::Num(v)) => transform!().skew.1 = v.to_radians(),
            (Prop::Origin, Lit::Pair(x, y)) => transform!().origin = (*x, *y),
            _ => {}
        }
    }
    (translate, local)
}
fn is_corner(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::RoundedTl | Prop::RoundedTr | Prop::RoundedBr | Prop::RoundedBl
    )
}
fn is_transform(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::X
            | Prop::Y
            | Prop::Scale
            | Prop::ScaleX
            | Prop::ScaleY
            | Prop::Rotate
            | Prop::Skew
            | Prop::SkewX
            | Prop::SkewY
            | Prop::Origin
    )
}

/// Applies literal styles; translation and the local transform collect
/// their parts first, so each is set once.
fn apply_all<S: Styled>(mut s: S, lits: &[(Prop, Lit)]) -> S {
    // Through its styles: a view moved in and out of each setter is a
    // kilobyte copied twice.
    let styles = s.styles_mut();
    for (prop, lit) in lits {
        if !is_transform(*prop) {
            apply(&mut *styles, *prop, lit);
        }
    }
    // Single corners refine the radius or corners set with them.
    if lits.iter().any(|(p, _)| is_corner(*p)) {
        let mut c = zgui::decoration::Corners::all(0.);
        for (prop, lit) in lits {
            match (prop, lit) {
                (Prop::Rounded, Lit::Num(r)) => c = zgui::decoration::Corners::all(*r),
                (Prop::Corners, Lit::Corners(all)) => c = *all,
                (Prop::RoundedTl, Lit::Num(r)) => c.top_left = *r,
                (Prop::RoundedTr, Lit::Num(r)) => c.top_right = *r,
                (Prop::RoundedBr, Lit::Num(r)) => c.bottom_right = *r,
                (Prop::RoundedBl, Lit::Num(r)) => c.bottom_left = *r,
                _ => {}
            }
        }
        styles.rounded_corners(c);
    }
    let (translate, local) = literal_transform(lits);
    if let Some((x, y)) = translate {
        s = s.translate(x, y);
    }
    if let Some(local) = local {
        s = crate::transform::style(s, local);
    }
    s
}

fn align(v: &str) -> Align {
    match v {
        "center" => Align::Center,
        "end" => Align::End,
        "baseline" => Align::Baseline,
        "stretch" => Align::Stretch,
        _ => Align::Start,
    }
}

fn apply<S: Styled>(s: S, prop: Prop, lit: &Lit) -> S {
    use Prop::*;
    if let Lit::Token(token) = lit {
        return match token_field(prop) {
            Some(field) => s.token(field, *token),
            None => s,
        };
    }
    if let Lit::Mix(from, to, t) = lit {
        return match token_field(prop) {
            Some(field) => s.token_mix(field, *from, *to, *t),
            None => s,
        };
    }
    // Lengths: points, percentages (of the containing block) or auto.
    let len = |l: &Lit| match l {
        Lit::Len(l) => Some(*l),
        Lit::Num(n) => Some(Length::Px(*n)),
        _ => None,
    };
    if let Some(l) = len(lit) {
        let pct = |p: f32| p * 100.;
        match (prop, l) {
            (W, Length::Px(v)) => return s.w(v),
            (W, Length::Percent(p)) => return s.w_percent(pct(p)),
            (H, Length::Px(v)) => return s.h(v),
            (H, Length::Percent(p)) => return s.h_percent(pct(p)),
            (MinW, Length::Px(v)) => return s.min_w(v),
            (MinW, Length::Percent(p)) => return s.min_w_percent(pct(p)),
            (MaxW, Length::Px(v)) => return s.max_w(v),
            (MaxW, Length::Percent(p)) => return s.max_w_percent(pct(p)),
            (MinH, Length::Px(v)) => return s.min_h(v),
            (MinH, Length::Percent(p)) => return s.min_h_percent(pct(p)),
            (MaxH, Length::Px(v)) => return s.max_h(v),
            (MaxH, Length::Percent(p)) => return s.max_h_percent(pct(p)),
            (P, Length::Px(v)) => return s.p(v),
            (P, Length::Percent(p)) => return s.p_percent(pct(p)),
            (Px, Length::Px(v)) => return s.px(v),
            (Px, Length::Percent(p)) => return s.px_percent(pct(p)),
            (Py, Length::Px(v)) => return s.py(v),
            (Py, Length::Percent(p)) => return s.py_percent(pct(p)),
            (Pt, Length::Px(v)) => return s.pt(v),
            (Pt, Length::Percent(p)) => return s.pt_percent(pct(p)),
            (Pr, Length::Px(v)) => return s.pr(v),
            (Pr, Length::Percent(p)) => return s.pr_percent(pct(p)),
            (Pb, Length::Px(v)) => return s.pb(v),
            (Pb, Length::Percent(p)) => return s.pb_percent(pct(p)),
            (Pl, Length::Px(v)) => return s.pl(v),
            (Pl, Length::Percent(p)) => return s.pl_percent(pct(p)),
            (M, Length::Px(v)) => return s.m(v),
            (M, Length::Percent(p)) => return s.m_percent(pct(p)),
            (M, Length::Auto) => return s.m_auto(),
            (Mx, Length::Px(v)) => return s.mx(v),
            (Mx, Length::Percent(p)) => return s.mx_percent(pct(p)),
            (Mx, Length::Auto) => return s.mx_auto(),
            (My, Length::Px(v)) => return s.my(v),
            (My, Length::Percent(p)) => return s.my_percent(pct(p)),
            (My, Length::Auto) => return s.my_auto(),
            (Mt, Length::Px(v)) => return s.mt(v),
            (Mt, Length::Percent(p)) => return s.mt_percent(pct(p)),
            (Mt, Length::Auto) => return s.mt_auto(),
            (Mr, Length::Px(v)) => return s.mr(v),
            (Mr, Length::Percent(p)) => return s.mr_percent(pct(p)),
            (Mr, Length::Auto) => return s.mr_auto(),
            (Mb, Length::Px(v)) => return s.mb(v),
            (Mb, Length::Percent(p)) => return s.mb_percent(pct(p)),
            (Mb, Length::Auto) => return s.mb_auto(),
            (Ml, Length::Px(v)) => return s.ml(v),
            (Ml, Length::Percent(p)) => return s.ml_percent(pct(p)),
            (Ml, Length::Auto) => return s.ml_auto(),
            (Top, l) => return s.top(l),
            (Left, l) => return s.left(l),
            (InsetStart, l) => return s.start(l),
            (InsetEnd, l) => return s.end(l),
            (Ps, l) => return s.ps(l),
            (Pe, l) => return s.pe(l),
            (Ms, l) => return s.ms(l),
            (Me, l) => return s.me(l),
            (Right, l) => return s.right(l),
            (Bottom, l) => return s.bottom(l),
            (Basis, l) => return s.flex_basis(l),
            (GapX, l) => return s.gap_x(l),
            (GapY, l) => return s.gap_y(l),
            _ => {}
        }
    }
    match (prop, lit) {
        (Size, Lit::Pair(w, h)) => s.size(*w, *h),
        (WFull, Lit::Bool(true)) => s.w_full(),
        (HFull, Lit::Bool(true)) => s.h_full(),
        (Aspect, Lit::Num(v)) => s.aspect_ratio(*v),
        (Absolute, Lit::Bool(true)) => s.absolute(),
        (Relative, Lit::Bool(true)) => s.relative(),
        (Gap, Lit::Num(v)) => s.gap(*v),
        (Direction, Lit::Str(v)) => match &**v {
            "row" => s.flex_row(),
            "row_reverse" => s.flex_row_reverse(),
            "column_reverse" => s.flex_col_reverse(),
            _ => s.flex_col(),
        },
        (Grow, Lit::Num(v)) => s.flex_grow(*v),
        (Shrink, Lit::Num(v)) => s.flex_shrink(*v),
        (FlexWrap, Lit::Bool(true)) => s.flex_wrap(),
        (WrapReverse, Lit::Bool(true)) => s.flex_wrap_reverse(),
        (Items, Lit::Str(v)) => match &**v {
            "start" => s.items_start(),
            "center" => s.items_center(),
            "end" => s.items_end(),
            "baseline" => s.items_baseline(),
            _ => s.items_stretch(),
        },
        (Justify, Lit::Str(v)) => match &**v {
            "center" => s.justify_center(),
            "end" => s.justify_end(),
            "between" => s.justify_between(),
            "around" => s.justify_around(),
            "evenly" => s.justify_evenly(),
            "stretch" => s.justify_stretch(),
            _ => s.justify_start(),
        },
        (AlignSelf, Lit::Str(v)) => s.align_self(align(v)),
        (AlignContent, Lit::Str(v)) => s.align_content(match &**v {
            "center" => ContentAlign::Center,
            "end" => ContentAlign::End,
            "stretch" => ContentAlign::Stretch,
            "between" => ContentAlign::SpaceBetween,
            "around" => ContentAlign::SpaceAround,
            "evenly" => ContentAlign::SpaceEvenly,
            _ => ContentAlign::Start,
        }),
        (Grid, Lit::Bool(true)) => s.grid(),
        (Cols, Lit::Num(v)) => s.grid_cols(v.max(1.) as u16),
        (Rows, Lit::Num(v)) => s.grid_rows(v.max(1.) as u16),
        (ColStart, Lit::Num(v)) => s.col_start(*v as i16),
        (RowStart, Lit::Num(v)) => s.row_start(*v as i16),
        (ColSpan, Lit::Num(v)) => s.col_span(v.max(1.) as u16),
        (RowSpan, Lit::Num(v)) => s.row_span(v.max(1.) as u16),
        (ColSpanFull, Lit::Bool(true)) => s.col_span_full(),
        (RowSpanFull, Lit::Bool(true)) => s.row_span_full(),
        (Overflow, Lit::Str(v)) => match &**v {
            "hidden" | "clip" => s.overflow_hidden(),
            _ => s.overflow_visible(),
        },
        (OverflowX, Lit::Str(v)) => match &**v {
            "hidden" | "clip" => s.overflow_x_hidden(),
            _ => s.overflow_x_visible(),
        },
        (OverflowY, Lit::Str(v)) => match &**v {
            "hidden" | "clip" => s.overflow_y_hidden(),
            _ => s.overflow_y_visible(),
        },
        (Hidden, Lit::Bool(true)) => s.hidden(),
        (Hidden, Lit::Bool(false)) => s.visible(),
        (Invisible, Lit::Bool(true)) => s.invisible(),
        (Bg, Lit::Color(c)) => s.bg(*c),
        (Gradient, Lit::Gradient(angle, stops)) => s.bg_gradient(*angle, stops.clone()),
        (TextColor, Lit::Color(c)) => s.text_color(*c),
        (Rounded, Lit::Num(v)) => s.rounded(*v),
        (Corners, Lit::Corners(c)) => s.rounded_corners(*c),
        // The border paints over the node's children (a CSS inset outline).
        (BorderOver, Lit::Bool(v)) => s.border_over(*v),
        // `{ top, right, bottom, left }`, as CSS orders border widths.
        (BorderSides, Lit::Corners(c)) => s.border_edges(zgui::scene::Insets {
            top: c.top_left,
            right: c.top_right,
            bottom: c.bottom_right,
            left: c.bottom_left,
        }),
        (RoundedTl | RoundedTr | RoundedBr | RoundedBl, _) => s,
        (Border, Lit::Num(v)) => s.border(*v),
        (BorderColor, Lit::Color(c)) => s.border_color(*c),
        (Outline, Lit::Num(v)) => s.outline_width(*v),
        (OutlineColor, Lit::Color(c)) => s.outline_color(*c),
        (OutlineOffset, Lit::Num(v)) => s.outline_offset(*v),
        (BorderDash, Lit::Pair(length, gap)) => {
            s.border_style(zgui::decoration::BorderStyle::Dashed {
                length: *length,
                gap: *gap,
            })
        }
        (Shadow, Lit::Shadows(shadows)) => s.shadows(shadows.clone()),
        (InsetShadow, Lit::Shadows(shadows)) => s.inset_shadows(shadows.clone()),
        (Opacity, Lit::Num(v)) => s.opacity(v.clamp(0., 1.)),
        (Blur, Lit::Num(v)) => s.filter_blur(v.max(0.)),
        (BackdropBlur, Lit::Num(v)) => s.blur(v.max(0.)),
        (Brightness, Lit::Num(v)) => s.brightness(*v),
        (Contrast, Lit::Num(v)) => s.contrast(*v),
        (Saturate, Lit::Num(v)) => s.saturate(*v),
        (Fade, Lit::Pair(top, bottom)) => s.fade_edges(*top, *bottom),
        (FadeX, Lit::Pair(left, right)) => s.fade_edges_x(*left, *right),
        (FadeOverflow, Lit::Bool(v)) => s.fade_overflow(*v),
        (Snap, Lit::Bool(v)) => s.snap(*v),
        // `{ top, right, bottom, left }` in points; negative reaches past the box.
        (ClipInset, Lit::Corners(c)) => {
            s.clip_inset(c.top_left, c.top_right, c.bottom_right, c.bottom_left)
        }
        // `{ top, right, bottom, left }`; 0 keeps an edge sharp.
        (FadeSides, Lit::Corners(c)) => {
            s.fade_sides(c.top_left, c.top_right, c.bottom_right, c.bottom_left)
        }
        (Layer, Lit::Bool(v)) => s.isolated(*v),
        (BgImage | BgFit, _) => s,
        (Fit, Lit::Str(v)) => s.object_fit(match &**v {
            "contain" => ObjectFit::Contain,
            "cover" => ObjectFit::Cover,
            "none" => ObjectFit::None,
            "scale_down" => ObjectFit::ScaleDown,
            _ => ObjectFit::Fill,
        }),
        (Cursor, Lit::Str(v)) => s.cursor(cursor(v)),
        (Dir, Lit::Str(v)) => match &**v {
            "rtl" => s.rtl(),
            "ltr" => s.direction(zgui::scene::Direction::Ltr),
            _ => s,
        },
        (TextSize, Lit::Num(v)) => s.text_size(*v),
        (Weight, Lit::Num(v)) => s.font_weight(*v as u16),
        (Bold, Lit::Bool(true)) => s.font_bold(),
        (Italic, Lit::Bool(v)) => s.italic(*v),
        (Align, Lit::Str(v)) => s.text_align(match &**v {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "left" => TextAlign::Left,
            "end" => TextAlign::End,
            _ => TextAlign::Start,
        }),
        (Font, Lit::Str(v)) => match &**v {
            "mono" | "monospace" => s.font_family(FontFamily::Monospace),
            "sans" | "sans-serif" => s.font_family(FontFamily::SansSerif),
            "serif" => s.font_family(FontFamily::Serif),
            name => s.font_family(FontFamily::Named(name.into())),
        },
        (Wrap, Lit::Bool(v)) => s.text_wrap(*v),
        (Truncate, Lit::Bool(true)) => s.truncate(),
        (Lines, Lit::Num(v)) => s.line_clamp(*v as u32),
        (LineHeight, Lit::Num(v)) => s.line_height(*v),
        (LetterSpacing, Lit::Num(v)) => s.letter_spacing(*v),
        (Underline, Lit::Bool(v)) => s.text_underline(*v),
        (UnderlineOffset, Lit::Num(v)) => s.underline_offset(*v),
        // Every digit one width, so changing numbers hold still.
        (Tabular, Lit::Bool(true)) => {
            s.font_features(zgui::text_layout::FontFeatures::new([(*b"tnum", 1)]))
        }
        _ => s,
    }
}

/// A cursor by its name: `"nwse_resize"`, or CSS's `"nwse-resize"`.
fn cursor(name: &str) -> zgui::cursor::Cursor {
    use zgui::cursor::Cursor::*;
    let name = name.replace('-', "_");
    match name.as_str() {
        "pointer" => Pointer,
        "text" => Text,
        "vertical_text" => VerticalText,
        "crosshair" => Crosshair,
        "move" => Move,
        "grab" => Grab,
        "grabbing" => Grabbing,
        "not_allowed" => NotAllowed,
        "wait" => Wait,
        "progress" => Progress,
        "help" => Help,
        "context_menu" => ContextMenu,
        "copy" => Copy,
        "alias" => Alias,
        "no_drop" => NoDrop,
        "all_scroll" => AllScroll,
        "col_resize" => ColResize,
        "row_resize" => RowResize,
        "ew_resize" => EwResize,
        "ns_resize" => NsResize,
        "nesw_resize" => NeswResize,
        "nwse_resize" => NwseResize,
        "zoom_in" => ZoomIn,
        "zoom_out" => ZoomOut,
        _ => Default,
    }
}

/// One piece of content in a presence: its value, and whether it is
/// leaving.
struct Entry {
    id: u64,
    value: Value,
    exiting: bool,
    animate_in: bool,
    /// Holds the presence's place while `mode = "wait"` waits for what
    /// leaves (an empty box: what leaves lies over it).
    placeholder: bool,
}
#[derive(Default)]
struct Present {
    entries: Vec<Entry>,
    next: u64,
    elements: HashMap<u64, (Rc<Element>, NodeHandle)>,
    /// `mode = "wait"`: the value to show once the leaving content is gone.
    waiting: Option<Value>,
}
impl Present {
    fn push(&mut self, value: Value, animate_in: bool, placeholder: bool) {
        let id = self.next;
        self.next += 1;
        self.entries.push(Entry {
            id,
            value,
            exiting: false,
            animate_in,
            placeholder,
        });
    }
}

/// Properties that make a presence a box of its own, which its content
/// fills: without them it lays out as its content, as if in its place.
const BOX: &[Prop] = &[
    Prop::W,
    Prop::H,
    Prop::Size,
    Prop::MinW,
    Prop::MaxW,
    Prop::MinH,
    Prop::MaxH,
    Prop::WFull,
    Prop::HFull,
    Prop::Aspect,
    Prop::Absolute,
    Prop::Top,
    Prop::Left,
    Prop::Right,
    Prop::Bottom,
    Prop::P,
    Prop::Px,
    Prop::Py,
    Prop::Pt,
    Prop::Pr,
    Prop::Pb,
    Prop::Pl,
    Prop::Grow,
    Prop::Shrink,
    Prop::Basis,
    Prop::AlignSelf,
    Prop::ColStart,
    Prop::RowStart,
    Prop::ColSpan,
    Prop::RowSpan,
    Prop::ColSpanFull,
    Prop::RowSpanFull,
];

/// `ui.presence`: content for the current value. When the value changes
/// the new content mounts and plays `enter`; the old leaves the flow
/// (positioned where it was, at the size it had) and plays `exit`, then
/// unmounts. With sizing of its own (`boxed`) it is a box the content
/// fills; otherwise the content lays out as if it stood in its place.
#[allow(clippy::too_many_arguments)]
fn presence(
    cx: &mut Context,
    value: Rc<Owned>,
    builder: mlua::Function,
    enter: Option<mlua::Table>,
    exit: Option<mlua::Table>,
    wait: bool,
    initial: bool,
    boxed: bool,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
) -> View {
    let state: Rc<std::cell::RefCell<Present>> = Rc::default();
    let shown = cx.state(Vec::<u64>::new());
    let publish = {
        let (state, shown) = (state.clone(), shown.clone());
        Rc::new(move || {
            let ids: Vec<u64> = state.borrow().entries.iter().map(|e| e.id).collect();
            shown.set(ids);
        })
    };
    // Plays `params` on `element`; `done` runs when it completes.
    let play = {
        let vm = vm.clone();
        Rc::new(
            move |element: &Rc<Element>,
                  params: &mlua::Table,
                  done: Option<Rc<dyn Fn()>>|
                  -> mlua::Result<()> {
                let lua = &vm.lua;
                let own = lua.create_table()?;
                for pair in params.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = pair?;
                    own.raw_set(k, v)?;
                }
                if let Some(done) = done {
                    let done = lua.create_function(move |_, _: mlua::MultiValue| {
                        done();
                        Ok(())
                    })?;
                    own.raw_set("onComplete", done)?;
                }
                let target =
                    mlua::Value::UserData(lua.create_userdata(ElementUd(element.clone()))?);
                crate::anime::api::animate(lua, target, own)?;
                Ok(())
            },
        )
    };
    let remove: Rc<dyn Fn(u64)> = {
        let (state, publish) = (state.clone(), publish.clone());
        Rc::new(move |id: u64| {
            {
                let mut st = state.borrow_mut();
                st.entries.retain(|e| e.id != id);
                st.elements.remove(&id);
                if let Some(value) = st
                    .waiting
                    .take()
                    .filter(|_| st.entries.iter().all(|e| !e.exiting))
                {
                    st.entries.retain(|e| !e.placeholder);
                    st.push(value, true, false);
                }
            }
            publish();
        })
    };
    let change = {
        let (state, publish, play, remove, vm, engine) = (
            state.clone(),
            publish.clone(),
            play.clone(),
            remove.clone(),
            vm.clone(),
            vm.lua
                .app_data_ref::<crate::node::Env>()
                .map(|env| env.engine.clone()),
        );
        move |value: Value| {
            let leaving = {
                let mut st = state.borrow_mut();
                let first = st.entries.is_empty() && st.waiting.is_none();
                if st
                    .entries
                    .iter()
                    .any(|e| !e.exiting && !e.placeholder && e.value == value)
                {
                    return;
                }
                let mut leaving = Vec::new();
                for e in st
                    .entries
                    .iter_mut()
                    .filter(|e| !e.exiting && !e.placeholder)
                {
                    e.exiting = true;
                    leaving.push(e.id);
                }
                if wait && st.entries.iter().any(|e| e.exiting) {
                    st.waiting = Some(value);
                    if !st.entries.iter().any(|e| e.placeholder) {
                        st.push(Value::Nil, false, true);
                    }
                } else {
                    st.entries.retain(|e| !e.placeholder);
                    st.push(value, !first || initial, false);
                }
                leaving
            };
            for id in leaving {
                let mounted = state.borrow().elements.get(&id).cloned();
                match (mounted, &exit) {
                    (Some((element, handle)), Some(exit)) => {
                        // Out of the flow, where it was and as large: what
                        // replaces it decides the size from now on.
                        let scene = handle.scene().filter(|_| handle.is_mounted());
                        let size = scene.as_ref().map_or((0., 0.), |scene| {
                            let b = scene.borrow().bounds(handle.node());
                            (b.width, b.height)
                        });
                        if let Some(scene) = &scene {
                            scene.borrow_mut().set_contents(handle.node(), false);
                        }
                        // A one-cell grid holds the content at that size,
                        // flex-sized content included.
                        let px = |v: f32| Lit::Len(zgui::layout::Length::Px(v));
                        for (prop, lit) in [
                            (Prop::Absolute, Lit::Bool(true)),
                            (Prop::Top, px(0.)),
                            (Prop::Left, px(0.)),
                            (Prop::W, px(size.0)),
                            (Prop::H, px(size.1)),
                            (Prop::Grid, Lit::Bool(true)),
                            (Prop::Cols, Lit::Num(1.)),
                            (Prop::Rows, Lit::Num(1.)),
                            (Prop::Items, Lit::Str("stretch".into())),
                        ] {
                            element.set_style(prop, lit);
                        }
                        if let Some(engine) = &engine {
                            engine.set_now(Vec::new());
                        }
                        let then = remove.clone();
                        if let Err(e) = play(&element, exit, Some(Rc::new(move || then(id)))) {
                            vm.report(e);
                            remove(id);
                        }
                    }
                    _ => remove(id),
                }
            }
            publish();
        }
    };
    let runtime = cx.runtime();
    let (value_vm, change_runtime) = (vm.clone(), runtime.clone());
    cx.retain(runtime.effect(move || {
        let current = resolve_value(&value, &value_vm);
        change_runtime.untracked(|| change(current));
    }));
    let (vm, low) = (vm.clone(), low.clone());
    // The content lays out as if it stood where the presence does (or in
    // the presence's box): the region and each entry's wrapper are contents
    // nodes, and what leaves lies over the current content.
    let showing = {
        let (state, shown) = (state.clone(), shown.clone());
        move || {
            shown.with(|_| ());
            state.borrow().entries.iter().any(|e| e.value != Value::Nil)
        }
    };
    let region = keyed(
        move || shown.get(),
        move |id, cx| {
            let (value, animate_in) = state
                .borrow()
                .entries
                .iter()
                .find(|e| e.id == id)
                .map_or((Value::Nil, false), |e| (e.value.clone(), e.animate_in));
            // Nothing shows for nil: `presence(cond and "x" or nil, …)`
            // enters and exits one piece of content.
            let content = if value == Value::Nil {
                div()
            } else {
                build_scoped(&vm, cx, &low, &builder, value)
            };
            let element = Element::new(&low.runtime, None, Rc::new([]), "presence");
            let (state, enter, play, vm) = (state.clone(), enter.clone(), play.clone(), vm.clone());
            let mounted = element.clone();
            element_view(
                // Until it leaves (see `change`).
                column().contents().child(content),
                element,
                &low,
                move |handle| {
                    state
                        .borrow_mut()
                        .elements
                        .insert(id, (mounted.clone(), handle));
                    if let (true, Some(enter)) = (animate_in, &enter)
                        && let Err(e) = play(&mounted, enter, None)
                    {
                        vm.report(e);
                    }
                },
            )
        },
    )
    .contents();
    if boxed {
        // Its content fills it, in both directions, unless it sizes itself.
        // Showing nothing, the box is not there for the pointer (a sheet's
        // full-window box over the panes while it is put away).
        cell().pointer_events_when(showing).child(region)
    } else {
        region
    }
}

/// A box whose one in-flow child fills it in both directions unless it
/// sizes itself: a one-cell grid, whose items stretch.
fn cell() -> View {
    column().grid().grid_cols(1).grid_rows(1).items_stretch()
}
