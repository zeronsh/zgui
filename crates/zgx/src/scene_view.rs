//! A canvas's view (its drawing panned and zoomed without being drawn
//! again) and grid fills (a dotted or ruled background that follows a view
//! with no call per cell). Both are rare, kept beside the nodes: a scene
//! with neither pays a length check for them.
use super::{Color, NodeId, PAINTED, Scene};
use std::collections::HashMap;

/// How a canvas's drawing is placed in its box: scaled by `scale` about the
/// box's origin, then moved by `x`, `y` points. A pan or a zoom changes the
/// view, not the drawing: nothing is drawn or laid out again, and a
/// renderer drawing the canvas as shapes scales them as it places them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasView {
    /// Across, in points, after scaling.
    pub x: f32,
    /// Down, in points, after scaling.
    pub y: f32,
    /// How large a point of the drawing shows (1: as drawn).
    pub scale: f32,
}
impl Default for CanvasView {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            scale: 1.,
        }
    }
}
impl CanvasView {
    /// Where drawing point (`x`, `y`) shows in the box.
    pub fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.x, y * self.scale + self.y)
    }
    /// The view with each non-finite part, or a scale that isn't positive,
    /// as the identity's.
    fn sanitized(self) -> Self {
        let or = |v: f32, d: f32| if v.is_finite() { v } else { d };
        Self {
            x: or(self.x, 0.),
            y: or(self.y, 0.),
            scale: if self.scale.is_finite() && self.scale > 0. {
                self.scale
            } else {
                1.
            },
        }
    }
}

/// What a grid fill marks at its points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridMark {
    /// A square dot this many points across, centred on each point.
    Dot(f32),
    /// Lines this many points wide through the points, across and down.
    Lines(f32),
}

/// A grid painted over a canvas's box, under its drawing (a board's dots,
/// graph paper): its points `step` apart from `x`, `y`, scaled by
/// `scale` (a canvas view's pan and zoom), the step doubled while it would
/// show closer than `min_step` points. Marks keep their size in points
/// whatever the scale, as a board's dots do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridFill {
    /// Between points, before scaling.
    pub step: f32,
    /// Where a point lies, in the box, after scaling (a view's `x`).
    pub x: f32,
    /// Where a point lies, in the box, after scaling (a view's `y`).
    pub y: f32,
    /// How far apart the points show per step (a view's `scale`).
    pub scale: f32,
    /// The closest the points may show, in points: the step doubles until
    /// they are this far apart at least.
    pub min_step: f32,
    /// What marks each point.
    pub mark: GridMark,
    /// The marks' colour.
    pub color: Color,
}
impl GridFill {
    /// The points' spacing as shown, in points, and their phase in the box
    /// (where the first at or after its origin lies), along each axis;
    /// `None` if there is nothing to mark.
    pub fn spacing(&self) -> Option<(f32, (f32, f32))> {
        let ok = |v: f32| v.is_finite() && v > 0.;
        if !ok(self.step) || !ok(self.scale) || !self.x.is_finite() || !self.y.is_finite() {
            return None;
        }
        let mut shown = self.step * self.scale;
        let min = if self.min_step.is_finite() {
            self.min_step.max(1.)
        } else {
            1.
        };
        // Doubling, bounded: a step shown smaller than a point never passes.
        for _ in 0..64 {
            if shown >= min {
                break;
            }
            shown *= 2.;
        }
        if !ok(shown) || shown < 1. {
            return None;
        }
        let phase = |at: f32| at.rem_euclid(shown);
        Some((shown, (phase(self.x), phase(self.y))))
    }
}

impl GridFill {
    /// Its marks over `bounds` (the canvas's box, translated), as the boxes
    /// they fill: a dot's square, or each line's strip across or down the
    /// box. At most 65,536 of them; none if more would show.
    pub fn marks(&self, bounds: super::Rect) -> Vec<super::Rect> {
        use super::Rect;
        const MOST: i64 = 65_536;
        let Some((step, (x0, y0))) = self.spacing() else {
            return Vec::new();
        };
        let across = ((bounds.width - x0) / step).floor().max(-1.) as i64 + 1;
        let down = ((bounds.height - y0) / step).floor().max(-1.) as i64 + 1;
        let at = |first: f32, k: i64| first + k as f32 * step;
        let mut marks = Vec::new();
        match self.mark {
            GridMark::Dot(size) if size > 0. && size.is_finite() => {
                if across.saturating_mul(down) > MOST {
                    return marks;
                }
                marks.reserve((across.max(0) * down.max(0)) as usize);
                for j in 0..down {
                    let y = at(bounds.y + y0, j);
                    for i in 0..across {
                        let x = at(bounds.x + x0, i);
                        marks.push(Rect::new(x - size / 2., y - size / 2., size, size));
                    }
                }
            }
            GridMark::Lines(width) if width > 0. && width.is_finite() => {
                if across.saturating_add(down) > MOST {
                    return marks;
                }
                for i in 0..across {
                    let x = at(bounds.x + x0, i);
                    marks.push(Rect::new(x - width / 2., bounds.y, width, bounds.height));
                }
                for j in 0..down {
                    let y = at(bounds.y + y0, j);
                    marks.push(Rect::new(bounds.x, y - width / 2., bounds.width, width));
                }
            }
            _ => {}
        }
        marks
    }
}

#[derive(Default)]
pub(super) struct Views {
    canvases: HashMap<NodeId, CanvasView>,
    grids: HashMap<NodeId, GridFill>,
}
impl Views {
    /// `id` is gone.
    pub(super) fn forget(&mut self, id: NodeId) {
        if !self.canvases.is_empty() {
            self.canvases.remove(&id);
        }
        if !self.grids.is_empty() {
            self.grids.remove(&id);
        }
    }
}

impl Scene {
    /// Place canvas `id`'s drawing by `view` (or, `None`, as drawn): it
    /// repaints, nothing is drawn or laid out again.
    pub fn set_canvas_view(&mut self, id: NodeId, view: Option<CanvasView>) {
        if !self.contains(id) {
            return;
        }
        let view = view
            .map(CanvasView::sanitized)
            .filter(|view| *view != CanvasView::default());
        let old = match view {
            Some(view) => self.views.canvases.insert(id, view),
            None => self.views.canvases.remove(&id),
        };
        if old != view {
            self.damage_subtree(id);
            self.invalidate(id, PAINTED);
        }
    }
    /// Canvas `id`'s view: the identity unless set.
    #[inline]
    pub fn canvas_view(&self, id: NodeId) -> CanvasView {
        if self.views.canvases.is_empty() {
            return CanvasView::default();
        }
        self.views.canvases.get(&id).copied().unwrap_or_default()
    }
    /// Paint `grid` over canvas `id`'s box, under its drawing (or, `None`,
    /// none).
    pub fn set_grid_fill(&mut self, id: NodeId, grid: Option<GridFill>) {
        if !self.contains(id) {
            return;
        }
        let old = match grid {
            Some(grid) => self.views.grids.insert(id, grid),
            None => self.views.grids.remove(&id),
        };
        if old != grid {
            self.damage_subtree(id);
            self.invalidate(id, PAINTED);
        }
    }
    /// The grid painted over canvas `id`'s box, if any.
    #[inline]
    pub fn grid_fill(&self, id: NodeId) -> Option<&GridFill> {
        if self.views.grids.is_empty() {
            return None;
        }
        self.views.grids.get(&id)
    }
    /// Whether any node has a grid fill: renderers look no further if not.
    #[inline]
    pub fn has_grid_fills(&self) -> bool {
        !self.views.grids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Rect;

    fn grid(step: f32, scale: f32, min_step: f32) -> GridFill {
        GridFill {
            step,
            x: -3.,
            y: 7.,
            scale,
            min_step,
            mark: GridMark::Dot(1.),
            color: Color(0, 0, 0, 255),
        }
    }

    #[test]
    fn a_grid_doubles_its_step_to_its_least_and_keeps_its_phase() {
        // 16 shown as 4 at a quarter: doubled to 16 to be 12 or more apart.
        let (step, (x, y)) = grid(16., 0.25, 12.).spacing().unwrap();
        assert_eq!(step, 16.);
        assert_eq!((x, y), (13., 7.));
        assert_eq!(grid(16., 2., 12.).spacing().unwrap().0, 32.);
        assert!(grid(0., 1., 0.).spacing().is_none());
        assert!(grid(16., f32::NAN, 0.).spacing().is_none());
        // Marks across a 100 by 50 box from its origin.
        let marks = grid(10., 1., 0.).marks(Rect::new(0., 0., 100., 50.));
        assert_eq!(marks.len(), 10 * 5);
        assert_eq!(marks[0], Rect::new(6.5, 6.5, 1., 1.));
        // Too many to mark: none.
        assert!(
            grid(1., 1., 0.)
                .marks(Rect::new(0., 0., 1000., 1000.))
                .is_empty()
        );
    }

    #[test]
    fn a_canvas_view_change_repaints_and_the_identity_is_no_view() {
        let mut scene = Scene::new(100., 100.);
        let node = scene.append(
            scene.root(),
            crate::scene::NodeKind::Canvas(Default::default()),
            crate::scene::Style {
                width: Some(50.),
                height: Some(50.),
                ..Default::default()
            },
        );
        scene.flush();
        let view = CanvasView {
            x: 4.,
            y: 0.,
            scale: 2.,
        };
        scene.set_canvas_view(node, Some(view));
        assert_eq!(scene.canvas_view(node), view);
        let report = scene.flush();
        assert!(!report.damage.is_empty());
        assert_eq!(report.layout_nodes, 0, "nothing lays out");
        // The same again: nothing to do.
        scene.set_canvas_view(node, Some(view));
        assert!(scene.flush().damage.is_empty());
        scene.set_canvas_view(
            node,
            Some(CanvasView {
                x: f32::NAN,
                y: 0.,
                scale: -1.,
            }),
        );
        assert_eq!(scene.canvas_view(node), CanvasView::default());
        scene.remove(node);
        assert_eq!(scene.canvas_view(node), CanvasView::default());
    }
}
