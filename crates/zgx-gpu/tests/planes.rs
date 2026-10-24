//! Planes around native views: the base renderer leaves out paint above a
//! native view; an overlay renderer draws just that paint, into a surface
//! covering only its box (`zgx::native_view`).
#![cfg(target_os = "macos")]
use std::rc::Rc;
use zgx::{
    native_view::{Plane, PlaneSplit},
    scene::*,
};
use zgx_gpu::GpuRenderer;

fn fixed(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}
/// A pixel of a readback (red, green, blue, alpha).
fn channels(bytes: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
}

#[test]
fn base_and_overlay_planes_draw_their_own_paint() {
    let mut scene = Scene::new(200., 200.);
    let root = scene.root();
    scene.set_kind(root, NodeKind::Container(Layout::Overlay));
    scene.append(
        root,
        NodeKind::Rect(Color(10, 20, 30, 255)),
        fixed(200., 200.),
    );
    // The native view's node: paints nothing, the platform shows it.
    let native = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        fixed(100., 100.),
    );
    scene.set_transform(native, Transform { x: 20., y: 20. });
    scene.set_platform_painted(native, true);
    // Over the native view: the overlay plane.
    let over = scene.append(root, NodeKind::Rect(Color(255, 0, 0, 255)), fixed(40., 40.));
    scene.set_transform(over, Transform { x: 80., y: 80. });
    // Beside it: the base plane.
    let beside = scene.append(root, NodeKind::Rect(Color(0, 0, 255, 255)), fixed(30., 30.));
    scene.set_transform(beside, Transform { x: 160., y: 160. });
    let report = scene.flush();
    let (split, _) = PlaneSplit::compute(&scene, &[native], &PlaneSplit::default());
    assert!(split.in_overlay(over) && !split.in_overlay(beside));
    let split = Rc::new(split);

    let mut base = GpuRenderer::new(200, 200).unwrap();
    base.set_plane(split.clone(), Plane::Base);
    base.render(&scene, &report.damage).unwrap();
    let pixels = base.readback().unwrap();
    assert_eq!(
        channels(&pixels, 200, 100, 100),
        [10, 20, 30, 255],
        "overlay paint left out"
    );
    assert_eq!(
        channels(&pixels, 200, 175, 175),
        [0, 0, 255, 255],
        "base paint drawn"
    );

    let host = objc2_quartz_core::CALayer::new();
    let mut overlay = GpuRenderer::for_overlay_layer(host, &base.context(), 1.).unwrap();
    overlay.set_plane(split.clone(), Plane::Overlay);
    overlay.set_plane_rect(Rect::new(70., 70., 60., 60.));
    assert_eq!(overlay.plane_rect(), Some(Rect::new(70., 70., 60., 60.)));
    overlay
        .render(&scene, &[Rect::new(0., 0., 200., 200.)])
        .unwrap();
    let pixels = overlay.readback().unwrap();
    assert_eq!(pixels.len(), 60 * 60 * 4, "a surface the size of its box");
    assert_eq!(
        channels(&pixels, 60, 30, 30),
        [255, 0, 0, 255],
        "overlay paint, offset"
    );
    assert_eq!(channels(&pixels, 60, 3, 3)[3], 0, "transparent elsewhere");

    // The overlay paint moves off the native view: both planes repaint it.
    scene.set_transform(over, Transform { x: 130., y: 20. });
    let report = scene.flush();
    let (moved, damage) = PlaneSplit::compute(&scene, &[native], &split);
    assert!(!moved.in_overlay(over));
    let moved = Rc::new(moved);
    let damage: Vec<Rect> = report.damage.iter().copied().chain(damage).collect();
    base.set_plane(moved.clone(), Plane::Base);
    base.render(&scene, &damage).unwrap();
    let pixels = base.readback().unwrap();
    assert_eq!(
        channels(&pixels, 200, 150, 40),
        [255, 0, 0, 255],
        "now base paint"
    );
    assert_eq!(channels(&pixels, 200, 100, 100), [10, 20, 30, 255]);
    overlay.set_plane(moved, Plane::Overlay);
    overlay.render(&scene, &damage).unwrap();
    let pixels = overlay.readback().unwrap();
    assert_eq!(channels(&pixels, 60, 30, 30)[3], 0, "gone from the overlay");
}

/// A frosted panel's drop shadow over a native view: the overlay box holds
/// all of it, and it fades out softly instead of stopping at the panel.
#[test]
fn overlay_glass_keeps_its_whole_soft_shadow() {
    let mut scene = Scene::new(400., 400.);
    let root = scene.root();
    scene.set_kind(root, NodeKind::Container(Layout::Overlay));
    let native = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        fixed(400., 400.),
    );
    scene.set_platform_painted(native, true);
    let shadow = BoxShadow {
        color: Color(0, 0, 0, 255),
        offset: Transform { x: 0., y: 12. },
        blur_radius: 16.,
        spread: 0.,
    };
    let panel = scene.append(
        root,
        NodeKind::Quad(QuadStyle {
            fill: Color(42, 41, 37, 160),
            radius: 12.,
            border_color: Color(255, 255, 255, 31),
            border_width: 1.,
            shadow: Some(shadow),
            ..Default::default()
        }),
        fixed(160., 120.),
    );
    scene.set_transform(panel, Transform { x: 120., y: 100. });
    scene.set_effects(
        panel,
        Effects {
            blur_radius: 24.,
            ..Default::default()
        },
    );
    scene.flush();
    let (split, _) = PlaneSplit::compute(&scene, &[native], &PlaneSplit::default());
    let bounds = split.overlay_bounds().unwrap();
    // The shadow reaches three sigmas past the offset box.
    assert!(
        bounds.y + bounds.height >= 100. + 120. + 12. + 3. * 16. - 0.5,
        "{bounds:?}"
    );
    assert!(bounds.x <= 120. - 3. * 16. + 0.5, "{bounds:?}");
    let split = Rc::new(split);
    let base = GpuRenderer::new(400, 400).unwrap();
    let host = objc2_quartz_core::CALayer::new();
    let mut overlay = GpuRenderer::for_overlay_layer(host, &base.context(), 1.).unwrap();
    overlay.set_plane(split.clone(), Plane::Overlay);
    overlay.set_plane_rect(bounds);
    overlay
        .render(&scene, &[Rect::new(0., 0., 400., 400.)])
        .unwrap();
    let rect = overlay.plane_rect().unwrap();
    let pixels = overlay.readback().unwrap();
    let width = rect.width as usize;
    let at =
        |x: f32, y: f32| channels(&pixels, width, (x - rect.x) as usize, (y - rect.y) as usize)[3];
    // Below the panel, in its shadow: darkened, fading with distance.
    let near = at(200., 100. + 120. + 6.);
    let far = at(200., 100. + 120. + 40.);
    assert!(near > 40, "shadow just under the panel: {near}");
    assert!(
        far > 0 && far < near,
        "softly fading: near {near}, far {far}"
    );
    // Beside the panel's left edge, the shadow fades out gradually too.
    let edge = at(116., 170.);
    let out = at(100., 170.);
    assert!(edge > out && out > 0, "left edge {edge}, further {out}");
}

/// The same through components: a frosted popover with a drop shadow, over
/// a page, as an app builds it.
#[test]
fn composed_popover_shadow_over_a_page() {
    use zgx::{compose::prelude::*, native_view::NativeViews, web_view::*, widgets::Ui};
    let mut ui = Ui::new(400., 400.);
    let natives = NativeViews::new();
    natives.set_supported(true);
    let web = WebView::new(&ui.runtime, WebViewOptions::default());
    let open = ui.signal(true);
    let view = ui.mount(provide(
        natives.clone(),
        overlay()
            .w(400.)
            .h(400.)
            .child(web_view(&web).w(400.).h(400.))
            .child(
                popover("Menu", open.clone(), button().id("anchor").w(60.).h(28.))
                    .id("menu")
                    .w(160.)
                    .h(120.)
                    .rounded(12.)
                    .blur(24.)
                    .bg(rgba(0x2a2925a0))
                    .border(1.)
                    .border_color(rgba(0xffffff1f))
                    .shadow(BoxShadow {
                        color: rgba(0x000000ff),
                        offset: Transform { x: 0., y: 12. },
                        blur_radius: 16.,
                        spread: 0.,
                    }),
            ),
    ));
    ui.prepare_frame();
    let menu = view.find("menu").unwrap();
    let mut scene = ui.scene.borrow_mut();
    let report = scene.flush();
    let _ = report;
    let panel = scene.bounds(menu);
    let (split, _) = PlaneSplit::compute(&scene, &natives.nodes(), &PlaneSplit::default());
    assert!(split.in_overlay(menu), "{panel:?}");
    let bounds = split.overlay_bounds().unwrap();
    assert!(
        bounds.y + bounds.height >= panel.y + panel.height + 12. + 40.,
        "{bounds:?} vs {panel:?}"
    );
    let split = Rc::new(split);
    let base = GpuRenderer::new(400, 400).unwrap();
    let host = objc2_quartz_core::CALayer::new();
    let mut overlay = GpuRenderer::for_overlay_layer(host, &base.context(), 1.).unwrap();
    overlay.set_plane(split, Plane::Overlay);
    overlay.set_plane_rect(bounds);
    overlay
        .render(&scene, &[Rect::new(0., 0., 400., 400.)])
        .unwrap();
    let rect = overlay.plane_rect().unwrap();
    let pixels = overlay.readback().unwrap();
    let width = rect.width as usize;
    let at =
        |x: f32, y: f32| channels(&pixels, width, (x - rect.x) as usize, (y - rect.y) as usize)[3];
    let mid = panel.x + panel.width / 2.;
    let near = at(mid, panel.y + panel.height + 6.);
    let far = at(mid, panel.y + panel.height + 40.);
    assert!(
        near > 40 && far > 0 && far < near,
        "{panel:?}: near {near}, far {far}"
    );
}
