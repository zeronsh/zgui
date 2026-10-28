-- Rex SurfaceStyle decoder defaults and ID-aware profile patching, recovered
-- from SurfaceStyle's schema producer and the profile resolver call sites.
-- Keep source nulls distinct from omitted fields throughout resolution.
local M = {}

local function copy(value: any): any
	if type(value) ~= "table" then
		return value
	end
	local out = {}
	for key, child in value do
		out[key] = copy(child)
	end
	return out
end

function M.merge(base: any, patch: any): any
	if patch == nil then
		return copy(base)
	end
	if patch == ui.null or type(patch) ~= "table" then
		return copy(patch)
	end
	if patch[1] ~= nil then
		local keyed = true
		for _, item in patch do
			if type(item) ~= "table" or type(item.id) ~= "string" then
				keyed = false
				break
			end
		end
		if not keyed then
			return copy(patch)
		end
		local out = if type(base) == "table" then copy(base) else {}
		for _, item in patch do
			local found = false
			for index, previous in out do
				if previous.id == item.id then
					out[index], found = M.merge(previous, item), true
					break
				end
			end
			if not found then
				table.insert(out, copy(item))
			end
		end
		return out
	end
	local out = if type(base) == "table" then copy(base) else {}
	for key, value in patch do
		out[key] = M.merge(out[key], value)
	end
	return out
end

local CLEAR = {
	color = { kind = "fixed", light = { red = 0, green = 0, blue = 0, alpha = 0, colorSpace = "extendedSRGB" } },
}
local function geometry(clips: boolean): any
	return {
		insets = { top = 0, trailing = 0, bottom = 0, leading = 0 },
		radius = { mode = "concentric", value = 0 },
		opacity = 1,
		clipsToSurface = clips,
	}
end
local function layer(enabled: boolean, clips: boolean): any
	return { isEnabled = enabled, blendMode = "normal", color = copy(CLEAR), geometry = geometry(clips) }
end
local function shadow(): any
	local out = layer(true, false)
	out.blurRadius, out.offsetX, out.offsetY, out.knockout = 0, 0, 0, true
	return out
end
local function stroke(): any
	local out = layer(true, false)
	out.width, out.alignment, out.level = 0.5, "inside", "aboveContent"
	return out
end

function M.normalize(raw: any): any
	local inner = shadow()
	inner.isEnabled = false
	local out = M.merge({
		shape = { cornerRadius = 0, cornerCurve = "continuous" },
		glass = {
			isEnabled = false,
			style = "regular",
			windowMaterial = "underWindowBackground",
			usesTint = false,
			tint = copy(CLEAR),
			geometry = geometry(true),
		},
		backdrop = layer(false, true),
		fill = layer(false, true),
		fill2 = layer(false, true),
		strokes = {},
		shadows = {},
		innerShadow = inner,
		clipsContent = true,
		contentInsets = { top = 0, trailing = 0, bottom = 0, leading = 0 },
		contentOpacity = 1,
		alpha = 1,
	}, raw or {})
	if type(out.strokes) == "table" then
		for index, item in out.strokes do
			out.strokes[index] = M.merge(stroke(), item)
		end
	end
	if type(out.shadows) == "table" then
		for index, item in out.shadows do
			out.shadows[index] = M.merge(shadow(), item)
		end
	end
	return out
end

function M.pane(branch: any, no_glass: boolean, focused: boolean): any
	local base, patch = branch.pane, branch.activePane
	if no_glass and type(branch.panesWithoutGlass) == "table" then
		-- This is a separately decoded complete profile, not a base-pane patch.
		base, patch = branch.panesWithoutGlass.pane, branch.panesWithoutGlass.activePane
	end
	local out = M.normalize(base)
	if focused then
		out = M.normalize(M.merge(out, patch or {}))
	end
	if no_glass then
		out.glass = M.merge(if type(out.glass) == "table" then out.glass else {}, { isEnabled = false })
	end
	return out
end

function M.selection(branch: any, hover: boolean): any
	local out = M.normalize(branch.tabSelection)
	return if hover then M.normalize(M.merge(out, branch.tabHover or {})) else out
end

return M
