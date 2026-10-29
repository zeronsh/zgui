//! A mention's suggestions, as a chat composer makes them: a popover at
//! the textarea's caret (`at_caret`) that takes no focus, the textarea's
//! keys moving through it and telling assistive technology the one it is
//! on (`active_descendant`).
mod common;
use common::Harness;
use zgx::input::{InputEvent, Key, Modifiers};

const APP: &str = r#"
local people = { "ada", "grace", "alan" }
local text = ui.state("text", "")
local active = ui.state("active", 1)
local asking = ui.computed(function()
    return text:get():match("@(%w*)$") ~= nil
end)
return ui.column {
    p = 20,
    ui.popover {
        open = function() return asking:get() end,
        on_close = function() end,
        at_caret = true,
        w = 120,
        anchor = ui.textarea {
            id = "composer",
            value = text,
            w = 300,
            h = 60,
            active_descendant = function()
                return if asking:get() then people[active:get()] else nil
            end,
            on_key = function(e)
                if asking:peek() and e.key == "ArrowDown" then
                    active:set(active:peek() % #people + 1)
                    return true
                end
                return false
            end,
        },
        ui.text { id = "ada", "Ada" },
        ui.text { id = "grace", "Grace" },
        ui.text { id = "alan", "Alan" },
    },
}
"#;

fn active(h: &mut Harness, editor: zgx::scene::NodeId) -> Option<zgx::scene::NodeId> {
    h.ui.semantics
        .borrow()
        .get(editor)
        .unwrap()
        .active_descendant
}

#[test]
fn suggestions_open_at_the_caret_and_the_textarea_moves_through_them() {
    let mut h = Harness::inline(APP);
    let editor = h.find("composer").unwrap();
    assert!(h.ui.input.focus(&h.ui.scene, Some(editor)));
    h.ui.dispatch(InputEvent::Text("Hi @".into()));
    h.frames(2);
    let ada = h.find("ada").unwrap();
    let grace = h.find("grace").unwrap();
    // Under the caret, focus left in the textarea.
    let caret = h.ui.editor(editor).unwrap().input_method_area();
    let panel = h.ui.scene.borrow().parent(ada).unwrap();
    let panel = h.ui.scene.borrow().transformed_bounds(panel);
    assert!(
        (panel.y - (caret.y + caret.height + 4.)).abs() < 0.5,
        "{panel:?} {caret:?}"
    );
    assert_eq!(h.ui.input.focused(), Some(editor));
    assert_eq!(active(&mut h, editor), Some(ada));
    h.ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    h.frames(1);
    assert_eq!(active(&mut h, editor), Some(grace));
    // Typing goes on into the textarea.
    h.ui.dispatch(InputEvent::Text("gr".into()));
    h.frames(1);
    assert_eq!(h.ui.input.focused(), Some(editor));
    assert_eq!(
        h.ui.editor(editor).unwrap().editor.borrow().text(),
        "Hi @gr"
    );
    // A space ends the mention: it closes, and points at nothing.
    h.ui.dispatch(InputEvent::Text(" ".into()));
    h.frames(2);
    assert_eq!(active(&mut h, editor), None);
    assert_eq!(h.error(), None);
}
