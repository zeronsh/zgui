//! `ui.markdown_view { text }`: markdown drawn, the way a chat or a notes
//! app shows it, without the script drawing its blocks. The text is parsed
//! as it changes ([`Incremental`]: a reply streaming in parses its tail),
//! and only the blocks whose value changed are built again; the rest keep
//! their nodes, layout and paint. Paragraphs and headings are runs of
//! styled text, lists and quotes hold blocks, code blocks show their
//! language and a Copy button, tables lay out their cells in rows. Colours
//! and sizes are the theme's tokens.
use super::{Incremental, Options, PENDING_LINK_URL};
use crate::value::{Value, identity};
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx::{
    compose::{Context, View, button, column, div, keyed, prelude::*, row, text},
    reactive::Signal,
    semantics::Role,
    text_layout::FontFamily,
    theme::{Themed, token},
};

/// How a markdown view draws.
#[derive(Clone)]
pub(crate) struct ViewOptions {
    pub parse: Options,
    /// Code blocks get a Copy button.
    pub copy: bool,
}

fn themed<T>(name: &str) -> Themed<T> {
    Themed::Token(token(name))
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Nil) | None => String::new(),
        Some(other) => other.to_string(),
    }
}
fn flag(run: &Value, name: &str) -> bool {
    matches!(run.field(name), Some(Value::Bool(true)))
}

/// A run of text as a span: bold, italic, struck, code or a link.
fn span(run: &Value, bold: bool) -> zgx::compose::TextSpan {
    let mut out = zgx::compose::text_span(text_of(run.field("x")));
    if bold || flag(run, "b") {
        out = out.font_bold();
    }
    if flag(run, "i") {
        out = out.italic(true);
    }
    if flag(run, "s") {
        out = out.strikethrough(Default::default());
    }
    if flag(run, "c") {
        out = out
            .font_family(FontFamily::Monospace)
            .background(themed("surface.sunken"))
            .padding(3., 0.5)
            .radius(3.);
    }
    if let Some(Value::Str(href)) = run.field("a") {
        out = out.text_color(themed("accent"));
        // Still streaming its address: styled as a link, going nowhere.
        if &**href != PENDING_LINK_URL {
            out = out.href(href.to_string());
        }
    }
    out
}

/// A paragraph of runs.
fn runs(list: Option<&Value>, bold: bool) -> zgx::compose::RichTextView {
    let spans = list
        .map(|l| l.list().iter().map(|r| span(r, bold)).collect::<Vec<_>>())
        .unwrap_or_default();
    zgx::compose::rich_text().children(spans).text_wrap(true)
}

fn blocks(list: Option<&Value>, options: &ViewOptions, cx: &mut Context) -> Vec<View> {
    list.map(|l| l.list().iter().map(|b| block(b, options, cx)).collect())
        .unwrap_or_default()
}

/// A code block: its language, a Copy button, and its text unwrapped,
/// scrolling sideways when long.
fn code(b: &Value, options: &ViewOptions, cx: &mut Context) -> View {
    let lang = text_of(b.field("lang"));
    let body = text_of(b.field("text"));
    let mut header = row()
        .w_full()
        .px(12.)
        .py(4.)
        .items_center()
        .selectable(false)
        .child(
            text(if lang.is_empty() {
                "text".into()
            } else {
                lang.clone()
            })
            .text_color(themed("text.tertiary"))
            .text_size(themed("font.size.sm"))
            .grow(),
        );
    if options.copy {
        let label: Signal<String> = cx.runtime().signal("Copy".to_string());
        let platform = cx.try_service::<zgx::platform::PlatformService>();
        let tasks = cx.tasks();
        let (shown, copied, body) = (label.clone(), label, body.clone());
        header = header.child(
            button()
                .id("copy")
                .px(8.)
                .py(3.)
                .bg(Themed::Value(zgx::style::rgba(0x00000000)))
                .on_click(move || {
                    if let Some(platform) = &platform {
                        platform.0.set_clipboard_text(body.clone());
                    }
                    copied.set("Copied".into());
                    let again = copied.clone();
                    tasks.spawn(async move {
                        zgx::timer::sleep(Duration::from_millis(1500)).await;
                        again.set("Copy".into());
                    });
                })
                .child(
                    zgx::compose::text_signal(move || shown.get())
                        .text_size(themed("font.size.sm")),
                ),
        );
    }
    column()
        .w_full()
        .role(Role::Group)
        .accessible_label(if lang.is_empty() {
            "Code".to_string()
        } else {
            format!("{lang} code")
        })
        .bg(themed("surface.sunken"))
        .rounded(8.)
        .border(1.)
        .border_color(themed("border.subtle"))
        .child(header)
        .child(
            zgx::compose::scroll_x(cx.state(0_f32))
                .w_full()
                .px(12.)
                .pb(10.)
                .child(
                    text(body)
                        .font_family(FontFamily::Monospace)
                        .text_size(13.)
                        .shrink_0(),
                ),
        )
}

fn block(b: &Value, options: &ViewOptions, cx: &mut Context) -> View {
    let kind = text_of(b.field("t"));
    match kind.as_str() {
        "h" => {
            let level = b.field("l").and_then(Value::as_f64).unwrap_or(1.) as usize;
            let size = match level {
                1 => "font.size.xl",
                2 => "font.size.lg",
                _ => "font.size.md",
            };
            View::from(runs(b.field("s"), true).text_size(themed(size)).pt(4.)).heading(level)
        }
        "code" => code(b, options, cx),
        "ul" | "ol" => {
            let start = b.field("start").and_then(Value::as_f64).unwrap_or(1.) as usize;
            let items = b
                .field("items")
                .map(|i| i.list().to_vec())
                .unwrap_or_default();
            let mut list = column().role(Role::List).gap(4.);
            for (i, item) in items.iter().enumerate() {
                let marker = if kind == "ol" {
                    format!("{}.", start + i)
                } else {
                    "•".into()
                };
                list = list.child(
                    row()
                        .role(Role::ListItem)
                        .gap(8.)
                        .child(
                            text(marker)
                                .text_color(themed("text.tertiary"))
                                .accessible_hidden()
                                .shrink_0(),
                        )
                        .child(column().grow().min_w(0.).gap(4.).children(blocks(
                            Some(item),
                            options,
                            cx,
                        ))),
                );
            }
            list
        }
        "quote" => column()
            .pl(12.)
            .gap(6.)
            .border_edges(zgx::scene::Insets {
                left: 3.,
                ..Default::default()
            })
            .border_color(themed("border"))
            .text_color(themed("text.secondary"))
            .children(blocks(b.field("b"), options, cx)),
        "hr" => div().w_full().h(1.).my(4.).bg(themed("border.subtle")),
        "table" => {
            let line = |cells: Option<&Value>, bold: bool| {
                let mut out = row().gap(12.);
                for cell in cells.map(|c| c.list().to_vec()).unwrap_or_default() {
                    out = out.child(column().w(160.).child(runs(Some(&cell), bold)));
                }
                out
            };
            let mut table = column().gap(4.).child(line(b.field("head"), true));
            for cells in b
                .field("rows")
                .map(|r| r.list().to_vec())
                .unwrap_or_default()
            {
                table = table.child(line(Some(&cells), false));
            }
            table
        }
        "img" => text(text_of(b.field("alt"))).text_color(themed("text.secondary")),
        _ => runs(b.field("s"), false).into(),
    }
}

/// The view of `source()`'s markdown, following it: parsed again for what
/// changed, and only the blocks whose value changed built again.
pub(crate) fn view(mut source: impl FnMut() -> String + 'static, options: ViewOptions) -> View {
    let parser = Rc::new(RefCell::new(Incremental::new(options.parse.clone())));
    // This pass's blocks, kept until the next has its keys: a block's key
    // is its value's address, which no block made since can share.
    let shown: Rc<RefCell<Vec<Value>>> = Rc::default();
    let (parsing, keeping) = (parser, shown.clone());
    keyed(
        move || {
            let mut parser = parsing.borrow_mut();
            parser.set_text(&source());
            let blocks = parser.blocks();
            let keys = blocks
                .iter()
                .enumerate()
                .map(|(i, b)| (i, identity(b)))
                .collect();
            *keeping.borrow_mut() = blocks;
            keys
        },
        move |(i, _): (usize, usize), cx: &mut Context| {
            let b = shown.borrow().get(i).cloned().unwrap_or(Value::Nil);
            block(&b, &options, cx)
        },
    )
    .w_full()
    .gap(10.)
}
