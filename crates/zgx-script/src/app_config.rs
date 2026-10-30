//! What an app's `zgx.json` gives its script host beyond the window: the
//! packages it requires, what it lets plugins reach, and its plugins with
//! what each may do. One reading for every host that runs a Luau app on its
//! own (`zgx run`, `zgx test`'s `mount()`, `zgx audit`), so an app behaves
//! the same under each.
//!
//! ```json
//! {
//!   "packages": { "kit": "../shared/kit" },
//!   "expose": {
//!     "shared": { "tasks": "read", "focus": "write" },
//!     "actions": ["complete"],
//!     "hooks": { "#tasks": "override", "hook:toolbar.add": "open" }
//!   },
//!   "plugins": {
//!     "focus": { "path": "plugins/focus", "allow": ["shared:focus", "override:#tasks"] }
//!   }
//! }
//! ```
//!
//! The app's side, `expose`, is its whole plugin surface: shared state read
//! only or written, actions (a Rust host's, or the script's `ui.actions`),
//! and targets a plugin may replace or filter (`"override"`; adding before
//! and after is open, `"open"`). Each plugin's side, `allow`, is what it is
//! granted, never more than `expose` offers: `shared:<key>`,
//! `action:<name>`, `override:<target or @app/module>`, and the sandbox's
//! `fetch`, `clipboard`, `files`, `terminals`, `web` and `windows`. A grant
//! beyond what is exposed is an error as the app loads, naming the plugin
//! and the grant.
use crate::{Capability, Plugin, ScriptHost};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The keys this reads from `zgx.json`; a host reading the rest (the
/// window's) accepts these.
pub const KEYS: &[&str] = &["packages", "expose", "plugins"];

/// What `zgx.json` says of an app's host (`read`), to give it (`apply`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppHost {
    /// Module folders outside the app it may require, by name.
    pub packages: Vec<(String, PathBuf)>,
    /// Shared state plugins reach, and whether they may write it.
    shared: Vec<(String, bool)>,
    /// Actions plugins may call.
    actions: Vec<String>,
    /// The plugins, each with its grants.
    plugins: Vec<Plugin>,
}

/// `dir`'s `zgx.json` (nothing if it has none), or what is wrong with it.
pub fn read(dir: &Path) -> Result<AppHost, String> {
    let path = dir.join("zgx.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(AppHost::default());
    };
    let fail = |e: String| format!("{}: {e}", path.display());
    let json: Value = serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
    let object = json
        .as_object()
        .ok_or_else(|| fail("not an object".into()))?;
    from_object(dir, object).map_err(fail)
}

/// The app's host as `zgx.json`'s `object` (already read, from `dir`) says,
/// its other keys left to the caller: for a host reading the rest.
pub fn from_object(dir: &Path, object: &Map<String, Value>) -> Result<AppHost, String> {
    let mut app = AppHost::default();
    if let Some(listed) = object.get("packages") {
        let listed = listed
            .as_object()
            .ok_or("`packages` is not an object of names and folders")?;
        for (name, folder) in listed {
            let folder = folder
                .as_str()
                .ok_or_else(|| format!("package `{name}` is not a folder's path"))?;
            let folder = dir.join(folder);
            if name.is_empty() || name.contains('/') {
                return Err(format!("package `{name}`: a name, without a `/`"));
            }
            if !folder.is_dir() {
                return Err(format!(
                    "package `{name}`: {} is not a folder",
                    folder.display()
                ));
            }
            app.packages.push((name.clone(), folder));
        }
    }
    // The app's side.
    let mut overridable: Vec<String> = Vec::new();
    if let Some(expose) = object.get("expose") {
        let expose = expose
            .as_object()
            .ok_or("`expose` is not an object of shared, actions and hooks")?;
        for key in expose.keys() {
            if !["shared", "actions", "hooks"].contains(&key.as_str()) {
                return Err(format!(
                    "unknown key `expose.{key}` (shared, actions or hooks)"
                ));
            }
        }
        if let Some(shared) = expose.get("shared") {
            let shared = shared
                .as_object()
                .ok_or("`expose.shared` is not an object of keys and \"read\" or \"write\"")?;
            for (key, access) in shared {
                let write = match access.as_str() {
                    Some("read") => false,
                    Some("write") => true,
                    _ => {
                        return Err(format!(
                            "`expose.shared.{key}` is {access}: \"read\" or \"write\""
                        ));
                    }
                };
                app.shared.push((key.clone(), write));
            }
        }
        if let Some(actions) = expose.get("actions") {
            let actions = actions
                .as_array()
                .ok_or("`expose.actions` is not a list of names")?;
            for action in actions {
                let name = action
                    .as_str()
                    .ok_or("`expose.actions` is not a list of names")?;
                app.actions.push(name.to_owned());
            }
        }
        if let Some(hooks) = expose.get("hooks") {
            let hooks = hooks
                .as_object()
                .ok_or("`expose.hooks` is not an object of targets and \"open\" or \"override\"")?;
            for (target, how) in hooks {
                match how.as_str() {
                    Some("open") => {}
                    Some("override") => overridable.push(target.clone()),
                    _ => {
                        return Err(format!(
                            "`expose.hooks.{target}` is {how}: \"open\" or \"override\""
                        ));
                    }
                }
            }
        }
    }
    // Each plugin's side.
    if let Some(plugins) = object.get("plugins") {
        let plugins = plugins
            .as_object()
            .ok_or("`plugins` is not an object of names and { path, allow }")?;
        for (name, plugin) in plugins {
            let plugin = plugin
                .as_object()
                .ok_or_else(|| format!("plugin `{name}` is not {{ path, allow }}"))?;
            for key in plugin.keys() {
                if !["path", "allow"].contains(&key.as_str()) {
                    return Err(format!(
                        "unknown key `plugins.{name}.{key}` (path or allow)"
                    ));
                }
            }
            let path = plugin
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("plugin `{name}` has no `path`"))?;
            let path = dir.join(path);
            let entry = if path.is_dir() {
                path.join("init.luau")
            } else {
                path.clone()
            };
            if !entry.is_file() {
                return Err(format!(
                    "plugin `{name}`: {} is neither a script nor a folder with an init.luau",
                    path.display()
                ));
            }
            let mut grants = Vec::new();
            let allow = match plugin.get("allow") {
                None => Vec::new(),
                Some(allow) => allow
                    .as_array()
                    .ok_or_else(|| format!("`plugins.{name}.allow` is not a list"))?
                    .clone(),
            };
            for grant in allow {
                let grant = grant
                    .as_str()
                    .ok_or_else(|| format!("`plugins.{name}.allow` is not a list of names"))?;
                let beyond = |what: &str, add: &str| {
                    Err(format!(
                        "plugin `{name}` is allowed `{grant}`, which the app doesn't expose \
                         (add {what} to {add})"
                    ))
                };
                grants.push(match grant.split_once(':') {
                    Some(("shared", key)) => {
                        if !app.shared.iter().any(|(k, _)| k == key) {
                            return beyond(&format!("\"{key}\""), "expose.shared");
                        }
                        shared_capability(key)
                    }
                    Some(("action", action)) => {
                        if !app.actions.iter().any(|a| a == action) {
                            return beyond(&format!("\"{action}\""), "expose.actions");
                        }
                        action_capability(action)
                    }
                    Some(("override", target)) => {
                        if !overridable.iter().any(|t| t == target || t == "*") {
                            return beyond(&format!("\"{target}\": \"override\""), "expose.hooks");
                        }
                        Capability::Override(target.to_owned())
                    }
                    _ => match grant {
                        "fetch" => Capability::Fetch,
                        "clipboard" => Capability::Clipboard,
                        "files" => Capability::Files,
                        "terminals" => Capability::Terminals,
                        "web" => Capability::Web,
                        "windows" => Capability::Windows,
                        _ => {
                            return Err(format!(
                                "plugin `{name}`: unknown grant `{grant}` (shared:<key>, \
                                 action:<name>, override:<target>, fetch, clipboard, files, \
                                 terminals, web or windows)"
                            ));
                        }
                    },
                });
            }
            app.plugins.push(Plugin {
                name: name.clone(),
                entry,
                grants,
            });
        }
    }
    Ok(app)
}

/// What a plugin needs to reach exposed shared state `key`.
fn shared_capability(key: &str) -> Capability {
    Capability::Named(format!("shared:{key}"))
}
/// What a plugin needs to call exposed action `name`.
fn action_capability(name: &str) -> Capability {
    Capability::Named(format!("action:{name}"))
}

impl AppHost {
    /// Whether it has plugins.
    pub fn has_plugins(&self) -> bool {
        !self.plugins.is_empty()
    }
    /// `host` given its packages, what the app exposes and its plugins: call
    /// it before the host first loads.
    pub fn apply(&self, host: ScriptHost) -> ScriptHost {
        let host = self
            .packages
            .iter()
            .fold(host, |host, (name, dir)| host.package(name.clone(), dir));
        for (key, write) in &self.shared {
            match write {
                true => host.expose_shared(key, Some(shared_capability(key))),
                false => host.expose_shared_read(key, Some(shared_capability(key))),
            }
        }
        for action in &self.actions {
            host.expose_action(action, Some(action_capability(action)));
        }
        if !self.plugins.is_empty() {
            host.set_plugins(self.plugins.clone());
        }
        host
    }
}
