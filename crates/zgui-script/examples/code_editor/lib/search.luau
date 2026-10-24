--!strict
-- A search's matches in the text: plain text (no patterns), with case or
-- without, at most `LIMIT` of them (a query of one letter in a large file
-- would match a million times).

local M = {}

M.LIMIT = 10000

-- `{ from, to }` (bytes from 1, as `string.find` gives them) of each match
-- of `query` in `text`, and whether there were more than the limit.
function M.matches(text: string, query: string, case: boolean): ({ { number } }, boolean)
	if query == "" then
		return {}, false
	end
	local haystack, needle = text, query
	if not case then
		haystack, needle = string.lower(text), string.lower(query)
	end
	local out: { { number } } = {}
	local at = 1
	while true do
		local s, e = string.find(haystack, needle, at, true)
		if not s then
			return out, false
		end
		if #out == M.LIMIT then
			return out, true
		end
		out[#out + 1] = { s, e :: number }
		at = (e :: number) + 1
	end
end

-- The first match at or after byte `at` (from 1), else the first: its index.
function M.after(matches: { { number } }, at: number): number
	local lo, hi = 1, #matches
	while lo <= hi do
		local mid = (lo + hi) // 2
		if matches[mid][1] < at then
			lo = mid + 1
		else
			hi = mid - 1
		end
	end
	return if lo > #matches then 1 else lo
end

return M
