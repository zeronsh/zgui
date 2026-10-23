use zgui::{
    compose::prelude::*,
    scene::{Color, NodeId, NodeKind},
    theme::{SystemPreferences, tok},
    widgets::Ui,
};

fn fill(ui: &Ui, node: NodeId) -> Color {
    match ui.scene.borrow().kind(node) {
        NodeKind::Panel { quad, .. } => quad.fill,
        other => panic!("expected panel {other:?}"),
    }
}

#[test]
fn reduced_transparency_paints_frosted_glass_opaque_and_follows_the_setting() {
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(
        column()
            .size(300., 200.)
            .child(
                div()
                    .id("glass")
                    .size(100., 50.)
                    .bg(rgba(0x10203080))
                    .blur(12.),
            )
            .child(div().id("clear").size(100., 50.).blur(8.))
            .child(div().id("plain").size(100., 50.).bg(rgba(0x10203080))),
    );
    ui.prepare_frame();
    let [glass, clear, plain] = ["glass", "clear", "plain"].map(|id| view.find(id).unwrap());
    let blur = |ui: &Ui, node| ui.scene.borrow().effects(node).blur_radius;
    assert_eq!(blur(&ui, glass), 12.);
    assert_eq!(fill(&ui, glass).3, 0x80);

    let set = |ui: &Ui, reduced_transparency| {
        ui.theme().set_system_preferences(SystemPreferences {
            reduced_transparency,
            ..Default::default()
        });
        ui.prepare_frame();
    };
    set(&ui, true);
    assert_eq!(blur(&ui, glass), 0., "no blur");
    assert_eq!(
        fill(&ui, glass),
        Color(0x10, 0x20, 0x30, 255),
        "its tint, solid"
    );
    assert_eq!(
        fill(&ui, clear),
        ui.theme().root().color(tok::SURFACE_RAISED),
        "a clear glass gets a surface"
    );
    assert_eq!(fill(&ui, plain).3, 0x80, "what has no blur keeps its own");
    set(&ui, false);
    assert_eq!(blur(&ui, glass), 12., "back as it was");
    assert_eq!(fill(&ui, glass).3, 0x80);
}
