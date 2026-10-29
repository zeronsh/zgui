//! `ui.async(f)`: a handler that awaits (a task per call, cancelled as its
//! owner goes, an uncaught error reported once), and `ui.optimistic`:
//! a write shown at once and taken back if its save fails, safe when calls
//! overlap (the newest write shows; a failure never reverts over a newer
//! one, whichever order they settle in).
mod common;
use common::Harness;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use zgx_script::{ScriptHost, Value};

/// Each `save` call's outcome, set by the test: `Some(true)` succeeds,
/// `Some(false)` fails, `None` keeps it waiting.
type Outcomes = Rc<RefCell<HashMap<i64, Option<bool>>>>;

fn host(code: &'static str, outcomes: &Outcomes, errors: &Rc<RefCell<Vec<String>>>) -> Harness {
    let (outcomes, errors) = (outcomes.clone(), errors.clone());
    Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", code);
        host.async_action("save", move |arg| {
            let outcomes = outcomes.clone();
            let id = match arg {
                Value::Num(n) => n as i64,
                _ => 0,
            };
            outcomes.borrow_mut().entry(id).or_insert(None);
            async move {
                loop {
                    if let Some(Some(ok)) = outcomes.borrow().get(&id).copied() {
                        return if ok {
                            Ok(Value::Num(id as f64))
                        } else {
                            Err(format!("save {id} refused"))
                        };
                    }
                    zgx::timer::sleep(Duration::from_millis(1)).await;
                }
            }
        });
        host.on_error(move |message| errors.borrow_mut().push(message.to_string()));
        host
    })
}
fn decide(h: &mut Harness, outcomes: &Outcomes, id: i64, ok: bool) {
    outcomes.borrow_mut().insert(id, Some(ok));
    std::thread::sleep(Duration::from_millis(5));
    h.frames(3);
    std::thread::sleep(Duration::from_millis(5));
    h.frames(3);
}

const ASYNC: &str = r##"
local saved = ui.state("saved", "")
local shown = ui.state("shown", true)
local function panel()
    return ui.component(function()
        return ui.button {
            id = "slow",
            w = 40,
            h = 20,
            "s",
            on_click = ui.async(function()
                local id = ui.call("save", 7):await()
                saved:set(`saved {id}`)
            end),
        }
    end)()
end
return ui.column {
    ui.button {
        id = "save",
        w = 40,
        h = 20,
        "s",
        on_click = ui.async(function()
            local id = ui.call("save", 1):await()
            saved:set(`saved {id}`)
        end),
    },
    ui.button {
        id = "fail",
        w = 40,
        h = 20,
        "f",
        on_click = ui.async(function()
            ui.call("save", 2):await()
        end),
    },
    ui.show(shown, panel()),
    ui.button { id = "close", w = 40, h = 20, "c", on_click = ui.toggle(shown) },
    ui.text { id = "saved", "{saved}" },
}
"##;

#[test]
fn an_async_handler_awaits() {
    let (outcomes, errors) = (Outcomes::default(), Rc::default());
    let mut h = host(ASYNC, &outcomes, &errors);
    assert_eq!(h.error(), None);
    h.click("save");
    decide(&mut h, &outcomes, 1, true);
    assert!(
        h.wait_until(|h| h.text("saved") == "saved 1"),
        "{}",
        h.text("saved")
    );
}

#[test]
fn an_uncaught_error_is_reported_once() {
    let (outcomes, errors) = (Outcomes::default(), Rc::default());
    let mut h = host(ASYNC, &outcomes, &errors);
    h.click("fail");
    decide(&mut h, &outcomes, 2, false);
    assert!(h.wait_until(|_| !errors.borrow().is_empty()));
    h.frames(5);
    let errors = errors.borrow();
    assert_eq!(
        errors
            .iter()
            .filter(|e| e.contains("save 2 refused"))
            .count(),
        1,
        "{errors:?}"
    );
}

#[test]
fn an_async_handler_is_cancelled_with_its_owner() {
    let (outcomes, errors) = (Outcomes::default(), Rc::default());
    let mut h = host(ASYNC, &outcomes, &errors);
    h.click("slow");
    h.frames(2);
    h.click("close");
    h.frames(2);
    decide(&mut h, &outcomes, 7, true);
    h.frames(5);
    assert_eq!(
        h.text("saved"),
        "",
        "its owner went: nothing after the await ran"
    );
    assert!(errors.borrow().is_empty(), "{:?}", errors.borrow());
}

const OPTIMISTIC: &str = r##"
local count = ui.state("count", 0)
local outcome = ui.state("outcome", "")
local function set(to)
    return ui.async(function()
        local err = ui.optimistic(count, to, function()
            return ui.call("save", to):await()
        end)
        outcome:set(if err then err else `kept {to}`)
    end)
end
return ui.column {
    ui.button { id = "one", w = 40, h = 20, "1", on_click = set(1) },
    ui.button { id = "two", w = 40, h = 20, "2", on_click = set(2) },
    ui.text { id = "count", "{count}" },
    ui.text { id = "outcome", "{outcome}" },
}
"##;

#[test]
fn an_optimistic_write_shows_at_once_and_goes_back_if_refused() {
    let (outcomes, errors) = (Outcomes::default(), Rc::default());
    let mut h = host(OPTIMISTIC, &outcomes, &errors);
    h.click("one");
    assert_eq!(h.text("count"), "1", "shown before the save answers");
    decide(&mut h, &outcomes, 1, true);
    assert!(h.wait_until(|h| h.text("outcome") == "kept 1"));
    assert_eq!(h.text("count"), "1");
    h.click("two");
    assert_eq!(h.text("count"), "2");
    decide(&mut h, &outcomes, 2, false);
    assert!(
        h.wait_until(|h| h.text("outcome") == "save 2 refused"),
        "{}",
        h.text("outcome")
    );
    assert_eq!(h.text("count"), "1", "back to what it replaced");
    assert!(
        errors.borrow().is_empty(),
        "returned, not reported: {:?}",
        errors.borrow()
    );
}

/// Two overlapping optimistic writes (1, then 2, from 0), settled in
/// `order` with `ok`: what the state ends at.
fn overlapping(order: [(i64, bool); 2]) -> String {
    let (outcomes, errors) = (Outcomes::default(), Rc::default());
    let mut h = host(OPTIMISTIC, &outcomes, &errors);
    h.click("one");
    h.click("two");
    assert_eq!(h.text("count"), "2", "the newest write shows");
    for (id, ok) in order {
        decide(&mut h, &outcomes, id, ok);
    }
    h.frames(3);
    h.text("count")
}

#[test]
fn overlapping_optimistic_writes_never_revert_over_a_newer_one() {
    // The older fails while the newer is pending, then the newer is kept.
    assert_eq!(overlapping([(1, false), (2, true)]), "2");
    // The newer fails first: back to the older's write, which is kept.
    assert_eq!(overlapping([(2, false), (1, true)]), "1");
    // Both fail, either order: back where it started.
    assert_eq!(overlapping([(1, false), (2, false)]), "0");
    assert_eq!(overlapping([(2, false), (1, false)]), "0");
    // The older fails after the newer was kept: the newer stays.
    assert_eq!(overlapping([(2, true), (1, false)]), "2");
}
