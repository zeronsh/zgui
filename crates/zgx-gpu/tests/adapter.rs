//! Windows: the shared device is on the adapter DXGI ranks first for low
//! power, the one wgpu would rank first, found without opening a device on
//! the others; a GPU always before software (WARP).
#![cfg(windows)]
use windows::Win32::Graphics::Dxgi::*;

/// DXGI's first adapter for `preference`, a GPU before software: its name,
/// vendor and device ids, and whether it is software.
fn first(preference: DXGI_GPU_PREFERENCE) -> (String, u32, u32, bool) {
    // SAFETY: DXGI calls on objects made here, writing locals (as below).
    let factory: IDXGIFactory6 =
        unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.unwrap();
    let mut adapters: Vec<_> = (0..)
        .map_while(|i| {
            // SAFETY: as above.
            let adapter: IDXGIAdapter1 =
                unsafe { factory.EnumAdapterByGpuPreference(i, preference) }.ok()?;
            // SAFETY: as above.
            let desc = unsafe { adapter.GetDesc1() }.unwrap();
            let name = String::from_utf16_lossy(&desc.Description);
            let name = name.trim_end_matches('\0').to_string();
            // Without a GPU, DXGI lists WARP twice, once claiming to be
            // hardware (wgpu-hal leaves that one out).
            let software = DXGI_ADAPTER_FLAG(desc.Flags as i32)
                .contains(DXGI_ADAPTER_FLAG_SOFTWARE)
                || (desc.VendorId == 0x1414 && name.contains("Basic Render Driver"));
            Some((name, desc.VendorId, desc.DeviceId, software))
        })
        .collect();
    adapters.sort_by_key(|adapter| adapter.3);
    adapters.remove(0)
}

fn assert_on(info: &wgpu::AdapterInfo, expected: (String, u32, u32, bool)) {
    // Software only where there is no GPU.
    assert_eq!(info.device_type == wgpu::DeviceType::Cpu, expected.3);
    if !expected.3 {
        assert_eq!(
            (info.name.as_str(), info.vendor, info.device),
            (expected.0.as_str(), expected.1, expected.2)
        );
    }
}

#[test]
fn the_device_is_on_dxgis_first_gpu_for_low_power() {
    let info = zgx_gpu::GpuRenderer::new(8, 8).unwrap().adapter_info();
    if info.backend != wgpu::Backend::Dx12 {
        return;
    }
    assert_on(&info, first(DXGI_GPU_PREFERENCE_MINIMUM_POWER));
}

#[test]
fn high_performance_takes_dxgis_first_gpu_for_performance() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    let instance = wgpu::Instance::new(descriptor);
    // SAFETY: the hal instance is only read (an adapter exposed from it).
    let Some(hal) = (unsafe { instance.as_hal::<wgpu::hal::api::Dx12>() }) else {
        return;
    };
    let exposed = hal
        .expose_preferred_adapter(wgpu::PowerPreference::HighPerformance)
        .unwrap();
    // SAFETY: exposed from this instance's own hal instance.
    let adapter = unsafe { instance.create_adapter_from_hal(exposed) };
    assert_on(
        &adapter.get_info(),
        first(DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE),
    );
}
