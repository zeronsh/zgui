//! Fields to try the caret by hand: type, move with the arrows, click to
//! place it, select, compose with an input method, scroll, zoom (⌘=, ⌘-,
//! ⌘0), and open a popover over a field. On macOS the caret is a Core
//! Animation layer whenever it can be (`Ui::native_caret`); every case here
//! should look and blink exactly as the editor's own caret
//! (`ZGX_NATIVE_CARET=0`) does, and an idle focused field keeps the app's
//! memory low (Activity Monitor's Memory column, after about 10 s).
//! Run: cargo run --release -p zgx-desktop --example caret_demo
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx caret".into(),
            width: 640.,
            height: 600.,
            ..Default::default()
        })
        .run(move |cx| {
            let ui = &cx.ui;
            let name = ui.signal(String::from("Type, move with the arrows, click inside"));
            let mixed = ui.signal(String::from("abc אבג def — mixed directions"));
            let hebrew = ui.signal(String::from("שלום עולם, right to left"));
            let notes = ui.signal(String::from(
                "A multiline field.\nUp and down keep the caret's column.\nSelect some text: the caret hides.",
            ));
            let rows: Vec<_> = (0..12)
                .map(|i| ui.signal(format!("Row {i}: scroll so this field leaves the view")))
                .collect();
            let scrolled = ui.signal(0_f32);
            let open = ui.signal(false);
            let shown = open.clone();
            let label = |words: &str| text(words).text_size(12.).text_color(rgb(0x8a98ad));
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(10.)
                    .bg(rgb(0x141a24))
                    .text_color(rgb(0xe5edf7))
                    .child(label("A plain field (focused at launch)"))
                    .child(
                        text_input("Name", name)
                            .w(560.)
                            .h(32.)
                            .on_mount(|field| {
                                field.focus();
                            }),
                    )
                    .child(label("Mixed directions, then right to left"))
                    .child(text_input("Mixed", mixed).w(560.).h(32.))
                    .child(text_input("Hebrew", hebrew).w(560.).h(32.))
                    .child(label("Multiline"))
                    .child(text_area("Notes", notes).w(560.).h(80.))
                    .child(label("Inside a scroll view: scroll a focused field partly and fully out"))
                    .child(
                        scroll(scrolled)
                            .w(560.)
                            .h(110.)
                            .gap(6.)
                            .children(rows.into_iter().map(|row| text_input("Row", row).w(520.).h(28.).shrink_0())),
                    )
                    .child(
                        button()
                            .h(32.)
                            .px(14.)
                            .rounded(6.)
                            .bg(rgb(0x2f4058))
                            .child(text("Cover the top fields (click again to uncover)"))
                            .on_click(move || {
                                open.update(|open| *open = !*open);
                            }),
                    )
                    .child(
                        div()
                            .absolute()
                            .ml(40.)
                            .mt(40.)
                            .size(300., 120.)
                            .rounded(10.)
                            .bg(rgba(0x2a3446ee))
                            .reactive_style(move || Styles::new().opacity(if shown.get() { 1. } else { 0. }))
                            .child(text("Over the fields: their caret is the editor's").m(16.)),
                    ),
            );
        })
}
