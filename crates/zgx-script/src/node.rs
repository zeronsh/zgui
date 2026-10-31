//! What a script builds: a description of components, state bindings and
//! animations. Evaluating a script produces this tree once; Rust lowers it to
//! retained zgx views and the script is not consulted again except by event
//! handlers, components as they mount, and explicitly computed (function)
//! properties.
use crate::{
    Vm,
    element::{Ref, RefUd},
    persist::Persist,
    value::{State, Store, Value},
};
use mlua::{FromLua, Function, Lua, Table, UserData};
use std::{
    any::Any,
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};
use zgx::size_calc::SizeCalc;
use zgx::{
    canvas::GradientStop,
    decoration::Corners,
    layout::Length,
    reactive::{Runtime, Signal},
    scene::{BoxShadow, Color, Transform},
    theme::Themed,
};

/// Plain styles for a pseudo-state such as `hover`.
pub(crate) type Styles = Rc<[(Prop, Lit)]>;

#[derive(Clone)]
pub(crate) struct Node {
    /// The script that built this node. Lowered closures hold it strongly;
    /// the Lua heap does not, so a replaced script can be dropped.
    pub owner: Weak<Vm>,
    pub kind: Kind,
    pub props: Vec<(Prop, Source)>,
    pub pseudo: Vec<(Pseudo, Styles)>,
    pub children: Vec<Rc<Node>>,
    pub id: Option<Rc<str>>,
    /// A container's `label`: the name assistive technology reads for it
    /// (a switch, an icon button). Widgets read their own `label`.
    pub label: Option<Rc<str>>,
    /// `role`, `heading`, `selected`, `live`, `description`, `aria_hidden`:
    /// what assistive technology hears beyond the name, boxed (most nodes
    /// say none of it).
    pub a11y: Option<Box<A11y>>,
    pub classes: Rc<[Rc<str>]>,
    pub refs: Vec<Rc<Ref>>,
    /// Handlers, boxed: most nodes have none (read them with `events()`).
    pub events: Option<Box<Events>>,
    pub focusable: Option<bool>,
    /// `pointer_events = "none"`: the pointer passes through. A literal, a
    /// state or a function (true or `"auto"` stops it, false or `"none"`
    /// lets it through).
    pub pointer_events: Option<Source>,
    pub disabled: Option<Source>,
    /// `layout = "height"`: the node's size follows its content, animated.
    pub layout: Option<LayoutAnim>,
    /// `layout_id = "selection"`: a node mounting as another with the same
    /// id leaves moves (and scales) from where that one was.
    pub layout_id: Option<Rc<str>>,
    /// `mask = { bitmap, w, h, x, y }`: the content shows through the
    /// bitmap's alpha, drawn at `w`×`h` from (`x`, `y`).
    pub mask: Option<Mask>,
    /// `bg_shader`, `shader_params`, `layer_shader` and `layer_params`
    /// (boxed: few nodes have any).
    pub shaders: Option<Box<Shaders>>,
    /// `window_drag = true`: a press on it moves the window.
    pub window_drag: bool,
    /// `hook = false` or `private = true`: no plugin may target it
    /// (`ui.contribute { target }`), whatever matches it.
    pub private: bool,
    /// `selectable = true`: the pointer selects the text in it (`false`
    /// keeps a part of a selectable node out).
    pub selectable: Option<bool>,
    /// `selection_color`, `selected_text_color`: few nodes have them.
    pub selection_colors: Option<Box<[Option<Source>; 2]>>,
    /// `tooltip = "text" | node`, `tooltip_placement`: shown over it while
    /// the pointer rests on it. Boxed: few nodes have one.
    pub tooltip: Option<Box<Tooltip>>,
    /// Where the script created it (`app.luau:12`), recorded only while the
    /// layout inspector or layout warnings want it.
    pub source: Option<Rc<str>>,
    /// `theme = …`: the subtree's theme scope.
    pub theme: Option<ThemeSpec>,
    /// `provide = { name = value }`: values the components and rows built
    /// in this subtree read with `ui.use_context(name)`.
    pub provide: Option<Rc<[Given]>>,
    /// `variants = …` chosen by state, with interaction styles: they follow
    /// it (see `crate::variants`).
    pub variant_interaction: Option<Rc<crate::variants::Selection>>,
}
/// What `theme = …` gives a subtree: fixed, or a state or function that
/// computes it (the scope follows).
#[derive(Clone)]
pub(crate) enum ThemeSpec {
    Fixed(zgx::theme::ThemeOverride),
    State(Signal<Value>),
    Computed(Function),
    /// `selection_radius = …`: `$selection.radius` for the subtree, inside
    /// the `theme` it is given beside, if any.
    SelectionRadius(Box<(Option<ThemeSpec>, Source)>),
}
/// An alpha mask over a node's content; the offset animates (`maskX`), or
/// sweeps on its own (`sweep`).
#[derive(Clone)]
pub(crate) struct Mask {
    pub image: Arc<zgx::image::ImageData>,
    pub size: (f32, f32),
    pub offset: (f32, f32),
    pub sweep: Option<Sweep>,
}
/// A mask moving `by` from its offset every `period`, over and over, at
/// `rate` steps a second at most (`NodeHandle::sweep_mask`).
#[derive(Clone)]
pub(crate) struct Sweep {
    pub by: (f32, f32),
    pub period: std::time::Duration,
    pub rate: f64,
}
/// A web view's page, controller and handlers.
#[derive(Clone)]
pub(crate) struct WebSpec {
    /// Shared with `ui.web()` when given, so the page outlives this node.
    pub web: zgx::web_view::WebView,
    pub src: Option<Source>,
    pub html: Option<(String, Option<String>)>,
    pub on_navigate: Option<Function>,
    pub on_load: Option<Function>,
    pub on_title: Option<Function>,
    pub on_message: Option<Function>,
    pub on_error: Option<Function>,
    pub on_new_window: Option<Function>,
}

/// `ui.web(opts)`: a web view's controller, usable before and after its
/// view mounts. `ui.web(id, opts)` keeps it in the host's registry by `id`,
/// across reloads, until `web:close()`.
#[derive(Clone)]
pub(crate) struct WebUd(pub zgx::web_view::WebView, pub Option<WebKey>);
#[derive(Clone)]
pub(crate) struct WebKey {
    id: String,
    webs: std::rc::Weak<Webs>,
}
/// The host's web pages by id (`ui.web(id, opts)`): they outlive reloads.
#[derive(Default)]
pub(crate) struct Webs(RefCell<std::collections::HashMap<String, zgx::web_view::WebView>>);

/// Installs `ui.web`: `ui.web(opts)` makes a page for this run;
/// `ui.web(id, opts)` returns the host's page for `id`, making it (with
/// `opts`) the first time. A plugin's pages are its own: its ids and its
/// stores (cookies, storage) are apart from the app's and other plugins',
/// so `web` lets it show pages, not reach the user's.
pub(crate) fn install_web(lua: &Lua, ui: &Table, webs: Rc<Webs>) -> mlua::Result<()> {
    ui.set(
        "web",
        lua.create_function(move |lua, (first, opts): (mlua::Value, Option<Table>)| {
            let plugin = {
                let env = env(lua)?;
                env.may(crate::Capability::Web, "ui.web")?;
                match &*env.grants {
                    crate::Grants::All => None,
                    crate::Grants::Only(_) => Some(env.owner.clone()),
                }
            };
            let (id, opts) = match first {
                mlua::Value::String(id) => (Some(id.to_str()?.to_owned()), opts),
                mlua::Value::Table(opts) => (None, Some(opts)),
                mlua::Value::Nil => (None, None),
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "ui.web takes an id or options, not {}",
                        other.type_name()
                    )));
                }
            };
            let id = match (&plugin, id) {
                (Some(plugin), Some(id)) => Some(format!("plugin:{plugin}/{id}")),
                (_, id) => id,
            };
            let make = |lua: &Lua| -> mlua::Result<zgx::web_view::WebView> {
                let mut options: zgx::web_view::WebViewOptions = match &opts {
                    Some(t) => web_options(t)?,
                    None => Default::default(),
                };
                if let Some(plugin) = &plugin {
                    use zgx::web_view::WebProfile;
                    options.profile = match options.profile {
                        WebProfile::Default => WebProfile::Named(format!("plugin:{plugin}")),
                        WebProfile::Named(name) => {
                            WebProfile::Named(format!("plugin:{plugin}/{name}"))
                        }
                        WebProfile::Ephemeral => WebProfile::Ephemeral,
                    };
                }
                Ok(zgx::web_view::WebView::new(&env(lua)?.runtime, options))
            };
            let Some(id) = id else {
                return Ok(WebUd(make(lua)?, None));
            };
            let existing = webs.0.borrow().get(&id).cloned();
            let web = match existing {
                Some(web) => web,
                None => {
                    let web = make(lua)?;
                    webs.0.borrow_mut().insert(id.clone(), web.clone());
                    web
                }
            };
            Ok(WebUd(
                web,
                Some(WebKey {
                    id,
                    webs: Rc::downgrade(&webs),
                }),
            ))
        })?,
    )
}
impl UserData for WebUd {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Reads are tracked: a text or computed that reads them follows them.
        fields.add_field_method_get("url", |_, w| Ok(w.0.url()));
        fields.add_field_method_get("title", |_, w| Ok(w.0.title()));
        fields.add_field_method_get("loading", |_, w| Ok(w.0.loading()));
        fields.add_field_method_get("progress", |_, w| Ok(w.0.progress()));
        fields.add_field_method_get("can_go_back", |_, w| Ok(w.0.can_go_back()));
        fields.add_field_method_get("can_go_forward", |_, w| Ok(w.0.can_go_forward()));
        fields.add_field_method_get("focused", |_, w| Ok(w.0.focused()));
        fields.add_field_method_get("error", |_, w| Ok(w.0.state().get().error));
        // `{ x, y }` in points from the page's top left, or nil outside it.
        fields.add_field_method_get("pointer", |lua, w| {
            w.0.pointer()
                .map(|(x, y)| {
                    let table = lua.create_table()?;
                    table.set("x", x)?;
                    table.set("y", y)?;
                    Ok(table)
                })
                .transpose()
        });
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("load", |_, w, url: String| {
            w.0.load_url(url);
            Ok(())
        });
        methods.add_method("html", |_, w, (html, base): (String, Option<String>)| {
            w.0.load_html(html, base.as_deref());
            Ok(())
        });
        methods.add_method("back", |_, w, ()| {
            w.0.go_back();
            Ok(())
        });
        // The page ends now (not when the script's handles are collected,
        // which an idle script may never do); one from `ui.web(id)` leaves
        // the registry too.
        methods.add_method("close", |_, w, ()| {
            if let Some(key) = &w.1
                && let Some(webs) = key.webs.upgrade()
            {
                webs.0.borrow_mut().remove(&key.id);
            }
            w.0.close();
            Ok(())
        });
        methods.add_method("forward", |_, w, ()| {
            w.0.go_forward();
            Ok(())
        });
        methods.add_method("reload", |_, w, ()| {
            w.0.reload();
            Ok(())
        });
        methods.add_method("stop", |_, w, ()| {
            w.0.stop();
            Ok(())
        });
        methods.add_method("focus", |_, w, ()| {
            w.0.focus();
            Ok(())
        });
        methods.add_method("blur", |_, w, ()| {
            w.0.blur();
            Ok(())
        });
        methods.add_method("devtools", |_, w, ()| {
            w.0.open_devtools();
            Ok(())
        });
        methods.add_method("zoom", |_, w, zoom: f64| {
            w.0.set_zoom(zoom);
            Ok(())
        });
        methods.add_method("run", |_, w, js: String| {
            w.0.run_js(js);
            Ok(())
        });
        // `web:eval(js, function(value, err) … end)`: the result as Luau data.
        methods.add_method("eval", |lua, w, (js, reply): (String, Option<Function>)| {
            let vm = env(lua)?.vm.clone();
            w.0.evaluate_js(js, move |result| {
                let (Some(reply), Some(vm)) = (reply, vm.upgrade()) else {
                    return;
                };
                let outcome = match result {
                    Ok(json) => vm.call::<()>(&reply, (json_value(&json), mlua::Value::Nil)),
                    Err(error) => vm.call::<()>(&reply, (Value::Nil, error)),
                };
                if let Err(e) = outcome {
                    vm.report(e);
                }
            });
            Ok(())
        });
    }
}
/// JSON as script data (objects as maps, arrays as lists).
pub(crate) fn json_value(json: &str) -> Value {
    serde_json::from_str(json)
        .map(from_json)
        .unwrap_or_else(|_| Value::Str(json.into()))
}
/// Parsed JSON as a value.
pub(crate) fn from_json(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => Value::Num(n.as_f64().unwrap_or(0.)),
        serde_json::Value::String(s) => Value::Str(s.into()),
        serde_json::Value::Array(a) => {
            Value::List(a.into_iter().map(from_json).collect::<Vec<_>>().into())
        }
        // Empty, still an object: a script sending it back sends `{}`.
        serde_json::Value::Object(o) if o.is_empty() => Value::Map(Rc::new([])),
        serde_json::Value::Object(o) => {
            let mut fields: Vec<(Rc<str>, Value)> = o
                .into_iter()
                .map(|(k, v)| (k.into(), from_json(v)))
                .collect();
            // Sorted, as maps are, whatever order serde_json keeps.
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Map(fields.into())
        }
    }
}
/// A controller's options from a script table.
fn web_options(table: &Table) -> mlua::Result<zgx::web_view::WebViewOptions> {
    use zgx::web_view::{WebProfile, WebViewOptions};
    let defaults = WebViewOptions::default();
    let profile = match table.get::<Option<String>>("profile")?.as_deref() {
        None | Some("default") => WebProfile::Default,
        Some("ephemeral") => WebProfile::Ephemeral,
        Some(name) => WebProfile::Named(name.to_owned()),
    };
    Ok(WebViewOptions {
        transparent: table
            .get::<Option<bool>>("transparent")?
            .unwrap_or(defaults.transparent),
        profile,
        inspectable: table
            .get::<Option<bool>>("inspectable")?
            .unwrap_or(defaults.inspectable),
        user_agent: table.get::<Option<String>>("user_agent")?,
        messages: table
            .get::<Option<bool>>("messages")?
            .unwrap_or(defaults.messages),
        new_windows: table
            .get::<Option<bool>>("new_windows")?
            .unwrap_or(defaults.new_windows),
        // Scripts and schemes are set from Rust, not from Luau.
        ..defaults
    })
}

/// `ui.shader(source, { fps })`: a WGSL fill, compiled once per source.
#[derive(Clone)]
pub(crate) struct ShaderUd(pub Arc<zgx::decoration::FillShader>);
impl UserData for ShaderUd {}
/// A node's shaders and their literal parameters.
#[derive(Clone, Default)]
pub(crate) struct Shaders {
    /// `bg_shader = ui.shader(...)`: the box is filled by a WGSL function.
    pub fill: Option<Arc<zgx::decoration::FillShader>>,
    /// `shader_params`: the fill's sixteen values, as four vec4s.
    pub params: Option<[[f32; 4]; 4]>,
    /// Parameters given as theme colours (`"$accent"`): resolved where the
    /// node is styled, and again when the theme changes.
    pub tokens: [Option<zgx::theme::TokenId>; 4],
    /// `layer_shader = ui.layer_shader(...)`: the subtree composites
    /// through a WGSL function.
    pub layer: Option<Arc<zgx::decoration::FillShader>>,
    /// `layer_params`: the layer shader's sixteen values, as four vec4s.
    pub layer_params: Option<[[f32; 4]; 4]>,
}
/// `ui.layer_shader(source, opts)`: a compiled layer shader, for
/// `layer_shader` (see `zgx::scene::LayerShader`).
pub(crate) struct LayerShaderUd(pub Arc<zgx::decoration::FillShader>);
impl UserData for LayerShaderUd {}

/// `shader_params` and `layer_params`: up to four vec4s, lists of numbers
/// or colors (0 to 1); theme colours (`"$accent"`) bind to `tokens`' slots
/// where it has them.
fn shader_params(
    lua: &Lua,
    value: &mlua::Value,
    name: &str,
    mut tokens: Option<&mut [Option<zgx::theme::TokenId>; 4]>,
) -> mlua::Result<[[f32; 4]; 4]> {
    let mlua::Value::Table(list) = value else {
        return Err(mlua::Error::runtime(format!(
            "`{name}` takes a list of up to four vec4s"
        )));
    };
    let mut params = [[0.; 4]; 4];
    for (i, entry) in list.sequence_values::<mlua::Value>().enumerate().take(4) {
        params[i] = match entry? {
            mlua::Value::Table(v) => {
                let mut out = [0.; 4];
                for (j, n) in v.sequence_values::<f32>().enumerate().take(4) {
                    out[j] = n?;
                }
                out
            }
            mlua::Value::String(token)
                if token
                    .to_str()
                    .is_ok_and(|t| t.starts_with('$') && t.len() > 1) =>
            {
                let Some(tokens) = tokens.as_deref_mut() else {
                    return Err(mlua::Error::runtime(format!(
                        "`{name}` takes numbers or colors, not theme tokens"
                    )));
                };
                let token = token.to_string_lossy();
                tokens[i] = Some(zgx::theme::token(&token[1..]));
                [0.; 4]
            }
            other => {
                let value: Value = lua.unpack(other)?;
                let c = color_of(&value).ok_or_else(|| {
                    mlua::Error::runtime("a shader parameter is a list of numbers or a color")
                })?;
                [
                    c.0 as f32 / 255.,
                    c.1 as f32 / 255.,
                    c.2 as f32 / 255.,
                    c.3 as f32 / 255.,
                ]
            }
        };
    }
    Ok(params)
}
/// `ui.bitmap(w, h, fn)`: pixels made by a script, for masks.
#[derive(Clone)]
pub(crate) struct BitmapUd(pub Arc<zgx::image::ImageData>);
impl UserData for BitmapUd {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("w", |_, b| Ok(b.0.width()));
        fields.add_field_method_get("h", |_, b| Ok(b.0.height()));
    }
}

/// How a node's size follows its content.
#[derive(Clone)]
pub(crate) struct LayoutAnim {
    pub width: bool,
    pub height: bool,
    /// `layout = "position"`: moves layout makes animate, on the compositor.
    pub position: bool,
    /// ms; a spring's own settling time when `None`.
    pub duration: Option<f64>,
    pub ease: Option<crate::anime::ease::Ease>,
    /// Grows from zero when it first mounts, instead of starting at its size.
    pub enter: bool,
    /// Clips the content to the animated size.
    pub clip: bool,
    /// `hit = "target"`: while an animation moves it, it takes hits where
    /// it's going (`Scene::set_hit_target`). Read late (`late_field`) and
    /// kept here, where nodes without `layout` lower past it.
    pub hit: bool,
}
impl Node {
    /// The hook the app declares here (`hook = "name"`), if any.
    pub fn hook(&self) -> Option<&str> {
        self.a11y.as_ref()?.hook.as_deref()
    }
    pub(crate) fn new(owner: Weak<Vm>, kind: Kind) -> Self {
        Self {
            owner,
            kind,
            props: Vec::new(),
            pseudo: Vec::new(),
            children: Vec::new(),
            id: None,
            label: None,
            a11y: None,
            classes: Rc::new([]),
            refs: Vec::new(),
            events: None,
            focusable: None,
            pointer_events: None,
            disabled: None,
            layout: None,
            layout_id: None,
            mask: None,
            shaders: None,
            window_drag: false,
            private: false,
            selectable: None,
            selection_colors: None,
            tooltip: None,
            source: None,
            theme: None,
            provide: None,
            variant_interaction: None,
        }
    }
    /// How the layout inspector names it: its kind, `#id` and `.class`es.
    /// Whether it says which way it runs (`dir`, `direction`).
    pub fn sets_direction(&self) -> bool {
        self.props
            .iter()
            .any(|(prop, _)| matches!(prop, Prop::Dir | Prop::Direction))
    }
    pub fn label(&self) -> String {
        let mut label = self.tag().to_string();
        if let Some(id) = &self.id {
            label.push_str(" #");
            label.push_str(id);
        }
        for class in self.classes.iter() {
            label.push_str(" .");
            label.push_str(class);
        }
        label
    }
    /// Whether scripts can reach this node (selectors, refs, `on_mount`):
    /// only those carry an element and its animated values.
    /// Whether layout moves animate: `layout = "position"` or a `layout_id`.
    pub fn tracks_layout(&self) -> bool {
        self.layout_id.is_some() || self.layout.as_ref().is_some_and(|l| l.position)
    }
    /// Whether the node has a box whose size animates (`layout = "height"`).
    pub fn sizes_layout(&self) -> bool {
        self.layout.as_ref().is_some_and(|l| l.width || l.height)
    }
    pub fn targetable(&self) -> bool {
        self.id.is_some()
            || !self.classes.is_empty()
            || !self.refs.is_empty()
            || self.events().mount.is_some()
            || self.events().resize.is_some()
            || self.events().visible.is_some()
    }
    pub fn events(&self) -> &Events {
        self.events.as_deref().unwrap_or(&Events::NONE)
    }
    /// Animations, scroll offsets, widget bindings and overlays need a
    /// component context.
    pub fn needs_context(&self) -> bool {
        matches!(
            self.kind,
            Kind::Scroll { .. }
                | Kind::Checkbox { .. }
                | Kind::Input { .. }
                | Kind::Progress { .. }
                | Kind::Slider { .. }
                | Kind::Split { .. }
                | Kind::List { .. }
                | Kind::Modal { .. }
                | Kind::Popover { .. }
                | Kind::Menu { .. }
                | Kind::Submenu { .. }
                | Kind::Presence { .. }
                | Kind::Each {
                    motion: Some(_),
                    ..
                }
        ) || self.kind.library_needs_context()
            || self.kind.is_scene3d()
            || self.props.iter().any(|(_, s)| matches!(s, Source::Anim(_)))
            // An icon's tint may be a theme token: it reads the scope.
            || matches!(self.kind, Kind::Svg { .. })
                && self.props.iter().any(|(p, _)| *p == Prop::Tint)
            // A sweeping mask runs on the component's frame clock.
            || self.mask.as_ref().is_some_and(|m| m.sweep.is_some())
    }
    /// The element kind selectors match (`button`, `text`…).
    pub fn tag(&self) -> &'static str {
        match self.kind {
            Kind::Column => "column",
            Kind::Row => "row",
            Kind::Div => "div",
            Kind::Overlay => "overlay",
            Kind::Button => "button",
            Kind::Scroll { .. } => "scroll",
            Kind::Text(_) => "text",
            Kind::Markdown { .. } => "markdown_view",
            Kind::Rich(_) => "rich",
            Kind::Image { .. } => "image",
            Kind::Svg { .. } => "svg",
            Kind::Canvas(..) => "canvas",
            #[cfg(feature = "scene3d")]
            Kind::Scene3d(_) => "scene3d",
            Kind::WebView(_) => "webview",
            Kind::Terminal(_) => "terminal",
            Kind::Checkbox { .. } => "checkbox",
            Kind::Input { .. } => "input",
            Kind::Progress { .. } => "progress",
            Kind::Slider { .. } => "slider",
            Kind::Split { .. } => "split",
            #[cfg(feature = "components")]
            Kind::Library(ref library) => library.tag(),
            Kind::Show { .. } => "show",
            Kind::Each { .. } => "each",
            Kind::List { .. } => "list",
            Kind::Switch { .. } => "switch",
            Kind::Modal { .. } => "modal",
            Kind::Popover { .. } => "popover",
            Kind::Menu { .. } => "menu",
            Kind::MenuItem(..) => "menu_item",
            Kind::Submenu { .. } => "submenu",
            Kind::Component { .. } => "component",
            Kind::Presence { .. } => "presence",
            Kind::Slot { .. } => "slot",
        }
    }
}

/// Script handlers for input and lifecycle events.
#[derive(Default, Clone)]
pub(crate) struct Events {
    pub click: Option<Handler>,
    /// Primary pointer-down: for what must start before release, such as
    /// dragging the window.
    pub press: Option<Handler>,
    pub release: Option<Handler>,
    /// Handlers few nodes have: `on_aux_click`, `on_navigate`.
    pub rare: Option<Box<RareEvents>>,
    /// `fn(inside)` on pointer enter and leave.
    pub hover: Option<Handler>,
    /// `fn({ x, y, local_x, local_y })`.
    pub pointer_move: Option<Handler>,
    /// `fn({ key, shift, ctrl, alt, meta, repeat })`; returning true
    /// consumes the key.
    pub key: Option<Handler>,
    pub key_up: Option<Handler>,
    pub focus: Option<Handler>,
    pub blur: Option<Handler>,
    /// `fn({ dx, dy, x, y, local_x, local_y, shift, ctrl, alt, meta })`.
    pub wheel: Option<Handler>,
    /// `fn({ scale, x, y, local_x, local_y, … })`: a trackpad pinch.
    pub pinch: Option<Handler>,
    /// `zoomable = state`: its zoom, bound to a number state the user
    /// changes with Control and the wheel or a pinch over it
    /// (`View::zoomable`).
    pub zoomable: Option<Signal<Value>>,
    /// `fn(element)` once mounted: start animations here.
    pub mount: Option<Function>,
    /// `fn({ w, h }, element)` when layout gives the node a new size (and
    /// once at first layout), as a ResizeObserver reports.
    pub resize: Option<Function>,
    /// `fn(visible, element)` when any of the node comes into view or all
    /// of it leaves, and once at first layout.
    pub visible: Option<Function>,
    /// Drag and drop within the window (payloads are script values):
    /// `on_drag() -> payload` starts a drag after a 4-point move;
    /// `drag_preview(payload) -> node` follows the pointer;
    /// `on_drag_end(payload, accepted)` on the source when it ends.
    pub drag: Option<Function>,
    pub drag_preview: Option<Function>,
    /// `drag_preview_anchor = { 0.5, 0 }`, `drag_preview_offset = { 0, 16 }`:
    /// that point of the preview (fractions of its size) that far from the
    /// pointer, rather than the preview at the grab point.
    pub drag_preview_anchor: Option<(f32, f32)>,
    pub drag_preview_offset: Option<(f32, f32)>,
    pub drag_end: Option<Function>,
    /// `drag_export(payload) -> { text, url, files }`: what the drag carries
    /// to other apps as it leaves the window (macOS).
    pub drag_export: Option<Function>,
    /// `on_drag_outside(payload, { x, y, grab_x, grab_y })`: released outside
    /// every window of the app (macOS), as a tab torn off its window is.
    pub drag_outside: Option<Function>,
    /// `drag_moves_window(payload) -> boolean`: the drag moves its window
    /// instead, with no preview (macOS), as a window's only tab does.
    pub drag_moves_window: Option<Function>,
    /// On a destination: `drop_when(payload) -> bool` (default: all),
    /// `on_drag_over(payload, { x, y, local_x, local_y, w, h })` while an
    /// accepted drag hovers, `on_drag_leave(payload)` when it leaves,
    /// `on_drop(payload, { ... })` on release.
    pub drop: Option<Function>,
    pub drop_when: Option<Function>,
    pub drag_over: Option<Function>,
    pub drag_leave: Option<Function>,
    /// `on_drag_spring(payload)`: an accepted drag has rested on it for the
    /// platform's spring-loading delay, or Space was pressed (returning
    /// `false` declines: it does not spring open).
    pub drag_spring: Option<Function>,
    /// `fn({ x, y, local_x, local_y }) -> { menu_item, … }`: a secondary
    /// click (control-click on macOS) shows what it returns as a menu at
    /// the pointer; nothing returned, no menu.
    pub context_menu: Option<Function>,
    /// `context_menu_style = { bg, rounded, p, backdrop_blur, … }`: the
    /// menu panel's styles (plain values).
    pub context_menu_style: Option<Styles>,
    /// `context_menu_below = true`: the menu opens under the node (a tab's),
    /// not at the pointer.
    pub context_menu_below: bool,
    /// Whether any input handler is set (`any_input`): set with them, so
    /// every node reads one flag rather than a dozen handlers.
    pub input: bool,
}
/// Handlers few nodes have, boxed: a node's `Events` stay small, and one
/// check covers them all for a node with none (`with_rare`).
#[derive(Default, Clone)]
pub(crate) struct RareEvents {
    /// `fn({ button, x, y, local_x, local_y, … })`: a middle (or other
    /// extra) button pressed and released on the node, as the DOM's
    /// `auxclick`; the innermost node with one takes it.
    pub aux_click: Option<Handler>,
    /// `fn({ direction })`: a step back or forward ("back", "forward") the
    /// platform's gesture for it takes over the node (a trackpad's swipe
    /// between pages, macOS); the nearest node with one takes it.
    pub navigate: Option<Handler>,
    /// `fn(files, { x, y, local_x, local_y })`: files dropped on it from
    /// outside the window (Finder, Explorer, a file manager), each a
    /// `File`; the innermost node with one takes them.
    pub file_drop: Option<Handler>,
    /// `fn(over)`: files dragged from outside the window come over it
    /// (`true`), then leave it, are dropped or the drag ends (`false`).
    pub file_hover: Option<Handler>,
    /// `fn({ x, y, local_x, local_y, … })`: the primary button clicked twice
    /// on one spot, as the DOM's `dblclick`, heard as the second is let go;
    /// the innermost node with one takes it.
    pub double_click: Option<Handler>,
}
impl Kind {
    /// Regions that become a box of any number of children, laid out as
    /// their parent lays out its own (`show`, `switch` and `presence` lay
    /// out as the one view they show instead).
    pub fn flows(&self) -> bool {
        matches!(self, Kind::Each { .. } | Kind::Slot { .. })
    }
}
impl Events {
    const NONE: Events = Events {
        click: None,
        press: None,
        release: None,
        rare: None,
        hover: None,
        pointer_move: None,
        key: None,
        key_up: None,
        focus: None,
        blur: None,
        wheel: None,
        pinch: None,
        zoomable: None,
        mount: None,
        resize: None,
        visible: None,
        drag: None,
        drag_preview: None,
        drag_preview_anchor: None,
        drag_preview_offset: None,
        drag_end: None,
        drag_export: None,
        drag_outside: None,
        drag_moves_window: None,
        drop: None,
        drop_when: None,
        drag_over: None,
        drag_leave: None,
        drag_spring: None,
        context_menu: None,
        context_menu_style: None,
        context_menu_below: false,
        input: false,
    };
    pub fn any_input(&self) -> bool {
        self.input
    }
    pub fn any_drag(&self) -> bool {
        self.drag.is_some()
            || self.drop.is_some()
            || self.drag_over.is_some()
            || self.drag_leave.is_some()
            || self.drag_spring.is_some()
    }
}

/// What few lists set (`Kind::List`'s `rare`).
#[derive(Clone, Default)]
pub(crate) struct ListRare {
    /// Rows that stick to the top while their section scrolls by (with
    /// `heights`): their indices, from 1, in order.
    pub(crate) sticky: Option<Heights>,
    /// How far below the top they stick (CSS's sticky `top`).
    pub(crate) sticky_top: f32,
    /// `row_text = function(i) … end`: the plain text of row `i`, what a
    /// copy of a selection over rows the list has not mounted takes
    /// (without it, the text each row's description shows).
    pub(crate) row_text: Option<Function>,
    /// `on_end` (with `end_distance`): what its end coming in reach calls.
    pub(crate) end: Option<ListEnd>,
    /// `on_start` and `start_distance` (200 points unless set): what its
    /// start coming in reach calls. Taken as `fill` walks the table.
    pub(crate) start: Option<Function>,
    pub(crate) start_distance: Option<f32>,
    /// `at_end = state`: kept saying whether it rests at its end.
    pub(crate) at_end: Option<Signal<Value>>,
    /// `overscroll = "contain"`: a wheel over it stays with it.
    pub(crate) contain: bool,
}

/// `ui.list`'s `on_end`, called as its end comes within `distance` points.
#[derive(Clone)]
pub(crate) struct ListEnd {
    pub(crate) on_end: Function,
    pub(crate) distance: f32,
}

/// An input's options few inputs set (`Kind::Input`'s `rare`).
#[derive(Clone, Default)]
pub(crate) struct InputRare {
    /// `placeholder_color`: the placeholder's colour.
    pub(crate) placeholder_color: Option<Source>,
    /// `caret_color` (the text's if unset) and `caret_width` (points; the
    /// app's if unset).
    pub(crate) caret_color: Option<Source>,
    pub(crate) caret_width: Option<f32>,
    /// `spellcheck = true` (or a state or function of one): the system's
    /// checker marks misspellings.
    pub(crate) spellcheck: Option<Source>,
    /// `active_descendant = id` (or a state or function of one): the
    /// suggestion it is on, for assistive technology, focus kept here.
    pub(crate) active_descendant: Option<Source>,
    /// `on_paste`: hears a person's paste first (`View::on_paste`).
    pub(crate) on_paste: Option<Function>,
    /// `overscroll = "contain"`: a wheel over it stays with it.
    pub(crate) contain: bool,
    /// `read_only = true` (or a state or function of one): selectable and
    /// focusable, the person's edits refused; `value` still sets it.
    pub(crate) read_only: Option<Source>,
    /// `autocorrect = true` and `smart_punctuation = true`: the system's
    /// substitutions as it is typed (`View::autocorrect`, `smart_punctuation`).
    pub(crate) autocorrect: bool,
    pub(crate) smart_punctuation: bool,
}

#[derive(Clone)]
pub(crate) enum Kind {
    Column,
    Row,
    Div,
    Overlay,
    Button,
    /// `offset`: state the scroll position follows and reports, in points.
    /// `follow`: stays at the end while the content grows, until scrolled away.
    Scroll {
        offset: Option<Signal<Value>>,
        horizontal: bool,
        follow: bool,
        /// `scrollbar = true`: an interactive scrollbar over its edge.
        scrollbar: bool,
        /// `overscroll = "contain"`: a wheel over it stays with it.
        contain: bool,
    },
    Text(Text),
    /// `ui.markdown_view { text }`: markdown drawn, parsed as the text
    /// changes and only its changed blocks built again.
    Markdown {
        text: Text,
        options: crate::markdown::view::ViewOptions,
    },
    Rich(Vec<Span>),
    Image {
        label: String,
        src: Source,
        /// Shown in its place while a remote picture loads, and if it fails.
        fallback: Option<Rc<Node>>,
    },
    Svg {
        label: String,
        src: Source,
    },
    /// `fn(canvas, width, height)`, run again when its size or the state it
    /// reads changes; with its `view` and `grid`, if any.
    /// `draw` may be absent when `each` draws it.
    Canvas(Option<Function>, Option<Box<CanvasOptions>>),
    /// `ui.scene3d { … }`: a 3D scene (zgx-3d) in a GPU view.
    #[cfg(feature = "scene3d")]
    Scene3d(Rc<crate::scene3d::Scene3dSpec>),
    /// `ui.webview { ... }`: a native web view laid out like a div.
    WebView(Box<WebSpec>),
    /// `ui.terminal { term = ui.term(id) }`: a terminal session, shown
    /// where the node is.
    Terminal(zgx::terminal::Terminal),
    Checkbox {
        label: String,
        value: Signal<Value>,
        /// `mixed = state`: neither on nor off while true.
        mixed: Option<Signal<Value>>,
    },
    Input {
        label: String,
        value: InputValue,
        multiline: bool,
        /// Shown while the value is empty (`placeholder`).
        placeholder: Option<Source>,
        /// What few inputs set (`InputRare`), boxed: every node is as large
        /// as its largest kind.
        rare: Option<Box<InputRare>>,
        /// `select_on_focus = true`: focusing it selects all its text.
        select_on_focus: bool,
        /// `line_numbers = true`: a textarea numbers its lines.
        line_numbers: bool,
        /// What a code editor adds to a textarea (`highlight`, …).
        code: Option<Rc<CodeProps>>,
        /// `secure = true`: a password field (masked, never copied).
        secure: bool,
        /// `inputmode = "email"` (HTML's): what kind of text it takes, a
        /// hint to touch keyboards and input methods.
        input_mode: zgx::text_edit::InputMode,
    },
    Progress {
        label: String,
        value: Signal<Value>,
    },
    Slider {
        label: String,
        value: Signal<Value>,
        min: f32,
        max: f32,
        /// `step`: what an arrow moves, and what a drag lands on.
        step: Option<f32>,
    },
    /// A component of the library (`zgx-components`, the `components`
    /// feature): a data grid, a tree, tabs, a select…
    #[cfg(feature = "components")]
    Library(Box<crate::library::Library>),
    /// `ui.split { size = state, a, b }`: two panes and a divider that
    /// resizes the first.
    Split {
        size: Signal<Value>,
        options: zgx::compose::SplitOptions,
    },
    Show {
        when: Source,
        then: Rc<Node>,
        otherwise: Option<Rc<Node>>,
    },
    Each {
        items: Signal<Value>,
        build: Function,
        key: Option<Function>,
        /// `animate`, `enter` or `exit`: rows move, enter and leave animated.
        motion: Option<Rc<crate::motion::EachMotion>>,
    },
    /// A virtualized list: only visible rows are built.
    List {
        count: Source,
        /// `data`: the list state its rows show, as `ui.each`'s: its length
        /// is the count, `key` is given an item, and `row` an item (built
        /// again when it changes) or, taking a third parameter, its item as
        /// a state too, kept current (it updates in place).
        items: Option<Signal<Value>>,
        height: f32,
        /// Each row's own height (`heights`), instead of one `height`: the
        /// row count is its length.
        heights: Option<Heights>,
        /// What few lists set (`ListRare`), boxed: every node is as large as
        /// its largest kind.
        rare: Option<Box<ListRare>>,
        overscan: usize,
        build: Function,
        /// `key`: each row's item's key (a function of its index), so a row
        /// follows its item as the list changes, moved rather than built or
        /// written again; `row` then takes the key and its index, a state.
        key: Option<Function>,
        /// `measured = true`: rows take their content's height, measured as
        /// they mount (`height` the estimate for the rest until then).
        measured: bool,
        /// `follow = true` (with `measured` or `heights`): resting at the
        /// end, it stays there as rows come and are measured (a chat).
        follow: bool,
        offset: Option<Signal<Value>>,
        /// `scrollbar = true`: an interactive scrollbar over its edge.
        scrollbar: bool,
        /// `keyboard = true`: its rows are one Tab stop, and the arrows,
        /// Page Up and Down, Home and End move focus through all of them.
        keyboard: bool,
    },
    Switch {
        value: Source,
        cases: Vec<(Value, Rc<Node>)>,
        default: Option<Rc<Node>>,
    },
    Modal {
        label: String,
        open: Signal<Value>,
        placement: Option<zgx::compose::ModalPlacement>,
        /// `autofocus = false`: opening focuses the panel, not its first
        /// control.
        autofocus: bool,
        /// `ui.layer`: non-modal, over the window (toasts).
        layer: bool,
        /// `dismiss_on_backdrop = false`: a click outside doesn't close it
        /// (a dialog that must be answered).
        dismiss: Option<bool>,
    },
    Popover {
        label: String,
        open: Signal<Value>,
        anchor: Rc<Node>,
        placement: Option<zgx::compose::Placement>,
        autofocus: bool,
        /// `arrow = size`: an arrow at its trigger (`View::arrow`).
        arrow: Option<f32>,
        dismiss: Option<bool>,
        /// `match_anchor_width = true`: at least as wide as its trigger.
        match_anchor: bool,
        /// `at_caret = true`: at the caret of the editor in its anchor,
        /// focus left there (`View::at_caret`).
        at_caret: bool,
    },
    Menu {
        label: String,
        open: Signal<Value>,
        anchor: Rc<Node>,
        placement: Option<zgx::compose::Placement>,
        autofocus: bool,
        /// `open_over = fn() -> id`: opens with that item over the trigger.
        over: Option<Function>,
        match_anchor: bool,
    },
    /// Its label, and how it is chosen and shown.
    MenuItem(String, MenuItemOptions),
    Submenu {
        label: String,
        open: Signal<Value>,
        /// Its own trigger row (a `ui.menu_item` with content).
        trigger: Option<Rc<Node>>,
    },
    /// `ui.component(fn)`: `fn(props)` runs as each instance mounts, with
    /// its own `ui.use_state` state, effects and mount hooks.
    Component {
        build: Function,
        props: Table,
    },
    /// `ui.presence(value, build, { enter, exit, mode })`: content for the
    /// current value, entering and exiting with animations as it changes.
    Presence {
        value: Source,
        build: Function,
        enter: Option<Table>,
        exit: Option<Table>,
        /// `mode = "wait"`: the new content enters once the old has left.
        wait: bool,
        /// `initial = true`: the first content enters too.
        initial: bool,
    },
    /// A plugin extension point; `context` reaches what is contributed to it
    /// (`props.slot` of a contributed component).
    Slot {
        name: String,
        context: Option<Value>,
        /// Only what this script (a plugin's name) contributes.
        from: Option<String>,
    },
}
impl Kind {
    fn takes_children(&self) -> bool {
        matches!(
            self,
            Kind::Column
                | Kind::Row
                | Kind::Div
                | Kind::Overlay
                | Kind::Button
                | Kind::Scroll { .. }
                | Kind::Split { .. }
                | Kind::Modal { .. }
                | Kind::Popover { .. }
                | Kind::Menu { .. }
                | Kind::Submenu { .. }
                | Kind::MenuItem(..)
        ) || self.library_takes_children()
    }
    /// A component of the library holding children (a disclosure's content,
    /// an accordion's sections), which it lowers itself.
    pub(crate) fn library_takes_children(&self) -> bool {
        #[cfg(feature = "components")]
        if let Kind::Library(library) = self {
            return library.takes_children();
        }
        false
    }
    /// A 3D scene (its effects and bindings live in a component context).
    fn is_scene3d(&self) -> bool {
        #[cfg(feature = "scene3d")]
        if let Kind::Scene3d(_) = self {
            return true;
        }
        false
    }
    /// A component of the library that needs a component context.
    fn library_needs_context(&self) -> bool {
        #[cfg(feature = "components")]
        if let Kind::Library(library) = self {
            return library.needs_context();
        }
        false
    }
}

#[derive(Clone)]
pub(crate) enum Text {
    Literal(String),
    /// `"Count: {count}"`: interpolated in Rust, no script call per update.
    Template(Vec<Segment>),
    /// Followed: rerun when what it reads changes.
    Computed(Computed),
}
/// A followed text: a script function, or a localized message (`ui.t`),
/// formatted in Rust as its locale, file or arguments change. One variant
/// of `Text` for both, so the texts most scripts show (literals and
/// templates) take no arm for messages as they are dropped or lowered.
#[derive(Clone)]
pub(crate) enum Computed {
    Script(Function),
    Message(zgx::i18n::Message),
}
#[derive(Clone)]
pub(crate) enum Segment {
    Literal(String),
    /// A state, or a field of it (`"{user.name}"`: the path after its key).
    State(Signal<Value>, Rc<[Box<str>]>),
}

/// A rich text run.
#[derive(Clone)]
pub(crate) struct Span {
    pub text: Text,
    pub color: Option<Themed<Color>>,
    pub background: Option<Themed<Color>>,
    pub size: Option<f32>,
    pub weight: Option<u16>,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// `font`: as `ui.text`'s, `"mono"` or a family's name.
    pub font: Option<Rc<str>>,
    /// Those of its options given a state or a function, and what it was
    /// given: read as the paragraph is built, so a change builds it again.
    /// Empty for most spans.
    pub followed: Vec<(&'static str, Ty, Source)>,
    /// Room around it (across, down) and its background's corners: an
    /// inline code chip.
    pub padding: [f32; 2],
    pub radius: f32,
    pub click: Option<Handler>,
    /// Where it links (`href = "https://…"`): a click opens it.
    pub href: Option<std::sync::Arc<str>>,
    /// How far below the baseline its underline sits.
    pub underline_offset: Option<f32>,
    /// How its underline is drawn: a line style, a colour, a thickness.
    pub underline_style: Option<zgx::rich_text::LineStyle>,
    pub underline_color: Option<Themed<Color>>,
    pub underline_thickness: Option<f32>,
    /// A link's look under the pointer: underlined, and its colour.
    pub hover: Option<(bool, Option<Themed<Color>>)>,
    /// An icon before the text, in the line.
    pub icon: Option<SpanIcon>,
}
/// `ui.span { icon = { src = …, size = 12, gap = 4, tint = … } }`.
#[derive(Clone)]
pub(crate) struct SpanIcon {
    /// A file, `https://…`, or `code:<svg …>`.
    pub src: String,
    pub size: f32,
    pub gap: f32,
    /// A colour, or `"currentColor"` (the span's text colour).
    pub tint: Option<Value>,
}

/// A node's tooltip (`tooltip`, `tooltip_placement`).
#[derive(Default, Clone)]
pub(crate) struct Tooltip {
    pub content: Option<TooltipContent>,
    pub placement: Option<zgx::compose::Placement>,
    /// `tooltip_interactive`: a hover card the pointer can move into (and
    /// click in); it stays while the pointer is over the node or the card.
    pub interactive: bool,
    /// `tooltip_delay`: how long the pointer rests before it shows.
    pub delay: Option<std::time::Duration>,
}
#[derive(Clone)]
pub(crate) enum TooltipContent {
    /// Drawn in the theme's tooltip look.
    Text(String),
    /// The app's own content (its look and all).
    Node(Rc<Node>),
}

/// A list's row heights: a list of numbers, a state holding one, or a
/// function returning one (tracked).
#[derive(Clone)]
pub(crate) enum Heights {
    Lit(Value),
    State(Signal<Value>),
    Computed(Function),
}

/// What a code editor adds to a textarea: `highlight`, `decorations`,
/// `current_line` and `tab`.
pub(crate) struct CodeProps {
    /// `highlight = function(line, state) return spans, state end`: a
    /// line's colours, `{ { from, to, color }, … }` (bytes, as
    /// `string.find` gives them), and what it leaves the next line.
    pub highlight: Option<Function>,
    /// `decorations`: a state or function of `{ { from, to, color }, … }`,
    /// bytes of the whole text marked behind it (a search's matches).
    pub decorations: Option<Readable>,
    /// `underlines`: a state or function of `{ { from, to, color,
    /// style = } }`.
    pub underlines: Option<Readable>,
    /// `current_line`: the colour behind the caret's row.
    pub current_line: Option<Source>,
    /// `tab`: what Tab inserts (`"    "`), indenting selected lines.
    pub tab: Option<String>,
    /// `on_caret = function(line, column, anchor, focus)`: where the caret
    /// is, whenever it moves.
    pub on_caret: Option<Function>,
    /// `on_top_line = function(line)`: the first line in view, whenever it
    /// changes.
    pub on_top_line: Option<Function>,
    /// `find = { query, case, current, limit, color, current_color }`: a
    /// search's matches marked behind the text, found where the text is
    /// held (no list of them made in the script).
    pub find: Option<FindProps>,
}
/// A textarea's `find`: what it searches for and how its matches show.
pub(crate) struct FindProps {
    /// The text looked for (empty: nothing marked).
    pub query: Source,
    /// Whether case matters (`true` by default).
    pub case: Option<Source>,
    /// Which match, from 1, is the current one (0: none).
    pub current: Option<Source>,
    /// At most this many marked.
    pub limit: Option<usize>,
    pub color: Option<Source>,
    pub current_color: Option<Source>,
}
/// `bg_grid`'s table, as a canvas takes it (`GridFill` in the types).
fn grid_spec(lua: &Lua, grid: &Table) -> mlua::Result<GridSpec> {
    let mark = match (
        grid.get::<Option<f32>>("dot")?,
        grid.get::<Option<f32>>("lines")?,
    ) {
        (_, Some(width)) => zgx::scene::GridMark::Lines(width),
        (dot, None) => zgx::scene::GridMark::Dot(dot.unwrap_or(1.)),
    };
    let color = match grid.get::<mlua::Value>("color")? {
        mlua::Value::Nil => Themed::Token(zgx::theme::token("border")),
        v => themed_color(&Value::from_lua(v, lua)?)
            .ok_or_else(|| mlua::Error::runtime("ui.canvas `bg_grid.color` must be a colour"))?,
    };
    Ok(GridSpec {
        step: grid.get::<Option<f32>>("step")?.unwrap_or(16.),
        min_step: grid.get::<Option<f32>>("min_step")?.unwrap_or(0.),
        mark,
        color,
    })
}
/// A canvas's `view` (its drawing's pan and zoom, followed) and `bg_grid`
/// (a grid under it, following the view).
#[derive(Clone)]
pub(crate) struct CanvasOptions {
    pub view: Option<Readable>,
    pub grid: Option<GridSpec>,
    /// `each = state, draw_item = function(c, item, w, h)`: each item drawn
    /// on its own and kept, drawn again only when it is written.
    pub each: Option<(Signal<Value>, Function)>,
}
/// `bg_grid = { step, min_step, dot | lines, color }`.
#[derive(Clone)]
pub(crate) struct GridSpec {
    pub step: f32,
    pub min_step: f32,
    pub mark: zgx::scene::GridMark,
    pub color: Themed<Color>,
}
/// A state, or a function of states, read as a value.
#[derive(Clone)]
pub(crate) enum Readable {
    State(Signal<Value>),
    Function(Function),
}
#[derive(Clone)]
pub(crate) enum Source {
    Lit(Lit),
    State(Signal<Value>),
    Computed(Function),
    Anim(Rc<Anim>),
    /// Set by `variants = …` chosen by state: what the choice sets now.
    Variant(Rc<crate::variants::Selection>, Prop),
}

/// What `radial` is told when it can't draw what it was given.
const RADIAL_STOPS: &str = "`radial` takes two or three stops, from the centre to the edge";
/// A `radial`: where it is centred, how far it reaches and its stops.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RadialLit {
    pub center: (f32, f32),
    pub radius: f32,
    pub stops: zgx::decoration::RadialStops,
}
/// One `Lit` variant for both: a third with a heap pointer takes `Lit`'s drop
/// out of line, and every styled node pays for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GradientLit {
    Linear(f32, Arc<[GradientStop]>),
    Radial(RadialLit),
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Lit {
    Num(f32),
    Bool(bool),
    Str(Rc<str>),
    Color(Color),
    Pair(f32, f32),
    Len(Length),
    Shadows(Arc<[BoxShadow]>),
    Gradient(Arc<GradientLit>),
    Corners(Corners),
    /// A theme token (`"$surface.raised"`), resolved when the node styles.
    Token(zgx::theme::TokenId),
    /// A theme token's colour `t` of the way to another colour or token:
    /// a tween between theme colours, resolved when the node styles.
    Mix(zgx::theme::TokenId, zgx::theme::Themed<Color>, f32),
}

#[derive(Clone)]
pub(crate) enum Handler {
    Lua(Function),
    Action(Action),
}
#[derive(Clone)]
pub(crate) enum Action {
    Set(Signal<Value>, Value),
    Toggle(Signal<Value>),
    Add(Signal<Value>, f64),
    /// A function the Rust application registered with `ScriptHost::action`.
    Host(Rc<str>, Value),
}

/// A Rust-driven animation of a scalar bound to state (`ui.tween`,
/// `ui.spring`, `ui.loop`). With `from`/`to`, the scalar is a 0..1 progress
/// mapped between them (numbers, colors or pairs).
pub(crate) struct Anim {
    pub source: Option<Signal<Value>>,
    pub curve: Curve,
    pub from: Option<Value>,
    pub to: Option<Value>,
}
#[derive(Clone)]
pub(crate) enum Curve {
    /// `ms`: the duration in milliseconds, as everywhere in scripts.
    Tween {
        ms: f32,
        ease: crate::anime::ease::Ease,
    },
    Spring {
        stiffness: f32,
        damping: f32,
    },
    /// `fps` caps the frames a decorative loop asks for.
    Loop {
        ms: f32,
        ease: crate::anime::ease::Ease,
        yoyo: bool,
        fps: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pseudo {
    Hover,
    Active,
    Focus,
    Disabled,
}
impl Pseudo {
    /// Its place in zgx's interaction styles (and `variants::INTERACTION`).
    pub(crate) fn slot(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ty {
    Num,
    Bool,
    Color,
    Str,
    Pair,
    /// Points, `"50%"` or `"auto"`.
    Len,
    /// A grid's equal tracks (a number) or a track list (`"220 1fr auto"`).
    Tracks,
    /// A grid's named areas: a string a row, or one string a row a line.
    Areas,
    /// An item's lines in its grid (`"side / main"`, `"2 / span 3"`, a number).
    GridLines,
    /// How a grid places items without a place (`"row"`, `"dense"`…).
    GridFlow,
    Shadow,
    Gradient,
    Radial,
    Corners,
    /// Wrapping: a boolean, or `"balance"`.
    Wrap,
}

macro_rules! props {
    ($($name:literal => $variant:ident: $ty:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum Prop { $($variant),* }
        impl Prop {
            pub(crate) fn parse(name: &str) -> Option<Self> {
                match name { $($name => Some(Prop::$variant),)* _ => None }
            }
            /// Numbers, lengths and colors interpolate; the rest switch.
            pub(crate) fn animatable(self) -> bool {
                matches!(self.ty(), Ty::Num | Ty::Color | Ty::Pair | Ty::Len | Ty::Tracks)
            }
            pub fn ty(self) -> Ty {
                match self { $(Prop::$variant => Ty::$ty),* }
            }
            /// Its name as scripts write it.
            pub(crate) fn name(self) -> &'static str {
                match self { $(Prop::$variant => $name),* }
            }
        }
    };
}
props! {
    // Size and position.
    "w" => W: Len, "h" => H: Len, "size" => Size: Pair,
    "min_w" => MinW: Len, "max_w" => MaxW: Len, "min_h" => MinH: Len, "max_h" => MaxH: Len,
    "w_full" => WFull: Bool, "h_full" => HFull: Bool, "aspect" => Aspect: Num,
    "absolute" => Absolute: Bool, "relative" => Relative: Bool,
    "top" => Top: Len, "left" => Left: Len, "right" => Right: Len, "bottom" => Bottom: Len,
    "inset_start" => InsetStart: Len, "inset_end" => InsetEnd: Len,
    // Spacing.
    "p" => P: Len, "px" => Px: Len, "py" => Py: Len,
    "pt" => Pt: Len, "pr" => Pr: Len, "pb" => Pb: Len, "pl" => Pl: Len,
    "ps" => Ps: Len, "pe" => Pe: Len,
    "m" => M: Len, "mx" => Mx: Len, "my" => My: Len,
    "mt" => Mt: Len, "mr" => Mr: Len, "mb" => Mb: Len, "ml" => Ml: Len,
    "ms" => Ms: Len, "me" => Me: Len,
    "gap" => Gap: Num, "gap_x" => GapX: Len, "gap_y" => GapY: Len,
    // Flex and grid.
    "dir" => Dir: Str,
    "direction" => Direction: Str, "grow" => Grow: Num, "shrink" => Shrink: Num, "basis" => Basis: Len,
    "flex_wrap" => FlexWrap: Bool, "wrap_reverse" => WrapReverse: Bool,
    "items" => Items: Str, "justify" => Justify: Str, "self" => AlignSelf: Str, "content" => AlignContent: Str,
    "grid" => Grid: Bool, "cols" => Cols: Tracks, "rows" => Rows: Tracks,
    "col_start" => ColStart: Num, "row_start" => RowStart: Num, "col_span" => ColSpan: Num, "row_span" => RowSpan: Num,
    "col_span_full" => ColSpanFull: Bool, "row_span_full" => RowSpanFull: Bool,
    "overflow" => Overflow: Str, "overflow_x" => OverflowX: Str, "overflow_y" => OverflowY: Str,
    "hidden" => Hidden: Bool, "invisible" => Invisible: Bool, "z" => Z: Num,
    "sticky_top" => StickyTop: Num,
    // Paint.
    "bg" => Bg: Color, "gradient" => Gradient: Gradient, "radial" => Radial: Radial, "mask_gradient" => MaskGradient: Gradient, "mask_radial" => MaskRadial: Radial, "color" => TextColor: Color,
    "bg_image" => BgImage: Str, "bg_fit" => BgFit: Str,
    "rounded" => Rounded: Num, "corners" => Corners: Corners,
    "rounded_tl" => RoundedTl: Num, "rounded_tr" => RoundedTr: Num,
    "rounded_br" => RoundedBr: Num, "rounded_bl" => RoundedBl: Num,
    "border" => Border: Num, "border_sides" => BorderSides: Corners, "border_color" => BorderColor: Color,
    "outline" => Outline: Num, "outline_color" => OutlineColor: Color, "outline_offset" => OutlineOffset: Num,
    "border_over" => BorderOver: Bool, "border_dash" => BorderDash: Pair, "border_style" => BorderStyle: Str, "outline_style" => OutlineStyle: Str, "outline_dash" => OutlineDash: Pair,
    "shadow" => Shadow: Shadow, "inset_shadow" => InsetShadow: Shadow, "text_shadow" => TextShadow: Shadow, "opacity" => Opacity: Num,
    "brightness" => Brightness: Num, "contrast" => Contrast: Num, "saturate" => Saturate: Num, "hue_rotate" => HueRotate: Num, "sepia" => Sepia: Num, "invert" => Invert: Num, "blur" => Blur: Num, "backdrop_blur" => BackdropBlur: Num,
    "blend" => Blend: Str,
    "fade" => Fade: Pair, "fade_x" => FadeX: Pair, "fade_sides" => FadeSides: Corners,
    "fade_overflow" => FadeOverflow: Bool, "snap" => Snap: Bool,
    "zoom" => Zoom: Num, "zoom_fixed" => ZoomFixed: Bool,
    "clip_inset" => ClipInset: Corners, "clip_feather" => ClipFeather: Num, "layer" => Layer: Bool, "object_position" => ObjectPosition: Pair, "fit" => Fit: Str, "image_rendering" => ImageRendering: Str, "tint" => Tint: Color, "decode" => Decode: Str, "dither" => Dither: Num, "characters" => Characters: Num,
    "cursor" => Cursor: Str,
    // Text.
    "text_size" => TextSize: Num, "weight" => Weight: Num, "bold" => Bold: Bool, "italic" => Italic: Bool,
    "align" => Align: Str, "font" => Font: Str, "wrap" => Wrap: Wrap, "truncate" => Truncate: Bool,
    "lines" => Lines: Num, "line_height" => LineHeight: Num, "letter_spacing" => LetterSpacing: Num,
    "tabular" => Tabular: Bool, "font_features" => FontFeatures: Str, "underline" => Underline: Bool, "underline_offset" => UnderlineOffset: Num,
    // After the rest, so that the props before keep their numbers
    // (`specificity` compiles to a test of them).
    "areas" => Areas: Areas, "flow" => Flow: GridFlow, "area" => Area: Str,
    "grid_column" => GridColumn: GridLines, "grid_row" => GridRow: GridLines,
    "display" => Display: Str,
    "text_gradient" => TextGradient: Gradient, "text_radial" => TextRadial: Radial,
    // Transforms: after layout, about `origin` (fractions of the node).
    // Last, so that `is_transform` is a single comparison.
    "x" => X: Num, "y" => Y: Num,
    "scale" => Scale: Num, "scale_x" => ScaleX: Num, "scale_y" => ScaleY: Num,
    "rotate" => Rotate: Num, "skew" => Skew: Num, "skew_x" => SkewX: Num, "skew_y" => SkewY: Num,
    "rotate_x" => RotateX: Num, "rotate_y" => RotateY: Num, "perspective" => Perspective: Num,
    "origin" => Origin: Pair,
}

/// Per-script data the `ui` library reads.
pub(crate) struct Env {
    pub vm: Weak<Vm>,
    pub store: Rc<Store>,
    pub shared: Rc<Store>,
    /// What it contributed to slots: the slot, the script (`ui.slot`'s
    /// `from` takes only its own) and the node.
    pub contributions: RefCell<Vec<Contribution>>,
    /// What it added at the app's elements (`ui.contribute { target }`).
    pub hooks: RefCell<Vec<HookContribution>>,
    /// The app's modules it replaces (`ui.override`): module name and the
    /// replacement.
    pub overrides: RefCell<Vec<(String, ModuleOverride)>>,
    pub engine: Rc<crate::anime::Engine>,
    pub runtime: Runtime,
    /// The component instance being built, if any.
    pub scope: RefCell<Option<Rc<Scope>>>,
    /// Effects created outside components: they live as long as the script.
    pub retained: RefCell<Vec<Box<dyn Any>>>,
    /// `ui.on_mount` outside components: run as the app's interface mounts.
    pub mounted: RefCell<Vec<Function>>,
    /// The host's actions (`ui.action` binds them, `ui.invoke` calls them).
    pub actions: crate::Actions,
    /// The clipboard's files as a paste hears them (`io::pasted_files`).
    pub platform: crate::io::SharedPlatform,
    pub picked: crate::io::Picked,
    /// The host's functions (`ui.call_sync`).
    pub functions: crate::Functions,
    /// The host's saved state (`ui.persist`).
    pub persist: Rc<Persist>,
    /// The host's warnings (`crate::warn`): each once, across reloads.
    pub warnings: crate::SharedWarnings,
    /// Where `ui.every` and `ui.after` run.
    pub timers: Rc<crate::timers::Timers>,
    /// What the script may do beyond its sandbox.
    pub grants: Rc<crate::Grants>,
    /// Host actions and shared keys plugins reach.
    pub exposed_actions: crate::Exposed,
    pub exposed_shared: crate::Exposed,
    /// Of those, the keys plugins only read (`expose_shared_read`).
    pub read_only_shared: crate::ReadOnly,
    /// Targets whose before/after need a capability (`expose_hook`).
    pub exposed_hooks: crate::Exposed,
    /// The script's name (a plugin's): what it contributes is its own.
    pub owner: Rc<str>,
    /// Whose `ui.persist` state it reads: the script's name, or for a
    /// plugin the app loads (`set_plugins`) one no script of the app's can
    /// have (a plugin named `app.luau` does not read the app's).
    pub persist_as: Rc<str>,
    /// A plugin's: what its states hold, counted against its memory limit.
    pub meter: Option<Rc<crate::meter::Meter>>,
    /// The images the host sets (`host:` sources): pictures scripts take.
    pub assets: Rc<crate::lower::Assets>,
}
/// What a script adds to a slot: the slot, who added it (`ui.slot`'s
/// `from`) and the node.
pub(crate) type Contribution = (String, Rc<str>, Rc<Node>);
/// Where a plugin's node goes against the element it targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    Before,
    After,
    Replace,
}
/// What a script adds at an element of the app's: a node (`ui.contribute
/// { target, before | after | replace }`) or filters (`ui.filter`).
pub(crate) struct HookContribution {
    pub target: crate::driver::Static,
    pub adds: Adds,
    /// Who added it.
    pub owner: Rc<str>,
    /// Its script, which runs the filters.
    pub vm: Weak<Vm>,
}
pub(crate) enum Adds {
    Node(Place, Rc<Node>),
    /// What each filtered prop is passed through, in the order written.
    Filters(Vec<(Filtered, Function)>),
}
impl HookContribution {
    /// Its node, if it adds one `place`.
    pub(crate) fn placed(&self, place: Place) -> Option<&Rc<Node>> {
        match &self.adds {
            Adds::Node(at, node) if *at == place => Some(node),
            _ => None,
        }
    }
}
/// What a plugin's `ui.filter` may pass through a function of its own: the
/// element's text and name, whether it is hidden or disabled, its tooltip's
/// text, the items a `ui.each` or a `ui.list` (`data`) shows and the order
/// of its children. Nothing else of the element's: its
/// handlers, styles and state stay the app's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filtered {
    Text,
    Label,
    Hidden,
    Disabled,
    Tooltip,
    Items,
    Order,
}
impl Filtered {
    pub(crate) const ALL: [(&'static str, Filtered); 7] = [
        ("text", Filtered::Text),
        ("label", Filtered::Label),
        ("hidden", Filtered::Hidden),
        ("disabled", Filtered::Disabled),
        ("tooltip", Filtered::Tooltip),
        ("items", Filtered::Items),
        ("order", Filtered::Order),
    ];
}
/// Each slot's contributions, by slot, with who added each.
pub(crate) type Slots = std::collections::HashMap<String, Vec<(Rc<str>, Rc<Node>)>>;
impl Env {
    /// `image` as `host:<name>`, replacing what was there: the `src` that
    /// shows it.
    pub(crate) fn host_image(&self, name: &str, image: Arc<zgx::image::ImageData>) -> String {
        self.assets.set_hosted(name, Some(image), &self.runtime);
        format!("host:{name}")
    }
    /// Whether the script may reach `key` of what the app `exposed`
    /// (`what` says which, for the error): the app's scripts always.
    pub(crate) fn permits(
        &self,
        exposed: &crate::Exposed,
        key: &str,
        what: &str,
    ) -> mlua::Result<()> {
        if matches!(*self.grants, crate::Grants::All) {
            return Ok(());
        }
        match exposed.borrow().get(key) {
            None => Err(mlua::Error::runtime(format!(
                "{what} `{key}`: the app does not expose it to plugins"
            ))),
            Some(Some(capability)) if !self.grants.allows(capability) => {
                Err(mlua::Error::runtime(format!(
                    "{what} `{key}`: a plugin reaches it only if the host grants it ({capability:?})"
                )))
            }
            Some(_) => Ok(()),
        }
    }
    /// Whether the script may use `capability` (`what` says how, for the
    /// error).
    pub(crate) fn may(&self, capability: crate::Capability, what: &str) -> mlua::Result<()> {
        if self.grants.allows(&capability) {
            Ok(())
        } else {
            Err(mlua::Error::runtime(format!(
                "{what}: a plugin does this only if the host grants it (`Capability::{capability:?}`)"
            )))
        }
    }
}

/// What a component instance owns while it is mounted.
#[derive(Default)]
pub(crate) struct Scope {
    pub retained: RefCell<Vec<Box<dyn Any>>>,
    pub mounted: RefCell<Vec<Function>>,
    /// What the subtree being built was provided (`provide`).
    pub provided: Option<Rc<Provided>>,
}
/// A value a node provides its subtree, by its name.
pub(crate) type Given = (Rc<str>, mlua::Value);
/// The values a subtree was provided, its own over its ancestors'.
#[derive(Default)]
pub(crate) struct Provided(pub std::collections::HashMap<Rc<str>, mlua::Value>);

pub(crate) struct NodeUd(pub Rc<Node>);
impl UserData for NodeUd {}
pub(crate) struct AnimUd(pub Rc<Anim>);
impl UserData for AnimUd {}
pub(crate) struct ActionUd(pub Action);
impl UserData for ActionUd {}
pub(crate) struct SpanUd(pub RefCell<Option<Span>>);
impl UserData for SpanUd {}

pub(crate) fn env(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, Env>> {
    lua.app_data_ref::<Env>()
        .ok_or_else(|| mlua::Error::runtime("ui library used outside a script host"))
}
pub(crate) fn node_ud(lua: &Lua, mut node: Node) -> mlua::Result<mlua::AnyUserData> {
    if node.owner.upgrade().is_some_and(|vm| vm.locate.get()) {
        node.source = location(lua);
    }
    // Every `ui.*` call makes one: without the memory limit's protected
    // call around each (see vendor/mlua/ZGX_PATCH.md).
    lua.create_userdata_relaxed(NodeUd(Rc::new(node)))
}
/// The script line calling the `ui` function running now: `app.luau:12`.
fn location(lua: &Lua) -> Option<Rc<str>> {
    lua.inspect_stack(1, |debug| {
        let line = debug.current_line()?;
        let source = debug.source().source?;
        let chunk = source.strip_prefix('@').unwrap_or(&source);
        Some(Rc::from(format!("{chunk}:{line}")))
    })
    .flatten()
}
/// An overlay's `open` (a modal's, a popover's, a menu's): a state, written
/// back as it closes; or, controlled, a function saying whether it shows,
/// its closing (Escape, the backdrop, a choice) going to `on_close` (a
/// modal route: open while the route says so, closing goes back).
pub(crate) fn open_of(lua: &Lua, table: &Table) -> mlua::Result<Signal<Value>> {
    match table.get::<mlua::Value>("open")? {
        mlua::Value::Function(show) => controlled(lua, show, table.get("on_close")?),
        open => expect_state(&open, "`open`"),
    }
}
/// A control's `value` (a checkbox's, a slider's, a segmented control's):
/// a state, both ways; or, controlled as React's are, a function it shows,
/// a change it makes going to `on_change` instead, the control showing the
/// function's answer again until what it reads changes.
pub(crate) fn control_value(lua: &Lua, table: &Table) -> mlua::Result<Signal<Value>> {
    match table.get::<mlua::Value>("value")? {
        mlua::Value::Function(show) => controlled(lua, show, table.get("on_change")?),
        value => expect_state(&value, "`value`"),
    }
}
/// The state a controlled control shows: what `show` returns, kept so as
/// what it reads changes; a change the control writes goes to `on_change`
/// and the control shows `show`'s answer again (the app's change, once it
/// makes one, shows through `show`).
#[cold]
#[inline(never)]
fn controlled(
    lua: &Lua,
    show: Function,
    on_change: Option<Function>,
) -> mlua::Result<Signal<Value>> {
    let env = env(lua)?;
    let runtime = env.runtime.clone();
    let shown = runtime.signal(Value::Nil);
    let last = Rc::new(RefCell::new(Value::Nil));
    let showing = {
        let (vm, shown, last) = (env.vm.clone(), shown.clone(), last.clone());
        runtime.effect_now(move || {
            let Some(vm) = vm.upgrade() else {
                return;
            };
            match vm.call::<Value>(&show, ()) {
                Ok(value) => {
                    *last.borrow_mut() = value.clone();
                    shown.set(value);
                }
                Err(error) => vm.report(error),
            }
        })
    };
    let changing = {
        let (vm, shown, runtime2) = (env.vm.clone(), shown.clone(), runtime.clone());
        runtime.effect_now(move || {
            let now = shown.get();
            let was = last.borrow().clone();
            if now == was {
                return;
            }
            runtime2.untracked(|| {
                if let (Some(vm), Some(on_change)) = (vm.upgrade(), &on_change)
                    && let Err(error) = vm.call::<()>(on_change, now)
                {
                    vm.report(error);
                }
                shown.set(was);
            });
        })
    };
    retain(&env, Box::new((showing, changing)));
    Ok(shown)
}
/// The state `value` holds, to read and follow: a plugin's read-only
/// shared state too.
pub(crate) fn state_of(value: &mlua::Value) -> Option<Signal<Value>> {
    match value {
        mlua::Value::UserData(ud) => match ud.borrow::<State>() {
            Ok(state) => Some(state.0.clone()),
            Err(_) => read_state_of(ud),
        },
        _ => None,
    }
}
#[cold]
fn read_state_of(ud: &mlua::AnyUserData) -> Option<Signal<Value>> {
    ud.borrow::<crate::value::ReadState>()
        .ok()
        .map(|s| s.0.0.clone())
}
/// The state `value` holds, to write as well (a two-way binding, an
/// action's target): never a plugin's read-only shared state.
pub(crate) fn writable_state_of(value: &mlua::Value) -> Option<Signal<Value>> {
    match value {
        mlua::Value::UserData(ud) => ud.borrow::<State>().ok().map(|s| s.0.clone()),
        _ => None,
    }
}
pub(crate) fn expect_state(value: &mlua::Value, what: &str) -> mlua::Result<Signal<Value>> {
    writable_state_of(value).ok_or_else(|| match read_only_key(value) {
        Some(key) => mlua::Error::runtime(format!(
            "{what} is written, and shared state `{key}` is read only to plugins"
        )),
        None => mlua::Error::runtime(format!("{what} must be a ui.state")),
    })
}
/// A state that is only read (what a list shows, an animation's source):
/// a plugin's read-only shared state too.
pub(crate) fn expect_readable(value: &mlua::Value, what: &str) -> mlua::Result<Signal<Value>> {
    state_of(value).ok_or_else(|| mlua::Error::runtime(format!("{what} must be a ui.state")))
}
#[cold]
fn read_only_key(value: &mlua::Value) -> Option<Rc<str>> {
    match value {
        mlua::Value::UserData(ud) => ud
            .borrow::<crate::value::ReadState>()
            .ok()
            .map(|s| s.1.clone()),
        _ => None,
    }
}
/// A plugin's module in place of one of the app's (`ui.override`): its file,
/// the plugin's directory (what its own requires reach) and the plugin's
/// name. It runs in the app's VM, as the app's own code.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModuleOverride {
    pub with: std::path::PathBuf,
    pub dir: std::path::PathBuf,
    pub owner: Rc<str>,
}

/// An `@app/…` module's name, as overrides and requires both write it:
/// `components/card` for `components/card`, `components/card.luau` and
/// `components/card/init.luau`.
pub(crate) fn module_name(path: &str) -> String {
    let path = path.trim_start_matches("./").trim_matches('/');
    let path = path.strip_suffix(".luau").unwrap_or(path);
    let path = path.strip_suffix("/init").unwrap_or(path);
    path.to_owned()
}

/// `ui.override { module = "@app/…", with = "./mine" }` from a plugin: the
/// app's module replaced by the plugin's, which runs as the app's own code
/// and may require the original as `@app-original/…`. Takes
/// `Capability::Override` of the module as written, or of `"*"`: a trust
/// grant, not a permission over the interface.
fn module_override(env: &Env, table: &Table) -> mlua::Result<(String, ModuleOverride)> {
    let module: String = table.get::<Option<String>>("module")?.ok_or_else(|| {
        mlua::Error::runtime("ui.override: `module` names the app's module (\"@app/…\")")
    })?;
    let Some(rest) = module.strip_prefix("@app/") else {
        return Err(mlua::Error::runtime(format!(
            "ui.override {{ module = \"{module}\" }}: only the app's `@app/…` modules are overridden"
        )));
    };
    let own = crate::Capability::Override(module.clone());
    let any = crate::Capability::Override("*".into());
    if !env.grants.allows(&own) && !env.grants.allows(&any) {
        return Err(mlua::Error::runtime(format!(
            "ui.override {{ module = \"{module}\" }}: {} needs {own:?}, a trust grant: the \
             module would run as the app's own code",
            env.owner
        )));
    }
    let with: String = table
        .get::<Option<String>>("with")?
        .ok_or_else(|| mlua::Error::runtime("ui.override: `with` names the plugin's module"))?;
    let base = env
        .vm
        .upgrade()
        .and_then(|vm| vm.base.clone())
        .ok_or_else(|| mlua::Error::runtime("ui.override needs a plugin loaded from a file"))?;
    let dir = base.canonicalize().unwrap_or(base);
    let mut path = crate::normalize(&dir.join(&with));
    if path.is_dir() {
        path = path.join("init.luau");
    } else if path.extension().is_none() {
        path.set_extension("luau");
    }
    if !crate::within(&dir, &path) {
        return Err(mlua::Error::runtime(format!(
            "ui.override {{ with = \"{with}\" }}: the replacement is one of the plugin's own files"
        )));
    }
    Ok((
        module_name(rest),
        ModuleOverride {
            with: path,
            dir,
            owner: env.owner.clone(),
        },
    ))
}

/// `ui.contribute { target, before | after | replace }`: the target read,
/// one place, and what the script may do there. Adding before or after an
/// element is open, unless the host asks a capability for that target
/// (`expose_hook`); replacing one takes `Capability::Override` of the
/// target as written, or of `"*"`.
fn hook_contribution(env: &Env, table: &Table, owner: Rc<str>) -> mlua::Result<HookContribution> {
    let written: String = table
        .get::<Option<String>>("target")?
        .ok_or_else(|| mlua::Error::runtime("ui.contribute: `target` names the element"))?;
    let target = crate::driver::Static::parse(&written).map_err(mlua::Error::runtime)?;
    let mut placed = Vec::new();
    for (key, place) in [
        ("before", Place::Before),
        ("after", Place::After),
        ("replace", Place::Replace),
    ] {
        let value: mlua::Value = table.get(key)?;
        if !value.is_nil() {
            placed.push((place, expect_node(value, &format!("`{key}`"))?));
        }
    }
    let [(place, node)] = <[_; 1]>::try_from(placed).map_err(|_| {
        mlua::Error::runtime(format!(
            "ui.contribute {{ target = \"{written}\" }}: one of `before`, `after` or `replace`"
        ))
    })?;
    let needs = match place {
        Place::Replace => overrides(env, &written),
        Place::Before | Place::After => env
            .exposed_hooks
            .borrow()
            .get(&written)
            .cloned()
            .flatten()
            .filter(|needed| !env.grants.allows(needed)),
    };
    if let Some(needed) = needs {
        return Err(mlua::Error::runtime(format!(
            "ui.contribute {{ target = \"{written}\" }}: {owner} needs {needed:?}"
        )));
    }
    Ok(HookContribution {
        target,
        adds: Adds::Node(place, node),
        owner,
        vm: env.vm.clone(),
    })
}
/// The capability replacing or filtering `written` takes, if the script
/// lacks it: `Override` of the target as written, or of `"*"`.
fn overrides(env: &Env, written: &str) -> Option<crate::Capability> {
    let own = crate::Capability::Override(written.into());
    let any = crate::Capability::Override("*".into());
    (!env.grants.allows(&own) && !env.grants.allows(&any)).then_some(own)
}
/// `ui.filter { target, text = fn, … }`: the target read, the props it
/// passes through functions (`Filtered`), and the capability it takes,
/// `Override`'s as replacing does.
fn hook_filter(env: &Env, table: &Table, owner: Rc<str>) -> mlua::Result<HookContribution> {
    let written: String = table
        .get::<Option<String>>("target")?
        .ok_or_else(|| mlua::Error::runtime("ui.filter: `target` names the element"))?;
    let target = crate::driver::Static::parse(&written).map_err(mlua::Error::runtime)?;
    for pair in table.pairs::<String, mlua::Value>() {
        let (key, _) = pair?;
        if key != "target" && !Filtered::ALL.iter().any(|(name, _)| *name == key) {
            return Err(mlua::Error::runtime(format!(
                "ui.filter {{ target = \"{written}\" }}: `{key}` is not filtered; \
                 one of text, label, hidden, disabled, tooltip, items or order"
            )));
        }
    }
    let mut filters = Vec::new();
    for (key, filtered) in Filtered::ALL {
        match table.get::<mlua::Value>(key)? {
            mlua::Value::Nil => {}
            mlua::Value::Function(f) => filters.push((filtered, f)),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ui.filter {{ target = \"{written}\" }}: `{key}` takes a function of its \
                     value, not {}",
                    other.type_name()
                )));
            }
        }
    }
    if filters.is_empty() {
        return Err(mlua::Error::runtime(format!(
            "ui.filter {{ target = \"{written}\" }}: name what it filters (text = function(text) … end)"
        )));
    }
    if let Some(needed) = overrides(env, &written) {
        return Err(mlua::Error::runtime(format!(
            "ui.filter {{ target = \"{written}\" }}: {owner} needs {needed:?}"
        )));
    }
    Ok(HookContribution {
        target,
        adds: Adds::Filters(filters),
        owner,
        vm: env.vm.clone(),
    })
}

pub(crate) fn expect_node(value: mlua::Value, what: &str) -> mlua::Result<Rc<Node>> {
    match value {
        mlua::Value::UserData(ud) if ud.is::<NodeUd>() => Ok(ud.borrow::<NodeUd>()?.0.clone()),
        other => Err(mlua::Error::runtime(format!(
            "{what} must be a ui node, not {}",
            other.type_name()
        ))),
    }
}

pub(crate) fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let [r, g, b, a] = crate::anime::value::parse_hex_color(hex)?;
        Some(Color(r, g, b, a))
    } else {
        crate::anime::value::parse_non_hex_color(text).map(crate::anime::target::v_to_color)
    }
}
/// A colour or a theme token (`"$text.secondary"`).
pub(crate) fn themed_color(value: &Value) -> Option<Themed<Color>> {
    match value {
        Value::Str(s) if s.starts_with('$') && s.len() > 1 => {
            Some(Themed::Token(zgx::theme::token(&s[1..])))
        }
        other => color_of(other).map(Themed::Value),
    }
}
fn color_of(value: &Value) -> Option<Color> {
    match value {
        Value::Num(n) => {
            let n = *n as u32;
            Some(Color((n >> 16) as u8, (n >> 8) as u8, n as u8, 255))
        }
        Value::Str(s) => parse_color(s),
        _ => None,
    }
}
fn length(value: &Value) -> Option<Length> {
    match value {
        Value::Num(n) => Some(Length::Px(*n as f32)),
        Value::Str(s) if &**s == "auto" => Some(Length::Auto),
        Value::Str(s) => match s.trim().strip_suffix('%') {
            Some(p) => p
                .trim()
                .parse::<f32>()
                .ok()
                .map(|p| Length::Percent(p / 100.)),
            None => s
                .trim()
                .trim_end_matches("px")
                .parse::<f32>()
                .ok()
                .map(Length::Px),
        },
        _ => None,
    }
}
fn num(value: &Value, key: &str) -> f32 {
    value.field(key).and_then(Value::as_f64).unwrap_or(0.) as f32
}
fn shadow(value: &Value) -> Option<BoxShadow> {
    Some(BoxShadow {
        color: value
            .field("color")
            .and_then(color_of)
            .unwrap_or(Color(0, 0, 0, 90)),
        offset: Transform {
            x: num(value, "x"),
            y: num(value, "y"),
        },
        // CSS's blur radius is twice the Gaussian sigma zgx takes.
        blur_radius: num(value, "blur") / 2.,
        spread: num(value, "spread"),
    })
}
/// What `lit` wouldn't take for the property `name`: size math on those
/// that take it, or why not. Apart, so that a literal that fits pays nothing.
#[cold]
#[inline(never)]
fn refused_lit(value: &Value, ty: Ty, name: &str) -> mlua::Result<Lit> {
    let reason = match (ty, value) {
        (Ty::Tracks, Value::Str(s)) => s
            .parse::<zgx::grid_tracks::GridTracks>()
            .err()
            .map(|why| format!("`{name}`: {why}")),
        (Ty::Areas, Value::Str(s)) => s
            .parse::<zgx::grid_tracks::GridAreas>()
            .err()
            .map(|why| format!("`{name}`: {why}")),
        (Ty::Areas, Value::List(rows)) => {
            let rows: Option<Vec<&str>> = rows
                .iter()
                .map(|row| match row {
                    Value::Str(row) => Some(&**row),
                    _ => None,
                })
                .collect();
            Some(match rows {
                Some(rows) => match rows.join("\n").parse::<zgx::grid_tracks::GridAreas>() {
                    Err(why) => format!("`{name}`: {why}"),
                    Ok(_) => format!("`{name}`: rows of names"),
                },
                None => {
                    format!("`{name}` takes a string a row: {{ \"head head\", \"side main\" }}")
                }
            })
        }
        (Ty::GridLines, Value::Str(s)) => s
            .parse::<zgx::grid_tracks::GridLines>()
            .err()
            .map(|why| format!("`{name}`: {why}")),
        (Ty::GridFlow, Value::Str(s)) => s
            .parse::<zgx::grid_tracks::GridFlow>()
            .err()
            .map(|why| format!("`{name}`: {why}")),
        (Ty::Radial, _) => return Err(mlua::Error::runtime(RADIAL_STOPS)),
        (Ty::Len, Value::Str(s)) => match s.parse::<SizeCalc>() {
            Ok(_) if takes_size_math(name) => return Ok(Lit::Str(s.clone())),
            Ok(_) => Some(format!(
                "`{name}`: calc(…), min(…), max(…) and clamp(…) size w, h and their limits"
            )),
            Err(why) if takes_size_math(name) => Some(format!("`{name}`: {why}")),
            Err(_) => None,
        },
        _ => None,
    };
    Err(mlua::Error::runtime(reason.unwrap_or_else(|| {
        format!("`{name}` expects a {ty:?}, got {value:?}")
    })))
}
/// Whether the property `name` takes `calc(…)` and the like.
fn takes_size_math(name: &str) -> bool {
    matches!(name, "w" | "h" | "min_w" | "max_w" | "min_h" | "max_h")
}
/// A length that isn't a plain one: a string, as a track list is, checked
/// here and worked out when it styles. Left to what `lit` refuses, so that
/// no plain length pays.
pub(crate) fn size_math(value: &Value) -> Option<Lit> {
    let Value::Str(text) = value else { return None };
    text.parse::<SizeCalc>().ok()?;
    Some(Lit::Str(text.clone()))
}
/// A grid's other texts (`areas`, `grid_column`, `flow`), checked and kept
/// as strings, as a track list is.
#[cold]
#[inline(never)]
fn grid_lit(value: &Value, ty: Ty) -> Option<Lit> {
    Some(match (ty, value) {
        (Ty::Areas, Value::List(rows)) => {
            let rows: Vec<&str> = rows
                .iter()
                .map(|row| match row {
                    Value::Str(row) => Some(&**row),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            let areas = rows.join("\n");
            areas.parse::<zgx::grid_tracks::GridAreas>().ok()?;
            Lit::Str(areas.into())
        }
        (Ty::Areas, Value::Str(s)) => {
            s.parse::<zgx::grid_tracks::GridAreas>().ok()?;
            Lit::Str(s.clone())
        }
        (Ty::GridLines, Value::Str(s)) => {
            s.parse::<zgx::grid_tracks::GridLines>().ok()?;
            Lit::Str(s.clone())
        }
        (Ty::GridLines, Value::Num(n)) if n.fract() == 0. && *n != 0. => {
            Lit::Str(format!("{n}").into())
        }
        (Ty::GridFlow, Value::Str(s)) => {
            s.parse::<zgx::grid_tracks::GridFlow>().ok()?;
            Lit::Str(s.clone())
        }
        _ => return None,
    })
}

/// Converts a value to a property's type, or `None` if it does not fit.
pub(crate) fn lit(value: &Value, ty: Ty) -> Option<Lit> {
    if let Value::Str(s) = value
        && let Some(name) = s.strip_prefix('$')
        && !name.is_empty()
        && matches!(ty, Ty::Color | Ty::Num | Ty::Len | Ty::Shadow | Ty::Str)
    {
        return Some(Lit::Token(zgx::theme::token(name)));
    }
    Some(match (ty, value) {
        (Ty::Num, v) => Lit::Num(v.as_f64()? as f32),
        (Ty::Bool, v) => Lit::Bool(v.truthy()),
        // `wrap = "balance"`: wrapped, its lines balanced.
        (Ty::Wrap, Value::Str(s)) if &**s == "balance" => Lit::Str(s.clone()),
        (Ty::Wrap, v) => Lit::Bool(v.truthy()),
        // An SVG's `tint`: the text colour the node inherits (see `lower`).
        (Ty::Color, Value::Str(s)) if s.eq_ignore_ascii_case("currentcolor") => Lit::Str(s.clone()),
        (Ty::Color, v) => Lit::Color(color_of(v)?),
        (Ty::Str, Value::Str(s)) => Lit::Str(s.clone()),
        (Ty::Str, Value::Text(t)) => Lit::Str(t.to_cow().into()),
        (Ty::Pair, Value::List(items)) if items.len() == 2 => {
            Lit::Pair(items[0].as_f64()? as f32, items[1].as_f64()? as f32)
        }
        (Ty::Pair, Value::Num(n)) => Lit::Pair(*n as f32, *n as f32),
        (Ty::Len, v) => Lit::Len(length(v)?),
        (Ty::Tracks, Value::Str(s)) => {
            s.parse::<zgx::grid_tracks::GridTracks>().ok()?;
            Lit::Str(s.clone())
        }
        (Ty::Tracks, v) => Lit::Num(v.as_f64()? as f32),
        // A grid's other texts, out of line: every literal comes this way.
        (Ty::Areas | Ty::GridLines | Ty::GridFlow, v) => return grid_lit(v, ty),
        (Ty::Shadow, Value::List(items)) => {
            Lit::Shadows(items.iter().map(shadow).collect::<Option<Vec<_>>>()?.into())
        }
        (Ty::Shadow, v @ Value::Map(_)) => Lit::Shadows(Arc::from([shadow(v)?])),
        (Ty::Shadow, Value::Bool(false) | Value::Nil) => Lit::Shadows(Arc::from([])),
        (Ty::Radial, v) => {
            let (center, radius, stops) = match v {
                Value::List(stops) => ((0.5, 0.5), 1., stops.clone()),
                Value::Map(_) => (
                    match v.field("center") {
                        Some(Value::List(c)) if c.len() == 2 => {
                            (c[0].as_f64()? as f32, c[1].as_f64()? as f32)
                        }
                        _ => (0.5, 0.5),
                    },
                    v.field("radius").and_then(Value::as_f64).unwrap_or(1.) as f32,
                    match v.field("stops")? {
                        Value::List(s) => s.clone(),
                        _ => return None,
                    },
                ),
                _ => return None,
            };
            let n = stops.len();
            if !(2..=3).contains(&n) {
                // From a function, nothing else would say so.
                eprintln!("zgx: {RADIAL_STOPS}");
                return None;
            }
            let stops: Vec<GradientStop> = stops
                .iter()
                .enumerate()
                .map(|(i, stop)| match stop {
                    Value::List(pair) if pair.len() == 2 => Some(GradientStop {
                        offset: pair[0].as_f64()? as f32,
                        color: color_of(&pair[1])?,
                    }),
                    other => Some(GradientStop {
                        offset: i as f32 / (n - 1) as f32,
                        color: color_of(other)?,
                    }),
                })
                .collect::<Option<_>>()?;
            let stops = match <[GradientStop; 2]>::try_from(stops) {
                Ok(two) => two.into(),
                Err(three) => <[GradientStop; 3]>::try_from(three).ok()?.into(),
            };
            Lit::Gradient(Arc::new(GradientLit::Radial(RadialLit {
                center,
                radius,
                stops,
            })))
        }
        (Ty::Gradient, v) => {
            let (angle, stops) = match v {
                Value::List(stops) => (180., stops.clone()),
                Value::Map(_) => (
                    v.field("angle").and_then(Value::as_f64).unwrap_or(180.),
                    match v.field("stops")? {
                        Value::List(s) => s.clone(),
                        _ => return None,
                    },
                ),
                _ => return None,
            };
            let n = stops.len();
            if n < 2 {
                return None;
            }
            let stops: Vec<GradientStop> = stops
                .iter()
                .enumerate()
                .map(|(i, stop)| match stop {
                    Value::List(pair) if pair.len() == 2 => Some(GradientStop {
                        offset: pair[0].as_f64()? as f32,
                        color: color_of(&pair[1])?,
                    }),
                    other => Some(GradientStop {
                        offset: i as f32 / (n - 1) as f32,
                        color: color_of(other)?,
                    }),
                })
                .collect::<Option<_>>()?;
            // CSS angles (0 points up, clockwise; the default 180 runs top
            // to bottom); zgx's are clockwise from +x.
            Lit::Gradient(Arc::new(GradientLit::Linear(
                ((angle - 90.) as f32).to_radians(),
                stops.into(),
            )))
        }
        (Ty::Corners, Value::List(r)) if r.len() == 4 => Lit::Corners(Corners {
            top_left: r[0].as_f64()? as f32,
            top_right: r[1].as_f64()? as f32,
            bottom_right: r[2].as_f64()? as f32,
            bottom_left: r[3].as_f64()? as f32,
        }),
        (Ty::Corners, v @ Value::Map(_)) => Lit::Corners(Corners {
            top_left: num(v, "tl"),
            top_right: num(v, "tr"),
            bottom_right: num(v, "br"),
            bottom_left: num(v, "bl"),
        }),
        _ => return None,
    })
}

/// A property's value as a script gives it (`name = value`): a literal is
/// checked by the design lint, unless `ui.raw` marks it.
pub(crate) fn source(lua: &Lua, ty: Ty, name: &str, value: mlua::Value) -> mlua::Result<Source> {
    source_linted(lua, ty, name, value, None)
}
/// As `source`, with the design lint's mode if the caller has read it.
fn source_linted(
    lua: &Lua,
    ty: Ty,
    name: &str,
    value: mlua::Value,
    lint: Option<crate::DesignLint>,
) -> mlua::Result<Source> {
    if let Some(state) = state_of(&value) {
        return Ok(Source::State(state));
    }
    match value {
        mlua::Value::Function(f) => Ok(Source::Computed(f)),
        // A message (`ui.t`) where text is taken: its text, followed.
        mlua::Value::UserData(ud) if ty == Ty::Str && ud.is::<crate::i18n::MessageUd>() => {
            let message = ud.borrow::<crate::i18n::MessageUd>()?.0.clone();
            Ok(Source::Computed(
                lua.create_function(move |_, ()| Ok(message.get()))?,
            ))
        }
        mlua::Value::UserData(ud) if ud.is::<AnimUd>() => {
            Ok(Source::Anim(ud.borrow::<AnimUd>()?.0.clone()))
        }
        other => {
            let raw = crate::design::is_raw(&other);
            // A colour written out, the commonest string a style takes, is
            // read where it lies, not copied into a value first.
            let written = match (&other, ty) {
                (mlua::Value::String(s), Ty::Color) => s
                    .to_str()
                    .ok()
                    .filter(|s| !s.starts_with('$'))
                    .and_then(|s| parse_color(&s)),
                _ => None,
            };
            let lit = match written {
                Some(color) => Lit::Color(color),
                None => {
                    let value = lua.unpack::<Value>(other)?;
                    match lit(&value, ty) {
                        Some(lit) => lit,
                        None => refused_lit(&value, ty, name)?,
                    }
                }
            };
            if !raw {
                lint_literal(lua, name, &lit, lint)?;
            }
            Ok(Source::Lit(lit))
        }
    }
}
/// The design lint's check of a literal `name` is set to (`source_linted`).
fn lint_literal(
    lua: &Lua,
    name: &str,
    lit: &Lit,
    lint: Option<crate::DesignLint>,
) -> mlua::Result<()> {
    if lint.unwrap_or_else(|| crate::design::mode(lua)) != crate::DesignLint::Off
        && let Some(prop) = Prop::parse(name)
    {
        // Hyphenation asked of a build without it: said where scripts are
        // linted (a debug build, `zgx lint`), not ignored. A build with it
        // never tests.
        if !zgx::text_layout::HYPHENATION && prop == Prop::FontFeatures {
            hyphenation_needs_the_feature(lit)?;
        }
        crate::design::check(lua, prop, name, lit)?;
    }
    Ok(())
}

#[cold]
#[inline(never)]
fn hyphenation_needs_the_feature(lit: &Lit) -> mlua::Result<()> {
    let Lit::Str(text) = lit else {
        return Ok(());
    };
    if text
        .split_whitespace()
        .any(|setting| setting.split('=').next() == Some("hyphens"))
    {
        return Err(mlua::Error::runtime(
            "`font_features = \"hyphens\"` needs zgx's `hyphens` feature, which this build \
             left out (a Rust host enables zgx-desktop's `hyphens`)",
        ));
    }
    Ok(())
}

fn handler(value: mlua::Value) -> mlua::Result<Handler> {
    match value {
        mlua::Value::Function(f) => Ok(Handler::Lua(f)),
        mlua::Value::UserData(ud) if ud.is::<ActionUd>() => {
            Ok(Handler::Action(ud.borrow::<ActionUd>()?.0.clone()))
        }
        other => Err(mlua::Error::runtime(format!(
            "handlers take a function or a ui action, not {}",
            other.type_name()
        ))),
    }
}

/// `{name}` binds state by name; `{{` and `}}` are literal braces. Anything
/// else in braces is literal, so data shown as text never needs escaping
/// unless it happens to look like `{identifier}`.
fn template(lua: &Lua, text: &str) -> mlua::Result<Text> {
    if !text.contains(['{', '}']) {
        return Ok(Text::Literal(text.to_owned()));
    }
    let env = env(lua)?;
    let prefix = module_prefix(lua, &env);
    let script = || {
        env.vm
            .upgrade()
            .map(|vm| vm.name.clone())
            .unwrap_or_default()
    };
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("{{").or_else(|| rest.strip_prefix("}}")) {
            literal.push(c);
            rest = after;
            continue;
        }
        let name = (c == '{')
            .then(|| rest[1..].split_once('}'))
            .flatten()
            .filter(|(name, _)| {
                // A key, and a field path after it (`user.name`, `rows.1`).
                name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                    && !name.ends_with('.')
                    && !name.contains("..")
            });
        let Some((name, after)) = name else {
            literal.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        let (name, path) = match name.split_once('.') {
            Some((key, path)) => (key, path.split('.').map(Box::from).collect::<Rc<[_]>>()),
            None => (name, Rc::from([])),
        };
        // A module's `ui.state` and `ui.persist` keys are its own (see
        // `module_key`): its templates name those first.
        let state = prefix
            .as_ref()
            .and_then(|prefix| {
                let own = format!("{prefix}{name}");
                env.store
                    .get(&own)
                    .or_else(|| env.persist.get(&script(), &own))
            })
            .or_else(|| env.store.get(name))
            .or_else(|| env.persist.get(&script(), name));
        // Shared state, as `ui.shared` reaches it: a plugin, only what the
        // app exposes.
        let state = match state {
            Some(state) => state,
            None => {
                let shared = env.shared.get(name).ok_or_else(|| {
                    let hint = if path.is_empty() {
                        String::new()
                    } else {
                        // `"{state.status}"`, a Luau path, for `{status}`.
                        format!(
                            " (a template names a state by its key, and a field of it after a dot: \
                             `{{{}}}` is state `{}`'s field `{}`)",
                            [name]
                                .into_iter()
                                .chain(path.iter().map(|p| &**p))
                                .collect::<Vec<_>>()
                                .join("."),
                            name,
                            path.join(".")
                        )
                    };
                    mlua::Error::runtime(format!(
                        "text `{text}` names unknown state `{name}`{hint}"
                    ))
                })?;
                env.permits(
                    &env.exposed_shared,
                    name,
                    &format!("text `{text}` names shared state"),
                )?;
                shared
            }
        };
        if !literal.is_empty() {
            segments.push(Segment::Literal(std::mem::take(&mut literal)));
        }
        segments.push(Segment::State(state, path));
        rest = after;
    }
    if segments.is_empty() {
        return Ok(Text::Literal(literal));
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Ok(Text::Template(segments))
}
/// `f` of a Luau string's text, invalid UTF-8 replaced (as
/// `to_string_lossy`): valid text (nearly every string) read where it lies,
/// not scanned for invalid sequences into a `String` first.
fn lossy<R>(string: &mlua::LuaString, f: impl FnOnce(&str) -> R) -> R {
    let bytes = string.as_bytes();
    match std::str::from_utf8(&bytes) {
        Ok(text) => f(text),
        Err(_) => f(&string.to_string_lossy()),
    }
}
/// `literal`: a string is shown as it is, braces and all (data: code,
/// titles, messages), not read as a template.
fn text_of(lua: &Lua, value: mlua::Value, what: &str, literal: bool) -> mlua::Result<Text> {
    // A state given as it is (a component's `ui.use_state`, which no
    // template can name): its value as text, followed without the script.
    if let Some(state) = state_of(&value) {
        return Ok(Text::Template(vec![Segment::State(state, Rc::from([]))]));
    }
    Ok(match value {
        mlua::Value::String(s) if literal => Text::Literal(lossy(&s, str::to_owned)),
        mlua::Value::String(s) => lossy(&s, |text| template(lua, text))?,
        mlua::Value::Function(f) => Text::Computed(Computed::Script(f)),
        mlua::Value::UserData(ref ud) if ud.is::<crate::i18n::MessageUd>() => Text::Computed(
            Computed::Message(ud.borrow::<crate::i18n::MessageUd>()?.0.clone()),
        ),
        mlua::Value::Number(n) => Text::Literal(Value::Num(n).to_string()),
        mlua::Value::Integer(n) => Text::Literal(n.to_string()),
        mlua::Value::Nil => Text::Literal(String::new()),
        other => {
            return Err(mlua::Error::runtime(format!(
                "{what} takes a string, a state or a function, not {}",
                other.type_name()
            )));
        }
    })
}

/// A child that isn't a node, string, function or list: a message
/// (`ui.t`), shown as its text, or an error. Out of line, off the path
/// most children take.
#[cold]
#[inline(never)]
fn uncommon_child(owner: &Weak<Vm>, value: mlua::Value) -> mlua::Result<Node> {
    match crate::i18n::message_of(&value) {
        Some(message) => Ok(Node::new(
            owner.clone(),
            Kind::Text(Text::Computed(Computed::Message(message))),
        )),
        None => Err(mlua::Error::runtime(format!(
            "a child must be a ui node, string, function or message, not {}",
            value.type_name()
        ))),
    }
}
/// A table's array entries in order, skipping holes: `cond and node or
/// nil` in a child list leaves a nil, which must not end the list (Lua's
/// length and `ipairs` both stop at the first hole).
pub(crate) fn array_items(table: &Table) -> mlua::Result<Vec<mlua::Value>> {
    let mut items = Vec::with_capacity(table.raw_len());
    table.for_each(|key, value| {
        if let mlua::Value::Integer(i) = key
            && i >= 1
        {
            items.push((i, value));
        }
        Ok(())
    })?;
    Ok(in_order(items))
}
/// Array entries by index: the array part comes in order, so this is one
/// pass unless the table has holes.
fn in_order<T>(mut items: Vec<(i64, T)>) -> Vec<T> {
    if !items.is_sorted_by_key(|(i, _)| *i) {
        items.sort_unstable_by_key(|(i, _)| *i);
    }
    items.into_iter().map(|(_, v)| v).collect()
}

/// A child as its table holds it: a node, read where it lies (no
/// reference to it is made), or any other value.
enum Child {
    Node(Rc<Node>),
    Value(mlua::Value),
}
impl Child {
    fn of(value: &mlua::StackValue<'_>) -> mlua::Result<Self> {
        if value.type_name() == "userdata"
            && let Ok(node) = value.get::<mlua::UserDataRef<NodeUd>>()
        {
            return Ok(Child::Node(node.0.clone()));
        }
        Ok(Child::Value(value.to_value()?))
    }
}

/// A table's indexed children, sorted when they are consumed.
fn child_items(table: &Table) -> mlua::Result<Vec<(i64, Child)>> {
    let mut items = Vec::with_capacity(table.raw_len());
    table.for_each_borrowed(|key, value| {
        if let mlua::Value::Integer(i) = key.to_value()?
            && i >= 1
        {
            items.push((i, Child::of(&value)?));
        }
        Ok(())
    })?;
    Ok(items)
}

/// Appends children from a table's array part: nodes, strings (text),
/// functions (computed text) and nested arrays, which flatten.
fn children(
    lua: &Lua,
    owner: &Weak<Vm>,
    mut items: Vec<(i64, Child)>,
    out: &mut Vec<Rc<Node>>,
) -> mlua::Result<()> {
    if !items.is_sorted_by_key(|(i, _)| *i) {
        items.sort_unstable_by_key(|(i, _)| *i);
    }
    for (_, child) in items {
        let child = match child {
            Child::Node(node) => {
                out.push(node);
                continue;
            }
            Child::Value(value) => value,
        };
        match child {
            mlua::Value::UserData(ud) if ud.is::<NodeUd>() => {
                out.push(ud.borrow::<NodeUd>()?.0.clone())
            }
            mlua::Value::String(s) => out.push(Rc::new(Node::new(
                owner.clone(),
                Kind::Text(lossy(&s, |text| template(lua, text))?),
            ))),
            mlua::Value::Function(f) => out.push(Rc::new(Node::new(
                owner.clone(),
                Kind::Text(Text::Computed(Computed::Script(f))),
            ))),
            mlua::Value::Table(nested) => children(lua, owner, child_items(&nested)?, out)?,
            mlua::Value::Nil | mlua::Value::Boolean(false) => {}
            other => out.push(Rc::new(uncommon_child(owner, other)?)),
        }
    }
    Ok(())
}

/// A node's accessibility beyond its name (`Node::a11y`).
#[derive(Default, Clone)]
pub(crate) struct A11y {
    pub role: Option<zgx::semantics::Role>,
    /// A heading's level, 1 to 6 (a byte: the box's size class is kept).
    pub level: Option<u8>,
    pub selected: Option<Source>,
    /// `checked` and `expanded`: a custom switch's state, a disclosure's.
    pub checked: Option<Source>,
    /// `role = "menuitemradio"`: one of a group of checkable menu items.
    pub radio_item: bool,
    pub expanded: Option<Source>,
    pub live: Option<zgx::semantics::Live>,
    pub description: Option<Rc<str>>,
    pub hidden: bool,
    pub focus_group: Option<zgx::input::FocusGroup>,
    /// A `label` given as a state or function.
    pub label: Option<Source>,
    /// What is wrong with its value (`aria-invalid`), followed: nil while
    /// it is fine.
    pub error: Option<Source>,
    /// `hook = "toolbar.save"`: a point the app declares for plugins, as
    /// they target it (`hook:toolbar.save`). Here, with what else names the
    /// element to its readers, as few elements have one.
    pub hook: Option<Rc<str>>,
}
/// `focus_group`'s axes.
fn focus_group(axis: &str) -> mlua::Result<zgx::input::FocusGroup> {
    use zgx::input::FocusGroup;
    match axis {
        "vertical" => Ok(FocusGroup::Vertical),
        "horizontal" => Ok(FocusGroup::Horizontal),
        "both" => Ok(FocusGroup::Both),
        other => Err(mlua::Error::runtime(format!(
            "`focus_group` is \"vertical\", \"horizontal\" or \"both\", not `{other}`"
        ))),
    }
}
/// `live`'s modes, as `aria-live` names them.
fn live_mode(mode: &str) -> mlua::Result<Option<zgx::semantics::Live>> {
    match mode {
        "polite" => Ok(Some(zgx::semantics::Live::Polite)),
        "assertive" => Ok(Some(zgx::semantics::Live::Assertive)),
        "off" => Ok(None),
        other => Err(mlua::Error::runtime(format!(
            "`live` is \"polite\", \"assertive\" or \"off\", not `{other}`"
        ))),
    }
}
/// A button's `variant`, as the styles it stands for, written into its
/// props where the script set none (its own `bg` still wins).
fn button_look(lua: &Lua, table: &Table) -> mlua::Result<()> {
    let variant = table.get::<Option<String>>("variant")?;
    table.raw_set("variant", mlua::Value::Nil)?;
    let hover = |field: &str, value: mlua::Value| -> mlua::Result<mlua::Value> {
        let t = lua.create_table()?;
        t.raw_set(field, value)?;
        Ok(mlua::Value::Table(t))
    };
    let str = |s: &str| lua.create_string(s).map(mlua::Value::String);
    let look: Vec<(&str, mlua::Value)> = match variant.as_deref() {
        None | Some("default") => Vec::new(),
        Some("primary") => vec![
            ("bg", str("$accent")?),
            ("color", str("$accent.text")?),
            ("hover", hover("bg", str("$accent.hover")?)?),
        ],
        Some("danger") => vec![
            ("bg", str("$danger")?),
            ("color", str("$accent.text")?),
            ("hover", hover("brightness", mlua::Value::Number(1.1))?),
        ],
        Some("ghost") => vec![
            ("bg", str("#00000000")?),
            ("hover", hover("bg", str("$hover")?)?),
        ],
        Some(other) => {
            return Err(mlua::Error::runtime(format!(
                "a button's `variant` is \"primary\", \"ghost\", \"danger\" or \"default\", not `{other}`"
            )));
        }
    };
    for (key, value) in look {
        if table.raw_get::<mlua::Value>(key)?.is_nil() {
            table.raw_set(key, value)?;
        }
    }
    Ok(())
}
/// Whether a `role` says it is checked: a menu item with a checkmark.
fn checkable_menu_role(name: &str) -> bool {
    matches!(name, "menuitemcheckbox" | "menuitemradio")
}
/// An `underline_style`, as CSS's `text-decoration-style` names it.
pub(crate) fn line_style(name: &str) -> mlua::Result<zgx::rich_text::LineStyle> {
    use zgx::rich_text::LineStyle;
    Ok(match name {
        "solid" => LineStyle::Solid,
        "double" => LineStyle::Double,
        "dotted" => LineStyle::Dotted,
        "dashed" => LineStyle::Dashed,
        "wavy" => LineStyle::Wavy,
        other => {
            return Err(mlua::Error::runtime(format!(
                "unknown `underline_style` `{other}`: one of solid, double, dotted, dashed, wavy"
            )));
        }
    })
}
/// A `role` as ARIA names it.
fn a11y_role(name: &str) -> Option<zgx::semantics::Role> {
    use zgx::semantics::Role;
    Some(match name {
        "group" => Role::Group,
        "button" => Role::Button,
        "link" => Role::Link,
        "heading" => Role::Heading,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "listbox" => Role::ListBox,
        "option" => Role::ListBoxOption,
        "toolbar" => Role::Toolbar,
        "status" => Role::Status,
        "tablist" => Role::TabList,
        "tab" => Role::Tab,
        "dialog" => Role::Dialog,
        "img" | "image" => Role::Image,
        "tabpanel" => Role::TabPanel,
        "article" => Role::Article,
        "radiogroup" => Role::RadioGroup,
        "radio" => Role::RadioButton,
        "switch" => Role::Switch,
        "checkbox" => Role::CheckBox,
        "spinbutton" => Role::SpinButton,
        "combobox" => Role::ComboBox,
        "tree" => Role::Tree,
        "treeitem" => Role::TreeItem,
        "grid" => Role::Grid,
        "row" => Role::Row,
        "gridcell" | "cell" => Role::Cell,
        "columnheader" => Role::ColumnHeader,
        "alert" => Role::Alert,
        "menu" => Role::Menu,
        "menuitem" | "menuitemcheckbox" | "menuitemradio" => Role::MenuItem,
        _ => return None,
    })
}

/// Rows past which `ui.each` suggests `ui.list`.
const LONG_EACH: usize = 1000;

/// A menu item's options: whether it leaves its menu open when chosen
/// (`keep_open`), whether it is checked (`checked`, a radio's with
/// `radio`), and the keys it shows (`shortcut`).
#[derive(Default, Clone)]
pub(crate) struct MenuItemOptions {
    pub keep_open: bool,
    pub checked: Option<Source>,
    pub radio: bool,
    pub shortcut: Option<String>,
}

/// What an input edits: a state, both ways, or (controlled, as React's
/// inputs are) what a function shows, edits going out by `on_change` and
/// `on_commit`; focused, the input keeps what is being typed.
#[derive(Clone)]
pub(crate) enum InputValue {
    /// A state, both ways, and its `on_commit(text)` (Enter, or leaving it
    /// changed).
    State {
        value: Signal<Value>,
        commit: Option<Handler>,
    },
    /// What it shows, and its `on_change(text)` (each edit) and
    /// `on_commit(text)` (Enter, or leaving it changed).
    Shown {
        shown: Source,
        change: Option<Handler>,
        commit: Option<Handler>,
    },
}

/// Fields a constructor reads itself rather than as style properties.
const WIDGET_FIELDS: &[&str] = &[
    "label",
    "value",
    "min",
    "max",
    "multiline",
    "offset",
    "follow",
    "scrollbar",
    "keyboard",
    "src",
    "code",
    "draw",
    "build",
    "field",
    "each",
    "draw_item",
    "view",
    "bg_grid",
    "count",
    "data",
    "height",
    "overscan",
    "heights",
    "measured",
    "row_text",
    "sticky",
    "placement",
    "fallback",
    "trigger",
    "autofocus",
    "open_over",
    "arrow",
    "row",
    "open",
    "anchor",
    "key",
    "enter",
    "exit",
    "mode",
    "initial",
    "placeholder",
    "placeholder_color",
    "mixed",
    "select_on_focus",
    "secure",
    "spellcheck",
    "line_numbers",
    "highlight",
    "decorations",
    "underlines",
    "find",
    "current_line",
    "tab",
    "on_caret",
    "on_top_line",
    "animate",
    "size",
    "vertical",
    "min_rest",
    "second",
    "on_change",
    "on_commit",
    "dismiss_on_backdrop",
    "match_anchor_width",
    "step",
    "options",
    "decimals",
    "summary",
    "title",
    "reorderable",
    "keep",
    "group",
    "on_move",
    "on_tear_off",
    "on_close",
    "panel",
    "text",
    "loading",
    "empty",
    "commands",
    "on_run",
    "on_open",
    "on_activate",
    "columns",
    "sort",
    "on_edit",
    "selection",
    "cell",
    "first_weekday",
    "unavailable",
    "format",
    "hour12",
    "minute_step",
];
/// The keys `late_field` takes: looked for only once a key is no style
/// property. Every key is compared against `WIDGET_FIELDS`, so a field
/// added there costs each style key of its length a comparison, and one
/// added here costs them nothing (a test checks none is a property).
#[cfg(test)]
const LATE_FIELDS: &[&str] = &[
    "hit",
    "inputmode",
    "caret_color",
    "caret_width",
    "hook",
    "private",
    "reorder",
    "item_label",
    "on_end",
    "end_distance",
    "active_descendant",
    "at_caret",
    "on_start",
    "start_distance",
    "at_end",
    "on_paste",
    "overscroll",
    "read_only",
    "selection_radius",
    "autocorrect",
    "smart_punctuation",
];

/// A key no style property names, taken where it lies once the walk finds
/// it, so no style key compares against it: an input's caret (few set it,
/// read only from those that do), whether plugins may target the node
/// (`hook`, `private`), an input's `on_paste`, `active_descendant`, `read_only`,
/// `autocorrect` and `smart_punctuation`, a
/// popover's `at_caret`; `inputmode` is read by `ui.input` itself, `on_end` and
/// `end_distance` by `ui.list`. Any other key is unknown.
#[cold]
#[inline(never)]
fn late_field(lua: &Lua, node: &mut Node, name: &str, value: mlua::Value) -> mlua::Result<()> {
    match name {
        // A theme scope of its own: what reads `$selection.radius` below it
        // (a selectable region's highlight, an editor's) follows it.
        "selection_radius" => {
            let radius = source(lua, Ty::Num, name, value)?;
            let given = node.theme.take();
            node.theme = Some(ThemeSpec::SelectionRadius(Box::new((given, radius))));
        }
        "caret_color" | "caret_width" => {
            let Kind::Input { rare, .. } = &mut node.kind else {
                return Ok(());
            };
            if name == "caret_color" {
                let color = source(lua, Ty::Color, name, value)?;
                rare.get_or_insert_default().caret_color = Some(color);
            } else {
                let width: f64 = lua.unpack(value)?;
                rare.get_or_insert_default().caret_width = Some(width as f32);
            }
        }
        "hit" => {
            let at: String = lua.unpack(value)?;
            let target = at == "target";
            if !target && at != "shown" {
                return Err(mlua::Error::runtime(
                    "`hit` takes \"target\" (where an animation is taking it) or \"shown\"",
                ));
            }
            let layout = node.layout.get_or_insert(LayoutAnim {
                width: false,
                height: false,
                position: false,
                duration: None,
                ease: None,
                enter: false,
                clip: false,
                hit: false,
            });
            layout.hit = target;
        }
        "inputmode" => {}
        // `ui.each`'s reorder, read by `ui.each` itself.
        "reorder" | "item_label" => {}
        // `ui.list`'s, read as the list is made.
        "on_end" | "end_distance" => {}
        "active_descendant" => {
            if let Kind::Input { rare, .. } = &mut node.kind {
                let active = source(lua, Ty::Str, name, value)?;
                rare.get_or_insert_default().active_descendant = Some(active);
            }
        }
        "on_paste" => {
            if let Kind::Input { rare, .. } = &mut node.kind {
                rare.get_or_insert_default().on_paste = Some(lua.unpack(value)?);
            }
        }
        "read_only" => {
            if let Kind::Input { rare, .. } = &mut node.kind {
                let read_only = source(lua, Ty::Bool, name, value)?;
                rare.get_or_insert_default().read_only = Some(read_only);
            }
        }
        "autocorrect" | "smart_punctuation" => {
            if let Kind::Input { rare, .. } = &mut node.kind {
                let on: bool = lua.unpack(value)?;
                let rare = rare.get_or_insert_default();
                match name {
                    "autocorrect" => rare.autocorrect = on,
                    _ => rare.smart_punctuation = on,
                }
            }
        }
        "on_start" | "start_distance" | "at_end" => {
            if let Kind::List { rare, .. } = &mut node.kind {
                let rare = rare.get_or_insert_default();
                match name {
                    "on_start" => rare.start = Some(lua.unpack(value)?),
                    "start_distance" => {
                        let distance: f32 = lua.unpack(value)?;
                        rare.start_distance = distance.is_finite().then_some(distance);
                    }
                    _ => rare.at_end = Some(expect_state(&value, "`at_end`")?),
                }
            }
        }
        "overscroll" => {
            let contain = match value {
                mlua::Value::String(ref text) if &*text.to_str()? == "contain" => true,
                mlua::Value::String(ref text) if &*text.to_str()? == "auto" => false,
                _ => {
                    return Err(mlua::Error::runtime(
                        "`overscroll` is \"auto\" or \"contain\"",
                    ));
                }
            };
            match &mut node.kind {
                Kind::Scroll { contain: set, .. } => *set = contain,
                Kind::List { rare, .. } => rare.get_or_insert_default().contain = contain,
                Kind::Input { rare, .. } => rare.get_or_insert_default().contain = contain,
                _ => {}
            }
        }
        "at_caret" => {
            if let Kind::Popover { at_caret, .. } = &mut node.kind {
                *at_caret = lua.unpack::<Option<bool>>(value)?.unwrap_or(false);
            }
        }
        "private" => node.private = lua.unpack(value)?,
        "hook" => match value {
            mlua::Value::String(name) => {
                node.a11y.get_or_insert_default().hook = Some(name.to_str()?.as_ref().into())
            }
            mlua::Value::Boolean(false) => node.private = true,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "`hook` takes a name (a hook plugins target) or false (none may), not {}",
                    other.type_name()
                )));
            }
        },
        _ => return Err(mlua::Error::runtime(format!("unknown property `{name}`"))),
    }
    Ok(())
}
/// Sets the handler `key` names, or gives `value` back if it names none.
fn set_handler(
    events: &mut Option<Box<Events>>,
    key: &str,
    value: mlua::Value,
) -> mlua::Result<Option<mlua::Value>> {
    if !(key.starts_with("on_")
        || matches!(
            key,
            "context_menu"
                | "zoomable"
                | "drag_preview"
                | "drag_preview_anchor"
                | "drag_preview_offset"
                | "drop_when"
                | "drag_export"
                | "drag_moves_window"
        ))
    {
        return Ok(Some(value));
    }
    let events = events.get_or_insert_default();
    if key == "zoomable" {
        events.zoomable = Some(expect_state(&value, "`zoomable`")?);
        return Ok(None);
    }
    if let "drag_preview_anchor" | "drag_preview_offset" = key {
        let mlua::Value::Table(t) = value else {
            return Err(mlua::Error::runtime(format!("{key} takes {{ x, y }}")));
        };
        let at = |i: i64, name: &str| -> mlua::Result<f32> {
            match t.get::<Option<f32>>(i)? {
                Some(v) => Ok(v),
                None => Ok(t.get::<Option<f32>>(name)?.unwrap_or(0.)),
            }
        };
        let point = Some((at(1, "x")?, at(2, "y")?));
        if key == "drag_preview_anchor" {
            events.drag_preview_anchor = point;
        } else {
            events.drag_preview_offset = point;
        }
        return Ok(None);
    }
    let slot = match key {
        "on_click" => &mut events.click,
        "on_press" => &mut events.press,
        "on_release" => &mut events.release,
        "on_aux_click" => &mut events.rare.get_or_insert_default().aux_click,
        "on_double_click" => &mut events.rare.get_or_insert_default().double_click,
        "on_navigate" => &mut events.rare.get_or_insert_default().navigate,
        "on_file_drop" => &mut events.rare.get_or_insert_default().file_drop,
        "on_file_hover" => &mut events.rare.get_or_insert_default().file_hover,
        "on_hover" => &mut events.hover,
        "on_pointer_move" => &mut events.pointer_move,
        "on_key" => &mut events.key,
        "on_key_up" => &mut events.key_up,
        "on_focus" => &mut events.focus,
        "on_blur" => &mut events.blur,
        "on_wheel" => &mut events.wheel,
        "on_pinch" => &mut events.pinch,
        "on_mount" => {
            let mlua::Value::Function(f) = value else {
                return Err(mlua::Error::runtime("on_mount takes a function"));
            };
            events.mount = Some(f);
            return Ok(None);
        }
        "on_resize" => {
            let mlua::Value::Function(f) = value else {
                return Err(mlua::Error::runtime("on_resize takes a function"));
            };
            events.resize = Some(f);
            return Ok(None);
        }
        "on_visible" => {
            let mlua::Value::Function(f) = value else {
                return Err(mlua::Error::runtime("on_visible takes a function"));
            };
            events.visible = Some(f);
            return Ok(None);
        }
        "context_menu" => {
            let mlua::Value::Function(f) = value else {
                return Err(mlua::Error::runtime("context_menu takes a function"));
            };
            events.context_menu = Some(f);
            return Ok(None);
        }
        "on_drag" | "drag_preview" | "on_drag_end" | "drag_export" | "on_drag_outside"
        | "drag_moves_window" | "on_drop" | "drop_when" | "on_drag_over" | "on_drag_leave"
        | "on_drag_spring" => {
            let mlua::Value::Function(f) = value else {
                return Err(mlua::Error::runtime(format!("{key} takes a function")));
            };
            *match key {
                "on_drag" => &mut events.drag,
                "drag_preview" => &mut events.drag_preview,
                "on_drag_end" => &mut events.drag_end,
                "drag_export" => &mut events.drag_export,
                "on_drag_outside" => &mut events.drag_outside,
                "drag_moves_window" => &mut events.drag_moves_window,
                "on_drop" => &mut events.drop,
                "drop_when" => &mut events.drop_when,
                "on_drag_over" => &mut events.drag_over,
                "on_drag_leave" => &mut events.drag_leave,
                _ => &mut events.drag_spring,
            } = Some(f);
            return Ok(None);
        }
        _ => return Ok(Some(value)),
    };
    *slot = Some(handler(value)?);
    // Every slot but `on_click`'s is an input handler.
    events.input |= key != "on_click";
    Ok(None)
}

/// Style keys a script has used, by the address of their (interned) Lua
/// string: one met before is its `Prop` without reading or matching the
/// name again. Each entry holds its string, so the address is not reused
/// while it is here, and there are at most as many as there are props.
#[derive(Default)]
pub(crate) struct StyleKeys(
    RefCell<std::collections::HashMap<usize, (mlua::LuaString, Prop), BuildAddressHasher>>,
);
type BuildAddressHasher = std::hash::BuildHasherDefault<AddressHasher>;
#[derive(Default)]
struct AddressHasher(u64);
impl std::hash::Hasher for AddressHasher {
    fn finish(&self) -> u64 {
        // Aligned addresses end in zeros: fold the high bits down.
        self.0 ^ (self.0 >> 32)
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_usize(&mut self, address: usize) {
        self.0 = (address as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

/// Fills a node's properties, pseudo-states, handlers and children.
pub(crate) fn fill(lua: &Lua, mut node: Node, table: Option<&Table>) -> mlua::Result<Node> {
    if let Some(table) = table {
        fill_from(lua, &mut node, table, None, crate::design::mode(lua))?;
    }
    Ok(node)
}
/// What a `ui.text` table holds besides styles: its content (element 1)
/// and whether to show it `literal`ly.
#[derive(Default)]
struct TextArgs {
    content: mlua::Value,
    literal: Option<bool>,
}
/// A numeric literal read without making either intermediate value enum.
struct StackNumber(Option<f64>);
impl FromLua for StackNumber {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        Ok(Self(match value {
            mlua::Value::Integer(n) => Some(n as f64),
            mlua::Value::Number(n) => Some(n),
            _ => None,
        }))
    }
    unsafe fn from_stack(idx: std::ffi::c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        use mlua::ffi;
        let state = lua.state();
        // SAFETY: the borrowed pair holds this slot during conversion.
        // Reading a checked number neither allocates nor calls script.
        let number = unsafe {
            (ffi::lua_type(state, idx) == ffi::LUA_TNUMBER)
                .then(|| ffi::lua_tonumberx(state, idx, std::ptr::null_mut()))
        };
        Ok(Self(number))
    }
}
/// As `fill`, in place (a node is large to move), keeping a text's own
/// fields in `text`.
fn fill_from(
    lua: &Lua,
    node: &mut Node,
    table: &Table,
    mut text: Option<&mut TextArgs>,
    lint: crate::DesignLint,
) -> mlua::Result<()> {
    let mut layout_given = false;
    let mut variants = None;
    // One walk over the table: properties as they come, children kept to
    // put in order after.
    let mut items = Vec::new();
    let keys = lua.app_data_ref::<StyleKeys>();
    let lint = Some(lint);
    // Keys are read where they lie on the stack (a string key's identity
    // finds its property): no reference is made for them.
    table.for_each_borrowed(|borrowed, value| {
        if borrowed.as_bytes().is_none() {
            match borrowed.to_value()? {
                mlua::Value::Integer(1) if let Some(text) = text.as_deref_mut() => {
                    text.content = value.to_value()?;
                }
                mlua::Value::Integer(i) if i >= 1 => items.push((i, Child::of(&value)?)),
                _ => {}
            }
            return Ok(());
        }
        let address = borrowed.to_pointer() as usize;
        let known = keys
            .as_ref()
            .and_then(|keys| keys.0.borrow().get(&address).map(|(_, prop)| *prop));
        if let Some(prop) = known {
            // Literal colours and numbers are read where they lie, without
            // making intermediate values. Everything else uses the general
            // conversion and its errors, as an unfamiliar key does.
            let written = match prop.ty() {
                Ty::Color => value
                    .as_str()
                    .filter(|s| !s.starts_with('$'))
                    .and_then(parse_color)
                    .map(Lit::Color),
                ty @ (Ty::Num | Ty::Len | Ty::Pair) => value.get::<StackNumber>()?.0.map(|n| {
                    match ty {
                        Ty::Num => Lit::Num(n as f32),
                        Ty::Len => Lit::Len(Length::Px(n as f32)),
                        _ => Lit::Pair(n as f32, n as f32),
                    }
                }),
                _ => None,
            };
            let source = match written {
                Some(lit) => {
                    lint_literal(lua, prop.name(), &lit, lint)?;
                    Source::Lit(lit)
                }
                None => source_linted(lua, prop.ty(), prop.name(), value.to_value()?, lint)?,
            };
            node.props.push((prop, source));
            return Ok(());
        }
        let value = value.to_value()?;
        let key = borrowed.as_str().ok_or_else(|| mlua::Error::FromLuaConversionError {
            from: "string",
            to: "&str".into(),
            message: Some("invalid utf-8 encoding".into()),
        })?;
        if key == "literal"
            && let Some(text) = text.as_deref_mut()
        {
            text.literal = lua.unpack(value)?;
            return Ok(());
        }
        let Some(value) = set_handler(&mut node.events, key, value)? else {
            return Ok(());
        };
        match key {
            "id" => node.id = Some(lua.unpack::<String>(value)?.into()),
            "class" => {
                let classes: String = lua.unpack(value)?;
                node.classes = classes.split_whitespace().map(Rc::from).collect();
            }
            "ref" => match value {
                mlua::Value::UserData(ud) if ud.is::<RefUd>() => {
                    node.refs.push(ud.borrow::<RefUd>()?.0.clone())
                }
                mlua::Value::Table(list) => {
                    for r in list.sequence_values::<mlua::AnyUserData>() {
                        node.refs.push(r?.borrow::<RefUd>()?.0.clone());
                    }
                }
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "`ref` takes ui.ref(), not {}",
                        other.type_name()
                    )));
                }
            },
            "focusable" => node.focusable = Some(lua.unpack(value)?),
            "window_drag" => node.window_drag = lua.unpack(value)?,
            "selectable" => node.selectable = lua.unpack(value)?,
            "selection_color" | "selected_text_color" => {
                let at = usize::from(key == "selected_text_color");
                node.selection_colors.get_or_insert_default()[at] =
                    Some(source(lua, Ty::Color, key, value)?);
            }
            "tooltip" => {
                let content = match value {
                    mlua::Value::Nil => None,
                    mlua::Value::String(text) => Some(TooltipContent::Text(text.to_string_lossy())),
                    mlua::Value::UserData(ud) if ud.is::<NodeUd>() => {
                        Some(TooltipContent::Node(ud.borrow::<NodeUd>()?.0.clone()))
                    }
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "`tooltip` takes a string or a node, not {}",
                            other.type_name()
                        )));
                    }
                };
                node.tooltip.get_or_insert_default().content = content;
            }
            "tooltip_placement" => {
                node.tooltip.get_or_insert_default().placement = placement_of(value)?;
            }
            "tooltip_interactive" => {
                node.tooltip.get_or_insert_default().interactive = lua.unpack(value)?;
            }
            // Milliseconds the pointer rests before it shows (300).
            "tooltip_delay" => {
                let ms: f64 = lua.unpack(value)?;
                node.tooltip.get_or_insert_default().delay =
                    Some(crate::duration_ms("tooltip_delay", ms)?);
            }
            "pointer_events" => {
                node.pointer_events = Some(match &value {
                    mlua::Value::Boolean(_) | mlua::Value::String(_) => {
                        let Some(enabled) = pointer_events(&lua.unpack::<Value>(value)?) else {
                            return Err(mlua::Error::runtime(
                                "`pointer_events` takes \"none\", \"auto\" or a boolean",
                            ));
                        };
                        Source::Lit(Lit::Bool(enabled))
                    }
                    _ => source(lua, Ty::Bool, "pointer_events", value)?,
                })
            }
            "bg_shader" => {
                let mlua::Value::UserData(ud) = &value else {
                    return Err(mlua::Error::runtime("`bg_shader` takes a ui.shader"));
                };
                node.shaders.get_or_insert_default().fill =
                    Some(ud.borrow::<ShaderUd>()?.0.clone());
            }
            "shader_params" => {
                let shaders = node.shaders.get_or_insert_default();
                shaders.params = Some(shader_params(
                    lua,
                    &value,
                    "shader_params",
                    Some(&mut shaders.tokens),
                )?);
            }
            "layer_shader" => {
                let mlua::Value::UserData(ud) = &value else {
                    return Err(mlua::Error::runtime(
                        "`layer_shader` takes a ui.layer_shader",
                    ));
                };
                node.shaders.get_or_insert_default().layer =
                    Some(ud.borrow::<LayerShaderUd>()?.0.clone());
            }
            "layer_params" => {
                node.shaders.get_or_insert_default().layer_params =
                    Some(shader_params(lua, &value, "layer_params", None)?);
            }
            "mask" => {
                let (bitmap, t) = match &value {
                    mlua::Value::UserData(ud) => (ud.borrow::<BitmapUd>()?.0.clone(), None),
                    mlua::Value::Table(t) => (
                        t.get::<mlua::AnyUserData>(1)?
                            .borrow::<BitmapUd>()?
                            .0
                            .clone(),
                        Some(t),
                    ),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "`mask` takes a ui.bitmap, not {}",
                            other.type_name()
                        )));
                    }
                };
                let get = |key: &str, default: f32| -> mlua::Result<f32> {
                    Ok(match t {
                        Some(t) => t.get::<Option<f32>>(key)?.unwrap_or(default),
                        None => default,
                    })
                };
                let size = (
                    get("w", bitmap.width() as f32)?,
                    get("h", bitmap.height() as f32)?,
                );
                let offset = (get("x", 0.)?, get("y", 0.)?);
                let sweep = match &t {
                    Some(t) => match t.get::<Option<mlua::Table>>("sweep")? {
                        Some(sweep) => {
                            let num = |key: &str, default: f64| -> mlua::Result<f64> {
                                Ok(sweep.get::<Option<f64>>(key)?.unwrap_or(default))
                            };
                            let ms = num("duration", 2000.)?;
                            if !(ms.is_finite() && ms > 0.) {
                                return Err(mlua::Error::runtime(
                                    "a mask's `sweep` takes a duration in milliseconds above 0",
                                ));
                            }
                            Some(Sweep {
                                by: (num("x", 0.)? as f32, num("y", 0.)? as f32),
                                period: crate::duration_ms("sweep's duration", ms)?,
                                rate: num("frame_rate", 60.)?,
                            })
                        }
                        None => None,
                    },
                    None => None,
                };
                node.mask = Some(Mask {
                    image: bitmap,
                    size,
                    offset,
                    sweep,
                });
            }
            "layout_id" => node.layout_id = Some(lua.unpack::<String>(value)?.into()),
            "layout" => {
                layout_given = true;
                let (width, height, position) = match &value {
                    mlua::Value::Boolean(true) => (false, true, false),
                    mlua::Value::Boolean(false) | mlua::Value::Nil => (false, false, false),
                    mlua::Value::String(s) => match &*s.to_string_lossy() {
                        "height" => (false, true, false),
                        "width" => (true, false, false),
                        "size" => (true, true, false),
                        "position" => (false, false, true),
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "layout is \"height\", \"width\", \"size\" or \"position\", not `{other}`"
                            )));
                        }
                    },
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "layout cannot be {}",
                            other.type_name()
                        )));
                    }
                };
                let layout = node.layout.get_or_insert(LayoutAnim {
                    width,
                    height,
                    position,
                    duration: None,
                    ease: None,
                    enter: false,
                    clip: false,
                    hit: false,
                });
                layout.width = width;
                layout.height = height;
                layout.position = position;
                if !width && !height && !position && !layout.hit {
                    node.layout = None;
                }
            }
            "layout_duration" | "layout_ease" | "layout_enter" | "layout_clip" => {
                let layout = node.layout.get_or_insert(LayoutAnim {
                    width: false,
                    height: true,
                    position: false,
                    duration: None,
                    ease: None,
                    enter: false,
                    clip: false,
                    hit: false,
                });
                if key == "layout_enter" {
                    layout.enter = lua.unpack::<bool>(value)?;
                } else if key == "layout_clip" {
                    layout.clip = lua.unpack::<bool>(value)?;
                } else if key == "layout_duration" {
                    let ms = lua.unpack::<f64>(value)?;
                    crate::check_duration(lua, "layout_duration", ms, false);
                    layout.duration = Some(ms);
                } else {
                    layout.ease = match value {
                        mlua::Value::String(s) => Some(
                            crate::anime::ease::Ease::parse(&s.to_string_lossy())
                                .map_err(mlua::Error::runtime)?,
                        ),
                        mlua::Value::UserData(ud) => {
                            Some(ud.borrow::<crate::anime::api::EaseUd>()?.0.clone())
                        }
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "layout_ease cannot be {}",
                                other.type_name()
                            )));
                        }
                    };
                }
            }
            "disabled" if !matches!(value, mlua::Value::Table(_)) => {
                node.disabled = Some(source(lua, Ty::Bool, "disabled", value)?);
            }
            "context_menu_below" => {
                let mlua::Value::Boolean(below) = value else {
                    return Err(mlua::Error::runtime("`context_menu_below` takes a boolean"));
                };
                node.events.get_or_insert_default().context_menu_below = below;
            }
            "context_menu_style" => {
                let mlua::Value::Table(styles) = value else {
                    return Err(mlua::Error::runtime(
                        "`context_menu_style` takes a table of styles",
                    ));
                };
                let mut lits = Vec::new();
                for pair in styles.pairs::<String, mlua::Value>() {
                    let (name, value) = pair?;
                    let prop = Prop::parse(&name).ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "unknown style `{name}` in `context_menu_style`"
                        ))
                    })?;
                    let Source::Lit(lit) = source(lua, prop.ty(), &name, value)? else {
                        return Err(mlua::Error::runtime(format!(
                            "`context_menu_style.{name}` must be a plain value"
                        )));
                    };
                    lits.push((prop, lit));
                }
                node.events.get_or_insert_default().context_menu_style = Some(lits.into());
            }
            "provide" => {
                let mlua::Value::Table(values) = value else {
                    return Err(mlua::Error::runtime(
                        "`provide` takes a table of names and values: `provide = { user = user }`",
                    ));
                };
                let mut provided: Vec<(Rc<str>, mlua::Value)> = Vec::new();
                for pair in values.pairs::<String, mlua::Value>() {
                    let (name, value) = pair?;
                    provided.push((name.as_str().into(), value));
                }
                node.provide = Some(provided.into());
            }
            "theme" => {
                let spec = if let Some(state) = state_of(&value) {
                    ThemeSpec::State(state)
                } else if let mlua::Value::Function(f) = value {
                    ThemeSpec::Computed(f)
                } else {
                    let value = lua.unpack::<Value>(value)?;
                    ThemeSpec::Fixed(
                        crate::theme::override_of(&value).map_err(mlua::Error::runtime)?,
                    )
                };
                // Given after `selection_radius`: inside it.
                match &mut node.theme {
                    Some(ThemeSpec::SelectionRadius(both)) => both.0 = Some(spec),
                    theme => *theme = Some(spec),
                }
            }
            "hover" | "active" | "focus" | "disabled" => {
                let pseudo = match key {
                    "hover" => Pseudo::Hover,
                    "active" => Pseudo::Active,
                    "focus" => Pseudo::Focus,
                    _ => Pseudo::Disabled,
                };
                let mlua::Value::Table(styles) = value else {
                    return Err(mlua::Error::runtime(format!(
                        "`{key}` takes a table of styles"
                    )));
                };
                let mut lits = Vec::new();
                for pair in styles.pairs::<String, mlua::Value>() {
                    let (name, value) = pair?;
                    let prop = Prop::parse(&name).ok_or_else(|| {
                        mlua::Error::runtime(format!("unknown style `{name}` in `{key}`"))
                    })?;
                    let Source::Lit(lit) = source(lua, prop.ty(), &name, value)? else {
                        return Err(mlua::Error::runtime(format!(
                            "`{key}.{name}` must be a plain value; animate the base style instead"
                        )));
                    };
                    lits.push((prop, lit));
                }
                lits.sort_by_key(|(prop, _)| specificity(*prop));
                node.pseudo.push((pseudo, lits.into()));
            }
            "variants" => match value {
                mlua::Value::UserData(ud) if ud.is::<crate::variants::SelectionUd>() => {
                    variants = Some(ud.borrow::<crate::variants::SelectionUd>()?.0.clone());
                }
                mlua::Value::UserData(ud) if ud.is::<crate::variants::RecipeUd>() => {
                    // The defaults: `variants = button`.
                    let recipe = ud.borrow::<crate::variants::RecipeUd>()?.0.clone();
                    variants = Some(Rc::new(crate::variants::select_defaults(lua, &recipe)?));
                }
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "`variants` takes what a ui.variants definition returns when called \
                         (`variants = button {{ tone = \"ghost\" }}`), not {}",
                        other.type_name()
                    )));
                }
            },
            "label"
                if matches!(
                    node.kind,
                    Kind::Column
                        | Kind::Row
                        | Kind::Div
                        | Kind::Overlay
                        | Kind::Button
                        | Kind::Scroll { .. }
                        | Kind::List { .. }
                        // A canvas named is read as a picture (a chart).
                        | Kind::Canvas(..)
                ) || !matches!(value, mlua::Value::String(_)) =>
            {
                // A widget's label that changes (a message, a state, a
                // function): its constructor read what it says now
                // (`label`), and this follows it. A plain string stays the
                // constructor's alone.
                match value {
                    mlua::Value::String(label) => node.label = Some(label.to_string_lossy().into()),
                    // A state or function: followed (a recycled row's name).
                    value => {
                        node.a11y.get_or_insert_default().label =
                            Some(source(lua, Ty::Str, "label", value)?);
                    }
                }
            }
            "role" => {
                let name: String = lua.unpack(value)?;
                let role = a11y_role(&name).ok_or_else(|| {
                    mlua::Error::runtime(format!(
                        "unknown role `{name}`: one of group, button, link, heading, list, \
                         listitem, listbox, option, toolbar, status, tablist, tab, tabpanel, \
                         dialog, img, radiogroup, radio, switch, checkbox, spinbutton, combobox, \
                         tree, treeitem, grid, row, gridcell, columnheader, alert, menu, \
                         menuitem, menuitemcheckbox, menuitemradio"
                    ))
                })?;
                // ARIA requires their state; there is no saying "unknown".
                if checkable_menu_role(&name) && !table.contains_key("checked")? {
                    return Err(mlua::Error::runtime(format!(
                        "role `{name}` says whether it is checked: give it `checked`"
                    )));
                }
                let a11y = node.a11y.get_or_insert_default();
                a11y.role = Some(role);
                a11y.radio_item = name == "menuitemradio";
            }
            "heading" => {
                let level: f64 = lua.unpack(value)?;
                if !(1. ..=6.).contains(&level) || level.fract() != 0. {
                    return Err(mlua::Error::runtime(format!(
                        "`heading` is a level from 1 to 6, not {level}"
                    )));
                }
                let a11y = node.a11y.get_or_insert_default();
                a11y.role = Some(zgx::semantics::Role::Heading);
                a11y.level = Some(level as u8);
            }
            "selected" => {
                node.a11y.get_or_insert_default().selected =
                    Some(source(lua, Ty::Bool, "selected", value)?);
            }
            "checked" => {
                node.a11y.get_or_insert_default().checked =
                    Some(source(lua, Ty::Bool, "checked", value)?);
            }
            "expanded" => {
                node.a11y.get_or_insert_default().expanded =
                    Some(source(lua, Ty::Bool, "expanded", value)?);
            }
            "live" => {
                let mode: String = lua.unpack(value)?;
                node.a11y.get_or_insert_default().live = live_mode(&mode)?;
            }
            "error" => {
                node.a11y.get_or_insert_default().error = Some(source(lua, Ty::Str, "error", value)?);
            }
            "description" => {
                node.a11y.get_or_insert_default().description =
                    Some(lua.unpack::<String>(value)?.into());
            }
            "aria_hidden" => {
                node.a11y.get_or_insert_default().hidden = lua.unpack(value)?;
            }
            "focus_group" => {
                let axis: String = lua.unpack(value)?;
                node.a11y.get_or_insert_default().focus_group = Some(focus_group(&axis)?);
            }
            name if WIDGET_FIELDS.contains(&name) => {}
            name => {
                let Some(prop) = Prop::parse(name) else {
                    return late_field(lua, node, name, value);
                };
                if let Some(keys) = &keys {
                    // The key kept alive while its address names it.
                    let string = borrowed.get::<mlua::LuaString>()?;
                    keys.0.borrow_mut().insert(address, (string, prop));
                }
                node.props
                    .push((prop, source_linted(lua, prop.ty(), name, value, lint)?));
            }
        }
        Ok(())
    })?;
    // With a `layout_id`, `layout_duration` and `layout_ease` alone time its
    // moves; they do not ask for a size box.
    if node.layout_id.is_some()
        && !layout_given
        && let Some(layout) = &mut node.layout
    {
        layout.height = false;
        layout.position = true;
    }
    // What the element's variants set and it does not.
    if let Some(selection) = variants {
        crate::variants::apply(node, selection);
    }
    // Table order is arbitrary, so shorthands apply before what refines
    // them: `p = 0, px = 8` pads the sides, and `border = 1,
    // border_sides = …` keeps the per-side widths `border` would reset.
    node.props.sort_by_key(|(prop, _)| specificity(*prop));
    let mut kids = Vec::with_capacity(items.len());
    children(lua, &node.owner, items, &mut kids)?;
    if !kids.is_empty() && !node.kind.takes_children() {
        return Err(mlua::Error::runtime(format!(
            "`{}` does not take children",
            node.tag()
        )));
    }
    node.children = kids;
    Ok(())
}

/// What a `pointer_events` value says: whether the pointer stops at the
/// node (`"auto"`, true) or passes through (`"none"`, false).
pub(crate) fn pointer_events(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(enabled) => Some(*enabled),
        Value::Str(s) if &**s == "auto" => Some(true),
        Value::Str(s) if &**s == "none" => Some(false),
        _ => None,
    }
}

/// Shorthands sort first (0), then an axis (1), then a single side (2).
pub(crate) fn specificity(prop: Prop) -> u8 {
    match prop {
        Prop::Px | Prop::Py | Prop::Mx | Prop::My | Prop::GapX | Prop::GapY | Prop::Corners => 1,
        Prop::Pt
        | Prop::Pr
        | Prop::Pb
        | Prop::Pl
        | Prop::Ps
        | Prop::Pe
        | Prop::Mt
        | Prop::Mr
        | Prop::Mb
        | Prop::Ml
        | Prop::Ms
        | Prop::Me => 2,
        Prop::RoundedTl
        | Prop::RoundedTr
        | Prop::RoundedBr
        | Prop::RoundedBl
        | Prop::BorderSides => 2,
        _ => 0,
    }
}

/// A modal's `placement = { x = "end", y = "start", inset = 8 }`: where its
/// panel sits in the window (`inset`: a number, or `{ top, right, bottom,
/// left }`).
fn modal_placement_of(value: mlua::Value) -> mlua::Result<Option<zgx::compose::ModalPlacement>> {
    use zgx::compose::PlacementEdge;
    let table = match value {
        mlua::Value::Nil => return Ok(None),
        mlua::Value::Table(t) => t,
        other => {
            return Err(mlua::Error::runtime(format!(
                "a modal's `placement` is a table ({{ x, y, inset }}), not {}",
                other.type_name()
            )));
        }
    };
    let edge = |key: &str| -> mlua::Result<PlacementEdge> {
        Ok(match table.get::<Option<String>>(key)?.as_deref() {
            None | Some("center") => PlacementEdge::Center,
            Some("start") => PlacementEdge::Start,
            Some("end") => PlacementEdge::End,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "a modal's placement `{key}` is start, center or end, not {other:?}"
                )));
            }
        })
    };
    let insets = match table.get::<mlua::Value>("inset")? {
        mlua::Value::Integer(n) => [n as f32; 4],
        mlua::Value::Number(n) => [n as f32; 4],
        mlua::Value::Table(t) => {
            let at = |i: usize| t.get::<Option<f32>>(i).map(|v| v.unwrap_or(0.));
            [at(1)?, at(2)?, at(3)?, at(4)?]
        }
        _ => [0.; 4],
    };
    Ok(Some(zgx::compose::ModalPlacement {
        x: edge("x")?,
        y: edge("y")?,
        insets,
    }))
}

/// `placement = { side = "bottom", align = "end", offset = 4 }`: where an
/// anchored popover or menu opens.
fn placement_of(value: mlua::Value) -> mlua::Result<Option<zgx::compose::Placement>> {
    use zgx::compose::{Placement, PlacementEdge, PlacementSide};
    let table = match value {
        mlua::Value::Nil => return Ok(None),
        mlua::Value::Table(t) => t,
        other => {
            return Err(mlua::Error::runtime(format!(
                "`placement` is a table ({{ side, align, offset }}), not {}",
                other.type_name()
            )));
        }
    };
    let mut at = Placement::default();
    if let Some(side) = table.get::<Option<String>>("side")? {
        at.side = match side.as_str() {
            "bottom" => PlacementSide::Bottom,
            "top" => PlacementSide::Top,
            "left" => PlacementSide::Left,
            "right" => PlacementSide::Right,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "placement `side` is bottom, top, left or right, not {other:?}"
                )));
            }
        };
    }
    if let Some(align) = table.get::<Option<String>>("align")? {
        at.align = match align.as_str() {
            "start" => PlacementEdge::Start,
            "center" => PlacementEdge::Center,
            "end" => PlacementEdge::End,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "placement `align` is start, center or end, not {other:?}"
                )));
            }
        };
    }
    if let Some(offset) = table.get::<Option<f32>>("offset")?
        && offset.is_finite()
    {
        at.offset = offset;
    }
    Ok(Some(at))
}

pub(crate) fn owner(lua: &Lua) -> mlua::Result<Weak<Vm>> {
    Ok(env(lua)?.vm.clone())
}
/// `owner`, and the mode its design lint runs in (`design::mode`), from one
/// look at the script's environment: an element took one for each.
fn owner_and_lint(lua: &Lua) -> mlua::Result<(Weak<Vm>, crate::DesignLint)> {
    let env = env(lua)?;
    Ok((env.vm.clone(), env.warnings.borrow().design))
}
pub(crate) fn label(table: &Table) -> mlua::Result<String> {
    // A message (`ui.t`) or a state names it as it mounts; `fill` keeps it
    // as the node's followed label (`A11y::label`), so it follows the locale
    // or the state after. Read once: most labels are strings, or none.
    match table.get::<mlua::Value>("label")? {
        mlua::Value::String(s) => Ok(s.to_str()?.to_owned()),
        mlua::Value::Nil => Ok(String::new()),
        value @ mlua::Value::UserData(_) if crate::i18n::message_of(&value).is_some() => {
            Ok(crate::i18n::message_of(&value)
                .map(|m| m.get())
                .unwrap_or_default())
        }
        value @ mlua::Value::UserData(_) if state_of(&value).is_some() => Ok(state_of(&value)
            .map(|state| state.with_untracked(|v| v.to_string()))
            .unwrap_or_default()),
        // A function: what it says once its effect runs.
        mlua::Value::Function(_) => Ok(String::new()),
        // A number, say: converted as a string label always was.
        _ => Ok(table.get::<Option<String>>("label")?.unwrap_or_default()),
    }
}

/// A span's options that follow a state or function, and their types.
const SPAN_FOLLOWS: &[(&str, Ty)] = &[
    ("color", Ty::Color),
    ("bg", Ty::Color),
    ("text_size", Ty::Num),
    ("weight", Ty::Num),
    ("bold", Ty::Bool),
    ("italic", Ty::Bool),
    ("underline", Ty::Bool),
    ("strike", Ty::Bool),
    ("font", Ty::Str),
];

/// `ui.span { "text", bold = true, color = "#f80", on_click = fn }`;
/// `href = "https://…"` makes it a link that opens it.
fn span(lua: &Lua, arg: mlua::Value, literal: bool) -> mlua::Result<Span> {
    let (content, table) = match arg {
        mlua::Value::Table(t) => (t.get::<mlua::Value>(1)?, Some(t)),
        other => (other, None),
    };
    let literal = match &table {
        Some(t) => t.get::<Option<bool>>("literal")?.unwrap_or(literal),
        None => literal,
    };
    let mut span = Span {
        text: text_of(lua, content, "ui.span", literal)?,
        color: None,
        background: None,
        size: None,
        weight: None,
        italic: false,
        underline: false,
        strike: false,
        font: None,
        followed: Vec::new(),
        padding: [0.; 2],
        radius: 0.,
        click: None,
        href: None,
        underline_offset: None,
        underline_style: None,
        underline_color: None,
        underline_thickness: None,
        hover: None,
        icon: None,
    };
    if let Some(t) = table {
        // One walk over what it holds: most spans set one or two options of
        // the twenty it could.
        let mut followed = Vec::new();
        let mut bold = false;
        t.for_each_borrowed(|key, value| {
            let Some(key) = key.as_bytes() else {
                return Ok(());
            };
            let given = value.to_value()?;
            if matches!(given, mlua::Value::Function(_) | mlua::Value::UserData(_))
                && let Some((name, ty)) =
                    SPAN_FOLLOWS.iter().find(|(name, _)| name.as_bytes() == key)
            {
                followed.push((*name, *ty, source(lua, *ty, name, given)?));
                return Ok(());
            }
            let number = |given: mlua::Value| {
                lua.unpack::<Value>(given)
                    .map(|value| value.as_f64().map(|v| v as f32))
            };
            match key {
                b"color" => span.color = themed_color(&lua.unpack(given)?),
                b"bg" => span.background = themed_color(&lua.unpack(given)?),
                b"text_size" => span.size = number(given)?,
                b"weight" => span.weight = number(given)?.map(|v| v as u16),
                b"bold" => bold = lua.unpack::<Value>(given)?.truthy(),
                b"italic" => span.italic = lua.unpack::<Value>(given)?.truthy(),
                b"underline" => span.underline = lua.unpack::<Value>(given)?.truthy(),
                b"strike" => span.strike = lua.unpack::<Value>(given)?.truthy(),
                b"font" => {
                    if let Value::Str(font) = lua.unpack(given)? {
                        span.font = Some(font);
                    }
                }
                // `padding = 4` (across) or `{ 4, 0.5 }` (across, down).
                b"padding" => {
                    span.padding = match lua.unpack(given)? {
                        Value::Num(x) => [x as f32, 0.],
                        Value::List(l) => [
                            l.first().and_then(Value::as_f64).unwrap_or(0.) as f32,
                            l.get(1).and_then(Value::as_f64).unwrap_or(0.) as f32,
                        ],
                        _ => [0.; 2],
                    }
                }
                b"radius" => span.radius = number(given)?.unwrap_or(0.),
                b"underline_offset" => span.underline_offset = number(given)?,
                b"underline_style" => {
                    span.underline_style = match lua.unpack(given)? {
                        Value::Str(style) => Some(line_style(&style)?),
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "`underline_style` is a name, not {other}"
                            )));
                        }
                    }
                }
                b"underline_color" => span.underline_color = themed_color(&lua.unpack(given)?),
                b"underline_thickness" => span.underline_thickness = number(given)?,
                // `hover = { underline = true, color = … }`: a link's look
                // under the pointer (CSS's `a:hover`).
                b"hover" => {
                    let hover: Table = lua.unpack(given)?;
                    span.hover = Some((
                        hover.get::<Value>("underline")?.truthy(),
                        themed_color(&hover.get::<Value>("color")?),
                    ));
                }
                b"icon" => {
                    let icon: Table = lua.unpack(given)?;
                    let src = match (
                        icon.get::<Option<String>>("src")?,
                        icon.get::<Option<String>>("code")?,
                    ) {
                        (Some(src), _) => src,
                        (None, Some(code)) => format!("code:{code}"),
                        (None, None) => {
                            return Err(mlua::Error::runtime(
                                "a span's icon takes `src` or `code`",
                            ));
                        }
                    };
                    let tint = icon.get::<Value>("tint")?;
                    span.icon = Some(SpanIcon {
                        src,
                        size: icon.get::<Option<f32>>("size")?.unwrap_or(12.),
                        gap: icon.get::<Option<f32>>("gap")?.unwrap_or(4.),
                        tint: (!matches!(tint, Value::Nil)).then_some(tint),
                    });
                }
                b"on_click" => span.click = Some(handler(given)?),
                b"href" => span.href = Some(lua.unpack::<String>(given)?.into()),
                _ => {}
            }
            Ok(())
        })?;
        span.weight = span.weight.or(bold.then_some(700));
        // As `SPAN_FOLLOWS` lists them, whatever order the table walks in:
        // `bold` is applied after `weight`.
        if followed.len() > 1 {
            followed.sort_by_key(|(name, ..)| {
                SPAN_FOLLOWS.iter().position(|(listed, _)| listed == name)
            });
        }
        span.followed = followed;
        // One of them says it is underlined, as `underline = true` does.
        span.underline |= span.underline_style.is_some()
            || span.underline_color.is_some()
            || span.underline_thickness.is_some();
    }
    Ok(span)
}

/// Installs the read-only `ui` global.
/// `ui.override`, `ui.contribute` and `ui.filter`: what plugins (and an
/// app's own extensions) hand the app, made once a script names one.
pub(crate) fn install_plugins(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set(
        "override",
        lua.create_function(|lua, table: Table| {
            let env = env(lua)?;
            let replaced = module_override(&env, &table)?;
            env.overrides.borrow_mut().push(replaced);
            Ok(())
        })?,
    )?;
    ui.set(
        "contribute",
        lua.create_function(|lua, (slot, node): (mlua::Value, Option<mlua::Value>)| {
            let env = env(lua)?;
            let owner = env.owner.clone();
            match (slot, node) {
                (mlua::Value::String(slot), Some(node)) => {
                    let node = expect_node(node, "a contribution")?;
                    let slot = slot.to_str()?.to_string();
                    env.contributions.borrow_mut().push((slot, owner, node));
                }
                (mlua::Value::Table(table), None) => {
                    let hook = hook_contribution(&env, &table, owner)?;
                    env.hooks.borrow_mut().push(hook);
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "ui.contribute takes a slot's name and a node, or \
                         { target = \"#id\", before = node | after = node | replace = node }",
                    ));
                }
            }
            Ok(())
        })?,
    )?;
    ui.set(
        "filter",
        lua.create_function(|lua, table: Table| {
            let env = env(lua)?;
            let hook = hook_filter(&env, &table, env.owner.clone())?;
            env.hooks.borrow_mut().push(hook);
            Ok(())
        })?,
    )?;
    Ok(())
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(StyleKeys::default());
    let ui = lua.create_table()?;
    let container = |kind: fn() -> Kind| {
        lua.create_function(move |lua, table: Option<Table>| {
            // The script's owner and lint mode, looked up once a call.
            let (owner, lint) = owner_and_lint(lua)?;
            let mut node = Node::new(owner, kind());
            if let Some(table) = &table {
                fill_from(lua, &mut node, table, None, lint)?;
            }
            node_ud(lua, node)
        })
    };
    ui.set("column", container(|| Kind::Column)?)?;
    ui.set("row", container(|| Kind::Row)?)?;
    ui.set("null", crate::value::null())?;
    ui.set("object", lua.create_function(crate::value::object)?)?;
    ui.set("div", container(|| Kind::Div)?)?;
    ui.set("overlay", container(|| Kind::Overlay)?)?;
    ui.set(
        "button",
        lua.create_function(|lua, table: Option<Table>| {
            let (owner, lint) = owner_and_lint(lua)?;
            let mut node = Node::new(owner, Kind::Button);
            if let Some(table) = &table {
                button_look(lua, table)?;
                fill_from(lua, &mut node, table, None, lint)?;
            }
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "grid",
        lua.create_function(|lua, table: Option<Table>| {
            let mut node = fill(lua, Node::new(owner(lua)?, Kind::Div), table.as_ref())?;
            node.props
                .insert(0, (Prop::Grid, Source::Lit(Lit::Bool(true))));
            node_ud(lua, node)
        })?,
    )?;
    for (name, horizontal) in [("scroll", false), ("scroll_x", true)] {
        ui.set(
            name,
            lua.create_function(move |lua, table: Option<Table>| {
                let offset = match &table {
                    Some(t) => t.get::<mlua::Value>("offset")?,
                    None => mlua::Value::Nil,
                };
                let offset = (!offset.is_nil())
                    .then(|| expect_state(&offset, "`offset`"))
                    .transpose()?;
                let follow = match &table {
                    Some(t) => t.get::<Option<bool>>("follow")?.unwrap_or(false),
                    None => false,
                };
                let scrollbar = match &table {
                    Some(t) => t.get::<Option<bool>>("scrollbar")?.unwrap_or(false),
                    None => false,
                };
                let node = fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Scroll {
                            offset,
                            horizontal,
                            follow,
                            scrollbar,
                            // Set as `fill` walks the table (`late_field`).
                            contain: false,
                        },
                    ),
                    table.as_ref(),
                )?;
                node_ud(lua, node)
            })?,
        )?;
    }

    ui.set(
        "text",
        lua.create_function(|lua, arg: mlua::Value| {
            let (owner, lint) = owner_and_lint(lua)?;
            let mlua::Value::Table(table) = arg else {
                let text = text_of(lua, arg, "ui.text", false)?;
                return node_ud(lua, Node::new(owner, Kind::Text(text)));
            };
            // The content is element 1; the rest are styles.
            let mut args = TextArgs::default();
            let mut node = Node::new(owner, Kind::Text(Text::Literal(String::new())));
            fill_from(lua, &mut node, &table, Some(&mut args), lint)?;
            let literal = args.literal.unwrap_or(false);
            node.kind = Kind::Text(text_of(lua, args.content, "ui.text", literal)?);
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "markdown_view",
        lua.create_function(|lua, table: Table| {
            // Its own options, read before the styles are.
            let flag = |name: &str, default: bool| -> mlua::Result<bool> {
                let value = table.get::<Option<bool>>(name)?.unwrap_or(default);
                table.raw_set(name, mlua::Value::Nil)?;
                Ok(value)
            };
            let options = crate::markdown::view::ViewOptions {
                parse: crate::markdown::Options {
                    breaks: flag("breaks", false)?,
                    paths: flag("paths", false)?,
                    code: None,
                },
                copy: flag("copy", true)?,
            };
            let mut args = TextArgs::default();
            let empty = Kind::Text(Text::Literal(String::new()));
            let (owner, lint) = owner_and_lint(lua)?;
            let mut node = Node::new(owner, empty);
            fill_from(lua, &mut node, &table, Some(&mut args), lint)?;
            let literal = args.literal.unwrap_or(false);
            let text = text_of(lua, args.content, "ui.markdown_view", literal)?;
            node.kind = Kind::Markdown { text, options };
            // Prose: selectable as a web page's is, unless asked not to be.
            if node.selectable.is_none() {
                node.selectable = Some(true);
            }
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "span",
        lua.create_function(|lua, arg: mlua::Value| {
            Ok(SpanUd(RefCell::new(Some(span(lua, arg, false)?))))
        })?,
    )?;
    ui.set(
        "rich",
        lua.create_function(|lua, table: Table| {
            // Runs written as strings or tables take the paragraph's
            // `literal` (a span may say otherwise).
            let literal = table.get::<Option<bool>>("literal")?.unwrap_or(false);
            table.raw_set("literal", mlua::Value::Nil)?;
            let mut spans = Vec::new();
            for item in table.sequence_values::<mlua::Value>() {
                match item? {
                    mlua::Value::UserData(ud) if ud.is::<SpanUd>() => spans.push(
                        ud.borrow::<SpanUd>()?
                            .0
                            .borrow_mut()
                            .take()
                            .ok_or_else(|| mlua::Error::runtime("a span can be used once"))?,
                    ),
                    other => spans.push(span(lua, other, literal)?),
                }
            }
            for i in (1..=table.raw_len()).rev() {
                table.raw_remove(i)?;
            }
            node_ud(
                lua,
                fill(lua, Node::new(owner(lua)?, Kind::Rich(spans)), Some(&table))?,
            )
        })?,
    )?;
    for name in ["image", "svg"] {
        ui.set(
            name,
            lua.create_function(move |lua, table: Table| {
                let src = match table.get::<mlua::Value>("src")? {
                    mlua::Value::Nil => match table.get::<mlua::Value>("code")? {
                        mlua::Value::Nil => {
                            return Err(mlua::Error::runtime(format!("ui.{name} needs `src`")));
                        }
                        code => {
                            let code: String = lua.unpack(code)?;
                            Source::Lit(Lit::Str(format!("code:{code}").into()))
                        }
                    },
                    // A bitmap (`ui.bitmap`, a pasted image) shown as it is.
                    mlua::Value::UserData(ud) if name == "image" && ud.is::<BitmapUd>() => {
                        let image = ud.borrow::<BitmapUd>()?.0.clone();
                        Source::State(env(lua)?.runtime.signal(Value::Image(image)))
                    }
                    src => source(lua, Ty::Str, "src", src)?,
                };
                let label = label(&table)?;
                let kind = if name == "image" {
                    let fallback = match table.get::<mlua::Value>("fallback")? {
                        mlua::Value::Nil => None,
                        node => Some(expect_node(node, "`fallback`")?),
                    };
                    Kind::Image {
                        label,
                        src,
                        fallback,
                    }
                } else {
                    Kind::Svg { label, src }
                };
                node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
            })?,
        )?;
    }
    ui.set(
        "canvas",
        lua.create_function(|lua, table: Table| {
            let draw: Option<Function> = table.get("draw")?;
            let each = match (
                table.get::<mlua::Value>("each")?,
                table.get::<Option<Function>>("draw_item")?,
            ) {
                (mlua::Value::Nil, None) => None,
                (items, Some(draw_item)) => Some((
                    state_of(&items).ok_or_else(|| {
                        mlua::Error::runtime("ui.canvas `each` must be a ui.state holding a list")
                    })?,
                    draw_item,
                )),
                (_, None) => {
                    return Err(mlua::Error::runtime("ui.canvas `each` needs `draw_item`"));
                }
            };
            if draw.is_none() && each.is_none() {
                return Err(mlua::Error::runtime(
                    "ui.canvas needs `draw` (or `each` and `draw_item`)",
                ));
            }
            let view = match table.get::<mlua::Value>("view")? {
                mlua::Value::Nil => None,
                mlua::Value::Function(f) => Some(Readable::Function(f)),
                v => Some(Readable::State(state_of(&v).ok_or_else(|| {
                    mlua::Error::runtime(
                        "ui.canvas `view` must be a ui.state or a function of { x, y, zoom }",
                    )
                })?)),
            };
            let grid = match table.get::<Option<Table>>("bg_grid")? {
                None => None,
                Some(grid) => Some(grid_spec(lua, &grid)?),
            };
            let options = (view.is_some() || grid.is_some() || each.is_some())
                .then(|| Box::new(CanvasOptions { view, grid, each }));
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(owner(lua)?, Kind::Canvas(draw, options)),
                    Some(&table),
                )?,
            )
        })?,
    )?;

    let widget = |build: fn(String, Signal<Value>, &Table) -> mlua::Result<Kind>| {
        lua.create_function(move |lua, table: Table| {
            crate::form::apply_field(lua, &table, false)?;
            let value = control_value(lua, &table)?;
            let kind = build(label(&table)?, value, &table)?;
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })
    };
    ui.set(
        "checkbox",
        widget(|label, value, table| {
            let mixed = match table.get::<mlua::Value>("mixed")? {
                mlua::Value::Nil => None,
                state => Some(expect_state(&state, "`mixed`")?),
            };
            Ok(Kind::Checkbox {
                label,
                value,
                mixed,
            })
        })?,
    )?;
    for (name, always_multiline) in [("input", false), ("textarea", true)] {
        ui.set(
            name,
            lua.create_function(move |lua, table: Table| {
                // A one-line input's Enter submits its field's form.
                let one_line =
                    !always_multiline && !table.get::<Option<bool>>("multiline")?.unwrap_or(false);
                crate::form::apply_field(lua, &table, one_line)?;
                let label = label(&table)?;
                let value = match table.get::<mlua::Value>("value")? {
                    v @ (mlua::Value::Function(_)
                    | mlua::Value::String(_)
                    | mlua::Value::Number(_)
                    | mlua::Value::Integer(_)) => {
                        let handler_of = |key: &str| -> mlua::Result<Option<Handler>> {
                            match table.get::<mlua::Value>(key)? {
                                mlua::Value::Nil => Ok(None),
                                h => handler(h).map(Some),
                            }
                        };
                        InputValue::Shown {
                            shown: source(lua, Ty::Str, "`value`", v)?,
                            change: handler_of("on_change")?,
                            commit: handler_of("on_commit")?,
                        }
                    }
                    v => InputValue::State {
                        value: writable_state_of(&v).ok_or_else(|| match read_only_key(&v) {
                            Some(key) => mlua::Error::runtime(format!(
                                "`value` is written as it is typed, and shared state `{key}` is \
                                 read only to plugins: give a function with on_change instead"
                            )),
                            None => mlua::Error::runtime(
                                "`value` must be a ui.state (two-way), or a function with \
                                 on_change or on_commit (the input shows it, and says what was \
                                 typed)",
                            ),
                        })?,
                        commit: match table.get::<mlua::Value>("on_commit")? {
                            mlua::Value::Nil => None,
                            h => Some(handler(h)?),
                        },
                    },
                };
                let optional = |key: &str, ty: Ty| -> mlua::Result<Option<Source>> {
                    match table.get::<mlua::Value>(key)? {
                        mlua::Value::Nil => Ok(None),
                        value => source(lua, ty, key, value).map(Some),
                    }
                };
                let kind = Kind::Input {
                    label,
                    value,
                    multiline: always_multiline
                        || table.get::<Option<bool>>("multiline")?.unwrap_or(false),
                    placeholder: optional("placeholder", Ty::Str)?,
                    rare: {
                        // `caret_color` and `caret_width` are taken as
                        // `fill` walks the table (`late_field`).
                        let rare = InputRare {
                            placeholder_color: optional("placeholder_color", Ty::Color)?,
                            spellcheck: optional("spellcheck", Ty::Bool)?,
                            ..InputRare::default()
                        };
                        (rare.placeholder_color.is_some() || rare.spellcheck.is_some())
                            .then(|| Box::new(rare))
                    },
                    select_on_focus: table
                        .get::<Option<bool>>("select_on_focus")?
                        .unwrap_or(false),
                    secure: table.get::<Option<bool>>("secure")?.unwrap_or(false),
                    input_mode: match table.get::<Option<String>>("inputmode")?.as_deref() {
                        None | Some("text") => zgx::text_edit::InputMode::Text,
                        Some("decimal") => zgx::text_edit::InputMode::Decimal,
                        Some("numeric") => zgx::text_edit::InputMode::Numeric,
                        Some("tel") => zgx::text_edit::InputMode::Tel,
                        Some("search") => zgx::text_edit::InputMode::Search,
                        Some("email") => zgx::text_edit::InputMode::Email,
                        Some("url") => zgx::text_edit::InputMode::Url,
                        Some("none") => zgx::text_edit::InputMode::None,
                        Some(other) => {
                            return Err(mlua::Error::runtime(format!(
                                "unknown inputmode `{other}`: one of text, decimal, numeric, \
                                 tel, search, email, url, none"
                            )));
                        }
                    },
                    line_numbers: table.get::<Option<bool>>("line_numbers")?.unwrap_or(false),
                    code: {
                        let highlight = table.raw_get::<Option<Function>>("highlight")?;
                        let decorations = match table.raw_get::<mlua::Value>("decorations")? {
                            mlua::Value::Nil => None,
                            mlua::Value::Function(f) => Some(Readable::Function(f)),
                            v => Some(Readable::State(state_of(&v).ok_or_else(|| {
                                mlua::Error::runtime(
                                    "`decorations` must be a ui.state or a function of ranges",
                                )
                            })?)),
                        };
                        let underlines = match table.raw_get::<mlua::Value>("underlines")? {
                            mlua::Value::Nil => None,
                            mlua::Value::Function(f) => Some(Readable::Function(f)),
                            v => Some(Readable::State(state_of(&v).ok_or_else(|| {
                                mlua::Error::runtime(
                                    "`underlines` must be a ui.state or a function of ranges",
                                )
                            })?)),
                        };
                        let current_line = optional("current_line", Ty::Color)?;
                        let tab = table.raw_get::<Option<String>>("tab")?;
                        let on_caret = table.raw_get::<Option<Function>>("on_caret")?;
                        let on_top_line = table.raw_get::<Option<Function>>("on_top_line")?;
                        let find = match table.raw_get::<Option<Table>>("find")? {
                            None => None,
                            Some(find) => {
                                let get = |key: &str, ty: Ty| -> mlua::Result<Option<Source>> {
                                    match find.get::<mlua::Value>(key)? {
                                        mlua::Value::Nil => Ok(None),
                                        value => source(lua, ty, key, value).map(Some),
                                    }
                                };
                                Some(FindProps {
                                    query: get("query", Ty::Str)?.ok_or_else(|| {
                                        mlua::Error::runtime("a textarea's `find` needs a `query`")
                                    })?,
                                    case: get("case", Ty::Bool)?,
                                    current: get("current", Ty::Num)?,
                                    limit: find
                                        .get::<Option<f64>>("limit")?
                                        .map(|n| n.max(0.) as usize),
                                    color: get("color", Ty::Color)?,
                                    current_color: get("current_color", Ty::Color)?,
                                })
                            }
                        };
                        (highlight.is_some()
                            || decorations.is_some()
                            || underlines.is_some()
                            || current_line.is_some()
                            || tab.is_some()
                            || on_caret.is_some()
                            || on_top_line.is_some()
                            || find.is_some())
                        .then(|| {
                            Rc::new(CodeProps {
                                highlight,
                                decorations,
                                underlines,
                                current_line,
                                tab,
                                on_caret,
                                on_top_line,
                                find,
                            })
                        })
                    },
                };
                node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
            })?,
        )?;
    }
    ui.set(
        "progress",
        lua.create_function(|lua, table: Table| {
            crate::form::apply_field(lua, &table, false)?;
            let value = match table.get::<mlua::Value>("value")? {
                // A bar that won't change, as a list row's: a state nothing
                // writes.
                mlua::Value::Integer(n) => env(lua)?.runtime.signal(Value::Num(n as f64)),
                mlua::Value::Number(n) => env(lua)?.runtime.signal(Value::Num(n)),
                _ => control_value(lua, &table)?,
            };
            let kind = Kind::Progress {
                label: label(&table)?,
                value,
            };
            node_ud(lua, fill(lua, Node::new(owner(lua)?, kind), Some(&table))?)
        })?,
    )?;
    ui.set(
        "slider",
        widget(|label, value, t| {
            let min = t.get::<Option<f32>>("min")?.unwrap_or(0.);
            let max = t.get::<Option<f32>>("max")?.unwrap_or(1.);
            if !(min.is_finite() && max.is_finite() && min < max) {
                return Err(mlua::Error::runtime("slider needs finite min < max"));
            }
            let step: Option<f32> = t.get("step")?;
            if step.is_some_and(|s| !(s.is_finite() && s > 0.)) {
                return Err(mlua::Error::runtime("a slider's step is positive"));
            }
            Ok(Kind::Slider {
                label,
                value,
                min,
                max,
                step,
            })
        })?,
    )?;

    ui.set(
        "split",
        lua.create_function(|lua, table: Table| {
            // Without one, a state of its own: the divider still drags.
            let size = match table.get::<mlua::Value>("size")? {
                mlua::Value::Nil => env(lua)?.runtime.signal(Value::Num(280.)),
                size => expect_state(&size, "ui.split's `size`")?,
            };
            let number = |key: &str, default: f32| -> mlua::Result<f32> {
                Ok(table.get::<Option<f32>>(key)?.unwrap_or(default))
            };
            let options = zgx::compose::SplitOptions {
                vertical: table.get::<Option<bool>>("vertical")?.unwrap_or(false),
                second: table.get::<Option<bool>>("second")?.unwrap_or(false),
                min: number("min", 0.)?,
                max: number("max", f32::INFINITY)?,
                min_rest: number("min_rest", 0.)?,
                label: table.get("label")?,
            };
            if !(options.min.is_finite() && options.min >= 0. && options.min <= options.max) {
                return Err(mlua::Error::runtime(
                    "ui.split needs 0 <= min <= max (finite min)",
                ));
            }
            let node = fill(
                lua,
                Node::new(owner(lua)?, Kind::Split { size, options }),
                Some(&table),
            )?;
            if node.children.len() != 2 {
                return Err(mlua::Error::runtime(format!(
                    "ui.split takes two panes, not {}",
                    node.children.len()
                )));
            }
            node_ud(lua, node)
        })?,
    )?;
    #[cfg(feature = "components")]
    crate::library::register(lua, &ui)?;
    ui.set(
        "show",
        lua.create_function(
            |lua, (when, then, otherwise): (mlua::Value, mlua::Value, mlua::Value)| {
                let when = source(lua, Ty::Bool, "show condition", when)?;
                let then = expect_node(then, "ui.show's content")?;
                let otherwise = (!otherwise.is_nil())
                    .then(|| expect_node(otherwise, "ui.show's alternative"))
                    .transpose()?;
                node_ud(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Show {
                            when,
                            then,
                            otherwise,
                        },
                    ),
                )
            },
        )?,
    )?;
    ui.set(
        "each",
        lua.create_function(
            |lua, (list, build, table): (mlua::Value, Function, Option<Table>)| {
                let items = match (state_of(&list), &list) {
                    (Some(items), _) => items,
                    // A list that never changes: a state nothing writes.
                    (None, mlua::Value::Table(_)) => env(lua)?
                        .runtime
                        .signal(Value::from_lua(list.clone(), lua)?),
                    _ => expect_readable(&list, "ui.each's list")?,
                };
                // Rows dragged or moved by keyboard into a new order.
                let build = match &table {
                    Some(t) if !t.raw_get::<mlua::Value>("reorder")?.is_nil() => {
                        crate::reorder::wrap(lua, list, build, t)?
                    }
                    _ => build,
                };
                // Every row is built and laid out: thousands are slow to
                // mount and lay out, where `ui.list` builds what shows.
                let rows = items.with_untracked(|list| list.list().len());
                if rows > LONG_EACH {
                    crate::warn(
                        lua,
                        &format!(
                            "`ui.each` over {rows} items builds every row; for a list this long, \
                             `ui.list` builds only the rows in view"
                        ),
                    );
                }
                let (key, motion) = match &table {
                    Some(t) => (
                        t.get::<Option<Function>>("key")?,
                        crate::motion::each_motion(t)?,
                    ),
                    None => (None, None),
                };
                let kind = Kind::Each {
                    items,
                    build,
                    key,
                    motion,
                };
                node_ud(
                    lua,
                    fill(lua, Node::new(owner(lua)?, kind), table.as_ref())?,
                )
            },
        )?,
    )?;
    ui.set(
        "list",
        lua.create_function(|lua, table: Table| {
            let heights = match table.get::<mlua::Value>("heights")? {
                mlua::Value::Nil => None,
                mlua::Value::Function(f) => Some(Heights::Computed(f)),
                t @ mlua::Value::Table(_) => Some(Heights::Lit(Value::from_lua(t, lua)?)),
                s => Some(Heights::State(expect_readable(
                    &s,
                    "`heights` (a list, a ui.state or a function)",
                )?)),
            };
            let sticky = match table.get::<mlua::Value>("sticky")? {
                mlua::Value::Nil => None,
                mlua::Value::Function(f) => Some(Heights::Computed(f)),
                t @ mlua::Value::Table(_) => Some(Heights::Lit(Value::from_lua(t, lua)?)),
                s => Some(Heights::State(expect_readable(
                    &s,
                    "`sticky` (a list, a ui.state or a function)",
                )?)),
            };
            if sticky.is_some() && heights.is_none() {
                return Err(mlua::Error::runtime("ui.list `sticky` needs `heights`"));
            }
            let measured = table.get::<Option<bool>>("measured")?.unwrap_or(false);
            if measured && heights.is_some() {
                return Err(mlua::Error::runtime(
                    "ui.list `measured` and `heights` don't go together: measured rows take their content's height",
                ));
            }
            let items = match table.get::<mlua::Value>("data")? {
                mlua::Value::Nil => None,
                data => Some(expect_readable(&data, "ui.list's `data`")?),
            };
            if items.is_some() && table.contains_key("count")? {
                return Err(mlua::Error::runtime(
                    "ui.list `data` gives the rows: not with `count`",
                ));
            }
            let count = match (&heights, &items, table.get::<mlua::Value>("count")?) {
                (Some(_), _, mlua::Value::Nil) | (_, Some(_), _) => Source::Lit(Lit::Num(0.)),
                (_, _, count) => source(lua, Ty::Num, "count", count)?,
            };
            let build: Function = table
                .get("row")
                .map_err(|_| mlua::Error::runtime("ui.list needs `row`"))?;
            let height = table.get::<Option<f32>>("height")?.unwrap_or(28.);
            if !(height.is_finite() && height > 0.) {
                return Err(mlua::Error::runtime("ui.list `height` must be positive"));
            }
            let offset = match table.get::<mlua::Value>("offset")? {
                mlua::Value::Nil => None,
                o => Some(expect_state(&o, "`offset`")?),
            };
            let overscan = table.get::<Option<usize>>("overscan")?.unwrap_or(2);
            let row_text = table.get::<Option<Function>>("row_text")?;
            let end = match table.get::<Option<Function>>("on_end")? {
                Some(on_end) => Some(ListEnd {
                    on_end,
                    distance: table
                        .get::<Option<f32>>("end_distance")?
                        .filter(|d| d.is_finite())
                        .unwrap_or(200.),
                }),
                None => None,
            };
            let inset = table.get::<Option<f32>>("sticky_top")?;
            let sticky_top = inset.filter(|t| t.is_finite()).unwrap_or(0.).max(0.);
            let rare = (sticky.is_some() || row_text.is_some() || end.is_some()).then(|| {
                Box::new(ListRare {
                    sticky,
                    sticky_top,
                    row_text,
                    end,
                    ..ListRare::default()
                })
            });
            let mut node = fill(
                lua,
                Node::new(
                    owner(lua)?,
                    Kind::List {
                        count,
                        items,
                        height,
                        heights,
                        rare,
                        overscan,
                        build,
                        key: table.get::<Option<Function>>("key")?,
                        measured,
                        follow: table.get::<Option<bool>>("follow")?.unwrap_or(false),
                        offset,
                        scrollbar: table.get::<Option<bool>>("scrollbar")?.unwrap_or(false),
                        keyboard: table.get::<Option<bool>>("keyboard")?.unwrap_or(false),

                    },
                ),
                Some(&table),
            )?;
            // On a list, the inset of its sticky rows: not a place of its own.
            if inset.is_some() {
                node.props.retain(|(prop, _)| *prop != Prop::StickyTop);
            }
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "presence",
        lua.create_function(
            |lua, (value, build, table): (mlua::Value, Function, Option<Table>)| {
                let value = source(lua, Ty::Str, "presence value", value)?;
                let (enter, exit, wait, initial) = match &table {
                    Some(t) => (
                        t.get::<Option<Table>>("enter")?,
                        t.get::<Option<Table>>("exit")?,
                        t.get::<Option<String>>("mode")?.as_deref() == Some("wait"),
                        t.get::<Option<bool>>("initial")?.unwrap_or(false),
                    ),
                    None => (None, None, false, false),
                };
                let kind = Kind::Presence {
                    value,
                    build,
                    enter,
                    exit,
                    wait,
                    initial,
                };
                node_ud(
                    lua,
                    fill(lua, Node::new(owner(lua)?, kind), table.as_ref())?,
                )
            },
        )?,
    )?;
    ui.set(
        "switch",
        lua.create_function(
            |lua, (value, cases, default): (mlua::Value, Table, mlua::Value)| {
                let value = source(lua, Ty::Str, "switch value", value)?;
                let mut list = Vec::new();
                for pair in cases.pairs::<Value, mlua::Value>() {
                    let (key, node) = pair?;
                    list.push((key, expect_node(node, "a switch case")?));
                }
                let default = (!default.is_nil())
                    .then(|| expect_node(default, "a switch default"))
                    .transpose()?;
                node_ud(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Switch {
                            value,
                            cases: list,
                            default,
                        },
                    ),
                )
            },
        )?,
    )?;
    ui.set(
        "modal",
        lua.create_function(|lua, table: Table| {
            let open = open_of(lua, &table)?;
            let mut node = fill(
                lua,
                Node::new(
                    owner(lua)?,
                    Kind::Modal {
                        label: label(&table)?,
                        open: open.clone(),
                        placement: modal_placement_of(table.get("placement")?)?,
                        autofocus: table.get::<Option<bool>>("autofocus")?.unwrap_or(true),
                        layer: false,
                        dismiss: table.get("dismiss_on_backdrop")?,
                    },
                ),
                Some(&table),
            )?;
            if let Some(build) = table.get::<Option<Function>>("build")? {
                let open = mlua::Value::UserData(lua.create_userdata(State::new(open))?);
                node.children.push(built_on_open(lua, open, build)?);
            }
            node_ud(lua, node)
        })?,
    )?;
    // `ui.layer { open?, placement?, … }`: over the window, non-modal (toasts).
    ui.set(
        "layer",
        lua.create_function(|lua, table: Table| {
            let open = match table.get::<mlua::Value>("open")? {
                mlua::Value::Nil => env(lua)?.runtime.signal(Value::Bool(true)),
                open => expect_state(&open, "`open`")?,
            };
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Modal {
                            label: label(&table)?,
                            open,
                            placement: modal_placement_of(table.get("placement")?)?,
                            autofocus: false,
                            layer: true,
                            dismiss: None,
                        },
                    ),
                    Some(&table),
                )?,
            )
        })?,
    )?;
    for name in ["popover", "menu"] {
        ui.set(
            name,
            lua.create_function(move |lua, table: Table| {
                let open = open_of(lua, &table)?;
                let anchor = expect_node(table.get("anchor")?, "`anchor`")?;
                let label = label(&table)?;
                let placement = placement_of(table.get("placement")?)?;
                let autofocus = table.get::<Option<bool>>("autofocus")?.unwrap_or(true);
                let match_anchor = table
                    .get::<Option<bool>>("match_anchor_width")?
                    .unwrap_or(false);
                let kind = if name == "menu" {
                    Kind::Menu {
                        label,
                        open,
                        anchor,
                        placement,
                        autofocus,
                        over: table.get("open_over")?,
                        match_anchor,
                    }
                } else {
                    Kind::Popover {
                        label,
                        open,
                        anchor,
                        placement,
                        autofocus,
                        arrow: table.get("arrow")?,
                        dismiss: table.get("dismiss_on_backdrop")?,
                        match_anchor,
                        // Set as `fill` walks the table (`late_field`).
                        at_caret: false,
                    }
                };
                let mut node = fill(lua, Node::new(owner(lua)?, kind), Some(&table))?;
                if let Some(build) = table.get::<Option<Function>>("build")? {
                    node.children
                        .push(built_on_open(lua, table.get("open")?, build)?);
                }
                node_ud(lua, node)
            })?,
        )?;
    }
    ui.set(
        "menu_item",
        lua.create_function(|lua, table: Table| {
            // `ui.menu_item { "Copy" }`, or `ui.menu_item { label = "Copy",
            // icon, ui.text "Copy", shortcut }` with content of its own.
            let label = match table.get::<mlua::Value>(1)? {
                mlua::Value::String(s) => {
                    table.raw_remove(1)?;
                    s.to_string_lossy()
                }
                _ => label(&table)?,
            };
            // `keep_open = true`: chosen, it leaves the menu open (a toggle).
            let keep_open = table.get::<Option<bool>>("keep_open")?.unwrap_or(false);
            table.raw_remove("keep_open")?;
            // `checked` (a state or function, or a boolean), `radio`, and
            // the keys it shows: `shortcut = "⌘C"`.
            let checked = match table.get::<mlua::Value>("checked")? {
                mlua::Value::Nil => None,
                value => Some(source(lua, Ty::Bool, "checked", value)?),
            };
            table.raw_remove("checked")?;
            let radio = table.get::<Option<bool>>("radio")?.unwrap_or(false);
            table.raw_remove("radio")?;
            let shortcut = table
                .get::<Option<String>>("shortcut")?
                .map(crate::lower::shown_shortcut);
            table.raw_remove("shortcut")?;
            let options = MenuItemOptions {
                keep_open,
                checked,
                radio,
                shortcut,
            };
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(owner(lua)?, Kind::MenuItem(label, options)),
                    Some(&table),
                )?,
            )
        })?,
    )?;
    ui.set(
        "submenu",
        lua.create_function(|lua, table: Table| {
            let open = open_of(lua, &table)?;
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Submenu {
                            label: label(&table)?,
                            open,
                            trigger: match table.get::<mlua::Value>("trigger")? {
                                mlua::Value::Nil => None,
                                node => Some(expect_node(node, "`trigger`")?),
                            },
                        },
                    ),
                    Some(&table),
                )?,
            )
        })?,
    )?;

    // Components: `local Card = ui.component(function(props) … end)`, then
    // `Card { title = "…", child, … }`.
    ui.set(
        "component",
        lua.create_function(|lua, build: Function| {
            lua.create_function(move |lua, props: Option<Table>| {
                let props = match props {
                    Some(p) => p,
                    None => lua.create_table()?,
                };
                // Children stay in the props table's array part and are
                // also given as `props.children`.
                let kids = lua.create_table()?;
                for (i, child) in array_items(&props)?.into_iter().enumerate() {
                    kids.raw_set(i + 1, child)?;
                }
                props.raw_set("children", kids)?;
                let mut node = Node::new(
                    owner(lua)?,
                    Kind::Component {
                        build: build.clone(),
                        props: props.clone(),
                    },
                );
                if let Some(id) = props.get::<Option<String>>("id")? {
                    node.id = Some(id.into());
                }
                if let Some(class) = props.get::<Option<String>>("class")? {
                    node.classes = class.split_whitespace().map(Rc::from).collect();
                }
                node_ud(lua, node)
            })
        })?,
    )?;
    // `local` is a Luau keyword: component state is `ui.use_state`.
    ui.set(
        "use_state",
        lua.create_function(|lua, initial: Value| {
            let env = env(lua)?;
            if env.scope.borrow().is_none() {
                return Err(mlua::Error::runtime(
                    "ui.use_state is for component state; use ui.state at the top level",
                ));
            }
            if let Some(meter) = &env.meter {
                meter.room(initial.weight())?;
            }
            let signal = env.runtime.signal(initial);
            if let Some(meter) = &env.meter {
                meter.set(&signal, signal.with_untracked(Value::weight));
            }
            Ok(State::new(signal))
        })?,
    )?;
    ui.set(
        "effect",
        lua.create_function(|lua, f: Function| {
            let env = env(lua)?;
            let vm = env.vm.clone();
            let effect = env.runtime.effect(move || {
                if let Some(vm) = vm.upgrade()
                    && let Err(e) = vm.call::<()>(&f, ())
                {
                    vm.report(e);
                }
            });
            // Named for devtools' traces by where the script made it.
            #[cfg(feature = "inspect")]
            if zgx::trace::enabled()
                && let Some(at) = location(lua)
            {
                zgx::trace::name(effect.number(), format!("{at} effect"));
            }
            retain(&env, Box::new(effect));
            Ok(())
        })?,
    )?;
    // Timers: `ui.every(ms, fn)` calls `fn` every `ms`, `ui.after(ms, fn)`
    // once; each returns a function that stops it. Returning false from
    // `fn` stops it too. They live as long as what made them (an effect's
    // owner: a component, a builder, the script).
    for (name, repeat) in [("every", true), ("after", false)] {
        ui.set(
            name,
            lua.create_function(move |lua, (ms, f): (f64, Function)| {
                let env = env(lua)?;
                if !(ms.is_finite() && ms >= 0.) {
                    return Err(mlua::Error::runtime(format!(
                        "ui.{name}: a number of milliseconds"
                    )));
                }
                if repeat && ms < 16. {
                    crate::warn(
                        lua,
                        &format!(
                            "`ui.every({ms})` calls the script more often than a display \
                             refreshes; animate with anime or ui.tween (run in Rust, paced by \
                             the display), or tick at the rate the value changes"
                        ),
                    );
                }
                let vm = env.vm.clone();
                let period = crate::duration_ms(&format!("ui.{name}"), ms)?
                    .max(std::time::Duration::from_millis(1));
                let stop = env.timers.start(period, repeat, move || {
                    let Some(vm) = vm.upgrade() else { return false };
                    match vm.call::<Option<bool>>(&f, ()) {
                        Ok(keep) => keep != Some(false),
                        Err(e) => {
                            vm.report(e);
                            true
                        }
                    }
                });
                let handle = stop.handle();
                retain(&env, Box::new(stop));
                lua.create_function(move |_, ()| {
                    handle.set(false);
                    Ok(())
                })
            })?,
        )?;
    }
    ui.set(
        "computed",
        lua.create_function(|lua, f: Function| {
            let env = env(lua)?;
            Ok(computed_state(lua, &env, f))
        })?,
    )?;
    ui.set("sorted", lua.create_function(crate::sorted::sorted)?)?;
    ui.set("variants", lua.create_function(crate::variants::define)?)?;
    ui.set("raw", lua.create_function(crate::design::raw)?)?;
    ui.set("markdown", lua.create_function(crate::markdown::script)?)?;
    ui.set(
        "on_mount",
        lua.create_function(|lua, f: Function| {
            let env = env(lua)?;
            match &*env.scope.borrow() {
                Some(scope) => scope.mounted.borrow_mut().push(f),
                None => env.mounted.borrow_mut().push(f),
            }
            Ok(())
        })?,
    )?;
    ui.set(
        "batch",
        lua.create_function(|lua, f: Function| {
            let (runtime, vm) = {
                let env = env(lua)?;
                (env.runtime.clone(), env.vm.upgrade())
            };
            // The writes inside count; the updates after them do not.
            match vm {
                Some(vm) => vm.off_clock(|| runtime.batch(|| vm.call::<mlua::MultiValue>(&f, ()))),
                None => runtime.batch(|| f.call::<mlua::MultiValue>(())),
            }
        })?,
    )?;
    ui.set(
        "ref",
        lua.create_function(|lua, ()| Ok(RefUd(Ref::new(&env(lua)?.runtime))))?,
    )?;
    ui.set(
        "webview",
        lua.create_function(|lua, table: Table| {
            env(lua)?.may(crate::Capability::Web, "ui.webview")?;
            let web = match table.get::<mlua::Value>("web")? {
                mlua::Value::Nil => {
                    zgx::web_view::WebView::new(&env(lua)?.runtime, web_options(&table)?)
                }
                mlua::Value::UserData(ud) => ud.borrow::<WebUd>()?.0.clone(),
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "`web` takes a ui.web(), not {}",
                        other.type_name()
                    )));
                }
            };
            let src = match table.get::<mlua::Value>("src")? {
                mlua::Value::Nil => None,
                value => Some(source(lua, Ty::Str, "src", value)?),
            };
            let html = table
                .get::<Option<String>>("html")?
                .map(|html| -> mlua::Result<_> {
                    Ok((html, table.get::<Option<String>>("base_url")?))
                })
                .transpose()?;
            let handler = |key: &str| table.get::<Option<Function>>(key);
            let spec = WebSpec {
                web,
                src,
                html,
                on_navigate: handler("on_navigate")?,
                on_load: handler("on_load")?,
                on_title: handler("on_title")?,
                on_message: handler("on_message")?,
                on_error: handler("on_error")?,
                on_new_window: handler("on_new_window")?,
            };
            // What the constructor read is not a style.
            for key in [
                "web",
                "src",
                "html",
                "base_url",
                "transparent",
                "profile",
                "inspectable",
                "user_agent",
                "messages",
                "new_windows",
                "on_navigate",
                "on_load",
                "on_title",
                "on_message",
                "on_error",
                "on_new_window",
            ] {
                table.raw_set(key, mlua::Value::Nil)?;
            }
            let node = fill(
                lua,
                Node::new(owner(lua)?, Kind::WebView(Box::new(spec))),
                Some(&table),
            )?;
            node_ud(lua, node)
        })?,
    )?;
    // `ui.invoke(name, arg)`: call a host action now, from any handler.
    ui.set(
        "invoke",
        lua.create_function(|lua, (name, arg): (String, Option<Value>)| {
            let action = {
                let env = env(lua)?;
                env.permits(&env.exposed_actions, &name, "ui.invoke")?;
                env.actions.borrow().get(&name).cloned()
            };
            match action {
                Some(action) => {
                    action(arg.unwrap_or(Value::Nil));
                    Ok(())
                }
                None => Err(mlua::Error::runtime(format!(
                    "no host action named `{name}`"
                ))),
            }
        })?,
    )?;
    // `ui.call_sync(name, arg)`: a host function's value, now.
    ui.set(
        "call_sync",
        lua.create_function(|lua, (name, arg): (String, Option<Value>)| {
            let function = {
                let env = env(lua)?;
                env.permits(&env.exposed_actions, &name, "ui.call_sync")?;
                env.functions.borrow().get(&name).cloned()
            };
            match function {
                Some(function) => Ok(function(arg.unwrap_or(Value::Nil))),
                None => Err(mlua::Error::runtime(format!(
                    "no host function named `{name}`"
                ))),
            }
        })?,
    )?;
    ui.set(
        "terminal",
        lua.create_function(|lua, table: Table| {
            let term = match table.get::<mlua::Value>("term")? {
                mlua::Value::UserData(ud) if ud.is::<crate::term::TermUd>() => {
                    ud.borrow::<crate::term::TermUd>()?.term.clone()
                }
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "ui.terminal needs `term = ui.term(id)`, not {}",
                        other.type_name()
                    )));
                }
            };
            table.raw_set("term", mlua::Value::Nil)?;
            let node = fill(
                lua,
                Node::new(owner(lua)?, Kind::Terminal(term)),
                Some(&table),
            )?;
            node_ud(lua, node)
        })?,
    )?;
    ui.set(
        "shader",
        lua.create_function(|_, (source, opts): (String, Option<Table>)| {
            let fps = match &opts {
                Some(t) => t.get::<Option<f32>>("fps")?,
                None => None,
            };
            // One shader per source and rate: rebuilding a view does not
            // compile it again.
            type Shaders =
                std::collections::HashMap<(String, Option<u32>), Arc<zgx::decoration::FillShader>>;
            thread_local! {
                static SHADERS: RefCell<Shaders> = RefCell::default();
            }
            // A shader that does not compile is the script's error, shown
            // with its line, not a plain fill.
            zgx_gpu::check_fill_shader(&source)
                .map_err(|e| mlua::Error::runtime(format!("ui.shader: {e}")))?;
            let key = (source, fps.map(f32::to_bits));
            let shader = SHADERS.with(|shaders| {
                shaders
                    .borrow_mut()
                    .entry(key.clone())
                    .or_insert_with(|| zgx::decoration::FillShader::new(key.0.clone(), fps))
                    .clone()
            });
            Ok(ShaderUd(shader))
        })?,
    )?;
    ui.set(
        "layer_shader",
        lua.create_function(|_, (source, opts): (String, Option<Table>)| {
            let fps = match &opts {
                Some(t) => t.get::<Option<f32>>("fps")?,
                None => None,
            };
            // One shader per source and rate: rebuilding a view does not
            // compile it again.
            type Shaders =
                std::collections::HashMap<(String, Option<u32>), Arc<zgx::decoration::FillShader>>;
            thread_local! {
                static LAYER_SHADERS: RefCell<Shaders> = RefCell::default();
            }
            // A shader that does not compile is the script's error, shown
            // with its line, not a layer composited as painted.
            zgx_gpu::check_layer_shader(&source)
                .map_err(|e| mlua::Error::runtime(format!("ui.layer_shader: {e}")))?;
            let key = (source, fps.map(f32::to_bits));
            let shader = LAYER_SHADERS.with(|shaders| {
                shaders
                    .borrow_mut()
                    .entry(key.clone())
                    .or_insert_with(|| zgx::decoration::FillShader::new(key.0.clone(), fps))
                    .clone()
            });
            Ok(LayerShaderUd(shader))
        })?,
    )?;
    ui.set(
        "bitmap",
        lua.create_function(|_, (w, h, f): (u32, u32, Function)| {
            if w == 0 || h == 0 || w > 4096 || h > 4096 {
                return Err(mlua::Error::runtime(
                    "ui.bitmap takes 1 to 4096 pixels a side",
                ));
            }
            // White, with the alpha the function gives each pixel (0 to 1).
            let mut pixels = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let a: f64 = f.call((x, y))?;
                    let a = (a.clamp(0., 1.) * 255.).round() as u8;
                    pixels.extend_from_slice(&[a, a, a, a]);
                }
            }
            let image = zgx::image::ImageData::new(w, h, pixels)
                .map_err(|e| mlua::Error::runtime(format!("ui.bitmap: {e}")))?;
            Ok(BitmapUd(Arc::new(image)))
        })?,
    )?;

    ui.set(
        "slot",
        lua.create_function(|lua, (name, table): (String, Option<Table>)| {
            let (context, from) = match &table {
                Some(table) => {
                    let context: Option<Value> = table.get("context")?;
                    let from: Option<String> = table.get("from")?;
                    table.raw_set("context", mlua::Value::Nil)?;
                    table.raw_set("from", mlua::Value::Nil)?;
                    (context, from)
                }
                None => (None, None),
            };
            node_ud(
                lua,
                fill(
                    lua,
                    Node::new(
                        owner(lua)?,
                        Kind::Slot {
                            name,
                            context,
                            from,
                        },
                    ),
                    table.as_ref(),
                )?,
            )
        })?,
    )?;
    ui.set(
        "state",
        lua.create_function(|lua, (key, initial): (String, Value)| {
            let env = env(lua)?;
            let key = module_key(lua, &env, key);
            if let Some(meter) = &env.meter
                && env.store.get(&key).is_none()
            {
                meter.room(initial.weight())?;
            }
            let signal = env.store.state(&key, initial);
            if let Some(meter) = &env.meter {
                meter.set(&signal, signal.with_untracked(Value::weight));
            }
            Ok(State::new(signal))
        })?,
    )?;
    // `ui.persist(key, initial)`: state saved in the host's storage and
    // restored on the next run (and kept across reloads, like `ui.state`).
    ui.set(
        "persist",
        lua.create_function(|lua, (key, initial): (String, Value)| {
            let env = env(lua)?;
            let key = module_key(lua, &env, key);
            if let Some(meter) = &env.meter
                && env.persist.get(&env.persist_as, &key).is_none()
            {
                meter.room(initial.weight())?;
            }
            let signal = env.persist.state(&env.persist_as, &key, initial);
            if let Some(meter) = &env.meter {
                meter.set(&signal, signal.with_untracked(Value::weight));
            }
            Ok(State::new(signal))
        })?,
    )?;
    ui.set(
        "shared",
        lua.create_function(|lua, (key, initial): (String, Value)| {
            let env = env(lua)?;
            env.permits(&env.exposed_shared, &key, "ui.shared")?;
            let plugin = matches!(*env.grants, crate::Grants::Only(_));
            let state = State::new(env.shared.shared(&key, initial, plugin));
            if plugin && env.read_only_shared.borrow().contains(&key) {
                let read = crate::value::ReadState(state, key.as_str().into());
                return lua.create_userdata(read);
            }
            lua.create_userdata(state)
        })?,
    )?;

    let anim = |lua: &Lua,
                source: Option<Signal<Value>>,
                opts: Option<Table>,
                default: Curve|
     -> mlua::Result<mlua::AnyUserData> {
        let opts = match opts {
            Some(t) => t,
            None => lua.create_table()?,
        };
        let ease = match opts.get::<mlua::Value>("ease")? {
            mlua::Value::Nil => None,
            mlua::Value::String(s) => Some(
                crate::anime::ease::Ease::parse(&s.to_string_lossy())
                    .map_err(mlua::Error::runtime)?,
            ),
            mlua::Value::UserData(ud) => Some(ud.borrow::<crate::anime::api::EaseUd>()?.0.clone()),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "ease must be a name or ease, not {}",
                    other.type_name()
                )));
            }
        };
        let duration = opts.get::<Option<f64>>("duration")?;
        if let Some(ms) = duration {
            let looping = matches!(default, Curve::Loop { .. });
            crate::check_duration(lua, "duration", ms, looping);
        }
        let duration = duration.map(|ms| ms as f32);
        let curve = match default {
            Curve::Tween { ms, ease: e } => Curve::Tween {
                ms: duration.unwrap_or(ms),
                ease: ease.unwrap_or(e),
            },
            Curve::Spring { stiffness, damping } => Curve::Spring {
                stiffness: opts.get::<Option<f32>>("stiffness")?.unwrap_or(stiffness),
                damping: opts.get::<Option<f32>>("damping")?.unwrap_or(damping),
            },
            Curve::Loop {
                ms,
                ease: e,
                yoyo,
                fps,
            } => Curve::Loop {
                ms: duration.unwrap_or(ms),
                ease: ease.unwrap_or(e),
                yoyo: opts.get::<Option<bool>>("yoyo")?.unwrap_or(yoyo),
                fps: opts.get::<Option<f64>>("fps")?.or(fps),
            },
        };
        if let Curve::Tween { ms, .. } | Curve::Loop { ms, .. } = curve
            && !(ms.is_finite() && ms > 0.)
        {
            return Err(mlua::Error::runtime("duration must be positive"));
        }
        let from: Option<Value> = opts.get("from")?;
        let to: Option<Value> = opts.get("to")?;
        if from.is_some() != to.is_some() {
            return Err(mlua::Error::runtime(
                "give both `from` and `to`, or neither",
            ));
        }
        lua.create_userdata(AnimUd(Rc::new(Anim {
            source,
            curve,
            from,
            to,
        })))
    };
    use crate::anime::ease::{Base, Ease, Mode};
    ui.set(
        "tween",
        lua.create_function(move |lua, (state, opts): (mlua::Value, Option<Table>)| {
            let curve = Curve::Tween {
                ms: 250.,
                ease: Ease::Curve(Mode::Out, Base::Power(3.)),
            };
            anim(
                lua,
                Some(expect_readable(&state, "ui.tween's source")?),
                opts,
                curve,
            )
        })?,
    )?;
    ui.set(
        "spring",
        lua.create_function(move |lua, (state, opts): (mlua::Value, Option<Table>)| {
            let curve = Curve::Spring {
                stiffness: 260.,
                damping: 24.,
            };
            anim(
                lua,
                Some(expect_readable(&state, "ui.spring's source")?),
                opts,
                curve,
            )
        })?,
    )?;
    ui.set(
        "loop",
        lua.create_function(move |lua, opts: Option<Table>| {
            let curve = Curve::Loop {
                ms: 1000.,
                ease: Ease::Curve(Mode::InOut, Base::Power(2.)),
                yoyo: true,
                fps: None,
            };
            anim(lua, None, opts, curve)
        })?,
    )?;

    let action = |lua: &Lua, action: Action| lua.create_userdata(ActionUd(action));
    ui.set(
        "set",
        lua.create_function(move |lua, (state, value): (mlua::Value, Value)| {
            action(
                lua,
                Action::Set(expect_state(&state, "ui.set's target")?, value),
            )
        })?,
    )?;
    ui.set(
        "toggle",
        lua.create_function(move |lua, state: mlua::Value| {
            action(
                lua,
                Action::Toggle(expect_state(&state, "ui.toggle's target")?),
            )
        })?,
    )?;
    ui.set(
        "add",
        lua.create_function(move |lua, (state, by): (mlua::Value, Option<f64>)| {
            action(
                lua,
                Action::Add(expect_state(&state, "ui.add's target")?, by.unwrap_or(1.)),
            )
        })?,
    )?;
    ui.set(
        "action",
        lua.create_function(move |lua, (name, arg): (String, Value)| {
            {
                let env = env(lua)?;
                env.permits(&env.exposed_actions, &name, "ui.action")?;
            }
            action(lua, Action::Host(name.into(), arg))
        })?,
    )?;
    // `ui.use_context(name, default)`: what the nearest `provide = { name =
    // … }` above the component or row being built gave, else `default`.
    ui.set(
        "use_context",
        lua.create_function(|lua, (name, default): (String, mlua::Value)| {
            let env = env(lua)?;
            let scope = env.scope.borrow();
            let Some(scope) = scope.as_ref() else {
                return Err(mlua::Error::runtime(
                    "ui.use_context reads what a component, row or presence is built in: call it in one's build",
                ));
            };
            Ok(scope
                .provided
                .as_ref()
                .and_then(|p| p.0.get(name.as_str()).cloned())
                .unwrap_or(default))
        })?,
    )?;
    // `ui.on_cleanup(f)`: `f` runs when what is being built goes away (a
    // component unmounting, an effect re-running, the script reloading).
    ui.set(
        "on_cleanup",
        lua.create_function(|lua, f: Function| {
            struct Cleanup(Weak<Vm>, Option<Function>);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    if let (Some(vm), Some(f)) = (self.0.upgrade(), self.1.take())
                        && let Err(error) = vm.call::<()>(&f, ())
                    {
                        vm.report(error);
                    }
                }
            }
            let env = env(lua)?;
            retain(&env, Box::new(Cleanup(env.vm.clone(), Some(f))));
            Ok(())
        })?,
    )?;
    // `ui.now()`: milliseconds since the epoch, to the microsecond (Luau's
    // `os.time` counts whole seconds). Not tracked.
    ui.set(
        "now",
        lua.create_function(|_, ()| {
            Ok(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0., |d| d.as_secs_f64() * 1000.))
        })?,
    )?;
    // `print` to stderr, a line at a time (stdout is buffered whole when an
    // app's output goes to a file, so prints never showed in its log).
    lua.globals().set(
        "print",
        lua.create_function(|_, args: mlua::Variadic<mlua::Value>| {
            let line = args
                .iter()
                .map(|v| v.to_string().unwrap_or_else(|_| format!("{v:?}")))
                .collect::<Vec<_>>()
                .join("\t");
            eprintln!("{line}");
            Ok(())
        })?,
    )?;
    // Read-only once the host's own parts are in (see `ScriptHost::run`).
    lua.globals().set("ui", ui)
}

/// A state key as the calling chunk names it: a required module's keys are
/// its own (two modules' "open" are two states); the script's own keys stay
/// as they are.
fn module_key(lua: &Lua, env: &Env, key: String) -> String {
    match module_prefix(lua, env) {
        Some(prefix) => prefix + &key,
        None => key,
    }
}
/// What the calling chunk's state keys start with: `ui/card.luau::` in a
/// required module, nothing in the script itself.
fn module_prefix(lua: &Lua, env: &Env) -> Option<String> {
    let chunk = lua
        .inspect_stack(1, |d| d.source().source.map(|s| s.into_owned()))
        .flatten()?;
    let path = chunk.strip_prefix('@')?;
    let script = env.vm.upgrade().is_some_and(|vm| vm.name == path);
    (!script).then(|| format!("{path}::"))
}

/// `f`'s value, kept current in Rust: `ui.computed`. It lives as long as
/// what is being built (a component), or the script.
pub(crate) fn computed(lua: &Lua, env: &Env, f: Function) -> Signal<Value> {
    computed_state(lua, env, f).0
}
/// `ui.computed`: the value `f` makes, kept current as what it reads
/// changes, and brought up to date when a script reads it.
pub(crate) fn computed_state(_: &Lua, env: &Env, f: Function) -> crate::value::State {
    let signal = env.runtime.signal(Value::Nil);
    let (vm, out) = (env.vm.clone(), signal.clone());
    let meter = env.meter.clone();
    // Its value is there as soon as it is made, even while a component
    // builds (inside a flush), where an effect would run only after the
    // build and its `on_mount`s.
    let effect = env.runtime.effect_now(move || {
        if let Some(vm) = vm.upgrade() {
            // Against what it was: unchanged parts keep their values.
            let old = out.with_untracked(Clone::clone);
            let result = vm.call::<mlua::Value>(&f, ()).and_then(|returned| {
                let table = returned.as_table().cloned();
                let value = crate::value::convert(returned, Some(&old))?;
                if let Some(meter) = &meter {
                    meter.hold(&out, value.weight())?;
                }
                if let Some(table) = table {
                    // Readers get the table back, not a conversion.
                    let _ = crate::value::remember(&vm.lua, &table, &value);
                }
                Ok(value)
            });
            match result {
                Ok(v) => {
                    out.set(v);
                }
                Err(e) => vm.report(e),
            }
        }
    });
    let refresh = effect.refresher();
    retain(env, Box::new(effect));
    crate::value::State(signal, Some(crate::value::Fresh::One(refresh)))
}
/// A node's interaction styles by slot (`variants::INTERACTION`'s order).
pub(crate) fn interaction_of(pseudo: &[(Pseudo, Styles)]) -> [Vec<(Prop, Lit)>; 4] {
    let mut out: [Vec<(Prop, Lit)>; 4] = Default::default();
    for (pseudo, styles) in pseudo {
        out[pseudo.slot()].extend(styles.iter().cloned());
    }
    out
}
/// The other way: the slots that set anything.
pub(crate) fn pseudo_of(slots: [Vec<(Prop, Lit)>; 4]) -> Vec<(Pseudo, Styles)> {
    let pseudos = [
        Pseudo::Hover,
        Pseudo::Active,
        Pseudo::Focus,
        Pseudo::Disabled,
    ];
    pseudos
        .into_iter()
        .zip(slots)
        .filter(|(_, styles)| !styles.is_empty())
        .map(|(pseudo, styles)| (pseudo, styles.into()))
        .collect()
}

/// Keeps `resource` for the component being built, or for the script.
/// A panel's `build`: its content, built again each time `open` turns
/// true (a form starting from what it edits, its own state new), and kept
/// while it closes. A `ui.presence` keyed by how many times it has opened.
fn built_on_open(lua: &Lua, open: mlua::Value, build: Function) -> mlua::Result<Rc<Node>> {
    const SOURCE: &str = r#"
local presence = ...
return function(open, build)
    local opened, was = 0, false
    return presence(function()
        local now = open:get()
        if now and not was then
            opened += 1
        end
        was = now
        return opened
    end, function()
        return build()
    end)
end
"#;
    const KEY: &str = "zgx.built_on_open";
    let make = match lua.named_registry_value::<Function>(KEY) {
        Ok(make) => make,
        Err(_) => {
            let ui: Table = lua.globals().get("ui")?;
            let presence: Function = ui.get("presence")?;
            let make: Function = lua.load(SOURCE).set_name("=build").call(presence)?;
            lua.set_named_registry_value(KEY, make.clone())?;
            make
        }
    };
    let built: mlua::AnyUserData = make.call((open, build))?;
    Ok(built.borrow::<NodeUd>()?.0.clone())
}

pub(crate) fn retain(env: &Env, resource: Box<dyn Any>) {
    match &*env.scope.borrow() {
        Some(scope) => scope.retained.borrow_mut().push(resource),
        None => env.retained.borrow_mut().push(resource),
    }
}

#[cfg(test)]
mod tests {
    /// The constructors' own fields are matched before style properties
    /// (`fill_from`), so a name in both is the constructor's: `size` is the
    /// one. A new field named like a style property fails here until it is
    /// given a meaning on purpose.
    #[test]
    fn constructor_fields_named_like_style_properties_keep_their_meaning() {
        use super::{Kind, LATE_FIELDS, Node, Prop, WIDGET_FIELDS, fill_from};
        let both: Vec<_> = WIDGET_FIELDS
            .iter()
            .chain(LATE_FIELDS)
            .copied()
            .filter(|name| Prop::parse(name).is_some())
            .collect();
        assert_eq!(
            both,
            ["size"],
            "a constructor field named like a style property"
        );
        let lua = mlua::Lua::new();
        let table: mlua::Table = lua
            .load("return { size = { 10, 20 }, w = 30 }")
            .eval()
            .unwrap();
        let mut node = Node::new(Default::default(), Kind::Div);
        fill_from(&lua, &mut node, &table, None, crate::DesignLint::Off).unwrap();
        let props: Vec<_> = node.props.iter().map(|(prop, _)| prop.name()).collect();
        assert_eq!(
            props,
            ["w"],
            "size is the constructor's, not a style property"
        );
    }

    /// A known style key keeps the same numeric bits and fallback errors
    /// as a key read for the first time.
    #[test]
    fn cached_numeric_style_keys_keep_values_and_errors() {
        use super::{Kind, Lit, Node, Prop, Source, StyleKeys, fill_from};
        fn read(lua: &mlua::Lua, table: &mlua::Table) -> Result<Vec<u32>, String> {
            let mut node = Node::new(Default::default(), Kind::Div);
            fill_from(lua, &mut node, table, None, crate::DesignLint::Off)
                .map_err(|e| e.to_string())?;
            let Source::Lit(lit) = &node.props[0].1 else {
                panic!("expected a literal");
            };
            Ok(match lit {
                Lit::Num(n) | Lit::Len(zgx::layout::Length::Px(n)) => vec![n.to_bits()],
                Lit::Pair(a, b) => vec![a.to_bits(), b.to_bits()],
                Lit::Len(zgx::layout::Length::Percent(n)) => vec![1, n.to_bits()],
                Lit::Len(zgx::layout::Length::Auto) => vec![2],
                other => panic!("unexpected literal: {other:?}"),
            })
        }
        let lua = mlua::Lua::new();
        let mut values: Vec<mlua::Value> = [
            0.,
            -0.,
            1.5,
            -3.25,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::from_bits(0x7ff8_1234_5678_0000),
            (1_u64 << 53) as f64,
        ]
        .into_iter()
        .map(mlua::Value::Number)
        .collect();
        values.extend([mlua::Value::Boolean(false), mlua::Value::Boolean(true)]);
        for text in ["12", "25%", "auto", "not a number"] {
            values.push(mlua::Value::String(lua.create_string(text).unwrap()));
        }
        values.push(mlua::Value::Table(
            lua.create_sequence_from([2., 3.]).unwrap(),
        ));
        for prop in [Prop::W, Prop::Opacity, Prop::Origin] {
            for value in &values {
                lua.set_app_data(StyleKeys::default());
                let table = lua.create_table().unwrap();
                table.raw_set(prop.name(), value.clone()).unwrap();
                let first = read(&lua, &table);
                let cached = read(&lua, &table);
                assert_eq!(cached, first, "{} = {value:?}", prop.name());
            }
        }
    }
    /// A hex colour read as bytes is the colour its animation value
    /// (channels as `f64`, alpha a fraction) rounds back to, in each form
    /// and at every channel value; and what is not hex is no colour.
    #[test]
    fn hex_colours_as_bytes_match_their_animation_values() {
        let animated = |text: &str| {
            crate::anime::value::parse_color(text).map(crate::anime::target::v_to_color)
        };
        for v in 0..=255_u8 {
            let d = v >> 4;
            for text in [
                format!("#{v:02x}{:02x}{:02X}{:02x}", 255 - v, v / 3, v ^ 0x5a),
                format!("#{v:02x}{:02x}{:02x}", 255 - v, v / 7),
                format!("#{d:x}{:x}{:X}{:x}", 15 - d, d / 2, d ^ 5),
                format!("#{d:x}{:x}{:x}", 15 - d, d / 3),
                format!("  #{v:02x}{v:02x}{v:02x}  "),
            ] {
                assert_eq!(super::parse_color(&text), animated(&text), "{text}");
                assert!(super::parse_color(&text).is_some(), "{text}");
            }
        }
        for text in [
            "#",
            "#12",
            "#12345",
            "#1234567",
            "#123456789",
            "#12g",
            "#１２３",
            "#é1",
            "#1é",
            "#12\0",
        ] {
            assert_eq!(super::parse_color(text), None, "{text}");
            assert_eq!(animated(text), None, "{text}");
        }
    }
    /// A walk's borrowed key and value (`for_each_borrowed`, as `fill_from`
    /// reads properties) read the same from a callback nested in the walk's,
    /// whose stack frame holds other values at the pair's indices.
    #[test]
    fn borrowed_pairs_read_the_same_from_nested_callbacks() {
        let lua = mlua::Lua::new();
        let table: mlua::Table = lua.load(r#"return { name = "outer" }"#).eval().unwrap();
        let call: mlua::Function = lua
            .load("return function(f) return f('a', 'b', 'c', 'd') end")
            .eval()
            .unwrap();
        let mut seen = 0;
        table
            .for_each_borrowed(|key, value| {
                lua.scope(|scope| {
                    let nested = scope.create_function_mut(|_, _: mlua::MultiValue| {
                        assert_eq!(key.as_bytes(), Some(&b"name"[..]));
                        assert_eq!(value.as_str(), Some("outer"));
                        assert_eq!(value.type_name(), "string");
                        seen += 1;
                        Ok(())
                    })?;
                    call.call::<()>(nested)
                })
            })
            .unwrap();
        assert_eq!(seen, 1);
    }
}
