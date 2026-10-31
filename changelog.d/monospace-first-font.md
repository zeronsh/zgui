---
section: performance
---
- **New monospaced text no longer searches every installed font** (zgx-gpu, zgx-cosmic-text): each new word of monospaced text (a terminal's every line, a log, a code view's edits) weighed every fixed-width face on the system before shaping. Its family's face is now kept, as other families' are, and the search runs only for a character the face lacks. Shaping a terminal's 24 new lines takes 32 µs instead of 135 µs, and an engine-drawn terminal printing 20,000 lines a second spends a quarter less CPU on macOS. A text's fallback families are looked up once rather than whenever it is laid out anew.
