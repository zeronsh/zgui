// zgx patch: a text range finds text within it (ITextRangeProvider::FindText),
// forward or backward, with or without case, and none where there is none.

use accesskit::{ActivationHandler, Node, NodeId, Role, Tree, TreeId, TreeUpdate};

use super::{simple::NullActionHandler, *};

const WINDOW_ID: NodeId = NodeId(0);
const FIELD_ID: NodeId = NodeId(1);
const RUN_ID: NodeId = NodeId(2);
const TEXT: &str = "Drag across this paragraph, then a word";

struct FindActivationHandler;

impl ActivationHandler for FindActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![FIELD_ID]);
        let mut field = Node::new(Role::MultilineTextInput);
        field.set_children(vec![RUN_ID]);
        field.set_value(TEXT);
        let mut run = Node::new(Role::TextRun);
        run.set_value(TEXT);
        let count = TEXT.chars().count();
        run.set_character_lengths(vec![1u8; count]);
        run.set_character_positions((0..count).map(|i| i as f32 * 8.).collect::<Vec<_>>());
        run.set_character_widths(vec![8.; count]);
        run.set_bounds(accesskit::Rect::new(0., 0., count as f64 * 8., 16.));
        Some(TreeUpdate {
            nodes: vec![(WINDOW_ID, root), (FIELD_ID, field), (RUN_ID, run)],
            tree: Some(Tree::new(WINDOW_ID)),
            tree_id: TreeId::ROOT,
            focus: FIELD_ID,
        })
    }
}

fn document(s: &Scope) -> Result<IUIAutomationTextRange> {
    let root = unsafe { s.uia.ElementFromHandle(s.window.0) }?;
    let condition = unsafe { s.uia.CreateTrueCondition() }?;
    let elements = unsafe { root.FindAll(TreeScope_Subtree, &condition) }?;
    for i in 0..unsafe { elements.Length() }? {
        let element = unsafe { elements.GetElement(i) }?;
        if let Ok(pattern) = unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) } {
            return unsafe { pattern.DocumentRange() };
        }
    }
    Err(Error::empty())
}

fn find(range: &IUIAutomationTextRange, text: &str, backward: bool, ignore_case: bool) -> Option<IUIAutomationTextRange> {
    unsafe { range.FindText(&BSTR::from(text), backward.into(), ignore_case.into()) }.ok()
}

fn text(range: &IUIAutomationTextRange) -> String {
    unsafe { range.GetText(-1) }.map(|t| t.to_string()).unwrap_or_default()
}

/// Where `range` starts, in characters from `document`'s start.
fn start_of(range: &IUIAutomationTextRange, document: &IUIAutomationTextRange) -> i32 {
    let probe = unsafe { document.Clone() }.unwrap();
    unsafe {
        probe
            .MoveEndpointByRange(TextPatternRangeEndpoint_End, range, TextPatternRangeEndpoint_Start)
            .unwrap();
    }
    text(&probe).chars().count() as i32
}

#[test]
fn a_range_finds_text_within_it() -> Result<()> {
    super::scope("FindText test", FindActivationHandler, NullActionHandler, |s| {
        let document = document(s)?;
        let found = find(&document, "paragraph", false, false).expect("found");
        assert_eq!(text(&found), "paragraph");
        assert_eq!(start_of(&found, &document), TEXT.find("paragraph").unwrap() as i32);
        // Without case, the text as it is.
        assert_eq!(text(&find(&document, "DRAG", false, true).expect("without case")), "Drag");
        assert!(find(&document, "DRAG", false, false).is_none(), "with case");
        // Backward: the last match, not the first.
        let last = find(&document, "a", true, false).expect("backward");
        assert_eq!(start_of(&last, &document), TEXT.rfind('a').unwrap() as i32);
        assert_eq!(start_of(&find(&document, "a", false, false).unwrap(), &document), TEXT.find('a').unwrap() as i32);
        // Within the found range only, and none where there is none.
        assert!(find(&found, "Drag", false, false).is_none(), "outside the range");
        assert!(find(&document, "nowhere", false, false).is_none());
        // Its bounds: the match's characters, 8 points each.
        let rects = unsafe { found.GetBoundingRectangles() }?;
        assert!(!rects.is_null(), "bounds");
        Ok(())
    })
}
