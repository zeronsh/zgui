use zgx::{
    compose::prelude::*,
    scene::{Color, NodeKind},
    widgets::Ui,
};
#[test]
fn inline_children_inherit_and_reactive_updates_keep_one_paragraph() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::from(" 世界"));
    let source = value.clone();
    let handle = ui.mount(
        column()
            .text_size(20.)
            .text_color(Color(20, 40, 60, 255))
            .font_weight(600)
            .child(
                rich_text_signal(move || {
                    vec![
                        text_span("Hello").text_color(Color(255, 0, 0, 255)),
                        text_span(source.get()).italic(true),
                    ]
                })
                .id("rich")
                .w(110.)
                .text_wrap(true),
            ),
    );
    ui.prepare_frame();
    let owner = handle.find("rich").unwrap();
    let node = ui.scene.borrow().children(owner)[0];
    let read = || {
        let scene = ui.scene.borrow();
        let NodeKind::RichText { text } = scene.kind(node) else {
            panic!()
        };
        text.clone()
    };
    let initial = read();
    assert_eq!(initial.text(), "Hello 世界");
    assert_eq!(initial.runs().len(), 2);
    assert_eq!(initial.runs()[0].font.weight, 600);
    assert_eq!(initial.runs()[1].color, Color(20, 40, 60, 255));
    assert!(initial.runs()[1].font.italic);
    ui.scene.borrow_mut().flush();
    value.set(" 世界".into());
    assert!(ui.scene.borrow_mut().flush().is_idle());
    value.set(" streaming longer words".into());
    ui.prepare_frame();
    assert_eq!(handle.find("rich"), Some(owner));
    assert!(read().text().ends_with("words"));
    assert!(ui.scene.borrow().bounds(node).height > 28.);
    ui.remove(handle.node());
    value.set("removed".into());
    assert!(!ui.scene.borrow().contains(node));
}
#[test]
fn rich_builder_owns_inline_children_and_empty_spans_are_ignored() {
    let mut ui = Ui::new(200., 100.);
    let handle = ui.mount(
        rich_text()
            .child(text_span(""))
            .child(text_span("mixed").text_size(24.))
            .child(text_span(" style").font_weight(700))
            .into(),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let NodeKind::RichText { text } = scene.kind(scene.children(handle.node())[0]) else {
        panic!()
    };
    assert_eq!(text.text(), "mixed style");
    assert_eq!(text.runs().len(), 2);
    assert_eq!(scene.children(handle.node()).len(), 1);
}

#[test]
fn inherited_color_changes_only_paint_and_font_size_reflows() {
    let mut ui = Ui::new(300., 150.);
    let color = ui.signal(Color(220, 30, 30, 255));
    let size = ui.signal(16.);
    let read_color = color.clone();
    let read_size = size.clone();
    let view = ui.mount(
        column()
            .reactive_style(move || {
                Styles::new()
                    .text_color(read_color.get())
                    .text_size(read_size.get())
            })
            .child(
                rich_text()
                    .id("text")
                    .child(text_span("inherited"))
                    .child(text_span(" reset").text_size(12.)),
            ),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    color.set(Color(30, 220, 30, 255));
    ui.prepare_frame();
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0);
    assert!(!frame.damage.is_empty());
    let node = ui.scene.borrow().children(view.find("text").unwrap())[0];
    let before = ui.scene.borrow().bounds(node);
    size.set(28.);
    ui.prepare_frame();
    assert!(ui.scene.borrow().bounds(node).width > before.width);
    let scene = ui.scene.borrow();
    let NodeKind::RichText { text } = scene.kind(node) else {
        panic!()
    };
    assert_eq!(text.runs()[1].font_size, 12.);
}

#[test]
fn styled_inline_button_label_is_accessible_and_updates() {
    let mut ui = Ui::new(200., 100.);
    let value = ui.signal(String::from("Save"));
    let read = value.clone();
    let view = ui.mount(button().child(rich_text_signal(move || {
        vec![text_span(read.get()).font_bold(), text_span(" file")]
    })));
    ui.prepare_frame();
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().label,
        "Save file"
    );
    value.set("Open".into());
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().label,
        "Open file"
    );
}

#[test]
fn wrapped_links_use_real_fragments_one_tab_stop_and_owned_callbacks() {
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        input::{InputEvent, Key, Modifiers, PointerButton},
        semantics::Role,
    };
    let mut ui = Ui::new(200., 200.);
    let clicks = Rc::new(Cell::new(0));
    let clicked = clicks.clone();
    let view = ui.mount(
        rich_text()
            .w(90.)
            .p(7.)
            .text_size(16.)
            .text_wrap(true)
            .child(text_span("plain "))
            .child(text_span("many link words").on_click(move || clicked.set(clicked.get() + 1)))
            .child(text_span(" tail"))
            .into(),
    );
    ui.prepare_frame();
    let links: Vec<_> = ui
        .semantics
        .borrow()
        .iter()
        .filter(|(_, s)| s.role == Role::Link)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(links.len(), 1);
    let link = links[0];
    let fragments = ui.scene.borrow().children(link).to_vec();
    assert!(!fragments.is_empty());
    let r = ui.scene.borrow().bounds(*fragments.last().unwrap());
    ui.dispatch(InputEvent::PointerDown {
        x: r.x + r.width / 2.,
        y: r.y + r.height / 2.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: r.x + r.width / 2.,
        y: r.y + r.height / 2.,
        button: PointerButton::Primary,
    });
    assert_eq!(clicks.get(), 1);
    assert_eq!(ui.input.focused(), Some(link));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    assert_eq!(clicks.get(), 2);
    ui.set_disabled(view.node(), true);
    ui.input.dispatch_to(&ui.scene, link, InputEvent::Activate);
    assert_eq!(clicks.get(), 2);
    ui.remove(view.node());
    assert!(!ui.scene.borrow().contains(link));
    assert!(ui.input.focused().is_none());
}

#[test]
fn link_geometry_reuses_shape_until_text_width_or_backend_changes() {
    use std::{cell::Cell, rc::Rc};
    use zgx::{semantics::Role, text_layout::FallbackTextLayout};
    let mut ui = Ui::new(400., 200.);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    ui.scene
        .borrow_mut()
        .set_rich_text_shaper(move |rich, width| {
            count.set(count.get() + 1);
            Box::new(FallbackTextLayout::with_rich(rich, width))
        });
    let view = ui.mount(
        rich_text()
            .w(200.)
            .child(text_span("link words").on_click(|| {}))
            .into(),
    );
    ui.prepare_frame();
    let initial = calls.get();
    assert!(initial > 0);
    ui.prepare_frame();
    assert_eq!(calls.get(), initial);
    let link = ui
        .semantics
        .borrow()
        .iter()
        .find(|(_, s)| s.role == Role::Link)
        .unwrap()
        .0;
    ui.input.focus(&ui.scene, Some(link));
    ui.prepare_frame();
    assert_eq!(calls.get(), initial);
    ui.refresh_text_geometry();
    ui.prepare_frame();
    assert!(calls.get() > initial);
    ui.remove(view.node());
    let after = calls.get();
    ui.refresh_text_geometry();
    assert_eq!(calls.get(), after);
}

#[test]
fn font_features_and_ordered_fallbacks_inherit_reset_and_normalize() {
    use zgx::text_layout::{FontFamily, FontFeatures};
    let mut ui = Ui::new(400., 200.);
    let settings = ui.signal(FontFeatures::new([(*b"liga", 0), (*b"kern", 1)]));
    let source = settings.clone();
    let view = ui.mount(
        column()
            .reactive_style(move || Styles::new().font_features(source.get()))
            .font_fallbacks(vec![FontFamily::Serif, FontFamily::Monospace])
            .child(
                rich_text().id("rich").child(text_span("inherit")).child(
                    text_span("reset")
                        .font_features(FontFeatures::default())
                        .font_fallbacks(Vec::new()),
                ),
            ),
    );
    ui.prepare_frame();
    let node = ui.scene.borrow().children(view.find("rich").unwrap())[0];
    {
        let scene = ui.scene.borrow();
        let NodeKind::RichText { text } = scene.kind(node) else {
            panic!()
        };
        assert_eq!(text.runs()[0].font.features, settings.get());
        assert_eq!(
            text.runs()[0].font.fallbacks.as_ref(),
            &[FontFamily::Serif, FontFamily::Monospace]
        );
        assert!(text.runs()[1].font.features.settings().is_empty());
        assert!(text.runs()[1].font.fallbacks.is_empty());
    }
    ui.scene.borrow_mut().flush();
    settings.set(FontFeatures::new([
        (*b"kern", 1),
        (*b"liga", 1),
        (*b"liga", 0),
    ]));
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().is_idle());
    settings.set(FontFeatures::new([(*b"liga", 1)]));
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
}

#[test]
fn display_clamping_reflows_retained_text_but_never_editor_documents() {
    let mut ui = Ui::new(220., 400.);
    let lines = ui.signal(2_u32);
    let source = lines.clone();
    let value = ui.signal("one two three four five six seven eight nine ten".to_string());
    let text = value.get();
    let view = ui.mount(
        column()
            .w(90.)
            .reactive_style(move || {
                Styles::new()
                    .line_clamp(source.get())
                    .text_overflow(if source.get() == 0 {
                        TextOverflow::Clip
                    } else {
                        TextOverflow::Ellipsis
                    })
            })
            .child(
                rich_text()
                    .id("rich")
                    .text_wrap(true)
                    .child(text_span(text.clone())),
            )
            .child(
                zgx::compose::text(text.clone())
                    .id("plain")
                    .text_wrap(true),
            )
            .child(text_area("Editor", value.clone()).id("editor").h(100.)),
    );
    ui.prepare_frame();
    let rich = ui.scene.borrow().children(view.find("rich").unwrap())[0];
    let plain = view.find("plain").unwrap();
    let initial = ui.scene.borrow().bounds(rich).height;
    assert!(initial <= 46., "two fallback lines");
    assert_eq!(
        ui.semantics.borrow().get(plain).unwrap().label.as_str(),
        text.as_str()
    );
    lines.set(0);
    ui.prepare_frame();
    assert!(ui.scene.borrow().bounds(rich).height > initial);
    assert_eq!(value.get(), text);
    assert_eq!(
        ui.scene
            .borrow()
            .style(view.find("editor").unwrap())
            .text_options,
        Default::default()
    );
}

#[test]
fn immutable_rich_options_are_authoritative_over_plain_text_style_options() {
    use zgx::{
        rich_text::{RichText, TextRun},
        scene::{Scene, Style},
    };
    let mut scene = Scene::new(200., 200.);
    let value = "one two three four five six seven eight";
    let one = TextOptions {
        overflow: TextOverflow::Clip,
        line_clamp: std::num::NonZeroU32::new(1),
    };
    let two = TextOptions {
        line_clamp: std::num::NonZeroU32::new(2),
        ..one
    };
    let rich = RichText::new(
        value,
        vec![TextRun {
            range: 0..value.len(),
            ..Default::default()
        }],
    )
    .unwrap()
    .with_options(two);
    let style = Style {
        width: Some(60.),
        text_options: one,
        ..Default::default()
    };
    let rich = scene.append(
        scene.root(),
        NodeKind::RichText {
            text: std::sync::Arc::new(rich),
        },
        style.clone(),
    );
    let plain = scene.append(
        scene.root(),
        NodeKind::Text {
            text: value.into(),
            font_size: 16.,
            color: Color(255, 255, 255, 255),
        },
        style,
    );
    scene.flush();
    assert_eq!(scene.bounds(rich).height, 46.);
    assert_eq!(scene.bounds(plain).height, 23.);
}
