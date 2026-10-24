// It counts allocations with an allocator of its own: not with the
// `mimalloc` feature's.
#![cfg(not(feature = "mimalloc"))]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use zgx::{
    scene::{Layout as SceneLayout, NodeKind, Scene, Style},
    semantics::{Role, SemanticNode, Semantics},
};

thread_local! {
    // Const, destructor-free TLS keeps allocator bookkeeping allocation-free.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static REQUESTS: Cell<usize> = const { Cell::new(0) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn record_request(size: usize) {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = REQUESTS.try_with(|count| count.set(count.get() + 1));
        let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
    }
}

// SAFETY: Every allocation operation delegates unchanged to System. TLS only
// records calls on the measured test thread and never allocates or owns memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_request(new_size);
        // SAFETY: The original allocation and new size are forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The original allocation and layout are forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocation_requests(f: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTING.with(|enabled| enabled.set(false));
        }
    }
    REQUESTS.with(|count| count.set(0));
    LARGEST.with(|largest| largest.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let reset = Reset;
    f();
    drop(reset);
    REQUESTS.with(Cell::get)
}

#[test]
fn unrelated_semantic_updates_do_not_reallocate_large_editor_projection() {
    use accesskit_consumer::{Node, Tree, TreeChangeHandler};
    use zgx_desktop::accessibility::AccessibilityTree;
    #[derive(Default)]
    struct Changes;
    impl TreeChangeHandler for Changes {
        fn node_added(&mut self, _: &Node) {}
        fn node_updated(&mut self, _: &Node, _: &Node) {}
        fn focus_moved(&mut self, _: Option<&Node>, _: Option<&Node>) {}
        fn node_removed(&mut self, _: &Node) {}
    }

    let document = "a".repeat(128 * 1024);
    let mut scene = Scene::new(400., 200.);
    let editor = scene.append(
        scene.root(),
        NodeKind::Container(SceneLayout::Overlay),
        Style {
            width: Some(300.),
            height: Some(100.),
            ..Style::default()
        },
    );
    let sibling = scene.append(
        scene.root(),
        NodeKind::Container(SceneLayout::Overlay),
        Style {
            width: Some(100.),
            height: Some(30.),
            ..Style::default()
        },
    );
    scene.flush();
    let mut semantics = Semantics::new();
    let mut metadata = SemanticNode::new(Role::MultilineTextInput, "Document");
    metadata.value = Some(document.clone());
    metadata.text_selection = Some((0, 0));
    semantics.set(editor, metadata);
    semantics.set(sibling, SemanticNode::new(Role::Button, "Before"));
    let mut projection = AccessibilityTree::new();
    let initial = projection.update(&scene, &semantics, Some(editor), "App", 1.);
    let editor_id = initial.focus;
    assert_eq!(projection.scene_node(editor_id), Some(editor));
    let editor_native = &initial
        .nodes
        .iter()
        .find(|(id, _)| *id == editor_id)
        .unwrap()
        .1;
    let run_ids = editor_native.children().to_vec();
    assert!(!run_ids.is_empty());
    let initial_selection = *editor_native.text_selection().unwrap();
    let mut consumer = Tree::new(initial, true);

    semantics.update(sibling, |node| node.label = "After".into());
    let mut changed = None;
    allocation_requests(|| {
        changed = Some(projection.update(&scene, &semantics, Some(editor), "App", 1.));
    });
    let largest = LARGEST.with(Cell::get);
    assert!(
        largest < document.len(),
        "unrelated update allocated {largest} bytes"
    );
    let changed = changed.unwrap();
    assert!(
        !changed.nodes.is_empty(),
        "the sibling change must be delivered"
    );
    assert!(
        changed
            .nodes
            .iter()
            .all(|(id, _)| *id != editor_id && !run_ids.contains(id))
    );
    consumer.update_and_process_changes(changed, &mut Changes);
    assert_eq!(
        consumer.state().focus().unwrap().value().as_deref(),
        Some(document.as_str())
    );
    assert_eq!(
        projection.resolve_selection(&initial_selection),
        Some((editor, 0, 0))
    );

    let mut idle = None;
    allocation_requests(|| {
        idle = Some(projection.update(&scene, &semantics, Some(editor), "App", 1.));
    });
    let largest = LARGEST.with(Cell::get);
    assert!(
        largest < document.len(),
        "idle projection allocated {largest} bytes"
    );
    assert!(idle.unwrap().nodes.is_empty());
    assert_eq!(projection.scene_node(editor_id), Some(editor));
    assert_eq!(
        projection.resolve_selection(&initial_selection),
        Some((editor, 0, 0))
    );

    // A later editor mutation must still update the cached native selection.
    semantics.update_text_input(editor, &document, (9, 3), false);
    let selection_update = projection.update(&scene, &semantics, Some(editor), "App", 1.);
    let updated_editor = &selection_update
        .nodes
        .iter()
        .find(|(id, _)| *id == editor_id)
        .unwrap()
        .1;
    assert_eq!(updated_editor.children(), run_ids.as_slice());
    assert_eq!(
        projection.resolve_selection(updated_editor.text_selection().unwrap()),
        Some((editor, 9, 3))
    );
    consumer.update_and_process_changes(selection_update, &mut Changes);
}
