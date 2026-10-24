//! The proposed model UI expressed as retained components and fluent styles.
//! Run: cargo run -p zgx-desktop --example components
use zgx::{
    collections::{List, ListError},
    compose::{
        Context, Slot, View, button, column, component, provide_with, row, text, text_signal,
    },
    reactive::Signal,
    scene::Color,
    style::Styled,
    task::yield_now,
};
use zgx_desktop::{Application, WindowOptions};

struct Model {
    items: List<i32>,
    title: Signal<String>,
}
fn count() -> View {
    component(|cx| {
        let model = cx.service::<Model>();
        text_signal(move || model.items.len().to_string()).id("count")
    })
}
fn title() -> View {
    component(|cx| {
        let model = cx.service::<Model>();
        text_signal(move || model.title.get())
    })
}
fn dialog(body: Slot) -> View {
    column()
        .px(12.0)
        .rounded(6.0)
        .bg(Color(36, 42, 56, 255))
        .child(body)
}
fn control(label: View) -> View {
    button()
        .w(72.0)
        .h(40.0)
        .rounded(8.0)
        .bg(Color(42, 53, 76, 255))
        .text_color(Color(235, 241, 255, 255))
        .hover(|s| s.bg(Color(58, 76, 108, 255)))
        .active(|s| s.bg(Color(33, 45, 66, 255)))
        .focus(|s| {
            s.border(2.0)
                .border_over(true)
                .border_color(Color(112, 174, 255, 255))
        })
        .child(label)
}
fn app() -> View {
    component(|cx: &mut Context| {
        let model = cx.service::<Model>();
        let done = cx.state(0_i32);
        let runtime = cx.runtime();
        let tasks = cx.tasks();
        let plus = control(text("+")).id("plus").on_click({
            let model = model.clone();
            let done = done.clone();
            let tasks = tasks.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                let runtime = runtime.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    runtime.batch(|| {
                        let result = (|| {
                            model.items.append(1)?;
                            model.items.append(2)?;
                            model.items.append(3)?;
                            Ok::<_, ListError>(())
                        })();
                        done.set(match result {
                            Ok(()) => 0,
                            Err(ListError::Capacity { .. }) => -1,
                            Err(ListError::Exhausted) => -2,
                            Err(error) => panic!("unexpected append error: {error}"),
                        });
                    });
                });
            }
        });
        let equals = control(text("=")).id("equals").on_click({
            let model = model.clone();
            let done = done.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    let result = (|| {
                        let item = model.items.at(0)?;
                        item.write(99)?;
                        item.write(99)?;
                        Ok::<_, ListError>(())
                    })();
                    done.set(match result {
                        Ok(()) => 0,
                        Err(ListError::Bounds { .. }) => -1,
                        Err(ListError::RowRemoved) => -2,
                        Err(error) => panic!("unexpected row error: {error}"),
                    });
                });
            }
        });
        let done_button = control(text_signal({
            let done = done.clone();
            move || done.get().to_string()
        }))
        .id("done")
        .on_click(move || {
            done.set(999);
        });
        let body = cx.slot(|cx| {
            let model = cx.service::<Model>();
            text_signal(move || model.items.len().to_string())
        });
        column()
            .p(24.0)
            .gap(20.0)
            .text_size(18.0)
            .text_color(Color(232, 238, 249, 255))
            .child(row().gap(12.0).child(plus).child(equals).child(done_button))
            .child(
                row()
                    .gap(24.0)
                    .items_center()
                    .child(count())
                    .child(title())
                    .child(dialog(body)),
            )
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx — components and fluent styling".into(),
            width: 600.0,
            height: 220.0,
            ..Default::default()
        })
        .run(|cx| {
            cx.render(provide_with(
                |cx| Model {
                    items: List::new(&cx.runtime(), 4),
                    title: cx.state("stable".to_owned()),
                },
                app(),
            ));
            if zgx::env::var_os("ZGX_SNAPSHOT").is_some() {
                let semantics = cx.ui.semantics.clone();
                cx.on_closed(move || {
                    for (_, node) in semantics.borrow().iter() {
                        println!("SEMANTIC {:?} {:?}", node.label, node.value);
                    }
                });
            }
            if let Ok(seconds) = zgx::env::var("ZGX_SECONDS")
                .unwrap_or_default()
                .parse::<u64>()
            {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_secs(seconds)).await;
                    window.close();
                });
            }
        })
}
