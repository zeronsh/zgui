---
section: added
---
- **Hyphenation** (zgx, zgx-gpu, zgx-script): words break at their hyphenation points, a hyphen ending the line, as CSS's `hyphens: auto`, in the system's language, from typst's `hypher` (48 languages). In 0.1 it's a font feature, inherited as features are: `FontFeatures::with_hyphens(true)` in Rust, `font_features = "hyphens"` in Luau; `hyphens = "auto"` of its own waits for 0.2. It's behind a `hyphens` feature (zgx-desktop's), off by default, about 1.2 MB, read in place; without it, Luau's `"hyphens"` is an error where scripts are linted (a debug build, `zgx lint`). The text is unchanged: selection, carets and copying see the word whole. zgx-cosmic-text is 0.19.5.
