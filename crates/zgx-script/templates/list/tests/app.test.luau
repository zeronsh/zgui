-- Run with `zgx test`. Targets are devtools' own: `#id`, `"label"`,
-- `#people row 1` (the first row mounted).
test("searching narrows the list once typing stops", function()
	local app = mount()
	expect(app:wait("#people row 1", { text = "Ada 1" })).to_be(true)
	app:type("#search", "radia 10")
	expect(app:wait("#people row 1", { text = "Radia 10" })).to_be(true)
	app:type("#search", "zzz")
	expect(app:wait("#empty")).to_be(true)
end)

test("scrolling loads the next page", function()
	local app = mount()
	expect(app:wait("#people row 1")).to_be(true)
	app:scroll("#people", 4000)
	expect(app:wait('"Ada 51"')).to_be(true)
end)

test("favourites move with Alt+Down", function()
	local app = mount()
	app:click("#favourite-Ada")
	app:key("alt+ArrowDown")
	expect(app:state("favourites")).to_equal { "Grace", "Ada", "Katherine" }
end)
