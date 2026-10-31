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

/// A notification the system shows (`Platform::notify`): in macOS's
/// Notification Center, as a Windows toast, through a Linux desktop's
/// notification server.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Notification {
    /// Its title, in bold.
    pub title: String,
    /// The text under it.
    pub body: String,
    /// One posted with the same tag replaces it (a conversation's newest
    /// message, not one for each).
    pub tag: Option<String>,
}
impl Notification {
    /// A notification of `title` and `body`.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            tag: None,
        }
    }
    /// One with the same tag replaces it.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }
}
/// Why a notification was not shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotifyError {
    /// This host or system shows none: why, and what would (an app
    /// bundle on macOS, an app identity on Windows).
    Unsupported(String),
    /// The person has not allowed this app's notifications, or turned them
    /// off.
    Denied(String),
    /// The system refused it, saying why.
    Failed(String),
}
impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotifyError::Unsupported(why) | NotifyError::Denied(why) | NotifyError::Failed(why) => {
                f.write_str(why)
            }
        }
    }
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
    /// What the system's substitutions would change in `text`, of the
    /// `kinds` asked for and the person has on (macOS's smart quotes and
    /// dashes, and their text replacements, in System Settings): each a
    /// UTF-8 byte range of `text` and what goes there. None changes nothing.
    fn substitutions(
        &self,
        text: &str,
        kinds: Substitutions,
    ) -> Vec<(std::ops::Range<usize>, String)> {
        let _ = (text, kinds);
        Vec::new()
    }
    /// The system's autocorrection for the word at `word` (a UTF-8 byte
    /// range of `text`, the rest its context), where the person has
    /// spelling correction on. None corrects nothing.
    fn correction(&self, text: &str, word: std::ops::Range<usize>) -> Option<String> {
        let _ = (text, word);
        None
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
    /// The files on the clipboard (copied in Finder, Explorer or a file
    /// manager), in order; none where it holds none. None reads none.
    fn clipboard_files(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
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
    /// Ask for a folder: the one the user picks, `None` if they cancel. A
    /// platform without a folder dialog refuses.
    fn open_folder(&self, picker: FilePicker) -> Pending<Result<Option<PathBuf>, String>> {
        let _ = picker;
        Box::pin(async { Err("no folder dialog here".into()) })
    }
    /// Show `notification`, settled once the system has it, or with why
    /// not. The first asks the person's permission where the system does
    /// (macOS). None shows none.
    fn notify(&self, notification: Notification) -> Pending<Result<(), NotifyError>> {
        let _ = notification;
        Box::pin(async {
            Err(NotifyError::Unsupported(
                "this host shows no system notifications".into(),
            ))
        })
    }
    /// The next notification the person clicks: its tag (as posted), the
    /// app brought forward first. None where clicks aren't told (a host
    /// without notifications).
    fn next_notification_click(&self) -> Pending<Option<String>> {
        Box::pin(async { None })
    }
    /// Put `badge` on the app's icon (the Dock's, the taskbar's, a Linux
    /// dock's), a count of what is unread; `None` takes it off. Whether the
    /// system shows one.
    fn set_badge(&self, badge: Option<&str>) -> bool {
        let _ = badge;
        false
    }
}

/// Which of the system's substitutions an editor asks for
/// (`Platform::substitutions`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Substitutions {
    /// Curly quotes and dashes for straight ones (`View::smart_punctuation`).
    pub punctuation: bool,
    /// The person's text replacements, "omw" for "On my way!"
    /// (`View::autocorrect`).
    pub replacements: bool,
}

/// The platform, as a service components find.
#[derive(Clone)]
pub struct PlatformService(pub Rc<dyn Platform>);
