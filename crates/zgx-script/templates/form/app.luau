-- Profile: a form checked as it is typed, saved with Save (enabled once
-- something changed), reverted with Revert, and a toast when it is saved.
-- `save` keeps it in a persisted state; make it a `ui.fetch` to send it to
-- a server, and the form shows its pending state and failure as they come.
local tokens = require("./theme")
local Field = require("@components/field")

type Profile = { name: string, email: string, theme: string }

local saved = ui.persist("profile", { name = "Ada Lovelace", email = "ada@example.com", theme = "system" } :: Profile)

local function save(profile: Profile): Profile
	saved:set(profile)
	return profile
end

local form = ui.form {
	fields = { name = "", email = "", theme = "system" },
	values = saved,
	validate = {
		name = function(name: string)
			return if name == "" then "A name is required" else nil
		end,
		email = function(email: string)
			return if email:find("@") then nil else "That is not an email address"
		end,
	},
	submit = function(values)
		return save { name = values.name, email = values.email, theme = values.theme }
	end,
	on_success = function(profile: Profile, _, form)
		form.reset(profile)
		ui.toast { title = "Profile saved", kind = "success" }
	end,
}

return ui.column {
	h_full = true,
	ui.row {
		px = "$space.4",
		py = "$space.3",
		bg = tokens.card,
		ui.text { "Profile", bold = true, text_size = "$font.size.lg" },
	},
	ui.column {
		id = "profile",
		p = "$space.4",
		gap = "$space.4",
		w = 420,
		Field { form = form, name = "name", label = "Name" },
		Field { form = form, name = "email", label = "Email" },
		ui.column {
			gap = "$space.1",
			ui.text { "Theme", color = tokens.muted, text_size = "$font.size.sm" },
			ui.segmented {
				id = "theme",
				label = "Theme",
				field = form.fields.theme,
				options = { "system", "light", "dark" },
			},
		},
		ui.presence(form.failure, function(message: string)
			return ui.text { id = "failure", message, color = "$danger" }
		end),
		ui.row {
			gap = "$space.2",
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
					form.reset(saved:peek())
				end,
			},
		},
	},
}
