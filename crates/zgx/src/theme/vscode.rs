//! VS Code colour themes as zgx themes.
//!
//! A VS Code theme names workbench colours (`editor.background`,
//! `focusBorder`, `list.activeSelectionBackground`, …). Each zgx token
//! takes the first of a short list of them the theme defines; a few are
//! derived when it defines none (secondary text is the foreground at 70%).
//! What a theme does not cover comes from the built-in zgx theme of its
//! type. Syntax colours (`tokenColors`) have no zgx counterpart and are
//! ignored.
use super::file::{Theme, ThemeError, ThemeKind};
use super::json::{self, Json};
use super::value::parse_color;

/// zgx token, the VS Code colours it reads (first found wins), and what it
/// derives to when the theme sets none of them.
const MAP: &[(&str, &[&str], Option<&str>)] = &[
    ("background", &["editor.background"], None),
    (
        "surface.base",
        &["sideBar.background", "editor.background"],
        None,
    ),
    (
        "surface.sunken",
        &[
            "panel.background",
            "terminal.background",
            "sideBar.background",
        ],
        None,
    ),
    (
        "surface.raised",
        &[
            "dropdown.background",
            "editorWidget.background",
            "button.secondaryBackground",
        ],
        None,
    ),
    (
        "surface.overlay",
        &[
            "menu.background",
            "editorSuggestWidget.background",
            "editorWidget.background",
            "quickInput.background",
        ],
        None,
    ),
    ("text.primary", &["editor.foreground", "foreground"], None),
    (
        "text.secondary",
        &["descriptionForeground"],
        Some("alpha({text.primary}, 0.7)"),
    ),
    (
        "text.tertiary",
        &["editorLineNumber.foreground"],
        Some("alpha({text.primary}, 0.5)"),
    ),
    (
        "text.disabled",
        &["disabledForeground"],
        Some("alpha({text.primary}, 0.35)"),
    ),
    (
        "border",
        &[
            "panel.border",
            "widget.border",
            "editorGroup.border",
            "contrastBorder",
        ],
        None,
    ),
    (
        "border.subtle",
        &[
            "editorGroup.border",
            "sideBar.border",
            "tab.border",
            "panel.border",
        ],
        None,
    ),
    (
        "border.strong",
        &["input.border", "dropdown.border", "contrastBorder"],
        None,
    ),
    (
        "accent",
        &[
            "button.background",
            "textLink.foreground",
            "focusBorder",
            "activityBarBadge.background",
        ],
        None,
    ),
    ("accent.hover", &["button.hoverBackground"], None),
    (
        "accent.text",
        &["button.foreground", "activityBarBadge.foreground"],
        None,
    ),
    (
        "danger",
        &[
            "errorForeground",
            "editorError.foreground",
            "terminal.ansiRed",
        ],
        None,
    ),
    (
        "warning",
        &["editorWarning.foreground", "terminal.ansiYellow"],
        None,
    ),
    (
        "success",
        &[
            "testing.iconPassed",
            "gitDecoration.addedResourceForeground",
            "terminal.ansiGreen",
        ],
        None,
    ),
    (
        "info",
        &["editorInfo.foreground", "terminal.ansiBlue"],
        None,
    ),
    (
        "selection",
        &["editor.selectionBackground", "selection.background"],
        None,
    ),
    (
        "selection.item",
        &[
            "list.activeSelectionBackground",
            "list.inactiveSelectionBackground",
        ],
        None,
    ),
    ("focus.ring", &["focusBorder"], None),
    (
        "hover",
        &["list.hoverBackground", "toolbar.hoverBackground"],
        None,
    ),
    ("pressed", &["toolbar.activeBackground"], None),
    ("scrollbar.thumb", &["scrollbarSlider.background"], None),
    (
        "scrollbar.thumb.active",
        &[
            "scrollbarSlider.activeBackground",
            "scrollbarSlider.hoverBackground",
        ],
        None,
    ),
    ("scrollbar.track", &[], Some("transparent")),
    ("glass.tint", &[], Some("alpha({background}, 0.75)")),
    ("button.bg", &["button.secondaryBackground"], None),
    ("button.hover", &["button.secondaryHoverBackground"], None),
    ("input.bg", &["input.background"], None),
    ("menu.bg", &["menu.background"], None),
    (
        "menu.hover",
        &["menu.selectionBackground", "list.hoverBackground"],
        None,
    ),
];

/// Whether a parsed file is a VS Code colour theme rather than a zgx one.
pub(crate) fn is_vscode(root: &Json) -> bool {
    root.get("tokens").is_none()
        && (root.get("colors").is_some()
            || root.get("tokenColors").is_some()
            || root.get("include").is_some())
}

impl Theme {
    /// A VS Code colour theme's workbench colours as a zgx theme. Its
    /// `include` (a base theme file) is not followed: use `Themes::load` for
    /// a file, or [`Theme::from_vscode_with`].
    pub fn from_vscode(text: &str) -> Result<Theme, ThemeError> {
        Self::from_vscode_with(text, |include| {
            Err(ThemeError(format!(
                "`include: {include}` needs Themes::load or Theme::from_vscode_with"
            )))
        })
    }
    /// As `from_vscode`, reading an `include` with `read` (given the path
    /// as written, relative to the theme file).
    pub fn from_vscode_with(
        text: &str,
        mut read: impl FnMut(&str) -> Result<String, ThemeError>,
    ) -> Result<Theme, ThemeError> {
        let root = json::parse(text).map_err(ThemeError)?;
        from_vscode_value(&root, None, &mut |include| {
            json::parse(&read(include)?).map_err(ThemeError)
        })
    }
}

pub(crate) fn from_vscode_value(
    root: &Json,
    fallback: Option<&str>,
    include: &mut dyn FnMut(&str) -> Result<Json, ThemeError>,
) -> Result<Theme, ThemeError> {
    let mut colors: Vec<(String, String)> = Vec::new();
    let mut kind = None;
    collect(root, include, &mut colors, &mut kind, 0)?;
    let name = match root.get("name").and_then(Json::as_str) {
        Some(name) => name.to_owned(),
        None => fallback
            .map(str::to_owned)
            .ok_or_else(|| ThemeError("a theme needs a `name`".into()))?,
    };
    // A colour a theme clears (`#00000000`) does not stand for a surface
    // or a line: the next candidate, or the base theme, does.
    let find = |key: &str| {
        colors
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| parse_color(v))
            .filter(|c| c.3 > 0)
    };
    let kind = kind.unwrap_or_else(|| {
        // No `type`: dark if the editor is.
        match find("editor.background") {
            Some(c) if (c.0 as u32 * 299 + c.1 as u32 * 587 + c.2 as u32 * 114) / 1000 > 128 => {
                ThemeKind::Light
            }
            _ => ThemeKind::Dark,
        }
    });
    let mut theme = Theme::new(name, kind);
    for (token, keys, derived) in MAP {
        let found = keys.iter().find_map(|key| find(key));
        theme = match (found, derived) {
            (Some(color), _) => theme.with(super::tokens::token(token), color),
            (None, Some(expr)) => theme.with_expr(super::tokens::token(token), expr)?,
            (None, None) => theme,
        };
    }
    Ok(theme)
}

/// The theme's colours after those of what it includes, and its type.
fn collect(
    root: &Json,
    include: &mut dyn FnMut(&str) -> Result<Json, ThemeError>,
    colors: &mut Vec<(String, String)>,
    kind: &mut Option<ThemeKind>,
    depth: usize,
) -> Result<(), ThemeError> {
    if depth > 8 {
        return Err(ThemeError("`include`s nest too deeply".into()));
    }
    if let Some(path) = root.get("include").and_then(Json::as_str) {
        let base = include(path)?;
        collect(&base, include, colors, kind, depth + 1)?;
    }
    if let Some(t) = root.get("type").and_then(Json::as_str) {
        *kind = Some(
            ThemeKind::parse(t).ok_or_else(|| ThemeError(format!("unknown theme type `{t}`")))?,
        );
    }
    if let Some(Json::Object(members)) = root.get("colors") {
        for (key, value) in members {
            if let Json::String(value) = value {
                colors.push((key.clone(), value.clone()));
            }
        }
    }
    Ok(())
}
