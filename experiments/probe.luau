-- Probe what a wrapper can observe from command-bar permission level.
local HttpService = game:GetService("HttpService")
local LogService = game:GetService("LogService")
local ScriptContext = game:GetService("ScriptContext")

local SENTINEL = "@@probe@@"
local events = {}
local t0 = os.clock()

local function record(kind, fields)
	fields.kind = kind
	fields.t = os.clock() - t0
	table.insert(events, fields)
end

LogService.MessageOut:Connect(function(message, messageType)
	if string.sub(message, 1, #SENTINEL) == SENTINEL then
		return
	end
	record("log", { level = messageType.Name, message = message })
end)

ScriptContext.Error:Connect(function(message, stackTrace, script)
	record("scriptError", {
		message = message,
		stack = stackTrace,
		script = script and script:GetFullName() or "<nil>",
	})
end)

-- Capabilities.
local caps = {
	loadstring = pcall(function() return loadstring("return 1")() end),
	setfenv = pcall(function() setfenv(function() end, {}) end),
	debugInfo = pcall(function() return debug.info(1, "sl") end),
	logHistory = #LogService:GetLogHistory(),
}

-- User-ish code: tagged print via a custom environment.
local userSource = [[
print("user print", 1, 2)
warn("user warn")
task.spawn(function() error("error in spawned thread") end)
local ok, e = pcall(error, "caught error")
print("pcall result", ok, e)
error("uncaught user error")
]]

local userResult = { status = "ok" }
local fn, compileErr = loadstring(userSource, "=user.luau")
if not fn then
	userResult = { status = "compileError", message = compileErr }
else
	local env = setmetatable({
		print = function(...)
			local parts = table.pack(...)
			for i = 1, parts.n do parts[i] = tostring(parts[i]) end
			record("userPrint", { message = table.concat(parts, " ", 1, parts.n) })
		end,
		warn = function(...)
			local parts = table.pack(...)
			for i = 1, parts.n do parts[i] = tostring(parts[i]) end
			record("userWarn", { message = table.concat(parts, " ", 1, parts.n) })
		end,
	}, { __index = getfenv(1) })
	setfenv(fn, env)
	local ok, err = xpcall(fn, function(e)
		return { message = tostring(e), traceback = debug.traceback(nil, 2) }
	end)
	if not ok then
		userResult = { status = "error", message = err.message, traceback = err.traceback }
	end
end

-- Normal prints, to see how MessageOut sees them.
print("plain print from wrapper")
warn("plain warn from wrapper")

task.wait(3) -- give plugins time to emit noise while we're listening

print(SENTINEL .. HttpService:JSONEncode({ caps = caps, result = userResult, events = events }))
