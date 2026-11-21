-- Probe: can we redirect print/warn inside ModuleScripts without reimplementing require?
-- Expects experiments/modules.rbxlx (ReplicatedStorage.Greeter).
local HttpService = game:GetService("HttpService")
local LogService = game:GetService("LogService")
local SENTINEL = "@@probe@@"
local events, results = {}, {}

local function rec(kind, ...)
	local p = table.pack(...)
	for i = 1, p.n do p[i] = tostring(p[i]) end
	table.insert(events, { kind = kind, message = table.concat(p, " ", 1, p.n) })
end
LogService.MessageOut:Connect(function(m, t)
	if string.sub(m, 1, #SENTINEL) ~= SENTINEL and string.sub(m, 1, 2) ~= "> " then
		table.insert(events, { kind = "MessageOut." .. t.Name, message = m })
	end
end)

local function fresh(tag)
	local m = game.ReplicatedStorage.Greeter:Clone()
	m.Name = "Greeter_" .. tag
	m.Parent = game.ReplicatedStorage
	return m
end
local function try(name, f)
	local ok, err = pcall(f)
	results[name] = ok and "ok" or ("ERR: " .. tostring(err))
end

local myEnv = getfenv(1)
local mt = getmetatable(myEnv)
results.myEnvHasMetatable = tostring(mt ~= nil)

-- A. Mutate the shared globals table that script envs fall back to.
try("A_mutateSharedGlobals", function()
	local shared = mt and mt.__index
	assert(type(shared) == "table", "no shared __index table: " .. typeof(mt and mt.__index))
	rawset(shared, "print", function(...) rec("A.print", ...) end)
end)
try("A_require", function() require(fresh("A")) end)

-- B. Replace the thread environment, then require.
try("B_setfenv0", function()
	setfenv(0, setmetatable({ print = function(...) rec("B.print", ...) end }, { __index = myEnv }))
end)
try("B_require", function() require(fresh("B")) end)
pcall(setfenv, 0, myEnv)

-- C. Real require, then patch the module's function envs afterwards.
local M
try("C_require", function() M = require(fresh("C")) end)
try("C_getfenvShared", function()
	local e = getfenv(M.greet)
	results.C_envIsMine = tostring(e == myEnv)
	results.C_envHasScript = tostring(rawget(e, "script") ~= nil)
	e.print = function(...) rec("C.envTable.print", ...) end
	M.greet("C1")
end)
try("C_setfenvFunction", function()
	setfenv(M.greet, setmetatable({ print = function(...) rec("C.setfenv.print", ...) end }, { __index = getfenv(M.greet) }))
	M.greet("C2")
end)

-- D. Load the module's own Source with loadstring + our env (the "own require" approach), for comparison.
try("D_loadstringSource", function()
	local src = game.ReplicatedStorage.Greeter.Source
	local fn = assert(loadstring(src, "=ReplicatedStorage.Greeter"))
	setfenv(fn, setmetatable({ print = function(...) rec("D.print", ...) end, script = game.ReplicatedStorage.Greeter }, { __index = myEnv }))
	local mod = fn()
	mod.greet("D")
	local ok, e = pcall(mod.boom)
	rec("D.boomError", e)
end)

task.wait(1)
print(SENTINEL .. HttpService:JSONEncode({ results = results, events = events }))
