//! An editor's context menu, as a browser's text fields have one: Cut, Copy,
//! Paste and Select All, each disabled where it would do nothing (nothing
//! selected, a read-only or password field, an empty clipboard).
use crate::{compose::menu_item, platform::PlatformService, widgets::EditorHandle};
use std::rc::Rc;

pub(crate) fn context_menu(
    editor: EditorHandle,
    platform: Option<Rc<PlatformService>>,
) -> crate::compose_context_menu::ContextMenuBuilder {
    Box::new(move |_| {
        // A password field's text never leaves it: `copy` gives nothing.
        let copyable = !editor.copy().is_empty();
        let editable = !editor.is_read_only();
        let clipboard = platform
            .as_ref()
            .and_then(|platform| platform.0.clipboard_text())
            .filter(|text| !text.is_empty());
        let (cut, copy, paste, all) = (
            (editor.clone(), platform.clone()),
            (editor.clone(), platform.clone()),
            editor.clone(),
            editor.clone(),
        );
        // On a misspelled word: the checker's guesses first, as AppKit's
        // and the browsers' menus have them.
        let mut items = platform
            .as_ref()
            .map(|platform| crate::editor_spell::menu_items(&editor, platform))
            .unwrap_or_default();
        // Look Up the selection, where the platform looks words up.
        if copyable && let Some(platform) = platform.clone().filter(|p| p.0.looks_up()) {
            let look = editor.clone();
            items.insert(
                0,
                menu_item(crate::selection::look_up_label(&editor.copy()))
                    .id("editor-look-up")
                    .on_click(move || {
                        look.look_up(&platform);
                    }),
            );
        }
        if !items.is_empty() {
            items.push(separator());
        }
        items.extend([
            menu_item("Cut")
                .id("editor-cut")
                .disabled(!(copyable && editable && platform.is_some()))
                .on_click(move || {
                    let (editor, platform) = &cut;
                    if let Some(platform) = platform
                        && platform.0.set_clipboard_text(editor.copy())
                    {
                        editor.cut();
                    }
                }),
            menu_item("Copy")
                .id("editor-copy")
                .disabled(!(copyable && platform.is_some()))
                .on_click(move || {
                    let (editor, platform) = &copy;
                    if let Some(platform) = platform {
                        platform.0.set_clipboard_text(editor.copy());
                    }
                }),
            menu_item("Paste")
                .id("editor-paste")
                .disabled(!editable || clipboard.is_none())
                .on_click(move || {
                    if let Some(text) = &clipboard {
                        paste.paste(text);
                    }
                }),
            menu_item("Select All")
                .id("editor-select-all")
                .on_click(move || all.select_all()),
        ]);
        items
    })
}

/// A rule between a menu's groups.
fn separator() -> crate::compose::View {
    use crate::style::Styled;
    crate::compose::div()
        .h(1.)
        .my(4.)
        .mx(8.)
        .bg(crate::theme::tok::BORDER_SUBTLE)
}
