-- A person's row in the list: every row the same height, so the list
-- builds only those in view.
local tokens = require("@app/theme")
local people = require("@app/people")

local row = {}

row.HEIGHT = 40

function row.row(person: people.Person)
	return ui.row {
		id = `person-{person.id}`,
		h = row.HEIGHT,
		px = "$space.3",
		gap = "$space.3",
		items = "center",
		ui.text { person.name, literal = true, grow = 1 },
		ui.text { person.role, literal = true, color = tokens.muted },
	}
end

return row
