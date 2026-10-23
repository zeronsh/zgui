//! Bounded keyboard navigation over the currently mounted virtual-list rows.
use crate::{
    input::{EventPhase, InputEvent, Key},
    reactive::Signal,
    scene::{Layout, NodeId, NodeKind, QuadStyle},
    semantics::{Role, SemanticNode},
    widgets::Ui,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Rows {
    length: usize,
    /// Each mounted row: its index, its slot, and the item focus goes to
    /// (the slot, or the app's row when that takes focus itself).
    visible: Vec<(usize, NodeId, NodeId)>,
}
pub(crate) struct Navigation {
    pub request: Signal<(u64, usize)>,
    rows: Rc<RefCell<Rows>>,
}
enum Geometry {
    Fixed(f32),
    Variable(crate::compose::VariableHeights),
}
impl Navigation {
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        offset: Signal<f32>,
        size: Signal<(f32, f32)>,
        row_height: f32,
    ) -> Self {
        Self::with_geometry(ui, root, offset, size, Geometry::Fixed(row_height))
    }
    pub fn new_variable(
        ui: &mut Ui,
        root: NodeId,
        offset: Signal<f32>,
        size: Signal<(f32, f32)>,
        heights: crate::compose::VariableHeights,
    ) -> Self {
        Self::with_geometry(ui, root, offset, size, Geometry::Variable(heights))
    }
    fn with_geometry(
        ui: &mut Ui,
        root: NodeId,
        offset: Signal<f32>,
        size: Signal<(f32, f32)>,
        geometry: Geometry,
    ) -> Self {
        // Its rows are one Tab stop: the row last focused, while mounted.
        ui.input.set_tab_stop_group(root);
        let request = ui.signal((0_u64, 0_usize));
        let write = request.clone();
        let rows = Rc::new(RefCell::new(Rows::default()));
        let state = rows.clone();
        ui.on_event(root, true, move |cx| {
            if cx.phase == EventPhase::Capture || cx.default_prevented() {
                return;
            }
            let InputEvent::KeyDown { key, .. } = &cx.event else {
                return;
            };
            let rows = state.borrow();
            let current = rows
                .visible
                .iter()
                .find(|(_, slot, item)| *slot == cx.target || *item == cx.target)
                .map(|(index, _, _)| *index);
            // Interactive descendants keep their own editing/control key behavior.
            if cx.target != root && current.is_none() {
                return;
            }
            if rows.length == 0 {
                return;
            }
            let visible_start = match &geometry {
                Geometry::Fixed(row_height) => ((offset.get() as f64 / *row_height as f64).floor()
                    as usize)
                    .min(rows.length - 1),
                Geometry::Variable(heights) => heights
                    .row_at(offset.get())
                    .unwrap_or(0)
                    .min(rows.length - 1),
            };
            let base = current.unwrap_or(visible_start);
            let (previous_page, next_page) = match &geometry {
                Geometry::Fixed(row_height) => {
                    let page = ((size.get().1 as f64 / *row_height as f64).floor() as usize).max(1);
                    (base.saturating_sub(page), base.saturating_add(page))
                }
                Geometry::Variable(heights) => {
                    let top = heights.row_offset(base);
                    let viewport = size.get().1.max(0.);
                    let previous = heights.row_at((top - viewport).max(0.)).unwrap_or(0);
                    let next = heights
                        .row_at((top as f64 + viewport as f64).min(f32::MAX as f64) as f32)
                        .unwrap_or(0);
                    (
                        previous.min(base.saturating_sub(1)),
                        next.max(base.saturating_add(1)),
                    )
                }
            };
            let target = match key {
                Key::Home => 0,
                Key::End => rows.length - 1,
                Key::ArrowDown => current.map_or(visible_start, |index| index.saturating_add(1)),
                Key::ArrowUp => current.map_or(visible_start, |index| index.saturating_sub(1)),
                Key::PageDown => next_page,
                Key::PageUp => previous_page,
                _ => return,
            }
            .min(rows.length - 1);
            drop(rows);
            let serial = write.get().0.wrapping_add(1);
            write.set((serial, target));
            cx.prevent_default();
            cx.stop_propagation();
        });
        Self { request, rows }
    }
    pub fn mount_row(&self, ui: &mut Ui, row: NodeId, _row_height: f32) {
        let theme = ui.theme_scope();
        let marker = ui.scene.borrow_mut().append(
            row,
            NodeKind::Rect(theme.color_untracked(crate::theme::tok::ACCENT)),
            crate::scene::Style {
                width: Some(2.),
                height_percent: Some(1.),
                absolute: true,
                ..Default::default()
            },
        );
        ui.scene.borrow_mut().set_effects(
            marker,
            crate::scene::Effects {
                opacity: 0.,
                ..Default::default()
            },
        );
        let scene = ui.scene.clone();
        ui.on_event(row, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            match cx.event {
                InputEvent::Focus => {
                    scene
                        .borrow_mut()
                        .set_effects(marker, crate::scene::Effects::default());
                    scene.borrow_mut().set_kind(
                        row,
                        NodeKind::Panel {
                            layout: Layout::Overlay,
                            quad: QuadStyle {
                                fill: theme.color_untracked(crate::theme::tok::BUTTON_HOVER),
                                ..Default::default()
                            },
                        },
                    );
                }
                InputEvent::Blur => {
                    scene.borrow_mut().set_effects(
                        marker,
                        crate::scene::Effects {
                            opacity: 0.,
                            ..Default::default()
                        },
                    );
                    scene
                        .borrow_mut()
                        .set_kind(row, NodeKind::Container(Layout::Overlay));
                }
                _ => {}
            }
        });
    }
    pub fn update(&self, ui: &Ui, root: NodeId, length: usize, visible: Vec<(usize, NodeId)>) {
        let mut semantics = ui.semantics.borrow_mut();
        semantics.update(root, |node| node.size_of_set = Some(length));
        let mut rows = Vec::with_capacity(visible.len());
        for (index, slot) in visible {
            // An app's row that says it is the item (`role` option or list
            // item) and takes focus is the item: focus goes to it, and it
            // says where it is. Other controls keep their own keys.
            let own = ui
                .scene
                .borrow()
                .children(slot)
                .first()
                .copied()
                .filter(|row| {
                    ui.input.options(*row).is_some_and(|o| o.focusable)
                        && semantics
                            .get(*row)
                            .is_some_and(|n| matches!(n.role, Role::ListBoxOption | Role::ListItem))
                });
            let (position, size) = (Some(index.saturating_add(1)), Some(length));
            match own {
                Some(row) => {
                    if let Some(options) = ui.input.options(slot).filter(|o| o.focusable) {
                        ui.input.set_options(
                            slot,
                            crate::input::NodeInput {
                                focusable: false,
                                ..options
                            },
                        );
                    }
                    semantics.remove(slot);
                    semantics.update(row, |node| {
                        node.position_in_set = position;
                        node.size_of_set = size;
                    });
                }
                None => {
                    let mut semantic = SemanticNode::new(Role::ListItem, "");
                    semantic.position_in_set = position;
                    semantic.size_of_set = size;
                    semantics.set(slot, semantic);
                }
            }
            rows.push((index, slot, own.unwrap_or(slot)));
        }
        *self.rows.borrow_mut() = Rows {
            length,
            visible: rows,
        };
    }
    /// Where focus goes for the row in `slot`.
    pub fn item(&self, slot: NodeId) -> NodeId {
        self.rows
            .borrow()
            .visible
            .iter()
            .find(|(_, s, _)| *s == slot)
            .map_or(slot, |(_, _, item)| *item)
    }
}
