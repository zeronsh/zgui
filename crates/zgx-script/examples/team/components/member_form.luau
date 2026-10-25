-- A person's details, checked as they are typed: each field says what is
-- wrong once it has been left (or a save tried), saving waits on the
-- server and shows what it answered.
local api = require("../api")

type Props = {
	member: api.Member,
	-- Emails taken by others, which a new one may not repeat.
	taken: { string },
	on_saved: (member: api.Member) -> (),
	on_cancel: () -> (),
}

local ROLES = { "Engineer", "Designer", "Manager" }

local function check_name(name: string): string?
	if name:match("^%s*$") then
		return "A name is required"
	end
	return nil
end

local function check_email(email: string, taken: { string }): string?
	if email:match("^%s*$") then
		return "An email is required"
	elseif not email:match("^[^@%s]+@[^@%s]+%.[^@%s]+$") then
		return "That is not an email address"
	elseif table.find(taken, email:lower()) then
		return "Someone already has that email"
	end
	return nil
end

-- A labelled input bound to its form's field: its error shows under it
-- once it has been left (or a save tried), and assistive technology hears
-- it as the field's.
local function field(form: Form, name: string, label: string)
	local f = form.fields[name]
	return ui.column {
		gap = 4,
		ui.text { label, text_size = "$font.size.sm", color = "$text.secondary" },
		ui.input {
			id = name,
			label = label,
			field = f,
			w_full = true,
			border = 1,
			border_color = function()
				return if f.error:get() then "$danger" else "$border"
			end,
		},
		ui.presence(f.error, function(problem: string)
			return ui.text { id = `{name}-error`, problem, color = "$danger", text_size = "$font.size.sm" }
		end),
	}
end

return ui.component(function(props: Props)
	local member = props.member
	local form = ui.form {
		fields = {
			name = member.name,
			email = member.email,
			role = if member.role ~= "" then member.role else ROLES[1],
		},
		validate = {
			name = check_name,
			email = function(email: string)
				return check_email(email, props.taken)
			end,
		},
		-- Enter in a field, or Save: checked, then sent; `form.pending`
		-- while it waits, `form.failure` what the server answered.
		submit = function(values)
			return api.save { id = member.id, name = values.name, email = values.email, role = values.role }
		end,
		on_success = function(saved)
			props.on_saved(saved)
		end,
	}

	return ui.column {
		gap = 16,
		ui.text {
			if member.id == "" then "Add a person" else "Edit person",
			bold = true,
			text_size = "$font.size.lg",
		},
		field(form, "name", "Name"),
		field(form, "email", "Email"),
		ui.column {
			gap = 4,
			ui.text { "Role", text_size = "$font.size.sm", color = "$text.secondary" },
			ui.segmented { id = "role", field = form.fields.role, options = ROLES },
		},
		ui.presence(form.failure, function(message: string)
			return ui.text { id = "save-error", message, color = "$danger" }
		end),
		ui.row {
			gap = 8,
			justify = "end",
			ui.button { id = "cancel", "Cancel", on_click = props.on_cancel },
			ui.button {
				id = "save",
				function()
					return if form.pending:get() then "Saving…" else "Save"
				end,
				disabled = form.pending,
				on_click = form.submit,
			},
		},
	}
end)
