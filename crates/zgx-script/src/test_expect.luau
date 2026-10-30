--!nocheck
-- `expect(actual)` in `zgx test` files: each check that fails is an error
-- at the test's line, naming both values. `never` holds the same checks
-- negated.
local function show(value: any, depth: number?): string
	local depth = depth or 0
	if type(value) == "string" then
		return string.format("%q", value)
	elseif type(value) ~= "table" then
		return tostring(value)
	elseif depth >= 2 then
		return "{…}"
	end
	local keys = {}
	for key in value do
		table.insert(keys, key)
	end
	table.sort(keys, function(a, b)
		return tostring(a) < tostring(b)
	end)
	local parts = {}
	for i, key in keys do
		if i > 8 then
			table.insert(parts, "…")
			break
		end
		local shown = show(value[key], depth + 1)
		if key == i then
			table.insert(parts, shown)
		else
			table.insert(parts, tostring(key) .. " = " .. shown)
		end
	end
	return "{ " .. table.concat(parts, ", ") .. " }"
end

local function equal(a: any, b: any): boolean
	if a == b then
		return true
	elseif type(a) ~= "table" or type(b) ~= "table" then
		return false
	end
	for key, value in a do
		if not equal(value, b[key]) then
			return false
		end
	end
	for key in b do
		if a[key] == nil then
			return false
		end
	end
	return true
end

local function contains(within: any, part: any): boolean
	if type(within) == "string" and type(part) == "string" then
		return string.find(within, part, 1, true) ~= nil
	elseif type(within) == "table" then
		for _, value in within do
			if equal(value, part) then
				return true
			end
		end
	end
	return false
end

local function checks(actual: any, negated: boolean)
	local function check(holds: boolean, what: string, ...)
		if holds == negated then
			local text = "expected " .. show(actual) .. (if negated then " not " else " ") .. what
			if select("#", ...) > 0 then
				text ..= " " .. show((...))
			end
			-- At the test's line: past `check` and the method calling it.
			error(text, 3)
		end
	end
	return {
		to_be = function(expected: any)
			check(actual == expected, "to be", expected)
		end,
		to_equal = function(expected: any)
			check(equal(actual, expected), "to equal", expected)
		end,
		to_contain = function(part: any)
			check(contains(actual, part), "to contain", part)
		end,
		to_be_truthy = function()
			check(not not actual, "to be truthy")
		end,
		to_be_nil = function()
			check(actual == nil, "to be nil")
		end,
	}
end

return function(actual: any)
	local expectation = checks(actual, false)
	expectation.never = checks(actual, true)
	return expectation
end
