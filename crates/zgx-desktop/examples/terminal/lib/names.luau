-- What a pane is called and which icon it shows, from what runs in it.
--
-- At a shell prompt the pane is its directory ("comet-native", "~"). With a
-- program running it is the title the program set, else the program's name;
-- a title that starts with a symbol (Claude Code's "✳ Fix the build") shows
-- that symbol as its icon. The program picks the icon otherwise.
local theme = require("../theme")
local icons = require("../icons")

local SHELLS = {
	zsh = true,
	bash = true,
	fish = true,
	sh = true,
	dash = true,
	nu = true,
	tcsh = true,
	ksh = true,
	xonsh = true,
	pwsh = true,
	login = true,
}

-- Programs by kind: the icon each shows, and a tint for agents.
export type Kind = { icon: icons.Name, tint: string? }
local KINDS: { [string]: Kind } = {}
local function kind(names: { string }, icon: icons.Name, tint: string?)
	for _, name in names do
		KINDS[name] = { icon = icon, tint = tint }
	end
end
kind({ "claude" }, "claude", theme.claude)
kind({ "codex" }, "openai", "#ffffff")
kind({ "omp", "pi", "opencode", "aider", "gemini", "copilot", "amp", "cursor-agent", "goose" }, "agent", theme.accent)
kind({ "nvim", "vim", "vi", "hx", "helix", "nano", "emacs", "micro", "kak", "zed" }, "editor")
kind({ "ssh", "mosh", "et", "telnet" }, "globe")
kind({ "htop", "btop", "top", "btm", "bottom", "glances", "atop" }, "gauge")
kind({ "git", "lazygit", "tig", "gitui", "gh" }, "branch")
kind({ "docker", "podman", "kubectl", "k9s", "lazydocker" }, "box")
kind(
	{ "cargo", "rustc", "go", "make", "cmake", "just", "ninja", "zig", "gradle", "mvn", "xcodebuild", "swift" },
	"code"
)
kind({ "less", "more", "man", "bat", "most", "glow" }, "doc")
kind({ "psql", "mysql", "sqlite3", "redis-cli", "mongosh", "duckdb" }, "database")

-- What agents call themselves, where it is not their command.
local LABELS: { [string]: string } = {
	claude = "Claude Code",
	codex = "Codex",
	opencode = "OpenCode",
	["cursor-agent"] = "Cursor Agent",
}

-- The symbols Claude Code's title starts with, idle (✳) and working: it
-- alternates ◐ and ◑ (2.1.3xx; earlier versions turned ✶ ✻ ✽ ✢ ·).
local CLAUDE_SPINNER = {}
for _, glyph in { "✳", "◐", "◑", "✶", "✻", "✽", "✢", "·" } do
	CLAUDE_SPINNER[glyph] = true
end
-- The braille spinner Codex (and others) turn before their title while
-- they work.
local SPINNER = {}
for _, glyph in { "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏" } do
	SPINNER[glyph] = true
end

local function trim(s: string): string
	return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

-- A file URL or path, as a path ("file://host/Users/a%20b" -> "/Users/a b").
local function path_of(pwd: string): string
	local path = pwd:gsub("^file://[^/]*", "")
	path = path:gsub("%%(%x%x)", function(hex)
		-- Two hex digits always convert.
		return string.char(tonumber(hex, 16) or 0)
	end)
	return path
end

-- The last part of a path; home is "~".
local function short(path: string, home: string): string
	path = path:gsub("/+$", "")
	if path == "" then
		return "/"
	end
	if path == home or path == "~" then
		return "~"
	end
	return path:match("([^/]+)$") or path
end

-- A shell's title is often "user@host:dir" or a directory: the directory.
local function directory_in(title: string): string?
	local dir = title:match(":%s*([~/][^:]*)$") or title:match("^([~/].*)$")
	return dir and trim(dir)
end

local M = {}

-- Coding agents, by the icon their kind shows (Claude Code, Codex, the
-- rest): what the sidebar lists.
local AGENT_ICONS: { [string]: boolean } = { claude = true, openai = true, agent = true }

-- `process`'s kind if it is a coding agent.
function M.agent(process: string): Kind?
	local known = KINDS[process]
	return if known and AGENT_ICONS[known.icon] then known else nil
end

-- `process`'s name for people ("claude" is "Claude Code").
function M.label(process: string): string
	return LABELS[process] or (process:sub(1, 1):upper() .. process:sub(2))
end

function M.is_shell(process: string): boolean
	return process == "" or SHELLS[process] == true
end

export type Description = {
	name: string,
	icon: icons.Name,
	tint: string?,
	glyph: string?,
	busy: boolean,
}

function M.describe(term: Term, home: string): Description
	return M.describe_parts(term.process, term.title, term.pwd, home)
end

-- `describe` from what runs, its title and its directory (a session the
-- session server reports, shown in no window).
function M.describe_parts(process: string, title: string, pwd: string, home: string): Description
	title = trim(title)
	pwd = path_of(pwd)
	if M.is_shell(process) then
		local dir = if pwd ~= "" then pwd else directory_in(title)
		return {
			name = if dir then short(dir, home) else (if title ~= "" then title else "Terminal"),
			icon = "terminal",
			busy = false,
		}
	end
	local known: Kind = KINDS[process] or { icon = "run" }
	-- A leading symbol (anything not ASCII) is the program's own icon,
	-- except Claude Code's spinner: Claude shows its mark.
	local glyph, rest = title:match("^([\128-\255]+)%s+(.+)$")
	-- (The captures match together.)
	if glyph and rest and (known.icon == "claude" or CLAUDE_SPINNER[glyph]) then
		return { name = trim(rest), icon = "claude", tint = theme.claude, busy = true }
	end
	-- A spinner frame says it works, not what it is: the program's icon.
	if glyph and rest and SPINNER[glyph] then
		return { name = trim(rest), icon = known.icon, tint = known.tint, busy = true }
	end
	if glyph and rest then
		return { name = trim(rest), icon = known.icon, tint = known.tint, glyph = glyph, busy = true }
	end
	local name = process
	if title ~= "" and not directory_in(title) then
		name = title
	end
	return { name = name, icon = known.icon, tint = known.tint, busy = true }
end

-- A browser pane: the page's title, else its host; "New Tab" before it
-- has a page.
function M.describe_page(web: Web): Description
	local url = web.url
	if url == "" or url == "about:blank" then
		return { name = "New Tab", icon = "browser", busy = false }
	end
	local title = trim(web.title)
	local host = url:match("^%a[%w+.-]*://([^/?#]+)") or url
	host = host:gsub("^www%.", "")
	return { name = if title ~= "" then title else host, icon = "browser", busy = false }
end

M.path_of = path_of
M.short = short

return M
