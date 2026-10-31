---
section: added
---
- **Autocorrect on Windows** (zgx-desktop): an editor with `View::autocorrect` (Luau `autocorrect = true`) corrects a misspelled word as it ends through Windows' own spelling checker (`ISpellChecker`'s replacements, "teh" to "the"), where the person has "Autocorrect misspelled words I type" on. Windows has no smart quotes or dashes of its own, so `smart_punctuation` still changes nothing there.
