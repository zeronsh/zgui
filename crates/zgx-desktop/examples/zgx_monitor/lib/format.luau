-- Numbers as the monitor shows them.
local M = {}

export type Metric = "cpu" | "gpu" | "mem"

-- "12.3%", and whole percents from 100 (a busy process passes it: one
-- core is 100%).
function M.percent(value: number): string
	if value >= 99.95 then
		return string.format("%.0f%%", value)
	end
	return string.format("%.1f%%", value)
end

-- "48.2 MB", "312 MB", "1.24 GB".
function M.memory(mb: number): string
	if mb >= 1024 then
		return string.format("%.2f GB", mb / 1024)
	end
	if mb >= 100 then
		return string.format("%.0f MB", mb)
	end
	return string.format("%.1f MB", mb)
end

function M.value(metric: Metric, value: number): string
	return if metric == "mem" then M.memory(value) else M.percent(value)
end

-- An axis label: whole numbers, gigabytes past 1024 MB.
function M.axis(metric: Metric, value: number): string
	if metric == "mem" and value >= 1024 then
		local gb = value / 1024
		return if gb == math.floor(gb) then string.format("%d G", gb) else string.format("%.1f G", gb)
	end
	return string.format("%.0f", value)
end

-- A 1-2-2.5-5 "nice" ceiling for an axis showing up to `peak`, never under
-- `floor` (near-zero series are not blown up).
function M.nice_max(peak: number, floor: number): number
	local target = math.max(peak * 1.15, floor)
	local magnitude = 10 ^ math.floor(math.log10(target))
	for _, step in { 1, 2, 2.5, 5, 10 } do
		if step * magnitude >= target then
			return step * magnitude
		end
	end
	return 10 * magnitude
end

return M
