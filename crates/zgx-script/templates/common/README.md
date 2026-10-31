# {{title}}

A [zgx](https://zgx.zeron.sh) app in Luau, made with `zgx new {{name}} --template {{template}}`.

## Run it

```sh
zgx run .
```

The window reloads as you save, keeping its state. `zgx run --inspect .` also serves devtools, so `zgx inspect tree` (or an agent, through `zgx inspect mcp`) can read and drive it.

## Check it

```sh
zgx check .   # types, with luau-lsp
zgx lint .    # patterns that cost more than they look
zgx audit .   # what a screen reader gets, and where Tab goes
zgx test      # tests/*.test.luau, against the app headless
```

The workflow in `.github/workflows/zgx.yml` runs all four on every push and pull request.

## What is where

| | |
| --- | --- |
| `app.luau` | the app |
| `components/` | its components (`require("@components/…")`, aliased in `.luaurc`) |
| `theme.luau` | its own design tokens, read as `ui.tokens.app.*` |
| `tests/` | its tests: `test`, `mount` and `expect` |
| `zgx.json` | the window: title, size, `app_id` (where `ui.persist` keeps its data) |
| `.zgx/` | generated types for your editor (`zgx types .`); not committed |

Point your editor's luau-lsp at `.zgx/types.d.luau` (`luau-lsp.types.definitionFiles`) to complete `ui.*` and the app's own tokens.

## Learn more

- [Getting started](https://zgx.zeron.sh/luau/getting-started) and [Coming from React](https://zgx.zeron.sh/luau/coming-from-react)
- [The API reference](https://zgx.zeron.sh/luau/reference)
- [Testing an app](https://zgx.zeron.sh/luau/elements#testing-an-app)
