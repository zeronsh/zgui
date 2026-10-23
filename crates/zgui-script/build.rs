//! Whether Luau was built with its asserts, as luau0-src decides it: on
//! when its build script has debug assertions (this one is built alike),
//! or unless `LUAU_CXXFLAGS` defines NDEBUG. A release build with them runs
//! scripts about a fifth slower, so it warns (`luau_asserts`).
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=LUAU_CXXFLAGS");
    println!("cargo::rustc-check-cfg=cfg(luau_asserts)");
    let flags = std::env::var("LUAU_CXXFLAGS").unwrap_or_default();
    let ndebug = flags.split_whitespace().any(|f| {
        let f = f.trim_start_matches(['-', '/']);
        f == "DNDEBUG" || f.starts_with("DNDEBUG=")
    });
    if cfg!(debug_assertions) || !ndebug {
        println!("cargo::rustc-cfg=luau_asserts");
    }
}
