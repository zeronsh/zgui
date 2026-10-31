//! A web page under test (Linux, `--features webkitgtk`; run by
//! `scripts/web_view_smoke.py`): a page in a box rounded by 24 points at
//! (40, 40), 600 × 400, with a red zgx square drawn over its bottom right
//! corner; on the page a field at (20, 20), 300 × 40, a `<select>` at
//! (360, 20), 200 × 30, a green target at (20, 100), 200 × 80, and enough
//! height to scroll.
//!
//! Prints `FINISHED <url>` as the page loads, `EVAL <json>` for a script it
//! then evaluates, `PAGE <json>` for each message the page sends (what it
//! heard: clicks, typing, compositions, keys, choices, scrolling) and `ACTION <name>`
//! for each of the app's shortcuts that fired: Ctrl+J (the page takes it),
//! Ctrl+K (the page leaves it alone) and Ctrl+L (reserved: the page never
//! hears it).
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers},
    web_view::{WebView, WebViewEvent, WebViewOptions, web_view},
};
use zgx_desktop::{Application, MenuAction, WindowOptions};

const PAGE: &str = r#"<!doctype html><html><head><title>probe page</title><style>
html, body { margin: 0; background: rgb(0, 0, 255); height: 3000px; }
#field { position: absolute; left: 20px; top: 20px; width: 300px; height: 40px; font-size: 20px; box-sizing: border-box; }
#choice { position: absolute; left: 360px; top: 20px; width: 200px; height: 30px; }
#target { position: absolute; left: 20px; top: 100px; width: 200px; height: 80px; background: rgb(0, 255, 0); }
</style></head><body><input id="field"><select id="choice"><option>Alpha</option><option>Beta</option><option>Gamma</option></select><div id="target"></div><script>
const post = (value) => window.zgx.postMessage(value);
const field = document.getElementById('field');
document.getElementById('target').addEventListener('click', (e) => post({ click: [e.offsetX, e.offsetY], detail: e.detail }));
field.addEventListener('input', () => post({ value: field.value }));
const choice = document.getElementById('choice');
choice.addEventListener('change', () => post({ chose: choice.value }));
field.addEventListener('compositionstart', () => post({ composition: 'start' }));
field.addEventListener('compositionend', (e) => post({ composition: 'end', data: e.data }));
document.addEventListener('keydown', (e) => {
  post({ key: e.key, code: e.code, ctrl: e.ctrlKey });
  if (e.ctrlKey && e.key === 'j') e.preventDefault();
}, true);
let scrolled = false;
addEventListener('scroll', () => { if (!scrolled && scrollY > 0) { scrolled = true; post({ scrolled: scrollY }); } });
</script></body></html>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Web view probe".into(),
            width: 760.,
            height: 520.,
            ..Default::default()
        })
        .run(|cx| {
            // `--transparent`: a page with no background of its own, over
            // its box's yellow.
            let transparent = std::env::args().any(|arg| arg == "--transparent");
            let options = WebViewOptions {
                transparent,
                ..Default::default()
            };
            let web = WebView::new(&cx.ui.runtime, options);
            let page = if transparent {
                PAGE.replace("background: rgb(0, 0, 255)", "background: transparent")
            } else {
                PAGE.into()
            };
            web.load_html(page, Some("https://probe.zgx.test/"));
            let evaluate = web.clone();
            let events = web.on_event(move |event| match event {
                WebViewEvent::NavigationFinished { url } => {
                    println!("FINISHED {url}");
                    evaluate.evaluate_js("document.title + ' ' + (1 + 2)", |result| match result {
                        Ok(json) => println!("EVAL {json}"),
                        Err(error) => println!("EVAL error {error}"),
                    });
                }
                WebViewEvent::Message {
                    body,
                    origin,
                    main_frame,
                } => {
                    println!("PAGE {body} origin={origin} main={main_frame}")
                }
                _ => {}
            });
            cx.render(
                div()
                    .w_full()
                    .h_full()
                    .bg(rgba(0x202020ff))
                    .child(
                        web_view(&web)
                            .absolute()
                            .left(40.)
                            .top(40.)
                            .w(600.)
                            .h(400.)
                            .rounded(24.)
                            .bg(rgba(0xffff00ff)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(560.)
                            .top(360.)
                            .w(120.)
                            .h(120.)
                            .bg(rgba(0xff0000ff)),
                    ),
            );
            let root = cx.ui.root();
            cx.ui.on_event(root, false, move |event| {
                if event.phase != EventPhase::Capture
                    && let InputEvent::Action(action) = &event.event
                    && let Some(action) = action.downcast_ref::<MenuAction>()
                {
                    println!("ACTION {}", action.0);
                    event.prevent_default();
                }
            });
            let control = Modifiers {
                control: true,
                ..Default::default()
            };
            let bind = |key: &str| {
                KeyBinding::new(
                    [Keystroke::new(Key::Character(key.into()), control)],
                    Action::new(MenuAction(key.into())),
                )
                .unwrap()
            };
            let keymap = cx.ui.input.bind_keys(
                root,
                Keymap::new()
                    .bind(bind("j"))
                    .bind(bind("k"))
                    .bind(bind("l").reserved()),
            );
            cx.tasks.spawn(async move {
                let _keep = (events, keymap, web);
                std::future::pending::<()>().await;
            });
        })
}
