-- The team's API: people, listed, added, changed and removed over HTTP.
-- Each call waits inside a `ui.task` and raises the server's message.
local M = {}

export type Member = {
	id: string,
	name: string,
	email: string,
	role: string,
}

local BASE = "https://team.example/api"

local function request(method: string, path: string, body: unknown?): any
	local response = ui.fetch(BASE .. path, { method = method, body = body }):await()
	if not response.ok then
		local json = response.json :: any
		error(if json and json.error then json.error else `The server answered {response.status}`, 0)
	end
	return response.json
end

function M.list(): { Member }
	return request("GET", "/members")
end

function M.save(member: Member): Member
	if member.id == "" then
		return request("POST", "/members", member)
	end
	return request("PUT", `/members/{member.id}`, member)
end

function M.remove(id: string)
	request("DELETE", `/members/{id}`)
end

return M
