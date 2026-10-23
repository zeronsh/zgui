//! Placement for a retained popover portal; ownership and focus live in compose_modal.
use crate::{
    scene::{NodeId, Rect, Transform},
    widgets::Ui,
};

/// Which side of its trigger an anchored popover or menu opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}
/// How it lines up along that side: the trigger's start edge, centre or
/// end edge (left/top, middle, right/bottom).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Start,
    Center,
    End,
}
/// Where an anchored popover or menu opens: on `side` of the trigger,
/// `offset` points away, aligned by `align`. It flips to the opposite side
/// when the preferred one lacks room, and stays inside the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub side: Side,
    pub align: Edge,
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
) {
    mount_placement(ui, owner, anchor, overlay, position, panel, Some(at), over);
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
    mount_placement(ui, owner, anchor, overlay, position, panel, None, None);
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
) {
    let anchor_bounds = ui.observe_bounds(anchor);
    let viewport = ui.observe_bounds(overlay);
    let panel_bounds = ui.observe_bounds(panel);
    let position_bounds = ui.observe_bounds(position);
    let screens = ui.popup_screens();
    let weak = ui.downgrade();
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
        let desired = match (at, item) {
            (_, Some(item)) => over_placement(anchor, viewport, panel.width, panel.height, item),
            (None, _) => submenu_placement(anchor, viewport, panel.width, panel.height),
            (Some(at), None) => placement(anchor, viewport, panel.width, panel.height, at),
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
    });
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

fn placement(anchor: Rect, viewport: Rect, width: f32, height: f32, at: Placement) -> (f32, f32) {
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
    // Across it: lined up with the trigger's start, centre or end.
    let across = |start: f32, size: f32, extent: f32| match at.align {
        Edge::Start => start,
        Edge::Center => start + (extent - size) / 2.,
        Edge::End => start + extent - size,
    };
    let (x, y) = match at.side {
        Side::Bottom | Side::Top => (
            across(anchor.x, width, anchor.width),
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
            across(anchor.y, height, anchor.height),
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
/// trigger, lined up with it, kept on the screen.
fn over_placement(anchor: Rect, viewport: Rect, width: f32, height: f32, item: Rect) -> (f32, f32) {
    let x = anchor.x - item.x;
    let y = anchor.y + (anchor.height - item.height) / 2. - item.y;
    let right = viewport.x + viewport.width;
    let bottom = viewport.y + viewport.height;
    (
        x.clamp(viewport.x, (right - width).max(viewport.x)),
        y.clamp(viewport.y, (bottom - height).max(viewport.y)),
    )
}

fn submenu_placement(anchor: Rect, viewport: Rect, width: f32, height: f32) -> (f32, f32) {
    let right_edge = viewport.x + viewport.width;
    let right = anchor.x + anchor.width + 4.;
    let left = anchor.x - width - 4.;
    let right_room = right_edge - right;
    let left_room = anchor.x - 4. - viewport.x;
    let x = if right_room >= width || right_room >= left_room {
        right
    } else {
        left
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
            placement(trigger, viewport, 120., 60., Placement::default()),
            (100., 74.)
        );
        // End-aligned: the panel's right edge on the trigger's.
        assert_eq!(
            placement(trigger, viewport, 120., 60., at(Side::Bottom, Edge::End)),
            (60., 74.)
        );
        assert_eq!(
            placement(trigger, viewport, 120., 60., at(Side::Bottom, Edge::Center)),
            (80., 74.)
        );
        // Above, with no room above: below instead.
        assert_eq!(
            placement(trigger, viewport, 120., 60., at(Side::Top, Edge::Start)),
            (100., 74.)
        );
        // Right of it, centred on it.
        assert_eq!(
            placement(trigger, viewport, 120., 60., at(Side::Right, Edge::Center)),
            (184., 30.)
        );
        // Near the bottom edge: flipped above.
        let low = Rect::new(100., 260., 80., 20.);
        assert_eq!(
            placement(low, viewport, 120., 60., Placement::default()),
            (100., 196.)
        );
    }
    #[test]
    fn a_popup_button_opens_with_its_item_over_the_trigger() {
        let viewport = Rect::new(0., 0., 400., 300.);
        let trigger = Rect::new(100., 150., 120., 24.);
        // The third 24-point row (after 5 points of padding) of a menu.
        let item = Rect::new(5., 53., 110., 24.);
        assert_eq!(
            over_placement(trigger, viewport, 120., 180., item),
            (95., 97.)
        );
        // Near the top, kept on the screen.
        let high = Rect::new(100., 10., 120., 24.);
        assert_eq!(over_placement(high, viewport, 120., 180., item), (95., 0.));
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
            submenu_placement(Rect::new(30., 50., 80., 20.), viewport, 100., 60.),
            (114., 50.)
        );
        assert_eq!(
            submenu_placement(Rect::new(230., 190., 60., 20.), viewport, 100., 60.),
            (126., 160.)
        );
        // Both sides are too small; choose the more spacious side then clamp.
        assert_eq!(
            submenu_placement(Rect::new(140., 10., 40., 20.), viewport, 180., 60.),
            (130., 20.)
        );
        assert_eq!(
            submenu_placement(Rect::new(30., 50., 80., 20.), viewport, 400., 300.),
            (10., 20.)
        );
    }
}
