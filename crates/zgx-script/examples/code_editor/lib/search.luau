--!strict
-- A search's matches (`state:find`, plain text, with case or without): at
-- most `LIMIT` of them (a query of one letter in a large file would match a
-- million times), and the one after a place.

local M = {}

M.LIMIT = 10000

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
