//! What text takes from a script beyond its size and colour: OpenType
//! features, and spans that follow state.
mod common;
use common::Harness;

fn features(h: &mut Harness, id: &str) -> Vec<(String, u32)> {
    let node = h.find(id).unwrap();
    let scene = h.ui.scene.borrow();
    // An `id` names a text's box; its text is the node inside.
    let font = scene.font(scene.children(node)[0]);
    (font.features.settings().iter())
        .map(|(tag, value)| (String::from_utf8_lossy(tag).into_owned(), *value))
        .collect()
}

#[test]
fn font_features_set_inherit_and_follow_state() {
    let mut h = Harness::inline(
        r##"local alternate = ui.state("alternate", false)
        return ui.column {
            ui.text { id = "plain", "plain" },
            ui.text { id = "set", font_features = "ss01 cv02=3 liga=0", "set" },
            ui.column { font_features = "smcp", ui.text { id = "inside", "inside" } },
            ui.text { id = "odd", font_features = "ss01 toolong x=1 cv02=no", "odd" },
            ui.text { id = "followed", "followed", font_features = function()
                return if alternate:get() then "ss02" else "ss03"
            end },
        }"##,
    );
    h.frames(1);
    assert_eq!(features(&mut h, "plain"), []);
    let pair = |tag: &str, value| (tag.to_string(), value);
    assert_eq!(
        features(&mut h, "set"),
        [pair("cv02", 3), pair("liga", 0), pair("ss01", 1)]
    );
    assert_eq!(features(&mut h, "inside"), [pair("smcp", 1)]);
    assert_eq!(
        features(&mut h, "odd"),
        [pair("ss01", 1)],
        "the rest left out"
    );
    assert_eq!(features(&mut h, "followed"), [pair("ss03", 1)]);
    h.host
        .set_state("alternate", zgx_script::Value::Bool(true))
        .unwrap();
    h.frames(1);
    assert_eq!(features(&mut h, "followed"), [pair("ss02", 1)]);
    assert_eq!(h.error(), None);
}

/// The runs a rich text shows, as its text, weight, family and colour.
fn runs(h: &mut Harness, id: &str) -> Vec<(String, u16, String, zgx::scene::Color)> {
    use zgx::scene::NodeKind;
    let node = h.find(id).unwrap();
    let scene = h.ui.scene.borrow();
    let mut pending = vec![node];
    while let Some(next) = pending.pop() {
        if let NodeKind::RichText { text } = scene.kind(next) {
            return (text.runs().iter())
                .map(|run| {
                    (
                        text.text()[run.range.clone()].to_string(),
                        run.font.weight,
                        format!("{:?}", run.font.family),
                        run.color,
                    )
                })
                .collect();
        }
        pending.extend(scene.children(next).iter().copied());
    }
    panic!("no rich text under {id}");
}

#[test]
fn a_spans_style_and_font_follow_state() {
    use zgx::scene::Color;
    let mut h = Harness::inline(
        r##"local on = ui.state("on", false)
        return ui.rich {
            id = "para",
            "Plain ",
            ui.span { "named", font = "Menlo" },
            ui.span { " marked", bold = on, color = function()
                return if on:get() then "#ff0000" else "#00ff00"
            end },
            ui.span { " still", color = "#0000ff" },
            ui.span { " faced", font = function()
                return if on:get() then "serif" else "mono"
            end },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.error(), None);
    let (green, red) = (Color(0, 255, 0, 255), Color(255, 0, 0, 255));
    let run = |h: &mut Harness, text: &str| {
        let found = runs(h, "para").into_iter().find(|run| run.0 == text);
        found.unwrap_or_else(|| panic!("no run {text:?}"))
    };
    assert!(run(&mut h, "named").2.contains("Menlo"));
    assert_eq!(run(&mut h, " marked").3, green);
    assert_eq!(run(&mut h, " marked").1, 400);
    assert!(run(&mut h, " faced").2.contains("Monospace"));
    let node = h.find("para").unwrap();
    h.host
        .set_state("on", zgx_script::Value::Bool(true))
        .unwrap();
    h.frames(1);
    assert_eq!(run(&mut h, " marked").3, red, "follows");
    assert_eq!(run(&mut h, " marked").1, 700);
    assert!(run(&mut h, " faced").2.contains("Serif"));
    assert_eq!(run(&mut h, " still").3, Color(0, 0, 255, 255));
    assert_eq!(h.find("para"), Some(node), "the same node");
    assert_eq!(h.error(), None);
}

#[test]
fn a_span_keeps_every_option_it_is_given() {
    use zgx::scene::{Color, NodeKind};
    let mut h = Harness::inline(
        r##"return ui.rich {
            id = "para",
            ui.span { "a", weight = 300, bold = true, text_size = 19, italic = true },
            ui.span { "b", bold = true, color = "#112233", bg = "#445566" },
            ui.span {
                "c",
                strike = true,
                padding = { 4, 2 },
                radius = 3,
                underline_style = "wavy",
                underline_color = "#778899",
                underline_thickness = 2,
                underline_offset = 1,
            },
            ui.span { "d", href = "https://zgx.zeron.sh", hover = { underline = true } },
            ui.span { "e", font = "mono", underline = true },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.error(), None);
    let node = h.find("para").unwrap();
    let scene = h.ui.scene.borrow();
    let mut pending = vec![node];
    let mut found = None;
    while let Some(next) = pending.pop() {
        if let NodeKind::RichText { text } = scene.kind(next) {
            found = Some(text.clone());
        }
        pending.extend(scene.children(next).iter().copied());
    }
    let text = found.expect("a rich text");
    let run = |at: &str| {
        let start = text.text().find(at).unwrap();
        (text.runs().iter())
            .find(|run| run.range.contains(&start))
            .expect("a run")
    };
    assert_eq!((run("a").font.weight, run("a").font_size), (300, 19.));
    assert!(run("a").font.italic);
    assert_eq!(run("b").font.weight, 700, "bold without a weight");
    assert_eq!(run("b").color, Color(0x11, 0x22, 0x33, 255));
    assert_eq!(run("b").background, Some(Color(0x44, 0x55, 0x66, 255)));
    assert!(run("c").strikethrough.is_some());
    assert_eq!(run("c").padding, [4., 2.]);
    assert_eq!(run("c").radius, 3.);
    assert!(run("c").underline.is_some(), "underlined by its style");
    assert!(run("e").underline.is_some());
    assert!(format!("{:?}", run("e").font.family).contains("Monospace"));
}

/// A span's table is read as an element's is, a plain table: what its
/// metatable's `__index` would give is not an option.
#[test]
fn a_span_does_not_read_options_behind_a_metatable() {
    let mut h = Harness::inline(
        r##"local options = setmetatable({ "held" }, { __index = { bold = true } })
        return ui.rich { id = "para", ui.span(options), ui.span { " own", bold = true } }"##,
    );
    h.frames(1);
    assert_eq!(h.error(), None);
    let weights: Vec<u16> = runs(&mut h, "para").iter().map(|run| run.1).collect();
    assert_eq!(weights, [400, 700], "only an option the table holds");
}

#[test]
fn a_restyled_span_keeps_a_selection_across_it() {
    use zgx::input::{InputEvent, PointerButton};
    let mut h = Harness::inline(
        r##"local on = ui.state("on", false)
        return ui.rich {
            id = "para",
            selectable = true,
            w = 300,
            "Some words ",
            ui.span { "to select", color = function()
                return if on:get() then "#ff0000" else "#00ff00"
            end },
        }"##,
    );
    h.frames(2);
    let (x, y) = {
        let node = h.find("para").unwrap();
        let bounds = h.ui.scene.borrow().bounds(node);
        (bounds.x, bounds.y + bounds.height / 2.)
    };
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown {
        x: x + 1.,
        y,
        button,
    });
    h.ui.dispatch(InputEvent::PointerMove { x: x + 280., y });
    h.ui.dispatch(InputEvent::PointerUp {
        x: x + 280.,
        y,
        button,
    });
    h.frames(1);
    let selected = h.ui.selected_text();
    assert!(selected.contains("words"), "{selected:?}");
    h.host
        .set_state("on", zgx_script::Value::Bool(true))
        .unwrap();
    h.frames(2);
    assert_eq!(h.ui.selected_text(), selected, "kept across the restyle");
    assert_eq!(h.error(), None);
}
