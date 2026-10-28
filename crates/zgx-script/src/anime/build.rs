//! Turns `animate` and timeline parameters into tracks, resolving them as
//! anime.js does: per-target delays and durations (functions, `stagger`),
//! keyframes in all four forms, explicit and current `from` values,
//! relative values, units and spring durations.
use super::{
    api::{EaseUd, ModifierUd, StaggerUd},
    ease::Ease,
    engine::{Callbacks, Composition, Engine, Mark, Modifier, Seg, Span, Timing, Track},
    target::{Key, Target, Unit},
    value::{self, Op, Raw, RawKind, V},
};
use crate::{Vm, element::ElementUd, value::Value};
use mlua::{Lua, Table};
use std::rc::Rc;

pub(crate) struct Ctx {
    pub lua: Lua,
    pub vm: Rc<Vm>,
    pub engine: Rc<Engine>,
}

/// Parameters with timeline defaults behind them.
pub(crate) struct Params<'a> {
    pub own: &'a Table,
    pub defaults: Option<&'a Table>,
}
impl Params<'_> {
    /// The first of `names` set here or in the defaults (camelCase and
    /// snake_case spellings are both accepted).
    pub fn get(&self, names: &[&str]) -> mlua::Value {
        for table in [Some(self.own), self.defaults].into_iter().flatten() {
            for name in names {
                if let Ok(v) = table.raw_get::<mlua::Value>(*name)
                    && !v.is_nil()
                {
                    return v;
                }
            }
        }
        mlua::Value::Nil
    }
}

/// Parameter names that are not animated properties.
const RESERVED: &[&str] = &[
    "targets",
    "duration",
    "delay",
    "ease",
    "easing",
    "composition",
    "modifier",
    "round",
    "loop",
    "loopDelay",
    "loop_delay",
    "alternate",
    "reversed",
    "autoplay",
    "frameRate",
    "frame_rate",
    "playbackRate",
    "playback_rate",
    "playbackEase",
    "playback_ease",
    "onBegin",
    "on_begin",
    "onComplete",
    "on_complete",
    "onBeforeUpdate",
    "on_before_update",
    "onUpdate",
    "on_update",
    "onRender",
    "on_render",
    "onLoop",
    "on_loop",
    "onPause",
    "on_pause",
    "keyframes",
    "id",
    "defaults",
    "direction",
];

fn err(e: mlua::Error) -> String {
    e.to_string()
}
fn number(v: &mlua::Value) -> Option<f64> {
    match v {
        mlua::Value::Integer(n) => Some(*n as f64),
        mlua::Value::Number(n) => Some(*n),
        mlua::Value::String(s) => value::parse_str(&s.to_string_lossy()).kind_num(),
        mlua::Value::Boolean(b) => Some(*b as u8 as f64),
        _ => None,
    }
}
impl Raw {
    fn kind_num(&self) -> Option<f64> {
        match &self.kind {
            RawKind::Num(n, _) => Some(*n),
            _ => None,
        }
    }
}

impl Ctx {
    /// A target as function-based values receive it.
    fn target_arg(&self, target: &Target) -> mlua::Value {
        match target {
            Target::Element(e) => self
                .lua
                .create_userdata(ElementUd(e.clone()))
                .map(mlua::Value::UserData)
                .unwrap_or(mlua::Value::Nil),
            Target::State(s) => self
                .lua
                .create_userdata(crate::value::State::new(s.clone()))
                .map(mlua::Value::UserData)
                .unwrap_or(mlua::Value::Nil),
            Target::Table(t) => mlua::Value::Table(t.clone()),
        }
    }
    /// Resolves function and stagger values for target `i` of `n`.
    pub fn per_target(
        &self,
        v: mlua::Value,
        target: &Target,
        i: usize,
        n: usize,
    ) -> Result<mlua::Value, String> {
        match v {
            mlua::Value::Function(f) => self
                .vm
                .call::<mlua::Value>(&f, (self.target_arg(target), i + 1, n))
                .map_err(err),
            mlua::Value::UserData(ud) if ud.is::<StaggerUd>() => {
                let stagger = ud.borrow::<StaggerUd>().map_err(err)?.0.clone();
                let custom = match &stagger.index {
                    Some(f) => self
                        .vm
                        .call::<Option<f64>>(f, (self.target_arg(target), i + 1, n))
                        .map_err(err)?,
                    None => None,
                };
                let mut out = stagger.resolve(i, n, custom);
                if let Some(m) = &stagger.modifier {
                    out = self.vm.call::<f64>(m, out).map_err(err)?;
                }
                Ok(match &stagger.unit {
                    Some(unit) => mlua::Value::String(
                        self.lua
                            .create_string(format!("{}{unit}", value::format_number(out)))
                            .map_err(err)?,
                    ),
                    None => mlua::Value::Number(out),
                })
            }
            other => Ok(other),
        }
    }
    /// A time in ms (`engine.timeUnit` converts seconds).
    pub fn time(
        &self,
        v: mlua::Value,
        target: &Target,
        i: usize,
        n: usize,
        default: f64,
    ) -> Result<f64, String> {
        let v = self.per_target(v, target, i, n)?;
        Ok(match number(&v) {
            Some(t) => self.ms("a time", t)?.max(0.),
            None if v.is_nil() => default,
            None => return Err(format!("expected a time, got {}", v.type_name())),
        })
    }
    /// A time the script gave (`what`), in milliseconds: an error for one
    /// that is no number of them (NaN, infinite), which the clock would
    /// carry into every frame's arithmetic (0 × ∞ is NaN).
    pub fn ms(&self, what: &str, value: f64) -> Result<f64, String> {
        let ms = self.engine.ms(value);
        if ms.is_finite() {
            Ok(ms)
        } else {
            Err(format!("{what} takes milliseconds, not {value}"))
        }
    }
    pub fn ease(&self, v: &mlua::Value) -> Result<Option<Ease>, String> {
        Ok(match v {
            mlua::Value::Nil => None,
            mlua::Value::String(s) => Some(Ease::parse(&s.to_string_lossy())?),
            mlua::Value::Function(f) => Some(Ease::Script(f.clone())),
            mlua::Value::UserData(ud) if ud.is::<EaseUd>() => {
                Some(ud.borrow::<EaseUd>().map_err(err)?.0.clone())
            }
            other => {
                return Err(format!(
                    "an ease is a name, function or spring, not {}",
                    other.type_name()
                ));
            }
        })
    }
    fn modifier(&self, params: &Params) -> Result<Option<Modifier>, String> {
        Ok(match params.get(&["modifier"]) {
            mlua::Value::Function(f) => Some(Modifier::Script(f)),
            mlua::Value::UserData(ud) if ud.is::<ModifierUd>() => {
                Some(ud.borrow::<ModifierUd>().map_err(err)?.0.clone())
            }
            mlua::Value::Nil => match number(&params.get(&["round"])) {
                // anime.js v3: `round: 10` keeps one decimal.
                Some(r) if r > 0. => Some(Modifier::Round(r.log10().max(0.).round())),
                _ => None,
            },
            other => {
                return Err(format!(
                    "modifier must be a function, not {}",
                    other.type_name()
                ));
            }
        })
    }
    fn composition(v: &mlua::Value) -> Result<Option<Composition>, String> {
        Ok(match v {
            mlua::Value::Nil => None,
            mlua::Value::String(s) => Some(match &*s.to_string_lossy() {
                "replace" => Composition::Replace,
                "none" => Composition::None,
                "add" | "blend" => Composition::Add,
                other => return Err(format!("unknown composition `{other}`")),
            }),
            other => return Err(format!("composition is a name, not {}", other.type_name())),
        })
    }

    /// A written value in the key's canonical unit, relative to `from`.
    fn to_v(
        &self,
        v: &mlua::Value,
        key: &Key,
        target: &Target,
        from: &V,
    ) -> Result<(V, Option<Rc<str>>), String> {
        let raw = match v {
            mlua::Value::Integer(n) => Raw {
                op: Op::Set,
                kind: RawKind::Num(*n as f64, None),
            },
            mlua::Value::Number(n) => Raw {
                op: Op::Set,
                kind: RawKind::Num(*n, None),
            },
            mlua::Value::String(s) => value::parse_str(&s.to_string_lossy()),
            mlua::Value::Boolean(b) => Raw {
                op: Op::Set,
                kind: RawKind::Other(Value::Bool(*b)),
            },
            other => {
                let value: Value = self.lua.unpack(other.clone()).map_err(err)?;
                value::parse(&value)
            }
        };
        let base = from.num().unwrap_or(0.);
        let relative = |n: f64| match raw.op {
            Op::Set => n,
            Op::Add => base + n,
            Op::Sub => base - n,
            Op::Mul => base * n,
        };
        let unit_of = |u: &Option<Rc<str>>| u.clone();
        Ok(match (&raw.kind, key.unit()) {
            (RawKind::Num(n, unit), Unit::Length(basis)) => {
                let points = value::length(
                    *n,
                    unit.as_deref(),
                    if unit.as_deref() == Some("%") {
                        target.basis(basis)
                    } else {
                        0.
                    },
                );
                (V::Num(relative(points)), None)
            }
            (RawKind::Num(n, unit), Unit::Angle) => {
                (V::Num(relative(value::angle(*n, unit.as_deref()))), None)
            }
            (RawKind::Num(n, _), Unit::Factor) => (V::Num(relative(*n)), None),
            (RawKind::Num(n, _), Unit::Color) => {
                let n = *n as u32;
                (
                    V::Color([
                        (n >> 16 & 255) as f64,
                        (n >> 8 & 255) as f64,
                        (n & 255) as f64,
                        1.,
                    ]),
                    None,
                )
            }
            (RawKind::Num(n, unit), Unit::Free) => (V::Num(relative(*n)), unit_of(unit)),
            (RawKind::Color(c), _) => (V::Color(*c), None),
            (RawKind::Complex { pieces, numbers }, _) => (
                V::Complex {
                    pieces: pieces.clone(),
                    numbers: numbers.clone(),
                },
                None,
            ),
            (RawKind::Other(value), _) => (V::Discrete(value.clone()), None),
        })
    }
}

/// One keyframe of one property.
struct Kf {
    to: mlua::Value,
    from: Option<mlua::Value>,
    duration: Option<f64>,
    delay: f64,
    ease: Option<Ease>,
    composition: Option<Composition>,
    modifier: Option<Modifier>,
}
impl Kf {
    fn to(to: mlua::Value) -> Self {
        Self {
            to,
            from: None,
            duration: None,
            delay: 0.,
            ease: None,
            composition: None,
            modifier: None,
        }
    }
}

/// Reads a property's value into keyframes and whether they split the
/// duration evenly (value arrays and parameter arrays without durations).
fn keyframes(
    ctx: &Ctx,
    v: mlua::Value,
    target: &Target,
    i: usize,
    n: usize,
) -> Result<Vec<Kf>, String> {
    let v = ctx.per_target(v, target, i, n)?;
    let mlua::Value::Table(t) = &v else {
        return Ok(vec![Kf::to(v)]);
    };
    if t.raw_len() == 0 {
        // A tween parameter object: { to, from, duration, delay, ease, … }.
        if t.contains_key("to").map_err(err)? || t.contains_key("from").map_err(err)? {
            return Ok(vec![object_kf(ctx, t, target, i, n)?]);
        }
        return Ok(vec![Kf::to(v)]);
    }
    let items: Vec<mlua::Value> = t
        .sequence_values::<mlua::Value>()
        .collect::<mlua::Result<_>>()
        .map_err(err)?;
    if items
        .iter()
        .all(|item| matches!(item, mlua::Value::Table(t) if t.raw_len() == 0))
    {
        return items
            .into_iter()
            .map(|item| match item {
                mlua::Value::Table(t) => object_kf(ctx, &t, target, i, n),
                _ => unreachable!(),
            })
            .collect();
    }
    // A value array: the first value is where it starts.
    let mut items = items.into_iter();
    let first = items.next().unwrap_or(mlua::Value::Nil);
    let rest: Vec<mlua::Value> = items.collect();
    if rest.is_empty() {
        return Ok(vec![Kf::to(first)]);
    }
    let mut kfs: Vec<Kf> = rest.into_iter().map(Kf::to).collect();
    kfs[0].from = Some(first);
    Ok(kfs)
}
fn object_kf(ctx: &Ctx, t: &Table, target: &Target, i: usize, n: usize) -> Result<Kf, String> {
    let get = |k: &str| t.raw_get::<mlua::Value>(k).map_err(err);
    let to = ctx.per_target(get("to")?, target, i, n)?;
    let from = match get("from")? {
        mlua::Value::Nil => None,
        f => Some(ctx.per_target(f, target, i, n)?),
    };
    let duration = match get("duration")? {
        mlua::Value::Nil => None,
        d => Some(ctx.time(d, target, i, n, 0.)?),
    };
    let modifier = match get("modifier")? {
        mlua::Value::Function(f) => Some(Modifier::Script(f)),
        mlua::Value::UserData(ud) if ud.is::<ModifierUd>() => {
            Some(ud.borrow::<ModifierUd>().map_err(err)?.0.clone())
        }
        _ => None,
    };
    Ok(Kf {
        to,
        from,
        duration,
        delay: ctx.time(get("delay")?, target, i, n, 0.)?,
        ease: ctx.ease(&get("ease")?)?,
        composition: Ctx::composition(&get("composition")?)?,
        modifier,
    })
}

pub(crate) struct Compiled {
    pub tracks: Vec<Track>,
    pub iteration: f64,
}

/// What a property is before a tween: the timeline's own value at that
/// time if an earlier child animated it, else the target's current value.
pub(crate) type Current<'a> = &'a dyn Fn(&Target, &Key, f64) -> Option<V>;

/// Compiles one animation's properties for `targets`.
pub(crate) fn compile(
    ctx: &Ctx,
    targets: &[Target],
    params: &Params,
    current: Current,
) -> Result<Compiled, String> {
    let n = targets.len();
    let explicit_duration = !params.get(&["duration"]).is_nil();
    let base_ease = ctx
        .ease(&params.get(&["ease", "easing"]))?
        .unwrap_or(Ease::DEFAULT);
    let composition =
        Ctx::composition(&params.get(&["composition"]))?.unwrap_or(Composition::Replace);
    let modifier = ctx.modifier(params)?;
    let mut tracks: Vec<Track> = Vec::new();
    let names: Vec<(String, mlua::Value)> = params
        .own
        .pairs::<mlua::Value, mlua::Value>()
        .filter_map(|pair| match pair {
            Ok((mlua::Value::String(k), v)) => Some((k.to_string_lossy(), v)),
            _ => None,
        })
        .filter(|(k, _)| !RESERVED.contains(&k.as_str()))
        .collect();
    for (i, target) in targets.iter().enumerate() {
        let delay = ctx.time(params.get(&["delay"]), target, i, n, 0.)?;
        let spring_duration = base_ease
            .spring_duration_ms()
            .filter(|_| !explicit_duration);
        let duration = ctx.time(
            params.get(&["duration"]),
            target,
            i,
            n,
            spring_duration.unwrap_or(1000.),
        )?;
        let mut add = |key: Key,
                       kfs: Vec<Kf>,
                       share: bool,
                       mut path: Option<(Rc<super::svg::Sampler>, super::svg::Axis)>|
         -> Result<(), String> {
            let segments = kfs.len().max(1) as f64;
            let mut cursor = delay;
            let mut previous: Option<V> = None;
            let mut segs = Vec::new();
            if path.is_some() {
                // Along a path, the tween runs progress 0 to 1.
                for kf in kfs {
                    let ease = kf.ease.clone().unwrap_or_else(|| base_ease.clone());
                    let start = cursor + kf.delay;
                    segs.push(Seg {
                        start,
                        end: start + kf.duration.unwrap_or(duration),
                        from: V::Num(0.),
                        to: V::Num(1.),
                        ease,
                    });
                }
                tracks.push(Track {
                    target: target.clone(),
                    key,
                    segs,
                    composition,
                    modifier: modifier.clone(),
                    unit: None,
                    original: None,
                    overridden: Rc::default(),
                    path: path.take(),
                    implicit_from: false,
                    deferred: Default::default(),
                });
                return Ok(());
            }
            let mut unit = None;
            let mut kf_composition = None;
            let mut kf_modifier = None;
            let implicit_from = kfs.first().is_some_and(|kf| kf.from.is_none());
            for kf in kfs {
                let ease = kf.ease.clone().unwrap_or_else(|| base_ease.clone());
                let length = kf.duration.unwrap_or_else(|| {
                    match ease.spring_duration_ms().filter(|_| !explicit_duration) {
                        Some(spring) => spring,
                        None if share => duration / segments,
                        None => duration,
                    }
                });
                let start = cursor + kf.delay;
                let before = match &previous {
                    Some(p) => p.clone(),
                    None => current(target, &key, start).unwrap_or_else(|| target.read(&key)),
                };
                let from = match &kf.from {
                    Some(f) => ctx.to_v(f, &key, target, &before)?.0,
                    None => before,
                };
                let (to, to_unit) = if kf.to.is_nil() {
                    (from.clone(), None)
                } else {
                    ctx.to_v(&kf.to, &key, target, &from)?
                };
                unit = unit.or(to_unit);
                kf_composition = kf_composition.or(kf.composition);
                kf_modifier = kf_modifier.or(kf.modifier);
                segs.push(Seg {
                    start,
                    end: start + length,
                    from,
                    to: to.clone(),
                    ease,
                });
                previous = Some(to);
                cursor = start + length;
            }
            tracks.push(Track {
                target: target.clone(),
                key,
                segs,
                composition: kf_composition.unwrap_or(composition),
                modifier: kf_modifier.or_else(|| modifier.clone()),
                unit,
                original: None,
                overridden: Rc::default(),
                path: path.take(),
                implicit_from,
                deferred: Default::default(),
            });
            Ok(())
        };
        for (name, v) in &names {
            let key = target.key_for(name)?;
            if let mlua::Value::UserData(ud) = v
                && let Ok(along) = ud.borrow::<super::api::PathValueUd>()
            {
                add(
                    key,
                    vec![Kf::to(mlua::Value::Nil)],
                    false,
                    Some((along.0.clone(), along.1)),
                )?;
                continue;
            }
            let kfs = keyframes(ctx, v.clone(), target, i, n)?;
            let share = kfs.len() > 1;
            add(key, kfs, share, None)?;
        }
        // `keyframes`: a list of property sets, or percentages of the duration.
        if let mlua::Value::Table(list) = params.get(&["keyframes"]) {
            for (key, kfs) in keyframe_param(ctx, &list, target, i, n, duration)? {
                add(key, kfs, false, None)?;
            }
        }
    }
    let iteration = tracks.iter().map(Track::end).fold(0., f64::max);
    let iteration = if tracks.is_empty() {
        let fallback = base_ease
            .spring_duration_ms()
            .filter(|_| !explicit_duration)
            .unwrap_or(1000.);
        match number(&params.get(&["duration"])) {
            Some(d) => ctx.ms("duration", d)?.max(0.),
            None => fallback,
        }
    } else {
        iteration
    };
    Ok(Compiled { tracks, iteration })
}

/// The `keyframes` parameter, per property: a list advances one cursor
/// through every property; percentages place each at a share of `duration`.
fn keyframe_param(
    ctx: &Ctx,
    list: &Table,
    target: &Target,
    i: usize,
    n: usize,
    duration: f64,
) -> Result<Vec<(Key, Vec<Kf>)>, String> {
    let mut per_key: Vec<(Key, Vec<Kf>, f64)> = Vec::new();
    let mut push = |key: Key, kf: Kf, at: f64| match per_key.iter_mut().find(|(k, _, _)| *k == key)
    {
        Some((_, kfs, end)) => {
            let mut kf = kf;
            kf.delay += at - *end;
            *end = at + kf.duration.unwrap_or(0.);
            kfs.push(kf);
        }
        None => {
            let mut kf = kf;
            let end = at + kf.duration.unwrap_or(0.);
            kf.delay += at;
            per_key.push((key, vec![kf], end));
        }
    };
    let props = |t: &Table| -> Result<Vec<(String, mlua::Value)>, String> {
        t.pairs::<String, mlua::Value>()
            .filter(|p| {
                p.as_ref().map_or(true, |(k, _)| {
                    !RESERVED.contains(&k.as_str()) || k == "ease"
                })
            })
            .collect::<mlua::Result<Vec<_>>>()
            .map_err(err)
            .map(|v| v.into_iter().filter(|(k, _)| k != "ease").collect())
    };
    if list.raw_len() > 0 {
        let frames: Vec<Table> = list
            .sequence_values::<Table>()
            .collect::<mlua::Result<_>>()
            .map_err(err)?;
        let share = duration / frames.len().max(1) as f64;
        let mut cursor = 0.;
        for frame in &frames {
            let length = ctx.time(frame.raw_get("duration").map_err(err)?, target, i, n, share)?;
            let delay = ctx.time(frame.raw_get("delay").map_err(err)?, target, i, n, 0.)?;
            let ease = ctx.ease(&frame.raw_get("ease").map_err(err)?)?;
            let at = cursor + delay;
            for (name, v) in props(frame)? {
                let key = target.key_for(&name)?;
                let mut kf = Kf::to(ctx.per_target(v, target, i, n)?);
                kf.duration = Some(length);
                kf.ease = ease.clone();
                push(key, kf, at);
            }
            cursor = at + length;
        }
    } else {
        let mut stops: Vec<(f64, Table)> = list
            .pairs::<String, Table>()
            .map(|p| {
                let (k, t) = p.map_err(err)?;
                let pct: f64 = k
                    .trim()
                    .trim_end_matches('%')
                    .parse()
                    .map_err(|_| format!("bad keyframe `{k}`"))?;
                Ok((pct / 100., t))
            })
            .collect::<Result<_, String>>()?;
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut last_at: Vec<(Key, f64)> = Vec::new();
        for (pct, frame) in &stops {
            let at = pct * duration;
            let ease = ctx.ease(&frame.raw_get("ease").map_err(err)?)?;
            for (name, v) in props(frame)? {
                let key = target.key_for(&name)?;
                let previous = last_at.iter().find(|(k, _)| *k == key).map(|(_, t)| *t);
                let mut kf = Kf::to(ctx.per_target(v, target, i, n)?);
                match previous {
                    Some(start) => {
                        kf.duration = Some(at - start);
                        kf.ease = ease.clone();
                        push(key.clone(), kf, start);
                    }
                    // The first stop is where the property starts.
                    None if at == 0. => {
                        kf.from = Some(kf.to.clone());
                        kf.duration = Some(0.);
                        push(key.clone(), kf, 0.);
                    }
                    None => {
                        kf.duration = Some(at);
                        kf.ease = ease.clone();
                        push(key.clone(), kf, 0.);
                    }
                }
                match last_at.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, t)) => *t = at,
                    None => last_at.push((key, at)),
                }
            }
        }
    }
    Ok(per_key.into_iter().map(|(k, kfs, _)| (k, kfs)).collect())
}

/// Clip timing from playback parameters.
pub(crate) fn timing(ctx: &Ctx, params: &Params) -> Result<Timing, String> {
    let loops = match params.get(&["loop"]) {
        mlua::Value::Boolean(true) => f64::INFINITY,
        mlua::Value::Nil | mlua::Value::Boolean(false) => 0.,
        v => number(&v)
            .filter(|count| !count.is_nan())
            .ok_or("loop is a count or true")?
            .max(0.),
    };
    // A rate or frame rate that is no number would carry NaN into the clock.
    let finite = |names: &[&str], what: &str| -> Result<Option<f64>, String> {
        match number(&params.get(names)) {
            Some(v) if !v.is_finite() => Err(format!("{what} is a number, not {v}")),
            v => Ok(v),
        }
    };
    let flag = |names: &[&str]| matches!(params.get(names), mlua::Value::Boolean(true));
    let direction = match params.get(&["direction"]) {
        mlua::Value::String(s) => s.to_string_lossy(),
        _ => String::new(),
    };
    Ok(Timing {
        delay: 0.,
        iteration: 0.,
        loops,
        loop_delay: match number(&params.get(&["loopDelay", "loop_delay"])) {
            Some(v) => ctx.ms("loopDelay", v)?.max(0.),
            None => 0.,
        },
        alternate: flag(&["alternate"]) || direction == "alternate",
        reversed: flag(&["reversed"]) || direction == "reverse",
        speed: finite(&["playbackRate", "playback_rate"], "playbackRate")?.unwrap_or(1.),
        playback_ease: ctx.ease(&params.get(&["playbackEase", "playback_ease"]))?,
        frame_rate: finite(&["frameRate", "frame_rate"], "frameRate")?,
        // `autoplay = onScroll(…)` hands playback to the scroll.
        autoplay: !matches!(
            params.get(&["autoplay"]),
            mlua::Value::Boolean(false) | mlua::Value::UserData(_)
        ),
    })
}

pub(crate) fn callbacks(params: &Params) -> Callbacks {
    let f = |names: &[&str]| match params.get(names) {
        mlua::Value::Function(f) => Some(f),
        _ => None,
    };
    Callbacks {
        begin: f(&["onBegin", "on_begin"]),
        before_update: f(&["onBeforeUpdate", "on_before_update"]),
        update: f(&["onUpdate", "on_update"]),
        render: f(&["onRender", "on_render"]),
        loop_: f(&["onLoop", "on_loop"]),
        pause: f(&["onPause", "on_pause"]),
        complete: f(&["onComplete", "on_complete"]),
        then: Vec::new(),
    }
}

/// Lays a child's iterations out end to end (loops, alternation and
/// reversal baked in), as a timeline plays them. Endless children play once.
pub(crate) fn unroll(tracks: Vec<Track>, timing: &Timing) -> (Vec<Track>, f64) {
    let iteration = timing.iteration;
    let iterations = if timing.loops.is_finite() {
        timing.iterations()
    } else {
        1.
    };
    let cycle = iteration + timing.loop_delay;
    let mut out = Vec::with_capacity(tracks.len());
    for mut track in tracks {
        let original = std::mem::take(&mut track.segs);
        for k in 0..iterations as usize {
            let offset = timing.delay + k as f64 * cycle;
            let backwards = (timing.alternate && k % 2 == 1) != timing.reversed;
            if backwards {
                for seg in original.iter().rev() {
                    track.segs.push(Seg {
                        start: offset + iteration - seg.end,
                        end: offset + iteration - seg.start,
                        from: seg.to.clone(),
                        to: seg.from.clone(),
                        ease: Ease::Mirror(Rc::new(seg.ease.clone())),
                    });
                }
            } else {
                for seg in &original {
                    track.segs.push(Seg {
                        start: seg.start + offset,
                        end: seg.end + offset,
                        ..seg.clone()
                    });
                }
            }
        }
        out.push(track);
    }
    let total =
        timing.delay + iterations * iteration + (iterations - 1.).max(0.) * timing.loop_delay;
    (out, total)
}

/// A timeline operation, replayed whenever the timeline builds.
pub(crate) enum TlOp {
    Add {
        targets: mlua::Value,
        params: Table,
        position: mlua::Value,
    },
    Timer {
        params: Table,
        position: mlua::Value,
    },
    Set {
        targets: mlua::Value,
        params: Table,
        position: mlua::Value,
    },
    Call {
        f: mlua::Function,
        position: mlua::Value,
    },
    Label {
        name: String,
        position: mlua::Value,
    },
    Sync {
        clip: Rc<super::engine::Clip>,
        position: mlua::Value,
    },
}

struct Cursor {
    end: f64,
    previous: (f64, f64),
}
/// anime.js timeline positions: ms, `+=`/`-=` from the end, `<` (previous
/// end), `<<` (previous start), either with `+=`/`-=`, or a label with them.
fn position(
    ctx: &Ctx,
    v: &mlua::Value,
    cursor: &Cursor,
    labels: &std::collections::HashMap<String, f64>,
) -> Result<f64, String> {
    let text = match v {
        mlua::Value::Nil => return Ok(cursor.end),
        mlua::Value::Integer(n) => return Ok(ctx.engine.ms(*n as f64)),
        mlua::Value::Number(n) => return Ok(ctx.engine.ms(*n)),
        mlua::Value::String(s) => s.to_string_lossy(),
        other => {
            return Err(format!(
                "a position is a time, label or relative string, not {}",
                other.type_name()
            ));
        }
    };
    let text = text.trim();
    let (base, rest) = if let Some(rest) = text.strip_prefix("<<") {
        (cursor.previous.0, rest)
    } else if let Some(rest) = text.strip_prefix('<') {
        (cursor.previous.1, rest)
    } else if text.starts_with("+=") || text.starts_with("-=") || text.starts_with("*=") {
        (cursor.end, text)
    } else {
        let split = text
            .find("+=")
            .or_else(|| text.find("-="))
            .unwrap_or(text.len());
        let (name, rest) = text.split_at(split);
        match labels.get(name.trim()) {
            Some(at) => (*at, rest),
            None => match name.trim().parse::<f64>() {
                Ok(n) if rest.is_empty() => return Ok(ctx.engine.ms(n)),
                _ => return Err(format!("unknown timeline position `{text}`")),
            },
        }
    };
    let offset = |s: &str| {
        s.trim()
            .parse::<f64>()
            .map(|n| ctx.engine.ms(n))
            .map_err(|_| format!("bad position offset in `{text}`"))
    };
    Ok(match rest.get(..2) {
        Some("+=") => base + offset(&rest[2..])?,
        Some("-=") => base - offset(&rest[2..])?,
        Some("*=") => {
            base * rest[2..]
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("bad position in `{text}`"))?
        }
        _ if rest.trim().is_empty() => base,
        _ => return Err(format!("bad timeline position `{text}`")),
    }
    .max(0.))
}

/// Builds a timeline: every operation in order, children's tracks merged
/// into the timeline's own at their positions.
pub(crate) fn build_timeline(
    ctx: &Ctx,
    clip: &Rc<super::engine::Clip>,
    ops: &[TlOp],
    defaults: Option<&Table>,
) -> Result<(), String> {
    let mut tracks: Vec<Track> = Vec::new();
    let mut marks: Vec<(f64, Mark)> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut labels = std::collections::HashMap::new();
    let mut cursor = Cursor {
        end: 0.,
        previous: (0., 0.),
    };
    for op in ops {
        let (start, end) = match op {
            TlOp::Add {
                targets,
                params,
                position,
            }
            | TlOp::Set {
                targets,
                params,
                position,
            } => {
                let at = position_of(ctx, position, &cursor, &labels)?;
                let set = matches!(op, TlOp::Set { .. });
                let targets = super::api::targets(ctx, targets.clone())?;
                let params_ref = Params {
                    own: params,
                    defaults: if set { None } else { defaults },
                };
                let current = |target: &Target, key: &Key, local: f64| -> Option<V> {
                    let time = at + local;
                    let track = tracks.iter().rev().find(|t| {
                        t.target.same(target)
                            && t.key == *key
                            && t.segs.first().is_some_and(|s| s.start <= time)
                    })?;
                    track.at(time, Some(&ctx.vm))
                };
                let compiled = if set {
                    let zero = ctx.lua.create_table().map_err(err)?;
                    for pair in params.pairs::<mlua::Value, mlua::Value>() {
                        let (k, v) = pair.map_err(err)?;
                        zero.raw_set(k, v).map_err(err)?;
                    }
                    zero.raw_set("duration", 0).map_err(err)?;
                    zero.raw_set("ease", "linear").map_err(err)?;
                    compile(
                        ctx,
                        &targets,
                        &Params {
                            own: &zero,
                            defaults: None,
                        },
                        &current,
                    )?
                } else {
                    compile(ctx, &targets, &params_ref, &current)?
                };
                let mut timing = timing(ctx, &params_ref)?;
                timing.iteration = compiled.iteration;
                let (child, length) = unroll(compiled.tracks, &timing);
                merge(&mut tracks, child, at);
                let callbacks = Rc::new(callbacks(&params_ref));
                if callbacks.begin.is_some() {
                    marks.push((
                        at,
                        Mark::Child(callbacks.clone(), super::engine::Event::Begin),
                    ));
                }
                if callbacks.complete.is_some() {
                    marks.push((
                        at + length,
                        Mark::Child(callbacks.clone(), super::engine::Event::Complete),
                    ));
                }
                if callbacks.update.is_some() {
                    spans.push(Span {
                        start: at,
                        end: at + length,
                        callbacks,
                    });
                }
                (at, at + length)
            }
            TlOp::Timer { params, position } => {
                let at = position_of(ctx, position, &cursor, &labels)?;
                let params_ref = Params {
                    own: params,
                    defaults,
                };
                let mut timing = timing(ctx, &params_ref)?;
                timing.iteration = match number(&params_ref.get(&["duration"])) {
                    Some(d) => ctx.ms("duration", d)?.max(0.),
                    None => 1000.,
                };
                let length = if timing.total().is_finite() {
                    timing.total()
                } else {
                    timing.iteration
                };
                let callbacks = Rc::new(callbacks(&params_ref));
                if callbacks.begin.is_some() {
                    marks.push((
                        at,
                        Mark::Child(callbacks.clone(), super::engine::Event::Begin),
                    ));
                }
                if callbacks.complete.is_some() {
                    marks.push((
                        at + length,
                        Mark::Child(callbacks.clone(), super::engine::Event::Complete),
                    ));
                }
                if callbacks.update.is_some() {
                    spans.push(Span {
                        start: at,
                        end: at + length,
                        callbacks,
                    });
                }
                (at, at + length)
            }
            TlOp::Call { f, position } => {
                let at = position_of(ctx, position, &cursor, &labels)?;
                marks.push((at, Mark::Call(f.clone())));
                (at, at)
            }
            TlOp::Label { name, position } => {
                let at = position_of(ctx, position, &cursor, &labels)?;
                labels.insert(name.clone(), at);
                continue;
            }
            TlOp::Sync {
                clip: other,
                position,
            } => {
                let at = position_of(ctx, position, &cursor, &labels)?;
                other.ensure_built();
                other.cancel();
                let timing = other.timing.borrow().clone();
                let (child, length) = unroll(other.tracks.borrow().clone(), &timing);
                merge(&mut tracks, child, at);
                (at, at + length)
            }
        };
        cursor.previous = (start, end);
        cursor.end = cursor.end.max(end);
    }
    for track in &mut tracks {
        track.segs.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    *clip.tracks.borrow_mut() = tracks;
    *clip.marks.borrow_mut() = marks;
    *clip.spans.borrow_mut() = spans;
    *clip.labels.borrow_mut() = labels;
    clip.timing.borrow_mut().iteration = cursor.end;
    Ok(())
}
fn position_of(
    ctx: &Ctx,
    v: &mlua::Value,
    cursor: &Cursor,
    labels: &std::collections::HashMap<String, f64>,
) -> Result<f64, String> {
    position(ctx, v, cursor, labels)
}
fn merge(tracks: &mut Vec<Track>, child: Vec<Track>, at: f64) {
    for mut track in child {
        for seg in &mut track.segs {
            seg.start += at;
            seg.end += at;
        }
        match tracks
            .iter_mut()
            .find(|t| t.target.same(&track.target) && t.key == track.key)
        {
            Some(existing) => {
                // A later child takes over from where it starts.
                let from = track.segs.first().map_or(f64::INFINITY, |s| s.start);
                existing.segs.retain(|s| s.start < from);
                existing.segs.extend(track.segs);
                existing.composition = track.composition;
                existing.modifier = existing.modifier.take().or(track.modifier);
            }
            None => tracks.push(track),
        }
    }
}
