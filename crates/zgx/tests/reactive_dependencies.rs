use std::{cell::Cell, rc::Rc};
use zgx::reactive::Runtime;

#[test]
fn effects_follow_the_signals_their_latest_run_read() {
    let runtime = Runtime::new();
    let (flag, a, b) = (runtime.signal(true), runtime.signal(0), runtime.signal(0));
    let runs = Rc::new(Cell::new(0));
    let effect = {
        let (flag, a, b, runs) = (flag.clone(), a.clone(), b.clone(), runs.clone());
        runtime.effect(move || {
            runs.set(runs.get() + 1);
            if flag.get() {
                a.get();
            } else {
                b.get();
            }
        })
    };
    assert_eq!(runs.get(), 1);
    a.set(1);
    assert_eq!(runs.get(), 2, "reads a");
    b.set(1);
    assert_eq!(runs.get(), 2, "does not read b yet");
    flag.set(false);
    assert_eq!(runs.get(), 3);
    a.set(2);
    assert_eq!(runs.get(), 3, "stopped reading a: unsubscribed");
    b.set(2);
    assert_eq!(runs.get(), 4, "now reads b");
    // Steady re-runs keep exactly the same subscriptions.
    for value in 3..10 {
        b.set(value);
    }
    assert_eq!(runs.get(), 11);
    drop(effect);
    b.set(100);
    flag.set(true);
    assert_eq!(
        runs.get(),
        11,
        "dropping the effect removes every subscription"
    );
}
