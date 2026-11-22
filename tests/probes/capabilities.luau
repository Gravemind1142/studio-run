-- What a RunScript script can use. Run through `studio-run run --no-capture`.
-- Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")

local function works(f)
	local ok = pcall(f)
	return ok
end

local module = Instance.new("ModuleScript")
module.Source = "return 42"

print("@@probe@@" .. HttpService:JSONEncode({
	loadstring = works(function()
		assert(loadstring("return 1")() == 1)
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
