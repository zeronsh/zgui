//! Advanced layout is entered only for containers with opted-in properties.
//! Ordinary rows/columns retain the existing allocation/cache path.
use super::*;
use crate::layout::{ContentAlign, Display as ViewDisplay, FlexWrap as ViewWrap, Length};
use taffy::{
    prelude as t,
    style_helpers::{line, span},
};
pub(super) struct AdvancedLayoutCache {
    tree: t::TaffyTree<NodeId>,
    root: t::NodeId,
    nodes: crate::scene::NodeMap<t::NodeId>,
    order: Vec<NodeId>,
    /// The percentage bases children were last measured under: taffy keys
    /// a leaf's cached size by its known and available sizes alone.
    bases: (Option<f32>, Option<f32>),
}
impl AdvancedLayoutCache {
    fn new() -> Self {
        let mut tree = t::TaffyTree::new();
        tree.disable_rounding();
        let root = tree
            .new_leaf(t::Style::default())
            .expect("empty retained layout root");
        Self {
            tree,
            root,
            nodes: Default::default(),
            order: Vec::new(),
            bases: (None, None),
        }
    }
}
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0. }
}
fn dim(value: Length) -> t::Dimension {
    match value {
        Length::Auto => t::Dimension::auto(),
        Length::Px(v) => t::Dimension::length(finite(v).max(0.)),
        Length::Percent(v) => t::Dimension::percent(finite(v).max(0.)),
    }
}
fn length(value: Length) -> t::LengthPercentage {
    match value {
        Length::Auto => t::LengthPercentage::length(0.),
        Length::Px(v) => t::LengthPercentage::length(finite(v)),
        Length::Percent(v) => t::LengthPercentage::percent(finite(v)),
    }
}
fn offset(value: Length) -> t::LengthPercentageAuto {
    match value {
        Length::Auto => t::LengthPercentageAuto::auto(),
        Length::Px(v) => t::LengthPercentageAuto::length(finite(v)),
        Length::Percent(v) => t::LengthPercentageAuto::percent(finite(v)),
    }
}
fn align(value: Align) -> t::AlignItems {
    match value {
        Align::Start => t::AlignItems::Start,
        Align::Baseline => t::AlignItems::Baseline,
        Align::Center => t::AlignItems::Center,
        Align::End => t::AlignItems::End,
        Align::Stretch => t::AlignItems::Stretch,
    }
}
fn content(value: ContentAlign) -> t::AlignContent {
    match value {
        ContentAlign::Start => t::AlignContent::Start,
        ContentAlign::End => t::AlignContent::End,
        ContentAlign::Center => t::AlignContent::Center,
        ContentAlign::Stretch => t::AlignContent::Stretch,
        ContentAlign::SpaceBetween => t::AlignContent::SpaceBetween,
        ContentAlign::SpaceAround => t::AlignContent::SpaceAround,
        ContentAlign::SpaceEvenly => t::AlignContent::SpaceEvenly,
    }
}
fn placement(start: Option<i16>, count: Option<u16>, full: bool) -> t::Line<t::GridPlacement> {
    if full {
        return t::Line {
            start: line(1),
            end: line(-1),
        };
    }
    t::Line {
        start: start
            .filter(|v| *v != 0)
            .map_or(t::GridPlacement::Auto, line),
        end: count.map_or(t::GridPlacement::Auto, |v| span(v.max(1))),
    }
}
fn style(style: &Style, layout: Layout, leaf: bool) -> t::Style {
    let options = style.layout_options.as_deref().copied().unwrap_or_default();
    let pad = style.padding_edges.unwrap_or(Insets::all(style.padding));
    let mut output = t::Style {
        display: match options.display.unwrap_or_default() {
            ViewDisplay::Grid => t::Display::Grid,
            ViewDisplay::None => t::Display::None,
            ViewDisplay::Flex => t::Display::Flex,
        },
        position: if style.absolute {
            t::Position::Absolute
        } else {
            t::Position::Relative
        },
        // An intrinsic size is the child's measured content size to taffy,
        // which a stretching parent then overrides.
        size: t::Size {
            width: style
                .width
                .filter(|_| !(leaf && style.intrinsic_width))
                .map(Length::Px)
                .or_else(|| style.width_percent.map(Length::Percent))
                .map_or(t::Dimension::auto(), dim),
            height: style
                .height
                .filter(|_| !(leaf && style.intrinsic_height))
                .map(Length::Px)
                .or_else(|| style.height_percent.map(Length::Percent))
                .map_or(t::Dimension::auto(), dim),
        },
        // Unset, CSS's automatic minimum (`auto`): a flex item shrinks no
        // further than its min-content size, and one that clips (a scroll
        // container, `overflow` below) to 0.
        min_size: t::Size {
            width: options
                .min_width
                .or_else(|| style.min_width.map(Length::Px))
                .map_or(t::Dimension::auto(), dim),
            height: options
                .min_height
                .or_else(|| style.min_height.map(Length::Px))
                .map_or(t::Dimension::auto(), dim),
        },
        overflow: overflow(super::clips(style)),
        max_size: t::Size {
            width: options
                .max_width
                .or_else(|| style.max_width.map(Length::Px))
                .map_or(t::Dimension::auto(), dim),
            height: options
                .max_height
                .or_else(|| style.max_height.map(Length::Px))
                .map_or(t::Dimension::auto(), dim),
        },
        aspect_ratio: options.aspect_ratio.filter(|v| v.is_finite() && *v > 0.),
        margin: t::Rect {
            left: offset(options.margin.left.unwrap_or(Length::Px(style.margin.left))),
            right: offset(
                options
                    .margin
                    .right
                    .unwrap_or(Length::Px(style.margin.right)),
            ),
            top: offset(options.margin.top.unwrap_or(Length::Px(style.margin.top))),
            bottom: offset(
                options
                    .margin
                    .bottom
                    .unwrap_or(Length::Px(style.margin.bottom)),
            ),
        },
        inset: t::Rect {
            left: offset(options.inset.left.unwrap_or_default()),
            right: offset(options.inset.right.unwrap_or_default()),
            top: offset(options.inset.top.unwrap_or_default()),
            bottom: offset(options.inset.bottom.unwrap_or_default()),
        },
        padding: if leaf {
            taffy::style_helpers::zero()
        } else {
            t::Rect {
                left: length(options.padding.left.unwrap_or(Length::Px(pad.left))),
                right: length(options.padding.right.unwrap_or(Length::Px(pad.right))),
                top: length(options.padding.top.unwrap_or(Length::Px(pad.top))),
                bottom: length(options.padding.bottom.unwrap_or(Length::Px(pad.bottom))),
            }
        },
        gap: t::Size {
            width: length(options.gap_x.unwrap_or(Length::Px(style.gap))),
            height: length(options.gap_y.unwrap_or(Length::Px(style.gap))),
        },
        flex_direction: match (layout, options.reverse.unwrap_or(false)) {
            (Layout::Row, false) => t::FlexDirection::Row,
            (Layout::Row, true) => t::FlexDirection::RowReverse,
            (_, false) => t::FlexDirection::Column,
            (_, true) => t::FlexDirection::ColumnReverse,
        },
        flex_wrap: match options.wrap.unwrap_or_default() {
            ViewWrap::NoWrap => t::FlexWrap::NoWrap,
            ViewWrap::Wrap => t::FlexWrap::Wrap,
            ViewWrap::Reverse => t::FlexWrap::WrapReverse,
        },
        flex_grow: finite(style.flex_grow).max(0.),
        flex_shrink: finite(style.flex_shrink).max(0.),
        flex_basis: options.basis.map_or(t::Dimension::auto(), dim),
        align_items: Some(align(style.align)),
        align_self: options.align_self.map(align),
        align_content: options.align_content.map(content),
        justify_content: Some(match style.justify {
            // As CSS's `justify-content: stretch` acts in flexbox.
            Justify::Start | Justify::Stretch => t::JustifyContent::Start,
            Justify::End => t::JustifyContent::End,
            Justify::Center => t::JustifyContent::Center,
            Justify::SpaceBetween => t::JustifyContent::SpaceBetween,
            Justify::SpaceAround => t::JustifyContent::SpaceAround,
            Justify::SpaceEvenly => t::JustifyContent::SpaceEvenly,
        }),
        grid_template_columns: options.columns.map_or_else(Vec::new, |count| {
            taffy::style_helpers::evenly_sized_tracks(count.max(1))
        }),
        grid_template_rows: options.rows.map_or_else(Vec::new, |count| {
            taffy::style_helpers::evenly_sized_tracks(count.max(1))
        }),
        grid_column: placement(
            options.column_start,
            options.column_span,
            options.column_full.unwrap_or(false),
        ),
        grid_row: placement(
            options.row_start,
            options.row_span,
            options.row_full.unwrap_or(false),
        ),
        ..Default::default()
    };
    // A leaf's natural padding and border are included by the retained child
    // measurement. An overlay stacks its children as CSS grid stacks items
    // in one area: a single track each way, its children aligned there on
    // their own, `align` across (`justify-items`) and `justify` down
    // (`align-items`). Explicit Grid overrides this policy.
    if !leaf && layout == Layout::Overlay && options.display != Some(ViewDisplay::Grid) {
        output.display = t::Display::Grid;
        output.grid_template_columns = taffy::style_helpers::evenly_sized_tracks(1);
        output.grid_template_rows = taffy::style_helpers::evenly_sized_tracks(1);
        output.justify_items = Some(match style.align {
            Align::Baseline => t::AlignItems::Start,
            other => align(other),
        });
        output.align_items = Some(overlay_down(style.justify));
        output.justify_content = None;
    }
    output
}
/// An overlay's vertical alignment of each child (see `Justify`).
fn overlay_down(value: Justify) -> t::AlignItems {
    match value {
        Justify::Start | Justify::SpaceBetween => t::AlignItems::Start,
        Justify::Center | Justify::SpaceAround | Justify::SpaceEvenly => t::AlignItems::Center,
        Justify::End => t::AlignItems::End,
        Justify::Stretch => t::AlignItems::Stretch,
    }
}
/// A child's style in its overlay parent: in the one grid area, its own
/// `align_self` across.
fn stacked(mut mapped: t::Style) -> t::Style {
    if mapped.position != t::Position::Absolute {
        mapped.grid_row = t::Line {
            start: line(1),
            end: span(1),
        };
        mapped.grid_column = t::Line {
            start: line(1),
            end: span(1),
        };
    }
    mapped.justify_self = mapped.align_self.take();
    mapped
}
/// Taffy's overflow for a node that clips, or not: clipping on either axis
/// makes it a scroll container, whose automatic minimum is 0.
fn overflow(clips: bool) -> taffy::geometry::Point<taffy::style::Overflow> {
    let overflow = if clips {
        taffy::style::Overflow::Hidden
    } else {
        taffy::style::Overflow::Visible
    };
    taffy::geometry::Point {
        x: overflow,
        y: overflow,
    }
}
fn available(value: Option<f32>) -> t::AvailableSpace {
    value.map_or(t::AvailableSpace::MaxContent, |v| {
        t::AvailableSpace::Definite(finite(v).max(0.))
    })
}
/// The width or height a child is laid out within for a probe: a min-content
/// probe lays it out with no room at all, so text and wrapping rows break at
/// every opportunity and report their widest unbreakable piece. Treating it
/// as unbounded (max-content) made a shrink-to-fit box as wide as its
/// unwrapped content: a wrapping row inside a narrower column overflowed it.
fn bound(value: t::AvailableSpace) -> Option<f32> {
    match value {
        t::AvailableSpace::Definite(v) => Some(v),
        t::AvailableSpace::MinContent => Some(0.),
        t::AvailableSpace::MaxContent => None,
    }
}
impl Scene {
    /// Lay `cache`'s tree out within `space`, each child (of `ids`, keyed
    /// `keys`) measured through `Scene::measure` with percentages against
    /// `bases`.
    fn compute_retained(
        &mut self,
        cache: &mut AdvancedLayoutCache,
        ids: &[NodeId],
        keys: &[t::NodeId],
        space: t::Size<t::AvailableSpace>,
        bases: (Option<f32>, Option<f32>),
    ) {
        self.rebase(cache, ids, keys, bases);
        let root = cache.root;
        cache
            .tree
            .compute_layout_with_measure(root, space, |known, space, _, context, _| {
                let Some(child) = context.copied() else {
                    return t::Size::ZERO;
                };
                let size = self.measure(
                    child,
                    Constraints {
                        width: known.width.or_else(|| bound(space.width)),
                        height: known.height.or_else(|| bound(space.height)),
                        percent_width: bases.0,
                        percent_height: bases.1,
                        force_width: known.width,
                        force_height: known.height,
                    },
                );
                t::Size {
                    width: size.0,
                    height: size.1,
                }
            })
            .expect("retained tree is structurally valid");
    }
    /// Where the root's in-flow children end along its main axis (`down`:
    /// a column's), with their margins and its padding: its size when
    /// sized by its content.
    fn main_extent(
        &self,
        cache: &AdvancedLayoutCache,
        ids: &[NodeId],
        keys: &[t::NodeId],
        down: bool,
    ) -> f32 {
        let root = cache.tree.layout(cache.root).expect("computed root");
        let mut end = 0_f32;
        for (child, key) in ids.iter().zip(keys) {
            if self.node(*child).style.absolute {
                continue;
            }
            let layout = cache.tree.layout(*key).expect("computed child");
            end = end.max(match down {
                true => layout.location.y + layout.size.height + layout.margin.bottom,
                false => layout.location.x + layout.size.width + layout.margin.right,
            });
        }
        end + match down {
            true => root.padding.bottom + root.border.bottom,
            false => root.padding.right + root.border.right,
        }
    }
    /// Children resolve percentages against `bases` now: drop taffy's cached
    /// sizes of those that do, if the bases changed.
    fn rebase(
        &self,
        cache: &mut AdvancedLayoutCache,
        ids: &[NodeId],
        keys: &[t::NodeId],
        bases: (Option<f32>, Option<f32>),
    ) {
        if cache.bases == bases {
            return;
        }
        let (width, height) = (cache.bases.0 != bases.0, cache.bases.1 != bases.1);
        cache.bases = bases;
        for (child, key) in ids.iter().zip(keys) {
            let (widths, heights) = self.percent_bases(*child);
            if (width && widths) || (height && heights) {
                cache
                    .tree
                    .mark_dirty(*key)
                    .expect("retained child invalidation");
            }
        }
    }
    /// Resolved padding from the most recent layout, including percentages.
    pub fn padding(&self, id: NodeId) -> Insets {
        let constraints = self
            .node(id)
            .measurement
            .as_ref()
            .map_or(Constraints::default(), |m| m.constraints);
        self.resolved_padding(id, constraints)
    }
    fn resolved_padding(&self, id: NodeId, constraints: Constraints) -> Insets {
        let node = self.node(id);
        let mut padding = node
            .style
            .padding_edges
            .unwrap_or(Insets::all(node.style.padding));
        let Some(options) = node.style.layout_options.as_deref() else {
            return padding;
        };
        let basis = if node.parent.is_none() {
            Some(self.viewport.width)
        } else {
            constraints.percent_width
        };
        for (target, value) in [
            (&mut padding.left, options.padding.left),
            (&mut padding.right, options.padding.right),
            (&mut padding.top, options.padding.top),
            (&mut padding.bottom, options.padding.bottom),
        ] {
            if let Some(value) = value {
                *target = match value {
                    Length::Auto => 0.,
                    Length::Px(v) => finite(v),
                    Length::Percent(v) => percentage(Some(v), basis).unwrap_or(0.),
                }
                .max(0.);
            }
        }
        padding
    }
    pub(super) fn resolved_leaf_style(&self, id: NodeId, constraints: Constraints) -> Style {
        let mut style = self.node(id).style.clone();
        let Some(options) = style.layout_options.as_deref() else {
            return style;
        };
        let resolve = |value: Length, basis: Option<f32>| match value {
            Length::Auto => None,
            Length::Px(value) => Some(finite(value)),
            Length::Percent(value) => percentage(Some(value), basis),
        };
        for (target, value, basis) in [
            (
                &mut style.min_width,
                options.min_width,
                constraints.percent_width,
            ),
            (
                &mut style.max_width,
                options.max_width,
                constraints.percent_width,
            ),
            (
                &mut style.min_height,
                options.min_height,
                constraints.percent_height,
            ),
            (
                &mut style.max_height,
                options.max_height,
                constraints.percent_height,
            ),
        ] {
            if let Some(value) = value {
                *target = resolve(value, basis);
            }
        }
        style.padding_edges = Some(self.resolved_padding(id, constraints));
        style
    }
    pub(super) fn uses_advanced_layout(&self, id: NodeId) -> bool {
        // By reference: most nodes have no options, and a copy is ~170 bytes.
        let own = self
            .node(id)
            .style
            .layout_options
            .as_deref()
            .is_some_and(|options| {
                matches!(options.display, Some(ViewDisplay::Grid | ViewDisplay::None))
                    || options.wrap.is_some()
                    || options.reverse == Some(true)
                    || options.gap_x.is_some()
                    || options.gap_y.is_some()
                    || options.align_content.is_some()
            });
        own || self.node(id).style.align == Align::Baseline
            || self.node(id).children.iter().any(|child| {
                self.node(*child)
                    .style
                    .layout_options
                    .as_deref()
                    .is_some_and(|options| {
                        let mut options = *options;
                        options.visible = None;
                        options.clip_x = None;
                        options.clip_y = None;
                        !options.is_empty()
                    })
            })
    }
    pub(super) fn measure_hidden(&mut self, id: NodeId, constraints: Constraints) -> (f32, f32) {
        self.node_mut(id).advanced_layout = None;
        let ids = self.node(id).children.clone();
        let hidden = Constraints {
            force_width: Some(f32::NAN),
            ..Constraints::default()
        };
        let children = ids
            .into_iter()
            .map(|child| {
                crate::deep(0, || self.measure_hidden(child, hidden));
                (child, Rect::default())
            })
            .collect::<Vec<_>>();
        let asked = vec![Some(hidden); children.len()];
        let constraints = self.relevant(id, constraints);
        self.store_measurement(id, constraints, (0., 0.), children, asked);
        (0., 0.)
    }
    pub(super) fn measure_advanced(
        &mut self,
        id: NodeId,
        layout: Layout,
        constraints: Constraints,
    ) -> (f32, f32) {
        let root_style = self.resolved_leaf_style(id, constraints);
        let root_options = root_style
            .layout_options
            .as_deref()
            .copied()
            .unwrap_or_default();
        if root_options.display == Some(ViewDisplay::None) {
            return self.measure_hidden(id, constraints);
        }
        let mut cache = self
            .node_mut(id)
            .advanced_layout
            .take()
            .unwrap_or_else(|| Box::new(AdvancedLayoutCache::new()));
        let mut ids = self.node(id).children.clone();
        // Those whose containing block is further up are placed by it.
        if self.relative_count > 0 {
            ids.retain(|child| !self.escapes(*child));
        }
        let live: std::collections::HashSet<_> = ids.iter().copied().collect();
        let removed: Vec<_> = cache
            .nodes
            .keys()
            .filter(|id| !live.contains(id))
            .copied()
            .collect();
        for removed in removed {
            let key = cache.nodes.remove(&removed).unwrap();
            cache.tree.remove(key).expect("owned retained layout child");
        }
        let children: Vec<_> = ids
            .iter()
            .map(|child| {
                let mut mapped = style(&self.node(*child).style, Layout::Column, true);
                // A `contents` child clips as the child it lays out as.
                mapped.overflow =
                    overflow(super::clips(&self.node(self.laid_out_as(*child)).style));
                if layout == Layout::Overlay && root_options.display != Some(ViewDisplay::Grid) {
                    mapped = stacked(mapped);
                }
                if let Some(key) = cache.nodes.get(child).copied() {
                    if cache.tree.style(key).expect("retained style") != &mapped {
                        cache
                            .tree
                            .set_style(key, mapped)
                            .expect("retained style update");
                    }
                    if self.node(*child).dirty & LAYOUT != 0 {
                        cache
                            .tree
                            .mark_dirty(key)
                            .expect("retained child invalidation");
                    }
                    key
                } else {
                    let key = cache
                        .tree
                        .new_leaf_with_context(mapped, *child)
                        .expect("retained leaf");
                    cache.nodes.insert(*child, key);
                    key
                }
            })
            .collect();
        let root = cache.root;
        if cache.order != ids {
            cache
                .tree
                .set_children(root, &children)
                .expect("retained children");
            cache.order.clone_from(&ids);
        }
        let mut mapping_style = root_style.clone();
        let mut mapping_options = root_options;
        mapping_options.padding = Default::default();
        mapping_options.min_width = None;
        mapping_options.max_width = None;
        mapping_options.min_height = None;
        mapping_options.max_height = None;
        mapping_style.layout_options = Some(Arc::new(mapping_options));
        let mut mapped = style(&mapping_style, layout, false);
        mapped.position = t::Position::Relative;
        mapped.margin = taffy::style_helpers::zero();
        mapped.inset = t::Rect::auto();
        // Inside its size, around its padding (border-box); its absolute
        // children's insets count from inside it.
        let border = self.layout_border(id);
        mapped.border = t::Rect {
            left: t::LengthPercentage::length(border.left),
            right: t::LengthPercentage::length(border.right),
            top: t::LengthPercentage::length(border.top),
            bottom: t::LengthPercentage::length(border.bottom),
        };
        mapped.size.width = constraints
            .force_width
            .or(root_style.width)
            .or_else(|| percentage(root_style.width_percent, constraints.percent_width))
            .map_or(t::Dimension::auto(), |v| dim(Length::Px(v)));
        mapped.size.height = constraints
            .force_height
            .or(root_style.height)
            .or_else(|| percentage(root_style.height_percent, constraints.percent_height))
            .map_or(t::Dimension::auto(), |v| dim(Length::Px(v)));
        // An overlay sized by its content is as large as its largest in-flow
        // child, measured within its content box (as the ordinary path
        // does); its one grid area is then definite, and its children align
        // and stretch in it.
        if layout == Layout::Overlay
            && root_options.display != Some(ViewDisplay::Grid)
            && (mapped.size.width.is_auto() || mapped.size.height.is_auto())
        {
            let padding = self.resolved_padding(id, constraints);
            let inset_w = padding.left + padding.right + border.left + border.right;
            let inset_h = padding.top + padding.bottom + border.top + border.bottom;
            let (known_w, known_h) = (
                mapped.size.width.into_option(),
                mapped.size.height.into_option(),
            );
            let room = |known: Option<f32>, available: Option<f32>, inset: f32| {
                known.or(available).map(|v| (v - inset).max(0.))
            };
            let natural_constraints = Constraints {
                width: room(known_w, constraints.width, inset_w),
                height: room(known_h, constraints.height, inset_h),
                percent_width: known_w.map(|v| (v - inset_w).max(0.)),
                percent_height: known_h.map(|v| (v - inset_h).max(0.)),
                ..Default::default()
            };
            let mut natural = (0_f32, 0_f32);
            for child in &ids {
                let (absolute, margin) = {
                    let style = &self.node(*child).style;
                    (style.absolute, style.margin)
                };
                if absolute {
                    continue;
                }
                let size = self.measure(*child, natural_constraints);
                natural.0 = natural.0.max(size.0 + margin.left + margin.right);
                natural.1 = natural.1.max(size.1 + margin.top + margin.bottom);
            }
            if mapped.size.width.is_auto() {
                mapped.size.width = dim(Length::Px(natural.0 + inset_w));
            }
            if mapped.size.height.is_auto() {
                mapped.size.height = dim(Length::Px(natural.1 + inset_h));
            }
        }
        let auto_width = mapped.size.width.is_auto();
        if cache.tree.style(root).expect("retained root style") != &mapped {
            cache
                .tree
                .set_style(root, mapped)
                .expect("retained root style update");
        }
        // Percentages count from the content box: inside padding and border.
        let padding = {
            let p = self.resolved_padding(id, constraints);
            Insets {
                top: p.top.max(0.) + border.top,
                right: p.right.max(0.) + border.right,
                bottom: p.bottom.max(0.) + border.bottom,
                left: p.left.max(0.) + border.left,
            }
        };
        let mut child_percent_width = constraints
            .force_width
            .or(root_style.width)
            .or_else(|| percentage(root_style.width_percent, constraints.percent_width))
            .map(|width| (width - padding.left - padding.right).max(0.));
        let child_percent_height = constraints
            .force_height
            .or(root_style.height)
            .or_else(|| percentage(root_style.height_percent, constraints.percent_height))
            .map(|height| (height - padding.top - padding.bottom).max(0.));
        // A root of auto width learns it from its children, measured before
        // it is known: their percentage widths resolve to nothing and their
        // text lays out unwrapped, so the heights it keeps are one-line
        // heights. A second pass at the width found gives them their basis:
        // a capped bubble is as tall as the text it wraps.
        let column = !matches!(
            cache
                .tree
                .style(root)
                .expect("retained root style")
                .flex_direction,
            t::FlexDirection::Row | t::FlexDirection::RowReverse
        );
        let flex = root_options.display.unwrap_or_default() == ViewDisplay::Flex
            && root_options.wrap.is_none();
        for pass in 0..2 {
            let available_space = t::Size {
                width: available(constraints.width),
                height: available(constraints.height),
            };
            let bases = (child_percent_width, child_percent_height);
            self.compute_retained(&mut cache, &ids, &children, available_space, bases);
            if pass == 1 || !auto_width {
                break;
            }
            let mut found = cache.tree.layout(root).expect("computed root").size.width;
            if !column && flex {
                found = found.max(self.main_extent(&cache, &ids, &children, false));
            }
            // Taffy lays an auto-width root out at its longest line's length,
            // as a parent that sizes it would first; shrunk to fit here, it
            // is no wider than the room given unless its content cannot
            // shrink (CSS fit-content: max-content, at most the room, at
            // least min-content). A wrapping row in a narrower column wraps.
            if let Some(room) = constraints.width
                && found > room + 0.01
            {
                let least_space = t::Size {
                    width: t::AvailableSpace::MinContent,
                    ..available_space
                };
                let bases = (None, child_percent_height);
                self.compute_retained(&mut cache, &ids, &children, least_space, bases);
                let least = cache.tree.layout(root).expect("computed root").size.width;
                found = found.min(least.max(room));
            }
            let mut fixed = cache.tree.style(root).expect("retained root style").clone();
            fixed.size.width = t::Dimension::length(found);
            cache
                .tree
                .set_style(root, fixed)
                .expect("retained root style update");
            child_percent_width = Some((found - padding.left - padding.right).max(0.));
        }
        // An auto height probed at max-content: taffy sizes it from each
        // item's contribution, which a negative margin (a layout animation
        // growing a box open) turns into a vast shrink, collapsing the
        // column while placing its items right. It is as tall as where they
        // end, as a definite probe finds.
        if column
            && flex
            && constraints.height.is_none()
            && cache
                .tree
                .style(root)
                .expect("retained root style")
                .size
                .height
                .is_auto()
        {
            let extent = self.main_extent(&cache, &ids, &children, true);
            if extent > cache.tree.layout(root).expect("computed root").size.height + 0.01 {
                let mut fixed = cache.tree.style(root).expect("retained root style").clone();
                fixed.size.height = t::Dimension::length(extent);
                cache
                    .tree
                    .set_style(root, fixed)
                    .expect("retained root style update");
                let available_space = t::Size {
                    width: available(constraints.width),
                    height: available(constraints.height),
                };
                let bases = (child_percent_width, child_percent_height);
                self.compute_retained(&mut cache, &ids, &children, available_space, bases);
            }
        }
        let root_layout = *cache.tree.layout(root).expect("computed root");
        let inner_width = (root_layout.size.width
            - root_layout.padding.left
            - root_layout.padding.right
            - root_layout.border.left
            - root_layout.border.right)
            .max(0.);
        let inner_height = (root_layout.size.height
            - root_layout.padding.top
            - root_layout.padding.bottom
            - root_layout.border.top
            - root_layout.border.bottom)
            .max(0.);
        let mut asked = Vec::with_capacity(ids.len());
        let positions = ids
            .into_iter()
            .zip(children)
            .map(|(child, key)| {
                let child_layout = *cache.tree.layout(key).expect("computed child");
                let child_constraints = Constraints {
                    width: Some(child_layout.size.width),
                    height: Some(child_layout.size.height),
                    percent_width: Some(inner_width),
                    percent_height: Some(inner_height),
                    force_width: Some(child_layout.size.width),
                    force_height: Some(child_layout.size.height),
                };
                if self
                    .node(child)
                    .style
                    .layout_options
                    .as_deref()
                    .is_some_and(|options| options.display == Some(ViewDisplay::None))
                {
                    self.measure_hidden(child, child_constraints);
                } else {
                    self.measure(child, child_constraints);
                }
                asked.push(Some(child_constraints));
                (
                    child,
                    Rect::new(
                        child_layout.location.x,
                        child_layout.location.y,
                        child_layout.size.width,
                        child_layout.size.height,
                    ),
                )
            })
            .collect();
        let size = (root_layout.size.width, root_layout.size.height);
        self.store_measurement(id, constraints, size, positions, asked);
        self.node_mut(id).advanced_layout = Some(cache);
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_cache_survives_changes_and_removes_stale_children() {
        let mut scene = Scene::new(500., 300.);
        let root = scene.root();
        let grid = scene.append(
            root,
            NodeKind::Container(Layout::Column),
            Style {
                width: Some(300.),
                height: Some(200.),
                layout_options: Some(Arc::new(crate::layout::LayoutOptions {
                    display: Some(ViewDisplay::Grid),
                    columns: Some(2),
                    ..Default::default()
                })),
                ..Default::default()
            },
        );
        let first = scene.append(
            grid,
            NodeKind::Container(Layout::Column),
            Style {
                height: Some(20.),
                ..Default::default()
            },
        );
        let second = scene.append(
            grid,
            NodeKind::Container(Layout::Column),
            Style {
                height: Some(30.),
                ..Default::default()
            },
        );
        scene.flush();
        let cache = scene.node(grid).advanced_layout.as_ref().unwrap();
        let pointer = (&**cache) as *const AdvancedLayoutCache;
        let second_key = cache.nodes[&second];
        assert!(scene.node(first).advanced_layout.is_none());
        assert_eq!(scene.flush().layout_nodes, 0);
        let mut style = scene.style(first);
        style.height = Some(40.);
        scene.set_style(first, style);
        scene.flush();
        assert_eq!(
            (&**scene.node(grid).advanced_layout.as_ref().unwrap()) as *const AdvancedLayoutCache,
            pointer
        );
        assert_eq!(
            scene.node(grid).advanced_layout.as_ref().unwrap().nodes[&second],
            second_key
        );
        scene.remove(first);
        scene.flush();
        let cache = scene.node(grid).advanced_layout.as_ref().unwrap();
        assert!(!cache.nodes.contains_key(&first));
        assert_eq!(cache.nodes.len(), 1);
        assert_eq!(cache.nodes[&second], second_key);
    }
}
