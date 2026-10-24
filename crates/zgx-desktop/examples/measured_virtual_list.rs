//! Natural component row measurement with estimated offscreen heights.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
    semantics::Role,
};
use zgx_desktop::{Application, WindowOptions};

struct Mounted(Rc<Cell<usize>>);
impl Drop for Mounted {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx measured virtual list".into(),
            width: 640.,
            height: 420.,
            ..Default::default()
        })
        .run(move |cx| {
            let heights = VariableHeights::new(&cx.ui.runtime, 100_000, 80.);
            let extra = cx.ui.signal(0_usize);
            let offset = cx.ui.signal(0.);
            let live = Rc::new(Cell::new(0));
            let built = Rc::new(Cell::new(0));
            let reports = Rc::new(Cell::new(0));
            let stream_chunks = Rc::new(Cell::new(0));
            let rows = measured_virtual_list(offset.clone(), heights.clone(), 2, |i| i, {
                let live = live.clone();
                let built = built.clone();
                let extra = extra.clone();
                move |_, key, cx| {
                    live.set(live.get() + 1);
                    built.set(built.get() + 1);
                    cx.retain(Mounted(live.clone()));
                    let extra = extra.clone();
                    column().id(format!("natural-{key}")).w_full().p(6.).gap(4.)
                        .bg(rgb(if key % 2 == 0 { 0x203040 } else { 0x304050 }))
                        .child(text(format!("Row {key}")).h(20.))
                        .child(keyed(move || (0..(key % 3 + 1 + if key == 1000 { extra.get() } else { 0 })).collect::<Vec<_>>(),
                            move |line, _| text(format!("Line {line} — measured children")).h(18.)).gap(4.))
                }
            })
            .id("list")
            .w_full()
            .grow()
            .flex_shrink(1.)
            .p(8.)
            .keyboard_navigation(true)
            .scrollbar(true)
            .bg(rgb(0x15202b));
            let view = cx.render(
                column().w_full().h_full().p(20.).gap(12.)
                    .text_size(14.).text_color(rgb(0xe5edf7))
                    .child(text("100k measured rows | G jump S stream R report").h(32.))
                    .child(rows),
            );
            let list = view.find("list").unwrap();
            cx.ui.input.focus(&cx.ui.scene, Some(list));
            let scene = cx.ui.scene.clone();
            let semantics = cx.ui.semantics.clone();
            let viewport = cx.viewport.clone();
            let tasks = cx.tasks.clone();
            let window = cx.window.clone();
            let (h, o, l, b, r, chunks) = (heights.clone(), offset.clone(), live.clone(), built.clone(), reports.clone(), stream_chunks.clone());
            cx.ui.on_event(view.node(), false, move |event| {
                if event.phase != EventPhase::Capture {
                    return;
                }
                let InputEvent::KeyDown { key, repeat: false, .. } = &event.event else { return; };
                if *key == Key::Escape {
                    window.close();
                    return;
                }
                let Key::Character(key) = key else { return; };
                match key.as_str() {
                    "g" => {
                        o.set(h.row_offset(1000) + 8.);
                    }
                    "s" => {
                        let extra = extra.clone();
                        let chunks = chunks.clone();
                        tasks.spawn(async move {
                            for _ in 0..4 {
                                zgx::timer::sleep(Duration::from_millis(100)).await;
                                extra.update(|value| *value += 1);
                                chunks.set(chunks.get() + 1);
                                println!("STREAM index=1000 chunks={}", chunks.get());
                            }
                        });
                    }
                    "r" => {
                        r.set(r.get() + 1);
                        let semantics = semantics.borrow();
                        let focused = semantics.get(event.target).and_then(|n| n.position_in_set)
                            .map_or_else(|| "null".into(), |n| (n - 1).to_string());
                        let scene = scene.borrow();
                        let bounds = scene.bounds(list);
                        let mut rows: Vec<_> = semantics.iter()
                            .filter(|(_, s)| s.role == Role::ListItem)
                            .filter_map(|(node, s)| s.position_in_set.map(|i| (i - 1, scene.bounds(node))))
                            .collect();
                        rows.sort_by_key(|(i, _)| *i);
                        // Report painted child bounds independently of the
                        // semantic row allocation when diagnosing native pixels.
                        let natural_rows = rows.iter().map(|(i, _)| {
                            let b = scene.bounds(view.find(&format!("natural-{i}")).unwrap());
                            format!("[{i}, {}, {}, {}, {}]", b.x, b.y, b.width, b.height)
                        }).collect::<Vec<_>>().join(",");
                        let rows = rows.iter().map(|(i, b)| format!("[{}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {:.3}]", i, b.x, b.y, b.width, b.height, h.row_height(*i).unwrap(), h.row_offset(*i), scene.bounds(view.find(&format!("natural-{i}")).unwrap()).height)).collect::<Vec<_>>().join(",");
                        println!("MEASURED {{\"report\":{},\"offset\":{:.3},\"extent\":{:.3},\"focused\":{},\"live\":{},\"built\":{},\"chunks\":{},\"viewport\":[{:.3},{:.3}],\"list\":[{:.3},{:.3},{:.3},{:.3}],\"rows\":[{}],\"natural_rows\":[{}]}}",
                            r.get(), o.get(), h.content_height(), focused, l.get(), b.get(), chunks.get(), viewport.get().0, viewport.get().1, bounds.x, bounds.y, bounds.width, bounds.height, rows, natural_rows);
                    }
                    _ => return,
                }
                event.prevent_default();
            });
            cx.on_closed(move || println!("MEASURED_CLOSED live={} built={} reports={} chunks={}", live.get(), built.get(), reports.get(), stream_chunks.get()));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(35)).await;
                    window.close();
                });
            }
        })
}
