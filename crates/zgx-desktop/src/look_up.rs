//! macOS's Look Up (`NSView.showDefinitionForAttributedString:atPoint:`): the
//! dictionary's entry for a word, shown over it, as AppKit's text views show
//! it from their context menu and ⌃⌘D.
use objc2::{AnyThread, runtime::AnyObject};
use objc2_app_kit::{NSFont, NSFontAttributeName, NSView};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSString};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

/// Show `text`'s definition in `window`, its first character's baseline at
/// `baseline` (the window's points: winit's view is flipped, origin top
/// left), set in the system font at `size`; whether it was asked.
pub(crate) fn show(window: &Window, text: &str, baseline: (f32, f32), size: f32) -> bool {
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    // SAFETY: winit's AppKit handle is its live NSView, used here on the
    // main thread, as the event loop and its menus run.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let font = NSFont::systemFontOfSize(size.max(1.) as f64);
    // SAFETY: a static AppKit string constant.
    let key = unsafe { NSFontAttributeName };
    let attributes: objc2::rc::Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[key], &[&*font as &AnyObject]);
    // SAFETY: an attributed string of `text` with a font attribute.
    let string = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attributes),
        )
    };
    view.showDefinitionForAttributedString_atPoint(
        Some(&string),
        NSPoint::new(baseline.0 as f64, baseline.1 as f64),
    );
    true
}
