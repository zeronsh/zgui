-- A tab's panes as a tree of splits. Plain data (kept in ui.state, so it
-- survives reloads); every change returns a new tree.
--
--   { kind = "pane", id = "p3" }
--   { kind = "split", id = "s2", axis = "row" | "column", a = node, b = node }
--
-- `row` lays a beside b, `column` a above b. Ratios live apart (keyed by
-- split id) so dragging a divider restyles without rebuilding the tree.
local M = {}

export type Axis = "row" | "column"
export type Pane = { kind: "pane", id: string }
export type Split = { kind: "split", id: string, axis: Axis, a: Tree, b: Tree }
export type Tree = Pane | Split

export type Rect = { x: number, y: number, w: number, h: number }
-- A place along one side of the content area, whatever its size: `p` of
-- its length, plus `q` points. Every edge a split tree makes is one (ratios
-- scale lengths, gaps add points), so the frames hold at every size.
export type Edge = { p: number, q: number }
-- A box by its edges, from the content area's top left.
export type Box = { left: Edge, right: Edge, top: Edge, bottom: Edge }
-- A divider's box, and the length its split shares out (for dragging).
export type Divider = Box & { id: string, axis: Axis, length: Edge }
-- Where a tree's panes (by id) and dividers (in order) go.
export type Frames = { panes: { [string]: Box }, dividers: { Divider } }

function M.pane(id: string): Tree
	return { kind = "pane", id = id }
end

-- Pane ids in reading order.
function M.leaves(node: Tree, out: { string }?): { string }
	local list = out or {}
	if node.kind == "pane" then
		table.insert(list, node.id)
	else
		M.leaves(node.a, list)
		M.leaves(node.b, list)
	end
	return list
end

function M.contains(node: Tree, pane: string): boolean
	return table.find(M.leaves(node), pane) ~= nil
end

-- Its structure, for rebuilding only when it changes.
function M.signature(node: Tree): string
	if node.kind == "pane" then
		return node.id
	end
	return `{node.id}{node.axis == "row" and "|" or "-"}({M.signature(node.a)},{M.signature(node.b)})`
end

-- `node` with the pane `pane` replaced by `replacement`.
function M.replace(node: Tree, pane: string, replacement: Tree): Tree
	if node.kind == "pane" then
		return if node.id == pane then replacement else node
	end
	return {
		kind = "split",
		id = node.id,
		axis = node.axis,
		a = M.replace(node.a, pane, replacement),
		b = M.replace(node.b, pane, replacement),
	}
end

-- Split `pane`: `new` goes after it (right or below), or before when `before`.
function M.split(node: Tree, pane: string, axis: Axis, new: string, split_id: string, before: boolean?): Tree
	local fresh = M.pane(new)
	local old = M.pane(pane)
	return M.replace(node, pane, {
		kind = "split",
		id = split_id,
		axis = axis,
		a = if before then fresh else old,
		b = if before then old else fresh,
	})
end

-- `node` without `pane`: its sibling takes its split's place. Returns the
-- new tree (nil when it was the last pane) and the pane next to where it was.
function M.remove(node: Tree, pane: string): (Tree?, string?)
	if node.kind == "pane" then
		return if node.id == pane then nil else node, nil
	end
	local a, b = node.a, node.b
	if a.kind == "pane" and a.id == pane then
		return b, M.leaves(b)[1]
	end
	if b.kind == "pane" and b.id == pane then
		local leaves = M.leaves(a)
		return a, leaves[#leaves]
	end
	-- The pane is deeper on one side, which keeps a node: only the last
	-- pane of a tree removes it all.
	if M.contains(a, pane) then
		local rest, near = M.remove(a, pane)
		return { kind = "split", id = node.id, axis = node.axis, a = rest or a, b = b }, near
	end
	local rest, near = M.remove(b, pane)
	return { kind = "split", id = node.id, axis = node.axis, a = a, b = rest or b }, near
end

type Step = { split: Split, side: "a" | "b" }

-- The path from the root to `pane`: each split and the side taken.
local function path_to(node: Tree, pane: string, path: { Step }): { Step }?
	if node.kind == "pane" then
		return if node.id == pane then path else nil
	end
	path[#path + 1] = { split = node, side = "a" }
	local found = path_to(node.a, pane, path)
	if found then
		return found
	end
	path[#path].side = "b"
	found = path_to(node.b, pane, path)
	if found then
		return found
	end
	table.remove(path)
	return nil
end

export type Direction = "left" | "right" | "up" | "down"

-- The pane beside `pane` in a direction.
function M.neighbor(node: Tree, pane: string, direction: Direction): string?
	local path = path_to(node, pane, {})
	if not path then
		return nil
	end
	local axis = if direction == "left" or direction == "right" then "row" else "column"
	local backward = direction == "left" or direction == "up"
	for i = #path, 1, -1 do
		local step = path[i]
		if step.split.axis == axis and step.side == (if backward then "b" else "a") then
			local other = if backward then step.split.a else step.split.b
			local leaves = M.leaves(other)
			return if backward then leaves[#leaves] else leaves[1]
		end
	end
	return nil
end

-- Put `subtree` beside `target`: after it (right or below), or before.
function M.attach(node: Tree, target: string, axis: Axis, subtree: Tree, split_id: string, before: boolean?): Tree
	local old = M.pane(target)
	return M.replace(node, target, {
		kind = "split",
		id = split_id,
		axis = axis,
		a = if before then subtree else old,
		b = if before then old else subtree,
	})
end

local function edge(p: number, q: number): Edge
	return { p = p, q = q }
end
-- `a + b * k`, edge-wise.
local function plus(a: Edge, b: Edge, k: number): Edge
	return { p = a.p + b.p * k, q = a.q + b.q * k }
end
-- The whole content area.
M.WHOLE = { left = edge(0, 0), right = edge(1, 0), top = edge(0, 0), bottom = edge(1, 0) } :: Box

-- Where every pane and divider goes, `gap` points apart: panes by id,
-- dividers in order. Independent of the area's size: see `Edge`.
function M.frames(node: Tree, ratios: { [string]: number }, gap: number): Frames
	local out: Frames = { panes = {}, dividers = {} }
	local function place(n: Tree, box: Box)
		if n.kind == "pane" then
			out.panes[n.id] = box
			return
		end
		local r = ratios[n.id] or 0.5
		local row = n.axis == "row"
		local first = if row then box.left else box.top
		local last = if row then box.right else box.bottom
		-- What the two sides share: the span less the gap.
		local length = edge(last.p - first.p, last.q - first.q - gap)
		local cut = plus(first, length, r)
		local after = plus(cut, edge(0, gap), 1)
		local function along(from: Edge, to: Edge): Box
			return if row
				then { left = from, right = to, top = box.top, bottom = box.bottom }
				else { left = box.left, right = box.right, top = from, bottom = to }
		end
		place(n.a, along(first, cut))
		local divider = along(cut, after) :: Divider
		divider.id, divider.axis, divider.length = n.id, n.axis, length
		table.insert(out.dividers, divider)
		place(n.b, along(after, last))
	end
	place(node, M.WHOLE)
	return out
end

-- An edge at a length (points).
function M.at(e: Edge, length: number): number
	return e.p * length + e.q
end
-- A box in a `w` x `h` area, in points.
function M.resolve(box: Box, w: number, h: number): Rect
	local x, y = M.at(box.left, w), M.at(box.top, h)
	return { x = x, y = y, w = M.at(box.right, w) - x, h = M.at(box.bottom, h) - y }
end

-- Split ids, for equalizing.
function M.splits(node: Tree, out: { string }?): { string }
	local list = out or {}
	if node.kind == "split" then
		table.insert(list, node.id)
		M.splits(node.a, list)
		M.splits(node.b, list)
	end
	return list
end

return M
