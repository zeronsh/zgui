-- Talking to the model: a reply streams in as server-sent events, each
-- `delta` appended to the assistant's message as it comes. Stopping
-- cancels the task, which stops the request.
local state = require("./state")

type Message = state.Message

local URL = "https://chat.example/v1/reply"

local M = {}

local running: Task? = nil

-- Change field `field` of message `id` (a path write: the rest of the
-- conversations are shared, not copied).
local function write(conversation: string, id: string, field: string, value: any)
	state.messages:set_key({ conversation, { id = id }, field }, value)
end

-- The conversation moves to the top of the sidebar.
local function touch(conversation: string, title: string?)
	local at = state.conversations:index_of("id", conversation)
	if not at then
		return
	end
	local c = state.conversations:peek()[at]
	state.conversations:remove(at)
	state.conversations:insert(1, { id = c.id, title = title or c.title, updated = os.time() })
end

function M.new_conversation()
	local id = state.new_id("c")
	state.messages:set_key(id, {})
	state.conversations:insert(1, { id = id, title = "New chat", updated = os.time() })
	state.current:set(id)
end

function M.stop()
	if running then
		running:cancel()
		running = nil
	end
	local conversation = state.streaming:peek()
	if conversation then
		local thread = state.messages:peek()[conversation] or {}
		local last = thread[#thread]
		if last and last.streaming then
			write(conversation, last.id, "streaming", false)
			write(conversation, last.id, "error", "Stopped")
		end
		state.streaming:set(nil)
	end
end

-- Send `text` in the conversation shown, and stream the reply into it.
function M.send(text: string)
	text = text:match("^%s*(.-)%s*$") or ""
	if text == "" or state.streaming:peek() then
		return
	end
	local conversation = state.current:peek()
	local history: { { role: string, text: string } } = {}
	for _, m in state.messages:peek()[conversation] or {} do
		table.insert(history, { role = m.role, text = m.text })
	end
	table.insert(history, { role = "user", text = text })
	local question: Message = { id = state.new_id("m"), role = "user", text = text }
	local reply: Message = { id = state.new_id("m"), role = "assistant", text = "", streaming = true }
	ui.batch(function()
		local at = #(state.messages:peek()[conversation] or {})
		state.messages:set_key({ conversation, at + 1 }, question)
		state.messages:set_key({ conversation, at + 2 }, reply)
		state.drafts:set_key(conversation, nil)
		state.streaming:set(conversation)
		-- A new chat is named for its first question.
		local first = at == 0
		touch(conversation, if first then text:sub(1, 48) else nil)
	end)
	running = ui.task(function()
		local response, err = ui.try(function()
			return ui.fetch(URL, {
				method = "POST",
				body = { messages = history },
				on_event = function(e: ServerEvent)
					if e.event == "delta" then
						state.messages:append({ conversation, { id = reply.id }, "text" }, ui.json.decode(e.data).text)
					end
				end,
			}):await()
		end)
		ui.batch(function()
			write(conversation, reply.id, "streaming", false)
			if err then
				write(conversation, reply.id, "error", err)
			elseif response and not response.ok then
				local body = ui.try(ui.json.decode, response.body)
				write(conversation, reply.id, "error", body and body.error or `HTTP {response.status}`)
			end
			state.streaming:set(nil)
		end)
		running = nil
	end)
end

return M
