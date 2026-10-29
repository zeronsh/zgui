-- Team: a directory loaded from an API, showing that it is loading and
-- what went wrong (with a retry); a person added or edited in a modal
-- form checked as it is typed; a confirmation before one is removed.
local api = require("./api")
local MemberForm = require("@components/member_form")

type Member = api.Member

-- The team, loaded as the app starts and on Reload: whether it is loading
-- and what went wrong are states beside it. Saves and removals write it.
local team = ui.resource(api.list, { initial = {} :: { Member } })
local members = team.data

-- The person being edited (a blank one to add), and the one to remove.
local editing = ui.state("editing", nil :: Member?)
local removing = ui.state("removing", nil :: Member?)
local form_open = ui.state("form_open", false)
local confirm_open = ui.state("confirm_open", false)
local removal_error = ui.state("removal_error", nil :: string?)

local function edit(member: Member?)
	editing:set(member or { id = "", name = "", email = "", role = "" })
	form_open:set(true)
end

local function saved(member: Member)
	local at = members:index_of("id", member.id)
	if at then
		members:set_key(at, member)
	else
		members:push(member)
	end
	form_open:set(false)
end

local function remove()
	local member = removing:peek()
	if not member then
		return
	end
	removal_error:set(nil)
	ui.task(function()
		local _, err = ui.try(api.remove, member.id)
		if not err then
			local at = members:index_of("id", member.id)
			if at then
				members:remove(at)
			end
			confirm_open:set(false)
		else
			removal_error:set(err.message)
		end
	end)
end

local function row(member: Member)
	return ui.row {
		id = `member-{member.id}`,
		gap = 12,
		px = 16,
		py = 10,
		items = "center",
		ui.column {
			grow = 1,
			min_w = 0,
			ui.text { member.name, bold = true },
			ui.text { `{member.email} · {member.role}`, color = "$text.secondary", text_size = "$font.size.sm" },
		},
		ui.button {
			id = `edit-{member.id}`,
			"Edit",
			on_click = function()
				edit(member)
			end,
		},
		ui.button {
			id = `remove-{member.id}`,
			"Remove",
			on_click = function()
				removing:set(member)
				removal_error:set(nil)
				confirm_open:set(true)
			end,
		},
	}
end

local function body()
	return ui.switch(team.status, {
		loading = ui.text { id = "loading", "Loading the team…", color = "$text.secondary", p = 16 },
		failed = ui.column {
			gap = 8,
			p = 16,
			ui.text {
				id = "load-error",
				function()
					return `Couldn't load the team: {team.error:get()}`
				end,
				color = "$danger",
			},
			ui.button { id = "retry", "Try again", on_click = team.reload },
		},
		ready = ui.column {
			ui.show(function()
				return members:len() == 0
			end, ui.text { id = "empty", "No one yet.", color = "$text.secondary", p = 16 }),
			ui.each(members, row, {
				key = function(member: Member)
					return member.id
				end,
			}),
		},
	})
end

return ui.column {
	id = "team",
	w_full = true,
	h_full = true,
	ui.row {
		px = 16,
		py = 12,
		gap = 8,
		items = "center",
		bg = "$surface.raised",
		ui.text { "Team", bold = true, text_size = "$font.size.lg", grow = 1 },
		ui.button { id = "reload", "Reload", on_click = team.reload },
		ui.button {
			id = "add",
			"Add person",
			on_click = function()
				edit(nil)
			end,
		},
	},
	ui.scroll { grow = 1, body() },
	ui.modal {
		open = form_open,
		w = 420,
		-- Built each time it opens: a new form, its fields starting from the
		-- person edited.
		build = function()
			local member = editing:peek() :: Member
			local taken = {}
			for _, other in members:peek() do
				if other.id ~= member.id then
					table.insert(taken, other.email:lower())
				end
			end
			return MemberForm {
				member = member,
				taken = taken,
				on_saved = saved,
				on_cancel = function()
					form_open:set(false)
				end,
			}
		end,
	},
	ui.modal {
		open = confirm_open,
		dismiss_on_backdrop = false,
		gap = 16,
		ui.text {
			function()
				local member = removing:get()
				return `Remove {if member then member.name else "them"} from the team?`
			end,
			bold = true,
		},
		ui.presence(removal_error, function(message: string)
			return ui.text { id = "remove-error", message, color = "$danger" }
		end),
		ui.row {
			gap = 8,
			justify = "end",
			ui.button {
				id = "keep",
				"Keep",
				on_click = function()
					confirm_open:set(false)
				end,
			},
			ui.button { id = "confirm-remove", "Remove", on_click = remove },
		},
	},
}
