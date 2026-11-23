-- Can the cloud wrapper compile the user's source as a ModuleScript instead of
-- inlining it (loadstring isn't available in tasks)? Each case puts a source in
-- a new ModuleScript named like a user file, requires it, and reports what came
-- back: error messages, chunk names, line numbers, tracebacks.
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

-- `wrapped`: the user's source becomes the body of a function the module
-- returns, starting on the module's line 1, so line numbers are unchanged.
local function module(name, source, wrapped)
	local m = Instance.new("ModuleScript")
	m.Name = name
	m.Source = if wrapped then "return function(...) " .. source .. "\nend" else source
	return m
end

local function load(name, source, wrapped)
	local ok, value = pcall(require, module(name, source, wrapped))
	return ok, value
end

local results = {}

-- 1. A compile error.
do
	local ok, err = load("tests/compile.luau", "local x = 1\nlocal y = = 2\n", true)
	results.compileError = { ok = ok, err = tostring(err) }
end

-- 2. A runtime error, with a traceback, called with arguments.
do
	local src = "local a, b = ...\nlocal function inner()\n\terror('boom ' .. a .. b)\nend\ninner()\n"
	local ok, fn = load("tests/runtime.luau", src, true)
	local tb
	local callOk, err = xpcall(fn, function(e)
		tb = debug.traceback(nil, 2)
		return e
	end, "x", "y")
	results.runtimeError = { loaded = ok, callOk = callOk, err = tostring(err), traceback = tb }
end

-- 3. Capturing print with setfenv, and returning a value.
do
	local ok, fn = load("tests/print.luau", "print('hello')\nreturn 7\n", true)
	local captured = {}
	setfenv(fn, setmetatable({
		print = function(...)
			table.insert(captured, table.concat({ ... }, " "))
		end,
	}, { __index = getfenv(1) }))
	results.capture = { loaded = ok, returned = fn(), captured = captured }
end

-- 4. Error from a coroutine spawned by the module code (async error attribution).
do
	local ok, fn = load("tests/async.luau", "task.spawn(function()\n\terror('async boom')\nend)\n", true)
	fn()
	task.wait(0.2)
	results.async = { loaded = ok }
end

-- 5. Unwrapped: source verbatim, returning nothing, one value, and two values.
do
	local ok0, v0 = load("tests/none.luau", "local x = 1\n", false)
	local ok1, v1 = load("tests/one.luau", "return 5\n", false)
	local ok2, v2 = load("tests/two.luau", "return 1, 2\n", false)
	local okT, vT = load("tests/types.luau", "export type T = number\nreturn 1\n", false)
	results.verbatim = {
		none = { ok = ok0, v = tostring(v0) },
		one = { ok = ok1, v = tostring(v1) },
		two = { ok = ok2, v = tostring(v2) },
		exportType = { ok = okT, v = tostring(vT) },
	}
end

-- 6. Reading the Source of a module that's part of the place (for --trace-modules).
do
	local ok, v = pcall(function()
		local found = workspace:FindFirstChildWhichIsA("ModuleScript", true)
		return if found then #found.Source else "no module in place"
	end)
	results.placeModuleSource = { ok = ok, v = v }
end

task.wait(0.2)
results.logged = logged
results.contextErrors = contextErrors
return results
