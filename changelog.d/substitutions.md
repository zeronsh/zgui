---
section: added
---
- **Autocorrect and smart punctuation, opt-in per field** (zgx, zgx-desktop, zgx-script): `View::autocorrect` (Luau `autocorrect = true`) corrects a misspelled word as it ends and expands the person's text replacements ("omw" to "On my way!"); `View::smart_punctuation` (`smart_punctuation = true`) makes straight quotes curly and `--` a dash as they are typed. Both go through the system's own services (macOS's `NSSpellChecker`, in the forms the person chose), only where the person has them on in System Settings, and never in a password field. Each substitution is an undo step of its own: undo puts back what was typed, and it stays. Off by default; other platforms substitute nothing yet (`Platform::substitutions`, `Platform::correction`).
