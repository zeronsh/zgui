//! Declarative editors with component state, inherited typography and fluent styles.
//! Run `cargo run -p zgx-desktop --example form`; `--smoke-test` closes after 12 s.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, reactive::Signal, scene::Color, text_layout::FontFamily};
use zgx_desktop::{Application, WindowOptions};

#[derive(Clone)]
struct FormState {
    name: Signal<String>,
    notes: Signal<String>,
    locked: Signal<bool>,
    wide: Signal<bool>,
}

fn action(label: &str) -> View {
    button()
        .h(38.0)
        .px(14.0)
        .rounded(6.0)
        .bg(Color(47, 64, 88, 255))
        .hover(|s| s.bg(Color(65, 85, 116, 255)))
        .focus(|s| {
            s.border(2.0)
                .border_over(true)
                .border_color(Color(122, 184, 255, 255))
        })
        .child(text(label))
}

fn form(report: Rc<RefCell<Option<FormState>>>) -> View {
    component(move |cx| {
        let state = FormState {
            name: cx.state("Ada".to_owned()),
            notes: cx.state("Write a note here.".to_owned()),
            locked: cx.state(false),
            wide: cx.state(false),
        };
        *report.borrow_mut() = Some(state.clone());
        let name = text_input("Name", state.name.clone())
            .id("name")
            .reactive_style({
                let wide = state.wide.clone();
                move || Styles::new().w(if wide.get() { 580.0 } else { 420.0 })
            })
            .h(44.0)
            .p(10.0)
            .rounded(6.0)
            .bg(Color(28, 35, 48, 255))
            .focus(|s| {
                s.border(2.0)
                    .border_over(true)
                    .border_color(Color(122, 184, 255, 255))
            })
            .disabled_style(|s| {
                s.bg(Color(38, 39, 43, 255))
                    .text_color(Color(138, 142, 153, 255))
            })
            .disabled_when({
                let locked = state.locked.clone();
                move || locked.get()
            });
        let notes = text_area("Notes", state.notes.clone())
            .id("notes")
            .reactive_style({
                let wide = state.wide.clone();
                move || Styles::new().w(if wide.get() { 580.0 } else { 420.0 })
            })
            .h(144.0)
            .p(10.0)
            .rounded(6.0)
            .bg(Color(28, 35, 48, 255))
            .focus(|s| {
                s.border(2.0)
                    .border_over(true)
                    .border_color(Color(122, 184, 255, 255))
            })
            .disabled_style(|s| {
                s.bg(Color(38, 39, 43, 255))
                    .text_color(Color(138, 142, 153, 255))
            })
            .disabled_when({
                let locked = state.locked.clone();
                move || locked.get()
            });
        let lock = action("Toggle editing").on_click({
            let locked = state.locked.clone();
            move || {
                locked.update(|value| *value = !*value);
            }
        });
        let resize = action("Resize editors").on_click({
            let wide = state.wide.clone();
            move || {
                wide.update(|value| *value = !*value);
            }
        });
        column()
            .p(24.0)
            .gap(12.0)
            .font_family(FontFamily::Monospace)
            .text_size(16.0)
            .text_color(Color(231, 236, 246, 255))
            .child(text("A component-owned form").h(26.0).font_bold())
            .child(
                column()
                    .id("editors")
                    .gap(8.0)
                    .reactive_style({
                        let wide = state.wide.clone();
                        move || Styles::new().w(if wide.get() { 580.0 } else { 420.0 })
                    })
                    .child(text("Name").h(22.0))
                    .child(name)
                    .child(text("Notes").h(22.0))
                    .child(notes),
            )
            .child(row().gap(12.0).child(lock).child(resize))
            .child(text_signal(move || {
                format!(
                    "Status: {} | {} name characters | {} note characters",
                    if state.locked.get() {
                        "locked"
                    } else {
                        "editing"
                    },
                    state.name.get().chars().count(),
                    state.notes.get().chars().count()
                )
            }))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx declarative form".into(),
            width: 700.0,
            height: 470.0,
            ..Default::default()
        })
        .run(move |cx| {
            let report = Rc::new(RefCell::new(None));
            let handle = cx.render(form(report.clone()));
            {
                let scene = cx.ui.scene.clone();
                let viewport = cx.viewport.clone();
                cx.on_closed(move || {
                    if let Some(state) = report.borrow().as_ref() {
                        println!(
                            "FORM name={:?} notes={:?} locked={} wide={}",
                            state.name.get(),
                            state.notes.get(),
                            state.locked.get(),
                            state.wide.get()
                        );
                    }
                    let scene = scene.borrow();
                    for id in ["name", "notes", "editors"] {
                        if let Some(node) = handle.find(id) {
                            println!("BOUNDS {id} {:?}", scene.bounds(node));
                        }
                    }
                    println!("VIEWPORT {:?}", viewport.get());
                });
            }
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(12)).await;
                    window.close();
                });
            }
        })
}
