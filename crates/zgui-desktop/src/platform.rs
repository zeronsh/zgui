//! The desktop's `zgui::platform::Platform`: the native clipboard and the
//! window's file dialogs, provided to what a window renders.
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use zgui::platform::{FilePicker, Pending, Platform};

pub(crate) struct Desktop {
    pub(crate) clipboard: Rc<RefCell<Option<crate::clipboard::Clipboard>>>,
    pub(crate) dialogs: crate::FileDialogs,
}

fn options(picker: &FilePicker) -> crate::FileDialogOptions {
    let mut options = crate::FileDialogOptions::new(picker.title.clone());
    options.directory = picker.directory.clone();
    options.file_name = picker.file_name.clone();
    options.filters = picker
        .filters
        .iter()
        .map(|(name, extensions)| crate::FileFilter::new(name.clone(), extensions.clone()))
        .collect();
    options
}

impl Platform for Desktop {
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.borrow_mut().as_mut()?.get_text().ok()
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        self.clipboard
            .borrow_mut()
            .as_mut()
            .is_some_and(|clipboard| clipboard.set_text(text).is_ok())
    }
    fn open_url(&self, url: &str) -> bool {
        // The launcher runs detached: nothing waits on the browser.
        crate::url::valid_url(url) && open::that_detached(url).is_ok()
    }
    fn set_clipboard_rich_text(&self, text: String, html: String) -> bool {
        self.clipboard
            .borrow_mut()
            .as_mut()
            .is_some_and(|clipboard| clipboard.set_rich_text(text, html).is_ok())
    }
    fn open_files(&self, picker: FilePicker) -> Pending<Result<Vec<PathBuf>, String>> {
        let dialogs = self.dialogs.clone();
        Box::pin(async move {
            let options = options(&picker);
            let picked = if picker.multiple {
                dialogs.open_files(options).await
            } else {
                dialogs
                    .open_file(options)
                    .await
                    .map(|one| one.map(|path| vec![path]))
            };
            picked
                .map(Option::unwrap_or_default)
                .map_err(|e| e.to_string())
        })
    }
    fn save_file(&self, picker: FilePicker) -> Pending<Result<Option<PathBuf>, String>> {
        let dialogs = self.dialogs.clone();
        Box::pin(async move {
            dialogs
                .save_file(options(&picker))
                .await
                .map_err(|e| e.to_string())
        })
    }
}
