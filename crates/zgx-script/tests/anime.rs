//! The anime.js API against its semantics: values in the scene at given
//! times of 60 Hz frames.
mod common;

use common::Harness;
use zgx::{
    input::InputEvent,
    scene::{Color, NodeKind},
};

const MS_PER_FRAME: f64 = 1000. / 60.;

impl Harness {
    /// Runs frames until `ms` of animation time have passed since the first
    /// frame (which renders the start).
    fn run_ms(&mut self, ms: f64) {
        let frames = (ms / MS_PER_FRAME).round() as usize;
        for _ in 0..frames {
            self.frame();
        }
    }
    fn x(&mut self, id: &str) -> f32 {
        let node = self.find(id).unwrap();
        self.ui.scene.borrow().transform(node).x
    }
    fn fill(&mut self, id: &str) -> Color {
        let node = self.find(id).unwrap();
        match self.ui.scene.borrow().kind(node) {
            NodeKind::Panel { quad, .. } | NodeKind::Quad(quad) => quad.fill,
            NodeKind::Rect(c) => *c,
            other => panic!("no fill: {other:?}"),
        }
    }
}
fn close(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 0.6, "{what}: {a} != {b}");
}

const BOX: &str = r##"
local root = ui.column { ui.div { id = "box", w = 10, h = 10, bg = "#000000" }, ui.text { "{log}", id = "log" } }
"##;
fn script(body: &str) -> String {
    format!("local log = ui.state('log', '')\n{body}\n{BOX}\nreturn root")
}

#[test]
fn animate_moves_with_the_default_ease_and_goes_idle() {
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = 100, opacity = 0.5, duration = 1000 })"##,
    ));
    h.frame();
    close(h.x("box"), 0., "starts where it is");
    h.run_ms(500.);
    // anime.js v4's default ease is out(2): 0.75 at half time.
    close(h.x("box"), 75., "x at 500 ms");
    close(h.opacity("box"), 1. - 0.5 * 0.75, "opacity at 500 ms");
    h.run_ms(550.);
    close(h.x("box"), 100., "x at the end");
    close(h.opacity("box"), 0.5, "opacity at the end");
    assert!(
        !h.clock.wants_frame(),
        "a finished animation asks for no frames"
    );
}

#[test]
fn stagger_delays_hold_from_values_until_each_target_starts() {
    let mut h = Harness::inline(
        r##"anime.animate(".dot", { opacity = { 0, 1 }, delay = anime.stagger(100), duration = 200, ease = "linear" })
        return ui.row {
            ui.div { class = "dot", id = "a", w = 4, h = 4 },
            ui.div { class = "dot", id = "b", w = 4, h = 4 },
            ui.div { class = "dot", id = "c", w = 4, h = 4 },
        }"##,
    );
    h.frame();
    for id in ["a", "b", "c"] {
        close(h.opacity(id), 0., id);
    }
    h.run_ms(150.);
    close(h.opacity("a") * 100., 75., "first is 3/4 in");
    close(h.opacity("b") * 100., 25., "second is 1/4 in");
    close(h.opacity("c"), 0., "third has not started");
    h.run_ms(300.);
    close(h.opacity("c"), 1., "third done");
}

#[test]
fn a_selector_finds_only_what_is_still_mounted_after_rows_go() {
    // Six of eight classed rows go, and the two left are rebuilt: the class
    // keeps entries for the gone ones until it sheds them in bulk, and a
    // selector must stagger over the two alone.
    let mut h = Harness::inline(
        r##"local ids = ui.state("ids", { "a", "b", "c", "d", "e", "f", "g", "h" })
        ui.command { id = "trim", run = function() ids:set({ "g", "h" }) end }
        ui.command { id = "go", run = function()
            anime.animate(".dot", { opacity = { 0, 1 }, delay = anime.stagger(100), duration = 200, ease = "linear" })
        end }
        return ui.row {
            ui.each(ids, function(id) return ui.div { class = "dot", id = id, w = 4, h = 4 } end),
        }"##,
    );
    h.frame();
    assert!(h.host.run_command("trim"));
    h.frame();
    assert!(h.find("a").is_none() && h.find("g").is_some());
    assert!(h.host.run_command("go"));
    h.frame();
    h.run_ms(150.);
    // Second of two, 100 ms behind: half the 200 ms tween.
    let (g, h) = (h.opacity("g"), h.opacity("h"));
    assert!(h > 0., "the second has started");
    close((g - h) * 100., 50., "the second is one step behind");
}

#[test]
fn loops_alternate_and_end_where_alternation_leaves_them() {
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = 100, duration = 100, loop = 1, alternate = true, ease = "linear" })"##,
    ));
    h.frame();
    h.run_ms(150.);
    close(h.x("box"), 50., "half way back");
    h.run_ms(100.);
    close(h.x("box"), 0., "ends at the start");
}

#[test]
fn timelines_place_children_by_position_label_and_relative_offset() {
    let mut h = Harness::inline(
        r##"local calls = ui.state("calls", 0)
        local tl = anime.createTimeline({ defaults = { ease = "linear", duration = 100 } })
        tl:add("#a", { x = 100 })
          :label("mid")
          :add("#b", { x = 50 }, "-=50")
          :call(function() calls:add(1) end, "mid+=10")
        return ui.column {
            ui.div { id = "a", w = 4, h = 4 }, ui.div { id = "b", w = 4, h = 4 },
            ui.text { "{calls}", id = "calls" },
        }"##,
    );
    h.frame();
    h.run_ms(100.);
    close(h.x("a"), 100., "a");
    close(h.x("b"), 25., "b started at 50 ms");
    assert_eq!(h.text("calls"), "0");
    h.run_ms(50.);
    assert_eq!(h.text("calls"), "1", "call at mid+10");
    h.run_ms(50.);
    close(h.x("b"), 50., "b done");
    assert!(!h.clock.wants_frame());
}

#[test]
fn a_later_animation_replaces_a_property_and_add_composes() {
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = 100, duration = 100, ease = "linear" })
        anime.animate("#box", { y = 0, duration = 1 })
        anime.animate("#box", { x = 50, duration = 100, ease = "linear", composition = "add" })"##,
    ));
    h.frame();
    h.run_ms(50.);
    close(h.x("box"), 75., "50 replace + 25 added");
    h.run_ms(60.);
    close(h.x("box"), 150., "both done");

    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = 100, duration = 1000, ease = "linear" })
        anime.createTimer({ duration = 500, onComplete = function()
            anime.animate("#box", { x = 0, duration = 100, ease = "linear" })
        end })"##,
    ));
    h.frame();
    h.run_ms(500.);
    let at_switch = h.x("box");
    h.run_ms(50.);
    assert!(
        h.x("box") < at_switch,
        "the new animation took x over: {} vs {at_switch}",
        h.x("box")
    );
    h.run_ms(600.);
    close(
        h.x("box"),
        0.,
        "and finished it, the first no longer writing",
    );
}

#[test]
fn removing_targets_stops_their_animations_and_leaves_timers() {
    // `utils.remove` (and a layout box snapping on a reflow) drops the
    // targets' tracks: an animation left with none stops, while a timer,
    // which never animated a target, plays on.
    let mut h = Harness::inline(&script(
        r##"local ticks = 0
        anime.createTimer({ duration = 300, onUpdate = function() ticks += 1 end,
            onComplete = function() log:set("timer " .. ticks) end })
        anime.animate("#box", { x = 100, duration = 300, ease = "linear",
            onComplete = function() log:set("animation completed") end })
        anime.createTimer({ duration = 50, onComplete = function() anime.utils.remove("#box") end })"##,
    ));
    h.frame();
    h.run_ms(400.);
    let log = h.text("log");
    assert!(
        log.starts_with("timer "),
        "the timer ran to its end: {log:?}"
    );
    assert!(
        h.x("box") < 50.,
        "the removed animation stopped: x = {}",
        h.x("box")
    );
}

#[test]
fn an_animation_whose_every_property_is_replaced_stops() {
    // A loop replaced on its only property: it neither runs on unseen,
    // keeping frames coming, nor calls back (as anime.js cancels it).
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = { 0, 100 }, duration = 400, loop = true, ease = "linear",
            onUpdate = function() log:set("first") end,
            onLoop = function() log:set("looped") end })
        anime.createTimer({ duration = 100, onComplete = function()
            anime.animate("#box", { x = 0, duration = 100, ease = "linear",
                onComplete = function() log:set("second") end })
        end })"##,
    ));
    h.frame();
    h.run_ms(1000.);
    assert_eq!(
        h.text("log"),
        "second",
        "the replaced loop no longer calls back"
    );
    close(h.x("box"), 0., "the replacement finished");
    assert!(
        !h.clock.wants_frame(),
        "nothing plays: the replaced loop stopped"
    );

    // A property left to it keeps it playing.
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = 100, opacity = 0.5, duration = 400, ease = "linear" })
        anime.createTimer({ duration = 100, onComplete = function()
            anime.animate("#box", { x = 0, duration = 100, ease = "linear" })
        end })"##,
    ));
    h.frame();
    h.run_ms(250.);
    close(h.opacity("box"), 1. - 0.5 * 0.625, "opacity still runs");
    h.run_ms(300.);
    close(h.opacity("box"), 0.5, "and finishes");
    close(h.x("box"), 0., "x is the replacement's");
}

#[test]
fn a_line_restarting_its_unfinished_animation_is_reported() {
    // An effect tweening to each new value of a stream faster than the
    // tween plays: it only ever chases.
    let mut h = Harness::inline(&script(
        r##"local value = ui.state("value", 0)
        ui.effect(function()
            anime.animate("#box", { x = value:get(), duration = 500 })
        end)
        anime.createTimer({ duration = 2000, frameRate = 20, onUpdate = function()
            value:set(value:peek() + 1)
        end })"##,
    ));
    h.frame();
    h.run_ms(500.);
    assert!(h.host.warnings().is_empty(), "{:?}", h.host.warnings());
    h.run_ms(1000.);
    let warnings = h.host.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("app:4: this `anime.animate` replaced its own animation"),
        "{warnings:?}"
    );

    // One that finishes before the next change is fine.
    let mut h = Harness::inline(&script(
        r##"local value = ui.state("value", 0)
        ui.effect(function()
            anime.animate("#box", { x = value:get(), duration = 30 })
        end)
        anime.createTimer({ duration = 2000, frameRate = 20, onUpdate = function()
            value:set(value:peek() + 1)
        end })"##,
    ));
    h.frame();
    h.run_ms(2000.);
    assert!(h.host.warnings().is_empty(), "{:?}", h.host.warnings());
}

#[test]
fn keyframes_in_value_and_percentage_forms() {
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = { 0, 100, 50 }, duration = 200, ease = "linear" })"##,
    ));
    h.frame();
    h.run_ms(100.);
    close(h.x("box"), 100., "second value at half");
    h.run_ms(50.);
    close(h.x("box"), 75., "toward the third");

    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", {
            keyframes = { ["0%"] = { x = 0 }, ["50%"] = { x = 100 }, ["100%"] = { x = 0 } },
            duration = 200, ease = "linear",
        })"##,
    ));
    h.frame();
    h.run_ms(50.);
    close(h.x("box"), 50., "a quarter in");
    h.run_ms(100.);
    close(h.x("box"), 50., "three quarters in");

    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = { { to = 40, duration = 100 }, { to = 0, duration = 100 } }, ease = "linear" })"##,
    ));
    h.frame();
    h.run_ms(100.);
    close(h.x("box"), 40., "first keyframe");
    h.run_ms(50.);
    close(h.x("box"), 20., "half of the second");
}

#[test]
fn state_and_table_targets_keep_units_and_modifiers() {
    let mut h = Harness::inline(
        r##"local w = ui.state("w", "0px")
        local n = ui.state("n", 0)
        local seen = ui.state("seen", 0)
        local obj = { v = 0 }
        anime.animate(w, { value = "100px", duration = 100, ease = "linear" })
        anime.animate(n, { value = 1000, modifier = anime.utils.round(0), duration = 100, ease = "linear" })
        anime.animate(obj, { v = 10, duration = 100, ease = "linear", onUpdate = function() seen:set(anime.utils.round(obj.v, 3)) end })
        return ui.column { ui.text { "{w}", id = "w" }, ui.text { "{n}", id = "n" }, ui.text { "{seen}", id = "seen" } }"##,
    );
    h.frame();
    h.run_ms(50.);
    assert_eq!(h.text("w"), "50px");
    assert_eq!(h.text("n"), "500");
    assert_eq!(h.text("seen"), "5");
}

#[test]
fn callbacks_and_then_run_in_order() {
    let mut h = Harness::inline(&script(
        r##"local a = anime.animate("#box", { x = 10, duration = 50,
            onBegin = function() log:set(log:peek() .. "b") end,
            onComplete = function() log:set(log:peek() .. "c") end,
        })
        a:done(function() log:set(log:peek() .. "t") end)"##,
    ));
    h.frame();
    assert_eq!(h.text("log"), "b");
    h.run_ms(100.);
    assert_eq!(h.text("log"), "bct");
}

#[test]
fn playback_controls_seek_reverse_and_revert() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local a = anime.animate("#box", { x = 100, duration = 1000, ease = "linear", autoplay = false })
        return ui.column {
            ui.div { id = "box", w = 10, h = 10 },
            ui.button { id = "seek", w = 10, h = 10, on_click = function() a:seek(250); log:set(tostring(a.currentTime)) end },
            ui.button { id = "back", w = 10, h = 10, on_click = function() a.reverse() end },
            ui.button { id = "revert", w = 10, h = 10, on_click = function() a:revert() end },
            ui.text { "{log}", id = "log" },
        }"##,
    );
    h.frame();
    h.run_ms(100.);
    close(h.x("box"), 0., "not autoplaying");
    h.click("seek");
    close(h.x("box"), 25., "seek renders at once");
    assert_eq!(h.text("log"), "250");
    h.click("back");
    h.frame();
    h.run_ms(100.);
    close(h.x("box"), 15., "playing backwards from 250");
    h.click("revert");
    close(h.x("box"), 0., "reverted");
}

#[test]
fn animatables_utils_refs_and_mount_hooks() {
    let mut h = Harness::inline(
        r##"local follow = anime.createAnimatable("#box", { x = 100, ease = "linear" })
        local r = ui.ref()
        local got = ui.state("got", 0)
        anime.animate(r, { opacity = 0.25, duration = 100, ease = "linear" })
        return ui.column {
            ui.div { id = "box", w = 10, h = 10, on_mount = function() follow.x(50) end },
            ui.div { id = "set", w = 10, h = 10, on_mount = function(el)
                anime.utils.set(el, { x = 30 })
                got:set(anime.utils.get(el, "x"))
            end },
            ui.div { id = "reffed", ref = r, w = 10, h = 10 },
            ui.text { "{got}", id = "got" },
        }"##,
    );
    close(h.x("set"), 30., "utils.set applies at once");
    assert_eq!(h.text("got"), "30");
    h.frame();
    h.run_ms(100.);
    close(h.x("box"), 50., "animatable reached its value");
    close(h.opacity("reffed"), 0.25, "ref target");
}

#[test]
fn colors_interpolate_and_hover_keeps_animated_values() {
    let mut h = Harness::inline(
        r##"anime.animate("#box", { backgroundColor = "#ff0000", x = 40, duration = 100, ease = "linear" })
        return ui.column { ui.div { id = "box", w = 50, h = 50, bg = "#000000", hover = { border = 2 } } }"##,
    );
    h.frame();
    h.run_ms(50.);
    let fill = h.fill("box");
    assert!((fill.0 as i32 - 128).abs() <= 2 && fill.1 == 0, "{fill:?}");
    h.run_ms(60.);
    close(h.x("box"), 40., "done");
    // Hovering re-resolves the node's styles; the animated values stay.
    h.ui.dispatch(InputEvent::PointerMove { x: 60., y: 20. });
    h.ui.prepare_frame();
    close(h.x("box"), 40., "x after hover");
    assert_eq!(h.fill("box"), Color(255, 0, 0, 255));
}

#[test]
fn springs_set_their_own_duration_and_settle() {
    let mut h = Harness::inline(&script(
        r##"local a = anime.animate("#box", { x = 100, ease = anime.createSpring({ stiffness = 200, damping = 20 }) })
        log:set(tostring(math.floor(a.duration)))"##,
    ));
    let duration: f64 = h.text("log").parse().unwrap();
    assert!(
        duration > 200. && duration < 2000.,
        "spring duration {duration}"
    );
    h.frame();
    h.run_ms(duration + 50.);
    close(h.x("box"), 100., "settled");
    assert!(!h.clock.wants_frame());
}

#[test]
fn draggables_follow_the_pointer_then_snap_and_settle() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        anime.createDraggable("#knob", {
            y = false, snap = 50, velocityMultiplier = 0, container = { 0, 200, 0, 0 },
            onGrab = function() log:set(log:peek() .. "g") end,
            onRelease = function() log:set(log:peek() .. "r") end,
            onSettle = function(d) log:set(log:peek() .. "s" .. math.floor(d.x + 0.5)) end,
        })
        return ui.column { ui.div { id = "knob", w = 20, h = 20 }, ui.text { "{log}", id = "log" } }"##,
    );
    let node = h.find("knob").unwrap();
    let b = h.ui.scene.borrow().bounds(node);
    let (x, y) = (b.x + 10., b.y + 10.);
    use zgx::input::PointerButton::Primary;
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: Primary,
    });
    h.ui.dispatch(InputEvent::PointerMove {
        x: x + 60.,
        y: y + 30.,
    });
    close(h.x("knob"), 60., "follows the pointer");
    let node = h.find("knob").unwrap();
    assert_eq!(h.ui.scene.borrow().transform(node).y, 0., "y is locked");
    h.ui.dispatch(InputEvent::PointerMove { x: x + 400., y });
    assert!(
        h.x("knob") > 200. && h.x("knob") < 400.,
        "past the container it gives way: {}",
        h.x("knob")
    );
    h.ui.dispatch(InputEvent::PointerMove { x: x + 60., y });
    h.ui.dispatch(InputEvent::PointerUp {
        x: x + 60.,
        y,
        button: Primary,
    });
    h.frame();
    h.run_ms(1500.);
    close(h.x("knob"), 50., "snapped");
    assert_eq!(h.text("log"), "grs50");
    assert!(!h.clock.wants_frame());
}

#[test]
fn on_scroll_syncs_progress_to_the_scroll_offset() {
    let mut h = Harness::inline(
        r##"local offset = ui.state("offset", 0)
        anime.animate("#t", { opacity = { 0, 1 }, ease = "linear", duration = 100,
            autoplay = anime.onScroll({ container = "#scroller", sync = true }) })
        return ui.column {
            ui.scroll { id = "scroller", offset = offset, w = 100, h = 200,
                ui.column { h = 1500, ui.div { h = 600 }, ui.div { id = "t", w = 50, h = 50 } } },
            ui.button { id = "half", w = 10, h = 10, on_click = ui.set(offset, 525) },
            ui.button { id = "past", w = 10, h = 10, on_click = ui.set(offset, 900) },
        }"##,
    );
    h.ui.prepare_frame();
    h.settle();
    h.click("half");
    // Entering at 600 - 200 = 400, leaving at 600 + 50 = 650.
    close(h.opacity("t") * 100., 50., "half way through");
    h.click("past");
    close(h.opacity("t"), 1., "past it");
    assert!(!h.clock.wants_frame(), "exact sync needs no frames");
}

#[test]
fn motion_paths_text_split_scopes_and_drawn_strokes() {
    let mut h = Harness::inline(
        r##"local path = anime.svg.createMotionPath("M0 0 H100 V100")
        anime.animate("#box", { x = path.translateX, y = path.translateY, rotate = path.rotate, duration = 200, ease = "linear" })
        local scope = anime.createScope({ defaults = { duration = 100, ease = "linear" } })
        local scoped
        scope:add(function() scoped = anime.animate("#s", { x = 10 }) end)
        local drawn = ui.state("drawn", 0.5)
        return ui.column {
            ui.div { id = "box", w = 10, h = 10 },
            ui.div { id = "s", w = 10, h = 10 },
            anime.text.split("Hi there", { words = true, props = { id = "split" } }),
            ui.canvas { id = "line", w = 100, h = 10, draw = function(c, w, h)
                c:stroke(ui.path():move(0, 5):line(w, 5), "#fff", 2, { draw = drawn:get() })
            end },
            ui.button { id = "revert", w = 10, h = 10, on_click = function() scope:revert() end },
        }"##,
    );
    h.frame();
    h.run_ms(50.);
    close(h.x("box"), 50., "a quarter along: half the first edge");
    let node = h.find("box").unwrap();
    assert_eq!(h.ui.scene.borrow().transform(node).y, 0.);
    h.run_ms(100.);
    let y = h.ui.scene.borrow().transform(node).y;
    assert!(
        (h.x("box") - 100.).abs() < 0.6 && (y - 50.).abs() < 0.6,
        "three quarters: down the second edge, {y}"
    );
    // The scope's defaults made it 100 ms long and linear.
    close(h.x("s"), 10., "scoped animation done at 100 ms");
    h.click("revert");
    close(h.x("s"), 0., "scope reverted");
    // Words and the space between them.
    let split = h.find("split").unwrap();
    assert_eq!(h.ui.scene.borrow().children(split).len(), 3);
    let line = {
        let node = h.find("line").unwrap();
        let scene = h.ui.scene.borrow();
        let mut stack = vec![node];
        let mut found = String::new();
        while let Some(n) = stack.pop() {
            let kind = format!("{:?}", scene.kind(n));
            if kind.starts_with("Canvas") {
                found = kind;
                break;
            }
            stack.extend(scene.children(n).iter().copied());
        }
        found
    };
    assert!(
        line.contains("dash: [50.0"),
        "half the line is drawn: {line}"
    );
}

#[test]
fn tilts_in_perspective_animate_as_compositor_transforms() {
    let mut h = Harness::inline(
        r##"anime.animate("#card", { rotateY = { 0, 20 }, rotateX = -8, duration = 100, ease = "linear" })
        return ui.column { ui.div { id = "card", w = 40, h = 60, bg = "#58a6ff", rotate_x = 4, perspective = 1100 } }"##,
    );
    let local = |h: &mut Harness| {
        let node = h.find("card").unwrap();
        h.ui.scene.borrow().local_transform(node)
    };
    let start = local(&mut h);
    assert!(
        (start.rotate_x - 4f32.to_radians()).abs() < 1e-4,
        "rotate_x in degrees"
    );
    assert_eq!(start.perspective, 1100.);
    h.frame();
    h.run_ms(50.);
    let t = local(&mut h);
    assert!(
        (t.rotate_y.to_degrees() - 10.).abs() < 0.5,
        "{}",
        t.rotate_y.to_degrees()
    );
    assert!(
        (t.rotate_x.to_degrees() + 2.).abs() < 0.5,
        "{}",
        t.rotate_x.to_degrees()
    );
    assert_eq!(t.perspective, 1100., "a tilt keeps the literal perspective");
    let node = h.find("card").unwrap();
    assert!(h.ui.scene.borrow().is_layer(node));
}

#[test]
fn rotation_and_scale_animate_as_compositor_transforms() {
    let mut h = Harness::inline(
        r##"anime.animate("#spin", { rotate = "1turn", scale = { 1, 2 }, duration = 100, ease = "linear" })
        return ui.column { ui.div { id = "spin", w = 40, h = 40, bg = "#58a6ff", rotate = 10 } }"##,
    );
    let local = |h: &mut Harness| {
        let node = h.find("spin").unwrap();
        h.ui.scene.borrow().local_transform(node)
    };
    assert!(
        (local(&mut h).rotate - 10f32.to_radians()).abs() < 1e-4,
        "static rotate in degrees"
    );
    h.frame();
    h.run_ms(50.);
    let t = local(&mut h);
    // From 10° toward 360°: half way is 185°.
    assert!(
        (t.rotate.to_degrees() - 185.).abs() < 0.5,
        "{}",
        t.rotate.to_degrees()
    );
    assert!(
        (t.scale.0 - 1.5).abs() < 0.01 && (t.scale.1 - 1.5).abs() < 0.01,
        "{:?}",
        t.scale
    );
    h.run_ms(60.);
    assert!((local(&mut h).rotate.to_degrees() - 360.).abs() < 0.5);
}

#[test]
fn presence_crossfades_old_and_new_content() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "a")
        return ui.column {
            ui.presence(tab, function(v) return ui.text { "content " .. v, id = "c" .. v } end, {
                enter = { opacity = { 0, 1 }, blur = { 6, 0 }, duration = 100, ease = "linear" },
                exit = { opacity = 0, blur = 6, duration = 100, ease = "linear" },
            }),
            ui.button { id = "b", w = 10, h = 10, on_click = ui.set(tab, "b") },
        }"##,
    );
    assert!(h.find("ca").is_some());
    close(h.opacity("ca"), 1., "the first content does not enter");
    h.click("b");
    // The new content starts from its `from` values at once.
    assert!(
        h.find("cb").is_some() && h.find("ca").is_some(),
        "both while changing"
    );
    let wrapper = |h: &mut Harness, id: &str| {
        let node = h.find(id).unwrap();
        let scene = h.ui.scene.borrow();
        let parent = scene.parent(node).unwrap();
        (
            scene.effects(parent).opacity,
            scene.effects(parent).filter_blur,
        )
    };
    let (o, b) = wrapper(&mut h, "cb");
    assert!(
        o < 0.05 && b > 5.,
        "entering starts hidden and blurred: {o} {b}"
    );
    h.frame();
    h.run_ms(50.);
    let (o_in, _) = wrapper(&mut h, "cb");
    let (o_out, b_out) = wrapper(&mut h, "ca");
    assert!(
        (o_in - 0.5).abs() < 0.05 && (o_out - 0.5).abs() < 0.05 && (b_out - 3.).abs() < 0.3,
        "{o_in} {o_out} {b_out}"
    );
    h.run_ms(70.);
    assert!(
        h.find("ca").is_none(),
        "the old content unmounts after its exit"
    );
    close(wrapper(&mut h, "cb").0, 1., "entered");
}

#[test]
fn presence_can_wait_for_the_exit() {
    let mut h = Harness::inline(
        r##"local tab = ui.state("tab", "a")
        return ui.column {
            ui.presence(tab, function(v) return ui.text { v, id = v } end, {
                mode = "wait", exit = { opacity = 0, duration = 100 },
                enter = { opacity = { 0, 1 }, duration = 100 },
            }),
            ui.button { id = "go", w = 10, h = 10, on_click = ui.set(tab, "b") },
        }"##,
    );
    h.click("go");
    assert!(h.find("b").is_none(), "waits");
    h.frame();
    h.run_ms(120.);
    assert!(h.find("a").is_none() && h.find("b").is_some());
}

#[test]
fn layout_boxes_spring_to_their_content() {
    let mut h = Harness::inline(
        r##"local rows = ui.state("rows", { "one" })
        return ui.column {
            ui.column { id = "box", layout = "height", layout_ease = "linear", layout_duration = 100,
                ui.each(rows, function(r) return ui.div { h = 20, w = 50 } end) },
            ui.div { id = "below", w = 10, h = 10 },
            ui.button { id = "add", w = 10, h = 10, on_click = function() rows:push("two") end },
        }"##,
    );
    let y = |h: &mut Harness| {
        let node = h.find("below").unwrap();
        h.ui.scene.borrow().bounds(node).y
    };
    h.ui.prepare_frame();
    h.settle();
    h.ui.prepare_frame();
    let start = y(&mut h);
    assert_eq!(start, 20., "the box holds its content's height");
    h.click("add");
    h.ui.prepare_frame();
    h.settle();
    assert_eq!(y(&mut h), 20., "no jump on the frame the content grows");
    h.frame();
    h.run_ms(50.);
    h.ui.prepare_frame();
    let mid = y(&mut h);
    assert!(mid > 25. && mid < 35., "half way: {mid}");
    h.run_ms(70.);
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 40.);
    assert!(!h.clock.wants_frame());
}

#[test]
fn layout_boxes_take_a_reflow_at_once() {
    // Narrowing the box wraps its row onto a second line: the height changed
    // because the width did, not the content, so it is not animated.
    let mut h = Harness::inline(
        r##"local width = ui.state("width", 80)
        local cells = {}
        for i = 1, 4 do table.insert(cells, ui.div { w = 20, h = 20, shrink = 0 }) end
        return ui.column {
            ui.column { id = "box", w = width, layout = "height", layout_ease = "linear", layout_duration = 100,
                ui.row { w_full = true, flex_wrap = true, cells } },
            ui.div { id = "below", w = 10, h = 10 },
            ui.button { id = "narrow", w = 10, h = 10, on_click = function() width:set(40) end },
        }"##,
    );
    let y = |h: &mut Harness| {
        let node = h.find("below").unwrap();
        h.ui.scene.borrow().bounds(node).y
    };
    h.ui.prepare_frame();
    h.settle();
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 20., "one row of cells");
    h.click("narrow");
    h.ui.prepare_frame();
    h.settle();
    h.frame();
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 40., "the reflowed height applies at once");
    h.run_ms(50.);
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 40., "and stays");
}

#[test]
fn a_reflow_leaves_timers_running() {
    // A reflow drops the box's own animation; a timer, which animates no
    // element, is not the box's and still completes.
    let mut h = Harness::inline(
        r##"local width = ui.state("width", 80)
        local done = ui.state("done", false)
        anime.createTimer { duration = 200, onComplete = function() done:set(true) end }
        local cells = {}
        for i = 1, 4 do table.insert(cells, ui.div { w = 20, h = 20, shrink = 0 }) end
        return ui.column {
            ui.column { id = "box", w = width, layout = "height", layout_ease = "linear", layout_duration = 100,
                ui.row { w_full = true, flex_wrap = true, cells } },
            ui.button { id = "narrow", w = 10, h = 10, on_click = function() width:set(40) end },
            ui.show(function() return done:get() end, ui.div { id = "done", w = 1, h = 1 }),
        }"##,
    );
    h.ui.prepare_frame();
    h.settle();
    h.frame();
    h.click("narrow");
    h.ui.prepare_frame();
    h.settle();
    h.frame();
    h.run_ms(300.);
    h.ui.prepare_frame();
    assert!(h.find("done").is_some(), "the timer completed");
}

#[test]
fn layout_boxes_still_ease_content_that_widens_them() {
    // A box sized by its content (a bubble) that gains a wider line grows in
    // both directions: that is new content, not a reflow, so it eases.
    let mut h = Harness::inline(
        r##"local more = ui.state("more", false)
        local function cells(n)
            local out = {}
            for _ = 1, n do table.insert(out, ui.div { w = 20, h = 20, shrink = 0 }) end
            return ui.row(out)
        end
        return ui.column {
            ui.column { id = "box", layout = "height", layout_ease = "linear", layout_duration = 100,
                cells(1),
                ui.show(function() return more:get() end, cells(2)) },
            ui.div { id = "below", w = 10, h = 10 },
            ui.button { id = "more", w = 10, h = 10, on_click = function() more:set(true) end },
        }"##,
    );
    let y = |h: &mut Harness| {
        let node = h.find("below").unwrap();
        h.ui.scene.borrow().bounds(node).y
    };
    h.ui.prepare_frame();
    h.settle();
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 20.);
    h.click("more");
    h.ui.prepare_frame();
    h.settle();
    h.frame();
    h.run_ms(50.);
    h.ui.prepare_frame();
    let mid = y(&mut h);
    assert!(mid > 20. && mid < 40., "eases toward 40, at {mid}");
    h.run_ms(100.);
    h.ui.prepare_frame();
    assert_eq!(y(&mut h), 40.);
}

#[test]
fn capped_clips_ask_for_frames_only_at_their_rate() {
    let mut h = Harness::inline(
        r##"anime.animate("#d", { x = { 0, 100 }, duration = 1000, loop = true, frameRate = 12 })
        return ui.div { id = "d", w = 10, h = 10 }"##,
    );
    h.frame();
    assert_eq!(
        h.clock.demanded_rate(120.),
        Some(12.),
        "a 12 fps loop asks for 12 fps"
    );
    let _ = h.find("d");
}

#[test]
fn animating_scale_keeps_a_static_rotation() {
    let mut h = Harness::inline(
        r##"anime.animate("#early", { scale = { 0.5, 1 }, duration = 100 })
        return ui.column {
            ui.div { id = "early", w = 10, h = 10, rotate = 10 },
            ui.div { id = "d", w = 10, h = 10, rotate = 10, on_mount = function(el)
                anime.animate(el, { scale = { 0.8, 1 }, y = { 6, 0 }, duration = 100 })
            end },
        }"##,
    );
    h.frame();
    h.run_ms(200.);
    for id in ["d", "early"] {
        let node = h.find(id).unwrap();
        let t = h.ui.scene.borrow().local_transform(node);
        close(
            t.rotate,
            10f32.to_radians(),
            &format!("{id} keeps its rotation"),
        );
        close(t.scale.0, 1., &format!("{id} scaled back"));
    }
}

#[test]
fn clean_inline_styles_can_forget_one_property() {
    let mut h = Harness::inline(
        r##"return ui.div { id = "d", w = 10, h = 10, opacity = 0.5, on_mount = function(el)
            anime.animate(el, { opacity = 0.2, x = 30, duration = 50, onComplete = function()
                anime.utils.cleanInlineStyles(el, "x")
            end })
        end }"##,
    );
    h.frame();
    h.run_ms(100.);
    let node = h.find("d").unwrap();
    let scene = h.ui.scene.borrow();
    close(scene.transform(node).x, 0., "x returns to the node's own");
    close(scene.effects(node).opacity, 0.2, "opacity stays animated");
    drop(scene);
    let mut h = Harness::inline(
        r##"return ui.div { id = "d", w = 10, h = 10, opacity = 0.5, on_mount = function(el)
            anime.animate(el, { opacity = 0.2, duration = 50, onComplete = function()
                anime.utils.cleanInlineStyles(el)
            end })
        end }"##,
    );
    h.frame();
    h.run_ms(100.);
    let node = h.find("d").unwrap();
    close(
        h.ui.scene.borrow().effects(node).opacity,
        0.5,
        "all: back to its literal opacity",
    );
}

#[test]
fn single_corners_animate_and_keep_the_others() {
    let mut h = Harness::inline(
        r##"return ui.div { id = "d", w = 40, h = 40, bg = "#fff", rounded = 16, on_mount = function(el)
            anime.animate(el, { borderTopLeftRadius = 0, rounded_br = 4, duration = 100, ease = "linear" })
        end }"##,
    );
    h.frame();
    h.run_ms(50.);
    let corners = |h: &mut Harness| {
        let node = h.find("d").unwrap();
        match h.ui.scene.borrow().kind(node) {
            zgx::scene::NodeKind::Panel { quad, .. } | zgx::scene::NodeKind::Quad(quad) => quad
                .decoration
                .as_ref()
                .and_then(|d| d.corners)
                .expect("per-corner radii"),
            other => panic!("{other:?}"),
        }
    };
    let mid = corners(&mut h);
    close(mid.top_left, 8., "half way to 0");
    close(mid.bottom_right, 10., "half way to 4");
    close(
        mid.top_right,
        16.,
        "untouched corners keep the literal radius",
    );
    h.run_ms(80.);
    let end = corners(&mut h);
    close(end.top_left, 0., "done");
    close(end.bottom_left, 16., "kept");
}

#[test]
fn animations_started_from_on_resize_play() {
    let mut h = Harness::inline(
        r##"local show = ui.state("show", false)
        return ui.column {
            ui.column { w = 100, h = 20, on_resize = function(size)
                anime.animate("#a", { opacity = { 0, 1 }, duration = 200 })
                show:set(true)
            end },
            ui.column { id = "a", w = 10, h = 10, opacity = 0 },
            ui.show(show, ui.column { id = "b", w = 10, h = 10, opacity = 0, on_mount = function(el)
                anime.animate(el, { opacity = { 0, 1 }, duration = 200 })
            end }),
        }"##,
    );
    for _ in 0..60 {
        h.frame();
    }
    close(
        h.opacity("a"),
        1.,
        "an animation started in on_resize plays",
    );
    close(
        h.opacity("b"),
        1.,
        "a node mounted by on_resize's state plays its on_mount animation",
    );
}

#[test]
fn clip_insets_animate_in_points_and_percent() {
    let mut h = Harness::inline(
        r##"return ui.column { id = "d", w = 100, h = 40, clip_inset = { -20, -20, -20, -20 }, on_mount = function(el)
            anime.animate(el, { clipTop = 0, clipRight = 0, clipLeft = 0, clipBottom = "100%", duration = 100, ease = "linear" })
        end }"##,
    );
    h.frame();
    h.run_ms(50.);
    let inset = |h: &mut Harness| {
        let node = h.find("d").unwrap();
        let c = h.ui.scene.borrow().clip_inset(node).expect("an inset");
        let px = |l: zgx::layout::Length| match l {
            zgx::layout::Length::Px(v) => v,
            other => panic!("{other:?}"),
        };
        [px(c.top), px(c.right), px(c.bottom), px(c.left)]
    };
    let mid = inset(&mut h);
    close(mid[0], -10., "half way from -20 to 0");
    close(mid[2], 10., "half way from -20 to 100% of 40");
    h.run_ms(80.);
    let end = inset(&mut h);
    close(end[2], 40., "closed to its top edge");
    close(end[1], 0., "right side");
}

#[test]
fn shader_fills_take_params_and_animate_their_uniforms() {
    let mut h = Harness::inline(
        r##"local fill = ui.shader("fn fill(input: FillInput) -> vec4<f32> { return input.params[0]; }", { fps = 30 })
        return ui.div { id = "d", w = 40, h = 40, bg_shader = fill, shader_params = { "#ff000080", { 1, 2, 3, 4 } },
            on_mount = function(el)
                anime.animate(el, { shaderTime = { 0, 2 }, shaderParam4 = { 1, 3 }, duration = 100, ease = "linear" })
            end }"##,
    );
    let node = h.find("d").unwrap();
    let text = format!("{:?}", h.ui.scene.borrow().kind(node));
    assert!(
        text.contains("Shader("),
        "the box is filled by the shader: {text}"
    );
    let start = h.ui.scene.borrow().shader_uniforms(node);
    close(start.params[0][0], 1., "a color parameter is 0 to 1");
    close(start.params[0][3], 128. / 255., "with its alpha");
    h.frame();
    h.run_ms(50.);
    let mid = h.ui.scene.borrow().shader_uniforms(node);
    close(mid.time, 1., "time half way");
    close(
        mid.params[1][0],
        2.,
        "shaderParam4 is the second vec4's first value",
    );
    close(mid.params[1][1], 2., "the rest keep their literal values");
}

#[test]
fn blend_names_a_mode_and_follows_state() {
    let mut h = Harness::inline(
        r##"local mode = ui.state("mode", "soft-light")
        return ui.column {
            ui.div { id = "d", w = 20, h = 20, bg = "#336699", blend = mode },
            ui.button { id = "b", label = "Swap", on_click = function() mode:set("difference") end },
        }"##,
    );
    let node = h.find("d").unwrap();
    assert_eq!(
        h.ui.scene.borrow().blend(node),
        zgx::scene::BlendMode::SoftLight
    );
    assert!(
        h.ui.scene.borrow().is_layer(node),
        "a blended node is a layer"
    );
    h.click("b");
    h.frame();
    assert_eq!(
        h.ui.scene.borrow().blend(node),
        zgx::scene::BlendMode::Difference
    );
}

#[test]
fn layer_shaders_take_params_and_animate_their_uniforms() {
    let mut h = Harness::inline(
        r##"local sheen = ui.layer_shader("fn layer(input: LayerInput) -> vec4<f32> { return input.color * input.params[0]; }")
        return ui.div { id = "d", w = 40, h = 40, bg = "#336699", layer_shader = sheen, layer_params = { "#ffffff80", { 1, 2, 3, 4 } },
            on_mount = function(el)
                anime.animate(el, { layerTime = { 0, 2 }, layerParam4 = { 1, 3 }, duration = 100, ease = "linear" })
            end }"##,
    );
    let node = h.find("d").unwrap();
    assert!(
        h.ui.scene.borrow().is_layer(node),
        "a layer shader caches the subtree as a layer"
    );
    let start = h.ui.scene.borrow().layer_shader(node).cloned().unwrap();
    assert!(start.shader.is_some());
    close(
        start.uniforms.params[0][0],
        1.,
        "a color parameter is 0 to 1",
    );
    close(start.uniforms.params[0][3], 128. / 255., "with its alpha");
    h.frame();
    h.run_ms(50.);
    let mid = h.ui.scene.borrow().layer_shader(node).unwrap().uniforms;
    close(mid.time, 1., "time half way");
    close(
        mid.params[1][0],
        2.,
        "layerParam4 is the second vec4's first value",
    );
    close(mid.params[1][1], 2., "the rest keep their literal values");
}

#[test]
fn color_filters_animate_on_the_compositor() {
    let mut h = Harness::inline(
        r##"return ui.div { id = "d", w = 20, h = 20, bg = "#336699", saturate = 0.5, on_mount = function(el)
            anime.animate(el, { brightness = { 1, 1.2 }, duration = 100, ease = "linear", onComplete = function()
                anime.utils.cleanInlineStyles(el, "saturate")
            end })
        end }"##,
    );
    h.frame();
    h.run_ms(50.);
    let node = h.find("d").unwrap();
    let e = h.ui.scene.borrow().effects(node);
    close(e.brightness, 1.1, "half way");
    close(
        e.saturate,
        0.5,
        "the literal saturate holds while brightness animates",
    );
    h.run_ms(80.);
    let e = h.ui.scene.borrow().effects(node);
    close(e.brightness, 1.2, "done");
}

#[test]
fn a_loop_no_one_can_see_waits_until_it_shows() {
    // A spinner inside a dialog opened once and closed: kept, unseen.
    let mut h = Harness::inline(
        r##"local open = ui.state("open", true)
        return ui.column {
            ui.button { id = "b", w = 20, h = 20, on_click = function() open:set(not open:peek()) end, "b" },
            ui.modal { open = open, w = 100, h = 100,
                ui.div { id = "spin", w = 10, h = 10, bg = "#ff0000",
                    on_mount = function(el)
                        anime.animate(el, { x = { 0, 100 }, duration = 1000, loop = true, frameRate = 30 })
                    end } } }"##,
    );
    h.frame();
    h.run_ms(100.);
    // The open dialog's backdrop takes the click and closes it.
    h.click("b");
    h.frame();
    let x = h.x("spin");
    h.run_ms(300.);
    assert_eq!(h.x("spin"), x, "unseen, it stands still");
    assert_eq!(
        h.clock.demanded_rate(120.),
        Some(4.),
        "and the engine only looks now and then"
    );
    h.click("b");
    h.run_ms(300.);
    assert!(h.x("spin") != x, "open, it moves: {}", h.x("spin"));
    assert_eq!(h.clock.demanded_rate(120.), Some(30.));
}

#[test]
fn a_loop_in_a_popup_past_the_window_runs_on() {
    // A popover opened near the window's bottom right, where the host shows
    // it in a window of its own: its panel hangs past this one's edges.
    let mut h = Harness::inline(
        r##"local open = ui.state("open", false)
        local SH = ui.shader([[fn fill(input: FillInput) -> vec4<f32> {
          return vec4<f32>(fract(input.time), 0.4, 1.0, 1.0);
        }]], { fps = 12 })
        return ui.div { absolute = true, right = 8, bottom = 8,
          ui.popover { open = open, w = 240,
            anchor = ui.button { "Open", id = "open", on_click = function() open:set(true) end },
            placement = { side = "bottom", align = "start" },
            build = function()
              return ui.div { id = "spin", w = 200, h = 40, bg_shader = SH,
                on_mount = function(el)
                  anime.animate(el, { shaderTime = { 0, 600 }, x = { 0, 100 }, duration = 600000,
                    loop = true, ease = "linear", frameRate = 12 })
                end }
            end } }"##,
    );
    h.ui.set_popup_screens(vec![zgx::scene::Rect::new(-1e5, -1e5, 2e5, 2e5)]);
    h.click("open");
    h.frames(2);
    let spin = h.find("spin").expect("open");
    assert!(
        !h.ui.scene.borrow().popups().is_empty(),
        "shown in a popup window"
    );
    let time = |h: &mut Harness| h.ui.scene.borrow().shader_uniforms(spin).time;
    let (t, x) = (time(&mut h), h.x("spin"));
    h.run_ms(500.);
    assert!(
        time(&mut h) > t,
        "its shader's time runs on: {}",
        time(&mut h)
    );
    assert!(h.x("spin") > x, "and it moves: {}", h.x("spin"));
    assert_eq!(h.clock.demanded_rate(120.), Some(12.), "at its own rate");
}

#[test]
fn a_fade_and_a_spring_in_a_popup_past_the_window_play_and_its_window_draws() {
    let mut h = Harness::inline(
        r##"local open = ui.state("open", false)
        local at = ui.state("at", 0)
        return ui.div { absolute = true, right = 8, bottom = 8,
          ui.popover { open = open, w = 240,
            anchor = ui.button { "Open", id = "open", on_click = function() open:set(true) end },
            placement = { side = "bottom", align = "start" },
            build = function()
              return ui.column {
                ui.div { id = "fade", w = 200, h = 20, bg = "#ff0000",
                  on_mount = function(el) anime.animate(el, { opacity = { 0, 1 }, y = { 6, 0 }, duration = 550 }) end },
                ui.div { id = "spring", w = 20, h = 20, bg = "#00ff00", x = ui.spring(at) },
                ui.button { "Move", id = "move", on_click = function() at:set(100) end } }
            end } }"##,
    );
    h.ui.set_popup_screens(vec![zgx::scene::Rect::new(-1e5, -1e5, 2e5, 2e5)]);
    h.click("open");
    h.frames(2);
    let root = h.ui.scene.borrow().popups()[0];
    let revision = h.ui.scene.borrow().layer_revision(root);
    let opacity = h.opacity("fade");
    h.run_ms(200.);
    assert!(
        h.opacity("fade") > opacity,
        "it fades in: {}",
        h.opacity("fade")
    );
    h.run_ms(500.);
    assert_eq!(h.opacity("fade"), 1., "and ends");
    assert_ne!(h.ui.scene.borrow().layer_revision(root), revision);
    let revision = h.ui.scene.borrow().layer_revision(root);
    h.click("move");
    h.run_ms(100.);
    let x = h.x("spring");
    assert!(x > 0. && x < 100., "it springs: {x}");
    assert_eq!(h.ui.scene.borrow().popups(), [root], "still open");
    assert_ne!(h.ui.scene.borrow().layer_revision(root), revision);
}

#[test]
fn a_delayed_animation_takes_over_where_an_earlier_one_left_its_property() {
    // Made together: the second, delayed, starts from where the first left
    // `x`, not from where `x` was when both were made (anime.js v4's replace).
    let mut h = Harness::inline(&script(
        r##"anime.animate("#box", { x = { 0, 100 }, duration = 1000, ease = "linear" })
        anime.animate("#box", { x = 300, delay = 1500, duration = 1000, ease = "linear" })"##,
    ));
    h.frame();
    h.run_ms(500.);
    close(h.x("box"), 50., "the first half way");
    h.run_ms(500.);
    close(h.x("box"), 100., "the first done");
    h.run_ms(500.);
    close(h.x("box"), 100., "held until the second starts");
    h.run_ms(500.);
    close(h.x("box"), 200., "the second half way, from 100");
}
