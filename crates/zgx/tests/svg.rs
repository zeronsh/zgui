use std::sync::Arc;
use zgx::{compose::prelude::*, scene::NodeKind, widgets::Ui};
#[test]
fn svg_component_retains_semantics_size_and_reactive_transform() {
    let mut ui = Ui::new(160., 120.);
    let angle = ui.signal(0.);
    let source = Arc::new(SvgData::new(&b"<svg/>"[..]).unwrap());
    let view = ui.mount(
        svg_signal("Icon", {
            let angle = angle.clone();
            move || Arc::new(source.transformed(Affine::rotation(angle.get())))
        })
        .id("icon")
        .size(40., 20.)
        .svg_tint(rgb(0x55aa88)),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    let root = view.find("icon").unwrap();
    let node = ui.scene.borrow().children(root)[0];
    assert!(matches!(ui.scene.borrow().kind(node), NodeKind::Svg(_)));
    assert_eq!(ui.scene.borrow().bounds(root).width, 40.);
    assert_eq!(ui.semantics.borrow().get(root).unwrap().label, "Icon");
    angle.set(0.5);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(!report.damage.is_empty());
    assert_eq!(ui.scene.borrow().children(root)[0], node);
}
