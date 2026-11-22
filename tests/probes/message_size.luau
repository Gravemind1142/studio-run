-- How much of a long print() LogService.MessageOut delivers.
-- Reports one JSON line prefixed with @@probe@@.
local HttpService = game:GetService("HttpService")
local LogService = game:GetService("LogService")

local sizes = { 1000, 10000, 100000, 200000, 1000000 }
local seen = {}
local conn = LogService.MessageOut:Connect(function(message)
	local n = string.match(message, "^SIZE(%d+):")
	if n then
		seen[n] = #message
	end
end)
for _, n in sizes do
	local prefix = "SIZE" .. n .. ":"
	print(prefix .. string.rep("x", n - #prefix))
end
task.wait(1)
conn:Disconnect()

local result = {}
for _, n in sizes do
	table.insert(result, { sent = n, seen = seen[tostring(n)] or 0 })
end
print("@@probe@@" .. HttpService:JSONEncode(result))
