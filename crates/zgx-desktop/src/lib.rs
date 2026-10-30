//! Native Linux/macOS/Windows host with GPU rendering, accessibility, clipboard,
//! and IME. A software raster backend is available for reference and headless tests.
//!
//! ```no_run
//! use zgx::compose::prelude::*;
//! use zgx_desktop::{Application, WindowOptions};
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

#![warn(missing_docs)]
/// The `mimalloc` feature's allocator for the whole program.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
/// mimalloc maps its memory with VM tag 100 unless told otherwise, and
/// macOS's tools (vmmap, footprint, Activity Monitor) name tag 100
/// "IOAccelerator": the program's heap read as GPU memory. Before main, so
/// before mimalloc maps anything, its default becomes tag 240, the first
/// tag for an application's own memory ("Memory Tag 240"); the
/// `MIMALLOC_OS_TAG` variable still overrides it.
#[cfg(all(feature = "mimalloc", target_os = "macos"))]
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static MIMALLOC_OS_TAG: extern "C" fn() = {
    unsafe extern "C" {
        // mimalloc's own (mimalloc.h), in the library the `mimalloc` crate links.
        fn mi_option_set_default(option: std::ffi::c_int, value: std::ffi::c_long);
    }
    /// `mi_option_os_tag` in mimalloc.h's `mi_option_e` (v2 and v3).
    const OS_TAG: std::ffi::c_int = 18;
    extern "C" fn tag() {
        // SAFETY: mimalloc's option setter, with an option it defines and a
        // tag in its range (100-255); it only stores the default.
        unsafe { mi_option_set_default(OS_TAG, 240) };
    }
    tag
};
#[cfg(all(test, feature = "mimalloc", target_os = "macos"))]
mod mimalloc_tag_tests {
    /// vmmap names the program's heap by the tag, not "IOAccelerator"
    /// (`cargo test -p zgx-desktop --features mimalloc --lib mimalloc_tag`).
    #[test]
    fn the_heap_is_tagged_as_the_programs_own() {
        let held: Vec<Box<[u8]>> = (0..64)
            .map(|_| vec![1u8; 128 * 1024].into_boxed_slice())
            .collect();
        let summary = std::process::Command::new("/usr/bin/vmmap")
            .args(["--summary", &std::process::id().to_string()])
            .output()
            .expect("vmmap runs");
        let summary = String::from_utf8_lossy(&summary.stdout);
        drop(held);
        assert!(summary.contains("Memory Tag 240"), "{summary}");
    }
}
pub mod app_menu;
mod clipboard;
mod composition;
mod contain;
pub mod file_dialog;
#[cfg(target_os = "linux")]
mod linux_dbus;
#[cfg(target_os = "linux")]
mod linux_icon;
#[cfg(target_os = "linux")]
mod linux_spelling;
#[cfg(target_os = "linux")]
mod linux_zenity;
mod local_time;
#[cfg(target_os = "macos")]
mod look_up;
mod native_drag;
mod native_events;
mod native_prompt;
mod notify;
mod platform;
mod popup_window;
#[cfg(target_os = "linux")]
mod portal_dialog;
#[cfg(target_os = "linux")]
mod portal_parent;
mod scrollers;
// Both answer the vendored winit's hooks (`zgx_winit_patched`); with
// crates.io's, Wayland's drags alone reach `text_drop`, through
// `wayland_data`.
#[cfg(all(target_os = "macos", zgx_winit_patched))]
mod services;
#[cfg(all(target_os = "macos", zgx_winit_patched))]
mod swipes;
#[cfg(any(
    all(any(target_os = "macos", windows), zgx_winit_patched),
    target_os = "linux"
))]
mod text_drop;
mod thumbnail;
mod url;
#[cfg(target_os = "linux")]
mod wayland_data;
mod window_chrome;
mod window_info;
mod window_place;
mod zoom_store;
pub use window_place::Tabbing;
#[cfg(target_os = "linux")]
mod x11_nudge;
#[cfg(target_os = "linux")]
mod x11_settings;
#[cfg(target_os = "linux")]
mod x11_startup;
#[cfg(target_os = "linux")]
mod x11_visual;
#[cfg(target_os = "linux")]
mod x11_work_area;
pub use app_menu::{
    AppMenu, AppMenuEntry, AppMenuError, MenuAccelerator, MenuAction, StandardItem, app_menu_bar,
    has_native_app_menu,
};
pub use composition::{CompositionLayer, WindowComposition};
pub use file_dialog::{
    FileDialogError, FileDialogOptions, FileDialogResult, FileDialogs, FileFilter,
};
pub use native_events::{ApplicationEvent, ApplicationEventError};
pub use native_prompt::{PromptButtons, PromptLevel, PromptOptions, PromptResponse};
pub use url::{OpenUrlError, open_url};
pub use window_chrome::{Glass, GlassMaterial, Titlebar};
pub use window_info::{
    DisplayInfo, DisplaySelector, WindowBounds, WindowCapabilities, WindowInfo, WindowLevel,
};
mod native_host;
#[cfg(target_os = "macos")]
pub use native_host::{BackdropReport, NativeDiagnostics, NativeViewReport};
pub use zgx::web_view::{WebProfile, WebView, WebViewEvent, WebViewOptions, WebViewState};
mod focus_memory;
mod frame_source;
mod native_ime;
// Windows' Text Services Framework: the document its input methods read.
pub mod presentation;
pub mod raster;
pub mod terminal_process;
#[cfg(any(windows, test))]
mod text_store;
#[cfg(windows)]
mod tsf;
pub use terminal_process::{
    ForegroundProcess, resolve_terminal_processes, terminal_processes_changed,
    terminal_processes_pushed,
};

pub mod accessibility;

pub mod application;
pub use application::{
    Application, TaskSpawner, WindowContext, WindowFactory, WindowHandle, WindowOptions,
};

#[cfg(windows)]
mod input_pane;
#[cfg(target_os = "linux")]
mod linux_appearance;
mod scoped_task;
mod scroll_resample;
mod system_colors;
mod touch;
mod wheel_tape;
pub use scoped_task::ScopedTask;

#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
mod readme_examples {}

#[cfg(doctest)]
#[doc = include_str!("../../../docs/getting-started.md")]
mod getting_started_examples {}

mod presentation_retry;

#[cfg(doctest)]
#[doc = include_str!("../../../docs/window-controls.md")]
mod window_control_examples {}

pub use zgx_gpu::text::FontData;
/// GPU views: an app's own wgpu drawing in a box (`zgx::compose::gpu_view`),
/// with the wgpu the renderer draws with.
pub use zgx_gpu::{viewport, wgpu};

mod file_drop;
mod gpu_stats;
mod memory;
