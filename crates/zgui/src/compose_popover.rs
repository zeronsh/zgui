//! Placement for a retained popover portal; ownership and focus live in compose_modal.
use crate::{
    scene::{Direction, NodeId, Rect, Transform},
    widgets::Ui,
};

/// Which side of its trigger an anchored popover or menu opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Below the trigger.
    #[default]
    Bottom,
    /// Above it.
    Top,
    /// To its left.
    Left,
    /// To its right.
    Right,
}
/// How it lines up along that side: the trigger's start edge, centre or
/// end edge. Start is the left (the right where the trigger's text runs
/// right to left) or the top; end the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    /// The trigger's start edge (left or top).
    #[default]
    Start,
    /// Its centre.
    Center,
    /// Its end edge (right or bottom).
    End,
}
/// Where an anchored popover or menu opens: on `side` of the trigger,
/// `offset` points away, aligned by `align`. It flips to the opposite side
/// when the preferred one lacks room, and stays inside the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The side it prefers.
    pub side: Side,
    /// How it lines up along that side.
    pub align: Edge,
    /// How far from the trigger, in points.
    pub offset: f32,
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            side: Side::Bottom,
            align: Edge::Start,
            offset: 4.,
        }
    }
}

/// A popover's arrow (`View::arrow`): a square turned on its corner and
/// drawn as the panel is, in a box that shows only the half of it past the
/// panel's edge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Arrow {
    /// The box clipping it, a child of the placement host.
    pub clip: NodeId,
    /// The turned square, in the box.
    pub diamond: NodeId,
    /// How far it reaches from the panel's edge.
    pub size: f32,
}

/// Position the private placement host, preserving the public panel's styles.
/// Registrations belong to the logical owner so disposal removes subscriptions.
/// Where the host shows popups past the window (`Ui::set_popup_screens`),
/// it flips and clamps to the screen its anchor is on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mount(
    ui: &mut Ui,
    owner: NodeId,
    anchor: NodeId,
    overlay: NodeId,
    position: NodeId,
    panel: NodeId,
    at: Placement,
    // `View::open_over`'s item, if any.
    over: Option<Box<dyn Fn() -> Option<NodeId>>>,
    arrow: Option<Arrow>,
) {
    mount_placement(
        ui,
        owner,
        anchor,
        overlay,
        position,
        panel,
        Some(at),
        over,
        arrow,
    );
}

/// A submenu starts beside its trigger, flipping left when the right side lacks room.
pub(crate) fn mount_submenu(
    ui: &mut Ui,
    owner: NodeId,
    anchor: NodeId,
    overlay: NodeId,
    position: NodeId,
    panel: NodeId,
) {
    mount_placement(
        ui, owner, anchor, overlay, position, panel, None, None, None,
    );
}

#[allow(clippy::too_many_arguments)]
fn mount_placement(
    ui: &mut Ui,
    owner: NodeId,
    anchor: NodeId,
    overlay: NodeId,
    position: NodeId,
    panel: NodeId,
    // None: a submenu's, beside its trigger.
    at: Option<Placement>,
    over: Option<Box<dyn Fn() -> Option<NodeId>>>,
    arrow: Option<Arrow>,
) {
    let anchor_bounds = ui.observe_bounds(anchor);
    let viewport = ui.observe_bounds(overlay);
    let panel_bounds = ui.observe_bounds(panel);
    let position_bounds = ui.observe_bounds(position);
    let screens = ui.popup_screens();
    let weak = ui.downgrade();
    let anchor_node = anchor;
    ui.bind(owner, move || {
        let anchor = anchor_bounds.get();
        let viewport = screens
            .with(|screens| screen_of(screens, anchor))
            .unwrap_or_else(|| viewport.get());
        let panel = panel_bounds.get();
        let current = position_bounds.get();
        let Some(ui) = weak.upgrade() else {
            return;
        };
        // Start and end, and which side a submenu prefers, follow the way
        // the trigger's text runs.
        let rtl = ui.scene.borrow().direction(anchor_node) == Direction::Rtl;
        // `open_over`'s item: where it lies in the panel (layout's, so the
        // panel's own placement does not move it).
        let item = over.as_ref().and_then(|over| over()).map(|item| {
            let scene = ui.scene.borrow();
            let (item, laid) = (
                scene.bounds(item),
                scene.bounds(current_panel(&scene, position)),
            );
            Rect::new(item.x - laid.x, item.y - laid.y, item.width, item.height)
        });
        // An arrow, but for a pop-up button's menu over its trigger, takes
        // room between the trigger and the panel.
        let arrow = arrow.filter(|_| item.is_none());
        let desired = match (at, item) {
            (_, Some(item)) => {
                over_placement(anchor, viewport, panel.width, panel.height, item, rtl)
            }
            (None, _) => submenu_placement(anchor, viewport, panel.width, panel.height, rtl),
            (Some(mut at), None) => {
                at.offset += arrow.map_or(0., |arrow| arrow.size);
                placement(anchor, viewport, panel.width, panel.height, at, rtl)
            }
        };
        let mut scene = ui.scene.borrow_mut();
        if !scene.contains(position) {
            return;
        }
        let previous = scene.transform(position);
        let dx = desired.0 - current.x;
        let dy = desired.1 - current.y;
        // Avoid introducing an allocation feedback cycle through float rounding.
        if dx.abs() > 0.01 || dy.abs() > 0.01 {
            scene.set_transform(
                position,
                Transform {
                    x: previous.x + dx,
                    y: previous.y + dy,
                },
            );
        }
        if let (Some(arrow), Some(at)) = (arrow, at)
            && scene.contains(arrow.clip)
        {
            // The panel within the placement host, as laid out.
            let within = (panel.x - current.x, panel.y - current.y);
            let panel = Rect::new(
                desired.0 + within.0,
                desired.1 + within.1,
                panel.width,
                panel.height,
            );
            place_arrow(&mut scene, arrow, anchor, panel, within, at.side);
        }
    });
}

/// Which way an arrow points, from the panel toward its trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Points {
    Up,
    Down,
    Left,
    Right,
}

/// Where an arrow goes on `panel` (placed, in window points; `within`, its
/// offset in the placement host) for a trigger at `anchor`: on the edge
/// facing the trigger (as placement flipped it), along the trigger's
/// centre, kept clear of the panel's corners. Its box and the square in it,
/// relative to the placement host and the box.
fn arrow_geometry(
    size: f32,
    anchor: Rect,
    panel: Rect,
    within: (f32, f32),
    side: Side,
) -> (Points, Rect, (f32, f32)) {
    let points = match side {
        Side::Top | Side::Bottom if panel.y >= anchor.y + anchor.height - 0.5 => Points::Up,
        Side::Top | Side::Bottom => Points::Down,
        Side::Left | Side::Right if panel.x >= anchor.x + anchor.width - 0.5 => Points::Left,
        Side::Left | Side::Right => Points::Right,
    };
    // Half the square's diagonal is `size`.
    let half = size * std::f32::consts::SQRT_2 / 2.;
    let along = |centre: f32, start: f32, extent: f32| {
        let margin = (size * 1.5).min(extent / 2.);
        (centre - start).clamp(margin, extent - margin)
    };
    let (w, h) = (panel.width, panel.height);
    let (box_rect, square) = match points {
        Points::Up | Points::Down => {
            let t = along(anchor.x + anchor.width / 2., panel.x, w);
            let y = if points == Points::Up {
                within.1 - size
            } else {
                within.1 + h
            };
            let square_y = if points == Points::Up {
                size - half
            } else {
                -half
            };
            (
                Rect::new(within.0 + t - size, y, 2. * size, size),
                (size - half, square_y),
            )
        }
        Points::Left | Points::Right => {
            let t = along(anchor.y + anchor.height / 2., panel.y, h);
            let x = if points == Points::Left {
                within.0 - size
            } else {
                within.0 + w
            };
            let square_x = if points == Points::Left {
                size - half
            } else {
                -half
            };
            (
                Rect::new(x, within.1 + t - size, size, 2. * size),
                (square_x, size - half),
            )
        }
    };
    (points, box_rect, square)
}

fn place_arrow(
    scene: &mut crate::scene::Scene,
    arrow: Arrow,
    anchor: Rect,
    panel: Rect,
    within: (f32, f32),
    side: Side,
) {
    let (_, area, (square_x, square_y)) = arrow_geometry(arrow.size, anchor, panel, within, side);
    let mut style = scene.style(arrow.clip);
    if style.width != Some(area.width) || style.height != Some(area.height) {
        style.width = Some(area.width);
        style.height = Some(area.height);
        scene.set_style(arrow.clip, style);
    }
    let moved = |current: Transform, x: f32, y: f32| {
        (current.x - x).abs() > 0.01 || (current.y - y).abs() > 0.01
    };
    if moved(scene.transform(arrow.clip), area.x, area.y) {
        scene.set_transform(
            arrow.clip,
            Transform {
                x: area.x,
                y: area.y,
            },
        );
    }
    if moved(scene.transform(arrow.diamond), square_x, square_y) {
        scene.set_transform(
            arrow.diamond,
            Transform {
                x: square_x,
                y: square_y,
            },
        );
    }
}

/// The screen of `screens` an anchored popup opens on: the one holding its
/// anchor's centre, else the nearest.
pub(crate) fn screen_of(screens: &[Rect], anchor: Rect) -> Option<Rect> {
    let (x, y) = (anchor.x + anchor.width / 2., anchor.y + anchor.height / 2.);
    let distance = |screen: &Rect| {
        let dx = (screen.x - x).max(x - (screen.x + screen.width)).max(0.);
        let dy = (screen.y - y).max(y - (screen.y + screen.height)).max(0.);
        dx.hypot(dy)
    };
    screens
        .iter()
        .copied()
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

fn placement(
    anchor: Rect,
    viewport: Rect,
    width: f32,
    height: f32,
    at: Placement,
    rtl: bool,
) -> (f32, f32) {
    let right = viewport.x + viewport.width;
    let bottom = viewport.y + viewport.height;
    let gap = at.offset;
    // Along the side's axis: the preferred side, or the other when only it
    // has room.
    let along = |start: f32, end: f32, size: f32, before: bool, low: f32, high: f32| {
        let (first, second) = (start - size - gap, end + gap);
        let (preferred, other) = if before {
            (first, second)
        } else {
            (second, first)
        };
        let fits = |v: f32| v >= low && v + size <= high;
        if !fits(preferred) && fits(other) {
            other
        } else {
            preferred
        }
    };
    // Across it: lined up with the trigger's start, centre or end. Below
    // or above the trigger, start is its right edge where text runs right
    // to left; beside it, its top either way.
    let across = |start: f32, size: f32, extent: f32, horizontal: bool| {
        let flipped = horizontal && rtl;
        match at.align {
            Edge::Start if flipped => start + extent - size,
            Edge::End if !flipped => start + extent - size,
            Edge::Center => start + (extent - size) / 2.,
            _ => start,
        }
    };
    let (x, y) = match at.side {
        Side::Bottom | Side::Top => (
            across(anchor.x, width, anchor.width, true),
            along(
                anchor.y,
                anchor.y + anchor.height,
                height,
                at.side == Side::Top,
                viewport.y,
                bottom,
            ),
        ),
        Side::Left | Side::Right => (
            along(
                anchor.x,
                anchor.x + anchor.width,
                width,
                at.side == Side::Left,
                viewport.x,
                right,
            ),
            across(anchor.y, height, anchor.height, false),
        ),
    };
    (
        x.clamp(viewport.x, (right - width).max(viewport.x)),
        y.clamp(viewport.y, (bottom - height).max(viewport.y)),
    )
}

/// The panel `position` places (its one child).
fn current_panel(scene: &crate::scene::Scene, position: NodeId) -> NodeId {
    scene
        .children(position)
        .first()
        .copied()
        .unwrap_or(position)
}

/// A pop-up button's menu: its `item` (where it lies in the panel) over the
/// trigger, lined up with its start edge (the right where text runs right
/// to left), kept on the screen.
fn over_placement(
    anchor: Rect,
    viewport: Rect,
    width: f32,
    height: f32,
    item: Rect,
    rtl: bool,
) -> (f32, f32) {
    let x = match rtl {
        true => anchor.x + anchor.width - (item.x + item.width),
        false => anchor.x - item.x,
    };
    let y = anchor.y + (anchor.height - item.height) / 2. - item.y;
    let right = viewport.x + viewport.width;
    let bottom = viewport.y + viewport.height;
    (
        x.clamp(viewport.x, (right - width).max(viewport.x)),
        y.clamp(viewport.y, (bottom - height).max(viewport.y)),
    )
}

/// A submenu beside its trigger: after it (the right, or the left where
/// text runs right to left), else the side with more room.
fn submenu_placement(
    anchor: Rect,
    viewport: Rect,
    width: f32,
    height: f32,
    rtl: bool,
) -> (f32, f32) {
    let right_edge = viewport.x + viewport.width;
    let right = anchor.x + anchor.width + 4.;
    let left = anchor.x - width - 4.;
    let right_room = right_edge - right;
    let left_room = anchor.x - 4. - viewport.x;
    let x = match rtl {
        false if right_room >= width || right_room >= left_room => right,
        false => left,
        true if left_room >= width || left_room >= right_room => left,
        true => right,
    };
    (
        x.clamp(viewport.x, (right_edge - width).max(viewport.x)),
        anchor.y.clamp(
            viewport.y,
            (viewport.y + viewport.height - height).max(viewport.y),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_aligns_flips_and_clamps() {
        let viewport = Rect::new(0., 0., 400., 300.);
        let trigger = Rect::new(100., 50., 80., 20.);
        let at = |side, align| Placement {
            side,
            align,
            offset: 4.,
        };
        // Below, start-aligned: the default, as before.
        assert_eq!(
            placement(trigger, viewport, 120., 60., Placement::default(), false),
            (100., 74.)
        );
        // End-aligned: the panel's right edge on the trigger's.
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Bottom, Edge::End),
                false
            ),
            (60., 74.)
        );
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Bottom, Edge::Center),
                false
            ),
            (80., 74.)
        );
        // Above, with no room above: below instead.
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Top, Edge::Start),
                false
            ),
            (100., 74.)
        );
        // Right of it, centred on it.
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Right, Edge::Center),
                false
            ),
            (184., 30.)
        );
        // Near the bottom edge: flipped above.
        let low = Rect::new(100., 260., 80., 20.);
        assert_eq!(
            placement(low, viewport, 120., 60., Placement::default(), false),
            (100., 196.)
        );
    }
    #[test]
    fn start_and_end_follow_the_triggers_direction() {
        let viewport = Rect::new(0., 0., 400., 300.);
        let trigger = Rect::new(100., 50., 80., 20.);
        let at = |side, align| Placement {
            side,
            align,
            offset: 4.,
        };
        // Right to left, start is the trigger's right edge, end its left.
        assert_eq!(
            placement(trigger, viewport, 120., 60., Placement::default(), true),
            (60., 74.)
        );
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Bottom, Edge::End),
                true
            ),
            (100., 74.)
        );
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Bottom, Edge::Center),
                true
            ),
            (80., 74.)
        );
        // Beside it, start is its top either way; sides stay physical.
        assert_eq!(
            placement(
                trigger,
                viewport,
                120.,
                60.,
                at(Side::Right, Edge::Start),
                true
            ),
            (184., 50.)
        );
        // Near the right edge, kept inside it.
        let edge = Rect::new(330., 50., 60., 20.);
        assert_eq!(
            placement(edge, viewport, 120., 60., Placement::default(), true),
            (270., 74.)
        );
        // A submenu prefers the left, and takes the right when only it has
        // room.
        let menu = Rect::new(10., 20., 300., 200.);
        assert_eq!(
            submenu_placement(Rect::new(230., 190., 60., 20.), menu, 100., 60., true),
            (126., 160.)
        );
        assert_eq!(
            submenu_placement(Rect::new(30., 50., 80., 20.), menu, 100., 60., true),
            (114., 50.)
        );
        // A pop-up button's item lines up with its right edge.
        let button = Rect::new(100., 150., 120., 24.);
        let item = Rect::new(5., 53., 110., 24.);
        assert_eq!(
            over_placement(button, viewport, 120., 180., item, true),
            (105., 97.)
        );
    }
    #[test]
    fn an_arrow_points_at_its_trigger_from_the_side_it_opened_on() {
        let trigger = Rect::new(100., 50., 80., 20.);
        // Below the trigger: on the panel's top edge, under the trigger's
        // centre (140), its box from 132 to 148, reaching 8 up.
        let below = Rect::new(100., 78., 200., 120.);
        let (points, area, square) = arrow_geometry(8., trigger, below, (0., 0.), Side::Bottom);
        assert_eq!(points, Points::Up);
        assert_eq!(area, Rect::new(32., -8., 16., 8.));
        let half = 8. * std::f32::consts::SQRT_2 / 2.;
        assert_eq!(square, (8. - half, 8. - half));
        // Flipped above it: on the bottom edge, pointing down.
        let above = Rect::new(100., -100., 200., 120.);
        let (points, area, _) = arrow_geometry(8., trigger, above, (0., 0.), Side::Bottom);
        assert_eq!(points, Points::Down);
        assert_eq!(area.y, 120.);
        // Beside it, on the right: on the left edge, at the trigger's middle
        // (60), pointing left.
        let right = Rect::new(188., 0., 200., 120.);
        let (points, area, _) = arrow_geometry(8., trigger, right, (0., 0.), Side::Right);
        assert_eq!(points, Points::Left);
        assert_eq!(area, Rect::new(-8., 52., 8., 16.));
        // A trigger past the panel's end: kept clear of its corner.
        let far = Rect::new(500., 50., 20., 20.);
        let (_, area, _) = arrow_geometry(8., far, below, (0., 0.), Side::Bottom);
        assert_eq!(area.x + area.width / 2., 200. - 12.);
    }
    #[test]
    fn a_popup_button_opens_with_its_item_over_the_trigger() {
        let viewport = Rect::new(0., 0., 400., 300.);
        let trigger = Rect::new(100., 150., 120., 24.);
        // The third 24-point row (after 5 points of padding) of a menu.
        let item = Rect::new(5., 53., 110., 24.);
        assert_eq!(
            over_placement(trigger, viewport, 120., 180., item, false),
            (95., 97.)
        );
        // Near the top, kept on the screen.
        let high = Rect::new(100., 10., 120., 24.);
        assert_eq!(
            over_placement(high, viewport, 120., 180., item, false),
            (95., 0.)
        );
    }
    #[test]
    fn popups_open_on_their_anchors_screen() {
        let screens = [
            Rect::new(-1920., -200., 1920., 1080.),
            Rect::new(0., -40., 1512., 942.),
        ];
        // On the right screen, and on the left one.
        let right = Rect::new(400., 300., 80., 20.);
        assert_eq!(screen_of(&screens, right), Some(screens[1]));
        let left = Rect::new(-300., 300., 80., 20.);
        assert_eq!(screen_of(&screens, left), Some(screens[0]));
        // Under both, nearer the right one.
        let below = Rect::new(20., 1000., 60., 20.);
        assert_eq!(screen_of(&screens, below), Some(screens[1]));
        assert_eq!(screen_of(&[], right), None);
    }
    #[test]
    fn submenu_placement_flips_and_clamps_with_nonzero_viewport_origin() {
        let viewport = Rect::new(10., 20., 300., 200.);
        assert_eq!(
            submenu_placement(Rect::new(30., 50., 80., 20.), viewport, 100., 60., false),
            (114., 50.)
        );
        assert_eq!(
            submenu_placement(Rect::new(230., 190., 60., 20.), viewport, 100., 60., false),
            (126., 160.)
        );
        // Both sides are too small; choose the more spacious side then clamp.
        assert_eq!(
            submenu_placement(Rect::new(140., 10., 40., 20.), viewport, 180., 60., false),
            (130., 20.)
        );
        assert_eq!(
            submenu_placement(Rect::new(30., 50., 80., 20.), viewport, 400., 300., false),
            (10., 20.)
        );
    }
}
