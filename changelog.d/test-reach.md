---
section: changed
---
- **A test or devtools action says what is in its way** (zgx-script): `click`, `double_click`, `type`, `fill`, `hover` and `drag` in `zgx test`, and `zgx inspect`'s actions, fail on a target outside its scroll's clip naming the clip ("scroll it into view first"), and on one under another node naming that node and where, rather than acting on whatever was there. Input's `InputDispatcher::pointer_target` is public: what a press at a point lands on.
