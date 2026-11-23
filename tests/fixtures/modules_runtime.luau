-- modules.luau for --cloud, where the test place is an empty baseplate: it
-- creates ReplicatedStorage.Greeter (the module in modules.rbxlx, with the
-- same source and line numbers) before requiring it. Nothing a task changes
-- is saved to the place.
local Greeter = Instance.new("ModuleScript")
Greeter.Name = "Greeter"
Greeter.Source = [[print("Greeter top-level print")
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
Greeter.Parent = game:GetService("ReplicatedStorage")

local M = require(game.ReplicatedStorage.Greeter)
print(M.greet("tests"))
M.boom()
