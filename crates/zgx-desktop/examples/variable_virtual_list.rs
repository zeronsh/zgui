//! Explicit reactive heights, streaming updates, and anchored virtualization.
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

fn first_row(heights: &VariableHeights, offset: f32) -> usize {
    let (mut lo, mut hi) = (0, heights.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if heights.row_offset(mid + 1) <= offset {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo.min(heights.len().saturating_sub(1))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx variable virtual list".into(),
            width: 640.,
            height: 420.,
            ..Default::default()
        })
        .run(move |cx| {
            let heights = VariableHeights::new(&cx.ui.runtime, 100_000, 24.);
            cx.ui.runtime.batch(|| {
                for i in 0..heights.len() {
                    heights.set_height(i, [24., 40., 64., 88.][i % 4]).unwrap();
                }
            });
            let offset = cx.ui.signal(0.);
            let live = Rc::new(Cell::new(0));
            let built = Rc::new(Cell::new(0));
            let reports = Rc::new(Cell::new(0));
            let stream_chunks = Rc::new(Cell::new(0));
            let rows = variable_virtual_list(offset.clone(), heights.clone(), 2, |i| i, {
                let live = live.clone();
                let built = built.clone();
                let heights = heights.clone();
                move |_, key, cx| {
                    live.set(live.get() + 1);
                    built.set(built.get() + 1);
                    cx.retain(Mounted(live.clone()));
                    let heights = heights.clone();
                    div()
                        .w_full()
                        .h_full()
                        .bg(rgb(if key % 2 == 0 { 0x203040 } else { 0x304050 }))
                        .child(text_signal(move || {
                            format!("Row {key} | {} px", heights.row_height(key).unwrap())
                        }).px(12.).py(2.))
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
                    .child(text("100k rows | G jump V grow A above S stream").h(32.))
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
                    "v" => {
                        let index = first_row(&h, o.get());
                        h.set_height(index, h.row_height(index).unwrap() + 40.).unwrap();
                        println!("CHANGE index={index} delta=40");
                    }
                    "a" => {
                        h.set_height(10, h.row_height(10).unwrap() + 32.).unwrap();
                        println!("CHANGE index=10 delta=32");
                    }
                    "s" => {
                        let index = (first_row(&h, o.get()) + 1).min(h.len() - 1);
                        let heights = h.clone();
                        let chunks = chunks.clone();
                        tasks.spawn(async move {
                            for _ in 0..4 {
                                zgx::timer::sleep(Duration::from_millis(60)).await;
                                heights.set_height(index, heights.row_height(index).unwrap() + 12.).unwrap();
                                chunks.set(chunks.get() + 1);
                                println!("STREAM index={index} chunks={}", chunks.get());
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
                        let rows = rows.iter().map(|(i, b)| format!("[{}, {:.3}, {:.3}, {:.3}, {:.3}]", i, b.x, b.y, b.width, b.height)).collect::<Vec<_>>().join(",");
                        println!("VARIABLE {{\"report\":{},\"offset\":{:.3},\"extent\":{:.3},\"focused\":{},\"live\":{},\"built\":{},\"chunks\":{},\"viewport\":[{:.3},{:.3}],\"list\":[{:.3},{:.3},{:.3},{:.3}],\"rows\":[{}]}}",
                            r.get(), o.get(), h.content_height(), focused, l.get(), b.get(), chunks.get(), viewport.get().0, viewport.get().1, bounds.x, bounds.y, bounds.width, bounds.height, rows);
                    }
                    _ => return,
                }
                event.prevent_default();
            });
            cx.on_closed(move || println!("VARIABLE_CLOSED live={} built={} reports={} chunks={}", live.get(), built.get(), reports.get(), stream_chunks.get()));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(35)).await;
                    window.close();
                });
            }
        })
}
