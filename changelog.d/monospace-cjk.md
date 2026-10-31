---
section: fixed
---
- **CJK in monospaced text shows** (zgx-gpu, macOS): a monospaced line with Chinese, Japanese or Korean characters fell back to a bitmap-only system font that has no units per em, took infinite width and showed nothing past the first such character. That font is passed by now.
