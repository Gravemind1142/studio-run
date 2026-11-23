-- Can the cloud wrapper load a place's ModuleScripts itself, for
-- --trace-modules? The test place is an empty baseplate, so this first creates
-- ReplicatedStorage.Greeter (the module in tests/fixtures/modules.rbxlx). Then
-- it reads the module's Source and loads it as the wrapper would: a new
-- ModuleScript named after the original's full name, with the source wrapped
-- in `return function() ... end`, required, setfenv'd and called once.
local LogService = game:GetService("LogService")
local ScriptContext = game:GetService("ScriptContext")

local logged = {}
LogService.MessageOut:Connect(function(message, messageType)
	table.insert(logged, messageType.Name .. ": " .. message)
end)
local contextErrors = {}
ScriptContext.Error:Connect(function(message, stack)
	table.insert(contextErrors, { message = message, stack = stack })
end)

local function wrapped(name, source)
	local m = Instance.new("ModuleScript")
	m.Name = name
	m.Source = "return function() " .. source .. "\nend"
	return m
end

local results = {}

-- 1. Reading the Source of a module in the DataModel.
do
	local greeter = Instance.new("ModuleScript")
	greeter.Name = "Greeter"
	greeter.Source = [[print("Greeter top-level print")
local M = {}
function M.greet(name)
	print("Greeter.greet says hi to", name)
	return "hi " .. name
end
function M.boom()
	error("Greeter.boom error")
end
return M
]]
	greeter.Parent = game:GetService("ReplicatedStorage")
end
local original = game:GetService("ReplicatedStorage"):FindFirstChild("Greeter")
results.found = original ~= nil
local readOk, source = pcall(function()
	return original.Source
end)
results.readSource = { ok = readOk, length = if readOk then #source else tostring(source) }

-- 2. Loading it under its full name, with print captured and `script` set to the original.
if readOk then
	local fullName = original:GetFullName()
	results.fullName = fullName
	local loaded, fn = pcall(require, wrapped(fullName, source))
	local captured = {}
	local scriptSeen
	if loaded then
		setfenv(fn, setmetatable({
			print = function(...)
				local parts = {}
				for i = 1, select("#", ...) do
					parts[i] = tostring((select(i, ...)))
				end
				table.insert(captured, table.concat(parts, " "))
			end,
			script = original,
		}, { __index = getfenv(1) }))
		local M = fn()
		M.greet("probe")
		scriptSeen = M.greet ~= nil
		local tb
		local ok, err = xpcall(M.boom, function(e)
			tb = debug.traceback(nil, 2)
			return e
		end)
		results.boom = { ok = ok, err = tostring(err), traceback = tb }
	end
	results.load = { ok = loaded, captured = captured, returnedTable = scriptSeen }
end

-- 3. A compile error in a module named with dots: does MessageOut name it?
do
	local ok, err = pcall(require, wrapped("ReplicatedStorage.Bad", "local x = = 1\n"))
	results.compileError = { ok = ok, err = tostring(err) }
end

-- 4. An error in a thread spawned by a module named with dots.
do
	local fn = require(wrapped("ReplicatedStorage.Async", "task.spawn(function()\n\terror('async boom')\nend)\nreturn 1\n"))
	results.asyncReturned = fn()
	task.wait(0.2)
end

task.wait(0.2)
results.logged = logged
results.contextErrors = contextErrors
return results
