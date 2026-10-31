---
section: changed
---
- **The renderer going on with less stays out of release banners** (zgx, zgx-desktop, zgx-script): a layer drawn coarser, an image softer, a glyph left out, a frame skipped after a GPU error or a layout that didn't settle showed "Something went wrong", sometimes in another window's banner. They are logged once as `degraded:`, in their own window's banner in debug builds only. `zgx::issues::IssueKind`, `on_issue_kind` and `report_kind` tell them apart; `ScriptHost::on_issue` and `issues()` give apps and `zgx inspect errors` the kind. An app's `on_error` still hears them.
