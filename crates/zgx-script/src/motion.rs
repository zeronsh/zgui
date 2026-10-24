//! Layout animations on the compositor: rows of a `ui.each` that move when
//! the list changes, nodes that move when layout moves them
//! (`layout = "position"`), and shared elements (`layout_id`).
//!
//! Each works the same way (FLIP): read where the node was before the
//! layout pass that moves it, read where that pass put it, set a translation
//! that holds it where it was, and ease the translation to nothing. The
//! frames that follow only translate (and for a shared element, scale) on the
//! compositor: no layout pass and no script call per frame, and the engine
//! asks for frames only while something moves. Positions come from the
//! scene's layout, so scrolling and other animations never count as moves.
use crate::{
    Vm,
    anime::{
        Engine, Key,
        ease::{Ease, Spring},
        target::Target,
        value::V,
    },
    element::{Element, ElementUd},
    lower::{Lowering, build_scoped, with_animated},
    node::{Lit, Prop},
    value::Value,
};
use mlua::{Function, Table};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
};
use zgx::{
    compose::{Context, NodeHandle, View, prelude::*},
    layout::Length,
    reactive::Signal,
    scene::{Rect, Scene},
};

/// How a layout animation moves: an ease and its length in ms.
#[derive(Clone, Default)]
pub(crate) struct Timing {
    pub ease: Option<Ease>,
    pub duration: Option<f64>,
}
impl Timing {
    /// The ease and its length. The default is the springy one the layout
    /// boxes use: anime.js `spring({ bounce = .1, duration = 400 })`.
    pub fn resolve(&self) -> (Ease, f64) {
        let ease = self
            .ease
            .clone()
            .unwrap_or_else(|| Ease::Spring(Rc::new(Spring::perceptual(0.1, 0.4))));
        let duration = self
            .duration
            .or_else(|| ease.spring_duration_ms())
            .unwrap_or(350.);
        (ease, duration)
    }
}

/// An ease from a script value: a name or an `anime.spring` (any ease).
fn ease_of(value: mlua::Value, what: &str) -> mlua::Result<Option<Ease>> {
    Ok(match value {
        mlua::Value::Nil => None,
        mlua::Value::String(s) => {
            Some(Ease::parse(&s.to_string_lossy()).map_err(mlua::Error::runtime)?)
        }
        mlua::Value::UserData(ud) => Some(ud.borrow::<crate::anime::api::EaseUd>()?.0.clone()),
        other => {
            return Err(mlua::Error::runtime(format!(
                "{what} takes an ease name or an anime ease, not {}",
                other.type_name()
            )));
        }
    })
}

/// `animate = true | "outQuint" | anime.spring {...} | { duration, ease } |
/// { bounce, duration } | { spring = anime.spring {...} }`.
fn timing_of(value: mlua::Value) -> mlua::Result<Option<Timing>> {
    Ok(match value {
        mlua::Value::Nil | mlua::Value::Boolean(false) => None,
        mlua::Value::Boolean(true) => Some(Timing::default()),
        mlua::Value::Table(t) => {
            let duration = t.get::<Option<f64>>("duration")?;
            let bounce = t.get::<Option<f64>>("bounce")?;
            let spring = ease_of(t.get("spring")?, "`animate.spring`")?;
            let ease = ease_of(t.get("ease")?, "`animate.ease`")?;
            match (spring, bounce) {
                (Some(spring), _) => Some(Timing {
                    ease: Some(spring),
                    duration,
                }),
                // A spring of this perceived bounce and length.
                (None, Some(bounce)) => Some(Timing {
                    ease: Some(Ease::Spring(Rc::new(Spring::perceptual(
                        bounce,
                        duration.unwrap_or(400.) / 1000.,
                    )))),
                    duration: None,
                }),
                (None, None) => Some(Timing { ease, duration }),
            }
        }
        other => Some(Timing {
            ease: ease_of(other, "`animate`")?,
            duration: None,
        }),
    })
}

/// `ui.each`'s motion: rows that move animate (`animate`), new rows play
/// `enter` and leaving ones `exit`, with `ui.presence`'s parameters.
pub(crate) struct EachMotion {
    pub moves: Option<Timing>,
    pub enter: Option<Table>,
    pub exit: Option<Table>,
    /// The first rows enter too.
    pub initial: bool,
}
pub(crate) fn each_motion(t: &Table) -> mlua::Result<Option<Rc<EachMotion>>> {
    let moves = timing_of(t.get("animate")?)?;
    let enter = t.get::<Option<Table>>("enter")?;
    let exit = t.get::<Option<Table>>("exit")?;
    if moves.is_none() && enter.is_none() && exit.is_none() {
        return Ok(None);
    }
    Ok(Some(Rc::new(EachMotion {
        moves,
        enter,
        exit,
        initial: t.get::<Option<bool>>("initial")?.unwrap_or(false),
    })))
}

fn engine_of(vm: &Vm) -> Option<Rc<Engine>> {
    vm.lua
        .app_data_ref::<crate::node::Env>()
        .map(|env| env.engine.clone())
}

/// Starts the moves `writes` describe, from their values now: each holds
/// its node where it was, then eases to where it belongs.
fn start(engine: &Rc<Engine>, vm: Weak<Vm>, writes: Vec<(Target, Key, f64, f64)>, timing: &Timing) {
    if writes.is_empty() {
        return;
    }
    let from = writes
        .iter()
        .map(|(target, key, from, _)| (target.clone(), key.clone(), V::Num(*from), None))
        .collect();
    let to = writes
        .into_iter()
        .map(|(target, key, _, to)| (target, key, V::Num(to)))
        .collect();
    engine.set_now(from);
    let (ease, duration) = timing.resolve();
    engine.tween(vm, to, duration, ease, Default::default());
}

/// Moves `element` holds its node by: from where it was laid out (`dx`,
/// `dy` from where it is now), plus what a move in flight holds it by, to
/// nothing. The offset goes over the node's own translation, so a script's
/// `x` and `y` animations compose with it.
fn hold(element: &Rc<Element>, dx: f32, dy: f32) -> [(Target, Key, f64, f64); 2] {
    let (ox, oy) = element.offset();
    let target = Target::Element(element.clone());
    [
        (target.clone(), Key::OffsetX, (ox + dx) as f64, 0.),
        (target, Key::OffsetY, (oy + dy) as f64, 0.),
    ]
}

/// A view `element` animates: its values go to the view's root on the
/// compositor, over (not instead of) the root's own reactive style.
fn motion_view(
    view: View,
    element: Rc<Element>,
    mounted: impl FnOnce(NodeHandle) + 'static,
) -> View {
    let style_element = element.clone();
    // A component wrapper refines the root it builds: its reactive style
    // merges over the root's own.
    component(move |_| view)
        .reactive_style(move || with_animated(&style_element, Vec::new()))
        .on_mount(move |handle| {
            element.attach(handle.clone());
            mounted(handle);
        })
}

/// Plays anime parameters on `element`, as `ui.presence` does; `done` runs
/// when they complete.
fn play(
    vm: &Vm,
    element: &Rc<Element>,
    params: &Table,
    done: Option<Rc<dyn Fn()>>,
) -> mlua::Result<()> {
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
    let target = mlua::Value::UserData(lua.create_userdata(ElementUd(element.clone()))?);
    crate::anime::api::animate(lua, target, own)?;
    Ok(())
}

// `ui.each` ------------------------------------------------------------------

/// A row: its identity in the list, and which time it came in under it (a
/// row leaving can still be on screen as one with its key comes back).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RowKey(Rc<str>, u64);

#[derive(Default)]
struct Rows {
    /// What is mounted, in order: the list's rows and those still leaving.
    shown: Vec<RowKey>,
    exiting: HashSet<RowKey>,
    /// Each identity in the list, and the row it has now.
    live: HashMap<Rc<str>, u64>,
    /// Each identity's index in the list, for building its row.
    indices: HashMap<Rc<str>, usize>,
    next: u64,
    /// Rows from before this generation were there from the start.
    first_end: Option<u64>,
    elements: HashMap<RowKey, Rc<Element>>,
    region: Option<NodeHandle>,
    /// Where each row was (from the region's corner) before a change whose
    /// layout pass has not run yet.
    before: Option<HashMap<RowKey, (f32, f32)>>,
}

/// Where layout put `node`, from `origin`'s corner.
fn offset(scene: &Scene, node: zgx::scene::NodeId, origin: Rect) -> (f32, f32) {
    let b = scene.layout_bounds(node);
    (b.x - origin.x, b.y - origin.y)
}
/// The same, for a row layout has placed: one mounted since the last layout
/// pass has no place yet to move from.
fn placed(scene: &Scene, node: zgx::scene::NodeId, origin: Rect) -> Option<(f32, f32)> {
    let b = scene.layout_bounds(node);
    (b != Rect::default()).then(|| offset(scene, node, origin))
}

/// The list's identities, as the plain `ui.each` keys them.
fn identities(
    list: &Value,
    key: &Option<Function>,
    item_keys: &mut crate::lower::ItemKeys,
    vm: &Vm,
) -> Vec<Rc<str>> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let keys = key.as_ref().map(|f| item_keys.of(list.list(), f, vm));
    list.list()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let base = match &keys {
                Some(keys) => keys[i].clone(),
                None => format!("{i}:{item:?}"),
            };
            let n = seen.entry(base.clone()).or_default();
            *n += 1;
            format!("{base}#{n}").into()
        })
        .collect()
}

/// `ui.each` with motion. Rows that stay but move slide from where they
/// were; new rows play `enter`; leaving rows leave the flow, held where they
/// were, and play `exit` before they unmount.
pub(crate) fn each(
    cx: &mut Context,
    items: Signal<Value>,
    builder: Function,
    key: Option<Function>,
    motion: Rc<EachMotion>,
    vm: &Rc<Vm>,
    low: &Rc<Lowering>,
) -> View {
    let rows: Rc<RefCell<Rows>> = Rc::default();
    let shown = cx.state(Vec::<RowKey>::new());
    let remove: Rc<dyn Fn(RowKey)> = {
        let (rows, shown) = (Rc::downgrade(&rows), shown.clone());
        Rc::new(move |row: RowKey| {
            let Some(rows) = rows.upgrade() else { return };
            let list = {
                let mut st = rows.borrow_mut();
                st.shown.retain(|k| *k != row);
                st.exiting.remove(&row);
                st.elements.remove(&row);
                st.shown.clone()
            };
            shown.set(list);
        })
    };
    let change = {
        let (rows, shown, motion, vm) = (rows.clone(), shown.clone(), motion.clone(), vm.clone());
        move |ids: Vec<Rc<str>>| {
            let mut leaving_out = Vec::new();
            let flip;
            let list = {
                let mut st = rows.borrow_mut();
                let st = &mut *st;
                let first = st.first_end.is_none();
                // Where every row is now, for the layout pass the change brings.
                flip = (!first && motion.moves.is_some() && st.before.is_none())
                    .then(|| st.region.clone())
                    .flatten()
                    .and_then(|region| {
                        let scene = region.scene()?;
                        let scene = scene.borrow();
                        if !scene.contains(region.node()) {
                            return None;
                        }
                        let origin = scene.layout_bounds(region.node());
                        let before = st
                            .elements
                            .iter()
                            .filter(|(k, _)| !st.exiting.contains(k))
                            .filter_map(|(k, e)| {
                                let node = e.node().filter(|n| scene.contains(*n))?;
                                Some((k.clone(), placed(&scene, node, origin)?))
                            })
                            .collect();
                        st.before = Some(before);
                        Some(region)
                    });
                let mut live = HashMap::with_capacity(ids.len());
                let mut indices = HashMap::with_capacity(ids.len());
                let mut keys = Vec::with_capacity(ids.len());
                for (i, id) in ids.into_iter().enumerate() {
                    let generation = st.live.remove(&id).unwrap_or_else(|| {
                        st.next += 1;
                        st.next
                    });
                    live.insert(id.clone(), generation);
                    indices.insert(id.clone(), i);
                    keys.push(RowKey(id, generation));
                }
                // What is left of the old list is leaving.
                let leaving = std::mem::replace(&mut st.live, live);
                st.indices = indices;
                if first {
                    st.first_end = Some(st.next + 1);
                }
                let region = st.region.clone();
                for (id, generation) in leaving {
                    let row = RowKey(id, generation);
                    let element = st.elements.get(&row).cloned();
                    match (element, &motion.exit, &region) {
                        (Some(element), Some(_), Some(region)) if element.is_mounted() => {
                            // Out of the flow, where it was: the rows after
                            // it close up while it leaves.
                            let placed = element.read(|scene, node| {
                                let origin = scene.layout_bounds(region.node());
                                (offset(scene, node, origin), scene.layout_bounds(node))
                            });
                            if let Some(((x, y), b)) = placed {
                                let px = |v: f32| Lit::Len(Length::Px(v));
                                for (prop, lit) in [
                                    (Prop::Absolute, Lit::Bool(true)),
                                    (Prop::M, px(0.)),
                                    (Prop::Left, px(x)),
                                    (Prop::Top, px(y)),
                                    (Prop::W, px(b.width)),
                                    (Prop::H, px(b.height)),
                                ] {
                                    element.set_style(prop, lit);
                                }
                                element.revision.update(|r| *r = r.wrapping_add(1));
                            }
                            st.exiting.insert(row.clone());
                            leaving_out.push((row, element));
                        }
                        _ => {
                            st.elements.remove(&row);
                        }
                    }
                }
                // Leaving rows keep their place in the order (it decides
                // what paints over what), after the row they followed.
                let staying: HashSet<&RowKey> = keys.iter().collect();
                let mut head = Vec::new();
                let mut after: HashMap<RowKey, Vec<RowKey>> = HashMap::new();
                let mut anchor: Option<&RowKey> = None;
                for k in &st.shown {
                    if st.exiting.contains(k) {
                        match anchor {
                            Some(a) => after.entry(a.clone()).or_default().push(k.clone()),
                            None => head.push(k.clone()),
                        }
                    } else if staying.contains(k) {
                        anchor = Some(k);
                    }
                }
                let mut list = head;
                list.reserve(keys.len() + after.values().map(Vec::len).sum::<usize>());
                for k in keys {
                    let followers = after.remove(&k);
                    list.push(k);
                    list.extend(followers.into_iter().flatten());
                }
                st.shown = list.clone();
                list
            };
            if let Some(region) = flip {
                let (rows, motion, vm) = (Rc::downgrade(&rows), motion.clone(), vm.clone());
                region.clone().after_layout(move || {
                    if let Some(rows) = rows.upgrade() {
                        slide_rows(&rows, &region, &motion, &vm);
                    }
                });
            }
            shown.set(list);
            if let Some(exit) = &motion.exit {
                for (row, element) in leaving_out {
                    let then = remove.clone();
                    let done: Rc<dyn Fn()> = {
                        let row = row.clone();
                        Rc::new(move || then(row.clone()))
                    };
                    if let Err(e) = play(&vm, &element, exit, Some(done)) {
                        vm.report(e);
                        remove(row);
                    }
                }
            }
        }
    };
    let runtime = cx.runtime();
    let (key_vm, untracked) = (vm.clone(), runtime.clone());
    let from = items.clone();
    let mut item_keys = crate::lower::ItemKeys::default();
    cx.retain(runtime.effect(move || {
        let ids = from.with(|list| identities(list, &key, &mut item_keys, &key_vm));
        untracked.untracked(|| change(ids));
    }));
    let (build_rows, vm, low) = (rows.clone(), vm.clone(), low.clone());
    let region_rows = Rc::downgrade(&rows);
    keyed(
        move || shown.get(),
        move |row: RowKey, cx| {
            let (index, enters) = {
                let st = build_rows.borrow();
                let index = st.indices.get(&row.0).copied().unwrap_or(0);
                let first = st.first_end.is_none_or(|end| row.1 < end);
                (index, motion.enter.is_some() && (!first || motion.initial))
            };
            let item = items.with_untracked(|list| list.list().get(index).cloned());
            let content = build_scoped(
                &vm,
                cx,
                &low,
                &builder,
                (item.unwrap_or_default(), index + 1),
            );
            let element = Element::new(&low.runtime, None, Rc::new([]), "each");
            let (rows, motion, vm) = (Rc::downgrade(&build_rows), motion.clone(), vm.clone());
            let (own, elements) = (element.clone(), low.elements.clone());
            motion_view(content, element, move |handle| {
                // A row the script can target moves through its own element,
                // so its own animations compose with the moves.
                let mounted = elements.at(handle.node()).unwrap_or(own);
                if let Some(rows) = rows.upgrade() {
                    rows.borrow_mut().elements.insert(row, mounted.clone());
                }
                if let (true, Some(enter)) = (enters, &motion.enter)
                    && let Err(e) = play(&vm, &mounted, enter, None)
                {
                    vm.report(e);
                }
            })
        },
    )
    .on_mount(move |handle| {
        if let Some(rows) = region_rows.upgrade() {
            rows.borrow_mut().region = Some(handle);
        }
    })
}

/// After the layout pass a list change brought: every row that moved holds
/// where it was and slides to its new place. One pass over the rows, reading
/// bounds layout already computed.
fn slide_rows(rows: &RefCell<Rows>, region: &NodeHandle, motion: &EachMotion, vm: &Rc<Vm>) {
    let (Some(timing), Some(engine), Some(scene)) = (&motion.moves, engine_of(vm), region.scene())
    else {
        rows.borrow_mut().before = None;
        return;
    };
    let mut writes = Vec::new();
    {
        let mut st = rows.borrow_mut();
        let Some(before) = st.before.take() else {
            return;
        };
        let scene = scene.borrow();
        if !scene.contains(region.node()) {
            return;
        }
        let origin = scene.layout_bounds(region.node());
        for (row, (x0, y0)) in before {
            let Some(element) = st.elements.get(&row).filter(|_| !st.exiting.contains(&row)) else {
                continue;
            };
            let Some(node) = element.node().filter(|n| scene.contains(*n)) else {
                continue;
            };
            let Some((x1, y1)) = placed(&scene, node, origin) else {
                continue;
            };
            let (dx, dy) = (x0 - x1, y0 - y1);
            if dx.abs() < 0.01 && dy.abs() < 0.01 {
                continue;
            }
            writes.extend(hold(element, dx, dy));
        }
    }
    start(&engine, Rc::downgrade(vm), writes, timing);
}

// `layout = "position"` and `layout_id` --------------------------------------

/// A node whose layout moves animate.
pub(crate) struct Tracker {
    element: Rc<Element>,
    handle: NodeHandle,
    /// Which ancestor its position is measured from (0: the parent).
    depth: usize,
    timing: Timing,
    /// Where layout last put it, from that ancestor's corner.
    at: Cell<Option<(f32, f32)>>,
    /// Its box at the last layout, ancestors' translations included and its
    /// own left out: where a successor with its `layout_id` starts.
    last: Cell<Option<Rect>>,
    /// A predecessor's box as it showed: the first layout starts from it.
    from: Cell<Option<Rect>>,
}
impl Tracker {
    /// Its box as it shows now, scale included.
    fn visual(&self) -> Option<Rect> {
        let e = &self.element;
        let base = match e.read(|scene, node| scene.bounds(node)) {
            Some(b) => b,
            None => {
                let last = self.last.get()?;
                let (tx, ty) = e.shown_translate(None).unwrap_or_else(|| e.translate());
                Rect::new(last.x + tx, last.y + ty, last.width, last.height)
            }
        };
        let local = e.local();
        let (sx, sy) = local.scale;
        let (ox, oy) = local.origin;
        Some(Rect::new(
            base.x + ox * base.width * (1. - sx),
            base.y + oy * base.height * (1. - sy),
            base.width * sx,
            base.height * sy,
        ))
    }
    fn laid_out(&self, vm: &Weak<Vm>) {
        let Some(scene) = self.handle.scene() else {
            return;
        };
        let node = self.handle.node();
        let (at, world) = {
            let scene = scene.borrow();
            if !scene.contains(node) {
                return;
            }
            let origin = scene
                .ancestors(node)
                .nth(self.depth)
                .map(|a| scene.layout_bounds(a))
                .unwrap_or_default();
            let (b, t) = (scene.bounds(node), scene.transform(node));
            let world = Rect::new(b.x - t.x, b.y - t.y, b.width, b.height);
            (offset(&scene, node, origin), world)
        };
        let previous = self.at.replace(Some(at));
        self.last.set(Some(world));
        let mut writes = Vec::new();
        if let Some(from) = self.from.take() {
            // Handed over: from the predecessor's box to this one's, by an
            // offset over its own translation and a scale over its own.
            let e = &self.element;
            let base = e.base_transform().1.unwrap_or_default();
            let origin = e.local().origin;
            let scale = |a: f32, b: f32| if b > 0. { a / b } else { 1. };
            let (sx, sy) = (
                scale(from.width, world.width) * base.scale.0,
                scale(from.height, world.height) * base.scale.1,
            );
            let (tx, ty) = e.translate();
            let dx = from.x - world.x - origin.0 * world.width * (1. - sx) - tx;
            let dy = from.y - world.y - origin.1 * world.height * (1. - sy) - ty;
            let (ox, oy) = e.offset();
            writes.extend(hold(e, dx - ox, dy - oy));
            if (sx - base.scale.0).abs() > 1e-3 || (sy - base.scale.1).abs() > 1e-3 {
                let target = Target::Element(e.clone());
                writes.push((target.clone(), Key::ScaleX, sx as f64, base.scale.0 as f64));
                writes.push((target, Key::ScaleY, sy as f64, base.scale.1 as f64));
            }
        } else if let Some((x0, y0)) = previous {
            let (dx, dy) = (x0 - at.0, y0 - at.1);
            if dx.abs() < 0.01 && dy.abs() < 0.01 {
                return;
            }
            writes.extend(hold(&self.element, dx, dy));
        }
        if writes.is_empty() {
            return;
        }
        let Some(engine) = vm.upgrade().and_then(|vm| engine_of(&vm)) else {
            return;
        };
        start(&engine, vm.clone(), writes, &self.timing);
    }
}

/// A script's mounted `layout_id` nodes, and those just unmounted (until
/// the next layout pass): what a node mounting with the same id starts from.
#[derive(Default)]
pub(crate) struct LayoutIds(RefCell<HashMap<Rc<str>, Rc<Tracker>>>);

/// Tracks a mounted node's layout moves (`layout = "position"`, a
/// `layout_id`) until it is removed.
pub(crate) fn track(
    element: &Rc<Element>,
    handle: &NodeHandle,
    layout_id: Option<Rc<str>>,
    timing: Timing,
    depth: usize,
    vm: &Rc<Vm>,
) {
    let tracker = Rc::new(Tracker {
        element: element.clone(),
        handle: handle.clone(),
        depth,
        timing,
        at: Cell::new(None),
        last: Cell::new(None),
        from: Cell::new(None),
    });
    if let Some(id) = layout_id {
        let previous = vm
            .layout_ids
            .0
            .borrow_mut()
            .insert(id.clone(), tracker.clone());
        if let Some(previous) = previous {
            tracker.from.set(previous.visual());
        }
        handle.retain(Departure {
            id,
            tracker: Rc::downgrade(&tracker),
            vm: Rc::downgrade(vm),
            handle: handle.clone(),
        });
    }
    let vm = Rc::downgrade(vm);
    handle.on_layout(move || tracker.laid_out(&vm));
}

/// Unmounting a `layout_id` node: a successor mounting before the next
/// layout pass starts from it; after that pass it is forgotten.
struct Departure {
    id: Rc<str>,
    tracker: Weak<Tracker>,
    vm: Weak<Vm>,
    handle: NodeHandle,
}
impl Drop for Departure {
    fn drop(&mut self) {
        let (id, tracker, vm) = (self.id.clone(), self.tracker.clone(), self.vm.clone());
        self.handle.after_layout(move || {
            let Some(vm) = vm.upgrade() else { return };
            let mut ids = vm.layout_ids.0.borrow_mut();
            if ids
                .get(&id)
                .is_some_and(|t| std::ptr::eq(Rc::as_ptr(t), tracker.as_ptr()))
            {
                ids.remove(&id);
            }
        });
    }
}
