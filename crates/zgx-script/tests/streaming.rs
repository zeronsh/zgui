//! The work a streamed delta costs: a model published whole on every delta
//! (a transcript as a reply streams in) must cost what changed, not what the
//! model holds. Budgets in tables converted for scripts and rows built.
mod common;

use common::Harness;
use zgx_script::markdown::{Incremental, Options};
use zgx_script::{ScriptHost, Value, tables_converted};

const SCRIPT: &str = r#"local t = ui.shared("t", nil)
local rows = ui.computed(function()
    local v = t:get()
    return if v then v.rows else {}
end)
local builds = ui.state("builds", 0)
return ui.column {
    ui.each(rows, function(row, _, live)
        builds:set(builds:peek() + 1)
        local blocks = ui.computed(function() return live:get().blocks end)
        return ui.column {
            ui.each(blocks, function(b)
                return ui.text { b.t, literal = true }
            end),
        }
    end, { key = function(r) return r.id end }),
    ui.text { id = "builds", "{builds}" },
}"#;

fn row(id: usize, text: &str) -> Value {
    let blocks = zgx_script::markdown::parse(text, &Options::default());
    Value::Map(
        [
            ("blocks".into(), Value::List(blocks.into())),
            ("id".into(), Value::Num(id as f64)),
            ("text".into(), Value::from(text)),
        ]
        .into(),
    )
}

/// Streams a reply after `held` rows; the tables converted per delta, and
/// the rows built while it streamed.
fn per_delta(held: usize) -> (u64, String) {
    let mut h = Harness::new(|runtime| ScriptHost::inline(runtime, "app", SCRIPT));
    let shared = h.host.shared("t", Value::Nil);
    // Each held row: a few paragraphs (sixteen and more values deep, so
    // each is converted and kept on its own).
    let text = "A paragraph of the thread.\n\nAnother, **bold**.\n\n- a list\n- of two\n";
    let publish = |reply: Option<Value>| {
        // Built afresh each time, as a host rebuilding its model does.
        let mut rows: Vec<Value> = (0..held).map(|i| row(i, text)).collect();
        rows.extend(reply);
        shared.set(Value::Map(
            [("rows".into(), Value::List(rows.into()))].into(),
        ))
    };
    publish(None);
    h.settle();
    let mut reply = Incremental::new(Options::default());
    let mut streamed = String::new();
    let reply_row = |reply: &Incremental, text: &str| {
        Value::Map(
            [
                ("blocks".into(), Value::List(reply.blocks().into())),
                ("id".into(), Value::Num(held as f64)),
                ("text".into(), Value::from(text)),
            ]
            .into(),
        )
    };
    // The reply starts, then grows a word at a time.
    streamed.push_str("Once");
    reply.set_text(&streamed);
    publish(Some(reply_row(&reply, &streamed)));
    h.settle();
    let builds = h.text("builds");
    let before = tables_converted();
    let deltas = 40;
    for i in 0..deltas {
        streamed.push_str(if i % 10 == 9 {
            " end.\n\nNext"
        } else {
            " word"
        });
        reply.set_text(&streamed);
        publish(Some(reply_row(&reply, &streamed)));
        h.settle();
    }
    let after = h.text("builds");
    assert_eq!(builds, after, "no row is built again as the reply streams");
    ((tables_converted() - before) / deltas as u64, after)
}

#[test]
fn a_delta_costs_what_changed_not_what_is_held() {
    let (small, _) = per_delta(50);
    let (large, _) = per_delta(500);
    assert_eq!(
        small, large,
        "a delta converts as much with 500 rows held as with 50"
    );
    // The new parts: the model and its row list, the reply's row, its
    // blocks and the growing block's runs, as each reader first reads them.
    assert!(small <= 24, "a delta converts {small} tables");
}

#[test]
fn rows_arriving_later_from_the_host_render() {
    // A new thread: its shared state starts empty, the view mounts, then
    // the host publishes its first rows (a pending message, then the
    // agent's), each as a whole new value.
    let mut h = Harness::new(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r#"local t = ui.shared("t", nil)
            local rows = ui.computed(function()
                local v = t:get()
                return if v then v.rows else {}
            end)
            return ui.column {
                ui.each(rows, function(row, _, live)
                    return ui.presence(function()
                        return live:get()
                    end, function(r)
                        return ui.text { id = "row-" .. r.id, r.text, literal = true }
                    end)
                end, { key = function(r) return r.id end }),
            }"#,
        )
    });
    let shared = h.host.shared("t", Value::Nil);
    h.settle();
    let row = |id: &str, text: &str| {
        Value::Map(
            [
                ("id".into(), Value::from(id)),
                ("text".into(), Value::from(text)),
            ]
            .into(),
        )
    };
    let doc = |rows: Vec<Value>| Value::Map([("rows".into(), Value::List(rows.into()))].into());
    shared.set(doc(vec![row("pending", "hi")]));
    h.settle();
    assert_eq!(h.text("row-pending"), "hi", "the first row renders");
    shared.set(doc(vec![row("pending", "hi"), row("a1", "Hello")]));
    h.settle();
    assert_eq!(h.text("row-a1"), "Hello", "a row added later renders");
    shared.set(doc(vec![row("u1", "hi"), row("a1", "Hello there")]));
    h.settle();
    assert_eq!(h.text("row-u1"), "hi", "a row whose id changed renders");
    assert_eq!(h.text("row-a1"), "Hello there", "a row that grew follows");
}
