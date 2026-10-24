//! After a panic is caught (the desktop host contains one per event-loop
//! turn), nothing is left borrowed or half-applied: the next event, write,
//! frame and layout work as if it had never happened.
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    widgets::Ui,
};

fn quiet<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    result
}

#[test]
fn a_panic_mid_batch_leaves_the_runtime_working() {
    let ui = Ui::new(200., 100.);
    let runtime = ui.runtime.clone();
    let value = runtime.signal(0);
    let seen = Rc::new(Cell::new(0));
    let _effect = runtime.effect({
        let (value, seen) = (value.clone(), seen.clone());
        move || seen.set(value.get())
    });
    assert!(
        quiet(|| runtime.batch(|| {
            value.set(1);
            panic!("mid batch");
        }))
        .is_err()
    );
    // The write stands; its effect runs at the next flush, and new writes
    // flush as ever (no batch left open).
    value.set(2);
    assert_eq!(seen.get(), 2);
    runtime.batch(|| value.set(3));
    assert_eq!(seen.get(), 3);
}

#[test]
fn a_panicking_effect_leaves_others_and_tracking_working() {
    let ui = Ui::new(200., 100.);
    let runtime = ui.runtime.clone();
    let trigger = runtime.signal(0);
    let fail = Rc::new(Cell::new(false));
    let _bad = runtime.effect({
        let (trigger, fail) = (trigger.clone(), fail.clone());
        move || {
            trigger.get();
            if fail.get() {
                panic!("an effect failed");
            }
        }
    });
    let other = runtime.signal(0);
    let seen = Rc::new(Cell::new(0));
    let _good = runtime.effect({
        let (other, seen) = (other.clone(), seen.clone());
        move || seen.set(other.get())
    });
    fail.set(true);
    assert!(quiet(|| trigger.set(1)).is_err());
    fail.set(false);
    // Reads outside an effect subscribe nothing, writes reach their own
    // effects, and the failed effect runs again when its input changes.
    let stray = runtime.signal(0);
    stray.get();
    other.set(5);
    assert_eq!(seen.get(), 5);
    trigger.set(2);
    let counted = runtime.effect_count();
    stray.set(1);
    assert_eq!(runtime.effect_count(), counted);
}

#[test]
fn a_panic_mid_layout_leaves_the_scene_laying_out() {
    let mut ui = Ui::new(300., 200.);
    let fail = Rc::new(Cell::new(false));
    {
        let fail = fail.clone();
        ui.scene
            .borrow_mut()
            .set_text_measurer(move |text: &str, size: f32, _| {
                if fail.get() {
                    panic!("the measurer failed");
                }
                (text.len() as f32 * size * 0.5, size)
            });
    }
    let label = ui.signal("one".to_owned());
    let shown = label.clone();
    let view = ui.mount(
        column()
            .id("column")
            .child(text_signal(move || shown.get()).id("label")),
    );
    ui.prepare_frame();
    fail.set(true);
    label.set("a longer label".into());
    assert!(quiet(|| ui.prepare_frame()).is_err());
    fail.set(false);
    // The scene isn't left borrowed, and the next frame lays out what the
    // failed one didn't.
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(view.find("label").unwrap());
    assert_eq!(bounds.width, "a longer label".len() as f32 * 16. * 0.5);
    ui.scene.borrow_mut().flush();
}

#[test]
fn a_panicking_handler_leaves_input_working() {
    let mut ui = Ui::new(300., 200.);
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    ui.mount(
        column()
            .child(
                button()
                    .id("bad")
                    .w(100.)
                    .h(40.)
                    .on_click(|| panic!("a handler failed"))
                    .child(text("Bad")),
            )
            .child(
                button()
                    .id("good")
                    .w(100.)
                    .h(40.)
                    .on_click(move || count.set(count.get() + 1))
                    .child(text("Good")),
            ),
    );
    ui.prepare_frame();
    let click = |ui: &mut Ui, y: f32| {
        let button = PointerButton::Primary;
        ui.dispatch(InputEvent::PointerDown { x: 10., y, button });
        ui.dispatch(InputEvent::PointerUp { x: 10., y, button });
    };
    assert!(quiet(|| click(&mut ui, 20.)).is_err());
    // The pointer isn't left captured by the failed press: the next press
    // reaches what it is on.
    click(&mut ui, 60.);
    assert_eq!(clicks.get(), 1);
    click(&mut ui, 60.);
    assert_eq!(clicks.get(), 2);
}

#[test]
fn an_effect_whose_first_run_panics_inside_a_batch_leaves_nothing_behind() {
    let ui = Ui::new(200., 100.);
    let runtime = ui.runtime.clone();
    let input = runtime.signal(0);
    let runs = Rc::new(Cell::new(0));
    let before = runtime.effect_count();
    runtime.batch(|| {
        // Made inside a batch, an effect runs at once (`effect_now`, as a
        // computed made while a component builds does).
        let failed = quiet(|| {
            let (input, runs) = (input.clone(), runs.clone());
            runtime.effect_now(move || {
                input.get();
                runs.set(runs.get() + 1);
                panic!("its first run failed");
            })
        });
        assert!(failed.is_err());
        // Nothing runs as it: reads here subscribe no effect.
        input.get();
    });
    assert_eq!(runtime.effect_count(), before, "no effect is left over");
    input.set(1);
    assert_eq!(runs.get(), 1, "the failed effect doesn't run again");
}
