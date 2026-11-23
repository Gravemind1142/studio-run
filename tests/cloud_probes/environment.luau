-- What a Luau Execution task can use. Returns one table, which the task
-- reports JSON-serialized in output.results.
local HttpService = game:GetService("HttpService")
local RunService = game:GetService("RunService")

local function try(f)
	local ok, v = pcall(f)
	if ok then
		return if v == nil then true else v
	end
	return "error: " .. tostring(v)
end

local module = Instance.new("ModuleScript")
module.Source = "return 42"

local _, errorMessage = pcall(error, "boom")

return {
	loadstring = try(function()
		return loadstring("return 1")() == 1
	end),
	setfenv = try(function()
		setfenv(function() end, {})
	end),
	debugInfoSource = try(function()
		return debug.info(1, "s")
	end),
	traceback = try(function()
		return debug.traceback()
	end),
	errorMessage = errorMessage,
	messageOut = try(function()
		game:GetService("LogService").MessageOut:Connect(function() end):Disconnect()
	end),
	scriptContextError = try(function()
		game:GetService("ScriptContext").Error:Connect(function() end):Disconnect()
	end),
	readModuleSource = try(function()
		return module.Source == "return 42"
	end),
	requireNewModule = try(function()
		return require(module)
	end),
	runService = {
		isServer = RunService:IsServer(),
		isClient = RunService:IsClient(),
		isStudio = RunService:IsStudio(),
		isEdit = try(function()
			return RunService:IsEdit()
		end),
		isRunning = RunService:IsRunning(),
		isRunMode = try(function()
			return RunService:IsRunMode()
		end),
	},
	httpEnabled = try(function()
		return HttpService.HttpEnabled
	end),
	httpGet = try(function()
		return #HttpService:GetAsync("https://example.com") > 0
	end),
	jsonEncode = try(function()
		return HttpService:JSONEncode({ a = 1 })
	end),
	pluginGlobal = typeof(plugin),
	scriptGlobal = typeof(script),
	coreGui = try(function()
		return #game:GetService("CoreGui"):GetChildren()
	end),
	studioService = try(function()
		return game:GetService("StudioService").Name
	end),
	changeHistory = try(function()
		game:GetService("ChangeHistoryService"):SetWaypoint("probe")
	end),
	dataStoreService = try(function()
		return game:GetService("DataStoreService").Name
	end),
	taskWait = try(function()
		return task.wait(0.1) > 0
	end),
	placeId = game.PlaceId,
	placeVersion = game.PlaceVersion,
	gameId = game.GameId,
	jobId = game.JobId,
	baseplate = workspace:FindFirstChild("Baseplate") ~= nil,
	version = version(),
	luauVersion = _VERSION,
	varargCount = select("#", ...),
	varargType = typeof((...)),
}
