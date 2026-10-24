//! Runs script-declared animations in Rust. Each animation is one task that
//! awaits display frames only while it moves; at rest it waits on its source
//! state and requests nothing.
use crate::{
    node::{Anim, Curve},
    value::Value,
};
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
    time::Instant,
};
use zgx::{compose::Context, reactive::Signal};

/// Wakes a task when the source state changes.
#[derive(Default)]
struct Notify {
    pending: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
impl Notify {
    fn notify(&self) {
        self.pending.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
    async fn wait(&self) {
        poll_fn(|cx| {
            if self.pending.replace(false) {
                Poll::Ready(())
            } else {
                *self.waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

fn scalar(value: &Value) -> f32 {
    value.as_f64().unwrap_or(0.) as f32
}

/// The animated scalar for `anim`, owned by the component in `cx`.
pub(crate) fn drive(cx: &mut Context, anim: &Anim) -> Signal<f32> {
    let frames = cx.frames();
    let curve = anim.curve.clone();
    let Some(source) = anim.source.clone() else {
        let out = cx.state(0_f32);
        let value = out.clone();
        let Curve::Loop {
            ms,
            ease,
            yoyo,
            fps,
        } = curve.clone()
        else {
            unreachable!("only loops have no source")
        };
        cx.tasks().spawn(async move {
            let start = frames.next().await.time;
            loop {
                let frame = match fps {
                    Some(hz) => frames.next().max_rate(hz).await,
                    None => frames.next().await,
                };
                let cycles = (frame.time - start).as_secs_f32() * 1000. / ms;
                let phase = if yoyo {
                    let p = cycles % 2.;
                    if p > 1. { 2. - p } else { p }
                } else {
                    cycles.fract()
                };
                value.set(ease.at(phase as f64, None) as f32);
            }
        });
        return out;
    };
    let out = cx.state(source.with_untracked(scalar));
    let target = Rc::new(Cell::new(out.with_untracked(|v| *v)));
    let notify = Rc::new(Notify::default());
    let effect = {
        let (target, notify) = (target.clone(), notify.clone());
        cx.runtime().effect(move || {
            let next = scalar(&source.get());
            if next != target.replace(next) {
                notify.notify();
            }
        })
    };
    cx.retain(effect);
    let value = out.clone();
    cx.tasks().spawn(async move {
        loop {
            notify.wait().await;
            let mut velocity = 0_f32;
            let mut start: Option<(Instant, f32, f32)> = None;
            loop {
                let goal = target.get();
                let current = value.with_untracked(|v| *v);
                if current == goal {
                    break;
                }
                let frame = frames.next().await;
                match &curve {
                    Curve::Tween { ms, ease } => {
                        // A retarget mid-flight restarts from where it is.
                        let (t0, from, to) = match start {
                            Some(s) if s.2 == target.get() => s,
                            _ => *start.insert((frame.time, current, target.get())),
                        };
                        let t = (frame.time - t0).as_secs_f32() * 1000. / *ms;
                        value.set(if t >= 1. {
                            to
                        } else {
                            from + (to - from) * ease.at(t as f64, None) as f32
                        });
                    }
                    Curve::Spring { stiffness, damping } => {
                        let dt = frame.interval.as_secs_f32().min(1. / 30.);
                        let goal = target.get();
                        let x = value.with_untracked(|v| *v);
                        velocity += (-*stiffness * (x - goal) - *damping * velocity) * dt;
                        let x = x + velocity * dt;
                        let settle = 1e-3 * goal.abs().max(1.);
                        let settled = (x - goal).abs() < settle && velocity.abs() < settle * 10.;
                        value.set(if settled { goal } else { x });
                        if settled {
                            velocity = 0.;
                        }
                    }
                    Curve::Loop { .. } => unreachable!("loops have no source"),
                }
            }
        }
    });
    out
}
