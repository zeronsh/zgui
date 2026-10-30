//! anime.js `createDraggable(target, params)`: drags an element's
//! translation with the pointer, within a container if given, and on
//! release throws it with its velocity onto a snap point with a spring.
//! Everything per move and per frame runs in Rust; scripts hear the
//! callbacks they set.
use super::{
    ease::{Ease, Spring},
    engine::{Callbacks, Clip, Engine},
    target::{Key, Target},
    value::V,
};
use crate::{
    Vm,
    element::Element,
    node::{Lit, Prop},
};
use mlua::{Function, Lua, MetaMethod, Table, UserData, UserDataMethods};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::Instant,
};
use zgx::input::{EventContext, InputEvent, PointerButton};

#[derive(Clone, Default)]
enum Snap {
    #[default]
    None,
    Step(f64),
    Points(Vec<f64>),
}
impl Snap {
    fn apply(&self, v: f64) -> f64 {
        match self {
            Snap::None => v,
            Snap::Step(step) if *step > 0. => (v / step).round() * step,
            Snap::Step(_) => v,
            Snap::Points(points) => points
                .iter()
                .copied()
                .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
                .unwrap_or(v),
        }
    }
}

#[derive(Clone, Default)]
struct Axis {
    enabled: bool,
    snap: Snap,
}

enum Container {
    None,
    /// Translation limits: top, right, bottom, left.
    Bounds([f64; 4]),
    Target(mlua::Value),
}

#[derive(Default)]
struct DragState {
    /// Weak: the element's listener holds the draggable.
    element: Option<Weak<Element>>,
    listener: Option<crate::element::Listener>,
    enabled: bool,
    dragging: bool,
    pointer: (f64, f64),
    start: (f64, f64),
    limits: [f64; 4],
    velocity: (f64, f64),
    last: Option<(Instant, f64, f64)>,
    release: Option<Rc<Clip>>,
}

pub(crate) struct Draggable {
    engine: Rc<Engine>,
    /// Whose state it lives in (its userdata): through this, not a handle
    /// of its own, which would keep the state from closing.
    owner: Weak<Vm>,
    x: Axis,
    y: Axis,
    container: Container,
    padding: [f64; 4],
    friction: f64,
    drag_speed: f64,
    velocity_multiplier: f64,
    max_velocity: f64,
    min_velocity: f64,
    release: (f64, f64, f64),
    cursor: bool,
    callbacks: [Option<Function>; 6],
    state: RefCell<DragState>,
    this: RefCell<Weak<Draggable>>,
}
const GRAB: usize = 0;
const DRAG: usize = 1;
const UPDATE: usize = 2;
const RELEASE: usize = 3;
const SNAP: usize = 4;
const SETTLE: usize = 5;

impl Draggable {
    fn element(&self) -> Option<Rc<Element>> {
        self.state.borrow().element.as_ref().and_then(Weak::upgrade)
    }
    fn position(&self) -> (f64, f64) {
        self.element().map_or((0., 0.), |e| {
            let (x, y) = e.translate();
            (x as f64, y as f64)
        })
    }
    fn call(&self, which: usize) {
        let Some(f) = &self.callbacks[which] else {
            return;
        };
        let (Some(vm), Some(this)) = (self.owner.upgrade(), self.this.borrow().upgrade()) else {
            return;
        };
        if let Err(e) = vm.call::<()>(f, DraggableUd(this)) {
            vm.report(e);
        }
    }
    /// Translation limits from the container, as [top, right, bottom, left].
    fn limits(&self, element: &Element) -> [f64; 4] {
        let unbounded = [
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let own = element.read(|s, id| s.bounds(id));
        match &self.container {
            Container::None => unbounded,
            Container::Bounds(b) => *b,
            Container::Target(v) => {
                let Some(vm) = self.owner.upgrade() else {
                    return unbounded;
                };
                let ctx = super::build::Ctx {
                    lua: vm.lua.clone(),
                    vm,
                    engine: self.engine.clone(),
                };
                let Ok(targets) = super::api::targets(&ctx, v.clone()) else {
                    return unbounded;
                };
                let Some(Target::Element(container)) = targets.into_iter().next() else {
                    return unbounded;
                };
                let (Some(c), Some(t)) = (container.read(|s, id| s.bounds(id)), own) else {
                    return unbounded;
                };
                let [pt, pr, pb, pl] = self.padding;
                [
                    (c.y - t.y) as f64 + pt,
                    ((c.x + c.width) - (t.x + t.width)) as f64 - pr,
                    ((c.y + c.height) - (t.y + t.height)) as f64 - pb,
                    (c.x - t.x) as f64 + pl,
                ]
            }
        }
    }
    /// Past a limit, a drag moves `1 - friction` as far.
    fn resist(&self, v: f64, min: f64, max: f64) -> f64 {
        let give = 1. - self.friction.clamp(0., 1.);
        if v < min {
            min - (min - v) * give
        } else if v > max {
            max + (v - max) * give
        } else {
            v
        }
    }
    fn write(&self, x: f64, y: f64) {
        let Some(element) = self.element() else {
            return;
        };
        let target = Target::Element(element);
        let mut writes = Vec::new();
        if self.x.enabled {
            writes.push((target.clone(), Key::X, V::Num(x), None));
        }
        if self.y.enabled {
            writes.push((target, Key::Y, V::Num(y), None));
        }
        self.engine.set_now(writes);
    }
    fn on_event(self: &Rc<Self>, event: &mut EventContext) {
        if !self.state.borrow().enabled {
            return;
        }
        match event.event.clone() {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                let Some(element) = self.element() else {
                    return;
                };
                if let Some(release) = self.state.borrow_mut().release.take() {
                    release.cancel();
                }
                let limits = self.limits(&element);
                let start = self.position();
                {
                    let mut s = self.state.borrow_mut();
                    s.dragging = true;
                    s.pointer = (x as f64, y as f64);
                    s.start = start;
                    s.limits = limits;
                    s.velocity = (0., 0.);
                    s.last = Some((Instant::now(), start.0, start.1));
                }
                if self.cursor {
                    element.set_style(Prop::Cursor, Lit::Str("grabbing".into()));
                    self.engine.set_now(Vec::new());
                }
                event.capture_pointer();
                event.stop_propagation();
                self.call(GRAB);
            }
            InputEvent::PointerMove { x, y } if self.state.borrow().dragging => {
                let (nx, ny) = {
                    let s = self.state.borrow();
                    let dx = (x as f64 - s.pointer.0) * self.drag_speed;
                    let dy = (y as f64 - s.pointer.1) * self.drag_speed;
                    let [top, right, bottom, left] = s.limits;
                    (
                        self.resist(s.start.0 + dx, left, right),
                        self.resist(s.start.1 + dy, top, bottom),
                    )
                };
                let now = Instant::now();
                {
                    let mut s = self.state.borrow_mut();
                    if let Some((at, px, py)) = s.last {
                        let ms = (now - at).as_secs_f64() * 1000.;
                        if ms > 0. {
                            // Smoothed px/ms, so one jittery move does not
                            // decide the throw.
                            let (vx, vy) = ((nx - px) / ms, (ny - py) / ms);
                            s.velocity =
                                (s.velocity.0 * 0.5 + vx * 0.5, s.velocity.1 * 0.5 + vy * 0.5);
                        }
                    }
                    s.last = Some((now, nx, ny));
                }
                self.write(nx, ny);
                self.call(DRAG);
                self.call(UPDATE);
            }
            InputEvent::PointerUp { .. } | InputEvent::PointerCancel
                if self.state.borrow().dragging =>
            {
                self.state.borrow_mut().dragging = false;
                event.release_pointer();
                self.throw();
            }
            _ => {}
        }
    }
    /// Projects the release velocity, clamps and snaps, then springs there.
    fn throw(self: &Rc<Self>) {
        let Some(element) = self.element() else {
            return;
        };
        if self.cursor {
            element.set_style(Prop::Cursor, Lit::Str("grab".into()));
        }
        self.call(RELEASE);
        let (x, y) = self.position();
        let (limits, (vx, vy)) = {
            let s = self.state.borrow();
            (s.limits, s.velocity)
        };
        let clamp_v = |v: f64| {
            let speed =
                (v.abs() * self.velocity_multiplier).clamp(self.min_velocity, self.max_velocity);
            speed * v.signum()
        };
        // Velocity in px/ms carried for about a sixth of a second.
        let projected = (x + clamp_v(vx) * 160., y + clamp_v(vy) * 160.);
        let [top, right, bottom, left] = limits;
        let tx = self
            .x
            .snap
            .apply(projected.0)
            .clamp(left.min(right), right.max(left));
        let ty = self
            .y
            .snap
            .apply(projected.1)
            .clamp(top.min(bottom), bottom.max(top));
        let snapped = (tx, ty) != projected;
        let (mass, stiffness, damping) = self.release;
        let spring = Rc::new(Spring::new(mass, stiffness, damping, 0.));
        let duration = spring.settle * 1000.;
        let target = Target::Element(element);
        let mut writes = Vec::new();
        if self.x.enabled {
            writes.push((target.clone(), Key::X, V::Num(tx)));
        }
        if self.y.enabled {
            writes.push((target, Key::Y, V::Num(ty)));
        }
        let settle = self.callbacks[SETTLE].clone().map(|f| {
            let this = Rc::downgrade(self);
            let Some(vm) = self.owner.upgrade() else {
                return f;
            };
            // The settle callback receives the draggable, not the clip.
            vm.lua
                .create_function(move |_, _: mlua::MultiValue| {
                    if let Some(this) = this.upgrade() {
                        this.call(SETTLE);
                    }
                    Ok(())
                })
                .unwrap_or(f)
        });
        let clip = self.engine.tween(
            self.owner.clone(),
            writes,
            duration,
            Ease::Spring(spring),
            Callbacks {
                complete: settle,
                ..Default::default()
            },
        );
        self.state.borrow_mut().release = Some(clip);
        if snapped {
            self.call(SNAP);
        }
    }
    /// Attaches to the target once it is mounted. The element's listener
    /// keeps the draggable alive, as anime.js keeps draggables whether or
    /// not a script holds them.
    fn attach(self: &Rc<Self>, element: Rc<Element>) {
        let this = self.clone();
        let listener: crate::element::Listener = Rc::new(move |event| this.on_event(event));
        element.listeners.borrow_mut().push(listener.clone());
        if self.cursor {
            element.set_style(Prop::Cursor, Lit::Str("grab".into()));
            self.engine.set_now(Vec::new());
        }
        let mut s = self.state.borrow_mut();
        s.element = Some(Rc::downgrade(&element));
        s.listener = Some(listener);
    }
    fn detach(&self) {
        let mut s = self.state.borrow_mut();
        let listener = s.listener.take();
        if let (Some(element), Some(listener)) =
            (s.element.as_ref().and_then(Weak::upgrade), listener)
        {
            element
                .listeners
                .borrow_mut()
                .retain(|l| !Rc::ptr_eq(l, &listener));
        }
        if let Some(release) = s.release.take() {
            release.cancel();
        }
    }
}

#[derive(Clone)]
pub(crate) struct DraggableUd(pub Rc<Draggable>);
impl UserData for DraggableUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let d = this.0.clone();
            Ok(match key.as_str() {
                "x" => mlua::Value::Number(d.position().0),
                "y" => mlua::Value::Number(d.position().1),
                "isDragging" => mlua::Value::Boolean(d.state.borrow().dragging),
                "enabled" => mlua::Value::Boolean(d.state.borrow().enabled),
                "velocity" => {
                    let (vx, vy) = d.state.borrow().velocity;
                    mlua::Value::Number((vx * vx + vy * vy).sqrt())
                }
                "enable" | "disable" | "reset" | "revert" | "stop" | "setX" | "setY" => {
                    let f = lua.create_function(move |_, args: mlua::MultiValue| {
                        let mut args: Vec<mlua::Value> = args.into_iter().collect();
                        if matches!(args.first(), Some(mlua::Value::UserData(ud)) if ud.is::<DraggableUd>()) {
                            args.remove(0);
                        }
                        let number = |i: usize| match args.get(i) {
                            Some(mlua::Value::Integer(n)) => *n as f64,
                            Some(mlua::Value::Number(n)) => *n,
                            _ => 0.,
                        };
                        match key.as_str() {
                            "enable" => d.state.borrow_mut().enabled = true,
                            "disable" => d.state.borrow_mut().enabled = false,
                            "stop" => {
                                if let Some(release) = d.state.borrow_mut().release.take() {
                                    release.cancel();
                                }
                            }
                            "reset" => d.write(0., 0.),
                            "setX" => {
                                let y = d.position().1;
                                d.write(number(0), y);
                            }
                            "setY" => {
                                let x = d.position().0;
                                d.write(x, number(0));
                            }
                            _ => {
                                d.detach();
                                d.write(0., 0.);
                            }
                        }
                        Ok(())
                    })?;
                    mlua::Value::Function(f)
                }
                _ => mlua::Value::Nil,
            })
        });
    }
}

fn axis(v: mlua::Value) -> mlua::Result<Axis> {
    Ok(match v {
        mlua::Value::Nil | mlua::Value::Boolean(true) => Axis {
            enabled: true,
            snap: Snap::None,
        },
        mlua::Value::Boolean(false) => Axis::default(),
        mlua::Value::Table(t) => Axis {
            enabled: true,
            snap: snap(t.get("snap")?)?,
        },
        other => {
            return Err(mlua::Error::runtime(format!(
                "an axis is true, false or a table, not {}",
                other.type_name()
            )));
        }
    })
}
fn snap(v: mlua::Value) -> mlua::Result<Snap> {
    Ok(match v {
        mlua::Value::Nil => Snap::None,
        mlua::Value::Integer(n) => Snap::Step(n as f64),
        mlua::Value::Number(n) => Snap::Step(n),
        mlua::Value::Table(t) => {
            Snap::Points(t.sequence_values::<f64>().collect::<mlua::Result<_>>()?)
        }
        other => {
            return Err(mlua::Error::runtime(format!(
                "snap is a step or a list of points, not {}",
                other.type_name()
            )));
        }
    })
}
fn four(v: mlua::Value) -> mlua::Result<[f64; 4]> {
    Ok(match v {
        mlua::Value::Integer(n) => [n as f64; 4],
        mlua::Value::Number(n) => [n; 4],
        mlua::Value::Table(t) => {
            let v: Vec<f64> = t.sequence_values().collect::<mlua::Result<_>>()?;
            match v.as_slice() {
                [a] => [*a; 4],
                [a, b] => [*a, *b, *a, *b],
                [a, b, c, d] => [*a, *b, *c, *d],
                _ => return Err(mlua::Error::runtime("give one, two or four values")),
            }
        }
        _ => [0.; 4],
    })
}

/// `anime.createDraggable(target, params)`.
pub(crate) fn create(
    lua: &Lua,
    target: mlua::Value,
    params: Option<Table>,
) -> mlua::Result<DraggableUd> {
    let env = crate::node::env(lua)?;
    let engine = env.engine.clone();
    let owner = env.vm.clone();
    drop(env);
    let params = match params {
        Some(p) => p,
        None => lua.create_table()?,
    };
    let get = |k: &str| params.get::<mlua::Value>(k);
    let number =
        |k: &str, d: f64| -> mlua::Result<f64> { Ok(params.get::<Option<f64>>(k)?.unwrap_or(d)) };
    let mut x = axis(get("x")?)?;
    let mut y = axis(get("y")?)?;
    // A shared `snap` applies to both axes.
    if let s @ (Snap::Step(_) | Snap::Points(_)) = snap(get("snap")?)? {
        if matches!(x.snap, Snap::None) {
            x.snap = s.clone();
        }
        if matches!(y.snap, Snap::None) {
            y.snap = s;
        }
    }
    let container = match get("container")? {
        mlua::Value::Nil => Container::None,
        mlua::Value::Table(t) if t.raw_len() == 4 => {
            Container::Bounds(four(mlua::Value::Table(t))?)
        }
        other => Container::Target(other),
    };
    let callback = |names: &[&str]| -> mlua::Result<Option<Function>> {
        for n in names {
            if let Some(f) = params.get::<Option<Function>>(*n)? {
                return Ok(Some(f));
            }
        }
        Ok(None)
    };
    let draggable = Rc::new(Draggable {
        engine: engine.clone(),
        owner,
        x,
        y,
        container,
        padding: four(get("containerPadding")?)?,
        friction: number("containerFriction", 0.8)?,
        drag_speed: number("dragSpeed", 1.)?,
        velocity_multiplier: number("velocityMultiplier", 1.)?,
        max_velocity: number("maxVelocity", 50.)?,
        min_velocity: number("minVelocity", 0.)?,
        release: (
            number("releaseMass", 1.)?,
            number("releaseStiffness", 80.)?,
            number("releaseDamping", 20.)?,
        ),
        cursor: !matches!(get("cursor")?, mlua::Value::Boolean(false)),
        callbacks: [
            callback(&["onGrab", "on_grab"])?,
            callback(&["onDrag", "on_drag"])?,
            callback(&["onUpdate", "on_update"])?,
            callback(&["onRelease", "on_release"])?,
            callback(&["onSnap", "on_snap"])?,
            callback(&["onSettle", "on_settle"])?,
        ],
        state: RefCell::new(DragState {
            enabled: true,
            ..Default::default()
        }),
        this: RefCell::default(),
    });
    *draggable.this.borrow_mut() = Rc::downgrade(&draggable);
    // The target may not be mounted yet: attach once it is. Until then the
    // pending attachment keeps the draggable.
    let (this, attached) = (draggable.clone(), Rc::new(Cell::new(false)));
    engine.defer(Rc::new(move || {
        if attached.get() {
            return true;
        }
        let Some(vm) = this.owner.upgrade() else {
            return true;
        };
        let ctx = super::build::Ctx {
            lua: vm.lua.clone(),
            vm,
            engine: this.engine.clone(),
        };
        let Ok(targets) = super::api::targets(&ctx, target.clone()) else {
            return true;
        };
        match targets.into_iter().find_map(|t| match t {
            Target::Element(e) => Some(e),
            _ => None,
        }) {
            Some(element) => {
                this.attach(element);
                attached.set(true);
                true
            }
            None => false,
        }
    }));
    Ok(DraggableUd(draggable))
}
