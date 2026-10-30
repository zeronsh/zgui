//! Lowers a script's node tree to retained zgx views. Literal styles become
//! ordinary fluent styles; state, computed and animated properties share one
//! reactive style per node; text templates interpolate state in Rust.
//! Targetable nodes (`id`, `class`, `ref`, `on_mount`) also carry an element:
//! what animations write, resolved over the node's own styles.
use crate::{
    Actions, Vm, anim,
    anime::{Key as Property, target::Target, value::V},
    element::{Element, ElementUd},
    node::{
        Action, Computed, Handler, Kind, LayoutAnim, Lit, Node, NodeUd, Prop, Pseudo, Scope,
        Segment, Source, Span, Text, Ty, lit,
    },
    value::Value,
};
use mlua::FromLua;
use std::{cell::OnceCell, collections::HashMap, rc::Rc, sync::Arc};
use zgx::{
    compose::{Context, NodeHandle, TextSpan, View, prelude::*},
    image::ImageData,
    input::{EventPhase, InputEvent, Key, PointerButton},
    layout::{ContentAlign, Length},
    reactive::Signal,
    scene::{Align, Color},
    svg::SvgData,
    text::TextValue,
};

/// Decoded images and SVGs by source. What a mounted node shows stays;
/// of the rest, the most recently shown stay while they fit `budget`, so a
/// reload or a page shown again is instant without every picture a run
/// ever showed staying in memory. Swept as nodes showing them unmount.
pub(crate) struct Assets {
    images: std::cell::RefCell<HashMap<String, Cached<ImageData>>>,
    svgs: std::cell::RefCell<HashMap<String, Cached<SvgData>>>,
    /// The bytes cached (the host's own images aside), and how many the
    /// pictures no node shows may take (`ScriptHost::image_cache`).
    held: std::cell::Cell<usize>,
    budget: std::cell::Cell<usize>,
    /// Counts uses, so the least recently shown go first.
    clock: std::cell::Cell<u64>,
    /// Pictures decoded or parsed: what a test reads to tell a hit.
    decoded: std::cell::Cell<usize>,
    /// Each file's path and modification time when it was read, so a
    /// watching host sees an edited image (see `changed`).
    files: std::cell::RefCell<HashMap<String, (std::path::PathBuf, Option<std::time::SystemTime>)>>,
    /// Where they are read from: the disk, or the app's bundle.
    source: std::cell::Cell<Option<crate::Files>>,
    /// Remote sources (`https://…`) by URL: fetched once, whoever shows
    /// them; what shows one follows its status (`LOADING`, `READY`, `FAILED`).
    remote: std::cell::RefCell<HashMap<String, zgx::reactive::Signal<u8>>>,
    /// Failed remote sources: when, and how many times in a row. Asked
    /// for again after a backoff, they're fetched again (as a browser
    /// retries a broken image when a new element shows it).
    failed: std::cell::RefCell<HashMap<String, (std::time::Instant, u32)>>,
    /// The host's tasks and transport, which remote sources go through.
    links: std::cell::RefCell<Option<(Rc<crate::timers::Timers>, crate::calls::SharedFetcher)>>,
    /// Images the host made (`ScriptHost::image`), as `host:<name>`
    /// sources: what shows one follows its version.
    hosted: std::cell::RefCell<HashMap<String, zgx::reactive::Signal<u64>>>,
}
/// A cached picture, its size, and when it was last shown.
struct Cached<T> {
    value: Arc<T>,
    bytes: usize,
    used: std::cell::Cell<u64>,
}
/// What the image cache keeps by default of pictures no node shows.
pub(crate) const IMAGE_CACHE_BUDGET: usize = 48 << 20;
impl Default for Assets {
    fn default() -> Self {
        Self {
            images: Default::default(),
            svgs: Default::default(),
            held: Default::default(),
            budget: std::cell::Cell::new(IMAGE_CACHE_BUDGET),
            clock: Default::default(),
            decoded: Default::default(),
            files: Default::default(),
            source: Default::default(),
            remote: Default::default(),
            failed: Default::default(),
            links: Default::default(),
            hosted: Default::default(),
        }
    }
}
/// Held by a node showing a cached picture: as it unmounts, the cache
/// sweeps if it holds more than its budget.
struct Sweep(std::rc::Weak<Assets>);
impl Drop for Sweep {
    fn drop(&mut self) {
        if let Some(assets) = self.0.upgrade()
            && assets.held.get() > assets.budget.get()
        {
            assets.sweep(assets.budget.get());
        }
    }
}
impl Assets {
    fn tick(&self) -> u64 {
        let now = self.clock.get() + 1;
        self.clock.set(now);
        now
    }
    /// The image decoded from `src`, if it is cached.
    fn image(&self, src: &str) -> Option<Arc<ImageData>> {
        let images = self.images.borrow();
        let cached = images.get(src)?;
        cached.used.set(self.tick());
        Some(cached.value.clone())
    }
    fn svg(&self, src: &str) -> Option<Arc<SvgData>> {
        let svgs = self.svgs.borrow();
        let cached = svgs.get(src)?;
        cached.used.set(self.tick());
        Some(cached.value.clone())
    }
    /// Cache `image` as `src`'s. The host's own are not counted: they go
    /// only when the host says.
    fn keep_image(&self, src: String, image: Arc<ImageData>) {
        let bytes = match src.starts_with("host:") {
            true => 0,
            false => image.width() as usize * image.height() as usize * 4,
        };
        let cached = Cached {
            value: image,
            bytes,
            used: std::cell::Cell::new(self.tick()),
        };
        self.held.set(self.held.get() + bytes);
        if let Some(old) = self.images.borrow_mut().insert(src, cached) {
            self.held.set(self.held.get() - old.bytes);
        }
        self.decoded.set(self.decoded.get() + 1);
    }
    fn keep_svg(&self, src: String, svg: Arc<SvgData>) {
        let bytes = svg.bytes().len();
        let cached = Cached {
            value: svg,
            bytes,
            used: std::cell::Cell::new(self.tick()),
        };
        self.held.set(self.held.get() + bytes);
        if let Some(old) = self.svgs.borrow_mut().insert(src, cached) {
            self.held.set(self.held.get() - old.bytes);
        }
        self.decoded.set(self.decoded.get() + 1);
    }
    /// Forget `src`'s picture.
    fn forget(&self, src: &str) {
        let images = self
            .images
            .try_borrow_mut()
            .ok()
            .and_then(|mut i| i.remove(src));
        let svgs = self
            .svgs
            .try_borrow_mut()
            .ok()
            .and_then(|mut s| s.remove(src));
        let bytes = images.map_or(0, |c| c.bytes) + svgs.map_or(0, |c| c.bytes);
        self.held.set(self.held.get() - bytes);
    }
    /// A guard for a node showing cached pictures: as it unmounts, the
    /// cache drops what no node shows beyond its budget.
    pub(crate) fn sweeper(self: &Rc<Self>) -> impl std::any::Any {
        Sweep(Rc::downgrade(self))
    }
    pub(crate) fn set_budget(&self, bytes: usize) {
        self.budget.set(bytes);
    }
    /// What is cached: pictures, bytes, and how many were decoded so far.
    pub(crate) fn stats(&self) -> (usize, usize, usize) {
        let count = self.images.borrow().len() + self.svgs.borrow().len();
        (count, self.held.get(), self.decoded.get())
    }
    /// Keeps only what a node shows: the host goes idle, and a page shown
    /// again decodes its pictures again rather than every picture shown
    /// lately staying in memory while nothing happens.
    pub(crate) fn trim(&self) {
        self.sweep(0);
    }
    /// The cache's lines of a memory report: pictures a node shows, and
    /// those kept within the budget for when one does again.
    fn memory_of<T>(
        cached: &HashMap<String, Cached<T>>,
        kind: &str,
        report: &mut zgx::memory::Report,
    ) {
        let (mut shown, mut shown_bytes, mut kept, mut kept_bytes) = (0, 0, 0, 0);
        for picture in cached.values() {
            if Arc::strong_count(&picture.value) > 1 {
                shown += 1;
                shown_bytes += picture.bytes;
            } else {
                kept += 1;
                kept_bytes += picture.bytes;
            }
        }
        report.add(
            "images",
            format!("{kind} shown"),
            Some(shown),
            Some(shown_bytes),
        );
        report.add(
            "images",
            format!("{kind} cached"),
            Some(kept),
            Some(kept_bytes),
        );
    }
    pub(crate) fn memory(&self, report: &mut zgx::memory::Report) {
        Self::memory_of(&self.images.borrow(), "decoded", report);
        Self::memory_of(&self.svgs.borrow(), "svg", report);
    }
    /// Keeps what a node shows (held elsewhere too), and of the rest the
    /// most recently shown while they fit `budget`; drops the others.
    /// Run as nodes unmount and as the host goes idle, never per frame.
    #[cold]
    #[inline(never)]
    fn sweep(&self, budget: usize) {
        let (Ok(mut images), Ok(mut svgs)) =
            (self.images.try_borrow_mut(), self.svgs.try_borrow_mut())
        else {
            return;
        };
        let mut idle: Vec<(u64, usize, bool, String)> = Vec::new();
        for (src, c) in images.iter() {
            if c.bytes > 0 && Arc::strong_count(&c.value) == 1 {
                idle.push((c.used.get(), c.bytes, true, src.clone()));
            }
        }
        for (src, c) in svgs.iter() {
            if Arc::strong_count(&c.value) == 1 {
                idle.push((c.used.get(), c.bytes, false, src.clone()));
            }
        }
        idle.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
        let (mut kept, mut freed) = (0, 0);
        for (_, bytes, image, src) in idle {
            if kept + bytes <= budget {
                kept += bytes;
                continue;
            }
            match image {
                true => drop(images.remove(&src)),
                false => drop(svgs.remove(&src)),
            }
            freed += bytes;
        }
        self.held.set(self.held.get() - freed);
    }
    /// `name`'s version signal, made with `runtime` the first time.
    fn hosted(&self, src: &str, runtime: &zgx::reactive::Runtime) -> zgx::reactive::Signal<u64> {
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
        runtime: &zgx::reactive::Runtime,
    ) {
        let src = format!("host:{name}");
        match image {
            Some(image) => self.keep_image(src.clone(), image),
            None => self.forget(&src),
        }
        let version = self.hosted(&src, runtime);
        version.update(|v| *v = v.wrapping_add(1));
    }
    /// Whether the host set image `host:<name>`.
    pub(crate) fn is_hosted(&self, name: &str) -> bool {
        self.images.borrow().contains_key(&format!("host:{name}"))
    }
    /// Forget image `host:<name>` and its version, for good (what showed it
    /// keeps its picture).
    fn forget_hosted(&self, name: &str) {
        let src = format!("host:{name}");
        self.forget(&src);
        if let Ok(mut hosted) = self.hosted.try_borrow_mut() {
            hosted.remove(&src);
        }
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
    /// Where its files come from: the disk, or the app's bundle.
    #[cfg(feature = "gltf")]
    pub(crate) fn source(&self) -> crate::Files {
        self.source.get().unwrap_or(crate::Files::Disk)
    }
    /// `src`'s bytes from `path`, the file kept to watch for changes.
    pub(crate) fn read(&self, src: &str, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
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
            self.forget(src);
        }
        !stale.is_empty()
    }
}

/// What plugins add at the app's elements (`ui.contribute { target }`),
/// indexed so an element looks up only what could match it: the targets
/// naming its id, its declared hook, and those naming neither (a role or a
/// name alone). Matched as the interface is built, never per frame; empty,
/// as without plugins, it costs a length compare per element.
#[derive(Default)]
pub(crate) struct Hooks {
    all: Vec<crate::node::HookContribution>,
    by_id: ContentMap<Rc<str>, Vec<usize>>,
    by_hook: ContentMap<Rc<str>, Vec<usize>>,
    loose: Vec<usize>,
    /// The application's script: only its elements are targets, not what
    /// plugins add (a replacement that matched its own target would
    /// replace itself without end).
    app: std::rc::Weak<Vm>,
}
impl Hooks {
    pub(crate) fn extend(&mut self, added: Vec<crate::node::HookContribution>) {
        for hook in added {
            let at = self.all.len();
            match (&hook.target.id, &hook.target.hook) {
                (Some(id), _) => self.by_id.entry(id.as_str().into()).or_default().push(at),
                (None, Some(name)) => self
                    .by_hook
                    .entry(name.as_str().into())
                    .or_default()
                    .push(at),
                (None, None) => self.loose.push(at),
            }
            self.all.push(hook);
        }
    }
    /// The application script's VM, whose elements plugins target.
    pub(crate) fn set_app(&mut self, app: &Rc<Vm>) {
        self.app = Rc::downgrade(app);
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
    /// What is added at `node`, in the order the scripts added it: none at
    /// a `private` element, inside one, or at one a plugin made.
    /// `by_name`: whether its name may match (not the text that names its
    /// parent, the button and its label being one thing).
    fn at(
        &self,
        node: &Node,
        by_name: bool,
        low: &Lowering,
    ) -> smallvec::SmallVec<[&crate::node::HookContribution; 2]> {
        if (node.id.is_none() && node.hook().is_none() && self.loose.is_empty())
            || node.private
            || low.sealed.get() > 0
            || !std::rc::Weak::ptr_eq(&node.owner, &self.app)
        {
            return smallvec::SmallVec::new();
        }
        let mut found: smallvec::SmallVec<[usize; 4]> = smallvec::SmallVec::new();
        if let Some(by_id) = node.id.as_deref().and_then(|id| self.by_id.get(id)) {
            found.extend_from_slice(by_id);
        }
        if let Some(by_hook) = node.hook().and_then(|name| self.by_hook.get(name)) {
            found.extend_from_slice(by_hook);
        }
        found.extend_from_slice(&self.loose);
        found.sort_unstable();
        found
            .into_iter()
            .map(|at| &self.all[at])
            .filter(|hook| (by_name || hook.target.text.is_none()) && targets(&hook.target, node))
            .collect()
    }
}
/// Whether `node` is what `target` names, by what is known of it before it
/// shows: its id, declared hook, role and name as written.
fn targets(target: &crate::driver::Static, node: &Node) -> bool {
    target
        .id
        .as_deref()
        .is_none_or(|id| node.id.as_deref() == Some(id))
        && target
            .hook
            .as_deref()
            .is_none_or(|name| node.hook() == Some(name))
        && target
            .role
            .as_deref()
            .is_none_or(|role| static_role(node) == Some(role))
        && target
            .text
            .as_deref()
            .is_none_or(|text| static_name(node) == Some(text))
}
/// A node's role as `role:` targets name it: the one it sets, else its
/// kind's.
fn static_role(node: &Node) -> Option<&'static str> {
    if let Some(role) = node.a11y.as_ref().and_then(|a11y| a11y.role) {
        return Some(crate::driver::role_name(role));
    }
    Some(match node.kind {
        Kind::Button => "button",
        Kind::Text(_) | Kind::Rich(_) => "text",
        Kind::Checkbox { .. } => "checkbox",
        Kind::Input { .. } => "textbox",
        Kind::Slider { .. } => "slider",
        Kind::Progress { .. } => "progressbar",
        Kind::Scroll { .. } => "scroll",
        Kind::Modal { .. } => "dialog",
        Kind::Image { .. } | Kind::Svg { .. } => "img",
        Kind::Menu { .. } => "menu",
        Kind::MenuItem(..) => "menuitem",
        _ => return None,
    })
}
/// A node's name as written: its `label`, its own text, or a button's or
/// item's text (its first child's).
fn static_name(node: &Node) -> Option<&str> {
    if let Some(label) = node.label.as_deref() {
        return Some(label);
    }
    fn literal(node: &Node) -> Option<&str> {
        match &node.kind {
            Kind::Text(Text::Literal(text)) => Some(text.as_str()),
            _ => None,
        }
    }
    if let Kind::MenuItem(label, _) = &node.kind {
        return Some(label.as_str());
    }
    literal(node).or_else(|| node.children.first().and_then(|child| literal(child)))
}
/// A plugin's node at an element of the app's: lowered as its own, and
/// told apart as the plugin's, so a contributed "Confirm" beside the app's
/// is traced to it (assistive technology hears whose it is; the layout
/// inspector shows the plugin's line that made it).
fn contributed(hook: &crate::node::HookContribution, node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    let view = lower(node, low);
    let from = format!("from the plugin {}", hook.owner);
    let described = match node
        .a11y
        .as_ref()
        .and_then(|a11y| a11y.description.as_deref())
    {
        Some(own) => format!("{own}, {from}"),
        None => format!("From the plugin {}", hook.owner),
    };
    view.accessible_description(described)
}
/// A component inside a `private` element: built later, as it mounts,
/// and still sealed from plugins then.
#[cold]
#[inline(never)]
fn sealed_component(node: &Rc<Node>, vm: &Rc<Vm>, low: &Rc<Lowering>, sealed: u32) -> View {
    let (node, low, vm) = (node.clone(), low.clone(), vm.clone());
    component(move |cx| {
        let outer = low.sealed.replace(sealed);
        let view = build(&node, &vm, &low, Some(cx));
        low.sealed.set(outer);
        view
    })
}
/// `node`'s children, with what plugins add before and after each or in
/// its place (the last script's replacement if several); none inside a
/// `private` element, which seals what it holds as it lowers. A child whose
/// text is its parent's name is not matched by that name (the parent is: a
/// button and its label are one thing). `flow`: the container's, for
/// regions among them. Apart from `build`, which lowers children without
/// plugins as before.
#[cold]
#[inline(never)]
fn hooked_children(
    view: View,
    node: &Node,
    low: &Rc<Lowering>,
    hooks: &Hooks,
    flow: Option<&[(Prop, Lit)]>,
) -> View {
    let lowered = |child: &Rc<Node>| {
        let view = lower(child, low);
        match flow {
            Some(flow) if child.kind.flows() => apply_all(view, flow),
            _ => view,
        }
    };
    if node.private {
        low.sealed.set(low.sealed.get() + 1);
        let view = view.children(node.children.iter().map(lowered));
        low.sealed.set(low.sealed.get() - 1);
        return view;
    }
    let parent = static_name(node);
    let mut children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        let names_parent = parent.is_some_and(|name| static_name(child) == Some(name));
        let at = hooks.at(child, !names_parent, low);
        let place = |place| {
            at.iter()
                .filter_map(move |hook| Some((*hook, hook.placed(place)?)))
        };
        children.extend(
            place(crate::node::Place::Before).map(|(hook, node)| contributed(hook, node, low)),
        );
        let filters = filters_in(&at);
        children.push(match place(crate::node::Place::Replace).next_back() {
            Some((hook, node)) => contributed(hook, node, low),
            None if filters.is_empty() => lowered(child),
            None => filtered(child, &filters, low, flow),
        });
        children.extend(
            place(crate::node::Place::After).map(|(hook, node)| contributed(hook, node, low)),
        );
    }
    view.children(children)
}

/// The filters among what is added at an element, each with its script,
/// in the order the scripts filter.
fn filters_in<'a>(
    at: &[&'a crate::node::HookContribution],
) -> Vec<(
    &'a crate::node::HookContribution,
    crate::node::Filtered,
    &'a mlua::Function,
)> {
    at.iter()
        .filter_map(|hook| match &hook.adds {
            crate::node::Adds::Filters(filters) => Some((*hook, filters)),
            crate::node::Adds::Node(..) => None,
        })
        .flat_map(|(hook, filters)| filters.iter().map(move |(what, f)| (hook, *what, f)))
        .collect()
}
/// What a `ui.show` or `ui.switch` shows now, with what plugins put in its
/// place or filter of it. Nothing goes before or after it: it lays out as
/// the one element it shows.
fn shown(node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    match &low.hooks {
        None => lower(node, low),
        Some(hooks) => hooked_shown(node, low, hooks),
    }
}
#[cold]
#[inline(never)]
fn hooked_shown(node: &Rc<Node>, low: &Rc<Lowering>, hooks: &Hooks) -> View {
    let at = hooks.at(node, true, low);
    let replaced = at
        .iter()
        .rev()
        .find_map(|hook| Some((*hook, hook.placed(crate::node::Place::Replace)?)));
    if let Some((hook, replacement)) = replaced {
        return contributed(hook, replacement, low);
    }
    let filters = filters_in(&at);
    match filters.is_empty() {
        true => lower(node, low),
        false => filtered(node, &filters, low, None),
    }
}
/// A value passed through a plugin's filter, by its script: what it
/// returns, or (when it fails, reported as its error) the value as it was.
fn passed(plugin: &std::rc::Weak<Vm>, f: &mlua::Function, value: Value) -> Value {
    let Some(vm) = plugin.upgrade() else {
        return value;
    };
    match vm.call::<Value>(f, value.clone()) {
        Ok(returned) => returned,
        Err(error) => {
            vm.report(error);
            value
        }
    }
}
/// A function of the app's script returning what `value` returns now: a
/// computed source of its, followed as any is (what `value` reads, the
/// app's state and the plugin's, makes it run again).
fn computed_fn(app: &Vm, value: impl Fn() -> Value + 'static) -> Option<mlua::Function> {
    app.lua
        .create_function(move |_, ()| Ok(value()))
        .map_err(|e| app.report(e))
        .ok()
}
/// `source`'s value now, as a script reads it: a string or a boolean.
fn value_now(source: &Owned, ty: Ty, app: &Vm) -> Value {
    match resolve(source, ty, None, app) {
        Some(Lit::Str(text)) => Value::Str(text),
        Some(Lit::Bool(b)) => Value::Bool(b),
        _ => Value::Nil,
    }
}
/// `source` (or, without one, `default`) passed through `f`, as a
/// computed source of the app's script.
fn filtered_source(
    source: Option<&Source>,
    default: Value,
    ty: Ty,
    plugin: &std::rc::Weak<Vm>,
    f: &mlua::Function,
    app: &Rc<Vm>,
) -> Option<Source> {
    let (source, plugin, f, vm) = (
        source.map(resolve_owned),
        plugin.clone(),
        f.clone(),
        Rc::downgrade(app),
    );
    computed_fn(app, move || {
        let now = match (&source, vm.upgrade()) {
            (Some(source), Some(vm)) => value_now(source, ty, &vm),
            _ => default.clone(),
        };
        passed(&plugin, &f, now)
    })
    .map(Source::Computed)
}
/// `text` passed through `f`, as text the app's script computes.
fn filtered_text(
    text: &Text,
    plugin: &std::rc::Weak<Vm>,
    f: &mlua::Function,
    app: &Rc<Vm>,
) -> Option<Text> {
    let (text, plugin, f, vm) = (
        copy_text(text),
        plugin.clone(),
        f.clone(),
        Rc::downgrade(app),
    );
    computed_fn(app, move || {
        let now = vm
            .upgrade()
            .map(|vm| render_text(&text, &vm))
            .unwrap_or_default();
        match passed(&plugin, &f, Value::Str(now.into())) {
            Value::Str(text) => Value::Str(text),
            Value::Nil => Value::Str("".into()),
            other => Value::Str(other.to_string().into()),
        }
    })
    .map(|f| Text::Computed(Computed::Script(f)))
}
/// `node` lowered with its props passed through plugins' filters
/// (`ui.filter`), in the order the scripts filter them: a copy of it, the
/// app's own node left as it is. Its text is its own or (a button's, an
/// item's) its first child's.
#[cold]
#[inline(never)]
fn filtered(
    node: &Rc<Node>,
    filters: &[(
        &crate::node::HookContribution,
        crate::node::Filtered,
        &mlua::Function,
    )],
    low: &Rc<Lowering>,
    flow: Option<&[(Prop, Lit)]>,
) -> View {
    use crate::node::{Filtered, TooltipContent};
    let flow: Option<Vec<(Prop, Lit)>> = flow.filter(|_| node.kind.flows()).map(<[_]>::to_vec);
    let lowered = move |node: &Rc<Node>, low: &Rc<Lowering>| {
        let view = lower(node, low);
        match &flow {
            Some(flow) => apply_all(view, flow),
            None => view,
        }
    };
    let Some(app) = node.owner.upgrade() else {
        return lowered(node, low);
    };
    let mut copy = (**node).clone();
    let (mut items, mut order) = (Vec::new(), Vec::new());
    for &(hook, what, f) in filters {
        let plugin = &hook.vm;
        match what {
            Filtered::Text => match &copy.kind {
                Kind::Text(text) => {
                    if let Some(text) = filtered_text(text, plugin, f, &app) {
                        copy.kind = Kind::Text(text);
                    }
                }
                _ => {
                    if let Some(first) = copy.children.first()
                        && let Kind::Text(text) = &first.kind
                        && let Some(text) = filtered_text(text, plugin, f, &app)
                    {
                        let mut child = (**first).clone();
                        child.kind = Kind::Text(text);
                        copy.children[0] = Rc::new(child);
                    }
                }
            },
            Filtered::Label => {
                let written = copy.label.take().map(|label| Source::Lit(Lit::Str(label)));
                let a11y = copy.a11y.get_or_insert_default();
                let source = a11y.label.take().or(written);
                a11y.label = filtered_source(source.as_ref(), Value::Nil, Ty::Str, plugin, f, &app);
            }
            Filtered::Disabled => {
                copy.disabled = filtered_source(
                    copy.disabled.as_ref(),
                    Value::Bool(false),
                    Ty::Bool,
                    plugin,
                    f,
                    &app,
                );
            }
            Filtered::Hidden => {
                let at = copy
                    .props
                    .iter()
                    .position(|(prop, _)| *prop == Prop::Hidden);
                let source = at.map(|at| &copy.props[at].1);
                if let Some(source) =
                    filtered_source(source, Value::Bool(false), Ty::Bool, plugin, f, &app)
                {
                    match at {
                        Some(at) => copy.props[at].1 = source,
                        None => copy.props.push((Prop::Hidden, source)),
                    }
                }
            }
            // A tooltip's text is written once: passed through as the
            // element is built.
            Filtered::Tooltip => {
                if let Some(tooltip) = &mut copy.tooltip
                    && let Some(TooltipContent::Text(text)) = &mut tooltip.content
                {
                    let value = passed(plugin, f, Value::Str(text.as_str().into()));
                    *text = match value {
                        Value::Str(passed) => passed.to_string(),
                        Value::Nil => String::new(),
                        other => other.to_string(),
                    };
                }
            }
            Filtered::Items => {
                if items_of(&mut copy.kind).is_some() {
                    items.push((plugin.clone(), f.clone()));
                }
            }
            Filtered::Order => {
                if !copy.children.is_empty() {
                    order.push((plugin.clone(), f.clone()));
                }
            }
        }
    }
    let shown = move |copy: Node, low: &Rc<Lowering>| match items.is_empty() {
        true => lowered(&Rc::new(copy), low),
        false => filtered_items(copy, items.clone(), &app, low, lowered.clone()),
    };
    if order.is_empty() {
        return shown(copy, low);
    }
    // Its children in the order the filters put them: a region rebuilt
    // when that order changes (what the filters read), kept while it does
    // not.
    let ids: Vec<Value> = copy
        .children
        .iter()
        .enumerate()
        .map(|(at, child)| match &child.id {
            Some(id) => Value::Str(id.clone()),
            None => Value::Num((at + 1) as f64),
        })
        .collect();
    let low = low.clone();
    switch(
        move || {
            let mut value = Value::List(Rc::from(ids.as_slice()));
            for (plugin, f) in &order {
                value = passed(plugin, f, value);
            }
            reordered(&ids, &value)
        },
        move |places: Vec<usize>, _| {
            let mut copy = copy.clone();
            copy.children = places.iter().map(|&at| copy.children[at].clone()).collect();
            shown(copy, &low)
        },
    )
}
/// The places of `ids` in the order `value` lists them: those it names
/// first, the rest after them as they were (an order filter moves
/// children; `hidden` hides them).
fn reordered(ids: &[Value], value: &Value) -> Vec<usize> {
    let mut places = Vec::with_capacity(ids.len());
    if let Value::List(listed) = value {
        for listed in listed.iter() {
            let at = ids.iter().position(|id| match (id, listed) {
                (Value::Str(a), Value::Str(b)) => a == b,
                (Value::Num(a), Value::Num(b)) => a == b,
                _ => false,
            });
            if let Some(at) = at
                && !places.contains(&at)
            {
                places.push(at);
            }
        }
    }
    let rest: Vec<usize> = (0..ids.len()).filter(|at| !places.contains(at)).collect();
    places.extend(rest);
    places
}
/// The state a `ui.each` or a `ui.list` with `data` shows, if it is one.
fn items_of(kind: &mut Kind) -> Option<&mut Signal<Value>> {
    match kind {
        Kind::Each { items, .. } => Some(items),
        Kind::List {
            items: Some(items), ..
        } => Some(items),
        _ => None,
    }
}
/// `copy` lowered with what its `ui.each` or `ui.list` shows passed
/// through `items`' filters: a state of the app's, kept current while
/// the element is there, written when the list or what the filters read
/// changes.
fn filtered_items(
    mut copy: Node,
    items: Vec<(std::rc::Weak<Vm>, mlua::Function)>,
    app: &Rc<Vm>,
    low: &Rc<Lowering>,
    lowered: impl Fn(&Rc<Node>, &Rc<Lowering>) -> View + 'static,
) -> View {
    let (low, runtime) = (low.clone(), app.runtime.clone());
    component(move |cx| {
        let Some(list) = items_of(&mut copy.kind) else {
            unreachable!("items filter lists")
        };
        let (original, shown) = (list.clone(), runtime.signal(Value::Nil));
        let out = shown.clone();
        let effect = runtime.effect_now(move || {
            let mut value = original.get();
            for (plugin, f) in &items {
                value = passed(plugin, f, value);
            }
            out.set(value);
        });
        cx.retain(effect);
        *list = shown;
        lowered(&Rc::new(copy), &low)
    })
}

pub(crate) struct Lowering {
    /// What each slot shows: its contributions and who made them.
    pub slots: crate::node::Slots,
    /// What plugins add at the app's elements.
    pub hooks: Option<Box<Hooks>>,
    /// How many `private` elements the lowering running now is inside:
    /// nothing in them is a plugin's target.
    pub sealed: std::cell::Cell<u32>,
    pub actions: Actions,
    pub elements: Rc<crate::element::Elements>,
    pub runtime: zgx::reactive::Runtime,
    pub assets: Rc<Assets>,
    /// The window's layout inspector, once the host view mounted: nodes
    /// with a recorded source are labelled for it while it records.
    pub inspector: Rc<std::cell::RefCell<Option<zgx::inspect::Inspector>>>,
    /// How deep the lowering running now is (`zgx::deep`).
    pub depth: std::cell::Cell<u32>,
    /// The app's commands, for a command palette listing them.
    #[cfg(feature = "components")]
    pub commands: Rc<crate::command::Commands>,
}

pub(crate) fn lower(node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    let depth = low.depth.get();
    low.depth.set(depth + 1);
    let view = zgx::deep(depth, || lower_node(node, low));
    low.depth.set(depth);
    view
}
fn lower_node(node: &Rc<Node>, low: &Rc<Lowering>) -> View {
    let Some(vm) = node.owner.upgrade() else {
        return div();
    };
    let view = if node.needs_context() {
        match low.sealed.get() {
            0 => {
                let (node, low, vm) = (node.clone(), low.clone(), vm.clone());
                component(move |cx| build(&node, &vm, &low, Some(cx)))
            }
            sealed => sealed_component(node, &vm, low, sealed),
        }
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
    let view = match &node.theme {
        None => view,
        Some(spec) => themed(spec, vm, view),
    };
    match &node.provide {
        None => view,
        // Its values over what its ancestors were given, for the
        // components and rows built under it (`ui.use_context`).
        Some(own) => {
            let own = own.clone();
            zgx::compose::provide_with(
                move |cx| {
                    let mut all = cx
                        .try_service::<crate::node::Provided>()
                        .map(|above| above.0.clone())
                        .unwrap_or_default();
                    all.extend(own.iter().cloned());
                    crate::node::Provided(all)
                },
                view,
            )
        }
    }
}

/// `anchor` with its tooltip: zgx's own (`View::tooltip`), its content the
/// script's node or text in the theme's look.
fn tooltip(anchor: View, tip: &crate::node::Tooltip, low: &Rc<Lowering>) -> View {
    use crate::node::TooltipContent;
    let mut core: zgx::compose::Tooltip = match tip.content.as_ref().expect("a tooltip's content") {
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
            zgx::theme::ThemeOverride::new()
        })
    };
    match spec {
        ThemeSpec::Fixed(over) => zgx::compose::theme_scope(over.clone(), view),
        ThemeSpec::State(state) => {
            let (state, vm) = (state.clone(), vm.clone());
            zgx::compose::theme_scope_with(move || compute(state.get(), &vm), view)
        }
        ThemeSpec::Computed(f) => {
            let (f, vm) = (f.clone(), vm.clone());
            zgx::compose::theme_scope_with(
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
fn token_field(prop: Prop) -> Option<zgx::theme::Field> {
    use Prop::*;
    use zgx::theme::Field;
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
/// told to `on_change` (the function's answer to an edit, a number
/// formatted, waits until it is left). A change from elsewhere while it is
/// focused (a send clearing the draft) shows at once. Enter, or leaving it
/// changed, tells `on_commit`, and it shows the function's text again. With
/// the listener that does that.
fn controlled_input(
    cx: &mut Context,
    shown: &Source,
    change: Option<Handler>,
    commit: Option<Handler>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
) -> (
    Signal<String>,
    impl FnMut(&mut zgx::input::EventContext) + 'static,
) {
    let text = cx.state(String::new());
    let runtime = cx.runtime();
    // The function's latest text, and whether the input is being edited.
    let latest: Rc<std::cell::RefCell<String>> = Rc::default();
    let editing: Rc<std::cell::Cell<bool>> = Rc::default();
    // An edit was told to `on_change`: the function's next change is its
    // answer, not a change from elsewhere.
    let echo: Rc<std::cell::Cell<bool>> = Rc::default();
    let (source, svm) = (resolve_owned(shown), vm.clone());
    let (t, l, e, o) = (text.clone(), latest.clone(), editing.clone(), echo.clone());
    cx.retain(runtime.effect(move || {
        let now = resolve_value(&source, &svm).to_string();
        let changed = *l.borrow() != now;
        *l.borrow_mut() = now.clone();
        if !e.get() || (!o.replace(false) && changed) {
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
                echo.set(true);
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
    let listener = move |cx: &mut zgx::input::EventContext| {
        if cx.phase == EventPhase::Capture || cx.target != cx.current_target {
            return;
        }
        let commit_now = |cx: &mut zgx::input::EventContext| {
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
                key: zgx::input::Key::Enter,
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

/// `on_commit(text)` for an input bound to a state (`value = state`): on
/// Enter, and on leaving it changed since it was focused, as a controlled
/// input's.
#[cold]
#[inline(never)]
fn state_commit(
    value: Signal<Value>,
    commit: Handler,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
) -> impl FnMut(&mut zgx::input::EventContext) + 'static {
    let (vm, actions) = (vm.clone(), low.actions.clone());
    // What it held as it was focused, or last committed.
    let mut since: Option<Value> = None;
    move |cx: &mut zgx::input::EventContext| {
        if cx.phase == EventPhase::Capture || cx.target != cx.current_target {
            return;
        }
        match &cx.event {
            InputEvent::Focus => since = Some(value.with_untracked(Clone::clone)),
            InputEvent::KeyDown {
                key: zgx::input::Key::Enter,
                modifiers,
                ..
            } if !modifiers.shift => {
                let now = value.with_untracked(Clone::clone);
                let text = now.to_string();
                since = Some(now);
                run(&commit, &vm, &actions, text);
            }
            InputEvent::Blur => {
                let now = value.with_untracked(Clone::clone);
                if since.take().is_some_and(|was| was != now) {
                    run(&commit, &vm, &actions, now.to_string());
                }
            }
            _ => {}
        }
    }
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
    /// What the row is known by: its name, and for a row built again when
    /// its item changes, its generation (`EachRow::identity_of`).
    identity: Rc<str>,
    /// A live row's item, as the state its builder was given.
    item: Option<Signal<Value>>,
    /// Seen in this pass.
    seen: bool,
}
impl EachRow {
    /// A row named `name` showing `item`, known by `name` if `live` (its
    /// item is state it is given) or else by `name` and its generation.
    fn new(name: &str, item: Value, live: bool) -> Self {
        let mut row = Self {
            last: item,
            generation: 0,
            identity: Rc::from(""),
            item: None,
            seen: true,
        };
        row.identity = row.identity_of(name, live);
        row
    }
    fn identity_of(&self, name: &str, live: bool) -> Rc<str> {
        if live {
            Rc::from(name)
        } else {
            Rc::from(format!("{name}@{}", self.generation))
        }
    }
    /// Its item changed: a row built again (a new generation).
    fn changed(&mut self, name: &str, item: Value) {
        self.last = item;
        self.generation += 1;
        self.identity = self.identity_of(name, false);
    }
}
/// A `ui.each`'s rows by name.
type Rows = ContentMap<Rc<str>, EachRow>;
/// A `ui.each`'s rows by identity: each row's index and row name.
type Noted = ContentMap<Rc<str>, (usize, Rc<str>)>;
/// A keyed `ui.each`'s last key pass: what the next one checks first.
struct LastPass {
    items: Vec<Value>,
    keys: Vec<String>,
    identities: Vec<Rc<str>>,
}
impl LastPass {
    /// The rows' identities if `items` keeps the last pass's keys in their
    /// order, each changed item passed on (a live row's state set, another
    /// row's generation bumped): `None` if a key moved, came or went, and
    /// the full pass decides.
    #[allow(clippy::too_many_arguments)]
    fn same_keys(
        &mut self,
        items: &[Value],
        key: &mlua::Function,
        vm: &Vm,
        rows: &mut Rows,
        noted: &mut Noted,
        live: bool,
        runtime: &zgx::reactive::Runtime,
    ) -> Option<Vec<Rc<str>>> {
        if items.len() != self.items.len() {
            return None;
        }
        let same = |a: &Value, b: &Value| {
            let id = crate::value::identity(a);
            if id != 0 {
                id == crate::value::identity(b)
            } else {
                a == b
            }
        };
        // The changed items first: one whose key changed sends the list to
        // the full pass before anything is touched.
        let mut changed = Vec::new();
        for (i, (now, then)) in items.iter().zip(&self.items).enumerate() {
            if same(now, then) {
                continue;
            }
            let k = match vm.call::<Value>(key, now.clone()) {
                Ok(k) => k.to_string(),
                Err(_) => return None,
            };
            if k != self.keys[i] {
                return None;
            }
            changed.push(i);
        }
        for i in changed {
            let identity = &self.identities[i];
            let (_, row) = noted.get(identity).cloned()?;
            let item = items[i].clone();
            let state = rows.get_mut(&row)?;
            if live {
                if let Some(signal) = &state.item {
                    let signal = signal.clone();
                    runtime.untracked(|| signal.set(item.clone()));
                }
            } else {
                state.changed(&row, item.clone());
                let next = state.identity.clone();
                noted.remove(identity);
                noted.insert(next.clone(), (i, row));
                self.identities[i] = next;
            }
            self.items[i] = item;
        }
        Some(self.identities.clone())
    }
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
        Value::Text(t) => Quick::Str(t.to_cow().into()),
        Value::List(l) => Quick::At(Rc::as_ptr(l).cast::<()>() as usize),
        Value::Map(m) => Quick::At(Rc::as_ptr(m).cast::<()>() as usize),
        Value::Element(e) => Quick::At(Rc::as_ptr(&e.0).cast::<()>() as usize),
        Value::Image(image) => Quick::At(std::sync::Arc::as_ptr(image).cast::<()>() as usize),
    }
}
/// Hashes items' contents to match rows (equal contents are compared
/// after): a word at a time, a multiply each, as rustc's FxHasher does,
/// where SipHash spent more than the comparisons it saves.
#[derive(Default, Clone, Copy)]
struct ContentHasher(u64);
impl ContentHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}
impl std::hash::Hasher for ContentHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}
type ContentMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<ContentHasher>>;
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
    order: Vec<Rc<str>>,
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
    rows: &mut Rows,
    state: &mut Unkeyed,
    live: bool,
    runtime: &zgx::reactive::Runtime,
) -> Vec<Rc<str>> {
    let mut names: Vec<Option<Rc<str>>> = vec![None; list.len()];
    for (index, item) in list.iter().enumerate() {
        if let Some(name) = state.order.get(index)
            && rows[name].last == *item
        {
            rows.get_mut(name).unwrap().seen = true;
            names[index] = Some(name.clone());
        }
    }
    let free: Vec<&Rc<str>> = state
        .order
        .iter()
        .filter(|name| !rows[*name].seen)
        .collect();
    if !free.is_empty() && names.iter().any(Option::is_none) {
        // Earlier rows first: equal items keep their order.
        let mut by_item: ContentMap<Quick, Vec<Rc<str>>> = ContentMap::default();
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
            let mut hasher = ContentHasher::default();
            content_hash(item, &mut hasher);
            std::hash::Hasher::finish(&hasher)
        };
        let mut by_contents: ContentMap<u64, Vec<Rc<str>>> = ContentMap::default();
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
                if !live {
                    row.changed(name, item.clone());
                } else {
                    if let Some(signal) = &row.item {
                        let (signal, item) = (signal.clone(), item.clone());
                        runtime.untracked(|| signal.set(item));
                    }
                    row.last = item.clone();
                }
                row.seen = true;
                name.clone()
            }
            None => {
                state.next += 1;
                let name: Rc<str> = Rc::from(state.next.to_string());
                rows.insert(name.clone(), EachRow::new(&name, item.clone(), live));
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
    build_scoped_seeing(vm, cx, low, f, args, |_| {})
}

/// A component given an `id` lays out as its root under that id: one set on
/// the root inside goes unseen, usually for a prop meant as data.
#[cold]
fn warn_replaced_id(vm: &Vm, given: &str, site: Option<&str>, root: &mlua::Value) {
    let mlua::Value::UserData(root) = root else {
        return;
    };
    let Ok(root) = root.borrow::<NodeUd>() else {
        return;
    };
    let Some(own) = root.0.id.as_deref().filter(|own| *own != given) else {
        return;
    };
    let Some(env) = vm.lua.app_data_ref::<crate::node::Env>() else {
        return;
    };
    let at = site.map_or(String::new(), |site| format!("{site}: "));
    let inside = root
        .0
        .source
        .as_deref()
        .map_or(String::new(), |site| format!(" ({site})"));
    crate::warning(
        Some(&env.warnings),
        format!(
            "{at}this component's `id = \"{given}\"` replaces `id = \"{own}\"` set on its \
             root{inside}, which `#{own}` no longer finds. An `id` prop names the element: give \
             data another name (`issue = {given}`)."
        ),
    );
}

/// `build_scoped`, with `see` given what `f` built.
fn build_scoped_seeing(
    vm: &Rc<Vm>,
    cx: &mut Context,
    low: &Rc<Lowering>,
    f: &mlua::Function,
    args: impl mlua::IntoLuaMulti,
    see: impl FnOnce(&mlua::Value),
) -> View {
    let scope = Rc::new(Scope {
        provided: cx.try_service::<crate::node::Provided>(),
        ..Scope::default()
    });
    let previous = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .map(|env| env.scope.replace(Some(scope.clone())));
    let result = vm.call::<mlua::Value>(f, args);
    if let Ok(root) = &result {
        see(root);
    }
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
    use zgx::web_view::{WebViewEvent, web_view, web_view_src};
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
            contain,
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
            let view = if *contain {
                view.overscroll_contain()
            } else {
                view
            };
            if *scrollbar {
                view.scrollbar(true)
            } else {
                view
            }
        }
        Kind::Text(text) => lower_text(text, vm),
        Kind::Markdown { text, options } => {
            let (text, vm) = (copy_text(text), vm.clone());
            crate::markdown::view::view(move || render_text(&text, &vm), options.clone())
        }
        Kind::Rich(spans) => lower_rich(spans, vm, low),
        Kind::Image { .. } => lower_image(node, vm, low),
        Kind::Svg { label, src } => {
            let (src, vm2, low2) = (resolve_owned(src), vm.clone(), low.clone());
            let theme = cx.as_deref().map(Context::theme);
            // The tint may be state or computed: the icon re-tints when it changes.
            let tint = node
                .props
                .iter()
                .find(|(p, _)| *p == Prop::Tint)
                .map(|(_, source)| resolve_owned(source));
            let sweeper = low.assets.clone();
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
            .on_mount(move |handle| handle.retain(sweeper.sweeper()))
        }
        Kind::WebView(spec) => web_view_of(spec, vm, low),
        Kind::Terminal(term) => zgx::terminal::terminal(term),
        #[cfg(feature = "scene3d")]
        Kind::Scene3d(spec) => crate::scene3d::lower(spec, vm, &low.assets, need(&mut cx)),
        Kind::Canvas(draw, options) => {
            let (draw, vm) = (draw.clone(), vm.clone());
            let theme = match cx.as_deref().map(Context::theme) {
                Some(theme) => theme,
                None => zgx::theme::Themes::of(&low.runtime).root().clone(),
            };
            let (drawn_theme, options_vm) = (theme.clone(), vm.clone());
            let each = options.as_ref().and_then(|o| o.each.clone());
            let mut kept = KeptItems::default();
            let view = canvas(move |(w, h)| {
                // One drawing's commands, as recorded from `draw(c, ...)`.
                let record = |draw: &mlua::Function, args: Option<&Value>| {
                    let recorder = crate::canvas::Recorder(Default::default());
                    let result = vm.lua.create_userdata(recorder).and_then(|ud| {
                        crate::canvas::draw_in(drawn_theme.clone(), || match args {
                            None => vm.call::<()>(draw, (ud.clone(), w, h)),
                            Some(item) => vm.call::<()>(draw, (ud.clone(), item.clone(), w, h)),
                        })?;
                        ud.take::<crate::canvas::Recorder>()
                    });
                    match result {
                        Ok(recorder) => Some(recorder.0.into_inner()),
                        Err(error) => {
                            vm.report(error);
                            None
                        }
                    }
                };
                let mut drawn = draw
                    .as_ref()
                    .and_then(|draw| record(draw, None))
                    .unwrap_or_default();
                if let Some((items, draw_item)) = &each {
                    // Each item's drawing kept by its identity: an item not
                    // written since (`set_key`, `patch` and `push` leave the
                    // others as they were) is not drawn again. Its draw is
                    // untracked: it reads its item alone.
                    let list = items.with(Value::clone);
                    if kept.size != (w, h) {
                        kept.size = (w, h);
                        kept.items.clear();
                    }
                    // Last pass's, in a map kept for its room.
                    let mut last = std::mem::take(&mut kept.last);
                    std::mem::swap(&mut last, &mut kept.items);
                    for item in list.list() {
                        let id = crate::value::identity(item);
                        let commands = match last.remove(&id).filter(|_| id != 0) {
                            Some((_, commands)) => commands,
                            None => match vm.runtime.untracked(|| record(draw_item, Some(item))) {
                                Some(commands) => commands,
                                None => continue,
                            },
                        };
                        drawn.extend(&commands);
                        if id != 0 {
                            kept.items.insert(id, (item.clone(), commands));
                        }
                    }
                    // Items gone since: their drawings with them.
                    last.clear();
                    kept.last = last;
                }
                drawn
            });
            match options {
                None => view,
                Some(options) => canvas_options(view, options, options_vm, theme),
            }
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
            rare,
            select_on_focus,
            line_numbers,
            secure,
            input_mode,
            code,
        } => {
            let (placeholder_color, caret_color, caret_width, spellcheck, active) =
                match rare.as_deref() {
                    Some(rare) => (
                        rare.placeholder_color.as_ref(),
                        rare.caret_color.as_ref(),
                        rare.caret_width,
                        rare.spellcheck.as_ref(),
                        rare.active_descendant.as_ref(),
                    ),
                    None => (None, None, None, None, None),
                };
            // What `find` searches: the state the text is in.
            let searched = match value {
                crate::node::InputValue::State { value, .. } => Some(value.clone()),
                crate::node::InputValue::Shown { .. } => None,
            };
            let mut committing = None;
            let (value, controlled): (zgx::widgets::EditorValue, _) = match value {
                // A long document is shared with the editor, never copied
                // on its way in or out.
                crate::node::InputValue::State { value, commit } => (
                    {
                        if let Some(commit) = commit {
                            committing = Some(state_commit(value.clone(), commit.clone(), vm, low));
                        }
                        bridge(
                            need(&mut cx),
                            value,
                            |v| match v {
                                Value::Text(t) => TextValue::Long(t.clone()),
                                Value::Str(s) => TextValue::from(&**s),
                                v => TextValue::Flat(v.to_string()),
                            },
                            |t| match t {
                                TextValue::Flat(s) => Value::Str(s.into()),
                                TextValue::Long(t) => Value::Text(t),
                            },
                        )
                        .into()
                    },
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
                    (text.into(), Some(control))
                }
            };
            let mut view = zgx::compose::editor_view(label.clone(), value, *multiline);
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
                        None => zgx::theme::Themes::of(&low.runtime).root().clone(),
                    };
                    view = view.placeholder_color_when(move || {
                        let muted = || theme.color(zgx::theme::tok::TEXT_TERTIARY);
                        match resolve(&color, Ty::Color, None, &vm) {
                            Some(Lit::Color(color)) => color,
                            Some(Lit::Token(t)) => theme.get(t).color().unwrap_or_else(muted),
                            _ => muted(),
                        }
                    });
                }
                None => {}
            }
            match caret_color {
                Some(Source::Lit(Lit::Color(color))) => view = view.caret_color(*color),
                Some(color) => {
                    // A token follows the element's theme; anything
                    // unresolved shows the text's colour.
                    let (color, vm) = (resolve_owned(color), vm.clone());
                    let theme = match cx.as_deref().map(Context::theme) {
                        Some(theme) => theme,
                        None => zgx::theme::Themes::of(&low.runtime).root().clone(),
                    };
                    view = view.caret_color_when(move || {
                        match resolve(&color, Ty::Color, None, &vm) {
                            Some(Lit::Color(color)) => Some(color),
                            Some(Lit::Token(t)) => theme.get(t).color(),
                            _ => None,
                        }
                    });
                }
                None => {}
            }
            if let Some(width) = caret_width {
                view = view.caret_width(width);
            }
            if let Some(control) = controlled {
                view = view.on_event(control);
            }
            if let Some(commit) = committing {
                view = view.on_event(commit);
            }
            if *input_mode != zgx::text_edit::InputMode::Text {
                view = view.input_mode(*input_mode);
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
            if let Some(active) = active {
                view = active_descendant(view, active, vm);
            }
            if let Some(hook) = rare.as_deref().and_then(|rare| rare.on_paste.clone()) {
                view = on_paste(view, hook, vm);
            }
            if rare.as_deref().is_some_and(|rare| rare.contain) {
                view = view.overscroll_contain();
            }
            match rare.as_deref().and_then(|rare| rare.read_only.as_ref()) {
                None | Some(Source::Lit(Lit::Bool(false))) => {}
                Some(Source::Lit(Lit::Bool(true))) => view = view.read_only(true),
                Some(read_only) => {
                    let (read_only, vm) = (resolve_owned(read_only), vm.clone());
                    view = view.read_only_when(move || {
                        resolve(&read_only, Ty::Bool, None, &vm) == Some(Lit::Bool(true))
                    });
                }
            }
            if *select_on_focus {
                view = view.select_on_focus(true);
            }
            if *line_numbers {
                view = view.line_numbers(true);
            }
            if let Some(code) = code {
                let theme = match cx.as_deref().map(Context::theme) {
                    Some(theme) => theme,
                    None => zgx::theme::Themes::of(&low.runtime).root().clone(),
                };
                view = code_editor(view, code, searched, vm, theme);
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
                        (true, _) => shown(&then, &low),
                        (false, Some(otherwise)) => shown(otherwise, &low),
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
                        (Some((_, node)), _) => shown(node, &low),
                        (None, Some(node)) => shown(node, &low),
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
            let rows: Rc<std::cell::RefCell<Rows>> = Rc::default();
            let noted: Rc<std::cell::RefCell<Noted>> = Rc::default();
            // The last pass's items, keys, rows and identities: a list whose
            // keys stay as they were (its edited items in place, the usual
            // update) is answered from them, each item an allocation's glance.
            let mut last_pass: Option<LastPass> = None;
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
                        let mut rows = rows_in_keys.borrow_mut();
                        if let (Some(last), Some(key_fn)) = (&mut last_pass, &key_fn)
                            && let Some(identities) = last.same_keys(
                                list.list(),
                                key_fn,
                                &key_vm,
                                &mut rows,
                                &mut noted,
                                live,
                                &runtime,
                            )
                        {
                            return identities;
                        }
                        noted.clear();
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
                                    let identity = rows[&row].identity.clone();
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
                                let name = rows.get_key_value(row.as_str()).map(|(n, _)| n.clone());
                                let found = name.and_then(|n| Some((n.clone(), rows.get_mut(&n)?)));
                                let (row, identity) = match found {
                                    Some((name, state)) if live => {
                                        state.seen = true;
                                        if let Some(signal) = &state.item {
                                            let item = item.clone();
                                            let signal = signal.clone();
                                            runtime.untracked(|| signal.set(item));
                                        }
                                        (name, state.identity.clone())
                                    }
                                    Some((name, state)) => {
                                        state.seen = true;
                                        if state.last != *item {
                                            state.changed(&name, item.clone());
                                        }
                                        (name, state.identity.clone())
                                    }
                                    None => {
                                        let name: Rc<str> = Rc::from(row);
                                        let state = EachRow::new(&name, item.clone(), live);
                                        let identity = state.identity.clone();
                                        rows.insert(name.clone(), state);
                                        (name, identity)
                                    }
                                };
                                noted.insert(identity.clone(), (i, row));
                                identity
                            })
                            .collect::<Vec<_>>();
                        // Rows gone from the list go from here too.
                        rows.retain(|_, state| std::mem::take(&mut state.seen));
                        if let Some(keys) = keys {
                            last_pass = Some(LastPass {
                                items: list.list().to_vec(),
                                keys,
                                identities: identities.clone(),
                            });
                        }
                        identities
                    })
                },
                move |identity: Rc<str>, cx| {
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
        Kind::List {
            scrollbar, rare, ..
        } => {
            let list = lower_list(&node.kind, vm, low, &mut cx);
            // Sticky rows wrap the list: it took these under the wrapper.
            if rare.as_ref().is_some_and(|rare| rare.sticky.is_some()) {
                return list;
            }
            list_chrome(list, *scrollbar, rare.as_deref(), vm)
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
            at_caret,
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
            let view = if *at_caret { view.at_caret() } else { view };
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
            let (given, site) = (node.id.clone(), node.source.clone());
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
                build_scoped_seeing(&vm, cx, &low, &builder, props, |root| {
                    if let Some(given) = &given {
                        warn_replaced_id(&vm, given, site.as_deref(), root);
                    }
                })
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
                        zgx::compose::provide(SlotContext(context.clone()), lower(n, low))
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
        match &low.hooks {
            None => {
                view = view.children(node.children.iter().map(|child| {
                    let view = lower(child, low);
                    match &flow {
                        Some(flow) if child.kind.flows() => apply_all(view, flow),
                        _ => view,
                    }
                }));
            }
            // What plugins add at an element is its sibling, or in its place.
            Some(hooks) => view = hooked_children(view, node, low, hooks, flow.as_deref()),
        }
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
                (Prop::Rounded, Lit::Num(r)) => corners = Some(zgx::decoration::Corners::all(*r)),
                (Prop::Corners, Lit::Corners(c)) => corners = Some(*c),
                _ => {}
            }
        }
        element.set_base_corners(corners);
        let shaders = node.shaders.as_deref();
        element.set_base_shader(
            shaders
                .and_then(|s| s.params)
                .map(|params| zgx::decoration::ShaderUniforms { time: 0., params }),
        );
        element.set_base_layer(
            shaders
                .and_then(|s| s.layer_params)
                .map(|params| zgx::decoration::ShaderUniforms { time: 0., params }),
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
    if let Some((_, _, low)) = &bg_image {
        view = swept(view, low);
    }
    let shaders = node.shaders.as_deref();
    let shader =
        shaders.and_then(|s| Some((s.fill.clone()?, s.params.unwrap_or_default(), s.tokens)));
    // Parameters with no fill of their own: an image's (its `characters`
    // reveal), set on its node all the same.
    let params_only = shaders.and_then(|s| s.params.filter(|_| s.fill.is_none()));
    // A layer shader, with its literal parameters unless animated (boxed:
    // the style closure below holds it, and few nodes have one).
    let layer = shaders.and_then(|s| {
        Some(Box::new((
            s.layer.clone()?,
            s.layer_params.unwrap_or_default(),
        )))
    });
    if !dynamic.is_empty()
        || element.is_some()
        || bg_image.is_some()
        || shader.is_some()
        || params_only.is_some()
        || layer.is_some()
    {
        let vm = vm.clone();
        let element = element.clone();
        view = view.reactive_style(move || {
            let styles = match &element {
                None if dynamic
                    .iter()
                    .all(|(prop, _, _)| !is_transform(*prop) && !is_corner(*prop)) =>
                {
                    // Independent properties apply as they resolve, without
                    // collecting and traversing another list of literals.
                    let mut styles = Styles::new();
                    for (prop, source, scalar) in &dynamic {
                        if let Some(lit) = resolve_style(source, *prop, scalar.as_ref(), &vm) {
                            apply(&mut styles, *prop, &lit);
                        }
                    }
                    styles
                }
                _ => {
                    // On the stack: this runs whenever what the styles read changes.
                    let lits: smallvec::SmallVec<[(Prop, Lit); 4]> = dynamic
                        .iter()
                        .filter_map(|(prop, source, scalar)| {
                            Some((*prop, resolve_style(source, *prop, scalar.as_ref(), &vm)?))
                        })
                        .collect();
                    match &element {
                        Some(element) => with_animated(element, lits.into_vec()),
                        None => apply_all(Styles::new(), &lits),
                    }
                }
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
            let styles = match layer.as_deref() {
                Some((shader, params)) => {
                    let animated = element
                        .as_ref()
                        .and_then(|e| e.animated.borrow().layer.as_deref().copied());
                    styles
                        .layer_shader(shader.clone())
                        .layer_uniforms(animated.unwrap_or(zgx::decoration::ShaderUniforms {
                            time: 0.,
                            params: *params,
                        }))
                }
                None => styles,
            };
            // A shader fill, with its literal parameters unless animated.
            match &shader {
                Some((fill, params, tokens)) => {
                    let animated = element.as_ref().and_then(|e| e.animated.borrow().shader);
                    let uniforms = animated.unwrap_or(zgx::decoration::ShaderUniforms {
                        time: 0.,
                        params: *params,
                    });
                    // Theme colours bind to their slots: resolved where the
                    // node is styled, and again when the theme changes.
                    let mut styles = styles;
                    for (slot, token) in tokens.iter().enumerate() {
                        if let Some(token) = token {
                            styles =
                                styles.token(zgx::theme::Field::ShaderParam(slot as u8), *token);
                        }
                    }
                    styles.bg_shader(fill.clone()).shader_uniforms(uniforms)
                }
                None => match params_only {
                    Some(params) => styles.shader_uniforms(
                        element
                            .as_ref()
                            .and_then(|e| e.animated.borrow().shader)
                            .unwrap_or(zgx::decoration::ShaderUniforms { time: 0., params }),
                    ),
                    None => styles,
                },
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
            None => zgx::theme::Themes::of(&low.runtime).root().clone(),
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
                    Owned::Computed(f, _) => match vm.call::<Value>(f, ()) {
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
    // Zoomable: its zoom bound to the script's number state, read and
    // written there (no context of its own to bridge it).
    if let Some(zoom) = &events.zoomable {
        let (read, written) = (zoom.clone(), zoom.clone());
        let zoom = zoom.clone();
        view = view.zoomable_with(
            Box::new(move || number(&zoom.get())),
            Box::new(move || read.with_untracked(number)),
            Box::new(move |next| {
                written.set(Value::Num(f64::from(next)));
            }),
        );
    }
    let handle: Option<Rc<OnceCell<NodeHandle>>> = (element.is_some()
        || events.any_input()
        || events.any_drag()
        || events.drag_preview.is_some())
    .then(Rc::default);
    let handle_for = || handle.clone().expect("made for what reads it");
    if events.any_input() {
        view = with_events(view, node, vm, low, handle_for());
        if let Some(rare) = &events.rare {
            view = with_rare(view, rare, vm, low, handle_for());
        }
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
            view = view.heading(usize::from(level));
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
        if a11y.radio_item {
            view = view.radio_item();
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
        if let Some(error) = &a11y.error {
            let (error, vm) = (resolve_owned(error), vm.clone());
            view = view.error_when(move || match resolve(&error, Ty::Str, None, &vm) {
                Some(Lit::Str(s)) if !s.is_empty() => Some(s.to_string()),
                _ => None,
            });
        }
        if let Some(label) = &a11y.label {
            let (label, vm) = (resolve_owned(label), vm.clone());
            let label = move || match resolve(&label, Ty::Str, None, &vm) {
                Some(Lit::Str(s)) => s.to_string(),
                _ => String::new(),
            };
            view = if follows_label(node) {
                // The component names its own inner node (a tab strip, a
                // select's button) with it.
                zgx::compose::FollowedLabel::around(label, view)
            } else {
                view.accessible_label_when(label)
            };
        }
    }
    if let Some(layout) = node.layout.as_ref().filter(|_| node.sizes_layout()) {
        view = layout_box(view, layout, vm, low);
    }
    view
}

/// Whether `node` is a library component that names an inner node with
/// its label (`FollowedLabel`), not its root.
fn follows_label(node: &Node) -> bool {
    #[cfg(feature = "components")]
    if let Kind::Library(library) = &node.kind {
        return crate::library::follows_label(library);
    }
    let _ = node;
    false
}

/// How a container lays out its children, for the regions among them that
/// hold any number (`each`, `slot`): a region is a box of its own, and lays
/// its children out as its parent would, stretched across the parent's
/// cross axis in a column. (`show`, `switch` and `presence` hold one view
/// at a time and are contents nodes: CSS `display: contents`, exactly.)
fn flow(node: &Node) -> Vec<(Prop, Lit)> {
    let scrolls = matches!(
        node.kind,
        Kind::Scroll {
            horizontal: true,
            ..
        }
    );
    let mut row = scrolls || matches!(node.kind, Kind::Row);
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
    if scrolls {
        // A scrolling row is as long as its children: the region takes its
        // height, and its own length from them.
        out.insert(0, (Prop::Direction, Lit::Str("row".into())));
        out.push((Prop::AlignSelf, Lit::Str("stretch".into())));
    } else if row {
        // In a row the region takes the row's room, so its children wrap
        // and shrink against the row's width, not their own.
        out.insert(0, (Prop::Direction, Lit::Str("row".into())));
        out.push((Prop::Grow, Lit::Num(1.)));
        out.push((Prop::Shrink, Lit::Num(1.)));
        out.push((Prop::MinW, Lit::Len(zgx::layout::Length::Px(0.))));
    } else {
        out.push((Prop::AlignSelf, Lit::Str("stretch".into())));
    }
    // A column's and a row's own default: a row of `each` fills across as
    // the same row would on its own.
    if !out.iter().any(|(prop, _)| *prop == Prop::Items) {
        out.push((Prop::Items, Lit::Str("stretch".into())));
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
                        Lit::Len(zgx::layout::Length::Px(v)) => Some(*v),
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
        Prop::RotateX,
        Prop::RotateY,
        Prop::Perspective,
        Prop::Origin,
    ],
];

fn placeholder() -> Arc<ImageData> {
    thread_local! {
        static EMPTY: Arc<ImageData> = Arc::new(ImageData::new(1, 1, vec![0, 0, 0, 0]).expect("1×1 image"));
    }
    EMPTY.with(Clone::clone)
}
/// An image's path, from the script's directory. A plugin's come from its
/// own directory or below, as its modules do, not from anywhere the user
/// can read.
pub(crate) fn resolve_path(vm: &Vm, path: &str) -> Result<std::path::PathBuf, String> {
    let given = std::path::Path::new(path);
    let resolved = match (&vm.base, given.is_relative()) {
        (Some(base), true) => base.join(given),
        _ => given.to_path_buf(),
    };
    let plugin = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .is_some_and(|env| matches!(*env.grants, crate::Grants::Only(_)));
    if !plugin {
        return Ok(resolved);
    }
    let root = vm
        .base
        .as_ref()
        .map(|base| base.canonicalize().unwrap_or_else(|_| base.clone()));
    match root {
        Some(root) if crate::within(&root, &crate::normalize(&root.join(given))) => {
            Ok(crate::normalize(&root.join(given)))
        }
        _ => Err(format!(
            "{path}: a plugin's images come from its directory or below"
        )),
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
    // A remote source is a request to the network: a plugin makes it only
    // if granted it, as `ui.fetch` (else an image's URL would carry out what
    // the plugin read).
    let fetches = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .is_none_or(|env| env.grants.allows(&crate::Capability::Fetch));
    if !fetches {
        vm.report(mlua::Error::runtime(format!(
            "{src}: a plugin shows remote images only if the host grants it \
             (`ScriptHost::grant(plugin, Capability::Fetch)`)"
        )));
        return FAILED;
    }
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
    let body = crate::calls::Body::kept();
    let future = fetch(
        crate::calls::Request {
            url: src.to_owned(),
            method: "GET".into(),
            headers: Vec::new(),
            body: None,
        },
        body.clone(),
    );
    let (assets, vm, src, runtime) = (
        low.assets.clone(),
        Rc::downgrade(vm),
        src.to_owned(),
        low.runtime.clone(),
    );
    let arriving = ready.clone();
    timers.spawn(async move {
        let bytes = match future.await {
            Ok(response) if (200..300).contains(&response.status) => Ok(body.whole(response.body)),
            Ok(response) => Err(format!("HTTP {}", response.status)),
            Err(error) => Err(error),
        };
        let decoded = match bytes {
            Ok(bytes) => {
                // Decoding a large image takes long enough to drop frames.
                let (tell, mut heard) = zgx::task::latest::<Result<Decoded, String>>();
                std::thread::spawn(move || {
                    tell.send(if svg {
                        SvgData::new(bytes)
                            .map(|svg| Decoded::Svg(Arc::new(svg)))
                            .map_err(|e| e.to_string())
                    } else {
                        zgx_gpu::assets::decode_image(&bytes)
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
                assets.keep_image(src, image);
            }
            Ok(Decoded::Svg(svg)) => {
                assets.failed.borrow_mut().remove(&src);
                assets.keep_svg(src, svg);
            }
            Err(error) => {
                // A picture that doesn't come is the network's doing, not
                // the script's: logged, and `fallback` shows in its place.
                let _ = vm;
                eprintln!("zgx: {src} didn't load: {error}");
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
        return low.assets.image(src);
    }
    if let Some(image) = low.assets.image(src) {
        return Some(image);
    }
    if is_remote(src) {
        remote_ready(vm, low, src, false);
        return None;
    }
    let decoded = resolve_path(vm, src)
        .and_then(|path| {
            low.assets
                .read(src, &path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .map(|bytes| (path, bytes))
        })
        .and_then(|(path, bytes)| {
            zgx_gpu::assets::decode_image(&bytes).map_err(|e| format!("{}: {e}", path.display()))
        });
    match decoded {
        Ok(image) => {
            low.assets.keep_image(src.to_owned(), image.clone());
            Some(image)
        }
        Err(e) => {
            let e = match literal_site(vm, src) {
                Some(site) => format!("{site}: {e}"),
                None => e,
            };
            vm.report(mlua::Error::runtime(e));
            None
        }
    }
}
/// The script line naming `src` as a string (`app.luau:12`), for its load
/// error: an image loads as its node mounts, when no script line runs.
/// Found in the script's files only once one fails, so the images that
/// load keep nothing of where they were named.
#[cold]
#[inline(never)]
fn literal_site(vm: &Vm, src: &str) -> Option<String> {
    let base = vm.base.as_ref()?;
    let quoted = [format!("\"{src}\""), format!("'{src}'")];
    let mut dirs = vec![base.clone()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "luau" || e == "lua") {
                files.push(path);
            }
        }
    }
    files.sort();
    files.iter().find_map(|file| {
        let text = std::fs::read_to_string(file).ok()?;
        let line = text
            .lines()
            .position(|line| quoted.iter().any(|q| line.contains(q.as_str())))?;
        let name = file.strip_prefix(base).unwrap_or(file);
        Some(format!("{}:{}", name.display(), line + 1))
    })
}
fn load_svg(vm: &Rc<Vm>, low: &Lowering, src: &str) -> Arc<SvgData> {
    if let Some(svg) = low.assets.svg(src) {
        return svg;
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
        None => resolve_path(vm, src).and_then(|path| {
            low.assets
                .read(src, &path)
                .map_err(|e| format!("{}: {e}", path.display()))
        }),
    };
    let parsed = bytes.and_then(|b| SvgData::new(b).map_err(|e| format!("{src}: {e}")));
    match parsed {
        Ok(svg) => {
            let svg = Arc::new(svg);
            low.assets.keep_svg(src.to_owned(), svg.clone());
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
fn modifiers_table(lua: &mlua::Lua, held: zgx::input::Modifiers) -> mlua::Result<mlua::Table> {
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

/// The handlers few nodes have (`RareEvents`), one check in `styled` for
/// them all.
#[inline(never)]
fn with_rare(
    mut view: View,
    rare: &crate::node::RareEvents,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    handle: Rc<OnceCell<NodeHandle>>,
) -> View {
    if let Some(handler) = &rare.aux_click {
        view = with_aux_click(view, handler.clone(), vm, low, handle.clone());
    }
    if let Some(handler) = &rare.navigate {
        view = with_navigate(view, handler.clone(), vm, low);
    }
    if rare.file_drop.is_some() || rare.file_hover.is_some() {
        view = with_file_drop(view, rare, vm, low, handle);
    }
    view
}

/// `on_file_drop` and `on_file_hover`: files dragged in from outside the
/// window, as react-dropzone hears them. A listener of their own, so a
/// node without them pays nothing.
#[inline(never)]
fn with_file_drop(
    view: View,
    rare: &crate::node::RareEvents,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    handle: Rc<OnceCell<NodeHandle>>,
) -> View {
    use zgx::input::{EventPhase, InputEvent};
    let (drop, hover) = (rare.file_drop.clone(), rare.file_hover.clone());
    let (vm, actions) = (vm.clone(), low.actions.clone());
    let over = std::cell::Cell::new(false);
    view.on_event(move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        let now = match &cx.event {
            InputEvent::FileHover { .. } => true,
            InputEvent::FileHoverCancelled
            | InputEvent::FilesDrop { .. }
            | InputEvent::FilesDropRejected { .. } => false,
            _ => return,
        };
        if over.replace(now) != now
            && let Some(hover) = &hover
        {
            run(hover, &vm, &actions, now);
        }
        let InputEvent::FilesDrop { x, y, paths } = &cx.event else {
            return;
        };
        let Some(drop) = &drop else { return };
        if cx.default_prevented() {
            return;
        }
        let files = Value::List(paths.iter().map(|p| crate::io::file_value(p)).collect());
        let at = handle
            .get()
            .and_then(|handle| handle.scene())
            .map(|scene| scene.borrow().transformed_bounds(cx.current_target))
            .map_or((0., 0.), |bounds| (bounds.x, bounds.y));
        let point = vm.lua.create_table().and_then(|t| {
            t.set("x", *x)?;
            t.set("y", *y)?;
            t.set("local_x", *x - at.0)?;
            t.set("local_y", *y - at.1)?;
            Ok(t)
        });
        if let Ok(point) = point {
            run(drop, &vm, &actions, (files, point));
        }
        cx.prevent_default();
        cx.stop_propagation();
    })
}

/// `on_aux_click`: a listener of its own, so a node without one pays nothing
/// for it. An aux click is a press and a release of one extra button on the
/// node, as a click is of the primary. Over a child, the release takes the
/// press going down (`armed`) and clicks coming back up, so a press a
/// child's handler took (stopping it there) is spent and never clicks a
/// later release that began elsewhere; the pointer leaving the node spends
/// it too, so a release elsewhere never leaves one behind.
#[inline(never)]
fn with_aux_click(
    view: View,
    handler: Handler,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    handle: Rc<OnceCell<NodeHandle>>,
) -> View {
    let (vm, actions) = (vm.clone(), low.actions.clone());
    let (mut pressed, mut armed) = (None::<PointerButton>, false);
    view.on_event(move |event| match &event.event {
        InputEvent::PointerDown {
            button: button @ (PointerButton::Middle | PointerButton::Other(_)),
            ..
        } if event.phase != EventPhase::Capture => pressed = Some(*button),
        InputEvent::PointerUp {
            x,
            y,
            button: button @ (PointerButton::Middle | PointerButton::Other(_)),
        } => {
            let clicked = match event.phase {
                EventPhase::Capture => {
                    armed = pressed.take() == Some(*button);
                    false
                }
                EventPhase::Target => {
                    armed = false;
                    pressed.take() == Some(*button)
                }
                _ => std::mem::take(&mut armed),
            };
            if !clicked {
                return;
            }
            let lua = &vm.lua;
            let origin = handle
                .get()
                .and_then(|h| h.scene().map(|s| s.borrow().bounds(h.node())))
                .map_or((0., 0.), |b| (b.x, b.y));
            let held = event.pointer_modifiers();
            let table = lua.create_table().and_then(|t| {
                t.set("x", *x)?;
                t.set("y", *y)?;
                t.set("local_x", *x - origin.0)?;
                t.set("local_y", *y - origin.1)?;
                t.set("shift", held.shift)?;
                t.set("ctrl", held.control)?;
                t.set("alt", held.alt)?;
                t.set("meta", held.meta)?;
                t.set("cmd", held.primary_shortcut())?;
                match button {
                    PointerButton::Other(n) => t.set("button", i64::from(*n))?,
                    _ => t.set("button", "middle")?,
                }
                Ok(t)
            });
            if let Ok(t) = table {
                // The innermost node with one takes it, as `on_click`.
                event.stop_propagation();
                run(&handler, &vm, &actions, t);
            }
        }
        InputEvent::PointerLeave => (pressed, armed) = (None, false),
        _ => {}
    })
}

/// `on_navigate`: a listener of its own, so a node without one pays nothing
/// for it; the node takes navigation (`View::on_navigate`), so a swipe
/// between pages over it navigates where nothing under the fingers scrolls
/// that way.
#[inline(never)]
fn with_navigate(view: View, handler: Handler, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    let (vm, actions) = (vm.clone(), low.actions.clone());
    view.on_navigate(move |navigation| {
        let direction = match navigation {
            zgx::input::Navigation::Back => "back",
            zgx::input::Navigation::Forward => "forward",
        };
        let table = vm.lua.create_table().and_then(|t| {
            t.set("direction", direction)?;
            Ok(t)
        });
        if let Ok(t) = table {
            run(&handler, &vm, &actions, t);
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
    use zgx::input::DragPhase;
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
        let build = move |payload: &Value| {
            let result = vm.call::<mlua::Value>(&preview, payload.clone());
            built(&vm, result, &low)
        };
        view = match (events.drag_preview_anchor, events.drag_preview_offset) {
            (None, None) => view.drag_preview(build),
            (anchor, offset) => view.drag_preview_anchored(
                anchor.unwrap_or_default(),
                offset.unwrap_or_default(),
                build,
            ),
        };
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
            use zgx::input::DragItem;
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
    let (drop, accept, over, leave, spring) = (
        events.drop.clone(),
        events.drop_when.clone(),
        events.drag_over.clone(),
        events.drag_leave.clone(),
        events.drag_spring.clone(),
    );
    if drop.is_none() && over.is_none() && leave.is_none() && spring.is_none() {
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
            // Sprang open unless it returned `false`.
            DragPhase::Spring if !cx.default_prevented() && accepts() => {
                if let Some(f) = &spring
                    && !matches!(
                        call(&vm, f, payload.clone()),
                        Some(mlua::Value::Boolean(false))
                    )
                {
                    cx.prevent_default();
                    cx.stop_propagation();
                }
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
    Computed(mlua::Function, std::cell::Cell<Option<CachedHex>>),
    Anim(Option<Value>, Option<Value>),
    Variant(Rc<crate::variants::Selection>, Prop),
}
fn resolve_owned(source: &Source) -> Owned {
    match source {
        Source::Lit(l) => Owned::Lit(l.clone()),
        Source::State(s) => Owned::State(s.clone()),
        Source::Computed(f) => Owned::Computed(f.clone(), std::cell::Cell::default()),
        Source::Anim(a) => Owned::Anim(a.from.clone(), a.to.clone()),
        Source::Variant(selection, prop) => Owned::Variant(selection.clone(), *prop),
    }
}
/// A style's value now. What a function returns is checked by the design
/// lint, unless it is `ui.raw`.
fn resolve_style(source: &Owned, prop: Prop, scalar: Option<&Signal<f32>>, vm: &Vm) -> Option<Lit> {
    if let Owned::Computed(f, _) = source
        && vm.warnings.borrow().design != crate::DesignLint::Off
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
/// A textarea's code editor props (`highlight`, `decorations`,
/// `current_line`, `tab`) as its editor takes them.
/// What a `highlight` leaves the next line, any Luau value, as the number
/// the editor keeps for each line: a whole number below 2^31 as itself
/// (the common case: no table), anything else by its place in a table of
/// its own, strings and booleans kept once each so the same state is the
/// same number (a line left as it was stops the lines after it being asked
/// again), tables and functions by identity.
#[derive(Default)]
struct LineStates {
    values: Vec<mlua::Value>,
    known: HashMap<StateKey, u32>,
}
#[derive(PartialEq, Eq, Hash)]
enum StateKey {
    Text(Vec<u8>),
    Bool(bool),
    Number(u64),
    At(usize),
}
/// What a `highlight` leaves the next line, read where it lies: a whole
/// number (most highlighters' states) as itself, with no `Value` made.
enum Next {
    Whole(u32),
    Other(mlua::Value),
}
impl mlua::FromLua for Next {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        Ok(match LineStates::whole(&value) {
            Some(number) => Self::Whole(number),
            None => Self::Other(value),
        })
    }
    unsafe fn from_stack(idx: std::ffi::c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        use mlua::ffi;
        let state = lua.state();
        // SAFETY: mlua converts a result at `idx` while it is on this
        // stack; a number's type is checked first, so `lua_tonumber`
        // converts nothing. Neither runs Lua or keeps a borrow.
        unsafe {
            match ffi::lua_type(state, idx) {
                ffi::LUA_TNIL | ffi::LUA_TNONE => return Ok(Self::Whole(0)),
                ffi::LUA_TNUMBER => {
                    let n = ffi::lua_tonumber(state, idx);
                    if (0. ..KEPT as f64).contains(&n) && n as u32 as f64 == n {
                        return Ok(Self::Whole(n as u32));
                    }
                }
                _ => {}
            }
            Ok(Self::Other(mlua::Value::from_stack(idx, lua)?))
        }
    }
}
/// A state's number from its place in `LineStates::values`.
const KEPT: u32 = 1 << 31;
impl LineStates {
    /// A state that is its own number: nil (0), or a whole number below
    /// 2^31.
    #[inline]
    fn whole(value: &mlua::Value) -> Option<u32> {
        match value {
            mlua::Value::Nil => Some(0),
            mlua::Value::Number(n) if (0. ..KEPT as f64).contains(n) && *n as u32 as f64 == *n => {
                Some(*n as u32)
            }
            mlua::Value::Integer(n) if (0..KEPT as i64).contains(n) => Some(*n as u32),
            _ => None,
        }
    }
    fn number(&mut self, value: mlua::Value) -> u32 {
        if let Some(number) = Self::whole(&value) {
            return number;
        }
        let key = match &value {
            mlua::Value::Number(n) => StateKey::Number(n.to_bits()),
            mlua::Value::Integer(n) => StateKey::Number((*n as f64).to_bits()),
            mlua::Value::Boolean(b) => StateKey::Bool(*b),
            mlua::Value::String(text) => StateKey::Text(text.as_bytes().to_vec()),
            other => StateKey::At(other.to_pointer() as usize),
        };
        if let Some(&at) = self.known.get(&key) {
            return KEPT | at;
        }
        // Few states differ in practice. A highlighter making a new table
        // for every line would grow this without end: past this many, it
        // starts again (lines left with a forgotten state are asked again).
        if self.values.len() >= 1 << 16 {
            self.values.clear();
            self.known.clear();
        }
        let at = self.values.len() as u32;
        self.values.push(value);
        self.known.insert(key, at);
        KEPT | at
    }
    fn value(&self, number: u32) -> mlua::Value {
        if number & KEPT == 0 {
            return mlua::Value::Integer(number as i64);
        }
        (self.values.get((number & !KEPT) as usize).cloned()).unwrap_or(mlua::Value::Nil)
    }
}

fn code_editor(
    mut view: View,
    code: &Rc<crate::node::CodeProps>,
    searched: Option<Signal<Value>>,
    vm: &Rc<Vm>,
    theme: Rc<zgx::theme::ThemeScope>,
) -> View {
    // A colour as scripts write them (`"#ff8800"`, `"$accent"`), each
    // distinct one parsed once while the theme's tokens stay the same: a
    // theme switch (which asks the highlighter anew) resolves them again.
    type Colors = (u64, HashMap<Rc<str>, Option<zgx::scene::Color>>);
    let colors: Rc<std::cell::RefCell<Colors>> = Rc::default();
    let color_of = {
        let theme = theme.clone();
        move |text: &str| -> Option<zgx::scene::Color> {
            {
                let mut colors = colors.borrow_mut();
                let revision = theme.revision();
                if colors.0 != revision {
                    *colors = (revision, HashMap::new());
                } else if let Some(color) = colors.1.get(text) {
                    return *color;
                }
            }
            let color = match text.strip_prefix('$') {
                Some(token) => theme.get(zgx::theme::token(token)).color(),
                None => crate::node::parse_color(text),
            };
            colors.borrow_mut().1.insert(Rc::from(text), color);
            color
        }
    };
    let color_of = Rc::new(color_of);
    // `{ { from, to, color }, … }`, from and to bytes counted from 1 (as
    // `string.find` gives them), `to` the last.
    let ranges = {
        let color_of = color_of.clone();
        move |list: &mlua::Table, offset: usize, color: Option<zgx::scene::Color>| {
            let mut out = Vec::with_capacity(list.raw_len());
            for span in list.sequence_values::<mlua::Table>().flatten() {
                let (Ok(from), Ok(to)) = (span.raw_get::<f64>(1), span.raw_get::<f64>(2)) else {
                    continue;
                };
                let shade = match span.raw_get::<Option<mlua::LuaString>>(3) {
                    Ok(Some(text)) => text.to_str().ok().and_then(|text| color_of(&text)),
                    _ => color,
                };
                if let Some(shade) = shade
                    && from >= 1.
                    && to >= from
                {
                    out.push((offset + from as usize - 1..offset + to as usize, shade));
                }
            }
            out
        }
    };
    let ranges = Rc::new(ranges);
    // A highlighter's `{ { from, to, color?, weight =, italic =, strike =,
    // bg = } }`: positional as `ranges`, its styles by name.
    let spans = {
        let color_of = color_of.clone();
        move |list: &mlua::Table| {
            let mut out = Vec::with_capacity(list.raw_len());
            for span in list.sequence_values::<mlua::Table>().flatten() {
                // One walk of the span, its keys and strings read where they
                // lie: a colour-only `{ from, to, color }` is three steps, its
                // styles read only where it has them.
                let (mut from, mut to) = (f64::NAN, f64::NAN);
                let mut style = zgx::widgets::SpanStyle::default();
                let number = |value: &mlua::StackValue| {
                    (value.type_name() == "number")
                        .then(|| value.get::<f64>().ok())
                        .flatten()
                };
                let yes = |value: &mlua::StackValue| {
                    value.type_name() == "boolean" && value.get::<bool>().unwrap_or(false)
                };
                let walked = span.for_each_borrowed(|key, value| {
                    match key.as_bytes() {
                        None => match number(&key) {
                            Some(1.) => from = number(&value).unwrap_or(f64::NAN),
                            Some(2.) => to = number(&value).unwrap_or(f64::NAN),
                            Some(3.) => style.color = value.as_str().and_then(&*color_of),
                            _ => {}
                        },
                        Some(b"weight") => {
                            style.weight = number(&value).map(|w| w.clamp(1., 1000.) as u16);
                        }
                        Some(b"italic") => style.italic = yes(&value),
                        Some(b"strike") => style.strike = yes(&value),
                        Some(b"bg") => style.background = value.as_str().and_then(&*color_of),
                        Some(_) => {}
                    }
                    Ok(())
                });
                if walked.is_err() || !(from >= 1. && to >= from) {
                    continue;
                }
                if style != zgx::widgets::SpanStyle::default() {
                    out.push((from as usize - 1..to as usize, style));
                }
            }
            out
        }
    };
    if let Some(highlight) = &code.highlight {
        let (highlight, vm) = (highlight.clone(), vm.clone());
        let states = std::cell::RefCell::new(LineStates::default());
        view = view.span_highlighter(move |line, state| {
            // A whole number is passed as itself (most highlighters' states,
            // no table touched); anything else from the side table.
            let called = if state & KEPT == 0 {
                vm.call::<(Option<mlua::Table>, Next)>(&highlight, (line, state))
            } else {
                let before = states.borrow().value(state);
                vm.call::<(Option<mlua::Table>, Next)>(&highlight, (line, before))
            };
            match called {
                Ok((list, next)) => (
                    list.map_or_else(Vec::new, |list| spans(&list)),
                    match next {
                        Next::Whole(number) => number,
                        Next::Other(value) => states.borrow_mut().number(value),
                    },
                ),
                Err(error) => {
                    vm.report(error);
                    (Vec::new(), 0)
                }
            }
        });
    }
    // Ranges a script gives (`decorations`), and a search's matches
    // (`find`), found where the text is held.
    let underline_colors = color_of.clone();
    let given = code.decorations.clone().map(|decorations| {
        let vm = vm.clone();
        let default = theme.clone();
        move || {
            let fallback = Some(default.color(zgx::theme::tok::SELECTION));
            let list = match &decorations {
                crate::node::Readable::State(state) => {
                    state.with(|value| crate::value::to_lua(value, &vm.lua, false).ok())
                }
                crate::node::Readable::Function(f) => vm.call::<mlua::Value>(f, ()).ok(),
            };
            match list {
                Some(mlua::Value::Table(list)) => ranges(&list, 0, fallback),
                _ => Vec::new(),
            }
        }
    });
    let found = code.find.as_ref().zip(searched).map(|(find, text)| {
        let query = resolve_owned(&find.query);
        let case = find.case.as_ref().map(resolve_owned);
        let current = find.current.as_ref().map(resolve_owned);
        let colors = (
            find.color.as_ref().map(resolve_owned),
            find.current_color.as_ref().map(resolve_owned),
        );
        let (limit, vm, theme) = (find.limit.unwrap_or(usize::MAX), vm.clone(), theme.clone());
        move || {
            let Some(Lit::Str(query)) = resolve(&query, Ty::Str, None, &vm) else {
                return Vec::new();
            };
            if query.is_empty() {
                return Vec::new();
            }
            let case = case
                .as_ref()
                .is_none_or(|c| resolve(c, Ty::Bool, None, &vm) != Some(Lit::Bool(false)));
            let current = match current
                .as_ref()
                .and_then(|c| resolve(c, Ty::Num, None, &vm))
            {
                Some(Lit::Num(n)) if n >= 1. => n as usize,
                _ => 0,
            };
            let color = |source: &Option<Owned>, token| match source
                .as_ref()
                .and_then(|c| resolve(c, Ty::Color, None, &vm))
            {
                Some(Lit::Color(color)) => color,
                Some(Lit::Token(t)) => theme.get(t).color().unwrap_or_else(|| theme.color(token)),
                _ => theme.color(token),
            };
            let (marked, strong) = (
                color(&colors.0, zgx::theme::tok::SELECTION),
                color(&colors.1, zgx::theme::tok::WARNING),
            );
            let (found, _) = text.with(|value| {
                crate::value::find_all(
                    &crate::value::string_pieces(value),
                    query.as_bytes(),
                    case,
                    limit,
                )
            });
            found
                .into_iter()
                .enumerate()
                .map(|(i, (from, to))| (from..to, if i + 1 == current { strong } else { marked }))
                .collect()
        }
    });
    match (given, found) {
        (Some(given), Some(found)) => {
            view = view.decorations(move || {
                let mut all = given();
                all.extend(found());
                all
            });
        }
        (Some(given), None) => view = view.decorations(given),
        (None, Some(found)) => view = view.decorations(found),
        (None, None) => {}
    }
    if let Some(underlines) = code.underlines.clone() {
        let (vm, theme, color_of) = (vm.clone(), theme.clone(), underline_colors);
        view = view.underlines(move || {
            let list = match &underlines {
                crate::node::Readable::State(state) => {
                    state.with(|value| crate::value::to_lua(value, &vm.lua, false).ok())
                }
                crate::node::Readable::Function(f) => vm.call::<mlua::Value>(f, ()).ok(),
            };
            let Some(mlua::Value::Table(list)) = list else {
                return Vec::new();
            };
            let mut out = Vec::with_capacity(list.raw_len());
            for span in list.sequence_values::<mlua::Table>().flatten() {
                // By name, or `{ from, to, color }` as `decorations` are.
                let number = |name: &str, at: usize| {
                    (span.raw_get::<Option<f64>>(name).ok().flatten())
                        .or_else(|| span.raw_get::<Option<f64>>(at).ok().flatten())
                };
                let (Some(from), Some(to)) = (number("from", 1), number("to", 2)) else {
                    continue;
                };
                if from < 1. || to < from {
                    continue;
                }
                let color = span
                    .raw_get::<Option<String>>("color")
                    .ok()
                    .flatten()
                    .or_else(|| span.raw_get::<Option<String>>(3).ok().flatten())
                    .and_then(|text| color_of(&text))
                    .unwrap_or_else(|| theme.color(zgx::theme::tok::DANGER));
                let line = (span.raw_get::<Option<String>>("style").ok().flatten())
                    .and_then(|style| crate::node::line_style(&style).ok())
                    .unwrap_or(zgx::rich_text::LineStyle::Wavy);
                out.push(zgx::widgets::EditorUnderline {
                    range: from as usize - 1..to as usize,
                    color,
                    line,
                });
            }
            out
        });
    }
    if let Some(color) = &code.current_line {
        let (color, vm, theme) = (resolve_owned(color), vm.clone(), theme.clone());
        view = view.current_line(move || match resolve(&color, Ty::Color, None, &vm) {
            Some(Lit::Color(color)) => Some(color),
            Some(Lit::Token(t)) => theme.get(t).color(),
            _ => None,
        });
    }
    if let Some(tab) = &code.tab {
        view = view.tab_indent(tab.clone());
    }
    if let Some(told) = &code.on_caret {
        let (told, vm) = (told.clone(), vm.clone());
        view = view.on_caret(move |caret| {
            let args = (caret.line, caret.column, caret.anchor, caret.focus);
            if let Err(error) = vm.call::<()>(&told, args) {
                vm.report(error);
            }
        });
    }
    view
}
/// An exact short hex key, copied while its Lua string is on the stack.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct ShortHex {
    bytes: [u8; 9],
    len: u8,
}
impl ShortHex {
    fn new(text: &str) -> Option<Self> {
        if !text.starts_with('#') || !matches!(text.len(), 4 | 5 | 7 | 9) {
            return None;
        }
        let mut hex = Self {
            len: text.len() as u8,
            ..Self::default()
        };
        hex.bytes[..text.len()].copy_from_slice(text.as_bytes());
        Some(hex)
    }
}
#[derive(Clone, Copy)]
struct CachedHex {
    hex: ShortHex,
    color: Option<zgx::scene::Color>,
}
fn cached_hex(
    hex: ShortHex,
    cache: &std::cell::Cell<Option<CachedHex>>,
) -> Option<zgx::scene::Color> {
    if let Some(cached) = cache.get()
        && cached.hex == hex
    {
        return cached.color;
    }
    let color = std::str::from_utf8(&hex.bytes[..hex.len as usize])
        .ok()
        .and_then(crate::node::parse_color);
    cache.set(Some(CachedHex { hex, color }));
    color
}

/// A computed colour copied or parsed while on the Lua stack, or a value
/// (a token, `currentcolor`, invalid UTF-8, a number) to convert as before.
enum ComputedColor {
    Hex(ShortHex),
    Parsed(Option<zgx::scene::Color>),
    Other(mlua::Value),
}
impl ComputedColor {
    fn parse(text: &str) -> Option<Self> {
        (!text.starts_with('$') && !text.eq_ignore_ascii_case("currentcolor")).then(|| {
            match ShortHex::new(text) {
                Some(hex) => Self::Hex(hex),
                None => Self::Parsed(crate::node::parse_color(text)),
            }
        })
    }
}
impl mlua::FromLua for ComputedColor {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        if let mlua::Value::String(s) = &value
            && let Ok(text) = s.to_str()
            && let Some(parsed) = Self::parse(&text)
        {
            return Ok(parsed);
        }
        Ok(Self::Other(value))
    }
    unsafe fn from_stack(idx: std::ffi::c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        use mlua::ffi;
        let state = lua.state();
        // SAFETY: mlua converts a result at `idx` while it is on this
        // stack. A string's type is checked first, so `lua_tolstring`
        // converts nothing and allocates nothing (the collector does not
        // run); its bytes stay where they are, held by the stack, while
        // they are copied or parsed. Neither runs Lua or keeps a borrow.
        unsafe {
            if ffi::lua_type(state, idx) == ffi::LUA_TSTRING {
                let mut len = 0;
                let bytes = ffi::lua_tolstring(state, idx, &mut len);
                let bytes = std::slice::from_raw_parts(bytes.cast::<u8>(), len);
                if let Ok(text) = std::str::from_utf8(bytes)
                    && let Some(parsed) = Self::parse(text)
                {
                    return Ok(parsed);
                }
            }
            Ok(Self::Other(mlua::Value::from_stack(idx, lua)?))
        }
    }
}
/// A value `lit` wouldn't take: a track list that can't be read says why,
/// where a literal one would have stopped the script (`node::source`).
#[cold]
#[inline(never)]
fn refused(value: &Value, ty: Ty, vm: &Vm) -> Option<Lit> {
    if ty == Ty::Tracks
        && let Value::Str(text) = value
        && let Err(why) = text.parse::<zgx::grid_tracks::GridTracks>()
    {
        vm.report(mlua::Error::runtime(format!("a grid's tracks: {why}")));
    }
    if ty == Ty::Len
        && let Value::Str(text) = value
        && let Err(why) = text.parse::<zgx::size_calc::SizeCalc>()
    {
        vm.report(mlua::Error::runtime(format!("a length: {why}")));
    }
    None
}
fn resolve(source: &Owned, ty: Ty, scalar: Option<&Signal<f32>>, vm: &Vm) -> Option<Lit> {
    match source {
        Owned::Lit(l) => Some(l.clone()),
        Owned::Variant(selection, prop) => selection
            .current()
            .props
            .iter()
            .find_map(|(p, l)| (p == prop).then(|| l.clone())),
        Owned::State(s) => s.with(|v| lit(v, ty).or_else(|| refused(v, ty, vm))),
        // A colour or a number (most computed styles): read from what the
        // function returned as it is, without making a `Value` of it first.
        Owned::Computed(f, cache) if matches!(ty, Ty::Color | Ty::Num) => {
            // Copy short hex keys or parse other colours before leaving the call guard.
            let result = if matches!(ty, Ty::Color) {
                vm.guard(vm.budget, || {
                    f.call::<ComputedColor>(()).map(|value| match value {
                        ComputedColor::Hex(hex) => ComputedColor::Parsed(cached_hex(hex, cache)),
                        other => other,
                    })
                })
            } else {
                vm.call::<mlua::Value>(f, ()).map(ComputedColor::Other)
            };
            match result {
                Ok(ComputedColor::Hex(_)) => unreachable!("hex is resolved inside the call guard"),
                Ok(ComputedColor::Parsed(color)) => color.map(Lit::Color),
                Ok(ComputedColor::Other(mlua::Value::Number(n))) => lit(&Value::Num(n), ty),
                Ok(ComputedColor::Other(mlua::Value::Integer(n))) => lit(&Value::Num(n as f64), ty),
                Ok(ComputedColor::Other(other)) => crate::value::convert(other, None)
                    .map_err(|e| vm.report(e))
                    .ok()
                    .and_then(|v| lit(&v, ty)),
                Err(e) => {
                    vm.report(e);
                    None
                }
            }
        }
        Owned::Computed(f, _) => vm
            .call::<Value>(f, ())
            .map_err(|e| vm.report(e))
            .ok()
            .and_then(|v| lit(&v, ty).or_else(|| refused(&v, ty, vm))),
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
        Owned::Computed(f, _) => vm.call::<Value>(f, ()).unwrap_or_else(|e| {
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
    use zgx::theme::Themed;
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

/// A computed text's short string can become its display string directly,
/// without an intermediate shared state value. Other values retain their
/// formatting, and long strings retain the shared document conversion.
enum ComputedText {
    Text(String),
    Other(Value),
}
impl ComputedText {
    fn into_string(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Other(value) => value.to_string(),
        }
    }
}
impl FromLua for ComputedText {
    fn from_lua(value: mlua::Value, lua: &mlua::Lua) -> mlua::Result<Self> {
        if let mlua::Value::String(text) = &value {
            let bytes = text.as_bytes();
            if bytes.len() <= zgx::text::LONG {
                return Ok(Self::Text(String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
        Value::from_lua(value, lua).map(Self::Other)
    }
    unsafe fn from_stack(idx: std::ffi::c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        use mlua::ffi;
        let state = lua.state();
        // SAFETY: mlua keeps the result at `idx` on the stack. Checking
        // its string type means lua_tolstring converts and allocates
        // nothing. Its bytes stay rooted while Rust copies them; no Lua
        // runs and no borrow escapes. Other values use the existing path.
        unsafe {
            if ffi::lua_type(state, idx) == ffi::LUA_TSTRING {
                let mut len = 0;
                let bytes = ffi::lua_tolstring(state, idx, &mut len);
                if len <= zgx::text::LONG {
                    let bytes = std::slice::from_raw_parts(bytes.cast::<u8>(), len);
                    return Ok(Self::Text(String::from_utf8_lossy(bytes).into_owned()));
                }
            }
            Value::from_stack(idx, lua).map(Self::Other)
        }
    }
}

fn render_text(text: &Text, vm: &Vm) -> String {
    match text {
        Text::Literal(s) => s.clone(),
        Text::Template(segments) => {
            let mut out = String::new();
            for segment in segments {
                match segment {
                    Segment::Literal(literal) => out.push_str(literal),
                    Segment::State(state, path) => state.with(|v| {
                        use std::fmt::Write;
                        // A field of it, by name or (in a list) position
                        // from 1; nothing where there is none.
                        let mut at = Some(v);
                        for step in path.iter() {
                            at = at.and_then(|v| match step.parse::<usize>() {
                                Ok(i) if i >= 1 => v.list().get(i - 1),
                                _ => v.field(step),
                            });
                        }
                        if let Some(v) = at {
                            let _ = write!(out, "{v}");
                        }
                    }),
                }
            }
            out
        }
        Text::Computed(Computed::Script(f)) => match vm.call::<ComputedText>(f, ()) {
            Ok(v) => v.into_string(),
            Err(error) => {
                vm.report(error);
                "⚠".into()
            }
        },
        Text::Computed(Computed::Message(message)) => message_text(message),
    }
}
/// A message's text now: kept out of `render_text`, whose script texts
/// re-render on every change.
#[inline(never)]
fn message_text(message: &zgx::i18n::Message) -> String {
    message.get()
}
/// `text`, owned: what a closure that renders it keeps.
fn copy_text(text: &Text) -> Text {
    match text {
        Text::Literal(s) => Text::Literal(s.clone()),
        Text::Template(segments) => Text::Template(
            segments
                .iter()
                .map(|s| match s {
                    Segment::Literal(l) => Segment::Literal(l.clone()),
                    Segment::State(s, path) => Segment::State(s.clone(), path.clone()),
                })
                .collect(),
        ),
        Text::Computed(computed) => Text::Computed(computed.clone()),
    }
}
fn lower_text(text: &Text, vm: &Rc<Vm>) -> View {
    match text {
        Text::Literal(s) => zgx::compose::text(s.clone()),
        Text::Template(segments) => {
            let segments: Vec<_> = segments
                .iter()
                .map(|s| match s {
                    Segment::Literal(l) => Segment::Literal(l.clone()),
                    Segment::State(s, path) => Segment::State(s.clone(), path.clone()),
                })
                .collect();
            let template = Text::Template(segments);
            let vm = vm.clone();
            text_signal(move || render_text(&template, &vm))
        }
        Text::Computed(Computed::Script(f)) => {
            let (f, vm) = (Text::Computed(Computed::Script(f.clone())), vm.clone());
            text_signal(move || render_text(&f, &vm))
        }
        Text::Computed(Computed::Message(message)) => zgx::compose::text_message(message.clone()),
    }
}
/// What an image's `src` shows now: a picture to load by its path, or a
/// bitmap itself (`ui.bitmap`, a pasted image, one kept in state).
enum Picture {
    Path(Rc<str>),
    Image(Arc<ImageData>),
}
fn picture_of(src: &Owned, vm: &Vm) -> Option<Picture> {
    let of = |value: &Value| match value {
        Value::Image(image) => Some(Picture::Image(image.clone())),
        Value::Str(path) => Some(Picture::Path(path.clone())),
        _ => None,
    };
    match src {
        Owned::State(state) => state.with(of),
        Owned::Computed(f, _) => match vm.call::<mlua::Value>(f, ()) {
            Ok(returned) => crate::value::convert(returned, None)
                .map_err(|e| vm.report(e))
                .ok()
                .and_then(|value| of(&value)),
            Err(e) => {
                vm.report(e);
                None
            }
        },
        other => match resolve(other, Ty::Str, None, vm) {
            Some(Lit::Str(path)) => Some(Picture::Path(path)),
            _ => None,
        },
    }
}

/// A picture a script took (`el:snapshot`, `term:snapshot`), as `host:<name>`
/// while it is held: forgotten as it drops (its element's node goes, its
/// terminal closes), so pictures of what is gone are not kept.
pub(crate) struct HostedPicture(pub std::rc::Weak<Assets>, pub String);
impl Drop for HostedPicture {
    fn drop(&mut self) {
        if let Some(assets) = self.0.upgrade() {
            assets.forget_hosted(&self.1);
        }
    }
}

/// A `ui.image` (out of `build`, as `lower_list` is).
#[inline(never)]
fn lower_image(node: &Node, vm: &Rc<Vm>, low: &Rc<Lowering>) -> View {
    let Kind::Image {
        label,
        src,
        fallback,
    } = &node.kind
    else {
        unreachable!("lower_image takes images")
    };
    // Dithered in dots, or characters in font pixels, this many points
    // square (`ImageData::dithered`, `ImageData::characters`): state or
    // computed, the picture follows them as it does its `src`.
    let prop = |prop: Prop| {
        node.props
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, source)| resolve_owned(source))
    };
    let (dither, characters) = (prop(Prop::Dither), prop(Prop::Characters));
    // The picture, as a view (made again each time it shows).
    let picture = {
        let (src, vm2, low2, label) = (resolve_owned(src), vm.clone(), low.clone(), label.clone());
        move || {
            let (src, vm2, low2) = (src.clone(), vm2.clone(), low2.clone());
            let (dither, characters) = (dither.clone(), characters.clone());
            image_signal(label.clone(), move || {
                let image = match picture_of(&src, &vm2) {
                    Some(Picture::Image(image)) => image,
                    Some(Picture::Path(path)) => {
                        load_image(&vm2, &low2, &path).unwrap_or_else(placeholder)
                    }
                    None => placeholder(),
                };
                let size = |source: &Option<Owned>| match source
                    .as_ref()
                    .and_then(|source| resolve(source, Ty::Num, None, &vm2))
                {
                    Some(Lit::Num(size)) if size > 0. => Some(size),
                    _ => None,
                };
                match (size(&dither), size(&characters)) {
                    (_, Some(pixel)) => Arc::new(image.characters(pixel)),
                    (Some(dot), None) => Arc::new(image.dithered(dot)),
                    (None, None) => image,
                }
            })
        }
    };
    // As it unmounts, the cache lets go of pictures no node shows.
    let sweeper = low.assets.clone();
    let view = match fallback {
        None => picture(),
        Some(fallback) => {
            // The picture once a remote one has arrived (a local one, or a
            // bitmap, at once), else the fallback.
            let (src, vm2, low2) = (resolve_owned(src), vm.clone(), low.clone());
            let shown = move || match picture_of(&src, &vm2) {
                Some(Picture::Path(path)) if is_remote(&path) => {
                    remote_status(&vm2, &low2, &path, false) == READY
                }
                Some(_) => true,
                None => false,
            };
            let (fallback, low3) = (fallback.clone(), low.clone());
            zgx::compose::switch(shown, move |ready, _| {
                if ready {
                    picture().w_full().h_full()
                } else {
                    lower(&fallback, &low3).w_full().h_full()
                }
            })
        }
    };
    view.on_mount(move |handle| handle.retain(sweeper.sweeper()))
}

/// `view`, showing a cached picture (a box's `bg_image`): as it unmounts,
/// the cache lets go of pictures no node shows beyond its budget.
#[cold]
#[inline(never)]
fn swept(view: View, low: &Lowering) -> View {
    let assets = low.assets.clone();
    view.on_mount(move |handle| handle.retain(assets.sweeper()))
}

/// A list row's key: its index, or (`key`) its item's key, a number,
/// string or boolean (any other kind keys by index).
#[derive(Clone, PartialEq, Eq, Hash)]
enum ListKey {
    Index(usize),
    Num(u64),
    Str(Rc<str>),
    Bool(bool),
}
impl ListKey {
    fn of(value: &Value) -> Option<ListKey> {
        Some(match value {
            Value::Num(n) => ListKey::Num(if *n == 0. { 0 } else { n.to_bits() }),
            Value::Str(s) => ListKey::Str(s.clone()),
            Value::Bool(b) => ListKey::Bool(*b),
            _ => return None,
        })
    }
    fn value(&self) -> Value {
        match self {
            ListKey::Index(i) => Value::Num((i + 1) as f64),
            ListKey::Num(bits) => Value::Num(f64::from_bits(*bits)),
            ListKey::Str(s) => Value::Str(s.clone()),
            ListKey::Bool(b) => Value::Bool(*b),
        }
    }
}

/// The keys of a list's `count` rows: `key` called for each in a Luau loop
/// (a call from Rust for each would cost ten times as much), its reads
/// tracked. A row whose key is not a number, string or boolean, or every
/// row if `key` fails, keys by its index.
fn list_keys(vm: &Vm, key: &mlua::Function, count: usize) -> Vec<ListKey> {
    const LOOP: &str = "zgx_list_keys";
    let lua = &vm.lua;
    let all = lua
        .named_registry_value::<mlua::Function>(LOOP)
        .or_else(|_| {
            let all: mlua::Function = lua
                .load(
                    "local key, n = ...\n\
                 local keys = table.create(n)\n\
                 for i = 1, n do keys[i] = key(i) end\n\
                 return keys",
                )
                .set_name("=ui.list keys")
                .into_function()?;
            lua.set_named_registry_value(LOOP, &all)?;
            Ok::<_, mlua::Error>(all)
        });
    let keys = all.and_then(|all| vm.call::<mlua::Table>(&all, (key, count)));
    let keys = match keys {
        Ok(keys) => keys,
        Err(e) => {
            vm.report(e);
            return (0..count).map(ListKey::Index).collect();
        }
    };
    (0..count)
        .map(|i| {
            let key = match keys.raw_get::<mlua::Value>(i + 1) {
                Ok(mlua::Value::Number(n)) => Some(ListKey::of(&Value::Num(n))),
                Ok(mlua::Value::Integer(n)) => Some(ListKey::of(&Value::Num(n as f64))),
                Ok(mlua::Value::Boolean(b)) => Some(Some(ListKey::Bool(b))),
                Ok(mlua::Value::String(s)) => {
                    Some(Some(ListKey::Str(Rc::from(&*s.to_string_lossy()))))
                }
                _ => None,
            };
            key.flatten().unwrap_or(ListKey::Index(i))
        })
        .collect()
}

/// What `builder` describes for `args`, built as a row is but not mounted,
/// in a scope of its own that ends at once: for the text of a row a list
/// has not mounted.
fn described(
    vm: &Rc<Vm>,
    builder: &mlua::Function,
    args: impl mlua::IntoLuaMulti,
) -> Option<Rc<Node>> {
    let scope = Rc::new(Scope::default());
    let previous = vm
        .lua
        .app_data_ref::<crate::node::Env>()
        .map(|env| env.scope.replace(Some(scope.clone())));
    let result = vm.call::<mlua::Value>(builder, args);
    if let (Some(previous), Some(env)) = (previous, vm.lua.app_data_ref::<crate::node::Env>()) {
        *env.scope.borrow_mut() = previous;
    }
    match result {
        Ok(mlua::Value::UserData(ud)) => ud.borrow::<NodeUd>().ok().map(|n| n.0.clone()),
        Ok(_) => None,
        Err(e) => {
            vm.report(e);
            None
        }
    }
}

/// The text `node` shows, as a copy of it takes it: its texts in order, a
/// tab between a row's children and a line break between others'.
fn node_text(node: &Node, vm: &Vm) -> String {
    match &node.kind {
        Kind::Text(text) => return render_text(text, vm),
        Kind::Rich(spans) => return spans.iter().map(|s| render_text(&s.text, vm)).collect(),
        Kind::Show {
            when,
            then,
            otherwise,
        } => {
            let on = resolve(&resolve_owned(when), Ty::Bool, None, vm) == Some(Lit::Bool(true));
            return match (on, otherwise) {
                (true, _) => node_text(then, vm),
                (false, Some(otherwise)) => node_text(otherwise, vm),
                (false, None) => String::new(),
            };
        }
        _ => {}
    }
    let separator = if matches!(node.kind, Kind::Row) {
        "\t"
    } else {
        "\n"
    };
    let parts: Vec<String> = node
        .children
        .iter()
        .map(|child| node_text(child, vm))
        .filter(|text| !text.is_empty())
        .collect();
    parts.join(separator)
}

/// A canvas's `view` (its drawing's pan and zoom) and `bg_grid` (a grid
/// under it that follows the view), each followed in Rust: a pan or zoom
/// runs the view's function, not `draw`.
/// A canvas's items' drawings, by identity, each with its item (kept alive,
/// so its address names it alone), at the size they were drawn for.
#[derive(Default)]
struct KeptItems {
    size: (f32, f32),
    items: HashMap<usize, (Value, zgx::canvas::Canvas)>,
    last: HashMap<usize, (Value, zgx::canvas::Canvas)>,
}

fn canvas_options(
    view: View,
    options: &crate::node::CanvasOptions,
    vm: Rc<Vm>,
    theme: Rc<zgx::theme::ThemeScope>,
) -> View {
    use crate::node::Readable;
    // `{ x, y, zoom }` (or `scale`), as it is now.
    let read = options.view.as_ref().map(|view| {
        let (view, vm) = (view.clone(), vm.clone());
        Rc::new(move || -> zgx::scene::CanvasView {
            let value = match &view {
                Readable::State(s) => s.get(),
                Readable::Function(f) => vm
                    .call::<Value>(f, ())
                    .map_err(|e| vm.report(e))
                    .unwrap_or_default(),
            };
            let field = |name: &str| value.field(name).and_then(Value::as_f64).map(|n| n as f32);
            zgx::scene::CanvasView {
                x: field("x").unwrap_or(0.),
                y: field("y").unwrap_or(0.),
                scale: field("zoom").or_else(|| field("scale")).unwrap_or(1.),
            }
        })
    });
    let mut view = view;
    if let Some(read) = &read {
        let read = read.clone();
        view = view.canvas_view(move || read());
    }
    if let Some(grid) = &options.grid {
        let (step, min_step, mark, color) = (grid.step, grid.min_step, grid.mark, grid.color);
        view = view.grid_fill(move || {
            let at = read
                .as_ref()
                .map_or_else(zgx::scene::CanvasView::default, |read| read());
            let color = match color {
                zgx::theme::Themed::Value(color) => color,
                zgx::theme::Themed::Token(token) => theme.get(token).color()?,
            };
            Some(zgx::scene::GridFill {
                step,
                x: at.x,
                y: at.y,
                scale: at.scale,
                min_step,
                mark,
                color,
            })
        });
    }
    view
}

/// A list's edges (`on_end`, `on_start`, `at_end`): watched in Rust, the
/// script called as one is reached or left. Out of line, so `lower`'s match
/// stays as it was for every other element.
#[inline(never)]
fn list_edges(mut list: View, rare: &crate::node::ListRare, vm: &Rc<Vm>) -> View {
    let call = |f: &mlua::Function| {
        let (f, vm) = (f.clone(), vm.clone());
        move || {
            if let Err(e) = vm.call::<()>(&f, ()) {
                vm.report(e);
            }
        }
    };
    if let Some(end) = &rare.end {
        list = list.on_end(end.distance, call(&end.on_end));
    }
    if let Some(start) = &rare.start {
        list = list.on_start(rare.start_distance.unwrap_or(200.), call(start));
    }
    if rare.contain {
        list = list.overscroll_contain();
    }
    if let Some(at_end) = &rare.at_end {
        let at_end = at_end.clone();
        list = list.on_at_end(move |now| {
            at_end.set(Value::Bool(now));
        });
    }
    list
}

/// A `ui.list` (kept out of `build`, which recurses: its locals would
/// weigh on every level of a deep interface's stack).
#[inline(never)]
/// The rows of a list with `heights`: every height replaced when the list
/// changes (linear, in Rust).
fn list_heights(
    heights: &crate::node::Heights,
    height: f32,
    follow: bool,
    vm: &Rc<Vm>,
    cx: &mut Context,
) -> zgx::compose::VariableHeights {
    let mut rows = zgx::compose::VariableHeights::new(&cx.runtime(), 0, height);
    if follow {
        rows = rows.anchor_end();
    }
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
    rows
}

/// A list's scrollbar and what happens at its edges.
fn list_chrome(
    mut list: View,
    scrollbar: bool,
    rare: Option<&crate::node::ListRare>,
    vm: &Rc<Vm>,
) -> View {
    if scrollbar {
        list = list.scrollbar(true);
    }
    match rare {
        Some(rare) => list_edges(list, rare, vm),
        None => list,
    }
}

fn lower_list(kind: &Kind, vm: &Rc<Vm>, low: &Rc<Lowering>, cx: &mut Option<&mut Context>) -> View {
    let Kind::List {
        count,
        items,
        height,
        heights,
        rare,
        overscan,
        build: builder,
        key,
        measured,
        follow,
        offset,
        keyboard,
        scrollbar,
        ..
    } = kind
    else {
        unreachable!("lower_list takes lists")
    };
    let (sticky, sticky_top, row_text) = match rare.as_deref() {
        Some(rare) => (
            rare.sticky.as_ref(),
            rare.sticky_top,
            rare.row_text.as_ref(),
        ),
        None => (None, 0., None),
    };
    if let Some(items) = items {
        return lower_item_list(kind, items, vm, low, cx);
    }
    // Keyed rows follow their items: built once a key, given the key and
    // their index as a state that follows where the row is.
    let keyed = key.clone().map(|key| (key, vm.clone()));
    let list_key = move |index: usize| -> ListKey {
        match &keyed {
            None => ListKey::Index(index),
            Some((key, vm)) => match vm.call::<Value>(key, index + 1) {
                Ok(value) => ListKey::of(&value).unwrap_or(ListKey::Index(index)),
                Err(e) => {
                    vm.report(e);
                    ListKey::Index(index)
                }
            },
        }
    };
    // A row of a list with no `key`, keyed by its place: a `usize` the
    // list hashes inline, not a `ListKey` (a list scrolls by looking up
    // every row in view).
    let plain_row = {
        let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
        move |index: Signal<usize>, _: usize, cx: &mut Context| -> View {
            build_scoped(&vm, cx, &low, &builder, index.with_untracked(|i| *i) + 1)
        }
    };
    let row = {
        let (builder, vm, low, keyed) = (builder.clone(), vm.clone(), low.clone(), key.is_some());
        move |index: Signal<usize>, key: ListKey, cx: &mut Context| -> View {
            if !keyed {
                return build_scoped(&vm, cx, &low, &builder, index.with_untracked(|i| *i) + 1);
            }
            // The row's index, from 1, following its row.
            let shown = cx
                .runtime()
                .signal(Value::Num((index.with_untracked(|i| *i) + 1) as f64));
            let follow = shown.clone();
            cx.retain(cx.runtime().effect(move || {
                follow.set(Value::Num((index.get() + 1) as f64));
            }));
            let state = crate::value::State(shown, None);
            build_scoped(&vm, cx, &low, &builder, (key.value(), state))
        }
    };
    let keyboard = *keyboard;
    let mut cx = cx.as_deref_mut();
    let offset = match offset {
        Some(offset) => bridge(need(&mut cx), offset, number, |x| Value::Num(x as f64)),
        None => need(&mut cx).state(0_f32),
    };
    let (count, count_vm) = (Rc::new(resolve_owned(count)), vm.clone());
    let count = move || match resolve(&count, Ty::Num, None, &count_vm) {
        Some(Lit::Num(n)) => n.max(0.) as usize,
        _ => 0,
    };
    // What a row the list has not mounted copies as, when a selection
    // spans it: `row_text`'s, or what its row would show.
    let copy = {
        let (builder, vm, count, list_key) =
            (builder.clone(), vm.clone(), count.clone(), list_key.clone());
        let (explicit, keyed) = (row_text.cloned(), key.is_some());
        let runtime = need(&mut cx).runtime();
        move |index: usize| -> Option<String> {
            if index >= count() {
                return None;
            }
            if let Some(text) = &explicit {
                return Some(match vm.call::<Value>(text, index + 1) {
                    Ok(Value::Nil) => String::new(),
                    Ok(value) => value.to_string(),
                    Err(e) => {
                        vm.report(e);
                        String::new()
                    }
                });
            }
            let node = if keyed {
                let shown = runtime.signal(Value::Num((index + 1) as f64));
                let args = (list_key(index).value(), crate::value::State(shown, None));
                described(&vm, &builder, args)
            } else {
                described(&vm, &builder, index + 1)
            };
            Some(node.map_or_else(String::new, |node| node_text(&node, &vm)))
        }
    };
    if *measured {
        let cx = need(&mut cx);
        // `height` is the estimate of a row not yet mounted.
        let mut rows = zgx::compose::VariableHeights::new(&cx.runtime(), 0, *height);
        if *follow {
            rows = rows.anchor_end();
        }
        let list = match key {
            // Heights by place, as many as there are rows.
            None => {
                let r = rows.clone();
                cx.retain(cx.runtime().effect(move || {
                    let _ = r.resize(count());
                }));
                measured_virtual_list(offset, rows, *overscan, |i| i, plain_row)
            }
            // Heights by key, kept as rows move: every row's key, read in
            // one Luau loop.
            Some(key) => {
                let (key, vm) = (key.clone(), vm.clone());
                let keys = move || list_keys(&vm, &key, count());
                // Nothing better than `height` for the rows not yet mounted.
                let estimate = |_: &[ListKey], _: usize| f32::NAN;
                measured_rows(offset, rows, *overscan, keys, estimate, row)
            }
        };
        return list.keyboard_navigation(keyboard).row_text(copy);
    }
    if let Some(heights) = heights {
        let cx = need(&mut cx);
        let rows = list_heights(heights, *height, *follow, vm, cx);
        let (sticky_key, sticky_row, sticky_plain) =
            (list_key.clone(), row.clone(), plain_row.clone());
        let list = match key {
            None => {
                variable_virtual_list(offset.clone(), rows.clone(), *overscan, |i| i, plain_row)
            }
            Some(_) => {
                variable_virtual_list(offset.clone(), rows.clone(), *overscan, list_key, row)
            }
        }
        .keyboard_navigation(keyboard)
        .row_text(copy);
        match sticky {
            None => list,
            Some(sticky) => {
                let list = list_chrome(list, *scrollbar, rare.as_deref(), vm);
                match key {
                    None => sticky_list(
                        list,
                        sticky,
                        sticky_top,
                        offset,
                        rows,
                        |i| i,
                        sticky_plain,
                        vm,
                        cx,
                    ),
                    Some(_) => sticky_list(
                        list, sticky, sticky_top, offset, rows, sticky_key, sticky_row, vm, cx,
                    ),
                }
            }
        }
    } else {
        // Each row owns what it makes, as `each` rows do.
        match key {
            None => virtual_list(offset, *height, *overscan, count, |i| i, plain_row),
            Some(_) => virtual_list(offset, *height, *overscan, count, list_key, row),
        }
        .keyboard_navigation(keyboard)
        .row_text(copy)
    }
}

/// Whether a row's item `now` is the one it shows, `was`: the same table
/// (kept by a write elsewhere in the list), or an equal value.
fn same_item(was: &Value, now: &Value) -> bool {
    let at = crate::value::identity(was);
    (at != 0 && at == crate::value::identity(now)) || was == now
}

/// Keys of a list's items by the item (its table's address), each item
/// kept so an address names it alone: a list written in one row (a token
/// streaming in) asks `key` of that row's item alone.
#[derive(Default)]
struct KeptKeys(HashMap<usize, (Value, ListKey)>);
impl KeptKeys {
    fn get(&self, item: &Value) -> Option<ListKey> {
        let at = crate::value::identity(item);
        (at != 0)
            .then(|| self.0.get(&at).map(|(_, k)| k.clone()))
            .flatten()
    }
    fn keep(&mut self, item: &Value, key: &ListKey) {
        let at = crate::value::identity(item);
        if at != 0 {
            self.0.insert(at, (item.clone(), key.clone()));
        }
    }
}

/// `key(item)` as a list key: a number, string or boolean, else (or if it
/// fails) the item's index.
fn item_key(vm: &Vm, key: &mlua::Function, item: &Value, index: usize) -> ListKey {
    match vm.call::<Value>(key, item.clone()) {
        Ok(value) => ListKey::of(&value).unwrap_or(ListKey::Index(index)),
        Err(e) => {
            vm.report(e);
            ListKey::Index(index)
        }
    }
}

/// The keys of `items`, `key(item)` asked only of the items not kept from
/// the last pass (in one Luau loop when there are many, as `list_keys`
/// asks them); `kept` then holds this pass's.
fn item_keys(vm: &Vm, key: &mlua::Function, items: &Value, kept: &mut KeptKeys) -> Vec<ListKey> {
    const LOOP: &str = "zgx_item_keys";
    let list = items.list();
    let mut keys: Vec<Option<ListKey>> = list.iter().map(|item| kept.get(item)).collect();
    let missing: Vec<usize> = (0..list.len()).filter(|&i| keys[i].is_none()).collect();
    if missing.len() <= 8 {
        for &i in &missing {
            keys[i] = Some(item_key(vm, key, &list[i], i));
        }
    } else {
        let lua = &vm.lua;
        let all = lua
            .named_registry_value::<mlua::Function>(LOOP)
            .or_else(|_| {
                let all: mlua::Function = lua
                    .load(
                        "local key, items = ...\n\
                     local keys = table.create(#items)\n\
                     for i, item in items do keys[i] = key(item) end\n\
                     return keys",
                    )
                    .set_name("=ui.list keys")
                    .into_function()?;
                lua.set_named_registry_value(LOOP, &all)?;
                Ok::<_, mlua::Error>(all)
            });
        let asked = Value::List(
            missing
                .iter()
                .map(|&i| list[i].clone())
                .collect::<Vec<_>>()
                .into(),
        );
        match all.and_then(|all| vm.call::<mlua::Table>(&all, (key, asked))) {
            Ok(table) => {
                for (n, &i) in missing.iter().enumerate() {
                    let key = match table.raw_get::<mlua::Value>(n + 1) {
                        Ok(mlua::Value::Number(v)) => ListKey::of(&Value::Num(v)),
                        Ok(mlua::Value::Integer(v)) => ListKey::of(&Value::Num(v as f64)),
                        Ok(mlua::Value::Boolean(b)) => Some(ListKey::Bool(b)),
                        Ok(mlua::Value::String(s)) => {
                            Some(ListKey::Str(Rc::from(&*s.to_string_lossy())))
                        }
                        _ => None,
                    };
                    keys[i] = Some(key.unwrap_or(ListKey::Index(i)));
                }
            }
            Err(e) => {
                vm.report(e);
                return (0..list.len()).map(ListKey::Index).collect();
            }
        }
    }
    let keys: Vec<ListKey> = keys.into_iter().map(|k| k.unwrap()).collect();
    kept.0.clear();
    for (item, key) in list.iter().zip(&keys) {
        kept.keep(item, key);
    }
    keys
}

/// A `ui.list` of `data` (a list state), whose rows are built from their
/// items as `ui.each`'s are: `row(item, index)`, built again when the item
/// changes, or `row(item, index, row)` built once, `row` its item as a
/// state kept current (a reply streaming in updates its own row alone).
/// `index` is a state, following the row as the list changes; with `key`
/// (given an item) rows follow their items.
fn lower_item_list(
    kind: &Kind,
    items: &Signal<Value>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
    cx: &mut Option<&mut Context>,
) -> View {
    let Kind::List {
        height,
        heights,
        overscan,
        build: builder,
        key,
        measured,
        rare,
        follow,
        offset,
        keyboard,
        scrollbar,
        ..
    } = kind
    else {
        unreachable!("lower_item_list takes lists")
    };
    let live = builder.info().num_params >= 3;
    let row = {
        let (items, builder, vm, low) = (items.clone(), builder.clone(), vm.clone(), low.clone());
        move |index: Signal<usize>, _: ListKey, cx: &mut Context| -> View {
            let at = |list: &Value, i: usize| list.list().get(i).cloned().unwrap_or(Value::Nil);
            let runtime = cx.runtime();
            let first = index.with_untracked(|i| *i);
            let item = runtime.signal(items.with_untracked(|l| at(l, first)));
            let shown = runtime.signal(Value::Num((first + 1) as f64));
            // Bumped as the item changes: a two-parameter row is built again.
            let changes = runtime.signal(0_u64);
            {
                let (items, item, shown, changes) =
                    (items.clone(), item.clone(), shown.clone(), changes.clone());
                let mut was = first;
                cx.retain(runtime.effect(move || {
                    let i = index.get();
                    let now = items.with(|l| at(l, i));
                    if i != was {
                        was = i;
                        shown.set(Value::Num((i + 1) as f64));
                    }
                    if !item.with_untracked(|was| same_item(was, &now)) {
                        item.set(now);
                        changes.update(|n| *n += 1);
                    }
                }));
            }
            let index = crate::value::State(shown, None);
            if live {
                let value = item.with_untracked(Clone::clone);
                let row = crate::value::State(item, None);
                return build_scoped(&vm, cx, &low, &builder, (value, index, row));
            }
            let (builder, vm, low) = (builder.clone(), vm.clone(), low.clone());
            switch(
                move || changes.get(),
                move |_, cx| {
                    let value = item.with_untracked(Clone::clone);
                    build_scoped(&vm, cx, &low, &builder, (value, index.clone()))
                },
            )
            .contents()
        }
    };
    let keyboard = *keyboard;
    let mut cx = cx.as_deref_mut();
    let offset = match offset {
        Some(offset) => bridge(need(&mut cx), offset, number, |x| Value::Num(x as f64)),
        None => need(&mut cx).state(0_f32),
    };
    let count = {
        let items = items.clone();
        move || items.with(|l| l.list().len())
    };
    let list_key = {
        let (items, key, vm) = (items.clone(), key.clone(), vm.clone());
        let kept = Rc::new(std::cell::RefCell::new(KeptKeys::default()));
        move |index: usize| -> ListKey {
            let Some(key) = &key else {
                return ListKey::Index(index);
            };
            let item = items.with(|l| l.list().get(index).cloned().unwrap_or(Value::Nil));
            if let Some(known) = kept.borrow().get(&item) {
                return known;
            }
            let found = item_key(&vm, key, &item, index);
            let mut kept = kept.borrow_mut();
            // The items gone since stop being kept as the list's length.
            if kept.0.len() > 4 * items.with_untracked(|l| l.list().len()) + 64 {
                kept.0.clear();
            }
            kept.keep(&item, &found);
            found
        }
    };
    // A row the list has not mounted copies as `row_text` says (given its
    // index), or as its row would show.
    let copy = {
        let (items, builder, vm, explicit) = (
            items.clone(),
            builder.clone(),
            vm.clone(),
            rare.as_ref().and_then(|rare| rare.row_text.clone()),
        );
        let runtime = need(&mut cx).runtime();
        move |index: usize| -> Option<String> {
            let item = items.with_untracked(|l| l.list().get(index).cloned())?;
            if let Some(text) = &explicit {
                return Some(match vm.call::<Value>(text, index + 1) {
                    Ok(Value::Nil) => String::new(),
                    Ok(value) => value.to_string(),
                    Err(e) => {
                        vm.report(e);
                        String::new()
                    }
                });
            }
            let shown = crate::value::State(runtime.signal(Value::Num((index + 1) as f64)), None);
            let node = if live {
                let row = crate::value::State(runtime.signal(item.clone()), None);
                described(&vm, &builder, (item, shown, row))
            } else {
                described(&vm, &builder, (item, shown))
            };
            Some(node.map_or_else(String::new, |node| node_text(&node, &vm)))
        }
    };
    if *measured {
        let cx = need(&mut cx);
        let mut rows = zgx::compose::VariableHeights::new(&cx.runtime(), 0, *height);
        if *follow {
            rows = rows.anchor_end();
        }
        let list = match key {
            None => {
                let r = rows.clone();
                cx.retain(cx.runtime().effect(move || {
                    let _ = r.resize(count());
                }));
                measured_virtual_list(offset, rows, *overscan, list_key, row)
            }
            // Heights by key, kept as rows move.
            Some(key) => {
                let (key, vm, items) = (key.clone(), vm.clone(), items.clone());
                let mut kept = KeptKeys::default();
                let keys = move || items.with(|l| item_keys(&vm, &key, l, &mut kept));
                let estimate = |_: &[ListKey], _: usize| f32::NAN;
                measured_rows(offset, rows, *overscan, keys, estimate, row)
            }
        };
        return list.keyboard_navigation(keyboard).row_text(copy);
    }
    if let Some(heights) = heights {
        let cx = need(&mut cx);
        let rows = list_heights(heights, *height, *follow, vm, cx);
        let (sticky_key, sticky_row) = (list_key.clone(), row.clone());
        let list = variable_virtual_list(offset.clone(), rows.clone(), *overscan, list_key, row)
            .keyboard_navigation(keyboard)
            .row_text(copy);
        let Some(sticky) = rare.as_ref().and_then(|rare| rare.sticky.as_ref()) else {
            return list;
        };
        let inset = rare.as_ref().map_or(0., |rare| rare.sticky_top);
        let list = list_chrome(list, *scrollbar, rare.as_deref(), vm);
        return sticky_list(
            list, sticky, inset, offset, rows, sticky_key, sticky_row, vm, cx,
        );
    }
    virtual_list(offset, *height, *overscan, count, list_key, row)
        .keyboard_navigation(keyboard)
        .row_text(copy)
}

/// A variable list with sticky rows: the list, and over it the sticky row
/// whose section covers the top of the view (the last one at or above it),
/// built as any row is and pushed up as the next sticky row arrives: CSS's
/// `position: sticky` for section headers, with the rows virtualized.
/// The sticky row in charge when the view's top is in row `top`: the last
/// of `sticky` (sorted) at or above it.
fn sticky_in_charge(sticky: &[usize], top: usize) -> Option<usize> {
    let after = sticky.partition_point(|&i| i <= top);
    after.checked_sub(1).map(|last| sticky[last])
}

/// How far the next sticky row pushes the one in charge up: `next` is how
/// far below the view's top it starts, `height` the one in charge's own.
fn sticky_push(next: Option<f32>, inset: f32, height: f32) -> f32 {
    next.map_or(0., |next| (next - inset - height).min(0.))
}

#[allow(clippy::too_many_arguments)]
fn sticky_list<K: PartialEq + Clone + 'static>(
    list: View,
    sticky: &crate::node::Heights,
    // How far below the top they stick (CSS's sticky `top`).
    inset: f32,
    offset: Signal<f32>,
    rows: zgx::compose::VariableHeights,
    // A row's identity, and its row built as the list builds it: rows that
    // keep a key stay the same row as the list changes.
    key: impl Fn(usize) -> K + 'static,
    row: impl Fn(Signal<usize>, K, &mut Context) -> View + 'static,
    vm: &Rc<Vm>,
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
            indices.with(|list| sticky_in_charge(list, top))
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
            sticky_push(
                next.map(|next| rows.row_offset(next) - offset.get()),
                inset,
                height,
            )
        }
    };
    // Its identity picks what shows: a row that moves stays built.
    let shown = {
        let current = current.clone();
        move || current().map(&key)
    };
    let header = zgx::compose::switch(shown, move |shown, cx| match shown {
        Some(key) => {
            let index = cx.runtime().signal(current().unwrap_or(0));
            let (follow, at) = (index.clone(), current.clone());
            cx.retain(cx.runtime().effect(move || {
                if let Some(at) = at() {
                    follow.set(at);
                }
            }));
            row(index, key, cx)
        }
        None => div(),
    });
    overlay().child(list.w_full().h_full()).child(
        column()
            .absolute()
            .top(inset)
            .left(0.)
            .right(0.)
            .child(header)
            .reactive_style(move || zgx::style::Styles::new().translate(0., shift())),
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
        color: Option<zgx::theme::Themed<Color>>,
        background: Option<zgx::theme::Themed<Color>>,
        size: Option<f32>,
        weight: Option<u16>,
        italic: bool,
        underline: bool,
        strike: bool,
        mono: bool,
        padding: [f32; 2],
        radius: f32,
        underline_offset: Option<f32>,
        underline_style: Option<zgx::rich_text::LineStyle>,
        underline_color: Option<zgx::theme::Themed<Color>>,
        underline_thickness: Option<f32>,
        hover: Option<(bool, Option<zgx::theme::Themed<Color>>)>,
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
                            Segment::State(s, path) => Segment::State(s.clone(), path.clone()),
                        })
                        .collect(),
                ),
                Text::Computed(computed) => Text::Computed(computed.clone()),
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
                underline_style: s.underline_style,
                underline_color: s.underline_color,
                underline_thickness: s.underline_thickness,
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
                    underline_style,
                    underline_color,
                    underline_thickness,
                    hover,
                } = span.style;
                let decoration = || {
                    let d = zgx::rich_text::Decoration::new(underline_thickness.unwrap_or(1.))
                        .line(underline_style.unwrap_or_default());
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
                if let Some(color) = underline_color {
                    out = out.underline_color(color);
                }
                if let Some((underlined, color)) = hover {
                    out = out.hover(zgx::compose::SpanHover {
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
                    out = out.icon(zgx::compose::SpanIcon {
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
            (Prop::RotateX, Lit::Num(v)) => transform!().rotate_x = v.to_radians(),
            (Prop::RotateY, Lit::Num(v)) => transform!().rotate_y = v.to_radians(),
            (Prop::Perspective, Lit::Num(v)) => transform!().perspective = *v,
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
            | Prop::RotateX
            | Prop::RotateY
            | Prop::Perspective
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
        let mut c = zgx::decoration::Corners::all(0.);
        for (prop, lit) in lits {
            match (prop, lit) {
                (Prop::Rounded, Lit::Num(r)) => c = zgx::decoration::Corners::all(*r),
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

/// A grid's track list, checked as it came in (`node::lit`).
#[cold]
#[inline(never)]
fn track_list<S: Styled>(s: S, prop: Prop, list: &str) -> S {
    match list.parse() {
        Ok(tracks) if prop == Prop::Cols => s.grid_template_columns(tracks),
        Ok(tracks) => s.grid_template_rows(tracks),
        Err(_) => s,
    }
}
/// A size worked out from the containing block, checked as it came in
/// (`node::lit`): a string in a `Lit`, as a track list is, so that dropping
/// one has no other heap kind to tell apart.
#[cold]
#[inline(never)]
fn size_math<S: Styled>(s: S, prop: Prop, written: &str) -> S {
    let Ok(size) = written.parse::<zgx::size_calc::SizeCalc>() else {
        return s;
    };
    match prop {
        Prop::W => s.w_calc(size),
        Prop::H => s.h_calc(size),
        Prop::MinW => s.min_w_calc(size),
        Prop::MaxW => s.max_w_calc(size),
        Prop::MinH => s.min_h_calc(size),
        _ => s.max_h_calc(size),
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
        (Cols | Rows, Lit::Str(list)) => track_list(s, prop, list),
        (W | H | MinW | MaxW | MinH | MaxH, Lit::Str(size)) => size_math(s, prop, size),
        (Z, Lit::Num(v)) => s.z(*v as i32),
        (StickyTop, Lit::Num(v)) => s.sticky_top(*v),
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
        (Gradient | Radial, Lit::Gradient(gradient)) => match &**gradient {
            crate::node::GradientLit::Linear(angle, stops) => s.bg_gradient(*angle, stops.clone()),
            crate::node::GradientLit::Radial(radial) => {
                s.bg_radial(radial.center, radial.radius, radial.stops)
            }
        },
        (TextColor, Lit::Color(c)) => s.text_color(*c),
        (Rounded, Lit::Num(v)) => s.rounded(*v),
        (Corners, Lit::Corners(c)) => s.rounded_corners(*c),
        // The border paints over the node's children (a CSS inset outline).
        (BorderOver, Lit::Bool(v)) => s.border_over(*v),
        // `{ top, right, bottom, left }`, as CSS orders border widths.
        (BorderSides, Lit::Corners(c)) => s.border_edges(zgx::scene::Insets {
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
            s.border_style(zgx::decoration::BorderStyle::Dashed {
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
        (HueRotate, Lit::Num(v)) => s.hue_rotate(*v),
        (Sepia, Lit::Num(v)) => s.sepia(*v),
        (Invert, Lit::Num(v)) => s.invert(*v),
        // CSS `mix-blend-mode` names; another leaves it normal.
        (Blend, Lit::Str(v)) => s.blend(zgx::scene::BlendMode::parse(v).unwrap_or_default()),
        (Fade, Lit::Pair(top, bottom)) => s.fade_edges(*top, *bottom),
        (FadeX, Lit::Pair(left, right)) => s.fade_edges_x(*left, *right),
        (FadeOverflow, Lit::Bool(v)) => s.fade_overflow(*v),
        (Snap, Lit::Bool(v)) => s.snap(*v),
        (Zoom, Lit::Num(v)) => s.zoom(*v),
        (ZoomFixed, Lit::Bool(true)) => s.zoom_fixed(),
        // `{ top, right, bottom, left }` in points; negative reaches past the box.
        (ClipInset, Lit::Corners(c)) => {
            s.clip_inset(c.top_left, c.top_right, c.bottom_right, c.bottom_left)
        }
        // Points inside its rounded clip that its content fades out over.
        (ClipFeather, Lit::Num(v)) => s.clip_feather(*v),
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
            "ltr" => s.direction(zgx::scene::Direction::Ltr),
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
            s.font_features(zgx::text_layout::FontFeatures::new([(*b"tnum", 1)]))
        }
        (FontFeatures, Lit::Str(v)) => s.font_features(font_features(v)),
        _ => s,
    }
}

/// `"ss01 cv02=3 liga=0"`: OpenType features, as CSS's
/// `font-feature-settings`, a tag on or `tag=value`. What isn't a four-letter
/// tag with a number is left out, as CSS drops a declaration it can't read.
fn font_features(text: &str) -> zgx::text_layout::FontFeatures {
    zgx::text_layout::FontFeatures::new(text.split_whitespace().filter_map(|setting| {
        let (tag, value) = setting.split_once('=').unwrap_or((setting, "1"));
        Some((tag.as_bytes().try_into().ok()?, value.parse().ok()?))
    }))
}

/// A cursor by its name: `"nwse_resize"`, or CSS's `"nwse-resize"`.
fn cursor(name: &str) -> zgx::cursor::Cursor {
    use zgx::cursor::Cursor::*;
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
                // Laid out, so shown: content never laid out (made in the
                // mount laying it out) just goes.
                let shown = mounted.as_ref().is_some_and(|(_, handle)| {
                    handle.is_mounted()
                        && handle.scene().is_some_and(|scene| {
                            scene.borrow().layout_bounds(handle.node())
                                != zgx::scene::Rect::default()
                        })
                });
                match (mounted.filter(|_| shown), &exit) {
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
                        let px = |v: f32| Lit::Len(zgx::layout::Length::Px(v));
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
        // full-window box over the panes while it is put away), nor while
        // what it shows is hidden (a page kept mounted over a web view).
        cell().pointer_events_when(showing).hollow().child(region)
    } else {
        region
    }
}

/// A box whose one in-flow child fills it in both directions unless it
/// sizes itself: a one-cell grid, whose items stretch.
fn cell() -> View {
    column().grid().grid_cols(1).grid_rows(1).items_stretch()
}

#[cfg(test)]
#[test]
fn cached_hex_matches_every_parsed_value_in_a_sequence() {
    let cache = std::cell::Cell::default();
    for text in [
        "#fff",
        "#fff",
        "#fff\0",
        "#fff",
        "#aBcD",
        "#123456",
        "#12345678",
        "#GGG",
        "#GGG",
        "#é1",
        "#000",
        "#fF0f",
    ] {
        let hex = ShortHex::new(text).unwrap();
        assert_eq!(
            cached_hex(hex, &cache),
            crate::node::parse_color(text),
            "{text:?}"
        );
    }
}

/// An input's `active_descendant`: the id it names, followed.
#[inline(never)]
/// `on_paste`: handed `{ text, files }`, a truthy return takes the paste.
fn on_paste(view: View, hook: mlua::Function, vm: &Rc<Vm>) -> View {
    let vm = vm.clone();
    view.on_paste(move |text| {
        let event = vm.lua.create_table().and_then(|event| {
            event.set("text", text)?;
            event.set("files", crate::io::pasted_files(&vm.lua)?)?;
            Ok(event)
        });
        match event.and_then(|event| vm.call::<mlua::Value>(&hook, event)) {
            Ok(mlua::Value::Nil | mlua::Value::Boolean(false)) => false,
            Ok(_) => true,
            Err(error) => {
                vm.report(error);
                false
            }
        }
    })
}

fn active_descendant(view: View, active: &Source, vm: &Rc<Vm>) -> View {
    let (active, vm) = (resolve_owned(active), vm.clone());
    view.active_descendant(move || match resolve(&active, Ty::Str, None, &vm) {
        Some(Lit::Str(id)) => Some(id.to_string()),
        _ => None,
    })
}
