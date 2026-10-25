//! With `terminal-bundle`: the terminal example's interface, compiled into
//! it (`zgx_script::include_bundle!("terminal")`), as it ships.
//!
//! `zgx_winit_patched` when winit is the repository's (vendor/winit, its
//! `links` metadata): Wayland popups use the window's `xdg_surface`, which
//! that winit gives and crates.io's does not (popups then open in the
//! window).
fn main() {
    // `zgx_accesskit_patched` when AccessKit's consumer is the repository's
    // (vendor/accesskit_consumer): its adapters give a field's error
    // message as the platform's own, which crates.io's do not (it is said
    // in the description then).
    println!("cargo::rustc-check-cfg=cfg(zgx_accesskit_patched)");
    if std::env::var_os("DEP_ZGX_ACCESSKIT_CONSUMER_PATCHED").is_some() {
        println!("cargo::rustc-cfg=zgx_accesskit_patched");
    }
    println!("cargo::rustc-check-cfg=cfg(zgx_winit_patched)");
    if std::env::var_os("DEP_ZGX_WINIT_PATCHED").is_some() {
        println!("cargo::rustc-cfg=zgx_winit_patched");
    }
    #[cfg(feature = "terminal-bundle")]
    zgx_script::build::bundle("examples/terminal", "terminal")
        .expect("bundle the terminal example's scripts");
}
