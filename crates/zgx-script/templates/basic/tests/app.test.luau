-- Run with `zgx test`. Targets are devtools' own: `#id`, `"label"`,
-- `role:checkbox "Water the plants"`.
test("adding, checking off and clearing a task", function()
	local app = mount()
	expect(app:text("#count")).to_be("0 tasks")

	app:type("#new", "Water the plants")
	app:key("Enter")
	expect(app:text("#count")).to_be("1 task")
	expect(app:has("#empty")).to_be(false)

	app:click('role:checkbox "Water the plants"')
	expect(app:state("tasks")[1].done).to_be(true)

	app:click("#clear")
	expect(app:text("#count")).to_be("0 tasks")
	expect(app:has("#empty")).to_be(true)
end)
