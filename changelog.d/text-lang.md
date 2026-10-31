---
section: added
---
- **Text says the language it is in with `lang`** (zgx, zgx-desktop, zgx-script): HTML's attribute, as `Styled::lang`, `TextSpan::lang`, and Luau's `lang = "zh-TW"` on any element or span. It is inherited by what the element holds. Its Han characters take that language's fonts, ahead of the app's locale and of what a line's kana or hangul say, so a Chinese quote in a Japanese app is drawn in a Chinese face. Screen readers are given it as the text's language. The fork's fallback search keeps simplified, traditional and Hong Kong Chinese lists whatever the locale.
