//! Applied themes: a runtime's registry of themes and the scopes that
//! resolve them.
//!
//! A [`ThemeScope`] holds every token's value for one subtree in an array
//! indexed by token id. A style reading a token reads that token's signal,
//! created the first time anything reads it, so it subscribes to exactly the
//! tokens it uses. Applying a theme evaluates every token once, compares,
//! and sets only the signals whose values changed: what reads them restyles,
//! nothing else runs.
use super::file::{Entry, Theme, ThemeError, ThemeKind, ThemeOverride, TokenInput};
use super::tokens::{
    self, ColorToken, FontToken, NumberToken, STANDARD, ShadowToken, TextToken, TokenId,
};
use super::value::{Expr, TokenValue};
use crate::reactive::{Runtime, Signal, WeakRuntime};
use crate::scene::{BoxShadow, Color};
use crate::text_layout::FontFamily;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{Arc, OnceLock},
    time::SystemTime,
};

/// The name of the built-in dark theme, the default.
pub const DARK: &str = "zgui-dark";
/// The name of the built-in light theme.
pub const LIGHT: &str = "zgui-light";

/// The built-in themes, parsed once per process.
fn builtins() -> &'static [Arc<Theme>; 2] {
    static BUILTINS: OnceLock<[Arc<Theme>; 2]> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let read = |text: &str| Arc::new(Theme::from_json(text).expect("a built-in theme"));
        [
            read(include_str!("../../themes/zgui-dark.json")),
            read(include_str!("../../themes/zgui-light.json")),
        ]
    })
}
/// A declared token: its default and the source it was written as.
type Declaration = (TokenId, Arc<Expr>, Arc<str>);
/// Every token's expression in a scope, by token id.
type Exprs = Rc<Vec<Option<Arc<Expr>>>>;
/// zgui's widgets' token declarations (see `tokens::STANDARD`).
fn widget_defaults() -> &'static [Declaration] {
    static DEFAULTS: OnceLock<Vec<Declaration>> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        STANDARD
            .iter()
            .enumerate()
            .filter_map(|(i, (_, kind, default))| {
                let default = (*default)?;
                let expr = super::value::parse(default, Some(*kind)).expect("a widget default");
                Some((TokenId(i as u32), Arc::new(expr), default.into()))
            })
            .collect()
    })
}

/// Whether the system shows light or dark windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
}

struct Watched {
    path: PathBuf,
    stamp: Option<SystemTime>,
}
struct State {
    themes: HashMap<String, Arc<Theme>>,
    /// Registration order, for `names`.
    order: Vec<String>,
    /// App and widget token defaults, by token id.
    declared: Vec<Option<(Arc<Expr>, Arc<str>)>>,
    current: String,
    /// Whether an app chose the theme (`set`), which `set_default` keeps.
    chosen: bool,
    follow: Option<(String, String)>,
    appearance: Appearance,
    files: Vec<Watched>,
    /// Where theme files come from, when not the disk (`set_reader`).
    reader: Option<ThemeReader>,
    /// What the platform sets (`Themes::set_system_tokens`): the system's
    /// selection colours, say.
    system: Vec<(TokenId, Arc<Expr>)>,
    /// The system's text size (`Themes::set_system_text_scale`).
    text_scale: f32,
}
impl State {
    /// Every token's expression under `name`: declarations, then the
    /// built-in base, then each theme from the base of `name`'s chain to it.
    fn flatten(&self, name: &str) -> Vec<Option<Arc<Expr>>> {
        let mut out: Vec<Option<Arc<Expr>>> = self
            .declared
            .iter()
            .map(|d| d.as_ref().map(|(e, _)| e.clone()))
            .collect();
        let mut chain: Vec<&Arc<Theme>> = Vec::new();
        let mut next = Some(name);
        while let Some(name) = next {
            let Some(theme) = self.themes.get(name) else {
                break;
            };
            if chain.iter().any(|t| Arc::ptr_eq(t, theme)) {
                warn(&format!("theme `{name}` extends itself"));
                break;
            }
            chain.push(theme);
            next = theme.extends.as_deref();
        }
        let base_kind = chain.last().map_or(ThemeKind::Dark, |t| t.kind);
        let builtin = &builtins()[usize::from(!base_kind.is_dark())];
        if !chain.iter().any(|t| Arc::ptr_eq(t, builtin)) {
            chain.push(builtin);
        }
        // The built-in base, then what the system sets over it, then the
        // app's themes: an app theme that sets a token keeps it.
        let mut themes = chain.iter().rev();
        if let Some(base) = themes.next() {
            apply(&mut out, &base.tokens);
        }
        for (id, expr) in &self.system {
            set(&mut out, *id, expr.clone());
        }
        for theme in themes {
            apply(&mut out, &theme.tokens);
        }
        out
    }
}
fn apply(out: &mut Vec<Option<Arc<Expr>>>, entries: &[Entry]) {
    for entry in entries {
        set(out, entry.id, entry.expr.clone());
    }
}
fn set(out: &mut Vec<Option<Arc<Expr>>>, id: TokenId, expr: Arc<Expr>) {
    if out.len() <= id.index() {
        out.resize(id.index() + 1, None);
    }
    out[id.index()] = Some(expr);
}

/// Evaluates a scope's expressions, each token once, following references.
struct Evaluator<'a> {
    exprs: &'a [Option<Arc<Expr>>],
    values: Vec<TokenValue>,
    /// 0 not yet, 1 being evaluated (a reference cycle), 2 done.
    state: Vec<u8>,
}
impl Evaluator<'_> {
    fn eval(&mut self, id: TokenId) -> TokenValue {
        let i = id.index();
        if i >= self.values.len() {
            return TokenValue::Unset;
        }
        match self.state[i] {
            2 => return self.values[i].clone(),
            1 => {
                warn(&format!("token `{id}` refers to itself"));
                return TokenValue::Unset;
            }
            _ => {}
        }
        self.state[i] = 1;
        let value = match self.exprs.get(i).cloned().flatten() {
            Some(expr) => expr.eval(&mut |id| self.eval(id)),
            None => TokenValue::Unset,
        };
        let value = match tokens::standard_kind(id) {
            Some(kind) if value.kind().is_some_and(|k| k != kind) => {
                warn(&format!("token `{id}` is a {kind:?} token, not {value}"));
                TokenValue::Unset
            }
            _ => value,
        };
        self.state[i] = 2;
        self.values[i] = value.clone();
        value
    }
}
/// The font size and line height tokens, times the system's text size. A
/// size set in points on a view stays as it is, as CSS's `px`.
fn scale_text(values: &mut [TokenValue], scale: f32) {
    if scale == 1. {
        return;
    }
    use super::tok;
    for token in [
        tok::FONT_SIZE_XS,
        tok::FONT_SIZE_SM,
        tok::FONT_SIZE_MD,
        tok::FONT_SIZE_LG,
        tok::FONT_SIZE_XL,
        tok::FONT_SIZE_XXL,
        tok::FONT_LINE_XS,
        tok::FONT_LINE_SM,
        tok::FONT_LINE_MD,
        tok::FONT_LINE_LG,
        tok::FONT_LINE_XL,
        tok::FONT_LINE_XXL,
    ] {
        if let Some(TokenValue::Number(n)) = values.get_mut(token.0.index()) {
            *n *= scale;
        }
    }
}
fn evaluate(exprs: &[Option<Arc<Expr>>]) -> Vec<TokenValue> {
    let count = tokens::count().max(exprs.len());
    let mut evaluator = Evaluator {
        exprs,
        values: vec![TokenValue::Unset; count],
        state: vec![0; count],
    };
    for i in 0..count {
        evaluator.eval(TokenId(i as u32));
    }
    evaluator.values
}

/// Theme problems go to stderr in debug builds; release builds stay quiet.
fn warn(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("zgui theme: {message}");
    }
}
/// The first read of a token nothing defines warns, once per token.
fn warn_unknown(id: TokenId) {
    if !cfg!(debug_assertions) {
        return;
    }
    static WARNED: OnceLock<std::sync::Mutex<std::collections::HashSet<TokenId>>> = OnceLock::new();
    if WARNED
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(id)
    {
        warn(&format!(
            "unknown token `{id}`: no theme sets it and nothing declared it, so it reads as \
             transparent, 0 or the default font"
        ));
    }
}

struct Shared {
    runtime: WeakRuntime,
    state: RefCell<State>,
    /// The root theme's name, for what shows it (a theme picker).
    current: Signal<Arc<str>>,
}

/// Token values for a subtree. The root scope is the runtime's theme;
/// child scopes ([`ThemeScope::child`]) change a theme or some tokens for a
/// subtree and follow their parent otherwise.
pub struct ThemeScope {
    shared: Rc<Shared>,
    parent: Option<Rc<ThemeScope>>,
    spec: RefCell<ThemeOverride>,
    /// Every token's expression here, shared with children that add nothing.
    exprs: RefCell<Rc<Vec<Option<Arc<Expr>>>>>,
    values: RefCell<Vec<TokenValue>>,
    /// Created by the first read of a token, so a scope keeps signals only
    /// for the tokens something uses.
    signals: RefCell<Vec<Option<Signal<TokenValue>>>>,
    children: RefCell<Vec<Weak<ThemeScope>>>,
    name: RefCell<Arc<str>>,
    /// Counts the refreshes that changed a value.
    epoch: std::cell::Cell<u64>,
    /// The body text style as one signal (see `ThemeScope::body`).
    body: RefCell<Option<Signal<Body>>>,
}
/// What text shows unless it says otherwise: `text.primary`, `font.size.md`
/// and `font.sans`. Text reads them together, with one subscription.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Body {
    pub color: Color,
    pub size: f32,
    pub family: FontFamily,
}
impl ThemeScope {
    fn new(
        shared: Rc<Shared>,
        parent: Option<Rc<ThemeScope>>,
        spec: ThemeOverride,
    ) -> Rc<ThemeScope> {
        let scope = Rc::new(ThemeScope {
            shared,
            parent,
            spec: RefCell::new(spec),
            exprs: RefCell::default(),
            values: RefCell::default(),
            signals: RefCell::default(),
            children: RefCell::default(),
            name: RefCell::new("".into()),
            epoch: std::cell::Cell::new(0),
            body: RefCell::default(),
        });
        scope.refresh();
        scope
    }
    /// The system's text size here: 1 under a scope with fixed text sizes.
    fn text_scale(&self, system: f32) -> f32 {
        let mut scope = Some(self);
        while let Some(current) = scope {
            if current.spec.borrow().fixed_text_size {
                return 1.;
            }
            scope = current.parent.as_deref();
        }
        system
    }
    /// Recompute this scope's values, notify what read changed tokens and do
    /// the same for its children. Call inside a batch.
    fn refresh(&self) {
        let (name, exprs, text_scale) = {
            let state = self.shared.state.borrow();
            let spec = self.spec.borrow();
            let (name, base): (Arc<str>, Exprs) = match (&self.parent, &spec.theme) {
                (_, Some(theme)) if state.themes.contains_key(theme) => {
                    (theme.as_str().into(), Rc::new(state.flatten(theme)))
                }
                (Some(parent), _) => {
                    if let Some(theme) = &spec.theme {
                        warn(&format!("no theme `{theme}` to scope"));
                    }
                    (parent.name.borrow().clone(), parent.exprs.borrow().clone())
                }
                (None, _) => (
                    state.current.as_str().into(),
                    Rc::new(state.flatten(&state.current)),
                ),
            };
            let exprs = if spec.tokens.is_empty() {
                base
            } else {
                let mut own = (*base).clone();
                for (id, expr) in &spec.tokens {
                    set(&mut own, *id, expr.clone());
                }
                Rc::new(own)
            };
            (name, exprs, self.text_scale(state.text_scale))
        };
        let mut values = evaluate(&exprs);
        scale_text(&mut values, text_scale);
        *self.exprs.borrow_mut() = exprs;
        *self.name.borrow_mut() = name;
        if *self.values.borrow() != values {
            self.epoch.set(self.epoch.get() + 1);
        }
        let changed: Vec<(Signal<TokenValue>, TokenValue)> = {
            let old = self.values.borrow();
            let signals = self.signals.borrow();
            signals
                .iter()
                .enumerate()
                .filter_map(|(i, signal)| {
                    let signal = signal.as_ref()?;
                    let next = values.get(i).cloned().unwrap_or_default();
                    (old.get(i) != Some(&next)).then(|| (signal.clone(), next))
                })
                .collect()
        };
        *self.values.borrow_mut() = values;
        for (signal, value) in changed {
            signal.set(value);
        }
        let body = self.body.borrow().clone();
        if let Some(body) = body {
            body.set(self.body_untracked());
        }
        let children: Vec<Rc<ThemeScope>> = {
            let mut children = self.children.borrow_mut();
            children.retain(|c| c.strong_count() > 0);
            children.iter().filter_map(Weak::upgrade).collect()
        };
        for child in children {
            child.refresh();
        }
    }
    fn batch(&self, f: impl FnOnce()) {
        match self.shared.runtime.upgrade() {
            Some(runtime) => runtime.batch(f),
            None => f(),
        }
    }
    fn signal(&self, id: TokenId) -> Option<Signal<TokenValue>> {
        let i = id.index();
        if let Some(Some(signal)) = self.signals.borrow().get(i) {
            return Some(signal.clone());
        }
        let runtime = self.shared.runtime.upgrade()?;
        let value = self.values.borrow().get(i).cloned().unwrap_or_default();
        if value == TokenValue::Unset {
            warn_unknown(id);
        }
        let signal = runtime.signal(value);
        let mut signals = self.signals.borrow_mut();
        if signals.len() <= i {
            signals.resize(i + 1, None);
        }
        signals[i] = Some(signal.clone());
        Some(signal)
    }
    /// The token's value here. Read inside an effect (a style, a computed
    /// property), it subscribes that effect to this token alone.
    pub fn get(&self, token: impl Into<TokenId>) -> TokenValue {
        let id = token.into();
        match self.signal(id) {
            Some(signal) => signal.get(),
            None => self.get_untracked(id),
        }
    }
    fn body_untracked(&self) -> Body {
        let values = self.values.borrow();
        let get = |token: TokenId| values.get(token.index());
        Body {
            color: get(super::tok::TEXT_PRIMARY.0)
                .and_then(TokenValue::color)
                .unwrap_or(Color(0, 0, 0, 0)),
            size: get(super::tok::FONT_SIZE_MD.0)
                .and_then(TokenValue::number)
                .unwrap_or(16.),
            family: get(super::tok::FONT_SANS.0)
                .and_then(TokenValue::font)
                .unwrap_or_default(),
        }
    }
    /// `f` of the body text style here, read in place (tracked, as one
    /// dependency): text reads it on every restyle, mostly to compare.
    pub(crate) fn with_body<R>(&self, f: impl FnOnce(&Body) -> R) -> R {
        if let Some(signal) = &*self.body.borrow() {
            return signal.with(f);
        }
        match self.body_signal() {
            Some(signal) => signal.with(f),
            None => f(&self.body_untracked()),
        }
    }
    /// `with_body` without subscribing: for an effect that called
    /// `subscribe_body`.
    pub(crate) fn with_body_untracked<R>(&self, f: impl FnOnce(&Body) -> R) -> R {
        match &*self.body.borrow() {
            Some(signal) => signal.with_untracked(f),
            None => f(&self.body_untracked()),
        }
    }
    /// Subscribe the running effect to the body text style for as long as
    /// it lives (see `Signal::subscribe`): text restyled every frame then
    /// reads it untracked.
    pub(crate) fn subscribe_body(&self) {
        let existing = self.body.borrow().clone();
        if let Some(signal) = existing.or_else(|| self.body_signal()) {
            signal.subscribe();
        }
    }
    /// The body signal, made on first use.
    fn body_signal(&self) -> Option<Signal<Body>> {
        if let Some(signal) = &*self.body.borrow() {
            return Some(signal.clone());
        }
        let runtime = self.shared.runtime.upgrade()?;
        let signal = runtime.signal(self.body_untracked());
        *self.body.borrow_mut() = Some(signal.clone());
        Some(signal)
    }
    /// Subscribe the running effect to the token without reading its value.
    pub(crate) fn track(&self, token: TokenId) {
        if let Some(signal) = self.signal(token) {
            signal.with(|_| ());
        }
    }
    /// Increases whenever a refresh changes any token's value here.
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.get()
    }
    /// The token's value here, without subscribing to it.
    pub fn get_untracked(&self, token: impl Into<TokenId>) -> TokenValue {
        self.values
            .borrow()
            .get(token.into().index())
            .cloned()
            .unwrap_or_default()
    }
    /// A colour token's value (transparent if unset).
    pub fn color(&self, token: ColorToken) -> Color {
        self.get(token).color().unwrap_or(Color(0, 0, 0, 0))
    }
    pub fn color_untracked(&self, token: ColorToken) -> Color {
        self.get_untracked(token)
            .color()
            .unwrap_or(Color(0, 0, 0, 0))
    }
    /// A number token's value (0 if unset).
    pub fn number(&self, token: NumberToken) -> f32 {
        self.get(token).number().unwrap_or(0.)
    }
    pub fn number_untracked(&self, token: NumberToken) -> f32 {
        self.get_untracked(token).number().unwrap_or(0.)
    }
    pub fn font(&self, token: FontToken) -> FontFamily {
        self.get(token).font().unwrap_or_default()
    }
    pub fn shadows(&self, token: ShadowToken) -> Arc<[BoxShadow]> {
        self.get(token).shadows().unwrap_or_else(|| Arc::from([]))
    }
    pub fn text(&self, token: TextToken) -> Arc<str> {
        self.get(token).text().unwrap_or_else(|| "".into())
    }
    /// Whether a theme or a declaration defines the token here.
    pub fn is_defined(&self, token: impl Into<TokenId>) -> bool {
        self.exprs
            .borrow()
            .get(token.into().index())
            .is_some_and(Option::is_some)
    }
    /// The name of the theme in effect here (not tracked).
    pub fn theme_name(&self) -> Arc<str> {
        self.name.borrow().clone()
    }
    /// The kind (dark, light) of the theme in effect here.
    pub fn theme_kind(&self) -> ThemeKind {
        let state = self.shared.state.borrow();
        state
            .themes
            .get(&**self.name.borrow())
            .map(|t| {
                // A theme's kind is its own, else its chain's.
                t.kind
            })
            .unwrap_or_default()
    }
    /// A scope for a subtree that changes `spec` and otherwise follows this
    /// one, including when this one's theme changes.
    pub fn child(self: &Rc<Self>, spec: ThemeOverride) -> Rc<ThemeScope> {
        let child = ThemeScope::new(self.shared.clone(), Some(self.clone()), spec);
        self.children.borrow_mut().push(Rc::downgrade(&child));
        child
    }
    /// Change what this child scope overrides; what reads changed tokens
    /// restyles. (The root scope follows `Themes::set` instead.)
    pub fn set_override(&self, spec: ThemeOverride) {
        if self.parent.is_none() || *self.spec.borrow() == spec {
            return;
        }
        *self.spec.borrow_mut() = spec;
        self.batch(|| self.refresh());
    }
    /// How many token signals this scope keeps: how many tokens were read.
    pub fn signal_count(&self) -> usize {
        self.signals.borrow().iter().filter(|s| s.is_some()).count()
    }
}

/// A runtime's themes: the ones it knows, the one applied, the tokens apps
/// declare, and the root [`ThemeScope`]. Get it with [`Themes::of`] (or
/// `Ui::theme`); every `Ui` and script host on the runtime shares it.
pub struct Themes {
    shared: Rc<Shared>,
    root: Rc<ThemeScope>,
}
impl Themes {
    /// The runtime's themes, created with the built-in dark theme applied.
    pub fn of(runtime: &Runtime) -> Rc<Themes> {
        runtime.extension(Themes::new)
    }
    fn new(runtime: &Runtime) -> Themes {
        let mut themes = HashMap::new();
        let mut order = Vec::new();
        for theme in builtins() {
            themes.insert(theme.name.clone(), theme.clone());
            order.push(theme.name.clone());
        }
        let mut declared = Vec::new();
        for (id, expr, source) in widget_defaults() {
            if declared.len() <= id.index() {
                declared.resize(id.index() + 1, None);
            }
            declared[id.index()] = Some((expr.clone(), source.clone()));
        }
        let shared = Rc::new(Shared {
            runtime: runtime.downgrade(),
            state: RefCell::new(State {
                themes,
                order,
                declared,
                current: DARK.into(),
                chosen: false,
                follow: None,
                appearance: Appearance::Dark,
                files: Vec::new(),
                reader: None,
                system: Vec::new(),
                text_scale: 1.,
            }),
            current: runtime.signal(DARK.into()),
        });
        let root = ThemeScope::new(shared.clone(), None, ThemeOverride::default());
        Themes { shared, root }
    }
    /// The root scope: the applied theme with nothing overridden.
    pub fn root(&self) -> &Rc<ThemeScope> {
        &self.root
    }
    fn refresh(&self) {
        self.root.batch(|| {
            let name = self.shared.state.borrow().current.clone();
            self.shared.current.set(name.as_str().into());
            self.root.refresh();
        });
    }
    /// Add a theme, or replace the one with its name. If it is applied (or
    /// something applied extends it), what it changes restyles.
    pub fn register(&self, theme: Theme) {
        {
            let mut state = self.shared.state.borrow_mut();
            if !state.themes.contains_key(&theme.name) {
                state.order.push(theme.name.clone());
            }
            state.themes.insert(theme.name.clone(), Arc::new(theme));
        }
        self.refresh();
    }
    /// The theme named `name`, if registered.
    pub fn theme(&self, name: &str) -> Option<Arc<Theme>> {
        self.shared.state.borrow().themes.get(name).cloned()
    }
    /// Every registered theme's name, built-ins first.
    pub fn names(&self) -> Vec<String> {
        self.shared.state.borrow().order.clone()
    }
    /// Apply the theme `name`. This also stops following the system.
    pub fn set(&self, name: &str) -> Result<(), ThemeError> {
        {
            let mut state = self.shared.state.borrow_mut();
            if !state.themes.contains_key(name) {
                return Err(ThemeError(format!("no theme named `{name}`")));
            }
            state.chosen = true;
            state.follow = None;
            if state.current == name {
                return Ok(());
            }
            state.current = name.into();
        }
        self.refresh();
        Ok(())
    }
    /// Apply `name` unless the app has chosen a theme (`set`) or follows the
    /// system: a default that a reload does not undo.
    pub fn set_default(&self, name: &str) -> Result<(), ThemeError> {
        {
            let mut state = self.shared.state.borrow_mut();
            if !state.themes.contains_key(name) {
                return Err(ThemeError(format!("no theme named `{name}`")));
            }
            if state.chosen || state.follow.is_some() || state.current == name {
                return Ok(());
            }
            state.current = name.into();
        }
        self.refresh();
        Ok(())
    }
    /// The applied theme's name. Read inside an effect, it subscribes to it.
    pub fn current(&self) -> Arc<str> {
        self.shared.current.get()
    }
    /// The applied theme's kind.
    pub fn kind(&self) -> ThemeKind {
        self.root.theme_kind()
    }
    /// Apply `dark` or `light` as the system's appearance changes (the
    /// desktop host reports it); `None` stops following.
    pub fn follow_system(&self, pair: Option<(&str, &str)>) {
        {
            let mut state = self.shared.state.borrow_mut();
            state.follow = pair.map(|(d, l)| (d.to_owned(), l.to_owned()));
        }
        self.apply_appearance();
    }
    /// Whether the theme follows the system's appearance.
    pub fn follows_system(&self) -> bool {
        self.shared.state.borrow().follow.is_some()
    }
    /// The system's appearance changed (the desktop host calls this).
    pub fn set_system_appearance(&self, appearance: Appearance) {
        self.shared.state.borrow_mut().appearance = appearance;
        self.apply_appearance();
    }
    pub fn system_appearance(&self) -> Appearance {
        self.shared.state.borrow().appearance
    }
    /// The system's text size, as a factor (the desktop host calls this):
    /// Windows' "Text size" (Settings, Accessibility), GNOME's
    /// `text-scaling-factor`. It scales the font size and line height tokens
    /// (`font.size.*`, `font.line.*`) whichever theme sets them; a size a
    /// view sets in points stays, as CSS's `px`.
    pub fn set_system_text_scale(&self, scale: f32) {
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 4.)
        } else {
            1.
        };
        {
            let mut state = self.shared.state.borrow_mut();
            if state.text_scale == scale {
                return;
            }
            state.text_scale = scale;
        }
        self.refresh();
    }
    pub fn system_text_scale(&self) -> f32 {
        self.shared.state.borrow().text_scale
    }
    /// Values the platform gives tokens (the desktop host calls this): the
    /// system's selection colours on macOS. They apply over the built-in
    /// themes, and under app themes and `theme_scope` overrides, which keep
    /// what they set. An empty list returns the tokens to the themes'.
    pub fn set_system_tokens(&self, tokens: impl IntoIterator<Item = (TokenId, TokenInput)>) {
        let system: Vec<(TokenId, Arc<Expr>)> = tokens
            .into_iter()
            .map(|(id, value)| (id, Arc::new(value.0)))
            .collect();
        {
            let mut state = self.shared.state.borrow_mut();
            if state.system == system {
                return;
            }
            state.system = system;
        }
        self.refresh();
    }
    fn apply_appearance(&self) {
        let changed = {
            let mut state = self.shared.state.borrow_mut();
            let Some((dark, light)) = state.follow.clone() else {
                return;
            };
            let name = match state.appearance {
                Appearance::Dark => dark,
                Appearance::Light => light,
            };
            if !state.themes.contains_key(&name) {
                warn(&format!(
                    "no theme named `{name}` to follow the system with"
                ));
                return;
            }
            let changed = state.current != name;
            state.current = name;
            changed
        };
        if changed {
            self.refresh();
        }
    }
    /// Declare an app token with its default, written in terms of other
    /// tokens (`"{surface.base}"`), so that every theme styles it; a theme
    /// may still set it by name. Declare before mounting what uses it.
    pub fn declare(&self, token: impl Into<TokenId>, default: impl Into<TokenInput>) {
        let id = token.into();
        let expr = default.into().0;
        self.declare_all([(id, expr)]);
    }
    /// Declare an app token from a theme expression.
    pub fn declare_expr(&self, token: impl Into<TokenId>, default: &str) -> Result<(), ThemeError> {
        let id = token.into();
        let input = TokenInput::parse(default)?;
        self.declare_all([(id, input.0)]);
        Ok(())
    }
    pub(crate) fn declare_all(&self, tokens: impl IntoIterator<Item = (TokenId, Expr)>) {
        let mut changed = false;
        {
            let mut state = self.shared.state.borrow_mut();
            for (id, expr) in tokens {
                if state.declared.len() <= id.index() {
                    state.declared.resize(id.index() + 1, None);
                }
                let slot = &mut state.declared[id.index()];
                if slot.as_ref().is_some_and(|(e, _)| **e == expr) {
                    continue;
                }
                *slot = Some((Arc::new(expr), "".into()));
                changed = true;
            }
        }
        if changed {
            self.refresh();
        }
    }
    /// Read a theme file and register it, keeping an eye on it for
    /// [`Themes::reload_if_changed`]. A zgui theme file, or a VS Code colour
    /// theme (its workbench colours mapped onto zgui's tokens). Returns the
    /// theme's name (the file's stem if it has none).
    pub fn load(&self, path: impl AsRef<Path>) -> Result<String, ThemeError> {
        let path = path.as_ref();
        let reader = self.shared.state.borrow().reader.clone();
        let theme = read_file(path, reader.as_deref())?;
        let name = theme.name.clone();
        let stamp = stamp(path);
        {
            let mut state = self.shared.state.borrow_mut();
            match state.files.iter_mut().find(|w| w.path == path) {
                Some(watched) => watched.stamp = stamp,
                None => state.files.push(Watched {
                    path: path.to_path_buf(),
                    stamp,
                }),
            }
        }
        self.register(theme);
        Ok(name)
    }
    /// Read again the theme files that changed since they were loaded;
    /// whether any did. What the new values change restyles in place. A file
    /// that no longer reads keeps its last good theme and reports the error.
    pub fn reload_if_changed(&self) -> Result<bool, ThemeError> {
        let changed: Vec<PathBuf> = {
            let state = self.shared.state.borrow();
            state
                .files
                .iter()
                .filter(|w| stamp(&w.path) != w.stamp)
                .map(|w| w.path.clone())
                .collect()
        };
        let mut error = None;
        for path in &changed {
            if let Err(e) = self.load(path) {
                // Do not report it again until it changes again.
                if let Some(w) = self
                    .shared
                    .state
                    .borrow_mut()
                    .files
                    .iter_mut()
                    .find(|w| &w.path == path)
                {
                    w.stamp = stamp(path);
                }
                error = Some(e);
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(!changed.is_empty()),
        }
    }
    /// Read theme files with `reader` instead of from the disk (an app's
    /// files compiled into it); `None` reads the disk again.
    pub fn set_reader(&self, reader: Option<ThemeReader>) {
        self.shared.state.borrow_mut().reader = reader;
    }
    /// The theme files loaded so far, watched for changes.
    pub fn files(&self) -> Vec<PathBuf> {
        self.shared
            .state
            .borrow()
            .files
            .iter()
            .map(|w| w.path.clone())
            .collect()
    }
    /// Poll the loaded theme files and reload the ones that change, as
    /// `ScriptHost::watch` does for scripts. Spawn it during development.
    pub async fn watch(self: Rc<Self>, every: std::time::Duration) {
        loop {
            crate::timer::sleep(every).await;
            if let Err(e) = self.reload_if_changed() {
                eprintln!("{e}");
            }
        }
    }
    /// The root scope's value of `token` (tracked), for drawing code.
    pub fn get(&self, token: impl Into<TokenId>) -> TokenValue {
        self.root.get(token)
    }
    pub fn color(&self, token: ColorToken) -> Color {
        self.root.color(token)
    }
    pub fn number(&self, token: NumberToken) -> f32 {
        self.root.number(token)
    }
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}
/// Reads a theme file's text, for [`Themes::set_reader`].
pub type ThemeReader = Rc<ReadText>;
type ReadText = dyn Fn(&Path) -> std::io::Result<String>;
/// A theme file: zgui's format, or a VS Code colour theme.
fn read_file(path: &Path, reader: Option<&ReadText>) -> Result<Theme, ThemeError> {
    let read = |path: &Path| match reader {
        Some(read) => read(path),
        None => std::fs::read_to_string(path),
    };
    let fail = |e: ThemeError| ThemeError(format!("{}: {e}", path.display()));
    let text = read(path).map_err(|e| ThemeError(format!("{}: {e}", path.display())))?;
    let root = super::json::parse(&text).map_err(|e| fail(ThemeError(e)))?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if super::vscode::is_vscode(&root) {
        super::vscode::from_vscode_value(&root, Some(&stem), &mut |include| {
            let included = path.parent().unwrap_or(Path::new(".")).join(include);
            let text =
                read(&included).map_err(|e| ThemeError(format!("{}: {e}", included.display())))?;
            super::json::parse(&text)
                .map_err(|e| ThemeError(format!("{}: {e}", included.display())))
        })
        .map_err(fail)
    } else {
        Theme::from_value(&root, Some(&stem)).map_err(fail)
    }
}
