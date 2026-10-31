# Edge fades ease in (smoothstep), Oct 31 2026

`fade` / `fade_sides` (`fade_edges`) bands were `t²`, which reaches full
opacity still rising at slope 2: the band's inner end showed as a line (a
Mach band). They are smoothstep now, `t²(3 − 2t)`, level at both ends.

Rendered by `GpuRenderer` offscreen (wgpu, Metal) at 1x, shown 2x: a 320 by
180 box with `overflow_hidden` and a 72 px bottom band, over #111318.

- `panel-compare.png`: a flat #d0d6e0 panel.
- `rows-compare.png`: rows of text in the same box.

Down the panel's band (x 180, y 120 to 201), its red channel:

- before: 208 208 208 208 208 208 208 208 **205 200 195 190 185** 180 175 …
- after: 208 208 208 208 208 208 208 208 208 208 **207 207 206 205 204** 202 …

Before, full level then −5 a pixel at once; after, it leaves the content
level. Smoothstep also holds nearer full opacity a little longer into the
band and falls more steeply in its middle.
