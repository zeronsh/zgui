# `zgx new`: a runnable app in one command

Status: design, for 0.1.7.

`zgx new <name> [--template basic|router|list|form|plugins] [--i18n]`
writes a Luau app that runs at once (`zgx run <name>`). Its four checks
(`zgx check`, `zgx lint`, `zgx audit`, `zgx test`) are clean, and so is
the CI workflow it ships with. It is what `npm create vite` is to a React
app: the shortest way from nothing to an app that follows the rules.

## What is missing today

Only `zgx check`, `zgx lint`, `zgx types` and `zgx inspect` exist. The
scaffold's README and workflow name four more commands:

- **`zgx run`**: the docs call it that, but it is the `zgx-run` binary.
- **`zgx audit`**: render's #988 adds it.
- **`zgx test`**: `TestApp` is Rust. A Luau app has no way to test itself.
- **`zgx new`**: no scaffolder exists either.

The `zgx` binary lives in zgx-script, which zgx-desktop optionally
depends on, so it can't link the audit. zgx-run links both crates.

## The commands

- **`zgx audit [--human] <dir>`** is render's #988: `zgx` runs the
  `zgx-run` beside itself, else the one on `PATH`, with `--audit`. It
  prints JSON (a table with `--human`) and exits 0 when clean, 1 with
  findings and 2 on an error.
- **`zgx run [--inspect] <dir> [args]`** goes to the same `zgx-run`, as
  cargo hands `cargo foo` to `cargo-foo`. #988's lookup becomes a shared
  `fn zgx_run()` that both commands call.
- **`zgx test [<dir>] [<filter>]`** is in zgx-script, because `TestApp` is
  headless and needs no window. It runs each `*.test.luau` under the dir
  (default `.`) in a VM of its own and prints a line per test (`ok`, or
  `FAIL` with the error at its line). It exits 1 on any failure and 2 when
  there is no app or no test. A filter runs only the tests whose names
  contain it.

## The test API

A test file gets three globals: `test`, `mount` and `expect`. Their types
are in a second definitions file, `types/zgx-test.d.luau`, which
`zgx check` adds for `*.test.luau` only. An app's own scripts can't see
them.

```luau
test("adding a task counts it", function()
	local app = mount() -- the dir's app.luau, headless, at zgx.json's size
	app:type("#new", "Water the plants")
	app:key("Enter")
	expect(app:text("#count")).to_be("1 task")
	expect(app:has('role:checkbox "Water the plants"')).to_be(true)
end)
```

- **`mount(path?)`** returns an app with these methods:
  - acting: `click`, `type`, `fill`, `key`, `scroll` and `command`;
  - reading: `text`, `has`, `count` and `tree`;
  - waiting: `wait(target, { text?, gone?, timeout_ms? })` and
    `frames(n)`;
  - state: `state(name)` and `set_state(name, value)`;
  - the script's failure: `error()`.
- **Targets are devtools' own** (`#id`, `"label"`, `role:button`,
  `#list row 3`, `a >> b`). Every method resolves them with
  `driver::find`, as `zgx inspect` does, without `TestApp`'s bare-id
  shortcut. One syntax serves the agent, the person scripting
  `zgx inspect`, and the test.
- **`expect(v)`** has `to_be`, `to_equal` (deep), `to_contain` (a
  substring or a member), `to_be_truthy` and `to_be_nil`, plus `never`
  for their negations. A failure names both values and the test's line.
  So does an error the runner raises (no node, a bad key).
- **Ordering:** apps are collected after each test, so tests can't leak
  state into each other. Tests run in file order.
- **What a headless app lacks:** a fetcher, so `ui.fetch` fails as if
  offline, and storage, so `ui.persist` keeps state in memory and a test
  never touches the user's data. The templates don't fetch. Faking
  responses is left for later.

## What `zgx new` writes

```
<name>/
  zgx.json              title, size, app_id (dev.zgx.<name>)
  app.luau              the template's app
  theme.luau            ui.theme.tokens { app = { … } }; the app reads ui.tokens.app.*
  components/           one or two of the template's components
  locales/en.json       with --i18n: ui.i18n { dir = "locales" }, ui.t everywhere
  tests/app.test.luau   one test that drives the template's main flow
  .luaurc               strict mode; @components alias
  .zgx/types.d.luau     generated (zgx types), and git-ignored
  .gitignore            .zgx/
  .github/workflows/zgx.yml
  README.md             run, the four checks, where the docs are
```

- **The templates** are trimmed copies of the examples, kept in
  `crates/zgx-script/templates/<name>/` and built in with `include_str!`.
  `{{name}}` and `{{title}}` are replaced as the files are written.
  - **basic:** a task list, with one component and the theme.
  - **router** (from inbox): typed routes `list`, `note/:id` and
    `settings` (a modal route), back and forward.
  - **list** (from inbox): `ui.list` over local data, `ui.paged`,
    `ui.debounced` search, and `ui.each` reorder.
  - **form** (from settings): `ui.form` with validation, dirty tracking,
    submit, revert and a toast, saved to `ui.persist`.
  - **plugins** (from plugins): `zgx.json`'s `expose` and the plugin's
    `allow`, a hook target, `plugins/focus` adding beside it, filtering the
    list and calling the app's `ui.actions` action.
- **`--i18n`** writes `locales/` and swaps every literal string for
  `ui.t`. Every template has a source variant written that way, since a
  rewrite at scaffold time would be fragile.
- **The workflow** checks out the repo, installs luau-lsp 1.70.1 the way
  zgx's own CI does, runs `cargo install zgx-run zgx-script --bin zgx`
  pinned to the version that wrote it (cached), and runs `zgx check .`,
  `zgx lint .`, `zgx audit .` and `zgx test`.
- **Refusals:** `zgx new` won't write into a non-empty directory. A name
  must be a valid directory name of letters, digits, `-` and `_`.
- **Nothing else:** no `cargo`, no network, no git. It writes the files
  and the types, then prints the next steps (`cd <name>`, `zgx run .`).

## Cost

None at run time. These are tools in the `zgx` and `zgx-run` binaries;
the library and an app's build don't change. The templates add about
20 KB of text to the `zgx` binary.

## Verified by

A zgx-run integration test, `templates.rs`, scaffolds every template,
with and without `--i18n`, into a temporary dir. On each it runs `lint`,
`audit` and `test` through the library, and `check` when `LUAU_LSP` is
set. CI sets it on Linux. A golden test lists each template's files, so
a template that loses one fails.

## PRs, in order

1. **`zgx test`**: the runner, the test API and its definitions, docs and
   tests.
2. **`zgx run`**: after #988, the shared `zgx_run()` lookup and the
   dispatch.
3. **`zgx new`**: the templates, `--i18n`, the workflow, the README, and
   the scaffold-and-check test.
