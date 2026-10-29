-- Settings: a profile loaded from a server, edited in a form checked as it
-- is typed and saved with an undo, and a notifications switch that saves
-- at once (optimistically), going back if the server refuses.
type Settings = { name: string, email: string, theme: string, notifications: boolean }

local api = "https://settings.example/api"

local function request(method: string, body: Settings?): Settings
	local response = ui.fetch(`{api}/settings`, { method = method, body = body }):await()
	if not response.ok then
		local message = if response.json and response.json.error
			then response.json.error
			else `The server answered {response.status}`
		error(message, 0)
	end
	return response.json :: Settings
end

local settings = ui.resource(function(): Settings?
	return request("GET")
end)

-- The form takes the server's values as they arrive (and again as they
-- change), a field the person has edited keeping what they typed.
local form = ui.form {
	fields = { name = "", email = "", theme = "system" },
	values = settings.data,
	validate = {
		name = function(name: string)
			return if name == "" then "A name is required" else nil
		end,
		email = function(email: string)
			return if email:find("@") then nil else "That is not an email address"
		end,
	},
	submit = function(values)
		local current = settings.data:peek() :: Settings
		return request("PUT", {
			name = values.name,
			email = values.email,
			theme = values.theme,
			notifications = current.notifications,
		})
	end,
	on_success = function(saved: Settings, _, form)
		local before = settings.data:peek() :: Settings
		settings.data:set(saved)
		form.reset(saved)
		ui.toast {
			title = "Settings saved",
			kind = "success",
			action = {
				label = "Undo",
				run = function()
					ui.task(function()
						local restored, err = ui.try(request, "PUT", before)
						if err then
							ui.toast { title = "Couldn't undo", body = err, kind = "error" }
							return
						end
						settings.data:set(restored)
						form.reset(restored)
					end)
				end,
			},
		}
	end,
}

-- The switch saves at once: shown as changed, and changed back if the
-- server refuses.
local function set_notifications(on: boolean)
	local before = settings.data:peek() :: Settings
	local next = table.clone(before)
	next.notifications = on
	settings.data:set(next)
	ui.task(function()
		local _, err = ui.try(request, "PUT", next)
		if err then
			settings.data:set(before)
			ui.toast { title = "Couldn't change notifications", body = err, kind = "error" }
		end
	end)
end

local function field(name: string, label: string)
	local f = form.fields[name]
	return ui.column {
		gap = 4,
		ui.text { label, color = "$text.secondary", text_size = "$font.size.sm" },
		ui.input { id = name, label = label, field = f, w_full = true },
		ui.presence(f.error, function(problem: string)
			return ui.text { problem, color = "$danger", text_size = "$font.size.sm" }
		end),
	}
end

local function body()
	return ui.column {
		gap = 16,
		p = 24,
		w = 420,
		field("name", "Name"),
		field("email", "Email"),
		ui.column {
			gap = 4,
			ui.text { "Theme", color = "$text.secondary", text_size = "$font.size.sm" },
			ui.segmented { id = "theme", field = form.fields.theme, options = { "system", "light", "dark" } },
		},
		ui.checkbox {
			id = "notifications",
			label = "Email me about activity",
			value = function()
				local loaded = settings.data:get()
				return loaded ~= nil and loaded.notifications
			end,
			on_change = set_notifications,
		},
		ui.presence(form.failure, function(message: string)
			return ui.text { id = "failure", message, color = "$danger" }
		end),
		ui.row {
			gap = 8,
			ui.button {
				id = "save",
				function()
					return if form.pending:get() then "Saving…" else "Save"
				end,
				disabled = function()
					return form.pending:get() or not form.dirty:get()
				end,
				on_click = form.submit,
			},
			ui.button {
				id = "revert",
				"Revert",
				disabled = function()
					return not form.dirty:get()
				end,
				on_click = function()
					form.reset(settings.data:peek())
				end,
			},
		},
	}
end

return ui.column {
	id = "settings",
	w_full = true,
	h_full = true,
	ui.row {
		px = 24,
		py = 12,
		bg = "$surface.raised",
		ui.text { "Settings", bold = true, text_size = "$font.size.lg" },
	},
	ui.switch(settings.status, {
		loading = ui.text { id = "loading", "Loading…", color = "$text.secondary", p = 24 },
		failed = ui.column {
			p = 24,
			gap = 8,
			ui.text {
				id = "load-error",
				function()
					return `Couldn't load your settings: {settings.error:get()}`
				end,
				color = "$danger",
			},
			ui.button { id = "retry", "Try again", on_click = settings.reload },
		},
		ready = body(),
	}),
}
