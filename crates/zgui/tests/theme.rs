//! Themes: token resolution, `extends`, app tokens, live switching (what
//! restyles, what lays out), scoped overrides, theme file hot reload and
//! VS Code theme import.
use std::time::Duration;
use zgui::{
    compose::prelude::*,
    input::InputEvent,
    reactive::Runtime,
    scene::{Color, NodeId, NodeKind},
    theme::{
        Appearance, DARK, LIGHT, Theme, ThemeKind, ThemeOverride, Themes, TokenValue, tok, token,
    },
    widgets::Ui,
};

fn fill(ui: &Ui, node: NodeId) -> Color {
    match ui.scene.borrow().kind(node) {
        NodeKind::Panel { quad, .. } | NodeKind::Quad(quad) => quad.fill,
        NodeKind::Rect(color) => *color,
        NodeKind::Container(_) => Color(0, 0, 0, 0),
        other => panic!("not a painted box: {other:?}"),
    }
}
fn border(ui: &Ui, node: NodeId) -> Color {
    match ui.scene.borrow().kind(node) {
        NodeKind::Panel { quad, .. } => quad.border_color,
        other => panic!("not a panel: {other:?}"),
    }
}
fn text_color(ui: &Ui, node: NodeId) -> Color {
    match ui.scene.borrow().kind(node) {
        NodeKind::Text { color, .. } => *color,
        other => panic!("not text: {other:?}"),
    }
}
const RED: Color = Color(255, 0, 0, 255);
const GREEN: Color = Color(0, 255, 0, 255);

#[test]
fn references_and_derivations_resolve() {
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    themes.register(
        Theme::from_json(
            r##"{
                "name": "derived",
                "type": "dark",
                "tokens": {
                    "accent": "#ff0000",
                    "focus.ring": "alpha({accent}, 0.5)",
                    "test.derive.mix": "mix({accent}, #0000ff, 0.5)",
                    "test.derive.over": "over(#000000, alpha(#ffffff, 0.5))",
                    "test.derive.chain": "{test.derive.mix}",
                    "test.derive.a": "{test.derive.b}",
                    "test.derive.b": "{test.derive.a}",
                    "test.derive.shadow": "0 4 12 0 alpha({accent}, 0.25)"
                }
            }"##,
        )
        .unwrap(),
    );
    themes.set("derived").unwrap();
    let get = |name: &str| themes.root().get_untracked(token(name));
    assert_eq!(themes.color(tok::FOCUS_RING), Color(255, 0, 0, 128));
    assert_eq!(
        get("test.derive.mix"),
        TokenValue::Color(Color(128, 0, 128, 255))
    );
    assert_eq!(
        get("test.derive.over"),
        TokenValue::Color(Color(128, 128, 128, 255))
    );
    assert_eq!(get("test.derive.chain"), get("test.derive.mix"));
    // A reference cycle resolves to nothing rather than hanging.
    assert_eq!(get("test.derive.a"), TokenValue::Unset);
    let TokenValue::Shadow(shadows) = get("test.derive.shadow") else {
        panic!("a shadow")
    };
    assert_eq!(shadows[0].color, Color(255, 0, 0, 64));
    assert_eq!(shadows[0].blur_radius, 6., "CSS blur is twice the sigma");
    // Everything else is the built-in dark theme's.
    assert_eq!(themes.color(tok::BACKGROUND), Color(16, 20, 28, 255));
    assert_eq!(themes.number(tok::RADIUS_MD), 6.);
}

#[test]
fn themes_extend_and_fall_back_to_their_type() {
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    themes.register(
        Theme::new("test-base", ThemeKind::Dark)
            .with(tok::ACCENT, RED)
            .with(tok::RADIUS_MD, 12.),
    );
    themes.register(
        Theme::new("test-child", ThemeKind::Dark)
            .extending("test-base")
            .with(tok::ACCENT, GREEN),
    );
    themes.set("test-child").unwrap();
    assert_eq!(themes.color(tok::ACCENT), GREEN);
    assert_eq!(themes.number(tok::RADIUS_MD), 12., "from the base");
    assert_eq!(themes.color(tok::BACKGROUND), Color(16, 20, 28, 255));
    // A light theme that sets nothing is the built-in light theme.
    themes.register(Theme::new("test-empty-light", ThemeKind::Light));
    themes.set("test-empty-light").unwrap();
    assert_eq!(themes.color(tok::BACKGROUND), Color(0xf5, 0xf6, 0xf8, 255));
    assert_eq!(themes.kind(), ThemeKind::Light);
    // Replacing the base restyles a theme that extends it.
    themes.set("test-child").unwrap();
    themes.register(Theme::new("test-base", ThemeKind::Dark).with(tok::RADIUS_MD, 3.));
    assert_eq!(themes.number(tok::RADIUS_MD), 3.);
    assert!(themes.set("no-such-theme").is_err());
    // Every built-in theme defines every standard token.
    for name in [DARK, LIGHT] {
        themes.set(name).unwrap();
        for (id, token, _) in zgui::theme::standard() {
            assert!(themes.root().is_defined(id), "{name} lacks {token}");
            assert_ne!(
                themes.root().get_untracked(id),
                TokenValue::Unset,
                "{token}"
            );
        }
    }
}

#[test]
fn app_tokens_default_to_standard_tokens_and_themes_may_set_them() {
    let mut ui = Ui::new(200., 200.);
    let card = token("test.app.card");
    ui.theme().declare(card, tok::SURFACE_RAISED);
    let handle = ui.mount(
        column()
            .child(div().id("card").size(20., 20.).bg(card))
            .child(
                div()
                    .id("unknown")
                    .size(20., 20.)
                    .bg(token("test.app.nothing")),
            ),
    );
    ui.scene.borrow_mut().flush();
    let node = handle.find("card").unwrap();
    assert_eq!(fill(&ui, node), Color(35, 46, 64, 255));
    ui.theme().set(LIGHT).unwrap();
    assert_eq!(
        fill(&ui, node),
        Color(255, 255, 255, 255),
        "follows the theme"
    );
    ui.theme().register(
        Theme::from_json(
            r##"{ "name": "test-cards", "type": "light", "tokens": { "test.app.card": "#123456" } }"##,
        )
        .unwrap(),
    );
    ui.theme().set("test-cards").unwrap();
    assert_eq!(fill(&ui, node), Color(0x12, 0x34, 0x56, 255), "set by name");
    // A token nothing defines resolves to nothing: transparent.
    let unknown = handle.find("unknown").unwrap();
    assert_eq!(fill(&ui, unknown), Color(0, 0, 0, 0));
    assert!(!ui.theme().root().is_defined(token("test.app.nothing")));
    // Declaring it later styles what already uses it.
    ui.theme()
        .declare_expr(token("test.app.nothing"), "{accent}")
        .unwrap();
    assert_eq!(fill(&ui, unknown), ui.theme().color(tok::ACCENT));
}

#[test]
fn colour_switches_repaint_only_token_nodes_without_layout() {
    let mut ui = Ui::new(400., 400.);
    let plain_runs = std::rc::Rc::new(std::cell::Cell::new(0));
    let themed_runs = std::rc::Rc::new(std::cell::Cell::new(0));
    let (plain, themed) = (plain_runs.clone(), themed_runs.clone());
    let view = column()
        .children((0..100).map(|i| div().size(4., 4.).bg(rgb(0x102030 + i))))
        .children((0..10).map(|_| div().size(4., 4.).bg(tok::SURFACE_RAISED)))
        .child(div().size(4., 4.).reactive_style(move || {
            plain.set(plain.get() + 1);
            Styles::new().bg(rgb(0x334455))
        }))
        .child(div().size(4., 4.).reactive_style(move || {
            themed.set(themed.get() + 1);
            Styles::new().bg(tok::ACCENT)
        }));
    ui.mount(view);
    ui.scene.borrow_mut().flush();
    let (plain_before, themed_before) = (plain_runs.get(), themed_runs.get());
    // Only the tokens read have signals.
    assert_eq!(
        ui.theme().root().signal_count(),
        2,
        "surface.raised and accent"
    );
    ui.theme().set(LIGHT).unwrap();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0, "colours need no layout");
    assert_eq!(
        report.paint_nodes, 11,
        "the ten raised boxes and the accent one"
    );
    assert_eq!(
        plain_runs.get(),
        plain_before,
        "a style without tokens is not run"
    );
    assert_eq!(themed_runs.get(), themed_before + 1);
    // A theme with the same values restyles nothing.
    ui.theme()
        .register(Theme::new("test-same", ThemeKind::Light).extending(LIGHT));
    ui.theme().set("test-same").unwrap();
    assert!(ui.scene.borrow_mut().flush().is_idle());
    assert_eq!(themed_runs.get(), themed_before + 1);
}

#[test]
fn metric_switches_relayout_only_what_uses_them() {
    let mut ui = Ui::new(400., 400.);
    let handle = ui.mount(
        column()
            .children((0..50).map(|_| div().size(10., 10.).bg(rgb(0x222222))))
            .child(
                div().size(100., 100.).child(
                    div()
                        .id("padded")
                        .p(tok::SPACE_3)
                        .child(div().size(10., 10.)),
                ),
            ),
    );
    ui.scene.borrow_mut().flush();
    let padded = handle.find("padded").unwrap();
    assert_eq!(ui.scene.borrow().bounds(padded).width, 34.);
    ui.theme().register(
        Theme::new("test-roomy", ThemeKind::Dark)
            .extending(DARK)
            .with(tok::SPACE_3, 20.),
    );
    ui.theme().set("test-roomy").unwrap();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(ui.scene.borrow().bounds(padded).width, 50.);
    assert!(report.layout_nodes > 0);
    assert!(
        report.layout_nodes < 10,
        "the fixed-size box bounds the relayout: {} nodes",
        report.layout_nodes
    );
}

#[test]
fn scoped_overrides_restyle_their_subtree() {
    let mut ui = Ui::new(400., 400.);
    let accent = ui.signal(RED);
    let read = accent.clone();
    let handle = ui.mount(
        column()
            .child(div().id("outer").size(10., 10.).bg(tok::SURFACE_RAISED))
            .child(theme_scope(
                ThemeOverride::new().theme(LIGHT),
                column()
                    .child(div().id("card").size(10., 10.).bg(tok::SURFACE_RAISED))
                    .child(
                        button()
                            .id("button")
                            .child(text("In the light").id("label")),
                    ),
            ))
            .child(theme_scope_with(
                move || ThemeOverride::new().with(tok::ACCENT, read.get()),
                div()
                    .id("ring")
                    .size(10., 10.)
                    .border(1.)
                    .border_color(tok::FOCUS_RING),
            )),
    );
    ui.scene.borrow_mut().flush();
    let find = |id: &str| handle.find(id).unwrap();
    assert_eq!(fill(&ui, find("outer")), Color(35, 46, 64, 255));
    assert_eq!(fill(&ui, find("card")), Color(255, 255, 255, 255));
    assert_eq!(fill(&ui, find("button")), Color(0xe8, 0xeb, 0xf0, 255));
    // Inherited text takes the scope's body colour: dark text on the light card.
    assert_eq!(text_color(&ui, find("label")), Color(0x1a, 0x1f, 0x29, 255));
    // The focus ring is `{accent}`: it follows the scope's accent.
    assert_eq!(border(&ui, find("ring")), RED);
    accent.set(GREEN);
    assert_eq!(border(&ui, find("ring")), GREEN);
    // The root's theme changes what follows it, not what the scope fixes.
    ui.theme().set(LIGHT).unwrap();
    assert_eq!(fill(&ui, find("outer")), Color(255, 255, 255, 255));
    assert_eq!(border(&ui, find("ring")), GREEN);
    ui.theme().set(DARK).unwrap();
    assert_eq!(fill(&ui, find("card")), Color(255, 255, 255, 255));
    assert_eq!(fill(&ui, find("outer")), Color(35, 46, 64, 255));
}

#[test]
fn builtin_widgets_keep_their_look_and_follow_the_theme() {
    let mut ui = Ui::new(400., 400.);
    let handle = ui.mount(column().child(button().id("b").child(text("Save"))));
    ui.scene.borrow_mut().flush();
    let b = handle.find("b").unwrap();
    // zgui's original widget colours, now the default dark theme's.
    assert_eq!(fill(&ui, b), Color(35, 46, 64, 255));
    let bounds = ui.scene.borrow().bounds(b);
    ui.dispatch(InputEvent::PointerMove {
        x: bounds.x + 4.,
        y: bounds.y + 4.,
    });
    assert_eq!(fill(&ui, b), Color(47, 64, 88, 255));
    ui.theme().set(LIGHT).unwrap();
    // The light theme's hover wash (black at 5%) over its button colour.
    assert_eq!(fill(&ui, b), Color(220, 223, 228, 255), "the light hover");
    ui.dispatch(InputEvent::PointerLeave);
    assert_eq!(fill(&ui, b), Color(0xe8, 0xeb, 0xf0, 255));
    // A variant not in effect during a switch resolves when it comes into
    // effect: back to dark, then hovered again.
    ui.theme().set(DARK).unwrap();
    ui.dispatch(InputEvent::PointerMove {
        x: bounds.x + 4.,
        y: bounds.y + 4.,
    });
    assert_eq!(fill(&ui, b), Color(47, 64, 88, 255));
    ui.dispatch(InputEvent::PointerLeave);
    ui.theme().set(LIGHT).unwrap();
    // The imperative widgets follow the theme too.
    let root = ui.root();
    let legacy = ui.button(root, "Legacy", 100., || {});
    let bg = ui.scene.borrow().children(legacy)[0];
    assert_eq!(fill(&ui, bg), Color(0xe8, 0xeb, 0xf0, 255));
    ui.theme().set(DARK).unwrap();
    assert_eq!(fill(&ui, bg), Color(35, 46, 64, 255));
}

#[test]
fn text_and_spans_follow_the_theme() {
    let mut ui = Ui::new(400., 400.);
    let handle = ui.mount(
        column()
            .child(text("Body").id("body"))
            .child(text("Muted").id("muted").text_color(tok::TEXT_SECONDARY))
            .child(rich_text_signal(|| {
                vec![text_span("token").text_color(tok::ACCENT)]
            })),
    );
    ui.scene.borrow_mut().flush();
    let body = handle.find("body").unwrap();
    let muted = handle.find("muted").unwrap();
    assert_eq!(text_color(&ui, body), Color(229, 237, 247, 255));
    assert_eq!(text_color(&ui, muted), Color(138, 154, 176, 255));
    ui.theme().set(LIGHT).unwrap();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(text_color(&ui, body), Color(0x1a, 0x1f, 0x29, 255));
    assert_eq!(text_color(&ui, muted), Color(0x5a, 0x64, 0x74, 255));
    assert_eq!(report.layout_nodes, 0, "a colour change reshapes nothing");
}

#[test]
fn the_system_appearance_switches_a_following_theme() {
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    themes.follow_system(Some((DARK, LIGHT)));
    assert_eq!(&*themes.current(), DARK);
    themes.set_system_appearance(Appearance::Light);
    assert_eq!(&*themes.current(), LIGHT);
    // A default does not override following, nor an explicit choice.
    themes.set_default(DARK).unwrap();
    assert_eq!(&*themes.current(), LIGHT);
    themes.set(DARK).unwrap();
    assert!(!themes.follows_system());
    themes.set_system_appearance(Appearance::Dark);
    themes.set_system_appearance(Appearance::Light);
    assert_eq!(&*themes.current(), DARK);
    themes.set_default(LIGHT).unwrap();
    assert_eq!(&*themes.current(), DARK, "the app chose");
}

#[test]
fn theme_files_hot_reload_in_place() {
    let dir = std::env::temp_dir().join(format!("zgui-theme-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test-reload.json");
    let write = |accent: &str| {
        std::fs::write(
            &path,
            format!(
                r#"{{ "name": "test-reload", "type": "dark", "tokens": {{ "accent": "{accent}" }} }}"#
            ),
        )
        .unwrap();
        // Make sure the change is visible to a coarse timestamp.
        let later = std::time::SystemTime::now() + Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
    };
    write("#ff0000");
    let mut ui = Ui::new(100., 100.);
    assert_eq!(ui.theme().load(&path).unwrap(), "test-reload");
    ui.theme().set("test-reload").unwrap();
    let handle = ui.mount(div().id("a").size(10., 10.).bg(tok::ACCENT));
    ui.scene.borrow_mut().flush();
    let node = handle.find("a").unwrap();
    assert_eq!(fill(&ui, node), RED);
    assert_eq!(ui.theme().reload_if_changed(), Ok(false));
    std::thread::sleep(Duration::from_millis(5));
    write("#00ff00");
    assert_eq!(ui.theme().reload_if_changed(), Ok(true));
    assert_eq!(fill(&ui, node), GREEN);
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    // A file that stops reading keeps its last good theme.
    std::fs::write(&path, "{ not json").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(4))
        .unwrap();
    assert!(ui.theme().reload_if_changed().is_err());
    assert_eq!(fill(&ui, node), GREEN);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn vscode_themes_import_onto_tokens() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vscode");
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    // VS Code's own Dark Modern, which includes Dark+, which includes Dark (VS).
    let name = themes.load(dir.join("dark_modern.json")).unwrap();
    assert_eq!(name, "Dark Modern");
    themes.set(&name).unwrap();
    assert_eq!(themes.kind(), ThemeKind::Dark);
    let hex = |token| zgui::theme::hex(themes.color(token));
    assert_eq!(hex(tok::BACKGROUND), "#1f1f1f", "editor.background");
    assert_eq!(hex(tok::TEXT_PRIMARY), "#cccccc", "editor.foreground");
    assert_eq!(hex(tok::TEXT_SECONDARY), "#9d9d9d", "descriptionForeground");
    assert_eq!(
        hex(tok::TEXT_TERTIARY),
        "#6e7681",
        "editorLineNumber.foreground"
    );
    assert_eq!(hex(tok::FOCUS_RING), "#0078d4", "focusBorder");
    assert_eq!(hex(tok::ACCENT), "#0078d4", "button.background");
    assert_eq!(hex(tok::SURFACE_BASE), "#181818", "sideBar.background");
    assert_eq!(hex(tok::INPUT_BG), "#313131", "input.background");
    assert_eq!(hex(tok::DANGER), "#f85149", "errorForeground");
    // A colour it clears (`button.secondaryBackground: #00000000`) leaves
    // the button to the next candidate: the raised surface.
    assert_eq!(
        themes.color(tok::BUTTON_BG),
        themes.color(tok::SURFACE_RAISED)
    );
    // Dark+'s colours all come from what it includes.
    let plus = themes.load(dir.join("dark_plus.json")).unwrap();
    themes.set(&plus).unwrap();
    assert_eq!(hex(tok::BACKGROUND), "#1e1e1e");
    assert_eq!(hex(tok::MENU_BG), "#252526");
    let light = themes.load(dir.join("light_modern.json")).unwrap();
    themes.set(&light).unwrap();
    assert_eq!(themes.kind(), ThemeKind::Light, "inferred from the editor");
    assert_eq!(
        hex(tok::SELECTION_ITEM),
        "#e8e8e8",
        "list.activeSelectionBackground"
    );
    // What VS Code has no colour for comes from zgui's theme of its type.
    assert_eq!(themes.number(tok::RADIUS_MD), 6.);
    // Converted, it is an ordinary zgui theme file.
    let theme = themes.theme(&light).unwrap();
    let again = Theme::from_json(&theme.to_json()).unwrap();
    assert_eq!(again.get(tok::SELECTION_ITEM), Some("#e8e8e8"));
    // Without a loader, an `include` is an error rather than a silent gap.
    let text = std::fs::read_to_string(dir.join("light_modern.json")).unwrap();
    assert!(Theme::from_vscode(&text).is_err());
    let read = |include: &str| {
        std::fs::read_to_string(dir.join(include))
            .map_err(|e| zgui::theme::ThemeError(e.to_string()))
    };
    assert!(Theme::from_vscode_with(&text, read).is_ok());
}

#[test]
fn interaction_styles_that_change_swap_and_follow_the_theme() {
    use zgui::style::Styles;
    let mut ui = Ui::new(200., 200.);
    let raised = ui.signal(false);
    let (read, mut last) = (raised.clone(), None);
    let handle = ui.mount(
        column().child(
            div()
                .id("tab")
                .size(40., 40.)
                .bg(GREEN)
                .reactive_interaction(move || {
                    let now = read.get();
                    if last.replace(now) == Some(now) {
                        return None;
                    }
                    let hover = if now {
                        Styles::new().bg(tok::SURFACE_RAISED)
                    } else {
                        Styles::new().bg(RED)
                    };
                    Some([Some(hover), None, None, None])
                }),
        ),
    );
    ui.scene.borrow_mut().flush();
    let node = handle.find("tab").unwrap();
    assert_eq!(fill(&ui, node), GREEN);
    ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
    assert_eq!(fill(&ui, node), RED, "the first hover style");
    // Swapped while hovered: the new one shows, its token resolved.
    raised.set(true);
    assert_eq!(fill(&ui, node), Color(35, 46, 64, 255));
    ui.theme().set(LIGHT).unwrap();
    assert_eq!(
        fill(&ui, node),
        Color(255, 255, 255, 255),
        "follows the theme"
    );
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0, "paint only");
    ui.dispatch(InputEvent::PointerLeave);
    assert_eq!(fill(&ui, node), GREEN);
    raised.set(false);
    ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
    assert_eq!(fill(&ui, node), RED, "swapped while not in effect");
}

#[test]
fn system_tokens_apply_over_built_in_themes_and_under_the_apps() {
    let runtime = Runtime::new();
    let themes = Themes::of(&runtime);
    let red = Color(255, 0, 0, 255);
    let builtin = themes.root().color(tok::SELECTION);
    themes.set_system_tokens([(tok::SELECTION.0, red.into())]);
    assert_eq!(
        themes.root().color(tok::SELECTION),
        red,
        "the built-in yields"
    );
    let scoped = themes
        .root()
        .child(ThemeOverride::new().with(tok::SELECTION, Color(0, 255, 0, 255)));
    assert_eq!(
        scoped.color(tok::SELECTION),
        Color(0, 255, 0, 255),
        "a scope keeps its own"
    );
    themes.register(
        Theme::from_json(
            r##"{ "name": "app", "type": "dark", "tokens": { "selection": "#0000ff" } }"##,
        )
        .unwrap(),
    );
    themes.set("app").unwrap();
    assert_eq!(
        themes.root().color(tok::SELECTION),
        Color(0, 0, 255, 255),
        "an app theme keeps its own"
    );
    themes.set(DARK).unwrap();
    assert_eq!(themes.root().color(tok::SELECTION), red);
    themes.set_system_tokens([]);
    assert_eq!(themes.root().color(tok::SELECTION), builtin);
}
