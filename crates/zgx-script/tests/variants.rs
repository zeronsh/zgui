//! `ui.variants` (a component's styles by variant, as shadcn's `cva`) and
//! the design lint (`ui.theme.strict`, `ui.raw`).
mod common;

use common::Harness;
use zgx::{
    input::InputEvent,
    scene::{Color, NodeKind},
};

impl Harness {
    fn fill(&mut self, id: &str) -> Color {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        match self.ui.scene.borrow().kind(node) {
            NodeKind::Panel { quad, .. } => quad.fill,
            NodeKind::Container(_) => Color(0, 0, 0, 0),
            other => panic!("`{id}` is not a box: {other:?}"),
        }
    }
    fn hover(&mut self, id: &str) {
        let b = self.bounds(id);
        self.ui.dispatch(InputEvent::PointerMove {
            x: b.x + b.width / 2.,
            y: b.y + b.height / 2.,
        });
    }
    fn warned(&self, part: &str) -> Vec<String> {
        self.host
            .warnings()
            .into_iter()
            .filter(|w| w.contains(part))
            .collect()
    }
}
/// What loading `code` fails with ("" if it loads).
fn load_error(code: &str) -> String {
    let runtime = zgx::reactive::Runtime::new();
    let host = zgx_script::ScriptHost::inline(&runtime, "app", code);
    host.reload()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}
const RED: Color = Color(255, 0, 0, 255);
const GREEN: Color = Color(0, 255, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);
const DARK_RAISED: Color = Color(35, 46, 64, 255);

const BUTTON: &str = r##"
local button = ui.variants {
    base = { h = 36, px = 12, bg = "#ff0000" },
    variants = {
        tone = {
            solid = { bg = "#00ff00" },
            ghost = { bg = "#0000", hover = { bg = "$surface.raised" } },
        },
        size = { sm = { h = 28 }, md = {}, lg = { h = 44, px = 18 } },
        wide = { [true] = { w = 120 }, [false] = { w = 60 } },
    },
    compound = { { tone = "ghost", size = { "sm", "lg" }, bg = "#0000ff" } },
    defaults = { tone = "solid", size = "md", wide = false },
}
"##;

#[test]
fn named_variants_resolve_into_the_elements_styles() {
    let mut h = Harness::inline(&format!(
        r##"{BUTTON}
        return ui.column {{
            ui.div {{ id = "default", variants = button }},
            ui.div {{ id = "sm", variants = button {{ size = "sm", wide = true }} }},
            ui.div {{ id = "compound", variants = button {{ tone = "ghost", size = "lg" }} }},
            ui.div {{ id = "own", variants = button {{ size = "lg" }}, h = 50, bg = "#ff0000" }},
            ui.div {{ id = "padded", variants = button, p = 0, ui.div {{ id = "inner", w = 4, h = 4 }} }},
        }}"##
    ));
    assert!(h.error().is_none(), "{:?}", h.error());
    assert_eq!(h.fill("default"), GREEN, "the defaults");
    let b = h.bounds("default");
    assert_eq!((b.width, b.height), (60., 36.));
    let b = h.bounds("sm");
    assert_eq!((b.width, b.height), (120., 28.), "an on-off axis");
    assert_eq!(h.fill("compound"), BLUE, "a compound rule over the axes");
    assert_eq!(h.bounds("own").height, 50., "the element's own win");
    assert_eq!(h.fill("own"), RED);
    // `p = 0` takes the variant's `px` too, as tailwind-merge would.
    let (outer, inner) = (h.bounds("padded"), h.bounds("inner"));
    assert_eq!(inner.x, outer.x);
}

#[test]
fn variants_chosen_by_state_restyle_in_rust() {
    let mut h = Harness::inline(&format!(
        r##"{BUTTON}
        local tone = ui.state("tone", "solid")
        local size = ui.state("size", "md")
        return ui.column {{
            ui.div {{ id = "tab", variants = button {{ tone = tone, size = function() return size:get() end }} }},
            ui.button {{ id = "ghost", w = 10, h = 10, on_click = function() tone:set("ghost") end }},
            ui.button {{ id = "lg", w = 10, h = 10, on_click = function() size:set("lg") end }},
            ui.button {{ id = "bad", w = 10, h = 10, on_click = function() tone:set("loud") end }},
        }}"##
    ));
    assert!(h.error().is_none(), "{:?}", h.error());
    assert_eq!(h.fill("tab"), GREEN);
    h.ui.scene.borrow_mut().flush();
    h.click("ghost");
    let calls = h.calls();
    h.ui.scene.borrow_mut().flush();
    assert_eq!(h.fill("tab"), Color(0, 0, 0, 0));
    // Hovered, the ghost tone's hover style shows, its token resolved.
    h.hover("tab");
    assert_eq!(h.fill("tab"), DARK_RAISED);
    // The size (chosen by a function) changes: the compound rule applies,
    // and its height lays out, without the script styling anything.
    h.click("lg");
    h.hover("tab");
    assert_eq!(h.fill("tab"), DARK_RAISED, "the hover style still over it");
    h.ui.dispatch(InputEvent::PointerLeave);
    assert_eq!(h.fill("tab"), BLUE);
    assert_eq!(h.bounds("tab").height, 44.);
    // The click's handler, and the function choosing the size once.
    assert_eq!(h.calls() - calls, 2, "styles resolve in Rust");
    h.click("bad");
    assert_eq!(h.fill("tab"), GREEN, "an unknown option takes the default");
    assert_eq!(h.warned("has no option `loud`").len(), 1);
}

#[test]
fn definitions_say_what_is_wrong() {
    for (code, wanted) in [
        ("ui.variants { variant = {} }", "not `variant`"),
        (
            r##"ui.variants { variants = { tone = { solid = { bgg = "#fff" } } } }"##,
            "unknown style `bgg` in `variants.tone.solid`",
        ),
        (
            r##"local v = ui.variants { variants = { tone = { solid = {} } } }
            return ui.div { variants = v { tone = "loud" } }"##,
            "variant `tone` has no option `loud` (it has `solid`)",
        ),
        (
            r##"ui.variants { variants = { tone = { solid = {} } }, compound = { { bg = "#fff" } } }"##,
            "names no variant",
        ),
        (
            r##"local s = ui.state("x", "#fff")
            ui.variants { base = { bg = s } }"##,
            "must be a plain value",
        ),
        (
            r##"return ui.div { variants = "solid" }"##,
            "`variants` takes",
        ),
    ] {
        let error = load_error(code);
        assert!(error.contains(wanted), "{code}: {error}");
    }
    let h = Harness::inline(
        r##"ui.variants { variants = { tone = { a = { px = 4 } }, size = { b = { p = 8 } } } }
        return ui.div {}"##,
    );
    assert_eq!(
        h.warned("variants `size` and `tone` both set").len(),
        1,
        "{:?}",
        h.host.warnings()
    );
}

#[test]
fn the_design_lint_names_the_token_to_use() {
    let h = Harness::inline(
        r##"ui.theme.strict("warn")
        return ui.column {
            ui.div { id = "raised", bg = "#232e40", w = 10, h = 10 },
            ui.div { id = "spaced", p = 8, gap = 7, w = "50%", h = "auto", m = 0 },
            ui.div { id = "fine", bg = "$surface.raised", rounded = "$radius.md", p = "$space.2" },
            ui.div { id = "clear", bg = "#0000", w = 1, h = 1, opacity = 0.5, border = 1 },
            ui.div { id = "logo", bg = ui.raw("#e5484d", "the brand's logo red") },
            ui.div { hover = { bg = "#abcdef" } },
        }"##,
    );
    assert!(h.error().is_none(), "{:?}", h.error());
    let colour = h.warned("hard-codes a colour");
    assert!(
        colour
            .iter()
            .any(|w| w.contains(r##""$surface.raised" is exactly this"##)),
        "{colour:#?}"
    );
    assert!(
        colour.iter().any(|w| w.contains("#abcdef")),
        "hover styles too"
    );
    let spacing = h.warned("hard-codes a spacing");
    assert!(
        spacing
            .iter()
            .any(|w| w.contains("`p = 8`") && w.contains(r##""$space.2" is exactly"##)),
        "{spacing:#?}"
    );
    assert!(
        spacing
            .iter()
            .any(|w| w.contains("`gap = 7`") && w.contains("The nearest is")),
        "{spacing:#?}"
    );
    assert_eq!(
        spacing.len(),
        2,
        "0, percentages and auto are not spacing values"
    );
    assert!(
        h.warned("e5484d").is_empty(),
        "ui.raw lets its value through"
    );
    assert!(h.warned("ui.raw(").len() >= 3, "each says how to opt out");
}

#[test]
fn suggestions_prefer_a_token_whose_role_fits() {
    // A wash with a border's colour: for a background, the wash.
    let h = Harness::inline(
        r##"ui.theme.strict("warn")
        local line = ui.theme.get("border.subtle")
        ui.theme.tokens { app = { wash = line } }
        return ui.column {
            ui.div { bg = ui.theme.get("border.subtle") },
            ui.div { border = 1, border_color = ui.theme.get("border.subtle") },
        }"##,
    );
    let warned = h.warned("hard-codes a colour");
    assert!(
        warned
            .iter()
            .any(|w| w.contains("`bg =") && w.contains(r#""$app.wash" is exactly"#)),
        "{warned:#?}"
    );
    assert!(
        warned
            .iter()
            .any(|w| w.contains("`border_color =") && w.contains(r#""$border.subtle" is exactly"#)),
        "{warned:#?}"
    );
}

#[test]
fn a_function_returning_a_hard_coded_value_warns_at_its_definition() {
    let mut h = Harness::inline(
        r##"ui.theme.strict(true)
        local pick = ui.state("pick", false)
        return ui.column {
            ui.div { id = "computed", w = 10, h = 10, bg = function()
                return if pick:get() then "#123456" else "$accent"
            end },
            ui.button { id = "pick", w = 10, h = 10, on_click = function() pick:set(true) end },
        }"##,
    );
    assert!(h.warned("#123456").is_empty());
    h.click("pick");
    let warned = h.warned("#123456");
    assert_eq!(warned.len(), 1, "{:?}", h.host.warnings());
    assert!(
        warned[0].contains(":4: the function for `bg` returns"),
        "{}",
        warned[0]
    );
    assert_eq!(
        h.fill("computed"),
        Color(0x12, 0x34, 0x56, 255),
        "a warning only"
    );
}

#[test]
fn the_error_mode_fails_and_ui_raw_needs_a_reason() {
    let error = load_error(
        r##"ui.theme.strict("error")
        return ui.div { bg = "#ff0000" }"##,
    );
    assert!(
        error.contains("design lint") && error.contains("app:2"),
        "{error}"
    );
    let error = load_error(r##"return ui.div { bg = ui.raw("#ff0000") }"##);
    assert!(error.contains("needs a reason"), "{error}");
    // Variants are checked as they are defined.
    let error = load_error(
        r##"ui.theme.strict("error")
        ui.variants { variants = { tone = { solid = { bg = "#ff0000" } } } }
        return ui.div {}"##,
    );
    assert!(error.contains("design lint"), "{error}");
}

#[test]
fn computed_colours_apply_as_literal_ones_do() {
    // A function returning a colour, in each form a literal takes.
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.div { id = "hex", w = 10, h = 10, bg = function() return "#123456" end },
            ui.div { id = "number", w = 10, h = 10, bg = function() return 0x112233 end },
            ui.div { id = "token", w = 10, h = 10, bg = function() return "$accent" end },
            ui.div { id = "literal-token", w = 10, h = 10, bg = "$accent" },
            ui.div { id = "current", w = 10, h = 10, bg = function() return "currentColor" end },
            ui.div { id = "literal-current", w = 10, h = 10, bg = "currentColor" },
            ui.div { id = "not-utf8", w = 10, h = 10, bg = function() return "\xff#123456" end },
            ui.div { id = "plain", w = 10, h = 10 },
        }"##,
    );
    assert_eq!(h.fill("hex"), Color(0x12, 0x34, 0x56, 255));
    assert_eq!(h.fill("number"), Color(0x11, 0x22, 0x33, 255));
    assert_eq!(h.fill("token"), h.fill("literal-token"));
    assert_eq!(h.fill("current"), h.fill("literal-current"));
    // Text that isn't UTF-8 is no colour: the box keeps none.
    assert_eq!(h.fill("not-utf8"), h.fill("plain"));
}

/// A computed colour that fails reports, and leaves the others updating;
/// once it returns a colour again, its box shows it (research B4).
#[test]
fn a_computed_colour_that_fails_reports_and_the_rest_update() {
    let mut h = Harness::inline(
        r##"local tick = ui.shared("tick", 0)
        return ui.column {
            ui.div { id = "first", w = 10, h = 10,
                bg = function() tick:get(); return "#123456", "ignored" end },
            ui.div { id = "bad", w = 10, h = 10,
                bg = function() if tick:get() == 1 then error("computed failure marker") end; return "#112233" end },
            ui.div { id = "last", w = 10, h = 10,
                bg = function() return if tick:get() == 0 then "#222222" else "#abcdef" end },
            ui.div { id = "none", w = 10, h = 10, bg = function() return end },
            ui.div { id = "plain", w = 10, h = 10 },
        }"##,
    );
    assert_eq!(h.fill("first"), Color(0x12, 0x34, 0x56, 255));
    assert_eq!(h.fill("none"), h.fill("plain"));
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(1.));
    h.settle();
    assert!(h.error().unwrap().contains("computed failure marker"));
    assert_eq!(h.fill("last"), Color(0xab, 0xcd, 0xef, 255));
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(2.));
    h.settle();
    assert_eq!(h.fill("bad"), Color(0x11, 0x22, 0x33, 255));
}

/// A retained computed reads the current lint mode after host and script changes.
#[test]
fn a_retained_computed_observes_design_lint_changes() {
    let mut h = Harness::new(|runtime| {
        zgx_script::ScriptHost::inline(
            runtime,
            "app",
            r##"local tick = ui.shared("tick", 0)
            return ui.column {
                ui.div { id = "computed", w = 10, h = 10, bg = function()
                    local n = tick:get()
                    if n == 5 then ui.theme.strict("off") end
                    if n == 6 then ui.theme.strict("error") end
                    return if n % 2 == 0 then "#123456" else "#abcdef"
                end },
                ui.text { id = "last", function() return tostring(tick:get()) end },
                ui.button { id = "warn", on_click = function()
                    ui.theme.strict("warn"); tick:set(4)
                end, "Warn" },
            }"##,
        )
        .error_banner(false)
    });
    assert!(h.host.warnings().is_empty());
    assert_eq!(h.fill("computed"), Color(0x12, 0x34, 0x56, 255));
    h.host.design_lint(zgx_script::DesignLint::Warn);
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(1.));
    h.settle();
    assert_eq!(h.warned("#abcdef").len(), 1);
    assert_eq!(h.fill("computed"), Color(0xab, 0xcd, 0xef, 255));
    h.host.design_lint(zgx_script::DesignLint::Error);
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(2.));
    h.settle();
    let error = h.error().expect("the failing computed is reported");
    assert!(error.contains("the function for `bg` returns"), "{error}");
    assert_eq!(h.text("last"), "2");
    h.host.design_lint(zgx_script::DesignLint::Off);
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(3.));
    h.settle();
    assert_eq!(h.fill("computed"), Color(0xab, 0xcd, 0xef, 255));
    h.click("warn");
    assert_eq!(h.warned("#123456").len(), 1);
    assert_eq!(h.fill("computed"), Color(0x12, 0x34, 0x56, 255));
    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let reported = errors.clone();
    h.host
        .on_error(move |error| reported.borrow_mut().push(error.to_owned()));
    let before = errors.borrow().len();
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(5.));
    h.settle();
    assert_eq!(h.fill("computed"), Color(0xab, 0xcd, 0xef, 255));
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(6.));
    h.settle();
    assert_eq!(h.fill("computed"), Color(0x12, 0x34, 0x56, 255));
    assert_eq!(
        errors.borrow().len(),
        before,
        "the call entered with lint off"
    );
    h.host.shared("tick", 0.).set(zgx_script::Value::Num(7.));
    h.settle();
    assert_eq!(errors.borrow().len(), before + 1);
}
