//! Each platform builds its own backends alone: Windows Direct3D 12 (Vulkan
//! only with the `vulkan` feature), macOS Metal, Linux Vulkan and GL.

#[test]
fn each_platform_builds_its_backends_alone() {
    let built = wgpu::Instance::enabled_backend_features();
    if cfg!(windows) {
        assert!(built.contains(wgpu::Backends::DX12));
        assert_eq!(
            built.contains(wgpu::Backends::VULKAN),
            cfg!(feature = "vulkan")
        );
        assert!(!built.contains(wgpu::Backends::GL));
    } else if cfg!(target_os = "macos") {
        assert_eq!(built, wgpu::Backends::METAL);
    } else if cfg!(target_os = "linux") {
        assert_eq!(built, wgpu::Backends::VULKAN | wgpu::Backends::GL);
    }
}
