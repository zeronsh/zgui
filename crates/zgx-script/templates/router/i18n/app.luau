-- Notes, with routes: a list, a note at /note/:id and settings over the
-- list as a modal route. The mouse's back and forward buttons, Alt+←/→
-- (⌘[ and ⌘] on macOS) and the toolbar's buttons all go through the
-- router; `zgx types` types the route names and params. Its text is
-- messages (`ui.t`) in locales/en.json; the notes are data.
ui.i18n { dir = "locales" }
local tokens = require("./theme")
local Toolbar = require("@components/toolbar")

type Note = { id: number, title: string, body: string }

local notes: { Note } = {
	{ id = 1, title = "Welcome", body = "Open a note, then go back with the toolbar or Alt+←." },
	{ id = 2, title = "Routes", body = "Each route has a name and a path: list, note/:id and settings." },
	{ id = 3, title = "Modal routes", body = "Settings is a modal route: it opens over the list and closes back." },
}

local shown = ui.state("notes", notes)

local router = ui.router {
	routes = { list = "/", note = "/note/:id", settings = "/settings" },
}

local function list_view()
	return ui.column {
		id = "notes",
		p = "$space.3",
		gap = "$space.1",
		grow = 1,
		ui.each(shown, function(note: Note)
			return ui.button {
				id = `note-{note.id}`,
				note.title,
				on_click = function()
					router.navigate("note", { id = note.id })
				end,
			}
		end, {
			key = function(note: Note)
				return note.id
			end,
		}),
	}
end

local function note_view(params: { id: string })
	local id = tonumber(params.id)
	local note: Note? = nil
	for _, each in notes do
		if each.id == id then
			note = each
		end
	end
	if not note then
		return ui.text { id = "missing", ui.t("note.missing"), p = "$space.4", color = tokens.muted }
	end
	return ui.column {
		p = "$space.4",
		gap = "$space.2",
		ui.text { id = "note-title", note.title, literal = true, bold = true, text_size = "$font.size.lg" },
		ui.text { id = "note-body", note.body, literal = true },
	}
end

local theme = ui.state("theme", "system")

return ui.column {
	w_full = true,
	h_full = true,
	Toolbar { router = router },
	router.view {
		list = list_view(),
		note = note_view,
		-- Settings shows over the list.
		settings = list_view(),
	},
	ui.modal {
		label = ui.t("settings.title"),
		open = function()
			return router.is("settings")
		end,
		on_close = router.back,
		w = 320,
		ui.column {
			p = "$space.4",
			gap = "$space.3",
			ui.text { ui.t("settings.title"), bold = true },
			ui.segmented {
				id = "theme",
				label = ui.t("settings.theme"),
				value = theme,
				options = { "system", "light", "dark" },
			},
			ui.button { id = "close-settings", ui.t("settings.done"), on_click = router.back },
		},
	},
}
