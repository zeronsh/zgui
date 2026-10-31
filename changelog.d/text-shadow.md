---
section: added
---
- **Text shadows** (zgx, zgx-script): `Styled::text_shadow(x, y, blur, color)` and `text_shadows`, Luau `text_shadow = { x, y, blur, color }` or a list, as CSS's `text-shadow`: the glyphs again under them, moved and tinted, inherited by the text inside. `Scene::set_text_shadows` for scenes built directly. `blur` blurs it (a Gaussian's sigma in points): each glyph's coverage is blurred once and kept in the glyph atlas, so a blurred shadow costs a frame what a sharp one does, on every backend. Shadows paint outside the text's box, as CSS's do (a text itself stays clipped to its box).
