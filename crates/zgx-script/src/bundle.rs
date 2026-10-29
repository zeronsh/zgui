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
//!     zgx_script::build::bundle("ui", "app").unwrap();
//! }
//! // the app
//! static UI: zgx_script::Bundle = zgx_script::include_bundle!("app");
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
    /// Where its files seem to be (`/zgx-bundle/<name>`): a root no disk
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
        include!(concat!(env!("OUT_DIR"), "/zgx-bundle-", $name, ".rs"))
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
    /// stay out. The packages its `zgx.json` lists (`"packages"`) come too,
    /// each as `@name/…` in the bundled host, with the `.luaurc` aliases into
    /// one pointing at its copy. Cargo rebuilds when a file in them changes;
    /// a script that does not parse fails the build, with its file and line.
    pub fn bundle(dir: impl AsRef<Path>, name: &str) -> io::Result<()> {
        let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("bundle runs in a build script"))?;
        let out = std::env::var_os("OUT_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("bundle runs in a build script"))?;
        let dir = manifest.join(dir.as_ref());
        let store = out.join(format!("zgx-bundle-{name}"));
        std::fs::create_dir_all(&store)?;
        let files = sources(&dir)?;
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut code = String::new();
        let _ = writeln!(
            code,
            "zgx_script::Bundle {{ name: {name:?}, root: {:?}, files: &[",
            format!("/zgx-bundle/{name}")
        );
        for (index, (rel, source, bytes)) in files.into_iter().enumerate() {
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = if rel.ends_with(".luau") {
                compile(&bytes).map_err(|e| io::Error::other(format!("{rel}: {e}")))?
            } else {
                bytes
            };
            let stored = store.join(format!("{index}"));
            std::fs::write(&stored, bytes)?;
            let _ = writeln!(
                code,
                "    zgx_script::BundleFile {{ path: {rel:?}, bytes: include_bytes!({:?}) }},",
                stored.display().to_string()
            );
        }
        code.push_str("] }\n");
        std::fs::write(out.join(format!("zgx-bundle-{name}.rs")), code)
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

    /// Where the bundle keeps a package's files: `.packages/<name>/`, a
    /// directory no app's own files have (dotfiles stay out).
    pub(crate) const PACKAGES: &str = ".packages";

    /// The files a bundle of `dir` holds, by path in it, with where each
    /// came from: `dir`'s own, and each package its `zgx.json` lists, under
    /// [`PACKAGES`]; the `.luaurc` aliases into a package name its copy.
    pub(crate) fn sources(dir: &Path) -> io::Result<Vec<(String, PathBuf, Vec<u8>)>> {
        let read = |rel: String, path: PathBuf| Ok((rel, std::fs::read(&path)?, path));
        let mut rels = Vec::new();
        collect(dir, dir, &mut rels)?;
        let mut files = rels
            .into_iter()
            .map(|rel| {
                let path = dir.join(&rel);
                read(rel, path)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let packages = packages(dir)?;
        for (name, folder) in &packages {
            println!("cargo:rerun-if-changed={}", folder.display());
            let mut rels = Vec::new();
            collect(folder, folder, &mut rels)?;
            for rel in rels {
                let path = folder.join(&rel);
                files.push(read(format!("{PACKAGES}/{name}/{rel}"), path)?);
            }
        }
        if let Some(luaurc) = files.iter_mut().find(|(rel, ..)| rel == ".luaurc") {
            luaurc.1 = repoint(dir, &luaurc.1, &packages)?;
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files
            .into_iter()
            .map(|(rel, bytes, path)| (rel, path, bytes))
            .collect())
    }

    /// The packages `dir`'s `zgx.json` lists, by name, as canonical folders.
    fn packages(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
        let config = dir.join("zgx.json");
        let Ok(text) = std::fs::read_to_string(&config) else {
            return Ok(Vec::new());
        };
        println!("cargo:rerun-if-changed={}", config.display());
        let fail = |why: String| io::Error::other(format!("{}: {why}", config.display()));
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
        let Some(listed) = json.get("packages") else {
            return Ok(Vec::new());
        };
        let listed = listed
            .as_object()
            .ok_or_else(|| fail("`packages` is not an object of names and folders".into()))?;
        listed
            .iter()
            .map(|(name, folder)| {
                let folder = folder
                    .as_str()
                    .ok_or_else(|| fail(format!("package `{name}` is not a folder's path")))?;
                let folder = dir
                    .join(folder)
                    .canonicalize()
                    .map_err(|e| fail(format!("package `{name}`: {folder}: {e}")))?;
                Ok((name.to_lowercase(), folder))
            })
            .collect()
    }

    /// `.luaurc` with its aliases into a package pointing at the package's
    /// copy in the bundle.
    fn repoint(dir: &Path, luaurc: &[u8], packages: &[(String, PathBuf)]) -> io::Result<Vec<u8>> {
        let Ok(mut config) = serde_json::from_slice::<serde_json::Value>(luaurc) else {
            return Ok(luaurc.to_vec());
        };
        let Some(aliases) = config.get_mut("aliases").and_then(|a| a.as_object_mut()) else {
            return Ok(luaurc.to_vec());
        };
        let mut changed = false;
        for path in aliases.values_mut() {
            let Some(target) = path.as_str().map(|p| crate::normalize(&dir.join(p))) else {
                continue;
            };
            let target = target.canonicalize().unwrap_or(target);
            for (name, folder) in packages {
                if let Ok(rest) = target.strip_prefix(folder) {
                    let rest = rest.to_string_lossy().replace('\\', "/");
                    let copy = format!("./{PACKAGES}/{name}/{rest}");
                    *path = copy.trim_end_matches('/').into();
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            return Ok(luaurc.to_vec());
        }
        serde_json::to_vec_pretty(&config).map_err(io::Error::other)
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
                Some(
                    "luau"
                        | "json"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "svg"
                        | "gif"
                        | "webp"
                        | "glb"
                        | "gltf"
                        | "bin"
                )
            ) || name == ".luaurc";
            if wanted && let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// A bundle carries the packages its app's zgx.json lists: the bundled
    /// host finds them by name and through `.luaurc`, from the bundle alone.
    #[test]
    fn a_bundle_carries_the_packages_its_app_lists() {
        let base = std::env::temp_dir().join(format!("zgx-bundle-packages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let write = |path: &str, text: &str| {
            let path = base.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("kit/button.luau", r#"return require("./parts/label")"#);
        write("kit/parts/label.luau", r#"return "Save""#);
        write("app/zgx.json", r#"{ "packages": { "kit": "../kit" } }"#);
        write(
            "app/.luaurc",
            r#"{ "aliases": { "parts": "../kit/parts", "own": "./lib" } }"#,
        );
        write("app/lib/name.luau", r#"return "own""#);
        write(
            "app/app.luau",
            r#"assert(require("@kit/button") == "Save", "by the package's name")
assert(require("@parts/label") == "Save", "through .luaurc")
assert(require("@own/name") == "own", "the app's own alias as it was")
return ui.text { "ok" }"#,
        );
        let sources = super::build::sources(&base.join("app")).unwrap();
        let paths: Vec<&str> = sources.iter().map(|(rel, ..)| rel.as_str()).collect();
        assert_eq!(
            paths,
            [
                ".luaurc",
                ".packages/kit/button.luau",
                ".packages/kit/parts/label.luau",
                "app.luau",
                "lib/name.luau",
                "zgx.json"
            ]
        );
        // Nothing on the disk: the bundle alone.
        std::fs::remove_dir_all(&base).unwrap();
        let files: Vec<super::BundleFile> = sources
            .into_iter()
            .map(|(rel, _, bytes)| super::BundleFile {
                bytes: Box::leak(
                    if rel.ends_with(".luau") {
                        super::build::compile(&bytes).unwrap()
                    } else {
                        bytes
                    }
                    .into_boxed_slice(),
                ),
                path: Box::leak(rel.into_boxed_str()),
            })
            .collect();
        let bundle = Box::leak(Box::new(super::Bundle {
            name: "packages",
            root: "/zgx-bundle/packages",
            files: Box::leak(files.into_boxed_slice()),
        }));
        let runtime = zgx::reactive::Runtime::new();
        crate::ScriptHost::bundled(&runtime, bundle, "app.luau")
            .reload()
            .unwrap();
    }
}
