//! `overflow_hidden()` with `rounded(r)` or `rounded_corners(..)` clips a
//! view's descendants to its rounded box: paint items inside carry the
//! clip (innermost first), radii no larger than half the box.
use zgx::{compose::prelude::*, decoration::Corners, scene::RoundRect, widgets::Ui};

fn items_rounded(ui: &Ui, id: &str, handle: &zgx::compose::ViewHandle) -> zgx::scene::RoundedClip {
    let node = handle.find(id).expect("mounted");
    let scene = ui.scene.borrow();
    scene
        .layer_items(None)
        .into_iter()
        .find(|item| item.id == node)
        .expect("painted")
        .rounded()
}

#[test]
fn rounded_overflow_clips_reach_descendants_innermost_first() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            div()
                .w(200.)
                .h(100.)
                .overflow_hidden()
                .rounded(16.)
                .child(
                    div()
                        .w(60.)
                        .h(40.)
                        .overflow_hidden()
                        .rounded_corners(Corners {
                            top_left: 50.,
                            top_right: 0.,
                            bottom_right: 6.,
                            bottom_left: 0.,
                        })
                        .child(div().id("photo").w(80.).h(80.)),
                )
                .child(div().id("caption").w(10.).h(10.)),
        ),
    );
    ui.prepare_frame();
    let card = RoundRect {
        rect: zgx::scene::Rect::new(0., 0., 200., 100.),
        radii: [16.; 4],
    };
    // The caption is inside the card alone.
    assert_eq!(items_rounded(&ui, "caption", &view), [Some(card), None]);
    // The photo inside the thumbnail too, its 50 px corner held to half the
    // thumbnail's 40 px height.
    let thumb = RoundRect {
        rect: zgx::scene::Rect::new(0., 0., 60., 40.),
        radii: [20., 0., 6., 0.],
    };
    assert_eq!(
        items_rounded(&ui, "photo", &view),
        [Some(thumb), Some(card)]
    );
}

#[test]
fn square_or_unclipped_boxes_clip_no_corners() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column()
            .child(
                div()
                    .w(50.)
                    .h(50.)
                    .rounded(8.)
                    .child(div().id("a").w(10.).h(10.)),
            )
            .child(
                div()
                    .w(50.)
                    .h(50.)
                    .overflow_hidden()
                    .child(div().id("b").w(10.).h(10.)),
            ),
    );
    ui.prepare_frame();
    assert_eq!(items_rounded(&ui, "a", &view), [None, None]);
    assert_eq!(items_rounded(&ui, "b", &view), [None, None]);
}

#[test]
fn border_over_reaches_the_panel() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            div()
                .id("frame")
                .w(50.)
                .h(50.)
                .border(2.)
                .border_color(zgx::scene::Color(255, 255, 255, 255))
                .border_over(true),
        ),
    );
    ui.prepare_frame();
    let node = view.find("frame").unwrap();
    let scene = ui.scene.borrow();
    assert!(matches!(scene.kind(node),
        zgx::scene::NodeKind::Panel { quad, .. } if quad.border_over && quad.border_width == 2.));
}

#[test]
fn bg_image_sets_the_panels_background_over_its_colour() {
    let image = std::sync::Arc::new(zgx::image::ImageData::new(1, 1, vec![255; 4]).unwrap());
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            div()
                .id("thumb")
                .w(50.)
                .h(50.)
                .bg(zgx::scene::Color(9, 9, 9, 255))
                .bg_image(image.clone(), ObjectFit::Cover)
                .rounded(8.),
        ),
    );
    ui.prepare_frame();
    let node = view.find("thumb").unwrap();
    let scene = ui.scene.borrow();
    let zgx::scene::NodeKind::Panel { quad, .. } = scene.kind(node) else {
        panic!("a panel")
    };
    assert_eq!(quad.fill, zgx::scene::Color(9, 9, 9, 255));
    assert!(matches!(
        quad.decoration.as_deref().and_then(|d| d.background.as_ref()),
        Some(zgx::decoration::Background::Image { image: shown, fit: ObjectFit::Cover })
            if std::sync::Arc::ptr_eq(shown, &image)
    ));
}

#[test]
fn fade_sides_reach_descendants_as_four_bands() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            row()
                .w(200.)
                .h(40.)
                .overflow_hidden()
                .fade_sides(0., 28., 0., 4.)
                .fade_insets_x(0., 2.)
                .child(div().id("chip").w(500.).h(40.)),
        ),
    );
    ui.prepare_frame();
    let node = view.find("chip").unwrap();
    let scene = ui.scene.borrow();
    let item = scene
        .layer_items(None)
        .into_iter()
        .find(|item| item.id == node)
        .unwrap();
    let (rect, bands) = item.mask().expect("faded");
    assert_eq!(bands, [0., 28., 0., 4.]);
    assert_eq!(rect, zgx::scene::Rect::new(0., 0., 198., 40.));
}

#[test]
fn clip_inset_styles_reach_the_scene_and_animate_alone() {
    use zgx::layout::Length::{Percent, Px};
    let mut ui = Ui::new(400., 300.);
    let closed = ui.signal(0_f32);
    let read = closed.clone();
    let view = ui.mount(
        column().child(
            div()
                .id("sheet")
                .w(100.)
                .h(80.)
                .bg(zgx::scene::Color(1, 2, 3, 255))
                .clip_inset(-20., -20., -20., -20.)
                .reactive_style(move || Styles::new().clip_inset_percent(0., 0., read.get(), 0.)),
        ),
    );
    ui.prepare_frame();
    let node = view.find("sheet").unwrap();
    let inset = |ui: &Ui| ui.scene.borrow().clip_inset(node).unwrap();
    assert_eq!(inset(&ui).bottom, Percent(0.));
    ui.scene.borrow_mut().flush();
    closed.set(50.);
    ui.prepare_frame();
    assert_eq!(inset(&ui).bottom, Percent(0.5));
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
    assert!(!frame.damage.is_empty());
    // The px form is the base the reactive percent replaced.
    assert_ne!(inset(&ui).top, Px(-20.));
}

#[test]
fn bg_shader_and_uniforms_reach_the_scene() {
    use zgx::decoration::{Background, FillShader, ShaderUniforms};
    let shader = FillShader::new(
        "fn fill(input: FillInput) -> vec4<f32> { return vec4(1.0); }",
        Some(30.),
    );
    let uniforms = ShaderUniforms {
        time: 0.5,
        params: [[1., 2., 3., 4.], [0.; 4], [0.; 4], [0.; 4]],
    };
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            div()
                .id("orb")
                .w(50.)
                .h(50.)
                .bg_shader(shader.clone())
                .shader_uniforms(uniforms),
        ),
    );
    ui.prepare_frame();
    let node = view.find("orb").unwrap();
    let scene = ui.scene.borrow();
    assert!(matches!(scene.kind(node),
        zgx::scene::NodeKind::Panel { quad, .. }
            if matches!(quad.decoration.as_deref().and_then(|d| d.background.as_ref()),
                Some(Background::Shader(s)) if s.id() == shader.id())));
    assert_eq!(scene.shader_uniforms(node), uniforms);
}

#[test]
fn colour_filter_styles_animate_without_layout() {
    let mut ui = Ui::new(400., 300.);
    let hover = ui.signal(1_f32);
    let read = hover.clone();
    let view = ui.mount(
        column().child(
            div()
                .id("button")
                .w(80.)
                .h(30.)
                .bg(zgx::scene::Color(40, 40, 40, 255))
                .saturate(0.5)
                .reactive_style(move || Styles::new().brightness(read.get())),
        ),
    );
    ui.prepare_frame();
    let node = view.find("button").unwrap();
    ui.scene.borrow_mut().flush();
    hover.set(1.2);
    ui.prepare_frame();
    let effects = ui.scene.borrow().effects(node);
    assert_eq!((effects.brightness, effects.saturate), (1.2, 0.5));
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!((frame.layout_nodes, frame.paint_nodes), (0, 0));
    assert!(!frame.damage.is_empty());
}

#[test]
fn outlines_and_stroke_alignment_reach_the_panel_and_animate_without_layout() {
    use zgx::scene::{Color, NodeKind, Outline};
    let white = Color(255, 255, 255, 255);
    let mut ui = Ui::new(400., 300.);
    let width = ui.signal(2_f32);
    let read = width.clone();
    let view = ui.mount(
        column()
            .child(div().id("ring").w(50.).h(50.).reactive_style(move || {
                Styles::new().outline(read.get(), white).outline_offset(3.)
            }))
            .child(
                div()
                    .id("center")
                    .w(50.)
                    .h(50.)
                    .stroke(4., white, StrokeAlign::Center),
            )
            .child(
                div()
                    .id("outside")
                    .w(50.)
                    .h(50.)
                    .stroke(4., white, StrokeAlign::Outside),
            )
            .child(
                div()
                    .id("inside")
                    .w(50.)
                    .h(50.)
                    .stroke(4., white, StrokeAlign::Inside),
            )
            .child(
                div()
                    .id("none")
                    .w(50.)
                    .h(50.)
                    .outline(2., white)
                    .outline_none(),
            ),
    );
    ui.prepare_frame();
    let outline = |id: &str| {
        let scene = ui.scene.borrow();
        let NodeKind::Panel { quad, .. } = scene.kind(view.find(id).unwrap()) else {
            panic!("{id} is a panel")
        };
        (quad.outline, quad.border_width, quad.border_over)
    };
    let ring = |width, offset| {
        Some(Outline {
            width,
            color: white,
            offset,
        })
    };
    assert_eq!(outline("ring"), (ring(2., 3.), 0., false));
    assert_eq!(outline("center"), (ring(4., -2.), 0., false));
    assert_eq!(outline("outside"), (ring(4., 0.), 0., false));
    assert_eq!(outline("inside"), (None, 4., true));
    // Nothing left to paint: a plain container.
    let none = view.find("none").unwrap();
    assert!(matches!(
        ui.scene.borrow().kind(none),
        NodeKind::Container(_)
    ));
    // Its width changes paint alone: an outline takes no layout.
    let node = view.find("ring").unwrap();
    let before = ui.scene.borrow().bounds(node);
    ui.scene.borrow_mut().flush();
    width.set(6.);
    ui.prepare_frame();
    assert_eq!(outline("ring").0, ring(6., 3.));
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(frame.layout_nodes, 0);
    assert!(!frame.damage.is_empty());
    assert_eq!(ui.scene.borrow().bounds(node), before);
}

#[test]
fn fade_overflow_shows_only_the_bands_content_runs_past() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        column().child(
            column()
                .id("view")
                .w(200.)
                .h(100.)
                .overflow_hidden()
                .fade_edges(14., 18.)
                .fade_overflow(true)
                .child(
                    column()
                        .id("list")
                        .shrink_0()
                        .h(300.)
                        .child(div().id("row").h(20.)),
                ),
        ),
    );
    let bands = |ui: &mut Ui| {
        ui.prepare_frame();
        // A frame's flush measures what runs past the edges.
        ui.scene.borrow_mut().flush();
        let node = view.find("row").unwrap();
        let scene = ui.scene.borrow();
        let item = scene
            .layer_items(None)
            .into_iter()
            .find(|item| item.id == node)
            .unwrap();
        item.mask().map(|(_, bands)| bands)
    };
    // At the top: content runs past the bottom only.
    assert_eq!(bands(&mut ui), Some([0., 0., 18., 0.]));
    // Scrolled 5 points: the top band is as deep as what went under it.
    let list = view.find("list").unwrap();
    let scroll = |ui: &mut Ui, y: f32| {
        ui.scene
            .borrow_mut()
            .set_transform(list, zgx::scene::Transform { x: 0., y: -y });
    };
    scroll(&mut ui, 5.);
    assert_eq!(bands(&mut ui), Some([5., 0., 18., 0.]));
    // At the end: only the top band, whole.
    scroll(&mut ui, 200.);
    assert_eq!(bands(&mut ui), Some([14., 0., 0., 0.]));
}

#[test]
fn fade_overflow_measures_a_scroll_views_content() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0_f32);
    let view = ui.mount(
        column().child(
            zgx::compose::scroll(offset.clone())
                .id("view")
                .w(200.)
                .h(100.)
                .fade_edges(14., 18.)
                .fade_overflow(true)
                .child(column().id("list").h(300.).child(div().id("row").h(20.))),
        ),
    );
    let bands = |ui: &mut Ui| {
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        let node = view.find("row").unwrap();
        let scene = ui.scene.borrow();
        let item = scene
            .layer_items(None)
            .into_iter()
            .find(|item| item.id == node)
            .unwrap();
        item.mask().map(|(_, bands)| bands)
    };
    // The scrolled content sits under the scroll view's own viewport: its
    // overflow counts all the same.
    assert_eq!(bands(&mut ui), Some([0., 0., 18., 0.]));
    offset.set(5.);
    assert_eq!(bands(&mut ui), Some([5., 0., 18., 0.]));
}
