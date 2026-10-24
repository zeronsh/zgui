//! Independent native windows. Run with --smoke-test for an automated lifetime check.
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{timer::sleep, widgets::fixed};
use zgx_desktop::{Application, WindowContext, WindowOptions};
async fn wait_for(condition: impl Fn() -> bool, failure: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "{failure}");
        sleep(Duration::from_millis(10)).await;
    }
}
fn child(ctx: &mut WindowContext) {
    let root = ctx.ui.root();
    ctx.ui.label(
        root,
        "Each window owns its focus, rendering and tasks.",
        fixed(440., 32.),
    );
    let text = ctx.ui.signal("Try typing here".to_owned());
    ctx.ui.text_input(root, "Notes", text, 400., true);
    let close = ctx.window.clone();
    ctx.ui
        .button(root, "Close this window", 220., move || close.close());
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let completed = Rc::new(Cell::new(false));
    let proof = completed.clone();
    let closed = Rc::new(Cell::new(0));
    let closed_proof = closed.clone();
    let requests = Rc::new(Cell::new(0));
    let request_proof = requests.clone();
    Application::new()
        .window(WindowOptions {
            title: "zgx windows".into(),
            width: 500.,
            height: 320.,
            ..Default::default()
        })
        .run(move |ctx| {
            let root = ctx.ui.root();
            ctx.ui.label(
                root,
                "Closing this window keeps other windows open.",
                fixed(470., 36.),
            );
            let windows = ctx.windows.clone();
            ctx.ui.button(root, "Open another window", 240., move || {
                windows.open(
                    WindowOptions {
                        title: "zgx child".into(),
                        width: 500.,
                        height: 320.,
                        ..Default::default()
                    },
                    child,
                );
            });
            let close = ctx.window.clone();
            ctx.ui
                .button(root, "Close this window", 240., move || close.close());
            if smoke {
                let observed_requests = request_proof.clone();
                ctx.on_close_requested(move || {
                    request_proof.set(request_proof.get() + 1);
                    request_proof.get() >= 2
                });
                ctx.on_closed(move || closed_proof.set(closed_proof.get() + 1));
                // A window canceled before creation must not run its builder.
                ctx.windows
                    .open(WindowOptions::default(), |_| {
                        panic!("canceled window was created")
                    })
                    .close();
                let parent = ctx.window.clone();
                let close_parent = parent.clone();
                ctx.tasks.spawn(async move {
                    sleep(Duration::from_millis(150)).await;
                    close_parent.request_close();
                    wait_for(
                        || observed_requests.get() >= 1,
                        "first close request was not delivered",
                    )
                    .await;
                    assert!(
                        !close_parent.is_closed(),
                        "rejected close request must keep the window alive"
                    );
                    close_parent.request_close();
                });
                ctx.windows.open(
                    WindowOptions {
                        title: "zgx surviving child".into(),
                        width: 500.,
                        height: 320.,
                        ..Default::default()
                    },
                    move |ctx| {
                        child(ctx);
                        let close_child = ctx.window.clone();
                        let child_lifetime = close_child.clone();
                        let windows = ctx.windows.clone();
                        ctx.tasks.spawn(async move {
                            wait_for(
                                || parent.is_closed(),
                                "root window should have closed independently",
                            )
                            .await;
                            windows.open(
                                WindowOptions {
                                    title: "zgx grandchild".into(),
                                    width: 500.,
                                    height: 320.,
                                    ..Default::default()
                                },
                                move |ctx| {
                                    child(ctx);
                                    let close = ctx.window.clone();
                                    ctx.tasks.spawn(async move {
                                        wait_for(
                                            || child_lifetime.is_closed(),
                                            "child closure must not close its new window",
                                        )
                                        .await;
                                        proof.set(true);
                                        close.close();
                                    });
                                },
                            );
                            close_child.close();
                        });
                    },
                );
            }
        })?;
    if smoke {
        assert!(
            completed.get(),
            "application exited before the last window finished"
        );
        assert_eq!(requests.get(), 2, "close policy must receive both requests");
        assert_eq!(closed.get(), 1, "on_closed must run exactly once");
        println!("multi-window smoke passed (close policy and exactly-once cleanup)");
    }
    Ok(())
}
