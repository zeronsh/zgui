--!nonstrict
-- (Non-strict until slot-provided props and decoded JSON are typed: the
-- solver cannot see `props.slot` arrive or narrow API data yet.)
-- Hacker News in a pane, with its own navigation: the feeds (Top, New,
-- Best, Ask, Show), a story's discussion as a thread you can fold, the
-- article itself, and the people who wrote them. Back goes back.
--
-- It asks for `network` (zterm fetches for it) and `web` (the article
-- shows in the pane, in pages and cookies of the plugin's own).
local API = "https://hacker-news.firebaseio.com/v0"
local ALGOLIA = "https://hn.algolia.com/api/v1"
local PAGE = 30
local ORANGE = "#ff6a1f"

local FEEDS = {
	{ id = "top", title = "Top", path = "topstories" },
	{ id = "new", title = "New", path = "newstories" },
	{ id = "best", title = "Best", path = "beststories" },
	{ id = "ask", title = "Ask", path = "askstories" },
	{ id = "show", title = "Show", path = "showstories" },
}

-- Line icons on a 24-point grid, tinted by the node.
local function icon(paths: string): string
	return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">'
		.. paths
		.. "</svg>"
end
local ICONS = {
	back = icon('<path d="M14.5 6l-6 6 6 6"/>'),
	refresh = icon('<path d="M19 9a7.5 7.5 0 0 0-13-3L4 8m0-4v4h4m-3 7a7.5 7.5 0 0 0 13 3l2-2m0 4v-4h-4"/>'),
	external = icon('<path d="M14 5h5v5M19 5l-8 8M17 14v4a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h4"/>'),
	comments = icon(
		'<path d="M5 6.5A2.5 2.5 0 0 1 7.5 4h9A2.5 2.5 0 0 1 19 6.5v6a2.5 2.5 0 0 1-2.5 2.5H11l-4 4v-4h0.5A2.5 2.5 0 0 1 5 12.5z"/>'
	),
	up = icon('<path d="M12 6l6 8H6z"/>'),
	article = icon(
		'<path d="M7 4h10a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"/><path d="M9 8h6M9 12h6M9 16h3"/>'
	),
	person = icon('<circle cx="12" cy="8.5" r="3.5"/><path d="M5 19.5c1.2-3.3 3.8-5 7-5s5.8 1.7 7 5"/>'),
}

type Story = {
	id: number,
	title: string,
	url: string?,
	domain: string,
	by: string,
	score: number,
	comments: number,
	time: number,
	text: string?,
	kids: { number },
}
type Route = { view: string, story: Story?, name: string? }
type Run = { text: string, link: boolean?, italic: boolean?, code: boolean? }
type Paragraph = { runs: { Run }, code: boolean? }
type Comment = { id: number, depth: number, by: string, time: number, body: { Paragraph }, replies: number }

-- Text -----------------------------------------------------------------

local ENTITIES = { quot = '"', amp = "&", lt = "<", gt = ">", apos = "'", nbsp = " " }
local function entities(s: string): string
	s = s:gsub("&#[xX](%x+);", function(hex)
		return utf8.char(tonumber(hex, 16) or 63)
	end)
	s = s:gsub("&#(%d+);", function(n)
		return utf8.char(tonumber(n) or 63)
	end)
	return (s:gsub("&(%a+);", function(name)
		return ENTITIES[name] or `&{name};`
	end))
end
-- HN's HTML (paragraphs, links, italics, code) as paragraphs of runs.
local function parse(html: string?): { Paragraph }
	local out: { Paragraph } = {}
	if not html or html == "" then
		return out
	end
	for piece in (html .. "<p>"):gmatch("(.-)<p>") do
		if piece:find("^%s*<pre>") then
			local code = piece:gsub("<[^>]+>", "")
			table.insert(out, { runs = { { text = entities(code):gsub("%s+$", ""), code = true } }, code = true })
		elseif piece ~= "" then
			local runs: { Run } = {}
			local italic, link, pos = false, false, 1
			while pos <= #piece do
				local s, e, tag = piece:find("<(/?%a+)[^>]*>", pos)
				local stop = if s then s - 1 else #piece
				if stop >= pos then
					table.insert(runs, { text = entities(piece:sub(pos, stop)), italic = italic, link = link })
				end
				if not s then
					break
				end
				if tag == "i" then
					italic = true
				elseif tag == "/i" then
					italic = false
				elseif tag == "a" then
					link = true
				elseif tag == "/a" then
					link = false
				end
				pos = e + 1
			end
			table.insert(out, { runs = runs })
		end
	end
	return out
end

local function domain(url: string?): string
	if not url then
		return ""
	end
	local host = url:match("^%a[%w+.-]*://([^/?#:]+)") or ""
	return (host:gsub("^www%.", ""))
end
local function ago(time: number): string
	local now = if os and os.time then os.time() else time
	local s = math.max(0, now - time)
	if s < 3600 then
		return `{math.max(1, s // 60)}m`
	elseif s < 86400 then
		return `{s // 3600}h`
	elseif s < 86400 * 30 then
		return `{s // 86400}d`
	elseif s < 86400 * 365 then
		return `{s // (86400 * 30)}mo`
	end
	return `{s // (86400 * 365)}y`
end
local function plural(n: number, one: string): string
	return if n == 1 then `1 {one}` else `{n} {one}s`
end

-- Data -----------------------------------------------------------------

-- What has been fetched, for every pane showing the plugin: going back
-- shows it at once.
local feeds: { [string]: { ids: { number }, stories: { Story } } } = {}
local threads: { [number]: { Comment } } = {}

local function story_of(item: any): Story?
	if type(item) ~= "table" or not item.title then
		return nil
	end
	local kids = {}
	for _, kid in item.kids or {} do
		table.insert(kids, kid)
	end
	return {
		id = item.id,
		title = item.title,
		url = item.url,
		domain = domain(item.url),
		by = item.by or "",
		score = item.score or 0,
		comments = item.descendants or 0,
		time = item.time or 0,
		text = item.text,
		kids = kids,
	}
end
-- Stories `from` to `to` of the feed's ids, all requested at once.
local function fetch_stories(ids: { number }, from: number, to: number): { Story }
	local pending = {}
	for i = from, math.min(to, #ids) do
		table.insert(pending, ui.fetch(`{API}/item/{ids[i]}.json`))
	end
	local out: { Story } = {}
	for _, request in pending do
		local ok, response = pcall(function()
			return request:await()
		end)
		local story = if ok then story_of(response.json) else nil
		if story then
			table.insert(out, story)
		end
	end
	return out
end
-- A story's discussion, one request (Algolia keeps the whole tree), in
-- HN's order: depth first, each comment with how many replies it has.
local function fetch_thread(story: Story): { Comment }
	local tree = ui.fetch(`{ALGOLIA}/items/{story.id}`):await().json
	local order: { [number]: number } = {}
	for i, kid in story.kids do
		order[kid] = i
	end
	local out: { Comment } = {}
	local function walk(children: any, depth: number): number
		local list = {}
		for _, child in children or {} do
			table.insert(list, child)
		end
		if depth == 0 then
			table.sort(list, function(a, b)
				return (order[a.id] or math.huge) < (order[b.id] or math.huge)
			end)
		end
		local count = 0
		for _, child in list do
			if child.text and child.author then
				local comment: Comment = {
					id = child.id,
					depth = depth,
					by = child.author,
					time = child.created_at_i or 0,
					body = parse(child.text),
					replies = 0,
				}
				table.insert(out, comment)
				comment.replies = walk(child.children, depth + 1)
				count += 1 + comment.replies
			end
		end
		return count
	end
	walk(tree and tree.children, 0)
	return out
end

-- Pieces -----------------------------------------------------------------

local function glyph(svg: string, size: number, tint: string): Node
	return ui.svg { code = svg, w = size, h = size, shrink = 0, tint = tint }
end
local function tool(svg: string, label: string, on_click: () -> ()): Node
	return ui.button {
		id = "hn-" .. label:lower(),
		w = 28,
		h = 28,
		rounded = 7,
		items = "center",
		justify = "center",
		shrink = 0,
		bg = "#00000000",
		hover = { bg = "$hover" },
		focus = { border = 0 },
		label = label,
		on_click = on_click,
		glyph(svg, 15, "$text.secondary"),
	}
end
local function pill(svg: string, label: string, primary: boolean, on_click: () -> ()): Node
	return ui.button {
		id = if primary then "hn-primary" else nil,
		h = 28,
		px = 11,
		gap = 6,
		rounded = 8,
		items = "center",
		bg = if primary then ORANGE .. "26" else "$pressed",
		border = 1,
		border_color = if primary then ORANGE .. "40" else "#ffffff0f",
		hover = { bg = if primary then ORANGE .. "38" else "$hover" },
		focus = { border = 0 },
		on_click = on_click,
		glyph(svg, 14, if primary then ORANGE else "$text.secondary"),
		ui.text { label, text_size = 12, weight = 500, color = if primary then "#ffb58a" else "$text.primary" },
	}
end
-- A paragraph of runs: links in orange, code in mono.
local function paragraph(p: Paragraph, size: number): Node
	if p.code then
		return ui.text {
			p.runs[1].text,
			font = "mono",
			text_size = size - 1,
			wrap = true,
			p = 8,
			rounded = 6,
			bg = "#ffffff0a",
			color = "$text.secondary",
		}
	end
	local props: any = { text_size = size, line_height = size * 1.5, wrap = true, color = "#d9dce3" }
	for _, run in p.runs do
		table.insert(
			props,
			ui.span {
				run.text,
				italic = run.italic or nil,
				color = if run.link then "#ff9a5c" else nil,
			}
		)
	end
	return ui.rich(props)
end
local function skeleton(n: number): Node
	local rows: any = { gap = 18, px = 16, pt = 14 }
	for i = 1, n do
		table.insert(
			rows,
			ui.column {
				gap = 8,
				ui.div { h = 12, w = `{58 + (i * 23) % 35}%`, rounded = 4, bg = "#ffffff12" },
				ui.div { h = 9, w = `{28 + (i * 17) % 20}%`, rounded = 4, bg = "#ffffff0a" },
			}
		)
	end
	return ui.column(rows)
end

-- The panel ----------------------------------------------------------------

type Props = { slot: { pane: string, radius: number? } }

local HackerNews = ui.component(function(props: Props)
	local pane = props.slot.pane
	local radius = props.slot.radius or 9
	-- Where the pane is, kept when it moves or the plugin reloads.
	local stack = ui.state(`stack:{pane}`, { { view = "feed" } } :: { Route })
	local feed = ui.state(`feed:{pane}`, "top")
	-- Bumped by Refresh: the list is fetched again.
	local generation = ui.use_state(0)
	local offset = ui.state(`offset:{pane}`, 0)
	-- Which way the last move went: a pushed view slides in from the right.
	local direction = 1
	local article = "article:" .. pane

	local function route(): Route
		local list = stack:get()
		return list[#list]
	end
	local function push(next: Route)
		direction = 1
		stack:push(next)
	end
	local function back()
		local list = stack:peek()
		if #list <= 1 then
			return
		end
		direction = -1
		if list[#list].view == "article" then
			-- The page ends with the view (a page left open costs memory).
			ui.web(article):close()
		end
		stack:remove(#list)
	end
	local function slide_in(el: Element)
		anime.animate(el, { opacity = { 0, 1 }, x = { 14 * direction, 0 }, duration = 200, ease = "outQuart" })
	end

	-- The bar: back, where you are, and what it does there.
	local function bar(title: () -> string, extra: Node?): Node
		return ui.row {
			h = 40,
			pl = 8,
			pr = 8,
			gap = 6,
			items = "center",
			shrink = 0,
			ui.show(function()
				return #stack:get() > 1
			end, tool(ICONS.back, "Back", back)),
			ui.row {
				w = 20,
				h = 20,
				ml = 4,
				shrink = 0,
				rounded = 5,
				items = "center",
				justify = "center",
				bg = ORANGE,
				ui.text { "Y", text_size = 12.5, bold = true, color = "#ffffff" },
			},
			ui.text {
				title,
				grow = 1,
				shrink = 1,
				min_w = 0,
				ml = 4,
				text_size = 13,
				weight = 600,
				color = "$text.primary",
				truncate = true,
			},
			extra,
		}
	end
	local function divider(): Node
		return ui.div { h = 1, shrink = 0, bg = "#ffffff0d" }
	end

	-- Feeds ------------------------------------------------------------------

	-- One feed's stories (built again when the feed changes or refreshes).
	local function feed_list(which: string): Node
		local cached = feeds[which]
		local stories = ui.use_state(if cached then cached.stories else {} :: { Story })
		local status = ui.use_state(if cached then "" else "loading")
		local function load(more: boolean)
			status:set(if more then "more" else "loading")
			ui.task(function()
				local ok, err = pcall(function()
					local entry = feeds[which]
					if not more or not entry then
						local path = ""
						for _, f in FEEDS do
							if f.id == which then
								path = f.path
							end
						end
						local ids = {}
						for _, id in ui.fetch(`{API}/{path}.json`):await().json or {} do
							table.insert(ids, id)
						end
						entry = { ids = ids, stories = {} }
						feeds[which] = entry
					end
					local have = #entry.stories
					for _, story in fetch_stories(entry.ids, have + 1, have + PAGE) do
						table.insert(entry.stories, story)
					end
					stories:set(entry.stories)
				end)
				status:set(if ok then "" else `Could not load: {err}`)
			end)
		end
		if not cached then
			load(false)
		end
		local function row(story: Story, i: number): Node
			return ui.row {
				id = `story-{story.id}`,
				px = 10,
				py = 9,
				gap = 10,
				items = "start",
				rounded = 9,
				cursor = "pointer",
				hover = { bg = "#ffffff08" },
				on_click = function()
					push { view = "story", story = story }
				end,
				ui.text {
					tostring(i),
					w = 22,
					mt = 1,
					shrink = 0,
					align = "right",
					tabular = true,
					text_size = 12,
					color = "$text.disabled",
				},
				ui.column {
					grow = 1,
					shrink = 1,
					min_w = 0,
					gap = 4,
					ui.rich {
						wrap = true,
						text_size = 13.5,
						line_height = 19,
						weight = 500,
						color = "$text.primary",
						ui.span { story.title },
						ui.span {
							if story.domain ~= "" then `  {story.domain}` else "",
							text_size = 11.5,
							weight = 400,
							color = "$text.tertiary",
						},
					},
					ui.row {
						gap = 10,
						items = "center",
						ui.row {
							gap = 3,
							items = "center",
							glyph(ICONS.up, 10, ORANGE),
							ui.text {
								tostring(story.score),
								tabular = true,
								text_size = 11.5,
								color = "$text.secondary",
							},
						},
						ui.text { story.by, text_size = 11.5, color = "$text.tertiary" },
						ui.text { ago(story.time), text_size = 11.5, color = "$text.tertiary" },
						ui.row {
							gap = 4,
							items = "center",
							glyph(ICONS.comments, 12, "$text.tertiary"),
							ui.text {
								tostring(story.comments),
								tabular = true,
								text_size = 11.5,
								color = "$text.tertiary",
							},
						},
					},
				},
			}
		end
		return ui.column {
			grow = 1,
			min_h = 0,
			items = "stretch",
			ui.show(function()
				return status:get() == "loading"
			end, skeleton(9)),
			ui.text {
				function()
					local s = status:get()
					return if s ~= "loading" and s ~= "more" then s else ""
				end,
				hidden = function()
					local s = status:get()
					return s == "" or s == "loading" or s == "more"
				end,
				p = 16,
				wrap = true,
				text_size = 12,
				color = "$danger",
			},
			ui.scroll {
				grow = 1,
				w_full = true,
				offset = offset,
				px = 6,
				py = 6,
				-- Titles, sites and bylines select (a press still opens a
				-- story; a drag selects).
				selectable = true,
				hidden = function()
					return status:get() == "loading"
				end,
				ui.each(stories, row),
				ui.row {
					py = 10,
					justify = "center",
					selectable = false,
					hidden = function()
						local entry = feeds[which]
						return #stories:get() == 0 or (entry ~= nil and #stories:get() >= #entry.ids)
					end,
					ui.button {
						function()
							return if status:get() == "more" then "Loading…" else "More stories"
						end,
						h = 28,
						px = 14,
						rounded = 8,
						text_size = 12,
						color = "$text.secondary",
						bg = "$pressed",
						hover = { bg = "$hover" },
						focus = { border = 0 },
						on_click = function()
							if status:peek() == "" then
								load(true)
							end
						end,
					},
				},
			},
		}
	end

	-- The feeds: the bar, the feeds' tabs (the highlight slides between
	-- them) and the chosen feed's list.
	local function feed_screen(): Node
		local tabs: any = { h = 36, px = 10, gap = 2, items = "center", shrink = 0 }
		for _, f in FEEDS do
			local function on(): boolean
				return feed:get() == f.id
			end
			table.insert(
				tabs,
				ui.row {
					relative = true,
					h = 26,
					px = 11,
					items = "center",
					cursor = "pointer",
					on_click = function()
						if feed:peek() ~= f.id then
							feed:set(f.id)
							offset:set(0)
						end
					end,
					ui.presence(function()
						return if on() then "on" else nil
					end, function(): Node
						return ui.div {
							layout_id = "hn-feed-" .. pane,
							layout_duration = 260,
							layout_ease = "cubicBezier(.22, 1, .36, 1)",
							w_full = true,
							h_full = true,
							rounded = 7,
							bg = "$pressed",
						}
					end, { absolute = true, top = 0, left = 0, right = 0, bottom = 0 }),
					ui.text {
						f.title,
						text_size = 12,
						weight = 500,
						color = function()
							return if on() then "$text.primary" else "$text.tertiary"
						end,
					},
				}
			)
		end
		return ui.column {
			grow = 1,
			min_h = 0,
			items = "stretch",
			on_mount = slide_in,
			bar(
				function()
					return "Hacker News"
				end,
				tool(ICONS.refresh, "Refresh", function()
					feeds[feed:peek()] = nil
					offset:set(0)
					generation:set(generation:peek() + 1)
				end)
			),
			ui.row(tabs),
			divider(),
			ui.presence(function()
				return `{feed:get()}#{generation:get()}`
			end, function(key: string): Node
				return feed_list((key:match("^(%a+)#") or "top"))
			end, { grow = 1, min_h = 0, enter = { opacity = { 0, 1 }, duration = 160 } }),
		}
	end

	-- A story and its discussion ---------------------------------------------

	local function story_view(story: Story): Node
		local comments = ui.use_state(threads[story.id] or {} :: { Comment })
		local status = ui.use_state(if threads[story.id] then "" else "loading")
		local folded = ui.use_state({} :: { [string]: boolean })
		local limit = ui.use_state(80)
		if not threads[story.id] then
			ui.task(function()
				local ok, result = pcall(fetch_thread, story)
				if ok then
					threads[story.id] = result
					comments:set(result)
					status:set("")
				else
					status:set(`Could not load the discussion: {result}`)
				end
			end)
		end
		-- The comments shown: none inside a folded one, up to `limit`.
		local shown = ui.computed(function(): { Comment }
			local all, fold, max = comments:get(), folded:get(), limit:get()
			local out: { Comment } = {}
			local skip_below: number? = nil
			for _, c in all do
				if skip_below and c.depth > skip_below then
					continue
				end
				skip_below = if fold[tostring(c.id)] then c.depth else nil
				table.insert(out, c)
				if #out >= max then
					break
				end
			end
			return out
		end)
		local THREAD = { "#ff6a1f66", "#6ea8ff55", "#b69cff55", "#5fcf8a55", "#e8b34b55" }
		local function comment(c: Comment): Node
			local key = tostring(c.id)
			local function is_folded(): boolean
				return folded:get()[key] == true
			end
			local body: any = { gap = 8 }
			for _, p in c.body do
				table.insert(body, paragraph(p, 12.5))
			end
			return ui.row {
				pl = 16 + c.depth * 14,
				pr = 16,
				pt = 10,
				gap = 0,
				items = "stretch",
				ui.div {
					w = 2,
					shrink = 0,
					mr = 10,
					rounded = 1,
					bg = if c.depth > 0 then THREAD[(c.depth - 1) % #THREAD + 1] else "#00000000",
				},
				ui.column {
					grow = 1,
					shrink = 1,
					min_w = 0,
					gap = 5,
					ui.row {
						gap = 8,
						items = "center",
						ui.text {
							c.by,
							text_size = 12,
							weight = 600,
							color = if c.by == story.by then ORANGE else "$text.secondary",
							cursor = "pointer",
							on_click = function()
								push { view = "user", name = c.by }
							end,
						},
						ui.text { ago(c.time), text_size = 11.5, color = "$text.disabled" },
						ui.button {
							function()
								return if is_folded() then `[+{c.replies + 1}]` else "[–]"
							end,
							selectable = false,
							h = 18,
							px = 4,
							rounded = 4,
							text_size = 11,
							color = "$text.tertiary",
							bg = "#00000000",
							hover = { bg = "$hover" },
							focus = { border = 0 },
							on_click = function()
								folded:set_key(key, if is_folded() then nil else true)
							end,
						},
					},
					ui.column {
						hidden = is_folded,
						gap = 8,
						table.unpack(body),
					},
				},
			}
		end
		local text: any = { gap = 8, mt = 4 }
		for _, p in parse(story.text) do
			table.insert(text, paragraph(p, 13))
		end
		return ui.column {
			grow = 1,
			min_h = 0,
			items = "stretch",
			on_mount = slide_in,
			bar(function()
				return if story.domain ~= "" then story.domain else "Discussion"
			end),
			divider(),
			ui.scroll {
				grow = 1,
				w_full = true,
				pb = 16,
				-- The story and its discussion read as one document.
				selectable = true,
				ui.column {
					px = 16,
					pt = 14,
					pb = 14,
					gap = 10,
					ui.text {
						story.title,
						wrap = true,
						text_size = 17,
						line_height = 23,
						weight = 600,
						color = "$text.primary",
					},
					ui.row {
						gap = 10,
						items = "center",
						ui.row {
							gap = 3,
							items = "center",
							glyph(ICONS.up, 11, ORANGE),
							ui.text { plural(story.score, "point"), text_size = 12, color = "$text.secondary" },
						},
						ui.text {
							story.by,
							text_size = 12,
							color = "$text.secondary",
							cursor = "pointer",
							on_click = function()
								push { view = "user", name = story.by }
							end,
						},
						ui.text { ago(story.time) .. " ago", text_size = 12, color = "$text.tertiary" },
					},
					ui.column(text),
					ui.row {
						mt = 4,
						gap = 8,
						selectable = false,
						hidden = story.url == nil,
						pill(ICONS.article, "Read article", true, function()
							push { view = "article", story = story }
						end),
					},
				},
				divider(),
				ui.text {
					function()
						local s = status:get()
						if s == "loading" then
							return "Loading the discussion…"
						elseif s ~= "" then
							return s
						end
						return plural(story.comments, "comment")
					end,
					px = 16,
					pt = 12,
					text_size = 11.5,
					weight = 600,
					letter_spacing = 0.3,
					color = "$text.tertiary",
				},
				ui.each(shown, comment),
				ui.row {
					pt = 14,
					justify = "center",
					selectable = false,
					hidden = function()
						return #shown:get() < limit:get()
					end,
					ui.button {
						"More comments",
						h = 28,
						px = 14,
						rounded = 8,
						text_size = 12,
						color = "$text.secondary",
						bg = "$pressed",
						hover = { bg = "$hover" },
						focus = { border = 0 },
						on_click = function()
							limit:set(limit:peek() + 80)
						end,
					},
				},
			},
		}
	end

	-- The article ---------------------------------------------------------------

	local function article_view(story: Story): Node
		local page = ui.web(article, { profile = "articles" })
		if page.url ~= story.url then
			page:load(story.url or "about:blank")
		end
		return ui.column {
			grow = 1,
			min_h = 0,
			items = "stretch",
			on_mount = slide_in,
			bar(
				function()
					local title = page.title
					return if title ~= "" then title else story.domain
				end,
				ui.row {
					gap = 4,
					items = "center",
					pill(ICONS.comments, tostring(story.comments), false, function()
						back()
					end),
				}
			),
			-- How far the page has loaded, while it loads.
			ui.div {
				h = 2,
				shrink = 0,
				bg = "#ffffff08",
				ui.div {
					h_full = true,
					bg = ORANGE,
					w = function()
						return if page.loading then `{math.floor(page.progress * 100)}%` else "0%"
					end,
				},
			},
			ui.overlay {
				grow = 1,
				min_h = 0,
				ui.webview {
					web = page,
					absolute = true,
					left = 0,
					top = 0,
					right = 0,
					bottom = 0,
					rounded_bl = radius,
					rounded_br = radius,
					on_new_window = function(url)
						page:load(url)
					end,
				},
			},
		}
	end

	-- A person -------------------------------------------------------------------

	local function user_view(name: string): Node
		local user = ui.use_state(nil :: any)
		local status = ui.use_state("loading")
		ui.task(function()
			local ok, result = pcall(function()
				return ui.fetch(`{API}/user/{name}.json`):await().json
			end)
			user:set(if ok then result else nil)
			status:set(if ok then "" else `Could not load: {result}`)
		end)
		return ui.column {
			grow = 1,
			min_h = 0,
			items = "stretch",
			on_mount = slide_in,
			bar(function()
				return name
			end),
			divider(),
			ui.scroll {
				grow = 1,
				w_full = true,
				px = 16,
				py = 16,
				gap = 12,
				selectable = true,
				ui.row {
					gap = 12,
					items = "center",
					ui.row {
						w = 44,
						h = 44,
						rounded = 22,
						items = "center",
						justify = "center",
						bg = ORANGE .. "26",
						glyph(ICONS.person, 22, ORANGE),
					},
					ui.column {
						gap = 3,
						ui.text { name, text_size = 16, weight = 600, color = "$text.primary" },
						ui.text {
							function()
								local u = user:get()
								if not u then
									return if status:get() == "loading" then "Loading…" else status:get()
								end
								return `{u.karma or 0} karma · joined {ago(u.created or 0)} ago`
							end,
							text_size = 12,
							color = "$text.tertiary",
						},
					},
				},
				ui.presence(function()
					local u = user:get()
					return if u and u.about then u.about else nil
				end, function(about: string): Node
					local out: any = { gap = 8 }
					for _, p in parse(about) do
						table.insert(out, paragraph(p, 13))
					end
					return ui.column(out)
				end),
			},
		}
	end

	return ui.column {
		grow = 1,
		min_h = 0,
		items = "stretch",
		-- The current view; the feed comes back as it was left.
		ui.presence(route, function(r: Route): Node
			if r.view == "story" and r.story then
				return story_view(r.story)
			elseif r.view == "article" and r.story then
				return article_view(r.story)
			elseif r.view == "user" and r.name then
				return user_view(r.name)
			end
			return feed_screen()
		end, { grow = 1, min_h = 0 }),
	}
end)

ui.contribute("panel:front-page", HackerNews {})
