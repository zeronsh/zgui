//! Native Linux/macOS/Windows host with GPU rendering, accessibility, clipboard,
//! and IME. A software raster backend is available for reference and headless tests.
//!
//! ```no_run
//! use zgui::compose::prelude::*;
//! use zgui_desktop::{Application, WindowOptions};
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     Application::new().window(WindowOptions {
//!         title: "Example".into(), ..Default::default()
//!     }).run(|cx| {
//!         cx.render(component(|cx| {
//!             let title = cx.state(String::from("Ready"));
//!             let displayed = title.clone();
//!             column().p(24.0).gap(12.0)
//!                 .child(text_signal(move || displayed.get()))
//!                 .child(button().w(140.0).h(36.0).child("Update")
//!                     .on_click(move || { title.set(String::from("Updated")); }))
//!         }));
//!     })
//! }
//! ```
pub mod app_menu;
mod clipboard;
pub mod file_dialog;
#[cfg(target_os = "macos")]
mod message_pump_macos;
mod native_events;
mod native_prompt;
#[cfg(target_os = "linux")]
mod portal_dialog;
#[cfg(target_os = "linux")]
mod portal_parent;
mod url;
#[cfg(target_os = "linux")]
mod wayland_files;
mod window_info;
pub use app_menu::{
    AppMenu, AppMenuEntry, AppMenuError, MenuAccelerator, MenuAction, app_menu_bar,
    has_native_app_menu,
};
pub use file_dialog::{
    FileDialogError, FileDialogOptions, FileDialogResult, FileDialogs, FileFilter,
};
pub use native_events::{ApplicationEvent, ApplicationEventError};
pub use native_prompt::{PromptButtons, PromptLevel, PromptOptions, PromptResponse};
pub use url::{OpenUrlError, open_url};
pub use window_info::{DisplayInfo, DisplaySelector, WindowBounds, WindowCapabilities, WindowInfo};
mod focus_memory;
mod frame_source;
mod native_ime;
pub mod presentation;
pub mod raster;

pub mod accessibility;

pub mod application;
pub use application::{
    Application, TaskSpawner, WindowContext, WindowFactory, WindowHandle, WindowOptions,
};

mod scoped_task;
mod scroll_resample;
pub use scoped_task::ScopedTask;

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
mod readme_examples {}

mod presentation_retry;

#[cfg(doctest)]
#[doc = include_str!("../../../docs/window-controls.md")]
mod window_control_examples {}

pub use zgui_gpu::text::FontData;

mod file_drop;
mod gpu_stats;
