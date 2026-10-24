use softbuffer::{Context, Surface};
use std::{
    cell::RefCell, collections::BTreeMap, num::NonZeroU32, rc::Rc, sync::Arc, time::Instant,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use zgx::{
    collections::List,
    reactive::{Effect, Runtime, Signal},
    scene::{Color, Effects, Layout, NodeId, NodeKind, Scene, Style, Transform},
    task::{LocalExecutor, yield_now},
};
use zgx_desktop::{presentation::Presentation, raster::Raster};
use zgx_gpu::GpuRenderer;
use zgx_workload::{Mode, PERIOD, ROW_HEIGHT, Workload, row_label};

const FG: Color = Color(229, 237, 247, 255);
fn style(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}
fn place(
    scene: &mut Scene,
    parent: NodeId,
    kind: NodeKind,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> NodeId {
    let id = scene.append(parent, kind, style(w, h));
    scene.set_transform(id, Transform { x, y });
    id
}
#[allow(clippy::too_many_arguments)] // Fixed demo geometry keeps call sites readable.
fn label(
    scene: &mut Scene,
    parent: NodeId,
    text: impl Into<std::sync::Arc<str>>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    size: f32,
) -> NodeId {
    place(
        scene,
        parent,
        NodeKind::Text {
            text: text.into(),
            color: FG,
            font_size: size,
        },
        x,
        y,
        w,
        h,
    )
}
struct Demo {
    scene: Rc<RefCell<Scene>>,
    runtime: Runtime,
    list: List<i32>,
    done: Signal<i32>,
    _effects: Vec<Effect>,
    workload: Workload,
    stream: NodeId,
    list_root: NodeId,
    rows: BTreeMap<usize, NodeId>,
    window: Option<Arc<Window>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    raster: Option<Raster>,
    gpu: Option<GpuRenderer>,
    software_pixels: Vec<u32>,
    physical_size: (u32, u32),
    scale_factor: f32,
    presentation: Presentation,
    start: Instant,
    next: Instant,
    cursor: (f64, f64),
    tasks: LocalExecutor,
    frames: u64,
    layout_nodes: u64,
    paint_nodes: u64,
    composite_nodes: u64,
}
impl Demo {
    fn new() -> Self {
        let mut scene = Scene::new(960., 720.);
        let root = scene.root();
        // Overlay children have fixed geometry; updates cannot invalidate sibling layout.
        let canvas = scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            style(960., 720.),
        );
        label(
            &mut scene,
            canvas,
            "zgx performance lab",
            20.,
            20.,
            920.,
            28.,
            20.,
        );
        for (x, text) in [(20., "+"), (80., "="), (140., "0")] {
            place(
                &mut scene,
                canvas,
                NodeKind::Rect(Color(41, 59, 80, 255)),
                x,
                55.,
                50.,
                32.,
            );
            if x != 140. {
                label(&mut scene, canvas, text, x + 12., 61., 38., 24., 14.);
            }
        }
        let done_node = label(&mut scene, canvas, "0", 152., 61., 38., 24., 14.);
        let count = label(&mut scene, canvas, "0", 220., 61., 50., 24., 14.);
        let title = label(&mut scene, canvas, "stable", 280., 61., 80., 24., 14.);
        let slot_count = label(&mut scene, canvas, "0", 370., 61., 50., 24., 14.);
        place(
            &mut scene,
            canvas,
            NodeKind::Rect(Color(25, 34, 48, 255)),
            20.,
            110.,
            920.,
            160.,
        );
        let stream = label(&mut scene, canvas, "", 28., 118., 904., 144., 14.);
        let mut viewport = style(920., 400.);
        viewport.clip = true;
        let list_root = scene.append(canvas, NodeKind::Container(Layout::Overlay), viewport);
        scene.set_transform(list_root, Transform { x: 20., y: 290. });
        if std::env::var_os("ZGX_EFFECTS").is_some() {
            let glass = place(
                &mut scene,
                canvas,
                NodeKind::Rect(Color(130, 170, 210, 100)),
                650.,
                130.,
                260.,
                100.,
            );
            scene.set_effects(
                glass,
                Effects {
                    opacity: 0.85,
                    blur_radius: 8.,
                    edge_fade: 16.,
                    filter_blur: 0.,
                    brightness: 1.,
                    contrast: 1.,
                    saturate: 1.,
                },
            );
            label(
                &mut scene,
                canvas,
                "Backdrop blur + transparency",
                665.,
                168.,
                235.,
                30.,
                14.,
            );
        }
        let scene = Rc::new(RefCell::new(scene));
        let runtime = Runtime::new();
        let list = List::new(&runtime, 4);
        let done = runtime.signal(0);
        let title_signal = runtime.signal(String::from("stable"));
        let mut effects = Vec::new();
        for node in [count, slot_count] {
            let list = list.clone();
            let scene = scene.clone();
            effects.push(runtime.effect(move || {
                scene.borrow_mut().set_text(node, list.len().to_string());
            }));
        }
        {
            let done = done.clone();
            let scene = scene.clone();
            effects.push(runtime.effect(move || {
                scene
                    .borrow_mut()
                    .set_text(done_node, done.get().to_string());
            }));
        }
        {
            let scene = scene.clone();
            effects.push(runtime.effect(move || {
                title_signal.with(|value| scene.borrow_mut().set_text(title, value.as_str()));
            }));
        }
        let now = Instant::now();
        let mut app = Self {
            scene,
            runtime,
            list,
            done,
            _effects: effects,
            workload: Workload::from_env(),
            stream,
            list_root,
            rows: BTreeMap::new(),
            window: None,
            surface: None,
            raster: None,
            gpu: None,
            software_pixels: Vec::new(),
            physical_size: (960, 720),
            scale_factor: 1.0,
            presentation: Presentation::default(),
            start: now,
            next: now + PERIOD,
            cursor: (0., 0.),
            tasks: LocalExecutor::new(),
            frames: 0,
            layout_nodes: 0,
            paint_nodes: 0,
            composite_nodes: 0,
        };
        app.scene
            .borrow_mut()
            .set_text(app.stream, app.workload.visible_text());
        app.sync_rows();
        app
    }
    fn sync_rows(&mut self) {
        let range = self.workload.row_range();
        let mut scene = self.scene.borrow_mut();
        let removed: Vec<_> = self
            .rows
            .keys()
            .filter(|i| !range.contains(i))
            .copied()
            .collect();
        for i in removed {
            scene.remove(self.rows.remove(&i).unwrap());
        }
        for i in range {
            let node = *self.rows.entry(i).or_insert_with(|| {
                let row = scene.append(
                    self.list_root,
                    NodeKind::Container(Layout::Overlay),
                    style(920., ROW_HEIGHT),
                );
                place(
                    &mut scene,
                    row,
                    NodeKind::Rect(if i % 2 == 0 {
                        Color(25, 34, 48, 255)
                    } else {
                        Color(29, 40, 56, 255)
                    }),
                    0.,
                    0.,
                    920.,
                    ROW_HEIGHT,
                );
                label(&mut scene, row, row_label(i), 8., 4., 904., 24., 14.);
                row
            });
            scene.set_transform(
                node,
                Transform {
                    x: 0.,
                    y: i as f32 * ROW_HEIGHT - self.workload.scroll_offset,
                },
            );
        }
    }
    fn tick(&mut self) {
        self.workload.tick();
        if matches!(self.workload.mode, Mode::Stream | Mode::Both) {
            self.scene
                .borrow_mut()
                .set_text(self.stream, self.workload.visible_text());
        }
        if matches!(self.workload.mode, Mode::Scroll | Mode::Both) {
            self.sync_rows();
        }
    }
    fn action(&mut self, action: u8) {
        if action == 2 {
            self.done.set(999);
            self.draw(false);
            return;
        }
        let runtime = self.runtime.clone();
        let list = self.list.clone();
        let done = self.done.clone();
        self.tasks.spawn(async move {
            yield_now().await;
            runtime.batch(|| {
                let result = match action {
                    0 => (|| {
                        for n in [1, 2, 3] {
                            list.append(n)?;
                        }
                        Ok::<_, zgx::collections::ListError>(())
                    })(),
                    _ => (|| {
                        let row = list.at(0)?;
                        row.write(99)?;
                        row.write(99)?;
                        Ok(())
                    })(),
                };
                done.set(match result {
                    Ok(()) => 0,
                    Err(
                        zgx::collections::ListError::Exhausted
                        | zgx::collections::ListError::RowRemoved,
                    ) => -2,
                    Err(_) => -1,
                });
            });
        });
    }
    fn draw(&mut self, force: bool) {
        let mut scene = self.scene.borrow_mut();
        let report = scene.flush();
        self.layout_nodes += report.layout_nodes as u64;
        self.paint_nodes += report.paint_nodes as u64;
        self.composite_nodes += report.composite_nodes as u64;
        if report.damage.is_empty() && !force {
            return;
        }
        let full = [scene.bounds(scene.root())];
        let damage = if force { &full[..] } else { &report.damage };
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.render(&scene, damage).expect("GPU render");
            gpu.present().expect("GPU present");
        } else {
            let raster = self.raster.get_or_insert_with(|| Raster::new(960, 720));
            raster.render(&scene, damage);
            if let Some(surface) = self.surface.as_mut() {
                let mut buffer = surface.buffer_mut().expect("window buffer");
                let age = buffer.age();
                let (width, height) = self.physical_size;
                let physical_damage: Vec<_> = damage
                    .iter()
                    .map(|rect| {
                        zgx::scene::Rect::new(
                            rect.x * self.scale_factor,
                            rect.y * self.scale_factor,
                            rect.width * self.scale_factor,
                            rect.height * self.scale_factor,
                        )
                    })
                    .collect();
                let pixels = if self.physical_size == (960, 720) && self.scale_factor == 1.0 {
                    &raster.pixels
                } else {
                    // The explicit reference backend keeps its logical raster and
                    // scales into the native physical surface on HiDPI monitors.
                    self.software_pixels
                        .resize(width as usize * height as usize, 0x10141c);
                    for y in 0..height as usize {
                        for x in 0..width as usize {
                            let lx = (x as f32 / self.scale_factor) as usize;
                            let ly = (y as f32 / self.scale_factor) as usize;
                            self.software_pixels[y * width as usize + x] = if lx < 960 && ly < 720 {
                                raster.pixels[ly * 960 + lx]
                            } else {
                                0x10141c
                            };
                        }
                    }
                    &self.software_pixels
                };
                let damage = self.presentation.copy(
                    pixels,
                    &mut buffer,
                    (width as usize, height as usize),
                    age,
                    &physical_damage,
                );
                buffer.present_with_damage(&damage).expect("present");
            }
        }
        self.frames += 1;
    }
    fn resize_surface(&mut self, width: u32, height: u32) {
        self.physical_size = (width, height);
        if let Some(gpu) = &mut self.gpu {
            gpu.resize(width, height);
        }
        if let Some(surface) = &mut self.surface {
            surface
                .resize(
                    NonZeroU32::new(width).unwrap(),
                    NonZeroU32::new(height).unwrap(),
                )
                .expect("resize");
            self.presentation = Presentation::default();
        }
        self.scene.borrow_mut().resize(
            width as f32 / self.scale_factor,
            height as f32 / self.scale_factor,
        );
    }
    fn report(&self) {
        if self.window.is_some()
            && let Some(path) = std::env::var_os("ZGX_SCREENSHOT")
        {
            if let Some(gpu) = &self.gpu {
                use std::io::Write;
                let pixels = gpu.readback().expect("GPU screenshot readback");
                let mut file =
                    std::io::BufWriter::new(std::fs::File::create(path).expect("screenshot file"));
                write!(
                    file,
                    "P6\n{} {}\n255\n",
                    self.physical_size.0, self.physical_size.1
                )
                .expect("screenshot header");
                for rgba in pixels.as_chunks::<4>().0.iter() {
                    file.write_all(&rgba[..3]).expect("screenshot pixels");
                }
                file.flush().expect("screenshot flush");
            } else if let Some(raster) = &self.raster {
                raster
                    .write_ppm(std::path::Path::new(&path))
                    .expect("software screenshot");
            }
        }
        let caches = self.gpu.as_ref().map(GpuRenderer::debug_cache_stats);
        println!(
            "{{\"framework\":\"zgx\",\"renderer\":\"{}\",\"gpu_atlas_bytes\":{},\"gpu_shaped_bytes\":{},\"gpu_vertex_bytes\":{},\"ticks\":{},\"frames\":{},\"layout_nodes\":{},\"paint_nodes\":{},\"composite_nodes\":{},\"mounted_rows\":{},\"elapsed_seconds\":{:.3}}}",
            if self.gpu.is_some() {
                "gpu"
            } else if self.window.is_some() {
                "software"
            } else {
                "headless-software"
            },
            caches.map_or(0, |stats| stats.atlas_bytes),
            caches.map_or(0, |stats| stats.shaped_bytes),
            caches.map_or(0, |stats| stats.vertex_buffer_bytes),
            self.workload.frames,
            self.frames,
            self.layout_nodes,
            self.paint_nodes,
            self.composite_nodes,
            self.rows.len(),
            self.start.elapsed().as_secs_f64()
        );
    }
}
impl ApplicationHandler for Demo {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("zgx performance lab")
                        .with_inner_size(LogicalSize::new(960, 720))
                        .with_resizable(false),
                )
                .expect("create window"),
        );
        let physical = window.inner_size();
        self.physical_size = (physical.width.max(1), physical.height.max(1));
        self.scale_factor = window.scale_factor() as f32;
        if std::env::var("ZGX_RENDERER").is_ok_and(|renderer| renderer == "software") {
            let context = Context::new(window.clone()).expect("display context");
            let mut surface = Surface::new(&context, window.clone()).expect("surface");
            surface
                .resize(
                    NonZeroU32::new(self.physical_size.0).unwrap(),
                    NonZeroU32::new(self.physical_size.1).unwrap(),
                )
                .expect("resize");
            self.surface = Some(surface);
            self.raster = Some(Raster::new(960, 720));
        } else {
            let mut gpu = GpuRenderer::for_window(window.clone())
                .expect("GPU renderer (select ZGX_RENDERER=software for the reference renderer)");
            gpu.set_scale_factor(self.scale_factor);
            gpu.set_background(Color(16, 20, 28, 255));
            let fonts = gpu.text_system();
            self.scene
                .borrow_mut()
                .set_text_measurer(move |text: &str, size, width| {
                    zgx_gpu::measure_text(&mut fonts.borrow_mut(), text, size, width)
                });
            eprintln!("zgx GPU adapter: {:?}", gpu.adapter_info());
            self.gpu = Some(gpu);
        }
        self.window = Some(window.clone());
        self.start = Instant::now();
        self.next = self.start + PERIOD;
        window.request_redraw();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(true),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.resize_surface(size.width, size.height);
                    self.draw(true);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor as f32;
                if let Some(gpu) = &mut self.gpu {
                    gpu.set_scale_factor(self.scale_factor);
                }
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    self.resize_surface(size.width.max(1), size.height.max(1));
                }
                self.draw(true);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f64>(self.window.as_ref().unwrap().scale_factor());
                self.cursor = (p.x, p.y);
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                if self.cursor.1 >= 55. && self.cursor.1 < 87. {
                    for (i, x) in [20., 80., 140.].iter().enumerate() {
                        if self.cursor.0 >= *x && self.cursor.0 < *x + 50. {
                            self.action(i as u8);
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * ROW_HEIGHT,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / self.scale_factor,
                };
                self.workload.scroll_offset =
                    (self.workload.scroll_offset - dy).clamp(0., 100_000. * ROW_HEIGHT - 400.);
                self.sync_rows();
                self.draw(false);
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.tasks.has_ready() {
            self.tasks.tick();
            self.draw(false);
        }
        let now = Instant::now();
        if self.workload.seconds > 0.
            && now.duration_since(self.start).as_secs_f64() >= self.workload.seconds
        {
            self.report();
            event_loop.exit();
            return;
        }
        if self.workload.mode == Mode::Idle {
            if self.workload.seconds > 0. {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    self.start + std::time::Duration::from_secs_f64(self.workload.seconds),
                ));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        } else {
            if now >= self.next {
                self.tick();
                self.draw(false);
                self.next += PERIOD;
                if self.next <= now {
                    self.next = now + PERIOD;
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next));
        }
        // A yield schedules one more event-loop turn; sleeping futures never poll.
        if self.tasks.has_ready() {
            event_loop.set_control_flow(ControlFlow::Poll);
        }
    }
}
fn main() {
    let mut app = Demo::new();
    if std::env::args().any(|arg| arg == "--headless") {
        app.draw(true);
        let ticks = std::env::var("ZGX_TICKS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(600);
        for _ in 0..ticks {
            if app.workload.mode != Mode::Idle {
                app.tick();
            }
            app.draw(false);
        }
        if let Some(path) = std::env::var_os("ZGX_SCREENSHOT") {
            app.raster
                .as_ref()
                .unwrap()
                .write_ppm(std::path::Path::new(&path))
                .unwrap();
        }
        app.report();
    } else {
        EventLoop::new()
            .expect("event loop")
            .run_app(&mut app)
            .expect("run");
    }
}
