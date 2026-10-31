---
section: added
---
- **Gradient text** (zgx, zgx-gpu, zgx-script): `Styled::text_gradient(Gradient)` (`Scene::set_text_gradient`), Luau `text_gradient` and `text_radial`: a text's glyphs filled with a linear or radial gradient over its box (CSS's `background-clip: text`). The text paints into a cached layer of its own, where its glyphs draw, the gradient is kept where they cover (a source-in blend) and its selection, marks and shadows draw behind them (destination-over); selected glyphs then take the selected-text colour. The same shader as every quad, two more blend states (wgpu and Direct3D 11). Glyphs rasterize at the layer's scale, and the gradient's colours are made again only when the box resizes, so a counter re-uploads nothing but glyphs. Text without a gradient is no layer.
