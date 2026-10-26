//! anime.js v4 easings, ported formula for formula: power (`in(p)`,
//! `out(p)`…), Quad to Quint, Sine, Circ, Expo, Bounce, Back(overshoot),
//! Elastic(amplitude, period) in `in`, `out`, `inOut` and `outIn` forms,
//! plus `steps`, `cubicBezier`, piecewise `linear`, `irregular` and springs.
//! v3 names (`easeInOutQuad`) and snake case (`in_out_quad`) also parse.
use std::{f64::consts::PI, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
    In,
    Out,
    InOut,
    OutIn,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Base {
    Power(f64),
    Sine,
    Circ,
    Expo,
    Bounce,
    Back(f64),
    /// Amplitude and period, clamped as anime.js does.
    Elastic(f64, f64),
}
impl Base {
    fn ease_in(self, t: f64) -> f64 {
        match self {
            Base::Power(p) => t.powf(p),
            Base::Sine => 1. - (t * PI / 2.).cos(),
            Base::Circ => 1. - (1. - t * t).max(0.).sqrt(),
            Base::Expo => {
                if t == 0. {
                    0.
                } else {
                    2f64.powf(10. * t - 10.)
                }
            }
            Base::Bounce => {
                let mut b = 4.;
                let mut pow2;
                loop {
                    b -= 1.;
                    pow2 = 2f64.powf(b);
                    if t >= (pow2 - 1.) / 11. || b <= -10. {
                        break;
                    }
                }
                1. / 4f64.powf(3. - b) - 7.5625 * ((pow2 * 3. - 2.) / 22. - t).powi(2)
            }
            Base::Back(overshoot) => (overshoot + 1.) * t * t * t - overshoot * t * t,
            Base::Elastic(amplitude, period) => {
                if t == 0. || t == 1. {
                    return t;
                }
                let a = amplitude.clamp(1., 10.);
                let p = period.clamp(1e-9, 2.);
                let s = (p / (2. * PI)) * (1. / a).asin();
                let e = 2. * PI / p;
                -a * 2f64.powf(-10. * (1. - t)) * (((1. - t) - s) * e).sin()
            }
        }
    }
    fn apply(self, mode: Mode, t: f64) -> f64 {
        match mode {
            Mode::In => self.ease_in(t),
            Mode::Out => 1. - self.ease_in(1. - t),
            Mode::InOut if t < 0.5 => self.ease_in(t * 2.) / 2.,
            Mode::InOut => 1. - self.ease_in(t * -2. + 2.) / 2.,
            Mode::OutIn if t < 0.5 => (1. - self.ease_in(1. - t * 2.)) / 2.,
            Mode::OutIn => (self.ease_in(t * 2. - 1.) + 1.) / 2.,
        }
    }
}

/// A damped spring, solved in closed form (anime.js `createSpring`). Its
/// settling time becomes the duration of tweens using it.
#[derive(Debug)]
pub(crate) struct Spring {
    zeta: f64,
    w0: f64,
    wd: f64,
    b: f64,
    /// Seconds until it settles.
    pub settle: f64,
    /// The last time it was solved at (its bits) and the value then. The
    /// tracks of one tween share its spring and its timing: a list's rows
    /// moving together ask for the same time in turn, a thousand of them a
    /// frame.
    last: std::cell::Cell<Option<(u64, f64)>>,
}
impl PartialEq for Spring {
    fn eq(&self, other: &Self) -> bool {
        (self.zeta, self.w0, self.wd, self.b, self.settle)
            == (other.zeta, other.w0, other.wd, other.b, other.settle)
    }
}
impl Spring {
    pub fn new(mass: f64, stiffness: f64, damping: f64, velocity: f64) -> Self {
        let mass = mass.clamp(1e-3, 1e4);
        let stiffness = stiffness.clamp(1e-3, 1e5);
        let damping = damping.clamp(1e-3, 1e4);
        let w0 = (stiffness / mass).sqrt();
        let zeta = damping / (2. * (stiffness * mass).sqrt());
        let wd = if zeta < 1. {
            w0 * (1. - zeta * zeta).sqrt()
        } else {
            0.
        };
        let b = if zeta < 1. {
            (zeta * w0 - velocity) / wd
        } else {
            -velocity + w0
        };
        let mut spring = Self {
            zeta,
            w0,
            wd,
            b,
            settle: 0.,
            last: std::cell::Cell::new(None),
        };
        // Settled once it stays within a thousandth of rest for a sixth of a
        // second, sampled at 120 Hz (the fastest displays animate at).
        let (step, hold) = (1. / 120., 20);
        let (mut time, mut still) = (0., 0);
        while still < hold && time < 60. {
            time += step;
            if (1. - spring.solve(time)).abs() < 1e-3 {
                still += 1;
            } else {
                still = 0;
            }
        }
        spring.settle = (time - step * hold as f64).max(step);
        spring
    }
    /// From perceived `bounce` (-1..1, 0 = critically damped) and duration
    /// in seconds, as anime.js `spring()` and SwiftUI define it.
    pub fn perceptual(bounce: f64, duration: f64) -> Self {
        let duration = duration.max(1e-3);
        let stiffness = (2. * PI / duration).powi(2);
        let ratio = (1. - bounce.clamp(-1., 0.99)).max(0.01);
        Self::new(1., stiffness, 2. * ratio * stiffness.sqrt(), 0.)
    }
    /// `solve`, remembering the last answer.
    fn at(&self, time: f64) -> f64 {
        if let Some((bits, value)) = self.last.get()
            && bits == time.to_bits()
        {
            return value;
        }
        let value = self.solve(time);
        self.last.set(Some((time.to_bits(), value)));
        value
    }
    fn solve(&self, time: f64) -> f64 {
        let t = if self.zeta < 1. {
            (-time * self.zeta * self.w0).exp()
                * ((self.wd * time).cos() + self.b * (self.wd * time).sin())
        } else {
            (1. + self.b * time) * (-time * self.w0).exp()
        };
        1. - t
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Ease {
    Linear,
    Curve(Mode, Base),
    Steps {
        count: f64,
        from_start: bool,
    },
    Bezier([f64; 4]),
    /// Points (t, value) in increasing t.
    Piecewise(Rc<[(f64, f64)]>),
    Spring(Rc<Spring>),
    /// A script function: called every frame, so it costs a script call.
    Script(mlua::Function),
    /// Played backwards: a reversed or alternating iteration.
    Mirror(Rc<Ease>),
}
impl PartialEq for Ease {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Ease::Linear, Ease::Linear) => true,
            (Ease::Curve(a, b), Ease::Curve(c, d)) => a == c && b == d,
            (
                Ease::Steps {
                    count: a,
                    from_start: b,
                },
                Ease::Steps {
                    count: c,
                    from_start: d,
                },
            ) => a == c && b == d,
            (Ease::Bezier(a), Ease::Bezier(b)) => a == b,
            (Ease::Piecewise(a), Ease::Piecewise(b)) => a == b,
            (Ease::Spring(a), Ease::Spring(b)) => Rc::ptr_eq(a, b),
            (Ease::Script(a), Ease::Script(b)) => a == b,
            (Ease::Mirror(a), Ease::Mirror(b)) => a == b,
            _ => false,
        }
    }
}
impl Ease {
    /// anime.js v4's default: `out(2)`.
    pub const DEFAULT: Ease = Ease::Curve(Mode::Out, Base::Power(2.));

    pub fn at(&self, t: f64, vm: Option<&crate::Vm>) -> f64 {
        match self {
            Ease::Linear => t,
            Ease::Curve(mode, base) => base.apply(*mode, t),
            Ease::Steps { count, from_start } => {
                let scaled = t.clamp(0., 1.) * count;
                (if *from_start {
                    scaled.ceil()
                } else {
                    scaled.floor()
                }) / count
            }
            Ease::Bezier([x1, y1, x2, y2]) => {
                if t == 0. || t == 1. {
                    return t;
                }
                bezier(solve_bezier_x(t, *x1, *x2), *y1, *y2)
            }
            Ease::Piecewise(points) => {
                let i = points.partition_point(|(x, _)| *x <= t);
                match (points.get(i.wrapping_sub(1)), points.get(i)) {
                    (Some(a), Some(b)) if b.0 > a.0 => a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0),
                    (Some(a), _) => a.1,
                    (None, Some(b)) => b.1,
                    (None, None) => t,
                }
            }
            Ease::Spring(spring) => {
                if t >= 1. {
                    1.
                } else {
                    spring.at(t * spring.settle)
                }
            }
            Ease::Mirror(inner) => 1. - inner.at(1. - t, vm),
            Ease::Script(f) => match vm {
                Some(vm) => vm.call::<f64>(f, t).unwrap_or_else(|e| {
                    vm.report(e);
                    t
                }),
                None => t,
            },
        }
    }
    /// The duration a spring sets for tweens that do not give their own.
    pub fn spring_duration_ms(&self) -> Option<f64> {
        match self {
            Ease::Spring(spring) => Some(spring.settle * 1000.),
            _ => None,
        }
    }

    /// Parses an ease name with optional arguments: `outQuad`, `out(3)`,
    /// `inOutBack(1.2)`, `outElastic(1, .4)`, `steps(5)`,
    /// `cubicBezier(.2, 0, 0, 1)`, `linear(0, .5 30%, 1)`, `irregular(10, 1)`.
    pub fn parse(text: &str) -> Result<Ease, String> {
        let text = text.trim();
        let (name, args) = match text.find('(') {
            Some(open) => {
                let close = text
                    .rfind(')')
                    .ok_or_else(|| format!("unclosed ease `{text}`"))?;
                (&text[..open], Some(&text[open + 1..close]))
            }
            None => (text, None),
        };
        let numbers = |args: Option<&str>| -> Result<Vec<f64>, String> {
            args.map_or(Ok(Vec::new()), |a| {
                a.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        s.parse::<f64>()
                            .map_err(|_| format!("bad ease argument `{s}` in `{text}`"))
                    })
                    .collect()
            })
        };
        let lower: String = name
            .chars()
            .filter(|c| *c != '_' && *c != '-')
            .collect::<String>()
            .to_ascii_lowercase();
        // v3 spelled every ease with an `ease` prefix.
        let lower = lower
            .strip_prefix("ease")
            .filter(|rest| !rest.is_empty())
            .unwrap_or(&lower)
            .to_owned();
        match lower.as_str() {
            "linear" if args.is_some_and(|a| !a.trim().is_empty()) => {
                return piecewise(args.unwrap());
            }
            "linear" | "none" => return Ok(Ease::Linear),
            "steps" => {
                let mut parts = args.unwrap_or("").split(',').map(str::trim);
                let count: f64 = parts
                    .next()
                    .filter(|s| !s.is_empty())
                    .map_or(Ok(10.), |s| s.parse())
                    .map_err(|_| format!("bad steps in `{text}`"))?;
                let from_start = parts
                    .next()
                    .is_some_and(|s| s == "true" || s == "start" || s == "jump-start");
                return Ok(Ease::Steps {
                    count: count.max(1.),
                    from_start,
                });
            }
            "cubicbezier" => {
                let n = numbers(args)?;
                let [x1, y1, x2, y2] = <[f64; 4]>::try_from(n.as_slice())
                    .map_err(|_| format!("cubicBezier takes four numbers: `{text}`"))?;
                if x1 == y1 && x2 == y2 {
                    return Ok(Ease::Linear);
                }
                return Ok(Ease::Bezier([x1.clamp(0., 1.), y1, x2.clamp(0., 1.), y2]));
            }
            "irregular" => {
                let n = numbers(args)?;
                return Ok(irregular(
                    n.first().copied().unwrap_or(10.),
                    n.get(1).copied().unwrap_or(1.),
                ));
            }
            "spring" => {
                let n = numbers(args)?;
                return Ok(Ease::Spring(Rc::new(Spring::new(
                    n.first().copied().unwrap_or(1.),
                    n.get(1).copied().unwrap_or(100.),
                    n.get(2).copied().unwrap_or(10.),
                    n.get(3).copied().unwrap_or(0.),
                ))));
            }
            _ => {}
        }
        let (mode, rest) = [
            ("inout", Mode::InOut),
            ("outin", Mode::OutIn),
            ("in", Mode::In),
            ("out", Mode::Out),
        ]
        .into_iter()
        .find_map(|(prefix, mode)| lower.strip_prefix(prefix).map(|rest| (mode, rest)))
        .ok_or_else(|| format!("unknown ease `{text}`"))?;
        let n = numbers(args)?;
        let base = match rest {
            "" => Base::Power(n.first().copied().unwrap_or(1.68)),
            "quad" => Base::Power(2.),
            "cubic" => Base::Power(3.),
            "quart" => Base::Power(4.),
            "quint" => Base::Power(5.),
            "sine" => Base::Sine,
            "circ" => Base::Circ,
            "expo" => Base::Expo,
            "bounce" => Base::Bounce,
            "back" => Base::Back(n.first().copied().unwrap_or(1.70158)),
            "elastic" => Base::Elastic(
                n.first().copied().unwrap_or(1.),
                n.get(1).copied().unwrap_or(0.3),
            ),
            _ => return Err(format!("unknown ease `{text}`")),
        };
        Ok(Ease::Curve(mode, base))
    }
}

fn bezier(t: f64, a1: f64, a2: f64) -> f64 {
    (((1. - 3. * a2 + 3. * a1) * t + (3. * a2 - 6. * a1)) * t + 3. * a1) * t
}
/// Binary subdivision, as anime.js solves it.
fn solve_bezier_x(x: f64, x1: f64, x2: f64) -> f64 {
    let (mut a, mut b, mut t) = (0., 1., 0.5);
    for _ in 0..100 {
        t = a + (b - a) / 2.;
        let current = bezier(t, x1, x2) - x;
        if current.abs() <= 1e-7 {
            break;
        }
        if current > 0. {
            b = t;
        } else {
            a = t;
        }
    }
    t
}

/// CSS `linear()`: values with optional percentage positions; missing
/// positions spread evenly between known ones.
fn piecewise(args: &str) -> Result<Ease, String> {
    let mut points: Vec<(Option<f64>, f64)> = Vec::new();
    for part in args.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let mut words = part.split_whitespace();
        let value: f64 = words
            .next()
            .unwrap_or("")
            .parse()
            .map_err(|_| format!("bad linear() value `{part}`"))?;
        let position = match words.next() {
            Some(p) => Some(
                p.trim_end_matches('%')
                    .parse::<f64>()
                    .map_err(|_| format!("bad linear() stop `{part}`"))?
                    / 100.,
            ),
            None => None,
        };
        points.push((position, value));
    }
    if points.len() < 2 {
        return Ok(Ease::Linear);
    }
    let last = points.len() - 1;
    points[0].0.get_or_insert(0.);
    points[last].0.get_or_insert(1.);
    let mut i = 0;
    while i < points.len() {
        if points[i].0.is_some() {
            i += 1;
            continue;
        }
        let start = i - 1;
        let end = (i..points.len()).find(|&j| points[j].0.is_some()).unwrap();
        let (a, b) = (points[start].0.unwrap(), points[end].0.unwrap());
        for (k, j) in (i..end).enumerate() {
            points[j].0 = Some(a + (b - a) * (k + 1) as f64 / (end - start) as f64);
        }
        i = end;
    }
    let mut resolved: Vec<(f64, f64)> = points.into_iter().map(|(p, v)| (p.unwrap(), v)).collect();
    for i in 1..resolved.len() {
        resolved[i].0 = resolved[i].0.max(resolved[i - 1].0);
    }
    Ok(Ease::Piecewise(resolved.into()))
}

/// anime.js `irregular(length, randomness)`: a random walk between 0 and 1,
/// fixed per call. Seeded from the arguments so a script reload repeats it.
fn irregular(length: f64, randomness: f64) -> Ease {
    let n = length.clamp(1., 1000.) as usize;
    let mut seed = (length.to_bits() ^ randomness.to_bits().rotate_left(17)) | 1;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut points = vec![(0., 0.)];
    for i in 1..n {
        let base = i as f64 / n as f64;
        let value = base + (random() - 0.5) * randomness / n as f64 * 2.;
        points.push((base, value.clamp(0., 1.)));
    }
    points.push((1., 1.));
    Ease::Piecewise(points.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(name: &str, t: f64) -> f64 {
        Ease::parse(name).unwrap().at(t, None)
    }
    #[test]
    fn matches_anime_js_v4_formulas() {
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-6, "{a} != {b}");
        close(at("linear", 0.3), 0.3);
        close(at("outQuad", 0.5), 0.75);
        close(at("inQuad", 0.5), 0.25);
        close(at("inOutCubic", 0.25), 0.0625);
        close(at("in", 0.5), 0.5f64.powf(1.68));
        close(at("out(3)", 0.5), 0.875);
        close(at("inOutQuad", 0.75), 0.875);
        close(at("outInQuad", 0.25), 0.375);
        close(at("inSine", 1.), 1.);
        close(at("inExpo", 0.), 0.);
        close(at("outBounce", 1.), 1.);
        close(at("outBounce", 0.5), 0.765625);
        close(at("inBounce", 0.5), 0.234375);
        close(at("outElastic", 0.), 0.);
        close(at("outElastic", 1.), 1.);
        close(at("inBack", 0.5), (1.70158 + 1.) * 0.125 - 1.70158 * 0.25);
        close(at("steps(4)", 0.3), 0.25);
        close(at("steps(4, true)", 0.3), 0.5);
        close(at("cubicBezier(0, 0, 1, 1)", 0.37), 0.37);
        close(at("cubicBezier(.25, .1, .25, 1)", 0.5), 0.8024033877399112);
        close(at("linear(0, 0.25 50%, 1)", 0.25), 0.125);
        close(at("linear(0, 0.25 50%, 1)", 0.75), 0.625);
        // v3 and snake-case spellings.
        close(at("easeInOutQuad", 0.75), 0.875);
        close(at("in_out_quad", 0.75), 0.875);
    }
    /// Its last answer, kept for tracks asking for the same time in turn,
    /// is the one solving it again gives, bit for bit.
    #[test]
    fn a_spring_answers_a_time_asked_again_as_solved() {
        let spring = Spring::perceptual(0.1, 0.4);
        for time in [0.1, 0.1, 0.2, 0.1, -0., 0., f64::NAN, f64::NAN, 0.35, 0.35] {
            let (kept, solved) = (spring.at(time), spring.solve(time));
            assert_eq!(kept.to_bits(), solved.to_bits(), "{time}");
        }
    }
    #[test]
    fn springs_settle_and_end_at_one() {
        let spring = Spring::new(1., 100., 10., 0.);
        assert!(
            spring.settle > 0.3 && spring.settle < 3.,
            "{}",
            spring.settle
        );
        let ease = Ease::Spring(Rc::new(spring));
        assert_eq!(ease.at(1., None), 1.);
        assert!(ease.at(0.2, None) > 0.5);
        let bouncy = Spring::perceptual(0.5, 0.5);
        let max = (0..100)
            .map(|i| Ease::Spring(Rc::new(Spring::perceptual(0.5, 0.5))).at(i as f64 / 100., None))
            .fold(0., f64::max);
        assert!(max > 1.05, "a bouncy spring overshoots: {max}");
        assert!(bouncy.settle > 0.4);
    }
}
