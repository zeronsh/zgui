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
        ui.command { id = "light", run = function() ui.theme.set("zgx-light") end }
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

/// `ui.sorted` keeps a list state's order as rows are written, always the
/// order `ui.sort` gives the list as it is (stable, nil keys last), with a
/// filter and a spec that changes; and a write re-judges the rows it wrote,
/// not the list.
#[test]
fn ui_sorted_keeps_the_order_ui_sort_gives_and_judges_only_written_rows() {
    let mut h = Harness::inline(
        r#"
        local seed = 7
        local function random(n)
            seed = (seed * 1103515245 + 12345) % 2147483648
            return seed % n
        end
        local initial = {}
        for i = 1, 2000 do
            initial[i] = { n = `r{i}`, v = if i % 97 == 0 then nil else random(500), sector = random(4) }
        end
        local rows = ui.state("rows", initial)
        local key = ui.state("key", "v")
        local descending = ui.state("descending", false)
        local only = ui.state("only", -1)
        local judged = 0
        local view = ui.sorted(rows, function()
            local sector, by = only:get(), key:get()
            return {
                by = if by == "fn" then function(r) judged += 1; return r.n end else by,
                descending = descending:get(),
                filter = if sector >= 0 then function(r) return r.sector == sector end else nil,
            }
        end)
        local function names(list)
            local out = table.create(#list)
            for i, r in list do out[i] = r.n end
            return table.concat(out, ",")
        end
        local function expected()
            local list = {}
            for _, r in rows:peek() do
                if only:peek() < 0 or r.sector == only:peek() then table.insert(list, r) end
            end
            local by = key:peek()
            local by = if by == "fn" then function(r) return r.n end else by
            return names(ui.sort(list, by, descending:peek()))
        end
        local checks = ui.state("checks", "")
        local function check(label)
            local same = names(view:get()) == expected()
            checks:set(checks:peek() .. (if same then "" else label .. " differs; "))
        end
        ui.command { id = "run", run = function()
            check("initial")
            for round = 1, 20 do
                ui.batch(function()
                    for _ = 1, 30 do
                        local i = random(#rows:peek()) + 1
                        rows:set_key({ i, "v" }, if random(10) == 0 then nil else random(500))
                    end
                end)
                check(`round {round}`)
            end
            rows:push({ n = "new", v = 250, sector = 1 })
            check("push")
            descending:set(true)
            check("descending")
            only:set(2)
            check("filtered")
            ui.batch(function()
                for _ = 1, 30 do
                    local i = random(#rows:peek()) + 1
                    rows:set_key({ i, "sector" }, random(4))
                end
            end)
            check("filter changes")
            -- One row written again and again in a batch: each write a new
            -- table where the last one was.
            ui.batch(function()
                rows:set_key({ 3, "v" }, 1)
                rows:set_key({ 3, "v" }, 499)
                rows:set_key({ 3, "v" }, 2)
            end)
            check("one row written thrice")
            -- Held by the script, the list is copied by the next write.
            local held = rows:get()
            rows:set_key({ 9, "v" }, 7)
            check("written while held")
            assert(#held == #rows:peek())
            rows:remove(1)
            check("removed")
            local copy = table.clone(rows:peek())
            copy[4] = { n = "swapped", v = 3, sector = 2 }
            rows:set(copy)
            check("set whole")
            only:set(-1)
            key:set("fn")
            check("by a function")
            judged = 0
            rows:set_key({ 5, "n" }, "zz")
            check("renamed")
            checks:set(checks:peek() .. `judged {judged}`)
        end }
        return ui.text { id = "checks", "{checks}" }
        "#,
    );
    assert_eq!(h.error(), None);
    run(&mut h, "run");
    assert_eq!(h.error(), None);
    // Every check the same as ui.sort, and the one write judged one row.
    assert_eq!(h.text("checks"), "judged 1");
}

/// A whole sort of numbers, of strings, or of keys of mixed kinds, each with
/// nils among them, gives `ui.sort`'s order both ways: negatives and minus
/// zero, strings sharing their first 8 bytes or shorter, past ASCII, equal
/// keys by position, rows of two shapes, and again after one row is written.
#[test]
fn ui_sorted_sorts_numbers_and_strings_as_ui_sort_does() {
    let mut h = Harness::inline(
        r#"
        local seed = 11
        local function random(n)
            seed = (seed * 1103515245 + 12345) % 2147483648
            return seed % n
        end
        local words = { "alpha", "alphabetical", "alphabetic", "", "zeta", "Zeta", "ünder", "under", "alpha", "a" }
        local rows = {}
        for i = 1, 3000 do
            local n = if i % 89 == 0 then nil elseif i % 7 == 0 then -0.0 else (random(2000) - 1000) / 8
            local w = if i % 83 == 0 then nil else words[random(#words) + 1] .. if random(3) == 0 then `{random(5)}` else ""
            -- Some rows of another shape (an extra field before the others):
            -- the field is not where the row before had it.
            local m = if i % 3 == 0 then `s{random(50)}` elseif i % 3 == 1 then random(50) else i % 2 == 0
            rows[i] = if i % 5 == 0 then { aa = true, id = i, m = m, n = n, w = w } else { id = i, m = m, n = n, w = w }
        end
        local list = ui.state("rows", rows)
        local key = ui.state("key", "n")
        local descending = ui.state("descending", false)
        local view = ui.sorted(list, function()
            return { by = key:get(), descending = descending:get() }
        end)
        local function ids(rows)
            local out = table.create(#rows)
            for i, r in rows do out[i] = r.id end
            return table.concat(out, ",")
        end
        local checks = ui.state("checks", "")
        ui.command { id = "run", run = function()
            local function check(label)
                local same = ids(view:get()) == ids(ui.sort(table.clone(list:peek()), key:peek(), descending:peek()))
                checks:set(checks:peek() .. (if same then "" else `{label} differs; `))
            end
            -- Numbers, strings, and keys of mixed kinds (sorted as before).
            for _, by in { "n", "w", "m" } do
                for _, down in { false, true } do
                    key:set(by)
                    descending:set(down)
                    check(`{by} {down}`)
                end
            end
            -- One row written: re-placed, then all sorted again.
            key:set("w")
            descending:set(false)
            list:set_key({ 10, "w" }, "alphabet")
            check("one written")
            descending:set(true)
            check("one written, sorted again")
            checks:set(checks:peek() .. "done")
        end }
        return ui.text { id = "checks", "{checks}" }
        "#,
    );
    assert_eq!(h.error(), None);
    run(&mut h, "run");
    assert_eq!(h.error(), None);
    assert_eq!(h.text("checks"), "done");
}

/// A table `filter` keeps what the same tests as a function keep: fields
/// equal to a string, a number or a boolean, holding a text (case folded or
/// not), rows written into and out of it, both ways round.
#[test]
fn ui_sorted_table_filters_keep_what_the_same_function_keeps() {
    let mut h = Harness::inline(
        r#"
        local seed = 3
        local function random(n)
            seed = (seed * 1103515245 + 12345) % 2147483648
            return seed % n
        end
        local names = { "Alpha", "beta", "GAMMA", "alphabet", "Delta" }
        local rows = {}
        for i = 1, 2000 do
            rows[i] = { id = i, name = names[random(#names) + 1] .. random(10), group = random(4), live = random(2) == 0, v = random(300) }
        end
        local list = ui.state("rows", rows)
        local spec = ui.state("spec", 1)
        local descending = ui.state("descending", false)
        local specs = {
            { table = { group = 2 }, fn = function(r) return r.group == 2 end },
            { table = { live = true, name = { contains = "alpha" } }, fn = function(r) return r.live and string.find(r.name, "alpha", 1, true) ~= nil end },
            { table = { name = { contains = "ALPHA", case = false } }, fn = function(r) return string.find(string.lower(r.name), "alpha", 1, true) ~= nil end },
            { table = { name = "beta3", group = 1 }, fn = function(r) return r.name == "beta3" and r.group == 1 end },
        }
        local by_table = ui.sorted(list, function()
            return { by = "v", descending = descending:get(), filter = specs[spec:get()].table }
        end)
        local by_fn = ui.sorted(list, function()
            return { by = "v", descending = descending:get(), filter = specs[spec:get()].fn }
        end)
        local function ids(rows)
            local out = table.create(#rows)
            for i, r in rows do out[i] = r.id end
            return table.concat(out, ",")
        end
        local checks = ui.state("checks", "")
        local function check(label)
            local same = ids(by_table:get()) == ids(by_fn:get())
            checks:set(checks:peek() .. (if same then "" else `{label} differs; `))
        end
        ui.command { id = "run", run = function()
            for k = 1, #specs do
                for _, down in { false, true } do
                    spec:set(k)
                    descending:set(down)
                    check(`spec {k} {down}`)
                end
                -- Rows written into and out of what it keeps.
                ui.batch(function()
                    for _ = 1, 20 do
                        local i = random(2000) + 1
                        list:set_key({ i, "group" }, random(4))
                        list:set_key({ i, "live" }, random(2) == 0)
                    end
                end)
                check(`spec {k} written`)
            end
            checks:set(checks:peek() .. "done")
        end }
        return ui.text { id = "checks", "{checks}" }
        "#,
    );
    assert_eq!(h.error(), None);
    run(&mut h, "run");
    assert_eq!(h.error(), None);
    assert_eq!(h.text("checks"), "done");
}

/// `index_of` finds an item by a field, tracked; a sorted view's `len`,
/// `at` and `index_of` are its order now inside the batch that wrote what
/// it sorts.
#[test]
fn index_of_finds_items_and_a_view_reads_as_written_within_a_batch() {
    let mut h = Harness::inline(
        r#"
        local items = ui.state("items", {
            { id = 1, box = "in" }, { id = 2, box = "in" }, { id = 3, box = "out" }, { id = 4, box = "in" },
        })
        local inbox = ui.sorted(items, { filter = { box = "in" } })
        local seen = ui.state("seen", "")
        local runs = ui.state("runs", 0)
        ui.effect(function()
            items:index_of("id", 3)
            runs:set(runs:peek() + 1)
        end)
        ui.command { id = "run", run = function()
            local out = {}
            table.insert(out, tostring(items:index_of("id", 3)))
            table.insert(out, tostring(items:index_of("id", 9)))
            table.insert(out, tostring(inbox:index_of("id", 4)))
            ui.batch(function()
                items:set_key({ 2, "box" }, "out")
                -- Within the batch: the view as written.
                table.insert(out, tostring(inbox:len()))
                table.insert(out, tostring(inbox:index_of("id", 4)))
                table.insert(out, tostring((inbox:at(2) :: any).id))
            end)
            seen:set(table.concat(out, " "))
        end }
        return ui.text { id = "seen", function() return `{seen:get()} runs {runs:get()}` end }
        "#,
    );
    assert_eq!(h.error(), None);
    run(&mut h, "run");
    assert_eq!(h.error(), None);
    // Found, not found, in the view; then two in the view, the 4th second.
    // The effect ran again once (its read follows the list's write).
    assert_eq!(h.text("seen"), "3 nil 3 2 2 4 runs 2");
}
