---
section: changed
---
- **A test's clicks are counted as a person's** (zgx-script): `zgx test`'s and `TestApp`'s presses come with the keys held, as a window's do, so `on_press`'s `clicks` and a double click count in a test as they do in a window; separate `app:click` calls in a row still count as separate clicks (`InputDispatcher::forget_clicks`).
