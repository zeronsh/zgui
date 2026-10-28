//! `ui.scene3d`: a 3D scene in a box, drawn only when what it reads
//! changes, its bindings stepped in Rust with no script per frame.
mod common;
use common::Harness;
use zgx::scene::{NodeId, NodeKind};

/// The image a GPU view's box shows, its frame (box, then its clip, then
/// the frame).
fn frame_id(h: &mut Harness, id: &str) -> Option<u64> {
    let root = h.find(id)?;
    let scene = h.ui.scene.borrow();
    let clip = *scene.children(root).first()?;
    let frame: NodeId = *scene.children(clip).first()?;
    match scene.kind(frame) {
        NodeKind::Image(image) if image.gpu_frame().is_some() => Some(image.id()),
        _ => None,
    }
}

const SCENE: &str = r##"
local angle = ui.state("angle", 0)
local red = ui.state("red", false)
return ui.column {
    ui.button { id = "turn", w = 40, h = 20, on_click = function() angle:set(angle:get() + 90) end, "turn" },
    ui.button { id = "paint", w = 40, h = 20, on_click = function() red:set(not red:get()) end, "paint" },
    ui.text { id = "label", "a label" },
    ui.scene3d {
        id = "scene",
        w = 200,
        h = 100,
        background = "#101418",
        camera = { position = { 0, 1, 4 }, fov = 40 },
        ui.light { kind = "directional", direction = { -1, -2, -1 }, intensity = 3 },
        ui.light { kind = "hemisphere", color = "#8899aa", ground = "#221a14" },
        ui.group {
            position = { 0, 0.5, 0 },
            ui.mesh {
                geometry = "box",
                rotation = { 0, ui.spring(angle), 0 },
                material = { color = function() return if red:get() then "#ff0000" else "#3366ff" end, roughness = 0.4 },
            },
            ui.mesh { geometry = "sphere", radius = 0.3, position = { 1.2, 0, 0 } },
        },
        ui.mesh { geometry = "plane", size = { 6, 1, 6 }, position = { 0, -0.5, 0 } },
    },
}
"##;

#[test]
fn a_scene_shows_in_a_gpu_view_box() {
    let mut h = Harness::inline(SCENE);
    assert_eq!(h.error(), None);
    h.frame();
    assert!(frame_id(&mut h, "scene").is_some(), "{}", h.tree());
    assert_eq!(h.bounds("scene").width, 200.);
}

#[test]
fn a_still_scene_draws_no_frame() {
    let mut h = Harness::inline(SCENE);
    h.frames(3);
    let still = frame_id(&mut h, "scene");
    let calls = h.calls();
    h.frames(30);
    assert_eq!(frame_id(&mut h, "scene"), still, "nothing changed");
    assert_eq!(h.calls(), calls, "no script ran");
}

#[test]
fn a_bound_state_draws_new_frames_without_script_per_frame() {
    let mut h = Harness::inline(SCENE);
    h.frames(3);
    let still = frame_id(&mut h, "scene");
    // A spring toward the new angle: frames while it moves, from Rust.
    h.click("turn");
    let calls = h.calls();
    let mut frames = std::collections::HashSet::new();
    for _ in 0..30 {
        h.frame();
        frames.insert(frame_id(&mut h, "scene"));
    }
    assert!(
        frames.len() > 5,
        "a frame each step of the spring: {}",
        frames.len()
    );
    assert_ne!(frame_id(&mut h, "scene"), still);
    assert_eq!(h.calls(), calls, "no script ran while the spring moved");
    // Settled, it draws no more.
    h.frames(300);
    let settled = frame_id(&mut h, "scene");
    h.frames(10);
    assert_eq!(frame_id(&mut h, "scene"), settled, "idle once settled");
}

#[test]
fn a_colour_read_from_state_draws_a_frame_when_it_changes() {
    let mut h = Harness::inline(SCENE);
    h.frames(3);
    let before = frame_id(&mut h, "scene");
    h.click("paint");
    h.frame();
    assert_ne!(frame_id(&mut h, "scene"), before);
    let after = frame_id(&mut h, "scene");
    h.frames(5);
    assert_eq!(frame_id(&mut h, "scene"), after);
}

/// What `make` raises, shown as text.
fn raised(make: &str) -> String {
    let mut h = Harness::inline(&format!(
        r#"local ok, err = pcall(function() return {make} end)
return ui.text {{ id = "error", if ok then "no error" else tostring(err) }}"#
    ));
    h.text("error")
}

#[test]
fn objects_and_props_are_checked() {
    assert!(raised(r#"ui.mesh { geometry = "teapot" }"#).contains("geometry"));
    assert!(raised(r#"ui.scene3d { ui.text { "not 3D" } }"#).contains("3D objects"));
    assert!(raised(r#"ui.mesh { position = { "x" } }"#).contains("position"));
    assert!(raised(r#"ui.light { kind = "laser" }"#).contains("kind"));
    assert!(raised(r#"ui.scene3d { orbit = 3 }"#).contains("orbit"));
    assert_eq!(
        raised(r#"ui.mesh { geometry = "torus", scale = 2 }"#),
        "no error"
    );
}
