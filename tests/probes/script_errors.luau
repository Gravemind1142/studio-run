-- How errors in threads spawned by a loadstring chunk are reported.
-- Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")
local ScriptContext = game:GetService("ScriptContext")

local reported = {}
local conn = ScriptContext.Error:Connect(function(message, stack, origin)
	if string.find(message, "spawned boom", 1, true) then
		table.insert(reported, {
			message = message,
			stack = stack,
			originIsNil = origin == nil,
		})
	end
end)

local chunk = assert(loadstring("task.spawn(function()\n\terror('spawned boom')\nend)", "=probe_chunk.luau"))
chunk()
task.wait(1)
conn:Disconnect()

print("@@probe@@" .. HttpService:JSONEncode({ reported = reported }))
