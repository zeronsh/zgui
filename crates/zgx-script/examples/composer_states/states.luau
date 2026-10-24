-- Generated from the Ask motion lab's composer design data (window.askDesign.composer().data):
-- every state's composer and the popovers drawn above it, as the lab's design nodes.
-- Shader fills are the lab's dither uniforms (drawn by app.luau's port of its shader); photos are downscaled copies.
return {
	icons = {
		["arrow-big-up"] = "<path d=\"M9 18v-6H5l7-7 7 7h-4v6H9z\"></path>",
		["arrow-down"] = "<path d=\"M12 5v14\"></path>\n  <path d=\"m19 12-7 7-7-7\"></path>",
		["arrow-right-to-line"] = "<path d=\"M17 12H3\"></path>\n  <path d=\"m11 18 6-6-6-6\"></path>\n  <path d=\"M21 5v14\"></path>",
		["arrow-up"] = "<path d=\"m5 12 7-7 7 7\"></path>\n  <path d=\"M12 19V5\"></path>",
		["at-sign"] = "<circle cx=\"12\" cy=\"12\" r=\"4\"></circle>\n  <path d=\"M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8\"></path>",
		["bookmark"] = "<path d=\"m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z\"></path>",
		["briefcase-business"] = "<path d=\"M12 12h.01\"></path>\n  <path d=\"M16 6V4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2\"></path>\n  <path d=\"M22 13a18.15 18.15 0 0 1-20 0\"></path>\n  <rect width=\"20\" height=\"14\" x=\"2\" y=\"6\" rx=\"2\"></rect>",
		["calendar-range"] = "<rect width=\"18\" height=\"18\" x=\"3\" y=\"4\" rx=\"2\"></rect>\n  <path d=\"M16 2v4\"></path>\n  <path d=\"M3 10h18\"></path>\n  <path d=\"M8 2v4\"></path>\n  <path d=\"M17 14h-6\"></path>\n  <path d=\"M13 18H7\"></path>\n  <path d=\"M7 14h.01\"></path>\n  <path d=\"M17 18h.01\"></path>",
		["camera"] = "<path d=\"M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z\"></path>\n  <circle cx=\"12\" cy=\"13\" r=\"3\"></circle>",
		["check"] = "<path d=\"M20 6 9 17l-5-5\"></path>",
		["chevron-down"] = "<path d=\"m6 9 6 6 6-6\"></path>",
		["chevron-left"] = "<path d=\"m15 18-6-6 6-6\"></path>",
		["chevron-right"] = "<path d=\"m9 18 6-6-6-6\"></path>",
		["chevron-up"] = "<path d=\"m18 15-6-6-6 6\"></path>",
		["chevrons-up-down"] = "<path d=\"m7 15 5 5 5-5\"></path>\n  <path d=\"m7 9 5-5 5 5\"></path>",
		["circle-check"] = "<circle cx=\"12\" cy=\"12\" r=\"10\"></circle>\n  <path d=\"m9 12 2 2 4-4\"></path>",
		["clipboard-type"] = "<rect width=\"8\" height=\"4\" x=\"8\" y=\"2\" rx=\"1\" ry=\"1\"></rect>\n  <path d=\"M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2\"></path>\n  <path d=\"M9 12v-1h6v1\"></path>\n  <path d=\"M11 17h2\"></path>\n  <path d=\"M12 11v6\"></path>",
		["command"] = "<path d=\"M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z\"></path>",
		["corner-down-left"] = "<path d=\"M20 4v7a4 4 0 0 1-4 4H4\"></path><path d=\"m9 10-5 5 5 5\"></path>",
		["corner-up-left"] = "<polyline points=\"9 14 4 9 9 4\"></polyline>\n  <path d=\"M20 20v-7a4 4 0 0 0-4-4H4\"></path>",
		["delete"] = "<path d=\"M10 5a2 2 0 0 0-1.344.519l-6.328 5.74a1 1 0 0 0 0 1.481l6.328 5.741A2 2 0 0 0 10 19h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2z\"></path>\n  <path d=\"m12 9 6 6\"></path>\n  <path d=\"m18 9-6 6\"></path>",
		["external-link"] = "<path d=\"M15 3h6v6\"></path>\n  <path d=\"M10 14 21 3\"></path>\n  <path d=\"M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6\"></path>",
		["eye"] = "<path d=\"M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0\"></path>\n  <circle cx=\"12\" cy=\"12\" r=\"3\"></circle>",
		["file-code"] = "<path d=\"M10 12.5 8 15l2 2.5\"></path>\n  <path d=\"m14 12.5 2 2.5-2 2.5\"></path>\n  <path d=\"M14 2v4a2 2 0 0 0 2 2h4\"></path>\n  <path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7z\"></path>",
		["file-spreadsheet"] = "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\"></path>\n  <path d=\"M14 2v4a2 2 0 0 0 2 2h4\"></path>\n  <path d=\"M8 13h2\"></path>\n  <path d=\"M14 13h2\"></path>\n  <path d=\"M8 17h2\"></path>\n  <path d=\"M14 17h2\"></path>",
		["file-text"] = "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\"></path>\n  <path d=\"M14 2v4a2 2 0 0 0 2 2h4\"></path>\n  <path d=\"M10 9H8\"></path>\n  <path d=\"M16 13H8\"></path>\n  <path d=\"M16 17H8\"></path>",
		["file-type"] = "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\"></path>\n  <path d=\"M14 2v4a2 2 0 0 0 2 2h4\"></path>\n  <path d=\"M9 13v-1h6v1\"></path>\n  <path d=\"M12 12v6\"></path>\n  <path d=\"M11 18h2\"></path>",
		["files"] = "<path d=\"M20 7h-3a2 2 0 0 1-2-2V2\"></path>\n  <path d=\"M9 18a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h7l4 4v10a2 2 0 0 1-2 2Z\"></path>\n  <path d=\"M3 7.6v12.8A1.6 1.6 0 0 0 4.6 22h9.8\"></path>",
		["history"] = "<path d=\"M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8\"></path>\n  <path d=\"M3 3v5h5\"></path>\n  <path d=\"M12 7v5l4 2\"></path>",
		["info"] = "<circle cx=\"12\" cy=\"12\" r=\"10\"></circle>\n  <path d=\"M12 16v-4\"></path>\n  <path d=\"M12 8h.01\"></path>",
		["link"] = "<path d=\"M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71\"></path>\n  <path d=\"M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71\"></path>",
		["list-ordered"] = "<path d=\"M10 12h11\"></path>\n  <path d=\"M10 18h11\"></path>\n  <path d=\"M10 6h11\"></path>\n  <path d=\"M4 10h2\"></path>\n  <path d=\"M4 6h1v4\"></path>\n  <path d=\"M6 18H4c0-1 2-2 2-3s-1-1.5-2-1\"></path>",
		["list-plus"] = "<path d=\"M11 12H3\"></path>\n  <path d=\"M16 6H3\"></path>\n  <path d=\"M16 18H3\"></path>\n  <path d=\"M18 9v6\"></path>\n  <path d=\"M21 12h-6\"></path>",
		["maximize-2"] = "<path d=\"M15 3h6v6\"></path>\n  <path d=\"m21 3-7 7\"></path>\n  <path d=\"m3 21 7-7\"></path>\n  <path d=\"M9 21H3v-6\"></path>",
		["message-square-reply"] = "<path d=\"M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z\"></path>\n  <path d=\"m10 7-3 3 3 3\"></path>\n  <path d=\"M17 13v-1a2 2 0 0 0-2-2H7\"></path>",
		["mic"] = "<path d=\"M12 19v3\"></path>\n  <path d=\"M19 10v2a7 7 0 0 1-14 0v-2\"></path>\n  <rect x=\"9\" y=\"2\" width=\"6\" height=\"13\" rx=\"3\"></rect>",
		["mouse-pointer-click"] = "<path d=\"M14 4.1 12 6\"></path>\n  <path d=\"m5.1 8-2.9-.8\"></path>\n  <path d=\"m6 12-1.9 2\"></path>\n  <path d=\"M7.2 2.2 8 5.1\"></path>\n  <path d=\"M9.037 9.69a.498.498 0 0 1 .653-.653l11 4.5a.5.5 0 0 1-.074.949l-4.349 1.041a1 1 0 0 0-.74.739l-1.04 4.35a.5.5 0 0 1-.95.074z\"></path>",
		["paperclip"] = "<path d=\"M13.234 20.252 21 12.3\"></path>\n  <path d=\"m16 6-8.414 8.586a2 2 0 0 0 0 2.828 2 2 0 0 0 2.828 0l8.414-8.586a4 4 0 0 0 0-5.656 4 4 0 0 0-5.656 0l-8.415 8.585a6 6 0 1 0 8.486 8.486\"></path>",
		["pen-line"] = "<path d=\"M12 20h9\"></path>\n  <path d=\"M16.376 3.622a1 1 0 0 1 3.002 3.002L7.368 18.635a2 2 0 0 1-.855.506l-2.872.838a.5.5 0 0 1-.62-.62l.838-2.872a2 2 0 0 1 .506-.854z\"></path>",
		["pencil"] = "<path d=\"M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z\"></path>\n  <path d=\"m15 5 4 4\"></path>",
		["plus"] = "<path d=\"M5 12h14\"></path>\n  <path d=\"M12 5v14\"></path>",
		["replace"] = "<path d=\"M14 4a2 2 0 0 1 2-2\"></path>\n  <path d=\"M16 10a2 2 0 0 1-2-2\"></path>\n  <path d=\"M20 2a2 2 0 0 1 2 2\"></path>\n  <path d=\"M22 8a2 2 0 0 1-2 2\"></path>\n  <path d=\"m3 7 3 3 3-3\"></path>\n  <path d=\"M6 10V5a3 3 0 0 1 3-3h1\"></path>\n  <rect x=\"2\" y=\"14\" width=\"8\" height=\"8\" rx=\"2\"></rect>",
		["scan"] = "<path d=\"M3 7V5a2 2 0 0 1 2-2h2\"></path>\n  <path d=\"M17 3h2a2 2 0 0 1 2 2v2\"></path>\n  <path d=\"M21 17v2a2 2 0 0 1-2 2h-2\"></path>\n  <path d=\"M7 21H5a2 2 0 0 1-2-2v-2\"></path>",
		["settings-2"] = "<path d=\"M14 17H5\"></path>\n  <path d=\"M19 7h-9\"></path>\n  <circle cx=\"17\" cy=\"17\" r=\"3\"></circle>\n  <circle cx=\"7\" cy=\"7\" r=\"3\"></circle>",
		["shopping-basket"] = "<path d=\"m15 11-1 9\"></path>\n  <path d=\"m19 11-4-7\"></path>\n  <path d=\"M2 11h20\"></path>\n  <path d=\"m3.5 11 1.6 7.4a2 2 0 0 0 2 1.6h9.8a2 2 0 0 0 2-1.6l1.7-7.4\"></path>\n  <path d=\"M4.5 15.5h15\"></path>\n  <path d=\"m5 11 4-7\"></path>\n  <path d=\"m9 11 1 9\"></path>",
		["text-cursor-input"] = "<path d=\"M5 4h1a3 3 0 0 1 3 3 3 3 0 0 1 3-3h1\"></path>\n  <path d=\"M13 20h-1a3 3 0 0 1-3-3 3 3 0 0 1-3 3H5\"></path>\n  <path d=\"M5 16H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h1\"></path>\n  <path d=\"M13 8h7a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-7\"></path>\n  <path d=\"M9 7v10\"></path>",
		["text-quote"] = "<path d=\"M17 6H3\"></path>\n  <path d=\"M21 12H8\"></path>\n  <path d=\"M21 18H8\"></path>\n  <path d=\"M3 12v6\"></path>",
		["triangle-alert"] = "<path d=\"m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3\"></path>\n  <path d=\"M12 9v4\"></path>\n  <path d=\"M12 17h.01\"></path>",
		["type"] = "<polyline points=\"4 7 4 4 20 4 20 7\"></polyline>\n  <line x1=\"9\" x2=\"15\" y1=\"20\" y2=\"20\"></line>\n  <line x1=\"12\" x2=\"12\" y1=\"4\" y2=\"20\"></line>",
		["x"] = "<path d=\"M18 6 6 18\"></path>\n  <path d=\"m6 6 12 12\"></path>",
	},
	states = {
		{ name = "Composer · master", group = "Composer states · row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 76, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 44, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Empty", group = "Composer states · row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 76, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 44, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Ask anything, @ for tabs, / for commands", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Typing", group = "Composer states · row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 76, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 44, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Summarize this page", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Attachments", group = "Composer states · row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 126, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 94, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = "fill_container", gap = 8, padding = { 2, 2, 0, 2 }, bw = 384, bh = 42, children = {
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 11.92, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "PDF", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 176, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-text", bw = 14, bh = 14, fill = "#E8A898" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 120, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 93, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 120, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
										{ type = "text", name = "Attachment detail", content = "6 pages · 2.4 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 96, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "Image", width = 40, height = 40, cornerRadius = 10, bw = 40, bh = 40, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Attachments · files", group = "Composer states · row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 126, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 94, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = "fill_container", gap = 8, padding = { 2, 2, 0, 2 }, bw = 384, bh = 42, children = {
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 11.92, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "PDF", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 176, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-text", bw = 14, bh = 14, fill = "#E8A898" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 120, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 93, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 120, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
										{ type = "text", name = "Attachment detail", content = "6 pages · 2.4 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 96, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "Image", width = 40, height = 40, cornerRadius = 10, bw = 40, bh = 40, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Uploading & error", group = "Composer states · row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 126, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 94, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = "fill_container", gap = 8, padding = { 2, 2, 0, 2 }, bw = 384, bh = 42, children = {
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 87.4, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "PDF", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 176, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-text", bw = 14, bh = 14, fill = "#E8A898" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 120, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 93, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 120, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
										{ type = "text", name = "Attachment detail", content = "6 pages · 2.4 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 96, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "Image", width = 40, height = 40, cornerRadius = 10, bw = 40, bh = 40, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Photo stack", group = "Composer states · overflow row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 126, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 94, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, gap = 8, padding = { 2, 2, 0, 2 }, alignItems = "center", clip = true, bw = 384, bh = 42, children = {
							{ type = "frame", name = "Photos", width = 72, height = 40, layout = "none", bw = 72, bh = 40, children = {
								{ type = "frame", name = "Photo 3", width = 32, height = 32, cornerRadius = 8, x = 36, y = 6, rotation = -9, clip = true, bw = 36.61, bh = 36.61, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1638890718904-3619e021b396.jpg", fit = "cover", children = {
									{ type = "rectangle", name = "Depth shade", width = 32, height = 32, x = 0, y = 0, layoutPosition = "absolute", bw = 32, bh = 32, fill = "#0A0A0959" },
								} },
								{ type = "frame", name = "Photo 2", width = 36, height = 36, cornerRadius = 9, x = 20, y = 3, rotation = -1, clip = true, bw = 36.62, bh = 36.62, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1754336241390-9a945d0d4daf.jpg", fit = "cover", children = {
									{ type = "rectangle", name = "Depth shade", width = 36, height = 36, x = 0, y = 0, layoutPosition = "absolute", bw = 36, bh = 36, fill = "#0A0A0933" },
								} },
								{ type = "frame", name = "Photo 1", width = 40, height = 40, layout = "none", cornerRadius = 10, x = 2, y = 5, rotation = 8, clip = true, bw = 45.18, bh = 45.18, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Count", height = 15, padding = { 0, 4 }, cornerRadius = 8, alignItems = "center", x = -3, y = 30, bw = 14, bh = 15, stroke = "#2E2D29", strokeWidth = 1, fill = "#0A0A09", children = {
									{ type = "text", name = "Count label", content = "6", fontFamily = "Geist Mono", fontSize = 9, fontWeight = "500", bw = 6, bh = 12, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 56.58, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "Text & code", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 108, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-code", bw = 14, bh = 14, fill = "#A3A199" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 52, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "notes.md", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 52, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 42, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "MD", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 13, bh = 12, fill = "#A3A199" },
										{ type = "text", name = "Attachment detail", content = "2 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 24, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Photo stack · Expanded", group = "Composer states · overflow row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 130, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 98, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 98, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, gap = 8, padding = { 2, 2, 0, 2 }, alignItems = "center", clip = true, bw = 384, bh = 46, children = {
							{ type = "frame", name = "Photos · expanded", width = 215, height = 44, layout = "none", bw = 215, bh = 44, children = {
								{ type = "frame", name = "Photo 1", width = 40, height = 40, cornerRadius = 10, x = 3, y = 3, rotation = 5, clip = true, bw = 43.33, bh = 43.33, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Photo 2", width = 40, height = 40, cornerRadius = 10, x = 34, y = 2, rotation = -3, clip = true, bw = 42.04, bh = 42.04, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1754336241390-9a945d0d4daf.jpg", fit = "cover" },
								{ type = "frame", name = "Photo 3", width = 40, height = 40, cornerRadius = 10, x = 71, y = -2, rotation = 2, clip = true, bw = 41.37, bh = 41.37, stroke = "#3A3934", strokeWidth = 1.5, img = "assets/photo-1638890718904-3619e021b396.jpg", fit = "cover" },
								{ type = "frame", name = "Photo 4", width = 40, height = 40, cornerRadius = 10, x = 102, y = 2, rotation = -4, clip = true, bw = 42.69, bh = 42.69, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1638191376884-f371a22c719f.jpg", fit = "cover" },
								{ type = "frame", name = "Photo 5", width = 40, height = 40, cornerRadius = 10, x = 139, y = 3, rotation = 3, clip = true, bw = 42.04, bh = 42.04, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1599855129764-f4cc28295202.jpg", fit = "cover" },
								{ type = "frame", name = "Photo 6", width = 40, height = 40, cornerRadius = 10, x = 172, y = 1, rotation = -2, clip = true, bw = 43.33, bh = 43.33, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1662947368791-8630979e964b.jpg", fit = "cover" },
							} },
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 54.93, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
							{ type = "frame", name = "Text & code", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 108, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-code", bw = 14, bh = 14, fill = "#A3A199" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 52, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "notes.md", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 52, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 42, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "MD", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 13, bh = 12, fill = "#A3A199" },
										{ type = "text", name = "Attachment detail", content = "2 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 24, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Compact chips", group = "Composer states · overflow row 1",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 144, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 112, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 112, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, layout = "vertical", gap = 6, padding = { 2, 2, 0, 6 }, bw = 384, bh = 60, children = {
							{ type = "frame", name = "Chip row 1", gap = 6, alignItems = "center", bw = 386, bh = 26, children = {
								{ type = "frame", name = "Photos", width = 48, height = 26, layout = "none", bw = 48, bh = 26, children = {
									{ type = "frame", name = "Photo 3", width = 21, height = 21, cornerRadius = 6, x = 24, y = 4, rotation = -9, clip = true, bw = 24.03, bh = 24.03, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1536664607464-9a9419341c4d.jpg", fit = "cover", children = {
										{ type = "rectangle", name = "Depth shade", width = 21, height = 21, x = 0, y = 0, layoutPosition = "absolute", bw = 21, bh = 21, fill = "#0A0A0959" },
									} },
									{ type = "frame", name = "Photo 2", width = 24, height = 24, cornerRadius = 6, x = 13, y = 2, rotation = -1, clip = true, bw = 24.42, bh = 24.42, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1788009145784-429a60a09b47.jpg", fit = "cover", children = {
										{ type = "rectangle", name = "Depth shade", width = 24, height = 24, x = 0, y = 0, layoutPosition = "absolute", bw = 24, bh = 24, fill = "#0A0A0933" },
									} },
									{ type = "frame", name = "Photo 1", width = 26, height = 26, cornerRadius = 7, x = 1.5, y = 3.5, rotation = 8, bw = 29.37, bh = 29.37, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
									{ type = "frame", name = "Count", height = 13, padding = { 0, 3.5 }, cornerRadius = 7, alignItems = "center", x = -3, y = 16, bw = 13, bh = 13, stroke = "#2E2D29", strokeWidth = 1, fill = "#0A0A09", children = {
										{ type = "text", name = "Count label", content = "6", fontFamily = "Geist Mono", fontSize = 8.5, fontWeight = "500", bw = 6, bh = 11, fill = "#EDEBE5" },
									} },
								} },
								{ type = "frame", name = "Link", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 84, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 56.71, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/opentable.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 47, bh = 15, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 126, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 123.56, soft = 0.18, ink = "#4285F559", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/google.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Flights to Lisbon", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 89, bh = 15, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 126, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 211.4, soft = 0.18, ink = "#4285F559", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Tram 28", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 89, bh = 15, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Chip row 2", gap = 6, alignItems = "center", bw = 349, bh = 26, children = {
								{ type = "frame", name = "Document", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 110, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#2E3A3D", strokeWidth = 1, fill = "#151A1B", children = {
										{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-type", bw = 11, bh = 11, fill = "#A9BABF" },
									} },
									{ type = "text", name = "Attachment name", content = "Itinerary.docx", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 73, bh = 15, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "PDF", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 126, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
										{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
									} },
									{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 89, bh = 15, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "Link", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 84, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 87.3, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/theverge.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Time Out Lisbon", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 47, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · File stack · Expanded", group = "Composer states · overflow row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 126, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 94, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, gap = 8, padding = { 2, 2, 0, 2 }, alignItems = "center", bw = 384, bh = 42, children = {
							{ type = "frame", name = "Files", width = 150, height = 40, layout = "none", bw = 150, bh = 40, children = {
								{ type = "frame", name = "Layer 3", width = 134, height = 26, cornerRadius = 8, x = 16, y = 7, bw = 134, bh = 26, stroke = "#22211E", strokeWidth = 1, fill = "#121211" },
								{ type = "frame", name = "Layer 2", width = 134, height = 33, cornerRadius = 9, x = 8, y = 3.5, bw = 134, bh = 33, stroke = "#262521", strokeWidth = 1, fill = "#161615" },
								{ type = "frame", name = "Top file", width = 134, height = 40, gap = 9, padding = { 0, 10, 0, 5 }, cornerRadius = 10, alignItems = "center", x = 0, y = 0, bw = 134, bh = 40, stroke = "#3A3934", strokeWidth = 1, fill = "#201F1D", children = {
									{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
										{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "files", bw = 14, bh = 14, fill = "#C9C6BE" },
									} },
									{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 67, bh = 30, children = {
										{ type = "text", name = "Attachment name", content = "4 files", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 35, bh = 16, fill = "#EDEBE5" },
										{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 67, bh = 12, children = {
											{ type = "text", name = "Type badge PDF", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
											{ type = "text", name = "Type badge XLSX", content = "XLSX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#C3CEB2" },
											{ type = "text", name = "Type badge +1", content = "+2", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 13, bh = 12, fill = "#6E6C65" },
										} },
									} },
								} },
							} },
							{ type = "frame", name = "Photos", width = 72, height = 40, layout = "none", bw = 72, bh = 40, children = {
								{ type = "frame", name = "Photo 3", width = 32, height = 32, cornerRadius = 8, x = 36, y = 6, rotation = -9, clip = true, bw = 36.61, bh = 36.61, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1638890718904-3619e021b396.jpg", fit = "cover", children = {
									{ type = "rectangle", name = "Depth shade", width = 32, height = 32, x = 0, y = 0, layoutPosition = "absolute", bw = 32, bh = 32, fill = "#0A0A0959" },
								} },
								{ type = "frame", name = "Photo 2", width = 36, height = 36, cornerRadius = 9, x = 20, y = 3, rotation = -1, clip = true, bw = 36.62, bh = 36.62, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1754336241390-9a945d0d4daf.jpg", fit = "cover", children = {
									{ type = "rectangle", name = "Depth shade", width = 36, height = 36, x = 0, y = 0, layoutPosition = "absolute", bw = 36, bh = 36, fill = "#0A0A0933" },
								} },
								{ type = "frame", name = "Photo 1", width = 40, height = 40, layout = "none", cornerRadius = 10, x = 2, y = 5, rotation = 8, clip = true, bw = 45.18, bh = 45.18, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Count", height = 15, padding = { 0, 4 }, cornerRadius = 8, alignItems = "center", x = -3, y = 30, bw = 14, bh = 15, stroke = "#2E2D29", strokeWidth = 1, fill = "#0A0A09", children = {
									{ type = "text", name = "Count label", content = "6", fontFamily = "Geist Mono", fontSize = 9, fontWeight = "500", bw = 6, bh = 12, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Link", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 134, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 56.58, soft = 0.18, ink = "#DC384159", speed = 0.1 }, children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", children = {
									{ type = "frame", name = "Attachment icon", width = 14, height = 14, cornerRadius = 3, bw = 14, bh = 14, img = "assets/opentable.png", fit = "cover" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 78, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 78, bh = 13, children = {
										{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "File stack popover", width = 268, layout = "vertical", padding = 3, cornerRadius = 12, bw = 268, bh = 144, stroke = "#2E2D29", strokeWidth = 1, fill = "#161615", shadows = {
							{ x = 0, y = 8, blur = 24, spread = 0, color = "#00000080", inner = false },
						}, children = {
						{ type = "frame", name = "Popover header", width = "fill_container", height = 26, gap = 6, padding = { 0, 8 }, alignItems = "center", bw = 262, bh = 26, children = {
							{ type = "text", name = "Popover title", content = "4 files", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 34, bh = 15, fill = "#EDEBE5" },
							{ type = "text", name = "Popover size", content = "5.6 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 114, bh = 1 },
							{ type = "text", name = "Ungroup", content = "Ungroup", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 44, bh = 14, fill = "#858379" },
						} },
						{ type = "frame", name = "Row · Dinner menu.pdf", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
							} },
							{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 93, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "2.4 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Floor plan.pdf", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, fill = "#201F1D", children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
							} },
							{ type = "text", name = "Attachment name", content = "Floor plan.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 78, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 46, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "3.1 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
							{ type = "icon", name = "Remove", width = 12, height = 12, icon = "x", bw = 12, bh = 12, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Row · Trip budget.xlsx", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#3A4232", strokeWidth = 1, fill = "#191C17", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-spreadsheet", bw = 11, bh = 11, fill = "#C3CEB2" },
							} },
							{ type = "text", name = "Attachment name", content = "Trip budget.xlsx", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 91, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "XLSX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#C3CEB2" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 53, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "84 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Itinerary.docx", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#2E3A3D", strokeWidth = 1, fill = "#151A1B", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-type", bw = 11, bh = 11, fill = "#A9BABF" },
							} },
							{ type = "text", name = "Attachment name", content = "Itinerary.docx", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 77, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "DOCX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#A9BABF" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 67, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "12 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
					} }, ml = 2 },
			},
		},
		{ name = "Composer · Overflow summary", group = "Composer states · overflow row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 110, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 78, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 78, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, padding = { 2, 2, 0, 2 }, bw = 384, bh = 26, children = {
							{ type = "frame", name = "Attachment list", width = "fill_container", layout = "vertical", gap = 4, padding = { 2, 2, 0, 2 }, bw = 380, bh = 24, children = {
								{ type = "frame", name = "List header", width = "fill_container", height = 22, gap = 6, padding = { 0, 6 }, alignItems = "center", bw = 376, bh = 22, children = {
									{ type = "icon", name = "List icon", width = 12, height = 12, icon = "paperclip", bw = 12, bh = 12, fill = "#A3A199" },
									{ type = "text", name = "List count", content = "13 attachments", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 78, bh = 15, fill = "#EDEBE5" },
									{ type = "text", name = "List size", content = "24.1 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 42, bh = 13, fill = "#6E6C65" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 148, bh = 1 },
									{ type = "text", name = "Clear all", content = "Clear all", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 42, bh = 14, fill = "#858379" },
									{ type = "icon", name = "Collapse", width = 12, height = 12, icon = "chevron-up", bw = 12, bh = 12, fill = "#858379" },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Composer · Overflow summary · Open", group = "Composer states · overflow row 2",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 110, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 78, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 78, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = 384, padding = { 2, 2, 0, 2 }, bw = 384, bh = 26, children = {
							{ type = "frame", name = "Attachment list", width = "fill_container", layout = "vertical", gap = 4, padding = { 2, 2, 0, 2 }, bw = 380, bh = 24, children = {
								{ type = "frame", name = "List header", width = "fill_container", height = 22, gap = 6, padding = { 0, 6 }, alignItems = "center", bw = 376, bh = 22, children = {
									{ type = "icon", name = "List icon", width = 12, height = 12, icon = "paperclip", bw = 12, bh = 12, fill = "#A3A199" },
									{ type = "text", name = "List count", content = "13 attachments", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 78, bh = 15, fill = "#EDEBE5" },
									{ type = "text", name = "List size", content = "24.1 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 42, bh = 13, fill = "#6E6C65" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 148, bh = 1 },
									{ type = "text", name = "Clear all", content = "Clear all", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 42, bh = 14, fill = "#858379" },
									{ type = "icon", name = "Collapse", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Overflow popover", width = 300, layout = "vertical", padding = 3, cornerRadius = 12, bw = 300, bh = 230, stroke = "#2E2D29", strokeWidth = 1, fill = "#161615", shadows = {
							{ x = 0, y = 8, blur = 24, spread = 0, color = "#00000080", inner = false },
						}, children = {
						{ type = "frame", name = "Row · 6 photos", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 374, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
							{ type = "text", name = "Attachment name", content = "6 photos", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 50, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "IMG", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#A3A199" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 206, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "8.2 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Dinner menu.pdf", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
							} },
							{ type = "text", name = "Attachment name", content = "Dinner menu.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 93, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "2.4 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Floor plan.pdf", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, fill = "#201F1D", children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
							} },
							{ type = "text", name = "Attachment name", content = "Floor plan.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 78, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 46, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "3.1 MB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
							{ type = "icon", name = "Remove", width = 12, height = 12, icon = "x", bw = 12, bh = 12, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Row · Trip budget.xlsx", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#3A4232", strokeWidth = 1, fill = "#191C17", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-spreadsheet", bw = 11, bh = 11, fill = "#C3CEB2" },
							} },
							{ type = "text", name = "Attachment name", content = "Trip budget.xlsx", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 91, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "XLSX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#C3CEB2" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 53, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "84 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Itinerary.docx", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#2E3A3D", strokeWidth = 1, fill = "#151A1B", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-type", bw = 11, bh = 11, fill = "#A9BABF" },
							} },
							{ type = "text", name = "Attachment name", content = "Itinerary.docx", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 77, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "DOCX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#A9BABF" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 67, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "12 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · notes.md", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-code", bw = 11, bh = 11, fill = "#A3A199" },
							} },
							{ type = "text", name = "Attachment name", content = "notes.md", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 77, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "MD", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#A3A199" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 67, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "3 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Lisboeta", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
								{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/opentable.png", fit = "cover" },
							} },
							{ type = "text", name = "Attachment name", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 77, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "LINK", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 67, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
						{ type = "frame", name = "Row · Flights to Lisbon", width = "fill_container", height = 28, gap = 8, padding = { 0, 8, 0, 5 }, cornerRadius = 7, alignItems = "center", bw = 262, bh = 28, children = {
							{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
								{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/google.png", fit = "cover" },
							} },
							{ type = "text", name = "Attachment name", content = "Flights to Lisbon", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 77, bh = 16, fill = "#EDEBE5" },
							{ type = "text", name = "Type badge", content = "TAB", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#9FB8D9" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 67, bh = 1 },
							{ type = "text", name = "Attachment detail", content = "google.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
						} },
					} }, ml = 2 },
			},
		},
		{ name = "Queue · Row hover", group = "Composer states · Queue and question tool",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 268, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 236, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 124, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 36, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
								{ type = "frame", name = "Hover actions · Edit then send now", width = 128, height = 28, gap = 4, padding = { 0, 0, 0, 8 }, alignItems = "center", x = 216, y = 1, layoutPosition = "absolute", bw = 128, bh = 28, fill = "#171716", children = {
									{ type = "frame", name = "Edit queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 28, bh = 28, children = {
										{ type = "icon", name = "Edit queued message", width = 14, height = 14, icon = "pencil", bw = 14, bh = 14, fill = "#A3A199" },
									} },
									{ type = "frame", name = "Send now · Hover action", height = 28, gap = 4, padding = { 0, 5 }, cornerRadius = 6, alignItems = "center", bw = 79, bh = 28, fill = "#292825", children = {
										{ type = "icon", name = "Send now icon", width = 14, height = 14, icon = "corner-up-left", bw = 14, bh = 14, fill = "#EDEBE5" },
										{ type = "text", name = "Send now label", content = "Send now", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 51, bh = 14, fill = "#EDEBE5" },
									} },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 36, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 112, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Working message actions", width = "fill_container", layout = "vertical", gap = 8, padding = { 4, 6, 2, 6 }, bw = 384, bh = 60, children = {
							{ type = "text", name = "Steering explanation", content = "Send now redirects the current run.", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 177, bh = 14, fill = "#A3A199" },
							{ type = "frame", name = "Send or queue actions", width = "fill_container", gap = 8, bw = 372, bh = 32, children = {
								{ type = "frame", name = "Send now button", width = "fill_container", height = 32, gap = 7, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 182, bh = 32, fill = "#EDEBE5", children = {
									{ type = "icon", name = "Send now icon", width = 14, height = 14, icon = "corner-up-left", bw = 14, bh = 14, fill = "#121211" },
									{ type = "text", name = "Send now label", content = "Send now", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 55, bh = 16, fill = "#121211" },
								} },
								{ type = "frame", name = "Queue button", width = "fill_container", height = 32, gap = 7, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 182, bh = 32, fill = "#292825", children = {
									{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-plus", bw = 14, bh = 14, fill = "#EDEBE5" },
									{ type = "text", name = "Queue label", content = "Queue", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 36, bh = 16, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Actually, make it 6 people.", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Selected choice", group = "Composer states · Queue and question tool",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 378, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 346, children = {
					{ type = "frame", name = "Question tool · Choice or typed answer", width = "fill_container", layout = "vertical", gap = 12, padding = 14, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 302, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Question status", width = "fill_container", gap = 8, alignItems = "center", bw = 368, bh = 17, children = {
							{ type = "ellipse", name = "Question orb", width = 14, height = 14, bw = 14, bh = 14, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question status label", width = "fill_container", content = "Question for you", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 302, bh = 17, fill = "#A8B4F5" },
							{ type = "text", name = "Question count", content = "1 of 1", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#858379" },
						} },
						{ type = "text", name = "Question prompt", width = "fill_container", content = "The window tables are full. Where would you like to sit?", fontFamily = "Geist", fontSize = 14, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 368, bh = 20, fill = "#EDEBE5" },
						{ type = "frame", name = "Choice · Main dining room", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#5F6AA3", strokeWidth = 1, fill = "#1D1F2B", children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#A8B4F5", strokeWidth = 1, fill = "#A8B4F5" },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "Main dining room", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "Quieter tables · Recommended", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "frame", name = "Choice · At the counter", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#33322D", strokeWidth = 1, children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#858379", strokeWidth = 1 },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "At the counter", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "A view into the open kitchen", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "frame", name = "Choice · Either is fine", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#33322D", strokeWidth = 1, children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#858379", strokeWidth = 1 },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "Either is fine", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "Book the first available table", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "text", name = "Custom answer hint", width = "fill_container", content = "Or type a different answer below.", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 368, bh = 15, fill = "#A3A199" },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Type another answer…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Typed answer", group = "Composer states · Queue and question tool",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 378, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 346, children = {
					{ type = "frame", name = "Question tool · Choice or typed answer", width = "fill_container", layout = "vertical", gap = 12, padding = 14, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 302, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Question status", width = "fill_container", gap = 8, alignItems = "center", bw = 368, bh = 17, children = {
							{ type = "ellipse", name = "Question orb", width = 14, height = 14, bw = 14, bh = 14, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question status label", width = "fill_container", content = "Question for you", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 302, bh = 17, fill = "#A8B4F5" },
							{ type = "text", name = "Question count", content = "1 of 1", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#858379" },
						} },
						{ type = "text", name = "Question prompt", width = "fill_container", content = "The window tables are full. Where would you like to sit?", fontFamily = "Geist", fontSize = 14, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 368, bh = 20, fill = "#EDEBE5" },
						{ type = "frame", name = "Choice · Main dining room", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#33322D", strokeWidth = 1, children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#858379", strokeWidth = 1 },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "Main dining room", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "Quieter tables · Recommended", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "frame", name = "Choice · At the counter", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#33322D", strokeWidth = 1, children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#858379", strokeWidth = 1 },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "At the counter", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "A view into the open kitchen", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "frame", name = "Choice · Either is fine", width = "fill_container", gap = 10, padding = 10, cornerRadius = 8, alignItems = "center", bw = 368, bh = 54, stroke = "#33322D", strokeWidth = 1, children = {
							{ type = "ellipse", name = "Choice indicator", width = 14, height = 14, bw = 14, bh = 14, stroke = "#858379", strokeWidth = 1 },
							{ type = "frame", name = "Option labels", width = "fill_container", layout = "vertical", gap = 2, bw = 324, bh = 34, children = {
								{ type = "text", name = "Option title", width = "fill_container", content = "Either is fine", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 17, fill = "#EDEBE5" },
								{ type = "text", name = "Option description", width = "fill_container", content = "Book the first available table", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 324, bh = 15, fill = "#A3A199" },
							} },
						} },
						{ type = "text", name = "Custom answer hint", width = "fill_container", content = "Or type a different answer below.", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 368, bh = 15, fill = "#A3A199" },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "A quiet table away from the kitchen.", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Compact · Selected", group = "Compact question states",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 226, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 194, children = {
					{ type = "frame", name = "Question tool · Compact", width = 396, layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 278, bh = 1 },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Prompt clamp", width = "fill_container", layout = "vertical", padding = { 0, 4 }, clip = true, bw = 380, bh = 18, children = {
							{ type = "text", name = "Question prompt", width = "fill_container", content = "The window tables are full. Where would you like to sit?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Options", width = "fill_container", layout = "vertical", gap = 1, bw = 380, bh = 86, children = {
							{ type = "frame", name = "Option 1", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, fill = "#1D1F2B", children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#A8B4F5", strokeWidth = 1, fill = "#A8B4F5", children = {
									{ type = "text", name = "Key label", content = "1", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#121211" },
								} },
								{ type = "text", name = "Option title", content = "Main dining room", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 97, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 236, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Quieter tables · Recommended", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 155, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 2", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "2", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "At the counter", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 81, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 252, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "A view into the open kitchen", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 143, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 3", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "3", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Either is fine", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 71, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 262, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Book the first available table", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 145, bh = 14, fill = "#858379" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Type another answer…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Compact · Long question", group = "Compact question states",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 392, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 360, children = {
					{ type = "frame", name = "Question tool · Compact", width = 396, layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 316, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 278, bh = 1 },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Prompt clamp", width = "fill_container", height = 110, layout = "vertical", padding = { 0, 4 }, clip = true, bw = 380, bh = 110, children = {
							{ type = "text", name = "Question prompt", width = "fill_container", content = "I found three restaurants near Alfama with a table for 6 at 8:30 pm on Saturday. Lisboeta has the best reviews but only has counter seats left, so the group would be sitting in a line facing the kitchen. Taberna da Rua has a terrace with a 15 minute wait, and they said the heaters will be on since it's forecast to drop to 14°. O Velho Eurico can do a private room if we pay a €40 deposit, which they refund when we arrive. All three can handle the vegetarian and gluten-free requests you mentioned. Which would you like me to book?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 162, fill = "#EDEBE5" },
							{ type = "frame", name = "Clamp fade", width = 380, height = 28, x = 0, y = 82, layoutPosition = "absolute", bw = 380, bh = 28, gradient = { angle = 180, stops = { "#17171600", "#171716" } } },
						} },
						{ type = "frame", name = "Show full question row", padding = { 0, 4 }, bw = 102, bh = 14, children = {
							{ type = "text", name = "Show full question", content = "Show full question", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 94, bh = 14, fill = "#C9C6BE" },
						} },
						{ type = "frame", name = "Options", width = "fill_container", layout = "vertical", gap = 1, bw = 380, bh = 140, children = {
							{ type = "frame", name = "Option 1", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "1", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Lisboeta", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 49, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 284, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Counter seats only · 4.7★ · best reviewed, mains around €28", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 303, bh = 16, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 2", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, fill = "#1D1F2B", children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#A8B4F5", strokeWidth = 1, fill = "#A8B4F5", children = {
									{ type = "text", name = "Key label", content = "2", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#121211" },
								} },
								{ type = "text", name = "Option title", content = "Taberna da Rua", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 87, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 246, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Terrace, 15 min wait · heaters on · 4.5★", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 194, bh = 16, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 3", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "3", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "O Velho Eurico", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 83, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 250, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Private room · €40 deposit, refunded on arrival", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 234, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 4", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "4", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Keep looking", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 73, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 260, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Search further out, including Graça and Mouraria", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 245, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "More options", width = "fill_container", height = 24, gap = 8, padding = { 0, 6 }, alignItems = "center", bw = 380, bh = 24, children = {
								{ type = "icon", name = "More icon", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
								{ type = "text", name = "More label", content = "2 more options", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 76, bh = 14, fill = "#A3A199" },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Type another answer…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Compact · Multiple questions", group = "Compact question states",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 226, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 194, children = {
					{ type = "frame", name = "Question tool · Compact", width = 396, layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 225, bh = 1 },
							{ type = "frame", name = "Pager", gap = 2, alignItems = "center", bw = 46, bh = 13, children = {
								{ type = "icon", name = "Previous", width = 12, height = 12, icon = "chevron-left", bw = 12, bh = 12, fill = "#6E6C65" },
								{ type = "text", name = "Pager label", content = "2/3", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 18, bh = 13, fill = "#A3A199" },
								{ type = "icon", name = "Next", width = 12, height = 12, icon = "chevron-right", bw = 12, bh = 12, fill = "#A3A199" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Prompt clamp", width = "fill_container", layout = "vertical", padding = { 0, 4 }, clip = true, bw = 380, bh = 18, children = {
							{ type = "text", name = "Question prompt", width = "fill_container", content = "What time should I book for?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Options", width = "fill_container", layout = "vertical", gap = 1, bw = 380, bh = 86, children = {
							{ type = "frame", name = "Option 1", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "1", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "7:30 pm", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 47, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 286, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Earliest slot left", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 79, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 2", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "2", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "8:30 pm", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 48, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 285, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Matches your calendar", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 116, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 3", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, fill = "#1D1F2B", children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#A8B4F5", strokeWidth = 1, fill = "#A8B4F5", children = {
									{ type = "text", name = "Key label", content = "3", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#121211" },
								} },
								{ type = "text", name = "Option title", content = "9:15 pm", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 45, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 288, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Terrace opens up", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 86, bh = 14, fill = "#858379" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Type another answer…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question · Compact · Minimized", group = "Compact question states",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 110, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 78, children = {
					{ type = "frame", name = "Question tool · Compact", width = 396, layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 34, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Inline prompt", width = "fill_container", height = 16, alignItems = "center", clip = true, bw = 278, bh = 16, children = {
								{ type = "text", name = "Inline prompt text", content = "The window tables are full. Where would you…", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 252, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-up", bw = 12, bh = 12, fill = "#858379" },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Answer, or press 1–3", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Queue · Attachments included", group = "Combined states · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 268, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 236, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 192, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Image", width = 26, height = 26, cornerRadius = 7, bw = 26, bh = 26, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 146, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 90.82, soft = 0.18, ink = "#1DBF6159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Parking near Alfama", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 109, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "PDF", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 110, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
										{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
									} },
									{ type = "text", name = "Attachment name", content = "Guest list.pdf", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 73, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Actually, make it 6 people.", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Queue · Attachment preview", group = "Combined states · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 268, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 236, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 192, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Image", width = 26, height = 26, cornerRadius = 7, bw = 26, bh = 26, stroke = "#858379", strokeWidth = 1.5, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 146, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 35.62, soft = 0.18, ink = "#1DBF6159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Parking near Alfama", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 109, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "PDF", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 110, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
										{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#E8A898" },
									} },
									{ type = "text", name = "Attachment name", content = "Guest list.pdf", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 73, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Attachment preview popover", width = 232, layout = "vertical", gap = 8, padding = 6, cornerRadius = 12, x = 31, y = 116, layoutPosition = "absolute", bw = 232, bh = 185, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", shadows = {
							{ x = 0, y = 12, blur = 32, spread = 0, color = "#000000B3", inner = false },
						}, children = {
						{ type = "frame", name = "Preview image", width = "fill_container", height = 132, cornerRadius = 8, bw = 220, bh = 132, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
						{ type = "frame", name = "Preview footer", width = "fill_container", gap = 8, padding = { 0, 4, 2, 4 }, alignItems = "center", bw = 220, bh = 33, children = {
							{ type = "frame", name = "Preview meta", width = "fill_container", layout = "vertical", gap = 2, bw = 180, bh = 31, children = {
								{ type = "text", name = "File name", content = "street-outside.jpg", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 97, bh = 15, fill = "#EDEBE5" },
								{ type = "text", name = "File detail", content = "JPG · 1.8 MB · sends with #1", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 131, bh = 14, fill = "#858379" },
							} },
							{ type = "frame", name = "Open full size", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, fill = "#292825", children = {
								{ type = "icon", name = "Open icon", width = 12, height = 12, icon = "maximize-2", bw = 12, bh = 12, fill = "#C9C6BE" },
							} },
						} },
					} } },
			},
		},
		{ name = "Queue · Upload still running", group = "Combined states · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 192, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 160, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 116, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 1", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 56, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 195, bh = 1 },
							{ type = "text", name = "Queue timing", content = "Waits for upload", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 83, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Image", width = 26, height = 26, cornerRadius = 7, bw = 26, bh = 26, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Uploading", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 136, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
									{ type = "frame", name = "Upload progress wash", width = 70, height = 26, cornerRadius = { 8, 0, 0, 8 }, x = 0, y = 0, layoutPosition = "absolute", bw = 70, bh = 26, dither = { mode = 0, pixel = 1, angle = 270, cx = 0.5, cy = 0.5, radius = 0.5, from = 1, to = 0.2, curve = 0.8, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE538" } },
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 1.1, radius = 1.1, from = 0.2, to = 0.7, curve = 0.8, noise = 0, scale = 1.6, seed = 4, soft = 0, ink = "#8A877EFF" } },
									{ type = "text", name = "Attachment name", content = "Floor plan.pdf", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 75, bh = 15, fill = "#EDEBE5" },
									{ type = "text", name = "Attachment detail", content = "62%", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "normal", bw = 18, bh = 12, fill = "#A3A199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question + queue", group = "Combined states · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 384, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 352, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 158, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Image", width = 26, height = 26, cornerRadius = 7, bw = 26, bh = 26, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 146, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 69.18, soft = 0.18, ink = "#1DBF6159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Parking near Alfama", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 109, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 36, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Question tool · Compact", width = "fill_container", layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 278, bh = 1 },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Prompt clamp", width = "fill_container", layout = "vertical", padding = { 0, 4 }, clip = true, bw = 380, bh = 18, children = {
							{ type = "text", name = "Question prompt", width = "fill_container", content = "The window tables are full. Where would you like to sit?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Options", width = "fill_container", layout = "vertical", gap = 1, bw = 380, bh = 86, children = {
							{ type = "frame", name = "Option 1", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "1", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Main dining room", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 97, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 236, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Quieter tables · Recommended", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 155, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 2", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "2", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "At the counter", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 81, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 252, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "A view into the open kitchen", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 143, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 3", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "3", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Either is fine", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 71, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 262, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Book the first available table", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 145, bh = 14, fill = "#858379" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Type another answer…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Question + queue + chat attachment", group = "Combined states · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 434, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 402, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 158, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Image", width = 26, height = 26, cornerRadius = 7, bw = 26, bh = 26, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 146, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 116.03, soft = 0.18, ink = "#1DBF6159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Parking near Alfama", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 109, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 36, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Question tool · Compact", width = "fill_container", layout = "vertical", gap = 6, padding = { 10, 8, 6, 8 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = 1, fill = "#171716", children = {
						{ type = "frame", name = "Question header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Question orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#1C1E28", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#A8B4F5FF" } },
							{ type = "text", name = "Question label", content = "Question", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 49, bh = 15, fill = "#A8B4F5" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 278, bh = 1 },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Prompt clamp", width = "fill_container", layout = "vertical", padding = { 0, 4 }, clip = true, bw = 380, bh = 18, children = {
							{ type = "text", name = "Question prompt", width = "fill_container", content = "The window tables are full. Where would you like to sit?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Options", width = "fill_container", layout = "vertical", gap = 1, bw = 380, bh = 86, children = {
							{ type = "frame", name = "Option 1", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "1", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Main dining room", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 97, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 236, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Quieter tables · Recommended", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 155, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 2", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "2", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "At the counter", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 81, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 252, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "A view into the open kitchen", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 143, bh = 14, fill = "#858379" },
								} },
							} },
							{ type = "frame", name = "Option 3", width = "fill_container", height = 28, gap = 8, padding = { 0, 6 }, cornerRadius = 7, alignItems = "center", bw = 380, bh = 28, children = {
								{ type = "frame", name = "Key", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 19, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "text", name = "Key label", content = "3", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
								} },
								{ type = "text", name = "Option title", content = "Either is fine", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 71, bh = 16, fill = "#EDEBE5" },
								{ type = "frame", name = "Option description", width = "fill_container", height = 15, alignItems = "center", clip = true, bw = 262, bh = 15, children = {
									{ type = "text", name = "Option description text", content = "Book the first available table", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 145, bh = 14, fill = "#858379" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 94, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Attachments", width = "fill_container", gap = 8, padding = { 2, 2, 0, 2 }, bw = 384, bh = 42, children = {
							{ type = "frame", name = "PDF", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 176, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
								{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#4A2C25", strokeWidth = 1, fill = "#1C1513", children = {
									{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "file-text", bw = 14, bh = 14, fill = "#E8A898" },
								} },
								{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 120, bh = 31, children = {
									{ type = "text", name = "Attachment name", content = "Dietary notes.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 97, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 120, bh = 13, children = {
										{ type = "text", name = "Type badge", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
										{ type = "text", name = "Attachment detail", content = "2 pages · 180 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 96, bh = 13, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Terrace is fine, here are the dietary notes", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 304, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Queue · Many attachments", group = "Combined states · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 268, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 236, children = {
					{ type = "frame", name = "Queued messages · After this run", width = "fill_container", layout = "vertical", gap = 6, padding = 12, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 192, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Queue header", width = "fill_container", gap = 8, alignItems = "center", bw = 372, bh = 16, children = {
							{ type = "icon", name = "Queue icon", width = 14, height = 14, icon = "list-ordered", bw = 14, bh = 14, fill = "#A3A199" },
							{ type = "text", name = "Queue count", content = "Queued · 2", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 59, bh = 16, fill = "#EDEBE5" },
							{ type = "frame", name = "Queue header spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "text", name = "Queue timing", content = "After this run", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 67, bh = 14, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Queued message 1", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "1", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 5, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Find parking near the restaurant", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 329, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "Photo stack", width = 48, height = 26, layout = "none", bw = 48, bh = 26, children = {
									{ type = "frame", name = "Photo 3", width = 21, height = 21, cornerRadius = 6, x = 24, y = 4, rotation = -9, clip = true, bw = 24.03, bh = 24.03, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1536664607464-9a9419341c4d.jpg", fit = "cover", children = {
										{ type = "rectangle", name = "Depth shade", width = 21, height = 21, x = 0, y = 0, layoutPosition = "absolute", bw = 21, bh = 21, fill = "#0A0A0959" },
									} },
									{ type = "frame", name = "Photo 2", width = 24, height = 24, cornerRadius = 6, x = 13, y = 2, rotation = -1, clip = true, bw = 24.42, bh = 24.42, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1788009145784-429a60a09b47.jpg", fit = "cover", children = {
										{ type = "rectangle", name = "Depth shade", width = 24, height = 24, x = 0, y = 0, layoutPosition = "absolute", bw = 24, bh = 24, fill = "#0A0A0933" },
									} },
									{ type = "frame", name = "Photo 1", width = 26, height = 26, cornerRadius = 7, x = 1.5, y = 3.5, rotation = 8, bw = 29.37, bh = 29.37, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
									{ type = "frame", name = "Count", height = 13, padding = { 0, 3.5 }, cornerRadius = 7, alignItems = "center", x = -3, y = 16, bw = 13, bh = 13, stroke = "#2E2D29", strokeWidth = 1, fill = "#0A0A09", children = {
										{ type = "text", name = "Count label", content = "6", fontFamily = "Geist Mono", fontSize = 8.5, fontWeight = "500", bw = 6, bh = 11, fill = "#EDEBE5" },
									} },
								} },
								{ type = "frame", name = "Tab", height = 26, gap = 6, padding = { 0, 9, 0, 4 }, cornerRadius = 8, alignItems = "center", bw = 146, bh = 26, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 0.5, curve = 1.3, noise = 0, scale = 2.6, seed = 86.85, soft = 0.18, ink = "#1DBF6159", speed = 0.1 }, children = {
									{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#121211", children = {
										{ type = "frame", name = "Attachment icon", width = 12, height = 12, cornerRadius = 3, bw = 12, bh = 12, img = "assets/gmaps.png", fit = "cover" },
									} },
									{ type = "text", name = "Attachment name", content = "Parking near Alfama", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 109, bh = 15, fill = "#EDEBE5" },
								} },
							} },
						} },
						{ type = "frame", name = "Queued message 2", width = "fill_container", layout = "vertical", padding = { 6, 0, 0, 0 }, bw = 372, bh = 70, stroke = "#2A2925", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Message and edit controls", width = "fill_container", height = 30, gap = 8, padding = { 0, 30, 0, 0 }, alignItems = "center", bw = 372, bh = 30, children = {
								{ type = "text", name = "Queue position", content = "2", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 7, bh = 14, fill = "#858379" },
								{ type = "text", name = "Queued message text", width = "fill_container", content = "Add the booking to my calendar", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 327, bh = 17, fill = "#C9C6BE" },
								{ type = "frame", name = "Remove queued message · 28px", width = 28, height = 28, cornerRadius = 6, justifyContent = "center", alignItems = "center", x = 344, y = 1, layoutPosition = "absolute", bw = 28, bh = 28, fill = "#171716", children = {
									{ type = "icon", name = "Remove queued message", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#A3A199" },
								} },
							} },
							{ type = "frame", name = "Queued attachments", width = "fill_container", gap = 6, padding = { 2, 30, 6, 15 }, alignItems = "center", bw = 372, bh = 34, children = {
								{ type = "frame", name = "File stack", width = 98, height = 26, layout = "none", bw = 98, bh = 26, children = {
									{ type = "frame", name = "Layer 3", width = 88, height = 16, cornerRadius = 6, x = 10, y = 5, bw = 88, bh = 16, stroke = "#22211E", strokeWidth = 1, fill = "#121211" },
									{ type = "frame", name = "Layer 2", width = 88, height = 21, cornerRadius = 7, x = 5, y = 2.5, bw = 88, bh = 21, stroke = "#262521", strokeWidth = 1, fill = "#161615" },
									{ type = "frame", name = "Top file", width = 88, height = 26, gap = 6, padding = { 0, 8, 0, 4 }, cornerRadius = 8, alignItems = "center", x = 0, y = 0, bw = 88, bh = 26, stroke = "#2E2D29", strokeWidth = 1, fill = "#1A1A18", children = {
										{ type = "frame", name = "Attachment thumb", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
											{ type = "icon", name = "Attachment icon", width = 11, height = 11, icon = "files", bw = 11, bh = 11, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Attachment name", content = "4 files", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 34, bh = 15, fill = "#EDEBE5" },
										{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 1, bh = 1 },
										{ type = "icon", name = "Expand", width = 10, height = 10, icon = "chevrons-up-down", bw = 10, bh = 10, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, @ for context", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Details only", group = "Approval variants · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 285, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 253, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 209, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Book a table for 4 at Lisboeta?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 95, children = {
							{ type = "frame", name = "Detail · When", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "When", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Fri 3 Oct · 20:00", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 87, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Party", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Party", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "4 guests", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 46, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Seating", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Seating", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Window (requested)", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 105, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Booked as", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Booked as", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Sen W. · sen@…", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 80, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Cancellation", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Cancellation", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Free until 24h before", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 110, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 98, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Approve", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 49, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · With the agent's reasoning", group = "Approval variants · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 289, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 257, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 213, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Add a card to hold the table?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Rationale", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 54, children = {
							{ type = "text", name = "Rationale text", width = "fill_container", content = "OpenTable wants a card to hold the table. It's only charged if you cancel late, but I never enter payment details without asking you first.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 54, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 35, children = {
							{ type = "frame", name = "Detail · Booked as", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Card", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Visa ··4821 (saved)", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 96, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Cancellation", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Cancellation", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "No charge unless cancelled <24h", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 175, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 98, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Approve", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 49, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · With page snapshot", group = "Approval variants · row 1",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 406, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 374, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 330, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Book a table for 4 at Lisboeta?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Snapshot", width = "fill_container", layout = "vertical", gap = 6, bw = 380, bh = 151, children = {
							{ type = "frame", name = "Page snapshot", width = "fill_container", height = 132, layout = "vertical", cornerRadius = 9, clip = true, bw = 380, bh = 132, stroke = "#262521", strokeWidth = 1, fill = "#F6F5F2", children = {
								{ type = "frame", name = "Site bar", width = "fill_container", height = 22, gap = 6, padding = { 0, 10 }, alignItems = "center", bw = 380, bh = 22, stroke = "#E4E2DC", strokeWidth = { bottom = 1 }, fill = "#FFFFFF", children = {
									{ type = "frame", name = "Logo dot", width = 8, height = 8, cornerRadius = 4, bw = 8, bh = 8, fill = "#DA3743" },
									{ type = "text", name = "Wordmark", content = "OpenTable", fontFamily = "Geist", fontSize = 9, fontWeight = "600", bw = 46, bh = 12, fill = "#1E1E1C" },
								} },
								{ type = "frame", name = "Page body", width = "fill_container", height = "fill_container", layout = "vertical", gap = 6, padding = { 9, 12 }, bw = 380, bh = 110, children = {
									{ type = "text", name = "Page title", content = "Complete your reservation", fontFamily = "Geist", fontSize = 11, fontWeight = "600", bw = 140, bh = 14, fill = "#1E1E1C" },
									{ type = "text", name = "Page meta", content = "Lisboeta · Fri 3 Oct · 20:00 · 4 people", fontFamily = "Geist", fontSize = 8.5, fontWeight = "normal", bw = 143, bh = 11, fill = "#6A6862" },
									{ type = "frame", name = "Fields", width = "fill_container", gap = 6, bw = 356, bh = 18, children = {
										{ type = "frame", name = "Field", width = "fill_container", height = 18, padding = { 0, 6 }, cornerRadius = 3, alignItems = "center", bw = 114.67, bh = 18, stroke = "#DDDBD4", strokeWidth = 1, fill = "#FFFFFF", children = {
											{ type = "text", name = "Field value", content = "Sen W.", fontFamily = "Geist", fontSize = 8, fontWeight = "normal", bw = 25, bh = 10, fill = "#3A3935" },
										} },
										{ type = "frame", name = "Field", width = "fill_container", height = 18, padding = { 0, 6 }, cornerRadius = 3, alignItems = "center", bw = 114.67, bh = 18, stroke = "#DDDBD4", strokeWidth = 1, fill = "#FFFFFF", children = {
											{ type = "text", name = "Field value", content = "sen@…", fontFamily = "Geist", fontSize = 8, fontWeight = "normal", bw = 25, bh = 10, fill = "#3A3935" },
										} },
										{ type = "frame", name = "Field", width = "fill_container", height = 18, padding = { 0, 6 }, cornerRadius = 3, alignItems = "center", bw = 114.67, bh = 18, stroke = "#DDDBD4", strokeWidth = 1, fill = "#FFFFFF", children = {
											{ type = "text", name = "Field value", content = "+44 ··· 4821", fontFamily = "Geist", fontSize = 8, fontWeight = "normal", bw = 41, bh = 10, fill = "#3A3935" },
										} },
									} },
									{ type = "frame", name = "Target ring", padding = 2, cornerRadius = 6, strokeAlignment = "outer", bw = 112, bh = 24, stroke = "#E2C98F", strokeWidth = 2, children = {
										{ type = "frame", name = "Page button", height = 20, padding = { 0, 10 }, cornerRadius = 3, alignItems = "center", bw = 108, bh = 20, fill = "#DA3743", children = {
											{ type = "text", name = "Page button label", content = "Complete reservation", fontFamily = "Geist", fontSize = 8.5, fontWeight = "600", bw = 88, bh = 11, fill = "#FFFFFF" },
										} },
									} },
								} },
							} },
							{ type = "frame", name = "Snapshot caption", width = "fill_container", gap = 6, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 13, children = {
								{ type = "frame", name = "Ring key", width = 8, height = 8, cornerRadius = 2, bw = 8, bh = 8, stroke = "#E2C98F", strokeWidth = 1.5 },
								{ type = "text", name = "Caption", content = "I'll click this · captured 3s ago", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 198, bh = 13, fill = "#858379" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 88, bh = 1 },
								{ type = "text", name = "Open tab", content = "Show tab ↗", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 60, bh = 13, fill = "#A3A199" },
							} },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 55, children = {
							{ type = "frame", name = "Detail · Party", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Party", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "4 guests", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 46, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Seating", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Seating", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Window (requested)", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 105, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Cancellation", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Cancellation", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Free until 24h before", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 110, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 98, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Approve", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 49, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Payment", group = "Approval variants · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 285, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 253, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 209, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK · payment", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 136, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 92, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Pay the £86.40 group deposit?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Rationale", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 36, children = {
							{ type = "text", name = "Rationale text", width = "fill_container", content = "Lisboeta asks for a deposit on Friday tables for 4 or more. I checked 19:30 to 21:30 and every slot needs one.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 36, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Amount", width = "fill_container", gap = 8, padding = { 0, 4 }, alignItems = "end", bw = 380, bh = 23, children = {
							{ type = "text", name = "Amount value", content = "£86.40", fontFamily = "Geist Mono", fontSize = 18, fontWeight = "500", bw = 65, bh = 23, fill = "#EDEBE5" },
							{ type = "text", name = "Amount detail", content = "Visa ··4821 · charged now", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 150, bh = 13, fill = "#858379" },
						} },
						{ type = "frame", name = "Warning", width = "fill_container", gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 16, children = {
							{ type = "icon", name = "Warning icon", width = 12, height = 12, icon = "triangle-alert", bw = 12, bh = 12, fill = "#E8A898" },
							{ type = "text", name = "Warning text", content = "Non‑refundable after Thu 2 Oct", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 165, bh = 16, fill = "#E8A898" },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 189, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 117, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Pay £86.40", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 68, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Sending a message", group = "Approval variants · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 263, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 231, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 187, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Add this note to the booking?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Message preview", width = "fill_container", layout = "vertical", gap = 6, padding = { 8, 10 }, cornerRadius = 9, bw = 380, bh = 73, stroke = "#262521", strokeWidth = 1, fill = "#121211", children = {
							{ type = "frame", name = "Message to", width = "fill_container", gap = 6, alignItems = "center", bw = 360, bh = 15, children = {
								{ type = "text", name = "To label", content = "TO", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", letterSpacing = 0.4, bw = 13, bh = 13, fill = "#6E6C65" },
								{ type = "text", name = "Recipient", content = "Lisboeta · reservation note", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 138, bh = 15, fill = "#A3A199" },
							} },
							{ type = "text", name = "Message body", width = "fill_container", content = "Hi, we're celebrating a birthday. If a window table frees up, we'd love it. Thanks!", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 360, bh = 36, fill = "#D4D2CB" },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 182, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow notes on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 162, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 3, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 87, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Don't send", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 63, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 80, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Send", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 31, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Several waiting", group = "Approval variants · row 2",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 245, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 213, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 169, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "2 need your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 83, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Pager", gap = 2, alignItems = "center", bw = 46, bh = 13, children = {
								{ type = "icon", name = "Previous", width = 12, height = 12, icon = "chevron-left", bw = 12, bh = 12, fill = "#6E6C65" },
								{ type = "text", name = "Pager label", content = "1/2", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 18, bh = 13, fill = "#A3A199" },
								{ type = "icon", name = "Next", width = 12, height = 12, icon = "chevron-right", bw = 12, bh = 12, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 92, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Book a table for 4 at Lisboeta?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 55, children = {
							{ type = "frame", name = "Detail · When", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "When", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Fri 3 Oct · 20:00", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 87, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Seating", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Seating", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Window (requested)", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 105, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Cancellation", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Cancellation", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Free until 24h before", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 110, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 98, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Approve", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 49, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Signing in", group = "Approval variants · row 3",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 291, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 259, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 215, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Sign in to OpenTable as sen@…?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Rationale", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 36, children = {
							{ type = "text", name = "Rationale text", width = "fill_container", content = "Guest bookings need a texted code I can't read. Your saved account skips that step.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 36, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 55, children = {
							{ type = "frame", name = "Detail · Seating", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Account", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "sen@… · saved in Passwords", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 148, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Booked as", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Fills in", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Email and password", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 106, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Cancellation", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Stays signed in", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "This tab only", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 67, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 60, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 89, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Sign in", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 40, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Asking for a change", group = "Approval variants · row 3",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 225, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 193, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 149, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 81, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 147, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-down", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Approval title", width = "fill_container", content = "Book a table for 4 at Lisboeta?", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Details", width = "fill_container", layout = "vertical", gap = 5, padding = { 0, 4 }, bw = 380, bh = 35, children = {
							{ type = "frame", name = "Detail · When", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "When", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Fri 3 Oct · 20:00", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 87, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Detail · Seating", width = "fill_container", gap = 8, bw = 372, bh = 15, children = {
								{ type = "text", name = "Key", width = 88, content = "Seating", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", textGrowth = "fixed-width", bw = 88, bh = 15, fill = "#858379" },
								{ type = "text", name = "Value", content = "Window (requested)", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 105, bh = 15, fill = "#EDEBE5" },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "frame", name = "Always allow", gap = 7, alignItems = "center", opacity = 0.35, bw = 149, bh = 15, children = {
								{ type = "frame", name = "Checkbox", width = 13, height = 13, cornerRadius = 4, bw = 13, bh = 13, stroke = "#5A5850", strokeWidth = 1 },
								{ type = "text", name = "Always allow label", content = "Always allow on this site", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", bw = 129, bh = 15, fill = "#858379" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 51, bh = 1 },
							{ type = "frame", name = "Deny", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", opacity = 0.35, bw = 54, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Deny", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 30, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Approve", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", opacity = 0.35, bw = 98, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Approve", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 49, bh = 16, fill = "#121211" },
								{ type = "frame", name = "Shortcut", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 18, bh = 20, stroke = "#12121133", strokeWidth = 1, children = {
									{ type = "text", name = "Shortcut label", content = "⏎", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", lineHeight = 1, bw = 6, bh = 10, fill = "#12121199" },
								} },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Make it 19:30 instead, and 5 people", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#EDEBE5" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Approval · Minimised", group = "Approval variants · row 3",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 112, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 80, children = {
					{ type = "frame", name = "Approval dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 36, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#25211A", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E2C98FFF" } },
							{ type = "text", name = "Dock label", content = "Needs your OK · Book Lisboeta for 4", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 194, bh = 15, fill = "#E2C98F" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 133, bh = 1 },
							{ type = "icon", name = "Minimize", width = 12, height = 12, icon = "chevron-up", bw = 12, bh = 12, fill = "#858379" },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Or tell Ask what to change…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "rectangle", name = "Stop glyph", width = 10, height = 10, cornerRadius = 2, bw = 10, bh = 10, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Error · Needs something only you have", group = "Error variants",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 226, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 194, children = {
					{ type = "frame", name = "Error dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#291A17", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E28C78FF" } },
							{ type = "text", name = "Dock label", content = "Stopped", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 46, bh = 15, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 182, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Dismiss", width = 12, height = 12, icon = "x", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Error title", width = "fill_container", content = "OpenTable needs a verification code", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Error body", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 36, children = {
							{ type = "text", name = "Error text", width = "fill_container", content = "A 6‑digit code went to the phone ending 4821. I can't read your messages, so I stopped before adding the seating note.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 36, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "text", name = "Reply hint", content = "or reply with the code", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 132, bh = 13, fill = "#858379" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 61, bh = 1 },
							{ type = "frame", name = "Take over", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 103, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Take over tab", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 79, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Retry", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 56, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Retry", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 32, bh = 16, fill = "#121211" },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply with the code…", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Error · The page changed", group = "Error variants",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 369, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 337, children = {
					{ type = "frame", name = "Error dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 293, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#291A17", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E28C78FF" } },
							{ type = "text", name = "Dock label", content = "Stopped", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 46, bh = 15, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 182, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Dismiss", width = 12, height = 12, icon = "x", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Error title", width = "fill_container", content = "The 20:00 table was taken while I was filling in the form", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Error body", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Error text", width = "fill_container", content = "OpenTable now offers 19:30 or 21:15. I haven't picked one.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Snapshot", width = "fill_container", layout = "vertical", gap = 6, bw = 380, bh = 151, children = {
							{ type = "frame", name = "Page snapshot", width = "fill_container", height = 132, layout = "vertical", cornerRadius = 9, clip = true, bw = 380, bh = 132, stroke = "#262521", strokeWidth = 1, fill = "#F6F5F2", children = {
								{ type = "frame", name = "Site bar", width = "fill_container", height = 22, gap = 6, padding = { 0, 10 }, alignItems = "center", bw = 380, bh = 22, stroke = "#E4E2DC", strokeWidth = { bottom = 1 }, fill = "#FFFFFF", children = {
									{ type = "frame", name = "Logo dot", width = 8, height = 8, cornerRadius = 4, bw = 8, bh = 8, fill = "#DA3743" },
									{ type = "text", name = "Wordmark", content = "OpenTable", fontFamily = "Geist", fontSize = 9, fontWeight = "600", bw = 46, bh = 12, fill = "#1E1E1C" },
								} },
								{ type = "frame", name = "Page body", width = "fill_container", height = "fill_container", layout = "vertical", gap = 6, padding = { 9, 12 }, bw = 380, bh = 110, children = {
									{ type = "text", name = "Page title", content = "This time is no longer available", fontFamily = "Geist", fontSize = 11, fontWeight = "600", bw = 162, bh = 14, fill = "#1E1E1C" },
									{ type = "text", name = "Page meta", content = "Lisboeta · Fri 3 Oct · 4 people · try 19:30 or 21:15", fontFamily = "Geist", fontSize = 8.5, fontWeight = "normal", bw = 185, bh = 11, fill = "#6A6862" },
									{ type = "frame", name = "Target ring", padding = 2, cornerRadius = 6, strokeAlignment = "outer", bw = 89, bh = 24, children = {
										{ type = "frame", name = "Page button", height = 20, padding = { 0, 10 }, cornerRadius = 3, alignItems = "center", bw = 85, bh = 20, fill = "#DA3743", children = {
											{ type = "text", name = "Page button label", content = "See other times", fontFamily = "Geist", fontSize = 8.5, fontWeight = "600", bw = 65, bh = 11, fill = "#FFFFFF" },
										} },
									} },
								} },
							} },
							{ type = "frame", name = "Snapshot caption", width = "fill_container", gap = 6, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 13, children = {
								{ type = "text", name = "Caption", content = "What I saw · captured 2s ago", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 168, bh = 13, fill = "#858379" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 198, bh = 1 },
							} },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "text", name = "Reply hint", content = "or reply with a time", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 120, bh = 13, fill = "#858379" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 73, bh = 1 },
							{ type = "frame", name = "Take over", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 80, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Show tab", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 56, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Retry", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 79, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Try 19:30", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 55, bh = 16, fill = "#121211" },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, or tell Ask what to do instead", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Error · Can't reach the site", group = "Error variants",
			composer = { type = "frame", name = "Composer", width = "fill_container", layout = "vertical", gap = 6, bw = 396, bh = 226, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 194, children = {
					{ type = "frame", name = "Error dock", width = "fill_container", layout = "vertical", gap = 10, padding = { 10, 8, 8, 8 }, cornerRadius = { 16, 16, 0, 0 }, bw = 396, bh = 150, stroke = "#33322D", strokeWidth = { top = 1, right = 1, left = 1 }, fill = "#171716", children = {
						{ type = "frame", name = "Dock header", width = "fill_container", height = 18, gap = 7, padding = { 0, 4 }, alignItems = "center", bw = 380, bh = 18, children = {
							{ type = "ellipse", name = "Dock orb", width = 12, height = 12, bw = 12, bh = 12, fill = "#291A17", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#E28C78FF" } },
							{ type = "text", name = "Dock label", content = "Stopped", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 46, bh = 15, fill = "#E8A898" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 182, bh = 1 },
							{ type = "frame", name = "Site", gap = 5, alignItems = "center", bw = 92, bh = 13, children = {
								{ type = "frame", name = "Favicon", width = 9, height = 9, cornerRadius = 2, bw = 9, bh = 9, fill = "#DA3743" },
								{ type = "text", name = "Domain", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "icon", name = "Dismiss", width = 12, height = 12, icon = "x", bw = 12, bh = 12, fill = "#858379" },
						} },
						{ type = "frame", name = "Title wrap", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 18, children = {
							{ type = "text", name = "Error title", width = "fill_container", content = "Can't reach opentable.com", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 372, bh = 18, fill = "#EDEBE5" },
						} },
						{ type = "frame", name = "Error body", width = "fill_container", layout = "vertical", padding = { 0, 4 }, bw = 380, bh = 36, children = {
							{ type = "text", name = "Error text", width = "fill_container", content = "The connection dropped twice while saving the form. Nothing was submitted.", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.5, textGrowth = "fixed-width", bw = 372, bh = 36, fill = "#A3A199" },
						} },
						{ type = "frame", name = "Dock actions", width = "fill_container", gap = 8, padding = { 2, 0, 0, 4 }, alignItems = "center", bw = 380, bh = 30, children = {
							{ type = "text", name = "Reply hint", content = "retrying in 8s", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 84, bh = 13, fill = "#858379" },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 132, bh = 1 },
							{ type = "frame", name = "Take over", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 52, bh = 28, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
								{ type = "text", name = "Label", content = "Stop", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 28, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Retry", height = 28, gap = 7, padding = { 0, 12 }, cornerRadius = 8, justifyContent = "center", alignItems = "center", bw = 84, bh = 28, fill = "#EDEBE5", children = {
								{ type = "text", name = "Label", content = "Retry now", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 60, bh = 16, fill = "#121211" },
							} },
						} },
					} },
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = { 0, 0, 16, 16 }, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "text", name = "Reply placeholder", width = "fill_container", content = "Reply, or tell Ask what to do instead", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 264, bh = 19, fill = "#57554F" },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {  },
		},
		{ name = "Resolved in the trace", group = "After a decision",
			composer = { type = "frame", name = "State · Resolved in the trace", width = 420, layout = "vertical", gap = 12, bw = 420, bh = 227, children = {
				{ type = "text", name = "State label", width = "fill_container", content = "Resolved · how it lands in the trace", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 420, bh = 17, fill = "#A3A199" },
				{ type = "frame", name = "Panel", width = "fill_container", layout = "vertical", padding = { 16, 18 }, cornerRadius = 12, bw = 420, bh = 156, stroke = "#262521", strokeWidth = 1, fill = "#121211", children = {
					{ type = "frame", name = "Action · Added seating note", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
						{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
							{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
								{ type = "icon", name = "Icon", width = 13, height = 13, icon = "pen-line", bw = 13, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
								{ type = "text", name = "Label", content = "Added seating note", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 108, bh = 16, fill = "#A3A199" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 245, bh = 1 },
							} },
						} },
						{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
							{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
						} },
					} },
					{ type = "frame", name = "Action · Approved by you · Book a table for 4", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
						{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
							{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
								{ type = "icon", name = "Icon", width = 13, height = 13, icon = "check", bw = 13, bh = 13, fill = "#C3CEB2" },
							} },
							{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
								{ type = "text", name = "Label", content = "Approved by you · Book a table for 4", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 199, bh = 16, fill = "#C3CEB2" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 116, bh = 1 },
								{ type = "text", name = "Meta", content = "20:01", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
							} },
						} },
						{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
							{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
						} },
					} },
					{ type = "frame", name = "Action · Completed reservation", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
						{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
							{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
								{ type = "icon", name = "Icon", width = 13, height = 13, icon = "mouse-pointer-click", bw = 13, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
								{ type = "text", name = "Label", content = "Completed reservation", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 125, bh = 16, fill = "#A3A199" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 142, bh = 1 },
								{ type = "text", name = "Meta", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
							} },
						} },
						{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
							{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
						} },
					} },
					{ type = "frame", name = "Action · Booking confirmed · #OT‑48213", width = "fill_container", layout = "vertical", bw = 384, bh = 22, children = {
						{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
							{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
								{ type = "icon", name = "Icon", width = 13, height = 13, icon = "circle-check", bw = 13, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
								{ type = "text", name = "Label", content = "Booking confirmed · #OT‑48213", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 171, bh = 16, fill = "#A3A199" },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 182, bh = 1 },
							} },
						} },
					} },
				} },
				{ type = "text", name = "Behavior", width = "fill_container", content = "The dock closes and the decision becomes a step in the trace, with who decided and when. Denied requests read “Denied by you” and the run ends there.", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", lineHeight = 1.4, textGrowth = "fixed-width", bw = 420, bh = 30, fill = "#A3A199" },
			} },
			overlays = {
				{ node = { type = "frame", name = "Panel", width = "fill_container", layout = "vertical", padding = { 16, 18 }, cornerRadius = 12, bw = 420, bh = 156, stroke = "#262521", strokeWidth = 1, fill = "#121211", children = {
						{ type = "frame", name = "Action · Added seating note", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
							{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
								{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
									{ type = "icon", name = "Icon", width = 13, height = 13, icon = "pen-line", bw = 13, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
									{ type = "text", name = "Label", content = "Added seating note", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 108, bh = 16, fill = "#A3A199" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 245, bh = 1 },
								} },
							} },
							{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
								{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
							} },
						} },
						{ type = "frame", name = "Action · Approved by you · Book a table for 4", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
							{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
								{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
									{ type = "icon", name = "Icon", width = 13, height = 13, icon = "check", bw = 13, bh = 13, fill = "#C3CEB2" },
								} },
								{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
									{ type = "text", name = "Label", content = "Approved by you · Book a table for 4", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 199, bh = 16, fill = "#C3CEB2" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 116, bh = 1 },
									{ type = "text", name = "Meta", content = "20:01", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
								} },
							} },
							{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
								{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
							} },
						} },
						{ type = "frame", name = "Action · Completed reservation", width = "fill_container", layout = "vertical", bw = 384, bh = 34, children = {
							{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
								{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
									{ type = "icon", name = "Icon", width = 13, height = 13, icon = "mouse-pointer-click", bw = 13, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
									{ type = "text", name = "Label", content = "Completed reservation", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 125, bh = 16, fill = "#A3A199" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 142, bh = 1 },
									{ type = "text", name = "Meta", content = "opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 78, bh = 13, fill = "#6E6C65" },
								} },
							} },
							{ type = "frame", name = "Stem", width = "fill_container", padding = { 0, 0, 0, 6 }, bw = 384, bh = 12, children = {
								{ type = "frame", name = "Body", width = "fill_container", height = 12, layout = "vertical", gap = 6, padding = { 0, 0, 0, 17 }, strokeAlignment = "inner", bw = 378, bh = 12, stroke = "#3A3934", strokeWidth = { left = 1 } },
							} },
						} },
						{ type = "frame", name = "Action · Booking confirmed · #OT‑48213", width = "fill_container", layout = "vertical", bw = 384, bh = 22, children = {
							{ type = "frame", name = "Head", width = "fill_container", gap = 10, bw = 384, bh = 22, children = {
								{ type = "frame", name = "Marker", width = 13, height = 22, justifyContent = "center", alignItems = "center", bw = 13, bh = 22, children = {
									{ type = "icon", name = "Icon", width = 13, height = 13, icon = "circle-check", bw = 13, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Heading", width = "fill_container", height = 22, gap = 8, alignItems = "center", bw = 361, bh = 22, children = {
									{ type = "text", name = "Label", content = "Booking confirmed · #OT‑48213", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 171, bh = 16, fill = "#A3A199" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 182, bh = 1 },
								} },
							} },
						} },
					} } },
			},
		},
		{ name = "@ Reference picker · Searching", group = "Mentions & commands · menus",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 76, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 44, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", height = 19, alignItems = "center", bw = 264, bh = 19, children = {
								{ type = "text", name = "Typed", content = "Is there a table tonight at ", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 159, bh = 18, fill = "#EDEBE5" },
								{ type = "frame", name = "Mention draft", height = 19, padding = { 0, 3 }, cornerRadius = 4, alignItems = "center", bw = 42, bh = 19, fill = "#2A2926", children = {
									{ type = "text", name = "Mention query", content = "@ope", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 36, bh = 18, fill = "#EDEBE5" },
								} },
								{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Popover row", gap = 8, bw = 612, bh = 506, children = {
						{ type = "frame", name = "Preview anchor", padding = { 70, 0, 0, 0 }, bw = 208, bh = 245, children = {
							{ type = "frame", name = "Hover preview", width = 208, layout = "vertical", gap = 8, padding = 6, cornerRadius = 12, bw = 208, bh = 175, stroke = "#33322D", strokeWidth = 1, fill = "#1A1A18", shadows = {
									{ x = 0, y = 12, blur = 32, spread = 0, color = "#00000066", inner = false },
									{ x = 0, y = 2, blur = 6, spread = 0, color = "#00000040", inner = false },
								}, children = {
								{ type = "frame", name = "Page thumbnail", width = "fill_container", height = 116, cornerRadius = 8, bw = 196, bh = 116, img = "assets/photo-1603072995690-d88c14fd840d.jpg", fit = "cover" },
								{ type = "frame", name = "Preview text", width = "fill_container", layout = "vertical", gap = 4, padding = { 0, 6, 6, 6 }, bw = 196, bh = 39, children = {
									{ type = "text", name = "Preview title", width = "fill_container", content = "OpenTable · Le Bernardin", fontFamily = "Geist", fontSize = 12, fontWeight = "500", textGrowth = "fixed-width", bw = 184, bh = 16, fill = "#EDEBE5" },
									{ type = "text", name = "Preview url", content = "opentable.com/r/le-bernardin", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 168, bh = 13, fill = "#6E6C65" },
								} },
							} },
						} },
						{ type = "frame", name = "Reference picker", width = 396, layout = "vertical", cornerRadius = 14, bw = 396, bh = 506, stroke = "#33322D", strokeWidth = 1, fill = "#1A1A18", shadows = {
								{ x = 0, y = 12, blur = 32, spread = 0, color = "#00000066", inner = false },
								{ x = 0, y = 2, blur = 6, spread = 0, color = "#00000040", inner = false },
							}, children = {
							{ type = "frame", name = "Search header", width = "fill_container", gap = 8, padding = { 10, 12, 8, 12 }, alignItems = "center", bw = 396, bh = 35, children = {
								{ type = "icon", name = "At icon", width = 14, height = 14, icon = "at-sign", bw = 14, bh = 14, fill = "#A3A199" },
								{ type = "frame", name = "Query", gap = 1, alignItems = "center", bw = 25.5, bh = 17, children = {
									{ type = "text", name = "Query text", content = "ope", fontFamily = "Geist", fontSize = 13, fontWeight = "normal", bw = 23, bh = 17, fill = "#EDEBE5" },
									{ type = "rectangle", name = "Caret", width = 1.5, height = 15, bw = 1.5, bh = 15, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 254.5, bh = 1 },
								{ type = "text", name = "Result count", content = "7 results", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 54, bh = 13, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Scopes", width = "fill_container", gap = 4, padding = { 0, 10, 8, 10 }, bw = 396, bh = 30, children = {
								{ type = "frame", name = "Scope · All", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 32, bh = 22, fill = "#EDEBE5", children = {
									{ type = "text", name = "Scope label", content = "All", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 14, bh = 14, fill = "#121211" },
								} },
								{ type = "frame", name = "Scope · Tabs", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 43, bh = 22, stroke = "#2A2925", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "text", name = "Scope label", content = "Tabs", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 25, bh = 14, fill = "#C9C6BE" },
								} },
								{ type = "frame", name = "Scope · Clipboard", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 69, bh = 22, stroke = "#2A2925", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "text", name = "Scope label", content = "Clipboard", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 51, bh = 14, fill = "#C9C6BE" },
								} },
								{ type = "frame", name = "Scope · History", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 55, bh = 22, stroke = "#2A2925", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "text", name = "Scope label", content = "History", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 37, bh = 14, fill = "#C9C6BE" },
								} },
								{ type = "frame", name = "Scope · Files", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 43, bh = 22, stroke = "#2A2925", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "text", name = "Scope label", content = "Files", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 25, bh = 14, fill = "#C9C6BE" },
								} },
								{ type = "frame", name = "Scope · Bookmarks", height = 22, gap = 5, padding = { 0, 9 }, cornerRadius = 11, alignItems = "center", bw = 77, bh = 22, stroke = "#2A2925", strokeWidth = 1, fill = "#1F1F1D", children = {
									{ type = "text", name = "Scope label", content = "Bookmarks", fontFamily = "Geist", fontSize = 11, fontWeight = "500", bw = 59, bh = 14, fill = "#C9C6BE" },
								} },
							} },
							{ type = "rectangle", name = "Divider", width = "fill_container", height = 1, bw = 396, bh = 1, fill = "#262521" },
							{ type = "frame", name = "Results", width = "fill_container", layout = "vertical", gap = 2, padding = 6, bw = 396, bh = 404, children = {
								{ type = "frame", name = "Group · Open tabs", width = "fill_container", padding = { 6, 8, 4, 8 }, bw = 384, bh = 23, children = {
									{ type = "text", name = "Group label", content = "Open tabs", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 54, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Result · OpenTable · Le Bernardin", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, fill = "#262624", children = {
									{ type = "frame", name = "Favicon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#DA3743", children = {
										{ type = "text", name = "Favicon letter", content = "O", fontFamily = "Geist", fontSize = 10, fontWeight = "600", lineHeight = 1, bw = 8, bh = 10, fill = "#FFFFFF" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 278.5, bh = 30, children = {
										{ type = "frame", name = "Title", bw = 144, bh = 16, children = {
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "nTable · Le Bernardin", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 119, bh = 16, fill = "#EDEBE5" },
										} },
										{ type = "text", name = "Source", content = "opentable.com/r/le-bernardin", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 168, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "Tab", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 18, bh = 14, fill = "#6E6C65" },
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 23.5, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 2, alignItems = "center", bw = 11.5, bh = 14, children = {
											{ type = "icon", name = "Key corner-down-left", width = 11.5, height = 11.5, icon = "corner-down-left", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										} },
									} },
								} },
								{ type = "frame", name = "Result · Open Kitchen — Dinner menu", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Favicon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#E9E3D3", children = {
										{ type = "text", name = "Favicon letter", content = "K", fontFamily = "Geist", fontSize = 10, fontWeight = "600", lineHeight = 1, bw = 7, bh = 10, fill = "#2B2A26" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 312, bh = 30, children = {
										{ type = "frame", name = "Title", bw = 169, bh = 16, children = {
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "n Kitchen — Dinner menu", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 144, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "openkitchen.nyc/menu", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 120, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "Tab", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 18, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Group · Clipboard", width = "fill_container", gap = 6, padding = { 6, 8, 4, 8 }, alignItems = "center", bw = 384, bh = 23, children = {
									{ type = "text", name = "Group label", content = "Clipboard", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 54, bh = 13, fill = "#6E6C65" },
									{ type = "text", name = "Group note", content = "last 3", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#4A4943" },
								} },
								{ type = "frame", name = "Result · Clipboard · “Tables are held 15 min — OpenTable may cancel after…”", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Kind icon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#262523", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "clipboard-type", bw = 11, bh = 11, fill = "#A3A199" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 303, bh = 30, children = {
										{ type = "frame", name = "Title", width = "fill_container", clip = true, bw = 303, bh = 16, children = {
											{ type = "text", name = "Rest", content = "“Held 15 min, then ", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 107, bh = 16, fill = "#C9C6BE" },
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "nTable releases it”", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 104, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "Text · copied from opentable.com", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 192, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "2 min", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 27, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Result · Clipboard · Screenshot · Open Kitchen floor plan", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Clipboard image", width = 18, height = 18, cornerRadius = 4, bw = 18, bh = 18, stroke = "#3A3934", strokeWidth = 1, img = "assets/photo-1584804746491-7413be6999d4.jpg", fit = "cover" },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 302, bh = 30, children = {
										{ type = "frame", name = "Title", width = "fill_container", clip = true, bw = 302, bh = 16, children = {
											{ type = "text", name = "Rest", content = "Screenshot · ", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 74, bh = 16, fill = "#C9C6BE" },
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "n Kitchen floor plan", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 112, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "Image · 1280×820", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 96, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "9 min", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 28, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Group · History", width = "fill_container", padding = { 6, 8, 4, 8 }, bw = 384, bh = 23, children = {
									{ type = "text", name = "Group label", content = "History", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 42, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Result · OpenTable · Carbone", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Kind icon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#262523", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "history", bw = 11, bh = 11, fill = "#A3A199" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 283, bh = 30, children = {
										{ type = "frame", name = "Title", bw = 120, bh = 16, children = {
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "nTable · Carbone", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 95, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "opentable.com/r/carbone-new-york", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 192, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "Yesterday", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 47, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Group · Files & bookmarks", width = "fill_container", padding = { 6, 8, 4, 8 }, bw = 384, bh = 23, children = {
									{ type = "text", name = "Group label", content = "Files & bookmarks", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 102, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Result · Opening hours.pdf", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Kind icon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#262523", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "file-text", bw = 11, bh = 11, fill = "#A3A199" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 310, bh = 30, children = {
										{ type = "frame", name = "Title", bw = 107, bh = 16, children = {
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "ning hours.pdf", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 82, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "Downloads · 240 KB", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 108, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "PDF", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 20, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Result · Open data · NYC restaurant grades", width = "fill_container", height = 40, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 384, bh = 40, children = {
									{ type = "frame", name = "Kind icon", width = 18, height = 18, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 18, bh = 18, fill = "#262523", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "bookmark", bw = 11, bh = 11, fill = "#A3A199" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 1, bw = 280, bh = 30, children = {
										{ type = "frame", name = "Title", bw = 199, bh = 16, children = {
											{ type = "text", name = "Match", content = "Ope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "600", bw = 25, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "n data · NYC restaurant grades", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 174, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Source", content = "data.cityofnewyork.us", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 126, bh = 13, fill = "#6E6C65" },
									} },
									{ type = "text", name = "Meta", content = "Bookmark", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 50, bh = 14, fill = "#6E6C65" },
								} },
							} },
							{ type = "frame", name = "Footer", width = "fill_container", gap = 12, padding = { 8, 12 }, alignItems = "center", bw = 396, bh = 36, stroke = "#262521", strokeWidth = { top = 1 }, children = {
								{ type = "frame", name = "Hint · Navigate", gap = 5, alignItems = "center", bw = 87, bh = 20, children = {
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 39, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 27, bh = 14, children = {
											{ type = "icon", name = "Key arrow-up", width = 11.5, height = 11.5, icon = "arrow-up", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "icon", name = "Key arrow-down", width = 11.5, height = 11.5, icon = "arrow-down", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										} },
									} },
									{ type = "text", name = "Hint label", content = "Navigate", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 43, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Hint · Insert", gap = 5, alignItems = "center", bw = 56.5, bh = 20, children = {
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 23.5, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 2, alignItems = "center", bw = 11.5, bh = 14, children = {
											{ type = "icon", name = "Key corner-down-left", width = 11.5, height = 11.5, icon = "corner-down-left", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										} },
									} },
									{ type = "text", name = "Hint label", content = "Insert", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 28, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Hint · Scope", gap = 5, alignItems = "center", bw = 59.5, bh = 20, children = {
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 23.5, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 2, alignItems = "center", bw = 11.5, bh = 14, children = {
											{ type = "icon", name = "Key arrow-right-to-line", width = 11.5, height = 11.5, icon = "arrow-right-to-line", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										} },
									} },
									{ type = "text", name = "Hint label", content = "Scope", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 31, bh = 14, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 31, bh = 1 },
								{ type = "frame", name = "Hint · Snapshot", gap = 5, alignItems = "center", bw = 90, bh = 20, children = {
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 39, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 27, bh = 14, children = {
											{ type = "icon", name = "Key command", width = 11.5, height = 11.5, icon = "command", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "icon", name = "Key corner-down-left", width = 11.5, height = 11.5, icon = "corner-down-left", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										} },
									} },
									{ type = "text", name = "Hint label", content = "Snapshot", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 46, bh = 14, fill = "#6E6C65" },
								} },
							} },
						} },
					} } },
			},
		},
		{ name = "/ Command palette · Searching", group = "Mentions & commands · menus",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 76, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 44, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 44, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 32, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", height = 19, gap = 1, alignItems = "center", bw = 264, bh = 19, children = {
								{ type = "text", name = "Command draft", content = "/su", fontFamily = "Geist Mono", fontSize = 13, fontWeight = "normal", bw = 24, bh = 17, fill = "#EDEBE5" },
								{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Command palette", width = 600, layout = "vertical", cornerRadius = 14, clip = true, bw = 600, bh = 343, stroke = "#33322D", strokeWidth = 1, fill = "#1A1A18", shadows = {
							{ x = 0, y = 12, blur = 32, spread = 0, color = "#00000066", inner = false },
							{ x = 0, y = 2, blur = 6, spread = 0, color = "#00000040", inner = false },
						}, children = {
						{ type = "frame", name = "Search header", width = "fill_container", gap = 10, padding = { 10, 12 }, alignItems = "center", bw = 600, bh = 42, stroke = "#262521", strokeWidth = { bottom = 1 }, children = {
							{ type = "frame", name = "Slash key", width = 20, height = 20, cornerRadius = 5, justifyContent = "center", alignItems = "center", bw = 20, bh = 20, fill = "#EDEBE5", children = {
								{ type = "text", name = "Slash", content = "/", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "600", lineHeight = 1, bw = 8, bh = 12, fill = "#121211" },
							} },
							{ type = "frame", name = "Query", gap = 1, alignItems = "center", bw = 18.5, bh = 17, children = {
								{ type = "text", name = "Query text", content = "su", fontFamily = "Geist Mono", fontSize = 13, fontWeight = "normal", bw = 16, bh = 17, fill = "#EDEBE5" },
								{ type = "rectangle", name = "Caret", width = 1.5, height = 15, bw = 1.5, bh = 15, fill = "#EDEBE5" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 355.5, bh = 1 },
							{ type = "frame", name = "Filter", height = 22, gap = 2, padding = 2, cornerRadius = 6, bw = 152, bh = 22, stroke = "#262521", strokeWidth = 1, fill = "#121211", children = {
								{ type = "frame", name = "Filter · All", height = "fill_container", padding = { 0, 7 }, cornerRadius = 4, alignItems = "center", bw = 32, bh = 18, fill = "#2A2926", children = {
									{ type = "text", name = "Filter label", content = "All", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 18, bh = 13, fill = "#EDEBE5" },
								} },
								{ type = "frame", name = "Filter · Commands", height = "fill_container", padding = { 0, 7 }, cornerRadius = 4, alignItems = "center", bw = 62, bh = 18, children = {
									{ type = "text", name = "Filter label", content = "Commands", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 48, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Filter · Skills", height = "fill_container", padding = { 0, 7 }, cornerRadius = 4, alignItems = "center", bw = 50, bh = 18, children = {
									{ type = "text", name = "Filter label", content = "Skills", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
								} },
							} },
						} },
						{ type = "frame", name = "Body", width = "fill_container", bw = 600, bh = 265, children = {
							{ type = "frame", name = "Detail", width = 232, layout = "vertical", gap = 14, padding = 14, bw = 232, bh = 265, stroke = "#262521", strokeWidth = { right = 1 }, fill = "#151514", children = {
								{ type = "frame", name = "Detail header", gap = 8, alignItems = "center", bw = 101, bh = 24, children = {
									{ type = "frame", name = "Command icon", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, stroke = "#2E2D29", strokeWidth = 1, fill = "#1F1F1D", children = {
										{ type = "icon", name = "Icon", width = 12, height = 12, icon = "text-quote", bw = 12, bh = 12, fill = "#C9C6BE" },
									} },
									{ type = "text", name = "Detail title", content = "Summarize", fontFamily = "Geist", fontSize = 13, fontWeight = "500", bw = 69, bh = 17, fill = "#EDEBE5" },
								} },
								{ type = "text", name = "Detail description", width = "fill_container", content = "Condenses what you point it at into key points, with links back to the source.", fontFamily = "Geist", fontSize = 11.5, fontWeight = "normal", lineHeight = 1.45, textGrowth = "fixed-width", bw = 204, bh = 51, fill = "#A3A199" },
								{ type = "frame", name = "Usage", width = "fill_container", layout = "vertical", gap = 6, bw = 204, bh = 47, children = {
									{ type = "text", name = "Usage label", content = "Usage", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
									{ type = "frame", name = "Usage code", width = "fill_container", padding = { 7, 8 }, cornerRadius = 6, bw = 204, bh = 28, stroke = "#222220", strokeWidth = 1, fill = "#0F0F0E", children = {
										{ type = "text", name = "Usage text", content = "/summarize [scope] [focus]", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", bw = 164, bh = 14, fill = "#C9C6BE" },
									} },
								} },
								{ type = "frame", name = "Arguments", width = "fill_container", layout = "vertical", gap = 6, bw = 204, bh = 73, children = {
									{ type = "text", name = "Arguments label", content = "Scope", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 30, bh = 13, fill = "#6E6C65" },
									{ type = "frame", name = "Arg · page", width = "fill_container", gap = 8, alignItems = "center", bw = 204, bh = 14, children = {
										{ type = "text", name = "Arg name", content = "page", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", bw = 26, bh = 14, fill = "#EDEBE5" },
										{ type = "text", name = "Arg description", content = "Current tab", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 58, bh = 14, fill = "#858379" },
										{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 56, bh = 1 },
										{ type = "text", name = "Default", content = "default", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "normal", bw = 40, bh = 12, fill = "#6E6C65" },
									} },
									{ type = "frame", name = "Arg · selection", width = "fill_container", gap = 8, alignItems = "center", bw = 204, bh = 14, children = {
										{ type = "text", name = "Arg name", content = "selection", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", bw = 57, bh = 14, fill = "#EDEBE5" },
										{ type = "text", name = "Arg description", content = "Highlighted text", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 80, bh = 14, fill = "#858379" },
									} },
									{ type = "frame", name = "Arg · @tabs", width = "fill_container", gap = 8, alignItems = "center", bw = 204, bh = 14, children = {
										{ type = "text", name = "Arg name", content = "@tabs", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", bw = 32, bh = 14, fill = "#EDEBE5" },
										{ type = "text", name = "Arg description", content = "Any tabs you mention", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 109, bh = 14, fill = "#858379" },
									} },
								} },
							} },
							{ type = "frame", name = "Results", width = "fill_container", height = "fill_container", layout = "vertical", gap = 1, padding = 6, bw = 368, bh = 265, children = {
								{ type = "frame", name = "Group · Commands", width = "fill_container", gap = 6, padding = { 6, 8, 4, 8 }, bw = 356, bh = 23, children = {
									{ type = "text", name = "Group label", content = "Commands", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 48, bh = 13, fill = "#6E6C65" },
									{ type = "text", name = "Group count", content = "2", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 6, bh = 13, fill = "#4A4943" },
								} },
								{ type = "frame", name = "Command · /summarize", width = "fill_container", height = 44, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 356, bh = 44, fill = "#262624", children = {
									{ type = "frame", name = "Command icon", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, stroke = "#2E2D29", strokeWidth = 1, fill = "#1F1F1D", children = {
										{ type = "icon", name = "Icon", width = 12, height = 12, icon = "text-quote", bw = 12, bh = 12, fill = "#C9C6BE" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 2, bw = 246, bh = 32, children = {
										{ type = "frame", name = "Name", bw = 74, bh = 16, children = {
											{ type = "text", name = "Slash", content = "/", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 8, bh = 16, fill = "#6E6C65" },
											{ type = "text", name = "Match", content = "su", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "600", bw = 15, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "mmarize", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 51, bh = 16, fill = "#EDEBE5" },
										} },
										{ type = "text", name = "Description", content = "Key points from a page, selection or tabs", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 205, bh = 14, fill = "#858379" },
									} },
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 50, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 38, bh = 14, children = {
											{ type = "icon", name = "Key command", width = 11.5, height = 11.5, icon = "command", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "icon", name = "Key arrow-big-up", width = 11.5, height = 11.5, icon = "arrow-big-up", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "text", name = "Key text", content = "S", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
										} },
									} },
								} },
								{ type = "frame", name = "Command · /suggest", width = "fill_container", height = 44, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 356, bh = 44, children = {
									{ type = "frame", name = "Command icon", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, stroke = "#2E2D29", strokeWidth = 1, fill = "#1F1F1D", children = {
										{ type = "icon", name = "Icon", width = 12, height = 12, icon = "message-square-reply", bw = 12, bh = 12, fill = "#C9C6BE" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 2, bw = 246, bh = 32, children = {
										{ type = "frame", name = "Name", bw = 60, bh = 16, children = {
											{ type = "text", name = "Slash", content = "/", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 8, bh = 16, fill = "#6E6C65" },
											{ type = "text", name = "Match", content = "su", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "600", bw = 15, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "ggest", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 37, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Description", content = "Draft a reply or next steps for this page", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 197, bh = 14, fill = "#858379" },
									} },
									{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 50, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
										{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 38, bh = 14, children = {
											{ type = "icon", name = "Key command", width = 11.5, height = 11.5, icon = "command", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "icon", name = "Key arrow-big-up", width = 11.5, height = 11.5, icon = "arrow-big-up", bw = 11.5, bh = 11.5, fill = "#A3A199" },
											{ type = "text", name = "Key text", content = "R", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#A3A199" },
										} },
									} },
								} },
								{ type = "frame", name = "Group · Skills", width = "fill_container", gap = 6, padding = { 6, 8, 4, 8 }, bw = 356, bh = 23, children = {
									{ type = "text", name = "Group label", content = "Skills", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 36, bh = 13, fill = "#6E6C65" },
									{ type = "text", name = "Group count", content = "2", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 6, bh = 13, fill = "#4A4943" },
								} },
								{ type = "frame", name = "Command · /supermarket-run", width = "fill_container", height = 44, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 356, bh = 44, children = {
									{ type = "frame", name = "Command icon", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, stroke = "#4D4130", strokeWidth = 1, fill = "#1F1B14", children = {
										{ type = "icon", name = "Icon", width = 12, height = 12, icon = "shopping-basket", bw = 12, bh = 12, fill = "#E2C98F" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 2, bw = 242, bh = 32, children = {
										{ type = "frame", name = "Name", bw = 117, bh = 16, children = {
											{ type = "text", name = "Slash", content = "/", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 8, bh = 16, fill = "#6E6C65" },
											{ type = "text", name = "Match", content = "su", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "600", bw = 15, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "permarket-run", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 94, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Description", content = "Turn a grocery list into a delivery cart", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 186, bh = 14, fill = "#858379" },
									} },
									{ type = "text", name = "Source", content = "instacart", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 54, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Command · /sunday-planner", width = "fill_container", height = 44, gap = 10, padding = { 0, 8 }, cornerRadius = 8, alignItems = "center", bw = 356, bh = 44, children = {
									{ type = "frame", name = "Command icon", width = 24, height = 24, cornerRadius = 6, justifyContent = "center", alignItems = "center", bw = 24, bh = 24, stroke = "#4D4130", strokeWidth = 1, fill = "#1F1B14", children = {
										{ type = "icon", name = "Icon", width = 12, height = 12, icon = "calendar-range", bw = 12, bh = 12, fill = "#E2C98F" },
									} },
									{ type = "frame", name = "Labels", width = "fill_container", layout = "vertical", gap = 2, bw = 248, bh = 32, children = {
										{ type = "frame", name = "Name", bw = 110, bh = 16, children = {
											{ type = "text", name = "Slash", content = "/", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 8, bh = 16, fill = "#6E6C65" },
											{ type = "text", name = "Match", content = "su", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "600", bw = 15, bh = 16, fill = "#EDEBE5" },
											{ type = "text", name = "Rest", content = "nday-planner", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "normal", bw = 87, bh = 16, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Description", content = "Plan the week around your calendar", fontFamily = "Geist", fontSize = 11, fontWeight = "normal", bw = 182, bh = 14, fill = "#858379" },
									} },
									{ type = "text", name = "Source", content = "personal", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 48, bh = 13, fill = "#6E6C65" },
								} },
							} },
						} },
						{ type = "frame", name = "Footer", width = "fill_container", gap = 12, padding = { 8, 12 }, alignItems = "center", bw = 600, bh = 36, stroke = "#262521", strokeWidth = { top = 1 }, children = {
							{ type = "frame", name = "Hint · Run", gap = 5, alignItems = "center", bw = 48.5, bh = 20, children = {
								{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 23.5, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 2, alignItems = "center", bw = 11.5, bh = 14, children = {
										{ type = "icon", name = "Key corner-down-left", width = 11.5, height = 11.5, icon = "corner-down-left", bw = 11.5, bh = 11.5, fill = "#A3A199" },
									} },
								} },
								{ type = "text", name = "Hint label", content = "Run", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 20, bh = 14, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Hint · Add arguments", gap = 5, alignItems = "center", bw = 102.5, bh = 20, children = {
								{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 23.5, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 2, alignItems = "center", bw = 11.5, bh = 14, children = {
										{ type = "icon", name = "Key arrow-right-to-line", width = 11.5, height = 11.5, icon = "arrow-right-to-line", bw = 11.5, bh = 11.5, fill = "#A3A199" },
									} },
								} },
								{ type = "text", name = "Hint label", content = "Add arguments", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 74, bh = 14, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Hint · Navigate", gap = 5, alignItems = "center", bw = 87, bh = 20, children = {
								{ type = "frame", name = "Kbd", height = 20, padding = { 0, 6 }, cornerRadius = 4, alignItems = "center", bw = 39, bh = 20, stroke = "#33322D", strokeWidth = 1, children = {
									{ type = "frame", name = "Kbd label · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 27, bh = 14, children = {
										{ type = "icon", name = "Key arrow-up", width = 11.5, height = 11.5, icon = "arrow-up", bw = 11.5, bh = 11.5, fill = "#A3A199" },
										{ type = "icon", name = "Key arrow-down", width = 11.5, height = 11.5, icon = "arrow-down", bw = 11.5, bh = 11.5, fill = "#A3A199" },
									} },
								} },
								{ type = "text", name = "Hint label", content = "Navigate", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 43, bh = 14, fill = "#6E6C65" },
							} },
							{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 208, bh = 1 },
							{ type = "frame", name = "Manage skills", gap = 5, alignItems = "center", bw = 82, bh = 14, children = {
								{ type = "icon", name = "Icon", width = 11, height = 11, icon = "settings-2", bw = 11, bh = 11, fill = "#858379" },
								{ type = "text", name = "Manage label", content = "Manage skills", fontFamily = "Geist", fontSize = 10.5, fontWeight = "normal", bw = 66, bh = 14, fill = "#A3A199" },
							} },
						} },
					} } },
			},
		},
		{ name = "@ Mention · Hover tooltip", group = "Mentions & commands · selected tokens",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 88, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 56, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 56, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 44, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", layout = "vertical", gap = 4, bw = 264, bh = 44, children = {
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 163, bh = 20, children = {
									{ type = "text", name = "Text", content = "Compare", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 57, bh = 18, fill = "#EDEBE5" },
									{ type = "frame", name = "Mention · Le Bernardin", height = 20, gap = 5, padding = { 0, 6, 0, 3 }, cornerRadius = 5, alignItems = "center", bw = 102, bh = 20, stroke = "#5A5850", strokeWidth = 1, fill = "#33322D", children = {
										{ type = "frame", name = "Favicon", width = 14, height = 14, cornerRadius = 4, justifyContent = "center", alignItems = "center", bw = 14, bh = 14, fill = "#DA3743", children = {
											{ type = "text", name = "Favicon letter", content = "O", fontFamily = "Geist", fontSize = 8.5, fontWeight = "600", lineHeight = 1, bw = 7, bh = 9, fill = "#FFFFFF" },
										} },
										{ type = "text", name = "Mention label", content = "Le Bernardin", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 74, bh = 16, fill = "#EDEBE5" },
									} },
								} },
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 235.5, bh = 20, children = {
									{ type = "text", name = "Text", content = "with", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 27, bh = 18, fill = "#EDEBE5" },
									{ type = "frame", name = "Mention · Opening hours.pdf", height = 20, gap = 5, padding = { 0, 6, 0, 3 }, cornerRadius = 5, alignItems = "center", bw = 136, bh = 20, stroke = "#3A3934", strokeWidth = 1, fill = "#262523", children = {
										{ type = "frame", name = "Kind icon", width = 14, height = 14, cornerRadius = 4, justifyContent = "center", alignItems = "center", bw = 14, bh = 14, fill = "#3A3934", children = {
											{ type = "icon", name = "Icon", width = 9, height = 9, icon = "file-text", bw = 9, bh = 9, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Mention label", content = "Opening hours.pdf", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 108, bh = 16, fill = "#EDEBE5" },
									} },
									{ type = "text", name = "Text", content = "for Friday", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 59, bh = 18, fill = "#EDEBE5" },
									{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Tooltip anchor", padding = { 0, 0, 0, 58 }, bw = 235.5, bh = 26, children = {
						{ type = "frame", name = "Tooltip · Live tab · opentable.com", height = 26, gap = 8, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 177.5, bh = 26, fill = "#EDEBE5", children = {
							{ type = "text", name = "Tooltip label", content = "Live tab · opentable.com", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 131, bh = 15, fill = "#121211" },
							{ type = "frame", name = "Tooltip key · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 22.5, bh = 14, children = {
								{ type = "icon", name = "Key command", width = 11.5, height = 11.5, icon = "command", bw = 11.5, bh = 11.5, fill = "#12121199" },
								{ type = "text", name = "Key text S", content = "S", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#12121199" },
							} },
						} },
					} } },
			},
		},
		{ name = "@ Mention · Context menu", group = "Mentions & commands · selected tokens",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 88, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 56, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 56, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 44, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", layout = "vertical", gap = 4, bw = 264, bh = 44, children = {
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 163, bh = 20, children = {
									{ type = "text", name = "Text", content = "Compare", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 57, bh = 18, fill = "#EDEBE5" },
									{ type = "frame", name = "Mention · Le Bernardin", height = 20, gap = 5, padding = { 0, 6, 0, 3 }, cornerRadius = 5, alignItems = "center", bw = 102, bh = 20, stroke = "#5A5850", strokeWidth = 1, fill = "#33322D", children = {
										{ type = "frame", name = "Favicon", width = 14, height = 14, cornerRadius = 4, justifyContent = "center", alignItems = "center", bw = 14, bh = 14, fill = "#DA3743", children = {
											{ type = "text", name = "Favicon letter", content = "O", fontFamily = "Geist", fontSize = 8.5, fontWeight = "600", lineHeight = 1, bw = 7, bh = 9, fill = "#FFFFFF" },
										} },
										{ type = "text", name = "Mention label", content = "Le Bernardin", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 74, bh = 16, fill = "#EDEBE5" },
									} },
								} },
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 235.5, bh = 20, children = {
									{ type = "text", name = "Text", content = "with", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 27, bh = 18, fill = "#EDEBE5" },
									{ type = "frame", name = "Mention · Opening hours.pdf", height = 20, gap = 5, padding = { 0, 6, 0, 3 }, cornerRadius = 5, alignItems = "center", bw = 136, bh = 20, stroke = "#3A3934", strokeWidth = 1, fill = "#262523", children = {
										{ type = "frame", name = "Kind icon", width = 14, height = 14, cornerRadius = 4, justifyContent = "center", alignItems = "center", bw = 14, bh = 14, fill = "#3A3934", children = {
											{ type = "icon", name = "Icon", width = 9, height = 9, icon = "file-text", bw = 9, bh = 9, fill = "#C9C6BE" },
										} },
										{ type = "text", name = "Mention label", content = "Opening hours.pdf", fontFamily = "Geist", fontSize = 12.5, fontWeight = "500", bw = 108, bh = 16, fill = "#EDEBE5" },
									} },
									{ type = "text", name = "Text", content = "for Friday", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 59, bh = 18, fill = "#EDEBE5" },
									{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Context menu anchor", padding = { 0, 0, 0, 104 }, bw = 348, bh = 227, children = {
						{ type = "frame", name = "Context menu · @ Mention", width = 244, layout = "vertical", padding = 4, cornerRadius = 10, bw = 244, bh = 227, stroke = "#33322D", strokeWidth = 1, fill = "#1A1A18", shadows = {
								{ x = 0, y = 12, blur = 28, spread = 0, color = "#00000066", inner = false },
								{ x = 0, y = 2, blur = 6, spread = 0, color = "#00000040", inner = false },
							}, children = {
							{ type = "frame", name = "Items", width = "fill_container", layout = "vertical", gap = 1, bw = 236, bh = 219, children = {
								{ type = "frame", name = "Header", width = "fill_container", gap = 6, padding = { 6, 8, 4, 8 }, alignItems = "center", bw = 236, bh = 23, children = {
									{ type = "text", name = "Header label", content = "Le Bernardin · Live tab", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 138, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Item · Open tab", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, fill = "#262624", children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "external-link", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Open tab", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 53, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 101, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 25, bh = 14, children = {
										{ type = "icon", name = "Key icon 1", width = 11, height = 11, icon = "command", bw = 11, bh = 11, fill = "#6E6C65" },
										{ type = "icon", name = "Key icon 2", width = 11, height = 11, icon = "corner-down-left", bw = 11, bh = 11, fill = "#6E6C65" },
									} },
								} },
								{ type = "frame", name = "Item · Quick look", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "eye", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Quick look", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 62, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 85, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 32, bh = 14, children = {
										{ type = "text", name = "Key text", content = "Space", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 32, bh = 11, fill = "#6E6C65" },
									} },
								} },
								{ type = "frame", name = "Item · Freeze as snapshot", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "camera", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Freeze as snapshot", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 111, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 77, bh = 1 },
								} },
								{ type = "frame", name = "Item · Replace with…", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "replace", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Replace with…", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 82, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 85, bh = 1 },
									{ type = "icon", name = "Submenu", width = 12, height = 12, icon = "chevron-right", bw = 12, bh = 12, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Item · Copy link", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "link", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Copy link", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 54, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 104, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 21, bh = 14, children = {
										{ type = "icon", name = "Key icon 1", width = 11, height = 11, icon = "command", bw = 11, bh = 11, fill = "#6E6C65" },
										{ type = "text", name = "Key text", content = "C", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#6E6C65" },
									} },
								} },
								{ type = "frame", name = "Separator", width = "fill_container", layout = "vertical", padding = { 4, 0 }, bw = 236, bh = 9, children = {
									{ type = "rectangle", name = "Line", width = "fill_container", height = 1, bw = 236, bh = 1, fill = "#262521" },
								} },
								{ type = "frame", name = "Item · Remove mention", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#E8A898" },
									{ type = "text", name = "Item label", content = "Remove mention", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 96, bh = 16, fill = "#E8A898" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 72, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 11, bh = 14, children = {
										{ type = "icon", name = "Key icon 1", width = 11, height = 11, icon = "delete", bw = 11, bh = 11, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
					} } },
			},
		},
		{ name = "/ Command · Hover tooltip", group = "Mentions & commands · selected tokens",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 88, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 56, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 56, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 44, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", layout = "vertical", gap = 4, bw = 264, bh = 44, children = {
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 171, bh = 20, children = {
									{ type = "frame", name = "Command · /summarize", height = 20, gap = 5, padding = { 0, 6, 0, 5 }, cornerRadius = 5, alignItems = "center", bw = 100, bh = 20, stroke = "#FFFFFF", strokeWidth = 2, fill = "#EDEBE5", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "text-quote", bw = 11, bh = 11, fill = "#121211" },
										{ type = "text", name = "Command label", content = "/summarize", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "500", bw = 73, bh = 16, fill = "#121211" },
									} },
									{ type = "frame", name = "Argument · scope", height = 20, gap = 4, padding = { 0, 4, 0, 6 }, cornerRadius = 5, alignItems = "center", bw = 67, bh = 20, stroke = "#3A3934", strokeWidth = 1, children = {
										{ type = "text", name = "Argument label", content = "2 tabs", fontFamily = "Geist Mono", fontSize = 11.5, fontWeight = "normal", bw = 42, bh = 15, fill = "#C9C6BE" },
										{ type = "icon", name = "Chevron", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#858379" },
									} },
								} },
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 171.5, bh = 20, children = {
									{ type = "text", name = "Text", content = "focus on cancellation rules", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 166, bh = 18, fill = "#EDEBE5" },
									{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Tooltip anchor", padding = { 0, 0, 0, 46 }, bw = 272.5, bh = 26, children = {
						{ type = "frame", name = "Tooltip · Summarize 2 tabs into key points", height = 26, gap = 8, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 226.5, bh = 26, fill = "#EDEBE5", children = {
							{ type = "text", name = "Tooltip label", content = "Summarize 2 tabs into key points", fontFamily = "Geist", fontSize = 11.5, fontWeight = "500", bw = 180, bh = 15, fill = "#121211" },
							{ type = "frame", name = "Tooltip key · aligned keys", height = 14, gap = 4, alignItems = "center", bw = 22.5, bh = 14, children = {
								{ type = "icon", name = "Key command", width = 11.5, height = 11.5, icon = "command", bw = 11.5, bh = 11.5, fill = "#12121199" },
								{ type = "text", name = "Key text S", content = "S", fontFamily = "Geist Mono", fontSize = 10.5, fontWeight = "normal", lineHeight = 1, bw = 7, bh = 11, fill = "#12121199" },
							} },
						} },
					} } },
			},
		},
		{ name = "/ Command · Context menu", group = "Mentions & commands · selected tokens",
			composer = { type = "frame", name = "Composer", width = 396, layout = "vertical", gap = 6, bw = 396, bh = 88, children = {
				{ type = "frame", name = "Composer body · Attached docks", width = "fill_container", layout = "vertical", bw = 396, bh = 56, children = {
					{ type = "frame", name = "Input box", width = "fill_container", layout = "vertical", gap = 8, padding = 6, cornerRadius = 22, bw = 396, bh = 56, stroke = "#2E2D29", strokeWidth = 1, fill = "#1C1C1A", children = {
						{ type = "frame", name = "Input row", width = "fill_container", gap = 8, alignItems = "center", bw = 384, bh = 44, children = {
							{ type = "frame", name = "Add", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#2A2926", children = {
								{ type = "icon", name = "Add icon", width = 16, height = 16, icon = "plus", bw = 16, bh = 16, fill = "#C9C6BE" },
							} },
							{ type = "frame", name = "Input content", width = "fill_container", layout = "vertical", gap = 4, bw = 264, bh = 44, children = {
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 171, bh = 20, children = {
									{ type = "frame", name = "Command · /summarize", height = 20, gap = 5, padding = { 0, 6, 0, 5 }, cornerRadius = 5, alignItems = "center", bw = 100, bh = 20, stroke = "#FFFFFF", strokeWidth = 2, fill = "#EDEBE5", children = {
										{ type = "icon", name = "Icon", width = 11, height = 11, icon = "text-quote", bw = 11, bh = 11, fill = "#121211" },
										{ type = "text", name = "Command label", content = "/summarize", fontFamily = "Geist Mono", fontSize = 12, fontWeight = "500", bw = 73, bh = 16, fill = "#121211" },
									} },
									{ type = "frame", name = "Argument · scope", height = 20, gap = 4, padding = { 0, 4, 0, 6 }, cornerRadius = 5, alignItems = "center", bw = 67, bh = 20, stroke = "#3A3934", strokeWidth = 1, children = {
										{ type = "text", name = "Argument label", content = "2 tabs", fontFamily = "Geist Mono", fontSize = 11.5, fontWeight = "normal", bw = 42, bh = 15, fill = "#C9C6BE" },
										{ type = "icon", name = "Chevron", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#858379" },
									} },
								} },
								{ type = "frame", name = "Line", height = 20, gap = 4, alignItems = "center", bw = 171.5, bh = 20, children = {
									{ type = "text", name = "Text", content = "focus on cancellation rules", fontFamily = "Geist", fontSize = 13.5, fontWeight = "normal", bw = 166, bh = 18, fill = "#EDEBE5" },
									{ type = "rectangle", name = "Caret", width = 1.5, height = 16, bw = 1.5, bh = 16, fill = "#EDEBE5" },
								} },
							} },
							{ type = "frame", name = "Voice input", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, children = {
								{ type = "icon", name = "Microphone", width = 15, height = 15, icon = "mic", bw = 15, bh = 15, fill = "#A3A199" },
							} },
							{ type = "frame", name = "Send", width = 32, height = 32, cornerRadius = 16, justifyContent = "center", alignItems = "center", bw = 32, bh = 32, fill = "#EDEBE5", children = {
								{ type = "icon", name = "Send icon", width = 16, height = 16, icon = "arrow-up", bw = 16, bh = 16, fill = "#121211" },
							} },
						} },
					} },
				} },
				{ type = "frame", name = "Composer footer", width = "fill_container", padding = { 0, 4 }, alignItems = "center", bw = 396, bh = 26, children = {
					{ type = "frame", name = "Tools", height = 26, gap = 4, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 45, bh = 26, children = {
						{ type = "icon", name = "Tools icon", width = 14, height = 14, icon = "briefcase-business", bw = 14, bh = 14, fill = "#A3A199" },
						{ type = "icon", name = "Tools menu", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
					{ type = "frame", name = "Model picker", height = 26, gap = 6, padding = { 0, 8 }, cornerRadius = 13, alignItems = "center", bw = 130, bh = 26, children = {
						{ type = "ellipse", name = "Model orb", width = 13, height = 13, bw = 13, bh = 13, fill = "#1F1E1B", dither = { mode = 2, pixel = 1, angle = 225, cx = 0.5, cy = 0.5, radius = 0.5, from = 0, to = 1, curve = 1.1, noise = 0, scale = 4, seed = 0, soft = 0, ink = "#EDEBE5FF" } },
						{ type = "text", name = "Selected model", content = "Grok 4.7", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 46, bh = 16, fill = "#A3A199" },
						{ type = "text", name = "Effort", content = "High", fontFamily = "Geist", fontSize = 12, fontWeight = "normal", bw = 26, bh = 16, fill = "#6E6C65" },
						{ type = "icon", name = "Choose model", width = 11, height = 11, icon = "chevron-down", bw = 11, bh = 11, fill = "#6E6C65" },
					} },
				} },
			} },
			overlays = {
				{ node = { type = "frame", name = "Context menu anchor", padding = { 0, 0, 0, 44 }, bw = 288, bh = 196, children = {
						{ type = "frame", name = "Context menu · / Command", width = 244, layout = "vertical", padding = 4, cornerRadius = 10, bw = 244, bh = 196, stroke = "#33322D", strokeWidth = 1, fill = "#1A1A18", shadows = {
								{ x = 0, y = 12, blur = 28, spread = 0, color = "#00000066", inner = false },
								{ x = 0, y = 2, blur = 6, spread = 0, color = "#00000040", inner = false },
							}, children = {
							{ type = "frame", name = "Items", width = "fill_container", layout = "vertical", gap = 1, bw = 236, bh = 188, children = {
								{ type = "frame", name = "Header", width = "fill_container", gap = 6, padding = { 6, 8, 4, 8 }, alignItems = "center", bw = 236, bh = 23, children = {
									{ type = "text", name = "Header label", content = "/summarize · Command", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 120, bh = 13, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Item · Edit arguments", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, fill = "#262624", children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "text-cursor-input", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Edit arguments", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 87, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 81, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 11, bh = 14, children = {
										{ type = "icon", name = "Key icon 1", width = 11, height = 11, icon = "arrow-right-to-line", bw = 11, bh = 11, fill = "#6E6C65" },
									} },
								} },
								{ type = "frame", name = "Item · Change scope", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "scan", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Change scope", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 82, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 85, bh = 1 },
									{ type = "icon", name = "Submenu", width = 12, height = 12, icon = "chevron-right", bw = 12, bh = 12, fill = "#6E6C65" },
								} },
								{ type = "frame", name = "Item · Command details", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "info", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Command details", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 101, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 87, bh = 1 },
								} },
								{ type = "frame", name = "Item · Send as plain text", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "type", bw = 14, bh = 14, fill = "#858379" },
									{ type = "text", name = "Item label", content = "Send as plain text", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 103, bh = 16, fill = "#EDEBE5" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 85, bh = 1 },
								} },
								{ type = "frame", name = "Separator", width = "fill_container", layout = "vertical", padding = { 4, 0 }, bw = 236, bh = 9, children = {
									{ type = "rectangle", name = "Line", width = "fill_container", height = 1, bw = 236, bh = 1, fill = "#262521" },
								} },
								{ type = "frame", name = "Item · Remove command", width = "fill_container", height = 30, gap = 9, padding = { 0, 8 }, cornerRadius = 6, alignItems = "center", bw = 236, bh = 30, children = {
									{ type = "icon", name = "Item icon", width = 14, height = 14, icon = "x", bw = 14, bh = 14, fill = "#E8A898" },
									{ type = "text", name = "Item label", content = "Remove command", fontFamily = "Geist", fontSize = 12.5, fontWeight = "normal", bw = 106, bh = 16, fill = "#E8A898" },
									{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 62, bh = 1 },
									{ type = "frame", name = "Item shortcut", height = 14, gap = 3, alignItems = "center", bw = 11, bh = 14, children = {
										{ type = "icon", name = "Key icon 1", width = 11, height = 11, icon = "delete", bw = 11, bh = 11, fill = "#6E6C65" },
									} },
								} },
							} },
						} },
					} } },
			},
		},
	},
	uploading = { type = "frame", name = "Attachment · Uploading", height = 40, gap = 9, padding = { 0, 12, 0, 5 }, cornerRadius = 10, alignItems = "center", bw = 146, bh = 40, stroke = "#262521", strokeWidth = 1, fill = "#1A1A18", children = {
		{ type = "frame", name = "Upload progress wash", width = 96, height = 40, cornerRadius = { 10, 0, 0, 10 }, x = 0, y = 0, layoutPosition = "absolute", bw = 96, bh = 40, dither = { mode = 0, pixel = 1, angle = 270, cx = 0.5, cy = 0.5, radius = 0.5, from = 1, to = 0.2, curve = 0.8, noise = 0.25, scale = 3, seed = 0, soft = 0.2, ink = "#EDEBE538", speed = 0.35 } },
		{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, fill = "#121211", dither = { mode = 3, pixel = 1.5, angle = 0, cx = 0.5, cy = 1.1, radius = 1.1, from = 0.2, to = 0.7, curve = 0.8, noise = 0, scale = 1.6, seed = 4, soft = 0, ink = "#8A877EFF" } },
		{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 90, bh = 31, children = {
			{ type = "text", name = "Attachment name", content = "Floor plan.pdf", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 78, bh = 16, fill = "#EDEBE5" },
			{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 90, bh = 13, children = {
				{ type = "text", name = "Attachment detail", content = "Uploading · 62%", fontFamily = "Geist Mono", fontSize = 10, fontWeight = "normal", bw = 90, bh = 13, fill = "#A3A199" },
			} },
		} },
	} },
	uploads = {
		{ name = "Floor plan.pdf", size = 2400000 },
		{ name = "Dinner photo.jpg", size = 3100000, photo = "assets/photo-1552334855-f750885782c6.jpg" },
		{ name = "Trip budget.xlsx", size = 640000 },
		{ name = "Tram photo.jpg", size = 2200000, photo = "assets/photo-1584804746491-7413be6999d4.jpg" },
		{ name = "Itinerary.docx", size = 12000 },
		{ name = "Market photo.jpg", size = 2800000, photo = "assets/photo-1599855129764-f4cc28295202.jpg" },
		{ name = "notes.md", size = 3000 },
		{ name = "Ticket.pdf", size = 410000 },
		{ name = "Packing list.docx", size = 26000 },
		{ name = "Beach photo.jpg", size = 3400000, photo = "assets/photo-1603072995690-d88c14fd840d.jpg" },
		{ name = "Hotel booking.pdf", size = 180000 },
		{ name = "Budget v2.xlsx", size = 720000 },
		{ name = "Map.pdf", size = 5200000 },
		{ name = "Restaurants.md", size = 4000 },
	},
	photo_stack = { type = "frame", name = "Attachment · Photo stack", width = 72, height = 40, layout = "none", bw = 72, bh = 40, children = {
		{ type = "frame", name = "Photo 3", width = 32, height = 32, cornerRadius = 8, x = 36, y = 6, rotation = -9, clip = true, bw = 36.61, bh = 36.61, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1638890718904-3619e021b396.jpg", fit = "cover", children = {
			{ type = "rectangle", name = "Depth shade", width = 32, height = 32, x = 0, y = 0, layoutPosition = "absolute", bw = 32, bh = 32, fill = "#0A0A0959" },
		} },
		{ type = "frame", name = "Photo 2", width = 36, height = 36, cornerRadius = 9, x = 20, y = 3, rotation = -1, clip = true, bw = 36.62, bh = 36.62, stroke = "#0A0A09", strokeWidth = 1.5, img = "assets/photo-1754336241390-9a945d0d4daf.jpg", fit = "cover", children = {
			{ type = "rectangle", name = "Depth shade", width = 36, height = 36, x = 0, y = 0, layoutPosition = "absolute", bw = 36, bh = 36, fill = "#0A0A0933" },
		} },
		{ type = "frame", name = "Photo 1", width = 40, height = 40, layout = "none", cornerRadius = 10, x = 2, y = 5, rotation = 8, clip = true, bw = 45.18, bh = 45.18, stroke = "#262521", strokeWidth = 1, img = "assets/photo-1707952131058-38e59c053eb8.jpg", fit = "cover" },
		{ type = "frame", name = "Count", height = 15, padding = { 0, 4 }, cornerRadius = 8, alignItems = "center", x = -3, y = 30, bw = 14, bh = 15, stroke = "#2E2D29", strokeWidth = 1, fill = "#0A0A09", children = {
			{ type = "text", name = "Count label", content = "6", fontFamily = "Geist Mono", fontSize = 9, fontWeight = "500", bw = 6, bh = 12, fill = "#EDEBE5" },
		} },
	} },
	file_stack = { type = "frame", name = "Attachment · File stack", width = 150, height = 40, layout = "none", bw = 150, bh = 40, children = {
		{ type = "frame", name = "Layer 3", width = 134, height = 26, cornerRadius = 8, x = 16, y = 7, bw = 134, bh = 26, stroke = "#22211E", strokeWidth = 1, fill = "#121211" },
		{ type = "frame", name = "Layer 2", width = 134, height = 33, cornerRadius = 9, x = 8, y = 3.5, bw = 134, bh = 33, stroke = "#262521", strokeWidth = 1, fill = "#161615" },
		{ type = "frame", name = "Top file", width = 134, height = 40, gap = 9, padding = { 0, 10, 0, 5 }, cornerRadius = 10, alignItems = "center", x = 0, y = 0, bw = 134, bh = 40, stroke = "#2E2D29", strokeWidth = 1, fill = "#1A1A18", children = {
			{ type = "frame", name = "Attachment thumb", width = 30, height = 30, cornerRadius = 7, justifyContent = "center", alignItems = "center", bw = 30, bh = 30, stroke = "#33322D", strokeWidth = 1, fill = "#1F1F1D", children = {
				{ type = "icon", name = "Attachment icon", width = 14, height = 14, icon = "files", bw = 14, bh = 14, fill = "#C9C6BE" },
			} },
			{ type = "frame", name = "Attachment text", layout = "vertical", gap = 2, bw = 67, bh = 30, children = {
				{ type = "text", name = "Attachment name", content = "4 files", fontFamily = "Geist", fontSize = 12, fontWeight = "500", bw = 35, bh = 16, fill = "#EDEBE5" },
				{ type = "frame", name = "Attachment meta", gap = 5, alignItems = "center", bw = 67, bh = 12, children = {
					{ type = "text", name = "Type badge PDF", content = "PDF", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 19, bh = 12, fill = "#E8A898" },
					{ type = "text", name = "Type badge XLSX", content = "XLSX", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 25, bh = 12, fill = "#C3CEB2" },
					{ type = "text", name = "Type badge +1", content = "+2", fontFamily = "Geist Mono", fontSize = 9.5, fontWeight = "500", letterSpacing = 0.4, bw = 13, bh = 12, fill = "#6E6C65" },
				} },
			} },
		} },
	} },
}
