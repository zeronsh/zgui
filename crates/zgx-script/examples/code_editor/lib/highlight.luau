--!strict
-- Syntax colours a line at a time: a tokenizer per language, given a line
-- and what the line before left open, returns the line's spans (`{ from,
-- to, colour }`, bytes from 1) and what it leaves the next line. The editor
-- asks for the lines in view, and again only for those that change.
--
-- States: 0 nothing open, 1 a block comment, 2 a string, 3 a Lua long
-- string or comment (its level in the tens: 30 is `[[`, 31 `[=[`, …).

local M = {}

-- Theme tokens, so a light or dark theme colours them its way.
local KEYWORD: string = "$accent"
local STRING: string = "$success"
local NUMBER: string = "$warning"
local COMMENT: string = "$text.tertiary"
local TYPE: string = "$info"
local MACRO: string = "$danger"

local function set(words: { string }): { [string]: boolean }
	local out = {}
	for _, word in words do
		out[word] = true
	end
	return out
end

local byte, find, sub = string.byte, string.find, string.sub
local SLASH, STAR, QUOTE, BACKSLASH, HASH, APOSTROPHE = 47, 42, 34, 92, 35, 39
local MINUS, OPEN, EQUALS, COLON = 45, 91, 61, 58

local function is_word(b: number): boolean
	return (b >= 97 and b <= 122) or (b >= 65 and b <= 90) or b == 95
end
local function is_digit(b: number): boolean
	return b >= 48 and b <= 57
end

-- A quoted string from `at` (its opening quote): where it ends, or nil if
-- it runs on past the line.
local function closing(line: string, from: number, quote: string): number?
	local at = from
	while true do
		local q = find(line, "[" .. quote .. "\\]", at)
		if not q then
			return nil
		end
		if byte(line, q) == BACKSLASH then
			at = q + 2
		else
			return q
		end
	end
end

local RUST_KEYWORDS = set {
	"as",
	"async",
	"await",
	"break",
	"const",
	"continue",
	"crate",
	"dyn",
	"else",
	"enum",
	"extern",
	"false",
	"fn",
	"for",
	"if",
	"impl",
	"in",
	"let",
	"loop",
	"match",
	"mod",
	"move",
	"mut",
	"pub",
	"ref",
	"return",
	"self",
	"Self",
	"static",
	"struct",
	"super",
	"trait",
	"true",
	"type",
	"unsafe",
	"use",
	"where",
	"while",
}

function M.rust(line: string, state: number): ({ { any } }, number)
	local spans: { { any } } = {}
	local n = #line
	local i = 1
	if state == 1 then
		local e = find(line, "*/", 1, true)
		if not e then
			return { { 1, n, COMMENT } }, 1
		end
		spans[1] = { 1, e + 1, COMMENT }
		i = e + 2
	elseif state == 2 then
		local q = closing(line, 1, '"')
		if not q then
			return { { 1, n, STRING } }, 2
		end
		spans[1] = { 1, q, STRING }
		i = q + 1
	end
	while i <= n do
		local b = byte(line, i)
		if b == SLASH and byte(line, i + 1) == SLASH then
			spans[#spans + 1] = { i, n, COMMENT }
			break
		elseif b == SLASH and byte(line, i + 1) == STAR then
			local e = find(line, "*/", i + 2, true)
			if not e then
				spans[#spans + 1] = { i, n, COMMENT }
				return spans, 1
			end
			spans[#spans + 1] = { i, e + 1, COMMENT }
			i = e + 2
		elseif b == QUOTE then
			local q = closing(line, i + 1, '"')
			if not q then
				spans[#spans + 1] = { i, n, STRING }
				return spans, 2
			end
			spans[#spans + 1] = { i, q, STRING }
			i = q + 1
		elseif b == APOSTROPHE then
			-- A character ('a', '\n'), or a lifetime ('a) left as it is.
			local q = closing(line, i + 1, "'")
			if q and q - i <= 3 + (byte(line, i + 1) == BACKSLASH and 2 or 0) then
				spans[#spans + 1] = { i, q, STRING }
				i = q + 1
			else
				i += 1
			end
		elseif b == HASH and (byte(line, i + 1) == OPEN or byte(line, i + 2) == OPEN) then
			local e = find(line, "]", i, true) or n
			spans[#spans + 1] = { i, e, MACRO }
			i = e + 1
		elseif is_word(b) then
			local _, e = find(line, "^[%w_]+", i)
			local last = e :: number
			local word = sub(line, i, last)
			if RUST_KEYWORDS[word] then
				spans[#spans + 1] = { i, last, KEYWORD }
			elseif byte(line, last + 1) == 33 then -- `!`: a macro
				spans[#spans + 1] = { i, last + 1, MACRO }
				last += 1
			elseif b >= 65 and b <= 90 then
				spans[#spans + 1] = { i, last, TYPE }
			end
			i = last + 1
		elseif is_digit(b) then
			local _, e = find(line, "^[%w_%.]+", i)
			spans[#spans + 1] = { i, e :: number, NUMBER }
			i = (e :: number) + 1
		else
			i += 1
		end
	end
	return spans, 0
end

local LUA_KEYWORDS = set {
	"and",
	"break",
	"continue",
	"do",
	"else",
	"elseif",
	"end",
	"export",
	"false",
	"for",
	"function",
	"if",
	"in",
	"local",
	"nil",
	"not",
	"or",
	"repeat",
	"return",
	"then",
	"true",
	"type",
	"until",
	"while",
}

-- A long bracket (`[[`, `[==[`) at `at`: its level, or nil.
local function long_open(line: string, at: number): (number?, number?)
	local s, e = find(line, "^%[=*%[", at)
	if not s then
		return nil, nil
	end
	return (e :: number) - (s :: number) - 1, e
end
local function long_close(line: string, from: number, level: number): number?
	local _, e = find(line, "]" .. string.rep("=", level) .. "]", from, true)
	return e
end

function M.lua(line: string, state: number): ({ { any } }, number)
	local spans: { { any } } = {}
	local n = #line
	local i = 1
	if state >= 30 then
		local level = state - 30
		local comment = level >= 50
		if comment then
			level -= 50
		end
		local e = long_close(line, 1, level)
		local colour = if comment then COMMENT else STRING
		if not e then
			return { { 1, n, colour } }, state
		end
		spans[1] = { 1, e, colour }
		i = e + 1
	end
	while i <= n do
		local b = byte(line, i)
		if b == MINUS and byte(line, i + 1) == MINUS then
			local level, e = long_open(line, i + 2)
			if level then
				local close = long_close(line, (e :: number) + 1, level)
				if not close then
					spans[#spans + 1] = { i, n, COMMENT }
					return spans, 30 + 50 + level
				end
				spans[#spans + 1] = { i, close, COMMENT }
				i = close + 1
			else
				spans[#spans + 1] = { i, n, COMMENT }
				break
			end
		elseif b == OPEN and (byte(line, i + 1) == OPEN or byte(line, i + 1) == EQUALS) then
			local level, e = long_open(line, i)
			if level then
				local close = long_close(line, (e :: number) + 1, level)
				if not close then
					spans[#spans + 1] = { i, n, STRING }
					return spans, 30 + level
				end
				spans[#spans + 1] = { i, close, STRING }
				i = close + 1
			else
				i += 1
			end
		elseif b == QUOTE or b == APOSTROPHE or b == 96 then -- " ' `
			local quote = if b == QUOTE then '"' elseif b == APOSTROPHE then "'" else "`"
			local q = closing(line, i + 1, quote) or n
			spans[#spans + 1] = { i, q, STRING }
			i = q + 1
		elseif is_word(b) then
			local _, e = find(line, "^[%w_]+", i)
			local last = e :: number
			local word = sub(line, i, last)
			if LUA_KEYWORDS[word] then
				spans[#spans + 1] = { i, last, KEYWORD }
			elseif word == "ui" or word == "self" then
				spans[#spans + 1] = { i, last, TYPE }
			end
			i = last + 1
		elseif is_digit(b) then
			local _, e = find(line, "^[%w_%.]+", i)
			spans[#spans + 1] = { i, e :: number, NUMBER }
			i = (e :: number) + 1
		else
			i += 1
		end
	end
	return spans, 0
end

local JSON_WORDS = set { "true", "false", "null" }

function M.json(line: string, _: number): ({ { any } }, number)
	local spans: { { any } } = {}
	local n = #line
	local i = 1
	while i <= n do
		local b = byte(line, i)
		if b == QUOTE then
			local q = closing(line, i + 1, '"') or n
			-- A key is followed by its colon.
			local after = find(line, "%S", q + 1)
			local key = after ~= nil and byte(line, after) == COLON
			spans[#spans + 1] = { i, q, if key then TYPE else STRING }
			i = q + 1
		elseif is_digit(b) or b == MINUS then
			local _, e = find(line, "^%-?[%d%.eE+%-]+", i)
			if e then
				spans[#spans + 1] = { i, e, NUMBER }
				i = e + 1
			else
				i += 1
			end
		elseif is_word(b) then
			local _, e = find(line, "^%a+", i)
			local last = e :: number
			if JSON_WORDS[sub(line, i, last)] then
				spans[#spans + 1] = { i, last, KEYWORD }
			end
			i = last + 1
		else
			i += 1
		end
	end
	return spans, 0
end

-- The language a file's name says (`main.rs`: Rust), or plain text.
function M.language(name: string?): string
	local extension = name and string.match(name, "%.(%w+)$")
	extension = extension and string.lower(extension)
	if extension == "rs" then
		return "rust"
	elseif extension == "lua" or extension == "luau" then
		return "lua"
	elseif extension == "json" then
		return "json"
	end
	return "text"
end

M.names = { rust = "Rust", lua = "Luau", json = "JSON", text = "Plain Text" }

return M
