---
section: added
---
- **Dotted borders, and dashed and dotted outlines** (zgx, zgx-gpu, zgx-script): `BorderStyle::DOTTED` (a `Dashed` of length 0) draws round dots as wide as the border, as far apart, spread evenly round the box and its rounded corners; Luau `border_style = "dotted"`. `Styled::outline_style` (and `Scene::set_outline_style`) draws an outline dashed or dotted, as CSS's `outline-style`; Luau `outline_style = "dotted"` and `outline_dash = { length, gap }`. A solid outline is the shader's ring as before; a broken one is rasterized once and kept. A canvas stroke's dash may now be 0 long (a dot, with round caps), as an HTML canvas's `setLineDash` takes.
