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

(Filled in with the change.)
