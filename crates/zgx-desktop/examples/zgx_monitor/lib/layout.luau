-- The panels as a tree of splits, as the terminal example's panes are.
-- Plain data (kept in ui.state, so it survives reloads); every change
-- returns a new tree.
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
export type Divider = { id: string, axis: Axis, x: number, y: number, w: number, h: number, length: number }
-- Where a tree's panes (by id) and dividers (in order) go.
export type Frames = { panes: { [string]: Rect }, dividers: { Divider } }

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

-- `node` with panes `a` and `b` in each other's places.
function M.swap(node: Tree, a: string, b: string): Tree
	if node.kind == "pane" then
		if node.id == a then
			return M.pane(b)
		end
		return if node.id == b then M.pane(a) else node
	end
	return { kind = "split", id = node.id, axis = node.axis, a = M.swap(node.a, a, b), b = M.swap(node.b, a, b) }
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

-- Where every pane and divider goes in a `w` x `h` box, `gap` apart: panes
-- by id, dividers in order. Sides round to whole points.
function M.frames(node: Tree, ratios: { [string]: number }, w: number, h: number, gap: number): Frames
	local out: Frames = { panes = {}, dividers = {} }
	local function place(n: Tree, x: number, y: number, w: number, h: number)
		if n.kind == "pane" then
			out.panes[n.id] = { x = x, y = y, w = w, h = h }
			return
		end
		local r = ratios[n.id] or 0.5
		if n.axis == "row" then
			local length = math.max(w - gap, 0)
			local a = math.floor(length * r + 0.5)
			place(n.a, x, y, a, h)
			out.dividers[#out.dividers + 1] =
				{ id = n.id, axis = "row", x = x + a, y = y, w = gap, h = h, length = length }
			place(n.b, x + a + gap, y, length - a, h)
		else
			local length = math.max(h - gap, 0)
			local a = math.floor(length * r + 0.5)
			place(n.a, x, y, w, a)
			out.dividers[#out.dividers + 1] =
				{ id = n.id, axis = "column", x = x, y = y + a, w = w, h = gap, length = length }
			place(n.b, x, y + a + gap, w, length - a)
		end
	end
	place(node, 0, 0, w, h)
	return out
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
