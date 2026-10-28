//! Scripts' input placeholders, selection API, select-on-focus, window drag
//! regions, and hover's enter/leave semantics.
mod common;

use common::Harness;
use zgx::{
    input::{InputEvent, Modifiers, PointerButton},
    scene::NodeKind,
    widgets::WindowGesture,
};

impl Harness {
    /// The first text a node paints.
    fn painted(&mut self, id: &str) -> String {
        self.ui.prepare_frame();
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        let scene = self.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(n) {
                return text.to_string();
            }
            stack.extend(scene.children(n).iter().rev());
        }
        String::new()
    }
    fn press(&mut self, x: f32, y: f32) {
        let button = PointerButton::Primary;
        let none = Modifiers::default();
        self.ui
            .dispatch_with_modifiers(InputEvent::PointerDown { x, y, button }, none);
        self.ui
            .dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, none);
    }
}

#[test]
fn placeholders_show_while_empty() {
    let mut h = Harness::inline(
        r##"local query = ui.state("query", "")
        local hint = ui.state("hint", "Search notes")
        return ui.column {
            ui.input { id = "query", label = "Search", value = query, w = 200,
                placeholder = hint, placeholder_color = "#808080" },
            ui.textarea { id = "body", label = "Body", value = ui.state("body", ""), w = 200, h = 80,
                placeholder = function() return "Start writing" end },
            ui.button { id = "type", w = 10, h = 10, on_click = function() query:set("zgx") end },
            ui.button { id = "rename", w = 10, h = 10, on_click = function() hint:set("Find") end },
        }"##,
    );
    assert_eq!(h.painted("query"), "Search notes");
    assert_eq!(h.painted("body"), "Start writing");
    let node = h.find("query").unwrap();
    assert_eq!(
        h.ui.semantics
            .borrow()
            .get(node)
            .unwrap()
            .placeholder
            .as_deref(),
        Some("Search notes")
    );
    h.click("rename");
    assert_eq!(h.painted("query"), "Find");
    h.click("type");
    assert_eq!(h.painted("query"), "zgx");
}

#[test]
fn selection_api_counts_characters() {
    let mut h = Harness::inline(
        r##"local text = ui.state("text", "naïve café")
        local log = ui.state("log", "")
        local field
        return ui.column {
            ui.input { id = "field", label = "Text", value = text, w = 200,
                on_mount = function(el) field = el end },
            ui.button { id = "all", w = 10, h = 10, on_click = function()
                field:select_all()
                local a, b = field:selection()
                log:set(a .. "-" .. b)
            end },
            ui.button { id = "some", w = 10, h = 10, on_click = function()
                field:set_selection(6, 2)
                local a, b = field:selection()
                log:set(a .. "-" .. b)
            end },
            ui.button { id = "caret", w = 10, h = 10, on_click = function()
                field:set_selection(3)
                local a, b = field:selection()
                log:set(a .. "-" .. b)
            end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.click("all");
    assert_eq!(h.text("log"), "0-10");
    h.click("some");
    assert_eq!(h.text("log"), "6-2");
    let node = h.find("field").unwrap();
    let editor = h.ui.editor(node).unwrap();
    // "na" is two bytes, "naïve " seven.
    assert_eq!(editor.selection(), (7, 2));
    h.click("caret");
    assert_eq!(h.text("log"), "3-3");
}

#[test]
fn an_address_bar_selects_all_on_focus() {
    let mut h = Harness::inline(
        r##"local address = ui.state("address", "example.com")
        return ui.column {
            ui.input { id = "address", label = "Address", value = address, w = 300, h = 24,
                select_on_focus = true,
                -- Focus shows the whole URL, as Safari's bar does.
                on_focus = function() address:set("https://example.com/") end },
        }"##,
    );
    h.ui.prepare_frame();
    let node = h.find("address").unwrap();
    let at = h.ui.scene.borrow().bounds(node);
    let (x, y) = (at.x + 30., at.y + at.height / 2.);
    h.press(x, y);
    let editor = h.ui.editor(node).unwrap();
    assert_eq!(h.ui.input.focused(), Some(node));
    assert_eq!(editor.editor.borrow().text(), "https://example.com/");
    assert_eq!(editor.selection(), (0, "https://example.com/".len()));
    // Not a double click: a caret where the second click lands, as it is
    // let go (pressed, the selection could be dragged, as a browser's).
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
    let (a, b) = editor.selection();
    assert_eq!(a, b);
    assert!(a > 0 && a < "https://example.com/".len());
}

#[test]
fn window_drag_regions_ask_the_host_to_move_the_window() {
    let mut h = Harness::inline(
        r##"return ui.row { id = "bar", w = 300, h = 40, window_drag = true,
            ui.div { w = 100, h = 40 },
            ui.button { id = "close", w = 40, h = 40, on_click = function() end },
        }"##,
    );
    h.ui.prepare_frame();
    h.press(50., 20.);
    assert_eq!(h.ui.take_window_gesture(), Some(WindowGesture::Drag));
    h.press(50., 20.);
    assert_eq!(h.ui.take_window_gesture(), Some(WindowGesture::DoubleClick));
    let close = h.find("close").unwrap();
    let at = h.ui.scene.borrow().bounds(close);
    h.press(at.x + 10., 20.);
    assert_eq!(h.ui.take_window_gesture(), None);
}

#[test]
fn hover_holds_across_children() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        return ui.column {
            ui.row { id = "area", w = 200, h = 50,
                on_hover = function(inside) log:set(log:peek() .. (inside and "[" or "]")) end,
                ui.button { w = 50, h = 50, on_click = function() end,
                    on_hover = function(inside) log:set(log:peek() .. (inside and "a" or "A")) end },
                ui.button { w = 50, h = 50, on_click = function() end },
            },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    for x in [10., 60., 20., 150.] {
        h.ui.dispatch(InputEvent::PointerMove { x, y: 10. });
    }
    h.ui.dispatch(InputEvent::PointerLeave);
    // The row: one enter, one leave; the first button its own two pairs.
    assert_eq!(h.text("log"), "[aAaA]");
}

/// HTML's `inputmode`: what kind of text a field takes, a hint the
/// platform's input reads (Windows' touch keyboard and input scope).
#[test]
fn inputmode_says_what_kind_of_text_a_field_takes() {
    use zgx::text_edit::InputMode;
    let mut h = Harness::inline(
        r##"local v = ui.state("v", "")
        return ui.column {
            ui.input { id = "email", label = "Email", value = v, w = 300, h = 24, inputmode = "email" },
            ui.input { id = "pin", label = "PIN", value = v, w = 300, h = 24, inputmode = "numeric" },
            ui.input { id = "keypad", label = "Amount", value = v, w = 300, h = 24, inputmode = "none" },
            ui.input { id = "plain", label = "Name", value = v, w = 300, h = 24 },
        }"##,
    );
    h.ui.prepare_frame();
    let mut mode = |id: &str| {
        let node = h.find(id).unwrap();
        h.ui.editor(node).unwrap().input_mode()
    };
    assert_eq!(mode("email"), InputMode::Email);
    assert_eq!(mode("pin"), InputMode::Numeric);
    assert_eq!(mode("keypad"), InputMode::None);
    assert_eq!(mode("plain"), InputMode::Text);
}

/// A mode it doesn't know is the script's error, naming those it does.
#[test]
#[should_panic(expected = "unknown inputmode `phone`: one of text, decimal, numeric, tel")]
fn an_unknown_inputmode_is_an_error() {
    Harness::inline(
        r##"return ui.input { label = "Phone", value = ui.state("p", ""), inputmode = "phone" }"##,
    );
}
