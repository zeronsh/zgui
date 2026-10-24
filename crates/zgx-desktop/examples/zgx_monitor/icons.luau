-- Line icons, white strokes on a 24-point grid (the terminal example's):
-- the svg node tints them.
local function icon(paths: string): string
	return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">'
		.. paths
		.. "</svg>"
end

return {
	search = icon('<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 4.5 4.5"/>'),
	-- Zeron's sidebar (icons.zeron.sh) without its panel, which
	-- controls.sidebar_glyph draws to animate it.
	sidebar_frame = icon('<rect x="3" y="4" width="18" height="16" rx="4"/>'),
	plus = icon('<path d="M12 5.5v13M5.5 12h13"/>'),
	close = icon('<path d="M7 7l10 10M17 7L7 17"/>'),
	split_right = icon('<rect x="3.5" y="5" width="17" height="14" rx="3"/><path d="M12 5v14"/>'),
	split_down = icon('<rect x="3.5" y="5" width="17" height="14" rx="3"/><path d="M3.5 12h17"/>'),
	expand = icon('<path d="M14 4.5h5.5V10M10 19.5H4.5V14M19.5 4.5 13.5 10.5M4.5 19.5l6-6"/>'),
	collapse = icon('<path d="M19.5 10H14V4.5M4.5 14H10v5.5M14 10l5.5-5.5M10 14l-5.5 5.5"/>'),
	pulse = icon('<path d="M3.5 12h4l2.5-6 4 12 2.5-6h4"/>'),
}
