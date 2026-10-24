//! The examples' scripts, played headless start to end.
mod common;
use common::Harness;
use std::path::Path;
use zgx_script::ScriptHost;

fn example(name: &str) -> Harness {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("examples/{name}/app.luau"));
    let h = Harness::new(|runtime| ScriptHost::new(runtime, path));
    h.ui.scene.borrow_mut().resize(720., 1000.);
    h
}
fn ok(h: &Harness) {
    if let Some(e) = h.host.error().with_untracked(|e| e.clone()) {
        panic!("script error: {e}");
    }
}

#[test]
fn agent_trace_plays_through_and_comes_to_rest() {
    let mut h = example("agent_trace");
    // Connecting, then steps until the run asks for approval.
    let mut frames = 0;
    while h.find("approve").is_none() {
        h.frame();
        ok(&h);
        frames += 1;
        assert!(frames < 60 * 60, "never reached the approval");
    }
    // Let it dock before pressing it.
    for _ in 0..60 {
        h.frame();
    }
    h.click("approve");
    for _ in 0..60 * 5 {
        h.frame();
        ok(&h);
    }
    assert!(h.find("approve").is_none(), "the approval leaves");
    // The answer's picks: a description that wraps pushes the next pick down.
    h.ui.prepare_frame();
    let texts = |h: &Harness| {
        let scene = h.ui.scene.borrow();
        let mut found = Vec::new();
        let mut stack = vec![scene.root()];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n) {
                found.push((text.to_string(), scene.bounds(n)));
            }
            stack.extend(scene.children(n).iter().copied());
        }
        found
    };
    let all = texts(&h);
    let find = |prefix: &str| {
        all.iter()
            .find(|(t, _)| t.starts_with(prefix))
            .map(|(_, b)| *b)
    };
    let why = find("Cordless, but").expect("the Ninja's description");
    let next = find("NutriBullet 600").expect("the next pick");
    assert!(
        why.height > 40.,
        "the description wraps here, so the check means something: {why:?}"
    );
    assert!(
        next.y >= why.y + why.height - 0.5,
        "the next pick starts below the wrapped text: {why:?} {next:?}"
    );
    assert!(
        !h.clock.wants_frame(),
        "nothing animates once the run is done"
    );
}

#[test]
#[ignore]
fn dump_agent_trace_layout() {
    let h = example("agent_trace");
    let mut worst = (0., 0);
    for i in 0..60 * 9 {
        let start = std::time::Instant::now();
        h.frame();
        h.ui.prepare_frame();
        let ms = start.elapsed().as_secs_f64() * 1000.;
        if ms > worst.0 {
            worst = (ms, i);
        }
        if i % 30 == 0 || ms > 20. {
            println!(
                "t={:.1}s frame {:.2} ms nodes {}",
                i as f64 / 60.,
                ms,
                h.ui.scene.borrow().len()
            );
        }
    }
    println!("worst {:.2} ms at frame {}", worst.0, worst.1);
    let scene = h.ui.scene.borrow();
    fn walk(scene: &zgx::scene::Scene, n: zgx::scene::NodeId, depth: usize) {
        let b = scene.bounds(n);
        let kind = format!("{:?}", scene.kind(n));
        if depth < 60
            && (b.width <= 1.5
                || kind.contains("Text { text: \"B")
                || kind.contains("Text { text: \"T")
                || kind.contains("Text { text: \"I'll"))
        {
            println!(
                "{:.1}x{:.1} @{:.0},{:.1} op={} {}",
                b.width,
                b.height,
                b.x,
                b.y,
                scene.effects(n).opacity,
                &kind[..kind.len().min(40)]
            );
        }
        for &c in scene.children(n) {
            walk(scene, c, depth + 1);
        }
    }
    walk(&scene, scene.root(), 0);
}

#[test]
#[ignore]
fn dump_presence_width() {
    let h = Harness::inline(
        r##"local open = ui.state("o", true)
        local function col(t) t.items = t.items or "stretch"; return ui.column(t) end
        return col { w = 300,
            ui.presence(function() return open:get() and "a" or nil end, function()
                return col { ui.text { "a long line of text that should wrap inside three hundred points of width, really it should", wrap = true } }
            end, { layout = "height" }),
            ui.presence(function() return open:get() and "a" or nil end, function()
                return col { ui.text { "a long line of text that should wrap inside three hundred points of width, really it should", wrap = true } }
            end),
            col { layout = "height", ui.text { "a long line of text that should wrap inside three hundred points of width, really it should", wrap = true } },
            col { gap = 14, ui.presence(function() return open:get() and "a" or nil end, function()
                return col { ui.each(ui.state("l4", { "x" }), function() return col { layout = "height", layout_enter = true, col { opacity = 0, ui.row { ui.div { w = 10 }, ui.row { grow = 1, shrink = 1, min_w = 0, flex_wrap = true, gap = 4, ui.div { w = 120, h = 10 }, ui.div { w = 120, h = 10 }, ui.div { w = 120, h = 10 } } } } } end) }
            end, { layout = "height" }) },
            col { ui.each(ui.state("l3", { "x" }), function() return col { layout = "height", layout_enter = true, col { opacity = 0, ui.row { ui.div { w = 10 }, ui.row { grow = 1, shrink = 1, min_w = 0, flex_wrap = true, gap = 4, ui.div { w = 120, h = 10 }, ui.div { w = 120, h = 10 }, ui.div { w = 120, h = 10 } } } } } end) },
            col { ui.each(ui.state("l2", { "x" }), function() return ui.row { ui.div { w = 10 }, col { grow = 1, shrink = 1, min_w = 0, anime.text.split("a long line of text that should wrap inside three hundred points of width, really it should", { words = { class = "w" } }) } } end) },
            col { ui.each(ui.state("l", { "x" }), function() return col { ui.text { "a long line of text that should wrap inside three hundred points of width, really it should", wrap = true } } end) },
            col { ui.each(ui.state("l", { "x" }), function() return ui.row { ui.div { w = 10 }, col { grow = 1, shrink = 1, min_w = 0, ui.text { "a long line of text that should wrap inside three hundred points of width, really it should", wrap = true } } } end) },
        }"##,
    );
    h.ui.prepare_frame();
    let scene = h.ui.scene.borrow();
    fn walk(scene: &zgx::scene::Scene, n: zgx::scene::NodeId, depth: usize) {
        let b = scene.bounds(n);
        let kind = format!("{:?}", scene.kind(n));
        println!(
            "{}{:.0}x{:.0} {}",
            " ".repeat(depth),
            b.width,
            b.height,
            &kind[..kind.len().min(40)]
        );
        for &c in scene.children(n) {
            walk(scene, c, depth + 1);
        }
    }
    walk(&scene, scene.root(), 0);
}

fn composer_states() -> Harness {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/composer_states");
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::new(runtime, dir.join("app.luau"));
        host.action("drag_window", |_| {});
        if let Some(n) = std::env::var("COMPOSER_STATE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
        {
            host.shared("composer_state", n);
        }
        host
    });
    h.ui.scene.borrow_mut().resize(900., 1000.);
    h
}

/// Every node under `id` that is still see-through after a transition
/// settles (ghost texts, empty, are fine).
fn faded(h: &mut Harness, id: &str) -> Vec<String> {
    let node = h.find(id).unwrap();
    let scene = h.ui.scene.borrow();
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let o = scene.effects(n).opacity;
        let empty = scene.bounds(n).width < 0.5;
        if o < 0.99 && (o - 0.35).abs() > 0.01 && !empty {
            let mut text = String::new();
            let mut st = vec![n];
            while let Some(m) = st.pop() {
                if let zgx::scene::NodeKind::Text { text: t, .. } = scene.kind(m) {
                    text = t.to_string();
                    break;
                }
                st.extend(scene.children(m).iter().copied());
            }
            let b = scene.bounds(n);
            out.push(format!("{o:.2} {:.0}x{:.0} {text:?}", b.width, b.height));
        }
        stack.extend(scene.children(n).iter().copied());
    }
    out
}

#[test]
fn composer_states_step_through_and_settle() {
    let mut h = composer_states();
    ok(&h);
    // Forward through every state, then a few quick back-and-forths before
    // transitions finish (photo stack ↔ expanded, overflow ↔ open).
    let mut order: Vec<usize> = (1..=44).collect();
    order.extend([8, 7, 8, 7, 9, 7, 12, 11, 12, 4, 7, 15, 14, 2]);
    let quick = 44;
    for (step, &state) in order.iter().enumerate() {
        h.click(&format!("state-{state}"));
        let settle = if step >= quick && step + 1 < order.len() {
            12
        } else {
            60
        };
        for f in 0..settle {
            h.frame();
            // Every frame: the composer's parts (the card, then the footer)
            // never overlap, and the composer ends where they do.
            let root = h.find("composer").unwrap();
            let scene = h.ui.scene.borrow();
            let region = *scene
                .children(root)
                .iter()
                .find(|c| !scene.children(**c).is_empty())
                .unwrap();
            let parts: Vec<_> = scene
                .children(region)
                .iter()
                .map(|&c| scene.bounds(c))
                .collect();
            for pair in parts.windows(2) {
                assert!(
                    pair[1].y >= pair[0].y + pair[0].height - 2.,
                    "state {state}, frame {f}: the footer overlaps the card: {parts:?}"
                );
            }
            let b = scene.bounds(root);
            let bottom = parts.iter().map(|p| p.y + p.height).fold(0f32, f32::max);
            assert!(
                bottom <= b.y + b.height + 2.,
                "state {state}, frame {f}: the card overflows the composer"
            );
            // No opaque bordered box starts inside the input row: the seam
            // above it never crosses it.
            if let Some(row) = h.handle.find("input-row") {
                let r = scene.bounds(row);
                let mut stack = vec![root];
                while let Some(n) = stack.pop() {
                    if let zgx::scene::NodeKind::Panel { quad, .. } = scene.kind(n) {
                        let nb = scene.bounds(n);
                        let mut o = 1.;
                        let mut m = Some(n);
                        while let Some(x) = m {
                            o *= scene.effects(x).opacity;
                            m = scene.parent(x);
                        }
                        assert!(
                            !(quad.border_width > 0.
                                && o > 0.99
                                && nb.width > r.width * 0.5
                                && nb.y > r.y + 1.
                                && nb.y < r.y + r.height - 1.),
                            "state {state}, frame {f}: a bordered box's top edge crosses the input row ({nb:?} in {r:?})"
                        );
                    }
                    stack.extend(scene.children(n).iter().copied());
                }
            }
        }
        ok(&h);
        if settle < 60 {
            continue;
        }
        // The composer's parts stack without overlapping (headless text
        // metrics are approximate, so heights are not the lab's exactly).
        let root = h.find("composer").unwrap();
        {
            let scene = h.ui.scene.borrow();
            let region = *scene
                .children(root)
                .iter()
                .find(|c| !scene.children(**c).is_empty())
                .unwrap();
            let parts: Vec<_> = scene
                .children(region)
                .iter()
                .map(|&c| scene.bounds(c))
                .collect();
            for pair in parts.windows(2) {
                assert!(
                    pair[1].y >= pair[0].y + pair[0].height - 2.,
                    "state {state}: composer parts overlap: {parts:?}"
                );
            }
            let b = scene.bounds(root);
            let bottom = parts.iter().map(|p| p.y + p.height).fold(0f32, f32::max);
            assert!(
                (b.y + b.height - bottom).abs() < 2.,
                "state {state}: the composer ends at {} but its parts at {bottom}",
                b.y + b.height
            );
        }
        let left = faded(&mut h, "composer");
        assert!(
            left.is_empty(),
            "state {state}: still faded after settling: {left:#?}"
        );
    }
    for _ in 0..120 {
        h.frame();
    }
    assert!(!h.clock.wants_frame(), "nothing animates at rest");
}

#[test]
#[ignore]
fn dump_wrapped_text_in_shrinking_column() {
    let h = Harness::inline(
        r##"local function col(t) t.items = t.items or "stretch"; return ui.column(t) end
        return col { w = 300,
            col { gap = 8,
                ui.row { gap = 12, items = "start",
                    ui.text { "£34.99", w = 44, shrink = 0 },
                    col { grow = 1, shrink = 1, min_w = 0, gap = 2,
                        ui.row { gap = 4, items = "center", ui.text { "Ninja Blast BC151" } },
                        ui.text { "Cordless, but it's the only portable one that handles frozen fruit.", wrap = true, id = "why" } } },
                ui.row { id = "next", ui.text { "£49.00" } } } }"##,
    );
    h.ui.prepare_frame();
    let scene = h.ui.scene.borrow();
    fn walk(scene: &zgx::scene::Scene, n: zgx::scene::NodeId, depth: usize) {
        let b = scene.bounds(n);
        let kind = format!("{:?}", scene.kind(n));
        println!(
            "{}{:.0}x{:.0} @{:.0},{:.0} {}",
            " ".repeat(depth),
            b.width,
            b.height,
            b.x,
            b.y,
            &kind[..kind.len().min(40)]
        );
        for &c in scene.children(n) {
            walk(scene, c, depth + 1);
        }
    }
    walk(&scene, scene.root(), 0);
}

#[test]
fn composer_states_upload_test_settles() {
    let mut h = composer_states();
    // Fourteen uploads (the lab's whole list), a second apart, into the
    // empty composer: chips, then stacks, compact rows and back. Each press
    // changes the row's measured layout decision at most once.
    let decisions = h.host.shared("composer_upload_decisions", 0.);
    let count = || decisions.with_untracked(|v| v.as_f64().unwrap_or(0.));
    for press in 0..14 {
        let before = count();
        h.click("upload");
        for _ in 0..60 {
            h.frame();
        }
        ok(&h);
        assert!(
            count() - before <= 1.,
            "press {press}: the layout decision changed {} times",
            count() - before
        );
    }
    for _ in 0..60 * 12 {
        h.frame();
    }
    ok(&h);
    let root = h.find("composer").unwrap();
    let mut texts = Vec::new();
    {
        let scene = h.ui.scene.borrow();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if let zgx::scene::NodeKind::Text { text, .. } = scene.kind(n) {
                texts.push(text.to_string());
            }
            stack.extend(scene.children(n).iter().copied());
        }
    }
    // The lab's end state: ten files in a file stack (PDF, XLSX, +4 other
    // types) beside a photo stack of four.
    for want in ["10 files", "PDF", "XLSX", "+4", "4"] {
        assert!(texts.iter().any(|t| t == want), "no `{want}` in {texts:?}");
    }
    assert!(
        !texts.iter().any(|t| t.starts_with("Uploading")),
        "an upload never finished: {texts:?}"
    );
    for _ in 0..120 {
        h.frame();
    }
    assert!(
        !h.clock.wants_frame(),
        "nothing animates once the uploads finish"
    );
}

#[test]
#[ignore]
fn time_full_reloads() {
    for name in ["composer_states", "agent_trace"] {
        let h = example(name);
        for _ in 0..30 {
            h.frame();
        }
        let mut times = Vec::new();
        for _ in 0..10 {
            let start = std::time::Instant::now();
            h.host.reload().unwrap();
            h.settle();
            h.ui.prepare_frame();
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "{name}: reload median {:.1} ms, worst {:.1} ms",
            times[5], times[9]
        );
    }
}

#[test]
fn composer_states_buttons_brighten_and_squish() {
    use zgx::input::{InputEvent, PointerButton};
    let mut h = composer_states();
    let send = h.find("send").unwrap();
    let b = h.ui.scene.borrow().bounds(send);
    let (x, y) = (b.x + b.width / 2., b.y + b.height / 2.);
    let brightness = |h: &mut Harness| {
        let send = h.find("send").unwrap();
        h.ui.scene.borrow().effects(send).brightness
    };
    // Hovered, it brightens to 1.18 (the lab's `.cmp-hit:hover`).
    h.ui.dispatch(InputEvent::PointerMove { x, y });
    for _ in 0..20 {
        h.frame();
    }
    assert!(
        (brightness(&mut h) - 1.18).abs() < 0.01,
        "hovered: {}",
        brightness(&mut h)
    );
    // Pressed, it dims to .92; released, it is hovered again.
    h.ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
    for _ in 0..12 {
        h.frame();
    }
    assert!(
        (brightness(&mut h) - 0.92).abs() < 0.01,
        "pressed: {}",
        brightness(&mut h)
    );
    h.ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
    for _ in 0..60 {
        h.frame();
    }
    assert!(
        (brightness(&mut h) - 1.18).abs() < 0.01,
        "released: {}",
        brightness(&mut h)
    );
    // Left, it fades back.
    h.ui.dispatch(InputEvent::PointerMove { x: 5., y: 5. });
    for _ in 0..20 {
        h.frame();
    }
    assert!(
        (brightness(&mut h) - 1.).abs() < 0.01,
        "left: {}",
        brightness(&mut h)
    );
}

#[test]
fn selectable_markdown_selects_as_one_document() {
    let h = example("selectable");
    h.frame();
    ok(&h);
    h.ui.prepare_frame();
    let first = {
        let scene = h.ui.scene.borrow();
        let mut stack = vec![scene.root()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if matches!(scene.kind(n), zgx::scene::NodeKind::RichText { .. }) && found.is_none() {
                found = Some(n);
            }
            stack.extend(scene.children(n).iter().rev().copied());
        }
        found.expect("the heading")
    };
    assert!(h.ui.select_all_text(first));
    let text = h.ui.selected_text();
    assert!(
        text.starts_with("Selectable markdown\n\nEverything below"),
        "{text:?}"
    );
    assert!(text.contains("•\tDouble-press a word"), "{text:?}");
    assert!(
        text.contains("return ui.column { selectable = true"),
        "{text:?}"
    );
    assert!(
        text.contains("Gesture\tSelects\ndrag\tcharacters"),
        "{text:?}"
    );
    assert!(!text.contains("kept out"), "{text:?}");
}
