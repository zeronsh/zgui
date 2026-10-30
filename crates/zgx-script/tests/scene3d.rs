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

const PICKED: &str = r##"
local clicked = ui.state("clicked", "none")
local over = ui.state("over", false)
local group = ui.state("group", 0)
return ui.column {
    ui.text { id = "clicked", "{clicked}" },
    ui.text { id = "over", function() return if over:get() then "over" else "out" end },
    ui.text { id = "group", "{group}" },
    ui.scene3d {
        id = "scene",
        w = 200,
        h = 200,
        orbit = true,
        camera = { position = { 0, 0, 4 }, fov = 40 },
        ui.mesh {
            geometry = "box",
            on_click = function(hit) clicked:set(string.format("%.2f", hit.z)) end,
            on_hover = function(is_over) over:set(is_over) end,
        },
        ui.group {
            position = { 0, 1.2, 0 },
            on_click = function() group:set(group:get() + 1) end,
            ui.mesh { geometry = "sphere", radius = 0.3 },
        },
    },
}
"##;

/// Press and release at `(x, y)` of the box `id`, from its top left, the
/// release `moved` points further right.
fn press(h: &mut zgx_script::testing::TestApp, id: &str, (x, y): (f32, f32), moved: f32) {
    use zgx::input::{InputEvent, PointerButton};
    let b = h.bounds(id);
    let (x, y) = (b.x + x, b.y + y);
    let button = PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown { x, y, button });
    h.ui.dispatch(InputEvent::PointerMove { x: x + moved, y });
    h.ui.dispatch(InputEvent::PointerUp {
        x: x + moved,
        y,
        button,
    });
    h.settle();
}

#[test]
fn a_click_on_a_mesh_says_where_and_one_beside_it_nothing() {
    let mut h = Harness::inline(PICKED);
    assert_eq!(h.error(), None);
    h.frame();
    // The box's front face, half a unit towards the camera.
    press(&mut h, "scene", (100., 100.), 0.);
    assert_eq!(h.text("clicked"), "0.50");
    h.ui.dispatch(zgx::input::InputEvent::PointerMove { x: 0., y: 0. });
    press(&mut h, "scene", (5., 195.), 0.);
    assert_eq!(h.text("clicked"), "0.50", "nothing there");
}

#[test]
fn a_drag_orbits_and_clicks_nothing() {
    let mut h = Harness::inline(PICKED);
    h.frame();
    press(&mut h, "scene", (100., 100.), 30.);
    assert_eq!(h.text("clicked"), "none");
}

#[test]
fn a_click_on_a_mesh_in_a_group_reaches_the_group() {
    let mut h = Harness::inline(PICKED);
    h.frame();
    // The sphere, 1.2 up: about 0.82 of the half height at fov 40, 4 away.
    let up = 1.2 / (4. * 20_f32.to_radians().tan());
    press(&mut h, "scene", (100., 100. - 100. * up), 0.);
    assert_eq!(h.text("group"), "1");
    assert_eq!(h.text("clicked"), "none");
}

#[test]
fn hovering_says_over_and_out() {
    let mut h = Harness::inline(PICKED);
    h.frame();
    let b = h.bounds("scene");
    let at = |x: f32, y: f32| zgx::input::InputEvent::PointerMove {
        x: b.x + x,
        y: b.y + y,
    };
    h.ui.dispatch(at(100., 100.));
    h.settle();
    assert_eq!(h.text("over"), "over");
    h.ui.dispatch(at(5., 195.));
    h.settle();
    assert_eq!(h.text("over"), "out");
    h.ui.dispatch(at(100., 100.));
    h.ui.dispatch(zgx::input::InputEvent::PointerMove { x: -50., y: -50. });
    h.settle();
    assert_eq!(h.text("over"), "out", "leaving the box");
}

#[test]
fn a_handler_must_be_a_function() {
    assert!(raised(r#"ui.scene3d { ui.mesh { on_click = 3 } }"#).contains("on_click"));
}

#[test]
fn shadows_are_asked_for_with_true() {
    let mut h = Harness::inline(
        r#"return ui.scene3d { id = "scene", w = 100, h = 100, shadows = true,
            ui.light { kind = "directional" }, ui.mesh { geometry = "box" } }"#,
    );
    assert_eq!(h.error(), None);
    h.frame();
    assert!(frame_id(&mut h, "scene").is_some());
    assert!(raised(r#"ui.scene3d { shadows = 1 }"#).contains("shadows"));
}

#[test]
fn an_environment_is_named_given_or_turned_off() {
    for environment in [
        r#""studio""#,
        r#""none""#,
        r#"{ intensity = 1.5, rotation = 90 }"#,
        r#"{ src = "studio", intensity = function() return 2 end }"#,
    ] {
        let mut h = Harness::inline(&format!(
            r#"return ui.scene3d {{ id = "scene", w = 100, h = 100, environment = {environment},
                ui.mesh {{ geometry = "sphere", material = {{ metallic = 1, roughness = 0.2 }} }} }}"#
        ));
        assert_eq!(h.error(), None, "{environment}");
        h.frame();
        assert!(frame_id(&mut h, "scene").is_some(), "{environment}");
    }
    assert!(raised(r#"ui.scene3d { environment = 3 }"#).contains("environment"));
    // A file that isn't there is an error the script sees.
    let h = Harness::inline(
        r#"return ui.scene3d { id = "scene", w = 100, h = 100, environment = "no such sky.hdr",
            ui.mesh { geometry = "box" } }"#,
    );
    h.frame();
    let error = h.error().unwrap_or_default();
    assert!(error.contains("no such sky.hdr"), "{error}");
}

/// A `.glb` of a unit square facing +z, as glTF writes one, with a clip
/// ("slide") moving it from 0 to 4 along x over 4 seconds.
#[cfg(feature = "gltf")]
fn square_glb() -> Vec<u8> {
    let positions: [f32; 12] = [-0.5, -0.5, 0., 0.5, -0.5, 0., 0.5, 0.5, 0., -0.5, 0.5, 0.];
    let mut bin: Vec<u8> = positions.iter().flat_map(|v| v.to_le_bytes()).collect();
    bin.extend([0_u16, 1, 2, 0, 2, 3].iter().flat_map(|i| i.to_le_bytes()));
    let keys: [f32; 8] = [0., 4., 0., 0., 0., 4., 0., 0.];
    bin.extend(keys.iter().flat_map(|v| v.to_le_bytes()));
    let mut json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],
        "bufferViews":[{{"buffer":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":12}},
            {{"buffer":0,"byteOffset":60,"byteLength":8}},{{"buffer":0,"byteOffset":68,"byteLength":24}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},
            {{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}},
            {{"bufferView":2,"componentType":5126,"count":2,"type":"SCALAR"}},
            {{"bufferView":3,"componentType":5126,"count":2,"type":"VEC3"}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
        "animations":[{{"name":"slide","samplers":[{{"input":2,"output":3}}],
            "channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}}]}}],
        "nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}]}}"#,
        bin.len()
    )
    .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut out = b"glTF".to_vec();
    out.extend(2_u32.to_le_bytes());
    out.extend(((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// A script `code` beside `square.glb`, in a directory of its own.
#[cfg(feature = "gltf")]
fn beside_a_model(name: &str, code: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("zgx-model-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("square.glb"), square_glb()).unwrap();
    std::fs::write(dir.join("app.luau"), code).unwrap();
    Harness::new(|runtime| zgx_script::ScriptHost::new(runtime, dir.join("app.luau")))
}

#[cfg(feature = "gltf")]
#[test]
fn a_model_shows_once_read_and_hears_clicks_on_its_meshes() {
    let mut h = beside_a_model(
        "click",
        r#"
local clicked = ui.state("clicked", "none")
return ui.column {
    ui.text { id = "clicked", "{clicked}" },
    ui.scene3d {
        id = "scene", w = 200, h = 200,
        camera = { position = { 0, 0, 3 } },
        ui.model { src = "square.glb", fit = 2, on_click = function(hit) clicked:set(string.format("%.1f", hit.x)) end },
    },
}
"#,
    );
    assert_eq!(h.error(), None);
    // Read off the main thread: it shows (and hears) once read.
    let heard = h.wait_until(|h| {
        h.frame();
        press(h, "scene", (100., 100.), 0.);
        h.text("clicked") != "none"
    });
    assert!(heard, "never clicked: {}", h.tree());
    assert_eq!(h.text("clicked"), "0.0");
}

#[cfg(feature = "gltf")]
#[test]
fn a_model_that_cannot_be_read_says_so() {
    let h = beside_a_model(
        "missing",
        r#"return ui.scene3d { w = 100, h = 100, ui.model { src = "nowhere.glb" } }"#,
    );
    h.frame();
    let error = h.error().unwrap_or_default();
    assert!(error.contains("nowhere.glb"), "{error}");
    assert!(raised(r#"ui.scene3d { ui.model { fit = 2 } }"#).contains("src"));
    // Named alone (a module building objects), it is still there.
    assert!(raised(r#"ui.model { fit = 2 }"#).contains("src"));
}

#[cfg(not(feature = "gltf"))]
#[test]
fn a_model_without_the_gltf_feature_says_so() {
    assert!(raised(r#"ui.scene3d { ui.model { src = "a.glb" } }"#).contains("`gltf` feature"));
}

#[cfg(feature = "gltf")]
#[test]
fn a_model_plays_its_clip_frame_by_frame() {
    // Sliding along x a unit a second; the camera 6 back sees x 1 at 40
    // points right of the middle (its half width 2.49 at that distance).
    let mut h = beside_a_model(
        "play",
        r#"
local clicks = ui.state("clicks", 0)
local at = ui.state("at", "none")
return ui.column {
    ui.text { id = "clicks", "{clicks}" },
    ui.text { id = "at", "{at}" },
    ui.scene3d {
        id = "scene", w = 200, h = 200,
        camera = { position = { 0, 0, 6 } },
        ui.model {
            src = "square.glb",
            animation = "slide",
            on_click = function(hit)
                clicks:set(clicks:get() + 1)
                at:set(string.format("%.1f", hit.x))
            end,
        },
    },
}
"#,
    );
    let px = |x: f32| (100. + x / (6. * 22.5_f32.to_radians().tan()) * 100., 100.);
    // Read, and where it starts (x 0.3 on it).
    assert!(
        h.wait_until(|h| {
            h.frame();
            press(h, "scene", px(0.3), 0.);
            h.text("clicks") != "0"
        }),
        "never read"
    );
    // A second on: a unit along, past that point, onto x 1.3.
    h.frames(60);
    let clicked = h.text("clicks");
    press(&mut h, "scene", px(0.3), 0.);
    assert_eq!(h.text("clicks"), clicked, "it slid away");
    press(&mut h, "scene", px(1.3), 0.);
    assert_ne!(h.text("clicks"), clicked);
    assert_eq!(h.text("at"), "1.3");
}

#[cfg(feature = "gltf")]
#[test]
fn a_clip_it_has_not_says_which_it_has() {
    let mut h = beside_a_model(
        "no-clip",
        r#"return ui.scene3d { w = 100, h = 100, ui.model { src = "square.glb", animation = "walk" } }"#,
    );
    assert!(
        h.wait_until(|h| {
            h.frame();
            h.error().is_some()
        }),
        "no error"
    );
    let error = h.error().unwrap();
    assert!(error.contains("no animation walk: it has slide"), "{error}");
    assert!(
        raised(r#"ui.scene3d { ui.model { src = "a.glb", animation = 0 } }"#).contains("animation")
    );
}

#[test]
fn tone_mapping_and_exposure_are_checked() {
    let mut h = Harness::inline(
        r#"local stops = ui.state("stops", 0)
        return ui.scene3d { id = "scene", w = 100, h = 100, tone_mapping = "aces", exposure = stops,
            ui.mesh { geometry = "box" } }"#,
    );
    assert_eq!(h.error(), None);
    h.frame();
    assert!(frame_id(&mut h, "scene").is_some());
    assert!(raised(r#"ui.scene3d { tone_mapping = "reinhard" }"#).contains("tone_mapping"));
    assert!(raised(r#"ui.scene3d { antialias = 4 }"#).contains("antialias"));
    let mut h = Harness::inline(
        r#"return ui.scene3d { id = "scene", w = 100, h = 100, antialias = false, ui.mesh { geometry = "box" } }"#,
    );
    assert_eq!(h.error(), None);
    h.frame();
    assert!(frame_id(&mut h, "scene").is_some());
}
