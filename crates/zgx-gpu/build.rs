//! `zgx_wgpu_hal_patched` when wgpu-hal is the repository's (vendor/wgpu-hal,
//! its `links` metadata): on Windows, the preferred Direct3D 12 adapter is
//! exposed without opening a device on every other one, which that wgpu-hal
//! adds and crates.io's does not (wgpu's `request_adapter` then ranks them).
//!
//! `own_surfaces` where a window can present through surfaces of the
//! renderer's own, which keep what they held (`LayerPresent`): macOS
//! (layer_present.rs) and Windows (dxgi_present.rs).
//!
//! `d3d11` on Windows with the `d3d11` feature: frames drawn with Direct3D
//! 11 under `ZGX_D3D11=1` (d3d11_frame.rs).
fn main() {
    println!("cargo::rustc-check-cfg=cfg(zgx_wgpu_hal_patched)");
    if std::env::var_os("DEP_ZGX_WGPU_HAL_PATCHED").is_some() {
        println!("cargo::rustc-cfg=zgx_wgpu_hal_patched");
    }
    println!("cargo::rustc-check-cfg=cfg(own_surfaces)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "macos" || os == "windows" {
        println!("cargo::rustc-cfg=own_surfaces");
    }
    // `d3d11`: Windows with the `d3d11` feature (d3d11_frame.rs).
    println!("cargo::rustc-check-cfg=cfg(d3d11)");
    if os == "windows" && std::env::var_os("CARGO_FEATURE_D3D11").is_some() {
        println!("cargo::rustc-cfg=d3d11");
    }
}
