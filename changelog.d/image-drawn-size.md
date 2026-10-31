---
section: fixed
---
- **Large images draw, at the size they show** (zgx-gpu): an image's texture held its whole file (a 4000 by 3000 photo was 48 MB however small it showed), so two large photos passed the 64 MiB budget and the second was left out, or, before 0.1.7, ended the app. Textures now hold an image at its drawn size, resampled as mip levels are, uploaded again as it is drawn larger. The budget is four of the window's frames (at least 64 MiB), and past it images are drawn softer, not left out.
