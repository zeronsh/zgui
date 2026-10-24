//! The design lint: a style that hard-codes what a theme token is for (a
//! colour, spacing, a corner radius, a type size, weight or font, a shadow)
//! warns, or with `"error"` fails, naming the token to use. Off unless the
//! app turns it on (`ui.theme.strict`, `ScriptHost::design_lint`) or
//! `ZGX_DESIGN_LINT` asks.
//!
//! What it checks: literal styles (on elements, in `hover` and the other
//! interaction tables, in `ui.variants`) as they are read, and what a
//! function property returns, each time it runs. State values are the app's
//! data and are not checked. Theme definitions (`ui.theme.tokens`,
//! `ui.theme.register`, theme files) are where raw values belong. `0`,
//! `"auto"`, percentages and transparent colours are not design values;
//! sizes, borders' widths, opacity and transforms have no tokens.
//! `ui.raw(value, reason)` marks a deliberate exception.
//!
//! Cost: nothing when off. On, a literal is classified as it is read, and
//! only a hard-coded one looks for the nearest token.
use crate::node::{Lit, Prop};
use crate::value::Value;
use mlua::{Lua, UserData};
use std::fmt::Write;
use zgx::{layout::Length, reactive::Runtime, scene::Color, theme::TokenId};

/// How strictly styles must use theme tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesignLint {
    /// No check.
    #[default]
    Off,
    /// Each hard-coded value warns, once per call site (stderr and
    /// `ScriptHost::warnings`).
    Warn,
    /// Each hard-coded value is an error, shown over the window: the script
    /// fails to load, or the function returning it fails.
    Error,
}
impl DesignLint {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "off" | "false" | "0" | "" => Self::Off,
            "warn" | "true" | "1" => Self::Warn,
            "error" => Self::Error,
            _ => return None,
        })
    }
    /// `ZGX_DESIGN_LINT=off|warn|error`, which overrides the app's choice.
    pub(crate) fn from_env() -> Option<Self> {
        let value = zgx::env::var("ZGX_DESIGN_LINT").ok()?;
        Self::parse(value.trim()).or_else(|| {
            eprintln!("warning: ZGX_DESIGN_LINT is off, warn or error, not `{value}`");
            None
        })
    }
}

/// `ui.raw(value, reason)`: a value the lint lets through. It is the value
/// itself everywhere else (`value::convert` unwraps it).
pub(crate) struct RawUd(pub Value);
impl UserData for RawUd {}

pub(crate) fn is_raw(value: &mlua::Value) -> bool {
    matches!(value, mlua::Value::UserData(ud) if ud.is::<RawUd>())
}

/// What a property's literal hard-codes, if a token is meant for it.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Colour,
    Space,
    Radius,
    TextSize,
    Weight,
    Font,
    Shadow,
}
impl Kind {
    fn of(prop: Prop) -> Option<Self> {
        use Prop::*;
        Some(match prop {
            Bg | TextColor | BorderColor | OutlineColor | Tint => Self::Colour,
            P | Px | Py | Pt | Pr | Pb | Pl | M | Mx | My | Mt | Mr | Mb | Ml | Gap | GapX
            | GapY => Self::Space,
            Rounded | RoundedTl | RoundedTr | RoundedBr | RoundedBl => Self::Radius,
            TextSize => Self::TextSize,
            Weight => Self::Weight,
            Font => Self::Font,
            Shadow | InsetShadow => Self::Shadow,
            _ => return None,
        })
    }
    fn what(self) -> &'static str {
        match self {
            Self::Colour => "colour",
            Self::Space => "spacing",
            Self::Radius => "corner radius",
            Self::TextSize => "type size",
            Self::Weight => "font weight",
            Self::Font => "font",
            Self::Shadow => "shadow",
        }
    }
    /// The standard tokens' prefix for a number's scale.
    fn scale(self) -> Option<&'static str> {
        match self {
            Self::Space => Some("space."),
            Self::Radius => Some("radius."),
            Self::TextSize => Some("font.size."),
            Self::Weight => Some("font.weight."),
            _ => None,
        }
    }
}

/// A design value the literal hard-codes, if it is one: what it is.
fn hard_coded(prop: Prop, lit: &Lit) -> Option<Kind> {
    let kind = Kind::of(prop)?;
    let hard = match lit {
        Lit::Token(_) | Lit::Mix(..) => false,
        Lit::Color(Color(_, _, _, a)) => *a > 0,
        Lit::Num(n) => match kind {
            Kind::Space | Kind::Radius => *n != 0.,
            _ => true,
        },
        Lit::Len(Length::Px(n)) => *n != 0.,
        Lit::Len(_) => false,
        // `currentcolor` takes the inherited text colour.
        Lit::Str(_) => kind == Kind::Font,
        Lit::Shadows(list) => !list.is_empty(),
        _ => false,
    };
    hard.then_some(kind)
}

/// The mode the host's scripts are linted in.
pub(crate) fn mode(lua: &Lua) -> DesignLint {
    lua.app_data_ref::<crate::node::Env>()
        .map_or(DesignLint::Off, |env| env.warnings.borrow().design)
}

/// Checks a literal style as a script gives it (`name = lit`), at the
/// script's call site.
pub(crate) fn check(lua: &Lua, prop: Prop, name: &str, lit: &Lit) -> mlua::Result<()> {
    let mode = mode(lua);
    if mode == DesignLint::Off {
        return Ok(());
    }
    let Some(kind) = hard_coded(prop, lit) else {
        return Ok(());
    };
    let runtime = match lua.app_data_ref::<crate::node::Env>() {
        Some(env) => env.runtime.clone(),
        None => return Ok(()),
    };
    let message = message(&runtime, prop, name, kind, lit);
    match mode {
        DesignLint::Error => Err(mlua::Error::runtime(message)),
        _ => {
            crate::warn(lua, &message);
            Ok(())
        }
    }
}

/// Checks what a function property returned. Errors go to the window, as
/// the function's own would; the value is then not applied.
pub(crate) fn check_returned(
    lua: &Lua,
    function: &mlua::Function,
    prop: Prop,
    name: &str,
    lit: &Lit,
) -> Result<(), String> {
    let mode = mode(lua);
    if mode == DesignLint::Off {
        return Ok(());
    }
    let Some(kind) = hard_coded(prop, lit) else {
        return Ok(());
    };
    let Some(env) = lua.app_data_ref::<crate::node::Env>() else {
        return Ok(());
    };
    let info = function.info();
    let site = match (info.short_src, info.line_defined) {
        (Some(src), Some(line)) => format!("{src}:{line}: the function for `{name}` returns: "),
        _ => format!("the function for `{name}` returns: "),
    };
    let message = format!("{site}{}", message(&env.runtime, prop, name, kind, lit));
    match mode {
        DesignLint::Error => Err(message),
        _ => {
            crate::warning(Some(&env.warnings), message);
            Ok(())
        }
    }
}

/// The warning: what is hard-coded, the token to use instead, and the ways
/// out. Written for whoever (or whatever) edits the script to act on.
fn message(runtime: &Runtime, prop: Prop, name: &str, kind: Kind, lit: &Lit) -> String {
    let shown = shown(lit);
    let mut message = format!(
        "design lint: `{name} = {shown}` hard-codes a {}. Use a theme token, so it follows the \
         theme and stays on the design system's scale.",
        kind.what()
    );
    match nearest(runtime, prop, kind, lit) {
        Some((token, true, _)) => {
            let _ = write!(
                message,
                " \"${token}\" is exactly this in the current theme: `{name} = \"${token}\"`."
            );
        }
        Some((token, false, value)) => {
            let _ = write!(message, " The nearest is \"${token}\" ({value}).");
        }
        None if kind == Kind::Shadow => {
            message.push_str(
                " The standard shadows are \"$shadow.sm\", \"$shadow.md\" and \"$shadow.lg\".",
            );
        }
        None => {}
    }
    let _ = write!(
        message,
        " If no token fits, declare one for the app (`ui.theme.tokens {{ app = {{ … }} }}`) and use \
         that; for a deliberate exception, `{name} = ui.raw({shown}, \"why\")`."
    );
    message
}

/// A literal as a script would write it.
fn shown(lit: &Lit) -> String {
    match lit {
        Lit::Color(color) => format!("\"{}\"", zgx::theme::hex(*color)),
        Lit::Num(n) => format!("{n}"),
        Lit::Len(Length::Px(n)) => format!("{n}"),
        Lit::Str(s) => format!("\"{s}\""),
        Lit::Shadows(_) => "{ … }".to_owned(),
        other => format!("{other:?}"),
    }
}

/// The defined token nearest the literal: its name, whether it is exactly
/// the literal, and its value as shown. Of tokens as near, one whose role
/// fits the property (a surface for a background, not a border's colour).
fn nearest(runtime: &Runtime, prop: Prop, kind: Kind, lit: &Lit) -> Option<(String, bool, String)> {
    let themes = zgx::theme::Themes::of(runtime);
    let root = themes.root();
    let mut best: Option<((f32, u8), TokenId, String)> = None;
    let mut consider = |distance: f32, id: TokenId, value: String| {
        let rank = (distance, misfit(prop, id));
        if best.as_ref().is_none_or(|(r, _, _)| rank < *r) {
            best = Some((rank, id, value));
        }
    };
    for id in zgx::theme::interned() {
        if !root.is_defined(id) {
            continue;
        }
        let value = root.get_untracked(id);
        match (kind, lit) {
            (Kind::Colour, Lit::Color(want)) => {
                if let Some(have) = value.color() {
                    consider(colour_distance(*want, have), id, zgx::theme::hex(have));
                }
            }
            (Kind::Font, Lit::Str(want)) => {
                if let Some(have) = value.font()
                    && zgx::theme::family_name(&have).eq_ignore_ascii_case(want)
                {
                    consider(0., id, String::new());
                }
            }
            (_, Lit::Num(want) | Lit::Len(Length::Px(want))) => {
                let Some(prefix) = kind.scale() else { continue };
                if let Some(have) = value.number()
                    && id.name().starts_with(prefix)
                {
                    consider((have - want).abs(), id, format!("{have}"));
                }
            }
            _ => {}
        }
    }
    best.map(|((distance, _), id, value)| (id.name().to_string(), distance == 0., value))
}
/// How badly a colour token's role fits the property: 0 a fit, 1 an app's
/// own token (its role unknown), 2 a standard token for something else.
fn misfit(prop: Prop, id: TokenId) -> u8 {
    if zgx::theme::standard_kind(id).is_none() {
        return 1;
    }
    let name = id.name();
    let is = |prefixes: &[&str]| prefixes.iter().any(|p| name.starts_with(p));
    let fits = match prop {
        Prop::TextColor | Prop::Tint => {
            is(&["text.", "accent", "danger", "warning", "success", "info"])
        }
        Prop::BorderColor | Prop::OutlineColor => is(&["border", "focus.ring", "accent", "danger"]),
        Prop::Bg => !is(&[
            "text.",
            "border",
            "focus.ring",
            "scrollbar.",
            "font.",
            "shadow.",
        ]),
        _ => true,
    };
    if fits { 0 } else { 2 }
}

fn colour_distance(Color(r, g, b, a): Color, Color(r2, g2, b2, a2): Color) -> f32 {
    let d = |x: u8, y: u8| (f32::from(x) - f32::from(y)).powi(2);
    d(r, r2) + d(g, g2) + d(b, b2) + d(a, a2)
}

/// `ui.raw(value, reason)`.
pub(crate) fn raw(_: &Lua, (value, reason): (Value, Option<String>)) -> mlua::Result<RawUd> {
    if reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
        return Err(mlua::Error::runtime(
            "ui.raw(value, reason) needs a reason: say why this value is not a theme token",
        ));
    }
    Ok(RawUd(value))
}
