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

-- A labelled input, its error under it once it has been left.
local function field(id: string, label: string, value: State<string>, problem: () -> string?, shown: State<boolean>)
	local function visible(): string?
		return if shown:get() then problem() else nil
	end
	return ui.column {
		gap = 4,
		ui.text { label, text_size = "$font.size.sm", color = "$text.secondary" },
		ui.input {
			id = id,
			label = label,
			value = value,
			w_full = true,
			border = 1,
			border_color = function()
				return if visible() then "$danger" else "$border"
			end,
			on_blur = function()
				shown:set(true)
			end,
		},
		-- Shown while there is something wrong.
		ui.presence(visible, function(problem: string)
			return ui.text { id = `{id}-error`, problem, color = "$danger", text_size = "$font.size.sm" }
		end),
	}
end

return ui.component(function(props: Props)
	local name = ui.use_state(props.member.name)
	local email = ui.use_state(props.member.email)
	local role = ui.use_state(if props.member.role ~= "" then props.member.role else ROLES[1])
	local shown_name = ui.use_state(false)
	local shown_email = ui.use_state(false)
	local saving = ui.use_state(false)
	local failure = ui.use_state(nil :: string?)

	local function name_problem()
		return check_name(name:get())
	end
	local function email_problem()
		return check_email(email:get(), props.taken)
	end

	local function submit()
		if saving:peek() then
			return
		end
		shown_name:set(true)
		shown_email:set(true)
		if check_name(name:peek()) or check_email(email:peek(), props.taken) then
			return
		end
		saving:set(true)
		failure:set(nil)
		ui.task(function()
			local ok, result = pcall(api.save, {
				id = props.member.id,
				name = name:peek(),
				email = email:peek(),
				role = role:peek(),
			})
			saving:set(false)
			if ok then
				props.on_saved(result)
			else
				failure:set(tostring(result))
			end
		end)
	end

	return ui.column {
		gap = 16,
		on_key = function(e: KeyEvent)
			if e.key == "Enter" then
				submit()
				return true
			end
			return false
		end,
		ui.text {
			if props.member.id == "" then "Add a person" else "Edit person",
			bold = true,
			text_size = "$font.size.lg",
		},
		field("name", "Name", name, name_problem, shown_name),
		field("email", "Email", email, email_problem, shown_email),
		ui.column {
			gap = 4,
			ui.text { "Role", text_size = "$font.size.sm", color = "$text.secondary" },
			ui.segmented { id = "role", value = role, options = ROLES },
		},
		ui.presence(failure, function(message: string)
			return ui.text { id = "save-error", message, color = "$danger" }
		end),
		ui.row {
			gap = 8,
			justify = "end",
			ui.button { id = "cancel", "Cancel", on_click = props.on_cancel },
			ui.button {
				id = "save",
				function()
					return if saving:get() then "Saving…" else "Save"
				end,
				disabled = saving,
				on_click = submit,
			},
		},
	}
end)
