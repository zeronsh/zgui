//! anime.js `onScroll(params)`, given as an animation's `autoplay`: links
//! playback to how far a target has travelled through a scroll container.
//! It reacts to the container's scroll offset (no polling): with `sync`, the
//! animation follows the scroll (exactly, eased, or smoothed over frames);
//! without, it plays on entering and reverses on leaving (`repeat`).
use super::{
    api::ClipUd,
    ease::Ease,
    engine::{Clip, Engine},
    target::Target,
};
use crate::{Vm, element::Element};
use mlua::{Function, Lua, Table, UserData};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Clone)]
enum Sync {
    Off,
    Exact,
    /// Follows the scroll with this much of the gap closed per 60 Hz frame.
    Smooth(f64),
    Eased(Ease),
}

/// An edge: a fraction of the size plus points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Edge {
    fraction: f64,
    points: f64,
}
fn edge(word: &str) -> Result<Edge, String> {
    let word = word.trim();
    let (base, offset) = match word.find("+=").or_else(|| word.find("-=")) {
        Some(i) => {
            let n: f64 = word[i + 2..]
                .trim()
                .trim_end_matches("px")
                .parse()
                .map_err(|_| format!("bad edge `{word}`"))?;
            (&word[..i], if &word[i..i + 1] == "+" { n } else { -n })
        }
        None => (word, 0.),
    };
    let fraction = match base.trim() {
        "start" | "top" | "left" => 0.,
        "center" => 0.5,
        "end" | "bottom" | "right" => 1.,
        "" => 0.,
        other if other.ends_with('%') => {
            other
                .trim_end_matches('%')
                .parse::<f64>()
                .map_err(|_| format!("bad edge `{word}`"))?
                / 100.
        }
        other => {
            return Ok(Edge {
                fraction: 0.,
                points: other
                    .trim_end_matches("px")
                    .parse()
                    .map_err(|_| format!("bad edge `{word}`"))?,
            });
        }
    };
    Ok(Edge {
        fraction,
        points: offset,
    })
}
/// `"<container edge> <target edge>"`, as anime.js writes thresholds.
fn threshold(text: &str) -> Result<(Edge, Edge), String> {
    let mut parts = text.split_whitespace();
    let container = edge(parts.next().unwrap_or("end"))?;
    let target = edge(parts.next().unwrap_or("start"))?;
    Ok((container, target))
}

pub(crate) struct ScrollParams {
    container: mlua::Value,
    target: mlua::Value,
    enter: (Edge, Edge),
    leave: (Edge, Edge),
    sync: Sync,
    repeat: bool,
    callbacks: [Option<Function>; 4],
}
#[derive(Clone)]
pub(crate) struct ScrollUd(pub Rc<ScrollParams>);
impl UserData for ScrollUd {}

const ENTER: usize = 0;
const LEAVE: usize = 1;
const UPDATE: usize = 2;
const SYNC: usize = 3;

/// `anime.onScroll(params)`.
pub(crate) fn on_scroll(lua: &Lua, params: Option<Table>) -> mlua::Result<ScrollUd> {
    let params = match params {
        Some(p) => p,
        None => lua.create_table()?,
    };
    let text = |k: &str, d: &str| -> mlua::Result<String> {
        Ok(params.get::<Option<String>>(k)?.unwrap_or_else(|| d.into()))
    };
    let sync = match params.get::<mlua::Value>("sync")? {
        mlua::Value::Nil | mlua::Value::Boolean(false) => Sync::Off,
        mlua::Value::Boolean(true) => Sync::Exact,
        mlua::Value::Integer(n) => Sync::Smooth((n as f64).clamp(0., 1.)),
        mlua::Value::Number(n) if n >= 1. => Sync::Exact,
        mlua::Value::Number(n) => Sync::Smooth(n.clamp(0., 1.)),
        mlua::Value::String(s) => {
            Sync::Eased(Ease::parse(&s.to_string_lossy()).map_err(mlua::Error::runtime)?)
        }
        other => {
            return Err(mlua::Error::runtime(format!(
                "sync is a boolean, number or ease, not {}",
                other.type_name()
            )));
        }
    };
    let f = |names: &[&str]| -> mlua::Result<Option<Function>> {
        for n in names {
            if let Some(f) = params.get::<Option<Function>>(*n)? {
                return Ok(Some(f));
            }
        }
        Ok(None)
    };
    Ok(ScrollUd(Rc::new(ScrollParams {
        container: params.get("container")?,
        target: params.get("target")?,
        enter: threshold(&text("enter", "end start")?).map_err(mlua::Error::runtime)?,
        leave: threshold(&text("leave", "start end")?).map_err(mlua::Error::runtime)?,
        sync,
        repeat: params.get::<Option<bool>>("repeat")?.unwrap_or(true),
        callbacks: [
            f(&["onEnter", "on_enter"])?,
            f(&["onLeave", "on_leave"])?,
            f(&["onUpdate", "on_update"])?,
            f(&["onSync", "on_sync"])?,
        ],
    })))
}

struct Observer {
    clip: Rc<Clip>,
    params: Rc<ScrollParams>,
    owner: Weak<Vm>,
    inside: Cell<bool>,
    progress: Cell<f64>,
    /// Smoothed progress and whether a ticker is easing toward the target.
    shown: Cell<f64>,
    smoothing: Cell<bool>,
}
impl Observer {
    fn call(&self, which: usize) {
        let (Some(f), Some(vm)) = (&self.params.callbacks[which], self.owner.upgrade()) else {
            return;
        };
        if let Err(e) = vm.call::<()>(f, ClipUd(self.clip.clone())) {
            vm.report(e);
        }
    }
    fn update(self: &Rc<Self>, progress: f64, engine: &Rc<Engine>) {
        let previous = self.progress.replace(progress);
        let inside = progress > 0. && progress < 1.;
        let was = self.inside.replace(inside);
        if inside && !was {
            self.call(ENTER);
            if matches!(self.params.sync, Sync::Off) {
                if previous >= 1. && self.params.repeat {
                    self.clip.reverse();
                } else if !self.clip.has_began() || self.params.repeat {
                    self.clip.play();
                }
            }
        }
        if !inside && was {
            self.call(LEAVE);
            if matches!(self.params.sync, Sync::Off) && self.params.repeat && progress <= 0. {
                self.clip.reverse();
            }
        }
        self.call(UPDATE);
        let total = self.clip.total();
        match &self.params.sync {
            Sync::Off => {}
            Sync::Exact => {
                self.clip.seek(progress * total);
                self.call(SYNC);
            }
            Sync::Eased(ease) => {
                self.clip.seek(ease.at(progress, None) * total);
                self.call(SYNC);
            }
            Sync::Smooth(k) => {
                if !self.smoothing.replace(true) {
                    let (this, k) = (Rc::downgrade(self), *k);
                    engine.tick_with(Rc::new(move |dt| {
                        let Some(this) = this.upgrade() else {
                            return false;
                        };
                        let target = this.progress.get();
                        let shown = this.shown.get();
                        // `k` of the gap per 60 Hz frame, at any frame rate.
                        let step = 1. - (1. - k).powf(dt / (1000. / 60.));
                        let next = if (target - shown).abs() < 1e-4 {
                            target
                        } else {
                            shown + (target - shown) * step
                        };
                        this.shown.set(next);
                        let total = this.clip.total();
                        this.clip.seek(next * total);
                        this.call(SYNC);
                        let moving = next != target;
                        if !moving {
                            this.smoothing.set(false);
                        }
                        moving
                    }));
                }
            }
        }
    }
}

/// Links `clip`'s playback to the scroll, once its container and target
/// are mounted.
pub(crate) fn link(engine: &Rc<Engine>, clip: Rc<Clip>, params: Rc<ScrollParams>) {
    let observer = Rc::new(Observer {
        clip: clip.clone(),
        params: params.clone(),
        owner: clip.owner.clone(),
        inside: Cell::new(false),
        progress: Cell::new(0.),
        shown: Cell::new(0.),
        smoothing: Cell::new(false),
    });
    let (weak_engine, linked) = (Rc::downgrade(engine), Rc::new(RefCell::new(None)));
    let keep = linked.clone();
    engine.defer(Rc::new(move || {
        let (Some(engine), Some(vm)) = (weak_engine.upgrade(), observer.owner.upgrade()) else {
            return true;
        };
        if keep.borrow().is_some() {
            return true;
        }
        let ctx = super::build::Ctx {
            lua: vm.lua.clone(),
            vm,
            engine: engine.clone(),
        };
        let element_of = |v: &mlua::Value| -> Option<Rc<Element>> {
            super::api::targets(&ctx, v.clone())
                .ok()?
                .into_iter()
                .find_map(|t| match t {
                    Target::Element(e) => Some(e),
                    _ => None,
                })
        };
        // The target defaults to the animation's first element.
        let target = if params.target.is_nil() {
            observer.clip.ensure_built();
            observer
                .clip
                .tracks
                .borrow()
                .iter()
                .find_map(|t| match &t.target {
                    Target::Element(e) => Some(e.clone()),
                    _ => None,
                })
        } else {
            element_of(&params.target)
        };
        let Some(target) = target else { return false };
        // The container defaults to the target's nearest scroll ancestor.
        let container = if params.container.is_nil() {
            nearest_scroll(&engine, &target)
        } else {
            element_of(&params.container)
        };
        let Some(container) = container else {
            return false;
        };
        let Some((offset, horizontal)) = container.scroll.get().cloned() else {
            return false;
        };
        let (observer, engine2) = (observer.clone(), engine.clone());
        let effect = engine.runtime.effect(move || {
            let scrolled = offset.get() as f64;
            // Positions in the scroll content: the content node moves with the
            // scroll, so the target's distance from it does not.
            let geometry = container.read(|s, id| {
                let viewport = *s.children(id).first()?;
                let content = *s.children(viewport).first()?;
                Some((s.bounds(content), s.bounds(id)))
            });
            let (Some(Some((content, c))), Some(t)) = (geometry, target.read(|s, id| s.bounds(id)))
            else {
                return;
            };
            let (t_start, t_size, c_size) = if horizontal {
                ((t.x - content.x) as f64, t.width as f64, c.width as f64)
            } else {
                ((t.y - content.y) as f64, t.height as f64, c.height as f64)
            };
            let at = |(ce, te): (Edge, Edge)| {
                t_start + te.fraction * t_size + te.points - ce.fraction * c_size - ce.points
            };
            let (enter, leave) = (at(observer.params.enter), at(observer.params.leave));
            let progress = if leave == enter {
                (scrolled >= enter) as u8 as f64
            } else {
                ((scrolled - enter) / (leave - enter)).clamp(0., 1.)
            };
            observer.update(progress, &engine2);
        });
        *keep.borrow_mut() = Some(effect);
        true
    }));
    // The effect lives as long as the clip, which the engine keeps until its
    // script is replaced: a scroll-linked animation needs no script handle.
    clip.keep.borrow_mut().push(Box::new(linked));
    engine.hold(clip);
}

fn nearest_scroll(engine: &Engine, target: &Element) -> Option<Rc<Element>> {
    let scene = target.scene()?;
    let scene = scene.borrow();
    let mut node = target.node();
    while let Some(id) = node {
        if let Some(element) = engine.elements.at(id)
            && element.scroll.get().is_some()
        {
            return Some(element);
        }
        node = scene.parent(id);
    }
    None
}
