---
section: added
---
- **Han characters take the app's language's fonts** (zgx, zgx-gpu, zgx-desktop, zgx-script): a script with `ui.i18n` sets the windows' text language to its locale, as CSS's `lang` on a document does, so a Japanese locale's kanji draw in a Japanese face and `zh-TW`'s in a traditional Chinese one, whatever the system's language. Rust apps call `WindowFactory::set_text_language`; `Platform::set_text_language` carries it, and `FontSystem::set_language` in the fork applies it. A Chinese, Japanese or Korean language set takes precedence over a line's own kana or hangul. A system locale such as `ja-JP` now gives Japanese faces too (the fallback lists named only `ja`).
