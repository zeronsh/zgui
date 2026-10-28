//! Retained scene graph. Mutations invalidate only their layout/paint/compositor dependencies.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_SCENE_ID: AtomicU64 = AtomicU64::new(1);

/// A node in a [`Scene`]. Generational: once its node is removed, an ID
/// never names the node that reuses its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}
impl NodeId {
    /// Its number, as diagnostics and devtools name it (`@12`): no other
    /// node in the scene has it while this one is there.
    pub fn number(self) -> u32 {
        self.index
    }
    /// Its slot: no other node in the scene has it while this one is there.
    pub(crate) fn slot(self) -> usize {
        self.index as usize
    }
}
/// What measuring a container works in, per child, kept for the next one
/// (`Scene::scratch`): laying out allocates none of it once warm.
#[derive(Default)]
struct Scratch {
    ids: Vec<NodeId>,
    absolute: Vec<NodeId>,
    asked: Vec<Constraints>,
    /// `asked` as each child's results are kept under it (`relevant`).
    kept: Vec<Constraints>,
    sizes: Vec<(f32, f32)>,
    allocated: Vec<bool>,
    bases: Vec<f32>,
    frozen: Vec<bool>,
    floors: Vec<Option<f32>>,
    weights: Vec<f32>,
    margins: Vec<Insets>,
}
/// A map keyed by node. Ids are small integers with a generation: a
/// multiplicative hash beats SipHash on them, and mounting or removing a
/// node touches several such maps.
pub(crate) type NodeMap<V> =
    std::collections::HashMap<NodeId, V, std::hash::BuildHasherDefault<NodeHasher>>;
#[derive(Default, Clone, Copy)]
pub(crate) struct NodeHasher(u64);
impl std::hash::Hasher for NodeHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u32(&mut self, value: u32) {
        self.0 = (self.0.rotate_left(32) ^ u64::from(value)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}
/// Separate damage rectangles a flush keeps before marking damage on a grid
/// of `DAMAGE_CELL` cells instead (see `DamageGrid`).
const DAMAGE_RECTS: usize = 64;
/// The side of a damage grid cell, in viewport units.
const DAMAGE_CELL: f32 = 8.;
/// The most words a damage grid takes (512 KiB: a viewport 32,768 points a
/// side); past it damage joins into one rectangle instead.
const DAMAGE_GRID_WORDS: usize = 1 << 16;
/// Damage past `DAMAGE_RECTS` separate places (a busy dashboard changes
/// hundreds a frame): marking cells costs the same however many there are,
/// where merging into a list scans it each time. Rectangles again at flush.
/// A row's cells are bits, so a rectangle marks each row's run in a word or
/// two rather than a cell at a time.
struct DamageGrid {
    columns: usize,
    rows: usize,
    /// Words per row.
    stride: usize,
    cells: Vec<u64>,
    /// The first word not yet full, moved past full ones as cells are
    /// marked (`mark`): marks only add cells, so the words before it stay
    /// full. The bits past the last column are set from the start, so a
    /// row's last word fills as the others.
    full_before: usize,
}
impl DamageGrid {
    /// A grid over `viewport`; `None` past `DAMAGE_GRID_WORDS` (a viewport
    /// of millions of points a side).
    fn new(viewport: Rect) -> Option<Self> {
        let columns = (viewport.width / DAMAGE_CELL).ceil().max(1.) as usize;
        let rows = (viewport.height / DAMAGE_CELL).ceil().max(1.) as usize;
        let stride = columns.div_ceil(64);
        let words = stride
            .checked_mul(rows)
            .filter(|&words| words <= DAMAGE_GRID_WORDS)?;
        let mut cells = vec![0; words];
        if !columns.is_multiple_of(64) {
            for row in 0..rows {
                cells[(row + 1) * stride - 1] = u64::MAX << (columns % 64);
            }
        }
        Some(Self {
            columns,
            rows,
            stride,
            cells,
            full_before: 0,
        })
    }
    /// The cells `rect` touches: columns and rows, end exclusive.
    fn span(&self, rect: Rect) -> (usize, usize, usize, usize) {
        let first = |value: f32| (value / DAMAGE_CELL).floor().max(0.) as usize;
        let end =
            |value: f32, count: usize| ((value / DAMAGE_CELL).ceil().max(0.) as usize).min(count);
        (
            first(rect.x).min(self.columns),
            first(rect.y).min(self.rows),
            end(rect.x + rect.width, self.columns),
            end(rect.y + rect.height, self.rows),
        )
    }
    /// The words of a row holding columns `left..right` (`left < right`),
    /// and the bits of those columns in the first and last of them.
    fn masks(left: usize, right: usize) -> (usize, usize, u64, u64) {
        let (first, last) = (left / 64, (right - 1) / 64);
        let head = u64::MAX << (left % 64);
        let tail = u64::MAX >> (63 - (right - 1) % 64);
        match first == last {
            true => (first, last, head & tail, head & tail),
            false => (first, last, head, tail),
        }
    }
    fn mark(&mut self, rect: Rect) {
        self.mark_span(self.span(rect));
        self.passed_full();
    }
    /// Move `full_before` past the words marks filled: one look a mark, so
    /// `full` is a comparison, asked as most damage is added
    /// (`damage_is_full`).
    #[inline(always)]
    fn passed_full(&mut self) {
        while self.cells.get(self.full_before) == Some(&u64::MAX) {
            self.full_before += 1;
        }
    }
    /// Mark the cells `span` (columns then rows, end exclusive) covers.
    #[inline(always)]
    fn mark_span(&mut self, (left, top, right, bottom): (usize, usize, usize, usize)) {
        if left >= right {
            return;
        }
        let (first, last, head, tail) = Self::masks(left, right);
        // Most damage is narrower than a word: a bar, a value.
        if first == last {
            for row in top..bottom {
                let cell = &mut self.cells[row * self.stride + first];
                *cell |= head;
            }
            return;
        }
        for row in top..bottom {
            let cells = &mut self.cells[row * self.stride + first..=row * self.stride + last];
            let count = cells.len();
            for (index, cell) in cells.iter_mut().enumerate() {
                let bits = match index {
                    0 => head,
                    _ if index + 1 == count => tail,
                    _ => u64::MAX,
                };
                *cell |= bits;
            }
        }
    }
    /// A moved leaf's two painted areas (`leaf_damage::<true>`), clipped but
    /// not rounded, marked as `mark` would mark them rounded to whole
    /// points: straight to cells, with no rounded rectangles, union or
    /// second clip in between. Two spans sharing their extent along an axis
    /// and overlapping are marked as one. The viewport is 1 to 2^22 points
    /// on each axis (`Scene::grid_takes_cells`), where whole-point edges are
    /// exact in f32; a far edge is rounded up before it is divided, so an
    /// area thinner than a point still covers its cell.
    #[inline(never)]
    fn mark_clipped_pair(&mut self, before: Option<Rect>, after: Option<Rect>) {
        let span = |rect: Rect| {
            let first = |value: f32, count: usize| ((value / DAMAGE_CELL) as usize).min(count);
            let end = |value: f32, count: usize| {
                (value.ceil() as usize)
                    .div_ceil(DAMAGE_CELL as usize)
                    .min(count)
            };
            (
                first(rect.x, self.columns),
                first(rect.y, self.rows),
                end(rect.x + rect.width, self.columns),
                end(rect.y + rect.height, self.rows),
            )
        };
        match (before.map(span), after.map(span)) {
            (Some((l, t, r, b)), Some((ll, tt, rr, bb)))
                if (l == ll && r == rr && t <= bb && tt <= b)
                    || (t == tt && b == bb && l <= rr && ll <= r) =>
            {
                self.mark_span((l.min(ll), t.min(tt), r.max(rr), b.max(bb)));
            }
            (before, after) => {
                for span in [before, after].into_iter().flatten() {
                    self.mark_span(span);
                }
            }
        }
        self.passed_full();
    }
    fn touches(&self, rect: Rect) -> bool {
        let (left, top, right, bottom) = self.span(rect);
        if left >= right {
            return false;
        }
        let (first, last, head, tail) = Self::masks(left, right);
        (top..bottom).any(|row| {
            let cells = &self.cells[row * self.stride + first..=row * self.stride + last];
            let count = cells.len();
            cells.iter().enumerate().any(|(index, cell)| {
                let bits = match index {
                    0 => head,
                    _ if index + 1 == count => tail,
                    _ => u64::MAX,
                };
                cell & bits != 0
            })
        })
    }
    fn full(&self) -> bool {
        self.full_before == self.cells.len()
    }
    /// Each row's runs of marked cells, a run repeated straight down the
    /// next rows joining them as one rectangle.
    #[inline(never)]
    fn rects(&self, viewport: Rect) -> Vec<Rect> {
        let mut done = Vec::new();
        // Runs still growing down: first and end column, first row.
        let (mut open, mut next) = (Vec::<(usize, usize, usize)>::new(), Vec::new());
        for row in 0..=self.rows {
            let columns = if row < self.rows { self.columns } else { 0 };
            let mut close = |&(left, right, top): &(usize, usize, usize)| {
                let cell = |index: usize| index as f32 * DAMAGE_CELL;
                let rect = Rect::new(cell(left), cell(top), cell(right - left), cell(row - top));
                done.extend(rect.intersection(viewport));
            };
            // Both lists are ordered. Close each old run as soon as no
            // following run can extend it, keeping the old emission order.
            let (mut column, mut at) = (0, 0);
            let mut word = if columns == 0 {
                0
            } else {
                self.cells[row * self.stride]
            };
            while column < columns {
                while word == 0 {
                    column = (column / 64 + 1) * 64;
                    if column >= columns {
                        break;
                    }
                    word = self.cells[row * self.stride + column / 64];
                }
                if column >= columns {
                    break;
                }
                let gap = word.trailing_zeros() as usize;
                column += gap;
                if column >= columns {
                    break;
                }
                word >>= gap;
                let start = column;
                loop {
                    let ones = word.trailing_ones() as usize;
                    let boundary = (column / 64 + 1) * 64;
                    if column + ones < boundary {
                        column = (column + ones).min(columns);
                        word >>= ones;
                        break;
                    }
                    column = boundary;
                    if column >= columns {
                        column = columns;
                        word = 0;
                        break;
                    }
                    word = self.cells[row * self.stride + column / 64];
                }
                while at < open.len() && open[at].0 < start {
                    close(&open[at]);
                    at += 1;
                }
                let top = match open.get(at) {
                    Some(&(left, right, top)) if (left, right) == (start, column) => {
                        at += 1;
                        top
                    }
                    _ => row,
                };
                next.push((start, column, top));
            }
            for run in &open[at..] {
                close(run);
            }
            std::mem::swap(&mut open, &mut next);
            next.clear();
        }
        done
    }
}
// Signed far edges make all four containment comparisons point the same way.
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
struct DamageEdges([f32; 4]);
impl From<Rect> for DamageEdges {
    #[inline]
    fn from(rect: Rect) -> Self {
        Self([
            rect.x,
            rect.y,
            -(rect.x + rect.width),
            -(rect.y + rect.height),
        ])
    }
}

/// An axis-aligned rectangle in logical points, y down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Its left edge.
    pub x: f32,
    /// Its top edge.
    pub y: f32,
    /// Its width.
    pub width: f32,
    /// Its height.
    pub height: f32,
}
impl Rect {
    /// The rectangle at `x`, `y`, `width` by `height`.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    /// Whether the two overlap by more than an edge.
    pub fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
    /// Where the two overlap, if they do by more than an edge.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let w = (self.x + self.width).min(other.x + other.width) - x;
        let h = (self.y + self.height).min(other.y + other.height) - y;
        (w > 0.0 && h > 0.0).then(|| Self::new(x, y, w, h))
    }
    /// The smallest rectangle holding both.
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            (self.x + self.width).max(other.x + other.width) - x,
            (self.y + self.height).max(other.y + other.height) - y,
        )
    }
    /// Grown by `by` on every side (shrunk when negative).
    pub fn expand(self, by: f32) -> Self {
        Self::new(
            self.x - by,
            self.y - by,
            self.width + 2.0 * by,
            self.height + 2.0 * by,
        )
    }
    fn translated(self, t: Transform) -> Self {
        Self::new(self.x + t.x, self.y + t.y, self.width, self.height)
    }
}
/// A colour: red, green, blue and alpha, each 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);
/// A translation, in logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    /// Across.
    pub x: f32,
    /// Down.
    pub y: f32,
}
/// Scale, rotation and skew about an origin, applied after layout and on
/// top of the node's translation (`Transform`). Identity by default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTransform {
    /// Across and down, about `origin`; 1 leaves it.
    pub scale: (f32, f32),
    /// Radians, clockwise on screen (positive Y down).
    pub rotate: f32,
    /// Radians, along x then y, as CSS `skew(x, y)`.
    pub skew: (f32, f32),
    /// Radians about the horizontal axis through `origin`, as CSS
    /// `rotateX`: positive tips the top away.
    pub rotate_x: f32,
    /// Radians about the vertical axis through `origin`, as CSS `rotateY`:
    /// positive turns the right side away.
    pub rotate_y: f32,
    /// How far the eye is from the plane, in logical points, as CSS
    /// `perspective()`: a tilted side nearer the eye shows larger. 0 has no
    /// perspective (a tilt then only narrows the node).
    pub perspective: f32,
    /// The pivot as a fraction of the node's own laid-out size; (0.5, 0.5) is its centre (CSS default).
    pub origin: (f32, f32),
}
impl Default for LocalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl LocalTransform {
    /// No scale, rotation or skew, about the centre.
    pub const IDENTITY: Self = Self {
        scale: (1., 1.),
        rotate: 0.,
        skew: (0., 0.),
        rotate_x: 0.,
        rotate_y: 0.,
        perspective: 0.,
        origin: (0.5, 0.5),
    };
    /// Whether it moves nothing, whatever its origin (and perspective, which
    /// moves nothing that isn't tilted).
    pub fn is_identity(&self) -> bool {
        self.scale == (1., 1.)
            && self.rotate == 0.
            && self.skew == (0., 0.)
            && self.rotate_x == 0.
            && self.rotate_y == 0.
    }
    /// Its scale, rotation and skew about the origin for a node at `bounds`
    /// (layout rect, untranslated), as an affine matrix: translate(origin) ·
    /// rotate · skew · scale · translate(-origin). Leaves out a tilt
    /// (`rotate_x`, `rotate_y`), which `projective` includes.
    pub fn matrix(&self, bounds: Rect) -> crate::affine::Affine {
        use crate::affine::Affine;
        let skew = Affine {
            b: self.skew.1.tan(),
            c: self.skew.0.tan(),
            ..Affine::IDENTITY
        };
        Affine::scale(self.scale.0, self.scale.1)
            .then(skew)
            .then(Affine::rotation(self.rotate))
            .around(
                bounds.x + bounds.width * self.origin.0,
                bounds.y + bounds.height * self.origin.1,
            )
    }
    /// The map for a node at `bounds` (layout rect, untranslated), as CSS
    /// `perspective() rotateX() rotateY() rotate() skew() scale()` about the
    /// origin: projective once tilted with a perspective, affine otherwise.
    pub fn projective(&self, bounds: Rect) -> crate::affine::Projective {
        use crate::affine::{Affine, Projective};
        let skew = Affine {
            b: self.skew.1.tan(),
            c: self.skew.0.tan(),
            ..Affine::IDENTITY
        };
        let flat = Affine::scale(self.scale.0, self.scale.1)
            .then(skew)
            .then(Affine::rotation(self.rotate));
        let (ox, oy) = (
            bounds.x + bounds.width * self.origin.0,
            bounds.y + bounds.height * self.origin.1,
        );
        let mut map = Projective::from(Affine::translation(-ox, -oy).then(flat));
        if self.rotate_x != 0. || self.rotate_y != 0. {
            // The plane (z = 0) turned about Y, then X, then seen from
            // `perspective` away: depth z divides as w = 1 - z / perspective.
            let (sx, cx) = self.rotate_x.sin_cos();
            let (sy, cy) = self.rotate_y.sin_cos();
            let depth = if self.perspective > 0. {
                1. / self.perspective
            } else {
                0.
            };
            let tilt = Projective {
                a: cy,
                b: sx * sy,
                c: 0.,
                d: cx,
                tx: 0.,
                ty: 0.,
                px: cx * sy * depth,
                py: -sx * depth,
                pw: 1.,
            };
            map = map.then(tilt);
        }
        map.then(Projective::from(Affine::translation(ox, oy)))
    }
    /// Each nonfinite component takes its identity value, so equal writes
    /// compare equal (NaN never would).
    pub(crate) fn normalized(self) -> Self {
        let finite = |value: f32, default: f32| if value.is_finite() { value } else { default };
        Self {
            scale: (finite(self.scale.0, 1.), finite(self.scale.1, 1.)),
            rotate: finite(self.rotate, 0.),
            skew: (finite(self.skew.0, 0.), finite(self.skew.1, 0.)),
            rotate_x: finite(self.rotate_x, 0.),
            rotate_y: finite(self.rotate_y, 0.),
            perspective: finite(self.perspective, 0.).max(0.),
            origin: (finite(self.origin.0, 0.5), finite(self.origin.1, 0.5)),
        }
    }
    /// The larger axis scale: how much bigger than laid out its content shows.
    pub fn magnification(&self) -> f32 {
        self.scale.0.abs().max(self.scale.1.abs())
    }
}
/// A transformed or filtered ancestor's mapping of its layer's content onto
/// the viewport, how far a content blur spreads it there, and the viewport
/// clip around that layer (see `Scene::descend`).
#[derive(Clone, Copy)]
struct Placement {
    matrix: crate::affine::Projective,
    /// Viewport units: what a layer's content paints shows this much past
    /// its mapped box (nested blurs, each stretched by the matrices after it).
    spread: f32,
    clip: Rect,
}
impl Placement {
    /// Where pixels `rect` paints inside the layer can change on screen.
    fn shows(&self, rect: Rect) -> Option<Rect> {
        let mapped = self.matrix.bounds(rect);
        // Resampled (scaled, rotated off the right angles, skewed), its
        // filtering reaches up to a pixel past the mapped box.
        let zero = |v: f32| v.abs() < 1e-4;
        let unit = |v: f32| (v.abs() - 1.).abs() < 1e-4;
        // In perspective, always.
        let resampled = self.matrix.affine().is_none_or(|m| {
            !((unit(m.a) && zero(m.b) && zero(m.c) && unit(m.d))
                || (zero(m.a) && unit(m.b) && unit(m.c) && zero(m.d)))
        });
        let spread = self.spread.max(0.) + if resampled { 1. } else { 0. };
        let mapped = if spread > 0. {
            mapped.expand(spread)
        } else {
            mapped
        };
        mapped.intersection(self.clip)
    }
}
/// How far a content blur of `sigma` spreads pixels: three standard
/// deviations, past which the Gaussian leaves under half an 8-bit step.
pub fn filter_spread(sigma: f32) -> f32 {
    3. * sigma
}
/// How far `matrix` moves a point that is one unit away, at most along an
/// axis: a spread inside a layer times this is its spread outside.
/// `map`, then a zoom of `zoom` about `bounds`' top-left
/// (`Node::projective`). Out of line: inlined, it takes registers from the
/// hot walks that reach `projective` (`hit_test_all`).
#[cold]
#[inline(never)]
fn then_zoom(map: crate::affine::Projective, bounds: Rect, zoom: f32) -> crate::affine::Projective {
    map.then(zoom_about(bounds, zoom).into())
}
/// A zoom of `zoom` about `bounds`' top-left (`Node::projective`).
fn zoom_about(bounds: Rect, zoom: f32) -> crate::affine::Affine {
    crate::affine::Affine {
        a: zoom,
        b: 0.,
        c: 0.,
        d: zoom,
        tx: bounds.x * (1. - zoom),
        ty: bounds.y * (1. - zoom),
    }
}
fn stretch(matrix: crate::affine::Projective) -> f32 {
    let linear = crate::affine::Projective {
        tx: 0.,
        ty: 0.,
        ..matrix
    };
    let unit = linear.bounds(Rect::new(-1., -1., 2., 2.));
    (unit.width.max(unit.height) / 2.).max(0.)
}
/// A clip inset (CSS `clip-path: inset(top right bottom left)`): the node
/// and its descendants paint only inside its box inset by these lengths,
/// pixels or fractions of its own laid-out size (`Length::Percent(1.)` is
/// 100%: top and bottom of its height, left and right of its width,
/// following it as it resizes). Negative insets reach past the box, so a
/// shadow or ring isn't cut. `Auto` is 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipInset {
    /// From the top edge.
    pub top: crate::layout::Length,
    /// From the right edge.
    pub right: crate::layout::Length,
    /// From the bottom edge.
    pub bottom: crate::layout::Length,
    /// From the left edge.
    pub left: crate::layout::Length,
}
impl ClipInset {
    /// All four sides in pixels.
    pub fn px(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        use crate::layout::Length::Px;
        Self {
            top: Px(top),
            right: Px(right),
            bottom: Px(bottom),
            left: Px(left),
        }
    }
    /// The box `bounds` inset by these lengths; empty when they cross.
    pub fn apply(&self, bounds: Rect) -> Rect {
        let length = |length: crate::layout::Length, basis: f32| match length {
            crate::layout::Length::Px(value) if value.is_finite() => value,
            crate::layout::Length::Percent(fraction) if fraction.is_finite() => fraction * basis,
            _ => 0.,
        };
        let (top, bottom) = (
            length(self.top, bounds.height),
            length(self.bottom, bounds.height),
        );
        let (left, right) = (
            length(self.left, bounds.width),
            length(self.right, bounds.width),
        );
        Rect::new(
            bounds.x + left,
            bounds.y + top,
            (bounds.width - left - right).max(0.),
            (bounds.height - top - bottom).max(0.),
        )
    }
}
/// An alpha mask over a node (CSS `mask-image` with `mask-position` and
/// `mask-size`, no repeat): its subtree, rendered as a layer and after any
/// `Effects::filter_blur`, shows by the image's alpha, sampled nearest. The
/// image is drawn `size` large with its top-left `offset` from the node's
/// laid-out origin, in the node's own (untransformed) space; outside it
/// nothing shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageMask {
    /// The image whose alpha shows the subtree.
    pub image: Arc<crate::image::ImageData>,
    /// Its top-left corner, from the node's laid-out origin.
    pub offset: (f32, f32),
    /// Its drawn width and height.
    pub size: (f32, f32),
}
impl ImageMask {
    /// Nonfinite components zeroed, so equal writes compare equal.
    fn normalized(mut self) -> Self {
        let finite = |value: f32| if value.is_finite() { value } else { 0. };
        self.offset = (finite(self.offset.0), finite(self.offset.1));
        self.size = (finite(self.size.0).max(0.), finite(self.size.1).max(0.));
        self
    }
}
/// How a node's subtree mixes with what lies beneath it as it composites,
/// as CSS `mix-blend-mode` (`Scene::set_blend`): the subtree paints into a
/// cached layer, and each of its pixels combines with the backdrop under it
/// (what its parent's layer, or the window, holds there) by the mode's
/// formula, as the W3C Compositing and Blending spec defines them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlendMode {
    /// Over the backdrop, as any node composites.
    #[default]
    Normal,
    /// Darkens: the colours multiplied.
    Multiply,
    /// Lightens: the inverse of multiplying the inverses.
    Screen,
    /// Multiply or screen, by the backdrop: contrast up, its highlights and shadows kept.
    Overlay,
    /// The darker of each channel.
    Darken,
    /// The lighter of each channel.
    Lighten,
    /// Brightens the backdrop toward the source.
    ColorDodge,
    /// Darkens the backdrop toward the source.
    ColorBurn,
    /// Multiply or screen, by the source: as overlay with the two swapped.
    HardLight,
    /// A gentler hard light: darkens or lightens by the source.
    SoftLight,
    /// The absolute difference.
    Difference,
    /// Difference with lower contrast.
    Exclusion,
    /// The source's hue, the backdrop's saturation and luminosity.
    Hue,
    /// The source's saturation, the backdrop's hue and luminosity.
    Saturation,
    /// The source's hue and saturation, the backdrop's luminosity: tints.
    Color,
    /// The source's luminosity, the backdrop's hue and saturation.
    Luminosity,
}
impl BlendMode {
    /// Every mode, in `as u8` order.
    pub const ALL: [Self; 16] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
    /// Its CSS name (`"soft-light"`).
    pub fn name(self) -> &'static str {
        [
            "normal",
            "multiply",
            "screen",
            "overlay",
            "darken",
            "lighten",
            "color-dodge",
            "color-burn",
            "hard-light",
            "soft-light",
            "difference",
            "exclusion",
            "hue",
            "saturation",
            "color",
            "luminosity",
        ][self as usize]
    }
    /// The mode a CSS name names (`"soft-light"`, or `"soft_light"`).
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.replace('_', "-");
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
    /// `B(cb, cs)`: the backdrop's straight colour `cb` and the source's
    /// `cs` (0 to 1) mixed by this mode, before compositing (as blend.wgsl
    /// does on the GPU).
    pub fn mix(self, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
        fn each(cb: [f32; 3], cs: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3] {
            [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])]
        }
        fn hard_light(cb: f32, cs: f32) -> f32 {
            if cs <= 0.5 {
                cb * 2. * cs
            } else {
                let s = 2. * cs - 1.;
                cb + s - cb * s
            }
        }
        fn lum(c: [f32; 3]) -> f32 {
            0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
        }
        fn clip_color(c: [f32; 3]) -> [f32; 3] {
            let l = lum(c);
            let n = c[0].min(c[1]).min(c[2]);
            let x = c[0].max(c[1]).max(c[2]);
            let mut c = c;
            if n < 0. {
                c = c.map(|v| l + (v - l) * l / (l - n).max(1e-6));
            }
            if x > 1. {
                c = c.map(|v| l + (v - l) * (1. - l) / (x - l).max(1e-6));
            }
            c
        }
        fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
            let d = l - lum(c);
            clip_color(c.map(|v| v + d))
        }
        fn sat(c: [f32; 3]) -> f32 {
            c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
        }
        fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
            let n = c[0].min(c[1]).min(c[2]);
            let x = c[0].max(c[1]).max(c[2]);
            if x <= n {
                return [0.; 3];
            }
            c.map(|v| (v - n) * s / (x - n))
        }
        let mixed = match self {
            Self::Normal => cs,
            Self::Multiply => each(cb, cs, |b, s| b * s),
            Self::Screen => each(cb, cs, |b, s| b + s - b * s),
            Self::Overlay => each(cb, cs, |b, s| hard_light(s, b)),
            Self::Darken => each(cb, cs, f32::min),
            Self::Lighten => each(cb, cs, f32::max),
            Self::ColorDodge => each(cb, cs, |b, s| match () {
                _ if b <= 0. => 0.,
                _ if s >= 1. => 1.,
                _ => (b / (1. - s)).min(1.),
            }),
            Self::ColorBurn => each(cb, cs, |b, s| match () {
                _ if b >= 1. => 1.,
                _ if s <= 0. => 0.,
                _ => 1. - ((1. - b) / s).min(1.),
            }),
            Self::HardLight => each(cb, cs, hard_light),
            Self::SoftLight => each(cb, cs, |b, s| {
                if s <= 0.5 {
                    b - (1. - 2. * s) * b * (1. - b)
                } else {
                    let d = if b <= 0.25 {
                        ((16. * b - 12.) * b + 4.) * b
                    } else {
                        b.sqrt()
                    };
                    b + (2. * s - 1.) * (d - b)
                }
            }),
            Self::Difference => each(cb, cs, |b, s| (b - s).abs()),
            Self::Exclusion => each(cb, cs, |b, s| b + s - 2. * b * s),
            Self::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
            Self::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
            Self::Color => set_lum(cs, lum(cb)),
            Self::Luminosity => set_lum(cb, lum(cs)),
        };
        mixed.map(|v| v.clamp(0., 1.))
    }
}
/// A shader a node's subtree composites through (`Scene::set_layer_shader`):
/// the subtree, painted once as a cached layer, has each of its pixels
/// passed to the shader's `layer` as it is drawn, so changing the
/// uniforms composites again without repainting the subtree. The shader is
/// a `FillShader` whose source defines
///
/// ```wgsl
/// fn layer(input: LayerInput) -> vec4<f32>
/// ```
///
/// returning a straight-alpha colour. `LayerInput` holds `color` (the
/// layer's straight-alpha pixel there), `p` (the pixel's logical position
/// from the node's top-left, in its own untransformed space), `size` (the
/// node's logical size), `time`, `scale` and `params` as `FillInput` has
/// them. It may call `zgx_sample(p)` for the layer's pixel at another
/// logical position. It runs over the node's box, which its ink then always
/// covers: the layer past the box composites as painted. Applied after any
/// content blur and mask, under the node's opacity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerShader {
    /// The shader; `None` while only uniforms are set.
    pub shader: Option<Arc<crate::decoration::FillShader>>,
    /// Its time and parameters.
    pub uniforms: crate::decoration::ShaderUniforms,
}
/// A clip that cuts nothing, as `clipped_bounds` starts from.
const UNCLIPPED: Rect = Rect {
    x: -f32::MAX / 4.,
    y: -f32::MAX / 4.,
    width: f32::MAX / 2.,
    height: f32::MAX / 2.,
};
/// How a node composites: its opacity, blurs, filters and edge fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    /// Opacity, 0 to 1, multiplied down the tree.
    pub opacity: f32,
    /// Backdrop blur (CSS `backdrop-filter: blur()`), the Gaussian sigma in
    /// logical pixels: what lies behind the node, inside its bounds, blurs.
    /// Its own content stays sharp.
    pub blur_radius: f32,
    /// Fade the top and bottom edges out over this many logical points.
    pub edge_fade: f32,
    /// Content blur (CSS `filter: blur()`), the Gaussian sigma in logical
    /// pixels: the node's own subtree, painted once as a cached layer,
    /// composites blurred, spreading `filter_spread` past its bounds. Opacity
    /// applies to the blurred result. Changing it composites again without
    /// repainting the subtree; at 0 the node paints as any other.
    pub filter_blur: f32,
    /// CSS `filter: brightness()`: the subtree's colours scaled (1 leaves
    /// them, 0 is black, past 1 brighter), as a group, under its opacity.
    pub brightness: f32,
    /// CSS `filter: contrast()`: colours pushed from (or towards) mid grey.
    pub contrast: f32,
    /// CSS `filter: saturate()`: 0 is greyscale, past 1 more vivid.
    pub saturate: f32,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            blur_radius: 0.0,
            edge_fade: 0.0,
            filter_blur: 0.0,
            brightness: 1.0,
            contrast: 1.0,
            saturate: 1.0,
        }
    }
}
/// An affine map of straight-alpha RGB: each row the weights of R, G, B
/// and an offset. Brightness, contrast and saturate filters compose into
/// one; being affine, it can be applied to each primitive of a subtree
/// before blending with the same result as to the subtree as a group.
pub type ColorMatrix = [[f32; 4]; 3];
impl Effects {
    /// Its colour filters as one matrix (brightness, then contrast, then
    /// saturate, as CSS lists them), or `None` when they change nothing.
    pub fn color_matrix(&self) -> Option<ColorMatrix> {
        if (self.brightness, self.contrast, self.saturate) == (1., 1., 1.) {
            return None;
        }
        let (b, k, s) = (self.brightness, self.contrast, self.saturate);
        // Contrast after brightness: (b·c - 0.5)·k + 0.5.
        let (scale, offset) = (b * k, 0.5 - 0.5 * k);
        // CSS's saturate matrix (Rec. 709 luma weights).
        let saturate = [
            [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
            [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
            [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
        ];
        Some(saturate.map(|row| {
            [
                row[0] * scale,
                row[1] * scale,
                row[2] * scale,
                (row[0] + row[1] + row[2]) * offset,
            ]
        }))
    }
}
/// `outer` applied after `inner`: a filtered subtree inside a filtered one.
pub fn compose_color(
    outer: Option<ColorMatrix>,
    inner: Option<ColorMatrix>,
) -> Option<ColorMatrix> {
    let (outer, inner) = match (outer, inner) {
        (None, inner) => return inner,
        (outer, None) => return outer,
        (Some(outer), Some(inner)) => (outer, inner),
    };
    Some(std::array::from_fn(|row| {
        let o = outer[row];
        let column = |c: usize| o[0] * inner[0][c] + o[1] * inner[1][c] + o[2] * inner[2][c];
        [column(0), column(1), column(2), column(3) + o[3]]
    }))
}
/// How a container lays out its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Side by side, from the start of the line.
    Row,
    /// One under another.
    Column,
    /// Children stacked in the whole content box, as CSS grid lays out items
    /// sharing one area: each sized against it and aligned in it on its own,
    /// across by `Style::align` and down by `Style::justify` (`Stretch` on
    /// either fills that axis unless the child sizes itself there). Its size
    /// is its largest in-flow child's.
    Overlay,
}
/// A rectangular outer shadow. Blur radius is the Gaussian sigma in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    /// The shadow's colour.
    pub color: Color,
    /// How far it is moved from the box.
    pub offset: Transform,
    /// The blur's Gaussian sigma, in logical points.
    pub blur_radius: f32,
    /// How far it is grown past the box before blurring.
    pub spread: f32,
}
/// How a box paints: its fill, border, corners, shadow and decoration.
#[derive(Clone, Debug, PartialEq)]
pub struct QuadStyle {
    /// The fill colour.
    pub fill: Color,
    /// The corner radius, for all four corners.
    pub radius: f32,
    /// The border's colour.
    pub border_color: Color,
    /// The border's width on every side.
    pub border_width: f32,
    /// One shadow under the box.
    pub shadow: Option<BoxShadow>,
    /// What the plain fields can't say: per-side borders and corners,
    /// gradients, images, shaders, more shadows.
    pub decoration: Option<Arc<crate::decoration::Decoration>>,
    /// Paint the border over the node's descendants (an inset stroke, as a
    /// CSS outline drawn inside): the fill paints first, the border after
    /// everything inside (`PaintItem::border_pass`).
    pub border_over: bool,
    /// An outline around the box (CSS `outline` and `outline-offset`).
    pub outline: Option<Outline>,
}
/// An outline (CSS `outline`, `outline-offset`): a ring `width` wide drawn
/// `offset` outside the box (inside it when negative), its corners the
/// box's radii grown by as much, over the node's descendants. It takes no
/// room in layout. A design tool's stroke aligned outside is offset 0;
/// centred, offset `-width / 2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    /// The ring's width.
    pub width: f32,
    /// Its colour.
    pub color: Color,
    /// How far outside the box it starts; negative draws over the box.
    pub offset: f32,
}
impl Outline {
    /// Whether it draws anything.
    pub fn visible(&self) -> bool {
        self.width.is_finite() && self.width > 0. && self.offset.is_finite() && self.color.3 > 0
    }
    /// The box it rings for a node at `bounds`: its outer edge.
    pub fn outer(&self, bounds: Rect) -> Rect {
        bounds.expand(self.offset + self.width)
    }
}
impl Default for QuadStyle {
    fn default() -> Self {
        Self {
            fill: Color(0, 0, 0, 0),
            radius: 0.,
            border_color: Color(0, 0, 0, 0),
            border_width: 0.,
            shadow: None,
            decoration: None,
            border_over: false,
            outline: None,
        }
    }
}
impl QuadStyle {
    /// Its border's widths (top, right, bottom, left): per side from the
    /// decoration, or `border_width` all round.
    pub fn border_sides(&self) -> Insets {
        self.decoration
            .as_ref()
            .and_then(|detail| detail.border_widths)
            .unwrap_or(Insets::all(self.border_width))
    }
    /// The room its border takes in layout (top, right, bottom, left), as
    /// CSS's `box-sizing: border-box` has it: a box's size includes its
    /// border, and its padding and children sit inside it. A border painted
    /// over the content (`border_over`, an inset stroke) takes none; nor do
    /// outlines. A transparent border still takes its width, so a colour
    /// change never moves anything.
    pub fn layout_border(&self) -> Insets {
        if self.border_over {
            return Insets::default();
        }
        let sides = self.border_sides();
        let side = |width: f32| if width.is_finite() { width.max(0.) } else { 0. };
        Insets {
            top: side(sides.top),
            right: side(sides.right),
            bottom: side(sides.bottom),
            left: side(sides.left),
        }
    }
    /// Whether it has a visible border.
    pub fn has_border(&self) -> bool {
        let sides = self.border_sides();
        self.border_color.3 > 0
            && [sides.top, sides.right, sides.bottom, sides.left]
                .into_iter()
                .any(|width| width.is_finite() && width > 0.)
    }
    /// Its shadows: the decoration's, else `shadow`.
    pub fn shadows(&self) -> &[BoxShadow] {
        self.decoration
            .as_ref()
            .and_then(|d| d.shadows.as_deref())
            .unwrap_or(self.shadow.as_slice())
    }
    /// The area it paints for a node at `bounds`: the box with its shadows
    /// and outline.
    #[inline]
    pub fn paint_bounds(&self, bounds: Rect) -> Rect {
        // Most boxes (a bar, a fill) have neither: the box itself.
        if self.decoration.is_none() && self.shadow.is_none() && self.outline.is_none() {
            return bounds;
        }
        let painted = self.shadows().iter().fold(bounds, |paint, shadow| {
            paint.union(
                bounds
                    .expand(shadow.spread.max(0.) + shadow.blur_radius.max(0.) * 3.)
                    .translated(shadow.offset),
            )
        });
        match self.outline.filter(Outline::visible) {
            Some(outline) => painted.union(outline.outer(bounds)),
            None => painted,
        }
    }
    /// Whether it paints a second time after its descendants (its border
    /// over them, its outline).
    pub fn paints_over(&self) -> bool {
        (self.border_over && self.has_border()) || self.outline.is_some_and(|o| o.visible())
    }
}
/// What a node is and paints.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A box laying out its children, painting nothing itself.
    Container(Layout),
    /// A decorated container. Its border takes room inside its box, as CSS's
    /// `box-sizing: border-box` has it (`QuadStyle::layout_border`): its
    /// padding and children lay out inside it. The rest of its paint does
    /// not participate in layout.
    Panel {
        /// How it lays out its children.
        layout: Layout,
        /// How it paints.
        quad: QuadStyle,
    },
    /// A filled rectangle.
    Rect(Color),
    /// A decorated box without children.
    Quad(QuadStyle),
    /// A decoded image.
    Image(Arc<crate::image::ImageData>),
    /// An SVG, rasterized at its drawn size.
    Svg(Arc<crate::svg::SvgData>),
    /// A vector drawing (see [`crate::canvas`]).
    Canvas(Arc<crate::canvas::Canvas>),
    /// A retained CoreVideo frame (macOS).
    #[cfg(target_os = "macos")]
    NativeSurface(std::rc::Rc<crate::native_surface::NativeSurface>),
    /// A paragraph of styled spans.
    RichText {
        /// Its spans, shaped as one paragraph.
        text: Arc<crate::rich_text::RichText>,
    },
    /// Plain text.
    Text {
        /// The text.
        text: Arc<str>,
        /// Its colour.
        color: Color,
        /// Its font size, in logical points.
        font_size: f32,
    },
}
/// Lengths on each side of a box, in logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// The top side.
    pub top: f32,
    /// The right side.
    pub right: f32,
    /// The bottom side.
    pub bottom: f32,
    /// The left side.
    pub left: f32,
}
impl Insets {
    /// `value` on every side.
    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}
/// Where children sit across a row's or column's axis (CSS
/// `align-items`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// A row's children aligned by their first lines' baselines.
    Baseline,
    /// At the start.
    #[default]
    Start,
    /// Centred.
    Center,
    /// At the end.
    End,
    /// Stretched across, unless they size themselves on that axis.
    Stretch,
}
/// Distribution along a row's or column's main axis (CSS `justify-content`),
/// and an overlay's vertical alignment (CSS grid's `align-items`: each child
/// on its own; the space-distributing values act as `Start` or `Center`, as
/// they do for one item).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Packed at the start.
    #[default]
    Start,
    /// Centred.
    Center,
    /// Packed at the end.
    End,
    /// The free space between the children, none at the ends.
    SpaceBetween,
    /// The free space around each child: half as much at the ends.
    SpaceAround,
    /// The free space spread evenly between the children and the ends.
    SpaceEvenly,
    /// An overlay's children fill its height, unless they size themselves
    /// there. Along a row or column it acts as `Start`, as CSS's
    /// `justify-content: stretch` does in flexbox.
    Stretch,
}
/// A platform text shaper can provide accurate intrinsic and wrapped text metrics.
///
/// Results are cached: this computation may be called any number of times,
/// in any order. Its result must be a function of its inputs and the signals
/// it reads; the invocation sequence and intermediate model states observed
/// are not part of the contract. Reactive reads participate in the calling
/// effect's dependency tracking.
/// Writing signals from a measurement or shaping callback is unsupported.
pub trait TextMeasurer {
    /// The size of `text` at `font_size`, wrapped at `max_width` if given.
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32);
}
impl<F: Fn(&str, f32, Option<f32>) -> (f32, f32)> TextMeasurer for F {
    fn measure(&self, text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
        self(text, font_size, max_width)
    }
}
/// A node's layout style: its size, spacing, flex and alignment. The
/// rarer options (grid, wrapping, percentages, positioning) are in
/// `layout_options`.
#[derive(Clone, Debug)]
pub struct Style {
    /// Opt-in grid, wrapping, richer lengths and positioning.
    pub layout_options: Option<Arc<crate::layout::LayoutOptions>>,
    /// Remove this child from normal flow and position it against its
    /// containing block: the nearest ancestor that is `relative`, found
    /// without passing an `absolute` ancestor; otherwise its parent. Its
    /// insets (`LayoutOptions::inset`) count from that block's padding box
    /// (inside its border); without any it sits at its parent's content
    /// origin plus margins. It does not contribute to intrinsic parent size.
    pub absolute: bool,
    /// A containing block for `absolute` descendants (CSS `position:
    /// relative`). It changes nothing about this node's own layout.
    pub relative: bool,
    /// `width` is this node's intrinsic width rather than a constraint: it
    /// takes it when nothing else sizes it (a flex basis, fit content), but
    /// a parent stretching its children overrides it, as it would a box
    /// sized by its content. A scroll view's and an input's default widths.
    pub intrinsic_width: bool,
    /// `height` is its intrinsic height, as `intrinsic_width`.
    pub intrinsic_height: bool,
    /// Its width in logical points; `None` sizes it by its content and
    /// parent.
    pub width: Option<f32>,
    /// Its height in logical points; `None` sizes it by its content and
    /// parent.
    pub height: Option<f32>,
    /// Fraction of the definite parent content width (1.0 means 100%).
    /// Falls back to intrinsic sizing when that axis is indefinite. Pixel width wins.
    pub width_percent: Option<f32>,
    /// Fraction of the definite parent content height. Absolute children use the
    /// final parent box. Invalid/negative fractions resolve to zero; values over 1 are allowed.
    pub height_percent: Option<f32>,
    /// Padding on every side, unless `padding_edges` is set.
    pub padding: f32,
    /// Per-edge padding, overriding `padding` when present.
    pub padding_edges: Option<Insets>,
    /// The room between children.
    pub gap: f32,
    /// Clip its descendants to its box (CSS `overflow: hidden`).
    pub clip: bool,
    /// Fade descendants out over these bands (top, bottom) at the edges of
    /// this node's clip, in the fragment shader: a scroll view whose content
    /// dissolves under a titlebar or into a composer, without a layer.
    pub fade_edges: [f32; 2],
    /// Where `fade_edges` bands end, this far in from the clip's edges (top,
    /// bottom): content past them is hidden, so a scroll view running on
    /// under a bar can be faded out by the bar's edge.
    pub fade_insets: [f32; 2],
    /// `fade_edges` at the left and right edges: a horizontally scrolling
    /// row fading its overflow.
    pub fade_edges_x: [f32; 2],
    /// `fade_insets` at the left and right edges.
    pub fade_insets_x: [f32; 2],
    /// Fade an edge only as far as content runs past it: a list scrolled to
    /// its top shows no top band, and the band grows to its width as the
    /// content scrolls under that edge.
    pub fade_overflow: bool,
    /// Lay its edges on whole points: a box placed by percentages or ratios
    /// (a split pane) keeps crisp borders at every size. Its size follows
    /// its rounded edges, so boxes that share an edge still meet.
    pub snap: bool,
    /// The smallest width it lays out at.
    pub min_width: Option<f32>,
    /// The largest width it lays out at.
    pub max_width: Option<f32>,
    /// The smallest height it lays out at.
    pub min_height: Option<f32>,
    /// The largest height it lays out at.
    pub max_height: Option<f32>,
    /// Room outside its box.
    pub margin: Insets,
    /// Its share of its parent's free space (CSS `flex-grow`).
    pub flex_grow: f32,
    /// How much it gives up when its parent is short of space (CSS
    /// `flex-shrink`; 1 by default).
    pub flex_shrink: f32,
    /// Where its children sit across its axis.
    pub align: Align,
    /// How its children spread along its axis.
    pub justify: Justify,
    /// Request wrapping at the available content width from the text measurer.
    pub text_wrap: bool,
    /// Plain-text display overflow; ignored by editor document shaping.
    /// `NodeKind::RichText` instead uses its immutable `RichText::options()` so
    /// measurement, rendering and actionable span geometry share one source.
    pub text_options: crate::text_layout::TextOptions,
}

/// Every field empty, zero or off, except `flex_shrink`, which is 1 as
/// CSS's initial value is: an item gives way in a container too small for
/// it, down to its content's min-content size (its automatic minimum).
impl Default for Style {
    fn default() -> Self {
        Self {
            layout_options: None,
            absolute: false,
            relative: false,
            intrinsic_width: false,
            intrinsic_height: false,
            width: None,
            height: None,
            width_percent: None,
            height_percent: None,
            padding: 0.,
            padding_edges: None,
            gap: 0.,
            clip: false,
            fade_edges: [0.; 2],
            fade_insets: [0.; 2],
            fade_edges_x: [0.; 2],
            fade_insets_x: [0.; 2],
            fade_overflow: false,
            snap: false,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            margin: Insets::default(),
            flex_grow: 0.,
            flex_shrink: 1.,
            align: Align::default(),
            justify: Justify::default(),
            text_wrap: false,
            text_options: Default::default(),
        }
    }
}
/// As derived, but with the flags and small enums compared together rather
/// than one branch each: restyles compare a node's whole style every time.
impl PartialEq for Style {
    fn eq(&self, other: &Self) -> bool {
        self.eq_except_box(other)
            && self.width == other.width
            && self.height == other.height
            && self.width_percent == other.width_percent
            && self.height_percent == other.height_percent
            && self.margin == other.margin
    }
}
impl Style {
    /// Whether `self` and `other` are equal but for their box (width,
    /// height, their percentages and margins: what a growing bar changes).
    fn eq_except_box(&self, other: &Self) -> bool {
        // Destructured so that a new field cannot be left out.
        let Style {
            layout_options,
            absolute,
            relative,
            intrinsic_width,
            intrinsic_height,
            width: _,
            height: _,
            width_percent: _,
            height_percent: _,
            padding,
            padding_edges,
            gap,
            clip,
            fade_edges,
            fade_insets,
            fade_edges_x,
            fade_insets_x,
            fade_overflow,
            snap,
            min_width,
            max_width,
            min_height,
            max_height,
            margin: _,
            flex_grow,
            flex_shrink,
            align,
            justify,
            text_wrap,
            text_options,
        } = self;
        let flags = (*absolute == other.absolute)
            & (*relative == other.relative)
            & (*intrinsic_width == other.intrinsic_width)
            & (*intrinsic_height == other.intrinsic_height)
            & (*clip == other.clip)
            & (*fade_overflow == other.fade_overflow)
            & (*snap == other.snap)
            & (*text_wrap == other.text_wrap)
            & (*align == other.align)
            & (*justify == other.justify);
        flags
            && *padding == other.padding
            && *padding_edges == other.padding_edges
            && *gap == other.gap
            && *fade_edges == other.fade_edges
            && *fade_insets == other.fade_insets
            && *fade_edges_x == other.fade_edges_x
            && *fade_insets_x == other.fade_insets_x
            && *min_width == other.min_width
            && *max_width == other.max_width
            && *min_height == other.min_height
            && *max_height == other.max_height
            && *flex_grow == other.flex_grow
            && *flex_shrink == other.flex_shrink
            && *text_options == other.text_options
            && *layout_options == other.layout_options
    }
    /// Whether this is `Style::default()`, as `==` would say, without making
    /// one to compare with: what nearly every node has before it is styled.
    pub(crate) fn is_default(&self) -> bool {
        // Destructured so that a new field cannot be left out.
        let Style {
            layout_options,
            absolute,
            relative,
            intrinsic_width,
            intrinsic_height,
            width,
            height,
            width_percent,
            height_percent,
            padding,
            padding_edges,
            gap,
            clip,
            fade_edges,
            fade_insets,
            fade_edges_x,
            fade_insets_x,
            fade_overflow,
            snap,
            min_width,
            max_width,
            min_height,
            max_height,
            margin,
            flex_grow,
            flex_shrink,
            align,
            justify,
            text_wrap,
            text_options,
        } = self;
        let set = *absolute
            | *relative
            | *intrinsic_width
            | *intrinsic_height
            | *clip
            | *fade_overflow
            | *snap
            | *text_wrap;
        let unset = layout_options.is_none()
            & width.is_none()
            & height.is_none()
            & width_percent.is_none()
            & height_percent.is_none()
            & padding_edges.is_none()
            & min_width.is_none()
            & max_width.is_none()
            & min_height.is_none()
            & max_height.is_none();
        !set && unset
            && (*padding == 0.) & (*gap == 0.) & (*flex_grow == 0.) & (*flex_shrink == 1.)
            && *fade_edges == [0.; 2]
            && *fade_insets == [0.; 2]
            && *fade_edges_x == [0.; 2]
            && *fade_insets_x == [0.; 2]
            && *margin == Insets::default()
            && *align == Align::default()
            && *justify == Justify::default()
            && *text_options == crate::text_layout::TextOptions::default()
    }
}
const LAYOUT: u8 = 1;
const PAINT: u8 = 2;
const COMPOSITE: u8 = 4;
/// Repainted where its damage is already recorded: layout damaged a node it
/// moved where it was and where it lands (see `arrange`).
const PAINTED: u8 = 8;
/// Its children changed: one was added or removed, or their order changed
/// (`children_changed`). Damage is the children's own.
const CHILDREN: u8 = 16;
/// Its paint changed in colour alone (`paints_only`): its geometry, text and
/// resources are as they were.
const PAINT_ONLY: u8 = 32;
/// Its paint changed otherwise too, this frame: not `PAINT_ONLY`.
const PAINT_MIXED: u8 = 64;
/// A setter changed the node itself, rather than layout moving it alone.
const CHANGED: u8 = 128;
/// Repaints a scene holds for a renderer before giving up on its patching
/// (`Scene::repaints`): past them, it would draw the frame whole anyway.
const REPAINTS_KEPT: usize = 1 << 14;
/// Flushes a scene keeps its repaints unread before it stops keeping them
/// (`Scene::repaints`): a renderer reads them every frame it patches.
const REPAINTS_UNREAD: u8 = 4;
/// What changed about a node a flush repainted (`Scene::repaints`).
pub mod repaint {
    /// Its own paint: style, text, kind.
    pub const PAINT: u8 = super::PAINT;
    /// Its box: layout moved or resized it.
    pub const MOVED: u8 = super::PAINTED;
    /// How it composites: opacity, transform, filters.
    pub const COMPOSITE: u8 = super::COMPOSITE;
    /// Its children: one was added or removed, or they were reordered.
    /// Every node a flush added or removed lies under a node repainted
    /// with this.
    pub const CHILDREN: u8 = super::CHILDREN;
    /// Its paint changed and nothing else did: colours alone (a fill, a
    /// border, a text's colour), its box, text, kind and resources as they
    /// were, nothing moved, composited or laid out anew. Its items can be
    /// made again in place.
    pub const PAINT_ONLY: u8 = 32;
    /// Layout moved or resized it and nothing else changed: its kind, style,
    /// paint and effects are as they were. Its items can be made again from
    /// its new box (their geometry from it, the rest unchanged).
    pub const BOX_ONLY: u8 = 64;
}
/// The `repaint` flags a flush records for a node from its dirty bits.
fn repaint_flags(dirty: u8) -> u8 {
    let shown = dirty & (PAINT | PAINTED | COMPOSITE | CHILDREN);
    let paint_only = dirty & (PAINT | PAINT_ONLY) == PAINT | PAINT_ONLY
        && dirty & (PAINT_MIXED | PAINTED | COMPOSITE | CHILDREN | LAYOUT) == 0;
    let box_only = dirty & PAINTED != 0 && dirty & (CHANGED | PAINT | COMPOSITE | CHILDREN) == 0;
    shown
        | if paint_only { repaint::PAINT_ONLY } else { 0 }
        | if box_only { repaint::BOX_ONLY } else { 0 }
}
/// Whether `new` differs from `old` in colour alone (`PAINT_ONLY`): a rect's
/// colour, a text's colour, a box's fill or border colour (with no
/// decoration, whose rasters are kept apart).
fn paints_only(old: &NodeKind, new: &NodeKind) -> bool {
    let colours = |a: &QuadStyle, b: &QuadStyle| {
        a.decoration.is_none()
            && QuadStyle {
                fill: b.fill,
                border_color: b.border_color,
                ..a.clone()
            } == *b
    };
    match (old, new) {
        (NodeKind::Rect(_), NodeKind::Rect(_)) => true,
        (
            NodeKind::Text {
                text: a,
                font_size: x,
                ..
            },
            NodeKind::Text {
                text: b,
                font_size: y,
                ..
            },
        ) => a == b && x == y,
        (NodeKind::Quad(a), NodeKind::Quad(b)) => colours(a, b),
        (NodeKind::Panel { layout: l, quad: a }, NodeKind::Panel { layout: m, quad: b }) => {
            l == m && colours(a, b)
        }
        _ => false,
    }
}
// Two feature counts are bounded by twice the Slot vector's allocation
// length, far below this bit (size_of::<Slot>() > 2, Vec bytes <= isize::MAX).
// While removals are pending, this bit routes only subtree damage through
// its existing exceptional path. Layout/flush materialize before other uses.
const PENDING_CHILDREN_BIT: usize = 1usize << (usize::BITS - 1);
const _: () = assert!(std::mem::size_of::<Slot>() > 2);
const BOUNDS_VALID: u8 = 1;
const HIT_DIRECTION: u8 = 2;
const HIT_CHECKED: u8 = 32;
const NON_RTL: u8 = 64;
const INK_VALID: u8 = 128;
// Pin the original font/children region despite UnsafeCell suppressing
// the Vec's niche. No extra fields or allocation.
#[repr(C)]
struct NodeHead {
    font: crate::text_layout::FontStyle,
    children: std::cell::UnsafeCell<Vec<NodeId>>,
}
impl std::ops::Deref for Node {
    type Target = NodeHead;
    #[inline(always)]
    fn deref(&self) -> &NodeHead {
        &self.head
    }
}
impl std::ops::DerefMut for Node {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut NodeHead {
        &mut self.head
    }
}
struct Node {
    cursor: Option<crate::cursor::Cursor>,
    parent: Option<NodeId>,
    kind: NodeKind,
    head: NodeHead,
    style: Style,
    bounds: Rect,
    transform: Transform,
    effects: Effects,
    /// What changed since the last flush: `LAYOUT`, `PAINT`, `COMPOSITE`,
    /// `PAINTED`, `CHILDREN`, `PAINT_ONLY`, `PAINT_MIXED`, `CHANGED`. All
    /// eight bits are taken: another needs a wider type.
    dirty: u8,
    queued: bool,
    /// The measurement `arrange` places children from: the most recent one used.
    measurement: Option<Measurement>,
    /// Other recent results under different constraints, most recent first.
    alternates: Vec<Measurement>,
    /// Sizes of older results, without their children's places: enough for
    /// an ancestor's probe (see `Scene::measure`), most recent first.
    sizes: Vec<ProbeSize>,
    /// Everything this subtree can paint, including its own translation but not
    /// its ancestors'. Rendering skips subtrees whose ink misses the damage.
    /// A valid visible node has only valid descendants, so invalidation stops
    /// at the first invalid ancestor. A hidden node is cached as painting
    /// nothing without visiting its children; it can only become visible
    /// through its own change, which invalidates it and its ancestors.
    ink: std::cell::Cell<Option<Rect>>,
    /// Ink and directional hit proofs share a validity byte. Any cached
    /// dependency sets BOUNDS_VALID, including descendants visited by a proof,
    /// so the existing invalidation walk reaches every dependent ancestor.
    bounds_valid: std::cell::Cell<u8>,
    /// Intrinsic text or rich text size by wrap limit and shaper revision. Flex layout
    /// measures a leaf under several constraint sets per pass, which defeats
    /// `measurement`; this keeps an unchanged label from being reshaped each
    /// time an ancestor relayouts. Cleared by the node's own layout invalidation.
    /// Damage fills it too (see `text_ink`), for text whose box is fixed.
    text_size: TextSizeCache,
    advanced_layout: Option<Box<advanced_layout::AdvancedLayoutCache>>,
    arrange_dirty: bool,
    /// Whether its size can depend on the available width and height it is
    /// measured under (`Scene::reads_available`): `READS_KNOWN` once worked
    /// out, cleared with its layout.
    reads: std::cell::Cell<u8>,
    isolated: bool,
    /// Scale, rotation and skew, and its zoom; a node with either paints as
    /// a layer. Boxed: few nodes have one, and every node's size costs each
    /// tree walk.
    local: Option<Box<Local>>,
    /// An alpha mask over the subtree (`Scene::set_mask`): a layer too.
    image_mask: Option<Box<ImageMask>>,
    /// Whether it composites through a layer shader (`Scene::layer_shaders`
    /// holds it): a layer while it has one.
    layer_shaded: bool,
    /// Whether a mask, a layer shader or a blend mode makes it a layer
    /// (`effect_layer` asks every node walked: one test, not three).
    layered: bool,
    /// How it mixes with its backdrop (`Scene::set_blend`): a layer unless
    /// `Normal`.
    blend: BlendMode,
    /// Its clip inset (`Scene::set_clip_inset`).
    clip_inset: Option<Box<ClipInset>>,
    /// Its shader fill's uniforms (`Scene::set_shader_uniforms`).
    shader_uniforms: Option<Box<crate::decoration::ShaderUniforms>>,
    /// Has had a local transform, content blur or mask: its layer revision
    /// keeps counting while it has none, so a renderer can keep its texture
    /// (see `invalidate`).
    promoted: bool,
    layer_revision: u64,
    /// Scroll content: a translation is reported as a `ScrollMove` the
    /// renderer may apply by copying pixels (see `set_scroll_copy`).
    scroll_copy: bool,
    /// `CONTENTS`: lays out as its last child (`Scene::set_contents`):
    /// `style` is then derived, its own kept in `Scene::contents_styles`.
    /// `ZOOMED`: has a zoom (`Scene::set_zoom`). One byte, so the branches
    /// that test for a contents node send a zoomed one aside as well, for no
    /// more work on the nodes with neither (`measure_uncached`, `restyle`).
    special: Special,
    /// Draws an image, an icon or a canvas to its box (`Scene::set_replaced`):
    /// a size it sets is also its least.
    replaced: bool,
    /// Its lines run right to left (`Scene::set_direction`, its own or its
    /// parent's): `style` is then laid out mirrored (`laid_out_style`).
    rtl: bool,
    /// Its font's alignment as given, if `Start` or `End`, which follow the
    /// direction: made `Left` or `Right` (`set_font`).
    align_logical: Option<crate::text_layout::TextAlign>,
    /// Writes to `style` so far: a caller that saw the count after its own
    /// write knows the style is still the one it gave (`set_box`).
    style_writes: u32,
}
/// A node's local transform and zoom (`Node::local`).
#[derive(Clone, Copy, PartialEq)]
struct Local {
    transform: LocalTransform,
    /// Its subtree's lengths times this on screen (`Scene::set_zoom`).
    zoom: f32,
    /// Laid out (`snap_zoomed`): the room its parent last measured it
    /// under, its result since in its own points (`measure_special`).
    placed: Option<Constraints>,
    /// Whether `transform` or `zoom` maps its subtree (`Node::transformed`),
    /// kept as either changes: one test where each step of a walk would
    /// compare the transform with the identity field by field.
    maps: bool,
}
/// `Node::special`: a contents node.
const CONTENTS: u8 = 1;
/// `Node::special`: a zoomed node.
const ZOOMED: u8 = 2;
/// `Node::special`'s bits (`CONTENTS`, `ZOOMED`), as an enum rather than a
/// `u8`: its unused values are a niche, as a `bool`'s are, so `Node` lays
/// its byte fields out as it did with `contents: bool` (a field moved puts
/// `invalidate_layout`'s two-byte store off its alignment).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Special {
    None = 0,
    Contents = 1,
    Zoomed = 2,
    Both = 3,
}
impl Special {
    /// Whether it has any bit.
    #[inline(always)]
    fn any(self) -> bool {
        self != Special::None
    }
    /// Whether it has `bit`.
    #[inline(always)]
    fn has(self, bit: u8) -> bool {
        self as u8 & bit != 0
    }
    /// It with `bit` set or cleared.
    fn with(self, bit: u8, on: bool) -> Self {
        match if on {
            self as u8 | bit
        } else {
            self as u8 & !bit
        } {
            0 => Special::None,
            1 => Special::Contents,
            2 => Special::Zoomed,
            _ => Special::Both,
        }
    }
}
impl Node {
    /// Whether it lays out as its last child (`Scene::set_contents`).
    #[inline(always)]
    fn contents(&self) -> bool {
        self.special.has(CONTENTS)
    }
    fn local(&self) -> LocalTransform {
        self.local
            .as_deref()
            .map_or(LocalTransform::IDENTITY, |local| local.transform)
    }
    /// Its zoom (`Scene::set_zoom`): 1 unless set.
    fn zoom(&self) -> f32 {
        self.local.as_deref().map_or(1., |local| local.zoom)
    }
    /// Whether it has a local transform other than the identity, or a
    /// zoom: either maps its subtree through `matrix`.
    fn transformed(&self) -> bool {
        self.local.as_deref().is_some_and(|local| local.maps)
    }
    /// Its subtree's mapping onto its parent's space, for its box at
    /// `bounds`: its local transform (`LocalTransform::projective`), then
    /// its zoom about its top-left. Out of line: the walks that reach it
    /// (`hit_test_all`, paint) do so for transformed nodes alone, and
    /// inlined, it takes registers from each of their steps.
    #[inline(never)]
    fn projective(&self, bounds: Rect) -> crate::affine::Projective {
        let Some(local) = self.local.as_deref() else {
            return crate::affine::Projective::IDENTITY;
        };
        let map = local.transform.projective(bounds);
        match local.zoom {
            1. => map,
            zoom => then_zoom(map, bounds, zoom),
        }
    }
    /// `projective` without the tilt (`LocalTransform::matrix`): what a
    /// paint item's `matrix` holds when the whole map is not affine.
    fn flat_matrix(&self, bounds: Rect) -> crate::affine::Affine {
        let Some(local) = self.local.as_deref() else {
            return crate::affine::Affine::IDENTITY;
        };
        let matrix = local.transform.matrix(bounds);
        match local.zoom {
            1. => matrix,
            zoom => matrix.then(zoom_about(bounds, zoom)),
        }
    }
    /// Whether it composites through a content blur.
    fn filtered(&self) -> bool {
        self.effects.filter_blur > 0.
    }
    /// Whether it paints as a layer for an effect on its content: a content
    /// blur, a mask, a layer shader or a blend mode.
    fn effect_layer(&self) -> bool {
        self.filtered() || self.layered
    }
    /// Works `layered` out again after its mask, layer shader or blend changes.
    fn relayer(&mut self) {
        self.layered =
            self.image_mask.is_some() || self.layer_shaded || self.blend != BlendMode::Normal;
    }
    fn set_local(&mut self, transform: LocalTransform) {
        match &mut self.local {
            // Zoomed: it maps either way.
            Some(local) if local.zoom != 1. => local.transform = transform,
            _ if transform == LocalTransform::IDENTITY => self.local = None,
            Some(local) => {
                local.transform = transform;
                local.maps = !transform.is_identity();
            }
            None => {
                self.local = Some(Box::new(Local {
                    transform,
                    zoom: 1.,
                    placed: None,
                    maps: !transform.is_identity(),
                }))
            }
        }
    }
    fn set_local_and_zoom(&mut self, transform: LocalTransform, zoom: f32) {
        let local = Local {
            transform,
            zoom,
            placed: None,
            maps: !transform.is_identity() || zoom != 1.,
        };
        match &mut self.local {
            _ if transform == LocalTransform::IDENTITY && zoom == 1. => self.local = None,
            Some(old) => **old = local,
            None => self.local = Some(Box::new(local)),
        }
    }
}
#[derive(Default)]
struct TextSizeCache(std::cell::Cell<[Option<TextSize>; 3]>);
impl TextSizeCache {
    fn get(&self) -> TextSizes {
        TextSizes(self.0.get())
    }
    fn set(&self, sizes: TextSizes) {
        self.0.set(sizes.0);
    }
}
/// A text node's measured sizes for its most recent wrap limits. Layout
/// measures a paragraph at more than one width per pass (intrinsic, then
/// final); one entry would evict the other and reshape the text each time.
#[derive(Clone, Copy, Default)]
struct TextSizes([Option<TextSize>; 3]);
/// A wrap limit, the shaper revision it was measured under, and the size.
type TextSize = (Option<f32>, u64, (f32, f32));
impl TextSizes {
    fn find(&self, limit: Option<f32>, revision: u64) -> Option<(f32, f32)> {
        self.0
            .iter()
            .flatten()
            .find(|(cached, cached_revision, _)| *cached == limit && *cached_revision == revision)
            .map(|(_, _, size)| *size)
    }
    fn insert(&mut self, limit: Option<f32>, revision: u64, size: (f32, f32)) {
        self.0.rotate_right(1);
        self.0[0] = Some((limit, revision, size));
    }
    fn remember(cell: &TextSizeCache, limit: Option<f32>, revision: u64, size: (f32, f32)) {
        let mut sizes = cell.get();
        sizes.insert(limit, revision, size);
        cell.set(sizes);
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Constraints {
    width: Option<f32>,
    height: Option<f32>,
    percent_width: Option<f32>,
    percent_height: Option<f32>,
    force_width: Option<f32>,
    force_height: Option<f32>,
}
/// Constraints no measure asks under (NaN equals nothing): a zoomed node's
/// result while it is in its own points (`snap_zoomed`).
const UNMATCHED: Constraints = Constraints {
    width: Some(f32::NAN),
    height: None,
    percent_width: None,
    percent_height: None,
    force_width: None,
    force_height: None,
};
impl Constraints {
    /// Every length times `factor`: a zoomed node's room in its own points
    /// (`Scene::set_zoom`).
    fn scaled(self, factor: f32) -> Self {
        let scale = |length: Option<f32>| length.map(|length| length * factor);
        Self {
            width: scale(self.width),
            height: scale(self.height),
            percent_width: scale(self.percent_width),
            percent_height: scale(self.percent_height),
            force_width: scale(self.force_width),
            force_height: scale(self.force_height),
        }
    }
}
type ChildPlacement = (NodeId, Rect, Constraints);
struct Measurement {
    constraints: Constraints,
    size: (f32, f32),
    /// Each child's place and the constraints that measured it. A later
    /// probe can leave it holding another result; `arrange` restores this
    /// one before placing it.
    children: Vec<ChildPlacement>,
    /// Layout pass that produced this result. A dirty node may reuse only
    /// results from the pass in progress.
    pass: u64,
}
#[inline(never)]
fn promote_measurement(
    current: &mut Option<Measurement>,
    alternates: &mut Vec<Measurement>,
    index: usize,
) -> (f32, f32) {
    if let Some(current) = current {
        // Keep the same MRU order without moving the untouched suffix twice.
        alternates[..=index].rotate_right(1);
        std::mem::swap(current, &mut alternates[0]);
        current.size
    } else {
        let hit = alternates.remove(index);
        let size = hit.size;
        *current = Some(hit);
        size
    }
}

/// Flex layout measures a child with its natural, flexed and stretched
/// constraints in one pass, and taffy adds min- and max-content probes, each
/// ancestor on a changed path asking again. Every result of the pass in
/// progress is kept (up to `PASS_RESULTS`), so each probe of a subtree is laid
/// out once per pass however many ancestors repeat it; with too few, each
/// level of nesting multiplied the work. Afterwards a node keeps this many,
/// most recently used first, for the next pass's clean probes.
const MEASUREMENT_ALTERNATES: usize = 7;
/// A bound on one pass's results per node, against pathological probing.
const PASS_RESULTS: usize = 64;
/// Sizes a node keeps of results it no longer holds whole (`Node::sizes`):
/// the probes a clean subtree answers every frame of an animation around it.
const PROBE_SIZES: usize = 32;
/// A result's size alone (see `Node::sizes`).
#[derive(Clone, Copy)]
struct ProbeSize {
    constraints: Constraints,
    size: (f32, f32),
    pass: u64,
}
impl From<&Measurement> for ProbeSize {
    fn from(m: &Measurement) -> Self {
        Self {
            constraints: m.constraints,
            size: m.size,
            pass: m.pass,
        }
    }
}
/// `Scene::relevant` for `node`, whose `reads_bits` are `bits`.
#[inline(always)]
fn relevant_to(node: &Node, bits: u8, mut constraints: Constraints) -> Constraints {
    relevant_to_in_place(node, bits, &mut constraints);
    constraints
}
#[inline(always)]
fn relevant_to_in_place(node: &Node, bits: u8, constraints: &mut Constraints) {
    if bits & OWN_PERCENT_WIDTH == 0 {
        constraints.percent_width = None;
    }
    if bits & OWN_PERCENT_HEIGHT == 0 {
        constraints.percent_height = None;
    }
    // Scroll content is as large as its content along an axis nothing
    // sizes it on: the viewport's room does not fit it (it scrolls),
    // so nothing in it shrinks or wraps to the viewport there.
    if node.scroll_copy {
        if node.style.width.is_none() && node.style.width_percent.is_none() {
            constraints.width = None;
        }
        if node.style.height.is_none() && node.style.height_percent.is_none() {
            constraints.height = None;
        }
    }
    if constraints.force_width.is_some() || bits & READS_WIDTH == 0 {
        constraints.width = constraints.force_width;
    }
    if constraints.force_height.is_some() || bits & READS_HEIGHT == 0 {
        constraints.height = constraints.force_height;
    }
}
/// Put `evicted` (most recent first) ahead of `sizes`' older entries, each
/// constraint kept once, at most `PROBE_SIZES`.
fn keep_sizes(sizes: &mut Vec<ProbeSize>, evicted: Vec<ProbeSize>) {
    if evicted.is_empty() {
        return;
    }
    sizes.retain(|kept| !evicted.iter().any(|m| m.constraints == kept.constraints));
    sizes.splice(0..0, evicted);
    sizes.truncate(PROBE_SIZES);
}
/// A size flex distribution gives, on a 1/256 point grid: sharing out the
/// room in floats drifts (644.00073 where 644 fits), and the drift would
/// reach every box placed after it. Boxes on the grid still meet exactly.
fn flexed(size: f32) -> f32 {
    if size.is_finite() {
        (size * 256.).round() / 256.
    } else {
        size
    }
}
/// Whether a node clips its content on either axis, which makes it a
/// scroll container to CSS: its automatic minimum is 0.
pub(crate) fn clips(style: &Style) -> bool {
    style.clip
        || style
            .layout_options
            .as_deref()
            .is_some_and(|o| o.clip_x == Some(true) || o.clip_y == Some(true))
}
use crate::text_layout::min_content_lines;
/// `Node::reads`: known, reads the available width, the height; has a
/// width or height percentage in its subtree; resolves its own against the
/// width or height basis its parent passes (`Scene::percent_bases`).
const READS_KNOWN: u8 = 1;
const READS_WIDTH: u8 = 2;
const READS_HEIGHT: u8 = 4;
const PERCENT_WIDTH: u8 = 8;
const PERCENT_HEIGHT: u8 = 16;
const OWN_PERCENT_WIDTH: u8 = 32;
const OWN_PERCENT_HEIGHT: u8 = 64;
/// Which way a node's lines run (CSS's `direction`), inherited by its
/// subtree: rows run from the right, text starts at the right, and the
/// arrows read the other way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left.
    Rtl,
}
impl Direction {
    /// The way a language's script runs, from its BCP 47 tag (`"ar"`,
    /// `"he-IL"`, `"fa_IR.UTF-8"`, `"ku-Arab"`): right to left for Arabic,
    /// Hebrew, Persian, Urdu and the others written in their scripts.
    pub fn of_language(tag: &str) -> Direction {
        let lower = tag.to_ascii_lowercase();
        let mut parts = lower.split(['-', '_', '.', '@']);
        let language = parts.next().unwrap_or_default();
        // A script subtag decides where one is given (Kurdish, Punjabi).
        if let Some(script) = parts.find(|p| p.len() == 4) {
            return match script {
                "arab" | "hebr" | "thaa" | "syrc" | "nkoo" | "adlm" | "rohg" => Direction::Rtl,
                _ => Direction::Ltr,
            };
        }
        match language {
            "ar" | "arc" | "ckb" | "dv" | "fa" | "he" | "iw" | "ks" | "nqo" | "ps" | "sd"
            | "syr" | "ug" | "ur" | "yi" => Direction::Rtl,
            _ => Direction::Ltr,
        }
    }
}
/// Which way a node runs (`own`: its padding and what it holds) and the
/// parent it sits in (`parent`: its margins and insets, which place it
/// there).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Way {
    own: bool,
    parent: bool,
}
/// Whether `style` lays out as given, or by `laid_out_style`.
fn laid_out_as_given(style: &Style, way: Way) -> bool {
    !way.own
        && !way.parent
        && !style
            .layout_options
            .as_deref()
            .is_some_and(|o| o.has_inline())
}
/// What a node given `style` lays out by. Right to left (`rtl`) it is laid
/// out left to right and its children mirrored in its box after
/// (`arrange`), so its left and right edges trade places here and come out
/// where they were given, as CSS keeps them physical. A line's start and
/// end (`*_inline`) are its left and right in that left-to-right layout:
/// right and left once mirrored. Points go where the plain lengths are,
/// which keeps the node off the flex-options path.
fn laid_out_style(mut style: Style, way: Way) -> Style {
    if laid_out_as_given(&style, way) {
        return style;
    }
    let flip = |i: &mut Insets| std::mem::swap(&mut i.left, &mut i.right);
    let flip_lengths =
        |e: &mut crate::layout::EdgeLengths| std::mem::swap(&mut e.left, &mut e.right);
    // Its margins and insets are its parent's to mirror; its padding, its
    // own box's.
    if way.parent {
        flip(&mut style.margin);
        if let Some(options) = &mut style.layout_options {
            let options = Arc::make_mut(options);
            flip_lengths(&mut options.inset);
            flip_lengths(&mut options.margin);
        }
    }
    if way.own {
        if let Some(padding) = &mut style.padding_edges {
            flip(padding);
        }
        if let Some(options) = &mut style.layout_options {
            flip_lengths(&mut Arc::make_mut(options).padding);
        }
    }
    let Some(mut options) = style
        .layout_options
        .as_deref()
        .copied()
        .filter(|o| o.has_inline())
    else {
        return style;
    };
    use crate::layout::Length;
    let (padding, margin, inset) = (
        options.padding_inline,
        options.margin_inline,
        options.inset_inline,
    );
    options.padding_inline = Default::default();
    options.margin_inline = Default::default();
    options.inset_inline = Default::default();
    for (value, left) in [(padding.start, true), (padding.end, false)] {
        match value {
            Some(Length::Px(v)) => {
                let edges = style
                    .padding_edges
                    .get_or_insert(Insets::all(style.padding));
                *if left {
                    &mut edges.left
                } else {
                    &mut edges.right
                } = v;
                *if left {
                    &mut options.padding.left
                } else {
                    &mut options.padding.right
                } = None;
            }
            Some(other) => {
                *if left {
                    &mut options.padding.left
                } else {
                    &mut options.padding.right
                } = Some(other)
            }
            None => {}
        }
    }
    for (value, left) in [(margin.start, true), (margin.end, false)] {
        match value {
            Some(Length::Px(v)) => {
                *if left {
                    &mut style.margin.left
                } else {
                    &mut style.margin.right
                } = v;
                *if left {
                    &mut options.margin.left
                } else {
                    &mut options.margin.right
                } = None;
            }
            Some(other) => {
                *if left {
                    &mut options.margin.left
                } else {
                    &mut options.margin.right
                } = Some(other)
            }
            None => {}
        }
    }
    if inset.start.is_some() {
        options.inset.left = inset.start;
    }
    if inset.end.is_some() {
        options.inset.right = inset.end;
    }
    style.layout_options = (!options.is_empty()).then(|| Arc::new(options));
    style
}
/// What a contents node lays out as (`Scene::set_contents`): `own` with
/// `lead`'s place in the parent. Its margins add to the node's own. The
/// node's visibility and clip stay its own (the lead's would also cut the
/// lead's own shadow), and hiding it (`display: none`) still hides it.
fn contents_style(mut own: Style, lead: &Style) -> Style {
    let extra = own.margin;
    own.absolute = lead.absolute;
    own.width = lead.width;
    own.height = lead.height;
    own.width_percent = lead.width_percent;
    own.height_percent = lead.height_percent;
    own.min_width = lead.min_width;
    own.max_width = lead.max_width;
    own.min_height = lead.min_height;
    own.max_height = lead.max_height;
    own.flex_grow = lead.flex_grow;
    own.flex_shrink = lead.flex_shrink;
    own.margin = Insets {
        top: lead.margin.top + extra.top,
        right: lead.margin.right + extra.right,
        bottom: lead.margin.bottom + extra.bottom,
        left: lead.margin.left + extra.left,
    };
    let mine = own.layout_options.as_deref().copied().unwrap_or_default();
    own.layout_options = match &lead.layout_options {
        None if mine.visible.is_none() && mine.clip_x.is_none() && mine.clip_y.is_none() => {
            match mine.display {
                Some(crate::layout::Display::None) => own.layout_options,
                _ => None,
            }
        }
        lead_options => {
            let mut options = lead_options.as_deref().copied().unwrap_or_default();
            options.visible = mine.visible;
            options.clip_x = mine.clip_x;
            options.clip_y = mine.clip_y;
            if mine.display == Some(crate::layout::Display::None) {
                options.display = mine.display;
            }
            // Its own margins add to a margin the lead sets as a length.
            for (side, add) in [
                (&mut options.margin.top, extra.top),
                (&mut options.margin.right, extra.right),
                (&mut options.margin.bottom, extra.bottom),
                (&mut options.margin.left, extra.left),
            ] {
                if let Some(crate::layout::Length::Px(v)) = side {
                    *v += add;
                }
            }
            match lead_options {
                Some(lead_options) if **lead_options == options => Some(lead_options.clone()),
                _ if options.is_empty() => None,
                _ => Some(Arc::new(options)),
            }
        }
    };
    own
}
fn dimension(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    value
        .max(0.0)
        .min(max.unwrap_or(f32::INFINITY).max(0.0))
        .max(min.unwrap_or(0.0).max(0.0))
}
fn percentage(fraction: Option<f32>, basis: Option<f32>) -> Option<f32> {
    fraction.zip(basis).map(|(fraction, basis)| {
        let fraction = if fraction.is_finite() {
            fraction.max(0.0)
        } else {
            0.0
        };
        ((fraction as f64 * basis.max(0.0) as f64).min(f32::MAX as f64)) as f32
    })
}
struct Slot {
    generation: u32,
    /// Removed and not yet freed (`Scene::kill`): its generation has moved
    /// on, so no id reaches it, and it is not reused until freed.
    dead: bool,
    node: Option<Node>,
}
/// A node id no longer in the scene was used (`Scene::tree_node`).
#[cold]
#[inline(never)]
#[track_caller]
fn stale() -> ! {
    panic!("stale scene node")
}
/// Where a parent's children are damaged (`Scene::damage_context`).
type DamageContext = Option<(Transform, Option<Rect>)>;
/// What a [`Scene::flush`] did, for renderers and diagnostics.
#[derive(Default, Debug)]
pub struct FrameReport {
    /// The areas to repaint, in logical points.
    pub damage: Vec<Rect>,
    /// How many nodes were laid out again.
    pub layout_nodes: usize,
    /// How many nodes were repainted.
    pub paint_nodes: usize,
    /// How many nodes only composited again (moved, faded, transformed).
    pub composite_nodes: usize,
}
impl FrameReport {
    /// Whether nothing changed: no damage and no work done.
    pub fn is_idle(&self) -> bool {
        self.damage.is_empty()
            && self.layout_nodes == 0
            && self.paint_nodes == 0
            && self.composite_nodes == 0
    }
}
/// The parts of `clip` that content shifted by `(dx, dy)` leaves uncovered.
fn exposed(clip: Rect, dx: f32, dy: f32) -> Vec<Rect> {
    let mut strips = Vec::new();
    if dy > 0. {
        strips.push(Rect::new(clip.x, clip.y, clip.width, dy));
    } else if dy < 0. {
        strips.push(Rect::new(
            clip.x,
            clip.y + clip.height + dy,
            clip.width,
            -dy,
        ));
    }
    if dx > 0. {
        strips.push(Rect::new(clip.x, clip.y, dx, clip.height));
    } else if dx < 0. {
        strips.push(Rect::new(
            clip.x + clip.width + dx,
            clip.y,
            -dx,
            clip.height,
        ));
    }
    strips
}
/// A `fade_edges` ancestor's clip (less its `fade_insets`) and its bands
/// (top, right, bottom, left).
pub type FadeMask = (Rect, [f32; 4]);
/// A box with rounded corners: its radii top-left, top-right, bottom-right,
/// bottom-left, each at most half its shorter side.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoundRect {
    /// The box.
    pub rect: Rect,
    /// Its corner radii.
    pub radii: [f32; 4],
    /// How far inside its edge what it clips fades out from (see
    /// `Decoration::clip_feather`); none cuts, antialiased over a pixel.
    pub feather: Feather,
}
/// A rounded clip's feather, in points: stored as its bits plus one, never
/// zero, so an `Option<RoundRect>` takes no more room for it (the option
/// uses the zero) and paint items, carrying two, copy no more than without.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feather(std::num::NonZeroU32);
impl Feather {
    /// None: cut at the edge.
    pub const NONE: Self = Self(std::num::NonZeroU32::MIN);
    /// `points` of feather; none unless positive and finite.
    pub fn new(points: f32) -> Self {
        if points > 0. && points.is_finite() {
            // Positive and finite: its bits are below `u32::MAX`.
            Self(std::num::NonZeroU32::MIN.saturating_add(points.to_bits()))
        } else {
            Self::NONE
        }
    }
    /// In points; 0 for none.
    pub fn points(self) -> f32 {
        f32::from_bits(self.0.get() - 1)
    }
}
impl Default for Feather {
    fn default() -> Self {
        Self::NONE
    }
}
/// The rounded clips around a paint item, innermost first: those of its
/// nearest ancestors that clip (`overflow: hidden`) with rounded corners
/// (`QuadStyle::radius` or per-corner decoration corners). Rectangular
/// clipping stays `PaintItem::clip`; these cut the corners, in the item's
/// layer's space. Two cover a rounded thumbnail inside a rounded card;
/// further out, the corners are those of the outer two.
pub type RoundedClip = [Option<RoundRect>; 2];
/// One thing to paint, as the paint walk yields it: a node's box, kind,
/// text style, effects and clips, in window space.
#[derive(Clone)]
pub struct PaintItem<'a> {
    /// A layer (an isolated or transformed subtree) represented by its cached image.
    pub isolated: bool,
    /// The node.
    pub id: NodeId,
    /// Its box as laid out and translated.
    pub bounds: Rect,
    /// What it paints.
    pub kind: &'a NodeKind,
    /// Its text style.
    pub font: &'a crate::text_layout::FontStyle,
    /// How its plain text wraps and overflows.
    pub text_options: crate::text_layout::TextOptions,
    /// How it composites.
    pub effects: Effects,
    /// The rectangle it paints within, if clipped.
    pub clip: Option<Rect>,
    /// This item paints `id`'s border alone, after its descendants
    /// (`QuadStyle::border_over`); the node's first item painted the rest.
    pub border_pass: bool,
    /// Its matrix, fade mask, rounded clips and colour filters, where it has
    /// any (see the accessors): `None` for most items.
    pub extras: Option<std::rc::Rc<PaintExtras>>,
}
/// What few paint items carry besides their box, clip and effects, kept
/// apart so the common item stays small (a frame walks thousands). A
/// subtree inside the same fade mask, rounded clips and colour filters
/// shares one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintExtras {
    /// See `PaintItem::matrix`.
    pub matrix: Option<crate::affine::Affine>,
    /// See `PaintItem::mask`.
    pub mask: Option<FadeMask>,
    /// See `PaintItem::rounded`.
    pub rounded: RoundedClip,
    /// See `PaintItem::color`.
    pub color: Option<ColorMatrix>,
    /// See `PaintItem::projective`: the map in perspective, which `matrix`
    /// cannot hold (`None` where `matrix` is the whole map).
    pub projective: Option<crate::affine::Projective>,
}
impl PaintExtras {
    /// `None` for nothing to carry, else shared.
    fn shared(self) -> Option<std::rc::Rc<Self>> {
        (self != Self::default()).then(|| std::rc::Rc::new(self))
    }
}
/// What a paint item took from its ancestors in the paint walk (see
/// [`Scene::layer_items_recorded`]): their translation, opacity, clip and
/// extras, and whether it is a border pass.
#[derive(Clone, Debug)]
pub struct Inherited {
    transform: Transform,
    opacity: f32,
    clip: Option<Rect>,
    extras: Option<std::rc::Rc<PaintExtras>>,
    border: bool,
}
/// What a node that is not plain adds, in the paint walk, to what its
/// parent passed on: its own and its children's fade masks, rounded clips
/// and colour filters.
struct Own {
    mask: Option<FadeMask>,
    rounded: RoundedClip,
    parent_color: Option<ColorMatrix>,
    color: Option<ColorMatrix>,
    child_mask: Option<FadeMask>,
    child_rounded: RoundedClip,
    item_rounded: RoundedClip,
}
/// Moves `items` by `(dx, dy)`, into a layer's or a plane's space: their
/// boxes, clips, fade masks and rounded clips (not a layer's matrix, which
/// maps from its own box). Items sharing extras share the moved ones.
pub fn translate_items(items: &mut [PaintItem<'_>], dx: f32, dy: f32) {
    let shift = |r: Rect| Rect::new(r.x + dx, r.y + dy, r.width, r.height);
    let mut moved: Option<(*const PaintExtras, std::rc::Rc<PaintExtras>)> = None;
    for item in items {
        item.bounds = shift(item.bounds);
        item.clip = item.clip.map(shift);
        let Some(extras) = &item.extras else {
            continue;
        };
        if let Some((from, to)) = &moved
            && std::ptr::eq(*from, &**extras)
        {
            item.extras = Some(to.clone());
            continue;
        }
        let from = std::rc::Rc::as_ptr(extras);
        let mut shifted = PaintExtras::clone(extras);
        shifted.mask = shifted.mask.map(|(rect, bands)| (shift(rect), bands));
        for clip in shifted.rounded.iter_mut().flatten() {
            clip.rect = shift(clip.rect);
        }
        let shifted = std::rc::Rc::new(shifted);
        item.extras = Some(shifted.clone());
        moved = Some((from, shifted));
    }
}
impl PaintItem<'_> {
    /// A transformed layer: where its content, laid out in `bounds`' space,
    /// lands in this item's (see `LocalTransform::matrix`). Its `clip` is
    /// then its parent's; its own clips inside the layer. Always `None`
    /// from `paint_items`.
    /// In perspective (a tilt seen from `perspective`), the map before the
    /// tilt: `projective` has the whole map.
    pub fn matrix(&self) -> Option<crate::affine::Affine> {
        self.extras.as_ref().and_then(|extras| extras.matrix)
    }
    /// A transformed layer's whole map, perspective included (see
    /// `LocalTransform::projective`): `matrix`'s where it is affine.
    pub fn projective(&self) -> Option<crate::affine::Projective> {
        let extras = self.extras.as_ref()?;
        extras
            .projective
            .or_else(|| extras.matrix.map(crate::affine::Projective::from))
    }
    /// The nearest `fade_edges` ancestor's clip and bands (top, bottom).
    pub fn mask(&self) -> Option<FadeMask> {
        self.extras.as_ref().and_then(|extras| extras.mask)
    }
    /// Rounded clips around it (see `RoundedClip`).
    pub fn rounded(&self) -> RoundedClip {
        self.extras
            .as_ref()
            .map_or_else(RoundedClip::default, |extras| extras.rounded)
    }
    /// Its and its ancestors' colour filters as one matrix (see
    /// `Effects::color_matrix`); a layer's own, at its composite.
    pub fn color(&self) -> Option<ColorMatrix> {
        self.extras.as_ref().and_then(|extras| extras.color)
    }
}
type FontMeasurer =
    dyn Fn(&std::sync::Arc<str>, f32, Option<f32>, &crate::text_layout::FontStyle) -> (f32, f32);
type FontMinContent =
    dyn Fn(&std::sync::Arc<str>, f32, &crate::text_layout::FontStyle) -> (f32, f32);
type RichMinContent = dyn Fn(&crate::rich_text::RichText) -> (f32, f32);
/// A node `Scene::walk` visits.
#[derive(Clone, Copy, Debug)]
pub struct WalkItem {
    /// The node.
    pub id: NodeId,
    /// Levels below the node the walk started from.
    pub depth: usize,
    /// Its box as laid out, translated by its own and its ancestors'
    /// translations.
    pub bounds: Rect,
    /// The clip it paints within (its ancestors' clips and the viewport).
    pub clip: Option<Rect>,
}
/// A retained tree of nodes with their styles, laid out and painted
/// incrementally: a change invalidates only the layout, paint or
/// compositing that depends on it, and [`Scene::flush`] does that work
/// and reports the damage.
pub struct Scene {
    // One live parent with unmaterialized removals. MAX means none; that
    // possible arena index uses eager removal, so no NodeId is reserved.
    // kill/free materialize before the parent's slot can be reused.
    pending_children: std::cell::Cell<u32>,
    identity: u64,
    slots: Vec<Slot>,
    free: Vec<u32>,
    /// Slots of nodes removed and not yet freed (`kill`).
    dead: usize,
    /// Flushes since `slots` last grew (`trim_slots`).
    slots_settled: u8,
    root: NodeId,
    dirty: Vec<NodeId>,
    damage: Vec<Rect>,
    /// The last rectangles added to `damage` (each is held by one of its
    /// rectangles, which only grow): a change damages the same places again
    /// and again (before and after, then where layout moves it).
    recent_damage: [DamageEdges; 4],
    damage_grid: Option<DamageGrid>,
    /// Tests: every layout walk as list mode's (`arrange::<false>`), never
    /// the grid's, to compare the two.
    #[cfg(test)]
    cells_off: bool,
    viewport: Rect,
    /// Roots of subtrees shown in popup windows of their own (`set_popup`),
    /// and those of them promoted for it (their layer revision counts
    /// changes inside them, `Node::promoted`).
    popups: Vec<NodeId>,
    /// Each popup root's trigger, where it has one (`set_popup_anchor`).
    popup_anchors: Vec<(NodeId, NodeId)>,
    popups_promoted: Vec<NodeId>,
    blur_nodes: Vec<NodeId>,
    /// `fade_overflow` nodes, with how far their content runs past each edge
    /// (top, right, bottom, left; at most the band there).
    overflow_fades: Vec<(NodeId, [f32; 4])>,
    /// Nodes the platform paints (native views): their box is their ink.
    platform_nodes: Vec<NodeId>,
    isolation_count: usize,
    /// Nodes with a local transform: none keeps every geometry query on
    /// the translation-only paths.
    transformed: usize,
    /// Nodes with a content blur: with `transformed`, whether damage takes
    /// the placed paths (`descend`).
    filtered: std::cell::Cell<usize>,
    /// Nodes that ever had one, a content blur or a mask (`Node::promoted`).
    promoted_count: usize,
    text_measurer: Option<Box<dyn TextMeasurer>>,
    text_shaper: Option<std::rc::Rc<dyn crate::text_layout::TextShaper>>,
    font_shaper: Option<std::rc::Rc<dyn crate::text_layout::FontTextShaper>>,
    font_measurer: Option<Box<FontMeasurer>>,
    /// Plain text's min-content size as `font_measurer` measures
    /// `min_content_lines`' copy, without shaping it (`leaf_size`).
    font_min_content: Option<Box<FontMinContent>>,
    /// Rich text's, as `rich_measurer` measures `min_content_lines`' copy.
    rich_min_content: Option<Box<RichMinContent>>,
    rich_measurer: crate::rich_text::TextMeasure,
    rich_shaper: Option<Box<crate::rich_text::RichShaper>>,
    text_shaper_revision: u64,
    pending_layout_nodes: usize,
    layout_revision: u64,
    layout_pass: u64,
    /// How deep the measure running now is (`crate::deep`).
    measure_depth: u32,
    /// Measures' working sets, one per level deep (`Scratch`).
    scratch: Vec<Scratch>,
    content_revision: u64,
    /// The deepest stack and the item count of the last paint walk
    /// (`layer_items_within`): the next one starts at those sizes.
    walk_hints: std::cell::Cell<(usize, usize)>,
    /// Dirty fixed-size containers whose ancestors stayed clean.
    layout_roots: Vec<NodeId>,
    /// How deep the contents syncs running now nest (`sync_contents`).
    syncing: u32,
    /// Nodes holding more than `MEASUREMENT_ALTERNATES` results of this pass,
    /// trimmed when it ends.
    overfull: Vec<NodeId>,
    /// Nodes with `Style::relative`: none keeps every absolute child placed
    /// by its parent, with no ancestor walk.
    relative_count: usize,
    /// How many nodes are contents nodes (`set_contents`) or zoomed
    /// (`set_zoom`), their `special` byte set: with none, style and child
    /// changes skip looking for either.
    contents_count: usize,
    /// Absolute nodes whose containing block is an ancestor above their
    /// parent: their parents leave them out, and `place_escaped` positions
    /// them once the block is laid out. May hold duplicates and stale ids.
    escaped: Vec<NodeId>,
    /// `measure` calls, for tests of how much work layout does.
    #[cfg(test)]
    measure_calls: u64,
    /// While a mount defers its text inputs' layout: the observer running
    /// now, whose geometry reads (`bounds` and the boxes made from it) are
    /// kept with what they found (`geometry_reads`), to run it again if
    /// layout moves them (`changed_geometry_readers`).
    geometry_reader: std::cell::Cell<Option<u64>>,
    geometry_reads: std::cell::RefCell<Vec<(u64, NodeId, Rect)>>,
    geometry_revision: u64,
    cursor_styles: usize,
    /// Scroll content translated since the last flush: its transform before
    /// the first move, and whether it moved again (see `resolve_moves`).
    pending_moves: Vec<(NodeId, Transform, bool)>,
    /// Device pixels a point (`set_pixel_scale`): scroll content moves by
    /// whole ones. 0 until a host says.
    pixel_scale: f32,
    /// The moves reported by the last flush, and that flush's serial.
    moves: Vec<ScrollMove>,
    flush_serial: u64,
    /// Nodes flushes repainted since a renderer last read them, with what
    /// changed (`repaints`).
    repaints: Vec<(NodeId, u8)>,
    /// The first and last flush `repaints` covers, counting every flush.
    repaints_from: u64,
    repaint_flushes: u64,
    /// Whether a renderer read `repaints`: the next flush starts afresh.
    repaints_read: std::cell::Cell<bool>,
    /// Flushes since a renderer last read `repaints`: past a few, it no
    /// longer patches, and flushes stop keeping them.
    repaints_unread: u8,
    /// The styles contents nodes were given (`set_contents`), where not the
    /// default: their own paint, clip and margins; the rest of what lays
    /// them out is their last child's.
    contents_styles: NodeMap<Style>,
    /// The style a node was given, where it lays out by another
    /// (`laid_out_style`: it runs right to left, or sets a line's start or
    /// end): `Scene::style` returns it.
    given_styles: NodeMap<Style>,
    /// Directions nodes set themselves (`set_direction`); others inherit.
    directions: NodeMap<Direction>,
    /// Layer shaders and their uniforms, by node (`set_layer_shader`):
    /// usually none.
    layer_shaders: NodeMap<LayerShader>,
    /// Subtrees hit testing skips (`set_inert`): usually none.
    inert: Vec<NodeId>,
    /// Text selection's settings and highlights (`set_user_select`).
    selectable: select::Selectable,
    /// Canvas views and grid fills (`scene_view`).
    views: view::Views,
}
/// A visual line of text, for assistive technology (`Scene::text_lines`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Its bytes in the node's text, its break left out.
    pub range: std::ops::Range<usize>,
    /// Its box, in the window's points.
    pub rect: Rect,
    /// Where each grapheme boundary in `range` falls across the window,
    /// its end's included: a grapheme's left and right edges (right to
    /// left, they decrease).
    pub carets: Vec<f32>,
    /// It runs right to left.
    pub rtl: bool,
}
/// Scroll content that moved rigidly by `(dx, dy)` within `clip`. The flush
/// damaged only what copying the old pixels cannot provide (`exposed`, plus
/// every other change inside the clip at both positions). A renderer that
/// cannot copy must repaint `clip` as well.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollMove {
    /// The scroll view.
    pub node: NodeId,
    /// The area its content moved within.
    pub clip: Rect,
    /// How far it moved across.
    pub dx: f32,
    /// How far it moved down.
    pub dy: f32,
    /// `flush_serial` of the flush that reported it: the copy is valid only
    /// over the frame rendered from the previous flush.
    pub serial: u64,
}
impl Scene {
    /// A scene with only its root, a `width` by `height` viewport.
    pub fn new(width: f32, height: f32) -> Self {
        let root = NodeId {
            index: 0,
            generation: 0,
        };
        Self {
            pending_children: std::cell::Cell::new(u32::MAX),
            identity: NEXT_SCENE_ID
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("scene identity exhausted"),
            slots: vec![Slot {
                generation: 0,
                dead: false,
                node: Some(Node {
                    cursor: None,
                    parent: None,
                    kind: NodeKind::Container(Layout::Column),
                    head: NodeHead {
                        font: Default::default(),
                        children: std::cell::UnsafeCell::new(Vec::new()),
                    },
                    style: Style {
                        width: Some(width),
                        height: Some(height),
                        clip: true,
                        ..Style::default()
                    },
                    bounds: Rect::default(),
                    transform: Transform::default(),
                    effects: Effects::default(),
                    dirty: LAYOUT | PAINT,
                    queued: true,
                    measurement: None,
                    alternates: Vec::new(),
                    sizes: Vec::new(),
                    ink: std::cell::Cell::new(None),
                    bounds_valid: std::cell::Cell::new(0),
                    text_size: Default::default(),
                    advanced_layout: None,
                    arrange_dirty: true,
                    reads: std::cell::Cell::new(0),
                    isolated: false,
                    local: None,
                    image_mask: None,
                    layer_shaded: false,
                    layered: false,
                    blend: BlendMode::Normal,
                    clip_inset: None,
                    shader_uniforms: None,
                    promoted: false,
                    scroll_copy: false,
                    special: Special::None,
                    replaced: false,
                    rtl: false,
                    align_logical: None,
                    layer_revision: 0,
                    style_writes: 0,
                }),
            }],
            free: Vec::new(),
            dead: 0,
            slots_settled: 0,
            root,
            dirty: vec![root],
            damage: Vec::new(),
            recent_damage: [DamageEdges::default(); 4],
            damage_grid: None,
            #[cfg(test)]
            cells_off: false,
            viewport: Rect::new(0.0, 0.0, width, height),
            popups: Vec::new(),
            popup_anchors: Vec::new(),
            popups_promoted: Vec::new(),
            blur_nodes: Vec::new(),
            overflow_fades: Vec::new(),
            platform_nodes: Vec::new(),
            pending_moves: Vec::new(),
            pixel_scale: 0.,
            moves: Vec::new(),
            flush_serial: 0,
            repaints: Vec::new(),
            repaints_from: 1,
            repaint_flushes: 0,
            repaints_read: std::cell::Cell::new(false),
            repaints_unread: REPAINTS_UNREAD,
            isolation_count: 0,
            transformed: 0,
            filtered: std::cell::Cell::new(0),
            promoted_count: 0,
            text_measurer: None,
            text_shaper: None,
            font_shaper: None,
            font_measurer: None,
            font_min_content: None,
            rich_min_content: None,
            rich_measurer: Default::default(),
            rich_shaper: None,
            text_shaper_revision: 0,
            pending_layout_nodes: 0,
            layout_revision: 0,
            layout_pass: 0,
            measure_depth: 0,
            scratch: Vec::new(),
            content_revision: 0,
            walk_hints: Default::default(),
            layout_roots: Vec::new(),
            syncing: 0,
            overfull: Vec::new(),
            relative_count: 0,
            contents_count: 0,
            escaped: Vec::new(),
            #[cfg(test)]
            measure_calls: 0,
            geometry_reader: Default::default(),
            geometry_reads: Default::default(),
            geometry_revision: 0,
            cursor_styles: 0,
            contents_styles: Default::default(),
            given_styles: Default::default(),
            directions: Default::default(),
            layer_shaders: Default::default(),
            inert: Vec::new(),
            selectable: Default::default(),
            views: Default::default(),
        }
    }
    /// Make `id`'s subtree invisible to hit testing (and so to pointer
    /// input and cursors): it paints but the pointer reaches what is under
    /// it. For overlays that annotate the interface, such as the layout
    /// inspector. Few nodes are inert; none keeps hit testing as it was.
    pub fn set_inert(&mut self, id: NodeId, inert: bool) {
        if !self.contains(id) {
            return;
        }
        let known = self.inert.contains(&id);
        if inert && !known {
            self.inert.push(id);
        } else if !inert && known {
            self.inert.retain(|n| *n != id);
        }
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
    }
    /// Whether `id` is inert (`set_inert`): hit testing passes through it.
    pub fn is_inert(&self, id: NodeId) -> bool {
        self.inert.contains(&id)
    }
    /// Show `id`'s subtree in a popup window of its own rather than in this
    /// scene's window (a menu or popover reaching past the window's edges):
    /// the window's paint walk (`layer_items(None)`) leaves it out, a popup
    /// renderer draws it alone (`layer_items(Some(id))`), and it is hit past
    /// the viewport. Its layer revision (`layer_revision`) counts changes
    /// inside it, which its popup window draws again.
    pub fn set_popup(&mut self, id: NodeId, popup: bool) {
        if !self.contains(id) || popup == self.popups.contains(&id) {
            return;
        }
        // Where it painted, then where it paints now.
        self.damage_subtree(id);
        if popup {
            self.popups.push(id);
            if !std::mem::replace(&mut self.node_mut(id).promoted, true) {
                self.promoted_count += 1;
                self.popups_promoted.push(id);
            }
        } else {
            self.popups.retain(|n| *n != id);
            self.popup_anchors.retain(|(root, _)| *root != id);
            if self.popups_promoted.contains(&id) {
                self.popups_promoted.retain(|n| *n != id);
                self.node_mut(id).promoted = false;
                self.promoted_count -= 1;
            }
        }
        self.damage_subtree(id);
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.invalidate(id, COMPOSITE);
    }
    /// Roots shown in popup windows (`set_popup`), in the order set.
    pub fn popups(&self) -> &[NodeId] {
        &self.popups
    }
    /// The trigger popup root `id` opens against (a menu's button), for a
    /// host whose platform places popups itself (Wayland's
    /// `xdg_positioner`), which then flips one at the screen's edge around
    /// its trigger. Kept while `id` is a popup root (`set_popup`).
    pub fn set_popup_anchor(&mut self, id: NodeId, anchor: NodeId) {
        if !self.popups.contains(&id) {
            return;
        }
        self.popup_anchors.retain(|(root, _)| *root != id);
        self.popup_anchors.push((id, anchor));
    }
    /// Popup root `id`'s trigger (`set_popup_anchor`), while both are in
    /// the scene.
    pub fn popup_anchor(&self, id: NodeId) -> Option<NodeId> {
        self.popup_anchors
            .iter()
            .find(|(root, _)| *root == id)
            .map(|(_, anchor)| *anchor)
            .filter(|anchor| self.contains(*anchor))
    }
    /// What popup root `id`'s subtree paints, in window coordinates (past
    /// the viewport as it may be); `None` while it shows nothing (it or what
    /// holds it is hidden or transparent).
    pub fn popup_bounds(&self, id: NodeId) -> Option<Rect> {
        self.settle_children();
        if !self.contains(id) || !self.layout_visible(id) || self.world(id).1.opacity <= 0. {
            return None;
        }
        let mut offset = Transform::default();
        let mut parent = self.node(id).parent;
        while let Some(ancestor) = parent {
            let node = self.node(ancestor);
            offset.x += node.transform.x;
            offset.y += node.transform.y;
            parent = node.parent;
        }
        self.ink(id)
            .map(|ink| ink.translated(offset))
            .filter(|ink| ink.width > 0. && ink.height > 0.)
    }
    /// Whether `id` shows in a popup window of its own (`set_popup`).
    pub fn is_popup(&self, id: NodeId) -> bool {
        !self.popups.is_empty() && self.popups.contains(&id)
    }
    /// Stable identity separates renderer caches when a whole scene is replaced.
    pub fn identity(&self) -> u64 {
        self.identity
    }
    /// The root node, which always exists.
    pub fn root(&self) -> NodeId {
        self.root
    }
    /// Mark `id` as painted by the platform (a native view over its box):
    /// its box counts as its ink, so damage culling and paint walks within
    /// regions reach it though zgx draws nothing there.
    pub fn set_platform_painted(&mut self, id: NodeId, painted: bool) {
        if !self.contains(id) {
            return;
        }
        let present = self.platform_nodes.contains(&id);
        if present == painted {
            return;
        }
        let slots = &self.slots;
        self.platform_nodes.retain(|node| {
            *node != id
                && slots
                    .get(node.index as usize)
                    .is_some_and(|s| s.generation == node.generation && s.node.is_some())
        });
        if painted {
            self.platform_nodes.push(id);
        }
        self.invalidate(id, COMPOSITE);
    }
    /// Whether `id` is painted by the platform (`set_platform_painted`).
    pub fn is_platform_painted(&self, id: NodeId) -> bool {
        self.platform_nodes.contains(&id)
    }
    /// Whether `id` is still in the scene.
    pub fn contains(&self, id: NodeId) -> bool {
        self.slots
            .get(id.index as usize)
            .is_some_and(|s| s.generation == id.generation && s.node.is_some())
    }
    /// The number of nodes, the root included.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len() - self.dead
    }
    /// The ids of the nodes in the scene, in slot order.
    fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            (slot.node.is_some() && !slot.dead).then_some(NodeId {
                index: index as u32,
                generation: slot.generation,
            })
        })
    }
    /// Layout and paint walk the tree from its root, which reaches no
    /// removed node: in debug builds, check it.
    #[inline(always)]
    fn visit(&self, id: NodeId) {
        debug_assert!(
            !self.slots[id.index as usize].dead,
            "layout or paint reached a removed node"
        );
    }
    /// Always false: a scene keeps its root.
    pub fn is_empty(&self) -> bool {
        false
    } // A scene always retains its root.
    fn node(&self, id: NodeId) -> &Node {
        assert!(self.contains(id), "stale scene node");
        self.slots[id.index as usize].node.as_ref().unwrap()
    }
    /// A node the tree itself led to (a live node's parent, an id checked
    /// just before): in the scene by the tree's invariant, so its
    /// generation is checked in debug builds only. Hot walks (`world`,
    /// `bounds`, `world_clip`, `shown`, `flush`'s dirty nodes) take it.
    #[inline(always)]
    fn tree_node(&self, id: NodeId) -> &Node {
        let slot = &self.slots[id.index as usize];
        debug_assert!(slot.generation == id.generation, "stale scene node");
        slot.node.as_ref().unwrap_or_else(|| stale())
    }
    /// `tree_node`, to change.
    #[inline(always)]
    fn tree_node_mut(&mut self, id: NodeId) -> &mut Node {
        let slot = &mut self.slots[id.index as usize];
        debug_assert!(slot.generation == id.generation, "stale scene node");
        slot.node.as_mut().unwrap_or_else(|| stale())
    }
    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        assert!(self.contains(id), "stale scene node");
        self.slots[id.index as usize].node.as_mut().unwrap()
    }
    /// `id`'s box as laid out, translated by its own and its ancestors'
    /// translations, in window points.
    ///
    /// Read by an observer while a mount lays out its text inputs (from a
    /// reactive style or a binding), it is a dependency, as is every box made
    /// from it (`layout_bounds`, `visible_bounds`, `transformed_bounds`):
    /// the observer runs again once the mount's layout settles, if the box
    /// has moved. Derive from it each time, rather than keeping a first read.
    #[inline]
    pub fn bounds(&self, id: NodeId) -> Rect {
        self.note_geometry_read(id);
        self.world_bounds(id)
    }
    /// `bounds`, read by the scene's and the frame's own work: noted by no
    /// mount.
    pub(crate) fn world_bounds(&self, id: NodeId) -> Rect {
        // `world`'s bounds, without the opacity and clip it also works out:
        // observed bounds are read for every observed node on every frame
        // that moves anything.
        let node = self.node(id);
        let mut transform = node.transform;
        let mut parent = node.parent;
        while let Some(id) = parent {
            let ancestor = self.tree_node(id);
            transform.x += ancestor.transform.x;
            transform.y += ancestor.transform.y;
            parent = ancestor.parent;
        }
        node.bounds.translated(transform)
    }
    /// Where layout put `id`, in window coordinates, before any translation
    /// (its own or its ancestors', scrolling included): what moves only when
    /// layout moves it.
    #[inline]
    pub fn layout_bounds(&self, id: NodeId) -> Rect {
        self.note_geometry_read(id);
        self.node(id).bounds
    }
    /// `layout_bounds`, read by the frame's own work: noted by no mount.
    pub(crate) fn layout_box(&self, id: NodeId) -> Rect {
        self.node(id).bounds
    }
    /// A read of `id`'s geometry by the observer a mount runs now, if one
    /// is (`geometry_reader`).
    #[inline(always)]
    fn note_geometry_read(&self, id: NodeId) {
        if let Some(effect) = self.geometry_reader.get() {
            self.keep_geometry_read(effect, id);
        }
    }
    #[cold]
    #[inline(never)]
    fn keep_geometry_read(&self, effect: u64, id: NodeId) {
        let bounds = self.world_bounds(id);
        self.geometry_reads.borrow_mut().push((effect, id, bounds));
    }
    /// The observer whose geometry reads are kept from now on (see
    /// `geometry_reader`); `None`: none.
    pub(crate) fn read_geometry_for(&self, effect: Option<u64>) {
        self.geometry_reader.set(effect);
    }
    /// The observers kept reading geometry that layout has moved since,
    /// oldest first: each once. The reads are let go.
    pub(crate) fn changed_geometry_readers(&self) -> Vec<u64> {
        let reads = std::mem::take(&mut *self.geometry_reads.borrow_mut());
        let mut changed: Vec<u64> = reads
            .into_iter()
            .filter(|&(_, id, seen)| !self.contains(id) || self.world_bounds(id) != seen)
            .map(|(effect, _, _)| effect)
            .collect();
        changed.sort_unstable();
        changed.dedup();
        changed
    }
    /// Whether `id` itself (not its ancestors) takes part in layout.
    pub(crate) fn shown_in_layout(&self, id: NodeId) -> bool {
        self.contains(id) && own_visible(&self.node(id).style)
    }
    /// Visible allocation after ancestor opacity, clipping and viewport intersection.
    /// This does not attempt occlusion testing against overlapping siblings.
    /// Layout visibility excludes hidden/display-none nodes and their descendants.
    /// Mounted ownership is retained; opacity remains an independent paint effect.
    pub fn layout_visible(&self, id: NodeId) -> bool {
        self.contains(id)
            && std::iter::once(id)
                .chain(self.ancestors(id))
                .all(|id| own_visible(&self.node(id).style))
    }
    /// Where `id`'s box shows exactly as laid out, in window points: its box
    /// cut by its clips, when nothing in its ancestry changes how it
    /// composites (no opacity below 1, filter, edge fade, blend, mask, layer
    /// shader, scale, rotation, skew, tilt or zoom; nor in `id` itself, but
    /// for its opacity) and no rounded clip reaches it. A host drawing the node
    /// itself (the caret, `Ui::native_caret`) draws that rectangle, as the
    /// node's paint would show; `None` when it can't, or nothing shows.
    pub fn plain_box(&self, id: NodeId) -> Option<Rect> {
        let plain = |node: &Node| {
            !node.layered && !node.filtered() && !node.transformed() && own_visible(&node.style)
        };
        let node = self.node(id);
        let effects = Effects {
            opacity: 1.,
            ..node.effects
        };
        if !plain(node) || effects != Effects::default() {
            return None;
        }
        let bounds = self.bounds(id).intersection(self.world_clip(id)?)?;
        let inside = |outer: Rect| {
            bounds.x >= outer.x
                && bounds.y >= outer.y
                && bounds.x + bounds.width <= outer.x + outer.width
                && bounds.y + bounds.height <= outer.y + outer.height
        };
        for ancestor in self.ancestors(id) {
            let node = self.tree_node(ancestor);
            if !plain(node) || node.effects != Effects::default() {
                return None;
            }
            // A rounded clip cuts its corners: the box keeps clear of them.
            if clip_axes(&node.style) == (true, true)
                && let NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } = &node.kind
            {
                let corners = quad
                    .decoration
                    .as_deref()
                    .and_then(|detail| detail.corners)
                    .unwrap_or(crate::decoration::Corners::all(quad.radius));
                let radius = corners
                    .top_left
                    .max(corners.top_right)
                    .max(corners.bottom_right)
                    .max(corners.bottom_left)
                    .max(0.);
                let feather = quad
                    .decoration
                    .as_deref()
                    .map_or(0., |detail| detail.clip_feather);
                if radius > 0. || feather > 0. {
                    let outer = self.bounds(ancestor);
                    let inset = radius + feather + quad.border_width.max(0.);
                    let across = Rect::new(
                        outer.x + inset,
                        outer.y,
                        outer.width - 2. * inset,
                        outer.height,
                    );
                    let down = Rect::new(
                        outer.x,
                        outer.y + inset,
                        outer.width,
                        outer.height - 2. * inset,
                    );
                    if !inside(across) && !inside(down) {
                        return None;
                    }
                }
            }
        }
        Some(bounds)
    }
    /// Whether anything painted after `id` (its later siblings', and its
    /// ancestors' later siblings' subtrees) can land on `rect`, in window
    /// points: what a host drawing `id` above the window's content would
    /// wrongly cover.
    pub fn painted_over(&self, id: NodeId, rect: Rect) -> bool {
        self.settle_children();
        let mut node = id;
        while let Some(parent) = self.node(node).parent {
            // Ink is in the parent's space: its ancestors' translations added.
            let mut offset = Transform::default();
            let mut up = Some(parent);
            while let Some(at) = up {
                let ancestor = self.tree_node(at);
                offset.x += ancestor.transform.x;
                offset.y += ancestor.transform.y;
                up = ancestor.parent;
            }
            let children = self.children_raw(self.tree_node(parent));
            let after = children
                .iter()
                .position(|child| *child == node)
                .map_or(0, |at| at + 1);
            for later in &children[after..] {
                if self
                    .ink(*later)
                    .is_some_and(|ink| ink.translated(offset).intersects(rect))
                {
                    return true;
                }
            }
            node = parent;
        }
        false
    }
    /// The part of `id`'s box that shows: inside its clips and not fully
    /// transparent; `None` when nothing does.
    pub fn visible_bounds(&self, id: NodeId) -> Option<Rect> {
        self.note_geometry_read(id);
        let (bounds, effects, clip) = self.world(id);
        if effects.opacity <= 0. {
            return None;
        }
        if self.transformed > 0 {
            let (bounds, clip, placement) = self.placed(id);
            let visible = bounds.intersection(clip.unwrap_or(UNCLIPPED))?;
            return match placement {
                Some(placement) => placement
                    .matrix
                    .bounds(visible)
                    .intersection(placement.clip),
                None => Some(visible),
            };
        }
        bounds.intersection(clip.unwrap_or(self.viewport))
    }
    /// Where `id`'s box shows: the viewport-aligned box around its corners
    /// after its own and its ancestors' local transforms, unclipped. `bounds`
    /// is the box as laid out and translated.
    pub fn transformed_bounds(&self, id: NodeId) -> Rect {
        self.note_geometry_read(id);
        self.transformed_world_bounds(id)
    }
    /// Where a layer's item (`PaintItem::isolated`) can paint, in window
    /// points: its whole subtree's ink, after its transform. An item of the
    /// window's paint walk, whose ancestors translate it alone. Renderer
    /// plumbing, not a stable API.
    #[doc(hidden)]
    pub fn layer_ink(&self, item: &PaintItem<'_>) -> Option<Rect> {
        let node = self.node(item.id);
        let ink = self.ink(item.id)?;
        Some(ink.translated(Transform {
            x: item.bounds.x - node.bounds.x - node.transform.x,
            y: item.bounds.y - node.bounds.y - node.transform.y,
        }))
    }
    /// `transformed_bounds`, read by the scene's and the frame's own work:
    /// noted by no mount.
    pub(crate) fn transformed_world_bounds(&self, id: NodeId) -> Rect {
        if self.transformed == 0 {
            return self.world_bounds(id);
        }
        let (bounds, _, placement) = self.placed(id);
        placement.map_or(bounds, |placement| placement.matrix.bounds(bounds))
    }
    /// `id`'s translated box, the clip it paints within (in its layer's
    /// space; `None` cuts nothing) and its layer's placement, if transformed.
    fn placed(&self, id: NodeId) -> (Rect, Option<Rect>, Option<Placement>) {
        let (transform, clip, placement) = match self.node(id).parent {
            Some(parent) => self.context(parent),
            None => (Transform::default(), Some(self.viewport), None),
        };
        let (_, bounds, clip, placement) = self.descend(self.node(id), transform, clip, placement);
        (bounds, clip, placement)
    }
    /// `id`'s scale, rotation, skew and origin.
    pub fn local_transform(&self, id: NodeId) -> LocalTransform {
        self.node(id).local()
    }
    /// Set scale, rotation, skew and origin, normalizing each nonfinite
    /// component to its identity value. Changes composite only: the subtree
    /// is drawn as a cached layer, damaged where it was and where it lands.
    pub fn set_local_transform(&mut self, id: NodeId, transform: LocalTransform) {
        let transform = transform.normalized();
        let node = self.node(id);
        // Untransformed and asked for none (most calls): against the
        // identity, a constant.
        let (old, unchanged) = match node.local.as_deref() {
            Some(own) => (own.transform, own.transform == transform),
            None => (
                LocalTransform::IDENTITY,
                transform == LocalTransform::IDENTITY,
            ),
        };
        if unchanged {
            return;
        }
        if old.is_identity() && transform.is_identity() {
            // Only the origin of nothing moved.
            self.node_mut(id).set_local(transform);
            return;
        }
        self.damage_subtree(id);
        // A zoomed node is transformed either way (`Node::transformed`).
        // Checked above.
        if old.is_identity() != transform.is_identity() && self.tree_node(id).zoom() == 1. {
            if transform.is_identity() {
                self.transformed -= 1;
            } else {
                self.transformed += 1;
            }
        }
        let node = self.node_mut(id);
        node.set_local(transform);
        if !std::mem::replace(&mut node.promoted, true) {
            self.promoted_count += 1;
        }
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.damage_subtree(id);
        self.invalidate(id, COMPOSITE);
    }
    /// `id`'s zoom (`set_zoom`): 1 unless set.
    pub fn zoom(&self, id: NodeId) -> f32 {
        self.node(id).zoom()
    }
    /// How much `id` shows zoomed: its own zoom times its ancestors'. A
    /// native view's content follows it (a page's zoom, a terminal's text).
    pub fn zoom_around(&self, id: NodeId) -> f32 {
        // A zoomed node is a transformed one.
        if self.transformed == 0 {
            return 1.;
        }
        std::iter::once(id)
            .chain(self.ancestors(id))
            .map(|id| self.node(id).zoom())
            .product()
    }
    /// Zoom `id`'s subtree by `zoom`, as CSS's `zoom`: it lays out in its own
    /// points, under its parent's room divided by `zoom`, and takes `zoom`
    /// times the room it needs there; it paints scaled about its top-left,
    /// as a layer rasterized at the scale it shows at (crisp, not
    /// resampled). Its own box (`bounds`) is in its own points;
    /// `transformed_bounds` gives where it shows. Hit testing, damage and
    /// clips follow it as a local transform's do. Nonfinite or nonpositive
    /// zooms are 1.
    pub fn set_zoom(&mut self, id: NodeId, zoom: f32) {
        let zoom = if zoom.is_finite() && zoom > 0. {
            zoom
        } else {
            1.
        };
        if self.node(id).zoom() == zoom {
            return;
        }
        let transform = self.node(id).local();
        self.set_local_parts(id, transform, zoom);
        let zoomed = zoom != 1.;
        if self.node(id).special.has(ZOOMED) != zoomed {
            self.set_special(id, ZOOMED, zoomed);
            // Laid out as `snap` while zoomed (`restyle_special`,
            // `sync_contents`).
            if self.node(id).contents() {
                self.sync_contents(id);
            } else {
                let given = self.style(id);
                self.restyle(id, given, true);
            }
        }
        // Its room and the room it takes change: laid out again.
        self.invalidate_layout(id);
        // So do the contents parents it lays out as, each its parent's last
        // child: sized as it is styled (`sync_contents`), they take no
        // change from it otherwise.
        let mut lead = id;
        while self.contents_count > 0
            && let Some(parent) = self.node(lead).parent
            && self.node(parent).contents()
            && self.children_of(self.node(parent)).last() == Some(&lead)
        {
            self.invalidate_layout(parent);
            lead = parent;
        }
        self.invalidate(id, COMPOSITE);
    }
    /// Give `id` `transform` and `zoom`, damaging its subtree where it
    /// showed and where it shows, and counting it as transformed.
    fn set_local_parts(&mut self, id: NodeId, transform: LocalTransform, zoom: f32) {
        self.damage_subtree(id);
        let was = self.node(id).transformed();
        let node = self.node_mut(id);
        node.set_local_and_zoom(transform, zoom);
        let is = node.transformed();
        if !std::mem::replace(&mut node.promoted, true) {
            self.promoted_count += 1;
        }
        match (was, is) {
            (false, true) => self.transformed += 1,
            (true, false) => self.transformed -= 1,
            _ => {}
        }
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.damage_subtree(id);
    }
    /// Whether `id` paints as a cached layer: isolated, transformed, blurred
    /// (`Effects::filter_blur`) or masked.
    #[inline]
    pub fn is_layer(&self, id: NodeId) -> bool {
        let node = self.node(id);
        node.isolated || node.transformed() || node.effect_layer()
    }
    /// `id`'s content blur (`Effects::filter_blur`).
    pub fn filter_blur(&self, id: NodeId) -> f32 {
        self.node(id).effects.filter_blur
    }
    /// Set `id`'s content blur (`Effects::filter_blur`), keeping its other
    /// effects. Composites only: damaged where the blur reached and reaches.
    pub fn set_filter_blur(&mut self, id: NodeId, sigma: f32) {
        let effects = Effects {
            filter_blur: sigma,
            ..self.node(id).effects
        };
        self.set_effects(id, effects);
    }
    /// `id`'s mask, if it has one.
    pub fn mask(&self, id: NodeId) -> Option<&ImageMask> {
        self.node(id).image_mask.as_deref()
    }
    /// Mask `id`'s subtree (see `ImageMask`), or with `None` show it whole.
    /// Nonfinite offsets and sizes are zeroed. A masked node paints as a
    /// cached layer: changing the mask composites again, damaging the
    /// subtree, without repainting it.
    pub fn set_mask(&mut self, id: NodeId, mask: Option<ImageMask>) {
        let mask = mask.map(ImageMask::normalized);
        let old = self.node(id).image_mask.as_deref();
        if old == mask.as_ref() {
            return;
        }
        // A renderer keeps mask textures for the images nodes show.
        if old.map(|old| old.image.id()) != mask.as_ref().map(|mask| mask.image.id()) {
            self.content_revision = self.content_revision.wrapping_add(1);
        }
        // A mask only hides: what it reveals or hides lies within what the
        // subtree paints anyway.
        self.damage_subtree(id);
        let node = self.node_mut(id);
        node.image_mask = mask.map(Box::new);
        node.relayer();
        if node.image_mask.is_some() && !std::mem::replace(&mut node.promoted, true) {
            self.promoted_count += 1;
        }
        self.invalidate(id, COMPOSITE);
    }
    /// How `id` mixes with its backdrop.
    pub fn blend(&self, id: NodeId) -> BlendMode {
        self.node(id).blend
    }
    /// Mix `id`'s subtree with its backdrop by `mode` (see `BlendMode`). A
    /// blended node paints as a cached layer: changing the mode composites
    /// again, damaging the subtree, without repainting it.
    pub fn set_blend(&mut self, id: NodeId, mode: BlendMode) {
        if self.node(id).blend == mode {
            return;
        }
        self.damage_subtree(id);
        let node = self.node_mut(id);
        node.blend = mode;
        node.relayer();
        if mode != BlendMode::Normal && !std::mem::replace(&mut node.promoted, true) {
            self.promoted_count += 1;
        }
        self.invalidate(id, COMPOSITE);
    }
    /// `id`'s layer shader and its uniforms, if either is set.
    pub fn layer_shader(&self, id: NodeId) -> Option<&LayerShader> {
        if self.layer_shaders.is_empty() {
            return None;
        }
        self.layer_shaders.get(&id)
    }
    /// Composite `id`'s subtree through `shader` (see `LayerShader`), or
    /// with `None` as it is, keeping its uniforms. A node with a layer
    /// shader paints as a cached layer: changing the shader composites
    /// again, damaging the subtree, without repainting it.
    pub fn set_layer_shader(
        &mut self,
        id: NodeId,
        shader: Option<Arc<crate::decoration::FillShader>>,
    ) {
        let uniforms = self
            .layer_shader(id)
            .map(|layer| layer.uniforms)
            .unwrap_or_default();
        self.set_layer(id, LayerShader { shader, uniforms });
    }
    /// Set the uniforms `id`'s layer shader draws with: its time (quantized
    /// to the shader's frame rate) and user parameters. Composites only, as
    /// `set_mask` does. Nonfinite values are zeroed.
    pub fn set_layer_uniforms(
        &mut self,
        id: NodeId,
        mut uniforms: crate::decoration::ShaderUniforms,
    ) {
        fn finite(value: f32) -> f32 {
            if value.is_finite() { value } else { 0. }
        }
        uniforms.time = finite(uniforms.time);
        uniforms.params = uniforms.params.map(|row| row.map(finite));
        let shader = self.layer_shader(id).and_then(|layer| layer.shader.clone());
        if let Some(fps) = shader.as_deref().and_then(|shader| shader.fps()) {
            uniforms.time = (uniforms.time * fps).floor() / fps;
        }
        self.set_layer(id, LayerShader { shader, uniforms });
    }
    fn set_layer(&mut self, id: NodeId, layer: LayerShader) {
        let old = self.layer_shader(id);
        if old.map_or(layer == LayerShader::default(), |old| *old == layer) {
            return;
        }
        // What it shades lies within what the subtree paints anyway.
        if old.is_some_and(|old| old.shader.is_some()) || layer.shader.is_some() {
            self.damage_subtree(id);
        }
        let shaded = layer.shader.is_some();
        if layer == LayerShader::default() {
            self.layer_shaders.remove(&id);
        } else {
            self.layer_shaders.insert(id, layer);
        }
        let node = self.node_mut(id);
        node.layer_shaded = shaded;
        node.relayer();
        if shaded && !std::mem::replace(&mut node.promoted, true) {
            self.promoted_count += 1;
        }
        self.invalidate(id, COMPOSITE);
    }
    /// `id`'s shader uniforms (see `crate::decoration::FillShader`). An
    /// image with none of its own draws with its view's: the nearest of the
    /// boxes wrapping it alone (its view's, an element's) that has some, as
    /// styles and animations set them on those (`ImageData::characters`'
    /// reveal). Only painting nothing and holding it alone: never another
    /// node's fill.
    pub fn shader_uniforms(&self, id: NodeId) -> crate::decoration::ShaderUniforms {
        let node = self.node(id);
        if let Some(uniforms) = node.shader_uniforms.as_deref() {
            return *uniforms;
        }
        if matches!(node.kind, NodeKind::Image(_)) {
            let mut at = node.parent;
            while let Some(parent) = at.map(|id| self.node(id)) {
                if !matches!(parent.kind, NodeKind::Container(_))
                    || self.children_of(parent).len() != 1
                {
                    break;
                }
                if let Some(uniforms) = parent.shader_uniforms.as_deref() {
                    return *uniforms;
                }
                at = parent.parent;
            }
        }
        Default::default()
    }
    /// Set the uniforms `id`'s shader fill draws with: its time (quantized
    /// to the shader's frame rate, so a capped shader repaints only when
    /// it steps) and user parameters. Repaints that box alone; no layout,
    /// nothing else. Nonfinite values are zeroed.
    pub fn set_shader_uniforms(
        &mut self,
        id: NodeId,
        mut uniforms: crate::decoration::ShaderUniforms,
    ) {
        fn finite(value: f32) -> f32 {
            if value.is_finite() { value } else { 0. }
        }
        uniforms.time = finite(uniforms.time);
        uniforms.params = uniforms.params.map(|row| row.map(finite));
        let fps = match &self.node(id).kind {
            NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad
                .decoration
                .as_deref()
                .and_then(|detail| match &detail.background {
                    Some(crate::decoration::Background::Shader(shader)) => shader.fps(),
                    _ => None,
                }),
            _ => None,
        };
        if let Some(fps) = fps {
            uniforms.time = (uniforms.time * fps).floor() / fps;
        }
        // Against its own: an image's borrowed ones (`shader_uniforms`) are
        // not its own, and setting those makes them so.
        let node = self.node(id);
        let unchanged = match node.shader_uniforms.as_deref() {
            Some(own) => *own == uniforms,
            None => uniforms == Default::default() && !matches!(node.kind, NodeKind::Image(_)),
        };
        if unchanged {
            return;
        }
        self.node_mut(id).shader_uniforms = Some(Box::new(uniforms));
        self.invalidate(id, PAINT);
        // The image drawing with them, if this box lends it them: its own
        // cached layer (an isolated or promoted image) repaints too.
        let mut at = id;
        loop {
            let node = self.node(at);
            if !matches!(node.kind, NodeKind::Container(_)) || self.children_of(node).len() != 1 {
                break;
            }
            let child = self.children_of(node)[0];
            let inner = self.node(child);
            if inner.shader_uniforms.is_some() {
                break;
            }
            if matches!(inner.kind, NodeKind::Image(_)) {
                self.invalidate(child, PAINT);
                break;
            }
            at = child;
        }
    }
    /// `id`'s clip inset, if it has one.
    pub fn clip_inset(&self, id: NodeId) -> Option<ClipInset> {
        self.node(id).clip_inset.as_deref().copied()
    }
    /// Whether `id` has `inset`, `transform` and `local` already, compared
    /// as `set_clip_inset`, `set_transform` and `set_local_transform`
    /// compare them: a style writing its paint again every frame (one
    /// property animated) looks the node up once for those left as they
    /// were. `local` is normalized already (`TransformPatch::resolve_finite`).
    pub(crate) fn paint_kept(
        &self,
        id: NodeId,
        inset: Option<ClipInset>,
        transform: Transform,
        local: LocalTransform,
    ) -> (bool, bool, bool) {
        let node = self.node(id);
        (
            node.clip_inset.as_deref().copied() == inset,
            node.transform == finite_translation(transform),
            // Untransformed (most nodes): against the identity, a constant.
            match node.local.as_deref() {
                Some(own) => own.transform == local,
                None => local == LocalTransform::IDENTITY,
            },
        )
    }
    /// Clip `id` and its descendants to its box inset by `inset` (see
    /// `ClipInset`), or with `None` stop. Animating it composites again,
    /// damaging the subtree, without layout or repainting cached layers
    /// (a transformed node clips inside its layer, which repaints).
    pub fn set_clip_inset(&mut self, id: NodeId, inset: Option<ClipInset>) {
        if self.clip_inset(id) == inset {
            return;
        }
        // Clipping more or less shows nothing the subtree doesn't paint.
        self.damage_subtree(id);
        let node = self.node_mut(id);
        node.clip_inset = inset.map(Box::new);
        let flags = if node.transformed() { PAINT } else { COMPOSITE };
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.invalidate(id, flags);
    }
    /// Move `id`'s mask (a shimmer's band, a progress ramp): composites only.
    /// Without a mask, nothing.
    pub fn set_mask_offset(&mut self, id: NodeId, offset: (f32, f32)) {
        if let Some(mask) = self.mask(id) {
            let mask = ImageMask {
                offset,
                ..mask.clone()
            };
            self.set_mask(id, Some(mask));
        }
    }
    /// An explicit cursor inherits through visual ancestors, including disabled regions.
    /// What `id` lays out as: itself, or for a `contents` node its last
    /// child (in turn).
    pub(crate) fn laid_out_as(&self, mut id: NodeId) -> NodeId {
        loop {
            let node = self.node(id);
            match self.children_raw(node).last() {
                Some(last) if node.contents() => id = *last,
                _ => return id,
            }
        }
    }
    /// A flex child's automatic minimum along the main axis (CSS's
    /// `min-width: auto` / `min-height: auto`): the smaller of its own size
    /// (when it sets one) and its content's min-content size, or 0 for one
    /// that clips, as CSS gives a scroll container.
    fn automatic_minimum(
        &mut self,
        id: NodeId,
        asked: Constraints,
        horizontal: bool,
        base: f32,
    ) -> f32 {
        // A `contents` node's style is the one it lays out with (its last
        // child's, with the margins added); its content, and so whether it
        // clips, is that child's.
        if clips(&self.node(self.laid_out_as(id)).style) {
            return 0.;
        }
        let style = &self.node(id).style;
        // An image or an icon that sets its size, or is sized by its
        // pixels, keeps it (CSS's replaced elements); its box holds no
        // content to measure.
        if self.node(self.laid_out_as(id)).replaced
            && let Some(size) = if horizontal {
                style.width
            } else {
                style.height
            }
        {
            return size;
        }
        if !horizontal {
            // Down a column, CSS's min-content block size at the width it
            // has: as tall as it lays out there, its own height set aside.
            // Sized by its content, that is the height it was measured at
            // (`base`), with no probe; given a height, its content's height
            // at that width (a probe under its own constraints, so what it
            // holds answers from the layout's caches).
            let laid = self.laid_out_as(id);
            if style.height.is_none()
                && style.height_percent.is_none()
                && !self.percentages_within(laid).1
            {
                return base;
            }
            // Percentage heights inside (a pane's `h_full` children) are
            // auto for this, and its base resolved them: what its children
            // need, walked as across.
            let content = self.min_content(id, asked, false);
            return base.min(content);
        }
        let specified = style.width;
        let content = self.min_content(id, asked, true);
        match specified {
            Some(size) => size.min(content),
            None => content,
        }
    }
    /// `id` measured under `probe` with its own size along an axis
    /// (`horizontal`, or down) set aside: what its content needs, not what
    /// it asks for. Its kept results (made with its size) are set aside too
    /// and put back after: the probe measures it afresh and leaves nothing
    /// behind, while what it holds answers from its own caches.
    fn measure_unsized(&mut self, id: NodeId, probe: Constraints, horizontal: bool) -> (f32, f32) {
        let (saved, kept) = {
            let node = self.node_mut(id);
            let saved = if horizontal {
                (node.style.width.take(), node.style.width_percent.take())
            } else {
                (node.style.height.take(), node.style.height_percent.take())
            };
            let kept = (
                node.measurement.take(),
                std::mem::take(&mut node.alternates),
                std::mem::take(&mut node.sizes),
            );
            (saved, kept)
        };
        let size = self.measure(id, probe);
        let node = self.node_mut(id);
        if horizontal {
            (node.style.width, node.style.width_percent) = saved;
        } else {
            (node.style.height, node.style.height_percent) = saved;
        }
        (node.measurement, node.alternates, node.sizes) = kept;
        size
    }
    /// The min-content size of `id`'s content along an axis (`horizontal`,
    /// or down, at the width it is `asked` for): what it needs at the least,
    /// its own size set aside. A text measures as narrow as it wraps (or as
    /// tall as it wraps at that width), a box as its children need plus its
    /// padding and border; something that clips needs nothing.
    fn min_content(&mut self, id: NodeId, asked: Constraints, horizontal: bool) -> f32 {
        let id = self.laid_out_as(id);
        let node = self.node(id);
        let kind_is_box = matches!(node.kind, NodeKind::Container(_) | NodeKind::Panel { .. });
        if !kind_is_box || self.children_raw(node).is_empty() {
            if kind_is_box {
                let insets = self.content_insets(id);
                return if horizontal {
                    insets.left + insets.right
                } else {
                    insets.top + insets.bottom
                };
            }
            // A leaf (text, an image, an editor): as narrow as it measures
            // with no room, or as tall as it lays out at its width.
            let probe = if horizontal {
                Constraints {
                    width: Some(0.),
                    ..Default::default()
                }
            } else {
                Constraints {
                    width: asked.force_width.or(asked.width),
                    ..Default::default()
                }
            };
            let size = self.measure_unsized(id, probe, horizontal);
            return if horizontal { size.0 } else { size.1 };
        }
        let layout = match node.kind {
            NodeKind::Container(layout) | NodeKind::Panel { layout, .. } => layout,
            _ => unreachable!(),
        };
        let (gap, children) = (node.style.gap, self.children_raw(node).clone());
        // Children that wrap onto lines need only the widest of them.
        let wraps = node
            .style
            .layout_options
            .as_deref()
            .is_some_and(|o| o.wrap.is_some_and(|w| w != crate::layout::FlexWrap::NoWrap));
        let along = !wraps
            && matches!(
                (layout, horizontal),
                (Layout::Row, true) | (Layout::Column, false)
            );
        let mut total = 0_f32;
        let mut count = 0;
        for child in children {
            // A `contents` child's style is the one it lays out with. What
            // this reads of it, copied (measuring it needs the scene).
            let style = &self.node(child).style;
            let (absolute, width, height, margin) =
                (style.absolute, style.width, style.height, style.margin);
            if absolute {
                continue;
            }
            let need = if clips(&self.node(self.laid_out_as(child)).style) {
                0.
            } else {
                let own = if horizontal { width } else { height };
                match own {
                    Some(size) => size,
                    // Down a column, at the width it is laid out at, so
                    // text wraps there; across, with no room.
                    None => crate::deep(0, || {
                        self.min_content(
                            child,
                            if horizontal {
                                Constraints::default()
                            } else {
                                asked
                            },
                            horizontal,
                        )
                    }),
                }
            };
            let margins = if horizontal {
                margin.left + margin.right
            } else {
                margin.top + margin.bottom
            };
            let need = (need + margins).max(0.);
            total = if along { total + need } else { total.max(need) };
            count += 1;
        }
        if along && count > 1 {
            total += gap * (count - 1) as f32;
        }
        let insets = self.content_insets(id);
        total
            + if horizontal {
                insets.left + insets.right
            } else {
                insets.top + insets.bottom
            }
    }
    /// The cursor over `id`: its own, else its nearest ancestor's.
    pub fn cursor_for(&self, mut id: NodeId) -> Option<crate::cursor::Cursor> {
        loop {
            let node = self.node(id);
            if node.cursor.is_some() {
                return node.cursor;
            }
            id = node.parent?;
        }
    }
    /// Show `cursor` over `id`, or with `None` its ancestors'.
    pub fn set_cursor(&mut self, id: NodeId, cursor: Option<crate::cursor::Cursor>) {
        let old = self.node(id).cursor;
        if old != cursor {
            self.cursor_styles =
                self.cursor_styles - usize::from(old.is_some()) + usize::from(cursor.is_some());
            self.node_mut(id).cursor = cursor;
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
    }
    /// Counts changes that affect what is under the pointer (geometry,
    /// cursors): unchanged, a host needn't hit test again.
    pub fn interaction_revision(&self) -> u64 {
        self.geometry_revision
    }
    /// The cursor at `x`, `y`: a selectable text's, or the topmost node's.
    pub fn cursor_at(&self, x: f32, y: f32) -> Option<crate::cursor::Cursor> {
        if self.cursor_styles == 0 && !self.selectable.any() {
            return None;
        }
        self.hit_test_all(x, y)
            .first()
            .and_then(|id| self.selection_cursor(*id).or_else(|| self.cursor_for(*id)))
    }
    /// `id`'s parent; `None` for the root.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }
    /// Materialize the pending child vector before exposing any reference to it.
    /// Marking a tombstone needs &mut Scene, so no slice of this vector survives
    /// the mark. The first subsequent read clears pending before returning one.
    #[inline(always)]
    fn children_of<'a>(&self, node: &'a Node) -> &'a Vec<NodeId> {
        if self.pending_children.get() != u32::MAX {
            return self.children_pending_of(node);
        }
        self.children_raw(node)
    }
    #[cold]
    #[inline(never)]
    fn children_pending_of<'a>(&self, node: &'a Node) -> &'a Vec<NodeId> {
        let index = self.pending_children.get();
        if index != u32::MAX {
            let held = self.slots[index as usize].node.as_ref().unwrap();
            if std::ptr::eq(held, node) {
                self.finish_pending_children();
            }
        }
        self.children_raw(node)
    }
    /// For a phase that materialized its child vectors at entry, or a
    /// removal's damage walk below (not through) the pending parent.
    #[inline(always)]
    fn children_raw<'a>(&self, node: &'a Node) -> &'a Vec<NodeId> {
        #[cfg(debug_assertions)]
        {
            let index = self.pending_children.get();
            if index != u32::MAX {
                let held = self.slots[index as usize].node.as_ref().unwrap();
                assert!(
                    !std::ptr::eq(held, node),
                    "unmaterialized child vector in internal walk"
                );
            }
        }
        // SAFETY: entry barriers and the checked private removal path guarantee
        // this vector has no tombstones and cannot be compacted while borrowed.
        unsafe { &*node.children.get() }
    }
    #[inline(always)]
    fn settle_children(&self) {
        if self.pending_children.get() != u32::MAX {
            self.finish_pending_children();
        }
    }
    #[cold]
    #[inline(never)]
    fn finish_pending_children(&self) {
        let index = self.pending_children.replace(u32::MAX);
        if index == u32::MAX {
            return;
        }
        self.filtered
            .set(self.filtered.get() & !PENDING_CHILDREN_BIT);
        // Pending is cleared before kill/free/reuse of this parent's slot.
        let slot = &self.slots[index as usize];
        debug_assert!(!slot.dead);
        let node = slot.node.as_ref().unwrap();
        // SAFETY: no reference to the pending vector has been exposed since
        // the exclusive write marked it. Retain invokes no user callback.
        unsafe { &mut *node.children.get() }.retain(|child| *child != self.root);
    }
    /// `id`'s children, in order.
    #[inline(always)]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        if self.pending_children.get() != u32::MAX {
            return self.children_pending(id);
        }
        assert!(self.contains(id), "stale scene node");
        self.children_raw(self.tree_node(id))
    }
    #[cold]
    #[inline(never)]
    fn children_pending(&self, id: NodeId) -> &[NodeId] {
        if self.pending_children.get() == id.index {
            self.finish_pending_children();
        }
        self.children_raw(self.node(id))
    }
    /// What `id` is and paints.
    pub fn kind(&self, id: NodeId) -> &NodeKind {
        &self.node(id).kind
    }
    /// The style `id` was given (a contents node's own, not what it lays
    /// out as).
    pub fn style(&self, id: NodeId) -> Style {
        if self.node(id).contents() {
            self.contents_styles.get(&id).cloned().unwrap_or_default()
        } else if let Some(given) = self.given_styles.get(&id) {
            given.clone()
        } else {
            self.node(id).style.clone()
        }
    }
    /// Mark `id` as drawing an image, an icon, a canvas or a native surface
    /// to its box, as CSS's replaced elements (`<img>`, `<canvas>`) do: a
    /// width or height it sets is also its automatic minimum, so a flex
    /// line too tight for it overflows rather than squashing the artwork
    /// (its box has no content of its own, which would let it shrink to
    /// nothing).
    pub fn set_replaced(&mut self, id: NodeId, replaced: bool) {
        if self.node(id).replaced != replaced {
            self.node_mut(id).replaced = replaced;
            self.invalidate_layout(id);
        }
    }
    /// Make `id` lay out as its last child would in its place, as CSS
    /// `display: contents` does: that child's size, flex and position
    /// properties (percentages, `absolute` and insets resolving against
    /// `id`'s parent) are `id`'s, its margins add to `id`'s own, and it
    /// fills `id`'s box, which is its own. Nothing else of `id`'s own layout
    /// applies (its size, padding, flex or gap); its paint and clip do.
    /// Children before the last are placed over it at its origin, sized
    /// within it: content leaving (absolute), say. With no children it lays
    /// out as its own style says. Control-flow regions holding one view at
    /// a time use it to stay out of layout.
    pub fn set_contents(&mut self, id: NodeId, contents: bool) {
        if self.node(id).contents() == contents {
            return;
        }
        if contents {
            let own = self.style(id);
            self.given_styles.remove(&id);
            self.keep_contents_style(id, own);
            self.set_special(id, CONTENTS, true);
            self.sync_contents(id);
        } else {
            let own = self.contents_styles.remove(&id).unwrap_or_default();
            self.set_special(id, CONTENTS, false);
            self.restyle(id, own, true);
        }
        // Its own measurement changes kind either way.
        self.invalidate_layout(id);
    }
    /// Set or clear `bit` of `id`'s `special` byte, counting the nodes with
    /// any (`contents_count`, which guards the branches that test it).
    fn set_special(&mut self, id: NodeId, bit: u8, on: bool) {
        let node = self.node_mut(id);
        let was = node.special;
        node.special = was.with(bit, on);
        match (was.any(), node.special.any()) {
            (false, true) => self.contents_count += 1,
            (true, false) => self.contents_count -= 1,
            _ => {}
        }
    }
    /// Whether `id` is a contents node (CSS `display: contents`): its
    /// children lay out in its place.
    pub fn is_contents(&self, id: NodeId) -> bool {
        self.node(id).contents()
    }
    /// Which way `id`'s lines run, for it and what it holds (CSS's
    /// `direction`), or with `None` as its parent's: right to left, its
    /// children lie from the right (rows run from the right, a column's
    /// start is its right), text starts at the right, and `*_inline` edges
    /// follow. Padding, margins and insets given as left and right stay
    /// where they were given, as CSS keeps them.
    pub fn set_direction(&mut self, id: NodeId, direction: Option<Direction>) {
        if self.directions.get(&id).copied() == direction {
            return;
        }
        match direction {
            Some(direction) => {
                self.directions.insert(id, direction);
            }
            None => {
                self.directions.remove(&id);
            }
        }
        let inherited = self
            .node(id)
            .parent
            .is_some_and(|parent| self.node(parent).rtl);
        self.refresh_direction::<true>(id, inherited, false);
    }
    /// Which way `id` and its parent run.
    fn way(&self, id: NodeId) -> Way {
        let node = self.node(id);
        Way {
            own: node.rtl,
            parent: node.parent.is_some_and(|parent| self.tree_node(parent).rtl),
        }
    }
    /// The way `id`'s lines run, its own or inherited.
    pub fn direction(&self, id: NodeId) -> Direction {
        if self.node(id).rtl {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }
    /// `id` under a parent running right to left (`inherited`) or not, the
    /// parent having changed way (`moved`) or not: it, and what of its
    /// subtree changes way or sits in what did, lay out again.
    fn refresh_direction<const ENTRY: bool>(&mut self, id: NodeId, inherited: bool, moved: bool) {
        let rtl = self
            .directions
            .get(&id)
            .map_or(inherited, |d| *d == Direction::Rtl);
        let turned = self.node(id).rtl != rtl;
        if !turned && !moved {
            return;
        }
        self.node_mut(id).rtl = rtl;
        // Children first: a contents node lays out as its last child.
        if turned {
            if ENTRY {
                self.settle_children();
            }
            for child in self.children_raw(self.node(id)).clone() {
                crate::deep(0, || self.refresh_direction::<false>(child, rtl, true));
            }
        }
        if self.node(id).contents() {
            self.sync_contents(id);
        } else {
            let given = self.style(id);
            self.restyle(id, given, true);
        }
        if turned && let Some(logical) = self.node(id).align_logical {
            let mut font = self.node(id).font.clone();
            font.align = logical;
            self.set_font(id, font);
        }
        self.damage_subtree(id);
        self.invalidate_layout(id);
        self.node_mut(id).arrange_dirty = true;
    }
    /// A contents node's own style; the default (most have it) is not kept.
    fn keep_contents_style(&mut self, id: NodeId, own: Style) {
        if own == Style::default() {
            self.contents_styles.remove(&id);
        } else {
            self.contents_styles.insert(id, own);
        }
    }
    /// The style a contents node `id` lays out as, from its own and its
    /// last child's.
    fn sync_contents(&mut self, id: NodeId) {
        let own = self.contents_styles.get(&id).cloned().unwrap_or_default();
        let way = self.way(id);
        let own = laid_out_style(own, way);
        let node = self.node(id);
        // Zoomed, as `restyle_special` lays a zoomed node out.
        let zoomed = node.special.has(ZOOMED);
        let mut style = match self.children_of(node).last() {
            Some(lead) => contents_style(own, &self.node(*lead).style),
            None => own,
        };
        style.snap |= zoomed;
        // Its style passes on to a contents parent it lays out as, through
        // `restyle` calling this again, a level a call: contents nest as
        // deep as an app makes them. Counted, so that the stack is looked at
        // every fourth level, not at every sync.
        self.syncing += 1;
        crate::deep(self.syncing, || self.apply_style(id, style));
        self.syncing -= 1;
    }
    /// After `parent` gained, lost or reordered children: queued already
    /// (`invalidate_layout`), it repaints with `CHILDREN`.
    fn children_changed(&mut self, parent: NodeId) {
        self.node_mut(parent).dirty |= CHILDREN;
        if self.contents_count > 0 && self.node(parent).contents() {
            self.sync_contents(parent);
        }
    }
    /// `id`'s own effects.
    pub fn effects(&self, id: NodeId) -> Effects {
        self.node(id).effects
    }
    /// `id`'s own translation.
    pub fn transform(&self, id: NodeId) -> Transform {
        self.node(id).transform
    }
    /// Ancestors from the immediate parent to the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.parent(id), |id| self.parent(*id))
    }
    /// Front-to-back hit targets, including containers; call after flushing layout.
    /// The point maps through each transformed ancestor's inverse, so nodes
    /// are hit where they are drawn; a singular transform hits nothing.
    pub fn hit_test_all(&self, x: f32, y: f32) -> Vec<NodeId> {
        self.settle_children();
        match self.popups.is_empty() {
            true => self.hit_test_walk::<false>(x, y),
            false => self.hit_test_walk::<true>(x, y),
        }
    }
    // One endpoint suffices to reject a point outside a subtree. Cache
    // proved directions (bits 1..4) and a completed scan (bit 5), with ink
    // in bit 7. Bit 0 witnesses every cached dependency, so invalidation
    // carries the same one-bit value as before. Other directions remain
    // conservative misses until invalidation or an ancestor's scan refines them.
    #[inline(always)]
    fn hit_outside(&self, node: &Node, b: Rect, point: (f32, f32)) -> bool {
        if point.0 < b.x {
            self.hit_proved::<0>(node)
        } else if point.0 >= b.x + b.width {
            self.hit_proved::<1>(node)
        } else if point.1 < b.y {
            self.hit_proved::<2>(node)
        } else if point.1 >= b.y + b.height {
            self.hit_proved::<3>(node)
        } else {
            false
        }
    }
    #[inline(never)]
    fn hit_proved<const D: usize>(&self, node: &Node) -> bool {
        if node.bounds_valid.get() & HIT_CHECKED == 0 {
            let b = node.bounds;
            let start = if D < 2 { b.x } else { b.y };
            let limit = if D.is_multiple_of(2) {
                start
            } else {
                start + if D < 2 { b.width } else { b.height }
            };
            self.hit_projection::<D>(node, 0., 0, limit);
        }
        node.bounds_valid.get() & (HIT_DIRECTION << D) != 0
    }
    #[inline(always)]
    fn hit_projection<const D: usize>(
        &self,
        node: &Node,
        offset: f32,
        level: u32,
        limit: f32,
    ) -> Option<f32> {
        let cached = node.bounds_valid.get();
        let zero = offset == 0.;
        let b = node.bounds;
        let start = if D < 2 { b.x } else { b.y };
        let size = if D < 2 { b.width } else { b.height };
        let own = if D.is_multiple_of(2) {
            start + offset
        } else {
            (start + offset) + size
        };
        if if D.is_multiple_of(2) {
            own < limit
        } else {
            own > limit
        } {
            node.bounds_valid
                .set(cached | BOUNDS_VALID | if zero { HIT_CHECKED } else { 0 });
            return None;
        }
        if zero && cached & (HIT_DIRECTION << D) != 0 {
            return Some(own);
        }
        let safe = !node.transformed()
            && !matches!(node.kind, NodeKind::Image(_) | NodeKind::Svg(_))
            && own.is_finite();
        if !safe {
            node.bounds_valid
                .set(cached | BOUNDS_VALID | if zero { HIT_CHECKED } else { 0 });
            return None;
        }
        if self.children_raw(node).is_empty() {
            node.bounds_valid.set(
                cached
                    | BOUNDS_VALID
                    | if zero {
                        HIT_CHECKED | (HIT_DIRECTION << D)
                    } else {
                        0
                    },
            );
            return Some(own);
        }
        if level == 0 {
            self.hit_projection_children::<D>(node, offset, level, own, limit)
        } else {
            crate::deep(level, || {
                self.hit_projection_children::<D>(node, offset, level, own, limit)
            })
        }
    }
    #[inline(never)]
    fn hit_projection_children<const D: usize>(
        &self,
        node: &Node,
        offset: f32,
        level: u32,
        own: f32,
        limit: f32,
    ) -> Option<f32> {
        let extent = self.children_raw(node).iter().try_fold(own, |held, child| {
            let child = self.node(*child);
            let shift = if D < 2 {
                child.transform.x
            } else {
                child.transform.y
            };
            let c = self.hit_projection::<D>(child, offset + shift, level + 1, limit)?;
            Some(if D.is_multiple_of(2) {
                if held < c { held } else { c }
            } else {
                if held > c { held } else { c }
            })
        });
        let cached = node.bounds_valid.get();
        if offset == 0. {
            node.bounds_valid.set(
                cached
                    | BOUNDS_VALID
                    | HIT_CHECKED
                    | if extent == Some(own) {
                        HIT_DIRECTION << D
                    } else {
                        0
                    },
            );
        } else {
            node.bounds_valid.set(cached | BOUNDS_VALID);
        }
        extent
    }
    /// `POPUPS`: popup roots exist, whose subtrees are hit past the viewport.
    fn hit_test_walk<const POPUPS: bool>(&self, x: f32, y: f32) -> Vec<NodeId> {
        let inside = |r: Rect, (x, y): (f32, f32)| {
            x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
        };
        let mut hits = Vec::new();
        // Keep the active siblings and their inherited context in locals.
        // Only a descent with siblings left to visit needs a stack entry.
        let roots = [self.root];
        let mut siblings = roots.iter();
        let mut parent_transform = Transform::default();
        let mut parent_opacity = 1.0;
        let mut parent_clip = self.viewport;
        let mut parent_point = (x, y);
        let mut stack = Vec::new();
        loop {
            let Some(&id) = siblings.next() else {
                let Some((rest, transform, opacity, clip, point)) = stack.pop() else {
                    break;
                };
                siblings = rest;
                parent_transform = transform;
                parent_opacity = opacity;
                parent_clip = clip;
                parent_point = point;
                continue;
            };
            let point = parent_point;
            let node = self.node(id);
            let inherited_clip = match POPUPS && self.popups.contains(&id) {
                true => UNCLIPPED,
                false => parent_clip,
            };
            let transform = Transform {
                x: parent_transform.x + node.transform.x,
                y: parent_transform.y + node.transform.y,
            };
            let bounds = node.bounds.translated(transform);
            let opacity = if own_visible(&node.style) {
                node.effects.opacity * parent_opacity
            } else {
                0.
            };
            // NaN opacity is non-hittable too.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(opacity > 0.) {
                continue;
            }
            let (point, inherited_clip) = if !node.transformed() {
                (point, inherited_clip)
            } else {
                // Clipped around it, or flattened to nothing, nothing in it is hit.
                let Some(inverse) = node.projective(bounds).inverse() else {
                    continue;
                };
                if !inside(inherited_clip, point) {
                    continue;
                }
                (inverse.point(point.0, point.1), UNCLIPPED)
            };
            let clip =
                clipped_bounds(Some(inherited_clip), bounds, &node.style).unwrap_or(inherited_clip);
            // Clipped by its inset too, in its own space.
            let clip = match node.clip_inset.as_deref() {
                Some(inset) => clip.intersection(inset.apply(bounds)).unwrap_or_default(),
                None => clip,
            };
            let painted = |transform: crate::affine::Affine| {
                transform
                    .inverse()
                    .is_some_and(|inverse| inside(bounds, inverse.point(point.0, point.1)))
            };
            let hit = match &node.kind {
                NodeKind::Image(image) => painted(image.paint_transform(bounds)),
                NodeKind::Svg(svg) => painted(svg.paint_transform(bounds)),
                _ => inside(bounds, point),
            };
            if hit && inside(clip, point) {
                hits.push(id);
            }
            if self.children_raw(node).is_empty() {
                continue;
            }
            if !hit
                && !POPUPS
                && transform == Transform::default()
                && self.hit_outside(node, bounds, point)
            {
                continue;
            }
            if !siblings.as_slice().is_empty() {
                stack.push((
                    siblings,
                    parent_transform,
                    parent_opacity,
                    parent_clip,
                    parent_point,
                ));
            }
            siblings = self.children_raw(node).iter();
            parent_transform = transform;
            parent_opacity = opacity;
            parent_clip = clip;
            parent_point = point;
        }
        // Inert subtrees paint but are not hit; what is under them is.
        if !self.inert.is_empty() {
            hits.retain(|hit| !self.inert.iter().any(|inert| self.is_within(*hit, *inert)));
        }
        hits.reverse();
        hits
    }
    /// The topmost node at `x`, `y`, containers included; call after
    /// flushing layout.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hit_test_all(x, y).into_iter().next()
    }
    /// Every laid-out node under `from` in paint order, with its translated
    /// box and the clip it paints within, for diagnostics (the layout
    /// inspector). `visit` returns whether to descend into the node. Nodes
    /// hidden by `display: none` or `visible = false` are skipped with their
    /// subtrees, as are inert ones (`set_inert`). Local transforms (scale,
    /// rotation, skew) are not applied: boxes are as laid out and translated.
    pub fn walk(&self, from: NodeId, mut visit: impl FnMut(&WalkItem) -> bool) {
        self.settle_children();
        if !self.contains(from) {
            return;
        }
        let (parent, clip) = match self.node(from).parent {
            Some(parent) => {
                let (bounds, _, clip) = self.world(parent);
                let node = self.node(parent);
                let offset = Transform {
                    x: bounds.x - node.bounds.x,
                    y: bounds.y - node.bounds.y,
                };
                (offset, clipped_bounds(clip, bounds, &node.style))
            }
            None => (Transform::default(), Some(self.viewport)),
        };
        let mut stack = vec![(from, parent, clip, 0_usize)];
        while let Some((id, parent, clip, depth)) = stack.pop() {
            if self.inert.contains(&id) {
                continue;
            }
            let node = self.node(id);
            if !own_visible(&node.style) {
                continue;
            }
            let transform = Transform {
                x: parent.x + node.transform.x,
                y: parent.y + node.transform.y,
            };
            let bounds = node.bounds.translated(transform);
            let item = WalkItem {
                id,
                depth,
                bounds,
                clip,
            };
            if !visit(&item) {
                continue;
            }
            let inner = clipped_bounds(clip, bounds, &node.style);
            stack.extend(
                self.children_raw(node)
                    .iter()
                    .rev()
                    .map(|child| (*child, transform, inner, depth + 1)),
            );
        }
    }
    /// Whether `id` clips its content horizontally and vertically.
    pub fn clips(&self, id: NodeId) -> (bool, bool) {
        clip_axes(&self.node(id).style)
    }
    /// Whether `id` is scroll content (`set_scroll_copy`): its parent is a
    /// scroll viewport.
    pub fn is_scroll_content(&self, id: NodeId) -> bool {
        self.node(id).scroll_copy
    }
    /// The axes (width, height) on which `id` has a percentage size that
    /// the last layout could not resolve, because its parent's size on that
    /// axis depends on its content: the percentage fell back to `id`'s own
    /// content size. A flexed or stretched size does not count.
    pub fn unresolved_percentages(&self, id: NodeId) -> (bool, bool) {
        let node = self.node(id);
        let Some(measurement) = &node.measurement else {
            return (false, false);
        };
        if node.parent.is_none() || node.style.absolute {
            return (false, false);
        }
        let c = measurement.constraints;
        (
            node.style.width.is_none()
                && node.style.width_percent.is_some()
                && c.percent_width.is_none()
                && c.force_width.is_none(),
            node.style.height.is_none()
                && node.style.height_percent.is_some()
                && c.percent_height.is_none()
                && c.force_height.is_none(),
        )
    }
    /// Uses the same installed font measurer as layout, useful for caret geometry.
    pub fn measure_text(&self, text: &str, font_size: f32, max_width: Option<f32>) -> (f32, f32) {
        if let Some(shaper) = &self.font_shaper {
            return shaper
                .shape(text, font_size, max_width, &Default::default())
                .size();
        }
        if let Some(shaper) = &self.text_shaper {
            return shaper.shape(text, font_size, max_width).size();
        }
        if let Some(measurer) = &self.text_measurer {
            return measurer.measure(text, font_size, max_width);
        }
        crate::text_layout::FallbackTextLayout::measure(
            text,
            font_size,
            max_width,
            crate::text_layout::LineHeight::NORMAL,
        )
    }
    /// Reorders an exact permutation of a parent's children, retaining every ID.
    /// Invalid parents, foreign nodes, omissions, and duplicates return `false`
    /// without modifying the scene. An unchanged order is a successful no-op.
    #[inline(never)]
    pub fn reorder_children(&mut self, parent: NodeId, children: &[NodeId]) -> bool {
        self.settle_children();
        if !self.contains(parent) {
            return false;
        }
        let current = self.children_raw(self.node(parent));
        if current == children {
            return true;
        }
        if current.len() != children.len() {
            return false;
        }
        self.reorder_changed_children(parent, children)
    }
    // Keep unchanged-order calls independent of the map and damage work:
    // cold, so that the entry keeps no register for it.
    #[cold]
    #[inline(never)]
    fn reorder_changed_children(&mut self, parent: NodeId, children: &[NodeId]) -> bool {
        let current = self.children_raw(self.tree_node(parent));
        let place: NodeMap<usize> = current.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let Some(places) = children
            .iter()
            .map(|id| place.get(id).copied())
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        // The same children, each once.
        let mut seen = vec![false; places.len()];
        if places
            .iter()
            .any(|&i| std::mem::replace(&mut seen[i], true))
        {
            return false;
        }
        let mut moved = vec![true; places.len()];
        for index in longest_increasing(&places) {
            moved[index] = false;
        }
        // Paint order changes only for the children that moved past others
        // (a keyed list's new row, appended and put in its place): they
        // repaint where they are, the rest stay as they were.
        for (child, moved) in children.iter().zip(moved) {
            if moved {
                self.damage_subtree(*child);
            }
        }
        self.node_mut(parent)
            .children
            .get_mut()
            .copy_from_slice(children);
        self.invalidate_layout(parent);
        self.children_changed(parent);
        true
    }
    /// Installs the native font engine for editor geometry and intrinsic measurement.
    pub fn set_text_shaper(&mut self, shaper: impl crate::text_layout::TextShaper + 'static) {
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
        self.text_shaper = Some(std::rc::Rc::new(shaper));
        let ids: Vec<_> = self.ids().collect();
        for id in ids {
            self.invalidate(id, LAYOUT | PAINT);
        }
    }
    /// Shape `text` at `font_size` in the default font, wrapped at
    /// `max_width` if given.
    pub fn shape_text(
        &self,
        text: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> Box<dyn crate::text_layout::TextLayout> {
        if let Some(shaper) = &self.font_shaper {
            shaper.shape(text, font_size, max_width, &Default::default())
        } else if let Some(shaper) = &self.text_shaper {
            shaper.shape(text, font_size, max_width)
        } else {
            Box::new(crate::text_layout::FallbackTextLayout::new(
                text, font_size, max_width,
            ))
        }
    }
    /// Measure plain text with `measurer` (a platform shaper's metrics)
    /// instead of the built-in estimate; text lays out again.
    pub fn set_text_measurer(&mut self, measurer: impl TextMeasurer + 'static) {
        self.text_measurer = Some(Box::new(measurer));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
        let ids: Vec<_> = self.ids().collect();
        for id in ids {
            self.invalidate(id, LAYOUT | PAINT);
        }
    }
    /// `id`'s text style.
    pub fn font(&self, id: NodeId) -> &crate::text_layout::FontStyle {
        &self.node(id).font
    }
    /// Font changes affect shaping, layout, paint and retained layer contents.
    pub fn set_font(&mut self, id: NodeId, mut font: crate::text_layout::FontStyle) {
        font.weight = font.weight.clamp(1, 1000);
        // Text starts at the right of a line that runs right to left, and
        // ends at its left. `Start` stays for left to right (the shaper's
        // natural alignment); `End` is the right then.
        use crate::text_layout::TextAlign;
        let logical = matches!(font.align, TextAlign::Start | TextAlign::End).then_some(font.align);
        let rtl = self.node(id).rtl;
        font.align = match font.align {
            TextAlign::Start if rtl => TextAlign::Right,
            TextAlign::End if rtl => TextAlign::Left,
            TextAlign::End => TextAlign::Right,
            other => other,
        };
        if self.node(id).font == font && self.node(id).align_logical == logical {
            return;
        }
        self.damage_subtree(id);
        self.node_mut(id).align_logical = logical;
        self.node_mut(id).font = font;
        self.invalidate(id, PAINT);
        self.invalidate_layout(id);
    }
    /// Shape and measure text with `shaper` (the renderer's font system);
    /// text lays out and paints again.
    pub fn set_font_text_shaper(
        &mut self,
        shaper: impl crate::text_layout::FontTextShaper + 'static,
    ) {
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
        self.font_shaper = Some(std::rc::Rc::new(shaper));
        let ids: Vec<_> = self
            .paint_items()
            .filter(|item| matches!(item.kind, NodeKind::Text { .. }))
            .map(|item| item.id)
            .collect();
        for id in ids {
            self.invalidate(id, PAINT);
            self.invalidate_layout(id);
        }
    }
    /// Install rich paragraph geometry for inline selection and interactive spans.
    pub fn set_rich_text_shaper(
        &mut self,
        shaper: impl Fn(
            &crate::rich_text::RichText,
            Option<f32>,
        ) -> Box<dyn crate::text_layout::TextLayout>
        + 'static,
    ) {
        self.rich_shaper = Some(Box::new(shaper));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
    }
    /// Shape a rich paragraph, wrapped at `width` if given.
    pub fn shape_rich_text(
        &self,
        rich: &crate::rich_text::RichText,
        width: Option<f32>,
    ) -> Box<dyn crate::text_layout::TextLayout> {
        self.rich_shaper.as_ref().map_or_else(
            || {
                Box::new(crate::text_layout::FallbackTextLayout::with_rich(
                    rich, width,
                )) as Box<dyn crate::text_layout::TextLayout>
            },
            |shaper| shaper(rich, width),
        )
    }
    /// Install measurement for plain text nodes, sized as the installed font
    /// shaper lays them out. Hosts measure from the text they draw from.
    pub fn set_font_text_measurer(
        &mut self,
        measure: impl Fn(
            &std::sync::Arc<str>,
            f32,
            Option<f32>,
            &crate::text_layout::FontStyle,
        ) -> (f32, f32)
        + 'static,
    ) {
        self.font_measurer = Some(Box::new(measure));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
        let ids: Vec<_> = self
            .paint_items()
            .filter(|item| matches!(item.kind, NodeKind::Text { .. }))
            .map(|item| item.id)
            .collect();
        for id in ids {
            self.invalidate(id, PAINT);
            self.invalidate_layout(id);
        }
    }
    /// Install plain text's min-content measurement: the size the font
    /// measurer gives `min_content_lines(text)` unwrapped (a line at every
    /// opportunity to break one), worked out without that copy.
    pub fn set_font_min_content_measurer(
        &mut self,
        measure: impl Fn(&std::sync::Arc<str>, f32, &crate::text_layout::FontStyle) -> (f32, f32)
        + 'static,
    ) {
        self.font_min_content = Some(Box::new(measure));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
    }
    /// Install rich text's min-content measurement: the size the rich
    /// measurer gives `RichText::min_content_lines` unwrapped, worked out
    /// without that copy.
    pub fn set_rich_min_content_measurer(
        &mut self,
        measure: impl Fn(&crate::rich_text::RichText) -> (f32, f32) + 'static,
    ) {
        self.rich_min_content = Some(Box::new(measure));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
    }
    /// Install native continuous mixed-run measurement for rich display text.
    pub fn set_rich_text_measurer(
        &mut self,
        measure: impl Fn(&crate::rich_text::RichText, Option<f32>) -> (f32, f32) + 'static,
    ) {
        self.rich_measurer.install(Box::new(measure));
        self.text_shaper_revision = self.text_shaper_revision.wrapping_add(1);
        let ids: Vec<_> = self
            .paint_items()
            .filter(|item| matches!(item.kind, NodeKind::RichText { .. }))
            .map(|item| item.id)
            .collect();
        for id in ids {
            self.invalidate(id, PAINT);
            self.invalidate_layout(id);
        }
    }
    /// Install measurement for rich text that is not mounted (`TextMeasure`),
    /// sized exactly as `set_rich_text_measurer` sizes it but without keeping
    /// what drawing it would need.
    pub fn set_detached_rich_text_measurer(
        &mut self,
        measure: impl Fn(&crate::rich_text::RichText, Option<f32>) -> (f32, f32) + 'static,
    ) {
        self.rich_measurer.install_detached(Box::new(measure));
    }
    /// The rich text measurer layout uses, including one installed later.
    pub fn text_measure(&self) -> crate::rich_text::TextMeasure {
        self.rich_measurer.clone()
    }
    /// Changes whenever either installed shaping callback is replaced.
    pub(crate) fn text_shaper_revision(&self) -> u64 {
        self.text_shaper_revision
    }
    /// Point (`x`, `y`) in the space `id` is laid out in (its `bounds`,
    /// translated) in the window's points: through its own and its
    /// ancestors' local transforms and zooms, as it shows (`point_in`'s
    /// inverse). Without transforms in the scene, the point itself.
    pub fn window_point(&self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        if self.transformed == 0 || !self.contains(id) {
            return (x, y);
        }
        match self.placed(id).2 {
            Some(placement) => placement.matrix.point(x, y),
            None => (x, y),
        }
    }
    /// `rect`, in the space `id` is laid out in, as it shows in the window:
    /// the box around its corners mapped as `window_point` maps them.
    pub fn window_rect(&self, id: NodeId, rect: Rect) -> Rect {
        if self.transformed == 0 || !self.contains(id) {
            return rect;
        }
        match self.placed(id).2 {
            Some(placement) => placement.matrix.bounds(rect),
            None => rect,
        }
    }
    /// Text node `id`'s lines as it shows them, for assistive technology
    /// (a screen reader's cursor, a magnifier following the caret): each
    /// visual line's bytes, its box and every grapheme boundary's place
    /// across it, in the window's points (through any transform or zoom it
    /// shows under). `None` for a node with no text, or one masked (a
    /// password). Shapes the text: ask only when a reader does.
    pub fn text_lines(&self, id: NodeId) -> Option<Vec<TextLine>> {
        let mut lines = self.laid_out_lines(id)?;
        if self.transformed > 0 {
            for line in &mut lines {
                // Each caret where it shows across the line's middle; the
                // line's box around its mapped corners.
                let middle = line.rect.y + line.rect.height / 2.;
                for caret in &mut line.carets {
                    *caret = self.window_point(id, *caret, middle).0;
                }
                line.rect = self.window_rect(id, line.rect);
            }
        }
        Some(lines)
    }
    /// `text_lines` in the space `id` is laid out in (its bounds,
    /// translated), before any transform or zoom.
    fn laid_out_lines(&self, id: NodeId) -> Option<Vec<TextLine>> {
        use crate::text_layout::{TextAffinity, TextPosition};
        use unicode_segmentation::UnicodeSegmentation;
        let bounds = self.bounds(id);
        let (text, layout): (Arc<str>, Box<dyn crate::text_layout::TextLayout>) =
            match self.kind(id) {
                NodeKind::Text {
                    text, font_size, ..
                } if !self.font(id).masked => (
                    text.clone(),
                    self.shape_text_with_font(text, *font_size, Some(bounds.width), self.font(id)),
                ),
                NodeKind::RichText { text } => (
                    text.text_arc(),
                    self.shape_rich_text(text, Some(bounds.width)),
                ),
                _ => return None,
            };
        // Every line in one pass where the shaper can (zgx-gpu's).
        if let Some(lines) = layout.line_carets(&text) {
            return Some(
                lines
                    .into_iter()
                    .map(|line| {
                        let carets: Vec<f32> =
                            line.carets.into_iter().map(|x| bounds.x + x).collect();
                        let (left, right) = carets
                            .iter()
                            .fold((f32::MAX, f32::MIN), |(l, r), x| (l.min(*x), r.max(*x)));
                        TextLine {
                            rtl: carets.len() > 1 && carets[0] > carets[carets.len() - 1],
                            range: line.range,
                            rect: Rect::new(
                                left,
                                bounds.y + line.top,
                                (right - left).max(0.),
                                line.height,
                            ),
                            carets,
                        }
                    })
                    .collect(),
            );
        }
        let at = |byte, affinity| {
            layout.caret_position(TextPosition {
                byte_offset: byte,
                affinity,
            })
        };
        let mut lines = Vec::new();
        let mut start = 0;
        loop {
            let (first, last) = layout.visual_line_range(TextPosition {
                byte_offset: start,
                affinity: TextAffinity::After,
            });
            let range = first.byte_offset.max(start)..last.byte_offset.max(start);
            // Every character's place in one pass over the line, not a caret
            // each from the text's start (quadratic in its length).
            let starts: Vec<usize> = text[range.clone()]
                .grapheme_indices(true)
                .map(|(offset, _)| range.start + offset)
                .collect();
            let mut carets: Vec<f32> = layout
                .carets_x(&starts)
                .into_iter()
                .map(|x| bounds.x + x)
                .collect();
            let end = at(range.end, TextAffinity::Before);
            carets.push(bounds.x + end.x);
            let top = at(range.start, TextAffinity::After);
            let (left, right) = carets
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, r), x| (l.min(*x), r.max(*x)));
            lines.push(TextLine {
                rtl: carets.len() > 1 && carets[0] > carets[carets.len() - 1],
                range: range.clone(),
                rect: Rect::new(left, bounds.y + top.y, (right - left).max(0.), top.height),
                carets,
            });
            // On past the line's break, or where it wrapped.
            let next = if text[range.end..].starts_with("\r\n") {
                range.end + 2
            } else if text[range.end..].starts_with(['\n', '\r']) {
                range.end + 1
            } else {
                range.end
            };
            if next <= start || next > text.len() || (next == range.end && next >= text.len()) {
                break;
            }
            start = next;
        }
        Some(lines)
    }
    /// Where the character at byte `at` of text node `id` starts its
    /// baseline, in the window's points, and its font's size: what the
    /// platform's Look Up shows a word's definition over.
    pub fn text_baseline(&self, id: NodeId, at: usize) -> Option<((f32, f32), f32)> {
        use unicode_segmentation::UnicodeSegmentation;
        let (text, size) = match self.kind(id) {
            NodeKind::Text {
                text, font_size, ..
            } => (text.clone(), *font_size),
            NodeKind::RichText { text } => {
                let size = text
                    .runs()
                    .iter()
                    .find(|run| run.range.contains(&at))
                    .or(text.runs().first())
                    .map_or(16., |run| run.font_size);
                (text.text_arc(), size)
            }
            _ => return None,
        };
        let lines = self.laid_out_lines(id)?;
        let line = lines
            .iter()
            .find(|line| line.range.start <= at && at <= line.range.end)?;
        let index = text[line.range.start..at.min(line.range.end)]
            .graphemes(true)
            .count();
        let x = *line.carets.get(index)?;
        // The line's box holds the font's ascent and descent centred in its
        // height: the baseline sits about 0.8 of the size below its top.
        let top = line.rect.y + (line.rect.height - size).max(0.) / 2.;
        let (x, y) = (x, top + size * 0.8);
        // Where it shows, at the size it shows (a zoom grows the type).
        let span = self.window_rect(id, Rect::new(x, top, 0., size));
        Some((self.window_point(id, x, y), span.height))
    }
    /// Shape `text` at `size` in `font`, wrapped at `width` if given.
    pub fn shape_text_with_font(
        &self,
        text: &str,
        size: f32,
        width: Option<f32>,
        font: &crate::text_layout::FontStyle,
    ) -> Box<dyn crate::text_layout::TextLayout> {
        if let Some(shaper) = &self.font_shaper {
            shaper.shape(text, size, width, font)
        } else if font.line_height != crate::text_layout::LineHeight::NORMAL
            || font.letter_spacing != crate::text_layout::LetterSpacing::ZERO
        {
            // Legacy callbacks cannot receive font metrics. Use the same
            // grapheme-cell fallback for measuring and editing explicit metrics.
            Box::new(crate::text_layout::FallbackTextLayout::with_font(
                text, size, width, font,
            ))
        } else {
            self.shape_text(text, size, width)
        }
    }
    /// Lines of `font` at `size` shaped apart from the scene, as
    /// `shape_text_with_font` shapes them: a long text's lines, shaped as
    /// they show (`editor_lines`), while the scene is borrowed or not.
    pub(crate) fn line_shaper(
        &self,
        size: f32,
        font: &crate::text_layout::FontStyle,
    ) -> crate::editor_lines::LineShaper {
        let font = font.clone();
        if let Some(shaper) = self.font_shaper.clone() {
            std::rc::Rc::new(move |text, width| shaper.shape(text, size, width, &font))
        } else if font.line_height != crate::text_layout::LineHeight::NORMAL
            || font.letter_spacing != crate::text_layout::LetterSpacing::ZERO
        {
            std::rc::Rc::new(move |text, width| {
                Box::new(crate::text_layout::FallbackTextLayout::with_font(
                    text, size, width, &font,
                ))
            })
        } else if let Some(shaper) = self.text_shaper.clone() {
            std::rc::Rc::new(move |text, width| shaper.shape(text, size, width))
        } else {
            std::rc::Rc::new(move |text, width| {
                Box::new(crate::text_layout::FallbackTextLayout::new(
                    text, size, width,
                ))
            })
        }
    }
    /// The size of `text` at `size` in `font`, wrapped at `width` if given.
    pub fn measure_text_with_font(
        &self,
        text: &str,
        size: f32,
        width: Option<f32>,
        font: &crate::text_layout::FontStyle,
    ) -> (f32, f32) {
        if let Some(shaper) = &self.font_shaper {
            shaper.shape(text, size, width, font).size()
        } else if font.line_height != crate::text_layout::LineHeight::NORMAL
            || font.letter_spacing != crate::text_layout::LetterSpacing::ZERO
        {
            crate::text_layout::FallbackTextLayout::measure_with_font(text, size, width, font)
        } else {
            self.measure_text(text, size, width)
        }
    }
    /// Change what `id` is or paints, invalidating only what the change
    /// affects.
    pub fn set_kind(&mut self, id: NodeId, kind: NodeKind) {
        if self.node(id).kind == kind {
            return;
        }
        // Renderer caches are keyed by node and kind; only a different kind or
        // a replaced texture-backed resource can leave one stale. Text and
        // colour changes, which animate every frame, do not.
        let resource = |kind: &NodeKind| {
            #[cfg(target_os = "macos")]
            if matches!(kind, NodeKind::NativeSurface(_)) {
                return true;
            }
            match kind {
                NodeKind::Image(_) | NodeKind::Svg(_) | NodeKind::Canvas(_) => true,
                NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad.decoration.is_some(),
                _ => false,
            }
        };
        if std::mem::discriminant(&self.node(id).kind) != std::mem::discriminant(&kind)
            || resource(&self.node(id).kind)
        {
            self.content_revision = self.content_revision.wrapping_add(1);
        }
        let container_layout = |kind: &NodeKind| match kind {
            NodeKind::Container(layout) | NodeKind::Panel { layout, .. } => Some(*layout),
            _ => None,
        };
        let old_layout = container_layout(&self.node(id).kind);
        // A panel's border takes room in layout (`QuadStyle::layout_border`).
        let border = |kind: &NodeKind| match kind {
            NodeKind::Panel { quad, .. } => quad.layout_border(),
            _ => Insets::default(),
        };
        let old_layout = old_layout.filter(|_| border(&self.node(id).kind) == border(&kind));
        let layout_unchanged = matches!(
            (&self.node(id).kind, &kind),
            (
                NodeKind::Rect(_) | NodeKind::Quad(_),
                NodeKind::Rect(_) | NodeKind::Quad(_)
            )
        ) || (old_layout.is_some() && old_layout == container_layout(&kind))
            || matches!((&self.node(id).kind, &kind),
                (NodeKind::Text { text: old_text, font_size: old_size, .. },
                 NodeKind::Text { text: new_text, font_size: new_size, .. })
                 if old_text == new_text && old_size == new_size)
            || matches!((&self.node(id).kind, &kind),
                (NodeKind::RichText { text: old }, NodeKind::RichText { text: new }) if old.same_metrics(new))
            || matches!((&self.node(id).kind, &kind),
                (NodeKind::Image(old), NodeKind::Image(new))
                if (old.width() == new.width() && old.height() == new.height())
                    // Both concrete axes make intrinsic source dimensions
                    // irrelevant, including when min/max constraints clamp them.
                    || (self.node(id).style.width.is_some() && self.node(id).style.height.is_some()));
        let layout_unchanged = layout_unchanged
            || matches!(
                (&self.node(id).kind, &kind),
                (NodeKind::Canvas(_), NodeKind::Canvas(_)) | (NodeKind::Svg(_), NodeKind::Svg(_))
            );
        #[cfg(target_os = "macos")]
        let layout_unchanged = layout_unchanged
            || matches!((&self.node(id).kind,&kind),(NodeKind::NativeSurface(old),NodeKind::NativeSurface(new)) if (old.width(),old.height())==(new.width(),new.height()) || (self.node(id).style.width.is_some() && self.node(id).style.height.is_some()));
        self.damage_subtree(id);
        if matches!((&self.node(id).kind, &kind), (NodeKind::Image(old), NodeKind::Image(new))
            if old.transform() != new.transform())
        {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
        if matches!((&self.node(id).kind, &kind), (NodeKind::Svg(old), NodeKind::Svg(new)) if old.transform()!=new.transform())
        {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
        let colour = paints_only(&self.node(id).kind, &kind);
        self.node_mut(id).kind = kind;
        self.invalidate(id, if colour { PAINT | PAINT_ONLY } else { PAINT });
        if !layout_unchanged {
            self.invalidate_layout(id);
        }
    }
    /// Show `image`, a GPU view's next frame (`crate::gpu_view`), where `id`
    /// shows that view's last one: its box repaints, and layout runs again
    /// only for a new size its style leaves open. Unlike `set_kind`, the
    /// scene's resources count as they were (`content_revision`): a renderer
    /// replaces the view's texture itself, rather than looking over every
    /// item for stale ones each frame a view draws. Panics unless `image` is
    /// a frame of the view `id` shows.
    pub fn set_image_frame(&mut self, id: NodeId, image: Arc<crate::image::ImageData>) {
        let view = image.gpu_frame().expect("a GPU view's frame").view;
        assert!(
            matches!(&self.node(id).kind, NodeKind::Image(old)
                if old.gpu_frame().is_some_and(|old| old.view == view)),
            "set_image_frame requires a node showing the same view's frame"
        );
        // As `set_kind`, but the resources counted as they were. Through it,
        // not beside it: its helpers keep the one caller they are inlined
        // into, rather than costing every other change a call.
        let resources = self.content_revision;
        self.set_kind(id, NodeKind::Image(image));
        self.content_revision = resources;
    }
    /// Append a node of `kind` with `style` as `parent`'s last child.
    pub fn append(&mut self, parent: NodeId, kind: NodeKind, mut style: Style) -> NodeId {
        self.content_revision = self.content_revision.wrapping_add(1);
        if let Some(options) = &style.layout_options {
            let normalized = (**options).normalized();
            if normalized != **options {
                style.layout_options = Some(Arc::new(normalized));
            }
        }
        self.node(parent);
        self.relative_count += usize::from(style.relative);
        let overflow_fade = style.fade_overflow;
        // It runs as its parent does until it says otherwise.
        let rtl = self.node(parent).rtl;
        let way = Way {
            own: rtl,
            parent: rtl,
        };
        let given = (!laid_out_as_given(&style, way)).then(|| style.clone());
        let style = match given {
            Some(_) => laid_out_style(style, way),
            None => style,
        };
        let node = Node {
            cursor: None,
            parent: Some(parent),
            kind,
            head: NodeHead {
                font: Default::default(),
                children: std::cell::UnsafeCell::new(Vec::new()),
            },
            style,
            bounds: Rect::default(),
            transform: Transform::default(),
            effects: Effects::default(),
            dirty: LAYOUT | PAINT,
            queued: true,
            measurement: None,
            alternates: Vec::new(),
            sizes: Vec::new(),
            ink: std::cell::Cell::new(None),
            bounds_valid: std::cell::Cell::new(0),
            text_size: Default::default(),
            advanced_layout: None,
            arrange_dirty: true,
            reads: std::cell::Cell::new(0),
            isolated: false,
            local: None,
            image_mask: None,
            layer_shaded: false,
            layered: false,
            blend: BlendMode::Normal,
            clip_inset: None,
            shader_uniforms: None,
            promoted: false,
            scroll_copy: false,
            special: Special::None,
            replaced: false,
            rtl,
            align_logical: None,
            layer_revision: 0,
            style_writes: 0,
        };
        let id = if let Some(index) = self.free.pop() {
            self.slots[index as usize].node = Some(node);
            NodeId {
                index,
                generation: self.slots[index as usize].generation,
            }
        } else {
            let index = self.slots.len() as u32;
            self.slots_settled = 0;
            self.slots.push(Slot {
                generation: 0,
                dead: false,
                node: Some(node),
            });
            NodeId {
                index,
                generation: 0,
            }
        };
        self.node_mut(parent).children.get_mut().push(id);
        if let Some(given) = given {
            self.given_styles.insert(id, given);
        }
        if overflow_fade {
            self.overflow_fades.push((id, [0.; 4]));
        }
        self.dirty.push(id);
        self.invalidate_layout(parent);
        self.children_changed(parent);
        id
    }
    /// Remove `id` and its subtree, damaging where they painted. Panics for
    /// the root.
    pub fn remove(&mut self, id: NodeId) {
        self.remove_as::<false>(id, 0);
    }
    pub(crate) fn remove_indexed(&mut self, id: NodeId, index: usize) {
        self.remove_as::<true>(id, index);
    }
    #[inline(always)]
    fn remove_as<const INDEXED: bool>(&mut self, id: NodeId, index: usize) {
        if !INDEXED {
            self.settle_children();
        }
        self.content_revision = self.content_revision.wrapping_add(1);
        assert_ne!(id, self.root, "cannot remove scene root");
        self.damage_subtree(id);
        let parent = self.node(id).parent.unwrap();
        if INDEXED
            && self.pending_children.get() != u32::MAX
            && self.pending_children.get() != parent.index
        {
            self.finish_pending_children();
        }
        let tombstone = self.root;
        let children = self.node_mut(parent).children.get_mut();
        if INDEXED && parent.index != u32::MAX && children.get(index) == Some(&id) {
            children[index] = tombstone;
            self.pending_children.set(parent.index);
            self.filtered
                .set(self.filtered.get() | PENDING_CHILDREN_BIT);
        } else {
            if INDEXED {
                self.finish_pending_children();
            }
            self.node_mut(parent)
                .children
                .get_mut()
                .retain(|child| *child != id);
        }
        self.free_subtree(id);
        if !self.inert.is_empty() {
            let mut inert = std::mem::take(&mut self.inert);
            inert.retain(|n| self.contains(*n));
            self.inert = inert;
        }
        self.invalidate_layout(parent);
        self.children_changed(parent);
        // Hidden windows may retain damage for an arbitrarily long time while
        // virtual rows are replaced. Amortize stale-generation cleanup so that
        // the queue stays proportional to arena capacity, not update count.
        if self.dirty.len() > self.slots.len().saturating_mul(2).max(64) {
            let slots = &self.slots;
            self.dirty.retain(|id| {
                slots
                    .get(id.index as usize)
                    .is_some_and(|slot| slot.generation == id.generation && slot.node.is_some())
            });
        }
    }
    /// Take `id` and its subtree out of the tree at once, as `remove` does,
    /// leaving their slots to be freed later, one at a time (`free_dead`):
    /// returns their ids, `id` first, none of which the scene contains from
    /// now on (nothing reaches them: their generations have moved on, and
    /// the lists that name nodes no longer name them). For a large subtree:
    /// the whole viewport repaints, rather than each node's ink. Panics for
    /// the root.
    pub(crate) fn kill(&mut self, id: NodeId) -> Vec<NodeId> {
        self.finish_pending_children();
        self.content_revision = self.content_revision.wrapping_add(1);
        assert_ne!(id, self.root, "cannot remove scene root");
        self.add_damage(self.viewport);
        let parent = self.node(id).parent.unwrap();
        self.node_mut(parent)
            .children
            .get_mut()
            .retain(|child| *child != id);
        let mut ids = vec![id];
        let mut at = 0;
        while let Some(&id) = ids.get(at) {
            at += 1;
            let slot = &mut self.slots[id.index as usize];
            slot.generation = slot
                .generation
                .checked_add(1)
                .expect("node generation exhausted");
            slot.dead = true;
            // SAFETY: pending removals were settled above (`kill` starts with
            // `finish_pending_children`), and nothing borrows this vector.
            ids.extend_from_slice(unsafe { &*slot.node.as_ref().unwrap().children.get() });
        }
        self.dead += ids.len();
        let slots = &self.slots;
        let live = |id: &NodeId| slots[id.index as usize].generation == id.generation;
        self.blur_nodes.retain(live);
        self.popups.retain(live);
        self.popups_promoted.retain(live);
        self.overflow_fades.retain(|(id, _)| live(id));
        self.inert.retain(live);
        self.invalidate_layout(parent);
        self.children_changed(parent);
        ids
    }
    /// Free the slot of `id`, a node `kill` took out (and not its subtree,
    /// whose nodes come in turn).
    pub(crate) fn free_dead(&mut self, id: NodeId) {
        let slot = &mut self.slots[id.index as usize];
        assert!(slot.dead, "freeing a node that was not removed");
        let node = slot.node.take().unwrap();
        slot.dead = false;
        self.dead -= 1;
        self.forget(id, &node);
        self.free.push(id.index);
    }
    /// What the scene keeps about `node`, gone, but in the lists `kill`
    /// filters.
    fn forget(&mut self, id: NodeId, node: &Node) {
        self.selectable.forget(id);
        self.views.forget(id);
        self.cursor_styles -= usize::from(node.cursor.is_some());
        self.relative_count -= usize::from(node.style.relative);
        if node.isolated {
            self.isolation_count -= 1;
        }
        if node.special.any() {
            if node.contents() {
                self.contents_styles.remove(&id);
            }
            self.contents_count -= 1;
        }
        if !self.given_styles.is_empty() {
            self.given_styles.remove(&id);
        }
        if !self.directions.is_empty() {
            self.directions.remove(&id);
        }
        if !self.layer_shaders.is_empty() {
            self.layer_shaders.remove(&id);
        }
        self.transformed -= usize::from(node.transformed());
        self.filtered
            .set(self.filtered.get() - usize::from(node.filtered()));
        self.promoted_count -= usize::from(node.promoted);
    }
    fn free_subtree(&mut self, id: NodeId) {
        // Only remove_as enters this walk. It either materialized first or
        // left tombstones in id's parent, outside the freed subtree.
        debug_assert!(
            self.pending_children.get() == u32::MAX || self.pending_children.get() != id.index
        );
        let node = self.slots[id.index as usize].node.take().unwrap();
        self.forget(id, &node);
        if node.effects.blur_radius > 0.0 {
            self.blur_nodes.retain(|entry| *entry != id);
        }
        if !self.popups.is_empty() {
            self.popups.retain(|entry| *entry != id);
            self.popups_promoted.retain(|entry| *entry != id);
        }
        if node.style.fade_overflow {
            self.overflow_fades.retain(|(entry, _)| *entry != id);
        }
        for child in node.head.children.into_inner() {
            crate::deep(0, || self.free_subtree(child));
        }
        self.slots[id.index as usize].generation = self.slots[id.index as usize]
            .generation
            .checked_add(1)
            .expect("node generation exhausted");
        self.free.push(id.index);
    }
    /// Change a text node's text. Panics if `id` isn't a text node.
    pub fn set_text(&mut self, id: NodeId, value: impl Into<Arc<str>>) {
        let value = value.into();
        let node = self.node(id);
        let NodeKind::Text { text, .. } = &node.kind else {
            panic!("set_text requires a text node")
        };
        if *text == value {
            return;
        }
        let fixed = node.style.width.is_some() && node.style.height.is_some();
        self.damage_subtree(id);
        let node = self.node_mut(id);
        if let NodeKind::Text { text, .. } = &mut node.kind {
            *text = value;
        }
        // Sizes of the old text; a fixed box skips relayout, not these.
        node.text_size.set(TextSizes::default());
        self.invalidate(id, PAINT);
        if !fixed {
            self.invalidate_layout(id);
        }
    }
    /// Change `id`'s layout style, invalidating its layout.
    pub fn set_style(&mut self, id: NodeId, style: Style) {
        self.restyle(id, style, true);
    }
    /// `set_style` for a style that may differ from `id`'s only in its box
    /// (width, height, their percentages and margins: a bar growing every
    /// frame): those set and its layout redone, without the style copied
    /// or each of its fields weighed for what it repaints. Any other
    /// difference takes `set_style`.
    ///
    /// `kept`: what the last call left there, for a caller whose `style`
    /// differs from the one it gave that call only in its box. While no
    /// other write has changed `id`'s style since, the rest is known equal
    /// and not weighed again; `None` weighs it.
    pub(crate) fn set_box(&mut self, id: NodeId, style: &Style, kept: &mut Option<u32>) {
        let node = self.node(id);
        let old = &node.style;
        let known = *kept == Some(node.style_writes);
        // `way(id)`, from the node at hand: its parent the tree's own.
        let way = Way {
            own: node.rtl,
            parent: node.parent.is_some_and(|parent| self.tree_node(parent).rtl),
        };
        if (self.contents_count > 0 && node.special.any())
            || !laid_out_as_given(style, way)
            || !(known
                || match (&old.layout_options, &style.layout_options) {
                    (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                    (old, new) => old.is_none() && new.is_none(),
                } && old.eq_except_box(style))
        {
            *kept = None;
            self.set_style(id, style.clone());
            return;
        }
        let same = old.width == style.width
            && old.height == style.height
            && old.width_percent == style.width_percent
            && old.height_percent == style.height_percent
            && old.margin == style.margin;
        if !self.given_styles.is_empty() {
            self.given_styles.remove(&id);
        }
        if same {
            // Checked as it starts, as below.
            *kept = Some(self.tree_node(id).style_writes);
            return;
        }
        let node = self.tree_node_mut(id);
        node.style_writes = node.style_writes.wrapping_add(1);
        *kept = Some(node.style_writes);
        let old = &mut node.style;
        // Sizes for sizes leave what it and its ancestors read (`read_bits`:
        // whether it has a width or height, and whether as a percentage).
        let reads = old.width.is_some() == style.width.is_some()
            && old.height.is_some() == style.height.is_some()
            && old.width_percent.is_some() == style.width_percent.is_some()
            && old.height_percent.is_some() == style.height_percent.is_some();
        old.width = style.width;
        old.height = style.height;
        old.width_percent = style.width_percent;
        old.height_percent = style.height_percent;
        old.margin = style.margin;
        // Only its box changed: laid out again, it is `repaint::BOX_ONLY`.
        match reads {
            true => self.invalidate_layout_with(id, true, false),
            false => self.invalidate_layout_with(id, false, false),
        }
        // A contents parent lays out as its last child.
        if self.contents_count > 0
            && let Some(parent) = self.tree_node(id).parent
            && self.tree_node(parent).contents()
            && self.children_of(self.tree_node(parent)).last() == Some(&id)
        {
            self.sync_contents(parent);
        }
    }
    /// `set_style` for a contents node or a zoomed one (`Node::special`).
    #[cold]
    #[inline(never)]
    fn restyle_special(&mut self, id: NodeId, style: Style) {
        if self.node(id).contents() {
            self.set_contents_style(id, style);
            return;
        }
        // Zoomed: laid out as `snap`, so that `arrange` takes its box in its
        // own points from `snap` (`set_zoom`); the style given is kept.
        let mut laid = laid_out_style(style.clone(), self.way(id));
        laid.snap = true;
        // Its snap, which `snap_zoomed` reads from the given style, changes
        // nothing in the style it lays out as.
        let snapped = self.given_styles.get(&id).is_some_and(|given| given.snap);
        let snap = style.snap;
        self.given_styles.insert(id, style);
        self.restyle(id, laid, false);
        if snap != snapped {
            self.invalidate_layout(id);
        }
    }
    /// `set_style` for a contents node: keep `style` as its own, and lay it
    /// out as its last child in its place.
    #[cold]
    #[inline(never)]
    fn set_contents_style(&mut self, id: NodeId, style: Style) {
        let own = self.contents_styles.get(&id);
        let changed = match own {
            Some(own) => *own != style,
            None => style != Style::default(),
        };
        if changed {
            // Zoomed, it lays out as snap whatever its own says, which
            // `snap_zoomed` reads (as `restyle_special`).
            let snap = own.is_some_and(|own| own.snap) != style.snap;
            self.keep_contents_style(id, style);
            self.sync_contents(id);
            if snap && self.node(id).special.has(ZOOMED) {
                self.invalidate_layout(id);
            }
        }
    }
    /// Lay `id` out by `style` (normalized) from now on: what a contents
    /// node lays out as.
    fn apply_style(&mut self, id: NodeId, style: Style) {
        self.restyle(id, style, false);
    }
    /// `set_style` (`given`: the style a caller gave `id`, which a contents
    /// node keeps as its own) or `apply_style`. One function, so that the
    /// style is used where the caller put it rather than moved again.
    #[inline(never)]
    fn restyle(&mut self, id: NodeId, mut style: Style, given: bool) {
        if given {
            if let Some(options) = &style.layout_options
                && !self
                    .node(id)
                    .style
                    .layout_options
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, options))
            {
                let normalized = (**options).normalized();
                if normalized != **options {
                    style.layout_options = Some(Arc::new(normalized));
                }
            }
            if self.contents_count > 0 && self.node(id).special.any() {
                self.restyle_special(id, style);
                return;
            }
            let way = self.way(id);
            if !laid_out_as_given(&style, way) {
                let laid = laid_out_style(style.clone(), way);
                self.given_styles.insert(id, style);
                style = laid;
            } else if !self.given_styles.is_empty() {
                self.given_styles.remove(&id);
            }
        }
        let old = &self.node(id).style;
        if *old == style {
            return;
        }
        // Fields that only place and size nodes paint nothing themselves:
        // layout repaints what they move (`arrange` damages a node whose
        // bounds change, where it was and where it lands).
        let paints = old.clip != style.clip
            || old.fade_edges != style.fade_edges
            || old.fade_insets != style.fade_insets
            || old.fade_edges_x != style.fade_edges_x
            || old.fade_insets_x != style.fade_insets_x
            || old.fade_overflow != style.fade_overflow
            || old.padding != style.padding
            || old.padding_edges != style.padding_edges
            || old.text_wrap != style.text_wrap
            || old.text_options != style.text_options
            || old.layout_options != style.layout_options;
        let overflow_fade = old.fade_overflow != style.fade_overflow;
        if paints {
            self.damage_subtree(id);
        }
        if overflow_fade {
            match style.fade_overflow {
                true => self.overflow_fades.push((id, [0.; 4])),
                false => self.overflow_fades.retain(|(entry, _)| *entry != id),
            }
        }
        if own_visible(&self.node(id).style) != own_visible(&style) {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
        // Which block its absolute descendants position against.
        let old = &self.node(id).style;
        let containing = old.relative != style.relative
            || (self.relative_count > 0 && old.absolute != style.absolute);
        if old.relative != style.relative {
            self.relative_count =
                self.relative_count + usize::from(style.relative) - usize::from(old.relative);
        }
        let node = self.node_mut(id);
        node.style = style;
        node.style_writes = node.style_writes.wrapping_add(1);
        if paints {
            self.damage_subtree(id);
            self.invalidate(id, PAINT);
        }
        self.invalidate_layout(id);
        if containing {
            self.invalidate_absolute_descendants(id);
        }
        // A contents parent lays out as its last child.
        if self.contents_count > 0
            && let Some(parent) = self.node(id).parent
            && self.node(parent).contents()
            && self.children_of(self.node(parent)).last() == Some(&id)
        {
            self.sync_contents(parent);
        }
    }
    /// Lay out again every absolute node under `id`, and so its parent: its
    /// containing block may have changed (see `containing_block`).
    fn invalidate_absolute_descendants(&mut self, id: NodeId) {
        self.settle_children();
        let mut stack = self.children_raw(self.node(id)).clone();
        while let Some(node) = stack.pop() {
            if self.node(node).style.absolute {
                self.invalidate_layout(node);
            }
            stack.extend_from_slice(self.children_raw(self.node(node)));
        }
    }
    /// Resize the viewport (the root's box) to `width` by `height`.
    pub fn resize(&mut self, width: f32, height: f32) {
        if self.viewport.width == width && self.viewport.height == height {
            return;
        }
        self.viewport = Rect::new(0.0, 0.0, width, height);
        let mut style = self.node(self.root).style.clone();
        style.width = Some(width);
        style.height = Some(height);
        self.set_style(self.root, style);
        // Everything repaints; a grid would be the old size.
        self.damage_grid = None;
        self.damage.clear();
        self.add_damage(self.viewport);
    }
    /// Set a translation, normalizing each non-finite coordinate to zero.
    pub fn set_transform(&mut self, id: NodeId, mut transform: Transform) {
        let coordinate = |value: f32| {
            if value.is_finite() && value != 0.0 {
                value
            } else {
                0.0
            }
        };
        transform.x = coordinate(transform.x);
        transform.y = coordinate(transform.y);
        // What it needs of the node, read once (a move is the hottest
        // setter: a reorder animation moves every row each frame).
        let node = self.node(id);
        let (scroll_copy, from, transparent) =
            (node.scroll_copy, node.transform, node.effects.opacity <= 0.);
        if scroll_copy {
            transform = self.on_pixels(transform);
        }
        if from == transform {
            return;
        }
        if scroll_copy {
            // Resolved at flush: a copy plus the exposed strip, or full damage.
            match self.pending_moves.iter_mut().find(|(node, ..)| *node == id) {
                Some((.., again)) => *again = true,
                None => self.pending_moves.push((id, from, false)),
            }
            self.node_mut(id).transform = transform;
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
            self.invalidate(id, COMPOSITE);
            return;
        }
        // Fully transparent, it paints nothing in either place (a caret the
        // host draws, moving): nothing to damage.
        if transparent {
            self.node_mut(id).transform = transform;
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
            self.invalidate(id, COMPOSITE);
            return;
        }
        // Its old place and its new, under the same ancestors: their
        // context is worked out once.
        let mut context = None;
        self.damage_subtree_with(id, &mut context);
        self.node_mut(id).transform = transform;
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.damage_subtree_with(id, &mut context);
        self.invalidate(id, COMPOSITE);
    }
    /// Apply finite render effects. NaN opacity uses the opaque default;
    /// infinite opacity clamps to its endpoint. Non-finite filter radii disable
    /// their filter, and negative radii clamp to zero.
    pub fn set_effects(&mut self, id: NodeId, mut effects: Effects) {
        effects.opacity = finite_opacity(effects.opacity);
        effects.blur_radius = finite_radius(effects.blur_radius);
        effects.edge_fade = finite_radius(effects.edge_fade);
        effects.filter_blur = finite_radius(effects.filter_blur);
        effects.brightness = finite_factor(effects.brightness);
        effects.contrast = finite_factor(effects.contrast);
        effects.saturate = finite_factor(effects.saturate);
        self.write_effects(id, effects);
    }
    /// Apply effects whose components were normalized while resolving a style.
    /// Out of line: inlined into the style effect, its body grew the effect,
    /// costing every run of it (a mount's too).
    #[inline(never)]
    pub(crate) fn set_finite_effects(&mut self, id: NodeId, effects: Effects) {
        self.write_effects(id, effects);
    }
    // Both entry points keep their own body: public calls do not pay for
    // another call, and styles do not normalize inherited fields again.
    #[inline(always)]
    fn write_effects(&mut self, id: NodeId, effects: Effects) {
        if self.node(id).effects == effects {
            return;
        }
        if (self.node(id).effects.opacity <= 0.) != (effects.opacity <= 0.) {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
        let old_blur = self.node(id).effects.blur_radius > 0.0;
        if !old_blur && effects.blur_radius > 0.0 {
            self.blur_nodes.push(id);
        }
        if old_blur && effects.blur_radius == 0.0 {
            self.blur_nodes.retain(|entry| *entry != id);
        }
        // A content blur's spread damages under the old effects, then the new.
        // Opacity and colour filters paint where the node painted before:
        // that place is damaged once (an animated fade, every frame).
        let old = self.node(id).effects;
        let same_area =
            old.blur_radius == effects.blur_radius && old.filter_blur == effects.filter_blur;
        let mut context = None;
        if !same_area {
            self.damage_subtree_with(id, &mut context);
        }
        let old_filter = self.node(id).filtered();
        if old_filter != (effects.filter_blur > 0.) {
            if old_filter {
                self.filtered.set(self.filtered.get() - 1);
            } else {
                // A layer from now on: its texture outlives a blur back at 0.
                self.filtered.set(self.filtered.get() + 1);
                if !std::mem::replace(&mut self.node_mut(id).promoted, true) {
                    self.promoted_count += 1;
                }
            }
        }
        self.node_mut(id).effects = effects;
        self.damage_subtree_with(id, &mut context);
        self.invalidate(id, COMPOSITE);
    }
    /// `set_transform` and `set_effects` together, as an animation moving and
    /// fading a node writes them: its old and its new look are damaged once
    /// each, where the two calls damage each place twice.
    pub fn set_transform_and_effects(
        &mut self,
        id: NodeId,
        transform: Transform,
        effects: Effects,
    ) {
        let (transform, effects) = (finite_translation(transform), finite_effects(effects));
        self.write_transform_and_effects(id, transform, effects);
    }
    /// The style resolver has already normalized `effects`; the translation
    /// keeps the public setter's normalization and scroll-copy behavior.
    pub(crate) fn set_transform_and_finite_effects(
        &mut self,
        id: NodeId,
        transform: Transform,
        effects: Effects,
    ) {
        self.write_transform_and_effects(id, finite_translation(transform), effects);
    }
    /// Apply normalized translation and effects through either entry point.
    #[inline(always)]
    fn write_transform_and_effects(&mut self, id: NodeId, transform: Transform, effects: Effects) {
        let node = self.node(id);
        if node.scroll_copy || node.transform == transform || node.effects == effects {
            self.set_transform(id, transform);
            self.set_finite_effects(id, effects);
            return;
        }
        self.effects_changing(id, &effects);
        let mut context = None;
        self.damage_subtree_with(id, &mut context);
        self.filter_changing(id, &effects);
        let node = self.node_mut(id);
        node.transform = transform;
        node.effects = effects;
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
        self.damage_subtree_with(id, &mut context);
        self.invalidate(id, COMPOSITE);
    }
    /// Bookkeeping before a node's effects change, ahead of its old damage.
    // Keep bookkeeping in each entry even when its application body is shared,
    // so the public setter does not gain calls from the style specialization.
    #[inline(always)]
    fn effects_changing(&mut self, id: NodeId, effects: &Effects) {
        if (self.node(id).effects.opacity <= 0.) != (effects.opacity <= 0.) {
            self.geometry_revision = self.geometry_revision.wrapping_add(1);
        }
        let old_blur = self.node(id).effects.blur_radius > 0.0;
        if !old_blur && effects.blur_radius > 0.0 {
            self.blur_nodes.push(id);
        }
        if old_blur && effects.blur_radius == 0.0 {
            self.blur_nodes.retain(|entry| *entry != id);
        }
    }
    /// Bookkeeping between a node's old damage and its new effects.
    #[inline(always)]
    fn filter_changing(&mut self, id: NodeId, effects: &Effects) {
        let old_filter = self.node(id).filtered();
        if old_filter != (effects.filter_blur > 0.) {
            if old_filter {
                self.filtered.set(self.filtered.get() - 1);
            } else {
                // A layer from now on: its texture outlives a blur back at 0.
                self.filtered.set(self.filtered.get() + 1);
                if !std::mem::replace(&mut self.node_mut(id).promoted, true) {
                    self.promoted_count += 1;
                }
            }
        }
    }
    // Inlined: each caller's `flags` is a constant, so the bits that demote a
    // repaint (`PAINT_MIXED`, `CHANGED`) are worked out as it compiles.
    #[inline(always)]
    fn invalidate(&mut self, id: NodeId, flags: u8) {
        self.invalidate_with(id, flags, false, true);
    }
    /// `invalidate`, keeping (`keep_reads`) what a node's layout reads
    /// (`read_bits`) and its text sizes where a change leaves them. Inlined:
    /// each caller's `keep_reads` is a constant, `invalidate`'s false.
    /// `own`: the node itself changed, not one it lays out (`CHANGED`).
    #[inline(always)]
    fn invalidate_with(&mut self, id: NodeId, flags: u8, keep_reads: bool, own: bool) {
        if self.isolation_count + self.promoted_count > 0 {
            let mut current = Some(id);
            while let Some(ancestor) = current {
                let node = self.node_mut(ancestor);
                if (node.isolated || node.promoted)
                    && (ancestor != id || flags & (LAYOUT | PAINT | PAINTED) != 0)
                {
                    node.layer_revision = node.layer_revision.wrapping_add(1);
                }
                current = node.parent;
            }
        }
        let mut current = Some(id);
        while let Some(ancestor) = current {
            let node = self.node(ancestor);
            if node.bounds_valid.replace(0) & BOUNDS_VALID == 0 {
                break;
            }
            current = node.parent;
        }
        let node = self.node_mut(id);
        node.dirty |= flags;
        if flags & (PAINT | PAINT_ONLY) == PAINT {
            node.dirty |= PAINT_MIXED;
        }
        if own && flags != PAINTED {
            node.dirty |= CHANGED;
        }
        if flags & LAYOUT != 0 && !keep_reads {
            node.text_size.set(TextSizes::default());
            node.reads.set(0);
        }
        if !node.queued {
            node.queued = true;
            self.dirty.push(id);
        }
    }
    /// `invalidate(id, PAINTED)` for a node the layout walk just placed: it
    /// and its ancestors are reached through the tree, so are looked up
    /// unchecked (`tree_node`). The same marks in the same order.
    #[inline(always)]
    fn invalidate_placed(&mut self, id: NodeId) {
        if self.isolation_count + self.promoted_count > 0 {
            let mut current = Some(id);
            while let Some(ancestor) = current {
                let node = self.tree_node_mut(ancestor);
                if node.isolated || node.promoted {
                    node.layer_revision = node.layer_revision.wrapping_add(1);
                }
                current = node.parent;
            }
        }
        let mut current = Some(id);
        while let Some(ancestor) = current {
            let node = self.tree_node(ancestor);
            if node.bounds_valid.replace(0) & BOUNDS_VALID == 0 {
                break;
            }
            current = node.parent;
        }
        let node = self.tree_node_mut(id);
        node.dirty |= PAINTED;
        if !node.queued {
            node.queued = true;
            self.dirty.push(id);
        }
    }
    /// Mark `id` and the ancestors whose geometry can depend on it. The walk
    /// stops at a layout boundary: its size cannot depend on its content, so
    /// its parent keeps its layout and only the boundary's subtree is redone.
    fn invalidate_layout(&mut self, id: NodeId) {
        self.invalidate_layout_with(id, false, true);
    }
    /// `invalidate_layout`, keeping (`keep_reads`) what the nodes it marks
    /// read (see `invalidate_with`).
    /// `own`: something besides its box changed (`CHANGED`): `set_box` alone
    /// leaves the node a `repaint::BOX_ONLY` candidate.
    #[inline(always)]
    fn invalidate_layout_with(&mut self, id: NodeId, keep_reads: bool, own: bool) {
        let layers = self.isolation_count + self.promoted_count > 0;
        let mut current = id;
        let mut first = true;
        let mut marked = 0u64;
        let (mut parent, mut ink) = loop {
            // Past `id`, a parent the tree led to (`tree_node_mut`).
            let node = if first {
                self.node_mut(current)
            } else {
                self.tree_node_mut(current)
            };
            let boundary = !first && Self::is_layout_boundary(node);
            let parent = node.parent;
            let ink = node.bounds_valid.replace(0);
            node.dirty |= LAYOUT;
            if first && own {
                node.dirty |= CHANGED;
            }
            if !keep_reads {
                node.text_size.set(TextSizes::default());
                node.reads.set(0);
            }
            if layers {
                marked = marked.wrapping_add(1);
                if node.isolated || node.promoted {
                    node.layer_revision = node.layer_revision.wrapping_add(marked);
                }
            }
            if !node.queued {
                node.queued = true;
                self.dirty.push(current);
            }
            if boundary {
                if self.layout_roots.last() != Some(&current) {
                    self.layout_roots.push(current);
                }
                break (parent, ink);
            }
            match parent {
                Some(parent) => current = parent,
                None => return,
            }
            first = false;
        };
        // Each ancestor above the boundary used to be visited once per
        // marked node. Add exactly that many layer revisions in one visit.
        if layers {
            while let Some(id) = parent {
                let node = self.tree_node_mut(id);
                if node.isolated || node.promoted {
                    node.layer_revision = node.layer_revision.wrapping_add(marked);
                }
                if ink & BOUNDS_VALID != 0 {
                    ink = node.bounds_valid.replace(0);
                }
                parent = node.parent;
            }
        } else if ink & BOUNDS_VALID != 0 {
            while let Some(id) = parent {
                let node = self.tree_node(id);
                if node.bounds_valid.replace(0) & BOUNDS_VALID == 0 {
                    break;
                }
                parent = node.parent;
            }
        }
    }
    /// A laid-out container with an explicit width and height: parents measure
    /// it from those (or from their own flex allocation), never from its
    /// children. Not one a row or column has shrunk below its size along
    /// its axis: it shrinks no further than its content (`min-width: auto`),
    /// so its children say where it stops, and a child growing past that
    /// grows it again. At its size, content past it changes nothing (its
    /// floor is the lesser of the two).
    fn is_layout_boundary(node: &Node) -> bool {
        let (Some(width), Some(height), Some(measured)) = (
            node.style.width,
            node.style.height,
            node.measurement.as_ref(),
        ) else {
            return false;
        };
        matches!(node.kind, NodeKind::Container(_) | NodeKind::Panel { .. })
            && node.parent.is_some()
            && node.style.layout_options.is_none()
            // A row or column shrinks it along its axis alone: at its size
            // on both, it was not shrunk.
            && (node.style.flex_shrink == 0.
                || node.style.absolute
                || (measured.size.0 >= width && measured.size.1 >= height))
    }
    /// Mark every ancestor up to the root, ignoring boundaries.
    fn invalidate_layout_to_root(&mut self, mut id: NodeId) {
        let mut own = true;
        loop {
            self.invalidate_with(id, LAYOUT, false, std::mem::take(&mut own));
            match self.node(id).parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
    }
    fn depth(&self, mut id: NodeId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.node(id).parent {
            depth += 1;
            id = parent;
        }
        depth
    }
    /// Whether this frame already repaints the whole viewport: more damage
    /// changes nothing, so callers skip working out where it would be.
    fn damage_is_full(&self) -> bool {
        self.damage.first() == Some(&self.viewport)
            || self.damage_grid.as_ref().is_some_and(DamageGrid::full)
    }
    /// What `damage_subtree_with` would add for leaf `id` (no children)
    /// where nothing is transformed or filtered, at its box `was` and at
    /// `now`, where layout places it: worked out together, from one look at
    /// the node. With `CELLS` (`arrange::<true>`: the grid takes cells,
    /// `DamageGrid::mark_clipped_pair`) they are clipped but not rounded.
    /// Otherwise each is quantized as `add_damage` quantizes it: two such
    /// areas sharing their extent along an axis and overlapping are,
    /// together, their union. `context`: its parent's, as
    /// `damage_subtree_with` keeps it. Out of line: `arrange` stays small.
    #[inline(never)]
    fn leaf_damage<const CELLS: bool>(
        &self,
        id: NodeId,
        context: &mut Option<DamageContext>,
        was: Rect,
        now: Rect,
    ) -> (Option<Rect>, Option<Rect>) {
        if self.damage_is_full() {
            return (None, None);
        }
        // `arrange` checked it.
        let node = self.tree_node(id);
        let Some((parent, parent_clip)) =
            *context.get_or_insert_with(|| self.damage_context(node.parent))
        else {
            return (None, None);
        };
        // As `damage_subtree_in` and `damage_painted` work it out.
        let unplaced = node.measurement.is_none() && node.bounds == Rect::default();
        if !own_visible(&node.style) || unplaced {
            return (None, None);
        }
        let transform = Transform {
            x: parent.x + node.transform.x,
            y: parent.y + node.transform.y,
        };
        let clips = clip_axes(&node.style) != (false, false);
        let past_clip = clips && matches!(node.kind, NodeKind::Quad(_) | NodeKind::Panel { .. });
        // Both placements paint the same kind. Select it once; only their
        // geometry and clipping differ.
        macro_rules! pair {
            ($paint:expr) => {{
                let paint = $paint;
                let at = |bounds| {
                    self.leaf_rect::<CELLS>(
                        node,
                        bounds,
                        transform,
                        parent_clip,
                        clips,
                        past_clip,
                        &paint,
                    )
                };
                (at(was), at(now))
            }};
        }
        match &node.kind {
            NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => {
                if quad.decoration.is_none() && quad.shadow.is_none() && quad.outline.is_none() {
                    pair!(Some)
                } else {
                    pair!(|bounds| Some(quad.paint_bounds(bounds)))
                }
            }
            NodeKind::Image(image) => pair!(|bounds| Some(image.paint_bounds(bounds))),
            NodeKind::Svg(svg) => pair!(|bounds| Some(svg.paint_bounds(bounds))),
            NodeKind::Text { .. } | NodeKind::RichText { .. }
                if node.effects.blur_radius <= 0.0 =>
            {
                pair!(|bounds| Some(self.text_ink(id, bounds)))
            }
            NodeKind::Container(_) if node.effects.blur_radius <= 0.0 => (None, None),
            _ => pair!(Some),
        }
    }
    /// One of `leaf_damage`'s areas: leaf `id` at `bounds`, translated by
    /// `transform` within `parent_clip`. Inlined into it, twice.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn leaf_rect<const CELLS: bool>(
        &self,
        node: &Node,
        bounds: Rect,
        transform: Transform,
        parent_clip: Option<Rect>,
        clips: bool,
        past_clip: bool,
        paint: &impl Fn(Rect) -> Option<Rect>,
    ) -> Option<Rect> {
        let bounds = bounds.translated(transform);
        let clip = match clips {
            true => clipped_bounds(parent_clip, bounds, &node.style),
            false => parent_clip,
        };
        let own = match past_clip {
            true => parent_clip,
            false => clip,
        };
        let painted = paint(bounds)?;
        let rect = own.and_then(|clip| painted.intersection(clip));
        match CELLS {
            true => rect,
            false => rect.map(quantized),
        }
    }
    fn add_damage(&mut self, rect: Rect) {
        self.add_damage_as::<false>(rect);
    }
    /// Whether moved leaves' damage can go to the grid as cells
    /// (`DamageGrid::mark_clipped_pair`): with the grid active, in a
    /// viewport of 1 to 2^22 points on each axis, where whole-point edges
    /// are exact in f32. Asked as a layout walk starts (`arrange_from`):
    /// the grid, once active, stays so until the frame's damage is taken.
    fn grid_takes_cells(&self) -> bool {
        #[cfg(test)]
        if self.cells_off {
            return false;
        }
        self.damage_grid.is_some()
            && (1.0..=4_194_304.).contains(&self.viewport.width)
            && (1.0..=4_194_304.).contains(&self.viewport.height)
    }
    /// A moved leaf's damage where it was and where it lands (`leaf_damage`).
    fn add_moved_leaf_damage(&mut self, before: Option<Rect>, after: Option<Rect>) {
        match (before, after) {
            // A box growing or moving along one axis (a bar): one area, as
            // the two would be marked or merged.
            (Some(before), Some(after))
                if (before.x == after.x && before.width == after.width
                    || before.y == after.y && before.height == after.height)
                    && before.intersects(after) =>
            {
                self.add_leaf_damage(before.union(after));
            }
            (before, after) => {
                for rect in [before, after].into_iter().flatten() {
                    self.add_leaf_damage(rect);
                }
            }
        }
    }
    /// `add_damage` for a moved leaf (`arrange`), the grid first. `rect`
    /// is quantized already (`leaf_damage`).
    #[inline(never)]
    fn add_leaf_damage(&mut self, rect: Rect) {
        self.add_damage_as::<true>(rect);
    }
    /// `add_damage`, looking for the grid before a recent rectangle that
    /// holds `rect` when `GRID_FIRST`: on the grid, marking cells again
    /// changes nothing, so the check is work for no change. Where most
    /// damage goes to a list (small scenes, a frame's first changes), the
    /// check comes first, as it then saves merging. Layout's moved leaves
    /// (bars, fills: hundreds a frame) take the grid first.
    #[inline(always)]
    fn add_damage_as<const GRID_FIRST: bool>(&mut self, rect: Rect) {
        // Raster coverage rounds fractional edges outwards. Quantize before merging
        // so adjacent subpixel rectangles cannot repaint the same alpha pixel twice,
        // and every primitive covering a touched pixel participates in its repaint.
        // A moved leaf's (`GRID_FIRST`) comes quantized from `leaf_damage`, or
        // as the union of two that did: whole points, which quantize to
        // themselves.
        let rect = if GRID_FIRST { rect } else { quantized(rect) };
        let Some(mut rect) = rect.intersection(self.viewport) else {
            return;
        };
        if GRID_FIRST && let Some(grid) = &mut self.damage_grid {
            grid.mark(rect);
            return;
        }
        let edges = DamageEdges::from(rect);
        let holds = |outer: &DamageEdges| {
            (outer.0[0] <= edges.0[0])
                & (outer.0[1] <= edges.0[1])
                & (outer.0[2] <= edges.0[2])
                & (outer.0[3] <= edges.0[3])
        };
        if self.recent_damage.iter().any(holds) {
            return;
        }
        self.recent_damage.rotate_right(1);
        self.recent_damage[0] = edges;
        if !GRID_FIRST && let Some(grid) = &mut self.damage_grid {
            grid.mark(rect);
            return;
        }
        // Each rectangle it meets joins it, from the first again after each
        // (it grew): one tight scan a join, no index kept across the removal.
        while !self.damage.is_empty() {
            // `rect.intersects(other)`, its own edges worked out once a scan.
            let (x, y) = (rect.x, rect.y);
            let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
            let meets = move |other: &Rect| {
                x < other.x + other.width
                    && other.x < right
                    && y < other.y + other.height
                    && other.y < bottom
            };
            let Some(index) = self.damage.iter().position(meets) else {
                break;
            };
            rect = self.damage.swap_remove(index).union(rect);
        }
        self.damage.push(rect);
        if self.damage.len() > DAMAGE_RECTS {
            self.damage_on_grid();
        }
    }
    /// Mark the damage on a grid, or join it into one rectangle where the
    /// viewport is too large for one.
    #[cold]
    fn damage_on_grid(&mut self) {
        match DamageGrid::new(self.viewport) {
            Some(mut grid) => {
                for rect in self.damage.drain(..) {
                    grid.mark(rect);
                }
                self.damage_grid = Some(grid);
            }
            None => {
                let joined = self.damage.drain(..).reduce(Rect::union);
                self.damage.extend(joined);
            }
        }
    }
    fn world(&self, id: NodeId) -> (Rect, Effects, Option<Rect>) {
        let node = self.node(id);
        let mut transform = node.transform;
        let mut effects = node.effects;
        if !own_visible(&node.style) {
            effects.opacity = 0.;
        }
        let mut parent = node.parent;
        while let Some(id) = parent {
            let ancestor = self.tree_node(id);
            transform.x += ancestor.transform.x;
            transform.y += ancestor.transform.y;
            effects.opacity *= if own_visible(&ancestor.style) {
                ancestor.effects.opacity
            } else {
                0.
            };
            parent = ancestor.parent;
        }
        let bounds = node.bounds.translated(transform);
        (bounds, effects, self.world_clip(id))
    }
    /// `world`'s clip alone (`.2`): what `id` paints within, from its own
    /// and its ancestors' clips, each at its box translated by its own and
    /// its ancestors' translations, summed from it upwards.
    fn world_clip(&self, id: NodeId) -> Option<Rect> {
        let mut clip = Some(self.viewport);
        let mut ancestor = self.node(id);
        loop {
            if clip_axes(&ancestor.style) != (false, false) {
                let mut offset = ancestor.transform;
                let mut parent = ancestor.parent;
                while let Some(parent_id) = parent {
                    let p = self.tree_node(parent_id);
                    offset.x += p.transform.x;
                    offset.y += p.transform.y;
                    parent = p.parent;
                }
                clip = clipped_bounds(clip, ancestor.bounds.translated(offset), &ancestor.style);
            }
            let Some(parent) = ancestor.parent else {
                break;
            };
            ancestor = self.tree_node(parent);
        }
        clip
    }
    fn damage_subtree(&mut self, id: NodeId) {
        self.damage_subtree_with(id, &mut None);
    }
    /// What `damage_subtree` works out for a parent's children: nothing if
    /// the parent is not shown, else their translation and clip.
    // Inlined, as `shown_context`: see there.
    #[inline(always)]
    fn damage_context(&self, parent: Option<NodeId>) -> DamageContext {
        match parent {
            Some(parent) => self.shown_context(parent),
            None => Some((Transform::default(), Some(self.viewport))),
        }
    }
    /// `damage_subtree`, for one of many siblings: their parent's `context`
    /// is worked out by the first and kept for the rest. Valid while the
    /// ancestors keep their places, clips and visibility.
    fn damage_subtree_with(&mut self, id: NodeId, context: &mut Option<DamageContext>) {
        // Hidden, or never laid out (a mounting row's nodes, each closing in
        // turn): it damages nothing (`damage_subtree_in`), so its ancestors
        // need no working out.
        let node = self.node(id);
        let unplaced = node.measurement.is_none() && node.bounds == Rect::default();
        if !own_visible(&node.style) || unplaced || self.damage_is_full() {
            return;
        }
        // Resolve the ancestors once, then carry translation and clipping down
        // instead of walking back up from every descendant.
        let special = self.transformed + self.filtered.get();
        if special > 0 {
            if (special as isize) < 0 {
                return self.damage_pending_subtree(id, context);
            }
            // Entry already established this node's visibility.
            let (transform, clip, placement) = match self.node(id).parent {
                Some(parent) => {
                    if !self.shown(parent) {
                        return;
                    }
                    self.context(parent)
                }
                None => (Transform::default(), Some(self.viewport), None),
            };
            self.damage_subtree_placed::<false>(id, transform, clip, placement, 0);
            return;
        }
        let parent = self.node(id).parent;
        let context = *context.get_or_insert_with(|| self.damage_context(parent));
        if let Some((transform, clip)) = context {
            self.damage_subtree_in::<false>(id, transform, clip, 0);
        }
    }
    #[cold]
    #[inline(never)]
    fn damage_pending_subtree(&mut self, id: NodeId, context: &mut Option<DamageContext>) {
        self.settle_damage_children(id);
        // The caller already checked visibility, placement and full damage.
        if self.transformed + (self.filtered.get() & !PENDING_CHILDREN_BIT) > 0 {
            // Entry already established this node's visibility.
            let (transform, clip, placement) = match self.node(id).parent {
                Some(parent) => {
                    if !self.shown(parent) {
                        return;
                    }
                    self.context(parent)
                }
                None => (Transform::default(), Some(self.viewport), None),
            };
            self.damage_subtree_placed::<false>(id, transform, clip, placement, 0);
            return;
        }
        let parent = self.node(id).parent;
        let context = *context.get_or_insert_with(|| self.damage_context(parent));
        if let Some((transform, clip)) = context {
            self.damage_subtree_in::<false>(id, transform, clip, 0);
        }
    }
    #[cold]
    #[inline(never)]
    fn settle_damage_children(&self, id: NodeId) {
        // A sibling removal walks below the pending parent, never through it.
        if self
            .node(id)
            .parent
            .is_none_or(|parent| parent.index != self.pending_children.get())
        {
            self.finish_pending_children();
        }
    }
    /// `level`: how deep in the walk (`deep`).
    #[inline(never)]
    fn damage_subtree_in<const CHECK: bool>(
        &mut self,
        id: NodeId,
        parent: Transform,
        parent_clip: Option<Rect>,
        level: u32,
    ) {
        let node = self.node(id);
        // Not laid out since it was appended, so never painted: nothing to
        // repaint yet (a mounting row styles each node before its first
        // layout, which damages where it lands). Nor are its descendants.
        if CHECK {
            let unplaced = node.measurement.is_none() && node.bounds == Rect::default();
            if !own_visible(&node.style) || unplaced || self.damage_is_full() {
                return;
            }
        }
        let transform = Transform {
            x: parent.x + node.transform.x,
            y: parent.y + node.transform.y,
        };
        let bounds = node.bounds.translated(transform);
        let clip = if clip_axes(&node.style) != (false, false) {
            clipped_bounds(parent_clip, bounds, &node.style)
        } else {
            parent_clip
        };
        let own = match self.paints_past_clip(id) {
            true => parent_clip,
            false => clip,
        };
        self.damage_painted(id, bounds, node.effects.blur_radius, own);
        for index in 0..self.children_raw(self.node(id)).len() {
            let child = self.children_raw(self.node(id))[index];
            crate::deep(level + 1, || {
                self.damage_subtree_in::<true>(child, transform, clip, level + 1)
            });
        }
    }
    /// `damage_subtree_in` where local transforms exist (see `descend`).
    #[inline(never)]
    fn damage_subtree_placed<const CHECK: bool>(
        &mut self,
        id: NodeId,
        parent: Transform,
        parent_clip: Option<Rect>,
        placement: Option<Placement>,
        level: u32,
    ) {
        let node = self.node(id);
        if CHECK {
            let unplaced = node.measurement.is_none() && node.bounds == Rect::default();
            if !own_visible(&node.style) || unplaced || self.damage_is_full() {
                return;
            }
        }
        let (transform, bounds, clip, placement) =
            self.descend(node, parent, parent_clip, placement);
        // Inside a layer it starts, nothing cuts its own paint.
        let own = match self.paints_past_clip(id) {
            true if node.transformed() || node.filtered() => None,
            true => parent_clip,
            false => clip,
        };
        self.damage_painted_placed(id, bounds, node.effects.blur_radius, own, placement);
        for index in 0..self.children_raw(self.node(id)).len() {
            let child = self.children_raw(self.node(id))[index];
            crate::deep(level + 1, || {
                self.damage_subtree_placed::<true>(child, transform, clip, placement, level + 1)
            });
        }
    }
    /// What `node` paints under, given what its parent passes on: its
    /// translation, translated box, clip (in its layer's space; `None` cuts
    /// nothing, only inside a transformed layer) and its layer's placement.
    /// A local transform starts a layer: the clip around it moves into the
    /// placement, leaving only the node's own clip inside.
    fn descend(
        &self,
        node: &Node,
        parent: Transform,
        parent_clip: Option<Rect>,
        placement: Option<Placement>,
    ) -> (Transform, Rect, Option<Rect>, Option<Placement>) {
        let transform = Transform {
            x: parent.x + node.transform.x,
            y: parent.y + node.transform.y,
        };
        let bounds = node.bounds.translated(transform);
        if !node.transformed() && !node.filtered() {
            let clip = if clip_axes(&node.style) != (false, false) {
                clipped_bounds(parent_clip, bounds, &node.style)
            } else {
                parent_clip
            };
            return (transform, bounds, clip, placement);
        }
        // A content blur starts a layer as a transform does: it spreads
        // what the layer paints, in the layer's space, and its own clip
        // applies inside, before the blur.
        let matrix = node.projective(bounds);
        let spread = match node.filtered() {
            true => filter_spread(node.effects.filter_blur) * stretch(matrix),
            false => 0.,
        };
        let placement = match placement {
            None => Placement {
                matrix,
                spread,
                clip: parent_clip.unwrap_or(self.viewport),
            },
            Some(outer) => Placement {
                matrix: matrix.then(outer.matrix),
                spread: outer.spread + spread * stretch(outer.matrix),
                // The outer layer's blur reaches past clips inside it.
                clip: match parent_clip {
                    Some(clip) => outer.shows(clip).unwrap_or_default(),
                    None => outer.clip,
                },
            },
        };
        let clip = clip_axes(&node.style)
            .ne(&(false, false))
            .then(|| clipped_bounds(None, bounds, &node.style))
            .flatten();
        (transform, bounds, clip, Some(placement))
    }
    /// What `id`'s children paint under (see `descend`), from the root down.
    fn context(&self, id: NodeId) -> (Transform, Option<Rect>, Option<Placement>) {
        let node = self.node(id);
        let (transform, clip, placement) = match node.parent {
            Some(parent) => self.context(parent),
            None => (Transform::default(), Some(self.viewport), None),
        };
        let (transform, _, clip, placement) = self.descend(node, transform, clip, placement);
        (transform, clip, placement)
    }
    /// If `id` is shown, its world translation including its own and the
    /// clip its children paint within. One walk up its ancestors checks they
    /// show and sums the translation (each of those walked them again);
    /// only an ancestor that clips sends the clip to `world`.
    // Inlined into `damage_subtree_with` and `leaf_damage`: damage runs it
    // for every node damaged alone.
    #[inline(always)]
    fn shown_context(&self, id: NodeId) -> DamageContext {
        let mut transform = Transform::default();
        let mut clips = false;
        let mut current = Some(id);
        while let Some(at) = current {
            let node = self.node(at);
            if !own_visible(&node.style) {
                return None;
            }
            transform.x += node.transform.x;
            transform.y += node.transform.y;
            clips |= clip_axes(&node.style) != (false, false);
            current = node.parent;
        }
        let clip = match clips {
            true => self.world_clip(id),
            false => Some(self.viewport),
        };
        Some((transform, clip))
    }
    /// See `Node::ink`. Computed on demand and cached until invalidated.
    fn ink(&self, id: NodeId) -> Option<Rect> {
        let node = self.node(id);
        if node.bounds_valid.get() & INK_VALID != 0 {
            return node.ink.get();
        }
        if node.effects.opacity <= 0.0 || !own_visible(&node.style) {
            node.ink.set(None);
            node.bounds_valid.set(INK_VALID | BOUNDS_VALID);
            return None;
        }
        // Its overflow clip cuts what it holds; a box's own paint (its
        // shadows) reaches past it, as the paint walks draw it.
        let own = self.own_ink(id);
        let mut held: Option<Rect> = None;
        for child in self.children_raw(node) {
            if let Some(child) = crate::deep(0, || self.ink(*child)) {
                held = Some(held.map_or(child, |ink| ink.union(child)));
            }
        }
        let held = held.and_then(|ink| clipped_bounds(Some(ink), node.bounds, &node.style));
        let own = match &node.kind {
            NodeKind::Quad(_) | NodeKind::Panel { .. } => own,
            _ => own.and_then(|ink| clipped_bounds(Some(ink), node.bounds, &node.style)),
        };
        let ink = match (own, held) {
            (Some(own), Some(held)) => Some(own.union(held)),
            (own, held) => own.or(held),
        };
        // A layer shader may paint anywhere in its box.
        let ink = match node.layer_shaded {
            true => Some(ink.map_or(node.bounds, |ink| ink.union(node.bounds))),
            false => ink,
        };
        let ink = ink
            .map(|ink| match node.filtered() {
                false => ink,
                // Blurred in its layer's space, before the transform.
                true => ink.expand(filter_spread(node.effects.filter_blur)),
            })
            .map(|ink| match node.transformed() {
                false => ink,
                // About its untranslated box: translating after commutes.
                true => node.projective(node.bounds).bounds(ink),
            })
            .map(|ink| ink.translated(node.transform));
        node.ink.set(ink);
        node.bounds_valid.set(INK_VALID | BOUNDS_VALID);
        ink
    }
    /// Pixels this node itself can touch, matching what damage covers.
    fn own_ink(&self, id: NodeId) -> Option<Rect> {
        let node = self.node(id);
        let blur = node.effects.blur_radius;
        if matches!(node.kind, NodeKind::Container(_))
            && blur <= 0.0
            && !self.platform_nodes.contains(&id)
        {
            return None;
        }
        let bounds = node.bounds;
        Some(match &node.kind {
            NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad.paint_bounds(bounds),
            NodeKind::Image(image) => image.paint_bounds(bounds),
            NodeKind::Svg(svg) => svg.paint_bounds(bounds),
            NodeKind::Text { .. } | NodeKind::RichText { .. } => {
                self.highlight_ink(id, bounds, self.text_ink(id, bounds))
            }
            _ => bounds,
        })
    }
    /// Where a text node's glyphs can land: its lines as laid out at its
    /// width, padded for glyph overhang, rather than its whole box. Text that
    /// changes every frame (a stream in a larger box) repaints only that. Text
    /// laid out otherwise (line clamps, ellipses, padding) or right to left
    /// keeps its box.
    // Out of line: `painted`, which every damaged node runs, stays small.
    #[inline(never)]
    fn text_ink(&self, id: NodeId, bounds: Rect) -> Rect {
        let node = self.node(id);
        let style = &node.style;
        let padded = style.padding != 0.
            || style
                .padding_edges
                .is_some_and(|edges| edges != Insets::default());
        if style.text_options != Default::default() || padded {
            return bounds;
        }
        // Renderers lay text out at its box's width, wrapping or not.
        let limit = Some(bounds.width);
        // What layout measured serves when it breaks lines as `limit` would:
        // a size measured wider (or unwrapped) whose widest line fits.
        let revision = self.text_shaper_revision;
        let cached = node
            .text_size
            .0
            .as_array_of_cells()
            .iter()
            .filter_map(std::cell::Cell::get)
            .find_map(|(measured, at, size)| {
                let same = at == revision
                    && match (limit, measured) {
                        (None, None) => true,
                        (Some(width), None) => size.0 <= width,
                        (Some(width), Some(measured)) => {
                            measured == width || (measured >= width && size.0 <= width)
                        }
                        (None, Some(_)) => false,
                    };
                same.then_some(size)
            });
        // Measured here, the size serves this node's next damage too: a box
        // of fixed size is not measured by layout.
        let remember = |size: (f32, f32)| {
            if cached.is_none() {
                TextSizes::remember(&node.text_size, limit, revision, size);
            }
            size
        };
        // ASCII holds none: a reply streaming in is checked a word at a
        // time, not a character at a time.
        #[inline(always)]
        fn rtl(text: &str, flags: &std::cell::Cell<u8>) -> bool {
            let validity = flags.get();
            if validity & NON_RTL != 0 {
                return false;
            }
            let rtl = !text.is_ascii()
                && text.chars().any(|c| {
                matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
            });
            if !rtl {
                flags.set(validity | NON_RTL);
            }
            rtl
        }
        let (size, font_size, line, align) = match &node.kind {
            NodeKind::Text {
                text, font_size, ..
            } => {
                // Headless approximations are not what a renderer draws.
                if rtl(text, &node.bounds_valid)
                    || (self.font_measurer.is_none() && self.font_shaper.is_none())
                {
                    return bounds;
                }
                let size = remember(cached.unwrap_or_else(|| match &self.font_measurer {
                    Some(measure) => measure(text, *font_size, limit, &node.font),
                    None => self.measure_text_with_font(text, *font_size, limit, &node.font),
                }));
                let line = node.font.line_height.resolve(*font_size);
                (size, *font_size, line, node.font.align)
            }
            NodeKind::RichText { text } => {
                if text.options() != Default::default()
                    || rtl(text.text(), &node.bounds_valid)
                    || !self.rich_measurer.is_native()
                {
                    return bounds;
                }
                let size = remember(
                    cached.unwrap_or_else(|| self.rich_measurer.measure_mounted(text, limit)),
                );
                let font_size = text
                    .runs()
                    .iter()
                    .map(|run| run.font_size)
                    .fold(0., f32::max);
                let line = text
                    .runs()
                    .iter()
                    .map(|run| run.font.line_height.resolve(run.font_size))
                    .fold(0., f32::max);
                let align = text
                    .runs()
                    .first()
                    .map_or_else(Default::default, |run| run.font.align);
                (size, font_size, line, align)
            }
            _ => return bounds,
        };
        // Lines starting inside the box are drawn whole.
        let (width, height) = (size.0, size.1.min(bounds.height + line));
        if !(width.is_finite() && height.is_finite()) {
            return bounds;
        }
        let x = match align {
            crate::text_layout::TextAlign::Start | crate::text_layout::TextAlign::Left => bounds.x,
            crate::text_layout::TextAlign::Center => bounds.x + (bounds.width - width) / 2.,
            crate::text_layout::TextAlign::Right | crate::text_layout::TextAlign::End => {
                bounds.x + bounds.width - width
            }
        };
        // Italic and antialiased edges reach past advances and line boxes.
        let pad = (font_size * 0.3).max(2.);
        Rect::new(x - pad, bounds.y - pad, width + 2. * pad, height + 2. * pad)
    }
    /// Whether anything `id` holds is displayed: a child neither
    /// `display: none` nor invisible, looking into a contents child (which
    /// has no box of its own) for what it holds.
    pub(crate) fn displays_content(&self, id: NodeId) -> bool {
        self.children(id).iter().any(|&child| {
            let node = self.node(child);
            if node.contents() {
                let own = self.contents_styles.get(&child).unwrap_or(&node.style);
                own_visible(own) && self.displays_content(child)
            } else {
                own_visible(&node.style)
            }
        })
    }
    /// Whether `id` and its ancestors are displayed: a hidden node paints
    /// nothing, so its changes damage nothing. Hiding one damages what it
    /// painted before the change.
    fn shown(&self, id: NodeId) -> bool {
        let mut node = self.node(id);
        loop {
            if !own_visible(&node.style) {
                return false;
            }
            let Some(parent) = node.parent else {
                return true;
            };
            node = self.tree_node(parent);
        }
    }
    fn damage_node(&mut self, id: NodeId) {
        if self.damage_is_full() || !self.shown(id) {
            return;
        }
        let past_clip = self.paints_past_clip(id);
        if self.transformed + self.filtered.get() > 0 {
            let (bounds, clip, placement) = self.placed(id);
            let node = self.node(id);
            let blur_radius = node.effects.blur_radius;
            let clip = match (past_clip, node.parent) {
                (false, _) => clip,
                _ if node.transformed() || node.filtered() => None,
                (true, Some(parent)) => self.context(parent).1,
                (true, None) => Some(self.viewport),
            };
            self.damage_painted_placed(id, bounds, blur_radius, clip, placement);
            return;
        }
        let (bounds, effects, clip) = self.world(id);
        let clip = match (past_clip, self.node(id).parent) {
            (false, _) => clip,
            (true, Some(parent)) => self.world_clip(parent),
            (true, None) => Some(self.viewport),
        };
        self.damage_painted(id, bounds, effects.blur_radius, clip);
    }
    fn damage_painted(&mut self, id: NodeId, bounds: Rect, blur_radius: f32, clip: Option<Rect>) {
        if let Some(bounds) = self.painted(id, bounds, blur_radius)
            // A backdrop filter samples only inside its bounds.
            && let Some(bounds) = clip.and_then(|clip| bounds.intersection(clip))
        {
            self.add_damage(bounds);
        }
    }
    /// Damage what `id` itself paints at `bounds` within `clip`, mapped onto
    /// the viewport by its layer's `placement`. Inside a transformed layer, no
    /// `clip` cuts nothing; outside one, everything.
    fn damage_painted_placed(
        &mut self,
        id: NodeId,
        bounds: Rect,
        blur_radius: f32,
        clip: Option<Rect>,
        placement: Option<Placement>,
    ) {
        let Some(placement) = placement else {
            return self.damage_painted(id, bounds, blur_radius, clip);
        };
        if let Some(bounds) = self
            .painted(id, bounds, blur_radius)
            .and_then(|bounds| bounds.intersection(clip.unwrap_or(UNCLIPPED)))
            .and_then(|visible| placement.shows(visible))
        {
            self.add_damage(bounds);
        }
    }
    /// Whether `id` clips what it holds but not its own paint: a box, whose
    /// shadows and outline reach past its overflow clip as the paint walks
    /// draw them. Its own paint is damaged within its parent's clip then.
    fn paints_past_clip(&self, id: NodeId) -> bool {
        let node = self.node(id);
        matches!(node.kind, NodeKind::Quad(_) | NodeKind::Panel { .. })
            && clip_axes(&node.style) != (false, false)
    }
    /// What `id` itself paints when laid out at `bounds`, if anything.
    #[inline]
    fn painted(&self, id: NodeId, bounds: Rect, blur_radius: f32) -> Option<Rect> {
        self.painted_of(id, self.node(id), bounds, blur_radius)
    }
    /// `painted` for `id` whose node the caller has: no lookup again (a moved
    /// leaf's damage asks twice a frame, before and after it moves).
    #[inline]
    fn painted_of(&self, id: NodeId, node: &Node, bounds: Rect, blur_radius: f32) -> Option<Rect> {
        if matches!(node.kind, NodeKind::Container(_)) && blur_radius <= 0.0 {
            return None;
        }
        Some(match &node.kind {
            NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => quad.paint_bounds(bounds),
            NodeKind::Image(image) => image.paint_bounds(bounds),
            NodeKind::Svg(svg) => svg.paint_bounds(bounds),
            NodeKind::Text { .. } | NodeKind::RichText { .. } if blur_radius <= 0.0 => {
                self.text_ink(id, bounds)
            }
            _ => bounds,
        })
    }
    /// `constraints` with what `id`'s result cannot depend on dropped, so
    /// probes that differ only there share one result: an available size
    /// a forced one overrides, percentage bases it has no percentages
    /// against, and available sizes a leaf never reads (text lays out at
    /// its available width only when it wraps).
    fn relevant(&self, id: NodeId, constraints: Constraints) -> Constraints {
        // Every probe asks, cache hits too: the answers are cached together.
        relevant_to(self.node(id), self.reads_bits(id), constraints)
    }
    /// `relevant` for `id`'s node, already in hand: one lookup for
    /// `measured`, which reads the node too.
    #[inline(always)]
    fn relevant_in(&self, id: NodeId, node: &Node, constraints: &mut Constraints) {
        let cached = node.reads.get();
        let bits = match cached & READS_KNOWN {
            0 => self.reads_bits(id),
            _ => cached,
        };
        relevant_to_in_place(node, bits, constraints);
    }
    /// The room `id`'s border takes in layout (`QuadStyle::layout_border`).
    pub(super) fn layout_border(&self, id: NodeId) -> Insets {
        match &self.node(id).kind {
            NodeKind::Panel { quad, .. } => quad.layout_border(),
            _ => Insets::default(),
        }
    }
    /// Where `id`'s content box sits inside its box, from its most recent
    /// layout: its padding (percentages resolved) plus its border.
    pub fn content_insets(&self, id: NodeId) -> Insets {
        let padding = self.padding(id);
        let border = self.layout_border(id);
        Insets {
            top: padding.top.max(0.) + border.top,
            right: padding.right.max(0.) + border.right,
            bottom: padding.bottom.max(0.) + border.bottom,
            left: padding.left.max(0.) + border.left,
        }
    }
    /// The containing block of absolute node `id` when it is not its parent:
    /// the nearest `relative` ancestor above the parent, the search
    /// stopping at an `absolute` one (which contains its own absolute
    /// descendants). `None`: its parent places it, as it does any child.
    fn containing_block(&self, id: NodeId) -> Option<NodeId> {
        if self.relative_count == 0 || !self.node(id).style.absolute {
            return None;
        }
        let parent = self.node(id).parent?;
        let style = &self.node(parent).style;
        if style.relative || style.absolute {
            return None;
        }
        if self.node(parent).zoom() != 1. {
            return None;
        }
        let mut current = self.node(parent).parent;
        while let Some(ancestor) = current {
            let node = self.node(ancestor);
            if node.style.relative {
                return Some(ancestor);
            }
            // A zoomed node holds what is placed within it, in its points.
            if node.style.absolute || node.zoom() != 1. {
                return None;
            }
            current = node.parent;
        }
        None
    }
    /// Whether `id`'s parent leaves it out of its own layout, for its
    /// containing block to place (`place_escaped`).
    fn escapes(&mut self, id: NodeId) -> bool {
        let escapes = self.containing_block(id).is_some();
        if escapes {
            self.escaped.push(id);
        }
        escapes
    }
    /// Whether `id` resolves percentages against the (width, height) basis
    /// its parent passes (`Constraints::percent_width`, `percent_height`): its
    /// own percentage sizes, limits and padding. Its children's bases come
    /// from its own size.
    pub(super) fn percent_bases(&self, id: NodeId) -> (bool, bool) {
        let node = self.node(id);
        let style = &node.style;
        let options = style.layout_options.as_deref();
        let percent = |length: Option<crate::layout::Length>| {
            matches!(length, Some(crate::layout::Length::Percent(_)))
        };
        let widths = node.parent.is_some()
            && (style.width_percent.is_some()
                || options.is_some_and(|o| {
                    percent(o.min_width)
                        || percent(o.max_width)
                        || [
                            o.padding.left,
                            o.padding.right,
                            o.padding.top,
                            o.padding.bottom,
                        ]
                        .into_iter()
                        .any(percent)
                }));
        let heights = style.height_percent.is_some()
            || options.is_some_and(|o| percent(o.min_height) || percent(o.max_height));
        (widths, heights)
    }
    /// Whether `id`'s size can depend on the available (width, height) it is
    /// measured under, unless forced. Text reads its width when it wraps;
    /// other leaves size themselves. A container passes what it is given to
    /// its children unless its own size fixes it, so it reads what they do;
    /// a taffy-laid-out one also fits its auto width to the room, and wraps
    /// a column or sizes grid tracks within the height. Most subtrees read
    /// no height: probes that differ only in it share one result. Cached
    /// until its layout is invalidated, which reaches every ancestor up to a
    /// fixed-size boundary (which reads neither).
    fn reads_available(&self, id: NodeId) -> (bool, bool) {
        let bits = self.reads_bits(id);
        (bits & READS_WIDTH != 0, bits & READS_HEIGHT != 0)
    }
    /// Whether anything in `id`'s subtree sizes itself by a percentage of a
    /// width (or a height) above it: what a definite size given to `id`
    /// can change beyond its own. Cached with `reads_available`.
    fn percentages_within(&self, id: NodeId) -> (bool, bool) {
        let bits = self.reads_bits(id);
        (bits & PERCENT_WIDTH != 0, bits & PERCENT_HEIGHT != 0)
    }
    fn reads_bits(&self, id: NodeId) -> u8 {
        let cached = self.node(id).reads.get();
        if cached & READS_KNOWN != 0 {
            return cached;
        }
        // Uncached, it recurses through the subtree (with room: `deep`).
        crate::deep(0, || self.read_bits(id))
    }
    fn read_bits(&self, id: NodeId) -> u8 {
        let node = self.node(id);
        // Percentages anywhere below, or its own.
        let (own_w, own_h) = self.percent_bases(id);
        let (mut percent_w, mut percent_h) = (own_w, own_h);
        for child in self.children_raw(node) {
            let (w, h) = self.percentages_within(*child);
            percent_w |= w;
            percent_h |= h;
        }
        let style = &node.style;
        let (width, height) = match &node.kind {
            // As its last child, which it measures as.
            _ if node.contents() && !self.children_raw(node).is_empty() => {
                self.reads_available(*self.children_raw(node).last().unwrap())
            }
            NodeKind::Text { .. } => (
                style.text_wrap || style.text_options != Default::default(),
                false,
            ),
            NodeKind::RichText { text } => (
                style.text_wrap || text.options() != Default::default(),
                false,
            ),
            NodeKind::Container(layout) | NodeKind::Panel { layout, .. } => {
                let mut children = (false, false);
                for child in self.children_raw(node) {
                    if self.node(*child).style.absolute {
                        continue;
                    }
                    let (w, h) = self.reads_available(*child);
                    children = (children.0 || w, children.1 || h);
                }
                // A row fits its shrinkable children into the room it has.
                if *layout == Layout::Row
                    && self.children_raw(node).iter().any(|child| {
                        let s = &self.node(*child).style;
                        !s.absolute && s.flex_shrink > 0.
                    })
                {
                    children.0 = true;
                }
                let (width, height) = if self.uses_advanced_layout(id) {
                    let options = style.layout_options.as_deref().copied().unwrap_or_default();
                    let wraps_column = *layout != Layout::Row && options.wrap.is_some();
                    let grid = options.display == Some(crate::layout::Display::Grid);
                    (true, children.1 || wraps_column || grid)
                } else {
                    children
                };
                (
                    width && style.width.is_none(),
                    height && style.height.is_none(),
                )
            }
            _ => (false, false),
        };
        let bits = READS_KNOWN
            | if width { READS_WIDTH } else { 0 }
            | if height { READS_HEIGHT } else { 0 }
            | if percent_w { PERCENT_WIDTH } else { 0 }
            | if percent_h { PERCENT_HEIGHT } else { 0 }
            | if own_w { OWN_PERCENT_WIDTH } else { 0 }
            | if own_h { OWN_PERCENT_HEIGHT } else { 0 };
        node.reads.set(bits);
        bits
    }
    /// `id`'s size under `constraints`, for a probe: a kept size serves,
    /// leaving whichever result `id` holds whole as it was.
    fn measure(&mut self, id: NodeId, mut constraints: Constraints) -> (f32, f32) {
        self.measured(id, &mut constraints, false)
    }
    /// `measure`, and `constraints` as `id`'s results are kept under them
    /// (`relevant`): a parent stores those, so that `arrange` finds the
    /// child's result without asking again.
    fn measure_kept(
        &mut self,
        id: NodeId,
        mut constraints: Constraints,
    ) -> ((f32, f32), Constraints) {
        let size = self.measured(id, &mut constraints, false);
        (size, constraints)
    }
    /// `id` laid out under `constraints`, the result it then holds (with its
    /// children's places) for `arrange`.
    fn measure_full(&mut self, id: NodeId, mut constraints: Constraints) -> (f32, f32) {
        self.measured(id, &mut constraints, true)
    }
    fn measured(&mut self, id: NodeId, constraints: &mut Constraints, full: bool) -> (f32, f32) {
        #[cfg(test)]
        {
            self.measure_calls += 1;
        }
        self.visit(id);
        let node = self.node(id);
        self.relevant_in(id, node, constraints);
        let pass = self.layout_pass;
        let clean = node.dirty & LAYOUT == 0;
        // The pass first: a changed node's older results never serve.
        let usable = |m: &Measurement| (clean || m.pass == pass) && m.constraints == *constraints;
        if let Some(current) = &node.measurement
            && usable(current)
        {
            return current.size;
        }
        if let Some(index) = node.alternates.iter().position(usable) {
            // Promote it: `arrange` places children from the current result.
            // Checked just above.
            let node = self.tree_node_mut(id);
            let size = promote_measurement(&mut node.measurement, &mut node.alternates, index);
            node.arrange_dirty = true;
            return size;
        }
        if !full
            && let Some(kept) = node
                .sizes
                .iter()
                .find(|m| (clean || m.pass == pass) && m.constraints == *constraints)
        {
            return kept.size;
        }
        // Measuring recurses into the children: one level of the tree more.
        self.measure_depth += 1;
        let size = crate::deep(self.measure_depth, || {
            self.measure_uncached(id, *constraints)
        });
        self.measure_depth -= 1;
        size
    }
    /// `measured`, when nothing kept serves.
    /// A contents or zoomed node goes to `measure_special`.
    fn measure_uncached(&mut self, id: NodeId, constraints: Constraints) -> (f32, f32) {
        // Checked by `measured`, its one caller.
        let node = self.tree_node(id);
        if node.special.any()
            && let Some(size) = self.measure_special(id, constraints)
        {
            return size;
        }
        if let Some(size) = self.measure_sized_box(id, constraints) {
            return size;
        }
        let node = self.tree_node(id);
        if let NodeKind::Container(layout) | NodeKind::Panel { layout, .. } = node.kind
            && self.uses_advanced_layout(id)
        {
            return self.measure_advanced(id, layout, constraints);
        }
        let node = self.tree_node_mut(id);
        if node.advanced_layout.is_some() {
            node.advanced_layout = None;
        }
        let style = self.resolved_leaf_style(id, constraints);
        let padding = style.padding_edges.unwrap_or(Insets::all(style.padding));
        // A container's layout, and the room its border takes.
        let (layout, border) = match &self.tree_node(id).kind {
            NodeKind::Panel { layout, quad } => (Some(*layout), quad.layout_border()),
            NodeKind::Container(layout) => (Some(*layout), Insets::default()),
            _ => (None, Insets::default()),
        };
        // The content box's insets: padding inside the border (border-box).
        let padding = Insets {
            top: padding.top.max(0.0) + border.top,
            right: padding.right.max(0.0) + border.right,
            bottom: padding.bottom.max(0.0) + border.bottom,
            left: padding.left.max(0.0) + border.left,
        };
        let horizontal_padding = padding.left + padding.right;
        let vertical_padding = padding.top + padding.bottom;
        let explicit_w = constraints
            .force_width
            .or(style.width)
            .or_else(|| percentage(style.width_percent, constraints.percent_width))
            .map(|w| dimension(w, style.min_width, style.max_width));
        let explicit_h = constraints
            .force_height
            .or(style.height)
            .or_else(|| percentage(style.height_percent, constraints.percent_height))
            .map(|h| dimension(h, style.min_height, style.max_height));
        let available_w = explicit_w
            .or_else(|| {
                constraints
                    .width
                    .map(|w| w.min(style.max_width.unwrap_or(f32::INFINITY)))
            })
            .or(style.max_width)
            .map(|w| (w - horizontal_padding).max(0.0));
        let available_h = explicit_h
            .or_else(|| {
                constraints
                    .height
                    .map(|h| h.min(style.max_height.unwrap_or(f32::INFINITY)))
            })
            .or(style.max_height)
            .map(|h| (h - vertical_padding).max(0.0));
        let percent_width = explicit_w.map(|w| (w - horizontal_padding).max(0.0));
        let percent_height = explicit_h.map(|h| (h - vertical_padding).max(0.0));
        // A changed node's result from an earlier pass is dropped as this
        // one is stored: its buffers hold this one's placements.
        let pass = self.layout_pass;
        let node = self.tree_node_mut(id);
        let stale = node.dirty & LAYOUT != 0;
        let mut children = match &mut node.measurement {
            Some(last) if layout.is_some() && stale && last.pass != pass => {
                std::mem::take(&mut last.children)
            }
            _ => Vec::new(),
        };
        children.clear();
        let (mut width, mut height) = match layout {
            None => self.leaf_size(id, &style, available_w),
            // An empty box (a bar, a swatch, a spacer): its padding and size,
            // as the flex pass below works them out with nothing in it.
            Some(layout) if self.children_raw(self.tree_node(id)).is_empty() => {
                let gaps = 0.0 + style.gap * 0.0;
                match layout {
                    Layout::Row => (gaps, 0.0),
                    Layout::Column => (0.0, gaps),
                    Layout::Overlay => (0.0, 0.0),
                }
            }
            Some(layout) => {
                // One set per level of the recursion, back in the pool as
                // this measure ends.
                let Scratch {
                    mut ids,
                    mut absolute,
                    mut asked,
                    mut kept,
                    mut sizes,
                    mut allocated,
                    mut bases,
                    mut frozen,
                    mut floors,
                    mut weights,
                    mut margins,
                } = self.scratch.pop().unwrap_or_default();
                ids.clear();
                absolute.clear();
                for &child in self.children_raw(self.tree_node(id)) {
                    if self.tree_node(child).style.absolute {
                        absolute.push(child);
                    } else {
                        ids.push(child);
                    }
                }
                // Those whose containing block is further up are placed by it.
                if self.relative_count != 0 {
                    absolute.retain(|child| !self.escapes(*child));
                }
                let horizontal = layout == Layout::Row;
                // The axes a child fills (stretch), unless it sizes itself
                // there: a row's or column's cross axis by `align`; an
                // overlay's width by `align` and its height by `justify`, as
                // CSS grid's `justify-items` and `align-items` stretch an item
                // in its area.
                // A child's own alignment across (`align_self`), else this
                // node's (in an overlay, across is its width).
                let aligned = |s: &Style| {
                    s.layout_options
                        .as_deref()
                        .and_then(|options| options.align_self)
                        .unwrap_or(style.align)
                };
                let stretches = |s: &Style| {
                    let stretch = aligned(s) == Align::Stretch;
                    let (fill_w, fill_h) = match layout {
                        Layout::Row => (false, stretch),
                        Layout::Column => (stretch, false),
                        Layout::Overlay => (stretch, style.justify == Justify::Stretch),
                    };
                    (
                        fill_w
                            && (s.width.is_none() || s.intrinsic_width)
                            && percentage(s.width_percent, percent_width).is_none(),
                        fill_h
                            && (s.height.is_none() || s.intrinsic_height)
                            && percentage(s.height_percent, percent_height).is_none(),
                    )
                };
                // Where this container's own size fixes a stretched axis, its
                // children are sized at the stretched size from the start, as
                // CSS sizes a flex item's content at its stretched cross size:
                // wrapping text then takes the lines it will show, and its
                // height is the one flex distributes.
                let filled = |scene: &Self, child: NodeId| {
                    let s = &scene.node(child).style;
                    let (w, h) = stretches(s);
                    let m = s.margin;
                    (
                        explicit_w
                            .filter(|_| w)
                            .map(|outer| (outer - horizontal_padding - m.left - m.right).max(0.)),
                        explicit_h
                            .filter(|_| h)
                            .map(|outer| (outer - vertical_padding - m.top - m.bottom).max(0.)),
                    )
                };
                let natural = Constraints {
                    width: available_w,
                    height: available_h,
                    percent_width,
                    percent_height,
                    ..Constraints::default()
                };
                // What each child's size was last asked under: its place is
                // that result's (see `store_measurement`).
                asked.clear();
                asked.extend(ids.iter().map(|child| {
                    let (width, height) = filled(self, *child);
                    Constraints {
                        force_width: width,
                        force_height: height,
                        ..natural
                    }
                }));
                sizes.clear();
                kept.clear();
                for i in 0..ids.len() {
                    let (size, under) = self.measure_kept(ids[i], asked[i]);
                    sizes.push(size);
                    kept.push(under);
                }
                // Children whose main size flex distribution decided.
                allocated.clear();
                allocated.resize(ids.len(), false);
                let main_margin = |s: &Style| {
                    if horizontal {
                        s.margin.left + s.margin.right
                    } else {
                        s.margin.top + s.margin.bottom
                    }
                };
                let main = |size: (f32, f32)| if horizontal { size.0 } else { size.1 };
                // `asked` with the main size set to `size`, the cross kept.
                let along = |asked: Constraints, size: f32| Constraints {
                    force_width: if horizontal {
                        Some(size)
                    } else {
                        asked.force_width
                    },
                    force_height: if horizontal {
                        asked.force_height
                    } else {
                        Some(size)
                    },
                    ..asked
                };
                let gaps = style.gap * ids.len().saturating_sub(1) as f32;
                let natural_main = sizes
                    .iter()
                    .enumerate()
                    .map(|(i, size)| main(*size) + main_margin(&self.tree_node(ids[i]).style))
                    .sum::<f32>()
                    + gaps;
                let main_limit = if horizontal {
                    // A row sized by its content, given less room than that,
                    // fits it (CSS fit-content): shrinkable children shrink,
                    // and what cannot shrink still overflows.
                    let content = explicit_w.unwrap_or(match available_w {
                        Some(room) => natural_main.min(room) + horizontal_padding,
                        None => natural_main + horizontal_padding,
                    });
                    Some(dimension(content, style.min_width, style.max_width) - horizontal_padding)
                } else {
                    Some(
                        dimension(
                            explicit_h.unwrap_or(natural_main + vertical_padding),
                            style.min_height,
                            style.max_height,
                        ) - vertical_padding,
                    )
                };
                if layout != Layout::Overlay
                    && let Some(limit) = main_limit
                {
                    bases.clear();
                    bases.extend(sizes.iter().copied().map(main));
                    frozen.clear();
                    frozen.resize(ids.len(), false);
                    // CSS's automatic minimum (`min-width: auto`): a child
                    // shrinks no further than its min-content size, unless it
                    // clips (a scroll container's is 0) or sets its own.
                    floors.clear();
                    floors.resize(ids.len(), None);
                    for _ in 0..=ids.len() {
                        let used: f32 = sizes
                            .iter()
                            .enumerate()
                            .map(|(i, size)| {
                                main(*size) + main_margin(&self.tree_node(ids[i]).style)
                            })
                            .sum::<f32>()
                            + gaps;
                        let remaining = limit - used;
                        if remaining.abs() < 0.001 {
                            break;
                        }
                        weights.clear();
                        weights.extend(ids.iter().enumerate().map(|(i, child)| {
                            let s = &self.node(*child).style;
                            if frozen[i] {
                                0.0
                            } else if remaining > 0.0 {
                                s.flex_grow.max(0.0)
                            } else {
                                s.flex_shrink.max(0.0) * bases[i]
                            }
                        }));
                        let weight: f32 = weights.iter().sum();
                        if weight <= 0.0 {
                            break;
                        }
                        let mut clamped = false;
                        for (i, child) in ids.iter().enumerate() {
                            if weights[i] == 0.0 {
                                continue;
                            }
                            let wanted = flexed(main(sizes[i]) + remaining * weights[i] / weight);
                            let (own_min, set_elsewhere) = {
                                let s = &self.node(*child).style;
                                let options = s.layout_options.as_deref();
                                if horizontal {
                                    (s.min_width, options.is_some_and(|o| o.min_width.is_some()))
                                } else {
                                    (
                                        s.min_height,
                                        options.is_some_and(|o| o.min_height.is_some()),
                                    )
                                }
                            };
                            // A minimum the child sets (in points or as a
                            // percentage) is its own; only an unset one is
                            // automatic.
                            let min = if remaining < 0.0 && own_min.is_none() && !set_elsewhere {
                                let floor = match floors[i] {
                                    Some(floor) => floor,
                                    None => {
                                        let floor = self.automatic_minimum(
                                            *child, asked[i], horizontal, bases[i],
                                        );
                                        floors[i] = Some(floor);
                                        floor
                                    }
                                };
                                Some(floor)
                            } else {
                                own_min
                            };
                            let s = &self.node(*child).style;
                            let size = if horizontal {
                                dimension(wanted, min, s.max_width)
                            } else {
                                dimension(wanted, min, s.max_height)
                            };
                            if (size - wanted).abs() > 0.001 {
                                frozen[i] = true;
                                clamped = true;
                            }
                            // Held where it is (a column's text at its
                            // content's height): as laid out, not again.
                            if !allocated[i] && (size - main(sizes[i])).abs() < 0.001 {
                                continue;
                            }
                            asked[i] = along(asked[i], size);
                            allocated[i] = true;
                            (sizes[i], kept[i]) = self.measure_kept(*child, asked[i]);
                        }
                        if !clamped {
                            break;
                        }
                    }
                }
                // A flex item receives a definite main allocation even when
                // its natural size already consumes exactly the available space.
                // Its percentage descendants must see that allocation as a basis.
                if layout != Layout::Overlay
                    && if horizontal {
                        explicit_w.is_some()
                    } else {
                        explicit_h.is_some()
                    }
                {
                    for (i, child) in ids.iter().enumerate() {
                        let child_style = &self.node(*child).style;
                        if child_style.flex_grow > 0.0 || child_style.flex_shrink > 0.0 {
                            // At its natural size with nothing in it sized by
                            // a percentage along the axis, the allocation
                            // changes nothing: it is not laid out again.
                            let (percent_w, percent_h) = self.percentages_within(*child);
                            let percent = if horizontal { percent_w } else { percent_h };
                            if !allocated[i] && !percent {
                                continue;
                            }
                            asked[i] = along(asked[i], main(sizes[i]));
                            allocated[i] = true;
                            (sizes[i], kept[i]) = self.measure_kept(*child, asked[i]);
                        }
                    }
                }
                // The children's extent: across rows (or columns) and gaps,
                // and the widest (or tallest), with their margins.
                margins.clear();
                margins.extend(ids.iter().map(|c| self.node(*c).style.margin));
                let extent = |sizes: &[(f32, f32)]| {
                    let (mut natural_w, mut natural_h) = (0.0_f32, 0.0_f32);
                    for (i, m) in margins.iter().enumerate() {
                        let (w, h) = (sizes[i].0 + m.left + m.right, sizes[i].1 + m.top + m.bottom);
                        match layout {
                            Layout::Row => {
                                natural_w += w;
                                natural_h = natural_h.max(h);
                            }
                            Layout::Column => {
                                natural_h += h;
                                natural_w = natural_w.max(w);
                            }
                            Layout::Overlay => {
                                natural_w = natural_w.max(w);
                                natural_h = natural_h.max(h);
                            }
                        }
                    }
                    match layout {
                        Layout::Row => (natural_w + gaps, natural_h),
                        Layout::Column => (natural_w, natural_h + gaps),
                        Layout::Overlay => (natural_w, natural_h),
                    }
                };
                let inner = |(natural_w, natural_h): (f32, f32)| {
                    (
                        dimension(
                            explicit_w.unwrap_or(natural_w + horizontal_padding),
                            style.min_width,
                            style.max_width,
                        ) - horizontal_padding,
                        dimension(
                            explicit_h.unwrap_or(natural_h + vertical_padding),
                            style.min_height,
                            style.max_height,
                        ) - vertical_padding,
                    )
                };
                // Stretched along an axis the container sizes to its widest
                // (or tallest) child: there, now that it is known. The main
                // size stays forced only where flex decided it; otherwise the
                // child takes its natural main size at the stretched size
                // (wrapped text grows taller), and the extent follows.
                let (inner_w, inner_h) = inner(extent(&sizes));
                for (i, child) in ids.iter().enumerate() {
                    let s = &self.node(*child).style;
                    let (w, h) = stretches(s);
                    let m = s.margin;
                    let stretch_w = w && asked[i].force_width.is_none();
                    let stretch_h = h && asked[i].force_height.is_none();
                    if !stretch_w && !stretch_h {
                        continue;
                    }
                    asked[i] = Constraints {
                        force_width: match stretch_w {
                            true => Some((inner_w - m.left - m.right).max(0.0)),
                            false => asked[i].force_width,
                        },
                        force_height: match stretch_h {
                            true => Some((inner_h - m.top - m.bottom).max(0.0)),
                            false => asked[i].force_height,
                        },
                        ..asked[i]
                    };
                    if horizontal && allocated[i] {
                        asked[i].force_width = Some(sizes[i].0);
                    }
                    if !horizontal && layout != Layout::Overlay && allocated[i] {
                        asked[i].force_height = Some(sizes[i].1);
                    }
                    (sizes[i], kept[i]) = self.measure_kept(*child, asked[i]);
                }
                let (natural_w, natural_h) = extent(&sizes);
                let (inner_w, inner_h) = inner((natural_w, natural_h));
                let extra = if horizontal {
                    inner_w - natural_w
                } else {
                    inner_h - natural_h
                };
                let (mut cursor, extra_gap) = match style.justify {
                    Justify::Start => (0.0, 0.0),
                    Justify::Center => (extra / 2.0, 0.0),
                    Justify::End => (extra, 0.0),
                    Justify::SpaceBetween if ids.len() > 1 => {
                        (0.0, extra.max(0.0) / (ids.len() - 1) as f32)
                    }
                    Justify::SpaceAround if !ids.is_empty() => (
                        extra.max(0.0) / ids.len() as f32 / 2.0,
                        extra.max(0.0) / ids.len() as f32,
                    ),
                    Justify::SpaceEvenly => {
                        let gap = extra.max(0.0) / (ids.len() + 1) as f32;
                        (gap, gap)
                    }
                    _ => (0.0, 0.0),
                };
                for (i, child) in ids.iter().enumerate() {
                    let s = &self.node(*child).style;
                    let m = s.margin;
                    let (w, h) = sizes[i];
                    let cross_free = if horizontal {
                        inner_h - h - m.top - m.bottom
                    } else {
                        inner_w - w - m.left - m.right
                    };
                    let cross = match aligned(s) {
                        Align::Center => cross_free / 2.0,
                        Align::End => cross_free,
                        _ => 0.0,
                    };
                    let (cx, cy) = match layout {
                        Layout::Row => (cursor + m.left, cross + m.top),
                        Layout::Column => (cross + m.left, cursor + m.top),
                        Layout::Overlay => {
                            // Each child aligned in the whole box on its own.
                            let free = inner_h - h - m.top - m.bottom;
                            let down = match style.justify {
                                Justify::Center | Justify::SpaceAround | Justify::SpaceEvenly => {
                                    free / 2.0
                                }
                                Justify::End => free,
                                _ => 0.0,
                            };
                            (cross + m.left, down + m.top)
                        }
                    };
                    children.push((
                        *child,
                        Rect::new(cx + padding.left, cy + padding.top, w, h),
                        kept[i],
                    ));
                    cursor += main(sizes[i]) + main_margin(s) + style.gap + extra_gap;
                }
                for &child in &absolute {
                    let margin = self.tree_node(child).style.margin;
                    let placed = Constraints {
                        width: Some((inner_w - margin.left - margin.right).max(0.0)),
                        height: Some((inner_h - margin.top - margin.bottom).max(0.0)),
                        percent_width: Some(inner_w.max(0.0)),
                        percent_height: Some(inner_h.max(0.0)),
                        ..Constraints::default()
                    };
                    let ((w, h), placed) = self.measure_kept(child, placed);
                    children.push((
                        child,
                        Rect::new(padding.left + margin.left, padding.top + margin.top, w, h),
                        placed,
                    ));
                }
                self.scratch.push(Scratch {
                    ids,
                    absolute,
                    asked,
                    kept,
                    sizes,
                    allocated,
                    bases,
                    frozen,
                    floors,
                    weights,
                    margins,
                });
                (natural_w, natural_h)
            }
        };
        width = dimension(
            explicit_w.unwrap_or(width + horizontal_padding),
            style.min_width,
            style.max_width,
        );
        height = dimension(
            explicit_h.unwrap_or(height + vertical_padding),
            style.min_height,
            style.max_height,
        );
        self.store_measurement(id, constraints, (width, height), children);
        (width, height)
    }
    /// An empty box whose size its style or constraints fix (a bar, a
    /// swatch), as the rest of `measure_uncached` works it out: its width and
    /// height, with no padding, children or advanced layout to weigh, and
    /// its style read in place rather than copied. `None` for any other node.
    fn measure_sized_box(&mut self, id: NodeId, constraints: Constraints) -> Option<(f32, f32)> {
        // Checked by `measured`, through `measure_uncached`.
        let node = self.tree_node(id);
        let style = &node.style;
        if !matches!(node.kind, NodeKind::Container(_) | NodeKind::Panel { .. })
            || !self.children_raw(node).is_empty()
            || node.contents()
            || style.layout_options.is_some()
            || style.align == Align::Baseline
        {
            return None;
        }
        let explicit_w = constraints
            .force_width
            .or(style.width)
            .or_else(|| percentage(style.width_percent, constraints.percent_width))?;
        let explicit_h = constraints
            .force_height
            .or(style.height)
            .or_else(|| percentage(style.height_percent, constraints.percent_height))?;
        // Each clamped to its limits once: a second clamp would not move it.
        let size = (
            dimension(explicit_w, style.min_width, style.max_width),
            dimension(explicit_h, style.min_height, style.max_height),
        );
        // As `measure_uncached` leaves it: no advanced layout, and the last
        // result's lists, when this pass replaces it, kept for this one.
        let pass = self.layout_pass;
        // Most boxes never had an advanced result: one compare, nothing
        // dropped.
        let node = self.tree_node_mut(id);
        if node.advanced_layout.is_some() {
            node.advanced_layout = None;
        }
        let stale = node.dirty & LAYOUT != 0;
        // Its last result replaced and nothing else kept (a bar changing
        // height): `store_measurement`'s result, written over that one.
        if stale
            && node.alternates.is_empty()
            && node.sizes.is_empty()
            && let Some(last) = &mut node.measurement
            && last.pass != pass
        {
            last.constraints = constraints;
            last.size = size;
            last.pass = pass;
            last.children.clear();
            node.arrange_dirty = true;
            return Some(size);
        }
        let mut children = match &mut node.measurement {
            Some(last) if stale && last.pass != pass => std::mem::take(&mut last.children),
            _ => Vec::new(),
        };
        children.clear();
        self.store_measurement(id, constraints, size, children);
        Some(size)
    }
    /// A leaf's own size: its text's at `available_w` (when it wraps), its
    /// image's pixels, or none. Reads the kind in place: cloning it took an
    /// atomic count on every measure of text, images and decorated paint.
    fn leaf_size(&self, id: NodeId, style: &Style, available_w: Option<f32>) -> (f32, f32) {
        match &self.node(id).kind {
            NodeKind::Text {
                text, font_size, ..
            } => {
                let limit = (style.text_wrap || style.text_options != Default::default())
                    .then_some(available_w)
                    .flatten();
                let revision = self.text_shaper_revision;
                if let Some(size) = self.node(id).text_size.get().find(limit, revision) {
                    size
                } else {
                    let size = if limit == Some(0.) && style.text_options == Default::default() {
                        // No room at all is a min-content probe: lines break
                        // at every opportunity, as CSS's min-content does,
                        // and never inside a word.
                        let font = &self.node(id).font;
                        match (&self.font_min_content, &self.font_measurer) {
                            (Some(measure), _) => measure(text, *font_size, font),
                            (None, Some(measure)) => {
                                measure(&min_content_lines(text).into(), *font_size, None, font)
                            }
                            (None, None) => {
                                let lines = min_content_lines(text);
                                self.measure_text_with_font(&lines, *font_size, None, font)
                            }
                        }
                    } else if style.text_options != Default::default() {
                        let runs = if text.is_empty() {
                            Vec::new()
                        } else {
                            vec![crate::rich_text::TextRun {
                                range: 0..text.len(),
                                font: self.node(id).font.clone(),
                                font_size: *font_size,
                                ..Default::default()
                            }]
                        };
                        let rich = crate::rich_text::RichText::new(text.clone(), runs)
                            .unwrap()
                            .with_options(style.text_options);
                        self.rich_measurer.measure_mounted(&rich, limit)
                    } else {
                        let font = &self.node(id).font;
                        match &self.font_measurer {
                            Some(measure) => measure(text, *font_size, limit, font),
                            None => self.measure_text_with_font(text, *font_size, limit, font),
                        }
                    };
                    TextSizes::remember(&self.node(id).text_size, limit, revision, size);
                    size
                }
            }
            NodeKind::RichText { text } => {
                let limit = (style.text_wrap || text.options() != Default::default())
                    .then_some(available_w)
                    .flatten();
                // Same cache as plain text: colour-only changes keep layout
                // (see `set_kind`), so they keep this size too.
                let revision = self.text_shaper_revision;
                if let Some(size) = self.node(id).text_size.get().find(limit, revision) {
                    size
                } else {
                    let size = if limit == Some(0.) && text.options() == Default::default() {
                        // A min-content probe, as plain text's: lines
                        // break at every opportunity, never inside a word.
                        match &self.rich_min_content {
                            Some(measure) => measure(text),
                            None => self
                                .rich_measurer
                                .measure_mounted(&text.min_content_lines(), None),
                        }
                    } else {
                        self.rich_measurer.measure_mounted(text, limit)
                    };
                    TextSizes::remember(&self.node(id).text_size, limit, revision, size);
                    size
                }
            }
            NodeKind::Rect(_) | NodeKind::Quad(_) | NodeKind::Canvas(_) => (0.0, 0.0),
            NodeKind::Svg(_) => (24., 24.),
            NodeKind::Image(image) => (image.width() as f32, image.height() as f32),
            #[cfg(target_os = "macos")]
            NodeKind::NativeSurface(frame) => (frame.width() as f32, frame.height() as f32),
            NodeKind::Container(_) | NodeKind::Panel { .. } => {
                unreachable!("containers lay their children out")
            }
        }
    }
    /// A contents node (`set_contents`) under `constraints`: its last child
    /// under the same ones (its style is that child's place), filling it;
    /// the children before it over it, within its box.
    fn measure_contents(&mut self, id: NodeId, constraints: Constraints) -> (f32, f32) {
        self.node_mut(id).advanced_layout = None;
        let ids = self.children_raw(self.node(id)).clone();
        let lead = *ids.last().unwrap();
        let size = self.measure(lead, constraints);
        let mut children = Vec::with_capacity(ids.len());
        for child in ids {
            if child == lead {
                children.push((child, Rect::new(0., 0., size.0, size.1), constraints));
                continue;
            }
            let margin = self.node(child).style.margin;
            let within = Constraints {
                width: Some((size.0 - margin.left - margin.right).max(0.)),
                height: Some((size.1 - margin.top - margin.bottom).max(0.)),
                percent_width: Some(size.0),
                percent_height: Some(size.1),
                ..Constraints::default()
            };
            let (w, h) = self.measure(child, within);
            children.push((child, Rect::new(margin.left, margin.top, w, h), within));
        }
        self.store_measurement(id, constraints, size, children);
        size
    }
    /// `measure_uncached` for a contents node (as its last child) or a
    /// zoomed one: laid out in its own points, under its parent's room
    /// divided by its zoom, the room it takes times it. A zoomed node's
    /// result is kept as its parent asks for it, in its parent's points, so
    /// that `measured` serves it as any other; `arrange` takes its box in its
    /// own from `snap` (it lays out as `snap`, `restyle_special`). `None`:
    /// measured as any node.
    #[cold]
    #[inline(never)]
    fn measure_special(&mut self, id: NodeId, constraints: Constraints) -> Option<(f32, f32)> {
        let node = self.node(id);
        let contents = node.contents() && !self.children_raw(node).is_empty();
        if !node.special.has(ZOOMED) {
            return contents.then(|| self.measure_contents(id, constraints));
        }
        let zoom = node.zoom();
        // Laid out since its parent last measured it under this room: its
        // result in its own points is the one to serve again, unless the
        // node changed since (a result from before is stale, as
        // `store_measurement` drops one).
        if node.local.as_deref().and_then(|local| local.placed) == Some(constraints)
            && let Some(measurement) = &node.measurement
            && measurement.constraints.width.is_some_and(f32::is_nan)
            && (node.dirty & LAYOUT == 0 || measurement.pass == self.layout_pass)
        {
            let size = measurement.size;
            let outer = (size.0 * zoom, size.1 * zoom);
            let node = self.node_mut(id);
            let measurement = node.measurement.as_mut().unwrap();
            measurement.constraints = constraints;
            measurement.size = outer;
            return Some(outer);
        }
        let mut inner = constraints.scaled(zoom.recip());
        // In its own points, as an unzoomed node, through `measured` (its
        // one copy of `measure_uncached` stays the hot one): its zoom and
        // the results kept in its parent's points set aside meanwhile, so
        // none serves this (nothing in measuring reads its zoom).
        let node = self.node_mut(id);
        node.special = node.special.with(ZOOMED, false);
        let alternates = std::mem::take(&mut node.alternates);
        let sizes = std::mem::take(&mut node.sizes);
        node.measurement = None;
        let size = self.measured(id, &mut inner, true);
        let outer = (size.0 * zoom, size.1 * zoom);
        let pass = self.layout_pass;
        let node = self.node_mut(id);
        node.special = node.special.with(ZOOMED, true);
        node.alternates = alternates;
        node.sizes = sizes;
        // A changed node's results from before are stale, as
        // `store_measurement` drops them (set aside, it did not see them):
        // from before a zoom, they are in other points.
        if node.dirty & LAYOUT != 0 {
            node.alternates.retain(|m| m.pass == pass);
            node.sizes.retain(|m| m.pass == pass);
        }
        if let Some(measurement) = &mut node.measurement {
            measurement.constraints = constraints;
            measurement.size = outer;
        }
        Some(outer)
    }
    /// Make a new result current, keeping earlier ones that are still valid.
    fn store_measurement(
        &mut self,
        id: NodeId,
        constraints: Constraints,
        size: (f32, f32),
        children: Vec<ChildPlacement>,
    ) {
        // `constraints` are as `measure` looks results up (`relevant`).
        let pass = self.layout_pass;
        let node = self.node_mut(id);
        let dirty = node.dirty & LAYOUT != 0;
        // A changed node's result from an earlier pass is stale (the retain
        // below drops it): not shifted in only to go.
        if let Some(previous) = node.measurement.take()
            && (previous.pass == pass || !dirty)
        {
            node.alternates.insert(0, previous);
        }
        // Most nodes keep no other results (a box changing alone): nothing
        // to sort out.
        if !node.alternates.is_empty() || !node.sizes.is_empty() {
            // Results from before a node changed are stale; of the rest, all of
            // this pass's and the most recent few of earlier ones.
            // Earlier results past the few kept whole keep their sizes.
            let mut earlier = 0;
            let mut evicted = Vec::new();
            node.alternates.retain(|m| {
                let valid = m.constraints != constraints && (m.pass == pass || !dirty);
                let whole = m.pass == pass || earlier < MEASUREMENT_ALTERNATES;
                earlier += usize::from(valid && whole && m.pass != pass);
                if valid && !whole {
                    evicted.push(ProbeSize::from(m));
                }
                valid && whole
            });
            evicted.extend(
                node.alternates
                    .drain(PASS_RESULTS.min(node.alternates.len())..)
                    .map(|m| ProbeSize::from(&m)),
            );
            node.sizes
                .retain(|m| m.constraints != constraints && (m.pass == pass || !dirty));
            keep_sizes(&mut node.sizes, evicted);
        }
        let overfull = node.alternates.len() == MEASUREMENT_ALTERNATES + 1;
        node.arrange_dirty = true;
        node.measurement = Some(Measurement {
            constraints,
            size,
            children,
            pass,
        });
        if overfull {
            self.overfull.push(id);
        }
    }
    /// `covered`: an ancestor moved and already damaged this whole subtree at
    /// its old and new positions.
    /// `level`: how deep in the walk (`deep`).
    /// `context`: where it is damaged when it moves, shared with its
    /// siblings (`damage_subtree_with`).
    #[allow(clippy::too_many_arguments)]
    fn arrange<const CELLS: bool>(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        report: &mut FrameReport,
        covered: bool,
        level: u32,
        context: &mut Option<DamageContext>,
    ) {
        self.visit(id);
        // Checked by the caller: `arrange_from`, or the loop below for a
        // child.
        let (x, y) = match self.tree_node(id).style.snap {
            true => self.snap(id, x, y),
            false => (x, y),
        };
        // What it holds, read at once: laying it out leaves these as they are.
        // Checked just above.
        let node = self.tree_node(id);
        let cache = node.measurement.as_ref().unwrap();
        let bounds = Rect::new(x, y, cache.size.0, cache.size.1);
        let (arrange_dirty, rtl, childless) = (
            node.arrange_dirty,
            node.rtl,
            self.children_raw(node).is_empty(),
        );
        if !arrange_dirty && node.bounds == bounds {
            return;
        }
        let count = cache.children.len();
        let changed = node.bounds != bounds;
        if changed || arrange_dirty {
            report.layout_nodes += 1;
        }
        let damage = changed && !covered;
        // A leaf, nothing transformed or filtered: what it painted is damaged
        // with what it will paint, once it is placed (see `leaf_damage`).
        let leaf = damage && childless && self.transformed + self.filtered.get() == 0;
        // A moved leaf's damage where it was and where it lands, worked out
        // and added before its box changes: a leaf has no children, so no
        // other damage comes between, and nothing it paints changes.
        match (damage, leaf) {
            (true, true) => {
                let was = self.tree_node(id).bounds;
                let (before, after) = self.leaf_damage::<CELLS>(id, context, was, bounds);
                match (CELLS, &mut self.damage_grid) {
                    (false, _) => self.add_moved_leaf_damage(before, after),
                    (true, Some(grid)) => grid.mark_clipped_pair(before, after),
                    // The grid stays for the walk (`arrange_from`): never
                    // reached, but damage as `add_damage` takes it. Not
                    // `add_moved_leaf_damage`, which a second caller would
                    // take out of `arrange::<false>`.
                    (true, None) => {
                        for rect in [before, after].into_iter().flatten() {
                            self.add_damage(rect);
                        }
                    }
                }
            }
            (true, false) => self.damage_subtree_with(id, context),
            _ => {}
        }
        if changed {
            self.tree_node_mut(id).bounds = bounds;
            self.invalidate(id, PAINTED);
        }
        // Where the children are damaged, worked out when the first moves.
        let mut children = None;
        // Read in place, not copied out: laying a child out leaves this
        // node's result as it is.
        for at in 0..count {
            let cache = self.tree_node(id).measurement.as_ref().unwrap();
            let (child, mut rect, constraints) = cache.children[at];
            // Right to left: laid out left to right, mirrored in its box.
            if rtl {
                rect.x = bounds.width - rect.x - rect.width;
            }
            // Cached: promotes the matching result; evicted: re-measures.
            // Kept as its results are (`measure_kept`): its current one is
            // found here without asking.
            // In the scene, as each child a measurement holds is: removing
            // one lays its parent out again before it is arranged.
            let node = self.tree_node(child);
            let current = node.measurement.as_ref().is_some_and(|m| {
                m.constraints == constraints
                    && (node.dirty & LAYOUT == 0 || m.pass == self.layout_pass)
            });
            if !current {
                self.measure_full(child, constraints);
            }
            // A leaf in the grid's walk is placed here, without a call of
            // its own; in list mode's, a second copy of `arrange`'s work
            // would outline what it inlines (`add_moved_leaf_damage`).
            if CELLS && self.children_raw(self.tree_node(child)).is_empty() {
                let (x, y) = (x + rect.x, y + rect.y);
                self.place_leaf(child, x, y, report, covered || changed, &mut children);
                continue;
            }
            crate::deep(level + 1, || {
                let covered = covered || changed;
                let (x, y) = (x + rect.x, y + rect.y);
                self.arrange::<CELLS>(child, x, y, report, covered, level + 1, &mut children)
            });
        }
        if damage && !leaf {
            self.damage_subtree_with(id, context);
        }
        let node = self.tree_node_mut(id);
        node.dirty &= !LAYOUT;
        node.arrange_dirty = false;
    }
    /// `arrange::<true>` for a leaf: placed, its damage added, with no
    /// children to walk. The same work in the same order.
    #[inline(always)]
    fn place_leaf(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        report: &mut FrameReport,
        covered: bool,
        context: &mut Option<DamageContext>,
    ) {
        self.visit(id);
        let (x, y) = match self.tree_node(id).style.snap {
            true => self.snap(id, x, y),
            false => (x, y),
        };
        let node = self.tree_node(id);
        let cache = node.measurement.as_ref().unwrap();
        // A leaf's result places no children: measured as it is now.
        debug_assert!(cache.children.is_empty());
        let bounds = Rect::new(x, y, cache.size.0, cache.size.1);
        if !node.arrange_dirty && node.bounds == bounds {
            return;
        }
        let changed = node.bounds != bounds;
        report.layout_nodes += 1;
        let damage = changed && !covered;
        let leaf = damage && self.transformed + self.filtered.get() == 0;
        match (damage, leaf) {
            (true, true) => {
                let was = self.tree_node(id).bounds;
                let (before, after) = self.leaf_damage::<true>(id, context, was, bounds);
                match &mut self.damage_grid {
                    Some(grid) => grid.mark_clipped_pair(before, after),
                    None => {
                        for rect in [before, after].into_iter().flatten() {
                            self.add_damage(rect);
                        }
                    }
                }
            }
            (true, false) => self.damage_subtree_with(id, context),
            _ => {}
        }
        if changed {
            self.tree_node_mut(id).bounds = bounds;
            self.invalidate_placed(id);
        }
        if damage && !leaf {
            self.damage_subtree_with(id, context);
        }
        let node = self.tree_node_mut(id);
        node.dirty &= !LAYOUT;
        node.arrange_dirty = false;
    }
    /// A `snap` node placed at `x`, `y`: its edges on whole points, laid
    /// out again at the size between them if that differs.
    fn snap(&mut self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        if self.node(id).special.has(ZOOMED) {
            return self.snap_zoomed(id, x, y);
        }
        self.snap_edges(id, x, y)
    }
    /// `snap` for a zoomed node, which lays out as `snap`
    /// (`restyle_special`): on whole points only if its style asks, and its
    /// result, kept in its parent's points (`measure_special`), made its
    /// own for `arrange` to take its box and its children's places from.
    /// Its parent's next measure finds the result in its own points and
    /// measures again, serving that from it.
    #[cold]
    #[inline(never)]
    fn snap_zoomed(&mut self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        // A contents node's given style is its own.
        let asked = match self.node(id).contents() {
            true => self.contents_styles.get(&id).is_some_and(|own| own.snap),
            false => self.given_styles.get(&id).is_some_and(|given| given.snap),
        };
        let zoom = self.node(id).zoom();
        // Arranged again since, not measured (its parent placed it again,
        // its size unchanged): its result is in its own points already.
        let node = self.node_mut(id);
        let measurement = node.measurement.as_mut().unwrap();
        if measurement.constraints.width.is_some_and(f32::is_nan) {
            if !asked {
                return (x, y);
            }
            // Its edges snap where it is now, from its size in its parent's
            // points.
            if let Some(placed) = node.local.as_deref().and_then(|local| local.placed) {
                measurement.constraints = placed;
                measurement.size = (measurement.size.0 * zoom, measurement.size.1 * zoom);
            }
        }
        let (x, y) = match asked {
            true => self.snap_edges(id, x, y),
            false => (x, y),
        };
        let node = self.node_mut(id);
        let measurement = node.measurement.as_mut().unwrap();
        let parents = std::mem::replace(&mut measurement.constraints, UNMATCHED);
        measurement.size = (measurement.size.0 / zoom, measurement.size.1 / zoom);
        if let Some(local) = &mut node.local {
            local.placed = Some(parents);
        }
        (x, y)
    }
    /// `snap`'s edges on whole points, laid out again at the size between
    /// them if that differs.
    fn snap_edges(&mut self, id: NodeId, x: f32, y: f32) -> (f32, f32) {
        let measurement = self.node(id).measurement.as_ref().unwrap();
        let (w, h) = measurement.size;
        let (x0, y0) = (x.round(), y.round());
        let size = (
            ((x + w).round() - x0).max(0.),
            ((y + h).round() - y0).max(0.),
        );
        if size != (w, h) {
            let constraints = Constraints {
                force_width: Some(size.0),
                force_height: Some(size.1),
                ..measurement.constraints
            };
            self.measure_full(id, constraints);
        }
        (x0, y0)
    }
    /// Lay out and place the absolute nodes their parents left out
    /// (`containing_block`), now that their blocks have their places,
    /// outermost first: against the block's padding box, as CSS places an
    /// absolutely positioned box. Insets resolve as taffy resolves them for
    /// a parent's own absolute children; without any on an axis it sits at
    /// its parent's content origin. Only scenes with `relative` nodes get
    /// here, and only their escaping nodes are visited.
    fn place_escaped(&mut self, report: &mut FrameReport) {
        use crate::layout::Length;
        let mut placed: Vec<NodeId> = Vec::new();
        loop {
            // Placing one lays its subtree out, which can find more.
            let mut batch = std::mem::take(&mut self.escaped);
            batch.retain(|id| {
                self.contains(*id) && !placed.contains(id) && self.containing_block(*id).is_some()
            });
            if batch.is_empty() {
                break;
            }
            batch.sort_by_key(|id| (self.depth(*id), id.index, id.generation));
            batch.dedup();
            for id in batch {
                placed.push(id);
                let Some(block) = self.containing_block(id) else {
                    continue;
                };
                let parent = self.node(id).parent.expect("absolute nodes have parents");
                // Under a node laid out hidden, it was placed hidden with it.
                let mut current = Some(parent);
                let mut hidden = false;
                while let Some(ancestor) = current {
                    let options = self.node(ancestor).style.layout_options.as_deref();
                    if options.is_some_and(|o| o.display == Some(crate::layout::Display::None)) {
                        hidden = true;
                        break;
                    }
                    current = (ancestor != block)
                        .then(|| self.node(ancestor).parent)
                        .flatten();
                }
                if hidden {
                    continue;
                }
                let area = self.node(block).bounds;
                let border = self.layout_border(block);
                let (origin_x, origin_y) = (area.x + border.left, area.y + border.top);
                let width = (area.width - border.left - border.right).max(0.);
                let height = (area.height - border.top - border.bottom).max(0.);
                let style = &self.node(id).style;
                let options = style.layout_options.as_deref().copied().unwrap_or_default();
                let finite = |v: f32| if v.is_finite() { v } else { 0. };
                let inset = |value: Option<Length>, basis: f32| match value {
                    Some(Length::Px(v)) => Some(finite(v)),
                    Some(Length::Percent(v)) => Some(finite(v) * basis),
                    _ => None,
                };
                // Percentage margins count from the block's width, as CSS's do.
                let margin = |value: Option<Length>, px: f32| match value {
                    Some(Length::Px(v)) => finite(v),
                    Some(Length::Percent(v)) => finite(v) * width,
                    Some(Length::Auto) => 0.,
                    None => px,
                };
                let (mut left, mut right) = (
                    inset(options.inset.left, width),
                    inset(options.inset.right, width),
                );
                // Its style runs mirrored right to left; placed here, not
                // mirrored after, its insets and margins are as given.
                let rtl = self.way(id).parent;
                if rtl {
                    std::mem::swap(&mut left, &mut right);
                }
                let (top, bottom) = (
                    inset(options.inset.top, height),
                    inset(options.inset.bottom, height),
                );
                let mut m = Insets {
                    top: margin(options.margin.top, style.margin.top),
                    right: margin(options.margin.right, style.margin.right),
                    bottom: margin(options.margin.bottom, style.margin.bottom),
                    left: margin(options.margin.left, style.margin.left),
                };
                if rtl {
                    std::mem::swap(&mut m.left, &mut m.right);
                }
                let sized_w = (style.width.is_some() && !style.intrinsic_width)
                    || style.width_percent.is_some();
                let sized_h = (style.height.is_some() && !style.intrinsic_height)
                    || style.height_percent.is_some();
                // Both insets on an axis without a size there: it fills between them.
                let between =
                    |start: Option<f32>, end: Option<f32>, room: f32, before: f32, after: f32| {
                        start
                            .zip(end)
                            .map(|(s, e)| (room - s - e - before - after).max(0.))
                    };
                let room_w =
                    (width - left.unwrap_or(0.) - right.unwrap_or(0.) - m.left - m.right).max(0.);
                let room_h =
                    (height - top.unwrap_or(0.) - bottom.unwrap_or(0.) - m.top - m.bottom).max(0.);
                let constraints = Constraints {
                    width: Some(room_w),
                    height: Some(room_h),
                    percent_width: Some(width),
                    percent_height: Some(height),
                    force_width: if sized_w {
                        None
                    } else {
                        between(left, right, width, m.left, m.right)
                    },
                    force_height: if sized_h {
                        None
                    } else {
                        between(top, bottom, height, m.top, m.bottom)
                    },
                };
                let (w, h) = self.measure_full(id, constraints);
                let start = self.node(parent).bounds;
                let content = self.content_insets(parent);
                let x = match (left, right) {
                    (Some(left), _) => origin_x + left + m.left,
                    (None, Some(right)) => origin_x + width - right - m.right - w,
                    (None, None) => start.x + content.left + m.left,
                };
                let y = match (top, bottom) {
                    (Some(top), _) => origin_y + top + m.top,
                    (None, Some(bottom)) => origin_y + height - bottom - m.bottom - h,
                    (None, None) => start.y + content.top + m.top,
                };
                self.arrange_from(id, x, y, report);
            }
        }
        self.escaped = placed;
    }
    /// `arrange` from `id` at `x`, `y`, a layout walk's start: moved leaves'
    /// damage goes to the grid as cells if it takes them now
    /// (`grid_takes_cells`), otherwise as rounded rectangles all walk long,
    /// a grid made meanwhile taking them as `add_damage` gives them.
    fn arrange_from(&mut self, id: NodeId, x: f32, y: f32, report: &mut FrameReport) {
        assert!(self.contains(id), "stale scene node");
        match self.grid_takes_cells() {
            true => self.arrange::<true>(id, x, y, report, false, 0, &mut None),
            false => self.arrange::<false>(id, x, y, report, false, 0, &mut None),
        }
    }
    pub(crate) fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }
    pub(crate) fn layout_revision(&self) -> u64 {
        self.layout_revision
    }
    /// Updates geometry for hit testing without consuming pending paint damage.
    /// The next `flush` reports this work and all damage accumulated by layout.
    #[inline(never)]
    pub fn prepare_layout(&mut self) {
        let full = self.node(self.root).dirty & LAYOUT != 0;
        if !full && self.layout_roots.is_empty() {
            return;
        }
        self.prepare_layout_active(full);
    }
    // The clean entry needs neither the active pass's stack nor its scratch.
    // Materializing children and starting the profile span do not dirty root.
    #[inline(never)]
    fn prepare_layout_active(&mut self, full: bool) {
        self.settle_children();
        let _span = crate::profile::span(crate::profile::Phase::Layout);
        self.layout_pass = self.layout_pass.wrapping_add(1);
        let mut report = FrameReport::default();
        if full {
            self.measure_full(self.root, Constraints::default());
            self.arrange_from(self.root, 0.0, 0.0, &mut report);
        }
        // Boundaries under clean ancestors keep their place and size; lay out
        // just their subtrees, outermost first.
        let mut roots = std::mem::take(&mut self.layout_roots);
        roots.retain(|id| self.contains(*id));
        // Each root's depth worked out once, not at every comparison.
        roots.sort_by_cached_key(|id| (self.depth(*id), id.index, id.generation));
        roots.dedup();
        let mut relayout_all = false;
        for id in roots {
            if !self.contains(id) || self.node(id).dirty & LAYOUT == 0 {
                continue;
            }
            let Some((constraints, size)) = self
                .node(id)
                .measurement
                .as_ref()
                .map(|m| (m.constraints, m.size))
            else {
                relayout_all = true;
                self.invalidate_layout_to_root(id);
                continue;
            };
            if self.measure_full(id, constraints) != size {
                // Not actually independent of its content: lay out normally.
                relayout_all = true;
                self.invalidate_layout_to_root(id);
                continue;
            }
            let bounds = self.node(id).bounds;
            self.arrange_from(id, bounds.x, bounds.y, &mut report);
        }
        if relayout_all && self.node(self.root).dirty & LAYOUT != 0 {
            self.measure_full(self.root, Constraints::default());
            self.arrange_from(self.root, 0.0, 0.0, &mut report);
        }
        if !self.escaped.is_empty() {
            self.place_escaped(&mut report);
        }
        // Probes this pass answered stay only as the next pass's few.
        for id in std::mem::take(&mut self.overfull) {
            if self.contains(id) {
                let node = self.node_mut(id);
                let at = MEASUREMENT_ALTERNATES.min(node.alternates.len());
                let evicted: Vec<_> = node
                    .alternates
                    .drain(at..)
                    .map(|m| ProbeSize::from(&m))
                    .collect();
                keep_sizes(&mut node.sizes, evicted);
            }
        }
        self.pending_layout_nodes += report.layout_nodes;
        self.layout_revision = self.layout_revision.wrapping_add(1);
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
    }
    /// Lay out, paint and composite what changed since the last flush, and
    /// report the damage.
    pub fn flush(&mut self) -> FrameReport {
        let mut report = FrameReport::default();
        self.repaint_flushes += 1;
        // Read, or past what a renderer would patch rather than redraw: from
        // this flush on. Kept only while a renderer reads them (most never
        // do): otherwise from the next.
        let read = self.repaints_read.replace(false);
        self.repaints_unread = match read {
            true => 0,
            false => self.repaints_unread.saturating_add(1),
        };
        let keep = self.repaints_unread < REPAINTS_UNREAD;
        if read || !keep || self.repaints.len() > REPAINTS_KEPT {
            self.repaints.clear();
            self.repaints_from = self.repaint_flushes + u64::from(!keep);
        }
        if self.dirty.is_empty() {
            return report;
        }
        self.prepare_layout();
        debug_assert_eq!(self.pending_children.get(), u32::MAX);
        let _span = crate::profile::span(crate::profile::Phase::Damage);
        report.layout_nodes = std::mem::take(&mut self.pending_layout_nodes);
        let dirty = std::mem::take(&mut self.dirty);
        for id in &dirty {
            if !self.contains(*id) {
                continue;
            }
            let flags = self.tree_node(*id).dirty;
            if flags & (PAINT | PAINTED) != 0 {
                report.paint_nodes += 1;
            }
            if keep && flags & (PAINT | PAINTED | COMPOSITE | CHILDREN) != 0 {
                self.repaints.push((*id, repaint_flags(flags)));
            }
            if flags & PAINT != 0 {
                self.damage_node(*id);
            }
            if flags & COMPOSITE != 0 {
                report.composite_nodes += 1;
            }
            let node = self.node_mut(*id);
            node.dirty = 0;
            node.queued = false;
        }
        self.dirty = dirty;
        self.dirty.clear();
        self.measure_overflow_fades();
        // Backdrop-filter output depends on pixels behind it, even when its own node is clean.
        if (!self.damage.is_empty() || self.damage_grid.is_some()) && !self.blur_nodes.is_empty() {
            let mut blur_regions: Vec<(Rect, bool)> = self
                .blur_nodes
                .iter()
                .filter_map(|id| {
                    let (bounds, effects, _) = self.world(*id);
                    // Fully transparent filters cannot change the visible backdrop.
                    // Hide/show still damages both old and new subtree bounds.
                    let bounds = match self.transformed {
                        0 => bounds,
                        _ => self.transformed_world_bounds(*id),
                    };
                    (effects.opacity > 0.0).then_some((bounds, false))
                })
                .collect();
            loop {
                let mut changed = false;
                for (rect, processed) in &mut blur_regions {
                    let touched = match &self.damage_grid {
                        Some(grid) => grid.touches(*rect),
                        None => self.damage.iter().any(|damage| damage.intersects(*rect)),
                    };
                    if !*processed && touched {
                        self.add_damage(*rect);
                        *processed = true;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        self.resolve_moves();
        report.damage = match self.damage_grid.take() {
            Some(grid) if grid.full() => vec![self.viewport],
            Some(grid) => grid.rects(self.viewport),
            // A copy: the list keeps its room for the next frame's damage.
            None => {
                let damage = self.damage.clone();
                self.damage.clear();
                damage
            }
        };
        self.recent_damage = Default::default();
        if !report.damage.is_empty() || !self.moves.is_empty() {
            self.flush_serial += 1;
            for scroll in &mut self.moves {
                scroll.serial = self.flush_serial;
            }
        }
        self.trim_slots();
        report
    }
    /// Give back the room doubling left in `slots` once they have stopped
    /// growing for a while (60 flushes): a slot is most of a kilobyte, and
    /// a mount that just passed a power of two keeps up to half of them
    /// unused (650 KB in the heavy dashboard's 1,179 nodes). Mounting itself
    /// grows them by doubling, which copies them least.
    fn trim_slots(&mut self) {
        if self.slots_settled < 60 {
            self.slots_settled += 1;
            return;
        }
        let len = self.slots.len();
        if self.slots.capacity() > len + len / 2 + 256 {
            self.slots.shrink_to(len + len / 4 + 64);
        }
    }
    /// Mark `id` as scroll content: translating it reports a `ScrollMove`
    /// instead of damaging everything it paints, and it moves by whole
    /// device pixels (`set_pixel_scale`).
    pub fn set_scroll_copy(&mut self, id: NodeId, enabled: bool) {
        self.node_mut(id).scroll_copy = enabled;
    }
    /// How many device pixels a point is (the window's scale factor).
    /// Scroll content then moves by whole device pixels, as Safari's and
    /// AppKit's do: a trackpad's fractional offsets would leave its edges
    /// and hairlines straddling pixels (blurred, shimmering as they move)
    /// while its text snaps to them on its own, and the renderer could not
    /// move what it drew by copying it. Offsets stay exact; only what shows
    /// is rounded. Unset (0), nothing is rounded.
    pub fn set_pixel_scale(&mut self, scale: f32) {
        let scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            0.
        };
        if scale == self.pixel_scale {
            return;
        }
        self.pixel_scale = scale;
        // What scrolled before is put on the new grid.
        let mut contents = Vec::new();
        let mut stack = vec![self.root()];
        while let Some(id) = stack.pop() {
            if self.node(id).scroll_copy {
                contents.push(id);
            }
            stack.extend_from_slice(self.children(id));
        }
        for id in contents {
            let transform = self.node(id).transform;
            self.set_transform(id, transform);
        }
    }
    /// `transform` moved to the nearest whole device pixels.
    fn on_pixels(&self, mut transform: Transform) -> Transform {
        let scale = self.pixel_scale;
        if scale > 0. {
            transform.x = (transform.x * scale).round() / scale;
            transform.y = (transform.y * scale).round() / scale;
            // No negative zero: an unmoved view stays as it was.
            transform.x += 0.;
            transform.y += 0.;
        }
        transform
    }
    /// Moves reported by the most recent flush.
    pub fn scroll_moves(&self) -> &[ScrollMove] {
        &self.moves
    }
    /// Counts flushes that reported damage or moves.
    pub fn flush_serial(&self) -> u64 {
        self.flush_serial
    }
    /// The nodes flushes repainted since the last look, each with what
    /// changed (see [`repaint`]), for a renderer that patches what it drew
    /// rather than drawing it again; and the first and last flush they
    /// cover, counting every flush: a renderer that drew after flush `n`
    /// has missed none if they begin at `n + 1`. Nodes whose layout alone
    /// changed (their box as it was) are left out.
    pub fn repaints(&self) -> (std::ops::RangeInclusive<u64>, &[(NodeId, u8)]) {
        self.repaints_read.set(true);
        (self.repaints_from..=self.repaint_flushes, &self.repaints)
    }
    /// Whether any container fades edges its content overflows (those
    /// fades follow scrolling without repainting the container).
    pub fn has_overflow_fades(&self) -> bool {
        !self.overflow_fades.is_empty()
    }
    /// Whether `id` is `ancestor` or inside it.
    pub fn is_within(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            match self.node(id).parent {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }
    /// How far each `fade_overflow` node's content runs past its edges, now
    /// that layout and scrolling have placed it; where that changes a band,
    /// repaint what the node shows.
    fn measure_overflow_fades(&mut self) {
        for index in 0..self.overflow_fades.len() {
            let (id, before) = self.overflow_fades[index];
            let node = self.node(id);
            let own = node.bounds;
            // What scrolls may sit a level or two down (a scroll view's
            // viewport fills it and holds the moving content): look past
            // a lone child as large as the node, adding up translations.
            let (mut holder, mut shift) = (node, Transform::default());
            while let [only] = self.children_raw(holder)[..] {
                let child = self.node(only);
                let same = (child.bounds.x - own.x).abs() < 0.5
                    && (child.bounds.y - own.y).abs() < 0.5
                    && (child.bounds.width - own.width).abs() < 0.5
                    && (child.bounds.height - own.height).abs() < 0.5;
                if !same || self.children_raw(child).is_empty() {
                    break;
                }
                shift.x += child.transform.x;
                shift.y += child.transform.y;
                holder = child;
            }
            let reach = self
                .children_raw(holder)
                .iter()
                .map(|child| {
                    let child = self.node(*child);
                    child.bounds.translated(child.transform).translated(shift)
                })
                .reduce(Rect::union);
            let [top, bottom] = node.style.fade_edges;
            let [left, right] = node.style.fade_edges_x;
            let past = match reach {
                None => [0.; 4],
                Some(r) => [
                    own.y - r.y,
                    r.x + r.width - (own.x + own.width),
                    r.y + r.height - (own.y + own.height),
                    own.x - r.x,
                ],
            };
            let past = [0, 1, 2, 3].map(|i| {
                let band = [top, right, bottom, left][i];
                // Under half a point is layout rounding, not overflow.
                if past[i] < 0.5 || !band.is_finite() {
                    0.
                } else {
                    past[i].min(band.max(0.))
                }
            });
            if past == before {
                continue;
            }
            self.overflow_fades[index].1 = past;
            let (bounds, _, clip) = self.world(id);
            if let Some(shown) = clip.map_or(Some(bounds), |clip| clip.intersection(bounds)) {
                self.add_damage(shown);
            }
        }
    }
    /// A `fade_overflow` node's overflow at each edge (`measure_overflow_fades`).
    fn overflow_of(&self, id: NodeId, node: &Node) -> Option<[f32; 4]> {
        if !node.style.fade_overflow {
            return None;
        }
        let found = self.overflow_fades.iter().find(|(entry, _)| *entry == id);
        Some(found.map_or([0.; 4], |(_, past)| *past))
    }
    /// The corner squares of the rounded clips around `id`, a scroll's
    /// moving content (`ScrollMove::node`), that stay where they are as it
    /// moves (its ancestors'; its own moves with it), in window points: a
    /// copy of the content's pixels moves
    /// their edges and what lies outside them, so a renderer copying them
    /// repaints these and where the copy took them (`ScrollMove`). `None`
    /// if one fades (a feathered clip, soft along its whole edge) or is
    /// inset, which a copy cannot keep.
    pub fn scroll_corners(&self, id: NodeId) -> Option<Vec<Rect>> {
        let mut corners = Vec::new();
        for node_id in self.ancestors(id) {
            let node = self.tree_node(node_id);
            let [Some(own), _] = rounded_clip(RoundedClip::default(), node, self.bounds(node_id))
            else {
                continue;
            };
            if node.clip_inset.is_some() || own.feather != Feather::NONE {
                return None;
            }
            let r = own.rect;
            let [top_left, top_right, bottom_right, bottom_left] = own.radii;
            // A pixel more: its antialiased edge.
            let square = |x: f32, y: f32, radius: f32| {
                (radius > 0.).then(|| Rect::new(x - 1., y - 1., radius + 2., radius + 2.))
            };
            corners.extend(
                [
                    square(r.x, r.y, top_left),
                    square(r.x + r.width - top_right, r.y, top_right),
                    square(
                        r.x + r.width - bottom_right,
                        r.y + r.height - bottom_right,
                        bottom_right,
                    ),
                    square(r.x, r.y + r.height - bottom_left, bottom_left),
                ]
                .into_iter()
                .flatten(),
            );
        }
        Some(corners)
    }
    /// Turn this frame's scroll translations into moves: damage what a copy
    /// cannot supply, or everything the content painted where a copy is
    /// unsafe (isolated ancestors, backdrop filters over the clip, nested or
    /// overlapping moves, jumps past the viewport).
    fn resolve_moves(&mut self) {
        self.moves.clear();
        let pending = std::mem::take(&mut self.pending_moves);
        let mut moves = Vec::new();
        for (id, from, again) in pending {
            if !self.contains(id) {
                continue;
            }
            let to = self.node(id).transform;
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            // A layer's pixels are its own: a copy of the window's moves
            // nothing in it.
            let layered = std::iter::once(id)
                .chain(self.ancestors(id))
                .any(|node| self.is_layer(node));
            if again && self.transformed + self.filtered.get() > 0 {
                // Its clip, wherever transformed or blurred ancestors show it.
                let (_, clip, placement) = self.placed(id);
                let shown = match (placement, clip) {
                    (Some(placement), Some(clip)) => placement.shows(clip),
                    (Some(placement), None) => Some(placement.clip),
                    (None, clip) => Some(clip.unwrap_or(self.viewport)),
                };
                if let Some(shown) = shown {
                    self.add_damage(shown);
                }
                continue;
            }
            if again {
                // Descendants changed at an intermediate position (a measured
                // list re-anchoring between layout passes, possibly back to
                // where it started) were damaged neither where they were
                // painted nor where they end: repaint everything it shows.
                match self.world_clip(id) {
                    Some(clip) => self.add_damage(clip),
                    None => self.add_damage(self.viewport),
                }
                continue;
            }
            if dx == 0. && dy == 0. {
                continue;
            }
            let clip = self.world_clip(id);
            let isolated = layered;
            let filtered = |scene: &Self, clip: Rect| {
                scene.blur_nodes.iter().any(|blur| {
                    let (bounds, effects, _) = scene.world(*blur);
                    effects.opacity > 0. && bounds.intersects(clip)
                })
            };
            let usable = clip.filter(|clip| {
                !isolated
                    && dx.abs() < clip.width
                    && dy.abs() < clip.height
                    && !filtered(self, *clip)
            });
            match usable {
                Some(clip) => moves.push(ScrollMove {
                    node: id,
                    clip,
                    dx,
                    dy,
                    serial: 0,
                }),
                None => {
                    // Everything the content painted, at both positions.
                    let now = self.node(id).transform;
                    self.node_mut(id).transform = from;
                    self.damage_subtree(id);
                    self.node_mut(id).transform = now;
                    self.damage_subtree(id);
                }
            }
        }
        // Nested or overlapping moves: repaint instead.
        let overlapping: Vec<usize> = (0..moves.len())
            .filter(|&i| {
                (0..moves.len()).any(|j| {
                    i != j
                        && (moves[i].clip.intersects(moves[j].clip)
                            || self.is_within(moves[i].node, moves[j].node))
                })
            })
            .collect();
        for (index, scroll) in moves.into_iter().enumerate() {
            if overlapping.contains(&index) {
                self.add_damage(scroll.clip);
                continue;
            }
            // Other changes inside the clip also land where the copy moves
            // them; the strip the copy cannot fill is exposed.
            let inside: Vec<Rect> = self
                .damage
                .iter()
                .filter_map(|rect| rect.intersection(scroll.clip))
                .collect();
            for rect in inside {
                let shifted = rect.translated(Transform {
                    x: scroll.dx,
                    y: scroll.dy,
                });
                if let Some(rect) = shifted.intersection(scroll.clip) {
                    self.add_damage(rect);
                }
            }
            for rect in exposed(scroll.clip, scroll.dx, scroll.dy) {
                self.add_damage(rect);
            }
            self.moves.push(scroll);
        }
    }
    /// Isolates this subtree into an offscreen group. Opacity then applies once to
    /// overlapping children. Legacy nodes retain per-primitive inherited opacity.
    pub fn set_isolated(&mut self, id: NodeId, isolated: bool) {
        if self.node(id).isolated == isolated {
            return;
        }
        self.damage_subtree(id);
        if isolated {
            self.isolation_count += 1;
        } else {
            self.isolation_count -= 1;
        }
        self.node_mut(id).isolated = isolated;
        self.invalidate(id, PAINT);
    }
    /// Whether `id` renders as a group into a cached layer (`isolated`).
    pub fn is_isolated(&self, id: NodeId) -> bool {
        self.node(id).isolated
    }
    /// Counts changes to `id`'s layer contents: a renderer repaints its
    /// cached layer when this changes.
    pub fn layer_revision(&self, id: NodeId) -> u64 {
        self.node(id).layer_revision
    }
    /// The isolated nodes.
    pub fn isolated_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.node
                .as_ref()
                .filter(|node| node.isolated && !slot.dead)
                .map(|_| NodeId {
                    index: index as u32,
                    generation: slot.generation,
                })
        })
    }
    /// World-space content bounds, without ancestor clipping, for offscreen
    /// allocation. A transformed layer's are in its own untransformed space,
    /// where its content is laid out.
    pub fn layer_bounds(&self, id: NodeId) -> Rect {
        let mut bounds = self.bounds(id);
        for item in self.layer_items(Some(id)) {
            let r = if item.isolated {
                // A nested blur's output, spread past its own content.
                let nested = self
                    .layer_bounds(item.id)
                    .expand(filter_spread(item.effects.filter_blur));
                item.projective()
                    .map_or(nested, |matrix| matrix.bounds(nested))
            } else if let NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } = item.kind {
                quad.paint_bounds(item.bounds)
            } else if let NodeKind::Svg(svg) = item.kind {
                svg.paint_bounds(item.bounds)
            } else if let NodeKind::Image(image) = item.kind {
                image.paint_bounds(item.bounds)
            } else {
                item.bounds
            };
            let r = item.clip.map_or(Some(r), |clip| r.intersection(clip));
            if let Some(r) = r {
                bounds = bounds.union(r);
            }
        }
        bounds
    }
    /// Paints a single compositor layer, replacing nested isolation boundaries with
    /// one item. A layer root excludes its own opacity/filter from its cached pixels.
    /// Changes whenever nodes are added, removed or change kind. Renderers can
    /// prune per-node caches only when it moves.
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    /// True when any node applies a backdrop blur, whose output depends on
    /// pixels outside its own subtree.
    pub fn has_backdrop_blur(&self) -> bool {
        !self.blur_nodes.is_empty()
    }
    /// The paint items in paint order: the window's with `None` (popups left
    /// out), or popup `root`'s alone.
    pub fn layer_items(&self, root: Option<NodeId>) -> Vec<PaintItem<'_>> {
        self.layer_items_within(root, None)
    }
    /// Like [`Scene::layer_items`], but skips whole subtrees that paint nothing
    /// inside `damage`. Items that remain may still lie partly outside it.
    pub fn layer_items_within(
        &self,
        root: Option<NodeId>,
        damage: Option<&[Rect]>,
    ) -> Vec<PaintItem<'_>> {
        match self.popups.is_empty() {
            true => self.layer_items_walk::<false, false>(root, damage, &mut Vec::new(), None),
            false => self.layer_items_walk::<true, false>(root, damage, &mut Vec::new(), None),
        }
    }
    /// The window's paint items (as [`Scene::layer_items`] with `None`), and
    /// into `inherited` what each took from its ancestors, for
    /// [`Scene::item_in`] to work it out again alone.
    pub fn layer_items_recorded(&self, inherited: &mut Vec<Inherited>) -> Vec<PaintItem<'_>> {
        inherited.clear();
        match self.popups.is_empty() {
            true => self.layer_items_walk::<false, true>(None, None, inherited, None),
            false => self.layer_items_walk::<true, true>(None, None, inherited, None),
        }
    }
    /// The paint items of `id`'s subtree as the window's walk makes them,
    /// from what `id` `inherited` in the walk that recorded it (its
    /// ancestors unchanged since), and into `out` what each inherits: for a
    /// renderer splicing a repainted subtree into a frame it kept.
    /// Renderer plumbing, not a stable API.
    #[doc(hidden)]
    pub fn layer_items_from(
        &self,
        id: NodeId,
        inherited: &Inherited,
        out: &mut Vec<Inherited>,
    ) -> Vec<PaintItem<'_>> {
        out.clear();
        match self.popups.is_empty() {
            true => self.layer_items_walk::<false, true>(None, None, out, Some((id, inherited))),
            false => self.layer_items_walk::<true, true>(None, None, out, Some((id, inherited))),
        }
    }
    /// Plain node `id`'s box as [`Scene::item_in`] places it, from what it
    /// `inherited`, and what it paints: for a renderer patching a node
    /// layout alone moved, whose item is otherwise as it was. Plumbing for
    /// zgx-gpu, not a stable API.
    #[doc(hidden)]
    pub fn box_in(&self, id: NodeId, inherited: &Inherited) -> (Rect, &NodeKind) {
        let node = self.node(id);
        let transform = Transform {
            x: inherited.transform.x + node.transform.x,
            y: inherited.transform.y + node.transform.y,
        };
        (node.bounds.translated(transform), &node.kind)
    }
    /// Paint item `id` worked out alone, from what it `inherited` in the walk
    /// that recorded it (`layer_items_recorded`), its ancestors unchanged
    /// since: the walk's step for a plain node, one adding no layer, colour
    /// filter, fade, clip inset or clip of its own. `None` for any other, or
    /// for one painting nothing now.
    pub fn item_in(&self, id: NodeId, inherited: &Inherited) -> Option<PaintItem<'_>> {
        let node = self.node(id);
        if node.isolated
            || node.transformed()
            || node.effect_layer()
            || node.effects.color_matrix().is_some()
            || node.clip_inset.is_some()
            || fades(&node.style)
            || (matches!(node.kind, NodeKind::Quad(_) | NodeKind::Panel { .. })
                && clip_axes(&node.style) == (true, true))
        {
            return None;
        }
        let transform = Transform {
            x: inherited.transform.x + node.transform.x,
            y: inherited.transform.y + node.transform.y,
        };
        let bounds = node.bounds.translated(transform);
        let mut effects = node.effects;
        effects.opacity *= inherited.opacity;
        if effects.opacity <= 0.0 || !own_visible(&node.style) {
            return None;
        }
        let clip = match inherited.border {
            true => inherited.clip,
            false => clipped_bounds(inherited.clip, bounds, &node.style),
        };
        let clip = match &node.kind {
            NodeKind::Quad(_) | NodeKind::Panel { .. } => inherited.clip,
            _ => clip,
        };
        Some(PaintItem {
            isolated: false,
            id,
            bounds,
            kind: &node.kind,
            font: &node.font,
            text_options: node.style.text_options,
            effects,
            clip,
            border_pass: inherited.border,
            extras: inherited.extras.clone(),
        })
    }
    /// `POPUPS`: popup roots exist, which this walk leaves out below `root`.
    /// `RECORD`: what each item inherits goes into `inherited`.
    /// `from`: a subtree alone, from what its root inherited (`root` and
    /// `damage` are then `None`).
    fn layer_items_walk<const POPUPS: bool, const RECORD: bool>(
        &self,
        root: Option<NodeId>,
        damage: Option<&[Rect]>,
        inherited: &mut Vec<Inherited>,
        from: Option<(NodeId, &Inherited)>,
    ) -> Vec<PaintItem<'_>> {
        self.settle_children();
        let reach = damage.map(|damage| {
            damage
                .iter()
                .copied()
                .reduce(Rect::union)
                .unwrap_or_default()
        });
        let touches = |ink: Option<Rect>, transform: Transform| -> bool {
            let (Some(damage), Some(reach)) = (damage, reach) else {
                return true;
            };
            let Some(ink) = ink else {
                return false;
            };
            let ink = ink.translated(transform);
            ink.intersects(reach) && damage.iter().any(|d| d.intersects(ink))
        };
        let start = root.unwrap_or(self.root);
        if from.is_none() && !self.layout_visible(start) {
            return Vec::new();
        }
        let mut transform = Transform::default();
        let mut parent = self.node(start).parent;
        while let Some(id) = parent {
            let n = self.node(id);
            transform.x += n.transform.x;
            transform.y += n.transform.y;
            parent = n.parent;
        }
        let clip = if root.is_some() {
            None
        } else {
            Some(self.viewport)
        };
        // Sized as the last walk was, rather than grown from nothing each frame.
        let (stack_hint, items_hint) = match from {
            None => self.walk_hints.get(),
            Some(_) => (1, 0),
        };
        let mut stack = Vec::with_capacity(stack_hint.max(1));
        // Each entry: a node, what its parent passes on (translation,
        // opacity, clip, and the fade mask, rounded clips and colour filters
        // as shared `PaintExtras`), and whether it is a border pass.
        stack.push(match from {
            None => (
                start,
                transform,
                1.,
                clip,
                None::<std::rc::Rc<PaintExtras>>,
                false,
            ),
            Some((id, from)) => (
                id,
                from.transform,
                from.opacity,
                from.clip,
                from.extras.clone(),
                from.border,
            ),
        });
        let mut result = Vec::with_capacity(items_hint);
        let mut deepest = 0;
        while let Some((id, parent_transform, parent_opacity, parent_clip, context, border)) =
            stack.pop()
        {
            self.visit(id);
            let node = self.node(id);
            let is_root = root == Some(id);
            // A popup window draws it (`set_popup`).
            if POPUPS && !is_root && self.popups.contains(&id) {
                continue;
            }
            let transform = Transform {
                x: parent_transform.x + node.transform.x,
                y: parent_transform.y + node.transform.y,
            };
            let bounds = node.bounds.translated(transform);
            let transformed = node.transformed();
            let effect = node.effect_layer();
            let isolated = (node.isolated || transformed || effect) && !is_root;
            let mut effects = if is_root {
                Effects::default()
            } else {
                node.effects
            };
            effects.opacity *= parent_opacity;
            // Its colour filters after its ancestors' (a layer root's own
            // apply at its composite).
            let own_color = if is_root {
                None
            } else {
                node.effects.color_matrix()
            };
            // Fully transparent subtrees contribute no cached pixels or layer
            // bounds. An explicit layer root keeps opacity external above.
            if effects.opacity <= 0.0 || !own_visible(&node.style) {
                continue;
            }
            if RECORD {
                inherited.push(Inherited {
                    transform: parent_transform,
                    opacity: parent_opacity,
                    clip: parent_clip,
                    extras: context.clone(),
                    border,
                });
            }
            // A transformed layer clips itself, inside, to its rotated box; a
            // blurred or masked one clips inside, before its effect.
            let matrix = (isolated && transformed).then(|| node.projective(bounds));
            // A border pass (its border and outline over what it holds)
            // isn't cut by its own clip, as CSS doesn't cut an outline.
            let clip = match (isolated && (transformed || effect)) || border {
                true => parent_clip,
                false => clipped_bounds(parent_clip, bounds, &node.style),
            };
            // Most nodes add no fade, rounded clip, clip inset or colour
            // filter: they and their children carry what was passed on.
            let plain = own_color.is_none()
                && matrix.is_none()
                && node.clip_inset.is_none()
                && !fades(&node.style)
                && !(matches!(node.kind, NodeKind::Quad(_) | NodeKind::Panel { .. })
                    && clip_axes(&node.style) == (true, true));
            // What it adds to what was passed on: none for a plain node, which
            // passes on what it was given (most nodes, so none of this is
            // worked out for them).
            let (clip, inset, own) = if plain {
                (clip, None, None)
            } else {
                let inherited = context.as_deref();
                let mask = inherited.and_then(|c| c.mask);
                let rounded = inherited.map_or_else(RoundedClip::default, |c| c.rounded);
                let parent_color = inherited.and_then(|c| c.color);
                let color = compose_color(parent_color, own_color);
                let child_mask = fade_mask(mask, clip, &node.style, self.overflow_of(id, node));
                let mut child_rounded = rounded_clip(rounded, node, bounds);
                // Its clip inset: on its item and descendants here, on the
                // composite of an untransformed layer (after its effect), and
                // inside a transformed one, where its own space is.
                let inset = node
                    .clip_inset
                    .as_deref()
                    .filter(|_| match (is_root, isolated) {
                        (true, _) => transformed,
                        (false, true) => !transformed,
                        (false, false) => true,
                    })
                    .map(|inset| inset.apply(bounds));
                let clip = match inset {
                    None => clip,
                    Some(inset) => Some(
                        clip.map_or(inset, |clip| clip.intersection(inset).unwrap_or_default()),
                    ),
                };
                // Its rounded clip rounds the inset box instead.
                let mut item_rounded = rounded;
                if let Some(inset) = inset
                    && child_rounded != rounded
                {
                    child_rounded = round_inset(child_rounded, inset);
                    item_rounded = child_rounded;
                }
                let own = Own {
                    mask,
                    rounded,
                    parent_color,
                    color,
                    child_mask,
                    child_rounded,
                    item_rounded,
                };
                (clip, inset, Some(own))
            };
            // Its border over what it holds: a second item once they paint.
            let border_over = matches!(&node.kind,
                NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } if quad.paints_over());
            // The item's extras and its children's, shared with what passed
            // them on wherever they are the same.
            let extras = match &own {
                Some(own)
                    if matrix.is_some()
                        || own.item_rounded != own.rounded
                        || own.color != own.parent_color =>
                {
                    PaintExtras {
                        // Affine where it is; in perspective, the map
                        // before the tilt, with the whole map beside it.
                        matrix: matrix
                            .map(|map| map.affine().unwrap_or_else(|| node.flat_matrix(bounds))),
                        mask: own.mask,
                        rounded: own.item_rounded,
                        color: own.color,
                        projective: matrix.filter(|map| map.affine().is_none()),
                    }
                    .shared()
                }
                _ => context.clone(),
            };
            if !isolated && !border && border_over {
                let opacity = parent_opacity;
                stack.push((
                    id,
                    parent_transform,
                    opacity,
                    parent_clip,
                    context.clone(),
                    true,
                ));
            }
            if !isolated && !border {
                let passed = match own {
                    None => context,
                    Some(own)
                        if own.child_mask == own.mask
                            && own.child_rounded == own.rounded
                            && own.color == own.parent_color =>
                    {
                        context
                    }
                    Some(own)
                        if matrix.is_none()
                            && own.child_rounded == own.item_rounded
                            && own.child_mask == own.mask =>
                    {
                        extras.clone()
                    }
                    Some(own) => PaintExtras {
                        matrix: None,
                        mask: own.child_mask,
                        rounded: own.child_rounded,
                        color: own.color,
                        projective: None,
                    }
                    .shared(),
                };
                stack.extend(
                    self.children_raw(node)
                        .iter()
                        .rev()
                        .filter(|child| touches(self.ink(**child), transform))
                        .map(|child| {
                            let opacity = effects.opacity;
                            (*child, transform, opacity, clip, passed.clone(), false)
                        }),
                );
                deepest = deepest.max(stack.len());
            }
            // A box's own paint (fill, border, shadows) isn't cut by its own
            // overflow clip: CSS clips what an element holds, not its
            // box-shadow. Its clip inset (clip-path) still cuts it.
            let own_clip = match &node.kind {
                NodeKind::Quad(_) | NodeKind::Panel { .. } => match inset {
                    None => parent_clip,
                    Some(inset) => Some(
                        parent_clip
                            .map_or(inset, |clip| clip.intersection(inset).unwrap_or_default()),
                    ),
                },
                _ => clip,
            };
            result.push(PaintItem {
                isolated,
                id,
                bounds,
                kind: &node.kind,
                font: &node.font,
                text_options: node.style.text_options,
                effects,
                clip: own_clip,
                border_pass: border,
                extras,
            });
        }
        // (A subtree's walk leaves the window's hints.)
        if from.is_none() {
            self.walk_hints.set((deepest, result.len()));
        }
        result
    }
    /// Every paint item, in paint order from the root.
    pub fn paint_items(&self) -> impl Iterator<Item = PaintItem<'_>> {
        self.settle_children();
        PaintIter {
            scene: self,
            stack: vec![(self.root, Transform::default(), 1.0, self.viewport, None)],
        }
    }
}
/// Whether `style` fades any edge (`fade_mask` makes a mask of its own).
fn fades(style: &Style) -> bool {
    style
        .fade_edges
        .iter()
        .chain(&style.fade_edges_x)
        .any(|band| band.is_finite() && *band > 0.)
}
/// The mask a node passes to its children: its own when it fades its edges
/// (over its clip, less `fade_insets`), otherwise the one it inherited.
fn fade_mask(
    inherited: Option<FadeMask>,
    clip: Option<Rect>,
    style: &Style,
    overflow: Option<[f32; 4]>,
) -> Option<FadeMask> {
    let [top, bottom] = style.fade_edges;
    let [left, right] = style.fade_edges_x;
    let mut bands =
        [top, right, bottom, left].map(|band| if band.is_finite() { band.max(0.) } else { 0. });
    if let Some(overflow) = overflow {
        bands = [0, 1, 2, 3].map(|i| bands[i].min(overflow[i]));
    }
    match clip {
        Some(clip) if bands.iter().any(|band| *band > 0.) => {
            let inset = |inset: f32| if inset.is_finite() { inset.max(0.) } else { 0. };
            let ([top, bottom], [left, right]) =
                (style.fade_insets.map(inset), style.fade_insets_x.map(inset));
            let rect = Rect::new(
                clip.x + left.min(clip.width),
                clip.y + top.min(clip.height),
                (clip.width - left - right).max(0.),
                (clip.height - top - bottom).max(0.),
            );
            Some((rect, bands))
        }
        _ => inherited,
    }
}
/// The rounded clips a node passes to its children: its own box first when
/// it clips both axes with rounded corners, otherwise those it inherited.
fn rounded_clip(inherited: RoundedClip, node: &Node, bounds: Rect) -> RoundedClip {
    let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = &node.kind else {
        return inherited;
    };
    if clip_axes(&node.style) != (true, true) {
        return inherited;
    }
    let (corners, feather) = match quad.decoration.as_deref() {
        Some(detail) => (detail.corners, detail.clip_feather),
        None => (None, 0.),
    };
    let corners = corners.unwrap_or(crate::decoration::Corners::all(quad.radius));
    let most = bounds.width.min(bounds.height).max(0.) / 2.;
    let radius = |value: f32| {
        if value.is_finite() {
            value.clamp(0., most)
        } else {
            0.
        }
    };
    let radii = [
        radius(corners.top_left),
        radius(corners.top_right),
        radius(corners.bottom_right),
        radius(corners.bottom_left),
    ];
    if radii.iter().all(|radius| *radius <= 0.) && feather <= 0. {
        return inherited;
    }
    // CSS clips overflow at the padding box: inside a border that takes
    // room, its corners rounded by the radius less the border. Clipped at
    // the border box, a child filling the padding box covers the border's
    // corner arcs. A clip inset rounds the inset box instead (its caller).
    let border = if node.clip_inset.is_none() {
        quad.layout_border()
    } else {
        Insets::default()
    };
    let inner = |radius: f32, a: f32, b: f32| (radius - a.max(b)).max(0.);
    let rect = Rect::new(
        bounds.x + border.left,
        bounds.y + border.top,
        (bounds.width - border.left - border.right).max(0.),
        (bounds.height - border.top - border.bottom).max(0.),
    );
    let radii = [
        inner(radii[0], border.top, border.left),
        inner(radii[1], border.top, border.right),
        inner(radii[2], border.bottom, border.right),
        inner(radii[3], border.bottom, border.left),
    ];
    [
        Some(RoundRect {
            rect,
            radii,
            feather: Feather::new(feather),
        }),
        inherited[0],
    ]
}
/// A node's own rounded clip (`clip[0]`, from `rounded_clip`) on its inset
/// box instead, its corners held to half that box's shorter side as they are
/// to the box's: a round box stays round as its inset closes in (an oval
/// reveal opening).
fn round_inset(mut clip: RoundedClip, inset: Rect) -> RoundedClip {
    if let Some(own) = &mut clip[0] {
        let most = inset.width.min(inset.height).max(0.) / 2.;
        own.rect = inset;
        own.radii = own.radii.map(|radius| radius.min(most));
    }
    clip
}
fn clip_axes(style: &Style) -> (bool, bool) {
    let options = style.layout_options.as_deref();
    (
        options.and_then(|o| o.clip_x).unwrap_or(style.clip),
        options.and_then(|o| o.clip_y).unwrap_or(style.clip),
    )
}
/// `rect` with its edges out to whole points (see `Scene::add_damage`).
#[inline(always)]
fn quantized(rect: Rect) -> Rect {
    Rect::new(
        rect.x.floor(),
        rect.y.floor(),
        (rect.x + rect.width).ceil() - rect.x.floor(),
        (rect.y + rect.height).ceil() - rect.y.floor(),
    )
}
fn clipped_bounds(parent: Option<Rect>, bounds: Rect, style: &Style) -> Option<Rect> {
    let (x, y) = clip_axes(style);
    if !x && !y {
        return parent;
    }
    let base = parent.unwrap_or(UNCLIPPED);
    let mask = Rect::new(
        if x { bounds.x } else { base.x },
        if y { bounds.y } else { base.y },
        if x { bounds.width } else { base.width },
        if y { bounds.height } else { base.height },
    );
    Some(base.intersection(mask).unwrap_or_default())
}
struct PaintIter<'a> {
    scene: &'a Scene,
    stack: Vec<PaintFrame>,
}
/// A node `PaintIter` will visit, with what its parent passes on: its
/// translation, opacity, clip, fade mask, rounded clips and colour filters.
type PaintFrame = (
    NodeId,
    Transform,
    f32,
    Rect,
    Option<std::rc::Rc<PaintExtras>>,
);
impl<'a> Iterator for PaintIter<'a> {
    type Item = PaintItem<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let (id, parent_transform, parent_opacity, parent_clip, extras) = self.stack.pop()?;
        let inherited = extras.as_deref();
        let mask = inherited.and_then(|c| c.mask);
        let rounded = inherited.map_or_else(RoundedClip::default, |c| c.rounded);
        let parent_color = inherited.and_then(|c| c.color);
        let node = self.scene.node(id);
        let transform = Transform {
            x: parent_transform.x + node.transform.x,
            y: parent_transform.y + node.transform.y,
        };
        let bounds = node.bounds.translated(transform);
        let mut effects = node.effects;
        if !own_visible(&node.style) {
            effects.opacity = 0.;
        }
        effects.opacity *= parent_opacity;
        let clip = clipped_bounds(Some(parent_clip), bounds, &node.style).unwrap_or(parent_clip);
        let child_mask = fade_mask(
            mask,
            Some(clip),
            &node.style,
            self.scene.overflow_of(id, node),
        );
        let inset = node.clip_inset.as_deref().map(|inset| inset.apply(bounds));
        let clip = match inset {
            Some(inset) => clip.intersection(inset).unwrap_or_default(),
            None => clip,
        };
        // A box's own paint isn't cut by its own overflow clip (as above).
        let own_clip = match &node.kind {
            NodeKind::Quad(_) | NodeKind::Panel { .. } => match inset {
                Some(inset) => parent_clip.intersection(inset).unwrap_or_default(),
                None => parent_clip,
            },
            _ => clip,
        };
        let mut child_rounded = rounded_clip(rounded, node, bounds);
        if let Some(inset) = inset
            && child_rounded != rounded
        {
            child_rounded = round_inset(child_rounded, inset);
        }
        let color = compose_color(parent_color, node.effects.color_matrix());
        let own = if color == parent_color {
            extras
        } else {
            PaintExtras {
                matrix: None,
                mask,
                rounded,
                color,
                projective: None,
            }
            .shared()
        };
        let passed = if child_mask == mask && child_rounded == rounded {
            own.clone()
        } else {
            PaintExtras {
                matrix: None,
                mask: child_mask,
                rounded: child_rounded,
                color,
                projective: None,
            }
            .shared()
        };
        self.stack
            .extend(self.scene.children_raw(node).iter().rev().map(|child| {
                let opacity = effects.opacity;
                (*child, transform, opacity, clip, passed.clone())
            }));
        Some(PaintItem {
            isolated: false,
            id,
            bounds,
            kind: &node.kind,
            font: &node.font,
            text_options: node.style.text_options,
            effects,
            clip: Some(own_clip),
            border_pass: false,
            extras: own,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(value: &str) -> NodeKind {
        NodeKind::Text {
            text: value.into(),
            color: Color(255, 255, 255, 255),
            font_size: 16.0,
        }
    }
    /// A text's proof that it holds no right-to-left script goes with its
    /// text: changed to Hebrew, its ink is its whole box again.
    #[test]
    fn a_text_changed_to_right_to_left_inks_its_whole_box_again() {
        let mut scene = Scene::new(300., 100.);
        scene.set_font_text_measurer(|text, _, _, _| (text.chars().count() as f32 * 4., 10.));
        let label = scene.append(
            scene.root(),
            text("hello"),
            Style {
                width: Some(200.),
                height: Some(20.),
                ..Style::default()
            },
        );
        scene.flush();
        let bounds = scene.bounds(label);
        let ink = scene.text_ink(label, bounds);
        assert_ne!(ink, bounds, "left to right: the text's own extent");
        // Asked again with the proof kept: the same answer.
        assert_eq!(scene.text_ink(label, bounds), ink);
        scene.set_text(label, "שלום עולם");
        scene.flush();
        let bounds = scene.bounds(label);
        assert_eq!(scene.text_ink(label, bounds), bounds);
    }
    #[test]
    fn flex_distribution_respects_limits_and_resizes() {
        let mut scene = Scene::new(300.0, 80.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Row));
        let a = scene.append(
            scene.root(),
            text("a"),
            Style {
                width: Some(50.0),
                max_width: Some(70.0),
                flex_grow: 1.0,
                ..Style::default()
            },
        );
        let b = scene.append(
            scene.root(),
            text("b"),
            Style {
                width: Some(50.0),
                flex_grow: 1.0,
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(a).width, 70.0);
        assert_eq!(scene.bounds(b).width, 230.0);
        scene.resize(400.0, 80.0);
        let report = scene.flush();
        assert_eq!(scene.bounds(b).width, 330.0);
        assert!(!report.damage.is_empty());
        assert!(scene.flush().is_idle());
    }
    #[test]
    fn flex_shrink_and_margins_are_included_in_allocation() {
        let mut scene = Scene::new(140.0, 80.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Row));
        let a = scene.append(
            scene.root(),
            text("a"),
            Style {
                width: Some(100.0),
                min_width: Some(80.0),
                flex_shrink: 1.0,
                ..Style::default()
            },
        );
        let b = scene.append(
            scene.root(),
            text("b"),
            Style {
                width: Some(100.0),
                flex_shrink: 1.0,
                margin: Insets::all(5.0),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(a).width, 80.0);
        assert_eq!(scene.bounds(b), Rect::new(85.0, 5.0, 50.0, 23.0));
    }
    #[test]
    fn text_measurement_reflows_when_parent_constraint_changes() {
        let mut scene = Scene::new(100.0, 300.0);
        scene.set_text_measurer(|text: &str, _: f32, limit: Option<f32>| {
            let width = text.len() as f32 * 10.0;
            let limit = limit.unwrap_or(width).max(1.0);
            (width.min(limit), (width / limit).ceil() * 20.0)
        });
        assert_eq!(
            scene.measure_text("abcdefghijabcdefghij", 16.0, Some(50.0)),
            (50.0, 80.0)
        );
        let a = scene.append(
            scene.root(),
            text("abcdefghijabcdefghij"),
            Style {
                text_wrap: true,
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(a).height, 40.0);
        scene.resize(50.0, 300.0);
        scene.flush();
        assert_eq!(scene.bounds(a).height, 80.0);
    }
    #[test]
    fn text_sizes_survive_alternating_wrap_limits() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = calls.clone();
        let mut scene = Scene::new(300.0, 100.0);
        scene.set_text_measurer(move |text: &str, _: f32, _: Option<f32>| {
            counter.set(counter.get() + 1);
            (text.len() as f32 * 10.0, 20.0)
        });
        let label = scene.append(
            scene.root(),
            text("label"),
            Style {
                text_wrap: true,
                ..Style::default()
            },
        );
        let mut sizes = TextSizes::default();
        for limit in [Some(100.), Some(200.), Some(100.), Some(200.)] {
            if sizes.find(limit, 0).is_none() {
                sizes.insert(limit, 0, (1., 1.));
            }
        }
        assert_eq!(sizes.0.iter().flatten().count(), 2);
        scene.flush();
        let first = calls.get();
        // Relayout of the parent re-measures the clean label at both widths.
        for width in [200.0, 100.0, 200.0, 100.0] {
            scene.set_style(
                scene.root(),
                Style {
                    width: Some(width),
                    height: Some(100.0),
                    ..Style::default()
                },
            );
            scene.flush();
        }
        assert!(
            calls.get() <= first + 2,
            "{} measurements",
            calls.get() - first
        );
        let _ = label;
    }
    #[test]
    fn cached_text_size_follows_text_and_measurer_changes() {
        let mut scene = Scene::new(300.0, 100.0);
        scene.set_text_measurer(|text: &str, _: f32, _: Option<f32>| {
            (text.len() as f32 * 10.0, 20.0)
        });
        let label = scene.append(scene.root(), text("label"), Style::default());
        scene.flush();
        assert_eq!(scene.bounds(label).width, 50.0);
        scene.set_kind(label, text("longer label"));
        scene.flush();
        assert_eq!(scene.bounds(label).width, 120.0);
        scene.set_text_measurer(|text: &str, _: f32, _: Option<f32>| {
            (text.len() as f32 * 5.0, 20.0)
        });
        scene.flush();
        assert_eq!(scene.bounds(label).width, 60.0);
    }
    #[test]
    fn alignment_stretch_and_justification_apply_to_children() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.set_style(
            scene.root(),
            Style {
                width: Some(100.0),
                height: Some(100.0),
                align: Align::Stretch,
                justify: Justify::SpaceBetween,
                ..Style::default()
            },
        );
        let a = scene.append(scene.root(), text("a"), Style::default());
        let b = scene.append(scene.root(), text("b"), Style::default());
        scene.flush();
        assert_eq!(scene.bounds(a), Rect::new(0.0, 0.0, 100.0, 23.0));
        assert_eq!(scene.bounds(b), Rect::new(0.0, 77.0, 100.0, 23.0));
    }
    #[test]
    fn popup_roots_are_hit_past_the_viewport_and_drawn_apart() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let quad = |fill| {
            NodeKind::Quad(QuadStyle {
                fill,
                ..Default::default()
            })
        };
        let root = scene.root();
        let popup = scene.append(root, NodeKind::Container(Layout::Overlay), Style::default());
        // Past the window's right edge, as a menu hangs.
        let item = scene.append(
            popup,
            quad(Color(1, 2, 3, 255)),
            Style {
                absolute: true,
                width: Some(60.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_transform(item, Transform { x: 90., y: 10. });
        scene.flush();
        assert_eq!(scene.hit_test(120., 20.), None);
        scene.set_popup(popup, true);
        scene.flush();
        // Hit past the viewport, left out of the window's paint, drawn alone.
        assert_eq!(scene.hit_test(120., 20.), Some(item));
        assert!(!scene.layer_items(None).iter().any(|i| i.id == item));
        let alone = scene.layer_items(Some(popup));
        assert!(alone.iter().any(|i| i.id == item && i.clip.is_none()));
        assert_eq!(
            scene.popup_bounds(popup),
            Some(Rect::new(90., 10., 60., 20.))
        );
        // A change inside it counts on its layer revision.
        let revision = scene.layer_revision(popup);
        scene.set_kind(item, quad(Color(4, 5, 6, 255)));
        scene.flush();
        assert_ne!(scene.layer_revision(popup), revision);
        // Hidden, it shows nothing; gone, it is no popup root.
        scene.set_effects(
            popup,
            Effects {
                opacity: 0.,
                ..Default::default()
            },
        );
        assert_eq!(scene.popup_bounds(popup), None);
        scene.remove(popup);
        assert!(scene.popups().is_empty());
    }
    #[test]
    fn hit_testing_respects_stacking_clip_transform_and_opacity() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let a = scene.append(scene.root(), text("aaaaaaaa"), Style::default());
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(20.0),
                height: Some(20.0),
                clip: true,
                ..Style::default()
            },
        );
        let child = scene.append(parent, text("bbbbbbbb"), Style::default());
        scene.set_transform(parent, Transform { x: 10.0, y: 0.0 });
        scene.flush();
        assert_eq!(scene.hit_test(15.0, 10.0), Some(child));
        assert_eq!(scene.hit_test(35.0, 10.0), Some(a));
        assert_eq!(
            scene.ancestors(child).collect::<Vec<_>>(),
            vec![parent, scene.root()]
        );
        scene.set_effects(
            parent,
            Effects {
                opacity: 0.0,
                ..Effects::default()
            },
        );
        assert_eq!(scene.hit_test(15.0, 10.0), Some(a));
        assert_eq!(scene.hit_test(110.0, 10.0), None);
    }
    #[test]
    fn preparing_input_geometry_preserves_damage_for_renderer() {
        let mut scene = Scene::new(200.0, 200.0);
        let node = scene.append(scene.root(), text("first"), Style::default());
        scene.prepare_layout();
        assert!(scene.bounds(node).width > 0.0);
        scene.set_text(node, "longer updated text");
        scene.prepare_layout();
        let report = scene.flush();
        assert!(report.layout_nodes >= 4);
        assert!(!report.damage.is_empty());
        assert!(scene.flush().is_idle());
    }
    #[test]
    fn idle_and_equal_writes_do_no_work() {
        let mut scene = Scene::new(800.0, 600.0);
        let id = scene.append(scene.root(), text("hi"), Style::default());
        assert!(!scene.flush().damage.is_empty());
        assert!(scene.flush().is_idle());
        scene.set_text(id, "hi");
        assert!(scene.flush().is_idle());
    }
    #[test]
    fn fixed_text_invalidates_only_its_paint() {
        let mut scene = Scene::new(800.0, 600.0);
        let a = scene.append(
            scene.root(),
            text("a"),
            Style {
                width: Some(200.0),
                height: Some(24.0),
                ..Style::default()
            },
        );
        scene.append(scene.root(), text("stable"), Style::default());
        scene.flush();
        scene.set_text(a, "stream");
        let frame = scene.flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 1);
        // Headless measurement is approximate: the whole box.
        assert_eq!(frame.damage, vec![Rect::new(0.0, 0.0, 200.0, 24.0)]);
    }
    #[test]
    fn transforms_damage_old_and_new_without_layout_or_paint() {
        let mut scene = Scene::new(800.0, 600.0);
        let id = scene.append(
            scene.root(),
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(20.0),
                height: Some(20.0),
                ..Style::default()
            },
        );
        scene.flush();
        scene.set_transform(id, Transform { x: 100.0, y: 0.0 });
        let frame = scene.flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 0);
        assert_eq!(frame.composite_nodes, 1);
        assert!(frame.damage.contains(&Rect::new(0.0, 0.0, 20.0, 20.0)));
        assert!(frame.damage.contains(&Rect::new(100.0, 0.0, 20.0, 20.0)));
    }
    #[test]
    fn reused_slots_do_not_resurrect_handles() {
        let mut scene = Scene::new(100.0, 100.0);
        let old = scene.append(scene.root(), text("old"), Style::default());
        scene.remove(old);
        let new = scene.append(scene.root(), text("new"), Style::default());
        assert!(!scene.contains(old));
        assert!(scene.contains(new));
        assert_ne!(old, new);
        scene.flush();
    }
    #[test]
    fn a_killed_subtree_leaves_at_once_and_its_slots_free_one_at_a_time() {
        let mut scene = Scene::new(100.0, 100.0);
        let page = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        let rows: Vec<_> = (0..4)
            .map(|i| scene.append(page, text(&format!("row {i}")), Style::default()))
            .collect();
        scene.set_isolated(rows[0], true);
        let kept = scene.append(scene.root(), text("kept"), Style::default());
        scene.flush();
        let before = scene.len();
        let ids = scene.kill(page);
        assert_eq!(ids.len(), 5);
        assert!(ids.iter().all(|id| !scene.contains(*id)));
        assert_eq!(scene.len(), before - 5);
        assert_eq!(scene.children(scene.root()), &[kept]);
        assert!(scene.isolated_nodes().all(|id| !ids.contains(&id)));
        // Layout and paint walk only what stays (a debug build asserts it).
        scene.set_text(kept, "kept, laid out again");
        scene.flush();
        assert!(scene.paint_items().all(|item| !ids.contains(&item.id)));
        // Its slots wait: a new node takes a fresh one.
        let fresh = scene.append(scene.root(), text("fresh"), Style::default());
        assert!(!ids.iter().any(|id| id.index == fresh.index));
        for id in &ids {
            scene.free_dead(*id);
        }
        assert_eq!(scene.len(), before - 5 + 1);
        let reused = scene.append(scene.root(), text("reused"), Style::default());
        assert!(ids.iter().any(|id| id.index == reused.index));
        assert!(ids.iter().all(|id| !scene.contains(*id)));
        scene.flush();
    }
    #[test]
    fn slots_give_back_doublings_room_once_they_stop_growing() {
        let mut scene = Scene::new(100.0, 100.0);
        let mut last = None;
        for i in 0..1100 {
            last = Some(scene.append(scene.root(), text(&format!("{i}")), Style::default()));
        }
        scene.flush();
        let grown = scene.slots.capacity();
        assert!(grown >= 2048, "doubling while it mounts: {grown}");
        // Frames while it settles keep the room; after them it goes.
        for _ in 0..59 {
            scene.set_text(last.unwrap(), "changed");
            scene.set_text(last.unwrap(), "again");
            scene.flush();
        }
        assert_eq!(scene.slots.capacity(), grown);
        for _ in 0..3 {
            scene.set_text(last.unwrap(), "changed");
            scene.flush();
        }
        let len = scene.slots.len();
        assert!(
            scene.slots.capacity() <= len + len / 4 + 64,
            "trimmed: {}",
            scene.slots.capacity()
        );
        // A scene that keeps growing is not trimmed between appends.
        let mut growing = Scene::new(100.0, 100.0);
        for i in 0..2000 {
            growing.append(growing.root(), text(&format!("{i}")), Style::default());
            growing.flush();
        }
        assert!(growing.slots.capacity() >= 2001);
    }
    #[test]
    fn opacity_inherits_and_clips_follow_transforms() {
        let mut scene = Scene::new(100.0, 100.0);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(20.0),
                height: Some(20.0),
                clip: true,
                ..Style::default()
            },
        );
        let child = scene.append(parent, text("hello"), Style::default());
        scene.flush();
        scene.set_effects(
            parent,
            Effects {
                opacity: 0.5,
                ..Effects::default()
            },
        );
        scene.set_effects(
            child,
            Effects {
                opacity: 0.5,
                ..Effects::default()
            },
        );
        scene.set_transform(parent, Transform { x: 10.0, y: 10.0 });
        scene.flush();
        let item = scene.paint_items().find(|i| i.id == child).unwrap();
        assert_eq!(item.effects.opacity, 0.25);
        assert_eq!(item.clip, Some(Rect::new(10.0, 10.0, 20.0, 20.0)));
    }
    #[test]
    fn backdrop_damage_expands_to_filter_output() {
        let mut scene = Scene::new(300.0, 300.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let a = scene.append(
            scene.root(),
            text("a"),
            Style {
                width: Some(20.0),
                height: Some(20.0),
                ..Style::default()
            },
        );
        let b = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 255, 255, 128)),
            Style {
                width: Some(100.0),
                height: Some(100.0),
                ..Style::default()
            },
        );
        scene.set_effects(
            b,
            Effects {
                blur_radius: 4.0,
                ..Effects::default()
            },
        );
        scene.flush();
        scene.set_text(a, "b");
        let frame = scene.flush();
        assert!(
            frame
                .damage
                .iter()
                .any(|r| r.width >= 100.0 && r.height >= 100.0)
        );
    }
    #[test]
    fn natural_text_layout_preserves_unaffected_subtrees() {
        let mut scene = Scene::new(500.0, 500.0);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let changed = scene.append(scene.root(), text("a"), Style::default());
        let stable = scene.append(scene.root(), text("stable"), Style::default());
        scene.set_transform(stable, Transform { x: 200.0, y: 200.0 });
        scene.flush();
        scene.set_text(changed, "longer");
        let frame = scene.flush();
        assert_eq!(frame.layout_nodes, 2); // Root and the changed leaf; stable is cached.
        assert_eq!(frame.paint_nodes, 1);
        assert!(
            frame
                .damage
                .iter()
                .all(|rect| !rect.intersects(scene.bounds(stable)))
        );
    }
    #[test]
    fn intrinsic_height_change_moves_and_damages_following_sibling() {
        let mut scene = Scene::new(500.0, 500.0);
        let first = scene.append(scene.root(), text("a"), Style::default());
        let second = scene.append(scene.root(), text("b"), Style::default());
        scene.flush();
        let old = scene.bounds(second);
        scene.set_text(first, "a\na");
        let frame = scene.flush();
        let new = scene.bounds(second);
        assert_eq!(old.y, 23.0);
        assert_eq!(new.y, 46.0);
        assert!(frame.damage.iter().any(|rect| rect.intersects(old)));
        assert!(frame.damage.iter().any(|rect| rect.intersects(new)));
        assert!(scene.flush().is_idle());
    }
    #[test]
    fn removing_a_subtree_clears_its_pixels_and_reclaims_every_node() {
        let mut scene = Scene::new(500.0, 500.0);
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        let child = scene.append(group, text("removed"), Style::default());
        scene.flush();
        let old = scene.bounds(child);
        scene.remove(group);
        let frame = scene.flush();
        assert_eq!(scene.len(), 1);
        assert!(!scene.contains(child));
        assert!(frame.damage.iter().any(|rect| rect.intersects(old)));
        assert!(scene.flush().is_idle());
    }
    #[test]
    fn parent_transform_damages_descendant_at_both_locations() {
        let mut scene = Scene::new(500.0, 500.0);
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        let child = scene.append(group, text("moving"), Style::default());
        scene.flush();
        let old = scene.bounds(child);
        scene.set_transform(group, Transform { x: 200.0, y: 200.0 });
        let new = scene.bounds(child);
        let frame = scene.flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 0);
        assert!(frame.damage.iter().any(|rect| rect.intersects(old)));
        assert!(frame.damage.iter().any(|rect| rect.intersects(new)));
    }
    #[test]
    fn expanding_clip_reveals_and_damages_previously_hidden_content() {
        let mut scene = Scene::new(500.0, 500.0);
        let mut style = Style {
            width: Some(20.0),
            height: Some(20.0),
            clip: true,
            ..Style::default()
        };
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            style.clone(),
        );
        scene.append(
            group,
            NodeKind::Rect(Color(255, 255, 255, 255)),
            Style {
                width: Some(100.0),
                height: Some(20.0),
                ..Style::default()
            },
        );
        scene.flush();
        style.clip = false;
        scene.set_style(group, style);
        let frame = scene.flush();
        assert!(frame.damage.iter().any(|rect| rect.x + rect.width >= 100.0));
    }
    #[test]
    fn resizing_clip_damages_newly_visible_pixels() {
        let mut scene = Scene::new(500.0, 500.0);
        let mut style = Style {
            width: Some(20.0),
            height: Some(20.0),
            clip: true,
            ..Style::default()
        };
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            style.clone(),
        );
        scene.append(
            group,
            NodeKind::Rect(Color(255, 255, 255, 255)),
            Style {
                width: Some(100.0),
                height: Some(20.0),
                ..Style::default()
            },
        );
        scene.flush();
        style.width = Some(100.0);
        scene.set_style(group, style);
        let frame = scene.flush();
        assert!(frame.damage.iter().any(|rect| rect.x + rect.width >= 100.0));
    }
    #[test]
    fn fractional_damage_is_rounded_to_cover_complete_pixels() {
        let mut scene = Scene::new(100.0, 100.0);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(100, 100, 100, 100)),
            Style {
                width: Some(10.0),
                height: Some(10.0),
                ..Style::default()
            },
        );
        scene.flush();
        scene.set_transform(node, Transform { x: 20.25, y: 20.75 });
        let frame = scene.flush();
        assert!(frame.damage.contains(&Rect::new(20.0, 20.0, 11.0, 11.0)));
        assert!(frame.damage.iter().all(|r| {
            [r.x, r.y, r.width, r.height]
                .into_iter()
                .all(|v| v.fract() == 0.0)
        }));
    }
}

#[cfg(test)]
mod measure_work_tests {
    use super::*;
    use crate::layout::{LayoutOptions, Length};
    /// A stack `depth` deep of layout boxes, each holding wrapping text, a
    /// fixed-width row and the next box, laid out by taffy (their children
    /// set layout options). The boxes of a few levels animate a negative
    /// bottom margin every frame (entering boxes growing open). As
    /// zgx-script lowered them once (`contents` false): an outer column
    /// and a middle one stretching their items, the margin on the middle;
    /// as it does now: a contents node, the margin its own, around the
    /// box. Returns the scene's measure calls per frame.
    fn measures_per_frame(depth: usize, contents: bool) -> u64 {
        let mut scene = Scene::new(600., 4000.);
        let shrinkable = Some(Arc::new(LayoutOptions {
            min_width: Some(Length::Px(0.)),
            ..Default::default()
        }));
        let column = |scene: &mut Scene, parent| {
            scene.append(
                parent,
                NodeKind::Container(Layout::Column),
                Style {
                    align: Align::Stretch,
                    layout_options: shrinkable.clone(),
                    ..Default::default()
                },
            )
        };
        let mut parent = scene.root();
        let mut animating = Vec::new();
        for level in 0..depth {
            let (slack, inner) = if contents {
                let slack = scene.append(
                    parent,
                    NodeKind::Container(Layout::Column),
                    Style::default(),
                );
                scene.set_contents(slack, true);
                (slack, column(&mut scene, slack))
            } else {
                let outer = column(&mut scene, parent);
                let middle = column(&mut scene, outer);
                (middle, middle)
            };
            scene.append(
                inner,
                NodeKind::Text {
                    text: format!("level {level} says something long enough to wrap").into(),
                    color: Color(255, 255, 255, 255),
                    font_size: 14.,
                },
                Style {
                    text_wrap: true,
                    layout_options: shrinkable.clone(),
                    ..Default::default()
                },
            );
            let row = scene.append(inner, NodeKind::Container(Layout::Row), Style::default());
            for _ in 0..3 {
                scene.append(
                    row,
                    NodeKind::Rect(Color(1, 2, 3, 255)),
                    Style {
                        width: Some(40.),
                        height: Some(10.),
                        ..Default::default()
                    },
                );
            }
            animating.push(slack);
            parent = inner;
        }
        scene.flush();
        let animated: Vec<_> = animating.iter().copied().step_by(4).collect();
        let before = scene.measure_calls;
        let frames = 12;
        for frame in 0..frames {
            for (i, id) in animated.iter().enumerate() {
                let mut style = scene.style(*id);
                style.margin.bottom = -((frames - frame) as f32) * 7. - i as f32;
                scene.set_style(*id, style);
            }
            scene.flush();
        }
        (scene.measure_calls - before) / frames as u64
    }
    #[test]
    fn a_fixed_box_that_shrank_grows_back_with_content_past_where_it_stopped() {
        // A 900-high box in a 300-high column shrinks to its content, no
        // further (min-height: auto); content growing past that grows it.
        let mut scene = Scene::new(400., 300.);
        let fixed = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                width: Some(380.),
                height: Some(900.),
                ..Default::default()
            },
        );
        let content = scene.append(
            fixed,
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(10.),
                height: Some(290.),
                ..Default::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(fixed).height, 300.);
        let mut style = scene.style(content);
        style.height = Some(310.);
        scene.set_style(content, style);
        scene.flush();
        assert_eq!(scene.bounds(fixed).height, 310.);
        // Unable to shrink, it is a boundary again: its own height.
        let mut style = scene.style(fixed);
        style.flex_shrink = 0.;
        scene.set_style(fixed, style);
        scene.flush();
        assert_eq!(scene.bounds(fixed).height, 900.);
    }
    #[test]
    fn arranging_children_asks_again_only_for_those_it_did_not_just_measure() {
        let mut scene = Scene::new(400., 300.);
        let card = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(300.),
                height: Some(200.),
                ..Default::default()
            },
        );
        let bars: Vec<_> = (0..20)
            .map(|i| {
                scene.append(
                    card,
                    NodeKind::Rect(Color(1, 2, 3, 255)),
                    Style {
                        absolute: true,
                        width: Some(4.),
                        height: Some(10.),
                        margin: Insets {
                            left: i as f32 * 6.,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
            })
            .collect();
        scene.flush();
        let before = scene.measure_calls;
        let mut style = scene.style(bars[3]);
        style.height = Some(30.);
        style.margin.top = 5.;
        scene.set_style(bars[3], style);
        scene.flush();
        // The card and its twenty bars, each once: arranging them finds the
        // results just made rather than asking for them again.
        assert_eq!(scene.measure_calls - before, 21);
        assert_eq!(scene.bounds(bars[3]), Rect::new(18., 5., 4., 30.));
        assert_eq!(scene.bounds(bars[4]), Rect::new(24., 0., 4., 10.));
    }
    /// Promoting a kept result leaves the results as taking it out and
    /// putting the current one first did: the same current, the same order
    /// after it, for every length, every hit and either kind of current.
    #[test]
    fn a_promoted_result_leaves_the_order_removing_and_inserting_did() {
        let made = |size: f32| Measurement {
            constraints: Constraints::default(),
            size: (size, 0.),
            children: Vec::new(),
            pass: 0,
        };
        let sizes = |list: &[Measurement]| list.iter().map(|m| m.size.0).collect::<Vec<_>>();
        for len in 1..=64 {
            for index in 0..len {
                for with_current in [true, false] {
                    let fresh = || {
                        let current = with_current.then(|| made(-1.));
                        let alternates: Vec<_> = (0..len).map(|i| made(i as f32)).collect();
                        (current, alternates)
                    };
                    let (mut old_current, mut old) = fresh();
                    let hit = old.remove(index);
                    let old_size = hit.size;
                    if let Some(previous) = old_current.replace(hit) {
                        old.insert(0, previous);
                    }
                    let (mut new_current, mut new) = fresh();
                    let new_size = promote_measurement(&mut new_current, &mut new, index);
                    assert_eq!(new_size, old_size, "{len} {index} {with_current}");
                    assert_eq!(
                        new_current.map(|m| m.size.0),
                        old_current.map(|m| m.size.0),
                        "{len} {index} {with_current}"
                    );
                    assert_eq!(sizes(&new), sizes(&old), "{len} {index} {with_current}");
                }
            }
        }
    }
    #[test]
    fn a_hidden_subtree_is_measured_once_to_no_box() {
        let mut scene = Scene::new(400., 300.);
        let column = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                width: Some(200.),
                ..Default::default()
            },
        );
        let shown = scene.append(
            column,
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(50.),
                height: Some(10.),
                ..Default::default()
            },
        );
        let hidden = scene.append(
            column,
            NodeKind::Container(Layout::Column),
            Style {
                layout_options: Some(Arc::new(LayoutOptions {
                    display: Some(crate::layout::Display::None),
                    ..Default::default()
                })),
                ..Default::default()
            },
        );
        let inner = scene.append(
            hidden,
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        let leaf = scene.append(
            inner,
            NodeKind::Rect(Color(4, 5, 6, 255)),
            Style {
                width: Some(5.),
                height: Some(5.),
                ..Default::default()
            },
        );
        scene.flush();
        for frame in 0..10 {
            let mut style = scene.style(shown);
            style.height = Some(11. + frame as f32);
            scene.set_style(shown, style);
            scene.flush();
            assert_eq!(
                scene.bounds(shown),
                Rect::new(0., 0., 50., 11. + frame as f32)
            );
        }
        // Measured once hidden, its one result serves every frame after:
        // none piles up beside it.
        for id in [inner, leaf] {
            assert!(
                scene.node(id).alternates.is_empty(),
                "{} kept beside the result",
                scene.node(id).alternates.len()
            );
            // No box, as CSS gives `display: none` content (not the 5 high
            // its own size made it at no width).
            let bounds = scene.bounds(id);
            assert_eq!((bounds.width, bounds.height), (0., 0.), "{id:?}");
            assert!(!scene.layout_visible(id));
        }
    }
    #[test]
    fn a_box_resized_frame_after_frame_lands_where_a_fresh_layout_puts_it() {
        let build = |height: f32| {
            let mut scene = Scene::new(200., 200.);
            let column = scene.append(
                scene.root(),
                NodeKind::Container(Layout::Column),
                Style {
                    width: Some(100.),
                    ..Default::default()
                },
            );
            let bar = scene.append(
                column,
                NodeKind::Container(Layout::Overlay),
                Style {
                    width: Some(10.),
                    height: Some(height),
                    margin: Insets {
                        top: 50. - height,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let below = scene.append(
                column,
                NodeKind::Rect(Color(1, 2, 3, 255)),
                Style {
                    width: Some(10.),
                    height: Some(5.),
                    ..Default::default()
                },
            );
            scene.flush();
            (scene, column, bar, below)
        };
        let (mut scene, column, bar, below) = build(10.);
        for height in [20., 35., 12., 48.] {
            let mut style = scene.style(bar);
            style.height = Some(height);
            style.margin.top = 50. - height;
            scene.set_style(bar, style);
            scene.flush();
            let (fresh, fresh_column, fresh_bar, fresh_below) = build(height);
            for (id, fresh_id) in [
                (column, fresh_column),
                (bar, fresh_bar),
                (below, fresh_below),
            ] {
                assert_eq!(scene.bounds(id), fresh.bounds(fresh_id), "at {height}");
            }
        }
    }
    #[test]
    fn nested_animated_margins_measure_their_paths_not_every_probe_combination() {
        let shallow = measures_per_frame(8, false);
        let deep = measures_per_frame(32, false);
        // Four times as deep, with four times the animated boxes: about
        // sixteen times the work on their paths at most, never the
        // exponential growth of each level re-probing the next.
        assert!(
            deep <= 20 * shallow,
            "{shallow} calls at depth 8, {deep} at 32"
        );
        // A handful of probes per node on a changed path and its children.
        let nodes = 32 * 7;
        assert!(deep < 40 * nodes as u64, "{deep} calls for {nodes} nodes");
    }
    #[test]
    fn contents_layout_boxes_measure_no_more_than_boxes_around_them() {
        for depth in [8, 32] {
            let (boxes, contents) = (
                measures_per_frame(depth, false),
                measures_per_frame(depth, true),
            );
            assert!(
                contents <= boxes,
                "depth {depth}: {contents} calls with contents nodes, {boxes} with boxes"
            );
        }
        let (shallow, deep) = (measures_per_frame(8, true), measures_per_frame(32, true));
        assert!(deep <= 20 * shallow, "{shallow} at depth 8, {deep} at 32");
    }
}
#[cfg(test)]
mod paint_geometry_tests {
    use super::*;
    #[test]
    fn rect_and_quad_paint_changes_preserve_layout() {
        let mut scene = Scene::new(200., 100.);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(100.),
                height: Some(40.),
                ..Style::default()
            },
        );
        scene.flush();
        let bounds = scene.bounds(node);
        for kind in [
            NodeKind::Rect(Color(0, 255, 0, 255)),
            NodeKind::Quad(QuadStyle {
                fill: Color(0, 0, 255, 255),
                radius: 8.,
                ..QuadStyle::default()
            }),
        ] {
            scene.set_kind(node, kind);
            let report = scene.flush();
            assert_eq!(report.layout_nodes, 0);
            assert!(!report.damage.is_empty());
            assert_eq!(scene.bounds(node), bounds);
        }
        assert!(scene.flush().is_idle());
    }
}

#[cfg(test)]
mod deferred_frame_tests {
    use super::*;
    #[test]
    fn unflushed_node_replacement_bounds_stale_dirty_generations() {
        let mut scene = Scene::new(100., 100.);
        scene.flush();
        for i in 0..100_000 {
            let node = scene.append(
                scene.root(),
                NodeKind::Rect(Color((i % 255) as u8, 0, 0, 255)),
                Style {
                    width: Some(10.),
                    height: Some(10.),
                    ..Style::default()
                },
            );
            scene.remove(node);
            assert!(scene.dirty.len() <= 64);
        }
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 255, 0, 255)),
            Style {
                width: Some(10.),
                height: Some(10.),
                ..Style::default()
            },
        );
        let report = scene.flush();
        assert!(!report.damage.is_empty());
        assert_eq!(scene.bounds(node).width, 10.);
        assert!(scene.flush().is_idle());
        scene.set_transform(node, Transform { x: 20., y: 0. });
        assert!(!scene.flush().damage.is_empty());
    }
}

#[cfg(test)]
mod absolute_layout_tests {
    use super::*;
    #[test]
    fn absolute_children_do_not_change_flow_intrinsic_size_or_gaps() {
        for layout in [Layout::Row, Layout::Column, Layout::Overlay] {
            let mut scene = Scene::new(500., 500.);
            let parent = scene.append(
                scene.root(),
                NodeKind::Container(layout),
                Style {
                    padding: 5.,
                    gap: 7.,
                    ..Style::default()
                },
            );
            let first = scene.append(
                parent,
                NodeKind::Rect(Color(255, 0, 0, 255)),
                Style {
                    width: Some(20.),
                    height: Some(30.),
                    ..Style::default()
                },
            );
            let floating = scene.append(
                parent,
                NodeKind::Rect(Color(0, 255, 0, 255)),
                Style {
                    absolute: true,
                    width: Some(200.),
                    height: Some(300.),
                    margin: Insets {
                        left: 3.,
                        top: 4.,
                        ..Insets::default()
                    },
                    flex_grow: 100.,
                    ..Style::default()
                },
            );
            let second = scene.append(
                parent,
                NodeKind::Rect(Color(0, 0, 255, 255)),
                Style {
                    width: Some(20.),
                    height: Some(30.),
                    ..Style::default()
                },
            );
            scene.flush();
            let expected = match layout {
                Layout::Row => (57., 40.),
                Layout::Column => (30., 77.),
                Layout::Overlay => (30., 40.),
            };
            assert_eq!(
                (scene.bounds(parent).width, scene.bounds(parent).height),
                expected
            );
            assert_eq!(scene.bounds(first), Rect::new(5., 5., 20., 30.));
            assert_eq!(scene.bounds(floating), Rect::new(8., 9., 200., 300.));
            let before = scene.bounds(second);
            scene.remove(floating);
            scene.flush();
            assert_eq!(scene.bounds(second), before);
            assert_eq!(
                (scene.bounds(parent).width, scene.bounds(parent).height),
                expected
            );
        }
    }
    #[test]
    fn absolute_text_remeasures_on_parent_resize_without_taking_flex_space() {
        let mut scene = Scene::new(200., 100.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Row));
        let sibling = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style {
                width: Some(20.),
                height: Some(10.),
                flex_grow: 1.,
                ..Style::default()
            },
        );
        let text = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "one two three four five six seven eight nine ten".into(),
                color: Color(255, 255, 255, 255),
                font_size: 16.,
            },
            Style {
                absolute: true,
                text_wrap: true,
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(sibling).width, 200.);
        let height = scene.bounds(text).height;
        scene.resize(80., 100.);
        scene.flush();
        assert_eq!(scene.bounds(sibling).width, 80.);
        assert!(scene.bounds(text).height > height);
    }
    /// A relative block holding a column holding an absolute box pinned to
    /// the block's bottom right; the block's measure count over a resize.
    fn escaping(relative: bool) -> (Scene, NodeId, NodeId, u64) {
        let mut scene = Scene::new(300., 200.);
        let block = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                relative,
                width: Some(200.),
                height: Some(100.),
                ..Style::default()
            },
        );
        let middle = scene.append(
            block,
            NodeKind::Container(Layout::Column),
            Style {
                padding: 5.,
                ..Style::default()
            },
        );
        let pinned = scene.append(
            middle,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style {
                absolute: true,
                width: Some(10.),
                height: Some(10.),
                layout_options: Some(Arc::new(crate::layout::LayoutOptions {
                    inset: crate::layout::EdgeLengths {
                        right: Some(crate::layout::Length::Px(0.)),
                        bottom: Some(crate::layout::Length::Px(0.)),
                        ..Default::default()
                    },
                    ..Default::default()
                })),
                ..Style::default()
            },
        );
        scene.flush();
        let before = scene.measure_calls;
        let mut style = scene.style(block);
        style.width = Some(250.);
        scene.set_style(block, style);
        scene.flush();
        let calls = scene.measure_calls - before;
        (scene, block, pinned, calls)
    }
    #[test]
    fn absolute_nodes_escape_to_a_relative_ancestor_only_when_there_is_one() {
        let (scene, block, pinned, plain_calls) = escaping(false);
        assert!(scene.escaped.is_empty(), "no relative node: no search");
        // Its parent places it: the padded middle column's top right.
        assert_eq!(scene.bounds(pinned), Rect::new(0., 0., 10., 10.));
        assert_eq!(scene.relative_count, 0);
        let (mut scene, _, pinned, calls) = escaping(true);
        assert_eq!(scene.escaped, vec![pinned]);
        assert_eq!(scene.bounds(pinned), Rect::new(240., 90., 10., 10.));
        // Placing it is a measure of its own, not a second pass of the rest.
        assert!(calls <= plain_calls + 1, "{calls} vs {plain_calls}");
        assert!(scene.flush().is_idle());
        // Unmarked, its parent places it again.
        let mut style = scene.style(block);
        style.relative = false;
        scene.set_style(block, style);
        scene.flush();
        assert_eq!(scene.bounds(pinned), Rect::new(0., 0., 10., 10.));
        assert!(scene.escaped.is_empty());
        assert_eq!(scene.relative_count, 0);
    }
    /// Every node holds a `Style`: the box-model flags (`relative`,
    /// `intrinsic_width`, `intrinsic_height`) live in its padding.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn style_stays_its_size() {
        assert_eq!(std::mem::size_of::<Style>(), 176);
    }
    #[test]
    fn layout_borders_relayout_when_their_width_changes_not_their_colour() {
        let mut scene = Scene::new(200., 200.);
        let panel = |width: f32, color: Color| NodeKind::Panel {
            layout: Layout::Column,
            quad: QuadStyle {
                border_width: width,
                border_color: color,
                ..QuadStyle::default()
            },
        };
        let frame = scene.append(
            scene.root(),
            panel(1., Color(255, 0, 0, 255)),
            Style::default(),
        );
        let child = scene.append(
            frame,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style {
                width: Some(10.),
                height: Some(10.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(frame), Rect::new(0., 0., 12., 12.));
        assert_eq!(scene.bounds(child), Rect::new(1., 1., 10., 10.));
        scene.set_kind(frame, panel(1., Color(0, 255, 0, 255)));
        assert_eq!(scene.flush().layout_nodes, 0);
        scene.set_kind(frame, panel(2., Color(0, 255, 0, 255)));
        assert!(scene.flush().layout_nodes > 0);
        assert_eq!(scene.bounds(child), Rect::new(2., 2., 10., 10.));
        // Painted over its content, it takes no room.
        let mut quad = QuadStyle {
            border_width: 2.,
            border_color: Color(0, 255, 0, 255),
            border_over: true,
            ..QuadStyle::default()
        };
        scene.set_kind(
            frame,
            NodeKind::Panel {
                layout: Layout::Column,
                quad: quad.clone(),
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(0., 0., 10., 10.));
        quad.border_over = false;
        quad.outline = Some(Outline {
            width: 3.,
            color: Color(0, 0, 255, 255),
            offset: 0.,
        });
        quad.border_width = 0.;
        scene.set_kind(
            frame,
            NodeKind::Panel {
                layout: Layout::Column,
                quad,
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(0., 0., 10., 10.));
    }
    #[test]
    fn geometry_revision_tracks_layout_and_transforms_but_not_equal_transforms() {
        let mut scene = Scene::new(100., 100.);
        let before = scene.geometry_revision();
        scene.prepare_layout();
        assert_ne!(scene.geometry_revision(), before);
        let before = scene.geometry_revision();
        scene.prepare_layout();
        assert_eq!(scene.geometry_revision(), before);
        scene.set_transform(scene.root(), Transform { x: 1., y: 2. });
        assert_ne!(scene.geometry_revision(), before);
        let before = scene.geometry_revision();
        scene.set_transform(scene.root(), Transform { x: 1., y: 2. });
        assert_eq!(scene.geometry_revision(), before);
    }
}

#[cfg(test)]
mod style_default_tests {
    use super::*;

    #[test]
    fn a_style_is_default_as_equality_says() {
        assert!(Style::default().is_default());
        let changes: [fn(&mut Style); 30] = [
            |s| s.layout_options = Some(Arc::new(Default::default())),
            |s| s.absolute = true,
            |s| s.relative = true,
            |s| s.intrinsic_width = true,
            |s| s.intrinsic_height = true,
            |s| s.width = Some(0.),
            |s| s.height = Some(0.),
            |s| s.width_percent = Some(0.),
            |s| s.height_percent = Some(0.),
            |s| s.padding = 1.,
            |s| s.padding_edges = Some(Insets::default()),
            |s| s.gap = 1.,
            |s| s.clip = true,
            |s| s.fade_edges = [0., 1.],
            |s| s.fade_insets = [1., 0.],
            |s| s.fade_edges_x = [0., 1.],
            |s| s.fade_insets_x = [1., 0.],
            |s| s.fade_overflow = true,
            |s| s.snap = true,
            |s| s.min_width = Some(0.),
            |s| s.max_width = Some(0.),
            |s| s.min_height = Some(0.),
            |s| s.max_height = Some(0.),
            |s| s.margin.left = 1.,
            |s| s.flex_grow = 1.,
            |s| s.flex_shrink = 0.,
            |s| s.align = Align::Center,
            |s| s.justify = Justify::Center,
            |s| s.text_wrap = true,
            |s| s.text_options.line_clamp = std::num::NonZeroU32::new(1),
        ];
        for (i, change) in changes.iter().enumerate() {
            let mut style = Style::default();
            change(&mut style);
            assert_ne!(style, Style::default(), "change {i}");
            assert!(!style.is_default(), "change {i}");
        }
    }
}

#[cfg(test)]
mod line_height_tests {
    use super::*;
    use crate::text_layout::{FallbackTextLayout, FontStyle, LineHeight, TextLayout};

    #[test]
    fn explicit_pitch_relayouts_once_and_native_font_callback_receives_metrics() {
        let mut scene = Scene::new(200., 200.);
        scene.set_font_text_shaper(
            |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
                Box::new(FallbackTextLayout::with_line_height(
                    text,
                    size,
                    width,
                    font.line_height,
                ))
            },
        );
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "one\ntwo\n".into(),
                font_size: 10.,
                color: Color(255, 255, 255, 255),
            },
            Style::default(),
        );
        scene.flush();
        assert_eq!(scene.bounds(node).height, 42.);
        let font = FontStyle {
            line_height: LineHeight::px(24.),
            ..FontStyle::default()
        };
        scene.set_font(node, font.clone());
        let changed = scene.flush();
        assert!(changed.layout_nodes > 0 && !changed.damage.is_empty());
        assert_eq!(scene.bounds(node).height, 72.);
        assert_eq!(
            scene
                .shape_text_with_font("a\nb", 10., None, &font)
                .caret(2)
                .y,
            24.
        );
        scene.set_font(node, font);
        assert!(scene.flush().is_idle());
    }

    #[test]
    fn fallback_measurement_matches_grapheme_shaping_for_normal_and_explicit_pitch() {
        let scene = Scene::new(100., 100.);
        for line_height in [LineHeight::NORMAL, LineHeight::px(3.), LineHeight::px(30.)] {
            let font = FontStyle {
                line_height,
                ..FontStyle::default()
            };
            let text = "e\u{301}👩‍💻\r\nx";
            let measured = scene.measure_text_with_font(text, 10., Some(6.), &font);
            let shaped = scene.shape_text_with_font(text, 10., Some(6.), &font);
            assert_eq!(measured, shaped.size());
            assert_eq!(measured.1, 3. * line_height.resolve(10.));
        }
    }
}

#[cfg(test)]
mod percentage_layout_tests {
    use super::*;

    fn rect() -> NodeKind {
        NodeKind::Rect(Color(255, 0, 0, 255))
    }

    #[test]
    fn percentages_follow_definite_padded_parent_and_resize_without_idle_work() {
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width_percent: Some(0.5),
                height_percent: Some(1.),
                padding: 10.,
                align: Align::Stretch,
                ..Style::default()
            },
        );
        let child = scene.append(
            parent,
            rect(),
            Style {
                width_percent: Some(0.5),
                height_percent: Some(0.5),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(10., 10., 90., 140.));
        scene.resize(600., 400.);
        scene.flush();
        assert_eq!(scene.bounds(child), Rect::new(10., 10., 140., 190.));
        assert!(scene.flush().is_idle());
        let unchanged = scene.style(child);
        scene.set_style(child, unchanged);
        assert!(scene.flush().is_idle());
    }

    #[test]
    fn percentages_supply_flex_bases_and_forced_allocation_supplies_descendant_basis() {
        let mut scene = Scene::new(400., 200.);
        let row = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Row),
            Style {
                width: Some(300.),
                height: Some(100.),
                ..Style::default()
            },
        );
        let first = scene.append(
            row,
            NodeKind::Container(Layout::Overlay),
            Style {
                width_percent: Some(0.75),
                height_percent: Some(1.),
                flex_shrink: 1.,
                ..Style::default()
            },
        );
        let second = scene.append(
            row,
            rect(),
            Style {
                width_percent: Some(0.75),
                height_percent: Some(1.),
                flex_shrink: 1.,
                ..Style::default()
            },
        );
        let nested = scene.append(
            first,
            rect(),
            Style {
                width_percent: Some(0.5),
                height_percent: Some(1.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(first).width, 150.);
        assert_eq!(scene.bounds(second).width, 150.);
        assert_eq!(scene.bounds(nested).width, 75.);
        assert_eq!(scene.bounds(nested).height, 100.);
    }

    #[test]
    fn indefinite_percentages_use_intrinsic_size_and_absolute_uses_final_content_box() {
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                padding: 10.,
                ..Style::default()
            },
        );
        let child = scene.append(
            parent,
            NodeKind::Container(Layout::Overlay),
            Style {
                width_percent: Some(0.5),
                height_percent: Some(1.),
                ..Style::default()
            },
        );
        scene.append(
            child,
            rect(),
            Style {
                width: Some(80.),
                height: Some(40.),
                ..Style::default()
            },
        );
        let absolute = scene.append(
            parent,
            rect(),
            Style {
                absolute: true,
                width_percent: Some(0.5),
                height_percent: Some(0.5),
                margin: Insets::all(3.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(parent).width, 100.);
        assert_eq!(scene.bounds(parent).height, 60.);
        assert_eq!(scene.bounds(child).width, 80.);
        assert_eq!(scene.bounds(child).height, 40.);
        assert_eq!(scene.bounds(absolute), Rect::new(13., 13., 40., 20.));
        assert!(scene.flush().is_idle());
    }

    #[test]
    fn indefinite_percentage_uses_auto_cross_axis_stretch() {
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                align: Align::Stretch,
                ..Style::default()
            },
        );
        scene.append(
            parent,
            rect(),
            Style {
                width: Some(200.),
                height: Some(20.),
                ..Style::default()
            },
        );
        let child = scene.append(
            parent,
            NodeKind::Container(Layout::Overlay),
            Style {
                width_percent: Some(0.5),
                ..Style::default()
            },
        );
        let nested = scene.append(
            child,
            rect(),
            Style {
                width: Some(80.),
                height: Some(20.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(parent).width, 200.);
        assert_eq!(scene.bounds(child).width, 200.);
        assert_eq!(scene.bounds(nested).width, 80.);
        assert!(scene.flush().is_idle());
    }

    #[test]
    fn growing_row_provides_definite_height_for_percentage_container() {
        let mut scene = Scene::new(640., 420.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style {
                width_percent: Some(1.),
                height_percent: Some(1.),
                padding: 20.,
                gap: 12.,
                ..Style::default()
            },
        );
        scene.append(
            parent,
            rect(),
            Style {
                width: Some(120.),
                height: Some(24.),
                ..Style::default()
            },
        );
        let row = scene.append(
            parent,
            NodeKind::Container(Layout::Row),
            Style {
                width_percent: Some(1.),
                flex_grow: 1.,
                ..Style::default()
            },
        );
        let right = scene.append(
            row,
            NodeKind::Container(Layout::Column),
            Style {
                width_percent: Some(0.5),
                height_percent: Some(1.),
                padding: 12.,
                ..Style::default()
            },
        );
        scene.append(
            right,
            rect(),
            Style {
                width: Some(80.),
                height: Some(74.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(row).height, 344.);
        assert_eq!(scene.bounds(right).height, 344.);
        for _ in 0..3 {
            scene.invalidate_layout(row);
            scene.prepare_layout();
            assert_eq!(scene.bounds(right).height, 344.);
        }
    }

    #[test]
    fn zero_delta_flex_allocation_is_definite_for_percentage_descendants() {
        let mut scene = Scene::new(200., 100.);
        let row = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Row),
            Style {
                flex_grow: 1.,
                ..Style::default()
            },
        );
        scene.append(
            row,
            rect(),
            Style {
                width: Some(50.),
                height: Some(100.),
                ..Style::default()
            },
        );
        let percent = scene.append(
            row,
            rect(),
            Style {
                width: Some(50.),
                height_percent: Some(1.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(row).height, 100.);
        assert_eq!(scene.bounds(percent).height, 100.);
        assert!(scene.flush().is_idle());
    }

    #[test]
    fn percentage_constraints_precedence_and_invalid_values_are_finite() {
        let mut scene = Scene::new(200., 100.);
        let child = scene.append(
            scene.root(),
            rect(),
            Style {
                width_percent: Some(2.),
                height_percent: Some(f32::INFINITY),
                max_width: Some(120.),
                min_height: Some(5.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child).width, 120.);
        assert_eq!(scene.bounds(child).height, 5.);
        scene.set_style(
            child,
            Style {
                width: Some(37.),
                width_percent: Some(0.5),
                height_percent: Some(-1.),
                ..Style::default()
            },
        );
        scene.flush();
        assert_eq!(scene.bounds(child).width, 37.);
        assert_eq!(scene.bounds(child).height, 0.);
    }
}

#[cfg(test)]
mod letter_spacing_fallback_tests {
    use super::*;
    use crate::text_layout::{FontStyle, LetterSpacing, TextLayout};
    #[test]
    fn explicit_spacing_bypasses_legacy_callbacks_without_losing_metrics() {
        let mut scene = Scene::new(200., 100.);
        scene.set_text_shaper(|_: &str, _, _| -> Box<dyn TextLayout> {
            panic!("legacy callback cannot receive nonzero letter spacing")
        });
        let font = FontStyle {
            letter_spacing: LetterSpacing::px(4.),
            ..FontStyle::default()
        };
        let measured = scene.measure_text_with_font("abc", 10., Some(20.), &font);
        let shaped = scene.shape_text_with_font("abc", 10., Some(20.), &font);
        assert_eq!(measured, (20., 28.));
        assert_eq!(measured, shaped.size());
    }
}

#[cfg(test)]
mod image_replacement_layout_tests {
    use super::*;
    use crate::image::ImageData;
    use std::sync::Arc;
    fn pixels(w: u32, h: u32) -> NodeKind {
        NodeKind::Image(Arc::new(
            ImageData::new(w, h, [0, 0, 0, 255].repeat((w * h) as usize)).unwrap(),
        ))
    }
    #[test]
    fn fixed_axes_ignore_source_size_even_zero_and_constrained_but_auto_axes_remeasure() {
        for style in [
            Style {
                width: Some(0.),
                height: Some(0.),
                ..Default::default()
            },
            Style {
                width: Some(80.),
                height: Some(70.),
                max_width: Some(30.),
                min_height: Some(100.),
                width_percent: Some(0.5),
                ..Default::default()
            },
        ] {
            let mut scene = Scene::new(200., 200.);
            let image = scene.append(scene.root(), pixels(20, 10), style.clone());
            scene.flush();
            let before = scene.bounds(image);
            scene.set_kind(image, pixels(10, 20));
            let report = scene.flush();
            assert_eq!(report.layout_nodes, 0);
            assert_eq!(scene.bounds(image), before);
        }
        for style in [
            Style::default(),
            Style {
                width: Some(40.),
                ..Default::default()
            },
            Style {
                width_percent: Some(0.5),
                height_percent: Some(0.5),
                ..Default::default()
            },
        ] {
            let mut scene = Scene::new(200., 200.);
            let image = scene.append(scene.root(), pixels(20, 10), style.clone());
            scene.flush();
            scene.set_kind(image, pixels(10, 20));
            assert!(
                scene.flush().layout_nodes > 0,
                "non-concrete axes use existing invalidation"
            );
            if style.height.is_none() && style.height_percent.is_none() {
                assert_eq!(scene.bounds(image).height, 20.);
            }
        }
    }
}

#[cfg(test)]
mod hidden_layer_tests {
    use super::*;
    #[test]
    fn hidden_huge_descendants_do_not_expand_visible_layer_allocation() {
        let mut scene = Scene::new(300., 200.);
        let root = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(80.),
                height: Some(60.),
                ..Default::default()
            },
        );
        scene.set_isolated(root, true);
        let hidden = scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            Style {
                absolute: true,
                width: Some(1.),
                height: Some(1.),
                ..Default::default()
            },
        );
        let huge = scene.append(
            hidden,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(10000.),
                height: Some(12000.),
                ..Default::default()
            },
        );
        scene.set_effects(
            hidden,
            Effects {
                opacity: 0.,
                ..Default::default()
            },
        );
        scene.flush();
        assert_eq!(scene.layer_bounds(root), scene.bounds(root));
        assert_eq!(
            scene
                .layer_items(Some(root))
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![root]
        );
        assert!(
            scene.paint_items().any(|p| p.id == huge),
            "public traversal retains cache liveness"
        );
        scene.set_effects(hidden, Effects::default());
        scene.flush();
        assert_eq!(scene.layer_bounds(root).width, 10000.);
        assert_eq!(scene.layer_bounds(root).height, 12000.);
    }

    #[test]
    fn hidden_nested_layers_prune_but_explicit_layer_root_opacity_stays_external() {
        let mut scene = Scene::new(300., 200.);
        let root = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(80.),
                height: Some(60.),
                ..Default::default()
            },
        );
        scene.set_isolated(root, true);
        let nested = scene.append(
            root,
            NodeKind::Rect(Color(0, 255, 0, 255)),
            Style {
                absolute: true,
                width: Some(10000.),
                height: Some(12000.),
                ..Default::default()
            },
        );
        scene.set_isolated(nested, true);
        scene.set_effects(
            nested,
            Effects {
                opacity: 0.,
                ..Default::default()
            },
        );
        scene.flush();
        assert_eq!(scene.layer_bounds(root), scene.bounds(root));
        assert!(!scene.layer_items(Some(root)).iter().any(|p| p.id == nested));
        assert!(scene.isolated_nodes().any(|id| id == nested));
        scene.set_effects(
            root,
            Effects {
                opacity: 0.,
                ..Default::default()
            },
        );
        scene.flush();
        assert!(!scene.layer_items(None).iter().any(|p| p.id == root));
        let cached = scene.layer_items(Some(root));
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].effects.opacity, 1.);
        assert_eq!(scene.layer_bounds(root), scene.bounds(root));
        assert_eq!(scene.layer_items(Some(nested))[0].effects.opacity, 1.);
    }
}

#[cfg(test)]
mod nonfinite_effect_tests {
    use super::*;
    #[test]
    fn invalid_effects_normalize_before_equality_and_paint_traversal() {
        let mut scene = Scene::new(100., 100.);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(50.),
                height: Some(50.),
                ..Default::default()
            },
        );
        scene.flush();
        let invalid = Effects {
            opacity: f32::NAN,
            blur_radius: f32::INFINITY,
            edge_fade: f32::NEG_INFINITY,
            filter_blur: f32::NAN,
            brightness: 1.,
            contrast: 1.,
            saturate: 1.,
        };
        scene.set_effects(node, invalid);
        assert_eq!(scene.effects(node), Effects::default());
        assert!(scene.flush().is_idle());
        for _ in 0..3 {
            scene.set_effects(node, invalid);
            assert!(scene.flush().is_idle());
        }
        assert!(scene.paint_items().all(|p| p.effects.opacity.is_finite()
            && p.effects.blur_radius.is_finite()
            && p.effects.edge_fade.is_finite()
            && p.effects.filter_blur.is_finite()));
        assert!(scene.blur_nodes.is_empty());
        assert_eq!(scene.filtered.get(), 0);
    }
    #[test]
    fn opacity_infinities_keep_clamped_endpoints_and_invalid_filters_remove_blur_tracking() {
        let mut scene = Scene::new(100., 100.);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(50.),
                height: Some(50.),
                ..Default::default()
            },
        );
        scene.set_effects(
            node,
            Effects {
                opacity: 0.5,
                blur_radius: 4.,
                edge_fade: 3.,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        );
        scene.flush();
        assert_eq!(scene.blur_nodes, vec![node]);
        scene.set_effects(
            node,
            Effects {
                opacity: f32::NEG_INFINITY,
                blur_radius: f32::NAN,
                edge_fade: f32::INFINITY,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        );
        assert_eq!(
            scene.effects(node),
            Effects {
                opacity: 0.,
                ..Default::default()
            }
        );
        assert!(scene.blur_nodes.is_empty());
        assert!(!scene.layer_items(None).iter().any(|p| p.id == node));
        scene.flush();
        scene.set_effects(
            node,
            Effects {
                opacity: f32::INFINITY,
                blur_radius: -2.,
                edge_fade: -0.,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        );
        assert_eq!(scene.effects(node), Effects::default());
        assert_eq!(scene.effects(node).edge_fade.to_bits(), 0);
        assert!(!scene.flush().damage.is_empty());
        scene.set_effects(
            node,
            Effects {
                opacity: 2.,
                blur_radius: f32::NEG_INFINITY,
                edge_fade: f32::NAN,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        );
        assert!(scene.flush().is_idle());
    }
}

#[cfg(test)]
mod nonfinite_transform_tests {
    use super::*;
    #[test]
    fn invalid_axes_normalize_independently_before_equality_and_damage() {
        let mut scene = Scene::new(200., 150.);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(30.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.flush();
        scene.set_transform(node, Transform { x: 40., y: 25. });
        scene.flush();
        assert_eq!(scene.hit_test(45., 30.), Some(node));
        scene.set_transform(
            node,
            Transform {
                x: f32::NAN,
                y: 25.,
            },
        );
        assert_eq!(scene.transform(node), Transform { x: 0., y: 25. });
        let report = scene.flush();
        assert_eq!(report.layout_nodes, 0);
        assert!(!report.damage.is_empty());
        assert!(report.damage.iter().all(|r| r.x.is_finite()
            && r.y.is_finite()
            && r.width.is_finite()
            && r.height.is_finite()));
        assert_eq!(scene.hit_test(5., 30.), Some(node));
        assert_ne!(scene.hit_test(45., 30.), Some(node));
        let revision = scene.geometry_revision;
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0., 0.] {
            scene.set_transform(node, Transform { x, y: 25. });
            assert_eq!(scene.transform(node).x.to_bits(), 0);
            assert!(scene.flush().is_idle());
            assert_eq!(scene.geometry_revision, revision);
        }
        scene.set_transform(
            node,
            Transform {
                x: 12.,
                y: f32::INFINITY,
            },
        );
        assert_eq!(scene.transform(node), Transform { x: 12., y: 0. });
        assert_eq!(scene.hit_test(15., 5.), Some(node));
        assert!(!scene.flush().damage.is_empty());
        scene.set_transform(node, Transform { x: 40., y: 25. });
        assert_eq!(scene.hit_test(45., 30.), Some(node));
        assert!(!scene.flush().damage.is_empty());
    }
}

#[path = "scene_layout.rs"]
mod advanced_layout;
#[path = "scene_select.rs"]
mod select;
pub use select::{TextHighlight, TextLeaf, TextMark};
#[path = "scene_view.rs"]
mod view;
pub use view::{CanvasView, GridFill, GridMark};

/// `set_transform`'s translation: each non-finite coordinate is zero. (The
/// setters keep their own copies of these and of the bookkeeping: calling
/// shared ones cost each of their calls about 1% more instructions.)
fn finite_translation(mut transform: Transform) -> Transform {
    let coordinate = |value: f32| {
        if value.is_finite() && value != 0.0 {
            value
        } else {
            0.0
        }
    };
    transform.x = coordinate(transform.x);
    transform.y = coordinate(transform.y);
    transform
}
/// NaN opacity is opaque; infinite opacity clamps to its endpoint.
#[inline(always)]
pub(crate) fn finite_opacity(value: f32) -> f32 {
    if value.is_nan() {
        1.0
    } else {
        value.clamp(0.0, 1.0).max(0.0)
    }
}
/// Nonfinite and nonpositive radii disable their filter.
#[inline(always)]
pub(crate) fn finite_radius(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}
/// Nonfinite colour factors change nothing; negative factors clamp to zero.
#[inline(always)]
pub(crate) fn finite_factor(value: f32) -> f32 {
    if value.is_finite() { value.max(0.) } else { 1. }
}
/// `set_effects`'s effects, made finite.
fn finite_effects(mut effects: Effects) -> Effects {
    effects.opacity = finite_opacity(effects.opacity);
    effects.blur_radius = finite_radius(effects.blur_radius);
    effects.edge_fade = finite_radius(effects.edge_fade);
    effects.filter_blur = finite_radius(effects.filter_blur);
    effects.brightness = finite_factor(effects.brightness);
    effects.contrast = finite_factor(effects.contrast);
    effects.saturate = finite_factor(effects.saturate);
    effects
}
/// The indices of a longest strictly increasing subsequence of `values`
/// (patience sorting, n log n).
// Apart: inlined into the reorder that calls it, its read-back loop cost
// the larger reorders more than it saved.
#[inline(never)]
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // `tails[k]`: the index of the smallest last value of an increasing run
    // of length k + 1; `previous`: each index's predecessor in its run.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![usize::MAX; values.len()];
    for (index, value) in values.iter().enumerate() {
        let at = tails.partition_point(|&tail| values[tail] < *value);
        if at > 0 {
            previous[index] = tails[at - 1];
        }
        if at == tails.len() {
            tails.push(index);
        } else {
            tails[at] = index;
        }
    }
    // The search is finished: its tails buffer can hold the selected
    // indices too, in the same reverse predecessor order as before.
    let mut index = tails.last().copied().unwrap_or(usize::MAX);
    for tail in &mut tails {
        *tail = index;
        index = previous[index];
    }
    tails
}
fn own_visible(style: &Style) -> bool {
    style.layout_options.as_deref().is_none_or(|options| {
        options.visible != Some(false) && options.display != Some(crate::layout::Display::None)
    })
}

#[cfg(test)]
mod set_box_tests {
    use super::*;
    /// Two scenes, one restyled by `set_style` and one by `set_box`: the
    /// same styles, layout, damage and boxes.
    fn both(change: impl Fn(&mut Style)) {
        let style = Style {
            width: Some(40.),
            height: Some(10.),
            padding: 2.,
            ..Default::default()
        };
        let mut scenes = [Scene::new(200., 200.), Scene::new(200., 200.)];
        let ids = scenes.each_mut().map(|scene| {
            let column = scene.append(
                scene.root(),
                NodeKind::Container(Layout::Column),
                Default::default(),
            );
            let bar = scene.append(column, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
            let after = scene.append(column, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
            scene.flush();
            (bar, after)
        });
        let mut next = style.clone();
        change(&mut next);
        scenes[0].set_style(ids[0].0, next.clone());
        scenes[1].set_box(ids[1].0, &next, &mut None);
        let laid = scenes.each_mut().map(|scene| scene.flush());
        assert_eq!(laid[0].layout_nodes, laid[1].layout_nodes);
        assert_eq!(laid[0].damage, laid[1].damage);
        let [by_style, by_box] = [0, 1].map(|i| {
            let (scene, (bar, after)) = (&scenes[i], ids[i]);
            (scene.style(bar), scene.bounds(bar), scene.bounds(after))
        });
        assert_eq!(by_style, by_box);
        assert_eq!(by_box.0, next);
    }
    #[test]
    fn a_box_resized_by_set_box_lays_out_as_by_set_style() {
        both(|style| style.height = Some(25.));
        both(|style| {
            style.margin = Insets {
                top: 3.,
                left: 4.,
                ..Default::default()
            }
        });
    }
    #[test]
    fn a_box_given_or_losing_a_size_by_set_box_lays_out_as_by_set_style() {
        both(|style| style.height = None);
        both(|style| {
            style.width = None;
            style.width_percent = Some(50.);
        });
    }
    #[test]
    fn a_flush_names_paint_alone_and_box_alone_and_demotes_any_other_change() {
        let sized = |height: f32| Style {
            width: Some(40.),
            height: Some(height),
            ..Default::default()
        };
        let text = |value: &str, color: Color| NodeKind::Text {
            text: value.into(),
            color,
            font_size: 16.,
        };
        let (red, blue, green) = (
            Color(255, 0, 0, 255),
            Color(0, 0, 255, 255),
            Color(0, 255, 0, 255),
        );
        let mut scene = Scene::new(400., 300.);
        let root = scene.root();
        let column = scene.append(root, NodeKind::Container(Layout::Column), Style::default());
        // Above `grows`, the first two keep their places as it grows.
        let [recolours, retexts, grows, follows, both] =
            [(); 5].map(|_| scene.append(column, NodeKind::Rect(red), sized(20.)));
        scene.set_kind(retexts, text("one", red));
        scene.flush();
        let _ = scene.repaints();
        let flags = |scene: &Scene, id| {
            scene
                .repaints()
                .1
                .iter()
                .find(|(entry, _)| *entry == id)
                .map_or(0, |(_, flags)| *flags)
        };
        // Colour alone, twice: paint only.
        scene.set_kind(recolours, NodeKind::Rect(blue));
        scene.set_kind(recolours, NodeKind::Rect(green));
        // Colour and text in one frame: a plain repaint.
        scene.set_kind(retexts, text("one", blue));
        scene.set_kind(retexts, text("two", blue));
        // `grows` grows by its box alone (`set_box`): it and the leaves it
        // moves are box only, unless something else changed about them too.
        let mut kept = None;
        scene.set_box(grows, &sized(30.), &mut kept);
        scene.set_kind(both, NodeKind::Rect(blue));
        scene.flush();
        let paint = |f: u8| f & repaint::PAINT_ONLY != 0;
        let moved = |f: u8| f & repaint::BOX_ONLY != 0;
        let [g, f, r, t, b] =
            [grows, follows, recolours, retexts, both].map(|id| flags(&scene, id));
        assert!(paint(r) && !moved(r), "{r:#b}");
        assert!(!paint(t) && t & repaint::PAINT != 0, "{t:#b}");
        assert!(moved(g) && !paint(g), "{g:#b}");
        assert!(moved(f) && !paint(f), "{f:#b}");
        // Moved and recoloured: neither.
        assert!(b & repaint::MOVED != 0 && !moved(b) && !paint(b), "{b:#b}");
        // Another style change than its box while it moves, one that lays it
        // out again and repaints nothing itself (as a visibility change does):
        // not box only.
        let _ = scene.repaints();
        scene.set_style(
            grows,
            Style {
                relative: true,
                ..sized(40.)
            },
        );
        scene.flush();
        let g = flags(&scene, grows);
        assert!(!moved(g), "{g:#b}");
    }
    #[test]
    fn set_box_with_any_other_change_restyles_in_full() {
        both(|style| {
            style.height = Some(25.);
            style.padding = 6.;
        });
    }
    #[test]
    fn set_box_weighs_a_style_again_once_another_write_changed_it() {
        let style = Style {
            width: Some(40.),
            height: Some(10.),
            ..Default::default()
        };
        let mut scene = Scene::new(200., 200.);
        let column = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Default::default(),
        );
        let bar = scene.append(column, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
        scene.flush();
        // A caller growing the bar keeps what set_box left...
        let mut kept = None;
        let mut grown = style.clone();
        grown.height = Some(20.);
        scene.set_box(bar, &grown, &mut kept);
        assert!(kept.is_some());
        // ...and another write since changes more than the box: the next
        // grow restores the caller's style whole, as set_style would.
        let mut padded = grown.clone();
        padded.padding = 6.;
        scene.set_style(bar, padded);
        grown.height = Some(30.);
        scene.set_box(bar, &grown, &mut kept);
        assert_eq!(scene.style(bar), grown);
        // Unchanged since, the next grow is a box alone again.
        grown.height = Some(35.);
        scene.set_box(bar, &grown, &mut kept);
        let mut again = grown.clone();
        again.height = Some(40.);
        scene.set_box(bar, &again, &mut kept);
        assert_eq!(scene.style(bar), again);
        assert_eq!(kept, Some(scene.node(bar).style_writes));
    }
}
#[cfg(test)]
mod grid_walk_tests {
    use super::*;

    /// A layout walk starting with the damage grid active gives it moved
    /// leaves' damage as cells (`arrange::<true>`): the same damage as list
    /// mode's walk (`arrange::<false>`) gives it as rounded rectangles, with leaves
    /// at fractional places in a viewport of a fractional width.
    #[test]
    fn grid_and_list_walks_damage_alike() {
        for (first, then) in [(0, 5), (3, 40), (10, 11)] {
            let mut scenes = [Scene::new(200.5, 4_000.25), Scene::new(200.5, 4_000.25)];
            scenes[1].cells_off = true;
            let laid = scenes.each_mut().map(|scene| {
                // Its own size fixed: a row growing moves the rows after it,
                // as leaves, not the column.
                let column = scene.append(
                    scene.root(),
                    NodeKind::Container(Layout::Column),
                    Style {
                        width: Some(200.),
                        height: Some(3_900.),
                        ..Default::default()
                    },
                );
                let rows: Vec<_> = (0..120)
                    .map(|i| {
                        let style = Style {
                            width: Some(20. + (i % 7) as f32 * 13.3),
                            height: Some(10.3 + (i % 3) as f32 * 0.35),
                            // Apart: each moved row's damage is its own.
                            margin: Insets {
                                top: 20.,
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        scene.append(column, NodeKind::Rect(Color(1, 2, 3, 255)), style)
                    })
                    .collect();
                scene.flush();
                let grow = |scene: &mut Scene, id: NodeId, height: f32| {
                    let mut style = scene.style(id).clone();
                    style.height = Some(height);
                    scene.set_style(id, style);
                };
                // More moved leaves than the list holds: the grid takes over.
                grow(scene, rows[first], 17.6);
                scene.prepare_layout();
                assert!(scene.damage_grid.is_some());
                // This walk starts with the grid active.
                grow(scene, rows[then], 3.45);
                scene.prepare_layout();
                let report = scene.flush();
                let bounds: Vec<_> = rows.iter().map(|id| scene.bounds(*id)).collect();
                (report.damage, report.layout_nodes, bounds)
            });
            assert_eq!(laid[0], laid[1], "rows {first} then {then}");
        }
    }
}
#[cfg(test)]
mod damage_grid_tests {
    use super::*;
    /// A grid of one flag per cell, as `DamageGrid` was.
    struct Reference {
        columns: usize,
        rows: usize,
        cells: Vec<bool>,
    }
    impl Reference {
        fn span(&self, rect: Rect) -> (usize, usize, usize, usize) {
            let first = |value: f32| (value / DAMAGE_CELL).floor().max(0.) as usize;
            let end = |value: f32, count: usize| {
                ((value / DAMAGE_CELL).ceil().max(0.) as usize).min(count)
            };
            (
                first(rect.x).min(self.columns),
                first(rect.y).min(self.rows),
                end(rect.x + rect.width, self.columns),
                end(rect.y + rect.height, self.rows),
            )
        }
        fn mark(&mut self, rect: Rect) {
            let (left, top, right, bottom) = self.span(rect);
            for row in top..bottom {
                for cell in &mut self.cells[row * self.columns + left..row * self.columns + right] {
                    *cell = true;
                }
            }
        }
        fn touches(&self, rect: Rect) -> bool {
            let (left, top, right, bottom) = self.span(rect);
            (top..bottom).any(|row| {
                self.cells[row * self.columns + left..row * self.columns + right].contains(&true)
            })
        }
        fn rects(&self, viewport: Rect) -> Vec<Rect> {
            let mut done = Vec::new();
            let (mut open, mut next) = (Vec::<(usize, usize, usize)>::new(), Vec::new());
            for row in 0..=self.rows {
                let cells = if row < self.rows {
                    &self.cells[row * self.columns..(row + 1) * self.columns]
                } else {
                    &[][..]
                };
                let mut column = 0;
                while column < cells.len() {
                    if !cells[column] {
                        column += 1;
                        continue;
                    }
                    let start = column;
                    while column < cells.len() && cells[column] {
                        column += 1;
                    }
                    let top = open
                        .iter()
                        .find(|(left, right, _)| (*left, *right) == (start, column))
                        .map_or(row, |run| run.2);
                    next.push((start, column, top));
                }
                for &(left, right, top) in &open {
                    if !next.iter().any(|run| (run.0, run.1) == (left, right)) {
                        let cell = |index: usize| index as f32 * DAMAGE_CELL;
                        let rect =
                            Rect::new(cell(left), cell(top), cell(right - left), cell(row - top));
                        done.extend(rect.intersection(viewport));
                    }
                }
                std::mem::swap(&mut open, &mut next);
                next.clear();
            }
            done
        }
    }
    #[test]
    fn a_grid_of_bits_marks_touches_and_joins_cells_as_one_of_flags() {
        // Wider than two words of cells, and not a whole number of them.
        let viewport = Rect::new(0., 0., 1100., 300.);
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut random = |limit: f32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f32 / 10_000. * limit
        };
        for round in 0..40 {
            let mut grid = DamageGrid::new(viewport).unwrap();
            let mut reference = Reference {
                columns: grid.columns,
                rows: grid.rows,
                cells: vec![false; grid.columns * grid.rows],
            };
            for _ in 0..round * 3 {
                let (x, y) = (random(1150.) - 25., random(320.) - 10.);
                let rect = Rect::new(x, y, random(600.), random(80.));
                grid.mark(rect);
                reference.mark(rect);
                let probe = Rect::new(random(1100.), random(300.), random(90.), random(30.));
                assert_eq!(grid.touches(probe), reference.touches(probe));
            }
            let marked = reference.cells.iter().filter(|cell| **cell).count();
            let padding = (grid.stride * 64 - grid.columns) * grid.rows;
            let bits: usize = grid
                .cells
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum();
            assert_eq!(bits - padding, marked);
            assert_eq!(grid.full(), marked == reference.cells.len());
            assert_eq!(grid.rects(viewport), reference.rects(viewport));
        }
        let mut grid = DamageGrid::new(viewport).unwrap();
        grid.mark(viewport);
        assert!(grid.full());
        assert_eq!(grid.rects(viewport), vec![viewport]);
    }
}
#[cfg(test)]
mod cursor_count_tests {
    use super::*;
    #[test]
    fn clearing_and_removing_last_cursor_restores_default_fast_path() {
        let mut scene = Scene::new(100., 100.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Default::default(),
        );
        let child = scene.append(
            parent,
            NodeKind::Container(Layout::Column),
            Default::default(),
        );
        scene.set_cursor(parent, Some(crate::cursor::Cursor::Grab));
        scene.set_cursor(child, Some(crate::cursor::Cursor::Grabbing));
        assert_eq!(scene.cursor_styles, 2);
        scene.set_cursor(child, Some(crate::cursor::Cursor::Pointer));
        assert_eq!(scene.cursor_styles, 2);
        scene.set_cursor(child, None);
        assert_eq!(scene.cursor_styles, 1);
        scene.remove(parent);
        assert_eq!(scene.cursor_styles, 0);
        assert_eq!(scene.cursor_at(10., 10.), None);
    }
}
#[cfg(test)]
mod local_transform_tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    /// Skipping normalization for an empty patch keeps the whole-transform result.
    #[test]
    fn resolving_finite_matches_normalizing_the_whole_transform() {
        let values = [
            LocalTransform::IDENTITY,
            LocalTransform {
                scale: (-0., f32::from_bits(1)),
                rotate: -0.,
                skew: (-0.25, 1.5),
                rotate_x: 0.3,
                rotate_y: -0.4,
                perspective: -0.,
                origin: (2., -1.),
            },
            LocalTransform {
                scale: (f32::NAN, f32::INFINITY),
                rotate: f32::NEG_INFINITY,
                skew: (f32::NAN, f32::NEG_INFINITY),
                rotate_x: f32::INFINITY,
                rotate_y: f32::NAN,
                perspective: f32::INFINITY,
                origin: (f32::NAN, f32::NEG_INFINITY),
            },
            LocalTransform {
                perspective: -200.,
                ..rotated(0.75)
            },
        ];
        let bits = |t: LocalTransform| {
            [
                t.scale.0,
                t.scale.1,
                t.rotate,
                t.skew.0,
                t.skew.1,
                t.rotate_x,
                t.rotate_y,
                t.perspective,
                t.origin.0,
                t.origin.1,
            ]
            .map(f32::to_bits)
        };
        for original in values {
            let base = original.normalized();
            for values in values {
                for set in 0..=u8::MAX {
                    let patch = crate::style::TransformPatch { values, set };
                    assert_eq!(
                        bits(patch.resolve_finite(base)),
                        bits(patch.resolve(base).normalized()),
                        "component mask {set}"
                    );
                }
            }
        }
    }
    fn fixed(width: f32, height: f32) -> Style {
        Style {
            width: Some(width),
            height: Some(height),
            ..Style::default()
        }
    }
    fn rotated(radians: f32) -> LocalTransform {
        LocalTransform {
            rotate: radians,
            ..LocalTransform::IDENTITY
        }
    }
    /// A scene with an overlay root and one `width` × `height` rect at (100, 100).
    fn placed(width: f32, height: f32) -> (Scene, NodeId) {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let id = scene.append(
            scene.root(),
            NodeKind::Rect(Color(1, 2, 3, 255)),
            fixed(width, height),
        );
        scene.set_transform(id, Transform { x: 100., y: 100. });
        scene.flush();
        (scene, id)
    }
    fn union(damage: &[Rect]) -> Rect {
        damage.iter().copied().reduce(Rect::union).unwrap()
    }
    fn near(a: Rect, b: Rect) -> bool {
        (a.x - b.x).abs() <= 1.
            && (a.y - b.y).abs() <= 1.
            && (a.width - b.width).abs() <= 2.
            && (a.height - b.height).abs() <= 2.
    }
    /// Reordering repaints the children whose paint order changed against
    /// the others, not the whole parent: in an overlay of four overlapping
    /// boxes, moving the last to second repaints it alone.
    #[test]
    fn reordering_damages_only_the_children_that_moved() {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let boxes: Vec<_> = (0..4)
            .map(|i| {
                let id = scene.append(
                    scene.root(),
                    NodeKind::Rect(Color(1, 2, 3, 255)),
                    fixed(40., 40.),
                );
                scene.set_transform(
                    id,
                    Transform {
                        x: 20. * i as f32,
                        y: 0.,
                    },
                );
                id
            })
            .collect();
        scene.flush();
        let (a, b, c, d) = (boxes[0], boxes[1], boxes[2], boxes[3]);
        assert!(scene.reorder_children(scene.root(), &[a, d, b, c]));
        let frame = scene.flush();
        assert_eq!(union(&frame.damage), Rect::new(60., 0., 40., 40.));
        assert_eq!(scene.children(scene.root()), &[a, d, b, c]);
        // Not a reordering of the same children: refused, nothing changes.
        assert!(!scene.reorder_children(scene.root(), &[a, a, b, c]));
        assert!(scene.flush().damage.is_empty());
    }
    /// Written together, a translation and effects leave the node, its layer
    /// and the pixels to repaint as the two setters one after the other do.
    #[test]
    fn moving_and_fading_together_is_the_two_setters() {
        let effects = |opacity, filter_blur, blur_radius| Effects {
            opacity,
            filter_blur,
            blur_radius,
            ..Effects::default()
        };
        let cases = [
            (Transform { x: 150., y: 100. }, effects(0.5, 0., 0.)),
            // The translation as it was: only the effects change.
            (Transform { x: 100., y: 100. }, effects(0.5, 0., 0.)),
            (
                Transform {
                    x: f32::NAN,
                    y: 40.,
                },
                effects(f32::INFINITY, 0., 0.),
            ),
            (Transform { x: 30., y: 130. }, effects(1., 4., 0.)),
            (Transform { x: 90., y: 105. }, effects(0.8, 0., 6.)),
            (Transform { x: 200., y: 100. }, effects(0., 0., 0.)),
        ];
        for (transform, effects) in cases {
            let (mut together, a) = placed(40., 20.);
            together.set_transform_and_effects(a, transform, effects);
            let (mut apart, b) = placed(40., 20.);
            apart.set_transform(b, transform);
            apart.set_effects(b, effects);
            assert_eq!(together.node(a).transform, apart.node(b).transform);
            assert_eq!(together.effects(a), apart.effects(b));
            assert_eq!(
                together.geometry_revision() == 0,
                apart.geometry_revision() == 0
            );
            let (t, s) = (together.flush(), apart.flush());
            assert_eq!(together.is_layer(a), apart.is_layer(b));
            assert_eq!(t.composite_nodes, s.composite_nodes);
            assert_eq!(
                union(&t.damage),
                union(&s.damage),
                "{transform:?} {effects:?}"
            );
        }
    }
    #[test]
    fn fading_in_or_out_damages_where_the_node_shows_and_a_transparent_one_moves_freely() {
        let at = |x, y| Transform { x, y };
        let faded = |opacity| Effects {
            opacity,
            ..Effects::default()
        };
        let place = |scene: &Scene, id| scene.bounds(id);
        // Shown, then fading out in place: where it showed repaints.
        let (mut scene, id) = placed(40., 20.);
        let shown = place(&scene, id);
        scene.set_effects(id, faded(0.));
        assert_eq!(union(&scene.flush().damage), shown, "fading out");
        // Transparent, it moves and damages nothing: it paints nothing.
        scene.set_transform(id, at(200., 150.));
        assert!(scene.flush().damage.is_empty(), "a transparent node moving");
        // Fading in where it now is: that place repaints.
        let there = place(&scene, id);
        scene.set_effects(id, faded(1.));
        assert_eq!(union(&scene.flush().damage), there, "fading in");
        // Shown and moving: its old place and its new.
        scene.set_transform(id, at(20., 30.));
        let both = there.union(place(&scene, id));
        assert_eq!(union(&scene.flush().damage), both, "moving shown");
        // Moving and fading at once, either way: both places, as the two
        // setters damage them.
        let (mut scene, id) = placed(40., 20.);
        let old = place(&scene, id);
        scene.set_transform_and_effects(id, at(250., 40.), faded(0.));
        let gone = place(&scene, id);
        assert_eq!(
            union(&scene.flush().damage),
            old.union(gone),
            "moved and faded out"
        );
        scene.set_transform_and_effects(id, at(60., 200.), faded(0.5));
        let new = place(&scene, id);
        assert_eq!(
            union(&scene.flush().damage),
            gone.union(new),
            "moved and faded in"
        );
    }
    #[test]
    fn matrix_is_css_order_about_the_origin() {
        let bounds = Rect::new(10., 20., 40., 20.);
        let transform = LocalTransform {
            scale: (2., 1.),
            rotate: FRAC_PI_2,
            ..LocalTransform::IDENTITY
        };
        let matrix = transform.matrix(bounds);
        // The centre stays; scale applies first, along x, then rotation.
        let (x, y) = matrix.point(30., 30.);
        assert!((x - 30.).abs() < 1e-4 && (y - 30.).abs() < 1e-4);
        let (x, y) = matrix.point(40., 30.);
        assert!((x - 30.).abs() < 1e-4 && (y - 50.).abs() < 1e-4);
        let corner = LocalTransform {
            origin: (0., 0.),
            ..transform
        };
        let (x, y) = corner.matrix(bounds).point(10., 20.);
        assert!((x - 10.).abs() < 1e-4 && (y - 20.).abs() < 1e-4);
        let skew = LocalTransform {
            skew: (FRAC_PI_4, 0.),
            origin: (0., 0.),
            ..LocalTransform::IDENTITY
        };
        let (x, y) = skew.matrix(bounds).point(10., 30.);
        assert!((x - 20.).abs() < 1e-4 && (y - 30.).abs() < 1e-4);
        assert!(LocalTransform::IDENTITY.matrix(bounds) == crate::affine::Affine::IDENTITY);
        assert!(LocalTransform::IDENTITY.projective(bounds) == crate::affine::Projective::IDENTITY);
        // Untilted, the whole map is the affine one, exactly.
        for t in [transform, corner, skew] {
            assert_eq!(t.projective(bounds).affine(), Some(t.matrix(bounds)));
        }
    }
    #[test]
    fn a_layer_in_perspective_has_its_whole_map_beside_the_affine_one() {
        let mut scene = Scene::new(200., 200.);
        let id = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(100., 100.),
        );
        let bounds = Rect::new(0., 0., 100., 100.);
        let tilt = LocalTransform {
            rotate_y: 0.5,
            perspective: 400.,
            ..LocalTransform::IDENTITY
        };
        scene.set_local_transform(id, tilt);
        scene.flush();
        let item = scene
            .layer_items(None)
            .into_iter()
            .find(|item| item.id == id && item.isolated)
            .unwrap();
        assert_eq!(item.projective(), Some(tilt.projective(bounds)));
        assert!(tilt.projective(bounds).affine().is_none());
        // The affine matrix is the map before the tilt (here none).
        assert_eq!(item.matrix(), Some(crate::affine::Affine::IDENTITY));
    }
    #[test]
    fn equal_and_normalized_writes_do_nothing() {
        let (mut scene, id) = placed(40., 20.);
        scene.set_local_transform(id, rotated(0.3));
        assert!(!scene.flush().is_idle());
        let revision = scene.interaction_revision();
        scene.set_local_transform(id, rotated(0.3));
        assert!(scene.flush().is_idle());
        assert_eq!(scene.interaction_revision(), revision);
        // Nonfinite components normalize before comparing: NaN never equals.
        scene.set_local_transform(
            id,
            LocalTransform {
                scale: (f32::NAN, 1.),
                skew: (0., f32::INFINITY),
                origin: (f32::NAN, 0.5),
                ..rotated(0.3)
            },
        );
        assert!(scene.flush().is_idle());
        assert_eq!(scene.local_transform(id), rotated(0.3));
        // Moving the origin of an identity transform moves nothing.
        scene.set_local_transform(id, LocalTransform::IDENTITY);
        scene.flush();
        scene.set_local_transform(
            id,
            LocalTransform {
                origin: (0., 0.),
                ..LocalTransform::IDENTITY
            },
        );
        assert!(scene.flush().is_idle());
        assert!(!scene.is_layer(id));
    }
    #[test]
    fn transform_changes_damage_old_and_new_transformed_bounds_only() {
        let (mut scene, id) = placed(40., 20.);
        let layout = scene.layout_revision();
        scene.set_local_transform(id, rotated(FRAC_PI_2));
        let frame = scene.flush();
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(frame.paint_nodes, 0);
        assert_eq!(frame.composite_nodes, 1);
        assert_eq!(scene.layout_revision(), layout);
        // Old box (100, 100, 40, 20) and the upright one about its centre
        // (110, 90, 20, 40), merged as they overlap.
        assert!(near(union(&frame.damage), Rect::new(100., 90., 40., 40.)));
        assert!(near(
            scene.transformed_bounds(id),
            Rect::new(110., 90., 20., 40.)
        ));
        assert_eq!(scene.bounds(id), Rect::new(100., 100., 40., 20.));
        assert!(scene.is_layer(id));
        // The layer's pixels are kept: its revision does not move.
        let revision = scene.layer_revision(id);
        scene.set_local_transform(id, rotated(1.0));
        scene.flush();
        assert_eq!(scene.layer_revision(id), revision);
        // Translating it carries the rotated box along.
        scene.set_local_transform(id, rotated(FRAC_PI_2));
        scene.flush();
        scene.set_transform(id, Transform { x: 200., y: 100. });
        let frame = scene.flush();
        assert!(
            frame
                .damage
                .iter()
                .any(|d| near(*d, Rect::new(110., 90., 20., 40.)))
        );
        assert!(
            frame
                .damage
                .iter()
                .any(|d| near(*d, Rect::new(210., 90., 20., 40.)))
        );
    }
    #[test]
    fn descendants_damage_where_a_transformed_ancestor_shows_them() {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(40., 40.),
        );
        scene.set_transform(parent, Transform { x: 100., y: 100. });
        let child = scene.append(parent, NodeKind::Rect(Color(9, 9, 9, 255)), fixed(10., 10.));
        scene.set_local_transform(
            parent,
            LocalTransform {
                scale: (2., 2.),
                origin: (0., 0.),
                ..LocalTransform::IDENTITY
            },
        );
        scene.flush();
        let revision = scene.layer_revision(parent);
        scene.set_kind(child, NodeKind::Rect(Color(1, 1, 1, 255)));
        let frame = scene.flush();
        // The 10 × 10 child at the parent's origin shows twice the size.
        assert!(near(union(&frame.damage), Rect::new(100., 100., 20., 20.)));
        assert_ne!(scene.layer_revision(parent), revision);
        assert!(near(
            scene.transformed_bounds(child),
            Rect::new(100., 100., 20., 20.)
        ));
        assert!(near(
            scene.visible_bounds(child).unwrap(),
            Rect::new(100., 100., 20., 20.)
        ));
    }
    #[test]
    fn rotated_ink_culls_by_where_it_paints() {
        let (mut scene, id) = placed(40., 20.);
        scene.set_local_transform(id, rotated(FRAC_PI_2));
        scene.flush();
        let visits = |scene: &Scene, damage: Rect| {
            scene
                .layer_items_within(None, Some(&[damage]))
                .iter()
                .any(|item| item.id == id && item.isolated && item.matrix().is_some())
        };
        // Above the laid-out box, inside the rotated one.
        assert!(visits(&scene, Rect::new(118., 92., 4., 4.)));
        // Inside the laid-out box's corner, outside the rotated one.
        assert!(!visits(&scene, Rect::new(101., 101., 4., 4.)));
        // Its own items are drawn in its unrotated space.
        let items = scene.layer_items(Some(id));
        assert_eq!(items[0].bounds, Rect::new(100., 100., 40., 20.));
        assert_eq!(items[0].matrix(), None);
    }
    #[test]
    fn hit_tests_map_through_rotation_scale_and_skew() {
        let (mut scene, id) = placed(40., 40.);
        let root = scene.root();
        scene.set_local_transform(id, rotated(FRAC_PI_4));
        scene.flush();
        // The diamond's top point reaches 28 px above the centre (120, 120).
        assert_eq!(scene.hit_test(120., 94.), Some(id));
        assert_eq!(scene.hit_test(102., 102.), Some(root));
        scene.set_local_transform(
            id,
            LocalTransform {
                scale: (1.5, 1.5),
                ..LocalTransform::IDENTITY
            },
        );
        scene.flush();
        assert_eq!(scene.hit_test(92., 92.), Some(id));
        assert_eq!(scene.hit_test(88., 88.), Some(root));
        scene.set_local_transform(
            id,
            LocalTransform {
                scale: (0., 1.),
                ..LocalTransform::IDENTITY
            },
        );
        scene.flush();
        assert_eq!(scene.hit_test(120., 120.), Some(root));
        // Skewed along x by 45°: rows shift right with their distance below
        // the centre.
        scene.set_local_transform(
            id,
            LocalTransform {
                skew: (FRAC_PI_4, 0.),
                ..LocalTransform::IDENTITY
            },
        );
        scene.flush();
        assert_eq!(scene.hit_test(150., 135.), Some(id));
        assert_eq!(scene.hit_test(105., 135.), Some(root));
    }
    #[test]
    fn descendants_and_clips_hit_where_drawn() {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let root = scene.root();
        let card = scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            Style {
                clip: true,
                ..fixed(40., 40.)
            },
        );
        scene.set_transform(card, Transform { x: 100., y: 100. });
        // Overflows the card to the right; the card clips it.
        let wide = scene.append(card, NodeKind::Rect(Color(0, 0, 0, 255)), fixed(80., 10.));
        scene.set_local_transform(card, rotated(FRAC_PI_2));
        scene.flush();
        // Rotated about (120, 120): the child's row runs down the card's
        // right side (x 130..140), inside the clip from y 100 to 140.
        assert_eq!(scene.hit_test(135., 105.), Some(wide));
        assert_eq!(scene.hit_test(135., 135.), Some(wide));
        // Where it would be unrotated: the card, not the child.
        assert_eq!(scene.hit_test(105., 105.), Some(card));
        // Below the card the child is clipped away, rotated or not.
        assert_eq!(scene.hit_test(135., 150.), Some(root));
        assert_eq!(scene.hit_test_all(135., 105.), vec![wide, card, root]);
    }
    #[test]
    fn nested_transforms_compose() {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let panel = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(100., 100.),
        );
        let card = scene.append(panel, NodeKind::Rect(Color(0, 0, 0, 255)), fixed(20., 20.));
        scene.set_transform(card, Transform { x: 40., y: 40. });
        scene.set_local_transform(
            panel,
            LocalTransform {
                scale: (2., 2.),
                origin: (0., 0.),
                ..LocalTransform::IDENTITY
            },
        );
        scene.set_local_transform(card, rotated(FRAC_PI_4));
        scene.flush();
        // The card's centre (50, 50) lands at (100, 100); its diamond spans
        // 2 × 14.1 px either side there.
        assert!(near(
            scene.transformed_bounds(card),
            Rect::new(71.7, 71.7, 56.6, 56.6)
        ));
        assert_eq!(scene.hit_test(100., 74.), Some(card));
        assert_eq!(scene.hit_test(82., 82.), Some(panel));
        let items = scene.layer_items(Some(panel));
        let inner = items.iter().find(|item| item.id == card).unwrap();
        assert!(inner.isolated && inner.matrix().is_some());
    }
    #[test]
    fn removing_transformed_nodes_restores_counts() {
        let (mut scene, id) = placed(10., 10.);
        scene.set_local_transform(id, rotated(1.));
        scene.flush();
        assert_eq!((scene.transformed, scene.promoted_count), (1, 1));
        scene.set_local_transform(id, LocalTransform::IDENTITY);
        assert_eq!((scene.transformed, scene.promoted_count), (0, 1));
        scene.remove(id);
        assert_eq!((scene.transformed, scene.promoted_count), (0, 0));
    }
    #[test]
    fn content_blur_promotes_and_animates_by_compositing_alone() {
        let (mut scene, id) = placed(40., 20.);
        assert!(!scene.is_layer(id));
        scene.set_filter_blur(id, 2.);
        let frame = scene.flush();
        assert!(scene.is_layer(id));
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert_eq!(frame.composite_nodes, 1);
        // The box (100, 100, 40, 20) spread three sigma all round.
        assert!(near(union(&frame.damage), Rect::new(94., 94., 52., 32.)));
        // Its own items paint unblurred inside the layer; the window sees
        // one layer item.
        let items = scene.layer_items(None);
        let item = items.iter().find(|item| item.id == id).unwrap();
        assert!(item.isolated && item.matrix().is_none());
        assert_eq!(item.effects.filter_blur, 2.);
        assert_eq!(scene.layer_items(Some(id))[0].effects, Effects::default());
        // Animating the sigma neither lays out nor repaints the content.
        let (revision, content, layout) = (
            scene.layer_revision(id),
            scene.content_revision(),
            scene.layout_revision(),
        );
        scene.set_effects(
            id,
            Effects {
                opacity: 0.5,
                filter_blur: 4.,
                ..Effects::default()
            },
        );
        let frame = scene.flush();
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert!(near(union(&frame.damage), Rect::new(88., 88., 64., 44.)));
        assert_eq!(scene.layer_revision(id), revision);
        assert_eq!(scene.content_revision(), content);
        assert_eq!(scene.layout_revision(), layout);
        // Culling sees the spread: damage beside the box reaches the layer.
        let visits = |scene: &Scene, damage: Rect| {
            scene
                .layer_items_within(None, Some(&[damage]))
                .iter()
                .any(|item| item.id == id)
        };
        assert!(visits(&scene, Rect::new(90., 90., 4., 4.)));
        assert!(!visits(&scene, Rect::new(80., 80., 4., 4.)));
        // Back at 0, it paints as any node again, damaging the old spread.
        scene.set_filter_blur(id, 0.);
        let frame = scene.flush();
        assert!(!scene.is_layer(id));
        assert!(near(union(&frame.damage), Rect::new(88., 88., 64., 44.)));
        assert!(
            scene
                .layer_items(None)
                .iter()
                .any(|item| item.id == id && !item.isolated)
        );
        assert_eq!((scene.filtered.get(), scene.promoted_count), (0, 1));
        scene.set_filter_blur(id, 1.);
        scene.remove(id);
        assert_eq!((scene.filtered.get(), scene.promoted_count), (0, 0));
    }
    #[test]
    fn descendants_of_a_blurred_node_damage_its_spread() {
        let mut scene = Scene::new(400., 300.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let row = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                clip: true,
                ..fixed(40., 40.)
            },
        );
        scene.set_transform(row, Transform { x: 100., y: 100. });
        let word = scene.append(row, NodeKind::Rect(Color(9, 9, 9, 255)), fixed(10., 10.));
        scene.set_filter_blur(row, 2.);
        scene.flush();
        let revision = scene.layer_revision(row);
        scene.set_kind(word, NodeKind::Rect(Color(1, 1, 1, 255)));
        let frame = scene.flush();
        // The 10 × 10 word at the row's origin, blurred out 6 px past it
        // (the row clips before its blur, not after).
        assert!(near(union(&frame.damage), Rect::new(94., 94., 22., 22.)));
        assert_ne!(scene.layer_revision(row), revision);
        // Scaled up twice, the spread doubles with the content.
        scene.set_local_transform(
            row,
            LocalTransform {
                scale: (2., 2.),
                origin: (0., 0.),
                ..LocalTransform::IDENTITY
            },
        );
        scene.flush();
        scene.set_kind(word, NodeKind::Rect(Color(2, 2, 2, 255)));
        let frame = scene.flush();
        assert!(near(union(&frame.damage), Rect::new(88., 88., 44., 44.)));
        // A blurred child spreads its parent layer's bounds.
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(40., 40.),
        );
        let child = scene.append(parent, NodeKind::Rect(Color(9, 9, 9, 255)), fixed(10., 10.));
        scene.set_isolated(parent, true);
        scene.set_filter_blur(child, 5.);
        scene.flush();
        assert_eq!(scene.layer_bounds(parent), Rect::new(-15., -15., 55., 55.));
    }
    #[test]
    fn a_feather_costs_a_paint_items_rounded_clips_no_room() {
        // A box and its radii, the option's tag in the feather's zero.
        assert_eq!(std::mem::size_of::<Option<RoundRect>>(), 36);
        assert_eq!(std::mem::size_of::<RoundedClip>(), 72);
        for points in [0., 0.5, 24., 1e30] {
            assert_eq!(Feather::new(points).points(), points);
        }
        for none in [-1., f32::NAN, f32::INFINITY] {
            assert_eq!(Feather::new(none), Feather::NONE);
        }
    }
    #[test]
    fn clip_insets_clip_items_follow_size_and_composite_alone() {
        use crate::layout::Length::{Percent, Px};
        let (mut scene, id) = placed(40., 20.);
        let child = scene.append(id, NodeKind::Rect(Color(9, 9, 9, 255)), fixed(40., 20.));
        scene.flush();
        let clip_of = |scene: &Scene, node: NodeId| {
            scene
                .layer_items(None)
                .into_iter()
                .find(|item| item.id == node)
                .and_then(|item| item.clip)
        };
        // Closed to the top half: top 0, bottom 50% of its 20 px height.
        let half = ClipInset {
            top: Px(0.),
            right: Px(-5.),
            bottom: Percent(0.5),
            left: Px(2.),
        };
        let layout = scene.layout_revision();
        scene.set_clip_inset(id, Some(half));
        let frame = scene.flush();
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert_eq!(scene.layout_revision(), layout);
        assert!(near(union(&frame.damage), Rect::new(100., 100., 40., 20.)));
        let inset = Rect::new(102., 100., 43., 10.);
        assert_eq!(clip_of(&scene, id), Some(inset));
        assert_eq!(clip_of(&scene, child), Some(inset));
        // Hits only inside it.
        assert_eq!(scene.hit_test(110., 105.), Some(id));
        assert_eq!(scene.hit_test(110., 115.), Some(scene.root()));
        // The percentage follows the box as it grows.
        let mut style = scene.style(id);
        style.height = Some(40.);
        scene.set_style(id, style);
        scene.flush();
        assert_eq!(clip_of(&scene, id), Some(Rect::new(102., 100., 43., 20.)));
        // A blurred layer's composite is clipped; its pixels keep.
        scene.set_filter_blur(id, 2.);
        scene.flush();
        let revision = scene.layer_revision(id);
        scene.set_clip_inset(id, Some(ClipInset::px(1., 1., 1., 1.)));
        scene.flush();
        assert_eq!(scene.layer_revision(id), revision);
        assert_eq!(clip_of(&scene, id), Some(Rect::new(101., 101., 38., 38.)));
        scene.set_clip_inset(id, None);
        assert!(!scene.flush().damage.is_empty());
        assert_eq!(scene.clip_inset(id), None);
    }
    #[test]
    fn colour_filters_compose_down_the_tree_without_layers() {
        let (mut scene, id) = placed(40., 20.);
        assert_eq!(Effects::default().color_matrix(), None);
        // Nonfinite factors change nothing; negative ones clamp to 0.
        scene.set_effects(
            id,
            Effects {
                brightness: f32::NAN,
                contrast: -1.,
                ..Effects::default()
            },
        );
        assert_eq!(
            (scene.effects(id).brightness, scene.effects(id).contrast),
            (1., 0.)
        );
        let half = Effects {
            brightness: 0.5,
            ..Effects::default()
        };
        scene.set_effects(id, half);
        let frame = scene.flush();
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert!(!scene.is_layer(id));
        let matrix = half.color_matrix().unwrap();
        assert_eq!(matrix[0][0], 0.5);
        assert_eq!(matrix[1][1], 0.5);
        assert_eq!(matrix[0][3], 0.);
        // A filtered node inside a filtered one: both, composed.
        let quarter = compose_color(Some(matrix), Some(matrix)).unwrap();
        assert_eq!(quarter[2][2], 0.25);
        let item = scene
            .layer_items(None)
            .into_iter()
            .find(|item| item.id == id)
            .unwrap();
        assert_eq!(item.color(), Some(matrix));
        // Contrast 0 maps everything to mid grey.
        let grey = Effects {
            contrast: 0.,
            ..Effects::default()
        }
        .color_matrix()
        .unwrap();
        assert_eq!(grey[0], [0., 0., 0., 0.5]);
    }
    #[test]
    fn masks_promote_and_move_by_compositing_alone() {
        let image = Arc::new(crate::image::ImageData::new(2, 1, vec![255; 8]).unwrap());
        let (mut scene, id) = placed(40., 20.);
        let content = scene.content_revision();
        scene.set_mask(
            id,
            Some(ImageMask {
                image: image.clone(),
                offset: (0., 0.),
                size: (20., 20.),
            }),
        );
        let frame = scene.flush();
        assert!(scene.is_layer(id));
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        // A mask only hides: damage stays within the box.
        assert!(near(union(&frame.damage), Rect::new(100., 100., 40., 20.)));
        assert_ne!(scene.content_revision(), content);
        let items = scene.layer_items(None);
        assert!(items.iter().any(|item| item.id == id && item.isolated));
        let (revision, content) = (scene.layer_revision(id), scene.content_revision());
        scene.set_mask_offset(id, (12.5, f32::NAN));
        let frame = scene.flush();
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
        assert_eq!(frame.composite_nodes, 1);
        assert!(near(union(&frame.damage), Rect::new(100., 100., 40., 20.)));
        assert_eq!(scene.mask(id).unwrap().offset, (12.5, 0.));
        assert_eq!(scene.layer_revision(id), revision);
        assert_eq!(scene.content_revision(), content);
        // The same offset again changes nothing.
        scene.set_mask_offset(id, (12.5, 0.));
        assert!(scene.flush().is_idle());
        scene.set_mask(id, None);
        scene.flush();
        assert!(!scene.is_layer(id));
        // Without a mask, moving one does nothing.
        scene.set_mask_offset(id, (1., 1.));
        assert!(scene.flush().is_idle());
    }
}

#[cfg(test)]
mod text_lines_tests {
    use super::*;

    #[test]
    fn text_lines_give_each_visual_lines_bytes_box_and_grapheme_edges() {
        let mut scene = Scene::new(400., 300.);
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "ab cd\nef".into(),
                color: Color(0, 0, 0, 255),
                font_size: 10.,
            },
            Style {
                width: Some(200.),
                ..Default::default()
            },
        );
        scene.flush();
        let lines = scene.text_lines(node).expect("lines");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0].range, 0..5, "its break left out");
        assert_eq!(lines[1].range, 6..8);
        assert_eq!(lines[0].carets.len(), 6, "five graphemes, six edges");
        assert!(
            lines[0].carets.windows(2).all(|w| w[1] > w[0]),
            "left to right"
        );
        assert!(!lines[0].rtl);
        assert!(
            lines[1].rect.y > lines[0].rect.y,
            "the second below the first"
        );
        let b = scene.bounds(node);
        assert_eq!(lines[0].carets[0], b.x, "in the window's points");
        // Wrapped: a line a row.
        let mut style = scene.style(node);
        style.width = Some(20.);
        scene.set_style(node, style);
        scene.flush();
        let wrapped = scene.text_lines(node).unwrap();
        assert!(wrapped.len() > 2, "{wrapped:?}");
        assert_eq!(wrapped.last().unwrap().range.end, 8);
        for pair in wrapped.windows(2) {
            assert!(pair[0].range.end <= pair[1].range.start, "{wrapped:?}");
        }
    }
}
