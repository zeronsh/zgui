//! The comparison UI built only with public retained components and fluent styles.
//! Run with ZGX_MODE=both ZGX_SECONDS=10; the final JSON is accepted by compare.py.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    collections::{List, ListError},
    compose::{prelude::*, virtual_list},
    reactive::Signal,
    scene::Color,
    task::yield_now,
};
use zgx_desktop::{Application, WindowOptions};
use zgx_workload::{
    HEIGHT, Mode, PERIOD, ROW_COUNT, ROW_HEIGHT, VIEWPORT_HEIGHT, WIDTH, Workload, row_label,
};

const FG: Color = Color(229, 237, 247, 255);
const SURFACE: Color = Color(25, 34, 48, 255);
struct Model {
    items: List<i32>,
    title: Signal<String>,
}
struct MountedRow(Rc<Cell<usize>>);
impl Drop for MountedRow {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
fn label(value: View, x: f32, y: f32, width: f32, height: f32) -> View {
    value.w(width).h(height).translate(x, y)
}
fn control(value: View, x: f32) -> View {
    button()
        .w(50.0)
        .h(32.0)
        .p(0.0)
        .gap(0.0)
        .items_start()
        .rounded(0.0)
        .bg(Color(41, 59, 80, 255))
        .translate(x, 55.0)
        .child(label(value, 12.0, 6.0, 38.0, 20.0))
}
fn count() -> View {
    component(|cx| {
        let model = cx.service::<Model>();
        text_signal(move || model.items.len().to_string())
    })
}
fn app(stream: Signal<String>, offset: Signal<f32>, mounted: Rc<Cell<usize>>) -> View {
    component(move |cx| {
        let model = cx.service::<Model>();
        let done = cx.state(0);
        let tasks = cx.tasks();
        let runtime = cx.runtime();
        let plus = control(text("+"), 20.0).on_click({
            let model = model.clone();
            let done = done.clone();
            let tasks = tasks.clone();
            let runtime = runtime.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                let runtime = runtime.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    runtime.batch(|| {
                        let result = (|| {
                            for value in [1, 2, 3] {
                                model.items.append(value)?;
                            }
                            Ok::<_, ListError>(())
                        })();
                        done.set(match result {
                            Ok(()) => 0,
                            Err(ListError::Exhausted) => -2,
                            Err(_) => -1,
                        });
                    });
                });
            }
        });
        let equals = control(text("="), 80.0).on_click({
            let model = model.clone();
            let done = done.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                let runtime = runtime.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    runtime.batch(|| {
                        let result = (|| {
                            let item = model.items.at(0)?;
                            item.write(99)?;
                            item.write(99)?;
                            Ok::<_, ListError>(())
                        })();
                        done.set(match result {
                            Ok(()) => 0,
                            Err(ListError::RowRemoved) => -2,
                            Err(_) => -1,
                        });
                    });
                });
            }
        });
        let done_button = control(
            text_signal({
                let done = done.clone();
                move || done.get().to_string()
            }),
            140.0,
        )
        .on_click(move || {
            done.set(999);
        });
        let body = cx.slot(|cx| {
            let model = cx.service::<Model>();
            text_signal(move || model.items.len().to_string())
        });
        let rows = virtual_list(
            offset,
            ROW_HEIGHT,
            2,
            || ROW_COUNT,
            |index| index,
            move |_, index, cx| {
                mounted.set(mounted.get() + 1);
                cx.retain(MountedRow(mounted.clone()));
                overlay()
                    .w(920.0)
                    .h(ROW_HEIGHT)
                    .bg(if index % 2 == 0 {
                        SURFACE
                    } else {
                        Color(29, 40, 56, 255)
                    })
                    .child(label(text(row_label(index)), 8.0, 4.0, 904.0, 20.0))
            },
        )
        .id("rows")
        .w(920.0)
        .h(VIEWPORT_HEIGHT)
        .translate(20.0, 290.0);
        overlay()
            .w(WIDTH)
            .h(HEIGHT)
            .bg(Color(16, 20, 28, 255))
            .text_color(FG)
            .text_size(14.0)
            .font_family("DejaVu Sans")
            .child(label(
                text("zgx performance lab").text_size(20.0),
                20.0,
                20.0,
                920.0,
                28.0,
            ))
            .child(plus)
            .child(equals)
            .child(done_button)
            .child(label(count(), 220.0, 62.0, 50.0, 24.0))
            .child(label(
                text_signal(move || model.title.get()),
                280.0,
                62.0,
                80.0,
                24.0,
            ))
            .child(overlay().w(50.0).h(24.0).translate(370.0, 62.0).child(body))
            .child(
                overlay()
                    .w(920.0)
                    .h(160.0)
                    .translate(20.0, 110.0)
                    .bg(SURFACE)
                    .overflow_hidden()
                    .child(label(
                        text_signal(move || stream.get()),
                        8.0,
                        8.0,
                        904.0,
                        144.0,
                    )),
            )
            .child(rows)
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new().window(WindowOptions {
        title: "zgx performance lab".into(), width: WIDTH as f64, height: HEIGHT as f64,
        resizable: false, ..Default::default()
    }).run(|cx| {
        let work = Rc::new(RefCell::new(Workload::from_env()));
        let stream = cx.ui.signal(work.borrow().visible_text());
        let offset = cx.ui.signal(work.borrow().scroll_offset);
        let mounted = Rc::new(Cell::new(0));
        cx.render(provide_with(|cx| Model {
            items: List::new(&cx.runtime(), 4), title: cx.state("stable".to_owned()),
        }, app(stream.clone(), offset.clone(), mounted.clone())));
        let started = Rc::new(Cell::new(Instant::now()));
        cx.on_closed({
            let work = work.clone(); let started = started.clone();
            move || println!(
                "{{\"framework\":\"zgx\",\"adapter\":\"components\",\"renderer\":\"gpu\",\"ticks\":{},\"mounted_rows\":{},\"elapsed_seconds\":{:.3}}}",
                work.borrow().frames, mounted.get(), started.get().elapsed().as_secs_f64())
        });
        let runtime = cx.ui.runtime.clone();
        let window = cx.window.clone();
        cx.tasks.spawn(async move {
            let start = Instant::now(); started.set(start);
            let mode = work.borrow().mode;
            let seconds = work.borrow().seconds;
            let end = if seconds > 0.0 { Some(start + Duration::from_secs_f64(seconds)) } else { None };
            if mode == Mode::Idle {
                if let Some(end) = end { zgx::timer::sleep_until(end).await; window.close(); }
                return;
            }
            let mut next = start + PERIOD;
            loop {
                zgx::timer::sleep_until(end.map_or(next, |end| end.min(next))).await;
                let now = Instant::now();
                if end.is_some_and(|end| now >= end) { window.close(); break; }
                runtime.batch(|| {
                    let mut work = work.borrow_mut();
                    work.scroll_offset = offset.get();
                    work.tick();
                    if matches!(mode, Mode::Stream | Mode::Both) { stream.set(work.visible_text()); }
                    if matches!(mode, Mode::Scroll | Mode::Both) { offset.set(work.scroll_offset); }
                });
                next += PERIOD;
                let finished = Instant::now();
                if next <= finished { next = finished + PERIOD; }
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgx::{compose::TaskRunner, scene::Rect, task::LocalExecutor, widgets::Ui};

    #[test]
    fn public_components_match_geometry_and_bound_virtual_rows() {
        let mut ui = Ui::new(WIDTH, HEIGHT);
        let executor = Rc::new(RefCell::new(LocalExecutor::new()));
        let stream = ui.signal("matched stream".to_owned());
        let offset = ui.signal(0.0_f32);
        let mounted = Rc::new(Cell::new(0));
        let handle = ui.mount(provide(
            TaskRunner::from_executor(executor),
            provide_with(
                |cx| Model {
                    items: List::new(&cx.runtime(), 4),
                    title: cx.state("stable".to_owned()),
                },
                app(stream, offset.clone(), mounted.clone()),
            ),
        ));
        ui.prepare_frame();
        let rows = handle.find("rows").unwrap();
        assert_eq!(
            ui.scene.borrow().bounds(rows),
            Rect::new(20.0, 290.0, 920.0, 400.0)
        );
        assert_eq!(mounted.get(), 17);
        for index in 1..100 {
            offset.set(index as f32 * ROW_HEIGHT);
            ui.prepare_frame();
            let work = Workload {
                text: String::new(),
                scroll_offset: offset.get(),
                frames: 0,
                mode: Mode::Idle,
                seconds: 1.,
            };
            assert_eq!(mounted.get(), work.row_range().len());
        }
        handle.unmount();
        assert_eq!(mounted.get(), 0);
    }
}
