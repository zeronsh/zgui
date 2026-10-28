//! Built without the `scene3d` feature, a script making a 3D scene hears
//! which feature it needs.
use zgx::widgets::Ui;
use zgx_script::ScriptHost;

fn error_of(code: &str) -> String {
    let ui = Ui::new(100., 100.);
    let host = ScriptHost::inline(&ui.runtime, "app", code);
    host.reload().err().unwrap_or_default()
}

#[test]
fn a_scene_without_the_feature_says_which_feature_it_needs() {
    for call in ["ui.scene3d {}", "ui.mesh {}", "ui.light {}", "ui.group {}"] {
        let error = error_of(&format!("local a = 1\nreturn ui.column {{\n  {call},\n}}"));
        assert!(error.contains("`scene3d` feature"), "{call}: {error}");
    }
    // Over several lines.
    let error = error_of("return ui.column {\n  ui.scene3d {\n    w = 100,\n    h = 100,\n  },\n}");
    assert!(error.contains("`scene3d` feature"), "{error}");
    // Any other call of nothing is only that.
    let error = error_of("local a = 1\nreturn ui.nothing {}");
    assert!(error.contains("attempt to call a nil value"), "{error}");
    assert!(!error.contains("scene3d"), "{error}");
}
