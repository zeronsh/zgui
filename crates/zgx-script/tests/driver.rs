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
fn targets_and_trees_follow_owned_modal_and_nested_popover_portals() {
    let mut h = Harness::inline(
        r##"return ui.column { id = "scope",
            ui.modal { id = "sheet", label = "Notes", open = ui.state("open", true),
                ui.popover { label = "Actions", open = ui.state("actions", true),
                    anchor = ui.button { "Actions" },
                    ui.button { id = "open:panel:notes/notes", "Save notes" } } } }"##,
    );
    h.frames(2);
    let button = h.handle.find("open:panel:notes/notes").unwrap();
    assert_eq!(h.find("open:panel:notes/notes"), Some(button));
    for target in [
        "#open:panel:notes/notes",
        "open:panel:notes/notes",
        r#""Save notes""#,
        r#"role:button "Save notes""#,
        r#"#scope >> #sheet >> role:dialog "Actions" >> role:button "Save notes""#,
    ] {
        assert_eq!(h.target(target).unwrap(), button, "{target}");
    }
    let number = zgx_script::driver::number(button);
    assert_eq!(h.target(&format!("@{number}")).unwrap(), button);
    let (lines, hidden) = zgx_script::driver::tree(&h.ui, &h.handle, None, &Default::default());
    assert_eq!(hidden, 0);
    assert_eq!(lines.iter().filter(|line| line.number == number).count(), 1);
    let scope = lines
        .iter()
        .find(|line| line.id.as_deref() == Some("scope"))
        .unwrap();
    let sheet = lines
        .iter()
        .find(|line| line.id.as_deref() == Some("sheet"))
        .unwrap();
    let saved = lines.iter().find(|line| line.number == number).unwrap();
    assert!(scope.depth < sheet.depth && sheet.depth < saved.depth);
    let (subtree, _) = zgx_script::driver::tree(
        &h.ui,
        &h.handle,
        h.handle.find("sheet"),
        &Default::default(),
    );
    assert!(subtree.iter().any(|line| line.number == number));
    assert_eq!(h.error(), None);
}

#[test]
fn anonymous_portals_are_scoped_to_their_view_handle() {
    use zgx::compose::{button, column, modal};
    use zgx::widgets::Ui;
    use zgx_script::driver::{TreeOptions, find, tree};

    let mut ui = Ui::new(400., 300.);
    let first = ui.mount(
        column().child(
            modal("First dialog", ui.signal(true))
                .child(button().child(zgx::compose::text("First action"))),
        ),
    );
    let second = ui.mount(
        column().child(
            modal("Other dialog", ui.signal(true)).child(
                button()
                    .id("foreign/action")
                    .child(zgx::compose::text("Other action")),
            ),
        ),
    );
    ui.prepare_frame();
    assert!(find(&ui, &first, r#"role:dialog "First dialog""#).is_ok());
    assert!(find(&ui, &first, r#""First action""#).is_ok());
    assert!(find(&ui, &first, r#"role:button "Other action""#).is_err());
    assert!(find(&ui, &first, "#foreign/action").is_err());
    let foreign = second.find("foreign/action").unwrap();
    assert!(find(&ui, &first, &format!("@{}", foreign.number())).is_err());
    let (lines, _) = tree(&ui, &first, None, &TreeOptions::default());
    assert!(
        !lines
            .iter()
            .any(|line| line.text.as_deref() == Some("Other action"))
    );
    assert!(
        tree(&ui, &first, Some(foreign), &TreeOptions::default())
            .0
            .is_empty()
    );
    assert_eq!(first.portals().len(), 1);
    assert_eq!(second.portals().len(), 1);
    first.unmount();
    assert!(first.portals().is_empty());
    assert!(
        tree(&ui, &first, None, &TreeOptions::default())
            .0
            .is_empty()
    );
    assert_eq!(second.portals().len(), 1);
    assert!(find(&ui, &second, "#foreign/action").is_ok());
}
