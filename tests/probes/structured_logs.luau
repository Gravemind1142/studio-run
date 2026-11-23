-- How LogService's structured logging (Info/Warn/Error/Output/Log with a
-- context table, and GetLogger) reaches LogService.MessageOut, Logger.MessageOut,
-- ScriptContext.Error and GetLogHistory. Run through `studio-run run --no-capture`.
-- Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")
local LogService = game:GetService("LogService")
local ScriptContext = game:GetService("ScriptContext")

-- A JSON-safe description of any value.
local function describe(v, depth)
	depth = depth or 0
	local t = typeof(v)
	if t == "table" then
		if depth > 3 then
			return { type = "table", truncated = true }
		end
		local out = {}
		for k, val in v do
			out[typeof(k) .. ":" .. tostring(k)] = describe(val, depth + 1)
		end
		return { type = "table", fields = out }
	elseif t == "string" or t == "number" or t == "boolean" then
		return { type = t, value = v }
	elseif t == "nil" then
		return { type = "nil" }
	end
	return { type = t, value = tostring(v) }
end

local function try(f)
	local ok, err = pcall(f)
	return if ok then true else "error: " .. tostring(err)
end

local seen = {} -- events from LogService.MessageOut whose message mentions SLOG
local loggerSeen = {} -- from Logger.MessageOut
local childSeen = {}
local scriptErrors = {}

local function record(into)
	return function(message, messageType, context, ...)
		if string.sub(message, 1, 4) == "SLOG" then
			table.insert(into, {
				message = message,
				messageType = tostring(messageType),
				context = describe(context),
				extraArgs = select("#", ...),
			})
		end
	end
end

local conns = {}
table.insert(conns, LogService.MessageOut:Connect(record(seen)))
table.insert(
	conns,
	ScriptContext.Error:Connect(function(message, stack, origin)
		if string.sub(message, 1, 4) == "SLOG" then
			table.insert(scriptErrors, { message = message, stack = stack, origin = tostring(origin) })
		end
	end)
)

local part = Instance.new("Part")
part.Name = "ProbePart"
local ctx = {
	str = "s",
	num = 1.5,
	bool = true,
	nested = { a = 1, list = { 1, 2, 3 } },
	vec = Vector3.new(1, 2, 3),
	inst = part,
	[1] = "array-key",
}

local calls = {}
calls.output = try(function()
	LogService:Output("SLOG output", ctx)
end)
calls.info = try(function()
	LogService:Info("SLOG info", ctx)
end)
calls.warn = try(function()
	LogService:Warn("SLOG warn", ctx)
end)
calls.error = try(function()
	LogService:Error("SLOG error", ctx)
end)
calls.logWarning = try(function()
	LogService:Log(Enum.MessageType.MessageWarning, "SLOG log-warning", { k = "v" })
end)
calls.infoNoContext = try(function()
	LogService:Info("SLOG info-no-context")
end)
calls.infoEmptyContext = try(function()
	LogService:Info("SLOG info-empty-context", {})
end)
calls.badContextString = try(function()
	LogService:Info("SLOG bad-context-string", "notatable" :: any)
end)
calls.badContextFunction = try(function()
	LogService:Info("SLOG bad-context-function", { f = function() end })
end)
calls.cyclicContext = try(function()
	local c = {}
	c.self = c
	LogService:Info("SLOG cyclic-context", c)
end)
calls.plainPrint = try(function()
	print("SLOG plain-print")
end)
calls.plainWarn = try(function()
	warn("SLOG plain-warn")
end)

local logger
local loggerInfo = {}
calls.getLogger = try(function()
	logger = LogService:GetLogger("ProbeLogger")
	loggerInfo.name = logger.Name
	loggerInfo.fullPath = logger.FullPath
	loggerInfo.className = logger.ClassName
end)
if logger then
	table.insert(conns, logger.MessageOut:Connect(record(loggerSeen)))
	local child
	calls.childLogger = try(function()
		child = logger:GetLogger("Child")
		loggerInfo.childName = child.Name
		loggerInfo.childFullPath = child.FullPath
		loggerInfo.sameLoggerTwice = LogService:GetLogger("ProbeLogger") == logger
	end)
	if child then
		table.insert(conns, child.MessageOut:Connect(record(childSeen)))
	end
	calls.loggerInfo = try(function()
		logger:Info("SLOG logger-info", { who = "logger" })
	end)
	calls.loggerError = try(function()
		logger:Error("SLOG logger-error", { who = "logger" })
	end)
	if child then
		calls.childWarn = try(function()
			child:Warn("SLOG child-warn", { who = "child" })
		end)
	end
end

task.wait(1)
for _, c in conns do
	c:Disconnect()
end

local history = {}
calls.getLogHistory = try(function()
	for _, entry in LogService:GetLogHistory() do
		if typeof(entry) == "table" and string.sub(tostring(entry.message), 1, 4) == "SLOG" then
			table.insert(history, describe(entry))
		end
	end
end)

print("@@probe@@" .. HttpService:JSONEncode({
	calls = calls,
	messageOut = seen,
	loggerMessageOut = loggerSeen,
	childMessageOut = childSeen,
	scriptContextError = scriptErrors,
	logger = loggerInfo,
	history = history,
}))
