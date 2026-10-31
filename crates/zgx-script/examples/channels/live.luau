-- Talking to the server: one WebSocket (`ui.socket`) that reconnects on
-- its own, sends shown at once and confirmed or refused later, earlier
-- pages of a channel loaded as its list reaches its start, and uploads
-- with their progress.
local state = require("./state")

type Message = state.Message
type File = state.File

local M = {}

local made = 0
local function local_id(): string
	made += 1
	return `l{made}`
end

-- A message that arrived from the server, where it belongs; counted unread,
-- and told to the person if they are elsewhere.
local function arrived(message: Message)
	local channel = message.channel
	local list = state.messages:peek()[channel] or {}
	state.messages:set_key({ channel, #list + 1 }, message)
	local away = not ui.window.active:peek()
	if channel ~= state.current:peek() or away then
		state.unread:set_key(channel, (state.unread:peek()[channel] or 0) + 1)
	end
	if away then
		ui.badge(state.total_unread:peek())
		ui.notify({
			title = `{state.name_of(message.from)} in #{channel}`,
			body = message.text,
			tag = channel,
		}):after(nil, function() end)
	end
end

-- The server's own message, keyed as its row (a socket's message is the
-- script's own table, to change before it goes into state).
local function stored(message: any): Message
	message.key = message.key or tostring(message.id)
	return message
end

local typing_cleared: { [string]: () -> () } = {}

local function heard(m: any)
	if m.type == "welcome" then
		ui.batch(function()
			state.me:set(m.me)
			state.channels:set(m.channels)
			local users = {}
			for _, user in m.users do
				users[user.id] = user
			end
			state.users:set(users)
			-- The server's pages, and after them what is still ours alone:
			-- sent and not yet answered, or refused (a reconnect's welcome
			-- must not lose them).
			local messages, before = {}, {}
			local had = state.messages:peek()
			for channel, page in m.history do
				local list = {}
				for _, message in page.items do
					table.insert(list, stored(message))
				end
				for _, message in had[channel] or {} do
					if message.pending or message.failed then
						table.insert(list, message)
					end
				end
				messages[channel] = list
				before[channel] = page.before
			end
			state.messages:set(messages)
			state.before:set(before)
		end)
	elseif m.type == "message" then
		arrived(stored(m.message))
	elseif m.type == "ack" then
		local message = m.message
		message.key = m.id
		state.messages:set_key({ message.channel, { key = m.id } }, message)
	elseif m.type == "refused" then
		for channel, list in state.messages:peek() do
			for _, message in list do
				if message.key == m.id then
					ui.batch(function()
						state.messages:set_key({ channel, { key = m.id }, "pending" }, false)
						state.messages:set_key({ channel, { key = m.id }, "failed" }, m.reason)
					end)
				end
			end
		end
	elseif m.type == "presence" then
		state.users:set_key({ m.user, "online" }, m.online)
	elseif m.type == "typing" then
		state.typing:set_key(m.channel, state.name_of(m.user))
		local clear = typing_cleared[m.channel]
		if clear then
			clear()
		end
		typing_cleared[m.channel] = ui.after(3000, function()
			state.typing:set_key(m.channel, nil)
		end)
	end
end

M.socket = ui.socket("wss://chat.test/socket", {
	json = true,
	on_message = heard,
})

-- Send `text` (with the files uploaded for it) in the channel shown: shown
-- at once, pending until the server answers.
function M.send(text: string, files: { File })
	text = text:match("^%s*(.-)%s*$") or ""
	if text == "" and #files == 0 then
		return
	end
	local channel = state.current:peek()
	local id = local_id()
	local message: Message = {
		id = id,
		key = id,
		channel = channel,
		from = state.me:peek(),
		text = text,
		at = os.time(),
		files = files,
		pending = true,
	}
	local list = state.messages:peek()[channel] or {}
	state.messages:set_key({ channel, #list + 1 }, message)
	M.socket.send { type = "send", id = id, channel = channel, text = text, files = files }
end

-- Send a refused message again.
function M.retry(message: Message)
	ui.batch(function()
		state.messages:set_key({ message.channel, { key = message.key }, "failed" }, nil)
		state.messages:set_key({ message.channel, { key = message.key }, "pending" }, true)
	end)
	M.socket.send {
		type = "send",
		id = message.key,
		channel = message.channel,
		text = message.text,
		files = message.files,
	}
end

-- Load `channel`'s page before its first message, once at a time, until
-- its start: the list keeps its rows in place as they come in above.
local loading: { [string]: boolean } = {}
function M.older(channel: string)
	local before = state.before:peek()[channel]
	if not before or loading[channel] then
		return
	end
	loading[channel] = true
	ui.task(function()
		local response, err = ui.try(function()
			return ui.fetch(`https://chat.test/history?channel={channel}&before={before}`):await()
		end)
		loading[channel] = false
		if err or not response or not response.ok then
			return
		end
		local page = response.json
		local earlier = {}
		-- A fetch's JSON is read-only (a socket's message is the script's
		-- own): copied to key it.
		for _, message in page.items do
			table.insert(earlier, stored(table.clone(message)))
		end
		for _, message in state.messages:peek()[channel] or {} do
			table.insert(earlier, message)
		end
		ui.batch(function()
			state.messages:set_key(channel, earlier)
			state.before:set_key(channel, page.before)
		end)
	end)
end

-- Upload `file` for the next message: its progress shown, cancelled by its
-- ×; done, it waits in the composer until the message is sent.
local tasks: { [string]: Task } = {}
function M.upload(file: any)
	local id = local_id()
	state.uploads:push { id = id, name = file.name, size = file.size, sent = 0 }
	tasks[id] = ui.task(function()
		local response = ui.fetch("https://chat.test/upload", {
			multipart = { file = file },
			on_progress = function(sent: number)
				state.uploads:set_key({ { id = id }, "sent" }, sent)
			end,
		}):await()
		tasks[id] = nil
		if response.ok then
			state.uploads:set_key({ { id = id }, "url" }, response.json.url)
		end
	end)
end

function M.cancel_upload(id: string)
	local task = tasks[id]
	if task then
		task:cancel()
		tasks[id] = nil
	end
	local at = state.uploads:index_of("id", id)
	if at then
		state.uploads:remove(at)
	end
end

-- The uploads done, as the message's files, and the composer cleared of them.
function M.take_uploads(): { File }
	local files: { File } = {}
	for _, upload in state.uploads:peek() do
		if upload.url then
			table.insert(files, { name = upload.name, size = upload.size, type = "", url = upload.url })
		end
	end
	state.uploads:set {}
	return files
end

return M
