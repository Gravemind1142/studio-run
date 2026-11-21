-- Probe things a test runner like jest-roblox relies on, under --task RunScript.
local HttpService = game:GetService("HttpService")
local LogService = game:GetService("LogService")
local SENTINEL = "@@probe@@"
local r = {}

-- jest-roblox's entry script sets the exit code via ProcessService:ExitAsync.
local okPS, ps = pcall(function() return game:GetService("ProcessService") end)
r.processService = okPS and tostring(ps) or ("ERR: " .. tostring(ps))
if okPS and ps then
	r.cmdArgs = select(2, pcall(function() return ps:GetCommandLineArgs() end))
end
r.loadmodule = pcall(function() return debug.loadmodule(Instance.new("ModuleScript")) end)
r.FileSystemService = pcall(function() return game:GetService("FileSystemService") end)
r.scriptGlobal = tostring(script)

-- jest-roblox prints its JSON report as one big print(). Is it delivered intact?
local seen = {}
LogService.MessageOut:Connect(function(m)
	if string.sub(m, 1, 5) == "BIG:<" then
		table.insert(seen, #m)
	end
end)
local sizes = { 1000, 10000, 100000, 1000000 }
for _, n in sizes do
	print("BIG:<" .. string.rep("x", n - 7) .. ">:")
end
-- ANSI escape codes: kept or stripped?
print("ANSI:\27[31mred\27[0m")

task.wait(1)
r.bigPrintSizesSent = sizes
r.bigPrintSizesSeenByMessageOut = seen
print(SENTINEL .. HttpService:JSONEncode(r))
