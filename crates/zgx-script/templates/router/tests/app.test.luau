-- Run with `zgx test`. Targets are devtools' own: `#id`, `"label"`,
-- `role:button "Settings"`.
test("opening a note and going back", function()
	local app = mount()
	app:click('"Routes"')
	expect(app:text("#note-title")).to_be("Routes")
	app:click("#back")
	expect(app:has("#note-title")).to_be(false)
	app:click("#forward")
	expect(app:text("#note-title")).to_be("Routes")
end)

test("settings opens over the list and closes back", function()
	local app = mount()
	app:click('role:button "Settings"')
	expect(app:has("#close-settings")).to_be(true)
	app:click("#close-settings")
	-- Closed, it lets clicks through to the list again.
	app:click('"Welcome"')
	expect(app:text("#note-title")).to_be("Welcome")
end)
