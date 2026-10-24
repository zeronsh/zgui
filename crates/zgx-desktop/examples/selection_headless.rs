//! Display text selection offscreen, rendered as a window would: a document
//! of `PARAGRAPHS` wrapped paragraphs, selectable unless `SELECTABLE=0`, and
//! one change per tick. `MODE=drag` moves a held pointer down the document
//! (each move extends the selection), `idle` keeps a selection and changes
//! nothing, `stream` keeps one while a status line beside it streams. Run it
//! under `/usr/bin/time -l` with two `TICKS` values: the difference in
//! instructions retired is what the ticks cost.
//! Run: MODE=drag TICKS=600 cargo run --release -p zgx-desktop --example selection_headless
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    widgets::Ui,
};
use zgx_gpu::GpuRenderer;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const WIDTH: f32 = 800.;
const HEIGHT: f32 = 900.;

fn main() {
    let ticks: usize = env("TICKS", 600);
    let paragraphs: usize = env("PARAGRAPHS", 40);
    let selectable = env("SELECTABLE", 1) != 0;
    let mode: String = env("MODE", "drag".to_owned());
    let mut gpu = GpuRenderer::new(WIDTH as u32 * 2, HEIGHT as u32 * 2).expect("GPU");
    gpu.set_scale_factor(2.);
    let mut ui = Ui::new(WIDTH, HEIGHT);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let status = ui.signal(String::from("tick 0"));
    let read = status.clone();
    ui.mount(
        column()
            .size(WIDTH, HEIGHT)
            .p(16.)
            .gap(6.)
            .text_size(13.)
            .line_height(18.)
            .selectable(selectable)
            .child(text_signal(move || read.get()))
            .children((0..paragraphs).map(|index| {
                text(format!(
                    "Paragraph {index}: selectable display text wraps across lines, \
                     and a drag over it extends one selection through every paragraph \
                     it passes, in document order."
                ))
                .w_full()
                .text_wrap(true)
            })),
    );
    let render = |ui: &Ui, gpu: &mut GpuRenderer| {
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        damage
            .iter()
            .map(|r| (r.width * r.height) as u64)
            .sum::<u64>()
    };
    render(&ui, &mut gpu);
    let pointer = |ui: &mut Ui, event| {
        ui.dispatch_with_modifiers(event, Modifiers::default());
    };
    pointer(
        &mut ui,
        InputEvent::PointerDown {
            x: 30.,
            y: 40.,
            button: PointerButton::Primary,
        },
    );
    pointer(&mut ui, InputEvent::PointerMove { x: 400., y: 300. });
    if mode != "drag" {
        pointer(
            &mut ui,
            InputEvent::PointerUp {
                x: 400.,
                y: 300.,
                button: PointerButton::Primary,
            },
        );
    }
    render(&ui, &mut gpu);
    let mut damaged = 0;
    for tick in 0..ticks {
        match mode.as_str() {
            "drag" => {
                // Down the document and back, a few points a move.
                let step = (tick % 300) as f32;
                let y = 40. + (if step < 150. { step } else { 300. - step }) * 5.;
                let x = 40. + (tick % 37) as f32 * 19.;
                pointer(&mut ui, InputEvent::PointerMove { x, y });
            }
            "stream" => {
                status.set(format!("tick {tick}"));
            }
            _ => {}
        }
        damaged += render(&ui, &mut gpu);
    }
    let _ = gpu.readback();
    println!(
        "{{\"mode\":\"{mode}\",\"selectable\":{selectable},\"ticks\":{ticks},\"selected_bytes\":{},\"damaged_area\":{damaged}}}",
        ui.selected_text().len()
    );
}
