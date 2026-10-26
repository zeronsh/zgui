//! Window-owned native file selection. Selecting a path never reads or writes it.
use std::{
    cell::{Cell, RefCell},
    fmt,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Weak},
};
use std::{
    future::Future,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};
use winit::window::Window;

/// An extension filter, without a leading dot (for example `png` or `rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    /// What the picker calls the filter (`Images`).
    pub name: String,
    /// The extensions it accepts.
    pub extensions: Vec<String>,
}
impl FileFilter {
    /// A filter called `name` accepting `extensions`.
    pub fn new(
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
        }
    }
}
/// Common settings for opening files or folders and choosing a save destination.
/// Filters affect files only; extension/overwrite behavior follows the native dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDialogOptions {
    /// The dialog's title.
    pub title: String,
    /// The folder it starts in.
    pub directory: Option<PathBuf>,
    /// The file name a save dialog suggests.
    pub file_name: Option<String>,
    /// Which files can be chosen; none means any.
    pub filters: Vec<FileFilter>,
}
impl FileDialogOptions {
    /// Options titled `title`, with no starting folder, name or filters.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }
    /// Start in `directory`.
    pub fn directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }
    /// Suggest `file_name` for saving.
    pub fn file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }
    /// Add `filter`.
    pub fn filter(mut self, filter: FileFilter) -> Self {
        self.filters.push(filter);
        self
    }
    fn validate(&self) -> Result<(), FileDialogError> {
        if self.title.contains('\0')
            || self.file_name.as_ref().is_some_and(|s| s.contains('\0'))
            || self.filters.iter().any(|f| {
                f.name.contains('\0')
                    || f.extensions.is_empty()
                    || f.extensions
                        .iter()
                        .any(|s| s.contains(['\0', '/', '\\']) || s.starts_with('.'))
            })
        {
            return Err(FileDialogError::InvalidOptions);
        }
        Ok(())
    }
}
/// Why a native dialog or prompt could not be shown or answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogError {
    /// The owning window has closed.
    WindowClosed,
    /// The native window is not created yet, or is currently suspended.
    WindowUnavailable,
    /// A title, file name or filter holds a NUL, or a filter's extension is
    /// empty, has a path separator or starts with a dot.
    InvalidOptions,
    /// The native worker could not be started or terminated unexpectedly.
    BackendUnavailable,
}
impl fmt::Display for FileDialogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WindowClosed => "the dialog's window is closed",
            Self::WindowUnavailable => "the dialog's native window is unavailable",
            Self::InvalidOptions => "invalid native file dialog options",
            Self::BackendUnavailable => "the native file dialog backend is unavailable",
        })
    }
}
impl std::error::Error for FileDialogError {}
/// `None` means no selection. Linux reports portal failures as errors; macOS and
/// Windows may also return no selection when a native picker is unavailable.
pub type FileDialogResult<T> = Result<Option<T>, FileDialogError>;
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    OpenFile,
    OpenFiles,
    Folder,
    Folders,
    Save,
}

/// Cloneable service for one window. Invoke from its UI executor after native creation.
/// Linux uses an owned XDG Desktop Portal request; macOS uses native sheets;
/// Windows uses native file dialogs owned by the window.
///
/// On Linux, dropping a file-picker future or closing its owner requests portal
/// dismissal and releases the native parent. On macOS, closing or suspending the
/// owner dismisses its native sheets, including prompts. Dropping an individual
/// macOS request future cancels delivery; its sheet remains until dismissed.
/// On Windows, cancellation invalidates delivery but does not dismiss the native
/// dialog. Its parent remains alive until the dialog is dismissed.
#[derive(Clone)]
pub struct FileDialogs {
    owner: crate::WindowHandle,
    native: Rc<RefCell<Option<Weak<Window>>>>,
    /// The event loop's display, which a dialog's worker holds while it
    /// uses the window's (`portal_parent`).
    #[cfg(target_os = "linux")]
    display: Rc<RefCell<Option<winit::event_loop::OwnedDisplayHandle>>>,
    generation: Rc<Cell<u64>>,
    cancellations: Rc<RefCell<Vec<Weak<Cancel>>>>,
}
impl FileDialogs {
    /// The window they belong to, while it is open.
    #[cfg(target_os = "macos")]
    pub(crate) fn window(&self) -> Option<std::sync::Arc<Window>> {
        self.native.borrow().as_ref()?.upgrade()
    }
    pub(crate) fn new(owner: crate::WindowHandle) -> Self {
        Self {
            owner,
            native: Rc::new(RefCell::new(None)),
            #[cfg(target_os = "linux")]
            display: Rc::default(),
            generation: Rc::new(Cell::new(0)),
            cancellations: Rc::new(RefCell::new(Vec::new())),
        }
    }
    /// The event loop's display, for the dialogs' workers to hold.
    #[cfg(target_os = "linux")]
    pub(crate) fn set_display(&self, display: winit::event_loop::OwnedDisplayHandle) {
        *self.display.borrow_mut() = Some(display);
    }
    pub(crate) fn attach(&self, window: Option<&Arc<Window>>) {
        #[cfg(target_os = "macos")]
        let old_parent = self.native.borrow().as_ref().and_then(Weak::upgrade);
        for token in self
            .cancellations
            .borrow_mut()
            .drain(..)
            .filter_map(|w| w.upgrade())
        {
            token.cancel();
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        *self.native.borrow_mut() = window.map(Arc::downgrade);
        // rfd does not dismiss sheets when their futures are dropped. Invalidate
        // requests and release RefCell borrows before AppKit completion callbacks.
        #[cfg(target_os = "macos")]
        if let Some(parent) = old_parent {
            dismiss_owned_sheets(&parent);
        }
    }
    async fn select(
        &self,
        kind: Kind,
        options: FileDialogOptions,
    ) -> FileDialogResult<Vec<PathBuf>> {
        options.validate()?;
        if self.owner.is_closed() {
            return Err(FileDialogError::WindowClosed);
        }
        let parent = self
            .native
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or(FileDialogError::WindowUnavailable)?;
        let generation = self.generation.get();
        let cancel = Arc::new(Cancel::new());
        self.cancellations
            .borrow_mut()
            .retain(|w| w.strong_count() > 0);
        self.cancellations
            .borrow_mut()
            .push(Arc::downgrade(&cancel));
        let _cancel_on_drop = CancelOnDrop(cancel.clone());
        #[cfg(target_os = "linux")]
        let result = {
            let display = self.display.borrow().clone();
            native_select(kind, options, parent, display, cancel).await?
        };
        #[cfg(not(target_os = "linux"))]
        let result = native_select(kind, options, parent, cancel).await?;
        if self.owner.is_closed() {
            return Err(FileDialogError::WindowClosed);
        }
        if generation != self.generation.get() {
            return Err(FileDialogError::WindowUnavailable);
        }
        Ok(result)
    }
    /// Show a native prompt; cancellation and owner-generation rules match file selection.
    pub async fn prompt(
        &self,
        options: crate::PromptOptions,
    ) -> Result<crate::PromptResponse, FileDialogError> {
        options.validate()?;
        if self.owner.is_closed() {
            return Err(FileDialogError::WindowClosed);
        }
        let parent = self
            .native
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or(FileDialogError::WindowUnavailable)?;
        let generation = self.generation.get();
        let response = crate::native_prompt::show(parent, options).await?;
        if self.owner.is_closed() {
            return Err(FileDialogError::WindowClosed);
        }
        if generation != self.generation.get() {
            return Err(FileDialogError::WindowUnavailable);
        }
        Ok(response)
    }
    /// Ask for one file to open.
    pub async fn open_file(&self, options: FileDialogOptions) -> FileDialogResult<PathBuf> {
        Ok(self
            .select(Kind::OpenFile, options)
            .await?
            .and_then(|p| p.into_iter().next()))
    }
    /// Ask for any number of files to open.
    pub async fn open_files(&self, options: FileDialogOptions) -> FileDialogResult<Vec<PathBuf>> {
        self.select(Kind::OpenFiles, options).await
    }
    /// Ask for one folder.
    pub async fn open_folder(&self, options: FileDialogOptions) -> FileDialogResult<PathBuf> {
        Ok(self
            .select(Kind::Folder, options)
            .await?
            .and_then(|p| p.into_iter().next()))
    }
    /// Ask for any number of folders.
    pub async fn open_folders(&self, options: FileDialogOptions) -> FileDialogResult<Vec<PathBuf>> {
        self.select(Kind::Folders, options).await
    }
    /// Choose a destination; this does not create, truncate, or write a file.
    pub async fn save_file(&self, options: FileDialogOptions) -> FileDialogResult<PathBuf> {
        Ok(self
            .select(Kind::Save, options)
            .await?
            .and_then(|p| p.into_iter().next()))
    }
}

#[cfg(target_os = "macos")]
fn dismiss_owned_sheets(parent: &Window) {
    use objc2::{MainThreadMarker, rc::autoreleasepool};
    use objc2_app_kit::{NSModalResponseCancel, NSView, NSWindow};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // FileDialogs and its host are UI-thread confined (Rc); AppKit still gets an
    // explicit thread check before dereferencing the native view.
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };
    let Ok(handle) = parent.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    fn dismiss(owner: &NSWindow) {
        // Snapshot includes queued sheets, but not nested sheets (for example a
        // save overwrite confirmation). Dismiss those children before their panel.
        for sheet in owner.sheets() {
            dismiss(&sheet);
            owner.endSheet_returnCode(&sheet, NSModalResponseCancel);
            sheet.orderOut(None);
        }
    }
    autoreleasepool(|_| {
        // SAFETY: The winit Window and borrowed handle keep this NSView alive;
        // winit hands out an AppKit handle only on the main thread.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        if let Some(owner) = view.window() {
            dismiss(&owner);
        }
    });
}

/// A request's cancellation: its future dropped or its window gone. On
/// Linux it also wakes the thread waiting on the portal's answer.
pub(crate) struct Cancel {
    cancelled: std::sync::atomic::AtomicBool,
    #[cfg(target_os = "linux")]
    wake: Option<crate::linux_dbus::Stop>,
}
impl Cancel {
    fn new() -> Self {
        Self {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            #[cfg(target_os = "linux")]
            wake: crate::linux_dbus::Stop::new(),
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        #[cfg(target_os = "linux")]
        if let Some(wake) = &self.wake {
            wake.stop();
        }
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
    /// What a thread waiting on the portal sleeps on besides it.
    #[cfg(target_os = "linux")]
    pub(crate) fn wake(&self) -> Option<&crate::linux_dbus::Stop> {
        self.wake.as_ref()
    }
}
struct CancelOnDrop(Arc<Cancel>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
#[cfg(target_os = "linux")]
async fn native_select(
    kind: Kind,
    options: FileDialogOptions,
    parent: Arc<Window>,
    display: Option<winit::event_loop::OwnedDisplayHandle>,
    cancel: Arc<Cancel>,
) -> FileDialogResult<Vec<PathBuf>> {
    background(move || crate::portal_dialog::select(kind, options, parent, display, cancel))?
        .await?
}
/// A window's handle, taken on its UI thread, for a worker to own a native
/// dialog with: winit gives raw handles on its own thread only, so rfd,
/// asking on the worker, got none and showed every dialog unowned (not
/// modal to the window, at the screen's corner). It keeps the window alive
/// until the dialog returns, even when the UI future is cancelled.
#[cfg(target_os = "windows")]
pub(crate) struct Owner {
    hwnd: std::num::NonZeroIsize,
    _window: Arc<Window>,
}
#[cfg(target_os = "windows")]
impl Owner {
    pub(crate) fn new(window: Arc<Window>) -> Option<Self> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        Some(Self {
            hwnd: handle.hwnd,
            _window: window,
        })
    }
}
#[cfg(target_os = "windows")]
impl winit::raw_window_handle::HasWindowHandle for Owner {
    fn window_handle(
        &self,
    ) -> Result<winit::raw_window_handle::WindowHandle<'_>, winit::raw_window_handle::HandleError>
    {
        let raw = winit::raw_window_handle::Win32WindowHandle::new(self.hwnd);
        // SAFETY: the window this handle names is kept alive by `_window`.
        Ok(unsafe { winit::raw_window_handle::WindowHandle::borrow_raw(raw.into()) })
    }
}
#[cfg(target_os = "windows")]
impl winit::raw_window_handle::HasDisplayHandle for Owner {
    fn display_handle(
        &self,
    ) -> Result<winit::raw_window_handle::DisplayHandle<'_>, winit::raw_window_handle::HandleError>
    {
        Ok(winit::raw_window_handle::DisplayHandle::windows())
    }
}
#[cfg(target_os = "windows")]
async fn native_select(
    kind: Kind,
    options: FileDialogOptions,
    parent: Arc<Window>,
    _cancel: Arc<Cancel>,
) -> FileDialogResult<Vec<PathBuf>> {
    let owner = Owner::new(parent);
    background(move || {
        let mut dialog = rfd::FileDialog::new().set_title(options.title);
        if let Some(owner) = &owner {
            dialog = dialog.set_parent(owner);
        }
        if let Some(directory) = options.directory {
            dialog = dialog.set_directory(directory);
        }
        if let Some(name) = options.file_name {
            dialog = dialog.set_file_name(name);
        }
        for filter in options.filters {
            dialog = dialog.add_filter(filter.name, &filter.extensions);
        }
        match kind {
            Kind::OpenFile => dialog.pick_file().map(|p| vec![p]),
            Kind::OpenFiles => dialog.pick_files(),
            Kind::Folder => dialog.pick_folder().map(|p| vec![p]),
            Kind::Folders => dialog.pick_folders(),
            Kind::Save => dialog.save_file().map(|p| vec![p]),
        }
    })?
    .await
}
#[cfg(target_os = "macos")]
async fn native_select(
    kind: Kind,
    options: FileDialogOptions,
    parent: Arc<Window>,
    _cancel: Arc<Cancel>,
) -> FileDialogResult<Vec<PathBuf>> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_parent(parent.as_ref())
        .set_title(options.title);
    if let Some(directory) = options.directory {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = options.file_name {
        dialog = dialog.set_file_name(name);
    }
    for filter in options.filters {
        dialog = dialog.add_filter(filter.name, &filter.extensions);
    }
    let result = match kind {
        Kind::OpenFile => dialog.pick_file().await.map(|p| vec![p]),
        Kind::OpenFiles => dialog.pick_files().await,
        Kind::Folder => dialog.pick_folder().await.map(|p| vec![p]),
        Kind::Folders => dialog.pick_folders().await,
        Kind::Save => dialog.save_file().await.map(|p| vec![p]),
    };
    Ok(result.map(|files| files.into_iter().map(|f| f.path().to_owned()).collect()))
}

struct Completion<T> {
    result: Option<Result<T, FileDialogError>>,
    waker: Option<Waker>,
}
pub(crate) struct Background<T>(Arc<Mutex<Completion<T>>>);
pub(crate) fn background<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<Background<T>, FileDialogError> {
    let state = Arc::new(Mutex::new(Completion {
        result: None,
        waker: None,
    }));
    let sender = state.clone();
    std::thread::Builder::new()
        .name("zgx-file-dialog".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .map_err(|_| FileDialogError::BackendUnavailable);
            let waker = {
                let mut state = sender.lock().unwrap();
                state.result = Some(result);
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        })
        .map_err(|_| FileDialogError::BackendUnavailable)?;
    Ok(Background(state))
}
impl<T> Future for Background<T> {
    type Output = Result<T, FileDialogError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().unwrap();
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// The portal's thread sleeps until the answer, or until the request is
    /// cancelled, which wakes it at once.
    #[cfg(target_os = "linux")]
    #[test]
    fn cancelling_wakes_the_thread_waiting_on_the_portal() {
        let Ok(connection) = crate::linux_dbus::session() else {
            return; // No session bus here.
        };
        let cancel = Arc::new(Cancel::new());
        let waiting = cancel.clone();
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            let wake = waiting.wake().unwrap();
            while crate::linux_dbus::wait(&connection, wake) {}
            started.elapsed()
        });
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!thread.is_finished(), "nothing came and nothing cancelled");
        cancel.cancel();
        assert!(cancel.is_cancelled());
        assert!(thread.join().unwrap() < std::time::Duration::from_secs(5));
    }
    #[test]
    fn invalid_filter_does_not_reach_native_cstring_conversion() {
        for filter in [
            FileFilter::new("bad\0name", ["rs"]),
            FileFilter::new("Rust", ["r\0s"]),
            FileFilter::new("Rust", [".rs"]),
        ] {
            assert_eq!(
                FileDialogOptions::default().filter(filter).validate(),
                Err(FileDialogError::InvalidOptions)
            );
        }
        assert!(
            FileDialogOptions::new("Open")
                .filter(FileFilter::new("Rust", ["rs", "toml"]))
                .validate()
                .is_ok()
        );
    }
    #[test]
    fn cancelled_receiver_releases_worker_owned_resources_after_completion() {
        let retained = Arc::new(());
        let worker = retained.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let future = background(move || {
            let _keep = worker;
            rx.recv().unwrap();
        })
        .unwrap();
        drop(future);
        assert_eq!(Arc::strong_count(&retained), 2);
        tx.send(()).unwrap();
        for _ in 0..100 {
            if Arc::strong_count(&retained) == 1 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("native worker retained its parent after completion");
    }
}
