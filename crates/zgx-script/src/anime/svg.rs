//! Motion along SVG paths (anime.js `svg.createMotionPath`) and path
//! lengths for drawing strokes progressively. Paths are flattened once into
//! points with cumulative lengths; sampling is a binary search.
use std::f64::consts::PI;

/// A flattened path: points and the length up to each.
pub(crate) struct Sampler {
    points: Vec<(f64, f64)>,
    lengths: Vec<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Axis {
    X,
    Y,
    /// The tangent's angle, in radians.
    Angle,
}
impl Sampler {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        let mut lengths = Vec::with_capacity(points.len());
        let mut total = 0.;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                let q = points[i - 1];
                total += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
            }
            lengths.push(total);
        }
        Self { points, lengths }
    }
    pub fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.)
    }
    /// The point (or angle) at `progress` (0..1) of the length.
    pub fn at(&self, progress: f64, axis: Axis) -> f64 {
        if self.points.is_empty() {
            return 0.;
        }
        let target = progress.clamp(0., 1.) * self.length();
        let i = self
            .lengths
            .partition_point(|l| *l < target)
            .clamp(1, self.points.len().max(2) - 1);
        let (a, b) = (
            self.points[i - 1],
            self.points.get(i).copied().unwrap_or(self.points[i - 1]),
        );
        let span = self.lengths.get(i).copied().unwrap_or(0.) - self.lengths[i - 1];
        let t = if span > 0. {
            (target - self.lengths[i - 1]) / span
        } else {
            0.
        };
        match axis {
            Axis::X => a.0 + (b.0 - a.0) * t,
            Axis::Y => a.1 + (b.1 - a.1) * t,
            Axis::Angle => (b.1 - a.1).atan2(b.0 - a.0),
        }
    }
}

const CURVE_STEPS: usize = 24;

fn cubic(out: &mut Vec<(f64, f64)>, p0: (f64, f64), c1: (f64, f64), c2: (f64, f64), p: (f64, f64)) {
    for i in 1..=CURVE_STEPS {
        let t = i as f64 / CURVE_STEPS as f64;
        let u = 1. - t;
        let w = [u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t];
        out.push((
            w[0] * p0.0 + w[1] * c1.0 + w[2] * c2.0 + w[3] * p.0,
            w[0] * p0.1 + w[1] * c1.1 + w[2] * c2.1 + w[3] * p.1,
        ));
    }
}
fn quad(out: &mut Vec<(f64, f64)>, p0: (f64, f64), c: (f64, f64), p: (f64, f64)) {
    for i in 1..=CURVE_STEPS {
        let t = i as f64 / CURVE_STEPS as f64;
        let u = 1. - t;
        out.push((
            u * u * p0.0 + 2. * u * t * c.0 + t * t * p.0,
            u * u * p0.1 + 2. * u * t * c.1 + t * t * p.1,
        ));
    }
}
/// An SVG elliptical arc, by the endpoint-to-center conversion in the SVG
/// specification, flattened.
#[allow(clippy::too_many_arguments)]
fn arc(
    out: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    rx: f64,
    ry: f64,
    phi: f64,
    large: bool,
    sweep: bool,
    p: (f64, f64),
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0. || ry == 0. || p0 == p {
        out.push(p);
        return;
    }
    let (sin, cos) = phi.to_radians().sin_cos();
    let (dx, dy) = ((p0.0 - p.0) / 2., (p0.1 - p.1) / 2.);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1. {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).max(0.).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let (cx1, cy1) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let (cx, cy) = (
        cos * cx1 - sin * cy1 + (p0.0 + p.0) / 2.,
        sin * cx1 + cos * cy1 + (p0.1 + p.1) / 2.,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let theta = angle(1., 0., (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !sweep && delta > 0. {
        delta -= 2. * PI;
    } else if sweep && delta < 0. {
        delta += 2. * PI;
    }
    let steps = ((delta.abs() / (PI / 2.)).ceil() as usize * CURVE_STEPS / 2).max(4);
    for i in 1..=steps {
        let a = theta + delta * i as f64 / steps as f64;
        let (s, c) = a.sin_cos();
        out.push((
            cx + rx * c * cos - ry * s * sin,
            cy + rx * c * sin + ry * s * cos,
        ));
    }
}

/// Parses SVG path data (`M`, `L`, `H`, `V`, `C`, `S`, `Q`, `T`, `A`, `Z`,
/// absolute and relative) into flattened points.
pub(crate) fn parse(d: &str) -> Result<Vec<(f64, f64)>, String> {
    let mut tokens = Vec::new();
    let mut chars = d.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            tokens.push(Tok::Cmd(c));
            chars.next();
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            let start = i;
            let mut end = i;
            let mut seen_dot = false;
            let mut seen_exp = false;
            let mut first = true;
            while let Some(&(j, ch)) = chars.peek() {
                let ok = ch.is_ascii_digit()
                    || (ch == '.' && !seen_dot && !seen_exp)
                    || ((ch == '-' || ch == '+') && (first || d[..j].ends_with(['e', 'E'])))
                    || ((ch == 'e' || ch == 'E') && !seen_exp);
                if !ok {
                    break;
                }
                seen_dot |= ch == '.';
                seen_exp |= ch == 'e' || ch == 'E';
                first = false;
                end = j + ch.len_utf8();
                chars.next();
            }
            tokens.push(Tok::Num(
                d[start..end]
                    .parse()
                    .map_err(|_| format!("bad number in path at {start}"))?,
            ));
        } else {
            chars.next();
        }
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    let (mut cur, mut start) = ((0., 0.), (0., 0.));
    let mut last_control: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    let mut i = 0;
    let mut cmd = 'M';
    let num = |i: &mut usize| -> Result<f64, String> {
        match tokens.get(*i) {
            Some(Tok::Num(n)) => {
                *i += 1;
                Ok(*n)
            }
            _ => Err("path data ends early".into()),
        }
    };
    while i < tokens.len() {
        if let Tok::Cmd(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                cur = start;
                out.push(cur);
                last_control = None;
                last_quad = None;
                continue;
            }
        }
        let relative = cmd.is_ascii_lowercase();
        let base = if relative { cur } else { (0., 0.) };
        let point = |i: &mut usize| -> Result<(f64, f64), String> {
            Ok((num(i)? + base.0, num(i)? + base.1))
        };
        match cmd.to_ascii_uppercase() {
            'M' => {
                cur = point(&mut i)?;
                start = cur;
                out.push(cur);
                // Further pairs are lines.
                cmd = if relative { 'l' } else { 'L' };
                last_control = None;
                last_quad = None;
            }
            'L' => {
                cur = point(&mut i)?;
                out.push(cur);
                last_control = None;
                last_quad = None;
            }
            'H' => {
                cur = (num(&mut i)? + if relative { cur.0 } else { 0. }, cur.1);
                out.push(cur);
                last_control = None;
                last_quad = None;
            }
            'V' => {
                cur = (cur.0, num(&mut i)? + if relative { cur.1 } else { 0. });
                out.push(cur);
                last_control = None;
                last_quad = None;
            }
            'C' => {
                let (c1, c2, p) = (point(&mut i)?, point(&mut i)?, point(&mut i)?);
                cubic(&mut out, cur, c1, c2, p);
                last_control = Some(c2);
                last_quad = None;
                cur = p;
            }
            'S' => {
                let c1 = last_control.map_or(cur, |c| (2. * cur.0 - c.0, 2. * cur.1 - c.1));
                let (c2, p) = (point(&mut i)?, point(&mut i)?);
                cubic(&mut out, cur, c1, c2, p);
                last_control = Some(c2);
                last_quad = None;
                cur = p;
            }
            'Q' => {
                let (c, p) = (point(&mut i)?, point(&mut i)?);
                quad(&mut out, cur, c, p);
                last_quad = Some(c);
                last_control = None;
                cur = p;
            }
            'T' => {
                let c = last_quad.map_or(cur, |c| (2. * cur.0 - c.0, 2. * cur.1 - c.1));
                let p = point(&mut i)?;
                quad(&mut out, cur, c, p);
                last_quad = Some(c);
                last_control = None;
                cur = p;
            }
            'A' => {
                let (rx, ry, phi) = (num(&mut i)?, num(&mut i)?, num(&mut i)?);
                let (large, sweep) = (num(&mut i)? != 0., num(&mut i)? != 0.);
                let p = point(&mut i)?;
                arc(&mut out, cur, rx, ry, phi, large, sweep, p);
                last_control = None;
                last_quad = None;
                cur = p;
            }
            other => return Err(format!("unknown path command `{other}`")),
        }
    }
    if out.is_empty() {
        return Err("empty path".into());
    }
    Ok(out)
}
enum Tok {
    Cmd(char),
    Num(f64),
}

/// The length of a canvas path, for drawing a fraction of a stroke.
pub(crate) fn canvas_length(path: &zgx::canvas::Path) -> f64 {
    use zgx::canvas::Segment;
    let mut points: Vec<(f64, f64)> = Vec::new();
    let (mut cur, mut start) = ((0., 0.), (0., 0.));
    let p = |p: &zgx::canvas::Point| (p.x as f64, p.y as f64);
    let mut total = 0.;
    let mut flush = |points: &mut Vec<(f64, f64)>| {
        total += Sampler::new(std::mem::take(points)).length();
    };
    for segment in path.segments() {
        match segment {
            Segment::Move(to) => {
                flush(&mut points);
                cur = p(to);
                start = cur;
                points.push(cur);
            }
            Segment::Line(to) => {
                cur = p(to);
                points.push(cur);
            }
            Segment::Quadratic(c, to) => {
                quad(&mut points, cur, p(c), p(to));
                cur = p(to);
            }
            Segment::Cubic(a, b, to) => {
                cubic(&mut points, cur, p(a), p(b), p(to));
                cur = p(to);
            }
            Segment::Close => {
                cur = start;
                points.push(cur);
            }
        }
    }
    flush(&mut points);
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_samples_paths() {
        let s = Sampler::new(parse("M0 0 H100 V100").unwrap());
        assert_eq!(s.length(), 200.);
        assert_eq!((s.at(0.25, Axis::X), s.at(0.25, Axis::Y)), (50., 0.));
        assert_eq!((s.at(0.75, Axis::X), s.at(0.75, Axis::Y)), (100., 50.));
        assert!((s.at(0.75, Axis::Angle) - PI / 2.).abs() < 1e-9);
        let rel = Sampler::new(parse("m10 10 l10 0 l0 10z").unwrap());
        assert!((rel.length() - (20. + 200f64.sqrt())).abs() < 1e-9);
        // A half circle of radius 50 by arc: length pi * 50.
        let arc = Sampler::new(parse("M0 0 A50 50 0 0 1 100 0").unwrap());
        assert!((arc.length() - PI * 50.).abs() < 0.5, "{}", arc.length());
        let curve = Sampler::new(parse("M0,0 C0,100 100,100 100,0 S200,-100 200,0").unwrap());
        assert!(curve.length() > 300.);
        assert!(parse("M0 0 L").is_err());
    }
}
