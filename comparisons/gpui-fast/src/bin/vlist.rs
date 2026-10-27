//! The variable-height list (see zgx_workload::vlist): 100,000 chat rows of
//! wrapped text in GPUI's `list` with a `ListState` (not `uniform_list`),
//! which lays rows out to measure them as they come into view, moved by the
//! scene's scroll, fling or jump each tick.
use gpui::{prelude::*, *};
#[path = "../menu.rs"]
mod menu;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};
use zgx_workload::{PERIOD, vlist};

/// Each row's laid-out height (probe mode), from its wrapper's prepaint.
type Heights = Rc<RefCell<HashMap<usize, f32>>>;

struct Chat {
    list: ListState,
    /// Probe mode only: rows are wrapped to read their heights (no cost otherwise).
    heights: Option<Heights>,
}
impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(vlist::BACKGROUND))
            .font_family(vlist::FONT)
            .text_size(px(vlist::FONT_SIZE))
            .line_height(px(vlist::LINE_HEIGHT))
            .text_color(rgb(vlist::TEXT))
            .child(
                list(self.list.clone(), {
                    let heights = self.heights.clone();
                    move |index, _, _| {
                        let Some(heights) = heights.clone() else {
                            return row(index).into_any_element();
                        };
                        div()
                            .w_full()
                            .on_children_prepainted(move |bounds, _, _| {
                                if let Some(b) = bounds.first() {
                                    heights.borrow_mut().insert(index, f32::from(b.size.height));
                                }
                            })
                            .child(row(index))
                            .into_any_element()
                    }
                })
                .size_full(),
            )
    }
}

fn row(index: usize) -> Div {
    let mut body = div()
        .flex()
        .flex_col()
        .w_full()
        .px(px(vlist::PAD.0))
        .py(px(vlist::PAD.1))
        .gap(px(vlist::GAP))
        .bg(rgb(vlist::ROW_COLORS[index % 2]))
        .child(
            div()
                .flex()
                .gap(px(8.))
                .h(px(vlist::LINE_HEIGHT))
                .child(
                    div()
                        .text_color(rgb(vlist::AUTHOR))
                        .child(vlist::author(index)),
                )
                .child(
                    div()
                        .text_color(rgb(vlist::MUTED))
                        .child(vlist::time(index)),
                ),
        )
        .child(div().w(px(vlist::TEXT_WIDTH)).child(vlist::text(index)));
    if vlist::has_image(index) {
        body = body.child(
            div()
                .w(px(vlist::IMAGE.0))
                .h(px(vlist::IMAGE.1))
                .bg(rgb(vlist::IMAGE_COLOR)),
        );
    }
    body
}

fn main() {
    let mode = vlist::Mode::from_env();
    let seconds: f64 = std::env::var("ZGX_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30.);
    gpui_platform::application().run(move |cx: &mut App| {
        menu::menu_bar(cx, "zgx vlist");
        // The comparisons' one font file (comparisons/fonts), as every framework loads it.
        if let Some(bytes) = zgx_workload::font_file() {
            cx.text_system()
                .add_fonts(vec![std::borrow::Cow::Owned(bytes)])
                .unwrap();
        }
        let bounds = Bounds::centered(None, size(px(vlist::WIDTH), px(vlist::HEIGHT)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx: &mut Context<Chat>| {
                    let list = ListState::new(vlist::ROWS, ListAlignment::Top, px(200.));
                    let heights: Heights = Rc::default();
                    let state = list.clone();
                    let _ = &heights;
                    if mode == vlist::Mode::Probe {
                        // Each probe row at the top in turn, measured after a few frames.
                        let measured_heights = heights.clone();
                        cx.spawn(async move |chat, cx| {
                            let mut measured = Vec::new();
                            for row in vlist::probe_rows() {
                                state.scroll_to(ListOffset {
                                    item_ix: row,
                                    offset_in_item: px(0.),
                                });
                                let _ = chat.update(cx, |_, cx| cx.notify());
                                cx.background_executor().timer(PERIOD * 12).await;
                                measured.push((
                                    row,
                                    measured_heights.borrow().get(&row).copied().unwrap_or(0.),
                                ));
                            }
                            eprintln!("{}", vlist::probe_report(&measured, vlist::WIDTH));
                            let _ = cx.update(|cx| cx.quit());
                        })
                        .detach();
                        return Chat {
                            list,
                            heights: Some(heights),
                        };
                    }
                    cx.spawn(async move |chat, cx| {
                        let start = Instant::now();
                        let mut deadline = start + PERIOD;
                        let mut tick = 0_u64;
                        loop {
                            cx.background_executor()
                                .timer(deadline.saturating_duration_since(Instant::now()))
                                .await;
                            if seconds > 0. && start.elapsed().as_secs_f64() >= seconds {
                                eprintln!("workload_ticks={tick}");
                                let _ = cx.update(|cx| cx.quit());
                                break;
                            }
                            tick += 1;
                            let before = state.logical_scroll_top();
                            match vlist::jump(tick) {
                                Some(row) if mode == vlist::Mode::Jump => {
                                    state.scroll_to(ListOffset {
                                        item_ix: row,
                                        offset_in_item: px(0.),
                                    })
                                }
                                _ => {
                                    // At the end, back to the top (100k rows take minutes to reach).
                                    if state.logical_scroll_top().item_ix + 64 >= vlist::ROWS {
                                        state.scroll_to(ListOffset::default());
                                    } else {
                                        state.scroll_by(px(vlist::delta(mode, tick)));
                                    }
                                }
                            }
                            // A frame only when the list moved, as zgx and MyGo draw.
                            let after = state.logical_scroll_top();
                            let moved = (before.item_ix, before.offset_in_item)
                                != (after.item_ix, after.offset_in_item);
                            if moved && chat.update(cx, |_, cx| cx.notify()).is_err() {
                                break;
                            }
                            deadline += PERIOD;
                            if deadline <= Instant::now() {
                                deadline = Instant::now() + PERIOD;
                            }
                        }
                    })
                    .detach();
                    Chat {
                        list,
                        heights: None,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
