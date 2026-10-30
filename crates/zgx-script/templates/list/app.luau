-- People: a long list searched as you type and loaded a page at a time as
-- it scrolls, and favourites you drag (or move with Alt+↑/↓) into order.
-- The data is made here; swap `people.page` for `ui.fetch` to load yours.
local tokens = require("./theme")
local people = require("./people")
local PersonRow = require("@components/person_row")

type Person = people.Person

-- Asked once the typing stops, not on every key.
local query = ui.state("query", "")
local searched = ui.debounced(query, 200)

-- A page at a time: the list asks for the next as its end comes near.
local pages = ui.paged(function(cursor: number?): { items: { Person }, next: number? }
	return people.page(searched:get(), cursor or 0)
end)

local favourites = ui.state("favourites", { "Ada", "Grace", "Katherine" })

local function reorder(from: number, to: number)
	local list = table.clone(favourites:peek())
	table.insert(list, to, table.remove(list, from) :: string)
	favourites:set(list)
end

return ui.row {
	w_full = true,
	h_full = true,
	ui.column {
		id = "favourites",
		w = 180,
		p = "$space.3",
		gap = "$space.1",
		bg = tokens.card,
		ui.text { "Favourites", bold = true },
		ui.each(favourites, function(name: string)
			return ui.row { id = `favourite-{name}`, px = "$space.2", py = "$space.1", ui.text { name, literal = true } }
		end, {
			key = function(name: string)
				return name
			end,
			reorder = reorder,
		}),
	},
	ui.column {
		grow = 1,
		p = "$space.3",
		gap = "$space.2",
		ui.input { id = "search", label = "Search", placeholder = "Search people", value = query },
		ui.show(function()
			return #pages.items:get() == 0 and not pages.loading:get()
		end, ui.text { id = "empty", "Nobody matches.", color = tokens.muted }),
		ui.list {
			id = "people",
			data = pages.items,
			height = PersonRow.HEIGHT,
			w_full = true,
			grow = 1,
			on_end = pages.more,
			key = function(person: Person)
				return person.id
			end,
			row = PersonRow.row,
		},
	},
}
