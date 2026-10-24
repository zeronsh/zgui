//! Direct3D 12 alone, without zgui or wgpu: threads that each create a
//! device, a queue, a render target and a readback buffer, clear and copy,
//! wait for the GPU and release it all, over and over (what zgui-gpu's
//! `independent_device_lifecycle_stress` does through wgpu). NVIDIA's driver
//! has hung and crashed under this (docs/validation.md); this checks a
//! driver for it with nothing of zgui's involved. Windows only; CI never
//! runs it.
//!
//!     cargo run --release -p zgui-gpu --example d3d12_device_churn
//!
//! `WORKERS` threads (24) make `ITERATIONS` devices each (10), `RUNS` times
//! (5). A run that takes over a minute is reported as hung, and the process
//! exits with 2; a crash ends it with the access violation's code.

#[cfg(windows)]
mod churn {
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::Common::*, Dxgi::*};
    use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
    use windows::core::Interface as _;

    fn setting(name: &str, default: usize) -> usize {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    /// `resource` as a structure's field: borrowed, not a reference of the
    /// structure's own (a clone there is never released, and keeps the
    /// resource and its device alive past `device_life`).
    ///
    /// # Safety
    ///
    /// The field is never dropped, and `resource` outlives its use.
    unsafe fn borrowed(
        resource: &ID3D12Resource,
    ) -> std::mem::ManuallyDrop<Option<ID3D12Resource>> {
        // SAFETY: an interface is one non-null pointer, as is `Some` of one;
        // copied without AddRef, and never released (`ManuallyDrop`).
        unsafe { std::mem::transmute_copy(resource) }
    }

    fn committed(
        device: &ID3D12Device,
        kind: D3D12_HEAP_TYPE,
        desc: &D3D12_RESOURCE_DESC,
        state: D3D12_RESOURCE_STATES,
    ) -> windows::core::Result<ID3D12Resource> {
        let mut resource = None;
        let heap = D3D12_HEAP_PROPERTIES {
            Type: kind,
            ..Default::default()
        };
        // SAFETY: descriptions of ours, read during the call; the resource
        // is written into a local.
        unsafe {
            device.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                desc,
                state,
                None,
                &mut resource,
            )?
        };
        Ok(resource.unwrap())
    }

    /// One device's life: made, drawn with, read back and released.
    fn device_life(adapter: &IDXGIAdapter1) -> windows::core::Result<()> {
        // SAFETY: Direct3D 12 objects made here, used on this thread; what
        // the command list names stays alive until the fence says the GPU
        // is done with it (the wait at the end). An error between submitting
        // and the wait drops them early, as the device fails: the caller
        // takes any error as the end of the run.
        unsafe {
            let mut device: Option<ID3D12Device> = None;
            D3D12CreateDevice(adapter, D3D_FEATURE_LEVEL_11_0, &mut device)?;
            let device = device.unwrap();
            let queue: ID3D12CommandQueue =
                device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                    ..Default::default()
                })?;
            let sample = DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            };
            let target = committed(
                &device,
                D3D12_HEAP_TYPE_DEFAULT,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                    Width: 32,
                    Height: 32,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: sample,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )?;
            let readback = committed(
                &device,
                D3D12_HEAP_TYPE_READBACK,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: 32 * 256,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    SampleDesc: sample,
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            let rtvs: ID3D12DescriptorHeap =
                device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: 1,
                    ..Default::default()
                })?;
            let rtv = rtvs.GetCPUDescriptorHandleForHeapStart();
            device.CreateRenderTargetView(&target, None, rtv);
            let allocator: ID3D12CommandAllocator =
                device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            let list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
            list.ClearRenderTargetView(rtv, &[0.2, 0.3, 0.4, 1.0], None);
            list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: borrowed(&target),
                        Subresource: 0,
                        StateBefore: D3D12_RESOURCE_STATE_RENDER_TARGET,
                        StateAfter: D3D12_RESOURCE_STATE_COPY_SOURCE,
                    }),
                },
                ..Default::default()
            }]);
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: borrowed(&readback),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            Width: 32,
                            Height: 32,
                            Depth: 1,
                            RowPitch: 256,
                        },
                    },
                },
            };
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: borrowed(&target),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            list.Close()?;
            queue.ExecuteCommandLists(&[Some(list.cast()?)]);
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            queue.Signal(&fence, 1)?;
            let event = CreateEventW(None, false, false, None)?;
            fence.SetEventOnCompletion(1, event)?;
            WaitForSingleObject(event, INFINITE);
            CloseHandle(event)?;
            Ok(())
        }
    }

    /// `workers` threads at once, each through `iterations` device lives.
    fn churn(workers: usize, iterations: usize) {
        let start = std::sync::Arc::new(std::sync::Barrier::new(workers));
        let threads: Vec<_> = (0..workers)
            .map(|_| {
                let start = start.clone();
                std::thread::spawn(move || {
                    // SAFETY: a factory and adapter of this thread's own.
                    let factory: IDXGIFactory6 =
                        unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.unwrap();
                    // SAFETY: as above.
                    let adapter: IDXGIAdapter1 = unsafe {
                        factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER)
                    }
                    .unwrap();
                    start.wait();
                    for _ in 0..iterations {
                        device_life(&adapter).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }

    pub fn main() {
        let (workers, iterations) = (setting("WORKERS", 24), setting("ITERATIONS", 10));
        // SAFETY (this and the three below): DXGI calls on objects made
        // here, writing locals.
        let factory: IDXGIFactory6 =
            unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.unwrap();
        let adapter: IDXGIAdapter1 =
            unsafe { factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_MINIMUM_POWER) }
                .unwrap();
        let desc = unsafe { adapter.GetDesc1() }.unwrap();
        let version = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }.unwrap_or(0);
        println!(
            "{} (driver {}.{}.{}.{}), {workers} workers x {iterations} devices",
            String::from_utf16_lossy(&desc.Description).trim_end_matches('\0'),
            version >> 48,
            (version >> 32) & 0xFFFF,
            (version >> 16) & 0xFFFF,
            version & 0xFFFF,
        );
        for run in 0..setting("RUNS", 5) {
            let started = Instant::now();
            let (done, finished) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                churn(workers, iterations);
                let _ = done.send(());
            });
            if finished.recv_timeout(Duration::from_secs(60)).is_err() {
                println!("run {run}: hung (over 60 s)");
                std::process::exit(2);
            }
            println!("run {run}: {:.1} s", started.elapsed().as_secs_f64());
        }
    }
}

fn main() {
    #[cfg(windows)]
    churn::main();
    #[cfg(not(windows))]
    eprintln!("d3d12_device_churn is Windows-only");
}
