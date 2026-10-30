-- Made-up people, searched and paged as a server would.
export type Person = { id: number, name: string, role: string }
export type Page = { items: { Person }, next: number? }

local FIRST = { "Ada", "Grace", "Katherine", "Alan", "Edsger", "Barbara", "Donald", "Frances", "Linus", "Radia" }
local ROLES = { "Engineer", "Designer", "Researcher", "Writer" }
local PAGE = 50

local everyone: { Person } = {}
for i = 1, 1000 do
	table.insert(everyone, {
		id = i,
		name = `{FIRST[(i - 1) % #FIRST + 1]} {i}`,
		role = ROLES[(i - 1) % #ROLES + 1],
	})
end

local people = {}

-- The people whose names hold `query` (any case), from `start`, a page of
-- them, and where the next page starts.
function people.page(query: string, start: number): Page
	local wanted = string.lower(query)
	local found = {}
	for _, person in everyone do
		if wanted == "" or string.find(string.lower(person.name), wanted, 1, true) then
			table.insert(found, person)
		end
	end
	local items = {}
	for i = start + 1, math.min(start + PAGE, #found) do
		table.insert(items, found[i])
	end
	return { items = items, next = if start + PAGE < #found then start + PAGE else nil }
end

return people
