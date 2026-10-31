---
section: changed
---
- **Edge fades ease in** (zgx-gpu): `fade` / `fade_sides` (`Styled::fade_edges`) bands follow smoothstep rather than `t²`. `t²` reached full opacity still rising, and the band's inner end showed as a line; smoothstep is level at both ends. Every fade looks a little different: content stays nearer full opacity further into the band and falls more steeply in its middle. Before and after: `docs/results/fade-smoothstep-2026-10-31/`.
