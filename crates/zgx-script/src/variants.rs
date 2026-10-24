//! `ui.variants`: a component's styles by variant, as shadcn's `cva`.
//!
//! ```lua
//! local button = ui.variants {
//!     base = { px = "$space.3", rounded = "$radius.md" },
//!     variants = {
//!         tone = { default = { bg = "$accent" }, ghost = { hover = { bg = "$hover" } } },
//!         size = { sm = { h = 28 }, md = { h = 36 } },
//!     },
//!     compound = { { tone = "ghost", size = "sm", px = "$space.2" } },
//!     defaults = { tone = "default", size = "md" },
//! }
//! ui.button { variants = button { tone = props.tone }, "Save" }
//! ```
//!
//! A definition's style tables are parsed once, when `ui.variants` runs (a
//! module runs once), into the literal styles elements hold. Choosing
//! variants returns a selection, which an element takes as `variants = …`:
//! chosen by name, it resolves as the element is built, into its literal
//! styles, costing what writing them out would; chosen by a state or a
//! function, it resolves in Rust each time that changes and restyles the
//! node in place, without calling the script.
//!
//! Layers apply in order: `base`, the axes by name, the `compound` rules
//! that match in their order, then the element's own properties. A property
//! replaces what it covers from earlier layers (`p` the `px` and `pt` before
//! it, `rounded` the corners), as `tailwind-merge` does for shadcn, so a
//! caller's `p = 0` takes all the padding a variant gave.
use crate::node::{self, Lit, Prop, Source};
use crate::value::Value;
use mlua::{Lua, MetaMethod, Table, UserData, UserDataMethods};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use zgx::reactive::Signal;

pub(crate) type Lits = Vec<(Prop, Lit)>;

/// Styles one layer sets: its own, and its hover, active, focus and
/// disabled styles.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Layer {
    pub props: Lits,
    pub interaction: [Lits; 4],
}
/// The interaction tables' names, in `Layer::interaction`'s order.
pub(crate) const INTERACTION: [&str; 4] = ["hover", "active", "focus", "disabled"];

struct Axis {
    name: Rc<str>,
    options: Vec<(Rc<str>, Layer)>,
    default: u16,
}
struct Compound {
    /// Each axis it names, with the options any of which matches.
    when: Vec<(usize, Vec<u16>)>,
    layer: Layer,
}
/// No option chosen on an axis.
const NONE: u16 = u16::MAX;

/// A parsed `ui.variants` definition.
pub(crate) struct Recipe {
    base: Layer,
    axes: Vec<Axis>,
    compound: Vec<Compound>,
    /// Every property some layer sets, outside interaction tables.
    props: Vec<Prop>,
    /// Whether some layer sets interaction styles.
    interacts: bool,
    /// What each choice resolves to, made the first time it is chosen.
    resolved: RefCell<HashMap<Box<[u16]>, Rc<Layer>>>,
}
impl Recipe {
    fn resolve(&self, key: &[u16]) -> Rc<Layer> {
        if let Some(layer) = self.resolved.borrow().get(key) {
            return layer.clone();
        }
        let mut out = self.base.clone();
        for (axis, &pick) in self.axes.iter().zip(key) {
            if let Some((_, layer)) = axis.options.get(usize::from(pick)) {
                over_layer(&mut out, layer);
            }
        }
        for rule in &self.compound {
            if rule
                .when
                .iter()
                .all(|(axis, options)| options.contains(&key[*axis]))
            {
                over_layer(&mut out, &rule.layer);
            }
        }
        let out = Rc::new(out);
        self.resolved.borrow_mut().insert(key.into(), out.clone());
        out
    }
    fn option(&self, axis: usize, value: &Value) -> Result<u16, String> {
        let axis = &self.axes[axis];
        let name: Rc<str> = match value {
            Value::Nil => return Ok(axis.default),
            Value::Str(s) => s.clone(),
            Value::Bool(b) => if *b { "true" } else { "false" }.into(),
            other => {
                return Err(format!(
                    "variant `{}` is chosen by name, not {other:?}",
                    axis.name
                ));
            }
        };
        match axis.options.iter().position(|(option, _)| *option == name) {
            Some(i) => Ok(i as u16),
            None => Err(format!(
                "variant `{}` has no option `{name}` (it has {})",
                axis.name,
                axis.options
                    .iter()
                    .map(|(o, _)| format!("`{o}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// Whether setting `a` replaces an earlier `b`: the same property, or one
/// `a` is a shorthand for (or, for corners, another spelling of).
pub(crate) fn covers(a: Prop, b: Prop) -> bool {
    use Prop::*;
    a == b
        || matches!(
            (a, b),
            (P, Px | Py | Pt | Pr | Pb | Pl)
                | (Px, Pl | Pr)
                | (Py, Pt | Pb)
                | (M, Mx | My | Mt | Mr | Mb | Ml)
                | (Mx, Ml | Mr)
                | (My, Mt | Mb)
                | (Gap, GapX | GapY)
                | (
                    Rounded | Corners,
                    Rounded | Corners | RoundedTl | RoundedTr | RoundedBr | RoundedBl
                )
                | (Border, BorderSides)
                | (Size, W | H | WFull | HFull)
                | (Overflow, OverflowX | OverflowY)
                | (Scale, ScaleX | ScaleY)
                | (Skew, SkewX | SkewY)
        )
}
/// `layer` laid over `styles`: what it covers goes, then it applies.
pub(crate) fn over(styles: &mut Lits, layer: &[(Prop, Lit)]) {
    if layer.is_empty() {
        return;
    }
    styles.retain(|(p, _)| !layer.iter().any(|(q, _)| covers(*q, *p)));
    styles.extend(layer.iter().cloned());
    // Shorthands apply before what refines them (as an element's own).
    styles.sort_by_key(|(prop, _)| node::specificity(*prop));
}
fn over_layer(styles: &mut Layer, layer: &Layer) {
    over(&mut styles.props, &layer.props);
    for (mine, theirs) in styles.interaction.iter_mut().zip(&layer.interaction) {
        over(mine, theirs);
    }
}

/// A style table: properties, and interaction tables of plain values.
fn layer(lua: &Lua, table: &Table, what: &str) -> mlua::Result<Layer> {
    let mut out = Layer::default();
    for pair in table.pairs::<String, mlua::Value>() {
        let (name, value) = pair?;
        add(lua, &mut out, &name, value, what)?;
    }
    sort(&mut out);
    Ok(out)
}
fn add(lua: &Lua, out: &mut Layer, name: &str, value: mlua::Value, what: &str) -> mlua::Result<()> {
    if let Some(slot) = INTERACTION.iter().position(|n| *n == name) {
        let mlua::Value::Table(styles) = value else {
            return Err(mlua::Error::runtime(format!(
                "`{what}.{name}` takes a table of styles"
            )));
        };
        let what = format!("{what}.{name}");
        for pair in styles.pairs::<String, mlua::Value>() {
            let (name, value) = pair?;
            out.interaction[slot].push(plain(lua, &name, value, &what)?);
        }
        return Ok(());
    }
    out.props.push(plain(lua, name, value, what)?);
    Ok(())
}
fn plain(lua: &Lua, name: &str, value: mlua::Value, what: &str) -> mlua::Result<(Prop, Lit)> {
    let prop = Prop::parse(name)
        .ok_or_else(|| mlua::Error::runtime(format!("unknown style `{name}` in `{what}`")))?;
    match node::source(lua, prop.ty(), name, value)? {
        Source::Lit(lit) => Ok((prop, lit)),
        _ => Err(mlua::Error::runtime(format!(
            "`{what}.{name}` must be a plain value: a variant is chosen by a state, its styles are fixed"
        ))),
    }
}
fn sort(layer: &mut Layer) {
    layer
        .props
        .sort_by_key(|(prop, _)| node::specificity(*prop));
    for styles in &mut layer.interaction {
        styles.sort_by_key(|(prop, _)| node::specificity(*prop));
    }
}
/// An option's name: a string, or `true` / `false` for an on-off axis.
fn option_name(key: mlua::Value, axis: &str) -> mlua::Result<Rc<str>> {
    match key {
        mlua::Value::String(s) => Ok(s.to_str()?.as_ref().into()),
        mlua::Value::Boolean(b) => Ok(if b { "true" } else { "false" }.into()),
        other => Err(mlua::Error::runtime(format!(
            "variant `{axis}`'s options are named by strings (or true and false), not {}",
            other.type_name()
        ))),
    }
}

/// `ui.variants(definition)`.
pub(crate) fn define(lua: &Lua, definition: Table) -> mlua::Result<RecipeUd> {
    for key in definition.pairs::<mlua::Value, mlua::Value>() {
        let (key, _) = key?;
        let name = key.to_string().unwrap_or_default();
        if !matches!(&*name, "base" | "variants" | "compound" | "defaults") {
            return Err(mlua::Error::runtime(format!(
                "ui.variants takes base, variants, compound and defaults, not `{name}`"
            )));
        }
    }
    let base = match definition.get::<Option<Table>>("base")? {
        Some(t) => layer(lua, &t, "base")?,
        None => Layer::default(),
    };
    let mut axes = Vec::new();
    if let Some(variants) = definition.get::<Option<Table>>("variants")? {
        for pair in variants.pairs::<String, Table>() {
            let (name, options) = pair?;
            let mut list = Vec::new();
            for pair in options.pairs::<mlua::Value, Table>() {
                let (key, styles) = pair?;
                let option = option_name(key, &name)?;
                let styles = layer(lua, &styles, &format!("variants.{name}.{option}"))?;
                list.push((option, styles));
            }
            list.sort_by(|a, b| a.0.cmp(&b.0));
            axes.push(Axis {
                name: name.into(),
                options: list,
                default: NONE,
            });
        }
    }
    // Table order is arbitrary: axes apply in name order.
    axes.sort_by(|a, b| a.name.cmp(&b.name));
    let mut recipe = Recipe {
        base,
        axes,
        compound: Vec::new(),
        props: Vec::new(),
        interacts: false,
        resolved: RefCell::default(),
    };
    if let Some(defaults) = definition.get::<Option<Table>>("defaults")? {
        for pair in defaults.pairs::<String, Value>() {
            let (name, value) = pair?;
            let axis = axis_named(&recipe, &name, "defaults")?;
            recipe.axes[axis].default =
                recipe.option(axis, &value).map_err(mlua::Error::runtime)?;
        }
    }
    if let Some(rules) = definition.get::<Option<Table>>("compound")? {
        for (i, rule) in rules.sequence_values::<Table>().enumerate() {
            let rule = rule?;
            let what = format!("compound[{}]", i + 1);
            let mut compound = Compound {
                when: Vec::new(),
                layer: Layer::default(),
            };
            for pair in rule.pairs::<String, mlua::Value>() {
                let (name, value) = pair?;
                match recipe.axes.iter().position(|a| *a.name == *name) {
                    Some(axis) => {
                        let values = match lua.unpack::<Value>(value)? {
                            Value::List(list) => list.to_vec(),
                            one => vec![one],
                        };
                        let options = values
                            .iter()
                            .map(|v| recipe.option(axis, v))
                            .collect::<Result<_, _>>()
                            .map_err(|e| mlua::Error::runtime(format!("`{what}`: {e}")))?;
                        compound.when.push((axis, options));
                    }
                    None => add(lua, &mut compound.layer, &name, value, &what)?,
                }
            }
            if compound.when.is_empty() {
                return Err(mlua::Error::runtime(format!(
                    "`{what}` names no variant: it would always apply (put its styles in `base`)"
                )));
            }
            sort(&mut compound.layer);
            recipe.compound.push(compound);
        }
    }
    let layers = std::iter::once(&recipe.base)
        .chain(
            recipe
                .axes
                .iter()
                .flat_map(|a| a.options.iter().map(|(_, l)| l)),
        )
        .chain(recipe.compound.iter().map(|c| &c.layer));
    for layer in layers {
        for (prop, _) in &layer.props {
            if !recipe.props.contains(prop) {
                recipe.props.push(*prop);
            }
        }
        recipe.interacts |= layer.interaction.iter().any(|s| !s.is_empty());
    }
    warn_overlaps(lua, &recipe);
    Ok(RecipeUd(Rc::new(recipe)))
}
fn axis_named(recipe: &Recipe, name: &str, what: &str) -> mlua::Result<usize> {
    recipe
        .axes
        .iter()
        .position(|a| *a.name == *name)
        .ok_or_else(|| mlua::Error::runtime(format!("`{what}` names no variant `{name}`")))
}
/// Two axes setting the same property: which wins is their names' order,
/// probably not what was meant.
fn warn_overlaps(lua: &Lua, recipe: &Recipe) {
    let sets = |axis: &Axis| -> Vec<Prop> {
        let mut props: Vec<Prop> = axis
            .options
            .iter()
            .flat_map(|(_, l)| l.props.iter().map(|(p, _)| *p))
            .collect();
        props.dedup();
        props
    };
    for (i, a) in recipe.axes.iter().enumerate() {
        for b in &recipe.axes[i + 1..] {
            let (pa, pb) = (sets(a), sets(b));
            let shared = pa
                .iter()
                .find(|p| pb.iter().any(|q| covers(**p, *q) || covers(*q, **p)));
            if let Some(prop) = shared {
                crate::warn(
                    lua,
                    &format!(
                        "ui.variants: variants `{}` and `{}` both set `{}`; they apply in name order, \
                         so `{}` wins. Set it in one of them, or decide in a compound rule.",
                        a.name,
                        b.name,
                        prop.name(),
                        b.name
                    ),
                );
            }
        }
    }
}

/// What `ui.variants` returns: call it with the variants to choose.
pub(crate) struct RecipeUd(pub Rc<Recipe>);
impl UserData for RecipeUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Call, |lua, this, chosen: Option<Table>| {
            select(lua, &this.0, chosen.as_ref()).map(|s| SelectionUd(Rc::new(s)))
        });
    }
}

/// How an axis is chosen.
enum Pick {
    Fixed(u16),
    State(Signal<Value>),
}
/// Variants chosen for an element (`variants = button { … }`).
pub(crate) struct Selection {
    recipe: Rc<Recipe>,
    picks: Vec<Pick>,
    warnings: crate::SharedWarnings,
}
pub(crate) struct SelectionUd(pub Rc<Selection>);
impl UserData for SelectionUd {}

/// `variants = button`: every axis its default.
pub(crate) fn select_defaults(lua: &Lua, recipe: &Rc<Recipe>) -> mlua::Result<Selection> {
    select(lua, recipe, None)
}
/// The choice: each axis by name, from a table that may hold more (a
/// component's whole `props`); an axis not given takes its default.
fn select(lua: &Lua, recipe: &Rc<Recipe>, chosen: Option<&Table>) -> mlua::Result<Selection> {
    let env = node::env(lua)?;
    let mut picks = Vec::with_capacity(recipe.axes.len());
    for (i, axis) in recipe.axes.iter().enumerate() {
        let value = match chosen {
            Some(t) => t.get::<mlua::Value>(&*axis.name)?,
            None => mlua::Value::Nil,
        };
        let pick = match value {
            mlua::Value::UserData(ud) if ud.is::<crate::value::State>() => {
                Pick::State(ud.borrow::<crate::value::State>()?.0.clone())
            }
            // Its value kept in Rust, so styles read it without calling it.
            mlua::Value::Function(f) => Pick::State(node::computed(lua, &env, f)),
            other => {
                let value = lua.unpack::<Value>(other)?;
                Pick::Fixed(recipe.option(i, &value).map_err(mlua::Error::runtime)?)
            }
        };
        picks.push(pick);
    }
    Ok(Selection {
        recipe: recipe.clone(),
        picks,
        warnings: env.warnings.clone(),
    })
}
impl Selection {
    /// Whether nothing chooses it later: it resolves once, as it is built.
    pub(crate) fn fixed(&self) -> bool {
        self.picks.iter().all(|p| matches!(p, Pick::Fixed(_)))
    }
    pub(crate) fn interacts(&self) -> bool {
        self.recipe.interacts
    }
    /// The choice now, reading (and so following) the states choosing it.
    pub(crate) fn key(&self) -> Box<[u16]> {
        self.picks
            .iter()
            .enumerate()
            .map(|(axis, pick)| match pick {
                Pick::Fixed(i) => *i,
                Pick::State(state) => {
                    state
                        .with(|v| self.recipe.option(axis, v))
                        .unwrap_or_else(|e| {
                            crate::warning(
                                Some(&self.warnings),
                                format!("ui.variants: {e}; using its default"),
                            );
                            self.recipe.axes[axis].default
                        })
                }
            })
            .collect()
    }
    /// The styles chosen now.
    pub(crate) fn current(&self) -> Rc<Layer> {
        self.recipe.resolve(&self.key())
    }
    pub(crate) fn layer(&self, key: &[u16]) -> Rc<Layer> {
        self.recipe.resolve(key)
    }
}

/// An element's `variants = …`, after its own properties are read: what the
/// selection sets and the element does not.
pub(crate) fn apply(node: &mut node::Node, selection: Rc<Selection>) {
    let own: Vec<Prop> = node.props.iter().map(|(p, _)| *p).collect();
    let kept = |p: Prop| !own.iter().any(|q| covers(*q, p));
    if selection.fixed() {
        let layer = selection.current();
        node.props.extend(
            layer
                .props
                .iter()
                .filter(|(p, _)| kept(*p))
                .map(|(p, l)| (*p, Source::Lit(l.clone()))),
        );
        // Interaction styles: the selection's, the element's own over them.
        let mut own = node::interaction_of(&node.pseudo);
        for (slot, styles) in own.iter_mut().enumerate() {
            let mut merged = layer.interaction[slot].clone();
            over(&mut merged, styles);
            *styles = merged;
        }
        node.pseudo = node::pseudo_of(own);
        return;
    }
    for &prop in &selection.recipe.props {
        if kept(prop) {
            node.props
                .push((prop, Source::Variant(selection.clone(), prop)));
        }
    }
    if selection.interacts() {
        node.variant_interaction = Some(selection);
    }
}
