# Typed app tokens

Status: shipped for 0.1.6: the lint in #890, `ui.tokens` and `zgx types`
in #895. This note says what was built and why it differs from the plan
(generated prop unions became a typed `ui.tokens` table).

Standard theme tokens are typed (`ColorToken`, `NumberToken` … in
`zgx.d.luau`); an app's own (`ui.theme.tokens { app = { … } }`) are not, so
a misspelled `"$app.crad"` passes `zgx check` and reads as transparent at
run time.

## The type checker can't be the guarantee

A colour prop takes `"#rrggbb"`, `"rgb(…)"` and `0xrrggbb` as well as tokens,
and Luau has no template-literal types, so `Color` stays `string | number`
(a closed union would also leak into a script's own string code). A
misspelled token can't be a type error without rejecting `"#fff"`. So:

- **the lint is the guarantee** (exact, no luau-lsp);
- **the types give completion and typo errors** through `ui.tokens`, a
  table of fields, rather than through props.

## Where names come from

Statically, with nothing run:

- the standard tokens, from the engine's schema;
- `ui.theme.tokens { … }` literal tables, nested groups flattened to dotted
  names. A name's kind comes from its default (`TokenInput::parse`): colour,
  number, font, shadow or text.

Theme files (`ui.theme.load("x.json")` with a literal path,
`ui.theme.register` literals) are checked against that set and add no name:
a theme setting an undeclared name is itself a finding (`glass.json sets
app.crad, which nothing declares`). A theme overrides; a declaration gives
the fallback every theme needs anyway.

A computed declaration (a non-literal key or table) marks its group open:
unknown names under it are not reported, to avoid false positives. The
run-time warning (`warn_unknown`) still covers them.

## Lint: `unknown-token`

In `zgx lint`, and so in `zgx check`'s report:

- every `"$…"` string literal in a style position, a `hover` table or a
  computed's return is checked against the set;
- the kind is checked too: a colour token where a number goes
  (`p = "$app.accent"`);
- a finding names the nearest declared name (`did you mean $app.card?`);
- `-- zgx-lint: allow(unknown-token)` accepts an exception.

## Generated types: `ui.tokens`

As built, an app names a token as a path of fields, `ui.tokens.app.card`,
whose value is its string (`"$app.card"`); props stay `string`:

- `ui.tokens.notes.` completes the app's `notes` tokens, and a typo is a
  type error (`Key 'wahs' not found`), not a transparent box. A group that is
  a token too has it as `value` (`ui.tokens.accent.value`); a number in a
  name is an index (`ui.tokens.space[3]`); a name that is a Luau keyword is
  a quoted field (`["end"]: string`).
- Its type is `Tokens` in `zgx.d.luau` for the standard tokens.
  `zgx types <dir>` writes `<dir>/.zgx/types.d.luau`: zgx's definitions with
  `Tokens` holding the app's declared names too, and those names in
  `ui.theme.get`'s key. An editor lists that file in place of `zgx.d.luau`.
- At run time it is built from the applied theme the first time a script
  reads it, and again once the tokens change: a script that never reads it
  builds nothing.

Why not the planned unions on props (`Color = string | number | ColorToken
| AppColorToken`): under luau-lsp's new solver they reported 80 false
errors over the examples and checked at half the speed, and a closed union
still couldn't reject a misspelling, since `"#fff"` must pass. Nested
tables of plain strings complete and catch typos with neither cost.

- `zgx check` writes the file first and passes it to luau-lsp.
- `.zgx/` is a dot directory, so `bundle` leaves it out; it stays out of
  version control too.

## Hot reload

The debug host already runs the check after each reload; it also
regenerates the file, writing it only when the set of names changed so
editors don't churn. A release build does nothing.

## Cost

None at run time: static tools only. The run-time warning stays as the
safety net and also shows in the reload banner.

## Settled

- The union question decided the shape: unions on props cost luau-lsp too
  much (above), so the types moved to `ui.tokens`.
- Apps that declare tokens only in theme files get findings until they
  declare defaults. Intended; called out in the changelog.
