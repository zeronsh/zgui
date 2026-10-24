//! Responsive retained grid, wrapped children and ordinary component composition.
use zgx::{compose::prelude::*, layout::ContentAlign, scene::Color};
use zgx_desktop::{Application, WindowOptions};
fn tile(label: impl Into<String>, color: Color) -> View {
    div().p(12.).bg(color).rounded(8.).child(text(label))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx retained layout".into(),
            width: 800.,
            height: 600.,
            ..Default::default()
        })
        .run(|window| {
            let offset = window.ui.signal(0.);
            window.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(16.)
                    .bg(Color(20, 24, 32, 255))
                    .text_color(Color(235, 240, 250, 255))
                    .child(text("Retained grid and wrapping layout").text_size(24.))
                    .child(
                        div()
                            .grid()
                            .grid_cols(4)
                            .grid_rows(3)
                            .gap_x(12.)
                            .gap_y(8.)
                            .w_full()
                            .h(240.)
                            .child(
                                tile("Header: four columns", Color(52, 66, 100, 255))
                                    .col_span_full(),
                            )
                            .child(tile("Sidebar", Color(48, 80, 82, 255)).row_span(2))
                            .child(
                                tile("Main: two columns", Color(64, 63, 107, 255))
                                    .col_span(2)
                                    .row_span(2),
                            )
                            .child(tile("Detail A", Color(89, 63, 71, 255)))
                            .child(tile("Detail B", Color(89, 63, 71, 255))),
                    )
                    .child(
                        scroll(offset)
                            .w_full()
                            .h(210.)
                            .flex_row()
                            .flex_wrap()
                            .gap_x(10.)
                            .gap_y(10.)
                            .align_content(ContentAlign::Start)
                            .children((0..30).map(|i| {
                                tile(format!("Item {i}"), Color(37, 54, 70, 255))
                                    .w(140.)
                                    .h(44.)
                            })),
                    ),
            );
        })?;
    Ok(())
}
