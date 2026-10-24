# SVG and image transformations

Image components support center-origin affine paint transforms, including
rotation, nonuniform scaling, reflection, shear and translation:

```rust,ignore
image("Logo", source)
    .size(160., 100.)
    .with_transformation(Affine::rotation(0.5))
```

For changing transforms, use `image_signal` and return
`Arc::new(source.transformed(matrix))`. The new value shares the original pixel
allocation and image identity. GPU uploads are reused, layout stays cached,
and damage includes both the previous and new painted bounds. `a.then(b)`
applies `a` first. Positive rotation is clockwise in screen coordinates.

Transforms do not change layout allocation. Fill/contain/scale-down transformed
images can paint outside that allocation; ancestor `overflow_hidden` still
clips them. Cover/none fitting retains its viewport crop. Hit testing uses the
inverse transform. Singular transforms paint nothing and receive no image hit;
nonfinite matrices supplied to `ImageData::transformed` normalize to identity.

This API applies to image/SVG views. Any view, container subtrees included, takes `.scale`, `.rotate` and `.skew` styles instead (see [styling](styling.md)), composited as a cached layer. SVG is
decoded once with `zgx_gpu::assets::decode_svg`; the
[runnable example](../crates/zgx-desktop/examples/svg_transform.rs) changes its
transform without redecoding. Resolution remains the decoded resolution.

Regression coverage in `affine_images` tests covers rotations, reflection,
shear, singular matrices, clipped/overflowing and isolated content, and compares
incremental frames with fresh full frames in GPU and software renderers.

For SVG without application-owned raster management, use `svg(label, source)`
or `svg_signal`, with `Arc<SvgData>` source bytes. These default to a 24-pixel
square and accept ordinary `.size` styling, `.svg_tint(color)`, and
`.with_transformation(matrix)`. The renderer rasterizes at the allocated display
resolution and caches up to 32 MiB of SVG pixels. Size/tint changes rebuild that
image; affine-only changes reuse the upload. External resources and SVG text
remain restricted as described by the asset decoder. Invalid XML surfaces as a
render error. `svg_rasterizations` and `svg_raster_bytes` expose cache activity.
