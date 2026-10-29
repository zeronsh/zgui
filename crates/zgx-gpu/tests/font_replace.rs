//! A font replaced while text shows in it: the text measures, lays out and
//! draws as it would had the new font been there from the start.
use zgx::{
    compose::prelude::*,
    scene::{Color, Rect},
    text_layout::FontFamily,
    widgets::Ui,
};
use zgx_gpu::{GpuRenderer, text::FontData};

const MEDIUM: &[u8] = include_bytes!("fonts/Geist-500-subset.ttf");
const SEMIBOLD: &[u8] = include_bytes!("fonts/Geist-600-subset.ttf");
const SIZE: (f32, f32) = (320., 120.);

/// A row holding a label in Geist, sized by it, then a box after it.
fn view() -> View {
    row()
        .items_start()
        .bg(Color(20, 24, 32, 255))
        .child(
            text("Requests 48,213")
                .id("label")
                .font_family(FontFamily::Named("Geist".into()))
                .text_size(18.),
        )
        .child(div().id("after").size(20., 20.).bg(Color(200, 80, 80, 255)))
}

struct Window {
    ui: Ui,
    gpu: GpuRenderer,
    view: zgx::compose::ViewHandle,
}
impl Window {
    fn new(font: &[u8]) -> Self {
        let gpu = GpuRenderer::new(SIZE.0 as u32, SIZE.1 as u32).unwrap();
        // Its font alone: a Geist installed on the machine would match too.
        *gpu.text_system().borrow_mut() = cosmic_text::FontSystem::new_with_locale_and_db(
            "en".into(),
            cosmic_text::fontdb::Database::new(),
        );
        FontData::new(font.to_vec())
            .unwrap()
            .install(&mut gpu.text_system().borrow_mut());
        let mut ui = Ui::new(SIZE.0, SIZE.1);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let view = ui.mount(view());
        Window { ui, gpu, view }
    }
    fn draw(&mut self) -> Vec<u8> {
        self.ui.prepare_frame();
        let report = self.ui.scene.borrow_mut().flush();
        self.gpu
            .render(&self.ui.scene.borrow(), &report.damage)
            .unwrap();
        self.gpu.readback().unwrap()
    }
    fn bounds(&self, id: &str) -> Rect {
        self.ui.scene.borrow().bounds(self.view.find(id).unwrap())
    }
}

#[test]
fn a_replaced_font_lays_out_and_draws_as_a_fresh_one() {
    let mut window = Window::new(MEDIUM);
    window.draw();
    let medium = window.bounds("label");
    // Replaced in the context its renderer shapes with, then told.
    window
        .gpu
        .context()
        .replace_font(&FontData::new(SEMIBOLD.to_vec()).unwrap());
    window.gpu.fonts_replaced();
    window.ui.scene.borrow_mut().fonts_replaced();
    window.ui.refresh_text_geometry();
    let replaced = window.draw();

    let mut fresh = Window::new(SEMIBOLD);
    let drawn = fresh.draw();
    assert_ne!(
        medium.width,
        fresh.bounds("label").width,
        "the two weights measure differently"
    );
    assert_eq!(
        window.bounds("label"),
        fresh.bounds("label"),
        "measured anew"
    );
    assert_eq!(
        window.bounds("after"),
        fresh.bounds("after"),
        "laid out anew"
    );
    assert!(replaced == drawn, "drawn as a fresh renderer draws it");
}

#[test]
fn replacing_a_font_takes_out_the_faces_of_its_families() {
    let mut fonts = cosmic_text::FontSystem::new_with_locale_and_db(
        "en".into(),
        cosmic_text::fontdb::Database::new(),
    );
    let medium = FontData::new(MEDIUM.to_vec()).unwrap();
    medium.install(&mut fonts);
    let geist = |fonts: &cosmic_text::FontSystem| {
        fonts
            .db()
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == "Geist"))
            .map(|face| face.weight.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(geist(&fonts), [500]);
    FontData::new(SEMIBOLD.to_vec())
        .unwrap()
        .replace_in(&mut fonts);
    assert_eq!(geist(&fonts), [600], "the old face is gone, the new one in");
}
