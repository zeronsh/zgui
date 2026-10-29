-- The mail API: messages searched and paged, a thread, the folders.
export type Message = { id: number, from: string, subject: string }
export type Thread = { id: number, from: string, subject: string, body: string }
export type Page = { messages: { Message }, next: number? }

local api = {}
local base = "https://mail.example/api"

local function get(path: string): any
	local response = ui.fetch(`{base}{path}`):await()
	if not response.ok then
		error(
			if response.json and response.json.error
				then response.json.error
				else `The server answered {response.status}`,
			0
		)
	end
	return response.json
end

function api.messages(query: string, cursor: number): Page
	return get(`/messages?q={query}&cursor={cursor}`) :: Page
end
function api.thread(id: number): Thread
	return get(`/threads/{id}`) :: Thread
end
function api.folders(): { string }
	return get("/folders") :: { string }
end
function api.save_folders(folders: { string })
	local response = ui.fetch(`{base}/folders`, { method = "PUT", body = folders }):await()
	if not response.ok then
		error(`The server answered {response.status}`, 0)
	end
end

return api
