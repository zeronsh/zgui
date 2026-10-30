# Styling retained views

zgx's Rust API combines GPUI/QuickGUI-style fluent methods with owned component children. Import `Styled` to use the methods on a `View` or reusable `Styles` patch. Values are logical pixels except explicit percentage dimensions; spacing methods take pixel values directly, not a numbered spacing scale.

```rust
use zgx::compose::{button, column, text};
use zgx::style::{rgb, StrokeAlign, Styled, Styles};

let card = column()
    .w(320.)
    .p(16.)
    .gap(12.)
    .bg(rgb(0x202838))
    .rounded(10.)
    .border_1()
    .border_color(rgb(0x465268))
    .text_color(rgb(0xf0f4ff))
    .text_size(16.)
    .child(text("Connection settings").text_size(22.))
    .child(button()
        .px(14.).py(8.)
        .bg(rgb(0x2854a0))
        .hover(|s| s.bg(rgb(0x3266bd)))
        .active(|s| s.bg(rgb(0x1d407e)))
        .focus(|s| s.stroke(1., rgb(0x9abfff), StrokeAlign::Inside))
        .disabled_style(|s| s.opacity(0.45))
        .child(text("Connect"))
        .on_click(|| println!("connect")));
```

`.active(..)` applies while a press on the view is held, as a native button's pressed look does. A press dragged off the view looks let go, and dragged back on it looks pressed again, since letting go there still clicks. A press let go elsewhere is over: coming back later is only a hover.

Mount with `Ui::mount(card)` or use the native window's view rendering entry point. A background, rounded border, or shadow decorates the container itself; it does not add a layout child. Children remain retained when a visual style changes.

## Box model

Sizes are border-box, as CSS's `box-sizing: border-box` and GPUI have them. A view's `w` and `h` include its border; its padding and children lay out inside the border, each side's width counted on its own (`.border_edges(Insets { .. })`); and a view sized by its content is its content, padding and border. The card above, `w(320.).p(16.).border_1()`, lays its children out 286 wide, 17 in from each edge. A transparent border still takes its width, so changing only a border's colour never moves anything; changing its width lays the view out again. Rings that only paint take no room: `.border_over(true)` (an inset stroke drawn over the content), `.outline(..)` and `.stroke(..)` with any alignment. Use one of those for a focus or selection ring that should not move content; controls' default focus rings are inset strokes, or recolour the control's own border when it has one.

An `overlay()` stacks its children in one box, as CSS grid lays out items sharing one area. Each child is sized against the overlay's own box (percentages count from it) and aligned in all of it on its own: `items_*` across, `justify_*` down (`justify_between` acts as start, `justify_around` and `justify_evenly` as centre, as they do for one item). By default children stretch to fill it both ways (`items_stretch().justify_stretch()`), except along an axis a child sizes itself on; `.items_center().justify_center()` centres an icon in a tile. The overlay itself is as large as its largest child.

A `scroll(..)` or `scroll_x(..)` view is 320 by 240 when nothing else sizes it, but those are intrinsic sizes, not constraints: a parent stretching its children (`items_stretch`, an overlay) stretches it across, `grow` grows it from them, and `w`/`h`/`w_full` replace them. Its content fills its cross axis (a vertical scroll's width, a horizontal one's height) unless it sizes itself there. Editors size like CSS form controls in flex: `text_input` and `text_area` are 240 wide by default, an intrinsic width their text never changes (long text scrolls inside, keeping the caret in view); a stretching parent widens them, and they shrink to nothing in a row too narrow for them (as every box shrinks, but an editor scrolls its text, so its content needs no room; `.shrink_0()` holds them).

## Properties and patches

| Area | Methods |
| --- | --- |
| Dimensions | `w`, `h`, `size`, `w_percent`, `h_percent`, `w_full`, `h_full`, `min_w`, `max_w`, `min_h`, `max_h` |
| Padding | `p`, `px`, `py`, `pt`, `pr`, `pb`, `pl` |
| Margins | `m`, `mx`, `my`, `mt`, `mr`, `mb`, `ml` |
| Layout | `flex_row`, `flex_col`, `gap`, `flex_grow`, `flex_shrink`, `grow`, `shrink_0`, `absolute`, `relative`, `left`, `top`, `right`, `bottom`, `start`, `end` (logical insets) |
| Direction | `rtl`, `direction(Direction)`; logical edges `ps`, `pe`, `ms`, `me` (see [layout](layout.md)) |
| Alignment | `items_start`, `items_center`, `items_end`, `items_stretch`, `justify_start`, `justify_center`, `justify_end`, `justify_between`, `justify_around`, `justify_stretch` |
| Surface | `bg`, `rounded`, `border`, `border_1`, `border_edges`, `border_color`, `border_over`, `outline`, `stroke`, `shadow`, `shadow_none` |
| Text | `text_color`, `text_size`, `line_height`, `line_height_normal`, `letter_spacing`, `text_wrap`, `font_family`, `font_weight`, `font_bold`, `italic` |
| Effects | `opacity`, `blur`, `filter_blur`, `mask`, `mask_offset`, `edge_fade`, `translate`, `scale`, `scale_xy`, `rotate`, `skew`, `rotate_x`, `rotate_y`, `perspective`, `transform_origin`, `blend`, `layer_shader`, `layer_uniforms`, `isolated` |
| Clipping | `overflow_hidden`, `overflow_visible` |

`Styles::new()` is an empty patch. `.style(patch)` changes only fields supplied by that patch. Later writes win, including individual edges: `.p(8.).px(12.).pt(3.)` produces top 3, right 12, bottom 8, left 12. A later `.pl(20.)` preserves the other three edges. Per-edge padding participates in intrinsic sizing, flex allocation, and resizing.

```rust
use zgx::style::{rgb, Styled, Styles};
let surface = Styles::new().p(12.).bg(rgb(0x202838)).rounded(8.);
let compact = surface.clone().py(6.); // retains horizontal padding and surface
```

Use `.apply(...)` for reusable style functions, `.when(...)` for a conditional transformation, and `.when_some(...)` for an optional value. These helpers work on both views and sparse `Styles` patches; skipped closures are never called.

```rust
use zgx::compose::{column, text};
use zgx::style::{rgb, Styled, Styles};

fn card_surface<S: Styled>(style: S) -> S {
    style.p(16.).bg(rgb(0x202838)).rounded(10.)
}

let compact = true;
let width = Some(320.);
let card = column()
    .apply(card_surface)
    .when(compact, |view| view.py(8.))
    .when_some(width, |view, width| view.w(width))
    .child(text("Connection settings"));
let reusable_patch = Styles::new().apply(card_surface);
```

These decisions run once when constructing the description. They do not subscribe to signals or reapply after mounting. For styles that change with state, return a patch from `reactive_style`; the same helpers can be used inside that tracked callback.

Text color, size, wrapping, font selection, line height and letter spacing inherit through children and components. A child can override each field independently. Updating an inherited value updates the affected text properties; it does not rerun component construction. Explicit overrides shield descendants from unchanged effective typography. Other style properties are local rather than inherited. Padded or decorated text mounts a retained box around its text node; plain text stays a single node. Static and reactive wrapping propagate through boxed text and reflow its retained text node when the available width changes.

## Percentage dimensions

`.w_percent(50.)` takes half of the parent's definite content width; `.h_percent(50.)`
does the same for height. `.w_full()` and `.h_full()` mean 100 percent. These
methods take percentages (50 means 50%, not 0.5), while `.w()` and `.h()` retain
their logical-pixel units. The parent's padding is excluded from the reference
size; the resulting child size includes the child's own padding. Margins and gaps
are separate, so two 50-percent children plus a gap can overflow unless flex
shrink is enabled.

```rust
use zgx::compose::prelude::*;
let content = row().w_full().h(240.).p(16.)
    .child(column().w_percent(40.).h_full().child("Navigation"))
    .child(column().w_percent(60.).h_full().child("Content"));
```

Pixel and percentage values replace one another per axis, including sparse style
patches and interaction variants. `.w(200.).w_percent(50.)` uses 50 percent;
a subsequent `.w(80.)` restores a pixel width. Removing a reactive override restores
the base style. Resizing a parent recomputes dependent child allocation without
reconstructing components or losing editor state. Images, ordinary scrollers and
virtual-list viewports accept the same dimension methods.

A percentage reference is definite when its parent dimension is explicit,
resolved from a percentage, or allocated by flex/stretch (the root uses the
window viewport). A mere available-space constraint or min/max bound does not
make an intrinsic axis definite. On indefinite axes, percentages fall back to
automatic intrinsic sizing, including ordinary cross-axis stretch; layout does
not iterate to solve cyclic percentage sizes. Absolute children use their
parent's final content box because they do not contribute to its intrinsic size.
For example, percentage height inside the content of a vertical scroller usually
has an indefinite reference, while its viewport can have a definite height.

Flex grow/shrink allocation and logical-pixel min/max constraints still apply.
Values above 100 percent are allowed; negative and non-finite values normalize to
zero. Only width and height currently expose percentage units; percentage
padding, margins and min/max dimensions are not provided.

## Zoom

`zoom(n)` zooms what a node holds, as CSS's `zoom`: its lengths (sizes, padding, gaps, borders, radii, shadows, text) show `n` times as large, laid out again at that size. The node lays its subtree out in its own points, under its parent's room divided by `n`, and takes `n` times the room it needs there, so the content around it reflows around the zoomed size. It is drawn at the scale it shows at, crisp, not resampled as a `scale` transform is. Zooms multiply with the node's ancestors' and the window's (`Ui::set_zoom`). `zoom_fixed()` keeps a node at its own size whatever zooms are around it, the window's included (a toolbar that stays put, a canvas that zooms itself).

```rust
# use zgx::compose::prelude::*;
let panel = column().zoom(1.5).child(text("Larger, laid out at that size"));
```

```lua
ui.column { zoom = 1.5, ui.text { "Larger" } }
ui.row { zoom_fixed = true, toolbar }
```

`zoomable(state)` lets the user zoom a node: its zoom is bound to `state`, which Control with the wheel over it steps through the window's levels (Windows and Linux, where the window zooms so) and a trackpad pinch over it changes (every platform, macOS included), from 25% to 500%. The innermost zoomable under the pointer takes the gesture, at its limit too, and a wheel gesture stays with it as it grows; the window zooms only where no zoomable is under the pointer. In a scroll view what is under the pointer stays there; elsewhere the node zooms about its top-left. The keyboard's zoom stays the window's. `Ui::set_zoomable(bool)` (whether the user may zoom the window) and `zoomable(state)` (the user zooms this node, through this state) are the same idea at two levels.

```rust
# use zgx::compose::prelude::*;
# let ui = zgx::widgets::Ui::new(400., 300.);
let zoom = ui.signal(1_f32);
let document = scroll(ui.signal(0.)).child(column().zoomable(zoom.clone()).child(text("Zoom me")));
```

Give the zoom to the state alone: a `zoom(n)` on the same node sets it again each time its style applies.

A zoomed node's `bounds` are in its own points; `Scene::transformed_bounds` gives where it shows, and hit testing, damage and accessibility follow that. A zoom of 1 is no zoom at all: no layer, the same layout and paint. Native views inside a zoomed node are placed at its zoom, and a web page's zoom and a terminal's text follow it.

A zoomed node paints as a cached layer rasterized at its exact scale: free while it holds still, and content changing inside it (typing, a caret) repaints only the part of the layer the change reaches. The CPU reference renderer samples zoomed layers at 1×, so they are not crisp there.

## Image fitting

Image views accept `.object_fit(ObjectFit::...)`, with centered placement inside
the content box after padding:

| Mode | Behavior |
| --- | --- |
| `Fill` (default) | Stretch independently to content width and height |
| `Contain` | Preserve aspect ratio and show the full image |
| `Cover` | Preserve aspect ratio, fill the box and clip excess pixels |
| `None` | Keep source pixel dimensions, centered and clipped |
| `ScaleDown` | Contain without scaling above the source dimensions |

`.object_contain()` and `.object_cover()` are shortcuts. This property is local
to the image and is not inherited by children. Sparse reactive and interaction
patches follow the same precedence as other style properties; removing an
override restores the underlying mode. Uncovered content shows the view's
background. Clipping is rectangular, including when the outer background is
rounded. These modes change placement, not the image's intrinsic size contribution.

```rust
use std::sync::Arc;
use zgx::{compose::prelude::*, image::ImageData};
let pixels = Arc::new(ImageData::new(2, 1, vec![255; 8]).unwrap());
let thumbnail = image("Preview", pixels)
    .size(160., 120.).p(8.).bg(rgb(0x202838)).object_contain();
```

## Line height

`.line_height(24.)` sets the distance between visual text rows to 24 logical pixels.
It inherits through ordinary children, components and providers, including boxed
labels and editors. Font size still controls glyph size. `.line_height_normal()`
explicitly resets an inherited pixel value to normal spacing for the child's own
font size. Omitting the field from a `Styles` patch preserves inheritance; removing
a dynamic override restores the underlying base or inherited value.

```rust
use zgx::compose::prelude::*;
let content = column().text_size(16.).line_height(24.)
    .child(text("First line\nSecond line"))
    .child(text("Compact caption").text_size(12.).line_height_normal());
```

Positive finite values are used exactly, including values smaller than the font
size. Tight spacing can overlap adjacent glyph rows; glyph ink outside the text
node's allocated rectangle is clipped, consistent with other text rendering.
Zero, negative and non-finite values resolve to normal spacing. The normalized
metric supports stable equality and hashing, so repeated equivalent styles do
not invalidate layout or paint.

Reactive line-height changes update retained text geometry and editor caret,
selection, hit testing and IME placement together. They do not reconstruct the
component or modify its text model. Single-line inputs still use the selected
line height for their caret and metrics while remaining unwrapped.

## Letter spacing

`.letter_spacing(2.)` adds two logical pixels of tracking to native glyph
advances. It inherits through children, components and lexical slots, including
editors. `.letter_spacing(0.)` explicitly resets inherited tracking; omitting the
field preserves inheritance. Signed values allow tighter or looser text.

```rust
use zgx::compose::prelude::*;
let heading = column().letter_spacing(2.)
    .child(text("STATUS"))
    .child(text("Normal caption").letter_spacing(0.));
```

Native shaping uses the same pixel-to-em conversion for measurement, editor
geometry, GPU placement and software rasterization. Tracking is part of the font
cache key. Changing it reflows text and updates carets, selections and hit testing
without rebuilding components or changing editor text. Equal normalized values
do not invalidate geometry. Integer tracking changes can reuse glyph bitmaps
while updating their positions.

The native shaper applies tracking per shaped glyph, including the final advance;
ligatures and combining sequences therefore follow its glyph clustering rather
than a per-character CSS spacing algorithm. A scene whose text shaper takes no
font (`Scene::set_text_shaper`) spaces that shaper's own advances after each
character and wraps them as the native shaper wraps (a word that fits a line
moves to the next, one wider breaks by glyph, spaces at a wrap hang): its
sizes and carets match the native shaper's wherever a character is one glyph
(`zgx-gpu/tests/letter_spacing_fallback.rs`), and right-to-left text keeps
grapheme cells. With no shaper at all, measurement uses approximate grapheme
cells and clamps their advances at zero for very tight spacing. Negative native advances can overlap glyphs; selection rectangles retain
ordered edges, and text ink still clips to its allocated bounds.

Zero and negative zero are equivalent; non-finite values normalize to zero.
Finite values clamp to ±1,000,000 logical pixels to keep extreme tracking from
producing nonfinite geometry. This is a bound on the style value, not a promise
that extreme spacing is readable. Font-aware custom shapers receive the
normalized value through `FontStyle::letter_spacing`.

## Interaction and reactive styles

Styles resolve in this order: base fluent properties, `reactive_style`, hover, active, focus, disabled. Later patches take precedence only for fields they specify. While disabled, hover, active, and focus patches are excluded. Removing a state or a dynamic property restores the underlying style; it does not leave stale values behind.

`.disabled(true)` and `.disabled_when(|| signal.get())` disable interaction and update accessibility. `.disabled_style(...)` controls appearance; supplying that patch alone does not disable a view. Focus is cleared when a focused subtree becomes disabled. Buttons support pointer and keyboard activation.

A view is hovered while the pointer is over it or anything inside it, as the DOM's `mouseenter` and `mouseleave` decide: `hover` styles hold while the pointer moves between its children, and its `on_event` listener hears one `PointerEnter` when the pointer comes inside and one `PointerLeave` when it is outside it and all its descendants, each at the target phase (they do not bubble). Overlapping siblings follow hit order: where a positioned sibling covers another, only it (and its ancestors) is hovered. Leaving the window, the window going to the back and the pointer moving onto a native view that takes input (a web page, a terminal) all leave whatever was hovered. Tracking costs a walk up from the new hit target when it changes and nothing when it does not.

### Input placeholders

`text_input` and `text_area` take `.placeholder("Search")` (or `.placeholder_when(|| …)`, reactive): text shown while the value is empty, in the editor's own typography and coloured the theme's `muted`, or `.placeholder_color(color)` (`placeholder_color_when` for a reactive one). The editor's text node shows it in place of the empty text, so it adds no node, is laid out as the empty value it stands for and never changes the editor's size; it is never part of the value, and accessibility exposes it as the input's placeholder. `.select_on_focus(true)` selects the whole text whenever the editor takes focus, as a browser's address bar does: the first click selects everything, the next places the caret, and a window coming back to the front keeps the selection it had. The selection is also the editor's to set: `EditorHandle::selection()`, `set_selection(anchor, focus)` and `select_all()` take UTF-8 byte offsets (moved back to a grapheme boundary), as do `NodeHandle`'s (from `View::on_mount`) and `Ui::editor(node)`.

```rust
use zgx::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let address = ui.signal(String::new());
ui.mount(
    text_input("Address", address)
        .w(320.)
        .placeholder("Search or enter address")
        .placeholder_color(rgba(0xffffff66))
        .select_on_focus(true),
);
```

```rust
use zgx::{compose::column, style::{rgb, Styled, Styles}, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let highlighted = ui.signal(false);
let read = highlighted.clone();
let view = column().p(12.).reactive_style(move || {
    Styles::new().bg(if read.get() { rgb(0x345888) } else { rgb(0x202838) })
});
let mounted = ui.mount(view);
highlighted.set(true);
```

Signal reads inside `reactive_style` are tracked. A background or text-color change damages paint without invalidating layout. Size, spacing, wrapping, or font-size changes invalidate dependent layout. Translations, scale/rotation/skew and opacity use compositor invalidation. Equal resolved properties produce no new scene mutation. The mounted children and component-local state are preserved in each case.

## Transparency and effects

`.translate(x, y)` uses logical pixels. Each nonfinite coordinate (NaN or either
infinity) resolves to zero at the scene boundary; the other coordinate remains
independent. Repeating an equivalent normalized translation leaves geometry
revision, layout and damage unchanged. Removing a sparse override restores the
base translation. This does not clamp otherwise finite coordinates.

Effect values normalize at the scene boundary before equality and damage checks.
Opacity clamps to `[0, 1]`; NaN uses the default `1`, while positive/negative
infinity clamp to `1`/`0`. Blur radius and edge-fade length map nonfinite or
negative values to `0`. An explicit invalid value still overrides an earlier
style: it resolves to this fallback rather than restoring inheritance. Repeating
the same normalized effect does not add scene damage or invalidation.

`rgb(0xRRGGBB)` creates an opaque color; `rgba(0xRRGGBBAA)` includes alpha. `.opacity(value)` multiplies opacity. Without isolation, opacity applies through individual subtree primitives: overlapping translucent children can accumulate alpha. Use `.isolated(true).opacity(0.5)` when the subtree should first render as a group and then composite once. GPU isolated layers retain their pixels across eligible transform/opacity-only updates, subject to cache limits.

`.scale(factor)`, `.scale_xy(x, y)`, `.rotate(radians)` and `.skew(x, y)` (radians, CSS `skew`) transform a view after layout, about `.transform_origin(x, y)` (fractions of its own size, centre by default), on top of its translation: CSS order translate, rotate, skew, scale. Each sets only its component, so a `.hover(|s| s.scale(1.05))` keeps a base rotation. Layout never sees them. A transformed view composites as a cached layer: its subtree is painted once into a texture (group opacity applies once, overflow clips to the rotated box), and a transform change only redraws the texture under the new matrix, damaging the old and new transformed bounds. Magnifying past about 1.3× (or shrinking below about 0.77×) re-renders the layer at a raster scale in √2 steps, with hysteresis, so a zoom repaints a few times rather than looking blurry or repainting every frame. Hit testing maps the pointer through each transform; a zero scale is hit nowhere. Nonfinite components resolve to their identity value. See `Scene::set_local_transform` for the scene API.

`.rotate_x(radians)` and `.rotate_y(radians)` tilt a view about the horizontal and vertical axes through its transform origin (CSS `rotateX`, `rotateY`: positive tips the top away, or turns the right side away), seen from `.perspective(distance)` logical points away (CSS `perspective()`; 0, the default, is none): the nearer side shows larger, and without a perspective a tilt only narrows the view. The view's cached layer composites as a projected quad, sampled perspective-correctly, so a card flipping or a row of covers turning repaints nothing; its bounds, damage and hit testing follow the projection. Tilts draw on the wgpu and direct Metal paths and in the software renderer. See `LocalTransform::rotate_x`, `rotate_y` and `perspective`; `LocalTransform::projective` gives the map in perspective, beside the affine `matrix`.

`.blur(radius)` is backdrop blur, not a generic image or text blur. It samples content painted behind the view. GPU backdrop updates reconstruct the affected filter outputs and their sampling halos in paint order. Unrelated damage stays local; connected filters can still require the full target. The software reference backend conservatively repaints the full target when blur is present. The GPU blur implementation caps sigma at 64 physical pixels. For blurring a view's own content (CSS `filter: blur()`), use `.filter_blur(sigma)` instead. `.brightness(x)`, `.contrast(x)` and `.saturate(x)` (CSS `filter`, 1 changes nothing) filter the view and its subtree's colours as a group: being affine maps of colour, they are applied as the GPU writes each colour, exactly as to the group, with no layer and no extra pass, composing with ancestors' filters and with opacity; animating them (a button brightening on hover) repaints only the view's area, and a layer's cached pixels are filtered at its composite. `.edge_fade(length)` fades the top and bottom edges. `.fade_edges(top, bottom)` on a clipping view fades its descendants out over bands at its clip's edges, per pixel and without a layer; `.fade_edges_x(left, right)` does the left and right edges (a horizontally scrolling row fading its overflow), `.fade_sides(top, right, bottom, left)` all four, each band independent, and `.fade_insets(..)`/`.fade_insets_x(..)` end the bands that far inside the clip. Fades nest: a view fading inside another (a label fading its overflow in a list fading its ends) is faded by both, multiplied; the nearest two apply. Overlapping items in a band fade as one, as a group would; on Direct3D 11 (opt-in, `ZGX_D3D11=1`), which draws no layers, they fade one by one, so where they overlap in a band the one under shows through more. `.clip_inset(top, right, bottom, left)` (CSS `clip-path: inset(..)`) clips the view and its descendants to its box inset by those pixels; negative values reach past it so shadows and rings aren't cut, `.clip_inset_percent(..)` takes percentages of its live size (`clip_inset_percent(0., 0., 100., 0.)` closes it to its top edge) and `.clip_inset_lengths(ClipInset { .. })` mixes them. With `overflow_hidden` and rounded corners, the inset box is rounded alike. Animating it composites again without layout; a blurred, masked or isolated view's cached pixels are cut at composite, a transformed view's layer repaints. See `Scene::set_clip_inset`. `.overflow_hidden()` clips descendants to the layout box, and with `.rounded(r)` or `.rounded_corners(..)` to its rounded corners too (CSS `overflow: hidden` with `border-radius`): every descendant (quads, text, images, SVGs, nested and transformed layers, blurred or masked ones) is cut along the curve, antialiased, in the GPU fragment shader, following the box's laid-out size every frame. Nested rounded clips intersect (the innermost two per item; further out, their corners). Radii are held to half the box's shorter side. The rounded clip runs along the padding box, as CSS's does: inside a border that takes room, each corner's radius less its border, so a child filling the box leaves the border's curve whole (with no border, and for a border drawn over the content, that is the box's outer edge). The rectangular clip is the box's outer edge. Solid borders (`.border(w)`, or per side with `.border_edges(Insets {..})`, under uniform or `.rounded_corners(..)` radii) are drawn analytically by the quad shader, so an animating box keeps them without re-rasterizing; dashed borders and gradient fills rasterize. `.border_over(true)` paints the border after the view's descendants (an inset stroke, a CSS outline drawn inside): a photo filling a framed thumbnail stays under its frame. A dashed border still paints with the fill. `.outline(width, color)` and `.outline_offset(offset)` (CSS `outline`, `outline-offset`) ring the box outside its border edge, `offset` past it (negative reaches inside), its corners the box's radii grown by `offset + width` (square corners stay square). It paints after the view's descendants, is not cut by the view's own `overflow_hidden` (an ancestor's clip still cuts it), takes no layout, and is drawn analytically by the quad shader, so changing its width, colour or offset repaints only where it was and where it lands. `.outline_none()` removes it. `.stroke(width, color, StrokeAlign::Inside | Center | Outside)` is a design tool's stroke alignment: `Inside` is `.border(width).border_color(color).border_over(true)` (inside the box, over its content), `Center` an outline at offset `-width / 2`, `Outside` one at offset 0. `.bg_image(image, fit)` fills the view with an image placed as `fit` has it, centred (`ObjectFit::Fill`, `Cover`, `Contain`, `None`, `ScaleDown`, as CSS `background-size`/`object-fit`), over its `.bg(..)` colour (set `bg` first: it clears the image), cut to its rounded corners, under its border and children. It is drawn as a textured quad following the box's size every frame, with no raster. A dashed border over a background image rasterizes without the image. `.bg_shader(FillShader::new(wgsl, fps))` fills the view with a WGSL function run per pixel on the GPU, `fn fill(input: FillInput) -> vec4<f32>` returning a straight-alpha colour, where `FillInput` has `p` (logical position in the box), `size`, `time`, `scale` (device pixels per logical pixel) and `params` (four `vec4`s of user values). It paints over the `bg` colour, cut to the rounded corners, under the border and children. `.shader_uniforms(ShaderUniforms { time, params })` or `Scene::set_shader_uniforms` sets them; changing them repaints that view alone, and the shader's `fps` quantizes time so a 30 fps effect repaints at 30 fps however often it is set. On direct Metal the same module is translated by naga. A shader that does not compile fills with the `bg` colour. The quad shader's names (`Vertex`, `Out`, `Region`, `shade`, `rounded`, `region`, `viewport`, `regions`, `atlas`, `basic`, `vs`, `fs`) and those starting `zgx_` are reserved.

`.filter_blur(sigma)` blurs the view and its subtree by a Gaussian of `sigma` logical pixels, spreading three sigma past its bounds (it is clipped by its ancestors' clips, not its own), with opacity applied to the blurred result. `.mask(image, (width, height))` shows the view through the image's alpha (CSS `mask-image`, no repeat, nearest sampling), drawn at that size and at `.mask_offset(x, y)` from the view's origin; outside the image nothing shows. Blur applies before the mask, the mask before opacity, all in the view's own space under any scale, rotation or skew. Either makes the view a cached layer: its subtree is painted once, and animating the sigma or the mask offset (a fade in from blurred text, a shimmer band, a progress fill) only blurs and masks the cached pixels again and composites them, without layout or repainting the subtree. On the GPU every layer blurred or masked anew in a frame shares two passes (a wide blur runs on a copy averaged down by a power of two, so its cost stays flat); at sigma 0 and without a mask the view paints as any other. See `Scene::set_filter_blur`, `Scene::set_mask` and `Scene::set_mask_offset`. The software reference backend approximates the blur with three box passes.

`.blend(BlendMode::SoftLight)` mixes the view with what lies beneath it, as CSS `mix-blend-mode`, in any of the W3C's fifteen modes (`Multiply`, `Screen`, `Overlay`, `Darken`, `Lighten`, `ColorDodge`, `ColorBurn`, `HardLight`, `SoftLight`, `Difference`, `Exclusion`, `Hue`, `Saturation`, `Color`, `Luminosity`; `Normal` is the default): its subtree paints once into a cached layer whose pixels combine with the backdrop as it composites, the backdrop being the window or, inside another layer, that layer. `.layer_shader(FillShader::new(wgsl, fps))` composites the view's cached layer through WGSL of the app's own, for a ripple, a chromatic split, scanlines or a dissolve: `fn layer(input: LayerInput) -> vec4<f32>` returns a straight-alpha colour for each pixel of the box, `LayerInput` holding the layer's pixel there (`color`) beside `p`, `size`, `time`, `scale` and `params` as `FillInput` has them, and `zgx_sample(p)` reads the layer at another logical position. It applies after any content blur and mask, under the view's opacity. `.layer_uniforms(ShaderUniforms { time, params })` or `Scene::set_layer_uniforms` changes its inputs, compositing again without repainting the subtree. See `Scene::set_blend` and `Scene::set_layer_shader`. Frames with a blend or a layer shader draw through wgpu rather than macOS's direct Metal path, for now.

The [native effects example](../crates/zgx-desktop/examples/effects.rs) builds its glass card and controls with component children, `.reactive_style(...)`, and these fluent effect methods. Component-owned tasks animate a translation signal; the application does not append or mutate scene nodes. Run it with `cargo run -p zgx-desktop --example effects`.

`.shadow(BoxShadow { ... })` adds an outer shadow without changing layout dimensions. Shadow extents participate in damage tracking. `.shadow_none()` explicitly clears an inherited patch's shadow setting (shadows do not inherit down the view tree).

The native GPU renderer shapes text and provides fallback fonts. Font family, weight and italic inherit through components and slots alongside color, size and wrapping. Use `.font_family("DejaVu Serif").font_weight(700).italic(true)`, or the portable `FontFamily::{SansSerif, Serif, Monospace}` choices. Weight is clamped to 1–1000; named families and unavailable faces use platform fallback. A variable font (one file with a `wght` axis, such as the design system's Geist and Geist Mono) serves every weight from that one file: register it once and `font_weight(500)` measures, shapes and draws the 500 instance, the same advances and outlines as a static 500 file cut from it, on every path. Native measurement, caret shaping and GPU glyph selection share font attributes; changing them invalidates layout and retained glyph geometry. The software reference backend also renders styled fonts, using a lazy platform font database. Letter spacing and custom line height use the shared inherited metrics described above. `WindowFactory::replace_font(font)` replaces the font of the families `font` holds, in every window from then on (a bundled font registered with `Application::font`, or a system one): their text measures, lays out and draws again, and a family not had before is added. It costs nothing until called. It does not yet expose the full CSS layout or styling surface of the inspiration projects. GPU cache and effect limits are documented in [the renderer notes](../crates/zgx-gpu/README.md).

The software styled-font cache retains at most 128 entries and 8 MiB of text keys/glyph images; oversized runs render without being retained. Its platform font database is created on first styled text and shared with software layer renderers. Headless core-only scenes use approximate metrics until a `Scene::set_font_text_shaper` backend is installed.

Editable views use the same fluent styles. For example, `text_input("Search", query).w(320.).px(12.).font_family(FontFamily::Monospace).focus(|s| s.stroke(2., rgb(0x5ea5ff), StrokeAlign::Inside))` styles the actual focusable editor root. Its text and caret inherit color, size, family, weight, and italic; root padding and allocated dimensions determine the clipped editing viewport. Typography and dimension changes preserve selection and the bound model. Use `.text_wrap(true)` on `text_area` to wrap at its allocated content width, including inherited or reactive wrapping styles. Rendering, pointer hit testing, selection, caret movement and IME geometry share the same wrapped layout. Resizing reflows visual lines without adding newlines to the model or losing selection. Wrapping defaults to false, preserving horizontal scrolling for long lines; `text_input` always remains a single unwrapped line. Home/End follow visual lines while wrapping; repeated Up/Down retain the preferred horizontal caret position across shorter lines. Edits, pointer placement, Home/End, and changes to wrapping, font metrics or allocated width reset that preferred position.

Multiline editors accept wheel scrolling without moving the selection; scrolling clamps to the text extent and bubbles to ancestors at an exhausted edge. Single-line editors leave vertical wheel events to their parent. Font/color changes preserve component ownership; color-only changes repaint without relayout, while font changes also update selection, hit testing and caret metrics.

`.absolute()` removes a child from normal layout flow and positions it against
its containing block:

- the nearest ancestor marked `.relative()` (CSS `position: relative`), searching
  up from the parent but not past an `.absolute()` ancestor, which contains its
  own absolute descendants;
- with no such ancestor, its parent.

`.left(..)`, `.top(..)`, `.right(..)` and `.bottom(..)` count from the containing
block's padding box: inside its border, ignoring its padding. Percentages are of
that box's width (left, right) or height (top, bottom), and setting both insets
on an axis without a size there stretches it between them. An axis with no inset
keeps the static place: the parent's content origin (inside its border and
padding) plus `.ml(..)`/`.mt(..)`. An absolute view does not consume flex space,
add a gap, or enlarge any ancestor's intrinsic size. It still paints in its
parent's place in the tree (its paint order and its ancestors' clips) and moves
with every ancestor's transform: transforms, including scroll offsets, are
paint-time and never change which block contains it. Switch, show and keyed
regions are ordinary wrappers here: mark the intended ancestor `.relative()`
rather than relying on the wrapper being the parent.

```rust
use zgx::compose::prelude::*;
let card = column().relative().w(300.).h(200.)
    .child(column().p(12.).child(div().absolute().right(8.).bottom(8.).size(16., 16.)));
```

`.relative()` also keeps a view in normal flow (undoing `.absolute()`), including
when applied by a reactive style patch; it changes nothing about the view's own
layout. Absolute positioning does not create a window-level portal. Use
`.translate(...)` for a compositor-only positional change when layout
participation should stay the same.


Focused editors reuse bounded shaped geometry for unchanged displayed text and
metrics. Selection, scrolling and navigation do not reshape that text; font,
line-height and wrapped-width changes invalidate the reuse entry. See
[editor layout reuse](performance/editor-shaping/README.md) for admission limits
and measured call counts. This does not change component ownership or the style
inheritance rules above.

## Theming

Every zgx app and widget can style itself from a standard set of semantic
tokens, and any theme gives them values, as any VS Code theme styles any
extension. Themes are data (JSON files, Luau tables or Rust values) and switch
live. zgx's own widgets (buttons, inputs, menus, sliders, progress bars,
scrollbars, modals and popovers) style themselves only from tokens.

```rust
use zgx::compose::prelude::*;
use zgx::theme::{LIGHT, tok, token};
use zgx::widgets::Ui;

let mut ui = Ui::new(400., 300.);
// An app's own token, declared with a default written in standard tokens.
ui.theme().declare(token("app.card"), tok::SURFACE_RAISED);
ui.mount(
    column()
        .bg(tok::BACKGROUND)
        .p(tok::SPACE_4)
        .gap(tok::SPACE_2)
        .child(text("Settings").text_size(tok::FONT_SIZE_XL))
        .child(text("Changes save as you go").text_color(tok::TEXT_SECONDARY))
        .child(div().h(80.).bg(token("app.card")).rounded(tok::RADIUS_LG).shadow(tok::SHADOW_MD)),
);
ui.theme().set(LIGHT).unwrap(); // what uses changed tokens restyles, nothing else
```

### The standard tokens

| Group | Tokens |
| --- | --- |
| Surfaces | `background`, `surface.sunken`, `surface.base`, `surface.raised`, `surface.overlay` |
| Text | `text.primary`, `text.secondary`, `text.tertiary`, `text.disabled` |
| Lines | `border`, `border.subtle`, `border.strong` |
| Accent | `accent`, `accent.hover`, `accent.text` (text on the accent) |
| Status | `danger`, `warning`, `success`, `info` |
| States | `selection` (selected text), `selection.item` (a selected row), `focus.ring`, `hover` and `pressed` (translucent washes laid over a surface) |
| Scrollbars | `scrollbar.thumb`, `scrollbar.thumb.active`, `scrollbar.track` |
| Glass | `glass.tint` (laid over a backdrop blur), `glass.blur` |
| Typography | `font.sans`, `font.mono`; `font.size.{xs,sm,md,lg,xl,xxl}` with matching line heights `font.line.*`; `font.weight.{regular,medium,semibold,bold}` |
| Metrics | `radius.{sm,md,lg,xl,full}`, `space.1` to `space.8` (4 to 64 points), `control.height` |
| Shadows | `shadow.sm`, `shadow.md`, `shadow.lg` |
| Motion | `motion.duration.{fast,normal,slow}` (milliseconds), `motion.ease.{standard,enter,exit}` (anime.js easings) |

The roles are the ones VS Code's workbench colours, Radix Themes and Apple's
semantic colours share: surfaces by elevation, text by emphasis, washes
rather than fixed hover colours (so they work on any surface), and scales for
size, radius and space. `tok::*` are typed constants (`ColorToken`,
`NumberToken`, `FontToken`, `ShadowToken`, `TextToken`), so a colour token
cannot be passed for a length. zgx's widgets declare tokens of their own in
standard terms, which a theme may set: `button.bg` (`{surface.raised}`),
`button.hover` (`over({button.bg}, {hover})`), `button.pressed`, `input.bg`,
`menu.bg` (`{surface.overlay}`), `menu.hover`, and `popover.bg`
(`{surface.overlay}`, popovers' and tooltips' panels). On macOS, where menus
and popovers show in windows of their own over AppKit's material, the system
sets `menu.bg` and `popover.bg` clear and `menu.hover` to the accent, as an
app theme setting them overrides.

Text shows `text.primary`, `font.size.md` and `font.sans` unless a view sets
its own. The built-in `zgx-dark` theme, the default, keeps zgx's original
palette and metrics, so apps written before themes look as they did;
`zgx-light` is its light counterpart.

### Theme files

```json
{
	"name": "Solarized Dark",
	"type": "dark",
	"extends": "zgx-dark",
	"tokens": {
		"background": "#002b36",
		"surface": { "base": "#073642", "raised": "{surface.base}" },
		"text.secondary": "alpha({text.primary}, 0.6)",
		"hover": "#ffffff0d",
		"shadow.md": "0 4 12 0 alpha({background}, 0.5)",
		"terminal.pane": "#00000040"
	}
}
```

`type` is `dark`, `light`, `high-contrast` or `high-contrast-light`. A theme
that leaves a token out takes it from the theme it `extends`, and every chain
ends in the built-in theme of its type, so a theme can set three tokens and
be complete. Groups nest (`"surface": { "base": … }` is `surface.base`), and
comments and trailing commas are allowed.

A value is a literal (`"#rrggbbaa"`, `"rgb(…)"`, `16`, `"Geist"`, a CSS
`box-shadow`, an easing), a reference (`"{surface.raised}"`) or a small
derivation of colours: `alpha(c, a)` (its opacity times `a`), `mix(a, b, t)`
(`t` of the way from `a` to `b`) and `over(base, wash)` (`wash` composited
over `base`). Expressions are parsed when a theme loads and evaluated once
per token when it is applied, never while styling. A reference cycle
resolves to nothing.

`ui.theme()` (or `Themes::of(&runtime)`: every `Ui` and script host on a
runtime shares it) holds the themes: `register(theme)`, `load(path)` (a zgx
theme file, or a VS Code colour theme, below), `set(name)`,
`set_default(name)` (applies unless the app chose one), `current()`
(tracked), `names()`, `declare(token, default)` and
`follow_system(Some((dark, light)))`, which the desktop host drives from the
system's light or dark appearance. The window's clear colour follows
`background`. Theme files hot-reload: `reload_if_changed()` reads the files
that changed and restyles in place, and `watch(every)` polls them (a script
host does this along with its scripts). A file that stops reading keeps its
last good theme and reports the error.

The desktop host also gives the themes the system's own settings, read at
startup and followed live: the selection colour (the user's accent on
macOS, Windows and Linux), high contrast, animations turned off (the motion
durations are zero) and the text size (the font tokens scale). On Linux they
come from the XDG settings portal, as GTK and Qt apps read them: dark or
light, the accent, contrast and reduced motion from
`org.freedesktop.appearance`, and GNOME's animations and Large Text. Where
the portal has no dark or light preference (XFCE's), the GTK theme on X11
decides, as it does GTK's own apps: the session's XSETTINGS `Net/ThemeName`,
dark where it is named so (`Adwaita-dark`, `Greybird-dark`), read again as a
window is activated. Windows'
are listed in [the Windows notes](windows.md#system-settings). An app theme
or `theme_scope` that sets a token keeps its own value.

### App tokens and unknown tokens

An app names its own tokens in its own namespace (`terminal.pane`,
`notes.wash`) and declares a default for each in terms of the standard ones
(`"{surface.sunken}"`, `"alpha({accent}, 0.24)"`). Any theme then styles the
app, and a theme made for it may still set them by name. A token that
nothing defines (no theme, no declaration) resolves to nothing: transparent,
0, the default font, no shadow; debug builds print a warning the first time
each such token is read. Declare tokens before mounting what uses them.

### Scoped overrides

```rust
use zgx::compose::prelude::*;
use zgx::theme::LIGHT;

let card = column().bg(tok::SURFACE_RAISED).child(text("A light card in a dark app"));
let light_card = theme_scope(ThemeOverride::new().theme(LIGHT), card);
let danger = theme_scope(
    ThemeOverride::new().with(tok::ACCENT, rgb(0xe5484d)),
    button().child(text("Delete")),
);
```

`theme_scope` gives a subtree another theme, some tokens, or both; otherwise
the subtree follows the enclosing scope, including when the app switches
themes, and tokens derived from an overridden one (a focus ring that is
`{accent}`) follow it. Inherited text takes the scope's body colour.
`theme_scope_with(|| …, child)` computes the override and keeps it current.
Overlays a subtree opens (menus, popovers) take its scope, and a component
reads tokens from `cx.theme()`.

### Styles and costs

`.bg`, `.text_color`, `.border_color`, `.rounded`, `.border`, the padding and
margin setters, `.gap`, `.text_size`, `.line_height`, `.letter_spacing`,
`.font_weight`, `.font_family`, `.shadow`, `.opacity`, `.blur`, `.w` and `.h`
take a value or a token, and `.token(Field::…, id)` binds any of them. A later
value, or a patch merged over the style, replaces a binding as it would a
value. A style holds token ids, interned once (`token("app.card")` hashes the
name when the view is built), and resolving one indexes the scope's array.

Each node subscribes to the tokens it shows and to nothing else: a resting
button reads its background and radius but not its hover colour, and nodes
without tokens subscribe to nothing. Applying a theme evaluates each token
once, compares, and notifies only what reads tokens whose values changed: a
colour repaints those nodes without layout, and a metric re-lays out those
nodes. Nothing runs per frame. On the repository's headless workloads
(retired instructions, against the same workloads before themes) the heavy
dashboard and `anime_bench` are within 0.5%; a form of 1,500 widgets
(`widget_bench`) takes 0.4% more instructions to mount and 2.8% more memory
(the token subscriptions of its buttons and inputs), and switching its theme
takes 0.67 ms, repainting its 3,000 themed nodes and laying out none.

### VS Code themes

`Themes::load` reads a VS Code colour theme too, following its `include`,
and maps its workbench colours onto tokens: `editor.background` to
`background`, `editor.foreground` to `text.primary`, `focusBorder` to
`focus.ring`, `button.background` to `accent`,
`list.activeSelectionBackground` to `selection.item`, `input.background` to
`input.bg`, `menu.background` to `menu.bg`, and so on. What it does not set
comes from zgx's theme of its type. `Theme::from_vscode` converts one in
memory and `Theme::to_json` writes the result as a zgx theme file (the
`vscode_theme` example converts a file). Syntax colours (`tokenColors`) are
ignored.
