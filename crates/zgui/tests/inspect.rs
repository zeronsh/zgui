//! The layout inspector: its overlay outlines every box and describes the
//! one under the pointer without changing the interface's layout or input,
//! and layout warnings find suspicious boxes once per node.
use zgui::{
    compose::{ViewHandle, prelude::*},
    inspect::{Inspector, WarningKind, layout_warnings},
    scene::{NodeId, Rect},
    widgets::Ui,
};

/// A window with one of each suspicious layout, and healthy neighbours.
fn suspicious(ui: &Ui) -> View {
    column()
        .w(400.)
        .h(300.)
        .gap(4.)
        .child(
            // A 200 px row whose second child shrinks to nothing: allowed
            // below its text's width (`min-width: 0`, as CSS needs it).
            row()
                .w(200.)
                .h(20.)
                .child(div().w(200.).h(20.).shrink_0())
                .child(
                    row()
                        .id("squeezed")
                        .flex_shrink(1.)
                        .min_w(0.)
                        .child(text("squeezed out")),
                ),
        )
        .child(
            // A 40 px clip around 120 px of content, not a scroll view.
            column()
                .id("clipped")
                .w(100.)
                .h(40.)
                .overflow_hidden()
                .child(div().w(80.).h(120.).shrink_0()),
        )
        .child(
            // Half the width of a parent that sizes to its content (in a
            // row it would flex, and its flexed width would resolve it).
            column().items_start().child(
                row()
                    .id("parent")
                    .child(div().id("half").w_percent(50.).h(10.).child(text("half")))
                    .child(div().w(60.).h(10.)),
            ),
        )
        .child(
            // Healthy: a scroll view's content overflows its viewport.
            scroll(ui.signal(0.))
                .id("scroll")
                .w(100.)
                .h(40.)
                .child(column().child(div().w(100.).h(400.))),
        )
        .child(
            row()
                .id("card")
                .w(160.)
                .h(48.)
                .p(6.)
                .m(2.)
                .gap(3.)
                .child(text("Card").id("card-text")),
        )
}

fn mounted(view: impl FnOnce(&Ui) -> View) -> (Ui, ViewHandle) {
    let mut ui = Ui::new(400., 300.);
    let view = view(&ui);
    let handle = ui.mount(view);
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    (ui, handle)
}

fn kinds(warnings: &[zgui::inspect::Warning], node: NodeId) -> Vec<WarningKind> {
    warnings
        .iter()
        .filter(|w| w.node == node)
        .map(|w| w.kind)
        .collect()
}

#[test]
fn warnings_find_collapsed_overflowing_and_unresolved_boxes() {
    let (ui, handle) = mounted(suspicious);
    let warnings = layout_warnings(&ui);
    let find = |id| handle.find(id).unwrap_or_else(|| panic!("no `{id}`"));
    assert_eq!(
        kinds(&warnings, find("squeezed")),
        [WarningKind::ZeroSize],
        "{warnings:#?}"
    );
    assert_eq!(
        kinds(&warnings, find("clipped")),
        [WarningKind::Overflow],
        "{warnings:#?}"
    );
    assert_eq!(
        kinds(&warnings, find("half")),
        [WarningKind::UnresolvedPercent],
        "{warnings:#?}"
    );
    // Nothing else: the scroll view, the card, the text squeezed with its
    // collapsed row, the clip's content.
    assert_eq!(warnings.len(), 3, "{warnings:#?}");
    let zero = warnings
        .iter()
        .find(|w| w.kind == WarningKind::ZeroSize)
        .unwrap();
    assert!(zero.message.contains("zero width"), "{zero}");
    let overflow = warnings
        .iter()
        .find(|w| w.kind == WarningKind::Overflow)
        .unwrap();
    assert!(overflow.message.contains("vertically by 80"), "{overflow}");
}

#[test]
fn healthy_layouts_have_no_warnings() {
    let (ui, _) = mounted(|ui| {
        column()
            .w(400.)
            .h(300.)
            .p(8.)
            .gap(8.)
            .child(row().gap(4.).child(text("a")).child(text("b")))
            .child(div().w_percent(50.).h(20.))
            .child(
                scroll(ui.signal(0.))
                    .h(50.)
                    .child(column().child(div().h(500.))),
            )
            .child(
                div()
                    .w(0.)
                    .h(10.)
                    .items_start()
                    .child(text("deliberately collapsed")),
            )
            // An empty placeholder (a hidden conditional): nothing to show.
            .child(row().child(column().child(div())))
            .child(text_input("name", ui.signal(String::new())).w(80.))
    });
    assert_eq!(layout_warnings(&ui), [], "no false alarms");
}

#[test]
fn overlay_outlines_boxes_and_describes_the_one_under_the_pointer() {
    let (ui, handle) = mounted(suspicious);
    let before = ui.scene.borrow().len();
    let card = handle.find("card").unwrap();
    let card_bounds = ui.scene.borrow().bounds(card);
    let layout_before: Vec<Rect> = {
        let scene = ui.scene.borrow();
        let mut boxes = Vec::new();
        scene.walk(scene.root(), |item| {
            boxes.push(item.bounds);
            true
        });
        boxes
    };

    let inspector = Inspector::off();
    assert!(!inspector.active(), "off, hosts skip it");
    inspector.update(&ui, Some((5., 5.)));
    assert_eq!(ui.scene.borrow().len(), before, "off, it adds nothing");

    inspector.label(card, "row #card", Some("app.luau:12".into()));
    assert_eq!(inspector.label_of(card), None, "labels only while on");
    inspector.set_enabled(true);
    assert!(inspector.active());
    inspector.label(card, "row #card", Some("app.luau:12".into()));
    let (x, y) = (card_bounds.x + 2., card_bounds.y + 2.);
    inspector.update(&ui, Some((x, y)));
    ui.scene.borrow_mut().flush();

    let overlay = inspector.overlay().expect("the overlay is shown");
    {
        let scene = ui.scene.borrow();
        assert_eq!(
            scene.children(scene.root()).last(),
            Some(&overlay),
            "last under the root: it paints over the interface"
        );
        // Every box the interface had keeps its layout.
        let mut boxes = Vec::new();
        scene.walk(scene.root(), |item| {
            boxes.push(item.bounds);
            true
        });
        assert_eq!(boxes, layout_before, "the overlay changes no layout");
        // Input passes through it.
        let hit = scene.hit_test(x, y).unwrap();
        assert!(!scene.is_within(hit, overlay), "hit tests skip the overlay");
    }
    assert!(inspector.outlines() >= 12, "{}", inspector.outlines());
    assert_eq!(inspector.picked(), Some(card));
    let details = inspector.details().join("\n");
    assert!(details.contains("row #card"), "{details}");
    assert!(details.contains("created at app.luau:12"), "{details}");
    assert!(details.contains("160 × 48 at"), "{details}");
    assert!(details.contains("padding 6"), "{details}");
    assert!(details.contains("margin 2"), "{details}");
    assert!(details.contains("gap 3"), "{details}");
    assert!(details.contains("grow 0 · shrink 1"), "{details}");
    assert!(details.contains("3 warnings"), "{details}");

    // A child inherits its creator's location.
    let text = handle.find("card-text").unwrap();
    let t = ui.scene.borrow().bounds(text);
    inspector.update(&ui, Some((t.x + 1., t.y + 1.)));
    let details = inspector.details().join("\n");
    assert!(details.contains("in row #card"), "{details}");
    assert!(details.contains("created inside app.luau:12"), "{details}");

    // The hovered node's own warning is listed.
    let clipped = handle.find("clipped").unwrap();
    let c = ui.scene.borrow().bounds(clipped);
    inspector.update(&ui, Some((c.x + 95., c.y + 1.)));
    let details = inspector.details().join("\n");
    assert!(details.contains("clips its content"), "{details}");
    assert!(details.contains("⚠ content overflows"), "{details}");

    // An unchanged frame does no work.
    ui.scene.borrow_mut().flush();
    inspector.update(&ui, Some((c.x + 95., c.y + 1.)));
    assert!(ui.scene.borrow_mut().flush().is_idle(), "idle stays idle");

    // Off again, it leaves nothing behind.
    inspector.set_enabled(false);
    assert!(inspector.active(), "until the overlay is removed");
    inspector.update(&ui, None);
    assert!(!inspector.active());
    assert_eq!(inspector.overlay(), None);
    assert_eq!(ui.scene.borrow().len(), before);
}

#[test]
fn warnings_are_checked_while_the_overlay_is_hidden() {
    let (ui, _) = mounted(suspicious);
    let inspector = Inspector::off();
    inspector.set_warnings(true);
    assert!(inspector.active() && !inspector.enabled());
    let before = ui.scene.borrow().len();
    inspector.update(&ui, None);
    assert_eq!(inspector.warnings().len(), 3);
    assert_eq!(ui.scene.borrow().len(), before, "no overlay");
    assert_eq!(inspector.overlay(), None);
}
