//! The `anime` global: anime.js v4's API for scripts. Methods work both as
//! `anim.play()` (as in JavaScript) and `anim:play()`.
use super::{
    build::{self, Ctx, Params, TlOp},
    ease::{Ease, Spring},
    engine::{Clip, Engine, Modifier},
    stagger::{From, Stagger},
    target::{Key, Target, Unit},
    value::{self, V},
};
use crate::{
    element::{ElementUd, RefUd},
    node::Env,
    value::State,
};
use mlua::{Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(crate) struct ClipUd(pub Rc<Clip>);
/// A value along a motion path (`svg.createMotionPath`).
#[derive(Clone)]
pub(crate) struct PathValueUd(pub Rc<super::svg::Sampler>, pub super::svg::Axis);
impl UserData for PathValueUd {}
#[derive(Clone)]
pub(crate) struct StaggerUd(pub Rc<Stagger>);
impl UserData for StaggerUd {}
#[derive(Clone)]
pub(crate) struct EaseUd(pub Ease);
impl UserData for EaseUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // An ease is also a function of progress.
        methods.add_meta_method(MetaMethod::Call, |lua, e, t: f64| {
            let vm = env(lua).ok().and_then(|env| env.vm.upgrade());
            Ok(e.0.at(t, vm.as_deref()))
        });
    }
}
#[derive(Clone)]
pub(crate) struct ModifierUd(pub Modifier);
impl UserData for ModifierUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Call, |lua, m, n: f64| {
            let vm = env(lua).ok().and_then(|env| env.vm.upgrade());
            Ok(match m.0.clone() {
                Modifier::Round(d) => {
                    let scale = 10f64.powf(d);
                    (n * scale).round() / scale
                }
                Modifier::Script(f) => match vm {
                    Some(vm) => vm.call::<f64>(&f, n)?,
                    None => n,
                },
            })
        });
    }
}

fn env(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, Env>> {
    lua.app_data_ref::<Env>()
        .ok_or_else(|| mlua::Error::runtime("anime used outside a script host"))
}
fn ctx(lua: &Lua) -> mlua::Result<Ctx> {
    let env = env(lua)?;
    let vm = env
        .vm
        .upgrade()
        .ok_or_else(|| mlua::Error::runtime("script unloaded"))?;
    Ok(Ctx {
        lua: lua.clone(),
        vm,
        engine: env.engine.clone(),
    })
}
fn rt(e: String) -> mlua::Error {
    mlua::Error::runtime(e)
}

/// Targets: selectors, refs, elements, states, plain tables, or arrays of
/// them; elements in document order without repeats.
pub(crate) fn targets(ctx: &Ctx, v: mlua::Value) -> Result<Vec<Target>, String> {
    let mut out: Vec<Target> = Vec::new();
    collect(ctx, v, &mut out)?;
    Ok(out)
}
fn collect(ctx: &Ctx, v: mlua::Value, out: &mut Vec<Target>) -> Result<(), String> {
    let mut push = |t: Target| {
        if !out.iter().any(|o| o.same(&t)) {
            out.push(t);
        }
    };
    match v {
        mlua::Value::Nil => {}
        mlua::Value::String(s) => {
            for e in ctx.engine.elements.select(&s.to_string_lossy()) {
                push(Target::Element(e));
            }
        }
        mlua::Value::UserData(ud) => {
            if let Ok(r) = ud.borrow::<RefUd>() {
                for e in r.0.elements() {
                    push(Target::Element(e));
                }
            } else if let Ok(e) = ud.borrow::<ElementUd>() {
                push(Target::Element(e.0.clone()));
            } else if let Ok(s) = ud.borrow::<State>() {
                push(Target::State(s.0.clone()));
            } else {
                return Err("targets are selectors, refs, elements, states or tables".into());
            }
        }
        mlua::Value::Table(t) => {
            if t.raw_len() > 0 {
                for item in t.sequence_values::<mlua::Value>() {
                    collect(ctx, item.map_err(|e| e.to_string())?, out)?;
                }
            } else {
                push(Target::Table(t));
            }
        }
        other => return Err(format!("targets cannot be {}", other.type_name())),
    }
    Ok(())
}

/// `animate(targets, params)`.
pub(crate) fn animate(
    lua: &Lua,
    targets_value: mlua::Value,
    params: Table,
) -> mlua::Result<ClipUd> {
    let ctx = ctx(lua)?;
    scoped(&ctx.engine, &params, None)?;
    // Milliseconds, unless the script set `engine.timeUnit = "s"`.
    if !ctx.engine.seconds.get()
        && let Ok(Some(ms)) = params.raw_get::<Option<f64>>("duration")
    {
        let looping = !matches!(
            params.raw_get::<mlua::Value>("loop")?,
            mlua::Value::Nil | mlua::Value::Boolean(false) | mlua::Value::Integer(0)
        );
        crate::check_duration(lua, "duration", ms, looping);
    }
    let (timing, callbacks, id) = {
        let p = Params {
            own: &params,
            defaults: None,
        };
        (
            build::timing(&ctx, &p).map_err(rt)?,
            build::callbacks(&p),
            p.get(&["id"]),
        )
    };
    let autoplay = timing.autoplay;
    let (lua2, engine, own) = (lua.clone(), ctx.engine.clone(), params.clone());
    let builder: super::engine::Builder = Box::new(move |clip| {
        let params = &own;
        let ctx = Ctx {
            lua: lua2.clone(),
            vm: clip.owner.upgrade().ok_or("script unloaded")?,
            engine: engine.clone(),
        };
        let targets = targets(&ctx, targets_value.clone())?;
        let compiled = build::compile(
            &ctx,
            &targets,
            &Params {
                own: params,
                defaults: None,
            },
            &|_, _, _| None,
        )?;
        *clip.tracks.borrow_mut() = compiled.tracks;
        clip.timing.borrow_mut().iteration = compiled.iteration;
        Ok(())
    });
    let clip = Clip::new(&ctx.engine, Rc::downgrade(&ctx.vm), timing, Some(builder));
    *clip.callbacks.borrow_mut() = Rc::new(callbacks);
    if let mlua::Value::String(id) = id {
        *clip.label.borrow_mut() = Some(id.to_string_lossy());
    }
    if autoplay {
        clip.start();
        clip.prime();
    }
    link_scroll(lua, &ctx.engine, &clip, &params)?;
    scoped(&ctx.engine, &params, Some(&clip))?;
    Ok(ClipUd(clip))
}

/// `autoplay = anime.onScroll(…)`.
fn link_scroll(
    lua: &Lua,
    engine: &Rc<Engine>,
    clip: &Rc<Clip>,
    params: &Table,
) -> mlua::Result<()> {
    if let mlua::Value::UserData(ud) = params.raw_get::<mlua::Value>("autoplay")?
        && let Ok(scroll) = ud.borrow::<super::scroll::ScrollUd>()
    {
        super::scroll::link(lua, engine, clip.clone(), scroll.0.clone());
    }
    Ok(())
}

fn timeline(lua: &Lua, params: Option<Table>) -> mlua::Result<ClipUd> {
    let ctx = ctx(lua)?;
    let params = match params {
        Some(p) => p,
        None => lua.create_table()?,
    };
    let p = Params {
        own: &params,
        defaults: None,
    };
    let timing = build::timing(&ctx, &p).map_err(rt)?;
    let autoplay = timing.autoplay;
    let (lua2, engine) = (lua.clone(), ctx.engine.clone());
    let builder: super::engine::Builder = Box::new(move |clip| {
        let ctx = Ctx {
            lua: lua2.clone(),
            vm: clip.owner.upgrade().ok_or("script unloaded")?,
            engine: engine.clone(),
        };
        let ops = std::mem::take(&mut *clip.ops.borrow_mut());
        let defaults = clip.defaults.borrow().clone();
        let result = build::build_timeline(&ctx, clip, &ops, defaults.as_ref());
        *clip.ops.borrow_mut() = ops;
        result
    });
    let clip = Clip::new(&ctx.engine, Rc::downgrade(&ctx.vm), timing, Some(builder));
    clip.is_timeline.set(true);
    *clip.callbacks.borrow_mut() = Rc::new(build::callbacks(&p));
    if let mlua::Value::Table(defaults) = p.get(&["defaults"]) {
        *clip.defaults.borrow_mut() = Some(defaults);
    }
    if autoplay {
        clip.start();
    }
    link_scroll(lua, &ctx.engine, &clip, &params)?;
    Ok(ClipUd(clip))
}

fn timer(lua: &Lua, params: Option<Table>) -> mlua::Result<ClipUd> {
    let ctx = ctx(lua)?;
    let params = match params {
        Some(p) => p,
        None => lua.create_table()?,
    };
    let p = Params {
        own: &params,
        defaults: None,
    };
    let mut timing = build::timing(&ctx, &p).map_err(rt)?;
    // A timer without a duration runs until stopped, as in anime.js.
    timing.iteration = match p.get(&["duration"]) {
        mlua::Value::Nil => f64::INFINITY,
        v => lua.unpack::<f64>(v).map(|d| ctx.engine.ms(d))?,
    };
    timing.delay = match p.get(&["delay"]) {
        mlua::Value::Nil => 0.,
        v => lua.unpack::<f64>(v).map(|d| ctx.engine.ms(d))?,
    };
    let autoplay = timing.autoplay;
    let clip = Clip::new(&ctx.engine, Rc::downgrade(&ctx.vm), timing, None);
    *clip.callbacks.borrow_mut() = Rc::new(build::callbacks(&p));
    if autoplay {
        clip.start();
    }
    Ok(ClipUd(clip))
}

/// The animation, timeline and timer API, with JavaScript-style bound
/// methods: `anim.play()` and `anim:play()` both work.
impl UserData for ClipUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            index(lua, this, &key)
        });
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, value): (String, mlua::Value)| {
                let clip = &this.0;
                let number = || lua.unpack::<f64>(value.clone());
                match key.as_str() {
                    "speed" | "playbackRate" => clip.set_speed(number()?),
                    "currentTime" => {
                        let engine = clip
                            .engine()
                            .ok_or_else(|| mlua::Error::runtime("engine gone"))?;
                        clip.seek(engine.ms(number()?));
                    }
                    "progress" => {
                        let total = clip.total();
                        if total.is_finite() {
                            clip.seek(total * number()?.clamp(0., 1.));
                        }
                    }
                    "paused" => {
                        if lua.unpack::<bool>(value)? {
                            clip.pause()
                        } else {
                            clip.resume()
                        }
                    }
                    other => return Err(mlua::Error::runtime(format!("cannot set `{other}`"))),
                }
                Ok(())
            },
        );
        methods.add_meta_method(MetaMethod::Eq, |_, a, b: mlua::AnyUserData| {
            Ok(b.borrow::<ClipUd>().is_ok_and(|b| Rc::ptr_eq(&a.0, &b.0)))
        });
    }
}

fn index(lua: &Lua, this: &ClipUd, key: &str) -> mlua::Result<mlua::Value> {
    let clip = this.0.clone();
    let engine = clip
        .engine()
        .ok_or_else(|| mlua::Error::runtime("engine gone"))?;
    let ms = |v: f64| engine.script_time(v);
    let field = |v: mlua::Value| Ok(v);
    match key {
        "id" => {
            return field(
                clip.label
                    .borrow()
                    .clone()
                    .map_or(mlua::Value::Integer(clip.id as i64), |l| {
                        mlua::Value::String(lua.create_string(l).unwrap())
                    }),
            );
        }
        "currentTime" => return field(mlua::Value::Number(ms(clip.time()))),
        "duration" => return field(mlua::Value::Number(ms(clip.total()))),
        "iterationDuration" => {
            clip.ensure_built();
            return field(mlua::Value::Number(ms(clip.timing.borrow().iteration)));
        }
        "progress" => {
            let total = clip.total();
            return field(mlua::Value::Number(if total.is_finite() && total > 0. {
                clip.time() / total
            } else {
                0.
            }));
        }
        "iterationProgress" | "iterationCurrentTime" => {
            clip.ensure_built();
            let t = clip.timing.borrow().clone();
            let local = (clip.time() - t.delay).max(0.);
            let cycle = t.iteration + t.loop_delay;
            let within = if cycle > 0. && cycle.is_finite() {
                (local - clip.iteration() * cycle).min(t.iteration)
            } else {
                local
            };
            return field(mlua::Value::Number(if key == "iterationProgress" {
                if t.iteration > 0. && t.iteration.is_finite() {
                    within / t.iteration
                } else {
                    0.
                }
            } else {
                ms(within)
            }));
        }
        "currentIteration" => return field(mlua::Value::Number(clip.iteration())),
        "paused" => {
            return field(mlua::Value::Boolean(
                clip.is_paused() || !clip.is_playing() && !clip.is_completed(),
            ));
        }
        "began" => return field(mlua::Value::Boolean(clip.has_began())),
        "completed" => return field(mlua::Value::Boolean(clip.is_completed())),
        "reversed" => return field(mlua::Value::Boolean(clip.is_backwards())),
        "speed" | "playbackRate" => return field(mlua::Value::Number(clip.timing.borrow().speed)),
        "labels" => {
            clip.ensure_built();
            let t = lua.create_table()?;
            for (k, v) in clip.labels.borrow().iter() {
                t.set(k.as_str(), ms(*v))?;
            }
            return field(mlua::Value::Table(t));
        }
        _ => {}
    }
    // Methods, bound to this clip: called with or without `self`.
    let name = key.to_owned();
    let is_method = matches!(
        key,
        "play"
            | "pause"
            | "resume"
            | "restart"
            | "reverse"
            | "alternate"
            | "seek"
            | "complete"
            | "cancel"
            | "revert"
            | "reset"
            | "stretch"
            | "refresh"
            | "init"
            | "then"
            | "done"
            | "add"
            | "set"
            | "call"
            | "label"
            | "sync"
            | "remove"
    );
    if !is_method {
        return Ok(mlua::Value::Nil);
    }
    let f = lua.create_function(move |lua, args: mlua::MultiValue| {
        let mut args: Vec<mlua::Value> = args.into_iter().collect();
        if let Some(mlua::Value::UserData(ud)) = args.first()
            && ud.borrow::<ClipUd>().is_ok_and(|c| Rc::ptr_eq(&c.0, &clip))
        {
            args.remove(0);
        }
        call_method(lua, &clip, &name, args)
    })?;
    Ok(mlua::Value::Function(f))
}

fn call_method(
    lua: &Lua,
    clip: &Rc<Clip>,
    name: &str,
    args: Vec<mlua::Value>,
) -> mlua::Result<mlua::Value> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(mlua::Value::Nil);
    let engine = clip
        .engine()
        .ok_or_else(|| mlua::Error::runtime("engine gone"))?;
    let this = || {
        lua.create_userdata(ClipUd(clip.clone()))
            .map(mlua::Value::UserData)
    };
    match name {
        "play" => clip.play(),
        "pause" => clip.pause(),
        "resume" => clip.resume(),
        "restart" => clip.restart(),
        "reverse" => clip.reverse(),
        "alternate" => clip.alternate(),
        "seek" => clip.seek(engine.ms(lua.unpack::<f64>(arg(0))?)),
        "complete" => clip.complete(),
        "cancel" => clip.cancel(),
        "revert" => clip.revert(),
        "reset" => clip.reset(),
        "stretch" => clip.stretch(engine.ms(lua.unpack::<f64>(arg(0))?)),
        "refresh" => clip.invalidate(),
        "init" => clip.ensure_built(),
        // `then` is a Luau keyword: `a:done(fn)`, or `a["then"](fn)`.
        "then" | "done" => {
            let f: mlua::Function = lua.unpack(arg(0))?;
            if clip.is_completed() {
                f.call::<()>(this()?)?;
            } else {
                let mut callbacks = clip.callbacks.borrow_mut();
                let mut updated = (**callbacks).clone_callbacks();
                updated.then.push(f);
                *callbacks = Rc::new(updated);
            }
        }
        "add" | "set" | "call" | "label" | "sync" | "remove" => {
            if !clip.is_timeline.get() {
                return Err(mlua::Error::runtime(format!(
                    "`{name}` is a timeline method"
                )));
            }
            let op = match name {
                "add" => match (arg(0), arg(1)) {
                    (targets, mlua::Value::Table(params)) => TlOp::Add {
                        targets,
                        params,
                        position: arg(2),
                    },
                    (mlua::Value::Table(params), position) => TlOp::Timer { params, position },
                    _ => {
                        return Err(mlua::Error::runtime(
                            "add(targets, params, position) or add(timerParams, position)",
                        ));
                    }
                },
                "set" => TlOp::Set {
                    targets: arg(0),
                    params: lua.unpack(arg(1))?,
                    position: arg(2),
                },
                "call" => TlOp::Call {
                    f: lua.unpack(arg(0))?,
                    position: arg(1),
                },
                "label" => TlOp::Label {
                    name: lua.unpack(arg(0))?,
                    position: arg(1),
                },
                "sync" => {
                    let other = match arg(0) {
                        mlua::Value::UserData(ud) => ud.borrow::<ClipUd>()?.0.clone(),
                        _ => {
                            return Err(mlua::Error::runtime(
                                "sync takes an animation, timer or timeline",
                            ));
                        }
                    };
                    TlOp::Sync {
                        clip: other,
                        position: arg(1),
                    }
                }
                _ => {
                    let ctx = ctx(lua)?;
                    let gone = targets(&ctx, arg(0)).map_err(rt)?;
                    clip.ensure_built();
                    clip.tracks
                        .borrow_mut()
                        .retain(|t| !gone.iter().any(|g| g.same(&t.target)));
                    return this();
                }
            };
            clip.ops.borrow_mut().push(op);
            clip.invalidate();
        }
        _ => unreachable!(),
    }
    this()
}

impl super::engine::Callbacks {
    fn clone_callbacks(&self) -> Self {
        Self {
            begin: self.begin.clone(),
            before_update: self.before_update.clone(),
            update: self.update.clone(),
            render: self.render.clone(),
            loop_: self.loop_.clone(),
            pause: self.pause.clone(),
            complete: self.complete.clone(),
            then: self.then.clone(),
        }
    }
}

/// `createAnimatable(targets, params)`: each listed property becomes a
/// function that animates its targets to a new value (and reads it when
/// called with nothing), for following the pointer and similar.
struct Animatable {
    targets: mlua::Value,
    props: Vec<(String, Table)>,
    defaults: Table,
    playing: RefCell<Vec<Rc<Clip>>>,
}
#[derive(Clone)]
struct AnimatableUd(Rc<Animatable>);
impl UserData for AnimatableUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let a = this.0.clone();
            if key == "revert" || key == "cancel" {
                let f = lua.create_function(move |_, _: mlua::MultiValue| {
                    for clip in a.playing.borrow_mut().drain(..) {
                        if key == "revert" { clip.revert() } else { clip.cancel() }
                    }
                    Ok(())
                })?;
                return Ok(mlua::Value::Function(f));
            }
            let Some((name, config)) = a.props.iter().find(|(n, _)| *n == key).cloned() else {
                return Ok(mlua::Value::Nil);
            };
            let f = lua.create_function(move |lua, args: mlua::MultiValue| {
                let mut args: Vec<mlua::Value> = args.into_iter().collect();
                if matches!(args.first(), Some(mlua::Value::UserData(ud)) if ud.is::<AnimatableUd>()) {
                    args.remove(0);
                }
                let ctx = ctx(lua)?;
                let targets = targets(&ctx, a.targets.clone()).map_err(rt)?;
                let value = args.first().cloned().unwrap_or(mlua::Value::Nil);
                if value.is_nil() {
                    let Some(target) = targets.first() else { return Ok(mlua::Value::Nil) };
                    let key = target.key_for(&name).map_err(rt)?;
                    return display(lua, &key, &target.read(&key));
                }
                let params = lua.create_table()?;
                for pair in a.defaults.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = pair?;
                    params.raw_set(k, v)?;
                }
                for pair in config.pairs::<mlua::Value, mlua::Value>() {
                    let (k, v) = pair?;
                    params.raw_set(k, v)?;
                }
                if let Some(d) = args.get(1).filter(|v| !v.is_nil()) {
                    params.raw_set("duration", d.clone())?;
                }
                if let Some(e) = args.get(2).filter(|v| !v.is_nil()) {
                    params.raw_set("ease", e.clone())?;
                }
                params.raw_set(name.as_str(), value)?;
                let clip = animate(lua, a.targets.clone(), params)?;
                let mut playing = a.playing.borrow_mut();
                playing.retain(|c| c.is_playing());
                playing.push(clip.0);
                Ok(mlua::Value::Nil)
            })?;
            Ok(mlua::Value::Function(f))
        });
    }
}

/// A canonical value as scripts read it: angles in degrees.
fn display(lua: &Lua, key: &Key, v: &V) -> mlua::Result<mlua::Value> {
    Ok(match v {
        V::Num(n) if key.unit() == Unit::Angle => mlua::Value::Number(n.to_degrees()),
        V::Num(n) => mlua::Value::Number(*n),
        V::Color(c) => mlua::Value::String(lua.create_string(value::format_color(*c))?),
        V::Complex { pieces, numbers } => {
            mlua::Value::String(lua.create_string(value::compose(pieces, numbers))?)
        }
        V::Discrete(value) => lua.pack(value.clone())?,
    })
}

fn stagger(lua: &Lua, (v, params): (mlua::Value, Option<Table>)) -> mlua::Result<StaggerUd> {
    let parse = |v: &mlua::Value| -> mlua::Result<(f64, Option<Rc<str>>)> {
        Ok(match v {
            mlua::Value::Integer(n) => (*n as f64, None),
            mlua::Value::Number(n) => (*n, None),
            mlua::Value::String(s) => match value::parse_str(&s.to_string_lossy()).kind {
                value::RawKind::Num(n, unit) => (n, unit),
                _ => {
                    return Err(mlua::Error::runtime(
                        "stagger takes a number, a number with a unit, or a range",
                    ));
                }
            },
            other => {
                return Err(mlua::Error::runtime(format!(
                    "stagger cannot take {}",
                    other.type_name()
                )));
            }
        })
    };
    let (value, range_end, unit) = match &v {
        mlua::Value::Table(t) => {
            let (a, ua) = parse(&t.raw_get(1)?)?;
            let (b, ub) = parse(&t.raw_get(2)?)?;
            (a, Some(b), ub.or(ua))
        }
        other => {
            let (a, u) = parse(other)?;
            (a, None, u)
        }
    };
    let params = match params {
        Some(p) => p,
        None => lua.create_table()?,
    };
    let ctx = ctx(lua)?;
    let from = match params.raw_get::<mlua::Value>("from")? {
        mlua::Value::Nil => From::Index(0.),
        mlua::Value::String(s) => match &*s.to_string_lossy() {
            "first" => From::Index(0.),
            "center" => From::Center,
            "last" => From::Last,
            "random" => From::Random,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "unknown stagger from `{other}`"
                )));
            }
        },
        v => From::Index(lua.unpack::<f64>(v)?),
    };
    let grid = match params.raw_get::<Option<Table>>("grid")? {
        Some(g) => Some((g.raw_get::<usize>(1)?, g.raw_get::<usize>(2)?)),
        None => None,
    };
    let axis = params
        .raw_get::<Option<String>>("axis")?
        .and_then(|a| a.chars().next());
    let index = match params.raw_get::<mlua::Value>("use")? {
        mlua::Value::Function(f) => Some(f),
        _ => None,
    };
    Ok(StaggerUd(Rc::new(Stagger::new(
        value,
        range_end,
        unit,
        params.raw_get::<Option<f64>>("start")?,
        from,
        params.raw_get::<Option<bool>>("reversed")?.unwrap_or(false),
        ctx.ease(&params.raw_get("ease")?).map_err(rt)?,
        grid,
        axis,
        params.raw_get::<Option<usize>>("total")?,
        index,
        params.raw_get::<Option<mlua::Function>>("modifier")?,
    ))))
}

fn spring(params: Option<Table>, perceptual: bool) -> mlua::Result<EaseUd> {
    let get = |k: &str, d: f64| -> mlua::Result<f64> {
        Ok(params
            .as_ref()
            .map(|p| p.raw_get::<Option<f64>>(k))
            .transpose()?
            .flatten()
            .unwrap_or(d))
    };
    let spring = if perceptual {
        Spring::perceptual(get("bounce", 0.5)?, get("duration", 628.)? / 1000.)
    } else {
        Spring::new(
            get("mass", 1.)?,
            get("stiffness", 100.)?,
            get("damping", 10.)?,
            get("velocity", 0.)?,
        )
    };
    Ok(EaseUd(Ease::Spring(Rc::new(spring))))
}

/// `utils.set(targets, props)`: applies values now.
fn utils_set(lua: &Lua, (targets_value, props): (mlua::Value, Table)) -> mlua::Result<()> {
    let ctx = ctx(lua)?;
    let list = targets(&ctx, targets_value).map_err(rt)?;
    let p = Params {
        own: &props,
        defaults: None,
    };
    let zero = lua.create_table()?;
    for pair in props.pairs::<mlua::Value, mlua::Value>() {
        let (k, v) = pair?;
        zero.raw_set(k, v)?;
    }
    zero.raw_set("duration", 0)?;
    zero.raw_set("ease", "linear")?;
    let _ = p;
    let compiled = build::compile(
        &ctx,
        &list,
        &Params {
            own: &zero,
            defaults: None,
        },
        &|_, _, _| None,
    )
    .map_err(rt)?;
    let mut writes = Vec::new();
    for track in compiled.tracks {
        // A set takes the property from any animation holding it.
        if let Target::Element(e) = &track.target {
            e.owners.borrow_mut().retain(|(k, owner)| {
                let taken = *k == track.key;
                if taken {
                    owner.track.set(true);
                }
                !taken
            });
        }
        if let Some(last) = track.segs.last() {
            writes.push((
                track.target.clone(),
                track.key.clone(),
                last.to.clone(),
                track.unit.clone(),
            ));
        }
    }
    ctx.engine.set_now(writes);
    Ok(())
}

fn utils_get(lua: &Lua, (targets_value, name): (mlua::Value, String)) -> mlua::Result<mlua::Value> {
    let ctx = ctx(lua)?;
    let list = targets(&ctx, targets_value).map_err(rt)?;
    let Some(target) = list.first() else {
        return Ok(mlua::Value::Nil);
    };
    let key = target.key_for(&name).map_err(rt)?;
    display(lua, &key, &target.read(&key))
}

/// `utils.remove(targets)`: stops every animation of those targets.
fn utils_remove(lua: &Lua, targets_value: mlua::Value) -> mlua::Result<()> {
    let ctx = ctx(lua)?;
    let list = targets(&ctx, targets_value).map_err(rt)?;
    ctx.engine.remove_targets(&list);
    Ok(())
}

/// Installs the read-only `anime` global.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let anime = lua.create_table()?;
    anime.set(
        "animate",
        lua.create_function(|lua, (t, p): (mlua::Value, Table)| animate(lua, t, p))?,
    )?;
    anime.set(
        "createTimeline",
        lua.create_function(|lua, p: Option<Table>| timeline(lua, p))?,
    )?;
    anime.set(
        "timeline",
        lua.create_function(|lua, p: Option<Table>| timeline(lua, p))?,
    )?;
    anime.set(
        "createTimer",
        lua.create_function(|lua, p: Option<Table>| timer(lua, p))?,
    )?;
    anime.set(
        "createAnimatable",
        lua.create_function(|lua, (targets, params): (mlua::Value, Table)| {
            let defaults = lua.create_table()?;
            let mut props = Vec::new();
            for pair in params.pairs::<String, mlua::Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("duration" | "ease" | "modifier" | "composition", v) => {
                        defaults.raw_set(k, v)?
                    }
                    (_, mlua::Value::Table(config)) => props.push((k, config)),
                    (_, v) => {
                        // A bare number is the property's duration.
                        let config = lua.create_table()?;
                        config.raw_set("duration", v)?;
                        props.push((k, config));
                    }
                }
            }
            Ok(AnimatableUd(Rc::new(Animatable {
                targets,
                props,
                defaults,
                playing: RefCell::default(),
            })))
        })?,
    )?;
    anime.set("stagger", lua.create_function(stagger)?)?;
    anime.set(
        "createDraggable",
        lua.create_function(|lua, (target, params): (mlua::Value, Option<Table>)| {
            super::draggable::create(lua, target, params)
        })?,
    )?;
    anime.set(
        "onScroll",
        lua.create_function(|lua, params: Option<Table>| super::scroll::on_scroll(lua, params))?,
    )?;
    anime.set(
        "createSpring",
        lua.create_function(|_, p: Option<Table>| spring(p, false))?,
    )?;
    anime.set(
        "spring",
        lua.create_function(|_, p: Option<Table>| spring(p, true))?,
    )?;

    // `eases.inOut(3)`, `eases.outElastic(1, .4)`, `eases.steps(5)`…
    let eases = lua.create_table()?;
    let names = [
        "linear",
        "none",
        "in",
        "out",
        "inOut",
        "outIn",
        "inQuad",
        "outQuad",
        "inOutQuad",
        "outInQuad",
        "inCubic",
        "outCubic",
        "inOutCubic",
        "outInCubic",
        "inQuart",
        "outQuart",
        "inOutQuart",
        "outInQuart",
        "inQuint",
        "outQuint",
        "inOutQuint",
        "outInQuint",
        "inSine",
        "outSine",
        "inOutSine",
        "outInSine",
        "inCirc",
        "outCirc",
        "inOutCirc",
        "outInCirc",
        "inExpo",
        "outExpo",
        "inOutExpo",
        "outInExpo",
        "inBounce",
        "outBounce",
        "inOutBounce",
        "outInBounce",
        "inBack",
        "outBack",
        "inOutBack",
        "outInBack",
        "inElastic",
        "outElastic",
        "inOutElastic",
        "outInElastic",
        "steps",
        "cubicBezier",
        "irregular",
    ];
    for name in names {
        eases.set(
            name,
            lua.create_function(move |_, args: mlua::Variadic<f64>| {
                let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                let text = if args.is_empty() {
                    name.to_owned()
                } else {
                    format!("{name}({})", args.join(","))
                };
                Ease::parse(&text).map(EaseUd).map_err(mlua::Error::runtime)
            })?,
        )?;
    }
    anime.set("eases", eases)?;

    let utils = lua.create_table()?;
    utils.set("set", lua.create_function(utils_set)?)?;
    utils.set("get", lua.create_function(utils_get)?)?;
    utils.set("remove", lua.create_function(utils_remove)?)?;
    utils.set(
        "cleanInlineStyles",
        lua.create_function(|lua, (v, props): (mlua::Value, Option<mlua::Value>)| {
            // An animation's targets, or targets directly; with `props` (a
            // name or a list of names), only those properties.
            let keys: Option<Vec<Key>> = match props {
                None | Some(mlua::Value::Nil) => None,
                Some(mlua::Value::String(s)) => Some(vec![element_key(&s.to_string_lossy())?]),
                Some(mlua::Value::Table(t)) => Some(
                    t.sequence_values::<String>()
                        .map(|n| element_key(&n?))
                        .collect::<mlua::Result<_>>()?,
                ),
                Some(other) => {
                    return Err(mlua::Error::runtime(format!(
                        "cleanInlineStyles' properties are a name or a list, not {}",
                        other.type_name()
                    )));
                }
            };
            let ctx = ctx(lua)?;
            let list = match &v {
                mlua::Value::UserData(ud) if ud.is::<ClipUd>() => {
                    let clip = ud.borrow::<ClipUd>()?.0.clone();
                    clip.ensure_built();
                    clip.tracks
                        .borrow()
                        .iter()
                        .map(|t| t.target.clone())
                        .collect()
                }
                _ => targets(&ctx, v).map_err(rt)?,
            };
            for target in list {
                if let Target::Element(e) = target {
                    e.clear_only(keys.as_deref());
                }
            }
            Ok(())
        })?,
    )?;
    install_math(lua, &utils)?;
    anime.set("utils", utils)?;

    anime.set("engine", lua.create_userdata(EngineUd)?)?;

    let svg = lua.create_table()?;
    svg.set(
        "createMotionPath",
        lua.create_function(|lua, d: String| {
            let sampler = Rc::new(super::svg::Sampler::new(
                super::svg::parse(&d).map_err(mlua::Error::runtime)?,
            ));
            let t = lua.create_table()?;
            use super::svg::Axis;
            t.set("translateX", PathValueUd(sampler.clone(), Axis::X))?;
            t.set("translateY", PathValueUd(sampler.clone(), Axis::Y))?;
            t.set("rotate", PathValueUd(sampler.clone(), Axis::Angle))?;
            t.set("x", PathValueUd(sampler.clone(), Axis::X))?;
            t.set("y", PathValueUd(sampler.clone(), Axis::Y))?;
            t.set("length", sampler.length())?;
            Ok(t)
        })?,
    )?;
    svg.set(
        "getTotalLength",
        lua.create_function(|_, d: String| {
            Ok(
                super::svg::Sampler::new(super::svg::parse(&d).map_err(mlua::Error::runtime)?)
                    .length(),
            )
        })?,
    )?;
    anime.set("svg", svg)?;

    let text = lua.create_table()?;
    text.set(
        "split",
        lua.load(TEXT_SPLIT)
            .set_name("=anime.text.split")
            .eval::<mlua::Function>()?,
    )?;
    anime.set("text", text)?;
    anime.set(
        "createScope",
        lua.create_function(|lua, params: Option<Table>| scope(lua, params))?,
    )?;
    // anime.js v3: `anime({ targets = …, … })`.
    let meta = lua.create_table()?;
    meta.set(
        "__call",
        lua.create_function(|lua, (_, params): (mlua::Value, Table)| {
            let targets: mlua::Value = params.raw_get("targets")?;
            animate(lua, targets, params)
        })?,
    )?;
    anime.set_metatable(Some(meta))?;
    anime.set_readonly(true);
    lua.globals().set("anime", anime)
}

/// anime.js math utilities. Called without the value, each returns a
/// function of it (`utils.round(2)`), usable as a `modifier`.
fn install_math(lua: &Lua, utils: &Table) -> mlua::Result<()> {
    let seed = std::cell::Cell::new(0x2545_f491_4f6c_dd1d_u64);
    let random = Rc::new(move || {
        let mut s = seed.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        seed.set(s);
        (s >> 11) as f64 / (1u64 << 53) as f64
    });
    let r = random.clone();
    utils.set(
        "random",
        lua.create_function(
            move |_, (min, max, decimals): (Option<f64>, Option<f64>, Option<f64>)| {
                let (min, max) = (min.unwrap_or(0.), max.unwrap_or(1.));
                let v = min + r() * (max - min);
                Ok(match decimals {
                    Some(d) => {
                        let s = 10f64.powf(d);
                        (v * s).round() / s
                    }
                    None if min.fract() == 0. && max.fract() == 0. && max - min >= 1. => {
                        (min + r() * (max - min + 1.)).floor().min(max)
                    }
                    None => v,
                })
            },
        )?,
    )?;
    let r = random.clone();
    utils.set(
        "randomPick",
        lua.create_function(move |_, items: Table| {
            let n = items.raw_len();
            if n == 0 {
                return Ok(mlua::Value::Nil);
            }
            items.raw_get::<mlua::Value>(1 + (r() * n as f64) as usize % n)
        })?,
    )?;
    let r = random;
    utils.set(
        "shuffle",
        lua.create_function(move |lua, items: Table| {
            let mut values: Vec<mlua::Value> =
                items.sequence_values().collect::<mlua::Result<_>>()?;
            for i in (1..values.len()).rev() {
                values.swap(i, (r() * (i + 1) as f64) as usize % (i + 1));
            }
            lua.create_sequence_from(values)
        })?,
    )?;
    utils.set(
        "round",
        lua.create_function(
            |lua, (a, b): (f64, Option<f64>)| -> mlua::Result<mlua::Value> {
                match b {
                    Some(decimals) => {
                        let s = 10f64.powf(decimals);
                        Ok(mlua::Value::Number((a * s).round() / s))
                    }
                    None => lua
                        .create_userdata(ModifierUd(Modifier::Round(a)))
                        .map(mlua::Value::UserData),
                }
            },
        )?,
    )?;
    utils.set(
        "clamp",
        lua.create_function(|_, (v, min, max): (f64, f64, f64)| {
            Ok(v.clamp(min.min(max), max.max(min)))
        })?,
    )?;
    utils.set(
        "snap",
        lua.create_function(|_, (v, step): (f64, f64)| {
            Ok(if step > 0. {
                (v / step).round() * step
            } else {
                v
            })
        })?,
    )?;
    utils.set(
        "wrap",
        lua.create_function(|_, (v, min, max): (f64, f64, f64)| {
            let range = max - min;
            Ok(if range == 0. {
                min
            } else {
                min + (v - min).rem_euclid(range)
            })
        })?,
    )?;
    utils.set(
        "mapRange",
        lua.create_function(|_, (v, a, b, c, d): (f64, f64, f64, f64, f64)| {
            Ok(if b == a {
                c
            } else {
                c + (v - a) * (d - c) / (b - a)
            })
        })?,
    )?;
    utils.set(
        "lerp",
        lua.create_function(|_, (a, b, t): (f64, f64, f64)| Ok(a + (b - a) * t))?,
    )?;
    utils.set(
        "damp",
        lua.create_function(|_, (a, b, dt, factor): (f64, f64, f64, f64)| {
            Ok(a + (b - a) * (1. - (-factor * dt / 1000.).exp()))
        })?,
    )?;
    utils.set(
        "degToRad",
        lua.create_function(|_, d: f64| Ok(d.to_radians()))?,
    )?;
    utils.set(
        "radToDeg",
        lua.create_function(|_, r: f64| Ok(r.to_degrees()))?,
    )?;
    utils.set(
        "roundPad",
        lua.create_function(|_, (v, decimals): (f64, usize)| Ok(format!("{v:.decimals$}")))?,
    )?;
    utils.set(
        "padStart",
        lua.create_function(
            |_, (v, width, fill): (mlua::Value, usize, Option<String>)| {
                let s = match v {
                    mlua::Value::String(s) => s.to_string_lossy(),
                    other => value::format_number(match other {
                        mlua::Value::Integer(n) => n as f64,
                        mlua::Value::Number(n) => n,
                        _ => 0.,
                    }),
                };
                let fill = fill.unwrap_or_else(|| " ".into());
                let pad: String = fill
                    .chars()
                    .cycle()
                    .take(width.saturating_sub(s.chars().count()))
                    .collect();
                Ok(pad + &s)
            },
        )?,
    )?;
    utils.set(
        "padEnd",
        lua.create_function(
            |_, (v, width, fill): (mlua::Value, usize, Option<String>)| {
                let s = match v {
                    mlua::Value::String(s) => s.to_string_lossy(),
                    other => value::format_number(match other {
                        mlua::Value::Integer(n) => n as f64,
                        mlua::Value::Number(n) => n,
                        _ => 0.,
                    }),
                };
                let fill = fill.unwrap_or_else(|| " ".into());
                let pad: String = fill
                    .chars()
                    .cycle()
                    .take(width.saturating_sub(s.chars().count()))
                    .collect();
                Ok(s + &pad)
            },
        )?,
    )?;
    utils.set(
        "createSeededRandom",
        lua.create_function(|lua, seed: Option<f64>| {
            let state = std::cell::Cell::new(seed.map_or(0x9e37_79b9_7f4a_7c15, |s| {
                (s as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1
            }));
            lua.create_function(
                move |_, (min, max, decimals): (Option<f64>, Option<f64>, Option<f64>)| {
                    let mut s = state.get();
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    state.set(s);
                    let u = (s >> 11) as f64 / (1u64 << 53) as f64;
                    let (min, max) = (min.unwrap_or(0.), max.unwrap_or(1.));
                    let v = min + u * (max - min);
                    Ok(match decimals {
                        Some(d) => {
                            let k = 10f64.powf(d);
                            (v * k).round() / k
                        }
                        None => v,
                    })
                },
            )
        })?,
    )?;
    Ok(())
}

/// `anime.engine`: global playback settings.
struct EngineUd;
impl UserData for EngineUd {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        let engine = |lua: &Lua| -> mlua::Result<Rc<Engine>> { Ok(env(lua)?.engine.clone()) };
        fields.add_field_function_get("speed", move |lua, _| Ok(engine(lua)?.speed.get()));
        fields.add_field_function_set("speed", move |lua, _, v: f64| {
            engine(lua)?.speed.set(v.max(0.));
            Ok(())
        });
        fields.add_field_function_get("playbackRate", move |lua, _| Ok(engine(lua)?.speed.get()));
        fields.add_field_function_set("playbackRate", move |lua, _, v: f64| {
            engine(lua)?.speed.set(v.max(0.));
            Ok(())
        });
        fields.add_field_function_get("fps", move |lua, _| Ok(engine(lua)?.fps.get()));
        fields.add_field_function_set("fps", move |lua, _, v: Option<f64>| {
            engine(lua)?.fps.set(v.filter(|v| *v > 0.));
            Ok(())
        });
        fields.add_field_function_get("timeUnit", move |lua, _| {
            Ok(if engine(lua)?.seconds.get() {
                "s"
            } else {
                "ms"
            })
        });
        fields.add_field_function_set("timeUnit", move |lua, _, v: String| {
            engine(lua)?.seconds.set(v == "s");
            Ok(())
        });
        fields.add_field_function_get("currentTime", move |lua, _| {
            let e = engine(lua)?;
            Ok(e.script_time(e.now.get()))
        });
        fields.add_field_function_get("paused", move |lua, _| Ok(engine(lua)?.paused.get()));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("pause", |lua, _: mlua::MultiValue| {
            env(lua)?.engine.paused.set(true);
            Ok(())
        });
        methods.add_function("resume", |lua, _: mlua::MultiValue| {
            let engine = env(lua)?.engine.clone();
            engine.paused.set(false);
            engine.wake();
            Ok(())
        });
    }
}

impl Engine {
    /// Drops every track of these targets from every clip.
    pub(crate) fn remove_targets(&self, targets: &[Target]) {
        for clip in self.playing_clips() {
            let mut tracks = clip.tracks.borrow_mut();
            let had = tracks.len();
            tracks.retain(|t| !targets.iter().any(|g| g.same(&t.target)));
            // A clip left with nothing to animate ends; one that never had
            // a track (a timer) is not about these targets and plays on.
            let emptied = had > 0 && tracks.is_empty();
            drop(tracks);
            if emptied && !clip.is_timeline.get() {
                clip.cancel();
            }
        }
        for target in targets {
            if let Target::Element(e) = target {
                for (_, owner) in e.owners.borrow_mut().drain(..) {
                    owner.track.set(true);
                }
            }
        }
    }
}

/// An element property name, as anime.js and this library spell them.
fn element_key(name: &str) -> mlua::Result<Key> {
    Key::for_element(name)
        .ok_or_else(|| mlua::Error::runtime(format!("`{name}` is not an animatable property")))
}

/// `anime.text.split(text, params)`: a row of text nodes, one per
/// character or word, each with a class to animate by selector. Spaces
/// stay their own pieces so the line keeps its spacing.
const TEXT_SPLIT: &str = r#"
return function(text, params)
    params = params or {}
    local words = params.words
    local chars = params.chars or not words
    local class = (type(chars) == "table" and chars.class) or (type(words) == "table" and words.class)
        or params.class or (words and "word" or "char")
    local style = params.style or {}
    local row = { flex_wrap = true, items = "baseline" }
    for key, value in pairs(params.props or {}) do
        row[key] = value
    end
    local function piece(s)
        local props = { s, class = class }
        if s == " " then
            props.class = nil
        end
        for key, value in pairs(style) do
            props[key] = value
        end
        table.insert(row, ui.text(props))
    end
    if words then
        for space, word in string.gmatch(text, "(%s*)(%S+)") do
            if space ~= "" then
                piece(" ")
            end
            piece(word)
        end
    else
        for _, code in utf8.codes(text) do
            piece(utf8.char(code))
        end
    end
    return ui.row(row)
end
"#;

/// `anime.createScope({ defaults })`: animations created in `scope:add(fn)`
/// take its defaults; `scope:revert()` reverts them all.
pub(crate) struct Scope {
    defaults: Option<Table>,
    clips: RefCell<Vec<Rc<Clip>>>,
}
#[derive(Clone)]
struct ScopeUd(Rc<Scope>);
impl UserData for ScopeUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let scope = this.0.clone();
            let f = match key.as_str() {
                "add" => lua.create_function(move |lua, args: mlua::MultiValue| {
                    let f = args.into_iter().find_map(|v| match v {
                        mlua::Value::Function(f) => Some(f),
                        _ => None,
                    });
                    let Some(f) = f else {
                        return Err(mlua::Error::runtime("scope:add takes a function"));
                    };
                    let engine = env(lua)?.engine.clone();
                    engine
                        .scopes
                        .borrow_mut()
                        .push((scope.defaults.clone(), Rc::downgrade(&scope)));
                    let result = f.call::<()>(ScopeUd(scope.clone()));
                    engine.scopes.borrow_mut().pop();
                    result?;
                    Ok(ScopeUd(scope.clone()))
                })?,
                "revert" => lua.create_function(move |_, _: mlua::MultiValue| {
                    for clip in scope.clips.borrow_mut().drain(..) {
                        clip.revert();
                    }
                    Ok(())
                })?,
                _ => return Ok(mlua::Value::Nil),
            };
            Ok(mlua::Value::Function(f))
        });
    }
}
fn scope(_: &Lua, params: Option<Table>) -> mlua::Result<ScopeUd> {
    let defaults = match &params {
        Some(p) => p.raw_get::<Option<Table>>("defaults")?,
        None => None,
    };
    Ok(ScopeUd(Rc::new(Scope {
        defaults,
        clips: RefCell::default(),
    })))
}
/// Gives an animation the active scope's defaults and records it there.
pub(crate) fn scoped(engine: &Engine, params: &Table, clip: Option<&Rc<Clip>>) -> mlua::Result<()> {
    let scopes = engine.scopes.borrow();
    let Some((defaults, scope)) = scopes.last() else {
        return Ok(());
    };
    if clip.is_none()
        && let Some(defaults) = defaults
    {
        for pair in defaults.pairs::<mlua::Value, mlua::Value>() {
            let (k, v) = pair?;
            if params.raw_get::<mlua::Value>(k.clone())?.is_nil() {
                params.raw_set(k, v)?;
            }
        }
    }
    if let (Some(clip), Some(scope)) = (clip, scope.upgrade()) {
        scope.clips.borrow_mut().push(clip.clone());
    }
    Ok(())
}
