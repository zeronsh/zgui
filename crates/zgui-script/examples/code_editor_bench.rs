//! What the code editor example (examples/code_editor) costs on a file of
//! 100,000 lines (`LINES` sets it), with text shaped and drawn offscreen as
//! a window draws it: opening it (the file read, to its first frame drawn),
//! a keystroke with the lines highlighted, a scroll, soft wrap, and the
//! memory a line takes. Instructions retired are macOS's (0 elsewhere).
//! Run: cargo run --release -p zgui-script --example code_editor_bench
use std::{cell::RefCell, rc::Rc, time::Instant};
use zgui::{
    compose::{TaskRunner, provide},
    frame::FrameClock,
    input::InputEvent,
    task::LocalExecutor,
    widgets::Ui,
};
use zgui_gpu::GpuRenderer;
use zgui_script::{ScriptHost, testing::FakeDesktop};

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/code_editor/app.luau");

/// This process's instructions retired, and its footprint in bytes.
fn usage() -> (u64, u64) {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Usage {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut Usage) -> i32;
        }
        let mut usage = Usage {
            uuid: [0; 16],
            fields: [0; 40],
        };
        // SAFETY: RUSAGE_INFO_V4 into a buffer as large as it; this process.
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        // `ri_instructions`, `ri_phys_footprint`.
        (usage.fields[29], usage.fields[7])
    }
    #[cfg(not(target_os = "macos"))]
    (0, 0)
}

/// A Rust file of `lines` lines, as the editor's sample makes them.
fn source(lines: usize) -> String {
    let body = [
        "/// A function, its docs and its body.",
        "pub fn compute_N(input: &[u8], scale: f32) -> Result<Vec<u32>, Error> {",
        "    let name = \"item N\"; // a comment",
        "    let total: u32 = input.iter().map(|b| *b as u32 * N).sum();",
        "    /* a block comment",
        "       running on */",
        "    if total > 0xff { return Err(Error::Overflow(total)); }",
        "    Ok(vec![total; scale as usize])",
        "}",
        "",
    ];
    (0..lines)
        .map(|i| body[i % body.len()].replace('N', &i.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The first node of the scene `found` holds for, from its root down.
fn find(
    scene: &zgui::scene::Scene,
    found: impl Fn(zgui::scene::NodeId) -> bool,
) -> Option<zgui::scene::NodeId> {
    let mut pending = vec![scene.root()];
    while let Some(id) = pending.pop() {
        if found(id) {
            return Some(id);
        }
        pending.extend(scene.children(id).iter().copied());
    }
    None
}

struct Bench {
    ui: Ui,
    gpu: GpuRenderer,
    executor: Rc<RefCell<LocalExecutor>>,
    frames: u64,
}
impl Bench {
    fn settle(&self) {
        while self.executor.borrow_mut().tick() > 0 {}
    }
    /// A frame laid out and drawn; its time (ms) and instructions (M).
    fn frame(&mut self) -> (f64, f64) {
        let (start, i) = (Instant::now(), usage().0);
        self.settle();
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap();
        // A window presents every frame; reading one back now and then only
        // keeps the GPU from falling behind.
        self.frames += 1;
        if self.frames.is_multiple_of(30) {
            self.gpu.readback().unwrap();
        }
        (
            start.elapsed().as_secs_f64() * 1e3,
            (usage().0 - i) as f64 / 1e6,
        )
    }
    /// `n` frames, each after `before`: the mean time and instructions.
    fn mean(&mut self, n: usize, mut before: impl FnMut(&mut Self, usize)) -> (f64, f64) {
        let (mut time, mut instructions) = (0., 0.);
        for k in 0..n {
            let (t, i) = (Instant::now(), usage().0);
            before(self, k);
            self.frame();
            time += t.elapsed().as_secs_f64() * 1e3;
            instructions += (usage().0 - i) as f64 / 1e6;
        }
        (time / n as f64, instructions / n as f64)
    }
}

fn main() {
    let lines: usize = std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let (w, h, scale) = (1100., 760., 2.);
    let path = std::env::temp_dir().join(format!("zgui-code-bench-{lines}.rs"));
    std::fs::write(&path, source(lines)).unwrap();
    let mut ui = Ui::new(w, h);
    let mut gpu = GpuRenderer::new((w * scale) as u32, (h * scale) as u32).expect("GPU");
    gpu.set_scale_factor(scale);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let host = ScriptHost::new(&ui.runtime, APP);
    host.reload().unwrap();
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let desktop = Rc::new(FakeDesktop::default());
    ui.mount(provide(
        zgui::platform::PlatformService(desktop.clone()),
        provide(
            TaskRunner::from_executor(executor.clone()),
            provide(FrameClock::new(), host.view()),
        ),
    ));
    let mut bench = Bench {
        ui,
        gpu,
        executor,
        frames: 0,
    };
    bench.frame();
    bench.frame();
    // The GPU's queued frames done (their buffers back), so the footprint
    // is the app's.
    bench.gpu.readback().unwrap();
    let footprint_before = usage().1;

    // Open: the dialog picks the file; to the first frame drawn with it.
    let before_open = Instant::now();
    let instructions_before = usage().0;
    desktop.pick([&path]);
    assert!(host.run_command("open"));
    let mut frames = 0;
    loop {
        bench.frame();
        frames += 1;
        let opened = {
            let scene = bench.ui.scene.borrow();
            find(&scene, |id| {
                matches!(scene.kind(id), zgui::scene::NodeKind::RichText { text }
                    if text.text().starts_with("pub fn compute_1("))
            })
            .is_some()
        };
        if opened || frames > 200 {
            break;
        }
    }
    let open = before_open.elapsed().as_secs_f64() * 1e3;
    let open_instructions = (usage().0 - instructions_before) as f64 / 1e6;
    bench.frame();
    bench.gpu.readback().unwrap();
    let per_line = (usage().1 as f64 - footprint_before as f64) / lines as f64;
    println!(
        "{lines} lines: open to first frame {open:.1} ms ({open_instructions:.0} M instructions, {frames} frames), {per_line:.0} bytes a line"
    );

    // A keystroke in the middle of the file, highlighted: to its frame.
    let editor = {
        let scene = bench.ui.scene.borrow();
        find(&scene, |id| bench.ui.editor(id).is_some()).expect("the editor")
    };
    bench.ui.input.focus(&bench.ui.scene, Some(editor));
    let handle = bench.ui.editor(editor).unwrap();
    let middle = handle.editor.borrow().text().len() / 2;
    handle.set_selection(middle, middle);
    bench.frame();
    bench.frame();
    let (time, instructions) = bench.mean(20, |b, k| {
        b.ui.dispatch(InputEvent::Text(if k % 2 == 0 { "x" } else { "y" }.into()));
    });
    println!("keystroke to frame: {time:.2} ms, {instructions:.1} M instructions");

    // `PROFILE=scroll` or `type`: that alone, for a profiler, for a while.
    match std::env::var("PROFILE").as_deref() {
        Ok("scroll") => {
            bench.mean(3000, |b, k| {
                b.ui.dispatch(InputEvent::Scroll {
                    x: 500.,
                    y: 400.,
                    delta_x: 0.,
                    delta_y: if (k / 200) % 2 == 0 { 19. } else { -19. },
                });
            });
            return;
        }
        Ok("type") => {
            bench.mean(1000, |b, k| {
                b.ui.dispatch(InputEvent::Text(if k % 2 == 0 { "x" } else { "y" }.into()));
            });
            return;
        }
        _ => {}
    }
    // Scrolling a screen at a time, then a line at a time (a wheel).
    for (label, step) in [("a page", 600.), ("a line", 19.)] {
        let (time, instructions) = bench.mean(30, |b, _| {
            b.ui.dispatch(InputEvent::Scroll {
                x: 500.,
                y: 400.,
                delta_x: 0.,
                delta_y: step,
            });
        });
        println!("scroll {label}: {time:.2} ms a frame, {instructions:.1} M instructions");
    }

    // Soft wrap on (its checkbox's state), then a scroll.
    assert!(host.run_command("wrap"));
    bench.frame();
    let (time, instructions) = bench.mean(30, |b, _| {
        b.ui.dispatch(InputEvent::Scroll {
            x: 500.,
            y: 400.,
            delta_x: 0.,
            delta_y: 600.,
        });
    });
    println!("scroll a page, wrapped: {time:.2} ms a frame, {instructions:.1} M instructions");
    let _ = std::fs::remove_file(&path);
}
