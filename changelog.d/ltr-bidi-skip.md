---
section: performance
---
- **Left-to-right text skips the bidi analysis** (zgx-gpu): a line with characters beyond ASCII (accents, `—`, `…`, `×`, emoji) but nothing right to left, no bidi control and no paragraph separator is laid out as one left-to-right run, as ASCII already was, without `unicode-bidi`'s analysis. One look at its bytes decides; a test checks every code point the analysis treats otherwise is caught. The gate's `lab-scroll` (such text, scrolled) runs 4.4% fewer instructions.
