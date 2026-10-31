---
section: changed
---
- **A selection is one rounded shape** (zgx): text selections, in selectable text and editors, draw lines that touch as one continuous shape, with rounded outer corners and curved inner ones, even in a translucent colour and at fractional scales. `$selection.radius` is now 3, not 0; set it to 0 for plain rectangles. An empty line in a selection shows as a sliver. An editor's selection is one node carrying a highlight, not a rectangle a line, so code that walked an editor's nodes for them finds it through `Scene::text_highlight`.
