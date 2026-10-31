//! `zgx audit <app.luau-or-dir>` (`zgx-run --audit`): the accessibility
//! audit (`zgx_desktop::accessibility::audit`) over a Luau app, headless.
//! It audits the first screen, then each state the app opens from it: every
//! command it declares (`ui.command`), every button and link on the first
//! screen pressed, and every route its routers declare (`ui.router`; a
//! route's `:params` given `1`), each from a fresh start. A finding is
//! reported once, at the first state showing it, with where the element was
//! built (`app.luau:42`).
use std::collections::HashSet;
use std::path::Path;
use zgx::{
    compose::provide,
    scene::{NodeId, Rect},
    semantics::Role,
};
use zgx_desktop::accessibility::audit;
use zgx_script::{
    ScriptHost,
    testing::{FakeDesktop, TestApp},
};

/// One thing a reader could not use, where it was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// The audit's rule (`no-name`, `focus-trap`, …).
    pub rule: &'static str,
    /// The element, as `zgx inspect` targets it: `#id`, else its role and
    /// name (`button "Save"`), else its node (`@12`).
    pub target: String,
    /// Its accessible role.
    pub role: String,
    /// Its accessible name, or its text.
    pub name: String,
    /// What is missing.
    pub detail: String,
    /// Where the script built it (`app.luau:42`), when known.
    pub source: Option<String>,
    /// The state it was found in: `start`, `command:save`,
    /// `press:#open`, `route:/thread/1`.
    pub state: String,
}

/// What `zgx audit` found of an app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audit {
    /// The states audited, in order.
    pub states: Vec<String>,
    /// What a reader could not use, each once.
    pub findings: Vec<Finding>,
    /// States that could not be reached (a script error), with why.
    pub errors: Vec<(String, String)>,
}

/// A state to reach from a fresh start.
enum Step {
    Start,
    Command(String),
    Press(Rect),
    Route(String),
}

/// Most buttons pressed: an app's first screen rarely has more that open
/// anything, and each is a fresh start.
const PRESSES: usize = 60;

/// The app at `script`, headless, sized as its window, with a desktop whose
/// dialogs answer as cancelled and the inspector recording where elements
/// were built.
fn start(script: &Path) -> Result<(TestApp, zgx::inspect::Inspector), String> {
    let (config, app_host, _) = crate::AppConfig::read(script)?;
    let inspector = zgx::inspect::Inspector::new();
    // Recording (its layout warnings, not its overlay): nodes keep where
    // they were built and their `#id`s.
    inspector.set_warnings(true);
    let desktop = std::rc::Rc::new(FakeDesktop::default());
    let given = (inspector.clone(), desktop.clone());
    let script = script.to_path_buf();
    let mut app = TestApp::wrapped(
        move |runtime| app_host.apply(ScriptHost::new(runtime, script)),
        move |view| {
            let (inspector, desktop) = given;
            provide(
                zgx::platform::PlatformService(desktop),
                provide(inspector, view),
            )
        },
    );
    app.desktop = Some(desktop);
    app.ui
        .scene
        .borrow_mut()
        .resize(config.width as f32, config.height as f32);
    app.frames(2);
    Ok((app, inspector))
}

/// Audit the app at `path` (a script or its directory).
pub fn audit_app(path: &Path) -> Result<Audit, String> {
    let script = crate::script_path(path);
    if !script.is_file() {
        return Err(format!(
            "{} is not a script or an app's directory",
            path.display()
        ));
    }
    let mut out = Audit::default();
    let mut seen = HashSet::new();
    // The first screen, and what it offers to open.
    let (mut app, inspector) = start(&script)?;
    if let Some(error) = app.error() {
        return Err(error);
    }
    let mut steps = vec![(String::from("start"), Step::Start)];
    for command in app
        .host
        .commands()
        .get()
        .into_iter()
        .filter(|c| !c.id.is_empty())
    {
        steps.push((
            format!("command:{}", command.id),
            Step::Command(command.id.clone()),
        ));
    }
    for (target, bounds) in pressable(&app, &inspector).into_iter().take(PRESSES) {
        steps.push((format!("press:{target}"), Step::Press(bounds)));
    }
    for (_, path) in app.host.routes() {
        let path = path
            .split('/')
            .map(|part| if part.starts_with(':') { "1" } else { part })
            .collect::<Vec<_>>()
            .join("/");
        steps.push((format!("route:{path}"), Step::Route(path)));
    }
    for (state, step) in steps {
        let inspector = if matches!(step, Step::Start) {
            inspector.clone()
        } else {
            let (fresh, inspector) = start(&script)?;
            app = fresh;
            inspector
        };
        let reached = match &step {
            Step::Start => true,
            Step::Command(id) => app.host.run_command(id),
            Step::Press(bounds) => {
                zgx_script::driver::click_at(&mut app.ui, *bounds);
                true
            }
            Step::Route(path) => app.host.open_route(path),
        };
        app.settle();
        app.frames(3);
        if let Some(error) = app.error() {
            out.errors.push((state, error));
            continue;
        }
        if !reached {
            out.errors.push((state, "nothing ran".into()));
            continue;
        }
        let report = audit::audit(&mut app.ui);
        out.states.push(state.clone());
        for finding in report.findings {
            let label = finding.node.and_then(|node| inspector.label_of(node));
            let source = label
                .as_ref()
                .and_then(|l| l.source.as_deref())
                .map(str::to_owned);
            let target = target_of(label.as_ref().map(|l| &*l.name), &finding);
            let key = (
                finding.rule,
                source.clone().unwrap_or(target.clone()),
                finding.detail.clone(),
            );
            if seen.insert(key) {
                out.findings.push(Finding {
                    rule: finding.rule,
                    target,
                    role: finding.role,
                    name: finding.name,
                    detail: finding.detail,
                    source,
                    state: state.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// How `zgx inspect` would target the element: `#id` from the inspector's
/// description (`button#save.primary`), else its role and name, else `@n`.
fn target_of(described: Option<&str>, finding: &audit::Finding) -> String {
    if let Some(id) = described
        .and_then(|d| d.split_once('#'))
        .map(|(_, rest)| rest.split(['.', ' ']).next().unwrap_or(rest))
        .filter(|id| !id.is_empty())
    {
        return format!("#{id}");
    }
    if !finding.name.is_empty() {
        return format!("{} {:?}", finding.role.to_lowercase(), finding.name);
    }
    finding.node.map_or_else(
        || "?".into(),
        |n| format!("@{}", zgx_script::driver::number(n)),
    )
}

/// The first screen's buttons and links that can be pressed, with where they
/// are: what may open a modal, a popover, a menu or another view.
fn pressable(app: &TestApp, inspector: &zgx::inspect::Inspector) -> Vec<(String, Rect)> {
    let scene = app.ui.scene.borrow();
    let semantics = app.ui.semantics.borrow();
    let mut out: Vec<(NodeId, String, Rect)> = Vec::new();
    for item in scene.paint_items() {
        let id = item.id;
        let Some(semantic) = semantics.get(id) else {
            continue;
        };
        if !matches!(semantic.role, Role::Button | Role::Link) || semantic.disabled {
            continue;
        }
        let bounds = scene.bounds(id);
        if bounds.width <= 0. || bounds.height <= 0. {
            continue;
        }
        let described = inspector.label_of(id).map(|l| l.name.to_string());
        let target = match described.as_deref().and_then(|d| d.split_once('#')) {
            Some((_, rest)) => format!("#{}", rest.split(['.', ' ']).next().unwrap_or(rest)),
            None if !semantic.label.is_empty() => format!("{:?}", semantic.label),
            None => format!("@{}", zgx_script::driver::number(id)),
        };
        if !out.iter().any(|(other, _, _)| *other == id) {
            out.push((id, target, bounds));
        }
    }
    out.into_iter()
        .map(|(_, target, bounds)| (target, bounds))
        .collect()
}

fn quote(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

impl Audit {
    /// One line of JSON: `{"states":[…],"findings":[{"rule","target","role",
    /// "name","detail","source","state"}],"errors":[{"state","error"}]}`.
    pub fn to_json(&self) -> String {
        let findings: Vec<String> = self
            .findings
            .iter()
            .map(|f| {
                format!(
                    "{{\"rule\":{},\"target\":{},\"role\":{},\"name\":{},\"detail\":{},\"source\":{},\"state\":{}}}",
                    quote(f.rule),
                    quote(&f.target),
                    quote(&f.role),
                    quote(&f.name),
                    quote(&f.detail),
                    f.source.as_deref().map_or("null".into(), quote),
                    quote(&f.state),
                )
            })
            .collect();
        let states: Vec<String> = self.states.iter().map(|s| quote(s)).collect();
        let errors: Vec<String> = self
            .errors
            .iter()
            .map(|(state, error)| {
                format!("{{\"state\":{},\"error\":{}}}", quote(state), quote(error))
            })
            .collect();
        format!(
            "{{\"states\":[{}],\"findings\":[{}],\"errors\":[{}]}}",
            states.join(","),
            findings.join(","),
            errors.join(",")
        )
    }
    /// A table for people: each finding a line, then what was audited.
    pub fn to_human(&self) -> String {
        let mut out = String::new();
        for f in &self.findings {
            out.push_str(&format!(
                "{:<18} {:<24} {:<14} {} ({}{})\n",
                f.rule,
                f.target,
                f.role,
                f.detail,
                f.source
                    .as_deref()
                    .map_or(String::new(), |s| format!("{s}, ")),
                f.state,
            ));
        }
        for (state, error) in &self.errors {
            out.push_str(&format!("not reached     {state}: {error}\n"));
        }
        out.push_str(&format!(
            "{} finding{} in {} state{}\n",
            self.findings.len(),
            if self.findings.len() == 1 { "" } else { "s" },
            self.states.len(),
            if self.states.len() == 1 { "" } else { "s" },
        ));
        out
    }
}
