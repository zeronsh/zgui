//! The component library from scripts: segmented controls, steppers,
//! disclosures and accordions, separators, and what menu items, sliders,
//! progress bars, overlays and plain nodes tell assistive technology.
mod common;

use common::Harness;
use zgx::{
    input::{Key, Modifiers},
    scene::NodeId,
    semantics::{Role, SemanticNode},
};

fn node(h: &mut Harness, id: &str) -> SemanticNode {
    let n = h.find(id).unwrap_or_else(|| panic!("no node `{id}`"));
    h.ui.semantics
        .borrow()
        .get(n)
        .cloned()
        .unwrap_or_else(|| panic!("no semantics on `{id}`"))
}
/// The nodes with `role`, in tree order.
fn with_role(h: &mut Harness, role: Role) -> Vec<NodeId> {
    h.ui.prepare_frame();
    let scene = h.ui.scene.borrow();
    let semantics = h.ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(id) = pending.pop() {
        if semantics.get(id).is_some_and(|n| n.role == role) {
            out.push(id);
        }
        pending.extend(scene.children(id).iter().rev().copied());
    }
    out
}
fn click_node(h: &mut Harness, node: NodeId) {
    let b = h.ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    let button = zgx::input::PointerButton::Primary;
    h.ui.dispatch(zgx::input::InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(zgx::input::InputEvent::PointerUp { x, y, button });
    h.settle();
}

#[test]
fn a_segmented_control_chooses_values_and_a_stepper_steps() {
    let mut h = Harness::inline(
        r#"local view = ui.state("view", "week")
        local copies = ui.state("copies", 2)
        return ui.column {
            ui.segmented { label = "Range", value = view, options = { { label = "Day", value = "day" }, { label = "Week", value = "week" }, "Month" } },
            ui.text { id = "view", "{view}" },
            ui.stepper { id = "copies", label = "Copies", value = copies, min = 1, max = 5 },
            ui.text { id = "copies-shown", "{copies}" },
        }"#,
    );
    assert_eq!(h.error(), None);
    let radios = with_role(&mut h, Role::RadioButton);
    assert_eq!(radios.len(), 3);
    let checked: Vec<_> = radios
        .iter()
        .map(|r| h.ui.semantics.borrow().get(*r).unwrap().checked)
        .collect();
    assert_eq!(checked, [Some(false), Some(true), Some(false)]);
    click_node(&mut h, radios[2]);
    assert_eq!(h.text("view"), "Month", "a bare option is its own value");
    click_node(&mut h, radios[0]);
    assert_eq!(h.text("view"), "day");
    let field = with_role(&mut h, Role::SpinButton)[0];
    h.ui.input.focus(&h.ui.scene, Some(field));
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    assert_eq!(h.text("copies-shown"), "4");
    h.key(Key::End);
    assert_eq!(h.text("copies-shown"), "5");
    assert_eq!(h.error(), None);
}

#[test]
fn disclosures_and_accordions_show_their_content() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        local section = ui.state("section", nil :: string?)
        return ui.column {
            ui.disclosure { open = open, summary = "Advanced", ui.text { id = "advanced", "Options" } },
            ui.accordion {
                open = section,
                ui.section { id = "ship", title = "Shipping", ui.text { id = "ship-body", "By sea" } },
                ui.section { id = "pay", title = "Payment", ui.text { id = "pay-body", "By card" } },
            },
            ui.text { id = "section", function() return tostring(section:get()) end },
            ui.separator {},
        }"#,
    );
    assert_eq!(h.error(), None);
    assert!(h.find("advanced").is_none());
    let buttons = with_role(&mut h, Role::Button);
    assert_eq!(buttons.len(), 3);
    click_node(&mut h, buttons[0]);
    assert!(h.find("advanced").is_some());
    assert_eq!(
        h.ui.semantics.borrow().get(buttons[0]).unwrap().expanded,
        Some(true)
    );
    click_node(&mut h, buttons[2]);
    assert_eq!(h.text("section"), "pay");
    assert!(h.find("pay-body").is_some() && h.find("ship-body").is_none());
    click_node(&mut h, buttons[1]);
    assert_eq!(h.text("section"), "ship");
    assert!(h.find("pay-body").is_none());
}

#[test]
fn menu_items_check_and_show_shortcuts_and_nodes_say_their_state() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", true)
        local wrap = ui.state("wrap", true)
        local on = ui.state("on", false)
        local anchor = ui.button { id = "anchor", "View" }
        return ui.column {
            anchor,
            ui.menu {
                open = open,
                anchor = anchor,
                match_anchor_width = true,
                ui.menu_item { id = "wrap", label = "Word Wrap", checked = wrap, shortcut = "⌥Z", keep_open = true, on_click = ui.toggle(wrap) },
                ui.menu_item { id = "small", label = "Small", radio = true, checked = true },
            },
            ui.div { id = "switch", role = "switch", checked = on, focusable = true, on_click = ui.toggle(on), w = 30, h = 16 },
            ui.slider { id = "volume", label = "Volume", value = ui.state("v", 0.5), step = 0.25 },
            ui.progress { id = "busy", label = "Syncing", value = ui.state("p", nil :: number?) },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let wrap = node(&mut h, "wrap");
    assert_eq!(wrap.checked, Some(true));
    assert_eq!(wrap.shortcut.as_deref(), Some("⌥Z"));
    assert!(!wrap.radio);
    assert!(h.texts().contains(&"⌥Z".to_owned()), "the shortcut shows");
    assert!(h.texts().contains(&"✓".to_owned()), "and the check");
    let small = node(&mut h, "small");
    assert!(small.radio && small.checked == Some(true));
    h.click("wrap");
    assert_eq!(
        node(&mut h, "wrap").checked,
        Some(false),
        "it follows its state"
    );
    h.key(Key::Escape);
    let switch = node(&mut h, "switch");
    assert_eq!((switch.role, switch.checked), (Role::Switch, Some(false)));
    h.click("switch");
    assert_eq!(node(&mut h, "switch").checked, Some(true));
    let volume = h.find("volume").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(volume));
    h.key_with(Key::ArrowRight, Modifiers::default());
    assert_eq!(node(&mut h, "volume").numeric_value, Some(0.75), "a step");
    assert_eq!(
        node(&mut h, "busy").numeric_value,
        None,
        "nil: indeterminate"
    );
}

#[test]
fn a_custom_menu_item_says_it_is_checkable_and_follows_its_state() {
    let mut h = Harness::inline(
        r#"
        local wrap = ui.state("wrap", false)
        return ui.column {
            ui.div { id = "wrap", role = "menuitemcheckbox", label = "Word wrap", checked = wrap,
                on_click = ui.toggle(wrap), w = 80, h = 20 },
            ui.div { id = "mono", role = "menuitemradio", label = "Mono", checked = true, w = 80, h = 20 },
            ui.div { id = "plain", role = "menuitem", label = "Plain", w = 80, h = 20 },
        }
        "#,
    );
    let (wrap, mono) = (node(&mut h, "wrap"), node(&mut h, "mono"));
    assert_eq!(
        (wrap.role, wrap.checked, wrap.radio),
        (Role::MenuItem, Some(false), false)
    );
    assert_eq!(
        (mono.role, mono.checked, mono.radio),
        (Role::MenuItem, Some(true), true)
    );
    assert_eq!(
        zgx_script::driver::semantic_role_name(&wrap),
        "menuitemcheckbox"
    );
    assert_eq!(
        zgx_script::driver::semantic_role_name(&mono),
        "menuitemradio"
    );
    let plain = node(&mut h, "plain");
    assert_eq!(zgx_script::driver::semantic_role_name(&plain), "menuitem");
    let id = with_role(&mut h, Role::MenuItem)[0];
    click_node(&mut h, id);
    assert_eq!(node(&mut h, "wrap").checked, Some(true));
}

#[test]
#[should_panic(expected = "role `menuitemradio` says whether it is checked")]
fn a_checkable_menu_role_needs_its_state() {
    Harness::inline(r#"return ui.div { role = "menuitemradio", label = "Mono" }"#);
}

#[test]
fn a_menus_button_opens_it() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", false)
        local anchor = ui.button { id = "anchor", "View" }
        return ui.column {
            ui.menu { open = open, anchor = anchor, ui.menu_item { id = "item", "Item" } },
            ui.text { id = "state", function() return tostring(open:get()) end },
        }"#,
    );
    h.click("anchor");
    h.frames(2);
    assert_eq!(h.text("state"), "true");
    assert!(h.find("item").is_some());
    // From the keyboard: Down (or Enter, or Space) on the button.
    h.key(Key::Escape);
    assert_eq!(h.text("state"), "false");
    let anchor = h.find("anchor").unwrap();
    h.ui.input.focus(&h.ui.scene, Some(anchor));
    h.key(Key::ArrowDown);
    assert_eq!(h.text("state"), "true");
}

#[test]
fn tabs_choose_close_and_keep_the_scripts_tables() {
    let mut h = Harness::inline(
        r#"local items = ui.state("items", {
            { id = "a", label = "Alpha", closable = true, path = "/a" },
            { id = "b", label = "Beta", closable = true, path = "/b" },
            { id = "c", label = "Gamma" },
        })
        local value = ui.state("value", "a")
        return ui.column {
            ui.tabs {
                label = "Files",
                items = items,
                value = value,
                reorderable = true,
                on_close = function(id) return id ~= "c" end,
                panel = function(id) return ui.text { id = "panel", `Panel {id}` } end,
            },
            ui.text { id = "order", function()
                local ids = {}
                for _, t in items:get() do table.insert(ids, `{t.id}{t.path or ""}`) end
                return table.concat(ids, ",")
            end },
            ui.text { id = "value", "{value}" },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    assert_eq!(h.text("panel"), "Panel a");
    let tabs = with_role(&mut h, Role::Tab);
    assert_eq!(tabs.len(), 3);
    click_node(&mut h, tabs[2]);
    assert_eq!(h.text("value"), "c");
    assert_eq!(h.text("panel"), "Panel c");
    // Closed from the keyboard: the script's own tables stay as they were.
    h.ui.input.focus(&h.ui.scene, Some(tabs[0]));
    h.key(Key::Delete);
    assert_eq!(h.text("order"), "b/b,c");
    assert_eq!(
        h.text("value"),
        "b",
        "focused, it was chosen; closed, its neighbour is"
    );
    let moved = Modifiers {
        meta: true,
        shift: true,
        ..Default::default()
    };
    let tabs = with_role(&mut h, Role::Tab);
    h.ui.input.focus(&h.ui.scene, Some(tabs[0]));
    h.key_with(Key::ArrowRight, moved);
    assert_eq!(h.text("order"), "c,b/b");
}

#[test]
fn a_select_chooses_values_from_its_menu_and_by_typing() {
    let mut h = Harness::inline(
        r#"local size = ui.state("size", 2)
        return ui.column {
            ui.select {
                id = "size",
                label = "Size",
                value = size,
                options = { { label = "Small", value = 1 }, { label = "Medium", value = 2 }, { label = "Large", value = 3, disabled = true } },
            },
            ui.text { id = "chosen", "{size}" },
        }"#,
    );
    assert_eq!(h.error(), None);
    let trigger = with_role(&mut h, Role::ComboBox)[0];
    assert_eq!(
        h.ui.semantics
            .borrow()
            .get(trigger)
            .unwrap()
            .value
            .as_deref(),
        Some("Medium")
    );
    h.ui.input.focus(&h.ui.scene, Some(trigger));
    h.key(Key::Character("s".into()));
    assert_eq!(h.text("chosen"), "1", "the value itself, a number");
    h.key(Key::Character("l".into()));
    assert_eq!(h.text("chosen"), "1", "Large is disabled");
    click_node(&mut h, trigger);
    h.frames(2);
    let medium = with_role(&mut h, Role::MenuItem)
        .into_iter()
        .find(|n| h.ui.semantics.borrow().get(*n).unwrap().label == "Medium")
        .unwrap();
    h.ui.input.focus(&h.ui.scene, Some(medium));
    h.key(Key::Enter);
    h.frames(2);
    assert_eq!(h.text("chosen"), "2");
}

#[test]
fn what_a_control_with_a_popup_is_given_lands_on_the_control() {
    let mut h = Harness::inline(
        r#"local v = ui.state("v", "a")
        local q = ui.state("q", "")
        local d = ui.state("d", nil :: string?)
        local options = { "a", "b" }
        local items = ui.state("items", options)
        return ui.column { w = 500,
            ui.select { id = "fills", label = "A", value = v, options = options },
            ui.select { id = "sized", label = "B", value = v, options = options, w = 300 },
            ui.combobox { id = "field", label = "C", text = q, items = items, w = 240 },
            ui.date_picker { id = "date", label = "D", value = d, w = 200 },
            ui.row { h = 60, items = "center",
                ui.select { id = "fits", label = "E", value = v, options = options },
                ui.date_picker { id = "fits-date", label = "F", value = d },
            },
            ui.text { id = "typed", "{q}" },
        }"#,
    );
    assert_eq!(h.error(), None);
    assert_eq!(
        h.bounds("fills").width,
        500.,
        "fills a column, as a field does"
    );
    assert_eq!(h.bounds("sized").width, 300.);
    assert_eq!(h.bounds("field").width, 240.);
    assert_eq!(h.bounds("date").width, 200.);
    assert_eq!(h.bounds("fits").height, 39., "centred, not stretched");
    assert!(h.bounds("fits").y > h.bounds("date").y + h.bounds("date").height);
    assert!(
        h.bounds("fits").width < 200.,
        "a row leaves it its own width"
    );
    assert!(h.bounds("fits-date").x >= h.bounds("fits").width);

    // The trigger is all of that box: nothing beside it to click, its menu
    // as wide as it.
    for id in ["fills", "sized", "field", "date"] {
        let held = h.bounds(id);
        let filled = with_role(&mut h, Role::ComboBox)
            .into_iter()
            .chain(with_role(&mut h, Role::Button))
            .any(|n| h.ui.scene.borrow().bounds(n) == held);
        assert!(filled, "{id}");
    }
    h.click("sized");
    h.frames(2);
    assert!(
        with_role(&mut h, Role::MenuItem)
            .iter()
            .any(|n| h.ui.scene.borrow().bounds(*n).width >= 280.),
        "open, under the whole trigger"
    );
    h.key(Key::Escape);
    h.frames(2);
    h.type_text("field", "x");
    assert_eq!(h.text("typed"), "x");
}

#[test]
fn a_tabs_panel_fills_the_tabs() {
    let mut h = Harness::inline(
        r#"local items = ui.state("items", { { id = "a", label = "A" } })
        local chosen = ui.state("chosen", "a")
        return ui.column { w = 500, h = 400,
            ui.tabs { label = "Files", items = items, value = chosen, grow = 1,
                panel = function(id)
                    return ui.column { id = "panel", grow = 1,
                        ui.textarea { id = "editor", label = "Editor", value = ui.state("t", ""), grow = 1, w_full = true },
                    }
                end,
            },
        }"#,
    );
    assert_eq!(h.error(), None);
    let (panel, editor) = (h.bounds("panel"), h.bounds("editor"));
    assert_eq!(panel.width, 500.);
    assert!(panel.height > 200., "{panel:?}");
    assert_eq!((editor.width, editor.height), (panel.width, panel.height));
}

const KEPT_TABS: &str = r##"
local items = ui.state("items", {
    { id = "a", label = "A", closable = true },
    { id = "b", label = "B", closable = true },
})
local chosen = ui.state("chosen", "a")
local builds = ui.state("builds", 0)
local notes = { a = ui.state("note_a", "alpha"), b = ui.state("note_b", "bravo") }
ui.command { id = "a", title = "a", run = function() chosen:set("a") end }
ui.command { id = "b", title = "b", run = function() chosen:set("b") end }
ui.command { id = "close", title = "c", run = function()
    chosen:set("b")
    items:set({ { id = "b", label = "B", closable = true } })
end }
return ui.column { h = 280,
    ui.tabs { label = "t", grow = 1, KEEP items = items, value = chosen,
        panel = function(id: string): Node
            builds:add(1)
            return ui.column { id = `panel-{id}`, grow = 1,
                ui.textarea { id = `note-{id}`, label = id, grow = 1, w_full = true, value = notes[id] },
            }
        end,
    },
    ui.text { id = "builds", "{builds}" },
    ui.text { id = "note", "{note_a}" },
}"##;

#[test]
fn kept_tabs_keep_their_panels_at_rest_until_they_close() {
    let mut h = Harness::inline(&KEPT_TABS.replace("KEEP", "keep = true,"));
    assert_eq!(h.error(), None);
    h.frame();
    h.type_text("note-a", "!");
    assert_eq!(h.text("note"), "alpha!");
    let caret = h.ui.focused_editor().expect("focused").selection();

    assert!(h.host.run_command("b"));
    h.frame();
    assert_eq!(h.text("builds"), "2");
    assert!(
        h.ui.focused_editor().is_none(),
        "focus leaves a hidden panel, so keys can't reach it"
    );
    let panel = h.find("panel-a").expect("kept, mounted");
    assert!(
        !h.ui.scene.borrow().layout_visible(panel),
        "and takes no layout"
    );

    assert!(h.host.run_command("a"));
    h.frame();
    assert_eq!(h.text("builds"), "2", "chosen again, it isn't rebuilt");
    let note = h.find("note-a").expect("its editor");
    assert_eq!(
        h.ui.editor(note).unwrap().selection(),
        caret,
        "its caret too"
    );
    h.click("note-a");
    h.keys("cmd+z");
    h.frame();
    assert_eq!(h.text("note"), "alpha", "its undo survived");

    assert!(h.host.run_command("close"));
    h.frame();
    assert!(
        h.find("panel-a").is_none(),
        "closing its tab drops the panel"
    );
}

#[test]
fn tabs_rebuild_their_panels_unless_they_keep_them() {
    let mut h = Harness::inline(&KEPT_TABS.replace("KEEP", ""));
    assert_eq!(h.error(), None);
    h.frame();
    assert!(h.host.run_command("b"));
    h.frame();
    assert!(h.host.run_command("a"));
    h.frame();
    assert_eq!(h.text("builds"), "3");
}

#[test]
fn a_list_row_taller_than_its_height_is_a_layout_warning() {
    let warned = |height: u32| {
        let h = Harness::inline(&format!(
            r#"return ui.list {{ id = "list", w = 200, h = 200, count = 20, height = {height},
                row = function(i) return ui.button {{ h = 40, "Row" }} end }}"#
        ));
        assert_eq!(h.error(), None);
        h.ui.prepare_frame();
        zgx::inspect::layout_warnings(&h.ui)
            .iter()
            .filter(|w| w.message.contains("runs over the next row"))
            .count()
    };
    assert!(warned(28) > 0);
    assert_eq!(warned(40), 0);
}

#[test]
fn a_comboboxs_items_may_be_a_plain_list() {
    let mut h = Harness::inline(
        r#"local text = ui.state("text", "")
        return ui.combobox { id = "city", label = "City", text = text, items = { "Paris", "Porto" } }"#,
    );
    assert_eq!(h.error(), None);
    h.type_text("city", "P");
    assert_eq!(with_role(&mut h, Role::ListBoxOption).len(), 2);
}

#[test]
fn a_textarea_scrolls_to_a_line_and_says_which_is_on_top() {
    let mut h = Harness::inline(
        r#"local lines = {}
        for i = 1, 200 do lines[i] = `line {i}` end
        local text = ui.state("text", table.concat(lines, "\n"))
        local top = ui.state("top", 0)
        local box = ui.ref()
        local function go(line: number, align: string?)
            local el = box:peek()
            if el then el:scroll_to_line(line, { align = align } :: any) end
        end
        ui.command { id = "start", title = "s", run = function() go(100, "start") end }
        ui.command { id = "center", title = "c", run = function() go(100, "center") end }
        ui.command { id = "end", title = "e", run = function() go(100, "end") end }
        ui.command { id = "near", title = "n", run = function() go(1) end }
        ui.command { id = "past", title = "p", run = function() go(9999, "start") end }
        return ui.column {
            ui.textarea { id = "note", ref = box, label = "Note", h = 230, w = 300, value = text,
                on_top_line = function(line) top:set(line) end },
            ui.text { id = "top", "{top}" },
        }"#,
    );
    assert_eq!(h.error(), None);
    let top = |h: &mut Harness, command: &str| {
        assert!(h.host.run_command(command));
        h.frames(3);
        h.text("top").parse::<usize>().expect("a line")
    };
    h.frames(2);
    assert_eq!(h.text("top"), "1");
    let note = h.find("note").expect("the editor");
    let caret = h.ui.editor(note).unwrap().selection();
    let start = top(&mut h, "start");
    assert_eq!(start, 100, "at the top");
    let center = top(&mut h, "center");
    assert!(center < 100, "above it now: {center}");
    let end = top(&mut h, "end");
    assert!(end < center, "further up: {end}");
    assert_eq!(top(&mut h, "near"), 1);
    assert!(top(&mut h, "past") > 190, "past the end, the end");
    assert_eq!(
        h.ui.editor(note).unwrap().selection(),
        caret,
        "the caret stays"
    );
}

#[test]
fn a_combobox_suggests_from_a_computed_and_chooses() {
    let mut h = Harness::inline(
        r#"local query = ui.state("query", "")
        local city = ui.state("city", nil :: string?)
        local cities = { "Paris", "Prague", "Porto", "Lisbon" }
        local matches = ui.computed(function()
            local q = string.lower(query:get())
            local out = {}
            for _, c in cities do
                if string.sub(string.lower(c), 1, #q) == q then
                    table.insert(out, { label = c, value = string.lower(c), detail = "Europe" })
                end
            end
            return out
        end)
        return ui.column {
            ui.combobox { id = "city", label = "City", text = query, items = matches, value = city },
            ui.text { id = "chosen", function() return tostring(city:get()) end },
        }"#,
    );
    assert_eq!(h.error(), None);
    let field = with_role(&mut h, Role::ComboBox)[0];
    h.ui.input.focus(&h.ui.scene, Some(field));
    h.ui.dispatch(zgx::input::InputEvent::Text("pr".into()));
    h.settle();
    h.frames(2);
    let options = with_role(&mut h, Role::ListBoxOption);
    assert_eq!(options.len(), 1, "Prague alone");
    h.key(Key::Enter);
    assert_eq!(h.text("chosen"), "prague");
}

#[test]
fn the_command_palette_lists_and_runs_the_apps_commands() {
    let mut h = Harness::inline(
        r#"local open = ui.state("open", true)
        local ran = ui.state("ran", "")
        ui.command { id = "save", title = "Save", menu = "File", key = "cmd+s", run = function() ran:set("save") end }
        ui.command { id = "theme", title = "Toggle Theme", menu = "View", run = function() ran:set("theme") end }
        return ui.column {
            ui.command_palette { open = open },
            ui.text { id = "ran", "{ran}" },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    assert_eq!(with_role(&mut h, Role::ListBoxOption).len(), 2);
    let shortcut = if cfg!(target_os = "macos") {
        "⌘S"
    } else {
        "Ctrl+S"
    };
    assert!(h.texts().contains(&shortcut.to_owned()), "{:?}", h.texts());
    h.ui.dispatch(zgx::input::InputEvent::Text("tt".into()));
    h.settle();
    h.frames(1);
    h.key(Key::Enter);
    assert_eq!(h.text("ran"), "theme");
}

#[test]
fn ui_toast_shows_in_the_hosts_corner_and_runs_its_action() {
    let mut h = Harness::inline(
        r#"local undone = ui.state("undone", false)
        local last = 0
        ui.command { id = "toast", run = function()
            last = ui.toast { title = "Deleted 3 files", kind = "success", action = { label = "Undo", run = function() undone:set(true) end } }
        end }
        ui.command { id = "dismiss", run = function() ui.dismiss_toast(last) end }
        return ui.column { ui.text { id = "undone", function() return tostring(undone:get()) end } }"#,
    );
    assert!(h.host.run_command("toast"));
    h.frames(2);
    assert!(h.texts().contains(&"Deleted 3 files".to_owned()));
    h.click_role_label(Role::Button, "Undo");
    assert_eq!(h.text("undone"), "true");
    assert!(
        !h.texts().contains(&"Deleted 3 files".to_owned()),
        "gone after its action"
    );
    assert!(h.host.run_command("toast"));
    h.frames(2);
    assert!(h.host.run_command("dismiss"));
    h.frames(2);
    assert!(!h.texts().contains(&"Deleted 3 files".to_owned()));
}

trait ClickRole {
    fn click_role_label(&mut self, role: Role, label: &str);
}
impl ClickRole for Harness {
    fn click_role_label(&mut self, role: Role, label: &str) {
        let node = with_role(self, role)
            .into_iter()
            .find(|n| self.ui.semantics.borrow().get(*n).unwrap().label == label)
            .unwrap_or_else(|| panic!("no {role:?} {label}"));
        click_node(self, node);
        self.frames(1);
    }
}

#[test]
fn a_tree_opens_loads_lazily_and_activates() {
    let mut h = Harness::inline(
        r#"local items = ui.state("items", {
            { id = "src", label = "src", children = { { id = "main", label = "main.luau" } } },
            { id = "docs", label = "docs", lazy = true },
        })
        local open = ui.state("open", {} :: { string })
        local current = ui.state("current", nil :: string?)
        local opened = ui.state("opened", "")
        return ui.column {
            h = 300,
            ui.tree {
                label = "Files",
                items = items,
                open = open,
                value = current,
                on_open = function(id)
                    if id == "docs" then
                        items:set_key({ 2, "children" }, { { id = "guide", label = "guide.md" } })
                    end
                end,
                on_activate = function(id) opened:set(id) end,
            },
            ui.text { id = "opened", "{opened}" },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let tree = with_role(&mut h, Role::Tree)[0];
    h.ui.input.focus(&h.ui.scene, Some(tree));
    h.frames(1);
    h.key(Key::ArrowRight);
    h.frames(1);
    assert_eq!(with_role(&mut h, Role::TreeItem).len(), 3, "src opened");
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.text("opened"), "main");
    h.key(Key::End);
    h.key(Key::ArrowRight);
    h.frames(2);
    let labels: Vec<String> = with_role(&mut h, Role::TreeItem)
        .iter()
        .map(|n| h.ui.semantics.borrow().get(*n).unwrap().label.clone())
        .collect();
    assert!(
        labels.contains(&"guide.md".to_owned()),
        "loaded as it opened: {labels:?}"
    );
}

#[test]
fn a_data_grid_sorts_edits_into_its_rows_and_writes_its_columns() {
    let mut h = Harness::inline(
        r#"local people = ui.state("people", {
            { name = "Ada", age = 36 },
            { name = "Grace", age = 85 },
            { name = "Linus", age = 21 },
        })
        local columns = ui.state("columns", {
            { id = "name", title = "Name", width = 140, sortable = true, editable = true },
            { id = "age", title = "Age", width = 80, sortable = true, editable = true, align = "end" },
        })
        local sort = ui.state("sort", nil :: { column: string, descending: boolean? }?)
        local current = ui.state("current", nil :: { row: number, column: string }?)
        local opened = ui.state("opened", 0)
        return ui.column {
            h = 300,
            ui.data_grid {
                label = "People",
                columns = columns,
                rows = people,
                sort = sort,
                value = current,
                on_activate = function(row) opened:set(row) end,
            },
            ui.text { id = "first", function() return people:get()[1].name end },
            ui.text { id = "age", function() return tostring(people:get()[3].age) end },
            ui.text { id = "width", function() return tostring(columns:get()[2].width) end },
            ui.text { id = "sort", function()
                local s = sort:get()
                return if s then s.column .. (if s.descending then " down" else " up") else "none"
            end },
            ui.text { id = "current", function()
                local c = current:get()
                return if c then c.column .. " " .. c.row else "none"
            end },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let grid = with_role(&mut h, Role::Grid)[0];
    h.ui.input.focus(&h.ui.scene, Some(grid));
    h.frames(1);
    assert_eq!(h.text("current"), "name 1");
    // The header: Age ascending, so Linus (21) shows first.
    h.key(Key::ArrowUp);
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.text("sort"), "age up");
    let first_row = |h: &mut Harness| {
        let rows = with_role(h, Role::Row);
        let cells: Vec<NodeId> = {
            let scene = h.ui.scene.borrow();
            let semantics = h.ui.semantics.borrow();
            let mut out = Vec::new();
            let mut pending = vec![rows[1]];
            while let Some(n) = pending.pop() {
                if semantics.get(n).is_some_and(|s| s.role == Role::Cell) {
                    out.push(n);
                }
                pending.extend(scene.children(n).iter().rev().copied());
            }
            out
        };
        let scene = h.ui.scene.borrow();
        cells
            .iter()
            .map(|c| {
                let mut stack = vec![*c];
                let mut text = String::new();
                while let Some(n) = stack.pop() {
                    if let zgx::scene::NodeKind::Text { text: t, .. } = scene.kind(n) {
                        text.push_str(t);
                    }
                    stack.extend(scene.children(n).iter().rev());
                }
                text
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(first_row(&mut h), ["Linus", "21"]);
    // The current cell followed its record, Ada's (now shown second): Down
    // into the rows lands there; Up, on Linus's age, row 3 of `rows`.
    h.key(Key::ArrowDown);
    assert_eq!(h.text("current"), "age 1");
    h.key(Key::ArrowUp);
    assert_eq!(h.text("current"), "age 3");
    // Typing over it edits; the number written is a number.
    h.key(Key::Character("4".into()));
    h.ui.dispatch(zgx::input::InputEvent::Text("4".into()));
    h.ui.dispatch(zgx::input::InputEvent::Text("2".into()));
    h.key(Key::Enter);
    assert_eq!(h.text("age"), "42");
    assert_eq!(
        first_row(&mut h),
        ["Ada", "36"],
        "sorted again: 42 is now last"
    );
    // Alt+Right on the header widens Age, written into the columns.
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    h.key_with(
        Key::ArrowRight,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    assert_eq!(h.text("width"), "96");
    assert_eq!(h.error(), None);
}

/// A date and a time follow a function as a select does: a change goes to
/// `on_change`, and what shows stays the function's until the app changes it.
#[test]
fn a_date_picker_and_a_time_field_are_controlled_by_a_function() {
    let mut h = Harness::inline(
        r#"local due = ui.state("due", "2026-03-10")
        local asked = ui.state("asked", "none")
        return ui.column {
            ui.date_picker {
                label = "Due",
                value = function() return due:get() end,
                on_change = function(date: string?) asked:set(date or "cleared") end,
            },
            ui.time_field {
                label = "Start",
                value = function() return "09:30" end,
                on_change = function(time: string?) asked:set(time or "cleared") end,
            },
            ui.text { id = "asked", function() return asked:get() end },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let trigger = with_role(&mut h, Role::Button)[0];
    assert_eq!(
        h.ui.semantics.borrow().get(trigger).unwrap().label,
        "Due: 2026-03-10"
    );
    h.ui.input.focus(&h.ui.scene, Some(trigger));
    h.key(Key::ArrowDown);
    h.frames(1);
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    h.frames(1);
    assert_eq!(h.text("asked"), "2026-03-11");
    assert_eq!(
        h.ui.semantics.borrow().get(trigger).unwrap().label,
        "Due: 2026-03-10",
        "the function still says so"
    );
    let segments = with_role(&mut h, Role::SpinButton);
    h.ui.input.focus(&h.ui.scene, Some(segments[1]));
    h.key(Key::ArrowUp);
    h.frames(1);
    assert_eq!(h.text("asked"), "09:31");
    assert_eq!(h.error(), None);
}

#[test]
fn a_date_picker_and_a_time_field_write_iso_dates_and_times() {
    let mut h = Harness::inline(
        r#"local due = ui.state("due", "2026-03-10" :: string?)
        local start = ui.state("start", nil :: string?)
        return ui.column {
            h = 420,
            ui.date_picker {
                id = "due",
                label = "Due",
                value = due,
                min = "2026-01-01",
                unavailable = function(date: string) return date == "2026-03-11" end,
                format = function(date: string) return "on " .. date end,
            },
            ui.time_field { id = "start", label = "Start", value = start, minute_step = 5 },
            ui.text { id = "due-text", function() return due:get() or "none" end },
            ui.text { id = "start-text", function() return start:get() or "none" end },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let trigger = with_role(&mut h, Role::Button)[0];
    assert_eq!(
        h.ui.semantics.borrow().get(trigger).unwrap().label,
        "Due: on 2026-03-10"
    );
    h.ui.input.focus(&h.ui.scene, Some(trigger));
    h.key(Key::ArrowDown);
    h.frames(1);
    // The 11th can't be chosen; the 12th can.
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.text("due-text"), "2026-03-10", "unavailable");
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    h.frames(1);
    assert_eq!(h.text("due-text"), "2026-03-12");
    // The time: typed hours, stepped minutes.
    let segments = with_role(&mut h, Role::SpinButton);
    h.ui.input.focus(&h.ui.scene, Some(segments[0]));
    h.key(Key::Character("9".into()));
    assert_eq!(h.text("start-text"), "none", "no minutes yet");
    h.key(Key::ArrowUp);
    assert_eq!(h.text("start-text"), "09:00");
    h.key(Key::ArrowUp);
    assert_eq!(h.text("start-text"), "09:05");
    assert_eq!(h.error(), None);
}

#[test]
fn a_data_grids_cells_are_formatted_and_its_selection_follows_its_records() {
    let mut h = Harness::inline(
        r#"local items = ui.state("items", {
            { name = "Tea", price = 3 },
            { name = "Cake", price = 4.5 },
            { name = "Bread", price = 2.25 },
        })
        local sort = ui.state("sort", nil :: { column: string, descending: boolean? }?)
        local chosen = ui.state("chosen", {} :: { number })
        return ui.column {
            h = 300,
            ui.data_grid {
                label = "Items",
                columns = {
                    { id = "name", title = "Name", sortable = true },
                    { id = "price", title = "Price", sortable = true, align = "end" },
                },
                rows = items,
                sort = sort,
                selection = chosen,
                cell = function(value: any, column: string): string
                    return if column == "price" then string.format("$%.2f", value) else tostring(value)
                end,
            },
            ui.text { id = "chosen", function()
                local names = {}
                for _, i in chosen:get() do
                    table.insert(names, items:get()[i].name)
                end
                table.sort(names)
                return table.concat(names, ",")
            end },
        }"#,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let texts: Vec<String> = with_role(&mut h, Role::Cell)
        .iter()
        .map(|c| {
            let scene = h.ui.scene.borrow();
            let mut stack = vec![*c];
            let mut text = String::new();
            while let Some(n) = stack.pop() {
                if let zgx::scene::NodeKind::Text { text: t, .. } = scene.kind(n) {
                    text.push_str(t);
                }
                stack.extend(scene.children(n).iter().rev());
            }
            text
        })
        .collect();
    assert_eq!(texts[..2], ["Tea", "$3.00"], "formatted: {texts:?}");
    let grid = with_role(&mut h, Role::Grid)[0];
    h.ui.input.focus(&h.ui.scene, Some(grid));
    h.frames(1);
    // Tea and Cake selected (Shift and Down from Tea).
    h.key_with(
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(h.text("chosen"), "Cake,Tea");
    // Sorted by price: the same records stay selected, wherever they show.
    let price = with_role(&mut h, Role::ColumnHeader)[1];
    click_node(&mut h, price);
    h.frames(1);
    assert_eq!(h.text("chosen"), "Cake,Tea");
    assert_eq!(h.error(), None);
}

#[test]
fn events_say_the_platforms_command_key_and_shortcuts_show_as_it_does() {
    use zgx::input::InputEvent;
    let mut h = Harness::inline(
        r##"local seen = ui.state("seen", "")
        local open = ui.state("open", false)
        local anchor = ui.button { id = "anchor", on_click = ui.toggle(open), "Edit" }
        return ui.column {
            on_key = function(e)
                seen:set(seen:peek() .. (if e.cmd then "C" else "c"))
                return true
            end,
            anchor,
            ui.menu { open = open, anchor = anchor,
                ui.menu_item { id = "redo", label = "Redo", shortcut = "cmd+shift+z" },
                ui.menu_item { id = "as_is", label = "As is", shortcut = "⌥Z" } },
            ui.text { "{seen}", id = "seen" },
            ui.text { ui.shortcut_text("cmd+alt+arrowleft"), id = "keys" },
            ui.text { ui.shortcut_text("cmd") .. " + wheel", id = "zoom" } }"##,
    );
    h.frames(2);
    assert_eq!(h.error(), None);
    let mac = cfg!(target_os = "macos");
    // Command on macOS, Control elsewhere; then the other one, which is not.
    let (command, other) = (
        Modifiers {
            meta: mac,
            control: !mac,
            ..Default::default()
        },
        Modifiers {
            meta: !mac,
            control: mac,
            ..Default::default()
        },
    );
    for held in [Modifiers::default(), command, other] {
        h.ui.dispatch_with_modifiers(
            InputEvent::KeyDown {
                key: Key::Character("z".into()),
                modifiers: held,
                repeat: false,
            },
            held,
        );
    }
    h.frames(1);
    assert_eq!(h.text("seen"), "cCc");
    h.click("anchor");
    h.frames(1);
    let redo = node(&mut h, "redo");
    let as_is = node(&mut h, "as_is");
    if mac {
        assert_eq!(redo.shortcut.as_deref(), Some("⇧⌘Z"));
        assert_eq!(h.text("keys"), "⌥⌘←");
        assert_eq!(h.text("zoom"), "⌘ + wheel");
    } else {
        assert_eq!(redo.shortcut.as_deref(), Some("Ctrl+Shift+Z"));
        assert_eq!(h.text("keys"), "Ctrl+Alt+Left");
        assert_eq!(h.text("zoom"), "Ctrl + wheel");
    }
    assert_eq!(as_is.shortcut.as_deref(), Some("⌥Z"), "text shows as it is");
}

/// A canvas is read as an image named by its `label` (a chart by what it
/// shows), its `description` after the name, the label followed as state;
/// one without a label is decoration, out of the accessibility tree.
#[test]
fn a_labelled_canvas_is_an_image_and_an_unlabelled_one_is_decoration() {
    let mut h = Harness::inline(
        r##"local change = ui.state("change", 2.5)
        local function line(c, w, h)
            c:stroke(ui.path():move(0, h):line(w / 2, 0):line(w, h / 2), "#3fb950", 1.5)
        end
        return ui.column {
            ui.canvas { id = "chart", w = 80, h = 20, draw = line,
                label = function() return string.format("Price trend, %+.1f%%", change:get()) end,
                description = "60 ticks, high 143.9, low 128.2" },
            ui.canvas { id = "plain", w = 80, h = 20, draw = line },
            ui.button { id = "fall", on_click = function() change:set(-1.25) end, "Fall" },
        }"##,
    );
    assert_eq!(h.error(), None);
    h.ui.prepare_frame();
    let chart = node(&mut h, "chart");
    assert_eq!(chart.role, Role::Image);
    assert_eq!(chart.label, "Price trend, +2.5%");
    assert_eq!(
        chart.description.as_deref(),
        Some("60 ticks, high 143.9, low 128.2")
    );
    h.click("fall");
    assert_eq!(node(&mut h, "chart").label, "Price trend, -1.2%");
    let plain = h.find("plain").unwrap();
    assert!(h.ui.semantics.borrow().get(plain).is_none(), "decoration");
}
