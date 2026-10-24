//! Intrinsic-height menu content with an allocation-limited scrolling viewport.
use crate::{
    input::{EventPhase, InputEvent},
    scene::{NodeId, Transform},
    widgets::Ui,
};

pub(crate) fn mount(ui: &mut Ui, panel: NodeId, viewport: NodeId, content: NodeId) {
    // No explicit height: natural content sizes the menu until its maximum height
    // constrains this flex child. Shrinking content can then shrink the menu again.
    {
        let mut scene = ui.scene.borrow_mut();
        let mut style = scene.style(viewport);
        style.clip = true;
        style.flex_shrink = 1.;
        style.flex_grow = 1.;
        scene.set_style(viewport, style);
    }
    let size = ui.observe_content_size(viewport);
    let extent = ui.observe_content_size(content);
    let offset = ui.signal(0_f32);
    let read_size = size.clone();
    let read_extent = extent.clone();
    let read_offset = offset.clone();
    let weak = ui.downgrade();
    ui.on_event(viewport, false, move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if matches!(cx.event, InputEvent::Focus) && cx.target != viewport {
            ui.prepare_frame();
            if ui.input.focused() != Some(cx.target) {
                return;
            }
            let movement = {
                let scene = ui.scene.borrow();
                if !scene.contains(cx.target) || !scene.contains(viewport) {
                    return;
                }
                let target = scene.bounds(cx.target);
                let visible = scene.bounds(viewport);
                let before = target.y as f64 - visible.y as f64;
                let after = target.y as f64 + target.height as f64
                    - visible.y as f64
                    - visible.height as f64;
                if before < 0. && after > 0. {
                    0.
                } else if before < 0. {
                    if target.height > visible.height {
                        after
                    } else {
                        before
                    }
                } else if after > 0. {
                    if target.height > visible.height {
                        before
                    } else {
                        after
                    }
                } else {
                    0.
                }
            };
            let limit = (read_extent.get().1 as f64 - read_size.get().1 as f64).max(0.);
            read_offset.set((read_offset.get() as f64 + movement).clamp(0., limit) as f32);
            // Focus routing can run in a reactive batch. Publish the transform now
            // so outer focus handlers see the final position, just like scroll().
            let mut scene = ui.scene.borrow_mut();
            if scene.contains(content) && ui.input.focused() == Some(cx.target) {
                scene.set_transform(
                    content,
                    Transform {
                        x: 0.,
                        y: -read_offset.get(),
                    },
                );
            }
        }
        if let InputEvent::Scroll { delta_y, .. } = cx.event {
            if cx.default_prevented() {
                return;
            }
            if !delta_y.is_finite() {
                return;
            }
            let before = read_offset.get();
            let limit = (read_extent.get().1 as f64 - read_size.get().1 as f64).max(0.);
            let after = (before as f64 + delta_y as f64).clamp(0., limit) as f32;
            if before != after {
                read_offset.set(after);
                cx.prevent_default();
                cx.stop_propagation();
            }
        }
    });
    let scene = ui.scene.clone();
    ui.bind(panel, move || {
        let (width, height) = size.get();
        let limit = (extent.get().1 - height).max(0.);
        let position = offset.get().clamp(0., limit);
        offset.set(position);
        let mut scene = scene.borrow_mut();
        let mut style = scene.style(content);
        style.width = Some(width);
        style.flex_shrink = 0.;
        scene.set_style(content, style);
        scene.set_transform(
            content,
            Transform {
                x: 0.,
                y: -position,
            },
        );
    });
}
