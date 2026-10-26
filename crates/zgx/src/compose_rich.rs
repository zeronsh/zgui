//! Display-only inline typography lowered to one retained paragraph.
use crate::{
    compose::View,
    scene::Color,
    style::{Styled, Styles},
    text_layout::{FontFamily, LetterSpacing, LineHeight},
};
use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};
/// A link's click, told the keys held (a ⌘-click).
#[derive(Clone)]
pub(crate) struct InlineAction(Rc<RefCell<dyn FnMut(crate::input::Modifiers)>>);
impl std::fmt::Debug for InlineAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InlineAction")
    }
}
impl PartialEq for InlineAction {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Link {
    pub range: Range<usize>,
    pub action: Option<InlineAction>,
    /// Where it goes (`TextSpan::href`): a click opens it, its context menu
    /// opens or copies it.
    pub href: Option<Arc<str>>,
    /// It restyles under the pointer, so no hover ring.
    pub styled_hover: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Content {
    pub rich: Arc<crate::rich_text::RichText>,
    pub links: Vec<Link>,
    pub icons: Vec<InlineIcon>,
}
/// An icon in the text (`TextSpan::icon`): drawn over the room its
/// placeholder run (`range`) keeps in the line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineIcon {
    pub range: Range<usize>,
    pub svg: Arc<crate::svg::SvgData>,
    pub size: f32,
    /// How far into the room it starts (the span's padding before it).
    pub inset: f32,
}
/// A span's leading icon: `size` square, `gap` before the text, tinted
/// with `tint` (a colour or token) if set, else as the SVG says (one that
/// follows the text colour takes the span's).
#[derive(Clone, Debug, PartialEq)]
pub struct SpanIcon {
    /// The icon.
    pub svg: Arc<crate::svg::SvgData>,
    /// Its width and height.
    pub size: f32,
    /// The room between it and the text.
    pub gap: f32,
    /// Its colour, or a colour token; `None` paints it as the SVG says.
    pub tint: Option<crate::theme::Themed<Color>>,
}
/// What keeps an icon's room in the line: a word joiner (zero width, and
/// no line break either side, so the icon stays with its text), padded.
pub(crate) const ICON_PLACEHOLDER: &str = "\u{2060}";

/// An inline run. Only typography is supported; layout belongs to the paragraph.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub(crate) action: Option<InlineAction>,
    pub(crate) href: Option<Arc<str>>,
    pub(crate) text: Arc<str>,
    pub(crate) styles: Styles,
    pub(crate) background: Option<Color>,
    pub(crate) underline: Option<crate::rich_text::Decoration>,
    pub(crate) strikethrough: Option<crate::rich_text::Decoration>,
    pub(crate) padding: [f32; 2],
    pub(crate) radius: f32,
    pub(crate) hover: Option<SpanHover>,
    pub(crate) icon: Option<SpanIcon>,
}
/// How a link span looks under the pointer (`TextSpan::hover`): CSS's
/// `a:hover { text-decoration: underline }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpanHover {
    /// The underline under the pointer.
    pub underline: Option<crate::rich_text::Decoration>,
    /// The text colour under the pointer, or a colour token.
    pub text_color: Option<crate::theme::Themed<Color>>,
}
/// A span of `text`, styled as the paragraph is.
pub fn text_span(text: impl Into<Arc<str>>) -> TextSpan {
    TextSpan {
        action: None,
        href: None,
        text: text.into(),
        styles: Styles::new(),
        background: None,
        underline: None,
        strikethrough: None,
        padding: [0.; 2],
        radius: 0.,
        hover: None,
        icon: None,
    }
}
impl TextSpan {
    /// An icon before the text, in the line (a file chip's file type):
    /// `size` square, centred on the line, `gap` from the text. An icon
    /// that follows the text colour takes the span's.
    pub fn icon(mut self, icon: SpanIcon) -> Self {
        self.icon = Some(icon);
        self
    }
    /// Restyled under the pointer, once it's a link (`on_click`), in place
    /// of the pointer's focus-ring outline. Not underlined at rest unless
    /// asked (`underline`).
    pub fn hover(mut self, hover: SpanHover) -> Self {
        self.hover = Some(hover);
        self
    }
    /// Room around the span (`x` before and after it, taking space in the
    /// line; `y` above and below its background), its background hugging
    /// the text: an inline code chip.
    pub fn padding(mut self, x: f32, y: f32) -> Self {
        self.padding = [x.max(0.), y.max(0.)];
        self
    }
    /// The span background's corner radius.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.);
        self
    }
    /// Make this span a keyboard-accessible link with shaped hit regions,
    /// underlined unless it has a `hover` style.
    pub fn on_click(self, mut callback: impl FnMut() + 'static) -> Self {
        self.on_click_with(move |_| callback())
    }
    /// `on_click`, told the keys held as it was clicked: a ⌘-click (Control
    /// elsewhere), or a middle click, which passes as one, as browsers open
    /// a link in the background.
    pub fn on_click_with(
        mut self,
        callback: impl FnMut(crate::input::Modifiers) + 'static,
    ) -> Self {
        self.action = Some(InlineAction(Rc::new(RefCell::new(callback))));
        self
    }
    /// A link to `url`: a click (after `on_click`'s, if it has one) opens it
    /// with the system's handler (`Platform::open_url`, a browser for
    /// `https:`), and its context menu opens or copies it. Only web and mail
    /// links open (`http:`, `https:`, `mailto:`): text often isn't the
    /// app's own (a message, a document), and a `file:` or another
    /// scheme's handler can run what the link names. An app's own schemes
    /// it handles itself, in `on_click`.
    pub fn href(mut self, url: impl Into<Arc<str>>) -> Self {
        self.href = Some(url.into());
        self
    }

    /// A colour or a colour token (`tok::SELECTION`).
    pub fn background(mut self, color: impl Into<crate::theme::Themed<Color>>) -> Self {
        match color.into() {
            crate::theme::Themed::Value(color) => self.background = Some(color),
            crate::theme::Themed::Token(token) => {
                self.background = None;
                self.styles = std::mem::take(&mut self.styles).bg(token);
            }
        }
        self
    }
    /// Underline the span.
    pub fn underline(mut self, decoration: crate::rich_text::Decoration) -> Self {
        self.underline = Some(decoration);
        self
    }
    /// Strike the span through.
    pub fn strikethrough(mut self, decoration: crate::rich_text::Decoration) -> Self {
        self.strikethrough = Some(decoration);
        self
    }
    /// A colour or a colour token (`tok::TEXT_SECONDARY`).
    pub fn text_color(mut self, color: impl Into<crate::theme::Themed<Color>>) -> Self {
        self.styles = std::mem::take(&mut self.styles).text_color(color);
        self
    }
    /// The font size in points; a size that isn't finite and positive is 16.
    pub fn text_size(mut self, size: f32) -> Self {
        self.styles
            .fields_mut(crate::style::bit::TEXT_SIZE)
            .text_size = Some(if size.is_finite() && size > 0. {
            size
        } else {
            16.
        })
        .into();
        self
    }
    /// Families to try, in order, for characters the span's font lacks.
    pub fn font_fallbacks(mut self, families: impl Into<std::sync::Arc<[FontFamily]>>) -> Self {
        self.styles.rare_mut().font_fallbacks = Some(families.into());
        self
    }
    /// OpenType features (ligatures, tabular figures) for the span.
    pub fn font_features(mut self, features: crate::text_layout::FontFeatures) -> Self {
        self.styles.rare_mut().font_features = Some(features);
        self
    }
    /// The span's font family.
    pub fn font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.styles
            .fields_mut(crate::style::bit::FONT_FAMILY)
            .font_family = Some(family.into());
        self
    }
    /// The span's font weight, from 1 to 1000 (400 regular, 700 bold).
    pub fn font_weight(mut self, weight: u16) -> Self {
        self.styles
            .fields_mut(crate::style::bit::FONT_WEIGHT)
            .font_weight = Some(weight.clamp(1, 1000));
        self
    }
    /// Italic, or upright.
    pub fn italic(mut self, italic: bool) -> Self {
        self.styles.fields_mut(crate::style::bit::ITALIC).italic = Some(italic);
        self
    }
    /// A bold span (weight 700).
    pub fn font_bold(self) -> Self {
        self.font_weight(700)
    }
    /// The font's own line height.
    pub fn line_height_normal(mut self) -> Self {
        self.styles
            .fields_mut(crate::style::bit::LINE_HEIGHT)
            .line_height = Some(LineHeight::NORMAL);
        self
    }
    /// A line height of `px` points.
    pub fn line_height(mut self, px: f32) -> Self {
        self.styles
            .fields_mut(crate::style::bit::LINE_HEIGHT)
            .line_height = Some(LineHeight::px(px));
        self
    }
    /// `px` points added between characters.
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.styles.rare_mut().letter_spacing = Some(LetterSpacing::px(px));
        self
    }
}
/// Builder for inline children. Conversion to View retains a single paragraph.
pub struct RichTextView {
    view: View,
    spans: Vec<TextSpan>,
}
/// A paragraph of spans (see [`text_span`]), laid out and painted as one.
pub fn rich_text() -> RichTextView {
    RichTextView {
        view: crate::compose::rich_text_signal(Vec::new),
        spans: Vec::new(),
    }
}
impl RichTextView {
    /// Add `span` after the existing spans.
    pub fn child(mut self, span: TextSpan) -> Self {
        self.spans.push(span);
        self
    }
    /// Add `spans` after the existing spans, in order.
    pub fn children(mut self, spans: impl IntoIterator<Item = TextSpan>) -> Self {
        self.spans.extend(spans);
        self
    }
    /// Name the paragraph's view (see `View::id`).
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.view = self.view.id(id);
        self
    }
    /// Let the pointer select the paragraph's text (see `View::selectable`).
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.view = self.view.selectable(selectable);
        self
    }
    /// The highlight behind selected text (see `View::selection_color`).
    pub fn selection_color(mut self, color: Color) -> Self {
        self.view = self.view.selection_color(color);
        self
    }
    /// The selected text's colour (see `View::selected_text_color`).
    pub fn selected_text_color(mut self, color: Color) -> Self {
        self.view = self.view.selected_text_color(color);
        self
    }
}
impl Styled for RichTextView {
    fn styles_mut(&mut self) -> &mut Styles {
        self.view.styles_mut()
    }
}
impl From<RichTextView> for View {
    fn from(builder: RichTextView) -> Self {
        crate::compose::with_rich_spans(builder.view, builder.spans)
    }
}

/// What a link does: its action, and where it goes.
type LinkTarget = (Option<InlineAction>, Option<Arc<str>>);
/// Link `node`'s context menu: Open Link and Copy Link for its `href` (none
/// without one, so the press goes on to what it is in).
fn mount_link_menu(
    ui: &mut crate::widgets::Ui,
    node: crate::scene::NodeId,
    link: Rc<RefCell<LinkTarget>>,
    platform: Option<Rc<crate::platform::PlatformService>>,
    environment: crate::compose::Environment,
) {
    crate::compose_context_menu::mount(
        ui,
        node,
        Box::new(move |_| {
            let Some(url) = link.borrow().1.clone() else {
                return Vec::new();
            };
            let (open, copy) = (platform.clone(), platform.clone());
            let opened = url.clone();
            let mut items = Vec::new();
            if opens(&url) {
                items.push(
                    crate::compose::menu_item("Open Link")
                        .id("link-open")
                        .on_click(move || {
                            if let Some(platform) = &open {
                                platform.0.open_url(&opened);
                            }
                        }),
                );
            }
            items.extend([crate::compose::menu_item("Copy Link")
                .id("link-copy")
                .on_click(move || {
                    if let Some(platform) = &copy {
                        platform.0.set_clipboard_text(url.to_string());
                    }
                })]);
            items
        }),
        None,
        false,
        environment,
    );
}

/// Whether a link opens through the system's handler: web and mail links
/// only (see `TextSpan::href`).
fn opens(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
    })
}
pub(crate) fn mount_links(
    ui: &mut crate::widgets::Ui,
    owner: crate::scene::NodeId,
    paragraph: crate::scene::NodeId,
    content: crate::reactive::Signal<Content>,
    hovered: crate::reactive::Signal<Option<usize>>,
    environment: &crate::compose::Environment,
) {
    use crate::{
        input::{EventPhase, InputEvent, Modifiers, PointerButton},
        scene::{Layout, NodeId, NodeKind, QuadStyle, Rect, Style},
        semantics::{Role, SemanticNode},
    };
    use std::cell::Cell;
    struct Mounted {
        node: NodeId,
        fragments: Rc<RefCell<Vec<NodeId>>>,
        action: Rc<RefCell<LinkTarget>>,
        focus: Rc<Cell<bool>>,
        hover: Rc<Cell<bool>>,
        /// Its Open Link and Copy Link menu is mounted.
        menu: bool,
        /// Restyled under the pointer: no ring for hover.
        styled: Rc<Cell<bool>>,
    }
    let bounds = ui.observe_bounds(paragraph);
    let owner_bounds = ui.observe_bounds(owner);
    let engine = ui.observe_text_geometry_revision();
    let theme = ui.theme_scope();
    let weak = ui.downgrade();
    // What opens and copies `href`s: the host's platform, if any.
    let platform: Option<Rc<crate::platform::PlatformService>> = environment.services.get();
    let environment = environment.clone();
    let mut mounted: Vec<Mounted> = Vec::new();
    let mut geometry_key = None;
    let mut geometry: Vec<Vec<Rect>> = Vec::new();
    // Where the links go, as copied HTML writes them.
    let mut targets: Vec<(std::ops::Range<usize>, Arc<str>)> = Vec::new();
    let guard = crate::selection::LinksGuard {
        ui: ui.downgrade(),
        paragraph,
    };
    ui.retain(owner, guard);
    ui.bind(owner, move || {
        let data = content.get();
        let bounds = bounds.get();
        let owner_bounds = owner_bounds.get();
        let engine = engine.get();
        let Some(mut ui) = weak.upgrade() else {
            return;
        };
        if !ui.scene.borrow().contains(owner) {
            return;
        }
        let now: Vec<_> = data
            .links
            .iter()
            .filter_map(|link| Some((link.range.clone(), link.href.clone()?)))
            .collect();
        if now != targets {
            crate::selection::set_links(&ui, paragraph, now.clone());
            targets = now;
        }
        let key = (
            data.rich.clone(),
            bounds.width.to_bits(),
            bounds.height.to_bits(),
            engine,
            data.links
                .iter()
                .map(|l| l.range.clone())
                .collect::<Vec<_>>(),
        );
        if geometry_key.as_ref() != Some(&key) {
            geometry = if data.links.is_empty() {
                Vec::new()
            } else {
                let layout = ui
                    .scene
                    .borrow()
                    .shape_rich_text(&data.rich, Some(bounds.width));
                data.links
                    .iter()
                    .map(|link| {
                        layout
                            .selection(link.range.clone())
                            .into_iter()
                            .filter_map(|rect| {
                                rect.intersection(Rect::new(0., 0., bounds.width, bounds.height))
                            })
                            .filter(|r| r.width > 0. && r.height > 0.)
                            .collect()
                    })
                    .collect()
            };
            geometry_key = Some(key);
        }
        while mounted.len() > data.links.len() {
            let old = mounted.pop().unwrap();
            ui.remove(old.node);
            if !ui.scene.borrow().contains(owner) {
                return;
            }
        }
        for (index, link) in data.links.iter().enumerate() {
            if index == mounted.len() {
                let node = ui.container(
                    owner,
                    Layout::Overlay,
                    Style {
                        absolute: true,
                        ..Default::default()
                    },
                );
                // A browser's link hand, over each of its fragments too.
                ui.scene
                    .borrow_mut()
                    .set_cursor(node, Some(crate::cursor::Cursor::Pointer));
                let fragments = Rc::new(RefCell::new(Vec::new()));
                let action = Rc::new(RefCell::new((link.action.clone(), link.href.clone())));
                let focus = Rc::new(Cell::new(false));
                let hover = Rc::new(Cell::new(false));
                let styled = Rc::new(Cell::new(link.styled_hover));
                let (nodes, callback, focused, hovered, restyled, under) = (
                    fragments.clone(),
                    action.clone(),
                    focus.clone(),
                    hover.clone(),
                    styled.clone(),
                    hovered.clone(),
                );
                let weak = ui.downgrade();
                let ring = theme.clone();
                let open = platform.clone();
                // A click's: its own action with the keys held, then its
                // `href` opened.
                let activate = move |link: &LinkTarget, held| {
                    if let Some(action) = &link.0 {
                        (action.0.borrow_mut())(held);
                    }
                    if let (Some(url), Some(platform)) = (&link.1, &open)
                        && opens(url)
                    {
                        platform.0.open_url(url);
                    }
                };
                let mut middle = false;
                ui.on_event(node, true, move |cx| {
                    if cx.phase == EventPhase::Capture
                        || (cx.default_prevented()
                            && !matches!(
                                cx.event,
                                InputEvent::Blur
                                    | InputEvent::PointerLeave
                                    | InputEvent::PointerCancel
                            ))
                    {
                        return;
                    }
                    match cx.event {
                        InputEvent::Activate => {
                            cx.stop_propagation();
                            let link = callback.borrow().clone();
                            activate(&link, cx.pointer_modifiers());
                            return;
                        }
                        // A middle click acts as a ⌘-click (Control
                        // elsewhere), as a browser opens a link in the
                        // background.
                        InputEvent::PointerDown {
                            button: PointerButton::Middle,
                            ..
                        } => {
                            middle = true;
                            cx.prevent_default();
                            return;
                        }
                        InputEvent::PointerUp {
                            button: PointerButton::Middle,
                            ..
                        } if std::mem::take(&mut middle) => {
                            let held = Modifiers {
                                meta: cfg!(target_os = "macos"),
                                control: !cfg!(target_os = "macos"),
                                ..cx.pointer_modifiers()
                            };
                            let link = callback.borrow().clone();
                            activate(&link, held);
                            cx.prevent_default();
                            return;
                        }
                        InputEvent::Focus => focused.set(true),
                        InputEvent::Blur => focused.set(false),
                        InputEvent::PointerEnter => {
                            hovered.set(true);
                            under.set(Some(index));
                        }
                        InputEvent::PointerLeave | InputEvent::PointerCancel => {
                            hovered.set(false);
                            if under.with_untracked(|h| *h == Some(index)) {
                                under.set(None);
                            }
                        }
                        _ => return,
                    }
                    let Some(ui) = weak.upgrade() else {
                        return;
                    };
                    let border = if focused.get() || (hovered.get() && !restyled.get()) {
                        1.
                    } else {
                        0.
                    };
                    let mut scene = ui.scene.borrow_mut();
                    if !scene.contains(node) {
                        return;
                    }
                    let quad = QuadStyle {
                        border_width: border,
                        border_color: ring.color_untracked(crate::theme::tok::FOCUS_RING),
                        ..Default::default()
                    };
                    scene.set_kind(
                        node,
                        NodeKind::Panel {
                            layout: Layout::Overlay,
                            quad: quad.clone(),
                        },
                    );
                    for id in nodes.borrow().iter().copied() {
                        if scene.contains(id) {
                            scene.set_kind(id, NodeKind::Quad(quad.clone()));
                        }
                    }
                });
                mounted.push(Mounted {
                    node,
                    fragments,
                    action,
                    focus,
                    hover,
                    styled,
                    menu: false,
                });
            }
            let entry = &mut mounted[index];
            entry.styled.set(link.styled_hover);
            let old = entry
                .action
                .replace((link.action.clone(), link.href.clone()));
            drop(old);
            if !ui.scene.borrow().contains(owner) {
                return;
            }
            // Its context menu, once it has an `href` to open or copy.
            if !entry.menu && link.href.is_some() {
                entry.menu = true;
                mount_link_menu(
                    &mut ui,
                    entry.node,
                    entry.action.clone(),
                    platform.clone(),
                    environment.clone(),
                );
            }
            ui.semantics.borrow_mut().set(
                entry.node,
                SemanticNode::new(Role::Link, &data.rich.text()[link.range.clone()]),
            );
            let rects = &geometry[index];
            let first = rects.first().copied().unwrap_or_default();
            let mut style = ui.scene.borrow().style(entry.node);
            style.width = Some(first.width);
            style.height = Some(first.height);
            // Placed from the owner's content box: inside its padding and border.
            let padding = ui.scene.borrow().content_insets(owner);
            style.margin.left = bounds.x - owner_bounds.x + first.x - padding.left;
            style.margin.top = bounds.y - owner_bounds.y + first.y - padding.top;
            ui.scene.borrow_mut().set_style(entry.node, style);
            ui.set_disabled(entry.node, rects.is_empty());
            if !ui.scene.borrow().contains(owner) {
                return;
            }
            let mut nodes = entry.fragments.borrow_mut();
            while nodes.len() > rects.len().saturating_sub(1) {
                ui.remove(nodes.pop().unwrap());
            }
            let quad = QuadStyle {
                border_width: if entry.focus.get() || (entry.hover.get() && !entry.styled.get()) {
                    1.
                } else {
                    0.
                },
                border_color: theme.color(crate::theme::tok::FOCUS_RING),
                ..Default::default()
            };
            for (i, rect) in rects.iter().skip(1).enumerate() {
                let style = Style {
                    absolute: true,
                    width: Some(rect.width),
                    height: Some(rect.height),
                    margin: crate::scene::Insets {
                        left: rect.x - first.x,
                        top: rect.y - first.y,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                if i == nodes.len() {
                    let child = ui.scene.borrow_mut().append(
                        entry.node,
                        NodeKind::Quad(quad.clone()),
                        style,
                    );
                    nodes.push(child);
                } else {
                    ui.scene.borrow_mut().set_style(nodes[i], style);
                }
            }
        }
    });
}

/// Draws each inline icon over the room its placeholder keeps, following
/// the paragraph's layout.
pub(crate) fn mount_icons(
    ui: &mut crate::widgets::Ui,
    owner: crate::scene::NodeId,
    paragraph: crate::scene::NodeId,
    content: crate::reactive::Signal<Content>,
) {
    use crate::scene::{NodeId, NodeKind, Style};
    let bounds = ui.observe_bounds(paragraph);
    let owner_bounds = ui.observe_bounds(owner);
    let engine = ui.observe_text_geometry_revision();
    let weak = ui.downgrade();
    let mut mounted: Vec<NodeId> = Vec::new();
    // The icons' rects, kept while the text and its width are the same:
    // scrolling moves the paragraph every frame without reshaping it.
    let mut geometry_key = None;
    let mut rects: Vec<Option<crate::scene::Rect>> = Vec::new();
    ui.bind(owner, move || {
        let data = content.get();
        let bounds = bounds.get();
        let owner_bounds = owner_bounds.get();
        let engine = engine.get();
        let Some(mut ui) = weak.upgrade() else {
            return;
        };
        if !ui.scene.borrow().contains(owner) {
            return;
        }
        while mounted.len() > data.icons.len() {
            ui.remove(mounted.pop().unwrap());
        }
        if data.icons.is_empty() {
            return;
        }
        let key = (data.rich.clone(), bounds.width.to_bits(), engine);
        if geometry_key.as_ref() != Some(&key) {
            let scene = ui.scene.borrow();
            let layout = scene.shape_rich_text(&data.rich, Some(bounds.width));
            rects = data
                .icons
                .iter()
                .map(|icon| layout.selection(icon.range.clone()).into_iter().next())
                .collect();
            drop(scene);
            geometry_key = Some(key);
        }
        let padding = ui.scene.borrow().content_insets(owner);
        for (index, (icon, rect)) in data.icons.iter().zip(rects.iter()).enumerate() {
            let rect = rect.unwrap_or_default();
            // At the room's start, centred on the line; placed from the
            // owner's content box.
            let style = Style {
                absolute: true,
                width: Some(icon.size),
                height: Some(icon.size),
                margin: crate::scene::Insets {
                    left: bounds.x - owner_bounds.x + rect.x + icon.inset - padding.left,
                    top: bounds.y - owner_bounds.y + rect.y + (rect.height - icon.size) / 2.
                        - padding.top,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut scene = ui.scene.borrow_mut();
            if index == mounted.len() {
                mounted.push(scene.append(owner, NodeKind::Svg(icon.svg.clone()), style));
            } else {
                // Unchanged (the paragraph only moved): nothing to lay out.
                let node = mounted[index];
                if !matches!(scene.kind(node), NodeKind::Svg(svg) if Arc::ptr_eq(svg, &icon.svg)) {
                    scene.set_kind(node, NodeKind::Svg(icon.svg.clone()));
                }
                if scene.style(node) != style {
                    scene.set_style(node, style);
                }
            }
        }
    });
}
