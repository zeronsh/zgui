-- The app's own design tokens, each a default written in zgx's standard
-- tokens, so the app takes any theme (light, dark, the user's) and a theme
-- may set them by name. Styles name them through `ui.tokens.app`: an editor
-- completes them and a typo is a type error (`zgx types` writes their
-- types to .zgx/types.d.luau; `zgx check` does too).
ui.theme.tokens {
	app = {
		card = "{surface.raised}",
		muted = "{text.secondary}",
		highlight = "alpha({accent}, 0.15)",
	},
}

return ui.tokens.app
