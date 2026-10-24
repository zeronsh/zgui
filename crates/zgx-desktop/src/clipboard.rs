//! Clipboard selection follows the actual native display, never environment guesses.
//!
//! Wayland uses the application's data worker (`wayland_data`), which owns the
//! connection's only data devices; X11, macOS and Windows retain arboard. The primary
//! selection (middle-click paste) exists on X11 and Wayland only.
use std::error::Error;
use winit::event_loop::OwnedDisplayHandle;
#[cfg(target_os = "linux")]
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

enum Backend {
    Native(arboard::Clipboard),
    #[cfg(target_os = "linux")]
    Wayland(crate::wayland_data::WaylandClipboard),
}

pub(crate) struct Clipboard {
    backend: Backend,
}

impl Clipboard {
    /// `wayland` is the data worker's clipboard, required on a Wayland display.
    pub(crate) fn new(
        display: OwnedDisplayHandle,
        #[cfg(target_os = "linux")] wayland: Option<crate::wayland_data::WaylandClipboard>,
    ) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if let RawDisplayHandle::Wayland(_) = display.display_handle()?.as_raw() {
            let wayland = wayland.ok_or("the Wayland data worker is unavailable")?;
            return Ok(Self {
                backend: Backend::Wayland(wayland),
            });
        }
        let _ = display;
        Ok(Self {
            backend: Backend::Native(arboard::Clipboard::new()?),
        })
    }

    /// Put `text` on the clipboard with `html`, its rich form, beside it
    /// where the platform takes HTML (macOS, Windows, X11); text alone on
    /// Wayland.
    pub(crate) fn set_rich_text(&mut self, text: String, html: String) -> Result<()> {
        match &mut self.backend {
            Backend::Native(clipboard) => clipboard.set_html(html, Some(text))?,
            #[cfg(target_os = "linux")]
            Backend::Wayland(clipboard) => clipboard.store_rich(text, html)?,
        }
        Ok(())
    }

    pub(crate) fn get_text(&mut self) -> Result<String> {
        match &mut self.backend {
            Backend::Native(clipboard) => Ok(clipboard.get_text()?),
            #[cfg(target_os = "linux")]
            Backend::Wayland(clipboard) => Ok(clipboard.load(false)?),
        }
    }

    /// Wayland publication is asynchronous: success means the selection was
    /// offered, not that another process has accepted or persisted its data.
    pub(crate) fn set_text(&mut self, text: String) -> Result<()> {
        match &mut self.backend {
            Backend::Native(clipboard) => clipboard.set_text(text)?,
            #[cfg(target_os = "linux")]
            Backend::Wayland(clipboard) => clipboard.store(false, text)?,
        }
        Ok(())
    }

    /// The primary selection (X11, Wayland): the text last selected
    /// anywhere, which a middle click pastes.
    #[cfg(target_os = "linux")]
    pub(crate) fn get_primary_text(&mut self) -> Result<String> {
        match &mut self.backend {
            Backend::Native(clipboard) => {
                use arboard::{GetExtLinux, LinuxClipboardKind};
                Ok(clipboard
                    .get()
                    .clipboard(LinuxClipboardKind::Primary)
                    .text()?)
            }
            Backend::Wayland(clipboard) => Ok(clipboard.load(true)?),
        }
    }

    /// Publish the primary selection (X11, Wayland).
    #[cfg(target_os = "linux")]
    pub(crate) fn set_primary_text(&mut self, text: String) -> Result<()> {
        match &mut self.backend {
            Backend::Native(clipboard) => {
                use arboard::{LinuxClipboardKind, SetExtLinux};
                clipboard
                    .set()
                    .clipboard(LinuxClipboardKind::Primary)
                    .text(text)?;
            }
            Backend::Wayland(clipboard) => clipboard.store(true, text)?,
        }
        Ok(())
    }
}
