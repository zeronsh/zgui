//! One Ghostty terminal in a frosted zgx window.
//! Run: cargo run --release -p zgx-desktop --features ghostty --example ghostty_pane
use zgx::{
    compose::prelude::*,
    terminal::{Terminal, TerminalOptions, terminal},
};
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};

const TITLE: &str = "zgx · ghostty pane";
const WIDTH: f64 = 900.;
const HEIGHT: f64 = 600.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let colors_fixture = std::env::args().any(|arg| arg == "--colors-fixture");
    let typed = std::env::args()
        .nth(1)
        .filter(|arg| arg != "--colors-fixture");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            decorations: cfg!(target_os = "macos"),
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            let term = Terminal::new(&cx.ui.runtime, TerminalOptions {
                font_family: colors_fixture.then(|| "Menlo".into()),
                font_size: colors_fixture.then_some(12.),
                ..Default::default()
            });
            if let Some(typed) = &typed {
                term.input(format!("{typed}\r"));
            }
            term.focus();
            if colors_fixture {
                cx.ui.set_zoom(1.25);
                let term = term.clone();
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    async fn title(term: &Terminal, expected: &str) {
                        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                        while term.title() != expected {
                            assert!(std::time::Instant::now() < deadline, "shell title never became {expected}: {}", term.title());
                            zgx::timer::sleep(std::time::Duration::from_millis(20)).await;
                        }
                    }
                    // These commands run only in this probe's new terminal.
                    term.input("ZGX_COLOR_AUDIT=kept; printf '\\033]2;READY\\007'; read -r ZGX_COLOR_STEP\r");
                    title(&term, "READY").await;
                    let options = term.options();
                    let cells = |term: &Terminal| {
                        let state = term.state().get();
                        (state.cell_width, state.cell_height)
                    };
                    let initial = cells(&term);
                    term.binding("increase_font_size:4");
                    zgx::timer::sleep(std::time::Duration::from_millis(150)).await;
                    let enlarged = cells(&term);
                    assert!(enlarged.1 > initial.1 && initial.1 > 0, "real native cells enlarge: {initial:?} → {enlarged:?}");
                    term.set_colors(0xffffffff, 0x111111ff);
                    term.input("\rprintf '\\033]2;%s\\007' \"$ZGX_COLOR_AUDIT\"; read -r ZGX_COLOR_STEP\r");
                    title(&term, "kept").await;
                    assert_eq!(cells(&term), enlarged, "light theme retains Bigger Text and window zoom");
                    assert_eq!(term.options(), options);
                    assert!(term.is_attached() && !term.is_closed());
                    term.set_colors(0x141517ff, 0xeeeeeeff);
                    term.input("\rprintf '\\033]2;dark-%s\\007' \"$ZGX_COLOR_AUDIT\"; read -r ZGX_COLOR_STEP\r");
                    title(&term, "dark-kept").await;
                    assert_eq!(cells(&term), enlarged, "dark theme retains Bigger Text and window zoom");
                    println!("TERMINAL COLORS PASS: light/dark updates retain shell variable, surface, options, enlarged cells {enlarged:?} and window zoom1.25");
                    window.close();
                });
            }
            let title = term.clone();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .bg(rgba(0x1214168c))
                    .child(
                        row().h(40.).pl(84.).items_center().child(
                            text_signal(move || {
                                let t = title.title();
                                if t.is_empty() { "Terminal".into() } else { t }
                            })
                            .text_size(13.)
                            .text_color(rgba(0xffffffa6)),
                        ),
                    )
                    .child(
                        terminal(&term)
                            .grow()
                            .mx(6.)
                            .mb(6.)
                            .rounded(10.)
                            .bg(rgba(0x0c0d10d9)),
                    ),
            );
        })
}
