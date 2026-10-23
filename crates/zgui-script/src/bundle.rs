//! An app's scripts compiled into its binary: what ships, where development
//! reads the files and reloads them as they change. Scripts are Luau
//! bytecode (compiled when the app builds, so a syntax error fails the
//! build); `.luaurc`, theme files and images are kept as they are. A host
//! made from one ([`crate::ScriptHost::bundled`]) reads nothing from the disk
//! and watches nothing.
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     zgui_script::build::bundle("ui", "app").unwrap();
//! }
//! // the app
//! static UI: zgui_script::Bundle = zgui_script::include_bundle!("app");
//! let host = ScriptHost::bundled(&runtime, &UI, "app.luau");
//! ```
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

/// A bundle's files, by path from its root (`/`-separated).
pub struct Bundle {
    /// Its name (`include_bundle!`'s), for paths in errors.
    pub name: &'static str,
    /// Where its files seem to be (`/zgui-bundle/<name>`): a root no disk
    /// has, so paths built from it (`require`, `@self`, aliases) never reach
    /// the disk.
    pub root: &'static str,
    /// Sorted by path: a lookup is a binary search, with nothing allocated.
    pub files: &'static [BundleFile],
}
/// One script file in a [`Bundle`], compiled into the binary.
pub struct BundleFile {
    /// Its path from the bundle's root, `/`-separated.
    pub path: &'static str,
    /// Its contents.
    pub bytes: &'static [u8],
}

impl Bundle {
    /// `path`'s place in the bundle ("" for its root), if it is under it.
    /// Paths made on Windows separate with `\`; the bundle's with `/`.
    fn rel<'p>(&self, path: &'p Path) -> Option<Cow<'p, str>> {
        let path = path.to_str()?;
        if cfg!(windows) && path.contains('\\') {
            let path = path.replace('\\', "/");
            return self.rest(&path).map(|rest| Cow::Owned(rest.to_owned()));
        }
        self.rest(path).map(Cow::Borrowed)
    }
    fn rest<'p>(&self, path: &'p str) -> Option<&'p str> {
        let rest = path.strip_prefix(self.root)?;
        if rest.is_empty() {
            Some(rest)
        } else {
            rest.strip_prefix('/')
        }
    }
    /// The file at `path`, if the bundle has it.
    pub(crate) fn get(&self, path: &Path) -> Option<&'static [u8]> {
        let rel = self.rel(path)?;
        let at = self
            .files
            .binary_search_by(|file| (*file.path).cmp(&rel))
            .ok()?;
        Some(self.files[at].bytes)
    }
    /// The files under directory `rel` (sorted paths share the prefix, so
    /// they are together).
    fn under<'b>(&'b self, rel: &'b str) -> impl Iterator<Item = &'b BundleFile> + 'b {
        let start = self.files.partition_point(|file| file.path < rel);
        self.files[start..]
            .iter()
            .take_while(move |file| file.path.starts_with(rel))
            .filter(move |file| {
                rel.is_empty() || file.path.as_bytes().get(rel.len()) == Some(&b'/')
            })
    }
    /// Whether `path` holds files.
    pub(crate) fn is_dir(&self, path: &Path) -> bool {
        self.rel(path)
            .is_some_and(|rel| self.under(&rel).next().is_some())
    }
    /// The files directly in directory `path`.
    pub(crate) fn list(&self, path: &Path) -> Vec<PathBuf> {
        let Some(rel) = self.rel(path) else {
            return Vec::new();
        };
        let skip = if rel.is_empty() { 0 } else { rel.len() + 1 };
        self.under(&rel)
            .map(|file| &file.path[skip..])
            .filter(|name| !name.contains('/'))
            .map(|name| path.join(name))
            .collect()
    }
}

/// The bundle a build script made with [`build::bundle`] under `name`.
#[macro_export]
macro_rules! include_bundle {
    ($name:literal) => {
        include!(concat!(env!("OUT_DIR"), "/zgui-bundle-", $name, ".rs"))
    };
}

/// For build scripts: compile a directory of scripts into a bundle.
pub mod build {
    use std::{
        fmt::Write as _,
        io,
        path::{Path, PathBuf},
    };

    /// Bundle `dir` (from the package's root) as `name`, for
    /// [`include_bundle!`](crate::include_bundle): every `.luau` file compiled
    /// to bytecode, and the other files the scripts read (`.luaurc`, theme
    /// `.json`, images, SVGs) as they are. Rust sources and other dotfiles
    /// stay out. Cargo rebuilds when a file in it changes; a script that
    /// does not parse fails the build, with its file and line.
    pub fn bundle(dir: impl AsRef<Path>, name: &str) -> io::Result<()> {
        let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("bundle runs in a build script"))?;
        let out = std::env::var_os("OUT_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("bundle runs in a build script"))?;
        let dir = manifest.join(dir.as_ref());
        let store = out.join(format!("zgui-bundle-{name}"));
        std::fs::create_dir_all(&store)?;
        let mut files = Vec::new();
        collect(&dir, &dir, &mut files)?;
        files.sort();
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut code = String::new();
        let _ = writeln!(
            code,
            "zgui_script::Bundle {{ name: {name:?}, root: {:?}, files: &[",
            format!("/zgui-bundle/{name}")
        );
        for (index, rel) in files.iter().enumerate() {
            let source = dir.join(rel);
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = std::fs::read(&source)?;
            let bytes = if rel.ends_with(".luau") {
                compile(&bytes).map_err(|e| io::Error::other(format!("{rel}: {e}")))?
            } else {
                bytes
            };
            let stored = store.join(format!("{index}"));
            std::fs::write(&stored, bytes)?;
            let _ = writeln!(
                code,
                "    zgui_script::BundleFile {{ path: {rel:?}, bytes: include_bytes!({:?}) }},",
                stored.display().to_string()
            );
        }
        code.push_str("] }\n");
        std::fs::write(out.join(format!("zgui-bundle-{name}.rs")), code)
    }

    /// A script's bytecode, as a host would load it: as an expression if it
    /// is one (`ui.text "hi"`), else as statements (`return ui.column {}`).
    pub fn compile(source: &[u8]) -> Result<Vec<u8>, String> {
        let compiler = mlua::chunk::Compiler::new();
        let mut expression = b"return ".to_vec();
        expression.extend_from_slice(source);
        if let Ok(bytecode) = compiler.compile(&expression) {
            return Ok(bytecode);
        }
        compiler.compile(source).map_err(|e| e.to_string())
    }

    fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') && name != ".luaurc" {
                continue;
            }
            if path.is_dir() {
                collect(root, &path, out)?;
                continue;
            }
            let wanted = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("luau" | "json" | "png" | "jpg" | "jpeg" | "svg" | "gif" | "webp")
            ) || name == ".luaurc";
            if wanted && let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
}
