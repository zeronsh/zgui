//! A software renderer that paints a scene into a pixel buffer, for
//! reference and headless tests.
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use zgx::scene::{Color, Effects, NodeKind, Rect, Scene};

type StyledGlyphKey = (String, u32, u32, u32, zgx::text_layout::FontStyle);
type RichGlyphImages = (
    Vec<(i32, i32, Color, cosmic_text::SwashImage)>,
    Vec<zgx_gpu::text::RichDecoration>,
);
type RichGlyphCache = Vec<(
    std::sync::Arc<zgx::rich_text::RichText>,
    u32,
    u32,
    RichGlyphImages,
)>;
type StyledGlyphImages = Vec<(i32, i32, cosmic_text::SwashImage)>;
const STYLED_GLYPH_BUDGET: usize = 8 * 1024 * 1024;
fn styled_bytes(key: &StyledGlyphKey, images: &StyledGlyphImages) -> usize {
    key.0.capacity()
        + key.4.storage_bytes()
        + images.capacity() * std::mem::size_of::<(i32, i32, cosmic_text::SwashImage)>()
        + images
            .iter()
            .map(|(_, _, image)| image.data.capacity())
            .sum::<usize>()
}

/// Persistent CPU backing store with cached glyph masks and damage-only rasterization.
/// Glyph cache is bounded; this backend intentionally favors a small portable implementation.
pub struct Raster {
    /// The frame, a row at a time, a pixel each as `0x00RRGGBB`.
    pub pixels: Vec<u32>,
    width: usize,
    height: usize,
    font: Font,
    styled_fonts: std::rc::Rc<std::cell::RefCell<Option<zgx_gpu::text::CpuTextRaster>>>,
    styled_glyphs: HashMap<StyledGlyphKey, StyledGlyphImages>,
    rich_glyphs: RichGlyphCache,
    canvases: zgx_gpu::canvas::CanvasCache,
    svgs: zgx_gpu::svg::SvgCache,
    glyphs: HashMap<(char, u32), (Metrics, Vec<u8>)>,
    scratch: Vec<u32>,
    background: u32,
    origin: (f32, f32),
    /// The selection on the text being drawn, which recolours its glyphs.
    selected: Option<zgx::scene::TextHighlight>,
}
impl Raster {
    /// Register an additional validated font and invalidate retained glyph shapes.
    pub fn register_font(&mut self, font: &zgx_gpu::text::FontData) {
        self.styled_fonts
            .borrow_mut()
            .get_or_insert_with(Default::default)
            .register_font(font);
        self.styled_glyphs.clear();
        self.rich_glyphs.clear();
    }

    /// A `width` by `height` store filled with the background colour.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0x10141c; width * height],
            width,
            height,
            font: Font::from_bytes(
                include_bytes!("../assets/DejaVuSans.ttf") as &[u8],
                FontSettings::default(),
            )
            .expect("bundled font"),
            styled_fonts: Default::default(),
            styled_glyphs: HashMap::new(),
            rich_glyphs: Vec::new(),
            canvases: Default::default(),
            svgs: Default::default(),
            glyphs: HashMap::new(),
            scratch: Vec::new(),
            background: 0x10141c,
            origin: (0., 0.),
            selected: None,
        }
    }
    /// Resize the store to `width` by `height`.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.pixels.resize(width * height, 0x10141c);
    }
    /// Repaint `damage` (and whatever scrolled) from `scene`.
    pub fn render(&mut self, scene: &Scene, damage: &[Rect]) {
        self.canvases.retain(scene);
        self.svgs.retain(scene);
        // No pixel copies here: scrolled regions repaint in full.
        let moved: Vec<Rect> = scene
            .scroll_moves()
            .iter()
            .filter(|scroll| scroll.serial == scene.flush_serial())
            .map(|scroll| scroll.clip)
            .collect();
        let damage = [damage, &moved].concat();
        self.render_flat(scene, scene.layer_items(None), &damage);
    }
    fn render_flat(
        &mut self,
        scene: &Scene,
        mut items: Vec<zgx::scene::PaintItem<'_>>,
        damage: &[Rect],
    ) {
        if damage.is_empty() {
            return;
        }
        // Invisible subtrees must not rasterize glyphs or alter the backdrop.
        items.retain(|item| item.effects.opacity > 0.);
        // Backdrop filters require already-composited pixels behind the filter. The CPU
        // reference path conservatively rebuilds the surface when a filter is present.
        let full = [Rect {
            x: 0.,
            y: 0.,
            width: self.width as f32,
            height: self.height as f32,
        }];
        let has_blur = items.iter().any(|item| item.effects.blur_radius > 0.);
        let damage = if has_blur { &full[..] } else { damage };
        for &dirty in damage {
            let (x0, y0, x1, y1) = self.region(dirty);
            for y in y0..y1 {
                self.pixels[y * self.width + x0..y * self.width + x1].fill(self.background);
            }
            for item in &items {
                let Some(clip) = intersect(dirty, item.clip.unwrap_or(full[0])) else {
                    continue;
                };
                // Rounded clips around it: its pixels there mixed back over
                // what lay beneath by how much of each the curves keep.
                let corners = self.save_corners(&item.rounded(), clip);
                'draw: {
                    if item.isolated {
                        self.isolated(scene, item, clip);
                        break 'draw;
                    }
                    let Some(visible) = intersect(
                        clip,
                        if let NodeKind::Quad(style) | NodeKind::Panel { quad: style, .. } =
                            item.kind
                        {
                            style.paint_bounds(item.bounds)
                        } else if let NodeKind::Svg(svg) = item.kind {
                            svg.paint_bounds(item.bounds)
                        } else if let NodeKind::Image(image) = item.kind {
                            image.paint_bounds(item.bounds)
                        } else {
                            item.bounds
                        },
                    ) else {
                        break 'draw;
                    };
                    if item.effects.blur_radius > 0. {
                        self.blur(visible, item.effects.blur_radius.ceil().min(64.) as usize);
                    }
                    // A text selection, under the glyphs, and what recolours them.
                    if let Some(highlight) = scene.text_highlight(item.id) {
                        for r in &highlight.rects {
                            let rect = Rect::new(
                                item.bounds.x + r.x,
                                item.bounds.y + r.y,
                                r.width,
                                r.height,
                            );
                            let color = tint(highlight.color, item.color());
                            if highlight.radius > 0. {
                                let style = zgx::scene::QuadStyle {
                                    fill: color,
                                    radius: highlight.radius,
                                    ..Default::default()
                                };
                                self.quad(item.id, rect, clip, style, item.effects);
                            } else {
                                self.rect(rect, clip, color, item.effects);
                            }
                        }
                        self.selected =
                            highlight.text_color.map(|text| zgx::scene::TextHighlight {
                                text_color: Some(tint(text, item.color())),
                                ..highlight.clone()
                            });
                    }
                    match item.kind {
                        NodeKind::Rect(color) => {
                            self.rect(item.bounds, clip, tint(*color, item.color()), item.effects)
                        }
                        NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => {
                            // A border pass: the border over what it holds
                            // (if it paints there), then the outline.
                            if !item.border_pass || (quad.border_over && quad.has_border()) {
                                let mut style = border_part(quad, item.border_pass);
                                style.fill = tint(style.fill, item.color());
                                style.border_color = tint(style.border_color, item.color());
                                self.quad(item.id, item.bounds, clip, style, item.effects)
                            }
                            if let Some(outline) = quad.outline.filter(|o| o.visible())
                                && item.border_pass
                            {
                                let grown = outline.offset + outline.width;
                                let ring = zgx::scene::QuadStyle {
                                    radius: if quad.radius > 0. {
                                        quad.radius + grown
                                    } else {
                                        0.
                                    },
                                    border_width: outline.width,
                                    border_color: tint(outline.color, item.color()),
                                    ..Default::default()
                                };
                                self.quad(
                                    item.id,
                                    outline.outer(item.bounds),
                                    clip,
                                    ring,
                                    item.effects,
                                )
                            }
                        }
                        NodeKind::Svg(svg) => {
                            if item.bounds.width > 0.
                                && item.bounds.height > 0.
                                && let Ok(image) = self.svgs.get(
                                    item.id,
                                    svg,
                                    item.bounds.width,
                                    item.bounds.height,
                                    1.,
                                )
                            {
                                self.image(item.bounds, clip, &image, item.effects);
                            }
                        }
                        NodeKind::Image(image) => {
                            self.image(item.bounds, clip, image, item.effects)
                        }
                        NodeKind::Canvas(canvas) => {
                            // A grid under its drawing, within its box.
                            if scene.has_grid_fills()
                                && let Some(grid) = scene.grid_fill(item.id)
                                && let Some(inside) = clip.intersection(item.bounds)
                            {
                                let color = tint(grid.color, item.color());
                                for mark in grid.marks(item.bounds) {
                                    self.rect(mark, inside, color, item.effects);
                                }
                            }
                            if item.bounds.width > 0.
                                && item.bounds.height > 0.
                                && let Ok(raster) = self.canvases.get_viewed(
                                    item.id,
                                    canvas,
                                    scene.canvas_view(item.id),
                                    item.bounds.width,
                                    item.bounds.height,
                                    1.,
                                )
                                // Drawn from memory: never marked uploaded.
                                && let Some(image) = raster.pixels
                            {
                                self.image(item.bounds, clip, &image, item.effects);
                            }
                        }
                        #[cfg(target_os = "macos")]
                        NodeKind::NativeSurface(_) => {} // Native GPU-only source; no CPU readback.
                        NodeKind::RichText { text } => {
                            self.rich_text(text, item.bounds, clip, item.effects)
                        }
                        NodeKind::Text {
                            text,
                            color,
                            font_size,
                        } => {
                            let color = &tint(*color, item.color());
                            if item.text_options != Default::default() {
                                let runs = if text.is_empty() {
                                    Vec::new()
                                } else {
                                    vec![zgx::rich_text::TextRun {
                                        range: 0..text.len(),
                                        font: item.font.clone(),
                                        font_size: *font_size,
                                        color: *color,
                                        ..Default::default()
                                    }]
                                };
                                let rich = std::sync::Arc::new(
                                    zgx::rich_text::RichText::new(text.clone(), runs)
                                        .unwrap()
                                        .with_options(item.text_options),
                                );
                                self.rich_text(&rich, item.bounds, clip, item.effects);
                            } else if *item.font == Default::default() {
                                self.text(text, item.bounds, clip, *color, *font_size, item.effects)
                            } else {
                                self.styled_text(
                                    text,
                                    item.bounds,
                                    clip,
                                    *color,
                                    *font_size,
                                    item.effects,
                                    item.font,
                                )
                            }
                        }
                        _ => {}
                    }
                }
                self.selected = None;
                self.mix_corners(&item.rounded(), corners);
            }
        }
    }
    fn isolated(&mut self, scene: &Scene, item: &zgx::scene::PaintItem<'_>, clip: Rect) {
        // A content blur spreads the layer; its mattes get room for it.
        let sigma = item.effects.filter_blur;
        let bounds = scene
            .layer_bounds(item.id)
            .expand(zgx::scene::filter_spread(sigma));
        let x = bounds.x.floor();
        let y = bounds.y.floor();
        let width = ((bounds.x + bounds.width).ceil() - x).max(1.) as usize;
        let height = ((bounds.y + bounds.height).ceil() - y).max(1.) as usize;
        // Reference backend extracts coverage using black/white mattes, preserving
        // its public opaque RGB pixel format. GPU layers store premultiplied RGBA.
        if width
            .checked_mul(height)
            .is_none_or(|n| n > 16 * 1024 * 1024)
        {
            return;
        }
        let make = |background| Self {
            pixels: vec![background; width * height],
            width,
            height,
            font: self.font.clone(),
            styled_fonts: self.styled_fonts.clone(),
            styled_glyphs: HashMap::new(),
            rich_glyphs: Vec::new(),
            canvases: Default::default(),
            svgs: Default::default(),
            glyphs: HashMap::new(),
            scratch: Vec::new(),
            background,
            origin: (x, y),
            selected: None,
        };
        let mut black = make(0);
        let mut white = make(0xffffff);
        let mut items = scene.layer_items(Some(item.id));
        zgx::scene::translate_items(&mut items, -x, -y);
        let full = [Rect::new(0., 0., width as f32, height as f32)];
        black.render_flat(scene, items.clone(), &full);
        white.render_flat(scene, items, &full);
        if sigma > 0. {
            // Three box passes of radius sigma: close to its Gaussian, and
            // reaching as far as the spread damage covers.
            let radius = sigma.round().clamp(1., 64.) as usize;
            for _ in 0..3 {
                black.blur(full[0], radius);
                white.blur(full[0], radius);
            }
        }
        // The mask's alpha at a matte pixel, nearest, none past its image.
        let node = (item.bounds.x + self.origin.0, item.bounds.y + self.origin.1);
        let mask = scene.mask(item.id);
        let masked = |sx: f32, sy: f32| -> f32 {
            let Some(mask) = mask else { return 1. };
            let image = &mask.image;
            let texel = |at: f32, node: f32, offset: f32, size: f32, pixels: u32| {
                let texel = ((at - node - offset) / size * pixels as f32).floor();
                (size > 0. && texel >= 0. && texel < pixels as f32).then_some(texel as usize)
            };
            let (Some(tx), Some(ty)) = (
                texel(x + sx, node.0, mask.offset.0, mask.size.0, image.width()),
                texel(y + sy, node.1, mask.offset.1, mask.size.1, image.height()),
            ) else {
                return 0.;
            };
            image.pixels()[(ty * image.width() as usize + tx) * 4 + 3] as f32 / 255.
        };
        let output = Rect::new(
            x - self.origin.0,
            y - self.origin.1,
            width as f32,
            height as f32,
        );
        // A transformed layer samples back through its matrix (from this
        // target's origin), nearest texel, as transformed images do.
        let matrix = item.matrix().map(|matrix| {
            let (x, y) = self.origin;
            zgx::affine::Affine::translation(x, y)
                .then(matrix)
                .then(zgx::affine::Affine::translation(-x, -y))
        });
        let inverse = match matrix.map(|matrix| matrix.inverse()) {
            Some(None) => return,
            Some(inverse) => inverse,
            None => None,
        };
        let painted = matrix.map_or(output, |matrix| matrix.bounds(output));
        let Some(visible) = intersect(painted, clip) else {
            return;
        };
        if item.effects.blur_radius > 0. {
            self.blur(visible, item.effects.blur_radius.ceil().min(64.) as usize)
        }
        let (x0, y0, x1, y1) = self.region(visible);
        for py in y0..y1 {
            for px in x0..x1 {
                let (sx, sy, fade_y) = match inverse {
                    None => (
                        (px as f32 - output.x) as usize,
                        (py as f32 - output.y) as usize,
                        py as f32,
                    ),
                    Some(inverse) => {
                        let (x, y) = inverse.point(px as f32 + 0.5, py as f32 + 0.5);
                        let (sx, sy) = (x - output.x, y - output.y);
                        if sx < 0. || sy < 0. || sx >= width as f32 || sy >= height as f32 {
                            continue;
                        }
                        (sx as usize, sy as usize, y)
                    }
                };
                let b = black.pixels[sy * width + sx];
                let w = white.pixels[sy * width + sx];
                let alpha = 255 - ((w & 255) as i32 - (b & 255) as i32).clamp(0, 255);
                if alpha == 0 {
                    continue;
                }
                let channel = |shift: u32| {
                    ((((b >> shift) & 255) * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8
                };
                let color = Color(channel(16), channel(8), channel(0), alpha as u8);
                blend(
                    &mut self.pixels[py * self.width + px],
                    color,
                    item.effects.opacity
                        * edge_alpha(fade_y, item.bounds, item.effects.edge_fade)
                        * masked(sx as f32 + 0.5, sy as f32 + 0.5),
                );
            }
        }
    }
    /// The pixels within `clip` a rounded clip's corners can cut, with
    /// their values before an item draws.
    fn save_corners(&self, rounded: &zgx::scene::RoundedClip, clip: Rect) -> Vec<(usize, u32)> {
        let mut saved = Vec::new();
        for round in rounded.iter().flatten() {
            let r = round.rect;
            let [tl, tr, br, bl] = round.radii;
            for (x, y, radius) in [
                (r.x, r.y, tl),
                (r.x + r.width - tr, r.y, tr),
                (r.x + r.width - br, r.y + r.height - br, br),
                (r.x, r.y + r.height - bl, bl),
            ] {
                let Some(square) = intersect(Rect::new(x, y, radius, radius), clip) else {
                    continue;
                };
                let (x0, y0, x1, y1) = self.region(square);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let at = py * self.width + px;
                        saved.push((at, self.pixels[at]));
                    }
                }
            }
        }
        saved
    }
    /// Mix the item just drawn back over `saved` by the rounded clips'
    /// coverage of each pixel's centre.
    fn mix_corners(&mut self, rounded: &zgx::scene::RoundedClip, saved: Vec<(usize, u32)>) {
        for (at, before) in saved {
            let (x, y) = (
                (at % self.width) as f32 + 0.5,
                (at / self.width) as f32 + 0.5,
            );
            let keep: f32 = rounded
                .iter()
                .flatten()
                .map(|round| rounded_coverage(round, x, y))
                .product();
            let after = self.pixels[at];
            let mix = |shift: u32| {
                let (b, a) = ((before >> shift) & 255, (after >> shift) & 255);
                ((b as f32 + (a as f32 - b as f32) * keep).round() as u32) << shift
            };
            self.pixels[at] = mix(16) | mix(8) | mix(0);
        }
    }
    fn region(&self, rect: Rect) -> (usize, usize, usize, usize) {
        (
            rect.x.floor().clamp(0., self.width as f32) as usize,
            rect.y.floor().clamp(0., self.height as f32) as usize,
            (rect.x + rect.width).ceil().clamp(0., self.width as f32) as usize,
            (rect.y + rect.height).ceil().clamp(0., self.height as f32) as usize,
        )
    }
    fn rect(&mut self, bounds: Rect, clip: Rect, color: Color, effects: Effects) {
        let Some(r) = intersect(bounds, clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(r);
        for y in y0..y1 {
            let edge = edge_alpha(y as f32, bounds, effects.edge_fade);
            for x in x0..x1 {
                blend(
                    &mut self.pixels[y * self.width + x],
                    color,
                    effects.opacity * edge,
                );
            }
        }
    }
    fn image(&mut self, bounds: Rect, clip: Rect, image: &zgx::image::ImageData, effects: Effects) {
        let transform = image.paint_transform(bounds);
        let Some(inverse) = transform.inverse() else {
            return;
        };
        let Some(visible) = intersect(image.paint_bounds(bounds), clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(visible);
        for y in y0..y1 {
            for x in x0..x1 {
                let (local_x, local_y) = inverse.point(x as f32 + 0.5, y as f32 + 0.5);
                if local_x < bounds.x
                    || local_x >= bounds.x + bounds.width
                    || local_y < bounds.y
                    || local_y >= bounds.y + bounds.height
                {
                    continue;
                }
                let sx = (((local_x - bounds.x) / bounds.width * image.width() as f32) as u32)
                    .min(image.width() - 1);
                let sy = (((local_y - bounds.y) / bounds.height * image.height() as f32) as u32)
                    .min(image.height() - 1);
                let i = ((sy * image.width() + sx) * 4) as usize;
                let p = &image.pixels()[i..i + 4];
                blend(
                    &mut self.pixels[y * self.width + x],
                    Color(p[0], p[1], p[2], p[3]),
                    effects.opacity * edge_alpha(local_y, bounds, effects.edge_fade),
                );
            }
        }
    }
    fn quad(
        &mut self,
        node: zgx::scene::NodeId,
        bounds: Rect,
        clip: Rect,
        style: zgx::scene::QuadStyle,
        effects: Effects,
    ) {
        for shadow in style.shadows().iter().take(32) {
            let core = bounds.expand(shadow.spread.max(0.));
            let core = Rect::new(
                core.x + shadow.offset.x,
                core.y + shadow.offset.y,
                core.width,
                core.height,
            );
            if let Some(visible) = intersect(core.expand(shadow.blur_radius.max(0.) * 3.), clip) {
                let (x0, y0, x1, y1) = self.region(visible);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let d = rounded_distance(x as f32 + 0.5, y as f32 + 0.5, core, {
                            let corners = zgx_gpu::decoration::shadow_corners(
                                &style,
                                core.width,
                                core.height,
                                shadow.spread.max(0.),
                            );
                            let right = x as f32 + 0.5 > core.x + core.width * 0.5;
                            let bottom = y as f32 + 0.5 > core.y + core.height * 0.5;
                            corners[match (right, bottom) {
                                (false, false) => 0,
                                (true, false) => 1,
                                (true, true) => 2,
                                (false, true) => 3,
                            }]
                        });
                        let alpha =
                            zgx_gpu::decoration::shadow_coverage(d, shadow.blur_radius.max(0.));
                        blend(
                            &mut self.pixels[y * self.width + x],
                            shadow.color,
                            alpha * effects.opacity,
                        );
                    }
                }
            }
        }
        // A background image: the fill, the image, then the border.
        if let Some(zgx::decoration::Background::Image { image, fit }) = style
            .decoration
            .as_deref()
            .and_then(|detail| detail.background.clone())
        {
            let staged = zgx::scene::QuadStyle {
                border_over: true,
                ..style.clone()
            };
            let mut fill = border_part(&staged, false);
            fill.border_over = false;
            fill.decoration = fill.decoration.map(|detail| {
                std::sync::Arc::new(zgx::decoration::Decoration {
                    background: None,
                    ..(*detail).clone()
                })
            });
            self.quad(node, bounds, clip, fill, effects);
            let (width, height) = fitted(image.width() as f32, image.height() as f32, bounds, fit);
            let placed = Rect::new(
                bounds.x + (bounds.width - width) / 2.,
                bounds.y + (bounds.height - height) / 2.,
                width,
                height,
            );
            if let Some(inside) = intersect(bounds, clip) {
                self.image(placed, inside, &image, effects);
            }
            if style.has_border() {
                let mut border = border_part(&staged, true);
                border.border_over = false;
                border.decoration = border.decoration.map(|detail| {
                    std::sync::Arc::new(zgx::decoration::Decoration {
                        background: Some(zgx::decoration::Background::Brush(
                            zgx::canvas::Brush::Solid(Color(0, 0, 0, 0)),
                        )),
                        ..(*detail).clone()
                    })
                });
                self.quad(node, bounds, clip, border, effects);
            }
            return;
        }
        if style.decoration.is_some() && bounds.width > 0. && bounds.height > 0. {
            if let Ok(raster) =
                self.canvases
                    .get_decoration(node, &style, bounds.width, bounds.height, 1.)
                && let Some(image) = raster.pixels
            {
                self.image(bounds, clip, &image, effects);
            }
            return;
        }
        let Some(visible) = intersect(bounds, clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(visible);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = rounded_distance(x as f32 + 0.5, y as f32 + 0.5, bounds, style.radius);
                let outer = (0.5 - d).clamp(0., 1.);
                let inner = (0.5 - d - style.border_width.max(0.)).clamp(0., 1.);
                let amount = effects.opacity * edge_alpha(y as f32, bounds, effects.edge_fade);
                let a = style.fill.3 as f32 / 255. * inner
                    + style.border_color.3 as f32 / 255. * (outer - inner);
                if a > 0. {
                    let channel = |fill: u8, border: u8| {
                        ((fill as f32 * style.fill.3 as f32 / 255. * inner
                            + border as f32 * style.border_color.3 as f32 / 255. * (outer - inner))
                            / a)
                            .round() as u8
                    };
                    let c = Color(
                        channel(style.fill.0, style.border_color.0),
                        channel(style.fill.1, style.border_color.1),
                        channel(style.fill.2, style.border_color.2),
                        (a * 255.).round() as u8,
                    );
                    blend(&mut self.pixels[y * self.width + x], c, amount);
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn rich_text(
        &mut self,
        text: &std::sync::Arc<zgx::rich_text::RichText>,
        bounds: Rect,
        clip: Rect,
        effects: Effects,
    ) {
        let Some(clip) = intersect(bounds, clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(clip);
        let width = bounds.width.to_bits();
        let height = bounds.height.to_bits();
        let mut index = self
            .rich_glyphs
            .iter()
            .position(|(rich, w, h, _)| rich == text && *w == width && *h == height);
        let mut oversized = false;
        if index.is_none() {
            let images = self
                .styled_fonts
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .rasterize_rich(text, bounds.width, bounds.height);
            let bytes = |rich: &zgx::rich_text::RichText, images: &RichGlyphImages| {
                rich.storage_bytes()
                    + images.0.capacity()
                        * std::mem::size_of::<(i32, i32, Color, cosmic_text::SwashImage)>()
                    + images.1.capacity() * std::mem::size_of::<zgx_gpu::text::RichDecoration>()
                    + images
                        .0
                        .iter()
                        .map(|(_, _, _, i)| i.data.capacity())
                        .sum::<usize>()
            };
            let new_bytes = bytes(text, &images);
            if self.rich_glyphs.len() >= 128
                || self
                    .rich_glyphs
                    .iter()
                    .map(|(r, _, _, i)| bytes(r, i))
                    .sum::<usize>()
                    .saturating_add(new_bytes)
                    > STYLED_GLYPH_BUDGET
            {
                self.rich_glyphs.clear();
            }
            oversized = new_bytes > STYLED_GLYPH_BUDGET;
            index = Some(self.rich_glyphs.len());
            self.rich_glyphs.push((text.clone(), width, height, images));
        }
        let images = &self.rich_glyphs[index.unwrap()].3;
        let paint_decorations = |pixels: &mut [u32], background: bool| {
            for d in images.1.iter().filter(|d| d.background == background) {
                let left = (bounds.x + d.bounds.x).floor().max(x0 as f32) as usize;
                let top = (bounds.y + d.bounds.y).floor().max(y0 as f32) as usize;
                let right = (bounds.x + d.bounds.x + d.bounds.width)
                    .ceil()
                    .min(x1 as f32)
                    .max(0.) as usize;
                let bottom = (bounds.y + d.bounds.y + d.bounds.height)
                    .ceil()
                    .min(y1 as f32)
                    .max(0.) as usize;
                for y in top..bottom {
                    for x in left..right {
                        blend(
                            &mut pixels[y * self.width + x],
                            d.color,
                            effects.opacity * edge_alpha(y as f32, bounds, effects.edge_fade),
                        );
                    }
                }
            }
        };
        paint_decorations(&mut self.pixels, true);
        for (x, y, color, image) in &images.0 {
            let gx = bounds.x.floor() as i32 + x + image.placement.left;
            let gy = bounds.y.floor() as i32 + y - image.placement.top;
            let color = &selected_ink(&self.selected, bounds, gx, gy, image, *color);
            for by in 0..image.placement.height as usize {
                let py = gy + by as i32;
                if py < y0 as i32 || py >= y1 as i32 {
                    continue;
                }
                for bx in 0..image.placement.width as usize {
                    let px = gx + bx as i32;
                    if px < x0 as i32 || px >= x1 as i32 {
                        continue;
                    }
                    let i = by * image.placement.width as usize + bx;
                    let (ink, alpha) = match image.content {
                        cosmic_text::SwashContent::Mask => (*color, image.data[i] as f32 / 255.),
                        cosmic_text::SwashContent::Color => (
                            Color(
                                image.data[i * 4],
                                image.data[i * 4 + 1],
                                image.data[i * 4 + 2],
                                color.3,
                            ),
                            image.data[i * 4 + 3] as f32 / 255.,
                        ),
                        cosmic_text::SwashContent::SubpixelMask => {
                            (*color, image.data[i * 4] as f32 / 255.)
                        }
                    };
                    blend(
                        &mut self.pixels[py as usize * self.width + px as usize],
                        ink,
                        effects.opacity * edge_alpha(py as f32, bounds, effects.edge_fade) * alpha,
                    );
                }
            }
        }
        paint_decorations(&mut self.pixels, false);
        if oversized {
            self.rich_glyphs.pop();
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn styled_text(
        &mut self,
        text: &str,
        bounds: Rect,
        clip: Rect,
        color: Color,
        size: f32,
        effects: Effects,
        font: &zgx::text_layout::FontStyle,
    ) {
        let Some(clip) = intersect(bounds, clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(clip);
        // Bound retained shaped data; default-font text keeps the lightweight fontdue path.
        let key = (
            text.to_owned(),
            size.to_bits(),
            bounds.width.to_bits(),
            bounds.height.to_bits(),
            font.clone(),
        );
        let mut oversized = false;
        if !self.styled_glyphs.contains_key(&key) {
            let images = self
                .styled_fonts
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .rasterize(text, size, bounds.width, bounds.height, font);
            let new_bytes = styled_bytes(&key, &images);
            let bytes: usize = self
                .styled_glyphs
                .iter()
                .map(|(key, images)| styled_bytes(key, images))
                .sum();
            if self.styled_glyphs.len() >= 128
                || bytes.saturating_add(new_bytes) > STYLED_GLYPH_BUDGET
            {
                self.styled_glyphs.clear();
            }
            oversized = new_bytes > STYLED_GLYPH_BUDGET;
            self.styled_glyphs.insert(key.clone(), images);
        }
        for (x, y, image) in &self.styled_glyphs[&key] {
            let gx = bounds.x.floor() as i32 + x + image.placement.left;
            let gy = bounds.y.floor() as i32 + y - image.placement.top;
            let color = selected_ink(&self.selected, bounds, gx, gy, image, color);
            for by in 0..image.placement.height as usize {
                let py = gy + by as i32;
                if py < y0 as i32 || py >= y1 as i32 {
                    continue;
                }
                for bx in 0..image.placement.width as usize {
                    let px = gx + bx as i32;
                    if px < x0 as i32 || px >= x1 as i32 {
                        continue;
                    }
                    let i = by * image.placement.width as usize + bx;
                    let (ink, alpha) = match image.content {
                        cosmic_text::SwashContent::Mask => (color, image.data[i] as f32 / 255.),
                        cosmic_text::SwashContent::Color => (
                            Color(
                                image.data[i * 4],
                                image.data[i * 4 + 1],
                                image.data[i * 4 + 2],
                                color.3,
                            ),
                            image.data[i * 4 + 3] as f32 / 255.,
                        ),
                        cosmic_text::SwashContent::SubpixelMask => {
                            (color, image.data[i * 4] as f32 / 255.)
                        }
                    };
                    blend(
                        &mut self.pixels[py as usize * self.width + px as usize],
                        ink,
                        effects.opacity * edge_alpha(py as f32, bounds, effects.edge_fade) * alpha,
                    );
                }
            }
        }
        if oversized {
            self.styled_glyphs.remove(&key);
        }
    }
    fn text(
        &mut self,
        text: &str,
        bounds: Rect,
        clip: Rect,
        color: Color,
        size: f32,
        effects: Effects,
    ) {
        let Some(clip) = intersect(bounds, clip) else {
            return;
        };
        let (x0, y0, x1, y1) = self.region(clip);
        let mut x = bounds.x;
        let mut baseline = bounds.y + size;
        let line_height = zgx::text_layout::LineHeight::default().resolve(size);
        for ch in text.chars() {
            if ch == '\n' {
                x = bounds.x;
                baseline += line_height;
                continue;
            }
            if self.glyphs.len() >= 4096 {
                self.glyphs.clear();
            }
            let (m, bitmap) = self
                .glyphs
                .entry((ch, size.to_bits()))
                .or_insert_with(|| self.font.rasterize(ch, size));
            if x + m.advance_width > bounds.x + bounds.width {
                x = bounds.x;
                baseline += line_height;
            }
            if baseline - size > clip.y + clip.height {
                break;
            }
            let gx = x.floor() as i32 + m.xmin;
            let gy = baseline.floor() as i32 - m.height as i32 - m.ymin;
            let color = self
                .selected
                .as_ref()
                .and_then(|h| {
                    h.glyph_color(
                        gx as f32 + m.width as f32 * 0.5 - bounds.x,
                        gy as f32 + m.height as f32 * 0.5 - bounds.y,
                    )
                })
                .unwrap_or(color);
            for by in 0..m.height {
                let py = gy + by as i32;
                if py < y0 as i32 || py >= y1 as i32 {
                    continue;
                }
                for bx in 0..m.width {
                    let px = gx + bx as i32;
                    if px < x0 as i32 || px >= x1 as i32 {
                        continue;
                    }
                    blend(
                        &mut self.pixels[py as usize * self.width + px as usize],
                        color,
                        effects.opacity
                            * edge_alpha(py as f32, bounds, effects.edge_fade)
                            * bitmap[by * m.width + bx] as f32
                            / 255.,
                    );
                }
            }
            x += m.advance_width;
        }
    }
    // Separable sliding-window box filter: O(area), independent of blur radius.
    fn blur(&mut self, bounds: Rect, radius: usize) {
        if radius == 0 {
            return;
        }
        let expanded = Rect {
            x: bounds.x - radius as f32,
            y: bounds.y - radius as f32,
            width: bounds.width + 2. * radius as f32,
            height: bounds.height + 2. * radius as f32,
        };
        let (x0, y0, x1, y1) = self.region(expanded);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let w = x1 - x0;
        let h = y1 - y0;
        self.scratch.resize(w * h, 0);
        for y in 0..h {
            let mut sum = [0u64; 3];
            let mut left = 0;
            let mut right = 0;
            for x in 0..w {
                let end = (x + radius + 1).min(w);
                while right < end {
                    add(&mut sum, self.pixels[(y + y0) * self.width + x0 + right]);
                    right += 1;
                }
                let start = x.saturating_sub(radius);
                while left < start {
                    sub(&mut sum, self.pixels[(y + y0) * self.width + x0 + left]);
                    left += 1;
                }
                self.scratch[y * w + x] = pack(sum, (right - left) as u64);
            }
        }
        let (bx0, by0, bx1, by1) = self.region(bounds);
        for x in bx0..bx1 {
            let mut sum = [0u64; 3];
            let mut top = 0;
            let mut bottom = 0;
            for y in 0..h {
                let end = (y + radius + 1).min(h);
                while bottom < end {
                    add(&mut sum, self.scratch[bottom * w + x - x0]);
                    bottom += 1;
                }
                let start = y.saturating_sub(radius);
                while top < start {
                    sub(&mut sum, self.scratch[top * w + x - x0]);
                    top += 1;
                }
                if y + y0 >= by0 && y + y0 < by1 {
                    self.pixels[(y + y0) * self.width + x] = pack(sum, (bottom - top) as u64);
                }
            }
        }
    }
    /// Write the frame to `path` as a binary PPM.
    pub fn write_ppm(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        write!(out, "P6\n{} {}\n255\n", self.width, self.height)?;
        for p in &self.pixels {
            out.write_all(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8])?;
        }
        Ok(())
    }
}
fn add(s: &mut [u64; 3], p: u32) {
    s[0] += ((p >> 16) & 255) as u64;
    s[1] += ((p >> 8) & 255) as u64;
    s[2] += (p & 255) as u64;
}
fn sub(s: &mut [u64; 3], p: u32) {
    s[0] -= ((p >> 16) & 255) as u64;
    s[1] -= ((p >> 8) & 255) as u64;
    s[2] -= (p & 255) as u64;
}
fn pack(s: [u64; 3], n: u64) -> u32 {
    ((s[0] / n) as u32) << 16 | ((s[1] / n) as u32) << 8 | (s[2] / n) as u32
}
fn blend(pixel: &mut u32, c: Color, opacity: f32) {
    let a = (c.3 as f32 * opacity.clamp(0., 1.)).round() as u32;
    let inv = 255 - a;
    let r = (c.0 as u32 * a + ((*pixel >> 16) & 255) * inv + 127) / 255;
    let g = (c.1 as u32 * a + ((*pixel >> 8) & 255) * inv + 127) / 255;
    let b = (c.2 as u32 * a + (*pixel & 255) * inv + 127) / 255;
    *pixel = (r << 16) | (g << 8) | b;
}
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = (a.x + a.width).min(b.x + b.width) - x;
    let height = (a.y + a.height).min(b.y + b.height) - y;
    (width > 0. && height > 0.).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}

fn edge_alpha(y: f32, bounds: Rect, fade: f32) -> f32 {
    if fade > 0. {
        ((y + 0.5 - bounds.y).min(bounds.y + bounds.height - y - 0.5) / fade).clamp(0., 1.)
    } else {
        1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgx::scene::{Effects, Layout, NodeKind, Style, Transform};
    #[test]
    fn a_selection_recolours_the_glyphs_it_holds_and_rounds_its_highlight() {
        let mut scene = Scene::new(120., 40.);
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "MMMMMM".into(),
                color: Color(255, 255, 255, 255),
                font_size: 20.,
            },
            Style {
                width: Some(120.),
                height: Some(30.),
                ..Default::default()
            },
        );
        let _ = scene.flush();
        let channels = |p: u32| [(p >> 16) as u8, (p >> 8) as u8, p as u8];
        let mut raster = Raster::new(120, 40);
        let full = [Rect::new(0., 0., 120., 40.)];
        raster.render(&scene, &full);
        let plain = raster.pixels.clone();
        scene.set_text_highlight(
            node,
            Some(zgx::scene::TextHighlight {
                color: Color(0, 0, 255, 255),
                rects: vec![Rect::new(0., 0., 36., 30.)],
                text_color: Some(Color(255, 0, 0, 255)),
                radius: 6.,
            }),
        );
        let _ = scene.flush();
        raster.render(&scene, &full);
        let ink = |from: usize, to: usize, test: fn([u8; 3]) -> bool| {
            (0..30).any(|y| (from..to).any(|x| test(channels(raster.pixels[y * 120 + x]))))
        };
        assert!(
            ink(0, 30, |p| p[0] > 200 && p[1] < 80 && p[2] < 80),
            "red glyphs"
        );
        assert!(
            !ink(0, 30, |p| p[0] > 200 && p[1] > 200 && p[2] > 200),
            "none white"
        );
        assert!(
            ink(40, 120, |p| p[0] > 200 && p[1] > 200 && p[2] > 200),
            "white glyphs"
        );
        assert_eq!(
            channels(raster.pixels[0]),
            channels(plain[0]),
            "a rounded corner"
        );
        assert_eq!(channels(raster.pixels[120 + 18]), [0, 0, 255]);
    }
    #[test]
    fn panel_background_border_and_children_have_correct_paint_order() {
        use zgx::scene::{Insets, QuadStyle};
        let mut scene = Scene::new(40., 40.);
        let panel = scene.append(
            scene.root(),
            NodeKind::Panel {
                layout: Layout::Row,
                quad: QuadStyle {
                    fill: Color(20, 40, 60, 255),
                    radius: 4.,
                    border_color: Color(200, 100, 0, 255),
                    border_width: 2.,
                    ..Default::default()
                },
            },
            Style {
                width: Some(40.),
                height: Some(40.),
                padding_edges: Some(Insets {
                    left: 10.,
                    top: 12.,
                    right: 3.,
                    bottom: 5.,
                }),
                ..Default::default()
            },
        );
        scene.append(
            panel,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(8.),
                height: Some(8.),
                ..Default::default()
            },
        );
        let mut raster = Raster::new(40, 40);
        let damage = scene.flush().damage;
        raster.render(&scene, &damage);
        assert_eq!(raster.pixels[6 * 40 + 6], 0x14283c);
        assert_eq!(raster.pixels[14 * 40 + 12], 0xff0000);
        assert_eq!(raster.pixels[40 + 20], 0xc86400);
    }
    fn scene() -> Scene {
        let mut s = Scene::new(100., 80.);
        s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
        s
    }
    fn rectangle(s: &mut Scene, x: f32, color: Color) -> zgx::scene::NodeId {
        let id = s.append(
            s.root(),
            NodeKind::Rect(color),
            Style {
                width: Some(30.),
                height: Some(30.),
                ..Default::default()
            },
        );
        s.set_transform(id, Transform { x, y: 10. });
        id
    }
    /// `scene`'s pixels drawn from its flushed damage, and drawn afresh.
    fn incremental_and_full(raster: &mut Raster, scene: &mut Scene) -> (Vec<u32>, Vec<u32>) {
        let damage = scene.flush().damage;
        raster.render(scene, &damage);
        let mut fresh = Raster::new(raster.width, raster.height);
        fresh.render(
            scene,
            &[Rect::new(0., 0., raster.width as f32, raster.height as f32)],
        );
        (raster.pixels.clone(), fresh.pixels)
    }
    #[test]
    fn colour_filters_tint_what_the_subtree_paints() {
        let mut s = scene();
        let red = rectangle(&mut s, 30., Color(255, 0, 0, 255));
        s.set_effects(
            red,
            Effects {
                brightness: 0.5,
                ..Effects::default()
            },
        );
        let mut raster = Raster::new(100, 80);
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        assert_eq!(pixels[25 * 100 + 45], 0x800000);
    }
    #[test]
    fn background_images_paint_between_fill_and_border() {
        let stripes = std::sync::Arc::new(
            zgx::image::ImageData::new(
                4,
                1,
                [
                    [255, 0, 0, 255],
                    [255, 0, 0, 255],
                    [0, 0, 255, 255],
                    [0, 0, 255, 255],
                ]
                .concat(),
            )
            .unwrap(),
        );
        let mut s = scene();
        let card = s.append(
            s.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: zgx::scene::QuadStyle {
                    fill: Color(50, 50, 50, 255),
                    border_color: Color(255, 255, 255, 255),
                    border_width: 2.,
                    decoration: Some(std::sync::Arc::new(zgx::decoration::Decoration {
                        background: Some(zgx::decoration::Background::Image {
                            image: stripes,
                            fit: zgx::style::ObjectFit::Contain,
                        }),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            },
            Style {
                width: Some(40.),
                height: Some(40.),
                ..Default::default()
            },
        );
        s.set_transform(card, Transform { x: 30., y: 10. });
        let mut raster = Raster::new(100, 80);
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        let at = |x: usize, y: usize| pixels[y * 100 + x];
        // Contained: 40 × 10 centred, the fill above, the border round all.
        assert_eq!(at(40, 30), 0xff0000);
        assert_eq!(at(60, 30), 0x0000ff);
        assert_eq!(at(50, 20), 0x323232);
        assert_eq!(at(50, 10), 0xffffff);
    }
    #[test]
    fn borders_over_content_paint_after_the_children() {
        for over in [false, true] {
            let mut s = scene();
            let frame = s.append(
                s.root(),
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: zgx::scene::QuadStyle {
                        fill: Color(20, 20, 20, 255),
                        border_color: Color(255, 255, 255, 255),
                        border_width: 3.,
                        border_over: over,
                        ..Default::default()
                    },
                },
                Style {
                    width: Some(40.),
                    height: Some(40.),
                    ..Default::default()
                },
            );
            s.set_transform(frame, Transform { x: 30., y: 10. });
            let fill = s.append(
                frame,
                NodeKind::Rect(Color(255, 0, 0, 255)),
                Style {
                    width: Some(40.),
                    height: Some(40.),
                    ..Default::default()
                },
            );
            // Over the border band: a layout border lays its content out
            // inside it, so move the child back onto the frame's edge.
            if !over {
                s.set_transform(fill, Transform { x: -3., y: -3. });
            }
            let mut raster = Raster::new(100, 80);
            let (pixels, full) = incremental_and_full(&mut raster, &mut s);
            assert_eq!(pixels, full);
            let at = |x: usize, y: usize| pixels[y * 100 + x];
            assert_eq!(
                at(50, 11),
                if over { 0xffffff } else { 0xff0000 },
                "over {over}"
            );
            assert_eq!(at(50, 30), 0xff0000);
        }
    }
    #[test]
    fn rounded_clips_cut_their_childrens_corners() {
        let mut s = scene();
        let card = s.append(
            s.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: zgx::scene::QuadStyle {
                    radius: 15.,
                    ..Default::default()
                },
            },
            Style {
                width: Some(40.),
                height: Some(40.),
                clip: true,
                ..Default::default()
            },
        );
        s.set_transform(card, Transform { x: 30., y: 10. });
        let red = s.append(
            card,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(40.),
                height: Some(40.),
                ..Default::default()
            },
        );
        let mut raster = Raster::new(100, 80);
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        let at = |pixels: &[u32], x: usize, y: usize| pixels[y * 100 + x];
        assert_eq!(at(&pixels, 50, 30), 0xff0000);
        assert_eq!(at(&pixels, 31, 11), 0x10141c);
        assert_eq!(at(&pixels, 68, 48), 0x10141c);
        assert_eq!(at(&pixels, 69, 30), 0xff0000);
        // Antialiased along the curve.
        let row: Vec<u32> = (30..40).map(|x| at(&pixels, x, 14) >> 16).collect();
        assert!(row.iter().any(|r| *r > 0x30 && *r < 0xf0), "{row:x?}");
        s.set_kind(red, NodeKind::Rect(Color(0, 0, 255, 255)));
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        assert_eq!(at(&pixels, 31, 11), 0x10141c);
    }
    #[test]
    fn content_blurs_spread_and_masks_hide_their_layers() {
        let mut s = scene();
        let red = rectangle(&mut s, 30., Color(255, 0, 0, 255));
        let mut raster = Raster::new(100, 80);
        incremental_and_full(&mut raster, &mut s);
        let at = |pixels: &[u32], x: usize, y: usize| pixels[y * 100 + x];
        s.set_filter_blur(red, 3.);
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        // Spread a few pixels past the box, not past three sigma.
        assert!((at(&pixels, 27, 25) >> 16) & 255 > 0x30);
        assert_eq!(at(&pixels, 19, 25), 0x10141c);
        assert!((at(&pixels, 45, 25) >> 16) & 255 > 0xf0);
        // Masked to its left half, then the band slid right.
        s.set_filter_blur(red, 0.);
        s.set_mask(
            red,
            Some(zgx::scene::ImageMask {
                image: std::sync::Arc::new(
                    zgx::image::ImageData::new(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 0]).unwrap(),
                ),
                offset: (0., 0.),
                size: (30., 30.),
            }),
        );
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        assert_eq!(at(&pixels, 35, 25), 0xff0000);
        assert_eq!(at(&pixels, 50, 25), 0x10141c);
        s.set_mask_offset(red, (15., 0.));
        let (pixels, full) = incremental_and_full(&mut raster, &mut s);
        assert_eq!(pixels, full);
        assert_eq!(at(&pixels, 35, 25), 0x10141c);
        assert_eq!(at(&pixels, 50, 25), 0xff0000);
    }
    #[test]
    fn rotated_layers_sample_through_their_matrix_and_repair_damage() {
        let mut s = scene();
        let background = s.root();
        let square = rectangle(&mut s, 35., Color(255, 0, 0, 255));
        let mut raster = Raster::new(100, 80);
        let damage = s.flush().damage;
        raster.render(&s, &damage);
        s.set_local_transform(
            square,
            zgx::scene::LocalTransform {
                rotate: std::f32::consts::FRAC_PI_4,
                ..Default::default()
            },
        );
        let damage = s.flush().damage;
        raster.render(&s, &damage);
        // A diamond about (50, 25), reaching 21 px: above the laid-out box
        // (y 10..40), but out of its corners.
        assert_eq!(raster.pixels[6 * 100 + 50], 0xff0000);
        assert_eq!(raster.pixels[12 * 100 + 37], 0x10141c);
        let incremental = raster.pixels.clone();
        raster.render(&s, &[s.bounds(background)]);
        assert_eq!(incremental, raster.pixels);
    }
    #[test]
    fn incremental_matches_full_after_move_opacity_and_removal() {
        let mut s = scene();
        rectangle(&mut s, 5., Color(80, 20, 20, 255));
        let top = rectangle(&mut s, 20., Color(0, 200, 255, 150));
        let mut partial = Raster::new(100, 80);
        let mut full = Raster::new(100, 80);
        let damage = s.flush().damage;
        partial.render(&s, &damage);
        for step in 0..4 {
            match step {
                0 => s.set_transform(top, Transform { x: 40., y: 20. }),
                1 => s.set_effects(
                    top,
                    Effects {
                        opacity: 0.4,
                        ..Default::default()
                    },
                ),
                2 => s.remove(top),
                _ => {}
            }
            let damage = s.flush().damage;
            partial.render(&s, &damage);
            full.render(&s, &[Rect::new(0., 0., 100., 80.)]);
            assert!(
                partial.pixels == full.pixels,
                "incremental image differs from full repaint at step {step}"
            );
        }
    }
    #[test]
    fn blur_and_edge_fade_change_pixels_and_stay_stable() {
        let mut s = scene();
        rectangle(&mut s, 10., Color(255, 255, 255, 255));
        let glass = rectangle(&mut s, 25., Color(50, 80, 160, 100));
        s.set_effects(
            glass,
            Effects {
                opacity: 0.8,
                blur_radius: 4.,
                edge_fade: 8.,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        );
        let mut r = Raster::new(100, 80);
        let damage = s.flush().damage;
        r.render(&s, &damage);
        let expected = r.pixels.clone();
        r.render(&s, &[Rect::new(0., 0., 100., 80.)]);
        assert_eq!(expected, r.pixels);
        s.set_effects(glass, Effects::default());
        let damage = s.flush().damage;
        r.render(&s, &damage);
        assert_ne!(expected, r.pixels);
    }
}

/// `color` through colour filters (`PaintItem::color`), as the GPU applies
/// them to straight RGB. Images and rich text keep their colours here.
fn tint(color: Color, matrix: Option<zgx::scene::ColorMatrix>) -> Color {
    let Some(m) = matrix else {
        return color;
    };
    let rgb = [color.0, color.1, color.2].map(|c| c as f32 / 255.);
    let row = |r: [f32; 4]| {
        let value = r[0] * rgb[0] + r[1] * rgb[1] + r[2] * rgb[2] + r[3];
        (value.clamp(0., 1.) * 255.).round() as u8
    };
    Color(row(m[0]), row(m[1]), row(m[2]), color.3)
}
/// The size an image of `source_w` × `source_h` shows at in `bounds` as
/// `fit` places it.
fn fitted(source_w: f32, source_h: f32, bounds: Rect, fit: zgx::style::ObjectFit) -> (f32, f32) {
    use zgx::style::ObjectFit;
    let (horizontal, vertical) = (bounds.width / source_w, bounds.height / source_h);
    let scale = match fit {
        ObjectFit::Fill => return (bounds.width, bounds.height),
        ObjectFit::None => return (source_w, source_h),
        ObjectFit::Cover => horizontal.max(vertical),
        ObjectFit::ScaleDown => horizontal.min(vertical).min(1.),
        ObjectFit::Contain => horizontal.min(vertical),
    };
    (source_w * scale, source_h * scale)
}
/// What of `style` an item paints when its border is drawn over its content
/// (`QuadStyle::border_over`): the border alone in the `border_pass`, the
/// rest before it; otherwise all of it.
fn border_part(style: &zgx::scene::QuadStyle, border_pass: bool) -> zgx::scene::QuadStyle {
    use zgx::decoration::{Background, Decoration};
    if !style.border_over || !style.has_border() {
        return style.clone();
    }
    let detail = style.decoration.as_deref().cloned();
    if border_pass {
        let clear = Color(0, 0, 0, 0);
        zgx::scene::QuadStyle {
            fill: clear,
            shadow: None,
            decoration: detail.map(|detail| {
                std::sync::Arc::new(Decoration {
                    background: Some(Background::Brush(zgx::canvas::Brush::Solid(clear))),
                    shadows: Some(std::sync::Arc::new([])),
                    ..detail
                })
            }),
            ..style.clone()
        }
    } else {
        zgx::scene::QuadStyle {
            border_width: 0.,
            decoration: detail.map(|detail| {
                std::sync::Arc::new(Decoration {
                    border_widths: None,
                    ..detail
                })
            }),
            ..style.clone()
        }
    }
}
/// How much of the pixel centred at (`x`, `y`) `round` keeps: 1 inside, 0
/// outside, antialiased over a pixel at its curves.
fn rounded_coverage(round: &zgx::scene::RoundRect, x: f32, y: f32) -> f32 {
    let r = round.rect;
    let (half_w, half_h) = (r.width / 2., r.height / 2.);
    let (qx, qy) = (x - r.x - half_w, y - r.y - half_h);
    let [tl, tr, br, bl] = round.radii;
    let radius = match (qx > 0., qy > 0.) {
        (false, false) => tl,
        (true, false) => tr,
        (true, true) => br,
        (false, true) => bl,
    };
    let (dx, dy) = (qx.abs() - half_w + radius, qy.abs() - half_h + radius);
    let outside = (dx.max(0.).powi(2) + dy.max(0.).powi(2)).sqrt() + dx.max(dy).min(0.) - radius;
    (0.5 - outside).clamp(0., 1.)
}
fn rounded_distance(x: f32, y: f32, bounds: Rect, radius: f32) -> f32 {
    let r = radius.max(0.).min(bounds.width.min(bounds.height) * 0.5);
    let qx = (x - bounds.x - bounds.width * 0.5).abs() - bounds.width * 0.5 + r;
    let qy = (y - bounds.y - bounds.height * 0.5).abs() - bounds.height * 0.5 + r;
    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - r
}

#[cfg(test)]
mod extended_tests {
    use super::*;
    use zgx::scene::{BoxShadow, Layout, QuadStyle, Style, Transform};
    #[test]
    fn shadow_damage_and_clipped_images_match_full_repaint() {
        let mut raster = Raster::new(80, 60);
        let mut scene = Scene::new(80., 60.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let quad = scene.append(
            scene.root(),
            NodeKind::Quad(QuadStyle {
                fill: Color(230, 30, 40, 255),
                radius: 8.,
                border_color: Color(0, 255, 0, 255),
                border_width: 2.,
                shadow: Some(BoxShadow {
                    color: Color(0, 0, 0, 160),
                    offset: Transform { x: -8., y: 3. },
                    blur_radius: 3.,
                    spread: 2.,
                }),
                decoration: None,
                border_over: false,
                outline: None,
            }),
            Style {
                width: Some(24.),
                height: Some(24.),
                ..Default::default()
            },
        );
        scene.set_transform(quad, Transform { x: 20., y: 10. });
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        scene.set_transform(quad, Transform { x: 45., y: 25. });
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        let partial = raster.pixels.clone();
        raster.render(&scene, &[Rect::new(0., 0., 80., 60.)]);
        assert_eq!(partial, raster.pixels);
        let image =
            std::sync::Arc::new(zgx::image::ImageData::new(1, 1, vec![255, 0, 255, 255]).unwrap());
        scene.append(
            scene.root(),
            NodeKind::Image(image),
            Style {
                width: Some(10.),
                height: Some(10.),
                ..Default::default()
            },
        );
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        assert_eq!(raster.pixels[0], 0xff00ff);
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    use zgx::scene::{Layout, Style, Transform};
    #[test]
    fn opacity_is_applied_once_to_overlapping_children() {
        let mut scene = Scene::new(40., 30.);
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(20.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_isolated(group, true);
        scene.set_effects(
            group,
            Effects {
                opacity: 0.5,
                ..Default::default()
            },
        );
        scene.append(
            group,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(20.),
                height: Some(20.),
                ..Default::default()
            },
        );
        let blue = scene.append(
            group,
            NodeKind::Rect(Color(0, 0, 255, 255)),
            Style {
                width: Some(12.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_transform(blue, Transform { x: 8., y: 0. });
        let mut raster = Raster::new(40, 30);
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        let p = raster.pixels[10 * 40 + 12];
        assert!(
            ((p >> 16) & 255) < 20,
            "red backdrop must not leak through the isolated blue child"
        );
        assert!((p & 255) > 130);
        scene.set_transform(group, Transform { x: 10., y: 5. });
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        let partial = raster.pixels.clone();
        raster.render(&scene, &[Rect::new(0., 0., 40., 30.)]);
        assert_eq!(partial, raster.pixels);
    }
}

#[cfg(test)]
mod line_height_tests {
    use super::*;
    use zgx::{
        scene::Style,
        text_layout::{FontStyle, LineHeight},
    };

    #[test]
    fn changing_line_pitch_repairs_old_pixels_and_keys_software_shapes() {
        let mut scene = Scene::new(120., 100.);
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "M\nM".into(),
                color: Color(255, 255, 255, 255),
                font_size: 18.,
            },
            Style {
                width: Some(80.),
                height: Some(90.),
                ..Default::default()
            },
        );
        let mut incremental = Raster::new(120, 100);
        for pitch in [40., 8., 24., 40.] {
            scene.set_font(
                node,
                FontStyle {
                    line_height: LineHeight::px(pitch),
                    ..Default::default()
                },
            );
            let damage = scene.flush().damage;
            assert!(!damage.is_empty());
            incremental.render(&scene, &damage);
            let mut fresh = Raster::new(120, 100);
            fresh.render(&scene, &[Rect::new(0., 0., 120., 100.)]);
            assert_eq!(incremental.pixels, fresh.pixels, "pitch {pitch}");
            assert!(
                incremental
                    .styled_glyphs
                    .keys()
                    .any(|key| key.4.line_height == LineHeight::px(pitch))
            );
            let cache_size = incremental.styled_glyphs.len();
            scene.set_font(
                node,
                FontStyle {
                    line_height: LineHeight::px(pitch),
                    ..Default::default()
                },
            );
            let idle = scene.flush().damage;
            assert!(idle.is_empty());
            incremental.render(&scene, &idle);
            assert_eq!(incremental.styled_glyphs.len(), cache_size);
        }
    }
}

#[cfg(test)]
mod letter_spacing_tests {
    use super::*;
    use zgx::{
        scene::Style,
        text_layout::{FontStyle, LetterSpacing},
    };

    #[test]
    fn tracking_changes_repair_old_glyphs_and_distinguish_cached_font_keys() {
        let mut scene = Scene::new(240., 70.);
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "MMMM".into(),
                color: Color(255, 255, 255, 255),
                font_size: 18.,
            },
            Style {
                width: Some(220.),
                height: Some(50.),
                ..Default::default()
            },
        );
        let mut incremental = Raster::new(240, 70);
        let mut previous = None;
        for spacing in [3., -1., -100., 3.] {
            scene.set_font(
                node,
                FontStyle {
                    letter_spacing: LetterSpacing::px(spacing),
                    ..Default::default()
                },
            );
            let damage = scene.flush().damage;
            incremental.render(&scene, &damage);
            let mut fresh = Raster::new(240, 70);
            fresh.render(&scene, &[Rect::new(0., 0., 240., 70.)]);
            assert_eq!(incremental.pixels, fresh.pixels);
            if let Some(old) = previous.replace(incremental.pixels.clone()) {
                assert_ne!(
                    old, incremental.pixels,
                    "tracking must move actual glyph ink"
                );
            }
            assert!(
                incremental
                    .styled_glyphs
                    .keys()
                    .any(|key| key.4.letter_spacing == LetterSpacing::px(spacing))
            );
        }
        assert_eq!(incremental.styled_glyphs.len(), 3);
    }
}

#[cfg(test)]
mod hidden_effect_tests {
    use super::*;
    use zgx::scene::{Layout, Style};

    #[test]
    fn transparent_subtrees_do_not_blur_backdrop_or_rasterize_text() {
        for isolated in [false, true] {
            let mut scene = Scene::new(60., 40.);
            scene.append(
                scene.root(),
                NodeKind::Rect(Color(255, 255, 255, 255)),
                Style {
                    width: Some(30.),
                    height: Some(40.),
                    absolute: true,
                    ..Default::default()
                },
            );
            let mut raster = Raster::new(60, 40);
            let damage = scene.flush().damage;
            raster.render(&scene, &damage);
            let baseline = raster.pixels.clone();
            let group = scene.append(
                scene.root(),
                NodeKind::Container(Layout::Overlay),
                Style {
                    width: Some(60.),
                    height: Some(40.),
                    absolute: true,
                    ..Default::default()
                },
            );
            scene.set_isolated(group, isolated);
            scene.set_effects(
                group,
                Effects {
                    opacity: 0.,
                    blur_radius: 5.,
                    ..Default::default()
                },
            );
            scene.append(
                group,
                NodeKind::Text {
                    text: "Hidden".into(),
                    color: Color(255, 0, 0, 255),
                    font_size: 16.,
                },
                Style::default(),
            );
            let damage = scene.flush().damage;
            raster.render(&scene, &damage);
            assert!(
                raster.pixels == baseline,
                "hidden subtree changed backdrop; isolated={isolated}"
            );
            assert!(raster.glyphs.is_empty());
            assert!(raster.scratch.is_empty());
            scene.set_effects(
                group,
                Effects {
                    opacity: 1.,
                    blur_radius: 5.,
                    ..Default::default()
                },
            );
            let damage = scene.flush().damage;
            raster.render(&scene, &damage);
            assert_ne!(raster.pixels, baseline);
            scene.set_effects(
                group,
                Effects {
                    opacity: 0.,
                    blur_radius: 5.,
                    ..Default::default()
                },
            );
            let damage = scene.flush().damage;
            raster.render(&scene, &damage);
            assert_eq!(raster.pixels, baseline);
        }
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    use zgx::scene::{Layout, Outline, QuadStyle, Style, Transform};
    #[test]
    fn outlines_ring_boxes_past_their_clip_and_repaint_where_they_were() {
        let mut raster = Raster::new(80, 80);
        let mut scene = Scene::new(80., 80.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let quad = |offset| QuadStyle {
            fill: Color(20, 20, 20, 255),
            outline: Some(Outline {
                width: 4.,
                color: Color(255, 255, 255, 255),
                offset,
            }),
            ..Default::default()
        };
        let panel = scene.append(
            scene.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: quad(2.),
            },
            Style {
                width: Some(40.),
                height: Some(40.),
                clip: true,
                ..Default::default()
            },
        );
        scene.append(
            panel,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(40.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_transform(panel, Transform { x: 20., y: 20. });
        let at = |raster: &Raster, x: usize, y: usize| raster.pixels[y * 80 + x];
        let clear = raster.background;
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        // 2 to 6 outside the box, square corners square, over nothing.
        assert_eq!(at(&raster, 15, 40), 0xffffff);
        assert_eq!(at(&raster, 19, 40), clear);
        assert_eq!(at(&raster, 21, 40), 0xff0000);
        assert_eq!(at(&raster, 15, 15), 0xffffff);
        // Centred (offset -2): over the child, 2 each side of the edge.
        scene.set_kind(
            panel,
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: quad(-2.),
            },
        );
        let d = scene.flush().damage;
        raster.render(&scene, &d);
        let partial = raster.pixels.clone();
        raster.render(&scene, &[Rect::new(0., 0., 80., 80.)]);
        assert_eq!(partial, raster.pixels, "incremental differs from full");
        assert_eq!(at(&raster, 15, 40), clear);
        assert_eq!(at(&raster, 19, 40), 0xffffff);
        assert_eq!(at(&raster, 21, 40), 0xffffff);
        assert_eq!(at(&raster, 23, 40), 0xff0000);
    }
}

/// A glyph image's ink colour: the selection's text colour where the glyph's
/// centre lies in the highlight on its node at `bounds`.
fn selected_ink(
    selected: &Option<zgx::scene::TextHighlight>,
    bounds: Rect,
    gx: i32,
    gy: i32,
    image: &cosmic_text::SwashImage,
    color: Color,
) -> Color {
    selected
        .as_ref()
        .and_then(|h| {
            h.glyph_color(
                gx as f32 + image.placement.width as f32 * 0.5 - bounds.x,
                gy as f32 + image.placement.height as f32 * 0.5 - bounds.y,
            )
        })
        .unwrap_or(color)
}
