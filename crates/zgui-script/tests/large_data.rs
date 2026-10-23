//! Data at an app's sizes: `ui.sort`, `ui.number`, and state holding long
//! lists of rows (written in place in a batch, converted through Luau's C
//! API) behaving as the general paths do.
mod common;

use common::Harness;

fn run(h: &mut Harness, id: &str) {
    assert!(h.host.run_command(id), "no command `{id}`");
    h.settle();
}

#[test]
fn ui_sort_is_stable_puts_nil_last_and_sorts_in_place() {
    let mut h = Harness::inline(
        r#"local rows = {
            { n = "a", v = 2 }, { n = "b", v = 1 }, { n = "c" }, { n = "d", v = 2 }, { n = "e", v = 1 },
        }
        local function names(list)
            local out = {}
            for _, r in list do table.insert(out, r.n) end
            return table.concat(out)
        end
        local up = names(ui.sort(table.clone(rows), "v"))
        local down = names(ui.sort(table.clone(rows), "v", true))
        local by_fn = names(ui.sort(table.clone(rows), function(r) return r.n end, true))
        local words = ui.sort({ "pear", "fig", "apple" }, function(s) return s end)
        local same = table.clone(rows)
        local returned = ui.sort(same, "n")
        local readonly = ui.state("rows", rows)
        local ok, err = pcall(ui.sort, readonly:get(), "v")
        local ok_hole, err_hole = pcall(ui.sort, { 3, nil, 1 } :: any, function(x) return x end)
        local ok_key, err_key = pcall(ui.sort, { {}, {} }, function(r) return r end)
        return ui.column {
            ui.text { id = "up", up },
            ui.text { id = "down", down },
            ui.text { id = "fn", by_fn },
            ui.text { id = "words", table.concat(words, " ") },
            ui.text { id = "same", tostring(returned == same) },
            ui.text { id = "readonly", if ok then "sorted" else tostring(err) },
            ui.text { id = "hole", if ok_hole then "sorted" else tostring(err_hole) },
            ui.text { id = "key", if ok_key then "sorted" else tostring(err_key) },
        }"#,
    );
    assert_eq!(h.error(), None);
    assert_eq!(
        h.text("up"),
        "beadc",
        "equal keys keep their order; nil last"
    );
    assert_eq!(
        h.text("down"),
        "adbec",
        "descending, still stable, nil last"
    );
    assert_eq!(h.text("fn"), "edcba");
    assert_eq!(h.text("words"), "apple fig pear");
    assert_eq!(h.text("same"), "true", "it returns the list it sorted");
    assert!(
        h.text("readonly").contains("read-only"),
        "{}",
        h.text("readonly")
    );
    let _ = run;
    // `{ 3, nil, 1 }` has a border at 1 or 3; either sorts or names the hole.
    let hole = h.text("hole");
    assert!(hole == "sorted" || hole.contains("hole"), "{hole}");
    assert!(
        h.text("key").contains("number, string or boolean"),
        "{}",
        h.text("key")
    );
}

#[test]
fn ui_number_formats_as_intl_does() {
    let h = Harness::inline(
        r#"return ui.column {
            ui.text { id = "plain", ui.number(1234567.891) },
            ui.text { id = "whole", ui.number(1234) },
            ui.text { id = "percent", ui.number(-0.0123, { style = "percent", decimals = 1 }) },
            ui.text { id = "compact", ui.number(4321000, { notation = "compact" }) },
            ui.text { id = "money", ui.number(-9.5, { currency = "$", decimals = 2 }) },
            ui.text { id = "sign", ui.number(3, { sign = "always" }) },
        }"#,
    );
    let mut h = h;
    assert_eq!(h.error(), None);
    assert_eq!(h.text("plain"), "1,234,567.89");
    assert_eq!(h.text("whole"), "1,234");
    assert_eq!(h.text("percent"), "-1.2%");
    assert_eq!(h.text("compact"), "4.3M");
    assert_eq!(h.text("money"), "-$9.50");
    assert_eq!(h.text("sign"), "+3");
}

#[test]
fn rows_written_in_a_batch_change_in_place_and_earlier_reads_keep_theirs() {
    let mut h = Harness::inline(
        r#"local made = {}
        for i = 1, 2000 do made[i] = { id = i, price = i, tags = { "a", "b" } } end
        local rows = ui.state("rows", made)
        local before = rows:get()
        local kept = ""
        ui.command { id = "write", run = function()
            ui.batch(function()
                for i = 1, 50 do rows:set_key({ i * 3, "price" }, -i) end
                rows:set_key({ 7, "tags", 2 }, "z")
                -- Read mid-batch, then written again: the read keeps its own.
                local mid = rows:peek()
                rows:set_key({ 1, "price" }, 1000)
                kept = `{mid[1].price} {before[3].price} {before[7].tags[2]}`
            end)
        end }
        return ui.column {
            ui.text { id = "now", function()
                local r = rows:get()
                return `{r[1].price} {r[3].price} {r[150].price} {r[151].price} {r[7].tags[2]} {#r}`
            end },
            ui.text { id = "kept", function() rows:get() return kept end },
        }"#,
    );
    assert_eq!(h.text("now"), "1 3 150 151 b 2000");
    run(&mut h, "write");
    assert_eq!(h.error(), None);
    assert_eq!(h.text("now"), "1000 -1 -50 151 z 2000");
    assert_eq!(
        h.text("kept"),
        "1 3 b",
        "tables read before the writes don't change"
    );
}

#[test]
fn state_conversion_keeps_its_rules_for_every_shape() {
    let mut h = Harness::inline(
        r#"local inner = ui.state("inner", 5)
        local function try(v)
            local ok, err = pcall(function() ui.state("probe", 0):set(v) end)
            return if ok then "ok" else tostring(err)
        end
        local deep = { { { { { { { { 1 } } } } } } } }
        local s = ui.state("deep", deep)
        local mixed = try({ 1, x = 2 })
        local bad_key = try({ [true] = 1 })
        local func = try({ f = function() end })
        local with_state = ui.state("with_state", { n = inner })
        local nulls = ui.state("nulls", { a = ui.null, b = 1 })
        local object = ui.state("object", ui.object())
        local strings = ui.state("strings", { { k = "x" }, { k = "x" } })
        return ui.column {
            ui.text { id = "deep", tostring(s:get()[1][1][1][1][1][1][1][1]) },
            ui.text { id = "mixed", mixed },
            ui.text { id = "bad_key", bad_key },
            ui.text { id = "func", func },
            ui.text { id = "with_state", tostring(with_state:get().n) },
            ui.text { id = "nulls", tostring(nulls:get().a) .. " " .. tostring(nulls:get().b) },
            ui.text { id = "strings", strings:get()[2].k },
        }"#,
    );
    assert_eq!(h.error(), None);
    assert_eq!(
        h.text("deep"),
        "1",
        "nesting deeper than the fast path reads converts"
    );
    assert!(h.text("mixed").contains("not both"), "{}", h.text("mixed"));
    assert!(
        h.text("bad_key").contains("boolean keys"),
        "{}",
        h.text("bad_key")
    );
    assert!(
        h.text("func").contains("not function"),
        "{}",
        h.text("func")
    );
    assert_eq!(
        h.text("with_state"),
        "5",
        "a state inside reads as its value"
    );
    assert_eq!(h.text("nulls"), "nil 1");
    assert_eq!(h.text("strings"), "x");
}

#[test]
fn canvas_colours_follow_theme_tokens() {
    let mut h = Harness::inline(
        r#"local draws = 0
        local shown = ui.state("shown", 0)
        ui.command { id = "count", run = function() shown:set(draws) end }
        ui.command { id = "light", run = function() ui.theme.set("zgui-light") end }
        return ui.column {
            ui.canvas { id = "c", w = 40, h = 20, draw = function(c, w, h)
                draws += 1
                c:rect(0, 0, w, h, "$accent", 4)
            end },
            ui.text { id = "draws", "{shown}" },
        }"#,
    );
    h.frames(2);
    run(&mut h, "count");
    assert_eq!(h.error(), None);
    let first: u32 = h.text("draws").parse().unwrap();
    assert!(first >= 1);
    run(&mut h, "light");
    h.frames(2);
    run(&mut h, "count");
    let after: u32 = h.text("draws").parse().unwrap();
    assert!(
        after > first,
        "a theme change draws the canvas again ({first} -> {after})"
    );
    assert_eq!(h.error(), None);
}

#[test]
fn ui_json_encodes_and_decodes_what_state_holds() {
    let mut h = Harness::inline(
        r#"local doc = { version = 1, shapes = { { id = "a", w = 12.5, tags = {} }, { id = "b", note = ui.null } } }
        local text = ui.json.encode(doc)
        local back = ui.json.decode(text)
        back.shapes[1].w += 1 -- decoded tables are the script's to change
        local pretty = ui.json.encode({ a = 1 }, { pretty = true })
        local ok, err = pcall(ui.json.decode, "{ not json")
        local ok_fn, err_fn = pcall(ui.json.encode, { f = function() end })
        return ui.column {
            ui.text { id = "text", text },
            ui.text { id = "back", `{back.version} {back.shapes[1].w} {#back.shapes} {back.shapes[2].note}` },
            ui.text { id = "pretty", pretty },
            ui.text { id = "bad", if ok then "parsed" else tostring(err) },
            ui.text { id = "fn", if ok_fn then "encoded" else tostring(err_fn) },
        }"#,
    );
    assert_eq!(h.error(), None);
    assert_eq!(
        h.text("text"),
        r#"{"shapes":[{"id":"a","tags":[],"w":12.5},{"id":"b","note":null}],"version":1}"#
    );
    assert_eq!(h.text("back"), "1 13.5 2 nil");
    assert_eq!(h.text("pretty"), "{\n  \"a\": 1\n}");
    assert!(
        h.text("bad")
            .contains("ui.json.decode: key must be a string at line 1"),
        "{}",
        h.text("bad")
    );
    assert!(h.text("fn").contains("ui.json.encode"), "{}", h.text("fn"));
}
