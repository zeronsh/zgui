---
section: fixed
---
- **A Japanese line's kanji draw in a Japanese font** (zgx-gpu): Han characters took the fallback font of the system's language, so on an English system a Japanese line's kanji came from a different font than its kana: Arial Unicode MS on macOS, Microsoft YaHei UI's Chinese forms on Windows. A line's own kana (or hangul, for Korean) now says which language its Han characters are in, and the run cache keeps them apart. Lines with neither keep the system language's font, and ASCII text is never looked through.
