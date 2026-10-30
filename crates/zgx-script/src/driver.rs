//! Driving a running interface by what it shows: one vocabulary for tests
//! ([`crate::testing::TestApp`]) and for agents (`zgx inspect`, its socket
//! and MCP server). A [target](parse) names a node; the actions press,
//! type and key at it; [`tree`] and [`node`] describe what is there,
//! compactly and in a stable order.
//!
//! Targets:
//! - `#send`: the node with that id (a script's `id = "send"`);
//! - `"Send"`: the node labelled or reading so (its accessible name, or its
//!   text), the outermost of a labelled node and its own text; `~"Sen"` the
//!   same with that anywhere in it;
//! - `role:button`: by role, ARIA's names (`button`, `textbox`, `option`…);
//! - `@12`: a node by its number, as [`tree`] shows it (stable while the
//!   node lives);
//! - `hook:toolbar.save`: an element the app declared a hook (`hook =
//!   "toolbar.save"`), for plugins (`ui.contribute { target }`);
//! - parts together narrow (`role:button "Send"`), `>>` looks inside what
//!   is before it (`#sidebar >> "Inbox"`), `row 3` is a list's third row
//!   (`#messages row 3`), and `[2]` picks the second of several matches.
//!
//! A target matching more than one node is an error naming them: an
//! action never picks one silently. Nodes hidden from layout (`hidden`, or
//! inside something hidden) match too, but where exactly one match shows,
//! the target names it: a person can reach only that one. A modal's,
//! popover's or menu's panel is under the node that owns it, wherever the
//! scene puts it.
use std::{collections::HashMap, fmt::Write as _};
use zgx::{
    compose::ViewHandle,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{NodeId, NodeKind, Rect, Scene},
    semantics::Role,
    widgets::Ui,
};

/// A node's role as ARIA names it, its checkable menu items told apart.
pub fn semantic_role_name(semantic: &zgx::semantics::SemanticNode) -> &'static str {
    match (semantic.role, semantic.checked, semantic.radio) {
        (Role::MenuItem, Some(_), true) => "menuitemradio",
        (Role::MenuItem, Some(_), false) => "menuitemcheckbox",
        (role, ..) => role_name(role),
    }
}
/// What a node is to assistive technology, as ARIA names it.
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Window => "window",
        Role::Group => "group",
        Role::Label => "text",
        Role::Button => "button",
        Role::Link => "link",
        Role::CheckBox => "checkbox",
        Role::Slider => "slider",
        Role::TextInput => "textbox",
        Role::MultilineTextInput => "textbox",
        Role::Progress => "progressbar",
        Role::ScrollView => "scroll",
        Role::ScrollBar => "scrollbar",
        Role::Splitter => "separator",
        Role::ListItem => "listitem",
        Role::Dialog => "dialog",
        Role::Menu => "menu",
        Role::MenuItem => "menuitem",
        Role::Image => "img",
        Role::Document => "document",
        Role::Article => "article",
        Role::Heading => "heading",
        Role::List => "list",
        Role::ListBox => "listbox",
        Role::ListBoxOption => "option",
        Role::Toolbar => "toolbar",
        Role::Status => "status",
        Role::TabList => "tablist",
        Role::Tab => "tab",
        Role::TabPanel => "tabpanel",
        Role::RadioGroup => "radiogroup",
        Role::RadioButton => "radio",
        Role::Switch => "switch",
        Role::SpinButton => "spinbutton",
        Role::ComboBox => "combobox",
        Role::Tree => "tree",
        Role::TreeItem => "treeitem",
        Role::Grid => "grid",
        Role::Row => "row",
        Role::Cell => "cell",
        Role::ColumnHeader => "columnheader",
        Role::Alert => "alert",
    }
}

/// One part of a target: what a node must be.
#[derive(Clone, Debug, Default, PartialEq)]
struct Part {
    id: Option<String>,
    hook: Option<String>,
    text: Option<String>,
    /// `~"part"`: what its label or text holds, anywhere in it.
    holding: Option<String>,
    role: Option<String>,
    number: Option<u32>,
    row: Option<usize>,
    nth: Option<usize>,
}
/// A target: parts, each looked for inside the one before.
#[derive(Clone, Debug, PartialEq)]
pub struct Target(Vec<Part>);

/// Reads a target (see the module's docs).
pub fn parse(text: &str) -> Result<Target, String> {
    let mut parts = Vec::new();
    for piece in text.split(">>") {
        let mut part = Part::default();
        let mut rest = piece.trim();
        if rest.is_empty() {
            return Err(format!("`{text}`: an empty part around `>>`"));
        }
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('"') {
                let end = after
                    .find('"')
                    .ok_or_else(|| format!("`{text}`: an unclosed quote"))?;
                part.text = Some(after[..end].to_string());
                rest = after[end + 1..].trim_start();
            } else if let Some(after) = rest.strip_prefix("~\"") {
                let end = after
                    .find('"')
                    .ok_or_else(|| format!("`{text}`: an unclosed quote"))?;
                part.holding = Some(after[..end].to_string());
                rest = after[end + 1..].trim_start();
            } else if let Some(after) = rest.strip_prefix('#') {
                let end = after
                    .find(|c: char| c.is_whitespace() || c == '[')
                    .unwrap_or(after.len());
                part.id = Some(after[..end].to_string());
                rest = after[end..].trim_start();
            } else if let Some(after) = rest.strip_prefix('@') {
                let end = after
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(after.len());
                part.number = Some(
                    after[..end]
                        .parse()
                        .map_err(|_| format!("`{text}`: `@` takes a number"))?,
                );
                rest = after[end..].trim_start();
            } else if let Some(after) = rest.strip_prefix("hook:") {
                let end = after
                    .find(|c: char| c.is_whitespace() || c == '[')
                    .unwrap_or(after.len());
                part.hook = Some(after[..end].to_string());
                rest = after[end..].trim_start();
            } else if let Some(after) = rest.strip_prefix("role:") {
                let end = after
                    .find(|c: char| c.is_whitespace() || c == '[')
                    .unwrap_or(after.len());
                part.role = Some(after[..end].to_string());
                rest = after[end..].trim_start();
            } else if let Some(after) = rest.strip_prefix("row ") {
                let after = after.trim_start();
                let end = after
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(after.len());
                part.row = Some(
                    after[..end]
                        .parse()
                        .map_err(|_| format!("`{text}`: `row` takes a number"))?,
                );
                rest = after[end..].trim_start();
            } else if let Some(after) = rest.strip_prefix('[') {
                let end = after
                    .find(']')
                    .ok_or_else(|| format!("`{text}`: an unclosed `[`"))?;
                part.nth = Some(
                    after[..end]
                        .trim()
                        .parse()
                        .map_err(|_| format!("`{text}`: `[n]` takes a number"))?,
                );
                rest = after[end + 1..].trim_start();
            } else {
                // A bare word is an id, as tests name nodes (`click("send")`).
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '[')
                    .unwrap_or(rest.len());
                part.id = Some(rest[..end].to_string());
                rest = rest[end..].trim_start();
            }
        }
        parts.push(part);
    }
    Ok(Target(parts))
}

/// What a plugin's contribution targets (`ui.contribute { target }`): a
/// target's static parts, matched against an element as the interface is
/// built, not against the scene. An id, a declared hook, a role, a name
/// (its label, or its text as written); together they narrow.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Static {
    pub id: Option<String>,
    pub hook: Option<String>,
    pub role: Option<String>,
    pub text: Option<String>,
}
impl Static {
    /// `text` read as a target, of the parts known before the interface
    /// shows: not a node's number, a row, the nth match or `>>`.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let Target(parts) = parse(text)?;
        let [part] = parts.as_slice() else {
            return Err(format!(
                "`{text}`: a plugin's target is one element, without `>>`"
            ));
        };
        if part.number.is_some()
            || part.row.is_some()
            || part.nth.is_some()
            || part.holding.is_some()
        {
            return Err(format!(
                "`{text}`: a plugin's target is matched as the interface is built: \
                 an id, `hook:`, `role:` or a name, not `@n`, `row` or `[n]`"
            ));
        }
        Ok(Self {
            id: part.id.clone(),
            hook: part.hook.clone(),
            role: part.role.clone(),
            text: part.text.clone(),
        })
    }
}

/// A node's number in targets and trees: its slot in the scene.
pub fn number(node: NodeId) -> u32 {
    node.number()
}

/// The text a node shows itself (a text's, a paragraph's), if any.
pub fn own_text(scene: &Scene, node: NodeId) -> Option<String> {
    match scene.kind(node) {
        NodeKind::Text { text, .. } => Some(text.to_string()),
        NodeKind::RichText { text, .. } => Some(text.text().to_owned()),
        _ => None,
    }
}
/// The first text inside `node` (itself included), depth first.
pub fn first_text(scene: &Scene, node: NodeId) -> Option<String> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if let Some(text) = own_text(scene, n) {
            return Some(text);
        }
        stack.extend(scene.children(n).iter().rev());
    }
    None
}

/// The rows of list `list` that are mounted, top to bottom.
pub fn rows(scene: &Scene, list: NodeId) -> Vec<NodeId> {
    // A list holds a viewport, which holds the content, which holds the rows.
    let Some(content) = scene
        .children(list)
        .first()
        .and_then(|viewport| scene.children(*viewport).first().copied())
    else {
        return Vec::new();
    };
    let mut rows: Vec<NodeId> = scene.children(content).to_vec();
    rows.sort_by(|a, b| scene.bounds(*a).y.total_cmp(&scene.bounds(*b).y));
    rows
}

/// The tree as the view was written: a portal's overlay (a modal's,
/// popover's or menu's panel, `Ui::portals`) under the node that owns it,
/// after its children, rather than where the scene puts it.
struct Logical<'a> {
    scene: &'a Scene,
    /// Overlays by their owners.
    owned: HashMap<NodeId, Vec<NodeId>>,
    /// Owners by their overlays.
    owners: HashMap<NodeId, NodeId>,
}
impl<'a> Logical<'a> {
    fn new(ui: &Ui, scene: &'a Scene) -> Self {
        let mut owned: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut owners = HashMap::new();
        for (owner, overlay) in ui.portals() {
            if scene.contains(owner) && scene.contains(overlay) {
                owned.entry(owner).or_default().push(overlay);
                owners.insert(overlay, owner);
            }
        }
        Self {
            scene,
            owned,
            owners,
        }
    }
    fn children(&self, node: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + '_ {
        self.scene
            .children(node)
            .iter()
            .copied()
            .filter(|child| !self.owners.contains_key(child))
            .chain(self.owned.get(&node).into_iter().flatten().copied())
    }
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.owners
            .get(&node)
            .copied()
            .or_else(|| self.scene.parent(node))
    }
    /// Every node under `from` in document order, itself first.
    fn under(&self, from: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![from];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children(n).rev());
        }
        out
    }
}

fn describe(ui: &Ui, node: NodeId) -> String {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = format!("@{}", number(node));
    if let Some(s) = semantics.get(node) {
        let _ = write!(out, " {}", semantic_role_name(s));
        if !s.label.is_empty() {
            let _ = write!(out, " {:?}", clip(&s.label, 40));
        }
    } else if let Some(text) = own_text(&scene, node) {
        let _ = write!(out, " {:?}", clip(&text, 40));
    }
    let b = scene.bounds(node);
    let _ = write!(out, " {}", rect(b));
    if !scene.layout_visible(node) {
        out.push_str(" hidden");
    }
    out
}

/// The nodes `target` names: all of them (an action wants one).
pub fn find_all(ui: &Ui, handle: &ViewHandle, target: &Target) -> Result<Vec<NodeId>, String> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let logical = Logical::new(ui, &scene);
    let mut scopes = vec![handle.node()];
    for (i, part) in target.0.iter().enumerate() {
        let mut found: Vec<NodeId> = Vec::new();
        for scope in &scopes {
            // The first part looks in the whole view, the rest inside.
            let candidates = logical.under(*scope);
            let candidates = if i == 0 {
                candidates
            } else {
                candidates[1..].to_vec()
            };
            if let Some(hook) = &part.hook {
                return Err(format!(
                    "`hook:{hook}` names a plugin hook; target the element by its id or name"
                ));
            }
            let ids = part.id.as_ref().map(|id| handle.find_all(id));
            for node in candidates {
                if let Some(ids) = &ids
                    && !ids.contains(&node)
                {
                    continue;
                }
                if let Some(n) = part.number
                    && number(node) != n
                {
                    continue;
                }
                if let Some(role) = &part.role
                    && !semantics.get(node).is_some_and(|s| {
                        // `role:menuitem` finds them all, as ARIA's roles
                        // are kinds of it.
                        role_name(s.role) == role || semantic_role_name(s) == role
                    })
                {
                    continue;
                }
                if let Some(text) = &part.text {
                    let labelled = semantics.get(node).is_some_and(|s| &s.label == text);
                    let reads = own_text(&scene, node).is_some_and(|t| &t == text);
                    if !labelled && !reads {
                        continue;
                    }
                }
                if let Some(part) = &part.holding {
                    let labelled = semantics.get(node).is_some_and(|s| s.label.contains(part));
                    let reads = own_text(&scene, node).is_some_and(|t| t.contains(part));
                    if !labelled && !reads {
                        continue;
                    }
                }
                if !found.contains(&node) {
                    found.push(node);
                }
            }
        }
        // A labelled node and the text inside it that reads the same are
        // one thing: the outermost stands for both.
        if part.text.is_some() || part.holding.is_some() {
            let all = found.clone();
            found.retain(|n| {
                std::iter::successors(logical.parent(*n), |a| logical.parent(*a))
                    .all(|a| !all.contains(&a))
            });
        }
        if let Some(row) = part.row {
            let mut rowed = Vec::new();
            for list in &found {
                let mounted = rows(&scene, *list);
                match mounted.get(row.wrapping_sub(1)) {
                    Some(r) => rowed.push(*r),
                    None => {
                        return Err(format!(
                            "{} has {} rows mounted, not {row}",
                            describe_with(&scene, &semantics, *list),
                            mounted.len()
                        ));
                    }
                }
            }
            found = rowed;
        }
        if let Some(nth) = part.nth {
            found = found
                .get(nth.wrapping_sub(1))
                .copied()
                .into_iter()
                .collect();
        }
        if found.is_empty() {
            return Err(format!("nothing matches `{}`", show_part(part)));
        }
        scopes = found;
    }
    Ok(scopes)
}

fn describe_with(scene: &Scene, semantics: &zgx::semantics::Semantics, node: NodeId) -> String {
    let mut out = format!("@{}", number(node));
    if let Some(s) = semantics.get(node) {
        let _ = write!(out, " {}", semantic_role_name(s));
    }
    let _ = write!(out, " {}", rect(scene.bounds(node)));
    out
}

fn show_part(part: &Part) -> String {
    let mut out = Vec::new();
    if let Some(hook) = &part.hook {
        out.push(format!("hook:{hook}"));
    }
    if let Some(id) = &part.id {
        out.push(format!("#{id}"));
    }
    if let Some(role) = &part.role {
        out.push(format!("role:{role}"));
    }
    if let Some(text) = &part.text {
        out.push(format!("{text:?}"));
    }
    if let Some(text) = &part.holding {
        out.push(format!("~{text:?}"));
    }
    if let Some(n) = part.number {
        out.push(format!("@{n}"));
    }
    if let Some(row) = part.row {
        out.push(format!("row {row}"));
    }
    out.join(" ")
}

/// The one node `target` names, or what is wrong: nothing, or several (each
/// described, so the next target can say which).
pub fn find(ui: &Ui, handle: &ViewHandle, target: &str) -> Result<NodeId, String> {
    let parsed = parse(target)?;
    let found = find_all(ui, handle, &parsed)?;
    match found.as_slice() {
        [one] => Ok(*one),
        many => {
            // Only one of them shows: that is the one a person means.
            let shown: Vec<NodeId> = {
                let scene = ui.scene.borrow();
                many.iter()
                    .copied()
                    .filter(|n| scene.layout_visible(*n))
                    .collect()
            };
            if let [one] = shown[..] {
                return Ok(one);
            }
            let mut message = format!("`{target}` matches {} nodes:", many.len());
            for node in many.iter().take(10) {
                let _ = write!(message, "\n  {}", describe(ui, *node));
            }
            if many.len() > 10 {
                let _ = write!(message, "\n  … {} more", many.len() - 10);
            }
            message.push_str("\nnarrow it (a role, `>>`, `[n]`, or `@n`)");
            Err(message)
        }
    }
}

/// A key and its modifiers as text: `Enter`, `cmd+shift+k`, `alt+ArrowDown`.
/// `cmd` is Command on macOS and Control elsewhere, as commands' keys are.
pub fn parse_keys(text: &str) -> Result<(Key, Modifiers), String> {
    let mut modifiers = Modifiers::default();
    let mut parts: Vec<&str> = text.split('+').collect();
    let name = parts
        .pop()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| format!("`{text}`: no key"))?;
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" if cfg!(target_os = "macos") => modifiers.meta = true,
            "cmd" | "command" | "ctrl" | "control" => modifiers.control = true,
            "meta" | "super" => modifiers.meta = true,
            "alt" | "opt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            other => return Err(format!("`{text}`: unknown modifier `{other}`")),
        }
    }
    let key = match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "space" => Key::Space,
        "arrowup" | "up" => Key::ArrowUp,
        "arrowdown" | "down" => Key::ArrowDown,
        "arrowleft" | "left" => Key::ArrowLeft,
        "arrowright" | "right" => Key::ArrowRight,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            Key::Function(f[1..].parse().unwrap_or(1))
        }
        _ if name.chars().count() == 1 => Key::Character(name.to_lowercase()),
        other => return Err(format!("`{text}`: unknown key `{other}`")),
    };
    Ok((key, modifiers))
}

/// Press and release the primary button at the middle of `bounds`.
pub fn click_at(ui: &mut Ui, bounds: Rect) {
    let (x, y) = (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}
/// A box's middle.
pub fn middle(bounds: Rect) -> (f32, f32) {
    (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
}
/// Move the pointer to `(x, y)`: what is there hovers, what was left stops.
pub fn hover_at(ui: &mut Ui, (x, y): (f32, f32)) {
    ui.dispatch(InputEvent::PointerMove { x, y });
}
/// Move to `(x, y)` and click the context button there.
pub fn right_click_at(ui: &mut Ui, (x, y): (f32, f32)) {
    let button = PointerButton::Secondary;
    ui.dispatch(InputEvent::PointerMove { x, y });
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}
/// Drag with the primary button from one point to another: pressed there,
/// moved past the drag threshold, on to `to` in steps, as a hand moves
/// (what lies between sees the drag pass), and released.
pub fn drag_between(ui: &mut Ui, from: (f32, f32), to: (f32, f32)) {
    /// Moves on the way, past the first.
    const STEPS: u16 = 8;
    let (x, y) = from;
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerMove { x, y });
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerMove {
        x: x + 16.,
        y: y + 8.,
    });
    for step in 1..=STEPS {
        let part = f32::from(step) / f32::from(STEPS);
        ui.dispatch(InputEvent::PointerMove {
            x: x + (to.0 - x) * part,
            y: y + (to.1 - y) * part,
        });
    }
    let (x, y) = to;
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}
/// Click `node`, then type `text` into what has focus.
pub fn type_into(ui: &mut Ui, node: NodeId, text: &str) {
    let bounds = ui.scene.borrow().bounds(node);
    click_at(ui, bounds);
    ui.dispatch(InputEvent::Text(text.into()));
}
/// Press and release `key` with `modifiers`, wherever focus is.
pub fn press(ui: &mut Ui, key: Key, modifiers: Modifiers) {
    ui.dispatch(InputEvent::KeyDown {
        key: key.clone(),
        modifiers,
        repeat: false,
    });
    ui.dispatch(InputEvent::KeyUp { key, modifiers });
}
/// Scroll the viewport under `node`'s middle by `dy` points (down is
/// positive).
pub fn scroll(ui: &mut Ui, node: NodeId, dy: f32) {
    let b = ui.scene.borrow().bounds(node);
    ui.dispatch(InputEvent::Scroll {
        x: b.x + b.width / 2.,
        y: b.y + b.height / 2.,
        delta_x: 0.,
        delta_y: dy,
    });
}

fn clip(text: &str, max: usize) -> String {
    let one_line = text.replace('\n', "⏎");
    if one_line.chars().count() <= max {
        return one_line;
    }
    let mut out: String = one_line.chars().take(max).collect();
    out.push('…');
    out
}
fn rect(b: Rect) -> String {
    format!(
        "{},{} {}x{}",
        b.x.round(),
        b.y.round(),
        b.width.round(),
        b.height.round()
    )
}

/// How [`tree`] shows the tree.
#[derive(Clone, Debug)]
pub struct TreeOptions {
    /// Levels below the root shown (all, by default).
    pub depth: Option<usize>,
    /// Lines at most; the rest are counted, not shown.
    pub budget: usize,
    /// Every node, layout boxes with nothing to say included.
    pub all: bool,
}
impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            depth: None,
            budget: 200,
            all: false,
        }
    }
}

/// One node as [`tree`] shows it, for JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// How deep it is among the lines shown.
    pub depth: usize,
    /// Its number (`@12`).
    pub number: u32,
    /// The script's `id` for it.
    pub id: Option<String>,
    /// Its role, as ARIA names it, unless a plain group.
    pub role: Option<&'static str>,
    /// Its accessible name, else its text.
    pub text: Option<String>,
    /// Its value (an input's text, a slider's number).
    pub value: Option<String>,
    /// Its box, in window points.
    pub bounds: Rect,
    /// It has the keyboard focus.
    pub focused: bool,
    /// It is disabled.
    pub disabled: bool,
    /// Checked or not, if it can be.
    pub checked: Option<bool>,
}

/// The tree under `from` (the view, by default), one node a line: its
/// number, id, role, name or text, value and box, in document order.
/// Boxes with nothing to say (no id, role or text) are left out unless
/// `all`, their children shown in their place. Past `budget` lines, the
/// rest are counted.
pub fn tree(
    ui: &Ui,
    handle: &ViewHandle,
    from: Option<NodeId>,
    options: &TreeOptions,
) -> (Vec<Line>, usize) {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let focused = ui.input.focused();
    let logical = Logical::new(ui, &scene);
    let root = from.unwrap_or_else(|| handle.node());
    let ids: std::collections::HashMap<NodeId, String> = handle.ids().into_iter().collect();
    let mut lines = Vec::new();
    let mut hidden = 0;
    let mut stack = vec![(root, 0_usize)];
    while let Some((node, depth)) = stack.pop() {
        let s = semantics.get(node);
        let role = s
            .map(semantic_role_name)
            .filter(|r| *r != "group" || options.all);
        let label = s.map(|s| s.label.clone()).filter(|l| !l.is_empty());
        let text = label.or_else(|| own_text(&scene, node));
        let id = ids.get(&node).cloned();
        let shown = options.all || id.is_some() || role.is_some() || text.is_some();
        let next = if shown { depth + 1 } else { depth };
        if shown {
            if lines.len() < options.budget {
                lines.push(Line {
                    depth,
                    number: number(node),
                    id,
                    role,
                    text,
                    // A text's value is its text again.
                    value: s
                        .filter(|s| s.role != Role::Label)
                        .and_then(|s| s.value.clone()),
                    bounds: scene.bounds(node),
                    focused: focused == Some(node),
                    disabled: s.is_some_and(|s| s.disabled),
                    checked: s.and_then(|s| s.checked),
                });
            } else {
                hidden += 1;
            }
        }
        if options.depth.is_none_or(|max| next <= max) {
            for child in logical.children(node).rev() {
                stack.push((child, next));
            }
        } else {
            hidden += logical.under(node).len() - 1;
        }
    }
    (fold(lines), hidden)
}

/// Lines with nothing of their own folded into what holds them: a text
/// that only says what its parent is called (a button's label), an empty
/// text, and a box's one text taken as its own (`#count "0"`).
fn fold(lines: Vec<Line>) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::with_capacity(lines.len());
    let plain =
        |l: &Line| l.id.is_none() && matches!(l.role, None | Some("text")) && l.value.is_none();
    for (i, line) in lines.iter().enumerate() {
        let parent = out.iter().rev().find(|p| p.depth < line.depth);
        if plain(line) {
            if line.text.as_deref().is_none_or(str::is_empty) {
                continue;
            }
            if let Some(parent) = parent
                && parent.depth + 1 == line.depth
                && parent.text == line.text
            {
                continue;
            }
        }
        // A box with an id and nothing to say, holding one plain text.
        let only_child = lines.get(i + 1).filter(|next| {
            next.depth == line.depth + 1
                && plain(next)
                && lines
                    .get(i + 2)
                    .is_none_or(|after| after.depth <= line.depth)
        });
        let mut line = line.clone();
        if line.text.is_none()
            && line.role.is_none()
            && let Some(child) = only_child
        {
            line.text = child.text.clone();
        }
        out.push(line);
    }
    // The texts taken by their boxes go.
    let mut kept: Vec<Line> = Vec::with_capacity(out.len());
    for line in out {
        if let Some(parent) = kept.iter().rev().find(|p| p.depth < line.depth)
            && parent.depth + 1 == line.depth
            && line.id.is_none()
            && matches!(line.role, None | Some("text"))
            && parent.text == line.text
            && parent.role.is_none()
        {
            continue;
        }
        kept.push(line);
    }
    kept
}

/// [`tree`]'s lines as text: indented two spaces a level.
pub fn tree_text(lines: &[Line], hidden: usize) -> String {
    let mut out = String::new();
    for line in lines {
        let _ = write!(out, "{}@{}", "  ".repeat(line.depth), line.number);
        if let Some(id) = &line.id {
            let _ = write!(out, " #{id}");
        }
        if let Some(role) = line.role {
            let _ = write!(out, " {role}");
        }
        if let Some(text) = &line.text {
            let _ = write!(out, " {:?}", clip(text, 60));
        }
        if let Some(value) = &line.value {
            let _ = write!(out, " ={:?}", clip(value, 40));
        }
        let _ = write!(out, " {}", rect(line.bounds));
        if line.focused {
            out.push_str(" focused");
        }
        if line.disabled {
            out.push_str(" disabled");
        }
        match line.checked {
            Some(true) => out.push_str(" checked"),
            Some(false) => out.push_str(" unchecked"),
            None => {}
        }
        out.push('\n');
    }
    if hidden > 0 {
        let _ = writeln!(
            out,
            "… {hidden} more (narrow with a target, --depth, or --budget)"
        );
    }
    out
}

/// What there is to know of `node`: its number, kind, where the script made
/// it and how it named it, its role, name, value and state, its box, its
/// text, and how many children it has.
pub fn node(
    ui: &Ui,
    inspector: Option<&zgx::inspect::Inspector>,
    node: NodeId,
) -> Vec<(String, String)> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = vec![
        ("node".to_string(), format!("@{}", number(node))),
        ("kind".to_string(), zgx::inspect::kind_name(&scene, node)),
    ];
    if let Some(label) = inspector.and_then(|i| i.label_of(node)) {
        out.push(("made".into(), label.name.to_string()));
        if let Some(source) = label.source {
            out.push(("source".into(), source.to_string()));
        }
    }
    if let Some(s) = semantics.get(node) {
        out.push(("role".into(), semantic_role_name(s).into()));
        if !s.label.is_empty() {
            out.push(("name".into(), s.label.clone()));
        }
        if let Some(value) = &s.value {
            out.push(("value".into(), clip(value, 200)));
        }
        if let Some(placeholder) = &s.placeholder {
            out.push(("placeholder".into(), placeholder.clone()));
        }
        let mut flags = Vec::new();
        if s.disabled {
            flags.push("disabled");
        }
        if s.read_only {
            flags.push("read-only");
        }
        match s.checked {
            Some(true) => flags.push("checked"),
            Some(false) => flags.push("unchecked"),
            None => {}
        }
        if s.expanded == Some(true) {
            flags.push("expanded");
        }
        if !flags.is_empty() {
            out.push(("state".into(), flags.join(" ")));
        }
    }
    if ui.input.focused() == Some(node) {
        out.push(("focused".into(), "yes".into()));
    }
    out.push(("box".into(), rect(scene.bounds(node))));
    // Its own text, else the first inside (a text's box).
    if let Some(text) = first_text(&scene, node) {
        out.push(("text".into(), clip(&text, 400)));
    }
    out.push(("children".into(), scene.children(node).len().to_string()));
    out
}

/// Drags from `from` onto `to` with the primary button: pressed near
/// `from`'s top left, past the drag threshold, then released over `to`'s
/// middle (its upper half: a reorderable row drops above it).
pub fn drag(ui: &mut Ui, from: Rect, to: Rect) {
    let start = (
        from.x + from.width.min(12.),
        from.y + from.height.min(12.) / 2.,
    );
    let end = (to.x + to.width / 2., to.y + to.height / 3.);
    drag_between(ui, start, end);
}
