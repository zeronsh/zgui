use std::sync::Arc;
use zgx::{
    rich_text::{RichText, TextRun},
    scene::{Color, NodeKind, Rect, Scene, Style},
};
use zgx_desktop::raster::Raster;
#[test]
fn software_rich_text_has_colored_glyphs_and_restores_changed_content() {
    let mut scene = Scene::new(200., 100.);
    let mut raster = Raster::new(200, 100);
    let make = |content: &str| {
        Arc::new(
            RichText::new(
                content,
                vec![
                    TextRun {
                        range: 0..4,
                        font: Default::default(),
                        font_size: 24.,
                        color: Color(255, 20, 20, 255),
                        ..Default::default()
                    },
                    TextRun {
                        range: 4..content.len(),
                        font: Default::default(),
                        font_size: 26.,
                        color: Color(20, 255, 20, 255),
                        ..Default::default()
                    },
                ],
            )
            .unwrap(),
        )
    };
    let node = scene.append(
        scene.root(),
        NodeKind::RichText {
            text: make("RED GREEN"),
        },
        Style {
            width: Some(180.),
            height: Some(80.),
            ..Default::default()
        },
    );
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    assert!(
        raster
            .pixels
            .iter()
            .filter(|p| ((*p >> 16) & 255) > ((*p >> 8) & 255) + 60)
            .count()
            > 20
    );
    assert!(
        raster
            .pixels
            .iter()
            .filter(|p| ((*p >> 8) & 255) > ((*p >> 16) & 255) + 60)
            .count()
            > 20
    );
    scene.set_kind(
        node,
        NodeKind::RichText {
            text: make("RED short"),
        },
    );
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    let mut fresh = Raster::new(200, 100);
    fresh.render(&scene, &[Rect::new(0., 0., 200., 100.)]);
    assert_eq!(raster.pixels, fresh.pixels);
}

#[test]
fn software_rich_background_and_line_decorations_are_visible_and_removable() {
    use zgx::rich_text::Decoration;
    let mut scene = Scene::new(200., 120.);
    let mut raster = Raster::new(200, 120);
    let value = "Decorated words";
    let rich = RichText::new(
        value,
        vec![TextRun {
            range: 0..value.len(),
            font_size: 24.,
            background: Some(Color(20, 60, 100, 255)),
            underline: Some(Decoration::new(2.).color(Color(255, 255, 0, 255))),
            strikethrough: Some(Decoration::new(2.).color(Color(255, 0, 255, 255))),
            ..Default::default()
        }],
    )
    .unwrap();
    let node = scene.append(
        scene.root(),
        NodeKind::RichText {
            text: Arc::new(rich),
        },
        Style {
            width: Some(140.),
            height: Some(110.),
            ..Default::default()
        },
    );
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    for color in [0x143c64, 0xffff00, 0xff00ff] {
        assert!(raster.pixels.iter().filter(|p| **p == color).count() > 30);
    }
    scene.remove(node);
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    let mut fresh = Raster::new(200, 120);
    fresh.render(&scene, &[Rect::new(0., 0., 200., 120.)]);
    assert_eq!(raster.pixels, fresh.pixels);
}

#[test]
fn native_bidi_links_use_shaped_fragments_and_expose_clickable_accessibility() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, PointerButton},
        semantics::Role,
        widgets::Ui,
    };
    let mut ui = Ui::new(260., 200.);
    let fonts = Rc::new(RefCell::new(cosmic_text::FontSystem::new()));
    let measure = fonts.clone();
    ui.scene
        .borrow_mut()
        .set_rich_text_measurer(move |rich, width| {
            zgx_gpu::text::ShapedText::with_runs(&mut measure.borrow_mut(), rich, width).size()
        });
    ui.scene
        .borrow_mut()
        .set_rich_text_shaper(move |rich, width| {
            Box::new(zgx_gpu::text::ShapedText::with_runs(
                &mut fonts.borrow_mut(),
                rich,
                width,
            ))
        });
    let clicks = Rc::new(Cell::new(0));
    let callback = clicks.clone();
    ui.mount(
        rich_text()
            .w(140.)
            .p(8.)
            .text_size(22.)
            .text_wrap(true)
            .child(text_span("Plain "))
            .child(
                text_span("العربية linked words")
                    .on_click(move || callback.set(callback.get() + 1)),
            )
            .into(),
    );
    ui.prepare_frame();
    let link = ui
        .semantics
        .borrow()
        .iter()
        .find(|(_, s)| s.role == Role::Link)
        .unwrap()
        .0;
    let fragments = ui.scene.borrow().children(link).to_vec();
    assert!(!fragments.is_empty());
    for fragment in std::iter::once(link).chain(fragments) {
        let r = ui.scene.borrow().bounds(fragment);
        let x = r.x + r.width / 2.;
        let y = r.y + r.height / 2.;
        ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        });
        ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        });
    }
    assert!(clicks.get() > 1);
    assert_eq!(ui.input.focused(), Some(link));
    let mut adapter = zgx_desktop::accessibility::AccessibilityTree::new();
    let tree = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(link),
        "Rich",
        1.,
    );
    let native = &tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Link)
        .unwrap()
        .1;
    assert!(native.supports_action(accesskit::Action::Click));
    assert!(native.supports_action(accesskit::Action::Focus));
    let consumer = accesskit_consumer::Tree::new(tree, true);
    assert_eq!(
        consumer.state().focus().unwrap().role(),
        accesskit::Role::Link
    );
}

#[test]
fn software_display_ellipsis_clears_previous_lines_and_restores_full_text() {
    use zgx::text_layout::{TextOptions, TextOverflow};
    let mut scene = Scene::new(180., 140.);
    let mut raster = Raster::new(180, 140);
    let node = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "one two three four five six seven eight".into(),
            font_size: 20.,
            color: Color(245, 245, 245, 255),
        },
        Style {
            width: Some(110.),
            height: Some(130.),
            ..Default::default()
        },
    );
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    let full = raster.pixels.clone();
    let mut style = scene.style(node);
    style.text_options = TextOptions {
        overflow: TextOverflow::Ellipsis,
        line_clamp: std::num::NonZeroU32::new(1),
    };
    scene.set_style(node, style);
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    assert_ne!(raster.pixels, full);
    assert!(
        raster
            .pixels
            .iter()
            .enumerate()
            .all(|(i, p)| i / 180 < 32 || (p & 255) < 100)
    );
    let mut fresh = Raster::new(180, 140);
    fresh.render(&scene, &[Rect::new(0., 0., 180., 140.)]);
    assert_eq!(fresh.pixels, raster.pixels);
    let mut style = scene.style(node);
    style.text_options = Default::default();
    scene.set_style(node, style);
    let frame = scene.flush();
    raster.render(&scene, &frame.damage);
    assert_eq!(raster.pixels, full);
}
