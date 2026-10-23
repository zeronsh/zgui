//! Themes from scripts: `"$token"` values, `ui.theme`, app tokens, scoped
//! `theme = …` and theme files that reload in place.
mod common;

use common::Harness;
use std::time::Duration;
use zgui::scene::{Color, NodeKind};
use zgui_script::ScriptHost;

impl Harness {
    fn fill(&mut self, id: &str) -> Color {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        match self.ui.scene.borrow().kind(node) {
            NodeKind::Panel { quad, .. } => quad.fill,
            NodeKind::Container(_) => Color(0, 0, 0, 0),
            other => panic!("`{id}` is not a box: {other:?}"),
        }
    }
    fn radius(&mut self, id: &str) -> f32 {
        let node = self.find(id).unwrap();
        match self.ui.scene.borrow().kind(node) {
            NodeKind::Panel { quad, .. } => quad.radius,
            other => panic!("`{id}` is not a box: {other:?}"),
        }
    }
    fn ink(&mut self, id: &str) -> Color {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        let scene = self.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let NodeKind::Text { color, .. } = scene.kind(n) {
                return *color;
            }
            stack.extend(scene.children(n).iter().rev());
        }
        panic!("no text in `{id}`")
    }
}
const DARK_RAISED: Color = Color(35, 46, 64, 255);
const WHITE: Color = Color(255, 255, 255, 255);
const LIGHT_TEXT: Color = Color(0x1a, 0x1f, 0x29, 255);
const DARK_TEXT: Color = Color(229, 237, 247, 255);

#[test]
fn token_values_style_nodes_and_follow_the_theme() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.div { id = "card", w = 40, h = 20, bg = "$surface.raised", rounded = "$radius.lg", p = "$space.2" },
            ui.text { "Muted", id = "muted", color = "$text.secondary" },
            ui.text { "Body", id = "body" },
            ui.button { id = "light", w = 20, h = 20, on_click = function() ui.theme.set("zgui-light") end },
        }"##,
    );
    assert_eq!(h.fill("card"), DARK_RAISED);
    assert_eq!(h.radius("card"), 10.);
    assert_eq!(h.ink("muted"), Color(138, 154, 176, 255));
    assert_eq!(h.ink("body"), DARK_TEXT);
    h.ui.scene.borrow_mut().flush();
    h.click("light");
    let report = h.ui.scene.borrow_mut().flush();
    assert_eq!(h.fill("card"), WHITE);
    assert_eq!(h.ink("body"), LIGHT_TEXT);
    assert_eq!(h.ink("muted"), Color(0x5a, 0x64, 0x74, 255));
    assert_eq!(report.layout_nodes, 0, "colours need no layout");
}

#[test]
fn app_tokens_and_luau_themes() {
    let mut h = Harness::inline(
        r##"ui.theme.tokens {
            ["app.card"] = "{surface.raised}",
            app = { badge = "alpha({accent}, 0.5)" },
        }
        ui.theme.register {
            name = "app-sepia",
            type = "light",
            tokens = { ["app.card"] = "#f4ecd8", accent = "#8a5a00" },
        }
        local accent = ui.theme.get("accent")
        return ui.column {
            ui.div { id = "card", w = 10, h = 10, bg = "$app.card" },
            ui.div { id = "badge", w = 10, h = 10, bg = "$app.badge" },
            ui.div { id = "unknown", w = 10, h = 10, bg = "$app.nowhere" },
            ui.text { accent, id = "accent" },
            ui.button { id = "sepia", w = 20, h = 20, on_click = function() ui.theme.set("app-sepia") end },
        }"##,
    );
    assert_eq!(h.fill("card"), DARK_RAISED, "declared in standard terms");
    assert_eq!(h.fill("badge"), Color(0x5e, 0xa5, 0xff, 128));
    assert_eq!(
        h.fill("unknown"),
        Color(0, 0, 0, 0),
        "unknown tokens are clear"
    );
    assert_eq!(h.text("accent"), "#5ea5ffff");
    h.click("sepia");
    assert_eq!(
        h.fill("card"),
        Color(0xf4, 0xec, 0xd8, 255),
        "the theme sets it"
    );
    assert_eq!(
        h.fill("badge"),
        Color(0x8a, 0x5a, 0x00, 128),
        "derived again"
    );
}

#[test]
fn scoped_themes_restyle_their_subtrees() {
    let mut h = Harness::inline(
        r##"local light = ui.state("light", true)
        return ui.column {
            ui.div { id = "outer", w = 10, h = 10, bg = "$surface.raised" },
            ui.column {
                theme = "zgui-light",
                ui.div { id = "card", w = 10, h = 10, bg = "$surface.raised" },
                ui.text { "Inside", id = "inside" },
            },
            ui.div {
                theme = { accent = "#e5484d" },
                ui.div { id = "ring", w = 10, h = 10, bg = "$focus.ring" },
            },
            ui.div {
                theme = function() return if light:get() then "zgui-light" else "zgui-dark" end,
                ui.div { id = "switch", w = 10, h = 10, bg = "$background" },
            },
            ui.button { id = "toggle", w = 20, h = 20, on_click = ui.toggle(light) },
        }"##,
    );
    assert_eq!(h.fill("outer"), DARK_RAISED);
    assert_eq!(h.fill("card"), WHITE);
    assert_eq!(
        h.ink("inside"),
        LIGHT_TEXT,
        "text inherits the scope's body colour"
    );
    assert_eq!(
        h.fill("ring"),
        Color(0xe5, 0x48, 0x4d, 255),
        "derived from the scope's accent"
    );
    assert_eq!(h.fill("switch"), Color(0xf5, 0xf6, 0xf8, 255));
    h.click("toggle");
    assert_eq!(h.fill("switch"), Color(16, 20, 28, 255));
}

#[test]
fn spans_take_tokens() {
    let mut h = Harness::inline(
        r##"return ui.rich {
            id = "rich",
            ui.span { "plain" },
            ui.span { "accent", color = "$accent", bg = "$selection" },
        }"##,
    );
    assert!(h.find("rich").is_some());
    h.ui.scene.borrow_mut().flush();
    h.ui.theme().set(zgui::theme::LIGHT).unwrap();
    h.settle();
    let report = h.ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0, "{report:?}");
}

#[test]
fn theme_files_reload_without_reloading_scripts() {
    let dir = std::env::temp_dir().join(format!("zgui-script-theme-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let theme = dir.join("brand.json");
    let write = |accent: &str, offset: u64| {
        std::fs::write(
            &theme,
            format!(
                r#"{{ "name": "brand", "type": "dark", "tokens": {{ "accent": "{accent}" }} }}"#
            ),
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&theme)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + Duration::from_secs(offset))
            .unwrap();
    };
    write("#ff0000", 0);
    std::fs::write(
        dir.join("app.luau"),
        r##"local count = ui.state("count", 0)
        ui.theme.set(ui.theme.load("brand.json"))
        return ui.column {
            ui.div { id = "accent", w = 10, h = 10, bg = "$accent" },
            ui.button { id = "add", w = 20, h = 20, on_click = ui.add(count) },
            ui.text { "{count}", id = "count" },
        }"##,
    )
    .unwrap();
    let path = dir.join("app.luau");
    let mut h = Harness::new(|runtime| ScriptHost::new(runtime, &path));
    assert_eq!(h.fill("accent"), Color(255, 0, 0, 255));
    h.click("add");
    assert_eq!(h.text("count"), "1");
    let before = h.find("accent").unwrap();
    write("#00ff00", 2);
    assert!(h.host.reload_if_changed());
    // Restyled in place: the same node, the state where it was.
    assert_eq!(h.find("accent"), Some(before));
    assert_eq!(h.fill("accent"), Color(0, 255, 0, 255));
    assert_eq!(h.text("count"), "1");
    // A file that stops reading shows its error and keeps the last theme.
    std::fs::write(&theme, "{ nope").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&theme)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(4))
        .unwrap();
    assert!(h.host.reload_if_changed());
    assert!(
        h.host
            .error()
            .get()
            .is_some_and(|e| e.contains("brand.json"))
    );
    assert_eq!(h.fill("accent"), Color(0, 255, 0, 255));
    write("#0000ff", 6);
    assert!(h.host.reload_if_changed());
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.fill("accent"), Color(0, 0, 255, 255));
    // Theme files stay inside the script's directory.
    std::fs::write(
        dir.join("app.luau"),
        r##"ui.theme.load("../elsewhere.json") return ui.div {}"##,
    )
    .unwrap();
    assert!(h.host.reload().is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn defaults_survive_reloads_but_not_a_choice() {
    let h = Harness::inline(
        r##"ui.theme.default("zgui-light")
        return ui.div {}"##,
    );
    assert_eq!(&*h.ui.theme().current(), "zgui-light");
    h.ui.theme().set(zgui::theme::DARK).unwrap();
    h.host.reload().unwrap();
    assert_eq!(
        &*h.ui.theme().current(),
        "zgui-dark",
        "the app's choice stands"
    );
}

#[test]
fn the_current_theme_is_state() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.text { function() return ui.theme.current:get() .. " " .. ui.theme.kind() end, id = "current" },
            ui.button { id = "light", w = 20, h = 20, on_click = function() ui.theme.current:set("zgui-light") end },
            ui.button { id = "bad", w = 20, h = 20, on_click = function() ui.theme.current:set("no-such") end },
        }"##,
    );
    assert_eq!(h.text("current"), "zgui-dark dark");
    h.ui.theme().set("zgui-light").unwrap();
    h.settle();
    assert_eq!(h.text("current"), "zgui-light light", "Rust's switch shows");
    h.ui.theme().set("zgui-dark").unwrap();
    h.click("light");
    assert_eq!(
        &*h.ui.theme().current(),
        "zgui-light",
        "the script's applies"
    );
    h.click("bad");
    assert_eq!(&*h.ui.theme().current(), "zgui-light");
    assert_eq!(h.text("current"), "zgui-light light");
}

/// Every fill and text colour in the scene.
fn colors(ui: &zgui::widgets::Ui) -> Vec<Color> {
    let scene = ui.scene.borrow();
    let mut out = Vec::new();
    let mut stack = vec![scene.root()];
    while let Some(id) = stack.pop() {
        match scene.kind(id) {
            NodeKind::Panel { quad, .. } => out.push(quad.fill),
            NodeKind::Text { color, .. } => out.push(*color),
            _ => {}
        }
        stack.extend(scene.children(id).iter().copied());
    }
    out
}

#[test]
fn the_notes_example_keeps_its_look_and_takes_any_theme() {
    let app = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/notes/app.luau");
    let h = Harness::new(|runtime| ScriptHost::new(runtime, app));
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    assert_eq!(&*h.ui.theme().current(), "notes-glass");
    let glass = colors(&h.ui);
    for (color, what) in [
        (Color(0x12, 0x14, 0x16, 0x6b), "the window's glass"),
        (Color(0x1b, 0x1c, 0x20, 0xd9), "the editor"),
        (Color(0x1b, 0x1c, 0x20, 0xf0), "the preview"),
        (Color(0xff, 0xff, 0xff, 0x10), "the search field"),
        (Color(0xff, 0xff, 0xff, 0xeb), "text"),
        (Color(0xf5, 0xb7, 0x0a, 0x3d), "the selected note"),
    ] {
        assert!(glass.contains(&color), "{what}: {color:?} not in {glass:?}");
    }
    h.ui.theme().set(zgui::theme::LIGHT).unwrap();
    h.settle();
    h.ui.prepare_frame();
    let light = colors(&h.ui);
    assert!(light.contains(&WHITE), "the editor on the light base");
    assert!(light.contains(&LIGHT_TEXT));
    assert!(!light.contains(&Color(0xff, 0xff, 0xff, 0xeb)));
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn ui_prefers_follows_the_systems_display_settings() {
    use zgui::theme::SystemPreferences;
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.text { function()
                local p = ui.prefers
                return `{p.reduced_motion:get()} {p.more_contrast:get()} {p.reduced_transparency:get()}`
            end, id = "p" } }"##,
    );
    h.frames(2);
    assert_eq!(h.text("p"), "false false false");
    h.ui.theme().set_system_preferences(SystemPreferences {
        reduced_motion: true,
        increased_contrast: false,
        reduced_transparency: true,
    });
    h.frames(1);
    assert_eq!(h.text("p"), "true false true", "as the system changes");
}
