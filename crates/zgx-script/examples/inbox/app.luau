-- Inbox: a list of messages searched as you type (debounced) and loaded
-- page by page as you scroll, a thread view, settings in a modal route,
-- folders you drag (or move with Alt+↑/↓) to reorder, and routing between
-- them with back and forward and deep links (mail://thread/42).
local api = require("./api")
local Avatar = require("@components/avatar")

type Message = api.Message
type Route = { name: string, id: number? }

-- Where we are: the list, a thread, or settings over the list. The mouse's
-- back and forward buttons, Alt+←/→ (⌘[ ⌘] on macOS) and deep links
-- (mail://thread/42) all go through it.
local router = ui.router {
	routes = { list = "/", thread = "/thread/:id", settings = "/settings" },
	scheme = "mail",
}

-- Search as you type, asked once the typing stops.
local query = ui.state("query", "")
local searched = ui.debounced(query, 250)

-- The messages found, a page at a time: the list asks for the next as its
-- end comes in reach (and fills a short view by itself).
local pages = ui.paged(function(cursor: number?): { items: { Message }, next: number? }
	local page = api.messages(searched:get(), cursor or 0)
	return { items = page.messages, next = page.next }
end)

local ROW = 44
local function row(m: Message)
	return ui.row {
		id = `message-{m.id}`,
		h = ROW,
		px = 16,
		gap = 12,
		items = "center",
		on_click = function()
			router.navigate("thread", { id = m.id })
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
			if pages.error then
				return "failed"
			elseif #pages.items:get() == 0 then
				return if pages.loading:get() then "loading" else "empty"
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
						return `Couldn't load your mail: {pages.error}`
					end,
					color = "$danger",
				},
				ui.button { id = "retry", "Try again", on_click = pages.reload },
			},
			ready = ui.list {
				id = "messages",
				data = pages.items,
				height = ROW,
				w_full = true,
				h = 600,
				on_end = pages.more,
				key = function(m: Message)
					return m.id
				end,
				row = row,
			},
		}),
	}
end

local thread = ui.resource(function(): api.Thread?
	local r = router.route:get()
	if r.name ~= "thread" then
		return nil
	end
	return api.thread(tonumber(r.params.id) :: number)
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

-- Folders, sliding into place as they move.
local folders = ui.state("folders", {} :: { string })
ui.task(function()
	local loaded = ui.try(api.folders)
	if loaded then
		folders:set(loaded)
	end
end)
-- Dragged (or moved with Alt+↑/↓) into a new order, saved to the server.
local function reorder(from: number, to: number)
	local list = table.clone(folders:peek())
	table.insert(list, to, table.remove(list, from) :: string)
	folders:set(list)
	ui.task(function()
		ui.try(api.save_folders, list)
	end)
end
local function folder(name: string)
	return ui.row { id = `folder-{name}`, px = 16, py = 6, ui.text { name, literal = true } }
end

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
		ui.button {
			id = "back",
			label = "Back",
			"‹",
			disabled = function()
				return not router.can_back:get()
			end,
			on_click = router.back,
		},
		ui.button {
			id = "forward",
			label = "Forward",
			"›",
			disabled = function()
				return not router.can_forward:get()
			end,
			on_click = router.forward,
		},
		ui.text { "Inbox", bold = true, grow = 1 },
		ui.button {
			id = "open-settings",
			"Settings",
			on_click = function()
				router.navigate("settings")
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
				reorder = reorder,
				animate = true,
				enter = { opacity = { 0, 1 }, duration = 200 },
				exit = { opacity = 0, duration = 150 },
			}),
		},
		-- Settings shows over the list.
		router.view {
			list = list_view(),
			thread = thread_view(),
			settings = list_view(),
		},
	},
	ui.modal {
		label = "Settings",
		open = function()
			return router.is("settings")
		end,
		on_close = router.back,
		w = 360,
		ui.column {
			p = 20,
			gap = 12,
			ui.text { "Settings", bold = true },
			ui.segmented { id = "theme", value = theme, options = { "system", "light", "dark" } },
			ui.button {
				id = "close-settings",
				"Done",
				on_click = router.back,
			},
		},
	},
}
