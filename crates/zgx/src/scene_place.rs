//! A box whose own box changed (a sparkline's bar, a progress fill), placed
//! again in its parent without the parent's layout. A parent of a fixed size
//! (a layout boundary) places its absolute children apart from the rest of
//! its content: each at its margins inside the parent's content box, sized
//! as it measures there. So a change to one of them needs that child
//! measured and the parent arranged, not the parent measured again.
//! Queued as `set_box` changes the child; placed in `prepare_layout` where
//! the parent's layout would be (its place among the layout roots), with the
//! same measuring and the same arranging, so bounds, damage and repaints
//! come out as the parent's layout gives them. Anything else that changed
//! the parent or the child lays them out as before.

use super::*;

impl Scene {
    /// From `set_box`, for `id`, whose box just changed and whose reads did
    /// not: queued to be placed again in its parent, with every mark
    /// `invalidate_layout_with` makes but the layout bits. False where that
    /// would not be exact, for the caller to lay it out as any change is.
    #[inline(always)]
    pub(super) fn place_later(&mut self, id: NodeId) -> bool {
        #[cfg(test)]
        if self.no_placing {
            return false;
        }
        self.isolation_count + self.promoted_count + self.relative_count + self.contents_count == 0
            && self.tree_node(id).style.absolute
            && self.place_absolute(id)
    }
    #[inline(never)]
    fn place_absolute(&mut self, id: NodeId) -> bool {
        let node = self.tree_node(id);
        let Some(parent) = node.parent else {
            return false;
        };
        if !self.children_raw(node).is_empty()
            || node.special.any()
            || node.style.snap
            || node.dirty & LAYOUT != 0
            || node.measurement.is_none()
        {
            return false;
        }
        let owner = self.tree_node(parent);
        if !Self::is_layout_boundary(owner)
            || owner.dirty & LAYOUT != 0
            || owner.special.any()
            || owner.style.snap
            || owner.advanced_layout.is_some()
            || !owner.alternates.is_empty()
            || !owner.sizes.is_empty()
        {
            return false;
        }
        // `invalidate_layout_with(id, true, false)`'s marks, the layout bits
        // aside: its ink and its parent's cleared, both on the dirty list,
        // the parent a layout root, the ink above it cleared.
        let node = self.tree_node_mut(id);
        node.bounds_valid.set(0);
        if !node.queued {
            node.queued = true;
            self.dirty.push(id);
        }
        let owner = self.tree_node_mut(parent);
        let ink = owner.bounds_valid.replace(0);
        if !owner.queued {
            owner.queued = true;
            self.dirty.push(parent);
        }
        if self.layout_roots.last() != Some(&parent) {
            self.layout_roots.push(parent);
        }
        if ink & BOUNDS_VALID != 0 {
            let mut above = self.tree_node(parent).parent;
            while let Some(id) = above {
                let node = self.tree_node(id);
                if node.bounds_valid.replace(0) & BOUNDS_VALID == 0 {
                    break;
                }
                above = node.parent;
            }
        }
        self.placing.push((parent, id));
        true
    }
    /// As a layout pass starts: the queued boxes ordered by parent, and those
    /// this pass cannot place apart (the whole tree is laid out, or their
    /// parent or they changed otherwise since) given their layout bits, for
    /// the pass to lay them out as `set_box` would have had it.
    #[cold]
    #[inline(never)]
    pub(super) fn settle_placements(&mut self, full: bool) {
        // Their parents in index order, each one's boxes in the order they
        // changed (a stable sort): found by parent as the roots come.
        self.placing.sort_by_key(|(parent, _)| parent.index);
        let mut placing = std::mem::take(&mut self.placing);
        // Another root laid out this pass, outermost first: one above a
        // queued box's parent could move it, so that box falls back.
        let others = self
            .layout_roots
            .iter()
            .any(|root| self.contains(*root) && self.tree_node(*root).dirty & LAYOUT != 0);
        placing.retain(|&(parent, id)| {
            let apart = !full
                && self.contains(id)
                && self.contains(parent)
                && self.tree_node(id).parent == Some(parent)
                && self.tree_node(id).dirty & LAYOUT == 0
                && self.tree_node(parent).dirty & LAYOUT == 0
                && !(others && self.laid_out_above(parent));
            if !apart {
                self.lay_out_queued(parent, id);
            }
            apart
        });
        // A parent one of whose boxes fell back is laid out whole.
        placing.retain(|&(parent, id)| {
            let apart = self.tree_node(parent).dirty & LAYOUT == 0;
            if !apart {
                self.lay_out_queued(parent, id);
            }
            apart
        });
        self.placing = placing;
    }
    /// Whether an ancestor of `id` is laid out this pass.
    fn laid_out_above(&self, id: NodeId) -> bool {
        let mut above = self.tree_node(id).parent;
        while let Some(node) = above {
            if self.tree_node(node).dirty & LAYOUT != 0 {
                return true;
            }
            above = self.tree_node(node).parent;
        }
        false
    }
    /// Queued box `id` laid out as `set_box` would have had it: its layout
    /// bit and its parent's set.
    fn lay_out_queued(&mut self, parent: NodeId, id: NodeId) {
        if self.contains(id) {
            self.tree_node_mut(id).dirty |= LAYOUT;
        }
        if self.contains(parent) {
            self.tree_node_mut(parent).dirty |= LAYOUT;
        }
    }
    /// Layout root `parent`, if its only change is boxes queued in it: those
    /// measured again as its layout measures an absolute child, their places
    /// written into its result, and it arranged where it is, as
    /// `prepare_layout` arranges a root. False where it holds none, or
    /// another of its children changed (then laid out whole, the queued ones
    /// with it).
    pub(super) fn place_children(&mut self, parent: NodeId, report: &mut FrameReport) -> bool {
        let run = {
            let start = self
                .placing
                .partition_point(|(owner, _)| owner.index < parent.index);
            let end = start
                + self.placing[start..]
                    .iter()
                    .take_while(|(owner, _)| owner.index == parent.index)
                    .count();
            start..end
        };
        if run.is_empty() || self.placing[run.start].0 != parent {
            return false;
        }
        let queued: Vec<NodeId> = self.placing[run.clone()]
            .iter()
            .map(|&(_, id)| id)
            .collect();
        self.placing.drain(run);
        let owner = self.tree_node(parent);
        let last = owner.measurement.as_ref().unwrap();
        let others = owner.dirty & LAYOUT != 0
            || last.children.iter().any(|&(child, ..)| {
                self.tree_node(child).dirty & LAYOUT != 0 && !queued.contains(&child)
            });
        if others {
            for id in queued {
                self.lay_out_queued(parent, id);
            }
            return false;
        }
        // As `measure_uncached` places an absolute child: inside the content
        // box its own size leaves, from the style it resolves under the
        // constraints it was measured with.
        let constraints = last.constraints;
        let (padding, inner_w, inner_h) = self.content_box(parent, constraints);
        for at in 0..self
            .tree_node(parent)
            .measurement
            .as_ref()
            .unwrap()
            .children
            .len()
        {
            let child = self
                .tree_node(parent)
                .measurement
                .as_ref()
                .unwrap()
                .children[at]
                .0;
            if !queued.contains(&child) {
                continue;
            }
            self.tree_node_mut(child).dirty |= LAYOUT;
            let margin = self.tree_node(child).style.margin;
            let placed = Constraints {
                width: Some((inner_w - margin.left - margin.right).max(0.0)),
                height: Some((inner_h - margin.top - margin.bottom).max(0.0)),
                percent_width: Some(inner_w.max(0.0)),
                percent_height: Some(inner_h.max(0.0)),
                ..Constraints::default()
            };
            let ((w, h), placed) = self.measure_kept(child, placed);
            let last = self.tree_node_mut(parent).measurement.as_mut().unwrap();
            last.children[at] = (
                child,
                Rect::new(padding.left + margin.left, padding.top + margin.top, w, h),
                placed,
            );
        }
        // Its result this pass's, as `store_measurement` leaves a changed
        // node's with nothing else kept; then arranged as a root is.
        let pass = self.layout_pass;
        let owner = self.tree_node_mut(parent);
        owner.measurement.as_mut().unwrap().pass = pass;
        owner.arrange_dirty = true;
        owner.dirty |= LAYOUT;
        let bounds = owner.bounds;
        self.arrange_from(parent, bounds.x, bounds.y, report);
        #[cfg(test)]
        {
            self.placed_parents += 1;
        }
        true
    }
    /// The content box `measure_uncached` lays `id`'s absolute children out
    /// in, measured under `constraints`: its padding and border, and the
    /// width and height inside them.
    fn content_box(&self, id: NodeId, constraints: Constraints) -> (Insets, f32, f32) {
        let style = self.resolved_leaf_style(id, constraints);
        let padding = style.padding_edges.unwrap_or(Insets::all(style.padding));
        let border = match &self.tree_node(id).kind {
            NodeKind::Panel { quad, .. } => quad.layout_border(),
            _ => Insets::default(),
        };
        let padding = Insets {
            top: padding.top.max(0.0) + border.top,
            right: padding.right.max(0.0) + border.right,
            bottom: padding.bottom.max(0.0) + border.bottom,
            left: padding.left.max(0.0) + border.left,
        };
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
        let size = self.tree_node(id).measurement.as_ref().unwrap().size;
        let inner_w = dimension(
            explicit_w.unwrap_or(size.0),
            style.min_width,
            style.max_width,
        ) - (padding.left + padding.right);
        let inner_h = dimension(
            explicit_h.unwrap_or(size.1),
            style.min_height,
            style.max_height,
        ) - (padding.top + padding.bottom);
        (padding, inner_w, inner_h)
    }
}

#[cfg(test)]
mod tests {
    use crate::{compose::prelude::*, widgets::Ui};

    /// A dashboard's cards of bars, the bars given new heights every frame
    /// (`set_box`), and every few frames something that placing apart must
    /// not skip: a bar sized in percent, held by min and max, by an aspect
    /// ratio or zoomed; a card whose width is its content's (no boundary); a
    /// card moved; the window resized. Laid out with and without placing
    /// apart: every node's bounds, the damage, the repaints and the layout
    /// count the same, frame by frame.
    #[test]
    fn boxes_placed_apart_lay_out_and_damage_as_their_parents_layout_does() {
        let build = |no_placing: bool| {
            let mut ui = Ui::new(700., 420.);
            ui.scene.borrow_mut().no_placing = no_placing;
            let frame = ui.signal(0u32);
            let variant = ui.signal(0u8);
            let cards = (0..3).map(|card| {
                let (moved, auto) = (frame.clone(), variant.clone());
                let (frame, variant) = (frame.clone(), variant.clone());
                let bars = (0..6).map(move |bar| {
                    let (frame, variant) = (frame.clone(), variant.clone());
                    div()
                        .absolute()
                        .ml(4. + bar as f32 * 11.)
                        .w(7.)
                        .bg(rgb(0x3060c0))
                        .reactive_style(move || {
                            let f = frame.get() as f32;
                            let h = 4. + ((f * 7.3 + bar as f32 * 13.1 + card as f32 * 5.7) % 51.);
                            let s = Styles::new().mt(70. - h).h(h);
                            match (variant.get(), bar) {
                                (1, 0) => s.w_percent(6.),
                                (2, 1) => s.min_h(20.).max_h(40.),
                                (3, 2) => s.aspect_ratio(0.5),
                                (4, 3) => s.zoom(1.5),
                                _ => s,
                            }
                        })
                });
                overlay()
                    .absolute()
                    .h(100.)
                    .rounded(6.)
                    .border(1.)
                    .border_color(rgb(0x404040))
                    .bg(rgb(0x202020))
                    .p(3.5)
                    .reactive_style(move || {
                        let s = Styles::new()
                            .ml(10. + card as f32 * 220.)
                            .mt(20.25 + if moved.get() % 37 > 30 { 0.5 } else { 0. });
                        match auto.get() {
                            5 if card == 1 => s,
                            _ => s.w(200.),
                        }
                    })
                    .child(text(format!("card {card}")).absolute().size(100., 14.))
                    .children(bars)
            });
            let handle = ui.mount(overlay().w(700.).h(420.).children(cards));
            ui.prepare_frame();
            ui.scene.borrow_mut().flush();
            (ui, handle, frame, variant)
        };
        let (a, _ha, frame_a, variant_a) = build(false);
        let (b, _hb, frame_b, variant_b) = build(true);
        for f in 1..240u32 {
            frame_a.set(f);
            frame_b.set(f);
            // Zoom last: a zoomed node keeps the scene off this path after.
            if f % 9 == 0 {
                let v = if f >= 200 {
                    4
                } else {
                    [0, 1, 2, 3, 5][(f as usize / 9) % 5]
                };
                variant_a.set(v);
                variant_b.set(v);
            }
            if f % 53 == 41 {
                let width = 690. + (f % 3) as f32 * 5.;
                a.scene.borrow_mut().resize(width, 420.);
                b.scene.borrow_mut().resize(width, 420.);
            }
            a.prepare_frame();
            b.prepare_frame();
            let (ra, rb) = (a.scene.borrow_mut().flush(), b.scene.borrow_mut().flush());
            assert_eq!(ra.damage, rb.damage, "damage at frame {f}");
            assert_eq!(ra.layout_nodes, rb.layout_nodes, "layout at frame {f}");
            let (sa, sb) = (a.scene.borrow(), b.scene.borrow());
            assert_eq!(sa.repaints().1, sb.repaints().1, "repaints at frame {f}");
            let (ia, ib): (Vec<_>, Vec<_>) = (sa.ids().collect(), sb.ids().collect());
            assert_eq!(ia, ib);
            for id in ia {
                assert_eq!(sa.bounds(id), sb.bounds(id), "{id:?} at frame {f}");
            }
        }
        let (sa, sb) = (a.scene.borrow(), b.scene.borrow());
        assert!(sa.placing.is_empty());
        assert!(
            sa.placed_parents > 300,
            "{} parents placed apart",
            sa.placed_parents
        );
        assert_eq!(sb.placed_parents, 0);
    }
}
