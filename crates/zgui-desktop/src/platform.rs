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
    #[cfg(target_os = "macos")]
    fn misspellings(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        spelling::misspellings(text)
    }
    #[cfg(target_os = "macos")]
    fn spelling_guesses(&self, word: &str) -> Vec<String> {
        spelling::guesses(word)
    }
    #[cfg(target_os = "macos")]
    fn learn_spelling(&self, word: &str) {
        spelling::learn(word);
    }
    #[cfg(target_os = "macos")]
    fn ignore_spelling(&self, word: &str) {
        spelling::ignore(word);
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

/// macOS's spelling checker (`NSSpellChecker`), for editors that check
/// (`View::spellcheck`). One document tag for the app: what Ignore Spelling
/// ignores, it ignores in every editor until the app quits.
#[cfg(target_os = "macos")]
mod spelling {
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSRange, NSString};
    use std::ops::Range;

    fn tag() -> isize {
        static TAG: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
        *TAG.get_or_init(NSSpellChecker::uniqueSpellDocumentTag)
    }
    /// UTF-16 offset `units` into `text` as a UTF-8 byte offset.
    fn byte_offset(text: &str, units: usize) -> usize {
        let mut seen = 0;
        for (byte, c) in text.char_indices() {
            if seen >= units {
                return byte;
            }
            seen += c.len_utf16();
        }
        text.len()
    }
    pub(super) fn misspellings(text: &str) -> Vec<Range<usize>> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let string = NSString::from_str(text);
        let length = string.length();
        let mut found = Vec::new();
        let mut from = 0;
        while from < length {
            // SAFETY: no word count is asked for (a null pointer).
            let range = unsafe {
                checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                    &string,
                    from as isize,
                    None,
                    false,
                    tag(),
                    std::ptr::null_mut(),
                )
            };
            if range.length == 0 || range.location >= length {
                break;
            }
            let start = byte_offset(text, range.location);
            let end = byte_offset(text, range.location + range.length);
            found.push(start..end);
            from = range.location + range.length;
        }
        found
    }
    pub(super) fn guesses(word: &str) -> Vec<String> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let string = NSString::from_str(word);
        let range = NSRange::new(0, string.length());
        checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                range,
                &string,
                None,
                tag(),
            )
            .map(|guesses| guesses.iter().map(|g| g.to_string()).collect())
            .unwrap_or_default()
    }
    pub(super) fn learn(word: &str) {
        NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
    }
    pub(super) fn ignore(word: &str) {
        NSSpellChecker::sharedSpellChecker()
            .ignoreWord_inSpellDocumentWithTag(&NSString::from_str(word), tag());
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_system_checker_finds_misspellings_as_byte_ranges() {
            let text = "Thé quikc brown fox";
            let found = super::misspellings(text);
            assert!(
                found.iter().any(|r| &text[r.clone()] == "quikc"),
                "{found:?}"
            );
            assert!(!super::guesses("quikc").is_empty());
        }
    }
}
