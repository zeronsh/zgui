//! What an app reaches on the machine beyond its window: the clipboard and
//! the file dialogs. A desktop host provides it to the components it renders
//! (`compose::provide`, as `Context::try_service::<PlatformService>()`);
//! headless runs have none, and tests provide a fake.
use std::{future::Future, path::PathBuf, pin::Pin, rc::Rc};

/// Work a dialog finishes later.
pub type Pending<T> = Pin<Box<dyn Future<Output = T>>>;

/// What a file dialog shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilePicker {
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
    /// Put `text` on the clipboard with `html`, its rich form, where the
    /// platform takes both; plain text alone otherwise.
    fn set_clipboard_rich_text(&self, text: String, html: String) -> bool {
        let _ = html;
        self.set_clipboard_text(text)
    }
    /// The files the user picks to open: none if they cancel.
    fn open_files(&self, picker: FilePicker) -> Pending<Result<Vec<PathBuf>, String>>;
    /// Where the user chooses to save: none if they cancel.
    fn save_file(&self, picker: FilePicker) -> Pending<Result<Option<PathBuf>, String>>;
}

/// The platform, as a service components find.
#[derive(Clone)]
pub struct PlatformService(pub Rc<dyn Platform>);
