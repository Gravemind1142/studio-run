-- What a RunScript script can use. Run through `studio-run run --no-capture`.
-- Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")

local function works(f)
	local ok = pcall(f)
	return ok
end

local module = Instance.new("ModuleScript")
module.Source = "return 42"

-- A required module calling loadstring, as jest-roblox's runtime does.
local loader = Instance.new("ModuleScript")
loader.Source = "return function() return loadstring('return 1')() end"

print("@@probe@@" .. HttpService:JSONEncode({
	loadstring = works(function()
		assert(loadstring("return 1")() == 1)
	end),
	moduleLoadstring = works(function()
		assert(require(loader)() == 1)
	end),
	-- Scripts can't even read it: "not a valid member".
	loadStringEnabledReadable = works(function()
		return game:GetService("ServerScriptService").LoadStringEnabled
	end),
	setfenv = works(function()
		setfenv(function() end, {})
	end),
	debugInfo = works(function()
		assert(debug.info(1, "l"))
	end),
	debugTraceback = works(function()
		assert(type(debug.traceback()) == "string")
	end),
	messageOut = works(function()
		game:GetService("LogService").MessageOut:Connect(function() end):Disconnect()
	end),
	scriptContextError = works(function()
		game:GetService("ScriptContext").Error:Connect(function() end):Disconnect()
	end),
	readModuleSource = works(function()
		assert(module.Source == "return 42")
	end),
	processService = works(function()
		assert(game:GetService("ProcessService"))
	end),
	debugLoadmodule = works(function()
		debug.loadmodule(module)
	end),
	fileSystemService = works(function()
		assert(game:GetService("FileSystemService"))
	end),
	scriptGlobal = typeof(script),
}))
