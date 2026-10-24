// zgx patch: UIA's HeadingLevel follows a heading's level.

use accesskit::{ActivationHandler, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use windows::Win32::System::Variant::VARIANT;

use super::{simple::NullActionHandler, *};

const WINDOW_ID: NodeId = NodeId(0);

fn heading(label: &str, level: usize) -> Node {
    let mut node = Node::new(Role::Heading);
    node.set_label(label);
    node.set_level(level);
    node
}

struct HeadingActivationHandler;

impl ActivationHandler for HeadingActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(1), NodeId(2), NodeId(3)]);
        let mut label = Node::new(Role::Label);
        label.set_value("Body");
        Some(TreeUpdate {
            nodes: vec![
                (WINDOW_ID, root),
                (NodeId(1), heading("Inbox", 2)),
                (NodeId(2), heading("Deep", 12)),
                (NodeId(3), label),
            ],
            tree: Some(Tree::new(WINDOW_ID)),
            tree_id: TreeId::ROOT,
            focus: WINDOW_ID,
        })
    }
}

fn heading_level(s: &Scope, name: &str) -> Result<i32> {
    let root = unsafe { s.uia.ElementFromHandle(s.window.0) }?;
    let condition = unsafe {
        s.uia
            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(name)))
    }?;
    let element = unsafe { root.FindFirst(TreeScope_Descendants, &condition) }?;
    let value = unsafe { element.GetCurrentPropertyValue(UIA_HeadingLevelPropertyId) }?;
    i32::try_from(&value)
}

#[test]
fn headings_have_their_level_as_heading_level() -> Result<()> {
    super::scope(
        "Heading test",
        HeadingActivationHandler,
        NullActionHandler,
        |s| {
            assert_eq!(heading_level(s, "Inbox")?, HeadingLevel2.0);
            // Past HeadingLevel9, and anything not a heading, have none.
            assert_eq!(heading_level(s, "Deep")?, HeadingLevel_None.0);
            assert_eq!(heading_level(s, "Body")?, HeadingLevel_None.0);
            Ok(())
        },
    )
}
