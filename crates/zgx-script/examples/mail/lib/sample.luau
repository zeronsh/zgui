-- Sample mail: the same few thousand messages every launch, made from a
-- seeded generator so the list has something to scroll.
local M = {}

export type Message = {
	id: number,
	mailbox: string,
	from: string,
	email: string,
	subject: string,
	preview: string,
	body: string,
	date: string,
	unread: boolean,
	starred: boolean,
}

local people = {
	{ "Ada Lovelace", "ada@analytical.engine" },
	{ "Grace Hopper", "grace@navy.mil" },
	{ "Alan Turing", "alan@bletchley.uk" },
	{ "Katherine Johnson", "katherine@nasa.gov" },
	{ "Edsger Dijkstra", "ewd@utexas.edu" },
	{ "Barbara Liskov", "liskov@mit.edu" },
	{ "Donald Knuth", "knuth@stanford.edu" },
	{ "Margaret Hamilton", "margaret@apollo.org" },
}
local topics = {
	"Quarterly review notes",
	"Lunch on Thursday?",
	"Draft of the compiler paper",
	"Re: flaky test in CI",
	"Trip itinerary",
	"Your invoice is ready",
	"Design review: the sidebar",
	"Weekend hike photos",
	"Benchmarks from last night",
	"Question about the API",
}
local sentences = {
	"I went through the numbers again and they hold up.",
	"Let me know if Thursday works for you, or suggest another day.",
	"The second section still reads a little dense; maybe split it?",
	"We should talk about the release before the end of the week.",
	"Attached are the photos from the trail, the view was worth it.",
	"The flaky test turned out to be a race in the setup code.",
	"Thanks for the quick turnaround on the review.",
	"I think the sidebar should collapse below 800 points.",
}
local mailboxes = { "Inbox", "Inbox", "Inbox", "Inbox", "Sent", "Archive", "Archive", "Drafts", "Trash" }

-- Park and Miller's generator: deterministic, and exact in Luau's doubles
-- (its products stay under 2^53).
local function generator(seed: number)
	local state = seed
	return function(n: number): number
		state = (state * 48271) % 2147483647
		return state % n + 1
	end
end

function M.messages(count: number): { Message }
	local pick = generator(42)
	local out: { Message } = {}
	for i = 1, count do
		local person = people[pick(#people)]
		local paragraphs = {}
		for _ = 1, pick(3) + 1 do
			local lines = {}
			for _ = 1, pick(3) + 1 do
				table.insert(lines, sentences[pick(#sentences)])
			end
			table.insert(paragraphs, table.concat(lines, " "))
		end
		local body = table.concat(paragraphs, "\n\n")
		local day = 28 - (i // 40) % 28
		table.insert(out, {
			id = i,
			mailbox = mailboxes[pick(#mailboxes)],
			from = person[1],
			email = person[2],
			subject = topics[pick(#topics)],
			preview = string.sub(paragraphs[1], 1, 90),
			body = body,
			date = `Oct {day}`,
			unread = pick(4) == 1,
			starred = pick(9) == 1,
		})
	end
	return out
end

return M
