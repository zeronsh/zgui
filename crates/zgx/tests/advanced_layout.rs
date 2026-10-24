use zgx::{
    compose::prelude::*,
    layout::{ContentAlign, Length},
    scene::{Align, Rect},
    widgets::Ui,
};
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}
fn bounds(ui: &Ui, view: &zgx::compose::ViewHandle, id: &str) -> Rect {
    ui.scene.borrow().bounds(view.find(id).unwrap())
}
#[test]
fn grid_spans_and_resize_retain_children() {
    let mut ui = Ui::new(600., 600.);
    let view = ui.mount(
        div()
            .grid()
            .size(500., 500.)
            .grid_cols(5)
            .grid_rows(5)
            .gap(4.)
            .child(
                div()
                    .id("header")
                    .w_full()
                    .h_full()
                    .row_start(1)
                    .col_span_full(),
            )
            .child(
                div()
                    .id("left")
                    .w_full()
                    .h_full()
                    .row_start(2)
                    .col_start(1)
                    .row_span(3),
            )
            .child(
                div()
                    .id("main")
                    .w_full()
                    .h_full()
                    .row_start(2)
                    .col_start(2)
                    .col_span(3)
                    .row_span(3),
            )
            .child(
                div()
                    .id("right")
                    .w_full()
                    .h_full()
                    .row_start(2)
                    .col_start(5)
                    .row_span(3),
            )
            .child(
                div()
                    .id("footer")
                    .w_full()
                    .h_full()
                    .row_start(5)
                    .col_span_full(),
            ),
    );
    ui.prepare_frame();
    let main = view.find("main").unwrap();
    let m = bounds(&ui, &view, "main");
    near(m.x, 100.8);
    near(m.y, 100.8);
    near(m.width, 298.4);
    near(m.height, 298.4);
    near(bounds(&ui, &view, "header").width, 500.);
    let mut style = ui.scene.borrow().style(view.node());
    style.width = Some(600.);
    ui.scene.borrow_mut().set_style(view.node(), style);
    ui.prepare_frame();
    assert_eq!(view.find("main"), Some(main));
    near(bounds(&ui, &view, "main").width, 358.4);
}
#[test]
fn wrapping_rows_have_independent_axis_gaps() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        row()
            .w(220.)
            .flex_wrap()
            .gap_x(10.)
            .gap_y(5.)
            .children((0..3).map(|i| div().id(i.to_string()).size(100., 20.))),
    );
    ui.prepare_frame();
    let first = bounds(&ui, &view, "0");
    let third = bounds(&ui, &view, "2");
    near(first.x, 0.);
    near(third.x, 0.);
    near(third.y, 25.);
    near(ui.scene.borrow().bounds(view.node()).height, 45.);
}
#[test]
fn reverse_basis_grow_and_self_alignment() {
    let mut ui = Ui::new(400., 200.);
    let view = ui.mount(
        row()
            .size(300., 80.)
            .flex_row_reverse()
            .gap(10.)
            .child(
                div()
                    .id("one")
                    .flex_basis(60.)
                    .grow()
                    .h(20.)
                    .align_self(Align::End),
            )
            .child(div().id("two").flex_basis(100.).grow().h(20.)),
    );
    ui.prepare_frame();
    let one = bounds(&ui, &view, "one");
    let two = bounds(&ui, &view, "two");
    near(one.width, 125.);
    near(two.width, 165.);
    near(one.x, 175.);
    near(two.x, 0.);
    near(one.y, 60.);
}
#[test]
fn absolute_insets_aspect_and_auto_margins() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column()
            .size(300., 200.)
            .child(div().id("center").w(50.).h(20.).mx_auto())
            .child(div().id("ratio").w(120.).aspect_ratio(2.))
            .child(
                div()
                    .id("absolute")
                    .absolute()
                    .size(50., 40.)
                    .right(10.)
                    .bottom(20.),
            ),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "center").x, 125.);
    near(bounds(&ui, &view, "ratio").height, 60.);
    let absolute = bounds(&ui, &view, "absolute");
    near(absolute.x, 240.);
    near(absolute.y, 140.);
}
#[test]
fn percentage_bounds_and_padding_resolve_against_parent() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column()
            .w(300.)
            .child(
                column()
                    .id("padded")
                    .w(100.)
                    .p_percent(10.)
                    .child(div().size(10., 20.)),
            )
            .child(div().id("minimum").w(10.).h(10.).min_w_percent(50.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "minimum").width, 150.);
    near(bounds(&ui, &view, "padded").height, 80.);
}
#[test]
fn wrapped_line_distribution_and_percent_basis() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        row()
            .size(200., 100.)
            .flex_wrap()
            .align_content(ContentAlign::SpaceBetween)
            .children((0..3).map(|i| {
                div()
                    .id(i.to_string())
                    .flex_basis(Length::Percent(0.5))
                    .h(20.)
            })),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "2").y, 80.);
    near(bounds(&ui, &view, "0").width, 100.);
}
#[test]
fn hidden_layout_and_invisible_paint_preserve_owned_identity() {
    let mut ui = Ui::new(300., 200.);
    let hidden = ui.runtime.signal(false);
    let invisible = hidden.clone();
    let view = ui.mount(
        column()
            .gap(5.)
            .child(button().id("one").size(100., 20.).reactive_style(move || {
                if invisible.get() {
                    Styles::new().hidden()
                } else {
                    Styles::new().layout_options(zgx::layout::LayoutOptions {
                        display: Some(zgx::layout::Display::Flex),
                        ..Default::default()
                    })
                }
            }))
            .child(div().id("two").size(100., 20.)),
    );
    ui.prepare_frame();
    let node = view.find("one").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    hidden.set(true);
    ui.prepare_frame();
    assert_eq!(view.find("one"), Some(node));
    assert!(ui.input.focused().is_none());
    assert!(ui.scene.borrow().visible_bounds(node).is_none());
    near(bounds(&ui, &view, "two").y, 0.);
    hidden.set(false);
    ui.prepare_frame();
    near(bounds(&ui, &view, "two").y, 25.);
    let mut style = ui.scene.borrow().style(node);
    style.layout_options = Some(std::sync::Arc::new(zgx::layout::LayoutOptions {
        visible: Some(false),
        ..Default::default()
    }));
    ui.scene.borrow_mut().set_style(node, style);
    ui.prepare_frame();
    near(bounds(&ui, &view, "two").y, 25.);
    assert!(ui.scene.borrow().visible_bounds(node).is_none());
}

#[test]
fn percentage_padding_consumers_use_containing_width() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().w(300.).child(
            div()
                .id("padded")
                .w(100.)
                .p_percent(10.)
                .child(div().size(10., 20.)),
        ),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let id = view.find("padded").unwrap();
    near(scene.padding(id).left, 30.);
    near(scene.padding(id).top, 30.);
    near(scene.bounds(id).height, 80.);
}

#[test]
fn fluent_pixel_overrides_clear_percentage_and_reverse_options() {
    let mut ui = Ui::new(500., 300.);
    let view = ui.mount(
        row()
            .w(300.)
            .flex_row_reverse()
            .flex_row()
            .gap_x(100.)
            .gap(10.)
            .child(
                div()
                    .id("first")
                    .w(60.)
                    .min_w_percent(80.)
                    .min_w(20.)
                    .p_percent(10.)
                    .p(5.)
                    .h(30.),
            )
            .child(div().id("second").size(50., 30.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "first").x, 0.);
    near(bounds(&ui, &view, "first").width, 60.);
    near(bounds(&ui, &view, "second").x, 70.);
    near(
        ui.scene.borrow().padding(view.find("first").unwrap()).left,
        5.,
    );
}

#[test]
fn evenly_spaced_ordinary_and_wrapped_rows_agree() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().children([
            row()
                .id("ordinary")
                .w(300.)
                .justify_evenly()
                .children((0..2).map(|i| div().id(format!("a{i}")).size(60., 20.))),
            row()
                .id("advanced")
                .w(300.)
                .flex_wrap()
                .justify_evenly()
                .children((0..2).map(|i| div().id(format!("b{i}")).size(60., 20.))),
        ]),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "a0").x, 60.);
    near(bounds(&ui, &view, "a1").x, 180.);
    near(bounds(&ui, &view, "b0").x, 60.);
    near(bounds(&ui, &view, "b1").x, 180.);
    ui.scene.borrow_mut().flush();
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
}

#[test]
fn scroll_content_keeps_grid_layout_across_viewport_resize() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        scroll(offset)
            .size(220., 70.)
            .grid()
            .grid_cols(2)
            .gap_x(20.)
            .gap_y(10.)
            .children((0..6).map(|i| div().id(format!("cell{i}")).w_full().h(30.))),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "cell0").width, 100.);
    near(bounds(&ui, &view, "cell1").x, 120.);
    near(bounds(&ui, &view, "cell4").y, 80.);
    let mut style = ui.scene.borrow().style(view.node());
    style.width = Some(320.);
    ui.scene.borrow_mut().set_style(view.node(), style);
    ui.prepare_frame();
    near(bounds(&ui, &view, "cell0").width, 150.);
    near(bounds(&ui, &view, "cell1").x, 170.);
}

#[test]
fn axis_overflow_clips_paint_hits_and_visibility_consistently() {
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(
        div().size(100., 100.).overflow_x_hidden().child(
            div()
                .id("wide")
                .size(200., 150.)
                .shrink_0()
                .bg(zgx::scene::Color(255, 0, 0, 255)),
        ),
    );
    ui.prepare_frame();
    let id = view.find("wide").unwrap();
    let scene = ui.scene.borrow();
    assert_eq!(
        scene.visible_bounds(id),
        Some(Rect::new(0., 0., 100., 150.))
    );
    assert!(scene.hit_test_all(50., 120.).contains(&id));
    assert!(!scene.hit_test_all(120., 50.).contains(&id));
    let paint = scene.paint_items().find(|p| p.id == id).unwrap();
    assert_eq!(paint.clip, Some(Rect::new(0., 0., 100., 200.)));
}

#[test]
fn intrinsic_overlay_preserves_stacked_child_size_with_advanced_properties() {
    let mut scene = zgx::scene::Scene::new(400., 300.);
    let parent = scene.append(
        scene.root(),
        zgx::scene::NodeKind::Container(zgx::scene::Layout::Overlay),
        Default::default(),
    );
    let child = scene.append(
        parent,
        zgx::scene::NodeKind::Container(zgx::scene::Layout::Column),
        zgx::scene::Style {
            width: Some(120.),
            height: Some(40.),
            layout_options: Some(std::sync::Arc::new(zgx::layout::LayoutOptions {
                align_self: Some(Align::Center),
                ..Default::default()
            })),
            ..Default::default()
        },
    );
    scene.flush();
    near(scene.bounds(parent).width, 120.);
    near(scene.bounds(parent).height, 40.);
    near(scene.bounds(child).width, 120.);
}

#[test]
fn baseline_alignment_uses_retained_taffy_baselines() {
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(
        row()
            .w(200.)
            .items_baseline()
            .child(div().id("short").size(50., 20.))
            .child(div().id("tall").size(50., 40.)),
    );
    ui.prepare_frame();
    near(bounds(&ui, &view, "short").y, 20.);
    near(bounds(&ui, &view, "tall").y, 0.);
}

#[test]
fn nested_percentage_padding_uses_parent_content_box_during_intrinsic_measurement() {
    let mut ui = Ui::new(500., 300.);
    let view = ui.mount(
        column().w(300.).p(20.).child(
            div()
                .id("padded")
                .w(100.)
                .p_percent(10.)
                .child(div().size(10., 20.)),
        ),
    );
    ui.prepare_frame();
    let id = view.find("padded").unwrap();
    near(ui.scene.borrow().padding(id).top, 26.);
    near(bounds(&ui, &view, "padded").height, 72.);
}

#[test]
fn unchanged_extended_options_share_storage_across_dimension_updates() {
    let mut ui = Ui::new(500., 300.);
    let width = ui.signal(200.);
    let read = width.clone();
    let view = ui.mount(
        row()
            .flex_wrap()
            .gap_x(8.)
            .reactive_style(move || zgx::style::Styles::new().w(read.get()))
            .child(div().size(50., 20.)),
    );
    ui.prepare_frame();
    let before = ui.scene.borrow().style(view.node()).layout_options.unwrap();
    width.set(300.);
    ui.prepare_frame();
    let after = ui.scene.borrow().style(view.node()).layout_options.unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &after));
    near(ui.scene.borrow().bounds(view.node()).width, 300.);
}
/// A flex-basis probe measures the panel at zero width after its real layout,
/// leaving the header holding the probe's result. Arranging the panel's real
/// layout must place the header with the result that layout used.
#[test]
fn percent_children_keep_their_layout_after_a_flex_basis_probe() {
    let mut ui = Ui::new(1280., 780.);
    let body = switch(
        || 1,
        |_, cx| {
            let offset = cx.state(0_f32);
            scroll(offset).w_full().h_full()
        },
    )
    .w_full()
    .grow()
    .min_h(0.);
    let panel = column()
        .grow()
        .flex_basis(0.)
        .min_w(0.)
        .h_full()
        .child(div().id("header").w_full().h(40.))
        .child(body);
    let view = ui.mount(
        row().w_full().h_full().child(
            column()
                .grow()
                .min_w(0.)
                .h_full()
                .child(row().w_full().grow().min_h(0.).child(panel)),
        ),
    );
    ui.try_prepare_frame().unwrap();
    near(bounds(&ui, &view, "header").width, 1280.);
}
