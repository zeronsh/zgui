//! The decoded-picture cache keeps what a mounted node shows and, of the
//! rest, the most recently shown within its byte budget
//! (`ScriptHost::image_cache`): a page of images that unmounts lets go of
//! all but that, and showing those again decodes nothing.
mod common;
use common::Harness;
use zgx_script::ScriptHost;

/// A 2×2 PNG: 16 bytes decoded.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 132,
    25, 96, 12, 0, 71, 202, 7, 249, 103, 89, 110, 183, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

const APP: &str = r##"
local page = ui.state("page", "all")
local function images(from, to)
    local list = {}
    for i = from, to do
        list[#list + 1] = ui.image { src = `img{i}.png`, w = 2, h = 2 }
    end
    return ui.row(list)
end
return ui.column {
    ui.switch(page, {
        all = images(1, 40),
        recent = images(31, 40),
        old = images(1, 10),
        none = ui.div {},
    }),
    ui.button { id = "none", w = 40, h = 20, on_click = function() page:set("none") end, "None" },
    ui.button { id = "recent", w = 40, h = 20, on_click = function() page:set("recent") end, "Recent" },
    ui.button { id = "old", w = 40, h = 20, on_click = function() page:set("old") end, "Old" },
}
"##;

#[test]
fn an_unmounted_page_leaves_the_budget_and_comes_back_from_it() {
    let dir = std::env::temp_dir().join(format!("zgx-image-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=40 {
        std::fs::write(dir.join(format!("img{i}.png")), PNG).unwrap();
    }
    let app = dir.join("app.luau");
    std::fs::write(&app, APP).unwrap();
    // Ten pictures' worth of what no node shows.
    let mut h = Harness::new(move |runtime| ScriptHost::new(runtime, app).image_cache(10 * 16));
    h.frames(1);
    assert_eq!(h.error(), None);
    assert_eq!(
        h.host.image_cache_stats(),
        (40, 40 * 16, 40),
        "all shown, all kept"
    );

    h.click("none");
    h.frames(1);
    let (count, bytes, decoded) = h.host.image_cache_stats();
    assert_eq!(
        (count, bytes, decoded),
        (10, 10 * 16, 40),
        "down to the budget"
    );

    h.click("recent");
    h.frames(1);
    assert_eq!(
        h.host.image_cache_stats().2,
        40,
        "the ten most recent are hits"
    );

    h.click("old");
    h.frames(1);
    assert_eq!(
        h.host.image_cache_stats().2,
        50,
        "the others are decoded again"
    );
    std::fs::remove_dir_all(&dir).ok();
}
