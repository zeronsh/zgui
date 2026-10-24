//! `Styled::filter_blur`, `mask` and `mask_offset` reach the scene, and
//! animating them composites a cached layer without layout or repaint.
use std::sync::Arc;
use zgx::{compose::prelude::*, scene::Color, widgets::Ui};

#[test]
fn reactive_content_blur_and_mask_offset_composite_alone() {
    let mut ui = Ui::new(200., 160.);
    let image = Arc::new(zgx::image::ImageData::new(2, 1, vec![255; 8]).unwrap());
    let tick = ui.signal(0_u8);
    let read = tick.clone();
    let view = ui.mount(
        div()
            .w(80.)
            .h(20.)
            .bg(Color(255, 0, 0, 255))
            .blur(5.)
            .mask(image.clone(), (40., 20.))
            .reactive_style(move || {
                let t = f32::from(read.get());
                Styles::new()
                    .opacity(t / 4.)
                    .filter_blur(2. - t / 2.)
                    .mask_offset(t * 10., 0.)
            }),
    );
    ui.prepare_frame();
    let node = view.node();
    {
        let scene = ui.scene.borrow();
        let effects = scene.effects(node);
        // The backdrop blur stays what `.blur` set; the content blur is apart.
        assert_eq!((effects.blur_radius, effects.filter_blur), (5., 2.));
        let mask = scene.mask(node).unwrap();
        assert_eq!((mask.offset, mask.size), ((0., 0.), (40., 20.)));
        assert!(Arc::ptr_eq(&mask.image, &image));
        assert!(scene.is_layer(node));
    }
    ui.scene.borrow_mut().flush();
    for t in 1..=4 {
        let revision = ui.scene.borrow().layer_revision(node);
        tick.set(t);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0), "{frame:?}");
        assert!(!frame.damage.is_empty());
        let scene = ui.scene.borrow();
        assert_eq!(scene.layer_revision(node), revision);
        assert_eq!(scene.effects(node).filter_blur, 2. - f32::from(t) / 2.);
        assert_eq!(scene.mask(node).unwrap().offset, (f32::from(t) * 10., 0.));
    }
    // Blur back at 0, the mask alone keeps it a layer.
    assert!(ui.scene.borrow().is_layer(node));
}
