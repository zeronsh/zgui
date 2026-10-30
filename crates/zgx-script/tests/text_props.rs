//! What text takes from a script beyond its size and colour: OpenType
//! features.
mod common;
use common::Harness;

fn features(h: &mut Harness, id: &str) -> Vec<(String, u32)> {
    let node = h.find(id).unwrap();
    let scene = h.ui.scene.borrow();
    // An `id` names a text's box; its text is the node inside.
    let font = scene.font(scene.children(node)[0]);
    (font.features.settings().iter())
        .map(|(tag, value)| (String::from_utf8_lossy(tag).into_owned(), *value))
        .collect()
}

#[test]
fn font_features_set_inherit_and_follow_state() {
    let mut h = Harness::inline(
        r##"local alternate = ui.state("alternate", false)
        return ui.column {
            ui.text { id = "plain", "plain" },
            ui.text { id = "set", font_features = "ss01 cv02=3 liga=0", "set" },
            ui.column { font_features = "smcp", ui.text { id = "inside", "inside" } },
            ui.text { id = "odd", font_features = "ss01 toolong x=1 cv02=no", "odd" },
            ui.text { id = "followed", "followed", font_features = function()
                return if alternate:get() then "ss02" else "ss03"
            end },
        }"##,
    );
    h.frames(1);
    assert_eq!(features(&mut h, "plain"), []);
    let pair = |tag: &str, value| (tag.to_string(), value);
    assert_eq!(
        features(&mut h, "set"),
        [pair("cv02", 3), pair("liga", 0), pair("ss01", 1)]
    );
    assert_eq!(features(&mut h, "inside"), [pair("smcp", 1)]);
    assert_eq!(
        features(&mut h, "odd"),
        [pair("ss01", 1)],
        "the rest left out"
    );
    assert_eq!(features(&mut h, "followed"), [pair("ss03", 1)]);
    h.host
        .set_state("alternate", zgx_script::Value::Bool(true))
        .unwrap();
    h.frames(1);
    assert_eq!(features(&mut h, "followed"), [pair("ss02", 1)]);
    assert_eq!(h.error(), None);
}
