# Dotted borders, dashed and dotted outlines, Oct 31 2026

`borders.png`: `GpuRenderer` offscreen at 2x, three 90 by 80 boxes with
14-point corners. Left, a 4-point border, `BorderStyle::DOTTED`. Middle, a
3-point outline 4 points out, dotted (`Scene::set_outline_style`). Right,
the same outline dashed, `{ length = 10, gap = 6 }`. Dots and dashes follow
the rounded corners; dots spread evenly all round.
