//! What the window server composites of a window, read in the process: the
//! native window's opacity and background, each layer of its tree, and the
//! frame its layer shows (`WindowHandle::debug_composition`). No screen is
//! captured: these are the inputs the system composites over the desktop, so
//! a fixture checks transparency without the Screen Recording permission.

/// A window's composition as the platform holds it, for fixtures: on macOS
/// read from AppKit and Core Animation; elsewhere empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowComposition {
    /// Whether the native window says it is opaque (the window server then
    /// draws nothing behind it).
    pub window_opaque: bool,
    /// The alpha of the native window's background colour.
    pub window_background_alpha: f64,
    /// The window's layers, depth first in paint order from its frame view's.
    pub layers: Vec<CompositionLayer>,
    /// The frame the window's layer shows: width and height in pixels, then
    /// premultiplied BGRA rows. `None` before a frame shows.
    pub frame: Option<(u32, u32, Vec<u8>)>,
}

/// One layer of a window's tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositionLayer {
    /// Its class.
    pub class: String,
    /// How deep it is in the tree (the frame view's layer is 0).
    pub depth: usize,
    /// Whether it says it is opaque.
    pub opaque: bool,
    /// Its background colour's alpha (0 with none).
    pub background_alpha: f64,
    /// Its opacity.
    pub opacity: f32,
    /// Whether it is hidden.
    pub hidden: bool,
    /// Whether it is the content view's layer or under it (not the title
    /// bar's).
    pub in_content: bool,
    /// Whether its contents are the surface `frame` was read from.
    pub shows_frame: bool,
}

impl WindowComposition {
    /// The shown frame's pixel at `(x, y)` as premultiplied RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let (width, height, bgra) = self.frame.as_ref()?;
        if x >= *width || y >= *height {
            return None;
        }
        let at = ((y * width + x) * 4) as usize;
        let [b, g, r, a] = bgra.get(at..at + 4)?.try_into().ok()?;
        Some([r, g, b, a])
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn read(
    window: &winit::window::Window,
    renderer: Option<&zgx_gpu::GpuRenderer>,
) -> WindowComposition {
    use objc2::{msg_send, rc::Retained, runtime::AnyObject};
    use objc2_app_kit::NSView;
    use objc2_quartz_core::CALayer;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let mut out = WindowComposition::default();
    let Ok(handle) = window.window_handle() else {
        return out;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return out;
    };
    // SAFETY: winit's content view, alive while its window is (borrowed
    // here); winit hands out an AppKit handle only on the main thread.
    let Some(view) = (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }) else {
        return out;
    };
    let shown = renderer.and_then(|renderer| renderer.shown_frame());
    if let Some(ns_window) = view.window() {
        out.window_opaque = ns_window.isOpaque();
        out.window_background_alpha = ns_window.backgroundColor().alphaComponent();
    }
    let content = view.layer();
    let mut root = content.clone();
    while let Some(up) = root.as_ref().and_then(|layer| layer.superlayer()) {
        root = Some(up);
    }
    fn walk(
        layer: &CALayer,
        depth: usize,
        in_content: bool,
        content: Option<&CALayer>,
        surface: Option<*const std::ffi::c_void>,
        out: &mut Vec<CompositionLayer>,
    ) {
        let in_content = in_content || content.is_some_and(|c| std::ptr::eq(c, layer));
        // SAFETY: `contents` returns a nullable object (`id`), retained here.
        let contents: Option<Retained<AnyObject>> = unsafe { msg_send![layer, contents] };
        out.push(CompositionLayer {
            class: layer.class().name().to_string_lossy().into_owned(),
            depth,
            opaque: layer.isOpaque(),
            background_alpha: layer.backgroundColor().map_or(0., |color| {
                objc2_core_graphics::CGColor::alpha(Some(&color))
            }),
            opacity: layer.opacity(),
            hidden: layer.isHidden(),
            in_content,
            shows_frame: contents.zip(surface).is_some_and(|(contents, surface)| {
                std::ptr::eq((&*contents as *const AnyObject).cast(), surface)
            }),
        });
        // SAFETY: the layer's live sublayer list, only read while walked (on
        // the main thread).
        if let Some(sublayers) = unsafe { layer.sublayers() } {
            for sublayer in sublayers.iter() {
                walk(&sublayer, depth + 1, in_content, content, surface, out);
            }
        }
    }
    if let Some(root) = root {
        let surface = shown.as_ref().map(|frame| frame.surface_ptr());
        walk(
            &root,
            0,
            false,
            content.as_deref(),
            surface,
            &mut out.layers,
        );
    }
    out.frame = renderer.and_then(|renderer| renderer.shown_pixels());
    out
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn read(
    _window: &winit::window::Window,
    _renderer: Option<&zgx_gpu::GpuRenderer>,
) -> WindowComposition {
    WindowComposition::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_reads_bgra_rows_as_rgba_and_nothing_outside() {
        // 2x2: blue, half-alpha red / green, clear.
        let bgra = vec![255, 0, 0, 255, 0, 0, 128, 128, 0, 255, 0, 255, 0, 0, 0, 0];
        let composition = WindowComposition {
            frame: Some((2, 2, bgra)),
            ..Default::default()
        };
        assert_eq!(composition.pixel(0, 0), Some([0, 0, 255, 255]));
        assert_eq!(composition.pixel(1, 0), Some([128, 0, 0, 128]));
        assert_eq!(composition.pixel(0, 1), Some([0, 255, 0, 255]));
        assert_eq!(composition.pixel(1, 1), Some([0, 0, 0, 0]));
        assert_eq!(composition.pixel(2, 0), None);
        assert_eq!(WindowComposition::default().pixel(0, 0), None);
    }
}
