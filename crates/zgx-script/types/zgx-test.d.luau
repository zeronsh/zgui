-- The globals of a `zgx test` file (`*.test.luau`): `zgx check` gives them
-- to test files alone, beside zgx.d.luau. Targets are devtools' own: `#id`,
-- `"label or text"`, `~"part of it"`, `role:button`, `role:checkbox "Done"`, `#list row 3`,
-- `"Copy"[2]`, `#a >> "b"` (inside).

-- An app `mount` made, headless, driven as a person would.
declare extern type TestApp with
	-- Press and release the primary button at the target's middle.
	function click(self, target: string): ()
	-- Twice, as a double click.
	function double_click(self, target: string): ()
	-- Click the target (a field), then type `text` into it.
	function type(self, target: string, text: string): ()
	-- Click the target (a field) and replace its whole text with `text`, as
	-- a person selecting it all and typing does.
	function fill(self, target: string, text: string): ()
	-- Move the pointer to the target's middle, as a mouse does: what it leaves
	-- hears it go.
	function hover(self, target: string): ()
	-- Drag `from` onto `to` with the primary button, past the drag threshold,
	-- released over the upper half of `to` (a reorderable row drops above it).
	function drag(self, from: string, to: string): ()
	-- Press keys where focus is: "Enter", "Escape", "cmd+k", "alt+ArrowDown"
	-- (cmd is Command on macOS, Control elsewhere).
	function key(self, keys: string): ()
	-- Scroll the viewport under the target by dy points (down is positive).
	function scroll(self, target: string, dy: number): ()
	-- Run a `ui.command` by its id, as its menu item or shortcut does.
	function command(self, id: string): ()
	-- The window's new size, laid out.
	function resize(self, width: number, height: number): ()
	-- The next `ui.open_files` picks these files, the next `ui.save_file` saves
	-- to this one. Names are in a directory of this app's own, gone with it.
	function pick(self, names: { string }): ()
	-- The next `ui.open_folder` picks this folder.
	function pick_folder(self, name: string): ()
	function save_to(self, name: string): ()
	-- A file of that directory, written for a dialog to pick, or read after
	-- the app saved it.
	function write_file(self, name: string, text: string): ()
	-- A fixture from the app's directory (an image, a folder of them), copied
	-- in as `name`.
	function copy_file(self, from: string, name: string): ()
	function read_file(self, name: string): string
	-- The clipboard's text, and text put on it as another app copies it.
	function clipboard(self): string
	function copy(self, text: string): ()
	-- The target's text: a field's value, its own, or the first inside it.
	function text(self, target: string): string
	-- A field's selection, in bytes from 0 (equal for a caret, the second
	-- where the caret is); nil for anything else.
	function selection(self, target: string): (number?, number?)
	-- Whether anything matches the target.
	function has(self, target: string): boolean
	-- How many nodes match the target.
	function count(self, target: string): number
	-- The interface, or the target and what is in it, as `zgx inspect tree`
	-- shows it, a node a line.
	function tree(self, target: string?): string
	-- Deliver `n` display frames (1 if not given): an animation's worth.
	function frames(self, n: number?): ()
	-- Deliver frames until the target shows (its text holding `text`, if
	-- given), or is gone; whether it did before `timeout_ms` (5000).
	function wait(self, target: string, options: { text: string?, gone: boolean?, timeout_ms: number? }?): boolean
	-- A named state's value (`ui.state`, `ui.persist`, `ui.shared`).
	function state(self, name: string): any
	-- Set a named state, as the script's `set` would.
	function set_state(self, name: string, value: any): ()
	-- The script error showing, if any.
	function error(self): string?
	-- Another window the app declares, by a `ui.window`'s id or the key of
	-- an item of `ui.windows`, with these same methods; nil while it is
	-- closed.
	function window(self, id: string): TestApp?
	-- A window from `window` closed, as a person closing it does.
	function close(self): ()
end

export type Expectation = {
	-- `==`: the same value, or the same table.
	to_be: (expected: any) -> (),
	-- The same value, tables compared field by field.
	to_equal: (expected: any) -> (),
	-- A string holding `part`, or a list holding a value equal to it.
	to_contain: (part: any) -> (),
	to_be_truthy: () -> (),
	to_be_nil: () -> (),
}

-- Adds a test: the file's tests run in its order, each with fresh apps.
declare function test(name: string, body: () -> ()): ()
-- The app (its directory's app.luau, or `path` from there), mounted
-- headless at its window's size (zgx.json's, else 960 by 640).
declare function mount(path: string?): TestApp
-- Checks `actual`; `never` holds the checks negated.
declare function expect(actual: any): Expectation & { never: Expectation }
