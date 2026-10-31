# Auditing an app for assistive technology

`zgx audit <app.luau-or-dir>` checks what a screen reader gets of a Luau app,
headless: no window, and no permission asked. It audits the app's first
screen, then each state it can open from there, each from a fresh start:

- every command it declares (`ui.command`), run as its menu item would;
- every enabled button and link on the first screen, pressed (what opens a
  modal, a popover, a menu, a toast or another view);
- every route its routers declare (`ui.router`), opened, a route's
  `:params` given `1`.

In each it checks the tree a reader gets (`zgx_desktop::accessibility::audit`)
and walks Tab through it:

| Rule | Found |
| --- | --- |
| `no-role` | something takes focus but is no control: a `ui.div` with `focusable = true` and no `role` |
| `no-name` | a control, an image or a dialog with no name (`label`, or text a button shows) |
| `no-action` | a control that can't be pressed, focused or set |
| `no-state` | a check box, switch, radio button, checkable menu item or tab that doesn't say whether it's chosen; a slider or spin button with no value; a slider or progress bar with a value and no range |
| `focus-unreachable` | Tab never reaches something that takes focus (outside a toolbar, list or other group the arrows move in) |
| `focus-trap` | Tab, Control-Tab and Escape all keep focus on one control or a few |
| `not-announced` | an alert or status that isn't live; an invalid field whose error isn't given to a reader |

A finding is reported once, at the first state showing it. Out comes one line
of JSON, for agents and tools:

```json
{"states":["start","press:#more"],"findings":[{"rule":"no-name","target":"#unnamed","role":"Button","name":"","detail":"a control with no name","source":"app.luau:5","state":"start"}],"errors":[]}
```

- `target` is how `zgx inspect` names the element: its `#id`, else its role
  and name (`button "Save"`), else its node (`@12`).
- `source` is the line that built it.
- `state` is where it was found: `start`, `command:<id>`, `press:<target>`
  or `route:<path>`.
- `errors` are states that could not be reached (a script error).

`--human` prints a table instead. The exit status is 0 with no findings, 1
with some and 2 when the app can't be audited, so `zgx audit app/` fails a CI
step on a regression. `zgx audit` runs `zgx-run --audit`, which must be
beside `zgx` or on `PATH`.

Fix a finding in the app: give the control a `label`, a focusable `ui.div` a
`role` (and a name), a decorative image `aria_hidden = true`. A `ui.popover`
or `ui.menu` without a `label` is named by its trigger. What the audit can't
judge is how the app sounds and flows to a person using VoiceOver, Narrator
or Orca: check that by hand.
