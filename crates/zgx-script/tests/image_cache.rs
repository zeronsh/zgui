//! The decoded-picture cache keeps what a mounted node shows and, of the
//! rest, the most recently shown within its byte budget
//! (`ScriptHost::image_cache`): a page of images that unmounts lets go of
//! all but that, and showing those again decodes nothing.
mod common;
use common::Harness;
use zgx_script::ScriptHost;

/// A 2×2 PNG: its file, which the cache counts (its pixels decode as drawn).
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
    let mut h =
        Harness::new(move |runtime| ScriptHost::new(runtime, app).image_cache(10 * PNG.len()));
    h.frames(1);
    assert_eq!(h.error(), None);
    assert_eq!(
        h.host.image_cache_stats(),
        (40, 40 * PNG.len(), 40),
        "all shown, all kept"
    );

    h.click("none");
    h.frames(1);
    let (count, bytes, decoded) = h.host.image_cache_stats();
    assert_eq!(
        (count, bytes, decoded),
        (10, 10 * PNG.len(), 40),
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

#[test]
fn boxes_with_background_images_leave_the_budget_as_they_unmount() {
    const BOXES: &str = r##"
local shown = ui.state("shown", true)
local boxes = {}
for i = 1, 40 do
    boxes[i] = ui.div { bg_image = `img{i}.png`, w = 2, h = 2 }
end
return ui.column {
    ui.show(shown, ui.row(boxes)),
    ui.button { id = "hide", w = 40, h = 20, on_click = function() shown:set(false) end, "Hide" },
}
"##;
    let dir = std::env::temp_dir().join(format!("zgx-bg-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=40 {
        std::fs::write(dir.join(format!("img{i}.png")), PNG).unwrap();
    }
    let app = dir.join("app.luau");
    std::fs::write(&app, BOXES).unwrap();
    let mut h =
        Harness::new(move |runtime| ScriptHost::new(runtime, app).image_cache(10 * PNG.len()));
    h.frames(1);
    assert_eq!(h.error(), None);
    assert_eq!(h.host.image_cache_stats().0, 40);
    h.click("hide");
    h.frames(1);
    assert_eq!(
        h.host.image_cache_stats().1,
        10 * PNG.len(),
        "down to the budget"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn going_idle_keeps_only_the_pictures_shown() {
    let dir = std::env::temp_dir().join(format!("zgx-image-idle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for i in 1..=40 {
        std::fs::write(dir.join(format!("img{i}.png")), PNG).unwrap();
    }
    let app = dir.join("app.luau");
    std::fs::write(&app, APP).unwrap();
    let mut h = Harness::new(move |runtime| ScriptHost::new(runtime, app));
    h.frames(1);
    h.click("recent");
    h.frames(1);
    // In use, the budget keeps the pictures the first page showed too.
    assert_eq!(h.host.image_cache_stats().0, 40, "kept within the budget");
    zgx::idle::trim(&h.ui.runtime);
    assert_eq!(
        h.host.image_cache_stats().0,
        10,
        "idle, only the ten shown stay"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// `decode` reaches the picture: as written, `"auto"` when not.
#[test]
fn decode_says_when_a_picture_decodes() {
    use zgx::image::Decoding;
    let dir = std::env::temp_dir().join(format!("zgx-image-decode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["auto", "sync", "async"] {
        std::fs::write(dir.join(format!("{name}.png")), PNG).unwrap();
    }
    let app = dir.join("app.luau");
    std::fs::write(
        &app,
        r##"return ui.column {
    ui.image { id = "auto", src = "auto.png", w = 2, h = 2 },
    ui.image { id = "sync", src = "sync.png", decode = "sync", w = 2, h = 2 },
    ui.image { id = "async", src = "async.png", decode = "async", w = 2, h = 2 },
}"##,
    )
    .unwrap();
    let mut h = Harness::new(move |runtime| ScriptHost::new(runtime, app));
    h.frames(1);
    assert_eq!(h.error(), None);
    let decoding = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Image(image) = scene.kind(n) {
                assert!(image.keeps_file());
                return image.decoding();
            }
            stack.extend(scene.children(n).iter().copied());
        }
        panic!("no picture in {id}")
    };
    assert_eq!(decoding(&mut h, "auto"), Decoding::Auto);
    assert_eq!(decoding(&mut h, "sync"), Decoding::Sync);
    assert_eq!(decoding(&mut h, "async"), Decoding::Async);
    std::fs::remove_dir_all(&dir).ok();
}
