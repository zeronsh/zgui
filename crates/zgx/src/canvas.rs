//! Retained, backend-independent vector drawing in local logical coordinates.
use crate::{
    affine::Affine,
    scene::{Color, Rect},
};
use std::sync::Arc;

/// A point in the canvas's local logical coordinates, y down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Across.
    pub x: f32,
    /// Down.
    pub y: f32,
}
impl Point {
    /// The point at `x`, `y`.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
/// One step of a path.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    /// Start a new subpath here.
    Move(Point),
    /// A straight line to here.
    Line(Point),
    /// A quadratic Bézier curve through a control point to an end point.
    Quadratic(Point, Point),
    /// A cubic Bézier curve through two control points to an end point.
    Cubic(Point, Point, Point),
    /// Close the subpath with a line back to its start.
    Close,
}
/// A path known as a simple shape (made by its constructor, or a builder
/// given nothing else): a renderer may draw it as that shape, without
/// rasterizing its segments, which stay what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A rectangle, its corners rounded by `radius` (0: square; at most
    /// half its shorter side).
    Rect {
        /// The rectangle.
        rect: Rect,
        /// Its corners' radius.
        radius: f32,
    },
    /// The ellipse inscribed in `rect` (a circle when it is square).
    Ellipse {
        /// Its bounds.
        rect: Rect,
    },
    /// A straight line from `from` to `to`.
    Line {
        /// Where it starts.
        from: Point,
        /// Where it ends.
        to: Point,
    },
}

/// An immutable vector path, cheap to clone. Its coordinates are finite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    segments: Arc<[Segment]>,
    shape: Option<Shape>,
}
impl Path {
    /// Start building a path.
    pub fn builder() -> PathBuilder {
        PathBuilder::default()
    }
    /// Its segments, in order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
    /// The simple shape it is, if it was made as one.
    pub fn shape(&self) -> Option<Shape> {
        self.shape
    }
    /// The outline of `rect`.
    pub fn rectangle(rect: Rect) -> Self {
        Self::rounded_rect(rect, 0.).expect("finite rectangle")
    }
    /// The outline of `rect` with corners rounded by `radius` (at most half
    /// its shorter side), or an error unless both are finite.
    pub fn rounded_rect(rect: Rect, radius: f32) -> Result<Self, &'static str> {
        let mut path = Self::builder();
        path.rounded_rect(rect, radius);
        path.build()
    }
    /// The ellipse inscribed in `rect`, or an error unless it is finite.
    pub fn ellipse(rect: Rect) -> Result<Self, &'static str> {
        let mut path = Self::builder();
        path.ellipse(rect);
        path.build()
    }
    /// A straight line from `from` to `to`, or an error unless they are
    /// finite.
    pub fn line(from: Point, to: Point) -> Result<Self, &'static str> {
        let mut path = Self::builder();
        path.move_to(from.x, from.y).line_to(to.x, to.y);
        path.shape = Some(Shape::Line { from, to });
        path.build()
    }
}
/// Builds a [`Path`] segment by segment; [`PathBuilder::build`] checks it.
#[derive(Default)]
pub struct PathBuilder {
    segments: Vec<Segment>,
    transform: Affine,
    /// The simple shape the path is, while it is nothing else.
    shape: Option<Shape>,
}
impl PathBuilder {
    /// A segment added: the path is no longer just its shape.
    fn push(&mut self, segment: Segment) -> &mut Self {
        self.segments.push(segment);
        self.shape = None;
        self
    }
    /// Start a new subpath at `x`, `y`.
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(Segment::Move(Point::new(x, y)))
    }
    /// A straight line to `x`, `y`.
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(Segment::Line(Point::new(x, y)))
    }
    /// A quadratic curve through `cx`, `cy` to `x`, `y`.
    pub fn quadratic_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self {
        self.push(Segment::Quadratic(Point::new(cx, cy), Point::new(x, y)))
    }
    /// A cubic curve through `c1` and `c2` to `end`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> &mut Self {
        self.push(Segment::Cubic(c1, c2, end))
    }
    /// Add `path`'s segments.
    pub fn append(&mut self, path: &Path) -> &mut Self {
        let first = self.segments.is_empty();
        self.segments.extend_from_slice(path.segments());
        self.shape = if first { path.shape() } else { None };
        self
    }
    /// Close the subpath.
    pub fn close(&mut self) -> &mut Self {
        self.push(Segment::Close)
    }
    /// A rectangle's outline with corners rounded by `radius` (at most half
    /// its shorter side), as a closed subpath: on an empty builder, the path
    /// is that shape (`Path::shape`).
    pub fn rounded_rect(&mut self, rect: Rect, radius: f32) -> &mut Self {
        let first = self.segments.is_empty();
        let radius = radius
            .max(0.)
            .min(rect.width.abs() / 2.)
            .min(rect.height.abs() / 2.);
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        if radius > 0. {
            use std::f32::consts::{FRAC_PI_2, PI};
            self.move_to(x + radius, y).line_to(x + w - radius, y);
            self.arc(
                Point::new(x + w - radius, y + radius),
                radius,
                -FRAC_PI_2,
                FRAC_PI_2,
            );
            self.line_to(x + w, y + h - radius);
            self.arc(
                Point::new(x + w - radius, y + h - radius),
                radius,
                0.,
                FRAC_PI_2,
            );
            self.line_to(x + radius, y + h);
            self.arc(
                Point::new(x + radius, y + h - radius),
                radius,
                FRAC_PI_2,
                FRAC_PI_2,
            );
            self.line_to(x, y + radius);
            self.arc(Point::new(x + radius, y + radius), radius, PI, FRAC_PI_2);
        } else {
            self.move_to(x, y)
                .line_to(x + w, y)
                .line_to(x + w, y + h)
                .line_to(x, y + h);
        }
        self.close();
        if first {
            self.shape = Some(Shape::Rect { rect, radius });
        }
        self
    }
    /// The ellipse inscribed in `rect`, as a closed subpath of four cubic
    /// arcs: on an empty builder, the path is that shape.
    pub fn ellipse(&mut self, rect: Rect) -> &mut Self {
        let first = self.segments.is_empty();
        let k = 0.552_284_8;
        let (rx, ry) = (rect.width / 2., rect.height / 2.);
        let (cx, cy) = (rect.x + rx, rect.y + ry);
        self.move_to(cx + rx, cy);
        self.cubic_to(
            Point::new(cx + rx, cy + ry * k),
            Point::new(cx + rx * k, cy + ry),
            Point::new(cx, cy + ry),
        );
        self.cubic_to(
            Point::new(cx - rx * k, cy + ry),
            Point::new(cx - rx, cy + ry * k),
            Point::new(cx - rx, cy),
        );
        self.cubic_to(
            Point::new(cx - rx, cy - ry * k),
            Point::new(cx - rx * k, cy - ry),
            Point::new(cx, cy - ry),
        );
        self.cubic_to(
            Point::new(cx + rx * k, cy - ry),
            Point::new(cx + rx, cy - ry * k),
            Point::new(cx + rx, cy),
        );
        self.close();
        if first {
            self.shape = Some(Shape::Ellipse { rect });
        }
        self
    }
    /// Append a circular arc, connecting to its start with a line. Angles are
    /// clockwise radians; sweeps are bounded to one full turn in either direction.
    pub fn arc(&mut self, center: Point, radius: f32, start: f32, sweep: f32) -> &mut Self {
        if ![center.x, center.y, radius, start, sweep]
            .iter()
            .all(|v| v.is_finite())
            || radius < 0.
        {
            return self.line_to(f32::NAN, f32::NAN);
        }
        let sweep = sweep.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
        let steps = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.) as usize;
        let delta = sweep / steps as f32;
        let first = Point::new(
            center.x + radius * start.cos(),
            center.y + radius * start.sin(),
        );
        if self.segments.is_empty() {
            self.move_to(first.x, first.y);
        } else {
            self.line_to(first.x, first.y);
        }
        for index in 0..steps {
            let a = start + index as f32 * delta;
            let b = a + delta;
            let k = 4. / 3. * (delta / 4.).tan();
            self.cubic_to(
                Point::new(
                    center.x + radius * (a.cos() - k * a.sin()),
                    center.y + radius * (a.sin() + k * a.cos()),
                ),
                Point::new(
                    center.x + radius * (b.cos() + k * b.sin()),
                    center.y + radius * (b.sin() - k * b.cos()),
                ),
                Point::new(center.x + radius * b.cos(), center.y + radius * b.sin()),
            );
        }
        self
    }
    /// Transform every point by `transform` when built, after any transform
    /// already set.
    pub fn transform(&mut self, transform: Affine) -> &mut Self {
        self.transform = self.transform.then(transform);
        self
    }
    /// The path, or an error if it has more than 65,536 segments or a point
    /// isn't finite after transforming.
    pub fn build(self) -> Result<Path, &'static str> {
        if self.segments.len() > 65536 {
            return Err("path exceeds 65536 segments");
        }
        let transform = self.transform;
        // Moved or scaled, a shape is that shape no longer: its points are.
        let shape = self.shape.filter(|_| transform == Affine::IDENTITY);
        let point = |p: Point| -> Result<Point, &'static str> {
            let (x, y) = transform.point(p.x, p.y);
            if x.is_finite() && y.is_finite() {
                Ok(Point::new(x, y))
            } else {
                Err("path coordinates must be finite")
            }
        };
        let segments = self
            .segments
            .into_iter()
            .map(|s| {
                Ok(match s {
                    Segment::Move(p) => Segment::Move(point(p)?),
                    Segment::Line(p) => Segment::Line(point(p)?),
                    Segment::Quadratic(a, b) => Segment::Quadratic(point(a)?, point(b)?),
                    Segment::Cubic(a, b, c) => Segment::Cubic(point(a)?, point(b)?, point(c)?),
                    Segment::Close => Segment::Close,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(Path {
            segments: segments.into(),
            shape,
        })
    }
}
/// A colour at a point along a gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Where, from 0 at the start to 1 at the end.
    pub offset: f32,
    /// The colour there.
    pub color: Color,
}
/// What fills or strokes a path.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    /// One colour.
    Solid(Color),
    /// A linear gradient from `start` to `end` (see [`Brush::linear`]).
    Linear {
        /// Where offset 0 lies.
        start: Point,
        /// Where offset 1 lies.
        end: Point,
        /// The colours along it, in offset order.
        stops: Arc<[GradientStop]>,
    },
    /// Diagonal parallel stripes in logical coordinates.
    Slash {
        /// The colour between the stripes.
        background: Color,
        /// The stripes' colour.
        foreground: Color,
        /// The distance from one stripe to the next.
        spacing: f32,
        /// Each stripe's width.
        width: f32,
    },
}
impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}
impl Brush {
    /// A linear gradient from `start` to `end`, or an error unless the points
    /// are finite and distinct and there are 2 to 1,024 stops with offsets
    /// from 0 to 1 in order.
    pub fn linear(
        start: Point,
        end: Point,
        stops: impl Into<Arc<[GradientStop]>>,
    ) -> Result<Self, &'static str> {
        let stops = stops.into();
        if ![start.x, start.y, end.x, end.y]
            .iter()
            .all(|v| v.is_finite())
            || start == end
            || stops.len() < 2
            || stops.len() > 1024
            || stops
                .iter()
                .any(|s| !s.offset.is_finite() || !(0. ..=1.).contains(&s.offset))
            || stops.windows(2).any(|s| s[0].offset > s[1].offset)
        {
            return Err("invalid linear gradient");
        }
        Ok(Self::Linear { start, end, stops })
    }
}
/// Which parts of a self-overlapping path are inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    /// Inside where the path winds round a nonzero number of times (SVG's
    /// `nonzero`).
    #[default]
    Winding,
    /// Inside where a ray crosses the path an odd number of times.
    EvenOdd,
}
/// How a stroke's open ends look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    /// Cut square at the end.
    #[default]
    Butt,
    /// Rounded past the end by half the width.
    Round,
    /// Squared off past the end by half the width.
    Square,
}
/// How a stroke turns its corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    /// A sharp point, beveled past `Stroke::miter_limit`.
    #[default]
    Miter,
    /// Rounded.
    Round,
    /// Cut off flat.
    Bevel,
}
/// How a path is stroked.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// The line's width in logical points.
    pub width: f32,
    /// Its open ends.
    pub cap: LineCap,
    /// Its corners.
    pub join: LineJoin,
    /// How long a miter may be, in line widths, before it bevels.
    pub miter_limit: f32,
    /// Dash and gap lengths, alternating; empty draws a solid line.
    pub dash: Vec<f32>,
    /// How far into the dash pattern the line starts.
    pub dash_offset: f32,
}
impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.,
            dash: vec![],
            dash_offset: 0.,
        }
    }
}
/// One canvas drawing command.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    /// Clip later commands to `path` until the matching `Restore`.
    Clip {
        /// The clip's shape.
        path: Path,
        /// Which parts of it are inside.
        rule: FillRule,
    },
    /// End the most recent clip.
    Restore,
    /// Fill `path` with `brush`.
    Fill {
        /// The shape.
        path: Path,
        /// What fills it.
        brush: Brush,
        /// Which parts of it are inside.
        rule: FillRule,
    },
    /// Stroke `path` with `brush`.
    Stroke {
        /// The shape.
        path: Path,
        /// What the line is painted with.
        brush: Brush,
        /// The line's width, ends, corners and dashes.
        style: Stroke,
    },
}
/// Equal command lists preserve pixels, uploads and retained layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Canvas {
    commands: Vec<Draw>,
}
impl Canvas {
    /// An empty canvas.
    pub fn new() -> Self {
        Self::default()
    }
    /// Fill `path` with `brush`, by the nonzero rule.
    pub fn fill(&mut self, path: Path, brush: impl Into<Brush>) -> &mut Self {
        self.fill_rule(path, brush, FillRule::Winding)
    }
    /// Fill `path` with `brush`, by `rule`.
    pub fn fill_rule(&mut self, path: Path, brush: impl Into<Brush>, rule: FillRule) -> &mut Self {
        self.commands.push(Draw::Fill {
            path,
            brush: brush.into(),
            rule,
        });
        self
    }
    /// Stroke `path` with `brush` and `style`.
    pub fn stroke(&mut self, path: Path, brush: impl Into<Brush>, style: Stroke) -> &mut Self {
        self.commands.push(Draw::Stroke {
            path,
            brush: brush.into(),
            style,
        });
        self
    }
    /// Clip the commands after this to `path` until `restore`.
    pub fn clip(&mut self, path: Path, rule: FillRule) -> &mut Self {
        self.commands.push(Draw::Clip { path, rule });
        self
    }
    /// End the most recent clip.
    pub fn restore(&mut self) -> &mut Self {
        self.commands.push(Draw::Restore);
        self
    }
    /// The commands, in order.
    pub fn commands(&self) -> &[Draw] {
        &self.commands
    }
}
