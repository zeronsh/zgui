--!strict
-- A 3D scene in Luau: shapes on a floor under a sun, a sky and a warm bulb.
-- Drag to turn the camera, scroll or pinch to move in and out. The buttons
-- change state the scene reads: the ring turns on a spring (Rust steps it,
-- no script runs per frame), the sphere turns to metal. Left alone, the
-- scene draws nothing.
local spin = ui.state("spin", 0)
local metal = ui.state("metal", true)
local turning = ui.state("turning", true)

return ui.column {
	w = "100%",
	h = "100%",
	p = 16,
	gap = 12,
	bg = "#0b0e13",
	color = "#e0eaf5",
	ui.row {
		gap = 8,
		items = "center",
		ui.text { "zgx 3D", size = 18 },
		ui.button {
			px = 12,
			py = 6,
			rounded = 6,
			bg = "#243447",
			on_click = function()
				spin:set(spin:get() + 120)
			end,
			"Turn the ring",
		},
		ui.button {
			px = 12,
			py = 6,
			rounded = 6,
			bg = "#243447",
			on_click = function()
				metal:set(not metal:get())
			end,
			function()
				return if metal:get() then "Gold: metal" else "Gold: paint"
			end,
		},
		ui.button {
			px = 12,
			py = 6,
			rounded = 6,
			bg = "#243447",
			on_click = function()
				turning:set(not turning:get())
			end,
			function()
				return if turning:get() then "Stop turning" else "Turn on its own"
			end,
		},
	},
	ui.scene3d {
		label = "Shapes on a floor",
		w = "100%",
		grow = 1,
		rounded = 14,
		background = "#10141b",
		camera = { position = { 2.2, 1.8, 4.2 }, target = { 0, 0, 0 }, fov = 40 },
		orbit = {
			auto_rotate = function()
				return if turning:get() then 12 else 0
			end,
		},
		ui.light { kind = "directional", direction = { -0.5, -1, -0.4 }, color = "#fff8ec", intensity = 2.5 },
		ui.light { kind = "hemisphere", color = "#5a7399", ground = "#1f1a14", intensity = 0.6 },
		ui.light { kind = "point", position = { 1.5, 1.2, 1.5 }, color = "#ff9a4d", intensity = 6, range = 6 },
		ui.mesh {
			geometry = "plane",
			size = { 8, 1, 8 },
			position = { 0, -0.5, 0 },
			material = { color = "#2a313c", roughness = 0.9 },
		},
		ui.mesh {
			geometry = "sphere",
			radius = 0.5,
			segments = 48,
			position = { -1.3, 0, 0 },
			material = {
				color = "#d9a441",
				metallic = function()
					return if metal:get() then 1 else 0
				end,
				roughness = 0.3,
			},
		},
		ui.mesh {
			geometry = "box",
			size = { 0.8, 0.8, 0.8 },
			position = { 0, -0.1, 0 },
			material = { color = "#4a90e2" },
		},
		ui.mesh {
			geometry = "cone",
			radius = 0.45,
			height = 1,
			position = { 1.3, 0, 0 },
			material = { color = "#e5533d", roughness = 0.35 },
		},
		ui.group {
			position = { 0, 1, -0.4 },
			rotation = { 30, ui.spring(spin), 0 },
			ui.mesh {
				geometry = "torus",
				radius = 0.5,
				tube = 0.12,
				segments = 64,
				material = { color = "#9ad1c4", metallic = 0.2, roughness = 0.2 },
			},
		},
	},
}
