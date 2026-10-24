-- Sample data: 100,000 instruments, deterministic, and a price series.
export type Row = {
	id: number,
	symbol: string,
	sector: string,
	price: number,
	change: number,
	volume: number,
}

local SECTORS = { "Tech", "Energy", "Health", "Finance", "Retail", "Industry", "Media", "Utilities" }
local LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

local M = {}
M.SECTORS = SECTORS

-- A small deterministic generator (the same rows every run).
local seed = 12345
local function random(): number
	seed = (seed * 1103515245 + 12345) % 2147483648
	return seed / 2147483648
end
M.random = random

-- `count` rows, and each one's recent prices by its id (for its sparkline).
function M.rows(count: number): ({ Row }, { [number]: { number } })
	local rows: { Row } = {}
	local histories: { [number]: { number } } = {}
	for i = 1, count do
		local a, b, c = (i % 26) + 1, (i // 26 % 26) + 1, (i // 676 % 26) + 1
		local price = 5 + random() * 495
		local history: { number } = {}
		local p = price
		for h = 1, 16 do
			p = math.max(1, p * (1 + (random() - 0.5) * 0.04))
			history[h] = p
		end
		rows[i] = {
			id = i,
			symbol = string.sub(LETTERS, a, a) .. string.sub(LETTERS, b, b) .. string.sub(LETTERS, c, c) .. tostring(
				i % 97
			),
			sector = SECTORS[(i % #SECTORS) + 1],
			price = price,
			change = (random() - 0.5) * 10,
			volume = math.floor(random() * 5_000_000),
		}
		histories[i] = history
	end
	return rows, histories
end

function M.series(count: number): { number }
	local values: { number } = {}
	local v = 100
	for i = 1, count do
		v = math.max(10, v + (random() - 0.5) * 6)
		values[i] = v
	end
	return values
end

return M
