---
section: fixed
---
- **An empty image `src` shows nothing** (zgx-script): `ui.image`, `ui.svg` and `bg_image` with `src = ""` (a picture not chosen yet, often a state's first value) show nothing and report no error, where they read the script's directory and failed with "Is a directory". A missing image's error names the script line asking for it, never a line of `.zgx/types.d.luau` or another declaration or hidden file that happened to hold the same text.
