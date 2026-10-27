//! Windows presentation through a swap chain of the renderer's own, whose
//! buffers are the retained frame (`ZGX_DXGI_PRESENT=0` presents through
//! wgpu's instead, for a release, to compare with).
//!
//! wgpu's swap chain discards a buffer's contents once shown, so presenting
//! through it means copying the whole retained frame into a buffer every
//! frame (`copy_present`): a copy, its barriers and a command list of its
//! own. A flip-sequential swap chain keeps what each buffer held, so the
//! retained frame can be the buffer itself, as macOS's surfaces are
//! (`layer_present`): each frame repaints what changed since that buffer was
//! last shown, presents the regions it changed (`Present1`'s dirty
//! rectangles, which the compositor composes alone), and copies nothing.
//!
//! The swap chain is made on wgpu's own Direct3D 12 queue and shown through
//! a DirectComposition visual of the window's, as wgpu's would be; wgpu's
//! surface stays unconfigured, made only to read the adapter's
//! capabilities. Its buffers are wrapped once as wgpu textures, which wgpu
//! tracks as any other: a frame moves its buffer to the present state
//! (`transition_resources`) before it is presented.
use crate::{FORMAT, GpuError, Scissor};
use windows::{
    Win32::{
        Foundation::{CloseHandle, DXGI_STATUS_OCCLUDED, HANDLE, HWND, RECT, WAIT_OBJECT_0},
        Graphics::{
            Direct3D12::{
                D3D12_FENCE_FLAG_NONE, ID3D12CommandQueue, ID3D12Device, ID3D12Fence,
                ID3D12Resource,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_IGNORE, DXGI_ALPHA_MODE_PREMULTIPLIED,
                    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT,
                DXGI_PRESENT_PARAMETERS, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
                DXGI_USAGE_SHADER_INPUT, IDXGIFactory2, IDXGISwapChain2, IDXGISwapChain3,
            },
        },
        System::Threading::WaitForSingleObjectEx,
    },
    core::{IUnknown, Interface},
};

/// Buffers in the swap chain: one shown, one drawn (as wgpu's).
const BUFFERS: u32 = 2;
/// Frames queued ahead of the compositor before a frame waits to start.
const LATENCY: u32 = 2;
/// `ZGX_DXGI_PRESENT_WHOLE`: each buffer presented whole, without dirty
/// rectangles. The compositor keeps its own copy of a window and takes only
/// the dirty rectangles from a present, so a buffer wrongly brought up to
/// date shows only when it reads one whole (as it may at any time); this
/// makes every present do so (`scripts/windows_present_probe.py`).
static WHOLE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| zgx::env::var_os("ZGX_DXGI_PRESENT_WHOLE").is_some());

/// A buffer of the swap chain, as wgpu draws into and reads it.
pub(crate) struct Buffer {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Reading it to bring another buffer up to date (copy_offset.wgsl).
    pub copy_source: std::cell::OnceCell<wgpu::BindGroup>,
}

/// What a swap chain presents on: wgpu's Direct3D 12 queue, or (d3d11_frame.rs)
/// a Direct3D 11 device.
enum Owner {
    /// wgpu's queue, which the swap chain presents on, and its device.
    D3d12 {
        queue: ID3D12CommandQueue,
        raw: ID3D12Device,
    },
    /// Direct3D 11 rotates a flip-model swap chain's buffers under their
    /// names: buffer 0 is always the one to draw, and of two, buffer 1 the
    /// one shown. `presents` counts them, standing in for Direct3D 12's
    /// buffer index (`current`); `views` are the two buffers', made with
    /// `buffers`.
    #[cfg(d3d11)]
    D3d11 {
        device: std::rc::Rc<crate::d3d11_frame::Device>,
        views: std::cell::RefCell<Option<[crate::d3d11_frame::Surface; 2]>>,
        presents: std::cell::Cell<usize>,
    },
}

/// The window's swap chain and the composition visual showing it.
pub(crate) struct Chain {
    device: wgpu::Device,
    owner: Owner,
    hwnd: HWND,
    swap_chain: IDXGISwapChain3,
    /// Signalled when a frame may start without queueing more than
    /// `LATENCY` frames ahead of the compositor.
    waitable: HANDLE,
    /// No buffer presented since it was made or resized: a present must
    /// cover all of it.
    whole: bool,
    /// Its buffers were handed out (`buffers`): a new size needs `resize`.
    wrapped: bool,
    // Kept alive while the swap chain is shown.
    _visual: IDCompositionVisual,
    _target: IDCompositionTarget,
    _dcomp: IDCompositionDevice,
}

impl Chain {
    /// A swap chain `width` by `height` for `hwnd`, opaque unless
    /// `transparent`, on `device`'s queue.
    pub fn new(
        device: &wgpu::Device,
        hwnd: HWND,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Self, GpuError> {
        // SAFETY: Reads the raw queue and device only; the guard is dropped before wgpu is
        // called again.
        let (queue, raw) = {
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }
                .ok_or_else(|| GpuError("not a Direct3D 12 device".into()))?;
            (hal.raw_queue().clone(), hal.raw_device().clone())
        };
        let creator: IUnknown = queue
            .cast()
            .map_err(|e| GpuError(format!("IUnknown: {e}")))?;
        Self::compose(
            device,
            Owner::D3d12 { queue, raw },
            &creator,
            hwnd,
            width,
            height,
            transparent,
        )
    }
    /// `new` on a Direct3D 11 device (d3d11_frame.rs); `device` is wgpu's
    /// no-op one, which the buffers' handles are made on.
    #[cfg(d3d11)]
    pub fn new_d3d11(
        device: &wgpu::Device,
        d3d11: &std::rc::Rc<crate::d3d11_frame::Device>,
        hwnd: HWND,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Self, GpuError> {
        let creator: IUnknown = d3d11
            .device
            .cast()
            .map_err(|e| GpuError(format!("IUnknown: {e}")))?;
        let owner = Owner::D3d11 {
            device: d3d11.clone(),
            views: Default::default(),
            presents: Default::default(),
        };
        Self::compose(device, owner, &creator, hwnd, width, height, transparent)
    }
    /// The swap chain made by `creator` (a queue or device of `owner`'s),
    /// shown through a visual of `hwnd`'s.
    fn compose(
        device: &wgpu::Device,
        owner: Owner,
        creator: &IUnknown,
        hwnd: HWND,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<Self, GpuError> {
        let fail = |what: &str, error: windows::core::Error| GpuError(format!("{what}: {error}"));
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT | DXGI_USAGE_SHADER_INPUT,
            BufferCount: BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: if transparent {
                DXGI_ALPHA_MODE_PREMULTIPLIED
            } else {
                DXGI_ALPHA_MODE_IGNORE
            },
            Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
        };
        // SAFETY: Plain DXGI and DirectComposition calls on objects made
        // here; `hwnd` is the live window the renderer presents to.
        unsafe {
            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
                .map_err(|e| fail("CreateDXGIFactory2", e))?;
            let swap_chain: IDXGISwapChain3 = factory
                .CreateSwapChainForComposition(creator, &desc, None)
                .map_err(|e| fail("CreateSwapChainForComposition", e))?
                .cast()
                .map_err(|e| fail("IDXGISwapChain3", e))?;
            let chain2: IDXGISwapChain2 =
                swap_chain.cast().map_err(|e| fail("IDXGISwapChain2", e))?;
            chain2
                .SetMaximumFrameLatency(LATENCY)
                .map_err(|e| fail("SetMaximumFrameLatency", e))?;
            let waitable = chain2.GetFrameLatencyWaitableObject();
            let dcomp: IDCompositionDevice =
                DCompositionCreateDevice(None).map_err(|e| fail("DCompositionCreateDevice", e))?;
            let target = dcomp
                .CreateTargetForHwnd(hwnd, false)
                .map_err(|e| fail("CreateTargetForHwnd", e))?;
            let visual = dcomp.CreateVisual().map_err(|e| fail("CreateVisual", e))?;
            visual
                .SetContent(&swap_chain)
                .map_err(|e| fail("SetContent", e))?;
            target.SetRoot(&visual).map_err(|e| fail("SetRoot", e))?;
            dcomp.Commit().map_err(|e| fail("Commit", e))?;
            Ok(Self {
                device: device.clone(),
                owner,
                hwnd,
                swap_chain,
                waitable,
                whole: true,
                wrapped: false,
                _visual: visual,
                _target: target,
                _dcomp: dcomp,
            })
        }
    }
    /// The swap chain's buffers, `width` by `height`, wrapped as wgpu
    /// textures. `release` them before `resize`, `remade` and the chain's
    /// drop.
    pub fn buffers(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Vec<Buffer>, GpuError> {
        let mut buffers = Vec::new();
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // Direct3D 11: views of buffers 0 and 1 (see `Owner::D3d11`), and
        // handles of the shell's standing for them.
        #[cfg(d3d11)]
        if let Owner::D3d11 {
            device: d3d11,
            views,
            ..
        } = &self.owner
        {
            let surface = |index: u32| {
                // SAFETY: A buffer of the swap chain, released before it
                // resizes or goes (`release`).
                let texture = unsafe { self.swap_chain.GetBuffer(index) }
                    .map_err(|e| GpuError(format!("swap chain buffer {index}: {e}")))?;
                // Buffer 1 and on are read-only: sampled alone.
                crate::d3d11_frame::Surface::of(&d3d11.device, texture, index == 0, true)
            };
            *views.borrow_mut() = Some([surface(0)?, surface(1)?]);
            for _ in 0..BUFFERS {
                let texture = crate::texture(device, width, height, "window buffer");
                let view = texture.create_view(&Default::default());
                buffers.push(Buffer {
                    texture,
                    view,
                    copy_source: Default::default(),
                });
            }
            self.wrapped = true;
            return Ok(buffers);
        }
        for index in 0..BUFFERS {
            // SAFETY: A buffer of the swap chain, which outlives the wrapper:
            // its owner releases the wrappers before `resize` and the chain.
            let resource: ID3D12Resource = unsafe { self.swap_chain.GetBuffer(index) }
                .map_err(|e| GpuError(format!("swap chain buffer {index}: {e}")))?;
            // SAFETY: The swap chain made this texture in `FORMAT`, this size,
            // one mip level and sample, usable as a render target and read by
            // shaders (`DXGI_USAGE_SHADER_INPUT`) and copies. wgpu takes it as
            // initialized: a buffer not yet drawn is caught up whole
            // (`begin_layer_frame`) before it shows.
            let texture = unsafe {
                let raw = wgpu::hal::dx12::Device::texture_from_raw(
                    resource,
                    FORMAT,
                    wgpu::TextureDimension::D2,
                    size,
                    1,
                    1,
                );
                device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
                    raw,
                    &wgpu::TextureDescriptor {
                        label: Some("window buffer"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_SRC
                            | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                )
            };
            let view = texture.create_view(&Default::default());
            buffers.push(Buffer {
                texture,
                view,
                copy_source: Default::default(),
            });
        }
        self.wrapped = true;
        Ok(buffers)
    }
    /// Whether `buffers` were handed out at the current size.
    pub fn buffers_made(&self) -> bool {
        self.wrapped
    }
    /// Let go of `buffers`, the wrappers of this chain's buffers, before it
    /// resizes or goes. Each is destroyed, its views and bind groups with
    /// it, and freed once the GPU has finished with it, now: wgpu frees a
    /// texture merely dropped at its next upkeep, which can come after the
    /// swap chain went, and Direct3D (as Windows Server's is) raises an
    /// exception that ends the process when a buffer is released after its
    /// swap chain.
    pub fn release(&self, buffers: impl IntoIterator<Item = Buffer>) {
        for buffer in buffers {
            buffer.texture.destroy();
        }
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        // Direct3D 11: the views go, and the context lets go of whatever
        // it still binds of them.
        #[cfg(d3d11)]
        if let Owner::D3d11 { device, views, .. } = &self.owner {
            views.borrow_mut().take();
            // SAFETY: Unbinds everything; each frame binds what it draws with.
            unsafe { device.context.ClearState() };
            device.wait_idle();
        }
    }
    /// Direct3D 11: the views of the buffer to draw and the one shown
    /// (`Owner::D3d11`), once `buffers` made them.
    #[cfg(d3d11)]
    pub fn d3d11_views(&self) -> Option<std::cell::Ref<'_, [crate::d3d11_frame::Surface; 2]>> {
        let Owner::D3d11 { views, .. } = &self.owner else {
            return None;
        };
        std::cell::Ref::filter_map(views.borrow(), Option::as_ref).ok()
    }
    /// Resize the buffers, once every wrapper of them was `release`d; take
    /// the new ones from `buffers`.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), GpuError> {
        self.wait_idle();
        // SAFETY: No reference to the old buffers remains (`release`).
        unsafe {
            self.swap_chain.ResizeBuffers(
                BUFFERS,
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0),
            )
        }
        .map_err(|e| GpuError(format!("ResizeBuffers: {e}")))?;
        self.whole = true;
        #[cfg(d3d11)]
        if let Owner::D3d11 { presents, .. } = &self.owner {
            presents.set(0);
        }
        Ok(())
    }
    /// The buffer the next frame draws into.
    pub fn current(&self) -> usize {
        #[cfg(d3d11)]
        if let Owner::D3d11 { presents, .. } = &self.owner {
            return presents.get() % BUFFERS as usize;
        }
        // SAFETY: Reads the swap chain's index.
        unsafe { self.swap_chain.GetCurrentBackBufferIndex() as usize }
    }
    /// Wait, up to `timeout_ms`, until a frame may start without queueing
    /// too far ahead of the compositor. False on a timeout.
    pub fn wait(&self, timeout_ms: u32) -> bool {
        // SAFETY: The swap chain's own waitable handle, alive with it.
        unsafe { WaitForSingleObjectEx(self.waitable, timeout_ms, true) == WAIT_OBJECT_0 }
    }
    /// Present the current buffer, `dirty` (physical pixels) the regions this
    /// frame changed (all of it before a buffer was first shown). False when
    /// the window can't be seen (minimized, or its display off).
    pub fn present(&mut self, dirty: &[Scissor]) -> Result<bool, GpuError> {
        let rects: Vec<RECT> = if self.whole || *WHOLE {
            Vec::new()
        } else {
            dirty
                .iter()
                .filter(|(_, _, w, h)| *w > 0 && *h > 0)
                .map(|&(x, y, w, h)| RECT {
                    left: x as i32,
                    top: y as i32,
                    right: (x + w) as i32,
                    bottom: (y + h) as i32,
                })
                .collect()
        };
        // Nothing changed: present it whole (an empty list means all of it,
        // and the frame was drawn for a reason; it is rare).
        let params = DXGI_PRESENT_PARAMETERS {
            DirtyRectsCount: rects.len() as u32,
            pDirtyRects: if rects.is_empty() {
                std::ptr::null_mut()
            } else {
                rects.as_ptr().cast_mut()
            },
            pScrollRect: std::ptr::null_mut(),
            pScrollOffset: std::ptr::null_mut(),
        };
        // SAFETY: `params` points at `rects`, alive across the call; sync
        // interval 0: a present replaces one the compositor has not taken.
        let result = unsafe { self.swap_chain.Present1(0, DXGI_PRESENT(0), &params) };
        result
            .ok()
            .map_err(|e| GpuError(format!("Present1: {e}")))?;
        self.whole = false;
        #[cfg(d3d11)]
        if let Owner::D3d11 { presents, .. } = &self.owner {
            presents.set(presents.get() + 1);
        }
        Ok(result != DXGI_STATUS_OCCLUDED)
    }
    /// The chain made again, opaque unless `transparent`, the size it was,
    /// once every wrapper of its buffers was `release`d. The old one, and
    /// the window's composition target with it, go first.
    pub fn remade(self, transparent: bool) -> Result<Self, GpuError> {
        let (device, hwnd) = (self.device.clone(), self.hwnd);
        // SAFETY: Reads the swap chain's description.
        let desc = unsafe { self.swap_chain.GetDesc1() }
            .map_err(|e| GpuError(format!("GetDesc1: {e}")))?;
        #[cfg(d3d11)]
        if let Owner::D3d11 { device: d3d11, .. } = &self.owner {
            let d3d11 = d3d11.clone();
            drop(self);
            return Self::new_d3d11(&device, &d3d11, hwnd, desc.Width, desc.Height, transparent);
        }
        drop(self);
        Self::new(&device, hwnd, desc.Width, desc.Height, transparent)
    }
    /// Wait until the queue has run all it was given, presents included:
    /// wgpu waits for its own submissions alone, and a swap chain resized or
    /// released while a present of it is still queued raises an exception
    /// that ends the process on Windows Server's Direct3D (CI's runners).
    /// wgpu waits likewise for its own (`wait_for_present_queue_idle`).
    fn wait_idle(&self) {
        let (queue, raw) = match &self.owner {
            Owner::D3d12 { queue, raw } => (queue, raw),
            #[cfg(d3d11)]
            Owner::D3d11 { device, .. } => return device.wait_idle(),
        };
        // SAFETY: A fence of our own, signalled on wgpu's queue; a null
        // event makes SetEventOnCompletion wait until it is reached.
        unsafe {
            let Ok(fence) = raw.CreateFence::<ID3D12Fence>(0, D3D12_FENCE_FLAG_NONE) else {
                return;
            };
            if queue.Signal(&fence, 1).is_ok() && fence.GetCompletedValue() < 1 {
                let _ = fence.SetEventOnCompletion(1, HANDLE::default());
            }
        }
    }
    /// For `ZGX_PRESENT_LOG`'s frame statistics.
    pub fn swap_chain(&self) -> &IDXGISwapChain3 {
        &self.swap_chain
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        self.wait_idle();
        // SAFETY: The swap chain's waitable, closed once.
        let _ = unsafe { CloseHandle(self.waitable) };
    }
}
