//! What a frame costs the CPU through each Direct3D, and through wgpu, with
//! nothing of zgx's drawn: one 1280x800 window, a flip-model swap chain of
//! three buffers in a DirectComposition visual presented at sync interval 0
//! (as zgx presents, and GPUI's D3D11 renderer), cleared and presented once a
//! compositor frame (`DwmFlush` paces). docs/windows.md gives the results.
//! Windows only; CI never runs it.
//!
//!     cargo run --release -p zgx-gpu --example d3d_frame_floor -- <mode> [frames]
//!
//! Modes: `none` (the window, its message pump and the pacing alone),
//! `d3d11` (the driver's internal threading off, as GPUI's renderer runs on
//! NVIDIA), `d3d11-threaded` (the driver's default), `d3d12`, and `wgpu`
//! (Direct3D 12 through wgpu, set up as zgx-gpu sets it up). Prints the
//! median cycles of each stage on the drawing thread, the process's cycles
//! a frame (every thread), and the private memory the device added.
//!
//! Direct3D 11 presents flip-sequential and Direct3D 12 flip-discard;
//! `FLOOR_EFFECT=sequential` or `discard` gives both the one. Its GPU time
//! is the floor docs/results/d3d11-proof-windows-2026-10-27 compares.

#[cfg(windows)]
mod floor {
    use std::mem::ManuallyDrop;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::{
        Direct3D::*, Direct3D11::*, Direct3D12::*, DirectComposition::*, Dwm::DwmFlush,
        Dxgi::Common::*, Dxgi::*,
    };
    use windows::Win32::System::{
        LibraryLoader::GetModuleHandleW, ProcessStatus::*, Threading::*, WindowsProgramming::*,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{Interface as _, Result, w};

    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 800;
    const BUFFERS: u32 = 3;
    const WARM_UP: usize = 120;

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> LRESULT {
        // SAFETY: the system's default handling of the window's messages.
        unsafe { DefWindowProcW(hwnd, message, w, l) }
    }

    fn private_mib() -> f64 {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: the structure's size is its `cb`.
        unsafe {
            GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), counters.cb)
                .expect("process memory");
        }
        counters.PrivateUsage as f64 / 1048576.
    }

    fn thread_cycles() -> u64 {
        let mut cycles = 0;
        // SAFETY: the calling thread's pseudo-handle.
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles).expect("cycles") };
        cycles
    }

    fn process_cycles() -> u64 {
        let mut cycles = 0;
        // SAFETY: the process's pseudo-handle.
        unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut cycles).expect("cycles") };
        cycles
    }

    fn pump() {
        let mut message = MSG::default();
        // SAFETY: this thread's own window messages.
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn swap_chain_desc(effect: DXGI_SWAP_EFFECT) -> DXGI_SWAP_CHAIN_DESC1 {
        DXGI_SWAP_CHAIN_DESC1 {
            Width: WIDTH,
            Height: HEIGHT,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFERS,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: match std::env::var("FLOOR_EFFECT").as_deref() {
                Ok("sequential") => DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                Ok("discard") => DXGI_SWAP_EFFECT_FLIP_DISCARD,
                _ => effect,
            },
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: 0,
            Stereo: false.into(),
        }
    }

    /// One way to draw a frame, by stage: each call returns the drawing
    /// thread's cycles for the stages it ran.
    trait Frame {
        fn stages(&self) -> &'static [&'static str];
        fn frame(&mut self, n: usize) -> Vec<u64>;
    }

    struct Nothing;

    impl Frame for Nothing {
        fn stages(&self) -> &'static [&'static str] {
            &[]
        }
        fn frame(&mut self, _: usize) -> Vec<u64> {
            vec![]
        }
    }

    fn shade(n: usize) -> [f32; 4] {
        [(n % 60) as f32 / 60., 0.2, 0.3, 1.]
    }

    struct D3d11 {
        context: ID3D11DeviceContext,
        target: ID3D11RenderTargetView,
        swap_chain: IDXGISwapChain1,
    }

    impl Frame for D3d11 {
        fn stages(&self) -> &'static [&'static str] {
            &["clear", "present"]
        }
        fn frame(&mut self, n: usize) -> Vec<u64> {
            let t0 = thread_cycles();
            // SAFETY: the swap chain's back buffer's view; flip-sequential
            // in Direct3D 11 keeps buffer 0 the one to draw.
            unsafe { self.context.ClearRenderTargetView(&self.target, &shade(n)) };
            let t1 = thread_cycles();
            // SAFETY: a configured swap chain.
            unsafe {
                self.swap_chain
                    .Present(0, DXGI_PRESENT(0))
                    .ok()
                    .expect("present")
            };
            vec![t1 - t0, thread_cycles() - t1]
        }
    }

    struct D3d12 {
        queue: ID3D12CommandQueue,
        swap_chain: IDXGISwapChain3,
        buffers: Vec<ID3D12Resource>,
        targets: Vec<D3D12_CPU_DESCRIPTOR_HANDLE>,
        allocators: Vec<ID3D12CommandAllocator>,
        list: ID3D12GraphicsCommandList,
        fence: ID3D12Fence,
        event: HANDLE,
        value: u64,
        pending: Vec<u64>,
    }

    /// `resource` as a barrier's field: borrowed, not a reference of the
    /// barrier's own (`ManuallyDrop` never releases it).
    fn transition(
        resource: &ID3D12Resource,
        before: D3D12_RESOURCE_STATES,
        after: D3D12_RESOURCE_STATES,
    ) -> D3D12_RESOURCE_BARRIER {
        D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    // SAFETY: the same pointer, outliving the barrier.
                    pResource: unsafe { std::mem::transmute_copy(resource) },
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: before,
                    StateAfter: after,
                }),
            },
        }
    }

    impl Frame for D3d12 {
        fn stages(&self) -> &'static [&'static str] {
            &["resets", "record", "execute", "signal", "present"]
        }
        fn frame(&mut self, n: usize) -> Vec<u64> {
            // SAFETY: Direct3D 12 calls on objects this owns; a buffer's
            // allocator is reset only once the GPU finished its last frame.
            unsafe {
                let i = self.swap_chain.GetCurrentBackBufferIndex() as usize;
                if self.fence.GetCompletedValue() < self.pending[i] {
                    self.fence
                        .SetEventOnCompletion(self.pending[i], self.event)
                        .expect("fence event");
                    WaitForSingleObject(self.event, INFINITE);
                }
                let t0 = thread_cycles();
                self.allocators[i].Reset().expect("reset");
                self.list.Reset(&self.allocators[i], None).expect("reset");
                let t1 = thread_cycles();
                let buffer = &self.buffers[i];
                self.list.ResourceBarrier(&[transition(
                    buffer,
                    D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                )]);
                self.list
                    .ClearRenderTargetView(self.targets[i], &shade(n), None);
                self.list.ResourceBarrier(&[transition(
                    buffer,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_PRESENT,
                )]);
                self.list.Close().expect("close");
                let t2 = thread_cycles();
                self.queue
                    .ExecuteCommandLists(&[Some(self.list.cast().expect("command list"))]);
                let t3 = thread_cycles();
                self.value += 1;
                self.queue.Signal(&self.fence, self.value).expect("signal");
                self.pending[i] = self.value;
                let t4 = thread_cycles();
                self.swap_chain
                    .Present(0, DXGI_PRESENT(0))
                    .ok()
                    .expect("present");
                vec![t1 - t0, t2 - t1, t3 - t2, t4 - t3, thread_cycles() - t4]
            }
        }
    }

    struct Wgpu {
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
    }

    impl Frame for Wgpu {
        fn stages(&self) -> &'static [&'static str] {
            &["acquire", "encode", "submit", "present"]
        }
        fn frame(&mut self, n: usize) -> Vec<u64> {
            let t0 = thread_cycles();
            let (wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame)) = self.surface.get_current_texture()
            else {
                panic!("no frame");
            };
            let view = frame.texture.create_view(&Default::default());
            let t1 = thread_cycles();
            let mut encoder = self.device.create_command_encoder(&Default::default());
            let [r, g, b, a] = shade(n).map(f64::from);
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let commands = encoder.finish();
            let t2 = thread_cycles();
            self.queue.submit([commands]);
            let t3 = thread_cycles();
            frame.present();
            vec![t1 - t0, t2 - t1, t3 - t2, thread_cycles() - t3]
        }
    }

    /// The frame for `mode`, presenting into `hwnd`.
    ///
    /// # Safety
    /// `hwnd` is a live window of this thread's, outliving the frame.
    unsafe fn frame_for(mode: &str, hwnd: HWND) -> Result<Box<dyn Frame>> {
        if mode == "none" {
            return Ok(Box::new(Nothing));
        }
        if mode == "wgpu" {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = wgpu::Backends::DX12;
            descriptor.backend_options.dx12.presentation_system =
                wgpu::Dx12SwapchainKind::DxgiFromVisual;
            let instance = wgpu::Instance::new(descriptor);
            let handle = wgpu::rwh::Win32WindowHandle::new(
                std::num::NonZeroIsize::new(hwnd.0 as isize).expect("window"),
            );
            // SAFETY: the window outlives the surface (the caller's).
            let surface = unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Windows(
                        wgpu::rwh::WindowsDisplayHandle::new(),
                    )),
                    raw_window_handle: wgpu::rwh::RawWindowHandle::Win32(handle),
                })
            }
            .expect("surface");
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .expect("adapter");
            let (device, queue) =
                pollster::block_on(adapter.request_device(&Default::default())).expect("device");
            let mut config = surface
                .get_default_config(&adapter, WIDTH, HEIGHT)
                .expect("surface config");
            config.format = config.format.remove_srgb_suffix();
            config.present_mode = wgpu::PresentMode::Mailbox;
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
            surface.configure(&device, &config);
            return Ok(Box::new(Wgpu {
                device,
                queue,
                surface,
            }));
        }
        // SAFETY: Direct3D and DirectComposition objects made and joined
        // here; the window is the caller's.
        unsafe {
            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let composition: IDCompositionDesktopDevice = DCompositionCreateDevice2(None)?;
            let target = composition.CreateTargetForHwnd(hwnd, true)?;
            let visual = composition.CreateVisual()?;
            let frame: Box<dyn Frame> = if mode.starts_with("d3d11") {
                let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
                if mode == "d3d11" {
                    flags |= D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS;
                }
                let (mut device, mut context) = (None, None);
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    flags,
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )?;
                let device = device.expect("device");
                let swap_chain = factory.CreateSwapChainForComposition(
                    &device,
                    &swap_chain_desc(DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL),
                    None,
                )?;
                let back: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
                let mut view = None;
                device.CreateRenderTargetView(&back, None, Some(&mut view))?;
                visual.SetContent(&swap_chain)?;
                Box::new(D3d11 {
                    context: context.expect("context"),
                    target: view.expect("view"),
                    swap_chain,
                })
            } else {
                let mut device: Option<ID3D12Device> = None;
                D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
                let device = device.expect("device");
                let queue: ID3D12CommandQueue =
                    device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                        Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                        ..Default::default()
                    })?;
                let swap_chain: IDXGISwapChain3 = factory
                    .CreateSwapChainForComposition(
                        &queue,
                        &swap_chain_desc(DXGI_SWAP_EFFECT_FLIP_DISCARD),
                        None,
                    )?
                    .cast()?;
                let heap: ID3D12DescriptorHeap =
                    device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                        Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                        NumDescriptors: BUFFERS,
                        ..Default::default()
                    })?;
                let step = device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
                    as usize;
                let start = heap.GetCPUDescriptorHandleForHeapStart();
                let (mut buffers, mut targets) = (vec![], vec![]);
                for i in 0..BUFFERS {
                    let buffer: ID3D12Resource = swap_chain.GetBuffer(i)?;
                    let view = D3D12_CPU_DESCRIPTOR_HANDLE {
                        ptr: start.ptr + i as usize * step,
                    };
                    device.CreateRenderTargetView(&buffer, None, view);
                    buffers.push(buffer);
                    targets.push(view);
                }
                let allocators = (0..BUFFERS)
                    .map(|_| device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT))
                    .collect::<Result<Vec<ID3D12CommandAllocator>>>()?;
                let list: ID3D12GraphicsCommandList = device.CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocators[0],
                    None,
                )?;
                list.Close()?;
                visual.SetContent(&swap_chain)?;
                // The views live in the heap for the process's life.
                std::mem::forget(heap);
                Box::new(D3d12 {
                    queue,
                    swap_chain,
                    buffers,
                    targets,
                    allocators,
                    list,
                    fence: device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?,
                    event: CreateEventW(None, false, false, None)?,
                    value: 0,
                    pending: vec![0; BUFFERS as usize],
                })
            };
            target.SetRoot(&visual)?;
            composition.Commit()?;
            // The tree shows the visual for the process's life.
            std::mem::forget((target, visual, composition));
            Ok(frame)
        }
    }

    pub fn main() -> Result<()> {
        let args: Vec<String> = std::env::args().collect();
        let mode = args.get(1).map_or("d3d12", String::as_str);
        let frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600);
        assert!(
            ["none", "d3d11", "d3d11-threaded", "d3d12", "wgpu"].contains(&mode),
            "modes: none, d3d11, d3d11-threaded, d3d12, wgpu"
        );
        // SAFETY: a window of this thread's, alive until the process ends.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("zgx_d3d_frame_floor");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            });
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP,
                class,
                w!("d3d_frame_floor"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                WIDTH as i32,
                HEIGHT as i32,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        pump();
        let before = private_mib();
        // SAFETY: the window lives until the process ends.
        let mut frame = unsafe { frame_for(mode, hwnd)? };
        let mut stages: Vec<Vec<u64>> = vec![vec![]; frame.stages().len()];
        let (mut process_start, mut thread_start) = (0, 0);
        for n in 0..WARM_UP + frames {
            pump();
            // SAFETY: no arguments; waits for the compositor's next frame.
            let _ = unsafe { DwmFlush() };
            if n == WARM_UP {
                (process_start, thread_start) = (process_cycles(), thread_cycles());
            }
            let cycles = frame.frame(n);
            if n >= WARM_UP {
                for (stage, c) in stages.iter_mut().zip(cycles) {
                    stage.push(c);
                }
            }
        }
        let per_frame = |cycles: u64| cycles as f64 / frames as f64 / 1e3;
        let process = per_frame(process_cycles() - process_start);
        let thread = per_frame(thread_cycles() - thread_start);
        let medians: Vec<String> = frame
            .stages()
            .iter()
            .zip(&mut stages)
            .map(|(name, cycles)| {
                cycles.sort_unstable();
                format!("{name} {}", cycles[cycles.len() / 2] / 1000)
            })
            .collect();
        println!(
            "{mode}: {process:.0} K cycles a frame (process), {thread:.0} K on the drawing thread \
             (pump and pacing included); stages (median K cycles): {}; private +{:.1} MiB",
            if medians.is_empty() {
                "none".into()
            } else {
                medians.join(", ")
            },
            private_mib() - before
        );
        Ok(())
    }
}

fn main() {
    #[cfg(windows)]
    floor::main().expect("frame floor");
    #[cfg(not(windows))]
    eprintln!("d3d_frame_floor measures Direct3D: Windows only");
}
