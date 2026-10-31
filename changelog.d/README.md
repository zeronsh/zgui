# Unreleased changelog entries

One file per change apps notice, named for its pull request or a slug (`1121.md`, `popover-press.md`), so two pull requests never edit the same lines:

```markdown
---
section: fixed
---
- **A press outside a popover closes it** (zgx-components): what is now true, then why.
```

`section` is `changed` (what to check in an existing app), `added`, `performance` or `fixed`. The body is the entry as `CHANGELOG.md` shows it, sub-bullets and all.

The site shows them in `CHANGELOG.md`'s Unreleased section (`scripts/changelog.py render`). A release writes them in and deletes them (`scripts/changelog.py assemble`, [releasing](../docs/releasing.md)). CI fails a pull request that edits the Unreleased section itself; one correcting an entry already there takes the `changelog-edit` label.
