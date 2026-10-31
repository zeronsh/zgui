//! With the `link` feature: fetch Ghostty at `COMMIT`, patch it, build it
//! with Zig 0.16 and link it. The patch (`patches/embedded-macos.patch`,
//! adapted from gpui-libghostty's vendored Ghostty, MIT) lets the Command
//! Line Tools build it, without Xcode: the embedding library installs as a
//! static archive (upstream only builds it into an XCFramework), and Metal
//! shaders compile at runtime (upstream precompiles them with Xcode's `metal`).
//! It also adds `GHOSTTY_QUIET_LOGIN`: a surface whose environment sets it
//! starts without login(1)'s "Last login" banner. And it cuts, rather than
//! wraps, the background-only cells at the end of a row that narrows (the
//! spaces TUIs fill their rows with), so resizing leaves no stub rows of
//! background. Builds are cached across targets and checkouts in
//! `~/Library/Caches/zgx-ghostty` (or `$ZGX_GHOSTTY_CACHE`).
//!
//! `ZIG` picks the Zig executable (default: Homebrew's `zig@0.16`, then
//! `zig`); `GHOSTTY_SOURCE_DIR` builds a local Ghostty checkout instead.
use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
};

/// Ghostty's commit (1.3.2-dev); `ghostty.h` and `src/bindings.rs` match it.
const COMMIT: &str = "9f0e1719dc918368367d368bfe300f59bb68b5a4";
const REPOSITORY: &str = "https://github.com/ghostty-org/ghostty.git";
const PATCH: &str = include_str!("patches/embedded-macos.patch");
const OPTIONS: &[&str] = &[
    "-Dversion-string=1.3.2-dev",
    "-Dapp-runtime=none",
    "-Demit-xcframework=false",
    "-Demit-macos-app=false",
    "-Demit-docs=false",
    "-Demit-terminfo=false",
    "-Demit-bench=false",
    "-Demit-webdata=false",
    "-Di18n=false",
    "-Dsentry=false",
    "-Doptimize=ReleaseFast",
];
const FRAMEWORKS: &[&str] = &[
    "AppKit",
    "Carbon",
    "CoreFoundation",
    "CoreGraphics",
    "CoreText",
    "CoreVideo",
    "Foundation",
    "IOSurface",
    "Metal",
    "QuartzCore",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=patches/embedded-macos.patch");
    // (ZGUI_GHOSTTY_CACHE: its name before the rename, read for one release.) rename-to-zgx: keep
    for var in [
        "ZIG",
        "GHOSTTY_SOURCE_DIR",
        "ZGX_GHOSTTY_CACHE",
        "ZGX_GHOSTTY_CACHE",
    ] {
        // rename-to-zgx: keep
        println!("cargo:rerun-if-env-changed={var}");
    }
    if env::var_os("CARGO_FEATURE_LINK").is_none() {
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        panic!("zgx-ghostty-sys links Ghostty's macOS embedding library: macOS only for now");
    }

    let zig = zig();
    let version = output(Command::new(&zig).arg("version")).unwrap_or_else(|| {
        panic!("run {zig:?}: install Zig 0.16 (`brew install zig@0.16`) or set ZIG")
    });
    assert!(
        version.starts_with("0.16"),
        "Ghostty {COMMIT} needs Zig 0.16, {zig:?} is {version}: `brew install zig@0.16` or set ZIG"
    );

    let cache = cache_root();
    fs::create_dir_all(&cache).expect("create the zgx-ghostty cache");
    let local = env::var_os("GHOSTTY_SOURCE_DIR").map(PathBuf::from);
    let key = format!(
        "{}-{:016x}-zig{version}",
        if local.is_some() {
            "local"
        } else {
            &COMMIT[..12]
        },
        fnv(PATCH.as_bytes()),
    );
    let prefix = cache.join(&key);
    let library = prefix.join("lib/libghostty-internal.a");

    // One build at a time per cache (cargo runs build scripts in parallel).
    let lock = File::create(cache.join(".lock")).expect("create the cache lock");
    lock.lock().expect("lock the zgx-ghostty cache");
    if local.is_some() || !library.is_file() {
        let source = match &local {
            Some(dir) => dir.clone(),
            None => fetch(&cache),
        };
        build(&zig, &source, &cache, &prefix);
    }
    drop(lock);

    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=ghostty-internal");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=objc");
    for framework in FRAMEWORKS {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:include={}", prefix.join("include").display());
}

fn zig() -> PathBuf {
    if let Some(zig) = env::var_os("ZIG") {
        return zig.into();
    }
    let brew = Path::new("/opt/homebrew/opt/zig@0.16/bin/zig");
    if brew.is_file() {
        brew.into()
    } else {
        "zig".into()
    }
}

fn cache_root() -> PathBuf {
    let dir = env::var_os("ZGX_GHOSTTY_CACHE").or_else(|| env::var_os("ZGUI_GHOSTTY_CACHE")); // rename-to-zgx: keep
    if let Some(dir) = dir {
        // Zig runs in Ghostty's source directory: a relative path would be
        // read from there, and the library looked for here.
        let dir = PathBuf::from(dir);
        return if dir.is_relative() {
            env::current_dir()
                .expect("the build script's directory")
                .join(dir)
        } else {
            dir
        };
    }
    let home = env::var_os("HOME").expect("HOME is set");
    Path::new(&home).join("Library/Caches/zgx-ghostty")
}

/// Ghostty's source at `COMMIT`, patched: fetched once into the cache.
fn fetch(cache: &Path) -> PathBuf {
    let source = cache.join(format!("ghostty-{}", &COMMIT[..12]));
    let stamp = source.join(".zgx-patched");
    let patched = format!("{:016x}", fnv(PATCH.as_bytes()));
    if fs::read_to_string(&stamp).ok().as_deref() == Some(patched.as_str()) {
        return source;
    }
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(&source)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    if source.join(".git").is_dir() {
        // Fetched before, patched differently: back to the commit.
        git(&["checkout", "--quiet", "--", "."]);
    } else {
        if source.exists() {
            fs::remove_dir_all(&source).expect("remove a stale Ghostty checkout");
        }
        fs::create_dir_all(&source).expect("create the Ghostty checkout");
        git(&["init", "--quiet"]);
        git(&["fetch", "--quiet", "--depth", "1", REPOSITORY, COMMIT]);
        git(&["checkout", "--quiet", "FETCH_HEAD"]);
    }
    let patch = source.join(".zgx.patch");
    fs::write(&patch, PATCH).expect("write the build patch");
    git(&["apply", ".zgx.patch"]);
    fs::write(&stamp, patched).expect("stamp the patched checkout");
    source
}

fn build(zig: &Path, source: &Path, cache: &Path, prefix: &Path) {
    let staging = prefix.with_extension("staging");
    if staging.exists() {
        fs::remove_dir_all(&staging).expect("remove a stale staging build");
    }
    let status = Command::new(zig)
        .current_dir(source)
        .env("ZIG_GLOBAL_CACHE_DIR", cache.join("zig-global"))
        .env("ZIG_LOCAL_CACHE_DIR", cache.join("zig-local"))
        .args(["build", "--prefix"])
        .arg(&staging)
        .args(OPTIONS)
        .status()
        .unwrap_or_else(|error| panic!("run {zig:?}: {error}"));
    assert!(status.success(), "Ghostty's Zig build failed");
    assert!(
        staging.join("lib/libghostty-internal.a").is_file(),
        "Ghostty's build produced no libghostty-internal.a"
    );
    if prefix.exists() {
        fs::remove_dir_all(prefix).expect("replace the cached build");
    }
    fs::rename(&staging, prefix).expect("publish the Ghostty build");
}

fn output(command: &mut Command) -> Option<String> {
    let out = command.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// FNV-1a: a stable fingerprint for the patch.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
