-- Ways to redirect print() inside a ModuleScript loaded with the real require.
-- Needs tests/fixtures/modules.rbxlx (ReplicatedStorage.Greeter, which prints
-- at top level and in greet()). Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")

local captured = {}
local function capture(tag)
	return function(...)
		captured[tag] = (captured[tag] or 0) + 1
	end
end
local function fresh(tag)
	local m = game.ReplicatedStorage.Greeter:Clone()
	m.Name = "Greeter_" .. tag
	m.Parent = game.ReplicatedStorage
	return m
end

local myEnv = getfenv(1)
local result = {}

-- A. Change the globals table that every script's environment falls back to.
local mt = getmetatable(myEnv)
local shared = mt and mt.__index
result.sharedGlobalsWritable = type(shared) == "table" and pcall(rawset, shared, "print", capture("A"))
pcall(function()
	require(fresh("A"))
end)
result.sharedGlobalsAffectModules = captured.A ~= nil

-- B. Replace the calling thread's environment, then require.
pcall(setfenv, 0, setmetatable({ print = capture("B") }, { __index = myEnv }))
pcall(function()
	require(fresh("B"))
end)
pcall(setfenv, 0, myEnv)
result.threadEnvAffectsModules = captured.B ~= nil

-- C. Require normally, then patch the module's function environments.
local M = require(fresh("C"))
setfenv(M.greet, setmetatable({ print = capture("C") }, { __index = getfenv(M.greet) }))
M.greet("C")
result.patchAfterRequireWorks = captured.C ~= nil

-- D. Compile the module's Source ourselves with our own environment.
local fn = loadstring(game.ReplicatedStorage.Greeter.Source, "=ReplicatedStorage.Greeter")
setfenv(fn, setmetatable({ print = capture("D"), script = game.ReplicatedStorage.Greeter }, { __index = myEnv }))
fn().greet("D")
result.ownLoaderCapturesTopLevel = (captured.D or 0) >= 2

print("@@probe@@" .. HttpService:JSONEncode(result))
