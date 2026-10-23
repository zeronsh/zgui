//! Keystroke to screen: each key pressed redraws a counter label (the same as
//! zgui-desktop's `key_workload`), for `scripts/windows_latency_probe.py`.
use gpui::{prelude::*, *};
use zgui_workload::Workload;
struct Keys {
    focus: FocusHandle,
    count: u64,
}
impl Render for Keys {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .bg(rgb(0x10141c))
            .text_color(rgb(0xe5edf7))
            .text_size(px(20.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, _: &KeyDownEvent, _, cx| {
                this.count += 1;
                cx.notify();
            }))
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(24.))
                    .child(format!("keys {}", self.count)),
            )
    }
}
fn main() {
    Application::new().run(|cx: &mut App| {
        let seconds = Workload::from_env().seconds;
        let bounds = Bounds::centered(None, size(px(480.), px(200.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| Keys {
                        focus: cx.focus_handle(),
                        count: 0,
                    });
                    window.focus(&view.read(cx).focus);
                    view
                },
            )
            .unwrap();
        cx.activate(true);
        if seconds > 0. {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs_f64(seconds))
                    .await;
                let _ = window.update(cx, |keys, _, _| eprintln!("keys={}", keys.count));
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
