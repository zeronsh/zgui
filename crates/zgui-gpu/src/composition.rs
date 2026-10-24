//! Presenting into DirectComposition visuals of the caller's (Windows).
//!
//! A window whose host composes its own visual tree (native views between
//! the planes: `zgui-desktop`'s native_host/windows.rs) hands each plane's
//! renderer a visual: the swap chain becomes its content, and the tree shows
//! a new one once the host's device commits. Frames themselves show at
//! present, as they do in the visual wgpu makes for a window of zgui alone.
use crate::{GpuError, GpuRenderer};
use std::sync::Arc;
use windows::{
    Win32::Graphics::DirectComposition::{IDCompositionDevice, IDCompositionVisual},
    core::Interface,
};

/// A visual a renderer presents into, and the device committing its tree.
pub(crate) struct Target {
    visual: IDCompositionVisual,
    device: IDCompositionDevice,
}
impl Target {
    pub fn surface(&self, instance: &wgpu::Instance) -> Result<wgpu::Surface<'static>, GpuError> {
        // SAFETY: `as_raw` is a live IDCompositionVisual (`self` holds a
        // reference) during the call, and wgpu keeps a reference of its own
        // for as long as the surface lives (`SurfaceTargetUnsafe::
        // CompositionVisual`).
        unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(
                self.visual.as_raw(),
            ))
        }
        .map_err(|e| GpuError(e.to_string()))
    }
    pub fn commit(&self) {
        // SAFETY: a live device of the caller's, on its thread.
        let _ = unsafe { self.device.Commit() };
    }
}

impl GpuRenderer {
    /// A window's renderer presenting into `visual` (its base plane) rather
    /// than a visual of wgpu's own for the window, sharing `context` when
    /// given. `device` made the visual; the swap chain shows in it from the
    /// device's next commit (made here).
    pub fn for_window_visual(
        window: Arc<winit::window::Window>,
        visual: IDCompositionVisual,
        device: IDCompositionDevice,
        context: Option<&crate::GpuContext>,
    ) -> Result<Self, GpuError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let owned;
        let context = match context {
            Some(context) => context,
            None => {
                owned = crate::GpuContext::new()?;
                &owned
            }
        };
        let target = Target { visual, device };
        let surface = target.surface(&context.inner.instance)?;
        let mut renderer = Self::create_on(
            Some(window),
            Some(surface),
            size.width,
            size.height,
            context,
        )?;
        target.commit();
        renderer.composition = Some(target);
        renderer.set_scale_factor(scale);
        Ok(renderer)
    }
    /// A renderer presenting into `visual`, above the window's native views
    /// (the overlay plane), sharing `context` with the window's renderer. It
    /// draws nothing until given a plane (`set_plane`) and its box
    /// (`set_plane_rect`), which sizes its swap chain.
    pub fn for_overlay_visual(
        visual: IDCompositionVisual,
        device: IDCompositionDevice,
        context: &crate::GpuContext,
        scale: f32,
    ) -> Result<Self, GpuError> {
        let target = Target { visual, device };
        let surface = target.surface(&context.inner.instance)?;
        let mut renderer = Self::create_on(None, Some(surface), 1, 1, context)?;
        target.commit();
        renderer.composition = Some(target);
        renderer.set_scale_factor(scale);
        Ok(renderer)
    }
}
