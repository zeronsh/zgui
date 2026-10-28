//! GPU views drawn by their painters (`zgx_gpu::viewport`): a frame is drawn
//! once, at its box's size in device pixels, and composited as an image is
//! (clipped, rounded, faded, transformed); unchanged, it shows again with no
//! call to its painter.
use std::{cell::RefCell, rc::Rc};
use zgx::{compose::prelude::*, gpu_view::GpuView, reactive::Signal, scene::Color, widgets::Ui};
use zgx_gpu::{
    GpuRenderer,
    viewport::{Viewport, ViewportFrame},
    wgpu,
};

/// Clears its frames to a colour, noting each frame's size.
struct Fill {
    color: Rc<RefCell<wgpu::Color>>,
    drawn: Rc<RefCell<Vec<(u32, u32)>>>,
    devices: Rc<RefCell<usize>>,
}
impl Viewport for Fill {
    fn render(&mut self, frame: &mut ViewportFrame<'_>) {
        self.drawn.borrow_mut().push(frame.size);
        frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fill"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(*self.color.borrow()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
    }
    fn device_changed(&mut self) {
        *self.devices.borrow_mut() += 1;
    }
}

struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    view: GpuView,
    color: Rc<RefCell<wgpu::Color>>,
    drawn: Rc<RefCell<Vec<(u32, u32)>>>,
    /// Elsewhere in the window: a box whose colour changes.
    other: Signal<bool>,
    width: Signal<f32>,
    scale: f32,
}
const RED: wgpu::Color = wgpu::Color {
    r: 1.,
    g: 0.,
    b: 0.,
    a: 1.,
};
const BACKGROUND: Color = Color(16, 32, 48, 255);
impl Window {
    /// A 200 by 100 window at `scale`, the view's box at (20, 10), 60 by 40,
    /// inside `wrap`.
    fn new(scale: f32, wrap: impl FnOnce(View) -> View) -> Option<Self> {
        let mut gpu = GpuRenderer::new((200. * scale) as u32, (100. * scale) as u32).ok()?;
        gpu.set_scale_factor(scale);
        let color = Rc::new(RefCell::new(RED));
        let drawn = Rc::default();
        let view = zgx_gpu::viewport::view(Fill {
            color: color.clone(),
            drawn: Rc::clone(&drawn),
            devices: Rc::default(),
        });
        let mut ui = Ui::new(200., 100.);
        let other = ui.signal(false);
        let width = ui.signal(60.);
        let (lit, w) = (other.clone(), width.clone());
        ui.mount(
            div()
                .size(200., 100.)
                .bg(BACKGROUND)
                .child(wrap(
                    gpu_view("Model", &view)
                        .h(40.)
                        .reactive_style(move || Styles::new().w(w.get())),
                ))
                .child(
                    div()
                        .absolute()
                        .ml(150.)
                        .mt(70.)
                        .size(20., 20.)
                        .reactive_style(move || {
                            Styles::new().bg(if lit.get() {
                                Color(255, 255, 255, 255)
                            } else {
                                Color(0, 0, 0, 255)
                            })
                        }),
                ),
        );
        Some(Self {
            ui,
            gpu,
            view,
            color,
            drawn,
            other,
            width,
            scale,
        })
    }
    fn plain(scale: f32) -> Option<Self> {
        Self::new(scale, |view| view.absolute().ml(20.).mt(10.))
    }
    fn draw(&mut self) -> zgx_gpu::GpuStats {
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap()
    }
    /// The pixel at (`x`, `y`) points, RGBA.
    fn pixel(&self, x: f32, y: f32) -> [u8; 4] {
        let scale = self.scale;
        let width = (200. * scale) as usize;
        let pixels = self.gpu.readback().unwrap();
        let at = ((y * scale) as usize * width + (x * scale) as usize) * 4;
        pixels[at..at + 4].try_into().unwrap()
    }
}

#[test]
fn a_view_draws_a_frame_its_box_s_size_in_device_pixels() {
    let Some(mut window) = Window::plain(2.) else {
        return;
    };
    window.draw();
    assert_eq!(*window.drawn.borrow(), [(120, 80)]);
    assert_eq!(window.pixel(50., 30.), [255, 0, 0, 255]);
    assert_eq!(window.pixel(10., 5.), [16, 32, 48, 255]);
    assert_eq!(
        window.pixel(85., 30.),
        [16, 32, 48, 255],
        "right of its box"
    );
}

#[test]
fn changes_elsewhere_show_its_frame_without_drawing_one() {
    let Some(mut window) = Window::plain(1.) else {
        return;
    };
    window.draw();
    for lit in [true, false, true] {
        window.other.set(lit);
        window.draw();
    }
    assert_eq!(window.drawn.borrow().len(), 1);
    assert_eq!(window.pixel(50., 30.), [255, 0, 0, 255]);
    assert_eq!(window.pixel(160., 80.), [255, 255, 255, 255]);
}

#[test]
fn an_invalidated_view_draws_one_frame_patched_into_the_kept_window() {
    let Some(mut window) = Window::plain(1.) else {
        return;
    };
    window.draw();
    let textures = window.gpu.debug_cache_stats().image_textures;
    let mut patched = 0;
    for frame in 0..8 {
        *window.color.borrow_mut() = wgpu::Color {
            g: f64::from(frame) / 8.,
            ..RED
        };
        window.view.invalidate();
        window.view.invalidate();
        let stats = window.draw();
        patched += usize::from(stats.patched_items > 0);
        assert!(
            stats.damaged_pixels <= 60 * 40,
            "its box alone: {}",
            stats.damaged_pixels
        );
        assert_eq!(
            window.gpu.debug_cache_stats().image_textures,
            textures,
            "its texture replaced in place"
        );
    }
    assert_eq!(window.drawn.borrow().len(), 9, "one frame each");
    assert!(patched >= 6, "{patched} frames patched");
    assert_eq!(window.pixel(50., 30.), [255, 223, 0, 255]);
}

#[test]
fn a_resized_view_draws_at_its_new_size() {
    let Some(mut window) = Window::plain(1.) else {
        return;
    };
    window.draw();
    window.width.set(100.25);
    window.draw();
    assert_eq!(*window.drawn.borrow(), [(60, 40), (101, 40)]);
    assert_eq!(window.pixel(110., 30.), [255, 0, 0, 255]);
    // A new scale: drawn again at it.
    window.scale = 2.;
    window.gpu.set_scale_factor(2.);
    window.gpu.resize(400, 200);
    window.ui.prepare_frame();
    window.ui.scene.borrow_mut().flush();
    window
        .gpu
        .render(
            &window.ui.scene.borrow(),
            &[zgx::scene::Rect::new(0., 0., 200., 100.)],
        )
        .unwrap();
    assert_eq!(window.drawn.borrow().last(), Some(&(201, 80)));
}

#[test]
fn a_view_composites_as_an_image_does() {
    // Inside a rounded clip, its corner is cut away.
    let Some(mut rounded) = Window::new(1., |view| {
        div()
            .absolute()
            .ml(20.)
            .mt(10.)
            .size(60., 40.)
            .rounded(16.)
            .overflow_hidden()
            .child(view)
    }) else {
        return;
    };
    rounded.draw();
    assert_eq!(rounded.pixel(21., 11.), [16, 32, 48, 255]);
    assert_eq!(rounded.pixel(50., 30.), [255, 0, 0, 255]);
    // Half opaque, it mixes with what lies behind it.
    let mut faded = Window::new(1., |view| view.absolute().ml(20.).mt(10.).opacity(0.5)).unwrap();
    faded.draw();
    let [r, g, b, _] = faded.pixel(50., 30.);
    assert!((134..=137).contains(&r) && (14..=18).contains(&g) && (22..=26).contains(&b));
    // Turned about its vertical axis in perspective, it narrows.
    let mut turned = Window::new(1., |view| {
        view.absolute()
            .ml(20.)
            .mt(10.)
            .perspective(200.)
            .rotate_y(60_f32.to_radians())
    })
    .unwrap();
    turned.draw();
    assert_eq!(turned.pixel(50., 30.), [255, 0, 0, 255]);
    assert_eq!(
        turned.pixel(22., 30.),
        [16, 32, 48, 255],
        "its left edge turned away"
    );
    assert_eq!(*turned.drawn.borrow(), [(60, 40)]);
}

#[test]
fn a_view_its_app_let_go_of_draws_while_its_box_shows_it() {
    let Some(mut window) = Window::plain(1.) else {
        return;
    };
    let view = std::mem::replace(&mut window.view, GpuView::new(()));
    let id = view.id();
    drop(view);
    window.draw();
    window.width.set(70.);
    window.draw();
    assert_eq!(window.drawn.borrow().len(), 2, "its box keeps it");
    assert_eq!(window.pixel(85., 30.), [255, 0, 0, 255]);
    assert!(zgx::gpu_view::painter(id).is_some());
}

#[test]
fn a_view_drawn_by_a_renderer_on_another_device_hears_its_device_changed() {
    let devices = Rc::new(RefCell::new(0));
    let view = zgx_gpu::viewport::view(Fill {
        color: Rc::new(RefCell::new(RED)),
        drawn: Rc::default(),
        devices: devices.clone(),
    });
    let (Ok(mut a), Ok(mut b)) = (GpuRenderer::new(16, 16), GpuRenderer::new(16, 16)) else {
        return;
    };
    assert!(!a.context().shares_device(&b.context()));
    let draw = |gpu: &mut GpuRenderer| {
        let mut ui = Ui::new(16., 16.);
        ui.mount(gpu_view("Model", &view).size(16., 16.));
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
    };
    draw(&mut a);
    draw(&mut b);
    draw(&mut b);
    assert_eq!(*devices.borrow(), 1, "once, on the second device");
}
