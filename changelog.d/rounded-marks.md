---
section: added
---
- **Find's marks and an editor's decorations are rounded as a selection is** (zgx): each match is one shape over the lines it runs on, rounded by the new `$mark.radius` token, which follows `$selection.radius` unless a theme sets it. `Scene::set_text_mark_shapes` sets marks so; `set_text_marks` keeps its look. An editor's decorations are marks on one node of their own, not a rectangle node each.
