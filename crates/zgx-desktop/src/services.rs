//! macOS's Services menu on a window's selection (winit's view answers it,
//! `winit::platform::macos::set_selection_services`): a service takes the
//! selected text, a field's or selectable text's, and one that returns
//! text puts it in place of a field's selection, as in AppKit's text views.
use std::{cell::RefCell, collections::HashMap};
use winit::window::WindowId;

thread_local! {
    /// The windows' interfaces, by their winit window.
    static WINDOWS: RefCell<HashMap<WindowId, zgx::widgets::WeakUi>> = RefCell::default();
}

/// `window` shows `ui`: the Services menu acts on its selection. The first
/// also hands the views their answers and registers the text the app sends
/// and takes.
pub(crate) fn add(window: WindowId, ui: &zgx::widgets::Ui) {
    let first = WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.insert(window, ui.downgrade());
        windows.len() == 1
    });
    if first {
        winit::platform::macos::set_selection_services(Some(std::rc::Rc::new(Windows)));
        register_types();
    }
}

/// `window` closed.
pub(crate) fn remove(window: WindowId) {
    WINDOWS.with(|windows| windows.borrow_mut().remove(&window));
}

fn ui(window: WindowId) -> Option<zgx::widgets::Ui> {
    WINDOWS.with(|windows| windows.borrow().get(&window)?.upgrade())
}

struct Windows;
impl winit::platform::macos::SelectionServices for Windows {
    fn selected_text(&self, window: WindowId) -> Option<String> {
        let ui = ui(window)?;
        // A password's `copy` is empty: its text never leaves it.
        let text = match ui.focused_editor() {
            Some(editor) => editor.copy(),
            None if ui.has_text_selection() => ui.selected_text(),
            None => String::new(),
        };
        (!text.is_empty()).then_some(text)
    }
    fn selection_editable(&self, window: WindowId) -> bool {
        ui(window)
            .and_then(|ui| ui.focused_editor())
            .is_some_and(|editor| !editor.is_read_only() && !editor.copy().is_empty())
    }
    fn replace_selection(&self, window: WindowId, text: &str) -> bool {
        let Some(editor) = ui(window).and_then(|ui| ui.focused_editor()) else {
            return false;
        };
        if editor.is_read_only() || editor.copy().is_empty() {
            return false;
        }
        editor.paste(text);
        true
    }
}

/// The app sends and takes plain text through Services.
fn register_types() {
    use objc2_app_kit::{NSApplication, NSPasteboardTypeString};
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    // SAFETY: AppKit's pasteboard type constant, on the main thread.
    let types = objc2_foundation::NSArray::from_slice(&[unsafe { NSPasteboardTypeString }]);
    NSApplication::sharedApplication(mtm).registerServicesMenuSendTypes_returnTypes(&types, &types);
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::platform::macos::SelectionServices;
    use zgx::compose::prelude::*;

    #[test]
    fn services_take_the_selection_and_put_their_text_in_a_fields_place() {
        let mut ui = zgx::widgets::Ui::new(400., 300.);
        let value = ui.signal(String::from("make this plain"));
        let secret = ui.signal(String::from("hunter2"));
        let view = ui.mount(
            column()
                .child(
                    text_input("Name", value.clone())
                        .id("field")
                        .size(200., 30.),
                )
                .child(
                    text_input("Password", secret)
                        .secure(true)
                        .id("secret")
                        .size(200., 30.),
                )
                .child(column().selectable(true).child(text("Display text"))),
        );
        ui.prepare_frame();
        // A placeholder id, never a window's.
        let window = WindowId::dummy();
        WINDOWS.with(|windows| windows.borrow_mut().insert(window, ui.downgrade()));
        let services = Windows;
        assert_eq!(services.selected_text(window), None, "nothing selected");
        // A field's selection: sent, and replaced by what returns.
        let field = view.find("field").unwrap();
        ui.input.focus(&ui.scene, Some(field));
        ui.editor(field).unwrap().set_selection(5, 9);
        assert_eq!(services.selected_text(window).as_deref(), Some("this"));
        assert!(services.selection_editable(window));
        assert!(services.replace_selection(window, "it"));
        assert_eq!(value.get(), "make it plain");
        // A password's: never sent.
        let secret = view.find("secret").unwrap();
        ui.input.focus(&ui.scene, Some(secret));
        ui.editor(secret).unwrap().set_selection(0, 7);
        assert_eq!(services.selected_text(window), None);
        assert!(!services.selection_editable(window));
        // Selected display text: sent, not replaced.
        ui.input.focus(&ui.scene, None);
        let label = {
            let scene = ui.scene.borrow();
            let mut stack = vec![scene.root()];
            let mut found = None;
            while let Some(node) = stack.pop() {
                if matches!(scene.kind(node), zgx::scene::NodeKind::Text { text, .. } if &**text == "Display text")
                {
                    found = Some(node);
                }
                stack.extend(scene.children(node).iter().copied());
            }
            found.unwrap()
        };
        assert!(ui.select_text(
            zgx::selection::TextPoint {
                node: label,
                offset: 0
            },
            zgx::selection::TextPoint {
                node: label,
                offset: 7
            },
        ));
        assert_eq!(services.selected_text(window).as_deref(), Some("Display"));
        assert!(!services.selection_editable(window));
        assert!(!services.replace_selection(window, "x"));
        remove(window);
        assert_eq!(services.selected_text(window), None, "closed");
    }
}
