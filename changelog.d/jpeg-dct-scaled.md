---
section: performance
---
- **A JPEG drawn small never decodes at full size** (zgx, zgx-gpu): it decodes at 1/2, 1/4 or 1/8 of its size, the smallest that covers the size drawn, with the DCT scaled (zgx's fork of zune-jpeg, published as `zgx-zune-jpeg`; `ImageData::kept_scaled` and `zgx::image::DecodeScaled` are new). A kept image's decode no longer copies its pixels to premultiply them. A page of eight 4000 by 3000 photos as 300 by 420 cards: each decode's peak 91.6 → 18.6 MiB and time 47 → 22 ms; the process's peak about 490 → 330 MB. JPEGs decode through the fork directly, not `image`, which no longer carries a JPEG decoder of its own; their pixels are unchanged at full size.
