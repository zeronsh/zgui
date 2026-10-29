-- Inbox: a list of messages searched as you type (debounced) and loaded
-- page by page as you scroll, a thread view, settings in a modal, folders
-- you drag to reorder, and routing between them with back and forward and
-- deep links (mail://thread/42).
local api = require("./api")
local Avatar = require("@components/avatar")

type Message = api.Message
type Route = { name: string, id: number? }

-- Routing: where we are, what's behind and ahead.
local route = ui.state("route", { name = "list" } :: Route)
local behind = ui.state("behind", {} :: { Route })
local ahead = ui.state("ahead", {} :: { Route })
local function navigate(to: Route)
	behind:push(route:peek())
	ahead:set({})
	route:set(to)
end
local function back()
	local stack = behind:peek()
	if #stack == 0 then
		return
	end
	ahead:push(route:peek())
	route:set(stack[#stack])
	behind:remove(#stack)
end
local function forward()
	local stack = ahead:peek()
	if #stack == 0 then
		return
	end
	behind:push(route:peek())
	route:set(stack[#stack])
	ahead:remove(#stack)
end
-- Deep links: mail://thread/42, mail://settings.
ui.on_open(function(e)
	for _, url in e.urls do
		local id = url:match("^mail://thread/(%d+)$")
		if id then
			navigate({ name = "thread", id = tonumber(id) })
		elseif url == "mail://settings" then
			navigate({ name = "settings" })
		end
	end
end)

-- Search, debounced: the query as typed, and as searched 250 ms later.
local query = ui.state("query", "")
local searched = ui.state("searched", "")
local cancel_search: (() -> ())? = nil
ui.effect(function()
	local typed = query:get()
	if cancel_search then
		cancel_search()
	end
	cancel_search = ui.after(250, function()
		searched:set(typed)
	end)
end)

-- The messages found, a page at a time.
local messages = ui.state("messages", {} :: { Message })
local next_cursor = ui.state("next", 0 :: number?)
local loading = ui.state("loading", false)
local failure = ui.state("failure", nil :: string?)
local loader: Task? = nil
local function load(from: number)
	if loader then
		loader:cancel()
	end
	loading:set(true)
	failure:set(nil)
	local q = searched:peek()
	loader = ui.task(function()
		local page, err = ui.try(api.messages, q, from)
		loading:set(false)
		if err then
			failure:set(err)
			return
		end
		local found = page :: api.Page
		if from == 0 then
			messages:set(found.messages)
		else
			for _, m in found.messages do
				messages:push(m)
			end
		end
		next_cursor:set(found.next)
	end)
end
ui.effect(function()
	searched:get()
	load(0)
end)

-- Infinite scroll: near the end of what's loaded, load the next page.
local ROW = 44
local offset = ui.state("offset", 0)
ui.effect(function()
	local at = offset:get()
	local count = #messages:get()
	local nxt = next_cursor:peek()
	if nxt and not loading:peek() and at + 600 > count * ROW - 200 then
		load(nxt)
	end
end)

local function row(m: Message)
	return ui.row {
		id = `message-{m.id}`,
		h = ROW,
		px = 16,
		gap = 12,
		items = "center",
		on_click = function()
			navigate({ name = "thread", id = m.id })
		end,
		ui.text { m.from, literal = true, w = 90, bold = true },
		ui.text { m.subject, literal = true, grow = 1 },
	}
end

local function list_view()
	return ui.column {
		grow = 1,
		ui.row {
			p = 12,
			gap = 8,
			ui.input { id = "search", label = "Search", value = query, placeholder = "Search mail", grow = 1 },
		},
		ui.switch(function()
			if failure:get() then
				return "failed"
			elseif #messages:get() == 0 then
				return if loading:get() then "loading" else "empty"
			end
			return "ready"
		end, {
			loading = ui.text { id = "loading", "Loading…", p = 16, color = "$text.secondary" },
			empty = ui.text { id = "empty", "No messages match.", p = 16, color = "$text.secondary" },
			failed = ui.column {
				p = 16,
				gap = 8,
				ui.text {
					id = "failure",
					function()
						return `Couldn't load your mail: {failure:get()}`
					end,
					color = "$danger",
				},
				ui.button {
					id = "retry",
					"Try again",
					on_click = function()
						load(0)
					end,
				},
			},
			ready = ui.list {
				id = "messages",
				data = messages,
				height = ROW,
				w_full = true,
				h = 600,
				offset = offset,
				key = function(m: Message)
					return m.id
				end,
				row = row,
			},
		}),
	}
end

local thread = ui.resource(function(): api.Thread?
	local r = route:get()
	if r.name ~= "thread" or r.id == nil then
		return nil
	end
	return api.thread(r.id)
end)

local function thread_view()
	return ui.column {
		p = 16,
		gap = 8,
		ui.switch(thread.status, {
			loading = ui.text { "Loading…", color = "$text.secondary" },
			failed = ui.text {
				function()
					return `Couldn't open it: {thread.error:get()}`
				end,
				color = "$danger",
			},
			ready = ui.column {
				gap = 8,
				ui.text {
					id = "thread-subject",
					function()
						local t = thread.data:get()
						return if t then t.subject else ""
					end,
					bold = true,
					text_size = "$font.size.lg",
				},
				ui.text {
					id = "thread-body",
					function()
						local t = thread.data:get()
						return if t then t.body else ""
					end,
				},
			},
		}),
	}
end

-- Folders, dragged to reorder (saved to the server), sliding into place.
local folders = ui.state("folders", {} :: { string })
ui.task(function()
	local loaded = ui.try(api.folders)
	if loaded then
		folders:set(loaded)
	end
end)
local function move(name: string, to: string)
	local list = table.clone(folders:peek())
	local from = table.find(list, name)
	local at = table.find(list, to)
	if not from or not at or from == at then
		return
	end
	table.remove(list, from)
	table.insert(list, at, name)
	folders:set(list)
	ui.task(function()
		ui.try(api.save_folders, list)
	end)
end
local function folder(name: string)
	return ui.row {
		id = `folder-{name}`,
		px = 16,
		py = 6,
		ui.text { name, literal = true },
		on_drag = function()
			return name
		end,
		drop_when = function(payload: any)
			return type(payload) == "string" and payload ~= name
		end,
		on_drop = function(payload: any)
			move(payload :: string, name)
		end,
	}
end

-- Settings, a modal route: open while the route is settings, and closing
-- it goes back.
local settings_open = ui.state("settings_open", false)
ui.effect(function()
	settings_open:set(route:get().name == "settings")
end)
ui.effect(function()
	if not settings_open:get() and route:peek().name == "settings" then
		back()
	end
end)
local theme = ui.state("theme", "system")

local user = ui.state("user", { name = "Ada Lovelace" })

return ui.column {
	id = "inbox",
	w_full = true,
	h_full = true,
	provide = { user = user, theme = theme },
	ui.row {
		px = 12,
		py = 8,
		gap = 8,
		items = "center",
		bg = "$surface.raised",
		ui.button { id = "back", "‹", on_click = back },
		ui.button { id = "forward", "›", on_click = forward },
		ui.text { "Inbox", bold = true, grow = 1 },
		ui.button {
			id = "open-settings",
			"Settings",
			on_click = function()
				navigate({ name = "settings" })
			end,
		},
		Avatar {},
	},
	ui.row {
		grow = 1,
		ui.column {
			id = "folders",
			w = 160,
			py = 8,
			ui.each(folders, folder, {
				key = function(name: string)
					return name
				end,
				animate = true,
				enter = { opacity = { 0, 1 }, duration = 200 },
				exit = { opacity = 0, duration = 150 },
			}),
		},
		ui.switch(function()
			local name = route:get().name
			return if name == "settings" then "list" else name
		end, {
			list = list_view(),
			thread = thread_view(),
		}),
	},
	ui.modal {
		open = settings_open,
		w = 360,
		ui.column {
			p = 20,
			gap = 12,
			ui.text { "Settings", bold = true },
			ui.segmented { id = "theme", value = theme, options = { "system", "light", "dark" } },
			ui.button {
				id = "close-settings",
				"Done",
				on_click = function()
					settings_open:set(false)
				end,
			},
		},
	},
}
