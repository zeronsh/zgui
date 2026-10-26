//! What an app reaches on the machine beyond its window: the clipboard (text,
//! HTML and images) and the file dialogs. A desktop host provides it to the components it renders
//! (`compose::provide`, as `Context::try_service::<PlatformService>()`);
//! headless runs have none, and tests provide a fake.
use std::{future::Future, path::PathBuf, pin::Pin, rc::Rc, sync::Arc};

/// Work a dialog finishes later.
pub type Pending<T> = Pin<Box<dyn Future<Output = T>>>;

/// What a file dialog shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePicker {
    /// The dialog's title.
    pub title: String,
    /// Where it opens.
    pub directory: Option<PathBuf>,
    /// The name a save dialog suggests.
    pub file_name: Option<String>,
    /// Filters: a name and its extensions, without the dot.
    pub filters: Vec<(String, Vec<String>)>,
    /// An open dialog lets the user pick several files.
    pub multiple: bool,
}

/// The clipboard, the system's URL handler and file dialogs, as a desktop
/// host provides them (see the module docs).
pub trait Platform {
    /// The clipboard's text, if it holds text.
    fn clipboard_text(&self) -> Option<String>;
    /// Put `text` on the clipboard; whether it took it.
    fn set_clipboard_text(&self, text: String) -> bool;
    /// Open `url` with the system's handler for it (a browser for
    /// `https:`); whether the request was made. None opens nothing.
    fn open_url(&self, url: &str) -> bool {
        let _ = url;
        false
    }
    /// The misspelled words in `text`, as UTF-8 byte ranges, by the system's
    /// spelling checker (`View::spellcheck`). None checks nothing.
    fn misspellings(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let _ = text;
        Vec::new()
    }
    /// The checker's suggestions for misspelled `word`, best first.
    fn spelling_guesses(&self, word: &str) -> Vec<String> {
        let _ = word;
        Vec::new()
    }
    /// Add `word` to the user's dictionary (the menu's Learn Spelling).
    fn learn_spelling(&self, word: &str) {
        let _ = word;
    }
    /// Stop flagging `word` for the rest of the session (Ignore Spelling).
    fn ignore_spelling(&self, word: &str) {
        let _ = word;
    }
    /// Whether the platform looks words up (`look_up`): macOS's dictionary
    /// panel. A context menu offers Look Up where it does.
    fn looks_up(&self) -> bool {
        false
    }
    /// Show the dictionary's entry for `text` over it, as AppKit's Look Up
    /// does: `baseline` is where its first character's baseline starts, in
    /// the window's points, and `size` its font size. Whether it showed.
    fn look_up(&self, text: &str, baseline: (f32, f32), size: f32) -> bool {
        let _ = (text, baseline, size);
        false
    }
    /// Put `text` on the clipboard with `html`, its rich form, where the
    /// platform takes both; plain text alone otherwise.
    fn set_clipboard_rich_text(&self, text: String, html: String) -> bool {
        let _ = html;
        self.set_clipboard_text(text)
    }
    /// The clipboard's image, if it holds one: a screenshot, an image
    /// editor's copy, a browser's Copy Image. Later, as decoding a large one
    /// takes a frame or more: a desktop host reads it off the UI thread.
    /// None reads none.
    fn clipboard_image(&self) -> Pending<Option<crate::image::ImageData>> {
        Box::pin(async { None })
    }
    /// Put `image` on the clipboard in the formats the platform's apps paste
    /// (its transform left out), encoded off the UI thread as reading is;
    /// whether it took it. None takes none.
    fn set_clipboard_image(&self, image: Arc<crate::image::ImageData>) -> Pending<bool> {
        let _ = image;
        Box::pin(async { false })
    }
    /// The files the user picks to open: none if they cancel.
    fn open_files(&self, picker: FilePicker) -> Pending<Result<Vec<PathBuf>, String>>;
    /// Where the user chooses to save: none if they cancel.
    fn save_file(&self, picker: FilePicker) -> Pending<Result<Option<PathBuf>, String>>;
}

/// The platform, as a service components find.
#[derive(Clone)]
pub struct PlatformService(pub Rc<dyn Platform>);
