//! `hit = "target"`: while an animation moves a node, it takes clicks where
//! it's going. Clicks land mid-animation, as a person clicking fast does.
mod common;

use common::Harness;
use zgx::scene::Rect;

impl Harness {
    /// A click at `(x, y)` in the window, whatever is there.
    fn click_point(&mut self, x: f32, y: f32) {
        zgx_script::driver::click_at(&mut self.ui, Rect::new(x - 0.5, y - 0.5, 1., 1.));
        self.frames(1);
    }
}

/// A card tweening to x = 200 over a second, and what it heard.
fn tweened(hit: &str) -> String {
    format!(
        r##"local at = ui.state("at", 0)
        local log = ui.state("log", "")
        ui.command {{ id = "move", run = function() at:set(200) end }}
        return ui.overlay {{ w = 400, h = 100,
            ui.div {{ id = "card", absolute = true, left = 0, top = 0, w = 100, h = 40,
                bg = "#336699", hit = "{hit}",
                x = ui.tween(at, {{ duration = 1000, ease = "linear" }}),
                on_click = function() log:set(log:get() .. "card ") end }},
            ui.text {{ id = "log", w = 1, h = 1, "{{log}}" }},
        }}"##
    )
}

#[test]
fn a_tweening_card_takes_clicks_where_it_is_going() {
    let mut h = Harness::inline(&tweened("target"));
    h.frames(2);
    assert!(h.host.run_command("move"));
    h.frames(6);
    // A tenth of the way: it shows near 20, it's going to 200.
    h.click_point(250., 20.);
    assert_eq!(h.text("log"), "card ");
    h.click_point(50., 20.);
    assert_eq!(h.text("log"), "card ", "not where it shows");
    assert_eq!(h.error(), None);
}

#[test]
fn shown_by_default_it_takes_clicks_where_it_shows() {
    let mut h = Harness::inline(&tweened("shown"));
    h.frames(2);
    assert!(h.host.run_command("move"));
    h.frames(6);
    h.click_point(250., 20.);
    assert_eq!(h.text("log"), "");
    h.click_point(50., 20.);
    assert_eq!(h.text("log"), "card ");
    assert_eq!(h.error(), None);
}

/// The photo stack: × removes a photo, and the rest slide to close the gap.
/// Clicking × in one place as fast as frames come removes one photo each
/// time, in order, though none has finished sliding.
#[test]
fn spam_clicking_a_stack_that_refans_removes_the_photo_under_the_pointer() {
    let code = |hit: &str| {
        format!(
            r##"local photos = ui.state("photos", {{ 1, 2, 3, 4 }})
            local log = ui.state("log", "")
            return ui.column {{ w = 400, h = 200,
                ui.row {{ id = "stack", gap = 10, h = 60,
                    ui.each(photos, function(photo)
                        return ui.div {{ id = "photo_" .. photo, w = 80, h = 60,
                            bg = "#336699", hit = "{hit}",
                            on_click = function()
                                log:set(log:get() .. photo .. " ")
                                photos:update(function(list)
                                    local kept = {{}}
                                    for _, p in list do if p ~= photo then table.insert(kept, p) end end
                                    return kept
                                end)
                            end }}
                    end, {{
                        key = function(photo) return photo end,
                        animate = {{ duration = 600, ease = "linear" }},
                        exit = {{ opacity = 0, duration = 300 }},
                    }}),
                }},
                ui.text {{ id = "log", w = 1, h = 1, "{{log}}" }},
            }}"##
        )
    };
    // At the first photo's place, a click a frame for four frames.
    let spam = |hit: &str| {
        let mut h = Harness::inline(&code(hit));
        h.frames(2);
        for _ in 0..4 {
            h.click_point(40., 30.);
            h.frames(1);
        }
        assert_eq!(h.error(), None);
        h.text("log")
    };
    assert_eq!(spam("target"), "1 2 3 4 ", "each click removes the next");
    assert_ne!(
        spam("shown"),
        "1 2 3 4 ",
        "where they show, the next photo hasn't slid under the pointer yet"
    );
}

/// A driver's click by id lands where the node takes clicks.
#[test]
fn a_click_by_id_lands_where_it_takes_clicks() {
    let mut h = Harness::inline(&tweened("target"));
    h.frames(2);
    assert!(h.host.run_command("move"));
    h.frames(6);
    h.click("card");
    assert_eq!(h.text("log"), "card ");
    assert_eq!(h.error(), None);
}

#[test]
fn hit_takes_target_or_shown() {
    let error = zgx_script::testing::TestApp::try_new(|runtime| {
        zgx_script::ScriptHost::inline(runtime, "app", r#"return ui.div { hit = "soon" }"#)
    })
    .err()
    .expect("an error");
    assert!(error.contains("`hit` takes"), "{error}");
}

/// Each way a card can move there: a spring on `x`, and a tween on an
/// absolute card's `left` (a new layout each frame, no second one).
#[test]
fn a_spring_and_a_tweened_left_take_clicks_where_they_go() {
    for moved in [
        "x = ui.spring(at)",
        r#"left = ui.tween(at, { duration = 1000, ease = "linear" })"#,
    ] {
        let mut h = Harness::inline(&format!(
            r##"local at = ui.state("at", 0)
            local log = ui.state("log", "")
            ui.command {{ id = "move", run = function() at:set(200) end }}
            return ui.overlay {{ w = 400, h = 100,
                ui.div {{ id = "card", absolute = true, top = 0, w = 100, h = 40,
                    hit = "target", {moved},
                    on_click = function() log:set(log:get() .. "card ") end }},
                ui.text {{ id = "log", w = 1, h = 1, "{{log}}" }},
            }}"##
        ));
        h.frames(2);
        assert!(h.host.run_command("move"));
        h.frames(3);
        h.click_point(250., 20.);
        assert_eq!(h.text("log"), "card ", "{moved}");
        assert_eq!(h.error(), None);
    }
}

/// `anime` moving a card: it takes clicks where the animation ends, from
/// the frame it starts; cancelled mid-way, where it stopped.
#[test]
fn an_anime_animation_hits_at_its_end_and_where_a_cancel_leaves_it() {
    let mut h = Harness::inline(
        r##"local log = ui.state("log", "")
        local running = nil
        ui.command { id = "go", run = function()
            running = anime.animate("#card", { x = 200, duration = 1000, ease = "linear" })
        end }
        ui.command { id = "stop", run = function() running:cancel() end }
        return ui.overlay { w = 400, h = 100,
            ui.div { id = "card", absolute = true, left = 0, top = 0, w = 100, h = 40,
                hit = "target",
                on_click = function() log:set(log:get() .. "card ") end },
            ui.text { id = "log", w = 1, h = 1, "{log}" },
        }"##,
    );
    h.frames(2);
    assert!(h.host.run_command("go"));
    h.frames(6);
    h.click_point(250., 20.);
    assert_eq!(h.text("log"), "card ", "where it's going");
    h.click_point(50., 20.);
    assert_eq!(h.text("log"), "card ", "not where it shows");
    assert!(h.host.run_command("stop"));
    h.frames(2);
    // Stopped about a tenth of the way (near 20): it takes clicks where it
    // shows, which its style's place (0 to 100) doesn't reach.
    h.click_point(110., 20.);
    assert_eq!(h.text("log"), "card card ");
    h.click_point(250., 20.);
    assert_eq!(h.text("log"), "card card ", "nor where it was going");
    assert_eq!(h.error(), None);
}

/// The photo stack, with `anime` moving each photo to its place in the fan.
#[test]
fn spam_clicking_a_stack_fanned_by_anime_removes_the_photo_under_the_pointer() {
    let mut h = Harness::inline(
        r##"local photos = ui.state("photos", { 1, 2, 3, 4 })
        local log = ui.state("log", "")
        local function fan()
            for i, photo in photos:get() do
                anime.animate("#photo_" .. photo, { x = (i - 1) * 90, duration = 600, ease = "linear" })
            end
        end
        return ui.overlay { w = 400, h = 100,
            ui.each(photos, function(photo)
                return ui.div { id = "photo_" .. photo, absolute = true, left = 0, top = 0,
                    w = 80, h = 60, hit = "target",
                    x = (photo - 1) * 90,
                    on_click = function()
                        log:set(log:get() .. photo .. " ")
                        photos:update(function(list)
                            local kept = {}
                            for _, p in list do if p ~= photo then table.insert(kept, p) end end
                            return kept
                        end)
                        fan()
                    end }
            end, { key = function(photo) return photo end }),
            ui.text { id = "log", w = 1, h = 1, "{log}" },
        }"##,
    );
    h.frames(2);
    for _ in 0..4 {
        h.click_point(40., 30.);
        h.frames(1);
    }
    assert_eq!(h.text("log"), "1 2 3 4 ");
    assert_eq!(h.error(), None);
}
