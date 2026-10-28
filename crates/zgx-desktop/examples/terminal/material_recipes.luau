-- Verbatim Rex MaterialStyles JSON values. Comfortable profiles are promoted
-- under active/inactive; compact and OS overrides are retained, not applied.
-- Explicit JSON null remains ui.null. Source defaults/patching live in material_profiles.
return {
	["blendTheme"] = {
		["dark"] = {
			["active"] = {
				["activePane"] = {
					["contentOpacity"] = 1,
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["geometry"] = {
							["opacity"] = 0,
						},
					},
				},
				["drawsCompactBoundaryLines"] = false,
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.2,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.8,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.2,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.35,
									["blue"] = 0.10006,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.048816,
									["red"] = 0.062444,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.3,
										["blue"] = 0.047059,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.070588,
										["red"] = 0.039216,
									},
								},
							},
						},
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.05,
							},
							["isEnabled"] = true,
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.1,
								},
								["id"] = "stroke1",
							},
						},
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.2,
										["blue"] = 0.047059,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.070588,
										["red"] = 0.039216,
									},
								},
								["role"] = "theme:surface",
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
						["shape"] = {
							["cornerRadius"] = 10,
						},
						["strokes"] = {
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.2,
								},
								["id"] = "stroke1",
								["width"] = 0.7,
							},
							{
								["alignment"] = "outside",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke2",
								["level"] = "belowContent",
								["width"] = 1,
							},
						},
					},
					["sheet"] = {
						["boundaryLines"] = false,
					},
				},
				["tabHover"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["isEnabled"] = false,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["blendMode"] = "color",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0.996078,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "theme:surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["geometry"] = {
							["opacity"] = 0.3,
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "theme:canvas",
						},
						["usesTint"] = true,
						["windowMaterial"] = "hudWindow",
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.1,
							},
						},
					},
					["horizontalSheet"] = {
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0.541176,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.784314,
											["red"] = 0.941176,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 1,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "overlay",
							["lineWidth"] = 1,
							["opacity"] = 1,
							["scope"] = "betweenPanes",
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "overlay",
						["lineWidth"] = 1,
						["opacity"] = 1,
						["scope"] = "betweenPanes",
					},
					["paneSheet"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.094118,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.086275,
										["red"] = 0.047059,
									},
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["geometry"] = {
								["insets"] = {
									["bottom"] = -1,
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.45,
										["blue"] = 0.121569,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.113725,
										["red"] = 0.078431,
									},
								},
								["role"] = "surfaceSubtle",
							},
							["usesTint"] = true,
						},
					},
					["panesWithoutGlass"] = {
						["activePane"] = {
							["contentOpacity"] = 1,
							["fill"] = {
								["blendMode"] = "overlay",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.25,
								},
							},
						},
						["horizontalSheet"] = {
							["divider"] = {
								["primary"] = {
									["blendMode"] = "overlay",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = true,
									["lineWidth"] = 1,
									["offset"] = 0,
									["opacity"] = 1,
								},
								["secondary"] = {
									["blendMode"] = "automatic",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = false,
									["lineWidth"] = 1,
									["offset"] = 1,
									["opacity"] = 0.1,
								},
							},
							["dividers"] = {
								["blendMode"] = "overlay",
								["lineWidth"] = 1,
								["opacity"] = 1,
								["scope"] = "betweenPanes",
							},
							["surface"] = {
								["backdrop"] = {
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.35,
												["blue"] = 0.071738,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.051338,
												["red"] = 0.037746,
											},
										},
										["role"] = "canvas",
									},
									["isEnabled"] = true,
								},
							},
						},
						["pane"] = {
							["contentOpacity"] = 0.7,
							["fill"] = {
								["isEnabled"] = true,
							},
						},
						["sheet"] = {
							["dividers"] = {
								["blendMode"] = "overlay",
								["lineWidth"] = 1,
								["opacity"] = 1,
								["scope"] = "betweenPanes",
							},
							["surface"] = {
								["backdrop"] = {
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0.094118,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.086275,
												["red"] = 0.047059,
											},
										},
										["role"] = "canvas",
									},
									["isEnabled"] = true,
								},
							},
						},
						["verticalSheet"] = {
							["divider"] = {
								["primary"] = {
									["blendMode"] = "plusLighter",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.1,
												["blue"] = 0.658824,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.898039,
												["red"] = 0.658824,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = true,
									["lineWidth"] = 1,
									["offset"] = 0,
									["opacity"] = 1,
								},
								["secondary"] = {
									["blendMode"] = "automatic",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = false,
									["lineWidth"] = 1,
									["offset"] = 1,
									["opacity"] = 0.1,
								},
							},
							["dividers"] = {
								["blendMode"] = "overlay",
								["lineWidth"] = 1,
								["opacity"] = 1,
								["scope"] = "betweenPanes",
							},
						},
					},
					["tabHover"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["isEnabled"] = false,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0.541176,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.784314,
											["red"] = 0.941176,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 1,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "overlay",
							["lineWidth"] = 1,
							["opacity"] = 1,
							["scope"] = "betweenPanes",
						},
					},
					["window"] = {
						["backdrop"] = {
							["blendMode"] = "color",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.996078,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "theme:surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.3,
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.856163,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.929635,
										["red"] = 0.956317,
									},
								},
								["role"] = "theme:canvas",
							},
							["usesTint"] = true,
							["windowMaterial"] = "hudWindow",
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.8,
				},
				["drawsCompactBoundaryLines"] = false,
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.6,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.35,
									["blue"] = 0.10006,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.048816,
									["red"] = 0.062444,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.1,
									["blue"] = 0.556863,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.541176,
									["red"] = 0.529412,
								},
							},
							["role"] = "contentTertiary",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "stroke1",
							["level"] = "belowContent",
							["width"] = 1,
						},
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0,
									["blue"] = 0.07058,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.07058,
									["red"] = 0.07058,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.2,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0.078892,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.089295,
									["red"] = 0.075429,
								},
							},
							["role"] = "theme:canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.8,
					},
					["horizontalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 0.5,
							["opacity"] = 0.05,
							["scope"] = "includingTopAndBottom",
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
						["fill"] = {
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 0.5,
						["opacity"] = 0.05,
						["scope"] = "includingTopAndBottom",
					},
					["paneSheet"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
							},
							["isEnabled"] = true,
						},
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0.392157,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.415686,
										["red"] = 0.458824,
									},
								},
								["role"] = "contentTertiary",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["glass"] = {
							["isEnabled"] = false,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "stroke1",
								["level"] = "belowContent",
								["width"] = 1,
							},
						},
					},
					["verticalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 0.5,
							["opacity"] = 0.05,
							["scope"] = "includingTopAndBottom",
						},
					},
					["window"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.55,
										["blue"] = 0.05098,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.07451,
										["red"] = 0.094118,
									},
								},
								["role"] = "surface",
							},
							["usesTint"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["windowInnerShadow"] = {
				["blendMode"] = "overlay",
				["blurRadius"] = 2.5,
				["color"] = {
					["color"] = {
						["kind"] = "fixed",
						["light"] = {
							["alpha"] = 0.6,
							["blue"] = 1,
							["colorSpace"] = "extendedSRGB",
							["green"] = 1,
							["red"] = 1,
						},
					},
					["role"] = "macOS:content",
				},
				["geometry"] = {
					["clipsToSurface"] = false,
					["insets"] = {
						["bottom"] = 0,
						["leading"] = 0,
						["top"] = 0,
						["trailing"] = 0,
					},
					["opacity"] = 1,
					["radius"] = {
						["mode"] = "concentric",
						["value"] = 0,
					},
				},
				["id"] = "inner",
				["isEnabled"] = true,
				["knockout"] = true,
				["offsetX"] = 0,
				["offsetY"] = 1.5,
			},
		},
		["light"] = {
			["active"] = {
				["activePane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.964706,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.964706,
									["red"] = 0.964706,
								},
							},
							["role"] = "surfaceRaised",
						},
					},
					["contentOpacity"] = 1,
					["fill"] = {
						["blendMode"] = "normal",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 0.980392,
								},
							},
							["role"] = "surfaceRaised",
						},
					},
				},
				["pane"] = {
					["contentOpacity"] = 0.7,
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0.156863,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.137255,
									["red"] = 0.121569,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.1,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.137255,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.12549,
										["red"] = 0.117647,
									},
								},
								["role"] = "surfaceRaised",
							},
						},
						["contentOpacity"] = 1,
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
						["shadows"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.65,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "shadow1",
							},
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.07,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke1",
							},
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 1,
								},
								["id"] = "stroke2",
							},
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0,
										["blue"] = 0.866667,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.952941,
										["red"] = 0.984314,
									},
								},
								["role"] = "theme:surface",
							},
							["isEnabled"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 2,
							},
						},
						["shape"] = {
							["cornerRadius"] = 10,
						},
						["strokes"] = {
							{
								["alignment"] = "outside",
								["blendMode"] = "plusDarker",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke1",
								["width"] = 1,
							},
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.4,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.1,
								},
								["id"] = "stroke2",
							},
						},
					},
					["sheet"] = {},
				},
				["tabContainer"] = {
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0.156863,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.137255,
									["red"] = 0.121569,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.08,
									["blue"] = 0.415686,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.439216,
									["red"] = 0.360784,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["isEnabled"] = false,
					},
					["shadows"] = { {
						["id"] = "shadow1",
						["isEnabled"] = false,
					} },
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.964706,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.964706,
									["red"] = 0.964706,
								},
							},
							["role"] = "surfaceRaised",
						},
						["usesTint"] = true,
					},
					["shadows"] = {
						{
							["blurRadius"] = 3.5,
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.407843,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.392157,
										["red"] = 0.380392,
									},
								},
								["role"] = "contentSecondary",
							},
							["geometry"] = {
								["opacity"] = 0.15,
							},
							["id"] = "shadow1",
							["offsetY"] = 1,
						},
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.55,
									["blue"] = 0.034151,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.054299,
									["red"] = 0.02825,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "macOS:canvas",
						},
						["usesTint"] = true,
						["windowMaterial"] = "hudWindow",
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.4,
										["blue"] = 0.957558,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.934196,
										["red"] = 0.944198,
									},
								},
								["role"] = "canvas",
							},
						},
					},
					["horizontalSheet"] = {
						["dividers"] = {
							["blendMode"] = "plusDarker",
							["lineWidth"] = 1,
							["opacity"] = 0.1,
							["scope"] = "includingTopAndBottom",
						},
						["surface"] = {
							["strokes"] = { {
								["id"] = "stroke1",
								["isEnabled"] = false,
							} },
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.25,
										["blue"] = 0.957558,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.934196,
										["red"] = 0.944198,
									},
								},
								["role"] = "theme:canvas",
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "plusDarker",
						["lineWidth"] = 1,
						["opacity"] = 0.1,
						["scope"] = "includingTopAndBottom",
					},
					["paneSheet"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["value"] = 15,
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.3,
										["blue"] = 0.941451,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.95146,
										["red"] = 0.921493,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
						["strokes"] = { {
							["id"] = "stroke1",
							["isEnabled"] = false,
						} },
					},
					["panesWithoutGlass"] = {
						["activePane"] = {
							["contentOpacity"] = 1,
							["fill"] = {
								["blendMode"] = "overlay",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.4,
											["blue"] = 0.957558,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.934196,
											["red"] = 0.944198,
										},
									},
									["role"] = "canvas",
								},
							},
						},
						["horizontalSheet"] = {
							["dividers"] = {
								["blendMode"] = "plusDarker",
								["lineWidth"] = 1,
								["opacity"] = 0.1,
								["scope"] = "includingTopAndBottom",
							},
							["surface"] = {
								["strokes"] = {
									{
										["id"] = "stroke1",
										["isEnabled"] = false,
									},
								},
							},
						},
						["pane"] = {
							["contentOpacity"] = 0.7,
							["fill"] = {
								["blendMode"] = "plusDarker",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.25,
											["blue"] = 0.957558,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.934196,
											["red"] = 0.944198,
										},
									},
									["role"] = "theme:canvas",
								},
								["isEnabled"] = true,
							},
						},
						["sheet"] = {
							["dividers"] = {
								["blendMode"] = "plusDarker",
								["lineWidth"] = 1,
								["opacity"] = 0.1,
								["scope"] = "includingTopAndBottom",
							},
							["surface"] = {
								["strokes"] = {
									{
										["id"] = "stroke1",
										["isEnabled"] = false,
									},
								},
							},
						},
						["verticalSheet"] = {
							["boundaryLines"] = false,
							["divider"] = {
								["primary"] = {
									["blendMode"] = "plusDarker",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0.952941,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.929412,
												["red"] = 0.901961,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = true,
									["lineWidth"] = 1,
									["offset"] = 0,
									["opacity"] = 0.07,
								},
								["secondary"] = {
									["blendMode"] = "overlay",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
										},
										["role"] = "",
									},
									["isEnabled"] = false,
									["lineWidth"] = 1,
									["offset"] = -0.5,
									["opacity"] = 0,
								},
							},
							["dividers"] = {
								["blendMode"] = "plusDarker",
								["lineWidth"] = 1,
								["opacity"] = 0.1,
								["scope"] = "includingTopAndBottom",
							},
						},
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0.156863,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.137255,
										["red"] = 0.121569,
									},
								},
								["role"] = "content",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.08,
										["blue"] = 0.415686,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.439216,
										["red"] = 0.360784,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["isEnabled"] = false,
						},
						["shadows"] = { {
							["id"] = "shadow1",
							["isEnabled"] = false,
						} },
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.25,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "surfaceRaised",
							},
							["usesTint"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 3.5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0.25098,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.223529,
											["red"] = 0.2,
										},
									},
									["role"] = "content",
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 1.5,
							},
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["boundaryLines"] = false,
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusDarker",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 0.952941,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.929412,
											["red"] = 0.901961,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 0.07,
							},
							["secondary"] = {
								["blendMode"] = "overlay",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = -0.5,
								["opacity"] = 0,
							},
						},
						["dividers"] = {
							["blendMode"] = "plusDarker",
							["lineWidth"] = 1,
							["opacity"] = 0.1,
							["scope"] = "includingTopAndBottom",
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.55,
										["blue"] = 0.034151,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.054299,
										["red"] = 0.02825,
									},
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.117647,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.117647,
										["red"] = 0.117647,
									},
								},
								["role"] = "macOS:canvas",
							},
							["usesTint"] = true,
							["windowMaterial"] = "hudWindow",
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.85,
				},
				["pane"] = {
					["contentOpacity"] = 0.5,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.4,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.1,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["contentOpacity"] = 0.85,
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0.556863,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.541176,
										["red"] = 0.529412,
									},
								},
								["role"] = "theme:contentTertiary",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 10,
						},
					},
					["sheet"] = {},
				},
				["tabContainer"] = {
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.08,
									["blue"] = 0.415686,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.439216,
									["red"] = 0.360784,
								},
							},
							["role"] = "contentSecondary",
						},
					},
					["glass"] = {
						["isEnabled"] = false,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.964706,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.964706,
									["red"] = 0.964706,
								},
							},
							["role"] = "surfaceRaised",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.6,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0,
									["blue"] = 0.082791,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.017864,
									["red"] = 0,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "macOS:canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.85,
					},
					["horizontalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 1,
							["opacity"] = 0.05,
							["scope"] = "includingTopAndBottom",
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.6,
						["fill"] = {
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 1,
						["opacity"] = 0.05,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.08,
										["blue"] = 0.415686,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.439216,
										["red"] = 0.360784,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["isEnabled"] = false,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.25,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "surfaceRaised",
							},
							["usesTint"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 1,
							["opacity"] = 0.05,
							["scope"] = "includingTopAndBottom",
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.6,
										["blue"] = 0.856163,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.929635,
										["red"] = 0.956317,
									},
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0,
										["blue"] = 0.875544,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.925725,
										["red"] = 0.945781,
									},
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "macOS:canvas",
							},
							["usesTint"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["windowInnerShadow"] = {
				["blendMode"] = "plusLighter",
				["blurRadius"] = 1,
				["color"] = {
					["color"] = {
						["kind"] = "fixed",
						["light"] = {
							["alpha"] = 1,
							["blue"] = 1,
							["colorSpace"] = "extendedSRGB",
							["green"] = 1,
							["red"] = 1,
						},
					},
					["role"] = "",
				},
				["geometry"] = {
					["clipsToSurface"] = false,
					["insets"] = {
						["bottom"] = 0,
						["leading"] = 0,
						["top"] = 0,
						["trailing"] = 0,
					},
					["opacity"] = 0.15,
					["radius"] = {
						["mode"] = "fixed",
						["value"] = 16,
					},
				},
				["id"] = "inner",
				["isEnabled"] = true,
				["knockout"] = true,
				["offsetX"] = 0,
				["offsetY"] = 0.5,
			},
		},
	},
	["modern"] = {
		["dark"] = {
			["active"] = {
				["activePane"] = {
					["contentOpacity"] = 1,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
					},
				},
				["drawsCompactBoundaryLines"] = false,
				["pane"] = {
					["backdrop"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.48,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.7,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.15,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.05,
							},
							["isEnabled"] = true,
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.1,
								},
								["id"] = "stroke1",
							},
						},
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.2,
										["blue"] = 0.047059,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.070588,
										["red"] = 0.039216,
									},
								},
								["role"] = "theme:surface",
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
						["shape"] = {
							["cornerRadius"] = 10,
						},
						["strokes"] = {
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.2,
								},
								["id"] = "stroke1",
								["width"] = 0.7,
							},
							{
								["alignment"] = "outside",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke2",
								["level"] = "belowContent",
								["width"] = 1,
							},
						},
					},
					["sheet"] = {
						["boundaryLines"] = false,
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.13,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
					},
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["isEnabled"] = false,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.6,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
						["windowMaterial"] = "hudWindow",
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.03,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.74,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
					},
					["drawsCompactBoundaryLines"] = "vertical",
					["horizontalSheet"] = {
						["boundaryLines"] = "vertical",
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["dark"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
										["kind"] = "adaptive",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 0.7,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "plusLighter",
							["lineWidth"] = 1,
							["opacity"] = 0.7,
							["scope"] = "betweenPanes",
						},
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
					},
					["paneDividers"] = {
						["blendMode"] = "plusLighter",
						["lineWidth"] = 1,
						["opacity"] = 0.7,
						["scope"] = "betweenPanes",
					},
					["paneSheet"] = {
						["glass"] = {
							["geometry"] = {
								["insets"] = {
									["bottom"] = -1,
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0.117647,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.117647,
										["red"] = 0.117647,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
					},
					["panesWithoutGlass"] = {
						["activePane"] = {
							["backdrop"] = {
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.03,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
							},
							["contentOpacity"] = 1,
							["fill"] = {
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.74,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
							},
						},
						["horizontalSheet"] = {
							["boundaryLines"] = "vertical",
							["divider"] = {
								["primary"] = {
									["blendMode"] = "plusLighter",
									["color"] = {
										["color"] = {
											["dark"] = {
												["alpha"] = 0.1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
											["kind"] = "adaptive",
											["light"] = {
												["alpha"] = 0.1,
												["blue"] = 0,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0,
												["red"] = 0,
											},
										},
										["role"] = "",
									},
									["isEnabled"] = true,
									["lineWidth"] = 1,
									["offset"] = 0,
									["opacity"] = 0.7,
								},
								["secondary"] = {
									["blendMode"] = "automatic",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = false,
									["lineWidth"] = 1,
									["offset"] = 1,
									["opacity"] = 0.1,
								},
							},
							["dividers"] = {
								["blendMode"] = "plusLighter",
								["lineWidth"] = 1,
								["opacity"] = 0.7,
								["scope"] = "betweenPanes",
							},
						},
						["pane"] = {
							["backdrop"] = {
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["isEnabled"] = true,
							},
							["contentOpacity"] = 0.7,
						},
						["sheet"] = {
							["boundaryLines"] = "vertical",
							["dividers"] = {
								["blendMode"] = "plusLighter",
								["lineWidth"] = 1,
								["opacity"] = 0.7,
								["scope"] = "betweenPanes",
							},
						},
						["verticalSheet"] = {
							["boundaryLines"] = "vertical",
							["divider"] = {
								["primary"] = {
									["blendMode"] = "plusLighter",
									["color"] = {
										["color"] = {
											["dark"] = {
												["alpha"] = 0.1,
												["blue"] = 1,
												["colorSpace"] = "extendedSRGB",
												["green"] = 1,
												["red"] = 1,
											},
											["kind"] = "adaptive",
											["light"] = {
												["alpha"] = 0.1,
												["blue"] = 0,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0,
												["red"] = 0,
											},
										},
										["role"] = "",
									},
									["isEnabled"] = true,
									["lineWidth"] = 1,
									["offset"] = 0,
									["opacity"] = 0.7,
								},
								["secondary"] = {
									["blendMode"] = "automatic",
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0,
												["red"] = 0,
											},
										},
										["role"] = "content",
									},
									["isEnabled"] = false,
									["lineWidth"] = 1,
									["offset"] = 1,
									["opacity"] = 0.1,
								},
							},
							["dividers"] = {
								["blendMode"] = "plusLighter",
								["lineWidth"] = 1,
								["opacity"] = 0.7,
								["scope"] = "betweenPanes",
							},
						},
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["insets"] = {
									["bottom"] = 1,
									["leading"] = 1,
									["top"] = 1,
									["trailing"] = 1,
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["isEnabled"] = false,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["boundaryLines"] = "vertical",
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["dark"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
										["kind"] = "adaptive",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 0.7,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "plusLighter",
							["lineWidth"] = 1,
							["opacity"] = 0.7,
							["scope"] = "betweenPanes",
						},
					},
					["window"] = {
						["backdrop"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.6,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.117647,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.117647,
										["red"] = 0.117647,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
							["windowMaterial"] = "hudWindow",
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.8,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.02,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
					},
				},
				["drawsCompactBoundaryLines"] = false,
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.03,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "stroke1",
							["level"] = "belowContent",
							["width"] = 1,
						},
					},
				},
				["window"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
						["windowMaterial"] = "hudWindow",
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.8,
					},
					["horizontalSheet"] = {
						["divider"] = {
							["primary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["dark"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
										["kind"] = "adaptive",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "",
								},
								["isEnabled"] = true,
								["lineWidth"] = 0.5,
								["offset"] = 0,
								["opacity"] = 0.55,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 0.5,
							["opacity"] = 0.55,
							["scope"] = "includingTopAndBottom",
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
						["fill"] = {
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 0.5,
						["opacity"] = 0.55,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.02,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["insets"] = {
									["bottom"] = 1,
									["leading"] = 1,
									["top"] = 1,
									["trailing"] = 1,
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "stroke1",
								["level"] = "belowContent",
								["width"] = 1,
							},
						},
					},
					["verticalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 0.5,
							["opacity"] = 0.55,
							["scope"] = "includingTopAndBottom",
						},
					},
					["window"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.117647,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.117647,
										["red"] = 0.117647,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["windowInnerShadow"] = {
				["blendMode"] = "overlay",
				["blurRadius"] = 2.5,
				["color"] = {
					["color"] = {
						["kind"] = "fixed",
						["light"] = {
							["alpha"] = 0.6,
							["blue"] = 1,
							["colorSpace"] = "extendedSRGB",
							["green"] = 1,
							["red"] = 1,
						},
					},
					["role"] = "macOS:content",
				},
				["geometry"] = {
					["clipsToSurface"] = false,
					["insets"] = {
						["bottom"] = 0,
						["leading"] = 0,
						["top"] = 0,
						["trailing"] = 0,
					},
					["opacity"] = 1,
					["radius"] = {
						["mode"] = "concentric",
						["value"] = 0,
					},
				},
				["id"] = "inner",
				["isEnabled"] = true,
				["knockout"] = true,
				["offsetX"] = 0,
				["offsetY"] = 1.5,
			},
		},
		["light"] = {
			["active"] = {
				["activePane"] = {
					["backdrop"] = {
						["isEnabled"] = true,
					},
					["contentOpacity"] = 1,
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
					},
				},
				["pane"] = {
					["contentOpacity"] = 0.7,
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.7,
										["blue"] = 0.866667,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.952941,
										["red"] = 0.984314,
									},
								},
							},
						},
						["shadows"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.65,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "shadow1",
							},
						},
						["strokes"] = {
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.07,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke1",
							},
							{
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 1,
								},
								["id"] = "stroke2",
							},
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0.866667,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.952941,
										["red"] = 0.984314,
									},
								},
								["role"] = "theme:surface",
							},
							["isEnabled"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 2,
							},
						},
						["shape"] = {
							["cornerRadius"] = 10,
						},
						["strokes"] = {
							{
								["alignment"] = "outside",
								["blendMode"] = "plusDarker",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
								},
								["id"] = "stroke1",
								["width"] = 1,
							},
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.4,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["geometry"] = {
									["opacity"] = 0.1,
								},
								["id"] = "stroke2",
							},
						},
					},
					["sheet"] = {},
				},
				["tabContainer"] = {
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["glass"] = {
						["isEnabled"] = false,
					},
					["shadows"] = { {
						["id"] = "shadow1",
						["isEnabled"] = false,
					} },
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shadows"] = {
						{
							["blurRadius"] = 3.5,
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.4,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "content",
							},
							["geometry"] = {
								["opacity"] = 0.15,
							},
							["id"] = "shadow1",
							["offsetY"] = 1.5,
						},
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.7,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
						["windowMaterial"] = "hudWindow",
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["blendMode"] = "normal",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.3,
										["blue"] = 0.156863,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.156863,
										["red"] = 0.156863,
									},
								},
							},
						},
					},
					["drawsCompactBoundaryLines"] = "horizontal",
					["horizontalSheet"] = {
						["boundaryLines"] = "horizontal",
						["dividers"] = {
							["blendMode"] = "plusDarker",
							["lineWidth"] = 1,
							["opacity"] = 0.7,
							["scope"] = "includingTopAndBottom",
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.8,
										["blue"] = 0.964706,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.964706,
										["red"] = 0.964706,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "plusDarker",
						["lineWidth"] = 1,
						["opacity"] = 0.7,
						["scope"] = "includingTopAndBottom",
					},
					["paneSheet"] = {
						["glass"] = {
							["isEnabled"] = true,
							["style"] = "clear",
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "content",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["glass"] = {
							["isEnabled"] = false,
						},
						["shadows"] = { {
							["id"] = "shadow1",
							["isEnabled"] = false,
						} },
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.25,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 3.5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.4,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "content",
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 1.5,
							},
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["boundaryLines"] = false,
						["divider"] = {
							["primary"] = {
								["blendMode"] = "plusDarker",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = true,
								["lineWidth"] = 1,
								["offset"] = 0,
								["opacity"] = 0.07,
							},
							["secondary"] = {
								["blendMode"] = "automatic",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
									["role"] = "content",
								},
								["isEnabled"] = false,
								["lineWidth"] = 1,
								["offset"] = 1,
								["opacity"] = 0.1,
							},
						},
						["dividers"] = {
							["blendMode"] = "plusDarker",
							["lineWidth"] = 1,
							["opacity"] = 0.7,
							["scope"] = "includingTopAndBottom",
						},
						["surface"] = {
							["glass"] = {
								["isEnabled"] = true,
								["tint"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0.117647,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.117647,
											["red"] = 0.117647,
										},
									},
									["role"] = "canvas",
								},
								["usesTint"] = true,
							},
						},
					},
					["window"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.7,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "macOS:canvas",
							},
							["usesTint"] = true,
							["windowMaterial"] = "hudWindow",
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.85,
				},
				["pane"] = {
					["contentOpacity"] = 0.5,
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["panesWithoutGlass"] = {
					["activePane"] = {
						["contentOpacity"] = 0.85,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.964706,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.964706,
										["red"] = 0.964706,
									},
								},
								["role"] = "surfaceRaised",
							},
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0.964706,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.964706,
										["red"] = 0.964706,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 10,
						},
					},
					["sheet"] = {},
				},
				["tabContainer"] = {
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
						["tint"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.25,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
							},
							["role"] = "canvas",
						},
						["usesTint"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["glass"] = {
						["geometry"] = {
							["radius"] = {
								["mode"] = "matchSurface",
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.85,
					},
					["pane"] = {
						["contentOpacity"] = 0.6,
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 1,
						["opacity"] = 0.4,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabSelection"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
							["tint"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.25,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "canvas",
							},
							["usesTint"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["verticalSheet"] = {
						["dividers"] = {
							["blendMode"] = "automatic",
							["lineWidth"] = 1,
							["opacity"] = 0.4,
							["scope"] = "includingTopAndBottom",
						},
					},
					["window"] = {
						["glass"] = {
							["geometry"] = {
								["radius"] = {
									["mode"] = "matchSurface",
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["windowInnerShadow"] = {
				["blendMode"] = "plusLighter",
				["blurRadius"] = 1,
				["color"] = {
					["color"] = {
						["kind"] = "fixed",
						["light"] = {
							["alpha"] = 1,
							["blue"] = 1,
							["colorSpace"] = "extendedSRGB",
							["green"] = 1,
							["red"] = 1,
						},
					},
					["role"] = "",
				},
				["geometry"] = {
					["clipsToSurface"] = false,
					["insets"] = {
						["bottom"] = 0,
						["leading"] = 0,
						["top"] = 0,
						["trailing"] = 0,
					},
					["opacity"] = 0.15,
					["radius"] = {
						["mode"] = "fixed",
						["value"] = 16,
					},
				},
				["id"] = "inner",
				["isEnabled"] = true,
				["knockout"] = true,
				["offsetX"] = 0,
				["offsetY"] = 0.5,
			},
		},
	},
	["useTheme"] = {
		["dark"] = {
			["active"] = {
				["activePane"] = {
					["backdrop"] = {
						["color"] = {
							["role"] = "theme:surface",
						},
						["geometry"] = {
							["opacity"] = 1,
						},
					},
					["contentOpacity"] = 1,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "container",
						},
					},
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.086275,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.070588,
									["red"] = 0.098039,
								},
							},
							["role"] = "surface",
						},
						["geometry"] = {
							["opacity"] = 0,
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.7,
					["shape"] = {
						["cornerRadius"] = 10,
					},
					["strokes"] = {
						{
							["blendMode"] = "plusLighter",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.07,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "container",
							["width"] = 0.8,
						},
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.145098,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.113725,
									["red"] = 0.113725,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
					},
					["fill"] = {
						["isEnabled"] = false,
					},
					["strokes"] = { {
						["id"] = "inner",
						["isEnabled"] = false,
					} },
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.184314,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.12549,
									["red"] = 0.141176,
								},
							},
							["role"] = "theme:surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["blendMode"] = "plusLighter",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "inner",
							["width"] = 0.8,
						},
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.078892,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.089295,
									["red"] = 0.075429,
								},
							},
							["role"] = "theme:canvas",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["backdrop"] = {
							["geometry"] = {
								["opacity"] = 1,
							},
						},
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.6,
										["blue"] = 0.145098,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.113725,
										["red"] = 0.113725,
									},
								},
								["role"] = "surfaceRaised",
							},
						},
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.086275,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.070588,
										["red"] = 0.098039,
									},
								},
								["role"] = "theme:surface",
							},
							["geometry"] = {
								["opacity"] = 0,
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
					},
					["paneDividers"] = {
						["blendMode"] = "plusLighter",
						["lineWidth"] = 1,
						["opacity"] = 0.08,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.145098,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.113725,
										["red"] = 0.113725,
									},
								},
								["role"] = "surfaceRaised",
							},
							["geometry"] = {
								["insets"] = {
									["bottom"] = 0.9,
									["leading"] = 0.9,
									["top"] = 0.9,
									["trailing"] = 0.9,
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
						["fill"] = {
							["isEnabled"] = false,
						},
						["strokes"] = { {
							["id"] = "inner",
							["isEnabled"] = false,
						} },
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.184314,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.12549,
										["red"] = 0.141176,
									},
								},
								["role"] = "theme:surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "inner",
								["width"] = 0.8,
							},
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.069917,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.056208,
										["red"] = 0.080213,
									},
								},
								["role"] = "theme:canvas",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.8,
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 0.094,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.122,
									["red"] = 0.141,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["fill"] = {
						["geometry"] = {
							["opacity"] = 0,
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.12549,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.133333,
									["red"] = 0.152941,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.058824,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.086275,
									["red"] = 0.101961,
								},
							},
							["role"] = "surfaceSubtle",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.8,
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 0.5,
						["opacity"] = 0.05,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.12549,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.133333,
										["red"] = 0.152941,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.058824,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.086275,
										["red"] = 0.101961,
									},
								},
								["role"] = "surfaceSubtle",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
		},
		["light"] = {
			["active"] = {
				["activePane"] = {
					["contentOpacity"] = 1,
					["fill"] = {
						["blendMode"] = "normal",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.917647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.917647,
									["red"] = 0.917647,
								},
							},
							["role"] = "theme:surface",
						},
						["geometry"] = {
							["opacity"] = 1,
						},
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.72549,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.803922,
										["red"] = 0.843137,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.7,
							},
							["id"] = "container",
							["width"] = 0.8,
						},
					},
				},
				["pane"] = {
					["contentOpacity"] = 0.6,
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "canvas",
						},
						["geometry"] = {
							["opacity"] = 0.25,
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.72549,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.803922,
										["red"] = 0.843137,
									},
								},
								["role"] = "theme:stroke",
							},
							["geometry"] = {
								["opacity"] = 0.5,
							},
							["id"] = "container",
							["width"] = 0.7,
						},
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0.156863,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.137255,
									["red"] = 0.121569,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["isEnabled"] = false,
					},
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.06,
									["blue"] = 0.415686,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.439216,
									["red"] = 0.360784,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["shadows"] = { {
						["id"] = "shadow1",
						["isEnabled"] = false,
					} },
					["strokes"] = {
						{
							["id"] = "container",
							["isEnabled"] = false,
						},
						{
							["id"] = "inner",
							["isEnabled"] = false,
						},
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 0.980392,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["shadows"] = {
						{
							["blurRadius"] = 3.5,
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.65,
										["blue"] = 0.25098,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.27451,
										["red"] = 0.184314,
									},
								},
								["role"] = "content",
							},
							["geometry"] = {
								["opacity"] = 0.15,
							},
							["id"] = "shadow1",
							["offsetY"] = 1.5,
						},
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["alignment"] = "center",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0.156863,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.137255,
										["red"] = 0.121569,
									},
								},
								["role"] = "content",
							},
							["id"] = "container",
						},
						{
							["blendMode"] = "plusLighter",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "inner",
							["width"] = 0.7,
						},
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.856163,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.929635,
									["red"] = 0.956317,
								},
							},
							["role"] = "theme:canvas",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.690196,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.729412,
										["red"] = 0.709804,
									},
								},
							},
						},
					},
					["pane"] = {
						["contentOpacity"] = 0.7,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.3,
										["blue"] = 0.937255,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.976471,
										["red"] = 0.956863,
									},
								},
								["role"] = "surface",
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "plusDarker",
						["lineWidth"] = 1,
						["opacity"] = 0.08,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0.156863,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.137255,
										["red"] = 0.121569,
									},
								},
								["role"] = "content",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["isEnabled"] = false,
						},
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.06,
										["blue"] = 0.415686,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.439216,
										["red"] = 0.360784,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["shadows"] = { {
							["id"] = "shadow1",
							["isEnabled"] = false,
						} },
						["strokes"] = {
							{
								["id"] = "container",
								["isEnabled"] = false,
							},
							{
								["id"] = "inner",
								["isEnabled"] = false,
							},
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 0.980392,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 3.5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0.156863,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.137255,
											["red"] = 0.121569,
										},
									},
									["role"] = "content",
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 1.5,
							},
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["alignment"] = "center",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0.156863,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0.137255,
											["red"] = 0.121569,
										},
									},
									["role"] = "content",
								},
								["id"] = "container",
							},
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "inner",
								["width"] = 0.7,
							},
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "canvas",
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.85,
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.4,
									["blue"] = 0.949,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.933,
									["red"] = 0.925,
								},
							},
							["role"] = "surfaceSubtle",
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["fill"] = {
						["geometry"] = {
							["opacity"] = 0,
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["isEnabled"] = false,
					},
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0.415686,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.439216,
									["red"] = 0.360784,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0.443137,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.427451,
									["red"] = 0.411765,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "canvas",
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.6,
					},
					["pane"] = {
						["contentOpacity"] = 0.6,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0,
										["blue"] = 0.898039,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.92549,
										["red"] = 0.937255,
									},
								},
								["role"] = "surfaceSubtle",
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 1,
						["opacity"] = 0.05,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["isEnabled"] = false,
						},
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0.415686,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.439216,
										["red"] = 0.360784,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0.443137,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.427451,
										["red"] = 0.411765,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "canvas",
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
		},
	},
	["system"] = {
		["dark"] = {
			["active"] = {
				["activePane"] = {
					["backdrop"] = {
						["geometry"] = {
							["opacity"] = 1,
						},
					},
					["contentOpacity"] = 1,
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.1,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
						},
					},
					["strokes"] = { {
						["id"] = "container",
						["width"] = 0.8,
					} },
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.086275,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.070588,
									["red"] = 0.098039,
								},
							},
							["role"] = "theme:surface",
						},
						["geometry"] = {
							["opacity"] = 0.5,
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.7,
					["shape"] = {
						["cornerRadius"] = 10,
					},
					["strokes"] = {
						{
							["alignment"] = "outside",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["geometry"] = {
								["opacity"] = 0.07,
							},
							["id"] = "container",
							["width"] = 0.7,
						},
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["isEnabled"] = false,
					},
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
					},
					["strokes"] = { {
						["id"] = "inner",
						["isEnabled"] = false,
					} },
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.08,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["blendMode"] = "overlay",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.2,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["blendMode"] = "plusLighter",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "inner",
							["width"] = 0.8,
						},
					},
				},
				["window"] = {
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["backdrop"] = {
							["geometry"] = {
								["opacity"] = 1,
							},
						},
						["contentOpacity"] = 1,
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.086275,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.070588,
										["red"] = 0.098039,
									},
								},
								["role"] = "theme:surface",
							},
							["geometry"] = {
								["opacity"] = 0.5,
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 1,
						["opacity"] = 0.8,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "content",
							},
							["geometry"] = {
								["insets"] = {
									["bottom"] = 1,
									["leading"] = 1,
									["top"] = 1,
									["trailing"] = 1,
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["isEnabled"] = false,
						},
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
						},
						["strokes"] = { {
							["id"] = "inner",
							["isEnabled"] = false,
						} },
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.06,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["fill"] = {
							["blendMode"] = "overlay",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.2,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "inner",
								["width"] = 0.8,
							},
						},
					},
					["window"] = {
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.8,
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.02,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "control",
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["fill"] = {
						["geometry"] = {
							["opacity"] = 0,
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.8,
					},
					["pane"] = {
						["contentOpacity"] = 0.5,
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 0.5,
						["opacity"] = 0.5,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["window"] = {
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
		},
		["light"] = {
			["active"] = {
				["activePane"] = {
					["backdrop"] = {
						["geometry"] = {
							["opacity"] = 1,
						},
					},
					["contentOpacity"] = 1,
					["strokes"] = {
						{
							["alignment"] = "inside",
							["id"] = "container",
							["width"] = 0.8,
						},
					},
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.086275,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.070588,
									["red"] = 0.098039,
								},
							},
							["role"] = "theme:surface",
						},
						["geometry"] = {
							["opacity"] = 0.5,
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.7,
					["shape"] = {
						["cornerRadius"] = 10,
					},
					["strokes"] = {
						{
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.294118,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.270588,
										["red"] = 0.317647,
									},
								},
								["role"] = "theme:stroke",
							},
							["geometry"] = {
								["opacity"] = 0.5,
							},
							["id"] = "container",
							["width"] = 0.7,
						},
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabHover"] = {
					["backdrop"] = {
						["isEnabled"] = false,
					},
					["fill"] = {
						["blendMode"] = "plusDarker",
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.05,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["shadows"] = { {
						["id"] = "shadow1",
						["isEnabled"] = false,
					} },
					["strokes"] = {
						{
							["id"] = "container",
							["isEnabled"] = false,
						},
						{
							["id"] = "inner",
							["isEnabled"] = false,
						},
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "surface",
						},
						["isEnabled"] = true,
					},
					["shadows"] = {
						{
							["blurRadius"] = 3.5,
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "content",
							},
							["geometry"] = {
								["opacity"] = 0.15,
							},
							["id"] = "shadow1",
							["offsetY"] = 1.5,
						},
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
					["strokes"] = {
						{
							["alignment"] = "center",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.1,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "stroke",
							},
							["id"] = "container",
						},
						{
							["blendMode"] = "plusLighter",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["id"] = "inner",
							["width"] = 0.7,
						},
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 0.964706,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.964706,
									["red"] = 0.964706,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["fill"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.5,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["backdrop"] = {
							["geometry"] = {
								["opacity"] = 1,
							},
						},
						["contentOpacity"] = 1,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "canvas",
							},
						},
					},
					["pane"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.901961,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.937255,
										["red"] = 0.976471,
									},
								},
								["role"] = "theme:surface",
							},
							["geometry"] = {
								["opacity"] = 0.5,
							},
							["isEnabled"] = true,
						},
						["contentOpacity"] = 0.7,
					},
					["paneDividers"] = {
						["blendMode"] = "plusDarker",
						["lineWidth"] = 1,
						["opacity"] = 0.8,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "content",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabHover"] = {
						["backdrop"] = {
							["isEnabled"] = false,
						},
						["fill"] = {
							["blendMode"] = "plusDarker",
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.05,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["shadows"] = { {
							["id"] = "shadow1",
							["isEnabled"] = false,
						} },
						["strokes"] = {
							{
								["id"] = "container",
								["isEnabled"] = false,
							},
							{
								["id"] = "inner",
								["isEnabled"] = false,
							},
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "surface",
							},
							["isEnabled"] = true,
						},
						["shadows"] = {
							{
								["blurRadius"] = 3.5,
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.5,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "content",
								},
								["geometry"] = {
									["opacity"] = 0.15,
								},
								["id"] = "shadow1",
								["offsetY"] = 1.5,
							},
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
						["strokes"] = {
							{
								["alignment"] = "center",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.1,
											["blue"] = 0,
											["colorSpace"] = "extendedSRGB",
											["green"] = 0,
											["red"] = 0,
										},
									},
									["role"] = "stroke",
								},
								["id"] = "container",
							},
							{
								["blendMode"] = "plusLighter",
								["color"] = {
									["color"] = {
										["kind"] = "fixed",
										["light"] = {
											["alpha"] = 0.05,
											["blue"] = 1,
											["colorSpace"] = "extendedSRGB",
											["green"] = 1,
											["red"] = 1,
										},
									},
								},
								["id"] = "inner",
								["width"] = 0.7,
							},
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 0.964706,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.964706,
										["red"] = 0.964706,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.5,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
			["inactive"] = {
				["activePane"] = {
					["contentOpacity"] = 0.85,
				},
				["pane"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.3,
									["blue"] = 0.964706,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.964706,
									["red"] = 0.964706,
								},
							},
							["role"] = "surfaceRaised",
						},
						["isEnabled"] = true,
					},
					["contentOpacity"] = 0.5,
					["fill"] = {
						["geometry"] = {
							["opacity"] = 0,
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 10,
					},
				},
				["tabContainer"] = {
					["fill"] = {
						["color"] = {
							["role"] = "content",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 40,
					},
				},
				["tabSelection"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["kind"] = "fixed",
								["light"] = {
									["alpha"] = 0.03,
									["blue"] = 0,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0,
									["red"] = 0,
								},
							},
							["role"] = "contentSecondary",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 16.5,
					},
				},
				["window"] = {
					["backdrop"] = {
						["color"] = {
							["color"] = {
								["dark"] = {
									["alpha"] = 1,
									["blue"] = 0.117647,
									["colorSpace"] = "extendedSRGB",
									["green"] = 0.117647,
									["red"] = 0.117647,
								},
								["kind"] = "adaptive",
								["light"] = {
									["alpha"] = 1,
									["blue"] = 1,
									["colorSpace"] = "extendedSRGB",
									["green"] = 1,
									["red"] = 1,
								},
							},
							["role"] = "canvas",
						},
						["isEnabled"] = true,
					},
					["shape"] = {
						["cornerRadius"] = 12,
					},
				},
				["compact"] = {
					["activePane"] = {
						["contentOpacity"] = 0.85,
					},
					["pane"] = {
						["contentOpacity"] = 0.6,
						["fill"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0,
										["blue"] = 0.964706,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.964706,
										["red"] = 0.964706,
									},
								},
								["role"] = "surfaceRaised",
							},
							["isEnabled"] = true,
						},
					},
					["paneDividers"] = {
						["blendMode"] = "automatic",
						["lineWidth"] = 1,
						["opacity"] = 0.4,
						["scope"] = "includingTopAndBottom",
					},
					["tabContainer"] = {
						["shape"] = {
							["cornerRadius"] = 40,
						},
					},
					["tabSelection"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["kind"] = "fixed",
									["light"] = {
										["alpha"] = 0.03,
										["blue"] = 0,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0,
										["red"] = 0,
									},
								},
								["role"] = "contentSecondary",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 16.5,
						},
					},
					["window"] = {
						["backdrop"] = {
							["color"] = {
								["color"] = {
									["dark"] = {
										["alpha"] = 1,
										["blue"] = 0.117647,
										["colorSpace"] = "extendedSRGB",
										["green"] = 0.117647,
										["red"] = 0.117647,
									},
									["kind"] = "adaptive",
									["light"] = {
										["alpha"] = 1,
										["blue"] = 1,
										["colorSpace"] = "extendedSRGB",
										["green"] = 1,
										["red"] = 1,
									},
								},
								["role"] = "canvas",
							},
							["isEnabled"] = true,
						},
						["shape"] = {
							["cornerRadius"] = 12,
						},
					},
				},
			},
		},
	},
	["sequoiaOverrides"] = {
		["styles"] = {
			["system"] = {
				["dark"] = {
					["active"] = {
						["comfortable"] = {
							["window"] = {
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["windowMaterial"] = "windowBackground",
								},
							},
						},
						["compact"] = {
							["activePane"] = {
								["backdrop"] = {
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0.090196,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.066667,
												["red"] = 0.05098,
											},
										},
									},
								},
							},
							["pane"] = {
								["backdrop"] = {
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0.090196,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.066667,
												["red"] = 0.05098,
											},
										},
									},
								},
							},
							["window"] = {
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["windowMaterial"] = "windowBackground",
								},
							},
						},
					},
					["inactive"] = {
						["comfortable"] = {
							["window"] = ui.null,
						},
						["compact"] = {
							["window"] = ui.null,
						},
					},
				},
				["light"] = {
					["active"] = {
						["comfortable"] = {
							["activePane"] = {
								["strokes"] = {
									{
										["alignment"] = ui.null,
										["id"] = "container",
									},
								},
							},
							["pane"] = {
								["strokes"] = {
									{
										["alignment"] = "outside",
										["id"] = "container",
									},
								},
							},
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
								},
							},
						},
						["compact"] = {
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["isEnabled"] = true,
								},
							},
						},
					},
					["inactive"] = {
						["comfortable"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
						["compact"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
					},
				},
			},
			["useTheme"] = {
				["dark"] = {
					["active"] = {
						["comfortable"] = {
							["pane"] = {
								["backdrop"] = {
									["geometry"] = {
										["opacity"] = 0.49,
									},
								},
							},
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["tint"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.5,
												["blue"] = 0.071738,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.051338,
												["red"] = 0.037746,
											},
										},
										["role"] = "theme:canvas",
									},
									["usesTint"] = true,
									["windowMaterial"] = "windowBackground",
								},
							},
						},
						["compact"] = {
							["pane"] = {
								["backdrop"] = {
									["geometry"] = {
										["opacity"] = 0.5,
									},
								},
							},
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["tint"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.5,
												["blue"] = 0.071738,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.051338,
												["red"] = 0.037746,
											},
										},
										["role"] = "theme:canvas",
									},
									["usesTint"] = true,
									["windowMaterial"] = "windowBackground",
								},
							},
						},
					},
					["inactive"] = {
						["comfortable"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
						["compact"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
					},
				},
				["light"] = {
					["active"] = {
						["comfortable"] = {
							["activePane"] = {
								["strokes"] = {
									{
										["alignment"] = "outside",
										["color"] = ui.null,
										["id"] = "container",
										["width"] = ui.null,
									},
								},
							},
							["pane"] = {
								["strokes"] = {
									{
										["alignment"] = "outside",
										["id"] = "container",
									},
								},
							},
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["tint"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.5,
												["blue"] = 0.976316,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.976316,
												["red"] = 0.976316,
											},
										},
										["role"] = "theme:canvas",
									},
									["usesTint"] = true,
								},
							},
						},
						["compact"] = {
							["activePane"] = {
								["fill"] = {
									["color"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 1,
												["blue"] = 0.090196,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.066667,
												["red"] = 0.05098,
											},
										},
									},
								},
							},
							["window"] = {
								["backdrop"] = ui.null,
								["glass"] = {
									["geometry"] = {
										["radius"] = {
											["mode"] = "fixed",
										},
									},
									["isEnabled"] = true,
									["tint"] = {
										["color"] = {
											["kind"] = "fixed",
											["light"] = {
												["alpha"] = 0.5,
												["blue"] = 0.071738,
												["colorSpace"] = "extendedSRGB",
												["green"] = 0.051338,
												["red"] = 0.037746,
											},
										},
										["role"] = "theme:canvas",
									},
									["usesTint"] = true,
								},
							},
						},
					},
					["inactive"] = {
						["comfortable"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
						["compact"] = {
							["window"] = {
								["shape"] = ui.null,
							},
						},
					},
				},
			},
		},
		["version"] = 1,
	},
}
