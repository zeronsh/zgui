//! The `ui` library's breadth: components, derived state, every widget
//! kind, layout lengths and input events.
mod common;

use common::Harness;
use std::{cell::RefCell, rc::Rc};
use zgx::{
    input::{InputEvent, Key, Modifiers},
    scene::NodeKind,
};

impl Harness {
    fn kind(&mut self, id: &str) -> String {
        let node = self.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
        let scene = self.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            match scene.kind(n) {
                NodeKind::Container(_) | NodeKind::Panel { .. } => {
                    stack.extend(scene.children(n).iter().rev())
                }
                other => return format!("{other:?}"),
            }
        }
        "empty".into()
    }
    fn width(&mut self, id: &str) -> f32 {
        let node = self.find(id).unwrap();
        self.ui.scene.borrow().bounds(node).width
    }
    fn count_text(&mut self, prefix: &str) -> usize {
        self.ui.prepare_frame();
        let scene = self.ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut n = 0;
        while let Some(id) = stack.pop() {
            if let NodeKind::Text { text, .. } = scene.kind(id)
                && text.starts_with(prefix)
            {
                n += 1;
            }
            stack.extend(scene.children(id).iter().copied());
        }
        n
    }
}

#[test]
fn computed_text_formats_as_state_text_does() {
    let mut h = Harness::inline(
        r#"local values = {
            "", "hello", "\0", "\xffa\xc0\xaf\xe2\x82", "e\u{301} 😀",
            string.rep("x", 65535), string.rep("y", 65536), string.rep("z", 65537),
            true, false, 0, -123, 1.25, math.huge, 0/0,
            {"a", 2, false}, {b = "two", a = 1},
        }
        local children = {}
        for i, value in values do
            local state = ui.state("case" .. i, value)
            table.insert(children, ui.text { id = "state" .. i, state })
            table.insert(children, ui.text { id = "computed" .. i, function() return value end })
        end
        return ui.column { children }"#,
    );
    for i in 1..=17 {
        assert_eq!(
            h.text(&format!("computed{i}")),
            h.text(&format!("state{i}")),
            "case {i}"
        );
    }
}

#[test]
fn components_have_their_own_state_and_mount_hooks() {
    let mut h = Harness::inline(
        r##"local mounted = ui.state("mounted", 0)
        local Counter = ui.component(function(props)
            local n = ui.use_state(props.start or 0)
            ui.on_mount(function() mounted:add(1) end)
            return ui.column {
                ui.text { function() return props.name .. " " .. n:get() end, id = props.name },
                ui.button { id = props.name .. "+", w = 30, h = 20, on_click = function() n:add(1) end },
            }
        end)
        return ui.column {
            Counter { name = "a" }, Counter { name = "b", start = 10 },
            ui.text { "{mounted}", id = "mounted" },
        }"##,
    );
    assert_eq!(h.text("mounted"), "2");
    h.click("a+");
    h.click("a+");
    h.click("b+");
    assert_eq!(h.text("a"), "a 2");
    assert_eq!(h.text("b"), "b 11");
}

#[test]
fn a_scripts_own_mount_hook_runs_as_its_interface_mounts() {
    let mut h = Harness::inline(
        r##"local mounted = ui.state("mounted", 0)
        local seen = ui.state("seen", "")
        local late = ui.ref()
        ui.on_mount(function()
            mounted:add(1)
            -- The interface is there: its nodes are mounted.
            seen:set(if late:peek() then "found" else "missing")
        end)
        return ui.column {
            ui.text { "{mounted}", id = "mounted" },
            ui.text { "{seen}", id = "late", ref = late },
        }"##,
    );
    assert_eq!(h.text("mounted"), "1");
    assert_eq!(h.text("late"), "found");
    h.frames(3);
    assert_eq!(h.text("mounted"), "1", "once");
    h.host.reload().unwrap();
    h.frames(2);
    assert_eq!(h.text("mounted"), "2", "again for a new load");
}

#[test]
fn a_modal_builds_its_content_each_time_it_opens() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", false)
        local builds = ui.state("builds", 0)
        local Field = ui.component(function()
            local typed = ui.use_state("")
            return ui.input { id = "field", value = typed }
        end)
        return ui.column {
            ui.button { id = "open", w = 30, h = 20, on_click = function() open:set(true) end },
            ui.text { "{builds}", id = "builds" },
            ui.modal {
                open = open,
                build = function()
                    builds:add(1)
                    return Field {}
                end,
            },
        }"##,
    );
    assert_eq!(h.text("builds"), "0", "not until it opens");
    h.click("open");
    h.frames(1);
    assert_eq!(h.text("builds"), "1");
    h.type_text("field", "typed");
    assert_eq!(h.text("field"), "typed");
    h.key(zgx::input::Key::Escape);
    h.frames(1);
    assert!(!h.shown("field"), "closed, kept unseen");
    assert_eq!(h.text("builds"), "1");
    h.click("open");
    h.frames(1);
    assert_eq!(h.text("builds"), "2");
    assert!(h.shown("field"));
    assert_eq!(h.text("field"), "", "its state new");
    assert_eq!(h.error(), None);
}

#[test]
fn computed_state_follows_its_inputs() {
    let mut h = Harness::inline(
        r##"local a = ui.state("a", 2)
        local b = ui.state("b", 3)
        local sum = ui.computed(function() return a:get() + b:get() end)
        return ui.column {
            ui.text { function() return "sum " .. sum:get() end, id = "sum" },
            ui.button { id = "inc", w = 20, h = 20, on_click = ui.add(a, 5) },
        }"##,
    );
    assert_eq!(h.text("sum"), "sum 5");
    h.click("inc");
    assert_eq!(h.text("sum"), "sum 10");
}

#[test]
fn rich_text_svg_and_canvas_render() {
    let mut h = Harness::inline(
        r##"local name = ui.state("name", "Ada")
        return ui.column {
            ui.rich { id = "rich", "Hello ", ui.span { "{name}", bold = true, color = "#f80" }, "!" },
            ui.svg { id = "svg", code = "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>", w = 10, h = 10 },
            ui.canvas { id = "canvas", w = 40, h = 40, draw = function(c, w, h)
                c:rect(0, 0, w, h, "#222", 6)
                c:stroke(ui.path():move(0, 0):line(w, h), "#fff", 2)
                c:circle(w / 2, h / 2, 5, ui.linear(0, 0, w, 0, { "#f00", "#00f" }))
            end },
        }"##,
    );
    assert!(h.kind("rich").contains("Hello Ada!"), "{}", h.kind("rich"));
    assert!(h.kind("svg").starts_with("Svg"), "{}", h.kind("svg"));
    let canvas = h.kind("canvas");
    assert!(
        canvas.starts_with("Canvas") && canvas.contains("Stroke") && canvas.contains("Linear"),
        "{canvas}"
    );
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn virtual_lists_mount_only_visible_rows() {
    let mut h = Harness::inline(
        r##"return ui.list { id = "list", count = 100000, height = 20, w = 200, h = 200,
            row = function(i) return ui.text("row " .. i) end }"##,
    );
    let rows = h.count_text("row ");
    assert!(rows > 5 && rows < 30, "{rows} rows mounted of 100,000");
}

#[test]
fn lists_and_scrolls_are_named_by_their_label() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.list { id = "list", role = "listbox", label = "Messages", count = 3, w = 200, h = 100,
                row = function(i) return ui.text("row " .. i) end },
            ui.scroll { id = "view", label = "Message body", w = 200, h = 100, ui.text("body") },
        }"##,
    );
    for (id, label, role) in [
        ("list", "Messages", zgx::semantics::Role::ListBox),
        ("view", "Message body", zgx::semantics::Role::ScrollView),
    ] {
        let node = h.find(id).unwrap();
        let semantics = h.ui.semantics.borrow();
        let node = semantics.get(node).unwrap();
        assert_eq!((node.label.as_str(), node.role), (label, role));
    }
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn script_scrolls_fade_only_where_content_runs_past() {
    let mut h = Harness::inline(
        r##"local off = ui.state("off", 0)
        return ui.scroll { id = "view", w = 200, h = 100, offset = off, fade = { 14, 18 }, fade_overflow = true,
            ui.column { h = 300, ui.div { id = "row", h = 20 } } }"##,
    );
    let bands = |h: &mut Harness| {
        h.ui.prepare_frame();
        h.ui.scene.borrow_mut().flush();
        let node = h.find("row").unwrap();
        let scene = h.ui.scene.borrow();
        scene
            .layer_items(None)
            .into_iter()
            .find(|item| item.id == node)
            .and_then(|item| item.mask().map(|(_, bands)| bands))
    };
    assert_eq!(bands(&mut h), Some([0., 0., 18., 0.]));
}

#[test]
fn inputs_take_letter_spacing() {
    let mut h = Harness::inline(
        r##"local v = ui.state("v", "Weekly dependency updates")
        return ui.input { id = "i", value = v, w = 300, letter_spacing = 3, text_size = 14 }"##,
    );
    h.ui.prepare_frame();
    let node = h.find("i").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = None;
    while let Some(n) = stack.pop() {
        if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n)
            && text.starts_with("Weekly")
        {
            found = Some(scene.font(n).letter_spacing);
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert_eq!(found, Some(zgx::text_layout::LetterSpacing::px(3.)));
}

#[test]
fn menus_open_where_placed() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", true)
        return ui.row { p = 100,
            ui.menu { open = open, w = 120, placement = { align = "end", offset = 8 },
                anchor = ui.button { id = "t", w = 40, h = 20, "x" },
                ui.menu_item { id = "first", "One" } } }"##,
    );
    h.frames(3);
    h.ui.prepare_frame();
    let bounds = |h: &mut Harness, id: &str| {
        let n = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(n)
    };
    let (t, item) = (bounds(&mut h, "t"), bounds(&mut h, "first"));
    // The panel's right edge on the trigger's (its items inset by padding).
    assert!(
        item.x + item.width <= t.x + t.width + 0.5 && item.x + item.width > t.x + t.width - 40.,
        "{t:?} {item:?}"
    );
    assert!(item.y >= t.y + t.height + 8., "{t:?} {item:?}");
}

#[test]
fn outlines_take_width_colour_and_offset() {
    let mut h = Harness::inline(
        r##"return ui.div { id = "d", w = 20, h = 20, bg = "#101010",
            outline = 1, outline_color = "#ff0000", outline_offset = -1 }"##,
    );
    h.ui.prepare_frame();
    let node = h.find("d").unwrap();
    let scene = h.ui.scene.borrow();
    let outline = match scene.kind(node) {
        zgx::scene::NodeKind::Quad(q) | zgx::scene::NodeKind::Panel { quad: q, .. } => q.outline,
        other => panic!("{other:?}"),
    }
    .expect("an outline");
    assert_eq!(
        (outline.width, outline.color, outline.offset),
        (1., zgx::scene::Color(255, 0, 0, 255), -1.)
    );
}

#[test]
fn a_closed_modal_in_an_overlay_blocks_no_clicks() {
    let mut h = Harness::inline(
        r##"local n = ui.state("n", 0)
        return ui.overlay { w = 200, h = 200,
            ui.button { id = "b", w = 50, h = 30, on_click = function() n:set(n:peek() + 1) end, "{n}" },
            ui.overlay { absolute = true, top = 0, left = 0, right = 0, bottom = 0, pointer_events = "none",
                ui.modal { open = ui.state("o", false), ui.text { "x" } } } }"##,
    );
    h.frames(2);
    h.click("b");
    h.frames(2);
    assert_eq!(h.text("b"), "1");
}

#[test]
fn modals_sit_where_placed() {
    // A side sheet: against the window's right edge, from its top, 8 in.
    let mut h = Harness::inline(
        r##"return ui.column { w_full = true, h_full = true,
            ui.modal { open = ui.state("o", true), w = 120, h = 100,
                placement = { x = "end", y = "start", inset = 8 },
                ui.text { "sheet", id = "t" } } }"##,
    );
    h.frames(3);
    h.ui.prepare_frame();
    let node = h.find("t").unwrap();
    let scene = h.ui.scene.borrow();
    let b = scene.transformed_bounds(node);
    // The harness window is 400 × 300: the panel's right edge at 392, top at 8.
    assert!(
        b.x >= 392. - 120. - 0.5 && b.x < 392. && b.y >= 8. && b.y < 40.,
        "{b:?}"
    );
}

#[test]
fn submenus_take_their_own_trigger_rows_and_open_on_hover() {
    let mut h = Harness::inline(
        r##"local sub = ui.state("sub", false)
        return ui.column { p = 20,
            ui.menu { open = ui.state("m", true), w = 160,
                anchor = ui.button { id = "a", w = 20, h = 20, "a" },
                ui.menu_item { id = "plain", "Plain" },
                ui.submenu { open = sub, label = "Fork",
                    trigger = ui.menu_item { id = "fork", label = "Fork",
                        ui.text { "Fork row", id = "fork-text" }, ui.text { "››" } },
                    ui.menu_item { id = "inner", "Inner" } } } }"##,
    );
    let shown = |h: &mut Harness, id: &str| {
        h.ui.prepare_frame();
        let Some(node) = h.find(id) else { return false };
        h.ui.scene
            .borrow()
            .visible_bounds(node)
            .is_some_and(|b| b.width > 0.)
    };
    h.frames(3);
    assert!(shown(&mut h, "fork-text"), "the app's own trigger row");
    assert!(!shown(&mut h, "inner"), "closed");
    let node = h.find("fork").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 5.,
        y: b.y + 5.,
    });
    h.frames(3);
    assert!(shown(&mut h, "inner"), "opened by hovering");
    let plain = h.find("plain").unwrap();
    let b = h.ui.scene.borrow().bounds(plain);
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 5.,
        y: b.y + 5.,
    });
    h.frames(3);
    assert!(!shown(&mut h, "inner"), "closed by hovering another item");
}

#[test]
fn presses_count_their_clicks_and_keys_carry_their_physical_code() {
    use zgx::input::{InputEvent, Key, Modifiers, PointerButton};
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        return ui.column {
            ui.div { id = "pad", w = 40, h = 40, focusable = true,
                on_press = function(e) seen:set(seen:peek() .. e.clicks) end,
                on_key = function(e) seen:set(seen:peek() .. (e.code or "?")) return true end },
            ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frames(2);
    let b = h.bounds("pad");
    let (x, y) = (b.x + 5., b.y + 5.);
    for _ in 0..2 {
        h.ui.dispatch(InputEvent::PointerMove { x, y });
        // With modifiers, as a platform sends presses (they're counted).
        let none = Modifiers::default();
        h.ui.dispatch_with_modifiers(
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            },
            none,
        );
        h.ui.dispatch_with_modifiers(
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            },
            none,
        );
    }
    h.ui.input.set_key_code(Some("Digit1".into()));
    h.ui.dispatch(InputEvent::KeyDown {
        // Shift+1 types "!": the code is still the 1 key's.
        key: Key::Character("!".into()),
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    h.ui.input.set_key_code(None);
    h.frames(1);
    assert_eq!(h.text("seen"), "12Digit1");
}

#[test]
fn focus_styles_show_for_the_keyboard_not_a_click() {
    use zgx::input::{InputEvent, Key, Modifiers};
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.button { id = "a", w = 40, h = 20, bg = "#000000", focus = { bg = "#ff0000" }, "a" },
            ui.button { id = "b", w = 40, h = 20, bg = "#000000", focus = { bg = "#ff0000" }, "b" },
            ui.input { id = "field", w = 80, value = ui.state("v", ""), focus = { bg = "#ff0000" } } }"##,
    );
    h.frames(2);
    let ring = |h: &mut Harness, id: &str| {
        h.frames(1);
        let node = h.find(id).unwrap();
        format!("{:?}", h.ui.scene.borrow().kind(node)).contains("fill: Color(255, 0, 0")
    };
    h.click("a");
    assert_eq!(h.ui.input.focused(), h.find("a"), "a click focuses it");
    assert!(!ring(&mut h, "a"), "but shows no focus ring");
    let key = |h: &mut Harness, key: Key| {
        h.ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
    };
    key(&mut h, Key::Tab);
    assert_eq!(h.ui.input.focused(), h.find("b"));
    assert!(ring(&mut h, "b"), "Tab's focus shows");
    h.click("a");
    assert!(!ring(&mut h, "a"));
    key(&mut h, Key::ArrowDown);
    assert!(
        ring(&mut h, "a"),
        "a key to the clicked button shows its focus"
    );
    h.click("field");
    assert!(
        ring(&mut h, "field"),
        "a text field shows focus however it got it"
    );
}

#[test]
fn a_hover_style_underlines_the_label_it_reaches() {
    let mut h = Harness::inline(
        r##"return ui.column { ui.button { id = "link", bg = "#00000000", underline_offset = 2,
            hover = { underline = true }, ui.text { "Read more", id = "label" } } }"##,
    );
    h.frames(2);
    let underline = |h: &mut Harness| {
        let label = h.find("label").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![label];
        while let Some(n) = stack.pop() {
            if matches!(scene.kind(n), zgx::scene::NodeKind::Text { .. }) {
                return scene.font(n).underline;
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no text")
    };
    assert_eq!(underline(&mut h), None, "not at rest");
    let b = h.bounds("link");
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 4.,
        y: b.y + 4.,
    });
    h.frames(2);
    assert_eq!(
        underline(&mut h).and_then(|u| u.offset()),
        Some(2.),
        "under the pointer, 2 points down"
    );
    h.ui.dispatch(zgx::input::InputEvent::PointerLeave);
    h.frames(2);
    assert_eq!(underline(&mut h), None);
}

#[test]
fn a_span_icon_sits_in_the_line_before_its_text() {
    use zgx::scene::NodeKind;
    let svg = r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'><rect width='12' height='12'/></svg>"#;
    let code = |icon: bool| {
        format!(
            r##"return ui.column {{ w = 300, ui.rich {{ id = "p",
                ui.span {{ "src/main.rs", literal = true, color = "#00ff00"{} }} }} }}"##,
            if icon {
                format!(
                    ", icon = {{ code = \"{svg}\", size = 12, gap = 4, tint = \"currentColor\" }}"
                )
            } else {
                String::new()
            }
        )
    };
    let measure = |icon: bool| {
        let mut h = Harness::inline(&code(icon));
        h.frames(2);
        let p = h.find("p").unwrap();
        let label = h.ui.semantics.borrow().get(p).map(|n| n.label.clone());
        let scene = h.ui.scene.borrow();
        let mut found = (None, 0.);
        let mut stack = vec![p];
        while let Some(n) = stack.pop() {
            match scene.kind(n) {
                NodeKind::Svg(svg) => found.0 = Some((scene.bounds(n), svg.clone())),
                NodeKind::RichText { text } => {
                    found.1 = scene.bounds(n).width;
                    assert!(!text.text().is_empty());
                }
                _ => {}
            }
            stack.extend(scene.children(n).iter().copied());
        }
        (found, scene.bounds(p), label)
    };
    let ((none, plain_w), _, _) = measure(false);
    assert!(none.is_none());
    let ((icon, iconed_w), p, label) = measure(true);
    let (bounds, svg) = icon.expect("the icon drawn");
    assert_eq!((bounds.width, bounds.height), (12., 12.));
    assert!(
        (bounds.x - p.x).abs() < 0.5,
        "at the line's start: {bounds:?} in {p:?}"
    );
    assert!(
        (iconed_w - plain_w - 16.).abs() < 0.5,
        "the text moves over by size + gap: {plain_w} → {iconed_w}"
    );
    assert!(svg.follows_text_color() || format!("{svg:?}").contains("tint"));
    assert!(
        label.is_none_or(|l| !l.contains('\u{2060}')),
        "no placeholder read out"
    );
}

#[test]
fn a_padded_span_wraps_its_icon_and_text() {
    use zgx::scene::NodeKind;
    let svg = r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'><rect width='12' height='12'/></svg>"#;
    let measure = |extra: &str| {
        let mut h = Harness::inline(&format!(
            r##"return ui.column {{ w = 300, ui.rich {{ id = "p",
                ui.span {{ "notes.rs", literal = true{extra} }} }} }}"##
        ));
        h.frames(2);
        let p = h.find("p").unwrap();
        let scene = h.ui.scene.borrow();
        let (mut icon_x, mut width) = (None, 0.);
        let mut stack = vec![p];
        while let Some(n) = stack.pop() {
            match scene.kind(n) {
                NodeKind::Svg(_) => icon_x = Some(scene.bounds(n).x - scene.bounds(p).x),
                NodeKind::RichText { .. } => width = scene.bounds(n).width,
                _ => {}
            }
            stack.extend(scene.children(n).iter().copied());
        }
        (icon_x, width)
    };
    let (_, plain) = measure("");
    let (x, chip) = measure(&format!(
        ", padding = {{ 4, 0 }}, icon = {{ code = \"{svg}\", size = 12, gap = 4 }}"
    ));
    assert_eq!(x.map(f32::round), Some(4.), "inside the padding before");
    assert!(
        (chip - plain - 24.).abs() < 0.5,
        "4 + 12 + 4 + text + 4: {plain} → {chip}"
    );
}

#[test]
fn a_span_says_how_its_underline_is_drawn() {
    use zgx::{
        rich_text::LineStyle,
        scene::{Color, NodeKind},
    };
    let mut h = Harness::inline(
        r##"ui.theme.tokens { app = { squiggle = "#ff0010" } }
        return ui.rich { id = "p", w = 300,
            ui.span { "typo", literal = true, underline_style = "wavy",
                underline_color = "$app.squiggle", underline_thickness = 2 },
            ui.span { " note", literal = true, underline_style = "dotted" },
            ui.span { " plain", literal = true, underline = true } }"##,
    );
    h.frames(2);
    let p = h.find("p").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![p];
    let text = loop {
        let n = stack.pop().expect("a paragraph");
        if let NodeKind::RichText { text } = scene.kind(n) {
            break text.clone();
        }
        stack.extend(scene.children(n).iter().copied());
    };
    let line = |k: usize| text.runs()[k].underline.expect("underlined");
    assert_eq!(line(0).line_style(), LineStyle::Wavy);
    assert_eq!(line(0).thickness(), 2.);
    assert_eq!(
        line(0).color,
        Some(Color(255, 0, 16, 255)),
        "a token resolved"
    );
    assert_eq!(
        line(1).line_style(),
        LineStyle::Dotted,
        "any of them underlines"
    );
    assert_eq!(line(2).line_style(), LineStyle::Solid);
}

#[test]
fn a_text_area_underlines_its_ranges_in_view_and_follows_them() {
    use zgx::scene::NodeKind;
    let mut h = Harness::inline(
        r##"local marks = ui.state("marks", { { 7, 11, "#ff0010" } })
        return ui.column {
            ui.button { id = "clear", on_click = function() marks:set({}) end, "Clear" },
            ui.button { id = "two", "Two", on_click = function()
                marks:set({ { from = 1, to = 5, color = "#ff0010", style = "dotted" }, { 7, 11 } })
            end },
            ui.textarea { id = "code", value = ui.state("text", "hello world"),
                w = 300, h = 100, underlines = marks },
        }"##,
    );
    h.frames(2);
    let canvases = |h: &mut Harness| {
        let node = h.find("code").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        let mut found = 0;
        while let Some(n) = stack.pop() {
            found += usize::from(matches!(scene.kind(n), NodeKind::Canvas(_)));
            stack.extend(scene.children(n).iter().copied());
        }
        found
    };
    assert_eq!(canvases(&mut h), 1, "a range, a row");
    h.click("two");
    h.frames(2);
    assert_eq!(canvases(&mut h), 2);
    h.click("clear");
    h.frames(2);
    assert_eq!(canvases(&mut h), 0, "and they go");
    assert_eq!(h.host.error().get(), None);
}

#[test]
#[should_panic(expected = "unknown `underline_style` `squiggly`")]
fn an_unknown_underline_style_says_which_there_are() {
    Harness::inline(r#"return ui.rich { ui.span { "a", underline_style = "squiggly" } }"#);
}

#[test]
fn a_box_takes_css_colour_filters() {
    let mut h = Harness::inline(
        r##"return ui.div { id = "icon", w = 20, h = 20, bg = "#ff0000",
            invert = 1, hue_rotate = 90, sepia = 0.5 }"##,
    );
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
    let icon = h.find("icon").unwrap();
    assert!(
        h.ui.scene.borrow().layer_shader(icon).is_some(),
        "drawn through its shader"
    );
}

#[test]
#[should_panic(expected = "two or three stops")]
fn a_radial_gradient_with_too_many_stops_says_so() {
    Harness::inline(
        r##"return ui.div { w = 20, h = 20, radial = { "#111", "#222", "#333", "#444" } }"##,
    );
}

#[test]
fn a_box_takes_a_radial_gradient() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.div { id = "a", w = 20, h = 20, radial = { "#ff0000", "#0000ff" } },
            ui.div { id = "b", w = 20, h = 20,
                radial = { center = { 0.5, 0 }, radius = 0.8, stops = { "#fff", { 0.3, "#888" }, "#0000" } } },
        }"##,
    );
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
    let a = h.find("a").unwrap();
    let scene = h.ui.scene.borrow();
    assert_eq!(scene.shader_uniforms(a).params[0], [0.5, 0.5, 1., 0.5]);
}

#[test]
fn an_image_takes_a_position() {
    let h = Harness::inline(
        r##"return ui.image { id = "photo", src = ui.bitmap(2, 8, function() return 1 end),
            w = 20, h = 20, fit = "cover", object_position = { 0.5, 0 } }"##,
    );
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn a_link_span_underlines_under_the_pointer() {
    use zgx::scene::NodeKind;
    let mut h = Harness::inline(
        r##"return ui.column { w = 300, ui.rich { id = "p",
            ui.span { "Read the ", literal = true },
            ui.span { "docs", literal = true, on_click = function() end,
                underline_offset = 2, hover = { underline = true, color = "#ff0000" } },
            ui.span { " and ", literal = true },
            ui.span { "plain", literal = true, on_click = function() end } } }"##,
    );
    h.frames(2);
    let runs = |h: &mut Harness| {
        let p = h.find("p").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![p];
        while let Some(n) = stack.pop() {
            if let NodeKind::RichText { text } = scene.kind(n) {
                return text
                    .runs()
                    .iter()
                    .map(|r| {
                        (
                            r.underline.and_then(|d| d.offset_px()),
                            r.underline.is_some(),
                            r.color,
                        )
                    })
                    .collect::<Vec<_>>();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no paragraph")
    };
    let rest = runs(&mut h);
    assert!(!rest[1].1, "a hover-styled link isn't underlined at rest");
    assert!(rest[3].1, "a plain link is");
    let p = h.find("p").unwrap();
    let b = h.ui.scene.borrow().bounds(p);
    // "Read the " is about 60 points wide at the default size: "docs" follows.
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 1.,
        y: b.y + b.height / 2.,
    });
    h.frames(2);
    assert!(!runs(&mut h)[1].1, "not over the link");
    let mut over = None;
    for x in (40..140).step_by(4) {
        h.ui.dispatch(zgx::input::InputEvent::PointerMove {
            x: b.x + x as f32,
            y: b.y + b.height / 2.,
        });
        h.frames(1);
        if runs(&mut h)[1].1 {
            over = Some(runs(&mut h)[1]);
            break;
        }
    }
    let (offset, _, color) = over.expect("underlined under the pointer");
    assert_eq!(offset, Some(2.));
    assert_eq!(color, zgx::scene::Color(255, 0, 0, 255));
    h.ui.dispatch(zgx::input::InputEvent::PointerLeave);
    h.frames(2);
    assert!(!runs(&mut h)[1].1, "back at rest once the pointer leaves");
}

/// `ui.each` in a `scroll_x` lays out across, with the viewport's gap and as
/// tall as a plain child there.
#[test]
fn each_in_a_scroll_x_lays_out_across() {
    let mut h = Harness::inline(
        r##"local names = ui.state("names", { "a", "b", "c" })
        return ui.column { w = 200, h = 300,
            ui.scroll_x { id = "s", h = 100, gap = 12,
                ui.each(names, function(n) return ui.column { id = n, w = 100, ui.text { n } } end) },
            ui.scroll_x { h = 100, ui.column { id = "plain", w = 100, ui.text { "p" } } } }"##,
    );
    h.ui.prepare_frame();
    let bounds = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    assert_eq!(bounds(&mut h, "a").x, 0.);
    assert_eq!(bounds(&mut h, "b").x, 112.);
    assert_eq!(bounds(&mut h, "c").x, 224.);
    let first = bounds(&mut h, "a");
    assert_eq!(bounds(&mut h, "c").y, first.y);
    let plain = bounds(&mut h, "plain");
    assert_eq!(first.height, plain.height);
}

/// A row of `ui.each` lays out as the same node on its own: it stretches
/// across its parent unless the parent's `items` says otherwise.
#[test]
fn each_rows_stretch_as_their_parents_children_do() {
    let mut h = Harness::inline(
        r##"local a = ui.state("a", { "in-column" })
        local b = ui.state("b", { "in-row" })
        local c = ui.state("c", { "centred" })
        local function row(n)
            return ui.row { id = n, ui.text { "x" }, ui.div { id = `{n}-fill`, grow = 1, h = 4 } }
        end
        return ui.column { w = 600, h = 400,
            ui.each(a, row),
            ui.row { h = 100, ui.each(b, function(n) return ui.column { id = n, w = 50 } end) },
            ui.column { items = "center", ui.each(c, function(n) return ui.column { id = n, h = 4, ui.text { "x" } } end) } }"##,
    );
    h.ui.prepare_frame();
    let bounds = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(node)
    };
    assert_eq!(bounds(&mut h, "in-column").width, 600.);
    assert!(bounds(&mut h, "in-column-fill").width > 500.);
    assert_eq!(bounds(&mut h, "in-row").height, 100.);
    assert!(
        bounds(&mut h, "centred").width < 100.,
        "the parent's `items` holds"
    );
}

/// In a row, a child of `each` takes the cross axis as a plain child does,
/// whatever the row's `items` says.
#[test]
fn each_rows_in_a_row_take_the_cross_axis_as_a_plain_child_does() {
    for items in [
        "",
        r#"items = "start","#,
        r#"items = "center","#,
        r#"items = "end","#,
    ] {
        let mut h = Harness::inline(&format!(
            r##"local names = ui.state("names", {{ "each" }})
            local function child(id)
                return ui.column {{ id = id, w = 50, ui.text {{ "x" }} }}
            end
            return ui.column {{ w = 600, h = 400,
                ui.row {{ h = 100, {items} child("plain") }},
                ui.row {{ h = 100, {items} ui.each(names, child) }} }}"##
        ));
        h.ui.prepare_frame();
        let bounds = |h: &mut Harness, id: &str| {
            let node = h.find(id).unwrap();
            h.ui.scene.borrow().bounds(node)
        };
        let (plain, each) = (bounds(&mut h, "plain"), bounds(&mut h, "each"));
        assert_eq!(each.height, plain.height, "{items}");
        assert_eq!(each.y - 100., plain.y, "{items}");
    }
}

/// A list's row fills the list's width, whatever it holds, as a block does:
/// fixed, variable and measured rows alike.
#[test]
fn list_rows_fill_the_width_of_the_list() {
    let mut h = Harness::inline(
        r##"local rows = ui.state("rows", { "short" })
        return ui.column { w = 600, h = 600,
            ui.list { w = 300, h = 80, height = 40, count = 1,
                row = function() return ui.column { id = "fixed", ui.text { "x" } } end },
            ui.list { w = 300, h = 80, heights = { 40 },
                row = function() return ui.column { id = "variable", ui.text { "x" } } end },
            ui.list { w = 300, h = 80, measured = true, height = 40, data = rows,
                key = function(r) return r end,
                row = function(r) return ui.column { id = "measured", ui.text { r } } end },
            ui.list { w = 300, h = 80, height = 40, count = 1,
                row = function() return ui.column { id = "sized", w = 120, ui.text { "x" } } end } }"##,
    );
    h.ui.prepare_frame();
    let width = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(node).width
    };
    assert_eq!(width(&mut h, "fixed"), 300.);
    assert_eq!(width(&mut h, "variable"), 300.);
    assert_eq!(width(&mut h, "measured"), 300.);
    assert_eq!(width(&mut h, "sized"), 120., "a row with a width keeps it");
}

#[test]
fn keys_reach_the_app_after_the_focused_node_goes() {
    let mut h = Harness::inline(
        r##"local shown = ui.shared("shown", true)
        local keys = ui.state("keys", 0)
        return ui.column { focusable = true, w = 200, h = 100,
            on_key = function(e) keys:set(keys:peek() + 1) return false end,
            ui.button { id = "hide", on_click = function() shown:set(false) end, "hide" },
            ui.text { id = "n", "{keys}" },
            ui.show(shown, ui.input { id = "i", value = ui.state("v", ""), w = 100,
                on_mount = function(el) el:focus() end }) }"##,
    );
    h.frames(2);
    let key = || zgx::input::InputEvent::KeyDown {
        key: zgx::input::Key::Character("k".into()),
        modifiers: Default::default(),
        repeat: false,
    };
    h.ui.dispatch(key());
    h.frames(1);
    assert_eq!(h.text("n"), "1", "from the input, bubbling up");
    // Hide the focused input (without clicking, which would move focus).
    h.host
        .shared("shown", zgx_script::Value::Nil)
        .set(zgx_script::Value::Bool(false));
    h.frames(2);
    h.ui.dispatch(key());
    h.frames(1);
    assert_eq!(h.text("n"), "2", "still reaching the app");
}

#[test]
fn popovers_can_open_without_focusing_their_first_control() {
    for (autofocus, focused) in [(true, true), (false, false)] {
        let mut h = Harness::inline(&format!(
            r##"return ui.column {{ p = 20,
                ui.popover {{ open = ui.state("o", true), autofocus = {autofocus},
                    anchor = ui.button {{ id = "a", w = 20, h = 20, "a" }},
                    ui.input {{ id = "q", value = ui.state("q", ""), w = 100 }} }} }}"##
        ));
        h.frames(3);
        let q = h.find("q").unwrap();
        let mut node = h.ui.input.focused();
        let mut inside = false;
        while let Some(id) = node {
            if id == q {
                inside = true;
            }
            node = h.ui.scene.borrow().parent(id);
        }
        assert_eq!(inside, focused, "autofocus = {autofocus}");
    }
}

#[test]
fn layers_stay_live_over_an_open_modal() {
    let mut h = Harness::inline(
        r##"local n = ui.state("n", 0)
        local toast = ui.state("toast", false)
        return ui.column { w_full = true, h_full = true,
            ui.modal { open = ui.state("dialog", true), w = 300, h = 200,
                ui.button { id = "show", on_click = function() toast:set(true) end, "show" } },
            ui.layer { open = toast, placement = { x = "end", y = "end", inset = 8 },
                ui.button { id = "dismiss", w = 80, h = 30,
                    on_click = function() n:set(n:peek() + 1) end, "{n}" } } }"##,
    );
    h.frames(3);
    h.click("show");
    h.frames(3);
    h.click("dismiss");
    h.frames(2);
    assert_eq!(
        h.text("dismiss"),
        "1",
        "the layer's button takes clicks under a modal's focus"
    );
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn auto_margins_keep_the_rows_gaps() {
    let mut h = Harness::inline(
        r##"return ui.row { w = 300, gap = 10,
            ui.div { id = "a", w = 20, h = 10 }, ui.div { id = "b", w = 20, h = 10 },
            ui.div { id = "c", w = 20, h = 10, ml = "auto" } }"##,
    );
    h.ui.prepare_frame();
    let x = |h: &mut Harness, id: &str| {
        let n = h.find(id).unwrap();
        h.ui.scene.borrow().bounds(n).x
    };
    let (a, b, c) = (x(&mut h, "a"), x(&mut h, "b"), x(&mut h, "c"));
    assert_eq!(b - a, 30., "a's gap");
    assert_eq!(c - a, 280., "c pushed to the end");
}

#[test]
fn wrapping_rows_keep_their_gaps() {
    let mut h = Harness::inline(
        r##"return ui.row { w = 100, flex_wrap = true, gap = 10,
            ui.div { id = "a", w = 40, h = 10 }, ui.div { id = "b", w = 40, h = 10 },
            ui.div { id = "c", w = 40, h = 10 } }"##,
    );
    h.ui.prepare_frame();
    let at = |h: &mut Harness, id: &str| {
        let n = h.find(id).unwrap();
        let b = h.ui.scene.borrow().bounds(n);
        (b.x, b.y)
    };
    let (a, b, c) = (at(&mut h, "a"), at(&mut h, "b"), at(&mut h, "c"));
    assert_eq!(b.0 - a.0, 50., "the gap across");
    assert_eq!(c.1 - a.1, 20., "the gap down");
}

#[test]
fn hidden_show_and_switch_take_no_gap() {
    let mut h = Harness::inline(
        r##"local off = ui.state("off", false)
        return ui.column { gap = 10,
            ui.div { id = "a", h = 20, w = 20 },
            ui.show(off, ui.div { h = 5 }),
            ui.switch(ui.state("k", "none"), { x = ui.div { h = 5 } }),
            ui.div { id = "b", h = 20, w = 20 } }"##,
    );
    h.frames(2);
    let (a, b) = (h.bounds("a"), h.bounds("b"));
    assert_eq!(b.y - a.y, 30., "one gap between a and b");
}

#[test]
fn a_keep_open_menu_item_runs_and_leaves_the_menu_open() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", true)
        local pinned = ui.state("pinned", false)
        return ui.column { p = 20,
            ui.menu { open = open, w = 160, anchor = ui.button { id = "anchor", w = 40, h = 20, "a" },
                ui.menu_item { id = "pin", keep_open = true, label = "Pin",
                    on_click = function() pinned:set(not pinned:get()) end, ui.text { "Pin" } },
                ui.menu_item { id = "go", "Go" } },
            ui.text { "{pinned}", id = "pinned" } }"##,
    );
    h.frames(3);
    h.click("pin");
    h.frames(2);
    assert_eq!(h.text("pinned"), "true", "the click ran");
    assert!(
        h.find("go")
            .is_some_and(|n| h.ui.scene.borrow().visible_bounds(n).is_some()),
        "still open"
    );
    h.click("go");
    h.frames(2);
    assert!(
        !h.find("go").is_some_and(|n| h
            .ui
            .scene
            .borrow()
            .visible_bounds(n)
            .is_some_and(|b| b.width > 0.)),
        "a plain item closes it"
    );
}

#[test]
fn tooltips_show_after_a_rest_and_at_once_after_another() {
    use zgx::input::InputEvent;
    let mut h = Harness::inline(
        r##"return ui.column { p = 40, gap = 10,
            ui.button { id = "a", w = 40, h = 20, tooltip = "Copy link", "a" },
            ui.button { id = "b", w = 40, h = 20, tooltip = ui.text { "Bee", id = "bee" }, "b" },
            ui.div { id = "away", w = 40, h = 20 } }"##,
    );
    h.frames(2);
    let visible = |h: &mut Harness, id: &str| {
        h.frames(1);
        h.find(id).is_some_and(|n| {
            let scene = h.ui.scene.borrow();
            scene.visible_bounds(n).is_some_and(|b| b.width > 0.) && scene.effects(n).opacity > 0.
        })
    };
    let to = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        h.ui.dispatch(InputEvent::PointerMove {
            x: b.x + 5.,
            y: b.y + 5.,
        });
    };
    to(&mut h, "b");
    assert!(!visible(&mut h, "bee"), "not at once");
    wait_ms(&mut h, 350);
    // (It fades in over 140 ms.)
    wait_ms(&mut h, 200);
    assert!(visible(&mut h, "bee"), "after a rest");
    to(&mut h, "away");
    wait_ms(&mut h, 200);
    assert!(
        !visible(&mut h, "bee"),
        "gone as the pointer leaves (after its fade)"
    );
    to(&mut h, "b");
    h.frames(1);
    assert!(visible(&mut h, "bee"), "at once just after another");
}
#[test]
fn a_key_closes_a_tooltip_and_escape_an_interactive_card() {
    use zgx::input::{InputEvent, Key, Modifiers};
    let mut h = Harness::inline(
        r##"return ui.column { p = 40, gap = 10,
            ui.button { id = "a", w = 40, h = 20, tooltip = ui.text { "Tip", id = "tip" }, "a" },
            ui.button { id = "b", w = 40, h = 20, "b", tooltip_interactive = true,
                tooltip = ui.column { id = "card", w = 120, h = 60, bg = "#202020" } },
            ui.div { id = "away", w = 40, h = 20, mt = 200 } }"##,
    );
    h.frames(2);
    let visible = |h: &mut Harness, id: &str| {
        h.frames(1);
        h.find(id).is_some_and(|n| {
            let scene = h.ui.scene.borrow();
            scene.visible_bounds(n).is_some_and(|b| b.width > 0.) && scene.effects(n).opacity > 0.
        })
    };
    let to = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        h.ui.dispatch(InputEvent::PointerMove {
            x: b.x + 5.,
            y: b.y + 5.,
        });
        h.frames(1);
    };
    let press = |h: &mut Harness, key: Key| {
        h.ui.dispatch(InputEvent::KeyDown {
            key: key.clone(),
            modifiers: Modifiers::default(),
            repeat: false,
        });
        h.ui.dispatch(InputEvent::KeyUp {
            key,
            modifiers: Modifiers::default(),
        });
        h.frames(1);
    };
    to(&mut h, "a");
    wait_ms(&mut h, 550);
    assert!(visible(&mut h, "tip"));
    press(&mut h, Key::Character("x".into()));
    assert!(!visible(&mut h, "tip"), "a key closes it, at once");
    // Away and back after the second's grace: it shows again.
    to(&mut h, "away");
    wait_ms(&mut h, 1100);
    to(&mut h, "b");
    wait_ms(&mut h, 550);
    assert!(visible(&mut h, "card"));
    press(&mut h, Key::Character("x".into()));
    assert!(visible(&mut h, "card"), "an interactive card takes typing");
    press(&mut h, Key::Escape);
    assert!(!visible(&mut h, "card"), "and closes on Escape");
}
/// Waits `ms` and lets timers (a tooltip's delay, its fade) run.
fn wait_ms(h: &mut Harness, ms: u64) {
    // In steps, settling as it goes: a timer that starts as one settles
    // gets its time.
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.settle();
        h.frames(1);
    }
}

#[test]
fn clicks_and_presses_carry_the_keys_held() {
    use zgx::input::{InputEvent, Modifiers, PointerButton};
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        return ui.column {
            ui.button { id = "b", w = 40, h = 20,
                on_press = function(e) seen:set(seen:peek() .. (if e.meta then "P" else "p")) end,
                on_click = function(e) seen:set(seen:peek() .. (if e.meta then "C" else "c")) end, "b" },
            ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frames(2);
    let b = h.bounds("b");
    let (x, y) = (b.x + 5., b.y + 5.);
    for meta in [false, true] {
        let held = Modifiers {
            meta,
            ..Default::default()
        };
        h.ui.dispatch_with_modifiers(InputEvent::PointerMove { x, y }, held);
        h.ui.dispatch_with_modifiers(
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            },
            held,
        );
        h.ui.dispatch_with_modifiers(
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            },
            held,
        );
    }
    h.frames(1);
    assert_eq!(h.text("seen"), "pcPC", "plain, then ⌘ held");
}

#[test]
fn a_span_with_an_href_is_a_link_and_its_click_carries_the_keys_held() {
    use zgx::{
        input::{InputEvent, Modifiers, PointerButton},
        semantics::Role,
    };
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        return ui.column { w = 300,
            ui.rich { ui.span { "docs", literal = true, href = "https://example.org",
                on_click = function(e)
                    seen:set(seen:peek() .. (if e.meta or e.ctrl then "C" else "c"))
                end } },
            ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frames(2);
    let link = {
        let semantics = h.ui.semantics.borrow();
        let links: Vec<_> = semantics
            .iter()
            .filter(|(_, s)| s.role == Role::Link)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(links.len(), 1);
        links[0]
    };
    let b = h.ui.scene.borrow().bounds(link);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    for button in [PointerButton::Primary, PointerButton::Middle] {
        let held = Modifiers::default();
        h.ui.dispatch_with_modifiers(InputEvent::PointerDown { x, y, button }, held);
        h.ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, held);
    }
    h.frames(1);
    assert_eq!(h.text("seen"), "cC", "a middle click is a ⌘-click");
}

#[test]
fn a_swipe_between_pages_reaches_the_nearest_on_navigate() {
    use zgx::input::{Navigation, ScrollGesture};
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        return ui.column {
            ui.column { id = "page", w = 200, h = 100,
                on_navigate = function(e) seen:set(seen:peek() .. e.direction .. ";") end,
                ui.div { id = "inner", w = 100, h = 40 } },
            ui.div { id = "away", w = 50, h = 40 },
            ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frames(1);
    let inner = h.bounds("inner");
    let (x, y) = (inner.x + 10., inner.y + 10.);
    assert!(h.ui.input.navigate(&h.ui.scene, x, y, Navigation::Back));
    assert!(h.ui.input.navigate(&h.ui.scene, x, y, Navigation::Forward));
    let away = h.bounds("away");
    assert!(
        !h.ui
            .input
            .navigate(&h.ui.scene, away.x + 5., away.y + 5., Navigation::Back),
        "nothing there navigates"
    );
    h.frames(1);
    assert_eq!(h.text("seen"), "back;forward;");
    // A trackpad gesture across it, where nothing scrolls, is a swipe.
    h.ui.set_elastic_scrolling(true);
    h.ui.set_navigation_swipes(true);
    h.ui.dispatch(InputEvent::ScrollGesture(ScrollGesture::Began));
    h.ui.dispatch(InputEvent::Scroll {
        x,
        y,
        delta_x: -20.,
        delta_y: 0.,
    });
    assert!(h.ui.take_navigation_swipe());
}

#[test]
fn a_middle_click_reaches_the_innermost_on_aux_click() {
    use zgx::input::PointerButton;
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        local function note(s) seen:set(seen:peek() .. s .. ";") end
        return ui.column {
            ui.row { id = "outer", w = 200, h = 60,
                on_aux_click = function(e) note("outer " .. tostring(e.button)) end,
                ui.div { id = "inner", w = 100, h = 40, on_click = function() note("click") end,
                    on_aux_click = function(e)
                        note("inner " .. tostring(e.button) .. (if e.local_x == 50 then " middle" else " off"))
                    end },
                ui.div { id = "plain", w = 50, h = 40 } },
            ui.div { id = "away", w = 50, h = 40 },
            ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frames(1);
    let at = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        (b.x + b.width / 2., b.y + b.height / 2.)
    };
    let press = |h: &mut Harness, down: (f32, f32), up: (f32, f32), button: PointerButton| {
        h.ui.dispatch(InputEvent::PointerMove {
            x: down.0,
            y: down.1,
        });
        h.ui.dispatch(InputEvent::PointerDown {
            x: down.0,
            y: down.1,
            button,
        });
        h.ui.dispatch(InputEvent::PointerMove { x: up.0, y: up.1 });
        h.ui.dispatch(InputEvent::PointerUp {
            x: up.0,
            y: up.1,
            button,
        });
        h.frames(1);
    };
    let inner = at(&mut h, "inner");
    press(&mut h, inner, inner, PointerButton::Middle);
    assert_eq!(
        h.text("seen"),
        "inner middle middle;",
        "the innermost takes it, told the button and where"
    );
    let (plain, away) = (at(&mut h, "plain"), at(&mut h, "away"));
    // The outer node heard that press too, but the inner one took its click:
    // a press elsewhere released over the outer node is no click of its.
    press(&mut h, away, plain, PointerButton::Middle);
    assert_eq!(h.text("seen"), "inner middle middle;");
    press(&mut h, plain, plain, PointerButton::Middle);
    assert_eq!(
        h.text("seen"),
        "inner middle middle;outer middle;",
        "a node without one passes it up"
    );
    press(&mut h, inner, inner, PointerButton::Other(4));
    assert_eq!(
        h.text("seen"),
        "inner middle middle;outer middle;inner 4 middle;",
        "other buttons by number"
    );
    // Released somewhere else, it is no click; nor is the primary button's.
    press(&mut h, inner, away, PointerButton::Middle);
    press(&mut h, inner, inner, PointerButton::Primary);
    assert_eq!(
        h.text("seen"),
        "inner middle middle;outer middle;inner 4 middle;click;"
    );
    // That press released away is spent: a later one begun elsewhere and
    // released over the node is no click of it, nor of the node around it.
    press(&mut h, inner, away, PointerButton::Middle);
    press(&mut h, away, inner, PointerButton::Middle);
    assert_eq!(
        h.text("seen"),
        "inner middle middle;outer middle;inner 4 middle;click;"
    );
}

#[test]
fn cleanups_run_when_a_component_unmounts() {
    let mut h = Harness::inline(
        r##"local shown = ui.state("shown", true)
        local gone = ui.state("gone", 0)
        local Ticker = ui.component(function()
            ui.on_cleanup(function() gone:set(gone:peek() + 1) end)
            return ui.text { "ticking", id = "t" }
        end)
        return ui.column {
            ui.button { id = "hide", on_click = function() shown:set(false) end, "hide" },
            ui.show(shown, Ticker {}),
            ui.text { "{gone}", id = "gone" },
            ui.text { if ui.now() > 1.6e12 then "now" else "then", id = "now" } }"##,
    );
    h.frames(1);
    assert_eq!(h.text("gone"), "0");
    h.click("hide");
    h.frames(1);
    assert!(h.find("t").is_none());
    assert_eq!(h.text("gone"), "1", "its cleanup ran once it went");
    assert_eq!(
        h.text("now"),
        "now",
        "ui.now is milliseconds since the epoch"
    );
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn a_menu_taller_than_its_max_h_scrolls() {
    let mut h = Harness::inline(
        r##"local items = {}
        for i = 1, 20 do table.insert(items, ui.menu_item { id = "i" .. i, "Item " .. i }) end
        return ui.column { p = 20,
            ui.menu { id = "m", open = ui.state("m", true), w = 160, max_h = 100,
                anchor = ui.button { id = "a", w = 20, h = 20, "a" }, table.unpack(items) } }"##,
    );
    h.frames(3);
    let first = h.bounds("i1");
    let last = h.find("i20").map(|n| h.ui.scene.borrow().visible_bounds(n));
    assert!(
        last.is_some_and(|b| b.is_none()),
        "the last item is scrolled out of view"
    );
    // The wheel over the menu brings it in.
    for _ in 0..20 {
        h.ui.dispatch(zgx::input::InputEvent::Scroll {
            x: first.x + 10.,
            y: first.y + 10.,
            delta_x: 0.,
            delta_y: 40.,
        });
    }
    h.frames(2);
    let n = h.find("i20").unwrap();
    assert!(
        h.ui.scene.borrow().visible_bounds(n).is_some(),
        "scrolled to the end"
    );
}

#[test]
fn an_interactive_hover_card_stays_while_the_pointer_is_on_it() {
    use zgx::input::InputEvent;
    let mut h = Harness::inline(
        r##"return ui.column { p = 40, gap = 10,
            ui.button { id = "a", w = 40, h = 20, "a", tooltip_interactive = true,
                tooltip_placement = { side = "bottom", offset = 4 },
                tooltip = ui.column { id = "card", w = 120, h = 60, bg = "#202020" } },
            ui.div { id = "away", w = 40, h = 20, mt = 200 } }"##,
    );
    h.frames(2);
    let visible = |h: &mut Harness, id: &str| {
        h.frames(1);
        h.find(id).is_some_and(|n| {
            let scene = h.ui.scene.borrow();
            scene.visible_bounds(n).is_some_and(|b| b.width > 0.) && scene.effects(n).opacity > 0.
        })
    };
    let wait = |h: &mut Harness, ms: u64| wait_ms(h, ms);
    let to = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        h.ui.dispatch(InputEvent::PointerMove {
            x: b.x + 5.,
            y: b.y + 5.,
        });
        // Hover moves as the frame is prepared.
        h.frames(1);
    };
    to(&mut h, "a");
    wait(&mut h, 350);
    wait(&mut h, 200);
    assert!(visible(&mut h, "card"), "open after a rest");
    to(&mut h, "card");
    wait(&mut h, 250);
    assert!(visible(&mut h, "card"), "kept while the pointer is on it");
    to(&mut h, "away");
    // The grace, then the fade out.
    wait(&mut h, 450);
    assert!(!visible(&mut h, "card"), "gone once the pointer left both");
}

#[test]
fn one_tooltip_shows_at_a_time() {
    use zgx::input::InputEvent;
    let mut h = Harness::inline(
        r##"return ui.column { p = 40, gap = 4,
            ui.button { id = "a", w = 40, h = 20, "a", tooltip_interactive = true,
                tooltip = ui.text { "Card A", id = "ca" } },
            ui.button { id = "b", w = 40, h = 20, "b", tooltip_interactive = true,
                tooltip = ui.text { "Card B", id = "cb" } } }"##,
    );
    h.frames(2);
    let visible = |h: &mut Harness, id: &str| {
        h.frames(1);
        h.find(id).is_some_and(|n| {
            let scene = h.ui.scene.borrow();
            scene.visible_bounds(n).is_some_and(|b| b.width > 0.) && scene.effects(n).opacity > 0.
        })
    };
    let to = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        h.ui.dispatch(InputEvent::PointerMove {
            x: b.x + 5.,
            y: b.y + 5.,
        });
        h.frames(1);
    };
    to(&mut h, "a");
    wait_ms(&mut h, 350);
    wait_ms(&mut h, 200);
    assert!(visible(&mut h, "ca"));
    // Straight to the next: its card opens at once, and the last one goes
    // (not left showing out its grace).
    to(&mut h, "b");
    assert!(visible(&mut h, "cb"), "the next opens at once");
    assert!(!visible(&mut h, "ca"), "and the last one closes");
}

#[test]
fn null_keeps_its_key_as_json_null() {
    let h = Harness::inline(
        r##"local sent = ui.shared("sent", nil :: any)
        sent:set({ caps = ui.null, list = { 1, ui.null, 3 }, name = "x" })
        return ui.text "ok""##,
    );
    let sent = h.host.shared("sent", zgx_script::Value::Nil).get();
    assert_eq!(
        sent.to_json(),
        r#"{"caps":null,"list":[1,null,3],"name":"x"}"#
    );
}

#[test]
fn object_sends_an_empty_table_as_an_object_both_ways() {
    let h = Harness::inline(
        r##"local sent = ui.shared("sent", nil :: any)
        sent:set({ scope = ui.object(), list = {}, filled = ui.object({ a = 1 }) })
        return ui.text "ok""##,
    );
    let sent = h.host.shared("sent", zgx_script::Value::Nil).get();
    assert_eq!(sent.to_json(), r#"{"filled":{"a":1},"list":[],"scope":{}}"#);
}

#[test]
fn an_empty_object_from_the_host_goes_back_as_one() {
    let h = Harness::new(|runtime| {
        let host = zgx_script::ScriptHost::inline(
            runtime,
            "app",
            r##"local from = ui.shared("from", nil :: any)
            local back = ui.shared("back", nil :: any)
            ui.effect(function()
                local v = from:get()
                if v then back:set({ again = v.scope }) end
            end)
            return ui.text "ok""##,
        );
        host.shared("from", zgx_script::Value::Nil)
            .set(zgx_script::Value::from_json(r#"{"scope":{}}"#).unwrap());
        host
    });
    h.frames(1);
    let back = h.host.shared("back", zgx_script::Value::Nil).get();
    assert_eq!(back.to_json(), r#"{"again":{}}"#);
}

#[test]
fn literal_text_shows_braces_as_written() {
    // `{x}` with no state named x is an error as a template; data shown
    // with `literal` is never read as one.
    let mut h = Harness::inline(
        r##"local code = "format!(\"{x}\") {{y}}"
        return ui.column {
            ui.text { code, literal = true },
            ui.rich { id = "rich", literal = true, "let a = {b};", { " {c}", color = "#f80" } },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.count_text("format!(\"{x}\") {{y}}"), 1);
    let rich = h.kind("rich");
    assert!(
        rich.contains("let a = {b};") && rich.contains(" {c}"),
        "{rich}"
    );
}

#[test]
fn virtual_lists_take_each_rows_height() {
    // Rows of 10 and 1000 points alternate: a 200-point view holds the first
    // two rows (and the overscan), not twenty; changing the heights relays
    // them out without a count.
    let mut h = Harness::inline(
        r##"local heights = ui.state("heights", {})
        local list = {}
        for i = 1, 10000 do list[i] = if i % 2 == 1 then 10 else 1000 end
        heights:set(list)
        return ui.column {
            ui.button { id = "flat", on_click = function()
                local flat = {}
                for i = 1, 10000 do flat[i] = 10 end
                heights:set(flat)
            end, "flat" },
            ui.list { heights = heights, overscan = 0, w = 200, h = 200,
                row = function(i) return ui.text("row " .. i) end },
        }"##,
    );
    let rows = h.count_text("row ");
    assert!(rows <= 3, "{rows} rows mounted");
    h.click("flat");
    let rows = h.count_text("row ");
    assert!((15..=25).contains(&rows), "{rows} rows mounted");
    assert_eq!(h.host.error().get(), None);
}

/// `ui.each` takes a plain table for a list that never changes.
#[test]
fn each_takes_a_plain_table() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.each({ "a", "b" }, function(n, i) return ui.text { id = n, `{n}{i}` } end) }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.text("a"), "a1");
    assert_eq!(h.text("b"), "b2");
}

#[test]
fn a_following_list_follows_its_first_rows_and_after_being_cleared() {
    // A chat transcript: empty at first, ten 20-point rows in a 60-point
    // view arrive, it is cleared and they arrive again. Each time it rests
    // at its end, 140 points down, measured or with its heights given.
    for kind in ["data = items, measured = true", "heights = hs"] {
        let mut h = Harness::inline(&format!(
            r##"local items = ui.state("items", {{}})
            local hs = ui.state("hs", {{}})
            local off = ui.state("off", 0)
            local function fill(n)
                local list, heights = {{}}, {{}}
                for i = 1, n do list[i] = i; heights[i] = 20 end
                items:set(list)
                hs:set(heights)
            end
            return ui.column {{
                ui.button {{ id = "add", on_click = function() fill(10) end, "add" }},
                ui.button {{ id = "clear", on_click = function() fill(0) end, "clear" }},
                ui.text {{ id = "off", function() return tostring(off:get()) end }},
                ui.list {{ {kind}, follow = true, offset = off, overscan = 0,
                    height = 20, w = 160, h = 60,
                    row = function(i) return ui.row {{ h = 20, ui.text("row") }} end }},
            }}"##
        ));
        h.click("add");
        h.frames(3);
        assert_eq!(h.text("off"), "140", "{kind}: its first rows");
        h.click("clear");
        h.frames(3);
        h.click("add");
        h.frames(3);
        assert_eq!(h.text("off"), "140", "{kind}: cleared and filled again");
        assert_eq!(h.host.error().get(), None);
    }
}

/// A scroll view or list in a `show`, `switch` or `presence` stretches as it
/// does on its own: their box lays out as the view they hold.
#[test]
fn every_scroll_kind_stretches_in_every_dynamic_region() {
    let mut h = Harness::inline(
        r##"local page = ui.state("page", "a")
        local yes = ui.state("yes", true)
        local kinds = {
            scroll_x = function(id) return ui.scroll_x { id = id, ui.column { w = 900, h = 40 } } end,
            scroll = function(id) return ui.scroll { id = id, ui.column { h = 40 } } end,
            list = function(id) return ui.list { id = id, count = 1, height = 20,
                row = function() return ui.text { "x" } end } end,
            variable = function(id) return ui.list { id = id, heights = { 20 },
                row = function() return ui.text { "x" } end } end,
        }
        local out = {}
        for kind, make in kinds do
            table.insert(out, make("plain-" .. kind))
            table.insert(out, ui.show(yes, make("show-" .. kind)))
            table.insert(out, ui.switch(page, { a = make("switch-" .. kind) }))
            table.insert(out, ui.presence(page, function() return make("presence-" .. kind) end))
        end
        return ui.column { w = 600, h = 3000, table.unpack(out) }"##,
    );
    h.ui.prepare_frame();
    for kind in ["scroll_x", "scroll", "list", "variable"] {
        for region in ["plain", "show", "switch", "presence"] {
            let id = format!("{region}-{kind}");
            let node = h.find(&id).unwrap();
            assert_eq!(h.ui.scene.borrow().bounds(node).width, 600., "{id}");
        }
    }
}

/// A script's root fills its parent across, as a block does, and takes
/// its height from what it holds.
#[test]
fn a_scripts_root_fills_its_parent_across() {
    let root = |code: &str| {
        let mut h = Harness::inline(code);
        h.ui.prepare_frame();
        assert_eq!(h.host.error().get(), None, "{code}");
        let node = h.find("root").unwrap();
        let window = h.ui.scene.borrow().bounds(h.ui.scene.borrow().root());
        (h.ui.scene.borrow().bounds(node), window)
    };
    let (bare, window) = root(r#"return ui.column { id = "root", ui.text { "x" } }"#);
    assert_eq!(bare.width, window.width);
    assert!(bare.height < 40., "its height is what it holds: {bare:?}");
    let (row, window) = root(r#"return ui.row { id = "root", ui.text { "x" } }"#);
    assert_eq!(row.width, window.width);
    let (sized, _) = root(r#"return ui.column { id = "root", w = 300, ui.text { "x" } }"#);
    assert_eq!(sized.width, 300., "a width of its own holds");
    let (half, window) = root(r#"return ui.column { id = "root", w = "50%", ui.text { "x" } }"#);
    assert_eq!(half.width, window.width / 2.);
    let (full, window) =
        root(r#"return ui.column { id = "root", w_full = true, h_full = true, ui.text { "x" } }"#);
    assert_eq!((full.width, full.height), (window.width, window.height));
    let (tall, window) =
        root(r#"return ui.column { id = "root", h_full = true, ui.text { "x" } }"#);
    assert_eq!(tall.height, window.height);
    // Content taller than the window overflows as it did: nothing shrinks.
    let (over, window) = root(
        r#"return ui.column { id = "root", ui.column { h = 5000, shrink = 1, ui.text { "x" } } }"#,
    );
    assert_eq!(over.height, 5000., "{window:?}");
}

/// Embedded in a Rust host's view, a script fills the box it is given, and
/// in a row sized by its content it takes its own width.
#[test]
fn an_embedded_script_fills_its_parent_not_the_window() {
    use zgx::compose::{div, row};
    use zgx::style::Styled;
    let code = r#"return ui.column { ui.text { "marker" } }"#;
    let width = |wrap: &dyn Fn(zgx::compose::View) -> zgx::compose::View| {
        let mut ui = zgx::widgets::Ui::new(800., 600.);
        let host = zgx_script::ScriptHost::inline(&ui.runtime, "embedded", code);
        host.reload().unwrap();
        let _mounted = ui.mount(wrap(host.view()));
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let mut stack = vec![scene.root()];
        while let Some(node) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(node)
                && &**text == "marker"
            {
                return scene.bounds(scene.parent(node).unwrap()).width;
            }
            stack.extend(scene.children(node).iter().copied());
        }
        panic!("no marker");
    };
    assert_eq!(width(&|view| view), 800.);
    assert_eq!(width(&|view| div().w(300.).h(200.).child(view)), 300.);
    // Beside a sibling in a row: what's left, or what it needs.
    assert_eq!(
        width(&|view| row()
            .w(500.)
            .child(div().w(100.))
            .child(div().grow().child(view))),
        400.
    );
    let shrunk = width(&|view| row().child(view).child(div().w(100.)));
    assert!(
        shrunk > 0. && shrunk < 200.,
        "as wide as what it holds: {shrunk}"
    );
}

/// `show`, `switch` and `presence` are no box of their own: what a node says
/// of its size and place holds there as it does anywhere.
#[test]
fn a_dynamic_region_changes_nothing_about_how_a_node_lays_out() {
    let cases = [
        ("width percent", r#"w = "50%", h = 20"#),
        ("height percent", r#"w = 40, h = "50%""#),
        ("full", "w_full = true, h_full = true"),
        ("grow", "grow = 1"),
        ("no shrink", "w = 900, h = 20, shrink = 0"),
        (
            "min and max",
            "min_w = 120, max_w = 130, min_h = 30, max_h = 40",
        ),
        ("aspect", "w = 100, aspect = 2"),
        ("margin", "w = 40, h = 20, m = 7"),
        ("basis", "basis = 80, h = 20"),
        ("align self", r#"w = 40, h = 20, self = "end""#),
        (
            "absolute",
            "absolute = true, top = 11, left = 13, w = 40, h = 20",
        ),
        (
            "absolute insets",
            "absolute = true, top = 5, bottom = 7, left = 9, right = 11",
        ),
        (
            "relative",
            "relative = true, top = 11, left = 13, w = 40, h = 20",
        ),
    ];
    for row in [false, true] {
        for (name, props) in cases {
            let container = if row { "ui.row" } else { "ui.column" };
            let mut h = Harness::inline(&format!(
                r##"local yes = ui.state("yes", true)
                local page = ui.state("page", "a")
                local function make(id)
                    return ui.column {{ id = id, {props} }}
                end
                local function box(child)
                    return {container} {{ w = 300, h = 200, relative = true, child,
                        ui.column {{ w = 10, h = 10 }} }}
                end
                return ui.column {{
                    box(make("plain")),
                    box(ui.show(yes, make("show"))),
                    box(ui.switch(page, {{ a = make("switch") }})),
                    box(ui.presence(page, function() return make("presence") end)),
                }}"##
            ));
            h.ui.prepare_frame();
            assert_eq!(h.host.error().get(), None, "{name}");
            let at = |h: &mut Harness, id: &str, index: f32| {
                let node = h.find(id).unwrap();
                let b = h.ui.scene.borrow().transformed_bounds(node);
                (b.x, b.y - 200. * index, b.width, b.height)
            };
            let plain = at(&mut h, "plain", 0.);
            for (index, region) in ["show", "switch", "presence"].into_iter().enumerate() {
                let inside = at(&mut h, region, index as f32 + 1.);
                assert_eq!(inside, plain, "{name} in a {region}, row {row}");
            }
        }
    }
}

#[test]
fn sticky_rows_hold_the_top_until_the_next_arrives() {
    // Sections of a 30-point header and three 10-point rows, 60 points each.
    let mut h = Harness::inline(
        r##"local off = ui.state("off", 0)
        local hs, sticky = {}, {}
        for s = 0, 9 do
            table.insert(sticky, #hs + 1)
            table.insert(hs, 30)
            for _ = 1, 3 do table.insert(hs, 10) end
        end
        return ui.column {
            ui.button { id = "a", on_click = function() off:set(25) end, "a" },
            ui.button { id = "b", on_click = function() off:set(40) end, "b" },
            ui.list { id = "l", heights = hs, sticky = sticky, offset = off, w = 200, h = 100,
                row = function(i) return ui.text { "row " .. i, h = hs[i] } end } }"##,
    );
    // The texts reading `name`, as (y relative to the list) in order.
    let ys = |h: &mut Harness, name: &str| -> Vec<f32> {
        h.ui.prepare_frame();
        let list = h.find("l").unwrap();
        let scene = h.ui.scene.borrow();
        let top = scene.bounds(list).y;
        let mut out = Vec::new();
        let mut stack = vec![list];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n)
                && &**text == name
            {
                let b = scene.transformed_bounds(n);
                out.push(b.y - top);
            }
            stack.extend(scene.children(n).iter().copied());
        }
        out.sort_by(f32::total_cmp);
        out
    };
    // Scrolled 25 into the first section: its header held at the top, over
    // row 1 itself (25 above, scrolling away).
    h.click("a");
    assert_eq!(ys(&mut h, "row 1"), vec![-25., 0.]);
    // At 40, the next header (natural top 60, 20 down) pushes it up by 10.
    h.click("b");
    assert_eq!(ys(&mut h, "row 1"), vec![-40., -10.]);
}

/// With `key`, a sticky row is its section's: the overlay builds it as the
/// list does, given its key and its index as a state, and keeps it while the
/// section moves; another section on top builds its own.
#[test]
fn keyed_sticky_rows_follow_their_sections() {
    let mut h = Harness::inline(
        r##"local off = ui.state("off", 0)
        local names = ui.state("names", { "a", "b", "c" })
        local built = ui.state("built", 0)
        local rows = ui.computed(function()
            local out = {}
            for _, n in names:get() do
                table.insert(out, n)
                for i = 1, 3 do table.insert(out, `{n}{i}`) end
            end
            return out
        end)
        local heights = ui.computed(function()
            local out = {}
            for _, r in rows:get() do table.insert(out, if #r == 1 then 30 else 10) end
            return out
        end)
        local sticky = ui.computed(function()
            local out = {}
            for i, r in rows:get() do if #r == 1 then table.insert(out, i) end end
            return out
        end)
        return ui.column {
            ui.text { id = "built", function() return tostring(built:get()) end },
            ui.button { id = "down", on_click = function() off:set(70) end, "down" },
            ui.button { id = "first", on_click = function() names:set({ "z", "a", "b", "c" }) end, "first" },
            ui.button { id = "drop", on_click = function() names:set({ "b", "c" }) off:set(5) end, "drop" },
            ui.list { id = "l", heights = heights, sticky = sticky, offset = off, w = 200, h = 100,
                key = function(i) return rows:get()[i] end,
                row = function(key, index)
                    if #key == 1 then built:update(function(n) return n + 1 end) end
                    return ui.text { function() return `{key}@{index:get()}` end, h = if #key == 1 then 30 else 10 }
                end } }"##,
    );
    let tops = |h: &mut Harness| -> Vec<String> {
        h.ui.prepare_frame();
        let list = h.find("l").unwrap();
        let scene = h.ui.scene.borrow();
        let top = scene.bounds(list).y;
        let mut out = Vec::new();
        let mut stack = vec![list];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n) {
                let at = scene.transformed_bounds(n).y - top;
                if text.contains('@') && (-0.5..0.5).contains(&at) {
                    out.push(text.to_string());
                }
            }
            stack.extend(scene.children(n).iter().copied());
        }
        out.sort();
        out
    };
    // Row 5 ("b", 60 down) has passed the top: its section holds it.
    h.click("down");
    assert_eq!(tops(&mut h), vec!["b@5".to_string()]);
    // A section in front: the same row, its index moved on.
    let built = h.text("built");
    h.click("first");
    assert_eq!(tops(&mut h), vec!["a@5".to_string()]);
    h.click("down");
    // Its section gone, the next takes over.
    h.click("drop");
    assert_eq!(tops(&mut h), vec!["b@1".to_string()]);
    assert_ne!(h.text("built"), "0", "rows built: {built}");
}

/// With `data`, a list of `heights` shows its items as they change: an edited
/// item's row builds again, its sticky header too, and the rest stay.
#[test]
fn a_list_of_data_and_heights_shows_edits() {
    let mut h = Harness::inline(
        r##"local entries = ui.state("entries", {
            { id = "h1", title = "Section", header = true },
            { id = "a", title = "first" },
            { id = "b", title = "second" },
        })
        local heights = ui.computed(function()
            local out = {}
            for _, e in entries:get() do table.insert(out, if e.header then 30 else 20) end
            return out
        end)
        return ui.column {
            ui.button { id = "edit", on_click = function()
                entries:set_key({ 2, "title" }, "edited")
            end, "edit" },
            ui.button { id = "head", on_click = function()
                entries:set_key({ 1, "title" }, "Renamed")
            end, "head" },
            ui.list { w = 200, h = 100, data = entries, heights = heights, sticky = { 1 },
                key = function(e) return e.id end,
                row = function(e) return ui.text { id = e.id .. "-row", e.title, h = if e.header then 30 else 20 } end } }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.text("a-row"), "first");
    h.click("edit");
    h.ui.prepare_frame();
    assert_eq!(h.text("a-row"), "edited");
    assert_eq!(h.text("b-row"), "second");
    h.click("head");
    h.ui.prepare_frame();
    assert_eq!(h.text("h1-row"), "Renamed");
    assert_eq!(h.host.error().get(), None);
}

/// A sticky list takes what any list takes: its scrollbar and edge callbacks
/// go to the list under the sticky row.
#[test]
fn a_sticky_list_takes_a_scrollbar_and_edge_callbacks() {
    let mut h = Harness::inline(
        r##"local ended = ui.state("ended", 0)
        local hs = {}
        for _ = 1, 100 do table.insert(hs, 30) end
        return ui.column {
            ui.text { id = "ended", function() return tostring(ended:get()) end },
            ui.list { id = "l", heights = hs, sticky = { 1, 50 }, w = 200, h = 100,
                scrollbar = true, overscroll = "contain", end_distance = 10000,
                on_end = function() ended:add(1) end,
                row = function(i) return ui.text { "row " .. i, h = 30 } end } }"##,
    );
    h.ui.prepare_frame();
    h.settle();
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    assert_eq!(h.text("ended"), "1");
}

#[test]
fn sticky_top_holds_them_lower() {
    // As above, stuck 12 points down (a diff's headers under their gap).
    let mut h = Harness::inline(
        r##"local off = ui.state("off", 0)
        local hs, sticky = {}, {}
        for s = 0, 9 do
            table.insert(sticky, #hs + 1)
            table.insert(hs, 30)
            for _ = 1, 3 do table.insert(hs, 10) end
        end
        return ui.column {
            ui.button { id = "a", on_click = function() off:set(5) end, "a" },
            ui.button { id = "b", on_click = function() off:set(40) end, "b" },
            ui.list { id = "l", heights = hs, sticky = sticky, sticky_top = 12, offset = off, w = 200, h = 100,
                row = function(i) return ui.text { "row " .. i, h = hs[i] } end } }"##,
    );
    let ys = |h: &mut Harness, name: &str| -> Vec<f32> {
        h.ui.prepare_frame();
        let list = h.find("l").unwrap();
        let scene = h.ui.scene.borrow();
        let top = scene.bounds(list).y;
        let mut out = Vec::new();
        let mut stack = vec![list];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n)
                && &**text == name
            {
                out.push(scene.transformed_bounds(n).y - top);
            }
            stack.extend(scene.children(n).iter().copied());
        }
        out.sort_by(f32::total_cmp);
        out
    };
    h.click("a");
    assert_eq!(ys(&mut h, "row 1"), vec![-5., 12.]);
    // The next header (natural top 60, 20 down) pushes it up: 12 + 30 > 20.
    h.click("b");
    assert_eq!(ys(&mut h, "row 1"), vec![-40., -10.]);
}

#[test]
fn virtual_list_rows_own_their_state_and_effects() {
    // A row is built as a component is: `ui.use_state` is its own, and the
    // effects it makes end when it scrolls away (they ran on, one more per
    // row ever shown, for as long as the script).
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        local tick = ui.state("tick", 0)
        local runs = ui.state("runs", 0)
        return ui.column {
            ui.button { id = "tick", on_click = ui.add(tick), "tick" },
            ui.button { id = "far", on_click = ui.set(offset, 20000), "far" },
            ui.text { id = "runs", "{runs}" },
            ui.list { count = 100000, height = 20, w = 200, h = 100, offset = offset,
                row = function(i)
                    local mine = ui.use_state(i)
                    ui.effect(function()
                        tick:get()
                        runs:set(runs:peek() + 1)
                    end)
                    return ui.text("row " .. mine:get())
                end },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    let visible = h.count_text("row ");
    assert!(visible > 3 && visible < 20, "{visible} rows");
    h.click("far");
    h.ui.prepare_frame();
    h.settle();
    let before: usize = h.text("runs").parse().unwrap();
    h.click("tick");
    let after: usize = h.text("runs").parse().unwrap();
    assert_eq!(
        after - before,
        h.count_text("row "),
        "only the mounted rows' effects run"
    );
}

#[test]
fn switch_shows_the_matching_case() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "one")
        return ui.column {
            ui.switch(tab, { one = ui.text { "first", id = "one" }, two = ui.text { "second", id = "two" } }),
            ui.button { id = "go", w = 20, h = 20, on_click = ui.set(tab, "two") },
        }"##,
    );
    assert!(h.find("one").is_some() && h.find("two").is_none());
    h.click("go");
    assert!(h.find("one").is_none() && h.find("two").is_some());
}

#[test]
fn percentage_and_auto_lengths() {
    let mut h = Harness::inline(
        r##"return ui.row { w = 400, h = 100,
            ui.div { id = "half", w = "50%", h = 10 },
            ui.div { id = "quarter", w = "25%", h = 10, ml = "auto" },
        }"##,
    );
    assert_eq!(h.width("half"), 200.);
    assert_eq!(h.width("quarter"), 100.);
    let node = h.find("quarter").unwrap();
    assert_eq!(
        h.ui.scene.borrow().bounds(node).x,
        300.,
        "auto margin pushes it to the end"
    );
}

#[test]
fn key_and_hover_events_reach_scripts() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        return ui.column {
            ui.div { id = "target", w = 100, h = 100, focusable = true,
                on_key = function(e) log:set(log:peek() .. e.key .. (e.shift and "!" or "")); return true end,
                on_hover = function(inside) log:set(log:peek() .. (inside and "[" or "]")) end,
                on_press = function(e) log:set(log:peek() .. "p" .. math.floor(e.local_x)) end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    h.ui.dispatch(InputEvent::PointerMove { x: 50., y: 50. });
    h.ui.dispatch(InputEvent::PointerDown {
        x: 30.,
        y: 50.,
        button: zgx::input::PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x: 30.,
        y: 50.,
        button: zgx::input::PointerButton::Primary,
    });
    let node = h.find("target").unwrap();
    let scene = h.ui.scene.clone();
    h.ui.input.focus(&scene, Some(node));
    h.ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("a".into()),
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    h.ui.dispatch(InputEvent::PointerMove { x: 300., y: 290. });
    assert_eq!(h.text("log"), "[p30a!]");
}

#[test]
fn a_parents_handlers_run_once_for_events_on_its_children() {
    // A parent hears its children's events as they bubble up, once (it ran
    // in the capture phase too).
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local function note(s: string) log:set(log:peek() .. s) end
        return ui.column {
            on_press = function() note("p") end,
            on_release = function() note("r") end,
            on_key = function(e) note(e.key) return false end,
            ui.div { id = "child", w = 100, h = 50, focusable = true },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.ui.prepare_frame();
    h.ui.dispatch(InputEvent::PointerDown {
        x: 20.,
        y: 20.,
        button: zgx::input::PointerButton::Primary,
    });
    h.ui.dispatch(InputEvent::PointerUp {
        x: 20.,
        y: 20.,
        button: zgx::input::PointerButton::Primary,
    });
    let node = h.find("child").unwrap();
    let scene = h.ui.scene.clone();
    h.ui.input.focus(&scene, Some(node));
    h.ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("k".into()),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(h.text("log"), "prk");
}

#[test]
fn keyed_rows_keep_their_state_when_the_list_reorders() {
    let mut h = Harness::inline(
        r##"local items = ui.state("items", { { id = 1, name = "x" }, { id = 2, name = "y" } })
        local Row = ui.component(function(props)
            local clicks = ui.use_state(0)
            return ui.button { id = "row" .. props.item.id, w = 50, h = 20,
                on_click = function() clicks:add(1) end,
                ui.text { function() return props.item.name .. clicks:get() end, id = "label" .. props.item.id } }
        end)
        return ui.column {
            ui.each(items, function(item) return Row { item = item } end, { key = function(item) return item.id end }),
            ui.button { id = "swap", w = 20, h = 20, on_click = function()
                local list = items:get()
                items:set({ list[2], list[1] })
            end },
        }"##,
    );
    h.click("row1");
    h.click("row1");
    h.click("swap");
    assert_eq!(h.text("label1"), "x2", "row 1 kept its clicks after moving");
    assert_eq!(h.text("label2"), "y0");
}

#[test]
fn rows_without_a_key_follow_their_items() {
    // No `key`: rows follow their items wherever they move, so an insert or
    // removal builds one row, and each row keeps its own state.
    let mut h = Harness::inline(
        r##"local items = ui.state("items", { { name = "a" }, { name = "b" }, { name = "c" } })
        local builds = ui.state("builds", 0)
        local fresh_builds = ui.state("fresh_builds", 0)
        local Row = ui.component(function(props)
            local clicks = ui.use_state(0)
            builds:set(builds:peek() + 1)
            return ui.button { id = "row-" .. props.item.name, w = 50, h = 20,
                on_click = function() clicks:add(1) end,
                ui.text { function() return props.item.name .. clicks:get() end, id = "label-" .. props.item.name } }
        end)
        -- Built afresh from the state each time: equal tables, new ones.
        local fresh = ui.computed(function()
            local out = {}
            for _, item in items:get() do table.insert(out, { name = item.name }) end
            return out
        end)
        return ui.column {
            ui.each(items, function(item) return Row { item = item } end),
            ui.each(fresh, function(item)
                fresh_builds:set(fresh_builds:peek() + 1)
                return ui.text { item.name, literal = true }
            end),
            ui.text { id = "builds", "{builds} {fresh_builds}" },
            ui.button { id = "front", w = 10, h = 10, on_click = function() items:insert(1, { name = "z" }) end },
            ui.button { id = "drop", w = 10, h = 10, on_click = function() items:remove(1) end },
            ui.button { id = "reverse", w = 10, h = 10, on_click = function()
                local list = items:get()
                items:set({ list[3], list[2], list[1] })
            end },
            ui.button { id = "edit", w = 10, h = 10, on_click = function() items:set_key({ 1, "name" }, "C") end },
        }"##,
    );
    h.click("row-b");
    h.click("row-b");
    assert_eq!(h.text("builds"), "3 3");
    h.click("front");
    assert_eq!(
        h.text("label-b"),
        "b2",
        "b kept its clicks when a row came before it"
    );
    assert_eq!(h.text("builds"), "4 4", "only the new row was built");
    h.click("drop");
    assert_eq!(h.text("label-b"), "b2", "and when the row before it left");
    assert_eq!(h.text("builds"), "4 4", "a removal builds nothing");
    h.click("reverse");
    assert_eq!(h.text("label-b"), "b2", "and when the list reversed");
    assert_eq!(h.text("label-a"), "a0");
    assert_eq!(h.text("builds"), "4 4", "a reorder builds nothing");
    h.click("edit");
    assert_eq!(
        h.text("label-C"),
        "C0",
        "the edited item's row is built again"
    );
    assert_eq!(h.text("label-b"), "b2");
    assert_eq!(h.text("builds"), "5 5", "and only that row");
}

#[test]
fn containers_take_a_label_for_assistive_technology() {
    let mut h = Harness::inline(
        r##"return ui.row {
            ui.button { id = "switch", label = "Turn off", w = 40, h = 20, ui.div { w = 10, h = 10 } },
            ui.div { id = "chart", label = "CPU history", w = 40, h = 20 },
        }"##,
    );
    let mut name = |id: &str| {
        let node = h.find(id).unwrap();
        h.ui.semantics.borrow().get(node).map(|n| n.label.clone())
    };
    assert_eq!(name("switch").as_deref(), Some("Turn off"));
    assert_eq!(name("chart").as_deref(), Some("CPU history"));
}

#[test]
fn modals_open_from_state() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", false)
        return ui.column {
            ui.button { id = "open", w = 30, h = 20, on_click = ui.set(open, true) },
            ui.modal { open = open, label = "Dialog", ui.text { "inside", id = "inside" } },
        }"##,
    );
    // A modal is built the first time it opens; closed again, it keeps its
    // content, hidden.
    assert!(h.find("inside").is_none(), "nothing before it opens");
    let shown = |h: &mut Harness| {
        let node = h.find("inside").unwrap();
        h.ui.scene.borrow().visible_bounds(node).is_some()
    };
    h.click("open");
    assert!(shown(&mut h));
}

#[test]
fn rotated_buttons_are_hit_where_they_are_drawn() {
    let mut h = Harness::inline(
        r##"local n = ui.state("n", 0)
        return ui.column { p = 100,
            ui.button { id = "b", w = 100, h = 20, rotate = 90, on_click = ui.add(n) },
            ui.text { "{n}", id = "n" },
        }"##,
    );
    h.ui.prepare_frame();
    use zgx::input::PointerButton::Primary;
    // Rotated a quarter turn about its centre (150, 110): it now spans
    // y 60..160 at x 140..160, and no longer covers (110, 110).
    for (x, y) in [(150., 70.), (110., 110.)] {
        h.ui.dispatch(InputEvent::PointerDown {
            x,
            y,
            button: Primary,
        });
        h.ui.dispatch(InputEvent::PointerUp {
            x,
            y,
            button: Primary,
        });
    }
    assert_eq!(
        h.text("n"),
        "1",
        "only the point inside the rotated box clicked"
    );
}

/// Sparse nested lists keep numeric order, including which invalid child fails first.
#[test]
fn nested_sparse_children_keep_order_and_first_error() {
    let mut h = Harness::inline(
        r#"local nested = {}
        nested[90] = function() return "c" end
        nested[3] = "b"
        nested[2] = false
        nested[0] = 123
        nested[-1] = true
        nested.ignored = true
        local root = {}
        root[40] = "d"
        root[5] = nested
        root[1] = ui.text { "a" }
        local ok, err = pcall(function()
            return ui.row { [30] = true, [7] = { [100] = true, [5] = 27 } }
        end)
        assert(not ok and string.find(tostring(err), "a child must be a ui node, string, function or message, not integer", 1, true), tostring(err))
        return ui.row(root)"#,
    );
    assert_eq!(h.texts(), ["a", "b", "c", "d"]);
}

#[test]
fn nil_children_leave_no_gap_in_the_list() {
    let mut h = common::Harness::inline(
        r##"return ui.row {
            ui.text { "a", id = "a" },
            false and ui.text { "b", id = "b" } or nil,
            ui.text { "c", id = "c" },
        }"##,
    );
    assert!(h.find("a").is_some() && h.find("b").is_none());
    assert!(h.find("c").is_some(), "a child after a nil one is kept");
}

#[test]
fn svg_tint_follows_state() {
    let mut h = common::Harness::inline(
        r##"local c = ui.state("c", "#ff0000")
        return ui.column {
            ui.svg { id = "i", w = 10, h = 10, tint = c,
                code = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#fff"/></svg>' },
            ui.button { id = "b", w = 10, h = 10, on_click = ui.set(c, "#00ff00") },
        }"##,
    );
    let svg = |h: &mut common::Harness| {
        let node = h.find("i").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Svg(data) = scene.kind(n) {
                return format!("{data:?}");
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no svg")
    };
    let before = svg(&mut h);
    h.click("b");
    assert_ne!(before, svg(&mut h), "a new tint re-tints the icon");
}

#[test]
fn svg_current_color_follows_the_inherited_text_color() {
    let mut h = common::Harness::inline(
        r##"return ui.button { id = "b", w = 20, h = 20, color = "#ff0000", hover = { color = "#00ff00" },
            ui.svg { id = "i", w = 10, h = 10, tint = "currentColor",
                code = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#fff"/></svg>' },
        }"##,
    );
    let tint = |h: &mut common::Harness| {
        let node = h.find("i").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Svg(data) = scene.kind(n) {
                return data.tint();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no svg")
    };
    assert_eq!(
        tint(&mut h),
        Some(zgx::scene::Color(255, 0, 0, 255)),
        "the button's colour"
    );
    let node = h.find("b").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 5.,
        y: b.y + 5.,
    });
    h.frame();
    assert_eq!(
        tint(&mut h),
        Some(zgx::scene::Color(0, 255, 0, 255)),
        "its hover colour"
    );
}

#[test]
fn wrapping_text_in_a_fitted_bubble_keeps_short_lines_whole() {
    // A chat bubble: as wide as its text up to 77%, the text wrapping.
    for (size, spacing) in [(14., -0.14), (14., 0.), (13., -0.13), (16., 0.)] {
        let mut h = Harness::inline(&format!(
            r##"return ui.column {{ w = 600, items = "end",
                ui.column {{ max_w = "77%", min_w = 0, px = 12, py = 8,
                    ui.rich {{ id = "t", wrap = true, w_full = true, text_size = {size}, line_height = {line},
                        letter_spacing = {spacing}, "hi" }} }} }}"##,
            line = size * 1.5
        ));
        h.ui.prepare_frame();
        let node = h.find("t").unwrap();
        let b = h.ui.scene.borrow().bounds(node);
        assert!(
            b.height <= size * 1.5 + 0.5,
            "{size}/{spacing}: {b:?} wrapped"
        );
    }
}

#[test]
fn hover_backgrounds_take_derived_tokens() {
    let mut h = common::Harness::inline(
        r##"ui.theme.tokens { app = { base = "#204060", hot = "mix({app.base}, #ffffff, 0.5)" } }
        return ui.button { id = "b", w = 20, h = 20, bg = "$app.base", hover = { bg = "$app.hot" } }"##,
    );
    let fill = |h: &mut common::Harness| {
        let node = h.find("b").unwrap();
        let scene = h.ui.scene.borrow();
        match scene.kind(node) {
            zgx::scene::NodeKind::Quad(q) | zgx::scene::NodeKind::Panel { quad: q, .. } => q.fill,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(fill(&mut h), zgx::scene::Color(0x20, 0x40, 0x60, 255));
    let node = h.find("b").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    h.ui.dispatch(zgx::input::InputEvent::PointerMove {
        x: b.x + 5.,
        y: b.y + 5.,
    });
    h.frame();
    let hot = fill(&mut h);
    assert!(hot.0 > 0x80 && hot.3 == 255, "the hover's mix: {hot:?}");
}

#[test]
fn tweens_ease_between_theme_colours() {
    let mut h = common::Harness::inline(
        r##"ui.theme.tokens { app = { rest = "#000000", hot = "#ffffff" } }
        local on = ui.state("on", false)
        return ui.column {
            ui.button { id = "go", w = 20, h = 20, on_click = ui.toggle(on) },
            ui.div { id = "a", w = 20, h = 20,
                bg = ui.tween(on, { duration = 100, ease = "linear", from = "$app.rest", to = "$app.hot" }) },
            ui.div { id = "b", w = 20, h = 20,
                bg = ui.tween(on, { duration = 100, ease = "linear", from = "#000000", to = "$app.hot" }) },
            ui.row { id = "c",
                color = ui.tween(on, { duration = 100, ease = "linear", from = "$app.rest", to = "$app.hot" }),
                ui.text { "inherits" } },
        }"##,
    );
    let fill = |h: &mut common::Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        match scene.kind(node) {
            zgx::scene::NodeKind::Quad(q) | zgx::scene::NodeKind::Panel { quad: q, .. } => q.fill,
            other => panic!("{other:?}"),
        }
    };
    let text = |h: &mut common::Harness| {
        let node = h.find("c").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { color, .. } = scene.kind(n) {
                return *color;
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no text")
    };
    let black = zgx::scene::Color(0, 0, 0, 255);
    assert_eq!(fill(&mut h, "a"), black);
    assert_eq!(fill(&mut h, "b"), black);
    assert_eq!(text(&mut h), black);
    h.click("go");
    let mut seen = Vec::new();
    while h.clock.wants_frame() {
        h.frame();
        seen.push((fill(&mut h, "a").0, fill(&mut h, "b").0, text(&mut h).0));
        assert!(seen.len() < 60, "the tween never settled");
    }
    assert!(
        seen.iter()
            .any(|&(a, b, c)| (1..255).contains(&a) && a == b && a == c),
        "eases through the colours between, the same either way and in inherited text: {seen:?}"
    );
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    let white = zgx::scene::Color(255, 255, 255, 255);
    assert_eq!(fill(&mut h, "a"), white);
    assert_eq!(fill(&mut h, "b"), white);
    assert_eq!(text(&mut h), white);
}

#[test]
fn shader_params_follow_theme_tokens() {
    let mut h = common::Harness::inline(
        r##"ui.theme.tokens { app = { ink = "#ff0000" } }
        local S = ui.shader("fn fill(input: FillInput) -> vec4<f32> { return input.params[1]; }")
        return ui.div { id = "d", w = 10, h = 10, bg = "#00000000", bg_shader = S,
            shader_params = { { 1, 2, 3, 4 }, "$app.ink" },
            on_mount = function(el)
                anime.animate(el, { shaderTime = { 0, 10 }, duration = 1000, ease = "linear" })
            end }"##,
    );
    h.ui.prepare_frame();
    for _ in 0..10 {
        h.frame();
    }
    let node = h.find("d").unwrap();
    let uniforms = h.ui.scene.borrow().shader_uniforms(node);
    assert!(uniforms.time > 0., "time animates: {uniforms:?}");
    assert_eq!(uniforms.params[0], [1., 2., 3., 4.]);
    assert_eq!(
        uniforms.params[1],
        [1., 0., 0., 1.],
        "the token's colour, animated time or not"
    );
}

#[test]
fn shader_params_follow_registered_theme_tokens_while_time_loops() {
    let mut h = common::Harness::inline(
        r##"ui.theme.tokens { app = { ink = "#000000" } }
        ui.theme.register { name = "t-dark", type = "dark", tokens = { ["app.ink"] = "#ff0000" } }
        ui.theme.register { name = "t-light", type = "light", tokens = { ["app.ink"] = "#0000ff" } }
        ui.theme.set("t-dark")
        local S = ui.shader("fn fill(input: FillInput) -> vec4<f32> { return input.params[0]; }", { fps = 30 })
        return ui.column {
            ui.button { id = "light", w = 10, h = 10, on_click = function() ui.theme.set("t-light") end },
            ui.div { id = "d", w = 10, h = 10, bg = "$app.ink", bg_shader = S,
                shader_params = { "$app.ink" }, pointer_events = "none",
                on_mount = function(el)
                    anime.animate(el, { shaderTime = { 40, 1840 }, duration = 3600000, ease = "linear",
                        loop = true, frameRate = 30 })
                end } }"##,
    );
    h.ui.prepare_frame();
    for _ in 0..10 {
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let node = h.find("d").unwrap();
    let uniforms = h.ui.scene.borrow().shader_uniforms(node);
    assert!(uniforms.time >= 40., "time loops from 40: {uniforms:?}");
    assert_eq!(uniforms.params[0], [1., 0., 0., 1.], "the dark theme's ink");
    h.click("light");
    for _ in 0..10 {
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let uniforms = h.ui.scene.borrow().shader_uniforms(node);
    assert_eq!(
        uniforms.params[0],
        [0., 0., 1., 1.],
        "the light theme's, as it changes"
    );
}

/// An `id` given to a component replaces one on its root: nothing changes,
/// and a warning names both.
#[test]
fn a_component_given_an_id_warns_when_it_replaces_its_roots() {
    let h = Harness::inline(
        r##"local Panel = ui.component(function(props)
            return ui.column { id = "detail", ui.text { "x" } }
        end)
        return ui.column { Panel { id = 5 }, Panel { id = "detail" }, Panel {} }"##,
    );
    h.ui.prepare_frame();
    let warnings = h.host.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("`id = \"5\"`") && warnings[0].contains("`id = \"detail\"`"),
        "{warnings:?}"
    );
}

#[test]
fn border_sides_set_each_width() {
    let mut h = common::Harness::inline(
        r##"return ui.div { id = "d", w = 40, h = 40, border_sides = { 1, 1, 0, 1 }, border_color = "#33322D" }"##,
    );
    let node = h.find("d").unwrap();
    let scene = h.ui.scene.borrow();
    let text = format!("{:?}", scene.kind(node));
    assert!(
        text.contains("bottom: 0.0") && text.contains("top: 1.0"),
        "{text}"
    );
}

#[test]
fn on_resize_reports_layout_sizes() {
    let mut h = common::Harness::inline(
        r##"local items = ui.state("items", { 1, 2 })
        local seen = ui.state("seen", "")
        return ui.column { w = 200, items = "start",
            ui.row { flex_wrap = true, gap = 0,
                on_resize = function(size, el) seen:set(size.w .. "x" .. size.h .. " " .. tostring(el.height)) end,
                ui.each(items, function() return ui.div { w = 80, h = 10 } end) },
            ui.text { "{seen}", id = "seen" },
            ui.button { id = "add", w = 10, h = 10, on_click = function() items:push(3) end },
        }"##,
    );
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(
        h.text("seen"),
        "160x10 10",
        "one row, as wide as its two items"
    );
    h.click("add");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(
        h.text("seen"),
        "200x20 20",
        "a third item wraps to a second row"
    );
}

#[test]
fn on_visible_reports_scrolling_into_view() {
    let mut h = common::Harness::inline(
        r##"local off = ui.state("off", 0)
        local seen = ui.state("seen", "")
        return ui.column { w = 200,
            ui.scroll { h = 100, offset = off,
                ui.div { h = 80 },
                ui.div { h = 80 },
                ui.div { h = 80,
                    on_visible = function(on, el) seen:set(seen:get() .. tostring(on) .. " ") end },
            },
            ui.text { "{seen}", id = "seen" },
            ui.button { id = "down", w = 10, h = 10, on_click = function() off:set(140) end },
        }"##,
    );
    h.ui.prepare_frame();
    wait_ms(&mut h, 20);
    assert_eq!(h.text("seen"), "false ", "below the viewport at first");
    h.click("down");
    h.ui.prepare_frame();
    wait_ms(&mut h, 20);
    assert_eq!(h.text("seen"), "false true ", "scrolled into view");
}

#[test]
fn on_visible_handlers_that_move_layout_settle() {
    // Shown, the spacer above grows and pushes the node out of view; out
    // of view, it shrinks again. Delivered after the frame, the handler
    // never feeds back into the layout that reported it.
    let mut h = common::Harness::inline(
        r##"local tall = ui.state("tall", false)
        return ui.scroll { h = 100, w = 100,
            ui.div { h = function() return if tall:get() then 200 else 0 end },
            ui.div { h = 20, on_visible = function(on) tall:set(on) end },
        }"##,
    );
    h.ui.prepare_frame();
    wait_ms(&mut h, 30);
    h.ui.prepare_frame();
}

#[test]
fn border_over_reaches_the_scene() {
    let mut h = common::Harness::inline(
        r##"return ui.div { id = "d", w = 20, h = 20, border = 1, border_color = "#fff", border_over = true, ui.div { w = 20, h = 20, bg = "#000" } }"##,
    );
    let node = h.find("d").unwrap();
    let text = format!("{:?}", h.ui.scene.borrow().kind(node));
    assert!(text.contains("border_over: true"), "{text}");
}

#[test]
fn bg_image_paints_and_survives_an_animated_bg() {
    // In a Luau string, so `/` even on Windows (a `\` would escape).
    let png = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/images/components.png"
    )
    .replace('\\', "/");
    let mut h = common::Harness::inline(&format!(
        r##"return ui.div {{ id = "d", w = 40, h = 40, bg = "#000000", rounded = 8, bg_image = "{png}", bg_fit = "cover",
            on_mount = function(el) anime.animate(el, {{ backgroundColor = "#ff0000", duration = 50 }}) end }}"##
    ));
    for _ in 0..10 {
        h.frame();
    }
    let node = h.find("d").unwrap();
    let text = format!("{:?}", h.ui.scene.borrow().kind(node));
    assert!(
        text.contains("Image {"),
        "the background image is set: {text}"
    );
    assert!(
        text.contains("fill: Color(255, 0, 0"),
        "the animated bg sits under it: {text}"
    );
}

#[test]
fn fades_reach_each_edge() {
    let mut h = common::Harness::inline(
        r##"return ui.scroll_x { id = "s", w = 100, h = 20, fade_x = { 0, 28 }, ui.div { w = 300, h = 20 } }"##,
    );
    let node = h.find("s").unwrap();
    let scene = h.ui.scene.borrow();
    let mut stack = vec![node];
    let mut found = false;
    while let Some(n) = stack.pop() {
        if scene.style(n).fade_edges_x == [0., 28.] {
            found = true;
        }
        stack.extend(scene.children(n).iter().copied());
    }
    assert!(found, "a right fade of 28 points");
}

/// Records a page's commands and answers evaluations.
struct Page(RefCell<Vec<String>>);
impl zgx::web_view::WebViewBackend for Page {
    fn run(&self, command: zgx::web_view::WebViewCommand) {
        self.0.borrow_mut().push(format!("{command:?}"));
        if let zgx::web_view::WebViewCommand::Evaluate {
            reply: Some(reply), ..
        } = command
        {
            zgx::native_view::post(move || reply(Ok(r#"{"n":42,"tags":["a","b"]}"#.into())));
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn web_views_load_their_source_and_report_page_events() {
    use zgx::{native_view::NativeContent, web_view::WebViewEvent};
    let mut h = Harness::inline(
        r##"local web = ui.web { profile = "ephemeral" }
        local tab = ui.state("tab", "a")
        local log = ui.state("log", "")
        local function note(s) log:set(log:get() .. s .. ";") end
        return ui.column {
            ui.webview {
                id = "page", web = web, w = 200, h = 100, rounded = 8,
                src = function() return "https://" .. tab:get() .. ".test/" end,
                on_navigate = function(e) note(e.kind .. " " .. e.url) end,
                on_load = function(url) note("load " .. url) end,
                on_title = function(t) note("title " .. t) end,
                on_message = function(m, origin, top) note(`message {m.kind} {m.count} from {origin} {top}`) end,
                on_error = function(url, err) note("error " .. err) end,
            },
            ui.text { function() return web.title end, id = "title" },
            ui.button { id = "b", w = 20, h = 20, on_click = function() tab:set("b") end },
            ui.button { id = "eval", w = 20, h = 20, on_click = function()
                web:eval("x", function(v) note("eval " .. v.n .. " " .. v.tags[2]) end)
            end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    let page = h.find("page").unwrap();
    assert_eq!(h.natives.nodes(), vec![page]);
    let Some(NativeContent::Web(web)) = h.natives.content(page) else {
        panic!("no page")
    };
    assert!(web.options().profile == zgx::web_view::WebProfile::Ephemeral);
    // Unsupported headless: a labelled box, sized like any node.
    assert_eq!(h.width("page"), 200.);
    let backend = Rc::new(Page(RefCell::new(Vec::new())));
    web.attach(backend.clone());
    assert_eq!(
        *backend.0.borrow(),
        vec!["LoadUrl(\"https://a.test/\")".to_string()]
    );
    h.click("b");
    assert_eq!(
        backend.0.borrow().last().unwrap(),
        "LoadUrl(\"https://b.test/\")"
    );

    web.emit(WebViewEvent::NavigationStarted {
        url: "https://b.test/".into(),
    });
    web.emit(WebViewEvent::NavigationFinished {
        url: "https://b.test/".into(),
    });
    web.update_state(|s| s.title = "B".into());
    web.emit(WebViewEvent::TitleChanged { title: "B".into() });
    web.emit(WebViewEvent::Message {
        body: r#"{"kind":"ping","count":3}"#.into(),
        origin: "https://b.test".into(),
        main_frame: true,
    });
    web.emit(WebViewEvent::NavigationFailed {
        url: "https://c.test/".into(),
        error: "offline".into(),
    });
    h.settle();
    assert_eq!(h.text("title"), "B");
    assert_eq!(
        h.text("log"),
        "started https://b.test/;finished https://b.test/;load https://b.test/;title B;\
         message ping 3 from https://b.test true;failed https://c.test/;error offline;"
    );
    h.click("eval");
    zgx::native_view::deliver_posted();
    h.settle();
    assert!(h.text("log").ends_with("eval 42 b;"), "{}", h.text("log"));
}

#[test]
fn longhand_properties_refine_their_shorthand_in_any_table_order() {
    // Lua tables have no order: `p = 0, px = 20` must pad the sides
    // whatever order the keys come out in, so vary the keys around them.
    let mut h = Harness::inline(
        r##"local rows = {}
        for i = 1, 24 do
            local props = { id = "row" .. i, p = 0, px = 20, py = 3, ui.div { id = "cell" .. i, w = 10, h = 10 } }
            props["h"] = 16 + i % 3
            if i % 2 == 0 then props.gap = i end
            if i % 3 == 0 then props.rounded = i end
            table.insert(rows, ui.row(props))
        end
        return ui.column { rows }"##,
    );
    for i in 1..=24 {
        let row = h.find(&format!("row{i}")).unwrap();
        let cell = h.find(&format!("cell{i}")).unwrap();
        let scene = h.ui.scene.borrow();
        let (r, c) = (scene.bounds(row), scene.bounds(cell));
        assert_eq!(c.x - r.x, 20., "row {i}: px refines p");
        assert_eq!(c.y - r.y, 3., "row {i}: py refines p");
    }
}

#[test]
fn pointer_events_none_lets_presses_through_to_what_is_under() {
    let mut h = Harness::inline(
        r##"local hits = ui.shared("hits", 0)
        return ui.overlay { w = 200, h = 100, on_key = function() return false end,
            ui.button { id = "under", w = 200, h = 100, on_click = function() hits:set(hits:peek() + 1) end, "Under" },
            ui.div { id = "layer", absolute = true, top = 0, left = 0, right = 0, bottom = 0, pointer_events = "none",
                ui.div { w = 10, h = 10 } },
        }"##,
    );
    let hits = h.host.shared("hits", 0.);
    h.click("under");
    assert_eq!(
        hits.with_untracked(|v| v.as_f64()),
        Some(1.),
        "the layer passes the press through"
    );
}

#[test]
fn elements_can_take_keyboard_focus() {
    let mut h = Harness::inline(
        r##"local focused = ui.shared("focused", false)
        local text = ui.state("text", "")
        return ui.column {
            ui.input { id = "field", label = "Field", value = text, on_focus = function() focused:set(true) end,
                on_mount = function(el) el:focus() end },
        }"##,
    );
    let focused = h.host.shared("focused", false);
    h.settle();
    assert_eq!(
        focused.with_untracked(|v| v.clone()),
        zgx_script::Value::Bool(true)
    );
    let _ = h.find("field");
}

#[test]
fn container_focus_reaches_its_input_and_blur_clears_it() {
    let mut h = Harness::inline(
        r##"local container = ui.ref()
        local text = ui.state("text", "")
        return ui.column {
            ui.column { ref = container,
                ui.input { id = "field", value = text } },
            ui.button { id = "take_focus", "Focus", on_click = function()
                assert(container:peek():focus())
                assert(container:peek():focus())
            end },
            ui.button { id = "clear_focus", "Blur", on_click = function()
                assert(container:peek():focus())
                container:peek():blur()
            end },
        }"##,
    );
    h.click("take_focus");
    let focused = h.ui.input.focused().expect("the input takes focus");
    let field = h.find("field").unwrap();
    assert!(focused == field || h.ui.scene.borrow().ancestors(focused).any(|n| n == field));
    h.click("clear_focus");
    assert_eq!(h.ui.input.focused(), None);
    assert!(h.host.error().with_untracked(|e| e.is_none()));
}

#[test]
fn pointer_events_follow_a_state() {
    let mut h = Harness::inline(
        r##"local hits = ui.shared("hits", 0)
        local through = ui.shared("through", "auto")
        return ui.overlay { w = 200, h = 100,
            ui.button { id = "under", w = 200, h = 100, on_click = function() hits:set(hits:peek() + 1) end, "Under" },
            ui.div { id = "layer", absolute = true, top = 0, left = 0, right = 0, bottom = 0,
                pointer_events = through, on_click = function() end },
        }"##,
    );
    let hits = h.host.shared("hits", 0.);
    let through = h.host.shared("through", "auto");
    h.click("under");
    assert_eq!(
        hits.with_untracked(|v| v.as_f64()),
        Some(0.),
        "the layer takes it"
    );
    through.set(zgx_script::Value::Str("none".into()));
    h.settle();
    h.click("under");
    assert_eq!(
        hits.with_untracked(|v| v.as_f64()),
        Some(1.),
        "now it passes through"
    );
    through.set(zgx_script::Value::Bool(true));
    h.settle();
    h.click("under");
    assert_eq!(
        hits.with_untracked(|v| v.as_f64()),
        Some(1.),
        "and stops it again"
    );
}

#[test]
fn container_focus_skips_hidden_fields() {
    let mut h = Harness::inline(
        r##"local container = ui.ref()
        local a, b = ui.state("a", ""), ui.state("b", "")
        return ui.column {
            ui.column { ref = container,
                ui.input { id = "hidden_field", value = a, hidden = true },
                ui.input { id = "field", value = b } },
            ui.button { id = "take_focus", "Focus", on_click = function()
                assert(container:peek():focus())
            end },
        }"##,
    );
    h.click("take_focus");
    let focused = h.ui.input.focused().expect("a field takes focus");
    let field = h.find("field").unwrap();
    assert!(focused == field || h.ui.scene.borrow().ancestors(focused).any(|n| n == field));
}

#[test]
fn a_textarea_numbers_only_the_lines_in_view() {
    let lines: Vec<String> = (1..=2000).map(|i| format!("line {i}")).collect();
    let mut h = Harness::inline(&format!(
        r#"
local text = ui.state("text", {:?})
return ui.textarea {{ id = "code", value = text, line_numbers = true, w = 300, h = 200, font = "mono" }}
"#,
        lines.join("\n")
    ));
    assert_eq!(h.error(), None);
    let numbers: Vec<String> = h
        .texts()
        .into_iter()
        .filter(|t| t.parse::<u32>().is_ok())
        .collect();
    assert_eq!(numbers.first().map(String::as_str), Some("1"));
    assert!(
        (5..40).contains(&numbers.len()),
        "only the lines in view: {}",
        numbers.len()
    );
    // The text starts right of the gutter.
    let editor = h.bounds("code");
    let text_x = {
        let scene = h.ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n)
                && text.starts_with("line 1\n")
            {
                found = Some(scene.bounds(n).x);
            }
            stack.extend(scene.children(n).iter().copied());
        }
        found.expect("the text")
    };
    assert!(text_x > editor.x + 30., "{text_x} vs {editor:?}");
}

#[test]
fn each_rows_follow_their_items_under_a_key() {
    // Keyed rows: a builder taking two parameters is built again when its
    // item changes; one taking the row as state (a third) updates in place.
    let mut h = common::Harness::inline(
        r##"local items = ui.state("items", { { id = "a", text = "one" }, { id = "b", text = "two" } })
        local builds = ui.state("builds", 0)
        local live_builds = ui.state("live_builds", 0)
        local function key(it) return it.id end
        return ui.column {
            ui.each(items, function(it)
                builds:set(builds:peek() + 1)
                return ui.text { id = "plain-" .. it.id, it.text, literal = true }
            end, { key = key }),
            ui.each(items, function(it, _, row)
                live_builds:set(live_builds:peek() + 1)
                return ui.text { id = "live-" .. it.id, function() return row:get().text end }
            end, { key = key }),
            ui.text { id = "counts", "{builds} {live_builds}" },
            ui.button { id = "edit", w = 10, h = 10, on_click = function()
                items:set_key({ 2, "text" }, "TWO")
            end },
            ui.button { id = "again", w = 10, h = 10, on_click = function()
                items:set_key({ 2, "text" }, "Two!")
            end },
            ui.button { id = "swap", w = 10, h = 10, on_click = function()
                local now = items:get()
                items:set({ now[2], now[1] })
            end },
            ui.button { id = "rekey", w = 10, h = 10, on_click = function()
                items:set_key({ 1, "id" }, "c")
            end },
        }"##,
    );
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(h.text("counts"), "2 2");
    h.click("edit");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(h.text("plain-b"), "TWO", "the changed row is built again");
    assert_eq!(h.text("live-b"), "TWO", "the live row follows its item");
    assert_eq!(
        h.text("counts"),
        "3 2",
        "only the changed plain row was built again"
    );
    // A second edit of the same row: the same again.
    h.click("again");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(
        (h.text("plain-b"), h.text("live-b")),
        ("Two!".into(), "Two!".into())
    );
    assert_eq!(h.text("counts"), "4 2");
    // Reordered under their keys: the rows move, none is built again.
    h.click("swap");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(h.text("counts"), "4 2", "rows keep through a reorder");
    assert_eq!(h.text("plain-a"), "one");
    // A key changed: that row is a new one, in both lists.
    h.click("rekey");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(h.text("counts"), "5 3", "a new key builds a new row");
    assert!(h.find("plain-b").is_none() && h.find("plain-c").is_some());
}

#[test]
fn closed_overlays_in_a_long_list_cost_no_nodes() {
    // Two hundred rows, each with a menu and a tooltip: closed, their
    // panels are not built; opening one builds that one alone, and it stays.
    let script = |rows: usize| {
        format!(
            r##"local rows = {{}}
            for i = 1, {rows} do table.insert(rows, i) end
            local list = ui.state("rows", rows)
            local open = ui.state("open", 0)
            return ui.column {{
                ui.button {{ id = "open", w = 10, h = 10, on_click = function() open:set(3) end }},
                ui.button {{ id = "close", w = 10, h = 10, on_click = function() open:set(0) end }},
                ui.each(list, function(i)
                    return ui.row {{ h = 20,
                        ui.text {{ "row " .. i, tooltip = "Row " .. i }},
                        ui.menu {{
                            open = ui.computed(function() return open:get() == i end),
                            anchor = ui.button {{ id = "more-" .. i, w = 10, h = 10 }},
                            ui.menu_item {{ label = "Rename", id = "rename-" .. i }},
                            ui.menu_item {{ label = "Archive" }},
                        }},
                    }}
                end),
            }}"##
        )
    };
    let nodes = |h: &mut common::Harness| {
        h.ui.prepare_frame();
        h.ui.scene.borrow().len()
    };
    let mut few = common::Harness::inline(&script(20));
    let mut many = common::Harness::inline(&script(200));
    let (a, b) = (nodes(&mut few), nodes(&mut many));
    // Per row: what the row shows, no panel.
    let per_row = (b - a) as f32 / 180.;
    assert!(per_row <= 12., "{per_row} nodes a closed row");
    assert!(
        many.find("rename-3").is_none(),
        "nothing inside a closed menu"
    );
    many.click("open");
    let opened = nodes(&mut many);
    assert!(many.find("rename-3").is_some(), "the opened menu is built");
    assert!(opened - b < 40, "{} nodes for one menu", opened - b);
    many.click("close");
    assert_eq!(nodes(&mut many), opened, "closed again, it is kept");
}

/// A keyed list's rows follow their items: removing an item moves the rows
/// after it, each kept (not built again, its text as it was), its index a
/// state that follows where it is; unkeyed rows stay where they are.
#[test]
fn keyed_list_rows_follow_their_items() {
    let mut h = Harness::inline(
        r##"local items = ui.state("items", (function()
            local list = {}
            for i = 1, 50 do list[i] = { id = i * 10, name = `item {i}` } end
            return list
        end)())
        local builds = ui.state("builds", 0)
        return ui.column {
            ui.button { id = "drop", on_click = function() items:remove(1) end, "drop" },
            ui.text { id = "builds", "{builds}" },
            ui.list {
                count = function() return items:len() end,
                height = 20, w = 200, h = 100,
                key = function(i) return items:at(i).id end,
                row = function(id, index)
                    builds:set(builds:peek() + 1)
                    local name = items:peek()[index:peek()].name
                    return ui.text { id = `k-{id}`, function() return `{index:get()}: {name}` end }
                end,
            },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let built: usize = h.text("builds").parse().unwrap();
    assert_eq!(h.text("k-20"), "2: item 2");
    let row = h.find("k-20").unwrap();
    h.click("drop");
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    // The row of item 2 is the same node, now first; at most the row coming
    // into view at the end is new.
    assert_eq!(h.find("k-20"), Some(row));
    assert_eq!(h.text("k-20"), "1: item 2");
    assert!(h.find("k-10").is_none());
    let after: usize = h.text("builds").parse().unwrap();
    assert!(after <= built + 1, "{built} → {after} builds");
}

/// Where the text `id` is on screen: its top and height.
fn span_of(h: &mut Harness, id: &str) -> (f32, f32) {
    let b = h.bounds(id);
    (b.y, b.height)
}

/// Rows of one to five wrapped lines, 20 points each, in a list 200 wide.
const MEASURED: &str = r##"local offset = ui.state("offset", 0)
    local function words(i) return string.rep("word ", (i % 5) * 9 + 1) end
    return ui.column {
        ui.button { id = "far", on_click = function() offset:set(6000) end, "far" },
        ui.list {
            id = "list", count = 500, height = 20, measured = true, offset = offset,
            keyboard = true, w = 200, h = 300,
            row = function(i)
                return ui.text { id = `m-{i}`, words(i), wrap = true, line_height = 20 }
            end,
        },
    }"##;

#[test]
fn measured_list_rows_take_their_contents_height() {
    let mut h = Harness::inline(MEASURED);
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let (mut heights, mut bottom) = (Vec::new(), None::<f32>);
    for i in 1..=5 {
        let (top, height) = span_of(&mut h, &format!("m-{i}"));
        // Each row starts where the one above it ends.
        if let Some(bottom) = bottom {
            assert!((top - bottom).abs() < 0.01, "m-{i} at {top}, not {bottom}");
        }
        bottom = Some(top + height);
        heights.push(height);
    }
    // One line (a word) for the fifth, more for the others: as they wrap.
    assert_eq!(heights[4], 20., "{heights:?}");
    assert!(heights[3] >= 60., "{heights:?}");
    assert!(heights[0] > 20. && heights[0] < heights[1], "{heights:?}");
}

#[test]
fn a_measured_list_keeps_its_place_as_estimates_are_measured() {
    let mut h = Harness::inline(MEASURED);
    h.click("far");
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
    let list = h.bounds("list");
    // A row fully in view, below the top, and where it is.
    let mut shown = None;
    for i in 1..=500 {
        let id = format!("m-{i}");
        if h.find(&id).is_some() && span_of(&mut h, &id).0 >= list.y {
            shown = Some(id);
            break;
        }
    }
    let shown = shown.expect("rows in view");
    let (top, _) = span_of(&mut h, &shown);
    // Up by 100: the rows coming into view above were estimates, measured
    // as they mount; what was in view moves by the 100 alone.
    h.ui.dispatch(zgx::input::InputEvent::Scroll {
        x: list.x + 10.,
        y: list.y + 10.,
        delta_x: 0.,
        delta_y: -100.,
    });
    h.frames(2);
    let (moved, _) = span_of(&mut h, &shown);
    assert!(
        (moved - (top + 100.)).abs() < 0.5,
        "{shown}: {top} → {moved}"
    );
    // And again, with no scrolling: nothing moves.
    h.frames(2);
    assert_eq!(span_of(&mut h, &shown).0, moved);
}

#[test]
fn a_measured_list_jumps_to_its_last_row() {
    let mut h = Harness::inline(MEASURED);
    h.ui.prepare_frame();
    h.click("m-1");
    h.key(Key::End);
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
    // The last row, measured, ends at the list's bottom edge.
    let list = h.bounds("list");
    let (top, height) = span_of(&mut h, "m-500");
    assert_eq!(height, 20., "one word, one line");
    assert!(
        (top + height - (list.y + list.height)).abs() < 0.5,
        "m-500 ends at {}, the list at {}",
        top + height,
        list.y + list.height
    );
    let (above, above_height) = span_of(&mut h, "m-499");
    assert!((above + above_height - top).abs() < 0.01);
}

#[test]
fn a_keyed_measured_list_keeps_heights_with_their_rows() {
    let mut h = Harness::inline(
        r##"local items = ui.state("items", (function()
            local list = {}
            for i = 1, 100 do list[i] = { id = `i{i}`, text = string.rep("word ", (i % 4) * 12 + 1) } end
            return list
        end)())
        local builds = ui.state("builds", 0)
        return ui.column {
            ui.button { id = "drop", on_click = function() items:remove(1) end, "drop" },
            ui.text { id = "builds", "{builds}" },
            ui.list {
                id = "list", count = function() return items:len() end,
                height = 20, measured = true, w = 200, h = 300,
                key = function(i) return items:at(i).id end,
                row = function(id, index)
                    builds:set(builds:peek() + 1)
                    local text = items:peek()[index:peek()].text
                    return ui.text { id = id, text, wrap = true, line_height = 20 }
                end,
            },
        }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    let (_, second) = span_of(&mut h, "i2");
    let (_, third) = span_of(&mut h, "i3");
    assert!(second > 20. && third > second, "{second} {third}");
    let node = h.find("i2").unwrap();
    let built: usize = h.text("builds").parse().unwrap();
    h.click("drop");
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    // The second item's row is first now: the same node, its height kept,
    // the third right below it.
    assert_eq!(h.find("i2"), Some(node));
    let (top, height) = span_of(&mut h, "i2");
    assert_eq!(height, second);
    assert!((span_of(&mut h, "i3").0 - (top + height)).abs() < 0.01);
    assert!(h.find("i1").is_none());
    let after: usize = h.text("builds").parse().unwrap();
    assert!(after <= built + 2, "{built} → {after} builds");
}

/// A split needs no `size` of the script's.
#[test]
fn a_split_takes_a_size_of_its_own() {
    let mut h = Harness::inline(
        r##"return ui.split { w = 600, h = 200,
            ui.column { id = "a", ui.text { "a" } },
            ui.column { id = "b", ui.text { "b" } } }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    let a = h.find("a").unwrap();
    assert_eq!(h.ui.scene.borrow().bounds(a).width, 280.);
}

#[test]
fn measured_and_heights_do_not_go_together() {
    let error = zgx_script::ScriptHost::inline(
        &zgx::reactive::Runtime::new(),
        "app",
        r##"return ui.list { count = 3, measured = true, heights = { 10, 20, 30 },
            row = function(i) return ui.text "row" end }"##,
    )
    .reload()
    .unwrap_err();
    assert!(error.contains("`measured` and `heights`"), "{error}");
}

#[test]
fn a_following_measured_list_stays_at_its_end() {
    let mut h = Harness::inline(
        r##"local lines = ui.state("lines", {} :: { string })
        return ui.column {
            ui.button { id = "send", on_click = function()
                lines:push(string.rep("word ", (lines:len() % 3) * 15 + 1))
            end, "send" },
            ui.list {
                id = "chat", count = function() return lines:len() end,
                height = 20, measured = true, follow = true, w = 200, h = 300,
                row = function(i)
                    return ui.text { id = `c-{i}`, lines:peek()[i], wrap = true, line_height = 20 }
                end,
            },
        }"##,
    );
    for _ in 0..30 {
        h.click("send");
        h.frames(1);
    }
    assert_eq!(h.host.error().get(), None);
    // The newest, measured, ends at the list's bottom edge.
    let list = h.bounds("chat");
    let (top, height) = span_of(&mut h, "c-30");
    assert!(
        (top + height - (list.y + list.height)).abs() < 0.5,
        "c-30 ends at {}, the list at {}",
        top + height,
        list.y + list.height
    );
}

#[test]
fn copying_a_selection_over_a_list_takes_the_rows_it_has_not_mounted() {
    let mut h = Harness::inline(
        r##"local people = ui.state("people", (function()
            local list = {}
            for i = 1, 300 do list[i] = { id = i, name = `Person {i}`, city = if i % 2 == 0 then "Oslo" else "Lima" } end
            return list
        end)())
        return ui.column {
            selectable = true,
            ui.text { "People", id = "title" },
            -- Rows by position, each a row of two texts: copied as shown.
            ui.list {
                id = "plain", count = 300, height = 20, w = 300, h = 100,
                row = function(i)
                    local p = people:peek()[i]
                    return ui.row { ui.text(p.name), ui.text { function() return p.city end } }
                end,
            },
            -- Keyed and measured, the row reading its item by key.
            ui.list {
                id = "keyed", count = function() return people:len() end, height = 20,
                measured = true, w = 300, h = 100,
                key = function(i) return people:at(i).id end,
                row = function(id, index)
                    return ui.text { function() return `{index:get()}. {people:at(id).name}` end, wrap = true }
                end,
            },
            -- A row of its own make: its text given.
            ui.list {
                id = "given", count = 300, height = 20, w = 300, h = 100,
                row_text = function(i) return `Item {i}` end,
                row = function(i) return ui.canvas { w_full = true, h = 20, draw = function() end } end,
            },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let title = h.find("title").unwrap();
    assert!(h.ui.select_all_text(title));
    let text = h.ui.selected_text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "People");
    assert_eq!(lines[1], "Person 1\tLima");
    assert_eq!(lines[300], "Person 300\tOslo");
    assert_eq!(lines[301], "1. Person 1");
    assert_eq!(lines[600], "300. Person 300");
    assert_eq!(lines[601], "Item 1");
    assert_eq!(lines[900], "Item 300");
    assert_eq!(lines.len(), 901, "{text}");
    assert_eq!(h.host.error().get(), None);
}

#[test]
fn a_list_scrolls_to_a_far_row_where_asked() {
    let mut h = Harness::inline(
        r##"local list = ui.ref()
        local plain = ui.ref()
        local function words(i) return string.rep("word ", (i % 5) * 9 + 1) end
        local function go(id, row, align)
            return ui.button { id = id, on_click = function()
                for _, r in { list, plain } do
                    local el = r:peek()
                    if el then el:scroll_to_row(row, { align = align }) end
                end
            end, id }
        end
        return ui.column {
            ui.row { go("s400", 400, "start"), go("e250", 250, "end"), go("c470", 470, "center") },
            ui.list {
                id = "measured", ref = list, count = 500, height = 20, measured = true,
                w = 200, h = 300,
                row = function(i)
                    return ui.text { id = `m-{i}`, words(i), wrap = true, line_height = 20 }
                end,
            },
            ui.list {
                id = "fixed", ref = plain, count = 10000, height = 24, w = 200, h = 240,
                row = function(i) return ui.text { id = `f-{i}`, `Row {i}`, h = 24 } end,
            },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let span = |h: &mut Harness, id: &str| {
        let b = h.bounds(id);
        (b.y, b.y + b.height)
    };
    let measured = h.bounds("measured");
    let fixed = h.bounds("fixed");
    h.click("s400");
    h.frames(2);
    assert_eq!(h.host.error().get(), None);
    assert!((span(&mut h, "m-400").0 - measured.y).abs() < 0.5);
    assert_eq!(span(&mut h, "f-400").0, fixed.y);
    h.click("e250");
    h.frames(2);
    assert!((span(&mut h, "m-250").1 - (measured.y + measured.height)).abs() < 0.5);
    assert_eq!(span(&mut h, "f-250").1, fixed.y + fixed.height);
    h.click("c470");
    h.frames(2);
    let (top, bottom) = span(&mut h, "m-470");
    assert!(((top + bottom) / 2. - (measured.y + measured.height / 2.)).abs() < 0.5);
    let (top, bottom) = span(&mut h, "f-470");
    assert_eq!((top + bottom) / 2., fixed.y + fixed.height / 2.);
}

#[test]
fn a_canvas_view_pans_its_drawing_without_drawing_it_again() {
    let mut h = Harness::inline(
        r##"local view = ui.state("view", { x = 0, y = 0, zoom = 1 })
        local draws = ui.state("draws", 0)
        return ui.column {
            ui.button { id = "pan", on_click = function()
                local v = view:peek()
                view:set { x = v.x + 10, y = v.y - 4, zoom = 2 }
            end, "pan" },
            ui.text { id = "draws", "{draws}" },
            ui.canvas {
                id = "board", w = 100, h = 100, view = view,
                bg_grid = { step = 10, min_step = 4, dot = 2, color = "#00ff00" },
                draw = function(c)
                    draws:set(draws:peek() + 1)
                    c:rect(0, 0, 10, 10, "#ff0000")
                end,
            },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let drawn = h.text("draws");
    assert_ne!(drawn, "0");
    let board = h.find("board").unwrap();
    let drawing = {
        let scene = h.ui.scene.borrow();
        *scene
            .children(board)
            .iter()
            .find(|id| matches!(scene.kind(**id), zgx::scene::NodeKind::Canvas(_)))
            .unwrap()
    };
    h.click("pan");
    h.ui.prepare_frame();
    assert_eq!(h.text("draws"), drawn, "panned, not drawn again");
    let scene = h.ui.scene.borrow();
    assert_eq!(
        scene.canvas_view(drawing),
        zgx::scene::CanvasView {
            x: 10.,
            y: -4.,
            scale: 2.
        }
    );
    let grid = scene.grid_fill(drawing).copied().unwrap();
    assert_eq!((grid.x, grid.y, grid.scale, grid.step), (10., -4., 2., 10.));
    assert_eq!(grid.mark, zgx::scene::GridMark::Dot(2.));
    assert_eq!(grid.color, zgx::scene::Color(0, 255, 0, 255));
}

#[test]
fn a_canvas_draws_again_only_the_items_written_since() {
    let mut h = Harness::inline(
        r##"local shapes = ui.state("shapes", {
            { x = 0, color = "#ff0000" }, { x = 10, color = "#00ff00" }, { x = 20, color = "#0000ff" },
        })
        local calls = 0
        local drawn = ui.state("drawn", "")
        return ui.column {
            ui.button { id = "move", on_click = function() shapes:set_key({ 2, "x" }, 50) end, "move" },
            ui.button { id = "add", on_click = function() shapes:push { x = 70, color = "#ffffff" } end, "add" },
            ui.button { id = "drop", on_click = function() shapes:remove(1) end, "drop" },
            ui.button { id = "count", on_click = function() drawn:set(tostring(calls)) end, "count" },
            ui.text { id = "drawn", "{drawn}" },
            ui.canvas {
                id = "board", w = 100, h = 100, each = shapes,
                draw_item = function(c, s)
                    calls += 1
                    c:rect(s.x, 0, 5, 5, s.color)
                end,
            },
        }"##,
    );
    assert_eq!(h.host.error().get(), None);
    h.ui.prepare_frame();
    let board = h.find("board").unwrap();
    let calls = |h: &mut Harness| {
        h.click("count");
        h.ui.prepare_frame();
        h.text("drawn")
    };
    // The rectangles' x, in the order drawn.
    let xs = |h: &mut Harness| {
        let scene = h.ui.scene.borrow();
        let id = *scene
            .children(board)
            .iter()
            .find(|id| matches!(scene.kind(**id), zgx::scene::NodeKind::Canvas(_)))
            .unwrap();
        let zgx::scene::NodeKind::Canvas(canvas) = scene.kind(id) else {
            unreachable!()
        };
        canvas
            .commands()
            .iter()
            .map(|draw| match draw {
                zgx::canvas::Draw::Fill { path, .. } => match path.shape() {
                    Some(zgx::canvas::Shape::Rect { rect, .. }) => rect.x,
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(calls(&mut h), "3");
    assert_eq!(xs(&mut h), [0., 10., 20.]);
    h.click("move");
    h.ui.prepare_frame();
    assert_eq!(calls(&mut h), "4", "the moved item alone");
    assert_eq!(xs(&mut h), [0., 50., 20.]);
    h.click("add");
    h.ui.prepare_frame();
    assert_eq!(calls(&mut h), "5", "the pushed item alone");
    h.click("drop");
    h.ui.prepare_frame();
    assert_eq!(calls(&mut h), "5", "a removal draws nothing");
    assert_eq!(xs(&mut h), [50., 20., 70.]);
}

#[test]
fn an_image_shows_a_bitmap_given_kept_in_state_or_returned() {
    let mut h = Harness::inline(
        r##"local white = function() return 1 end
        local pic = ui.state("pic", ui.bitmap(4, 2, white))
        local shown = ui.state("shown", true)
        return ui.column {
            ui.image { id = "given", src = ui.bitmap(3, 3, white) },
            ui.image { id = "kept", src = pic },
            ui.image { id = "returned", src = function()
                return if shown:get() then pic:get() else nil
            end },
            ui.button { id = "swap", w = 20, h = 20, on_click = function()
                pic:set(ui.bitmap(5, 1, white))
            end },
            ui.button { id = "hide", w = 20, h = 20, on_click = function() shown:set(false) end },
        }"##,
    );
    assert_eq!(h.error(), None);
    // The picture's width and height, inside node `id`.
    let size = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let NodeKind::Image(image) = scene.kind(n) {
                return (image.width(), image.height());
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no image in `{id}`")
    };
    assert_eq!(size(&mut h, "given"), (3, 3));
    assert_eq!(size(&mut h, "kept"), (4, 2));
    assert_eq!(size(&mut h, "returned"), (4, 2));
    h.click("swap");
    h.frames(1);
    assert_eq!(size(&mut h, "kept"), (5, 1), "follows the state");
    assert_eq!(size(&mut h, "returned"), (5, 1));
    h.click("hide");
    h.frames(1);
    assert_eq!(size(&mut h, "returned"), (1, 1), "nothing shown: empty");
    assert_eq!(h.error(), None);
}

#[test]
fn a_controlled_input_keeps_its_typing_and_shows_changes_from_elsewhere() {
    let mut h = Harness::inline(
        r##"local name = ui.state("name", "")
        local told = ui.state("told", "")
        ui.command { id = "clear", run = function() name:set("") end }
        return ui.column {
            ui.input { id = "field", w = 200,
                -- Shown in capitals: what is typed stays as typed meanwhile.
                value = function() return name:get():upper() end,
                on_change = function(t) name:set(t); told:set(t) end },
            ui.text { id = "told", "{told}" },
            ui.button { id = "other", w = 40, h = 20, "o" },
        }"##,
    );
    h.type_text("field", "ada");
    h.frames(1);
    assert_eq!(h.text("told"), "ada");
    assert_eq!(h.text("field"), "ada", "the function's answer waits");
    // A change from elsewhere shows while it is focused, as a send
    // clearing a composer does.
    assert!(h.host.run_command("clear"));
    h.frames(1);
    assert_eq!(h.text("told"), "ada", "not told back to on_change");
    h.ui.dispatch(zgx::input::InputEvent::Text("b".into()));
    h.settle();
    h.frames(1);
    assert_eq!(h.text("told"), "b", "typing goes on from what it shows");
    // Left, it shows the function's text.
    h.click("other");
    h.frames(1);
    assert_eq!(h.text("field"), "B");
    assert_eq!(h.error(), None);
}

#[test]
fn an_input_bound_to_a_state_commits_on_enter_and_on_leaving_it_changed() {
    let mut h = Harness::inline(
        r##"local draft = ui.state("draft", "")
        local committed = ui.state("committed", "")
        local commits = ui.state("commits", 0)
        return ui.column {
            ui.input { id = "field", w = 200, value = draft,
                on_commit = function(t) committed:set(t); commits:add(1) end },
            ui.text { id = "committed", "{committed}" },
            ui.text { id = "commits", "{commits}" },
            ui.button { id = "other", w = 40, h = 20, "o" },
        }"##,
    );
    h.type_text("field", "ada");
    h.frames(1);
    assert_eq!(h.text("commits"), "0", "typing doesn't commit");
    h.key(zgx::input::Key::Enter);
    h.frames(1);
    assert_eq!(h.text("committed"), "ada");
    assert_eq!(h.text("commits"), "1");
    // Left unchanged since Enter: nothing more.
    h.click("other");
    h.frames(1);
    assert_eq!(h.text("commits"), "1");
    // Left changed: told once more.
    h.type_text("field", "!");
    h.click("other");
    h.frames(1);
    assert_eq!(h.text("committed"), "ada!");
    assert_eq!(h.text("commits"), "2");
    assert_eq!(h.error(), None);
}

#[test]
fn a_list_of_data_builds_rows_from_items_and_runs_only_the_row_that_changed() {
    let mut h = Harness::inline(
        r##"local messages = ui.state("messages", {
            { id = "a", text = "one" }, { id = "b", text = "two" }, { id = "c", text = "three" },
        })
        local built = ui.state("built", 0)
        ui.command { id = "grow", run = function()
            messages:set_key({ 2, "text" }, messages:peek()[2].text .. "!")
        end }
        ui.command { id = "first", run = function() messages:remove(1) end }
        ui.command { id = "retitle", run = function() messages:set_key({ 1, "title" }, "x") end }
        return ui.column { w = 300, h = 400,
            ui.text { id = "built", "{built}" },
            -- Built once each, following its item.
            ui.list { id = "live", w = 300, h = 120, height = 20,
                data = messages, key = function(m) return m.id end,
                row = function(m, index, row)
                    built:set(built:peek() + 1)
                    return ui.text { id = `live-{m.id}`, function()
                        return `{index:get()} {row:get().text}`
                    end }
                end },
            -- Built again when its item changes, by place.
            ui.list { id = "plain", w = 300, h = 120, height = 20, data = messages,
                row = function(m, index)
                    return ui.text { id = `plain-{index:peek()}`, m.text, literal = true }
                end },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.text("live-b"), "2 two");
    assert_eq!(h.text("plain-3"), "three");
    assert_eq!(h.text("built"), "3");
    // One item changes: its row's function runs, no row is built.
    let calls = h.calls();
    assert!(h.host.run_command("grow"));
    h.frames(1);
    assert_eq!(h.text("live-b"), "2 two!");
    assert_eq!(h.text("plain-2"), "two!");
    assert_eq!(h.text("built"), "3");
    // The command, the live row's text and the plain row's build.
    assert!(h.calls() - calls <= 4, "{} calls", h.calls() - calls);
    // Keyed rows move with their items; their index follows.
    assert!(h.host.run_command("first"));
    h.frames(1);
    assert!(h.find("live-a").is_none());
    assert_eq!(h.text("live-b"), "1 two!");
    assert_eq!(h.text("live-c"), "2 three");
    assert_eq!(h.text("plain-1"), "two!");
    assert_eq!(h.text("built"), "3");
    assert_eq!(h.error(), None);
}

#[test]
fn a_list_of_data_takes_no_count() {
    let runtime = zgx::reactive::Runtime::new();
    let host = zgx_script::ScriptHost::inline(
        &runtime,
        "app",
        r#"return ui.list { data = ui.state("l", {}), count = 3, row = function() return ui.div {} end }"#,
    );
    let error = host.reload().unwrap_err();
    assert!(error.contains("not with `count`"), "{error}");
}

#[test]
fn a_markdown_view_draws_blocks_and_builds_again_only_those_that_change() {
    let mut h = Harness::with_desktop(|runtime| {
        zgx_script::ScriptHost::inline(
            runtime,
            "app",
            r##"local reply = ui.state("reply", "# Title\n\nSome **bold** words.\n\n```lua\nprint(1)\n```\n\nThen")
            ui.command { id = "more", run = function() reply:set(reply:peek() .. " more") end }
            return ui.column { w = 300, h = 280, ui.markdown_view { "{reply}", w_full = true } }"##,
        )
    });
    h.frames(1);
    let texts = h.texts();
    for want in [
        "Title",
        "Some bold words.",
        "lua",
        "print(1)",
        "Copy",
        "Then",
    ] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    // The code block before the growing paragraph keeps its nodes.
    let copy = h.find("copy").unwrap();
    assert!(h.host.run_command("more"));
    h.frames(1);
    assert!(h.texts().iter().any(|t| t == "Then more"));
    assert_eq!(
        h.find("copy"),
        Some(copy),
        "the code block was not built again"
    );
    // Copy puts the code on the clipboard.
    h.click("copy");
    h.frames(1);
    assert_eq!(h.desktop.as_ref().unwrap().clipboard(), "print(1)");
    h.frames(1);
    assert!(h.texts().iter().any(|t| t == "Copied"));
    assert_eq!(h.error(), None);
}

#[test]
fn a_buttons_variant_styles_what_its_props_leave_unset() {
    let mut h = Harness::inline(
        r##"return ui.column {
            ui.button { id = "plain", "Send" },
            ui.button { id = "primary", "Send", variant = "primary" },
            ui.button { id = "icon", "x", variant = "ghost", size = 24 },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.bounds("primary").height, h.bounds("plain").height);
    // `size` stays the box's, as on any element: no variant of its own.
    assert!(h.find("icon").is_some());
    assert_eq!(h.error(), None);
    let runtime = zgx::reactive::Runtime::new();
    let host = zgx_script::ScriptHost::inline(
        &runtime,
        "app",
        r#"return ui.button { "x", variant = "loud" }"#,
    );
    let error = host.reload().unwrap_err();
    assert!(error.contains("`variant` is"), "{error}");
}

#[test]
fn a_subtree_provides_values_its_components_and_rows_read() {
    let mut h = Harness::inline(
        r##"local user = ui.state("user", "Ada")
        local Badge = ui.component(function(props: { id: string })
            local who = ui.use_context("user")
            local tone = ui.use_context("tone", "plain")
            return ui.text { id = props.id, function() return `{who:get()} {tone}` end }
        end)
        local names = ui.state("names", { "a" })
        ui.command { id = "rename", run = function() user:set("Grace") end }
        return ui.column {
            provide = { user = user },
            Badge { id = "outer" },
            ui.column { provide = { tone = "loud" },
                Badge { id = "inner" },
                ui.each(names, function(n) return Badge { id = `row-{n}` } end),
            },
        }"##,
    );
    h.frames(1);
    assert_eq!(h.text("outer"), "Ada plain");
    assert_eq!(
        h.text("inner"),
        "Ada loud",
        "nearer values over farther, the rest kept"
    );
    assert_eq!(h.text("row-a"), "Ada loud", "rows read it too");
    // A state provided is the state: what reads it follows it.
    assert!(h.host.run_command("rename"));
    h.frames(1);
    assert_eq!(h.text("inner"), "Grace loud");
    assert_eq!(h.error(), None);
}

#[test]
fn a_text_given_a_state_follows_it_without_the_script() {
    let mut h = Harness::inline(
        r##"local Counter = ui.component(function()
            local count = ui.use_state(2)
            return ui.column {
                ui.text { id = "count", count },
                ui.button { id = "add", w = 40, h = 20, on_click = ui.add(count, 1), "+" },
            }
        end)
        return Counter {}"##,
    );
    h.frames(1);
    assert_eq!(h.text("count"), "2");
    let calls = h.calls();
    h.click("add");
    h.frames(1);
    assert_eq!(h.text("count"), "3");
    assert_eq!(
        h.calls(),
        calls,
        "no script ran: the add and the text are Rust's"
    );
    assert_eq!(h.error(), None);
}

/// An app's scene as it shows: every node in document order, its kind,
/// text and box: what two renders are compared by.
fn scene_shown(h: &mut Harness) -> String {
    h.ui.prepare_frame();
    let scene = h.ui.scene.borrow();
    let mut out = String::new();
    let mut stack = vec![(scene.root(), 0)];
    while let Some((node, depth)) = stack.pop() {
        let b = scene.bounds(node);
        let text = match scene.kind(node) {
            zgx::scene::NodeKind::Text { text, .. } => format!(" {text:?}"),
            zgx::scene::NodeKind::RichText { text, .. } => format!(" {:?}", text.text()),
            _ => String::new(),
        };
        out.push_str(&format!(
            "{}{}{text} {:.1},{:.1} {:.1}x{:.1}\n",
            "  ".repeat(depth),
            zgx::inspect::kind_name(&scene, node),
            b.x,
            b.y,
            b.width,
            b.height
        ));
        for child in scene.children(node).iter().rev() {
            stack.push((*child, depth + 1));
        }
    }
    out
}

#[test]
fn a_streamed_markdown_view_matches_a_full_render_after_every_piece() {
    let document = "# Plan\n\nSome **bold** words, then `code` and a [link](https://example.com).\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nAfter the code.\n\n1. one\n2. two\n3. three continues\n   on a second line\n\n- a\n- b\n\n> quoted\n> twice\n\n## Done\n\nThe end.";
    const APP: &str =
        r#"return ui.column { w = 380, h = 900, ui.markdown_view { "{reply}", w_full = true } }"#;
    let app = |text: &str| {
        let mut reply = None;
        let h = Harness::new(|runtime| {
            let host = zgx_script::ScriptHost::inline(runtime, "app", APP);
            reply = Some(host.shared("reply", zgx_script::Value::from(text)));
            host
        });
        (h, reply.unwrap())
    };
    let (mut streamed, reply) = app("");
    let mut at = 0;
    while at < document.len() {
        // Pieces of 1 to 7 bytes, at character boundaries.
        let mut next = (at + 1 + at % 7).min(document.len());
        while !document.is_char_boundary(next) {
            next += 1;
        }
        let so_far = &document[..next];
        reply.set(zgx_script::Value::from(so_far));
        streamed.frames(1);
        let (mut whole, _) = app(so_far);
        whole.frames(1);
        assert_eq!(
            scene_shown(&mut streamed),
            scene_shown(&mut whole),
            "after {next} bytes: {so_far:?}"
        );
        at = next;
    }
    assert_eq!(streamed.error(), None);
}

#[test]
fn a_shrunk_column_grows_back_to_content_that_outgrew_it() {
    let mut h = Harness::inline(
        r##"local tall = ui.state("tall", 290)
        ui.command { id = "grow", run = function() tall:set(310) end }
        return ui.column { w = 380, h = 900, id = "outer",
            ui.column { id = "inner", ui.div { w = 10, h = tall } } }"##,
    );
    h.frames(1);
    let before = h.bounds("outer").height;
    assert!(h.host.run_command("grow"));
    h.frames(1);
    let streamed = h.bounds("outer").height;
    let mut fresh = Harness::inline(
        r##"return ui.column { w = 380, h = 900, id = "outer",
            ui.column { id = "inner", ui.div { w = 10, h = 310 } } }"##,
    );
    fresh.frames(1);
    assert_eq!(streamed, fresh.bounds("outer").height, "was {before}");
}

/// A Luau input's blinking caret moves nothing, by a pixel, and damages the
/// caret alone: a field sized by its text and a multiline one, beside text
/// that would shift if either did.
#[test]
fn a_luau_inputs_blinking_caret_moves_nothing() {
    use std::time::Duration;
    for multiline in [false, true] {
        let mut h = Harness::inline(&format!(
            r#"
local name = ui.state("name", "blink, and nothing moves")
return ui.column {{
    ui.row {{
        ui.text {{ id = "before", "before" }},
        ui.input {{ id = "field", label = "Name", value = name, multiline = {multiline} }},
        ui.text {{ id = "after", "after" }},
    }},
}}
"#
        ));
        h.click("field");
        h.frame();
        h.ui.set_caret_blink(Some(zgx::caret_blink::CaretBlink {
            on: Duration::from_millis(500),
            off: Duration::from_millis(400),
            timeout: None,
        }));
        h.ui.scene.borrow_mut().flush();
        let boxes = |h: &Harness| {
            let scene = h.ui.scene.borrow();
            let mut out = Vec::new();
            let mut stack = vec![scene.root()];
            while let Some(node) = stack.pop() {
                out.push((node, scene.bounds(node)));
                stack.extend(scene.children(node).iter().rev().copied());
            }
            out
        };
        let before = boxes(&h);
        let caret = h.ui.focused_editor().expect("the field is focused").caret;
        for blink in 0..6 {
            let at = h.ui.next_interaction_deadline().expect("blinking");
            assert!(h.ui.advance_interactions(at).unwrap());
            h.ui.prepare_frame();
            let report = h.ui.scene.borrow_mut().flush();
            assert_eq!(
                report.layout_nodes, 0,
                "multiline {multiline}, blink {blink}"
            );
            assert!(
                boxes(&h) == before,
                "multiline {multiline}, blink {blink}: a box moved"
            );
            let shown = h.ui.scene.borrow().bounds(caret);
            for damage in &report.damage {
                assert!(
                    damage.x >= shown.x - 1.
                        && damage.y >= shown.y - 1.
                        && damage.x + damage.width <= shown.x + shown.width + 1.
                        && damage.y + damage.height <= shown.y + shown.height + 1.,
                    "multiline {multiline}, blink {blink}: {damage:?} past the caret {shown:?}"
                );
            }
        }
    }
}

#[test]
fn an_element_zooms_from_a_script_and_follows_its_state() {
    let mut h = Harness::inline(
        r##"local big = ui.state("big", false)
        return ui.column {
            ui.row {
                ui.column { id = "panel", zoom = function() return big:get() and 2 or 1 end,
                    ui.div { w = 50, h = 20 },
                    ui.div { id = "fixed", zoom_fixed = true, w = 30, h = 10 } },
                ui.div { id = "after", w = 10, h = 10 } },
            ui.button { id = "b", on_click = function() big:set(true) end, "zoom" } }"##,
    );
    h.frames(2);
    h.ui.prepare_frame();
    let shown = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.scene.borrow().transformed_bounds(node)
    };
    assert_eq!(shown(&mut h, "panel").width, 50.);
    assert_eq!(shown(&mut h, "after").x, 50.);
    h.click("b");
    h.frames(2);
    h.ui.prepare_frame();
    // Twice as large, laid out again: what follows moves past it. Its
    // fixed child takes 5 of its points, 10 on screen.
    let panel = shown(&mut h, "panel");
    assert_eq!((panel.width, panel.height), (100., 50.));
    assert_eq!(shown(&mut h, "after").x, 100.);
    // A fixed node keeps its own size inside it.
    let fixed = shown(&mut h, "fixed");
    assert_eq!((fixed.width, fixed.height), (30., 10.));
}

#[test]
fn a_zoomable_element_takes_the_control_wheel_and_its_state_follows() {
    let mut h = Harness::inline(
        r##"local zoom = ui.state("zoom", 1)
        return ui.column { items = "start",
            ui.column { id = "doc", zoomable = zoom, ui.div { w = 100, h = 50 } },
            ui.text { id = "level", function() return string.format("%.2f", zoom:get()) end } }"##,
    );
    h.frames(2);
    h.ui.prepare_frame();
    h.ui.dispatch(zgx::input::InputEvent::PointerMove { x: 20., y: 20. });
    let taken = zgx::zoom::wheel_notched(40., || {
        h.ui.dispatch(zgx::input::InputEvent::Scroll {
            x: 20.,
            y: 20.,
            delta_x: 0.,
            delta_y: -40.,
        })
        .default_prevented
    });
    assert!(taken, "the zoomable takes the Control-wheel over it");
    h.frames(2);
    h.ui.prepare_frame();
    // A level in, shown larger, and the script's state says so.
    let doc = h.find("doc").unwrap();
    assert_eq!(h.ui.scene.borrow().transformed_bounds(doc).width, 110.);
    assert_eq!(h.text("level"), "1.10");
}

/// A settled spring or tween between values far apart (±1e36, whose
/// difference times the spring's stiffness passes f32's range) settles,
/// as nearer values do: nothing keeps asking for frames.
fn retargeted_far_settles(curve: &str, amplitude: f64) -> bool {
    let h = Harness::inline(&format!(
        r#"local x = ui.state("x", 0)
        return ui.div {{ id = "dot", w = 20, h = 20, opacity = ui.{curve}(x) }}"#
    ));
    h.host
        .set_state("x", zgx_script::Value::Num(amplitude))
        .unwrap();
    h.settle();
    h.frames(600);
    assert!(
        !h.clock.wants_frame(),
        "{curve} {amplitude} settles the first time"
    );
    h.host
        .set_state("x", zgx_script::Value::Num(-amplitude))
        .unwrap();
    h.settle();
    h.frames(1200);
    !h.clock.wants_frame()
}

#[test]
fn a_spring_or_tween_between_far_values_settles() {
    for curve in ["spring", "tween"] {
        assert!(retargeted_far_settles(curve, 1e30), "{curve} 1e30");
        assert!(retargeted_far_settles(curve, 1e36), "{curve} 1e36");
        assert!(retargeted_far_settles(curve, 3e38), "{curve} 3e38");
    }
}

#[test]
fn a_textarea_styles_its_highlight_spans_and_takes_its_own_caret() {
    use zgx::scene::Color;
    let mut h = Harness::inline(
        r##"local text = ui.state("text", "a **bold** `code`")
        return ui.column {
            ui.textarea { id = "draft", value = text, w = 300, h = 100, wrap = true,
                caret_color = "#ff4000", caret_width = 2,
                highlight = function(line, state)
                    local spans = {}
                    local a, b = string.find(line, "%*%*.-%*%*")
                    if a then
                        table.insert(spans, { a, a + 1, "#808080" })
                        table.insert(spans, { a, b, weight = 600 })
                        table.insert(spans, { b - 1, b, "#808080" })
                    end
                    local c, d = string.find(line, "`.-`")
                    if c then
                        table.insert(spans, { c, d, "$accent", bg = "#202020", italic = true, strike = true })
                    end
                    return spans, state
                end },
            ui.input { id = "name", value = ui.state("name", ""), caret_color = "$danger" },
        }"##,
    );
    assert_eq!(h.error(), None);
    let draft = h.find("draft").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(draft));
    h.frames(1);
    let editor = h.ui.focused_editor().unwrap();
    {
        let scene = h.ui.scene.borrow();
        assert_eq!(scene.bounds(editor.caret).width, 2.);
        assert_eq!(
            scene.kind(editor.caret),
            &NodeKind::Rect(Color(255, 64, 0, 255))
        );
    }
    let runs = {
        let scene = h.ui.scene.borrow();
        let mut pending = vec![draft];
        let mut found = None;
        while let Some(id) = pending.pop() {
            if let NodeKind::RichText { text } = scene.kind(id) {
                found = Some(
                    text.runs()
                        .iter()
                        .map(|run| (text.text()[run.range.clone()].to_string(), run.clone()))
                        .collect::<Vec<_>>(),
                );
            }
            pending.extend(scene.children(id).iter().copied());
        }
        found.expect("the line shows styled")
    };
    let run = |text: &str| runs.iter().find(|(t, _)| t == text).unwrap().1.clone();
    // The delimiters grey, the whole bold: spans merge.
    assert_eq!(run("**").font.weight, 600);
    assert_eq!(run("**").color, Color(128, 128, 128, 255));
    assert_eq!(run("bold").font.weight, 600);
    let code = run("`code`");
    assert!(code.font.italic && code.strikethrough.is_some());
    assert_eq!(code.background, Some(Color(32, 32, 32, 255)));
    assert_eq!(run("a ").font.weight, 400);
    // A token colours the input's caret; its width is the app's.
    let name = h.find("name").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(name));
    h.frames(1);
    let editor = h.ui.focused_editor().unwrap();
    let danger = zgx::theme::Themes::of(&h.ui.runtime)
        .root()
        .color(zgx::theme::tok::DANGER);
    let scene = h.ui.scene.borrow();
    assert_eq!(scene.kind(editor.caret), &NodeKind::Rect(danger));
    assert_eq!(scene.bounds(editor.caret).width, h.ui.caret_width());
}

/// A progress bar takes a number or a function, as a list row's does.
#[test]
fn a_progress_bar_takes_a_number_or_a_function() {
    let mut h = Harness::inline(
        r##"local v = ui.state("v", 0.5)
        return ui.column {
            ui.progress { id = "number", value = 0.25 },
            ui.progress { id = "one", value = 1 },
            ui.progress { id = "function", value = function() return v:get() end } }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    assert!(h.find("number").is_some() && h.find("function").is_some());
}

#[test]
fn a_paste_is_heard_first_and_may_be_taken() {
    let mut h = Harness::inline(
        r##"local heard = ui.state("heard", "")
        local text = ui.state("text", "")
        local box = ui.ref()
        return ui.column {
            ui.textarea { id = "box", ref = box, value = text, w = 200, h = 60, on_paste = function(e)
                heard:set(e.text or "no text")
                return e.text == nil or #e.text > 8
            end },
            ui.textarea { id = "plain", value = ui.state("other", ""), w = 200, h = 60 },
            ui.textarea { id = "locked", value = ui.state("kept", "x"), w = 200, h = 60, read_only = true,
                on_paste = function() heard:set("locked") return true end },
            ui.text { id = "heard", "{heard}" },
            ui.text { id = "shown", "{text}" },
            ui.button { id = "swap", w = 20, h = 20, on_click = function()
                box:get():replace_selection("from a script")
            end },
        }"##,
    );
    h.frames(1);
    let editor = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        h.ui.editor(node).unwrap()
    };
    let paste = |h: &mut Harness, id: &str, text: Option<&str>| {
        let editor = editor(h, id);
        h.ui.paste_into(&editor, text);
        h.frames(1);
    };
    // Not taken: it goes in as ever.
    paste(&mut h, "box", Some("short"));
    assert_eq!(h.text("heard"), "short");
    assert_eq!(h.text("shown"), "short");
    // Taken: nothing goes in.
    paste(&mut h, "box", Some("a long text, an attachment"));
    assert_eq!(h.text("heard"), "a long text, an attachment");
    assert_eq!(h.text("shown"), "short");
    // No text (an image): heard, and nothing to put in.
    paste(&mut h, "box", None);
    assert_eq!(h.text("heard"), "no text");
    assert_eq!(h.text("shown"), "short");
    // An editor without it pastes as it did; a read-only one hears nothing.
    paste(&mut h, "plain", Some("in"));
    assert_eq!(
        editor(&mut h, "plain").editor.borrow().text().to_string(),
        "in"
    );
    paste(&mut h, "locked", Some("no"));
    assert_eq!(h.text("heard"), "no text");
    // A script's own edit isn't a person's paste.
    h.click("swap");
    h.frames(1);
    assert_eq!(h.text("heard"), "no text");
    assert!(
        h.text("shown").contains("from a script"),
        "{}",
        h.text("shown")
    );
    assert_eq!(h.error(), None);
}

/// `c:text` lays a line over the drawing, placed by its anchor, measured by
/// `c:measure_text`, and follows a state as the drawing does.
#[test]
fn a_canvas_draws_text_where_it_says() {
    let mut h = Harness::inline(
        r##"local label = ui.state("label", "first")
        local measured = ui.state("measured", 0)
        local seen = 0
        return ui.column { w = 400, h = 300,
            ui.button { id = "read", on_click = function() measured:set(seen) end, "read" },
            ui.button { id = "change", on_click = function() label:set("second") end, "change" },
            ui.button { id = "fewer", on_click = function() label:set("") end, "fewer" },
            ui.text { id = "measured", function() return tostring(measured:get()) end },
            ui.canvas { id = "c", w = 200, h = 100,
                draw = function(c, w, h)
                    c:rect(0, 0, w, h, "#333")
                    local text = label:get()
                    if text ~= "" then
                        c:text(text, 10, 20)
                        c:text("right", w, 50, { align = "end", baseline = "middle", size = 10 })
                    end
                    seen = c:measure_text("right", { size = 10 })
                end } }"##,
    );
    h.ui.prepare_frame();
    assert_eq!(h.host.error().get(), None);
    let find = |h: &mut Harness, text: &str| {
        let canvas = h.find("c").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![canvas];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text: t, .. } = scene.kind(n)
                && &**t == text
            {
                let b = scene.transformed_bounds(n);
                let top = scene.bounds(canvas);
                return Some((b.x - top.x, b.y - top.y, b.width, b.height));
            }
            stack.extend(scene.children(n).iter().copied());
        }
        None
    };
    let first = find(&mut h, "first").expect("the text is drawn");
    assert_eq!((first.0, first.1), (10., 20.));
    let right = find(&mut h, "right").unwrap();
    assert!(
        (right.0 + right.2 - 200.).abs() < 0.5,
        "ends at the edge: {right:?}"
    );
    assert!(
        (right.1 + right.3 / 2. - 50.).abs() < 0.5,
        "middle: {right:?}"
    );
    h.click("read");
    h.ui.prepare_frame();
    let measured: f32 = h.text("measured").parse().unwrap();
    assert!((measured - right.2).abs() < 0.5, "{measured} and {right:?}");
    h.click("change");
    h.ui.prepare_frame();
    assert!(find(&mut h, "first").is_none());
    assert!(find(&mut h, "second").is_some());
    h.click("fewer");
    h.ui.prepare_frame();
    assert!(find(&mut h, "second").is_none() && find(&mut h, "right").is_none());
    assert_eq!(h.host.error().get(), None);
}

/// A canvas without text makes no nodes for it, and text takes a canvas's
/// pan and zoom.
#[test]
fn canvas_text_costs_nothing_unused_and_follows_a_view() {
    let mut h = Harness::inline(
        r##"local view = ui.state("view", { x = 0, y = 0, zoom = 1 })
        return ui.column { w = 400, h = 300,
            ui.button { id = "zoom", on_click = function() view:set({ x = 20, y = 10, zoom = 2 }) end, "zoom" },
            ui.canvas { id = "plain", w = 100, h = 50, draw = function(c) c:rect(0, 0, 10, 10, "#333") end },
            ui.canvas { id = "c", w = 100, h = 50, view = view,
                draw = function(c) c:text("mark", 10, 5) end } }"##,
    );
    h.ui.prepare_frame();
    let plain = h.find("plain").unwrap();
    assert_eq!(
        h.ui.scene.borrow().children(plain).len(),
        1,
        "the drawing alone"
    );
    let canvas = h.find("c").unwrap();
    let mark = |h: &mut Harness| {
        let scene = h.ui.scene.borrow();
        let group = *scene.children(canvas).last().unwrap();
        let text = scene.children(group)[0];
        let (b, top) = (scene.transformed_bounds(text), scene.bounds(canvas));
        (b.x - top.x, b.y - top.y, b.height)
    };
    let before = mark(&mut h);
    assert_eq!((before.0, before.1), (10., 5.));
    // Read by the canvas's `label`, not line by line.
    let text = {
        let scene = h.ui.scene.borrow();
        scene.children(*scene.children(canvas).last().unwrap())[0]
    };
    assert!(h.ui.semantics.borrow().get(text).is_none());
    h.click("zoom");
    h.ui.prepare_frame();
    let zoomed = mark(&mut h);
    assert_eq!((zoomed.0, zoomed.1), (40., 20.));
    assert!(
        (zoomed.2 - before.2 * 2.).abs() < 0.5,
        "{zoomed:?} {before:?}"
    );
}

/// A canvas redrawn keeps its text nodes: a line's node is reused by place,
/// changed or not, and only lines beyond the new count go.
#[test]
fn a_redrawn_canvas_reuses_its_text_nodes() {
    let mut h = Harness::inline(
        r##"local t = ui.state("t", 0)
        return ui.column { w = 400, h = 300,
            ui.button { id = "tick", on_click = function() t:add(1) end, "tick" },
            ui.canvas { id = "c", w = 300, h = 100,
                draw = function(c)
                    local v = t:get()
                    for i = 1, 50 do
                        c:text(`{i}: {if i % 2 == 0 then v else 0}`, i * 5, 10)
                    end
                end } }"##,
    );
    h.ui.prepare_frame();
    let canvas = h.find("c").unwrap();
    let nodes = |h: &Harness| -> Vec<_> {
        let scene = h.ui.scene.borrow();
        scene
            .children(*scene.children(canvas).last().unwrap())
            .to_vec()
    };
    let first = nodes(&h);
    assert_eq!(first.len(), 50);
    for _ in 0..20 {
        h.click("tick");
        h.ui.prepare_frame();
        assert_eq!(nodes(&h), first, "the same nodes");
    }
    assert_eq!(h.host.error().get(), None);
}
