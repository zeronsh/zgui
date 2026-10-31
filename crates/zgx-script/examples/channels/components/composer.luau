-- The composer: Enter sends and Shift+Enter breaks the line; **bold**,
-- *italic* and `code` styled as they are typed; an @name opening the
-- people who match at the caret, moved through with the arrows (the
-- textarea keeps focus, and tells a screen reader which one it is on);
-- an emoji picker; and the files going up for the message, each with its
-- progress and a ×.
local state = require("../state")
local live = require("../live")

type User = state.User

local EMOJI = {
	"😀",
	"😂",
	"🙂",
	"😉",
	"😍",
	"🤔",
	"😅",
	"😎",
	"🙌",
	"👏",
	"👍",
	"👎",
	"🙏",
	"💪",
	"🎉",
	"🔥",
	"✨",
	"💯",
	"✅",
	"❌",
	"❤️",
	"💡",
	"📌",
	"📎",
	"🚀",
	"☕",
	"🍕",
	"🐛",
	"🛠️",
	"📦",
}

-- Markdown's emphasis in a line: spans for **bold**, *italic* and `code`.
local function emphasis(line: string, carried: any): ({ any }, any)
	local spans: { any } = {}
	for from, to in line:gmatch("()%*%*[^*]+%*%*()") do
		table.insert(spans, { from = from :: number, to = (to :: number) - 1, weight = 700 })
	end
	for from, to in line:gmatch("()%f[*]%*[^*]+%*%f[^*]()") do
		table.insert(spans, { from = from :: number, to = (to :: number) - 1, italic = true })
	end
	for from, to in line:gmatch("()`[^`]+`()") do
		table.insert(spans, { from = from :: number, to = (to :: number) - 1, bg = "$surface.raised" })
	end
	return spans, carried
end

local function size_of(bytes: number): string
	if bytes < 1024 then
		return `{bytes} B`
	elseif bytes < 1024 * 1024 then
		return `{math.floor(bytes / 1024)} KB`
	end
	return string.format("%.1f MB", bytes / 1024 / 1024)
end

return ui.component(function()
	local draft = ui.computed(function(): string
		return state.drafts:get()[state.current:get()] or ""
	end)
	local text = ui.use_state("")
	-- The draft follows the channel shown.
	ui.effect(function()
		text:set(draft:get())
	end)
	local composer = ui.ref()
	local emoji_open = ui.use_state(false)
	local active = ui.use_state(1)

	-- The @name before the caret, if the caret is just after one.
	local query = ui.computed(function(): string?
		return text:get():match("@(%w*)$")
	end)
	local matches = ui.computed(function(): { User }
		local asked = query:get()
		if asked == nil then
			return {}
		end
		local found: { User } = {}
		for _, user in state.users:get() do
			if user.id ~= state.me:get() and user.name:lower():find(asked:lower(), 1, true) == 1 then
				table.insert(found, user)
			end
		end
		table.sort(found, function(a: User, b: User)
			return a.name < b.name
		end)
		return found
	end)
	local suggesting = ui.computed(function(): boolean
		return #matches:get() > 0
	end)

	local function insert(snippet: string)
		local el = composer:peek()
		if el then
			el:replace_selection(snippet)
			el:focus()
		else
			text:set(text:peek() .. snippet)
		end
	end
	local function mention(user: User)
		text:set((text:peek():gsub("@%w*$", `@{user.name} `)))
		active:set(1)
	end
	local function send()
		live.send(text:peek(), live.take_uploads())
		text:set("")
		state.drafts:set_key(state.current:peek(), nil)
	end

	local function suggestion(user: User, index: State<number>)
		return ui.button {
			id = `mention-{user.id}`,
			role = "option",
			variant = "ghost",
			justify = "start",
			selected = function()
				return active:get() == index:get()
			end,
			bg = function()
				return if active:get() == index:get() then "$selection.item" else "transparent"
			end,
			on_click = function()
				mention(user)
			end,
			user.name,
		}
	end
	local emoji_grid: ContainerProps = { cols = 10, gap = 2 }
	for _, emoji in EMOJI do
		table.insert(
			emoji_grid,
			ui.button {
				variant = "ghost",
				w = 32,
				h = 32,
				label = emoji,
				on_click = function()
					emoji_open:set(false)
					insert(emoji)
				end,
				emoji,
			}
		)
	end

	local function upload_chip(upload: state.Upload)
		return ui.row {
			id = `upload-{upload.id}`,
			gap = 6,
			px = 8,
			py = 4,
			rounded = 6,
			bg = "$surface.raised",
			items = "center",
			ui.text { `📎 {upload.name} · {size_of(upload.size)}` },
			ui.progress {
				w = 60,
				value = ui.computed(function(): number
					local now = state.uploads:get()
					for _, u in now do
						if u.id == upload.id then
							return if u.size > 0 then u.sent / u.size else 1
						end
					end
					return 1
				end),
			},
			ui.button {
				id = `cancel-{upload.id}`,
				variant = "ghost",
				label = `Remove {upload.name}`,
				"×",
				on_click = function()
					live.cancel_upload(upload.id)
				end,
			},
		}
	end

	return ui.column {
		px = 16,
		pb = 12,
		gap = 6,
		ui.text {
			id = "typing",
			size = 12,
			color = "$text.tertiary",
			function()
				local who = state.typing:get()[state.current:get()]
				return if who then `{who} is typing…` else " "
			end,
		},
		ui.row {
			gap = 6,
			wrap = true,
			ui.each(state.uploads, upload_chip, {
				key = function(u: state.Upload)
					return u.id
				end,
			}),
		},
		ui.row {
			gap = 8,
			items = "end",
			ui.button {
				id = "attach",
				variant = "ghost",
				label = "Attach files",
				"📎",
				on_click = ui.async(function()
					local files = ui.open_files({ title = "Attach", multiple = true }):await()
					for _, file in files do
						live.upload(file)
					end
				end),
			},
			ui.popover {
				open = function()
					return suggesting:get()
				end,
				on_close = function() end,
				at_caret = true,
				w = 200,
				p = 4,
				label = "People",
				anchor = ui.textarea {
					id = "composer",
					ref = composer,
					grow = 1,
					label = function()
						return `Message #{state.current:get()}`
					end,
					placeholder = function()
						return `Message #{state.current:get()}`
					end,
					value = text,
					highlight = emphasis,
					active_descendant = function()
						local found = matches:get()
						local user = found[active:get()]
						return if user then `mention-{user.id}` else nil
					end,
					on_key = function(e: KeyEvent)
						if suggesting:peek() then
							local count = #matches:peek()
							if e.key == "ArrowDown" or e.key == "ArrowUp" then
								local step = if e.key == "ArrowDown" then 1 else -1
								active:set(((active:peek() or 1) - 1 + step) % count + 1)
								return true
							elseif e.key == "Enter" or e.key == "Tab" then
								mention(matches:peek()[math.min(active:peek() or 1, count)])
								return true
							elseif e.key == "Escape" then
								text:set(text:peek() .. " ")
								return true
							end
						end
						if e.key == "Enter" and not e.shift then
							send()
							return true
						end
						return false
					end,
					on_blur = function()
						state.drafts:set_key(state.current:peek(), text:peek())
					end,
				},
				ui.column {
					role = "listbox",
					gap = 2,
					ui.each(matches, suggestion, {
						key = function(u: User)
							return u.id
						end,
					}),
				},
			},
			ui.popover {
				open = emoji_open,
				label = "Emoji",
				w = 360,
				p = 8,
				anchor = ui.button {
					id = "emoji",
					variant = "ghost",
					label = "Emoji",
					"🙂",
					on_click = function()
						emoji_open:set(not emoji_open:peek())
					end,
				},
				ui.grid(emoji_grid),
			},
			ui.button {
				id = "send",
				variant = "primary",
				"Send",
				on_click = send,
			},
		},
	}
end)
