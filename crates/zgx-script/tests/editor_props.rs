//! An editor's script options: highlight colours that follow the theme, a
//! highlighter's state of any Luau value, `read_only`, and the system's
//! substitutions as it is typed (`autocorrect`, `smart_punctuation`).
mod common;
use common::Harness;
use std::rc::Rc;
use zgx::scene::{Color, NodeKind};

/// Each styled run under `id`, as its text and colour.
fn runs(h: &mut Harness, id: &str) -> Vec<(String, Color)> {
    let root = h.find(id).unwrap();
    let scene = h.ui.scene.borrow();
    let mut pending = vec![root];
    let mut found = Vec::new();
    while let Some(node) = pending.pop() {
        if let NodeKind::RichText { text } = scene.kind(node) {
            for run in text.runs() {
                found.push((text.text()[run.range.clone()].to_string(), run.color));
            }
        }
        pending.extend(scene.children(node).iter().copied());
    }
    found
}

fn color_of(h: &mut Harness, id: &str, text: &str) -> Color {
    let runs = runs(h, id);
    (runs.iter().find(|(t, _)| t == text))
        .unwrap_or_else(|| panic!("no run {text:?} in {runs:?}"))
        .1
}

#[test]
fn a_highlight_token_colour_follows_a_theme_switch() {
    let mut h = Harness::inline(
        r##"return ui.textarea { id = "code", value = ui.state("text", "let x"), w = 300, h = 100,
            highlight = function(line, state)
                return { { 1, 3, "$accent" } }, state
            end }"##,
    );
    h.ui.theme().set(zgx::theme::DARK).unwrap();
    h.frames(2);
    let accent = |h: &Harness| {
        zgx::theme::Themes::of(&h.ui.runtime)
            .root()
            .color(zgx::theme::tok::ACCENT)
    };
    let dark = accent(&h);
    assert_eq!(color_of(&mut h, "code", "let"), dark);
    h.ui.theme().set(zgx::theme::LIGHT).unwrap();
    h.frames(2);
    let light = accent(&h);
    assert_ne!(dark, light, "the themes' accents differ");
    assert_eq!(color_of(&mut h, "code", "let"), light, "resolved again");
    assert_eq!(h.error(), None);
}

/// Lines inside `/* … */` grey, the highlighter carrying whether a line
/// starts inside one as `state`, in the form `carry` gives.
fn comments(carry: &str) -> String {
    format!(
        r##"local text = ui.state("text", "a /* b\nc\nd */ e\nf")
        {carry}
        return ui.textarea {{ id = "code", value = text, w = 300, h = 200,
            highlight = function(line, state)
                local inside = open(state)
                local spans = {{}}
                if inside then
                    local close = string.find(line, "*/", 1, true)
                    if close then
                        table.insert(spans, {{ 1, close + 1, "#808080" }})
                        return spans, carried(false)
                    end
                    table.insert(spans, {{ 1, #line, "#808080" }})
                    return spans, carried(true)
                end
                local start = string.find(line, "/*", 1, true)
                if start then
                    table.insert(spans, {{ start, #line, "#808080" }})
                    return spans, carried(true)
                end
                return spans, carried(false)
            end }}"##
    )
}

fn grey_lines(carry: &str) {
    let mut h = Harness::inline(&comments(carry));
    h.frames(2);
    assert_eq!(h.error(), None);
    let grey = Color(128, 128, 128, 255);
    assert_eq!(color_of(&mut h, "code", "/* b"), grey);
    assert_eq!(color_of(&mut h, "code", "c"), grey, "{carry}: carried in");
    assert_eq!(color_of(&mut h, "code", "d */"), grey);
    // A line without spans shows as plain text.
    assert!(
        !runs(&mut h, "code").contains(&("f".into(), grey)),
        "{carry}: carried out"
    );
}

#[test]
fn a_highlighter_state_can_be_any_luau_value() {
    // A string, a boolean, a table made anew each line and a float: each
    // comes back as it was left.
    grey_lines(
        r#"local function open(state) return state == "comment" end
        local function carried(inside) return if inside then "comment" else nil end"#,
    );
    grey_lines(
        r#"local function open(state) return state == true end
        local function carried(inside) return inside end"#,
    );
    grey_lines(
        r#"local function open(state) return type(state) == "table" and state.depth > 0 end
        local function carried(inside) return { depth = if inside then 1 else 0 } end"#,
    );
    grey_lines(
        r#"local function open(state) return state == 0.5 end
        local function carried(inside) return if inside then 0.5 else 0 end"#,
    );
}

#[test]
fn a_read_only_editor_refuses_typing_and_follows_its_state() {
    use zgx::input::InputEvent;
    let mut h = Harness::inline(
        r##"local locked = ui.state("locked", true)
        local text = ui.state("text", "draft")
        return ui.column {
            ui.textarea { id = "notes", value = text, w = 200, h = 60, read_only = locked },
            ui.input { id = "fixed", value = ui.state("fixed", "kept"), w = 200, read_only = true },
            ui.text { id = "shown", "{text}" },
        }"##,
    );
    h.frames(1);
    let type_in = |h: &mut Harness, id: &str, text: &str| {
        let node = h.find(id).unwrap();
        h.ui.input.focus(&h.ui.scene, Some(node));
        assert_eq!(h.ui.input.focused(), Some(node), "{id} takes focus");
        h.ui.dispatch(InputEvent::Text(text.into()));
        h.frames(1);
    };
    type_in(&mut h, "notes", "!");
    assert_eq!(h.text("shown"), "draft", "read only: refused");
    // The script still sets it.
    h.host
        .set_state("text", zgx_script::Value::Str("set".into()))
        .unwrap();
    h.frames(1);
    assert_eq!(h.text("shown"), "set");
    // Unlocked, the same editor takes typing.
    h.host
        .set_state("locked", zgx_script::Value::Bool(false))
        .unwrap();
    h.frames(1);
    type_in(&mut h, "notes", "!");
    assert!(h.text("shown").contains('!'), "{}", h.text("shown"));
    type_in(&mut h, "fixed", "?");
    let fixed = h.find("fixed").unwrap();
    let editor = h.ui.editor(fixed).unwrap();
    assert_eq!(editor.editor.borrow().text().to_string(), "kept");
    assert_eq!(h.error(), None);
}

/// A system that curls double quotes and corrects "teh".
struct Substituting;
impl zgx::platform::Platform for Substituting {
    fn clipboard_text(&self) -> Option<String> {
        None
    }
    fn set_clipboard_text(&self, _: String) -> bool {
        false
    }
    fn substitutions(
        &self,
        text: &str,
        kinds: zgx::platform::Substitutions,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        // Curled or not, each quote opens or closes.
        let mut open = false;
        let quotes = text
            .char_indices()
            .filter(|(_, c)| matches!(c, '"' | '“' | '”'));
        quotes
            .filter_map(|(i, c)| {
                open = !open;
                (kinds.punctuation && c == '"')
                    .then(|| (i..i + 1, if open { "“" } else { "”" }.into()))
            })
            .collect()
    }
    fn correction(&self, text: &str, word: std::ops::Range<usize>) -> Option<String> {
        (&text[word] == "teh").then(|| "the".into())
    }
    fn open_files(
        &self,
        _: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn save_file(
        &self,
        _: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(None) })
    }
}

#[test]
fn autocorrect_and_smart_punctuation_substitute_what_is_typed() {
    use zgx::input::InputEvent;
    let code = r##"local a = ui.state("a", "")
        local b = ui.state("b", "")
        local c = ui.state("c", "")
        return ui.column {
            ui.input { id = "both", value = a, w = 200, autocorrect = true, smart_punctuation = true },
            ui.textarea { id = "quotes", value = b, w = 200, h = 60, smart_punctuation = true },
            ui.input { id = "plain", value = c, w = 200 },
            ui.text { id = "a", "{a}" },
            ui.text { id = "b", "{b}" },
            ui.text { id = "c", "{c}" },
        }"##;
    let mut h = Harness::wrapped(
        move |runtime| zgx_script::ScriptHost::inline(runtime, "app", code),
        |view| zgx::compose::provide(zgx::platform::PlatformService(Rc::new(Substituting)), view),
    );
    h.frames(1);
    for id in ["both", "quotes", "plain"] {
        let node = h.find(id).unwrap();
        h.ui.input.focus(&h.ui.scene, Some(node));
        for c in "teh \"x\"".chars() {
            h.ui.dispatch(InputEvent::Text(c.into()));
        }
        h.frames(1);
    }
    assert_eq!(h.text("a"), "the “x”");
    assert_eq!(h.text("b"), "teh “x”", "quotes alone");
    assert_eq!(h.text("c"), "teh \"x\"", "neither");
}
