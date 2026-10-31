//! Hyphenation rides with a text's font features (`FontFeatures::HYPHENS`),
//! inherited as they are.
use zgx::{compose::prelude::*, text_layout::FontFeatures, widgets::Ui};

#[test]
fn hyphenation_is_a_feature_inherited_as_features_are() {
    let mut ui = Ui::new(400., 300.);
    let hyphens = FontFeatures::default().with_hyphens(true);
    let handle = ui.mount(
        column()
            .font_features(hyphens)
            .child(text("inherited").id("inherited"))
            .child(
                text("own features")
                    .font_features(FontFeatures::new([(*b"tnum", 1)]))
                    .id("own"),
            ),
    );
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let features = |id: &str| scene.font(handle.find(id).unwrap()).features.clone();
    assert!(features("inherited").hyphens());
    // Features of its own replace the inherited ones, as they always have.
    assert!(!features("own").hyphens());
}

#[test]
fn the_hyphenation_tag_comes_and_goes_with_the_rest_kept() {
    let base = FontFeatures::new([(*b"liga", 0)]);
    let on = base.with_hyphens(true);
    assert!(on.hyphens());
    assert_eq!(on.with_hyphens(false), base);
    assert_eq!(base.with_hyphens(false), base);
}
