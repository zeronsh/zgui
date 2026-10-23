-- Which messages a mailbox and a search show, newest first (as stored).
local state = require("../state")

return function(messages: { state.Message }, mailbox: string, query: string): { state.Message }
	local out: { state.Message } = {}
	local needle = string.lower(query)
	for _, m in messages do
		local here = if mailbox == "Starred" then m.starred else m.mailbox == mailbox
		if here and (needle == "" or string.find(string.lower(m.subject .. " " .. m.from), needle, 1, true)) then
			table.insert(out, m)
		end
	end
	return out
end
