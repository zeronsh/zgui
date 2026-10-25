-- macos_accessibility_probe.py's form: a field wrong from the start, its
-- error shown under it (a submit tried as it mounts), and one wrong with
-- nothing showing why.
local form = ui.form {
	fields = { email = "ada", name = "" },
	validate = {
		email = function(email)
			return if email:find("@") then nil else "That is not an email address"
		end,
	},
}
local name = ui.state("name", "")
ui.on_mount(form.submit)
return ui.column {
	p = 16,
	gap = 8,
	ui.text { "Email" },
	ui.input { id = "email", label = "Email", field = form.fields.email },
	ui.presence(form.fields.email.error, function(problem: string)
		return ui.text { problem, color = "$danger" }
	end),
	ui.text { "Name" },
	ui.input { id = "name", label = "Name", value = name, error = "A name is required" },
}
