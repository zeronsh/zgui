//! The driver's vocabulary (`zgx_script::driver`), as tests and agents both
//! name nodes: ids, labels and texts, roles, lists' rows, nesting, and the
//! errors that say what matched.
mod common;

use common::Harness;

fn app() -> Harness {
    Harness::inline(
        r##"local items = ui.state("items", { "Inbox", "Archive", "Trash" })
        local said = ui.state("said", "")
        return ui.column {
            ui.row { id = "toolbar",
                ui.button { id = "send", w = 60, h = 24, on_click = ui.set(said, "sent"), "Send" },
                ui.button { w = 60, h = 24, on_click = ui.set(said, "copied"), "Copy" },
                ui.button { w = 60, h = 24, label = "Copy link", on_click = ui.set(said, "linked"), "🔗" },
            },
            ui.text { id = "said", "{said}" },
            ui.input { id = "name", label = "Name", value = ui.state("name", ""), w = 120 },
            ui.list { id = "boxes", count = 3, height = 20, w = 200, h = 80,
                row = function(i) return ui.text { items:peek()[i], literal = true } end },
            ui.column { id = "more", ui.button { w = 60, h = 24, "Copy" } },
        }"##,
    )
}

#[test]
fn targets_name_nodes_by_id_label_text_role_and_row() {
    let mut h = app();
    assert!(h.find("send").is_some(), "a bare id, as tests write");
    assert_eq!(h.find("#send"), h.find("send"));
    assert_eq!(
        h.find(r#""Send""#),
        h.find("send"),
        "the button, not its text inside"
    );
    assert_eq!(h.find(r#"role:button "Send""#), h.find("send"));
    assert_eq!(
        h.find(r#""Copy link""#).map(|_| ()),
        Some(()),
        "an accessible name"
    );
    // Two "Copy" buttons: an error naming both, then ways to say which.
    let error = h.target(r#""Copy""#).unwrap_err();
    assert!(error.contains("matches 2 nodes"), "{error}");
    assert!(error.contains("button"), "{error}");
    assert!(h.find(r#"#toolbar >> "Copy""#).is_some());
    assert!(h.find(r#"#more >> "Copy""#).is_some());
    assert!(h.find(r#""Copy"[2]"#).is_some());
    // A list's row, and a node by its number.
    let row = h.find("#boxes row 2").unwrap();
    assert_eq!(
        zgx_script::driver::first_text(&h.ui.scene.borrow(), row).as_deref(),
        Some("Archive")
    );
    let number = zgx_script::driver::number(row);
    assert_eq!(h.find(&format!("@{number}")), Some(row));
    let error = h.target("#boxes row 9").unwrap_err();
    assert!(error.contains("3 rows mounted"), "{error}");
    assert!(
        h.target(r#""Nowhere""#)
            .unwrap_err()
            .contains("nothing matches")
    );
}

#[test]
fn actions_and_the_tree_read_as_agents_see_them() {
    let mut h = app();
    h.click(r#"role:button "Send""#);
    h.frames(1);
    assert_eq!(h.text("said"), "sent");
    h.type_text(r#""Name""#, "Ada");
    h.frames(1);
    let tree = h.tree();
    // A line a node, in document order: number, id, role, name, value, box.
    assert!(tree.contains(r#"#send button "Send""#), "{tree}");
    assert!(tree.contains(r#"#name textbox "Name" ="Ada""#), "{tree}");
    assert!(tree.contains("focused"), "{tree}");
    let send = tree.find("#send").unwrap();
    let name = tree.find("#name").unwrap();
    assert!(send < name, "document order");
    assert_eq!(tree, h.tree(), "the same tree twice reads the same");
    h.keys("cmd+a");
    h.keys("Backspace");
    h.frames(1);
    assert!(!h.tree().contains(r#"="Ada""#), "cleared");
    assert_eq!(h.error(), None);
}

#[test]
fn targets_and_the_tree_reach_into_a_modal_and_a_popover_in_it() {
    let mut h = Harness::inline(
        r##"return ui.column { id = "scope",
            ui.modal { id = "sheet", label = "Notes", open = ui.state("open", true),
                ui.popover { label = "Actions", open = ui.state("actions", true),
                    anchor = ui.button { "Actions" },
                    ui.button { id = "save", "Save notes" } } } }"##,
    );
    h.frames(2);
    let save = h.handle.find("save").unwrap();
    assert_eq!(h.target(r#""Save notes""#).unwrap(), save);
    assert_eq!(h.target(r#"role:button "Save notes""#).unwrap(), save);
    assert_eq!(
        h.target(r#"#scope >> #sheet >> role:button "Save notes""#)
            .unwrap(),
        save
    );
    // Once each, the popover's under the modal's, though the scene puts
    // its overlay inside the modal's own.
    let (lines, _) = zgx_script::driver::tree(&h.ui, &h.handle, None, &Default::default());
    let all = zgx_script::driver::tree_text(&lines, 0);
    let depth = |role: &str, text: &str| {
        let found: Vec<_> = lines
            .iter()
            .filter(|line| line.role == Some(role) && line.text.as_deref() == Some(text))
            .collect();
        assert_eq!(found.len(), 1, "{role} {text}: {all}");
        found[0].depth
    };
    let (sheet, panel) = (depth("dialog", "Notes"), depth("dialog", "Actions"));
    assert!(sheet < panel && panel < depth("button", "Save notes"));
}

#[test]
fn a_target_does_not_reach_another_views_modal() {
    use zgx::compose::prelude::*;
    use zgx_script::driver::find;
    let mut ui = zgx::widgets::Ui::new(400., 300.);
    let first = ui.mount(modal("First", ui.signal(true)).child(button().child(text("Mine"))));
    let other = ui.mount(modal("Other", ui.signal(true)).child(button().child(text("Theirs"))));
    ui.prepare_frame();
    assert!(find(&ui, &first, r#""Mine""#).is_ok());
    assert!(find(&ui, &first, r#""Theirs""#).is_err());
    assert!(find(&ui, &other, r#""Theirs""#).is_ok());
}
