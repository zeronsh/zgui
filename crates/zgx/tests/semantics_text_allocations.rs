use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use zgx::{
    scene::Scene,
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
fn text_metadata_updates_reuse_storage_and_only_advance_changed_revisions() {
    let scene = Scene::new(100., 100.);
    let id = scene.root();
    let text = "a".repeat(256 * 1024);
    let changed = "b".repeat(text.len());
    let mut semantics = Semantics::new();
    let mut node = SemanticNode::new(Role::MultilineTextInput, "Document");
    node.value = Some(text.clone());
    node.text_selection = Some((0, 0));
    semantics.set(id, node);
    let initial_revision = semantics.revision();

    assert_eq!(
        allocation_requests(|| {
            for _ in 0..32 {
                semantics.update_text_input(id, &text, (0, 0), false);
            }
        }),
        0
    );
    assert_eq!(semantics.revision(), initial_revision);

    assert_eq!(
        allocation_requests(|| {
            for position in 1..=32 {
                semantics.update_text_input(id, &text, (position, 0), false);
            }
            semantics.update_text_input(id, &text, (32, 0), true);
            semantics.update_text_input(id, &text, (32, 0), true);
        }),
        0
    );
    assert_eq!(semantics.revision(), initial_revision + 33);
    assert_eq!(semantics.get(id).unwrap().text_selection, Some((32, 0)));
    assert!(semantics.get(id).unwrap().read_only);

    let original_capacity = semantics
        .get(id)
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .capacity();
    assert_eq!(
        allocation_requests(|| {
            semantics.update_text_input(id, &changed, (32, 0), true);
        }),
        0
    );
    assert_eq!(semantics.revision(), initial_revision + 34);
    assert_eq!(
        semantics.get(id).unwrap().value.as_deref(),
        Some(changed.as_str())
    );
    assert_eq!(
        semantics
            .get(id)
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .capacity(),
        original_capacity
    );

    let larger = "c".repeat(original_capacity * 2 + 1);
    assert_eq!(
        allocation_requests(|| {
            semantics.update_text_input(id, &larger, (32, 0), true);
        }),
        1,
        "growing storage should need one allocation request"
    );
    assert_eq!(semantics.revision(), initial_revision + 35);
    assert_eq!(
        allocation_requests(|| {
            semantics.update_text_input(id, &larger, (32, 0), true);
            semantics.update_text_input(id, &text, (0, 0), false);
            semantics.update_text_input(id, &larger, (0, 0), false);
        }),
        0,
        "shrinking must retain capacity for subsequent growth"
    );
    assert_eq!(semantics.revision(), initial_revision + 37);
    assert_eq!(
        semantics.get(id).unwrap().value.as_deref(),
        Some(larger.as_str())
    );

    semantics.remove(id);
    let removed_revision = semantics.revision();
    assert_eq!(
        allocation_requests(|| {
            semantics.update_text_input(id, &larger, (0, 0), false);
        }),
        0
    );
    assert_eq!(semantics.revision(), removed_revision);
    assert!(semantics.get(id).is_none());
}

#[test]
fn warmed_editor_selection_and_policy_refresh_do_not_copy_the_document() {
    use std::sync::Arc;
    use zgx::{
        scene::{NodeId, NodeKind},
        widgets::Ui,
    };

    fn text_node(scene: &Scene, node: NodeId) -> Option<(NodeId, Arc<str>)> {
        if let NodeKind::Text { text, .. } = scene.kind(node) {
            return Some((node, text.clone()));
        }
        scene
            .children(node)
            .iter()
            .find_map(|child| text_node(scene, *child))
    }

    // 32 KiB of Unicode, with 8192 fallback cells: both the text key and
    // accounted layout fit the public editor's bounded shaping reuse policy.
    let document = "🙂".repeat(8192);
    let mut ui = Ui::new(320., 120.);
    let value = ui.signal(document.clone());
    let editor = ui.text_input(ui.root(), "Document", value, 280., true);
    ui.input.focus(&ui.scene, Some(editor.node));
    ui.prepare_frame();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    let (paint_id, original_paint) = text_node(&ui.scene.borrow(), editor.node).unwrap();
    let semantic_pointer = ui
        .semantics
        .borrow()
        .get(editor.node)
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .as_ptr();

    allocation_requests(|| {
        for offset in 1..=16 {
            editor
                .editor
                .borrow_mut()
                .set_selection(offset * 4, offset * 4);
            editor.refresh();
            editor.set_read_only(offset % 2 == 0);
        }
    });
    let largest_request = LARGEST.with(Cell::get);
    assert!(
        largest_request < document.len(),
        "warm metadata refresh allocated {largest_request} bytes for a {}-byte document",
        document.len()
    );
    let scene = ui.scene.borrow();
    let NodeKind::Text {
        text: current_paint,
        ..
    } = scene.kind(paint_id)
    else {
        panic!("retained text node changed kind");
    };
    assert!(Arc::ptr_eq(&original_paint, current_paint));
    assert_eq!(current_paint.as_ref(), document);
    let semantics = ui.semantics.borrow();
    let metadata = semantics.get(editor.node).unwrap();
    let current_value = metadata.value.as_ref().unwrap();
    assert_eq!(current_value.as_ptr(), semantic_pointer);
    assert_eq!(current_value, &document);
    assert_eq!(metadata.text_selection, Some((64, 64)));
    assert!(metadata.read_only);
}
