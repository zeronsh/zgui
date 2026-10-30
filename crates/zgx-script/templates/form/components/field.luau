-- A labelled input bound to one of a form's fields, with its error below
-- once the person has left it.
local tokens = require("@app/theme")

export type Props = { form: Form, name: string, label: string | Message }

return ui.component(function(props: Props)
	local field = props.form.fields[props.name]
	return ui.column {
		gap = "$space.1",
		ui.text { props.label, color = tokens.muted, text_size = "$font.size.sm" },
		ui.input { id = props.name, label = props.label, field = field, w_full = true },
		ui.presence(field.error, function(problem: string)
			return ui.text { id = `{props.name}-error`, problem, color = "$danger", text_size = "$font.size.sm" }
		end),
	}
end)
