# zgx patch to zune-jpeg 0.5.15

Vendored from crates.io `zune-jpeg` 0.5.15 (MIT OR Apache-2.0 OR Zlib) and
published as `zgx-zune-jpeg`, its library still `zune_jpeg`. zgx-gpu decodes
JPEGs with it directly (not through `image`), so zgx's crates build from
crates.io without a `[patch]` section. The changes are marked `zgx:` in the
source.

## Why

A photo is mostly shown far smaller than it is: a 4000 by 3000 photo as a
300 by 420 card. Decoded whole and then shrunk, each decode peaked at its full
size (46 MiB of RGBA, plus the decoder's planes) before the pixels the card
needs existed. A JPEG holds its pixels as 8 by 8 blocks of frequencies, and
the low ones alone give the block at 1/2, 1/4 or 1/8 the size (libjpeg's
scaled IDCT).

## What

1. `src/decoder.rs`: `JpegDecoder::set_scale(denominator)` decodes at 1/1,
   1/2, 1/4 or 1/8 (other values round down to one of those), kept as
   `scale_block`, the side of the pixel block an 8x8 block of coefficients
   becomes (8, 4, 2 or 1). After the headers, `dimensions` and
   `output_buffer_size` report the scaled size, `ceil(width / d)` by
   `ceil(height / d)`, as libjpeg does; `info` keeps the file's size.
   `set_upsampling` up-samples only what a component's block leaves to do,
   and in a scaled decode only generically (the full decode's nearest sample,
   for the ratios of 4 or 3 it already does so).
2. `src/misc.rs`: `setup_component_params` lays rows out in scaled pixels,
   and `setup_component_blocks` gives each component its block, `block_w` by
   `block_h` (new fields in `src/components.rs`): `scale_block` square, or,
   for a subsampled component, `scale_block` times its ratio where that is
   8 or less, so 4:2:0 and 4:2:2 chroma come out at the image's scaled size
   from a larger IDCT and are never up-sampled (libjpeg does this for 4:2:0).
   It also records each component's size in its own samples (`full_w`,
   `full_h`). `calculate_padded_width` takes the block size.
3. `src/idct/scalar.rs`: `idct_scaled`, a separable IDCT of any block shape
   from 8x8 down to 1x1, with libjpeg's 1-D kernels: `jidctred.c`'s for 4, 2
   and 1 points and `jidctint.c`'s for 8. The reduced kernels read every
   coefficient but those that cancel over the pixels they stand for, so each
   output is the mean of the full IDCT's pixels it replaces, to rounding.
   `idct_scaled_at` takes it for a block inside the image; for one straddling
   the right or bottom edge it makes the block whole and averages only the
   pixels inside the image, not the encoder's padding.
4. `src/mcu.rs` (baseline, including scans of one component each) and
   `src/mcu_prog.rs` (progressive, whose coefficients are stored whole as
   before): row buffers, `raw_coeff`, the buffered whole-image planes, block
   positions and the rows each MCU row writes are in a component's block
   size instead of 8, and a block not 8x8 takes `idct_scaled_at`. Coefficient
   storage and indexing stay 64 a block.
5. `src/lib.rs`: the crate-level `#![macro_use]` is gone; on a crate it does
   nothing, and Rust 1.99 warns of it in every build of zgx.

Unscaled, every one of these is the arithmetic of 0.5.15 (a block is 8, the
same IDCTs and up-samplers run), and the output is the same byte for byte:
`tests/scaled.rs` checks it against crates.io's 0.5.15 on every fixture,
truncated and corrupted ones included, and checks each scaled decode against
the full decode averaged by area. Its fixtures are made by
`tests/scaled/make_fixtures.py`.
