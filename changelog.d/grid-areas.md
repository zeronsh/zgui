---
section: added
---
- **Grid areas, named lines and dense flow** (zgx, zgx-script): `areas = { "head head", "side main" }` names a grid's cells and `area = "main"` places an item in one (CSS `grid-template-areas`, `grid-area`). `[name]` in `cols` and `rows` names a line, `grid_column` and `grid_row` take CSS's `"side / main"` and `"2 / span 3"`, and `flow = "dense"` fills holes. In Rust: `grid_template_areas`, `grid_area`, `grid_column`, `grid_row`, `grid_flow`.
