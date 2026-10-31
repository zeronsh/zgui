# Start an app: zgx new

`zgx new <name>` writes a Luau app that runs at once. Every template is clean under `zgx check`, `zgx lint`, `zgx audit` and `zgx test` from the start, so a failure afterwards is something you changed.

```sh
zgx new my-app                   # the basic template
zgx new notes --template router  # or list, form, plugins
zgx new shop --i18n              # its text as ui.t messages, with locales/en.json
cd my-app
zgx run .                        # a window, reloading as you save
zgx test                         # its tests, headless
```

The directory must be new or empty. `zgx run` finds the `zgx-run` beside `zgx` or on `PATH`, and says how to install it when neither is there ([getting started](getting-started.md#in-luau)).

## Templates

| Template | What it is | What it's there to show |
| --- | --- | --- |
| `basic` | Tasks | State, a component, a test |
| `router` | Notes | [Routes](reference/state.md#routing) and a modal route |
| `list` | A long list of people | A list paged as it scrolls ([`ui.paged`](reference/state.md#pages-and-debouncing)), search as you type, reorderable favourites |
| `form` | A profile | [Forms](reference/controls.md#forms) and their validation |
| `plugins` | Tasks made to be extended | [Plugins from `zgx.json`](plugins.md#plugins-in-a-luau-app-zgxjson): shared state, an action from `ui.actions`, a filtered list, and `plugins/focus` using them |

## What it writes

Each template has:

| File | |
| --- | --- |
| `app.luau` | The app's entry |
| `components/` | A component of its own |
| `theme.luau` | The app's own tokens, read as `ui.tokens.app.*` ([typed tokens](scripting.md#theming)) |
| `tests/app.test.luau` | Its tests ([testing an app](scripting.md#testing-an-app)) |
| `zgx.json` | The window's title and size, and in `plugins` what's exposed and allowed |
| `.luaurc` | Strict mode, and an alias for `components/` |
| `README.md` | How to run and check it |
| `.github/workflows/` | A GitHub Actions job running `zgx check`, `zgx lint`, `zgx audit` and `zgx test` on pushes to `main` and on pull requests |

With `--i18n`, text is written as `ui.t` messages and `locales/en.json` holds them ([localized messages](reference/text.md#localized-messages)).

## The four checks

```sh
zgx check .   # types, with luau-lsp
zgx lint .    # costly patterns, unknown tokens, untranslated keys
zgx audit .   # what a screen reader gets of the app
zgx test      # the app's own tests
```

`zgx check` needs [luau-lsp](scripting.md#types) installed, and `zgx audit` a `zgx-run` it can find. [Testing an app](scripting.md#testing-an-app) and the [accessibility audit](accessibility-audit.md) have the details.
